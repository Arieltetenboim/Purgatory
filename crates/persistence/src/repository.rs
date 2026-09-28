use std::path::{Path, PathBuf};

use purgatory_common::CharacterId;

use crate::character::{PersistentCharacter, PersistentCharacterSnapshot};
use crate::domain::{
    self, ClockCheckpointKind, CommitResult, DurableContentRules, MapDropRecord, OwnershipChange,
    ReservedItemIds,
};
use crate::error::PersistError;
use crate::journal;

#[must_use]
pub fn character_file_name(id: CharacterId) -> String {
    format!("char_{:016x}.json", id.raw())
}

/// File-backed character records. One logical writer per Character (the
/// persistence worker).
#[derive(Debug)]
pub struct FileCharacterRepository {
    dir: PathBuf,
    rules: DurableContentRules,
}

impl FileCharacterRepository {
    pub fn open(dir: &Path) -> Result<Self, PersistError> {
        std::fs::create_dir_all(dir).map_err(|e| PersistError::io(dir, e))?;
        let repo = Self {
            dir: dir.to_path_buf(),
            rules: DurableContentRules::new(),
        };
        repo.migrate_pending()?;
        Ok(repo)
    }

    pub fn set_durable_content_rules(&mut self, rules: DurableContentRules) {
        self.rules = rules;
    }

    #[must_use]
    pub fn path_for(&self, id: CharacterId) -> PathBuf {
        self.dir.join(character_file_name(id))
    }

    pub fn load(&self, id: CharacterId) -> Result<Option<PersistentCharacter>, PersistError> {
        self.migrate_pending()?;
        let live = journal::recover(&self.dir)?;
        let Some(character) = live.characters.get(&id.raw()) else {
            return Ok(None);
        };
        self.ensure_character_content(character)?;
        Ok(Some(character.clone()))
    }

    /// Load an existing character, or create the normal default only when the
    /// character file is genuinely absent. Existing invalid data fails closed.
    pub fn load_or_default(&self, id: CharacterId) -> Result<PersistentCharacter, PersistError> {
        match self.load(id)? {
            Some(character) => Ok(character),
            None => {
                let character = PersistentCharacter::new_default(id);
                self.save(&character)?;
                Ok(character)
            }
        }
    }

    /// Writes one complete character post-state. Callers must include owned
    /// items; a restore-only snapshot uses [`Self::save_restore_snapshot`].
    /// An unreadable existing record fails without replacement.
    pub fn save(&self, character: &PersistentCharacter) -> Result<(), PersistError> {
        self.migrate_pending()?;
        let live = journal::recover(&self.dir)?;
        if let Some(existing) = live.characters.get(&character.character_id.raw()) {
            if character.persistence_revision < existing.persistence_revision {
                return Ok(());
            }
            if character.persistence_revision == existing.persistence_revision {
                if character.body_eq(existing) {
                    return Ok(());
                }
                return Err(PersistError::integrity(
                    self.path_for(character.character_id),
                    "equal revision with different content",
                ));
            }
        }
        self.commit_ownership(OwnershipChange::character(character.clone()))
            .map(|_| ())
    }

    /// Applies restore and revision from the existing gameplay snapshot while
    /// keeping the committed item set. Phase 12C replaces this with a snapshot
    /// that already carries items.
    pub fn save_restore_snapshot(
        &self,
        snapshot: PersistentCharacterSnapshot,
    ) -> Result<(), PersistError> {
        self.migrate_pending()?;
        let live = journal::recover(&self.dir)?;
        let Some(existing) = live.characters.get(&snapshot.character_id.raw()).cloned() else {
            return self.save(&snapshot.into_character());
        };
        if snapshot.persistence_revision < existing.persistence_revision {
            return Ok(());
        }
        if snapshot.persistence_revision == existing.persistence_revision {
            if snapshot.restore == existing.restore
                && snapshot.instance_exit == existing.instance_exit
            {
                return Ok(());
            }
            return Err(PersistError::integrity(
                self.path_for(snapshot.character_id),
                "equal revision with different content",
            ));
        }
        let mut next = existing;
        next.persistence_revision = snapshot.persistence_revision;
        next.restore = snapshot.restore;
        next.instance_exit = snapshot.instance_exit;
        self.save(&next)
    }

    pub fn commit_ownership(&self, change: OwnershipChange) -> Result<CommitResult, PersistError> {
        self.migrate_pending()?;
        journal::commit_ownership(&self.dir, &self.rules, change)
    }

    pub fn reserve_item_instance_ids(&self, count: u32) -> Result<ReservedItemIds, PersistError> {
        self.migrate_pending()?;
        journal::reserve_ids(&self.dir, count)
    }

    pub fn checkpoint_active_clock(
        &self,
        tick: u64,
        kind: ClockCheckpointKind,
    ) -> Result<CommitResult, PersistError> {
        self.migrate_pending()?;
        journal::checkpoint_clock(&self.dir, tick, kind)
    }

    pub fn active_clock_tick(&self) -> Result<u64, PersistError> {
        self.migrate_pending()?;
        Ok(journal::recover(&self.dir)?.clock_tick)
    }

    pub fn map_drops(&self) -> Result<Vec<MapDropRecord>, PersistError> {
        self.migrate_pending()?;
        let live = journal::recover(&self.dir)?;
        let mut drops: Vec<_> = live.drops.into_values().collect();
        drops.sort_by_key(|drop| drop.item_instance_id.raw());
        for drop in &drops {
            let path = self.dir.join("map_drops.json");
            domain::validate_drop_content(drop, &self.rules, &path)?;
        }
        Ok(drops)
    }

    pub fn compact_durable_log(&self) -> Result<(), PersistError> {
        self.migrate_pending()?;
        journal::compact(&self.dir)
    }

    fn migrate_pending(&self) -> Result<(), PersistError> {
        loop {
            let pending = journal::pending_migrations(&self.dir)?;
            let Some(v1) = pending.into_iter().next() else {
                return Ok(());
            };
            journal::migrate_one(&self.dir, &v1)?;
        }
    }

    fn ensure_character_content(
        &self,
        character: &PersistentCharacter,
    ) -> Result<(), PersistError> {
        let path = self.path_for(character.character_id);
        for item in &character.items {
            domain::validate_item_content(
                item.item_instance_id,
                item.definition_content_id,
                item.quantity,
                item.location,
                &self.rules,
                &path,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::PersistentCharacter;
    use purgatory_common::RestoreIntent;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_dir() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "purgatory-repo-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn roundtrip_and_stale_revision_ignored() {
        let dir = unique_dir();
        let repo = FileCharacterRepository::open(&dir).unwrap();
        let id = CharacterId::from_raw(7);
        let mut character = PersistentCharacter::new_default(id);
        character.persistence_revision = 3;
        character.restore = RestoreIntent {
            map_authored: "map.map2".into(),
            point_id: "default".into(),
            checkpoint_id: None,
        };
        repo.save(&character).unwrap();
        let loaded = repo.load(id).unwrap().unwrap();
        assert_eq!(loaded.restore.map_authored, "map.map2");
        assert_eq!(loaded.persistence_revision, 3);

        let mut stale = loaded.clone();
        stale.persistence_revision = 2;
        stale.restore.point_id = "ignored".into();
        repo.save(&stale).unwrap();
        let again = repo.load(id).unwrap().unwrap();
        assert_eq!(again.persistence_revision, 3);
        assert_eq!(again.restore.point_id, "default");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_load_or_default_creates_default() {
        let dir = unique_dir();
        let repo = FileCharacterRepository::open(&dir).unwrap();
        let id = CharacterId::from_raw(9);
        let path = repo.path_for(id);
        assert!(!path.exists());
        let character = repo.load_or_default(id).unwrap();
        assert_eq!(character.character_id, id);
        assert_eq!(character.restore.map_authored, "map.map1");
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_json_fails_closed_and_preserves_original_bytes() {
        let dir = unique_dir();
        let repo = FileCharacterRepository::open(&dir).unwrap();
        let id = CharacterId::from_raw(10);
        let path = repo.path_for(id);
        let original = b"{not json".to_vec();
        std::fs::write(&path, &original).unwrap();
        assert!(matches!(
            repo.load_or_default(id),
            Err(PersistError::Json { .. })
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsupported_schema_fails_closed_and_preserves_original_bytes() {
        let dir = unique_dir();
        let repo = FileCharacterRepository::open(&dir).unwrap();
        let id = CharacterId::from_raw(11);
        let path = repo.path_for(id);
        let original = br#"{"schema_version":99,"character_id":11,"persistence_revision":1,"restore":{"map_authored":"map.map1","point_id":"default"}}"#.to_vec();
        std::fs::write(&path, &original).unwrap();
        let err = repo.load_or_default(id).unwrap_err();
        assert!(
            matches!(err, PersistError::Schema { found: 99, .. }),
            "{err}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn character_id_mismatch_fails_closed_and_preserves_original_bytes() {
        let dir = unique_dir();
        let repo = FileCharacterRepository::open(&dir).unwrap();
        let id = CharacterId::from_raw(12);
        let path = repo.path_for(id);
        let original = br#"{"schema_version":1,"character_id":13,"persistence_revision":1,"restore":{"map_authored":"map.map1","point_id":"default"}}"#.to_vec();
        std::fs::write(&path, &original).unwrap();
        let err = repo.load_or_default(id).unwrap_err();
        assert!(matches!(err, PersistError::Corrupt { .. }), "{err}");
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failure_injection_never_uses_developer_persist_tree() {
        let dir = unique_dir();
        let temp = std::env::temp_dir();
        assert!(
            dir.starts_with(&temp),
            "must use process temp ({}) not developer persist: {}",
            temp.display(),
            dir.display()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bak_only_state_recovers_in_temp_dir() {
        let dir = unique_dir();
        let repo = FileCharacterRepository::open(&dir).unwrap();
        let id = CharacterId::from_raw(13);
        let mut character = PersistentCharacter::new_default(id);
        character.persistence_revision = 4;
        repo.save(&character).unwrap();
        let path = repo.path_for(id);
        let bak = {
            let mut s = path.as_os_str().to_os_string();
            s.push(".bak");
            PathBuf::from(s)
        };
        std::fs::rename(&path, &bak).unwrap();
        assert!(!path.exists());
        let loaded = repo.load(id).unwrap().unwrap();
        assert_eq!(loaded.persistence_revision, 4);
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn character_filename_is_not_login() {
        assert_eq!(
            character_file_name(CharacterId::from_raw(1)),
            "char_0000000000000001.json"
        );
    }
}
