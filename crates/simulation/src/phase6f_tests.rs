//! Phase 6F runtime-service tests. Synthetic actions/effects only.

use crate::action::{ActionEnd, ActionKind, ActionPhase};
use crate::action_gate::ActionGateContext;
use crate::cadence::Cadence;
use crate::effect::EffectKind;
use crate::entity::EntityKind;
use crate::query::{QueryFilter, QueryLimit};
use crate::runtime_event::RuntimeEvent;
use crate::scheduler::{DEFERRED_DRAIN_BUDGET, ScheduleOwner, ScheduledKind, WorkLane};
use crate::spawn::RuntimeSpawnRequest;
use crate::time::SimulationTick;
use crate::transform::Transform;
use crate::{Aabb, Health, World, WorldAddress};

fn tick_world(world: &mut World, tick: u64) {
    world.begin_tick(SimulationTick::from_count(tick));
    world.drain_critical_scheduler();
    let _ = world.commit_runtime_events();
    world.pump_cadence();
    world.drain_deferred_scheduler();
}

fn spawn_generic(world: &mut World, x: f32) -> crate::EntityId {
    world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([x, 1.0]))
                .visible(),
        )
        .expect("spawn")
}

#[test]
fn action_request_start_complete() {
    let mut world = World::new();
    let owner = spawn_generic(&mut world, 0.0);
    world.begin_tick(SimulationTick::from_count(1));
    let action = world
        .try_start_action(
            owner,
            ActionKind::Test { token: 1 },
            ActionGateContext::in_world(),
        )
        .expect("start");
    assert_eq!(action.phase, ActionPhase::Active);
    let ended = world.end_action(action.id, ActionEnd::Completed).unwrap();
    assert_eq!(ended.phase, ActionPhase::Completed);
    assert!(world.active_action(owner).is_none());
}

#[test]
fn action_gate_transition_locked_does_not_create_slot() {
    let mut world = World::new();
    let owner = spawn_generic(&mut world, 0.0);
    world.begin_tick(SimulationTick::from_count(1));
    let ctx = ActionGateContext {
        transition: Some(crate::InputGateReason::MapTransition),
        session_bound: true,
    };
    let denied = world.try_start_action(owner, ActionKind::Test { token: 8 }, ctx);
    assert_eq!(denied, Err(crate::ActionDenialReason::TransitionLocked));
    assert!(world.active_action(owner).is_none());
}

#[test]
fn action_gate_rejects_without_slot() {
    let mut world = World::new();
    let owner = spawn_generic(&mut world, 0.0);
    world.begin_tick(SimulationTick::from_count(1));
    world
        .try_start_action(
            owner,
            ActionKind::Test { token: 1 },
            ActionGateContext::in_world(),
        )
        .unwrap();
    let denied = world.try_start_action(
        owner,
        ActionKind::Test { token: 2 },
        ActionGateContext::in_world(),
    );
    assert_eq!(denied, Err(crate::ActionDenialReason::Busy));
    assert_eq!(
        world.active_action(owner).unwrap().kind,
        ActionKind::Test { token: 1 }
    );
    let events = world.commit_runtime_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ActionRejected { .. }))
    );
}

#[test]
fn action_cancel_and_no_double_complete() {
    let mut world = World::new();
    let owner = spawn_generic(&mut world, 0.0);
    world.begin_tick(SimulationTick::from_count(1));
    let action = world
        .try_start_action(
            owner,
            ActionKind::Test { token: 3 },
            ActionGateContext::in_world(),
        )
        .unwrap();
    world.end_action(action.id, ActionEnd::Cancelled).unwrap();
    assert!(world.end_action(action.id, ActionEnd::Completed).is_err());
}

#[test]
fn owner_loss_cancels_action() {
    let mut world = World::new();
    let owner = spawn_generic(&mut world, 0.0);
    world.begin_tick(SimulationTick::from_count(1));
    let action = world
        .try_start_action(
            owner,
            ActionKind::Test { token: 4 },
            ActionGateContext::in_world(),
        )
        .unwrap();
    assert!(world.despawn(owner));
    assert!(world.active_action(owner).is_none());
    let events = world.commit_runtime_events();
    assert!(events.iter().any(|e| matches!(
        e,
        RuntimeEvent::ActionEnded { id, end: ActionEnd::Cancelled, .. } if *id == action.id
    )));
}

#[test]
fn effect_apply_expire_remove() {
    let mut world = World::new();
    let target = spawn_generic(&mut world, 2.0);
    world.begin_tick(SimulationTick::from_count(10));
    let effect = world
        .apply_test_effect(target, 2, EffectKind::Test { token: 9 }, None)
        .unwrap();
    assert!(world.effect(effect.id).is_some());
    tick_world(&mut world, 11);
    assert!(world.effect(effect.id).is_some());
    tick_world(&mut world, 12);
    assert!(world.effect(effect.id).is_none());
}

#[test]
fn effect_explicit_remove_stale_expiry_is_noop() {
    let mut world = World::new();
    let target = spawn_generic(&mut world, 2.0);
    world.begin_tick(SimulationTick::from_count(1));
    let effect = world
        .apply_test_effect(target, 5, EffectKind::Test { token: 1 }, None)
        .unwrap();
    assert!(world.remove_effect(effect.id).is_some());
    assert!(world.remove_effect(effect.id).is_none());
    tick_world(&mut world, 6);
    assert!(world.effect(effect.id).is_none());
}

#[test]
fn effect_target_despawn_cleans_up() {
    let mut world = World::new();
    let target = spawn_generic(&mut world, 3.0);
    world.begin_tick(SimulationTick::from_count(1));
    let effect = world
        .apply_test_effect(target, 30, EffectKind::Test { token: 2 }, None)
        .unwrap();
    world.despawn(target);
    assert!(world.effect(effect.id).is_none());
    tick_world(&mut world, 31);
}

#[test]
fn scheduled_spawn_mints_fresh_entity_id() {
    let mut world = World::new();
    world.begin_tick(SimulationTick::from_count(1));
    let first = spawn_generic(&mut world, 0.0);
    world.despawn(first);
    let req = RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
        .with_transform(Transform::from_position([4.0, 1.0]))
        .visible();
    world
        .schedule_spawn(
            req,
            SimulationTick::from_count(3),
            ScheduleOwner::World,
            WorkLane::Deferred,
        )
        .unwrap();
    tick_world(&mut world, 2);
    assert_eq!(world.len(), 0);
    tick_world(&mut world, 3);
    assert_eq!(world.len(), 1);
    let spawned = world.iter_kind(EntityKind::Generic).next().unwrap();
    assert_ne!(spawned, first, "respawn must not assume the old EntityId");
    assert!(world.contains(spawned));
    assert!(!world.contains(first));
}

#[test]
fn scheduled_spawn_cancel() {
    let mut world = World::new();
    world.begin_tick(SimulationTick::from_count(1));
    let id = world
        .schedule_spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([1.0, 1.0]))
                .visible(),
            SimulationTick::from_count(4),
            ScheduleOwner::World,
            WorkLane::Deferred,
        )
        .unwrap();
    assert!(world.cancel_scheduled_spawn(id));
    tick_world(&mut world, 4);
    assert_eq!(world.len(), 0);
}

#[test]
fn stale_entity_scheduled_work_is_noop() {
    let mut world = World::new();
    let entity = spawn_generic(&mut world, 0.0);
    world.begin_tick(SimulationTick::from_count(1));
    world
        .schedule_at(
            SimulationTick::from_count(2),
            ScheduleOwner::Entity(entity),
            WorkLane::Critical,
            ScheduledKind::DespawnEntity(entity),
        )
        .unwrap();
    world.despawn(entity);
    tick_world(&mut world, 2);
}

#[test]
fn spatial_query_isolates_world_address_and_despawn() {
    let mut world = World::new();
    let a = spawn_generic(&mut world, 0.0);
    let b = spawn_generic(&mut world, 1.0);
    let other = world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::new(
                purgatory_common::MapId::DEV,
                purgatory_common::ChannelId::from_raw(1),
                purgatory_common::InstanceId::DEFAULT,
            ))
            .with_transform(Transform::from_position([0.0, 1.0]))
            .visible(),
        )
        .unwrap();
    let hits = world.query_radius(WorldAddress::DEV, [0.0, 1.0], 2.0);
    assert!(hits.contains(&a) && hits.contains(&b));
    assert!(!hits.contains(&other));
    world.despawn(a);
    let after = world.query_aabb(WorldAddress::DEV, Aabb::new([0.5, 1.0], [2.0, 2.0]));
    assert!(!after.contains(&a));
    let capped = world.query_radius_filtered(
        WorldAddress::DEV,
        [0.0, 1.0],
        8.0,
        QueryFilter::Any,
        Some(QueryLimit::max(1)),
    );
    assert_eq!(capped.len(), 1);
}

#[test]
fn cadence_is_not_every_tick() {
    let mut world = World::new();
    world.register_cadence(Cadence::EveryN { n: 4 }, 7);
    let mut due_ticks = 0u32;
    for t in 0..8 {
        world.begin_tick(SimulationTick::from_count(t));
        world.pump_cadence();
        let events = world.commit_runtime_events();
        if events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::CadenceFired { token: 7, .. }))
        {
            due_ticks += 1;
        }
    }
    assert_eq!(due_ticks, 2);
}

#[test]
fn events_are_staged_not_recursive() {
    let mut world = World::new();
    let id = spawn_generic(&mut world, 0.0);
    world.begin_tick(SimulationTick::from_count(1));
    world
        .schedule_at(
            SimulationTick::from_count(1),
            ScheduleOwner::World,
            WorkLane::Critical,
            ScheduledKind::RaiseEvent { token: 11 },
        )
        .unwrap();
    world.drain_critical_scheduler();
    let first = world.commit_runtime_events();
    assert!(
        first
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ScheduledFired { token: 11, .. }))
    );
    assert!(
        first
            .iter()
            .any(|e| matches!(e, RuntimeEvent::EntitySpawned { .. }))
    );
    let _ = id;
    world
        .schedule_at(
            SimulationTick::from_count(1),
            ScheduleOwner::World,
            WorkLane::Critical,
            ScheduledKind::RaiseEvent { token: 12 },
        )
        .unwrap();
    world.drain_critical_scheduler();
    let second = world.commit_runtime_events();
    assert!(
        second
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ScheduledFired { token: 12, .. }))
    );
}

#[test]
fn deferred_budget_progress_and_critical_not_behind() {
    let mut world = World::new();
    world.begin_tick(SimulationTick::from_count(1));
    let n = DEFERRED_DRAIN_BUDGET + 5;
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
    world
        .schedule_at(
            SimulationTick::from_count(1),
            ScheduleOwner::World,
            WorkLane::Critical,
            ScheduledKind::TestProbe { token: 999 },
        )
        .unwrap();
    world.drain_critical_scheduler();
    let after_crit = world.commit_runtime_events();
    assert!(
        after_crit
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ScheduledFired { token: 999, .. }))
    );
    world.drain_deferred_scheduler();
    let deferred = world.commit_runtime_events();
    let probes = deferred
        .iter()
        .filter(|e| matches!(e, RuntimeEvent::ScheduledFired { token, .. } if *token < 900))
        .count();
    assert_eq!(probes, DEFERRED_DRAIN_BUDGET as usize);
    world.drain_deferred_scheduler();
    let rest = world.commit_runtime_events();
    let rest_n = rest
        .iter()
        .filter(|e| matches!(e, RuntimeEvent::ScheduledFired { token, .. } if *token < 900))
        .count();
    assert_eq!(rest_n, 5);
}

#[test]
fn health_query_filter_and_same_inputs_stable() {
    let mut world = World::new();
    let a = world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([0.0, 0.0]))
                .with_health(Health::full(1.0))
                .visible(),
        )
        .unwrap();
    let _b = spawn_generic(&mut world, 0.2);
    let first = world.query_radius_filtered(
        WorldAddress::DEV,
        [0.0, 0.0],
        2.0,
        QueryFilter::HasHealth,
        None,
    );
    let second = world.query_radius_filtered(
        WorldAddress::DEV,
        [0.0, 0.0],
        2.0,
        QueryFilter::HasHealth,
        None,
    );
    assert_eq!(first, second);
    assert_eq!(first, vec![a]);
}
