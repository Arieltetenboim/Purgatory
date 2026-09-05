//! Client presentation adapter for one Humanoid v0 debug rig.
//!
//! Skeleton math stays in `purgatory-skeleton`. This module places the root at
//! the local presented pose and emits Stage D debug primitives (all 16 bones)
//! plus P4 placeholder body pieces. Skeleton joints/lines remain an optional
//! overlay underneath. Presentation scale is independent of bind / AABB.

use purgatory_animation::{a1_head_rotation_clip, sample};
use purgatory_simulation::PLAYER_HALF_EXTENTS;
use purgatory_skeleton::{
    BIND_FOOT_BACK, BIND_FOOT_FRONT, BIND_HAND_BACK, BIND_HAND_FRONT, BIND_LOWER_ARM_BACK,
    BIND_LOWER_ARM_FRONT, BIND_LOWER_LEG_BACK, BIND_LOWER_LEG_FRONT, BoneIndex, FOOT_BACK,
    FOOT_FRONT, HAND_BACK, HAND_FRONT, HEAD, LOWER_ARM_BACK, LOWER_ARM_FRONT, LOWER_LEG_BACK,
    LOWER_LEG_FRONT, LocalPose, PELVIS, ROOT, SkeletonDef, TORSO, UPPER_ARM_BACK, UPPER_ARM_FRONT,
    UPPER_LEG_BACK, UPPER_LEG_FRONT, WorldPose, evaluate, humanoid_v0, torso_far_local_corners,
    torso_local_corners,
};

use crate::renderer::DrawQuad;

const BONE_COLOR: [f32; 4] = [0.95, 0.82, 0.25, 1.0];
const BONE_COLOR_BACK: [f32; 4] = [0.52, 0.46, 0.22, 1.0];
const ROOT_JOINT_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const PELVIS_JOINT_COLOR: [f32; 4] = [0.95, 0.85, 0.2, 1.0];
const TORSO_JOINT_COLOR: [f32; 4] = [0.95, 0.55, 0.2, 1.0];
const HEAD_JOINT_COLOR: [f32; 4] = [0.35, 0.9, 0.95, 1.0];
const UPPER_LEG_FRONT_COLOR: [f32; 4] = [0.45, 0.95, 0.35, 1.0];
const LOWER_LEG_FRONT_COLOR: [f32; 4] = [0.2, 0.75, 0.45, 1.0];
const FOOT_FRONT_COLOR: [f32; 4] = [0.95, 0.3, 0.4, 1.0];
const UPPER_ARM_FRONT_COLOR: [f32; 4] = [0.75, 0.45, 0.95, 1.0];
const LOWER_ARM_FRONT_COLOR: [f32; 4] = [0.55, 0.32, 0.85, 1.0];
const HAND_FRONT_COLOR: [f32; 4] = [1.0, 0.45, 0.85, 1.0];
const FOREARM_PLACEHOLDER_COLOR: [f32; 4] = [0.50, 0.26, 0.78, 0.95];
const UPPER_ARM_PLACEHOLDER_COLOR: [f32; 4] = [0.86, 0.68, 1.00, 0.95];
const HAND_PLACEHOLDER_COLOR: [f32; 4] = [1.00, 0.34, 0.48, 0.95];
const UPPER_LEG_PLACEHOLDER_COLOR: [f32; 4] = [0.62, 0.96, 0.50, 0.95];
const SHIN_PLACEHOLDER_COLOR: [f32; 4] = [0.22, 0.56, 0.34, 0.95];
const FOOT_PLACEHOLDER_COLOR: [f32; 4] = [1.00, 0.64, 0.24, 0.95];
const UPPER_LEG_BACK_PLACEHOLDER_COLOR: [f32; 4] = [0.20, 0.36, 0.22, 0.95];
const SHIN_BACK_PLACEHOLDER_COLOR: [f32; 4] = [0.10, 0.22, 0.16, 0.95];
const FOOT_BACK_PLACEHOLDER_COLOR: [f32; 4] = [0.42, 0.20, 0.10, 0.95];
const TORSO_PLACEHOLDER_COLOR: [f32; 4] = [0.76, 0.64, 0.50, 0.95];
const TORSO_FAR_PLACEHOLDER_COLOR: [f32; 4] = [0.52, 0.42, 0.32, 0.95];
const HEAD_PLACEHOLDER_COLOR: [f32; 4] = [0.86, 0.94, 0.96, 0.95];
const UPPER_ARM_BACK_PLACEHOLDER_COLOR: [f32; 4] = [0.36, 0.24, 0.42, 0.95];
const FOREARM_BACK_PLACEHOLDER_COLOR: [f32; 4] = [0.20, 0.10, 0.30, 0.95];
const HAND_BACK_PLACEHOLDER_COLOR: [f32; 4] = [0.40, 0.12, 0.22, 0.95];
const ARM_PLACEHOLDER_WIDTH: f32 = 0.0944;
const LEG_PLACEHOLDER_WIDTH: f32 = 0.0976;
const HAND_PLACEHOLDER_SIZE: [f32; 2] = [0.084, 0.096];
/// Forward length along canonical +X from the ankle. Thickness only is calibrated.
const FOOT_PLACEHOLDER_SIZE: [f32; 2] = [0.12, 0.069];
/// Pre-P3.1 head panel. Bind `head` is unchanged; P3.1 only multiplies this visual.
const HEAD_PLACEHOLDER_BASE: [f32; 2] = [0.14, 0.16];
const HEAD_PLACEHOLDER_VISUAL_SCALE: f32 = 2.0;
const HEAD_PLACEHOLDER_WIDTH_SCALE: f32 = 1.10;
/// Short rectangle above the head joint (P3.1 visual ×2, then width +10%). Not a Slot.
const HEAD_PLACEHOLDER_SIZE: [f32; 2] = [
    HEAD_PLACEHOLDER_BASE[0] * HEAD_PLACEHOLDER_VISUAL_SCALE * HEAD_PLACEHOLDER_WIDTH_SCALE,
    HEAD_PLACEHOLDER_BASE[1] * HEAD_PLACEHOLDER_VISUAL_SCALE,
];
/// Identity scale for layout tests. Not offered in the Skeleton tab.
pub const CHARACTER_VISUAL_SCALE_1: f32 = 1.0;
/// Baseline character presentation size, about planted feet.
pub const CHARACTER_VISUAL_SCALE_115: f32 = 1.15;
/// Diagnostic inspection scale. Does not change the 1.15 baseline or skeleton data.
pub const CHARACTER_VISUAL_SCALE_2: f32 = 2.0;
const P1_PLACEHOLDERS: usize = 3;
const P1_BACK_PLACEHOLDERS: usize = 3;
const P2_PLACEHOLDERS: usize = 3;
const P2_BACK_PLACEHOLDERS: usize = 3;
const P3_PLACEHOLDERS: usize = 3;
const PLACEHOLDERS: usize = P1_PLACEHOLDERS
    + P1_BACK_PLACEHOLDERS
    + P2_PLACEHOLDERS
    + P2_BACK_PLACEHOLDERS
    + P3_PLACEHOLDERS;
const UPPER_LEG_BACK_COLOR: [f32; 4] = [0.28, 0.48, 0.32, 1.0];
const LOWER_LEG_BACK_COLOR: [f32; 4] = [0.18, 0.36, 0.28, 1.0];
const FOOT_BACK_COLOR: [f32; 4] = [0.52, 0.26, 0.28, 1.0];
const UPPER_ARM_BACK_COLOR: [f32; 4] = [0.40, 0.30, 0.50, 1.0];
const LOWER_ARM_BACK_COLOR: [f32; 4] = [0.30, 0.24, 0.40, 1.0];
const HAND_BACK_COLOR: [f32; 4] = [0.55, 0.30, 0.45, 1.0];

/// Small opaque joint dots. Two overlapping squares read as a circle at this size.
const JOINT_SIZE: f32 = 0.045;
const JOINT_ALPHA: f32 = 1.0;
const BAR_THICK: f32 = 0.05;

/// Stage D visible set: all 16 Humanoid v0 bones. Draw-order is far → near
/// (back limbs first) so back reads as the farther layer. Hierarchy is unchanged.
const STAGE_D_BONES: [BoneIndex; 16] = [
    UPPER_ARM_BACK,
    LOWER_ARM_BACK,
    HAND_BACK,
    UPPER_LEG_BACK,
    LOWER_LEG_BACK,
    FOOT_BACK,
    ROOT,
    PELVIS,
    TORSO,
    HEAD,
    UPPER_ARM_FRONT,
    LOWER_ARM_FRONT,
    HAND_FRONT,
    UPPER_LEG_FRONT,
    LOWER_LEG_FRONT,
    FOOT_FRONT,
];

/// Added to `foot_front` bind local rotation. Shin / upper leg locals are not written.
/// Rotation-only: translating the ankle would open a shin→foot gap on a rigid cutout.
const PROOF_FOOT_ROTATION: f32 = 0.70;

/// Added to `lower_leg_front` bind local rotation. Upper-leg local is not written.
const PROOF_SHIN_ROTATION: f32 = 0.90;

/// Added to `hand_front` bind local rotation. Forearm / upper arm / torso locals are not written.
/// Rotation-only: translating the wrist would open a forearm→hand gap on a rigid cutout.
const PROOF_HAND_ROTATION: f32 = 0.70;

/// Added to `lower_arm_front` bind local rotation. Upper-arm / torso locals are not written.
const PROOF_FOREARM_ROTATION: f32 = 0.90;

/// Off-by-default diagnostic pose. Not an animation system.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FrontLegProof {
    #[default]
    Off,
    /// `foot_front` local rotation about the ankle; `lower_leg_front` must stay put.
    FootIndependent,
    /// `lower_leg_front` local rotation; foot follows; upper leg stays put.
    ShinCarriesFoot,
}

impl FrontLegProof {
    pub const ALL: [Self; 3] = [Self::Off, Self::FootIndependent, Self::ShinCarriesFoot];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::FootIndependent => "Foot independent",
            Self::ShinCarriesFoot => "Shin carries foot",
        }
    }
}

/// Off-by-default diagnostic pose. Not an animation system.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FrontArmProof {
    #[default]
    Off,
    /// `hand_front` local rotation about the wrist; forearm / upper arm / torso must stay put.
    HandIndependent,
    /// `lower_arm_front` local rotation; hand follows; upper arm / torso stay put.
    ForearmCarriesHand,
}

impl FrontArmProof {
    pub const ALL: [Self; 3] = [Self::Off, Self::HandIndependent, Self::ForearmCarriesHand];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::HandIndependent => "Hand independent",
            Self::ForearmCarriesHand => "Forearm carries hand",
        }
    }
}

/// Authoring labels for Humanoid v0 indices. Not a closed bone enum.
#[allow(unused_imports)] // DEV skeleton inspect / diagnostics assemble
pub use purgatory_skeleton::HUMANOID_V0_BONE_LABELS;

/// Presentation mapping: player draw AABB center → skeleton root (feet).
/// Not a skeleton-core or collision contract.
#[must_use]
pub fn presented_root(body_center: [f32; 2]) -> [f32; 2] {
    [body_center[0], body_center[1] - PLAYER_HALF_EXTENTS[1]]
}

/// Y-up NDC (`Camera::world_to_ndc`) → Y-down window pixels.
#[must_use]
#[cfg(test)]
pub fn ndc_to_window_px(ndc: [f32; 2], window_w: u32, window_h: u32) -> [f32; 2] {
    [
        (ndc[0] * 0.5 + 0.5) * window_w as f32,
        (0.5 - ndc[1] * 0.5) * window_h as f32,
    ]
}

/// Caller-owned pose buffers for the single Stage D humanoid.
pub struct HumanoidDebug {
    local: LocalPose,
    world: WorldPose,
}

impl HumanoidDebug {
    #[must_use]
    pub fn new() -> Self {
        let def = humanoid_v0();
        Self {
            local: LocalPose::from_bind(def),
            world: WorldPose::new(def),
        }
    }

    /// Evaluate with resolved animation sample `t` (A1 slider or A2 player), then optional proofs.
    /// Root is always written last from the presented body center (adapter-owned).
    /// Full 16-bone pose is always evaluated; Stage D draws every bone.
    pub fn evaluate_at(
        &mut self,
        body_center: [f32; 2],
        leg_proof: FrontLegProof,
        arm_proof: FrontArmProof,
        a1_sample_t: f32,
    ) {
        let def = humanoid_v0();
        self.local
            .copy_bind(def)
            .expect("Stage D buffers are sized to Humanoid v0");
        sample(a1_head_rotation_clip(), a1_sample_t, &mut self.local)
            .expect("A1 clip bone_count matches Humanoid v0 LocalPose");
        apply_front_leg_proof(&mut self.local, leg_proof);
        apply_front_arm_proof(&mut self.local, arm_proof);
        let root = presented_root(body_center);
        if let Some(local_root) = self.local.get_mut(ROOT) {
            local_root.translation = root;
            local_root.rotation = 0.0;
        }
        evaluate(def, &self.local, &mut self.world)
            .expect("Stage D buffers are sized to Humanoid v0");
    }

    #[must_use]
    pub fn local(&self) -> &LocalPose {
        &self.local
    }

    #[must_use]
    pub fn world(&self) -> &WorldPose {
        &self.world
    }
}

fn apply_front_leg_proof(local: &mut LocalPose, proof: FrontLegProof) {
    match proof {
        FrontLegProof::Off => {}
        FrontLegProof::FootIndependent => {
            if let Some(foot) = local.get_mut(FOOT_FRONT) {
                foot.rotation += PROOF_FOOT_ROTATION;
            }
        }
        FrontLegProof::ShinCarriesFoot => {
            if let Some(shin) = local.get_mut(LOWER_LEG_FRONT) {
                shin.rotation += PROOF_SHIN_ROTATION;
            }
        }
    }
}

fn apply_front_arm_proof(local: &mut LocalPose, proof: FrontArmProof) {
    match proof {
        FrontArmProof::Off => {}
        FrontArmProof::HandIndependent => {
            if let Some(hand) = local.get_mut(HAND_FRONT) {
                hand.rotation += PROOF_HAND_ROTATION;
            }
        }
        FrontArmProof::ForearmCarriesHand => {
            if let Some(forearm) = local.get_mut(LOWER_ARM_FRONT) {
                forearm.rotation += PROOF_FOREARM_ROTATION;
            }
        }
    }
}

/// Stage D joints/connections plus P4 placeholder body.
/// Draw order (placeholders, back → front): back arm, back leg, torso far shade,
/// torso, front leg, head, front arm.
/// `preview_scale` is presentation-only (1.15 baseline, or 2.0 debug preview).
/// Joint markers are small opaque circular dots; connection bars get thinner as the figure grows.
/// Placeholders and joints are **world units** and draw in the scaled world pass
/// with platforms and parallax. Render Scale changes sampling, not authored size.
/// Placeholders draw after joints so the skeleton is an overlay underneath.
/// Diagonal connections stamp several thin AABBs (`DrawQuad` cannot rotate).
/// Binary draw path uses [`skeleton_overlay_quads`]; this wrapper is the test helper.
#[cfg(test)]
#[must_use]
pub fn skeleton_debug_quads(
    def: &SkeletonDef,
    world: &WorldPose,
    preview_scale: f32,
) -> Vec<DrawQuad> {
    skeleton_overlay_quads(def, world, preview_scale, true, true)
}

/// Same primitives as [`skeleton_debug_quads`], with independent skeleton / placeholder flags.
#[must_use]
pub fn skeleton_overlay_quads(
    def: &SkeletonDef,
    world: &WorldPose,
    preview_scale: f32,
    draw_skeleton: bool,
    draw_placeholders: bool,
) -> Vec<DrawQuad> {
    let mut quads = Vec::with_capacity(96);
    if world.bone_count() != def.bone_count() {
        return quads;
    }
    let scale = sanitize_preview_scale(preview_scale);
    let root = world.get(ROOT).map(|t| t.translation).unwrap_or([0.0, 0.0]);
    let bar_thick = BAR_THICK / scale;

    if draw_skeleton {
        for idx in STAGE_D_BONES {
            if idx == ROOT {
                continue;
            }
            let Some(parent) = def.parent(idx) else {
                continue;
            };
            let Some(bone) = world.get(idx) else {
                continue;
            };
            let Some(parent_t) = world.get(parent) else {
                continue;
            };
            push_thin_segment(
                &mut quads,
                scale_about_root(parent_t.translation, root, scale),
                scale_about_root(bone.translation, root, scale),
                bar_thick,
                connection_color(idx),
            );
        }

        for idx in STAGE_D_BONES {
            let Some(bone) = world.get(idx) else {
                continue;
            };
            let color = stage_d_joint_color(idx);
            push_joint_dot(
                &mut quads,
                scale_about_root(bone.translation, root, scale),
                color,
            );
        }
    }

    if draw_placeholders {
        let n_before = quads.len();
        for panel in p1_back_arm_placeholders(world, root, scale) {
            quads.push(panel);
        }
        for panel in p2_back_leg_placeholders(world, root, scale) {
            quads.push(panel);
        }
        if let Some(q) = p3_torso_far_placeholder(world, root, scale) {
            quads.push(q);
        }
        if let Some(q) = p3_torso_placeholder(world, root, scale) {
            quads.push(q);
        }
        for panel in p2_front_leg_placeholders(world, root, scale) {
            quads.push(panel);
        }
        if let Some(q) = p3_head_placeholder(world, root, scale) {
            quads.push(q);
        }
        for panel in p1_front_arm_placeholders(world, root, scale) {
            quads.push(panel);
        }
        debug_assert_eq!(quads.len() - n_before, PLACEHOLDERS);
    }
    quads
}

/// P4 placeholders keyed by bone. Hidden bones emit nothing (torso far+near both skip).
#[cfg(test)]
#[must_use]
pub fn character_placeholder_quads(
    world: &WorldPose,
    preview_scale: f32,
    hide_bone: impl Fn(BoneIndex) -> bool,
) -> Vec<DrawQuad> {
    let mut quads = Vec::with_capacity(PLACEHOLDERS);
    let scale = sanitize_preview_scale(preview_scale);
    let root = world.get(ROOT).map(|t| t.translation).unwrap_or([0.0, 0.0]);
    let push = |quads: &mut Vec<DrawQuad>, bone: BoneIndex, quad: Option<DrawQuad>| {
        if !hide_bone(bone)
            && let Some(q) = quad
        {
            quads.push(q);
        }
    };
    push(
        &mut quads,
        UPPER_ARM_BACK,
        p1_upper_arm_back_placeholder(world, root, scale),
    );
    push(
        &mut quads,
        LOWER_ARM_BACK,
        p1_forearm_back_placeholder(world, root, scale),
    );
    push(
        &mut quads,
        HAND_BACK,
        p1_hand_back_placeholder(world, root, scale),
    );
    push(
        &mut quads,
        UPPER_LEG_BACK,
        p2_upper_leg_back_placeholder(world, root, scale),
    );
    push(
        &mut quads,
        LOWER_LEG_BACK,
        p2_shin_back_placeholder(world, root, scale),
    );
    push(
        &mut quads,
        FOOT_BACK,
        p2_foot_back_placeholder(world, root, scale),
    );
    if !hide_bone(TORSO) {
        if let Some(q) = p3_torso_far_placeholder(world, root, scale) {
            quads.push(q);
        }
        if let Some(q) = p3_torso_placeholder(world, root, scale) {
            quads.push(q);
        }
    }
    push(
        &mut quads,
        UPPER_LEG_FRONT,
        p2_upper_leg_front_placeholder(world, root, scale),
    );
    push(
        &mut quads,
        LOWER_LEG_FRONT,
        p2_shin_front_placeholder(world, root, scale),
    );
    push(
        &mut quads,
        FOOT_FRONT,
        p2_foot_front_placeholder(world, root, scale),
    );
    push(&mut quads, HEAD, p3_head_placeholder(world, root, scale));
    push(
        &mut quads,
        UPPER_ARM_FRONT,
        p1_upper_arm_front_placeholder(world, root, scale),
    );
    push(
        &mut quads,
        LOWER_ARM_FRONT,
        p1_forearm_front_placeholder(world, root, scale),
    );
    push(
        &mut quads,
        HAND_FRONT,
        p1_hand_front_placeholder(world, root, scale),
    );
    quads
}

/// One P4 placeholder panel. `torso_far` is only used when `bone == TORSO`.
#[must_use]
pub fn character_placeholder_quad(
    world: &WorldPose,
    preview_scale: f32,
    bone: BoneIndex,
    torso_far: bool,
) -> Option<DrawQuad> {
    let scale = sanitize_preview_scale(preview_scale);
    let root = world.get(ROOT).map(|t| t.translation).unwrap_or([0.0, 0.0]);
    match bone {
        TORSO if torso_far => p3_torso_far_placeholder(world, root, scale),
        TORSO => p3_torso_placeholder(world, root, scale),
        UPPER_ARM_BACK => p1_upper_arm_back_placeholder(world, root, scale),
        LOWER_ARM_BACK => p1_forearm_back_placeholder(world, root, scale),
        HAND_BACK => p1_hand_back_placeholder(world, root, scale),
        UPPER_LEG_BACK => p2_upper_leg_back_placeholder(world, root, scale),
        LOWER_LEG_BACK => p2_shin_back_placeholder(world, root, scale),
        FOOT_BACK => p2_foot_back_placeholder(world, root, scale),
        UPPER_LEG_FRONT => p2_upper_leg_front_placeholder(world, root, scale),
        LOWER_LEG_FRONT => p2_shin_front_placeholder(world, root, scale),
        FOOT_FRONT => p2_foot_front_placeholder(world, root, scale),
        HEAD => p3_head_placeholder(world, root, scale),
        UPPER_ARM_FRONT => p1_upper_arm_front_placeholder(world, root, scale),
        LOWER_ARM_FRONT => p1_forearm_front_placeholder(world, root, scale),
        HAND_FRONT => p1_hand_front_placeholder(world, root, scale),
        _ => None,
    }
}

/// P1: upper arm, forearm, hand. Direct bone attachment. Not Slots.
fn p1_front_arm_placeholders(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Vec<DrawQuad> {
    let mut out = Vec::with_capacity(P1_PLACEHOLDERS);
    if let Some(q) = p1_upper_arm_front_placeholder(world, root, preview_scale) {
        out.push(q);
    }
    if let Some(q) = p1_forearm_front_placeholder(world, root, preview_scale) {
        out.push(q);
    }
    if let Some(q) = p1_hand_front_placeholder(world, root, preview_scale) {
        out.push(q);
    }
    out
}

fn p1_upper_arm_front_placeholder(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Option<DrawQuad> {
    placeholder_bone_toward_child(
        world,
        root,
        preview_scale,
        UPPER_ARM_FRONT,
        BIND_LOWER_ARM_FRONT.translation,
        ARM_PLACEHOLDER_WIDTH,
        UPPER_ARM_PLACEHOLDER_COLOR,
    )
}

fn p1_forearm_front_placeholder(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Option<DrawQuad> {
    placeholder_bone_toward_child(
        world,
        root,
        preview_scale,
        LOWER_ARM_FRONT,
        BIND_HAND_FRONT.translation,
        ARM_PLACEHOLDER_WIDTH,
        FOREARM_PLACEHOLDER_COLOR,
    )
}

fn p1_hand_front_placeholder(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Option<DrawQuad> {
    let bone = world.get(HAND_FRONT)?;
    let scale = sanitize_preview_scale(preview_scale);
    let size = [
        HAND_PLACEHOLDER_SIZE[0] * scale,
        HAND_PLACEHOLDER_SIZE[1] * scale,
    ];
    let local_center = [0.0, -size[1] * 0.5];
    let pivot = scale_about_root(bone.translation, root, scale);
    Some(DrawQuad::oriented(
        pivot,
        size,
        local_center,
        bone.rotation,
        HAND_PLACEHOLDER_COLOR,
    ))
}

/// P4 back arm: same child-span / wrist-pivot rules as the front arm. Farther draw layer.
fn p1_back_arm_placeholders(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Vec<DrawQuad> {
    let mut out = Vec::with_capacity(P1_BACK_PLACEHOLDERS);
    if let Some(q) = p1_upper_arm_back_placeholder(world, root, preview_scale) {
        out.push(q);
    }
    if let Some(q) = p1_forearm_back_placeholder(world, root, preview_scale) {
        out.push(q);
    }
    if let Some(q) = p1_hand_back_placeholder(world, root, preview_scale) {
        out.push(q);
    }
    out
}

fn p1_upper_arm_back_placeholder(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Option<DrawQuad> {
    placeholder_bone_toward_child(
        world,
        root,
        preview_scale,
        UPPER_ARM_BACK,
        BIND_LOWER_ARM_BACK.translation,
        ARM_PLACEHOLDER_WIDTH,
        UPPER_ARM_BACK_PLACEHOLDER_COLOR,
    )
}

fn p1_forearm_back_placeholder(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Option<DrawQuad> {
    placeholder_bone_toward_child(
        world,
        root,
        preview_scale,
        LOWER_ARM_BACK,
        BIND_HAND_BACK.translation,
        ARM_PLACEHOLDER_WIDTH,
        FOREARM_BACK_PLACEHOLDER_COLOR,
    )
}

fn p1_hand_back_placeholder(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Option<DrawQuad> {
    let bone = world.get(HAND_BACK)?;
    let scale = sanitize_preview_scale(preview_scale);
    let size = [
        HAND_PLACEHOLDER_SIZE[0] * scale,
        HAND_PLACEHOLDER_SIZE[1] * scale,
    ];
    let local_center = [0.0, -size[1] * 0.5];
    let pivot = scale_about_root(bone.translation, root, scale);
    Some(DrawQuad::oriented(
        pivot,
        size,
        local_center,
        bone.rotation,
        HAND_BACK_PLACEHOLDER_COLOR,
    ))
}

/// P2: upper leg, shin, foot. Direct bone attachment. Not Slots.
fn p2_front_leg_placeholders(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Vec<DrawQuad> {
    let mut out = Vec::with_capacity(P2_PLACEHOLDERS);
    if let Some(q) = p2_upper_leg_front_placeholder(world, root, preview_scale) {
        out.push(q);
    }
    if let Some(q) = p2_shin_front_placeholder(world, root, preview_scale) {
        out.push(q);
    }
    if let Some(q) = p2_foot_front_placeholder(world, root, preview_scale) {
        out.push(q);
    }
    out
}

fn p2_upper_leg_front_placeholder(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Option<DrawQuad> {
    placeholder_bone_toward_child(
        world,
        root,
        preview_scale,
        UPPER_LEG_FRONT,
        BIND_LOWER_LEG_FRONT.translation,
        LEG_PLACEHOLDER_WIDTH,
        UPPER_LEG_PLACEHOLDER_COLOR,
    )
}

fn p2_shin_front_placeholder(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Option<DrawQuad> {
    placeholder_bone_toward_child(
        world,
        root,
        preview_scale,
        LOWER_LEG_FRONT,
        BIND_FOOT_FRONT.translation,
        LEG_PLACEHOLDER_WIDTH,
        SHIN_PLACEHOLDER_COLOR,
    )
}

fn p2_foot_front_placeholder(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Option<DrawQuad> {
    let bone = world.get(FOOT_FRONT)?;
    let scale = sanitize_preview_scale(preview_scale);
    let size = [
        FOOT_PLACEHOLDER_SIZE[0] * scale,
        FOOT_PLACEHOLDER_SIZE[1] * scale,
    ];
    // Canonical RIGHT-facing +X. Forward length is this visual, not foot bind T.
    let local_center = [size[0] * 0.5, 0.0];
    let pivot = scale_about_root(bone.translation, root, scale);
    Some(DrawQuad::oriented(
        pivot,
        size,
        local_center,
        bone.rotation,
        FOOT_PLACEHOLDER_COLOR,
    ))
}

/// P2 back: same child-span rule as the front leg. Farther draw layer. Not Slots.
fn p2_back_leg_placeholders(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Vec<DrawQuad> {
    let mut out = Vec::with_capacity(P2_BACK_PLACEHOLDERS);
    if let Some(q) = p2_upper_leg_back_placeholder(world, root, preview_scale) {
        out.push(q);
    }
    if let Some(q) = p2_shin_back_placeholder(world, root, preview_scale) {
        out.push(q);
    }
    if let Some(q) = p2_foot_back_placeholder(world, root, preview_scale) {
        out.push(q);
    }
    out
}

fn p2_upper_leg_back_placeholder(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Option<DrawQuad> {
    placeholder_bone_toward_child(
        world,
        root,
        preview_scale,
        UPPER_LEG_BACK,
        BIND_LOWER_LEG_BACK.translation,
        LEG_PLACEHOLDER_WIDTH,
        UPPER_LEG_BACK_PLACEHOLDER_COLOR,
    )
}

fn p2_shin_back_placeholder(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Option<DrawQuad> {
    placeholder_bone_toward_child(
        world,
        root,
        preview_scale,
        LOWER_LEG_BACK,
        BIND_FOOT_BACK.translation,
        LEG_PLACEHOLDER_WIDTH,
        SHIN_BACK_PLACEHOLDER_COLOR,
    )
}

fn p2_foot_back_placeholder(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Option<DrawQuad> {
    let bone = world.get(FOOT_BACK)?;
    let scale = sanitize_preview_scale(preview_scale);
    let size = [
        FOOT_PLACEHOLDER_SIZE[0] * scale,
        FOOT_PLACEHOLDER_SIZE[1] * scale,
    ];
    let local_center = [size[0] * 0.5, 0.0];
    let pivot = scale_about_root(bone.translation, root, scale);
    Some(DrawQuad::oriented(
        pivot,
        size,
        local_center,
        bone.rotation,
        FOOT_BACK_PLACEHOLDER_COLOR,
    ))
}

fn p3_torso_far_placeholder(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
) -> Option<DrawQuad> {
    let bone = world.get(TORSO)?;
    let scale = sanitize_preview_scale(preview_scale);
    let pivot = scale_about_root(bone.translation, root, scale);
    Some(DrawQuad::convex(
        pivot,
        torso_far_local_corners(scale),
        bone.rotation,
        TORSO_FAR_PLACEHOLDER_COLOR,
    ))
}

fn p3_torso_placeholder(world: &WorldPose, root: [f32; 2], preview_scale: f32) -> Option<DrawQuad> {
    let bone = world.get(TORSO)?;
    let scale = sanitize_preview_scale(preview_scale);
    let pivot = scale_about_root(bone.translation, root, scale);
    Some(DrawQuad::convex(
        pivot,
        torso_local_corners(scale),
        bone.rotation,
        TORSO_PLACEHOLDER_COLOR,
    ))
}

fn head_local_corners(scale: f32) -> [[f32; 2]; 4] {
    let w = HEAD_PLACEHOLDER_SIZE[0] * scale;
    let h = HEAD_PLACEHOLDER_SIZE[1] * scale;
    [
        [-w * 0.38, 0.0],
        [w * 0.38, 0.0],
        [w * 0.56, h],
        [-w * 0.22, h],
    ]
}

fn p3_head_placeholder(world: &WorldPose, root: [f32; 2], preview_scale: f32) -> Option<DrawQuad> {
    let bone = world.get(HEAD)?;
    let scale = sanitize_preview_scale(preview_scale);
    let pivot = scale_about_root(bone.translation, root, scale);
    Some(DrawQuad::convex(
        pivot,
        head_local_corners(scale),
        bone.rotation,
        HEAD_PLACEHOLDER_COLOR,
    ))
}

/// Length and direction from authored **child** rest translation, not this bone's
/// parent-relative local. Local `-Y` of the panel is aligned with that child offset.
fn placeholder_bone_toward_child(
    world: &WorldPose,
    root: [f32; 2],
    preview_scale: f32,
    bone: BoneIndex,
    child_rest: [f32; 2],
    width: f32,
    color: [f32; 4],
) -> Option<DrawQuad> {
    let xf = world.get(bone)?;
    let scale = sanitize_preview_scale(preview_scale);
    let length = child_rest[0].hypot(child_rest[1]) * scale;
    let size = [width * scale, length];
    let local_center = [0.0, -size[1] * 0.5];
    let pivot = scale_about_root(xf.translation, root, scale);
    let rotation = xf.rotation + child_rest[0].atan2(-child_rest[1]);
    Some(DrawQuad::oriented(
        pivot,
        size,
        local_center,
        rotation,
        color,
    ))
}

/// Debug preview only. `p' = root + (p - root) * scale`. Does not write Local/World pose.
#[must_use]
pub fn scale_about_root(point: [f32; 2], root: [f32; 2], scale: f32) -> [f32; 2] {
    let s = sanitize_preview_scale(scale);
    [
        root[0] + (point[0] - root[0]) * s,
        root[1] + (point[1] - root[1]) * s,
    ]
}

#[must_use]
pub fn character_visual_scale(debug_preview_2x: bool) -> f32 {
    if debug_preview_2x {
        CHARACTER_VISUAL_SCALE_2
    } else {
        CHARACTER_VISUAL_SCALE_115
    }
}

#[must_use]
pub fn sanitize_preview_scale(scale: f32) -> f32 {
    if (scale - CHARACTER_VISUAL_SCALE_2).abs() < 1e-4 {
        CHARACTER_VISUAL_SCALE_2
    } else if (scale - CHARACTER_VISUAL_SCALE_1).abs() < 1e-4 {
        CHARACTER_VISUAL_SCALE_1
    } else {
        CHARACTER_VISUAL_SCALE_115
    }
}

/// Local-player presentation AABB center. Simulation AABB is not scaled; feet stay at `presented_root`.
#[must_use]
pub fn preview_local_player_center(body_center: [f32; 2], scale: f32) -> [f32; 2] {
    let s = sanitize_preview_scale(scale);
    let root = presented_root(body_center);
    [root[0], root[1] + PLAYER_HALF_EXTENTS[1] * s]
}

/// Full width/height of the local-player presentation quad at preview scale.
#[must_use]
pub fn preview_local_player_size(scale: f32) -> [f32; 2] {
    let s = sanitize_preview_scale(scale);
    [
        PLAYER_HALF_EXTENTS[0] * 2.0 * s,
        PLAYER_HALF_EXTENTS[1] * 2.0 * s,
    ]
}

fn is_back_limb(idx: BoneIndex) -> bool {
    idx == UPPER_ARM_BACK
        || idx == LOWER_ARM_BACK
        || idx == HAND_BACK
        || idx == UPPER_LEG_BACK
        || idx == LOWER_LEG_BACK
        || idx == FOOT_BACK
}

fn connection_color(child: BoneIndex) -> [f32; 4] {
    if is_back_limb(child) {
        BONE_COLOR_BACK
    } else {
        BONE_COLOR
    }
}

fn stage_d_joint_color(idx: BoneIndex) -> [f32; 4] {
    if idx == ROOT {
        ROOT_JOINT_COLOR
    } else if idx == PELVIS {
        PELVIS_JOINT_COLOR
    } else if idx == TORSO {
        TORSO_JOINT_COLOR
    } else if idx == HEAD {
        HEAD_JOINT_COLOR
    } else if idx == UPPER_LEG_FRONT {
        UPPER_LEG_FRONT_COLOR
    } else if idx == LOWER_LEG_FRONT {
        LOWER_LEG_FRONT_COLOR
    } else if idx == FOOT_FRONT {
        FOOT_FRONT_COLOR
    } else if idx == UPPER_ARM_FRONT {
        UPPER_ARM_FRONT_COLOR
    } else if idx == LOWER_ARM_FRONT {
        LOWER_ARM_FRONT_COLOR
    } else if idx == HAND_FRONT {
        HAND_FRONT_COLOR
    } else if idx == UPPER_LEG_BACK {
        UPPER_LEG_BACK_COLOR
    } else if idx == LOWER_LEG_BACK {
        LOWER_LEG_BACK_COLOR
    } else if idx == FOOT_BACK {
        FOOT_BACK_COLOR
    } else if idx == UPPER_ARM_BACK {
        UPPER_ARM_BACK_COLOR
    } else if idx == LOWER_ARM_BACK {
        LOWER_ARM_BACK_COLOR
    } else if idx == HAND_BACK {
        HAND_BACK_COLOR
    } else {
        BONE_COLOR
    }
}

fn push_joint_dot(quads: &mut Vec<DrawQuad>, center: [f32; 2], mut color: [f32; 4]) {
    color[3] = JOINT_ALPHA;
    let size = [JOINT_SIZE, JOINT_SIZE];
    quads.push(DrawQuad::oriented(center, size, [0.0, 0.0], 0.0, color));
    quads.push(DrawQuad::oriented(
        center,
        size,
        [0.0, 0.0],
        std::f32::consts::FRAC_PI_4,
        color,
    ));
}

/// Axis-aligned parent–child uses one thin bar. Diagonals are stamped as small
/// squares along the segment so `DrawQuad` AABBs cannot fill the bounding box.
fn push_thin_segment(
    quads: &mut Vec<DrawQuad>,
    a: [f32; 2],
    b: [f32; 2],
    thickness: f32,
    color: [f32; 4],
) {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    if dx.abs() <= thickness || dy.abs() <= thickness {
        quads.push(DrawQuad::rect(
            [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5],
            [dx.abs().max(thickness), dy.abs().max(thickness)],
            color,
        ));
        return;
    }
    let len = (dx * dx + dy * dy).sqrt();
    let step = (thickness * 0.75).max(0.016);
    let n = ((len / step).ceil() as usize).clamp(1, 10);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        quads.push(DrawQuad::rect(
            [a[0] + dx * t, a[1] + dy * t],
            [thickness, thickness],
            color,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::MAX_QUADS;
    use purgatory_skeleton::{
        BIND_HEAD, BIND_TORSO, BIND_UPPER_LEG_FRONT, TORSO_LOCAL_BL_X, TORSO_LOCAL_BR_X,
        TORSO_LOCAL_TL_X, TORSO_LOCAL_TR_X, torso_placeholder_layout,
    };

    const EPS: f32 = 1e-4;
    const STAGE_D_JOINTS: usize = 16;
    const STAGE_D_JOINT_QUADS: usize = STAGE_D_JOINTS * 2;
    const STAGE_D_CONNECTIONS: usize = 15;

    fn tr_eq(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < EPS && (a[1] - b[1]).abs() < EPS
    }

    fn eval(rig: &mut HumanoidDebug, center: [f32; 2], leg: FrontLegProof, arm: FrontArmProof) {
        rig.evaluate_at(center, leg, arm, 0.0);
    }

    #[test]
    fn presented_root_uses_body_aabb_feet() {
        let center = [3.0, 2.0];
        let root = presented_root(center);
        assert!((root[0] - 3.0).abs() < 1e-5);
        assert!((root[1] - (2.0 - PLAYER_HALF_EXTENTS[1])).abs() < 1e-5);
    }

    #[test]
    fn evaluate_places_root_at_presented_feet() {
        let mut rig = HumanoidDebug::new();
        let center = [1.5, -0.4];
        eval(&mut rig, center, FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap();
        let expect = presented_root(center);
        assert!((root.translation[0] - expect[0]).abs() < EPS);
        assert!((root.translation[1] - expect[1]).abs() < EPS);
    }

    fn joint_quads(quads: &[DrawQuad]) -> &[DrawQuad] {
        let n = quads.len();
        assert!(
            n >= STAGE_D_JOINT_QUADS + PLACEHOLDERS,
            "expected joints then placeholders, got {n}"
        );
        &quads[n - STAGE_D_JOINT_QUADS - PLACEHOLDERS..n - PLACEHOLDERS]
    }

    fn find_joint(joints: &[DrawQuad], center: [f32; 2]) -> &DrawQuad {
        joints
            .iter()
            .find(|q| tr_eq(q.center, center))
            .unwrap_or_else(|| panic!("missing joint at ({:.4}, {:.4})", center[0], center[1]))
    }

    fn visible_parent_child_pairs(def: &SkeletonDef) -> Vec<(BoneIndex, BoneIndex)> {
        STAGE_D_BONES
            .iter()
            .copied()
            .filter_map(|idx| {
                if idx == ROOT {
                    None
                } else {
                    def.parent(idx).map(|parent| (idx, parent))
                }
            })
            .collect()
    }

    const BACK_LIMBS: [BoneIndex; 6] = [
        UPPER_ARM_BACK,
        LOWER_ARM_BACK,
        HAND_BACK,
        UPPER_LEG_BACK,
        LOWER_LEG_BACK,
        FOOT_BACK,
    ];

    #[test]
    fn stage_d_visible_set_is_all_sixteen_bones() {
        let mut seen = [false; 16];
        for idx in STAGE_D_BONES {
            seen[idx.as_usize()] = true;
        }
        assert!(seen.iter().all(|&v| v));
        assert_eq!(STAGE_D_BONES.len(), STAGE_D_JOINTS);
    }

    #[test]
    fn stage_d_emits_sixteen_joints_and_fifteen_connections() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let def = humanoid_v0();
        let pairs = visible_parent_child_pairs(def);
        assert_eq!(pairs.len(), STAGE_D_CONNECTIONS);
        for scale in [
            CHARACTER_VISUAL_SCALE_1,
            CHARACTER_VISUAL_SCALE_115,
            CHARACTER_VISUAL_SCALE_2,
        ] {
            let quads = skeleton_debug_quads(def, rig.world(), scale);
            assert_eq!(joint_quads(&quads).len(), STAGE_D_JOINT_QUADS);
            assert!(quads.len() >= STAGE_D_JOINTS + STAGE_D_CONNECTIONS);
            assert!(
                quads.len() < MAX_QUADS,
                "Stage D debug quads {} must stay inside MAX_QUADS {MAX_QUADS}",
                quads.len()
            );
            assert!(
                quads.len() <= MAX_QUADS * 3 / 4,
                "Stage D debug quads {} should stay well below MAX_QUADS {MAX_QUADS}",
                quads.len()
            );
        }
    }

    #[test]
    fn joints_are_small_opaque_circular_dots() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let quads = skeleton_debug_quads(humanoid_v0(), rig.world(), CHARACTER_VISUAL_SCALE_1);
        let joints = joint_quads(&quads);
        assert_eq!(joints.len(), STAGE_D_JOINT_QUADS);
        const { assert!(JOINT_SIZE < 0.08) };
        assert!((JOINT_ALPHA - 1.0).abs() < EPS);
        for pair in joints.as_chunks::<2>().0 {
            assert!(tr_eq(pair[0].center, pair[1].center));
            assert_eq!(pair[0].size, [JOINT_SIZE, JOINT_SIZE]);
            assert_eq!(pair[1].size, [JOINT_SIZE, JOINT_SIZE]);
            assert!((pair[0].color[3] - 1.0).abs() < EPS);
            assert!((pair[1].color[3] - 1.0).abs() < EPS);
            assert!(!pair[0].triangle);
            assert!(!pair[1].triangle);
            let a = pair[0].world_corners();
            let b = pair[1].world_corners();
            assert!(
                (a[0][0] - b[0][0]).abs() > EPS || (a[0][1] - b[0][1]).abs() > EPS,
                "octagon dots need a rotated square on top of the axis-aligned one"
            );
        }
        let root_dot = find_joint(joints, root);
        assert!(tr_eq(root_dot.center, root));
    }

    #[test]
    fn overlay_size_and_thickness_are_world_units() {
        assert!((JOINT_SIZE - 0.045).abs() < EPS);
        assert!((BAR_THICK - 0.05).abs() < EPS);
        assert!((ARM_PLACEHOLDER_WIDTH - 0.0944).abs() < EPS);
        let mut rig = HumanoidDebug::new();
        eval(
            &mut rig,
            [1.0, -2.0],
            FrontLegProof::Off,
            FrontArmProof::Off,
        );
        let scale = CHARACTER_VISUAL_SCALE_115;
        let quads = skeleton_debug_quads(humanoid_v0(), rig.world(), scale);
        let joints = joint_quads(&quads);
        for pair in joints.as_chunks::<2>().0 {
            assert_eq!(pair[0].size, [JOINT_SIZE, JOINT_SIZE]);
        }
        let arm = quads
            .iter()
            .find(|q| (q.size[0] - ARM_PLACEHOLDER_WIDTH * scale).abs() < EPS)
            .expect("front/back arm placeholder width");
        assert!((arm.size[0] - ARM_PLACEHOLDER_WIDTH * scale).abs() < EPS);
        let expected_bar = BAR_THICK / scale;
        let bar = quads
            .iter()
            .find(|q| {
                (q.size[0] - expected_bar).abs() < EPS && (q.size[1] - expected_bar).abs() < EPS
            })
            .expect("connection stamp uses world-unit thickness");
        assert!((bar.size[0] - expected_bar).abs() < EPS);
    }

    #[test]
    fn stage_d_joints_cover_every_bone_world_translation() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let def = humanoid_v0();
        let quads = skeleton_debug_quads(def, rig.world(), 1.0);
        let joints = joint_quads(&quads);
        for i in 0u8..16 {
            let idx = BoneIndex::from_u8(i);
            let world = rig.world().get(idx).unwrap().translation;
            let _ = find_joint(joints, world);
        }
    }

    #[test]
    fn stage_d_back_arm_chain_parents_are_correct() {
        let def = humanoid_v0();
        let pairs = visible_parent_child_pairs(def);
        assert!(pairs.contains(&(UPPER_ARM_BACK, TORSO)));
        assert!(pairs.contains(&(LOWER_ARM_BACK, UPPER_ARM_BACK)));
        assert!(pairs.contains(&(HAND_BACK, LOWER_ARM_BACK)));
        assert!(!pairs.contains(&(UPPER_ARM_BACK, UPPER_ARM_FRONT)));
        assert!(!pairs.contains(&(LOWER_ARM_BACK, LOWER_ARM_FRONT)));
        assert!(!pairs.contains(&(HAND_BACK, HAND_FRONT)));
    }

    #[test]
    fn stage_d_back_leg_chain_parents_are_correct() {
        let def = humanoid_v0();
        let pairs = visible_parent_child_pairs(def);
        assert!(pairs.contains(&(UPPER_LEG_BACK, PELVIS)));
        assert!(pairs.contains(&(LOWER_LEG_BACK, UPPER_LEG_BACK)));
        assert!(pairs.contains(&(FOOT_BACK, LOWER_LEG_BACK)));
        assert!(!pairs.contains(&(UPPER_LEG_BACK, UPPER_LEG_FRONT)));
        assert!(!pairs.contains(&(LOWER_LEG_BACK, LOWER_LEG_FRONT)));
        assert!(!pairs.contains(&(FOOT_BACK, FOOT_FRONT)));
    }

    #[test]
    fn stage_d_back_arm_hangs_from_torso() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let torso = rig.world().get(TORSO).unwrap().translation;
        let upper = rig.world().get(UPPER_ARM_BACK).unwrap().translation;
        let forearm = rig.world().get(LOWER_ARM_BACK).unwrap().translation;
        let hand = rig.world().get(HAND_BACK).unwrap().translation;
        assert!(upper[0] > torso[0]);
        assert!(forearm[1] < upper[1]);
        assert!(hand[1] < forearm[1]);
        assert!((forearm[0] - upper[0]).abs() < EPS);
        assert!(hand[0] > forearm[0]);
    }

    #[test]
    fn stage_d_back_leg_hangs_from_pelvis() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let pelvis = rig.world().get(PELVIS).unwrap().translation;
        let upper = rig.world().get(UPPER_LEG_BACK).unwrap().translation;
        let shin = rig.world().get(LOWER_LEG_BACK).unwrap().translation;
        let foot = rig.world().get(FOOT_BACK).unwrap().translation;
        assert!(upper[1] < pelvis[1]);
        assert!(shin[1] < upper[1]);
        assert!(foot[1] < shin[1]);
        assert!(upper[0] > pelvis[0]);
        assert!((shin[0] - upper[0]).abs() < EPS);
        assert!((foot[0] - shin[0]).abs() < EPS);
    }

    #[test]
    fn front_proofs_do_not_move_back_limb_locals_or_world() {
        let mut bind = HumanoidDebug::new();
        let center = [0.0, 0.0];
        eval(&mut bind, center, FrontLegProof::Off, FrontArmProof::Off);
        let proofs = [
            (FrontLegProof::FootIndependent, FrontArmProof::Off),
            (FrontLegProof::ShinCarriesFoot, FrontArmProof::Off),
            (FrontLegProof::Off, FrontArmProof::HandIndependent),
            (FrontLegProof::Off, FrontArmProof::ForearmCarriesHand),
            (
                FrontLegProof::ShinCarriesFoot,
                FrontArmProof::ForearmCarriesHand,
            ),
        ];
        for (leg, arm) in proofs {
            let mut proof = HumanoidDebug::new();
            eval(&mut proof, center, leg, arm);
            for bone in BACK_LIMBS {
                let b_l = bind.local().get(bone).unwrap();
                let p_l = proof.local().get(bone).unwrap();
                assert!(tr_eq(b_l.translation, p_l.translation));
                assert!((b_l.rotation - p_l.rotation).abs() < EPS);
                let b_w = bind.world().get(bone).unwrap();
                let p_w = proof.world().get(bone).unwrap();
                assert!(tr_eq(b_w.translation, p_w.translation));
                assert!((b_w.rotation - p_w.rotation).abs() < EPS);
            }
        }
    }

    #[test]
    fn bone_connectors_are_not_filled_parent_child_aabbs() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        for scale in [
            CHARACTER_VISUAL_SCALE_1,
            CHARACTER_VISUAL_SCALE_115,
            CHARACTER_VISUAL_SCALE_2,
        ] {
            let quads = skeleton_debug_quads(humanoid_v0(), rig.world(), scale);
            let thick = BAR_THICK / scale;
            let connectors = &quads[..quads.len() - STAGE_D_JOINT_QUADS - PLACEHOLDERS];
            assert!(!connectors.is_empty());
            for q in connectors {
                let min_side = q.size[0].min(q.size[1]);
                assert!(
                    min_side <= thick * 1.25,
                    "connector min side {min_side} at scale {scale} should stay near thickness {thick}"
                );
            }
        }
    }

    #[test]
    fn preview_125_keeps_root_fixed_and_scales_offsets() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let pelvis = rig.world().get(PELVIS).unwrap().translation;
        let quads_1 = skeleton_debug_quads(humanoid_v0(), rig.world(), CHARACTER_VISUAL_SCALE_1);
        let quads_125 =
            skeleton_debug_quads(humanoid_v0(), rig.world(), CHARACTER_VISUAL_SCALE_115);
        let joints_1 = joint_quads(&quads_1);
        let joints_125 = joint_quads(&quads_125);
        let root_joint_1 = find_joint(joints_1, root).center;
        let root_joint_125 = find_joint(joints_125, root).center;
        assert!(tr_eq(root_joint_1, root));
        assert!(tr_eq(root_joint_125, root));
        let pelvis_joint_1 = find_joint(joints_1, pelvis).center;
        let expected_125 = scale_about_root(pelvis, root, CHARACTER_VISUAL_SCALE_115);
        let pelvis_joint_125 = find_joint(joints_125, expected_125).center;
        assert!(tr_eq(pelvis_joint_1, pelvis));
        assert!(tr_eq(pelvis_joint_125, expected_125));
        assert!(
            (pelvis_joint_125[1] - root[1] - CHARACTER_VISUAL_SCALE_115 * (pelvis[1] - root[1]))
                .abs()
                < EPS
        );
        let after = rig.world().get(PELVIS).unwrap().translation;
        assert!(tr_eq(after, pelvis));
        assert_eq!(find_joint(joints_1, root).size, [JOINT_SIZE, JOINT_SIZE]);
        assert_eq!(find_joint(joints_125, root).size, [JOINT_SIZE, JOINT_SIZE]);
        assert!((find_joint(joints_1, root).color[3] - JOINT_ALPHA).abs() < EPS);
        assert!((JOINT_ALPHA - 1.0).abs() < EPS);
    }

    #[test]
    fn preview_local_player_125_keeps_feet_at_root() {
        let center = [1.5, 2.5];
        let root = presented_root(center);
        let c1 = preview_local_player_center(center, CHARACTER_VISUAL_SCALE_1);
        assert!(tr_eq(c1, center));
        let c125 = preview_local_player_center(center, CHARACTER_VISUAL_SCALE_115);
        let size125 = preview_local_player_size(CHARACTER_VISUAL_SCALE_115);
        let feet_125 = [c125[0], c125[1] - size125[1] * 0.5];
        assert!(tr_eq(feet_125, root));
        assert!(
            (size125[0] - PLAYER_HALF_EXTENTS[0] * 2.0 * CHARACTER_VISUAL_SCALE_115).abs() < EPS
        );
        assert!(
            (size125[1] - PLAYER_HALF_EXTENTS[1] * 2.0 * CHARACTER_VISUAL_SCALE_115).abs() < EPS
        );
    }

    #[test]
    fn stage_a_spine_is_vertical_and_inside_player_aabb_height() {
        let mut rig = HumanoidDebug::new();
        let center = [2.0, 1.0];
        eval(&mut rig, center, FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let pelvis = rig.world().get(PELVIS).unwrap().translation;
        let torso = rig.world().get(TORSO).unwrap().translation;
        let head = rig.world().get(HEAD).unwrap().translation;
        let expect_root = presented_root(center);
        assert!(tr_eq(root, expect_root));
        for p in [pelvis, torso] {
            assert!((p[0] - root[0]).abs() < EPS);
        }
        assert!(head[0] > torso[0]);
        assert!(head[0] - torso[0] < 0.10);
        assert!((head[0] - torso[0] - BIND_HEAD.translation[0]).abs() < EPS);
        assert!(pelvis[1] > root[1]);
        assert!(torso[1] > pelvis[1]);
        assert!(head[1] > torso[1]);
        let aabb_top = center[1] + PLAYER_HALF_EXTENTS[1];
        assert!(head[1] < aabb_top);
        assert!(head[1] > center[1]);
    }

    #[test]
    fn stage_b_front_leg_hangs_from_pelvis() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let pelvis = rig.world().get(PELVIS).unwrap().translation;
        let upper = rig.world().get(UPPER_LEG_FRONT).unwrap().translation;
        let shin = rig.world().get(LOWER_LEG_FRONT).unwrap().translation;
        let foot = rig.world().get(FOOT_FRONT).unwrap().translation;
        assert!(upper[1] < pelvis[1]);
        assert!(shin[1] < upper[1]);
        assert!(foot[1] < shin[1]);
        assert!(upper[0] < pelvis[0]);
        assert!((shin[0] - upper[0]).abs() < EPS);
        assert!((foot[0] - shin[0]).abs() < EPS);
    }

    #[test]
    fn stage_c_front_arm_hangs_from_torso() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let torso = rig.world().get(TORSO).unwrap().translation;
        let upper = rig.world().get(UPPER_ARM_FRONT).unwrap().translation;
        let forearm = rig.world().get(LOWER_ARM_FRONT).unwrap().translation;
        let hand = rig.world().get(HAND_FRONT).unwrap().translation;
        assert!(upper[0] < torso[0]);
        assert!(forearm[1] < upper[1]);
        assert!(hand[1] < forearm[1]);
        assert!((forearm[0] - upper[0]).abs() < EPS);
        assert!((hand[0] - forearm[0]).abs() < EPS);
    }

    #[test]
    fn evaluate_still_writes_full_sixteen_bone_pose() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        assert_eq!(rig.world().bone_count(), 16);
        assert!(rig.world().get(HAND_FRONT).is_some());
        assert!(rig.world().get(HAND_BACK).is_some());
        assert!(rig.world().get(FOOT_FRONT).is_some());
        assert!(rig.world().get(FOOT_BACK).is_some());
        assert!(rig.local().get(HEAD).is_some());
    }

    #[test]
    fn proof_defaults_to_off() {
        assert_eq!(FrontLegProof::default(), FrontLegProof::Off);
        assert_eq!(FrontArmProof::default(), FrontArmProof::Off);
    }

    #[test]
    fn proof_foot_independent_does_not_move_shin_or_upper() {
        let mut bind = HumanoidDebug::new();
        let mut proof = HumanoidDebug::new();
        let center = [0.0, 0.0];
        eval(&mut bind, center, FrontLegProof::Off, FrontArmProof::Off);
        eval(
            &mut proof,
            center,
            FrontLegProof::FootIndependent,
            FrontArmProof::Off,
        );
        let shin_b = bind.world().get(LOWER_LEG_FRONT).unwrap();
        let shin_p = proof.world().get(LOWER_LEG_FRONT).unwrap();
        assert!(tr_eq(shin_b.translation, shin_p.translation));
        assert!((shin_b.rotation - shin_p.rotation).abs() < EPS);
        let upper_b = bind.world().get(UPPER_LEG_FRONT).unwrap();
        let upper_p = proof.world().get(UPPER_LEG_FRONT).unwrap();
        assert!(tr_eq(upper_b.translation, upper_p.translation));
        assert!((upper_b.rotation - upper_p.rotation).abs() < EPS);
        let foot_b = bind.world().get(FOOT_FRONT).unwrap();
        let foot_p = proof.world().get(FOOT_FRONT).unwrap();
        assert!(tr_eq(foot_b.translation, foot_p.translation));
        assert!((foot_p.rotation - foot_b.rotation - PROOF_FOOT_ROTATION).abs() < EPS);
        let foot_local = proof.local().get(FOOT_FRONT).unwrap();
        let bind_local = bind.local().get(FOOT_FRONT).unwrap();
        assert!(tr_eq(foot_local.translation, bind_local.translation));
        assert!((foot_local.rotation - bind_local.rotation - PROOF_FOOT_ROTATION).abs() < EPS);
    }

    #[test]
    fn proof_shin_rotation_carries_foot_not_upper() {
        let mut bind = HumanoidDebug::new();
        let mut proof = HumanoidDebug::new();
        let center = [0.0, 0.0];
        eval(&mut bind, center, FrontLegProof::Off, FrontArmProof::Off);
        eval(
            &mut proof,
            center,
            FrontLegProof::ShinCarriesFoot,
            FrontArmProof::Off,
        );
        let upper_b = bind.world().get(UPPER_LEG_FRONT).unwrap();
        let upper_p = proof.world().get(UPPER_LEG_FRONT).unwrap();
        assert!(tr_eq(upper_b.translation, upper_p.translation));
        assert!((upper_b.rotation - upper_p.rotation).abs() < EPS);
        let shin_b = bind.world().get(LOWER_LEG_FRONT).unwrap();
        let shin_p = proof.world().get(LOWER_LEG_FRONT).unwrap();
        assert!((shin_p.rotation - shin_b.rotation - PROOF_SHIN_ROTATION).abs() < EPS);
        let foot_local_b = bind.local().get(FOOT_FRONT).unwrap();
        let foot_local_p = proof.local().get(FOOT_FRONT).unwrap();
        assert!(tr_eq(foot_local_b.translation, foot_local_p.translation));
        assert!((foot_local_b.rotation - foot_local_p.rotation).abs() < EPS);
        let foot_b = bind.world().get(FOOT_FRONT).unwrap();
        let foot_p = proof.world().get(FOOT_FRONT).unwrap();
        assert!(!tr_eq(foot_b.translation, foot_p.translation));
    }

    #[test]
    fn proof_hand_independent_does_not_move_forearm_upper_or_torso() {
        let mut bind = HumanoidDebug::new();
        let mut proof = HumanoidDebug::new();
        let center = [0.0, 0.0];
        eval(&mut bind, center, FrontLegProof::Off, FrontArmProof::Off);
        eval(
            &mut proof,
            center,
            FrontLegProof::Off,
            FrontArmProof::HandIndependent,
        );
        for bone in [TORSO, UPPER_ARM_FRONT, LOWER_ARM_FRONT] {
            let b = bind.world().get(bone).unwrap();
            let p = proof.world().get(bone).unwrap();
            assert!(tr_eq(b.translation, p.translation));
            assert!((b.rotation - p.rotation).abs() < EPS);
        }
        let hand_b = bind.world().get(HAND_FRONT).unwrap();
        let hand_p = proof.world().get(HAND_FRONT).unwrap();
        assert!(tr_eq(hand_b.translation, hand_p.translation));
        assert!((hand_p.rotation - hand_b.rotation - PROOF_HAND_ROTATION).abs() < EPS);
        let hand_local = proof.local().get(HAND_FRONT).unwrap();
        let bind_local = bind.local().get(HAND_FRONT).unwrap();
        assert!(tr_eq(hand_local.translation, bind_local.translation));
        assert!((hand_local.rotation - bind_local.rotation - PROOF_HAND_ROTATION).abs() < EPS);
    }

    #[test]
    fn proof_forearm_rotation_carries_hand_not_upper_or_torso() {
        let mut bind = HumanoidDebug::new();
        let mut proof = HumanoidDebug::new();
        let center = [0.0, 0.0];
        eval(&mut bind, center, FrontLegProof::Off, FrontArmProof::Off);
        eval(
            &mut proof,
            center,
            FrontLegProof::Off,
            FrontArmProof::ForearmCarriesHand,
        );
        for bone in [TORSO, UPPER_ARM_FRONT] {
            let b = bind.world().get(bone).unwrap();
            let p = proof.world().get(bone).unwrap();
            assert!(tr_eq(b.translation, p.translation));
            assert!((b.rotation - p.rotation).abs() < EPS);
        }
        let arm_b = bind.world().get(LOWER_ARM_FRONT).unwrap();
        let arm_p = proof.world().get(LOWER_ARM_FRONT).unwrap();
        assert!((arm_p.rotation - arm_b.rotation - PROOF_FOREARM_ROTATION).abs() < EPS);
        let hand_local_b = bind.local().get(HAND_FRONT).unwrap();
        let hand_local_p = proof.local().get(HAND_FRONT).unwrap();
        assert!(tr_eq(hand_local_b.translation, hand_local_p.translation));
        assert!((hand_local_b.rotation - hand_local_p.rotation).abs() < EPS);
        let hand_b = bind.world().get(HAND_FRONT).unwrap();
        let hand_p = proof.world().get(HAND_FRONT).unwrap();
        assert!(!tr_eq(hand_b.translation, hand_p.translation));
    }

    fn limb_span_ends(panel: DrawQuad) -> ([f32; 2], [f32; 2]) {
        let c = panel.world_corners();
        let top = [(c[2][0] + c[3][0]) * 0.5, (c[2][1] + c[3][1]) * 0.5];
        let bot = [(c[0][0] + c[1][0]) * 0.5, (c[0][1] + c[1][1]) * 0.5];
        (top, bot)
    }

    fn foot_span_ends(panel: DrawQuad) -> ([f32; 2], [f32; 2]) {
        let c = panel.world_corners();
        let ankle = [(c[0][0] + c[3][0]) * 0.5, (c[0][1] + c[3][1]) * 0.5];
        let toe = [(c[1][0] + c[2][0]) * 0.5, (c[1][1] + c[2][1]) * 0.5];
        (ankle, toe)
    }

    fn corners_eq(a: [[f32; 2]; 4], b: [[f32; 2]; 4]) -> bool {
        a.iter().zip(b.iter()).all(|(p, q)| tr_eq(*p, *q))
    }

    #[test]
    fn p1_upper_arm_visual_uses_elbow_child_span() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let panel = p1_upper_arm_front_placeholder(rig.world(), root, 1.0).unwrap();
        let shoulder = rig.world().get(UPPER_ARM_FRONT).unwrap().translation;
        let elbow = rig.world().get(LOWER_ARM_FRONT).unwrap().translation;
        let c = panel.world_corners();
        let top = [(c[2][0] + c[3][0]) * 0.5, (c[2][1] + c[3][1]) * 0.5];
        let bot = [(c[0][0] + c[1][0]) * 0.5, (c[0][1] + c[1][1]) * 0.5];
        assert!(tr_eq(top, shoulder));
        assert!(tr_eq(bot, elbow));
        assert!((panel.size[1] - BIND_LOWER_ARM_FRONT.translation[1].abs()).abs() < EPS);
        assert!(
            (BIND_HAND_FRONT.translation[1].abs() - panel.size[1]).abs() > 0.05,
            "upper-arm length must not use hand_front rest"
        );
    }

    #[test]
    fn p1_forearm_visual_uses_wrist_child_span() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let panel = p1_forearm_front_placeholder(rig.world(), root, 1.0).unwrap();
        let elbow = rig.world().get(LOWER_ARM_FRONT).unwrap().translation;
        let hand = rig.world().get(HAND_FRONT).unwrap().translation;
        let c = panel.world_corners();
        let top = [(c[2][0] + c[3][0]) * 0.5, (c[2][1] + c[3][1]) * 0.5];
        let bot = [(c[0][0] + c[1][0]) * 0.5, (c[0][1] + c[1][1]) * 0.5];
        assert!(tr_eq(top, elbow));
        assert!(tr_eq(bot, hand));
        assert!((panel.size[1] - BIND_HAND_FRONT.translation[1].abs()).abs() < EPS);
        assert!(
            (BIND_LOWER_ARM_FRONT.translation[1].abs() - panel.size[1]).abs() > 0.05,
            "forearm length must not use lower_arm_front parent-relative span"
        );
    }

    #[test]
    fn p1_hand_independent_changes_only_hand_visual() {
        let mut bind = HumanoidDebug::new();
        let mut proof = HumanoidDebug::new();
        let center = [0.0, 0.0];
        eval(&mut bind, center, FrontLegProof::Off, FrontArmProof::Off);
        eval(
            &mut proof,
            center,
            FrontLegProof::Off,
            FrontArmProof::HandIndependent,
        );
        let root = bind.world().get(ROOT).unwrap().translation;
        let upper_b = p1_upper_arm_front_placeholder(bind.world(), root, 1.0).unwrap();
        let upper_p = p1_upper_arm_front_placeholder(proof.world(), root, 1.0).unwrap();
        let arm_b = p1_forearm_front_placeholder(bind.world(), root, 1.0).unwrap();
        let arm_p = p1_forearm_front_placeholder(proof.world(), root, 1.0).unwrap();
        let hand_b = p1_hand_front_placeholder(bind.world(), root, 1.0).unwrap();
        let hand_p = p1_hand_front_placeholder(proof.world(), root, 1.0).unwrap();
        assert!(corners_eq(upper_b.world_corners(), upper_p.world_corners()));
        assert!(corners_eq(arm_b.world_corners(), arm_p.world_corners()));
        assert!(!corners_eq(hand_b.world_corners(), hand_p.world_corners()));
        let wrist_b = bind.world().get(HAND_FRONT).unwrap().translation;
        let wrist_p = proof.world().get(HAND_FRONT).unwrap().translation;
        assert!(tr_eq(wrist_b, wrist_p));
        let (_, forearm_end_b) = limb_span_ends(arm_b);
        let (_, forearm_end_p) = limb_span_ends(arm_p);
        assert!(tr_eq(forearm_end_b, wrist_b));
        assert!(tr_eq(forearm_end_p, wrist_p));
        let (hand_attach_b, hand_far_b) = limb_span_ends(hand_b);
        let (hand_attach_p, hand_far_p) = limb_span_ends(hand_p);
        assert!(tr_eq(hand_attach_b, wrist_b));
        assert!(tr_eq(hand_attach_p, wrist_p));
        assert!(!tr_eq(hand_far_b, hand_far_p));
        let hand_span = (hand_far_b[0] - hand_attach_b[0]).hypot(hand_far_b[1] - hand_attach_b[1]);
        assert!((hand_span - HAND_PLACEHOLDER_SIZE[1]).abs() < EPS);
    }

    #[test]
    fn p1_forearm_carries_hand_moves_forearm_and_hand_not_upper() {
        let mut bind = HumanoidDebug::new();
        let mut proof = HumanoidDebug::new();
        let center = [0.0, 0.0];
        eval(&mut bind, center, FrontLegProof::Off, FrontArmProof::Off);
        eval(
            &mut proof,
            center,
            FrontLegProof::Off,
            FrontArmProof::ForearmCarriesHand,
        );
        let root = bind.world().get(ROOT).unwrap().translation;
        let elbow_b = bind.world().get(LOWER_ARM_FRONT).unwrap().translation;
        let elbow_p = proof.world().get(LOWER_ARM_FRONT).unwrap().translation;
        assert!(tr_eq(elbow_b, elbow_p));
        let upper_b = p1_upper_arm_front_placeholder(bind.world(), root, 1.0).unwrap();
        let upper_p = p1_upper_arm_front_placeholder(proof.world(), root, 1.0).unwrap();
        let arm_b = p1_forearm_front_placeholder(bind.world(), root, 1.0).unwrap();
        let arm_p = p1_forearm_front_placeholder(proof.world(), root, 1.0).unwrap();
        let hand_b = p1_hand_front_placeholder(bind.world(), root, 1.0).unwrap();
        let hand_p = p1_hand_front_placeholder(proof.world(), root, 1.0).unwrap();
        assert!(corners_eq(upper_b.world_corners(), upper_p.world_corners()));
        assert!(!corners_eq(arm_b.world_corners(), arm_p.world_corners()));
        assert!(!corners_eq(hand_b.world_corners(), hand_p.world_corners()));
        let cp = arm_p.world_corners();
        let top_p = [(cp[2][0] + cp[3][0]) * 0.5, (cp[2][1] + cp[3][1]) * 0.5];
        assert!(tr_eq(top_p, elbow_p));
    }

    #[test]
    fn p1_front_arm_chain_stays_inside_renderer_batch() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        for scale in [
            CHARACTER_VISUAL_SCALE_1,
            CHARACTER_VISUAL_SCALE_115,
            CHARACTER_VISUAL_SCALE_2,
        ] {
            let quads = skeleton_debug_quads(humanoid_v0(), rig.world(), scale);
            assert_eq!(joint_quads(&quads).len(), STAGE_D_JOINT_QUADS);
            assert_eq!(quads[quads.len() - PLACEHOLDERS..].len(), PLACEHOLDERS);
            assert_eq!(
                quads[quads.len() - P1_PLACEHOLDERS..].len(),
                P1_PLACEHOLDERS
            );
            assert!(quads.len() < MAX_QUADS);
            assert!(quads.len() <= MAX_QUADS * 3 / 4);
        }
    }

    #[test]
    fn r1_forearm_panel_spans_elbow_to_hand_in_bind() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let panel = p1_forearm_front_placeholder(rig.world(), root, 1.0).unwrap();
        let elbow = rig.world().get(LOWER_ARM_FRONT).unwrap().translation;
        let hand = rig.world().get(HAND_FRONT).unwrap().translation;
        let c = panel.world_corners();
        let top = [(c[2][0] + c[3][0]) * 0.5, (c[2][1] + c[3][1]) * 0.5];
        let bot = [(c[0][0] + c[1][0]) * 0.5, (c[0][1] + c[1][1]) * 0.5];
        assert!(tr_eq(top, elbow));
        assert!(tr_eq(bot, hand));
        assert!((panel.size[1] - BIND_HAND_FRONT.translation[1].abs()).abs() < EPS);
        assert!(
            (purgatory_skeleton::BIND_LOWER_ARM_FRONT.translation[1].abs() - panel.size[1]).abs()
                > 0.05,
            "panel length must not use lower_arm_front parent-relative span"
        );
    }

    #[test]
    fn r1_forearm_carries_hand_rotates_panel_around_planted_elbow() {
        let mut bind = HumanoidDebug::new();
        let mut proof = HumanoidDebug::new();
        let center = [0.0, 0.0];
        eval(&mut bind, center, FrontLegProof::Off, FrontArmProof::Off);
        eval(
            &mut proof,
            center,
            FrontLegProof::Off,
            FrontArmProof::ForearmCarriesHand,
        );
        let root = bind.world().get(ROOT).unwrap().translation;
        let elbow_b = bind.world().get(LOWER_ARM_FRONT).unwrap().translation;
        let elbow_p = proof.world().get(LOWER_ARM_FRONT).unwrap().translation;
        assert!(tr_eq(elbow_b, elbow_p));
        let panel_b = p1_forearm_front_placeholder(bind.world(), root, 1.0).unwrap();
        let panel_p = p1_forearm_front_placeholder(proof.world(), root, 1.0).unwrap();
        let cb = panel_b.world_corners();
        let cp = panel_p.world_corners();
        assert!(!corners_eq(cb, cp));
        let top_p = [(cp[2][0] + cp[3][0]) * 0.5, (cp[2][1] + cp[3][1]) * 0.5];
        assert!(tr_eq(top_p, elbow_p));
        let hand_b = bind.world().get(HAND_FRONT).unwrap().translation;
        let hand_p = proof.world().get(HAND_FRONT).unwrap().translation;
        assert!(!tr_eq(hand_b, hand_p));
    }

    #[test]
    fn r1_hand_independent_does_not_move_forearm_panel() {
        let mut bind = HumanoidDebug::new();
        let mut proof = HumanoidDebug::new();
        let center = [0.0, 0.0];
        eval(&mut bind, center, FrontLegProof::Off, FrontArmProof::Off);
        eval(
            &mut proof,
            center,
            FrontLegProof::Off,
            FrontArmProof::HandIndependent,
        );
        let root = bind.world().get(ROOT).unwrap().translation;
        let panel_b = p1_forearm_front_placeholder(bind.world(), root, 1.0).unwrap();
        let panel_p = p1_forearm_front_placeholder(proof.world(), root, 1.0).unwrap();
        assert!(corners_eq(panel_b.world_corners(), panel_p.world_corners()));
        let hand_b = bind.world().get(HAND_FRONT).unwrap().translation;
        let hand_p = proof.world().get(HAND_FRONT).unwrap().translation;
        assert!(tr_eq(hand_b, hand_p));
        let (_, forearm_end) = limb_span_ends(panel_p);
        assert!(tr_eq(forearm_end, hand_p));
        let vis_b = p1_hand_front_placeholder(bind.world(), root, 1.0).unwrap();
        let vis_p = p1_hand_front_placeholder(proof.world(), root, 1.0).unwrap();
        assert!(!corners_eq(vis_b.world_corners(), vis_p.world_corners()));
        let (attach_p, _) = limb_span_ends(vis_p);
        assert!(tr_eq(attach_p, hand_p));
    }

    #[test]
    fn r1_preview_125_scales_pivot_and_size_once() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let p1 = p1_forearm_front_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_1).unwrap();
        let p125 =
            p1_forearm_front_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap();
        let elbow = rig.world().get(LOWER_ARM_FRONT).unwrap().translation;
        let c1 = p1.world_corners();
        let c125 = p125.world_corners();
        let top1 = [(c1[2][0] + c1[3][0]) * 0.5, (c1[2][1] + c1[3][1]) * 0.5];
        let top125 = [
            (c125[2][0] + c125[3][0]) * 0.5,
            (c125[2][1] + c125[3][1]) * 0.5,
        ];
        assert!(tr_eq(top1, elbow));
        assert!(tr_eq(
            top125,
            scale_about_root(elbow, root, CHARACTER_VISUAL_SCALE_115)
        ));
        assert!((p125.size[0] - p1.size[0] * CHARACTER_VISUAL_SCALE_115).abs() < EPS);
        assert!((p125.size[1] - p1.size[1] * CHARACTER_VISUAL_SCALE_115).abs() < EPS);
        let vis1 = [
            (c1[0][0] + c1[1][0] + c1[2][0] + c1[3][0]) * 0.25,
            (c1[0][1] + c1[1][1] + c1[2][1] + c1[3][1]) * 0.25,
        ];
        let vis125 = [
            (c125[0][0] + c125[1][0] + c125[2][0] + c125[3][0]) * 0.25,
            (c125[0][1] + c125[1][1] + c125[2][1] + c125[3][1]) * 0.25,
        ];
        let d1 = (vis1[0] - top1[0]).hypot(vis1[1] - top1[1]);
        let d125 = (vis125[0] - top125[0]).hypot(vis125[1] - top125[1]);
        assert!((d1 - p1.size[1] * 0.5).abs() < EPS);
        assert!((d125 - p125.size[1] * 0.5).abs() < EPS);
    }

    #[test]
    fn p2_upper_leg_visual_uses_knee_child_span() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let panel = p2_upper_leg_front_placeholder(rig.world(), root, 1.0).unwrap();
        let hip = rig.world().get(UPPER_LEG_FRONT).unwrap().translation;
        let knee = rig.world().get(LOWER_LEG_FRONT).unwrap().translation;
        let (top, bot) = limb_span_ends(panel);
        assert!(tr_eq(top, hip));
        assert!(tr_eq(bot, knee));
        assert!((knee[0] - hip[0]).abs() < EPS);
        let expect = BIND_LOWER_LEG_FRONT.translation[0].hypot(BIND_LOWER_LEG_FRONT.translation[1]);
        assert!((panel.size[1] - expect).abs() < EPS);
        assert!(
            (purgatory_skeleton::BIND_UPPER_LEG_FRONT.translation[1].abs() - panel.size[1]).abs()
                > 0.05,
            "upper-leg visual must use hip→knee child span, not hip placement offset"
        );
    }

    #[test]
    fn p2_shin_visual_uses_ankle_child_span() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let panel = p2_shin_front_placeholder(rig.world(), root, 1.0).unwrap();
        let knee = rig.world().get(LOWER_LEG_FRONT).unwrap().translation;
        let ankle = rig.world().get(FOOT_FRONT).unwrap().translation;
        let (top, bot) = limb_span_ends(panel);
        assert!(tr_eq(top, knee));
        assert!(tr_eq(bot, ankle));
        assert!((ankle[0] - knee[0]).abs() < EPS);
        assert!(ankle[1] < knee[1]);
        let expect = BIND_FOOT_FRONT.translation[0].hypot(BIND_FOOT_FRONT.translation[1]);
        assert!((panel.size[1] - expect).abs() < EPS);
        assert!(
            (BIND_LOWER_LEG_FRONT.translation[1].abs() - panel.size[1]).abs() > 0.01,
            "shin length must not use lower_leg_front parent-relative thigh span"
        );
    }

    #[test]
    fn p2_foot_placeholder_pivots_at_ankle_and_extends_away() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let panel = p2_foot_front_placeholder(rig.world(), root, 1.0).unwrap();
        let ankle = rig.world().get(FOOT_FRONT).unwrap().translation;
        let (attach, toe) = foot_span_ends(panel);
        assert!(tr_eq(attach, ankle));
        assert!(!tr_eq(toe, ankle));
        assert!((toe[0] - attach[0] - FOOT_PLACEHOLDER_SIZE[0]).abs() < EPS);
        assert!((toe[1] - attach[1]).abs() < EPS);
    }

    #[test]
    fn p2_foot_independent_changes_only_foot_visual() {
        let mut bind = HumanoidDebug::new();
        let mut proof = HumanoidDebug::new();
        let center = [0.0, 0.0];
        eval(&mut bind, center, FrontLegProof::Off, FrontArmProof::Off);
        eval(
            &mut proof,
            center,
            FrontLegProof::FootIndependent,
            FrontArmProof::Off,
        );
        let root = bind.world().get(ROOT).unwrap().translation;
        let ankle_b = bind.world().get(FOOT_FRONT).unwrap().translation;
        let ankle_p = proof.world().get(FOOT_FRONT).unwrap().translation;
        assert!(tr_eq(ankle_b, ankle_p));
        let upper_b = p2_upper_leg_front_placeholder(bind.world(), root, 1.0).unwrap();
        let upper_p = p2_upper_leg_front_placeholder(proof.world(), root, 1.0).unwrap();
        let shin_b = p2_shin_front_placeholder(bind.world(), root, 1.0).unwrap();
        let shin_p = p2_shin_front_placeholder(proof.world(), root, 1.0).unwrap();
        let foot_b = p2_foot_front_placeholder(bind.world(), root, 1.0).unwrap();
        let foot_p = p2_foot_front_placeholder(proof.world(), root, 1.0).unwrap();
        assert!(corners_eq(upper_b.world_corners(), upper_p.world_corners()));
        assert!(corners_eq(shin_b.world_corners(), shin_p.world_corners()));
        assert!(!corners_eq(foot_b.world_corners(), foot_p.world_corners()));
        let (_, shin_end_b) = limb_span_ends(shin_b);
        let (_, shin_end_p) = limb_span_ends(shin_p);
        assert!(tr_eq(shin_end_b, ankle_b));
        assert!(tr_eq(shin_end_p, ankle_p));
        let (attach_p, toe_p) = foot_span_ends(foot_p);
        let (_, toe_b) = foot_span_ends(foot_b);
        assert!(tr_eq(attach_p, ankle_p));
        assert!(!tr_eq(toe_b, toe_p));
    }

    #[test]
    fn p2_shin_carries_foot_moves_shin_and_foot_not_upper() {
        let mut bind = HumanoidDebug::new();
        let mut proof = HumanoidDebug::new();
        let center = [0.0, 0.0];
        eval(&mut bind, center, FrontLegProof::Off, FrontArmProof::Off);
        eval(
            &mut proof,
            center,
            FrontLegProof::ShinCarriesFoot,
            FrontArmProof::Off,
        );
        let root = bind.world().get(ROOT).unwrap().translation;
        let knee_b = bind.world().get(LOWER_LEG_FRONT).unwrap().translation;
        let knee_p = proof.world().get(LOWER_LEG_FRONT).unwrap().translation;
        assert!(tr_eq(knee_b, knee_p));
        let upper_b = p2_upper_leg_front_placeholder(bind.world(), root, 1.0).unwrap();
        let upper_p = p2_upper_leg_front_placeholder(proof.world(), root, 1.0).unwrap();
        let shin_b = p2_shin_front_placeholder(bind.world(), root, 1.0).unwrap();
        let shin_p = p2_shin_front_placeholder(proof.world(), root, 1.0).unwrap();
        let foot_b = p2_foot_front_placeholder(bind.world(), root, 1.0).unwrap();
        let foot_p = p2_foot_front_placeholder(proof.world(), root, 1.0).unwrap();
        assert!(corners_eq(upper_b.world_corners(), upper_p.world_corners()));
        assert!(!corners_eq(shin_b.world_corners(), shin_p.world_corners()));
        assert!(!corners_eq(foot_b.world_corners(), foot_p.world_corners()));
        let (shin_top_p, shin_end_p) = limb_span_ends(shin_p);
        assert!(tr_eq(shin_top_p, knee_p));
        let ankle_p = proof.world().get(FOOT_FRONT).unwrap().translation;
        assert!(tr_eq(shin_end_p, ankle_p));
        let (foot_attach_p, _) = foot_span_ends(foot_p);
        assert!(tr_eq(foot_attach_p, ankle_p));
        let ankle_b = bind.world().get(FOOT_FRONT).unwrap().translation;
        assert!(!tr_eq(ankle_b, ankle_p));
    }

    #[test]
    fn p2_front_leg_chain_stays_inside_renderer_batch() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        for scale in [
            CHARACTER_VISUAL_SCALE_1,
            CHARACTER_VISUAL_SCALE_115,
            CHARACTER_VISUAL_SCALE_2,
        ] {
            let quads = skeleton_debug_quads(humanoid_v0(), rig.world(), scale);
            assert_eq!(joint_quads(&quads).len(), STAGE_D_JOINT_QUADS);
            let placeholders = &quads[quads.len() - PLACEHOLDERS..];
            assert_eq!(placeholders.len(), PLACEHOLDERS);
            // back arm, back leg, torso+head, front leg, front arm
            assert_eq!(
                placeholders.len(),
                P1_BACK_PLACEHOLDERS
                    + P2_BACK_PLACEHOLDERS
                    + P3_PLACEHOLDERS
                    + P2_PLACEHOLDERS
                    + P1_PLACEHOLDERS
            );
            assert!(quads.len() < MAX_QUADS);
            assert!(quads.len() <= MAX_QUADS * 3 / 4);
        }
    }

    #[test]
    fn p2_back_upper_leg_visual_uses_knee_child_span() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let panel = p2_upper_leg_back_placeholder(rig.world(), root, 1.0).unwrap();
        let hip = rig.world().get(UPPER_LEG_BACK).unwrap().translation;
        let knee = rig.world().get(LOWER_LEG_BACK).unwrap().translation;
        let (top, bot) = limb_span_ends(panel);
        assert!(tr_eq(top, hip));
        assert!(tr_eq(bot, knee));
        assert!((knee[0] - hip[0]).abs() < EPS);
        let expect = BIND_LOWER_LEG_BACK.translation[0].hypot(BIND_LOWER_LEG_BACK.translation[1]);
        assert!((panel.size[1] - expect).abs() < EPS);
    }

    #[test]
    fn p2_back_shin_visual_uses_ankle_child_span() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let panel = p2_shin_back_placeholder(rig.world(), root, 1.0).unwrap();
        let knee = rig.world().get(LOWER_LEG_BACK).unwrap().translation;
        let ankle = rig.world().get(FOOT_BACK).unwrap().translation;
        let (top, bot) = limb_span_ends(panel);
        assert!(tr_eq(top, knee));
        assert!(tr_eq(bot, ankle));
        assert!((ankle[0] - knee[0]).abs() < EPS);
        assert!(ankle[1] < knee[1]);
    }

    #[test]
    fn p2_back_foot_placeholder_pivots_at_ankle_and_extends_away() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let panel = p2_foot_back_placeholder(rig.world(), root, 1.0).unwrap();
        let ankle = rig.world().get(FOOT_BACK).unwrap().translation;
        let (attach, toe) = foot_span_ends(panel);
        assert!(tr_eq(attach, ankle));
        assert!((toe[0] - attach[0] - FOOT_PLACEHOLDER_SIZE[0]).abs() < EPS);
        assert!((toe[1] - attach[1]).abs() < EPS);
    }

    #[test]
    fn p2_front_proofs_do_not_modify_back_leg_visuals() {
        let mut bind = HumanoidDebug::new();
        eval(
            &mut bind,
            [0.0, 0.0],
            FrontLegProof::Off,
            FrontArmProof::Off,
        );
        let root = bind.world().get(ROOT).unwrap().translation;
        let upper_b = p2_upper_leg_back_placeholder(bind.world(), root, 1.0).unwrap();
        let shin_b = p2_shin_back_placeholder(bind.world(), root, 1.0).unwrap();
        let foot_b = p2_foot_back_placeholder(bind.world(), root, 1.0).unwrap();
        for (leg, arm) in [
            (FrontLegProof::FootIndependent, FrontArmProof::Off),
            (FrontLegProof::ShinCarriesFoot, FrontArmProof::Off),
            (FrontLegProof::Off, FrontArmProof::HandIndependent),
            (FrontLegProof::Off, FrontArmProof::ForearmCarriesHand),
        ] {
            let mut proof = HumanoidDebug::new();
            eval(&mut proof, [0.0, 0.0], leg, arm);
            let upper_p = p2_upper_leg_back_placeholder(proof.world(), root, 1.0).unwrap();
            let shin_p = p2_shin_back_placeholder(proof.world(), root, 1.0).unwrap();
            let foot_p = p2_foot_back_placeholder(proof.world(), root, 1.0).unwrap();
            assert!(corners_eq(upper_b.world_corners(), upper_p.world_corners()));
            assert!(corners_eq(shin_b.world_corners(), shin_p.world_corners()));
            assert!(corners_eq(foot_b.world_corners(), foot_p.world_corners()));
        }
    }

    fn find_placeholder_index(quads: &[DrawQuad], panel: DrawQuad) -> usize {
        quads
            .iter()
            .position(|q| corners_eq(q.world_corners(), panel.world_corners()))
            .expect("placeholder missing from debug emission")
    }

    #[test]
    fn p3_torso_and_head_use_evaluated_bone_transforms() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let torso_xf = rig.world().get(TORSO).unwrap();
        let head_xf = rig.world().get(HEAD).unwrap();
        let torso = p3_torso_placeholder(rig.world(), root, 1.0).unwrap();
        let head = p3_head_placeholder(rig.world(), root, 1.0).unwrap();
        let layout = torso_placeholder_layout();
        let top_width = (TORSO_LOCAL_TR_X - TORSO_LOCAL_TL_X).abs();
        assert!((torso.size[0] - top_width).abs() < EPS);
        assert!((torso.size[1] - layout.height).abs() < EPS);
        assert!((torso.size[1] - BIND_TORSO.translation[1].abs()).abs() > 0.1);
        assert!((torso.size[0] - PLAYER_HALF_EXTENTS[0] * 2.0).abs() > 0.02);
        let tc = torso.world_corners();
        let torso_mid = [
            (tc[0][0] + tc[1][0] + tc[2][0] + tc[3][0]) * 0.25,
            (tc[0][1] + tc[1][1] + tc[2][1] + tc[3][1]) * 0.25,
        ];
        assert!(tr_eq(
            torso_mid,
            [
                torso_xf.translation[0] + layout.local_center[0],
                torso_xf.translation[1] + layout.local_center[1],
            ]
        ));
        let (head_attach, head_top) = {
            let c = head.world_corners();
            let attach = [(c[0][0] + c[1][0]) * 0.5, (c[0][1] + c[1][1]) * 0.5];
            let top = [(c[2][0] + c[3][0]) * 0.5, (c[2][1] + c[3][1]) * 0.5];
            (attach, top)
        };
        assert!(tr_eq(head_attach, head_xf.translation));
        assert!(head_top[1] > head_xf.translation[1]);
        assert!((head_top[1] - head_xf.translation[1] - HEAD_PLACEHOLDER_SIZE[1]).abs() < EPS);
    }

    #[test]
    fn p3_torso_is_emitted_behind_front_arm() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let quads = skeleton_debug_quads(humanoid_v0(), rig.world(), 1.0);
        let torso = p3_torso_placeholder(rig.world(), root, 1.0).unwrap();
        let upper = p1_upper_arm_front_placeholder(rig.world(), root, 1.0).unwrap();
        let forearm = p1_forearm_front_placeholder(rig.world(), root, 1.0).unwrap();
        let hand = p1_hand_front_placeholder(rig.world(), root, 1.0).unwrap();
        let torso_i = find_placeholder_index(&quads, torso);
        let upper_i = find_placeholder_index(&quads, upper);
        let forearm_i = find_placeholder_index(&quads, forearm);
        let hand_i = find_placeholder_index(&quads, hand);
        assert!(torso_i < upper_i);
        assert!(torso_i < forearm_i);
        assert!(torso_i < hand_i);
        let head = p3_head_placeholder(rig.world(), root, 1.0).unwrap();
        let thigh = p2_upper_leg_front_placeholder(rig.world(), root, 1.0).unwrap();
        let head_i = find_placeholder_index(&quads, head);
        let thigh_i = find_placeholder_index(&quads, thigh);
        assert!(torso_i < thigh_i);
        assert!(thigh_i < head_i);
        assert!(head_i < upper_i);
        let back_thigh = p2_upper_leg_back_placeholder(rig.world(), root, 1.0).unwrap();
        let back_i = find_placeholder_index(&quads, back_thigh);
        assert!(back_i < torso_i);
        assert!(back_i < thigh_i);
        let back_upper = p1_upper_arm_back_placeholder(rig.world(), root, 1.0).unwrap();
        let back_arm_i = find_placeholder_index(&quads, back_upper);
        assert!(back_arm_i < back_i);
        assert!(back_arm_i < torso_i);
        let far = p3_torso_far_placeholder(rig.world(), root, 1.0).unwrap();
        let far_i = find_placeholder_index(&quads, far);
        assert!(back_i < far_i);
        assert!(far_i < torso_i);
    }

    #[test]
    fn p3_limb_proofs_do_not_modify_torso_or_head() {
        let mut bind = HumanoidDebug::new();
        eval(
            &mut bind,
            [0.0, 0.0],
            FrontLegProof::Off,
            FrontArmProof::Off,
        );
        let root = bind.world().get(ROOT).unwrap().translation;
        let torso_b = p3_torso_placeholder(bind.world(), root, 1.0).unwrap();
        let head_b = p3_head_placeholder(bind.world(), root, 1.0).unwrap();
        let proofs = [
            (FrontLegProof::Off, FrontArmProof::HandIndependent),
            (FrontLegProof::Off, FrontArmProof::ForearmCarriesHand),
            (FrontLegProof::FootIndependent, FrontArmProof::Off),
            (FrontLegProof::ShinCarriesFoot, FrontArmProof::Off),
        ];
        for (leg, arm) in proofs {
            let mut proof = HumanoidDebug::new();
            eval(&mut proof, [0.0, 0.0], leg, arm);
            let torso_p = p3_torso_placeholder(proof.world(), root, 1.0).unwrap();
            let head_p = p3_head_placeholder(proof.world(), root, 1.0).unwrap();
            assert!(
                corners_eq(torso_b.world_corners(), torso_p.world_corners()),
                "torso moved under {leg:?} {arm:?}"
            );
            assert!(
                corners_eq(head_b.world_corners(), head_p.world_corners()),
                "head moved under {leg:?} {arm:?}"
            );
        }
    }

    #[test]
    fn p3_placeholders_stay_inside_renderer_batch() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        for scale in [
            CHARACTER_VISUAL_SCALE_1,
            CHARACTER_VISUAL_SCALE_115,
            CHARACTER_VISUAL_SCALE_2,
        ] {
            let quads = skeleton_debug_quads(humanoid_v0(), rig.world(), scale);
            assert_eq!(joint_quads(&quads).len(), STAGE_D_JOINT_QUADS);
            assert_eq!(quads[quads.len() - PLACEHOLDERS..].len(), PLACEHOLDERS);
            assert_eq!(P3_PLACEHOLDERS, 3);
            assert!(quads.len() < MAX_QUADS);
            assert!(quads.len() <= MAX_QUADS * 3 / 4);
        }
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn p31_head_visual_is_twice_base_without_moving_head_joint() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let head_xf = rig.world().get(HEAD).unwrap();
        let head = p3_head_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_1).unwrap();
        assert!((HEAD_PLACEHOLDER_VISUAL_SCALE - 2.0).abs() < EPS);
        assert!(
            (HEAD_PLACEHOLDER_SIZE[0]
                - HEAD_PLACEHOLDER_BASE[0] * 2.0 * HEAD_PLACEHOLDER_WIDTH_SCALE)
                .abs()
                < EPS
        );
        assert!((HEAD_PLACEHOLDER_SIZE[1] - HEAD_PLACEHOLDER_BASE[1] * 2.0).abs() < EPS);
        assert!((HEAD_PLACEHOLDER_WIDTH_SCALE - 1.10).abs() < EPS);
        assert!((head.size[1] - HEAD_PLACEHOLDER_SIZE[1]).abs() < EPS);
        let c = head.world_corners();
        let attach = [(c[0][0] + c[1][0]) * 0.5, (c[0][1] + c[1][1]) * 0.5];
        assert!(tr_eq(attach, head_xf.translation));
        assert!(c[2][0] - head_xf.translation[0] > head_xf.translation[0] - c[3][0]);
        assert!((BIND_HEAD.translation[0] - 0.07).abs() < EPS);
        assert!((BIND_HEAD.translation[1] - 0.24).abs() < EPS);
        assert!(head_xf.translation[0] > rig.world().get(TORSO).unwrap().translation[0]);
    }

    #[test]
    fn proportion_calibration_thickness_keeps_child_span() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let arm = p1_upper_arm_front_placeholder(rig.world(), root, 1.0).unwrap();
        let thigh = p2_upper_leg_front_placeholder(rig.world(), root, 1.0).unwrap();
        let torso = p3_torso_placeholder(rig.world(), root, 1.0).unwrap();
        let arm_len =
            BIND_LOWER_ARM_FRONT.translation[0].hypot(BIND_LOWER_ARM_FRONT.translation[1]);
        let thigh_len =
            BIND_LOWER_LEG_FRONT.translation[0].hypot(BIND_LOWER_LEG_FRONT.translation[1]);
        assert!((arm.size[0] - ARM_PLACEHOLDER_WIDTH).abs() < EPS);
        assert!((arm.size[1] - arm_len).abs() < EPS);
        assert!((thigh.size[0] - LEG_PLACEHOLDER_WIDTH).abs() < EPS);
        assert!((thigh.size[1] - thigh_len).abs() < EPS);
        let top_width = (TORSO_LOCAL_TR_X - TORSO_LOCAL_TL_X).abs();
        let bottom_width = (TORSO_LOCAL_BR_X - TORSO_LOCAL_BL_X).abs();
        assert!((torso.size[0] - top_width).abs() < EPS);
        assert!(top_width > bottom_width);
        let trap_ratio = top_width / bottom_width;
        assert!(
            trap_ratio > 1.25 && trap_ratio < 1.45,
            "inverted trapezoid ratio {trap_ratio} should stay restrained"
        );
        let corners = torso_local_corners(1.0);
        assert!(corners[2][0] > corners[3][0].abs());
        let layout = torso_placeholder_layout();
        assert!(layout.local_center[0].abs() < EPS);
        assert!((ARM_PLACEHOLDER_WIDTH - 0.08 * 1.18).abs() < EPS);
        assert!((LEG_PLACEHOLDER_WIDTH - 0.08 * 1.22).abs() < EPS);
    }

    #[test]
    fn p31_character_visual_scale_does_not_write_bind_or_local() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let locals_before: Vec<_> = (0u8..16)
            .map(|i| {
                let xf = rig.local().get(BoneIndex::from_u8(i)).unwrap();
                (xf.translation, xf.rotation)
            })
            .collect();
        let worlds_before: Vec<_> = (0u8..16)
            .map(|i| {
                let xf = rig.world().get(BoneIndex::from_u8(i)).unwrap();
                (xf.translation, xf.rotation)
            })
            .collect();
        let _ = skeleton_debug_quads(humanoid_v0(), rig.world(), CHARACTER_VISUAL_SCALE_115);
        let _ = skeleton_debug_quads(humanoid_v0(), rig.world(), CHARACTER_VISUAL_SCALE_2);
        for i in 0u8..16 {
            let local = rig.local().get(BoneIndex::from_u8(i)).unwrap();
            let world = rig.world().get(BoneIndex::from_u8(i)).unwrap();
            assert!(tr_eq(local.translation, locals_before[i as usize].0));
            assert!((local.rotation - locals_before[i as usize].1).abs() < EPS);
            assert!(tr_eq(world.translation, worlds_before[i as usize].0));
            assert!((world.rotation - worlds_before[i as usize].1).abs() < EPS);
        }
        assert!((CHARACTER_VISUAL_SCALE_115 - 1.15).abs() < EPS);
        assert!((character_visual_scale(false) - CHARACTER_VISUAL_SCALE_115).abs() < EPS);
        assert!((character_visual_scale(true) - CHARACTER_VISUAL_SCALE_2).abs() < EPS);
        assert!(
            (sanitize_preview_scale(CHARACTER_VISUAL_SCALE_2) - CHARACTER_VISUAL_SCALE_2).abs()
                < EPS
        );
        assert!(
            (sanitize_preview_scale(CHARACTER_VISUAL_SCALE_115) - CHARACTER_VISUAL_SCALE_115).abs()
                < EPS
        );
    }

    #[test]
    fn p3_torso_is_inverted_trapezoid_around_torso_pivot() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let torso_xf = rig.world().get(TORSO).unwrap();
        let torso = p3_torso_placeholder(rig.world(), root, 1.0).unwrap();
        assert!(torso_xf.rotation.abs() < EPS);
        let c = torso.world_corners();
        let expected = torso_local_corners(1.0).map(|p| {
            [
                torso_xf.translation[0] + p[0],
                torso_xf.translation[1] + p[1],
            ]
        });
        assert!(corners_eq(c, expected));
        let bottom_w = (c[1][0] - c[0][0]).abs();
        let top_w = (c[2][0] - c[3][0]).abs();
        assert!(top_w > bottom_w);
        let trap_ratio = top_w / bottom_w;
        assert!(trap_ratio > 1.25 && trap_ratio < 1.45);
        assert!(c[2][0] - torso_xf.translation[0] > torso_xf.translation[0] - c[3][0]);
        let layout = torso_placeholder_layout();
        assert!(
            (c[0][1] - (torso_xf.translation[1] + layout.local_center[1] - layout.height * 0.5))
                .abs()
                < EPS
        );
        let rotated = DrawQuad::convex(
            torso_xf.translation,
            torso_local_corners(1.0),
            std::f32::consts::FRAC_PI_2,
            TORSO_PLACEHOLDER_COLOR,
        );
        let rc = rotated.world_corners();
        for p in rc {
            let dx = p[0] - torso_xf.translation[0];
            let dy = p[1] - torso_xf.translation[1];
            assert!(dx * dx + dy * dy > 0.01);
        }
        let dist = |p: [f32; 2]| {
            let dx = p[0] - torso_xf.translation[0];
            let dy = p[1] - torso_xf.translation[1];
            (dx * dx + dy * dy).sqrt()
        };
        for i in 0..4 {
            assert!((dist(c[i]) - dist(rc[i])).abs() < EPS);
        }
    }

    #[test]
    fn p4_placeholder_pieces_emit_once_in_layer_order() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let quads = skeleton_debug_quads(humanoid_v0(), rig.world(), CHARACTER_VISUAL_SCALE_115);
        let pieces = [
            p1_upper_arm_back_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p1_forearm_back_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p1_hand_back_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p2_upper_leg_back_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p2_shin_back_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p2_foot_back_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p3_torso_far_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p3_torso_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p2_upper_leg_front_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p2_shin_front_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p2_foot_front_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p3_head_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p1_upper_arm_front_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p1_forearm_front_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
            p1_hand_front_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap(),
        ];
        assert_eq!(pieces.len(), PLACEHOLDERS);
        let mut indices = Vec::new();
        for panel in pieces {
            let matches: Vec<usize> = quads
                .iter()
                .enumerate()
                .filter(|(_, q)| corners_eq(q.world_corners(), panel.world_corners()))
                .map(|(i, _)| i)
                .collect();
            assert_eq!(matches.len(), 1, "each body piece must emit exactly once");
            indices.push(matches[0]);
        }
        for pair in indices.windows(2) {
            assert!(
                pair[0] < pair[1],
                "placeholder draw order must be back → front"
            );
        }
        assert_eq!(
            skeleton_overlay_quads(humanoid_v0(), rig.world(), 1.0, false, true).len(),
            PLACEHOLDERS
        );
    }

    #[test]
    fn p4_back_arm_uses_back_bone_transforms() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let upper = p1_upper_arm_back_placeholder(rig.world(), root, 1.0).unwrap();
        let forearm = p1_forearm_back_placeholder(rig.world(), root, 1.0).unwrap();
        let hand = p1_hand_back_placeholder(rig.world(), root, 1.0).unwrap();
        let shoulder = rig.world().get(UPPER_ARM_BACK).unwrap().translation;
        let elbow = rig.world().get(LOWER_ARM_BACK).unwrap().translation;
        let wrist = rig.world().get(HAND_BACK).unwrap().translation;
        let (upper_attach, _) = limb_span_ends(upper);
        let (forearm_attach, _) = limb_span_ends(forearm);
        assert!(tr_eq(upper_attach, shoulder));
        assert!(tr_eq(forearm_attach, elbow));
        let hc = hand.world_corners();
        let hand_attach = [(hc[2][0] + hc[3][0]) * 0.5, (hc[2][1] + hc[3][1]) * 0.5];
        assert!(tr_eq(hand_attach, wrist));
        let front_upper = p1_upper_arm_front_placeholder(rig.world(), root, 1.0).unwrap();
        assert!(!corners_eq(
            upper.world_corners(),
            front_upper.world_corners()
        ));
    }

    #[test]
    fn p4_front_proofs_do_not_modify_back_arm_visuals() {
        let mut bind = HumanoidDebug::new();
        eval(
            &mut bind,
            [0.0, 0.0],
            FrontLegProof::Off,
            FrontArmProof::Off,
        );
        let root = bind.world().get(ROOT).unwrap().translation;
        let upper_b = p1_upper_arm_back_placeholder(bind.world(), root, 1.0).unwrap();
        let forearm_b = p1_forearm_back_placeholder(bind.world(), root, 1.0).unwrap();
        let hand_b = p1_hand_back_placeholder(bind.world(), root, 1.0).unwrap();
        let proofs = [
            (FrontLegProof::FootIndependent, FrontArmProof::Off),
            (FrontLegProof::ShinCarriesFoot, FrontArmProof::Off),
            (FrontLegProof::Off, FrontArmProof::HandIndependent),
            (FrontLegProof::Off, FrontArmProof::ForearmCarriesHand),
        ];
        for (leg, arm) in proofs {
            let mut proof = HumanoidDebug::new();
            eval(&mut proof, [0.0, 0.0], leg, arm);
            let upper_p = p1_upper_arm_back_placeholder(proof.world(), root, 1.0).unwrap();
            let forearm_p = p1_forearm_back_placeholder(proof.world(), root, 1.0).unwrap();
            let hand_p = p1_hand_back_placeholder(proof.world(), root, 1.0).unwrap();
            assert!(corners_eq(upper_b.world_corners(), upper_p.world_corners()));
            assert!(corners_eq(
                forearm_b.world_corners(),
                forearm_p.world_corners()
            ));
            assert!(corners_eq(hand_b.world_corners(), hand_p.world_corners()));
        }
    }

    #[test]
    fn p4_oriented_limbs_remain_rectangles() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let arm = p1_upper_arm_front_placeholder(rig.world(), root, 1.0).unwrap();
        let thigh = p2_upper_leg_front_placeholder(rig.world(), root, 1.0).unwrap();
        let expected_arm = placeholder_bone_toward_child(
            rig.world(),
            root,
            1.0,
            UPPER_ARM_FRONT,
            BIND_LOWER_ARM_FRONT.translation,
            ARM_PLACEHOLDER_WIDTH,
            UPPER_ARM_PLACEHOLDER_COLOR,
        )
        .unwrap();
        assert!(corners_eq(
            arm.world_corners(),
            expected_arm.world_corners()
        ));
        let ac = arm.world_corners();
        let edge_len = |a: [f32; 2], b: [f32; 2]| {
            let dx = a[0] - b[0];
            let dy = a[1] - b[1];
            (dx * dx + dy * dy).sqrt()
        };
        assert!((edge_len(ac[0], ac[1]) - edge_len(ac[3], ac[2])).abs() < EPS);
        assert!((edge_len(ac[0], ac[3]) - edge_len(ac[1], ac[2])).abs() < EPS);
        let tc = thigh.world_corners();
        assert!((edge_len(tc[0], tc[1]) - edge_len(tc[3], tc[2])).abs() < EPS);
    }

    #[test]
    fn debug_preview_2x_does_not_modify_gameplay_aabb_or_pose() {
        use purgatory_simulation::PLAYER_HALF_EXTENTS as HALF;
        let mut rig = HumanoidDebug::new();
        let center = [1.5, 2.5];
        eval(&mut rig, center, FrontLegProof::Off, FrontArmProof::Off);
        let locals_before: Vec<_> = (0u8..16)
            .map(|i| {
                let xf = rig.local().get(BoneIndex::from_u8(i)).unwrap();
                (xf.translation, xf.rotation)
            })
            .collect();
        let _ = skeleton_debug_quads(humanoid_v0(), rig.world(), CHARACTER_VISUAL_SCALE_2);
        for i in 0u8..16 {
            let local = rig.local().get(BoneIndex::from_u8(i)).unwrap();
            assert!(tr_eq(local.translation, locals_before[i as usize].0));
            assert!((local.rotation - locals_before[i as usize].1).abs() < EPS);
        }
        let size_1 = preview_local_player_size(CHARACTER_VISUAL_SCALE_1);
        let size_2 = preview_local_player_size(CHARACTER_VISUAL_SCALE_2);
        assert_eq!(size_1, [HALF[0] * 2.0, HALF[1] * 2.0]);
        assert!((size_2[0] - HALF[0] * 2.0 * CHARACTER_VISUAL_SCALE_2).abs() < EPS);
        assert_ne!(size_1, size_2);
        assert!((CHARACTER_VISUAL_SCALE_115 - 1.15).abs() < EPS);
    }

    #[test]
    fn p4_placeholder_count_stays_inside_renderer_budget() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        for scale in [
            CHARACTER_VISUAL_SCALE_1,
            CHARACTER_VISUAL_SCALE_115,
            CHARACTER_VISUAL_SCALE_2,
        ] {
            let quads = skeleton_debug_quads(humanoid_v0(), rig.world(), scale);
            assert_eq!(quads[quads.len() - PLACEHOLDERS..].len(), PLACEHOLDERS);
            assert!(quads.len() < MAX_QUADS);
            assert!(quads.len() <= MAX_QUADS * 3 / 4);
        }
    }

    #[test]
    fn p41_front_back_attachments_stay_distinct_without_stance_rotations() {
        assert!((purgatory_skeleton::BIND_UPPER_ARM_FRONT.translation[0] + 0.10).abs() < EPS);
        assert!((purgatory_skeleton::BIND_UPPER_ARM_BACK.translation[0] - 0.08).abs() < EPS);
        assert!(
            (purgatory_skeleton::BIND_UPPER_ARM_BACK.translation[0]
                - purgatory_skeleton::BIND_UPPER_ARM_FRONT.translation[0]
                - 0.18)
                .abs()
                < EPS
        );
        assert!((BIND_UPPER_LEG_FRONT.translation[0] + 0.06).abs() < EPS);
        assert!((purgatory_skeleton::BIND_UPPER_LEG_BACK.translation[0] - 0.04).abs() < EPS);
        assert!(
            (purgatory_skeleton::BIND_UPPER_LEG_BACK.translation[0]
                - BIND_UPPER_LEG_FRONT.translation[0]
                - 0.10)
                .abs()
                < EPS
        );
        assert!(purgatory_skeleton::BIND_UPPER_ARM_FRONT.rotation.abs() < EPS);
        assert!(purgatory_skeleton::BIND_UPPER_ARM_BACK.rotation.abs() < EPS);
        assert!(BIND_UPPER_LEG_FRONT.rotation.abs() < EPS);
        assert!(purgatory_skeleton::BIND_UPPER_LEG_BACK.rotation.abs() < EPS);
        assert!(BIND_LOWER_ARM_FRONT.translation[0].abs() < EPS);
        assert!(BIND_LOWER_LEG_FRONT.translation[0].abs() < EPS);
        assert!((purgatory_skeleton::BIND_LOWER_ARM_BACK.rotation - 0.50).abs() < EPS);
        assert!(purgatory_skeleton::BIND_HAND_BACK.rotation.abs() < EPS);
    }

    #[test]
    fn p41_draw_order_keeps_back_arm_behind_torso_and_front_arm_above() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let quads = skeleton_debug_quads(humanoid_v0(), rig.world(), CHARACTER_VISUAL_SCALE_115);
        let back =
            p1_upper_arm_back_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap();
        let torso = p3_torso_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap();
        let front =
            p1_upper_arm_front_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap();
        let back_i = find_placeholder_index(&quads, back);
        let torso_i = find_placeholder_index(&quads, torso);
        let front_i = find_placeholder_index(&quads, front);
        assert!(back_i < torso_i);
        assert!(torso_i < front_i);
    }

    #[test]
    fn p41_three_quarter_torso_corners_are_deterministic() {
        let a = torso_local_corners(1.0);
        let b = torso_local_corners(1.0);
        assert!(corners_eq(a, b));
        assert!(a[2][0] > a[3][0].abs());
        assert!(
            (sanitize_preview_scale(CHARACTER_VISUAL_SCALE_115) - CHARACTER_VISUAL_SCALE_115).abs()
                < EPS
        );
        assert!(
            (sanitize_preview_scale(CHARACTER_VISUAL_SCALE_2) - CHARACTER_VISUAL_SCALE_2).abs()
                < EPS
        );
    }

    fn rgb_luma(color: [f32; 4]) -> f32 {
        color[0] + color[1] + color[2]
    }

    #[test]
    fn p42_back_placeholders_are_dimmer_than_front() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let root = rig.world().get(ROOT).unwrap().translation;
        let front_arm = p1_upper_arm_front_placeholder(rig.world(), root, 1.0).unwrap();
        let back_arm = p1_upper_arm_back_placeholder(rig.world(), root, 1.0).unwrap();
        let front_leg = p2_upper_leg_front_placeholder(rig.world(), root, 1.0).unwrap();
        let back_leg = p2_upper_leg_back_placeholder(rig.world(), root, 1.0).unwrap();
        let torso = p3_torso_placeholder(rig.world(), root, 1.0).unwrap();
        let far = p3_torso_far_placeholder(rig.world(), root, 1.0).unwrap();
        assert!(rgb_luma(front_arm.color) > rgb_luma(torso.color));
        assert!(rgb_luma(torso.color) > rgb_luma(far.color));
        assert!(rgb_luma(front_arm.color) > rgb_luma(back_arm.color));
        assert!(rgb_luma(front_leg.color) > rgb_luma(back_leg.color));
    }

    #[test]
    fn p42_head_silhouette_leads_toward_right() {
        let c = head_local_corners(1.0);
        assert!((c[0][0] + c[1][0]).abs() < EPS);
        assert!(c[2][0] > c[3][0].abs());
        assert!((c[2][1] - HEAD_PLACEHOLDER_SIZE[1]).abs() < EPS);
        assert!(c[0][1].abs() < EPS);
    }

    #[test]
    fn p42_back_forearm_peeks_toward_right() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let elbow = rig.world().get(LOWER_ARM_BACK).unwrap().translation;
        let wrist = rig.world().get(HAND_BACK).unwrap().translation;
        let shoulder = rig.world().get(UPPER_ARM_BACK).unwrap().translation;
        assert!((elbow[0] - shoulder[0]).abs() < EPS);
        assert!(wrist[0] > elbow[0]);
        assert!(wrist[1] < elbow[1]);
        assert!((purgatory_skeleton::BIND_LOWER_ARM_BACK.rotation - 0.50).abs() < EPS);
        assert!(purgatory_skeleton::BIND_UPPER_ARM_BACK.rotation.abs() < EPS);
        assert!(BIND_HAND_BACK.rotation.abs() < EPS);
        let root = rig.world().get(ROOT).unwrap().translation;
        let quads = skeleton_debug_quads(humanoid_v0(), rig.world(), CHARACTER_VISUAL_SCALE_115);
        let back =
            p1_upper_arm_back_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap();
        let torso = p3_torso_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap();
        let front =
            p1_upper_arm_front_placeholder(rig.world(), root, CHARACTER_VISUAL_SCALE_115).unwrap();
        assert!(find_placeholder_index(&quads, back) < find_placeholder_index(&quads, torso));
        assert!(find_placeholder_index(&quads, torso) < find_placeholder_index(&quads, front));
    }

    #[test]
    fn character_placeholder_quads_skip_hidden_torso() {
        let mut rig = HumanoidDebug::new();
        eval(&mut rig, [0.0, 0.0], FrontLegProof::Off, FrontArmProof::Off);
        let full = character_placeholder_quads(rig.world(), 1.0, |_| false);
        let hidden = character_placeholder_quads(rig.world(), 1.0, |bone| bone == TORSO);
        assert_eq!(full.len(), PLACEHOLDERS);
        assert_eq!(hidden.len(), PLACEHOLDERS - 2);
    }

    #[test]
    fn ndc_origin_maps_to_window_center() {
        let px = ndc_to_window_px([0.0, 0.0], 1280, 720);
        assert!((px[0] - 640.0).abs() < 1e-4);
        assert!((px[1] - 360.0).abs() < 1e-4);
    }

    #[test]
    fn ndc_top_right_maps_to_window_top_right() {
        let px = ndc_to_window_px([1.0, 1.0], 1280, 720);
        assert!((px[0] - 1280.0).abs() < 1e-4);
        assert!(px[1].abs() < 1e-4);
    }
}
