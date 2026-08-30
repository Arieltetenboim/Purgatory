//! Harness-level metrics aggregation.

use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default)]
pub struct BotMetrics {
    pub connected: u32,
    pub connecting: u32,
    pub disconnected: u32,
    pub failed: u32,
    pub total_commands_sent: u64,
    pub total_snapshots_received: u64,
}

#[derive(Clone, Debug)]
pub struct HarnessMetrics {
    pub start_time: Instant,
    pub sample_time: Instant,
    pub bots: BotMetrics,
    pub tick_times_ms: Vec<f64>,
    pub snapshot_starvation_samples: u32,
    pub overflow_events: u64,
    pub encode_failures: u64,
    pub admission_refusals: u64,
    pub unexpected_disconnects: u64,
    pub already_connected_rejects: u64,
    pub duplicate_welcome: u64,
    pub reconnect_entity_unchanged: u64,
    pub portal_activates_sent: u64,
}

impl HarnessMetrics {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            start_time: now,
            sample_time: now,
            bots: BotMetrics::default(),
            tick_times_ms: Vec::with_capacity(128),
            snapshot_starvation_samples: 0,
            overflow_events: 0,
            encode_failures: 0,
            admission_refusals: 0,
            unexpected_disconnects: 0,
            already_connected_rejects: 0,
            duplicate_welcome: 0,
            reconnect_entity_unchanged: 0,
            portal_activates_sent: 0,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.sample_time.elapsed()
    }

    pub fn record_tick_time(&mut self, ms: f64) {
        self.tick_times_ms.push(ms);
    }

    pub fn bot_scheduler_p50_ms(&self) -> f64 {
        percentile(&self.tick_times_ms, 50.0)
    }

    pub fn bot_scheduler_p95_ms(&self) -> f64 {
        percentile(&self.tick_times_ms, 95.0)
    }

    pub fn bot_scheduler_p99_ms(&self) -> f64 {
        percentile(&self.tick_times_ms, 99.0)
    }

    pub fn bot_scheduler_max_ms(&self) -> f64 {
        self.tick_times_ms.iter().copied().fold(0.0_f64, f64::max)
    }

    /// Deprecated aliases kept for internal call sites during transition.
    pub fn tick_p50_ms(&self) -> f64 {
        self.bot_scheduler_p50_ms()
    }
    pub fn tick_p95_ms(&self) -> f64 {
        self.bot_scheduler_p95_ms()
    }
    pub fn tick_p99_ms(&self) -> f64 {
        self.bot_scheduler_p99_ms()
    }
    pub fn tick_max_ms(&self) -> f64 {
        self.bot_scheduler_max_ms()
    }

    pub fn clear_tick_times(&mut self) {
        self.tick_times_ms.clear();
    }
}

impl Default for HarnessMetrics {
    fn default() -> Self {
        Self::new()
    }
}

pub fn percentile(data: &[f64], p: f64) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f64> = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_empty() {
        assert_eq!(percentile(&[], 50.0), 0.0);
    }

    #[test]
    fn percentile_single() {
        assert_eq!(percentile(&[5.0], 50.0), 5.0);
        assert_eq!(percentile(&[5.0], 99.0), 5.0);
    }

    #[test]
    fn percentile_multiple() {
        let data: Vec<f64> = (1..=100).map(|x| x as f64).collect();
        let p50 = percentile(&data, 50.0);
        assert!((49.0..=51.0).contains(&p50), "p50={p50}");
        let p95 = percentile(&data, 95.0);
        assert!((94.0..=96.0).contains(&p95), "p95={p95}");
        let p99 = percentile(&data, 99.0);
        assert!((98.0..=100.0).contains(&p99), "p99={p99}");
    }

    #[test]
    fn metrics_record_and_query() {
        let mut m = HarnessMetrics::new();
        for i in 1..=100 {
            m.record_tick_time(i as f64);
        }
        assert!((49.0..=51.0).contains(&m.tick_p50_ms()));
        assert!(m.tick_p99_ms() >= 98.0);
        assert_eq!(m.tick_max_ms(), 100.0);
    }
}
