//! Collision detection and axis response.
//!
//! # Normal collision vs depenetration
//!
//! **Normal collision** fires only when the player travels this tick and
//! **crosses** a blocking surface (previous AABB → proposed AABB). Candidates
//! are gathered without mutating the body; one nearest valid crossed surface is
//! selected, then a single correction is applied.
//!
//! **Recovery** is a separate exceptional path when the player begins a tick
//! already meaningfully inside Solid geometry. It must not run as a fallback
//! inside ordinary horizontal/vertical resolution (Entity-16 underside snap).
//!
//! Detection reports geometric overlap only.
//! Response consults FOOTNOTE [`crate::footnote::surface_blocks`].

use crate::aabb::Aabb;
use crate::body::CollisionBody;
use crate::contact::{
    CONTACT_EPSILON, MAX_RECOVERY_TRANSLATION, RECOVERY_PENETRATION_MIN, overlap_x, overlap_y,
    penetrates,
};
use crate::entity::EntityId;
use crate::footnote::{BlockQuery, surface_blocks};
use crate::platform::{Approach, PlatformKind, PlatformView};
use crate::transform::Transform;

/// Geometric overlap. Detection output only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Overlap {
    pub entity: EntityId,
}

/// True when the body AABB overlaps the platform collider AABB (strict edges).
#[must_use]
pub fn detect_overlap(body: Aabb, platform_aabb: Aabb) -> bool {
    body.overlaps(platform_aabb)
}

/// Overlaps against the supplied colliders. Does not mutate `body`.
pub fn detect_overlaps(
    body: Aabb,
    platforms: &[PlatformView],
) -> impl Iterator<Item = Overlap> + '_ {
    platforms.iter().copied().filter_map(move |platform| {
        detect_overlap(body, platform.aabb()).then_some(Overlap {
            entity: platform.id,
        })
    })
}

/// Result of exceptional start-of-tick Solid penetration recovery.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecoveryResult {
    pub platform: EntityId,
    pub correction: [f32; 2],
}

/// If the player begins the tick meaningfully inside a **Solid**, apply one
/// minimum-translation recovery (shallower axis). Caps magnitude.
///
/// OneWay is ignored. Surface-touch within [`CONTACT_EPSILON`] is not recovery.
pub fn recover_solid_penetration<B: CollisionBody>(
    transform: &mut Transform,
    body_state: &B,
    platforms: impl Iterator<Item = PlatformView>,
) -> Option<RecoveryResult> {
    let body = body_state.aabb(*transform);
    // (id, pen_x, pen_y, push_x, push_y) — push_* already signed toward exit
    let mut best: Option<(EntityId, f32, f32, f32, f32)> = None;

    for platform in platforms {
        if platform.platform.kind != PlatformKind::Solid {
            continue;
        }
        let pa = platform.aabb();
        if !penetrates(body, pa) {
            continue;
        }
        let ox = overlap_x(body, pa);
        let oy = overlap_y(body, pa);
        if ox < RECOVERY_PENETRATION_MIN || oy < RECOVERY_PENETRATION_MIN {
            continue;
        }
        let to_left = pa.min_x() - body.max_x(); // negative
        let to_right = pa.max_x() - body.min_x(); // positive
        let push_x = if to_left.abs() <= to_right.abs() {
            to_left
        } else {
            to_right
        };
        let to_down = pa.min_y() - body.max_y();
        let to_up = pa.max_y() - body.min_y();
        let push_y = if to_down.abs() <= to_up.abs() {
            to_down
        } else {
            to_up
        };
        let depth = ox.min(oy);
        best = Some(match best {
            Some((id, bx, by, px, py)) if bx.min(by) <= depth => (id, bx, by, px, py),
            _ => (platform.id, ox, oy, push_x, push_y),
        });
    }

    let (id, ox, oy, push_x, push_y) = best?;
    let before = transform.position;
    if ox <= oy {
        let mag = push_x.abs().min(MAX_RECOVERY_TRANSLATION);
        transform.position[0] += push_x.signum() * mag;
    } else {
        let mag = push_y.abs().min(MAX_RECOVERY_TRANSLATION);
        transform.position[1] += push_y.signum() * mag;
    }
    let result = RecoveryResult {
        platform: id,
        correction: [
            transform.position[0] - before[0],
            transform.position[1] - before[1],
        ],
    };
    Some(result)
}

/// Separate the player on X against platforms that block this approach.
///
/// **Crossing-only normal collision.** Stale overlaps without a genuine
/// side-face crossing are ignored (no nearest-face shove).
///
/// Returns `(candidate, correction_delta_x)` when a wall response was applied.
pub fn resolve_horizontal<B: CollisionBody>(
    transform: &mut Transform,
    body_state: &mut B,
    platforms: impl Iterator<Item = PlatformView>,
    previous_bottom: f32,
    previous_left: f32,
    previous_right: f32,
) -> Option<(EntityId, f32)> {
    let body = body_state.aabb(*transform);
    let vx = body_state.velocity()[0];
    if vx == 0.0 {
        return None;
    }
    let approach = Approach::from_horizontal_velocity(vx);
    let half = body_state.half_extents()[0];
    let eps = CONTACT_EPSILON;

    let mut best_cross: Option<(f32, EntityId)> = None;

    for platform in platforms {
        if !detect_overlap(body, platform.aabb()) {
            continue;
        }
        let query = BlockQuery {
            approach,
            platform_id: platform.id,
            previous_bottom,
            platform_top: platform.top_surface(),
            ignored_platform: body_state.ignored_platform(),
        };
        if !surface_blocks(platform.platform, query) {
            continue;
        }
        let min_x = platform.platform.min_x(platform.transform);
        let max_x = platform.platform.max_x(platform.transform);
        if vx > 0.0 {
            if previous_right <= min_x + eps && body.max_x() > min_x + eps {
                best_cross = Some(match best_cross {
                    Some((e, id)) if e <= min_x => (e, id),
                    _ => (min_x, platform.id),
                });
            }
        } else if previous_left >= max_x - eps && body.min_x() < max_x - eps {
            best_cross = Some(match best_cross {
                Some((e, id)) if e >= max_x => (e, id),
                _ => (max_x, platform.id),
            });
        }
    }

    let before = transform.position[0];
    if let Some((edge, id)) = best_cross {
        if vx > 0.0 {
            transform.position[0] = edge - half;
        } else {
            transform.position[0] = edge + half;
        }
        let mut velocity = body_state.velocity();
        velocity[0] = 0.0;
        body_state.set_velocity(velocity);
        Some((id, transform.position[0] - before))
    } else {
        None
    }
}

/// Result of one vertical resolution step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalContact {
    None,
    /// Feet crossed onto a blocking top.
    Land(EntityId),
    /// Head crossed into a blocking underside (not a support).
    Ceiling(EntityId),
}

impl VerticalContact {
    #[must_use]
    pub fn platform(self) -> Option<EntityId> {
        match self {
            Self::None => None,
            Self::Land(id) | Self::Ceiling(id) => Some(id),
        }
    }

    #[must_use]
    pub fn landing(self) -> Option<EntityId> {
        match self {
            Self::Land(id) => Some(id),
            _ => None,
        }
    }
}

/// Separate the player on Y against platforms that block this approach.
///
/// Returns `(contact, correction_delta_y)`. Landing sets [`VerticalContact::Land`];
/// ceiling hits set [`VerticalContact::Ceiling`] and must not ground the player.
///
/// Overlapping candidates are not resolved sequentially. One valid surface is
/// chosen: first top crossed from above when falling, or nearest underside
/// crossed from below when rising.
pub fn resolve_vertical<B: CollisionBody>(
    transform: &mut Transform,
    body_state: &mut B,
    platforms: impl Iterator<Item = PlatformView>,
    previous_bottom: f32,
    previous_top: f32,
) -> (VerticalContact, f32) {
    let body = body_state.aabb(*transform);
    let vy = body_state.velocity()[1];

    if vy > 0.0 {
        resolve_upward(
            transform,
            body_state,
            platforms,
            body,
            previous_bottom,
            previous_top,
        )
    } else if vy < 0.0 {
        let (landed, dy) =
            resolve_downward(transform, body_state, platforms, body, previous_bottom);
        (
            landed.map_or(VerticalContact::None, VerticalContact::Land),
            dy,
        )
    } else {
        (VerticalContact::None, 0.0)
    }
}

fn resolve_upward<B: CollisionBody>(
    transform: &mut Transform,
    body_state: &mut B,
    platforms: impl Iterator<Item = PlatformView>,
    body: Aabb,
    previous_bottom: f32,
    previous_top: f32,
) -> (VerticalContact, f32) {
    let current_top = body.center[1] + body_state.half_extents()[1];
    let mut best: Option<(f32, EntityId)> = None;

    for platform in platforms {
        if !detect_overlap(body, platform.aabb()) {
            continue;
        }
        let underside = platform.platform.min_y(platform.transform);
        let query = BlockQuery {
            approach: Approach::Up,
            platform_id: platform.id,
            previous_bottom,
            platform_top: platform.top_surface(),
            ignored_platform: body_state.ignored_platform(),
        };
        if !surface_blocks(platform.platform, query) {
            continue;
        }
        if previous_top > underside + CONTACT_EPSILON {
            continue;
        }
        if current_top < underside - CONTACT_EPSILON {
            continue;
        }
        best = Some(match best {
            Some((u, id)) if u <= underside => (u, id),
            _ => (underside, platform.id),
        });
    }

    if let Some((underside, id)) = best {
        let before = transform.position[1];
        // Separate slightly below the underside so flush contact is not
        // reported as AABB penetration on the next X phase (float noise).
        transform.position[1] = underside - body_state.half_extents()[1] - CONTACT_EPSILON;
        let mut velocity = body_state.velocity();
        velocity[1] = 0.0;
        body_state.set_velocity(velocity);
        (VerticalContact::Ceiling(id), transform.position[1] - before)
    } else {
        (VerticalContact::None, 0.0)
    }
}

fn resolve_downward<B: CollisionBody>(
    transform: &mut Transform,
    body_state: &mut B,
    platforms: impl Iterator<Item = PlatformView>,
    body: Aabb,
    previous_bottom: f32,
) -> (Option<EntityId>, f32) {
    let current_bottom = body.center[1] - body_state.half_extents()[1];
    let mut best: Option<(f32, EntityId)> = None;

    for platform in platforms {
        if !detect_overlap(body, platform.aabb()) {
            continue;
        }
        let top = platform.top_surface();
        let query = BlockQuery {
            approach: Approach::Down,
            platform_id: platform.id,
            previous_bottom,
            platform_top: top,
            ignored_platform: body_state.ignored_platform(),
        };
        if !surface_blocks(platform.platform, query) {
            continue;
        }
        if previous_bottom < top - CONTACT_EPSILON {
            continue;
        }
        if current_bottom > top + CONTACT_EPSILON {
            continue;
        }
        best = Some(match best {
            Some((best_top, best_id)) if best_top >= top => (best_top, best_id),
            _ => (top, platform.id),
        });
    }

    if let Some((top, id)) = best {
        let before = transform.position[1];
        transform.position[1] = top + body_state.half_extents()[1];
        let mut velocity = body_state.velocity();
        velocity[1] = 0.0;
        body_state.set_velocity(velocity);
        (Some(id), transform.position[1] - before)
    } else {
        (None, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::{PLAYER_HALF_EXTENTS, PlayerState};
    use crate::input::PlayerInput;
    use crate::motion_debug::{CorrectionAxis, ResponseKind};
    use crate::platform::{Platform, RAISED_PLATFORM, RAISED_PLATFORM_POSITION};
    use crate::world::World;

    const DT: f32 = 1.0 / 30.0;

    #[test]
    fn detection_does_not_mutate_the_body() {
        let world = World::dev_stage();
        let mut player = world.player_body().expect("player");
        player.position[1] -= 0.05;
        let before = player;
        let platforms: Vec<_> = world.iter_platforms().collect();
        let hits: Vec<_> = detect_overlaps(player.aabb(), &platforms).collect();
        assert_eq!(player, before);
        assert!(
            hits.iter()
                .any(|hit| world.kind(hit.entity) == Some(crate::entity::EntityKind::Platform))
        );
    }

    #[test]
    fn overlap_is_not_automatically_a_collision() {
        let world = World::dev_stage();
        let player = world.player_body().expect("player");
        let raised = Transform::from_position(RAISED_PLATFORM_POSITION);
        let raised_aabb = RAISED_PLATFORM.aabb(raised);
        assert!(
            !detect_overlap(player.aabb(), raised_aabb),
            "spawned player must not geometrically overlap the raised platform"
        );
    }

    fn overlap_world_two_solids() -> (World, EntityId, EntityId) {
        let mut world = World::new();
        let a = world.spawn_platform(
            Transform::from_position([0.0, 0.0]),
            Platform::solid([1.5, 0.4]),
        );
        let b = world.spawn_platform(
            Transform::from_position([0.4, 0.35]),
            Platform::solid([1.5, 0.5]),
        );
        let top_a = world
            .iter_platforms()
            .find(|v| v.id == a)
            .expect("A")
            .top_surface();
        let (transform, state) = PlayerState::standing_on_at(a, top_a, 0.0);
        world.spawn_player(transform, state);
        (world, a, b)
    }

    #[test]
    fn overlapping_solids_jump_does_not_teleport() {
        let (mut world, _a, _b) = overlap_world_two_solids();
        let y0 = world.player_body().expect("p").position[1];
        world.tick(DT, PlayerInput::from_buttons(false, false, true));
        let body = world.player_body().expect("p");
        // Must leave ground with upward velocity — not snap to another top.
        assert!(body.velocity[1] > 0.0, "vy={}", body.velocity[1]);
        assert!(!body.grounded);
        assert!(
            body.position[1] > y0 - 0.05,
            "unexpected downward snap: y0={y0} y={}",
            body.position[1]
        );
        // Continue a few ticks: no discontinuous teleport (dy per tick bounded).
        let mut prev_y = body.position[1];
        for _ in 0..8 {
            world.tick(DT, PlayerInput::idle());
            let y = world.player_body().expect("p").position[1];
            let dy = (y - prev_y).abs();
            assert!(dy < 1.2, "teleport-like dy={dy} from {prev_y} to {y}");
            prev_y = y;
        }
    }

    #[test]
    fn solid_oneway_overlap_jump_is_not_blocked_by_oneway() {
        let mut world = World::new();
        let solid = world.spawn_platform(
            Transform::from_position([0.0, 0.0]),
            Platform::solid([1.5, 0.35]),
        );
        let _ow = world.spawn_platform(
            Transform::from_position([0.2, 0.25]),
            Platform::one_way([1.5, 0.2]),
        );
        let top = world
            .iter_platforms()
            .find(|v| v.id == solid)
            .expect("solid")
            .top_surface();
        let (transform, state) = PlayerState::standing_on_at(solid, top, 0.0);
        world.spawn_player(transform, state);

        let y0 = world.player_body().expect("p").position[1];
        world.tick(DT, PlayerInput::from_buttons(false, false, true));
        let body = world.player_body().expect("p");
        assert!(body.velocity[1] > 0.0);
        assert!(body.position[1] >= y0 - 0.05);
        // Rise further through the OneWay region.
        for _ in 0..10 {
            world.tick(DT, PlayerInput::idle());
        }
        let body = world.player_body().expect("p");
        // Either still rising/falling normally or landed back — never stuck mid-snap.
        assert!(body.position[1].is_finite());
    }

    #[test]
    fn descending_picks_highest_crossed_top_order_independent() {
        let mut world_ab = World::new();
        let low = world_ab.spawn_platform(
            Transform::from_position([0.0, -1.0]),
            Platform::solid([2.0, 0.2]),
        );
        let high = world_ab.spawn_platform(
            Transform::from_position([0.0, -0.5]),
            Platform::solid([2.0, 0.2]),
        );
        // Player above both, falling.
        let (mut t, mut s) = PlayerState::standing_on_at(
            high,
            world_ab
                .iter_platforms()
                .find(|v| v.id == high)
                .unwrap()
                .top_surface(),
            0.0,
        );
        t.position = [0.0, 1.5];
        s.grounded = false;
        s.grounded_on = None;
        s.velocity = [0.0, -8.0];
        world_ab.spawn_player(t, s);

        let mut world_ba = World::new();
        // Reverse spawn order (iteration order differs).
        let high2 = world_ba.spawn_platform(
            Transform::from_position([0.0, -0.5]),
            Platform::solid([2.0, 0.2]),
        );
        let _low2 = world_ba.spawn_platform(
            Transform::from_position([0.0, -1.0]),
            Platform::solid([2.0, 0.2]),
        );
        let (mut t2, mut s2) = PlayerState::standing_on_at(
            high2,
            world_ba
                .iter_platforms()
                .find(|v| v.id == high2)
                .unwrap()
                .top_surface(),
            0.0,
        );
        t2.position = [0.0, 1.5];
        s2.grounded = false;
        s2.grounded_on = None;
        s2.velocity = [0.0, -8.0];
        world_ba.spawn_player(t2, s2);

        for _ in 0..40 {
            world_ab.tick(DT, PlayerInput::idle());
            world_ba.tick(DT, PlayerInput::idle());
        }
        let a = world_ab.player_body().expect("a");
        let b = world_ba.player_body().expect("b");
        assert!(a.grounded && b.grounded);
        // Both must land on the higher top, not the lower one.
        let top_high = -0.5 + 0.2;
        assert!((a.position[1] - (top_high + PLAYER_HALF_EXTENTS[1])).abs() < 0.05);
        assert!((b.position[1] - (top_high + PLAYER_HALF_EXTENTS[1])).abs() < 0.05);
        assert_eq!(a.grounded_on.map(|id| id.index()), Some(high.index()));
        assert_eq!(b.grounded_on.map(|id| id.index()), Some(high2.index()));
        let _ = low;
    }

    #[test]
    fn stale_overlap_jump_does_not_snap_to_foreign_top() {
        let mut world = World::new();
        let floor = world.spawn_platform(
            Transform::from_position([0.0, -2.0]),
            Platform::solid([3.0, 0.3]),
        );
        let floating = world.spawn_platform(
            Transform::from_position([0.0, -0.5]),
            Platform::solid([1.2, 0.8]),
        );
        // Place player so they already penetrate `floating` while standing on floor.
        let floor_top = world
            .iter_platforms()
            .find(|v| v.id == floor)
            .unwrap()
            .top_surface();
        let (mut transform, mut state) = PlayerState::standing_on_at(floor, floor_top, 0.0);
        // Nudge into the floating solid's AABB without being a valid landing on it.
        transform.position[1] = -0.3;
        state.grounded = true;
        state.grounded_on = Some(floor);
        world.spawn_player(transform, state);

        let y0 = world.player_body().expect("p").position[1];
        let float_top = world
            .iter_platforms()
            .find(|v| v.id == floating)
            .unwrap()
            .top_surface();
        world.tick(DT, PlayerInput::from_buttons(false, false, true));
        let body = world.player_body().expect("p");
        let snapped_to_float = (body.position[1] - (float_top + PLAYER_HALF_EXTENTS[1])).abs()
            < 0.05
            && body.grounded_on == Some(floating);
        assert!(
            !snapped_to_float,
            "stale overlap must not become a landing on floating solid"
        );
        assert!(
            body.velocity[1] > 0.0 || body.position[1] >= y0 - 0.1,
            "jump must not collapse into a downward teleport"
        );
    }

    #[test]
    fn stale_horizontal_overlap_without_crossing_does_not_shove() {
        // Player already overlaps a Solid (underside / slope clip) and moves
        // right — must NOT nearest-face teleport; no X face was crossed.
        let mut world = World::new();
        let floor = world.spawn_platform(
            Transform::from_position([0.0, -2.0]),
            Platform::solid([6.0, 0.3]),
        );
        let step = world.spawn_platform(
            Transform::from_position([-0.5, -1.1]),
            Platform::solid([0.7, 0.16]),
        );
        let floor_top = world
            .iter_platforms()
            .find(|v| v.id == floor)
            .unwrap()
            .top_surface();
        let (transform, state) = PlayerState::standing_on_at(floor, floor_top, 0.0);
        world.spawn_player(transform, state);

        let step_view = world.iter_platforms().find(|v| v.id == step).unwrap();
        let body0 = world.player_body().expect("p");
        assert!(
            detect_overlap(body0.aabb(), step_view.aabb()),
            "fixture must start overlapping the step"
        );
        let x0 = body0.position[0];
        let step_min_x = step_view.platform.min_x(step_view.transform);

        world.tick(DT, PlayerInput::from_buttons(false, true, false));
        let body = world.player_body().expect("p");
        let motion = world.last_motion_debug();
        // Far-face teleport would place the player left of the step's min_x.
        assert!(
            body.position[0] > step_min_x - 0.05,
            "teleported toward far face: x0={x0} x={} step_min={step_min_x}",
            body.position[0]
        );
        assert!(
            motion.correction[0].abs() < 0.05,
            "stale overlap must not produce ordinary horizontal wall correction: {motion:?}"
        );
    }

    #[test]
    fn entity16_underside_head_bonk_has_zero_horizontal_correction() {
        // Freestyle Solid geometry matching footnote Entity 16.
        let mut world = World::new();
        world.set_bounds(crate::bounds::WorldBounds::FOOTNOTE_TEST);
        let floor = world.spawn_platform(
            Transform::from_position([16.0, -4.0]),
            Platform::solid([8.0, 0.4]),
        );
        let ceil = world.spawn_platform(
            Transform::from_position([16.0, -1.8]),
            Platform::solid([1.4, 0.2]),
        );
        let floor_top = world
            .iter_platforms()
            .find(|v| v.id == floor)
            .unwrap()
            .top_surface();
        let (transform, state) = PlayerState::standing_on_at(floor, floor_top, 16.0);
        world.spawn_player(transform, state);

        let left_snap = 14.6 - PLAYER_HALF_EXTENTS[0];
        let right_snap = 17.4 + PLAYER_HALF_EXTENTS[0];

        world.tick(DT, PlayerInput::from_buttons(false, true, true));
        let mut saw_ceiling = false;
        for _ in 0..45 {
            world.tick(
                DT,
                PlayerInput::from_buttons(false, true, false).with_jump_held(true),
            );
            let motion = world.last_motion_debug();
            let body = world.player_body().expect("p");
            if motion.correction_axis == CorrectionAxis::Vertical
                && motion.correction[1] < -0.01
                && motion.collision_candidate == Some(ceil)
            {
                saw_ceiling = true;
                assert!(
                    motion.correction[0].abs() < 1e-4,
                    "head-bonk must not apply horizontal correction: {motion:?}"
                );
                assert!(
                    (body.position[0] - left_snap).abs() > 0.05
                        && (body.position[0] - right_snap).abs() > 0.05,
                    "must not teleport to Entity-16 faces: x={}",
                    body.position[0]
                );
                assert!(body.velocity[0] > 0.0, "horizontal momentum must survive");
                assert!(!body.grounded);
            }
            assert!(
                !motion.discontinuity || motion.response_kind == ResponseKind::Recovery,
                "unexpected discontinuity: {motion:?}"
            );
        }
        assert!(
            saw_ceiling,
            "expected underside contact with freestyle Solid"
        );
    }

    #[test]
    fn jump_head_bonk_with_momentum_stays_continuous() {
        let mut world = World::new();
        let floor = world.spawn_platform(
            Transform::from_position([0.0, -2.0]),
            Platform::solid([4.0, 0.3]),
        );
        let ceiling = world.spawn_platform(
            Transform::from_position([0.5, 1.0]),
            Platform::solid([2.0, 0.25]),
        );
        let floor_top = world
            .iter_platforms()
            .find(|v| v.id == floor)
            .unwrap()
            .top_surface();
        let ceil = world.iter_platforms().find(|v| v.id == ceiling).unwrap();
        let underside = ceil.platform.min_y(ceil.transform);
        let (transform, state) = PlayerState::standing_on_at(floor, floor_top, 0.0);
        world.spawn_player(transform, state);

        world.tick(DT, PlayerInput::from_buttons(false, true, true));
        let mut saw_ceiling = false;
        for _ in 0..40 {
            world.tick(
                DT,
                PlayerInput::from_buttons(false, true, false).with_jump_held(true),
            );
            let motion = world.last_motion_debug();
            assert!(
                !motion.discontinuity,
                "ceiling/momentum tick discontinuous: {motion:?}"
            );
            assert!(
                motion.correction[0].abs() < 0.05
                    || motion.correction_axis != CorrectionAxis::Horizontal,
                "no horizontal shove on head-bonk: {motion:?}"
            );
            let body = world.player_body().expect("p");
            let top = body.position[1] + PLAYER_HALF_EXTENTS[1];
            if motion.correction[1] < -0.02 && (top - underside).abs() < 0.05 {
                saw_ceiling = true;
                assert!(!body.grounded, "ceiling must not ground the player");
                assert!(body.grounded_on.is_none());
                assert!(
                    body.velocity[0] > 0.0,
                    "head bonk must not zero horizontal momentum without a wall cross"
                );
            }
        }
        assert!(
            saw_ceiling,
            "expected to hit the solid underside while rising"
        );
    }

    #[test]
    fn fresh_wall_crossing_still_blocks() {
        let mut world = World::new();
        let floor = world.spawn_platform(
            Transform::from_position([0.0, -2.0]),
            Platform::solid([8.0, 0.3]),
        );
        let wall = world.spawn_platform(
            Transform::from_position([3.0, -0.8]),
            Platform::solid([0.4, 1.5]),
        );
        let floor_top = world
            .iter_platforms()
            .find(|v| v.id == floor)
            .unwrap()
            .top_surface();
        let (transform, state) = PlayerState::standing_on_at(floor, floor_top, 0.0);
        world.spawn_player(transform, state);

        let wall_left = world
            .iter_platforms()
            .find(|v| v.id == wall)
            .unwrap()
            .platform
            .min_x(
                world
                    .iter_platforms()
                    .find(|v| v.id == wall)
                    .unwrap()
                    .transform,
            );
        for _ in 0..90 {
            world.tick(DT, PlayerInput::from_buttons(false, true, false));
        }
        let body = world.player_body().expect("p");
        let right = body.position[0] + PLAYER_HALF_EXTENTS[0];
        assert!(
            right <= wall_left + 0.05,
            "player must stop at wall face: right={right} wall_left={wall_left}"
        );
    }
}
