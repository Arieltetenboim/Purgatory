//! Hard-coded FOOTNOTE development test arena (Phase 4.8).
//!
//! Horizontal extent ≈ 2× the Phase-4.6 map via wider floor + redistributed
//! regions and empty traversal space — not by doubling platform count.
//! Labels in comments are developer-only. Not a map loader.

use crate::aabb::Aabb;
use crate::body::PlayerState;
use crate::bounds::WorldBounds;
use crate::contact::{CONTACT_EPSILON, overlap_x, overlap_y, penetrates};
use crate::entity::EntityId;
use crate::platform::{Platform, PlatformKind};
use crate::transform::Transform;
use crate::world::World;

/// Main Solid floor (P0) spanning full horizontal world bounds.
pub const P0: Platform = Platform::solid([24.0, 0.4]);
pub const P0_POSITION: [f32; 2] = [0.0, -4.0];

/// Player spawn X on P0 (far left, near left-boundary test).
///
/// Must sit to the right of the middle slope step (`[-20.5, -2.5]`, half
/// `[0.7, 0.16]`) so a standing player AABB does not embed in Solid geometry.
pub const FOOTNOTE_SPAWN_X: f32 = -19.4;

/// Former spawn X that stood inside the middle slope step (Entity 22).
/// Fixture / regression only. Not used by [`World::footnote_test_stage`].
#[cfg(test)]
const LEGACY_INVALID_FOOTNOTE_SPAWN_X: f32 = -20.0;

/// Suggested client logical viewport height for this arena.
pub const FOOTNOTE_TEST_VIEWPORT_HEIGHT: f32 = 14.0;

impl World {
    /// Phase-4.8 FOOTNOTE movement laboratory with wide bounds for camera tests.
    #[must_use]
    pub fn footnote_test_stage() -> Self {
        let mut world = Self::new();
        world.set_bounds(WorldBounds::FOOTNOTE_TEST);

        // P0 — full-width Solid floor (left/right boundary walk)
        let p0 = world.spawn_platform(Transform::from_position(P0_POSITION), P0);

        // --- Main test flow (left-center) ---
        // P1 — elevated Solid
        world.spawn_platform(
            Transform::from_position([-10.0, -1.9]),
            Platform::solid([1.5, 0.3]),
        );
        // P2 — central OneWay
        world.spawn_platform(
            Transform::from_position([-4.0, -1.8]),
            Platform::one_way([1.6, 0.09]),
        );
        // P3 — lower OneWay
        world.spawn_platform(
            Transform::from_position([0.5, -2.2]),
            Platform::one_way([1.5, 0.09]),
        );
        // P4 — upper OneWay
        world.spawn_platform(
            Transform::from_position([0.5, -0.9]),
            Platform::one_way([1.5, 0.09]),
        );

        // --- Descent zig-zag (far left) ---
        world.spawn_platform(
            Transform::from_position([-18.5, 2.8]),
            Platform::one_way([1.35, 0.09]),
        );
        world.spawn_platform(
            Transform::from_position([-16.3, 1.9]),
            Platform::one_way([1.35, 0.09]),
        );
        world.spawn_platform(
            Transform::from_position([-18.5, 1.0]),
            Platform::one_way([1.35, 0.09]),
        );
        world.spawn_platform(
            Transform::from_position([-16.3, 0.1]),
            Platform::one_way([1.35, 0.09]),
        );
        // LP
        world.spawn_platform(
            Transform::from_position([-17.4, -1.4]),
            Platform::solid([2.2, 0.25]),
        );

        // --- Multi-drop stack (mid-right) ---
        world.spawn_platform(
            Transform::from_position([10.0, 1.6]),
            Platform::one_way([1.2, 0.09]),
        );
        world.spawn_platform(
            Transform::from_position([10.0, 0.7]),
            Platform::one_way([1.2, 0.09]),
        );
        world.spawn_platform(
            Transform::from_position([10.0, -0.2]),
            Platform::one_way([1.2, 0.09]),
        );

        // --- Momentum / edge (upper mid) ---
        world.spawn_platform(
            Transform::from_position([-6.0, 4.0]),
            Platform::solid([3.2, 0.22]),
        );
        world.spawn_platform(
            Transform::from_position([-1.0, 4.0]),
            Platform::solid([1.1, 0.22]),
        );
        world.spawn_platform(
            Transform::from_position([-3.2, 2.8]),
            Platform::one_way([1.6, 0.09]),
        );

        // --- Freestyle (far right, spaced) ---
        world.spawn_platform(
            Transform::from_position([16.0, -1.8]),
            Platform::solid([1.4, 0.2]),
        );
        world.spawn_platform(
            Transform::from_position([18.0, -0.5]),
            Platform::one_way([1.1, 0.09]),
        );
        world.spawn_platform(
            Transform::from_position([15.5, 0.8]),
            Platform::solid([1.0, 0.18]),
        );
        world.spawn_platform(
            Transform::from_position([18.5, 1.8]),
            Platform::one_way([1.0, 0.09]),
        );
        world.spawn_platform(
            Transform::from_position([16.5, 3.0]),
            Platform::one_way([1.2, 0.09]),
        );

        // --- Slope approximation (near left bound) ---
        world.spawn_platform(
            Transform::from_position([-21.5, -3.0]),
            Platform::solid([0.7, 0.16]),
        );
        world.spawn_platform(
            Transform::from_position([-20.5, -2.5]),
            Platform::solid([0.7, 0.16]),
        );
        world.spawn_platform(
            Transform::from_position([-19.5, -2.0]),
            Platform::solid([0.7, 0.16]),
        );

        // --- OV Overlap Regression (isolated mid) ---
        world.spawn_platform(
            Transform::from_position([5.5, 2.2]),
            Platform::solid([1.3, 0.45]),
        );
        world.spawn_platform(
            Transform::from_position([5.9, 2.55]),
            Platform::solid([1.3, 0.5]),
        );

        let floor_top = P0.top_surface(Transform::from_position(P0_POSITION));
        let (transform, player) = PlayerState::standing_on_at(p0, floor_top, FOOTNOTE_SPAWN_X);
        world.spawn_player(transform, player);
        #[cfg(debug_assertions)]
        if let Some(embed) = first_solid_embed(&world) {
            panic!(
                "FOOTNOTE spawn embeds in Solid {embed:?}; normal spawn must not depend on penetration recovery (CONTACT_EPSILON={CONTACT_EPSILON})"
            );
        }
        world
    }
}

/// First Solid the player AABB meaningfully penetrates (`penetrates` / both axes
/// deeper than [`CONTACT_EPSILON`]). Construction-time check only.
#[cfg(any(debug_assertions, test))]
fn first_solid_embed(world: &World) -> Option<(EntityId, [f32; 2], f32, f32)> {
    let body = world.player_body()?.aabb();
    first_solid_embed_of(world, body)
}

#[cfg(any(debug_assertions, test))]
fn first_solid_embed_of(world: &World, body: Aabb) -> Option<(EntityId, [f32; 2], f32, f32)> {
    for view in world.iter_platforms() {
        if view.platform.kind != PlatformKind::Solid {
            continue;
        }
        let pa = view.aabb();
        if penetrates(body, pa) {
            return Some((
                view.id,
                view.transform.position,
                overlap_x(body, pa),
                overlap_y(body, pa),
            ));
        }
    }
    None
}

#[cfg(test)]
fn standing_on_p0_aabb(spawn_x: f32) -> Aabb {
    use crate::body::PLAYER_HALF_EXTENTS;
    let floor_top = P0.top_surface(Transform::from_position(P0_POSITION));
    Aabb::new(
        [spawn_x, floor_top + PLAYER_HALF_EXTENTS[1]],
        PLAYER_HALF_EXTENTS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DebugAction;
    use crate::platform::PlatformKind;

    #[test]
    fn footnote_test_stage_builds() {
        let world = World::footnote_test_stage();
        assert!(world.len() >= 20);
        assert!(world.len() <= 32);
        assert!(world.player_id().is_some());
        assert_eq!(world.bounds(), WorldBounds::FOOTNOTE_TEST);
        let mut solids = 0u32;
        let mut oneways = 0u32;
        for view in world.iter_platforms() {
            assert!(view.platform.half_extents[0] > 0.0);
            assert!(view.platform.half_extents[1] > 0.0);
            match view.platform.kind {
                PlatformKind::Solid => solids += 1,
                PlatformKind::OneWay => oneways += 1,
            }
        }
        assert!(solids >= 6, "solids={solids}");
        assert!(oneways >= 8, "oneways={oneways}");
    }

    #[test]
    fn footnote_stage_is_about_twice_prior_horizontal_extent() {
        let b = World::footnote_test_stage().bounds();
        // Phase 4.6 span was ~26; Phase 4.8 targets ~48.
        assert!((b.width() / 26.0 - 1.85).abs() < 0.3 || b.width() >= 45.0);
    }

    #[test]
    fn footnote_stage_has_isolated_overlap_regression_pair() {
        let world = World::footnote_test_stage();
        let ov: Vec<_> = world
            .iter_platforms()
            .filter(|v| {
                v.platform.kind == PlatformKind::Solid
                    && (v.transform.position[0] - 5.5).abs() < 1.0
                    && v.transform.position[1] > 1.5
                    && v.transform.position[1] < 3.2
            })
            .collect();
        assert!(ov.len() >= 2, "overlap regression Solids missing");
        assert!(ov[0].aabb().overlaps(ov[1].aabb()));
    }

    fn solid_at(world: &World, position: [f32; 2]) -> crate::platform::PlatformView {
        world
            .iter_platforms()
            .find(|view| {
                view.platform.kind == PlatformKind::Solid
                    && (view.transform.position[0] - position[0]).abs() < 1e-4
                    && (view.transform.position[1] - position[1]).abs() < 1e-4
            })
            .unwrap_or_else(|| panic!("missing Solid at {position:?}"))
    }

    #[test]
    fn current_dev_spawn_is_not_inside_solid() {
        let world = World::footnote_test_stage();
        assert!(
            (world.player_body().expect("player").position[0] - FOOTNOTE_SPAWN_X).abs() < 1e-4,
            "test must use the live FOOTNOTE spawn X"
        );
        assert!(
            first_solid_embed(&world).is_none(),
            "current dev spawn meaningfully penetrates a Solid: {:?}",
            first_solid_embed(&world)
        );
        let p0 = solid_at(&world, P0_POSITION);
        let step = solid_at(&world, [-20.5, -2.5]);
        let aabb = world.player_body().expect("player").aabb();
        assert!(
            !penetrates(aabb, p0.aabb()),
            "spawn must not penetrate P0; touch within CONTACT_EPSILON={CONTACT_EPSILON} is allowed"
        );
        assert!(
            !penetrates(aabb, step.aabb()),
            "spawn must not penetrate the middle slope step"
        );
    }

    #[test]
    fn old_invalid_spawn_is_detected_as_invalid() {
        let world = World::footnote_test_stage();
        let step = solid_at(&world, [-20.5, -2.5]);
        let legacy = standing_on_p0_aabb(LEGACY_INVALID_FOOTNOTE_SPAWN_X);
        let embed = first_solid_embed_of(&world, legacy)
            .expect("legacy spawn X=-20 must be detected as Solid penetration");
        assert!(
            (embed.1[0] + 20.5).abs() < 1e-4 && (embed.1[1] + 2.5).abs() < 1e-4,
            "legacy embed should be the middle slope step, got {embed:?}"
        );
        assert!(
            embed.2 > CONTACT_EPSILON && embed.3 > CONTACT_EPSILON,
            "legacy embed must be deeper than CONTACT_EPSILON on both axes: {embed:?}"
        );
        assert!(
            penetrates(legacy, step.aabb()),
            "legacy spawn must remain an unacceptable normal-player spawn against the slope step"
        );
        assert!(
            first_solid_embed(&world).is_none(),
            "the live map spawn must stay valid; this fixture is not the current spawn"
        );
    }

    #[test]
    fn footnote_spawn_is_on_main_floor() {
        let world = World::footnote_test_stage();
        let body = world.player_body().expect("player");
        assert!((body.position[0] - FOOTNOTE_SPAWN_X).abs() < 1e-3);
        assert!(body.grounded);
        assert!(body.ignored_platform.is_none());
    }

    #[test]
    fn footnote_spawn_idle_does_not_enter_recovery() {
        use crate::input::PlayerInput;
        use crate::motion_debug::ResponseKind;
        let mut world = World::footnote_test_stage();
        let spawn = world.player_body().expect("player").position;
        for _ in 0..30 {
            world.tick(1.0 / 30.0, PlayerInput::idle());
            let m = world.last_motion_debug();
            assert_ne!(
                m.response_kind,
                ResponseKind::Recovery,
                "spawn idle must not recover: {m:?}"
            );
        }
        let body = world.player_body().expect("player");
        assert!((body.position[0] - spawn[0]).abs() < 1e-3);
        assert!((body.position[1] - spawn[1]).abs() < 1e-3);
        let m = world.last_motion_debug();
        assert_eq!(m.response_kind, ResponseKind::None);
    }

    #[test]
    fn footnote_valid_spawn_walk_into_slope_does_not_recover() {
        use crate::input::PlayerInput;
        use crate::motion_debug::ResponseKind;
        let mut world = World::footnote_test_stage();
        let mut recovered = 0u32;
        // Walk into the staircase, jump against it, then walk away.
        for i in 0..180 {
            let input = if i < 60 {
                PlayerInput::from_buttons(true, false, false)
            } else if i == 60 {
                PlayerInput::from_buttons(true, false, true)
            } else if i < 90 {
                PlayerInput::from_buttons(true, false, false)
            } else {
                PlayerInput::from_buttons(false, true, false)
            };
            world.tick(1.0 / 30.0, input);
            let m = world.last_motion_debug();
            if m.response_kind == ResponseKind::Recovery {
                recovered += 1;
            }
            let body = world.player_body().expect("player");
            assert!(
                body.position[1] > -6.5,
                "player fell toward world bound during slope approach: {body:?}"
            );
        }
        assert_eq!(
            recovered, 0,
            "valid spawn gameplay must not enter Solid recovery against the slope"
        );
    }

    #[test]
    fn reset_player_on_footnote_stage() {
        let mut world = World::footnote_test_stage();
        let id = world.player_id().expect("player");
        let spawn = world.player_body().expect("player").position;
        if let Some((t, p)) = world.player_parts_mut() {
            t.position = [5.0, 2.0];
            p.velocity = [3.0, -1.0];
            p.ignored_platform = Some(id);
            p.grounded = false;
            p.grounded_on = None;
        }
        world.apply_debug_action(DebugAction::ResetPlayer);
        let body = world.player_body().expect("player");
        assert_eq!(body.id, id);
        assert!((body.position[0] - spawn[0]).abs() < 1e-3);
        assert!((body.position[1] - spawn[1]).abs() < 1e-3);
        assert_eq!(body.velocity, [0.0, 0.0]);
        assert!(body.grounded);
        assert!(body.ignored_platform.is_none());
    }
}
