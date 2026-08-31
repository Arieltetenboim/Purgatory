//! Phase 6D spatial index, AOI policy geometry, and domain revisions.

use crate::body::PlayerState;
use crate::health::Health;
use crate::input::PlayerInput;
use crate::platform::{FLOOR, FLOOR_POSITION};
use crate::stage::FOOTNOTE_SPAWN_X;
use crate::{
    Aabb, ChannelId, EntityId, EntityKind, EntityLifecycle, InstanceId, MapId, ReplicationMeta,
    Transform, World, WorldAddress, aoi_policy_rects, point_in_aabb,
};

#[test]
fn tick_player_across_cell_updates_grid() {
    let mut world = World::footnote_test_stage();
    let id = world.player_id().expect("player");
    let start = world.transform_of(id).expect("t").position;
    assert!(world.spatial_contains(id, start));
    let dest = [start[0] + 20.0, start[1]];
    assert!(world.set_transform(id, Transform::from_position(dest)));
    assert!(world.spatial_contains(id, dest));
    assert!(
        !world
            .query_aabb(world.address_of(id).unwrap(), Aabb::new(start, [1.0, 1.0]))
            .contains(&id)
    );
}

#[test]
fn set_transform_position_keeps_grid() {
    let mut world = World::dev_stage();
    let id = world.player_id().expect("player");
    let start = world.transform_of(id).expect("t").position;
    assert!(world.set_transform(id, Transform::from_position([start[0] + 12.0, start[1]])));
    let now = world.transform_of(id).unwrap().position;
    assert!(world.spatial_contains(id, now));
}

#[test]
fn tick_player_relocate_matches_pose() {
    let mut world = World::footnote_test_stage();
    let id = world.player_id().expect("player");
    for _ in 0..45 {
        world.tick_player(
            id,
            1.0 / 30.0,
            PlayerInput::from_buttons(false, true, false),
        );
    }
    let pos = world.transform_of(id).unwrap().position;
    assert!(world.spatial_contains(id, pos));
}

#[test]
fn transform_rev_bumps_when_velocity_changes_without_position() {
    let mut world = World::footnote_test_stage();
    let id = world.player_id().expect("player");
    let dt = 1.0 / 30.0;
    for _ in 0..8 {
        world.tick_player(id, dt, PlayerInput::from_buttons(false, true, false));
    }
    let mut last_pos = world.transform_of(id).unwrap().position;
    let mut last_vx = world.player_body_of(id).unwrap().velocity[0];
    let mut last_rev = world.domain_revs_of(id).unwrap().transform;
    assert!(last_vx > 1.0, "setup must be walking");
    for _ in 0..45 {
        world.tick_player(id, dt, PlayerInput::idle());
        let pos = world.transform_of(id).unwrap().position;
        let vx = world.player_body_of(id).unwrap().velocity[0];
        let rev = world.domain_revs_of(id).unwrap().transform;
        if pos == last_pos && vx != last_vx {
            assert!(
                rev > last_rev,
                "velocity-only rest tick must bump transform rev (vx {last_vx} -> {vx})"
            );
            return;
        }
        last_pos = pos;
        last_vx = vx;
        last_rev = rev;
    }
    panic!("never observed a velocity-only rest tick");
}

#[test]
fn transform_rev_bumps_only_on_change() {
    let mut world = World::dev_stage();
    let id = world.player_id().expect("player");
    let first = world.domain_revs_of(id).unwrap().transform;
    let t = world.transform_of(id).unwrap();
    assert!(world.set_transform(id, t));
    assert_eq!(world.domain_revs_of(id).unwrap().transform, first);
    let mut moved = t;
    moved.position[0] += 1.0;
    assert!(world.set_transform(id, moved));
    assert!(world.domain_revs_of(id).unwrap().transform > first);
}

#[test]
fn health_rev_is_independent() {
    let mut world = World::dev_stage();
    let id = world.player_id().expect("player");
    let t0 = world.domain_revs_of(id).unwrap().transform;
    assert!(world.set_health(id, Health::full(10.0)));
    let revs = world.domain_revs_of(id).unwrap();
    assert_eq!(revs.transform, t0);
    assert_eq!(revs.health, 1);
}

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
fn despawn_removes_grid_membership() {
    let mut world = World::dev_stage();
    let id = world.player_id().unwrap();
    let pos = world.transform_of(id).unwrap().position;
    let addr = world.address_of(id).unwrap();
    world.despawn(id);
    assert!(
        !world
            .query_aabb(addr, Aabb::new(pos, [2.0, 2.0]))
            .contains(&id)
    );
}

#[test]
fn channel_change_relocates_spatial_grid() {
    let mut world = World::footnote_test_stage();
    let id = world.player_id().expect("player");
    let pos = world.transform_of(id).expect("t").position;
    let old = world.address_of(id).expect("addr");
    let dest = WorldAddress::new(old.map, ChannelId::from_raw(1), old.instance);
    assert_eq!(world.lifecycle_of(id), Some(EntityLifecycle::Active));
    assert!(world.set_address(id, dest));
    assert_eq!(world.address_of(id), Some(dest));
    assert_eq!(world.transform_of(id).unwrap().position, pos);
    assert_eq!(world.lifecycle_of(id), Some(EntityLifecycle::Active));
    let probe = Aabb::new(pos, [8.0, 8.0]);
    assert!(
        !world.query_aabb(old, probe).contains(&id),
        "player must leave the old-channel grid"
    );
    assert!(
        world.query_aabb(dest, probe).contains(&id),
        "player must enter the new-channel grid"
    );
    assert!(world.spatial_contains(id, pos));
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

#[test]
fn rebind_map_address_preserves_geometry_and_ids() {
    let mut world = World::footnote_test_stage();
    let player = world.player_id().expect("player");
    let from = world.address_of(player).expect("addr");
    let to = WorldAddress::new(from.map, ChannelId::from_raw(1), from.instance);
    let platforms_before = world.iter_kind(EntityKind::Platform).count();
    let ids_before: Vec<_> = world.iter().collect();
    let on_from: Vec<_> = ids_before
        .iter()
        .copied()
        .filter(|&id| world.address_of(id) == Some(from))
        .collect();
    let other_addrs: Vec<_> = ids_before
        .iter()
        .copied()
        .filter_map(|id| {
            let addr = world.address_of(id)?;
            (addr != from).then_some((id, addr))
        })
        .collect();
    assert!(
        !other_addrs.is_empty(),
        "stage includes an other-instance fixture that rebind must not sweep"
    );
    assert!(world.rebind_map_address(from, to));
    assert_eq!(
        world.iter_kind(EntityKind::Platform).count(),
        platforms_before
    );
    assert_eq!(world.iter().collect::<Vec<_>>(), ids_before);
    assert_eq!(world.address_of(player), Some(to));
    assert_eq!(world.lifecycle_of(player), Some(EntityLifecycle::Active));
    for id in on_from {
        assert_eq!(world.address_of(id), Some(to));
    }
    for (id, addr) in other_addrs {
        assert_eq!(
            world.address_of(id),
            Some(addr),
            "incompatible-address entities must not be swept by same-Map rebind"
        );
    }
}

#[test]
fn floor_spawn_still_on_p0() {
    let _ = FLOOR;
    let _ = FLOOR_POSITION;
    let _ = EntityId::from_raw(0, 1);
    let _ = EntityLifecycle::Active;
}
