//! Spatial index behavior tests: grid membership, movement synchronization,
//! despawn/removal, and channel/address relocations.

use crate::{
    Aabb, ChannelId, EntityKind, EntityLifecycle, PlayerInput, Transform, World, WorldAddress,
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
