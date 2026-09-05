//! Sample an immutable clip at an explicit presentation time into a LocalPose.

use purgatory_skeleton::LocalPose;

use crate::clip::{AnimationClip, Channel, Interpolation, Keyframe};

/// Why sampling failed. On any error, `sample` leaves `LocalPose` unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleError {
    NonFiniteTime,
    BoneCountMismatch { clip: usize, local: usize },
}

/// Write keyed channels from `clip` at time `t` into `local`.
///
/// Caller must `copy_bind` first. Unkeyed bones/channels stay as the caller left them.
/// Does not wrap/loop `t`. Does not mutate `SkeletonDef`. On `Err`, `local` is unchanged.
pub fn sample(clip: &AnimationClip, t: f32, local: &mut LocalPose) -> Result<(), SampleError> {
    if !t.is_finite() {
        return Err(SampleError::NonFiniteTime);
    }
    if local.bone_count() != clip.bone_count() {
        return Err(SampleError::BoneCountMismatch {
            clip: clip.bone_count(),
            local: local.bone_count(),
        });
    }

    // Collect writes first so a mid-loop failure cannot leave a partial pose.
    // A1 clips are tiny (one bone / few keys); stack-sized temp via Vec is fine at
    // construction of the write list, but we avoid heap on the success path by
    // writing directly after the bone-count check — validation already guarantees
    // tracks are consistent, so the only remaining errors are checked above.
    for track in clip.tracks() {
        let Some(bone) = local.get_mut(track.bone) else {
            // Bone index was validated against bone_count at construction; local
            // length matches. Unreachable in practice.
            continue;
        };
        if let Some(channel) = track.rotation.as_ref() {
            bone.rotation = sample_rotation(channel, t);
        }
        if let Some(channel) = track.translation_x.as_ref() {
            bone.translation[0] = sample_scalar(channel, t);
        }
        if let Some(channel) = track.translation_y.as_ref() {
            bone.translation[1] = sample_scalar(channel, t);
        }
    }
    Ok(())
}

pub(crate) fn sample_scalar(channel: &Channel, t: f32) -> f32 {
    let keys = channel.keys();
    debug_assert!(!keys.is_empty());
    if t <= keys[0].time {
        return keys[0].value;
    }
    let last = keys.len() - 1;
    if t >= keys[last].time {
        return keys[last].value;
    }
    for i in 0..last {
        let a = &keys[i];
        let b = &keys[i + 1];
        if t == a.time {
            return a.value;
        }
        if t < b.time {
            return interpolate_scalar(a, b, t);
        }
        if t == b.time {
            return b.value;
        }
    }
    keys[last].value
}

fn sample_rotation(channel: &Channel, t: f32) -> f32 {
    let keys = channel.keys();
    debug_assert!(!keys.is_empty());
    if t <= keys[0].time {
        return keys[0].value;
    }
    let last = keys.len() - 1;
    if t >= keys[last].time {
        return keys[last].value;
    }
    for i in 0..last {
        let a = &keys[i];
        let b = &keys[i + 1];
        if t == a.time {
            return a.value;
        }
        if t < b.time {
            return interpolate_rotation(a, b, t);
        }
        if t == b.time {
            return b.value;
        }
    }
    keys[last].value
}

fn interpolate_scalar(a: &Keyframe, b: &Keyframe, t: f32) -> f32 {
    match a.interpolation {
        Interpolation::Step => a.value,
        Interpolation::Linear => {
            let span = b.time - a.time;
            if span == 0.0 {
                return a.value;
            }
            let alpha = (t - a.time) / span;
            a.value + (b.value - a.value) * alpha
        }
    }
}

fn interpolate_rotation(a: &Keyframe, b: &Keyframe, t: f32) -> f32 {
    match a.interpolation {
        Interpolation::Step => a.value,
        Interpolation::Linear => {
            let span = b.time - a.time;
            if span == 0.0 {
                return a.value;
            }
            let alpha = (t - a.time) / span;
            let delta = shortest_angle_delta(a.value, b.value);
            a.value + delta * alpha
        }
    }
}

/// Signed shortest delta from `from` to `to`, wrapped into `(-π, π]`.
#[must_use]
pub(crate) fn shortest_angle_delta(from: f32, to: f32) -> f32 {
    let mut d = to - from;
    let pi = core::f32::consts::PI;
    let two_pi = 2.0 * pi;
    d = (d + pi).rem_euclid(two_pi) - pi;
    if d <= -pi { d + two_pi } else { d }
}
