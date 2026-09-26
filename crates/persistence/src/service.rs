use std::path::Path;

use purgatory_common::{CharacterId, DevLogin};

use crate::character::{PersistentCharacter, PersistentCharacterSnapshot};
use crate::error::PersistError;
use crate::identity::{CharacterRosterEntry, DevIdentityStore};
use crate::repository::FileCharacterRepository;

/// Single-threaded owner of identity allocation and character files.
pub struct PersistenceService {
    identity: DevIdentityStore,
    repo: FileCharacterRepository,
}

impl PersistenceService {
    pub fn open(dir: &Path) -> Result<Self, PersistError> {
        Ok(Self {
            identity: DevIdentityStore::open(dir)?,
            repo: FileCharacterRepository::open(dir)?,
        })
    }

    /// Temporary direct-play compatibility: resolve roster slot zero, creating
    /// one compatibility entry only when empty. Production R5B uses roster/create;
    /// this remains for historical direct-play callers.
    pub fn resolve_or_create(
        &mut self,
        login: &DevLogin,
    ) -> Result<PersistentCharacter, PersistError> {
        let id = self.identity.lookup_or_allocate(login)?;
        self.repo.load_or_default(id)
    }

    /// Check the authoritative roster before touching the exact character file.
    /// None means not owned; no identity is allocated by entry.
    pub fn load_owned_character(
        &self,
        login: &DevLogin,
        id: CharacterId,
    ) -> Result<Option<PersistentCharacter>, PersistError> {
        if !self.owns_character(login, id) {
            return Ok(None);
        }
        self.repo.load_or_default(id).map(Some)
    }

    pub fn save_snapshot(
        &mut self,
        snapshot: PersistentCharacterSnapshot,
    ) -> Result<(), PersistError> {
        self.repo.save(&snapshot.into_character())
    }

    #[must_use]
    pub fn lookup(&self, login: &DevLogin) -> Option<CharacterId> {
        self.identity.lookup(login)
    }

    #[must_use]
    pub fn roster(&self, login: &DevLogin) -> Vec<CharacterRosterEntry> {
        self.identity.roster(login)
    }

    pub fn create_character(
        &mut self,
        login: &DevLogin,
        name: &str,
    ) -> Result<CharacterRosterEntry, PersistError> {
        self.identity.create_character(login, name)
    }

    #[must_use]
    pub fn owns_character(&self, login: &DevLogin, id: CharacterId) -> bool {
        self.identity.owns_character(login, id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CreateCharacterRejection, IDENTITY_FILE_NAME, character_file_name};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_dir() -> std::path::PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "purgatory-service-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_owned_character_selects_exact_id_and_never_allocates() {
        let dir = unique_dir();
        let mut service = PersistenceService::open(&dir).unwrap();
        let alice = DevLogin::parse("alice").unwrap();
        let bob = DevLogin::parse("bob").unwrap();
        let first = service.create_character(&alice, "First").unwrap();
        let second = service.create_character(&alice, "Second").unwrap();
        assert!(
            service
                .load_owned_character(&bob, second.character_id)
                .unwrap()
                .is_none()
        );
        assert!(service.roster(&bob).is_empty());
        assert!(!dir.join(character_file_name(second.character_id)).exists());
        let loaded = service
            .load_owned_character(&alice, second.character_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.character_id, second.character_id);
        assert_ne!(loaded.character_id, first.character_id);
        assert!(
            service
                .load_owned_character(&alice, CharacterId::from_raw(99999))
                .unwrap()
                .is_none()
        );
        assert_eq!(service.roster(&alice), vec![first, second]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn first_login_creates_character() {
        let dir = unique_dir();
        let mut svc = PersistenceService::open(&dir).unwrap();
        let login = DevLogin::parse("dev.local").unwrap();
        let a = svc.resolve_or_create(&login).unwrap();
        let b = svc.resolve_or_create(&login).unwrap();
        assert_eq!(a.character_id, b.character_id);
        assert_eq!(a.restore.map_authored, "map.map1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn roster_creation_ownership_and_restart() {
        let dir = unique_dir();
        let mut svc = PersistenceService::open(&dir).unwrap();
        let alice = DevLogin::parse("alice").unwrap();
        let bob = DevLogin::parse("bob").unwrap();
        assert!(svc.roster(&alice).is_empty());
        assert!(!svc.owns_character(&alice, CharacterId::from_raw(1)));
        assert!(matches!(
            svc.create_character(&alice, "ab"),
            Err(PersistError::CreateRejected(
                CreateCharacterRejection::InvalidName(_)
            ))
        ));
        let first = svc.create_character(&alice, "Ariel").unwrap();
        assert_ne!(first.character_id.raw(), 0);
        assert_eq!(first.display_name.as_str(), "Ariel");
        for owner in [&alice, &bob] {
            for name in ["ARIEL", "Ariel"] {
                assert!(matches!(
                    svc.create_character(owner, name),
                    Err(PersistError::CreateRejected(
                        CreateCharacterRejection::NameTaken
                    ))
                ));
            }
        }
        let second = svc.create_character(&alice, "Second").unwrap();
        let third = svc.create_character(&alice, "Third").unwrap();
        assert_ne!(first.character_id, second.character_id);
        assert_ne!(second.character_id, third.character_id);
        assert!(matches!(
            svc.create_character(&alice, "Fourth"),
            Err(PersistError::CreateRejected(
                CreateCharacterRejection::RosterFull
            ))
        ));
        let other = svc.create_character(&bob, "Other").unwrap();
        assert_eq!(other.character_id.raw(), third.character_id.raw() + 1);
        let expected = vec![first.clone(), second, third];
        assert_eq!(svc.roster(&alice), expected);
        let mut owned = svc.roster(&alice);
        owned.clear();
        assert_eq!(svc.roster(&alice), expected);
        assert!(svc.owns_character(&alice, first.character_id));
        assert!(!svc.owns_character(&bob, first.character_id));
        assert!(!svc.owns_character(&alice, other.character_id));
        for entry in expected.iter().chain(std::iter::once(&other)) {
            assert!(!dir.join(character_file_name(entry.character_id)).exists());
        }
        drop(svc);
        let mut svc = PersistenceService::open(&dir).unwrap();
        assert_eq!(svc.roster(&alice), expected);
        assert_eq!(svc.roster(&bob), vec![other]);
        assert_eq!(
            svc.resolve_or_create(&alice).unwrap().character_id,
            first.character_id
        );
        assert_eq!(svc.roster(&alice), expected);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn explicit_empty_roster_compatibility_skips_taken_names_and_survives_restart() {
        let dir = unique_dir();
        std::fs::write(
            dir.join(IDENTITY_FILE_NAME),
            r#"{"schema_version":2,"next_character_id":1,"logins":{"alice":[]}}"#,
        )
        .unwrap();
        let alice = DevLogin::parse("alice").unwrap();
        let bob = DevLogin::parse("bob").unwrap();
        let mut svc = PersistenceService::open(&dir).unwrap();
        assert!(svc.roster(&alice).is_empty());
        svc.create_character(&bob, "000").unwrap();
        let id = svc.resolve_or_create(&alice).unwrap().character_id;
        let roster = svc.roster(&alice);
        assert_eq!(roster.len(), 1);
        assert_eq!(roster[0].display_name.as_str(), "001");
        drop(svc);
        let mut svc = PersistenceService::open(&dir).unwrap();
        assert_eq!(svc.resolve_or_create(&alice).unwrap().character_id, id);
        assert_eq!(svc.roster(&alice), roster);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_create_preserves_rosters_allocator_and_name_availability() {
        let dir = unique_dir();
        let alice = DevLogin::parse("alice").unwrap();
        let bob = DevLogin::parse("bob").unwrap();
        let mut svc = PersistenceService::open(&dir).unwrap();
        let first = svc.create_character(&alice, "First").unwrap();
        let before = std::fs::read(dir.join(IDENTITY_FILE_NAME)).unwrap();
        // A directory at the staging path deterministically fails File::create
        // on all supported platforms, without changing developer persistence.
        let obstacle = crate::atomic::tmp_path(&dir.join(IDENTITY_FILE_NAME));
        std::fs::create_dir(&obstacle).unwrap();
        for owner in [&alice, &bob] {
            assert!(matches!(
                svc.create_character(owner, "Retry"),
                Err(PersistError::Io { .. })
            ));
            assert_eq!(svc.roster(&alice), vec![first.clone()]);
            assert!(svc.roster(&bob).is_empty());
            assert!(
                !svc.owns_character(owner, CharacterId::from_raw(first.character_id.raw() + 1))
            );
            assert_eq!(std::fs::read(dir.join(IDENTITY_FILE_NAME)).unwrap(), before);
        }
        std::fs::remove_dir(&obstacle).unwrap();
        let retried = svc.create_character(&bob, "Retry").unwrap();
        assert_eq!(retried.character_id.raw(), first.character_id.raw() + 1);
        drop(svc);
        let svc = PersistenceService::open(&dir).unwrap();
        assert_eq!(svc.roster(&alice), vec![first]);
        assert_eq!(svc.roster(&bob), vec![retried]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
