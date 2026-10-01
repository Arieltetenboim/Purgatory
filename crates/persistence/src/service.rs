use purgatory_common::{CharacterId, DevLogin, ItemInstanceId};

use crate::character::{PersistentCharacter, PersistentCharacterSnapshot};
use crate::domain::{
    CharacterNarrativeState, DurableCommand, DurableCommandResult, DurableContentRules, ItemRecord,
};
use crate::error::PersistError;
use crate::identity::CharacterRosterEntry;
#[cfg(test)]
use crate::lifecycle::LeaseBarrier;
use crate::lifecycle::{Admission, ChannelClaim, LeaseAuthority, OwnedRestore};
use crate::memory::MemoryStore;
use crate::postgres::{PostgresSettings, PostgresStore};

/// PostgreSQL grants a generation or refuses while another session's lease is
/// still live. The unit fixture grants without a lease.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionAdmission {
    Granted {
        authority: Option<LeaseAuthority>,
        restore: Box<OwnedRestore>,
    },
    Held,
    NotOwned,
}

/// Single-threaded owner of development users and character state.
///
/// Server startup uses [`Self::open_from_env`] on the persistence worker and
/// only reopens an initialized database. Creating or resetting that database
/// is a separate administration operation. [`Self::unit_fixture`] is an
/// in-memory test double and does not write files.
pub struct PersistenceService {
    backend: Backend,
}

enum Backend {
    Postgres(Box<PostgresStore>),
    Fixture(MemoryStore),
}

fn missing_database_url() -> PersistError {
    PersistError::storage(
        "PURGATORY_DATABASE_URL is required; there is no file-backed game database",
    )
}

fn fixture_has_no_durable_store(what: &str) -> PersistError {
    PersistError::migration(
        "<postgresql>",
        format!("{what} require the postgresql writer"),
    )
}

impl PersistenceService {
    /// In-memory roster for unit tests. Does not open a database or write files.
    pub fn unit_fixture() -> Self {
        Self {
            backend: Backend::Fixture(MemoryStore::new()),
        }
    }

    pub fn open_postgresql(settings: &PostgresSettings) -> Result<Self, PersistError> {
        Ok(Self {
            backend: Backend::Postgres(Box::new(PostgresStore::open(settings)?)),
        })
    }

    /// Apply migrations and empty durable metadata inside an existing database.
    /// Does not create the physical database and does not read legacy files.
    pub fn bootstrap_postgresql(settings: &PostgresSettings) -> Result<(), PersistError> {
        PostgresStore::bootstrap(settings)
    }

    /// Server startup. PostgreSQL is required. The dedicated server calls this
    /// on the persistence worker, outside the Tokio runtime. A missing URL
    /// does not open a file writer.
    pub fn open_from_env() -> Result<Self, PersistError> {
        Self::open_configured(PostgresSettings::from_env()?)
    }

    pub fn bootstrap_from_env() -> Result<(), PersistError> {
        match PostgresSettings::from_env()? {
            Some(settings) => Self::bootstrap_postgresql(&settings),
            None => Err(missing_database_url()),
        }
    }

    fn open_configured(settings: Option<PostgresSettings>) -> Result<Self, PersistError> {
        match settings {
            Some(settings) => Self::open_postgresql(&settings),
            None => Err(missing_database_url()),
        }
    }

    pub fn set_durable_content_rules(&mut self, rules: DurableContentRules) {
        if let Backend::Postgres(store) = &mut self.backend {
            store.set_rules(rules);
        }
    }

    /// Insert a development allowlist row. The restricted runtime role cannot
    /// do this; database administration uses the migration connection.
    pub fn provision_dev_user(&mut self, login: &DevLogin) -> Result<bool, PersistError> {
        if login.as_str() == crate::postgres::DEVELOPMENT_PROBE_LOGIN {
            return Err(PersistError::storage(
                "the readiness probe is not a player account",
            ));
        }
        match &mut self.backend {
            Backend::Postgres(store) => store.provision_dev_user(login),
            Backend::Fixture(store) => store.provision_dev_user(login),
        }
    }

    pub fn user_registered(&mut self, login: &DevLogin) -> Result<bool, PersistError> {
        if login.as_str() == crate::postgres::DEVELOPMENT_PROBE_LOGIN {
            return Ok(false);
        }
        match &mut self.backend {
            Backend::Postgres(store) => store.user_registered(login),
            Backend::Fixture(store) => Ok(store.user_registered(login)),
        }
    }

    /// Load roster slot zero for a registered user. Does not create a user or
    /// a character.
    pub fn resolve_existing(
        &mut self,
        login: &DevLogin,
    ) -> Result<PersistentCharacter, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.resolve_existing(login),
            Backend::Fixture(store) => store.resolve_existing(login),
        }
    }

    pub fn load_owned_character(
        &mut self,
        login: &DevLogin,
        id: CharacterId,
    ) -> Result<Option<PersistentCharacter>, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => {
                if !store.user_registered(login)? {
                    return Err(PersistError::CreateRejected(
                        crate::error::CreateCharacterRejection::Unregistered,
                    ));
                }
                if !store.owns(login, id)? {
                    return Ok(None);
                }
                store.load_character(id).map(Some)
            }
            Backend::Fixture(store) => store.load_owned_character(login, id),
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
            Backend::Postgres(store) => store.save_restore(snapshot, lease),
            Backend::Fixture(store) => store.save_snapshot(snapshot),
        }
    }

    pub fn commit_durable(
        &mut self,
        command: &DurableCommand,
    ) -> Result<DurableCommandResult, PersistError> {
        self.commit_durable_leased(command, None)
    }

    pub fn reserve_item_ids(&mut self, count: u32) -> Result<Vec<ItemInstanceId>, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.reserve_item_ids(count),
            Backend::Fixture(_) => Err(fixture_has_no_durable_store("durable item ids")),
        }
    }

    pub fn read_item(&mut self, id: ItemInstanceId) -> Result<Option<ItemRecord>, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.read_item(id),
            Backend::Fixture(_) => Ok(None),
        }
    }

    pub fn read_owned_restore(&mut self, id: CharacterId) -> Result<OwnedRestore, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.read_owned_restore(id),
            Backend::Fixture(_) => Err(fixture_has_no_durable_store("committed character restore")),
        }
    }

    pub fn commit_durable_leased(
        &mut self,
        command: &DurableCommand,
        lease: Option<&LeaseAuthority>,
    ) -> Result<DurableCommandResult, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.commit(command, lease),
            Backend::Fixture(_) => Err(fixture_has_no_durable_store("durable commands")),
        }
    }

    pub fn admit(
        &mut self,
        login: &DevLogin,
        character_id: CharacterId,
    ) -> Result<SessionAdmission, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => Ok(match store.admit(login, character_id)? {
                Admission::Granted { authority, restore } => SessionAdmission::Granted {
                    authority: Some(authority),
                    restore,
                },
                Admission::Held => SessionAdmission::Held,
                Admission::NotOwned => SessionAdmission::NotOwned,
            }),
            Backend::Fixture(store) => match store.admit(login, character_id)? {
                Some(restore) => Ok(SessionAdmission::Granted {
                    authority: None,
                    restore: Box::new(restore),
                }),
                None => Ok(SessionAdmission::NotOwned),
            },
        }
    }

    pub fn supersede(
        &mut self,
        authority: &LeaseAuthority,
    ) -> Result<(LeaseAuthority, OwnedRestore), PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.supersede(authority),
            Backend::Fixture(_) => Err(fixture_has_no_durable_store("character leases")),
        }
    }

    pub fn renew_lease(&mut self, authority: &LeaseAuthority) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.renew_lease(authority),
            Backend::Fixture(_) => Err(fixture_has_no_durable_store("character leases")),
        }
    }

    pub fn release_lease(&mut self, authority: &LeaseAuthority) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.release_lease(authority),
            Backend::Fixture(_) => Err(fixture_has_no_durable_store("character leases")),
        }
    }

    pub fn claim_channel(
        &mut self,
        channel_id: i64,
        retire_limit: Option<i64>,
    ) -> Result<ChannelClaim, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.claim_channel(channel_id, retire_limit),
            Backend::Fixture(_) => Err(fixture_has_no_durable_store("channel generations")),
        }
    }

    pub fn sweep_channel(
        &mut self,
        channel_id: i64,
        generation: u64,
        retire_limit: Option<i64>,
    ) -> Result<u64, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.sweep_channel(channel_id, generation, retire_limit),
            Backend::Fixture(_) => Err(fixture_has_no_durable_store("channel generations")),
        }
    }

    pub fn renew_channel(&mut self, channel_id: i64, generation: u64) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.renew_channel(channel_id, generation),
            Backend::Fixture(_) => Err(fixture_has_no_durable_store("channel generations")),
        }
    }

    pub fn release_channel(
        &mut self,
        channel_id: i64,
        generation: u64,
    ) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.release_channel(channel_id, generation),
            Backend::Fixture(_) => Err(fixture_has_no_durable_store("channel generations")),
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
            Backend::Postgres(store) => store.clock_moved_inside_one_statement(),
            Backend::Fixture(_) => Ok((false, false)),
        }
    }

    #[cfg(test)]
    pub fn sessions_waiting_on_a_lock(&mut self) -> Result<i64, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.sessions_waiting_on_a_lock(),
            Backend::Fixture(_) => Ok(0),
        }
    }

    #[cfg(test)]
    pub fn leave_unissued_item_gap_for_test(&mut self, next: u64) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.leave_unissued_item_gap_for_test(next),
            Backend::Fixture(_) => Err(fixture_has_no_durable_store("durable item ids")),
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
            Backend::Postgres(store) => store.expire_lease_for_test(login),
            Backend::Fixture(_) => Ok(()),
        }
    }

    #[cfg(test)]
    pub fn expire_channel_for_test(&mut self, channel_id: i64) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.expire_channel_for_test(channel_id),
            Backend::Fixture(_) => Ok(()),
        }
    }

    #[cfg(test)]
    pub fn unstamp_ground_for_test(&mut self, id: ItemInstanceId) -> Result<(), PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.unstamp_ground_for_test(id),
            Backend::Fixture(_) => Ok(()),
        }
    }

    #[cfg(test)]
    pub fn hide_next_commit_reply_for_test(&mut self) {
        if let Backend::Postgres(store) = &mut self.backend {
            store.hide_next_commit_reply();
        }
    }

    #[cfg(test)]
    pub fn discard_connection_after_next_commit_for_test(&mut self) {
        if let Backend::Postgres(store) = &mut self.backend {
            store.discard_connection_after_next_commit();
        }
    }

    #[cfg(test)]
    pub fn fail_next_reconnects_for_test(&mut self, count: u32) {
        if let Backend::Postgres(store) = &mut self.backend {
            store.fail_next_reconnects(count);
        }
    }

    pub fn item(&mut self, id: ItemInstanceId) -> Result<Option<ItemRecord>, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.item(id),
            Backend::Fixture(_) => Err(fixture_has_no_durable_store("item records")),
        }
    }

    pub fn narrative(&mut self, id: CharacterId) -> Result<CharacterNarrativeState, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.narrative(id),
            Backend::Fixture(_) => Err(fixture_has_no_durable_store("narrative records")),
        }
    }

    pub fn lookup(&mut self, login: &DevLogin) -> Result<Option<CharacterId>, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.lookup(login),
            Backend::Fixture(store) => store.lookup(login),
        }
    }

    pub fn roster(&mut self, login: &DevLogin) -> Result<Vec<CharacterRosterEntry>, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.roster(login),
            Backend::Fixture(store) => store.roster(login),
        }
    }

    pub fn create_character(
        &mut self,
        login: &DevLogin,
        name: &str,
    ) -> Result<CharacterRosterEntry, PersistError> {
        if login.as_str() == crate::postgres::DEVELOPMENT_PROBE_LOGIN {
            return Err(PersistError::CreateRejected(
                crate::error::CreateCharacterRejection::Unregistered,
            ));
        }
        match &mut self.backend {
            Backend::Postgres(store) => store.create_character(login, name),
            Backend::Fixture(store) => store.create_character(login, name),
        }
    }

    pub fn owns_character(
        &mut self,
        login: &DevLogin,
        id: CharacterId,
    ) -> Result<bool, PersistError> {
        match &mut self.backend {
            Backend::Postgres(store) => store.owns(login, id),
            Backend::Fixture(store) => {
                if !store.user_registered(login) {
                    return Err(PersistError::CreateRejected(
                        crate::error::CreateCharacterRejection::Unregistered,
                    ));
                }
                Ok(store.owns(login, id))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CreateCharacterRejection;

    fn fixture() -> PersistenceService {
        PersistenceService::unit_fixture()
    }

    #[test]
    fn missing_postgresql_configuration_does_not_open_a_writer() {
        let err = match PersistenceService::open_configured(None) {
            Ok(_) => panic!("missing database url opened a writer"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("PURGATORY_DATABASE_URL"), "{err}");
        assert!(err.to_string().contains("file-backed"), "{err}");
        let absent = PostgresSettings::from_vars(|_| Err(std::env::VarError::NotPresent)).unwrap();
        assert!(absent.is_none());
        let err = match PostgresSettings::from_vars(|key| match key {
            "PURGATORY_DATABASE_URL" => {
                Ok("postgres://postgres@127.0.0.1/purgatory_12a_test".into())
            }
            _ => Err(std::env::VarError::NotPresent),
        }) {
            Ok(_) => panic!("missing deployment id was accepted"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("PURGATORY_DEPLOYMENT_ID"), "{err}");
    }

    #[test]
    fn unregistered_user_cannot_create_or_receive_a_roster() {
        let mut service = fixture();
        let alice = DevLogin::parse("alice").unwrap();
        assert!(!service.user_registered(&alice).unwrap());
        assert!(matches!(
            service.roster(&alice),
            Err(PersistError::CreateRejected(
                CreateCharacterRejection::Unregistered
            ))
        ));
        assert!(matches!(
            service.create_character(&alice, "Alice"),
            Err(PersistError::CreateRejected(
                CreateCharacterRejection::Unregistered
            ))
        ));
        assert!(matches!(
            service.resolve_existing(&alice),
            Err(PersistError::CreateRejected(
                CreateCharacterRejection::Unregistered
            ))
        ));
    }

    #[test]
    fn provisioned_user_starts_empty_and_stays_isolated() {
        let mut service = fixture();
        let alice = DevLogin::parse("alice").unwrap();
        let bob = DevLogin::parse("bob").unwrap();
        assert!(service.provision_dev_user(&alice).unwrap());
        assert!(!service.provision_dev_user(&alice).unwrap());
        assert!(service.roster(&alice).unwrap().is_empty());
        assert!(matches!(
            service.create_character(&alice, "ab"),
            Err(PersistError::CreateRejected(
                CreateCharacterRejection::InvalidName(_)
            ))
        ));
        let first = service.create_character(&alice, "Ariel").unwrap();
        assert!(service.provision_dev_user(&bob).unwrap());
        for owner in [&alice, &bob] {
            assert!(matches!(
                service.create_character(owner, "ARIEL"),
                Err(PersistError::CreateRejected(
                    CreateCharacterRejection::NameTaken
                ))
            ));
        }
        let second = service.create_character(&alice, "Second").unwrap();
        let third = service.create_character(&alice, "Third").unwrap();
        assert!(matches!(
            service.create_character(&alice, "Fourth"),
            Err(PersistError::CreateRejected(
                CreateCharacterRejection::RosterFull
            ))
        ));
        let other = service.create_character(&bob, "Other").unwrap();
        assert_eq!(
            service.roster(&alice).unwrap(),
            vec![first.clone(), second, third]
        );
        assert_eq!(service.roster(&bob).unwrap(), vec![other.clone()]);
        assert!(service.owns_character(&alice, first.character_id).unwrap());
        assert!(!service.owns_character(&bob, first.character_id).unwrap());
        let loaded = service
            .load_owned_character(&alice, first.character_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.character_id, first.character_id);
        assert!(
            service
                .load_owned_character(&bob, first.character_id)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            service.resolve_existing(&alice).unwrap().character_id,
            first.character_id
        );
    }

    #[test]
    fn unit_fixture_does_not_create_game_files() {
        let dir = std::env::temp_dir().join(format!("purgatory-no-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut service = fixture();
        let alice = DevLogin::parse("alice").unwrap();
        service.provision_dev_user(&alice).unwrap();
        let entry = service.create_character(&alice, "Alice").unwrap();
        service
            .save_snapshot(PersistentCharacterSnapshot::from_character(
                &PersistentCharacter::new_default(entry.character_id),
            ))
            .unwrap();
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(names.is_empty(), "unit fixture wrote {names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
