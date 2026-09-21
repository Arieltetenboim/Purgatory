//! Protocol v31 ability activation and owner-private grant envelopes.
//!
//! Client sends an input anchor, ability id, and optional selected entity.
//! Never damage, hits, query dimensions, facing, or Health.

use std::fmt;

use purgatory_common::ContentId;

use crate::snapshot::WireEntityId;

/// Payload sizes (no control tag). Independent activation omits the entity.
pub const ABILITY_ACTIVATE_INDEPENDENT_BYTES: usize = 4 + 2 + 4 + 8 + 1;
pub const ABILITY_ACTIVATE_SELECTED_BYTES: usize = ABILITY_ACTIVATE_INDEPENDENT_BYTES + 8;
pub const ABILITY_ACCEPTED_BYTES: usize = 4;
pub const ABILITY_REJECTED_BYTES: usize = 4 + 1;
pub const MAX_GRANTED_ABILITIES: usize = 32;

/// Owner-private authoritative baseline. Resent after attach and grant changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerAbilityGrants {
    pub abilities: Vec<ContentId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AbilityActivateRequest {
    /// Per-ability request identity.
    pub seq: u32,
    /// Input epoch and sequence whose simulation step owns activation.
    pub input_epoch: u16,
    pub input_sequence: u32,
    pub ability_id: ContentId,
    /// Present only when the ability's activation contract requires a selected entity.
    pub selected: Option<WireEntityId>,
}

/// Compact wire reject. Not a hit result and not a Health write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AbilityCommandReject {
    UnknownAbility = 1,
    NotGranted = 2,
    InvalidActivation = 3,
    StaleRequest = 4,
    InvalidRequest = 5,
    ActorDead = 6,
    Busy = 7,
    OnCooldown = 8,
    StateBlocked = 9,
    StaleInputAnchor = 10,
    InvalidInputAnchor = 11,
    RequiresGrounded = 12,
}

impl AbilityCommandReject {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::UnknownAbility),
            2 => Some(Self::NotGranted),
            3 => Some(Self::InvalidActivation),
            4 => Some(Self::StaleRequest),
            5 => Some(Self::InvalidRequest),
            6 => Some(Self::ActorDead),
            7 => Some(Self::Busy),
            8 => Some(Self::OnCooldown),
            9 => Some(Self::StateBlocked),
            10 => Some(Self::StaleInputAnchor),
            11 => Some(Self::InvalidInputAnchor),
            12 => Some(Self::RequiresGrounded),
            _ => None,
        }
    }
}

impl fmt::Display for AbilityCommandReject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnknownAbility => "UnknownAbility",
            Self::NotGranted => "NotGranted",
            Self::InvalidActivation => "InvalidActivation",
            Self::StaleRequest => "StaleRequest",
            Self::InvalidRequest => "InvalidRequest",
            Self::ActorDead => "ActorDead",
            Self::Busy => "Busy",
            Self::OnCooldown => "OnCooldown",
            Self::StateBlocked => "StateBlocked",
            Self::StaleInputAnchor => "StaleInputAnchor",
            Self::InvalidInputAnchor => "InvalidInputAnchor",
            Self::RequiresGrounded => "RequiresGrounded",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerAbility {
    Accepted {
        seq: u32,
    },
    Rejected {
        seq: u32,
        reason: AbilityCommandReject,
    },
}
