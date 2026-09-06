//! AOI visibility behavior tests (permanent owner-oriented suites).
//
//! Covers enter/leave AOI geometry, spatial candidate visibility semantics,
//! non-spatial/owner-only visibility cases, and WorldAddress relevance isolation.

use crate::PlayerState;
use crate::stage::FOOTNOTE_SPAWN_X;
use crate::{
    ChannelId, InstanceId, MapId, ReplicationMeta, Transform, World, WorldAddress,
    aoi_policy_rects, point_in_aabb,
};

#[test]
fn spatial_candidates_are_not_hysteresis() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let floor = world.iter_platforms().next().expect("floor");
    let (t, s) = PlayerState::standing_on_at(floor.id, floor.top_surface(), FOOTNOTE_SPAWN_X);
    let observer = world.spawn_player(t, s);
    let far = FOOTNOTE_SPAWN_X + 40.0;
    let (t, s) = PlayerState::standing_on_at(floor.id, floor.top_surface(), far);
    let remote = world.spawn_player(t, s);
    let candidates = world.spatial_candidates(observer);
    assert!(candidates.contains(&observer));
    assert!(
        !candidates.contains(&remote),
        "leave-rect candidates must not include a far entity"
    );
    let rects = world.aoi_rects_for(observer).unwrap();
    let remote_pos = world.transform_of(remote).unwrap().position;
    assert!(!point_in_aabb(remote_pos, rects.leave));
}

#[test]
fn leave_rect_includes_hysteresis_band_but_world_does_not_enter() {
    let bounds = crate::WorldBounds::FOOTNOTE_TEST;
    let rects = aoi_policy_rects([0.0, 0.0], bounds);
    let band = [rects.enter.max_x() + 0.5, 0.0];
    assert!(!point_in_aabb(band, rects.enter));
    assert!(point_in_aabb(band, rects.leave));
}

#[test]
fn visible_observers_without_transform_are_not_candidates() {
    let mut world = World::new();
    let observer = world
        .spawn(
            crate::RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([0.0, 0.0]))
                .visible(),
        )
        .unwrap();
    let ghost = world
        .spawn(crate::RuntimeSpawnRequest::transient_at(WorldAddress::DEV).visible())
        .unwrap();
    assert!(world.transform_of(ghost).is_none());
    let candidates = world.spatial_candidates(observer);
    assert!(!candidates.contains(&ghost));
}

#[test]
fn owner_only_may_be_non_spatial() {
    let mut world = World::new();
    let id = world
        .spawn(
            crate::RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([0.0, 0.0])),
        )
        .unwrap();
    world.set_replication(id, ReplicationMeta::owner_only());
    let candidates = world.spatial_candidates(id);
    assert_eq!(candidates, vec![id]);
}

#[test]
fn platforms_are_not_spatial_candidates() {
    let world = World::dev_stage();
    let player = world.player_id().unwrap();
    let candidates = world.spatial_candidates(player);
    for id in world.iter_platforms() {
        assert!(!candidates.contains(&id.id));
    }
    assert!(candidates.contains(&player));
}

#[test]
fn relevance_isolates_world_address_components() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let floor = world.iter_platforms().next().expect("floor");
    let (t, s) = PlayerState::standing_on_at(floor.id, floor.top_surface(), FOOTNOTE_SPAWN_X);
    let a = world.spawn_player(t, s);
    let (t, s) = PlayerState::standing_on_at(floor.id, floor.top_surface(), FOOTNOTE_SPAWN_X + 0.5);
    let b = world.spawn_player(t, s);
    assert!(world.relevance_for(a).contains(&b));
    assert!(world.relevance_for(b).contains(&a));

    let same_map = world.address_of(a).unwrap();
    let other_channel = WorldAddress::new(same_map.map, ChannelId::from_raw(1), same_map.instance);
    assert!(world.set_address(b, other_channel));
    assert!(
        !world.relevance_for(a).contains(&b),
        "same map, different channel"
    );
    assert!(!world.relevance_for(b).contains(&a));

    let other_instance = WorldAddress::new(same_map.map, same_map.channel, InstanceId::from_raw(2));
    assert!(world.set_address(b, other_instance));
    assert!(
        !world.relevance_for(a).contains(&b),
        "same map+channel, different instance"
    );

    let other_map = WorldAddress::new(MapId::from_raw(99), same_map.channel, same_map.instance);
    assert!(world.set_address(b, other_map));
    assert!(!world.relevance_for(a).contains(&b), "different map");

    assert!(world.set_address(b, same_map));
    assert!(
        world.relevance_for(a).contains(&b),
        "same map+channel+instance is eligible"
    );
}
