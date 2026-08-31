//! Logical/world camera for the Phase-2 orthographic 2D renderer.
//!
//! World units are independent of physical window pixels. One world unit is
//! not permanently one screen pixel.
//!
//! Phase 4.8: optional player-follow with world-bound clamping (presentation only).

use purgatory_simulation::WorldBounds;

/// Fixed logical viewport height in world units (compact stages).
#[allow(dead_code)] // retained for compact-stage camera helpers / tests
pub const DEFAULT_LOGICAL_HEIGHT: f32 = 9.0;

/// Phase-4.6/4.8 FOOTNOTE arena logical height (static aspect; follow moves center).
pub const FOOTNOTE_LOGICAL_HEIGHT: f32 = purgatory_simulation::FOOTNOTE_TEST_VIEWPORT_HEIGHT;

/// True when a wgpu surface may be configured and drawn.
#[must_use]
pub fn is_usable_surface(width: u32, height: u32) -> bool {
    width > 0 && height > 0
}

/// Clamp a desired camera center so the viewport stays inside `bounds`.
///
/// If the world is smaller than the viewport on an axis, center on that axis.
#[must_use]
pub fn clamp_camera_center(
    desired: [f32; 2],
    viewport_width: f32,
    viewport_height: f32,
    bounds: WorldBounds,
) -> [f32; 2] {
    let half_w = viewport_width * 0.5;
    let half_h = viewport_height * 0.5;
    let x = if bounds.width() <= viewport_width {
        bounds.center()[0]
    } else {
        desired[0].clamp(bounds.min_x + half_w, bounds.max_x - half_w)
    };
    let y = if bounds.height() <= viewport_height {
        bounds.center()[1]
    } else {
        desired[1].clamp(bounds.min_y + half_h, bounds.max_y - half_h)
    };
    [x, y]
}

/// 2D orthographic camera in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// World-space center.
    pub position: [f32; 2],
    /// Visible world width.
    pub viewport_width: f32,
    /// Visible world height.
    pub viewport_height: f32,
}

impl Camera {
    /// Default camera looking at the origin with a 16×9 logical viewport.
    #[must_use]
    #[allow(dead_code)]
    pub fn default_dev() -> Self {
        Self {
            position: [0.0, 0.0],
            viewport_width: 16.0,
            viewport_height: DEFAULT_LOGICAL_HEIGHT,
        }
    }

    /// Static-sized camera for the FOOTNOTE test arena (initial placement).
    #[must_use]
    pub fn footnote_test_dev() -> Self {
        Self {
            position: [0.0, 0.0],
            viewport_width: FOOTNOTE_LOGICAL_HEIGHT * (16.0 / 9.0),
            viewport_height: FOOTNOTE_LOGICAL_HEIGHT,
        }
    }

    /// Build a camera from a physical framebuffer size.
    #[must_use]
    #[allow(dead_code)]
    pub fn from_physical_pixels(width: u32, height: u32) -> Option<Self> {
        Self::from_physical_pixels_with_height(width, height, DEFAULT_LOGICAL_HEIGHT, [0.0, 0.0])
    }

    /// Build a camera with an explicit logical height and center (dev stages).
    #[must_use]
    pub fn from_physical_pixels_with_height(
        width: u32,
        height: u32,
        logical_height: f32,
        position: [f32; 2],
    ) -> Option<Self> {
        if !is_usable_surface(width, height) {
            return None;
        }
        let aspect = width as f32 / height as f32;
        Some(Self {
            position,
            viewport_width: logical_height * aspect,
            viewport_height: logical_height,
        })
    }

    /// Replace viewport size after a resize. Position is unchanged.
    pub fn set_physical_pixels(&mut self, width: u32, height: u32) -> bool {
        let height_units = self.viewport_height;
        let position = self.position;
        let Some(updated) =
            Self::from_physical_pixels_with_height(width, height, height_units, position)
        else {
            return false;
        };
        self.viewport_width = updated.viewport_width;
        self.viewport_height = updated.viewport_height;
        true
    }

    /// Follow a world-space point, then clamp to `bounds`.
    pub fn follow_clamped(&mut self, target: [f32; 2], bounds: WorldBounds) {
        self.position =
            clamp_camera_center(target, self.viewport_width, self.viewport_height, bounds);
    }

    /// Re-apply bound clamping without changing the follow target intent.
    pub fn clamp_to_bounds(&mut self, bounds: WorldBounds) {
        self.position = clamp_camera_center(
            self.position,
            self.viewport_width,
            self.viewport_height,
            bounds,
        );
    }

    /// Map a world-space point to clip-space XY in `[-1, 1]`.
    #[must_use]
    pub fn world_to_ndc(&self, world: [f32; 2]) -> [f32; 2] {
        let sx = 2.0 / self.viewport_width;
        let sy = 2.0 / self.viewport_height;
        [
            (world[0] - self.position[0]) * sx,
            (world[1] - self.position[1]) * sy,
        ]
    }

    /// Column-major 4×4 orthographic matrix for the WGSL uniform.
    #[must_use]
    pub fn view_proj_column_major(&self) -> [f32; 16] {
        let sx = 2.0 / self.viewport_width;
        let sy = 2.0 / self.viewport_height;
        let tx = -self.position[0] * sx;
        let ty = -self.position[1] * sy;
        [
            sx, 0.0, 0.0, 0.0, 0.0, sy, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, tx, ty, 0.0, 1.0,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_camera_is_16_by_9_at_origin() {
        let camera = Camera::default_dev();
        assert_eq!(camera.position, [0.0, 0.0]);
        assert_eq!(camera.viewport_width, 16.0);
        assert_eq!(camera.viewport_height, 9.0);
    }

    #[test]
    fn physical_1280x720_maps_to_16_by_9_world() {
        let camera = Camera::from_physical_pixels(1280, 720).expect("usable");
        assert!((camera.viewport_width - 16.0).abs() < f32::EPSILON);
        assert!((camera.viewport_height - 9.0).abs() < f32::EPSILON);
    }

    #[test]
    fn zero_size_is_not_usable() {
        assert!(!is_usable_surface(0, 720));
        assert!(!is_usable_surface(1280, 0));
        assert!(Camera::from_physical_pixels(0, 0).is_none());
    }

    #[test]
    fn world_origin_projects_to_ndc_origin() {
        let camera = Camera::default_dev();
        assert_eq!(camera.world_to_ndc([0.0, 0.0]), [0.0, 0.0]);
    }

    #[test]
    fn viewport_edge_projects_to_clip_edge() {
        let camera = Camera::default_dev();
        let ndc = camera.world_to_ndc([8.0, 4.5]);
        assert!((ndc[0] - 1.0).abs() < 1e-5);
        assert!((ndc[1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn world_units_are_not_pixels() {
        let camera = Camera::from_physical_pixels(1280, 720).expect("usable");
        let ndc = camera.world_to_ndc([1.0, 0.0]);
        assert!((ndc[0] - (2.0 / 16.0)).abs() < 1e-5);
    }

    #[test]
    fn resize_updates_aspect_and_keeps_camera_center() {
        let mut camera = Camera::from_physical_pixels(1280, 720).expect("usable");
        camera.position = [3.0, -1.0];
        assert!(camera.set_physical_pixels(800, 600));
        assert_eq!(camera.position, [3.0, -1.0]);
        assert!((camera.viewport_height - 9.0).abs() < f32::EPSILON);
        assert!((camera.viewport_width - 12.0).abs() < f32::EPSILON);
    }

    #[test]
    fn follow_moves_toward_player_then_clamps_left() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let mut cam = Camera::footnote_test_dev();
        let half_w = cam.viewport_width * 0.5;
        cam.follow_clamped([bounds.min_x + 1.0, 0.0], bounds);
        assert!((cam.position[0] - (bounds.min_x + half_w)).abs() < 1e-3);
    }

    #[test]
    fn follow_clamps_right_edge() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let mut cam = Camera::footnote_test_dev();
        let half_w = cam.viewport_width * 0.5;
        cam.follow_clamped([bounds.max_x - 1.0, 0.0], bounds);
        assert!((cam.position[0] - (bounds.max_x - half_w)).abs() < 1e-3);
    }

    #[test]
    fn follow_clamps_vertical() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let mut cam = Camera::footnote_test_dev();
        let half_h = cam.viewport_height * 0.5;
        cam.follow_clamped([0.0, bounds.max_y - 0.5], bounds);
        assert!((cam.position[1] - (bounds.max_y - half_h)).abs() < 1e-3);
        cam.follow_clamped([0.0, bounds.min_y + 0.5], bounds);
        assert!((cam.position[1] - (bounds.min_y + half_h)).abs() < 1e-3);
    }

    #[test]
    fn viewport_larger_than_world_centers() {
        let bounds = WorldBounds {
            min_x: -2.0,
            max_x: 2.0,
            min_y: -1.0,
            max_y: 1.0,
        };
        let mut cam = Camera {
            position: [10.0, 10.0],
            viewport_width: 20.0,
            viewport_height: 20.0,
        };
        cam.follow_clamped([0.0, 0.0], bounds);
        assert!((cam.position[0] - bounds.center()[0]).abs() < 1e-4);
        assert!((cam.position[1] - bounds.center()[1]).abs() < 1e-4);
    }

    #[test]
    fn interior_follow_tracks_player() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let mut cam = Camera::footnote_test_dev();
        let target = [0.0, 1.0];
        cam.follow_clamped(target, bounds);
        assert!((cam.position[0] - target[0]).abs() < 1e-3);
        assert!((cam.position[1] - target[1]).abs() < 1e-3);
    }

    #[test]
    fn clamp_matches_server_aoi_camera_clamp() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let [vw, vh] = purgatory_simulation::aoi_viewport_size();
        let desired = [-19.4, -3.0];
        let client = clamp_camera_center(desired, vw, vh, bounds);
        let server = purgatory_simulation::aoi_clamp_camera_center(desired, vw, vh, bounds);
        assert!((client[0] - server[0]).abs() < 1e-5);
        assert!((client[1] - server[1]).abs() < 1e-5);
        assert!((client[0] - (-11.556)).abs() < 0.01);
    }
}
