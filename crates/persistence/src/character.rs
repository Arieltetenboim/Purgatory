use purgatory_common::{CharacterId, InstanceExitContext, RestoreIntent};
use serde::{Deserialize, Serialize};

use crate::domain::{self, CHARACTER_RECORD_SCHEMA_VERSION, PersistentItem};
use crate::error::PersistError;

pub const PERSISTENCE_SCHEMA_VERSION: u32 = CHARACTER_RECORD_SCHEMA_VERSION;

/// Durable character checkpoint. No EntityId, ConnectionId, ChannelId,
/// InstanceId, WorldAddress, or exact coordinates.
///
/// `items` is the canonical owned inventory and equipment. `EquipmentState`
/// is not stored; entry rebuilds that projection from these records.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentCharacter {
    pub schema_version: u32,
    /// Transaction id that last changed this record (its checkpoint version).
    /// Zero only before the first commit. The log, not this field, is the
    /// authority.
    pub applied_transaction_id: u64,
    pub character_id: CharacterId,
    pub persistence_revision: u64,
    pub restore: RestoreIntent,
    pub instance_exit: Option<InstanceExitContext>,
    pub items: Vec<PersistentItem>,
}

impl PersistentCharacter {
    #[must_use]
    pub fn new_default(character_id: CharacterId) -> Self {
        Self {
            schema_version: PERSISTENCE_SCHEMA_VERSION,
            applied_transaction_id: 0,
            character_id,
            persistence_revision: 1,
            restore: RestoreIntent::map1_default(),
            instance_exit: None,
            items: Vec::new(),
        }
    }

    pub fn validate(&self, path: &std::path::Path) -> Result<(), PersistError> {
        if self.schema_version != PERSISTENCE_SCHEMA_VERSION {
            return Err(PersistError::schema(path, self.schema_version));
        }
        if self.character_id.raw() == 0 {
            return Err(PersistError::corrupt(path, "character_id 0 is reserved"));
        }
        if self.persistence_revision == 0 {
            return Err(PersistError::corrupt(
                path,
                "persistence_revision 0 is reserved",
            ));
        }
        if self.restore.map_authored.is_empty() || self.restore.point_id.is_empty() {
            return Err(PersistError::corrupt(path, "restore map/point required"));
        }
        domain::validate_character_items(&self.items, path)?;
        Ok(())
    }

    pub(crate) fn body_eq(&self, other: &Self) -> bool {
        self.schema_version == other.schema_version
            && self.character_id == other.character_id
            && self.persistence_revision == other.persistence_revision
            && self.restore == other.restore
            && self.instance_exit == other.instance_exit
            && self.items == other.items
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
            applied_transaction_id: 0,
            character_id: self.character_id,
            persistence_revision: self.persistence_revision,
            restore: self.restore,
            instance_exit: self.instance_exit,
            items: Vec::new(),
        }
    }
}
