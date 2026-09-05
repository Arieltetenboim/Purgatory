//! Character Presentation draw-order + Front/Back visibility (Phase 8F-A/B).
//!
//! Skeleton owns bone structure and pose. This module owns what is drawn and
//! in which painter's-algorithm layer. Equipment content supplies `BoneTarget`
//! / slot / anchor only — never unrestricted z.
//!
//! [`PresentationView`] masks and remaps the same [`PresentationLayer`] set.
//! It does not introduce numeric z or a second order table.

use purgatory_content::BoneTarget;

use super::bone_map::hide_contains;
use super::resolve::BoundAttachment;
use super::state::PresentationView;

/// Far → near painter layers. Discriminant order is the contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum PresentationLayer {
    ArmBack = 0,
    LegBack = 1,
    Core = 2,
    LegFront = 3,
    Head = 4,
    ArmFront = 5,
}

impl PresentationLayer {
    pub const ALL: [Self; 6] = [
        Self::ArmBack,
        Self::LegBack,
        Self::Core,
        Self::LegFront,
        Self::Head,
        Self::ArmFront,
    ];

    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Whether this authored layer is drawn for `view`.
    ///
    /// Side: every layer. Back: hide the 3/4-view chest-side limbs
    /// (`ArmFront`, `LegFront`). Core and Head stay. Climb uses the same map.
    #[must_use]
    pub const fn authored_layer_visible(self, view: PresentationView) -> bool {
        match view {
            PresentationView::Side => true,
            PresentationView::Back => !matches!(
                self,
                PresentationLayer::ArmFront | PresentationLayer::LegFront
            ),
        }
    }

    /// Paint slot after Front/Back remapping. Canonical order is unchanged.
    ///
    /// Back: remaining back limbs occupy the near Front slots so they paint
    /// over Core/Head. Front→Back remap is unused while Front is hidden.
    #[must_use]
    pub const fn paint_layer(self, view: PresentationView) -> PresentationLayer {
        match view {
            PresentationView::Side => self,
            PresentationView::Back => match self {
                PresentationLayer::ArmBack => PresentationLayer::ArmFront,
                PresentationLayer::LegBack => PresentationLayer::LegFront,
                PresentationLayer::ArmFront => PresentationLayer::ArmBack,
                PresentationLayer::LegFront => PresentationLayer::LegBack,
                other => other,
            },
        }
    }
}

/// Base body pieces in canonical far → near order (P4).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BasePiece {
    UpperArmBack,
    LowerArmBack,
    HandBack,
    UpperLegBack,
    LowerLegBack,
    FootBack,
    TorsoFar,
    TorsoNear,
    UpperLegFront,
    LowerLegFront,
    FootFront,
    Head,
    UpperArmFront,
    LowerArmFront,
    HandFront,
}

impl BasePiece {
    pub const ALL: [Self; 15] = [
        Self::UpperArmBack,
        Self::LowerArmBack,
        Self::HandBack,
        Self::UpperLegBack,
        Self::LowerLegBack,
        Self::FootBack,
        Self::TorsoFar,
        Self::TorsoNear,
        Self::UpperLegFront,
        Self::LowerLegFront,
        Self::FootFront,
        Self::Head,
        Self::UpperArmFront,
        Self::LowerArmFront,
        Self::HandFront,
    ];

    #[must_use]
    pub const fn layer(self) -> PresentationLayer {
        layer_for_bone_target(self.hide_target())
    }

    #[must_use]
    pub const fn hide_target(self) -> BoneTarget {
        match self {
            Self::UpperArmBack => BoneTarget::UpperArmBack,
            Self::LowerArmBack => BoneTarget::LowerArmBack,
            Self::HandBack => BoneTarget::HandBack,
            Self::UpperLegBack => BoneTarget::UpperLegBack,
            Self::LowerLegBack => BoneTarget::LowerLegBack,
            Self::FootBack => BoneTarget::FootBack,
            Self::TorsoFar | Self::TorsoNear => BoneTarget::Torso,
            Self::UpperLegFront => BoneTarget::UpperLegFront,
            Self::LowerLegFront => BoneTarget::LowerLegFront,
            Self::FootFront => BoneTarget::FootFront,
            Self::Head => BoneTarget::Head,
            Self::UpperArmFront => BoneTarget::UpperArmFront,
            Self::LowerArmFront => BoneTarget::LowerArmFront,
            Self::HandFront => BoneTarget::HandFront,
        }
    }

    #[must_use]
    pub const fn intra(self) -> u8 {
        match self {
            Self::UpperArmBack
            | Self::UpperLegBack
            | Self::TorsoFar
            | Self::UpperLegFront
            | Self::UpperArmFront => 0,
            Self::LowerArmBack
            | Self::LowerLegBack
            | Self::TorsoNear
            | Self::LowerLegFront
            | Self::LowerArmFront => 1,
            Self::HandBack | Self::FootBack | Self::FootFront | Self::HandFront | Self::Head => 2,
        }
    }

    #[must_use]
    pub const fn torso_far(self) -> bool {
        matches!(self, Self::TorsoFar)
    }
}

/// Planned emit: base piece or an index into the caller's attachment slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlannedKind {
    Base(BasePiece),
    Attachment(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct DrawSortKey {
    layer: u8,
    role: u8,
    intra: u8,
    slot: u8,
    name_ord: u32,
}

/// Map a content bone target onto the presentation layer. Not a numeric z.
#[must_use]
pub const fn layer_for_bone_target(target: BoneTarget) -> PresentationLayer {
    match target {
        BoneTarget::UpperArmBack | BoneTarget::LowerArmBack | BoneTarget::HandBack => {
            PresentationLayer::ArmBack
        }
        BoneTarget::UpperLegBack | BoneTarget::LowerLegBack | BoneTarget::FootBack => {
            PresentationLayer::LegBack
        }
        BoneTarget::Torso => PresentationLayer::Core,
        BoneTarget::UpperLegFront | BoneTarget::LowerLegFront | BoneTarget::FootFront => {
            PresentationLayer::LegFront
        }
        BoneTarget::Head => PresentationLayer::Head,
        BoneTarget::UpperArmFront | BoneTarget::LowerArmFront | BoneTarget::HandFront => {
            PresentationLayer::ArmFront
        }
    }
}

/// Attachment layer follows its authored bone target (slot does not invent z).
#[must_use]
pub fn layer_for_attachment(bound: &BoundAttachment) -> PresentationLayer {
    layer_for_bone_target(bound.bone_target)
}

/// Deterministic far → near plan. Hidden base pieces are omitted; remaining
/// relative order is unchanged. Attachment slice order does not affect emit.
///
/// `view` filters authored layers and remaps paint slots. Attachments inherit
/// the visibility and paint slot of their `BoneTarget` layer. `hidden_base`
/// still drops only matching base pieces (not attachments). Attachments with
/// no visual key for `view` are omitted (no Side-as-Back).
#[must_use]
pub fn plan_character_draw(
    hidden_base: u16,
    attachments: &[BoundAttachment],
    view: PresentationView,
) -> Vec<PlannedKind> {
    let mut items: Vec<(DrawSortKey, PlannedKind)> =
        Vec::with_capacity(BasePiece::ALL.len() + attachments.len());
    for piece in BasePiece::ALL {
        let authored = piece.layer();
        if !authored.authored_layer_visible(view) {
            continue;
        }
        if hide_contains(hidden_base, piece.hide_target()) {
            continue;
        }
        items.push((
            DrawSortKey {
                layer: authored.paint_layer(view).as_u8(),
                role: 0,
                intra: piece.intra(),
                slot: 0,
                name_ord: 0,
            },
            PlannedKind::Base(piece),
        ));
    }
    for (i, bound) in attachments.iter().enumerate() {
        let authored = layer_for_attachment(bound);
        if !authored.authored_layer_visible(view) {
            continue;
        }
        if bound.visual_key_for_view(view).is_none() {
            continue;
        }
        items.push((
            attachment_sort_key(bound, authored.paint_layer(view)),
            PlannedKind::Attachment(i),
        ));
    }
    items.sort_by_key(|a| a.0);
    items.into_iter().map(|(_, kind)| kind).collect()
}

fn attachment_sort_key(bound: &BoundAttachment, paint: PresentationLayer) -> DrawSortKey {
    DrawSortKey {
        layer: paint.as_u8(),
        role: 1,
        intra: 0,
        slot: u8::try_from(bound.slot.index()).unwrap_or(u8::MAX),
        name_ord: fnv1a_u32(bound.attachment_id.as_bytes()),
    }
}

fn fnv1a_u32(bytes: &[u8]) -> u32 {
    let mut h = 0x811c_9dc5u32;
    for byte in bytes {
        h ^= u32::from(*byte);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}
