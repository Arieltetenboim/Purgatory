//! Roster metadata shared by PostgreSQL and the in-memory unit fixture.
//!
//! Character state is not written to `identity.json` or `char_*.json`.

use purgatory_common::{CharacterId, CharacterName};
use serde::{Deserialize, Serialize};

pub const MAX_ROSTER_SIZE: usize = 3;

/// Identity/selection metadata only. Gameplay state lives in PostgreSQL.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CharacterRosterEntry {
    pub character_id: CharacterId,
    pub display_name: CharacterName,
}
