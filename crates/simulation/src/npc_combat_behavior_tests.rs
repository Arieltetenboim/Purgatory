//! NPC combat behavior tests (Phase 9E -> permanent).
//! acquisition, approach, attack geometry/execution, reacquisition, dead/despawn handling and deterministic targeting.

use crate::ability::{
    AbilityActivation, AbilityDefinition, AbilityDelivery, AbilityEffect, AbilityRequest,
    AbilityTiming, forward_query_aabb,
};
use crate::action_gate::ActionGateContext;
use crate::fixtures::RuntimeFixtures;
use crate::health::Health;
use crate::npc::{NPC_HEALTH_MAX, STRIKE_RANGE};
use crate::time::SimulationTick;
use crate::transform::Transform;
use crate::{ContentId, PLAYER_HEALTH_MAX, World, WorldAddress};

const AGGRO_RADIUS: f32 = 3.0;
const STRIKE_ABILITY_RANGE: f32 = 1.5;

fn strike() -> AbilityDefinition {
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

fn setup(player_x: f32, npc_x: f32) -> (World, crate::EntityId, crate::EntityId) {
    let mut world = World::new();
    let player = RuntimeFixtures::test_player(&mut world);
    world.set_health(player, Health::full(20.0));
    world.set_transform(player, Transform::from_position([player_x, 1.0]));
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let npc = world
        .spawn(
            World::npc_spawn_request(
                WorldAddress::DEV,
                [npc_x, 1.0],
                9,
                1.0,
                7,
                now,
                true,
                NPC_HEALTH_MAX,
            )
            .with_health(Health::full(NPC_HEALTH_MAX)),
        )
        .unwrap();
    let mut state = world.npc_of(npc).unwrap();
    state.walking = false;
    world.set_npc(npc, state);
    world.grant_ability(npc, strike().id);
    (world, npc, player)
}

#[test]
fn live_combat_creature_receives_basic_strike_grant() {
    let (world, npc, _) = setup(1.0, 0.0);
    assert!(world.ability_granted(npc, strike().id));
}

#[test]
fn nearby_living_player_is_acquired_and_outside_is_not() {
    let (world, npc, player) = setup(1.0, 0.0);
    assert_eq!(
        world.nearest_living_player_target(npc, AGGRO_RADIUS),
        Some(player)
    );
    let (world, npc, _) = setup(4.0, 0.0);
    assert_eq!(world.nearest_living_player_target(npc, AGGRO_RADIUS), None);
}

#[test]
fn creature_approaches_living_player_outside_strike_range() {
    let (mut world, npc, player) = setup(2.5, 0.0);
    let before = world.transform_of(npc).unwrap().position;
    world.tick_npcs_with_approach(1.0 / 30.0, Some((AGGRO_RADIUS, STRIKE_ABILITY_RANGE, 0.8)));
    let after = world.transform_of(npc).unwrap().position;

    assert!(after[0] > before[0]);
    assert!(after[0] < world.transform_of(player).unwrap().position[0]);
    assert!(world.npc_of(npc).unwrap().velocity[0] > 0.0);
}

#[test]
fn creature_stops_inside_strike_range_and_keeps_ability_path() {
    let (mut world, npc, player) = setup(1.0, 0.0);
    let before = world.transform_of(npc).unwrap().position;
    world.tick_npcs_with_approach(1.0 / 30.0, Some((AGGRO_RADIUS, STRIKE_ABILITY_RANGE, 0.8)));

    assert_eq!(world.transform_of(npc).unwrap().position, before);
    assert_eq!(world.npc_of(npc).unwrap().velocity, [0.0, 0.0]);

    let def = strike();
    world.drive_npc_combat(&def, AGGRO_RADIUS);
    assert!(world.active_action(npc).is_some());
    world.begin_tick(SimulationTick::from_count(4));
    world.drain_critical_scheduler();
    assert_eq!(world.health_of(player).unwrap().current, 15.0);
}

#[test]
fn creature_at_forward_query_boundary_can_hit_before_approach_stops() {
    let (mut world, npc, player) = setup(1.5, 0.0);
    world.tick_npcs_with_approach(1.0 / 30.0, Some((AGGRO_RADIUS, STRIKE_ABILITY_RANGE, 0.8)));
    assert_eq!(world.npc_of(npc).unwrap().velocity, [0.0, 0.0]);

    let def = strike();
    world.drive_npc_combat(&def, AGGRO_RADIUS);
    world.begin_tick(SimulationTick::from_count(4));
    world.drain_critical_scheduler();
    assert_eq!(world.health_of(player).unwrap().current, 15.0);
}

#[test]
fn creature_approach_uses_forward_query_geometry_before_attacking_offset_target() {
    let (mut world, npc, player) = setup(1.0, 0.0);
    world.set_transform(player, Transform::from_position([1.0, 2.05]));

    let initial = world.transform_of(npc).unwrap().position;
    let initial_query = forward_query_aabb(initial, 1.0, STRIKE_ABILITY_RANGE, 0.8);
    let distance_sq = 1.0_f32 * 1.0 + 1.05 * 1.05;
    assert!(distance_sq < STRIKE_ABILITY_RANGE * STRIKE_ABILITY_RANGE);
    assert!(!initial_query.contains_point([1.0, 2.05]));

    world.tick_npcs_with_approach(1.0 / 30.0, Some((AGGRO_RADIUS, STRIKE_ABILITY_RANGE, 0.8)));
    assert_ne!(world.transform_of(npc).unwrap().position, initial);
    assert_ne!(world.npc_of(npc).unwrap().velocity, [0.0, 0.0]);

    for _ in 0..30 {
        if world.npc_of(npc).unwrap().velocity == [0.0, 0.0] {
            break;
        }
        world.tick_npcs_with_approach(1.0 / 30.0, Some((AGGRO_RADIUS, STRIKE_ABILITY_RANGE, 0.8)));
    }

    let stopped = world.transform_of(npc).unwrap().position;
    assert_eq!(world.npc_of(npc).unwrap().velocity, [0.0, 0.0]);
    let stopped_query = forward_query_aabb(stopped, 1.0, STRIKE_ABILITY_RANGE, 0.8);
    assert!(stopped_query.contains_point([1.0, 2.05]));

    let def = strike();
    world.drive_npc_combat(&def, AGGRO_RADIUS);
    world.begin_tick(SimulationTick::from_count(4));
    world.drain_critical_scheduler();
    assert_eq!(world.health_of(player).unwrap().current, 15.0);
}

#[test]
fn dead_player_is_not_acquired() {
    let (mut world, npc, player) = setup(1.0, 0.0);
    world.set_health(
        player,
        Health {
            current: 0.0,
            max: 20.0,
        },
    );
    assert_eq!(world.nearest_living_player_target(npc, AGGRO_RADIUS), None);
}

#[test]
fn dead_target_stops_existing_approach() {
    let (mut world, npc, player) = setup(2.5, 0.0);
    world.tick_npcs_with_approach(1.0 / 30.0, Some((AGGRO_RADIUS, STRIKE_ABILITY_RANGE, 0.8)));
    let before = world.transform_of(npc).unwrap().position;
    world.set_health(
        player,
        Health {
            current: 0.0,
            max: 20.0,
        },
    );
    world.tick_npcs_with_approach(1.0 / 30.0, Some((AGGRO_RADIUS, STRIKE_ABILITY_RANGE, 0.8)));

    assert_eq!(world.transform_of(npc).unwrap().position, before);
    assert_eq!(world.npc_of(npc).unwrap().velocity, [0.0, 0.0]);
}

#[test]
fn despawned_target_stops_existing_approach() {
    let (mut world, npc, player) = setup(2.5, 0.0);
    world.tick_npcs_with_approach(1.0 / 30.0, Some((AGGRO_RADIUS, STRIKE_ABILITY_RANGE, 0.8)));
    let before = world.transform_of(npc).unwrap().position;
    assert!(world.despawn(player));
    world.tick_npcs_with_approach(1.0 / 30.0, Some((AGGRO_RADIUS, STRIKE_ABILITY_RANGE, 0.8)));

    assert_eq!(world.transform_of(npc).unwrap().position, before);
    assert_eq!(world.npc_of(npc).unwrap().velocity, [0.0, 0.0]);
}

#[test]
fn approach_reacquires_another_live_player() {
    let (mut world, npc, first) = setup(2.5, 0.0);
    let second = RuntimeFixtures::test_player(&mut world);
    world.set_health(second, Health::full(20.0));
    world.set_transform(second, Transform::from_position([-2.5, 1.0]));
    world.set_health(
        first,
        Health {
            current: 0.0,
            max: 20.0,
        },
    );
    world.tick_npcs_with_approach(1.0 / 30.0, Some((AGGRO_RADIUS, STRIKE_ABILITY_RANGE, 0.8)));

    assert!(world.transform_of(npc).unwrap().position[0] < 0.0);
    assert!(world.npc_of(npc).unwrap().velocity[0] < 0.0);
}

#[test]
fn acquisition_is_deterministic_with_multiple_players() {
    let (mut world, npc, first) = setup(1.0, 0.0);
    let second = RuntimeFixtures::test_player(&mut world);
    world.set_health(second, Health::full(20.0));
    world.set_transform(second, Transform::from_position([1.0, 1.0]));
    let expected = if (first.index(), first.generation()) < (second.index(), second.generation()) {
        first
    } else {
        second
    };
    assert_eq!(
        world.nearest_living_player_target(npc, AGGRO_RADIUS),
        Some(expected)
    );
}

#[test]
fn driver_requests_ability_and_runtime_applies_damage() {
    let (mut world, npc, player) = setup(1.0, 0.0);
    let def = strike();
    world.drive_npc_combat(&def, AGGRO_RADIUS);
    assert!(world.active_action(npc).is_some());
    assert_eq!(world.health_of(player).unwrap().current, 20.0);
    world.begin_tick(SimulationTick::from_count(4));
    world.drain_critical_scheduler();
    assert_eq!(world.health_of(player).unwrap().current, 15.0);
    assert!(world.presentation_oneshot_of(player).is_some());
}

#[test]
fn lifecycle_and_cooldown_prevent_attack_spam() {
    let (mut world, npc, player) = setup(1.0, 0.0);
    let def = strike();
    world.drive_npc_combat(&def, AGGRO_RADIUS);
    let first = world.active_action(npc).unwrap();
    world.drive_npc_combat(&def, AGGRO_RADIUS);
    assert_eq!(world.active_action(npc).unwrap().id, first.id);
    world.begin_tick(SimulationTick::from_count(4));
    world.drain_critical_scheduler();
    assert_eq!(world.health_of(player).unwrap().current, 15.0);
    assert!(!world.ability_cooldown_ready(npc, def.id));
}

#[test]
fn dead_creature_stops_requesting_abilities() {
    let (mut world, npc, player) = setup(1.0, 0.0);
    let def = strike();
    world.set_health(
        npc,
        Health {
            current: 0.0,
            max: 20.0,
        },
    );
    world.drive_npc_combat(&def, AGGRO_RADIUS);
    assert!(world.active_action(npc).is_none());
    assert_eq!(world.health_of(player).unwrap().current, 20.0);
}

#[test]
fn player_triggered_basic_strike_path_still_works() {
    let (mut world, _, player) = setup(1.0, 0.0);
    let target = RuntimeFixtures::test_mob_like(&mut world);
    world.set_transform(
        target,
        Transform::from_position([player_position(&world, player)[0] + 1.0, 1.0]),
    );
    let def = strike();
    world.grant_ability(player, def.id);
    world
        .request_ability(
            AbilityRequest {
                actor: player,
                selected: None,
                definition: &def,
            },
            ActionGateContext::in_world(),
        )
        .unwrap();
    world.begin_tick(SimulationTick::from_count(4));
    world.drain_critical_scheduler();
    assert_eq!(world.health_of(target).unwrap().current, 15.0);
}

fn player_position(world: &World, player: crate::EntityId) -> [f32; 2] {
    world.transform_of(player).unwrap().position
}

#[test]
fn nearest_health_target_skips_players() {
    let mut world = World::new();
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let npc = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.0, 1.0],
            1,
            4.0,
            1,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let player = crate::fixtures::RuntimeFixtures::test_player(&mut world);
    assert!(world.set_health(player, Health::full(PLAYER_HEALTH_MAX)));
    let _ = world.set_transform(player, Transform::from_position([0.5, 1.0]));
    let other = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [1.0, 1.0],
            2,
            4.0,
            2,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let target = world.nearest_health_target(npc, STRIKE_RANGE).unwrap();
    assert_eq!(target, other);
    assert_ne!(target, player);
}
