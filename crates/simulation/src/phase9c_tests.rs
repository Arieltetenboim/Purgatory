//! Phase 9C grants and 7.2 Strike isolation. Command path lives in the server crate.

use crate::ability::AbilityId;
use crate::npc::{NPC_HEALTH_MAX, STRIKE_RANGE};
use crate::spawn::RuntimeSpawnRequest;
use crate::time::SimulationTick;
use crate::transform::Transform;
use crate::{Health, PLAYER_HEALTH_MAX, World, WorldAddress};
use purgatory_common::ContentId;

fn strike_id() -> AbilityId {
    ContentId::from_authored("skill.basic.strike").unwrap()
}

#[test]
fn grant_is_explicit_and_dropped_on_despawn() {
    let mut world = World::new();
    let owner = world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([0.0, 1.0]))
                .visible(),
        )
        .unwrap();
    assert!(!world.ability_granted(owner, strike_id()));
    assert!(world.grant_ability(owner, strike_id()));
    assert!(world.ability_granted(owner, strike_id()));
    assert!(world.despawn(owner));
    assert!(!world.ability_granted(owner, strike_id()));
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
