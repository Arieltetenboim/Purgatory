//! 1 Hz console dashboard.

use crate::metrics::HarnessMetrics;
use purgatory_common::LoadMetricsV1;

pub struct Dashboard;

impl Dashboard {
    pub fn print(
        metrics: &HarnessMetrics,
        target: u32,
        connected: u32,
        server: Option<&LoadMetricsV1>,
        elapsed_secs: f64,
        bytes_out_per_sec: Option<f64>,
        harness_memory_mb: Option<f64>,
    ) {
        let tick_p95 = server.map(|s| s.tick_work_p95_ms).unwrap_or(0.0);
        let tick_max = server.map(|s| s.tick_work_max_ms).unwrap_or(0.0);
        let overruns = server.map(|s| s.tick_overrun_count).unwrap_or(0);
        let out_mbs = bytes_out_per_sec
            .map(|b| b / (1024.0 * 1024.0))
            .unwrap_or(0.0);
        let mem_mb = server
            .map(|s| s.memory_working_set_bytes as f64 / (1024.0 * 1024.0))
            .or(harness_memory_mb)
            .unwrap_or(0.0);
        let errors = metrics.unexpected_disconnects + metrics.overflow_events;
        println!(
            "[{elapsed_secs:7.1}s] Bots {connected:>4} / {target:<4}  Server tick p95 {tick_p95:6.2} ms  max {tick_max:6.2} ms  Overruns {overruns:<5}  Out {out_mbs:5.2} MB/s  Server mem {mem_mb:6.1} MB  Errors {errors}"
        );
    }
}
