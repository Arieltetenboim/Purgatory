//! Capacity characterization artifacts (Phase 6G.2).
//!
//! Coarse per-domain tick timings and process resources written to run
//! directories. **Not** part of the live UDP `PURGSTAT` schema (schema 4
//! is validation execution totals; domain timings stay in these files).
//! Set `PURGATORY_CAPACITY_ARTIFACT_DIR` on the server to enable file export.

use serde::{Deserialize, Serialize};

/// Env var: directory where the server writes capacity JSON/NDJSON.
pub const CAPACITY_ARTIFACT_DIR_ENV: &str = "PURGATORY_CAPACITY_ARTIFACT_DIR";

/// One domain's percentile summary in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DomainTimingMs {
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    pub sample_count: u64,
}

/// Snapshot of coarse tick-domain accounting.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TickDomainSnapshot {
    pub schema: u32,
    pub wall_secs: f64,
    pub tick_count: u64,
    pub tick_overrun_count: u64,
    pub tick_total: DomainTimingMs,
    pub commands_input: DomainTimingMs,
    pub simulation_movement: DomainTimingMs,
    pub gameplay_services: DomainTimingMs,
    pub spatial_aoi: DomainTimingMs,
    pub replication: DomainTimingMs,
    pub persistence_enqueue: DomainTimingMs,
}

/// Process-level resources for capacity headroom.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProcessResourceSnapshot {
    pub schema: u32,
    pub wall_secs: f64,
    pub logical_cpus: u32,
    pub working_set_bytes: u64,
    pub working_set_peak_bytes: u64,
    pub working_set_start_bytes: u64,
    /// Cumulative process CPU time (user+kernel) in seconds.
    pub cpu_time_secs: f64,
    /// Approximate utilization over the last sample interval: 0..100 * logical_cpus
    /// (100 = one core saturated).
    pub cpu_utilization_pct: f64,
    pub cpu_time_delta_secs: f64,
    pub sample_interval_secs: f64,
}

impl TickDomainSnapshot {
    pub const SCHEMA: u32 = 1;
}

impl ProcessResourceSnapshot {
    pub const SCHEMA: u32 = 1;
}

/// Percentile summary for a non-negative integer sample stream (6G.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IntDistribution {
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: u64,
    pub sample_count: u64,
}

/// Aggregate interest-invalidation locality snapshot (capacity artifact).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InterestLocalitySnapshot {
    pub schema: u32,
    pub invalidation_events: u64,
    pub cell_boundary_crossings: u64,
    pub same_cell_moves: u64,
    pub cells_touched_total: u64,
    /// Observers actually marked dirty after XOR/presence filter (6G.7A).
    pub observers_dirtied: IntDistribution,
    pub observers_dirtied_total: u64,
    pub entities_moved_total: u64,
    /// Influence prefilter player count before XOR (6G.7A schema 2).
    #[serde(default)]
    pub influence_prefilter_total: u64,
    /// Observers with enter/leave XOR true (excludes subject-only marks) (6G.7A).
    #[serde(default)]
    pub membership_xor_total: u64,
    #[serde(default)]
    pub influence_prefilter: IntDistribution,
}

impl InterestLocalitySnapshot {
    pub const SCHEMA: u32 = 2;

    /// `observers_dirtied_total / entities_moved_total` (0 if no moves).
    #[must_use]
    pub fn observers_dirtied_per_moved_entity(&self) -> f64 {
        if self.entities_moved_total == 0 {
            0.0
        } else {
            self.observers_dirtied_total as f64 / self.entities_moved_total as f64
        }
    }
}

/// Aggregate replication dirty fan-out discovery metrics (6G.7B capacity artifact).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReplicationFanoutSnapshot {
    pub schema: u32,
    pub publish_passes: u64,
    pub dirty_entities_total: u64,
    pub dirty_transform_total: u64,
    pub dirty_health_total: u64,
    /// Σ |Known| across observers (pre-6G.7B scan volume).
    pub known_relationships_present_total: u64,
    /// Relationships actually examined for update discovery.
    pub known_relationships_scanned_total: u64,
    pub interested_observers_total: u64,
    pub updates_emitted_total: u64,
    pub serialize_attempts_total: u64,
    pub budget_deferred_total: u64,
    pub cadence_deferred_total: u64,
    pub recovery_rescues_total: u64,
    pub scanned_per_pass: IntDistribution,
    pub known_present_per_pass: IntDistribution,
    /// 6G.7C policy metrics (schema 2).
    #[serde(default)]
    pub policy_eligible_total: u64,
    #[serde(default)]
    pub policy_domain_suppressed_total: u64,
    #[serde(default)]
    pub priority_deferred_total: u64,
    #[serde(default)]
    pub state_coalesced_total: u64,
    #[serde(default)]
    pub bytes_emitted_total: u64,
}

impl ReplicationFanoutSnapshot {
    pub const SCHEMA: u32 = 2;

    /// Pre-fanout style ratio: present Known / emitted updates (∞-like → large if 0 emits).
    #[must_use]
    pub fn present_per_update(&self) -> f64 {
        if self.updates_emitted_total == 0 {
            0.0
        } else {
            self.known_relationships_present_total as f64 / self.updates_emitted_total as f64
        }
    }

    /// Actual discovery ratio: scanned / emitted.
    #[must_use]
    pub fn scanned_per_update(&self) -> f64 {
        if self.updates_emitted_total == 0 {
            0.0
        } else {
            self.known_relationships_scanned_total as f64 / self.updates_emitted_total as f64
        }
    }

    /// scanned / dirty entities (0 if no dirty).
    #[must_use]
    pub fn scanned_per_dirty_entity(&self) -> f64 {
        if self.dirty_entities_total == 0 {
            0.0
        } else {
            self.known_relationships_scanned_total as f64 / self.dirty_entities_total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_snapshot_roundtrips_json() {
        let snap = TickDomainSnapshot {
            schema: TickDomainSnapshot::SCHEMA,
            tick_count: 10,
            tick_total: DomainTimingMs {
                mean_ms: 1.0,
                p50_ms: 1.0,
                p95_ms: 2.0,
                p99_ms: 3.0,
                max_ms: 4.0,
                sample_count: 10,
            },
            ..TickDomainSnapshot::default()
        };
        let s = serde_json::to_string(&snap).unwrap();
        let back: TickDomainSnapshot = serde_json::from_str(&s).unwrap();
        assert_eq!(back.tick_count, 10);
        assert_eq!(back.tick_total.p99_ms, 3.0);
    }
}
