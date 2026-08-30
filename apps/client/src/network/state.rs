//! Explicit client connection state. Not a pile of booleans.

use std::fmt;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use purgatory_protocol::{ConnectionId, DEFAULT_DEV_HOST, DEFAULT_DEV_PORT, DisconnectReason};

use super::diagnostics::{
    DiagKind, NETWORK_HISTORY_CAP, NetworkCounters, NetworkDiagEvent, NetworkHistory, RttStats,
};
use super::failure::{FailureOrigin, NetworkFailureKind};

/// Client-local generation for one connect attempt. Never sent on the wire.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ConnectionAttemptId(u64);

impl ConnectionAttemptId {
    pub const NONE: Self = Self(0);

    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for ConnectionAttemptId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Client-side session lifecycle (NetworkState equivalent).
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

/// Commands from the UI thread.
///
/// Connect is `try_send` on a bounded mpsc (never block winit). Disconnect and
/// Shutdown are delivered via the runtime watch control plane so they cannot
/// be stranded behind Connect pressure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkCommand {
    Connect {
        attempt_id: ConnectionAttemptId,
        dev_login: String,
    },
    Disconnect,
    Shutdown,
}

/// Semantic events consumed by presentation. No Quinn types.
#[derive(Clone, Debug)]
pub enum NetworkEvent {
    Connecting {
        attempt_id: ConnectionAttemptId,
    },
    Handshaking {
        attempt_id: ConnectionAttemptId,
    },
    Connected {
        attempt_id: ConnectionAttemptId,
        connection_id: ConnectionId,
        protocol_version: u32,
        server_tick_rate: u32,
    },
    Rejected {
        attempt_id: ConnectionAttemptId,
        reason: DisconnectReason,
    },
    Disconnected {
        attempt_id: ConnectionAttemptId,
        kind: NetworkFailureKind,
    },
    RttUpdated {
        attempt_id: ConnectionAttemptId,
        rtt: Duration,
    },
    Interact {
        attempt_id: ConnectionAttemptId,
        event: purgatory_protocol::ServerInteract,
    },
}

impl NetworkEvent {
    #[must_use]
    pub const fn attempt_id(&self) -> ConnectionAttemptId {
        match *self {
            Self::Connecting { attempt_id }
            | Self::Handshaking { attempt_id }
            | Self::Connected { attempt_id, .. }
            | Self::Rejected { attempt_id, .. }
            | Self::Disconnected { attempt_id, .. }
            | Self::RttUpdated { attempt_id, .. }
            | Self::Interact { attempt_id, .. } => attempt_id,
        }
    }

    /// Lifecycle-critical events must not be dropped under telemetry pressure.
    /// Interaction control is gameplay, not RTT telemetry.
    #[must_use]
    pub const fn is_lifecycle(&self) -> bool {
        !matches!(self, Self::RttUpdated { .. })
    }
}

/// UI-thread copy of network status. Updated only from [`NetworkEvent`]
/// (or explicit local connect/disconnect ownership).
#[derive(Clone, Debug)]
pub struct NetworkView {
    pub state: ConnectionState,
    pub active_attempt: ConnectionAttemptId,
    pub server: SocketAddr,
    pub protocol_version: Option<u32>,
    pub connection_id: Option<ConnectionId>,
    pub server_tick_rate: Option<u32>,
    pub rtt: Option<Duration>,
    pub rtt_stats: RttStats,
    pub connected_since: Option<Instant>,
    pub last_failure: Option<NetworkFailureKind>,
    pub last_failure_origin: Option<FailureOrigin>,
    pub messages_tx: u64,
    pub messages_rx: u64,
    pub counters: NetworkCounters,
    pub history: NetworkHistory,
    next_attempt: u64,
}

impl NetworkView {
    #[must_use]
    pub fn new(server: SocketAddr) -> Self {
        Self {
            state: ConnectionState::Disconnected,
            active_attempt: ConnectionAttemptId::NONE,
            server,
            protocol_version: None,
            connection_id: None,
            server_tick_rate: None,
            rtt: None,
            rtt_stats: RttStats::default(),
            connected_since: None,
            last_failure: None,
            last_failure_origin: None,
            messages_tx: 0,
            messages_rx: 0,
            counters: NetworkCounters::default(),
            history: NetworkHistory::new(),
            next_attempt: 0,
        }
    }

    #[must_use]
    pub fn can_connect(&self) -> bool {
        matches!(
            self.state,
            ConnectionState::Disconnected | ConnectionState::Rejected
        )
    }

    /// Start a new attempt. None if Connecting/Handshaking/Connected.
    pub fn try_begin_connect(&mut self) -> Option<ConnectionAttemptId> {
        if !self.can_connect() {
            return None;
        }
        self.next_attempt = self.next_attempt.saturating_add(1);
        let id = ConnectionAttemptId::from_raw(self.next_attempt);
        self.active_attempt = id;
        self.state = ConnectionState::Connecting;
        self.counters.reconnect_attempts = self.counters.reconnect_attempts.saturating_add(1);
        self.clear_session_local();
        Some(id)
    }

    /// Retire the active attempt immediately. Does not wait for the runtime.
    pub fn invalidate_attempt(&mut self) {
        let attempt = self.active_attempt.get();
        let cid = self.connection_id.map(ConnectionId::get).unwrap_or(0);
        self.active_attempt = ConnectionAttemptId::NONE;
        self.state = ConnectionState::Disconnected;
        self.set_failure(
            NetworkFailureKind::ClientRequestedDisconnect,
            FailureOrigin::Local,
        );
        self.history.push(
            DiagKind::Disconnected,
            attempt,
            cid,
            self.state,
            self.last_failure,
        );
        self.counters.lifecycle_events = self.counters.lifecycle_events.saturating_add(1);
        self.clear_session_local();
    }

    /// Connect command never reached the runtime.
    pub fn fail_unsent_connect(&mut self) {
        let attempt = self.active_attempt.get();
        self.active_attempt = ConnectionAttemptId::NONE;
        self.state = ConnectionState::Disconnected;
        self.set_failure(NetworkFailureKind::ConnectFailed, FailureOrigin::Local);
        self.history.push(
            DiagKind::Disconnected,
            attempt,
            0,
            self.state,
            self.last_failure,
        );
        self.counters.lifecycle_events = self.counters.lifecycle_events.saturating_add(1);
        self.clear_session_local();
    }

    pub fn note_stale(&mut self, record_history: bool, attempt_id: u64) {
        self.counters.stale_events_ignored = self.counters.stale_events_ignored.saturating_add(1);
        if record_history {
            self.history.push(
                DiagKind::StaleIgnored,
                attempt_id,
                self.connection_id.map(ConnectionId::get).unwrap_or(0),
                self.state,
                None,
            );
        }
    }

    pub fn set_dropped_telemetry(&mut self, dropped: u64) {
        self.counters.dropped_telemetry = dropped;
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    fn set_failure(&mut self, kind: NetworkFailureKind, origin: FailureOrigin) {
        self.last_failure = Some(kind);
        self.last_failure_origin = Some(origin);
    }

    pub fn clear_session_local(&mut self) {
        self.connection_id = None;
        self.protocol_version = None;
        self.server_tick_rate = None;
        self.rtt = None;
        self.rtt_stats.reset();
        self.connected_since = None;
    }

    pub fn apply_trusted(&mut self, event: NetworkEvent) {
        match event {
            NetworkEvent::Connecting { attempt_id } => {
                self.state = ConnectionState::Connecting;
                self.clear_session_local();
                self.counters.lifecycle_events = self.counters.lifecycle_events.saturating_add(1);
                self.history
                    .push(DiagKind::Connecting, attempt_id.get(), 0, self.state, None);
            }
            NetworkEvent::Handshaking { attempt_id } => {
                self.state = ConnectionState::Handshaking;
                self.counters.lifecycle_events = self.counters.lifecycle_events.saturating_add(1);
                self.history
                    .push(DiagKind::Handshaking, attempt_id.get(), 0, self.state, None);
            }
            NetworkEvent::Connected {
                attempt_id,
                connection_id,
                protocol_version,
                server_tick_rate,
                ..
            } => {
                self.state = ConnectionState::Connected;
                self.connection_id = Some(connection_id);
                self.protocol_version = Some(protocol_version);
                self.server_tick_rate = Some(server_tick_rate);
                self.connected_since = Some(Instant::now());
                self.last_failure = None;
                self.last_failure_origin = None;
                self.messages_tx = self.messages_tx.saturating_add(1);
                self.messages_rx = self.messages_rx.saturating_add(1);
                self.counters.lifecycle_events = self.counters.lifecycle_events.saturating_add(1);
                self.history.push(
                    DiagKind::Connected,
                    attempt_id.get(),
                    connection_id.get(),
                    self.state,
                    None,
                );
            }
            NetworkEvent::Rejected {
                attempt_id, reason, ..
            } => {
                let kind = NetworkFailureKind::from_wire(reason.code);
                self.state = ConnectionState::Rejected;
                self.set_failure(kind, FailureOrigin::Wire);
                self.clear_session_local();
                self.messages_rx = self.messages_rx.saturating_add(1);
                self.counters.lifecycle_events = self.counters.lifecycle_events.saturating_add(1);
                self.history.push(
                    DiagKind::Rejected,
                    attempt_id.get(),
                    0,
                    self.state,
                    Some(kind),
                );
            }
            NetworkEvent::Disconnected {
                attempt_id, kind, ..
            } => {
                self.state = ConnectionState::Disconnected;
                self.set_failure(kind, kind.origin());
                self.clear_session_local();
                self.counters.lifecycle_events = self.counters.lifecycle_events.saturating_add(1);
                self.history.push(
                    DiagKind::Disconnected,
                    attempt_id.get(),
                    0,
                    self.state,
                    Some(kind),
                );
            }
            NetworkEvent::RttUpdated { rtt, .. } => {
                self.rtt = Some(rtt);
                self.rtt_stats.observe(rtt);
                self.messages_tx = self.messages_tx.saturating_add(1);
                self.messages_rx = self.messages_rx.saturating_add(1);
                self.counters.telemetry_events = self.counters.telemetry_events.saturating_add(1);
            }
            NetworkEvent::Interact { .. } => {}
        }
    }

    #[must_use]
    pub fn frontend_status(&self) -> &'static str {
        match self.state {
            ConnectionState::Connecting => "Connecting...",
            ConnectionState::Handshaking => "Handshaking...",
            ConnectionState::Connected => "Connected",
            ConnectionState::Rejected | ConnectionState::Disconnected => self
                .last_failure
                .map(NetworkFailureKind::frontend_status)
                .unwrap_or("Disconnected"),
        }
    }

    #[must_use]
    pub fn snapshot(&self, client_screen: &'static str) -> NetworkSnapshot {
        NetworkSnapshot {
            client_screen,
            state: self.state,
            server_host: DEFAULT_DEV_HOST,
            server_port: self.server.port(),
            protocol_version: self.protocol_version,
            connection_id: self.connection_id,
            attempt_id: self.active_attempt.get(),
            rtt: self.rtt_stats.latest,
            rtt_min: self.rtt_stats.min,
            rtt_max: self.rtt_stats.max,
            rtt_ewma: self.rtt_stats.ewma,
            connected_for: self.connected_since.map(|t| t.elapsed()),
            last_failure: self.last_failure,
            last_failure_retryable: self.last_failure.is_some_and(NetworkFailureKind::retryable),
            last_status: self
                .last_failure
                .map(NetworkFailureKind::debug_label)
                .unwrap_or("—"),
            transport: "QUIC",
            messages_tx: self.messages_tx,
            messages_rx: self.messages_rx,
            lifecycle_events: self.counters.lifecycle_events,
            telemetry_events: self.counters.telemetry_events,
            events_dropped: self.counters.dropped_telemetry,
            stale_events_ignored: self.counters.stale_events_ignored,
            reconnect_attempts: self.counters.reconnect_attempts,
            can_connect: self.can_connect(),
            history: self.history.snapshot_newest_first(),
            history_len: if self.history.is_empty() {
                0
            } else {
                self.history.len().min(NetworkHistory::cap()) as u8
            },
        }
    }
}

/// Copy-friendly debug overlay fields.
#[derive(Clone, Copy, Debug)]
pub struct NetworkSnapshot {
    pub client_screen: &'static str,
    pub state: ConnectionState,
    pub server_host: &'static str,
    pub server_port: u16,
    pub protocol_version: Option<u32>,
    pub connection_id: Option<ConnectionId>,
    pub attempt_id: u64,
    pub rtt: Option<Duration>,
    pub rtt_min: Option<Duration>,
    pub rtt_max: Option<Duration>,
    pub rtt_ewma: Option<Duration>,
    pub connected_for: Option<Duration>,
    pub last_failure: Option<NetworkFailureKind>,
    pub last_failure_retryable: bool,
    pub last_status: &'static str,
    pub transport: &'static str,
    pub messages_tx: u64,
    pub messages_rx: u64,
    pub lifecycle_events: u64,
    pub telemetry_events: u64,
    pub events_dropped: u64,
    pub stale_events_ignored: u64,
    pub reconnect_attempts: u64,
    pub can_connect: bool,
    pub history: [Option<NetworkDiagEvent>; NETWORK_HISTORY_CAP],
    pub history_len: u8,
}

impl Default for NetworkSnapshot {
    fn default() -> Self {
        Self {
            client_screen: "Connection",
            state: ConnectionState::Disconnected,
            server_host: DEFAULT_DEV_HOST,
            server_port: DEFAULT_DEV_PORT,
            protocol_version: None,
            connection_id: None,
            attempt_id: 0,
            rtt: None,
            rtt_min: None,
            rtt_max: None,
            rtt_ewma: None,
            connected_for: None,
            last_failure: None,
            last_failure_retryable: false,
            last_status: "—",
            transport: "QUIC",
            messages_tx: 0,
            messages_rx: 0,
            lifecycle_events: 0,
            telemetry_events: 0,
            events_dropped: 0,
            stale_events_ignored: 0,
            reconnect_attempts: 0,
            can_connect: true,
            history: [None; NETWORK_HISTORY_CAP],
            history_len: 0,
        }
    }
}
