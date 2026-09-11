//! Individual bot session state and network handling.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use quinn::{Connection, Endpoint, RecvStream, SendStream};

use purgatory_protocol::{
    ClientControl, ConnectionId, DevSetChannel, DisconnectReasonCode, HANDSHAKE_TIMEOUT, Hello,
    InteractRejectReason, MoveAxis, PROTOCOL_VERSION, PortalActivate, ReplicatedKind,
    ReplicationFrame, ReplicationRecord, ServerControl, ServerInteract, WireEntityId,
    decode_replication_frame, decode_server_control, encode_client_control, encode_frame,
    peek_frame_len, peek_gameplay_frame_len,
};

use crate::behavior::{BotBehavior, BotProfile};
use crate::client_build;
use crate::roles::{BotRole, ReplicaView};

/// Non-blocking poll so the single-threaded controller cannot stall.
pub const STREAM_POLL_TIMEOUT: Duration = Duration::from_millis(1);
/// Payload `read_exact` must not block other bots past the idle timeout.
pub const SNAPSHOT_PAYLOAD_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Disconnected,
    Connecting,
    Handshaking,
    Connected,
    Failed,
}

#[derive(Default)]
pub struct SessionMetrics {
    pub commands_sent: u64,
    pub snapshots_received: u64,
    pub last_snapshot_at: Option<Instant>,
    pub last_snapshot_sequence: u32,
    pub local_grounded: bool,
    pub portal_attempts: u64,
    pub portal_out_of_range: u64,
    pub portal_rejected: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ControlPoll {
    pub out_of_range: u64,
    pub rejected: u64,
    pub disconnected: bool,
}

pub struct BotSession {
    pub bot_id: u32,
    pub login: String,
    pub role: BotRole,
    pub state: SessionState,
    pub metrics: SessionMetrics,
    pub behavior: BotBehavior,
    pub intent: purgatory_protocol::IntentNet,
    pub connection_id: Option<ConnectionId>,
    pub local_entity: Option<WireEntityId>,
    pub previous_local_entity: Option<WireEntityId>,
    pub last_disconnect: Option<DisconnectReasonCode>,
    pub visible_portals: Vec<WireEntityId>,
    pub replica: ReplicaView,
    connection: Option<Connection>,
    send_stream: Option<SendStream>,
    control_recv: Option<RecvStream>,
    snapshot_recv: Option<RecvStream>,
    /// Client-owned connect-attempt start (not a server timestamp).
    pub attempt_to_quic_ready_us: Option<u64>,
    pub quic_ready_to_welcome_us: Option<u64>,
    pub attempt_to_welcome_us: Option<u64>,
}

impl BotSession {
    pub fn new(bot_id: u32, profile: BotProfile, seed: u64) -> Self {
        Self {
            bot_id,
            login: String::new(),
            role: BotRole::PersistentMove,
            state: SessionState::Disconnected,
            metrics: SessionMetrics::default(),
            behavior: BotBehavior::new(profile, seed, bot_id),
            intent: purgatory_protocol::IntentNet::default(),
            connection_id: None,
            local_entity: None,
            previous_local_entity: None,
            last_disconnect: None,
            visible_portals: Vec::new(),
            replica: ReplicaView::default(),
            connection: None,
            send_stream: None,
            control_recv: None,
            snapshot_recv: None,
            attempt_to_quic_ready_us: None,
            quic_ready_to_welcome_us: None,
            attempt_to_welcome_us: None,
        }
    }

    pub async fn connect(&mut self, endpoint: &Endpoint, server: SocketAddr) -> Result<(), String> {
        let login = format!("bot.{:04}", self.bot_id);
        self.connect_with_login(endpoint, server, &login).await
    }

    pub async fn connect_with_login(
        &mut self,
        endpoint: &Endpoint,
        server: SocketAddr,
        dev_login: &str,
    ) -> Result<(), String> {
        self.state = SessionState::Connecting;
        self.login = dev_login.to_string();
        self.last_disconnect = None;
        self.previous_local_entity = self.local_entity.take();
        self.local_entity = None;
        self.visible_portals.clear();
        self.replica = ReplicaView::default();
        self.control_recv = None;
        self.snapshot_recv = None;
        self.attempt_to_quic_ready_us = None;
        self.quic_ready_to_welcome_us = None;
        self.attempt_to_welcome_us = None;

        let attempt = Instant::now();
        let connecting = endpoint
            .connect(server, "localhost")
            .map_err(|e| format!("connect error: {e}"))?;

        let connection = tokio::time::timeout(HANDSHAKE_TIMEOUT, connecting)
            .await
            .map_err(|_| "QUIC connect timeout".to_string())?
            .map_err(|e| format!("QUIC connect failed: {e}"))?;
        let quic_ready = Instant::now();
        self.attempt_to_quic_ready_us = Some(us(attempt.elapsed()));

        self.state = SessionState::Handshaking;

        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|e| format!("open bi stream: {e}"))?;

        let hello = ClientControl::Hello(Hello {
            protocol_version: PROTOCOL_VERSION,
            client_build: client_build(),
            dev_login: dev_login.to_string(),
        });
        write_client_control(&mut send, &hello).await?;

        let control_msg = tokio::time::timeout(HANDSHAKE_TIMEOUT, read_server_control(&mut recv))
            .await
            .map_err(|_| "Welcome timeout".to_string())?
            .map_err(|e| format!("read Welcome: {e}"))?;

        match control_msg {
            ServerControl::Welcome(welcome) => {
                self.intent.set_epoch(0);
                self.connection_id = Some(welcome.connection_id);
                self.connection = Some(connection);
                self.send_stream = Some(send);
                self.control_recv = Some(recv);
                self.state = SessionState::Connected;
                self.quic_ready_to_welcome_us = Some(us(quic_ready.elapsed()));
                self.attempt_to_welcome_us = Some(us(attempt.elapsed()));
                Ok(())
            }
            ServerControl::Disconnect(reason) => {
                self.last_disconnect = Some(reason.code);
                self.state = SessionState::Failed;
                Err(format!("server rejected: {}", reason.code))
            }
            ServerControl::Interact(_) => {
                self.state = SessionState::Failed;
                Err("unexpected interact during handshake".into())
            }
            ServerControl::DialogueLine(_) => {
                self.state = SessionState::Failed;
                Err("unexpected dialogue line during handshake".into())
            }
            ServerControl::DialogueChoiceAccepted(_) => {
                self.state = SessionState::Failed;
                Err("unexpected dialogue choice during handshake".into())
            }
            ServerControl::Equipment(_) => {
                self.state = SessionState::Failed;
                Err("unexpected equipment during handshake".into())
            }
            ServerControl::PresentationOneShot(_) => {
                self.state = SessionState::Failed;
                Err("unexpected presentation oneshot during handshake".into())
            }
            ServerControl::Ability(_) => {
                self.state = SessionState::Failed;
                Err("unexpected ability during handshake".into())
            }
            ServerControl::Item(_) => {
                self.state = SessionState::Failed;
                Err("unexpected item result during handshake".into())
            }
            ServerControl::Inventory(_) => {
                self.state = SessionState::Failed;
                Err("unexpected inventory during handshake".into())
            }
        }
    }

    /// Non-blocking uni accept. Must not stall the controller tick loop.
    pub async fn poll_accept_snapshot(&mut self) -> Result<(), String> {
        if self.snapshot_recv.is_some() {
            return Ok(());
        }
        let conn = self.connection.as_ref().ok_or("not connected")?;
        match tokio::time::timeout(STREAM_POLL_TIMEOUT, conn.accept_uni()).await {
            Ok(Ok(stream)) => {
                self.snapshot_recv = Some(stream);
                Ok(())
            }
            Ok(Err(e)) => Err(format!("accept uni: {e}")),
            Err(_) => Ok(()),
        }
    }

    pub async fn accept_snapshot_stream(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            self.poll_accept_snapshot().await?;
            if self.snapshot_recv.is_some() {
                return Ok(());
            }
            tokio::time::sleep(STREAM_POLL_TIMEOUT).await;
        }
        Err("accept uni timeout".into())
    }

    pub async fn poll_snapshot(&mut self) -> Result<Option<ReplicationFrame>, String> {
        let Some(recv) = self.snapshot_recv.as_mut() else {
            return Ok(None);
        };

        let mut prefix = [0u8; 4];
        match tokio::time::timeout(STREAM_POLL_TIMEOUT, recv.read_exact(&mut prefix)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                self.snapshot_recv = None;
                return Ok(None);
            }
            Err(_) => return Ok(None),
        }

        let len = peek_gameplay_frame_len(&prefix).map_err(|_| "framing broken")?;
        let mut payload = vec![0u8; len as usize];
        match tokio::time::timeout(SNAPSHOT_PAYLOAD_TIMEOUT, recv.read_exact(&mut payload)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                self.snapshot_recv = None;
                return Err(format!("read snapshot payload: {e}"));
            }
            Err(_) => {
                self.snapshot_recv = None;
                return Err("snapshot payload timeout".into());
            }
        }

        let frame = decode_replication_frame(&payload).map_err(|e| format!("decode frame: {e}"))?;

        self.metrics.snapshots_received += 1;
        self.metrics.last_snapshot_at = Some(Instant::now());
        self.metrics.last_snapshot_sequence = frame.snapshot_sequence;
        self.metrics.local_grounded = frame.local_grounded;
        self.intent.set_epoch(frame.input_epoch);
        self.behavior.on_snapshot(frame.local_grounded);
        self.local_entity = Some(frame.local_player_entity);
        self.replica.apply_frame(&frame);
        for record in &frame.records {
            match record {
                ReplicationRecord::Enter { entity, .. }
                    if entity.kind == ReplicatedKind::Portal =>
                {
                    if !self.visible_portals.contains(&entity.entity_id) {
                        self.visible_portals.push(entity.entity_id);
                    }
                }
                ReplicationRecord::Leave { entity_id } => {
                    self.visible_portals.retain(|id| id != entity_id);
                }
                _ => {}
            }
        }

        Ok(Some(frame))
    }

    pub async fn poll_control(&mut self) -> Result<ControlPoll, String> {
        let mut poll = ControlPoll::default();
        let Some(recv) = self.control_recv.as_mut() else {
            return Ok(poll);
        };

        let mut prefix = [0u8; 4];
        match tokio::time::timeout(STREAM_POLL_TIMEOUT, recv.read_exact(&mut prefix)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                self.control_recv = None;
                return Ok(poll);
            }
            Err(_) => return Ok(poll),
        }

        let len = peek_frame_len(&prefix).map_err(|e| format!("peek len: {e}"))?;
        let mut payload = vec![0u8; len as usize];
        match tokio::time::timeout(SNAPSHOT_PAYLOAD_TIMEOUT, recv.read_exact(&mut payload)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(format!("read control payload: {e}")),
            Err(_) => return Err("control payload timeout".into()),
        }

        match decode_server_control(&payload).map_err(|e| format!("decode control: {e}"))? {
            ServerControl::Disconnect(reason) => {
                self.last_disconnect = Some(reason.code);
                poll.disconnected = true;
            }
            ServerControl::Interact(ServerInteract::Rejected { reason, .. }) => {
                poll.rejected = 1;
                self.metrics.portal_rejected = self.metrics.portal_rejected.saturating_add(1);
                if reason == InteractRejectReason::OutOfRange {
                    poll.out_of_range = 1;
                    self.metrics.portal_out_of_range =
                        self.metrics.portal_out_of_range.saturating_add(1);
                }
            }
            ServerControl::Welcome(_)
            | ServerControl::Interact(_)
            | ServerControl::DialogueLine(_)
            | ServerControl::DialogueChoiceAccepted(_)
            | ServerControl::Equipment(_)
            | ServerControl::PresentationOneShot(_)
            | ServerControl::Ability(_)
            | ServerControl::Item(_)
            | ServerControl::Inventory(_) => {}
        }
        Ok(poll)
    }

    pub async fn send_tick(&mut self) -> Result<(), String> {
        let action = self.behavior.next_action();
        let (axis, jump, down) = self.behavior.action_to_input(action);
        self.send_input(axis, jump, down).await
    }

    pub async fn send_input(
        &mut self,
        axis: MoveAxis,
        jump: bool,
        down: bool,
    ) -> Result<(), String> {
        let Some(send) = self.send_stream.as_mut() else {
            return Err("not connected".into());
        };
        let cmd = self
            .intent
            .emit_tick(axis, jump, down)
            .ok_or("sequence overflow")?;
        write_client_control(send, &ClientControl::Input(cmd)).await?;
        self.metrics.commands_sent += 1;
        Ok(())
    }

    pub async fn send_portal_activate(&mut self, target: WireEntityId) -> Result<(), String> {
        let Some(send) = self.send_stream.as_mut() else {
            return Err("not connected".into());
        };
        write_client_control(
            send,
            &ClientControl::PortalActivate(PortalActivate { target }),
        )
        .await?;
        self.metrics.portal_attempts = self.metrics.portal_attempts.saturating_add(1);
        Ok(())
    }

    pub async fn send_dev_set_channel(&mut self, channel: u32) -> Result<(), String> {
        let Some(send) = self.send_stream.as_mut() else {
            return Err("not connected".into());
        };
        write_client_control(
            send,
            &ClientControl::DevSetChannel(DevSetChannel { channel }),
        )
        .await
    }

    /// Close and Hello again with the same DevLogin. CharacterId is not on Welcome.
    pub async fn reconnect_same_login(
        &mut self,
        endpoint: &Endpoint,
        server: SocketAddr,
    ) -> Result<(), String> {
        let login = self.login.clone();
        if login.is_empty() {
            return Err("no login to reconnect".into());
        }
        self.close_and_wait(Duration::from_millis(200)).await;
        self.connect_with_login(endpoint, server, &login).await?;
        Ok(())
    }

    #[must_use]
    pub fn entity_changed_on_reconnect(&self) -> Option<bool> {
        match (self.previous_local_entity, self.local_entity) {
            (Some(prev), Some(now)) => Some(prev != now),
            _ => None,
        }
    }

    pub async fn close(&mut self) {
        if let Some(conn) = self.connection.take() {
            conn.close(0u32.into(), b"bot shutdown");
        }
        self.send_stream = None;
        self.control_recv = None;
        self.snapshot_recv = None;
        self.state = SessionState::Disconnected;
    }

    pub async fn close_and_wait(&mut self, wait: Duration) {
        if let Some(conn) = self.connection.take() {
            conn.close(0u32.into(), b"bot shutdown");
            let _ = tokio::time::timeout(wait, conn.closed()).await;
        }
        self.send_stream = None;
        self.control_recv = None;
        self.snapshot_recv = None;
        self.state = SessionState::Disconnected;
    }
}

fn us(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

async fn write_client_control(send: &mut SendStream, msg: &ClientControl) -> Result<(), String> {
    let payload = encode_client_control(msg).map_err(|e| format!("encode: {e}"))?;
    let frame = encode_frame(&payload).map_err(|e| format!("frame: {e}"))?;
    send.write_all(&frame)
        .await
        .map_err(|e| format!("write: {e}"))
}

async fn read_server_control(recv: &mut RecvStream) -> Result<ServerControl, String> {
    let mut prefix = [0u8; 4];
    recv.read_exact(&mut prefix)
        .await
        .map_err(|e| format!("read prefix: {e}"))?;
    let len = peek_frame_len(&prefix).map_err(|e| format!("peek len: {e}"))?;
    let mut payload = vec![0u8; len as usize];
    recv.read_exact(&mut payload)
        .await
        .map_err(|e| format!("read payload: {e}"))?;
    decode_server_control(&payload).map_err(|e| format!("decode: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_timeouts_are_shorter_than_idle() {
        assert!(STREAM_POLL_TIMEOUT < Duration::from_secs(1));
        assert!(SNAPSHOT_PAYLOAD_TIMEOUT < purgatory_protocol::IDLE_TIMEOUT);
        assert!(SNAPSHOT_PAYLOAD_TIMEOUT > STREAM_POLL_TIMEOUT);
    }
}
