//! Humanoid v0 presentation-anchor locals.
//!
//! `AnchorPoint` is content vocabulary. Locals come from skeleton rig constants
//! derived from bind/slot rest — not ad-hoc numbers in compose/draw.

use purgatory_content::AnchorPoint;
use purgatory_skeleton::{ANCHOR_CHEST, ANCHOR_CROWN, ANCHOR_FOOT, ANCHOR_GRIP, BoneTransform};

/// Local transform of an authored [`AnchorPoint`] in the parent bone's space.
#[must_use]
pub fn anchor_local(anchor: AnchorPoint) -> BoneTransform {
    match anchor {
        AnchorPoint::BoneOrigin => BoneTransform::IDENTITY,
        AnchorPoint::Crown => ANCHOR_CROWN,
        AnchorPoint::Chest => ANCHOR_CHEST,
        AnchorPoint::GripFront | AnchorPoint::GripBack => ANCHOR_GRIP,
        AnchorPoint::FootFront | AnchorPoint::FootBack => ANCHOR_FOOT,
    }
}
