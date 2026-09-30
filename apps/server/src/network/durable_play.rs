//! Build one durable command for an accepted gameplay outcome.
//!
//! The simulation thread validates and reserves. It does not call PostgreSQL.
//! A connection-side task submits the command. `World` changes only after the
//! committed result returns.

use purgatory_common::{CharacterId, ContentId, ItemInstanceId};
use purgatory_content::ContentRegistry;
use purgatory_persistence::{
    CharacterItemLocation, DurableCommand, DurableContentRules, DurableEquipmentSlot,
    ItemContentRule, LearnedAbilityWrite, LiveDestination, MoveItem, NarrativeWrite, PlaceNewItem,
};
use purgatory_simulation::EquipmentSlot;

use super::dialogue::ChoicePlan;

#[derive(Clone, Debug)]
pub(crate) struct DurableSubmit {
    pub token: u64,
    pub command: DurableCommand,
    pub lease: Option<purgatory_persistence::LeaseAuthority>,
}

pub(crate) fn commit_outcome_unknown(err: &purgatory_persistence::PersistError) -> bool {
    matches!(err, purgatory_persistence::PersistError::Storage { reason } if reason.starts_with("commit outcome unknown"))
}

#[derive(Clone, Debug)]
pub(crate) enum DurableEffect {
    Drop {
        connection_id: purgatory_protocol::ConnectionId,
        seq: u32,
        item: ItemInstanceId,
    },
    Pickup {
        connection_id: purgatory_protocol::ConnectionId,
        seq: u32,
        item: ItemInstanceId,
        slot: u16,
        /// `true` when the instance is already a durable ground item.
        durable: bool,
        definition: ContentId,
        quantity: u32,
        stack_limit: u32,
    },
    Equip {
        connection_id: purgatory_protocol::ConnectionId,
        seq: u32,
        item: ItemInstanceId,
        slot: EquipmentSlot,
    },
    Unequip {
        connection_id: purgatory_protocol::ConnectionId,
        seq: u32,
        slot: EquipmentSlot,
    },
    // No authored expiry duration exists yet, so production does not start
    // this effect. Tests retire a live drop explicitly.
    #[cfg_attr(not(test), allow(dead_code))]
    RetireGround {
        connection_id: purgatory_protocol::ConnectionId,
        item: ItemInstanceId,
    },
    Dialogue {
        connection_id: purgatory_protocol::ConnectionId,
        plan: ChoicePlan,
        beat_id: String,
        /// definition, quantity, stack limit, reserved inventory slot.
        gives: Vec<(ContentId, u32, u32, u16)>,
        retires: Vec<ItemInstanceId>,
    },
    Heard {
        connection_id: purgatory_protocol::ConnectionId,
        npc_content_id: ContentId,
        beat_index: purgatory_content::DialogueBeatIndex,
        session_id: u32,
    },
}

pub(crate) fn rules_from_registry(registry: &ContentRegistry) -> DurableContentRules {
    let mut rules = DurableContentRules::new();
    for item in registry.iter_items() {
        let equip_slot = registry
            .equipment_by_id(item.content_id)
            .map(|equipment| to_durable_slot(equipment.slot));
        let _ = rules.insert_item(ItemContentRule {
            content_id: item.content_id,
            stack_limit: item.stack_limit,
            equip_slot,
            retired: false,
        });
    }
    for ability in registry.iter_abilities() {
        let _ = rules.insert_ability(ability.id, false);
    }
    rules
}

pub(crate) fn to_durable_slot(slot: EquipmentSlot) -> DurableEquipmentSlot {
    match slot {
        EquipmentSlot::Headwear => DurableEquipmentSlot::Headwear,
        EquipmentSlot::Bodywear => DurableEquipmentSlot::Bodywear,
        EquipmentSlot::Pants => DurableEquipmentSlot::Pants,
        EquipmentSlot::Gloves => DurableEquipmentSlot::Gloves,
        EquipmentSlot::Boots => DurableEquipmentSlot::Boots,
        EquipmentSlot::Weapon => DurableEquipmentSlot::Weapon,
    }
}

fn key(character_id: CharacterId, revision: u64, body: &str) -> String {
    format!("c{}-r{revision}-{body}", character_id.raw())
}

pub(crate) fn drop_command(
    character_id: CharacterId,
    revision: u64,
    item: ItemInstanceId,
) -> DurableCommand {
    DurableCommand {
        key: key(character_id, revision, &format!("drop-{}", item.raw())),
        expected_revisions: vec![(character_id, revision)],
        place_new: Vec::new(),
        moves: vec![MoveItem {
            item_instance_id: item,
            to: LiveDestination::Ground,
        }],
        retire: Vec::new(),
        narrative: Vec::new(),
        learned: Vec::new(),
    }
}

pub(crate) fn pickup_place_command(
    character_id: CharacterId,
    revision: u64,
    world_item: ItemInstanceId,
    definition: ContentId,
    quantity: u32,
    slot: u16,
) -> DurableCommand {
    DurableCommand {
        key: key(
            character_id,
            revision,
            &format!("pickup-new-{}", world_item.raw()),
        ),
        expected_revisions: vec![(character_id, revision)],
        place_new: vec![PlaceNewItem {
            owner: character_id,
            definition_content_id: definition,
            quantity,
            location: CharacterItemLocation::Inventory { slot },
        }],
        moves: Vec::new(),
        retire: Vec::new(),
        narrative: Vec::new(),
        learned: Vec::new(),
    }
}

pub(crate) fn pickup_command(
    character_id: CharacterId,
    revision: u64,
    item: ItemInstanceId,
    slot: u16,
) -> DurableCommand {
    DurableCommand {
        key: key(character_id, revision, &format!("pickup-{}", item.raw())),
        expected_revisions: vec![(character_id, revision)],
        place_new: Vec::new(),
        moves: vec![MoveItem {
            item_instance_id: item,
            to: LiveDestination::Character {
                character_id,
                location: CharacterItemLocation::Inventory { slot },
            },
        }],
        retire: Vec::new(),
        narrative: Vec::new(),
        learned: Vec::new(),
    }
}

pub(crate) fn equip_command(
    character_id: CharacterId,
    revision: u64,
    item: ItemInstanceId,
    slot: EquipmentSlot,
    displaced: Option<(ItemInstanceId, u16)>,
) -> DurableCommand {
    let mut moves = Vec::new();
    if let Some((displaced_item, inventory_slot)) = displaced {
        moves.push(MoveItem {
            item_instance_id: displaced_item,
            to: LiveDestination::Character {
                character_id,
                location: CharacterItemLocation::Inventory {
                    slot: inventory_slot,
                },
            },
        });
    }
    moves.push(MoveItem {
        item_instance_id: item,
        to: LiveDestination::Character {
            character_id,
            location: CharacterItemLocation::Equipped {
                slot: to_durable_slot(slot),
            },
        },
    });
    DurableCommand {
        key: key(
            character_id,
            revision,
            &format!("equip-{}-{}", item.raw(), to_durable_slot(slot).as_str()),
        ),
        expected_revisions: vec![(character_id, revision)],
        place_new: Vec::new(),
        moves,
        retire: Vec::new(),
        narrative: Vec::new(),
        learned: Vec::new(),
    }
}

pub(crate) fn unequip_command(
    character_id: CharacterId,
    revision: u64,
    item: ItemInstanceId,
    from: EquipmentSlot,
    inventory_slot: u16,
) -> DurableCommand {
    DurableCommand {
        key: key(
            character_id,
            revision,
            &format!("unequip-{}-{}", item.raw(), to_durable_slot(from).as_str()),
        ),
        expected_revisions: vec![(character_id, revision)],
        place_new: Vec::new(),
        moves: vec![MoveItem {
            item_instance_id: item,
            to: LiveDestination::Character {
                character_id,
                location: CharacterItemLocation::Inventory {
                    slot: inventory_slot,
                },
            },
        }],
        retire: Vec::new(),
        narrative: Vec::new(),
        learned: Vec::new(),
    }
}

/// Ground has no character owner, so this command cannot carry a character
/// revision. The live session still reserves the item before submission.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn retire_command(item: ItemInstanceId) -> DurableCommand {
    DurableCommand {
        key: format!("retire-{}", item.raw()),
        expected_revisions: Vec::new(),
        place_new: Vec::new(),
        moves: Vec::new(),
        retire: vec![item],
        narrative: Vec::new(),
        learned: Vec::new(),
    }
}

pub(crate) struct DialogueCommandParts {
    pub character_id: CharacterId,
    pub revision: u64,
    pub npc_content_id: ContentId,
    pub beat_id: String,
    pub choice_index: u32,
    pub places: Vec<PlaceNewItem>,
    pub retire: Vec<ItemInstanceId>,
    pub facts: Vec<(String, bool)>,
    pub npcs_met: Vec<String>,
    pub learned: Vec<ContentId>,
}

pub(crate) fn dialogue_command(parts: DialogueCommandParts) -> DurableCommand {
    let mut narrative = Vec::new();
    for (fact, value) in parts.facts {
        narrative.push(NarrativeWrite::SetFact {
            character_id: parts.character_id,
            fact_key: fact,
            value,
        });
    }
    for npc in parts.npcs_met {
        narrative.push(NarrativeWrite::MarkNpcMet {
            character_id: parts.character_id,
            npc_authored: npc,
        });
    }
    narrative.push(NarrativeWrite::MarkDialogueHeard {
        character_id: parts.character_id,
        npc_content_id: parts.npc_content_id,
        beat_id: parts.beat_id.clone(),
    });
    DurableCommand {
        key: key(
            parts.character_id,
            parts.revision,
            &format!(
                "talk-{}-{}-{}",
                parts.npc_content_id.raw().unwrap_or(0),
                parts.beat_id,
                parts.choice_index
            ),
        ),
        expected_revisions: vec![(parts.character_id, parts.revision)],
        place_new: parts.places,
        moves: Vec::new(),
        retire: parts.retire,
        narrative,
        learned: parts
            .learned
            .into_iter()
            .map(|ability_content_id| LearnedAbilityWrite {
                character_id: parts.character_id,
                ability_content_id,
            })
            .collect(),
    }
}

pub(crate) fn heard_command(
    character_id: CharacterId,
    revision: u64,
    npc_content_id: ContentId,
    beat_id: &str,
) -> DurableCommand {
    DurableCommand {
        key: key(
            character_id,
            revision,
            &format!("heard-{}-{beat_id}", npc_content_id.raw().unwrap_or(0)),
        ),
        expected_revisions: vec![(character_id, revision)],
        place_new: Vec::new(),
        moves: Vec::new(),
        retire: Vec::new(),
        narrative: vec![NarrativeWrite::MarkDialogueHeard {
            character_id,
            npc_content_id,
            beat_id: beat_id.to_string(),
        }],
        learned: Vec::new(),
    }
}
