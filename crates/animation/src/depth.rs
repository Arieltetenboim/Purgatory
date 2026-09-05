//! Depth / foreshortening: signed limb projection angle as an authored channel.
//!
//! `depth_angle = 0` leaves parent-relative translation unchanged (current 2D).
//! Non-zero values scale each child's parent-relative offset around the parent
//! joint: `projected_length = current_length × cos(|θ|)` with a small minimum
//! so geometry never collapses to zero. Sign is preserved for later presentation
//! use; it does not swap Front/Back identity and does not change projection
//! length vs the opposite sign.

use purgatory_skeleton::{BoneIndex, LocalPose, SkeletonDef};

use crate::clip::AnimationClip;
use crate::sample::{SampleError, sample, sample_scalar};

pub use crate::clip::{DEPTH_ANGLE_LIMIT, DEPTH_PROJECTION_MIN};

/// Per-bone sampled depth angles. Default `0` matches unkeyed / pre-A7.1 clips.
#[derive(Clone, Debug, PartialEq)]
pub struct DepthPose {
    angles: Box<[f32]>,
}

impl DepthPose {
    #[must_use]
    pub fn zeros(bone_count: usize) -> Self {
        Self {
            angles: vec![0.0; bone_count].into_boxed_slice(),
        }
    }

    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.angles.len()
    }

    #[must_use]
    pub fn get(&self, bone: BoneIndex) -> Option<f32> {
        self.angles.get(bone.as_usize()).copied()
    }

    pub fn set(&mut self, bone: BoneIndex, angle: f32) -> bool {
        if let Some(slot) = self.angles.get_mut(bone.as_usize()) {
            *slot = angle;
            true
        } else {
            false
        }
    }

    #[must_use]
    pub fn as_slice(&self) -> &[f32] {
        &self.angles
    }

    pub fn fill_zero(&mut self) {
        self.angles.fill(0.0);
    }

    pub fn copy_from(&mut self, other: &Self) -> Result<(), SampleError> {
        if self.angles.len() != other.angles.len() {
            return Err(SampleError::BoneCountMismatch {
                clip: other.angles.len(),
                local: self.angles.len(),
            });
        }
        self.angles.copy_from_slice(&other.angles);
        Ok(())
    }
}

/// Projected length factor for a signed depth angle.
///
/// `0` → `1`. `±90°` → [`DEPTH_PROJECTION_MIN`]. Non-finite → `1` (no-op).
#[must_use]
pub fn depth_projection(angle: f32) -> f32 {
    if !angle.is_finite() {
        return 1.0;
    }
    let clamped = angle.clamp(-DEPTH_ANGLE_LIMIT, DEPTH_ANGLE_LIMIT);
    clamped.cos().max(DEPTH_PROJECTION_MIN)
}

/// Write keyed `depth_angle` channels into `depth`. Caller should `fill_zero` first.
///
/// Unkeyed bones stay as the caller left them (normally `0`). On `Err`, `depth`
/// is left unchanged by collecting writes after the same guards as [`sample`].
pub fn sample_depth(
    clip: &AnimationClip,
    t: f32,
    depth: &mut DepthPose,
) -> Result<(), SampleError> {
    if !t.is_finite() {
        return Err(SampleError::NonFiniteTime);
    }
    if depth.bone_count() != clip.bone_count() {
        return Err(SampleError::BoneCountMismatch {
            clip: clip.bone_count(),
            local: depth.bone_count(),
        });
    }
    for track in clip.tracks() {
        let Some(channel) = track.depth_angle.as_ref() else {
            continue;
        };
        let i = track.bone.as_usize();
        if i >= depth.angles.len() {
            continue;
        }
        depth.angles[i] = sample_scalar(channel, t);
    }
    Ok(())
}

/// Scale each child's parent-relative translation by the **parent** bone's depth.
///
/// Parent joint stays put. Child joint moves along the current offset ray.
/// Descendants inherit through evaluate. Does not allocate.
pub fn apply_depth_projection(
    def: &SkeletonDef,
    depth: &DepthPose,
    local: &mut LocalPose,
) -> Result<(), SampleError> {
    let n = def.bone_count();
    if local.bone_count() != n || depth.bone_count() != n {
        return Err(SampleError::BoneCountMismatch {
            clip: n,
            local: local.bone_count(),
        });
    }
    for parent_i in 0..n {
        let factor = depth_projection(depth.angles[parent_i]);
        if (factor - 1.0).abs() < 1e-8 {
            continue;
        }
        let parent = BoneIndex::from_u8(parent_i as u8);
        for child_i in 0..n {
            let child = BoneIndex::from_u8(child_i as u8);
            if def.parent(child) != Some(parent) {
                continue;
            }
            if let Some(xf) = local.get_mut(child) {
                xf.translation[0] *= factor;
                xf.translation[1] *= factor;
            }
        }
    }
    Ok(())
}

/// `copy_bind` is the caller's job. Sample rot/tx/ty, sample depth, then project.
pub fn sample_and_project(
    def: &SkeletonDef,
    clip: &AnimationClip,
    t: f32,
    local: &mut LocalPose,
    depth: &mut DepthPose,
) -> Result<(), SampleError> {
    sample(clip, t, local)?;
    depth.fill_zero();
    sample_depth(clip, t, depth)?;
    apply_depth_projection(def, depth, local)?;
    Ok(())
}

/// Linear blend of depth angles. Same alpha rules as pose blend.
pub fn blend_depth_poses(
    from: &DepthPose,
    to: &DepthPose,
    alpha: f32,
    out: &mut DepthPose,
) -> Result<(), crate::blend::BlendError> {
    if !alpha.is_finite() {
        return Err(crate::blend::BlendError::NonFiniteAlpha);
    }
    let from_n = from.bone_count();
    let to_n = to.bone_count();
    let out_n = out.bone_count();
    if from_n != to_n || from_n != out_n {
        return Err(crate::blend::BlendError::BoneCountMismatch {
            from: from_n,
            to: to_n,
            out: out_n,
        });
    }
    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.0 {
        out.copy_from(from)
            .map_err(|_| crate::blend::BlendError::BoneCountMismatch {
                from: from_n,
                to: to_n,
                out: out_n,
            })?;
        return Ok(());
    }
    if a >= 1.0 {
        out.copy_from(to)
            .map_err(|_| crate::blend::BlendError::BoneCountMismatch {
                from: from_n,
                to: to_n,
                out: out_n,
            })?;
        return Ok(());
    }
    for i in 0..from_n {
        out.angles[i] = from.angles[i] + (to.angles[i] - from.angles[i]) * a;
    }
    Ok(())
}
