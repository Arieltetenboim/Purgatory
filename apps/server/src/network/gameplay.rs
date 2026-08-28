//! Authoritative gameplay input. Network tasks never touch [`World`].
//!
//! Late-collapse is intentional authoritative input compaction: after
//! Continuation starvation, a prefix of delayed commands is acknowledged in
//! one `tick_player` (latest held + `jump_pressed` OR). Intermediate historical
//! held commands may be acknowledged without individual physics steps.

use std::collections::{HashMap, VecDeque};

use purgatory_protocol::{ConnectionId, InputCommand, MoveAxis, WorldSnapshot};
use purgatory_simulation::{
    EntityId, FOOTNOTE_SPAWN_X, P0, P0_POSITION, PlayerInput, PlayerState, World,
};
use tokio::sync::watch;

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
    #[allow(dead_code)]
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
        Some(self.input_epoch)
    }

    /// One player simulation step's input. Does not run physics.
    #[must_use]
    pub fn take_for_tick(&mut self) -> PlayerInput {
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
    pub input: SessionInput,
    snapshots: Option<watch::Sender<Option<WorldSnapshot>>>,
}

/// Simulation-thread owner of `World` and `ConnectionId → EntityId`.
pub struct GameplayOwner {
    world: World,
    bindings: HashMap<ConnectionId, PlayerBinding>,
    ticks: u64,
    pub input_received: u64,
    pub input_accepted: u64,
    pub input_duplicate: u64,
    pub input_stale: u64,
    pub snapshots_built: u64,
    pub snapshot_sequence: u32,
    pub last_snapshot_entities: u16,
    pub snapshot_send_failed: u64,
}

pub enum LifecycleCmd {
    Attach {
        connection_id: ConnectionId,
        snapshots: Option<watch::Sender<Option<WorldSnapshot>>>,
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
        self.attach_with_snapshots(connection_id, None);
    }

    pub fn attach_with_snapshots(
        &self,
        connection_id: ConnectionId,
        snapshots: Option<watch::Sender<Option<WorldSnapshot>>>,
    ) {
        let _ = self.lifecycle.try_send(LifecycleCmd::Attach {
            connection_id,
            snapshots,
        });
    }

    pub fn detach(&self, connection_id: ConnectionId) {
        let _ = self
            .lifecycle
            .try_send(LifecycleCmd::Detach { connection_id });
    }

    pub fn try_input(&self, connection_id: ConnectionId, command: InputCommand) -> bool {
        self.input
            .try_send(InputUpdate::Command {
                connection_id,
                command,
            })
            .is_ok()
    }

    pub fn try_held_cancel(&self, connection_id: ConnectionId) -> bool {
        self.input
            .try_send(InputUpdate::HeldCancel { connection_id })
            .is_ok()
    }
}

const LIFECYCLE_CAP: usize = 64;
const INPUT_CAP: usize = 128;

#[must_use]
pub const fn lifecycle_cap() -> usize {
    LIFECYCLE_CAP
}

#[must_use]
pub const fn input_cap() -> usize {
    INPUT_CAP
}

impl GameplayOwner {
    /// Platforms-only FOOTNOTE arena. Players spawn on connect.
    #[must_use]
    pub fn new() -> Self {
        let mut world = World::footnote_test_stage();
        if let Some(id) = world.player_id() {
            world.despawn(id);
        }
        Self {
            world,
            bindings: HashMap::new(),
            ticks: 0,
            input_received: 0,
            input_accepted: 0,
            input_duplicate: 0,
            input_stale: 0,
            snapshots_built: 0,
            snapshot_sequence: 0,
            last_snapshot_entities: 0,
            snapshot_send_failed: 0,
        }
    }

    #[must_use]
    #[allow(dead_code)] // diagnostic accessors used by tests and the quality gate
    pub fn world(&self) -> &World {
        &self.world
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
        let Some(view) = self
            .world
            .iter_platforms()
            .find(|view| view.platform.kind == PlatformKind::OneWay)
        else {
            return false;
        };
        let (transform, state) =
            PlayerState::standing_on_at(view.id, view.top_surface(), view.transform.position[0]);
        let Some((t, player)) = self.world.player_parts_mut_for(entity) else {
            return false;
        };
        *t = transform;
        *player = state;
        true
    }

    #[cfg(test)]
    pub fn set_player_x(&mut self, id: ConnectionId, x: f32) -> bool {
        let Some(entity) = self.entity_of(id) else {
            return false;
        };
        let Some((transform, _)) = self.world.player_parts_mut_for(entity) else {
            return false;
        };
        transform.position[0] = x;
        true
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn contains_entity(&self, id: EntityId) -> bool {
        self.world.contains(id)
    }

    pub fn attach(&mut self, connection_id: ConnectionId) {
        if self.bindings.contains_key(&connection_id) {
            return;
        }
        let floor = self
            .world
            .iter_platforms()
            .find(|view| {
                view.platform.half_extents == P0.half_extents
                    && (view.transform.position[0] - P0_POSITION[0]).abs() < 0.01
                    && (view.transform.position[1] - P0_POSITION[1]).abs() < 0.01
            })
            .or_else(|| self.world.iter_platforms().next());
        let Some(view) = floor else {
            return;
        };
        let (transform, state) =
            PlayerState::standing_on_at(view.id, view.top_surface(), FOOTNOTE_SPAWN_X);
        let entity = self.world.spawn_player(transform, state);
        self.bindings.insert(
            connection_id,
            PlayerBinding {
                entity,
                input: SessionInput::new(),
                snapshots: None,
            },
        );
    }

    pub fn detach(&mut self, connection_id: ConnectionId) {
        if let Some(binding) = self.bindings.remove(&connection_id) {
            self.world.despawn(binding.entity);
        }
    }

    pub fn apply_input(&mut self, update: InputUpdate) -> SeqDecision {
        self.input_received = self.input_received.saturating_add(1);
        match update {
            InputUpdate::HeldCancel { connection_id } => {
                let Some(binding) = self.bindings.get_mut(&connection_id) else {
                    self.input_stale = self.input_stale.saturating_add(1);
                    return SeqDecision::Stale;
                };
                binding.input.held_cancel();
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
                let decision = binding.input.apply(command);
                match decision {
                    SeqDecision::Accept => {
                        self.input_accepted = self.input_accepted.saturating_add(1)
                    }
                    SeqDecision::Duplicate => {
                        self.input_duplicate = self.input_duplicate.saturating_add(1);
                    }
                    SeqDecision::Stale
                    | SeqDecision::Gap
                    | SeqDecision::Overflow
                    | SeqDecision::OldEpoch => {
                        self.input_stale = self.input_stale.saturating_add(1)
                    }
                }
                decision
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
                    snapshots,
                } => {
                    self.attach(connection_id);
                    if let Some(binding) = self.bindings.get_mut(&connection_id) {
                        binding.snapshots = snapshots;
                    }
                }
                LifecycleCmd::Detach { connection_id } => self.detach(connection_id),
            }
        }
        while let Ok(update) = input.try_recv() {
            let _ = self.apply_input(update);
        }
    }

    /// One simulation tick for every attached player. Packet count is irrelevant.
    pub fn simulate_tick(&mut self, dt: f32) {
        let ids: Vec<(ConnectionId, EntityId, PlayerInput)> = self
            .bindings
            .iter_mut()
            .map(|(cid, binding)| (*cid, binding.entity, binding.input.take_for_tick()))
            .collect();
        for (_, entity, player_input) in ids {
            self.world.tick_player(entity, dt, player_input);
        }
        self.ticks = self.ticks.saturating_add(1);
        self.publish_snapshots();
    }

    fn publish_snapshots(&mut self) {
        let visible: Vec<EntityId> = self.bindings.values().map(|b| b.entity).collect();
        self.snapshot_sequence = self.snapshot_sequence.saturating_add(1);
        self.snapshots_built = self.snapshots_built.saturating_add(1);
        let mut last_entities = 0u16;
        if verbose_snapshots() {
            println!(
                "snapshot seq={} tick={} entities={}",
                self.snapshot_sequence,
                self.ticks,
                visible.len()
            );
        }
        for (cid, binding) in &self.bindings {
            let Some(tx) = binding.snapshots.as_ref() else {
                continue;
            };
            let snap = super::snapshot::build(
                self.snapshot_sequence,
                self.ticks,
                binding.entity,
                &self.world,
                &visible,
                binding.input.input_epoch,
                binding.input.last_acknowledged(),
                binding.input.unmatched_continuation_ticks,
            );
            last_entities = u16::try_from(snap.entities.len()).unwrap_or(u16::MAX);
            if verbose_snapshots() {
                println!(
                    "cid={cid} snapshot seq={} local_entity={}",
                    snap.snapshot_sequence, snap.local_player_entity
                );
            }
            if tx.send(Some(snap)).is_err() {
                self.snapshot_send_failed = self.snapshot_send_failed.saturating_add(1);
            }
        }
        self.last_snapshot_entities = last_entities;
    }
}

fn verbose_snapshots() -> bool {
    std::env::var_os("PURGATORY_NET_VERBOSE").is_some()
}

impl Default for GameplayOwner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::watch;

    fn cmd(seq: u32, axis: MoveAxis, jump: bool, down: bool) -> InputCommand {
        InputCommand {
            input_epoch: 0,
            sequence: seq,
            move_axis: axis,
            jump_pressed: jump,
            down_held: down,
        }
    }

    fn command_update(id: ConnectionId, command: InputCommand) -> InputUpdate {
        InputUpdate::Command {
            connection_id: id,
            command,
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
        if let Some((transform, _)) = owner.world.player_parts_mut_for(entity) {
            transform.position[0] = 0.0;
        }
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
    fn snapshot_watch_keeps_latest_and_does_not_block_ticks() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        let b = ConnectionId::from_raw(2);
        let (tx_a, rx_a) = watch::channel(None);
        let (tx_b, rx_b) = watch::channel(None);
        owner.attach(a);
        owner.attach(b);
        owner.bindings.get_mut(&a).unwrap().snapshots = Some(tx_a);
        owner.bindings.get_mut(&b).unwrap().snapshots = Some(tx_b);
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        for _ in 0..40 {
            owner.simulate_tick(dt);
        }
        assert_eq!(owner.ticks(), 40);
        assert_eq!(owner.snapshot_sequence, 40);
        let latest_a = rx_a.borrow().clone().expect("a snapshot");
        let latest_b = rx_b.borrow().clone().expect("b snapshot");
        assert_eq!(latest_a.snapshot_sequence, 40);
        assert_eq!(latest_b.snapshot_sequence, 40);
        assert_eq!(latest_a.entities.len(), 2);
        assert_eq!(latest_b.entities.len(), 2);
        assert_ne!(latest_a.local_player_entity, latest_b.local_player_entity);
        assert_eq!(
            latest_a.local_player_entity,
            super::super::snapshot::to_wire_id(owner.entity_of(a).unwrap())
        );
        drop(rx_a);
        for _ in 0..8 {
            owner.simulate_tick(dt);
        }
        assert_eq!(owner.ticks(), 48);
        assert!(owner.snapshot_send_failed >= 1);
        let still_b = rx_b.borrow().clone().expect("b still updating");
        assert_eq!(still_b.snapshot_sequence, 48);
    }

    #[test]
    fn two_clients_movement_appears_in_shared_snapshot() {
        let mut owner = GameplayOwner::new();
        let a = ConnectionId::from_raw(1);
        let b = ConnectionId::from_raw(2);
        let (tx, rx) = watch::channel(None);
        owner.attach(a);
        owner.attach(b);
        owner.bindings.get_mut(&a).unwrap().snapshots = Some(tx);
        assert!(owner.set_player_x(a, 0.0));
        assert!(owner.set_player_x(b, 0.0));
        owner.apply_input(command_update(a, cmd(1, MoveAxis::Right, false, false)));
        owner.apply_input(command_update(b, cmd(1, MoveAxis::Left, false, false)));
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        for _ in 0..12 {
            owner.simulate_tick(dt);
        }
        let snap = rx.borrow().clone().expect("snapshot");
        assert_eq!(snap.entities.len(), 2);
        let ea = super::super::snapshot::to_wire_id(owner.entity_of(a).unwrap());
        let eb = super::super::snapshot::to_wire_id(owner.entity_of(b).unwrap());
        let ax = snap
            .entities
            .iter()
            .find(|e| e.entity_id == ea)
            .unwrap()
            .position[0];
        let bx = snap
            .entities
            .iter()
            .find(|e| e.entity_id == eb)
            .unwrap()
            .position[0];
        assert!(ax > 0.2, "A right from 0, got {ax}");
        assert!(bx < -0.2, "B left from 0, got {bx}");
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
        let (tx_a, rx_a) = watch::channel(None);
        let (tx_b, rx_b) = watch::channel(None);
        owner.attach(a);
        owner.attach(b);
        owner.bindings.get_mut(&a).unwrap().snapshots = Some(tx_a);
        owner.bindings.get_mut(&b).unwrap().snapshots = Some(tx_b);
        owner.apply_input(command_update(a, cmd(1, MoveAxis::Right, false, false)));
        owner.apply_input(command_update(b, cmd(1, MoveAxis::Left, false, false)));
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        owner.simulate_tick(dt);
        let snap_a = rx_a.borrow().clone().expect("a");
        let snap_b = rx_b.borrow().clone().expect("b");
        assert_eq!(snap_a.last_acknowledged_input_sequence, 1);
        assert_eq!(snap_b.last_acknowledged_input_sequence, 1);
        assert_ne!(snap_a.local_player_entity, snap_b.local_player_entity);
        assert_eq!(snap_a.input_epoch, 0);
        assert_eq!(snap_b.input_epoch, 0);
        assert!(snap_a.local_grounded);
        assert!(snap_b.local_grounded);
    }
}
