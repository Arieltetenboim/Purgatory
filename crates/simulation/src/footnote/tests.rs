//! FOOTNOTE dynamics, OneWay, and drop-through tests.

use crate::body::PLAYER_HALF_EXTENTS;
use crate::entity::EntityId;
use crate::footnote::FootnoteConfig;
use crate::health::Health;
use crate::input::PlayerInput;
use crate::platform::{
    FLOOR, ONEWAY_A, ONEWAY_A_POSITION, ONEWAY_B, PlatformKind, RAISED_PLATFORM,
    RAISED_PLATFORM_POSITION,
};
use crate::transform::Transform;
use crate::world::World;

const DT_30: f32 = 1.0 / 30.0;
const DT_40: f32 = 1.0 / 40.0;
const CFG: FootnoteConfig = FootnoteConfig::DEFAULT;

fn drive(world: &mut World, ticks: u32, dt: f32, input: PlayerInput) {
    for _ in 0..ticks {
        world.tick(dt, input);
    }
}

fn player(world: &World) -> crate::body::PlayerBody {
    world.player_body().expect("player")
}

fn floor_id(world: &World) -> EntityId {
    world
        .iter_platforms()
        .find(|view| {
            view.platform.kind == PlatformKind::Solid
                && view.platform.half_extents == FLOOR.half_extents
        })
        .expect("floor")
        .id
}

fn raised_id(world: &World) -> EntityId {
    world
        .iter_platforms()
        .find(|view| {
            view.platform.kind == PlatformKind::Solid
                && view.platform.half_extents == RAISED_PLATFORM.half_extents
        })
        .expect("raised")
        .id
}

fn oneway_a_id(world: &World) -> EntityId {
    world
        .iter_platforms()
        .find(|view| {
            view.platform.kind == PlatformKind::OneWay
                && (view.transform.position[0] - ONEWAY_A_POSITION[0]).abs() < 0.01
        })
        .expect("oneway a")
        .id
}

fn set_player(
    world: &mut World,
    position: [f32; 2],
    velocity: [f32; 2],
    grounded: bool,
    grounded_on: Option<EntityId>,
) {
    let Some((transform, player)) = world.player_parts_mut() else {
        panic!("missing player");
    };
    transform.position = position;
    player.velocity = velocity;
    player.grounded = grounded;
    player.grounded_on = grounded_on;
    player.ignored_platform = None;
}

#[test]
fn ground_acceleration_ramps_toward_max() {
    let mut world = World::dev_stage();
    world.tick(DT_30, PlayerInput::from_buttons(false, true, false));
    let v1 = player(&world).velocity[0];
    assert!(v1 > 0.0);
    assert!(v1 < CFG.max_ground_speed);
    world.tick(DT_30, PlayerInput::from_buttons(false, true, false));
    let v2 = player(&world).velocity[0];
    assert!(v2 > v1);
    drive(
        &mut world,
        30,
        DT_30,
        PlayerInput::from_buttons(false, true, false),
    );
    assert!((player(&world).velocity[0] - CFG.max_ground_speed).abs() < 0.05);
}

#[test]
fn stored_footnote_config_caps_ground_speed() {
    let mut world = World::dev_stage();
    world.set_footnote_config(FootnoteConfig::with_move_speed(2.0));
    drive(
        &mut world,
        40,
        DT_30,
        PlayerInput::from_buttons(false, true, false),
    );
    assert!((player(&world).velocity[0] - 2.0).abs() < 0.05);
    assert_eq!(world.footnote_config().max_ground_speed, 2.0);
    assert_eq!(world.footnote_config().max_air_speed, 2.0);
}

#[test]
fn releasing_input_decelerates_without_snap() {
    let mut world = World::dev_stage();
    drive(
        &mut world,
        30,
        DT_30,
        PlayerInput::from_buttons(false, true, false),
    );
    let before = player(&world).velocity[0];
    assert!((before - CFG.max_ground_speed).abs() < 0.05);
    world.tick(DT_30, PlayerInput::idle());
    let after = player(&world).velocity[0];
    assert!(after > 0.0);
    assert!(after < before);
}

#[test]
fn reverse_direction_does_not_teleport_velocity() {
    let mut world = World::dev_stage();
    drive(
        &mut world,
        30,
        DT_30,
        PlayerInput::from_buttons(false, true, false),
    );
    let before = player(&world).velocity[0];
    world.tick(DT_30, PlayerInput::from_buttons(true, false, false));
    let after = player(&world).velocity[0];
    assert!(after < before);
    assert!(after > -CFG.max_ground_speed);
    // Must not snap to -max in one tick.
    assert!(after > -CFG.max_ground_speed + 0.5);
}

#[test]
fn jump_preserves_horizontal_momentum() {
    let mut world = World::dev_stage();
    drive(
        &mut world,
        30,
        DT_30,
        PlayerInput::from_buttons(false, true, false),
    );
    let vx = player(&world).velocity[0];
    world.tick(DT_30, PlayerInput::from_buttons(false, true, true));
    assert!(!player(&world).grounded);
    assert!((player(&world).velocity[0] - vx).abs() < 0.05);
}

#[test]
fn walk_off_preserves_horizontal_momentum() {
    let mut world = World::dev_stage();
    let raised = raised_id(&world);
    let top = RAISED_PLATFORM.top_surface(Transform::from_position(RAISED_PLATFORM_POSITION));
    set_player(
        &mut world,
        [
            RAISED_PLATFORM.max_x(Transform::from_position(RAISED_PLATFORM_POSITION))
                - PLAYER_HALF_EXTENTS[0] * 0.5,
            top + PLAYER_HALF_EXTENTS[1],
        ],
        [CFG.max_ground_speed, 0.0],
        true,
        Some(raised),
    );
    drive(
        &mut world,
        8,
        DT_30,
        PlayerInput::from_buttons(false, true, false),
    );
    let body = player(&world);
    assert!(!body.grounded);
    assert!(body.velocity[0] > CFG.max_ground_speed * 0.5);
    assert!(body.velocity[1] < 0.0);
}

#[test]
fn airborne_idle_preserves_horizontal_velocity() {
    let mut world = World::dev_stage();
    set_player(&mut world, [-2.0, 2.0], [4.0, 0.0], false, None);
    world.tick(DT_30, PlayerInput::idle());
    assert!((player(&world).velocity[0] - 4.0).abs() < 1e-4);
}

#[test]
fn airborne_input_uses_air_acceleration() {
    let mut world = World::dev_stage();
    set_player(&mut world, [-2.0, 2.0], [0.0, 0.0], false, None);
    world.tick(DT_30, PlayerInput::from_buttons(false, true, false));
    let vx = player(&world).velocity[0];
    let expected = CFG.air_acceleration * DT_30;
    assert!((vx - expected).abs() < 1e-3);
    assert!(vx < CFG.max_air_speed);
}

#[test]
fn thirty_and_forty_hz_match_over_one_second() {
    let input = PlayerInput::from_buttons(false, true, false);
    let mut a = World::dev_stage();
    let mut b = World::dev_stage();
    drive(&mut a, 30, DT_30, input);
    drive(&mut b, 40, DT_40, input);
    assert!(
        (player(&a).position[0] - player(&b).position[0]).abs() < 0.15,
        "30 Hz x={} vs 40 Hz x={}",
        player(&a).position[0],
        player(&b).position[0]
    );
}

#[test]
fn rising_player_passes_through_oneway() {
    let mut world = World::dev_stage();
    let top = ONEWAY_A.top_surface(Transform::from_position(ONEWAY_A_POSITION));
    set_player(
        &mut world,
        [ONEWAY_A_POSITION[0], top - 0.8],
        [0.0, CFG.jump_velocity],
        false,
        None,
    );
    // While still ascending, the player must cross above the top (not stopped as a ceiling).
    let mut crossed = false;
    for _ in 0..12 {
        world.tick(DT_30, PlayerInput::idle());
        let body = player(&world);
        if body.aabb().min_y() > top + 0.05 {
            crossed = true;
            break;
        }
        assert!(
            body.velocity[1] > 0.0 || body.aabb().min_y() > top - 0.9,
            "OneWay must not act as a solid ceiling from below"
        );
    }
    assert!(crossed, "rising player must pass upward through OneWay");
}

#[test]
fn descending_player_lands_on_oneway() {
    let mut world = World::dev_stage();
    let oa = oneway_a_id(&world);
    let top = ONEWAY_A.top_surface(Transform::from_position(ONEWAY_A_POSITION));
    set_player(
        &mut world,
        [ONEWAY_A_POSITION[0], top + 1.2],
        [0.0, 0.0],
        false,
        None,
    );
    drive(&mut world, 90, DT_30, PlayerInput::idle());
    let body = player(&world);
    assert!(body.grounded);
    assert_eq!(body.grounded_on, Some(oa));
    assert!((body.aabb().min_y() - top).abs() < 1e-2);
}

#[test]
fn oneway_is_not_a_horizontal_wall() {
    let mut world = World::dev_stage();
    let top = ONEWAY_A.top_surface(Transform::from_position(ONEWAY_A_POSITION));
    let left = ONEWAY_A.min_x(Transform::from_position(ONEWAY_A_POSITION));
    set_player(
        &mut world,
        [left - PLAYER_HALF_EXTENTS[0] - 0.05, top],
        [CFG.max_ground_speed, 0.0],
        false,
        None,
    );
    let start_x = player(&world).position[0];
    drive(
        &mut world,
        10,
        DT_30,
        PlayerInput::from_buttons(false, true, false),
    );
    assert!(player(&world).position[0] > start_x + 0.2);
}

#[test]
fn solid_floor_still_supports() {
    let mut world = World::dev_stage();
    drive(&mut world, 60, DT_30, PlayerInput::idle());
    let body = player(&world);
    assert!(body.grounded);
    assert_eq!(body.grounded_on, Some(floor_id(&world)));
}

#[test]
fn drop_through_oneway_clears_grounding_and_ignores() {
    let mut world = World::dev_stage();
    let oa = oneway_a_id(&world);
    let top = ONEWAY_A.top_surface(Transform::from_position(ONEWAY_A_POSITION));
    set_player(
        &mut world,
        [ONEWAY_A_POSITION[0], top + PLAYER_HALF_EXTENTS[1]],
        [0.0, 0.0],
        true,
        Some(oa),
    );
    world.tick(
        DT_30,
        PlayerInput::from_buttons_ext(false, false, true, true),
    );
    let body = player(&world);
    assert!(!body.grounded);
    assert!(body.grounded_on.is_none());
    assert_eq!(body.ignored_platform, Some(oa));
    assert!(body.velocity[1] < 0.0);
}

#[test]
fn drop_through_does_not_immediately_reland() {
    let mut world = World::dev_stage();
    let oa = oneway_a_id(&world);
    let top = ONEWAY_A.top_surface(Transform::from_position(ONEWAY_A_POSITION));
    set_player(
        &mut world,
        [ONEWAY_A_POSITION[0], top + PLAYER_HALF_EXTENTS[1]],
        [0.0, 0.0],
        true,
        Some(oa),
    );
    world.tick(
        DT_30,
        PlayerInput::from_buttons_ext(false, false, true, true),
    );
    for _ in 0..5 {
        world.tick(DT_30, PlayerInput::idle());
        assert_ne!(player(&world).grounded_on, Some(oa));
    }
}

#[test]
fn ignore_clears_after_passing_below() {
    let mut world = World::dev_stage();
    let oa = oneway_a_id(&world);
    let top = ONEWAY_A.top_surface(Transform::from_position(ONEWAY_A_POSITION));
    set_player(
        &mut world,
        [ONEWAY_A_POSITION[0], top + PLAYER_HALF_EXTENTS[1]],
        [0.0, 0.0],
        true,
        Some(oa),
    );
    world.tick(
        DT_30,
        PlayerInput::from_buttons_ext(false, false, true, true),
    );
    drive(&mut world, 90, DT_30, PlayerInput::idle());
    let body = player(&world);
    assert!(body.ignored_platform.is_none());
    assert!(body.grounded);
    assert_eq!(body.grounded_on, Some(floor_id(&world)));
}

#[test]
fn other_oneway_remains_collidable_during_ignore() {
    let mut world = World::dev_stage();
    let oa = oneway_a_id(&world);
    let ob = world
        .iter_platforms()
        .find(|view| view.platform.kind == PlatformKind::OneWay && view.id != oa)
        .expect("oneway b")
        .id;
    let top_b = ONEWAY_B.top_surface(Transform::from_position(crate::platform::ONEWAY_B_POSITION));
    // Drop through A, then fall onto B region.
    let top_a = ONEWAY_A.top_surface(Transform::from_position(ONEWAY_A_POSITION));
    set_player(
        &mut world,
        [ONEWAY_A_POSITION[0], top_a + PLAYER_HALF_EXTENTS[1]],
        [0.0, 0.0],
        true,
        Some(oa),
    );
    world.tick(
        DT_30,
        PlayerInput::from_buttons_ext(false, false, true, true),
    );
    // Place above B while still ignoring A.
    set_player(
        &mut world,
        [crate::platform::ONEWAY_B_POSITION[0], top_b + 1.0],
        [0.0, 0.0],
        false,
        None,
    );
    if let Some((_, player)) = world.player_parts_mut() {
        player.ignored_platform = Some(oa);
    }
    drive(&mut world, 60, DT_30, PlayerInput::idle());
    assert_eq!(player(&world).grounded_on, Some(ob));
}

#[test]
fn solid_cannot_be_dropped_through() {
    let mut world = World::dev_stage();
    let floor = floor_id(&world);
    let y_before = player(&world).position[1];
    world.tick(
        DT_30,
        PlayerInput::from_buttons_ext(false, false, true, true),
    );
    // Jump fires instead of drop (solid).
    assert!(!player(&world).grounded || player(&world).velocity[1] > 0.0);
    assert!(player(&world).ignored_platform.is_none());
    let _ = (floor, y_before);
}

#[test]
fn drop_while_airborne_is_harmless() {
    let mut world = World::dev_stage();
    set_player(&mut world, [-2.0, 2.0], [0.0, 0.0], false, None);
    world.tick(
        DT_30,
        PlayerInput::from_buttons_ext(false, false, true, true),
    );
    assert!(player(&world).ignored_platform.is_none());
    assert!(!player(&world).grounded);
}

#[test]
fn despawned_ignored_platform_clears() {
    let mut world = World::dev_stage();
    let oa = oneway_a_id(&world);
    if let Some((_, player)) = world.player_parts_mut() {
        player.ignored_platform = Some(oa);
    }
    assert!(world.despawn(oa));
    world.tick(DT_30, PlayerInput::idle());
    assert!(player(&world).ignored_platform.is_none());
}

#[test]
fn grounded_jump_sets_upward_velocity() {
    let mut world = World::dev_stage();
    world.tick(DT_30, PlayerInput::from_buttons(false, false, true));
    let body = player(&world);
    assert!(!body.grounded);
    let expected = CFG.jump_velocity - CFG.gravity * DT_30;
    assert!((body.velocity[1] - expected).abs() < 1e-3);
}

#[test]
fn gravity_accelerates_airborne_player_downward() {
    let mut world = World::dev_stage();
    set_player(&mut world, [-2.0, 2.0], [0.0, 0.0], false, None);
    world.tick(DT_30, PlayerInput::idle());
    assert!((player(&world).velocity[1] + CFG.gravity * DT_30).abs() < 1e-4);
}

#[test]
fn no_input_does_not_move_horizontally_when_at_rest() {
    let mut world = World::dev_stage();
    let start_x = player(&world).position[0];
    drive(&mut world, 30, DT_30, PlayerInput::idle());
    assert!((player(&world).position[0] - start_x).abs() < 1e-4);
    assert!(player(&world).grounded);
}

#[test]
fn dead_player_cannot_move_or_jump_from_input() {
    let mut world = World::dev_stage();
    let id = world.player_id().expect("player");
    assert!(world.set_health(
        id,
        Health {
            current: 0.0,
            max: 20.0,
        },
    ));
    let start = player(&world).position;

    world.tick(DT_30, PlayerInput::from_buttons(false, true, true));

    let body = player(&world);
    assert!((body.position[0] - start[0]).abs() < 1e-4);
    assert_eq!(body.velocity[0], 0.0);
    assert!(body.grounded, "dead player must not jump");
}

#[test]
fn dead_player_keeps_world_physics_without_player_locomotion() {
    let mut world = World::dev_stage();
    let id = world.player_id().expect("player");
    assert!(world.set_health(
        id,
        Health {
            current: 0.0,
            max: 20.0,
        },
    ));
    set_player(&mut world, [-2.0, 2.0], [4.0, 0.0], false, None);

    world.tick(DT_30, PlayerInput::from_buttons(false, true, false));

    let body = player(&world);
    assert!((body.position[0] + 2.0).abs() < 1e-4);
    assert_eq!(body.velocity[0], 0.0);
    assert!(
        body.position[1] < 2.0,
        "dead airborne player must continue falling under gravity"
    );
    assert!(body.velocity[1] < 0.0);
}

#[test]
fn player_cannot_move_beyond_left_world_bound() {
    let mut world = World::footnote_test_stage();
    let min_x = world.bounds().min_x;
    let half = PLAYER_HALF_EXTENTS[0];
    if let Some((t, p)) = world.player_parts_mut() {
        t.position[0] = min_x + half + 0.05;
        p.velocity = [-20.0, 0.0];
        p.grounded = true;
    }
    for _ in 0..10 {
        world.tick(DT_30, PlayerInput::from_buttons(true, false, false));
    }
    let body = player(&world);
    assert!(
        body.position[0] >= min_x + half - 1e-3,
        "x={} below bound",
        body.position[0]
    );
    assert!(body.velocity[0] >= -1e-3);
}

#[test]
fn player_cannot_move_beyond_right_world_bound() {
    let mut world = World::footnote_test_stage();
    let max_x = world.bounds().max_x;
    let half = PLAYER_HALF_EXTENTS[0];
    if let Some((t, p)) = world.player_parts_mut() {
        t.position[0] = max_x - half - 0.05;
        p.velocity = [20.0, 0.0];
        p.grounded = true;
    }
    for _ in 0..10 {
        world.tick(DT_30, PlayerInput::from_buttons(false, true, false));
    }
    let body = player(&world);
    assert!(
        body.position[0] <= max_x - half + 1e-3,
        "x={} above bound",
        body.position[0]
    );
    assert!(body.velocity[0] <= 1e-3);
}

#[test]
fn world_bound_hit_does_not_teleport_player() {
    let mut world = World::footnote_test_stage();
    let min_x = world.bounds().min_x;
    let half = PLAYER_HALF_EXTENTS[0];
    let start = min_x + half + 0.2;
    if let Some((t, p)) = world.player_parts_mut() {
        t.position[0] = start;
        p.velocity = [-6.0, 0.0];
    }
    world.tick(DT_30, PlayerInput::from_buttons(true, false, false));
    let x = player(&world).position[0];
    assert!((x - start).abs() < 1.0, "unexpected teleport dx");
    assert!(x >= min_x + half - 1e-3);
}

/// Regression: ignore must clear once below OneWay top, not wait for floor.
#[test]
fn ignore_clears_below_oneway_top_then_reland_after_ascent() {
    use crate::platform::Platform;

    let mut world = World::new();
    // OneWay A above Solid B (close vertical spacing reproduces the bug).
    let a = world.spawn_platform(
        Transform::from_position([0.0, -1.0]),
        Platform::one_way([1.5, 0.09]),
    );
    let b = world.spawn_platform(
        Transform::from_position([0.0, -2.6]),
        Platform::solid([2.0, 0.25]),
    );
    let top_a = world
        .iter_platforms()
        .find(|v| v.id == a)
        .expect("view A")
        .top_surface();
    let (transform, state) = crate::body::PlayerState::standing_on_at(a, top_a, 0.0);
    world.spawn_player(transform, state);

    // Drop through A.
    world.tick(
        DT_30,
        PlayerInput::from_buttons_ext(false, false, true, true),
    );
    assert_eq!(player(&world).ignored_platform, Some(a));

    // Fall until on Solid B; ignore must be clear without depending on global floor.
    let mut landed_b = false;
    for _ in 0..90 {
        world.tick(DT_30, PlayerInput::idle());
        let body = player(&world);
        if body.grounded_on == Some(b) {
            landed_b = true;
            assert!(
                body.ignored_platform.is_none(),
                "ignored_platform must clear once below A / on landing B"
            );
            break;
        }
        // Once fully below A's top, ignore must already be gone (primary lifecycle).
        let top = body.position[1] + body.half_extents[1];
        if top < top_a {
            assert!(
                body.ignored_platform.is_none(),
                "ignore must clear when player_top < platform_top"
            );
        }
    }
    assert!(landed_b, "expected land on Solid B");
    assert!(player(&world).ignored_platform.is_none());

    // Jump upward through A, then descend and land on A again.
    world.tick(DT_30, PlayerInput::from_buttons(false, false, true));
    drive(&mut world, 45, DT_30, PlayerInput::idle());
    // Nudge above A if needed and fall onto it.
    let body = player(&world);
    if body.position[1] < top_a + 0.5 {
        set_player(&mut world, [0.0, top_a + 1.5], [0.0, 0.0], false, None);
    }
    drive(&mut world, 60, DT_30, PlayerInput::idle());
    let body = player(&world);
    assert_eq!(
        body.grounded_on,
        Some(a),
        "OneWay A must support landing again after ignore expired"
    );
    assert!(body.ignored_platform.is_none());
}

/// Stacked OneWays: drop A → land B; A ignore clears; drop B ignores only B.
#[test]
fn stacked_oneway_drop_creates_fresh_ignore_per_platform() {
    use crate::platform::Platform;

    let mut world = World::new();
    let a = world.spawn_platform(
        Transform::from_position([0.0, 0.5]),
        Platform::one_way([1.4, 0.09]),
    );
    let b = world.spawn_platform(
        Transform::from_position([0.0, -0.5]),
        Platform::one_way([1.4, 0.09]),
    );
    let c = world.spawn_platform(
        Transform::from_position([0.0, -2.4]),
        Platform::solid([2.0, 0.25]),
    );
    let top_a = world
        .iter_platforms()
        .find(|v| v.id == a)
        .expect("A")
        .top_surface();
    let (transform, state) = crate::body::PlayerState::standing_on_at(a, top_a, 0.0);
    world.spawn_player(transform, state);

    world.tick(
        DT_30,
        PlayerInput::from_buttons_ext(false, false, true, true),
    );
    assert_eq!(player(&world).ignored_platform, Some(a));

    let mut landed_b = false;
    for _ in 0..90 {
        world.tick(DT_30, PlayerInput::idle());
        let body = player(&world);
        if body.grounded_on == Some(b) {
            landed_b = true;
            assert!(body.ignored_platform.is_none());
            break;
        }
    }
    assert!(landed_b, "expected land on OneWay B");
    assert_ne!(player(&world).grounded_on, Some(a));

    // Drop through B — new ignore for B only.
    world.tick(
        DT_30,
        PlayerInput::from_buttons_ext(false, false, true, true),
    );
    assert_eq!(player(&world).ignored_platform, Some(b));

    let mut landed_c = false;
    for _ in 0..90 {
        world.tick(DT_30, PlayerInput::idle());
        let body = player(&world);
        if body.grounded_on == Some(c) {
            landed_c = true;
            assert!(body.ignored_platform.is_none());
            break;
        }
    }
    assert!(landed_c, "expected land on Solid C");
}

#[test]
fn idle_grounded_position_is_bit_stable() {
    let mut world = World::dev_stage();
    drive(&mut world, 8, DT_30, PlayerInput::idle());
    let start = player(&world);
    assert!(start.grounded);
    let pos = start.position;
    let vel = start.velocity;
    let revs = world
        .domain_revs_of(world.player_id().expect("player"))
        .expect("revs");
    for _ in 0..120 {
        world.tick(DT_30, PlayerInput::idle());
    }
    let later = player(&world);
    assert_eq!(later.position, pos);
    assert_eq!(later.velocity, vel);
    let later_revs = world
        .domain_revs_of(world.player_id().expect("player"))
        .expect("revs");
    assert_eq!(later_revs.transform, revs.transform);
}
