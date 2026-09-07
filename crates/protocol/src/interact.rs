//! Phase 6B interaction control envelopes. Event/change-driven, not per-tick.

use crate::snapshot::WireEntityId;

/// Client → server: request to open an interaction with `target`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteractOpen {
    pub target: WireEntityId,
}

/// Client → server: request to close a session the client believes it owns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteractClose {
    pub session_id: u32,
}

/// Client → server: edge-triggered portal travel request. Not `InteractOpen`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortalActivate {
    pub target: WireEntityId,
}

/// DEV-only: request a ChannelId change on the current map/instance.
/// Server-authoritative. Not a Portal and not client WorldAddress mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DevSetChannel {
    pub channel: u32,
}

/// DEV-only request to set or clear the bound player's movement speed.
/// `speed` is in hundredths of world units per second; `None` resets to the
/// canonical default. The server validates the range and owns the result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DevSetSpeed {
    pub speed: Option<u16>,
}

/// DEV-only request to set or clear the bound player's jump speed.
/// `jump` is in hundredths of world units per second.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DevSetJump {
    pub jump: Option<u16>,
}

/// Authoritative reject reason. Unknown wire values are [`CodecError::InvalidValue`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum InteractRejectReason {
    TargetMissing = 1,
    StaleId = 2,
    WrongAddress = 3,
    OutOfRange = 4,
    NotInteractable = 5,
    Unavailable = 6,
    InvalidSession = 7,
}

impl InteractRejectReason {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::TargetMissing),
            2 => Some(Self::StaleId),
            3 => Some(Self::WrongAddress),
            4 => Some(Self::OutOfRange),
            5 => Some(Self::NotInteractable),
            6 => Some(Self::Unavailable),
            7 => Some(Self::InvalidSession),
            _ => None,
        }
    }
}

impl std::fmt::Display for InteractRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TargetMissing => "TargetMissing",
            Self::StaleId => "StaleId",
            Self::WrongAddress => "WrongAddress",
            Self::OutOfRange => "OutOfRange",
            Self::NotInteractable => "NotInteractable",
            Self::Unavailable => "Unavailable",
            Self::InvalidSession => "InvalidSession",
        })
    }
}

/// Why the server closed a world-bound interaction session.
///
/// `AddressChanged` is WorldAddress-scoped (switch/chest/portal-like). It is
/// not a generic “close every player-related session” signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum InteractCloseReason {
    Requested = 1,
    TargetGone = 2,
    /// World-bound interaction: actor/target no longer WorldAddress-compatible.
    AddressChanged = 3,
    OutOfRange = 4,
    Disconnected = 5,
}

impl InteractCloseReason {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Requested),
            2 => Some(Self::TargetGone),
            3 => Some(Self::AddressChanged),
            4 => Some(Self::OutOfRange),
            5 => Some(Self::Disconnected),
            _ => None,
        }
    }
}

impl std::fmt::Display for InteractCloseReason {
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

/// Server → client interaction control. Not snapshot spam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerInteract {
    Opened {
        session_id: u32,
        target: WireEntityId,
    },
    Rejected {
        target: WireEntityId,
        reason: InteractRejectReason,
    },
    Updated {
        session_id: u32,
        target: WireEntityId,
    },
    Closed {
        session_id: u32,
        reason: InteractCloseReason,
    },
}
