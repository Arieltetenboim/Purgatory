//! Capacity characterization artifacts (Phase 6G.2, extended in Phase 7.1).
//!
//! Coarse per-domain tick timings and process resources written to run
//! directories. **Not** part of the live UDP `PURGSTAT` contract (adopted
//! schema 3; domain timings stay in these files). Set
//! `PURGATORY_CAPACITY_ARTIFACT_DIR` on the server to enable file export.
//!
//! Timing hierarchy: parent rollups (`gameplay_services`, `replication`) are
//! **not** added into `unattributed = total - attributed`. Attributed uses a
//! non-overlapping leaf set. Percentiles for every owner use the same window
//! (last [`TICK_DOMAIN_WINDOW_SAMPLES`] ticks at ~1 Hz flush).

use serde::{Deserialize, Serialize};

/// Env var: directory where the server writes capacity JSON/NDJSON.
pub const CAPACITY_ARTIFACT_DIR_ENV: &str = "PURGATORY_CAPACITY_ARTIFACT_DIR";

/// `0`/`false`/`off` = coarse 6G.2 leaves only. Unset or `1` = child owners.
pub const CAPACITY_DETAIL_ENV: &str = "PURGATORY_CAPACITY_DETAIL";

/// Percentile / dominant-owner window: last N completed ticks (matches the ring).
pub const TICK_DOMAIN_WINDOW_SAMPLES: u32 = 120;

/// Nominal 30 Hz tick spacing in nanoseconds (`1_000_000_000 / 30`). Accounting
/// only — not a production SLA.
pub const CAPACITY_TICK_BUDGET_NS: u64 = 1_000_000_000 / 30;

/// Whether child (detail) owners should be timed.
#[must_use]
pub fn capacity_detail_enabled() -> bool {
    match std::env::var(CAPACITY_DETAIL_ENV) {
        Ok(v) => {
            let t = v.trim();
            !(t == "0" || t.eq_ignore_ascii_case("false") || t.eq_ignore_ascii_case("off"))
        }
        Err(_) => true,
    }
}

/// Saturation class. Never force a named class when evidence is insufficient.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaturationClass {
    /// Evidence does not assign simulation, transport, or harness ownership.
    #[default]
    UnknownUnattributed,
    SimulationTick,
    ServerTransportBackpressure,
    HarnessClient,
}

/// Leaf or parent owner id for dominant-owner reporting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TickOwnerId {
    CommandsInput,
    SimulationMovement,
    NpcActivity,
    GameplayServices,
    Scheduler,
    Actions,
    Effects,
    EntityLifecycle,
    Cadence,
    SpatialAoi,
    Replication,
    ReplicationDiscover,
    ReplicationPolicy,
    ReplicationEncode,
    ReplicationEnqueue,
    PersistenceEnqueue,
    Unattributed,
}

impl TickOwnerId {
    /// True for owners that live on the authoritative tick path (not remainder).
    #[must_use]
    pub fn is_simulation_tick_owner(self) -> bool {
        !matches!(self, Self::Unattributed)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CommandsInput => "commands_input",
            Self::SimulationMovement => "simulation_movement",
            Self::NpcActivity => "npc_activity",
            Self::GameplayServices => "gameplay_services",
            Self::Scheduler => "scheduler",
            Self::Actions => "actions",
            Self::Effects => "effects",
            Self::EntityLifecycle => "entity_lifecycle",
            Self::Cadence => "cadence",
            Self::SpatialAoi => "spatial_aoi",
            Self::Replication => "replication",
            Self::ReplicationDiscover => "replication_discover",
            Self::ReplicationPolicy => "replication_policy",
            Self::ReplicationEncode => "replication_encode",
            Self::ReplicationEnqueue => "replication_enqueue",
            Self::PersistenceEnqueue => "persistence_enqueue",
            Self::Unattributed => "unattributed",
        }
    }
}

/// Non-overlapping leaf micros used for remainder. Parents are excluded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TickLeafMicros {
    pub commands_input: u64,
    pub simulation_movement: u64,
    pub npc_activity: u64,
    pub gameplay_services: u64,
    pub scheduler: u64,
    pub actions: u64,
    pub effects: u64,
    pub entity_lifecycle: u64,
    pub cadence: u64,
    pub spatial_aoi: u64,
    pub replication: u64,
    pub replication_discover: u64,
    pub replication_policy: u64,
    pub replication_encode: u64,
    pub replication_enqueue: u64,
    pub persistence_enqueue: u64,
}

impl TickLeafMicros {
    /// Sum of the non-overlapping leaf set for this detail mode.
    #[must_use]
    pub fn attributed(self, detail: bool) -> u64 {
        let base = self
            .commands_input
            .saturating_add(self.simulation_movement)
            .saturating_add(self.npc_activity)
            .saturating_add(self.spatial_aoi)
            .saturating_add(self.persistence_enqueue);
        if detail {
            base.saturating_add(self.scheduler)
                .saturating_add(self.actions)
                .saturating_add(self.effects)
                .saturating_add(self.entity_lifecycle)
                .saturating_add(self.cadence)
                .saturating_add(self.replication_discover)
                .saturating_add(self.replication_policy)
                .saturating_add(self.replication_encode)
                .saturating_add(self.replication_enqueue)
        } else {
            base.saturating_add(self.gameplay_services)
                .saturating_add(self.replication)
        }
    }

    /// `unattributed = total - attributed`. Negative remainder is clamped and flagged.
    #[must_use]
    pub fn remainder(self, total: u64, detail: bool) -> RemainderResult {
        let attributed = self.attributed(detail);
        if attributed > total {
            RemainderResult {
                unattributed_us: 0,
                attributed_us: attributed,
                accounting_error: true,
            }
        } else {
            RemainderResult {
                unattributed_us: total - attributed,
                attributed_us: attributed,
                accounting_error: false,
            }
        }
    }

    #[must_use]
    pub fn leaf_pairs(self, detail: bool, unattributed_us: u64) -> Vec<(TickOwnerId, u64)> {
        let mut v = vec![
            (TickOwnerId::CommandsInput, self.commands_input),
            (TickOwnerId::SimulationMovement, self.simulation_movement),
            (TickOwnerId::NpcActivity, self.npc_activity),
            (TickOwnerId::SpatialAoi, self.spatial_aoi),
            (TickOwnerId::PersistenceEnqueue, self.persistence_enqueue),
            (TickOwnerId::Unattributed, unattributed_us),
        ];
        if detail {
            v.extend([
                (TickOwnerId::Scheduler, self.scheduler),
                (TickOwnerId::Actions, self.actions),
                (TickOwnerId::Effects, self.effects),
                (TickOwnerId::EntityLifecycle, self.entity_lifecycle),
                (TickOwnerId::Cadence, self.cadence),
                (TickOwnerId::ReplicationDiscover, self.replication_discover),
                (TickOwnerId::ReplicationPolicy, self.replication_policy),
                (TickOwnerId::ReplicationEncode, self.replication_encode),
                (TickOwnerId::ReplicationEnqueue, self.replication_enqueue),
            ]);
        } else {
            v.extend([
                (TickOwnerId::GameplayServices, self.gameplay_services),
                (TickOwnerId::Replication, self.replication),
            ]);
        }
        v
    }
}

/// Result of remainder accounting for one tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RemainderResult {
    pub unattributed_us: u64,
    pub attributed_us: u64,
    pub accounting_error: bool,
}

/// Dominant owner from **mean** share over the window (not a single max spike).
#[must_use]
pub fn dominant_owner_from_means(pairs: &[(TickOwnerId, f64)]) -> Option<TickOwnerId> {
    pairs
        .iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .filter(|(_, share)| *share > 0.0)
        .map(|(id, _)| *id)
}

/// Worst-spike owner from **max** micros in the same window.
#[must_use]
pub fn worst_spike_owner_from_max(pairs: &[(TickOwnerId, f64)]) -> Option<TickOwnerId> {
    dominant_owner_from_means(pairs)
}

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

/// Snapshot of coarse tick-domain accounting (file schema 2 in 7.1).
///
/// All `*_ms` percentiles use the same window: last
/// [`TICK_DOMAIN_WINDOW_SAMPLES`] ticks (or fewer at start of run).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TickDomainSnapshot {
    pub schema: u32,
    pub wall_secs: f64,
    pub tick_count: u64,
    pub tick_overrun_count: u64,
    #[serde(default)]
    pub window_samples: u32,
    #[serde(default)]
    pub detail_enabled: bool,
    #[serde(default)]
    pub tick_budget_ms: f64,
    #[serde(default)]
    pub tick_utilization_pct: f64,
    #[serde(default)]
    pub tick_remaining_mean_ms: f64,
    #[serde(default)]
    pub consecutive_overrun_streak: u64,
    #[serde(default)]
    pub max_overrun_ms: f64,
    #[serde(default)]
    pub achieved_tick_hz: f64,
    #[serde(default)]
    pub accounting_error_ticks: u64,
    #[serde(default)]
    pub dominant_owner: Option<TickOwnerId>,
    #[serde(default)]
    pub worst_spike_owner: Option<TickOwnerId>,
    pub tick_total: DomainTimingMs,
    pub commands_input: DomainTimingMs,
    pub simulation_movement: DomainTimingMs,
    #[serde(default)]
    pub npc_activity: DomainTimingMs,
    pub gameplay_services: DomainTimingMs,
    pub spatial_aoi: DomainTimingMs,
    pub replication: DomainTimingMs,
    pub persistence_enqueue: DomainTimingMs,
    #[serde(default)]
    pub scheduler: DomainTimingMs,
    #[serde(default)]
    pub actions: DomainTimingMs,
    #[serde(default)]
    pub effects: DomainTimingMs,
    #[serde(default)]
    pub entity_lifecycle: DomainTimingMs,
    #[serde(default)]
    pub cadence: DomainTimingMs,
    #[serde(default)]
    pub replication_discover: DomainTimingMs,
    #[serde(default)]
    pub replication_policy: DomainTimingMs,
    #[serde(default)]
    pub replication_encode: DomainTimingMs,
    #[serde(default)]
    pub replication_enqueue: DomainTimingMs,
    #[serde(default)]
    pub unattributed: DomainTimingMs,
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
    /// Raw process CPU vs wall: **100 = one logical core fully busy**.
    /// May exceed 100 on multi-core. Not machine-relative.
    pub cpu_utilization_pct: f64,
    /// `cpu_utilization_pct / logical_cpus`: **100 = all logical cores busy**.
    #[serde(default)]
    pub cpu_normalized_per_logical_pct: f64,
    /// Peak of `cpu_utilization_pct` over the run.
    #[serde(default)]
    pub cpu_utilization_peak_pct: f64,
    pub cpu_time_delta_secs: f64,
    pub sample_interval_secs: f64,
    /// Working set minus start-of-run sample.
    #[serde(default)]
    pub working_set_delta_bytes: i64,
    #[serde(default)]
    pub thread_count: Option<u32>,
    #[serde(default)]
    pub handle_count: Option<u32>,
}

impl TickDomainSnapshot {
    pub const SCHEMA: u32 = 2;
}

impl ProcessResourceSnapshot {
    pub const SCHEMA: u32 = 2;
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

/// Evidence for [`classify_saturation`]. Missing or mixed evidence stays unknown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SaturationEvidence {
    pub consecutive_overrun_streak: u64,
    pub tick_utilization_pct: f64,
    pub dominant_owner: Option<TickOwnerId>,
    pub writer_queue_push_fail_total: u64,
    pub writer_queue_depth_max: u64,
    pub writer_queue_cap: u64,
    /// Write/drain/backpressure p99 in milliseconds — **not** QUIC CPU/send cost.
    pub write_drain_p99_ms: f64,
    pub tick_budget_ms: f64,
    pub admission_refused_total: u64,
    pub connected_sessions: u64,
    pub admission_cap: u64,
    pub harness_timeouts: u64,
    pub harness_snapshot_starvation_samples: u64,
}

/// Conservative saturation classification. Never force a named class.
#[must_use]
pub fn classify_saturation(e: &SaturationEvidence) -> SaturationClass {
    let sim = evidence_simulation_tick(e);
    let transport = evidence_server_transport(e);
    let harness = evidence_harness_client(e);
    match (sim, transport, harness) {
        (true, false, false) => SaturationClass::SimulationTick,
        (false, true, false) => SaturationClass::ServerTransportBackpressure,
        (false, false, true) => SaturationClass::HarnessClient,
        _ => SaturationClass::UnknownUnattributed,
    }
}

fn evidence_simulation_tick(e: &SaturationEvidence) -> bool {
    e.consecutive_overrun_streak >= 3
        && e.tick_utilization_pct >= 100.0
        && e.dominant_owner
            .is_some_and(TickOwnerId::is_simulation_tick_owner)
        && !queue_at_cap(e)
        && e.writer_queue_push_fail_total == 0
}

fn evidence_server_transport(e: &SaturationEvidence) -> bool {
    let drain_beyond_tick = e.tick_budget_ms > 0.0 && e.write_drain_p99_ms > e.tick_budget_ms;
    let queue_pressure = e.writer_queue_push_fail_total > 0 || queue_at_cap(e) || drain_beyond_tick;
    queue_pressure && e.consecutive_overrun_streak == 0 && e.tick_utilization_pct < 100.0
}

fn evidence_harness_client(e: &SaturationEvidence) -> bool {
    let at_admission_wall = e.admission_cap > 0
        && e.admission_refused_total > 0
        && e.connected_sessions >= e.admission_cap;
    let harness_fail = e.harness_timeouts > 0 || e.harness_snapshot_starvation_samples > 0;
    (at_admission_wall || harness_fail)
        && e.consecutive_overrun_streak == 0
        && e.writer_queue_push_fail_total == 0
        && !queue_at_cap(e)
}

fn queue_at_cap(e: &SaturationEvidence) -> bool {
    e.writer_queue_cap > 0 && e.writer_queue_depth_max >= e.writer_queue_cap
}

/// Percentile summary from a non-negative sample ring (same contract as tick owners).
#[must_use]
pub fn int_distribution_from_micros(samples: &[u64]) -> IntDistribution {
    if samples.is_empty() {
        return IntDistribution::default();
    }
    let mut us = samples.to_vec();
    us.sort_unstable();
    let n = us.len();
    let mean = us.iter().sum::<u64>() as f64 / n as f64;
    let idx = |p: f64| -> u64 {
        let i = ((n as f64 - 1.0) * p).round() as usize;
        us[i.min(n - 1)]
    };
    IntDistribution {
        mean,
        p50: idx(0.50) as f64,
        p95: idx(0.95) as f64,
        p99: idx(0.99) as f64,
        max: *us.last().unwrap(),
        sample_count: n as u64,
    }
}

/// Per-client pressure row for the top-N artifact (not a Hub wall).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientPressureRow {
    pub connection_id: u64,
    pub queue_depth: u32,
    pub write_drain_max_us: u64,
    pub write_drain_last_us: u64,
    pub queue_age_max_us: u64,
    pub bytes_out: u64,
    pub enqueue_fails: u64,
}

/// Network / queue / write-drain pressure (file schema 2). `write_drain_*` is
/// write/drain/backpressure latency, **not** QUIC CPU/send cost.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NetworkPressureSnapshot {
    pub schema: u32,
    pub wall_secs: f64,
    pub bytes_in_per_sec: f64,
    pub bytes_out_per_sec: f64,
    pub bytes_out_per_session_per_sec: f64,
    pub connected_sessions: u64,
    pub writer_queue_cap: u64,
    pub writer_queue_depth_max: u64,
    pub writer_queue_push_fail_total: u64,
    pub write_drain: IntDistribution,
    pub queue_age_us: IntDistribution,
    pub top_clients: Vec<ClientPressureRow>,
    /// Publish attempts that reached the outbound path (one per observer/tick intent).
    #[serde(default)]
    pub frames_publish_attempt_total: u64,
    /// Frames that completed packer encode (payload built).
    #[serde(default)]
    pub frames_encoded_total: u64,
    /// Frames accepted into the per-client writer queue.
    #[serde(default)]
    pub frames_enqueued_total: u64,
    /// `try_push` attempts (success + fail).
    #[serde(default)]
    pub enqueue_attempts_total: u64,
    /// Frames drained by the connection writer (`write_all` completed).
    #[serde(default)]
    pub frames_drained_total: u64,
    /// Encoded gameplay payload bytes (pre-framing) accepted for enqueue.
    #[serde(default)]
    pub bytes_encoded_total: u64,
    /// Bytes actually written on the uni stream (framed).
    #[serde(default)]
    pub bytes_drained_total: u64,
    /// `write_all` calls that returned Ok.
    #[serde(default)]
    pub write_calls_total: u64,
    /// Sum of current per-client queue depths at last snapshot (approx backlog frames).
    #[serde(default)]
    pub queued_frames_current: u64,
    #[serde(default)]
    pub note: String,
}

impl NetworkPressureSnapshot {
    pub const SCHEMA: u32 = 2;
    pub const NOTE: &'static str = "schema 2: outbound path produced→encoded→queued→drained. write_drain is write/drain/backpressure latency, not QUIC CPU.";
}

/// Server-visible connection-stage timings. Client/harness owns connect-attempt
/// start; this snapshot does **not** invent a server timestamp for that stage.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ConnectionLifecycleSnapshot {
    pub schema: u32,
    pub wall_secs: f64,
    pub note: String,
    pub transport_accept_ok: u64,
    pub hello_ok: u64,
    pub welcome_ok: u64,
    pub session_accepted: u64,
    pub fail_transport_accept: u64,
    pub fail_hello: u64,
    pub fail_welcome: u64,
    pub fail_enter: u64,
    pub disconnects: u64,
    /// Accept → Hello (server-visible), microseconds.
    pub accept_to_hello_us: IntDistribution,
    /// Hello → Welcome write, microseconds.
    pub hello_to_welcome_us: IntDistribution,
    /// Welcome → session insert, microseconds.
    pub welcome_to_session_us: IntDistribution,
    /// Session insert → disconnect, microseconds.
    pub session_to_disconnect_us: IntDistribution,
}

impl ConnectionLifecycleSnapshot {
    pub const SCHEMA: u32 = 1;
    pub const NOTE: &'static str = "client/harness owns connect-attempt start; server timing begins at transport accept/ready/Hello. Correlate via wall_secs in the shared run directory.";
}

/// Harness-owned connect-attempt histograms (not server-observed).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HarnessConnectionSnapshot {
    pub schema: u32,
    pub wall_secs: f64,
    pub note: String,
    pub connect_attempts: u64,
    pub connect_ok: u64,
    pub connect_fail: u64,
    #[serde(default)]
    pub harness_timeouts: u64,
    /// Connect-attempt start → QUIC ready (client-owned).
    pub attempt_to_quic_ready_us: IntDistribution,
    /// QUIC ready → Welcome (client-owned handshake).
    pub quic_ready_to_welcome_us: IntDistribution,
    /// Connect-attempt start → Welcome.
    pub attempt_to_welcome_us: IntDistribution,
}

impl HarnessConnectionSnapshot {
    pub const SCHEMA: u32 = 1;
    pub const NOTE: &'static str = "harness owns connect-attempt start. Server-visible stages live in connection_lifecycle.json. Correlate via wall_secs.";
}

/// Funnel inputs for [`compose_ramp_ownership`].
///
/// **Issuance funnel** (harness-owned, single run):
/// `requested → spawn_issued → connect_attempts_completed → transport → welcome → world_entered`
/// with `peak_active` as a concurrent high-water (not a cumulative stage).
///
/// Downstream issuance-funnel counts must not exceed `spawn_issued_total` for that run.
/// Server lifecycle totals and reconnects are correlators — do not `.max()` them into
/// the issuance stages (that produced transport/Welcome > issued in 7.1F / 7.2 mixed).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RampFunnel {
    pub requested_clients: u64,
    pub spawn_issued_total: u64,
    pub connect_attempts_completed: u64,
    pub transport_established_total: u64,
    pub welcome_total: u64,
    pub world_entered_total: u64,
    pub peak_active_clients: u64,
    pub connect_fail_total: u64,
    pub harness_timeouts: u64,
    pub admission_refused: u64,
    pub admission_cap: u64,
    pub disconnects: u64,
    /// Successful harness reconnects (not counted in `spawn_issued_total`).
    pub reconnect_ok_total: u64,
    /// Server lifecycle correlators (may include reconnects / probes).
    pub server_transport_accept_ok: u64,
    pub server_welcome_ok: u64,
    pub server_session_accepted: u64,
}

/// Combined ramp funnel (file schema 2). Harness exit 0 does not prove attainment.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ConnectionRampSnapshot {
    pub schema: u32,
    pub wall_secs: f64,
    pub note: String,
    pub harness_exit_is_not_attainment: bool,
    pub requested_clients: u64,
    /// Harness `queue_bot_connect` / `spawn_bot` count for this run (initial ramp).
    pub spawn_issued_total: u64,
    pub connect_attempts_completed: u64,
    pub peak_connection_attempts: u64,
    /// Harness QUIC-ready completions for issuance funnel (not server accept max).
    pub transport_established_total: u64,
    pub peak_transport_established: u64,
    /// Harness Welcome-ok completions for issuance funnel.
    pub welcome_total: u64,
    /// Harness world-entered proxy for issuance funnel (`connect_ok`).
    pub world_entered_total: u64,
    pub peak_world_entered: u64,
    pub peak_active_clients: u64,
    pub attainment_pct: f64,
    pub connect_fail_total: u64,
    pub harness_timeouts: u64,
    pub admission_refused: u64,
    pub admission_cap: u64,
    pub disconnects: u64,
    pub controller_ticks: u64,
    pub pending_in_flight: u64,
    /// Reconnect / churn connects that hit the server but are not `spawn_issued`.
    #[serde(default)]
    pub reconnect_ok_total: u64,
    /// Server `connection_lifecycle.json` correlators (include reconnects).
    #[serde(default)]
    pub server_transport_accept_ok: u64,
    #[serde(default)]
    pub server_welcome_ok: u64,
    #[serde(default)]
    pub server_session_accepted: u64,
    /// True when issuance funnel is monotone and server excess is explained by reconnects.
    #[serde(default)]
    pub funnel_invariant_ok: bool,
    #[serde(default)]
    pub funnel_invariant_note: String,
    /// Controller tick wall time (ms) p50 over the run window samples.
    #[serde(default)]
    pub controller_tick_p50_ms: f64,
    #[serde(default)]
    pub controller_tick_p99_ms: f64,
    /// Time spent in `tick_all_bots` (ms) p50/p99.
    #[serde(default)]
    pub tick_all_bots_p50_ms: f64,
    #[serde(default)]
    pub tick_all_bots_p99_ms: f64,
    /// Connection issues beyond the first in a catch-up burst (schedule backlog).
    #[serde(default)]
    pub spawn_catchup_issued_total: u64,
    /// Max due connection issues enqueued in one controller iteration.
    #[serde(default)]
    pub spawn_due_peak: u64,
    pub attempt_to_quic_ready_us: IntDistribution,
    pub quic_ready_to_welcome_us: IntDistribution,
    pub attempt_to_welcome_us: IntDistribution,
    pub accept_to_hello_us: IntDistribution,
    pub hello_to_welcome_us: IntDistribution,
    pub server_cpu_utilization_pct: f64,
    pub server_cpu_normalized_per_logical_pct: f64,
    pub harness_cpu_utilization_pct: f64,
    pub harness_cpu_normalized_per_logical_pct: f64,
    pub server_tick_p99_ms: f64,
    pub server_tick_utilization_pct: f64,
    pub writer_queue_depth_max: u64,
    pub writer_queue_push_fail_total: u64,
    pub write_drain_p99_ms: f64,
    pub saturation_class: SaturationClass,
    pub ownership_statement: String,
}

impl ConnectionRampSnapshot {
    pub const SCHEMA: u32 = 2;
    pub const NOTE: &'static str = "schema 2: issuance funnel is harness-owned (requested→spawn_issued→attempts→transport→Welcome→entered). Server lifecycle totals are correlators only — they include reconnects/probes and must not be max-mixed into issuance stages. Harness exit 0 is not attainment.";
}

/// Result of [`check_ramp_funnel_invariant`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunnelInvariantReport {
    pub ok: bool,
    pub note: String,
}

/// Issuance-funnel monotone check + explained server excess.
///
/// Server accept/welcome/session may exceed `spawn_issued` only by
/// `reconnect_ok_total` (and small pending drain slack). Unexplained excess is
/// annotated; do not treat it as capacity attainment.
#[must_use]
pub fn check_ramp_funnel_invariant(
    f: &RampFunnel,
    pending_in_flight: u64,
) -> FunnelInvariantReport {
    let mut problems = Vec::new();
    if f.transport_established_total > f.spawn_issued_total {
        problems.push(format!(
            "transport {} > spawn_issued {}",
            f.transport_established_total, f.spawn_issued_total
        ));
    }
    if f.welcome_total > f.spawn_issued_total {
        problems.push(format!(
            "welcome {} > spawn_issued {}",
            f.welcome_total, f.spawn_issued_total
        ));
    }
    if f.world_entered_total > f.spawn_issued_total {
        problems.push(format!(
            "world_entered {} > spawn_issued {}",
            f.world_entered_total, f.spawn_issued_total
        ));
    }
    if f.connect_attempts_completed > f.spawn_issued_total {
        problems.push(format!(
            "attempts {} > spawn_issued {}",
            f.connect_attempts_completed, f.spawn_issued_total
        ));
    }
    if f.welcome_total
        > f.connect_attempts_completed
            .saturating_add(pending_in_flight)
    {
        problems.push(format!(
            "welcome {} > attempts {} + pending {}",
            f.welcome_total, f.connect_attempts_completed, pending_in_flight
        ));
    }
    let explained_cap = f
        .spawn_issued_total
        .saturating_add(f.reconnect_ok_total)
        .saturating_add(pending_in_flight);
    let server_peak = f
        .server_transport_accept_ok
        .max(f.server_welcome_ok)
        .max(f.server_session_accepted);
    if server_peak > explained_cap {
        problems.push(format!(
            "server stage {} > spawn_issued {} + reconnect_ok {} + pending {} (unexplained excess {})",
            server_peak,
            f.spawn_issued_total,
            f.reconnect_ok_total,
            pending_in_flight,
            server_peak.saturating_sub(explained_cap)
        ));
    }
    if problems.is_empty() {
        let note = if server_peak > f.spawn_issued_total {
            format!(
                "issuance funnel monotone; server correlator {} exceeds spawn_issued {} by reconnect_ok {} (expected).",
                server_peak, f.spawn_issued_total, f.reconnect_ok_total
            )
        } else {
            "issuance funnel monotone; server correlators within spawn_issued (+pending)."
                .to_string()
        };
        FunnelInvariantReport { ok: true, note }
    } else {
        FunnelInvariantReport {
            ok: false,
            note: format!(
                "funnel invariant violated: {}. Do not use mixed server/harness max as capacity evidence.",
                problems.join("; ")
            ),
        }
    }
}

/// `peak_active / requested * 100`. Zero requested → 0.
#[must_use]
pub fn ramp_attainment_pct(peak_active: u64, requested: u64) -> f64 {
    if requested == 0 {
        0.0
    } else {
        (peak_active as f64 / requested as f64) * 100.0
    }
}

fn ramp_stage_drop(prev: u64, next: u64) -> bool {
    if prev == 0 {
        return false;
    }
    let floor = prev.saturating_mul(90) / 100;
    next + 8 < prev && next < floor
}

/// Exclusive first-drop ownership sentence. Mixed or small gaps stay unresolved.
#[must_use]
pub fn compose_ramp_ownership(f: &RampFunnel) -> String {
    let req = f.requested_clients;
    if req == 0 {
        return "no clients requested; ramp attribution does not apply.".to_string();
    }
    if f.admission_cap > 0
        && f.admission_refused > 0
        && f.peak_active_clients >= f.admission_cap
        && !ramp_stage_drop(req, f.spawn_issued_total)
    {
        return format!(
            "{} were requested; peak active {} hit admission cap {} with {} refusals (harness/admission wall). This is not {}-server capacity.",
            req, f.peak_active_clients, f.admission_cap, f.admission_refused, req
        );
    }
    if ramp_stage_drop(req, f.spawn_issued_total) {
        return format!(
            "{} were requested, but the harness only created {} connection attempts within the run window.",
            req, f.spawn_issued_total
        );
    }
    if ramp_stage_drop(f.spawn_issued_total, f.connect_attempts_completed) {
        return format!(
            "{} connection attempts were issued, but only {} finished connect (in-flight or still handshaking at window end; fail={} timeout={}).",
            f.spawn_issued_total,
            f.connect_attempts_completed,
            f.connect_fail_total,
            f.harness_timeouts
        );
    }
    if ramp_stage_drop(
        f.connect_attempts_completed.max(f.spawn_issued_total),
        f.transport_established_total,
    ) {
        return format!(
            "{} attempts completed, but only {} reached QUIC transport.",
            f.connect_attempts_completed.max(f.spawn_issued_total),
            f.transport_established_total
        );
    }
    if ramp_stage_drop(f.transport_established_total, f.welcome_total) {
        return format!(
            "{} reached QUIC transport, but only {} reached Welcome.",
            f.transport_established_total, f.welcome_total
        );
    }
    if ramp_stage_drop(f.welcome_total, f.world_entered_total) {
        return format!(
            "{} reached Welcome, but only {} entered the world.",
            f.welcome_total, f.world_entered_total
        );
    }
    if f.world_entered_total + 8 < f.peak_active_clients {
        return format!(
            "{} entered the server successfully, but the harness under-counted them (peak active {}).",
            f.world_entered_total, f.peak_active_clients
        );
    }
    if ramp_stage_drop(f.world_entered_total, f.peak_active_clients) {
        return format!(
            "{} entered the world, but peak active was only {} (disconnects={}).",
            f.world_entered_total, f.peak_active_clients, f.disconnects
        );
    }
    if f.peak_active_clients >= req.saturating_mul(90) / 100 {
        return format!(
            "requested workload attained: peak active {} / requested {} ({:.1}%).",
            f.peak_active_clients,
            req,
            ramp_attainment_pct(f.peak_active_clients, req)
        );
    }
    format!(
        "ramp ownership unknown_unattributed: requested {} issued {} transport {} welcome {} entered {} peak_active {} (attainment {:.1}%).",
        req,
        f.spawn_issued_total,
        f.transport_established_total,
        f.welcome_total,
        f.world_entered_total,
        f.peak_active_clients,
        ramp_attainment_pct(f.peak_active_clients, req)
    )
}

/// Mean-share row for the expanded Hub owner table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OwnerShareRow {
    pub owner: TickOwnerId,
    pub mean_ms: f64,
    pub p99_ms: f64,
    pub share_pct: f64,
}

/// Compact operational view written ~1 Hz (`capacity_live.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CapacityLiveSnapshot {
    pub schema: u32,
    pub available: bool,
    pub wall_secs: f64,
    /// Classification is a heuristic, not a verdict.
    pub classification_is_heuristic: bool,
    pub saturation_class: SaturationClass,
    pub dominant_owner: Option<TickOwnerId>,
    pub worst_spike_owner: Option<TickOwnerId>,
    pub tick_p95_ms: f64,
    pub tick_p99_ms: f64,
    pub tick_max_ms: f64,
    pub tick_utilization_pct: f64,
    pub tick_overrun_count: u64,
    pub consecutive_overrun_streak: u64,
    /// Raw process CPU: **100 = one logical core fully busy**.
    pub cpu_utilization_pct: f64,
    /// Machine-relative: **100 = all logical cores busy**.
    pub cpu_normalized_per_logical_pct: f64,
    pub working_set_bytes: u64,
    pub bytes_in_per_sec: f64,
    pub bytes_out_per_sec: f64,
    pub connected_sessions: u64,
    pub entities: u64,
    pub writer_queue_depth_max: u64,
    pub writer_queue_push_fail_total: u64,
    /// Write/drain/backpressure p99 (ms). Not QUIC CPU/send cost.
    pub write_drain_p99_ms: f64,
    pub unattributed_mean_ms: f64,
    pub accounting_error_ticks: u64,
    pub owners: Vec<OwnerShareRow>,
}

impl CapacityLiveSnapshot {
    pub const SCHEMA: u32 = 1;
}

/// Phase 7.2 representative gameplay counters (file artifact only; not UDP).
///
/// Written ~1 Hz beside `capacity_live.json` when capacity artifacts are enabled.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GameplayWorkloadSnapshot {
    pub schema: u32,
    pub wall_secs: f64,
    pub npcs_active: u32,
    pub npc_updates_total: u64,
    pub actions_attempted_total: u64,
    pub actions_started_total: u64,
    pub actions_completed_total: u64,
    pub actions_rejected_total: u64,
    pub health_mutations_total: u64,
    pub deaths_total: u64,
    pub respawns_total: u64,
    pub pulse_ticks_total: u64,
    pub effects_active: u32,
    pub effects_applied_total: u64,
    pub effects_expired_total: u64,
    pub actions_active: u32,
    pub scheduler_queued: u32,
}

impl GameplayWorkloadSnapshot {
    pub const SCHEMA: u32 = 1;
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
    fn gameplay_workload_snapshot_roundtrips_json() {
        let snap = GameplayWorkloadSnapshot {
            schema: GameplayWorkloadSnapshot::SCHEMA,
            wall_secs: 1.5,
            npcs_active: 3,
            npc_updates_total: 10,
            actions_attempted_total: 4,
            actions_started_total: 2,
            actions_completed_total: 1,
            actions_rejected_total: 1,
            health_mutations_total: 5,
            deaths_total: 1,
            respawns_total: 1,
            pulse_ticks_total: 2,
            effects_active: 1,
            effects_applied_total: 2,
            effects_expired_total: 1,
            actions_active: 1,
            scheduler_queued: 2,
        };
        let s = serde_json::to_string(&snap).unwrap();
        let back: GameplayWorkloadSnapshot = serde_json::from_str(&s).unwrap();
        assert_eq!(back, snap);
    }

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
        assert_eq!(back.schema, 2);
    }

    #[test]
    fn schema1_json_decodes_new_fields_as_default() {
        let old = r#"{"schema":1,"wall_secs":1.0,"tick_count":2,"tick_overrun_count":0,"tick_total":{"mean_ms":1.0,"p50_ms":1.0,"p95_ms":1.0,"p99_ms":1.0,"max_ms":1.0,"sample_count":2},"commands_input":{"mean_ms":0.0,"p50_ms":0.0,"p95_ms":0.0,"p99_ms":0.0,"max_ms":0.0,"sample_count":2},"simulation_movement":{"mean_ms":0.0,"p50_ms":0.0,"p95_ms":0.0,"p99_ms":0.0,"max_ms":0.0,"sample_count":2},"gameplay_services":{"mean_ms":0.0,"p50_ms":0.0,"p95_ms":0.0,"p99_ms":0.0,"max_ms":0.0,"sample_count":2},"spatial_aoi":{"mean_ms":0.0,"p50_ms":0.0,"p95_ms":0.0,"p99_ms":0.0,"max_ms":0.0,"sample_count":2},"replication":{"mean_ms":0.0,"p50_ms":0.0,"p95_ms":0.0,"p99_ms":0.0,"max_ms":0.0,"sample_count":2},"persistence_enqueue":{"mean_ms":0.0,"p50_ms":0.0,"p95_ms":0.0,"p99_ms":0.0,"max_ms":0.0,"sample_count":2}}"#;
        let back: TickDomainSnapshot = serde_json::from_str(old).unwrap();
        assert_eq!(back.tick_count, 2);
        assert_eq!(back.unattributed.sample_count, 0);
        assert!(back.dominant_owner.is_none());
    }

    #[test]
    fn coarse_remainder_excludes_parents_when_detail() {
        let leaves = TickLeafMicros {
            commands_input: 100,
            simulation_movement: 200,
            npc_activity: 0,
            gameplay_services: 9999,
            scheduler: 50,
            actions: 20,
            effects: 10,
            entity_lifecycle: 5,
            cadence: 5,
            spatial_aoi: 300,
            replication: 8888,
            replication_discover: 40,
            replication_policy: 30,
            replication_encode: 20,
            replication_enqueue: 10,
            persistence_enqueue: 10,
        };
        let rem = leaves.remainder(1000, true);
        assert!(!rem.accounting_error);
        assert_eq!(rem.attributed_us, 800);
        assert_eq!(rem.unattributed_us, 200);
    }

    #[test]
    fn coarse_remainder_uses_parent_leaves_when_not_detail() {
        let leaves = TickLeafMicros {
            commands_input: 100,
            simulation_movement: 200,
            gameplay_services: 50,
            spatial_aoi: 300,
            replication: 250,
            persistence_enqueue: 10,
            scheduler: 999,
            ..TickLeafMicros::default()
        };
        let rem = leaves.remainder(1000, false);
        assert!(!rem.accounting_error);
        assert_eq!(rem.attributed_us, 910);
        assert_eq!(rem.unattributed_us, 90);
    }

    #[test]
    fn overlap_flags_accounting_error_and_clamps() {
        let leaves = TickLeafMicros {
            commands_input: 800,
            simulation_movement: 800,
            ..TickLeafMicros::default()
        };
        let rem = leaves.remainder(1000, false);
        assert!(rem.accounting_error);
        assert_eq!(rem.unattributed_us, 0);
        assert_eq!(rem.attributed_us, 1600);
    }

    #[test]
    fn dominant_owner_uses_mean_share_not_spike_label_alone() {
        let means = [
            (TickOwnerId::SpatialAoi, 4.0),
            (TickOwnerId::Replication, 2.0),
            (TickOwnerId::Unattributed, 1.0),
        ];
        assert_eq!(
            dominant_owner_from_means(&means),
            Some(TickOwnerId::SpatialAoi)
        );
        let spikes = [
            (TickOwnerId::SpatialAoi, 5.0),
            (TickOwnerId::Replication, 40.0),
        ];
        assert_eq!(
            worst_spike_owner_from_max(&spikes),
            Some(TickOwnerId::Replication)
        );
    }

    #[test]
    fn saturation_default_is_unknown() {
        assert_eq!(
            SaturationClass::default(),
            SaturationClass::UnknownUnattributed
        );
    }

    #[test]
    fn classify_does_not_force_named_class_on_empty_evidence() {
        assert_eq!(
            classify_saturation(&SaturationEvidence::default()),
            SaturationClass::UnknownUnattributed
        );
    }

    #[test]
    fn classify_simulation_requires_sustained_overrun_and_sim_owner() {
        let e = SaturationEvidence {
            consecutive_overrun_streak: 5,
            tick_utilization_pct: 120.0,
            dominant_owner: Some(TickOwnerId::SpatialAoi),
            tick_budget_ms: 33.33,
            ..SaturationEvidence::default()
        };
        assert_eq!(classify_saturation(&e), SaturationClass::SimulationTick);
    }

    #[test]
    fn classify_mixed_sim_and_queue_stays_unknown() {
        let e = SaturationEvidence {
            consecutive_overrun_streak: 5,
            tick_utilization_pct: 120.0,
            dominant_owner: Some(TickOwnerId::SpatialAoi),
            writer_queue_push_fail_total: 10,
            writer_queue_cap: 4,
            writer_queue_depth_max: 4,
            tick_budget_ms: 33.33,
            ..SaturationEvidence::default()
        };
        assert_eq!(
            classify_saturation(&e),
            SaturationClass::UnknownUnattributed
        );
    }

    #[test]
    fn classify_admission_wall_is_harness_when_tick_healthy() {
        let e = SaturationEvidence {
            admission_refused_total: 80,
            connected_sessions: 256,
            admission_cap: 256,
            tick_utilization_pct: 20.0,
            tick_budget_ms: 33.33,
            ..SaturationEvidence::default()
        };
        assert_eq!(classify_saturation(&e), SaturationClass::HarnessClient);
    }

    #[test]
    fn classify_queue_fail_under_budget_is_transport() {
        let e = SaturationEvidence {
            writer_queue_push_fail_total: 3,
            writer_queue_cap: 4,
            writer_queue_depth_max: 4,
            tick_utilization_pct: 40.0,
            tick_budget_ms: 33.33,
            ..SaturationEvidence::default()
        };
        assert_eq!(
            classify_saturation(&e),
            SaturationClass::ServerTransportBackpressure
        );
    }

    #[test]
    fn ramp_statement_issued_short_of_requested() {
        let f = RampFunnel {
            requested_clients: 384,
            spawn_issued_total: 190,
            ..RampFunnel::default()
        };
        let s = compose_ramp_ownership(&f);
        assert!(s.contains("384 were requested"));
        assert!(s.contains("190 connection attempts"));
    }

    #[test]
    fn ramp_statement_transport_without_welcome() {
        let f = RampFunnel {
            requested_clients: 320,
            spawn_issued_total: 320,
            connect_attempts_completed: 320,
            transport_established_total: 320,
            welcome_total: 185,
            world_entered_total: 185,
            peak_active_clients: 180,
            ..RampFunnel::default()
        };
        let s = compose_ramp_ownership(&f);
        assert!(s.contains("320 reached QUIC transport"), "{s}");
        assert!(s.contains("185 reached Welcome"), "{s}");
    }

    #[test]
    fn ramp_statement_harness_undercount() {
        let f = RampFunnel {
            requested_clients: 256,
            spawn_issued_total: 256,
            connect_attempts_completed: 256,
            transport_established_total: 256,
            welcome_total: 256,
            world_entered_total: 256,
            peak_active_clients: 280,
            ..RampFunnel::default()
        };
        let s = compose_ramp_ownership(&f);
        assert!(s.contains("256 entered the server successfully"));
        assert!(s.contains("under-counted"));
    }

    #[test]
    fn ramp_unknown_when_funnel_does_not_drop() {
        let f = RampFunnel {
            requested_clients: 384,
            spawn_issued_total: 370,
            connect_attempts_completed: 360,
            transport_established_total: 350,
            welcome_total: 340,
            world_entered_total: 330,
            peak_active_clients: 300,
            ..RampFunnel::default()
        };
        let s = compose_ramp_ownership(&f);
        assert!(s.contains("unknown_unattributed"), "{s}");
    }

    #[test]
    fn ramp_attainment_pct_is_peak_over_requested() {
        assert!((ramp_attainment_pct(64, 256) - 25.0).abs() < f64::EPSILON);
        assert_eq!(ramp_attainment_pct(10, 0), 0.0);
    }

    #[test]
    fn funnel_invariant_rejects_server_max_mixed_into_issuance() {
        // Historical bug shape: representative_mixed@8 issued 8 but server Welcome 14
        // was max-mixed into welcome_total.
        let bad = RampFunnel {
            requested_clients: 8,
            spawn_issued_total: 8,
            connect_attempts_completed: 8,
            transport_established_total: 14,
            welcome_total: 14,
            world_entered_total: 14,
            peak_active_clients: 8,
            reconnect_ok_total: 6,
            server_welcome_ok: 14,
            server_session_accepted: 14,
            server_transport_accept_ok: 14,
            ..RampFunnel::default()
        };
        let report = check_ramp_funnel_invariant(&bad, 0);
        assert!(!report.ok, "{}", report.note);
        assert!(
            report.note.contains("welcome 14 > spawn_issued 8"),
            "{}",
            report.note
        );
    }

    #[test]
    fn funnel_invariant_accepts_harness_issuance_with_reconnect_correlator() {
        let good = RampFunnel {
            requested_clients: 8,
            spawn_issued_total: 8,
            connect_attempts_completed: 8,
            transport_established_total: 8,
            welcome_total: 8,
            world_entered_total: 8,
            peak_active_clients: 8,
            reconnect_ok_total: 6,
            server_transport_accept_ok: 14,
            server_welcome_ok: 14,
            server_session_accepted: 14,
            disconnects: 6,
            ..RampFunnel::default()
        };
        let report = check_ramp_funnel_invariant(&good, 0);
        assert!(report.ok, "{}", report.note);
        assert!(report.note.contains("reconnect_ok"), "{}", report.note);
    }

    #[test]
    fn funnel_invariant_flags_unexplained_server_excess() {
        let f = RampFunnel {
            requested_clients: 384,
            spawn_issued_total: 114,
            connect_attempts_completed: 112,
            transport_established_total: 112,
            welcome_total: 112,
            world_entered_total: 112,
            peak_active_clients: 114,
            reconnect_ok_total: 0,
            server_transport_accept_ok: 120,
            server_welcome_ok: 120,
            server_session_accepted: 120,
            ..RampFunnel::default()
        };
        let report = check_ramp_funnel_invariant(&f, 2);
        assert!(!report.ok, "{}", report.note);
        assert!(
            report.note.contains("unexplained excess"),
            "{}",
            report.note
        );
    }

    #[test]
    fn connection_ramp_schema2_note_mentions_harness_owned_funnel() {
        assert_eq!(ConnectionRampSnapshot::SCHEMA, 2);
        assert!(ConnectionRampSnapshot::NOTE.contains("harness-owned"));
        assert!(ConnectionRampSnapshot::NOTE.contains("correlators"));
    }
}
