//! Basic attack behavior tests (Phase 9B -> permanent).
//! Basic Attack activation, delivery, hit/miss, cooldown and shared player/NPC runtime.

use crate::ability::{
    AbilityActivation, AbilityDefinition, AbilityDelivery, AbilityEffect, AbilityRejectReason,
    AbilityRequest, AbilityTiming,
};
use crate::action::{ActionKind, ActionPhase};
use crate::action_gate::ActionGateContext;
use crate::spawn::RuntimeSpawnRequest;
use crate::time::SimulationTick;
use crate::transform::Transform;
use crate::{Health, PLAYER_HEALTH_MAX, World, WorldAddress};
use purgatory_common::ContentId;

/// Authored `content/shared/abilities/skill.basic.strike.json` (keep in sync).
fn basic_strike() -> AbilityDefinition {
    AbilityDefinition {
        id: ContentId::from_authored("skill.basic.strike").unwrap(),
        timing: AbilityTiming {
            windup_ticks: 3,
            active_ticks: 2,
            recovery_ticks: 4,
            cooldown_ticks: 12,
        },
        activation: AbilityActivation::Independent,
        delivery: AbilityDelivery::ForwardQuery {
            range: 1.5,
            half_height: 0.8,
            max_targets: 8,
        },
        effects: vec![AbilityEffect::Damage { amount: 5.0 }],
    }
}

fn tick_critical(world: &mut World, tick: u64) {
    world.begin_tick(SimulationTick::from_count(tick));
    world.drain_critical_scheduler();
}

fn spawn_combatant(world: &mut World, x: f32, hp: f32) -> crate::EntityId {
    world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([x, 1.0]))
                .with_health(Health::full(hp))
                .visible(),
        )
        .expect("spawn")
}

fn activate(world: &mut World, actor: crate::EntityId, def: &AbilityDefinition) {
    world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: def,
            },
            ActionGateContext::in_world(),
        )
        .expect("activate");
}

#[test]
fn basic_attack_starts_with_no_target_nearby() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let def = basic_strike();
    let started = world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &def,
            },
            ActionGateContext::in_world(),
        )
        .expect("empty space is a valid activation");
    assert_eq!(started.kind, ActionKind::Ability { id: def.id });
    assert_eq!(started.phase, ActionPhase::Windup);
    tick_critical(&mut world, 4);
    let live = world.active_action(actor).unwrap();
    assert_eq!(live.phase, ActionPhase::Active);
    assert_eq!(world.health_of(actor).unwrap().current, 10.0);
}

#[test]
fn target_inside_forward_query_takes_damage() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let inside = spawn_combatant(&mut world, 1.0, 10.0);
    activate(&mut world, actor, &basic_strike());
    tick_critical(&mut world, 4);
    assert!((world.health_of(inside).unwrap().current - 5.0).abs() < 1e-5);
}

#[test]
fn target_outside_forward_query_is_untouched() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let behind = spawn_combatant(&mut world, -1.0, 10.0);
    let far = spawn_combatant(&mut world, 4.0, 10.0);
    activate(&mut world, actor, &basic_strike());
    tick_critical(&mut world, 4);
    assert_eq!(world.health_of(behind).unwrap().current, 10.0);
    assert_eq!(world.health_of(far).unwrap().current, 10.0);
}

#[test]
fn no_health_in_query_is_safe() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let bare = world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([1.0, 1.0]))
                .visible(),
        )
        .expect("bare");
    activate(&mut world, actor, &basic_strike());
    tick_critical(&mut world, 4);
    assert!(world.health_of(bare).is_none());
}

#[test]
fn dead_attacker_cannot_start() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 1.0);
    assert!(world.apply_damage(actor, 1.0));
    let denied = world.request_ability(
        AbilityRequest {
            actor,
            selected: None,
            definition: &basic_strike(),
        },
        ActionGateContext::in_world(),
    );
    assert_eq!(denied, Err(AbilityRejectReason::ActorDead));
}

#[test]
fn one_active_execution_damages_each_target_once() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let a = spawn_combatant(&mut world, 0.5, 10.0);
    let b = spawn_combatant(&mut world, 1.0, 10.0);
    activate(&mut world, actor, &basic_strike());
    tick_critical(&mut world, 4);
    tick_critical(&mut world, 5);
    tick_critical(&mut world, 6);
    assert!((world.health_of(a).unwrap().current - 5.0).abs() < 1e-5);
    assert!((world.health_of(b).unwrap().current - 5.0).abs() < 1e-5);
}

#[test]
fn cooldown_blocks_immediate_reactivation() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let def = basic_strike();
    activate(&mut world, actor, &def);
    assert_eq!(
        world.request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &def,
            },
            ActionGateContext::in_world(),
        ),
        Err(AbilityRejectReason::OnCooldown)
    );
}

#[test]
fn runtime_does_not_special_case_basic_strike_id() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let inside = spawn_combatant(&mut world, 1.0, 10.0);
    let mut def = basic_strike();
    def.id = ContentId::from_authored("skill.test.forward_cleave").unwrap();
    activate(&mut world, actor, &def);
    tick_critical(&mut world, 4);
    assert!((world.health_of(inside).unwrap().current - 5.0).abs() < 1e-5);
    let live = world.active_action(actor).unwrap();
    assert_eq!(live.kind, ActionKind::Ability { id: def.id });
}

#[test]
fn player_and_npc_combatants_can_use_the_same_path() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let player = crate::fixtures::RuntimeFixtures::test_player(&mut world);
    assert!(world.set_health(player, Health::full(PLAYER_HEALTH_MAX)));
    let _ = world.set_transform(player, Transform::from_position([0.0, 1.0]));
    let npc = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [1.0, 1.0],
            1,
            2.0,
            1,
            SimulationTick::from_count(1),
            true,
            10.0,
        ))
        .expect("npc");
    let mut npc_state = world.npc_of(npc).unwrap();
    npc_state.heading = [1.0, 0.0];
    assert!(world.set_npc(npc, npc_state));
    activate(&mut world, player, &basic_strike());
    tick_critical(&mut world, 4);
    assert!((world.health_of(npc).unwrap().current - 5.0).abs() < 1e-5);

    tick_critical(&mut world, 20);
    let dummy = spawn_combatant(&mut world, 2.0, 10.0);
    activate(&mut world, npc, &basic_strike());
    tick_critical(&mut world, 23);
    assert!((world.health_of(dummy).unwrap().current - 5.0).abs() < 1e-5);
}

#[test]
fn strike_workload_is_not_the_ability_path() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let target = spawn_combatant(&mut world, 1.0, 10.0);
    let action = world
        .request_action(
            crate::ActionRequest {
                actor,
                target,
                kind: ActionKind::Strike,
            },
            ActionGateContext::in_world(),
        )
        .unwrap();
    assert_eq!(action.kind, ActionKind::Strike);
    assert_eq!(action.phase, ActionPhase::Active);
    assert!((world.health_of(target).unwrap().current - 9.0).abs() < 1e-5);
}
