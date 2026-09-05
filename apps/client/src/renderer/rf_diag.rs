//! RF0 isolated rendering diagnostic: fixed rotated/translating quads.
//!
//! Presentation-only. Independent of animation, skeleton, prediction, and
//! simulation. Drawn through the existing world primitive pass.

use super::camera::{Camera, PixelViewport};
use super::gpu::DrawQuad;

/// World-space origin of the RF0 row. Near FOOTNOTE spawn-follow camera center
/// (`spawn_x + deadzone_x` ≈ `-16.4`) so the scene is visible at Game start.
pub const RF_ORIGIN: [f32; 2] = [-16.0, 2.2];
pub const RF_QUAD_SIZE: [f32; 2] = [1.35, 0.85];
/// Subpixel-per-frame at 1280×720 (~51 px/wu): 0.05 wu/s ≈ 0.04 px/frame @ 60 Hz.
pub const RF_TRANSLATE_SPEED: f32 = 0.05;
pub const RF_TRANSLATE_AMPLITUDE: f32 = 0.85;
/// Slow rotation so edge crawling is visible (~12°/s).
pub const RF_ROTATE_SPEED: f32 = 12.0_f32 * std::f32::consts::PI / 180.0;
pub const RF_STATIC_ANGLES_DEG: [f32; 4] = [0.0, 15.0, 30.0, 45.0];
const STATIC_SPACING: f32 = 2.15;
const MOTION_ROW: f32 = -2.15;

/// Which RF0 solid the screen-space probe tracks.
pub const RF_PROBE_LABEL: &str = "translate+rotate";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RfProjectedCorners {
    pub world: [[f32; 2]; 4],
    pub ndc: [[f32; 2]; 4],
    pub internal_px: [[f32; 2]; 4],
    pub output_px: [[f32; 2]; 4],
}

/// Compact per-frame proof for one diagnostic quad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RfVertexProof {
    pub elapsed: f32,
    pub camera: [f32; 2],
    pub world: [[f32; 2]; 4],
    pub ndc: [[f32; 2]; 4],
    pub internal_px: [[f32; 2]; 4],
    pub output_px: [[f32; 2]; 4],
    pub d_internal_px: [[f32; 2]; 4],
    pub max_d_world: f32,
    pub max_d_ndc: f32,
    pub max_d_internal: f32,
    pub max_d_output: f32,
    /// True when any internal-pixel axis landed within 1e-3 of an integer.
    pub near_integer_internal: bool,
}

impl Default for RfVertexProof {
    fn default() -> Self {
        Self {
            elapsed: 0.0,
            camera: [0.0, 0.0],
            world: [[0.0; 2]; 4],
            ndc: [[0.0; 2]; 4],
            internal_px: [[0.0; 2]; 4],
            output_px: [[0.0; 2]; 4],
            d_internal_px: [[0.0; 2]; 4],
            max_d_world: 0.0,
            max_d_ndc: 0.0,
            max_d_internal: 0.0,
            max_d_output: 0.0,
            near_integer_internal: false,
        }
    }
}

impl RfVertexProof {
    #[must_use]
    pub fn compact_line(&self) -> String {
        let ip = self.internal_px;
        format!(
            "RF0 {RF_PROBE_LABEL} t={:.4} cam=({:.6},{:.6}) ipx=({:.4},{:.4})({:.4},{:.4})({:.4},{:.4})({:.4},{:.4}) d_ipx={:.5} d_world={:.6} near_int={}",
            self.elapsed,
            self.camera[0],
            self.camera[1],
            ip[0][0],
            ip[0][1],
            ip[1][0],
            ip[1][1],
            ip[2][0],
            ip[2][1],
            ip[3][0],
            ip[3][1],
            self.max_d_internal,
            self.max_d_world,
            self.near_integer_internal
        )
    }
}

/// Ping-pong offset in `[-amplitude, amplitude]`.
#[must_use]
pub fn rf_translate_offset(elapsed: f32) -> f32 {
    if !elapsed.is_finite() {
        return 0.0;
    }
    let period = (4.0 * RF_TRANSLATE_AMPLITUDE / RF_TRANSLATE_SPEED).max(1e-6);
    let t = elapsed.rem_euclid(period);
    let half = period * 0.5;
    if t <= half {
        -RF_TRANSLATE_AMPLITUDE + RF_TRANSLATE_SPEED * t
    } else {
        RF_TRANSLATE_AMPLITUDE - RF_TRANSLATE_SPEED * (t - half)
    }
}

#[must_use]
pub fn rf_rotation(elapsed: f32) -> f32 {
    if !elapsed.is_finite() {
        return 0.0;
    }
    elapsed * RF_ROTATE_SPEED
}

fn static_center(index: usize) -> [f32; 2] {
    [RF_ORIGIN[0] + index as f32 * STATIC_SPACING, RF_ORIGIN[1]]
}

fn motion_center(column: usize, elapsed: f32, translate: bool) -> [f32; 2] {
    let x = RF_ORIGIN[0]
        + column as f32 * STATIC_SPACING
        + if translate {
            rf_translate_offset(elapsed)
        } else {
            0.0
        };
    [x, RF_ORIGIN[1] + MOTION_ROW]
}

fn oriented_at(center: [f32; 2], rotation: f32, color: [f32; 4]) -> DrawQuad {
    DrawQuad::oriented(center, RF_QUAD_SIZE, [0.0, 0.0], rotation, color)
}

/// Seven solids: static 0/15/30/45°, rotate, translate, translate+rotate.
#[must_use]
pub fn rf_scene_quads(elapsed: f32) -> [DrawQuad; 7] {
    let static_colors = [
        [0.92, 0.92, 0.94, 1.0],
        [0.95, 0.82, 0.18, 1.0],
        [0.95, 0.48, 0.16, 1.0],
        [0.92, 0.22, 0.22, 1.0],
    ];
    [
        oriented_at(
            static_center(0),
            RF_STATIC_ANGLES_DEG[0].to_radians(),
            static_colors[0],
        ),
        oriented_at(
            static_center(1),
            RF_STATIC_ANGLES_DEG[1].to_radians(),
            static_colors[1],
        ),
        oriented_at(
            static_center(2),
            RF_STATIC_ANGLES_DEG[2].to_radians(),
            static_colors[2],
        ),
        oriented_at(
            static_center(3),
            RF_STATIC_ANGLES_DEG[3].to_radians(),
            static_colors[3],
        ),
        oriented_at(
            motion_center(0, elapsed, false),
            rf_rotation(elapsed),
            [0.18, 0.82, 0.92, 1.0],
        ),
        oriented_at(
            motion_center(1, elapsed, true),
            0.0,
            [0.86, 0.22, 0.86, 1.0],
        ),
        rf_probe_quad(elapsed),
    ]
}

/// Selected probe: translation + rotation together.
#[must_use]
pub fn rf_probe_quad(elapsed: f32) -> DrawQuad {
    oriented_at(
        motion_center(2, elapsed, true),
        rf_rotation(elapsed),
        [0.22, 0.92, 0.38, 1.0],
    )
}

#[must_use]
pub fn project_corners(
    corners: [[f32; 2]; 4],
    camera: &Camera,
    internal: PixelViewport,
    output: PixelViewport,
) -> RfProjectedCorners {
    let mut ndc = [[0.0; 2]; 4];
    let mut internal_px = [[0.0; 2]; 4];
    let mut output_px = [[0.0; 2]; 4];
    for i in 0..4 {
        ndc[i] = camera.world_to_ndc(corners[i]);
        internal_px[i] = internal.ndc_to_px(ndc[i]);
        output_px[i] = output.ndc_to_px(ndc[i]);
    }
    RfProjectedCorners {
        world: corners,
        ndc,
        internal_px,
        output_px,
    }
}

#[must_use]
fn max_abs_delta(prev: [[f32; 2]; 4], now: [[f32; 2]; 4]) -> (f32, [[f32; 2]; 4]) {
    let mut max = 0.0_f32;
    let mut d = [[0.0; 2]; 4];
    for i in 0..4 {
        d[i] = [now[i][0] - prev[i][0], now[i][1] - prev[i][1]];
        max = max.max(d[i][0].abs()).max(d[i][1].abs());
    }
    (max, d)
}

#[must_use]
fn near_integer(v: f32) -> bool {
    (v - v.round()).abs() < 1e-3
}

/// Project the probe and attach frame-to-frame deltas.
#[must_use]
pub fn rf_probe_proof(
    elapsed: f32,
    camera: &Camera,
    internal: PixelViewport,
    output: PixelViewport,
    previous: Option<&RfVertexProof>,
) -> RfVertexProof {
    let projected = project_corners(
        rf_probe_quad(elapsed).world_corners(),
        camera,
        internal,
        output,
    );
    let (max_d_world, _) = previous
        .map(|p| max_abs_delta(p.world, projected.world))
        .unwrap_or((0.0, [[0.0; 2]; 4]));
    let (max_d_ndc, _) = previous
        .map(|p| max_abs_delta(p.ndc, projected.ndc))
        .unwrap_or((0.0, [[0.0; 2]; 4]));
    let (max_d_internal, d_internal_px) = previous
        .map(|p| max_abs_delta(p.internal_px, projected.internal_px))
        .unwrap_or((0.0, [[0.0; 2]; 4]));
    let (max_d_output, _) = previous
        .map(|p| max_abs_delta(p.output_px, projected.output_px))
        .unwrap_or((0.0, [[0.0; 2]; 4]));
    let near_integer_internal = projected
        .internal_px
        .iter()
        .any(|p| near_integer(p[0]) || near_integer(p[1]));
    RfVertexProof {
        elapsed,
        camera: camera.position,
        world: projected.world,
        ndc: projected.ndc,
        internal_px: projected.internal_px,
        output_px: projected.output_px,
        d_internal_px,
        max_d_world,
        max_d_ndc,
        max_d_internal,
        max_d_output,
        near_integer_internal,
    }
}

/// Fixed 16:9 diagnostic panel. Source pixels equal displayed pixels for RF1.5.
pub const RF_AB_PANEL_W: u32 = 320;
pub const RF_AB_PANEL_H: u32 = 180;
pub const RF_AB_MARGIN: u32 = 8;
pub const RF_AB_GAP: u32 = 8;
pub const RF_AB_CAPTION: u32 = 18;
/// Frozen diagnostic camera. Independent of gameplay follow / Render Scale.
pub const RF_AB_VIEWPORT: [f32; 2] = [8.0, 4.5];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RfAbSlot {
    Msaa1x,
    Msaa4x,
    BlitNearest,
    BlitLinear,
    Scale100,
    Scale200,
    Scale400,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RfAbPanel {
    pub slot: RfAbSlot,
    pub label: &'static str,
    pub dest: PixelViewport,
    pub source: (u32, u32),
    pub samples: u32,
    pub nearest: bool,
}

#[must_use]
pub fn rf_ab_camera() -> Camera {
    Camera {
        position: [0.0, 0.0],
        viewport_width: RF_AB_VIEWPORT[0],
        viewport_height: RF_AB_VIEWPORT[1],
    }
}

#[must_use]
pub fn rf_ab_panel_source(scale_percent: u16) -> (u32, u32) {
    rf_ab_scaled_extent(RF_AB_PANEL_W, RF_AB_PANEL_H, scale_percent)
}

#[must_use]
pub fn rf_ab_scaled_extent(width: u32, height: u32, scale_percent: u16) -> (u32, u32) {
    let percent = u64::from(scale_percent);
    (
        ((u64::from(width) * percent) / 100).max(1) as u32,
        ((u64::from(height) * percent) / 100).max(1) as u32,
    )
}

fn rf_ab_cell(col: u32, row: u32) -> PixelViewport {
    PixelViewport {
        x: RF_AB_MARGIN + col * (RF_AB_PANEL_W + RF_AB_GAP),
        y: RF_AB_MARGIN + RF_AB_CAPTION + row * (RF_AB_CAPTION + RF_AB_PANEL_H + RF_AB_GAP),
        width: RF_AB_PANEL_W,
        height: RF_AB_PANEL_H,
    }
}

/// Integer supersample percents for the RF3 row. Not Display presets.
pub const RF3_SCALE_PERCENTS: [u16; 3] = [100, 200, 400];

/// Seven on-screen cells: MSAA A/B, blit A/B, then 100/200/400% at 4× (linear↓).
#[must_use]
pub fn rf_ab_layout(msaa_4x: bool) -> [RfAbPanel; 7] {
    let samples = if msaa_4x { 4 } else { 1 };
    let src_1 = rf_ab_panel_source(100);
    let src_200 = rf_ab_panel_source(200);
    let src_400 = rf_ab_panel_source(400);
    [
        RfAbPanel {
            slot: RfAbSlot::Msaa1x,
            label: "1× MSAA",
            dest: rf_ab_cell(0, 0),
            source: src_1,
            samples: 1,
            nearest: true,
        },
        RfAbPanel {
            slot: RfAbSlot::Msaa4x,
            label: if msaa_4x {
                "4× MSAA"
            } else {
                "4× MSAA (n/a)"
            },
            dest: rf_ab_cell(1, 0),
            source: src_1,
            samples,
            nearest: true,
        },
        RfAbPanel {
            slot: RfAbSlot::BlitNearest,
            label: "Nearest 1:1",
            dest: rf_ab_cell(0, 1),
            source: src_1,
            samples: 1,
            nearest: true,
        },
        RfAbPanel {
            slot: RfAbSlot::BlitLinear,
            label: "Linear 1:1",
            dest: rf_ab_cell(1, 1),
            source: src_1,
            samples: 1,
            nearest: false,
        },
        RfAbPanel {
            slot: RfAbSlot::Scale100,
            label: "100% + 4×",
            dest: rf_ab_cell(0, 2),
            source: src_1,
            samples,
            nearest: false,
        },
        RfAbPanel {
            slot: RfAbSlot::Scale200,
            label: "200% + 4×",
            dest: rf_ab_cell(1, 2),
            source: src_200,
            samples,
            nearest: false,
        },
        RfAbPanel {
            slot: RfAbSlot::Scale400,
            label: "400% + 4×",
            dest: rf_ab_cell(2, 2),
            source: src_400,
            samples,
            nearest: false,
        },
    ]
}

#[must_use]
pub fn rf_ab_sample_cost(source: (u32, u32), samples: u32) -> u64 {
    u64::from(source.0) * u64::from(source.1) * u64::from(samples.max(1))
}

/// RF0 primitives in a compact frozen frame: 45°, rotate, translate, both.
#[must_use]
pub fn rf_ab_scene_quads(elapsed: f32) -> [DrawQuad; 4] {
    let rot = rf_rotation(elapsed);
    [
        oriented_at(
            [-2.0, 1.05],
            RF_STATIC_ANGLES_DEG[3].to_radians(),
            [0.92, 0.22, 0.22, 1.0],
        ),
        oriented_at([2.0, 1.05], rot, [0.18, 0.82, 0.92, 1.0]),
        oriented_at(
            [-2.0 + rf_translate_offset(elapsed), -1.05],
            0.0,
            [0.86, 0.22, 0.86, 1.0],
        ),
        oriented_at(
            [2.0 + rf_translate_offset(elapsed), -1.05],
            rot,
            [0.22, 0.92, 0.38, 1.0],
        ),
    ]
}

#[cfg(test)]
#[must_use]
pub fn rf_ab_scene_corners(elapsed: f32) -> [[[f32; 2]; 4]; 4] {
    rf_ab_scene_quads(elapsed).map(DrawQuad::world_corners)
}

#[cfg(test)]
mod tests {
    use super::super::camera::constrained_pixel_viewport;
    use super::*;

    fn footnote_camera_at(position: [f32; 2]) -> Camera {
        let mut camera = Camera::footnote_test_dev();
        camera.position = position;
        camera
    }

    fn viewports() -> (PixelViewport, PixelViewport) {
        let output = constrained_pixel_viewport(1280, 720).expect("16:9");
        let internal = PixelViewport {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        };
        (internal, output)
    }

    #[test]
    fn static_scene_uses_authored_angles() {
        let quads = rf_scene_quads(0.0);
        for (i, deg) in RF_STATIC_ANGLES_DEG.iter().enumerate() {
            let expected =
                oriented_at(static_center(i), deg.to_radians(), [1.0; 4]).world_corners();
            let got = quads[i].world_corners();
            for k in 0..4 {
                assert!(
                    (expected[k][0] - got[k][0]).abs() < 1e-5
                        && (expected[k][1] - got[k][1]).abs() < 1e-5,
                    "angle {deg} corner {k}"
                );
            }
        }
    }

    #[test]
    fn translate_offset_is_fractional_not_snapped() {
        let a = rf_translate_offset(1.0 / 60.0);
        let b = rf_translate_offset(2.0 / 60.0);
        assert!(a.abs() > 1e-6 && a.abs() < RF_TRANSLATE_AMPLITUDE);
        assert!((b - a).abs() > 1e-6);
        assert!(
            (a - a.round()).abs() > 1e-4,
            "world offset must not be integer"
        );
        assert!((b - a - RF_TRANSLATE_SPEED / 60.0).abs() < 1e-5);
    }

    #[test]
    fn frozen_camera_static_probe_vertices_are_identical() {
        let camera = footnote_camera_at([-16.4, 0.0]);
        let (internal, output) = viewports();
        let a = rf_probe_proof(0.0, &camera, internal, output, None);
        let b = rf_probe_proof(0.0, &camera, internal, output, Some(&a));
        assert!(b.max_d_world < 1e-7);
        assert!(b.max_d_ndc < 1e-7);
        assert!(b.max_d_internal < 1e-5);
        assert!(b.max_d_output < 1e-5);
    }

    #[test]
    fn frozen_camera_subpixel_translation_is_continuous_not_quantized() {
        let camera = footnote_camera_at([-16.4, 0.0]);
        let (internal, output) = viewports();
        let dt = 1.0 / 60.0;
        let mut prev = None;
        let mut xs = Vec::new();
        let mut deltas = Vec::new();
        for i in 0..90 {
            let t = i as f32 * dt;
            let proof = rf_probe_proof(t, &camera, internal, output, prev.as_ref());
            xs.push(proof.internal_px[0][0]);
            if i > 0 {
                deltas.push(proof.max_d_internal);
                assert!(
                    proof.max_d_world > 0.0,
                    "world vertices must move under translation+rotation"
                );
                assert!(
                    proof.max_d_internal > 1e-5,
                    "internal pixels must move; got {}",
                    proof.max_d_internal
                );
                // Expected motion is a fraction of a pixel per frame, not a 1 px snap.
                assert!(
                    proof.max_d_internal < 0.35,
                    "internal jump {} looks quantized",
                    proof.max_d_internal
                );
            }
            prev = Some(proof);
        }
        let unique: std::collections::BTreeSet<i32> =
            xs.iter().map(|x| (x * 1000.0).round() as i32).collect();
        assert!(
            unique.len() > 40,
            "screen X must keep changing, got {}",
            unique.len()
        );
        let integer_hits = xs.iter().filter(|x| near_integer(**x)).count();
        assert!(
            integer_hits < xs.len() / 3,
            "internal X clustered on integers ({integer_hits}/{})",
            xs.len()
        );
        let mean = deltas.iter().sum::<f32>() / deltas.len() as f32;
        let var = deltas.iter().map(|d| (d - mean) * (d - mean)).sum::<f32>() / deltas.len() as f32;
        assert!(
            var.sqrt() < 0.08,
            "internal deltas should be smooth, std={}",
            var.sqrt()
        );
    }

    #[test]
    fn moving_camera_shifts_static_quad_screen_vertices() {
        let (internal, output) = viewports();
        let a_cam = footnote_camera_at([-16.4, 0.0]);
        let mut b_cam = a_cam;
        b_cam.position[0] += 0.01;
        let static_quad = rf_scene_quads(0.0)[3];
        let a = project_corners(static_quad.world_corners(), &a_cam, internal, output);
        let b = project_corners(static_quad.world_corners(), &b_cam, internal, output);
        let (d, _) = max_abs_delta(a.internal_px, b.internal_px);
        assert!(d > 0.01, "camera motion must move screen vertices, d={d}");
        assert!(d < 1.0, "0.01 wu should stay subpixel at 1280×720, d={d}");
    }

    #[test]
    fn world_to_internal_pixels_does_not_round() {
        let camera = footnote_camera_at([0.0, 0.0]);
        let (internal, _) = viewports();
        let ndc = camera.world_to_ndc([0.013, -0.007]);
        let px = internal.ndc_to_px(ndc);
        assert!((px[0] - px[0].round()).abs() > 1e-3);
        assert!((px[1] - px[1].round()).abs() > 1e-3);
        assert_eq!(px, internal.ndc_to_px(ndc));
    }

    #[test]
    fn rf_ab_uses_existing_rf0_angles_and_motion() {
        let quads = rf_ab_scene_quads(0.0);
        let expected_45 = oriented_at([-2.0, 1.05], RF_STATIC_ANGLES_DEG[3].to_radians(), [1.0; 4])
            .world_corners();
        let got = quads[0].world_corners();
        for k in 0..4 {
            assert!((expected_45[k][0] - got[k][0]).abs() < 1e-5);
            assert!((expected_45[k][1] - got[k][1]).abs() < 1e-5);
        }
        let a = rf_ab_scene_corners(0.4);
        let b = rf_ab_scene_corners(0.4);
        assert_eq!(a, b);
        assert_ne!(rf_ab_scene_corners(0.0), rf_ab_scene_corners(0.2));
        let cam = rf_ab_camera();
        assert_eq!(cam.position, [0.0, 0.0]);
        let again = rf_ab_camera();
        assert_eq!(cam.view_proj_column_major(), again.view_proj_column_major());
    }

    #[test]
    fn rf_ab_panels_are_1to1_and_matched() {
        let layout = rf_ab_layout(true);
        let msaa_l = layout[0];
        let msaa_r = layout[1];
        assert_eq!(msaa_l.dest.width, msaa_r.dest.width);
        assert_eq!(msaa_l.dest.height, msaa_r.dest.height);
        assert_eq!(msaa_l.source, (RF_AB_PANEL_W, RF_AB_PANEL_H));
        assert_eq!(msaa_l.source, (msaa_l.dest.width, msaa_l.dest.height));
        assert_eq!(msaa_r.source, (msaa_r.dest.width, msaa_r.dest.height));
        assert_eq!(msaa_l.samples, 1);
        assert_eq!(msaa_r.samples, 4);
        assert!(msaa_l.nearest && msaa_r.nearest);

        let near = layout[2];
        let lin = layout[3];
        assert_eq!(near.source, lin.source);
        assert_eq!(near.dest.width, lin.dest.width);
        assert_eq!(near.source, (near.dest.width, near.dest.height));
        assert!(near.nearest && !lin.nearest);
        assert_eq!(near.samples, 1);

        let s100 = layout[4];
        let s200 = layout[5];
        let s400 = layout[6];
        assert_eq!(s100.source, (s100.dest.width, s100.dest.height));
        assert!(!s100.nearest && !s200.nearest && !s400.nearest);
        assert_eq!(s200.source, (640, 360));
        assert_eq!(s400.source, (1280, 720));
        assert_eq!(s200.source.0, s100.dest.width * 2);
        assert_eq!(s200.source.1, s100.dest.height * 2);
        assert_eq!(s400.source.0, s100.dest.width * 4);
        assert_eq!(s400.source.1, s100.dest.height * 4);
        assert_eq!(s100.dest.width, s200.dest.width);
        assert_eq!(s200.dest.width, s400.dest.width);
        assert_eq!(s100.samples, 4);
        assert_eq!(s200.samples, 4);
        assert_eq!(s400.samples, 4);
        assert_eq!(rf_ab_sample_cost(s200.source, 4), 640u64 * 360 * 4);
        assert_eq!(rf_ab_sample_cost(s400.source, 4), 1280u64 * 720 * 4);
        assert!(rf_ab_sample_cost(s400.source, 4) > rf_ab_sample_cost(s200.source, 4));
        assert!(rf_ab_sample_cost(s200.source, 4) > rf_ab_sample_cost(s100.source, 4));
        assert_eq!(RF3_SCALE_PERCENTS, [100, 200, 400]);
        assert!(
            !rf_ab_layout(true)
                .iter()
                .any(|panel| panel.label.contains("150"))
        );
        assert_eq!(rf_ab_scaled_extent(1280, 720, 200), (2560, 1440));
        assert_eq!(rf_ab_scaled_extent(1280, 720, 400), (5120, 2880));
    }

    #[test]
    fn rf_ab_layout_does_not_scale_sides_differently() {
        for panel in rf_ab_layout(true) {
            let aspect = panel.dest.width as f64 / panel.dest.height as f64;
            assert!((aspect - 16.0 / 9.0).abs() < 0.002);
            if panel.source == (panel.dest.width, panel.dest.height) {
                continue;
            }
            let sx = panel.source.0 as f64 / panel.dest.width as f64;
            let sy = panel.source.1 as f64 / panel.dest.height as f64;
            assert!((sx - sy).abs() < 1e-9, "non-uniform scale {} vs {}", sx, sy);
        }
    }

    #[test]
    fn rf_ab_panels_share_world_and_ndc_geometry() {
        let t = 0.85;
        let cam = rf_ab_camera();
        let corners = rf_ab_scene_corners(t);
        assert_eq!(corners, rf_ab_scene_corners(t));
        let vp_1 = PixelViewport {
            x: 0,
            y: 0,
            width: RF_AB_PANEL_W,
            height: RF_AB_PANEL_H,
        };
        let src_400 = rf_ab_panel_source(400);
        let vp_400 = PixelViewport {
            x: 0,
            y: 0,
            width: src_400.0,
            height: src_400.1,
        };
        for quad_corners in corners {
            let a = project_corners(quad_corners, &cam, vp_1, vp_1);
            let b = project_corners(quad_corners, &cam, vp_400, vp_1);
            assert_eq!(a.world, b.world);
            assert_eq!(a.ndc, b.ndc);
            assert_ne!(a.internal_px, b.internal_px);
        }
    }
}
