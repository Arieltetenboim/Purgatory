//! Per-tick runtime service stats. Low cardinality only.

/// Snapshot of 6F / 7.2 runtime counters. Not keyed by CharacterId/EntityId.
///
/// Queue-depth / active-count fields are **gauges** sampled after the tick
/// (or at `begin_tick`). Monotonic `*_total` fields prove work ran even when
/// a 1 Hz poll misses a short-lived gauge spike.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeStats {
    pub scheduler_queued: u32,
    pub scheduler_due_critical: u32,
    pub scheduler_due_deferred: u32,
    pub scheduler_critical_fired: u32,
    pub scheduler_deferred_fired: u32,
    pub scheduler_critical_ceiling_hits: u64,
    pub scheduler_deferred_exhausted: u64,
    pub scheduler_scheduled_total: u64,
    pub scheduler_cancelled_total: u64,
    pub scheduler_critical_executed_total: u64,
    pub scheduler_deferred_executed_total: u64,
    pub actions_active: u32,
    pub actions_started_total: u64,
    pub actions_completed_total: u64,
    pub actions_attempted_total: u64,
    pub actions_rejected_total: u64,
    pub effects_active: u32,
    pub effects_applied_total: u64,
    pub effects_expired_total: u64,
    pub events_produced: u64,
    pub events_processed: u64,
    pub spawn_queue_depth: u32,
    pub spawn_requests_total: u64,
    pub spawns_completed_total: u64,
    pub despawns_completed_total: u64,
    pub cadence_due: u32,
    pub cadence_executions_total: u64,
    pub entities_spawned_total: u64,
    pub command_rejects_gate: u64,
    pub command_rejects_other: u64,
    pub domain_rev_advances: u64,
    pub npcs_active: u32,
    pub npc_updates_total: u64,
    pub health_mutations_total: u64,
    pub deaths_total: u64,
    pub respawns_total: u64,
    pub pulse_ticks_total: u64,
}
