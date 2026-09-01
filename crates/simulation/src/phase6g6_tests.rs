//! Phase 6G.6 — AOI locality characterization scenarios (measure only).

use crate::{PlayerState, SPATIAL_CELL_SIZE_WU, World, aoi_policy_rects, point_in_aabb};

fn spawn_player_at(world: &mut World, x: f32) -> crate::EntityId {
    let floor = world.iter_platforms().next().expect("floor");
    let (t, s) = PlayerState::standing_on_at(floor.id, floor.top_surface(), x);
    world.spawn_player(t, s)
}

fn exact_leave_xor_observers(
    world: &World,
    address: purgatory_common::WorldAddress,
    old: [f32; 2],
    new: [f32; 2],
) -> usize {
    let bounds = world.bounds_for(address);
    world
        .iter_kind(crate::EntityKind::Player)
        .filter(|&obs| world.address_of(obs) == Some(address))
        .filter(|&obs| {
            let Some(pos) = world.transform_of(obs).map(|t| t.position) else {
                return false;
            };
            let leave = aoi_policy_rects(pos, bounds).leave;
            point_in_aabb(old, leave) != point_in_aabb(new, leave)
        })
        .count()
}

#[test]
fn small_displacement_dirties_only_mover_when_xor_empty() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let left = world.bounds().min_x + 2.0;
    let mid = 0.0;
    let mut crowd = Vec::new();
    for i in 0..20 {
        crowd.push(spawn_player_at(&mut world, left + i as f32 * 0.1));
    }
    let mover = spawn_player_at(&mut world, mid);
    for id in crowd.iter().copied().chain(std::iter::once(mover)) {
        world.clear_interest_observer_dirty(id);
    }
    world.reset_interest_locality();

    let old = world.transform_of(mover).unwrap().position;
    let mut t = world.transform_of(mover).unwrap();
    t.position[0] += 0.05; // tiny same-cell move
    let new = t.position;
    let addr = world.address_of(mover).unwrap();
    let xor = exact_leave_xor_observers(&world, addr, old, new);
    world.set_transform(mover, t);
    let dirty = world.interest_dirty_observers_sorted();
    assert_eq!(xor, 0);
    assert_eq!(
        dirty,
        vec![mover],
        "6G.7A: zero leave-XOR ⇒ only mover dirty"
    );
    let snap = world.interest_locality_snapshot();
    assert_eq!(snap.same_cell_moves, 1);
    assert_eq!(snap.membership_xor_total, 0);
    assert!(
        (new[0] - old[0]).abs() < SPATIAL_CELL_SIZE_WU,
        "fixture expects same-cell displacement"
    );
}

#[test]
fn cell_boundary_cross_still_local_vs_far_cluster() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let left = world.bounds().min_x + 1.0;
    let right = world.bounds().max_x - 1.0;
    let mut far = Vec::new();
    for i in 0..16 {
        far.push(spawn_player_at(&mut world, left + i as f32 * 0.05));
    }
    let mover = spawn_player_at(&mut world, right - SPATIAL_CELL_SIZE_WU * 0.25);
    for id in far.iter().copied().chain(std::iter::once(mover)) {
        world.clear_interest_observer_dirty(id);
    }
    world.reset_interest_locality();
    let mut t = world.transform_of(mover).unwrap();
    t.position[0] += SPATIAL_CELL_SIZE_WU; // cross at least one cell
    world.set_transform(mover, t);
    let dirty = world.interest_dirty_observers_sorted();
    let snap = world.interest_locality_snapshot();
    assert_eq!(snap.cell_boundary_crossings, 1);
    for id in &far {
        assert!(
            !dirty.contains(id),
            "far cluster dirtied on right cell cross"
        );
    }
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
fn dense_mutual_visibility_dirties_most_of_cluster() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let mut ids = Vec::new();
    for i in 0..24 {
        ids.push(spawn_player_at(&mut world, -2.0 + i as f32 * 0.15));
    }
    for id in &ids {
        world.clear_interest_observer_dirty(*id);
    }
    world.reset_interest_locality();
    let mover = ids[12];
    let mut t = world.transform_of(mover).unwrap();
    t.position[0] += 0.1;
    world.set_transform(mover, t);
    let dirtied = world.interest_dirty_observer_count();
    assert!(
        dirtied >= ids.len() / 2,
        "dense cluster should dirty many observers, got {dirtied}/{}",
        ids.len()
    );
}

#[test]
fn mover_among_stationary_crowd_records_dirtied_per_move() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let mut stationary = Vec::new();
    for i in 0..30 {
        stationary.push(spawn_player_at(&mut world, -8.0 + i as f32 * 0.2));
    }
    let mover = spawn_player_at(&mut world, 8.0);
    for id in stationary.iter().copied().chain(std::iter::once(mover)) {
        world.clear_interest_observer_dirty(id);
    }
    world.reset_interest_locality();
    let mut t = world.transform_of(mover).unwrap();
    t.position[0] -= 0.3;
    world.set_transform(mover, t);
    let snap = world.interest_locality_snapshot();
    assert_eq!(snap.entities_moved_total, 1);
    assert!(snap.observers_dirtied_per_moved_entity() >= 1.0);
    eprintln!(
        "6G6 mover_in_crowd dirtied_per_move={:.2} max={}",
        snap.observers_dirtied_per_moved_entity(),
        snap.observers_dirtied.max
    );
}

#[test]
fn aoi_boundary_proximity_can_change_membership_for_near_observer() {
    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let observer = spawn_player_at(&mut world, 0.0);
    let rects = world.aoi_rects_for(observer).unwrap();
    // Place remote just outside enter, then step inward.
    let x_out = rects.enter.max_x() + 0.4;
    let remote = spawn_player_at(&mut world, x_out);
    world.clear_interest_observer_dirty(observer);
    world.clear_interest_observer_dirty(remote);
    world.reset_interest_locality();
    let old = world.transform_of(remote).unwrap().position;
    let mut t = world.transform_of(remote).unwrap();
    t.position[0] = rects.enter.max_x() - 0.2;
    let new = t.position;
    let addr = world.address_of(remote).unwrap();
    let xor = exact_leave_xor_observers(&world, addr, old, new);
    world.set_transform(remote, t);
    assert!(
        world.interest_observer_dirty(observer) || xor > 0,
        "crossing toward enter should dirty observer or register leave-xor"
    );
}
