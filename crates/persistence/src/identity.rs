//! Serialized DEV login → CharacterId mapping.
//!
//! Lookup, allocate, update, and persist run as one operation on a single
//! owner. Concurrent first-time logins must not share an id, and one login
//! must not race into two CharacterIds.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use purgatory_common::{CharacterId, DevLogin};
use serde::{Deserialize, Serialize};

use crate::atomic::{recover_if_needed, replace_file_recoverable};
use crate::error::PersistError;

pub const IDENTITY_SCHEMA_VERSION: u32 = 1;
pub const IDENTITY_FILE_NAME: &str = "identity.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct IdentityFile {
    schema_version: u32,
    next_character_id: u64,
    logins: BTreeMap<String, u64>,
}

impl Default for IdentityFile {
    fn default() -> Self {
        Self {
            schema_version: IDENTITY_SCHEMA_VERSION,
            next_character_id: 1,
            logins: BTreeMap::new(),
        }
    }
}

/// File-backed login map. Methods take `&mut self`; the persistence worker
/// is the sole concurrent owner.
#[derive(Debug)]
pub struct DevIdentityStore {
    path: PathBuf,
    state: IdentityFile,
}

impl DevIdentityStore {
    pub fn open(dir: &Path) -> Result<Self, PersistError> {
        std::fs::create_dir_all(dir).map_err(|e| PersistError::io(dir, e))?;
        let path = dir.join(IDENTITY_FILE_NAME);
        recover_if_needed(&path).map_err(|e| PersistError::io(&path, e))?;
        let state = if path.exists() {
            let text = std::fs::read_to_string(&path).map_err(|e| PersistError::io(&path, e))?;
            let parsed: IdentityFile =
                serde_json::from_str(&text).map_err(|e| PersistError::json(&path, e))?;
            if parsed.schema_version != IDENTITY_SCHEMA_VERSION {
                return Err(PersistError::schema(&path, parsed.schema_version));
            }
            if parsed.next_character_id == 0 {
                return Err(PersistError::corrupt(&path, "next_character_id 0"));
            }
            parsed
        } else {
            let fresh = IdentityFile::default();
            persist_state(&path, &fresh)?;
            fresh
        };
        Ok(Self { path, state })
    }

    #[must_use]
    pub fn lookup(&self, login: &DevLogin) -> Option<CharacterId> {
        self.state
            .logins
            .get(login.as_str())
            .copied()
            .map(CharacterId::from_raw)
    }

    /// Lookup, or allocate the next CharacterId, update the map, and persist.
    /// This entire sequence is one `&mut self` critical section.
    pub fn lookup_or_allocate(&mut self, login: &DevLogin) -> Result<CharacterId, PersistError> {
        if let Some(id) = self.lookup(login) {
            return Ok(id);
        }
        let raw = self.state.next_character_id;
        if raw == 0 {
            return Err(PersistError::corrupt(&self.path, "next_character_id 0"));
        }
        self.state.next_character_id = raw.saturating_add(1);
        self.state.logins.insert(login.as_str().to_string(), raw);
        if let Err(err) = persist_state(&self.path, &self.state) {
            self.state.logins.remove(login.as_str());
            self.state.next_character_id = raw;
            return Err(err);
        }
        Ok(CharacterId::from_raw(raw))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn persist_state(path: &Path, state: &IdentityFile) -> Result<(), PersistError> {
    let bytes = serde_json::to_vec_pretty(state).map_err(|e| PersistError::json(path, e))?;
    replace_file_recoverable(path, &bytes).map_err(|e| PersistError::io(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PersistError;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_dir() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "purgatory-identity-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn login(raw: &str) -> DevLogin {
        DevLogin::parse(raw).unwrap()
    }

    #[test]
    fn same_login_is_stable_across_restart() {
        let dir = unique_dir();
        let a = {
            let mut store = DevIdentityStore::open(&dir).unwrap();
            store.lookup_or_allocate(&login("alice")).unwrap()
        };
        let b = {
            let mut store = DevIdentityStore::open(&dir).unwrap();
            store.lookup_or_allocate(&login("alice")).unwrap()
        };
        assert_eq!(a, b);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn different_logins_get_different_ids() {
        let dir = unique_dir();
        let mut store = DevIdentityStore::open(&dir).unwrap();
        let a = store.lookup_or_allocate(&login("alice")).unwrap();
        let b = store.lookup_or_allocate(&login("client1")).unwrap();
        assert_ne!(a, b);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn login_is_not_a_filesystem_path() {
        let dir = unique_dir();
        let mut store = DevIdentityStore::open(&dir).unwrap();
        let id = store.lookup_or_allocate(&login("dev.local")).unwrap();
        assert!(!dir.join("dev.local").exists());
        assert!(!dir.join("dev.local.json").exists());
        let text = std::fs::read_to_string(dir.join(IDENTITY_FILE_NAME)).unwrap();
        assert!(text.contains("dev.local"));
        assert_eq!(id, CharacterId::from_raw(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsupported_identity_schema_fails_in_temp_dir() {
        let dir = unique_dir();
        let path = dir.join(IDENTITY_FILE_NAME);
        std::fs::write(
            &path,
            r#"{"schema_version":7,"next_character_id":1,"logins":{}}"#,
        )
        .unwrap();
        let err = DevIdentityStore::open(&dir).unwrap_err();
        assert!(
            matches!(err, PersistError::Schema { found: 7, .. }),
            "{err}"
        );
        let text = dir.to_string_lossy().to_ascii_lowercase();
        assert!(!text.contains("localappdata"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
