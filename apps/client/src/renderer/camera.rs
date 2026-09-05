//! Logical/world camera for the Phase-2 orthographic 2D renderer.
//!
//! World units are independent of physical window pixels. One world unit is
//! not permanently one screen pixel. **Resolution is not camera zoom.**
//!
//! Gameplay-view policy (visibility parity):
//! - The visible world is a fixed 16:9 rectangle whose height is the requested
//!   logical height (FOOTNOTE uses [`FOOTNOTE_LOGICAL_HEIGHT`]). Width is
//!   `height × `[`GAMEPLAY_ASPECT`]. This matches server AOI
//!   [`purgatory_simulation::aoi_viewport_size`].
//! - Pixel resolution only changes how many pixels cover that rectangle.
//! - Same-aspect sizes (1280×720, 1920×1080, 2560×1440, …) show the same world.
//! - Other aspects do **not** expand FOV. The largest 16:9 pixel rectangle that
//!   fits the framebuffer is used (pillarbox if wider, letterbox if taller).
//! - X and Y are never scaled independently (no stretch).

use purgatory_simulation::WorldBounds;

/// Fixed logical viewport height in world units (compact stages).
#[allow(dead_code)] // retained for compact-stage camera helpers / tests
pub const DEFAULT_LOGICAL_HEIGHT: f32 = 9.0;

/// Phase-4.6/4.8 FOOTNOTE arena logical height (static aspect; follow moves center).
pub const FOOTNOTE_LOGICAL_HEIGHT: f32 = purgatory_simulation::FOOTNOTE_TEST_VIEWPORT_HEIGHT;

/// Gameplay-safe camera aspect. Owned by simulation AOI; the client camera
/// must not invent a second ratio.
pub const GAMEPLAY_ASPECT: f32 = purgatory_simulation::AOI_VIEWPORT_ASPECT;

/// True when a wgpu surface may be configured and drawn.
#[must_use]
pub fn is_usable_surface(width: u32, height: u32) -> bool {
    width > 0 && height > 0
}

/// World-space size of the locked gameplay view for a logical height.
#[must_use]
pub fn gameplay_viewport_size(logical_height: f32) -> [f32; 2] {
    [logical_height * GAMEPLAY_ASPECT, logical_height]
}

/// Axis-aligned pixel rectangle inside a framebuffer. wgpu origin is top-left.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PixelViewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelViewport {
    /// Map Y-up NDC (`Camera::world_to_ndc`) to Y-down framebuffer pixels.
    #[must_use]
    pub fn ndc_to_px(self, ndc: [f32; 2]) -> [f32; 2] {
        [
            self.x as f32 + (ndc[0] * 0.5 + 0.5) * self.width as f32,
            self.y as f32 + (0.5 - ndc[1] * 0.5) * self.height as f32,
        ]
    }
}

/// Largest 16:9 pixel rectangle that fits `fb_w`×`fb_h`, centered.
///
/// Exact 16:9 (`width * 9 == height * 16`) fills the framebuffer. Wider
/// frames pillarbox; taller frames letterbox. Integer arithmetic so the
/// branch is not a float epsilon.
#[must_use]
pub fn constrained_pixel_viewport(fb_w: u32, fb_h: u32) -> Option<PixelViewport> {
    if fb_w == 0 || fb_h == 0 {
        return None;
    }
    let lhs = fb_w as u64 * 9;
    let rhs = fb_h as u64 * 16;
    let (width, height) = if lhs == rhs {
        (fb_w, fb_h)
    } else if lhs > rhs {
        let width = ((fb_h as u64 * 16) / 9) as u32;
        (width.clamp(1, fb_w), fb_h)
    } else {
        let height = ((fb_w as u64 * 9) / 16) as u32;
        (fb_w, height.clamp(1, fb_h))
    };
    Some(PixelViewport {
        x: (fb_w - width) / 2,
        y: (fb_h - height) / 2,
        width,
        height,
    })
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
        let [viewport_width, viewport_height] = gameplay_viewport_size(DEFAULT_LOGICAL_HEIGHT);
        Self {
            position: [0.0, 0.0],
            viewport_width,
            viewport_height,
        }
    }

    /// Static-sized camera for the FOOTNOTE test arena (initial placement).
    #[must_use]
    pub fn footnote_test_dev() -> Self {
        let [viewport_width, viewport_height] = gameplay_viewport_size(FOOTNOTE_LOGICAL_HEIGHT);
        Self {
            position: [0.0, 0.0],
            viewport_width,
            viewport_height,
        }
    }

    /// Build a camera from a physical framebuffer size.
    ///
    /// Pixel size must be usable; it does **not** change the world FOV.
    #[must_use]
    #[allow(dead_code)]
    pub fn from_physical_pixels(width: u32, height: u32) -> Option<Self> {
        Self::from_physical_pixels_with_height(width, height, DEFAULT_LOGICAL_HEIGHT, [0.0, 0.0])
    }

    /// Build a camera with an explicit logical height and center (dev stages).
    ///
    /// `width`/`height` gate usability only. World size is
    /// [`gameplay_viewport_size`]`(logical_height)`, not framebuffer aspect.
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
        let [viewport_width, viewport_height] = gameplay_viewport_size(logical_height);
        Some(Self {
            position,
            viewport_width,
            viewport_height,
        })
    }

    /// Confirm the framebuffer is usable. World viewport is unchanged.
    #[allow(dead_code)] // tests; production FOV does not follow pixel size
    pub fn set_physical_pixels(&mut self, width: u32, height: u32) -> bool {
        is_usable_surface(width, height)
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
    ///
    /// Float-only. This must not round, floor, or snap to pixels; screen
    /// conversion is [`PixelViewport::ndc_to_px`].
    #[must_use]
    pub fn world_to_ndc(&self, world: [f32; 2]) -> [f32; 2] {
        let sx = 2.0 / self.viewport_width;
        let sy = 2.0 / self.viewport_height;
        [
            (world[0] - self.position[0]) * sx,
            (world[1] - self.position[1]) * sy,
        ]
    }

    /// Map a world-space AABB to Y-down pixel bounds in `gameplay`.
    ///
    /// Internal render resolution is not an input. Render Scale must not change
    /// these bounds for a fixed camera and gameplay rect.
    #[must_use]
    #[allow(dead_code)]
    pub fn world_aabb_to_gameplay_px(
        &self,
        min: [f32; 2],
        max: [f32; 2],
        gameplay: PixelViewport,
    ) -> [f32; 4] {
        let corners = [
            [min[0], min[1]],
            [max[0], min[1]],
            [max[0], max[1]],
            [min[0], max[1]],
        ];
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for corner in corners {
            let px = gameplay.ndc_to_px(self.world_to_ndc(corner));
            min_x = min_x.min(px[0]);
            min_y = min_y.min(px[1]);
            max_x = max_x.max(px[0]);
            max_y = max_y.max(px[1]);
        }
        [min_x, min_y, max_x, max_y]
    }

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
        assert!(constrained_pixel_viewport(0, 1080).is_none());
        assert!(constrained_pixel_viewport(1920, 0).is_none());
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
    fn resize_does_not_change_world_viewport() {
        let mut camera = Camera::from_physical_pixels(1280, 720).expect("usable");
        camera.position = [3.0, -1.0];
        let before = (camera.viewport_width, camera.viewport_height);
        assert!(camera.set_physical_pixels(800, 600));
        assert_eq!(camera.position, [3.0, -1.0]);
        assert_eq!((camera.viewport_width, camera.viewport_height), before);
        assert!(!camera.set_physical_pixels(0, 600));
        assert_eq!((camera.viewport_width, camera.viewport_height), before);
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

    #[test]
    fn gameplay_view_matches_server_aoi_viewport() {
        let client = gameplay_viewport_size(FOOTNOTE_LOGICAL_HEIGHT);
        let server = purgatory_simulation::aoi_viewport_size();
        assert!((client[0] - server[0]).abs() < 1e-5);
        assert!((client[1] - server[1]).abs() < 1e-5);
        assert!((GAMEPLAY_ASPECT - purgatory_simulation::AOI_VIEWPORT_ASPECT).abs() < 1e-6);
    }

    #[test]
    fn render_scale_does_not_change_view_projection() {
        use crate::display::{RENDER_SCALE_PRESETS, internal_render_size};

        let camera = Camera::footnote_test_dev();
        let matrix = camera.view_proj_column_major();
        let world = gameplay_viewport_size(FOOTNOTE_LOGICAL_HEIGHT);
        for scale in RENDER_SCALE_PRESETS {
            let _ = internal_render_size(1920, 1080, scale, 8192);
            assert_eq!(camera.view_proj_column_major(), matrix);
            assert!((camera.viewport_width - world[0]).abs() < 1e-5);
            assert!((camera.viewport_height - world[1]).abs() < 1e-5);
        }
        let vp = constrained_pixel_viewport(1600, 1000).expect("usable");
        assert_eq!((vp.width, vp.height), (1600, 900));
        let scaled = crate::display::internal_render_size(
            vp.width,
            vp.height,
            crate::display::RenderScale::P75,
            8192,
        )
        .expect("valid");
        assert_eq!((scaled.width, scaled.height), (1200, 675));
    }

    #[test]
    fn render_scale_does_not_change_output_pixel_bounds() {
        use crate::display::{RENDER_SCALE_PRESETS, internal_render_size};

        let camera = Camera::footnote_test_dev();
        let gameplay = constrained_pixel_viewport(1920, 1080).expect("16:9");
        let matrix = camera.view_proj_column_major();
        // Joint 0.045 wu and limb width 0.0944 wu, authored in world units.
        let joint_half = 0.045 * 0.5;
        let limb_half = 0.0944 * 0.5;
        let joint = camera.world_aabb_to_gameplay_px(
            [-joint_half, -joint_half],
            [joint_half, joint_half],
            gameplay,
        );
        let limb = camera.world_aabb_to_gameplay_px([-limb_half, -0.2], [limb_half, 0.2], gameplay);
        let ndc_min = camera.world_to_ndc([-limb_half, -0.2]);
        let ndc_max = camera.world_to_ndc([limb_half, 0.2]);
        let mut last_internal = None;
        for scale in RENDER_SCALE_PRESETS {
            let internal =
                internal_render_size(gameplay.width, gameplay.height, scale, 8192).expect("valid");
            if let Some(prev) = last_internal {
                assert_ne!(prev, (internal.width, internal.height), "{scale:?}");
            }
            last_internal = Some((internal.width, internal.height));
            assert_eq!(camera.view_proj_column_major(), matrix);
            assert_eq!(camera.world_to_ndc([-limb_half, -0.2]), ndc_min);
            assert_eq!(camera.world_to_ndc([limb_half, 0.2]), ndc_max);
            let joint_now = camera.world_aabb_to_gameplay_px(
                [-joint_half, -joint_half],
                [joint_half, joint_half],
                gameplay,
            );
            let limb_now =
                camera.world_aabb_to_gameplay_px([-limb_half, -0.2], [limb_half, 0.2], gameplay);
            for i in 0..4 {
                assert!(
                    (joint[i] - joint_now[i]).abs() < 1e-4,
                    "joint bounds moved at {scale:?}: {joint:?} vs {joint_now:?}"
                );
                assert!(
                    (limb[i] - limb_now[i]).abs() < 1e-4,
                    "limb bounds moved at {scale:?}: {limb:?} vs {limb_now:?}"
                );
            }
        }
        let w = joint[2] - joint[0];
        let h = joint[3] - joint[1];
        assert!((w - h).abs() < 1e-3, "joint projection stretched {w}x{h}");
    }

    fn assert_finite_camera(camera: &Camera) {
        assert!(camera.viewport_width.is_finite() && camera.viewport_width > 0.0);
        assert!(camera.viewport_height.is_finite() && camera.viewport_height > 0.0);
        let m = camera.view_proj_column_major();
        for v in m {
            assert!(v.is_finite(), "{m:?}");
        }
        let ndc = camera.world_to_ndc([1.0, 1.0]);
        assert!(ndc[0].is_finite() && ndc[1].is_finite());
        assert!(!ndc[0].is_nan() && !ndc[1].is_nan());
    }

    fn assert_unstretched_in_pixel_rect(fb_w: u32, fb_h: u32, camera: &Camera) {
        let vp = constrained_pixel_viewport(fb_w, fb_h).expect("usable");
        let px_per_wu_x = vp.width as f32 / camera.viewport_width;
        let px_per_wu_y = vp.height as f32 / camera.viewport_height;
        assert!(
            (px_per_wu_x - px_per_wu_y).abs() < 1e-3,
            "stretch {px_per_wu_x} vs {px_per_wu_y} in {fb_w}x{fb_h} vp={vp:?}"
        );
        let aspect = vp.width as f64 / vp.height as f64;
        assert!(
            (aspect - f64::from(GAMEPLAY_ASPECT)).abs() < 0.002,
            "pixel viewport aspect {aspect} fb={fb_w}x{fb_h}"
        );
    }

    #[test]
    fn same_aspect_preserves_world_viewport_across_resolutions() {
        let a = Camera::from_physical_pixels_with_height(
            1280,
            720,
            FOOTNOTE_LOGICAL_HEIGHT,
            [0.0, 0.0],
        )
        .expect("usable");
        let b = Camera::from_physical_pixels_with_height(
            1920,
            1080,
            FOOTNOTE_LOGICAL_HEIGHT,
            [0.0, 0.0],
        )
        .expect("usable");
        let c = Camera::from_physical_pixels_with_height(
            2560,
            1440,
            FOOTNOTE_LOGICAL_HEIGHT,
            [0.0, 0.0],
        )
        .expect("usable");
        assert!((a.viewport_height - FOOTNOTE_LOGICAL_HEIGHT).abs() < 1e-5);
        assert!((a.viewport_width - b.viewport_width).abs() < 1e-4);
        assert!((a.viewport_width - c.viewport_width).abs() < 1e-4);
        assert_finite_camera(&a);
        assert_finite_camera(&b);
        assert_finite_camera(&c);
        let one_wu = a.world_to_ndc([1.0, 0.0])[0];
        assert!((b.world_to_ndc([1.0, 0.0])[0] - one_wu).abs() < 1e-5);
        assert_eq!(
            constrained_pixel_viewport(1280, 720).unwrap(),
            PixelViewport {
                x: 0,
                y: 0,
                width: 1280,
                height: 720
            }
        );
        assert_eq!(
            constrained_pixel_viewport(1920, 1080).unwrap(),
            PixelViewport {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(
            constrained_pixel_viewport(2560, 1440).unwrap(),
            PixelViewport {
                x: 0,
                y: 0,
                width: 2560,
                height: 1440
            }
        );
    }

    #[test]
    fn aspect_changes_letterbox_not_world_fov() {
        let hd = Camera::from_physical_pixels_with_height(
            1920,
            1080,
            FOOTNOTE_LOGICAL_HEIGHT,
            [0.0, 0.0],
        )
        .expect("usable");
        let four_three = Camera::from_physical_pixels_with_height(
            1024,
            768,
            FOOTNOTE_LOGICAL_HEIGHT,
            [0.0, 0.0],
        )
        .expect("usable");
        let ultra = Camera::from_physical_pixels_with_height(
            2560,
            1080,
            FOOTNOTE_LOGICAL_HEIGHT,
            [0.0, 0.0],
        )
        .expect("usable");
        let sixteen_ten = Camera::from_physical_pixels_with_height(
            1920,
            1200,
            FOOTNOTE_LOGICAL_HEIGHT,
            [0.0, 0.0],
        )
        .expect("usable");
        let tiny =
            Camera::from_physical_pixels_with_height(2, 2, FOOTNOTE_LOGICAL_HEIGHT, [0.0, 0.0])
                .expect("usable");
        let expected = gameplay_viewport_size(FOOTNOTE_LOGICAL_HEIGHT);
        for cam in [hd, four_three, ultra, sixteen_ten, tiny] {
            assert!((cam.viewport_width - expected[0]).abs() < 1e-5);
            assert!((cam.viewport_height - expected[1]).abs() < 1e-5);
            assert_finite_camera(&cam);
            assert_eq!(cam.view_proj_column_major(), hd.view_proj_column_major());
        }

        assert_eq!(
            constrained_pixel_viewport(1920, 1080).unwrap(),
            PixelViewport {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(
            constrained_pixel_viewport(2560, 1080).unwrap(),
            PixelViewport {
                x: 320,
                y: 0,
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(
            constrained_pixel_viewport(1024, 768).unwrap(),
            PixelViewport {
                x: 0,
                y: 96,
                width: 1024,
                height: 576
            }
        );
        assert_eq!(
            constrained_pixel_viewport(1920, 1200).unwrap(),
            PixelViewport {
                x: 0,
                y: 60,
                width: 1920,
                height: 1080
            }
        );

        assert_unstretched_in_pixel_rect(1920, 1080, &hd);
        assert_unstretched_in_pixel_rect(1024, 768, &four_three);
        assert_unstretched_in_pixel_rect(2560, 1080, &ultra);
        assert_unstretched_in_pixel_rect(1920, 1200, &sixteen_ten);
    }

    #[test]
    fn ndc_maps_into_letterboxed_pixel_rect() {
        let vp = constrained_pixel_viewport(2560, 1080).unwrap();
        let origin = vp.ndc_to_px([0.0, 0.0]);
        assert!((origin[0] - 1280.0).abs() < 1e-4);
        assert!((origin[1] - 540.0).abs() < 1e-4);
        let top_right = vp.ndc_to_px([1.0, 1.0]);
        assert!((top_right[0] - 2240.0).abs() < 1e-4);
        assert!(top_right[1].abs() < 1e-4);
    }

    #[test]
    fn world_to_pixels_is_not_quantized() {
        let camera = Camera::footnote_test_dev();
        let vp = constrained_pixel_viewport(1280, 720).unwrap();
        let px = vp.ndc_to_px(camera.world_to_ndc([0.017, 0.009]));
        assert!((px[0] - px[0].round()).abs() > 1e-3);
        assert!((px[1] - px[1].round()).abs() > 1e-3);
        let stepped = vp.ndc_to_px(camera.world_to_ndc([0.018, 0.009]));
        let d = (stepped[0] - px[0]).abs();
        assert!(d > 1e-5 && d < 0.2, "subpixel world step quantized, d={d}");
    }
}
