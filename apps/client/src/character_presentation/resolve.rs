//! ContentId → BoundAttachment. Called on equipment change only.

use purgatory_common::ContentId;
use purgatory_content::{ContentRegistry, CoverageMode, PresentationAttachment};
use purgatory_simulation::EquipmentSlot;
use purgatory_skeleton::{BoneIndex, BoneTransform};

use super::bone_map::{BoneTargetMap, hide_mask};
use super::compose::correction_local;
use super::state::{EquipmentView, PresentationView};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissingPresentationReason {
    UnknownContent,
    MissingPresentation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MissingPresentation {
    pub slot: EquipmentSlot,
    pub content_id: ContentId,
    pub reason: MissingPresentationReason,
}

/// Resolved once per equipment change. Ready for per-frame compose.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundAttachment {
    pub slot: EquipmentSlot,
    pub content_id: ContentId,
    pub attachment_id: String,
    pub visual_key_side: String,
    pub visual_key_back: Option<String>,
    pub bone: BoneIndex,
    pub bone_target: purgatory_content::BoneTarget,
    pub anchor: purgatory_content::AnchorPoint,
    pub coverage: CoverageMode,
    pub hide_base: u16,
    pub correction: BoneTransform,
}

impl BoundAttachment {
    /// Select the authored visual key for `view`. Side never substitutes for Back.
    #[must_use]
    pub fn visual_key_for_view(&self, view: PresentationView) -> Option<&str> {
        match view {
            super::state::PresentationView::Side => Some(self.visual_key_side.as_str()),
            super::state::PresentationView::Back => self.visual_key_back.as_deref(),
        }
    }
}

pub struct ResolveOutput {
    pub bound: Vec<BoundAttachment>,
    pub missing: Vec<MissingPresentation>,
    pub hidden_base: u16,
}

impl ResolveOutput {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            bound: Vec::new(),
            missing: Vec::new(),
            hidden_base: 0,
        }
    }
}

/// Resolve equipped ContentIds. Missing slots are skipped (no panic, no substitute).
#[must_use]
pub fn resolve_equipment(
    equipment: EquipmentView,
    registry: &ContentRegistry,
    bone_map: BoneTargetMap,
) -> ResolveOutput {
    let EquipmentView::Present(slots) = equipment else {
        return ResolveOutput::empty();
    };
    let mut bound = Vec::new();
    let mut missing = Vec::new();
    for slot in EquipmentSlot::ALL {
        let Some(content_id) = slots[slot.index()] else {
            continue;
        };
        let Some(pres) = registry.equipment_presentation_by_id(content_id) else {
            let reason = if registry.equipment_by_id(content_id).is_some() {
                MissingPresentationReason::MissingPresentation
            } else {
                MissingPresentationReason::UnknownContent
            };
            missing.push(MissingPresentation {
                slot,
                content_id,
                reason,
            });
            continue;
        };
        for att in &pres.attachments {
            bound.push(bind_attachment(slot, content_id, att, bone_map));
        }
    }
    let hidden_base = union_hidden_base(&bound);
    ResolveOutput {
        bound,
        missing,
        hidden_base,
    }
}

#[must_use]
pub fn union_hidden_base(bound: &[BoundAttachment]) -> u16 {
    bound
        .iter()
        .filter(|att| att.coverage == CoverageMode::ReplaceBase)
        .fold(0, |acc, att| acc | att.hide_base)
}

fn bind_attachment(
    slot: EquipmentSlot,
    content_id: ContentId,
    att: &PresentationAttachment,
    bone_map: BoneTargetMap,
) -> BoundAttachment {
    let hide_base = if att.coverage == CoverageMode::ReplaceBase {
        hide_mask(att.hide_base.iter().copied())
    } else {
        0
    };
    BoundAttachment {
        slot,
        content_id,
        attachment_id: att.id.clone(),
        visual_key_side: att.visuals.side.clone(),
        visual_key_back: att.visuals.back.clone(),
        bone: bone_map.bone(att.bone),
        bone_target: att.bone,
        anchor: att.anchor,
        coverage: att.coverage,
        hide_base,
        correction: correction_local(att.correction),
    }
}
