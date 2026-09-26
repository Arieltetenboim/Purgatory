use purgatory_common::{CharacterId, InstanceExitContext, RestoreIntent};
use serde::{Deserialize, Serialize};

use crate::error::PersistError;

pub const PERSISTENCE_SCHEMA_VERSION: u32 = 1;

/// Durable character record. No EntityId, ConnectionId, ChannelId, InstanceId,
/// WorldAddress, or exact coordinates.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PersistentCharacter {
    pub schema_version: u32,
    pub character_id: CharacterId,
    pub persistence_revision: u64,
    pub restore: RestoreIntent,
    #[serde(default)]
    pub instance_exit: Option<InstanceExitContext>,
}

impl PersistentCharacter {
    #[must_use]
    pub fn new_default(character_id: CharacterId) -> Self {
        Self {
            schema_version: PERSISTENCE_SCHEMA_VERSION,
            character_id,
            persistence_revision: 1,
            restore: RestoreIntent::footnote_default(),
            instance_exit: None,
        }
    }

    pub fn validate(&self, path: &std::path::Path) -> Result<(), PersistError> {
        if self.schema_version != PERSISTENCE_SCHEMA_VERSION {
            return Err(PersistError::schema(path, self.schema_version));
        }
        if self.character_id.raw() == 0 {
            return Err(PersistError::corrupt(path, "character_id 0 is reserved"));
        }
        if self.restore.map_authored.is_empty() || self.restore.point_id.is_empty() {
            return Err(PersistError::corrupt(path, "restore map/point required"));
        }
        Ok(())
    }
}

/// Owned projection produced on the simulation thread. The persistence worker
/// performs JSON serialization and filesystem IO.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistentCharacterSnapshot {
    pub character_id: CharacterId,
    pub persistence_revision: u64,
    pub restore: RestoreIntent,
    pub instance_exit: Option<InstanceExitContext>,
}

impl PersistentCharacterSnapshot {
    #[must_use]
    pub fn from_character(character: &PersistentCharacter) -> Self {
        Self {
            character_id: character.character_id,
            persistence_revision: character.persistence_revision,
            restore: character.restore.clone(),
            instance_exit: character.instance_exit.clone(),
        }
    }

    #[must_use]
    pub fn into_character(self) -> PersistentCharacter {
        PersistentCharacter {
            schema_version: PERSISTENCE_SCHEMA_VERSION,
            character_id: self.character_id,
            persistence_revision: self.persistence_revision,
            restore: self.restore,
            instance_exit: self.instance_exit,
        }
    }
}
