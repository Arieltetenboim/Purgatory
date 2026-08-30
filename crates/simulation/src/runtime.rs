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
use crate::cadence::{Cadence, CadenceBinding};
use crate::effect::{EffectError, EffectId, EffectKind, TempEffect};
use crate::entity::EntityId;
use crate::query::{QueryFilter, QueryLimit};
use crate::runtime_event::RuntimeEvent;
use crate::runtime_stats::RuntimeStats;
use crate::scheduler::{DrainOutcome, FiredJob, ScheduleOwner, ScheduledKind, TimerId, WorkLane};
use crate::spawn::RuntimeSpawnRequest;
use crate::spawn_schedule::{ScheduledSpawn, SpawnRequestId};
use crate::time::SimulationTick;
use crate::world::World;
use purgatory_common::WorldAddress;

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
        self.refresh_runtime_gauges();
    }

    /// Critical scheduler drain + apply. Remainder past the overload ceiling
    /// carries forward; [`RuntimeStats::scheduler_critical_ceiling_hits`] counts hits.
    pub fn drain_critical_scheduler(&mut self) {
        let tick = self.tick;
        let outcome = self.scheduler.drain_due(tick, WorkLane::Critical);
        self.runtime_stats.scheduler_critical_fired =
            u32::try_from(outcome.fired.len()).unwrap_or(u32::MAX);
        if outcome.ceiling_hit {
            self.runtime_stats.scheduler_critical_ceiling_hits = self
                .runtime_stats
                .scheduler_critical_ceiling_hits
                .saturating_add(1);
        }
        self.apply_fired_jobs(outcome);
    }

    /// Deferred scheduler drain + apply. Budgeted FIFO with progress guarantee.
    pub fn drain_deferred_scheduler(&mut self) {
        let tick = self.tick;
        let outcome = self.scheduler.drain_due(tick, WorkLane::Deferred);
        self.runtime_stats.scheduler_deferred_fired =
            u32::try_from(outcome.fired.len()).unwrap_or(u32::MAX);
        if outcome.budget_exhausted {
            self.runtime_stats.scheduler_deferred_exhausted = self
                .runtime_stats
                .scheduler_deferred_exhausted
                .saturating_add(1);
        }
        self.apply_fired_jobs(outcome);
    }

    /// Commit staged runtime events once. Newly pushed events wait until next commit.
    pub fn commit_runtime_events(&mut self) -> Vec<RuntimeEvent> {
        self.events.commit()
    }

    pub fn pump_cadence(&mut self) {
        let tick = self.tick.get();
        let due = self.cadence.due_this_tick(tick);
        self.runtime_stats.cadence_due = u32::try_from(due.len()).unwrap_or(u32::MAX);
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
        match self.actions.start(owner, kind) {
            Ok(action) => {
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
        self.events.push(RuntimeEvent::EntitySpawned { id });
    }

    pub(crate) fn cleanup_owned_runtime(&mut self, id: EntityId) {
        self.scheduler.cancel_owner(id);
        if let Some(action) = self.actions.drop_owner(id) {
            self.events.push(RuntimeEvent::ActionEnded {
                id: action.id,
                owner: action.owner,
                end: ActionEnd::Cancelled,
            });
        }
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

    fn apply_fired_jobs(&mut self, outcome: DrainOutcome) {
        for job in outcome.fired {
            self.apply_fired_job(job);
        }
    }

    fn apply_fired_job(&mut self, job: FiredJob) {
        match job.kind {
            ScheduledKind::ExpireEffect(id) => {
                let Some(effect) = self.effects.remove(id) else {
                    return;
                };
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
            ScheduledKind::SpawnDue(id) => {
                let Some(scheduled) = self.spawn_schedule.take(id) else {
                    return;
                };
                let _ = self.spawn(scheduled.request);
            }
            ScheduledKind::DespawnEntity(entity) => {
                if self.contains(entity) {
                    let _ = self.despawn(entity);
                }
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
            actions_active: self.actions.active_count(),
            effects_active: self.effects.active_count(),
            events_produced: self.events.produced(),
            events_processed: self.events.processed(),
            spawn_queue_depth: self.spawn_schedule.queued_count(),
            cadence_due: self.runtime_stats.cadence_due,
            command_rejects_gate: self.runtime_stats.command_rejects_gate,
            command_rejects_other: self.runtime_stats.command_rejects_other,
            domain_rev_advances: self.runtime_stats.domain_rev_advances,
        }
    }
}
