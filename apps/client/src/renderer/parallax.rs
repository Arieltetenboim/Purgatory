//! Procedural multi-layer development background (parallax).
//!
//! Presentation only. Offsets are `camera_position * factor`. Does not touch
//! simulation coordinates.

use purgatory_simulation::WorldBounds;

use super::camera::Camera;
use super::gpu::DrawQuad;

/// Far / mid / near parallax factors (world foreground = 1.0).
pub const PARALLAX_FAR: f32 = 0.15;
pub const PARALLAX_MID: f32 = 0.40;
pub const PARALLAX_NEAR: f32 = 0.70;

const FAR_COLOR: [f32; 4] = [0.06, 0.09, 0.16, 1.0];
const MID_COLOR: [f32; 4] = [0.09, 0.12, 0.20, 1.0];
const NEAR_COLOR: [f32; 4] = [0.12, 0.15, 0.24, 1.0];
const BAND_COLOR: [f32; 4] = [0.10, 0.14, 0.22, 1.0];

/// Build parallax quads for the current camera. Cheap, stack-friendly count.
#[must_use]
pub fn parallax_quads(camera: &Camera, bounds: WorldBounds) -> Vec<DrawQuad> {
    let mut quads = Vec::with_capacity(24);
    push_layer(
        &mut quads,
        camera,
        bounds,
        PARALLAX_FAR,
        FAR_COLOR,
        8.0,
        3.5,
    );
    push_layer(
        &mut quads,
        camera,
        bounds,
        PARALLAX_MID,
        MID_COLOR,
        5.0,
        2.2,
    );
    push_layer(
        &mut quads,
        camera,
        bounds,
        PARALLAX_NEAR,
        NEAR_COLOR,
        3.5,
        1.4,
    );
    // Subtle horizon band (near layer).
    let ox = camera.position[0] * PARALLAX_NEAR;
    let oy = camera.position[1] * PARALLAX_NEAR;
    quads.push(DrawQuad::rect(
        [ox, oy - 2.5],
        [bounds.width() + camera.viewport_width, 0.35],
        BAND_COLOR,
    ));
    quads
}

fn push_layer(
    quads: &mut Vec<DrawQuad>,
    camera: &Camera,
    bounds: WorldBounds,
    factor: f32,
    color: [f32; 4],
    spacing: f32,
    size: f32,
) {
    let ox = camera.position[0] * factor;
    let oy = camera.position[1] * factor * 0.5;
    let start = ((bounds.min_x / spacing).floor() as i32) - 1;
    let end = ((bounds.max_x / spacing).ceil() as i32) + 1;
    for i in start..=end {
        let x = i as f32 * spacing + ox;
        let y = -1.5 + ((i.rem_euclid(3)) as f32) * 1.1 + oy;
        quads.push(DrawQuad::rect([x, y], [size, size * 0.55], color));
    }
}

/// Sparse debug markers showing parallax layer origins (when toggled).
#[must_use]
pub fn parallax_debug_quads(camera: &Camera) -> Vec<DrawQuad> {
    let markers = [
        (PARALLAX_FAR, [1.0, 0.3, 0.3, 1.0]),
        (PARALLAX_MID, [0.3, 1.0, 0.3, 1.0]),
        (PARALLAX_NEAR, [0.3, 0.6, 1.0, 1.0]),
    ];
    markers
        .into_iter()
        .map(|(f, color)| {
            DrawQuad::rect(
                [camera.position[0] * f, camera.position[1] * f],
                [0.25, 0.25],
                color,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallax_produces_quads() {
        let cam = Camera::footnote_test_dev();
        let q = parallax_quads(&cam, WorldBounds::FOOTNOTE_TEST);
        assert!(q.len() >= 6);
    }
}
