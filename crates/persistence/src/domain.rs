//! Durable ownership types for the PostgreSQL writer.
//!
//! These types are the persistence-service command boundary. They do not name a
//! file journal, a map-drop clock, or a generic repository trait. Callers on
//! the persistence worker submit one command; PostgreSQL commits it or rolls
//! it back. Gameplay wiring of Drop, pickup, and dialogue stays in 12C.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use purgatory_common::{
    CONTENT_ABILITY_END, CONTENT_ABILITY_START, CONTENT_ITEM_END, CONTENT_ITEM_START,
    CONTENT_NPC_END, CONTENT_NPC_START, CharacterId, ContentId, ContentKind, ItemInstanceId,
};

use crate::error::PersistError;

/// Character inventory capacity shared with the Phase 11 runtime table.
pub const DURABLE_INVENTORY_CAPACITY: u16 = 20;

/// The six equipment slots, spelled like the simulation slot names.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum DurableEquipmentSlot {
    Headwear,
    Bodywear,
    Pants,
    Gloves,
    Boots,
    Weapon,
}

impl DurableEquipmentSlot {
    pub const ALL: [Self; 6] = [
        Self::Headwear,
        Self::Bodywear,
        Self::Pants,
        Self::Gloves,
        Self::Boots,
        Self::Weapon,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Headwear => "headwear",
            Self::Bodywear => "bodywear",
            Self::Pants => "pants",
            Self::Gloves => "gloves",
            Self::Boots => "boots",
            Self::Weapon => "weapon",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.as_str() == name)
    }
}

/// Character-relative item location. No `EntityId` or `ConnectionId`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CharacterItemLocation {
    Inventory { slot: u16 },
    Equipped { slot: DurableEquipmentSlot },
}

/// Where a live or retired item instance sits. A ground owner is temporary.
/// Retirement keeps the id and does not name a previous character.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemOwner {
    Character {
        character_id: CharacterId,
        location: CharacterItemLocation,
    },
    Ground,
    Retired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemRecord {
    pub item_instance_id: ItemInstanceId,
    pub definition_content_id: ContentId,
    pub quantity: u32,
    pub owner: ItemOwner,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaceNewItem {
    pub owner: CharacterId,
    pub definition_content_id: ContentId,
    pub quantity: u32,
    pub location: CharacterItemLocation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveItem {
    pub item_instance_id: ItemInstanceId,
    pub to: LiveDestination,
}

/// Destination of a live item. Retired is not a move target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveDestination {
    Character {
        character_id: CharacterId,
        location: CharacterItemLocation,
    },
    Ground,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NarrativeWrite {
    SetFact {
        character_id: CharacterId,
        fact_key: String,
        value: bool,
    },
    ClearFact {
        character_id: CharacterId,
        fact_key: String,
    },
    MarkNpcMet {
        character_id: CharacterId,
        npc_authored: String,
    },
    /// `beat_id` is the authored beat id. A beat index is rejected.
    MarkDialogueHeard {
        character_id: CharacterId,
        npc_content_id: ContentId,
        beat_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearnedAbilityWrite {
    pub character_id: CharacterId,
    pub ability_content_id: ContentId,
}

/// What a previously reserved item id becomes in one durable command.
///
/// The id was allocated before it was visible. This command inserts that exact
/// id. It does not draw a new id from `next_item_instance_id`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReservedItemOutcome {
    Inventory { owner: CharacterId, slot: u16 },
    Retired,
}

/// One already-reserved item id to insert as inventory or a retired stub.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReservedItemUse {
    pub item_instance_id: ItemInstanceId,
    pub definition_content_id: ContentId,
    pub quantity: u32,
    pub outcome: ReservedItemOutcome,
}

/// One accepted economic or earned-state command.
///
/// `expected_revisions` lists every character the command affects, including
/// the current owner of a moved or retired item.
/// The stored result is what a retry returns after a lost reply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableCommand {
    pub key: String,
    /// Characters whose committed revision must match. Sources of moved or
    /// retired items must be included as well as every character the command
    /// writes. Duplicate ids are rejected.
    pub expected_revisions: Vec<(CharacterId, u64)>,
    pub place_new: Vec<PlaceNewItem>,
    pub moves: Vec<MoveItem>,
    pub retire: Vec<ItemInstanceId>,
    pub narrative: Vec<NarrativeWrite>,
    pub learned: Vec<LearnedAbilityWrite>,
    /// Explicit ids reserved by the durable allocator before they were visible.
    pub reserved_uses: Vec<ReservedItemUse>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableCommandResult {
    pub revisions: Vec<(CharacterId, u64)>,
    pub minted_item_ids: Vec<ItemInstanceId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CharacterNarrativeState {
    pub facts: BTreeMap<String, bool>,
    pub npcs_met: BTreeSet<String>,
    /// NPC numeric content id and authored beat id.
    pub dialogue_heard: BTreeSet<(u32, String)>,
    pub learned_abilities: BTreeSet<u32>,
}

/// Facts the server installs before an item or ability id is durable.
///
/// The persistence crate does not load content files. Retired and unknown ids
/// fail closed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemContentRule {
    pub content_id: ContentId,
    pub stack_limit: u32,
    pub equip_slot: Option<DurableEquipmentSlot>,
    pub retired: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DurableContentRules {
    items: BTreeMap<u32, ItemContentRule>,
    abilities: BTreeMap<u32, bool>,
}

impl DurableContentRules {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_item(&mut self, rule: ItemContentRule) -> Result<(), String> {
        let Some(raw) = rule.content_id.raw() else {
            return Err("item content id is outside the numeric catalog".into());
        };
        if rule.content_id.kind() != Some(ContentKind::Item) {
            return Err(format!("content id {raw} is not in the item block"));
        }
        if rule.stack_limit == 0 {
            return Err(format!("item {raw} stack_limit must be positive"));
        }
        self.items.insert(raw, rule);
        Ok(())
    }

    pub fn insert_ability(&mut self, content_id: ContentId, retired: bool) -> Result<(), String> {
        let Some(raw) = content_id.raw() else {
            return Err("ability content id is outside the numeric catalog".into());
        };
        if content_id.kind() != Some(ContentKind::Ability) {
            return Err(format!("content id {raw} is not in the ability block"));
        }
        self.abilities.insert(raw, retired);
        Ok(())
    }

    #[must_use]
    pub fn item(&self, content_id: ContentId) -> Option<&ItemContentRule> {
        content_id.raw().and_then(|raw| self.items.get(&raw))
    }

    #[must_use]
    pub fn ability_retired(&self, content_id: ContentId) -> Option<bool> {
        content_id
            .raw()
            .and_then(|raw| self.abilities.get(&raw).copied())
    }
}

pub(crate) fn db_path() -> PathBuf {
    PathBuf::from("<postgresql>")
}

pub(crate) fn validate_command(
    command: &DurableCommand,
    rules: &DurableContentRules,
) -> Result<(), PersistError> {
    let path = db_path();
    if command.key.is_empty()
        || command.key.len() > 128
        || !command.key.bytes().all(|byte| (32..=126).contains(&byte))
    {
        return Err(PersistError::corrupt(
            &path,
            "command key must be 1..=128 visible ASCII characters",
        ));
    }
    if command.place_new.is_empty()
        && command.moves.is_empty()
        && command.retire.is_empty()
        && command.narrative.is_empty()
        && command.learned.is_empty()
        && command.reserved_uses.is_empty()
    {
        return Err(PersistError::corrupt(&path, "durable command is empty"));
    }
    for revision in command
        .expected_revisions
        .iter()
        .map(|(_, revision)| *revision)
    {
        if revision == 0 {
            return Err(PersistError::corrupt(
                &path,
                "expected persistence_revision 0 is reserved",
            ));
        }
    }
    let mut seen_revisions = BTreeSet::new();
    for (character_id, _) in &command.expected_revisions {
        if !seen_revisions.insert(character_id.raw()) {
            return Err(PersistError::corrupt(
                &path,
                format!(
                    "duplicate expected revision for character {}",
                    character_id.raw()
                ),
            ));
        }
    }
    let mut seen_items = BTreeSet::new();
    for id in command
        .moves
        .iter()
        .map(|item| item.item_instance_id)
        .chain(command.retire.iter().copied())
        .chain(
            command
                .reserved_uses
                .iter()
                .map(|use_| use_.item_instance_id),
        )
    {
        if id.raw() == 0 {
            return Err(PersistError::corrupt(
                &path,
                "item_instance_id 0 is reserved",
            ));
        }
        if !seen_items.insert(id.raw()) {
            return Err(PersistError::corrupt(
                &path,
                format!("item {} is named more than once in the command", id.raw()),
            ));
        }
    }
    for place in &command.place_new {
        validate_quantity_location(place.quantity, place.location, &path)?;
        validate_item_content(
            place.definition_content_id,
            place.quantity,
            place.location,
            rules,
        )?;
    }
    for reserved in &command.reserved_uses {
        match reserved.outcome {
            ReservedItemOutcome::Inventory { slot, .. } => {
                let location = CharacterItemLocation::Inventory { slot };
                validate_quantity_location(reserved.quantity, location, &path)?;
                validate_item_content(
                    reserved.definition_content_id,
                    reserved.quantity,
                    location,
                    rules,
                )?;
            }
            ReservedItemOutcome::Retired => {
                validate_item_content(
                    reserved.definition_content_id,
                    reserved.quantity,
                    CharacterItemLocation::Inventory { slot: 0 },
                    rules,
                )?;
                if reserved.quantity == 0 {
                    return Err(PersistError::corrupt(
                        &path,
                        "item quantity must be positive",
                    ));
                }
            }
        }
    }
    for write in &command.narrative {
        validate_narrative(write)?;
    }
    for grant in &command.learned {
        validate_ability(grant.ability_content_id, rules)?;
    }
    let affected = affected_characters(command);
    let expected: BTreeSet<_> = command
        .expected_revisions
        .iter()
        .map(|(character_id, _)| *character_id)
        .collect();
    if !affected.is_subset(&expected) {
        return Err(PersistError::corrupt(
            &path,
            "expected revisions must include every character the command writes",
        ));
    }
    Ok(())
}

pub(crate) fn affected_characters(command: &DurableCommand) -> BTreeSet<CharacterId> {
    let mut affected = BTreeSet::new();
    for place in &command.place_new {
        affected.insert(place.owner);
    }
    for item in &command.moves {
        if let LiveDestination::Character { character_id, .. } = item.to {
            affected.insert(character_id);
        }
    }
    for write in &command.narrative {
        affected.insert(match write {
            NarrativeWrite::SetFact { character_id, .. }
            | NarrativeWrite::ClearFact { character_id, .. }
            | NarrativeWrite::MarkNpcMet { character_id, .. }
            | NarrativeWrite::MarkDialogueHeard { character_id, .. } => *character_id,
        });
    }
    for grant in &command.learned {
        affected.insert(grant.character_id);
    }
    for reserved in &command.reserved_uses {
        if let ReservedItemOutcome::Inventory { owner, .. } = reserved.outcome {
            affected.insert(owner);
        }
    }
    affected
}

fn validate_quantity_location(
    quantity: u32,
    location: CharacterItemLocation,
    path: &std::path::Path,
) -> Result<(), PersistError> {
    if quantity == 0 {
        return Err(PersistError::corrupt(
            path,
            "item quantity must be positive",
        ));
    }
    if let CharacterItemLocation::Inventory { slot } = location
        && slot >= DURABLE_INVENTORY_CAPACITY
    {
        return Err(PersistError::corrupt(
            path,
            format!("inventory slot {slot} is outside 0..{DURABLE_INVENTORY_CAPACITY}"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_item_content(
    definition: ContentId,
    quantity: u32,
    location: CharacterItemLocation,
    rules: &DurableContentRules,
) -> Result<(), PersistError> {
    let path = db_path();
    if !is_item_block(definition) {
        return Err(PersistError::content(
            &path,
            format!(
                "definition {} is outside the item block",
                definition.token()
            ),
        ));
    }
    let Some(rule) = rules.item(definition) else {
        return Err(PersistError::content(
            &path,
            format!(
                "content {} is missing from the installed catalog",
                definition.token()
            ),
        ));
    };
    if rule.retired {
        return Err(PersistError::content(
            &path,
            format!("content {} is retired", definition.token()),
        ));
    }
    if quantity > rule.stack_limit {
        return Err(PersistError::content(
            &path,
            format!(
                "quantity {quantity} exceeds stack_limit {}",
                rule.stack_limit
            ),
        ));
    }
    if let CharacterItemLocation::Equipped { slot } = location {
        match rule.equip_slot {
            Some(authorized) if authorized == slot => {}
            Some(authorized) => {
                return Err(PersistError::content(
                    &path,
                    format!(
                        "equip slot {} is not the authorized {} slot",
                        slot.as_str(),
                        authorized.as_str()
                    ),
                ));
            }
            None => {
                return Err(PersistError::content(
                    &path,
                    format!(
                        "content {} has no equipment facet for {}",
                        definition.token(),
                        slot.as_str()
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_ability(
    definition: ContentId,
    rules: &DurableContentRules,
) -> Result<(), PersistError> {
    let path = db_path();
    if !matches!(
        definition.raw(),
        Some(raw) if (CONTENT_ABILITY_START..=CONTENT_ABILITY_END).contains(&raw)
    ) {
        return Err(PersistError::content(
            &path,
            format!(
                "ability {} is outside the ability block",
                definition.token()
            ),
        ));
    }
    match rules.ability_retired(definition) {
        None => Err(PersistError::content(
            &path,
            format!(
                "ability {} is missing from the installed catalog",
                definition.token()
            ),
        )),
        Some(true) => Err(PersistError::content(
            &path,
            format!("ability {} is retired", definition.token()),
        )),
        Some(false) => Ok(()),
    }
}

fn validate_narrative(write: &NarrativeWrite) -> Result<(), PersistError> {
    let path = db_path();
    match write {
        NarrativeWrite::SetFact { fact_key, .. } | NarrativeWrite::ClearFact { fact_key, .. } => {
            semantic_key(fact_key, "fact key", &path)
        }
        NarrativeWrite::MarkNpcMet { npc_authored, .. } => {
            semantic_key(npc_authored, "npc authored id", &path)
        }
        NarrativeWrite::MarkDialogueHeard {
            npc_content_id,
            beat_id,
            ..
        } => {
            if !matches!(
                npc_content_id.raw(),
                Some(raw) if (CONTENT_NPC_START..=CONTENT_NPC_END).contains(&raw)
            ) {
                return Err(PersistError::content(
                    &path,
                    format!(
                        "dialogue npc {} is outside the npc block",
                        npc_content_id.token()
                    ),
                ));
            }
            // A bare integer is a beat index. Authored beat ids in content
            // contain at least one letter (`intro`, `ask_road`).
            if beat_id.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(PersistError::corrupt(
                    &path,
                    "dialogue heard must use an authored beat id, not a beat index",
                ));
            }
            semantic_key(beat_id, "beat id", &path)
        }
    }
}

fn semantic_key(value: &str, label: &str, path: &std::path::Path) -> Result<(), PersistError> {
    let bytes = value.as_bytes();
    if (1..=80).contains(&bytes.len())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Ok(());
    }
    Err(PersistError::corrupt(
        path,
        format!("{label} must be 1..=80 ASCII letters, digits, '.', '_' or '-'"),
    ))
}

fn is_item_block(id: ContentId) -> bool {
    matches!(id.raw(), Some(raw) if (CONTENT_ITEM_START..=CONTENT_ITEM_END).contains(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> DurableContentRules {
        let mut rules = DurableContentRules::new();
        rules
            .insert_item(ItemContentRule {
                content_id: ContentId::from_raw(30_001),
                stack_limit: 20,
                equip_slot: None,
                retired: false,
            })
            .unwrap();
        rules
            .insert_ability(ContentId::from_raw(40_001), false)
            .unwrap();
        rules
    }

    fn place(slot: u16) -> PlaceNewItem {
        PlaceNewItem {
            owner: CharacterId::from_raw(1),
            definition_content_id: ContentId::from_raw(30_001),
            quantity: 1,
            location: CharacterItemLocation::Inventory { slot },
        }
    }

    #[test]
    fn numeric_beat_id_is_rejected() {
        let command = DurableCommand {
            key: "heard".into(),
            expected_revisions: vec![(CharacterId::from_raw(1), 1)],
            place_new: Vec::new(),
            moves: Vec::new(),
            retire: Vec::new(),
            narrative: vec![NarrativeWrite::MarkDialogueHeard {
                character_id: CharacterId::from_raw(1),
                npc_content_id: ContentId::from_raw(20_001),
                beat_id: "3".into(),
            }],
            learned: Vec::new(),

            reserved_uses: Vec::new(),
        };
        let err = validate_command(&command, &rules()).unwrap_err();
        assert!(matches!(err, PersistError::Corrupt { .. }), "{err}");
    }

    #[test]
    fn authored_beat_id_and_matching_owners_pass() {
        let command = DurableCommand {
            key: "heard".into(),
            expected_revisions: vec![(CharacterId::from_raw(1), 1)],
            place_new: vec![place(0)],
            moves: Vec::new(),
            retire: Vec::new(),
            narrative: vec![NarrativeWrite::MarkDialogueHeard {
                character_id: CharacterId::from_raw(1),
                npc_content_id: ContentId::from_raw(20_001),
                beat_id: "intro".into(),
            }],
            learned: Vec::new(),

            reserved_uses: Vec::new(),
        };
        validate_command(&command, &rules()).unwrap();
    }

    #[test]
    fn missing_catalog_and_unlisted_owner_fail_closed() {
        let command = DurableCommand {
            key: "mint".into(),
            expected_revisions: Vec::new(),
            place_new: vec![place(0)],
            moves: Vec::new(),
            retire: Vec::new(),
            narrative: Vec::new(),
            learned: Vec::new(),

            reserved_uses: Vec::new(),
        };
        let err = validate_command(&command, &DurableContentRules::new()).unwrap_err();
        assert!(matches!(err, PersistError::ContentRejected { .. }), "{err}");

        let err = validate_command(&command, &rules()).unwrap_err();
        assert!(matches!(err, PersistError::Corrupt { .. }), "{err}");
    }

    #[test]
    fn inventory_slot_past_capacity_is_rejected() {
        let mut command = DurableCommand {
            key: "slot".into(),
            expected_revisions: vec![(CharacterId::from_raw(1), 1)],
            place_new: vec![place(DURABLE_INVENTORY_CAPACITY)],
            moves: Vec::new(),
            retire: Vec::new(),
            narrative: Vec::new(),
            learned: Vec::new(),

            reserved_uses: Vec::new(),
        };
        let err = validate_command(&command, &rules()).unwrap_err();
        assert!(matches!(err, PersistError::Corrupt { .. }), "{err}");
        command.place_new[0].location = CharacterItemLocation::Inventory { slot: 0 };
        command.place_new[0].quantity = 0;
        let err = validate_command(&command, &rules()).unwrap_err();
        assert!(matches!(err, PersistError::Corrupt { .. }), "{err}");
    }
}
