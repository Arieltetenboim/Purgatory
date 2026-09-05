//! A1/A2 proof clips are hard-coded in Rust.
//!
//! A3-A5 and ClimbBack proof clips are A6 data-driven: parsed + validated from
//! external `.anim` assets into immutable [`AnimationClip`] values.

use std::sync::OnceLock;

use purgatory_skeleton::{HEAD, humanoid_v0};

use crate::asset::{bind_rotation_noop_clip, parse_animation_asset_v1};
use crate::clip::{AnimationClip, BoneTrack, Interpolation, Keyframe, LoopPolicy};

/// Duration of the A1/A2 head rotation proof clips in presentation seconds.
pub const A1_HEAD_CLIP_DURATION: f32 = 1.0;

/// Duration of the A3 debug Idle loop (seconds).
pub const A3_IDLE_CLIP_DURATION: f32 = 2.0;

/// Duration of the A3 debug Move loop (seconds).
pub const A3_MOVE_CLIP_DURATION: f32 = 0.8;

// A6 data-driven proof clips: A3-A5 are validated into AnimationClip at
// construction-time and sampled via the existing player/sample pipeline.
const A3_IDLE_ASSET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../content/shared/animations/dev/a3_idle.anim"
));
const A3_MOVE_ASSET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../content/shared/animations/dev/a3_move.anim"
));
const A4_JUMP_ASSET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../content/shared/animations/dev/a4_jump.anim"
));
const A4_FALL_ASSET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../content/shared/animations/dev/a4_fall.anim"
));
const A5_ATTACK_ASSET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../content/shared/animations/dev/a5_attack.anim"
));
const A5_HURT_ASSET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../content/shared/animations/dev/a5_hurt.anim"
));
const CLIMB_BACK_ASSET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../content/shared/animations/dev/climb_back.anim"
));

/// Duration of the A4 debug Jump loop (seconds).
pub const A4_JUMP_CLIP_DURATION: f32 = 0.6;

/// Duration of the A4 debug Fall loop (seconds).
pub const A4_FALL_CLIP_DURATION: f32 = 0.6;

/// Fixed A4 activity-change pose blend duration (seconds).
pub const A4_TRANSITION_DURATION: f32 = 0.10;

/// Duration of the A5 debug Attack one-shot (seconds).
pub const A5_ATTACK_CLIP_DURATION: f32 = 0.40;

/// Duration of the A5 debug Hurt one-shot (seconds).
pub const A5_HURT_CLIP_DURATION: f32 = 0.35;

/// Bind/no-animation fallback only. Authored ClimbBack duration lives on the parsed clip.
const CLIMB_BACK_BIND_FALLBACK_DURATION: f32 = 1.0;

fn linear_keys(times_values: &[(f32, f32)]) -> Vec<Keyframe> {
    times_values
        .iter()
        .map(|(t, v)| Keyframe {
            time: *t,
            value: *v,
            interpolation: Interpolation::Linear,
        })
        .collect()
}

fn head_rotation_keys() -> Vec<Keyframe> {
    linear_keys(&[(0.0, 0.0), (0.5, 0.40), (1.0, -0.40)])
}

/// Shared immutable A1 proof clip: `head` rotation only. Does not key `root`.
///
/// Keys: t=0 → 0 (bind-equivalent), t=0.5 → +0.40 rad, t=1.0 → −0.40 rad.
/// [`LoopPolicy::Once`].
#[must_use]
pub fn a1_head_rotation_clip() -> &'static AnimationClip {
    static CLIP: OnceLock<AnimationClip> = OnceLock::new();
    CLIP.get_or_init(|| {
        let def = humanoid_v0();
        let track = BoneTrack::rotation_only(HEAD, head_rotation_keys());
        AnimationClip::try_new(def, A1_HEAD_CLIP_DURATION, LoopPolicy::Once, vec![track])
            .expect("A1 head clip is valid for Humanoid v0")
    })
}

/// A2 dev/test fixture: same key data as [`a1_head_rotation_clip`] with [`LoopPolicy::Loop`].
///
/// This duplicates keys only for the A2 playback proof. It is **not** a long-term
/// asset-authoring rule and does not establish that identical animation data should
/// be duplicated merely to vary playback policy. LoopPolicy placement may be
/// reassessed before broader clip authoring.
#[must_use]
pub fn a1_head_loop_clip() -> &'static AnimationClip {
    static CLIP: OnceLock<AnimationClip> = OnceLock::new();
    CLIP.get_or_init(|| {
        let def = humanoid_v0();
        let track = BoneTrack::rotation_only(HEAD, head_rotation_keys());
        AnimationClip::try_new(def, A1_HEAD_CLIP_DURATION, LoopPolicy::Loop, vec![track])
            .expect("A1 head loop clip is valid for Humanoid v0")
    })
}

/// A3 debug Idle: subtle looping pelvis / torso / head rotation. No root, no translation.
#[must_use]
pub fn a3_idle_clip() -> &'static AnimationClip {
    static CLIP: OnceLock<AnimationClip> = OnceLock::new();
    CLIP.get_or_init(|| {
        let def = humanoid_v0();
        parse_animation_asset_v1("a3_idle.anim", A3_IDLE_ASSET, def)
            .map(|asset| asset.clip)
            .unwrap_or_else(|err| {
                eprintln!("A3 idle asset invalid: {err}; falling back to bind/no-animation");
                bind_rotation_noop_clip(def, A3_IDLE_CLIP_DURATION, LoopPolicy::Loop)
            })
    })
}

/// A3 debug Move: clearly different looping limb swing. No root, no translation.
///
/// Authored for Facing::Right: positive hanging-limb rotation displaces the
/// distal child toward +X (forward). Near-leg forward pairs with near-arm back.
#[must_use]
pub fn a3_move_clip() -> &'static AnimationClip {
    static CLIP: OnceLock<AnimationClip> = OnceLock::new();
    CLIP.get_or_init(|| {
        let def = humanoid_v0();
        parse_animation_asset_v1("a3_move.anim", A3_MOVE_ASSET, def)
            .map(|asset| asset.clip)
            .unwrap_or_else(|err| {
                eprintln!("A3 move asset invalid: {err}; falling back to bind/no-animation");
                bind_rotation_noop_clip(def, A3_MOVE_CLIP_DURATION, LoopPolicy::Loop)
            })
    })
}

/// A4 debug Jump: arms raised forward/up, legs tucked. Distinct from Idle/Move/Fall. No root.
/// Authored for Facing::Right (positive hanging-limb rotation → +X).
#[must_use]
pub fn a4_jump_clip() -> &'static AnimationClip {
    static CLIP: OnceLock<AnimationClip> = OnceLock::new();
    CLIP.get_or_init(|| {
        let def = humanoid_v0();
        parse_animation_asset_v1("a4_jump.anim", A4_JUMP_ASSET, def)
            .map(|asset| asset.clip)
            .unwrap_or_else(|err| {
                eprintln!("A4 jump asset invalid: {err}; falling back to bind/no-animation");
                bind_rotation_noop_clip(def, A4_JUMP_CLIP_DURATION, LoopPolicy::Loop)
            })
    })
}

/// A4 debug Fall: arms out/back, legs more open. Distinct from Jump. No root.
/// Authored for Facing::Right (positive hanging-limb rotation → +X).
#[must_use]
pub fn a4_fall_clip() -> &'static AnimationClip {
    static CLIP: OnceLock<AnimationClip> = OnceLock::new();
    CLIP.get_or_init(|| {
        let def = humanoid_v0();
        parse_animation_asset_v1("a4_fall.anim", A4_FALL_ASSET, def)
            .map(|asset| asset.clip)
            .unwrap_or_else(|err| {
                eprintln!("A4 fall asset invalid: {err}; falling back to bind/no-animation");
                bind_rotation_noop_clip(def, A4_FALL_CLIP_DURATION, LoopPolicy::Loop)
            })
    })
}

/// A5 debug Attack: one-shot forward arm swing toward +X. Distinct from locomotion. No root.
/// Authored for Facing::Right (positive hanging-limb rotation → +X / screen-right).
#[must_use]
pub fn a5_attack_clip() -> &'static AnimationClip {
    static CLIP: OnceLock<AnimationClip> = OnceLock::new();
    CLIP.get_or_init(|| {
        let def = humanoid_v0();
        parse_animation_asset_v1("a5_attack.anim", A5_ATTACK_ASSET, def)
            .map(|asset| asset.clip)
            .unwrap_or_else(|err| {
                eprintln!("A5 attack asset invalid: {err}; falling back to bind/no-animation");
                bind_rotation_noop_clip(def, A5_ATTACK_CLIP_DURATION, LoopPolicy::Once)
            })
    })
}

/// A5 debug Hurt: one-shot flinch. Distinct from Attack/locomotion. No root.
/// Authored for Facing::Right. Torso lean uses positive rotation so the head
/// recoils toward −X (away from facing-forward); arms raise with mixed signs.
#[must_use]
pub fn a5_hurt_clip() -> &'static AnimationClip {
    static CLIP: OnceLock<AnimationClip> = OnceLock::new();
    CLIP.get_or_init(|| {
        let def = humanoid_v0();
        parse_animation_asset_v1("a5_hurt.anim", A5_HURT_ASSET, def)
            .map(|asset| asset.clip)
            .unwrap_or_else(|err| {
                eprintln!("A5 hurt asset invalid: {err}; falling back to bind/no-animation");
                bind_rotation_noop_clip(def, A5_HURT_CLIP_DURATION, LoopPolicy::Once)
            })
    })
}

/// ClimbBack: authored `climb_back.anim` (pelvis `tx` loop). No root. Invalid
/// parse falls back to bind/no-animation, not Idle.
#[must_use]
pub fn climb_back_clip() -> &'static AnimationClip {
    static CLIP: OnceLock<AnimationClip> = OnceLock::new();
    CLIP.get_or_init(|| {
        let def = humanoid_v0();
        parse_animation_asset_v1("climb_back.anim", CLIMB_BACK_ASSET, def)
            .map(|asset| asset.clip)
            .unwrap_or_else(|err| {
                eprintln!("climb_back asset invalid: {err}; falling back to bind/no-animation");
                bind_rotation_noop_clip(def, CLIMB_BACK_BIND_FALLBACK_DURATION, LoopPolicy::Loop)
            })
    })
}
