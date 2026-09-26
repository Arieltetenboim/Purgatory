//! Phase 6C map instantiate / destroy / transition (JSON-free).

use crate::{
    ContentId, InstantiateError, MapRuntimePlan, PlanPlatform, Platform, PlayerInput, PlayerState,
    RuntimeSpawnRequest, Transform, World, WorldAddress, WorldBounds,
};
use purgatory_common::{ChannelId, InstanceId, MapId};

fn tiny_plan(address: WorldAddress, token: u64) -> MapRuntimePlan {
    MapRuntimePlan {
        address,
        map_content: ContentId::from_token(token),
        bounds: WorldBounds::DEV_COMPACT,
        platforms: vec![PlanPlatform {
            position: [0.0, -1.0],
            platform: Platform::solid([4.0, 0.2]),
            content_id: Some(ContentId::from_token(token)),
        }],
        placements: vec![],
    }
}

#[test]
fn instantiate_rejects_empty_plan_before_mutation() {
    let mut world = World::new();
    let plan = MapRuntimePlan {
        address: WorldAddress::DEV,
        map_content: ContentId::from_token(1),
        bounds: WorldBounds::DEV_COMPACT,
        platforms: vec![],
        placements: vec![],
    };
    assert!(matches!(
        world.instantiate_map(&plan),
        Err(InstantiateError::EmptyPlan)
    ));
    assert_eq!(world.len(), 0);
    assert_eq!(world.instantiated_count(), 0);
}

#[test]
fn instantiate_then_already_instantiated() {
    let mut world = World::new();
    let plan = tiny_plan(WorldAddress::DEV, 11);
    world.instantiate_map(&plan).expect("first");
    assert_eq!(world.instantiated_count(), 1);
    assert!(matches!(
        world.instantiate_map(&plan),
        Err(InstantiateError::AlreadyInstantiated)
    ));
}

#[test]
fn same_map_two_addresses_are_isolated() {
    let mut world = World::new();
    let a = WorldAddress::DEV;
    let b = WorldAddress::new(MapId::DEV, ChannelId::DEFAULT, InstanceId::from_raw(2));
    world.instantiate_map(&tiny_plan(a, 21)).expect("A");
    world.instantiate_map(&tiny_plan(b, 21)).expect("B");
    assert_eq!(world.instantiated_count(), 2);
    let n_a = world.entities_at(a).count();
    let n_b = world.entities_at(b).count();
    assert_eq!(n_a, 1);
    assert_eq!(n_b, 1);
}

#[test]
fn authored_spawn_preserves_content_id_without_persistent_id() {
    let mut world = World::new();
    let content = ContentId::from_authored("map.dev.footnote").unwrap();
    let id = world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([1.0, 2.0]))
                .with_content(content),
        )
        .unwrap();
    assert_eq!(world.content_id_of(id), Some(content));
    assert!(world.persistent_id_of(id).is_none());
}

#[test]
fn destroy_map_keeps_players() {
    let mut world = World::new();
    let plan = tiny_plan(WorldAddress::DEV, 31);
    world.instantiate_map(&plan).unwrap();
    let floor = world.iter_platforms().next().unwrap();
    let (transform, player) = PlayerState::standing_on_at(floor.id, floor.top_surface(), 0.0);
    let actor = world.spawn_player(transform, player);
    let before = world.len();
    let n = world.destroy_map(WorldAddress::DEV);
    assert!(n >= 1);
    assert!(world.contains(actor));
    assert!(world.len() < before);
    assert!(!world.map_instantiated(WorldAddress::DEV));
}

#[test]
fn transition_moves_entity_without_despawn() {
    let mut world = World::new();
    world
        .instantiate_map(&tiny_plan(WorldAddress::DEV, 41))
        .unwrap();
    let dest = WorldAddress::new(MapId::from_raw(2), ChannelId::DEFAULT, InstanceId::DEFAULT);
    world.instantiate_map(&tiny_plan(dest, 42)).unwrap();
    let floor = world
        .iter_platforms()
        .find(|v| world.address_of(v.id) == Some(WorldAddress::DEV))
        .unwrap();
    let (transform, player) = PlayerState::standing_on_at(floor.id, floor.top_surface(), 0.0);
    let actor = world.spawn_player(transform, player);
    assert!(world.transition_entity(actor, dest, [1.5, 0.0]));
    assert!(world.contains(actor));
    assert_eq!(world.address_of(actor), Some(dest));
    assert_eq!(world.transform_of(actor).unwrap().position, [1.5, 0.0]);
}

#[test]
fn overlapping_geometry_does_not_collide_across_address() {
    let mut world = World::new();
    let a = WorldAddress::DEV;
    let b = WorldAddress::new(MapId::from_raw(2), ChannelId::DEFAULT, InstanceId::DEFAULT);
    let mut plan_a = tiny_plan(a, 51);
    plan_a.platforms[0].position = [0.0, -1.0];
    let mut plan_b = tiny_plan(b, 52);
    plan_b.platforms[0].position = [0.0, 4.0];
    plan_b.platforms[0].platform = Platform::solid([8.0, 0.4]);
    world.instantiate_map(&plan_a).unwrap();
    world.instantiate_map(&plan_b).unwrap();
    let floor = world
        .iter_platforms()
        .find(|v| world.address_of(v.id) == Some(a))
        .unwrap();
    let (transform, player) = PlayerState::standing_on_at(floor.id, floor.top_surface(), 0.0);
    let actor = world.spawn_player_at(a, transform, player);
    let y0 = world.player_body_of(actor).unwrap().position[1];
    world.tick_player(actor, 1.0 / 30.0, PlayerInput::idle());
    let y1 = world.player_body_of(actor).unwrap().position[1];
    assert!(
        (y1 - y0).abs() < 0.2,
        "must stay on map A floor, y0={y0} y1={y1}"
    );
    assert_eq!(world.address_of(actor), Some(a));
}

#[test]
fn ensure_map_is_idempotent() {
    let mut world = World::new();
    let plan = tiny_plan(WorldAddress::DEV, 61);
    let first = world.ensure_map(&plan).unwrap();
    let second = world.ensure_map(&plan).unwrap();
    assert_eq!(first.address, second.address);
    assert_eq!(world.instantiated_count(), 1);
}
