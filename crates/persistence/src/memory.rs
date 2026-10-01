//! In-memory roster fixture for unit tests.
//!
//! This is not a durable writer and it does not create files. Server startup
//! never selects it.

use std::collections::{BTreeMap, BTreeSet};

use purgatory_common::{CharacterId, CharacterName, DevLogin};

use crate::character::{PersistentCharacter, PersistentCharacterSnapshot};
use crate::domain::CharacterNarrativeState;
use crate::error::{CreateCharacterRejection, PersistError};
use crate::identity::{CharacterRosterEntry, MAX_ROSTER_SIZE};
use crate::lifecycle::OwnedRestore;

#[derive(Clone)]
struct FixtureCharacter {
    entry: CharacterRosterEntry,
    character: PersistentCharacter,
}

#[derive(Default)]
pub(crate) struct MemoryStore {
    users: BTreeSet<String>,
    next_character_id: u64,
    /// login → roster order
    rosters: BTreeMap<String, Vec<CharacterId>>,
    characters: BTreeMap<u64, FixtureCharacter>,
    names: BTreeSet<String>,
}

impl MemoryStore {
    pub(crate) fn new() -> Self {
        Self {
            next_character_id: 1,
            ..Self::default()
        }
    }

    pub(crate) fn provision_dev_user(&mut self, login: &DevLogin) -> Result<bool, PersistError> {
        Ok(self.users.insert(login.as_str().to_string()))
    }

    pub(crate) fn user_registered(&self, login: &DevLogin) -> bool {
        self.users.contains(login.as_str())
    }

    fn require_user(&self, login: &DevLogin) -> Result<(), PersistError> {
        if self.user_registered(login) {
            Ok(())
        } else {
            Err(unregistered())
        }
    }

    pub(crate) fn resolve_existing(
        &self,
        login: &DevLogin,
    ) -> Result<PersistentCharacter, PersistError> {
        self.require_user(login)?;
        let id = self
            .rosters
            .get(login.as_str())
            .and_then(|roster| roster.first().copied())
            .ok_or_else(|| PersistError::storage("development user has no character to resolve"))?;
        Ok(self
            .characters
            .get(&id.raw())
            .expect("roster id")
            .character
            .clone())
    }

    pub(crate) fn load_owned_character(
        &self,
        login: &DevLogin,
        id: CharacterId,
    ) -> Result<Option<PersistentCharacter>, PersistError> {
        self.require_user(login)?;
        if !self.owns(login, id) {
            return Ok(None);
        }
        Ok(Some(
            self.characters
                .get(&id.raw())
                .expect("owned character")
                .character
                .clone(),
        ))
    }

    pub(crate) fn save_snapshot(
        &mut self,
        snapshot: PersistentCharacterSnapshot,
    ) -> Result<(), PersistError> {
        if let Some(row) = self.characters.get_mut(&snapshot.character_id.raw()) {
            row.character.persistence_revision = snapshot.persistence_revision;
            row.character.restore = snapshot.restore;
            row.character.instance_exit = snapshot.instance_exit;
            return Ok(());
        }
        let name = CharacterName::parse("Saved").expect("fixture name");
        self.names.insert(name.as_str().to_string());
        self.characters.insert(
            snapshot.character_id.raw(),
            FixtureCharacter {
                entry: CharacterRosterEntry {
                    character_id: snapshot.character_id,
                    display_name: name,
                },
                character: PersistentCharacter {
                    schema_version: crate::PERSISTENCE_SCHEMA_VERSION,
                    character_id: snapshot.character_id,
                    persistence_revision: snapshot.persistence_revision,
                    restore: snapshot.restore,
                    instance_exit: snapshot.instance_exit,
                },
            },
        );
        Ok(())
    }

    /// `Ok(None)` means the registered user does not own the character.
    pub(crate) fn admit(
        &self,
        login: &DevLogin,
        character_id: CharacterId,
    ) -> Result<Option<OwnedRestore>, PersistError> {
        self.require_user(login)?;
        if !self.owns(login, character_id) {
            return Ok(None);
        }
        let character = self
            .characters
            .get(&character_id.raw())
            .expect("owned character")
            .character
            .clone();
        Ok(Some(OwnedRestore {
            character,
            items: Vec::new(),
            narrative: CharacterNarrativeState::default(),
        }))
    }

    pub(crate) fn lookup(&self, login: &DevLogin) -> Result<Option<CharacterId>, PersistError> {
        self.require_user(login)?;
        Ok(self
            .rosters
            .get(login.as_str())
            .and_then(|roster| roster.first().copied()))
    }

    pub(crate) fn roster(
        &self,
        login: &DevLogin,
    ) -> Result<Vec<CharacterRosterEntry>, PersistError> {
        self.require_user(login)?;
        let Some(ids) = self.rosters.get(login.as_str()) else {
            return Ok(Vec::new());
        };
        Ok(ids
            .iter()
            .map(|id| {
                self.characters
                    .get(&id.raw())
                    .expect("roster id")
                    .entry
                    .clone()
            })
            .collect())
    }

    pub(crate) fn create_character(
        &mut self,
        login: &DevLogin,
        name: &str,
    ) -> Result<CharacterRosterEntry, PersistError> {
        self.require_user(login)?;
        let display_name = CharacterName::parse(name).map_err(|err| {
            PersistError::CreateRejected(CreateCharacterRejection::InvalidName(err))
        })?;
        let roster_len = self
            .rosters
            .get(login.as_str())
            .map(|roster| roster.len())
            .unwrap_or(0);
        if roster_len >= MAX_ROSTER_SIZE {
            return Err(PersistError::CreateRejected(
                CreateCharacterRejection::RosterFull,
            ));
        }
        let key = display_name.uniqueness_key();
        if self.names.contains(&key) {
            return Err(PersistError::CreateRejected(
                CreateCharacterRejection::NameTaken,
            ));
        }
        if self.next_character_id == 0 || self.next_character_id == u64::MAX {
            return Err(PersistError::CharacterIdsExhausted);
        }
        let id = CharacterId::from_raw(self.next_character_id);
        self.next_character_id += 1;
        let entry = CharacterRosterEntry {
            character_id: id,
            display_name,
        };
        self.names.insert(key);
        self.rosters
            .entry(login.as_str().to_string())
            .or_default()
            .push(id);
        self.characters.insert(
            id.raw(),
            FixtureCharacter {
                entry: entry.clone(),
                character: PersistentCharacter::new_default(id),
            },
        );
        Ok(entry)
    }

    pub(crate) fn owns(&self, login: &DevLogin, id: CharacterId) -> bool {
        self.rosters
            .get(login.as_str())
            .is_some_and(|roster| roster.contains(&id))
    }
}

fn unregistered() -> PersistError {
    PersistError::CreateRejected(CreateCharacterRejection::Unregistered)
}
