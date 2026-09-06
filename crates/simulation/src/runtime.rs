//! World-owned runtime service orchestration (Phase 6F).
//!
//! Error containment:
//! - invalid command → typed reject
//! - stale runtime target → controlled no-op / cancel
//! - expired or cancelled timer → cannot fire twice
//! - missing owner → cleanup
//! - internal impossible invariant → debug_assert according to project rules

use crate::Aabb;
use crate::action::{Action, ActionEnd, ActionError, ActionKind};
use crate::action_gate::{ActionDenialReason, ActionGateContext, evaluate_action_gate};
use crate::body::CollisionBody;
use crate::cadence::{Cadence, CadenceBinding};
use crate::collision::{recover_solid_penetration, resolve_horizontal, resolve_vertical};
use crate::effect::{EffectError, EffectId, EffectKind, TempEffect};
use crate::entity::EntityId;
use crate::health::{DamageImmunityPolicy, Health};
use crate::npc::{
    ActionRejectReason, ActionRequest, CONTACT_DAMAGE, NPC_MOVE_SPEED, NPC_STOP_PERIOD_TICKS,
    NPC_TURN_PERIOD_TICKS, NPC_WALK_PERIOD_TICKS, NpcState, STRIKE_DAMAGE, STRIKE_DURATION_TICKS,
    STRIKE_RANGE,
};
use crate::platform::PlatformView;
use crate::query::{QueryFilter, QueryLimit};
use crate::runtime_event::RuntimeEvent;
use crate::runtime_stats::RuntimeStats;
use crate::scheduler::{DrainOutcome, FiredJob, ScheduleOwner, ScheduledKind, TimerId, WorkLane};
use crate::spawn::RuntimeSpawnRequest;
use crate::spawn_schedule::{ScheduledSpawn, SpawnRequestId};
use crate::time::SimulationTick;
use crate::transform::Transform;
use crate::world::World;
use purgatory_common::WorldAddress;

/// Instrumentation split for one scheduler drain + apply (not tick identity).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrainApplyTiming {
    pub scheduler_us: u64,
    pub actions_us: u64,
    pub effects_us: u64,
    pub lifecycle_us: u64,
    pub other_us: u64,
}

impl DrainApplyTiming {
    #[must_use]
    pub fn saturating_add(self, other: Self) -> Self {
        Self {
            scheduler_us: self.scheduler_us.saturating_add(other.scheduler_us),
            actions_us: self.actions_us.saturating_add(other.actions_us),
            effects_us: self.effects_us.saturating_add(other.effects_us),
            lifecycle_us: self.lifecycle_us.saturating_add(other.lifecycle_us),
            other_us: self.other_us.saturating_add(other.other_us),
        }
    }
}

fn micros(d: std::time::Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

impl World {
    #[must_use]
    pub fn simulation_tick(&self) -> SimulationTick {
        self.tick
    }

    #[must_use]
    pub fn runtime_stats(&self) -> RuntimeStats {
        self.snapshot_runtime_stats()
    }

    pub fn begin_tick(&mut self, tick: SimulationTick) {
        self.tick = tick;
        self.expire_presentation_oneshots();
        self.refresh_runtime_gauges();
    }

    /// Critical scheduler drain + apply. Remainder past the overload ceiling
    /// carries forward; [`RuntimeStats::scheduler_critical_ceiling_hits`] counts hits.
    ///
    /// Returned micros are **instrumentation Instant** splits (not tick identity).
    pub fn drain_critical_scheduler(&mut self) -> DrainApplyTiming {
        let tick = self.tick;
        let t0 = std::time::Instant::now();
        let outcome = self.scheduler.drain_due(tick, WorkLane::Critical);
        let scheduler_us = micros(t0.elapsed());
        self.runtime_stats.scheduler_critical_fired =
            u32::try_from(outcome.fired.len()).unwrap_or(u32::MAX);
        if outcome.ceiling_hit {
            self.runtime_stats.scheduler_critical_ceiling_hits = self
                .runtime_stats
                .scheduler_critical_ceiling_hits
                .saturating_add(1);
        }
        let mut apply = self.apply_fired_jobs_timed(outcome);
        apply.scheduler_us = apply.scheduler_us.saturating_add(scheduler_us);
        apply
    }

    /// Deferred scheduler drain + apply. Budgeted FIFO with progress guarantee.
    pub fn drain_deferred_scheduler(&mut self) -> DrainApplyTiming {
        let tick = self.tick;
        let t0 = std::time::Instant::now();
        let outcome = self.scheduler.drain_due(tick, WorkLane::Deferred);
        let scheduler_us = micros(t0.elapsed());
        self.runtime_stats.scheduler_deferred_fired =
            u32::try_from(outcome.fired.len()).unwrap_or(u32::MAX);
        if outcome.budget_exhausted {
            self.runtime_stats.scheduler_deferred_exhausted = self
                .runtime_stats
                .scheduler_deferred_exhausted
                .saturating_add(1);
        }
        let mut apply = self.apply_fired_jobs_timed(outcome);
        apply.scheduler_us = apply.scheduler_us.saturating_add(scheduler_us);
        apply
    }

    /// Commit staged runtime events once. Newly pushed events wait until next commit.
    pub fn commit_runtime_events(&mut self) -> Vec<RuntimeEvent> {
        self.events.commit()
    }

    pub fn pump_cadence(&mut self) {
        let tick = self.tick.get();
        let due = self.cadence.due_this_tick(tick);
        self.runtime_stats.cadence_due = u32::try_from(due.len()).unwrap_or(u32::MAX);
        self.runtime_stats.cadence_executions_total = self
            .runtime_stats
            .cadence_executions_total
            .saturating_add(u64::try_from(due.len()).unwrap_or(u64::MAX));
        for item in due {
            self.events.push(RuntimeEvent::CadenceFired {
                key: item.key.0,
                token: item.token,
            });
        }
    }

    pub fn register_cadence(&mut self, cadence: Cadence, token: u32) -> crate::cadence::CadenceKey {
        self.cadence.register(cadence, token)
    }

    #[must_use]
    pub fn cadence_due_bindings(&self) -> Vec<CadenceBinding> {
        self.cadence.due_this_tick(self.tick.get())
    }

    #[must_use]
    pub fn scheduled_spawn(
        &self,
        id: SpawnRequestId,
    ) -> Option<&crate::spawn_schedule::ScheduledSpawn> {
        self.spawn_schedule.get(id)
    }

    pub fn schedule_at(
        &mut self,
        due: SimulationTick,
        owner: ScheduleOwner,
        lane: WorkLane,
        kind: ScheduledKind,
    ) -> Option<TimerId> {
        self.scheduler.schedule_at(due, owner, lane, kind)
    }

    pub fn schedule_after(
        &mut self,
        ticks: u64,
        owner: ScheduleOwner,
        lane: WorkLane,
        kind: ScheduledKind,
    ) -> Option<TimerId> {
        let now = self.tick;
        self.scheduler.schedule_after(now, ticks, owner, lane, kind)
    }

    pub fn cancel_timer(&mut self, id: TimerId) -> bool {
        self.scheduler.cancel(id)
    }

    #[must_use]
    pub fn active_action(&self, owner: EntityId) -> Option<Action> {
        self.actions.active_of(owner)
    }

    /// Start a synthetic action after the gate. Denial does not allocate an Action slot.
    pub fn try_start_action(
        &mut self,
        owner: EntityId,
        kind: ActionKind,
        ctx: ActionGateContext,
    ) -> Result<Action, ActionDenialReason> {
        self.try_start_action_in_phase(owner, kind, crate::action::ActionPhase::Active, ctx)
    }

    pub fn try_start_action_in_phase(
        &mut self,
        owner: EntityId,
        kind: ActionKind,
        phase: crate::action::ActionPhase,
        ctx: ActionGateContext,
    ) -> Result<Action, ActionDenialReason> {
        if let Err(reason) = evaluate_action_gate(self, owner, ctx) {
            self.events
                .push(RuntimeEvent::ActionRejected { owner, reason });
            if matches!(
                reason,
                ActionDenialReason::TransitionLocked | ActionDenialReason::Busy
            ) {
                self.runtime_stats.command_rejects_gate =
                    self.runtime_stats.command_rejects_gate.saturating_add(1);
            } else {
                self.runtime_stats.command_rejects_other =
                    self.runtime_stats.command_rejects_other.saturating_add(1);
            }
            return Err(reason);
        }
        match if phase == crate::action::ActionPhase::Active {
            self.actions.start(owner, kind)
        } else {
            self.actions.start_in_phase(owner, kind, phase)
        } {
            Ok(action) => {
                self.runtime_stats.actions_started_total =
                    self.runtime_stats.actions_started_total.saturating_add(1);
                self.events.push(RuntimeEvent::ActionStarted {
                    id: action.id,
                    owner,
                });
                Ok(action)
            }
            Err(ActionError::Busy) => {
                let reason = ActionDenialReason::Busy;
                self.events
                    .push(RuntimeEvent::ActionRejected { owner, reason });
                self.runtime_stats.command_rejects_gate =
                    self.runtime_stats.command_rejects_gate.saturating_add(1);
                Err(reason)
            }
            Err(_) => {
                let reason = ActionDenialReason::MissingOwner;
                self.events
                    .push(RuntimeEvent::ActionRejected { owner, reason });
                self.runtime_stats.command_rejects_other =
                    self.runtime_stats.command_rejects_other.saturating_add(1);
                Err(reason)
            }
        }
    }

    pub fn end_action(
        &mut self,
        id: crate::action::ActionId,
        end: ActionEnd,
    ) -> Result<Action, ActionError> {
        let action = self.actions.end(id, end)?;
        self.ability_runtime.remove(id);
        if matches!(end, ActionEnd::Completed) {
            self.runtime_stats.actions_completed_total =
                self.runtime_stats.actions_completed_total.saturating_add(1);
        }
        self.events.push(RuntimeEvent::ActionEnded {
            id: action.id,
            owner: action.owner,
            end,
        });
        Ok(action)
    }

    pub fn apply_test_effect(
        &mut self,
        target: EntityId,
        duration_ticks: u64,
        kind: EffectKind,
        source: Option<EntityId>,
    ) -> Result<TempEffect, EffectError> {
        if !self.contains(target) {
            return Err(EffectError::TargetMissing);
        }
        let expire_at = self.tick.saturating_add_ticks(duration_ticks);
        let placeholder = TimerId::from_raw(0, 0);
        let id = self.effects.insert(TempEffect {
            id: EffectId::from_raw(0, 0),
            kind,
            owner: Some(target),
            source,
            target,
            expire_at,
            timer: placeholder,
        });
        let Some(timer) = self.scheduler.schedule_at(
            expire_at,
            ScheduleOwner::Entity(target),
            WorkLane::Critical,
            ScheduledKind::ExpireEffect(id),
        ) else {
            self.effects.remove(id);
            return Err(EffectError::TargetMissing);
        };
        let _ = self.effects.set_timer(id, timer);
        let effect = self.effects.get(id).expect("inserted effect");
        self.runtime_stats.effects_applied_total =
            self.runtime_stats.effects_applied_total.saturating_add(1);
        self.events.push(RuntimeEvent::EffectApplied { id, target });
        Ok(effect)
    }

    pub fn remove_effect(&mut self, id: EffectId) -> Option<TempEffect> {
        let effect = self.effects.remove(id)?;
        self.scheduler.cancel(effect.timer);
        self.events.push(RuntimeEvent::EffectRemoved {
            id: effect.id,
            target: effect.target,
        });
        Some(effect)
    }

    #[must_use]
    pub fn effect(&self, id: EffectId) -> Option<TempEffect> {
        self.effects.get(id)
    }

    pub fn schedule_spawn(
        &mut self,
        request: RuntimeSpawnRequest,
        due: SimulationTick,
        owner: ScheduleOwner,
        lane: WorkLane,
    ) -> Option<SpawnRequestId> {
        let placeholder = TimerId::from_raw(0, 0);
        let id = self.spawn_schedule.insert(ScheduledSpawn {
            id: SpawnRequestId::from_raw(0, 0),
            request,
            owner,
            due,
            timer: placeholder,
        });
        let Some(timer) = self
            .scheduler
            .schedule_at(due, owner, lane, ScheduledKind::SpawnDue(id))
        else {
            self.spawn_schedule.take(id);
            return None;
        };
        let _ = self.spawn_schedule.set_timer(id, timer);
        self.runtime_stats.spawn_requests_total =
            self.runtime_stats.spawn_requests_total.saturating_add(1);
        Some(id)
    }

    pub fn cancel_scheduled_spawn(&mut self, id: SpawnRequestId) -> bool {
        let Some(spawn) = self.spawn_schedule.take(id) else {
            return false;
        };
        self.scheduler.cancel(spawn.timer);
        true
    }

    pub fn schedule_despawn(
        &mut self,
        entity: EntityId,
        due: SimulationTick,
        lane: WorkLane,
    ) -> Option<TimerId> {
        if !self.contains(entity) {
            return None;
        }
        self.scheduler.schedule_at(
            due,
            ScheduleOwner::Entity(entity),
            lane,
            ScheduledKind::DespawnEntity(entity),
        )
    }

    /// Apply an ability effect. Ability code must not call [`Self::set_health`].
    pub fn execute_ability_effect(
        &mut self,
        _source: EntityId,
        target: EntityId,
        effect: crate::ability::AbilityEffect,
    ) -> bool {
        match effect {
            crate::ability::AbilityEffect::Damage { amount } => {
                self.apply_damage_with_immunity(target, amount, DamageImmunityPolicy::Bypass)
            }
        }
    }

    /// Server-authoritative ability request. Not a wire control. Not Strike.
    pub fn request_ability(
        &mut self,
        request: crate::ability::AbilityRequest<'_>,
        ctx: ActionGateContext,
    ) -> Result<Action, crate::ability::AbilityRejectReason> {
        use crate::ability::{AbilityActivation, AbilityLive, AbilityRejectReason};
        use crate::action::{ActionKind, ActionPhase};

        if request.definition.validate().is_err() {
            return Err(AbilityRejectReason::InvalidDefinition);
        }
        if !self.contains(request.actor) {
            return Err(AbilityRejectReason::MissingActor);
        }
        if self.ability_combatant_dead(request.actor) {
            return Err(AbilityRejectReason::ActorDead);
        }
        if !self
            .cooldowns
            .is_ready(request.actor, request.definition.id, self.tick)
        {
            return Err(AbilityRejectReason::OnCooldown);
        }

        match request.definition.activation {
            AbilityActivation::Independent => {}
            AbilityActivation::SelectedEntity => {
                let Some(selected) = request.selected else {
                    return Err(AbilityRejectReason::MissingTarget);
                };
                if !self.contains(selected) {
                    return Err(AbilityRejectReason::MissingTarget);
                }
                if self.ability_combatant_dead(selected) {
                    return Err(AbilityRejectReason::TargetDead);
                }
            }
        }

        let phase = request.definition.initial_phase();
        let action = match self.try_start_action_in_phase(
            request.actor,
            ActionKind::Ability {
                id: request.definition.id,
            },
            phase,
            ctx,
        ) {
            Ok(a) => a,
            Err(ActionDenialReason::Busy) => return Err(AbilityRejectReason::Busy),
            Err(reason) => return Err(AbilityRejectReason::Gate(reason)),
        };

        self.ability_runtime.insert(AbilityLive::from_definition(
            action.id,
            request.actor,
            request.selected,
            request.definition,
        ));

        if request.definition.timing.cooldown_ticks > 0 {
            let ready = self
                .tick
                .saturating_add_ticks(request.definition.timing.cooldown_ticks);
            self.cooldowns
                .set(request.actor, request.definition.id, ready);
        }

        match phase {
            ActionPhase::Windup => {
                self.schedule_ability_advance(
                    action.id,
                    request.actor,
                    request.definition.timing.windup_ticks,
                );
            }
            ActionPhase::Active => {
                self.apply_ability_effects(action.id);
                self.schedule_or_leave_active(action.id);
            }
            ActionPhase::Recovery => {
                self.schedule_ability_advance(
                    action.id,
                    request.actor,
                    request.definition.timing.recovery_ticks,
                );
            }
            _ => {}
        }
        // Ability execution → Attack (independent of hits / delivery shape / ability id).
        if let Some(kind) =
            crate::ability::oneshot_kind_for_cue(crate::ability::cue_for_ability_cast())
        {
            let _ = self.try_start_presentation_oneshot(request.actor, kind);
        }
        Ok(action)
    }

    #[must_use]
    pub fn ability_cooldown_ready(&self, owner: EntityId, id: crate::ability::AbilityId) -> bool {
        self.cooldowns.is_ready(owner, id, self.tick)
    }

    pub fn grant_ability(&mut self, owner: EntityId, id: crate::ability::AbilityId) -> bool {
        if !self.contains(owner) {
            return false;
        }
        self.ability_grants.insert(owner, id);
        true
    }

    pub fn revoke_ability(&mut self, owner: EntityId, id: crate::ability::AbilityId) {
        self.ability_grants.remove(owner, id);
    }

    #[must_use]
    pub fn ability_granted(&self, owner: EntityId, id: crate::ability::AbilityId) -> bool {
        self.ability_grants.contains(owner, id)
    }

    fn ability_combatant_dead(&self, id: EntityId) -> bool {
        self.health_of(id).is_some_and(|h| h.is_dead())
            || self.npc_of(id).is_some_and(|n| n.dead_pending)
    }

    fn apply_ability_effects(&mut self, action_id: crate::action::ActionId) {
        let (source, affected, effects) = {
            let Some(live) = self.ability_runtime.get_mut(action_id) else {
                return;
            };
            if live.effects_applied {
                return;
            }
            live.effects_applied = true;
            let source = live.owner;
            let snapshot = *live;
            let effects = live.effects().to_vec();
            (source, snapshot, effects)
        };
        let targets = self.resolve_ability_affected(&affected);
        for target in targets {
            for effect in &effects {
                let _ = self.execute_ability_effect(source, target, *effect);
            }
        }
    }

    fn resolve_ability_affected(&self, live: &crate::ability::AbilityLive) -> Vec<EntityId> {
        use crate::ability::{AbilityDelivery, forward_query_aabb};
        match live.delivery {
            AbilityDelivery::ForwardQuery {
                range,
                half_height,
                max_targets,
            } => {
                let Some(address) = self.address_of(live.owner) else {
                    return Vec::new();
                };
                let Some(origin) = self.transform_of(live.owner).map(|t| t.position) else {
                    return Vec::new();
                };
                let aabb = forward_query_aabb(
                    origin,
                    self.combat_facing_x(live.owner),
                    range,
                    half_height,
                );
                let mut hits: Vec<EntityId> = self
                    .query_aabb(address, aabb)
                    .into_iter()
                    .filter(|&id| self.is_ability_hit_target(live.owner, id))
                    .collect();
                hits.dedup();
                let cap = max_targets as usize;
                if hits.len() > cap {
                    hits.truncate(cap);
                }
                hits
            }
            AbilityDelivery::SelectedEntity => {
                let Some(selected) = live.selected else {
                    return Vec::new();
                };
                if self.is_ability_hit_target(live.owner, selected) {
                    vec![selected]
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn combat_facing_x(&self, id: EntityId) -> f32 {
        if let Some(player) = self.get_player(id) {
            return if player.1.facing_sign < 0 { -1.0 } else { 1.0 };
        }
        if let Some(npc) = self.npc_of(id)
            && npc.heading[0] < 0.0
        {
            return -1.0;
        }
        1.0
    }

    fn is_ability_hit_target(&self, owner: EntityId, id: EntityId) -> bool {
        if id == owner {
            return false;
        }
        if !self.contains(id) {
            return false;
        }
        if self.health_of(id).is_none() {
            return false;
        }
        !self.ability_combatant_dead(id)
    }

    fn schedule_ability_advance(
        &mut self,
        action_id: crate::action::ActionId,
        owner: EntityId,
        ticks: u64,
    ) {
        if ticks == 0 {
            self.on_ability_phase_due(action_id);
            return;
        }
        let due = self.tick.saturating_add_ticks(ticks);
        let _ = self.scheduler.schedule_at(
            due,
            ScheduleOwner::Entity(owner),
            WorkLane::Critical,
            ScheduledKind::AdvanceAction(action_id),
        );
    }

    fn schedule_or_leave_active(&mut self, action_id: crate::action::ActionId) {
        let (ticks, owner) = {
            let Some(live) = self.ability_runtime.get_mut(action_id) else {
                return;
            };
            (live.active_ticks, live.owner)
        };
        if ticks == 0 {
            self.enter_ability_recovery_or_complete(action_id);
        } else {
            self.schedule_ability_advance(action_id, owner, ticks);
        }
    }

    fn enter_ability_recovery_or_complete(&mut self, action_id: crate::action::ActionId) {
        let (recovery, owner) = {
            let Some(live) = self.ability_runtime.get_mut(action_id) else {
                return;
            };
            (live.recovery_ticks, live.owner)
        };
        if recovery == 0 {
            let _ = self.end_action(action_id, ActionEnd::Completed);
            return;
        }
        let _ = self
            .actions
            .set_phase(action_id, crate::action::ActionPhase::Recovery);
        self.schedule_ability_advance(action_id, owner, recovery);
    }

    fn on_ability_phase_due(&mut self, action_id: crate::action::ActionId) {
        use crate::action::ActionPhase;
        let Some(action) = self.actions.get(action_id) else {
            return;
        };
        match action.phase {
            ActionPhase::Windup => {
                let _ = self.actions.set_phase(action_id, ActionPhase::Active);
                self.apply_ability_effects(action_id);
                self.schedule_or_leave_active(action_id);
            }
            ActionPhase::Active => {
                self.apply_ability_effects(action_id);
                self.enter_ability_recovery_or_complete(action_id);
            }
            ActionPhase::Recovery => {
                let _ = self.end_action(action_id, ActionEnd::Completed);
            }
            _ => {}
        }
    }

    /// Apply flat damage via [`Self::set_health`]. Returns false if no Health.
    ///
    /// Presentation: non-lethal actual damage → Hurt oneshot; lethal → clear
    /// oneshots (Dead is Health-derived, not a oneshot). Already-dead targets
    /// do not restart Hurt.
    pub fn apply_damage(&mut self, target: EntityId, amount: f32) -> bool {
        self.apply_damage_with_immunity(target, amount, DamageImmunityPolicy::Respect)
    }

    /// Apply normal NPC contact damage through the victim immunity gate.
    pub fn apply_contact_damage(&mut self, target: EntityId, amount: f32) -> bool {
        self.apply_damage_with_immunity(target, amount, DamageImmunityPolicy::Respect)
    }

    /// Apply flat damage with an explicit victim-immunity policy.
    pub fn apply_damage_with_immunity(
        &mut self,
        target: EntityId,
        amount: f32,
        immunity: DamageImmunityPolicy,
    ) -> bool {
        let Some(mut health) = self.health_of(target) else {
            return false;
        };
        let before = health.current;
        if before <= 0.0 {
            // Already dead: Health write is a no-op presentation-wise.
            health.current = 0.0;
            return self.set_health(target, health);
        }
        if immunity == DamageImmunityPolicy::Respect && self.damage_immunity_active(target) {
            return false;
        }
        health.current = (health.current - amount).max(0.0);
        if !self.set_health(target, health) {
            return false;
        }
        if immunity == DamageImmunityPolicy::Respect && health.current < before {
            let until = self
                .tick
                .saturating_add_ticks(crate::npc::CONTACT_IMMUNITY_TICKS);
            if let Some(data) = self.slot_live_mut(target) {
                data.damage_immunity_until = Some(until);
            }
        }
        if health.current <= 0.0 {
            let _ = self.clear_presentation_oneshot(target);
            self.handle_zero_health(target);
        } else if health.current < before
            && let Some(kind) =
                crate::ability::oneshot_kind_for_cue(crate::ability::cue_for_damage_outcome(false))
        {
            let _ = self.try_start_presentation_oneshot(target, kind);
        }
        true
    }

    /// Server-authoritative action request (simulation-level; not wire control).
    pub fn request_action(
        &mut self,
        request: ActionRequest,
        ctx: ActionGateContext,
    ) -> Result<Action, ActionRejectReason> {
        self.runtime_stats.actions_attempted_total =
            self.runtime_stats.actions_attempted_total.saturating_add(1);
        let reject = |world: &mut World, reason: ActionRejectReason| {
            world.runtime_stats.actions_rejected_total =
                world.runtime_stats.actions_rejected_total.saturating_add(1);
            Err(reason)
        };
        if !self.contains(request.actor) {
            return reject(self, ActionRejectReason::MissingActor);
        }
        if !self.contains(request.target) {
            return reject(self, ActionRejectReason::MissingTarget);
        }
        if self
            .health_of(request.actor)
            .is_some_and(|h| h.current <= 0.0)
            || self.npc_of(request.actor).is_some_and(|n| n.dead_pending)
        {
            return reject(self, ActionRejectReason::ActorDead);
        }
        if self
            .health_of(request.target)
            .is_some_and(|h| h.current <= 0.0)
            || self.npc_of(request.target).is_some_and(|n| n.dead_pending)
        {
            return reject(self, ActionRejectReason::TargetDead);
        }
        if !matches!(request.kind, ActionKind::Strike | ActionKind::Test { .. }) {
            return reject(self, ActionRejectReason::UnsupportedKind);
        }
        if matches!(request.kind, ActionKind::Strike) {
            let Some(ap) = self.transform_of(request.actor).map(|t| t.position) else {
                return reject(self, ActionRejectReason::MissingActor);
            };
            let Some(tp) = self.transform_of(request.target).map(|t| t.position) else {
                return reject(self, ActionRejectReason::MissingTarget);
            };
            let dx = ap[0] - tp[0];
            let dy = ap[1] - tp[1];
            if dx * dx + dy * dy > STRIKE_RANGE * STRIKE_RANGE {
                return reject(self, ActionRejectReason::OutOfRange);
            }
        }
        let action = match self.try_start_action(request.actor, request.kind, ctx) {
            Ok(a) => a,
            Err(ActionDenialReason::Busy) => return reject(self, ActionRejectReason::Busy),
            Err(reason) => return reject(self, ActionRejectReason::Gate(reason)),
        };
        if matches!(request.kind, ActionKind::Strike) {
            let _ = self.apply_damage_with_immunity(
                request.target,
                STRIKE_DAMAGE,
                DamageImmunityPolicy::Bypass,
            );
            let due = self.tick.saturating_add_ticks(STRIKE_DURATION_TICKS);
            let _ = self.scheduler.schedule_at(
                due,
                ScheduleOwner::Entity(request.actor),
                WorkLane::Critical,
                ScheduledKind::CompleteAction(action.id),
            );
        }
        Ok(action)
    }

    /// Apply a Pulse effect with periodic damage ticks.
    pub fn apply_pulse_effect(
        &mut self,
        target: EntityId,
        duration_ticks: u64,
        period_ticks: u64,
        source: Option<EntityId>,
    ) -> Result<TempEffect, EffectError> {
        if !self.contains(target) {
            return Err(EffectError::TargetMissing);
        }
        let period = period_ticks.max(1);
        let duration = duration_ticks.max(period);
        let expire_at = self.tick.saturating_add_ticks(duration);
        let placeholder = TimerId::from_raw(0, 0);
        let id = self.effects.insert(TempEffect {
            id: EffectId::from_raw(0, 0),
            kind: EffectKind::Pulse {
                period_ticks: period,
            },
            owner: Some(target),
            source,
            target,
            expire_at,
            timer: placeholder,
        });
        let first = self.tick.saturating_add_ticks(period);
        let Some(timer) = self.scheduler.schedule_at(
            first,
            ScheduleOwner::Entity(target),
            WorkLane::Critical,
            ScheduledKind::PulseTick(id),
        ) else {
            self.effects.remove(id);
            return Err(EffectError::TargetMissing);
        };
        let _ = self.effects.set_timer(id, timer);
        let effect = self.effects.get(id).expect("inserted effect");
        self.runtime_stats.effects_applied_total =
            self.runtime_stats.effects_applied_total.saturating_add(1);
        self.events.push(RuntimeEvent::EffectApplied { id, target });
        Ok(effect)
    }

    /// Deterministic NPC activity and ground-physics step.
    pub fn tick_npcs(&mut self, dt_seconds: f32) {
        self.tick_npcs_with_approach(dt_seconds, None);
    }

    /// Deterministic NPC activity step with optional live-player approach.
    ///
    /// This remains the sole owner of authoritative NPC movement state. The
    /// Player acquisition is retained by the NPC while the target remains
    /// alive, in the same world address, and inside its home leash.
    pub fn tick_npcs_with_approach(&mut self, dt_seconds: f32, approach: Option<(f32, f32, f32)>) {
        let now = self.tick;
        let ids: Vec<EntityId> = self
            .iter()
            .filter(|&id| self.npc_of(id).is_some())
            .collect();
        let mut active = 0u32;
        for id in ids {
            let Some(mut npc) = self.npc_of(id) else {
                continue;
            };
            if !npc.active || npc.dead_pending || self.health_of(id).is_some_and(|h| h.is_dead()) {
                npc.target = None;
                npc.velocity = [0.0, 0.0];
                let _ = self.set_npc(id, npc);
                continue;
            }
            active = active.saturating_add(1);
            self.runtime_stats.npc_updates_total =
                self.runtime_stats.npc_updates_total.saturating_add(1);

            if let Some(target) = npc.target
                && !self.valid_npc_target(id, target, f32::MAX)
            {
                npc.target = None;
            }
            let approach_target =
                approach.and_then(|(acquisition_radius, stop_range, stop_half_height)| {
                    if let Some(target) = npc.target
                        && !self.valid_npc_target(
                            id,
                            target,
                            npc.hotspot_radius.max(acquisition_radius),
                        )
                    {
                        npc.target = None;
                    }
                    if npc.target.is_none() {
                        npc.target = self
                            .nearest_living_player_target(id, acquisition_radius)
                            .filter(|&target| {
                                self.valid_npc_target(
                                    id,
                                    target,
                                    npc.hotspot_radius.max(acquisition_radius),
                                )
                            });
                    }
                    npc.target.and_then(|target| {
                        let actor_position = self.transform_of(id)?.position;
                        let target_position = self.transform_of(target)?.position;
                        let dx = target_position[0] - actor_position[0];
                        let dy = target_position[1] - actor_position[1];
                        let facing_x = if dx < 0.0 { -1.0 } else { 1.0 };
                        let hittable = crate::ability::forward_query_aabb(
                            actor_position,
                            facing_x,
                            stop_range,
                            stop_half_height,
                        )
                        .contains_point(target_position);
                        Some((target_position, dx * dx + dy * dy, hittable))
                    })
                });
            if let Some((target_position, _distance_sq, hittable)) = approach_target {
                let Some(transform) = self.transform_of(id) else {
                    let _ = self.set_npc(id, npc);
                    continue;
                };
                if hittable {
                    npc.heading = if target_position[0] < transform.position[0] {
                        [-1.0, 0.0]
                    } else {
                        [1.0, 0.0]
                    };
                    npc.velocity[0] = 0.0;
                } else {
                    let dx = target_position[0] - transform.position[0];
                    let direction = dx.signum();
                    npc.heading = [direction, 0.0];
                    npc.velocity[0] = direction * NPC_MOVE_SPEED;
                }
            } else {
                if now.get() >= npc.next_turn_tick.get() {
                    npc.heading = if npc.advance_rng() & 1 == 0 {
                        [-1.0, 0.0]
                    } else {
                        [1.0, 0.0]
                    };
                    npc.next_turn_tick = now.saturating_add_ticks(NPC_TURN_PERIOD_TICKS);
                }
                if now.get() >= npc.next_mode_tick.get() {
                    npc.walking = !npc.walking;
                    let period = if npc.walking {
                        NPC_WALK_PERIOD_TICKS
                    } else {
                        NPC_STOP_PERIOD_TICKS
                    };
                    npc.next_mode_tick = now.saturating_add_ticks(period);
                }
                npc.velocity[0] = if npc.walking {
                    npc.heading[0] * NPC_MOVE_SPEED
                } else {
                    0.0
                };
                if npc.walking {
                    let Some(transform) = self.transform_of(id) else {
                        let _ = self.set_npc(id, npc);
                        continue;
                    };
                    let next_x = transform.position[0] + npc.velocity[0] * dt_seconds;
                    let mut min_x = npc.home[0] - npc.hotspot_radius;
                    let mut max_x = npc.home[0] + npc.hotspot_radius;
                    if npc.grounded
                        && let Some(support_id) = npc.grounded_on
                        && let Some(support) = self.iter_platforms().find(|view| {
                            view.id == support_id && self.address_of(view.id) == self.address_of(id)
                        })
                    {
                        min_x = min_x
                            .max(support.platform.min_x(support.transform) + npc.half_extents[0]);
                        max_x = max_x
                            .min(support.platform.max_x(support.transform) - npc.half_extents[0]);
                    }
                    if min_x > max_x {
                        npc.velocity[0] = 0.0;
                    } else if next_x < min_x {
                        npc.velocity[0] = 0.0;
                        npc.heading[0] = 1.0;
                    } else if next_x > max_x {
                        npc.velocity[0] = 0.0;
                        npc.heading[0] = -1.0;
                    }
                }
            }
            self.tick_npc_physics(id, &mut npc, dt_seconds);
            let _ = self.set_npc(id, npc);
        }
        self.runtime_stats.npcs_active = active;
        self.apply_npc_contact_damage();
    }

    /// Resolve NPC/player overlap as a gameplay query after all NPC physics.
    fn apply_npc_contact_damage(&mut self) {
        let ids: Vec<EntityId> = self
            .iter()
            .filter(|&id| self.npc_of(id).is_some())
            .collect();
        for id in ids {
            let Some(npc) = self.npc_of(id) else {
                continue;
            };
            if !npc.active || npc.dead_pending || self.health_of(id).is_some_and(|h| h.is_dead()) {
                continue;
            }
            let Some(target) = npc.target else {
                continue;
            };
            if !self.valid_npc_target(id, target, f32::MAX) {
                continue;
            }
            let (Some(npc_transform), Some((player_transform, player))) =
                (self.transform_of(id), self.get_player(target))
            else {
                continue;
            };
            if npc
                .aabb(npc_transform)
                .overlaps(player.aabb(*player_transform))
            {
                let _ = self.apply_contact_damage(target, CONTACT_DAMAGE);
            }
        }
    }

    fn tick_npc_physics(&mut self, id: EntityId, npc: &mut NpcState, dt_seconds: f32) {
        let Some(mut transform) = self.transform_of(id) else {
            return;
        };
        let previous = transform.position;
        let previous_bottom = previous[1] - npc.half_extents[1];
        let previous_top = previous[1] + npc.half_extents[1];
        let previous_left = previous[0] - npc.half_extents[0];
        let previous_right = previous[0] + npc.half_extents[0];
        let platforms: Vec<PlatformView> = self
            .iter_platforms()
            .filter(|view| self.address_of(view.id) == self.address_of(id))
            .collect();

        let _ = recover_solid_penetration(&mut transform, npc, platforms.iter().copied());
        if npc.grounded {
            let glued = crate::footnote::glue_to_support(
                &mut transform,
                npc,
                platforms.iter().copied(),
                previous_bottom,
            );
            if glued.is_none() {
                npc.grounded = false;
                npc.grounded_on = None;
            }
        }
        if !npc.grounded {
            npc.velocity[1] -= self.footnote_config().gravity * dt_seconds;
        }
        transform.position[0] += npc.velocity[0] * dt_seconds;
        let _ = resolve_horizontal(
            &mut transform,
            npc,
            platforms.iter().copied(),
            previous_bottom,
            previous_left,
            previous_right,
        );
        if !npc.grounded {
            transform.position[1] += npc.velocity[1] * dt_seconds;
            let (contact, _) = resolve_vertical(
                &mut transform,
                npc,
                platforms.iter().copied(),
                previous_bottom,
                previous_top,
            );
            if let Some(platform) = contact.landing() {
                npc.grounded = true;
                npc.grounded_on = Some(platform);
                npc.last_contact = crate::footnote::ContactEvent::Landed { platform };
            }
        }
        let _ = self.set_transform(id, transform);
    }

    /// Drive granted NPC abilities against the nearest living player.
    ///
    /// Acquisition belongs to the NPC driver. Movement and facing remain
    /// owned by `tick_npcs_with_approach`. Ability lifecycle, delivery,
    /// timing, cooldown, and effects remain owned by the ability runtime.
    /// Independent abilities intentionally receive no selected target.
    pub fn drive_npc_combat(
        &mut self,
        definition: &crate::ability::AbilityDefinition,
        acquisition_radius: f32,
    ) {
        let ids: Vec<EntityId> = self
            .iter()
            .filter(|&id| self.npc_of(id).is_some())
            .collect();
        for id in ids {
            let Some(npc) = self.npc_of(id) else {
                continue;
            };
            if !npc.active
                || npc.dead_pending
                || self.health_of(id).is_some_and(|h| h.is_dead())
                || !self.ability_granted(id, definition.id)
                || self.active_action(id).is_some()
                || !self.ability_cooldown_ready(id, definition.id)
            {
                continue;
            }
            let Some(mut npc) = self.npc_of(id) else {
                continue;
            };
            if let Some(target) = npc.target
                && !self.valid_npc_target(id, target, f32::MAX)
            {
                npc.target = None;
            }
            if npc.target.is_none() {
                npc.target = self
                    .nearest_living_player_target(id, acquisition_radius)
                    .filter(|&target| {
                        self.valid_npc_target(
                            id,
                            target,
                            npc.hotspot_radius.max(acquisition_radius),
                        )
                    });
            }
            let Some(_target) = npc.target else {
                continue;
            };
            let _ = self.set_npc(id, npc);
            let _ = self.request_ability(
                crate::ability::AbilityRequest {
                    actor: id,
                    selected: None,
                    definition,
                },
                crate::action_gate::ActionGateContext::in_world(),
            );
        }
    }

    fn valid_npc_target(&self, actor: EntityId, target: EntityId, leash_radius: f32) -> bool {
        let Some(address) = self.address_of(actor) else {
            return false;
        };
        let Some(npc) = self.npc_of(actor) else {
            return false;
        };
        if self.address_of(target) != Some(address)
            || self.kind(target) != Some(crate::entity::EntityKind::Player)
            || !self
                .health_of(target)
                .is_some_and(|health| health.is_alive())
        {
            return false;
        }
        let Some(target_position) = self.transform_of(target).map(|t| t.position) else {
            return false;
        };
        let dx = target_position[0] - npc.home[0];
        let dy = target_position[1] - npc.home[1];
        dx * dx + dy * dy <= leash_radius * leash_radius
    }

    /// Deterministic nearest living player in the actor's exact live address.
    #[must_use]
    pub fn nearest_living_player_target(&self, actor: EntityId, radius: f32) -> Option<EntityId> {
        let address = self.address_of(actor)?;
        let position = self.transform_of(actor)?.position;
        let mut candidates: Vec<(EntityId, f32)> = self
            .entities_near(address, position, radius)
            .filter(|&id| id != actor)
            .filter(|&id| self.kind(id) == Some(crate::entity::EntityKind::Player))
            .filter(|&id| self.health_of(id).is_some_and(|health| health.is_alive()))
            .filter_map(|id| {
                let target = self.transform_of(id)?.position;
                let dx = target[0] - position[0];
                let dy = target[1] - position[1];
                Some((id, dx * dx + dy * dy))
            })
            .collect();
        candidates.sort_by(|(left_id, left_distance), (right_id, right_distance)| {
            left_distance.total_cmp(right_distance).then_with(|| {
                (left_id.index(), left_id.generation())
                    .cmp(&(right_id.index(), right_id.generation()))
            })
        });
        candidates.first().map(|(id, _)| *id)
    }

    /// Nearest other Health-bearing entity within `radius`, tie-break by EntityId.
    #[must_use]
    pub fn nearest_health_target(&self, actor: EntityId, radius: f32) -> Option<EntityId> {
        let address = self.address_of(actor)?;
        let pos = self.transform_of(actor)?.position;
        let candidates =
            self.query_radius_filtered(address, pos, radius, QueryFilter::HasHealth, None);
        let mut best: Option<(EntityId, f32)> = None;
        for id in candidates {
            if id == actor {
                continue;
            }
            if self.kind(id) == Some(crate::entity::EntityKind::Player) {
                continue;
            }
            if self.health_of(id).is_some_and(|h| h.current <= 0.0) {
                continue;
            }
            if self.npc_of(id).is_some_and(|n| n.dead_pending) {
                continue;
            }
            let Some(tp) = self.transform_of(id).map(|t| t.position) else {
                continue;
            };
            let dx = tp[0] - pos[0];
            let dy = tp[1] - pos[1];
            let d2 = dx * dx + dy * dy;
            match best {
                None => best = Some((id, d2)),
                Some((bid, bd2)) => {
                    if d2 < bd2
                        || (d2 == bd2
                            && (id.index() < bid.index()
                                || (id.index() == bid.index()
                                    && id.generation() < bid.generation())))
                    {
                        best = Some((id, d2));
                    }
                }
            }
        }
        best.map(|(id, _)| id)
    }

    /// Build a visible NPC spawn request at `home`.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn npc_spawn_request(
        address: WorldAddress,
        home: [f32; 2],
        type_token: u32,
        hotspot_radius: f32,
        seed: u32,
        now: SimulationTick,
        active: bool,
        health_max: f32,
    ) -> RuntimeSpawnRequest {
        RuntimeSpawnRequest::transient_at(address)
            .with_transform(Transform::from_position(home))
            .visible()
            .with_health(Health::full(health_max))
            .with_npc(NpcState::new(
                type_token,
                home,
                hotspot_radius,
                seed,
                now,
                active,
            ))
    }

    fn handle_zero_health(&mut self, id: EntityId) {
        let Some(mut npc) = self.npc_of(id) else {
            return;
        };
        if npc.dead_pending {
            return;
        }
        npc.dead_pending = true;
        npc.active = false;
        npc.velocity = [0.0, 0.0];
        npc.walking = false;
        let home = npc.home;
        let radius = npc.hotspot_radius;
        let type_token = npc.type_token;
        let seed = npc.rng_state;
        let address = self.address_of(id).unwrap_or(WorldAddress::DEV);
        let _ = self.set_npc(id, npc);
        self.runtime_stats.deaths_total = self.runtime_stats.deaths_total.saturating_add(1);

        let delay = if self.npc_respawn_delay_ticks == 0 {
            30u64
        } else {
            self.npc_respawn_delay_ticks
        };
        let health_max = self
            .health_of(id)
            .map(|h| h.max)
            .filter(|m| *m > 0.0)
            .unwrap_or(crate::npc::NPC_HEALTH_MAX);
        let despawn_due = self.tick.saturating_add_ticks(delay);
        let _ = self.schedule_despawn(id, despawn_due, WorkLane::Deferred);
        let respawn_due = self.tick.saturating_add_ticks(delay.saturating_add(1));
        let req = Self::npc_spawn_request(
            address,
            home,
            type_token,
            radius,
            seed.wrapping_add(1),
            respawn_due,
            true,
            health_max,
        );
        if self
            .schedule_spawn(req, respawn_due, ScheduleOwner::World, WorkLane::Deferred)
            .is_some()
        {
            self.runtime_stats.respawns_total = self.runtime_stats.respawns_total.saturating_add(1);
        }
    }

    fn apply_pulse_tick(&mut self, id: EffectId) {
        let Some(effect) = self.effects.get(id) else {
            return;
        };
        let EffectKind::Pulse { period_ticks } = effect.kind else {
            return;
        };
        if !self.contains(effect.target) {
            let _ = self.effects.remove(id);
            return;
        }
        let _ = self.apply_damage_with_immunity(
            effect.target,
            crate::npc::PULSE_DAMAGE,
            DamageImmunityPolicy::Bypass,
        );
        self.runtime_stats.pulse_ticks_total =
            self.runtime_stats.pulse_ticks_total.saturating_add(1);
        let now = self.tick;
        if now.get() >= effect.expire_at.get() {
            let _ = self.effects.remove(id);
            self.runtime_stats.effects_expired_total =
                self.runtime_stats.effects_expired_total.saturating_add(1);
            self.events.push(RuntimeEvent::EffectExpired {
                id: effect.id,
                target: effect.target,
            });
            return;
        }
        let next = now.saturating_add_ticks(period_ticks.max(1));
        if let Some(timer) = self.scheduler.schedule_at(
            next.min(effect.expire_at),
            ScheduleOwner::Entity(effect.target),
            WorkLane::Critical,
            ScheduledKind::PulseTick(id),
        ) {
            let _ = self.effects.set_timer(id, timer);
        }
    }

    #[must_use]
    pub fn query_aabb_filtered(
        &self,
        address: WorldAddress,
        aabb: Aabb,
        filter: QueryFilter,
        limit: Option<QueryLimit>,
    ) -> Vec<EntityId> {
        let mut out: Vec<EntityId> = self
            .query_aabb(address, aabb)
            .into_iter()
            .filter(|&id| self.matches_query_filter(id, filter))
            .collect();
        if let Some(limit) = limit {
            out.truncate(limit.max);
        }
        out
    }

    #[must_use]
    pub fn query_radius_filtered(
        &self,
        address: WorldAddress,
        position: [f32; 2],
        radius: f32,
        filter: QueryFilter,
        limit: Option<QueryLimit>,
    ) -> Vec<EntityId> {
        let mut out: Vec<EntityId> = self
            .query_radius(address, position, radius)
            .into_iter()
            .filter(|&id| self.matches_query_filter(id, filter))
            .collect();
        if let Some(limit) = limit {
            out.truncate(limit.max);
        }
        out
    }

    pub(crate) fn note_entity_spawned(&mut self, id: EntityId) {
        self.runtime_stats.entities_spawned_total =
            self.runtime_stats.entities_spawned_total.saturating_add(1);
        self.events.push(RuntimeEvent::EntitySpawned { id });
    }

    pub(crate) fn cleanup_owned_runtime(&mut self, id: EntityId) {
        self.scheduler.cancel_owner(id);
        if let Some(action) = self.actions.drop_owner(id) {
            self.ability_runtime.remove(action.id);
            self.events.push(RuntimeEvent::ActionEnded {
                id: action.id,
                owner: action.owner,
                end: ActionEnd::Cancelled,
            });
        }
        self.ability_runtime.drop_owner(id);
        self.cooldowns.drop_owner(id);
        self.ability_grants.drop_owner(id);
        for effect in self.effects.drop_target(id) {
            self.scheduler.cancel(effect.timer);
            self.events.push(RuntimeEvent::EffectRemoved {
                id: effect.id,
                target: effect.target,
            });
        }
        for spawn in self.spawn_schedule.drop_owner(id) {
            self.scheduler.cancel(spawn.timer);
        }
        self.events.push(RuntimeEvent::EntityDespawned { id });
    }

    /// Clear runtime state that cannot survive a player restoration.
    ///
    /// Unlike [`Self::cleanup_owned_runtime`], this preserves the entity and
    /// its ability grants. A restored player starts with no live action,
    /// ability execution, cooldown, or target effect.
    pub(crate) fn clear_restoration_runtime(&mut self, id: EntityId) {
        self.scheduler.cancel_owner(id);
        if let Some(action) = self.actions.drop_owner(id) {
            self.ability_runtime.remove(action.id);
            self.events.push(RuntimeEvent::ActionEnded {
                id: action.id,
                owner: action.owner,
                end: ActionEnd::Cancelled,
            });
        }
        self.ability_runtime.drop_owner(id);
        self.cooldowns.drop_owner(id);
        for effect in self.effects.drop_target(id) {
            self.scheduler.cancel(effect.timer);
            self.events.push(RuntimeEvent::EffectRemoved {
                id: effect.id,
                target: effect.target,
            });
        }
    }

    pub(crate) fn note_domain_rev(&mut self) {
        self.runtime_stats.domain_rev_advances =
            self.runtime_stats.domain_rev_advances.saturating_add(1);
    }

    fn matches_query_filter(&self, id: EntityId, filter: QueryFilter) -> bool {
        match filter {
            QueryFilter::Any => true,
            QueryFilter::Kind(kind) => self.kind(id) == Some(kind),
            QueryFilter::HasHealth => self.health_of(id).is_some(),
            QueryFilter::HasInteractable => self.interactable_of(id).is_some(),
        }
    }

    fn apply_fired_jobs_timed(&mut self, outcome: DrainOutcome) -> DrainApplyTiming {
        let mut timing = DrainApplyTiming::default();
        for job in outcome.fired {
            let t0 = std::time::Instant::now();
            let kind = job.kind;
            self.apply_fired_job(job);
            let us = micros(t0.elapsed());
            match kind {
                ScheduledKind::CompleteAction(_) | ScheduledKind::AdvanceAction(_) => {
                    timing.actions_us = timing.actions_us.saturating_add(us);
                }
                ScheduledKind::ExpireEffect(_) => {
                    timing.effects_us = timing.effects_us.saturating_add(us);
                }
                ScheduledKind::SpawnDue(_) | ScheduledKind::DespawnEntity(_) => {
                    timing.lifecycle_us = timing.lifecycle_us.saturating_add(us);
                }
                ScheduledKind::PulseTick(_) => {
                    timing.effects_us = timing.effects_us.saturating_add(us);
                }
                ScheduledKind::TestProbe { .. } | ScheduledKind::RaiseEvent { .. } => {
                    timing.other_us = timing.other_us.saturating_add(us);
                }
            }
        }
        timing
    }

    fn apply_fired_job(&mut self, job: FiredJob) {
        match job.kind {
            ScheduledKind::ExpireEffect(id) => {
                let Some(effect) = self.effects.remove(id) else {
                    return;
                };
                self.runtime_stats.effects_expired_total =
                    self.runtime_stats.effects_expired_total.saturating_add(1);
                if !self.contains(effect.target) {
                    return;
                }
                self.events.push(RuntimeEvent::EffectExpired {
                    id: effect.id,
                    target: effect.target,
                });
            }
            ScheduledKind::CompleteAction(id) => {
                let _ = self.end_action(id, ActionEnd::Completed);
            }
            ScheduledKind::AdvanceAction(id) => {
                self.on_ability_phase_due(id);
            }
            ScheduledKind::SpawnDue(id) => {
                let Some(scheduled) = self.spawn_schedule.take(id) else {
                    return;
                };
                if self.spawn(scheduled.request).is_some() {
                    self.runtime_stats.spawns_completed_total =
                        self.runtime_stats.spawns_completed_total.saturating_add(1);
                }
            }
            ScheduledKind::DespawnEntity(entity) => {
                if self.contains(entity) {
                    let _ = self.despawn(entity);
                    self.runtime_stats.despawns_completed_total = self
                        .runtime_stats
                        .despawns_completed_total
                        .saturating_add(1);
                }
            }
            ScheduledKind::PulseTick(id) => {
                self.apply_pulse_tick(id);
            }
            ScheduledKind::TestProbe { token } | ScheduledKind::RaiseEvent { token } => {
                self.events.push(RuntimeEvent::ScheduledFired {
                    timer: job.id,
                    token,
                });
            }
        }
    }

    fn refresh_runtime_gauges(&mut self) {
        let stats = self.snapshot_runtime_stats();
        self.runtime_stats.scheduler_queued = stats.scheduler_queued;
        self.runtime_stats.scheduler_due_critical = stats.scheduler_due_critical;
        self.runtime_stats.scheduler_due_deferred = stats.scheduler_due_deferred;
        self.runtime_stats.actions_active = stats.actions_active;
        self.runtime_stats.effects_active = stats.effects_active;
        self.runtime_stats.events_produced = stats.events_produced;
        self.runtime_stats.events_processed = stats.events_processed;
        self.runtime_stats.spawn_queue_depth = stats.spawn_queue_depth;
        self.runtime_stats.scheduler_critical_ceiling_hits = stats.scheduler_critical_ceiling_hits;
        self.runtime_stats.scheduler_deferred_exhausted = stats.scheduler_deferred_exhausted;
    }

    fn snapshot_runtime_stats(&self) -> RuntimeStats {
        let tick = self.tick;
        RuntimeStats {
            scheduler_queued: self.scheduler.live_count(),
            scheduler_due_critical: self.scheduler.due_count(tick, WorkLane::Critical),
            scheduler_due_deferred: self.scheduler.due_count(tick, WorkLane::Deferred),
            scheduler_critical_fired: self.runtime_stats.scheduler_critical_fired,
            scheduler_deferred_fired: self.runtime_stats.scheduler_deferred_fired,
            scheduler_critical_ceiling_hits: self.scheduler.critical_ceiling_hits(),
            scheduler_deferred_exhausted: self.scheduler.deferred_exhausted(),
            scheduler_scheduled_total: self.scheduler.scheduled_total(),
            scheduler_cancelled_total: self.scheduler.cancelled_total(),
            scheduler_critical_executed_total: self.scheduler.critical_executed_total(),
            scheduler_deferred_executed_total: self.scheduler.deferred_executed_total(),
            actions_active: self.actions.active_count(),
            actions_started_total: self.runtime_stats.actions_started_total,
            actions_completed_total: self.runtime_stats.actions_completed_total,
            actions_attempted_total: self.runtime_stats.actions_attempted_total,
            actions_rejected_total: self.runtime_stats.actions_rejected_total,
            effects_active: self.effects.active_count(),
            effects_applied_total: self.runtime_stats.effects_applied_total,
            effects_expired_total: self.runtime_stats.effects_expired_total,
            events_produced: self.events.produced(),
            events_processed: self.events.processed(),
            spawn_queue_depth: self.spawn_schedule.queued_count(),
            spawn_requests_total: self.runtime_stats.spawn_requests_total,
            spawns_completed_total: self.runtime_stats.spawns_completed_total,
            despawns_completed_total: self.runtime_stats.despawns_completed_total,
            cadence_due: self.runtime_stats.cadence_due,
            cadence_executions_total: self.runtime_stats.cadence_executions_total,
            entities_spawned_total: self.runtime_stats.entities_spawned_total,
            command_rejects_gate: self.runtime_stats.command_rejects_gate,
            command_rejects_other: self.runtime_stats.command_rejects_other,
            domain_rev_advances: self.runtime_stats.domain_rev_advances,
            npcs_active: self.runtime_stats.npcs_active,
            npc_updates_total: self.runtime_stats.npc_updates_total,
            health_mutations_total: self.runtime_stats.health_mutations_total,
            deaths_total: self.runtime_stats.deaths_total,
            respawns_total: self.runtime_stats.respawns_total,
            pulse_ticks_total: self.runtime_stats.pulse_ticks_total,
        }
    }
}
