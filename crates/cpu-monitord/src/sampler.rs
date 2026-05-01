use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use cpu_monitor_core::Snapshot;
use tokio::sync::watch;
use tokio::time::{interval, MissedTickBehavior};

use crate::proc_source::CpuSource;

pub fn build_snapshot(host: &str, kernel: Option<String>, source: &dyn CpuSource) -> Snapshot {
    let cpu = source.sample().unwrap_or_else(|err| {
        tracing::warn!(error = %err, "CPU sample failed; emitting empty snapshot");
        cpu_monitor_core::Cpu {
            model: source.cpu_model(),
            vendor: None,
            logical_cores: 0,
            physical_cores: None,
            usage_percent: 0.0,
            per_core_usage: Vec::new(),
            temperature_c: None,
            primary_sensor: None,
            temperatures: Vec::new(),
            frequency_mhz: None,
            load_average: None,
            uptime_s: None,
            processes: Vec::new(),
        }
    });
    Snapshot {
        timestamp: Utc::now().to_rfc3339(),
        host: host.to_string(),
        kernel,
        cpu,
    }
}

pub fn spawn(
    source: Arc<dyn CpuSource>,
    host: String,
    kernel: Option<String>,
    interval_ms: u64,
    tx: watch::Sender<Snapshot>,
) {
    tokio::spawn(async move {
        let period = Duration::from_millis(interval_ms.max(50));
        let mut ticker = interval(period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;
            let snapshot = build_snapshot(&host, kernel.clone(), source.as_ref());
            if tx.send(snapshot).is_err() {
                tracing::info!("snapshot channel closed; sampler exiting");
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc_source::MockSource;

    #[test]
    fn build_snapshot_uses_source_metadata() {
        let source = MockSource::new();
        let snap = build_snapshot("host-x", Some("6.5.0".into()), &source);
        assert_eq!(snap.host, "host-x");
        assert_eq!(snap.kernel.as_deref(), Some("6.5.0"));
        assert_eq!(snap.cpu.logical_cores, 8);
        assert!(!snap.timestamp.is_empty());
    }
}
