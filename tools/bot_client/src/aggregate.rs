//! Per-metric run aggregation for server load telemetry (Phase 5.7).
//!
//! Each field is aggregated independently from valid samples. A missed UDP poll
//! or a missing optional field must not erase other metrics.
//!
//! Memory: O(1) — no raw sample retention.
//!
//! # Tick statistic semantics
//!
//! Each UDP sample's `tick_work_*` fields come from the server's sliding ring of
//! up to [`TICK_WORK_RING_CAP`] recent simulation ticks (not a fixed count while
//! the ring is filling). Run-wide mean is a **tick-count-weighted** average of
//! those window means (weight = Δ`tick_count` between samples).
//!
//! Run-wide p95/p99/max are **peaks of window percentiles / window max**, not
//! full-run distribution percentiles. See [`TICK_PERCENTILE_SEMANTICS`].

use serde::Serialize;

use purgatory_common::LoadMetricsV1;

use crate::log::SummaryExtras;

/// Server `TickSampleRing` capacity (must match `metrics_export.rs`).
pub const TICK_WORK_RING_CAP: u64 = 120;

/// Documented meaning of summary `server_tick_work_p95_ms` / `p99_ms`.
pub const TICK_PERCENTILE_SEMANTICS: &str = "peak_of_window_percentiles";

/// Telemetry health for the run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricsHealth {
    /// Zero successful server metric samples.
    #[default]
    None,
    /// Some successes, but miss ratio ≥ [`PARTIAL_MISS_RATIO`].
    Partial,
    /// Successes with miss ratio below the partial threshold.
    Healthy,
}

impl MetricsHealth {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Partial => "partial",
            Self::Healthy => "healthy",
        }
    }
}

/// Miss ratio at/above which health is [`MetricsHealth::Partial`].
pub const PARTIAL_MISS_RATIO: f64 = 0.05;

/// Optional per-sample observation (CSV / intermittent fields).
#[derive(Clone, Debug, Default)]
pub struct PartialServerObservation {
    pub tick_work_mean_ms: Option<f64>,
    pub tick_work_p95_ms: Option<f64>,
    pub tick_work_p99_ms: Option<f64>,
    pub tick_work_max_ms: Option<f64>,
    pub tick_overruns_total: Option<u64>,
    /// Cumulative server tick count when known (enables weighted mean).
    pub tick_count: Option<u64>,
    pub bytes_in: Option<u64>,
    pub bytes_out: Option<u64>,
    pub memory_mb: Option<f64>,
    pub memory_peak_mb: Option<f64>,
    pub input_queue_max: Option<u64>,
    pub session_queue_max: Option<u64>,
    pub scheduler_lateness_p95_ms: Option<f64>,
    pub scheduler_lateness_max_ms: Option<f64>,
}

/// Bounded run-wide aggregator. Does not store sample history.
#[derive(Clone, Debug, Default)]
pub struct ServerRunAggregator {
    samples_ok: u64,
    samples_missed: u64,
    /// Σ (window_mean × weight).
    tick_mean_weighted_sum: f64,
    /// Σ weight (Δ tick_count).
    tick_mean_weight: f64,
    prev_tick_count: Option<u64>,
    tick_p95_peak: Option<f64>,
    tick_p99_peak: Option<f64>,
    tick_max_peak: Option<f64>,
    lateness_p95_peak: Option<f64>,
    lateness_max_peak: Option<f64>,
    last_overruns: Option<u64>,
    last_bytes_in: Option<u64>,
    last_bytes_out: Option<u64>,
    memory_start_mb: Option<f64>,
    memory_peak_mb: Option<f64>,
    memory_end_mb: Option<f64>,
    input_queue_max: Option<u64>,
    session_queue_max: Option<u64>,
    scheduler_queued_start: Option<u64>,
    scheduler_queued_end: Option<u64>,
    scheduler_queued_max: Option<u64>,
    actions_active_max: Option<u64>,
    spawn_queue_depth_max: Option<u64>,
    aoi_enters_total: Option<u64>,
    aoi_leaves_total: Option<u64>,
    aoi_updates_total: Option<u64>,
    scheduler_critical_ceiling_hits: Option<u64>,
    scheduler_deferred_exhausted: Option<u64>,
}

impl ServerRunAggregator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn note_miss(&mut self) {
        self.samples_missed = self.samples_missed.saturating_add(1);
    }

    /// Observe a full successful `LoadMetricsV1` poll.
    pub fn observe(&mut self, s: &LoadMetricsV1) {
        self.samples_ok = self.samples_ok.saturating_add(1);
        self.observe_tick_mean(Some(s.tick_work_mean_ms), Some(s.tick_count));
        self.observe_tick_peaks(
            Some(s.tick_work_p95_ms),
            Some(s.tick_work_p99_ms),
            Some(s.tick_work_max_ms),
        );
        self.observe_lateness(
            Some(s.scheduler_lateness_p95_ms),
            Some(s.scheduler_lateness_max_ms),
        );
        self.last_overruns = Some(s.tick_overrun_count);
        self.last_bytes_in = Some(s.bytes_in);
        self.last_bytes_out = Some(s.bytes_out);
        self.observe_memory(
            if s.memory_working_set_bytes > 0 {
                Some(s.memory_working_set_bytes as f64 / (1024.0 * 1024.0))
            } else {
                None
            },
            if s.memory_working_set_peak_bytes > 0 {
                Some(s.memory_working_set_peak_bytes as f64 / (1024.0 * 1024.0))
            } else {
                None
            },
        );
        self.observe_queue_max(Some(s.input_queue_max), Some(s.session_queue_max));
        self.observe_runtime(s);
    }

    /// Observe independently present fields (missed fields left as `None`).
    pub fn observe_partial(&mut self, o: &PartialServerObservation) {
        let any = o.tick_work_mean_ms.is_some()
            || o.tick_work_p95_ms.is_some()
            || o.tick_work_p99_ms.is_some()
            || o.tick_work_max_ms.is_some()
            || o.tick_overruns_total.is_some()
            || o.bytes_in.is_some()
            || o.bytes_out.is_some()
            || o.memory_mb.is_some()
            || o.memory_peak_mb.is_some()
            || o.input_queue_max.is_some()
            || o.session_queue_max.is_some()
            || o.scheduler_lateness_p95_ms.is_some()
            || o.scheduler_lateness_max_ms.is_some();
        if !any {
            self.note_miss();
            return;
        }
        self.samples_ok = self.samples_ok.saturating_add(1);
        self.observe_tick_mean(o.tick_work_mean_ms, o.tick_count);
        self.observe_tick_peaks(o.tick_work_p95_ms, o.tick_work_p99_ms, o.tick_work_max_ms);
        self.observe_lateness(o.scheduler_lateness_p95_ms, o.scheduler_lateness_max_ms);
        if let Some(v) = o.tick_overruns_total {
            self.last_overruns = Some(v);
        }
        if let Some(v) = o.bytes_in {
            self.last_bytes_in = Some(v);
        }
        if let Some(v) = o.bytes_out {
            self.last_bytes_out = Some(v);
        }
        self.observe_memory(o.memory_mb, o.memory_peak_mb);
        self.observe_queue_max(o.input_queue_max, o.session_queue_max);
    }

    fn observe_tick_mean(&mut self, mean: Option<f64>, tick_count: Option<u64>) {
        let Some(v) = mean else {
            return;
        };
        let weight = match (tick_count, self.prev_tick_count) {
            (Some(now), Some(prev)) => now.saturating_sub(prev).max(1) as f64,
            (Some(now), None) => now.clamp(1, TICK_WORK_RING_CAP) as f64,
            _ => 1.0,
        };
        if let Some(now) = tick_count {
            self.prev_tick_count.replace(now);
        }
        self.tick_mean_weighted_sum += v * weight;
        self.tick_mean_weight += weight;
    }

    fn observe_tick_peaks(&mut self, p95: Option<f64>, p99: Option<f64>, max: Option<f64>) {
        if let Some(v) = p95 {
            self.tick_p95_peak = Some(self.tick_p95_peak.unwrap_or(v).max(v));
        }
        if let Some(v) = p99 {
            self.tick_p99_peak = Some(self.tick_p99_peak.unwrap_or(v).max(v));
        }
        if let Some(v) = max {
            self.tick_max_peak = Some(self.tick_max_peak.unwrap_or(v).max(v));
        }
    }

    fn observe_lateness(&mut self, p95: Option<f64>, max: Option<f64>) {
        if let Some(v) = p95 {
            self.lateness_p95_peak = Some(self.lateness_p95_peak.unwrap_or(v).max(v));
        }
        if let Some(v) = max {
            self.lateness_max_peak = Some(self.lateness_max_peak.unwrap_or(v).max(v));
        }
    }

    fn observe_memory(&mut self, current_mb: Option<f64>, peak_mb: Option<f64>) {
        if let Some(mb) = current_mb {
            if self.memory_start_mb.is_none() {
                self.memory_start_mb = Some(mb);
            }
            self.memory_end_mb = Some(mb);
            self.memory_peak_mb = Some(self.memory_peak_mb.unwrap_or(mb).max(mb));
        }
        if let Some(mb) = peak_mb {
            self.memory_peak_mb = Some(self.memory_peak_mb.unwrap_or(mb).max(mb));
        }
    }

    fn observe_queue_max(&mut self, input: Option<u64>, session: Option<u64>) {
        if let Some(v) = input {
            self.input_queue_max = Some(self.input_queue_max.unwrap_or(0).max(v));
        }
        if let Some(v) = session {
            self.session_queue_max = Some(self.session_queue_max.unwrap_or(0).max(v));
        }
    }

    fn observe_runtime(&mut self, s: &LoadMetricsV1) {
        if self.scheduler_queued_start.is_none() {
            self.scheduler_queued_start = Some(s.scheduler_queued);
        }
        self.scheduler_queued_end = Some(s.scheduler_queued);
        self.scheduler_queued_max = Some(
            self.scheduler_queued_max
                .unwrap_or(0)
                .max(s.scheduler_queued),
        );
        self.actions_active_max = Some(self.actions_active_max.unwrap_or(0).max(s.actions_active));
        self.spawn_queue_depth_max = Some(
            self.spawn_queue_depth_max
                .unwrap_or(0)
                .max(s.spawn_queue_depth),
        );
        self.aoi_enters_total = Some(s.aoi_enters);
        self.aoi_leaves_total = Some(s.aoi_leaves);
        self.aoi_updates_total = Some(s.aoi_updates);
        self.scheduler_critical_ceiling_hits = Some(s.scheduler_critical_ceiling_hits);
        self.scheduler_deferred_exhausted = Some(s.scheduler_deferred_exhausted);
    }

    #[must_use]
    pub fn samples_ok(&self) -> u64 {
        self.samples_ok
    }

    #[must_use]
    pub fn samples_missed(&self) -> u64 {
        self.samples_missed
    }

    #[must_use]
    pub fn miss_ratio(&self) -> f64 {
        let total = self.samples_ok.saturating_add(self.samples_missed);
        if total == 0 {
            return 1.0;
        }
        self.samples_missed as f64 / total as f64
    }

    #[must_use]
    pub fn health(&self) -> MetricsHealth {
        if self.samples_ok == 0 {
            MetricsHealth::None
        } else if self.miss_ratio() >= PARTIAL_MISS_RATIO {
            MetricsHealth::Partial
        } else {
            MetricsHealth::Healthy
        }
    }

    /// True when at least one valid sample contributed aggregates.
    #[must_use]
    pub fn server_metrics_ok(&self) -> bool {
        self.samples_ok > 0
    }

    #[must_use]
    pub fn to_summary_extras(&self, harness_memory_peak_mb: Option<f64>) -> SummaryExtras {
        SummaryExtras {
            server_tick_overruns_total: self.last_overruns,
            server_tick_work_mean_ms: if self.tick_mean_weight > 0.0 {
                Some(self.tick_mean_weighted_sum / self.tick_mean_weight)
            } else {
                None
            },
            server_tick_work_p95_ms: self.tick_p95_peak,
            server_tick_work_p99_ms: self.tick_p99_peak,
            server_tick_work_max_ms: self.tick_max_peak,
            server_scheduler_lateness_p95_ms: self.lateness_p95_peak,
            server_scheduler_lateness_max_ms: self.lateness_max_peak,
            server_bytes_in: self.last_bytes_in,
            server_bytes_out: self.last_bytes_out,
            server_memory_start_mb: self.memory_start_mb,
            server_memory_peak_mb: self.memory_peak_mb,
            server_memory_end_mb: self.memory_end_mb,
            harness_memory_peak_mb,
            server_input_queue_max: self.input_queue_max,
            server_session_queue_max: self.session_queue_max,
            input_handoff_dropped_total: None,
            scheduler_queued_start: self.scheduler_queued_start,
            scheduler_queued_end: self.scheduler_queued_end,
            scheduler_queued_max: self.scheduler_queued_max,
            actions_active_max: self.actions_active_max,
            spawn_queue_depth_max: self.spawn_queue_depth_max,
            aoi_enters_total: self.aoi_enters_total,
            aoi_leaves_total: self.aoi_leaves_total,
            aoi_updates_total: self.aoi_updates_total,
            scheduler_critical_ceiling_hits: self.scheduler_critical_ceiling_hits,
            scheduler_deferred_exhausted: self.scheduler_deferred_exhausted,
        }
    }

    /// Attach final counter snapshots that are not window-aggregated.
    #[must_use]
    pub fn with_final_counters(
        mut extras: SummaryExtras,
        input_handoff_dropped: u64,
    ) -> SummaryExtras {
        if input_handoff_dropped > 0 || extras.input_handoff_dropped_total.is_none() {
            extras.input_handoff_dropped_total = Some(input_handoff_dropped);
        }
        extras
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(
        mean: f64,
        p95: f64,
        p99: f64,
        max: f64,
        overruns: u64,
        bytes: u64,
        tick_count: u64,
    ) -> LoadMetricsV1 {
        LoadMetricsV1 {
            tick_work_mean_ms: mean,
            tick_work_p95_ms: p95,
            tick_work_p99_ms: p99,
            tick_work_max_ms: max,
            tick_overrun_count: overruns,
            tick_count,
            bytes_in: bytes,
            bytes_out: bytes * 2,
            memory_working_set_bytes: 10 * 1024 * 1024,
            memory_working_set_peak_bytes: 12 * 1024 * 1024,
            input_queue_max: 3,
            session_queue_max: 2,
            scheduler_lateness_p95_ms: 0.1,
            scheduler_lateness_max_ms: 0.5,
            ..LoadMetricsV1::with_schema()
        }
    }

    #[test]
    fn complete_short_sequence_populates_all() {
        let mut a = ServerRunAggregator::new();
        a.observe(&sample(1.0, 2.0, 3.0, 4.0, 0, 100, 30));
        a.observe(&sample(3.0, 4.0, 5.0, 6.0, 1, 200, 60));
        let e = a.to_summary_extras(Some(8.0));
        assert_eq!(a.health(), MetricsHealth::Healthy);
        assert!(a.server_metrics_ok());
        // weights: first min(30,120)=30, second Δ=30 → mean = (1*30 + 3*30)/60 = 2.0
        assert!((e.server_tick_work_mean_ms.unwrap() - 2.0).abs() < 1e-9);
        assert_eq!(e.server_tick_work_p95_ms, Some(4.0));
        assert_eq!(e.server_tick_work_p99_ms, Some(5.0));
        assert_eq!(e.server_tick_work_max_ms, Some(6.0));
        assert_eq!(e.server_tick_overruns_total, Some(1));
        assert_eq!(e.server_bytes_in, Some(200));
        assert_eq!(e.server_bytes_out, Some(400));
        assert!(e.server_memory_start_mb.is_some());
        assert_eq!(e.server_scheduler_lateness_max_ms, Some(0.5));
        assert_eq!(e.harness_memory_peak_mb, Some(8.0));
    }

    #[test]
    fn weighted_mean_prefers_heavier_tick_intervals() {
        let mut a = ServerRunAggregator::new();
        a.observe(&sample(10.0, 10.0, 10.0, 10.0, 0, 1, 10));
        a.observe(&sample(1.0, 1.0, 1.0, 1.0, 0, 2, 110));
        // weights 10 and 100 → (10*10 + 1*100)/110
        let mean = a.to_summary_extras(None).server_tick_work_mean_ms.unwrap();
        assert!((mean - (100.0 + 100.0) / 110.0).abs() < 1e-9);
    }

    #[test]
    fn long_sequence_thousands_of_samples_never_all_none() {
        let mut a = ServerRunAggregator::new();
        let mut ticks = 0u64;
        for i in 0..5000u64 {
            if i % 17 == 0 {
                a.note_miss();
                continue;
            }
            ticks = ticks.saturating_add(30);
            a.observe(&sample(
                1.0 + (i % 5) as f64 * 0.1,
                2.0,
                3.0,
                4.0 + (i % 3) as f64,
                i / 100,
                i * 10,
                ticks,
            ));
        }
        assert!(a.samples_ok() > 4000);
        assert!(a.server_metrics_ok());
        let e = a.to_summary_extras(None);
        assert!(e.server_tick_work_mean_ms.is_some());
        assert!(e.server_tick_work_p95_ms.is_some());
        assert!(e.server_tick_work_p99_ms.is_some());
        assert!(e.server_tick_work_max_ms.is_some());
        assert_eq!(a.health(), MetricsHealth::Partial);
    }

    #[test]
    fn intermittent_misses_do_not_erase_prior_aggregates() {
        let mut a = ServerRunAggregator::new();
        a.observe(&sample(5.0, 6.0, 7.0, 8.0, 2, 1000, 30));
        for _ in 0..10 {
            a.note_miss();
        }
        let e = a.to_summary_extras(None);
        assert_eq!(e.server_tick_work_mean_ms, Some(5.0));
        assert_eq!(e.server_bytes_in, Some(1000));
        assert_eq!(e.server_tick_overruns_total, Some(2));
        assert!(a.server_metrics_ok());
    }

    #[test]
    fn one_metric_missing_in_partial_sample_keeps_others() {
        let mut a = ServerRunAggregator::new();
        a.observe_partial(&PartialServerObservation {
            tick_work_mean_ms: Some(1.5),
            tick_work_p95_ms: None,
            tick_work_p99_ms: Some(3.0),
            tick_work_max_ms: Some(4.0),
            tick_overruns_total: Some(0),
            tick_count: Some(30),
            bytes_in: Some(50),
            bytes_out: None,
            memory_mb: None,
            memory_peak_mb: None,
            input_queue_max: Some(1),
            session_queue_max: None,
            scheduler_lateness_p95_ms: None,
            scheduler_lateness_max_ms: None,
        });
        let e = a.to_summary_extras(None);
        assert_eq!(e.server_tick_work_mean_ms, Some(1.5));
        assert!(e.server_tick_work_p95_ms.is_none());
        assert_eq!(e.server_tick_work_p99_ms, Some(3.0));
        assert_eq!(e.server_bytes_in, Some(50));
        assert!(e.server_bytes_out.is_none());
        assert!(e.server_memory_start_mb.is_none());
        assert_eq!(e.server_input_queue_max, Some(1));
        assert!(e.server_session_queue_max.is_none());
    }

    #[test]
    fn no_metrics_at_all() {
        let a = ServerRunAggregator::new();
        assert_eq!(a.health(), MetricsHealth::None);
        assert!(!a.server_metrics_ok());
        let e = a.to_summary_extras(None);
        assert!(e.server_tick_work_mean_ms.is_none());
        assert!(e.server_bytes_in.is_none());
        assert!(e.server_memory_peak_mb.is_none());
    }

    #[test]
    fn partial_health_when_miss_ratio_high() {
        let mut a = ServerRunAggregator::new();
        a.observe(&sample(1.0, 1.0, 1.0, 1.0, 0, 1, 30));
        a.note_miss();
        assert_eq!(a.health(), MetricsHealth::Partial);
        assert!(a.server_metrics_ok());
        assert!(a.to_summary_extras(None).server_tick_work_mean_ms.is_some());
    }

    #[test]
    fn healthy_with_few_misses() {
        let mut a = ServerRunAggregator::new();
        let mut ticks = 0u64;
        for _ in 0..100 {
            ticks += 30;
            a.observe(&sample(1.0, 1.0, 1.0, 1.0, 0, 1, ticks));
        }
        a.note_miss();
        a.note_miss();
        assert_eq!(a.health(), MetricsHealth::Healthy);
    }

    #[test]
    fn percentile_semantics_constant() {
        assert_eq!(TICK_PERCENTILE_SEMANTICS, "peak_of_window_percentiles");
        assert_eq!(TICK_WORK_RING_CAP, 120);
    }
}
