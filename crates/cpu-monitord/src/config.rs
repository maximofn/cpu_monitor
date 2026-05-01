use std::net::IpAddr;

use clap::Parser;
use cpu_monitor_core::{DEFAULT_BIND, DEFAULT_PORT};

#[derive(Debug, Clone, Parser)]
#[command(name = "cpu-monitord", about = "CPU monitor backend daemon", version)]
pub struct Config {
    #[arg(long, env = "CPU_MONITORD_BIND", default_value = DEFAULT_BIND)]
    pub bind: IpAddr,

    #[arg(long, env = "CPU_MONITORD_PORT", default_value_t = DEFAULT_PORT)]
    pub port: u16,

    #[arg(long, env = "CPU_MONITORD_SAMPLE_INTERVAL_MS", default_value_t = 1000)]
    pub sample_interval_ms: u64,

    #[arg(long, env = "RUST_LOG", default_value = "info")]
    pub log_level: String,

    #[arg(long, env = "CPU_MONITORD_MOCK", default_value_t = false)]
    pub mock: bool,

    /// Number of top CPU-consuming processes to include in each snapshot. Set to 0 to disable.
    #[arg(long, env = "CPU_MONITORD_TOP_PROCESSES", default_value_t = 5)]
    pub top_processes: u32,
}
