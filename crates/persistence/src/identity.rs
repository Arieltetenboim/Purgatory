//! Serialized DEV profile → ordered character roster. One mutable owner
//! allocates IDs, checks global names, and commits identity metadata.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use purgatory_common::{
    CHARACTER_NAME_MAX_LEN, CHARACTER_NAME_MIN_LEN, CharacterId, CharacterName, DevLogin,
};
use serde::{Deserialize, Serialize};

use crate::atomic::{recover_if_needed, replace_file_recoverable};
use crate::error::{CreateCharacterRejection, PersistError};

pub const IDENTITY_SCHEMA_VERSION: u32 = 2;
pub const IDENTITY_FILE_NAME: &str = "identity.json";
pub const MAX_ROSTER_SIZE: usize = 3;

/// Identity/selection metadata only; gameplay state remains in character files.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CharacterRosterEntry {
    pub character_id: CharacterId,
    pub display_name: CharacterName,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct IdentityFile {
    schema_version: u32,
    next_character_id: u64,
    #[serde(deserialize_with = "deserialize_logins")]
    logins: BTreeMap<String, Vec<CharacterRosterEntry>>,
}

#[derive(Deserialize)]
struct LegacyIdentityFile {
    next_character_id: u64,
    #[serde(deserialize_with = "deserialize_logins")]
    logins: BTreeMap<String, u64>,
}

// serde's default map decoding overwrites duplicate keys. Identity ownership
// must reject that ambiguity in both schemas rather than discard Characters.
fn deserialize_logins<'de, D, T>(deserializer: D) -> Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Logins<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Logins<T> {
        type Value = BTreeMap<String, T>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("unique DEV login keys")
        }

        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut logins = BTreeMap::new();
            while let Some((login, value)) = map.next_entry::<String, T>()? {
                if logins.insert(login, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate DEV login key"));
                }
            }
            Ok(logins)
        }
    }
    deserializer.deserialize_map(Logins(std::marker::PhantomData))
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

/// File-backed roster map. Mutations take `&mut self`; the persistence worker
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
            #[derive(Deserialize)]
            struct Header {
                schema_version: u32,
            }
            let header: Header =
                serde_json::from_str(&text).map_err(|e| PersistError::json(&path, e))?;
            let parsed = match header.schema_version {
                1 => migrate_v1(&path, &text)?,
                IDENTITY_SCHEMA_VERSION => serde_json::from_str(&text).map_err(|e| {
                    PersistError::corrupt(&path, format!("invalid identity state: {e}"))
                })?,
                version => return Err(PersistError::schema(&path, version)),
            };
            validate_state(&path, &parsed)?;
            if header.schema_version == 1 {
                persist_state(&path, &parsed)?;
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
            .and_then(|roster| roster.first())
            .map(|entry| entry.character_id)
    }

    #[must_use]
    pub fn roster(&self, login: &DevLogin) -> Vec<CharacterRosterEntry> {
        self.state
            .logins
            .get(login.as_str())
            .cloned()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn owns_character(&self, login: &DevLogin, id: CharacterId) -> bool {
        self.state
            .logins
            .get(login.as_str())
            .is_some_and(|roster| roster.iter().any(|entry| entry.character_id == id))
    }

    pub fn create_character(
        &mut self,
        login: &DevLogin,
        name: &str,
    ) -> Result<CharacterRosterEntry, PersistError> {
        let display_name = CharacterName::parse(name).map_err(|err| {
            PersistError::CreateRejected(CreateCharacterRejection::InvalidName(err))
        })?;
        if self
            .state
            .logins
            .get(login.as_str())
            .is_some_and(|roster| roster.len() >= MAX_ROSTER_SIZE)
        {
            return Err(PersistError::CreateRejected(
                CreateCharacterRejection::RosterFull,
            ));
        }
        let key = display_name.uniqueness_key();
        if self
            .entries()
            .any(|entry| entry.display_name.uniqueness_key() == key)
        {
            return Err(PersistError::CreateRejected(
                CreateCharacterRejection::NameTaken,
            ));
        }
        let raw = self.state.next_character_id;
        // MAX is a saturated exhaustion marker only when MAX itself is owned.
        if raw == u64::MAX && self.entries().any(|entry| entry.character_id.raw() == raw) {
            return Err(PersistError::CharacterIdsExhausted);
        }
        let entry = CharacterRosterEntry {
            character_id: CharacterId::from_raw(raw),
            display_name,
        };
        let mut candidate = self.state.clone();
        candidate.next_character_id = raw.saturating_add(1);
        candidate
            .logins
            .entry(login.as_str().to_owned())
            .or_default()
            .push(entry.clone());
        persist_state(&self.path, &candidate)?;
        self.state = candidate;
        Ok(entry)
    }

    fn entries(&self) -> impl Iterator<Item = &CharacterRosterEntry> {
        self.state.logins.values().flatten()
    }

    /// Temporary direct-play compatibility until selected-character entry exists.
    /// Resolve slot zero, or create one deterministic unused compatibility name
    /// through the same capacity, uniqueness, allocation and commit operation.
    pub fn lookup_or_allocate(&mut self, login: &DevLogin) -> Result<CharacterId, PersistError> {
        if let Some(id) = self.lookup(login) {
            return Ok(id);
        }
        let used: HashSet<_> = self
            .entries()
            .map(|entry| entry.display_name.uniqueness_key())
            .collect();
        for ordinal in 0..=used.len() as u64 {
            let name =
                compatibility_name(ordinal).ok_or(PersistError::CompatibilityNamesExhausted)?;
            if !used.contains(&name.uniqueness_key()) {
                return self
                    .create_character(login, name.as_str())
                    .map(|entry| entry.character_id);
            }
        }
        Err(PersistError::CompatibilityNamesExhausted)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Enumerate all case-insensitive names: 000..zzz, 0000..zzzz, ... (base 36).
/// The ordinal is unrelated to CharacterId; even u64::MAX IDs can migrate.
fn compatibility_name(mut ordinal: u64) -> Option<CharacterName> {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    for width in CHARACTER_NAME_MIN_LEN..=CHARACTER_NAME_MAX_LEN {
        let count = 36_u64.pow(width as u32);
        if ordinal >= count {
            ordinal -= count;
            continue;
        }
        let mut name = vec![b'0'; width];
        for ch in name.iter_mut().rev() {
            *ch = DIGITS[(ordinal % 36) as usize];
            ordinal /= 36;
        }
        return CharacterName::parse(std::str::from_utf8(&name).ok()?).ok();
    }
    None
}

fn migrate_v1(path: &Path, text: &str) -> Result<IdentityFile, PersistError> {
    let legacy: LegacyIdentityFile = serde_json::from_str(text)
        .map_err(|e| PersistError::corrupt(path, format!("invalid v1 identity state: {e}")))?;
    let mut mappings: Vec<_> = legacy.logins.into_iter().collect();
    mappings.sort_by(|(login_a, id_a), (login_b, id_b)| id_a.cmp(id_b).then(login_a.cmp(login_b)));
    let mut state = IdentityFile {
        next_character_id: legacy.next_character_id,
        ..IdentityFile::default()
    };
    for (ordinal, (login, raw)) in mappings.into_iter().enumerate() {
        let display_name = u64::try_from(ordinal)
            .ok()
            .and_then(compatibility_name)
            .ok_or_else(|| PersistError::Migration {
                path: path.to_owned(),
                reason: "compatibility name namespace exhausted".into(),
            })?;
        state.logins.insert(
            login,
            vec![CharacterRosterEntry {
                character_id: CharacterId::from_raw(raw),
                display_name,
            }],
        );
    }
    Ok(state)
}

fn validate_state(path: &Path, state: &IdentityFile) -> Result<(), PersistError> {
    if state.next_character_id == 0 {
        return Err(PersistError::corrupt(path, "next_character_id 0"));
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for (login, roster) in &state.logins {
        if DevLogin::parse(login).is_err() {
            return Err(PersistError::corrupt(path, "invalid DEV login"));
        }
        if roster.len() > MAX_ROSTER_SIZE {
            return Err(PersistError::corrupt(
                path,
                "roster exceeds three characters",
            ));
        }
        for entry in roster {
            let raw = entry.character_id.raw();
            if raw == 0 || !ids.insert(raw) {
                return Err(PersistError::corrupt(path, "zero or duplicate CharacterId"));
            }
            if raw >= state.next_character_id
                && !(raw == u64::MAX && state.next_character_id == u64::MAX)
            {
                return Err(PersistError::corrupt(
                    path,
                    "next_character_id would reuse an owned CharacterId",
                ));
            }
            // Deserialization already enforces CharacterName syntax without normalization.
            if !names.insert(entry.display_name.uniqueness_key()) {
                return Err(PersistError::corrupt(path, "duplicate character name"));
            }
        }
    }
    Ok(())
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

    #[test]
    fn migration_is_id_ordered_preserves_large_ids_and_character_files() {
        let dir = unique_dir();
        let path = dir.join(IDENTITY_FILE_NAME);
        let high = u64::MAX;
        let old = serde_json::json!({"schema_version": 1, "next_character_id": high,
            "logins": {"alice": high, "zed": 7, "bob": high - 1}});
        std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        let character_path = dir.join(crate::character_file_name(CharacterId::from_raw(7)));
        let character_bytes = serde_json::to_vec(&crate::PersistentCharacter::new_default(
            CharacterId::from_raw(7),
        ))
        .unwrap();
        std::fs::write(&character_path, &character_bytes).unwrap();
        let mut store = DevIdentityStore::open(&dir).unwrap();
        for (owner, id, name) in [
            ("zed", 7, "000"),
            ("bob", high - 1, "001"),
            ("alice", high, "002"),
        ] {
            let roster = store.roster(&login(owner));
            assert_eq!(roster.len(), 1);
            assert_eq!(roster[0].character_id.raw(), id);
            assert_eq!(roster[0].display_name.as_str(), name);
            assert!(CharacterName::parse(name).is_ok());
        }
        assert_eq!(std::fs::read(&character_path).unwrap(), character_bytes);
        assert_eq!(
            store.lookup_or_allocate(&login("alice")).unwrap().raw(),
            high
        );
        assert!(matches!(
            store.create_character(&login("new"), "Fresh"),
            Err(PersistError::CharacterIdsExhausted)
        ));
        let migrated = std::fs::read(&path).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&migrated).unwrap();
        assert_eq!(json["schema_version"], 2);
        drop(store);
        let store = DevIdentityStore::open(&dir).unwrap();
        assert_eq!(store.lookup(&login("alice")).unwrap().raw(), high);
        assert_eq!(std::fs::read(&path).unwrap(), migrated);
        assert_eq!(std::fs::read(&character_path).unwrap(), character_bytes);
        // Input JSON property order does not determine migration names.
        let reordered = format!(
            r#"{{"schema_version":1,"next_character_id":{high},"logins":{{"zed":7,"bob":{},"alice":{high}}}}}"#,
            high - 1
        );
        assert_eq!(
            serde_json::to_value(migrate_v1(&path, &reordered).unwrap()).unwrap(),
            json
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn migration_failure_keeps_v1_recoverable_and_retries() {
        let dir = unique_dir();
        let path = dir.join(IDENTITY_FILE_NAME);
        let old = br#"{"schema_version":1,"next_character_id":8,"logins":{"alice":7}}"#;
        // Exercise the existing backup recovery path before migration.
        std::fs::write(crate::atomic::bak_path(&path), old).unwrap();
        let obstacle = crate::atomic::tmp_path(&path);
        std::fs::create_dir(&obstacle).unwrap();
        assert!(matches!(
            DevIdentityStore::open(&dir),
            Err(PersistError::Io { .. })
        ));
        assert_eq!(std::fs::read(&path).unwrap(), old);
        std::fs::remove_dir(&obstacle).unwrap();
        let store = DevIdentityStore::open(&dir).unwrap();
        assert_eq!(store.lookup(&login("alice")).unwrap().raw(), 7);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn compatibility_names_cover_lengths_without_collisions_and_report_exhaustion() {
        let mut offset = 0;
        for width in CHARACTER_NAME_MIN_LEN..=CHARACTER_NAME_MAX_LEN {
            let count = 36_u64.pow(width as u32);
            assert_eq!(
                compatibility_name(offset).unwrap().as_str(),
                "0".repeat(width)
            );
            assert_eq!(
                compatibility_name(offset + count - 1).unwrap().as_str(),
                "z".repeat(width)
            );
            offset += count;
        }
        assert!(compatibility_name(offset).is_none());
        assert!(compatibility_name(u64::MAX).is_none());
        let keys: HashSet<_> = (0..50_000)
            .map(|n| compatibility_name(n).unwrap().uniqueness_key())
            .collect();
        assert_eq!(keys.len(), 50_000);
    }

    #[test]
    fn allocator_can_mint_max_once_and_never_wraps_or_reuses_it() {
        let dir = unique_dir();
        let path = dir.join(IDENTITY_FILE_NAME);
        std::fs::write(
            &path,
            format!(
                r#"{{"schema_version":2,"next_character_id":{},"logins":{{}}}}"#,
                u64::MAX
            ),
        )
        .unwrap();
        let mut store = DevIdentityStore::open(&dir).unwrap();
        let last = store.create_character(&login("alice"), "Last").unwrap();
        assert_eq!(last.character_id.raw(), u64::MAX);
        drop(store);
        let mut store = DevIdentityStore::open(&dir).unwrap();
        assert!(matches!(
            store.create_character(&login("bob"), "Next"),
            Err(PersistError::CharacterIdsExhausted)
        ));
        assert!(store.roster(&login("bob")).is_empty());
        assert_eq!(store.roster(&login("alice")), vec![last]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_v2_is_rejected_without_rewriting() {
        use serde_json::json;
        let entry = |id, name| json!({"character_id":id,"display_name":name});
        let cases = [
            json!({"schema_version":2,"next_character_id":0,"logins":{}}),
            json!({"schema_version":2,"next_character_id":5,"logins":{"alice":[entry(0,"Zero")]}}),
            json!({"schema_version":2,"next_character_id":5,"logins":{"alice":[entry(1,"One")],"bob":[entry(1,"Two")]}}),
            json!({"schema_version":2,"next_character_id":5,"logins":{"alice":[entry(1,"One"),entry(1,"Two")]}}),
            json!({"schema_version":2,"next_character_id":5,"logins":{"alice":[entry(1,"Ariel")],"bob":[entry(2,"ARIEL")]}}),
            json!({"schema_version":2,"next_character_id":5,"logins":{"alice":[entry(1,"One"),entry(2,"Two"),entry(3,"Three"),entry(4,"Four")]}}),
            json!({"schema_version":2,"next_character_id":1,"logins":{"alice":[entry(1,"One")]}}),
            json!({"schema_version":2,"next_character_id":2,"logins":{"alice":[entry(4,"Four")]}}),
            json!({"schema_version":2,"next_character_id":5,"logins":{"../bad":[entry(1,"One")]}}),
        ];
        let dir = unique_dir();
        let path = dir.join(IDENTITY_FILE_NAME);
        for state in cases.into_iter().chain(["ab", "abcdefghijklM", "A b", "A_b", "A.b", "Äbc"].into_iter().map(|name|
            json!({"schema_version":2,"next_character_id":5,"logins":{"alice":[entry(1,name)]}}))) {
            let bytes = serde_json::to_vec(&state).unwrap();
            std::fs::write(&path, &bytes).unwrap();
            assert!(matches!(DevIdentityStore::open(&dir), Err(PersistError::Corrupt { .. })), "{state}");
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_v1_ownership_is_not_migrated_or_repaired() {
        let dir = unique_dir();
        let path = dir.join(IDENTITY_FILE_NAME);
        for old in [
            r#"{"schema_version":1,"next_character_id":3,"logins":{"alice":1,"bob":1}}"#,
            r#"{"schema_version":1,"next_character_id":3,"logins":{"alice":0}}"#,
            r#"{"schema_version":1,"next_character_id":0,"logins":{}}"#,
            r#"{"schema_version":1,"next_character_id":1,"logins":{"alice":2}}"#,
        ] {
            std::fs::write(&path, old).unwrap();
            assert!(matches!(
                DevIdentityStore::open(&dir),
                Err(PersistError::Corrupt { .. })
            ));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), old);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn duplicate_login_keys_never_silently_discard_characters() {
        let dir = unique_dir();
        let path = dir.join(IDENTITY_FILE_NAME);
        for text in [
            r#"{"schema_version":1,"next_character_id":3,"logins":{"alice":1,"alice":2}}"#,
            r#"{"schema_version":2,"next_character_id":3,"logins":{"alice":[{"character_id":1,"display_name":"One"}],"alice":[{"character_id":2,"display_name":"Two"}]}}"#,
        ] {
            std::fs::write(&path, text).unwrap();
            assert!(matches!(
                DevIdentityStore::open(&dir),
                Err(PersistError::Corrupt { .. })
            ));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
