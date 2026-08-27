//! Per-tick contact transitions. Not a global event bus.

use crate::entity::EntityId;

/// Contact transition recorded for the most recent FOOTNOTE tick.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ContactEvent {
    #[default]
    None,
    Landed {
        platform: EntityId,
    },
    LeftGround {
        platform: EntityId,
    },
}

impl ContactEvent {
    #[must_use]
    pub fn platform(self) -> Option<EntityId> {
        match self {
            Self::None => None,
            Self::Landed { platform } | Self::LeftGround { platform } => Some(platform),
        }
    }
}
