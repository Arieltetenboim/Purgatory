//! Generic authored item definitions.

use crate::domain::ContentDomain;
use crate::error::{ContentError, ValidationIssue};
use purgatory_common::{
    ContentId, ContentKind, allocated_id_for_label, label_for_allocated_id, validate_authored_id,
};

/// Item gameplay content schema v3: numeric canonical `id` + metadata `label`.
pub const ITEM_CONTENT_SCHEMA_VERSION: u32 = 3;
/// Item presentation content schema v2: same numeric Item ID + metadata label.
pub const ITEM_PRESENTATION_SCHEMA_VERSION: u32 = 2;

/// Stable gameplay category used by inventory policy and client filtering.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ItemCategory {
    Equipment,
    Consumable,
    Material,
    Tool,
    Misc,
}

impl ItemCategory {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "equipment" => Some(Self::Equipment),
            "consumable" => Some(Self::Consumable),
            "material" => Some(Self::Material),
            "tool" => Some(Self::Tool),
            "misc" => Some(Self::Misc),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Equipment => "equipment",
            Self::Consumable => "consumable",
            Self::Material => "material",
            Self::Tool => "tool",
            Self::Misc => "misc",
        }
    }
}

/// Minimal generic item contract. Equipment capability remains in
/// [`crate::EquipmentDefinition`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemDefinition {
    pub content_id: ContentId,
    pub authored_id: String,
    pub domain: ContentDomain,
    pub category: ItemCategory,
    pub stack_limit: u32,
    pub drop_requires_confirmation: bool,
}

/// Client-safe icon selection for the same [`ContentId`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemPresentation {
    pub content_id: ContentId,
    pub authored_id: String,
    /// Logical visual key, never a filesystem path or renderer handle.
    pub icon: String,
}

#[must_use]
pub const fn is_stackable(def: &ItemDefinition) -> bool {
    def.stack_limit > 1
}

pub(crate) fn validate_item_catalog_identity(
    content_id: ContentId,
    label: &str,
) -> Result<(), String> {
    validate_authored_id(label).map_err(|err| format!("invalid item label ({err:?})"))?;
    if content_id.kind() != Some(ContentKind::Item) {
        return Err("item id must be allocated in the Item block".into());
    }
    if label_for_allocated_id(content_id) != Some(label) {
        return Err(format!(
            "item id {content_id} is not allocated to label '{label}' in the content catalog"
        ));
    }
    if allocated_id_for_label(label) != Some(content_id) {
        return Err(format!(
            "item label '{label}' does not resolve to id {content_id} in the content catalog"
        ));
    }
    Ok(())
}

pub fn validate_item_definition(def: &ItemDefinition) -> Result<(), ContentError> {
    let mut issues = Vec::new();
    if let Err(reason) = validate_item_catalog_identity(def.content_id, &def.authored_id) {
        issues.push(item_issue(&def.authored_id, "id", reason));
    }
    if def.stack_limit == 0 {
        issues.push(item_issue(
            &def.authored_id,
            "stack_limit",
            "must be greater than zero",
        ));
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContentError { issues })
    }
}

pub fn validate_item_presentation(def: &ItemPresentation) -> Result<(), ContentError> {
    let mut issues = Vec::new();
    if let Err(reason) = validate_item_catalog_identity(def.content_id, &def.authored_id) {
        issues.push(item_issue(&def.authored_id, "id", reason));
    }
    if let Err(reason) = crate::equipment::validate_visual_key(&def.icon) {
        issues.push(item_issue(&def.authored_id, "icon", reason));
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContentError { issues })
    }
}

pub(crate) fn item_issue(
    definition: &str,
    field: &str,
    detail: impl std::fmt::Display,
) -> ValidationIssue {
    ValidationIssue::new("item", definition, field, detail.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str, stack_limit: u32) -> ItemDefinition {
        ItemDefinition {
            content_id: ContentId::from_authored(id).unwrap(),
            authored_id: id.into(),
            domain: ContentDomain::Shared,
            category: ItemCategory::Misc,
            stack_limit,
            drop_requires_confirmation: false,
        }
    }

    #[test]
    fn valid_definition_accepts_single_and_stacked_items() {
        assert!(!is_stackable(&def("item.debug.single", 1)));
        assert!(is_stackable(&def("item.debug.stack", 20)));
        validate_item_definition(&def("item.debug.stack", 20)).unwrap();
    }

    #[test]
    fn zero_stack_limit_is_rejected() {
        let err = validate_item_definition(&def("item.debug.invalid", 0)).unwrap_err();
        assert!(err.to_string().contains("stack_limit"));
    }

    #[test]
    fn mismatched_content_id_is_rejected() {
        let mut item = def("item.debug.one", 1);
        item.content_id = ContentId::from_authored("item.debug.other").unwrap();
        let err = validate_item_definition(&item).unwrap_err();
        assert!(err.to_string().contains("does not match authored id"));
    }

    #[test]
    fn item_categories_roundtrip_the_authored_tokens() {
        for category in [
            ItemCategory::Equipment,
            ItemCategory::Consumable,
            ItemCategory::Material,
            ItemCategory::Tool,
            ItemCategory::Misc,
        ] {
            assert_eq!(ItemCategory::parse(category.as_str()), Some(category));
        }
        assert_eq!(ItemCategory::parse("quest"), None);
    }

    #[test]
    fn item_presentation_rejects_paths_as_icon_keys() {
        let def = ItemPresentation {
            content_id: ContentId::from_authored("item.debug.icon").unwrap(),
            authored_id: "item.debug.icon".into(),
            icon: "Graphic/items/icon.png".into(),
        };
        let err = validate_item_presentation(&def).unwrap_err();
        assert!(err.to_string().contains("must not be a path"));
    }
}
