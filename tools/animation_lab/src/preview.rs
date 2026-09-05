//! Preview evaluation, camera fit, and direct-manipulation math. No hidden pose buffer.

use purgatory_animation::{
    DepthPose, apply_depth_projection, blend_depth_poses, blend_local_poses, sample, sample_depth,
};
use purgatory_skeleton::{BoneIndex, LocalPose, WorldPose, evaluate, humanoid_v0};

use crate::document::{
    AnimDocument, ChannelKind, clamp_key_time, rotation_authorable, times_equal,
};

/// World window shown in the preview. Padding leaves room for jump / reach.
/// Preview-camera only; does not change authored clip values.
pub const VIEW_X0: f32 = -0.70;
pub const VIEW_X1: f32 = 0.70;
pub const VIEW_Y0: f32 = -0.18;
pub const VIEW_Y1: f32 = 1.42;

#[derive(Clone, Copy, Debug)]
pub struct PreviewCamera {
    /// Canvas pixel of world origin (0, 0).
    pub origin: [f32; 2],
    /// Pixels per world unit. Independent of clip / skeleton data.
    pub scale: f32,
}

impl PreviewCamera {
    #[must_use]
    pub fn to_canvas(&self, xy: [f32; 2]) -> [f32; 2] {
        [
            self.origin[0] + xy[0] * self.scale,
            self.origin[1] - xy[1] * self.scale,
        ]
    }

    #[must_use]
    pub fn to_world(&self, canvas: [f32; 2]) -> [f32; 2] {
        if self.scale.abs() < 1e-6 {
            return [0.0, 0.0];
        }
        [
            (canvas[0] - self.origin[0]) / self.scale,
            (self.origin[1] - canvas[1]) / self.scale,
        ]
    }
}

/// Fit the Humanoid v0 working volume into `canvas` (left, top, width, height).
#[must_use]
pub fn fit_preview_camera(left: f32, top: f32, width: f32, height: f32) -> PreviewCamera {
    let width = width.max(1.0);
    let height = height.max(1.0);
    let world_w = (VIEW_X1 - VIEW_X0).max(0.01);
    let world_h = (VIEW_Y1 - VIEW_Y0).max(0.01);
    let pad_x = 16.0;
    let pad_y = 20.0;
    let usable_w = (width - pad_x * 2.0).max(1.0);
    let usable_h = (height - pad_y * 2.0).max(1.0);
    let scale = (usable_w / world_w).min(usable_h / world_h);
    let origin_x = left + width * 0.5 - (VIEW_X0 + VIEW_X1) * 0.5 * scale;
    let origin_y = top + pad_y + VIEW_Y1 * scale;
    PreviewCamera {
        origin: [origin_x, origin_y],
        scale,
    }
}

#[derive(Clone, Debug)]
pub struct EvaluatedPreview {
    pub local: LocalPose,
    pub world: WorldPose,
    pub depth: DepthPose,
}

pub fn evaluate_document(document: &AnimDocument, t: f32) -> Result<EvaluatedPreview, String> {
    evaluate_preview(document, t, None, 0.0)
}

/// Sample clip A, optionally blend toward clip B (`alpha` 0=A … 1=B), then project depth.
///
/// `t` is used for clip A. Clip B is sampled at the same time, clamped to B's duration.
pub fn evaluate_preview(
    document: &AnimDocument,
    t: f32,
    other: Option<&AnimDocument>,
    alpha: f32,
) -> Result<EvaluatedPreview, String> {
    let t_b = other.map(|b| t.clamp(0.0, b.duration)).unwrap_or(t);
    evaluate_preview_at(document, t, other, t_b, alpha)
}

/// Same as [`evaluate_preview`] with an explicit sample time for clip B.
pub fn evaluate_preview_at(
    document: &AnimDocument,
    t_a: f32,
    other: Option<&AnimDocument>,
    t_b: f32,
    alpha: f32,
) -> Result<EvaluatedPreview, String> {
    let def = humanoid_v0();
    let (mut local, mut depth) = sample_unprojected(document, t_a)?;
    if let Some(other_doc) = other {
        let a = alpha.clamp(0.0, 1.0);
        if a > 0.0 {
            let (local_b, depth_b) = sample_unprojected(other_doc, t_b)?;
            let mut blended = LocalPose::from_bind(def);
            let mut blended_depth = DepthPose::zeros(def.bone_count());
            blend_local_poses(&local, &local_b, a, &mut blended)
                .map_err(|e| format!("blend failed: {e:?}"))?;
            blend_depth_poses(&depth, &depth_b, a, &mut blended_depth)
                .map_err(|e| format!("depth blend failed: {e:?}"))?;
            local = blended;
            depth = blended_depth;
        }
    }
    apply_depth_projection(def, &depth, &mut local)
        .map_err(|e| format!("depth project failed: {e:?}"))?;
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).map_err(|e| format!("evaluate failed: {e:?}"))?;
    Ok(EvaluatedPreview {
        local,
        world,
        depth,
    })
}

fn sample_unprojected(document: &AnimDocument, t: f32) -> Result<(LocalPose, DepthPose), String> {
    let def = humanoid_v0();
    let clip = document
        .to_clip(def)
        .map_err(|e| format!("clip validation failed: {e:?}"))?;
    let mut local = LocalPose::from_bind(def);
    sample(&clip, t, &mut local).map_err(|e| format!("sample failed: {e:?}"))?;
    let mut depth = DepthPose::zeros(def.bone_count());
    sample_depth(&clip, t, &mut depth).map_err(|e| format!("sample depth failed: {e:?}"))?;
    Ok((local, depth))
}

/// Horizontal mirror about the root world X. Presentation only.
#[must_use]
pub fn mirror_x(xy: [f32; 2], root_x: f32) -> [f32; 2] {
    [2.0 * root_x - xy[0], xy[1]]
}

#[must_use]
pub fn pick_bone(world: &WorldPose, skeleton_xy: [f32; 2], radius: f32) -> Option<BoneIndex> {
    let def = humanoid_v0();
    let mut best: Option<(f32, BoneIndex)> = None;
    for i in 0..def.bone_count() {
        let bone = BoneIndex::from_u8(i as u8);
        let Some(xf) = world.get(bone) else {
            continue;
        };
        let dx = xf.translation[0] - skeleton_xy[0];
        let dy = xf.translation[1] - skeleton_xy[1];
        let d = (dx * dx + dy * dy).sqrt();
        if d <= radius && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, bone));
        }
    }
    best.map(|(_, b)| b)
}

/// World-space angle of the vector from `origin` to `point`.
#[must_use]
pub fn angle_about(origin: [f32; 2], point: [f32; 2]) -> f32 {
    (point[1] - origin[1]).atan2(point[0] - origin[0])
}

pub fn apply_direct_rotation(
    document: &mut AnimDocument,
    bone: BoneIndex,
    playhead: f32,
    local_rotation: f32,
) -> Result<(), String> {
    if !rotation_authorable(bone) {
        return Err("root is not keyable".to_string());
    }
    if !local_rotation.is_finite() {
        return Err("non-finite rotation".to_string());
    }
    let time = clamp_key_time(playhead, document.duration);
    let interp = document
        .track(bone)
        .and_then(|t| t.rotation.iter().find(|k| times_equal(k.time, time)))
        .map(|k| k.interpolation)
        .unwrap_or(purgatory_animation::Interpolation::Linear);
    document.upsert_key(
        bone,
        ChannelKind::Rotation,
        purgatory_animation::Keyframe {
            time,
            value: local_rotation,
            interpolation: interp,
        },
    )
}
