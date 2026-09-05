//! Client-only local player prediction and Phase 5.5 reconciliation.
//!
//! Presentation layer separate from [`crate::replica::ReplicatedWorld`].
//! Does **not** overwrite authoritative replica state or send client positions.
//!
//! On each accepted snapshot: restore durable FOOTNOTE state from the replica
//! header, drop pending commands with `sequence <= last_acknowledged_input_sequence`,
//! and replay the remainder through [`World::tick_predicted_player`]. Replay uses
//! `tick_player` only — never a whole-world tick.
//!
//! Late-collapse on the server is intentional authoritative input compaction:
//! intermediate historical held commands may be acknowledged without receiving
//! individual physics steps. Continuation debt (`unmatched_continuation_ticks`)
//! is diagnostic here.
//!
//! # Failsafes
//!
//! While pending commands remain, AlignedDistance / LeadSafety / VerticalSettled
//! are not snap targets. Hard snaps are for structural issues (non-finite,
//! invalid epoch/ack/seq, unresolvable support) and FirstActive / generation
//! change. With an empty pending window, the existing 8 wu / VerticalSettled
//! checks may still fire (restore bug). DriftCorrection is not used.
//!
//! # Clock
//!
//! Prediction advances only via the shared [`SimulationClock`] fixed step.
//! A hitch (`ticks_executed > 1`) is a prediction discontinuity: pose history
//! is cleared, pending is kept and replayed, and exactly one new command is
//! emitted — not N catch-up commands.

use std::collections::VecDeque;

use purgatory_protocol::{InputCommand, WireEntityId};
use purgatory_simulation::{Health, PlayerInput, TICK_DURATION, World};

use crate::replica::{ReplicatedEntity, ReplicatedWorld};

/// World-unit distance at which prediction hard-snaps to authority (total).
/// Presentation safety only — not a gameplay rule.
pub const PREDICTION_REANCHOR_DISTANCE: f32 = 8.0;

/// Aligned-residual budget, in world units, below which a difference counts as
/// temporal lead/lag rather than divergence.
///
/// Applied **only** to the residual left after temporal alignment (see
/// [`LocalPrediction::aligned_residual`]) — never to raw
/// current-pred-vs-latest-auth distance, which legitimately grows with RTT.
pub const PREDICTION_ALIGNED_RESIDUAL_WU: f32 = 0.05;

/// Consecutive snapshots the aligned residual had to stay above the budget
/// before Phase 5.4 applied drift correction. Retained as the documented
/// threshold; Phase 5.5 does not offset-correct.
#[allow(dead_code)]
pub const PREDICTION_DIVERGENCE_SNAPSHOTS: u32 = 4;

/// Largest temporal offset that still counts as a plausible pipeline delay.
///
/// Snapshot delivery and input application are each well inside one tick on a
/// local link, so sixteen ticks (~533 ms one-way) is generous. Beyond it, a
/// "matching" history sample means the authority sits at a point this client
/// left long ago — drift, not lag — so it must not explain the difference.
pub const PREDICTION_MAX_PLAUSIBLE_LAG_TICKS: u64 = 16;

/// Vertical snap when authority is **settled** and divergent `|dy|` is large.
pub const PREDICTION_VERTICAL_REANCHOR: f32 = 2.0;

/// Auth `|vy|` at or below this counts as vertically settled for re-anchor.
pub const PREDICTION_AUTH_SETTLED_VY: f32 = 0.15;

/// Bounded predicted-state history for tick-aligned snapshot compares (~2 s).
pub const PREDICTION_HISTORY_CAP: usize = 64;

/// Send / replay window. Matches the server session queue cap.
pub const PREDICTION_PENDING_CAP: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapReason {
    FirstActive,
    Generation,
    AlignedDistance,
    LeadSafety,
    VerticalSettled,
    Structural,
}

impl SnapReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FirstActive => "first_active",
            Self::Generation => "generation",
            Self::AlignedDistance => "aligned_distance",
            Self::LeadSafety => "lead_safety",
            Self::VerticalSettled => "vertical_settled",
            Self::Structural => "structural",
        }
    }
}

/// Immutable HeldCancel confirmation barrier. Captured at send; never re-read
/// from a mutating last-sent cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CancelBarrier {
    pub epoch: u16,
    pub target_sequence: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PredictionDiagnostics {
    pub enabled: bool,
    pub active: bool,
    pub auth_position: Option<[f32; 2]>,
    pub auth_velocity: Option<[f32; 2]>,
    pub predicted_position: Option<[f32; 2]>,
    pub predicted_velocity: Option<[f32; 2]>,
    /// Raw: current pred vs latest auth (includes expected RTT lead).
    pub lead_error: Option<f32>,
    /// Residual left after temporal alignment (true divergence).
    pub aligned_error: Option<f32>,
    pub aligned_dx: Option<f32>,
    pub aligned_dy: Option<f32>,
    /// Ticks between the current predicted tick and the best-matching history
    /// sample: the temporal offset that best explains the difference.
    pub best_temporal_offset: Option<i64>,
    /// Deprecated alias for UI: same as [`Self::lead_error`].
    #[allow(dead_code)]
    pub position_error: Option<f32>,
    pub prediction_tick: u64,
    pub auth_server_tick: u64,
    /// Client tick of the best temporal match (not a tick-map assumption).
    pub best_match_tick: Option<u64>,
    pub reset_count: u64,
    /// Soft offset corrections (never teleports), separate from hard snaps.
    #[allow(dead_code)]
    pub drift_correction_count: u64,
    /// Consecutive snapshots whose aligned residual exceeded the budget.
    pub consecutive_aligned_divergence: u32,
    pub max_aligned_error: f32,
    pub last_snap_reason: Option<&'static str>,
    pub pending_count: u32,
    pub last_ack: u32,
    pub input_epoch: u16,
    #[allow(dead_code)]
    pub last_sent_seq: u32,
    pub continuation_debt: u16,
    #[allow(dead_code)]
    pub hard_snap_count: u64,
    #[allow(dead_code)]
    pub vertical_settled_count: u64,
    pub cancel_pending: bool,
    pub total_reconciliation_count: u64,
    /// Pre-reconcile predicted → post restore+replay predicted.
    pub last_correction_wu: f32,
    pub max_correction_wu: f32,
    /// Client-observed ack advancement between accepted snapshots. Not late-collapse.
    pub observed_ack_delta: u32,
    pub observed_ack_jump_count: u64,
    pub max_observed_ack_delta: u32,
    /// Last locally executed predicted tick velocity. Remainder extra uses this.
    pub last_tick_velocity: [f32; 2],
    /// Ticks the client skipped because [`PREDICTION_PENDING_CAP`] was full.
    pub pending_window_stall_ticks: u64,
}

#[derive(Clone, Copy, Debug)]
struct PredSample {
    client_tick: u64,
    position: [f32; 2],
    velocity: [f32; 2],
}

/// Owns local-prediction lifecycle metadata. Simulation bodies live on [`World`].
#[derive(Debug)]
pub struct LocalPrediction {
    active: bool,
    local_entity: Option<WireEntityId>,
    prediction_tick: u64,
    reset_count: u64,
    last_auth_position: Option<[f32; 2]>,
    last_auth_velocity: Option<[f32; 2]>,
    last_auth_server_tick: u64,
    history: VecDeque<PredSample>,
    pending: VecDeque<InputCommand>,
    input_epoch: u16,
    last_sent_seq: u32,
    cancel_barrier: Option<CancelBarrier>,
    /// Soft offset corrections are no longer applied; kept at 0 for overlay.
    drift_correction_count: u64,
    last_aligned_error: Option<f32>,
    consecutive_aligned_divergence: u32,
    max_aligned_error: f32,
    last_snap_reason: Option<&'static str>,
    vertical_settled_count: u64,
    structural_snap_count: u64,
    total_reconciliation_count: u64,
    last_correction_wu: f32,
    last_correction_delta: [f32; 2],
    last_sync_hard_snap: bool,
    last_sync_durable: bool,
    /// Velocity after the last locally executed predicted tick (or non-empty replay).
    /// Remainder extrapolation must use this, not a lagged restore's replica velocity.
    last_tick_velocity: [f32; 2],
    pending_window_stall_ticks: u64,
    /// Post-FOOTNOTE pose after the last predicted tick (Y-lerp current).
    tick_pose: Option<[f32; 2]>,
    /// Post-FOOTNOTE pose after the previous predicted tick (Y-lerp previous).
    prev_tick_pose: Option<[f32; 2]>,
    max_correction_wu: f32,
    last_observed_ack: u32,
    observed_ack_delta: u32,
    observed_ack_jump_count: u64,
    max_observed_ack_delta: u32,
}

impl Default for LocalPrediction {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalPrediction {
    #[must_use]
    pub fn new() -> Self {
        Self {
            active: false,
            local_entity: None,
            prediction_tick: 0,
            reset_count: 0,
            last_auth_position: None,
            last_auth_velocity: None,
            last_auth_server_tick: 0,
            history: VecDeque::with_capacity(PREDICTION_HISTORY_CAP),
            pending: VecDeque::with_capacity(PREDICTION_PENDING_CAP),
            input_epoch: 0,
            last_sent_seq: 0,
            cancel_barrier: None,
            drift_correction_count: 0,
            last_aligned_error: None,
            consecutive_aligned_divergence: 0,
            max_aligned_error: 0.0,
            last_snap_reason: None,
            vertical_settled_count: 0,
            structural_snap_count: 0,
            total_reconciliation_count: 0,
            last_correction_wu: 0.0,
            last_correction_delta: [0.0, 0.0],
            last_sync_hard_snap: false,
            last_sync_durable: true,
            last_tick_velocity: [0.0, 0.0],
            pending_window_stall_ticks: 0,
            tick_pose: None,
            prev_tick_pose: None,
            max_correction_wu: 0.0,
            last_observed_ack: 0,
            observed_ack_delta: 0,
            observed_ack_jump_count: 0,
            max_observed_ack_delta: 0,
        }
    }

    #[must_use]
    pub fn active(&self) -> bool {
        self.active
    }

    #[cfg(test)]
    #[must_use]
    pub fn local_entity(&self) -> Option<WireEntityId> {
        self.local_entity
    }

    #[cfg(test)]
    #[must_use]
    pub fn prediction_tick(&self) -> u64 {
        self.prediction_tick
    }

    #[cfg(test)]
    #[must_use]
    pub fn reset_count(&self) -> u64 {
        self.reset_count
    }

    #[cfg(test)]
    #[must_use]
    pub fn drift_correction_count(&self) -> u64 {
        self.drift_correction_count
    }

    #[cfg(test)]
    #[must_use]
    pub fn last_aligned_error(&self) -> Option<f32> {
        self.last_aligned_error
    }

    #[cfg(test)]
    #[must_use]
    pub fn last_snap_reason(&self) -> Option<&'static str> {
        self.last_snap_reason
    }

    #[cfg(test)]
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn cancel_pending(&self) -> bool {
        self.cancel_barrier.is_some()
    }

    #[must_use]
    pub fn pending_window_full(&self) -> bool {
        self.pending.len() >= PREDICTION_PENDING_CAP
    }

    #[must_use]
    pub fn last_correction_delta(&self) -> [f32; 2] {
        self.last_correction_delta
    }

    #[must_use]
    pub fn last_tick_velocity(&self) -> [f32; 2] {
        self.last_tick_velocity
    }

    #[must_use]
    pub fn tick_pose(&self) -> Option<[f32; 2]> {
        self.tick_pose
    }

    #[must_use]
    pub fn prev_tick_pose(&self) -> Option<[f32; 2]> {
        self.prev_tick_pose
    }

    #[must_use]
    pub fn pending_window_stall_ticks(&self) -> u64 {
        self.pending_window_stall_ticks
    }

    pub fn note_input_stall(&mut self) {
        self.pending_window_stall_ticks = self.pending_window_stall_ticks.saturating_add(1);
    }

    #[must_use]
    pub fn last_sync_hard_snap(&self) -> bool {
        self.last_sync_hard_snap
    }

    #[must_use]
    pub fn capture_cancel_barrier(&self) -> CancelBarrier {
        CancelBarrier {
            epoch: self.input_epoch,
            target_sequence: self.last_sent_seq,
        }
    }

    pub fn enter_cancel_pending(&mut self, barrier: CancelBarrier) {
        self.cancel_barrier = Some(barrier);
    }

    /// Append a sent command to the replay window. Caller must not send if this
    /// returns false.
    pub fn try_push_pending(&mut self, cmd: InputCommand) -> bool {
        if self.pending.len() >= PREDICTION_PENDING_CAP {
            return false;
        }
        self.input_epoch = cmd.input_epoch;
        self.last_sent_seq = cmd.sequence;
        self.pending.push_back(cmd);
        true
    }

    /// Test helper: inject a history sample at `client_tick` from the current body.
    #[cfg(test)]
    pub fn record_history_for_test(&mut self, client_tick: u64, world: &World) {
        if let Some(body) = world.player_body() {
            self.push_history(PredSample {
                client_tick,
                position: body.position,
                velocity: body.velocity,
            });
        }
    }

    /// Clear on disconnect / screen change. Does not mutate replica.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// Hitch: clock jumped more than one tick. Clear pose history, keep pending,
    /// restore replica durable state, replay unacked commands. Caller emits
    /// exactly one new clock command afterwards.
    pub fn on_hitch_discontinuity(
        &mut self,
        replica: &ReplicatedWorld,
        world: &mut World,
        client_tick: u64,
    ) {
        self.history.clear();
        if replica.local_entity().is_none() || !self.active {
            return;
        }
        restore_durable(world, replica);
        self.replay_pending(world);
        self.capture_tick_velocity(world);
        self.snap_tick_poses(world);
        self.reset_history(client_tick, world);
    }

    /// Sync prediction from the latest accepted authoritative replica.
    ///
    /// Restore durable state, drop commands ≤ ack, replay remaining unacked
    /// commands. Never writes into `replica`. Does not bump input epoch
    /// (server-owned).
    pub fn sync_from_replica(
        &mut self,
        replica: &ReplicatedWorld,
        world: &mut World,
        client_tick: u64,
    ) {
        self.last_auth_server_tick = replica.last_server_tick();
        let Some(auth) = replica.local_entity() else {
            self.clear();
            return;
        };
        sync_local_health(world, auth.health);
        self.last_sync_hard_snap = false;
        self.last_correction_delta = [0.0, 0.0];
        let durable = replica.local_durable_updated();
        self.last_sync_durable = durable;
        if durable {
            self.last_auth_position = Some(auth.position);
            self.last_auth_velocity = Some(auth.velocity);
        }

        let lead = predicted_error(world, auth.position);
        let split = self.aligned_residual(&auth);
        let aligned = split.map(|s| s.residual);
        let aligned_dx = split.map(|s| s.delta_position[0].abs());
        let aligned_dy = split.map(|s| s.delta_position[1].abs());
        let mapped = split.map(|s| s.sample.client_tick);

        if let Some(aerr) = aligned {
            self.max_aligned_error = self.max_aligned_error.max(aerr);
            if aerr >= PREDICTION_ALIGNED_RESIDUAL_WU {
                self.consecutive_aligned_divergence =
                    self.consecutive_aligned_divergence.saturating_add(1);
            } else {
                self.consecutive_aligned_divergence = 0;
            }
            self.last_aligned_error = Some(aerr);
        }

        let non_finite = !auth.position[0].is_finite()
            || !auth.position[1].is_finite()
            || !auth.velocity[0].is_finite()
            || !auth.velocity[1].is_finite();
        let unresolvable_support = replica.local_grounded()
            && replica
                .local_grounded_on()
                .get()
                .is_some_and(|id| world.platform_entity_by_support_id(id).is_none());
        let ack = replica.last_acknowledged_input_sequence();
        let snap_epoch = replica.input_epoch();
        let invalid_ack = ack > self.last_sent_seq && self.last_sent_seq > 0;
        let epoch_changed = self.active && snap_epoch != self.input_epoch;

        let auth_settled = auth.velocity[1].abs() <= PREDICTION_AUTH_SETTLED_VY;
        let vertical_split =
            auth_settled && aligned_dy.is_some_and(|d| d >= PREDICTION_VERTICAL_REANCHOR);

        let pending_before = self.pending.len();
        let was_active = self.active;
        let pre_pos = world.player_body().map(|b| b.position);
        self.note_ack_advancement(snap_epoch, ack);

        let reason = if !self.active {
            Some(SnapReason::FirstActive)
        } else if self.local_entity != Some(auth.entity_id) {
            Some(SnapReason::Generation)
        } else if non_finite || unresolvable_support || invalid_ack {
            Some(SnapReason::Structural)
        } else if epoch_changed {
            Some(SnapReason::Generation)
        } else if !durable {
            None
        } else if pending_before == 0 && aligned.is_some_and(|e| e >= PREDICTION_REANCHOR_DISTANCE)
        {
            Some(SnapReason::AlignedDistance)
        } else if pending_before == 0 && lead.is_some_and(|e| e >= PREDICTION_REANCHOR_DISTANCE) {
            Some(SnapReason::LeadSafety)
        } else if pending_before == 0 && vertical_split {
            Some(SnapReason::VerticalSettled)
        } else {
            None
        };

        tracing::debug!(
            target: "purgatory_pred",
            auth_tick = replica.last_server_tick(),
            client_tick,
            mapped_client_tick = ?mapped,
            lead = ?lead,
            aligned = ?aligned,
            aligned_dx = ?aligned_dx,
            aligned_dy = ?aligned_dy,
            ack,
            epoch = snap_epoch,
            pending = pending_before,
            snap = ?reason.map(SnapReason::as_str),
            "prediction snapshot reconcile"
        );

        self.input_epoch = snap_epoch;
        self.local_entity = Some(auth.entity_id);
        self.active = true;

        if matches!(
            reason,
            Some(SnapReason::FirstActive | SnapReason::Generation)
        ) {
            self.pending.clear();
            self.cancel_barrier = None;
            restore_durable(world, replica);
            self.reset_count = self.reset_count.saturating_add(1);
            self.prediction_tick = 0;
            self.last_sent_seq = 0;
            self.reset_history(client_tick, world);
            self.capture_tick_velocity(world);
            self.snap_tick_poses(world);
            self.consecutive_aligned_divergence = 0;
            self.last_aligned_error = Some(0.0);
            self.last_snap_reason = reason.map(SnapReason::as_str);
            self.last_sync_hard_snap = true;
            return;
        }

        if reason == Some(SnapReason::Structural) {
            self.pending.clear();
            self.cancel_barrier = None;
            restore_durable(world, replica);
            self.reset_count = self.reset_count.saturating_add(1);
            self.structural_snap_count = self.structural_snap_count.saturating_add(1);
            self.last_sent_seq = ack;
            self.reset_history(client_tick, world);
            self.capture_tick_velocity(world);
            self.snap_tick_poses(world);
            self.last_snap_reason = Some(SnapReason::Structural.as_str());
            self.last_sync_hard_snap = true;
            if was_active {
                self.finish_correction(pre_pos, world);
            }
            return;
        }

        let predicted_grounded = world.player_body().is_some_and(|b| b.grounded);
        let predicted_at_rest = world
            .player_body()
            .is_some_and(|b| b.grounded && b.velocity[0] == 0.0 && b.velocity[1] == 0.0);
        let landing_lead = predicted_grounded
            && auth.velocity[1] < -PREDICTION_AUTH_SETTLED_VY
            && pre_pos.is_some_and(|p| auth.position[1] > p[1] + 0.05);
        // Replica still carrying leftover walk velocity must not rewind a
        // locally settled idle body (remainder extra would sawtooth ~vx*dt).
        let rest_lead = predicted_at_rest && auth.velocity[0] != 0.0;

        if !durable || landing_lead || rest_lead {
            self.trim_pending(snap_epoch, ack);
            if let Some(barrier) = self.cancel_barrier
                && snap_epoch == barrier.epoch
                && ack >= barrier.target_sequence
            {
                self.cancel_barrier = None;
            }
            self.push_history_from_world(client_tick, world);
            if was_active {
                self.finish_correction(pre_pos, world);
            }
            return;
        }

        restore_durable(world, replica);
        self.trim_pending(snap_epoch, ack);
        if let Some(barrier) = self.cancel_barrier
            && snap_epoch == barrier.epoch
            && ack >= barrier.target_sequence
        {
            self.cancel_barrier = None;
        }
        let replayed = !self.pending.is_empty();
        self.replay_pending(world);
        if replayed {
            self.capture_tick_velocity(world);
        } else {
            // Restore with nothing to replay moved the sim body. Collapse Y
            // lerp only when that body actually left the last tick pose so a
            // rewind-to-ground cannot keep interpolating an airborne segment.
            // Do not snap when pending replay keeps us on the predicted
            // trajectory — that path used to run every durable snapshot and
            // made interpolate_tick_y jump a full tick of Y in one frame.
            let diverged = match (self.tick_pose, world.player_body().map(|b| b.position)) {
                (Some(tick), Some(post)) => {
                    (tick[1] - post[1]).abs() > PREDICTION_ALIGNED_RESIDUAL_WU
                }
                (None, Some(_)) => true,
                _ => false,
            };
            if diverged {
                self.snap_tick_poses(world);
            }
        }

        if reason == Some(SnapReason::VerticalSettled) {
            restore_durable(world, replica);
            self.reset_count = self.reset_count.saturating_add(1);
            self.vertical_settled_count = self.vertical_settled_count.saturating_add(1);
            self.reset_history(client_tick, world);
            self.capture_tick_velocity(world);
            self.snap_tick_poses(world);
            self.last_snap_reason = Some(SnapReason::VerticalSettled.as_str());
            self.last_sync_hard_snap = true;
            if was_active {
                self.finish_correction(pre_pos, world);
            }
            return;
        }
        if matches!(
            reason,
            Some(SnapReason::AlignedDistance | SnapReason::LeadSafety)
        ) {
            restore_durable(world, replica);
            self.reset_count = self.reset_count.saturating_add(1);
            self.reset_history(client_tick, world);
            self.capture_tick_velocity(world);
            self.snap_tick_poses(world);
            self.last_snap_reason = reason.map(SnapReason::as_str);
            self.last_sync_hard_snap = true;
            if was_active {
                self.finish_correction(pre_pos, world);
            }
            return;
        }

        self.push_history_from_world(client_tick, world);
        if was_active {
            self.finish_correction(pre_pos, world);
        }
    }

    fn note_ack_advancement(&mut self, snap_epoch: u16, ack: u32) {
        if self.active && snap_epoch == self.input_epoch {
            let delta = ack.saturating_sub(self.last_observed_ack);
            self.observed_ack_delta = delta;
            if delta > 1 {
                self.observed_ack_jump_count = self.observed_ack_jump_count.saturating_add(1);
            }
            self.max_observed_ack_delta = self.max_observed_ack_delta.max(delta);
        } else {
            self.observed_ack_delta = 0;
        }
        self.last_observed_ack = ack;
    }

    fn finish_correction(&mut self, pre: Option<[f32; 2]>, world: &World) {
        self.total_reconciliation_count = self.total_reconciliation_count.saturating_add(1);
        if let (Some(a), Some(body)) = (pre, world.player_body()) {
            self.last_correction_delta = [body.position[0] - a[0], body.position[1] - a[1]];
            let mag = distance(a, body.position);
            self.last_correction_wu = mag;
            if mag > self.max_correction_wu {
                self.max_correction_wu = mag;
            }
        } else {
            self.last_correction_delta = [0.0, 0.0];
            self.last_correction_wu = 0.0;
        }
    }

    fn trim_pending(&mut self, epoch: u16, ack: u32) {
        self.pending
            .retain(|cmd| cmd.input_epoch == epoch && cmd.sequence > ack);
    }

    fn replay_pending(&mut self, world: &mut World) {
        let dt = TICK_DURATION.as_secs_f32();
        for cmd in self.pending.iter().copied() {
            world.tick_predicted_player(dt, player_input_from_command(cmd));
        }
    }

    fn capture_tick_velocity(&mut self, world: &World) {
        self.last_tick_velocity = world
            .player_body()
            .map(|b| b.velocity)
            .unwrap_or([0.0, 0.0]);
    }

    fn note_tick_pose(&mut self, world: &World) {
        let Some(pos) = world.player_body().map(|b| b.position) else {
            return;
        };
        self.prev_tick_pose = self.tick_pose.or(Some(pos));
        self.tick_pose = Some(pos);
    }

    fn snap_tick_poses(&mut self, world: &World) {
        let pos = world.player_body().map(|b| b.position);
        self.tick_pose = pos;
        self.prev_tick_pose = pos;
    }

    fn push_history_from_world(&mut self, client_tick: u64, world: &World) {
        if let Some(body) = world.player_body() {
            self.push_history(PredSample {
                client_tick,
                position: body.position,
                velocity: body.velocity,
            });
        }
    }

    /// Force re-anchor from the current replica local player (networked Reset).
    /// Does not bump epoch. Pending is kept and replayed after restore.
    pub fn force_reanchor_from_replica(
        &mut self,
        replica: &ReplicatedWorld,
        world: &mut World,
        client_tick: u64,
    ) {
        let Some(auth) = replica.local_entity() else {
            return;
        };
        self.last_auth_server_tick = replica.last_server_tick();
        self.last_auth_position = Some(auth.position);
        self.last_auth_velocity = Some(auth.velocity);
        self.last_snap_reason = Some("force_reset");
        self.last_sync_hard_snap = true;
        restore_durable(world, replica);
        self.replay_pending(world);
        self.local_entity = Some(auth.entity_id);
        self.active = true;
        self.reset_count = self.reset_count.saturating_add(1);
        self.prediction_tick = 0;
        self.reset_history(client_tick, world);
        self.capture_tick_velocity(world);
        self.snap_tick_poses(world);
        self.consecutive_aligned_divergence = 0;
        self.last_aligned_error = Some(0.0);
    }

    /// Advance one fixed simulation tick using shared FOOTNOTE rules.
    ///
    /// `client_tick` must be the absolute simulation clock tick for this step.
    /// Does not record a pending command; the caller appends after a successful
    /// send-window check.
    pub fn tick(&mut self, world: &mut World, input: PlayerInput, client_tick: u64) {
        if !self.active {
            return;
        }
        let dt = TICK_DURATION.as_secs_f32();
        world.tick_predicted_player(dt, input);
        self.prediction_tick = self.prediction_tick.saturating_add(1);
        self.capture_tick_velocity(world);
        self.note_tick_pose(world);
        self.push_history_from_world(client_tick, world);
    }

    #[must_use]
    pub fn predicted_pose(&self, world: &World) -> Option<[f32; 2]> {
        if !self.active {
            return None;
        }
        world.player_body().map(|b| b.position)
    }

    #[must_use]
    pub fn predicted_velocity(&self, world: &World) -> Option<[f32; 2]> {
        if !self.active {
            return None;
        }
        world.player_body().map(|b| b.velocity)
    }

    #[must_use]
    pub fn diagnostics(&self, world: &World, replica: &ReplicatedWorld) -> PredictionDiagnostics {
        let auth = replica.local_entity();
        let pred_pos = self.predicted_pose(world);
        let lead = match (auth.map(|a| a.position), pred_pos) {
            (Some(a), Some(p)) => Some(distance(a, p)),
            _ => None,
        };
        let split = auth.and_then(|a| self.aligned_residual(&a));
        let aligned = split.map(|s| s.residual);
        let adx = split.map(|s| s.delta_position[0].abs());
        let ady = split.map(|s| s.delta_position[1].abs());
        let mapped = split.map(|s| s.sample.client_tick);
        let offset = split.map(|s| {
            self.history.back().map_or(0, |newest| {
                newest.client_tick as i64 - s.sample.client_tick as i64
            })
        });
        PredictionDiagnostics {
            enabled: true,
            active: self.active,
            auth_position: auth.map(|a| a.position),
            auth_velocity: auth.map(|a| a.velocity).or(self.last_auth_velocity),
            predicted_position: pred_pos,
            predicted_velocity: self.predicted_velocity(world),
            lead_error: lead,
            aligned_error: aligned,
            aligned_dx: adx,
            aligned_dy: ady,
            best_temporal_offset: offset,
            position_error: lead,
            prediction_tick: self.prediction_tick,
            auth_server_tick: self.last_auth_server_tick,
            best_match_tick: mapped,
            reset_count: self.reset_count,
            drift_correction_count: self.drift_correction_count,
            consecutive_aligned_divergence: self.consecutive_aligned_divergence,
            max_aligned_error: self.max_aligned_error,
            last_snap_reason: self.last_snap_reason,
            pending_count: self.pending.len() as u32,
            last_ack: replica.last_acknowledged_input_sequence(),
            input_epoch: self.input_epoch,
            last_sent_seq: self.last_sent_seq,
            continuation_debt: replica.continuation_debt(),
            hard_snap_count: self.reset_count,
            vertical_settled_count: self.vertical_settled_count,
            cancel_pending: self.cancel_barrier.is_some(),
            total_reconciliation_count: self.total_reconciliation_count,
            last_correction_wu: self.last_correction_wu,
            max_correction_wu: self.max_correction_wu,
            observed_ack_delta: self.observed_ack_delta,
            observed_ack_jump_count: self.observed_ack_jump_count,
            max_observed_ack_delta: self.max_observed_ack_delta,
            last_tick_velocity: self.last_tick_velocity(),
            pending_window_stall_ticks: self.pending_window_stall_ticks(),
        }
    }

    fn reset_history(&mut self, client_tick: u64, world: &World) {
        self.history.clear();
        if let Some(body) = world.player_body() {
            self.push_history(PredSample {
                client_tick,
                position: body.position,
                velocity: body.velocity,
            });
        }
    }

    fn push_history(&mut self, sample: PredSample) {
        if self
            .history
            .back()
            .is_some_and(|s| s.client_tick == sample.client_tick)
        {
            *self.history.back_mut().expect("back exists") = sample;
        } else {
            if self.history.len() >= PREDICTION_HISTORY_CAP {
                self.history.pop_front();
            }
            self.history.push_back(sample);
        }
    }

    /// Residual after temporal alignment, plus the offset that explains it.
    ///
    /// The client tick matching a server tick is unknowable without an
    /// acknowledgement in the snapshot; the previous
    /// `client0 + (server − server0)` map silently assumed zero pipeline delay,
    /// so it reported raw lead as if it were same-tick error. Searching the
    /// history ring instead is latency-immune: a state the authority has simply
    /// not reached yet (or has already passed) still lies on the predicted
    /// path, and matches at some offset with a near-zero residual.
    ///
    /// Velocity participates in the residual, scaled by one tick, so a sample
    /// that merely shares a position (same height mid-jump, opposite `vy`)
    /// cannot masquerade as a temporal match. Candidates are limited to
    /// [`PREDICTION_MAX_PLAUSIBLE_LAG_TICKS`]: a match on a path left seconds
    /// ago is drift, not lag.
    fn aligned_residual(&self, auth: &ReplicatedEntity) -> Option<AlignedResidual> {
        let now = self.history.back()?.client_tick;
        let scored = |s: &PredSample| {
            distance(s.position, auth.position)
                + distance(s.velocity, auth.velocity) * TICK_DURATION.as_secs_f32()
        };
        let sample = *self
            .history
            .iter()
            .filter(|s| now.saturating_sub(s.client_tick) <= PREDICTION_MAX_PLAUSIBLE_LAG_TICKS)
            .min_by(|a, b| {
                scored(a)
                    .total_cmp(&scored(b))
                    .then(b.client_tick.cmp(&a.client_tick))
            })?;
        Some(AlignedResidual {
            residual: scored(&sample),
            delta_position: [
                auth.position[0] - sample.position[0],
                auth.position[1] - sample.position[1],
            ],
            delta_velocity: [
                auth.velocity[0] - sample.velocity[0],
                auth.velocity[1] - sample.velocity[1],
            ],
            sample,
        })
    }
}

/// Temporally aligned comparison against one authoritative snapshot.
#[derive(Clone, Copy, Debug)]
struct AlignedResidual {
    /// World-unit residual left once the best tick offset is accounted for.
    residual: f32,
    delta_position: [f32; 2],
    #[allow(dead_code)]
    delta_velocity: [f32; 2],
    sample: PredSample,
}

/// Explicit presentation-pose resolver for the local player.
///
/// Predicted when active; otherwise authoritative replica (or `None`).
///
/// This is the **source** pose for [`crate::local_presentation::LocalPresentation`].
/// Draw and camera follow the finalized presentation pose, not this value
/// directly, so small restore+replay pops are not shown 1:1.
///
/// When `maps_aligned` is false the replica already belongs to a new
/// `WorldAddress` while local geometry does not. Presentation must keep the
/// old-map pose and must not snap to the new-epoch self Enter.
#[must_use]
pub fn local_presentation_pose(
    prediction: &LocalPrediction,
    world: &World,
    replica: &ReplicatedWorld,
    maps_aligned: bool,
) -> Option<[f32; 2]> {
    if !maps_aligned {
        if prediction.active() {
            return prediction.predicted_pose(world);
        }
        return world.player_body().map(|b| b.position);
    }
    if prediction.active() {
        prediction.predicted_pose(world)
    } else {
        replica.local_entity().map(|e| e.position)
    }
}

fn restore_durable(world: &mut World, replica: &ReplicatedWorld) {
    let Some(auth) = replica.local_entity() else {
        return;
    };
    world.restore_player_sim_state(
        auth.position,
        auth.velocity,
        replica.local_grounded(),
        replica.local_grounded_on().get(),
        replica.local_ignored_platform().get(),
    );
}

fn sync_local_health(world: &mut World, health: Option<purgatory_protocol::ReplicatedHealth>) {
    let Some(health) = health else {
        return;
    };
    let Some(id) = world.player_id() else {
        return;
    };
    world.set_health(
        id,
        Health {
            current: health.current,
            max: health.max,
        },
    );
}

fn player_input_from_command(cmd: InputCommand) -> PlayerInput {
    PlayerInput {
        move_axis: cmd.move_axis.to_i8(),
        jump_pressed: cmd.jump_pressed,
        down_held: cmd.down_held,
    }
}

fn predicted_error(world: &World, auth: [f32; 2]) -> Option<f32> {
    let body = world.player_body()?;
    Some(distance(body.position, auth))
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::ActionState;
    use crate::interp::{InterpolationBuffer, PresentationPose};
    use purgatory_protocol::{
        InputCommand, MoveAxis, PlatformSupportId, ReplicatedHealth, ReplicatedKind,
        ReplicationFrame, ReplicationRecord, SnapshotEntity, WorldSnapshot,
    };
    use purgatory_simulation::{MAX_CATCH_UP_TICKS, SimulationClock};
    use std::time::Duration;

    fn wire(index: u32, generation: u32) -> WireEntityId {
        WireEntityId { index, generation }
    }

    fn snap(
        seq: u32,
        tick: u64,
        local: WireEntityId,
        pos: [f32; 2],
        vel: [f32; 2],
    ) -> WorldSnapshot {
        let mut snap = WorldSnapshot::from_poses(
            seq,
            tick,
            local,
            vec![SnapshotEntity {
                entity_id: local,
                kind: ReplicatedKind::Player,
                position: pos,
                velocity: vel,
            }],
        );
        snap.local_grounded = vel[1].abs() <= PREDICTION_AUTH_SETTLED_VY;
        snap.local_grounded_on = if snap.local_grounded {
            PlatformSupportId(1)
        } else {
            PlatformSupportId::NONE
        };
        snap
    }

    fn cmd(seq: u32, input: PlayerInput) -> InputCommand {
        InputCommand {
            input_epoch: 0,
            sequence: seq,
            move_axis: match input.move_axis {
                -1 => MoveAxis::Left,
                1 => MoveAxis::Right,
                _ => MoveAxis::Neutral,
            },
            jump_pressed: input.jump_pressed,
            down_held: input.down_held,
            portal_held: false,
        }
    }

    fn tick_recorded(
        pred: &mut LocalPrediction,
        world: &mut World,
        input: PlayerInput,
        client_tick: u64,
        seq: u32,
    ) {
        pred.tick(world, input, client_tick);
        assert!(pred.try_push_pending(cmd(seq, input)));
    }

    fn auth_at(replica: &mut ReplicatedWorld, seq: u32, id: WireEntityId, pos: [f32; 2]) {
        let _ = replica.apply(snap(seq, u64::from(seq), id, pos, [0.0, 0.0]));
    }

    fn dead_auth_frame(id: WireEntityId, pos: [f32; 2]) -> ReplicationFrame {
        ReplicationFrame {
            snapshot_sequence: 1,
            server_tick: 1,
            local_player_entity: id,
            input_epoch: 0,
            last_acknowledged_input_sequence: 0,
            local_grounded: true,
            local_grounded_on: PlatformSupportId(1),
            local_ignored_platform: PlatformSupportId::NONE,
            continuation_debt: 0,
            local_map: 1,
            local_channel: 0,
            local_instance: 0,
            observer_baseline_epoch: 1,
            records: vec![ReplicationRecord::Enter {
                entity: SnapshotEntity {
                    entity_id: id,
                    kind: ReplicatedKind::Player,
                    position: pos,
                    velocity: [0.0, 0.0],
                },
                health: Some(ReplicatedHealth {
                    current: 0.0,
                    max: 20.0,
                }),
                equipment: None,
            }],
            aoi_debug: None,
        }
    }

    #[test]
    fn prediction_starts_from_authoritative_local_state() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(3, 1);
        auth_at(&mut replica, 1, id, [4.0, 3.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        assert!(pred.active());
        assert_eq!(pred.local_entity(), Some(id));
        assert_eq!(pred.reset_count(), 1);
        let body = world.player_body().expect("player");
        assert_eq!(body.position, [4.0, 3.0]);
        assert_eq!(
            local_presentation_pose(&pred, &world, &replica, true),
            Some([4.0, 3.0])
        );
    }

    #[test]
    fn dead_replica_health_blocks_local_predicted_locomotion() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        assert!(matches!(
            replica.apply_frame(dead_auth_frame(id, [0.0, 2.0])),
            crate::replica::FrameDecision::Applied { .. }
        ));
        pred.sync_from_replica(&replica, &mut world, 1);
        let start = world.player_body().expect("player").position;

        for _ in 0..10 {
            pred.tick(
                &mut world,
                PlayerInput::from_buttons_ext(false, true, true, false),
                1,
            );
        }

        let body = world.player_body().expect("player");
        assert_eq!(
            world.health_of(world.player_id().unwrap()).unwrap().current,
            0.0
        );
        assert!((body.position[0] - start[0]).abs() < 1e-4);
        assert_eq!(body.velocity[0], 0.0);
        assert!(
            body.velocity[1] <= 0.0,
            "dead predicted player must not receive a jump impulse"
        );
    }

    #[test]
    fn move_right_advances_without_new_snapshot() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        auth_at(&mut replica, 1, id, [0.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        let start = world.player_body().unwrap().position;
        let auth_before = replica.local_entity().unwrap().position;
        for _ in 0..10 {
            pred.tick(
                &mut world,
                PlayerInput::from_buttons_ext(false, true, false, false),
                1,
            );
        }
        let after = world.player_body().unwrap().position;
        assert!(after[0] > start[0], "predicted x should increase");
        assert_eq!(
            replica.local_entity().unwrap().position,
            auth_before,
            "ReplicatedWorld must not be overwritten"
        );
        assert_eq!(pred.prediction_tick(), 10);
    }

    #[test]
    fn move_left_advances_equivalently() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        auth_at(&mut replica, 1, id, [0.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        let start = world.player_body().unwrap().position[0];
        for _ in 0..10 {
            pred.tick(
                &mut world,
                PlayerInput::from_buttons_ext(true, false, false, false),
                1,
            );
        }
        assert!(world.player_body().unwrap().position[0] < start);
    }

    #[test]
    fn neutral_input_follows_simulation_friction() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        auth_at(&mut replica, 1, id, [0.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        for _ in 0..8 {
            pred.tick(
                &mut world,
                PlayerInput::from_buttons_ext(false, true, false, false),
                1,
            );
        }
        let moving_vx = world.player_body().unwrap().velocity[0];
        assert!(moving_vx.abs() > 0.1);
        for _ in 0..30 {
            pred.tick(&mut world, PlayerInput::idle(), 1);
        }
        let stopped_vx = world.player_body().unwrap().velocity[0];
        assert!(
            stopped_vx.abs() < moving_vx.abs(),
            "neutral should bleed speed under FOOTNOTE friction"
        );
    }

    #[test]
    fn jump_is_predicted_locally() {
        let mut world = World::footnote_test_stage();
        let spawn = world.player_body().unwrap().position;
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        for _ in 0..10 {
            pred.tick(&mut world, PlayerInput::idle(), 1);
        }
        assert!(world.player_body().unwrap().grounded);
        let y0 = world.player_body().unwrap().position[1];
        pred.tick(
            &mut world,
            PlayerInput::from_buttons_ext(false, false, true, false),
            1,
        );
        let y1 = world.player_body().unwrap().position[1];
        let vy = world.player_body().unwrap().velocity[1];
        assert!(vy > 0.0 || y1 > y0, "jump should lift immediately");
    }

    #[test]
    fn fixed_tick_progression_is_deterministic() {
        fn run() -> Vec<[f32; 2]> {
            let mut world = World::footnote_test_stage();
            let mut replica = ReplicatedWorld::new();
            let mut pred = LocalPrediction::new();
            auth_at(&mut replica, 1, wire(1, 1), [-1.0, 2.0]);
            pred.sync_from_replica(&replica, &mut world, 1);
            let mut path = Vec::new();
            for i in 0..20 {
                let jump = i == 5;
                pred.tick(
                    &mut world,
                    PlayerInput::from_buttons_ext(false, true, jump, false),
                    1,
                );
                path.push(world.player_body().unwrap().position);
            }
            path
        }
        assert_eq!(run(), run());
    }

    #[test]
    fn remote_entities_remain_interpolation_based() {
        // Local prediction never synthesizes remote poses; remotes stay on
        // InterpolationBuffer (Phase 5.3). Full interp math is tested there.
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let local = wire(1, 1);
        let remote = wire(2, 1);
        let _ = replica.apply(WorldSnapshot::from_poses(
            1,
            1,
            local,
            vec![
                SnapshotEntity {
                    entity_id: local,
                    kind: ReplicatedKind::Player,
                    position: [0.0, 2.0],
                    velocity: [0.0, 0.0],
                },
                SnapshotEntity {
                    entity_id: remote,
                    kind: ReplicatedKind::Player,
                    position: [5.0, 2.0],
                    velocity: [0.0, 0.0],
                },
            ],
        ));
        pred.sync_from_replica(&replica, &mut world, 1);
        for _ in 0..5 {
            pred.tick(
                &mut world,
                PlayerInput::from_buttons_ext(false, true, false, false),
                1,
            );
        }
        let remote_auth = replica.get(remote).unwrap().position;
        assert_eq!(remote_auth, [5.0, 2.0]);
        assert_ne!(
            world.player_body().unwrap().position,
            remote_auth,
            "prediction must not move remote replica entries"
        );
        let _interp = InterpolationBuffer::new();
        assert!(
            pred.predicted_pose(&world).is_some(),
            "local prediction poses only the local body"
        );
        let _: &[PresentationPose] = &[];
    }

    #[test]
    fn replica_not_overwritten_by_prediction() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        auth_at(&mut replica, 1, id, [2.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        for _ in 0..15 {
            pred.tick(
                &mut world,
                PlayerInput::from_buttons_ext(false, true, false, false),
                1,
            );
        }
        assert_eq!(replica.local_entity().unwrap().position, [2.0, 2.0]);
        assert_eq!(replica.len(), 1);
    }

    #[test]
    fn focus_loss_input_reset_affects_prediction() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let mut actions = ActionState::default();
        auth_at(&mut replica, 1, wire(1, 1), [0.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        actions.set_action(crate::input::Action::MoveRight, true, false);
        for _ in 0..8 {
            let input = actions.consume_tick_input();
            pred.tick(&mut world, input, 1);
        }
        let vx_held = world.player_body().unwrap().velocity[0];
        assert!(vx_held > 0.1);
        actions.release_on_focus_loss();
        assert!(!actions.has_held_input());
        for _ in 0..20 {
            let input = actions.consume_tick_input();
            pred.tick(&mut world, input, 1);
        }
        let vx_after = world.player_body().unwrap().velocity[0];
        assert!(
            vx_after.abs() < vx_held,
            "cleared input must stop predicted motion"
        );
    }

    #[test]
    fn prediction_resets_on_entity_generation_change() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id_v1 = wire(5, 1);
        auth_at(&mut replica, 1, id_v1, [1.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        for _ in 0..10 {
            pred.tick(
                &mut world,
                PlayerInput::from_buttons_ext(false, true, false, false),
                1,
            );
        }
        let drifted = world.player_body().unwrap().position;
        assert_ne!(drifted, [1.0, 2.0]);
        let resets = pred.reset_count();
        // Reconnect: same index, new generation, new spawn.
        let id_v2 = wire(5, 2);
        auth_at(&mut replica, 2, id_v2, [-3.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        assert_eq!(pred.local_entity(), Some(id_v2));
        assert!(pred.reset_count() > resets);
        assert_eq!(world.player_body().unwrap().position, [-3.0, 2.0]);
        assert_eq!(pred.prediction_tick(), 0);
    }

    #[test]
    fn prediction_clears_when_no_local_player() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        auth_at(&mut replica, 1, wire(1, 1), [0.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        assert!(pred.active());
        replica.clear();
        pred.sync_from_replica(&replica, &mut world, 1);
        assert!(!pred.active());
        assert!(local_presentation_pose(&pred, &world, &replica, true).is_none());
    }

    #[test]
    fn presentation_resolver_prefers_predicted_pose() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        auth_at(&mut replica, 1, wire(1, 1), [0.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        for _ in 0..5 {
            pred.tick(
                &mut world,
                PlayerInput::from_buttons_ext(false, true, false, false),
                1,
            );
        }
        let pose = local_presentation_pose(&pred, &world, &replica, true).unwrap();
        assert_eq!(pose, world.player_body().unwrap().position);
        assert_ne!(pose, replica.local_entity().unwrap().position);
    }

    #[test]
    fn unaligned_maps_do_not_present_new_epoch_replica_pose() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        auth_at(&mut replica, 1, id, [0.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        let old = world.player_body().unwrap().position;
        auth_at(&mut replica, 2, id, [10.0, -2.3]);
        pred.clear();
        let presented = local_presentation_pose(&pred, &world, &replica, false).unwrap();
        assert_eq!(presented, old);
        assert!((presented[0] - 10.0).abs() > 1.0);
        let aligned = local_presentation_pose(&pred, &world, &replica, true).unwrap();
        assert!((aligned[0] - 10.0).abs() < 1e-4);
    }

    #[test]
    fn large_auth_error_reanchors() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        auth_at(&mut replica, 1, id, [0.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        // Manually teleport predicted body far away.
        if let Some((t, _)) = world.player_parts_mut() {
            t.position = [PREDICTION_REANCHOR_DISTANCE + 2.0, 2.0];
        }
        let resets = pred.reset_count();
        // Same entity; auth still at origin → error forces re-anchor.
        auth_at(&mut replica, 2, id, [0.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        assert!(pred.reset_count() > resets);
        assert_eq!(world.player_body().unwrap().position, [0.0, 2.0]);
    }

    #[test]
    fn bounded_catch_up_after_frame_stall() {
        let mut clock = SimulationClock::new();
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        auth_at(&mut replica, 1, wire(1, 1), [0.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        let update = clock.advance(Duration::from_secs(5));
        assert!(update.ticks_executed <= MAX_CATCH_UP_TICKS + 1);
        for _ in 0..update.ticks_executed {
            pred.tick(&mut world, PlayerInput::idle(), 1);
        }
        assert_eq!(pred.prediction_tick(), u64::from(update.ticks_executed));
        let pos = world.player_body().unwrap().position;
        assert!(pos[0].is_finite() && pos[1].is_finite());
    }

    #[test]
    fn oneway_drop_through_uses_shared_simulation() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        // Multi-drop OneWay stack at x=10; top surface ≈ 1.69 → stand at 2.29.
        auth_at(&mut replica, 1, wire(1, 1), [10.0, 2.29]);
        pred.sync_from_replica(&replica, &mut world, 1);
        for _ in 0..25 {
            pred.tick(&mut world, PlayerInput::idle(), 1);
        }
        let grounded_y = world.player_body().unwrap().position[1];
        assert!(
            world.player_body().unwrap().grounded,
            "should settle on OneWay before drop-through"
        );
        // Drop-through: down + jump edge per FOOTNOTE rules.
        pred.tick(
            &mut world,
            PlayerInput::from_buttons_ext(false, false, true, true),
            1,
        );
        for _ in 0..20 {
            pred.tick(
                &mut world,
                PlayerInput::from_buttons_ext(false, false, false, true),
                1,
            );
        }
        let after = world.player_body().unwrap().position[1];
        assert!(
            after < grounded_y - 0.2,
            "drop-through should lower the predicted player (before={grounded_y} after={after})"
        );
    }

    #[test]
    fn reconnect_clear_then_reinit() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        auth_at(&mut replica, 1, wire(1, 1), [2.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        pred.clear();
        replica.clear();
        assert!(!pred.active());
        assert_eq!(pred.prediction_tick(), 0);
        auth_at(&mut replica, 1, wire(9, 1), [-1.0, 2.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        assert!(pred.active());
        assert_eq!(world.player_body().unwrap().position, [-1.0, 2.0]);
    }

    #[test]
    fn inactive_prediction_does_not_tick_world_as_predicted() {
        let mut world = World::footnote_test_stage();
        let before = world.player_body().unwrap().position;
        let mut pred = LocalPrediction::new();
        pred.tick(
            &mut world,
            PlayerInput::from_buttons_ext(false, true, false, false),
            1,
        );
        assert_eq!(world.player_body().unwrap().position, before);
        assert_eq!(pred.prediction_tick(), 0);
    }

    #[test]
    fn identical_worlds_same_inputs_stay_deterministic() {
        let mut a = World::footnote_test_stage();
        let mut b = World::footnote_test_stage();
        let inputs = [
            PlayerInput::from_buttons_ext(false, true, false, false),
            PlayerInput::from_buttons_ext(false, true, false, false),
            PlayerInput::from_buttons_ext(false, true, true, false),
            PlayerInput::idle(),
            PlayerInput::from_buttons_ext(true, false, false, false),
            PlayerInput::from_buttons_ext(false, false, false, true),
            PlayerInput::from_buttons_ext(false, false, true, true),
            PlayerInput::idle(),
        ];
        let dt = TICK_DURATION.as_secs_f32();
        for input in inputs {
            a.tick(dt, input);
            b.tick(dt, input);
        }
        let ba = a.player_body().unwrap();
        let bb = b.player_body().unwrap();
        assert_eq!(ba.position, bb.position);
        assert_eq!(ba.velocity, bb.velocity);
        assert_eq!(ba.grounded, bb.grounded);
        assert_eq!(ba.ignored_platform.is_some(), bb.ignored_platform.is_some());
    }

    #[test]
    fn reanchor_restores_grounding_when_auth_settled() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, wire(1, 1), spawn);
        if let Some((t, p)) = world.player_parts_mut() {
            t.position = [spawn[0] + 2.0, spawn[1] + 3.0];
            p.velocity = [1.0, 5.0];
            p.grounded = false;
            p.grounded_on = None;
        }
        pred.sync_from_replica(&replica, &mut world, 1);
        let body = world.player_body().unwrap();
        assert!(
            body.grounded,
            "settled auth reanchor must restore grounded from geometry"
        );
        assert!(body.grounded_on.is_some());
        assert!((body.position[0] - spawn[0]).abs() < 1e-3);
        assert!((body.position[1] - spawn[1]).abs() < 0.15);
        assert_eq!(body.velocity, [0.0, 0.0]);
    }

    #[test]
    fn vertical_divergence_triggers_reanchor_without_raising_8wu() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = World::footnote_test_stage().player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 10);
        let resets = pred.reset_count();
        if let Some((t, _)) = world.player_parts_mut() {
            t.position = [spawn[0], spawn[1] + 2.5];
        }
        // A real path split: the whole plausible-lag window is elevated, so no
        // tick offset can explain the authority standing 2.5 wu below.
        for t in 11..=40 {
            pred.record_history_for_test(t, &world);
        }
        let err = distance(
            world.player_body().unwrap().position,
            replica.local_entity().unwrap().position,
        );
        assert!(err < PREDICTION_REANCHOR_DISTANCE);
        assert!(err >= PREDICTION_VERTICAL_REANCHOR);
        // Settled auth (vy=0) + large *aligned* dy → vertical snap.
        let _ = replica.apply(snap(2, 2, id, spawn, [0.0, 0.0]));
        pred.sync_from_replica(&replica, &mut world, 40);
        assert!(
            pred.reset_count() > resets,
            "settled vertical criterion must reanchor"
        );
        assert_eq!(pred.last_snap_reason(), Some("vertical_settled"));
        assert!((world.player_body().unwrap().position[1] - spawn[1]).abs() < 0.15);
    }

    #[test]
    fn lead_error_alone_does_not_reanchor_below_safety() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = [0.0, 2.0];
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let resets = pred.reset_count();
        // Advance prediction ahead of auth (RTT lead) while history stays aligned.
        let right = PlayerInput::from_buttons_ext(false, true, false, false);
        for t in 2..=12u64 {
            tick_recorded(&mut pred, &mut world, right, t, t as u32 - 1);
        }
        // Auth still at spawn (lagged snapshot, ack 0). Replay pending preserves lead.
        let mut lagged = snap(2, 2, id, spawn, [0.0, 0.0]);
        lagged.last_acknowledged_input_sequence = 0;
        let _ = replica.apply(lagged);
        pred.sync_from_replica(&replica, &mut world, 12);
        let diag = pred.diagnostics(&world, &replica);
        assert!(
            diag.lead_error.unwrap_or(0.0) > 0.5,
            "lead should reflect RTT ahead-of-auth motion"
        );
        assert!(
            diag.aligned_error.unwrap_or(999.0) < 0.35,
            "aligned compare at auth tick must stay near zero when history matches"
        );
        assert!(pred.last_aligned_error().unwrap_or(999.0) < 0.35);
        assert_eq!(pred.reset_count(), resets, "lead alone must not hard-snap");
    }

    #[test]
    fn temporal_lag_within_window_is_not_corrected() {
        // Authority sits on a pose this client held a few ticks ago, with the
        // matching velocity: a plausible tick offset explains it entirely.
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let resets = pred.reset_count();
        let held = PlayerInput::from_buttons_ext(false, true, false, false);
        const LAG: u32 = 5;
        let mut delayed: VecDeque<([f32; 2], [f32; 2])> = VecDeque::new();
        let mut seq = 0u32;
        for t in 2..=40u64 {
            seq = seq.saturating_add(1);
            tick_recorded(&mut pred, &mut world, held, t, seq);
            let body = world.player_body().unwrap();
            delayed.push_back((body.position, body.velocity));
            if delayed.len() > LAG as usize {
                let (position, velocity) = delayed.pop_front().expect("lagged pose");
                let mut s = snap(t as u32, t, id, position, velocity);
                s.last_acknowledged_input_sequence = seq.saturating_sub(LAG);
                let _ = replica.apply(s);
                pred.sync_from_replica(&replica, &mut world, t);
            }
        }
        let diag = pred.diagnostics(&world, &replica);
        assert!(
            diag.lead_error.unwrap_or(0.0) > 0.5,
            "a five-tick lag at speed must show as raw lead"
        );
        assert!(
            diag.aligned_error.unwrap_or(999.0) < PREDICTION_ALIGNED_RESIDUAL_WU,
            "aligned residual must absorb pure temporal lag, got {:?}",
            diag.aligned_error
        );
        assert_eq!(diag.consecutive_aligned_divergence, 0);
        assert_eq!(
            pred.reset_count(),
            resets,
            "temporal lag must never trigger a correction"
        );
    }

    #[test]
    fn persistent_at_rest_residual_is_not_offset_corrected() {
        // DriftCorrection is removed. Pending-empty restore places the body on
        // authority; 0.2 wu is below the 8 wu failsafe.
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let resets = pred.reset_count();
        let drifted = [spawn[0] + 0.2, spawn[1]];
        for t in 2..=8u64 {
            pred.tick(&mut world, PlayerInput::idle(), t);
            let _ = replica.apply(snap(t as u32, t, id, drifted, [0.0, 0.0]));
            pred.sync_from_replica(&replica, &mut world, t);
        }
        assert_eq!(pred.reset_count(), resets, "sub-8wu must not hard-snap");
        assert_eq!(pred.drift_correction_count(), 0);
        assert_ne!(pred.last_snap_reason(), Some("drift_correction"));
        let body = world.player_body().unwrap();
        assert!((body.position[0] - drifted[0]).abs() < 0.01);
    }

    #[test]
    fn stale_match_outside_window_is_not_drift_corrected() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let right = PlayerInput::from_buttons_ext(false, true, false, false);
        for t in 2..=20u64 {
            pred.tick(&mut world, right, t);
        }
        for t in 21..=60u64 {
            pred.tick(&mut world, PlayerInput::idle(), t);
        }
        let _ = replica.apply(snap(2, 2, id, spawn, [0.0, 0.0]));
        let diag = pred.diagnostics(&world, &replica);
        assert!(
            diag.aligned_error.unwrap_or(0.0) > PREDICTION_ALIGNED_RESIDUAL_WU,
            "a pose left 40+ ticks ago is not a temporal explanation, got {:?}",
            diag.aligned_error
        );
        pred.sync_from_replica(&replica, &mut world, 60);
        assert_eq!(pred.drift_correction_count(), 0);
        assert_ne!(pred.last_snap_reason(), Some("drift_correction"));
    }

    #[test]
    fn delayed_input_lead_stays_bounded() {
        // Dual sim: auth applies the same held input D ticks late. Lead (pred−auth
        // at the same wall tick) must stay bounded — not grow without limit.
        const DELAY: usize = 4;
        let mut auth = World::footnote_test_stage();
        let mut pred = World::footnote_test_stage();
        let dt = TICK_DURATION.as_secs_f32();
        let input = PlayerInput::from_buttons_ext(false, true, false, false);
        let mut pending = std::collections::VecDeque::new();
        let mut max_lead = 0.0_f32;
        let mut last_leads = Vec::new();
        for _ in 0..90 {
            pending.push_back(input);
            pred.tick(dt, input);
            let auth_in = if pending.len() > DELAY {
                pending.pop_front().unwrap()
            } else {
                PlayerInput::idle()
            };
            auth.tick(dt, auth_in);
            let a = auth.player_body().unwrap().position;
            let p = pred.player_body().unwrap().position;
            let lead = distance(a, p);
            max_lead = max_lead.max(lead);
            last_leads.push(lead);
        }
        assert!(
            max_lead < 3.5,
            "delayed-input lead must stay bounded, max_lead={max_lead}"
        );
        let early = last_leads[40];
        let late = *last_leads.last().unwrap();
        assert!(
            late < early + 0.75,
            "lead must not keep accumulating after spin-up early={early} late={late}"
        );
    }

    #[test]
    fn airborne_auth_phase_lag_does_not_vertical_reanchor() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = World::footnote_test_stage().player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let resets = pred.reset_count();
        // Pred on a platform; auth still falling (phase lag) with |dy| > threshold.
        if let Some((t, p)) = world.player_parts_mut() {
            t.position = [spawn[0] + 1.0, spawn[1] + 2.5];
            p.grounded = true;
            p.velocity = [6.0, 0.0];
        }
        let falling = [spawn[0], spawn[1] + 0.3];
        let _ = replica.apply(snap(2, 2, id, falling, [0.0, -11.0]));
        let dy = (world.player_body().unwrap().position[1] - falling[1]).abs();
        assert!(dy >= PREDICTION_VERTICAL_REANCHOR);
        pred.sync_from_replica(&replica, &mut world, 1);
        assert_eq!(
            pred.reset_count(),
            resets,
            "falling auth must not yank prediction via vertical snap"
        );
    }

    #[test]
    fn force_reanchor_matches_replica_not_footnote_spawn() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        auth_at(&mut replica, 1, id, [5.0, -3.0]);
        pred.sync_from_replica(&replica, &mut world, 1);
        for _ in 0..20 {
            pred.tick(
                &mut world,
                PlayerInput::from_buttons_ext(false, true, false, false),
                1,
            );
        }
        let auth = replica.local_entity().unwrap().position;
        pred.force_reanchor_from_replica(&replica, &mut world, 1);
        let pred_pos = world.player_body().unwrap().position;
        assert!((pred_pos[0] - auth[0]).abs() < 1e-3);
        assert!((pred_pos[1] - auth[1]).abs() < 0.15);
        assert!((pred_pos[0] - purgatory_simulation::FOOTNOTE_SPAWN_X).abs() > 1.0);
        assert_eq!(replica.local_entity().unwrap().position, auth);
    }

    #[test]
    fn complete_contact_copy_keeps_worlds_aligned() {
        let mut auth = World::footnote_test_stage();
        let mut pred = World::footnote_test_stage();
        let dt = TICK_DURATION.as_secs_f32();
        for _ in 0..10 {
            auth.tick(dt, PlayerInput::idle());
            pred.tick(dt, PlayerInput::idle());
        }
        let auth_body = auth.player_body().unwrap();
        if let Some((t, p)) = pred.player_parts_mut() {
            t.position = auth_body.position;
            p.velocity = auth_body.velocity;
            p.grounded = auth_body.grounded;
            p.grounded_on = auth_body.grounded_on;
            p.ignored_platform = auth_body.ignored_platform;
            p.last_contact = auth_body.last_contact;
        }
        for _ in 0..40 {
            let input = PlayerInput::from_buttons_ext(false, true, false, false);
            auth.tick(dt, input);
            pred.tick(dt, input);
        }
        let a = auth.player_body().unwrap();
        let p = pred.player_body().unwrap();
        let err = distance(a.position, p.position);
        assert!(
            err < 1e-3,
            "full contact copy must stay aligned, err={err} auth={:?} pred={:?}",
            a.position,
            p.position
        );
    }

    #[test]
    fn snapshot_header_carries_ack_and_contact_entity_stays_pose_only() {
        let src = include_str!("../../../crates/protocol/src/snapshot.rs");
        assert!(src.contains("last_acknowledged_input_sequence"));
        assert!(src.contains("local_grounded"));
        assert!(src.contains("continuation_debt"));
        assert!(src.contains("pub struct SnapshotEntity"));
        let entity = src
            .split("pub struct SnapshotEntity")
            .nth(1)
            .expect("entity")
            .split('}')
            .next()
            .expect("body");
        assert!(entity.contains("position"));
        assert!(entity.contains("velocity"));
        assert!(
            !entity.contains("grounded"),
            "SnapshotEntity remains pose-only; contact is the header"
        );
    }

    #[test]
    fn hitch_replays_pending_and_emits_one_new_command() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let right = PlayerInput::from_buttons_ext(false, true, false, false);
        for seq in 1..=4 {
            tick_recorded(&mut pred, &mut world, right, u64::from(seq) + 1, seq);
        }
        assert_eq!(pred.pending_len(), 4);
        let mut clock = SimulationClock::new();
        let update = clock.advance(Duration::from_secs(1));
        assert_eq!(update.ticks_executed, 30);
        pred.on_hitch_discontinuity(&replica, &mut world, clock.tick().get());
        assert_eq!(pred.pending_len(), 4, "hitch keeps pending");
        let idle = PlayerInput::idle();
        pred.tick(&mut world, idle, clock.tick().get());
        assert!(pred.try_push_pending(cmd(5, idle)));
        assert_eq!(pred.pending_len(), 5);
        assert_eq!(pred.prediction_tick(), 5);
    }

    #[test]
    fn held_cancel_barrier_ignores_stale_ack_until_target() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let right = PlayerInput::from_buttons_ext(false, true, false, false);
        for seq in 1..=3 {
            tick_recorded(&mut pred, &mut world, right, u64::from(seq) + 1, seq);
        }
        let barrier = pred.capture_cancel_barrier();
        assert_eq!(
            barrier,
            CancelBarrier {
                epoch: 0,
                target_sequence: 3
            }
        );
        pred.enter_cancel_pending(barrier);
        let _ = pred.try_push_pending(cmd(4, right));
        let still = pred.capture_cancel_barrier();
        assert_eq!(
            still.target_sequence, 4,
            "capture after more sends would mutate; stored barrier must not"
        );
        assert!(pred.cancel_pending());
        let mut stale = snap(2, 2, id, spawn, [0.0, 0.0]);
        stale.last_acknowledged_input_sequence = 1;
        let _ = replica.apply(stale);
        pred.sync_from_replica(&replica, &mut world, 10);
        assert!(
            pred.cancel_pending(),
            "pre-confirm snapshot must not clear the barrier"
        );
        assert!(pred.pending_len() >= 1);
        let mut confirm = snap(3, 3, id, spawn, [0.0, 0.0]);
        confirm.last_acknowledged_input_sequence = 3;
        let _ = replica.apply(confirm);
        pred.sync_from_replica(&replica, &mut world, 11);
        assert!(!pred.cancel_pending());
    }

    #[test]
    fn late_jump_invalid_after_grounded_changed_is_authoritative_correction() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        assert!(world.player_body().unwrap().grounded);
        let jump = PlayerInput::from_buttons_ext(false, false, true, false);
        tick_recorded(&mut pred, &mut world, jump, 2, 1);
        assert!(
            !world.player_body().unwrap().grounded
                || world.player_body().unwrap().velocity[1] > 0.0,
            "predicted jump was valid while grounded"
        );
        let mut collapsed = snap(2, 2, id, spawn, [0.0, -4.0]);
        collapsed.local_grounded = false;
        collapsed.local_grounded_on = PlatformSupportId::NONE;
        collapsed.last_acknowledged_input_sequence = 1;
        let _ = replica.apply(collapsed);
        pred.sync_from_replica(&replica, &mut world, 3);
        let body = world.player_body().unwrap();
        assert!(
            !body.grounded,
            "late-collapse ack of the jump is a legitimate correction"
        );
        assert!(body.velocity[1] < 0.0);
        assert_eq!(pred.pending_len(), 0);
        assert_eq!(pred.reset_count(), 1, "must not count as reconcile failure");
    }

    #[test]
    fn restore_and_replay_matches_local_prediction_for_held_move() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let right = PlayerInput::from_buttons_ext(false, true, false, false);
        for seq in 1..=8 {
            tick_recorded(&mut pred, &mut world, right, u64::from(seq) + 1, seq);
        }
        let predicted = world.player_body().unwrap().position;
        let mut lagged = snap(2, 2, id, spawn, [0.0, 0.0]);
        lagged.last_acknowledged_input_sequence = 0;
        let _ = replica.apply(lagged);
        pred.sync_from_replica(&replica, &mut world, 9);
        let replayed = world.player_body().unwrap().position;
        assert!((replayed[0] - predicted[0]).abs() < 1e-3);
        assert!((replayed[1] - predicted[1]).abs() < 1e-3);
        let d = pred.diagnostics(&world, &replica);
        assert!(
            d.last_correction_wu < 1e-3,
            "restore+replay correction is pre→post predicted, not aligned residual"
        );
        assert_eq!(d.total_reconciliation_count, 1);
        assert!(d.aligned_error.is_some());
    }

    #[test]
    fn observed_ack_delta_is_not_late_collapse() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        let mut first = snap(1, 1, id, spawn, [0.0, 0.0]);
        first.last_acknowledged_input_sequence = 1;
        let _ = replica.apply(first);
        pred.sync_from_replica(&replica, &mut world, 1);
        let mut second = snap(2, 2, id, spawn, [0.0, 0.0]);
        second.last_acknowledged_input_sequence = 5;
        let _ = replica.apply(second);
        pred.sync_from_replica(&replica, &mut world, 2);
        let d = pred.diagnostics(&world, &replica);
        assert_eq!(d.observed_ack_delta, 4);
        assert_eq!(d.observed_ack_jump_count, 1);
        assert_eq!(d.max_observed_ack_delta, 4);
    }

    #[test]
    fn header_only_frame_does_not_restore_stale_local_pose() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let right = PlayerInput::from_buttons_ext(false, true, false, false);
        for seq in 1..=8 {
            tick_recorded(&mut pred, &mut world, right, u64::from(seq) + 1, seq);
        }
        let predicted = world.player_body().unwrap().position;
        assert!(
            (predicted[0] - spawn[0]).abs() > 0.5,
            "setup must have walked away from spawn"
        );
        let header = ReplicationFrame {
            snapshot_sequence: 2,
            server_tick: 2,
            local_player_entity: id,
            input_epoch: 0,
            last_acknowledged_input_sequence: 0,
            local_grounded: true,
            local_grounded_on: PlatformSupportId(1),
            local_ignored_platform: PlatformSupportId::NONE,
            continuation_debt: 0,
            local_map: 1,
            local_channel: 0,
            local_instance: 0,
            observer_baseline_epoch: 0,
            records: vec![],
            aoi_debug: None,
        };
        assert!(matches!(
            replica.apply_frame(header),
            crate::replica::FrameDecision::Applied { .. }
        ));
        assert!(
            !replica.local_durable_updated(),
            "header-only must not claim a fresh local pose"
        );
        pred.sync_from_replica(&replica, &mut world, 9);
        let after = world.player_body().unwrap().position;
        assert!(
            (after[0] - predicted[0]).abs() < 1e-3,
            "stale replica pose must not rewind predicted x: {after:?} vs {predicted:?}"
        );
        assert!(
            (after[1] - predicted[1]).abs() < 1e-3,
            "stale replica pose must not rewind predicted y: {after:?} vs {predicted:?}"
        );
    }

    #[test]
    fn landed_prediction_is_not_rewound_to_falling_replica() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let jump = PlayerInput::from_buttons_ext(false, false, true, false);
        let idle = PlayerInput::idle();
        tick_recorded(&mut pred, &mut world, jump, 2, 1);
        for seq in 2..=24 {
            tick_recorded(&mut pred, &mut world, idle, u64::from(seq) + 1, seq);
        }
        let landed = world.player_body().unwrap();
        assert!(landed.grounded, "setup must have landed");
        let floor_y = landed.position[1];
        let mut falling = snap(2, 30, id, [spawn[0], spawn[1] + 1.5], [0.0, -8.0]);
        falling.local_grounded = false;
        falling.local_grounded_on = PlatformSupportId::NONE;
        falling.last_acknowledged_input_sequence = 0;
        let _ = replica.apply(falling);
        pred.sync_from_replica(&replica, &mut world, 25);
        let body = world.player_body().unwrap();
        assert!(
            body.grounded,
            "local landing must not be rewound to a lagged falling replica"
        );
        assert!(
            (body.position[1] - floor_y).abs() < 1e-3,
            "must stay on the floor, got {} want {floor_y}",
            body.position[1]
        );
    }

    #[test]
    fn idle_rest_does_not_restore_lagged_walk_velocity() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let right = PlayerInput::from_buttons_ext(false, true, false, false);
        let idle = PlayerInput::idle();
        for seq in 1..=12 {
            tick_recorded(&mut pred, &mut world, right, u64::from(seq) + 1, seq);
        }
        for seq in 13..=36 {
            tick_recorded(&mut pred, &mut world, idle, u64::from(seq) + 1, seq);
        }
        let settled = world.player_body().unwrap();
        assert!(settled.grounded);
        assert_eq!(settled.velocity[0], 0.0);
        let rest_x = settled.position[0];
        let tick_vx = pred.last_tick_velocity()[0];
        assert_eq!(tick_vx, 0.0);
        let mut lagged = snap(2, 40, id, [rest_x - 0.05, spawn[1]], [1.2, 0.0]);
        lagged.local_grounded = true;
        lagged.local_grounded_on = PlatformSupportId(1);
        lagged.last_acknowledged_input_sequence = 36;
        let _ = replica.apply(lagged);
        pred.sync_from_replica(&replica, &mut world, 37);
        let body = world.player_body().unwrap();
        assert_eq!(
            body.velocity[0], 0.0,
            "must not rewind idle vx from replica"
        );
        assert!(
            (body.position[0] - rest_x).abs() < 1e-4,
            "idle X must stay, got {} want {rest_x}",
            body.position[0]
        );
        assert_eq!(pred.last_tick_velocity()[0], 0.0);
        let rem = Duration::from_secs_f32(TICK_DURATION.as_secs_f32() * 0.8);
        let extra = crate::local_presentation::extrapolate_tick_pose(
            body.position,
            pred.last_tick_velocity(),
            rem,
        );
        assert_eq!(
            extra[0], body.position[0],
            "remainder extra must not sawtooth idle X"
        );
        let mut settled = snap(3, 50, id, [rest_x - 0.05, spawn[1]], [0.0, 0.0]);
        settled.local_grounded = true;
        settled.local_grounded_on = PlatformSupportId(1);
        settled.last_acknowledged_input_sequence = 36;
        let _ = replica.apply(settled);
        pred.sync_from_replica(&replica, &mut world, 38);
        let restored = world.player_body().unwrap();
        assert!(
            (restored.position[0] - (rest_x - 0.05)).abs() < 1e-3,
            "idle replica with vx=0 must still restore, got {}",
            restored.position[0]
        );
    }

    #[test]
    fn last_tick_velocity_drives_walk_remainder_not_replica() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let right = PlayerInput::from_buttons_ext(false, true, false, false);
        tick_recorded(&mut pred, &mut world, right, 2, 1);
        let vx = pred.last_tick_velocity()[0];
        assert!(
            vx > 1.0,
            "walking tick must leave remainder velocity, got {vx}"
        );
        let pose = world.player_body().unwrap().position;
        let rem = Duration::from_secs_f32(TICK_DURATION.as_secs_f32() * 0.5);
        let extra =
            crate::local_presentation::extrapolate_tick_pose(pose, pred.last_tick_velocity(), rem);
        assert!(
            extra[0] > pose[0] + 0.01,
            "walk remainder must still advance X"
        );
        assert_eq!(extra[1], pose[1]);
    }

    #[test]
    fn remainder_extra_never_invents_vertical_on_jump_fall_land() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let rem = Duration::from_secs_f32(TICK_DURATION.as_secs_f32() * 0.9);
        let jump = PlayerInput::from_buttons_ext(false, false, true, false);
        let idle = PlayerInput::idle();
        pred.tick(&mut world, jump, 2);
        let mut airborne = false;
        let mut landed = false;
        let mut min_y = f32::MAX;
        for t in 3..45u64 {
            pred.tick(&mut world, idle, t);
            let body = world.player_body().unwrap();
            let current = pred.tick_pose().unwrap_or(body.position);
            let composed = crate::local_presentation::compose_local_render_pose(
                body.position,
                pred.prev_tick_pose(),
                current,
                pred.last_tick_velocity(),
                rem,
            );
            if let (Some(prev), Some(cur)) = (pred.prev_tick_pose(), pred.tick_pose()) {
                let lo = prev[1].min(cur[1]);
                let hi = prev[1].max(cur[1]);
                assert!(
                    composed[1] + 1e-4 >= lo && composed[1] <= hi + 1e-4,
                    "rendered Y {} outside [{lo}, {hi}]",
                    composed[1]
                );
            }
            min_y = min_y.min(composed[1]);
            if composed[1] < spawn[1] - 0.001 {
                panic!(
                    "presented Y {:.4} dropped below spawn floor {:.4}",
                    composed[1], spawn[1]
                );
            }
            if !body.grounded {
                airborne = true;
            } else if airborne {
                landed = true;
                assert!(
                    (body.position[1] - spawn[1]).abs() < 0.05,
                    "must land on the Solid floor, got {} want {}",
                    body.position[1],
                    spawn[1]
                );
                break;
            }
        }
        assert!(airborne && landed, "jump must leave the ground and land");
        assert!(
            min_y + 0.002 >= spawn[1],
            "presented Y must never cross the contact floor"
        );
        let right = PlayerInput::from_buttons_ext(false, true, false, false);
        pred.tick(&mut world, right, 50);
        let walking = world.player_body().unwrap();
        let walk = crate::local_presentation::compose_local_render_pose(
            walking.position,
            pred.prev_tick_pose(),
            pred.tick_pose().unwrap_or(walking.position),
            pred.last_tick_velocity(),
            rem,
        );
        assert!(walk[0] > walking.position[0] + 0.01);
    }

    #[test]
    fn restore_snaps_y_interp_history() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let jump = PlayerInput::from_buttons_ext(false, false, true, false);
        pred.tick(&mut world, jump, 2);
        pred.tick(&mut world, PlayerInput::idle(), 3);
        let airborne = world.player_body().unwrap().position;
        assert!(pred.prev_tick_pose().is_some());
        assert_ne!(
            pred.prev_tick_pose().unwrap()[1],
            pred.tick_pose().unwrap()[1]
        );
        auth_at(&mut replica, 2, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 4);
        let prev = pred.prev_tick_pose().unwrap();
        let cur = pred.tick_pose().unwrap();
        assert_eq!(
            prev[1], cur[1],
            "hard/restore must not lerp from airborne Y"
        );
        assert!((cur[1] - spawn[1]).abs() < 0.05 || (cur[1] - airborne[1]).abs() < 0.05);
    }

    #[test]
    fn ordinary_snapshot_preserves_y_interp_history() {
        let mut world = World::footnote_test_stage();
        let mut replica = ReplicatedWorld::new();
        let mut pred = LocalPrediction::new();
        let id = wire(1, 1);
        let spawn = world.player_body().unwrap().position;
        auth_at(&mut replica, 1, id, spawn);
        pred.sync_from_replica(&replica, &mut world, 1);
        let jump = PlayerInput::from_buttons_ext(false, false, true, false);
        tick_recorded(&mut pred, &mut world, jump, 2, 1);
        tick_recorded(&mut pred, &mut world, PlayerInput::idle(), 3, 2);
        assert_ne!(
            pred.prev_tick_pose().unwrap()[1],
            pred.tick_pose().unwrap()[1]
        );
        let body = world.player_body().unwrap();
        let mut airborne = snap(2, 4, id, body.position, body.velocity);
        airborne.local_grounded = false;
        airborne.local_grounded_on = PlatformSupportId::NONE;
        airborne.last_acknowledged_input_sequence = 2;
        let _ = replica.apply(airborne);
        pred.sync_from_replica(&replica, &mut world, 4);
        let prev = pred.prev_tick_pose().unwrap();
        let cur = pred.tick_pose().unwrap();
        assert_ne!(
            prev[1], cur[1],
            "ordinary durable snapshot must keep tick-to-tick Y lerp (got prev={} cur={})",
            prev[1], cur[1]
        );
        assert!(!pred.last_sync_hard_snap());
    }
}
