//! Frozen Humanoid v0 topology (16 bones) as test/dev definition data.
//!
//! Index constants are labels for this definition, not a closed `BoneId` enum for all rigs.
//! Bind translations are readable temporary presentation units. They are **not** a
//! gameplay AABB or collision contract and remain tunable before art integration.

use std::sync::OnceLock;

use crate::def::{SkeletonDef, SlotDef};
use crate::xform::{BoneIndex, BoneTransform, SlotIndex};

pub const BONE_COUNT: u8 = 16;
pub const SLOT_COUNT: u8 = 17;

pub const ROOT: BoneIndex = BoneIndex::from_u8(0);
pub const PELVIS: BoneIndex = BoneIndex::from_u8(1);
pub const TORSO: BoneIndex = BoneIndex::from_u8(2);
pub const HEAD: BoneIndex = BoneIndex::from_u8(3);
pub const UPPER_ARM_FRONT: BoneIndex = BoneIndex::from_u8(4);
pub const LOWER_ARM_FRONT: BoneIndex = BoneIndex::from_u8(5);
pub const HAND_FRONT: BoneIndex = BoneIndex::from_u8(6);
pub const UPPER_ARM_BACK: BoneIndex = BoneIndex::from_u8(7);
pub const LOWER_ARM_BACK: BoneIndex = BoneIndex::from_u8(8);
pub const HAND_BACK: BoneIndex = BoneIndex::from_u8(9);
pub const UPPER_LEG_FRONT: BoneIndex = BoneIndex::from_u8(10);
pub const LOWER_LEG_FRONT: BoneIndex = BoneIndex::from_u8(11);
pub const FOOT_FRONT: BoneIndex = BoneIndex::from_u8(12);
pub const UPPER_LEG_BACK: BoneIndex = BoneIndex::from_u8(13);
pub const LOWER_LEG_BACK: BoneIndex = BoneIndex::from_u8(14);
pub const FOOT_BACK: BoneIndex = BoneIndex::from_u8(15);

pub const SLOT_HEAD: SlotIndex = SlotIndex::from_u8(0);
pub const SLOT_HAIR_BACK: SlotIndex = SlotIndex::from_u8(1);
pub const SLOT_HAIR_FRONT: SlotIndex = SlotIndex::from_u8(2);
pub const SLOT_TORSO: SlotIndex = SlotIndex::from_u8(3);
pub const SLOT_UPPER_ARM_BACK: SlotIndex = SlotIndex::from_u8(4);
pub const SLOT_LOWER_ARM_BACK: SlotIndex = SlotIndex::from_u8(5);
pub const SLOT_HAND_BACK: SlotIndex = SlotIndex::from_u8(6);
pub const SLOT_UPPER_ARM_FRONT: SlotIndex = SlotIndex::from_u8(7);
pub const SLOT_LOWER_ARM_FRONT: SlotIndex = SlotIndex::from_u8(8);
pub const SLOT_HAND_FRONT: SlotIndex = SlotIndex::from_u8(9);
pub const SLOT_UPPER_LEG_BACK: SlotIndex = SlotIndex::from_u8(10);
pub const SLOT_LOWER_LEG_BACK: SlotIndex = SlotIndex::from_u8(11);
pub const SLOT_SHOE_BACK: SlotIndex = SlotIndex::from_u8(12);
pub const SLOT_UPPER_LEG_FRONT: SlotIndex = SlotIndex::from_u8(13);
pub const SLOT_LOWER_LEG_FRONT: SlotIndex = SlotIndex::from_u8(14);
pub const SLOT_SHOE_FRONT: SlotIndex = SlotIndex::from_u8(15);
pub const SLOT_WEAPON: SlotIndex = SlotIndex::from_u8(16);

const fn tr(x: f32, y: f32) -> BoneTransform {
    BoneTransform::from_translation_rotation([x, y], 0.0)
}

/// Temporary bind locals (presentation units). Tests lock these values; they may change
/// before art integration without changing bone topology.
pub const BIND_ROOT: BoneTransform = tr(0.0, 0.0);
pub const BIND_PELVIS: BoneTransform = tr(0.0, 0.42);
pub const BIND_TORSO: BoneTransform = tr(0.0, 0.30);
/// P4.2: +X lead so the head reads RIGHT-facing 3/4, not camera-front.
pub const BIND_HEAD: BoneTransform = tr(0.07, 0.24);
/// Near arm chain (Front). Bind X is the former Back shoulder X; Front identity is unchanged.
pub const BIND_UPPER_ARM_FRONT: BoneTransform = tr(-0.10, 0.14);
pub const BIND_LOWER_ARM_FRONT: BoneTransform = tr(0.0, -0.22);
pub const BIND_HAND_FRONT: BoneTransform = tr(0.0, -0.10);
/// Far arm chain (Back). Bind X is the former Front shoulder X; Back identity is unchanged.
pub const BIND_UPPER_ARM_BACK: BoneTransform = tr(0.08, 0.14);
/// Mild +CCW elbow bend (~29°) so the far forearm peeks toward +X (RIGHT). Length unchanged.
pub const BIND_LOWER_ARM_BACK: BoneTransform =
    BoneTransform::from_translation_rotation([0.0, -0.22], 0.50);
pub const BIND_HAND_BACK: BoneTransform = tr(0.0, -0.10);
/// Near hip. X only: farther toward −X than the far hip. Length/rotation unchanged.
pub const BIND_UPPER_LEG_FRONT: BoneTransform = tr(-0.06, -0.04);
/// Hip→knee thigh span. `upper_leg_front` world origin is the hip.
pub const BIND_LOWER_LEG_FRONT: BoneTransform = tr(0.0, -0.20);
/// Knee→ankle shin span (vertical in bind). Forward foot length is a visual, not this T.
pub const BIND_FOOT_FRONT: BoneTransform = tr(0.0, -0.18);
/// Far hip. Inward vs prior +0.09 so the chain stays inside the pelvis mass. Rotation 0.
pub const BIND_UPPER_LEG_BACK: BoneTransform = tr(0.04, -0.04);
pub const BIND_LOWER_LEG_BACK: BoneTransform = tr(0.0, -0.20);
pub const BIND_FOOT_BACK: BoneTransform = tr(0.0, -0.18);

/// Shoe rest: slight forward offset + small rotation so slot TR compose is observable.
pub const SLOT_REST_SHOE_FRONT: BoneTransform =
    BoneTransform::from_translation_rotation([0.06, 0.0], 0.15);
pub const SLOT_REST_WEAPON: BoneTransform =
    BoneTransform::from_translation_rotation([0.08, 0.0], -0.2);

/// Presentation-anchor locals derived from Humanoid v0 bind/slot rest.
/// Content `AnchorPoint` stays in `purgatory-content`; these are rig constants only.
///
/// - Crown: one authored head-bind Y above the head joint (head has no child).
/// - Chest: midpoint of the authored torso→head span (`BIND_HEAD`).
/// - Grip: existing weapon-slot rest, used for both front and back hands.
/// - Foot: existing shoe-front rest, used for both feet.
pub const ANCHOR_CROWN: BoneTransform = tr(0.0, BIND_HEAD.translation[1]);
pub const ANCHOR_CHEST: BoneTransform = tr(
    BIND_HEAD.translation[0] * 0.5,
    BIND_HEAD.translation[1] * 0.5,
);
pub const ANCHOR_GRIP: BoneTransform = SLOT_REST_WEAPON;
pub const ANCHOR_FOOT: BoneTransform = SLOT_REST_SHOE_FRONT;

fn identity_slot(bone: BoneIndex) -> SlotDef {
    SlotDef {
        bone,
        rest: BoneTransform::IDENTITY,
    }
}

fn build() -> SkeletonDef {
    let p = |parent: BoneIndex| Some(parent);
    let parents = vec![
        None,
        p(ROOT),
        p(PELVIS),
        p(TORSO),
        p(TORSO),
        p(UPPER_ARM_FRONT),
        p(LOWER_ARM_FRONT),
        p(TORSO),
        p(UPPER_ARM_BACK),
        p(LOWER_ARM_BACK),
        p(PELVIS),
        p(UPPER_LEG_FRONT),
        p(LOWER_LEG_FRONT),
        p(PELVIS),
        p(UPPER_LEG_BACK),
        p(LOWER_LEG_BACK),
    ];
    let bind_locals = vec![
        BIND_ROOT,
        BIND_PELVIS,
        BIND_TORSO,
        BIND_HEAD,
        BIND_UPPER_ARM_FRONT,
        BIND_LOWER_ARM_FRONT,
        BIND_HAND_FRONT,
        BIND_UPPER_ARM_BACK,
        BIND_LOWER_ARM_BACK,
        BIND_HAND_BACK,
        BIND_UPPER_LEG_FRONT,
        BIND_LOWER_LEG_FRONT,
        BIND_FOOT_FRONT,
        BIND_UPPER_LEG_BACK,
        BIND_LOWER_LEG_BACK,
        BIND_FOOT_BACK,
    ];
    let slots = vec![
        identity_slot(HEAD),
        identity_slot(HEAD),
        identity_slot(HEAD),
        identity_slot(TORSO),
        identity_slot(UPPER_ARM_BACK),
        identity_slot(LOWER_ARM_BACK),
        identity_slot(HAND_BACK),
        identity_slot(UPPER_ARM_FRONT),
        identity_slot(LOWER_ARM_FRONT),
        identity_slot(HAND_FRONT),
        identity_slot(UPPER_LEG_BACK),
        identity_slot(LOWER_LEG_BACK),
        identity_slot(FOOT_BACK),
        identity_slot(UPPER_LEG_FRONT),
        identity_slot(LOWER_LEG_FRONT),
        SlotDef {
            bone: FOOT_FRONT,
            rest: SLOT_REST_SHOE_FRONT,
        },
        SlotDef {
            bone: HAND_FRONT,
            rest: SLOT_REST_WEAPON,
        },
    ];
    let draw_order = vec![
        SLOT_HAIR_BACK,
        SLOT_UPPER_ARM_BACK,
        SLOT_LOWER_ARM_BACK,
        SLOT_HAND_BACK,
        SLOT_UPPER_LEG_BACK,
        SLOT_LOWER_LEG_BACK,
        SLOT_SHOE_BACK,
        SLOT_TORSO,
        SLOT_UPPER_LEG_FRONT,
        SLOT_LOWER_LEG_FRONT,
        SLOT_SHOE_FRONT,
        SLOT_HEAD,
        SLOT_UPPER_ARM_FRONT,
        SLOT_LOWER_ARM_FRONT,
        SLOT_HAND_FRONT,
        SLOT_WEAPON,
        SLOT_HAIR_FRONT,
    ];
    SkeletonDef::try_new(parents, bind_locals, slots, draw_order)
        .expect("Humanoid v0 topology and slot table are a valid definition")
}

/// Shared Humanoid v0 definition. Callers must not clone this per pose instance.
#[must_use]
pub fn humanoid_v0() -> &'static SkeletonDef {
    static DEF: OnceLock<SkeletonDef> = OnceLock::new();
    DEF.get_or_init(build)
}

/// Authoring labels aligned with Humanoid v0 dense slot indices. Not a closed slot enum.
pub const HUMANOID_V0_SLOT_LABELS: [&str; SLOT_COUNT as usize] = [
    "head",
    "hair_back",
    "hair_front",
    "torso",
    "upper_arm_back",
    "lower_arm_back",
    "hand_back",
    "upper_arm_front",
    "lower_arm_front",
    "hand_front",
    "upper_leg_back",
    "lower_leg_back",
    "shoe_back",
    "upper_leg_front",
    "lower_leg_front",
    "shoe_front",
    "weapon",
];

/// Authoring labels aligned with Humanoid v0 dense indices. Not a closed bone enum.
pub const HUMANOID_V0_BONE_LABELS: [&str; BONE_COUNT as usize] = [
    "root",
    "pelvis",
    "torso",
    "head",
    "upper_arm_front",
    "lower_arm_front",
    "hand_front",
    "upper_arm_back",
    "lower_arm_back",
    "hand_back",
    "upper_leg_front",
    "lower_leg_front",
    "foot_front",
    "upper_leg_back",
    "lower_leg_back",
    "foot_back",
];

/// Bind-time lookup. Do not call from evaluate.
#[must_use]
pub fn humanoid_v0_bone_by_label(label: &str) -> Option<BoneIndex> {
    HUMANOID_V0_BONE_LABELS
        .iter()
        .position(|&name| name == label)
        .map(|i| BoneIndex::from_u8(i as u8))
}
