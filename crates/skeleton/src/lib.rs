//! Runtime-independent 2D skeleton math.
//!
//! Core contract: **Definition + local Pose → world Pose**.
//!
//! This crate has no knowledge of Player/NPC, velocity, animation clips, replicas,
//! networking, gameplay, wgpu, or egui.

mod def;
mod humanoid;
mod placeholder;
mod pose;
mod xform;

pub use def::{SkeletonDef, SkeletonDefError, SlotDef};
pub use humanoid::{
    ANCHOR_CHEST, ANCHOR_CROWN, ANCHOR_FOOT, ANCHOR_GRIP, BIND_FOOT_BACK, BIND_FOOT_FRONT,
    BIND_HAND_BACK, BIND_HAND_FRONT, BIND_HEAD, BIND_LOWER_ARM_BACK, BIND_LOWER_ARM_FRONT,
    BIND_LOWER_LEG_BACK, BIND_LOWER_LEG_FRONT, BIND_PELVIS, BIND_ROOT, BIND_TORSO,
    BIND_UPPER_ARM_BACK, BIND_UPPER_ARM_FRONT, BIND_UPPER_LEG_BACK, BIND_UPPER_LEG_FRONT,
    BONE_COUNT, FOOT_BACK, FOOT_FRONT, HAND_BACK, HAND_FRONT, HEAD, HUMANOID_V0_BONE_LABELS,
    HUMANOID_V0_SLOT_LABELS, LOWER_ARM_BACK, LOWER_ARM_FRONT, LOWER_LEG_BACK, LOWER_LEG_FRONT,
    PELVIS, ROOT, SLOT_COUNT, SLOT_HAIR_BACK, SLOT_HAIR_FRONT, SLOT_HAND_BACK, SLOT_HAND_FRONT,
    SLOT_HEAD, SLOT_LOWER_ARM_BACK, SLOT_LOWER_ARM_FRONT, SLOT_LOWER_LEG_BACK,
    SLOT_LOWER_LEG_FRONT, SLOT_REST_SHOE_FRONT, SLOT_REST_WEAPON, SLOT_SHOE_BACK, SLOT_SHOE_FRONT,
    SLOT_TORSO, SLOT_UPPER_ARM_BACK, SLOT_UPPER_ARM_FRONT, SLOT_UPPER_LEG_BACK,
    SLOT_UPPER_LEG_FRONT, SLOT_WEAPON, TORSO, UPPER_ARM_BACK, UPPER_ARM_FRONT, UPPER_LEG_BACK,
    UPPER_LEG_FRONT, humanoid_v0, humanoid_v0_bone_by_label,
};
pub use placeholder::{
    TORSO_FAR_BR_X, TORSO_FAR_TR_X, TORSO_LOCAL_BL_X, TORSO_LOCAL_BR_X, TORSO_LOCAL_TL_X,
    TORSO_LOCAL_TR_X, TorsoPlaceholderLayout, torso_far_local_corners, torso_local_corners,
    torso_placeholder_layout,
};
pub use pose::{LocalPose, PoseError, WorldPose, evaluate, slot_world};
pub use xform::{BoneIndex, BoneTransform, SlotIndex};

#[cfg(test)]
mod tests;
