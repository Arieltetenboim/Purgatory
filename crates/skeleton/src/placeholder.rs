//! Canonical P4 torso placeholder geometry in **torso bone local space**.
//!
//! Pivot is the torso joint (bone origin). The trapezoid is not centered on its
//! visual AABB and is not derived from live pelvis/head world positions.
//!
//! - Bottom Y: hip in torso-local bind (`BIND_UPPER_LEG_FRONT.y - BIND_TORSO.y`)
//! - Top Y: authored torso→head span (`BIND_HEAD.y`)
//! - X: P4.2 RIGHT-facing 3/4 skew (far −X compressed, near +X exposed), then
//!   shifted so the four-corner centroid X is on the torso origin.
//!
//! Client debug draw and Animation Lab preview both use these corners. Changing
//! them changes bind-pose torso placement everywhere.

use crate::humanoid::{BIND_HEAD, BIND_TORSO, BIND_UPPER_LEG_FRONT};

/// P4.2 torso local X (unscaled), centroid-centered on the torso origin.
/// Winding BL, BR, TR, TL. Shape is the prior 3/4 trapezoid shifted by −0.0675.
pub const TORSO_LOCAL_BL_X: f32 = -0.1175;
pub const TORSO_LOCAL_BR_X: f32 = 0.0925;
pub const TORSO_LOCAL_TR_X: f32 = 0.1525;
pub const TORSO_LOCAL_TL_X: f32 = -0.1275;
/// Darker far-half split of the same trapezoid.
pub const TORSO_FAR_BR_X: f32 = -0.0525;
pub const TORSO_FAR_TR_X: f32 = -0.0475;

/// Height from hip (torso-local) to head-bind Y. Not `BIND_TORSO` itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TorsoPlaceholderLayout {
    pub height: f32,
    pub local_center: [f32; 2],
}

#[must_use]
pub fn torso_placeholder_layout() -> TorsoPlaceholderLayout {
    let top = BIND_HEAD.translation[1];
    let hip_y = BIND_UPPER_LEG_FRONT.translation[1] - BIND_TORSO.translation[1];
    let height = top - hip_y;
    let xs = [
        TORSO_LOCAL_BL_X,
        TORSO_LOCAL_BR_X,
        TORSO_LOCAL_TR_X,
        TORSO_LOCAL_TL_X,
    ];
    let cx = (xs[0] + xs[1] + xs[2] + xs[3]) * 0.25;
    let local_center = [cx, (top + hip_y) * 0.5];
    TorsoPlaceholderLayout {
        height,
        local_center,
    }
}

#[must_use]
pub fn torso_local_corners(scale: f32) -> [[f32; 2]; 4] {
    torso_corners(scale, TORSO_LOCAL_BR_X, TORSO_LOCAL_TR_X)
}

#[must_use]
pub fn torso_far_local_corners(scale: f32) -> [[f32; 2]; 4] {
    torso_corners(scale, TORSO_FAR_BR_X, TORSO_FAR_TR_X)
}

fn torso_corners(scale: f32, br: f32, tr: f32) -> [[f32; 2]; 4] {
    let layout = torso_placeholder_layout();
    let cy = layout.local_center[1] * scale;
    let hy = layout.height * scale * 0.5;
    [
        [TORSO_LOCAL_BL_X * scale, cy - hy],
        [br * scale, cy - hy],
        [tr * scale, cy + hy],
        [TORSO_LOCAL_TL_X * scale, cy + hy],
    ]
}
