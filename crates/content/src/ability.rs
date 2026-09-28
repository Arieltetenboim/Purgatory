//! Ability content schema v2. Runtime type is [`purgatory_simulation::AbilityDefinition`].
//!
//! `content_id` is the canonical Ability-block catalog ID. `id` is the human-readable
//! authored label and must match that allocation exactly.

use purgatory_common::{
    ContentId, ContentKind, allocated_id_for_label, label_for_allocated_id, validate_authored_id,
};

pub const ABILITY_CONTENT_SCHEMA_VERSION: u32 = 2;

pub(crate) fn validate_ability_catalog_identity(
    content_id: ContentId,
    label: &str,
) -> Result<(), String> {
    validate_authored_id(label).map_err(|err| format!("invalid ability id ({err:?})"))?;
    if content_id.kind() != Some(ContentKind::Ability) {
        return Err("ability id must be allocated in the Ability block".into());
    }
    if label_for_allocated_id(content_id) != Some(label) {
        return Err(format!(
            "ability id {content_id} is not allocated to label '{label}' in the content catalog"
        ));
    }
    if allocated_id_for_label(label) != Some(content_id) {
        return Err(format!(
            "ability label '{label}' does not resolve to id {content_id} in the content catalog"
        ));
    }
    Ok(())
}
