//! Per-tick runtime service stats. Low cardinality only.

/// Snapshot of 6F runtime counters. Not keyed by CharacterId/EntityId.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeStats {
    pub scheduler_queued: u32,
    pub scheduler_due_critical: u32,
    pub scheduler_due_deferred: u32,
    pub scheduler_critical_fired: u32,
    pub scheduler_deferred_fired: u32,
    pub scheduler_critical_ceiling_hits: u64,
    pub scheduler_deferred_exhausted: u64,
    pub actions_active: u32,
    pub effects_active: u32,
    pub events_produced: u64,
    pub events_processed: u64,
    pub spawn_queue_depth: u32,
    pub cadence_due: u32,
    pub command_rejects_gate: u64,
    pub command_rejects_other: u64,
    pub domain_rev_advances: u64,
}
