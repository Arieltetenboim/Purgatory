//! Explicit client connection state. Not a pile of booleans.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use purgatory_protocol::{
    ConnectionId, DEFAULT_DEV_HOST, DEFAULT_DEV_PORT, DisconnectReason, DisconnectReasonCode,
};

/// Client-side session lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Handshaking,
    Connected,
    Rejected,
}

impl ConnectionState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disconnected => "Disconnected",
            Self::Connecting => "Connecting",
            Self::Handshaking => "Handshaking",
            Self::Connected => "Connected",
            Self::Rejected => "Rejected",
        }
    }
}

/// Local errors that are not wire [`DisconnectReasonCode`] values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalConnectionError {
    ConnectFailed,
    TransportError,
    ClientClosed,
}

impl LocalConnectionError {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConnectFailed => "connect failed",
            Self::TransportError => "transport error",
            Self::ClientClosed => "client closed",
        }
    }
}

/// Last human-readable disconnect/reject status (wire or local).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LastStatus {
    None,
    Wire(DisconnectReasonCode),
    Local(LocalConnectionError),
}

impl LastStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "—",
            Self::Wire(code) => code.as_str(),
            Self::Local(err) => err.as_str(),
        }
    }
}

/// Commands from the UI thread. Sent with `try_send` (never block winit).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkCommand {
    Connect,
    Disconnect,
    Shutdown,
}

/// Semantic events consumed by presentation. No Quinn types.
#[derive(Clone, Debug)]
pub enum NetworkEvent {
    State(ConnectionState),
    Connected {
        connection_id: ConnectionId,
        protocol_version: u32,
        server_tick_rate: u32,
    },
    Rejected {
        reason: DisconnectReason,
    },
    Disconnected {
        error: LocalConnectionError,
    },
    RttUpdated {
        rtt: Duration,
    },
}

/// UI-thread copy of network status. Updated only from [`NetworkEvent`].
#[derive(Clone, Debug)]
pub struct NetworkView {
    pub state: ConnectionState,
    pub server: SocketAddr,
    pub protocol_version: Option<u32>,
    pub connection_id: Option<ConnectionId>,
    pub server_tick_rate: Option<u32>,
    pub rtt: Option<Duration>,
    pub connected_since: Option<Instant>,
    pub last_status: LastStatus,
    pub messages_tx: u64,
    pub messages_rx: u64,
    pub events_dropped: u64,
}

impl NetworkView {
    #[must_use]
    pub fn new(server: SocketAddr) -> Self {
        Self {
            state: ConnectionState::Disconnected,
            server,
            protocol_version: None,
            connection_id: None,
            server_tick_rate: None,
            rtt: None,
            connected_since: None,
            last_status: LastStatus::None,
            messages_tx: 0,
            messages_rx: 0,
            events_dropped: 0,
        }
    }

    pub fn apply(&mut self, event: NetworkEvent) {
        match event {
            NetworkEvent::State(state) => {
                self.state = state;
                if matches!(
                    state,
                    ConnectionState::Disconnected | ConnectionState::Connecting
                ) {
                    self.connection_id = None;
                    self.protocol_version = None;
                    self.server_tick_rate = None;
                    self.rtt = None;
                    self.connected_since = None;
                }
            }
            NetworkEvent::Connected {
                connection_id,
                protocol_version,
                server_tick_rate,
            } => {
                self.state = ConnectionState::Connected;
                self.connection_id = Some(connection_id);
                self.protocol_version = Some(protocol_version);
                self.server_tick_rate = Some(server_tick_rate);
                self.connected_since = Some(Instant::now());
                self.last_status = LastStatus::None;
                self.messages_tx = self.messages_tx.saturating_add(1);
                self.messages_rx = self.messages_rx.saturating_add(1);
            }
            NetworkEvent::Rejected { reason } => {
                self.state = ConnectionState::Rejected;
                self.connection_id = None;
                self.rtt = None;
                self.connected_since = None;
                self.last_status = LastStatus::Wire(reason.code);
                self.messages_rx = self.messages_rx.saturating_add(1);
            }
            NetworkEvent::Disconnected { error } => {
                self.state = ConnectionState::Disconnected;
                self.connection_id = None;
                self.rtt = None;
                self.connected_since = None;
                self.last_status = LastStatus::Local(error);
            }
            NetworkEvent::RttUpdated { rtt } => {
                self.rtt = Some(rtt);
                self.messages_tx = self.messages_tx.saturating_add(1);
                self.messages_rx = self.messages_rx.saturating_add(1);
            }
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> NetworkSnapshot {
        NetworkSnapshot {
            state: self.state,
            server_host: DEFAULT_DEV_HOST,
            server_port: self.server.port(),
            protocol_version: self.protocol_version,
            connection_id: self.connection_id,
            rtt: self.rtt,
            connected_for: self.connected_since.map(|t| t.elapsed()),
            last_status: self.last_status.as_str(),
            transport: "QUIC",
            messages_tx: self.messages_tx,
            messages_rx: self.messages_rx,
            events_dropped: self.events_dropped,
        }
    }
}

/// Copy-friendly debug overlay fields.
#[derive(Clone, Copy, Debug)]
pub struct NetworkSnapshot {
    pub state: ConnectionState,
    pub server_host: &'static str,
    pub server_port: u16,
    pub protocol_version: Option<u32>,
    pub connection_id: Option<ConnectionId>,
    pub rtt: Option<Duration>,
    pub connected_for: Option<Duration>,
    pub last_status: &'static str,
    pub transport: &'static str,
    pub messages_tx: u64,
    pub messages_rx: u64,
    pub events_dropped: u64,
}

impl Default for NetworkSnapshot {
    fn default() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            server_host: DEFAULT_DEV_HOST,
            server_port: DEFAULT_DEV_PORT,
            protocol_version: None,
            connection_id: None,
            rtt: None,
            connected_for: None,
            last_status: LastStatus::None.as_str(),
            transport: "QUIC",
            messages_tx: 0,
            messages_rx: 0,
            events_dropped: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_protocol::PROTOCOL_VERSION;

    const SERVER: SocketAddr = purgatory_protocol::dev_socket_addr();

    #[test]
    fn success_path_states() {
        let mut view = NetworkView::new(SERVER);
        assert_eq!(view.state, ConnectionState::Disconnected);
        view.apply(NetworkEvent::State(ConnectionState::Connecting));
        assert_eq!(view.state, ConnectionState::Connecting);
        view.apply(NetworkEvent::State(ConnectionState::Handshaking));
        assert_eq!(view.state, ConnectionState::Handshaking);
        view.apply(NetworkEvent::Connected {
            connection_id: ConnectionId::from_raw(1),
            protocol_version: PROTOCOL_VERSION,
            server_tick_rate: 30,
        });
        assert_eq!(view.state, ConnectionState::Connected);
        assert_eq!(view.connection_id.map(|id| id.get()), Some(1));
        view.apply(NetworkEvent::RttUpdated {
            rtt: Duration::from_millis(1),
        });
        assert!(view.rtt.is_some());
    }

    #[test]
    fn connect_failed_stays_disconnected_with_local_error() {
        let mut view = NetworkView::new(SERVER);
        view.apply(NetworkEvent::State(ConnectionState::Connecting));
        view.apply(NetworkEvent::Disconnected {
            error: LocalConnectionError::ConnectFailed,
        });
        assert_eq!(view.state, ConnectionState::Disconnected);
        assert_eq!(
            view.last_status,
            LastStatus::Local(LocalConnectionError::ConnectFailed)
        );
        assert!(view.connection_id.is_none());
    }

    #[test]
    fn reject_uses_wire_reason_not_local_error() {
        let mut view = NetworkView::new(SERVER);
        view.apply(NetworkEvent::Rejected {
            reason: DisconnectReason::new(DisconnectReasonCode::VersionMismatch, "v"),
        });
        assert_eq!(view.state, ConnectionState::Rejected);
        assert_eq!(
            view.last_status,
            LastStatus::Wire(DisconnectReasonCode::VersionMismatch)
        );
    }
}
