//! Bone world ∘ anchor local ∘ correction. No content parse.

use purgatory_content::CorrectionOffset;
use purgatory_skeleton::{BoneTransform, WorldPose};

use super::anchors::anchor_local;
use super::resolve::BoundAttachment;

/// 8B authoring canvas: 128×128 px → 1 presentation unit.
pub const AUTHORING_CANVAS_PX: f32 = 128.0;

#[must_use]
pub fn correction_local(correction: CorrectionOffset) -> BoneTransform {
    BoneTransform::from_translation_rotation(
        [
            correction.x / AUTHORING_CANVAS_PX,
            correction.y / AUTHORING_CANVAS_PX,
        ],
        correction.rotation.to_radians(),
    )
}

/// `world(bone).compose(anchor_local).compose(correction_local)`.
#[must_use]
pub fn compose_attachment(bound: &BoundAttachment, world: &WorldPose) -> Option<BoneTransform> {
    let bone = world.get(bound.bone)?;
    Some(
        bone.compose(anchor_local(bound.anchor))
            .compose(bound.correction),
    )
}
