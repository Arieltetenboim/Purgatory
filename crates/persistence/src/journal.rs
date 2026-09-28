//! File-backed write-ahead log and derived checkpoints.
//!
//! The log is the authority for ownership, item-id reservation and the active
//! clock. Character files, `map_drops.json` and `durable_manifest.json` are
//! checkpoints rewritten after a synced frame. Paths, framing and `sync_all`
//! stay in this module.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use purgatory_common::{CharacterId, InstanceExitContext, ItemInstanceId, RestoreIntent};
use serde::{Deserialize, Serialize};

use crate::atomic::{self, replace_file_recoverable};
use crate::character::PersistentCharacter;
use crate::domain::{
    self, ACTIVE_SERVER_TICKS_PER_SECOND, CHARACTER_RECORD_SCHEMA_V1, ClockCheckpointKind,
    CommitResult, DURABLE_DOMAIN_VERSION, DURABLE_MANIFEST_SCHEMA_VERSION, DurableContentRules,
    MAP_DROP_SCHEMA_VERSION, MapDropRecord, OwnershipChange, ReservedItemIds,
};
use crate::error::PersistError;

const WAL_NAME: &str = "ownership.wal";
const MANIFEST_NAME: &str = "durable_manifest.json";
const DROPS_NAME: &str = "map_drops.json";
const WAL_MAGIC: &[u8; 8] = b"PGWAL001";
const HEADER_LEN: usize = 20;
const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;
const MAX_RECORD_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct LiveState {
    pub characters: BTreeMap<u64, PersistentCharacter>,
    pub drops: BTreeMap<u64, MapDropRecord>,
    pub clock_tick: u64,
    pub reserved_through: u64,
    pub last_transaction_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V1Character {
    pub character_id: CharacterId,
    pub persistence_revision: u64,
    pub restore: RestoreIntent,
    pub instance_exit: Option<InstanceExitContext>,
    pub original_bytes: Vec<u8>,
}

struct Survey {
    live: LiveState,
    pending_v1: Vec<V1Character>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TxnRecord {
    domain_version: u32,
    transaction_id: u64,
    characters: Vec<PersistentCharacter>,
    drops_upsert: Vec<MapDropRecord>,
    drops_remove: Vec<ItemInstanceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reserved_through: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clock_tick: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    schema_version: u32,
    domain_version: u32,
    applied_transaction_id: u64,
    clock_tick: u64,
    reserved_through: u64,
    checkpoint_crc: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapDropFile {
    schema_version: u32,
    applied_transaction_id: u64,
    drops: Vec<MapDropRecord>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterFileV1 {
    schema_version: u32,
    character_id: CharacterId,
    persistence_revision: u64,
    restore: RestoreIntent,
    #[serde(default)]
    instance_exit: Option<InstanceExitContext>,
}

#[derive(Deserialize)]
struct SchemaProbe {
    schema_version: u32,
}

struct WalScan {
    first_transaction_id: u64,
    txns: Vec<TxnRecord>,
    torn_tail: bool,
    good_len: u64,
}

enum DiskCharacter {
    V1(V1Character),
    V2(PersistentCharacter),
}

pub(crate) fn recover(dir: &Path) -> Result<LiveState, PersistError> {
    Ok(survey(dir)?.live)
}

pub(crate) fn pending_migrations(dir: &Path) -> Result<Vec<V1Character>, PersistError> {
    Ok(survey(dir)?.pending_v1)
}

pub(crate) fn migrate_one(dir: &Path, v1: &V1Character) -> Result<CommitResult, PersistError> {
    let next_revision =
        v1.persistence_revision
            .checked_add(1)
            .ok_or_else(|| PersistError::Migration {
                path: character_path(dir, v1.character_id),
                reason: "persistence_revision overflow".into(),
            })?;
    let character = PersistentCharacter {
        schema_version: crate::PERSISTENCE_SCHEMA_VERSION,
        applied_transaction_id: 0,
        character_id: v1.character_id,
        persistence_revision: next_revision,
        restore: v1.restore.clone(),
        instance_exit: v1.instance_exit.clone(),
        items: Vec::new(),
    };
    character.validate(&character_path(dir, v1.character_id))?;
    commit_txn(
        dir,
        &DurableContentRules::new(),
        OwnershipChange::character(character),
        None,
        None,
    )
}

pub(crate) fn commit_ownership(
    dir: &Path,
    rules: &DurableContentRules,
    change: OwnershipChange,
) -> Result<CommitResult, PersistError> {
    commit_txn(dir, rules, change, None, None)
}

pub(crate) fn reserve_ids(dir: &Path, count: u32) -> Result<ReservedItemIds, PersistError> {
    if count == 0 {
        return Err(PersistError::corrupt(dir, "item id reservation count is 0"));
    }
    let live = recover(dir)?;
    let start = live
        .reserved_through
        .checked_add(1)
        .ok_or(PersistError::ItemIdsExhausted)?;
    let through = live
        .reserved_through
        .checked_add(u64::from(count))
        .ok_or(PersistError::ItemIdsExhausted)?;
    let committed = commit_txn(
        dir,
        &DurableContentRules::new(),
        OwnershipChange {
            characters: Vec::new(),
            drops_upsert: Vec::new(),
            drops_remove: Vec::new(),
        },
        Some(through),
        None,
    )?;
    Ok(ReservedItemIds {
        transaction_id: committed.transaction_id,
        first: ItemInstanceId::from_raw(start),
        count,
    })
}

pub(crate) fn checkpoint_clock(
    dir: &Path,
    tick: u64,
    kind: ClockCheckpointKind,
) -> Result<CommitResult, PersistError> {
    let live = recover(dir)?;
    if tick < live.clock_tick {
        return Err(PersistError::integrity(
            dir,
            format!(
                "active clock moved backward from {} to {tick}",
                live.clock_tick
            ),
        ));
    }
    if tick == live.clock_tick {
        return Ok(CommitResult {
            transaction_id: live.last_transaction_id,
            active_clock_tick: live.clock_tick,
            reserved_through: live.reserved_through,
        });
    }
    if kind == ClockCheckpointKind::Periodic
        && tick - live.clock_tick > ACTIVE_SERVER_TICKS_PER_SECOND
    {
        return Err(PersistError::ClockBound {
            committed_tick: live.clock_tick,
            requested_tick: tick,
        });
    }
    commit_txn(
        dir,
        &DurableContentRules::new(),
        OwnershipChange {
            characters: Vec::new(),
            drops_upsert: Vec::new(),
            drops_remove: Vec::new(),
        },
        None,
        Some(tick),
    )
}

pub(crate) fn compact(dir: &Path) -> Result<(), PersistError> {
    let survey = survey(dir)?;
    if !survey.pending_v1.is_empty() {
        return Err(PersistError::integrity(
            dir,
            "cannot compact while a v1 character is unmigrated",
        ));
    }
    write_checkpoints(dir, &survey.live)?;
    let first = survey
        .live
        .last_transaction_id
        .checked_add(1)
        .ok_or(PersistError::ItemIdsExhausted)?;
    install_compacted_log(dir, first)?;
    Ok(())
}

fn commit_txn(
    dir: &Path,
    rules: &DurableContentRules,
    change: OwnershipChange,
    reserved_through: Option<u64>,
    clock_tick: Option<u64>,
) -> Result<CommitResult, PersistError> {
    let survey = survey(dir)?;
    let txn = prepare_txn(
        &survey.live,
        rules,
        change,
        reserved_through,
        clock_tick,
        dir,
    )?;
    append_synced_frame(dir, &txn)?;
    let mut live = survey.live;
    apply_txn(&mut live, &txn, dir)?;
    // The frame is already a commit. A checkpoint error must not roll it back;
    // the next recover replays it. Surface the checkpoint error only when the
    // frame itself did not sync (append_synced_frame).
    write_checkpoints(dir, &live)?;
    Ok(CommitResult {
        transaction_id: live.last_transaction_id,
        active_clock_tick: live.clock_tick,
        reserved_through: live.reserved_through,
    })
}

fn prepare_txn(
    live: &LiveState,
    rules: &DurableContentRules,
    mut change: OwnershipChange,
    reserved_through: Option<u64>,
    clock_tick: Option<u64>,
    dir: &Path,
) -> Result<TxnRecord, PersistError> {
    let transaction_id = live
        .last_transaction_id
        .checked_add(1)
        .ok_or_else(|| PersistError::integrity(dir, "transaction id overflow"))?;
    if let Some(reserved) = reserved_through
        && reserved < live.reserved_through
    {
        return Err(PersistError::integrity(
            dir,
            "item id high water moved backward",
        ));
    }
    if let Some(tick) = clock_tick
        && tick < live.clock_tick
    {
        return Err(PersistError::integrity(dir, "active clock moved backward"));
    }
    let mut projected = live.clone();
    if let Some(reserved) = reserved_through {
        projected.reserved_through = reserved;
    }
    if let Some(tick) = clock_tick {
        projected.clock_tick = tick;
    }
    for id in &change.drops_remove {
        if !projected.drops.contains_key(&id.raw()) {
            return Err(PersistError::corrupt(
                dir,
                format!("removed drop {} is not on a map", id.raw()),
            ));
        }
        projected.drops.remove(&id.raw());
    }
    for drop in &mut change.drops_upsert {
        domain::sort_drop(drop);
        let path = dir.join(DROPS_NAME);
        domain::validate_drop_structure(drop, &path)?;
        domain::validate_drop_content(drop, rules, &path)?;
        let previous = projected.drops.get(&drop.item_instance_id.raw());
        if previous.is_none() {
            domain::validate_new_drop_timing(drop, projected.clock_tick, &path)?;
        }
        projected
            .drops
            .insert(drop.item_instance_id.raw(), drop.clone());
    }
    for character in &mut change.characters {
        let path = character_path(dir, character.character_id);
        domain::sort_character_items(&mut character.items);
        character.schema_version = crate::PERSISTENCE_SCHEMA_VERSION;
        character.validate(&path)?;
        for item in &character.items {
            domain::validate_item_content(
                item.item_instance_id,
                item.definition_content_id,
                item.quantity,
                item.location,
                rules,
                &path,
            )?;
        }
        if let Some(existing) = projected.characters.get(&character.character_id.raw())
            && character.persistence_revision <= existing.persistence_revision
        {
            return Err(PersistError::integrity(
                &path,
                format!(
                    "revision {} does not advance {}",
                    character.persistence_revision, existing.persistence_revision
                ),
            ));
        }
        projected
            .characters
            .insert(character.character_id.raw(), character.clone());
    }
    check_owners(&projected, dir, Some(live))?;
    for character in &mut change.characters {
        character.applied_transaction_id = transaction_id;
    }
    change
        .characters
        .sort_by_key(|character| character.character_id.raw());
    change
        .drops_upsert
        .sort_by_key(|drop| drop.item_instance_id.raw());
    change.drops_remove.sort_by_key(|id| id.raw());
    Ok(TxnRecord {
        domain_version: DURABLE_DOMAIN_VERSION,
        transaction_id,
        characters: change.characters,
        drops_upsert: change.drops_upsert,
        drops_remove: change.drops_remove,
        reserved_through,
        clock_tick,
    })
}

fn survey(dir: &Path) -> Result<Survey, PersistError> {
    let disk_characters = read_character_files(dir)?;
    let manifest = read_manifest(dir)?;
    let drop_file = read_drop_file(dir)?;
    let wal_path = dir.join(WAL_NAME);
    let mut scan = scan_wal(&wal_path)?;
    if scan.torn_tail {
        repair_torn_tail(&wal_path, scan.good_len, scan.first_transaction_id)?;
        scan = scan_wal(&wal_path)?;
        if scan.torn_tail {
            return Err(PersistError::integrity(
                &wal_path,
                "torn transaction tail remained after repair",
            ));
        }
    }
    check_contiguous(&scan, &wal_path)?;
    if scan.first_transaction_id == 1 {
        survey_genesis(dir, &scan, disk_characters, manifest, drop_file)
    } else {
        survey_compacted(dir, &scan, disk_characters, manifest, drop_file)
    }
}

fn survey_genesis(
    dir: &Path,
    scan: &WalScan,
    disk_characters: Vec<DiskCharacter>,
    manifest: Option<ManifestFile>,
    drop_file: Option<MapDropFile>,
) -> Result<Survey, PersistError> {
    let wal_path = dir.join(WAL_NAME);
    let (snapshots, live) = replay_all(&scan.txns, &wal_path)?;
    let mut pending_v1 = Vec::new();
    for character in &disk_characters {
        match character {
            DiskCharacter::V1(v1) => classify_v1(dir, v1, scan, &mut pending_v1)?,
            DiskCharacter::V2(v2) => {
                let snapshot = snapshots.get(&v2.applied_transaction_id).ok_or_else(|| {
                    PersistError::integrity(
                        character_path(dir, v2.character_id),
                        format!(
                            "checkpoint transaction {} is not in the log",
                            v2.applied_transaction_id
                        ),
                    )
                })?;
                let expected =
                    snapshot
                        .characters
                        .get(&v2.character_id.raw())
                        .ok_or_else(|| {
                            PersistError::integrity(
                                character_path(dir, v2.character_id),
                                "checkpoint character is absent from that transaction",
                            )
                        })?;
                if !v2.body_eq(expected) {
                    return Err(PersistError::integrity(
                        character_path(dir, v2.character_id),
                        "character checkpoint does not match the log",
                    ));
                }
            }
        }
    }
    if let Some(drops) = &drop_file {
        let snapshot = snapshots
            .get(&drops.applied_transaction_id)
            .ok_or_else(|| {
                PersistError::integrity(
                    dir.join(DROPS_NAME),
                    "map-drop checkpoint is not in the log",
                )
            })?;
        if drop_map(&drops.drops) != snapshot.drops {
            return Err(PersistError::integrity(
                dir.join(DROPS_NAME),
                "map-drop checkpoint does not match the log",
            ));
        }
    }
    if let Some(manifest) = &manifest {
        let snapshot = snapshots
            .get(&manifest.applied_transaction_id)
            .ok_or_else(|| {
                PersistError::integrity(
                    dir.join(MANIFEST_NAME),
                    "manifest transaction is not in the log",
                )
            })?;
        if manifest.clock_tick != snapshot.clock_tick
            || manifest.reserved_through != snapshot.reserved_through
            || manifest.checkpoint_crc != fingerprint(snapshot)
        {
            return Err(PersistError::integrity(
                dir.join(MANIFEST_NAME),
                "manifest does not match the log at its applied transaction",
            ));
        }
    }
    if pending_v1.is_empty() && !checkpoints_current(&disk_characters, &drop_file, &manifest, &live)
    {
        write_checkpoints(dir, &live)?;
    }
    Ok(Survey { live, pending_v1 })
}

fn classify_v1(
    dir: &Path,
    v1: &V1Character,
    scan: &WalScan,
    pending: &mut Vec<V1Character>,
) -> Result<(), PersistError> {
    let Some(first) = scan.txns.iter().find(|txn| {
        txn.characters
            .iter()
            .any(|character| character.character_id == v1.character_id)
    }) else {
        pending.push(v1.clone());
        return Ok(());
    };
    let post = first
        .characters
        .iter()
        .find(|character| character.character_id == v1.character_id)
        .expect("find returned this transaction");
    let expected_revision =
        v1.persistence_revision
            .checked_add(1)
            .ok_or_else(|| PersistError::Migration {
                path: character_path(dir, v1.character_id),
                reason: "persistence_revision overflow".into(),
            })?;
    let image = post.items.is_empty()
        && post.persistence_revision == expected_revision
        && post.restore == v1.restore
        && post.instance_exit == v1.instance_exit;
    if !image {
        return Err(PersistError::integrity(
            character_path(dir, v1.character_id),
            format!(
                "v1 character {} does not match its first committed transaction",
                v1.character_id
            ),
        ));
    }
    Ok(())
}

fn survey_compacted(
    dir: &Path,
    scan: &WalScan,
    disk_characters: Vec<DiskCharacter>,
    manifest: Option<ManifestFile>,
    drop_file: Option<MapDropFile>,
) -> Result<Survey, PersistError> {
    let manifest = manifest.ok_or_else(|| {
        PersistError::integrity(
            dir.join(MANIFEST_NAME),
            "compacted log requires a synced manifest",
        )
    })?;
    if scan.first_transaction_id > manifest.applied_transaction_id.saturating_add(1) {
        return Err(PersistError::integrity(
            dir.join(WAL_NAME),
            "compacted log skipped transactions after the manifest",
        ));
    }
    let drops = drop_file.ok_or_else(|| {
        PersistError::integrity(
            dir.join(DROPS_NAME),
            "compacted log requires a map-drop checkpoint",
        )
    })?;
    if drops.applied_transaction_id != manifest.applied_transaction_id {
        return Err(PersistError::integrity(
            dir.join(DROPS_NAME),
            "map-drop checkpoint does not match the manifest",
        ));
    }
    let mut live = LiveState {
        characters: BTreeMap::new(),
        drops: drop_map(&drops.drops),
        clock_tick: manifest.clock_tick,
        reserved_through: manifest.reserved_through,
        last_transaction_id: manifest.applied_transaction_id,
    };
    for character in disk_characters {
        match character {
            DiskCharacter::V1(v1) => {
                return Err(PersistError::integrity(
                    character_path(dir, v1.character_id),
                    "v1 character remains after log compaction",
                ));
            }
            DiskCharacter::V2(v2) => {
                if v2.applied_transaction_id != manifest.applied_transaction_id {
                    return Err(PersistError::integrity(
                        character_path(dir, v2.character_id),
                        "character checkpoint does not match the manifest",
                    ));
                }
                live.characters.insert(v2.character_id.raw(), v2);
            }
        }
    }
    if fingerprint(&live) != manifest.checkpoint_crc {
        return Err(PersistError::integrity(
            dir.join(MANIFEST_NAME),
            "checkpoint bytes do not match the manifest fingerprint",
        ));
    }
    check_owners(&live, dir, None)?;
    for txn in &scan.txns {
        if txn.transaction_id <= manifest.applied_transaction_id {
            continue;
        }
        apply_txn(&mut live, txn, dir)?;
    }
    if !checkpoints_current(
        &read_character_files(dir)?,
        &Some(drops),
        &Some(manifest),
        &live,
    ) {
        write_checkpoints(dir, &live)?;
    }
    Ok(Survey {
        live,
        pending_v1: Vec::new(),
    })
}

fn checkpoints_current(
    characters: &[DiskCharacter],
    drops: &Option<MapDropFile>,
    manifest: &Option<ManifestFile>,
    live: &LiveState,
) -> bool {
    if live.last_transaction_id == 0 {
        return characters.is_empty() && drops.is_none() && manifest.is_none();
    }
    let Some(manifest) = manifest else {
        return false;
    };
    if manifest.applied_transaction_id != live.last_transaction_id
        || manifest.clock_tick != live.clock_tick
        || manifest.reserved_through != live.reserved_through
        || manifest.checkpoint_crc != fingerprint(live)
    {
        return false;
    }
    let Some(drops) = drops else {
        return false;
    };
    if drops.applied_transaction_id != live.last_transaction_id
        || drop_map(&drops.drops) != live.drops
    {
        return false;
    }
    if characters.len() != live.characters.len() {
        return false;
    }
    for character in characters {
        let DiskCharacter::V2(v2) = character else {
            return false;
        };
        let Some(expected) = live.characters.get(&v2.character_id.raw()) else {
            return false;
        };
        if v2.applied_transaction_id != live.last_transaction_id || !v2.body_eq(expected) {
            return false;
        }
    }
    true
}

fn replay_all(
    txns: &[TxnRecord],
    path: &Path,
) -> Result<(BTreeMap<u64, LiveState>, LiveState), PersistError> {
    let mut snapshots = BTreeMap::new();
    let mut live = LiveState::default();
    snapshots.insert(0, live.clone());
    for txn in txns {
        apply_txn(&mut live, txn, path)?;
        snapshots.insert(txn.transaction_id, live.clone());
    }
    Ok((snapshots, live))
}

fn apply_txn(state: &mut LiveState, txn: &TxnRecord, path: &Path) -> Result<(), PersistError> {
    if txn.domain_version != DURABLE_DOMAIN_VERSION {
        return Err(PersistError::schema(path, txn.domain_version));
    }
    if txn.transaction_id <= state.last_transaction_id {
        return Ok(());
    }
    if txn.transaction_id != state.last_transaction_id + 1 {
        return Err(PersistError::integrity(
            path,
            format!(
                "transaction {} is not the next id after {}",
                txn.transaction_id, state.last_transaction_id
            ),
        ));
    }
    let before = locate_items(state, path)?;
    if let Some(reserved) = txn.reserved_through {
        if reserved < state.reserved_through {
            return Err(PersistError::integrity(
                path,
                "item id high water moved backward",
            ));
        }
        state.reserved_through = reserved;
    }
    if let Some(tick) = txn.clock_tick {
        if tick < state.clock_tick {
            return Err(PersistError::integrity(path, "active clock moved backward"));
        }
        state.clock_tick = tick;
    }
    for id in &txn.drops_remove {
        state.drops.remove(&id.raw());
    }
    for drop in &txn.drops_upsert {
        state
            .drops
            .insert(drop.item_instance_id.raw(), drop.clone());
    }
    for character in &txn.characters {
        state
            .characters
            .insert(character.character_id.raw(), character.clone());
    }
    state.last_transaction_id = txn.transaction_id;
    check_owners(state, path, None)?;
    let after = locate_items(state, path)?;
    for (id, item) in &after {
        if item.raw > state.reserved_through {
            return Err(PersistError::integrity(
                path,
                format!("item {id} was not reserved before use"),
            ));
        }
        if let Some(previous) = before.get(id) {
            if previous.definition != item.definition {
                return Err(PersistError::integrity(
                    path,
                    format!("item {id} definition changed"),
                ));
            }
            if previous.owner != item.owner && previous.quantity != item.quantity {
                return Err(PersistError::integrity(
                    path,
                    format!("item {id} changed quantity while changing owner"),
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct LocatedItem {
    raw: u64,
    definition: u64,
    quantity: u32,
    owner: Owner,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Owner {
    Character(u64),
    Map,
}

fn locate_items(
    state: &LiveState,
    path: &Path,
) -> Result<BTreeMap<u64, LocatedItem>, PersistError> {
    let mut items = BTreeMap::new();
    for character in state.characters.values() {
        for item in &character.items {
            let located = LocatedItem {
                raw: item.item_instance_id.raw(),
                definition: item.definition_content_id.token(),
                quantity: item.quantity,
                owner: Owner::Character(character.character_id.raw()),
            };
            if items.insert(item.item_instance_id.raw(), located).is_some() {
                return Err(PersistError::integrity(
                    path,
                    format!("duplicate item id {}", item.item_instance_id.raw()),
                ));
            }
        }
    }
    for drop in state.drops.values() {
        let located = LocatedItem {
            raw: drop.item_instance_id.raw(),
            definition: drop.definition_content_id.token(),
            quantity: drop.quantity,
            owner: Owner::Map,
        };
        if items.insert(drop.item_instance_id.raw(), located).is_some() {
            return Err(PersistError::integrity(
                path,
                format!("duplicate item id {}", drop.item_instance_id.raw()),
            ));
        }
    }
    Ok(items)
}

fn check_owners(
    state: &LiveState,
    path: &Path,
    previous: Option<&LiveState>,
) -> Result<(), PersistError> {
    let current = locate_items(state, path)?;
    if let Some(previous) = previous {
        let before = locate_items(previous, path)?;
        for (id, item) in &current {
            if item.raw > state.reserved_through {
                return Err(PersistError::integrity(
                    path,
                    format!("item {id} was not reserved before use"),
                ));
            }
            if let Some(earlier) = before.get(id)
                && earlier.definition != item.definition
            {
                return Err(PersistError::integrity(
                    path,
                    format!("item {id} definition changed"),
                ));
            }
            if let Some(earlier) = before.get(id)
                && earlier.owner != item.owner
                && earlier.quantity != item.quantity
            {
                return Err(PersistError::integrity(
                    path,
                    format!("item {id} changed quantity while changing owner"),
                ));
            }
        }
    }
    let mut drop_ids = BTreeMap::<u64, ()>::new();
    for drop in state.drops.values() {
        if drop_ids.insert(drop.drop_id, ()).is_some() {
            return Err(PersistError::integrity(
                path,
                format!("duplicate drop_id {}", drop.drop_id),
            ));
        }
    }
    let _ = current;
    Ok(())
}

fn write_checkpoints(dir: &Path, live: &LiveState) -> Result<(), PersistError> {
    if live.last_transaction_id == 0 {
        return Ok(());
    }
    for character in live.characters.values() {
        let mut out = character.clone();
        out.schema_version = crate::PERSISTENCE_SCHEMA_VERSION;
        out.applied_transaction_id = live.last_transaction_id;
        domain::sort_character_items(&mut out.items);
        let path = character_path(dir, out.character_id);
        let bytes =
            serde_json::to_vec_pretty(&out).map_err(|err| PersistError::json(&path, err))?;
        replace_file_recoverable(&path, &bytes).map_err(|err| PersistError::io(&path, err))?;
    }
    let mut drops: Vec<_> = live.drops.values().cloned().collect();
    drops.sort_by_key(|drop| drop.item_instance_id.raw());
    let drop_file = MapDropFile {
        schema_version: MAP_DROP_SCHEMA_VERSION,
        applied_transaction_id: live.last_transaction_id,
        drops,
    };
    let drop_path = dir.join(DROPS_NAME);
    let drop_bytes =
        serde_json::to_vec_pretty(&drop_file).map_err(|err| PersistError::json(&drop_path, err))?;
    replace_file_recoverable(&drop_path, &drop_bytes)
        .map_err(|err| PersistError::io(&drop_path, err))?;
    let manifest = ManifestFile {
        schema_version: DURABLE_MANIFEST_SCHEMA_VERSION,
        domain_version: DURABLE_DOMAIN_VERSION,
        applied_transaction_id: live.last_transaction_id,
        clock_tick: live.clock_tick,
        reserved_through: live.reserved_through,
        checkpoint_crc: fingerprint(live),
    };
    let manifest_path = dir.join(MANIFEST_NAME);
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|err| PersistError::json(&manifest_path, err))?;
    replace_file_recoverable(&manifest_path, &manifest_bytes)
        .map_err(|err| PersistError::io(&manifest_path, err))?;
    atomic::sync_directory(dir).map_err(|err| PersistError::io(dir, err))?;
    Ok(())
}

fn fingerprint(state: &LiveState) -> u32 {
    let mut stamped = state.clone();
    for character in stamped.characters.values_mut() {
        character.applied_transaction_id = stamped.last_transaction_id;
        domain::sort_character_items(&mut character.items);
    }
    for drop in stamped.drops.values_mut() {
        domain::sort_drop(drop);
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&stamped.last_transaction_id.to_le_bytes());
    bytes.extend_from_slice(&stamped.clock_tick.to_le_bytes());
    bytes.extend_from_slice(&stamped.reserved_through.to_le_bytes());
    for character in stamped.characters.values() {
        let json = serde_json::to_vec(character).expect("character checkpoint fingerprint");
        bytes.extend_from_slice(&(json.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&json);
    }
    for drop in stamped.drops.values() {
        let json = serde_json::to_vec(drop).expect("drop checkpoint fingerprint");
        bytes.extend_from_slice(&(json.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&json);
    }
    crc32(&bytes)
}

fn append_synced_frame(dir: &Path, txn: &TxnRecord) -> Result<(), PersistError> {
    let path = dir.join(WAL_NAME);
    let payload = serde_json::to_vec(txn).map_err(|err| PersistError::json(&path, err))?;
    if payload.len() > MAX_FRAME_LEN {
        return Err(PersistError::corrupt(
            &path,
            "transaction frame is too large",
        ));
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|err| PersistError::io(&path, err))?;
    let len = file
        .metadata()
        .map_err(|err| PersistError::io(&path, err))?
        .len();
    if len == 0 {
        file.write_all(&header_bytes(1))
            .map_err(|err| PersistError::io(&path, err))?;
    } else {
        file.seek(SeekFrom::End(0))
            .map_err(|err| PersistError::io(&path, err))?;
    }
    file.write_all(&frame_bytes(&payload))
        .map_err(|err| PersistError::io(&path, err))?;
    file.sync_all()
        .map_err(|err| PersistError::io(&path, err))?;
    atomic::sync_directory(dir).map_err(|err| PersistError::io(dir, err))?;
    Ok(())
}

fn install_compacted_log(dir: &Path, first_transaction_id: u64) -> Result<(), PersistError> {
    let wal = dir.join(WAL_NAME);
    let staged = dir.join("ownership.wal.compact");
    let backup = dir.join("ownership.wal.bak");
    {
        let mut file = File::create(&staged).map_err(|err| PersistError::io(&staged, err))?;
        file.write_all(&header_bytes(first_transaction_id))
            .map_err(|err| PersistError::io(&staged, err))?;
        file.sync_all()
            .map_err(|err| PersistError::io(&staged, err))?;
    }
    atomic::sync_directory(dir).map_err(|err| PersistError::io(dir, err))?;
    if wal.exists() {
        let _ = fs::remove_file(&backup);
        fs::rename(&wal, &backup).map_err(|err| PersistError::io(&wal, err))?;
    }
    fs::rename(&staged, &wal).map_err(|err| PersistError::io(&wal, err))?;
    atomic::sync_directory(dir).map_err(|err| PersistError::io(dir, err))?;
    let _ = fs::remove_file(&backup);
    Ok(())
}

fn header_bytes(first_transaction_id: u64) -> [u8; HEADER_LEN] {
    let mut bytes = [0u8; HEADER_LEN];
    bytes[..8].copy_from_slice(WAL_MAGIC);
    bytes[8..12].copy_from_slice(&DURABLE_DOMAIN_VERSION.to_le_bytes());
    bytes[12..20].copy_from_slice(&first_transaction_id.to_le_bytes());
    bytes
}

fn frame_bytes(payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + payload.len());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes.extend_from_slice(&crc32(payload).to_le_bytes());
    bytes
}

fn scan_wal(path: &Path) -> Result<WalScan, PersistError> {
    if !path.exists() {
        return Ok(WalScan {
            first_transaction_id: 1,
            txns: Vec::new(),
            torn_tail: false,
            good_len: 0,
        });
    }
    if path.is_dir() {
        return Err(PersistError::io(
            path,
            std::io::Error::other("transaction log is a directory"),
        ));
    }
    let bytes = fs::read(path).map_err(|err| PersistError::io(path, err))?;
    if bytes.is_empty() {
        return Ok(WalScan {
            first_transaction_id: 1,
            txns: Vec::new(),
            torn_tail: true,
            good_len: 0,
        });
    }
    if bytes.len() < HEADER_LEN {
        return Ok(WalScan {
            first_transaction_id: 1,
            txns: Vec::new(),
            torn_tail: true,
            good_len: 0,
        });
    }
    if &bytes[..8] != WAL_MAGIC {
        return Err(PersistError::integrity(
            path,
            "transaction log magic mismatch",
        ));
    }
    let domain = u32::from_le_bytes(bytes[8..12].try_into().expect("4 bytes"));
    if domain != DURABLE_DOMAIN_VERSION {
        return Err(PersistError::schema(path, domain));
    }
    let first = u64::from_le_bytes(bytes[12..20].try_into().expect("8 bytes"));
    if first == 0 {
        return Err(PersistError::integrity(
            path,
            "transaction log first id is 0",
        ));
    }
    let mut offset = HEADER_LEN;
    let mut txns = Vec::new();
    let mut good_len = HEADER_LEN as u64;
    while offset < bytes.len() {
        let remaining = bytes.len() - offset;
        if remaining < 4 {
            return Ok(WalScan {
                first_transaction_id: first,
                txns,
                torn_tail: true,
                good_len,
            });
        }
        let len =
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("4 bytes")) as usize;
        let Some(frame_len) = 4usize
            .checked_add(len)
            .and_then(|value| value.checked_add(4))
        else {
            return Ok(WalScan {
                first_transaction_id: first,
                txns,
                torn_tail: true,
                good_len,
            });
        };
        if len == 0 || len > MAX_FRAME_LEN {
            if remaining < frame_len {
                return Ok(WalScan {
                    first_transaction_id: first,
                    txns,
                    torn_tail: true,
                    good_len,
                });
            }
            return Err(PersistError::integrity(
                path,
                "transaction frame length is not a committed record",
            ));
        }
        if remaining < frame_len {
            return Ok(WalScan {
                first_transaction_id: first,
                txns,
                torn_tail: true,
                good_len,
            });
        }
        let payload = &bytes[offset + 4..offset + 4 + len];
        let stored = u32::from_le_bytes(
            bytes[offset + 4 + len..offset + frame_len]
                .try_into()
                .expect("4 bytes"),
        );
        if stored != crc32(payload) {
            if offset + frame_len == bytes.len() {
                return Ok(WalScan {
                    first_transaction_id: first,
                    txns,
                    torn_tail: true,
                    good_len,
                });
            }
            return Err(PersistError::integrity(
                path,
                "transaction checksum failed before the end of the log",
            ));
        }
        let txn: TxnRecord = serde_json::from_slice(payload).map_err(|err| {
            PersistError::integrity(path, format!("committed transaction did not parse: {err}"))
        })?;
        txns.push(txn);
        offset += frame_len;
        good_len = offset as u64;
    }
    Ok(WalScan {
        first_transaction_id: first,
        txns,
        torn_tail: false,
        good_len,
    })
}

fn check_contiguous(scan: &WalScan, path: &Path) -> Result<(), PersistError> {
    for (index, txn) in scan.txns.iter().enumerate() {
        let expected = scan
            .first_transaction_id
            .checked_add(index as u64)
            .ok_or_else(|| PersistError::integrity(path, "transaction id overflow"))?;
        if txn.transaction_id != expected {
            return Err(PersistError::integrity(
                path,
                format!(
                    "transaction {} is out of order; expected {expected}",
                    txn.transaction_id
                ),
            ));
        }
    }
    Ok(())
}

fn repair_torn_tail(
    path: &Path,
    good_len: u64,
    first_transaction_id: u64,
) -> Result<(), PersistError> {
    if good_len == 0 {
        let mut file = File::create(path).map_err(|err| PersistError::io(path, err))?;
        file.write_all(&header_bytes(first_transaction_id.max(1)))
            .map_err(|err| PersistError::io(path, err))?;
        file.sync_all().map_err(|err| PersistError::io(path, err))?;
        return Ok(());
    }
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|err| PersistError::io(path, err))?;
    file.set_len(good_len)
        .map_err(|err| PersistError::io(path, err))?;
    file.sync_all().map_err(|err| PersistError::io(path, err))?;
    Ok(())
}

fn read_character_files(dir: &Path) -> Result<Vec<DiskCharacter>, PersistError> {
    let mut paths = Vec::new();
    let entries = fs::read_dir(dir).map_err(|err| PersistError::io(dir, err))?;
    for entry in entries {
        let entry = entry.map_err(|err| PersistError::io(dir, err))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with("char_") && name.ends_with(".json") {
            paths.push(entry.path());
        }
    }
    paths.sort();
    let mut characters = Vec::new();
    for path in paths {
        atomic::recover_if_needed(&path).map_err(|err| PersistError::io(&path, err))?;
        if !path.exists() {
            continue;
        }
        characters.push(parse_character_file(&path)?);
    }
    Ok(characters)
}

fn parse_character_file(path: &Path) -> Result<DiskCharacter, PersistError> {
    let bytes = read_limited(path)?;
    let probe: SchemaProbe =
        serde_json::from_slice(&bytes).map_err(|err| PersistError::json(path, err))?;
    match probe.schema_version {
        CHARACTER_RECORD_SCHEMA_V1 => {
            let parsed: CharacterFileV1 =
                serde_json::from_slice(&bytes).map_err(|err| PersistError::json(path, err))?;
            if parsed.schema_version != CHARACTER_RECORD_SCHEMA_V1 {
                return Err(PersistError::schema(path, parsed.schema_version));
            }
            validate_v1(&parsed, path)?;
            let filename_id = character_id_from_path(path).ok_or_else(|| {
                PersistError::corrupt(path, "character filename is not char_{id}.json")
            })?;
            if filename_id != parsed.character_id {
                return Err(PersistError::corrupt(
                    path,
                    format!(
                        "file character_id {} does not match {}",
                        parsed.character_id, filename_id
                    ),
                ));
            }
            Ok(DiskCharacter::V1(V1Character {
                character_id: parsed.character_id,
                persistence_revision: parsed.persistence_revision,
                restore: parsed.restore,
                instance_exit: parsed.instance_exit,
                original_bytes: bytes,
            }))
        }
        crate::PERSISTENCE_SCHEMA_VERSION => {
            let mut parsed: PersistentCharacter =
                serde_json::from_slice(&bytes).map_err(|err| PersistError::json(path, err))?;
            domain::sort_character_items(&mut parsed.items);
            parsed.validate(path)?;
            let filename_id = character_id_from_path(path).ok_or_else(|| {
                PersistError::corrupt(path, "character filename is not char_{id}.json")
            })?;
            if filename_id != parsed.character_id {
                return Err(PersistError::corrupt(
                    path,
                    format!(
                        "file character_id {} does not match {}",
                        parsed.character_id, filename_id
                    ),
                ));
            }
            Ok(DiskCharacter::V2(parsed))
        }
        other => Err(PersistError::schema(path, other)),
    }
}

fn validate_v1(parsed: &CharacterFileV1, path: &Path) -> Result<(), PersistError> {
    if parsed.character_id.raw() == 0 {
        return Err(PersistError::corrupt(path, "character_id 0 is reserved"));
    }
    if parsed.persistence_revision == 0 {
        return Err(PersistError::corrupt(
            path,
            "persistence_revision 0 is reserved",
        ));
    }
    if parsed.restore.map_authored.is_empty() || parsed.restore.point_id.is_empty() {
        return Err(PersistError::corrupt(path, "restore map/point required"));
    }
    Ok(())
}

fn read_manifest(dir: &Path) -> Result<Option<ManifestFile>, PersistError> {
    let path = dir.join(MANIFEST_NAME);
    atomic::recover_if_needed(&path).map_err(|err| PersistError::io(&path, err))?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = read_limited(&path)?;
    let manifest: ManifestFile =
        serde_json::from_slice(&bytes).map_err(|err| PersistError::json(&path, err))?;
    if manifest.schema_version != DURABLE_MANIFEST_SCHEMA_VERSION {
        return Err(PersistError::schema(&path, manifest.schema_version));
    }
    if manifest.domain_version != DURABLE_DOMAIN_VERSION {
        return Err(PersistError::schema(&path, manifest.domain_version));
    }
    Ok(Some(manifest))
}

fn read_drop_file(dir: &Path) -> Result<Option<MapDropFile>, PersistError> {
    let path = dir.join(DROPS_NAME);
    atomic::recover_if_needed(&path).map_err(|err| PersistError::io(&path, err))?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = read_limited(&path)?;
    let mut file: MapDropFile =
        serde_json::from_slice(&bytes).map_err(|err| PersistError::json(&path, err))?;
    if file.schema_version != MAP_DROP_SCHEMA_VERSION {
        return Err(PersistError::schema(&path, file.schema_version));
    }
    for drop in &mut file.drops {
        domain::sort_drop(drop);
        domain::validate_drop_structure(drop, &path)?;
    }
    Ok(Some(file))
}

fn drop_map(drops: &[MapDropRecord]) -> BTreeMap<u64, MapDropRecord> {
    drops
        .iter()
        .cloned()
        .map(|drop| (drop.item_instance_id.raw(), drop))
        .collect()
}

fn read_limited(path: &Path) -> Result<Vec<u8>, PersistError> {
    let file = File::open(path).map_err(|err| PersistError::io(path, err))?;
    let len = file
        .metadata()
        .map_err(|err| PersistError::io(path, err))?
        .len();
    if len > MAX_RECORD_BYTES {
        return Err(PersistError::corrupt(path, "record exceeds the size limit"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|err| PersistError::io(path, err))?;
    Ok(bytes)
}

fn character_path(dir: &Path, id: CharacterId) -> PathBuf {
    dir.join(crate::character_file_name(id))
}

fn character_id_from_path(path: &Path) -> Option<CharacterId> {
    let name = path.file_name()?.to_str()?;
    let hex = name.strip_prefix("char_")?.strip_suffix(".json")?;
    if hex.len() != 16 {
        return None;
    }
    Some(CharacterId::from_raw(u64::from_str_radix(hex, 16).ok()?))
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
pub(crate) fn testing_wal_path(dir: &Path) -> PathBuf {
    dir.join(WAL_NAME)
}

#[cfg(test)]
mod crc_tests {
    use super::*;

    #[test]
    fn crc32_matches_iso_hdlc_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
