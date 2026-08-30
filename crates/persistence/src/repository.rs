use std::path::{Path, PathBuf};

use purgatory_common::CharacterId;

use crate::atomic::{recover_if_needed, replace_file_recoverable};
use crate::character::{PERSISTENCE_SCHEMA_VERSION, PersistentCharacter};
use crate::error::PersistError;

#[must_use]
pub fn character_file_name(id: CharacterId) -> String {
    format!("char_{:016x}.json", id.raw())
}

/// File-backed character records. One logical writer per Character (the
/// persistence worker).
#[derive(Debug)]
pub struct FileCharacterRepository {
    dir: PathBuf,
}

impl FileCharacterRepository {
    pub fn open(dir: &Path) -> Result<Self, PersistError> {
        std::fs::create_dir_all(dir).map_err(|e| PersistError::io(dir, e))?;
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    #[must_use]
    pub fn path_for(&self, id: CharacterId) -> PathBuf {
        self.dir.join(character_file_name(id))
    }

    pub fn load(&self, id: CharacterId) -> Result<Option<PersistentCharacter>, PersistError> {
        let path = self.path_for(id);
        recover_if_needed(&path).map_err(|e| PersistError::io(&path, e))?;
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path).map_err(|e| PersistError::io(&path, e))?;
        let parsed: PersistentCharacter =
            serde_json::from_str(&text).map_err(|e| PersistError::json(&path, e))?;
        if parsed.character_id != id {
            return Err(PersistError::corrupt(
                &path,
                format!(
                    "file character_id {} does not match {}",
                    parsed.character_id, id
                ),
            ));
        }
        parsed.validate(&path)?;
        Ok(Some(parsed))
    }

    /// Load, or recreate a default record after corruption / missing file.
    pub fn load_or_default(&self, id: CharacterId) -> Result<PersistentCharacter, PersistError> {
        match self.load(id) {
            Ok(Some(character)) => Ok(character),
            Ok(None) => {
                let character = PersistentCharacter::new_default(id);
                self.save(&character)?;
                Ok(character)
            }
            Err(err) => {
                eprintln!("PURGATORY persist load fallback id={id} err={err}");
                let character = PersistentCharacter::new_default(id);
                self.save(&character)?;
                Ok(character)
            }
        }
    }

    pub fn save(&self, character: &PersistentCharacter) -> Result<(), PersistError> {
        let path = self.path_for(character.character_id);
        character.validate(&path)?;
        if let Ok(Some(existing)) = self.load(character.character_id)
            && character.persistence_revision <= existing.persistence_revision
        {
            return Ok(());
        }
        let mut out = character.clone();
        out.schema_version = PERSISTENCE_SCHEMA_VERSION;
        let bytes = serde_json::to_vec_pretty(&out).map_err(|e| PersistError::json(&path, e))?;
        replace_file_recoverable(&path, &bytes).map_err(|e| PersistError::io(&path, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{PERSISTENCE_SCHEMA_VERSION, PersistentCharacter};
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
            map_authored: "map.dev.second".into(),
            point_id: "default".into(),
            checkpoint_id: None,
        };
        repo.save(&character).unwrap();
        let loaded = repo.load(id).unwrap().unwrap();
        assert_eq!(loaded.restore.map_authored, "map.dev.second");
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
    fn corrupt_file_falls_back_to_default() {
        let dir = unique_dir();
        let repo = FileCharacterRepository::open(&dir).unwrap();
        let id = CharacterId::from_raw(9);
        let path = repo.path_for(id);
        std::fs::write(&path, b"{not json").unwrap();
        let character = repo.load_or_default(id).unwrap();
        assert_eq!(character.character_id, id);
        assert_eq!(character.restore.map_authored, "map.dev.footnote");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsupported_schema_is_error_then_load_or_default_recovers() {
        let dir = unique_dir();
        let repo = FileCharacterRepository::open(&dir).unwrap();
        let id = CharacterId::from_raw(11);
        let path = repo.path_for(id);
        std::fs::write(
            &path,
            r#"{"schema_version":99,"character_id":11,"persistence_revision":1,"restore":{"map_authored":"map.dev.footnote","point_id":"default"}}"#,
        )
        .unwrap();
        let err = repo.load(id).unwrap_err();
        assert!(
            matches!(err, PersistError::Schema { found: 99, .. }),
            "{err}"
        );
        let recovered = repo.load_or_default(id).unwrap();
        assert_eq!(recovered.character_id, id);
        assert_eq!(recovered.schema_version, PERSISTENCE_SCHEMA_VERSION);
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
