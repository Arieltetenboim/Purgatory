//! Load/test synthetic pressure. Test infrastructure only.
//!
//! Applied from [`GameplayOwner`] when load-mode is on and
//! `PURGATORY_LOAD_VALIDATION` is active. Production `World::tick` does not
//! read this config. Synthetic kinds stay on this path.

use purgatory_common::{LoadValidationConfig, WorldAddress};
use purgatory_simulation::{
    ActionGateContext, ActionKind, Cadence, EffectKind, EntityId, FOOTNOTE_SPAWN_X,
    RuntimeSpawnRequest, ScheduleOwner, ScheduledKind, SimulationTick, Transform, WorkLane, World,
};

/// Safety clamps so a malformed JSON count cannot allocate without bound.
/// Not gameplay quality thresholds.
const MAX_SYNTHETIC_ENTITIES: u32 = 2048;
const MAX_SCHEDULER_BATCH: u32 = 512;
const MAX_SPAWN_CHURN: u32 = 256;
const MAX_OWNERS: u32 = 64;
const MAX_CADENCE: u32 = 256;
const DEFAULT_SPAWN_INTERVAL: u64 = 30;
const REFILL_PERIOD: u64 = 30;

/// Per-process load-validation driver. Inactive when the config is empty.
pub struct LoadPressure {
    cfg: LoadValidationConfig,
    armed: bool,
    logged: bool,
    synthetics: Vec<EntityId>,
    action_owners: Vec<EntityId>,
    effect_targets: Vec<EntityId>,
    spawn_queue_seeded: bool,
}

impl LoadPressure {
    #[must_use]
    pub fn inactive() -> Self {
        Self::from_config(LoadValidationConfig::default())
    }

    #[must_use]
    pub fn from_config(cfg: LoadValidationConfig) -> Self {
        Self {
            cfg,
            armed: false,
            logged: false,
            synthetics: Vec::new(),
            action_owners: Vec::new(),
            effect_targets: Vec::new(),
            spawn_queue_seeded: false,
        }
    }

    /// Parse process env. Load-mode (`PURGATORY_ADMISSION_CAP`) is required.
    #[must_use]
    pub fn from_process_env() -> Self {
        match LoadValidationConfig::from_process_env() {
            Ok(cfg) => Self::from_config(cfg),
            Err(err) => {
                eprintln!("PURGATORY {err}; load-validation ignored");
                Self::inactive()
            }
        }
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.cfg.is_active()
    }

    /// Arm once, then cheaply refill. No per-tick logging.
    pub fn maintain(&mut self, world: &mut World, address: WorldAddress, tick: SimulationTick) {
        if !self.cfg.is_active() {
            return;
        }
        if !self.armed {
            self.arm(world, address, tick);
            return;
        }
        let n = tick.get();
        if n > 0 && n.is_multiple_of(REFILL_PERIOD) {
            self.refill(world, address, tick);
        }
    }

    fn arm(&mut self, world: &mut World, address: WorldAddress, tick: SimulationTick) {
        self.armed = true;
        self.spawn_synthetics(world, address);
        self.arm_scheduler(world, tick);
        self.arm_cadence(world);
        self.arm_actions(world, address, tick);
        self.arm_effects(world, address);
        self.arm_events(world, tick);
        self.seed_spawn_queue(world, address, tick);
        self.arm_spawn_churn(world, address, tick);
        if !self.logged {
            self.logged = true;
            println!(
                "PURGATORY load-validation armed synthetic={} sched_c={} sched_d={} spawn={} actions={} effects={} events={} cadence={}",
                self.cfg.synthetic_entities,
                self.cfg.scheduler.critical,
                self.cfg.scheduler.deferred,
                self.cfg.spawn_despawn.count,
                self.cfg.actions,
                self.cfg.effects,
                self.cfg.events,
                self.cfg.cadence_consumers
            );
        }
    }

    fn refill(&mut self, world: &mut World, address: WorldAddress, tick: SimulationTick) {
        self.arm_scheduler(world, tick);
        self.arm_events(world, tick);
        self.arm_spawn_churn(world, address, tick);
        self.rearm_actions(world, tick);
        self.rearm_effects(world);
        self.cancel_churn(world, tick);
    }

    fn spawn_synthetics(&mut self, world: &mut World, address: WorldAddress) {
        let n = self.cfg.synthetic_entities.min(MAX_SYNTHETIC_ENTITIES);
        self.synthetics.reserve(n as usize);
        for i in 0..n {
            let x = FOOTNOTE_SPAWN_X + (i as f32) * 1.5;
            let Some(id) = spawn_visible(world, address, x) else {
                break;
            };
            self.synthetics.push(id);
        }
    }

    fn arm_scheduler(&mut self, world: &mut World, tick: SimulationTick) {
        let critical = self.cfg.scheduler.critical.min(MAX_SCHEDULER_BATCH);
        for i in 0..critical {
            let due = tick.saturating_add_ticks(u64::from(i % 8));
            let _ = world.schedule_at(
                due,
                ScheduleOwner::World,
                WorkLane::Critical,
                ScheduledKind::TestProbe { token: i },
            );
        }
        let deferred = self.cfg.scheduler.deferred.min(MAX_SCHEDULER_BATCH);
        for i in 0..deferred {
            let due = tick.saturating_add_ticks(u64::from(i % 16));
            let _ = world.schedule_at(
                due,
                ScheduleOwner::World,
                WorkLane::Deferred,
                ScheduledKind::TestProbe { token: 1000 + i },
            );
        }
        self.cancel_churn(world, tick);
    }

    fn cancel_churn(&mut self, world: &mut World, tick: SimulationTick) {
        let n = self.cfg.scheduler.cancel_churn.min(MAX_SCHEDULER_BATCH);
        for i in 0..n {
            let Some(id) = world.schedule_at(
                tick.saturating_add_ticks(8),
                ScheduleOwner::World,
                WorkLane::Deferred,
                ScheduledKind::TestProbe { token: 2000 + i },
            ) else {
                break;
            };
            if i % 2 == 0 {
                let _ = world.cancel_timer(id);
            }
        }
    }

    fn arm_cadence(&mut self, world: &mut World) {
        let n = self.cfg.cadence_consumers.min(MAX_CADENCE);
        for i in 0..n {
            let _ = world.register_cadence(Cadence::EveryN { n: 4 }, i);
        }
    }

    fn arm_actions(&mut self, world: &mut World, address: WorldAddress, tick: SimulationTick) {
        let n = self.cfg.actions.min(MAX_OWNERS);
        for i in 0..n {
            let owner = self
                .synthetics
                .get(i as usize)
                .copied()
                .or_else(|| spawn_visible(world, address, FOOTNOTE_SPAWN_X + 40.0 + i as f32));
            let Some(owner) = owner else {
                break;
            };
            self.action_owners.push(owner);
            let _ = world.try_start_action(
                owner,
                ActionKind::Test { token: i },
                ActionGateContext::in_world(),
            );
            if let Some(action) = world.active_action(owner) {
                let due = tick.saturating_add_ticks(12);
                let _ = world.schedule_at(
                    due,
                    ScheduleOwner::Entity(owner),
                    WorkLane::Critical,
                    ScheduledKind::CompleteAction(action.id),
                );
            }
        }
    }

    fn rearm_actions(&mut self, world: &mut World, tick: SimulationTick) {
        for (i, owner) in self.action_owners.clone().into_iter().enumerate() {
            if !world.contains(owner) || world.active_action(owner).is_some() {
                continue;
            }
            let token = u32::try_from(i).unwrap_or(0);
            if world
                .try_start_action(
                    owner,
                    ActionKind::Test { token },
                    ActionGateContext::in_world(),
                )
                .is_ok()
                && let Some(action) = world.active_action(owner)
            {
                let _ = world.schedule_at(
                    tick.saturating_add_ticks(12),
                    ScheduleOwner::Entity(owner),
                    WorkLane::Critical,
                    ScheduledKind::CompleteAction(action.id),
                );
            }
        }
    }

    fn arm_effects(&mut self, world: &mut World, address: WorldAddress) {
        let n = self.cfg.effects.min(MAX_OWNERS);
        for i in 0..n {
            let target = self
                .synthetics
                .get(i as usize)
                .copied()
                .or_else(|| spawn_visible(world, address, FOOTNOTE_SPAWN_X + 48.0 + i as f32));
            let Some(target) = target else {
                break;
            };
            self.effect_targets.push(target);
            let _ = world.apply_test_effect(target, 20, EffectKind::Test { token: i }, None);
        }
    }

    fn rearm_effects(&mut self, world: &mut World) {
        for (i, target) in self.effect_targets.iter().copied().enumerate() {
            if !world.contains(target) {
                continue;
            }
            let token = u32::try_from(i).unwrap_or(0);
            let _ = world.apply_test_effect(target, 20, EffectKind::Test { token }, None);
        }
    }

    fn arm_events(&mut self, world: &mut World, tick: SimulationTick) {
        let n = self.cfg.events.min(MAX_SCHEDULER_BATCH);
        for i in 0..n {
            let due = tick.saturating_add_ticks(u64::from(i % 6));
            let _ = world.schedule_at(
                due,
                ScheduleOwner::World,
                WorkLane::Deferred,
                ScheduledKind::RaiseEvent { token: 3000 + i },
            );
        }
    }

    fn seed_spawn_queue(&mut self, world: &mut World, address: WorldAddress, tick: SimulationTick) {
        if self.spawn_queue_seeded {
            return;
        }
        self.spawn_queue_seeded = true;
        let n = self.cfg.spawn_despawn.count.min(MAX_SPAWN_CHURN);
        for i in 0..n {
            let x = FOOTNOTE_SPAWN_X + 12.0 + (i as f32) * 0.5;
            let req = RuntimeSpawnRequest::transient_at(address)
                .with_transform(Transform::from_position([x, 2.0]))
                .visible();
            let due = tick.saturating_add_ticks(2 + u64::from(i % 4));
            let _ = world.schedule_spawn(req, due, ScheduleOwner::World, WorkLane::Deferred);
        }
    }

    fn arm_spawn_churn(&mut self, world: &mut World, address: WorldAddress, tick: SimulationTick) {
        let n = self.cfg.spawn_despawn.count.min(MAX_SPAWN_CHURN);
        if n == 0 {
            return;
        }
        let interval = if self.cfg.spawn_despawn.interval_ticks == 0 {
            DEFAULT_SPAWN_INTERVAL
        } else {
            u64::from(self.cfg.spawn_despawn.interval_ticks)
        };
        for i in 0..n {
            let x = FOOTNOTE_SPAWN_X + 8.0 + (i as f32) * 0.8;
            let Some(entity) = spawn_visible(world, address, x) else {
                break;
            };
            let _ = world.schedule_despawn(
                entity,
                tick.saturating_add_ticks(interval),
                WorkLane::Deferred,
            );
        }
    }
}

fn spawn_visible(world: &mut World, address: WorldAddress, x: f32) -> Option<EntityId> {
    world.spawn(
        RuntimeSpawnRequest::transient_at(address)
            .with_transform(Transform::from_position([x, 1.0]))
            .visible(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_common::{SchedulerPressure, SpawnPressure};

    fn tick_world(world: &mut World, n: u64) {
        world.begin_tick(SimulationTick::from_count(n));
        world.drain_critical_scheduler();
        let _ = world.commit_runtime_events();
        world.pump_cadence();
        world.drain_deferred_scheduler();
    }

    #[test]
    fn inactive_does_not_spawn_or_schedule() {
        let mut world = World::new();
        let before = world.len();
        let mut pressure = LoadPressure::inactive();
        pressure.maintain(&mut world, WorldAddress::DEV, SimulationTick::from_count(1));
        tick_world(&mut world, 1);
        assert_eq!(world.len(), before);
        assert_eq!(world.runtime_stats().scheduler_queued, 0);
    }

    #[test]
    fn active_spawns_generics_and_queues_work() {
        let mut world = World::new();
        let before = world.len();
        let cfg = LoadValidationConfig {
            synthetic_entities: 8,
            scheduler: SchedulerPressure {
                critical: 4,
                deferred: 4,
                cancel_churn: 4,
            },
            spawn_despawn: SpawnPressure {
                count: 2,
                interval_ticks: 8,
            },
            actions: 1,
            effects: 1,
            events: 3,
            cadence_consumers: 4,
        };
        let mut pressure = LoadPressure::from_config(cfg);
        pressure.maintain(&mut world, WorldAddress::DEV, SimulationTick::from_count(1));
        tick_world(&mut world, 1);
        assert!(world.len() > before, "synthetic generics must exist");
        let stats = world.runtime_stats();
        assert!(
            stats.scheduler_queued > 0 || stats.actions_active > 0 || stats.effects_active > 0,
            "runtime work must be queued"
        );
        assert!(stats.scheduler_scheduled_total > 0);
        assert!(stats.entities_spawned_total >= 8);
        assert!(pressure.synthetics.len() >= 8);
    }

    fn mixed_like_config() -> LoadValidationConfig {
        LoadValidationConfig {
            synthetic_entities: 64,
            scheduler: SchedulerPressure {
                critical: 32,
                deferred: 48,
                cancel_churn: 8,
            },
            spawn_despawn: SpawnPressure {
                count: 16,
                interval_ticks: 60,
            },
            actions: 4,
            effects: 8,
            events: 16,
            cadence_consumers: 32,
        }
    }

    fn server_like_tick(pressure: &mut LoadPressure, world: &mut World, tick: u64) {
        let t = SimulationTick::from_count(tick);
        world.begin_tick(t);
        pressure.maintain(world, WorldAddress::DEV, t);
        world.drain_critical_scheduler();
        let _ = world.commit_runtime_events();
        world.pump_cadence();
        world.drain_deferred_scheduler();
    }

    #[test]
    fn mixed_validation_executes_every_configured_workload() {
        let mut world = World::new();
        let mut pressure = LoadPressure::from_config(mixed_like_config());
        let mut sampled_queued_max = 0u32;
        let mut sampled_actions_max = 0u32;
        let mut sampled_spawn_max = 0u32;
        let mut actions_at_tick_29 = None;
        for t in 1..=90 {
            server_like_tick(&mut pressure, &mut world, t);
            let stats = world.runtime_stats();
            if t == 29 {
                actions_at_tick_29 = Some(stats.actions_active);
            }
            if t.is_multiple_of(30) {
                sampled_queued_max = sampled_queued_max.max(stats.scheduler_queued);
                sampled_actions_max = sampled_actions_max.max(stats.actions_active);
                sampled_spawn_max = sampled_spawn_max.max(stats.spawn_queue_depth);
            }
        }
        let stats = world.runtime_stats();
        assert!(
            stats.entities_spawned_total >= 64,
            "synthetic entities must spawn (got {})",
            stats.entities_spawned_total
        );
        assert!(
            stats.scheduler_scheduled_total >= 32 + 48,
            "scheduler arm must schedule critical+deferred (got {})",
            stats.scheduler_scheduled_total
        );
        assert!(
            stats.scheduler_critical_executed_total >= 32,
            "critical TestProbe/CompleteAction/ExpireEffect must fire (got {})",
            stats.scheduler_critical_executed_total
        );
        assert!(
            stats.scheduler_deferred_executed_total >= 48,
            "deferred TestProbe/RaiseEvent/spawn/despawn must fire (got {})",
            stats.scheduler_deferred_executed_total
        );
        assert!(
            stats.scheduler_cancelled_total >= 4,
            "cancel churn must cancel (got {})",
            stats.scheduler_cancelled_total
        );
        assert!(
            stats.actions_started_total >= 4,
            "validation actions must start (got {})",
            stats.actions_started_total
        );
        assert!(
            stats.actions_completed_total >= 4,
            "validation actions must complete (got {})",
            stats.actions_completed_total
        );
        assert!(
            stats.effects_applied_total >= 8,
            "validation effects must apply (got {})",
            stats.effects_applied_total
        );
        assert!(
            stats.effects_expired_total >= 8,
            "validation effects must expire (got {})",
            stats.effects_expired_total
        );
        assert!(
            stats.spawn_requests_total >= 16,
            "seed spawn queue must request (got {})",
            stats.spawn_requests_total
        );
        assert!(
            stats.spawns_completed_total >= 16,
            "seed spawn queue must complete (got {})",
            stats.spawns_completed_total
        );
        assert!(
            stats.despawns_completed_total >= 16,
            "spawn/despawn churn must despawn (got {})",
            stats.despawns_completed_total
        );
        assert!(
            stats.events_produced >= 16,
            "typed events must be produced (got {})",
            stats.events_produced
        );
        assert!(
            stats.events_processed >= 16,
            "typed events must be committed (got {})",
            stats.events_processed
        );
        assert!(
            stats.cadence_executions_total >= 32,
            "cadence consumers must fire (got {})",
            stats.cadence_executions_total
        );
        assert!(
            sampled_queued_max > 0,
            "end-of-tick 1 Hz samples should see remaining scheduler live jobs"
        );
        assert_eq!(
            sampled_spawn_max, 0,
            "spawn_queue_depth is a short-lived gauge; 1 Hz samples after the seed window may be zero"
        );
        assert_eq!(
            actions_at_tick_29,
            Some(0),
            "actions last 12 ticks then idle until refill; a 1 Hz sample can miss the active window"
        );
        assert!(
            sampled_actions_max > 0,
            "refill ticks (30/60/90) start actions, so aligned 1 Hz samples can see actions_active"
        );
    }

    #[test]
    fn spawn_churn_does_not_unbounded_grow() {
        let mut world = World::new();
        let cfg = LoadValidationConfig {
            spawn_despawn: SpawnPressure {
                count: 4,
                interval_ticks: 4,
            },
            ..LoadValidationConfig::default()
        };
        let mut pressure = LoadPressure::from_config(cfg);
        let mut peak = 0u32;
        for t in 1..=40 {
            let tick = SimulationTick::from_count(t);
            pressure.maintain(&mut world, WorldAddress::DEV, tick);
            tick_world(&mut world, t);
            peak = peak.max(world.len());
        }
        let end = world.len();
        assert!(
            peak < 80,
            "churn must not accumulate without despawn (peak={peak})"
        );
        assert!(end < 80, "end population {end} grew without bound");
    }
}
