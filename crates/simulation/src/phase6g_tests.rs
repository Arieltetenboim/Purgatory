//! Phase 6G integrated runtime gates. Uses existing 6F kinds; does not add
//! gameplay. Critical ceiling stays [`CRITICAL_DRAIN_CEILING`] (1024).

use crate::action::{ActionEnd, ActionKind};
use crate::action_gate::ActionGateContext;
use crate::cadence::Cadence;
use crate::effect::EffectKind;
use crate::entity::EntityKind;
use crate::runtime_event::RuntimeEvent;
use crate::scheduler::{
    CRITICAL_DRAIN_CEILING, DEFERRED_DRAIN_BUDGET, ScheduleOwner, ScheduledKind, WorkLane,
};
use crate::spawn::RuntimeSpawnRequest;
use crate::time::SimulationTick;
use crate::transform::Transform;
use crate::{ChannelId, InstanceId, MapId, World, WorldAddress};

fn tick_world(world: &mut World, tick: u64) {
    world.begin_tick(SimulationTick::from_count(tick));
    world.drain_critical_scheduler();
    let _ = world.commit_runtime_events();
    world.pump_cadence();
    world.drain_deferred_scheduler();
}

fn spawn_generic_at(world: &mut World, address: WorldAddress, x: f32) -> crate::EntityId {
    world
        .spawn(
            RuntimeSpawnRequest::transient_at(address)
                .with_transform(Transform::from_position([x, 1.0]))
                .visible(),
        )
        .expect("spawn")
}

fn spawn_generic(world: &mut World, x: f32) -> crate::EntityId {
    spawn_generic_at(world, WorldAddress::DEV, x)
}

#[test]
fn critical_ceiling_is_1024_and_remainder_carries() {
    assert_eq!(
        CRITICAL_DRAIN_CEILING, 1024,
        "do not lower the ceiling to pass"
    );
    let mut world = World::new();
    world.begin_tick(SimulationTick::from_count(1));
    let extra = 8u32;
    let n = CRITICAL_DRAIN_CEILING.saturating_add(extra);
    for i in 0..n {
        world
            .schedule_at(
                SimulationTick::from_count(1),
                ScheduleOwner::World,
                WorkLane::Critical,
                ScheduledKind::TestProbe { token: i },
            )
            .unwrap();
    }
    world.drain_critical_scheduler();
    let stats = world.runtime_stats();
    assert!(stats.scheduler_critical_ceiling_hits >= 1);
    assert_eq!(stats.scheduler_due_critical, extra);
    world.drain_critical_scheduler();
    assert_eq!(world.runtime_stats().scheduler_due_critical, 0);
}

#[test]
fn deferred_carry_forward_makes_progress() {
    let mut world = World::new();
    world.begin_tick(SimulationTick::from_count(1));
    let n = DEFERRED_DRAIN_BUDGET.saturating_add(6);
    for i in 0..n {
        world
            .schedule_at(
                SimulationTick::from_count(1),
                ScheduleOwner::World,
                WorkLane::Deferred,
                ScheduledKind::TestProbe { token: i },
            )
            .unwrap();
    }
    world.drain_deferred_scheduler();
    let first = world.runtime_stats();
    assert!(first.scheduler_deferred_exhausted >= 1);
    assert_eq!(first.scheduler_deferred_fired, DEFERRED_DRAIN_BUDGET);
    world.drain_deferred_scheduler();
    assert_eq!(world.runtime_stats().scheduler_deferred_fired, 6);
}

#[test]
fn cleanup_after_owner_loss_returns_to_baseline() {
    let mut world = World::new();
    let baseline = world.len();
    let owner = spawn_generic(&mut world, 0.0);
    world.begin_tick(SimulationTick::from_count(1));
    world
        .try_start_action(
            owner,
            ActionKind::Test { token: 1 },
            ActionGateContext::in_world(),
        )
        .unwrap();
    world
        .apply_test_effect(owner, 40, EffectKind::Test { token: 2 }, None)
        .unwrap();
    world
        .schedule_after(
            10,
            ScheduleOwner::Entity(owner),
            WorkLane::Deferred,
            ScheduledKind::TestProbe { token: 3 },
        )
        .unwrap();
    assert!(world.despawn(owner));
    let _ = world.commit_runtime_events();
    assert_eq!(world.len(), baseline);
    let stats = world.runtime_stats();
    assert_eq!(stats.actions_active, 0);
    assert_eq!(stats.effects_active, 0);
    assert_eq!(stats.scheduler_queued, 0);
    assert!(!world.contains(owner));
}

#[test]
fn temporal_order_spawn_action_effect_commit() {
    let mut world = World::new();
    world.begin_tick(SimulationTick::from_count(1));
    let owner = spawn_generic(&mut world, 1.0);
    world
        .try_start_action(
            owner,
            ActionKind::Test { token: 7 },
            ActionGateContext::in_world(),
        )
        .unwrap();
    world
        .apply_test_effect(owner, 5, EffectKind::Test { token: 8 }, None)
        .unwrap();
    let events = world.commit_runtime_events();
    let spawn_at = events
        .iter()
        .position(|e| matches!(e, RuntimeEvent::EntitySpawned { id } if *id == owner));
    let action_at = events
        .iter()
        .position(|e| matches!(e, RuntimeEvent::ActionStarted { owner: o, .. } if *o == owner));
    let effect_at = events
        .iter()
        .position(|e| matches!(e, RuntimeEvent::EffectApplied { target, .. } if *target == owner));
    assert!(spawn_at.unwrap() < action_at.unwrap());
    assert!(action_at.unwrap() < effect_at.unwrap());
}

#[test]
fn cadence_every_n_staggers_and_does_not_fire_all_at_once() {
    let mut world = World::new();
    for i in 0..8 {
        world.register_cadence(Cadence::EveryN { n: 4 }, i);
    }
    tick_world(&mut world, 0);
    let due0 = world.runtime_stats().cadence_due;
    tick_world(&mut world, 1);
    let due1 = world.runtime_stats().cadence_due;
    assert!(due0 > 0);
    assert!(due1 > 0);
    assert_ne!(due0, 8, "EveryN must not fire every consumer on tick 0");
}

/// A: Generic entity consumes action+effect without Character/session.
#[test]
fn case_a_generic_action_and_effect_without_character() {
    let mut world = World::new();
    let owner = spawn_generic(&mut world, 0.0);
    assert_eq!(world.kind(owner), Some(EntityKind::Generic));
    world.begin_tick(SimulationTick::from_count(1));
    assert!(
        world
            .try_start_action(
                owner,
                ActionKind::Test { token: 1 },
                ActionGateContext::in_world(),
            )
            .is_ok()
    );
    assert!(
        world
            .apply_test_effect(owner, 3, EffectKind::Test { token: 1 }, None)
            .is_ok()
    );
}

/// B: Scheduler World owner is independent of Entity/Character.
#[test]
fn case_b_world_owned_job_survives_entity_despawn() {
    let mut world = World::new();
    let owner = spawn_generic(&mut world, 0.0);
    world.begin_tick(SimulationTick::from_count(1));
    world
        .schedule_at(
            SimulationTick::from_count(1),
            ScheduleOwner::World,
            WorkLane::Critical,
            ScheduledKind::TestProbe { token: 44 },
        )
        .unwrap();
    assert!(world.despawn(owner));
    world.drain_critical_scheduler();
    let events = world.commit_runtime_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ScheduledFired { token: 44, .. }))
    );
}

/// C: Cross-WorldAddress visibility isolation.
#[test]
fn case_c_incompatible_address_is_not_visible() {
    let mut world = World::new();
    let here = spawn_generic(&mut world, 0.0);
    let other_addr = WorldAddress::new(MapId::from_raw(9), ChannelId::DEFAULT, InstanceId::DEFAULT);
    let _there = spawn_generic_at(&mut world, other_addr, 0.0);
    let hits = world.query_radius(WorldAddress::DEV, [0.0, 1.0], 8.0);
    assert!(hits.contains(&here));
    assert_eq!(hits.len(), 1);
}

/// D: Stale EntityId cannot address a reused slot.
#[test]
fn case_d_stale_entity_id_does_not_resurrect() {
    let mut world = World::new();
    let first = spawn_generic(&mut world, 0.0);
    assert!(world.despawn(first));
    let second = spawn_generic(&mut world, 1.0);
    assert_ne!(first, second);
    assert!(!world.contains(first));
    assert!(world.contains(second));
    world.begin_tick(SimulationTick::from_count(1));
    assert!(
        world
            .try_start_action(
                first,
                ActionKind::Test { token: 1 },
                ActionGateContext::in_world(),
            )
            .is_err()
    );
}

/// E: Action complete scheduled while owner lives, then owner loss cancels.
#[test]
fn case_e_despawn_cancels_pending_complete_action() {
    let mut world = World::new();
    let owner = spawn_generic(&mut world, 0.0);
    world.begin_tick(SimulationTick::from_count(1));
    let action = world
        .try_start_action(
            owner,
            ActionKind::Test { token: 9 },
            ActionGateContext::in_world(),
        )
        .unwrap();
    world
        .schedule_at(
            SimulationTick::from_count(3),
            ScheduleOwner::Entity(owner),
            WorkLane::Critical,
            ScheduledKind::CompleteAction(action.id),
        )
        .unwrap();
    assert!(world.despawn(owner));
    tick_world(&mut world, 3);
    assert!(world.active_action(owner).is_none());
    assert!(world.end_action(action.id, ActionEnd::Completed).is_err());
}

/// F: Runtime event producer is scheduler/cadence, not a client command.
#[test]
fn case_f_raise_event_is_not_a_player_command() {
    let mut world = World::new();
    world.begin_tick(SimulationTick::from_count(1));
    world
        .schedule_at(
            SimulationTick::from_count(1),
            ScheduleOwner::World,
            WorkLane::Deferred,
            ScheduledKind::RaiseEvent { token: 77 },
        )
        .unwrap();
    world.drain_deferred_scheduler();
    let events = world.commit_runtime_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ScheduledFired { token: 77, .. }))
    );
}

#[test]
fn replication_entity_need_not_be_player() {
    let mut world = World::new();
    let id = spawn_generic(&mut world, 0.0);
    let replicated: Vec<_> = world.replicated_in_address(WorldAddress::DEV).collect();
    assert!(replicated.contains(&id));
    assert_eq!(world.kind(id), Some(EntityKind::Generic));
}
