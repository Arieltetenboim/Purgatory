//! Cross-system runtime services integration tests retained from Phase 6G.
//! Focused, behavior-oriented contracts that exercise composition between
//! World, Scheduler, Actions, Effects, Cadence, and RuntimeEvent delivery.

use crate::action::{ActionEnd, ActionKind};
use crate::action_gate::ActionGateContext;
use crate::effect::EffectKind;
use crate::entity::EntityKind;
use crate::runtime_event::RuntimeEvent;
use crate::scheduler::{ScheduleOwner, ScheduledKind, WorkLane};
use crate::spawn::RuntimeSpawnRequest;
use crate::time::SimulationTick;
use crate::transform::Transform;
use crate::{World, WorldAddress};

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
fn runtime_event_order_preserves_spawn_action_effect() {
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
fn generic_entity_can_use_runtime_services_without_character() {
    let mut world = World::new();
    let owner = spawn_generic(&mut world, 0.0);
    assert_eq!(world.kind(owner), Some(EntityKind::Generic));
    world.begin_tick(SimulationTick::from_count(1));
    assert!(
        world
            .try_start_action(
                owner,
                ActionKind::Test { token: 1 },
                ActionGateContext::in_world()
            )
            .is_ok()
    );
    assert!(
        world
            .apply_test_effect(owner, 3, EffectKind::Test { token: 1 }, None)
            .is_ok()
    );
}

#[test]
fn world_owned_scheduled_work_survives_unrelated_entity_despawn() {
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

#[test]
fn stale_entity_id_does_not_resurrect() {
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
                ActionGateContext::in_world()
            )
            .is_err()
    );
}

#[test]
fn despawn_cancels_pending_complete_action() {
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

#[test]
fn replication_entity_need_not_be_player() {
    let mut world = World::new();
    let id = spawn_generic(&mut world, 0.0);
    let replicated: Vec<_> = world.replicated_in_address(WorldAddress::DEV).collect();
    assert!(replicated.contains(&id));
    assert_eq!(world.kind(id), Some(EntityKind::Generic));
}
