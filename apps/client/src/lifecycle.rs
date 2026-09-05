//! Top-level client screen vs network lifecycle. Not a pile of booleans.
//!
//! Allowed `ConnectionState` transitions (invalid events are ignored, never panic):
//!
//! ```text
//! Disconnected → Connecting
//! Rejected     → Connecting
//! Connecting   → Handshaking | Disconnected
//! Handshaking  → Connected | Rejected | Disconnected
//! Connected    → Disconnected
//! ```
//!
//! `RttUpdated` is legal only while `Connected` and never changes lifecycle.
//! Stale `event.attempt_id != active_attempt` is ignored centrally in `apply`.

use std::net::SocketAddr;

use crate::network::NetworkFailureKind;
use crate::network::state::{
    ConnectionAttemptId, ConnectionState, NetworkEvent, NetworkSnapshot, NetworkView,
};

/// What the user is viewing. Intentionally small; no Login/Channel placeholders.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientScreen {
    Connection,
    Game,
}

/// Screen + network view with legal transitions and attempt ownership.
pub struct ClientLifecycle {
    screen: ClientScreen,
    view: NetworkView,
    log_lifecycle: bool,
    log_verbose: bool,
}

impl ClientLifecycle {
    #[must_use]
    pub fn new(server: SocketAddr) -> Self {
        Self {
            screen: ClientScreen::Connection,
            view: NetworkView::new(server),
            log_lifecycle: false,
            log_verbose: false,
        }
    }

    #[must_use]
    pub fn screen(&self) -> ClientScreen {
        self.screen
    }

    #[must_use]
    pub fn view(&self) -> &NetworkView {
        &self.view
    }

    #[must_use]
    pub fn gameplay_actions_allowed(&self) -> bool {
        matches!(self.screen, ClientScreen::Game)
    }

    #[must_use]
    pub fn can_connect(&self) -> bool {
        self.view.can_connect()
    }

    /// New attempt id and Connecting. None if a session is already in progress.
    pub fn try_begin_connect(&mut self) -> Option<ConnectionAttemptId> {
        let id = self.view.try_begin_connect()?;
        self.screen = ClientScreen::Connection;
        Some(id)
    }

    /// Retire the active attempt immediately, then the caller should `try_send(Disconnect)`.
    pub fn request_disconnect(&mut self) -> bool {
        let should_send = !self.view.active_attempt.is_none()
            || matches!(
                self.view.state,
                ConnectionState::Connecting
                    | ConnectionState::Handshaking
                    | ConnectionState::Connected
            );
        self.view.invalidate_attempt();
        self.screen = ClientScreen::Connection;
        should_send
    }

    pub fn fail_unsent_connect(&mut self) {
        self.view.fail_unsent_connect();
        self.screen = ClientScreen::Connection;
        self.emit_log("ConnectFailed (command dropped)");
    }

    pub fn set_events_dropped(&mut self, dropped: u64) {
        self.view.set_dropped_telemetry(dropped);
    }

    pub fn set_log_flags(&mut self, lifecycle: bool, verbose: bool) {
        self.log_lifecycle = lifecycle;
        self.log_verbose = verbose;
    }

    pub fn clear_history(&mut self) {
        self.view.clear_history();
    }

    #[must_use]
    pub fn snapshot(&self) -> NetworkSnapshot {
        let screen = match self.screen {
            ClientScreen::Connection => "Connection",
            ClientScreen::Game => "Game",
        };
        self.view.snapshot(screen)
    }

    /// Central stale-event filter. `event.attempt_id != active_attempt` never
    /// mutates connection or screen state.
    pub fn apply(&mut self, event: NetworkEvent) {
        let attempt_id = event.attempt_id();
        if attempt_id.is_none() || attempt_id != self.view.active_attempt {
            let record = self.log_verbose && event.is_lifecycle();
            self.view.note_stale(record, attempt_id.get());
            return;
        }
        if !self.is_legal(&event) {
            self.view.note_stale(self.log_verbose, attempt_id.get());
            return;
        }
        self.log_event(&event);
        self.view.apply_trusted(event);
        self.sync_screen();
    }

    fn log_event(&self, event: &NetworkEvent) {
        if !self.log_lifecycle && !self.log_verbose {
            return;
        }
        match event {
            NetworkEvent::RttUpdated { .. } => {}
            NetworkEvent::Connecting { attempt_id } => {
                self.emit_log(&format!("attempt={attempt_id} Connecting"));
            }
            NetworkEvent::Handshaking { attempt_id } => {
                self.emit_log(&format!("attempt={attempt_id} Handshaking"));
            }
            NetworkEvent::Connected {
                attempt_id,
                connection_id,
                ..
            } => {
                self.emit_log(&format!(
                    "attempt={attempt_id} Connected connection_id={connection_id}"
                ));
            }
            NetworkEvent::Rejected { attempt_id, reason } => {
                let kind = NetworkFailureKind::from_wire(reason.code);
                self.emit_log(&format!(
                    "attempt={attempt_id} {} retryable={}",
                    kind.debug_label(),
                    kind.retryable()
                ));
            }
            NetworkEvent::Disconnected { attempt_id, kind } => {
                self.emit_log(&format!(
                    "attempt={attempt_id} {} retryable={}",
                    kind.debug_label(),
                    kind.retryable()
                ));
            }
            NetworkEvent::Interact { event, .. } => {
                self.emit_log(&format!("Interact {event:?}"));
            }
            NetworkEvent::Equipment { event, .. } => {
                self.emit_log(&format!("Equipment {event:?}"));
            }
            NetworkEvent::PresentationOneShot { event, .. } => {
                self.emit_log(&format!("PresentationOneShot {event:?}"));
            }
            NetworkEvent::Ability { event, .. } => {
                self.emit_log(&format!("Ability {event:?}"));
            }
        }
    }

    fn emit_log(&self, line: &str) {
        if self.log_lifecycle || self.log_verbose {
            println!("PURGATORY net {line}");
        }
    }

    fn is_legal(&self, event: &NetworkEvent) -> bool {
        let state = self.view.state;
        match event {
            NetworkEvent::Connecting { .. } => {
                matches!(state, ConnectionState::Connecting)
            }
            NetworkEvent::Handshaking { .. } => {
                matches!(state, ConnectionState::Connecting)
            }
            NetworkEvent::Connected { .. } => {
                matches!(state, ConnectionState::Handshaking)
            }
            NetworkEvent::Rejected { .. } => {
                matches!(state, ConnectionState::Handshaking)
            }
            NetworkEvent::Disconnected { .. } => matches!(
                state,
                ConnectionState::Connecting
                    | ConnectionState::Handshaking
                    | ConnectionState::Connected
            ),
            NetworkEvent::RttUpdated { .. } => {
                matches!(state, ConnectionState::Connected)
            }
            NetworkEvent::Interact { .. } => {
                matches!(state, ConnectionState::Connected)
            }
            NetworkEvent::Equipment { .. }
            | NetworkEvent::PresentationOneShot { .. }
            | NetworkEvent::Ability { .. } => {
                matches!(state, ConnectionState::Connected)
            }
        }
    }

    fn sync_screen(&mut self) {
        if self.view.state == ConnectionState::Connected && self.view.connection_id.is_some() {
            self.screen = ClientScreen::Game;
        } else {
            self.screen = ClientScreen::Connection;
        }
        if self.screen == ClientScreen::Game && self.view.state != ConnectionState::Connected {
            self.screen = ClientScreen::Connection;
        }
        if self.view.state == ConnectionState::Connected && self.view.connection_id.is_none() {
            eprintln!("PURGATORY lifecycle: Connected with no ConnectionId; returning to frontend");
            self.view.fail_unsent_connect();
            self.screen = ClientScreen::Connection;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::NETWORK_HISTORY_CAP;
    use purgatory_protocol::{
        ConnectionId, DisconnectReason, DisconnectReasonCode, PROTOCOL_VERSION,
    };
    use std::time::Duration;

    const SERVER: SocketAddr = purgatory_protocol::dev_socket_addr();

    fn attempt(n: u64) -> ConnectionAttemptId {
        ConnectionAttemptId::from_raw(n)
    }

    fn connecting(n: u64) -> NetworkEvent {
        NetworkEvent::Connecting {
            attempt_id: attempt(n),
        }
    }

    #[test]
    fn interact_event_is_lifecycle_not_droppable_telemetry() {
        let event = NetworkEvent::Interact {
            attempt_id: attempt(1),
            event: purgatory_protocol::ServerInteract::Rejected {
                target: purgatory_protocol::WireEntityId {
                    index: 1,
                    generation: 1,
                },
                reason: purgatory_protocol::InteractRejectReason::OutOfRange,
            },
        };
        assert!(event.is_lifecycle());
        let rtt = NetworkEvent::RttUpdated {
            attempt_id: attempt(1),
            rtt: Duration::from_millis(10),
        };
        assert!(!rtt.is_lifecycle());
    }

    fn handshaking(n: u64) -> NetworkEvent {
        NetworkEvent::Handshaking {
            attempt_id: attempt(n),
        }
    }

    fn welcome(n: u64, conn: u64) -> NetworkEvent {
        NetworkEvent::Connected {
            attempt_id: attempt(n),
            connection_id: ConnectionId::from_raw(conn),
            protocol_version: PROTOCOL_VERSION,
            server_tick_rate: 30,
        }
    }

    fn rejected(n: u64) -> NetworkEvent {
        NetworkEvent::Rejected {
            attempt_id: attempt(n),
            reason: DisconnectReason::new(DisconnectReasonCode::VersionMismatch, "v"),
        }
    }

    fn disconnected(n: u64, kind: NetworkFailureKind) -> NetworkEvent {
        NetworkEvent::Disconnected {
            attempt_id: attempt(n),
            kind,
        }
    }

    fn rtt(n: u64) -> NetworkEvent {
        NetworkEvent::RttUpdated {
            attempt_id: attempt(n),
            rtt: Duration::from_millis(12),
        }
    }

    fn start_handshaking(life: &mut ClientLifecycle) -> ConnectionAttemptId {
        let id = life.try_begin_connect().expect("connect");
        life.apply(connecting(id.get()));
        life.apply(handshaking(id.get()));
        id
    }

    #[test]
    fn a_startup_is_connection_disconnected() {
        let life = ClientLifecycle::new(SERVER);
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert_eq!(life.view().state, ConnectionState::Disconnected);
        assert!(life.view().active_attempt.is_none());
        assert!(life.view().connection_id.is_none());
        assert!(life.can_connect());
    }

    #[test]
    fn b_connect_allocates_attempt_and_enters_connecting() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = life.try_begin_connect().expect("connect");
        assert_eq!(id.get(), 1);
        assert_eq!(life.view().state, ConnectionState::Connecting);
        assert_eq!(life.view().active_attempt, id);
        assert_eq!(life.screen(), ClientScreen::Connection);
    }

    #[test]
    fn c_double_connect_during_connecting_is_ignored() {
        let mut life = ClientLifecycle::new(SERVER);
        let first = life.try_begin_connect().expect("first");
        assert!(life.try_begin_connect().is_none());
        assert_eq!(life.view().active_attempt, first);
        assert_eq!(life.view().state, ConnectionState::Connecting);
    }

    #[test]
    fn d_double_connect_during_handshaking_is_ignored() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        assert!(life.try_begin_connect().is_none());
        assert_eq!(life.view().active_attempt, id);
        assert_eq!(life.view().state, ConnectionState::Handshaking);
    }

    #[test]
    fn e_failed_attempt_stays_connection_and_retry_allowed() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = life.try_begin_connect().expect("connect");
        life.apply(connecting(id.get()));
        life.apply(disconnected(id.get(), NetworkFailureKind::ConnectFailed));
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert_eq!(life.view().state, ConnectionState::Disconnected);
        assert!(life.can_connect());
        assert!(life.view().connection_id.is_none());
    }

    #[test]
    fn f_retry_uses_newer_attempt_id() {
        let mut life = ClientLifecycle::new(SERVER);
        let a = life.try_begin_connect().expect("a");
        life.apply(disconnected(a.get(), NetworkFailureKind::ConnectFailed));
        let b = life.try_begin_connect().expect("b");
        assert!(b.get() > a.get());
        assert_eq!(b.get(), 2);
    }

    #[test]
    fn g_welcome_for_active_attempt_enters_game() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 7));
        assert_eq!(life.screen(), ClientScreen::Game);
        assert_eq!(life.view().state, ConnectionState::Connected);
        assert_eq!(life.view().connection_id.map(|c| c.get()), Some(7));
    }

    #[test]
    fn h_welcome_for_stale_attempt_is_ignored() {
        let mut life = ClientLifecycle::new(SERVER);
        let a = life.try_begin_connect().expect("a");
        life.apply(disconnected(a.get(), NetworkFailureKind::ConnectFailed));
        let b = start_handshaking(&mut life);
        life.apply(welcome(a.get(), 1));
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert_eq!(life.view().state, ConnectionState::Handshaking);
        assert_eq!(life.view().active_attempt, b);
        assert!(life.view().connection_id.is_none());
    }

    #[test]
    fn i_disconnected_for_stale_attempt_is_ignored() {
        let mut life = ClientLifecycle::new(SERVER);
        let a = life.try_begin_connect().expect("a");
        life.apply(disconnected(a.get(), NetworkFailureKind::ConnectFailed));
        let b = life.try_begin_connect().expect("b");
        life.apply(connecting(b.get()));
        life.apply(disconnected(a.get(), NetworkFailureKind::TransportLost));
        assert_eq!(life.view().state, ConnectionState::Connecting);
        assert_eq!(life.view().active_attempt, b);
        assert_eq!(life.screen(), ClientScreen::Connection);
    }

    #[test]
    fn j_disconnect_active_game_returns_to_connection() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 3));
        assert_eq!(life.screen(), ClientScreen::Game);
        life.apply(disconnected(id.get(), NetworkFailureKind::TransportLost));
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert_eq!(life.view().state, ConnectionState::Disconnected);
        assert!(life.view().connection_id.is_none());
    }

    #[test]
    fn k_rejected_active_attempt_stays_connection() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(rejected(id.get()));
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert_eq!(life.view().state, ConnectionState::Rejected);
        assert!(life.can_connect());
        assert!(life.view().connection_id.is_none());
    }

    #[test]
    fn l_connection_id_cleared_on_disconnect() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 9));
        assert!(life.view().connection_id.is_some());
        life.apply(disconnected(
            id.get(),
            NetworkFailureKind::ClientRequestedDisconnect,
        ));
        assert!(life.view().connection_id.is_none());
    }

    #[test]
    fn m_retry_does_not_reuse_old_connection_id() {
        let mut life = ClientLifecycle::new(SERVER);
        let a = start_handshaking(&mut life);
        life.apply(welcome(a.get(), 4));
        life.apply(disconnected(a.get(), NetworkFailureKind::TransportLost));
        let b = start_handshaking(&mut life);
        assert!(life.view().connection_id.is_none());
        life.apply(welcome(b.get(), 11));
        assert_eq!(life.view().connection_id.map(|c| c.get()), Some(11));
        assert_ne!(b.get(), a.get());
    }

    #[test]
    fn stale_failure_does_not_mutate_newer_attempt() {
        let mut life = ClientLifecycle::new(SERVER);
        let a = life.try_begin_connect().expect("1");
        life.apply(connecting(a.get()));
        life.apply(disconnected(a.get(), NetworkFailureKind::ConnectFailed));
        let b = life.try_begin_connect().expect("2");
        life.apply(connecting(b.get()));
        life.apply(handshaking(b.get()));
        life.apply(disconnected(a.get(), NetworkFailureKind::ConnectFailed));
        assert_eq!(life.view().active_attempt, b);
        assert_eq!(life.view().state, ConnectionState::Handshaking);
        assert_eq!(life.screen(), ClientScreen::Connection);
    }

    #[test]
    fn connected_then_immediate_disconnect_converges() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 2));
        life.apply(disconnected(id.get(), NetworkFailureKind::TransportLost));
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert_ne!(life.view().state, ConnectionState::Connected);
        assert!(life.view().connection_id.is_none());
        assert_eq!(life.view().frontend_status(), "Connection lost");
    }

    #[test]
    fn welcome_from_connecting_does_not_enter_game() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = life.try_begin_connect().expect("connect");
        life.apply(connecting(id.get()));
        life.apply(welcome(id.get(), 1));
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert_eq!(life.view().state, ConnectionState::Connecting);
        assert!(life.view().connection_id.is_none());
    }

    #[test]
    fn delayed_welcome_after_disconnect_is_ignored() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        let old = id.get();
        assert!(life.request_disconnect());
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert!(life.view().active_attempt.is_none());
        life.apply(welcome(old, 99));
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert_ne!(life.view().state, ConnectionState::Connected);
        assert!(life.view().connection_id.is_none());
        assert!(life.can_connect());
    }

    #[test]
    fn local_disconnect_from_game_invalidates_before_runtime() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 5));
        assert_eq!(life.screen(), ClientScreen::Game);
        life.request_disconnect();
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert!(life.view().connection_id.is_none());
        life.apply(welcome(id.get(), 5));
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert!(life.view().connection_id.is_none());
    }

    #[test]
    fn delayed_connect_failed_cannot_knock_newer_connected_offline() {
        let mut life = ClientLifecycle::new(SERVER);
        let a = life.try_begin_connect().expect("a");
        life.apply(connecting(a.get()));
        life.apply(disconnected(a.get(), NetworkFailureKind::ConnectFailed));
        let b = start_handshaking(&mut life);
        life.apply(welcome(b.get(), 8));
        assert_eq!(life.screen(), ClientScreen::Game);
        life.apply(disconnected(a.get(), NetworkFailureKind::ConnectFailed));
        assert_eq!(life.screen(), ClientScreen::Game);
        assert_eq!(life.view().state, ConnectionState::Connected);
        assert_eq!(life.view().active_attempt, b);
        assert_eq!(life.view().connection_id.map(|c| c.get()), Some(8));
    }

    #[test]
    fn delayed_disconnect_during_newer_handshaking_is_ignored() {
        let mut life = ClientLifecycle::new(SERVER);
        let a = start_handshaking(&mut life);
        life.request_disconnect();
        let b = start_handshaking(&mut life);
        life.apply(disconnected(a.get(), NetworkFailureKind::TransportLost));
        assert_eq!(life.view().state, ConnectionState::Handshaking);
        assert_eq!(life.view().active_attempt, b);
        assert_eq!(life.screen(), ClientScreen::Connection);
    }

    #[test]
    fn stale_rtt_from_old_attempt_is_ignored() {
        let mut life = ClientLifecycle::new(SERVER);
        let a = start_handshaking(&mut life);
        life.apply(welcome(a.get(), 1));
        life.apply(rtt(a.get()));
        assert!(life.view().rtt.is_some());
        life.apply(disconnected(a.get(), NetworkFailureKind::TransportLost));
        let b = start_handshaking(&mut life);
        life.apply(welcome(b.get(), 2));
        assert!(life.view().rtt.is_none());
        life.apply(rtt(a.get()));
        assert!(life.view().rtt.is_none());
        life.apply(rtt(b.get()));
        assert!(life.view().rtt.is_some());
        assert_eq!(life.view().state, ConnectionState::Connected);
        assert_eq!(life.screen(), ClientScreen::Game);
    }

    #[test]
    fn rtt_never_changes_connection_state() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 3));
        let before = life.view().state;
        life.apply(rtt(id.get()));
        assert_eq!(life.view().state, before);
        assert_eq!(life.screen(), ClientScreen::Game);
    }

    #[test]
    fn rapid_connect_commands_keep_one_attempt() {
        let mut life = ClientLifecycle::new(SERVER);
        let first = life.try_begin_connect().expect("first");
        for _ in 0..10 {
            assert!(life.try_begin_connect().is_none());
        }
        assert_eq!(life.view().active_attempt, first);
        assert_eq!(life.view().state, ConnectionState::Connecting);
    }

    #[test]
    fn disconnect_during_connecting_ignores_delayed_welcome() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = life.try_begin_connect().expect("connect");
        life.apply(connecting(id.get()));
        assert!(life.request_disconnect());
        life.apply(handshaking(id.get()));
        life.apply(welcome(id.get(), 4));
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert_ne!(life.view().state, ConnectionState::Connected);
        assert!(life.view().connection_id.is_none());
        assert!(life.can_connect());
    }

    #[test]
    fn disconnect_during_handshaking_stays_frontend() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        assert!(life.request_disconnect());
        life.apply(welcome(id.get(), 6));
        life.apply(disconnected(
            id.get(),
            NetworkFailureKind::ClientRequestedDisconnect,
        ));
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert!(life.view().connection_id.is_none());
        assert!(life.view().rtt.is_none());
        assert!(life.view().connected_since.is_none());
    }

    #[test]
    fn session_local_state_clears_on_teardown() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 9));
        life.apply(rtt(id.get()));
        assert!(life.view().connection_id.is_some());
        assert!(life.view().rtt.is_some());
        assert!(life.view().connected_since.is_some());
        assert!(life.view().protocol_version.is_some());
        life.apply(disconnected(
            id.get(),
            NetworkFailureKind::ClientRequestedDisconnect,
        ));
        assert!(life.view().connection_id.is_none());
        assert!(life.view().rtt.is_none());
        assert!(life.view().connected_since.is_none());
        assert!(life.view().protocol_version.is_none());
        assert!(life.view().server_tick_rate.is_none());
        assert_eq!(life.screen(), ClientScreen::Connection);
    }

    #[test]
    fn invalid_transitions_are_ignored() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = life.try_begin_connect().expect("connect");
        life.apply(connecting(id.get()));
        life.apply(welcome(id.get(), 1));
        life.apply(rejected(id.get()));
        life.apply(rtt(id.get()));
        assert_eq!(life.view().state, ConnectionState::Connecting);
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert!(life.view().connection_id.is_none());
        life.apply(handshaking(id.get()));
        life.apply(connecting(id.get()));
        assert_eq!(life.view().state, ConnectionState::Handshaking);
    }

    #[test]
    fn connected_without_game_mismatch_cannot_stick() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 2));
        life.apply(disconnected(id.get(), NetworkFailureKind::TransportLost));
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert_ne!(life.view().state, ConnectionState::Connected);
        assert!(life.view().connection_id.is_none());
    }

    #[test]
    fn version_mismatch_frontend_and_not_retryable() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(rejected(id.get()));
        assert_eq!(life.view().frontend_status(), "Version mismatch");
        let kind = life.view().last_failure.expect("failure");
        assert_eq!(kind, NetworkFailureKind::VersionMismatch);
        assert!(!kind.retryable());
    }

    #[test]
    fn manual_disconnect_is_not_connection_failed() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 1));
        life.request_disconnect();
        assert_eq!(life.view().frontend_status(), "Disconnected");
        assert_eq!(
            life.view().last_failure,
            Some(NetworkFailureKind::ClientRequestedDisconnect)
        );
        assert!(!NetworkFailureKind::ClientRequestedDisconnect.retryable());
    }

    #[test]
    fn stale_events_increment_counter() {
        let mut life = ClientLifecycle::new(SERVER);
        let a = life.try_begin_connect().expect("a");
        life.apply(disconnected(a.get(), NetworkFailureKind::ConnectFailed));
        let b = life.try_begin_connect().expect("b");
        life.apply(connecting(b.get()));
        let before = life.view().counters.stale_events_ignored;
        life.apply(disconnected(a.get(), NetworkFailureKind::ConnectFailed));
        assert!(life.view().counters.stale_events_ignored > before);
        assert_eq!(life.view().state, ConnectionState::Connecting);
    }

    #[test]
    fn history_does_not_record_rtt() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 3));
        let before = life.view().history.len();
        for _ in 0..20 {
            life.apply(rtt(id.get()));
        }
        assert_eq!(life.view().history.len(), before);
        assert!(life.view().counters.telemetry_events >= 20);
    }

    #[test]
    fn session_b_resets_rtt_and_connection_id() {
        let mut life = ClientLifecycle::new(SERVER);
        let a = start_handshaking(&mut life);
        life.apply(welcome(a.get(), 4));
        life.apply(rtt(a.get()));
        assert!(life.view().rtt_stats.latest.is_some());
        life.apply(disconnected(a.get(), NetworkFailureKind::TransportLost));
        let b = start_handshaking(&mut life);
        assert!(life.view().rtt.is_none());
        assert!(life.view().rtt_stats.latest.is_none());
        assert!(life.view().connection_id.is_none());
        life.apply(welcome(b.get(), 9));
        assert_eq!(life.view().connection_id.map(|c| c.get()), Some(9));
        assert!(life.view().history.len() >= 2);
        assert!(life.view().rtt.is_none());
    }

    #[test]
    fn history_is_bounded_through_lifecycle() {
        let mut life = ClientLifecycle::new(SERVER);
        for _ in 0..60 {
            let id = life.try_begin_connect().expect("connect");
            life.apply(connecting(id.get()));
            life.apply(disconnected(id.get(), NetworkFailureKind::ConnectFailed));
        }
        assert!(life.view().history.len() <= NETWORK_HISTORY_CAP);
        assert_eq!(life.view().history.len(), NETWORK_HISTORY_CAP);
    }

    #[test]
    fn handshake_timeout_is_distinct_from_idle() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(disconnected(id.get(), NetworkFailureKind::HandshakeTimeout));
        assert_eq!(life.view().frontend_status(), "Handshake timed out");
        assert_eq!(
            life.view().last_failure,
            Some(NetworkFailureKind::HandshakeTimeout)
        );
        assert_ne!(
            NetworkFailureKind::HandshakeTimeout.frontend_status(),
            NetworkFailureKind::IdleTimeout.frontend_status()
        );
        assert!(NetworkFailureKind::HandshakeTimeout.retryable());
    }

    #[test]
    fn idle_timeout_maps_to_connection_lost() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 1));
        life.apply(disconnected(id.get(), NetworkFailureKind::IdleTimeout));
        assert_eq!(life.view().frontend_status(), "Connection lost");
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert!(NetworkFailureKind::IdleTimeout.retryable());
    }

    #[test]
    fn unexpected_message_is_rejected_not_malformed_label() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 1));
        life.apply(disconnected(
            id.get(),
            NetworkFailureKind::UnexpectedMessage,
        ));
        assert_eq!(
            life.view().last_failure,
            Some(NetworkFailureKind::UnexpectedMessage)
        );
        assert_eq!(life.view().frontend_status(), "Connection rejected");
        assert!(!NetworkFailureKind::UnexpectedMessage.retryable());
        assert_ne!(
            NetworkFailureKind::UnexpectedMessage.debug_label(),
            NetworkFailureKind::MalformedMessage.debug_label()
        );
    }

    #[test]
    fn server_shutdown_is_not_transport_lost() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 1));
        life.apply(disconnected(id.get(), NetworkFailureKind::ServerShutdown));
        assert_eq!(life.view().frontend_status(), "Server shutting down");
        assert_eq!(
            life.view().last_failure,
            Some(NetworkFailureKind::ServerShutdown)
        );
        assert_eq!(life.screen(), ClientScreen::Connection);
    }

    #[test]
    fn local_shutdown_is_not_connection_lost() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 1));
        life.apply(disconnected(id.get(), NetworkFailureKind::LocalShutdown));
        assert_eq!(life.view().frontend_status(), "Disconnected");
        assert!(NetworkFailureKind::LocalShutdown.is_benign());
    }

    #[test]
    fn dropped_telemetry_counter_is_observable() {
        let mut life = ClientLifecycle::new(SERVER);
        life.set_events_dropped(7);
        assert_eq!(life.view().counters.dropped_telemetry, 7);
        assert_eq!(life.snapshot().events_dropped, 7);
    }

    // --- Phase 5.0F chaos soaks (pure state, no IO) ----------------------

    /// Deterministic PRNG so a failing soak is reproducible from its seed.
    struct Lcg(u64);

    impl Lcg {
        fn new(seed: u64) -> Self {
            Self(seed ^ 0x9E37_79B9_7F4A_7C15)
        }

        fn below(&mut self, n: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 11) % n as u64) as usize
        }
    }

    const SOAK_SEEDS: [u64; 3] = [0x1, 0x00C0_FFEE, 0xDEAD_BEEF];

    /// The two combinations that must never exist, however events interleave.
    fn assert_screen_invariant(life: &ClientLifecycle, context: &str) {
        if life.screen() == ClientScreen::Game {
            assert!(
                life.view().connection_id.is_some(),
                "{context}: Game without a ConnectionId"
            );
            assert_eq!(
                life.view().state,
                ConnectionState::Connected,
                "{context}: Game while not Connected"
            );
        }
        if life.view().state != ConnectionState::Connected {
            assert_eq!(
                life.screen(),
                ClientScreen::Connection,
                "{context}: stale Game screen"
            );
        }
    }

    #[test]
    fn welcome_disconnect_race_soak_never_produces_invalid_states() {
        for seed in SOAK_SEEDS {
            let mut rng = Lcg::new(seed);
            let mut life = ClientLifecycle::new(SERVER);
            for round in 0..200u64 {
                let context = format!("seed=0x{seed:X} round={round}");
                let id = life.try_begin_connect().expect(&context);
                life.apply(connecting(id.get()));
                life.apply(handshaking(id.get()));
                match rng.below(4) {
                    // Welcome wins, then the transport closes.
                    0 => {
                        life.apply(welcome(id.get(), round + 1));
                        assert_screen_invariant(&life, &context);
                        life.apply(disconnected(id.get(), NetworkFailureKind::TransportLost));
                    }
                    // The transport closes first; a late Welcome must not land.
                    1 => {
                        life.apply(disconnected(id.get(), NetworkFailureKind::TransportLost));
                        life.apply(welcome(id.get(), round + 1));
                    }
                    // Local disconnect retires the attempt mid-handshake.
                    2 => {
                        life.request_disconnect();
                        life.apply(welcome(id.get(), round + 1));
                        life.apply(disconnected(id.get(), NetworkFailureKind::TransportLost));
                    }
                    // Server rejection instead of Welcome.
                    _ => {
                        life.apply(rejected(id.get()));
                        life.apply(welcome(id.get(), round + 1));
                    }
                }
                assert_screen_invariant(&life, &context);
                assert!(
                    life.view().connection_id.is_none(),
                    "{context}: session state survived teardown"
                );
                assert!(life.can_connect(), "{context}: cannot retry");
            }
            assert!(life.view().history.len() <= NETWORK_HISTORY_CAP);
        }
    }

    #[test]
    fn stale_event_chaos_soak_never_mutates_the_active_attempt() {
        for seed in SOAK_SEEDS {
            let mut rng = Lcg::new(seed);
            let mut life = ClientLifecycle::new(SERVER);
            let mut retired: Vec<u64> = Vec::new();
            for round in 0..150u64 {
                let context = format!("seed=0x{seed:X} round={round}");
                let active = life.try_begin_connect().expect(&context);
                life.apply(connecting(active.get()));
                life.apply(handshaking(active.get()));
                let before = life.view().counters.stale_events_ignored;
                // Delayed events from every previously retired attempt.
                for old in retired.iter().rev().take(3) {
                    match rng.below(4) {
                        0 => life.apply(disconnected(*old, NetworkFailureKind::ConnectFailed)),
                        1 => life.apply(welcome(*old, 900 + *old)),
                        2 => life.apply(rtt(*old)),
                        _ => life.apply(rejected(*old)),
                    }
                    assert_eq!(
                        life.view().active_attempt,
                        active,
                        "{context}: stale event stole the active attempt"
                    );
                    assert_eq!(
                        life.view().state,
                        ConnectionState::Handshaking,
                        "{context}: stale event changed state"
                    );
                    assert!(life.view().connection_id.is_none());
                }
                if !retired.is_empty() {
                    assert!(
                        life.view().counters.stale_events_ignored > before,
                        "{context}: stale events were not counted"
                    );
                }
                assert_screen_invariant(&life, &context);
                life.apply(welcome(active.get(), round + 1));
                assert_eq!(life.screen(), ClientScreen::Game, "{context}");
                life.apply(disconnected(
                    active.get(),
                    NetworkFailureKind::TransportLost,
                ));
                retired.push(active.get());
            }
            assert!(life.view().counters.stale_events_ignored > 0);
            assert!(life.view().history.len() <= NETWORK_HISTORY_CAP);
        }
    }

    /// Logging is observational: OFF, ON, and verbose must produce identical
    /// state, and verbose history stays bounded by event occurrence.
    #[test]
    fn log_flags_never_change_lifecycle_state() {
        let mut quiet = ClientLifecycle::new(SERVER);
        let mut lifecycle_only = ClientLifecycle::new(SERVER);
        lifecycle_only.set_log_flags(true, false);
        let mut verbose = ClientLifecycle::new(SERVER);
        verbose.set_log_flags(true, true);
        for round in 0..20u64 {
            for life in [&mut quiet, &mut lifecycle_only, &mut verbose] {
                let id = life.try_begin_connect().expect("connect");
                life.apply(connecting(id.get()));
                life.apply(handshaking(id.get()));
                life.apply(welcome(id.get(), round + 1));
                for _ in 0..8 {
                    life.apply(rtt(id.get()));
                }
                life.apply(disconnected(id.get(), NetworkFailureKind::TransportLost));
            }
        }
        for loud in [&lifecycle_only, &verbose] {
            assert_eq!(quiet.view().state, loud.view().state);
            assert_eq!(quiet.screen(), loud.screen());
            assert_eq!(
                quiet.view().counters.lifecycle_events,
                loud.view().counters.lifecycle_events
            );
            assert_eq!(
                quiet.view().counters.telemetry_events,
                loud.view().counters.telemetry_events
            );
            assert_eq!(quiet.view().history.len(), loud.view().history.len());
            assert!(loud.view().history.len() <= NETWORK_HISTORY_CAP);
        }
    }

    #[test]
    fn diagnostic_history_stays_bounded_under_long_churn() {
        let mut life = ClientLifecycle::new(SERVER);
        for round in 0..500u64 {
            let id = life.try_begin_connect().expect("connect");
            life.apply(connecting(id.get()));
            life.apply(handshaking(id.get()));
            life.apply(welcome(id.get(), round + 1));
            for _ in 0..4 {
                life.apply(rtt(id.get()));
            }
            life.apply(disconnected(id.get(), NetworkFailureKind::TransportLost));
            // Four recorded events per cycle (RTT is telemetry, never history).
            let expected = (4 * (round as usize + 1)).min(NETWORK_HISTORY_CAP);
            assert_eq!(life.view().history.len(), expected, "round {round}");
        }
        assert_eq!(life.view().history.len(), NETWORK_HISTORY_CAP);
        // Clear History resets the ring only; counters and identity survive.
        let counters = life.view().counters;
        life.clear_history();
        assert!(life.view().history.is_empty());
        assert_eq!(
            life.view().counters.lifecycle_events,
            counters.lifecycle_events
        );
        assert!(life.can_connect());
    }

    #[test]
    fn clear_history_does_not_reset_counters() {
        let mut life = ClientLifecycle::new(SERVER);
        let id = start_handshaking(&mut life);
        life.apply(welcome(id.get(), 3));
        let events = life.view().counters.lifecycle_events;
        assert!(!life.view().history.is_empty());
        life.clear_history();
        assert!(life.view().history.is_empty());
        assert_eq!(life.view().counters.lifecycle_events, events);
        assert_eq!(life.view().connection_id.map(|c| c.get()), Some(3));
    }
}
