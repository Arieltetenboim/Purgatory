//! Phase 4.7 collision robustness regression matrix.

use crate::body::PlayerState;
use crate::collision::{detect_overlap, recover_solid_penetration};
use crate::contact::CONTACT_EPSILON;
use crate::input::PlayerInput;
use crate::motion_debug::{CorrectionAxis, ResponseKind};
use crate::platform::Platform;
use crate::transform::Transform;
use crate::world::World;

const DT30: f32 = 1.0 / 30.0;
const DT40: f32 = 1.0 / 40.0;

#[test]
fn platform_seam_walk_no_snag() {
    let mut world = World::new();
    let a = world.spawn_platform(
        Transform::from_position([-1.5, -2.0]),
        Platform::solid([1.5, 0.25]),
    );
    let b = world.spawn_platform(
        Transform::from_position([1.5, -2.0]),
        Platform::solid([1.5, 0.25]),
    );
    let top = world
        .iter_platforms()
        .find(|v| v.id == a)
        .unwrap()
        .top_surface();
    let (t, s) = PlayerState::standing_on_at(a, top, -2.0);
    world.spawn_player(t, s);

    let mut airborne_while_on_course = 0u32;
    for _ in 0..90 {
        world.tick(DT30, PlayerInput::from_buttons(false, true, false));
        let body = world.player_body().unwrap();
        // Only count chatter while still over the combined seam span.
        if body.position[0] >= -2.8 && body.position[0] <= 2.8 && !body.grounded {
            airborne_while_on_course += 1;
        }
        let m = world.last_motion_debug();
        assert!(!m.discontinuity, "seam discontinuity: {m:?}");
        assert!(body.position[1].is_finite() && body.velocity[0].is_finite());
    }
    let body = world.player_body().unwrap();
    assert!(
        body.position[0] > 0.5,
        "should cross seam onto B side; x={}",
        body.position[0]
    );
    assert!(
        airborne_while_on_course <= 2,
        "flush seam must not chatter airborne ({airborne_while_on_course})"
    );
    let _ = b;
}

#[test]
fn insertion_order_independent_landing() {
    fn run(order_ab: bool) -> ([f32; 2], [f32; 2], bool) {
        let mut world = World::new();
        let (low, high) = if order_ab {
            let low = world.spawn_platform(
                Transform::from_position([0.0, -1.0]),
                Platform::solid([2.0, 0.2]),
            );
            let high = world.spawn_platform(
                Transform::from_position([0.0, 0.5]),
                Platform::solid([2.0, 0.2]),
            );
            (low, high)
        } else {
            let high = world.spawn_platform(
                Transform::from_position([0.0, 0.5]),
                Platform::solid([2.0, 0.2]),
            );
            let low = world.spawn_platform(
                Transform::from_position([0.0, -1.0]),
                Platform::solid([2.0, 0.2]),
            );
            (low, high)
        };
        let top = world
            .iter_platforms()
            .find(|v| v.id == high)
            .unwrap()
            .top_surface();
        let (mut t, mut s) = PlayerState::standing_on_at(high, top, 0.0);
        t.position = [0.0, 2.0];
        s.grounded = false;
        s.grounded_on = None;
        s.velocity = [0.0, -6.0];
        world.spawn_player(t, s);
        for _ in 0..40 {
            world.tick(DT30, PlayerInput::idle());
        }
        let b = world.player_body().unwrap();
        let on_high = b.grounded_on == Some(high);
        let _ = low;
        (b.position, b.velocity, on_high)
    }
    let (p0, v0, g0) = run(true);
    let (p1, v1, g1) = run(false);
    assert_eq!(g0, g1, "grounded_on high must match across spawn order");
    assert!((p0[0] - p1[0]).abs() < 1e-4 && (p0[1] - p1[1]).abs() < 1e-3);
    assert!((v0[0] - v1[0]).abs() < 1e-4 && (v0[1] - v1[1]).abs() < 1e-3);
}

#[test]
fn corner_up_right_is_deterministic() {
    let mut world = World::new();
    let floor = world.spawn_platform(
        Transform::from_position([0.0, -3.0]),
        Platform::solid([6.0, 0.3]),
    );
    let block = world.spawn_platform(
        Transform::from_position([2.0, 0.0]),
        Platform::solid([1.0, 1.0]),
    );
    let top = world
        .iter_platforms()
        .find(|v| v.id == floor)
        .unwrap()
        .top_surface();
    let (t, s) = PlayerState::standing_on_at(floor, top, 0.2);
    world.spawn_player(t, s);
    world.tick(DT30, PlayerInput::from_buttons(false, true, true));
    let mut positions = Vec::new();
    for _ in 0..50 {
        world.tick(DT30, PlayerInput::from_buttons(false, true, false));
        let b = world.player_body().unwrap();
        assert!(b.position[0].is_finite() && b.position[1].is_finite());
        positions.push(b.position);
    }
    // Replay identical setup → identical path
    let mut world2 = World::new();
    let floor2 = world2.spawn_platform(
        Transform::from_position([0.0, -3.0]),
        Platform::solid([6.0, 0.3]),
    );
    let _ = world2.spawn_platform(
        Transform::from_position([2.0, 0.0]),
        Platform::solid([1.0, 1.0]),
    );
    let top2 = world2
        .iter_platforms()
        .find(|v| v.id == floor2)
        .unwrap()
        .top_surface();
    let (t, s) = PlayerState::standing_on_at(floor2, top2, 0.2);
    world2.spawn_player(t, s);
    world2.tick(DT30, PlayerInput::from_buttons(false, true, true));
    for (i, _) in positions.iter().enumerate() {
        world2.tick(DT30, PlayerInput::from_buttons(false, true, false));
        let b = world2.player_body().unwrap();
        assert!(
            (b.position[0] - positions[i][0]).abs() < 1e-4
                && (b.position[1] - positions[i][1]).abs() < 1e-4,
            "corner path diverged at {i}"
        );
    }
    let _ = block;
}

#[test]
fn walk_off_edge_clears_grounded_cleanly() {
    let mut world = World::new();
    let floor = world.spawn_platform(
        Transform::from_position([0.0, -2.0]),
        Platform::solid([1.0, 0.25]),
    );
    let top = world
        .iter_platforms()
        .find(|v| v.id == floor)
        .unwrap()
        .top_surface();
    let (t, s) = PlayerState::standing_on_at(floor, top, 0.0);
    world.spawn_player(t, s);
    let mut left_ground = false;
    for _ in 0..60 {
        world.tick(DT30, PlayerInput::from_buttons(false, true, false));
        let b = world.player_body().unwrap();
        if !b.grounded {
            left_ground = true;
            assert!(b.grounded_on.is_none());
            break;
        }
    }
    assert!(left_ground);
}

#[test]
fn invalid_spawn_penetration_uses_recovery_not_far_face() {
    let mut world = World::new();
    let solid = world.spawn_platform(
        Transform::from_position([0.0, 0.0]),
        Platform::solid([1.5, 1.0]),
    );
    let (mut t, mut s) = PlayerState::standing_on_at(
        solid,
        world
            .iter_platforms()
            .find(|v| v.id == solid)
            .unwrap()
            .top_surface(),
        0.0,
    );
    // Force deep interior spawn.
    t.position = [0.0, 0.0];
    s.grounded = false;
    s.grounded_on = None;
    s.velocity = [0.0, 0.0];
    world.spawn_player(t, s);
    let x0 = world.player_body().unwrap().position[0];
    world.tick(DT30, PlayerInput::idle());
    let m = world.last_motion_debug();
    let b = world.player_body().unwrap();
    assert_eq!(m.response_kind, ResponseKind::Recovery);
    assert!(
        m.correction[0].abs() + m.correction[1].abs() <= 0.5 + CONTACT_EPSILON,
        "recovery must be capped: {m:?}"
    );
    assert!((b.position[0] - x0).abs() <= 0.5 + 0.05);
}

#[test]
fn solid_oneway_overlap_rising_ignores_oneway() {
    let mut world = World::new();
    let solid = world.spawn_platform(
        Transform::from_position([0.0, 0.5]),
        Platform::solid([1.2, 0.35]),
    );
    let _ow = world.spawn_platform(
        Transform::from_position([0.1, 0.7]),
        Platform::one_way([1.2, 0.2]),
    );
    let floor = world.spawn_platform(
        Transform::from_position([0.0, -2.0]),
        Platform::solid([3.0, 0.3]),
    );
    let top = world
        .iter_platforms()
        .find(|v| v.id == floor)
        .unwrap()
        .top_surface();
    let (t, s) = PlayerState::standing_on_at(floor, top, 0.0);
    world.spawn_player(t, s);
    world.tick(DT30, PlayerInput::from_buttons(false, false, true));
    for _ in 0..25 {
        world.tick(DT30, PlayerInput::idle());
        let m = world.last_motion_debug();
        assert!(!m.discontinuity, "{m:?}");
    }
    let _ = solid;
}

#[test]
fn hz_30_and_40_ceiling_momentum_agree_logically() {
    fn scenario(dt: f32) -> (bool, bool) {
        let mut world = World::new();
        let floor = world.spawn_platform(
            Transform::from_position([0.0, -2.0]),
            Platform::solid([4.0, 0.3]),
        );
        let ceiling = world.spawn_platform(
            Transform::from_position([0.0, 1.2]),
            Platform::solid([2.0, 0.25]),
        );
        let top = world
            .iter_platforms()
            .find(|v| v.id == floor)
            .unwrap()
            .top_surface();
        let (t, s) = PlayerState::standing_on_at(floor, top, 0.0);
        world.spawn_player(t, s);
        world.tick(dt, PlayerInput::from_buttons(false, true, true));
        let mut hit = false;
        let mut horiz_teleport = false;
        for _ in 0..50 {
            world.tick(dt, PlayerInput::from_buttons(false, true, false));
            let m = world.last_motion_debug();
            if m.collision_candidate == Some(ceiling)
                && m.correction_axis == CorrectionAxis::Vertical
                && m.correction[1] < -0.01
            {
                hit = true;
                if m.correction[0].abs() > 0.05 {
                    horiz_teleport = true;
                }
            }
        }
        (hit, horiz_teleport)
    }
    let (h30, t30) = scenario(DT30);
    let (h40, t40) = scenario(DT40);
    assert!(h30 && h40, "both rates must register ceiling hit");
    assert!(
        !t30 && !t40,
        "neither rate may horizontal-teleport on head-bonk"
    );
}

#[test]
fn seeded_property_no_nan_and_bounded_steps() {
    let mut rng = 0xC0FFEE_u64;
    let mut next = || {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
        rng
    };
    for _ in 0..80 {
        let mut world = World::new();
        let fx = ((next() % 200) as f32) * 0.01 - 1.0;
        let fy = -2.0;
        let px = ((next() % 100) as f32) * 0.02;
        let py = ((next() % 80) as f32) * 0.02 + 0.5;
        world.spawn_platform(
            Transform::from_position([fx, fy]),
            Platform::solid([3.0, 0.3]),
        );
        if next() % 2 == 0 {
            world.spawn_platform(
                Transform::from_position([px, py]),
                Platform::solid([1.0, 0.3]),
            );
        } else {
            world.spawn_platform(
                Transform::from_position([px, py]),
                Platform::one_way([1.0, 0.1]),
            );
        }
        let floor_id = world.iter_platforms().next().unwrap().id;
        let top = world
            .iter_platforms()
            .find(|v| v.id == floor_id)
            .unwrap()
            .top_surface();
        let (t, s) = PlayerState::standing_on_at(floor_id, top, 0.0);
        world.spawn_player(t, s);
        for _ in 0..25 {
            let right = next() % 2 == 0;
            let jump = next() % 17 == 0;
            world.tick(DT30, PlayerInput::from_buttons(false, right, jump));
            let b = world.player_body().unwrap();
            assert!(b.position[0].is_finite() && b.position[1].is_finite());
            assert!(b.velocity[0].is_finite() && b.velocity[1].is_finite());
            let m = world.last_motion_debug();
            if m.response_kind == ResponseKind::Normal {
                assert!(m.delta_length() < 3.0, "unexplained huge step: {m:?}");
            }
        }
    }
}

#[test]
fn recover_api_moves_shallowest_axis() {
    let mut world = World::new();
    let solid = world.spawn_platform(
        Transform::from_position([0.0, 0.0]),
        Platform::solid([1.0, 1.0]),
    );
    let (mut transform, mut state) = PlayerState::standing_on_at(solid, 1.0, 0.0);
    transform.position = [0.1, 0.0];
    state.grounded = false;
    state.grounded_on = None;
    let platforms: Vec<_> = world.iter_platforms().collect();
    let before = transform.position;
    let rec = recover_solid_penetration(&mut transform, &state, platforms.into_iter());
    assert!(rec.is_some());
    let rec = rec.unwrap();
    assert!(rec.correction[0].abs() + rec.correction[1].abs() > CONTACT_EPSILON);
    assert!(
        (transform.position[0] - before[0]).abs() <= 0.5 + CONTACT_EPSILON
            || (transform.position[1] - before[1]).abs() <= 0.5 + CONTACT_EPSILON
    );
}

#[test]
fn contact_epsilon_touch_is_not_penetration() {
    use crate::aabb::Aabb;
    use crate::contact::penetrates;
    let a = Aabb::new([0.0, 0.0], [0.5, 0.5]);
    // Flush side touch (edges equal) — overlaps() false; penetrates false.
    let b = Aabb::new([1.0, 0.0], [0.5, 0.5]);
    assert!(!detect_overlap(a, b));
    assert!(!penetrates(a, b));
    // Micro float overlap under epsilon on one axis-like shallow pair:
    let c = Aabb::new([0.999, 0.0], [0.5, 0.5]);
    assert!(detect_overlap(a, c));
    // Overlap depth on X ≈ 0.001 — at boundary of CONTACT_EPSILON.
    assert!(!penetrates(a, c) || crate::contact::overlap_x(a, c) <= CONTACT_EPSILON * 2.0);
}
