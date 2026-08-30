//! Semantic network failure categories. No Quinn/rustls types.

use purgatory_protocol::DisconnectReasonCode;

/// Unified diagnostic category. Wire codes and local failures map here.
/// Local-only variants are never serialized on the wire.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkFailureKind {
    ConnectFailed,
    TransportLost,
    HandshakeTimeout,
    ProtocolRejected,
    VersionMismatch,
    MalformedMessage,
    UnexpectedMessage,
    IdleTimeout,
    ServerShutdown,
    AlreadyConnected,
    ClientRequestedDisconnect,
    LocalShutdown,
    InternalNetworkError,
}

/// Whether the category originated as a peer/server wire reason or a local runtime failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureOrigin {
    Wire,
    Local,
}

/// Transport-level symptom after Quinn types have been stripped at the runtime boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportSymptom {
    TimedOut,
    ApplicationClosed(u8),
    LocallyClosed,
    Reset,
    ConnectRefused,
    Other,
}

impl NetworkFailureKind {
    #[must_use]
    pub const fn from_wire(code: DisconnectReasonCode) -> Self {
        match code {
            DisconnectReasonCode::VersionMismatch => Self::VersionMismatch,
            DisconnectReasonCode::Malformed => Self::MalformedMessage,
            DisconnectReasonCode::HandshakeTimeout => Self::HandshakeTimeout,
            DisconnectReasonCode::UnexpectedMessage => Self::UnexpectedMessage,
            DisconnectReasonCode::ServerShutdown => Self::ServerShutdown,
            DisconnectReasonCode::AlreadyConnected => Self::AlreadyConnected,
        }
    }

    #[must_use]
    pub const fn from_transport(symptom: TransportSymptom) -> Self {
        match symptom {
            TransportSymptom::TimedOut => Self::IdleTimeout,
            TransportSymptom::ConnectRefused => Self::ConnectFailed,
            TransportSymptom::LocallyClosed => Self::ClientRequestedDisconnect,
            TransportSymptom::ApplicationClosed(code) => {
                match DisconnectReasonCode::from_u8(code) {
                    Some(wire) => Self::from_wire(wire),
                    None => Self::ProtocolRejected,
                }
            }
            TransportSymptom::Reset | TransportSymptom::Other => Self::TransportLost,
        }
    }

    #[must_use]
    pub const fn origin(self) -> FailureOrigin {
        match self {
            Self::VersionMismatch
            | Self::MalformedMessage
            | Self::UnexpectedMessage
            | Self::ProtocolRejected
            | Self::ServerShutdown
            | Self::AlreadyConnected => FailureOrigin::Wire,
            Self::ConnectFailed
            | Self::TransportLost
            | Self::IdleTimeout
            | Self::ClientRequestedDisconnect
            | Self::LocalShutdown
            | Self::InternalNetworkError => FailureOrigin::Local,
            // Client Welcome wait is local. Server Hello timeout arrives as Rejected (Wire).
            Self::HandshakeTimeout => FailureOrigin::Local,
        }
    }

    /// Retryability is diagnostic metadata. There is no automatic reconnect loop.
    #[must_use]
    pub const fn retryable(self) -> bool {
        match self {
            Self::ConnectFailed
            | Self::TransportLost
            | Self::IdleTimeout
            | Self::ServerShutdown
            | Self::HandshakeTimeout
            | Self::InternalNetworkError => true,
            Self::VersionMismatch
            | Self::MalformedMessage
            | Self::UnexpectedMessage
            | Self::ProtocolRejected
            | Self::AlreadyConnected
            | Self::ClientRequestedDisconnect
            | Self::LocalShutdown => false,
        }
    }

    /// Manual disconnect and app close are not failures.
    #[must_use]
    pub const fn is_benign(self) -> bool {
        matches!(self, Self::ClientRequestedDisconnect | Self::LocalShutdown)
    }

    /// Short frontend status. Single mapping used by Connection Frontend and debug.
    #[must_use]
    pub const fn frontend_status(self) -> &'static str {
        match self {
            Self::ConnectFailed => "Connection failed",
            Self::TransportLost | Self::IdleTimeout | Self::InternalNetworkError => {
                "Connection lost"
            }
            Self::HandshakeTimeout => "Handshake timed out",
            Self::VersionMismatch => "Version mismatch",
            Self::MalformedMessage | Self::UnexpectedMessage | Self::ProtocolRejected => {
                "Connection rejected"
            }
            Self::ServerShutdown => "Server shutting down",
            Self::AlreadyConnected => "Already connected",
            Self::ClientRequestedDisconnect | Self::LocalShutdown => "Disconnected",
        }
    }

    #[must_use]
    pub const fn debug_label(self) -> &'static str {
        match self {
            Self::ConnectFailed => "ConnectFailed",
            Self::TransportLost => "TransportLost",
            Self::HandshakeTimeout => "HandshakeTimeout",
            Self::ProtocolRejected => "ProtocolRejected",
            Self::VersionMismatch => "VersionMismatch",
            Self::MalformedMessage => "MalformedMessage",
            Self::UnexpectedMessage => "UnexpectedMessage",
            Self::IdleTimeout => "IdleTimeout",
            Self::ServerShutdown => "ServerShutdown",
            Self::AlreadyConnected => "AlreadyConnected",
            Self::ClientRequestedDisconnect => "ClientRequestedDisconnect",
            Self::LocalShutdown => "LocalShutdown",
            Self::InternalNetworkError => "InternalNetworkError",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_codes_map_to_categories() {
        assert_eq!(
            NetworkFailureKind::from_wire(DisconnectReasonCode::VersionMismatch),
            NetworkFailureKind::VersionMismatch
        );
        assert_eq!(
            NetworkFailureKind::from_wire(DisconnectReasonCode::Malformed),
            NetworkFailureKind::MalformedMessage
        );
        assert_eq!(
            NetworkFailureKind::from_wire(DisconnectReasonCode::HandshakeTimeout),
            NetworkFailureKind::HandshakeTimeout
        );
        assert_eq!(
            NetworkFailureKind::from_wire(DisconnectReasonCode::UnexpectedMessage),
            NetworkFailureKind::UnexpectedMessage
        );
        assert_eq!(
            NetworkFailureKind::from_wire(DisconnectReasonCode::ServerShutdown),
            NetworkFailureKind::ServerShutdown
        );
        assert_eq!(
            NetworkFailureKind::from_wire(DisconnectReasonCode::AlreadyConnected),
            NetworkFailureKind::AlreadyConnected
        );
    }

    /// QUIC `endpoint.connect` / `Connecting` failures (closed port, refused,
    /// unreachable) map through [`TransportSymptom::ConnectRefused`].
    #[test]
    fn transport_symptoms_map_to_categories() {
        assert_eq!(
            NetworkFailureKind::from_transport(TransportSymptom::ConnectRefused),
            NetworkFailureKind::ConnectFailed
        );
        assert_eq!(
            NetworkFailureKind::from_transport(TransportSymptom::TimedOut),
            NetworkFailureKind::IdleTimeout
        );
        assert_eq!(
            NetworkFailureKind::from_transport(TransportSymptom::Reset),
            NetworkFailureKind::TransportLost
        );
        assert_eq!(
            NetworkFailureKind::from_transport(TransportSymptom::ApplicationClosed(
                DisconnectReasonCode::ServerShutdown.as_u8()
            )),
            NetworkFailureKind::ServerShutdown
        );
        assert_eq!(
            NetworkFailureKind::from_transport(TransportSymptom::LocallyClosed),
            NetworkFailureKind::ClientRequestedDisconnect
        );
        assert_eq!(
            NetworkFailureKind::from_transport(TransportSymptom::ApplicationClosed(255)),
            NetworkFailureKind::ProtocolRejected
        );
        assert!(!NetworkFailureKind::ProtocolRejected.retryable());
        assert_eq!(
            NetworkFailureKind::InternalNetworkError.frontend_status(),
            "Connection lost"
        );
    }

    #[test]
    fn retryability_policy() {
        assert!(NetworkFailureKind::ConnectFailed.retryable());
        assert!(NetworkFailureKind::TransportLost.retryable());
        assert!(NetworkFailureKind::IdleTimeout.retryable());
        assert!(NetworkFailureKind::ServerShutdown.retryable());
        assert!(NetworkFailureKind::HandshakeTimeout.retryable());
        assert!(!NetworkFailureKind::VersionMismatch.retryable());
        assert!(!NetworkFailureKind::MalformedMessage.retryable());
        assert!(!NetworkFailureKind::UnexpectedMessage.retryable());
        assert!(!NetworkFailureKind::ClientRequestedDisconnect.retryable());
        assert!(!NetworkFailureKind::LocalShutdown.retryable());
    }

    #[test]
    fn frontend_mapping_is_concise() {
        assert_eq!(
            NetworkFailureKind::ConnectFailed.frontend_status(),
            "Connection failed"
        );
        assert_eq!(
            NetworkFailureKind::VersionMismatch.frontend_status(),
            "Version mismatch"
        );
        assert_eq!(
            NetworkFailureKind::TransportLost.frontend_status(),
            "Connection lost"
        );
        assert_eq!(
            NetworkFailureKind::IdleTimeout.frontend_status(),
            "Connection lost"
        );
        assert_eq!(
            NetworkFailureKind::MalformedMessage.frontend_status(),
            "Connection rejected"
        );
        assert_eq!(
            NetworkFailureKind::ClientRequestedDisconnect.frontend_status(),
            "Disconnected"
        );
        assert_eq!(
            NetworkFailureKind::LocalShutdown.frontend_status(),
            "Disconnected"
        );
    }

    #[test]
    fn manual_disconnect_is_not_a_failure() {
        assert!(NetworkFailureKind::ClientRequestedDisconnect.is_benign());
        assert!(NetworkFailureKind::LocalShutdown.is_benign());
        assert!(!NetworkFailureKind::TransportLost.is_benign());
    }
}
