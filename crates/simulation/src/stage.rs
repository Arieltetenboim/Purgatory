//! Hard-coded FOOTNOTE development test arena (Phase 4.8).
//!
//! Horizontal extent ≈ 2× the Phase-4.6 map via wider floor + redistributed
//! regions and empty traversal space — not by doubling platform count.
//! Labels in comments are developer-only. Not a map loader.

use crate::body::PlayerState;
use crate::bounds::WorldBounds;
use crate::platform::Platform;
use crate::transform::Transform;
use crate::world::World;

/// Main Solid floor (P0) spanning full horizontal world bounds.
pub const P0: Platform = Platform::solid([24.0, 0.4]);
pub const P0_POSITION: [f32; 2] = [0.0, -4.0];

/// Player spawn X on P0 (far left, near left-boundary test).
pub const FOOTNOTE_SPAWN_X: f32 = -20.0;

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
        world
    }
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

    #[test]
    fn footnote_spawn_is_on_main_floor() {
        let world = World::footnote_test_stage();
        let body = world.player_body().expect("player");
        assert!((body.position[0] - FOOTNOTE_SPAWN_X).abs() < 1e-3);
        assert!(body.grounded);
        assert!(body.ignored_platform.is_none());
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
