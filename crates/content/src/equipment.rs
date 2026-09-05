//! Equipment Content Schema v1: gameplay slot identity + client presentation.
//!
//! Not protocol. Not skeleton internals. Not ART. Authoritative runtime state
//! remains `EquipmentSlot → Option<ContentId>` (Phase 8A).

use std::collections::HashSet;
use std::fmt;

use crate::domain::ContentDomain;
use crate::error::{ContentError, ValidationIssue};
use purgatory_common::{ContentId, validate_authored_id};
use purgatory_simulation::EquipmentSlot;

/// Equipment content schema v1. Independent of map/entity `CONTENT_SCHEMA_VERSION`.
pub const EQUIPMENT_CONTENT_SCHEMA_VERSION: u32 = 1;

/// Authoring-space correction bounds (128×128 reference canvas).
pub const CORRECTION_OFFSET_MAX_PX: f32 = 8.0;
pub const CORRECTION_ROTATION_MAX_DEG: f32 = 15.0;

const IMAGE_EXT_SEGMENTS: &[&str] = &["png", "jpg", "jpeg", "webp", "tga", "bmp", "gif"];

/// Humanoid v0 presentation bone targets. Names match skeleton labels; this
/// enum is content vocabulary and does not depend on `purgatory-skeleton`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum BoneTarget {
    Head,
    Torso,
    UpperArmFront,
    LowerArmFront,
    HandFront,
    UpperArmBack,
    LowerArmBack,
    HandBack,
    UpperLegFront,
    LowerLegFront,
    FootFront,
    UpperLegBack,
    LowerLegBack,
    FootBack,
}

impl BoneTarget {
    pub const ALL: [Self; 14] = [
        Self::Head,
        Self::Torso,
        Self::UpperArmFront,
        Self::LowerArmFront,
        Self::HandFront,
        Self::UpperArmBack,
        Self::LowerArmBack,
        Self::HandBack,
        Self::UpperLegFront,
        Self::LowerLegFront,
        Self::FootFront,
        Self::UpperLegBack,
        Self::LowerLegBack,
        Self::FootBack,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Head => "head",
            Self::Torso => "torso",
            Self::UpperArmFront => "upper_arm_front",
            Self::LowerArmFront => "lower_arm_front",
            Self::HandFront => "hand_front",
            Self::UpperArmBack => "upper_arm_back",
            Self::LowerArmBack => "lower_arm_back",
            Self::HandBack => "hand_back",
            Self::UpperLegFront => "upper_leg_front",
            Self::LowerLegFront => "lower_leg_front",
            Self::FootFront => "foot_front",
            Self::UpperLegBack => "upper_leg_back",
            Self::LowerLegBack => "lower_leg_back",
            Self::FootBack => "foot_back",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "head" => Some(Self::Head),
            "torso" => Some(Self::Torso),
            "upper_arm_front" => Some(Self::UpperArmFront),
            "lower_arm_front" => Some(Self::LowerArmFront),
            "hand_front" => Some(Self::HandFront),
            "upper_arm_back" => Some(Self::UpperArmBack),
            "lower_arm_back" => Some(Self::LowerArmBack),
            "hand_back" => Some(Self::HandBack),
            "upper_leg_front" => Some(Self::UpperLegFront),
            "lower_leg_front" => Some(Self::LowerLegFront),
            "foot_front" => Some(Self::FootFront),
            "upper_leg_back" => Some(Self::UpperLegBack),
            "lower_leg_back" => Some(Self::LowerLegBack),
            "foot_back" => Some(Self::FootBack),
            _ => None,
        }
    }
}

impl fmt::Display for BoneTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Fixed attachment points. Content cannot invent extra names.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AnchorPoint {
    BoneOrigin,
    Crown,
    Chest,
    GripFront,
    GripBack,
    FootFront,
    FootBack,
}

impl AnchorPoint {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BoneOrigin => "bone_origin",
            Self::Crown => "crown",
            Self::Chest => "chest",
            Self::GripFront => "grip_front",
            Self::GripBack => "grip_back",
            Self::FootFront => "foot_front",
            Self::FootBack => "foot_back",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "bone_origin" => Some(Self::BoneOrigin),
            "crown" => Some(Self::Crown),
            "chest" => Some(Self::Chest),
            "grip_front" => Some(Self::GripFront),
            "grip_back" => Some(Self::GripBack),
            "foot_front" => Some(Self::FootFront),
            "foot_back" => Some(Self::FootBack),
            _ => None,
        }
    }
}

impl fmt::Display for AnchorPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CoverageMode {
    Overlay,
    ReplaceBase,
}

impl CoverageMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Overlay => "overlay",
            Self::ReplaceBase => "replace_base",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "overlay" => Some(Self::Overlay),
            "replace_base" => Some(Self::ReplaceBase),
            _ => None,
        }
    }
}

impl fmt::Display for CoverageMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Authored view. Side is mandatory on every attachment. Back is optional.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ViewVariant {
    Side,
    Back,
}

impl ViewVariant {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Side => "side",
            Self::Back => "back",
        }
    }
}

/// Local px/degree correction. No scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CorrectionOffset {
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
}

impl Default for CorrectionOffset {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
        }
    }
}

impl CorrectionOffset {
    #[must_use]
    pub fn is_zero(self) -> bool {
        self.x == 0.0 && self.y == 0.0 && self.rotation == 0.0
    }
}

/// Logical visual key per view. Not a filesystem path or GPU handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewVisuals {
    pub side: String,
    pub back: Option<String>,
}

impl ViewVisuals {
    #[must_use]
    pub fn completeness(&self) -> PresentationCompleteness {
        PresentationCompleteness {
            has_side: !self.side.is_empty(),
            has_back: self.back.is_some(),
        }
    }

    /// Logical key for an authored view. Back is omitted when not authored;
    /// callers must not substitute Side.
    #[must_use]
    pub fn key_for(&self, view: ViewVariant) -> Option<&str> {
        match view {
            ViewVariant::Side => {
                if self.side.is_empty() {
                    None
                } else {
                    Some(self.side.as_str())
                }
            }
            ViewVariant::Back => self.back.as_deref(),
        }
    }
}

/// 8F hook surface: which authored views exist. Back-required-by-animation is not 8B.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationCompleteness {
    pub has_side: bool,
    pub has_back: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresentationAttachment {
    pub id: String,
    pub bone: BoneTarget,
    pub anchor: AnchorPoint,
    pub coverage: CoverageMode,
    pub hide_base: Vec<BoneTarget>,
    pub correction: CorrectionOffset,
    pub visuals: ViewVisuals,
}

impl PresentationAttachment {
    #[must_use]
    pub fn completeness(&self) -> PresentationCompleteness {
        self.visuals.completeness()
    }
}

/// Gameplay equipment identity. Slot only; no presentation, stats, or inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EquipmentDefinition {
    pub content_id: ContentId,
    pub authored_id: String,
    pub slot: EquipmentSlot,
    pub domain: ContentDomain,
}

/// Client presentation for the same [`ContentId`].
#[derive(Clone, Debug, PartialEq)]
pub struct EquipmentPresentation {
    pub content_id: ContentId,
    pub authored_id: String,
    pub attachments: Vec<PresentationAttachment>,
}

#[must_use]
pub fn slot_allows_bone(slot: EquipmentSlot, bone: BoneTarget) -> bool {
    match slot {
        EquipmentSlot::Headwear => bone == BoneTarget::Head,
        EquipmentSlot::Bodywear => matches!(
            bone,
            BoneTarget::Torso
                | BoneTarget::UpperArmFront
                | BoneTarget::UpperArmBack
                | BoneTarget::LowerArmFront
                | BoneTarget::LowerArmBack
        ),
        EquipmentSlot::Pants => matches!(
            bone,
            BoneTarget::UpperLegFront
                | BoneTarget::LowerLegFront
                | BoneTarget::UpperLegBack
                | BoneTarget::LowerLegBack
        ),
        EquipmentSlot::Gloves => matches!(bone, BoneTarget::HandFront | BoneTarget::HandBack),
        EquipmentSlot::Boots => matches!(bone, BoneTarget::FootFront | BoneTarget::FootBack),
        EquipmentSlot::Weapon => matches!(bone, BoneTarget::HandFront | BoneTarget::HandBack),
    }
}

#[must_use]
pub fn bone_allows_anchor(bone: BoneTarget, anchor: AnchorPoint) -> bool {
    match anchor {
        AnchorPoint::BoneOrigin => true,
        AnchorPoint::Crown => bone == BoneTarget::Head,
        AnchorPoint::Chest => bone == BoneTarget::Torso,
        AnchorPoint::GripFront => bone == BoneTarget::HandFront,
        AnchorPoint::GripBack => bone == BoneTarget::HandBack,
        AnchorPoint::FootFront => bone == BoneTarget::FootFront,
        AnchorPoint::FootBack => bone == BoneTarget::FootBack,
    }
}

/// Weapon must use a grip; grips are weapon-only.
#[must_use]
pub fn slot_allows_anchor(slot: EquipmentSlot, bone: BoneTarget, anchor: AnchorPoint) -> bool {
    if !bone_allows_anchor(bone, anchor) {
        return false;
    }
    match slot {
        EquipmentSlot::Weapon => matches!(anchor, AnchorPoint::GripFront | AnchorPoint::GripBack),
        _ => !matches!(anchor, AnchorPoint::GripFront | AnchorPoint::GripBack),
    }
}

pub fn validate_equipment_definition(def: &EquipmentDefinition) -> Result<(), ContentError> {
    let mut issues = Vec::new();
    if let Err(err) = validate_authored_id(&def.authored_id) {
        issues.push(equip_issue(
            &def.authored_id,
            Some(def.slot),
            None,
            "id",
            "schema",
            format!("invalid ContentId ({err:?})"),
        ));
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContentError { issues })
    }
}

pub fn validate_equipment_presentation(
    presentation: &EquipmentPresentation,
    slot: EquipmentSlot,
) -> Result<(), ContentError> {
    let mut issues = Vec::new();
    if let Err(err) = validate_authored_id(&presentation.authored_id) {
        issues.push(equip_issue(
            &presentation.authored_id,
            Some(slot),
            None,
            "id",
            "schema",
            format!("invalid ContentId ({err:?})"),
        ));
    }
    let mut seen_ids = HashSet::new();
    for (i, att) in presentation.attachments.iter().enumerate() {
        validate_attachment(
            &mut issues,
            &presentation.authored_id,
            slot,
            i,
            att,
            &mut seen_ids,
        );
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContentError { issues })
    }
}

/// Gameplay-only equip authorization. Does not load or inspect presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EquipmentAuthError {
    UnknownContent,
    SlotMismatch,
}

pub fn authorize_equip(
    registry: &crate::registry::ContentRegistry,
    slot: EquipmentSlot,
    content_id: ContentId,
) -> Result<(), EquipmentAuthError> {
    let Some(def) = registry.equipment_by_id(content_id) else {
        return Err(EquipmentAuthError::UnknownContent);
    };
    if def.slot != slot {
        return Err(EquipmentAuthError::SlotMismatch);
    }
    Ok(())
}

fn validate_attachment(
    issues: &mut Vec<ValidationIssue>,
    def: &str,
    slot: EquipmentSlot,
    index: usize,
    att: &PresentationAttachment,
    seen_ids: &mut HashSet<String>,
) {
    let att_label = if att.id.is_empty() {
        format!("{index}")
    } else {
        att.id.clone()
    };
    if let Err(reason) = validate_attachment_id(&att.id) {
        issues.push(equip_issue(
            def,
            Some(slot),
            Some(&att_label),
            "id",
            "schema",
            reason,
        ));
    } else if !seen_ids.insert(att.id.clone()) {
        issues.push(equip_issue(
            def,
            Some(slot),
            Some(&att.id),
            "id",
            "schema",
            "duplicate attachment id",
        ));
    }

    if !slot_allows_bone(slot, att.bone) {
        issues.push(equip_issue(
            def,
            Some(slot),
            Some(&att_label),
            "bone",
            "slot_target",
            format!(
                "BoneTarget {} is not allowed on slot {}",
                att.bone,
                slot.as_str()
            ),
        ));
    }
    if !bone_allows_anchor(att.bone, att.anchor) {
        issues.push(equip_issue(
            def,
            Some(slot),
            Some(&att_label),
            "anchor",
            "bone_anchor",
            format!(
                "AnchorPoint {} is not compatible with BoneTarget {}",
                att.anchor, att.bone
            ),
        ));
    } else if !slot_allows_anchor(slot, att.bone, att.anchor) {
        issues.push(equip_issue(
            def,
            Some(slot),
            Some(&att_label),
            "anchor",
            "slot_anchor",
            format!(
                "AnchorPoint {} is not allowed on slot {} with BoneTarget {}",
                att.anchor,
                slot.as_str(),
                att.bone
            ),
        ));
    }

    match att.coverage {
        CoverageMode::Overlay => {
            if !att.hide_base.is_empty() {
                issues.push(equip_issue(
                    def,
                    Some(slot),
                    Some(&att_label),
                    "hide_base",
                    "coverage",
                    "Overlay requires hide_base to be empty",
                ));
            }
        }
        CoverageMode::ReplaceBase => {
            let mut hidden = HashSet::new();
            for (h, bone) in att.hide_base.iter().copied().enumerate() {
                if !hidden.insert(bone) {
                    issues.push(equip_issue(
                        def,
                        Some(slot),
                        Some(&att_label),
                        &format!("hide_base[{h}]"),
                        "hide_base",
                        format!("duplicate base visual {bone}"),
                    ));
                }
            }
        }
    }

    push_correction_issues(issues, def, slot, &att_label, att.correction);

    if let Err(reason) = validate_visual_key(&att.visuals.side) {
        issues.push(equip_issue(
            def,
            Some(slot),
            Some(&att_label),
            "visuals.side",
            "schema",
            reason,
        ));
    }
    if let Some(back) = &att.visuals.back
        && let Err(reason) = validate_visual_key(back)
    {
        issues.push(equip_issue(
            def,
            Some(slot),
            Some(&att_label),
            "visuals.back",
            "schema",
            reason,
        ));
    }
    if att.visuals.side.is_empty() {
        issues.push(equip_issue(
            def,
            Some(slot),
            Some(&att_label),
            "visuals.side",
            "schema",
            "Side visual is mandatory",
        ));
    }
}

fn push_correction_issues(
    issues: &mut Vec<ValidationIssue>,
    def: &str,
    slot: EquipmentSlot,
    att: &str,
    c: CorrectionOffset,
) {
    for (field, value, max) in [
        ("correction.x", c.x, CORRECTION_OFFSET_MAX_PX),
        ("correction.y", c.y, CORRECTION_OFFSET_MAX_PX),
        (
            "correction.rotation",
            c.rotation,
            CORRECTION_ROTATION_MAX_DEG,
        ),
    ] {
        if !value.is_finite() || !(-max..=max).contains(&value) {
            issues.push(equip_issue(
                def,
                Some(slot),
                Some(att),
                field,
                "correction",
                format!("value {value} is outside [-{max}, {max}]"),
            ));
        }
    }
}

fn validate_attachment_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("attachment id is empty".into());
    }
    if id.len() > 32 {
        return Err("attachment id is too long".into());
    }
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return Err("attachment id is empty".into());
    };
    if !first.is_ascii_lowercase() {
        return Err("attachment id must start with a lowercase letter".into());
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err("attachment id must be [a-z][a-z0-9_]*".into());
    }
    Ok(())
}

fn validate_visual_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("visual key is empty".into());
    }
    if key.contains('/') || key.contains('\\') || key.contains(':') {
        return Err("visual key must not be a path or renderer handle".into());
    }
    validate_authored_id(key).map_err(|e| format!("invalid visual key ({e:?})"))?;
    for segment in key.split('.') {
        if IMAGE_EXT_SEGMENTS.contains(&segment) {
            return Err("visual key must not include an asset file extension".into());
        }
    }
    Ok(())
}

pub(crate) fn equip_issue(
    definition: &str,
    slot: Option<EquipmentSlot>,
    attachment: Option<&str>,
    field: &str,
    rule: &str,
    detail: impl fmt::Display,
) -> ValidationIssue {
    let field = match attachment {
        Some(id) => format!("attachments[{id}].{field}"),
        None => field.to_string(),
    };
    let reason = match slot {
        Some(slot) => format!("slot={} rule={rule}: {detail}", slot.as_str()),
        None => format!("rule={rule}: {detail}"),
    };
    ValidationIssue::new("equipment", definition, field, reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str, slot: EquipmentSlot) -> EquipmentDefinition {
        EquipmentDefinition {
            content_id: ContentId::from_authored(id).unwrap(),
            authored_id: id.into(),
            slot,
            domain: ContentDomain::Shared,
        }
    }

    fn overlay_att(id: &str, bone: BoneTarget, anchor: AnchorPoint) -> PresentationAttachment {
        PresentationAttachment {
            id: id.into(),
            bone,
            anchor,
            coverage: CoverageMode::Overlay,
            hide_base: Vec::new(),
            correction: CorrectionOffset::default(),
            visuals: ViewVisuals {
                side: "equipment.debug.visual.side".into(),
                back: None,
            },
        }
    }

    fn presentation(id: &str, attachments: Vec<PresentationAttachment>) -> EquipmentPresentation {
        EquipmentPresentation {
            content_id: ContentId::from_authored(id).unwrap(),
            authored_id: id.into(),
            attachments,
        }
    }

    #[test]
    fn valid_gameplay_definition() {
        let d = def("equipment.debug.cloth_cap", EquipmentSlot::Headwear);
        validate_equipment_definition(&d).unwrap();
        assert_eq!(d.slot, EquipmentSlot::Headwear);
    }

    #[test]
    fn valid_client_presentation() {
        let p = presentation(
            "equipment.debug.cloth_cap",
            vec![overlay_att("crown", BoneTarget::Head, AnchorPoint::Crown)],
        );
        validate_equipment_presentation(&p, EquipmentSlot::Headwear).unwrap();
    }

    #[test]
    fn zero_attachments_allowed() {
        let p = presentation("equipment.debug.unadorned", Vec::new());
        validate_equipment_presentation(&p, EquipmentSlot::Headwear).unwrap();
        assert!(p.attachments.is_empty());
    }

    #[test]
    fn multiple_attachments_allowed() {
        let p = presentation(
            "equipment.debug.tunic",
            vec![
                overlay_att("torso", BoneTarget::Torso, AnchorPoint::Chest),
                overlay_att(
                    "sleeve_front",
                    BoneTarget::UpperArmFront,
                    AnchorPoint::BoneOrigin,
                ),
                overlay_att(
                    "sleeve_back",
                    BoneTarget::UpperArmBack,
                    AnchorPoint::BoneOrigin,
                ),
            ],
        );
        validate_equipment_presentation(&p, EquipmentSlot::Bodywear).unwrap();
        assert_eq!(p.attachments.len(), 3);
    }

    #[test]
    fn attachment_ids_must_be_unique() {
        let p = presentation(
            "equipment.debug.tunic",
            vec![
                overlay_att("torso", BoneTarget::Torso, AnchorPoint::Chest),
                overlay_att("torso", BoneTarget::UpperArmFront, AnchorPoint::BoneOrigin),
            ],
        );
        let err = validate_equipment_presentation(&p, EquipmentSlot::Bodywear).unwrap_err();
        assert!(err.to_string().contains("duplicate attachment id"));
    }

    #[test]
    fn overlay_valid_with_empty_hide_base() {
        let p = presentation(
            "equipment.debug.cloth_cap",
            vec![overlay_att("crown", BoneTarget::Head, AnchorPoint::Crown)],
        );
        validate_equipment_presentation(&p, EquipmentSlot::Headwear).unwrap();
    }

    #[test]
    fn overlay_rejects_hide_base() {
        let mut att = overlay_att("crown", BoneTarget::Head, AnchorPoint::Crown);
        att.hide_base = vec![BoneTarget::Head];
        let p = presentation("equipment.debug.cloth_cap", vec![att]);
        let err = validate_equipment_presentation(&p, EquipmentSlot::Headwear).unwrap_err();
        assert!(err.to_string().contains("Overlay requires hide_base"));
    }

    #[test]
    fn replace_base_accepts_valid_base_hides() {
        let att = PresentationAttachment {
            id: "shell".into(),
            bone: BoneTarget::Torso,
            anchor: AnchorPoint::Chest,
            coverage: CoverageMode::ReplaceBase,
            hide_base: vec![
                BoneTarget::Torso,
                BoneTarget::UpperArmFront,
                BoneTarget::UpperArmBack,
            ],
            correction: CorrectionOffset::default(),
            visuals: ViewVisuals {
                side: "equipment.debug.plate_cuirass.side".into(),
                back: Some("equipment.debug.plate_cuirass.back".into()),
            },
        };
        let p = presentation("equipment.debug.plate_cuirass", vec![att]);
        validate_equipment_presentation(&p, EquipmentSlot::Bodywear).unwrap();
    }

    #[test]
    fn replace_base_rejects_duplicate_hide() {
        let att = PresentationAttachment {
            id: "shell".into(),
            bone: BoneTarget::Torso,
            anchor: AnchorPoint::Chest,
            coverage: CoverageMode::ReplaceBase,
            hide_base: vec![BoneTarget::Torso, BoneTarget::Torso],
            correction: CorrectionOffset::default(),
            visuals: ViewVisuals {
                side: "equipment.debug.plate_cuirass.side".into(),
                back: None,
            },
        };
        let err = validate_equipment_presentation(
            &presentation("equipment.debug.plate_cuirass", vec![att]),
            EquipmentSlot::Bodywear,
        )
        .unwrap_err();
        assert!(err.to_string().contains("duplicate base visual"));
    }

    #[test]
    fn valid_bone_anchor_pair() {
        assert!(bone_allows_anchor(
            BoneTarget::HandFront,
            AnchorPoint::GripFront
        ));
        assert!(slot_allows_anchor(
            EquipmentSlot::Weapon,
            BoneTarget::HandFront,
            AnchorPoint::GripFront
        ));
    }

    #[test]
    fn invalid_bone_anchor_pair() {
        assert!(!bone_allows_anchor(
            BoneTarget::FootFront,
            AnchorPoint::GripFront
        ));
        let mut att = overlay_att("blade", BoneTarget::HandFront, AnchorPoint::GripFront);
        att.bone = BoneTarget::FootFront;
        let err = validate_equipment_presentation(
            &presentation("equipment.debug.practice_sword", vec![att]),
            EquipmentSlot::Weapon,
        )
        .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("bone_anchor") || text.contains("slot_target"));
    }

    #[test]
    fn slot_target_compatibility() {
        assert!(slot_allows_bone(EquipmentSlot::Headwear, BoneTarget::Head));
        assert!(!slot_allows_bone(
            EquipmentSlot::Headwear,
            BoneTarget::Torso
        ));
        assert!(slot_allows_bone(
            EquipmentSlot::Gloves,
            BoneTarget::HandBack
        ));
        assert!(!slot_allows_bone(
            EquipmentSlot::Weapon,
            BoneTarget::FootFront
        ));
        let att = overlay_att("crown", BoneTarget::Torso, AnchorPoint::Chest);
        let err = validate_equipment_presentation(
            &presentation("equipment.debug.cloth_cap", vec![att]),
            EquipmentSlot::Headwear,
        )
        .unwrap_err();
        assert!(err.to_string().contains("slot_target"));
    }

    #[test]
    fn correction_bounds_rejected() {
        let mut att = overlay_att("crown", BoneTarget::Head, AnchorPoint::Crown);
        att.correction.x = 9.0;
        let err = validate_equipment_presentation(
            &presentation("equipment.debug.cloth_cap", vec![att]),
            EquipmentSlot::Headwear,
        )
        .unwrap_err();
        assert!(err.to_string().contains("correction"));
    }

    #[test]
    fn default_zero_correction() {
        let att = overlay_att("crown", BoneTarget::Head, AnchorPoint::Crown);
        assert!(att.correction.is_zero());
        validate_equipment_presentation(
            &presentation("equipment.debug.cloth_cap", vec![att]),
            EquipmentSlot::Headwear,
        )
        .unwrap();
    }

    #[test]
    fn side_only_accepted() {
        let att = overlay_att("grip", BoneTarget::HandFront, AnchorPoint::GripFront);
        assert!(att.completeness().has_side);
        assert!(!att.completeness().has_back);
        validate_equipment_presentation(
            &presentation("equipment.debug.practice_sword", vec![att]),
            EquipmentSlot::Weapon,
        )
        .unwrap();
    }

    #[test]
    fn side_and_back_accepted() {
        let mut att = overlay_att("crown", BoneTarget::Head, AnchorPoint::Crown);
        att.visuals.back = Some("equipment.debug.cloth_cap.back".into());
        assert!(att.completeness().has_back);
        validate_equipment_presentation(
            &presentation("equipment.debug.cloth_cap", vec![att]),
            EquipmentSlot::Headwear,
        )
        .unwrap();
    }

    #[test]
    fn missing_mandatory_side_rejected() {
        let mut att = overlay_att("crown", BoneTarget::Head, AnchorPoint::Crown);
        att.visuals.side.clear();
        let err = validate_equipment_presentation(
            &presentation("equipment.debug.cloth_cap", vec![att]),
            EquipmentSlot::Headwear,
        )
        .unwrap_err();
        assert!(err.to_string().contains("Side visual is mandatory"));
    }

    #[test]
    fn visual_key_rejects_path_and_extension() {
        assert!(validate_visual_key("assets/foo.png").is_err());
        assert!(validate_visual_key("equipment.debug.cap.png").is_err());
        assert!(validate_visual_key("equipment.debug.cloth_cap.side").is_ok());
    }

    #[test]
    fn no_direct_asset_or_renderer_type_required() {
        let att = overlay_att("crown", BoneTarget::Head, AnchorPoint::Crown);
        assert!(!att.visuals.side.contains('/'));
        assert!(!att.visuals.side.contains("Texture"));
        validate_equipment_presentation(
            &presentation("equipment.debug.cloth_cap", vec![att]),
            EquipmentSlot::Headwear,
        )
        .unwrap();
    }

    #[test]
    fn weapon_requires_grip_anchor() {
        let att = overlay_att("blade", BoneTarget::HandFront, AnchorPoint::BoneOrigin);
        let err = validate_equipment_presentation(
            &presentation("equipment.debug.practice_sword", vec![att]),
            EquipmentSlot::Weapon,
        )
        .unwrap_err();
        assert!(err.to_string().contains("slot_anchor"));
    }

    #[test]
    fn bounded_nonzero_correction_accepted() {
        let mut att = overlay_att("foot_front", BoneTarget::FootFront, AnchorPoint::FootFront);
        att.correction = CorrectionOffset {
            x: 2.0,
            y: -1.0,
            rotation: 5.0,
        };
        validate_equipment_presentation(
            &presentation("equipment.debug.iron_boots", vec![att]),
            EquipmentSlot::Boots,
        )
        .unwrap();
    }

    #[test]
    fn gloves_cannot_use_weapon_grip() {
        let att = overlay_att("hand_front", BoneTarget::HandFront, AnchorPoint::GripFront);
        let err = validate_equipment_presentation(
            &presentation("equipment.debug.leather_gloves", vec![att]),
            EquipmentSlot::Gloves,
        )
        .unwrap_err();
        assert!(err.to_string().contains("slot_anchor"));
    }

    #[test]
    fn presentation_completeness_does_not_require_back_by_slot() {
        let att = overlay_att("crown", BoneTarget::Head, AnchorPoint::Crown);
        assert!(att.completeness().has_side);
        assert!(!att.completeness().has_back);
        validate_equipment_presentation(
            &presentation("equipment.debug.cloth_cap", vec![att]),
            EquipmentSlot::Headwear,
        )
        .unwrap();
    }

    #[test]
    fn view_visuals_key_for_does_not_substitute_side_as_back() {
        let side_only = ViewVisuals {
            side: "equipment.debug.visual.side".into(),
            back: None,
        };
        assert_eq!(
            side_only.key_for(ViewVariant::Side),
            Some("equipment.debug.visual.side")
        );
        assert_eq!(side_only.key_for(ViewVariant::Back), None);
        let both = ViewVisuals {
            side: "equipment.debug.cloth_cap.side".into(),
            back: Some("equipment.debug.cloth_cap.back".into()),
        };
        assert_eq!(
            both.key_for(ViewVariant::Back),
            Some("equipment.debug.cloth_cap.back")
        );
        assert_ne!(
            both.key_for(ViewVariant::Side),
            both.key_for(ViewVariant::Back)
        );
    }

    #[test]
    fn authorize_equip_uses_gameplay_only() {
        let mut registry = crate::registry::ContentRegistry::new();
        let id = "equipment.debug.solo";
        let def = def(id, EquipmentSlot::Weapon);
        let cid = def.content_id;
        registry.insert_equipment(def).unwrap();
        assert!(authorize_equip(&registry, EquipmentSlot::Weapon, cid).is_ok());
        assert!(registry.equipment_presentation(id).is_none());
        assert!(registry.equipment_presentation_by_id(cid).is_none());
        assert_eq!(
            authorize_equip(&registry, EquipmentSlot::Headwear, cid),
            Err(EquipmentAuthError::SlotMismatch)
        );
        assert_eq!(
            authorize_equip(&registry, EquipmentSlot::Weapon, ContentId::from_token(999)),
            Err(EquipmentAuthError::UnknownContent)
        );
    }
}
