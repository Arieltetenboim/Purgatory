//! Shared primitives for PURGATORY client, server, and tools.

pub mod capacity_accounting;
pub mod identity;
pub mod impairment;
pub mod load_metrics;
pub mod load_validation;
pub mod memory;
pub mod world_address;

pub use capacity_accounting::{
    CAPACITY_ARTIFACT_DIR_ENV, CAPACITY_DETAIL_ENV, CAPACITY_TICK_BUDGET_NS, CapacityLiveSnapshot,
    ClientPressureRow, ConnectionLifecycleSnapshot, ConnectionRampSnapshot, DomainTimingMs,
    FunnelInvariantReport, GameplayWorkloadSnapshot, HarnessConnectionSnapshot, IntDistribution,
    InterestLocalitySnapshot, NetworkPressureSnapshot, OwnerShareRow, ProcessResourceSnapshot,
    RampFunnel, RemainderResult, ReplicationFanoutSnapshot, SaturationClass, SaturationEvidence,
    TICK_DOMAIN_WINDOW_SAMPLES, TickDomainSnapshot, TickLeafMicros, TickOwnerId,
    capacity_detail_enabled, check_ramp_funnel_invariant, classify_saturation,
    compose_ramp_ownership, dominant_owner_from_means, int_distribution_from_micros,
    ramp_attainment_pct, worst_spike_owner_from_max,
};
pub use identity::{
    AuthoredIdError, CONTENT_ABILITY_END, CONTENT_ABILITY_START, CONTENT_ID_BLOCK_SIZE,
    CONTENT_ITEM_END, CONTENT_ITEM_START, CONTENT_MAP_END, CONTENT_MAP_START,
    CONTENT_MONSTER_END, CONTENT_MONSTER_START, CONTENT_NPC_END, CONTENT_NPC_START,
    CONTENT_WORLD_OBJECT_END, CONTENT_WORLD_OBJECT_START, CharacterId, ContentId, ContentKind,
    DEFAULT_DEV_LOGIN, DEFAULT_RESTORE_POINT, DEV_LOGIN_MAX_LEN, DEV_LOGIN_MIN_LEN, DevLogin,
    DevLoginError, InstanceExitContext, ItemInstanceId, MAP_FOOTNOTE_AUTHORED,
    MAP_SECOND_AUTHORED, MAX_AUTHORED_CONTENT_ID_LEN, PersistentId, RestoreIntent, fnv1a64,
    validate_authored_id,
};
pub use load_metrics::{
    DEFAULT_METRICS_PORT, LOAD_METRICS_SCHEMA_VERSION, LoadMetricsV1, METRICS_MAX_DATAGRAM_BYTES,
    METRICS_MAX_REQUEST_BYTES, METRICS_REQUEST_MAGIC, METRICS_REQUEST_VERSION,
    decode_metrics_response, encode_metrics_response, is_metrics_request, metrics_request_datagram,
};
pub use load_validation::{
    LOAD_MODE_ADMISSION_ENV, LOAD_VALIDATION_ENV, LoadValidationConfig, NpcWorkloadConfig,
    SchedulerPressure, SpawnPressure,
};
pub use memory::{
    ProcessCpu, ProcessMemory, ProcessResources, current_process_memory, current_process_resources,
    logical_cpu_count,
};
pub use world_address::{ChannelId, InstanceId, MapId, WorldAddress};

/// Cargo package version for this crate.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Current master-plan phase. Source of truth: repo-root `PHASE`.
pub fn phase() -> &'static str {
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../PHASE")).trim()
}

/// Compact label for logs, window titles, and the dev launcher.
#[must_use]
pub fn identity() -> String {
    format!("v{}  phase {}", version(), phase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_nonempty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn phase_is_nonempty() {
        assert!(!phase().is_empty());
        assert!(
            phase()
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.'),
            "PHASE must be a simple token like 5.4"
        );
    }
}
