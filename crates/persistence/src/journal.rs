//! File-backed write-ahead log and derived checkpoints.
//!
//! The log is the commit authority. A commit validates against the in-memory
//! store, appends one framed transaction and syncs it; it writes nothing else.
//! The store keeps an item-owner index, so validating a commit reads only the
//! characters and drops the transaction names.
//!
//! A checkpoint stages changed character files and the map-drop file as
//! `.next` files, commits a manifest recording every checkpoint file's version
//! and CRC, installs the staged files in bounded batches, and then replaces the
//! log with one that starts after the manifest and keeps any later frames.
//! Recovery accepts a checkpoint file only when it matches the manifest (a
//! matching `.next` is rolled forward), then replays the log suffix. Paths,
//! framing and `sync_all` stay in this module.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use purgatory_common::{CharacterId, InstanceExitContext, ItemInstanceId, RestoreIntent};
use serde::{Deserialize, Serialize};

use crate::atomic::{self, replace_file_recoverable};
use crate::character::PersistentCharacter;
use crate::domain::{
    self, ACTIVE_SERVER_TICKS_PER_SECOND, CHARACTER_RECORD_SCHEMA_V1, ClockCheckpointKind,
    CommitResult, DURABLE_DOMAIN_VERSION, DurableContentRules, MAP_DROP_SCHEMA_VERSION,
    MapDropRecord, OwnershipChange, ReservedItemIds,
};
use crate::error::PersistError;

const WAL_NAME: &str = "ownership.wal";
const WAL_STAGED_NAME: &str = "ownership.wal.next";
const MANIFEST_NAME: &str = "durable_manifest.json";
const DROPS_NAME: &str = "map_drops.json";
const STAGED_SUFFIX: &str = ".next";
const MANIFEST_SCHEMA_VERSION: u32 = 2;
const WAL_MAGIC: &[u8; 8] = b"PGWAL001";
const HEADER_LEN: usize = 20;
pub(crate) const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;
const MAX_RECORD_BYTES: u64 = 8 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 256 * 1024 * 1024;
const MIGRATION_BATCH: usize = 256;

/// A checkpoint is due once the log holds this many frames.
pub(crate) const CHECKPOINT_AFTER_FRAMES: u64 = 4096;
/// A checkpoint is due once the log holds this many bytes.
pub(crate) const CHECKPOINT_AFTER_LOG_BYTES: u64 = 8 * 1024 * 1024;
/// Changed characters staged per maintenance call, so a checkpoint never
/// stalls the worker for more than this many file syncs at once.
pub(crate) const CHECKPOINT_STAGE_BATCH: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
struct V1Character {
    character_id: CharacterId,
    persistence_revision: u64,
    restore: RestoreIntent,
    instance_exit: Option<InstanceExitContext>,
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

/// Version (the transaction that last changed the record) and CRC-32 of the
/// exact checkpoint bytes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileVersion {
    version: u64,
    crc: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterCheckpoint {
    character_id: CharacterId,
    version: u64,
    crc: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IdRange {
    first: u64,
    last: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    schema_version: u32,
    domain_version: u32,
    applied_transaction_id: u64,
    clock_tick: u64,
    reserved_through: u64,
    unissued_item_ids: Vec<IdRange>,
    characters: Vec<CharacterCheckpoint>,
    map_drops: FileVersion,
    manifest_crc: u32,
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
    /// File offset immediately after each parsed frame, in transaction order.
    frame_ends: Vec<u64>,
    torn_tail: bool,
    good_len: u64,
}

enum DiskCharacter {
    V1(V1Character),
    V2(PersistentCharacter),
}

/// Reserved item ids that have not become items. Only these may be minted.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct IdRanges(BTreeMap<u64, u64>);

impl IdRanges {
    fn contains(&self, id: u64) -> bool {
        self.0
            .range(..=id)
            .next_back()
            .is_some_and(|(_, &last)| id <= last)
    }

    /// `first` must be above every existing range (reservations are monotonic).
    fn push(&mut self, first: u64, last: u64) {
        if let Some((&start, &end)) = self.0.iter().next_back()
            && end.checked_add(1) == Some(first)
        {
            self.0.insert(start, last);
            return;
        }
        self.0.insert(first, last);
    }

    fn take(&mut self, id: u64) -> bool {
        let Some((&start, &end)) = self.0.range(..=id).next_back() else {
            return false;
        };
        if id > end {
            return false;
        }
        self.0.remove(&start);
        if start < id {
            self.0.insert(start, id - 1);
        }
        if id < end {
            self.0.insert(id + 1, end);
        }
        true
    }

    fn to_file(&self) -> Vec<IdRange> {
        self.0
            .iter()
            .map(|(&first, &last)| IdRange { first, last })
            .collect()
    }

    fn from_file(
        ranges: &[IdRange],
        reserved_through: u64,
        path: &Path,
    ) -> Result<Self, PersistError> {
        let mut out = Self::default();
        let mut floor = 0u64;
        for range in ranges {
            if range.first <= floor || range.first > range.last || range.last > reserved_through {
                return Err(PersistError::integrity(
                    path,
                    "unissued item-id ranges are not ordered inside the reservation",
                ));
            }
            out.0.insert(range.first, range.last);
            floor = range.last;
        }
        Ok(out)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Owner {
    Character(u64),
    Map,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LocatedItem {
    definition: u64,
    quantity: u32,
    owner: Owner,
}

#[derive(Debug, Default)]
struct State {
    characters: BTreeMap<u64, PersistentCharacter>,
    drops: BTreeMap<u64, MapDropRecord>,
    drops_version: u64,
    clock_tick: u64,
    reserved_through: u64,
    last_transaction_id: u64,
    unissued: IdRanges,
    items: BTreeMap<u64, LocatedItem>,
    drop_ids: BTreeMap<u64, u64>,
}

#[derive(Debug)]
struct LogCursor {
    first_transaction_id: u64,
    frames: u64,
    len: u64,
}

/// What the synced manifest on disk records.
#[derive(Clone, Debug, Default)]
struct CheckpointIndex {
    present: bool,
    applied_transaction_id: u64,
    characters: BTreeMap<u64, FileVersion>,
    map_drops: FileVersion,
}

/// Recovered durable state plus the log position this process appends at.
/// The persistence worker is the only owner.
#[derive(Debug)]
pub(crate) struct Store {
    dir: PathBuf,
    state: State,
    log: LogCursor,
    checkpoint: CheckpointIndex,
    /// Characters whose current version the manifest does not record.
    dirty: BTreeSet<u64>,
    /// Synced `.next` files written for the checkpoint in progress.
    staged: BTreeMap<u64, FileVersion>,
    /// Manifest-named `.next` files still waiting to be renamed into place.
    install_queue: Vec<(PathBuf, PathBuf)>,
    /// Leading log bytes already covered by the manifest. The suffix after
    /// this length was committed later and must survive log replacement.
    obsolete_prefix_len: Option<u64>,
    frames_in_obsolete_prefix: u64,
    fault: Option<String>,
}

impl Store {
    /// Recover checkpoints and the log, then commit pending v1 migrations.
    pub(crate) fn open(dir: &Path) -> Result<Self, PersistError> {
        let (mut store, pending_v1) = recover(dir)?;
        store.migrate(pending_v1)?;
        Ok(store)
    }

    pub(crate) fn character(&self, id: CharacterId) -> Option<&PersistentCharacter> {
        self.state.characters.get(&id.raw())
    }

    /// For a character the log does not know: fail closed on existing bytes
    /// at its path, migrate a valid v1 file, or report absence.
    pub(crate) fn probe_unindexed(
        &mut self,
        id: CharacterId,
    ) -> Result<Option<PersistentCharacter>, PersistError> {
        if let Some(character) = self.character(id) {
            return Ok(Some(character.clone()));
        }
        let path = character_path(&self.dir, id);
        atomic::recover_if_needed(&path).map_err(|err| PersistError::io(&path, err))?;
        if !path.exists() {
            return Ok(None);
        }
        let bytes = read_limited(&path, MAX_RECORD_BYTES)?;
        match parse_character_bytes(&path, &bytes)? {
            DiskCharacter::V1(v1) => {
                self.migrate(vec![v1])?;
                Ok(self.character(id).cloned())
            }
            DiskCharacter::V2(_) => Err(PersistError::integrity(
                &path,
                "character checkpoint is not in the durable manifest",
            )),
        }
    }

    pub(crate) fn commit_ownership(
        &mut self,
        rules: &DurableContentRules,
        change: OwnershipChange,
    ) -> Result<CommitResult, PersistError> {
        self.commit(Some(rules), change, None, None)
    }

    pub(crate) fn reserve_ids(&mut self, count: u32) -> Result<ReservedItemIds, PersistError> {
        if count == 0 {
            return Err(PersistError::corrupt(
                &self.dir,
                "item id reservation count is 0",
            ));
        }
        let start = self
            .state
            .reserved_through
            .checked_add(1)
            .ok_or(PersistError::ItemIdsExhausted)?;
        let through = self
            .state
            .reserved_through
            .checked_add(u64::from(count))
            .ok_or(PersistError::ItemIdsExhausted)?;
        let committed = self.commit(None, empty_change(), Some(through), None)?;
        Ok(ReservedItemIds {
            transaction_id: committed.transaction_id,
            first: ItemInstanceId::from_raw(start),
            count,
        })
    }

    pub(crate) fn checkpoint_clock(
        &mut self,
        tick: u64,
        kind: ClockCheckpointKind,
    ) -> Result<CommitResult, PersistError> {
        let committed = self.state.clock_tick;
        if tick < committed {
            return Err(PersistError::integrity(
                &self.dir,
                format!("active clock moved backward from {committed} to {tick}"),
            ));
        }
        if tick == committed {
            return Ok(self.result());
        }
        if kind == ClockCheckpointKind::Periodic
            && tick - committed > ACTIVE_SERVER_TICKS_PER_SECOND
        {
            return Err(PersistError::ClockBound {
                committed_tick: committed,
                requested_tick: tick,
            });
        }
        self.commit(None, empty_change(), None, Some(tick))
    }

    pub(crate) fn clock_tick(&self) -> u64 {
        self.state.clock_tick
    }

    pub(crate) fn drops(&self) -> impl Iterator<Item = &MapDropRecord> {
        self.state.drops.values()
    }

    pub(crate) fn checkpoint_due(&self) -> bool {
        self.log.frames >= CHECKPOINT_AFTER_FRAMES || self.log.len >= CHECKPOINT_AFTER_LOG_BYTES
    }

    /// Write a checkpoint for everything committed so far and restart the log
    /// after it. Cost is proportional to characters changed since the last
    /// checkpoint plus the manifest index.
    pub(crate) fn checkpoint(&mut self) -> Result<(), PersistError> {
        while !self.checkpoint_step(CHECKPOINT_STAGE_BATCH)? {}
        Ok(())
    }

    /// One bounded unit of checkpoint work, once the log is due or a
    /// checkpoint is already in progress. Returns whether work ran.
    ///
    /// Staging, the manifest write, each install batch and the log replacement
    /// are separate calls, so none of them waits behind the whole backlog.
    pub(crate) fn maintain(&mut self) -> Result<bool, PersistError> {
        let in_progress = !self.staged.is_empty()
            || !self.install_queue.is_empty()
            || self.obsolete_prefix_len.is_some();
        if !in_progress && !self.checkpoint_due() {
            return Ok(false);
        }
        self.checkpoint_step(CHECKPOINT_STAGE_BATCH)?;
        Ok(true)
    }

    /// Advance one checkpoint phase by at most `budget` files. Returns true
    /// when the manifest, installs and log replacement are all caught up.
    pub(crate) fn checkpoint_step(&mut self, budget: usize) -> Result<bool, PersistError> {
        self.ensure_usable()?;
        let applied = self.state.last_transaction_id;
        if applied == 0 && !self.checkpoint.present {
            return Ok(true);
        }
        if !self.install_queue.is_empty() {
            self.install_batch(budget)?;
            return Ok(false);
        }
        if self.obsolete_prefix_len.is_some() {
            self.replace_obsolete_prefix()?;
            return Ok(false);
        }
        if self.checkpoint.present
            && self.checkpoint.applied_transaction_id == applied
            && self.dirty.is_empty()
            && self.staged.is_empty()
            && self.log.first_transaction_id == applied + 1
        {
            return Ok(true);
        }
        let todo: Vec<u64> = self
            .dirty
            .iter()
            .copied()
            .filter(|id| {
                self.staged.get(id).map(|entry| entry.version)
                    != Some(self.state.characters[id].applied_transaction_id)
            })
            .take(budget.saturating_add(1))
            .collect();
        let staging = todo.len().min(budget);
        for &id in todo.iter().take(budget) {
            self.stage_character(id)?;
        }
        if todo.len() > budget || staging > 0 {
            return Ok(false);
        }
        self.commit_manifest(applied)?;
        Ok(false)
    }

    /// Record the staged checkpoint in the manifest. Installing the files and
    /// replacing the log happen on later calls.
    fn commit_manifest(&mut self, applied: u64) -> Result<(), PersistError> {
        let mut installs: Vec<(PathBuf, PathBuf)> = self
            .staged
            .keys()
            .map(|&id| {
                let main = character_path(&self.dir, CharacterId::from_raw(id));
                (staged_path(&main), main)
            })
            .collect();
        let mut characters = self.checkpoint.characters.clone();
        characters.extend(self.staged.iter().map(|(&id, &entry)| (id, entry)));
        let mut map_drops = self.checkpoint.map_drops;
        if !self.checkpoint.present || self.state.drops_version != self.checkpoint.map_drops.version
        {
            let main = self.dir.join(DROPS_NAME);
            let next = staged_path(&main);
            let file = MapDropFile {
                schema_version: MAP_DROP_SCHEMA_VERSION,
                applied_transaction_id: self.state.drops_version,
                drops: self.state.drops.values().cloned().collect(),
            };
            let bytes =
                serde_json::to_vec_pretty(&file).map_err(|err| PersistError::json(&main, err))?;
            if let Err(err) = write_synced(&next, &bytes) {
                let _ = fs::remove_file(&next);
                return Err(err);
            }
            map_drops = FileVersion {
                version: self.state.drops_version,
                crc: crc32(&bytes),
            };
            installs.push((next, main));
        }
        atomic::sync_directory(&self.dir).map_err(|err| PersistError::io(&self.dir, err))?;
        crash_point(CrashPoint::CheckpointDataBeforeManifest, &self.dir)?;
        let manifest_path = self.dir.join(MANIFEST_NAME);
        let bytes = manifest_bytes(ManifestFile {
            schema_version: MANIFEST_SCHEMA_VERSION,
            domain_version: DURABLE_DOMAIN_VERSION,
            applied_transaction_id: applied,
            clock_tick: self.state.clock_tick,
            reserved_through: self.state.reserved_through,
            unissued_item_ids: self.state.unissued.to_file(),
            characters: characters
                .iter()
                .map(|(&id, entry)| CharacterCheckpoint {
                    character_id: CharacterId::from_raw(id),
                    version: entry.version,
                    crc: entry.crc,
                })
                .collect(),
            map_drops,
            manifest_crc: 0,
        })
        .map_err(|err| PersistError::json(&manifest_path, err))?;
        if let Err(err) = replace_file_recoverable(&manifest_path, &bytes) {
            // Either manifest may be on disk now; reopen decides from the files.
            self.fault = Some(format!("manifest replacement failed: {err}"));
            return Err(PersistError::io(&manifest_path, err));
        }
        self.checkpoint = CheckpointIndex {
            present: true,
            applied_transaction_id: applied,
            characters,
            map_drops,
        };
        self.install_queue = installs;
        self.obsolete_prefix_len = Some(self.log.len);
        self.frames_in_obsolete_prefix = self.log.frames;
        self.dirty.clear();
        self.staged.clear();
        Ok(())
    }

    fn install_batch(&mut self, budget: usize) -> Result<(), PersistError> {
        let count = budget.min(self.install_queue.len());
        let batch: Vec<_> = self.install_queue.drain(..count).collect();
        if let Err(err) = self.install_staged(&batch) {
            self.fault = Some(format!(
                "checkpoint install failed after the manifest: {err}"
            ));
            return Err(err);
        }
        Ok(())
    }

    /// Write one changed character as a synced `.next` file. It becomes a
    /// checkpoint only when a later manifest names its version and CRC.
    fn stage_character(&mut self, id: u64) -> Result<(), PersistError> {
        let character = &self.state.characters[&id];
        let main = character_path(&self.dir, character.character_id);
        let next = staged_path(&main);
        let bytes =
            serde_json::to_vec_pretty(character).map_err(|err| PersistError::json(&main, err))?;
        let entry = FileVersion {
            version: character.applied_transaction_id,
            crc: crc32(&bytes),
        };
        if let Err(err) = write_synced(&next, &bytes) {
            self.staged.remove(&id);
            let _ = fs::remove_file(&next);
            return Err(err);
        }
        self.staged.insert(id, entry);
        Ok(())
    }

    fn install_staged(&self, staged: &[(PathBuf, PathBuf)]) -> Result<(), PersistError> {
        crash_point(CrashPoint::CheckpointManifestBeforeInstall, &self.dir)?;
        for (next, main) in staged {
            fs::rename(next, main).map_err(|err| PersistError::io(main, err))?;
        }
        atomic::sync_directory(&self.dir).map_err(|err| PersistError::io(&self.dir, err))?;
        Ok(())
    }

    /// Replace the log with a header after the manifest plus any frames
    /// committed after that manifest. `rename` replaces the old log in one step.
    fn replace_obsolete_prefix(&mut self) -> Result<(), PersistError> {
        let Some(prefix) = self.obsolete_prefix_len else {
            return Ok(());
        };
        let canonical = self.dir.join(WAL_NAME);
        let staged = self.dir.join(WAL_STAGED_NAME);
        let applied = self.checkpoint.applied_transaction_id;
        let first = applied
            .checked_add(1)
            .ok_or_else(|| PersistError::integrity(&canonical, "transaction id overflow"))?;
        let on_disk = fs::metadata(&canonical)
            .map_err(|err| PersistError::io(&canonical, err))?
            .len();
        if on_disk < prefix || self.log.frames < self.frames_in_obsolete_prefix {
            return Err(PersistError::integrity(
                &canonical,
                "ownership log is shorter than the checkpointed prefix",
            ));
        }
        let mut suffix = Vec::new();
        if on_disk > prefix {
            let mut file =
                File::open(&canonical).map_err(|err| PersistError::io(&canonical, err))?;
            file.seek(SeekFrom::Start(prefix))
                .map_err(|err| PersistError::io(&canonical, err))?;
            file.read_to_end(&mut suffix)
                .map_err(|err| PersistError::io(&canonical, err))?;
        }
        let mut bytes = header_bytes(first).to_vec();
        bytes.extend_from_slice(&suffix);
        if let Err(err) = write_synced(&staged, &bytes).and_then(|()| {
            atomic::sync_directory(&self.dir)
                .map(|_| ())
                .map_err(|err| PersistError::io(&self.dir, err))
        }) {
            let _ = fs::remove_file(&staged);
            return Err(err);
        }
        crash_point(CrashPoint::LogReplacementBeforeInstall, &self.dir)?;
        if let Err(err) = fs::rename(&staged, &canonical) {
            if !canonical.exists() {
                self.fault = Some(format!("ownership log replacement failed: {err}"));
            }
            return Err(PersistError::io(&canonical, err));
        }
        atomic::sync_directory(&self.dir).map_err(|err| PersistError::io(&self.dir, err))?;
        self.log = LogCursor {
            first_transaction_id: first,
            frames: self.log.frames - self.frames_in_obsolete_prefix,
            len: bytes.len() as u64,
        };
        self.obsolete_prefix_len = None;
        self.frames_in_obsolete_prefix = 0;
        Ok(())
    }

    fn migrate(&mut self, mut pending: Vec<V1Character>) -> Result<(), PersistError> {
        pending.sort_by_key(|v1| v1.character_id.raw());
        let mut migrated = Vec::with_capacity(pending.len());
        for v1 in pending {
            let path = character_path(&self.dir, v1.character_id);
            let persistence_revision =
                v1.persistence_revision
                    .checked_add(1)
                    .ok_or_else(|| PersistError::Migration {
                        path: path.clone(),
                        reason: "persistence_revision overflow".into(),
                    })?;
            let character = PersistentCharacter {
                schema_version: crate::PERSISTENCE_SCHEMA_VERSION,
                applied_transaction_id: 0,
                character_id: v1.character_id,
                persistence_revision,
                restore: v1.restore,
                instance_exit: v1.instance_exit,
                items: Vec::new(),
            };
            character.validate(&path)?;
            migrated.push(character);
        }
        for batch in migrated.chunks(MIGRATION_BATCH) {
            self.commit(
                None,
                OwnershipChange {
                    characters: batch.to_vec(),
                    drops_upsert: Vec::new(),
                    drops_remove: Vec::new(),
                },
                None,
                None,
            )?;
        }
        Ok(())
    }

    fn commit(
        &mut self,
        rules: Option<&DurableContentRules>,
        change: OwnershipChange,
        reserved_through: Option<u64>,
        clock_tick: Option<u64>,
    ) -> Result<CommitResult, PersistError> {
        self.ensure_usable()?;
        let txn = prepare_txn(&self.state, change, reserved_through, clock_tick, &self.dir)?;
        check_txn(&self.state, &txn, &self.dir)?;
        if let Some(rules) = rules {
            check_content(&txn, rules, &self.dir)?;
        }
        self.append(&txn)?;
        for character in &txn.characters {
            self.dirty.insert(character.character_id.raw());
        }
        apply_txn(&mut self.state, &txn);
        Ok(self.result())
    }

    fn result(&self) -> CommitResult {
        CommitResult {
            transaction_id: self.state.last_transaction_id,
            active_clock_tick: self.state.clock_tick,
            reserved_through: self.state.reserved_through,
        }
    }

    fn ensure_usable(&self) -> Result<(), PersistError> {
        match &self.fault {
            Some(reason) => Err(PersistError::integrity(
                &self.dir,
                format!("durable store must be reopened: {reason}"),
            )),
            None => Ok(()),
        }
    }

    /// Append and sync one frame. A failed write is rolled back to the last
    /// acknowledged length; if that fails too, the store refuses further work.
    fn append(&mut self, txn: &TxnRecord) -> Result<(), PersistError> {
        let path = self.dir.join(WAL_NAME);
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
        let on_disk = file
            .metadata()
            .map_err(|err| PersistError::io(&path, err))?
            .len();
        if on_disk != self.log.len {
            return Err(PersistError::integrity(
                &path,
                format!(
                    "ownership log is {on_disk} bytes but this store last wrote {}",
                    self.log.len
                ),
            ));
        }
        let created = on_disk == 0;
        let mut bytes = Vec::with_capacity(HEADER_LEN + 8 + payload.len());
        if created {
            bytes.extend_from_slice(&header_bytes(self.log.first_transaction_id));
        }
        bytes.extend_from_slice(&frame_bytes(&payload));
        let written = file
            .seek(SeekFrom::End(0))
            .and_then(|_| file.write_all(&bytes))
            .and_then(|()| file.sync_all())
            .and_then(|()| {
                if created {
                    atomic::sync_directory(&self.dir).map(|_| ())
                } else {
                    Ok(())
                }
            });
        if let Err(err) = written {
            let rolled_back = file.set_len(self.log.len).and_then(|()| file.sync_all());
            if let Err(rollback) = rolled_back {
                self.fault = Some(format!(
                    "ownership log write failed ({err}) and was not rolled back ({rollback})"
                ));
            }
            return Err(PersistError::io(&path, err));
        }
        self.log.len += bytes.len() as u64;
        self.log.frames += 1;
        Ok(())
    }
}

fn empty_change() -> OwnershipChange {
    OwnershipChange {
        characters: Vec::new(),
        drops_upsert: Vec::new(),
        drops_remove: Vec::new(),
    }
}

fn prepare_txn(
    state: &State,
    mut change: OwnershipChange,
    reserved_through: Option<u64>,
    clock_tick: Option<u64>,
    dir: &Path,
) -> Result<TxnRecord, PersistError> {
    let transaction_id = state
        .last_transaction_id
        .checked_add(1)
        .ok_or_else(|| PersistError::integrity(dir.join(WAL_NAME), "transaction id overflow"))?;
    for character in &mut change.characters {
        domain::sort_character_items(&mut character.items);
        character.schema_version = crate::PERSISTENCE_SCHEMA_VERSION;
        character.applied_transaction_id = transaction_id;
    }
    for drop in &mut change.drops_upsert {
        domain::sort_drop(drop);
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

fn check_content(
    txn: &TxnRecord,
    rules: &DurableContentRules,
    dir: &Path,
) -> Result<(), PersistError> {
    for character in &txn.characters {
        let path = character_path(dir, character.character_id);
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
    }
    let drops_path = dir.join(DROPS_NAME);
    for drop in &txn.drops_upsert {
        domain::validate_drop_content(drop, rules, &drops_path)?;
    }
    Ok(())
}

/// Validate one transaction against the current state. Reads only the
/// records the transaction names plus the item-owner index.
fn check_txn(state: &State, txn: &TxnRecord, dir: &Path) -> Result<(), PersistError> {
    let log_path = dir.join(WAL_NAME);
    if txn.domain_version != DURABLE_DOMAIN_VERSION {
        return Err(PersistError::schema(&log_path, txn.domain_version));
    }
    if Some(txn.transaction_id) != state.last_transaction_id.checked_add(1) {
        return Err(PersistError::integrity(
            &log_path,
            format!(
                "transaction {} is not the next id after {}",
                txn.transaction_id, state.last_transaction_id
            ),
        ));
    }
    let reserved_after = match txn.reserved_through {
        Some(reserved) if reserved <= state.reserved_through => {
            return Err(PersistError::integrity(
                &log_path,
                "item id high water did not advance",
            ));
        }
        Some(reserved) => reserved,
        None => state.reserved_through,
    };
    let clock_after = match txn.clock_tick {
        Some(tick) if tick <= state.clock_tick => {
            return Err(PersistError::integrity(
                &log_path,
                "active clock did not advance",
            ));
        }
        Some(tick) => tick,
        None => state.clock_tick,
    };
    let mut characters = BTreeSet::new();
    for character in &txn.characters {
        let raw = character.character_id.raw();
        let path = character_path(dir, character.character_id);
        if !characters.insert(raw) {
            return Err(PersistError::integrity(
                &path,
                "character appears twice in one transaction",
            ));
        }
        character.validate(&path)?;
        if character.applied_transaction_id != txn.transaction_id {
            return Err(PersistError::integrity(
                &path,
                "character version is not its transaction id",
            ));
        }
        if let Some(existing) = state.characters.get(&raw)
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
    }
    let drops_path = dir.join(DROPS_NAME);
    let mut removed = BTreeSet::new();
    for id in &txn.drops_remove {
        if !state.drops.contains_key(&id.raw()) {
            return Err(PersistError::corrupt(
                &drops_path,
                format!("removed drop {} is not on a map", id.raw()),
            ));
        }
        if !removed.insert(id.raw()) {
            return Err(PersistError::integrity(
                &drops_path,
                format!("drop {} is removed twice", id.raw()),
            ));
        }
    }
    let mut upserted = BTreeSet::new();
    let mut drop_ids = BTreeSet::new();
    for drop in &txn.drops_upsert {
        let raw = drop.item_instance_id.raw();
        domain::validate_drop_structure(drop, &drops_path)?;
        if removed.contains(&raw) || !upserted.insert(raw) {
            return Err(PersistError::integrity(
                &drops_path,
                format!("drop {raw} appears twice in one transaction"),
            ));
        }
        if !drop_ids.insert(drop.drop_id) {
            return Err(PersistError::integrity(
                &drops_path,
                format!("duplicate drop_id {}", drop.drop_id),
            ));
        }
        if !state.drops.contains_key(&raw) {
            domain::validate_new_drop_timing(drop, clock_after, &drops_path)?;
        }
    }
    for drop in &txn.drops_upsert {
        if let Some(&holder) = state.drop_ids.get(&drop.drop_id)
            && holder != drop.item_instance_id.raw()
            && !removed.contains(&holder)
            && !upserted.contains(&holder)
        {
            return Err(PersistError::integrity(
                &drops_path,
                format!("duplicate drop_id {}", drop.drop_id),
            ));
        }
    }
    let mut post = BTreeMap::<u64, LocatedItem>::new();
    for character in &txn.characters {
        for item in &character.items {
            let located = LocatedItem {
                definition: item.definition_content_id.token(),
                quantity: item.quantity,
                owner: Owner::Character(character.character_id.raw()),
            };
            if post.insert(item.item_instance_id.raw(), located).is_some() {
                return Err(PersistError::integrity(
                    &log_path,
                    format!("duplicate item id {}", item.item_instance_id.raw()),
                ));
            }
        }
    }
    for drop in &txn.drops_upsert {
        let located = LocatedItem {
            definition: drop.definition_content_id.token(),
            quantity: drop.quantity,
            owner: Owner::Map,
        };
        if post.insert(drop.item_instance_id.raw(), located).is_some() {
            return Err(PersistError::integrity(
                &log_path,
                format!("duplicate item id {}", drop.item_instance_id.raw()),
            ));
        }
    }
    for (&id, item) in &post {
        match state.items.get(&id) {
            Some(previous) => {
                let released = match previous.owner {
                    Owner::Character(owner) => characters.contains(&owner),
                    Owner::Map => removed.contains(&id) || upserted.contains(&id),
                };
                if !released {
                    return Err(PersistError::integrity(
                        &log_path,
                        format!("item {id} is still owned by a record outside this transaction"),
                    ));
                }
                if previous.definition != item.definition {
                    return Err(PersistError::integrity(
                        &log_path,
                        format!("item {id} definition changed"),
                    ));
                }
                if previous.owner != item.owner && previous.quantity != item.quantity {
                    return Err(PersistError::integrity(
                        &log_path,
                        format!("item {id} changed quantity while changing owner"),
                    ));
                }
            }
            None => {
                let newly_reserved = id > state.reserved_through && id <= reserved_after;
                if !state.unissued.contains(id) && !newly_reserved {
                    return Err(PersistError::integrity(
                        &log_path,
                        format!("item {id} is not an unissued reserved id"),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Apply a transaction that [`check_txn`] accepted.
fn apply_txn(state: &mut State, txn: &TxnRecord) {
    let minted: Vec<u64> = txn
        .characters
        .iter()
        .flat_map(|character| {
            character
                .items
                .iter()
                .map(|item| item.item_instance_id.raw())
        })
        .chain(
            txn.drops_upsert
                .iter()
                .map(|drop| drop.item_instance_id.raw()),
        )
        .filter(|id| !state.items.contains_key(id))
        .collect();
    if let Some(reserved) = txn.reserved_through {
        state.unissued.push(state.reserved_through + 1, reserved);
        state.reserved_through = reserved;
    }
    if let Some(tick) = txn.clock_tick {
        state.clock_tick = tick;
    }
    let replaced_drops = txn
        .drops_remove
        .iter()
        .copied()
        .chain(txn.drops_upsert.iter().map(|drop| drop.item_instance_id));
    for id in replaced_drops {
        if let Some(old) = state.drops.remove(&id.raw()) {
            state.items.remove(&id.raw());
            state.drop_ids.remove(&old.drop_id);
        }
    }
    for character in &txn.characters {
        if let Some(old) = state.characters.remove(&character.character_id.raw()) {
            for item in &old.items {
                state.items.remove(&item.item_instance_id.raw());
            }
        }
    }
    for drop in &txn.drops_upsert {
        let raw = drop.item_instance_id.raw();
        state.items.insert(
            raw,
            LocatedItem {
                definition: drop.definition_content_id.token(),
                quantity: drop.quantity,
                owner: Owner::Map,
            },
        );
        state.drop_ids.insert(drop.drop_id, raw);
        state.drops.insert(raw, drop.clone());
    }
    for character in &txn.characters {
        let owner = Owner::Character(character.character_id.raw());
        for item in &character.items {
            state.items.insert(
                item.item_instance_id.raw(),
                LocatedItem {
                    definition: item.definition_content_id.token(),
                    quantity: item.quantity,
                    owner,
                },
            );
        }
        state
            .characters
            .insert(character.character_id.raw(), character.clone());
    }
    for id in minted {
        state.unissued.take(id);
    }
    if !txn.drops_remove.is_empty() || !txn.drops_upsert.is_empty() {
        state.drops_version = txn.transaction_id;
    }
    state.last_transaction_id = txn.transaction_id;
}

/// Recover the store. All files are read and validated before anything on
/// disk is repaired, rolled forward or deleted.
fn recover(dir: &Path) -> Result<(Store, Vec<V1Character>), PersistError> {
    let manifest = read_manifest(dir)?;
    let applied = manifest
        .as_ref()
        .map_or(0, |manifest| manifest.applied_transaction_id);
    let canonical = dir.join(WAL_NAME);
    let staged_log = dir.join(WAL_STAGED_NAME);
    let install_staged_log = if canonical.exists() {
        false
    } else if staged_log.exists() {
        true
    } else if manifest.is_some() {
        return Err(PersistError::integrity(
            &canonical,
            "the checkpoint manifest exists but the ownership log is missing",
        ));
    } else {
        false
    };
    let log_path = if install_staged_log {
        &staged_log
    } else {
        &canonical
    };
    let scan = scan_wal(log_path)?;
    check_contiguous(&scan, log_path)?;
    let log_last = scan.first_transaction_id - 1 + scan.txns.len() as u64;
    if scan.first_transaction_id > applied.saturating_add(1) {
        return Err(PersistError::integrity(
            log_path,
            "ownership log starts after the checkpoint manifest",
        ));
    }
    if log_last < applied {
        return Err(PersistError::integrity(
            log_path,
            "ownership log ends before the checkpoint manifest",
        ));
    }
    if install_staged_log && (scan.first_transaction_id != applied + 1 || scan.torn_tail) {
        return Err(PersistError::integrity(
            &staged_log,
            "staged ownership log does not start after the checkpoint manifest",
        ));
    }

    let index = CheckpointIndex::from_manifest(manifest.as_ref());
    let mut roll_forward = Vec::new();
    let mut stale = Vec::new();
    let mut base_characters = BTreeMap::new();
    let mut v1_files = Vec::new();
    let listed = list_character_files(dir)?;
    for (&id, &has_next) in &listed {
        let main = character_path(dir, CharacterId::from_raw(id));
        let next = staged_path(&main);
        let main_bytes = if main.exists() {
            Some(read_limited(&main, MAX_RECORD_BYTES)?)
        } else {
            None
        };
        match index.characters.get(&id) {
            Some(expected) => {
                if let Some(bytes) = &main_bytes
                    && crc32(bytes) == expected.crc
                {
                    base_characters.insert(id, parse_checkpoint(&main, bytes, *expected)?);
                    if has_next {
                        stale.push(next);
                    }
                } else if has_next {
                    let bytes = read_limited(&next, MAX_RECORD_BYTES)?;
                    if crc32(&bytes) != expected.crc {
                        return Err(PersistError::integrity(
                            &main,
                            "character checkpoint does not match the manifest",
                        ));
                    }
                    base_characters.insert(id, parse_checkpoint(&next, &bytes, *expected)?);
                    roll_forward.push((next, main));
                } else {
                    return Err(PersistError::integrity(
                        &main,
                        if main_bytes.is_some() {
                            "character checkpoint does not match the manifest"
                        } else {
                            "character checkpoint is missing"
                        },
                    ));
                }
            }
            None => {
                if has_next {
                    stale.push(next);
                }
                if let Some(bytes) = &main_bytes {
                    match parse_character_bytes(&main, bytes)? {
                        DiskCharacter::V1(v1) => v1_files.push(v1),
                        DiskCharacter::V2(_) => {
                            return Err(PersistError::integrity(
                                &main,
                                "character checkpoint is not in the manifest",
                            ));
                        }
                    }
                }
            }
        }
    }
    for &id in index.characters.keys() {
        if !listed.contains_key(&id) {
            return Err(PersistError::integrity(
                character_path(dir, CharacterId::from_raw(id)),
                "character checkpoint is missing",
            ));
        }
    }
    let (drops, drops_version) = recover_drop_file(dir, &index, &mut roll_forward, &mut stale)?;

    let manifest_path = dir.join(MANIFEST_NAME);
    let mut state = State {
        characters: base_characters,
        drops,
        drops_version,
        clock_tick: manifest.as_ref().map_or(0, |manifest| manifest.clock_tick),
        reserved_through: manifest
            .as_ref()
            .map_or(0, |manifest| manifest.reserved_through),
        last_transaction_id: applied,
        unissued: match &manifest {
            Some(manifest) => IdRanges::from_file(
                &manifest.unissued_item_ids,
                manifest.reserved_through,
                &manifest_path,
            )?,
            None => IdRanges::default(),
        },
        items: BTreeMap::new(),
        drop_ids: BTreeMap::new(),
    };
    index_base(&mut state, &manifest_path)?;
    for txn in &scan.txns {
        if txn.transaction_id <= applied {
            continue;
        }
        check_txn(&state, txn, dir)?;
        apply_txn(&mut state, txn);
    }
    let migration_images = first_logged_characters(&scan, applied);
    let mut pending_v1 = Vec::new();
    for v1 in v1_files {
        if state.characters.contains_key(&v1.character_id.raw()) {
            check_migrated_v1(dir, &v1, migration_images.get(&v1.character_id.raw()))?;
        } else {
            pending_v1.push(v1);
        }
    }

    if scan.torn_tail {
        repair_torn_tail(log_path, scan.good_len, scan.first_transaction_id)?;
    }
    let mut renamed = false;
    for (next, main) in &roll_forward {
        fs::rename(next, main).map_err(|err| PersistError::io(main, err))?;
        renamed = true;
    }
    for path in &stale {
        remove_if_present(path)?;
    }
    if install_staged_log {
        fs::rename(&staged_log, &canonical).map_err(|err| PersistError::io(&canonical, err))?;
        renamed = true;
    } else {
        remove_if_present(&staged_log)?;
    }
    if renamed {
        atomic::sync_directory(dir).map_err(|err| PersistError::io(dir, err))?;
    }
    let len = match fs::metadata(&canonical) {
        Ok(metadata) => metadata.len(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => 0,
        Err(err) => return Err(PersistError::io(&canonical, err)),
    };
    let dirty = state
        .characters
        .iter()
        .filter(|(id, character)| {
            index.characters.get(id).map(|entry| entry.version)
                != Some(character.applied_transaction_id)
        })
        .map(|(&id, _)| id)
        .collect();
    // Reservations belong to the process that made them. Ids a crashed
    // process reserved and never used stay gaps; they are never issued.
    state.unissued = IdRanges::default();
    let obsolete = obsolete_log_prefix(&scan, applied);
    let store = Store {
        dir: dir.to_path_buf(),
        state,
        log: LogCursor {
            first_transaction_id: scan.first_transaction_id,
            frames: scan.txns.len() as u64,
            len,
        },
        checkpoint: index,
        dirty,
        staged: BTreeMap::new(),
        install_queue: Vec::new(),
        obsolete_prefix_len: obsolete.map(|(prefix_len, _)| prefix_len),
        frames_in_obsolete_prefix: obsolete.map_or(0, |(_, frames)| frames),
        fault: None,
    };
    Ok((store, pending_v1))
}

impl CheckpointIndex {
    fn from_manifest(manifest: Option<&ManifestFile>) -> Self {
        let Some(manifest) = manifest else {
            return Self::default();
        };
        Self {
            present: true,
            applied_transaction_id: manifest.applied_transaction_id,
            characters: manifest
                .characters
                .iter()
                .map(|entry| {
                    (
                        entry.character_id.raw(),
                        FileVersion {
                            version: entry.version,
                            crc: entry.crc,
                        },
                    )
                })
                .collect(),
            map_drops: manifest.map_drops,
        }
    }
}

fn recover_drop_file(
    dir: &Path,
    index: &CheckpointIndex,
    roll_forward: &mut Vec<(PathBuf, PathBuf)>,
    stale: &mut Vec<PathBuf>,
) -> Result<(BTreeMap<u64, MapDropRecord>, u64), PersistError> {
    let main = dir.join(DROPS_NAME);
    atomic::recover_if_needed(&main).map_err(|err| PersistError::io(&main, err))?;
    let next = staged_path(&main);
    if !index.present {
        if main.exists() {
            return Err(PersistError::integrity(
                &main,
                "map-drop checkpoint is not in the manifest",
            ));
        }
        if next.exists() {
            stale.push(next);
        }
        return Ok((BTreeMap::new(), 0));
    }
    let expected = index.map_drops;
    if main.exists() {
        let bytes = read_limited(&main, MAX_RECORD_BYTES)?;
        if crc32(&bytes) == expected.crc {
            if next.exists() {
                stale.push(next);
            }
            return parse_drop_file(&main, &bytes, expected);
        }
    }
    if next.exists() {
        let bytes = read_limited(&next, MAX_RECORD_BYTES)?;
        if crc32(&bytes) == expected.crc {
            let parsed = parse_drop_file(&next, &bytes, expected)?;
            roll_forward.push((next, main));
            return Ok(parsed);
        }
    }
    Err(PersistError::integrity(
        &main,
        "map-drop checkpoint does not match the manifest",
    ))
}

fn parse_drop_file(
    path: &Path,
    bytes: &[u8],
    expected: FileVersion,
) -> Result<(BTreeMap<u64, MapDropRecord>, u64), PersistError> {
    let probe: SchemaProbe =
        serde_json::from_slice(bytes).map_err(|err| PersistError::json(path, err))?;
    if probe.schema_version != MAP_DROP_SCHEMA_VERSION {
        return Err(PersistError::schema(path, probe.schema_version));
    }
    let file: MapDropFile =
        serde_json::from_slice(bytes).map_err(|err| PersistError::json(path, err))?;
    if file.applied_transaction_id != expected.version {
        return Err(PersistError::integrity(
            path,
            "map-drop checkpoint version does not match the manifest",
        ));
    }
    let mut drops = BTreeMap::new();
    for mut drop in file.drops {
        domain::sort_drop(&mut drop);
        domain::validate_drop_structure(&drop, path)?;
        if drops.insert(drop.item_instance_id.raw(), drop).is_some() {
            return Err(PersistError::integrity(path, "duplicate map-drop item id"));
        }
    }
    Ok((drops, file.applied_transaction_id))
}

fn parse_checkpoint(
    path: &Path,
    bytes: &[u8],
    expected: FileVersion,
) -> Result<PersistentCharacter, PersistError> {
    match parse_character_bytes(path, bytes)? {
        DiskCharacter::V2(character) if character.applied_transaction_id == expected.version => {
            Ok(character)
        }
        _ => Err(PersistError::integrity(
            path,
            "character checkpoint version does not match the manifest",
        )),
    }
}

/// Build the owner index for a checkpoint. Every live item appears once, was
/// reserved, and is not also listed as unissued.
fn index_base(state: &mut State, path: &Path) -> Result<(), PersistError> {
    let mut items = BTreeMap::new();
    let mut drop_ids = BTreeMap::new();
    let mut add = |raw: u64, located: LocatedItem| {
        if raw > state.reserved_through {
            return Err(PersistError::integrity(
                path,
                format!("item {raw} was never reserved"),
            ));
        }
        if state.unissued.contains(raw) {
            return Err(PersistError::integrity(
                path,
                format!("item {raw} is live and also unissued"),
            ));
        }
        if items.insert(raw, located).is_some() {
            return Err(PersistError::integrity(
                path,
                format!("duplicate item id {raw}"),
            ));
        }
        Ok(())
    };
    for character in state.characters.values() {
        if character.applied_transaction_id > state.last_transaction_id {
            return Err(PersistError::integrity(
                path,
                "character checkpoint is newer than the manifest",
            ));
        }
        for item in &character.items {
            add(
                item.item_instance_id.raw(),
                LocatedItem {
                    definition: item.definition_content_id.token(),
                    quantity: item.quantity,
                    owner: Owner::Character(character.character_id.raw()),
                },
            )?;
        }
    }
    for drop in state.drops.values() {
        add(
            drop.item_instance_id.raw(),
            LocatedItem {
                definition: drop.definition_content_id.token(),
                quantity: drop.quantity,
                owner: Owner::Map,
            },
        )?;
        if drop_ids
            .insert(drop.drop_id, drop.item_instance_id.raw())
            .is_some()
        {
            return Err(PersistError::integrity(
                path,
                format!("duplicate drop_id {}", drop.drop_id),
            ));
        }
    }
    if state.drops_version > state.last_transaction_id {
        return Err(PersistError::integrity(
            path,
            "map-drop checkpoint is newer than the manifest",
        ));
    }
    state.items = items;
    state.drop_ids = drop_ids;
    Ok(())
}

/// First logged post-state of each character after the manifest, from one scan.
fn first_logged_characters(scan: &WalScan, applied: u64) -> BTreeMap<u64, &PersistentCharacter> {
    let mut images = BTreeMap::new();
    for txn in scan.txns.iter().filter(|txn| txn.transaction_id > applied) {
        for character in &txn.characters {
            images
                .entry(character.character_id.raw())
                .or_insert(character);
        }
    }
    images
}

/// Byte length and frame count of the log prefix the manifest already covers.
fn obsolete_log_prefix(scan: &WalScan, applied: u64) -> Option<(u64, u64)> {
    if applied == 0 || scan.first_transaction_id > applied {
        return None;
    }
    let mut frames = 0u64;
    let mut end = HEADER_LEN as u64;
    for (txn, &frame_end) in scan.txns.iter().zip(&scan.frame_ends) {
        if txn.transaction_id > applied {
            break;
        }
        frames += 1;
        end = frame_end;
    }
    Some((end, frames))
}

/// A v1 file whose character the log suffix already holds is a stale
/// checkpoint only when the first logged image is exactly its migration.
fn check_migrated_v1(
    dir: &Path,
    v1: &V1Character,
    first: Option<&&PersistentCharacter>,
) -> Result<(), PersistError> {
    let path = character_path(dir, v1.character_id);
    let expected_revision = v1.persistence_revision.checked_add(1);
    let image = first.copied().is_some_and(|post| {
        post.items.is_empty()
            && Some(post.persistence_revision) == expected_revision
            && post.restore == v1.restore
            && post.instance_exit == v1.instance_exit
    });
    if !image {
        return Err(PersistError::integrity(
            &path,
            format!(
                "v1 character {} does not match its first committed transaction",
                v1.character_id
            ),
        ));
    }
    Ok(())
}

/// CRC of the manifest serialized with `manifest_crc` set to zero.
fn manifest_crc(manifest: &ManifestFile) -> Result<u32, serde_json::Error> {
    let mut zeroed = manifest.clone();
    zeroed.manifest_crc = 0;
    Ok(crc32(&serde_json::to_vec(&zeroed)?))
}

fn manifest_bytes(mut manifest: ManifestFile) -> Result<Vec<u8>, serde_json::Error> {
    manifest.manifest_crc = manifest_crc(&manifest)?;
    serde_json::to_vec(&manifest)
}

fn read_manifest(dir: &Path) -> Result<Option<ManifestFile>, PersistError> {
    let path = dir.join(MANIFEST_NAME);
    atomic::recover_if_needed(&path).map_err(|err| PersistError::io(&path, err))?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = read_limited(&path, MAX_MANIFEST_BYTES)?;
    let probe: SchemaProbe =
        serde_json::from_slice(&bytes).map_err(|err| PersistError::json(&path, err))?;
    if probe.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(PersistError::schema(&path, probe.schema_version));
    }
    let manifest: ManifestFile =
        serde_json::from_slice(&bytes).map_err(|err| PersistError::json(&path, err))?;
    if manifest.domain_version != DURABLE_DOMAIN_VERSION {
        return Err(PersistError::schema(&path, manifest.domain_version));
    }
    if manifest_crc(&manifest).map_err(|err| PersistError::json(&path, err))?
        != manifest.manifest_crc
    {
        return Err(PersistError::integrity(
            &path,
            "manifest checksum does not match its contents",
        ));
    }
    let mut previous = 0u64;
    for entry in &manifest.characters {
        let raw = entry.character_id.raw();
        if raw <= previous || entry.version == 0 || entry.version > manifest.applied_transaction_id
        {
            return Err(PersistError::integrity(
                &path,
                "manifest character entries are not ordered committed checkpoints",
            ));
        }
        previous = raw;
    }
    if manifest.map_drops.version > manifest.applied_transaction_id {
        return Err(PersistError::integrity(
            &path,
            "manifest map-drop entry is newer than the manifest",
        ));
    }
    Ok(Some(manifest))
}

/// Character ids with a main, `.bak`, `.tmp` or staged `.next` file, and
/// whether a staged file exists. Recoverable `.bak`/`.tmp` state is resolved.
fn list_character_files(dir: &Path) -> Result<BTreeMap<u64, bool>, PersistError> {
    let mut found = BTreeMap::<u64, bool>::new();
    let entries = fs::read_dir(dir).map_err(|err| PersistError::io(dir, err))?;
    for entry in entries {
        let entry = entry.map_err(|err| PersistError::io(dir, err))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let (base, staged) = match name.strip_suffix(STAGED_SUFFIX) {
            Some(base) => (base, true),
            None => (
                name.strip_suffix(".bak")
                    .or_else(|| name.strip_suffix(".tmp"))
                    .unwrap_or(name),
                false,
            ),
        };
        let Some(id) = character_id_from_name(base) else {
            continue;
        };
        let has_next = found.entry(id.raw()).or_default();
        *has_next |= staged;
    }
    for &id in found.keys() {
        let path = character_path(dir, CharacterId::from_raw(id));
        atomic::recover_if_needed(&path).map_err(|err| PersistError::io(&path, err))?;
    }
    Ok(found)
}

fn parse_character_bytes(path: &Path, bytes: &[u8]) -> Result<DiskCharacter, PersistError> {
    let probe: SchemaProbe =
        serde_json::from_slice(bytes).map_err(|err| PersistError::json(path, err))?;
    let filename_id = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| character_id_from_name(name.strip_suffix(STAGED_SUFFIX).unwrap_or(name)))
        .ok_or_else(|| PersistError::corrupt(path, "character filename is not char_{id}.json"))?;
    match probe.schema_version {
        CHARACTER_RECORD_SCHEMA_V1 => {
            let parsed: CharacterFileV1 =
                serde_json::from_slice(bytes).map_err(|err| PersistError::json(path, err))?;
            if parsed.schema_version != CHARACTER_RECORD_SCHEMA_V1 {
                return Err(PersistError::schema(path, parsed.schema_version));
            }
            validate_v1(&parsed, path)?;
            if parsed.character_id != filename_id {
                return Err(id_mismatch(path, parsed.character_id, filename_id));
            }
            Ok(DiskCharacter::V1(V1Character {
                character_id: parsed.character_id,
                persistence_revision: parsed.persistence_revision,
                restore: parsed.restore,
                instance_exit: parsed.instance_exit,
            }))
        }
        crate::PERSISTENCE_SCHEMA_VERSION => {
            let mut parsed: PersistentCharacter =
                serde_json::from_slice(bytes).map_err(|err| PersistError::json(path, err))?;
            domain::sort_character_items(&mut parsed.items);
            parsed.validate(path)?;
            if parsed.character_id != filename_id {
                return Err(id_mismatch(path, parsed.character_id, filename_id));
            }
            Ok(DiskCharacter::V2(parsed))
        }
        other => Err(PersistError::schema(path, other)),
    }
}

fn id_mismatch(path: &Path, found: CharacterId, expected: CharacterId) -> PersistError {
    PersistError::corrupt(
        path,
        format!("file character_id {found} does not match {expected}"),
    )
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

/// Parse the log. Only an incomplete final frame is a torn tail; a complete
/// frame that fails its checksum or does not parse fails closed.
fn scan_wal(path: &Path) -> Result<WalScan, PersistError> {
    let empty = |torn_tail| WalScan {
        first_transaction_id: 1,
        txns: Vec::new(),
        frame_ends: Vec::new(),
        torn_tail,
        good_len: 0,
    };
    if !path.exists() {
        return Ok(empty(false));
    }
    if path.is_dir() {
        return Err(PersistError::io(
            path,
            std::io::Error::other("transaction log is a directory"),
        ));
    }
    let bytes = fs::read(path).map_err(|err| PersistError::io(path, err))?;
    if bytes.len() < HEADER_LEN {
        return Ok(empty(true));
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
    let mut frame_ends = Vec::new();
    let torn = |txns, frame_ends, offset: usize| WalScan {
        first_transaction_id: first,
        txns,
        frame_ends,
        torn_tail: true,
        good_len: offset as u64,
    };
    while offset < bytes.len() {
        let remaining = bytes.len() - offset;
        if remaining < 4 {
            return Ok(torn(txns, frame_ends, offset));
        }
        let len =
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("4 bytes")) as usize;
        // A length we would never write is corruption of a committed frame,
        // even when the file ends before that many bytes. Only a short tail
        // whose length is a legal frame size is incomplete and repairable.
        if len == 0 || len > MAX_FRAME_LEN {
            return Err(PersistError::integrity(
                path,
                "transaction frame length is not a committed record",
            ));
        }
        let frame_len = len + 8;
        if remaining < frame_len {
            return Ok(torn(txns, frame_ends, offset));
        }
        let payload = &bytes[offset + 4..offset + 4 + len];
        let stored = u32::from_le_bytes(
            bytes[offset + 4 + len..offset + frame_len]
                .try_into()
                .expect("4 bytes"),
        );
        if stored != crc32(payload) {
            return Err(PersistError::integrity(
                path,
                "complete transaction frame failed its checksum",
            ));
        }
        let txn: TxnRecord = serde_json::from_slice(payload).map_err(|err| {
            PersistError::integrity(path, format!("committed transaction did not parse: {err}"))
        })?;
        txns.push(txn);
        offset += frame_len;
        frame_ends.push(offset as u64);
    }
    Ok(WalScan {
        first_transaction_id: first,
        txns,
        frame_ends,
        torn_tail: false,
        good_len: offset as u64,
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
        return write_synced(path, &header_bytes(first_transaction_id.max(1)));
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

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), PersistError> {
    let mut file = File::create(path).map_err(|err| PersistError::io(path, err))?;
    file.write_all(bytes)
        .map_err(|err| PersistError::io(path, err))?;
    file.sync_all().map_err(|err| PersistError::io(path, err))
}

fn remove_if_present(path: &Path) -> Result<(), PersistError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(PersistError::io(path, err)),
    }
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, PersistError> {
    let file = File::open(path).map_err(|err| PersistError::io(path, err))?;
    let len = file
        .metadata()
        .map_err(|err| PersistError::io(path, err))?
        .len();
    if len > limit {
        return Err(PersistError::corrupt(path, "record exceeds the size limit"));
    }
    let mut bytes = Vec::new();
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(|err| PersistError::io(path, err))?;
    Ok(bytes)
}

fn staged_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(STAGED_SUFFIX);
    PathBuf::from(name)
}

fn character_path(dir: &Path, id: CharacterId) -> PathBuf {
    dir.join(crate::character_file_name(id))
}

fn character_id_from_name(name: &str) -> Option<CharacterId> {
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

/// Named stop points for crash tests. An armed point returns an injected I/O
/// error without cleanup, as a stopped process would; the test then drops the
/// repository and reopens from disk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CrashPoint {
    /// Changed checkpoint files are staged and synced; the manifest is not written.
    CheckpointDataBeforeManifest,
    /// The manifest is committed; staged checkpoint files are not installed.
    CheckpointManifestBeforeInstall,
    /// The replacement log is staged and not yet installed under the log name.
    LogReplacementBeforeInstall,
}

#[cfg(test)]
thread_local! {
    static CRASH_AT: std::cell::Cell<Option<CrashPoint>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn testing_crash_at(point: CrashPoint) {
    CRASH_AT.with(|armed| armed.set(Some(point)));
}

#[cfg(test)]
pub(crate) fn testing_crash_armed() -> bool {
    CRASH_AT.with(|armed| armed.get().is_some())
}

fn crash_point(point: CrashPoint, path: &Path) -> Result<(), PersistError> {
    #[cfg(test)]
    if CRASH_AT.with(|armed| armed.get()) == Some(point) {
        CRASH_AT.with(|armed| armed.set(None));
        return Err(PersistError::io(
            path,
            std::io::Error::other(format!("injected crash at {point:?}")),
        ));
    }
    let _ = (point, path);
    Ok(())
}

#[cfg(test)]
mod crc_tests {
    use super::*;

    #[test]
    fn crc32_matches_iso_hdlc_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn unissued_ranges_split_and_merge() {
        let mut ranges = IdRanges::default();
        ranges.push(1, 3);
        ranges.push(4, 6);
        assert_eq!(ranges.to_file(), vec![IdRange { first: 1, last: 6 }]);
        assert!(ranges.take(3));
        assert!(!ranges.take(3));
        assert!(ranges.contains(2) && !ranges.contains(3) && ranges.contains(4));
        assert_eq!(
            ranges.to_file(),
            vec![IdRange { first: 1, last: 2 }, IdRange { first: 4, last: 6 }]
        );
        assert!(!ranges.take(7));
    }
}
