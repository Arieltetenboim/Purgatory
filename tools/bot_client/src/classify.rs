//! Run status classification (correctness, not capacity).

use serde::Serialize;

use crate::aggregate::MetricsHealth;
use crate::metrics::HarnessMetrics;
use purgatory_common::LoadMetricsV1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunStatus {
    Complete,
    Warn,
    Failed,
    Aborted,
}

impl RunStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "COMPLETE",
            Self::Warn => "WARN",
            Self::Failed => "FAILED",
            Self::Aborted => "ABORTED",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StatusReason {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

impl StatusReason {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        value: Option<serde_json::Value>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            value,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClassifyInput {
    pub aborted: bool,
    pub ramp_complete: bool,
    pub connected_bots: u32,
    pub starved_bots: u32,
    pub consecutive_starvation_samples: u32,
    pub all_connected_starved: bool,
    /// Consecutive 1 Hz samples where **server** tick_work_p99 exceeded 33.333 ms.
    pub consecutive_server_p99_over_budget: u32,
    pub server_overrun_count: u64,
    pub overflow_events: u64,
    pub encode_failures: u64,
    pub admission_refusals: u64,
    pub unexpected_disconnects: u64,
    pub cleanup_leak: bool,
    /// Harness `--timeout` elapsed (distinct from scenario duration).
    pub timed_out: bool,
    pub reconnect_entity_unchanged: u64,
    pub duplicate_welcome: u64,
    /// Server UDP metrics health for the run (`none` / `partial` / `healthy`).
    pub metrics_health: MetricsHealth,
    pub metrics_samples_ok: u64,
    pub metrics_samples_missed: u64,
    /// Mixed/soak continuity. `None` keeps Phase 5 / non-mixed classify unchanged.
    pub soak: Option<SoakClassify>,
}

/// Evidence for mixed/soak real-client correctness. Not a metrics-schema bump.
#[derive(Clone, Debug, PartialEq)]
pub struct SoakClassify {
    pub require_persistent_baseline: bool,
    pub require_portal_transition: bool,
    pub require_churn: bool,
    pub persistent_target: u32,
    pub min_persistent_connected: u32,
    pub avg_persistent_connected: f64,
    pub time_below_baseline_secs: f64,
    pub consecutive_below_baseline_secs: f64,
    pub connected_seconds: f64,
    pub duration_secs: f64,
    pub ramp_complete: bool,
    pub portal_transitions: u64,
    pub portal_attempts: u64,
    pub aoi_updates_delta: u64,
    pub aoi_enters_delta: u64,
    pub aoi_observed: bool,
    pub churn_disconnects: u64,
    pub early_fail: Option<String>,
}

/// Persistent occupancy integral: `count × elapsed seconds`, not one unit per sample.
#[must_use]
pub fn occupancy_seconds(persistent: u32, dt_secs: f64) -> f64 {
    f64::from(persistent) * dt_secs.max(0.0)
}

impl Default for ClassifyInput {
    fn default() -> Self {
        Self {
            aborted: false,
            ramp_complete: false,
            connected_bots: 0,
            starved_bots: 0,
            consecutive_starvation_samples: 0,
            all_connected_starved: false,
            consecutive_server_p99_over_budget: 0,
            server_overrun_count: 0,
            overflow_events: 0,
            encode_failures: 0,
            admission_refusals: 0,
            unexpected_disconnects: 0,
            cleanup_leak: false,
            timed_out: false,
            reconnect_entity_unchanged: 0,
            duplicate_welcome: 0,
            // Tests / callers that omit telemetry treat metrics as healthy unless set.
            metrics_health: MetricsHealth::Healthy,
            metrics_samples_ok: 0,
            metrics_samples_missed: 0,
            soak: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Classification {
    pub status: RunStatus,
    pub reasons: Vec<StatusReason>,
}

impl Classification {
    /// Distinguishes interrupt / timeout / correctness / performance observation.
    #[must_use]
    pub fn failure_class(&self) -> &'static str {
        match self.status {
            RunStatus::Aborted => "interrupted",
            RunStatus::Failed => {
                if self.reasons.iter().any(|r| r.code == "timeout") {
                    "timeout"
                } else {
                    "correctness"
                }
            }
            RunStatus::Warn => "performance_observation",
            RunStatus::Complete => "none",
        }
    }
}

impl ClassifyInput {
    /// Fill server-derived counters (or harness fallbacks when the poll missed).
    pub fn with_server_counters(
        mut self,
        metrics: &HarnessMetrics,
        server: Option<&LoadMetricsV1>,
    ) -> Self {
        let (overrun, overflow, encode, admission) = match server {
            Some(s) => (
                s.tick_overrun_count,
                s.lifecycle_handoff_dropped
                    .saturating_add(s.input_queue_overflow)
                    .saturating_add(s.input_handoff_dropped),
                s.snapshot_encode_failed,
                s.admission_refused,
            ),
            None => (
                0,
                metrics.overflow_events,
                metrics.encode_failures,
                metrics.admission_refusals,
            ),
        };
        self.server_overrun_count = overrun;
        self.overflow_events = overflow;
        self.encode_failures = encode;
        self.admission_refusals = admission;
        self.unexpected_disconnects = metrics.unexpected_disconnects;
        self
    }

    #[must_use]
    pub fn classify(&self) -> Classification {
        if self.aborted {
            return Classification {
                status: RunStatus::Aborted,
                reasons: vec![StatusReason::new(
                    "aborted",
                    "run aborted by shutdown signal",
                    None,
                )],
            };
        }

        let mut failed: Vec<StatusReason> = Vec::new();
        if self.timed_out {
            failed.push(StatusReason::new(
                "timeout",
                "harness wall-clock timeout elapsed before scenario duration completed",
                None,
            ));
        }
        if self.reconnect_entity_unchanged > 0 {
            failed.push(StatusReason::new(
                "stale_entity_id",
                format!(
                    "reconnect reused EntityId {} time(s)",
                    self.reconnect_entity_unchanged
                ),
                Some(serde_json::json!(self.reconnect_entity_unchanged)),
            ));
        }
        if self.duplicate_welcome > 0 {
            failed.push(StatusReason::new(
                "duplicate_character",
                format!(
                    "duplicate live Character accepted {} time(s)",
                    self.duplicate_welcome
                ),
                Some(serde_json::json!(self.duplicate_welcome)),
            ));
        }
        if self.overflow_events > 0 {
            failed.push(StatusReason::new(
                "queue_overflow",
                format!("queue/lifecycle overflow events={}", self.overflow_events),
                Some(serde_json::json!(self.overflow_events)),
            ));
        }
        if self.encode_failures > 0 {
            failed.push(StatusReason::new(
                "snapshot_encode_failed",
                format!("snapshot encode failures={}", self.encode_failures),
                Some(serde_json::json!(self.encode_failures)),
            ));
        }
        if self.cleanup_leak {
            failed.push(StatusReason::new(
                "cleanup_leak",
                "sessions remained after cleanup",
                None,
            ));
        }
        if let Some(soak) = &self.soak {
            append_soak_failures(&mut failed, soak);
        }
        if self.ramp_complete
            && self.connected_bots > 0
            && (self.consecutive_starvation_samples >= 5 || self.all_connected_starved)
        {
            failed.push(StatusReason::new(
                "snapshot_starvation",
                format!(
                    "snapshot starvation samples={} starved_bots={} all_starved={}",
                    self.consecutive_starvation_samples,
                    self.starved_bots,
                    self.all_connected_starved
                ),
                Some(serde_json::json!({
                    "consecutive_samples": self.consecutive_starvation_samples,
                    "starved_bots": self.starved_bots,
                    "all_connected_starved": self.all_connected_starved,
                })),
            ));
        }
        if !failed.is_empty() {
            return Classification {
                status: RunStatus::Failed,
                reasons: failed,
            };
        }

        let mut warn: Vec<StatusReason> = Vec::new();
        // Sustained **server** tick pressure only. Bot scheduler cadence is not a WARN.
        if self.server_overrun_count >= 10 {
            warn.push(StatusReason::new(
                "sustained_tick_pressure",
                format!(
                    "server tick overrun count {} (>= 10)",
                    self.server_overrun_count
                ),
                Some(serde_json::json!(self.server_overrun_count)),
            ));
        }
        if self.consecutive_server_p99_over_budget >= 3 {
            warn.push(StatusReason::new(
                "sustained_tick_pressure",
                format!(
                    "server tick_work_p99 over budget for {} consecutive samples",
                    self.consecutive_server_p99_over_budget
                ),
                Some(serde_json::json!(self.consecutive_server_p99_over_budget)),
            ));
        }
        if self.admission_refusals > 0 {
            warn.push(StatusReason::new(
                "admission_refusals",
                format!("admission refusals={}", self.admission_refusals),
                Some(serde_json::json!(self.admission_refusals)),
            ));
        }
        if self.unexpected_disconnects > 0 {
            warn.push(StatusReason::new(
                "unexpected_disconnects",
                format!("unexpected_disconnects={}", self.unexpected_disconnects),
                Some(serde_json::json!(self.unexpected_disconnects)),
            ));
        }
        match self.metrics_health {
            MetricsHealth::None => {
                warn.push(StatusReason::new(
                    "server_metrics_unavailable",
                    "no successful server metric samples received",
                    Some(serde_json::json!({
                        "samples_ok": self.metrics_samples_ok,
                        "samples_missed": self.metrics_samples_missed,
                    })),
                ));
            }
            MetricsHealth::Partial => {
                warn.push(StatusReason::new(
                    "incomplete_server_metrics",
                    format!(
                        "incomplete server metrics: ok={} missed={}",
                        self.metrics_samples_ok, self.metrics_samples_missed
                    ),
                    Some(serde_json::json!({
                        "health": self.metrics_health.as_str(),
                        "samples_ok": self.metrics_samples_ok,
                        "samples_missed": self.metrics_samples_missed,
                    })),
                ));
            }
            MetricsHealth::Healthy => {}
        }
        if !warn.is_empty() {
            return Classification {
                status: RunStatus::Warn,
                reasons: warn,
            };
        }

        Classification {
            status: RunStatus::Complete,
            reasons: Vec::new(),
        }
    }
}

fn append_soak_failures(failed: &mut Vec<StatusReason>, soak: &SoakClassify) {
    if let Some(reason) = &soak.early_fail {
        failed.push(StatusReason::new("early_fail", reason.clone(), None));
    }
    if soak.require_persistent_baseline && soak.ramp_complete {
        if soak.persistent_target > 0 && soak.min_persistent_connected == 0 {
            failed.push(StatusReason::new(
                "persistent_clients_lost",
                format!(
                    "persistent real clients reached 0 (target={})",
                    soak.persistent_target
                ),
                Some(serde_json::json!({
                    "target": soak.persistent_target,
                    "min": soak.min_persistent_connected,
                    "time_below_secs": soak.time_below_baseline_secs,
                })),
            ));
        }
        if soak.time_below_baseline_secs > 5.0 || soak.consecutive_below_baseline_secs >= 8.0 {
            failed.push(StatusReason::new(
                "persistent_baseline_lost",
                format!(
                    "persistent real clients below target {} for {:.1}s (consecutive {:.1}s)",
                    soak.persistent_target,
                    soak.time_below_baseline_secs,
                    soak.consecutive_below_baseline_secs
                ),
                Some(serde_json::json!({
                    "target": soak.persistent_target,
                    "min": soak.min_persistent_connected,
                    "avg": soak.avg_persistent_connected,
                    "time_below_secs": soak.time_below_baseline_secs,
                    "consecutive_below_secs": soak.consecutive_below_baseline_secs,
                    "connected_seconds": soak.connected_seconds,
                })),
            ));
        }
        let min_exposure = f64::from(soak.persistent_target) * soak.duration_secs * 0.5;
        if soak.duration_secs >= 8.0 && soak.connected_seconds + 0.01 < min_exposure {
            failed.push(StatusReason::new(
                "real_client_exposure",
                format!(
                    "connected-seconds {:.1} below required {:.1} (target={} duration={}s)",
                    soak.connected_seconds,
                    min_exposure,
                    soak.persistent_target,
                    soak.duration_secs
                ),
                Some(serde_json::json!({
                    "connected_seconds": soak.connected_seconds,
                    "required": min_exposure,
                })),
            ));
        }
    }
    if soak.require_portal_transition && soak.portal_transitions == 0 {
        failed.push(StatusReason::new(
            "portal_transition_missing",
            format!(
                "no authoritative portal transition (attempts={} rejects counted separately)",
                soak.portal_attempts
            ),
            Some(serde_json::json!({
                "attempts": soak.portal_attempts,
                "transitions": soak.portal_transitions,
            })),
        ));
    }
    if soak.require_persistent_baseline
        && soak.aoi_observed
        && soak.ramp_complete
        && soak.duration_secs >= 8.0
        && soak.aoi_updates_delta == 0
        && soak.aoi_enters_delta <= 3
    {
        failed.push(StatusReason::new(
            "aoi_replication_idle",
            format!(
                "AOI/replication did not continue past startup (enters_delta={} updates_delta={})",
                soak.aoi_enters_delta, soak.aoi_updates_delta
            ),
            Some(serde_json::json!({
                "aoi_enters_delta": soak.aoi_enters_delta,
                "aoi_updates_delta": soak.aoi_updates_delta,
            })),
        ));
    }
    if soak.require_churn
        && soak.ramp_complete
        && soak.duration_secs >= 12.0
        && soak.churn_disconnects == 0
    {
        failed.push(StatusReason::new(
            "churn_missing",
            "mixed soak produced no churn disconnects after ramp",
            None,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_overrun_is_complete() {
        let input = ClassifyInput {
            server_overrun_count: 1,
            ..Default::default()
        };
        let c = input.classify();
        assert_eq!(c.status, RunStatus::Complete);
        assert!(c.reasons.is_empty());
    }

    #[test]
    fn sustained_overruns_warn_with_reason() {
        let input = ClassifyInput {
            server_overrun_count: 10,
            ..Default::default()
        };
        let c = input.classify();
        assert_eq!(c.status, RunStatus::Warn);
        assert!(!c.reasons.is_empty());
        assert_eq!(c.reasons[0].code, "sustained_tick_pressure");
    }

    #[test]
    fn bot_scheduler_pressure_fields_do_not_exist_as_warn_inputs() {
        // Regression: harness wall-clock cadence > 33ms must not WARN by itself.
        let input = ClassifyInput {
            consecutive_server_p99_over_budget: 0,
            server_overrun_count: 0,
            ..Default::default()
        };
        assert_eq!(input.classify().status, RunStatus::Complete);
    }

    #[test]
    fn overflow_fails_with_reason() {
        let input = ClassifyInput {
            overflow_events: 1,
            ..Default::default()
        };
        let c = input.classify();
        assert_eq!(c.status, RunStatus::Failed);
        assert_eq!(c.reasons[0].code, "queue_overflow");
    }

    #[test]
    fn starvation_fails_after_ramp() {
        let input = ClassifyInput {
            ramp_complete: true,
            connected_bots: 5,
            consecutive_starvation_samples: 5,
            starved_bots: 1,
            ..Default::default()
        };
        let c = input.classify();
        assert_eq!(c.status, RunStatus::Failed);
        assert_eq!(c.reasons[0].code, "snapshot_starvation");
    }

    #[test]
    fn unexpected_disconnects_warn() {
        let input = ClassifyInput {
            unexpected_disconnects: 2,
            ..Default::default()
        };
        let c = input.classify();
        assert_eq!(c.status, RunStatus::Warn);
        assert_eq!(c.reasons[0].code, "unexpected_disconnects");
    }

    #[test]
    fn duplicate_character_and_stale_entity_are_hard_fails() {
        let dup = ClassifyInput {
            duplicate_welcome: 1,
            ..Default::default()
        }
        .classify();
        assert_eq!(dup.status, RunStatus::Failed);
        assert_eq!(dup.reasons[0].code, "duplicate_character");

        let stale = ClassifyInput {
            reconnect_entity_unchanged: 1,
            ..Default::default()
        }
        .classify();
        assert_eq!(stale.status, RunStatus::Failed);
        assert_eq!(stale.reasons[0].code, "stale_entity_id");
    }

    #[test]
    fn timeout_fails_distinctly_from_abort() {
        let timeout = ClassifyInput {
            timed_out: true,
            ..Default::default()
        }
        .classify();
        assert_eq!(timeout.status, RunStatus::Failed);
        assert_eq!(timeout.reasons[0].code, "timeout");

        let aborted = ClassifyInput {
            aborted: true,
            timed_out: true,
            ..Default::default()
        }
        .classify();
        assert_eq!(aborted.status, RunStatus::Aborted);
    }

    #[test]
    fn complete_has_no_reasons() {
        let c = ClassifyInput::default().classify();
        assert_eq!(c.status, RunStatus::Complete);
        assert!(c.reasons.is_empty());
    }

    fn mixed_ok() -> SoakClassify {
        SoakClassify {
            require_persistent_baseline: true,
            require_portal_transition: true,
            require_churn: true,
            persistent_target: 6,
            min_persistent_connected: 6,
            avg_persistent_connected: 6.0,
            time_below_baseline_secs: 0.0,
            consecutive_below_baseline_secs: 0.0,
            connected_seconds: 120.0,
            duration_secs: 20.0,
            ramp_complete: true,
            portal_transitions: 1,
            portal_attempts: 1,
            aoi_updates_delta: 40,
            aoi_enters_delta: 12,
            aoi_observed: true,
            churn_disconnects: 2,
            early_fail: None,
        }
    }

    #[test]
    fn mixed_shaped_input_passes() {
        let input = ClassifyInput {
            soak: Some(mixed_ok()),
            ..Default::default()
        };
        assert_eq!(input.classify().status, RunStatus::Complete);
    }

    #[test]
    fn occupancy_seconds_is_count_times_dt() {
        assert!((occupancy_seconds(6, 2.5) - 15.0).abs() < f64::EPSILON);
        assert_eq!(occupancy_seconds(6, -1.0), 0.0);
    }

    #[test]
    fn mixed_exposure_boundary_uses_integral_not_sample_count() {
        let mut soak = mixed_ok();
        soak.connected_seconds = 60.0;
        let pass = ClassifyInput {
            soak: Some(soak.clone()),
            ..Default::default()
        };
        assert_eq!(pass.classify().status, RunStatus::Complete);

        soak.connected_seconds = 59.0;
        let fail = ClassifyInput {
            soak: Some(soak),
            ..Default::default()
        };
        assert!(
            fail.classify()
                .reasons
                .iter()
                .any(|r| r.code == "real_client_exposure")
        );
    }

    #[test]
    fn mixed_zero_portal_transitions_fails() {
        let mut soak = mixed_ok();
        soak.portal_transitions = 0;
        soak.portal_attempts = 4;
        let input = ClassifyInput {
            soak: Some(soak),
            ..Default::default()
        };
        let c = input.classify();
        assert_eq!(c.status, RunStatus::Failed);
        assert!(
            c.reasons
                .iter()
                .any(|r| r.code == "portal_transition_missing")
        );
    }

    #[test]
    fn mixed_persistent_baseline_lost_fails() {
        let mut soak = mixed_ok();
        soak.time_below_baseline_secs = 6.5;
        soak.min_persistent_connected = 0;
        let input = ClassifyInput {
            soak: Some(soak),
            ..Default::default()
        };
        let c = input.classify();
        assert_eq!(c.status, RunStatus::Failed);
        assert!(c.reasons.iter().any(|r| {
            r.code == "persistent_baseline_lost" || r.code == "persistent_clients_lost"
        }));
    }

    #[test]
    fn incomplete_metrics_warn_with_reason() {
        let input = ClassifyInput {
            metrics_health: MetricsHealth::Partial,
            ..Default::default()
        };
        let c = input.classify();
        assert_eq!(c.status, RunStatus::Warn);
        assert!(
            c.reasons
                .iter()
                .any(|r| r.code == "incomplete_server_metrics")
        );
    }

    #[test]
    fn no_metrics_warn_with_reason() {
        let input = ClassifyInput {
            metrics_health: MetricsHealth::None,
            ..Default::default()
        };
        let c = input.classify();
        assert_eq!(c.status, RunStatus::Warn);
        assert!(
            c.reasons
                .iter()
                .any(|r| r.code == "server_metrics_unavailable")
        );
    }

    #[test]
    fn warn_always_has_reasons() {
        let cases = [
            ClassifyInput {
                unexpected_disconnects: 1,
                ..Default::default()
            },
            ClassifyInput {
                metrics_health: MetricsHealth::Partial,
                ..Default::default()
            },
            ClassifyInput {
                server_overrun_count: 10,
                ..Default::default()
            },
        ];
        for input in cases {
            let c = input.classify();
            assert_eq!(c.status, RunStatus::Warn);
            assert!(!c.reasons.is_empty());
        }
    }
}
