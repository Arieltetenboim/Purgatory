//! Versioned durable-domain types.
//!
//! These operations are the persistence service boundary. File paths, JSON,
//! log framing, checkpoint replacement and `sync_all` stay in the file-backed
//! journal. A later database can implement the same operations; this module
//! does not name a database, a second writer, or a generic repository trait.
//!
//! Filesystem work belongs on the persistence worker, never on the 30 Hz
//! simulation tick. `reserve_item_instance_ids` and clock checkpoints perform
//! I/O and must be called from that worker. Gameplay may consume an
//! already-returned id range without further I/O.

use std::collections::BTreeMap;

use purgatory_common::{
    CONTENT_ITEM_END, CONTENT_ITEM_START, CONTENT_MAP_END, CONTENT_MAP_START, CharacterId,
    ContentId, ContentKind, ItemInstanceId,
};
use serde::{Deserialize, Serialize};

use crate::error::PersistError;

/// Domain operation version. Independent of character-file schema version.
pub const DURABLE_DOMAIN_VERSION: u32 = 1;

/// One second of the authoritative 30 Hz simulation clock.
///
/// Drop timers are measured in these ticks. A periodic checkpoint may advance
/// the durable clock by at most this many ticks; a crash can therefore give a
/// Drop at most one second of extra active life, never less downtime.
pub const ACTIVE_SERVER_TICKS_PER_SECOND: u64 = 30;

/// Character inventory capacity shared with the Phase 11 runtime table.
pub const DURABLE_INVENTORY_CAPACITY: u16 = 20;

/// Character checkpoint schema. v1 is accepted only as a migration input.
pub const CHARACTER_RECORD_SCHEMA_V1: u32 = 1;
pub const CHARACTER_RECORD_SCHEMA_VERSION: u32 = 2;

pub const MAP_DROP_SCHEMA_VERSION: u32 = 1;

/// The six equipment slots, spelled like the simulation slot names.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CharacterItemLocation {
    Inventory { slot: u16 },
    Equipped { slot: DurableEquipmentSlot },
}

/// One character-owned item inside a schema v2 record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentItem {
    pub item_instance_id: ItemInstanceId,
    #[serde(with = "content_id_as_u64")]
    pub definition_content_id: ContentId,
    pub quantity: u32,
    pub location: CharacterItemLocation,
}

/// Thousandths of a world unit. Runtime `f32` conversion belongs to 12C.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDropPosition {
    pub x_milli: i32,
    pub y_milli: i32,
}

/// Map-owned Drop. The runtime drop entity is not part of this record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDropRecord {
    pub item_instance_id: ItemInstanceId,
    #[serde(with = "content_id_as_u64")]
    pub definition_content_id: ContentId,
    pub quantity: u32,
    #[serde(with = "content_id_as_u64")]
    pub map_content_id: ContentId,
    pub map_space_key: String,
    pub drop_id: u64,
    pub position: MapDropPosition,
    /// Absolute active-server tick when an unclaimed drop is eligible for deletion.
    pub expiry_tick: u64,
    /// Absolute active-server tick when a restricted drop becomes public.
    /// Unused when `eligible_character_ids` is empty (already public).
    pub public_at_tick: u64,
    /// Empty means any character may pick the drop up. A non-empty list is the
    /// restricted claimant set until `public_at_tick`.
    pub eligible_character_ids: Vec<CharacterId>,
}

impl MapDropRecord {
    #[must_use]
    pub fn remaining_expiry_ticks(&self, clock_tick: u64) -> u64 {
        self.expiry_tick.saturating_sub(clock_tick)
    }

    #[must_use]
    pub fn remaining_restricted_ticks(&self, clock_tick: u64) -> u64 {
        if self.eligible_character_ids.is_empty() {
            0
        } else {
            self.public_at_tick.saturating_sub(clock_tick)
        }
    }
}

/// Facts the content catalog must supply before an item or map id is durable.
///
/// The persistence crate does not load content files. The server (or a test)
/// installs this snapshot. Retired and unknown ids fail closed.
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
    maps: BTreeMap<u32, bool>,
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

    pub fn insert_map(&mut self, content_id: ContentId, retired: bool) -> Result<(), String> {
        let Some(raw) = content_id.raw() else {
            return Err("map content id is outside the numeric catalog".into());
        };
        if content_id.kind() != Some(ContentKind::Map) {
            return Err(format!("content id {raw} is not in the map block"));
        }
        self.maps.insert(raw, retired);
        Ok(())
    }

    #[must_use]
    pub fn item(&self, content_id: ContentId) -> Option<&ItemContentRule> {
        content_id.raw().and_then(|raw| self.items.get(&raw))
    }

    #[must_use]
    pub fn map_retired(&self, content_id: ContentId) -> Option<bool> {
        content_id
            .raw()
            .and_then(|raw| self.maps.get(&raw).copied())
    }
}

/// Complete post-state for one ownership change.
///
/// Each included character replaces that character's committed record. Map
/// drops listed in `drops_upsert` replace that item's map record. Items in
/// `drops_remove` leave the map. The journal checks that every live item id
/// still has exactly one owner before the transaction is acknowledged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnershipChange {
    pub characters: Vec<crate::character::PersistentCharacter>,
    pub drops_upsert: Vec<MapDropRecord>,
    pub drops_remove: Vec<ItemInstanceId>,
}

impl OwnershipChange {
    #[must_use]
    pub fn character(character: crate::character::PersistentCharacter) -> Self {
        Self {
            characters: vec![character],
            drops_upsert: Vec::new(),
            drops_remove: Vec::new(),
        }
    }
}

/// Acknowledged only after the transaction frame is appended and synced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitResult {
    pub transaction_id: u64,
    pub active_clock_tick: u64,
    pub reserved_through: u64,
}

/// Ids `first` inclusive through `first + count - 1`, already in the log.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReservedItemIds {
    pub transaction_id: u64,
    pub first: ItemInstanceId,
    pub count: u32,
}

impl ReservedItemIds {
    #[must_use]
    pub fn contains(self, id: ItemInstanceId) -> bool {
        let start = self.first.raw();
        let end = start.saturating_add(u64::from(self.count));
        (start..end).contains(&id.raw())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockCheckpointKind {
    /// Rejects an advance of more than [`ACTIVE_SERVER_TICKS_PER_SECOND`] ticks.
    Periodic,
    /// Clean shutdown may sync any forward tick.
    Shutdown,
}

pub(crate) fn validate_item_structure(
    item: &PersistentItem,
    path: &std::path::Path,
) -> Result<(), PersistError> {
    if item.item_instance_id.raw() == 0 {
        return Err(PersistError::corrupt(
            path,
            "item_instance_id 0 is reserved",
        ));
    }
    if item.quantity == 0 {
        return Err(PersistError::corrupt(
            path,
            format!("item {} quantity must be positive", item.item_instance_id),
        ));
    }
    if !is_item_block(item.definition_content_id) {
        return Err(PersistError::content(
            path,
            format!(
                "item {} definition {} is outside the item block",
                item.item_instance_id,
                item.definition_content_id.token()
            ),
        ));
    }
    match item.location {
        CharacterItemLocation::Inventory { slot } => {
            if slot >= DURABLE_INVENTORY_CAPACITY {
                return Err(PersistError::corrupt(
                    path,
                    format!("inventory slot {slot} is outside 0..{DURABLE_INVENTORY_CAPACITY}"),
                ));
            }
        }
        CharacterItemLocation::Equipped { .. } => {}
    }
    Ok(())
}

pub(crate) fn validate_character_items(
    items: &[PersistentItem],
    path: &std::path::Path,
) -> Result<(), PersistError> {
    let mut ids = BTreeMap::<u64, ()>::new();
    let mut inventory_slots = BTreeMap::<u16, ()>::new();
    let mut equipped = BTreeMap::<u8, ()>::new();
    for item in items {
        validate_item_structure(item, path)?;
        if ids.insert(item.item_instance_id.raw(), ()).is_some() {
            return Err(PersistError::integrity(
                path,
                format!("duplicate item id {}", item.item_instance_id.raw()),
            ));
        }
        match item.location {
            CharacterItemLocation::Inventory { slot } => {
                if inventory_slots.insert(slot, ()).is_some() {
                    return Err(PersistError::corrupt(
                        path,
                        format!("duplicate inventory slot {slot}"),
                    ));
                }
            }
            CharacterItemLocation::Equipped { slot } => {
                let key = slot_index(slot);
                if equipped.insert(key, ()).is_some() {
                    return Err(PersistError::corrupt(
                        path,
                        format!("duplicate equipment slot {}", slot.as_str()),
                    ));
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_item_content(
    item_instance_id: ItemInstanceId,
    definition: ContentId,
    quantity: u32,
    location: CharacterItemLocation,
    rules: &DurableContentRules,
    path: &std::path::Path,
) -> Result<(), PersistError> {
    let Some(rule) = rules.item(definition) else {
        return Err(PersistError::content(
            path,
            format!(
                "item {} content {} is missing from the installed catalog",
                item_instance_id.raw(),
                definition.token()
            ),
        ));
    };
    if rule.retired {
        return Err(PersistError::content(
            path,
            format!(
                "item {} content {} is retired",
                item_instance_id.raw(),
                definition.token()
            ),
        ));
    }
    if quantity > rule.stack_limit {
        return Err(PersistError::content(
            path,
            format!(
                "item {} quantity {quantity} exceeds stack_limit {}",
                item_instance_id.raw(),
                rule.stack_limit
            ),
        ));
    }
    if let CharacterItemLocation::Equipped { slot } = location {
        match rule.equip_slot {
            Some(authorized) if authorized == slot => {}
            Some(authorized) => {
                return Err(PersistError::content(
                    path,
                    format!(
                        "item {} equip slot {} is not the authorized {} slot",
                        item_instance_id.raw(),
                        slot.as_str(),
                        authorized.as_str()
                    ),
                ));
            }
            None => {
                return Err(PersistError::content(
                    path,
                    format!(
                        "item {} content {} has no equipment facet for {}",
                        item_instance_id.raw(),
                        definition.token(),
                        slot.as_str()
                    ),
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_drop_structure(
    drop: &MapDropRecord,
    path: &std::path::Path,
) -> Result<(), PersistError> {
    validate_item_structure(
        &PersistentItem {
            item_instance_id: drop.item_instance_id,
            definition_content_id: drop.definition_content_id,
            quantity: drop.quantity,
            location: CharacterItemLocation::Inventory { slot: 0 },
        },
        path,
    )?;
    if drop.drop_id == 0 {
        return Err(PersistError::corrupt(path, "drop_id 0 is reserved"));
    }
    if !valid_map_space_key(&drop.map_space_key) {
        return Err(PersistError::corrupt(
            path,
            "map_space_key must be 1..=64 ASCII letters, digits, '.', '_' or '-'",
        ));
    }
    if !is_map_block(drop.map_content_id) {
        return Err(PersistError::content(
            path,
            format!(
                "map content {} is outside the map block",
                drop.map_content_id.token()
            ),
        ));
    }
    if drop.expiry_tick == 0 {
        return Err(PersistError::corrupt(path, "expiry_tick must be positive"));
    }
    if drop.public_at_tick > drop.expiry_tick {
        return Err(PersistError::corrupt(
            path,
            "public_at_tick cannot be after expiry_tick",
        ));
    }
    let mut seen = BTreeMap::<u64, ()>::new();
    for id in &drop.eligible_character_ids {
        if id.raw() == 0 {
            return Err(PersistError::corrupt(
                path,
                "eligible character_id 0 is reserved",
            ));
        }
        if seen.insert(id.raw(), ()).is_some() {
            return Err(PersistError::corrupt(
                path,
                format!("duplicate eligible character {}", id.raw()),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_drop_content(
    drop: &MapDropRecord,
    rules: &DurableContentRules,
    path: &std::path::Path,
) -> Result<(), PersistError> {
    validate_item_content(
        drop.item_instance_id,
        drop.definition_content_id,
        drop.quantity,
        CharacterItemLocation::Inventory { slot: 0 },
        rules,
        path,
    )?;
    match rules.map_retired(drop.map_content_id) {
        None => {
            return Err(PersistError::content(
                path,
                format!(
                    "map content {} is missing from the installed catalog",
                    drop.map_content_id.token()
                ),
            ));
        }
        Some(true) => {
            return Err(PersistError::content(
                path,
                format!("map content {} is retired", drop.map_content_id.token()),
            ));
        }
        Some(false) => {}
    }
    Ok(())
}

pub(crate) fn validate_new_drop_timing(
    drop: &MapDropRecord,
    clock_tick: u64,
    path: &std::path::Path,
) -> Result<(), PersistError> {
    if drop.expiry_tick <= clock_tick {
        return Err(PersistError::corrupt(
            path,
            format!(
                "expiry_tick {} is not after the active clock {clock_tick}",
                drop.expiry_tick
            ),
        ));
    }
    if !drop.eligible_character_ids.is_empty() && drop.public_at_tick < clock_tick {
        return Err(PersistError::corrupt(
            path,
            "restricted eligibility cannot become public before the active clock",
        ));
    }
    Ok(())
}

fn slot_index(slot: DurableEquipmentSlot) -> u8 {
    match slot {
        DurableEquipmentSlot::Headwear => 0,
        DurableEquipmentSlot::Bodywear => 1,
        DurableEquipmentSlot::Pants => 2,
        DurableEquipmentSlot::Gloves => 3,
        DurableEquipmentSlot::Boots => 4,
        DurableEquipmentSlot::Weapon => 5,
    }
}

fn is_item_block(id: ContentId) -> bool {
    matches!(id.raw(), Some(raw) if (CONTENT_ITEM_START..=CONTENT_ITEM_END).contains(&raw))
}

fn is_map_block(id: ContentId) -> bool {
    matches!(id.raw(), Some(raw) if (CONTENT_MAP_START..=CONTENT_MAP_END).contains(&raw))
}

fn valid_map_space_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub(crate) fn sort_character_items(items: &mut [PersistentItem]) {
    items.sort_by_key(|item| item.item_instance_id.raw());
}

pub(crate) fn sort_drop(drop: &mut MapDropRecord) {
    drop.eligible_character_ids.sort_by_key(|id| id.raw());
}

mod content_id_as_u64 {
    use purgatory_common::ContentId;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(id: &ContentId, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(id.token())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<ContentId, D::Error> {
        let token = u64::deserialize(deserializer)?;
        Ok(ContentId::from_token(token))
    }
}
