//! Presentation animation sampling.
//!
//! Core contract: **immutable AnimationClip + explicit time t → write keyed channels
//! into a caller-owned LocalPose**.
//!
//! This crate has no knowledge of Player/NPC, velocity, networking, wgpu, egui,
//! simulation, or protocol. Loop / Once wrap lives in [`AnimationPlayer`];
//! `sample` never wraps time.

mod asset;
mod blend;
mod clip;
mod depth;
mod dev;
mod player;
mod sample;

pub use asset::{
    AnimationAssetError, AnimationMarker, ValidatedAnimationAsset, parse_animation_asset_v1,
    serialize_animation_asset_v1,
};
pub use blend::{BlendError, blend_local_poses};
pub use clip::{
    AnimationClip, BoneTrack, Channel, ClipError, DEPTH_ANGLE_LIMIT, DEPTH_PROJECTION_MIN,
    Interpolation, Keyframe, LoopPolicy,
};
pub use depth::{
    DepthPose, apply_depth_projection, blend_depth_poses, depth_projection, sample_and_project,
    sample_depth,
};
pub use dev::{
    A1_HEAD_CLIP_DURATION, A3_IDLE_CLIP_DURATION, A3_MOVE_CLIP_DURATION, A4_FALL_CLIP_DURATION,
    A4_JUMP_CLIP_DURATION, A4_TRANSITION_DURATION, A5_ATTACK_CLIP_DURATION, A5_HURT_CLIP_DURATION,
    a1_head_loop_clip, a1_head_rotation_clip, a3_idle_clip, a3_move_clip, a4_fall_clip,
    a4_jump_clip, a5_attack_clip, a5_hurt_clip, climb_back_clip,
};
pub use player::{AnimationPlayer, PlayerError};
pub use sample::{SampleError, sample};

#[cfg(test)]
mod tests;
