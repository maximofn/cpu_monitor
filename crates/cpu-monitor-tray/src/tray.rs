use std::path::PathBuf;

use anyhow::{Context, Result};
use cpu_monitor_core::{Cpu, Snapshot};
use ksni::menu::StandardItem;
use ksni::{MenuItem, ToolTip, Tray};

use crate::icon::IconRenderer;

const REPO_URL: &str = "https://github.com/maximofn/cpu_monitor";
const COFFEE_URL: &str = "https://www.buymeacoffee.com/maximofn";
const ICON_BASENAME: &str = "cpu-monitor-tray";

#[derive(Debug, Clone)]
pub enum State {
    Connecting,
    Connected(Snapshot),
    Disconnected(String),
}

pub struct CpuTray {
    renderer: IconRenderer,
    backend_url: String,
    state: State,
    icon_dir: PathBuf,
    /// Counter that increments on every redraw so the panel sees a new
    /// `IconName` and reloads the file from disk (matches what AppIndicator's
    /// `set_icon_full` does internally — GNOME-shell otherwise caches by name).
    generation: u64,
    current_icon_name: String,
}

impl CpuTray {
    pub fn new(renderer: IconRenderer, backend_url: String, icon_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&icon_dir)
            .with_context(|| format!("creating icon dir {}", icon_dir.display()))?;
        if let Ok(entries) = std::fs::read_dir(&icon_dir) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(ICON_BASENAME)
                {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        let mut tray = Self {
            renderer,
            backend_url,
            state: State::Connecting,
            icon_dir,
            generation: 0,
            current_icon_name: String::new(),
        };
        tray.refresh_icon_file();
        Ok(tray)
    }

    pub fn set_state(&mut self, state: State) {
        self.state = state;
        self.refresh_icon_file();
    }

    fn refresh_icon_file(&mut self) {
        let png = match self
            .renderer
            .render_png(self.current_cpu(), self.connected())
        {
            Ok(bytes) => bytes,
            Err(err) => {
                tracing::warn!(error = %err, "failed to render icon PNG");
                return;
            }
        };
        self.generation = self.generation.wrapping_add(1);
        let new_name = format!("{ICON_BASENAME}-{}", self.generation);
        let new_path = self.icon_dir.join(format!("{new_name}.png"));
        if let Err(err) = std::fs::write(&new_path, &png) {
            tracing::warn!(error = %err, path = %new_path.display(), "failed to write icon PNG");
            return;
        }

        if !self.current_icon_name.is_empty() {
            let old = self
                .icon_dir
                .join(format!("{}.png", self.current_icon_name));
            let _ = std::fs::remove_file(old);
        }
        self.current_icon_name = new_name;
    }

    fn current_cpu(&self) -> Option<&Cpu> {
        match &self.state {
            State::Connected(snap) => Some(&snap.cpu),
            _ => None,
        }
    }

    fn connected(&self) -> bool {
        matches!(self.state, State::Connected(_))
    }
}

impl Tray for CpuTray {
    fn id(&self) -> String {
        "cpu-monitor".to_string()
    }

    fn title(&self) -> String {
        "CPU Monitor".to_string()
    }

    fn icon_name(&self) -> String {
        self.current_icon_name.clone()
    }

    fn icon_theme_path(&self) -> String {
        self.icon_dir.to_string_lossy().into_owned()
    }

    fn tool_tip(&self) -> ToolTip {
        let title = "CPU Monitor".to_string();
        let description = match &self.state {
            State::Connecting => format!("Connecting to {}", self.backend_url),
            State::Connected(snap) => describe_cpu(&snap.cpu, snap.kernel.as_deref()),
            State::Disconnected(err) => format!("Backend offline: {err}"),
        };
        ToolTip {
            icon_name: String::new(),
            icon_pixmap: Vec::new(),
            title,
            description,
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = Vec::new();

        match &self.state {
            State::Connecting => {
                items.push(disabled_item(format!(
                    "Connecting to {}\u{2026}",
                    self.backend_url
                )));
                items.push(MenuItem::Separator);
            }
            State::Disconnected(err) => {
                items.push(disabled_item(format!("Backend offline: {err}")));
                items.push(disabled_item(format!("Backend: {}", self.backend_url)));
                items.push(MenuItem::Separator);
            }
            State::Connected(snap) => {
                let cpu = &snap.cpu;
                items.push(disabled_item(format!(
                    "CPU Temp: {}",
                    cpu.temperature_c
                        .map(|t| format!("{:.1}\u{00b0}C", t))
                        .unwrap_or_else(|| "N/A".to_string())
                )));
                items.push(disabled_item(format!(
                    "CPU Usage: {:.1}%",
                    cpu.usage_percent
                )));
                if let Some(freq) = cpu.frequency_mhz {
                    items.push(disabled_item(format!("Frequency: {:.0} MHz", freq)));
                }
                if let Some(load) = cpu.load_average {
                    items.push(disabled_item(format!(
                        "Load avg: {:.2} / {:.2} / {:.2}",
                        load.one, load.five, load.fifteen
                    )));
                }
                items.push(disabled_item(format!(
                    "Cores: {}{}",
                    cpu.logical_cores,
                    cpu.physical_cores
                        .map(|p| format!(" logical / {p} physical"))
                        .unwrap_or_default()
                )));
                if let Some(model) = &cpu.model {
                    items.push(disabled_item(model.clone()));
                }
                items.push(MenuItem::Separator);
                if !cpu.temperatures.is_empty() {
                    items.push(disabled_item("Sensors:".to_string()));
                    for sensor in &cpu.temperatures {
                        items.push(disabled_item(format!(
                            "  {}/{}: {:.1}\u{00b0}C",
                            sensor.chip, sensor.label, sensor.temp_c
                        )));
                    }
                    items.push(MenuItem::Separator);
                }
                if !cpu.processes.is_empty() {
                    items.push(disabled_item(format!("Top processes ({}):", cpu.processes.len())));
                    for proc in &cpu.processes {
                        items.push(disabled_item(format!(
                            "  {:>6} {:>5.1}%  {} ({})",
                            proc.pid,
                            proc.cpu_percent,
                            proc.name,
                            format_bytes(proc.memory_bytes)
                        )));
                    }
                    items.push(MenuItem::Separator);
                }
                items.push(disabled_item(format!("Backend: {}", self.backend_url)));
                items.push(disabled_item(format!(
                    "Updated: {}",
                    short_time(&snap.timestamp)
                )));
                items.push(MenuItem::Separator);
            }
        }

        items.push(MenuItem::Standard(StandardItem {
            label: "Repository".into(),
            activate: Box::new(|_| open_url(REPO_URL)),
            ..Default::default()
        }));
        items.push(MenuItem::Standard(StandardItem {
            label: "Buy me a coffee".into(),
            activate: Box::new(|_| open_url(COFFEE_URL)),
            ..Default::default()
        }));
        items.push(MenuItem::Separator);
        items.push(MenuItem::Standard(StandardItem {
            label: "Quit".into(),
            activate: Box::new(|_| std::process::exit(0)),
            ..Default::default()
        }));

        items
    }
}

fn describe_cpu(cpu: &Cpu, kernel: Option<&str>) -> String {
    let mut lines = Vec::new();
    if let Some(model) = &cpu.model {
        lines.push(model.clone());
    }
    if let Some(temp) = cpu.temperature_c {
        lines.push(format!("Temp: {:.1}\u{00b0}C", temp));
    }
    lines.push(format!("Usage: {:.1}%", cpu.usage_percent));
    if let Some(load) = cpu.load_average {
        lines.push(format!(
            "Load: {:.2} / {:.2} / {:.2}",
            load.one, load.five, load.fifteen
        ));
    }
    if let Some(k) = kernel {
        lines.push(format!("Kernel: {k}"));
    }
    lines.join("\n")
}

fn disabled_item(label: String) -> MenuItem<CpuTray> {
    MenuItem::Standard(StandardItem {
        label,
        enabled: false,
        ..Default::default()
    })
}

fn open_url(url: &str) {
    if let Err(err) = open::that(url) {
        tracing::warn!(%url, error = %err, "could not open url");
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.0} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} B", bytes)
    }
}

fn short_time(rfc3339: &str) -> &str {
    rfc3339
        .split('T')
        .nth(1)
        .and_then(|s| s.split('.').next())
        .unwrap_or(rfc3339)
}
