//! Lab debug body overlay driven by evaluated [`WorldPose`].
//!
//! Torso trapezoid corners come from `purgatory-skeleton` (same bind-local layout
//! as client P4). The lab must not depend on the client crate. Limb segment panels
//! still span current parent→child world joints; head/hand/foot are bone-origin local.

use purgatory_skeleton::{
    BoneIndex, FOOT_BACK, FOOT_FRONT, HAND_BACK, HAND_FRONT, HEAD, LOWER_ARM_BACK, LOWER_ARM_FRONT,
    LOWER_LEG_BACK, LOWER_LEG_FRONT, ROOT, SkeletonDef, TORSO, UPPER_ARM_BACK, UPPER_ARM_FRONT,
    UPPER_LEG_BACK, UPPER_LEG_FRONT, WorldPose, torso_far_local_corners, torso_local_corners,
};

/// Same semantic body colors as client P4 placeholders (`skeleton_debug`).
pub const FOREARM_PLACEHOLDER_COLOR: [f32; 4] = [0.50, 0.26, 0.78, 0.95];
pub const UPPER_ARM_PLACEHOLDER_COLOR: [f32; 4] = [0.86, 0.68, 1.00, 0.95];
pub const HAND_PLACEHOLDER_COLOR: [f32; 4] = [1.00, 0.34, 0.48, 0.95];
pub const UPPER_LEG_PLACEHOLDER_COLOR: [f32; 4] = [0.62, 0.96, 0.50, 0.95];
pub const SHIN_PLACEHOLDER_COLOR: [f32; 4] = [0.22, 0.56, 0.34, 0.95];
pub const FOOT_PLACEHOLDER_COLOR: [f32; 4] = [1.00, 0.64, 0.24, 0.95];
pub const UPPER_LEG_BACK_PLACEHOLDER_COLOR: [f32; 4] = [0.20, 0.36, 0.22, 0.95];
pub const SHIN_BACK_PLACEHOLDER_COLOR: [f32; 4] = [0.10, 0.22, 0.16, 0.95];
pub const FOOT_BACK_PLACEHOLDER_COLOR: [f32; 4] = [0.42, 0.20, 0.10, 0.95];
pub const TORSO_PLACEHOLDER_COLOR: [f32; 4] = [0.76, 0.64, 0.50, 0.95];
pub const TORSO_FAR_PLACEHOLDER_COLOR: [f32; 4] = [0.52, 0.42, 0.32, 0.95];
pub const HEAD_PLACEHOLDER_COLOR: [f32; 4] = [0.86, 0.94, 0.96, 0.95];
pub const UPPER_ARM_BACK_PLACEHOLDER_COLOR: [f32; 4] = [0.36, 0.24, 0.42, 0.95];
pub const FOREARM_BACK_PLACEHOLDER_COLOR: [f32; 4] = [0.20, 0.10, 0.30, 0.95];
pub const HAND_BACK_PLACEHOLDER_COLOR: [f32; 4] = [0.40, 0.12, 0.22, 0.95];

const ARM_WIDTH: f32 = 0.0944;
const LEG_WIDTH: f32 = 0.0976;
const HAND_SIZE: [f32; 2] = [0.084, 0.096];
const FOOT_SIZE: [f32; 2] = [0.12, 0.069];
const HEAD_SIZE: [f32; 2] = [0.308, 0.32];

/// Convex panel in skeleton world space. Preview camera maps corners to pixels.
#[derive(Clone, Copy, Debug)]
pub struct DebugPanel {
    pub bone: BoneIndex,
    pub corners: [[f32; 2]; 4],
    pub color: [f32; 4],
}

/// Back → front, matching Phase 8 placeholder order: back arm, back leg, torso far,
/// torso, front leg, head, front arm.
#[must_use]
pub fn body_panels(_def: &SkeletonDef, world: &WorldPose) -> Vec<DebugPanel> {
    let mut out = Vec::with_capacity(15);
    push_limb(
        &mut out,
        world,
        LimbSpec {
            upper: UPPER_ARM_BACK,
            lower: LOWER_ARM_BACK,
            tip: HAND_BACK,
            width: ARM_WIDTH,
            upper_color: UPPER_ARM_BACK_PLACEHOLDER_COLOR,
            lower_color: FOREARM_BACK_PLACEHOLDER_COLOR,
            tip_color: HAND_BACK_PLACEHOLDER_COLOR,
            tip_size: HAND_SIZE,
            tip_is_foot: false,
        },
    );
    push_limb(
        &mut out,
        world,
        LimbSpec {
            upper: UPPER_LEG_BACK,
            lower: LOWER_LEG_BACK,
            tip: FOOT_BACK,
            width: LEG_WIDTH,
            upper_color: UPPER_LEG_BACK_PLACEHOLDER_COLOR,
            lower_color: SHIN_BACK_PLACEHOLDER_COLOR,
            tip_color: FOOT_BACK_PLACEHOLDER_COLOR,
            tip_size: FOOT_SIZE,
            tip_is_foot: true,
        },
    );
    if let Some(p) = torso_panel(world, true) {
        out.push(p);
    }
    if let Some(p) = torso_panel(world, false) {
        out.push(p);
    }
    push_limb(
        &mut out,
        world,
        LimbSpec {
            upper: UPPER_LEG_FRONT,
            lower: LOWER_LEG_FRONT,
            tip: FOOT_FRONT,
            width: LEG_WIDTH,
            upper_color: UPPER_LEG_PLACEHOLDER_COLOR,
            lower_color: SHIN_PLACEHOLDER_COLOR,
            tip_color: FOOT_PLACEHOLDER_COLOR,
            tip_size: FOOT_SIZE,
            tip_is_foot: true,
        },
    );
    if let Some(p) = head_panel(world) {
        out.push(p);
    }
    push_limb(
        &mut out,
        world,
        LimbSpec {
            upper: UPPER_ARM_FRONT,
            lower: LOWER_ARM_FRONT,
            tip: HAND_FRONT,
            width: ARM_WIDTH,
            upper_color: UPPER_ARM_PLACEHOLDER_COLOR,
            lower_color: FOREARM_PLACEHOLDER_COLOR,
            tip_color: HAND_PLACEHOLDER_COLOR,
            tip_size: HAND_SIZE,
            tip_is_foot: false,
        },
    );
    out
}

struct LimbSpec {
    upper: BoneIndex,
    lower: BoneIndex,
    tip: BoneIndex,
    width: f32,
    upper_color: [f32; 4],
    lower_color: [f32; 4],
    tip_color: [f32; 4],
    tip_size: [f32; 2],
    tip_is_foot: bool,
}

fn push_limb(out: &mut Vec<DebugPanel>, world: &WorldPose, spec: LimbSpec) {
    if let Some(p) = segment_panel(world, spec.upper, spec.lower, spec.width, spec.upper_color) {
        out.push(p);
    }
    if let Some(p) = segment_panel(world, spec.lower, spec.tip, spec.width, spec.lower_color) {
        out.push(p);
    }
    if let Some(p) = tip_panel(
        world,
        spec.tip,
        spec.tip_size,
        spec.tip_color,
        spec.tip_is_foot,
    ) {
        out.push(p);
    }
}

fn segment_panel(
    world: &WorldPose,
    bone: BoneIndex,
    child: BoneIndex,
    width: f32,
    color: [f32; 4],
) -> Option<DebugPanel> {
    let a = world.get(bone)?.translation;
    let b = world.get(child)?.translation;
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = dx.hypot(dy);
    if len < 1e-5 {
        return None;
    }
    let nx = -dy / len * width * 0.5;
    let ny = dx / len * width * 0.5;
    Some(DebugPanel {
        bone,
        corners: [
            [a[0] + nx, a[1] + ny],
            [a[0] - nx, a[1] - ny],
            [b[0] - nx, b[1] - ny],
            [b[0] + nx, b[1] + ny],
        ],
        color,
    })
}

fn tip_panel(
    world: &WorldPose,
    bone: BoneIndex,
    size: [f32; 2],
    color: [f32; 4],
    is_foot: bool,
) -> Option<DebugPanel> {
    let xf = world.get(bone)?;
    let (w, h) = (size[0], size[1]);
    // Bone-origin local. Hands hang along bone −Y from the wrist; feet extend
    // +X from the ankle. Not parent→child world-segment direction.
    let local = if is_foot {
        [[0.0, -h * 0.5], [w, -h * 0.5], [w, h * 0.5], [0.0, h * 0.5]]
    } else {
        [
            [-w * 0.5, -h],
            [w * 0.5, -h],
            [w * 0.5, 0.0],
            [-w * 0.5, 0.0],
        ]
    };
    Some(DebugPanel {
        bone,
        corners: local.map(|p| add(xf.translation, rotate(p, xf.rotation))),
        color,
    })
}

fn torso_panel(world: &WorldPose, far: bool) -> Option<DebugPanel> {
    let torso = world.get(TORSO)?;
    // Canonical P4 layout: corners in torso-bone local space, pivot = torso origin.
    let local = if far {
        torso_far_local_corners(1.0)
    } else {
        torso_local_corners(1.0)
    };
    Some(DebugPanel {
        bone: TORSO,
        corners: local.map(|p| add(torso.translation, rotate(p, torso.rotation))),
        color: if far {
            TORSO_FAR_PLACEHOLDER_COLOR
        } else {
            TORSO_PLACEHOLDER_COLOR
        },
    })
}

fn head_panel(world: &WorldPose) -> Option<DebugPanel> {
    let xf = world.get(HEAD)?;
    let (w, h) = (HEAD_SIZE[0], HEAD_SIZE[1]);
    let local = [
        [-w * 0.38, 0.0],
        [w * 0.38, 0.0],
        [w * 0.56, h],
        [-w * 0.22, h],
    ];
    Some(DebugPanel {
        bone: HEAD,
        corners: local.map(|p| add(xf.translation, rotate(p, xf.rotation))),
        color: HEAD_PLACEHOLDER_COLOR,
    })
}

fn rotate(p: [f32; 2], angle: f32) -> [f32; 2] {
    let (s, c) = angle.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

#[must_use]
pub fn is_back_bone(bone: BoneIndex) -> bool {
    matches!(
        bone,
        UPPER_ARM_BACK | LOWER_ARM_BACK | HAND_BACK | UPPER_LEG_BACK | LOWER_LEG_BACK | FOOT_BACK
    )
}

#[must_use]
pub fn root_translation(world: &WorldPose) -> [f32; 2] {
    world.get(ROOT).map(|t| t.translation).unwrap_or([0.0, 0.0])
}
