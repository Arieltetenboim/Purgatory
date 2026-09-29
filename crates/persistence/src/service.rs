use std::path::Path;

use purgatory_common::{CharacterId, DevLogin};

use crate::character::{PersistentCharacter, PersistentCharacterSnapshot};
use crate::domain::{
    CharacterNarrativeState, DurableCommand, DurableCommandResult, DurableContentRules, ItemRecord,
};
use crate::error::PersistError;
use crate::identity::{CharacterRosterEntry, DevIdentityStore};
#[cfg(test)]
use crate::lifecycle::LeaseBarrier;
use crate::lifecycle::{Admission, ChannelClaim, LeaseAuthority, OwnedRestore};
use crate::postgres::{self, PostgresSettings, PostgresStore};
use crate::repository::FileCharacterRepository;
use purgatory_common::ItemInstanceId;

/// File mode grants without a lease. PostgreSQL grants a generation or refuses
/// while another session's lease is still live.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionAdmission {
    Granted {
        authority: Option<LeaseAuthority>,
        restore: Box<OwnedRestore>,
    },
    Held,
    NotOwned,
}

/// Single-threaded owner of identity allocation and character state.
///
/// `open` is the pre-cutover file writer. `open_postgresql` imports supported
/// files once and then writes only to PostgreSQL.
pub struct PersistenceService {
    backend: Backend,
}

enum Backend {
    Files {
        identity: DevIdentityStore,
        repo: FileCharacterRepository,
    },
    Postgres(Box<PostgresStore>),
}

impl PersistenceService {
    pub fn open(dir: &Path) -> Result<Self, PersistError> {
        postgres::reject_file_writer_if_cut_over(dir)?;
        Ok(Self {
            backend: Backend::Files {
                identity: DevIdentityStore::open(dir)?,
                repo: FileCharacterRepository::open(dir)?,
            },
        })
    }

    pub fn open_postgresql(dir: &Path, settings: &PostgresSettings) -> Result<Self, PersistError> {
        Ok(Self {
            backend: Backend::Postgres(Box::new(PostgresStore::open(dir, settings)?)),
        })
    }

    /// Server startup. Tests keep using [`Self::open`] so an ambient database
    /// URL cannot redirect them onto a developer database.
    pub fn open_from_env(dir: &Path) -> Result<Self, PersistError> {
        match PostgresSettings::from_env()? {
            Some(settings) => Self::open_postgresql(dir, &settings),
            None => Self::open(dir),
        }
    }

    pub fn set_durable_content_rules(&mut self, rules: DurableContentRules) {
        if let Backend::Postgres(store) = &mut self.backend {
            store.set_rules(rules);
        }
    }

    /// Temporary direct-play compatibility: resolve roster slot zero, creating
    /// one compatibility entry only when empty. Production R5B uses roster/create;
    /// this remains for historical direct-play callers.
    pub fn resolve_or_create(
        &mut self,
        login: &DevLogin,
    ) -> Result<PersistentCharacter, PersistError> {
        match &mut self.backend {
            Backend::Files { identity, repo } => {
                let id = identity.lookup_or_allocate(login)?;
                repo.load_or_default(id)
            }
            Backend::Postgres(store) => store.resolve_or_create(login),
        }
    }

    /// Check the authoritative roster before touching the character record.
    /// None means not owned; no identity is allocated by entry.
    pub fn load_owned_character(
        &mut self,
        login: &DevLogin,
        id: CharacterId,
    ) -> Result<Option<PersistentCharacter>, PersistError> {
        match &mut self.backend {
            Backend::Files { identity, repo } => {
                if !identity.owns_character(login, id) {
                    return Ok(None);
                }
                repo.load_or_default(id).map(Some)
            }
            Backend::Postgres(store) => {
                if !store.owns(login, id)? {
                    return Ok(None);
                }
                store.load_character(id).map(Some)
            }
        }
    }

    pub fn save_snapshot(
        &mut self,
        snapshot: PersistentCharacterSnapshot,
    ) -> Result<(), PersistError> {
        self.save_snapshot_leased(snapshot, None)
    }

    pub fn save_snapshot_leased(
        &mut self,
        snapshot: PersistentCharacterSnapshot,
        lease: Option<&LeaseAuthority>,
    ) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Files { repo, .. } => repo.save(&snapshot.into_character()),
            Backend::Postgres(store) => store.save_restore(snapshot, lease),
        }
    }

    pub fn commit_durable(
        &mut self,
        command: &DurableCommand,
    ) -> Result<DurableCommandResult, PersistError> {
        self.commit_durable_leased(command, None)
    }

    pub fn commit_durable_leased(
        &mut self,
        command: &DurableCommand,
        lease: Option<&LeaseAuthority>,
    ) -> Result<DurableCommandResult, PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Err(PersistError::migration(
                "<postgresql>",
                "durable commands require the postgresql writer",
            )),
            Backend::Postgres(store) => store.commit(command, lease),
        }
    }

    pub fn admit(
        &mut self,
        login: &DevLogin,
        character_id: CharacterId,
    ) -> Result<SessionAdmission, PersistError> {
        match &mut self.backend {
            Backend::Files { identity, repo } => {
                if !identity.owns_character(login, character_id) {
                    return Ok(SessionAdmission::NotOwned);
                }
                let character = repo.load_or_default(character_id)?;
                Ok(SessionAdmission::Granted {
                    authority: None,
                    restore: Box::new(OwnedRestore {
                        character,
                        items: Vec::new(),
                        narrative: crate::domain::CharacterNarrativeState::default(),
                    }),
                })
            }
            Backend::Postgres(store) => Ok(match store.admit(login, character_id)? {
                Admission::Granted { authority, restore } => SessionAdmission::Granted {
                    authority: Some(authority),
                    restore,
                },
                Admission::Held => SessionAdmission::Held,
                Admission::NotOwned => SessionAdmission::NotOwned,
            }),
        }
    }

    pub fn supersede(
        &mut self,
        authority: &LeaseAuthority,
    ) -> Result<(LeaseAuthority, OwnedRestore), PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Err(PersistError::migration(
                "<postgresql>",
                "character leases require the postgresql writer",
            )),
            Backend::Postgres(store) => store.supersede(authority),
        }
    }

    pub fn renew_lease(&mut self, authority: &LeaseAuthority) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Err(PersistError::migration(
                "<postgresql>",
                "character leases require the postgresql writer",
            )),
            Backend::Postgres(store) => store.renew_lease(authority),
        }
    }

    pub fn release_lease(&mut self, authority: &LeaseAuthority) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Err(PersistError::migration(
                "<postgresql>",
                "character leases require the postgresql writer",
            )),
            Backend::Postgres(store) => store.release_lease(authority),
        }
    }

    pub fn claim_channel(
        &mut self,
        channel_id: i64,
        retire_limit: Option<i64>,
    ) -> Result<ChannelClaim, PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Err(PersistError::migration(
                "<postgresql>",
                "channel generations require the postgresql writer",
            )),
            Backend::Postgres(store) => store.claim_channel(channel_id, retire_limit),
        }
    }

    pub fn sweep_channel(
        &mut self,
        channel_id: i64,
        generation: u64,
        retire_limit: Option<i64>,
    ) -> Result<u64, PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Err(PersistError::migration(
                "<postgresql>",
                "channel generations require the postgresql writer",
            )),
            Backend::Postgres(store) => store.sweep_channel(channel_id, generation, retire_limit),
        }
    }

    pub fn renew_channel(&mut self, channel_id: i64, generation: u64) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Err(PersistError::migration(
                "<postgresql>",
                "channel generations require the postgresql writer",
            )),
            Backend::Postgres(store) => store.renew_channel(channel_id, generation),
        }
    }

    pub fn release_channel(
        &mut self,
        channel_id: i64,
        generation: u64,
    ) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Err(PersistError::migration(
                "<postgresql>",
                "channel generations require the postgresql writer",
            )),
            Backend::Postgres(store) => store.release_channel(channel_id, generation),
        }
    }

    #[cfg(test)]
    pub fn set_lease_barrier(&mut self, barrier: LeaseBarrier) {
        if let Backend::Postgres(store) = &mut self.backend {
            store.set_lease_barrier(barrier);
        }
    }

    #[cfg(test)]
    pub fn clock_moved_inside_one_statement(&mut self) -> Result<(bool, bool), PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Ok((false, false)),
            Backend::Postgres(store) => store.clock_moved_inside_one_statement(),
        }
    }

    #[cfg(test)]
    pub fn sessions_waiting_on_a_lock(&mut self) -> Result<i64, PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Ok(0),
            Backend::Postgres(store) => store.sessions_waiting_on_a_lock(),
        }
    }

    #[cfg(test)]
    pub fn remember_channel_for_test(&mut self, channel_id: i64, generation: u64) {
        if let Backend::Postgres(store) = &mut self.backend {
            store.remember_channel_for_test(channel_id, generation);
        }
    }

    #[cfg(test)]
    pub fn expire_lease_for_test(&mut self, login: &DevLogin) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Ok(()),
            Backend::Postgres(store) => store.expire_lease_for_test(login),
        }
    }

    #[cfg(test)]
    pub fn expire_channel_for_test(&mut self, channel_id: i64) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Ok(()),
            Backend::Postgres(store) => store.expire_channel_for_test(channel_id),
        }
    }

    #[cfg(test)]
    pub fn hide_next_commit_reply_for_test(&mut self) {
        if let Backend::Postgres(store) = &mut self.backend {
            store.hide_next_commit_reply();
        }
    }

    /// After the server acknowledges the next commit, drop that connection
    /// before the reply can be read back.
    #[cfg(test)]
    pub fn discard_connection_after_next_commit_for_test(&mut self) {
        if let Backend::Postgres(store) = &mut self.backend {
            store.discard_connection_after_next_commit();
        }
    }

    pub fn item(&mut self, id: ItemInstanceId) -> Result<Option<ItemRecord>, PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Err(PersistError::migration(
                "<postgresql>",
                "item records require the postgresql writer",
            )),
            Backend::Postgres(store) => store.item(id),
        }
    }

    pub fn narrative(&mut self, id: CharacterId) -> Result<CharacterNarrativeState, PersistError> {
        match &mut self.backend {
            Backend::Files { .. } => Err(PersistError::migration(
                "<postgresql>",
                "narrative records require the postgresql writer",
            )),
            Backend::Postgres(store) => store.narrative(id),
        }
    }

    pub fn lookup(&mut self, login: &DevLogin) -> Result<Option<CharacterId>, PersistError> {
        match &mut self.backend {
            Backend::Files { identity, .. } => Ok(identity.lookup(login)),
            Backend::Postgres(store) => store.lookup(login),
        }
    }

    pub fn roster(&mut self, login: &DevLogin) -> Result<Vec<CharacterRosterEntry>, PersistError> {
        match &mut self.backend {
            Backend::Files { identity, .. } => Ok(identity.roster(login)),
            Backend::Postgres(store) => store.roster(login),
        }
    }

    pub fn create_character(
        &mut self,
        login: &DevLogin,
        name: &str,
    ) -> Result<CharacterRosterEntry, PersistError> {
        match &mut self.backend {
            Backend::Files { identity, .. } => identity.create_character(login, name),
            Backend::Postgres(store) => store.create_character(login, name),
        }
    }

    pub fn owns_character(
        &mut self,
        login: &DevLogin,
        id: CharacterId,
    ) -> Result<bool, PersistError> {
        match &mut self.backend {
            Backend::Files { identity, .. } => Ok(identity.owns_character(login, id)),
            Backend::Postgres(store) => store.owns(login, id),
        }
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
        assert!(service.roster(&bob).unwrap().is_empty());
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
        assert_eq!(service.roster(&alice).unwrap(), vec![first, second]);
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
        assert!(svc.roster(&alice).unwrap().is_empty());
        assert!(
            !svc.owns_character(&alice, CharacterId::from_raw(1))
                .unwrap()
        );
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
        assert_eq!(svc.roster(&alice).unwrap(), expected);
        let mut owned = svc.roster(&alice).unwrap();
        owned.clear();
        assert_eq!(svc.roster(&alice).unwrap(), expected);
        assert!(svc.owns_character(&alice, first.character_id).unwrap());
        assert!(!svc.owns_character(&bob, first.character_id).unwrap());
        assert!(!svc.owns_character(&alice, other.character_id).unwrap());
        for entry in expected.iter().chain(std::iter::once(&other)) {
            assert!(!dir.join(character_file_name(entry.character_id)).exists());
        }
        drop(svc);
        let mut svc = PersistenceService::open(&dir).unwrap();
        assert_eq!(svc.roster(&alice).unwrap(), expected);
        assert_eq!(svc.roster(&bob).unwrap(), vec![other]);
        assert_eq!(
            svc.resolve_or_create(&alice).unwrap().character_id,
            first.character_id
        );
        assert_eq!(svc.roster(&alice).unwrap(), expected);
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
        assert!(svc.roster(&alice).unwrap().is_empty());
        svc.create_character(&bob, "000").unwrap();
        let id = svc.resolve_or_create(&alice).unwrap().character_id;
        let roster = svc.roster(&alice).unwrap();
        assert_eq!(roster.len(), 1);
        assert_eq!(roster[0].display_name.as_str(), "001");
        drop(svc);
        let mut svc = PersistenceService::open(&dir).unwrap();
        assert_eq!(svc.resolve_or_create(&alice).unwrap().character_id, id);
        assert_eq!(svc.roster(&alice).unwrap(), roster);
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
            assert_eq!(svc.roster(&alice).unwrap(), vec![first.clone()]);
            assert!(svc.roster(&bob).unwrap().is_empty());
            assert!(
                !svc.owns_character(owner, CharacterId::from_raw(first.character_id.raw() + 1))
                    .unwrap()
            );
            assert_eq!(std::fs::read(dir.join(IDENTITY_FILE_NAME)).unwrap(), before);
        }
        std::fs::remove_dir(&obstacle).unwrap();
        let retried = svc.create_character(&bob, "Retry").unwrap();
        assert_eq!(retried.character_id.raw(), first.character_id.raw() + 1);
        drop(svc);
        let mut svc = PersistenceService::open(&dir).unwrap();
        assert_eq!(svc.roster(&alice).unwrap(), vec![first]);
        assert_eq!(svc.roster(&bob).unwrap(), vec![retried]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn already_open_file_service_does_not_write_after_the_cutover_marker() {
        let dir = unique_dir();
        let mut files = PersistenceService::open(&dir).unwrap();
        let alice = DevLogin::parse("alice").unwrap();
        let entry = files.create_character(&alice, "Alice").unwrap();
        files
            .save_snapshot(PersistentCharacterSnapshot {
                character_id: entry.character_id,
                persistence_revision: 1,
                restore: purgatory_common::RestoreIntent {
                    map_authored: "map.map1".into(),
                    point_id: "default".into(),
                    checkpoint_id: None,
                },
                instance_exit: None,
            })
            .unwrap();
        let identity = std::fs::read(dir.join(IDENTITY_FILE_NAME)).unwrap();
        let character_path = dir.join(character_file_name(entry.character_id));
        let character = std::fs::read(&character_path).unwrap();
        crate::postgres::fence_file_writer(&dir).unwrap();
        let save = files.save_snapshot(PersistentCharacterSnapshot {
            character_id: entry.character_id,
            persistence_revision: 2,
            restore: purgatory_common::RestoreIntent {
                map_authored: "map.map2".into(),
                point_id: "after-cutover".into(),
                checkpoint_id: None,
            },
            instance_exit: None,
        });
        assert!(
            matches!(save, Err(PersistError::Migration { .. })),
            "{save:?}"
        );
        let created = files.create_character(&DevLogin::parse("bob").unwrap(), "Bob");
        assert!(
            matches!(created, Err(PersistError::Migration { .. })),
            "{created:?}"
        );
        assert_eq!(
            std::fs::read(dir.join(IDENTITY_FILE_NAME)).unwrap(),
            identity
        );
        assert_eq!(std::fs::read(&character_path).unwrap(), character);
        assert_eq!(files.roster(&alice).unwrap(), vec![entry]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
