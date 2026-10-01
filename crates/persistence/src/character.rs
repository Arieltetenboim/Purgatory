use purgatory_common::{CharacterId, InstanceExitContext, RestoreIntent};
use serde::{Deserialize, Serialize};

use crate::error::PersistError;

pub const PERSISTENCE_SCHEMA_VERSION: u32 = 1;

/// Durable character record. No EntityId, ConnectionId, ChannelId, InstanceId,
/// WorldAddress, or exact coordinates.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistentCharacter {
    pub schema_version: u32,
    pub character_id: CharacterId,
    pub persistence_revision: u64,
    pub restore: RestoreIntent,
    #[serde(default)]
    pub instance_exit: Option<InstanceExitContext>,
    /// Thousandths of a health point. `None` is a row that has never stored HP
    /// and loads as full maximum health.
    #[serde(default)]
    pub current_health_milli: Option<u32>,
    #[serde(default)]
    pub health_revision: u64,
}

impl PersistentCharacter {
    #[must_use]
    pub fn new_default(character_id: CharacterId) -> Self {
        Self {
            schema_version: PERSISTENCE_SCHEMA_VERSION,
            character_id,
            persistence_revision: 1,
            restore: RestoreIntent::map1_default(),
            instance_exit: None,
            current_health_milli: None,
            health_revision: 0,
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
    pub current_health_milli: Option<u32>,
    pub health_revision: u64,
}

impl PersistentCharacterSnapshot {
    #[must_use]
    pub fn from_character(character: &PersistentCharacter) -> Self {
        Self {
            character_id: character.character_id,
            persistence_revision: character.persistence_revision,
            restore: character.restore.clone(),
            instance_exit: character.instance_exit.clone(),
            current_health_milli: character.current_health_milli,
            health_revision: character.health_revision,
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
            current_health_milli: self.current_health_milli,
            health_revision: self.health_revision,
        }
    }
}

const HEALTH_MILLI_SCALE: f32 = 1000.0;

/// Clamp `current` into `0..=max` and store it in thousandths of a point.
#[must_use]
pub fn health_milli(current: f32, max: f32) -> u32 {
    if !max.is_finite() || max <= 0.0 {
        return 0;
    }
    let current = if current.is_finite() { current } else { 0.0 };
    let clamped = current.clamp(0.0, max);
    let milli = (clamped * HEALTH_MILLI_SCALE).round();
    let max_milli = (max * HEALTH_MILLI_SCALE).round();
    if !milli.is_finite() || milli <= 0.0 {
        return 0;
    }
    let milli = milli.min(max_milli);
    if milli >= u32::MAX as f32 {
        u32::MAX
    } else {
        milli as u32
    }
}

/// Restore a stored thousandth-point value inside the live maximum.
#[must_use]
pub fn current_from_milli(milli: u32, max: f32) -> f32 {
    if !max.is_finite() || max <= 0.0 {
        return 0.0;
    }
    let max_milli = health_milli(max, max);
    let milli = milli.min(max_milli);
    (milli as f32) / HEALTH_MILLI_SCALE
}

#[cfg(test)]
mod tests {
    use super::{current_from_milli, health_milli};

    #[test]
    fn stored_health_stays_inside_the_live_maximum() {
        assert_eq!(health_milli(15.0, 20.0), 15_000);
        assert_eq!(health_milli(25.0, 20.0), 20_000);
        assert_eq!(health_milli(-1.0, 20.0), 0);
        assert_eq!(health_milli(f32::NAN, 20.0), 0);
        assert_eq!(current_from_milli(15_000, 20.0), 15.0);
        assert_eq!(current_from_milli(25_000, 20.0), 20.0);
        assert_eq!(current_from_milli(0, 20.0), 0.0);
    }
}
