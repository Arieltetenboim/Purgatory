//! Blend two caller-owned LocalPoses into a third. No mixer/layers.

use purgatory_skeleton::LocalPose;

use crate::sample::shortest_angle_delta;

/// Why pose blending failed. On error, `out` is left unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendError {
    NonFiniteAlpha,
    BoneCountMismatch { from: usize, to: usize, out: usize },
}

/// Blend `from` → `to` into `out` with `alpha` clamped to `[0, 1]`.
///
/// Translation channels use linear interpolation. Rotation uses shortest-angle
/// interpolation. Does not allocate. Does not mutate `SkeletonDef`.
/// Callers that own presentation root placement should rewrite `root` after blending.
pub fn blend_local_poses(
    from: &LocalPose,
    to: &LocalPose,
    alpha: f32,
    out: &mut LocalPose,
) -> Result<(), BlendError> {
    if !alpha.is_finite() {
        return Err(BlendError::NonFiniteAlpha);
    }
    let from_n = from.bone_count();
    let to_n = to.bone_count();
    let out_n = out.bone_count();
    if from_n != to_n || from_n != out_n {
        return Err(BlendError::BoneCountMismatch {
            from: from_n,
            to: to_n,
            out: out_n,
        });
    }

    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.0 {
        out.copy_from(from)
            .map_err(|_| BlendError::BoneCountMismatch {
                from: from_n,
                to: to_n,
                out: out_n,
            })?;
        return Ok(());
    }
    if a >= 1.0 {
        out.copy_from(to)
            .map_err(|_| BlendError::BoneCountMismatch {
                from: from_n,
                to: to_n,
                out: out_n,
            })?;
        return Ok(());
    }

    let from_s = from.as_slice();
    let to_s = to.as_slice();
    let out_s = out.as_mut_slice();
    for i in 0..from_n {
        let f = from_s[i];
        let t = to_s[i];
        out_s[i].translation[0] = f.translation[0] + (t.translation[0] - f.translation[0]) * a;
        out_s[i].translation[1] = f.translation[1] + (t.translation[1] - f.translation[1]) * a;
        let delta = shortest_angle_delta(f.rotation, t.rotation);
        out_s[i].rotation = f.rotation + delta * a;
    }
    Ok(())
}
