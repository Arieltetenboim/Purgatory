//! Generic authored item definitions.

use crate::domain::ContentDomain;
use crate::error::{ContentError, ValidationIssue};
use purgatory_common::{ContentId, validate_authored_id};

/// Item content schema v1.
pub const ITEM_CONTENT_SCHEMA_VERSION: u32 = 1;

/// Minimal generic item contract. Equipment capability remains in
/// [`crate::EquipmentDefinition`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemDefinition {
    pub content_id: ContentId,
    pub authored_id: String,
    pub domain: ContentDomain,
    pub stack_limit: u32,
}

#[must_use]
pub const fn is_stackable(def: &ItemDefinition) -> bool {
    def.stack_limit > 1
}

pub fn validate_item_definition(def: &ItemDefinition) -> Result<(), ContentError> {
    let mut issues = Vec::new();
    if let Err(err) = validate_authored_id(&def.authored_id) {
        issues.push(item_issue(
            &def.authored_id,
            "id",
            format!("invalid ContentId ({err:?})"),
        ));
    } else if ContentId::from_authored(&def.authored_id).expect("validated") != def.content_id {
        issues.push(item_issue(
            &def.authored_id,
            "id",
            "content_id does not match authored id",
        ));
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
            stack_limit,
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
}
