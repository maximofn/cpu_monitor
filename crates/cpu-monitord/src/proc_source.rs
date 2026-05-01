use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use cpu_monitor_core::{Cpu, LoadAverage, Process, TempSensor};

pub trait CpuSource: Send + Sync {
    fn sample(&self) -> Result<Cpu>;
    fn cpu_model(&self) -> Option<String> {
        None
    }
}

#[derive(Clone, Copy, Default, Debug)]
struct CpuTimes {
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    steal: u64,
}

impl CpuTimes {
    fn busy(&self) -> u64 {
        self.user + self.nice + self.system + self.irq + self.softirq + self.steal
    }
    fn total(&self) -> u64 {
        self.busy() + self.idle + self.iowait
    }
    fn delta_percent(prev: &Self, now: &Self) -> f32 {
        let total = now.total().saturating_sub(prev.total());
        if total == 0 {
            return 0.0;
        }
        let busy = now.busy().saturating_sub(prev.busy());
        (busy as f32 / total as f32) * 100.0
    }
}

#[derive(Default)]
struct State {
    aggregate: CpuTimes,
    per_core: Vec<CpuTimes>,
    per_pid: HashMap<u32, u64>, // pid -> last (utime + stime) in clock ticks
    have_baseline: bool,
}

pub struct ProcfsSource {
    state: Mutex<State>,
    cpu_model: Option<String>,
    cpu_vendor: Option<String>,
    physical_cores: Option<u32>,
    top_processes: usize,
    /// hwmon directories that expose CPU temperatures (k10temp, coretemp, …).
    hwmon_paths: Vec<PathBuf>,
    primary_chip: Option<String>,
}

impl ProcfsSource {
    pub fn init(top_processes: usize) -> Result<Self> {
        let cpuinfo = fs::read_to_string("/proc/cpuinfo").context("reading /proc/cpuinfo")?;
        let cpu_model = parse_cpuinfo_field(&cpuinfo, "model name");
        let cpu_vendor = parse_cpuinfo_field(&cpuinfo, "vendor_id");
        let physical_cores =
            parse_cpuinfo_field(&cpuinfo, "cpu cores").and_then(|v| v.parse().ok());

        let (hwmon_paths, primary_chip) = discover_hwmon();

        let mut state = State::default();
        if let Ok((agg, per_core)) = read_proc_stat() {
            state.aggregate = agg;
            state.per_core = per_core;
        }

        Ok(Self {
            state: Mutex::new(state),
            cpu_model,
            cpu_vendor,
            physical_cores,
            top_processes,
            hwmon_paths,
            primary_chip,
        })
    }
}

impl CpuSource for ProcfsSource {
    fn cpu_model(&self) -> Option<String> {
        self.cpu_model.clone()
    }

    fn sample(&self) -> Result<Cpu> {
        let (agg_now, per_core_now) = read_proc_stat()?;
        let mut st = self.state.lock().expect("state mutex poisoned");

        let usage_percent = CpuTimes::delta_percent(&st.aggregate, &agg_now);
        let mut per_core_usage: Vec<f32> = Vec::with_capacity(per_core_now.len());
        for (i, now) in per_core_now.iter().enumerate() {
            let prev = st.per_core.get(i).copied().unwrap_or_default();
            per_core_usage.push(CpuTimes::delta_percent(&prev, now));
        }

        let prev_total = st.aggregate.total();
        let now_total = agg_now.total();
        let total_delta = now_total.saturating_sub(prev_total) as f32;

        st.aggregate = agg_now;
        st.per_core = per_core_now;

        let processes = if self.top_processes > 0 {
            sample_top_processes(self.top_processes, &mut st, total_delta).unwrap_or_default()
        } else {
            Vec::new()
        };
        st.have_baseline = true;
        let logical_cores = st.per_core.len() as u32;
        drop(st);

        let load_average = read_loadavg().ok();
        let uptime_s = read_uptime().ok();
        let frequency_mhz = read_cur_freq();
        let temperatures = read_temperatures(&self.hwmon_paths);
        let (temperature_c, primary_sensor) =
            pick_primary_temperature(&temperatures, self.primary_chip.as_deref());

        Ok(Cpu {
            model: self.cpu_model.clone(),
            vendor: self.cpu_vendor.clone(),
            logical_cores,
            physical_cores: self.physical_cores,
            usage_percent,
            per_core_usage,
            temperature_c,
            primary_sensor,
            temperatures,
            frequency_mhz,
            load_average,
            uptime_s,
            processes,
        })
    }
}

fn read_proc_stat() -> Result<(CpuTimes, Vec<CpuTimes>)> {
    let content = fs::read_to_string("/proc/stat").context("reading /proc/stat")?;
    let mut aggregate = CpuTimes::default();
    let mut per_core: Vec<CpuTimes> = Vec::new();
    for line in content.lines() {
        if !line.starts_with("cpu") {
            break;
        }
        let mut parts = line.split_whitespace();
        let label = parts.next().unwrap_or("");
        let nums: Vec<u64> = parts.filter_map(|s| s.parse().ok()).collect();
        if nums.len() < 4 {
            continue;
        }
        let times = CpuTimes {
            user: nums[0],
            nice: nums[1],
            system: nums[2],
            idle: nums[3],
            iowait: nums.get(4).copied().unwrap_or(0),
            irq: nums.get(5).copied().unwrap_or(0),
            softirq: nums.get(6).copied().unwrap_or(0),
            steal: nums.get(7).copied().unwrap_or(0),
        };
        if label == "cpu" {
            aggregate = times;
        } else {
            per_core.push(times);
        }
    }
    Ok((aggregate, per_core))
}

fn read_loadavg() -> Result<LoadAverage> {
    let content = fs::read_to_string("/proc/loadavg").context("reading /proc/loadavg")?;
    let mut parts = content.split_whitespace();
    let one: f32 = parts.next().unwrap_or("0").parse().unwrap_or(0.0);
    let five: f32 = parts.next().unwrap_or("0").parse().unwrap_or(0.0);
    let fifteen: f32 = parts.next().unwrap_or("0").parse().unwrap_or(0.0);
    Ok(LoadAverage { one, five, fifteen })
}

fn read_uptime() -> Result<u64> {
    let content = fs::read_to_string("/proc/uptime").context("reading /proc/uptime")?;
    let secs: f64 = content
        .split_whitespace()
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0.0);
    Ok(secs as u64)
}

fn read_cur_freq() -> Option<f32> {
    let content = fs::read_to_string("/proc/cpuinfo").ok()?;
    let mut total = 0.0f32;
    let mut count = 0u32;
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("cpu MHz") {
            let value = value.trim_start_matches([':', ' ', '\t']);
            if let Ok(mhz) = value.trim().parse::<f32>() {
                total += mhz;
                count += 1;
            }
        }
    }
    if count == 0 {
        None
    } else {
        Some(total / count as f32)
    }
}

fn parse_cpuinfo_field(content: &str, field: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix(field) {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix(':') {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

const CPU_HWMON_NAMES: &[&str] = &["k10temp", "coretemp", "zenpower", "cpu_thermal"];

fn discover_hwmon() -> (Vec<PathBuf>, Option<String>) {
    let root = Path::new("/sys/class/hwmon");
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return (Vec::new(), None),
    };
    let mut paths = Vec::new();
    let mut primary_chip: Option<String> = None;
    let mut primary_priority = u32::MAX;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match fs::read_to_string(path.join("name")) {
            Ok(s) => s.trim().to_string(),
            Err(_) => continue,
        };
        if let Some(prio) = CPU_HWMON_NAMES.iter().position(|&n| n == name) {
            paths.push(path);
            if (prio as u32) < primary_priority {
                primary_priority = prio as u32;
                primary_chip = Some(name);
            }
        }
    }
    (paths, primary_chip)
}

fn read_temperatures(paths: &[PathBuf]) -> Vec<TempSensor> {
    let mut out = Vec::new();
    for chip_path in paths {
        let chip = fs::read_to_string(chip_path.join("name"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let entries = match fs::read_dir(chip_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let mut indices: Vec<u32> = Vec::new();
        for entry in entries.flatten() {
            let fname = entry.file_name();
            let fname = fname.to_string_lossy();
            if let Some(stripped) = fname.strip_prefix("temp") {
                if let Some(num) = stripped.strip_suffix("_input") {
                    if let Ok(n) = num.parse::<u32>() {
                        indices.push(n);
                    }
                }
            }
        }
        indices.sort_unstable();
        for idx in indices {
            let input_path = chip_path.join(format!("temp{idx}_input"));
            let label_path = chip_path.join(format!("temp{idx}_label"));
            let raw = match fs::read_to_string(&input_path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let millideg: f32 = match raw.trim().parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            let label = fs::read_to_string(&label_path)
                .ok()
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| format!("temp{idx}"));
            out.push(TempSensor {
                chip: chip.clone(),
                label,
                temp_c: millideg / 1000.0,
            });
        }
    }
    out
}

fn pick_primary_temperature(
    temps: &[TempSensor],
    primary_chip: Option<&str>,
) -> (Option<f32>, Option<String>) {
    let preferred_labels = ["Tctl", "Package id 0"];
    for label in preferred_labels {
        if let Some(s) = temps.iter().find(|t| t.label == label) {
            return (Some(s.temp_c), Some(format!("{}/{}", s.chip, s.label)));
        }
    }
    if let Some(chip) = primary_chip {
        if let Some(s) = temps.iter().find(|t| t.chip == chip) {
            return (Some(s.temp_c), Some(format!("{}/{}", s.chip, s.label)));
        }
    }
    if let Some(s) = temps.first() {
        return (Some(s.temp_c), Some(format!("{}/{}", s.chip, s.label)));
    }
    (None, None)
}

fn sample_top_processes(top_n: usize, state: &mut State, total_delta: f32) -> Result<Vec<Process>> {
    let entries = match fs::read_dir("/proc") {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };

    let n_cores = state.per_core.len().max(1) as f32;
    let mut new_pids: HashMap<u32, u64> = HashMap::new();
    let mut candidates: Vec<Process> = Vec::new();

    for entry in entries.flatten() {
        let fname = entry.file_name();
        let pid: u32 = match fname.to_string_lossy().parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // /proc/<pid>/stat: PID (comm) state ppid ... — comm may contain spaces
        // and parens, so split on the LAST ')'.
        let close_paren = match stat.rfind(')') {
            Some(i) => i,
            None => continue,
        };
        let comm_start = match stat.find('(') {
            Some(i) => i + 1,
            None => continue,
        };
        if close_paren <= comm_start {
            continue;
        }
        let comm = stat[comm_start..close_paren].to_string();
        let rest = &stat[close_paren + 1..];
        let fields: Vec<&str> = rest.split_whitespace().collect();
        // After the closing paren, fields are 0-indexed. utime is field 11, stime
        // 12, rss 21 (in pages). See proc(5).
        if fields.len() < 22 {
            continue;
        }
        let utime: u64 = fields[11].parse().unwrap_or(0);
        let stime: u64 = fields[12].parse().unwrap_or(0);
        let rss_pages: u64 = fields[21].parse().unwrap_or(0);
        let proc_total = utime + stime;

        let prev = state.per_pid.get(&pid).copied();
        new_pids.insert(pid, proc_total);

        // Only emit a CPU% once we have a previous baseline to subtract from;
        // otherwise the delta would be the entire lifetime of the process
        // measured against one sampling interval, producing absurd spikes.
        let cpu_percent = match prev {
            Some(prev) if state.have_baseline && total_delta > 0.0 => {
                let proc_delta = proc_total.saturating_sub(prev) as f32;
                (proc_delta / total_delta) * 100.0 * n_cores
            }
            _ => 0.0,
        };

        let memory_bytes = rss_pages * page_size_bytes();

        candidates.push(Process {
            pid,
            name: comm,
            cpu_percent,
            memory_bytes,
        });
    }

    state.per_pid = new_pids;

    candidates.sort_by(|a, b| {
        b.cpu_percent
            .partial_cmp(&a.cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.memory_bytes.cmp(&a.memory_bytes))
    });
    candidates.truncate(top_n);
    Ok(candidates)
}

fn page_size_bytes() -> u64 {
    // sysconf(_SC_PAGESIZE) — 4096 on x86_64. Hardcoded to avoid pulling libc.
    4096
}

pub struct MockSource {
    cpu: Cpu,
}

impl MockSource {
    pub fn new() -> Self {
        Self {
            cpu: Cpu {
                model: Some("Mock CPU".to_string()),
                vendor: Some("MockVendor".to_string()),
                logical_cores: 8,
                physical_cores: Some(4),
                usage_percent: 25.0,
                per_core_usage: vec![20.0, 30.0, 25.0, 28.0, 22.0, 19.0, 35.0, 21.0],
                temperature_c: Some(48.0),
                primary_sensor: Some("k10temp/Tctl".to_string()),
                temperatures: vec![
                    TempSensor {
                        chip: "k10temp".to_string(),
                        label: "Tctl".to_string(),
                        temp_c: 48.0,
                    },
                    TempSensor {
                        chip: "k10temp".to_string(),
                        label: "Tccd1".to_string(),
                        temp_c: 46.5,
                    },
                ],
                frequency_mhz: Some(3400.0),
                load_average: Some(LoadAverage {
                    one: 0.4,
                    five: 0.5,
                    fifteen: 0.6,
                }),
                uptime_s: Some(12345),
                processes: vec![
                    Process {
                        pid: 1234,
                        name: "firefox".to_string(),
                        cpu_percent: 12.0,
                        memory_bytes: 1_500_000_000,
                    },
                    Process {
                        pid: 4321,
                        name: "code".to_string(),
                        cpu_percent: 4.5,
                        memory_bytes: 800_000_000,
                    },
                ],
            },
        }
    }
}

impl CpuSource for MockSource {
    fn cpu_model(&self) -> Option<String> {
        self.cpu.model.clone()
    }

    fn sample(&self) -> Result<Cpu> {
        Ok(self.cpu.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_times_delta_zero_when_unchanged() {
        let t = CpuTimes {
            user: 100,
            system: 50,
            idle: 200,
            ..Default::default()
        };
        assert_eq!(CpuTimes::delta_percent(&t, &t), 0.0);
    }

    #[test]
    fn cpu_times_delta_full_busy() {
        let prev = CpuTimes::default();
        let now = CpuTimes {
            user: 100,
            system: 0,
            idle: 0,
            ..Default::default()
        };
        assert!((CpuTimes::delta_percent(&prev, &now) - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cpu_times_delta_half_busy() {
        let prev = CpuTimes::default();
        let now = CpuTimes {
            user: 50,
            idle: 50,
            ..Default::default()
        };
        let pct = CpuTimes::delta_percent(&prev, &now);
        assert!((pct - 50.0).abs() < 0.001);
    }

    #[test]
    fn primary_picks_tctl_first() {
        let temps = vec![
            TempSensor {
                chip: "k10temp".into(),
                label: "Tccd1".into(),
                temp_c: 50.0,
            },
            TempSensor {
                chip: "k10temp".into(),
                label: "Tctl".into(),
                temp_c: 55.0,
            },
        ];
        let (val, name) = pick_primary_temperature(&temps, Some("k10temp"));
        assert_eq!(val, Some(55.0));
        assert_eq!(name.as_deref(), Some("k10temp/Tctl"));
    }

    #[test]
    fn primary_falls_back_to_chip_first_sensor() {
        let temps = vec![TempSensor {
            chip: "coretemp".into(),
            label: "Core 0".into(),
            temp_c: 42.0,
        }];
        let (val, name) = pick_primary_temperature(&temps, Some("coretemp"));
        assert_eq!(val, Some(42.0));
        assert_eq!(name.as_deref(), Some("coretemp/Core 0"));
    }

    #[test]
    fn parse_cpuinfo_grabs_field() {
        let sample = "processor\t: 0\nvendor_id\t: AuthenticAMD\nmodel name\t: AMD Ryzen 9\n";
        assert_eq!(
            parse_cpuinfo_field(sample, "vendor_id").as_deref(),
            Some("AuthenticAMD")
        );
        assert_eq!(
            parse_cpuinfo_field(sample, "model name").as_deref(),
            Some("AMD Ryzen 9")
        );
    }

    #[test]
    fn mock_source_returns_seeded_data() {
        let m = MockSource::new();
        let cpu = m.sample().unwrap();
        assert_eq!(cpu.logical_cores, 8);
        assert_eq!(cpu.processes.len(), 2);
    }
}
