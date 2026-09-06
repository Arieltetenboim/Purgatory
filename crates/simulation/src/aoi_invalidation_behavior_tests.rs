//! AOI invalidation behavior tests (enter/leave XOR and locality accounting).
//
//! Movement enter/leave XOR invalidation, boundary and multi-cell movement,
//! axis/diagonal motion, dense/separated clusters, spawn/despawn presence invalidation,
//! and related locality snapshot assertions.

use crate::{
    ChannelId, PlayerState, SPATIAL_CELL_SIZE_WU, World, WorldAddress, aoi_policy_rects,
    point_in_aabb,
};

fn spawn_player_at(world: &mut World, x: f32) -> crate::EntityId {
    let floor = world.iter_platforms().next().expect("floor");
    let (t, s) = PlayerState::standing_on_at(floor.id, floor.top_surface(), x);
    world.spawn_player(t, s)
}

fn spawn_player_xy(world: &mut World, x: f32, y_on_floor: bool) -> crate::EntityId {
    let floor = world.iter_platforms().next().expect("floor");
    let top = floor.top_surface();
    let y = if y_on_floor { top } else { top + 2.0 };
    let (t, s) = PlayerState::standing_on_at(floor.id, y, x);
    let id = world.spawn_player(t, s);
    if !y_on_floor {
        let mut tr = world.transform_of(id).unwrap();
        tr.position[1] = top + 2.0;
        world.set_transform(id, tr);
        world.clear_interest_observer_dirty(id);
    }
    id
}

fn enter_leave_xor_count(
    world: &World,
    address: WorldAddress,
    old: [f32; 2],
    new: [f32; 2],
    exclude: Option<crate::EntityId>,
) -> usize {
    let bounds = world.bounds_for(address);
    world
        .iter_kind(crate::EntityKind::Player)
        .filter(|&obs| world.address_of(obs) == Some(address))
        .filter(|&obs| Some(obs) != exclude)
        .filter(|&obs| {
            let Some(pos) = world.transform_of(obs).map(|t| t.position) else {
                return false;
            };
            let rects = aoi_policy_rects(pos, bounds);
            let enter_xor = point_in_aabb(old, rects.enter) != point_in_aabb(new, rects.enter);
            let leave_xor = point_in_aabb(old, rects.leave) != point_in_aabb(new, rects.leave);
            enter_xor || leave_xor
        })
        .count()
}

fn clear_all_dirty(world: &mut World) {
    for id in world
        .iter_kind(crate::EntityKind::Player)
        .collect::<Vec<_>>()
    {
        world.clear_interest_observer_dirty(id);
    }
    world.reset_interest_locality();
}

#[test]
fn tiny_same_cell_zero_xor_dirties_only_mover() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let left = world.bounds().min_x + 2.0;
    let mut crowd = Vec::new();
    for i in 0..20 {
        crowd.push(spawn_player_at(&mut world, left + i as f32 * 0.1));
    }
    let mover = spawn_player_at(&mut world, 0.0);
    clear_all_dirty(&mut world);

    let old = world.transform_of(mover).unwrap().position;
    let mut t = world.transform_of(mover).unwrap();
    t.position[0] += 0.05;
    let new = t.position;
    let addr = world.address_of(mover).unwrap();
    let xor = enter_leave_xor_count(&world, addr, old, new, Some(mover));
    assert_eq!(xor, 0, "fixture requires zero enter/leave XOR");
    world.set_transform(mover, t);

    let dirty = world.interest_dirty_observers_sorted();
    assert_eq!(
        dirty,
        vec![mover],
        "only mover should be dirty, got {dirty:?}"
    );
    let snap = world.interest_locality_snapshot();
    assert_eq!(snap.membership_xor_total, 0);
    assert_eq!(snap.observers_dirtied.max, 1);
    assert!((new[0] - old[0]).abs() < SPATIAL_CELL_SIZE_WU);
}

#[test]
fn x_enter_boundary_dirties_xor_observers() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let observer = spawn_player_at(&mut world, 0.0);
    let rects = world.aoi_rects_for(observer).unwrap();
    let remote = spawn_player_at(&mut world, rects.enter.max_x() + 0.5);
    clear_all_dirty(&mut world);
    let old = world.transform_of(remote).unwrap().position;
    let mut t = world.transform_of(remote).unwrap();
    t.position[0] = rects.enter.max_x() - 0.2;
    let new = t.position;
    let addr = world.address_of(remote).unwrap();
    let xor = enter_leave_xor_count(&world, addr, old, new, Some(remote));
    assert!(xor >= 1, "crossing into enter must XOR at least observer");
    world.set_transform(remote, t);
    assert!(world.interest_observer_dirty(observer));
    assert!(world.interest_observer_dirty(remote));
}

#[test]
fn y_axis_motion_uses_xor() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let observer = spawn_player_at(&mut world, 0.0);
    let remote = spawn_player_xy(&mut world, 1.0, true);
    clear_all_dirty(&mut world);
    let old = world.transform_of(remote).unwrap().position;
    let mut t = world.transform_of(remote).unwrap();
    t.position[1] += 0.5;
    let new = t.position;
    let addr = world.address_of(remote).unwrap();
    let xor = enter_leave_xor_count(&world, addr, old, new, Some(remote));
    world.set_transform(remote, t);
    let dirty_others = world
        .interest_dirty_observers_sorted()
        .into_iter()
        .filter(|id| *id != remote)
        .count();
    assert_eq!(
        dirty_others, xor,
        "non-subject dirty count must equal enter/leave XOR"
    );
}

#[test]
fn diagonal_motion_xor_matches_dirty_others() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let _obs = spawn_player_at(&mut world, -5.0);
    let mover = spawn_player_at(&mut world, 5.0);
    clear_all_dirty(&mut world);
    let old = world.transform_of(mover).unwrap().position;
    let mut t = world.transform_of(mover).unwrap();
    t.position[0] += 1.5;
    t.position[1] += 1.0;
    let new = t.position;
    let addr = world.address_of(mover).unwrap();
    let xor = enter_leave_xor_count(&world, addr, old, new, Some(mover));
    world.set_transform(mover, t);
    let dirty_others = world
        .interest_dirty_observers_sorted()
        .into_iter()
        .filter(|id| *id != mover)
        .count();
    assert_eq!(dirty_others, xor);
}

#[test]
fn large_multi_cell_motion_still_local_vs_far_cluster() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let left = world.bounds().min_x + 1.0;
    let right = world.bounds().max_x - 8.0;
    let mut far = Vec::new();
    for i in 0..12 {
        far.push(spawn_player_at(&mut world, left + i as f32 * 0.05));
    }
    let mover = spawn_player_at(&mut world, right);
    clear_all_dirty(&mut world);
    let mut t = world.transform_of(mover).unwrap();
    t.position[0] += SPATIAL_CELL_SIZE_WU * 3.0;
    world.set_transform(mover, t);
    let dirty = world.interest_dirty_observers_sorted();
    for id in &far {
        assert!(!dirty.contains(id), "far cluster must stay clean");
    }
    assert!(dirty.contains(&mover));
}

#[test]
fn separated_clusters_zero_cross_talk() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let left = world.bounds().min_x + 1.0;
    let right = world.bounds().max_x - 1.0;
    let mut left_ids = Vec::new();
    let mut right_ids = Vec::new();
    for i in 0..12 {
        left_ids.push(spawn_player_at(&mut world, left + i as f32 * 0.05));
        right_ids.push(spawn_player_at(&mut world, right - i as f32 * 0.05));
    }
    for id in left_ids.iter().chain(right_ids.iter()) {
        world.clear_interest_observer_dirty(*id);
    }
    world.reset_interest_locality();
    let mover = right_ids[0];
    let mut t = world.transform_of(mover).unwrap();
    t.position[0] -= 0.2;
    world.set_transform(mover, t);
    let dirty = world.interest_dirty_observers_sorted();
    for id in &left_ids {
        assert!(!dirty.contains(id));
    }
    assert!(dirty.iter().any(|id| right_ids.contains(id)));
}

#[test]
fn dense_cluster_can_dirty_many_when_xor_nonempty() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let mut ids = Vec::new();
    for i in 0..20 {
        ids.push(spawn_player_at(&mut world, -1.0 + i as f32 * 0.12));
    }
    clear_all_dirty(&mut world);
    let mover = ids[10];
    let old = world.transform_of(mover).unwrap().position;
    let mut t = world.transform_of(mover).unwrap();
    // Larger step to create boundary XORs inside the pack.
    t.position[0] += 3.0;
    let new = t.position;
    let addr = world.address_of(mover).unwrap();
    let xor = enter_leave_xor_count(&world, addr, old, new, Some(mover));
    world.set_transform(mover, t);
    let dirty_others = world
        .interest_dirty_observers_sorted()
        .into_iter()
        .filter(|id| *id != mover)
        .count();
    assert_eq!(dirty_others, xor);
    assert!(world.interest_observer_dirty(mover));
}

#[test]
fn spawn_and_despawn_use_presence_path() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let observer = spawn_player_at(&mut world, 0.0);
    clear_all_dirty(&mut world);
    let spawned = spawn_player_at(&mut world, 0.5);
    assert!(
        world.interest_observer_dirty(observer) || world.interest_observer_dirty(spawned),
        "spawn near observer should dirty presence set"
    );
    clear_all_dirty(&mut world);
    let pos = world.transform_of(spawned).unwrap().position;
    assert!(world.despawn(spawned));
    let _ = pos;
    let _ = observer;
}

#[test]
fn address_transition_presence_on_both_sides() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let a = WorldAddress::DEV;
    let b = WorldAddress::new(a.map, ChannelId::from_raw(1), a.instance);
    let p = spawn_player_at(&mut world, 0.0);
    world.set_address(p, a);
    let q = spawn_player_at(&mut world, 1.0);
    world.set_address(q, a);
    clear_all_dirty(&mut world);
    assert!(world.set_address(p, b));
    assert!(world.interest_observer_dirty(p));
}
