//! Authoritative gameplay input. Network tasks never touch [`World`].
//!
//! Late-collapse is intentional authoritative input compaction: after
//! Continuation starvation, a prefix of delayed commands is acknowledged in
//! one `tick_player` (latest held + `jump_pressed` OR). Intermediate historical
//! held commands may be acknowledged without individual physics steps.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use purgatory_common::{
    ChannelId, CharacterId, ContentId, InstanceId, MAP_FOOTNOTE_AUTHORED, MAP_SECOND_AUTHORED,
    RestoreIntent, WorldAddress,
};
use purgatory_content::{
    ContentRegistry, EquipmentAuthError, LoadMode, authorize_equip, default_content_root,
    load_registry, map_plan, resolve_restore, runtime_placement, world_address_for_map,
};
use purgatory_persistence::{PersistentCharacter, PersistentCharacterSnapshot};
use purgatory_protocol::{
    AbilityActivateRequest, AbilityCommandReject, ConnectionId, DEV_CHANNEL_MAX, EquipRequest,
    EquipmentRejectReason, InputCommand, InteractCloseReason, InteractRejectReason, MoveAxis,
    ServerAbility, ServerControl, ServerEquipment, ServerInteract, ServerPresentationOneShot,
    UnequipRequest, WireEntityId,
};
use purgatory_simulation::{
    AbilityActivation, AbilityRejectReason, AbilityRequest, ActionGateContext, Cadence,
    CommandClass, CommandDenial, EntityId, EntityKind, EquipmentSlot, FOOTNOTE_SPAWN_X, Health,
    InputGateReason, InteractionCloseReason, InteractionReject, P0, P0_POSITION, PLAYER_HEALTH_MAX,
    PlayerInput, PlayerState, PresentationOneShotKind, RuntimeSpawnRequest, ScheduleOwner,
    SimulationTick, Transform, WorkLane, World, validate_command_preamble,
};

use super::persist::PersistenceHandle;
use super::replication::{
    InterestFanoutIndex, ObserverReplicationState, PublishPolicyInput, ReplicationPipe,
    publish_observer_frame,
};
use super::replication_fanout::ReplicationFanoutAccounting;
use super::replication_policy::{PolicyMode, PopulationClass, population_class_from_env};
use super::tick_domains::TickDomainSample;

/// How a command compared against the last accepted sequence in this epoch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeqDecision {
    Accept,
    Duplicate,
    Stale,
    Gap,
    Overflow,
    OldEpoch,
}

const SESSION_QUEUE_CAP: usize = 128;
const LIVE_COMBAT_CREATURE_TYPE_TOKEN: u32 = 9_000;
const LIVE_COMBAT_CREATURE_AGGRO_RADIUS: f32 = 3.0;

fn live_basic_strike_id() -> ContentId {
    ContentId::from_authored("skill.basic.strike").expect("authored basic strike id")
}

fn map_ability_reject(reason: AbilityRejectReason) -> AbilityCommandReject {
    match reason {
        AbilityRejectReason::InvalidDefinition => AbilityCommandReject::InvalidRequest,
        AbilityRejectReason::MissingActor => AbilityCommandReject::StateBlocked,
        AbilityRejectReason::MissingTarget => AbilityCommandReject::InvalidActivation,
        AbilityRejectReason::ActorDead => AbilityCommandReject::ActorDead,
        AbilityRejectReason::TargetDead => AbilityCommandReject::InvalidActivation,
        AbilityRejectReason::OutOfRange => AbilityCommandReject::InvalidActivation,
        AbilityRejectReason::OnCooldown => AbilityCommandReject::OnCooldown,
        AbilityRejectReason::Busy => AbilityCommandReject::Busy,
        AbilityRejectReason::Gate(_) => AbilityCommandReject::StateBlocked,
    }
}

/// Server-side transition input barrier. Not Phase 6F action-state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum InputGate {
    #[default]
    Open,
    Locked {
        reason: InputGateReason,
        remaining_ticks: u16,
    },
}

impl InputGate {
    #[must_use]
    fn locked(reason: InputGateReason) -> Self {
        Self::Locked {
            reason,
            remaining_ticks: reason.lock_ticks(),
        }
    }

    #[must_use]
    fn is_locked(self) -> bool {
        matches!(self, Self::Locked { remaining_ticks, .. } if remaining_ticks > 0)
    }

    #[must_use]
    fn reason(self) -> Option<InputGateReason> {
        match self {
            Self::Open => None,
            Self::Locked { reason, .. } => Some(reason),
        }
    }

    fn tick(&mut self) {
        let Self::Locked {
            remaining_ticks, ..
        } = self
        else {
            return;
        };
        *remaining_ticks = remaining_ticks.saturating_sub(1);
        if *remaining_ticks == 0 {
            *self = Self::Open;
        }
    }
}

/// Per-session command queue and acknowledgement state.
pub struct SessionInput {
    pub input_epoch: u16,
    pub last_received_seq: Option<u32>,
    pub last_acknowledged_seq: Option<u32>,
    /// Unmatched Continuation steps. Saturates; never wraps.
    pub unmatched_continuation_ticks: u16,
    pub move_axis: MoveAxis,
    pub down_held: bool,
    queue: VecDeque<InputCommand>,
    pub late_collapse_count: u64,
    pub late_collapse_max_batch: u16,
    pub held_cancel_count: u64,
    gate: InputGate,
}

impl Default for SessionInput {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionInput {
    #[must_use]
    pub fn new() -> Self {
        Self {
            input_epoch: 0,
            last_received_seq: None,
            last_acknowledged_seq: None,
            unmatched_continuation_ticks: 0,
            move_axis: MoveAxis::Neutral,
            down_held: false,
            queue: VecDeque::new(),
            late_collapse_count: 0,
            late_collapse_max_batch: 0,
            held_cancel_count: 0,
            gate: InputGate::Open,
        }
    }

    /// `u32` sequences do not wrap within an epoch. First Accept of an epoch
    /// must be sequence 1.
    #[must_use]
    pub fn classify(last: Option<u32>, next: u32) -> SeqDecision {
        match last {
            None if next == 1 => SeqDecision::Accept,
            None => SeqDecision::Gap,
            Some(prev) if next == prev => SeqDecision::Duplicate,
            Some(prev) if next > prev && next == prev.saturating_add(1) => SeqDecision::Accept,
            Some(prev) if next > prev => SeqDecision::Gap,
            Some(_) => SeqDecision::Stale,
        }
    }

    pub fn apply(&mut self, cmd: InputCommand) -> SeqDecision {
        if cmd.input_epoch < self.input_epoch {
            return SeqDecision::OldEpoch;
        }
        if cmd.input_epoch > self.input_epoch {
            return SeqDecision::Gap;
        }
        let decision = Self::classify(self.last_received_seq, cmd.sequence);
        if decision != SeqDecision::Accept {
            return decision;
        }
        if self.queue.len() >= SESSION_QUEUE_CAP {
            return SeqDecision::Overflow;
        }
        self.last_received_seq = Some(cmd.sequence);
        self.queue.push_back(cmd);
        decision
    }

    /// Idempotent barrier: idle held state, flush queue, cancel-ack through
    /// last received sequence without simulating those commands.
    pub fn held_cancel(&mut self) {
        self.queue.clear();
        self.move_axis = MoveAxis::Neutral;
        self.down_held = false;
        self.last_acknowledged_seq = self.last_received_seq;
        self.unmatched_continuation_ticks = 0;
        self.held_cancel_count = self.held_cancel_count.saturating_add(1);
    }

    /// Lifecycle rebase. `None` if epoch is `u16::MAX` (do not wrap to 0).
    pub fn bump_epoch(&mut self) -> Option<u16> {
        if self.input_epoch == u16::MAX {
            return None;
        }
        self.input_epoch = self.input_epoch.saturating_add(1);
        self.queue.clear();
        self.last_received_seq = None;
        self.last_acknowledged_seq = None;
        self.unmatched_continuation_ticks = 0;
        self.move_axis = MoveAxis::Neutral;
        self.down_held = false;
        self.gate = InputGate::Open;
        Some(self.input_epoch)
    }

    /// Neutralize held/queued movement and bar gameplay until the presentation
    /// FadeOut + min Hold window elapses (earliest honest FadeIn).
    pub fn lock_transition(&mut self, reason: InputGateReason) {
        self.queue.clear();
        self.move_axis = MoveAxis::Neutral;
        self.down_held = false;
        self.unmatched_continuation_ticks = 0;
        self.gate = InputGate::locked(reason);
    }

    #[must_use]
    pub fn input_gated(&self) -> bool {
        self.gate.is_locked()
    }

    #[must_use]
    pub fn input_gate_reason(&self) -> Option<InputGateReason> {
        self.gate.reason()
    }

    /// Ack queued commands as idle. Do not adopt their held movement.
    fn ack_queued_as_idle(&mut self) {
        while let Some(cmd) = self.queue.pop_front() {
            self.last_acknowledged_seq = Some(cmd.sequence);
        }
        self.move_axis = MoveAxis::Neutral;
        self.down_held = false;
        self.unmatched_continuation_ticks = 0;
    }

    /// One player simulation step's input. Does not run physics.
    #[must_use]
    pub fn take_for_tick(&mut self) -> PlayerInput {
        if self.gate.is_locked() {
            self.ack_queued_as_idle();
            self.gate.tick();
            return PlayerInput::idle();
        }
        if self.queue.is_empty() {
            self.unmatched_continuation_ticks = self.unmatched_continuation_ticks.saturating_add(1);
            return PlayerInput {
                move_axis: self.move_axis.to_i8(),
                jump_pressed: false,
                down_held: self.down_held,
            };
        }
        let debt = self.unmatched_continuation_ticks;
        if debt == 0 {
            let cmd = self.queue.pop_front().expect("queue non-empty");
            self.consume_one(cmd)
        } else {
            self.late_collapse(debt)
        }
    }

    fn consume_one(&mut self, cmd: InputCommand) -> PlayerInput {
        self.move_axis = cmd.move_axis;
        self.down_held = cmd.down_held;
        self.last_acknowledged_seq = Some(cmd.sequence);
        PlayerInput {
            move_axis: cmd.move_axis.to_i8(),
            jump_pressed: cmd.jump_pressed,
            down_held: cmd.down_held,
        }
    }

    fn late_collapse(&mut self, debt: u16) -> PlayerInput {
        let n = (debt as usize).min(self.queue.len());
        debug_assert!(n >= 1);
        let mut jump = false;
        let mut last = None;
        for _ in 0..n {
            let cmd = self.queue.pop_front().expect("prefix");
            jump |= cmd.jump_pressed;
            last = Some(cmd);
        }
        let cmd = last.expect("n >= 1");
        self.move_axis = cmd.move_axis;
        self.down_held = cmd.down_held;
        self.last_acknowledged_seq = Some(cmd.sequence);
        self.unmatched_continuation_ticks = debt.saturating_sub(n as u16);
        self.late_collapse_count = self.late_collapse_count.saturating_add(1);
        self.late_collapse_max_batch = self.late_collapse_max_batch.max(n as u16);
        PlayerInput {
            move_axis: cmd.move_axis.to_i8(),
            jump_pressed: jump,
            down_held: cmd.down_held,
        }
    }

    #[must_use]
    pub fn last_acknowledged(&self) -> u32 {
        self.last_acknowledged_seq.unwrap_or(0)
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn queued_len(&self) -> usize {
        self.queue.len()
    }
}

pub struct PlayerBinding {
    pub entity: EntityId,
    pub character_id: Option<CharacterId>,
    persistence_revision: u64,
    restore: RestoreIntent,
    pub input: SessionInput,
    replication: Option<ReplicationPipe>,
    interest: ObserverReplicationState,
    interact: Option<tokio::sync::mpsc::Sender<ServerControl>>,
    last_equipment_seq: Option<u32>,
    last_equipment_result: Option<ServerEquipment>,
    last_ability_seq: Option<u32>,
    last_ability_result: Option<ServerAbility>,
}

/// Simulation-thread owner of `World` and `ConnectionId → EntityId`.
pub struct GameplayOwner {
    world: World,
    registry: ContentRegistry,
    bindings: HashMap<ConnectionId, PlayerBinding>,
    occupancy: HashMap<CharacterId, ConnectionId>,
    persist: Option<PersistenceHandle>,
    ticks: u64,
    pub input_received: u64,
    pub input_accepted: u64,
    pub input_duplicate: u64,
    pub input_stale: u64,
    pub input_queue_overflow: u64,
    pub snapshots_built: u64,
    pub snapshot_build_count: u64,
    pub snapshot_sequence: u32,
    pub last_snapshot_entities: u16,
    pub snapshot_send_failed: u64,
    pub player_entity_spawned: u64,
    pub player_entity_despawned: u64,
    pub duplicate_session_detected: u64,
    pub session_queue_max: u64,
    pub snapshot_build_time_max_us: u64,
    pub aoi_enters: u64,
    pub aoi_leaves: u64,
    pub aoi_updates: u64,
    pub aoi_churn_reentry: u64,
    pub aoi_update_bytes: u64,
    pub oldest_pending_ticks: u64,
    pub max_deferred_ticks: u64,
    pub replication_queue_depth_max: u64,
    pub command_rejects_gate: u64,
    pub command_rejects_other: u64,
    pub observer_pending_updates: u64,
    pub observer_pending_enters: u64,
    pub cadence_deferred_updates: u64,
    pub writer_queue_push_fail_total: u64,
    /// Cumulative micros spent in persist try_save during the current tick window.
    persist_enqueue_us: u64,
    placement: LoadPlacement,
    trace_relevance: bool,
    trace_snapshot: bool,
    runtime_probe: RuntimeProbe,
    load_pressure: super::load_pressure::LoadPressure,
    /// Shared entity → Known-observer reverse index (6G.7B).
    interest_fanout: InterestFanoutIndex,
    replication_fanout_accounting: ReplicationFanoutAccounting,
    replication_policy_mode: PolicyMode,
    population_class: PopulationClass,
    last_observer_bytes: HashMap<ConnectionId, u32>,
    tick_overrun_hint: bool,
    pressure: Option<std::sync::Arc<super::network_pressure::NetworkPressureBook>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoadPlacement {
    Cluster,
    /// Pack every spawn inside a small shared AOI (mutual relevance hotspot).
    Hotspot,
    Spread,
    MultiMap,
}

/// Opt-in DEV demonstration of scheduler/spawn. Off unless `PURGATORY_RUNTIME_PROBE=1`.
struct RuntimeProbe {
    enabled: bool,
    armed: bool,
}

impl RuntimeProbe {
    fn from_env() -> Self {
        let enabled = matches!(
            std::env::var("PURGATORY_RUNTIME_PROBE")
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "1" | "true" | "yes" | "on"
        );
        Self {
            enabled,
            armed: false,
        }
    }
}

fn load_placement_from_env() -> LoadPlacement {
    match std::env::var("PURGATORY_LOAD_PLACEMENT")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "hotspot" => LoadPlacement::Hotspot,
        "spread" => LoadPlacement::Spread,
        "maps" | "multimap" => LoadPlacement::MultiMap,
        _ => LoadPlacement::Cluster,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnterError {
    Occupied,
    RestoreFailed,
    SpawnFailed,
}

pub enum LifecycleCmd {
    Attach {
        connection_id: ConnectionId,
        replication: Option<ReplicationPipe>,
        interact: Option<tokio::sync::mpsc::Sender<ServerControl>>,
    },
    Enter {
        connection_id: ConnectionId,
        character: PersistentCharacter,
        replication: Option<ReplicationPipe>,
        interact: Option<tokio::sync::mpsc::Sender<ServerControl>>,
        reply: tokio::sync::oneshot::Sender<Result<(), EnterError>>,
    },
    Detach {
        connection_id: ConnectionId,
    },
}

pub enum InputUpdate {
    Command {
        connection_id: ConnectionId,
        command: InputCommand,
    },
    HeldCancel {
        connection_id: ConnectionId,
    },
    InteractOpen {
        connection_id: ConnectionId,
        target: WireEntityId,
    },
    InteractClose {
        connection_id: ConnectionId,
        session_id: u32,
    },
    PortalActivate {
        connection_id: ConnectionId,
        target: WireEntityId,
    },
    DevSetChannel {
        connection_id: ConnectionId,
        channel: u32,
    },
    Equip {
        connection_id: ConnectionId,
        request: EquipRequest,
    },
    Unequip {
        connection_id: ConnectionId,
        request: UnequipRequest,
    },
    DevPresentationOneShot {
        connection_id: ConnectionId,
        kind: u8,
    },
    DevResetPlayer {
        connection_id: ConnectionId,
    },
    Respawn {
        connection_id: ConnectionId,
    },
    AbilityActivate {
        connection_id: ConnectionId,
        request: AbilityActivateRequest,
    },
}

/// Cloneable senders. Connection tasks only `try_send`; they never lock World.
#[derive(Clone)]
pub struct GameplayTx {
    pub lifecycle: tokio::sync::mpsc::Sender<LifecycleCmd>,
    pub input: tokio::sync::mpsc::Sender<InputUpdate>,
}

impl GameplayTx {
    #[allow(dead_code)]
    pub fn attach(&self, connection_id: ConnectionId) {
        self.attach_with_snapshots(connection_id, None, None);
    }

    pub fn attach_with_snapshots(
        &self,
        connection_id: ConnectionId,
        replication: Option<ReplicationPipe>,
        interact: Option<tokio::sync::mpsc::Sender<ServerControl>>,
    ) -> bool {
        self.lifecycle
            .try_send(LifecycleCmd::Attach {
                connection_id,
                replication,
                interact,
            })
            .is_ok()
    }

    /// Await backpressure so occupancy is released even if the lifecycle
    /// channel is full. Used on the live session teardown path.
    pub async fn send_detach(&self, connection_id: ConnectionId) -> bool {
        self.lifecycle
            .send(LifecycleCmd::Detach { connection_id })
            .await
            .is_ok()
    }

    /// Non-blocking detach for Drop guards. Idempotent in [`GameplayOwner::detach`].
    pub fn try_detach(&self, connection_id: ConnectionId) -> bool {
        self.lifecycle
            .try_send(LifecycleCmd::Detach { connection_id })
            .is_ok()
    }

    pub async fn enter(
        &self,
        connection_id: ConnectionId,
        character: PersistentCharacter,
        replication: Option<ReplicationPipe>,
        interact: Option<tokio::sync::mpsc::Sender<ServerControl>>,
    ) -> Result<Result<(), EnterError>, ()> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.lifecycle
            .send(LifecycleCmd::Enter {
                connection_id,
                character,
                replication,
                interact,
                reply,
            })
            .await
            .map_err(|_| ())?;
        rx.await.map_err(|_| ())
    }

    /// Non-blocking probe (tests / diagnostics). Production path uses [`Self::send_input`].
    #[allow(dead_code)]
    pub fn try_input(&self, connection_id: ConnectionId, command: InputCommand) -> bool {
        self.input
            .try_send(InputUpdate::Command {
                connection_id,
                command,
            })
            .is_ok()
    }

    #[allow(dead_code)]
    pub fn try_held_cancel(&self, connection_id: ConnectionId) -> bool {
        self.input
            .try_send(InputUpdate::HeldCancel { connection_id })
            .is_ok()
    }

    /// Authoritative gameplay input must not be silently discarded: sequences are
    /// contiguous and `jump_pressed` is an edge. Await backpressure instead of
    /// `try_send` drop (which would open a Gap and soft-brick the session).
    pub async fn send_input(&self, connection_id: ConnectionId, command: InputCommand) -> bool {
        self.input
            .send(InputUpdate::Command {
                connection_id,
                command,
            })
            .await
            .is_ok()
    }

    pub async fn send_held_cancel(&self, connection_id: ConnectionId) -> bool {
        self.input
            .send(InputUpdate::HeldCancel { connection_id })
            .await
            .is_ok()
    }

    pub async fn send_interact_open(
        &self,
        connection_id: ConnectionId,
        target: WireEntityId,
    ) -> bool {
        self.input
            .send(InputUpdate::InteractOpen {
                connection_id,
                target,
            })
            .await
            .is_ok()
    }

    pub async fn send_interact_close(&self, connection_id: ConnectionId, session_id: u32) -> bool {
        self.input
            .send(InputUpdate::InteractClose {
                connection_id,
                session_id,
            })
            .await
            .is_ok()
    }

    pub async fn send_portal_activate(
        &self,
        connection_id: ConnectionId,
        target: WireEntityId,
    ) -> bool {
        self.input
            .send(InputUpdate::PortalActivate {
                connection_id,
                target,
            })
            .await
            .is_ok()
    }

    pub async fn send_dev_set_channel(&self, connection_id: ConnectionId, channel: u32) -> bool {
        self.input
            .send(InputUpdate::DevSetChannel {
                connection_id,
                channel,
            })
            .await
            .is_ok()
    }

    pub async fn send_equip(&self, connection_id: ConnectionId, request: EquipRequest) -> bool {
        self.input
            .send(InputUpdate::Equip {
                connection_id,
                request,
            })
            .await
            .is_ok()
    }

    pub async fn send_unequip(&self, connection_id: ConnectionId, request: UnequipRequest) -> bool {
        self.input
            .send(InputUpdate::Unequip {
                connection_id,
                request,
            })
            .await
            .is_ok()
    }

    pub async fn send_dev_presentation_oneshot(
        &self,
        connection_id: ConnectionId,
        kind: u8,
    ) -> bool {
        self.input
            .send(InputUpdate::DevPresentationOneShot {
                connection_id,
                kind,
            })
            .await
            .is_ok()
    }

    pub async fn send_dev_reset_player(&self, connection_id: ConnectionId) -> bool {
        self.input
            .send(InputUpdate::DevResetPlayer { connection_id })
            .await
            .is_ok()
    }

    pub async fn send_respawn(&self, connection_id: ConnectionId) -> bool {
        self.input
            .send(InputUpdate::Respawn { connection_id })
            .await
            .is_ok()
    }

    pub async fn send_ability_activate(
        &self,
        connection_id: ConnectionId,
        request: AbilityActivateRequest,
    ) -> bool {
        self.input
            .send(InputUpdate::AbilityActivate {
                connection_id,
                request,
            })
            .await
            .is_ok()
    }
}

const LIFECYCLE_CAP: usize = 64;
const INPUT_CAP: usize = 128;

#[must_use]
#[allow(dead_code)]
pub const fn lifecycle_cap() -> usize {
    LIFECYCLE_CAP
}

#[must_use]
#[allow(dead_code)]
pub const fn input_cap() -> usize {
    INPUT_CAP
}

/// Create gameplay channels sized for the active server config.
#[must_use]
pub fn gameplay_channels(
    lifecycle_cap: usize,
    input_cap: usize,
) -> (
    GameplayTx,
    tokio::sync::mpsc::Receiver<LifecycleCmd>,
    tokio::sync::mpsc::Receiver<InputUpdate>,
) {
    let (life_tx, life_rx) = tokio::sync::mpsc::channel(lifecycle_cap.max(1));
    let (input_tx, input_rx) = tokio::sync::mpsc::channel(input_cap.max(1));
    (
        GameplayTx {
            lifecycle: life_tx,
            input: input_tx,
        },
        life_rx,
        input_rx,
    )
}

impl GameplayOwner {
    /// Content-backed maps. Dev-eager Map A + Map B; runtime still supports lazy ensure/destroy.
    #[must_use]
    pub fn new() -> Self {
        let registry =
            load_registry(&default_content_root(), LoadMode::Full).unwrap_or_else(|err| {
                panic!("PURGATORY server content invalid:\n{err}");
            });
        Self::with_registry(registry)
    }

    #[must_use]
    pub fn with_registry(registry: ContentRegistry) -> Self {
        let mut world = World::new();
        instantiate_dev_maps(&mut world, &registry);
        log_dev_interaction_fixtures(&world);
        Self {
            world,
            registry,
            bindings: HashMap::new(),
            occupancy: HashMap::new(),
            persist: None,
            ticks: 0,
            input_received: 0,
            input_accepted: 0,
            input_duplicate: 0,
            input_stale: 0,
            input_queue_overflow: 0,
            snapshots_built: 0,
            snapshot_build_count: 0,
            snapshot_sequence: 0,
            last_snapshot_entities: 0,
            snapshot_send_failed: 0,
            player_entity_spawned: 0,
            player_entity_despawned: 0,
            duplicate_session_detected: 0,
            session_queue_max: 0,
            snapshot_build_time_max_us: 0,
            aoi_enters: 0,
            aoi_leaves: 0,
            aoi_updates: 0,
            aoi_churn_reentry: 0,
            aoi_update_bytes: 0,
            oldest_pending_ticks: 0,
            max_deferred_ticks: 0,
            replication_queue_depth_max: 0,
            command_rejects_gate: 0,
            command_rejects_other: 0,
            observer_pending_updates: 0,
            observer_pending_enters: 0,
            cadence_deferred_updates: 0,
            writer_queue_push_fail_total: 0,
            persist_enqueue_us: 0,
            placement: load_placement_from_env(),
            trace_relevance: false,
            trace_snapshot: false,
            runtime_probe: RuntimeProbe::from_env(),
            load_pressure: super::load_pressure::LoadPressure::from_process_env(),
            interest_fanout: InterestFanoutIndex::new(),
            replication_fanout_accounting: ReplicationFanoutAccounting::default(),
            replication_policy_mode: PolicyMode::from_env(),
            population_class: population_class_from_env(),
            last_observer_bytes: HashMap::new(),
            tick_overrun_hint: false,
            pressure: None,
        }
    }

    pub fn set_persist(&mut self, persist: PersistenceHandle) {
        self.persist = Some(persist);
    }

    pub fn set_pressure(
        &mut self,
        pressure: std::sync::Arc<super::network_pressure::NetworkPressureBook>,
    ) {
        self.pressure = Some(pressure);
    }

    pub fn flush_persistent_snapshots(&mut self) {
        let ids: Vec<ConnectionId> = self.bindings.keys().copied().collect();
        for id in ids {
            self.request_save(id);
        }
    }

    #[must_use]
    #[allow(dead_code)] // diagnostic accessors used by tests and the quality gate
    pub fn world(&self) -> &World {
        &self.world
    }

    #[cfg(test)]
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn runtime_stats(&self) -> purgatory_simulation::RuntimeStats {
        self.world.runtime_stats()
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn player_count(&self) -> usize {
        self.bindings.len()
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn entity_of(&self, id: ConnectionId) -> Option<EntityId> {
        self.bindings.get(&id).map(|b| b.entity)
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn last_seq(&self, id: ConnectionId) -> Option<u32> {
        self.bindings
            .get(&id)
            .and_then(|b| b.input.last_acknowledged_seq)
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn last_received(&self, id: ConnectionId) -> Option<u32> {
        self.bindings
            .get(&id)
            .and_then(|b| b.input.last_received_seq)
    }

    #[cfg(test)]
    pub fn stand_on_first_oneway(&mut self, id: ConnectionId) -> bool {
        use purgatory_simulation::PlatformKind;
        let Some(entity) = self.entity_of(id) else {
            return false;
        };
        let Some(view) = self.world.iter_platforms().find(|view| {
            view.platform.kind == PlatformKind::OneWay
                && self.world.address_of(view.id) == self.world.address_of(entity)
        }) else {
            return false;
        };
        let (transform, state) =
            PlayerState::standing_on_at(view.id, view.top_surface(), view.transform.position[0]);
        let previous = {
            let Some((t, player)) = self.world.player_parts_mut_for(entity) else {
                return false;
            };
            let previous = t.position;
            *t = transform;
            *player = state;
            previous
        };
        self.world.refresh_spatial(entity, previous);
        true
    }

    #[cfg(test)]
    pub fn set_player_x(&mut self, id: ConnectionId, x: f32) -> bool {
        let Some(entity) = self.entity_of(id) else {
            return false;
        };
        let Some(prev) = self.world.transform_of(entity) else {
            return false;
        };
        let mut next = prev;
        next.position[0] = x;
        self.world.set_transform(entity, next)
    }

    #[cfg(test)]
    pub fn set_entity_address(
        &mut self,
        id: ConnectionId,
        address: purgatory_simulation::WorldAddress,
    ) -> bool {
        let Some(entity) = self.entity_of(id) else {
            return false;
        };
        self.world.set_address(entity, address)
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn contains_entity(&self, id: EntityId) -> bool {
        self.world.contains(id)
    }

    pub fn attach(&mut self, connection_id: ConnectionId) {
        if self.bindings.contains_key(&connection_id) {
            self.duplicate_session_detected = self.duplicate_session_detected.saturating_add(1);
            return;
        }
        self.ensure_live_combat_creature();
        let address = self.map_a_address();
        let n = self.bindings.len();
        let spawn_x = match self.placement {
            // Default cluster and hotspot both pack near spawn. Hotspot keeps
            // every index within ~2 wu so AOI leave rects fully overlap.
            LoadPlacement::Cluster => FOOTNOTE_SPAWN_X,
            LoadPlacement::Hotspot => FOOTNOTE_SPAWN_X + (n as f32 % 8.0) * 0.25,
            LoadPlacement::Spread => FOOTNOTE_SPAWN_X + (n as f32 % 8.0) * 5.0,
            LoadPlacement::MultiMap => FOOTNOTE_SPAWN_X,
        };
        let spawn_address = match self.placement {
            LoadPlacement::MultiMap if n % 2 == 1 => self.map_b_address(),
            _ => address,
        };
        let _ = self.spawn_player_binding(
            connection_id,
            spawn_address,
            spawn_x,
            None,
            0,
            RestoreIntent::footnote_default(),
            None,
            None,
        );
    }

    pub fn enter(
        &mut self,
        connection_id: ConnectionId,
        character: PersistentCharacter,
        replication: Option<ReplicationPipe>,
        interact: Option<tokio::sync::mpsc::Sender<ServerControl>>,
    ) -> Result<(), EnterError> {
        if self.bindings.contains_key(&connection_id) {
            self.duplicate_session_detected = self.duplicate_session_detected.saturating_add(1);
            return Err(EnterError::Occupied);
        }
        let character_id = character.character_id;
        if self.occupancy.contains_key(&character_id) {
            return Err(EnterError::Occupied);
        }
        self.ensure_live_combat_creature();
        self.occupancy.insert(character_id, connection_id);
        let logical = resolve_restore(&self.registry, &character.restore);
        let Some((address, spawn_pos)) = runtime_placement(&self.registry, &logical) else {
            self.occupancy.remove(&character_id);
            return Err(EnterError::RestoreFailed);
        };
        let plan = match map_plan(&self.registry, &logical.map_authored, address) {
            Ok(plan) => plan,
            Err(_) => {
                self.occupancy.remove(&character_id);
                return Err(EnterError::RestoreFailed);
            }
        };
        if self.world.ensure_map(&plan).is_err() {
            self.occupancy.remove(&character_id);
            return Err(EnterError::RestoreFailed);
        }
        match self.spawn_player_binding(
            connection_id,
            address,
            spawn_pos[0],
            Some(character_id),
            character.persistence_revision,
            character.restore.clone(),
            replication,
            interact,
        ) {
            true => Ok(()),
            false => {
                self.occupancy.remove(&character_id);
                Err(EnterError::SpawnFailed)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_player_binding(
        &mut self,
        connection_id: ConnectionId,
        spawn_address: WorldAddress,
        spawn_x: f32,
        character_id: Option<CharacterId>,
        persistence_revision: u64,
        restore: RestoreIntent,
        replication: Option<ReplicationPipe>,
        interact: Option<tokio::sync::mpsc::Sender<ServerControl>>,
    ) -> bool {
        let floor = self
            .world
            .iter_platforms()
            .find(|view| {
                self.world.address_of(view.id) == Some(spawn_address)
                    && view.platform.half_extents == P0.half_extents
                    && (view.transform.position[0] - P0_POSITION[0]).abs() < 0.01
                    && (view.transform.position[1] - P0_POSITION[1]).abs() < 0.01
            })
            .or_else(|| {
                self.world
                    .iter_platforms()
                    .find(|view| self.world.address_of(view.id) == Some(spawn_address))
            });
        let Some(view) = floor else {
            return false;
        };
        let (transform, state) = PlayerState::standing_on_at(view.id, view.top_surface(), spawn_x);
        let entity = self.world.spawn_player_at(spawn_address, transform, state);
        let _ = self
            .world
            .set_health(entity, Health::full(PLAYER_HEALTH_MAX));
        let _ = self.world.grant_ability(entity, live_basic_strike_id());
        self.bindings.insert(
            connection_id,
            PlayerBinding {
                entity,
                character_id,
                persistence_revision,
                restore,
                input: SessionInput::new(),
                replication,
                interest: ObserverReplicationState::new(),
                interact,
                last_equipment_seq: None,
                last_equipment_result: None,
                last_ability_seq: None,
                last_ability_result: None,
            },
        );
        self.player_entity_spawned = self.player_entity_spawned.saturating_add(1);
        true
    }

    fn ensure_live_combat_creature(&mut self) {
        if self.world.iter().any(|id| {
            self.world
                .npc_of(id)
                .is_some_and(|npc| npc.type_token == LIVE_COMBAT_CREATURE_TYPE_TOKEN)
        }) {
            return;
        }
        let address = self.map_a_address();
        let Some(floor) = self
            .world
            .iter_platforms()
            .find(|platform| self.world.address_of(platform.id) == Some(address))
        else {
            return;
        };
        let (_, player_state) =
            PlayerState::standing_on_at(floor.id, floor.top_surface(), FOOTNOTE_SPAWN_X);
        let player_y = floor.top_surface() + player_state.half_extents[1];
        let now = SimulationTick::from_count(self.ticks);
        let Some(creature) = self.world.spawn(World::npc_spawn_request(
            address,
            [FOOTNOTE_SPAWN_X + 6.0, player_y],
            LIVE_COMBAT_CREATURE_TYPE_TOKEN,
            LIVE_COMBAT_CREATURE_AGGRO_RADIUS,
            9,
            now,
            true,
            purgatory_simulation::NPC_HEALTH_MAX,
        )) else {
            return;
        };
        if let Some(mut npc) = self.world.npc_of(creature) {
            npc.walking = false;
            let _ = self.world.set_npc(creature, npc);
        }
    }

    pub fn detach(&mut self, connection_id: ConnectionId) {
        if let Some(binding) = self.bindings.remove(&connection_id) {
            if let Some(character_id) = binding.character_id {
                self.occupancy.remove(&character_id);
                self.emit_save(&PersistentCharacterSnapshot {
                    character_id,
                    persistence_revision: binding.persistence_revision.saturating_add(1),
                    restore: binding.restore.clone(),
                    instance_exit: None,
                });
            }
            let closed = self
                .world
                .close_sessions_involving(binding.entity, InteractionCloseReason::Disconnected);
            if let Some(tx) = &binding.interact {
                for (session, reason) in closed {
                    let _ = tx.try_send(ServerControl::Interact(ServerInteract::Closed {
                        session_id: session.id.get(),
                        reason: map_close_reason(reason),
                    }));
                }
            }
            self.interest_fanout.clear_observer(binding.entity);
            self.interest_fanout.clear_subject(binding.entity);
            self.world.despawn(binding.entity);
            self.player_entity_despawned = self.player_entity_despawned.saturating_add(1);
        }
    }

    fn request_save(&mut self, connection_id: ConnectionId) {
        let Some(binding) = self.bindings.get_mut(&connection_id) else {
            return;
        };
        let Some(character_id) = binding.character_id else {
            return;
        };
        binding.persistence_revision = binding.persistence_revision.saturating_add(1);
        let snapshot = PersistentCharacterSnapshot {
            character_id,
            persistence_revision: binding.persistence_revision,
            restore: binding.restore.clone(),
            instance_exit: None,
        };
        self.emit_save(&snapshot);
    }

    fn emit_save(&mut self, snapshot: &PersistentCharacterSnapshot) {
        if let Some(persist) = &self.persist {
            let t0 = std::time::Instant::now();
            let _ = persist.try_save(snapshot.clone());
            let us = u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX);
            self.persist_enqueue_us = self.persist_enqueue_us.saturating_add(us);
        }
    }

    fn note_persistent_restore(&mut self, connection_id: ConnectionId, restore: RestoreIntent) {
        let Some(binding) = self.bindings.get_mut(&connection_id) else {
            return;
        };
        if binding.character_id.is_none() {
            return;
        }
        binding.restore = restore;
        self.request_save(connection_id);
    }

    pub fn apply_input(&mut self, update: InputUpdate) -> SeqDecision {
        if matches!(
            update,
            InputUpdate::InteractOpen { .. }
                | InputUpdate::InteractClose { .. }
                | InputUpdate::PortalActivate { .. }
                | InputUpdate::DevSetChannel { .. }
                | InputUpdate::Equip { .. }
                | InputUpdate::Unequip { .. }
                | InputUpdate::DevPresentationOneShot { .. }
                | InputUpdate::DevResetPlayer { .. }
                | InputUpdate::Respawn { .. }
                | InputUpdate::AbilityActivate { .. }
        ) {
            match update {
                InputUpdate::InteractOpen {
                    connection_id,
                    target,
                } => self.handle_interact_open(connection_id, target),
                InputUpdate::InteractClose {
                    connection_id,
                    session_id,
                } => self.handle_interact_close(connection_id, session_id),
                InputUpdate::PortalActivate {
                    connection_id,
                    target,
                } => self.handle_portal_activate(connection_id, target),
                InputUpdate::DevSetChannel {
                    connection_id,
                    channel,
                } => self.handle_dev_set_channel(connection_id, channel),
                InputUpdate::Equip {
                    connection_id,
                    request,
                } => self.handle_equip(connection_id, request),
                InputUpdate::Unequip {
                    connection_id,
                    request,
                } => self.handle_unequip(connection_id, request),
                InputUpdate::DevPresentationOneShot {
                    connection_id,
                    kind,
                } => self.handle_dev_presentation_oneshot(connection_id, kind),
                InputUpdate::DevResetPlayer { connection_id } => {
                    self.handle_dev_reset_player(connection_id)
                }
                InputUpdate::Respawn { connection_id } => self.handle_respawn(connection_id),
                InputUpdate::AbilityActivate {
                    connection_id,
                    request,
                } => self.handle_ability_activate(connection_id, request),
                _ => {}
            }
            return SeqDecision::Accept;
        }
        self.input_received = self.input_received.saturating_add(1);
        match update {
            InputUpdate::HeldCancel { connection_id } => {
                let Some(binding) = self.bindings.get_mut(&connection_id) else {
                    self.input_stale = self.input_stale.saturating_add(1);
                    return SeqDecision::Stale;
                };
                let entity = binding.entity;
                binding.input.held_cancel();
                if self.world.release_portal_reentry(entity) {
                    println!(
                        "6C_PORTAL reentry_unlock actor={entity} reason=held_cancel tick={}",
                        self.ticks
                    );
                }
                SeqDecision::Accept
            }
            InputUpdate::Command {
                connection_id,
                command,
            } => {
                let Some(binding) = self.bindings.get_mut(&connection_id) else {
                    self.input_stale = self.input_stale.saturating_add(1);
                    return SeqDecision::Stale;
                };
                let entity = binding.entity;
                let portal_held = command.portal_held;
                let seq = command.sequence;
                let epoch = command.input_epoch;
                let decision = binding.input.apply(command);
                match decision {
                    SeqDecision::Accept => {
                        self.input_accepted = self.input_accepted.saturating_add(1);
                        if !portal_held && self.world.release_portal_reentry(entity) {
                            println!(
                                "6C_PORTAL reentry_unlock actor={entity} reason=up_released tick={} seq={seq} epoch={epoch}",
                                self.ticks
                            );
                        }
                    }
                    SeqDecision::Duplicate => {
                        self.input_duplicate = self.input_duplicate.saturating_add(1);
                    }
                    SeqDecision::Overflow => {
                        self.input_queue_overflow = self.input_queue_overflow.saturating_add(1);
                    }
                    SeqDecision::Stale | SeqDecision::Gap | SeqDecision::OldEpoch => {
                        self.input_stale = self.input_stale.saturating_add(1)
                    }
                }
                decision
            }
            InputUpdate::InteractOpen { .. }
            | InputUpdate::InteractClose { .. }
            | InputUpdate::PortalActivate { .. }
            | InputUpdate::DevSetChannel { .. }
            | InputUpdate::Equip { .. }
            | InputUpdate::Unequip { .. }
            | InputUpdate::DevPresentationOneShot { .. }
            | InputUpdate::DevResetPlayer { .. }
            | InputUpdate::Respawn { .. }
            | InputUpdate::AbilityActivate { .. } => {
                unreachable!(
                    "interact/portal/channel/equipment/oneshot/reset/ability handled above"
                )
            }
        }
    }

    pub fn drain(
        &mut self,
        lifecycle: &mut tokio::sync::mpsc::Receiver<LifecycleCmd>,
        input: &mut tokio::sync::mpsc::Receiver<InputUpdate>,
    ) {
        while let Ok(cmd) = lifecycle.try_recv() {
            match cmd {
                LifecycleCmd::Attach {
                    connection_id,
                    replication,
                    interact,
                } => {
                    self.attach(connection_id);
                    if let Some(binding) = self.bindings.get_mut(&connection_id) {
                        binding.replication = replication;
                        binding.interact = interact;
                    }
                }
                LifecycleCmd::Enter {
                    connection_id,
                    character,
                    replication,
                    interact,
                    reply,
                } => {
                    let result = self.enter(connection_id, character, replication, interact);
                    let _ = reply.send(result);
                }
                LifecycleCmd::Detach { connection_id } => self.detach(connection_id),
            }
        }
        while let Ok(update) = input.try_recv() {
            let _ = self.apply_input(update);
        }
    }

    /// 6G.6 capacity artifact: interest-invalidation locality snapshot.
    #[must_use]
    pub fn world_interest_locality(&self) -> purgatory_common::InterestLocalitySnapshot {
        self.world.interest_locality_snapshot()
    }

    /// 6G.7B capacity artifact: replication dirty fan-out discovery snapshot.
    #[must_use]
    pub fn replication_fanout_snapshot(&self) -> purgatory_common::ReplicationFanoutSnapshot {
        self.replication_fanout_accounting.snapshot()
    }

    pub fn set_tick_overrun_hint(&mut self, overrun: bool) {
        self.tick_overrun_hint = overrun;
    }

    /// One simulation tick for every attached player. Packet count is irrelevant.
    /// Returns coarse domain timings for capacity characterization (6G.2).
    pub fn simulate_tick(&mut self, dt: f32) -> TickDomainSample {
        let tick_t0 = std::time::Instant::now();
        let mut sample = TickDomainSample::default();
        self.persist_enqueue_us = 0;
        let detail = purgatory_common::capacity_detail_enabled();

        let tick = SimulationTick::from_count(self.ticks.saturating_add(1));
        let map_a = self.map_a_address();

        let mut drain_us = purgatory_simulation::DrainApplyTiming::default();
        if detail {
            // Phase 7.5A: fold begin_tick / gauge refresh into lifecycle leaf.
            let t0 = std::time::Instant::now();
            self.world.begin_tick(tick);
            self.load_pressure.maintain(&mut self.world, map_a, tick);
            if !self.load_pressure.is_active() {
                self.maybe_arm_runtime_probe(tick);
            }
            sample.entity_lifecycle += t0.elapsed();
            drain_us = drain_us.saturating_add(self.world.drain_critical_scheduler());
        } else {
            let services_t0 = std::time::Instant::now();
            self.world.begin_tick(tick);
            self.load_pressure.maintain(&mut self.world, map_a, tick);
            if !self.load_pressure.is_active() {
                self.maybe_arm_runtime_probe(tick);
            }
            self.world.drain_critical_scheduler();
            sample.gameplay_services += services_t0.elapsed();
        }

        let input_t0 = std::time::Instant::now();
        let ids: Vec<(ConnectionId, EntityId, PlayerInput, bool)> = self
            .bindings
            .iter_mut()
            .map(|(cid, binding)| {
                let gated = binding.input.input_gated();
                let reason = binding.input.input_gate_reason();
                let entity = binding.entity;
                let player_input = binding.input.take_for_tick();
                if gated && !binding.input.input_gated() {
                    println!("6D_INPUT unlock actor={entity} reason={reason:?}");
                }
                (*cid, entity, player_input, gated)
            })
            .collect();
        sample.commands_input += input_t0.elapsed();

        let move_t0 = std::time::Instant::now();
        for (_, entity, player_input, gated) in ids {
            if gated && let Some((_, player)) = self.world.player_parts_mut_for(entity) {
                player.velocity = [0.0, 0.0];
            }
            self.world.tick_player(entity, dt, player_input);
        }
        sample.simulation_movement += move_t0.elapsed();

        let npc_t0 = std::time::Instant::now();
        self.world.tick_npcs_with_approach(
            dt,
            Some((LIVE_COMBAT_CREATURE_AGGRO_RADIUS, 0.8, 1.2)),
        );
        self.load_pressure.drive_npc_workload(&mut self.world, tick);
        sample.npc_activity += npc_t0.elapsed();

        if detail {
            let life_t0 = std::time::Instant::now();
            self.world.maintain_portal_reentry();
            let closed = self.world.maintain_interaction_sessions();
            self.emit_closed(closed);
            self.fanout_presentation_runtime_events();
            sample.entity_lifecycle += life_t0.elapsed();
            let cad_t0 = std::time::Instant::now();
            self.world.pump_cadence();
            sample.cadence += cad_t0.elapsed();
            drain_us = drain_us.saturating_add(self.world.drain_deferred_scheduler());
            sample.scheduler += Duration::from_micros(drain_us.scheduler_us);
            sample.actions += Duration::from_micros(drain_us.actions_us);
            sample.effects += Duration::from_micros(drain_us.effects_us);
            sample.entity_lifecycle += Duration::from_micros(drain_us.lifecycle_us);
            sample.scheduler += Duration::from_micros(drain_us.other_us);
        } else {
            let services2_t0 = std::time::Instant::now();
            self.world.maintain_portal_reentry();
            let closed = self.world.maintain_interaction_sessions();
            self.emit_closed(closed);
            self.fanout_presentation_runtime_events();
            self.world.pump_cadence();
            self.world.drain_deferred_scheduler();
            sample.gameplay_services += services2_t0.elapsed();
        }

        self.ticks = tick.get();
        let pub_t = self.publish_snapshots(detail);
        sample.spatial_aoi += Duration::from_micros(pub_t.aoi_us);
        sample.replication_discover += Duration::from_micros(pub_t.discover_us);
        if detail {
            sample.replication_policy += Duration::from_micros(pub_t.policy_us);
            sample.replication_encode += Duration::from_micros(pub_t.encode_us);
            sample.replication_enqueue += Duration::from_micros(pub_t.enqueue_us);
        } else {
            sample.replication += Duration::from_micros(pub_t.replicate_us);
        }
        sample.persistence_enqueue += Duration::from_micros(self.persist_enqueue_us);
        sample.total = tick_t0.elapsed();
        sample
    }

    fn maybe_arm_runtime_probe(&mut self, tick: SimulationTick) {
        if !self.runtime_probe.enabled || self.runtime_probe.armed {
            return;
        }
        self.runtime_probe.armed = true;
        let _ = self.world.register_cadence(Cadence::EveryN { n: 30 }, 6);
        let due = tick.saturating_add_ticks(30);
        let req = RuntimeSpawnRequest::transient_at(self.map_a_address())
            .with_transform(Transform::from_position([FOOTNOTE_SPAWN_X + 6.0, 2.0]))
            .visible();
        if self
            .world
            .schedule_spawn(req, due, ScheduleOwner::World, WorkLane::Deferred)
            .is_some()
        {
            println!("6F_PROBE scheduled visible generic at tick {}", due.get());
        }
    }

    fn command_actor(
        &mut self,
        connection_id: ConnectionId,
        class: CommandClass,
    ) -> Result<EntityId, CommandDenial> {
        let Some(binding) = self.bindings.get(&connection_id) else {
            self.command_rejects_other = self.command_rejects_other.saturating_add(1);
            return Err(CommandDenial::Disconnected);
        };
        let transition = binding.input.input_gate_reason();
        let busy = self.world.active_action(binding.entity).is_some();
        match validate_command_preamble(Some(binding.entity), transition, busy, class) {
            Ok(actor) => Ok(actor),
            Err(denial) => {
                if denial.is_gate() {
                    self.command_rejects_gate = self.command_rejects_gate.saturating_add(1);
                } else {
                    self.command_rejects_other = self.command_rejects_other.saturating_add(1);
                }
                Err(denial)
            }
        }
    }

    fn begin_transition_input_barrier(
        &mut self,
        connection_id: ConnectionId,
        reason: InputGateReason,
    ) {
        let Some(binding) = self.bindings.get_mut(&connection_id) else {
            return;
        };
        let entity = binding.entity;
        binding.input.lock_transition(reason);
        if let Some((_, player)) = self.world.player_parts_mut_for(entity) {
            player.velocity = [0.0, 0.0];
        }
        println!(
            "6D_INPUT barrier actor={entity} reason={reason:?} ticks={}",
            reason.lock_ticks()
        );
    }

    fn map_b_address(&self) -> WorldAddress {
        ContentId::from_authored(MAP_SECOND_AUTHORED)
            .ok()
            .and_then(|id| {
                world_address_for_map(&self.registry, id, ChannelId::DEFAULT, InstanceId::DEFAULT)
            })
            .unwrap_or(WorldAddress::DEV)
    }

    fn map_a_address(&self) -> WorldAddress {
        ContentId::from_authored(MAP_FOOTNOTE_AUTHORED)
            .ok()
            .and_then(|id| {
                world_address_for_map(&self.registry, id, ChannelId::DEFAULT, InstanceId::DEFAULT)
            })
            .unwrap_or(WorldAddress::DEV)
    }

    fn apply_content_transition(
        &mut self,
        connection_id: ConnectionId,
        actor: EntityId,
        tr: &purgatory_content::TransitionRef,
    ) -> Result<EntityId, InteractionReject> {
        let dest_content = ContentId::from_authored(&tr.map_authored)
            .map_err(|_| InteractionReject::Unavailable)?;
        let dest_portal_content = ContentId::from_authored(&tr.portal_authored)
            .map_err(|_| InteractionReject::Unavailable)?;
        let Some(current) = self.world.address_of(actor) else {
            return Err(InteractionReject::Unavailable);
        };
        let Some(dest) = world_address_for_map(
            &self.registry,
            dest_content,
            current.channel,
            current.instance,
        ) else {
            return Err(InteractionReject::Unavailable);
        };
        let plan = map_plan(&self.registry, &tr.map_authored, dest)
            .map_err(|_| InteractionReject::Unavailable)?;
        self.world
            .ensure_map(&plan)
            .map_err(|_| InteractionReject::Unavailable)?;
        let Some(dest_portal) = self.world.entity_with_content_at(dest, dest_portal_content) else {
            return Err(InteractionReject::Unavailable);
        };
        let Some(portal_pos) = self.world.transform_of(dest_portal).map(|t| t.position) else {
            return Err(InteractionReject::Unavailable);
        };
        let pos = self
            .standing_pose_on_map(dest, portal_pos[0])
            .unwrap_or(portal_pos);
        if !self.world.transition_entity(actor, dest, pos) {
            return Err(InteractionReject::Unavailable);
        }
        let floor_id = self
            .world
            .iter_platforms()
            .find(|v| {
                self.world
                    .address_of(v.id)
                    .is_some_and(|a| a.compatible_with(dest))
                    && pos[0] >= v.aabb().min_x()
                    && pos[0] <= v.aabb().max_x()
            })
            .map(|v| v.id);
        if let Some(floor_id) = floor_id
            && let Some((_, player)) = self.world.player_parts_mut_for(actor)
        {
            player.grounded = true;
            player.grounded_on = Some(floor_id);
            player.velocity = [0.0, 0.0];
        }
        if let Some(binding) = self.bindings.get_mut(&connection_id) {
            self.interest_fanout.clear_observer(binding.entity);
            binding.interest.bump_epoch();
            if let Some(pipe) = &binding.replication {
                pipe.purge_older_than(binding.interest.epoch);
            }
            if binding.input.bump_epoch().is_none() {
                return Err(InteractionReject::Unavailable);
            }
        }
        self.begin_transition_input_barrier(connection_id, InputGateReason::MapTransition);
        self.world.lock_portal_reentry(actor, dest_portal);
        let epoch = self
            .bindings
            .get(&connection_id)
            .map(|b| b.interest.epoch)
            .unwrap_or(0);
        let pose = self
            .world
            .transform_of(actor)
            .map(|t| t.position)
            .unwrap_or(pos);
        println!(
            "6D_POSE server_dest_ready actor={actor} dest={dest} portal={dest_portal} pose=({:.3},{:.3}) epoch={epoch} before_tick=true",
            pose[0], pose[1]
        );
        self.publish_snapshots(false);
        Ok(dest_portal)
    }

    fn standing_pose_on_map(&self, address: WorldAddress, x: f32) -> Option<[f32; 2]> {
        let view = self.world.iter_platforms().find(|v| {
            self.world
                .address_of(v.id)
                .is_some_and(|a| a.compatible_with(address))
                && x >= v.aabb().min_x()
                && x <= v.aabb().max_x()
        })?;
        Some(
            PlayerState::standing_on_at(view.id, view.top_surface(), x)
                .0
                .position,
        )
    }

    fn send_equipment_result(
        tx: Option<&tokio::sync::mpsc::Sender<ServerControl>>,
        event: ServerEquipment,
    ) {
        if let Some(tx) = tx
            && tx.try_send(ServerControl::Equipment(event)).is_err()
        {
            println!("8C_EQUIP response dropped (interact channel full or closed)");
        }
    }

    fn handle_equip(&mut self, connection_id: ConnectionId, request: EquipRequest) {
        self.handle_equipment_request(
            connection_id,
            request.seq,
            request.slot,
            Some(request.content_id),
        );
    }

    fn handle_unequip(&mut self, connection_id: ConnectionId, request: UnequipRequest) {
        self.handle_equipment_request(connection_id, request.seq, request.slot, None);
    }

    fn handle_equipment_request(
        &mut self,
        connection_id: ConnectionId,
        seq: u32,
        slot: u8,
        content_id: Option<ContentId>,
    ) {
        let interact_tx = self
            .bindings
            .get(&connection_id)
            .and_then(|b| b.interact.clone());
        let Some(binding) = self.bindings.get(&connection_id) else {
            return;
        };
        match SessionInput::classify(binding.last_equipment_seq, seq) {
            SeqDecision::Duplicate => {
                if let Some(event) = binding.last_equipment_result {
                    Self::send_equipment_result(interact_tx.as_ref(), event);
                }
                return;
            }
            SeqDecision::Stale => {
                Self::send_equipment_result(
                    interact_tx.as_ref(),
                    ServerEquipment::Rejected {
                        seq,
                        reason: EquipmentRejectReason::StaleRequest,
                    },
                );
                return;
            }
            SeqDecision::Gap | SeqDecision::Overflow | SeqDecision::OldEpoch => {
                Self::send_equipment_result(
                    interact_tx.as_ref(),
                    ServerEquipment::Rejected {
                        seq,
                        reason: EquipmentRejectReason::InvalidRequest,
                    },
                );
                return;
            }
            SeqDecision::Accept => {}
        }

        let result = self.apply_equipment_mutation(connection_id, slot, content_id);
        let event = match result {
            Ok(()) => ServerEquipment::Accepted { seq },
            Err(reason) => ServerEquipment::Rejected { seq, reason },
        };
        if let Some(binding) = self.bindings.get_mut(&connection_id) {
            binding.last_equipment_seq = Some(seq);
            binding.last_equipment_result = Some(event);
        }
        Self::send_equipment_result(interact_tx.as_ref(), event);
    }

    fn send_ability_result(
        tx: Option<&tokio::sync::mpsc::Sender<ServerControl>>,
        event: ServerAbility,
    ) {
        if let Some(tx) = tx
            && tx.try_send(ServerControl::Ability(event)).is_err()
        {
            println!("9C_ABILITY response dropped (interact channel full or closed)");
        }
    }

    fn handle_ability_activate(
        &mut self,
        connection_id: ConnectionId,
        request: AbilityActivateRequest,
    ) {
        let interact_tx = self
            .bindings
            .get(&connection_id)
            .and_then(|b| b.interact.clone());
        let Some(binding) = self.bindings.get(&connection_id) else {
            return;
        };
        match SessionInput::classify(binding.last_ability_seq, request.seq) {
            SeqDecision::Duplicate => {
                if let Some(event) = binding.last_ability_result {
                    Self::send_ability_result(interact_tx.as_ref(), event);
                }
                return;
            }
            SeqDecision::Stale => {
                Self::send_ability_result(
                    interact_tx.as_ref(),
                    ServerAbility::Rejected {
                        seq: request.seq,
                        reason: AbilityCommandReject::StaleRequest,
                    },
                );
                return;
            }
            SeqDecision::Gap | SeqDecision::Overflow | SeqDecision::OldEpoch => {
                Self::send_ability_result(
                    interact_tx.as_ref(),
                    ServerAbility::Rejected {
                        seq: request.seq,
                        reason: AbilityCommandReject::InvalidRequest,
                    },
                );
                return;
            }
            SeqDecision::Accept => {}
        }

        let event = match self.apply_ability_activate(connection_id, request) {
            Ok(()) => ServerAbility::Accepted { seq: request.seq },
            Err(reason) => ServerAbility::Rejected {
                seq: request.seq,
                reason,
            },
        };
        if let Some(binding) = self.bindings.get_mut(&connection_id) {
            binding.last_ability_seq = Some(request.seq);
            binding.last_ability_result = Some(event);
        }
        Self::send_ability_result(interact_tx.as_ref(), event);
        if matches!(event, ServerAbility::Accepted { .. }) {
            self.fanout_presentation_runtime_events();
        }
    }

    fn apply_ability_activate(
        &mut self,
        connection_id: ConnectionId,
        request: AbilityActivateRequest,
    ) -> Result<(), AbilityCommandReject> {
        let actor = match self.command_actor(connection_id, CommandClass::Ability) {
            Ok(actor) => actor,
            Err(CommandDenial::Disconnected) | Err(CommandDenial::MissingActor) => {
                return Err(AbilityCommandReject::StateBlocked);
            }
            Err(CommandDenial::TransitionLocked) | Err(CommandDenial::Busy) => {
                return Err(AbilityCommandReject::StateBlocked);
            }
        };
        let def = self
            .registry
            .ability_by_id(request.ability_id)
            .cloned()
            .ok_or(AbilityCommandReject::UnknownAbility)?;
        if !self.world.ability_granted(actor, def.id) {
            return Err(AbilityCommandReject::NotGranted);
        }
        let selected = match def.activation {
            AbilityActivation::Independent => {
                if request.selected.is_some() {
                    return Err(AbilityCommandReject::InvalidActivation);
                }
                None
            }
            AbilityActivation::SelectedEntity => {
                let Some(wire) = request.selected else {
                    return Err(AbilityCommandReject::InvalidActivation);
                };
                Some(super::snapshot::from_wire_id(wire))
            }
        };
        match self.world.request_ability(
            AbilityRequest {
                actor,
                selected,
                definition: &def,
            },
            ActionGateContext::in_world(),
        ) {
            Ok(_) => Ok(()),
            Err(reason) => Err(map_ability_reject(reason)),
        }
    }

    fn apply_equipment_mutation(
        &mut self,
        connection_id: ConnectionId,
        slot: u8,
        content_id: Option<ContentId>,
    ) -> Result<(), EquipmentRejectReason> {
        let actor = match self.command_actor(connection_id, CommandClass::Equipment) {
            Ok(actor) => actor,
            Err(CommandDenial::Disconnected) | Err(CommandDenial::MissingActor) => {
                return Err(EquipmentRejectReason::StateBlocked);
            }
            Err(CommandDenial::TransitionLocked) | Err(CommandDenial::Busy) => {
                return Err(EquipmentRejectReason::StateBlocked);
            }
        };
        if self.world.kind(actor) != Some(EntityKind::Player) {
            return Err(EquipmentRejectReason::InvalidRequest);
        }
        let Some(slot) = EquipmentSlot::from_u8(slot) else {
            return Err(EquipmentRejectReason::InvalidRequest);
        };
        if let Some(content_id) = content_id {
            match authorize_equip(&self.registry, slot, content_id) {
                Ok(()) => {}
                Err(EquipmentAuthError::UnknownContent) => {
                    return Err(EquipmentRejectReason::UnknownContent);
                }
                Err(EquipmentAuthError::SlotMismatch) => {
                    return Err(EquipmentRejectReason::SlotMismatch);
                }
            }
            if !self.world.set_equipment_slot(actor, slot, Some(content_id)) {
                return Err(EquipmentRejectReason::StateBlocked);
            }
        } else if !self.world.clear_equipment_slot(actor, slot) {
            return Err(EquipmentRejectReason::StateBlocked);
        }
        Ok(())
    }

    fn handle_interact_open(&mut self, connection_id: ConnectionId, target: WireEntityId) {
        let interact_tx = self
            .bindings
            .get(&connection_id)
            .and_then(|b| b.interact.clone());
        let actor = match self.command_actor(connection_id, CommandClass::Interact) {
            Ok(actor) => actor,
            Err(CommandDenial::Disconnected) => {
                println!(
                    "6B_INTERACT validate other connection={connection_id} target={target} reason=no_binding"
                );
                return;
            }
            Err(reason) => {
                println!(
                    "6B_INTERACT rejected connection={connection_id} target={target} reason={}",
                    reason.as_str()
                );
                if let Some(tx) = interact_tx {
                    let _ = tx.try_send(ServerControl::Interact(ServerInteract::Rejected {
                        target,
                        reason: InteractRejectReason::Unavailable,
                    }));
                }
                return;
            }
        };
        let target_id = super::snapshot::from_wire_id(target);
        let event = match self.world.try_open_interaction(actor, target_id) {
            Ok(session) => {
                println!(
                    "6B_INTERACT validate opened actor={actor} target={target} session={}",
                    session.id.get()
                );
                if session.state == purgatory_simulation::InteractionSessionState::Updated {
                    ServerInteract::Updated {
                        session_id: session.id.get(),
                        target,
                    }
                } else {
                    ServerInteract::Opened {
                        session_id: session.id.get(),
                        target,
                    }
                }
            }
            Err(reason) => {
                println!(
                    "6B_INTERACT validate {} actor={actor} target={target}",
                    interact_reject_trace(reason)
                );
                ServerInteract::Rejected {
                    target,
                    reason: map_reject(reason),
                }
            }
        };
        println!("6B_INTERACT response {event:?}");
        if let Some(tx) = interact_tx {
            if tx.try_send(ServerControl::Interact(event)).is_err() {
                println!("6B_INTERACT response dropped (interact channel full or closed)");
            }
        } else {
            println!("6B_INTERACT response dropped (no interact channel)");
        }
    }

    fn handle_dev_presentation_oneshot(&mut self, connection_id: ConnectionId, kind: u8) {
        let Some(kind) = PresentationOneShotKind::from_u8(kind) else {
            println!("A5_ONESHOT reject connection={connection_id} reason=invalid_kind");
            return;
        };
        let Some(binding) = self.bindings.get(&connection_id) else {
            println!("A5_ONESHOT reject connection={connection_id} reason=no_binding");
            return;
        };
        let actor = binding.entity;
        if self.world.kind(actor) != Some(EntityKind::Player) {
            println!("A5_ONESHOT reject connection={connection_id} reason=not_player");
            return;
        }
        let started = match self.world.try_start_presentation_oneshot(actor, kind) {
            Ok(oneshot) => oneshot,
            Err(_) => {
                println!(
                    "A5_ONESHOT blocked connection={connection_id} actor={actor} kind={kind:?} reason=hurt_active"
                );
                return;
            }
        };
        let until_tick = u32::try_from(started.until_tick.get()).unwrap_or(u32::MAX);
        let event = ServerControl::PresentationOneShot(ServerPresentationOneShot {
            entity: WireEntityId {
                index: actor.index(),
                generation: actor.generation(),
            },
            kind: started.kind.as_u8(),
            until_tick,
        });
        println!(
            "A5_ONESHOT start actor={actor} kind={:?} until_tick={until_tick}",
            started.kind
        );
        self.broadcast_presentation_oneshot(event);
    }

    fn handle_dev_reset_player(&mut self, connection_id: ConnectionId) {
        let Some(binding) = self.bindings.get(&connection_id) else {
            println!("DEV_RESET reject connection={connection_id} reason=no_binding");
            return;
        };
        let actor = binding.entity;
        if self.world.kind(actor) != Some(EntityKind::Player) {
            println!("DEV_RESET reject connection={connection_id} reason=not_player");
            return;
        }
        self.world.reset_player_entity(actor);
        println!("DEV_RESET spawn actor={actor} connection={connection_id}");
    }

    fn handle_respawn(&mut self, connection_id: ConnectionId) {
        let Some(binding) = self.bindings.get(&connection_id) else {
            return;
        };
        let actor = binding.entity;
        if self.world.kind(actor) != Some(EntityKind::Player) {
            return;
        }
        if !self.world.respawn_player_entity(actor) {
            return;
        }
        if let Some(binding) = self.bindings.get_mut(&connection_id) {
            let _ = binding.input.bump_epoch();
        }
        println!("RESPAWN actor={actor} connection={connection_id}");
    }

    fn broadcast_presentation_oneshot(&self, event: ServerControl) {
        for binding in self.bindings.values() {
            if let Some(tx) = binding.interact.as_ref()
                && tx.try_send(event.clone()).is_err()
            {
                println!("A5_ONESHOT fanout dropped (interact channel full or closed)");
            }
        }
    }

    fn fanout_presentation_runtime_events(&mut self) {
        use purgatory_simulation::RuntimeEvent;
        let events = self.world.commit_runtime_events();
        for event in events {
            match event {
                RuntimeEvent::PresentationOneShotStarted {
                    entity,
                    kind,
                    until_tick,
                } => {
                    let until = u32::try_from(until_tick).unwrap_or(u32::MAX);
                    let wire = ServerControl::PresentationOneShot(ServerPresentationOneShot {
                        entity: WireEntityId {
                            index: entity.index(),
                            generation: entity.generation(),
                        },
                        kind: kind.as_u8(),
                        until_tick: until,
                    });
                    println!(
                        "9D_PRESENTATION start actor={entity} kind={kind:?} until_tick={until}"
                    );
                    self.broadcast_presentation_oneshot(wire);
                }
                RuntimeEvent::PresentationOneShotCleared { entity } => {
                    let wire = ServerControl::PresentationOneShot(ServerPresentationOneShot {
                        entity: WireEntityId {
                            index: entity.index(),
                            generation: entity.generation(),
                        },
                        kind: 0,
                        until_tick: 0,
                    });
                    println!("9D_PRESENTATION clear actor={entity}");
                    self.broadcast_presentation_oneshot(wire);
                }
                _ => {}
            }
        }
    }

    fn handle_dev_set_channel(&mut self, connection_id: ConnectionId, channel: u32) {
        if channel > DEV_CHANNEL_MAX {
            println!(
                "6D_CHANNEL reject connection={connection_id} channel={channel} reason=invalid_channel max={DEV_CHANNEL_MAX}"
            );
            return;
        }
        let Some(binding) = self.bindings.get(&connection_id) else {
            println!("6D_CHANNEL reject connection={connection_id} reason=no_binding");
            return;
        };
        if binding.input.input_gated() {
            let _ = self.command_actor(connection_id, CommandClass::DevChannel);
            println!(
                "6D_CHANNEL reject connection={connection_id} channel={channel} reason=input_gated"
            );
            return;
        }
        let actor = binding.entity;
        let Some(current) = self.world.address_of(actor) else {
            println!("6D_CHANNEL reject connection={connection_id} reason=no_address");
            return;
        };
        let dest = WorldAddress::new(current.map, ChannelId::from_raw(channel), current.instance);
        if dest == current {
            println!("6D_CHANNEL no_op actor={actor} address={current} channel={channel}");
            return;
        }
        let Some(map_def) = self.registry.map_by_map_id(dest.map) else {
            println!("6D_CHANNEL reject actor={actor} dest={dest} reason=no_map");
            return;
        };
        let Ok(plan) = map_plan(&self.registry, &map_def.authored_id, dest) else {
            println!("6D_CHANNEL reject actor={actor} dest={dest} reason=map_plan");
            return;
        };
        if self.world.ensure_map(&plan).is_err() {
            println!("6D_CHANNEL reject actor={actor} dest={dest} reason=ensure_map");
            return;
        }
        let known_before = binding.interest.known_count();
        let epoch_before = binding.interest.epoch;
        let interact_tx = binding.interact.clone();
        let existing = self.world.interaction_session_of(actor);
        let pose = self.world.transform_of(actor).map(|t| t.position);
        if !self.world.set_address(actor, dest) {
            println!("6D_CHANNEL reject actor={actor} dest={dest} reason=set_address");
            return;
        }
        if let (Some(tx), Some(session)) = (interact_tx, existing) {
            let _ = tx.try_send(ServerControl::Interact(ServerInteract::Closed {
                session_id: session.id.get(),
                reason: InteractCloseReason::AddressChanged,
            }));
            println!(
                "6D_CHANNEL session_closed actor={actor} session={} reason=AddressChanged",
                session.id.get()
            );
        }
        if let Some(pos) = pose {
            let floor_id = self
                .world
                .iter_platforms()
                .find(|v| {
                    self.world
                        .address_of(v.id)
                        .is_some_and(|a| a.compatible_with(dest))
                        && pos[0] >= v.aabb().min_x()
                        && pos[0] <= v.aabb().max_x()
                })
                .map(|v| v.id);
            if let Some(floor_id) = floor_id
                && let Some((_, player)) = self.world.player_parts_mut_for(actor)
            {
                player.grounded_on = Some(floor_id);
            }
        }
        if let Some(binding) = self.bindings.get_mut(&connection_id) {
            self.interest_fanout.clear_observer(binding.entity);
            binding.interest.bump_epoch();
            if let Some(pipe) = &binding.replication {
                pipe.purge_older_than(binding.interest.epoch);
            }
            if binding.input.bump_epoch().is_none() {
                println!("6D_CHANNEL reject actor={actor} dest={dest} reason=epoch_wrap");
                return;
            }
        }
        self.begin_transition_input_barrier(connection_id, InputGateReason::MembershipTransition);
        self.publish_snapshots(false);
        let epoch = self
            .bindings
            .get(&connection_id)
            .map(|b| b.interest.epoch)
            .unwrap_or(0);
        println!(
            "6D_CHANNEL transition actor={actor} from={current} to={dest} epoch={epoch_before}->{epoch} known_cleared={known_before} pose=({:.3},{:.3})",
            pose.map(|p| p[0]).unwrap_or(f32::NAN),
            pose.map(|p| p[1]).unwrap_or(f32::NAN)
        );
    }

    fn handle_portal_activate(&mut self, connection_id: ConnectionId, target: WireEntityId) {
        let Some(binding) = self.bindings.get(&connection_id) else {
            println!(
                "6C_PORTAL portal_activate connection={connection_id} target={target} reason=no_binding"
            );
            return;
        };
        let actor = binding.entity;
        let epoch_before = binding.input.input_epoch;
        let interact_tx = binding.interact.clone();
        let gated = binding.input.input_gated();
        let target_id = super::snapshot::from_wire_id(target);
        let addr_before = self
            .world
            .address_of(actor)
            .map(|a| a.to_string())
            .unwrap_or_else(|| "-".into());
        let locked = self.world.portal_reentry_locked(actor, target_id);
        println!(
            "6C_PORTAL activate_recv actor={actor} target={target} addr={addr_before} epoch={epoch_before} tick={} reentry_locked={locked} input_gated={gated}",
            self.ticks
        );
        if gated {
            let _ = self.command_actor(connection_id, CommandClass::Portal);
            println!(
                "6C_PORTAL rejected actor={actor} target={target} reason=input_gated locked={locked} addr={addr_before}"
            );
            if let Some(tx) = interact_tx {
                let _ = tx.try_send(ServerControl::Interact(ServerInteract::Rejected {
                    target,
                    reason: InteractRejectReason::Unavailable,
                }));
            }
            return;
        }
        if let Err(reason) = self.world.validate_portal_activate(actor, target_id) {
            println!(
                "6C_PORTAL rejected actor={actor} target={target} reason={} locked={locked} addr={addr_before}",
                interact_reject_trace(reason)
            );
            if let Some(tx) = interact_tx {
                let _ = tx.try_send(ServerControl::Interact(ServerInteract::Rejected {
                    target,
                    reason: map_reject(reason),
                }));
            }
            return;
        }
        let Some(tr) = self
            .world
            .content_id_of(target_id)
            .and_then(|id| self.registry.entity_by_id(id))
            .and_then(|def| def.transition.clone())
        else {
            println!("6C_PORTAL rejected actor={actor} target={target} reason=no_link");
            if let Some(tx) = interact_tx {
                let _ = tx.try_send(ServerControl::Interact(ServerInteract::Rejected {
                    target,
                    reason: InteractRejectReason::Unavailable,
                }));
            }
            return;
        };
        let existing = self.world.interaction_session_of(actor);
        match self.apply_content_transition(connection_id, actor, &tr) {
            Ok(dest_portal) => {
                let addr_after = self
                    .world
                    .address_of(actor)
                    .map(|a| a.to_string())
                    .unwrap_or_else(|| "-".into());
                let dest_pos = self
                    .world
                    .transform_of(dest_portal)
                    .map(|t| t.position)
                    .unwrap_or([f32::NAN, f32::NAN]);
                let pose = self
                    .world
                    .transform_of(actor)
                    .map(|t| t.position)
                    .unwrap_or([f32::NAN, f32::NAN]);
                let epoch_after = self
                    .bindings
                    .get(&connection_id)
                    .map(|b| b.input.input_epoch)
                    .unwrap_or(epoch_before);
                println!(
                    "6C_PORTAL accepted actor={actor} dest_map={} dest_portal={} dest_entity={dest_portal} addr {addr_before} -> {addr_after} pose=({:.2},{:.2}) dest_pose=({:.2},{:.2}) epoch {epoch_before} -> {epoch_after} reentry_lock=on",
                    tr.map_authored, tr.portal_authored, pose[0], pose[1], dest_pos[0], dest_pos[1]
                );
                if let (Some(tx), Some(session)) = (interact_tx, existing) {
                    let _ = tx.try_send(ServerControl::Interact(ServerInteract::Closed {
                        session_id: session.id.get(),
                        reason: InteractCloseReason::AddressChanged,
                    }));
                }
                let restore = RestoreIntent {
                    map_authored: tr.map_authored.clone(),
                    point_id: "default".into(),
                    checkpoint_id: None,
                };
                let logical = resolve_restore(&self.registry, &restore);
                self.note_persistent_restore(
                    connection_id,
                    RestoreIntent {
                        map_authored: logical.map_authored,
                        point_id: logical.point_id,
                        checkpoint_id: logical.checkpoint_id,
                    },
                );
            }
            Err(reason) => {
                println!(
                    "6C_PORTAL transition_failed actor={actor} reason={}",
                    interact_reject_trace(reason)
                );
                if let Some(tx) = interact_tx {
                    let _ = tx.try_send(ServerControl::Interact(ServerInteract::Rejected {
                        target,
                        reason: map_reject(reason),
                    }));
                }
            }
        }
    }

    fn handle_interact_close(&mut self, connection_id: ConnectionId, session_id: u32) {
        let Some(binding) = self.bindings.get(&connection_id) else {
            println!(
                "6B_INTERACT recv InteractClose connection={connection_id} session={session_id} reason=no_binding"
            );
            return;
        };
        let actor = binding.entity;
        let interact_tx = binding.interact.clone();
        let event = match self.world.close_interaction(
            actor,
            purgatory_simulation::InteractionSessionId(session_id),
        ) {
            Ok(session) => ServerInteract::Closed {
                session_id: session.id.get(),
                reason: InteractCloseReason::Requested,
            },
            Err(_) => ServerInteract::Rejected {
                target: WireEntityId {
                    index: 0,
                    generation: 0,
                },
                reason: InteractRejectReason::InvalidSession,
            },
        };
        println!("6B_INTERACT response {event:?}");
        if let Some(tx) = interact_tx {
            let _ = tx.try_send(ServerControl::Interact(event));
        }
    }

    fn emit_closed(
        &mut self,
        closed: Vec<(
            purgatory_simulation::InteractionSession,
            InteractionCloseReason,
        )>,
    ) {
        for (session, reason) in closed {
            let Some(tx) = self
                .bindings
                .values()
                .find(|b| b.entity == session.actor)
                .and_then(|b| b.interact.clone())
            else {
                continue;
            };
            let _ = tx.try_send(ServerControl::Interact(ServerInteract::Closed {
                session_id: session.id.get(),
                reason: map_close_reason(reason),
            }));
        }
    }

    /// Returns AOI / replication instrumentation splits (non-overlapping).
    fn publish_snapshots(&mut self, _detail: bool) -> PublishTimings {
        self.snapshot_sequence = self.snapshot_sequence.saturating_add(1);
        self.snapshots_built = self.snapshots_built.saturating_add(1);
        let mut last_entities = 0u16;
        let mut timings = PublishTimings::default();
        if verbose_snapshots() {
            println!(
                "snapshot seq={} tick={}",
                self.snapshot_sequence, self.ticks,
            );
        }

        // 6G.7B: dirty entity → interested observers before per-observer pack.
        let discover_t0 = std::time::Instant::now();
        let dirty: Vec<_> = self.world.drain_replication_dirty().into_iter().collect();
        let mut dirty_entities = 0u32;
        let mut dirty_transform = 0u32;
        let mut dirty_health = 0u32;
        let mut interested = 0u32;
        let entity_to_cid: HashMap<EntityId, ConnectionId> = self
            .bindings
            .iter()
            .map(|(cid, b)| (b.entity, *cid))
            .collect();
        for (subject, mask) in dirty {
            dirty_entities = dirty_entities.saturating_add(1);
            if mask.transform {
                dirty_transform = dirty_transform.saturating_add(1);
            }
            if mask.health {
                dirty_health = dirty_health.saturating_add(1);
            }
            // 7.5: iterate fan-out observers without an intermediate Vec (discover path).
            for obs in self.interest_fanout.observers(subject) {
                // Enqueue all Known interested; domain/cadence/priority policy runs at emit.
                interested = interested.saturating_add(1);
                if let Some(cid) = entity_to_cid.get(&obs)
                    && let Some(binding) = self.bindings.get_mut(cid)
                {
                    binding.interest.queue_pending_update(subject);
                }
            }
        }
        self.replication_fanout_accounting.note_dirty_pass(
            dirty_entities,
            dirty_transform,
            dirty_health,
            interested,
        );
        // Include observer-id collection in discover handoff (7.5A).
        let ids: Vec<ConnectionId> = self.bindings.keys().copied().collect();
        timings.discover_us = u64::try_from(discover_t0.elapsed().as_micros()).unwrap_or(u64::MAX);

        let mut observer_pending_updates = 0u64;
        let mut observer_pending_enters = 0u64;
        let mut cadence_deferred_updates = 0u64;
        let policy_mode = self.replication_policy_mode;
        let population = self.population_class;
        let overrun_hint = self.tick_overrun_hint;
        for cid in ids {
            // Phase 7.5A: attribute per-observer prep + post-frame counters to
            // enqueue (existing leaf) so detail remainder is not inflated by glue.
            let outer_t0 = std::time::Instant::now();
            let Some(binding) = self.bindings.get_mut(&cid) else {
                continue;
            };
            let Some(pipe) = binding.replication.clone() else {
                continue;
            };
            if !self.trace_relevance {
                self.trace_relevance = true;
                let visible = self.world.spatial_candidates(binding.entity);
                println!(
                    "6B_TRACE relevance observer={} visible={}",
                    binding.entity,
                    visible.len()
                );
            }
            let recent_bytes = self.last_observer_bytes.get(&cid).copied().unwrap_or(0);
            let overrides = super::replication_policy::RelationOverrides::default();
            let policy = PublishPolicyInput {
                mode: policy_mode,
                population,
                overrides: &overrides,
                recent_observer_bytes: recent_bytes,
                tick_overrun_hint: overrun_hint,
            };
            let prep_us = u64::try_from(outer_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
            let build_start = std::time::Instant::now();
            let stats = publish_observer_frame(
                &mut binding.interest,
                &pipe,
                &mut self.world,
                &mut self.interest_fanout,
                binding.entity,
                self.snapshot_sequence,
                self.ticks,
                binding.input.input_epoch,
                binding.input.last_acknowledged(),
                binding.input.unmatched_continuation_ticks,
                policy,
            );
            let build_us = u64::try_from(build_start.elapsed().as_micros()).unwrap_or(u64::MAX);
            let post_t0 = std::time::Instant::now();
            self.snapshot_build_time_max_us = self.snapshot_build_time_max_us.max(build_us);
            self.snapshot_build_count = self.snapshot_build_count.saturating_add(1);
            timings.aoi_us = timings.aoi_us.saturating_add(stats.aoi_us);
            timings.replicate_us = timings.replicate_us.saturating_add(stats.replicate_us);
            timings.policy_us = timings.policy_us.saturating_add(stats.policy_us);
            timings.encode_us = timings.encode_us.saturating_add(stats.encode_us);
            last_entities = last_entities.max(stats.known as u16);
            self.aoi_enters = self.aoi_enters.saturating_add(u64::from(stats.enters));
            self.aoi_leaves = self.aoi_leaves.saturating_add(u64::from(stats.leaves));
            self.aoi_updates = self.aoi_updates.saturating_add(u64::from(stats.updates));
            self.aoi_churn_reentry = self
                .aoi_churn_reentry
                .saturating_add(u64::from(stats.churn_reentry));
            self.aoi_update_bytes = self.aoi_update_bytes.saturating_add(u64::from(stats.bytes));
            self.last_observer_bytes.insert(cid, stats.bytes);
            self.oldest_pending_ticks = self.oldest_pending_ticks.max(stats.oldest_pending_ticks);
            self.max_deferred_ticks = self.max_deferred_ticks.max(stats.oldest_pending_ticks);
            self.replication_queue_depth_max = self
                .replication_queue_depth_max
                .max(u64::from(stats.queue_depth));
            if stats.mailbox_merge != 0 {
                self.writer_queue_push_fail_total =
                    self.writer_queue_push_fail_total.saturating_add(1);
            }
            if let Some(book) = &self.pressure {
                book.note_queue(cid.get(), stats.queue_depth, stats.mailbox_merge != 0);
                let encoded = stats.frame_encoded != 0;
                let enqueued = stats.frame_enqueued != 0;
                let enqueue_attempted = encoded;
                book.note_outbound_publish(
                    encoded,
                    u64::from(stats.bytes),
                    enqueue_attempted,
                    enqueued,
                );
            }
            observer_pending_updates =
                observer_pending_updates.saturating_add(u64::from(stats.pending_updates));
            observer_pending_enters = observer_pending_enters
                .saturating_add(u64::from(binding.interest.want_enter_count()));
            cadence_deferred_updates =
                cadence_deferred_updates.saturating_add(u64::from(stats.cadence_deferred));
            self.replication_fanout_accounting.note_observer_publish(
                stats.known_relationships_present,
                stats.known_relationships_scanned,
                stats.updates,
                stats.serialize_attempts,
                stats.budget_deferred_updates,
                stats.cadence_deferred,
                stats.recovery_rescues,
            );
            self.replication_fanout_accounting.note_policy(
                stats.policy_eligible,
                stats.policy_domain_suppressed,
                stats.priority_deferred,
                stats.state_coalesced,
                stats.bytes,
            );
            if !self.trace_snapshot {
                self.trace_snapshot = true;
                println!(
                    "6D_TRACE frame cid={cid} enters={} leaves={} updates={} bytes={}",
                    stats.enters, stats.leaves, stats.updates, stats.bytes
                );
            }
            let post_us = u64::try_from(post_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
            timings.enqueue_us = timings
                .enqueue_us
                .saturating_add(stats.enqueue_us)
                .saturating_add(prep_us)
                .saturating_add(post_us);
        }
        self.last_snapshot_entities = last_entities;
        self.observer_pending_updates = observer_pending_updates;
        self.observer_pending_enters = observer_pending_enters;
        self.cadence_deferred_updates = cadence_deferred_updates;
        let session_t0 = std::time::Instant::now();
        let mut session_max = 0u64;
        for binding in self.bindings.values() {
            session_max = session_max.max(binding.input.queued_len() as u64);
        }
        self.session_queue_max = self.session_queue_max.max(session_max);
        timings.enqueue_us = timings
            .enqueue_us
            .saturating_add(u64::try_from(session_t0.elapsed().as_micros()).unwrap_or(u64::MAX));
        timings
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct PublishTimings {
    aoi_us: u64,
    discover_us: u64,
    replicate_us: u64,
    policy_us: u64,
    encode_us: u64,
    enqueue_us: u64,
}

fn instantiate_dev_maps(world: &mut World, registry: &ContentRegistry) {
    for authored in [MAP_FOOTNOTE_AUTHORED, MAP_SECOND_AUTHORED] {
        let cid = ContentId::from_authored(authored).unwrap_or_else(|_| {
            panic!("authored id {authored}");
        });
        let Some(addr) =
            world_address_for_map(registry, cid, ChannelId::DEFAULT, InstanceId::DEFAULT)
        else {
            panic!("no MapId for {authored}");
        };
        let plan = map_plan(registry, authored, addr).unwrap_or_else(|err| {
            panic!("map plan {authored}: {err}");
        });
        world.ensure_map(&plan).unwrap_or_else(|err| {
            panic!("instantiate {authored}: {err:?}");
        });
    }
}

fn log_dev_interaction_fixtures(world: &purgatory_simulation::World) {
    let mut n = 0u32;
    for id in world.iter() {
        if world.interactable_of(id).is_none() {
            continue;
        }
        n += 1;
        let pos = world
            .transform_of(id)
            .map(|t| t.position)
            .unwrap_or([f32::NAN, f32::NAN]);
        let addr = world
            .address_of(id)
            .map(|a| a.to_string())
            .unwrap_or_else(|| "-".into());
        println!(
            "6B_TRACE spawn id={id} pos=({:.3},{:.3}) address={addr}",
            pos[0], pos[1]
        );
    }
    println!("6B_TRACE spawn count={n} (startup once, not per tick)");
}

fn verbose_snapshots() -> bool {
    std::env::var_os("PURGATORY_NET_VERBOSE").is_some()
}

fn map_reject(reason: InteractionReject) -> InteractRejectReason {
    match reason {
        InteractionReject::TargetMissing => InteractRejectReason::TargetMissing,
        InteractionReject::StaleId => InteractRejectReason::StaleId,
        InteractionReject::WrongAddress => InteractRejectReason::WrongAddress,
        InteractionReject::OutOfRange => InteractRejectReason::OutOfRange,
        InteractionReject::NotInteractable => InteractRejectReason::NotInteractable,
        InteractionReject::Unavailable => InteractRejectReason::Unavailable,
        InteractionReject::InvalidSession => InteractRejectReason::InvalidSession,
        InteractionReject::ReentryLocked => InteractRejectReason::OutOfRange,
    }
}

fn interact_reject_trace(reason: InteractionReject) -> &'static str {
    match reason {
        InteractionReject::TargetMissing => "target_missing",
        InteractionReject::StaleId => "stale_id",
        InteractionReject::WrongAddress => "wrong_address",
        InteractionReject::OutOfRange => "out_of_range",
        InteractionReject::NotInteractable => "not_interactable",
        InteractionReject::Unavailable => "unavailable",
        InteractionReject::InvalidSession => "invalid_session",
        InteractionReject::ReentryLocked => "reentry_locked",
    }
}

fn map_close_reason(reason: InteractionCloseReason) -> InteractCloseReason {
    match reason {
        InteractionCloseReason::Requested => InteractCloseReason::Requested,
        InteractionCloseReason::TargetGone => InteractCloseReason::TargetGone,
        InteractionCloseReason::AddressChanged => InteractCloseReason::AddressChanged,
        InteractionCloseReason::OutOfRange => InteractCloseReason::OutOfRange,
        InteractionCloseReason::Disconnected => InteractCloseReason::Disconnected,
    }
}

impl Default for GameplayOwner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_protocol::{ReplicationRecord, SnapshotEntity, decode_replication_frame};
    use purgatory_simulation::{ActionKind, ActionPhase};
    use std::collections::HashMap;

    struct ReplicaView {
        epoch: u32,
        sequence: u32,
        local_player_entity: WireEntityId,
        last_acknowledged_input_sequence: u32,
        input_epoch: u16,
        local_grounded: bool,
        local_map: u32,
        local_channel: u32,
        local_instance: u32,
        entities: HashMap<WireEntityId, SnapshotEntity>,
        saw_update_before_enter: bool,
    }

    impl ReplicaView {
        fn new() -> Self {
            Self {
                epoch: 0,
                sequence: 0,
                local_player_entity: WireEntityId {
                    index: 0,
                    generation: 0,
                },
                last_acknowledged_input_sequence: 0,
                input_epoch: 0,
                local_grounded: false,
                local_map: 0,
                local_channel: 0,
                local_instance: 0,
                entities: HashMap::new(),
                saw_update_before_enter: false,
            }
        }

        fn apply_payload(&mut self, payload: &[u8]) {
            let frame = decode_replication_frame(payload).expect("frame");
            if frame.observer_baseline_epoch < self.epoch {
                return;
            }
            if frame.observer_baseline_epoch > self.epoch {
                self.entities.clear();
                self.epoch = frame.observer_baseline_epoch;
            }
            self.sequence = frame.snapshot_sequence;
            self.local_player_entity = frame.local_player_entity;
            self.last_acknowledged_input_sequence = frame.last_acknowledged_input_sequence;
            self.input_epoch = frame.input_epoch;
            self.local_grounded = frame.local_grounded;
            self.local_map = frame.local_map;
            self.local_channel = frame.local_channel;
            self.local_instance = frame.local_instance;
            for rec in frame.records {
                match rec {
                    ReplicationRecord::Enter { entity, .. } => {
                        self.entities.insert(entity.entity_id, entity);
                    }
                    ReplicationRecord::Update {
                        entity_id,
                        position,
                        velocity,
                        ..
                    } => {
                        if let Some(e) = self.entities.get_mut(&entity_id) {
                            if let Some(p) = position {
                                e.position = p;
                            }
                            if let Some(v) = velocity {
                                e.velocity = v;
                            }
                        } else {
                            self.saw_update_before_enter = true;
                        }
                    }
                    ReplicationRecord::Leave { entity_id } => {
                        self.entities.remove(&entity_id);
                    }
                }
            }
        }

        fn player_count(&self) -> usize {
            self.entities
                .values()
                .filter(|e| e.kind == purgatory_protocol::ReplicatedKind::Player)
                .count()
        }
    }

    fn bind_pipe(owner: &mut GameplayOwner, id: ConnectionId) -> ReplicationPipe {
        let (pipe, _rx) = ReplicationPipe::new();
        owner.bindings.get_mut(&id).unwrap().replication = Some(pipe.clone());
        pipe
    }

    fn drain(pipe: &ReplicationPipe, view: &mut ReplicaView) {
        while let Some(frame) = pipe.pop() {
            view.apply_payload(&frame.payload);
        }
    }

    fn cmd(seq: u32, axis: MoveAxis, jump: bool, down: bool) -> InputCommand {
        InputCommand {
            input_epoch: 0,
            sequence: seq,
            move_axis: axis,
            jump_pressed: jump,
            down_held: down,
            portal_held: false,
        }
    }

    fn latch_cmd(epoch: u16, seq: u32, portal_held: bool) -> InputCommand {
        InputCommand {
            input_epoch: epoch,
            sequence: seq,
            move_axis: MoveAxis::Neutral,
            jump_pressed: false,
            down_held: false,
            portal_held,
        }
    }

    fn cmd_epoch(epoch: u16, seq: u32, axis: MoveAxis, jump: bool, down: bool) -> InputCommand {
        InputCommand {
            input_epoch: epoch,
            sequence: seq,
            move_axis: axis,
            jump_pressed: jump,
            down_held: down,
            portal_held: false,
        }
    }

    fn command_update(id: ConnectionId, command: InputCommand) -> InputUpdate {
        InputUpdate::Command {
            connection_id: id,
            command,
        }
    }

    fn expire_input_gate(owner: &mut GameplayOwner) {
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        let n = purgatory_simulation::map_transition_input_lock_ticks()
            .max(purgatory_simulation::membership_transition_input_lock_ticks())
            .saturating_add(1);
        for _ in 0..n {
            owner.simulate_tick(dt);
        }
    }

    #[test]
    fn first_sequence_must_be_one() {
        let mut s = SessionInput::new();
        assert_eq!(
            s.apply(cmd(2, MoveAxis::Right, false, false)),
            SeqDecision::Gap
        );
        assert_eq!(
            s.apply(cmd(1, MoveAxis::Right, false, false)),
            SeqDecision::Accept
        );
        assert_eq!(s.last_received_seq, Some(1));
        assert_eq!(s.last_acknowledged_seq, None);
        assert_eq!(s.queued_len(), 1);
    }

    #[test]
    fn duplicate_is_ignored() {
        let mut s = SessionInput::new();
        assert_eq!(
            s.apply(cmd(1, MoveAxis::Right, false, false)),
            SeqDecision::Accept
        );
        assert_eq!(
            s.apply(cmd(1, MoveAxis::Left, true, true)),
            SeqDecision::Duplicate
        );
        assert_eq!(s.queued_len(), 1);
        assert_eq!(s.move_axis, MoveAxis::Neutral);
    }

    #[test]
    fn stale_is_ignored() {
        let mut s = SessionInput::new();
        s.apply(cmd(1, MoveAxis::Neutral, false, false));
        s.apply(cmd(2, MoveAxis::Neutral, false, false));
        assert_eq!(
            s.apply(cmd(1, MoveAxis::Right, true, false)),
            SeqDecision::Stale
        );
        assert_eq!(s.queued_len(), 2);
    }

    #[test]
    fn wrap_is_stale_no_wrap_within_session() {
        assert_eq!(
            SessionInput::classify(Some(u32::MAX), 0),
            SeqDecision::Stale
        );
        assert_eq!(SessionInput::classify(None, 0), SeqDecision::Gap);
        assert_eq!(SessionInput::classify(None, 1), SeqDecision::Accept);
        assert_eq!(SessionInput::classify(Some(1), 2), SeqDecision::Accept);
        assert_eq!(SessionInput::classify(Some(1), 3), SeqDecision::Gap);
    }

    #[test]
    fn right_then_neutral_consumes_one_per_tick() {
        let mut s = SessionInput::new();
        s.apply(cmd(1, MoveAxis::Right, false, false));
        s.apply(cmd(2, MoveAxis::Neutral, false, false));
        let first = s.take_for_tick();
        assert_eq!(first.move_axis, 1);
        assert_eq!(s.last_acknowledged(), 1);
        let second = s.take_for_tick();
        assert_eq!(second.move_axis, 0);
        assert_eq!(s.last_acknowledged(), 2);
    }

    #[test]
    fn jump_consumed_once_then_continuation_is_not_a_jump() {
        let mut s = SessionInput::new();
        s.apply(cmd(1, MoveAxis::Neutral, true, false));
        let first = s.take_for_tick();
        let second = s.take_for_tick();
        assert!(first.jump_pressed);
        assert!(!second.jump_pressed);
        assert_eq!(s.unmatched_continuation_ticks, 1);
    }

    #[test]
    fn duplicate_jump_does_not_double_jump() {
        let mut s = SessionInput::new();
        s.apply(cmd(1, MoveAxis::Neutral, true, false));
        s.apply(cmd(1, MoveAxis::Neutral, true, false));
        let input = s.take_for_tick();
        assert!(input.jump_pressed);
        assert!(!s.take_for_tick().jump_pressed);
    }

    #[test]
    fn queued_jump_is_not_or_ed_into_earlier_tick() {
        let mut s = SessionInput::new();
        s.apply(cmd(1, MoveAxis::Right, false, false));
        s.apply(cmd(2, MoveAxis::Neutral, true, false));
        let first = s.take_for_tick();
        assert_eq!(first.move_axis, 1);
        assert!(!first.jump_pressed);
        let second = s.take_for_tick();
        assert_eq!(second.move_axis, 0);
        assert!(second.jump_pressed);
    }

    #[test]
    fn continuation_debt_saturates_and_does_not_wrap() {
        let mut s = SessionInput::new();
        s.unmatched_continuation_ticks = u16::MAX;
        let _ = s.take_for_tick();
        assert_eq!(s.unmatched_continuation_ticks, u16::MAX);
    }

    #[test]
    fn k_zero_queue_depth_three_does_not_collapse() {
        let mut s = SessionInput::new();
        s.apply(cmd(1, MoveAxis::Right, false, false));
        s.apply(cmd(2, MoveAxis::Left, false, false));
        s.apply(cmd(3, MoveAxis::Neutral, false, false));
        assert_eq!(s.unmatched_continuation_ticks, 0);
        let first = s.take_for_tick();
        assert_eq!(first.move_axis, 1);
        assert_eq!(s.last_acknowledged(), 1);
        assert_eq!(s.queued_len(), 2);
        assert_eq!(s.unmatched_continuation_ticks, 0);
        assert_eq!(s.late_collapse_count, 0);
    }

    #[test]
    fn split_hol_late_collapse_keeps_remainder_debt() {
        let mut s = SessionInput::new();
        for _ in 0..5 {
            let _ = s.take_for_tick();
        }
        assert_eq!(s.unmatched_continuation_ticks, 5);
        s.apply(cmd(1, MoveAxis::Right, false, false));
        s.apply(cmd(2, MoveAxis::Right, false, false));
        let first = s.take_for_tick();
        assert_eq!(first.move_axis, 1);
        assert_eq!(s.last_acknowledged(), 2);
        assert_eq!(s.unmatched_continuation_ticks, 3);
        assert_eq!(s.late_collapse_count, 1);
        s.apply(cmd(3, MoveAxis::Left, false, false));
        s.apply(cmd(4, MoveAxis::Left, false, false));
        s.apply(cmd(5, MoveAxis::Neutral, false, false));
        let second = s.take_for_tick();
        assert_eq!(second.move_axis, 0);
        assert_eq!(s.last_acknowledged(), 5);
        assert_eq!(s.unmatched_continuation_ticks, 0);
        assert_eq!(s.queued_len(), 0);
    }

    #[test]
    fn late_collapse_ors_jump_and_acks_prefix_in_one_tick() {
        let mut s = SessionInput::new();
        for _ in 0..3 {
            let _ = s.take_for_tick();
        }
        s.apply(cmd(1, MoveAxis::Right, false, false));
        s.apply(cmd(2, MoveAxis::Right, true, false));
        s.apply(cmd(3, MoveAxis::Neutral, false, false));
        let input = s.take_for_tick();
        assert!(input.jump_pressed);
        assert_eq!(input.move_axis, 0);
        assert_eq!(s.last_acknowledged(), 3);
        assert_eq!(s.unmatched_continuation_ticks, 0);
        assert_eq!(s.late_collapse_count, 1);
    }

    #[test]
    fn held_cancel_is_idempotent() {
        let mut s = SessionInput::new();
        s.apply(cmd(1, MoveAxis::Right, false, true));
        s.apply(cmd(2, MoveAxis::Right, false, true));
        s.apply(cmd(3, MoveAxis::Left, true, false));
        s.held_cancel();
        assert_eq!(s.queued_len(), 0);
        assert_eq!(s.move_axis, MoveAxis::Neutral);
        assert!(!s.down_held);
        assert_eq!(s.last_acknowledged_seq, Some(3));
        assert_eq!(s.unmatched_continuation_ticks, 0);
        assert_eq!(s.held_cancel_count, 1);
        s.held_cancel();
        assert_eq!(s.last_acknowledged_seq, Some(3));
        assert_eq!(s.queued_len(), 0);
        assert_eq!(s.held_cancel_count, 2);
        assert_eq!(s.take_for_tick().move_axis, 0);
    }

    #[test]
    fn epoch_at_max_next_bump_does_not_wrap() {
        let mut s = SessionInput::new();
        s.input_epoch = u16::MAX;
        assert!(s.bump_epoch().is_none());
        assert_eq!(s.input_epoch, u16::MAX);
        assert_eq!(s.last_received_seq, None);
    }

    #[test]
    fn old_epoch_is_rejected() {
        let mut s = SessionInput::new();
        assert_eq!(s.bump_epoch(), Some(1));
        let mut old = cmd(1, MoveAxis::Right, false, false);
        old.input_epoch = 0;
        assert_eq!(s.apply(old), SeqDecision::OldEpoch);
        let mut next = cmd(1, MoveAxis::Right, false, false);
        next.input_epoch = 1;
        assert_eq!(s.apply(next), SeqDecision::Accept);
    }

    #[test]
    fn transition_lock_neutralizes_and_ignores_queued_movement() {
        let mut s = SessionInput::new();
        assert_eq!(
            s.apply(cmd(1, MoveAxis::Right, false, false)),
            SeqDecision::Accept
        );
        assert_eq!(s.take_for_tick().move_axis, 1);
        assert_eq!(s.bump_epoch(), Some(1));
        s.lock_transition(InputGateReason::MapTransition);
        assert!(s.input_gated());
        assert_eq!(s.input_gate_reason(), Some(InputGateReason::MapTransition));
        assert_eq!(s.queued_len(), 0);
        let mut held = cmd(1, MoveAxis::Right, true, true);
        held.input_epoch = 1;
        assert_eq!(s.apply(held), SeqDecision::Accept);
        assert_eq!(s.queued_len(), 1);
        let ticks = InputGateReason::MapTransition.lock_ticks();
        for i in 0..ticks {
            let input = s.take_for_tick();
            assert_eq!(input.move_axis, 0, "tick {i} must stay idle while gated");
            assert!(!input.jump_pressed);
            assert!(!input.down_held);
            assert_eq!(s.move_axis, MoveAxis::Neutral);
        }
        assert!(!s.input_gated());
        assert_eq!(s.input_gate_reason(), None);
        assert_eq!(s.queued_len(), 0);
        let continued = s.take_for_tick();
        assert_eq!(
            continued.move_axis, 0,
            "stale gated command must not become continuation"
        );
        let mut resume = cmd(2, MoveAxis::Right, false, false);
        resume.input_epoch = 1;
        assert_eq!(s.apply(resume), SeqDecision::Accept);
        assert_eq!(s.take_for_tick().move_axis, 1);
    }

    #[test]
    fn old_epoch_still_rejected_while_gated() {
        let mut s = SessionInput::new();
        assert_eq!(s.bump_epoch(), Some(1));
        s.lock_transition(InputGateReason::MapTransition);
        let mut old = cmd(1, MoveAxis::Right, false, false);
        old.input_epoch = 0;
        assert_eq!(s.apply(old), SeqDecision::OldEpoch);
        assert_eq!(s.take_for_tick().move_axis, 0);
    }

    #[test]
    fn reconnect_starts_clean() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        owner.attach(a);
        owner.apply_input(command_update(a, cmd(1, MoveAxis::Right, true, true)));
        let old = owner.entity_of(a).expect("entity");
        owner.detach(a);
        assert!(!owner.contains_entity(old));
        assert_eq!(owner.player_count(), 0);
        owner.attach(a);
        let new = owner.entity_of(a).expect("new");
        assert_ne!(old, new);
        assert!(!owner.contains_entity(old), "stale EntityId is dead");
        assert_eq!(owner.last_seq(a), None);
        let grounded = owner.world().player_body_of(new).unwrap();
        assert!(grounded.grounded);
        assert_eq!(grounded.velocity, [0.0, 0.0]);
    }

    #[test]
    fn character_occupancy_rejects_second_session_and_reconnect_gets_new_entity() {
        let mut owner = GameplayOwner::new();
        let character = PersistentCharacter::new_default(CharacterId::from_raw(7));
        let a = ConnectionId::from_raw(1);
        let b = ConnectionId::from_raw(2);
        assert_eq!(owner.enter(a, character.clone(), None, None), Ok(()));
        let first = owner.entity_of(a).expect("spawned");
        assert_eq!(
            owner.enter(b, character.clone(), None, None),
            Err(EnterError::Occupied)
        );
        assert!(owner.entity_of(b).is_none());
        owner.detach(a);
        let c = ConnectionId::from_raw(3);
        assert_eq!(owner.enter(c, character, None, None), Ok(()));
        let second = owner.entity_of(c).expect("respawned");
        assert_ne!(first, second);
        assert!(!owner.contains_entity(first));
    }

    #[test]
    fn packet_burst_does_not_create_ticks() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        owner.attach(a);
        for seq in 1..=50 {
            owner.apply_input(command_update(
                a,
                cmd(seq, MoveAxis::Right, seq == 1, false),
            ));
        }
        assert_eq!(owner.ticks(), 0);
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        owner.simulate_tick(dt);
        assert_eq!(owner.ticks(), 1);
        let body = owner
            .world()
            .player_body_of(owner.entity_of(a).unwrap())
            .unwrap();
        assert!(body.position[0] > FOOTNOTE_SPAWN_X);
    }

    #[test]
    fn a_input_does_not_move_b() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        let b = ConnectionId::from_raw(2);
        owner.attach(a);
        owner.attach(b);
        let ea = owner.entity_of(a).unwrap();
        let eb = owner.entity_of(b).unwrap();
        assert_ne!(ea, eb);
        let bx0 = owner.world().player_body_of(eb).unwrap().position[0];
        owner.apply_input(command_update(a, cmd(1, MoveAxis::Right, false, false)));
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        for _ in 0..12 {
            owner.simulate_tick(dt);
        }
        let ax = owner.world().player_body_of(ea).unwrap().position[0];
        let bx = owner.world().player_body_of(eb).unwrap().position[0];
        assert!(ax > FOOTNOTE_SPAWN_X);
        assert!((bx - bx0).abs() < 0.05, "B must stay put");
    }

    #[test]
    fn left_moves_negative_x() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        owner.attach(a);
        let entity = owner.entity_of(a).unwrap();
        let mut t = owner.world().transform_of(entity).unwrap();
        t.position[0] = 0.0;
        owner.world.set_transform(entity, t);
        let x0 = owner.world().player_body_of(entity).unwrap().position[0];
        owner.apply_input(command_update(a, cmd(1, MoveAxis::Left, false, false)));
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        for _ in 0..12 {
            owner.simulate_tick(dt);
        }
        let x = owner.world().player_body_of(entity).unwrap().position[0];
        assert!(x < x0 - 0.2, "left should decrease x: {x0} -> {x}");
    }

    #[test]
    fn jump_from_solid_leaves_ground() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        owner.attach(a);
        let grounded = owner
            .world()
            .player_body_of(owner.entity_of(a).unwrap())
            .unwrap();
        assert!(grounded.grounded);
        let y0 = grounded.position[1];
        owner.apply_input(command_update(a, cmd(1, MoveAxis::Neutral, true, false)));
        owner.simulate_tick(purgatory_simulation::TICK_DURATION.as_secs_f32());
        let body = owner
            .world()
            .player_body_of(owner.entity_of(a).unwrap())
            .unwrap();
        assert!(
            !body.grounded && (body.velocity[1] > 0.0 || body.position[1] > y0 + 0.05),
            "jump from solid: grounded={} vy={} y0={y0} y={}",
            body.grounded,
            body.velocity[1],
            body.position[1]
        );
    }

    #[test]
    fn down_jump_on_oneway_uses_drop_through() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        owner.attach(a);
        assert!(owner.stand_on_first_oneway(a));
        let before = owner
            .world()
            .player_body_of(owner.entity_of(a).unwrap())
            .unwrap();
        assert!(before.grounded);
        let platform = before.grounded_on;
        owner.apply_input(command_update(a, cmd(1, MoveAxis::Neutral, true, true)));
        owner.simulate_tick(purgatory_simulation::TICK_DURATION.as_secs_f32());
        let body = owner
            .world()
            .player_body_of(owner.entity_of(a).unwrap())
            .unwrap();
        assert!(!body.grounded);
        assert_eq!(body.ignored_platform, platform);
        assert!(body.velocity[1] < 0.0);
    }

    #[test]
    fn replication_queue_does_not_block_ticks() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        let b = ConnectionId::from_raw(2);
        owner.attach(a);
        owner.attach(b);
        let pipe_a = bind_pipe(&mut owner, a);
        let pipe_b = bind_pipe(&mut owner, b);
        assert!(owner.set_player_x(a, -8.0));
        assert!(owner.set_player_x(b, -8.0));
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        for _ in 0..40 {
            owner.simulate_tick(dt);
        }
        assert_eq!(owner.ticks(), 40);
        assert_eq!(owner.snapshot_sequence, 40);
        assert!(pipe_a.len() <= super::super::replication::WRITER_QUEUE_CAP);
        assert!(pipe_b.len() <= super::super::replication::WRITER_QUEUE_CAP);
        let mut view_a = ReplicaView::new();
        drain(&pipe_a, &mut view_a);
        let mut view_b = ReplicaView::new();
        drain(&pipe_b, &mut view_b);
        assert_eq!(
            view_a
                .entities
                .values()
                .filter(|e| e.kind == purgatory_protocol::ReplicatedKind::Player)
                .count(),
            2
        );
        assert_eq!(
            view_b
                .entities
                .values()
                .filter(|e| e.kind == purgatory_protocol::ReplicatedKind::Player)
                .count(),
            2
        );
        assert_ne!(view_a.local_player_entity, view_b.local_player_entity);
        assert_eq!(
            view_a
                .entities
                .values()
                .filter(|e| e.kind == purgatory_protocol::ReplicatedKind::Interactable)
                .count(),
            2,
            "AOI at x=-8 must include Map A switch and chest"
        );
        assert_eq!(
            view_a
                .entities
                .values()
                .filter(|e| e.kind == purgatory_protocol::ReplicatedKind::Portal)
                .count(),
            1,
            "AOI at x=-8 must include the Map A portal as ReplicatedKind::Portal"
        );
        assert_eq!(
            view_a.local_player_entity,
            super::super::snapshot::to_wire_id(owner.entity_of(a).unwrap())
        );
        for _ in 0..8 {
            owner.simulate_tick(dt);
        }
        assert_eq!(owner.ticks(), 48);
        drain(&pipe_b, &mut view_b);
        assert!(view_b.sequence >= 1);
    }

    #[test]
    fn two_clients_movement_appears_in_shared_snapshot() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        let b = ConnectionId::from_raw(2);
        owner.attach(a);
        owner.attach(b);
        let pipe = bind_pipe(&mut owner, a);
        assert!(owner.set_player_x(a, 0.0));
        assert!(owner.set_player_x(b, 0.0));
        owner.apply_input(command_update(a, cmd(1, MoveAxis::Right, false, false)));
        owner.apply_input(command_update(b, cmd(1, MoveAxis::Left, false, false)));
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        let mut view = ReplicaView::new();
        for _ in 0..12 {
            owner.simulate_tick(dt);
            drain(&pipe, &mut view);
        }
        assert_eq!(
            view.entities
                .values()
                .filter(|e| e.kind == purgatory_protocol::ReplicatedKind::Player)
                .count(),
            2
        );
        let ea = super::super::snapshot::to_wire_id(owner.entity_of(a).unwrap());
        let eb = super::super::snapshot::to_wire_id(owner.entity_of(b).unwrap());
        let ax = view.entities.get(&ea).unwrap().position[0];
        let bx = view.entities.get(&eb).unwrap().position[0];
        assert!(ax > 0.2, "A right from 0, got {ax}");
        assert!(bx < -0.2, "B left from 0, got {bx}");
    }

    #[test]
    fn snapshot_relevance_excludes_incompatible_world_address() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        let b = ConnectionId::from_raw(2);
        owner.attach(a);
        owner.attach(b);
        let pipe = bind_pipe(&mut owner, a);
        let other = purgatory_simulation::WorldAddress::new(
            purgatory_simulation::MapId::DEV,
            purgatory_simulation::ChannelId::DEFAULT,
            purgatory_simulation::InstanceId::from_raw(2),
        );
        assert!(owner.set_entity_address(b, other));
        owner.simulate_tick(purgatory_simulation::TICK_DURATION.as_secs_f32());
        let mut view = ReplicaView::new();
        drain(&pipe, &mut view);
        let ea = super::super::snapshot::to_wire_id(owner.entity_of(a).unwrap());
        let eb = super::super::snapshot::to_wire_id(owner.entity_of(b).unwrap());
        assert!(view.entities.contains_key(&ea));
        assert!(!view.entities.contains_key(&eb));
        assert!(owner.contains_entity(owner.entity_of(b).unwrap()));
    }

    #[test]
    fn channel_transition_isolates_then_rejoins_without_despawn() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        let b = ConnectionId::from_raw(2);
        owner.attach(a);
        owner.attach(b);
        let pipe_a = bind_pipe(&mut owner, a);
        let pipe_b = bind_pipe(&mut owner, b);
        assert!(owner.set_player_x(a, -8.0));
        assert!(owner.set_player_x(b, -8.0));
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        owner.simulate_tick(dt);
        let mut view_a = ReplicaView::new();
        let mut view_b = ReplicaView::new();
        drain(&pipe_a, &mut view_a);
        drain(&pipe_b, &mut view_b);
        assert_eq!(view_a.player_count(), 2);
        assert_eq!(view_b.player_count(), 2);
        assert_eq!(view_a.local_channel, 0);
        assert_eq!(view_b.local_channel, 0);

        let entity_a = owner.entity_of(a).unwrap();
        let entity_b = owner.entity_of(b).unwrap();
        let pose_b = owner.world().transform_of(entity_b).unwrap().position;
        let epoch_a0 = owner.bindings.get(&a).unwrap().interest.epoch;
        let epoch_b0 = owner.bindings.get(&b).unwrap().interest.epoch;
        let known_b0 = owner.bindings.get(&b).unwrap().interest.known_count();
        assert!(known_b0 >= 1);

        owner.apply_input(InputUpdate::DevSetChannel {
            connection_id: b,
            channel: 1,
        });
        assert_eq!(owner.entity_of(b), Some(entity_b));
        assert_eq!(
            owner.world().lifecycle_of(entity_b),
            Some(purgatory_simulation::EntityLifecycle::Active)
        );
        assert_eq!(
            owner.world().address_of(entity_b).unwrap().channel,
            purgatory_simulation::ChannelId::from_raw(1)
        );
        assert_eq!(
            owner.world().address_of(entity_a).unwrap().channel,
            purgatory_simulation::ChannelId::DEFAULT
        );
        assert_eq!(
            owner.world().transform_of(entity_b).unwrap().position,
            pose_b
        );
        let epoch_b1 = owner.bindings.get(&b).unwrap().interest.epoch;
        assert!(epoch_b1 > epoch_b0);
        assert_eq!(owner.bindings.get(&a).unwrap().interest.epoch, epoch_a0);
        assert!(
            !owner.bindings.get(&b).unwrap().interest.is_known(entity_a),
            "old-channel remote must not remain Known after B's epoch reset"
        );

        owner.simulate_tick(dt);
        drain(&pipe_a, &mut view_a);
        drain(&pipe_b, &mut view_b);
        let wa = super::super::snapshot::to_wire_id(entity_a);
        let wb = super::super::snapshot::to_wire_id(entity_b);
        assert!(view_a.entities.contains_key(&wa));
        assert!(!view_a.entities.contains_key(&wb));
        assert!(view_b.entities.contains_key(&wb));
        assert!(!view_b.entities.contains_key(&wa));
        assert_eq!(view_a.player_count(), 1);
        assert_eq!(view_b.player_count(), 1);
        assert_eq!(view_a.local_channel, 0);
        assert_eq!(view_b.local_channel, 1);
        assert!(!view_a.saw_update_before_enter);
        assert!(!view_b.saw_update_before_enter);
        assert_eq!(view_a.epoch, epoch_a0);
        assert_eq!(view_b.epoch, epoch_b1);

        let membership_ticks =
            purgatory_simulation::membership_transition_input_lock_ticks().saturating_add(1);
        for _ in 0..membership_ticks {
            owner.simulate_tick(dt);
            drain(&pipe_a, &mut view_a);
            drain(&pipe_b, &mut view_b);
        }

        owner.apply_input(InputUpdate::DevSetChannel {
            connection_id: a,
            channel: 1,
        });
        assert_eq!(owner.entity_of(a), Some(entity_a));
        assert_eq!(
            owner.world().lifecycle_of(entity_a),
            Some(purgatory_simulation::EntityLifecycle::Active)
        );
        owner.simulate_tick(dt);
        drain(&pipe_a, &mut view_a);
        drain(&pipe_b, &mut view_b);
        assert_eq!(view_a.player_count(), 2);
        assert_eq!(view_b.player_count(), 2);
        assert!(view_a.entities.contains_key(&wb));
        assert!(view_b.entities.contains_key(&wa));
        assert_eq!(view_a.local_channel, 1);
        assert_eq!(view_b.local_channel, 1);
        assert!(!view_a.saw_update_before_enter);
        assert!(!view_b.saw_update_before_enter);
        assert!(owner.bindings.get(&a).unwrap().interest.epoch > epoch_a0);

        owner.apply_input(InputUpdate::DevSetChannel {
            connection_id: b,
            channel: 0,
        });
        owner.simulate_tick(dt);
        drain(&pipe_a, &mut view_a);
        drain(&pipe_b, &mut view_b);
        assert!(!view_a.entities.contains_key(&wb));
        assert!(!view_b.entities.contains_key(&wa));
        assert_eq!(view_a.local_channel, 1);
        assert_eq!(view_b.local_channel, 0);
        assert_eq!(
            owner.world().lifecycle_of(entity_b),
            Some(purgatory_simulation::EntityLifecycle::Active)
        );
    }

    #[test]
    fn dev_reset_player_moves_bound_actor_to_spawn() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        assert!(owner.set_player_x(id, -8.0));
        let actor = owner.entity_of(id).unwrap();
        let before = owner.world().transform_of(actor).unwrap().position;
        assert!((before[0] + 8.0).abs() < 0.05);
        owner.apply_input(InputUpdate::DevResetPlayer { connection_id: id });
        assert_eq!(owner.entity_of(id), Some(actor));
        let after = owner.world().transform_of(actor).unwrap().position;
        assert!(
            (after[0] - FOOTNOTE_SPAWN_X).abs() < 0.05,
            "authoritative spawn x, got {}",
            after[0]
        );
        let (_, player) = owner.world().get_player(actor).unwrap();
        assert_eq!(player.velocity, [0.0, 0.0]);
    }

    #[test]
    fn channel_transition_rejects_out_of_range_and_is_idempotent() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        let addr = owner.world().address_of(actor).unwrap();
        let epoch = owner.bindings.get(&id).unwrap().interest.epoch;
        owner.apply_input(InputUpdate::DevSetChannel {
            connection_id: id,
            channel: 99,
        });
        assert_eq!(owner.world().address_of(actor), Some(addr));
        assert_eq!(owner.bindings.get(&id).unwrap().interest.epoch, epoch);
        owner.apply_input(InputUpdate::DevSetChannel {
            connection_id: id,
            channel: 0,
        });
        assert_eq!(owner.world().address_of(actor), Some(addr));
        assert_eq!(owner.bindings.get(&id).unwrap().interest.epoch, epoch);
    }

    #[test]
    fn channel_transition_closes_world_interaction_session() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let pose = owner.world().transform_of(actor).unwrap().position;
        let target = nearby_dev_interactable(&owner, actor);
        owner.apply_input(InputUpdate::InteractOpen {
            connection_id: id,
            target: wire_id(target),
        });
        let opened = match rx.try_recv().expect("opened") {
            ServerControl::Interact(ServerInteract::Opened { session_id, .. }) => session_id,
            other => panic!("expected Opened, got {other:?}"),
        };
        assert!(owner.world().interaction_session_of(actor).is_some());
        owner.apply_input(InputUpdate::DevSetChannel {
            connection_id: id,
            channel: 1,
        });
        assert_eq!(owner.entity_of(id), Some(actor));
        assert_eq!(
            owner.world().lifecycle_of(actor),
            Some(purgatory_simulation::EntityLifecycle::Active)
        );
        assert_eq!(owner.world().transform_of(actor).unwrap().position, pose);
        assert!(
            owner.world().interaction_session_of(actor).is_none(),
            "world-bound session must close on Channel change"
        );
        match rx.try_recv().expect("closed") {
            ServerControl::Interact(ServerInteract::Closed { session_id, reason }) => {
                assert_eq!(session_id, opened);
                assert_eq!(reason, InteractCloseReason::AddressChanged);
            }
            other => panic!("expected Closed AddressChanged, got {other:?}"),
        }
    }

    #[test]
    fn portal_preserves_current_channel_id() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let pipe = bind_pipe(&mut owner, id);
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        owner.simulate_tick(dt);
        let mut view = ReplicaView::new();
        drain(&pipe, &mut view);
        owner.apply_input(InputUpdate::DevSetChannel {
            connection_id: id,
            channel: 1,
        });
        expire_input_gate(&mut owner);
        let actor = owner.entity_of(id).unwrap();
        assert_eq!(
            owner.world().address_of(actor).unwrap().channel,
            purgatory_simulation::ChannelId::from_raw(1)
        );
        let map_before = owner.world().address_of(actor).unwrap().map;
        let dest_addr = owner.world().address_of(actor).unwrap();
        let portal = find_content_at(&owner, "entity.portal.to_second", dest_addr);
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal),
        });
        let dest = owner.world().address_of(actor).unwrap();
        assert_eq!(dest.channel, purgatory_simulation::ChannelId::from_raw(1));
        assert_eq!(dest.instance, purgatory_simulation::InstanceId::DEFAULT);
        assert_ne!(dest.map, map_before);
        assert_eq!(
            owner.world().lifecycle_of(actor),
            Some(purgatory_simulation::EntityLifecycle::Active)
        );
    }

    #[test]
    fn late_jump_after_walk_off_is_authoritative_correction() {
        // Jump was valid when the client predicted it (grounded) but invalid
        // when the delayed command is late-collapsed after Continuation left
        // the player airborne. This is compaction, not reconcile failure.
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        owner.attach(a);
        let entity = owner.entity_of(a).unwrap();
        assert!(owner.world().player_body_of(entity).unwrap().grounded);
        owner.apply_input(command_update(a, cmd(1, MoveAxis::Neutral, true, false)));
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        owner.simulate_tick(dt);
        assert!(!owner.world().player_body_of(entity).unwrap().grounded);
        owner.simulate_tick(dt);
        assert!(
            !owner.world().player_body_of(entity).unwrap().grounded,
            "one Continuation tick must keep the jumper airborne"
        );
        let debt = owner
            .bindings
            .get(&a)
            .unwrap()
            .input
            .unmatched_continuation_ticks;
        assert!(debt >= 1);
        let vy_air = owner.world().player_body_of(entity).unwrap().velocity[1];
        owner.apply_input(command_update(a, cmd(2, MoveAxis::Neutral, true, false)));
        owner.simulate_tick(dt);
        let body = owner.world().player_body_of(entity).unwrap();
        assert!(
            !body.grounded,
            "late-collapsed jump must not re-ground from air"
        );
        assert!(
            body.velocity[1] <= vy_air + 0.01,
            "airborne jump is ignored: before vy={vy_air} after vy={}",
            body.velocity[1]
        );
        assert_eq!(owner.last_seq(a), Some(2));
    }

    #[test]
    fn repeated_held_cancel_is_idempotent_on_owner() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        owner.attach(a);
        for seq in 1..=4 {
            owner.apply_input(command_update(a, cmd(seq, MoveAxis::Right, false, true)));
        }
        assert_eq!(
            owner.apply_input(InputUpdate::HeldCancel { connection_id: a }),
            SeqDecision::Accept
        );
        assert_eq!(owner.last_seq(a), Some(4));
        assert_eq!(
            owner.apply_input(InputUpdate::HeldCancel { connection_id: a }),
            SeqDecision::Accept
        );
        assert_eq!(owner.last_seq(a), Some(4));
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        owner.simulate_tick(dt);
        let body = owner
            .world()
            .player_body_of(owner.entity_of(a).unwrap())
            .unwrap();
        assert!((body.velocity[0]).abs() < 0.05);
    }

    #[test]
    fn two_recipients_snapshots_differ_in_ack_header() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        let b = ConnectionId::from_raw(2);
        owner.attach(a);
        owner.attach(b);
        let pipe_a = bind_pipe(&mut owner, a);
        let pipe_b = bind_pipe(&mut owner, b);
        owner.apply_input(command_update(a, cmd(1, MoveAxis::Right, false, false)));
        owner.apply_input(command_update(b, cmd(1, MoveAxis::Left, false, false)));
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        owner.simulate_tick(dt);
        let mut view_a = ReplicaView::new();
        let mut view_b = ReplicaView::new();
        drain(&pipe_a, &mut view_a);
        drain(&pipe_b, &mut view_b);
        assert_eq!(view_a.last_acknowledged_input_sequence, 1);
        assert_eq!(view_b.last_acknowledged_input_sequence, 1);
        assert_ne!(view_a.local_player_entity, view_b.local_player_entity);
        assert_eq!(view_a.input_epoch, 0);
        assert_eq!(view_b.input_epoch, 0);
        assert!(view_a.local_grounded);
        assert!(view_b.local_grounded);
    }

    #[test]
    fn impairment_stall_then_burst_late_collapses_and_clears_debt() {
        use purgatory_common::impairment::{INPUT_DRAIN_PER_TURN, ImpairmentLane, OverflowPolicy};
        const TICK_NS: u64 = 33_333_333;
        const STALL_NS: u64 = 500_000_000;
        let ticks = (STALL_NS / TICK_NS) as u32;
        let mut lane: ImpairmentLane<InputCommand> =
            ImpairmentLane::new(256, OverflowPolicy::Fail, 1);
        let mut session = SessionInput::new();
        let mut now = 0_u64;
        lane.begin_stall(now, STALL_NS);
        for seq in 1..=ticks {
            lane.enqueue(cmd(seq, MoveAxis::Right, false, false), now)
                .unwrap();
            let _ = session.take_for_tick();
            now += TICK_NS;
        }
        assert_eq!(session.unmatched_continuation_ticks, ticks as u16);
        assert_eq!(session.queued_len(), 0);
        assert!(lane.poll_due(STALL_NS.saturating_sub(1), 256).is_empty());
        now = STALL_NS;
        let mut released = Vec::new();
        loop {
            let batch = lane.poll_due(now, INPUT_DRAIN_PER_TURN);
            if batch.is_empty() {
                break;
            }
            released.extend(batch);
        }
        assert_eq!(released.len(), ticks as usize);
        for command in released {
            assert_eq!(session.apply(command), SeqDecision::Accept);
        }
        let collapsed = session.take_for_tick();
        assert_eq!(collapsed.move_axis, 1);
        assert_eq!(session.last_acknowledged(), ticks);
        assert_eq!(session.unmatched_continuation_ticks, 0);
        assert_eq!(session.queued_len(), 0);
        assert_eq!(session.late_collapse_count, 1);
        assert_eq!(session.late_collapse_max_batch, ticks as u16);
    }

    fn wire_id(id: EntityId) -> WireEntityId {
        WireEntityId {
            index: id.index(),
            generation: id.generation(),
        }
    }

    fn nearby_dev_interactable(owner: &GameplayOwner, actor: EntityId) -> EntityId {
        let actor_x = owner.world().transform_of(actor).unwrap().position[0];
        owner
            .world()
            .iter()
            .find(|&eid| {
                owner
                    .world()
                    .interactable_of(eid)
                    .is_some_and(|cap| cap.kind != purgatory_simulation::InteractableKind::Portal)
                    && owner.world().address_of(eid)
                        == Some(purgatory_simulation::WorldAddress::DEV)
                    && {
                        let x = owner.world().transform_of(eid).unwrap().position[0];
                        (x - actor_x).abs() < purgatory_simulation::INTERACT_RANGE
                    }
            })
            .expect("nearby DEV interactable")
    }

    fn far_dev_interactable(owner: &GameplayOwner, actor: EntityId) -> EntityId {
        let actor_x = owner.world().transform_of(actor).unwrap().position[0];
        owner
            .world()
            .iter()
            .find(|&eid| {
                owner
                    .world()
                    .interactable_of(eid)
                    .is_some_and(|cap| cap.kind != purgatory_simulation::InteractableKind::Portal)
                    && owner.world().address_of(eid)
                        == Some(purgatory_simulation::WorldAddress::DEV)
                    && {
                        let x = owner.world().transform_of(eid).unwrap().position[0];
                        (x - actor_x).abs() > purgatory_simulation::INTERACT_RANGE
                    }
            })
            .expect("far DEV interactable")
    }

    #[test]
    fn interact_open_nearby_fixture_opens() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let target = nearby_dev_interactable(&owner, actor);
        owner.apply_input(InputUpdate::InteractOpen {
            connection_id: id,
            target: wire_id(target),
        });
        match rx.try_recv().expect("response") {
            ServerControl::Interact(ServerInteract::Opened { target: opened, .. }) => {
                assert_eq!(opened, wire_id(target));
            }
            other => panic!("expected Opened, got {other:?}"),
        }
    }

    #[test]
    fn interact_open_far_fixture_rejects_out_of_range() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let target = far_dev_interactable(&owner, actor);
        owner.apply_input(InputUpdate::InteractOpen {
            connection_id: id,
            target: wire_id(target),
        });
        match rx.try_recv().expect("response") {
            ServerControl::Interact(ServerInteract::Rejected {
                reason,
                target: rejected,
            }) => {
                assert_eq!(reason, InteractRejectReason::OutOfRange);
                assert_eq!(rejected, wire_id(target));
            }
            other => panic!("expected Rejected OutOfRange, got {other:?}"),
        }
    }

    #[test]
    fn interact_open_other_player_rejects_not_interactable() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        let b = ConnectionId::from_raw(2);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(a);
        owner.attach(b);
        owner.bindings.get_mut(&a).unwrap().interact = Some(tx);
        let other = owner.entity_of(b).unwrap();
        owner.apply_input(InputUpdate::InteractOpen {
            connection_id: a,
            target: wire_id(other),
        });
        match rx.try_recv().expect("response") {
            ServerControl::Interact(ServerInteract::Rejected { reason, .. }) => {
                assert_eq!(reason, InteractRejectReason::NotInteractable);
            }
            other => panic!("expected Rejected NotInteractable, got {other:?}"),
        }
    }

    fn find_content(owner: &GameplayOwner, authored: &str) -> EntityId {
        let cid = ContentId::from_authored(authored).unwrap();
        owner
            .world()
            .iter()
            .find(|&eid| owner.world().content_id_of(eid) == Some(cid))
            .unwrap_or_else(|| panic!("missing {authored}"))
    }

    fn find_content_at(
        owner: &GameplayOwner,
        authored: &str,
        address: purgatory_simulation::WorldAddress,
    ) -> EntityId {
        let cid = ContentId::from_authored(authored).unwrap();
        owner
            .world()
            .iter()
            .find(|&eid| {
                owner.world().content_id_of(eid) == Some(cid)
                    && owner.world().address_of(eid) == Some(address)
            })
            .unwrap_or_else(|| panic!("missing {authored} at {address}"))
    }

    #[test]
    fn portal_e_does_not_activate() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let portal = find_content(&owner, "entity.portal.to_second");
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(InputUpdate::InteractOpen {
            connection_id: id,
            target: wire_id(portal),
        });
        match rx.try_recv().expect("response") {
            ServerControl::Interact(ServerInteract::Rejected { reason, .. }) => {
                assert_eq!(reason, InteractRejectReason::NotInteractable);
            }
            other => panic!("expected Rejected NotInteractable, got {other:?}"),
        }
        let actor = owner.entity_of(id).unwrap();
        assert_eq!(
            owner.world().address_of(actor).unwrap().map,
            purgatory_simulation::MapId::DEV
        );
    }

    #[test]
    fn portal_outside_zone_rejected() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let portal = find_content(&owner, "entity.portal.to_second");
        assert!(owner.set_player_x(id, 4.0));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal),
        });
        match rx.try_recv().expect("response") {
            ServerControl::Interact(ServerInteract::Rejected { reason, .. }) => {
                assert_eq!(reason, InteractRejectReason::OutOfRange);
            }
            other => panic!("expected Rejected OutOfRange, got {other:?}"),
        }
    }

    #[test]
    fn portal_a_to_b_arrives_at_linked_portal() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        let portal = find_content(&owner, "entity.portal.to_second");
        let dest_portal = find_content(&owner, "entity.portal.to_footnote");
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal),
        });
        let dest = owner.world().address_of(actor).expect("address");
        assert_eq!(dest.map, purgatory_simulation::MapId::from_raw(2));
        let pos = owner.world().transform_of(actor).unwrap().position;
        let dest_pos = owner.world().transform_of(dest_portal).unwrap().position;
        assert!(
            (pos[0] - dest_pos[0]).abs() < 0.05,
            "must arrive at Map B portal x={}, got {}",
            dest_pos[0],
            pos[0]
        );
        assert_eq!(
            owner.world().lifecycle_of(actor),
            Some(purgatory_simulation::EntityLifecycle::Active)
        );
        assert!(owner.world().interaction_session_of(actor).is_none());
        assert_eq!(owner.bindings.get(&id).unwrap().input.input_epoch, 1);
        let visible = owner.world().relevance_for(actor);
        let map_a_switch = find_content(&owner, "entity.interactable.switch");
        assert!(
            !visible.contains(&map_a_switch),
            "Map A switch must not be relevant on Map B"
        );
        assert!(visible.contains(&dest_portal));
    }

    #[test]
    fn held_movement_does_not_drift_during_portal_input_barrier() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        let portal = find_content(&owner, "entity.portal.to_second");
        let dest_portal = find_content(&owner, "entity.portal.to_footnote");
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(command_update(id, cmd(1, MoveAxis::Right, false, false)));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal),
        });
        let dest_pos = owner.world().transform_of(dest_portal).unwrap().position;
        let arrived = owner.world().transform_of(actor).unwrap().position;
        assert!((arrived[0] - dest_pos[0]).abs() < 0.05);
        assert!(owner.bindings.get(&id).unwrap().input.input_gated());
        let epoch = owner.bindings.get(&id).unwrap().input.input_epoch;
        let lock_ticks = InputGateReason::MapTransition.lock_ticks();
        for seq in 1..=u32::from(lock_ticks) {
            owner.apply_input(command_update(
                id,
                cmd_epoch(epoch, seq, MoveAxis::Right, false, false),
            ));
            owner.simulate_tick(dt);
            let pos = owner.world().transform_of(actor).unwrap().position;
            assert!(
                (pos[0] - dest_pos[0]).abs() < 0.05,
                "held Right must not move dest pose during barrier: dest={} got={}",
                dest_pos[0],
                pos[0]
            );
            let vel = owner.world().player_body_of(actor).unwrap().velocity;
            assert!(
                vel[0].abs() < 1e-4,
                "authoritative velocity must stay neutral"
            );
        }
        assert!(!owner.bindings.get(&id).unwrap().input.input_gated());
        let next_seq = u32::from(lock_ticks) + 1;
        owner.apply_input(command_update(
            id,
            cmd_epoch(epoch, next_seq, MoveAxis::Right, false, false),
        ));
        owner.simulate_tick(dt);
        let after = owner.world().transform_of(actor).unwrap().position[0];
        assert!(
            after > dest_pos[0] + 0.01,
            "held movement must resume after unlock, dest={} after={after}",
            dest_pos[0]
        );
    }

    #[test]
    fn queued_pre_transition_command_cannot_move_destination() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        let portal = find_content(&owner, "entity.portal.to_second");
        let dest_portal = find_content(&owner, "entity.portal.to_footnote");
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(command_update(id, cmd(1, MoveAxis::Right, false, false)));
        owner.apply_input(command_update(id, cmd(2, MoveAxis::Right, true, false)));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal),
        });
        let dest_x = owner.world().transform_of(dest_portal).unwrap().position[0];
        assert!((owner.world().transform_of(actor).unwrap().position[0] - dest_x).abs() < 0.05);
        let epoch = owner.bindings.get(&id).unwrap().input.input_epoch;
        owner.apply_input(command_update(
            id,
            cmd_epoch(epoch, 1, MoveAxis::Right, true, true),
        ));
        owner.simulate_tick(dt);
        let pos = owner.world().transform_of(actor).unwrap().position;
        assert!((pos[0] - dest_x).abs() < 0.05);
        let vel = owner.world().player_body_of(actor).unwrap().velocity;
        assert!(
            vel[0].abs() < 1e-3 && vel[1].abs() < 1e-3,
            "queued jump must not fire on destination during barrier, vel={vel:?}"
        );
    }

    #[test]
    fn gated_jump_and_interact_do_not_execute() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let portal = find_content(&owner, "entity.portal.to_second");
        let dest_portal = find_content(&owner, "entity.portal.to_footnote");
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal),
        });
        let dest_x = owner.world().transform_of(dest_portal).unwrap().position[0];
        let epoch = owner.bindings.get(&id).unwrap().input.input_epoch;
        owner.apply_input(command_update(
            id,
            cmd_epoch(epoch, 1, MoveAxis::Neutral, true, false),
        ));
        owner.simulate_tick(dt);
        let body = owner.world().player_body_of(actor).unwrap();
        assert!(body.grounded, "gated jump must not launch");
        assert!((owner.world().transform_of(actor).unwrap().position[0] - dest_x).abs() < 0.05);

        owner.apply_input(InputUpdate::InteractOpen {
            connection_id: id,
            target: wire_id(dest_portal),
        });
        match rx.try_recv().expect("gated interact reject") {
            ServerControl::Interact(ServerInteract::Rejected { reason, .. }) => {
                assert_eq!(reason, InteractRejectReason::Unavailable);
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
        assert!(owner.world().interaction_session_of(actor).is_none());

        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(dest_portal),
        });
        assert_eq!(
            owner.world().address_of(actor).unwrap().map,
            purgatory_simulation::MapId::from_raw(2),
            "portal during barrier must not bounce"
        );
    }

    #[test]
    fn channel_held_movement_does_not_drift_through_membership_ready() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        assert!(owner.set_player_x(id, -8.0));
        owner.apply_input(command_update(id, cmd(1, MoveAxis::Right, false, false)));
        owner.simulate_tick(dt);
        owner.simulate_tick(dt);
        let pose_before = owner.world().transform_of(actor).unwrap().position;
        owner.apply_input(InputUpdate::DevSetChannel {
            connection_id: id,
            channel: 1,
        });
        assert_eq!(
            owner.world().address_of(actor).unwrap().channel,
            purgatory_simulation::ChannelId::from_raw(1)
        );
        let epoch = owner.bindings.get(&id).unwrap().input.input_epoch;
        assert!(owner.bindings.get(&id).unwrap().input.input_gated());
        let lock_ticks = InputGateReason::MembershipTransition.lock_ticks();
        for seq in 1..=u32::from(lock_ticks) {
            owner.apply_input(command_update(
                id,
                cmd_epoch(epoch, seq, MoveAxis::Right, false, false),
            ));
            owner.simulate_tick(dt);
            let pose = owner.world().transform_of(actor).unwrap().position;
            assert!(
                (pose[0] - pose_before[0]).abs() < 0.05 && (pose[1] - pose_before[1]).abs() < 0.05,
                "channel barrier must keep pose stable"
            );
        }
        assert!(!owner.bindings.get(&id).unwrap().input.input_gated());
        let next_seq = u32::from(lock_ticks) + 1;
        owner.apply_input(command_update(
            id,
            cmd_epoch(epoch, next_seq, MoveAxis::Right, false, false),
        ));
        owner.simulate_tick(dt);
        let after = owner.world().transform_of(actor).unwrap().position[0];
        assert!(after > pose_before[0] + 0.01);
    }

    #[test]
    fn normal_movement_outside_transition_unchanged() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        let start = owner.world().transform_of(actor).unwrap().position[0];
        owner.apply_input(command_update(id, cmd(1, MoveAxis::Right, false, false)));
        owner.simulate_tick(dt);
        owner.apply_input(command_update(id, cmd(2, MoveAxis::Right, false, false)));
        owner.simulate_tick(dt);
        let after = owner.world().transform_of(actor).unwrap().position[0];
        assert!(after > start + 0.05);
        assert!(!owner.bindings.get(&id).unwrap().input.input_gated());
    }

    #[test]
    fn portal_new_epoch_first_self_enter_is_destination_pose() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let pipe = bind_pipe(&mut owner, id);
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        owner.simulate_tick(dt);
        let mut view = ReplicaView::new();
        drain(&pipe, &mut view);
        let epoch_before = owner.bindings.get(&id).unwrap().interest.epoch;
        let actor = owner.entity_of(id).unwrap();
        let portal = find_content(&owner, "entity.portal.to_second");
        let dest_portal = find_content(&owner, "entity.portal.to_footnote");
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal),
        });
        let world_pose = owner.world().transform_of(actor).unwrap().position;
        let dest_pos = owner.world().transform_of(dest_portal).unwrap().position;
        let epoch_after = owner.bindings.get(&id).unwrap().interest.epoch;
        assert!(
            epoch_after > epoch_before,
            "epoch must bump after dest pose"
        );
        assert!(
            (world_pose[0] - dest_pos[0]).abs() < 0.05,
            "world dest pose must be established before new-epoch baseline"
        );
        let mut saw_self = false;
        while let Some(queued) = pipe.pop() {
            let frame = decode_replication_frame(&queued.payload).expect("frame");
            if frame.observer_baseline_epoch != epoch_after {
                continue;
            }
            let local = frame.local_player_entity;
            let enter = frame.records.iter().find_map(|rec| match rec {
                ReplicationRecord::Enter { entity, .. } if entity.entity_id == local => {
                    Some(entity.position)
                }
                _ => None,
            });
            let pose = enter.expect("first new-epoch frame must Enter self");
            assert!(
                (pose[0] - world_pose[0]).abs() < 1e-4 && (pose[1] - world_pose[1]).abs() < 1e-4,
                "self Enter must match dest Transform ({:.3},{:.3}), got ({:.3},{:.3})",
                world_pose[0],
                world_pose[1],
                pose[0],
                pose[1]
            );
            assert!(
                (pose[0] - dest_pos[0]).abs() < 0.05,
                "self Enter must be linked portal dest, got {}",
                pose[0]
            );
            saw_self = true;
            view.apply_payload(&queued.payload);
        }
        assert!(
            saw_self,
            "new-epoch baseline must be published before the next tick"
        );
        let presented = view
            .entities
            .get(&view.local_player_entity)
            .expect("known self")
            .position;
        assert!((presented[0] - dest_pos[0]).abs() < 0.05);
    }

    #[test]
    fn portal_b_to_a_arrives_at_linked_portal() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        let a_portal = find_content(&owner, "entity.portal.to_second");
        let b_portal = find_content(&owner, "entity.portal.to_footnote");
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(a_portal),
        });
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(b_portal),
        });
        assert_eq!(
            owner.world().address_of(actor).unwrap().map,
            purgatory_simulation::MapId::from_raw(2),
            "held/reentry lock must block bounce-back"
        );
        let epoch = owner.bindings.get(&id).unwrap().input.input_epoch;
        owner.apply_input(command_update(id, latch_cmd(epoch, 1, true)));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(b_portal),
        });
        assert_eq!(
            owner.world().address_of(actor).unwrap().map,
            purgatory_simulation::MapId::from_raw(2),
            "portal_held true must keep the reentry lock"
        );
        owner.apply_input(command_update(id, latch_cmd(epoch, 2, false)));
        assert!(
            !owner.world().portal_reentry_locked(actor, b_portal),
            "Up release must clear the lock without leaving the zone"
        );
        expire_input_gate(&mut owner);
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(b_portal),
        });
        assert_eq!(
            owner.world().address_of(actor).unwrap().map,
            purgatory_simulation::MapId::DEV
        );
        let pos = owner.world().transform_of(actor).unwrap().position;
        let a_pos = owner.world().transform_of(a_portal).unwrap().position;
        assert!((pos[0] - a_pos[0]).abs() < 0.05);
    }

    #[test]
    fn portal_return_trip_does_not_require_leaving_the_zone() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        let a_portal = find_content(&owner, "entity.portal.to_second");
        let b_portal = find_content(&owner, "entity.portal.to_footnote");
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(a_portal),
        });
        let dest_x = owner.world().transform_of(b_portal).unwrap().position[0];
        let actor_x = owner.world().transform_of(actor).unwrap().position[0];
        assert!(
            (actor_x - dest_x).abs() < 0.05,
            "must remain centered on dest portal"
        );
        let epoch = owner.bindings.get(&id).unwrap().input.input_epoch;
        owner.apply_input(command_update(id, latch_cmd(epoch, 1, false)));
        expire_input_gate(&mut owner);
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(b_portal),
        });
        assert_eq!(
            owner.world().address_of(actor).unwrap().map,
            purgatory_simulation::MapId::DEV
        );
    }

    #[test]
    fn invalid_portal_destination_rejected() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let dest_portal = find_content(&owner, "entity.portal.to_footnote");
        assert!(owner.world_mut().despawn(dest_portal));
        let portal = find_content(&owner, "entity.portal.to_second");
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal),
        });
        match rx.try_recv().expect("response") {
            ServerControl::Interact(ServerInteract::Rejected { reason, .. }) => {
                assert_eq!(reason, InteractRejectReason::Unavailable);
            }
            other => panic!("expected Rejected Unavailable, got {other:?}"),
        }
        let actor = owner.entity_of(id).unwrap();
        assert_eq!(
            owner.world().address_of(actor).unwrap().map,
            purgatory_simulation::MapId::DEV
        );
    }

    #[test]
    fn generic_e_interaction_still_opens() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let switch = find_content(&owner, "entity.interactable.switch");
        assert!(owner.set_player_x(id, -17.8));
        owner.apply_input(InputUpdate::InteractOpen {
            connection_id: id,
            target: wire_id(switch),
        });
        match rx.try_recv().expect("response") {
            ServerControl::Interact(ServerInteract::Opened { .. }) => {}
            other => panic!("expected Opened, got {other:?}"),
        }
    }

    #[test]
    fn generic_e_opens_chest() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let chest = find_content(&owner, "entity.interactable.chest");
        assert!(owner.set_player_x(id, -7.4));
        owner.apply_input(InputUpdate::InteractOpen {
            connection_id: id,
            target: wire_id(chest),
        });
        match rx.try_recv().expect("response") {
            ServerControl::Interact(ServerInteract::Opened { .. }) => {}
            other => panic!("expected Opened, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_and_activation_zone_agree_on_centered_portal() {
        use purgatory_protocol::ReplicatedKind;
        use purgatory_simulation::in_portal_activation_zone;

        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        assert!(owner.set_player_x(id, 6.0));
        let actor = owner.entity_of(id).unwrap();
        let portal = find_content(&owner, "entity.portal.to_second");
        let actor_pos = owner.world().transform_of(actor).unwrap().position;
        let portal_pos = owner.world().transform_of(portal).unwrap().position;
        owner
            .world()
            .validate_portal_activate(actor, portal)
            .expect("authored portal x at standing y is inside the activation zone");
        assert!(in_portal_activation_zone(actor_pos, portal_pos));

        let visible = owner.world().relevance_for(actor);
        let snap = super::super::snapshot::build(1, 1, actor, owner.world(), &visible, 0, 0, 0);
        let snap_player = snap
            .entities
            .iter()
            .find(|e| e.entity_id == snap.local_player_entity)
            .expect("player");
        let snap_portal = snap
            .entities
            .iter()
            .find(|e| e.kind == ReplicatedKind::Portal)
            .expect("portal");
        assert_eq!(snap_player.position, actor_pos);
        assert_eq!(snap_portal.position, portal_pos);
        assert_eq!(snap_portal.entity_id, wire_id(portal));
        assert!(in_portal_activation_zone(
            snap_player.position,
            snap_portal.position
        ));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal),
        });
        assert_eq!(
            owner.world().address_of(actor).unwrap().map,
            purgatory_simulation::MapId::from_raw(2)
        );
    }

    #[test]
    fn stale_world_interact_after_portal_rejected() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let portal = find_content(&owner, "entity.portal.to_second");
        let map_a_switch = find_content(&owner, "entity.interactable.switch");
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal),
        });
        assert_eq!(
            owner.world().address_of(actor).unwrap().map,
            purgatory_simulation::MapId::from_raw(2)
        );
        expire_input_gate(&mut owner);
        owner.apply_input(InputUpdate::InteractOpen {
            connection_id: id,
            target: wire_id(map_a_switch),
        });
        match rx.try_recv().expect("stale-world interact reject") {
            ServerControl::Interact(ServerInteract::Rejected { reason, .. }) => {
                assert_eq!(reason, InteractRejectReason::WrongAddress);
            }
            other => panic!("expected WrongAddress, got {other:?}"),
        }
        assert!(owner.world().interaction_session_of(actor).is_none());
    }

    #[test]
    fn old_world_portal_after_transition_rejected() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let portal_a = find_content(&owner, "entity.portal.to_second");
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal_a),
        });
        assert_eq!(
            owner.world().address_of(actor).unwrap().map,
            purgatory_simulation::MapId::from_raw(2)
        );
        expire_input_gate(&mut owner);
        while rx.try_recv().is_ok() {}
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal_a),
        });
        match rx.try_recv().expect("old-world portal reject") {
            ServerControl::Interact(ServerInteract::Rejected { reason, .. }) => {
                assert_eq!(reason, InteractRejectReason::WrongAddress);
            }
            other => panic!("expected WrongAddress, got {other:?}"),
        }
        assert_eq!(
            owner.world().address_of(actor).unwrap().map,
            purgatory_simulation::MapId::from_raw(2),
            "must stay on destination map"
        );
    }

    #[test]
    fn repeated_portal_activation_single_transition() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        let portal = find_content(&owner, "entity.portal.to_second");
        let dest_portal = find_content(&owner, "entity.portal.to_footnote");
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal),
        });
        let dest_x = owner.world().transform_of(dest_portal).unwrap().position[0];
        assert_eq!(
            owner.world().address_of(actor).unwrap().map,
            purgatory_simulation::MapId::from_raw(2)
        );
        // Immediate retries while gated / reentry-locked must not bounce maps.
        for _ in 0..4 {
            owner.apply_input(InputUpdate::PortalActivate {
                connection_id: id,
                target: wire_id(portal),
            });
            owner.apply_input(InputUpdate::PortalActivate {
                connection_id: id,
                target: wire_id(dest_portal),
            });
        }
        assert_eq!(
            owner.world().address_of(actor).unwrap().map,
            purgatory_simulation::MapId::from_raw(2)
        );
        let pos = owner.world().transform_of(actor).unwrap().position[0];
        assert!((pos - dest_x).abs() < 0.05, "no double-teleport drift");
    }

    #[test]
    fn old_epoch_command_after_portal_is_ignored() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        let portal = find_content(&owner, "entity.portal.to_second");
        let dest_portal = find_content(&owner, "entity.portal.to_footnote");
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        assert!(owner.set_player_x(id, 6.0));
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(portal),
        });
        let dest_x = owner.world().transform_of(dest_portal).unwrap().position[0];
        let epoch = owner.bindings.get(&id).unwrap().input.input_epoch;
        assert!(epoch >= 1);
        let decision = owner.apply_input(command_update(
            id,
            cmd_epoch(0, 1, MoveAxis::Right, false, false),
        ));
        assert_eq!(decision, SeqDecision::OldEpoch);
        owner.simulate_tick(dt);
        let pos = owner.world().transform_of(actor).unwrap().position[0];
        assert!((pos - dest_x).abs() < 0.05);
    }

    #[test]
    fn detach_then_old_connection_commands_are_stale() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let old = owner.entity_of(id).unwrap();
        owner.apply_input(command_update(id, cmd(1, MoveAxis::Right, false, false)));
        owner.detach(id);
        assert!(!owner.contains_entity(old));
        let decision = owner.apply_input(command_update(id, cmd(2, MoveAxis::Left, true, false)));
        assert_eq!(decision, SeqDecision::Stale);
        assert_eq!(owner.player_count(), 0);
        owner.apply_input(InputUpdate::PortalActivate {
            connection_id: id,
            target: wire_id(old),
        });
        assert_eq!(owner.player_count(), 0);
    }

    #[test]
    fn reconnect_entity_not_controlled_by_prior_detached_session_context() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        owner.attach(a);
        let e1 = owner.entity_of(a).unwrap();
        owner.apply_input(command_update(a, cmd(1, MoveAxis::Right, false, false)));
        owner.detach(a);
        owner.attach(a);
        let e2 = owner.entity_of(a).unwrap();
        assert_ne!(e1, e2);
        assert!(!owner.contains_entity(e1));
        // Fresh session starts at seq gap unless seq=1; old high seq is Gap/Stale.
        let decision = owner.apply_input(command_update(a, cmd(99, MoveAxis::Left, false, false)));
        assert!(
            matches!(decision, SeqDecision::Gap | SeqDecision::Stale),
            "stale reconnect seq must not accept, got {decision:?}"
        );
        let start = owner.world().transform_of(e2).unwrap().position[0];
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        owner.simulate_tick(dt);
        let after = owner.world().transform_of(e2).unwrap().position[0];
        assert!(
            (after - start).abs() < 0.05,
            "rejected seq must not move e2"
        );
    }

    #[test]
    fn stale_entity_generation_interact_cannot_hit_replacement() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let chest = find_content(&owner, "entity.interactable.chest");
        let stale = chest;
        assert!(owner.world_mut().despawn(chest));
        // Reuse slot via a fresh interactable spawn near player if possible; despawned
        // generation must fail even before a replacement exists.
        assert!(owner.set_player_x(id, -7.4));
        owner.apply_input(InputUpdate::InteractOpen {
            connection_id: id,
            target: wire_id(stale),
        });
        match rx.try_recv().expect("stale id reject") {
            ServerControl::Interact(ServerInteract::Rejected { reason, .. }) => {
                assert!(
                    matches!(
                        reason,
                        InteractRejectReason::StaleId | InteractRejectReason::Unavailable
                    ),
                    "got {reason:?}"
                );
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_session_input_does_not_reapply_movement_intent() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        let start = owner.world().transform_of(actor).unwrap().position[0];
        assert_eq!(
            owner.apply_input(command_update(id, cmd(1, MoveAxis::Right, false, false))),
            SeqDecision::Accept
        );
        assert_eq!(
            owner.apply_input(command_update(id, cmd(1, MoveAxis::Right, false, false))),
            SeqDecision::Duplicate
        );
        owner.simulate_tick(dt);
        let mid = owner.world().transform_of(actor).unwrap().position[0];
        assert!(mid > start + 0.01);
        // Duplicate was ignored: only one accepted intent drives the tick.
        assert_eq!(owner.input_duplicate, 1);
    }

    fn debug_sword() -> ContentId {
        ContentId::from_authored("equipment.debug.practice_sword").unwrap()
    }

    fn recv_equipment(rx: &mut tokio::sync::mpsc::Receiver<ServerControl>) -> ServerEquipment {
        match rx.try_recv().expect("equipment result") {
            ServerControl::Equipment(event) => event,
            other => panic!("expected Equipment, got {other:?}"),
        }
    }

    #[test]
    fn equip_creates_domain_and_duplicate_seq_does_not_redirty() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        assert!(owner.world().equipment_of(actor).is_none());
        owner.apply_input(InputUpdate::Equip {
            connection_id: id,
            request: EquipRequest {
                seq: 1,
                slot: EquipmentSlot::Weapon as u8,
                content_id: debug_sword(),
            },
        });
        assert_eq!(
            recv_equipment(&mut rx),
            ServerEquipment::Accepted { seq: 1 }
        );
        assert_eq!(
            owner.world().equipment_slot(actor, EquipmentSlot::Weapon),
            Some(debug_sword())
        );
        let _ = owner.world_mut().consume_dirty(actor);
        let revs = owner.world().domain_revs_of(actor).unwrap().equipment;
        owner.apply_input(InputUpdate::Equip {
            connection_id: id,
            request: EquipRequest {
                seq: 1,
                slot: EquipmentSlot::Weapon as u8,
                content_id: debug_sword(),
            },
        });
        assert_eq!(
            recv_equipment(&mut rx),
            ServerEquipment::Accepted { seq: 1 }
        );
        assert!(!owner.world().dirty_of(actor).unwrap().equipment);
        assert_eq!(owner.world().domain_revs_of(actor).unwrap().equipment, revs);
    }

    #[test]
    fn last_unequip_keeps_empty_domain() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        owner.apply_input(InputUpdate::Equip {
            connection_id: id,
            request: EquipRequest {
                seq: 1,
                slot: EquipmentSlot::Weapon as u8,
                content_id: debug_sword(),
            },
        });
        let _ = recv_equipment(&mut rx);
        owner.apply_input(InputUpdate::Unequip {
            connection_id: id,
            request: UnequipRequest {
                seq: 2,
                slot: EquipmentSlot::Weapon as u8,
            },
        });
        assert_eq!(
            recv_equipment(&mut rx),
            ServerEquipment::Accepted { seq: 2 }
        );
        let state = owner.world().equipment_of(actor).expect("domain remains");
        assert!(state.is_empty());
    }

    #[test]
    fn stale_and_gap_seq_rejected_without_mutation() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        owner.apply_input(InputUpdate::Unequip {
            connection_id: id,
            request: UnequipRequest {
                seq: 0,
                slot: EquipmentSlot::Weapon as u8,
            },
        });
        match recv_equipment(&mut rx) {
            ServerEquipment::Rejected {
                seq: 0,
                reason: EquipmentRejectReason::InvalidRequest,
            } => {}
            other => panic!("first seq 0 must be InvalidRequest, got {other:?}"),
        }
        owner.apply_input(InputUpdate::Equip {
            connection_id: id,
            request: EquipRequest {
                seq: 1,
                slot: EquipmentSlot::Weapon as u8,
                content_id: debug_sword(),
            },
        });
        let _ = recv_equipment(&mut rx);
        owner.apply_input(InputUpdate::Equip {
            connection_id: id,
            request: EquipRequest {
                seq: 1,
                slot: EquipmentSlot::Weapon as u8,
                content_id: debug_sword(),
            },
        });
        let _ = recv_equipment(&mut rx);
        owner.apply_input(InputUpdate::Unequip {
            connection_id: id,
            request: UnequipRequest {
                seq: 0,
                slot: EquipmentSlot::Weapon as u8,
            },
        });
        match recv_equipment(&mut rx) {
            ServerEquipment::Rejected {
                seq: 0,
                reason: EquipmentRejectReason::StaleRequest,
            } => {}
            other => panic!("seq 0 after last=1 must be StaleRequest, got {other:?}"),
        }
        owner.apply_input(InputUpdate::Unequip {
            connection_id: id,
            request: UnequipRequest {
                seq: 4,
                slot: EquipmentSlot::Weapon as u8,
            },
        });
        match recv_equipment(&mut rx) {
            ServerEquipment::Rejected {
                reason: EquipmentRejectReason::InvalidRequest,
                ..
            } => {}
            other => panic!("gap must be InvalidRequest, got {other:?}"),
        }
        owner.apply_input(InputUpdate::Equip {
            connection_id: id,
            request: EquipRequest {
                seq: 2,
                slot: EquipmentSlot::Weapon as u8,
                content_id: debug_sword(),
            },
        });
        match recv_equipment(&mut rx) {
            ServerEquipment::Accepted { seq: 2 } => {}
            other => panic!("seq 2 same-value Equip must Accept, got {other:?}"),
        }
        owner.apply_input(InputUpdate::Unequip {
            connection_id: id,
            request: UnequipRequest {
                seq: 1,
                slot: EquipmentSlot::Weapon as u8,
            },
        });
        match recv_equipment(&mut rx) {
            ServerEquipment::Rejected {
                reason: EquipmentRejectReason::StaleRequest,
                ..
            } => {}
            other => panic!("seq 1 after last=2 must be StaleRequest, got {other:?}"),
        }
        assert_eq!(
            owner.world().equipment_slot(actor, EquipmentSlot::Weapon),
            Some(debug_sword()),
            "stale/gap must not mutate"
        );
    }

    #[test]
    fn wrong_slot_and_unknown_content_rejected() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        owner.apply_input(InputUpdate::Equip {
            connection_id: id,
            request: EquipRequest {
                seq: 1,
                slot: EquipmentSlot::Headwear as u8,
                content_id: debug_sword(),
            },
        });
        match recv_equipment(&mut rx) {
            ServerEquipment::Rejected {
                reason: EquipmentRejectReason::SlotMismatch,
                ..
            } => {}
            other => panic!("expected SlotMismatch, got {other:?}"),
        }
        assert!(owner.world().equipment_of(actor).is_none());
        owner.apply_input(InputUpdate::Equip {
            connection_id: id,
            request: EquipRequest {
                seq: 2,
                slot: EquipmentSlot::Weapon as u8,
                content_id: ContentId::from_token(1),
            },
        });
        match recv_equipment(&mut rx) {
            ServerEquipment::Rejected {
                reason: EquipmentRejectReason::UnknownContent,
                ..
            } => {}
            other => panic!("expected UnknownContent, got {other:?}"),
        }
        assert!(owner.world().equipment_of(actor).is_none());
    }

    #[test]
    fn repeated_same_equip_accepts_without_second_mutation() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        owner.apply_input(InputUpdate::Equip {
            connection_id: id,
            request: EquipRequest {
                seq: 1,
                slot: EquipmentSlot::Weapon as u8,
                content_id: debug_sword(),
            },
        });
        let _ = recv_equipment(&mut rx);
        let _ = owner.world_mut().consume_dirty(actor);
        let revs = owner.world().domain_revs_of(actor).unwrap();
        owner.apply_input(InputUpdate::Equip {
            connection_id: id,
            request: EquipRequest {
                seq: 2,
                slot: EquipmentSlot::Weapon as u8,
                content_id: debug_sword(),
            },
        });
        assert_eq!(
            recv_equipment(&mut rx),
            ServerEquipment::Accepted { seq: 2 }
        );
        assert!(!owner.world().dirty_of(actor).unwrap().equipment);
        let after = owner.world().domain_revs_of(actor).unwrap();
        assert_eq!(after.equipment, revs.equipment);
        assert_eq!(after.transform, revs.transform);
        assert_eq!(after.health, revs.health);
    }

    fn basic_strike_id() -> ContentId {
        ContentId::from_authored("skill.basic.strike").unwrap()
    }

    fn recv_ability(rx: &mut tokio::sync::mpsc::Receiver<ServerControl>) -> ServerAbility {
        loop {
            match rx.try_recv().expect("ability result") {
                ServerControl::Ability(event) => return event,
                ServerControl::PresentationOneShot(_) => continue,
                other => panic!("expected Ability, got {other:?}"),
            }
        }
    }

    fn activate_strike(
        owner: &mut GameplayOwner,
        id: ConnectionId,
        seq: u32,
        selected: Option<WireEntityId>,
    ) {
        owner.apply_input(InputUpdate::AbilityActivate {
            connection_id: id,
            request: AbilityActivateRequest {
                seq,
                ability_id: basic_strike_id(),
                selected,
            },
        });
    }

    fn tick_ability(owner: &mut GameplayOwner, n: u32) {
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        for _ in 0..n {
            owner.simulate_tick(dt);
        }
    }

    fn health_snapshot(owner: &GameplayOwner) -> Vec<(u32, u32, f32)> {
        let mut rows: Vec<_> = owner
            .world()
            .iter()
            .filter_map(|eid| {
                owner
                    .world()
                    .health_of(eid)
                    .map(|h| (eid.index(), eid.generation(), h.current))
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        rows
    }

    #[test]
    fn live_player_has_health_and_basic_strike_grant() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        assert!(owner.world().health_of(actor).is_some());
        assert!(owner.world().ability_granted(actor, basic_strike_id()));
        let creatures: Vec<_> = owner
            .world()
            .iter()
            .filter(|&entity| owner.world().npc_of(entity).is_some())
            .collect();
        assert_eq!(creatures.len(), 1);
        assert!(!owner
            .world()
            .ability_granted(creatures[0], basic_strike_id()));
    }

    #[test]
    fn live_creature_acquires_player_and_uses_contact_damage_runtime() {
        let mut owner = GameplayOwner::new();
        let connection = ConnectionId::from_raw(1);
        owner.attach(connection);
        let player = owner.entity_of(connection).unwrap();
        let creature = owner
            .world()
            .iter()
            .find(|&entity| owner.world().npc_of(entity).is_some())
            .expect("live creature");
        let creature_x = owner.world().transform_of(creature).unwrap().position[0];
        assert!(owner.set_player_x(connection, creature_x - 0.5));
        for _ in 0..4 {
            owner.simulate_tick(purgatory_simulation::TICK_DURATION.as_secs_f32());
        }
        assert_eq!(
            owner.world().health_of(player).unwrap().current,
            PLAYER_HEALTH_MAX - 1.0
        );
        assert!(!owner
            .world()
            .ability_granted(creature, basic_strike_id()));
    }

    #[test]
    fn normal_session_pve_encounter_completes_both_directions_and_respawns() {
        let mut owner = GameplayOwner::new();
        let connection = ConnectionId::from_raw(1);
        owner.attach(connection);
        let player = owner.entity_of(connection).expect("player");
        let creature = owner
            .world()
            .iter()
            .find(|&entity| {
                owner
                    .world()
                    .npc_of(entity)
                    .is_some_and(|npc| npc.type_token == LIVE_COMBAT_CREATURE_TYPE_TOKEN)
            })
            .expect("live combat creature");
        let creature_x = owner.world().transform_of(creature).unwrap().position[0];
        assert!(owner.set_player_x(connection, creature_x - 1.0));

        // Player -> creature: use the normal network ability command path until
        // the authoritative NPC death lifecycle is entered.
        if let Some(mut npc) = owner.world().npc_of(creature) {
            npc.active = false;
            assert!(owner.world_mut().set_npc(creature, npc));
        }
        for seq in 1..=4 {
            activate_strike(&mut owner, connection, seq, None);
            tick_ability(&mut owner, 16);
        }
        assert_eq!(owner.world().health_of(creature).unwrap().current, 0.0);
        assert!(owner.world().npc_of(creature).unwrap().dead_pending);

        // The dead creature leaves, then the existing scheduled spawn path
        // creates a fresh runtime entity at its authored home.
        for _ in 0..32 {
            owner.simulate_tick(purgatory_simulation::TICK_DURATION.as_secs_f32());
        }
        let respawned = owner
            .world()
            .iter()
            .find(|&entity| {
                owner
                    .world()
                    .npc_of(entity)
                    .is_some_and(|npc| npc.type_token == LIVE_COMBAT_CREATURE_TYPE_TOKEN)
            })
            .expect("respawned creature");
        assert_ne!(respawned, creature);
        assert_eq!(
            owner.world().health_of(respawned).unwrap().current,
            purgatory_simulation::NPC_HEALTH_MAX
        );

        // Creature -> player: place the live player in range and let the
        // authoritative NPC driver deliver damage.
        let respawned_x = owner.world().transform_of(respawned).unwrap().position[0];
        assert!(owner.set_player_x(connection, respawned_x - 1.0));
        assert!(owner.world_mut().set_health(
            player,
            purgatory_simulation::Health {
                current: 5.0,
                max: PLAYER_HEALTH_MAX,
            },
        ));
        for _ in 0..=400 {
            owner.simulate_tick(purgatory_simulation::TICK_DURATION.as_secs_f32());
            if owner.world().health_of(player).unwrap().is_dead() {
                break;
            }
        }
        assert!(owner.world().health_of(player).unwrap().is_dead());
        owner.simulate_tick(purgatory_simulation::TICK_DURATION.as_secs_f32());
        assert!(owner.world().health_of(player).unwrap().is_dead());
        owner.handle_respawn(connection);
        assert_eq!(
            owner.world().health_of(player),
            Some(purgatory_simulation::Health::full(PLAYER_HEALTH_MAX))
        );
        assert!(owner.world().health_of(player).unwrap().is_alive());
    }

    #[test]
    fn ability_empty_swing_accepts_and_does_not_change_health() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let before = health_snapshot(&owner);
        activate_strike(&mut owner, id, 1, None);
        assert_eq!(recv_ability(&mut rx), ServerAbility::Accepted { seq: 1 });
        let live = owner.world().active_action(actor).expect("ability started");
        assert_eq!(
            live.kind,
            ActionKind::Ability {
                id: basic_strike_id()
            }
        );
        assert_eq!(live.phase, ActionPhase::Windup);
        tick_ability(&mut owner, 12);
        assert_eq!(health_snapshot(&owner), before);
        assert!(owner.world().active_action(actor).is_none());
    }

    #[test]
    fn ability_forward_query_damages_nearby_combatant() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let pos = owner.world().transform_of(actor).unwrap().position;
        let address = owner.world().address_of(actor).unwrap();
        let dummy = owner
            .world_mut()
            .spawn(
                RuntimeSpawnRequest::transient_at(address)
                    .with_transform(Transform::from_position([pos[0] + 1.0, pos[1]]))
                    .with_health(Health::full(10.0))
                    .visible(),
            )
            .expect("dummy");
        activate_strike(&mut owner, id, 1, None);
        assert_eq!(recv_ability(&mut rx), ServerAbility::Accepted { seq: 1 });
        tick_ability(&mut owner, 8);
        assert!((owner.world().health_of(dummy).unwrap().current - 5.0).abs() < 1e-5);
        assert_eq!(
            owner.world().health_of(actor).unwrap().current,
            PLAYER_HEALTH_MAX
        );
    }

    #[test]
    fn ability_rejects_unknown_ungranted_duplicate_stale_dead_busy_cooldown() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();

        owner.apply_input(InputUpdate::AbilityActivate {
            connection_id: id,
            request: AbilityActivateRequest {
                seq: 1,
                ability_id: ContentId::from_authored("skill.does.not.exist").unwrap(),
                selected: None,
            },
        });
        assert_eq!(
            recv_ability(&mut rx),
            ServerAbility::Rejected {
                seq: 1,
                reason: AbilityCommandReject::UnknownAbility,
            }
        );
        assert!(owner.world().active_action(actor).is_none());

        owner.world_mut().revoke_ability(actor, basic_strike_id());
        activate_strike(&mut owner, id, 2, None);
        assert_eq!(
            recv_ability(&mut rx),
            ServerAbility::Rejected {
                seq: 2,
                reason: AbilityCommandReject::NotGranted,
            }
        );
        assert!(owner.world_mut().grant_ability(actor, basic_strike_id()));

        activate_strike(&mut owner, id, 3, Some(wire_id(actor)));
        assert_eq!(
            recv_ability(&mut rx),
            ServerAbility::Rejected {
                seq: 3,
                reason: AbilityCommandReject::InvalidActivation,
            }
        );

        activate_strike(&mut owner, id, 4, None);
        assert_eq!(recv_ability(&mut rx), ServerAbility::Accepted { seq: 4 });
        activate_strike(&mut owner, id, 4, None);
        assert_eq!(
            recv_ability(&mut rx),
            ServerAbility::Accepted { seq: 4 },
            "duplicate seq replays last result without a second start"
        );

        activate_strike(&mut owner, id, 5, None);
        assert_eq!(
            recv_ability(&mut rx),
            ServerAbility::Rejected {
                seq: 5,
                reason: AbilityCommandReject::OnCooldown,
            }
        );

        activate_strike(&mut owner, id, 4, None);
        assert_eq!(
            recv_ability(&mut rx),
            ServerAbility::Rejected {
                seq: 4,
                reason: AbilityCommandReject::StaleRequest,
            }
        );

        tick_ability(&mut owner, 16);
        assert!(owner.world().active_action(actor).is_none());
        assert!(owner.world_mut().apply_damage(actor, PLAYER_HEALTH_MAX));
        activate_strike(&mut owner, id, 6, None);
        assert_eq!(
            recv_ability(&mut rx),
            ServerAbility::Rejected {
                seq: 6,
                reason: AbilityCommandReject::ActorDead,
            }
        );
        assert!(owner.world().active_action(actor).is_none());
    }

    #[test]
    fn ability_busy_uses_request_ability_not_a_network_path() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        owner
            .world_mut()
            .try_start_action(
                actor,
                ActionKind::Test { token: 1 },
                ActionGateContext::in_world(),
            )
            .expect("occupy exclusive action");
        activate_strike(&mut owner, id, 1, None);
        assert_eq!(
            recv_ability(&mut rx),
            ServerAbility::Rejected {
                seq: 1,
                reason: AbilityCommandReject::Busy,
            }
        );
        let live = owner.world().active_action(actor).unwrap();
        assert_eq!(live.kind, ActionKind::Test { token: 1 });
    }

    #[test]
    fn ability_activate_broadcasts_attack_oneshot() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        activate_strike(&mut owner, id, 1, None);
        assert_eq!(recv_ability(&mut rx), ServerAbility::Accepted { seq: 1 });
        match rx.try_recv().expect("attack oneshot") {
            ServerControl::PresentationOneShot(event) => {
                assert_eq!(event.entity, wire_id(actor));
                assert_eq!(event.kind, PresentationOneShotKind::Attack.as_u8());
                assert!(event.until_tick > 0);
            }
            other => panic!("expected PresentationOneShot, got {other:?}"),
        }
        assert_eq!(
            owner.world().presentation_oneshot_of(actor).map(|o| o.kind),
            Some(PresentationOneShotKind::Attack)
        );
    }

    #[test]
    fn ability_hit_broadcasts_hurt_to_observers() {
        let mut owner = GameplayOwner::new();
        let attacker = ConnectionId::from_raw(1);
        let victim_conn = ConnectionId::from_raw(2);
        let (tx_a, mut rx_a) = tokio::sync::mpsc::channel(16);
        let (tx_v, mut rx_v) = tokio::sync::mpsc::channel(16);
        owner.attach(attacker);
        owner.attach(victim_conn);
        owner.bindings.get_mut(&attacker).unwrap().interact = Some(tx_a);
        owner.bindings.get_mut(&victim_conn).unwrap().interact = Some(tx_v);
        let actor = owner.entity_of(attacker).unwrap();
        let victim = owner.entity_of(victim_conn).unwrap();
        let actor_pos = owner.world().transform_of(actor).unwrap().position;
        let _ = owner.world_mut().set_transform(
            victim,
            Transform::from_position([actor_pos[0] + 1.0, actor_pos[1]]),
        );
        activate_strike(&mut owner, attacker, 1, None);
        assert_eq!(recv_ability(&mut rx_a), ServerAbility::Accepted { seq: 1 });
        tick_ability(&mut owner, 8);
        let mut saw_hurt = false;
        for rx in [&mut rx_a, &mut rx_v] {
            while let Ok(msg) = rx.try_recv() {
                if let ServerControl::PresentationOneShot(event) = msg
                    && event.entity == wire_id(victim)
                    && event.kind == PresentationOneShotKind::Hurt.as_u8()
                {
                    saw_hurt = true;
                }
            }
        }
        assert!(saw_hurt, "valid hit must broadcast Hurt");
        assert!(
            (owner.world().health_of(victim).unwrap().current - (PLAYER_HEALTH_MAX - 5.0)).abs()
                < 1e-5
        );
        assert_eq!(
            owner.world().presentation_oneshot_of(actor).map(|o| o.kind),
            Some(PresentationOneShotKind::Attack)
        );
    }

    #[test]
    fn respawn_request_restores_same_entity_and_rebases_input() {
        let mut owner = GameplayOwner::new();
        let connection = ConnectionId::from_raw(1);
        owner.attach(connection);
        let actor = owner.entity_of(connection).expect("actor");
        let spawn = owner.world().transform_of(actor).expect("spawn").position;
        owner.apply_input(command_update(
            connection,
            cmd(1, MoveAxis::Right, false, false),
        ));
        owner.world_mut().set_health(
            actor,
            Health {
                current: 0.0,
                max: PLAYER_HEALTH_MAX,
            },
        );

        owner.handle_respawn(connection);

        assert_eq!(owner.entity_of(connection), Some(actor));
        assert_eq!(owner.world().transform_of(actor).unwrap().position, spawn);
        assert_eq!(
            owner.world().health_of(actor).unwrap().current,
            PLAYER_HEALTH_MAX
        );
        assert_eq!(owner.bindings[&connection].input.input_epoch, 1);
        assert_eq!(owner.bindings[&connection].input.queued_len(), 0);
        assert_eq!(
            owner
                .bindings
                .get_mut(&connection)
                .expect("binding")
                .input
                .take_for_tick(),
            PlayerInput::idle()
        );
    }
}
