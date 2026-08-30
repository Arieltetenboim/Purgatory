//! Domain interaction session. Not a UI window.

use crate::entity::EntityId;

/// Server-allocated session identity. Not a connection id.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct InteractionSessionId(pub u32);

impl InteractionSessionId {
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Authoritative interaction lifecycle. Distinct from entity lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractionSessionState {
    Opened,
    Active,
    Updated,
    Closed,
}

/// One actor–target interaction. Owned by [`crate::World`], keyed by actor entity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteractionSession {
    pub id: InteractionSessionId,
    pub actor: EntityId,
    pub target: EntityId,
    pub state: InteractionSessionState,
}

/// Structured rejection. Client never supplies this.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractionReject {
    TargetMissing,
    StaleId,
    WrongAddress,
    OutOfRange,
    NotInteractable,
    Unavailable,
    InvalidSession,
    /// Destination portal is latched until Up is released (or the actor leaves the zone).
    ReentryLocked,
}

/// Why a live [`InteractionSession`] ended.
///
/// This enum is WorldAddress-scoped world-entity interaction only
/// (switch / chest / portal / NPC-like interactables). It is not a generic
/// "close every player session" signal. Identity / social sessions must use
/// their own types when they exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractionCloseReason {
    Requested,
    TargetGone,
    /// Actor or target is no longer WorldAddress-compatible, or left the world.
    /// Invalidates this world-bound interaction, not arbitrary identity/social
    /// sessions. **WorldAddress boundary ≠ social identity boundary.**
    AddressChanged,
    OutOfRange,
    Disconnected,
}

impl std::fmt::Display for InteractionReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TargetMissing => "TargetMissing",
            Self::StaleId => "StaleId",
            Self::WrongAddress => "WrongAddress",
            Self::OutOfRange => "OutOfRange",
            Self::NotInteractable => "NotInteractable",
            Self::Unavailable => "Unavailable",
            Self::InvalidSession => "InvalidSession",
            Self::ReentryLocked => "ReentryLocked",
        })
    }
}

impl std::fmt::Display for InteractionCloseReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Requested => "Requested",
            Self::TargetGone => "TargetGone",
            Self::AddressChanged => "AddressChanged",
            Self::OutOfRange => "OutOfRange",
            Self::Disconnected => "Disconnected",
        })
    }
}

impl std::fmt::Display for InteractionSessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Opened => "Opened",
            Self::Active => "Active",
            Self::Updated => "Updated",
            Self::Closed => "Closed",
        })
    }
}
