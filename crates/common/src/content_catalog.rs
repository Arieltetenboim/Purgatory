//! First-party stable numeric content allocations.
//!
//! This is the code-side companion to `content/CONTENT_ID_CATALOG.md`.
//! Numbers are permanent once allocated. Labels are migration/search metadata only.

use crate::{ContentId, ContentKind};

pub const ABILITY_BASIC_STRIKE: ContentId = ContentId::from_raw(40_001);
pub const ABILITY_PRACTICE_SWORD_STRIKE: ContentId = ContentId::from_raw(40_002);

pub const MAP_FOOTNOTE: ContentId = ContentId::from_raw(50_001);
pub const MAP_SECOND: ContentId = ContentId::from_raw(50_002);

pub const ITEM_CLOTH_CAP: ContentId = ContentId::from_raw(30_001);
pub const ITEM_CLOTH_PANTS: ContentId = ContentId::from_raw(30_002);
pub const ITEM_IRON_BOOTS: ContentId = ContentId::from_raw(30_003);
pub const ITEM_LEATHER_GLOVES: ContentId = ContentId::from_raw(30_004);
pub const ITEM_PLATE_CUIRASS: ContentId = ContentId::from_raw(30_005);
pub const ITEM_PRACTICE_SWORD: ContentId = ContentId::from_raw(30_006);
pub const ITEM_TUNIC: ContentId = ContentId::from_raw(30_007);
pub const ITEM_UNADORNED: ContentId = ContentId::from_raw(30_008);

pub const WORLD_OBJECT_CHEST: ContentId = ContentId::from_raw(60_001);
pub const WORLD_OBJECT_MAP_B_SWITCH: ContentId = ContentId::from_raw(60_002);
pub const WORLD_OBJECT_SWITCH: ContentId = ContentId::from_raw(60_003);
pub const WORLD_OBJECT_PORTAL_TO_FOOTNOTE: ContentId = ContentId::from_raw(60_004);
pub const WORLD_OBJECT_PORTAL_TO_SECOND: ContentId = ContentId::from_raw(60_005);

/// Temporary migration lookup for existing authored labels.
///
/// This must disappear after production JSON/call sites store the numeric IDs directly.
#[must_use]
pub fn allocated_id_for_label(label: &str) -> Option<ContentId> {
    Some(match label {
        "skill.basic.strike" => ABILITY_BASIC_STRIKE,
        "skill.debug.practice_sword_strike" => ABILITY_PRACTICE_SWORD_STRIKE,
        "map.dev.footnote" => MAP_FOOTNOTE,
        "map.dev.second" => MAP_SECOND,
        "equipment.debug.cloth_cap" => ITEM_CLOTH_CAP,
        "equipment.debug.cloth_pants" => ITEM_CLOTH_PANTS,
        "equipment.debug.iron_boots" => ITEM_IRON_BOOTS,
        "equipment.debug.leather_gloves" => ITEM_LEATHER_GLOVES,
        "equipment.debug.plate_cuirass" => ITEM_PLATE_CUIRASS,
        "equipment.debug.practice_sword" => ITEM_PRACTICE_SWORD,
        "equipment.debug.tunic" => ITEM_TUNIC,
        "equipment.debug.unadorned" => ITEM_UNADORNED,
        "entity.interactable.chest" => WORLD_OBJECT_CHEST,
        "entity.interactable.map_b_switch" => WORLD_OBJECT_MAP_B_SWITCH,
        "entity.interactable.switch" => WORLD_OBJECT_SWITCH,
        "entity.portal.to_footnote" => WORLD_OBJECT_PORTAL_TO_FOOTNOTE,
        "entity.portal.to_second" => WORLD_OBJECT_PORTAL_TO_SECOND,
        _ => return None,
    })
}

#[must_use]
pub fn label_for_allocated_id(id: ContentId) -> Option<&'static str> {
    Some(match id {
        ABILITY_BASIC_STRIKE => "skill.basic.strike",
        ABILITY_PRACTICE_SWORD_STRIKE => "skill.debug.practice_sword_strike",
        MAP_FOOTNOTE => "map.dev.footnote",
        MAP_SECOND => "map.dev.second",
        ITEM_CLOTH_CAP => "equipment.debug.cloth_cap",
        ITEM_CLOTH_PANTS => "equipment.debug.cloth_pants",
        ITEM_IRON_BOOTS => "equipment.debug.iron_boots",
        ITEM_LEATHER_GLOVES => "equipment.debug.leather_gloves",
        ITEM_PLATE_CUIRASS => "equipment.debug.plate_cuirass",
        ITEM_PRACTICE_SWORD => "equipment.debug.practice_sword",
        ITEM_TUNIC => "equipment.debug.tunic",
        ITEM_UNADORNED => "equipment.debug.unadorned",
        WORLD_OBJECT_CHEST => "entity.interactable.chest",
        WORLD_OBJECT_MAP_B_SWITCH => "entity.interactable.map_b_switch",
        WORLD_OBJECT_SWITCH => "entity.interactable.switch",
        WORLD_OBJECT_PORTAL_TO_FOOTNOTE => "entity.portal.to_footnote",
        WORLD_OBJECT_PORTAL_TO_SECOND => "entity.portal.to_second",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocations_are_in_the_expected_blocks() {
        for id in [
            ITEM_CLOTH_CAP,
            ITEM_CLOTH_PANTS,
            ITEM_IRON_BOOTS,
            ITEM_LEATHER_GLOVES,
            ITEM_PLATE_CUIRASS,
            ITEM_PRACTICE_SWORD,
            ITEM_TUNIC,
            ITEM_UNADORNED,
        ] {
            assert_eq!(id.kind(), Some(ContentKind::Item));
        }
        for id in [ABILITY_BASIC_STRIKE, ABILITY_PRACTICE_SWORD_STRIKE] {
            assert_eq!(id.kind(), Some(ContentKind::Ability));
        }
        for id in [MAP_FOOTNOTE, MAP_SECOND] {
            assert_eq!(id.kind(), Some(ContentKind::Map));
        }
        for id in [
            WORLD_OBJECT_CHEST,
            WORLD_OBJECT_MAP_B_SWITCH,
            WORLD_OBJECT_SWITCH,
            WORLD_OBJECT_PORTAL_TO_FOOTNOTE,
            WORLD_OBJECT_PORTAL_TO_SECOND,
        ] {
            assert_eq!(id.kind(), Some(ContentKind::WorldObject));
        }
    }

    #[test]
    fn migration_labels_round_trip_through_one_allocation() {
        for label in [
            "skill.basic.strike",
            "map.dev.footnote",
            "equipment.debug.practice_sword",
            "entity.portal.to_second",
        ] {
            let id = allocated_id_for_label(label).expect("allocated label");
            assert_eq!(label_for_allocated_id(id), Some(label));
        }
        assert_eq!(allocated_id_for_label("item.not.allocated"), None);
    }
}
