//! Individual bot session state and network handling.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use quinn::{Connection, Endpoint, RecvStream, SendStream};

use purgatory_protocol::{
    ClientControl, ConnectionId, DevSetChannel, DisconnectReasonCode, HANDSHAKE_TIMEOUT, Hello,
    PROTOCOL_VERSION, PortalActivate, ReplicatedKind, ReplicationFrame, ReplicationRecord,
    ServerControl, WireEntityId, decode_replication_frame, decode_server_control,
    encode_client_control, encode_frame, peek_frame_len, peek_gameplay_frame_len,
};

use crate::behavior::{BotBehavior, BotProfile};
use crate::client_build;

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
}

pub struct BotSession {
    pub bot_id: u32,
    pub login: String,
    pub state: SessionState,
    pub metrics: SessionMetrics,
    pub behavior: BotBehavior,
    pub intent: purgatory_protocol::IntentNet,
    pub connection_id: Option<ConnectionId>,
    pub local_entity: Option<WireEntityId>,
    pub previous_local_entity: Option<WireEntityId>,
    pub last_disconnect: Option<DisconnectReasonCode>,
    pub visible_portals: Vec<WireEntityId>,
    connection: Option<Connection>,
    send_stream: Option<SendStream>,
    snapshot_recv: Option<RecvStream>,
}

impl BotSession {
    pub fn new(bot_id: u32, profile: BotProfile, seed: u64) -> Self {
        Self {
            bot_id,
            login: String::new(),
            state: SessionState::Disconnected,
            metrics: SessionMetrics::default(),
            behavior: BotBehavior::new(profile, seed, bot_id),
            intent: purgatory_protocol::IntentNet::default(),
            connection_id: None,
            local_entity: None,
            previous_local_entity: None,
            last_disconnect: None,
            visible_portals: Vec::new(),
            connection: None,
            send_stream: None,
            snapshot_recv: None,
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

        let connecting = endpoint
            .connect(server, "localhost")
            .map_err(|e| format!("connect error: {e}"))?;

        let connection = tokio::time::timeout(HANDSHAKE_TIMEOUT, connecting)
            .await
            .map_err(|_| "QUIC connect timeout".to_string())?
            .map_err(|e| format!("QUIC connect failed: {e}"))?;

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
                self.state = SessionState::Connected;
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
        }
    }

    pub async fn accept_snapshot_stream(&mut self) -> Result<(), String> {
        let conn = self.connection.as_ref().ok_or("not connected")?;
        let stream = conn
            .accept_uni()
            .await
            .map_err(|e| format!("accept uni: {e}"))?;
        self.snapshot_recv = Some(stream);
        Ok(())
    }

    pub async fn poll_snapshot(&mut self) -> Result<Option<ReplicationFrame>, String> {
        let Some(recv) = self.snapshot_recv.as_mut() else {
            return Ok(None);
        };

        let mut prefix = [0u8; 4];
        match tokio::time::timeout(Duration::from_millis(1), recv.read_exact(&mut prefix)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                self.snapshot_recv = None;
                return Ok(None);
            }
            Err(_) => return Ok(None),
        }

        let len = peek_gameplay_frame_len(&prefix).map_err(|_| "framing broken")?;
        let mut payload = vec![0u8; len as usize];
        recv.read_exact(&mut payload)
            .await
            .map_err(|e| format!("read snapshot payload: {e}"))?;

        let frame = decode_replication_frame(&payload).map_err(|e| format!("decode frame: {e}"))?;

        self.metrics.snapshots_received += 1;
        self.metrics.last_snapshot_at = Some(Instant::now());
        self.metrics.last_snapshot_sequence = frame.snapshot_sequence;
        self.metrics.local_grounded = frame.local_grounded;
        self.intent.set_epoch(frame.input_epoch);
        self.behavior.on_snapshot(frame.local_grounded);
        self.local_entity = Some(frame.local_player_entity);
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

    pub async fn send_tick(&mut self) -> Result<(), String> {
        let Some(send) = self.send_stream.as_mut() else {
            return Err("not connected".into());
        };

        let action = self.behavior.next_action();
        let (axis, jump, down) = self.behavior.action_to_input(action);
        let cmd = self
            .intent
            .emit_tick(axis, jump, down)
            .ok_or("sequence overflow")?;

        let msg = ClientControl::Input(cmd);
        write_client_control(send, &msg).await?;
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
        .await
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
        self.accept_snapshot_stream().await?;
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
        self.snapshot_recv = None;
        self.state = SessionState::Disconnected;
    }

    pub async fn close_and_wait(&mut self, wait: Duration) {
        if let Some(conn) = self.connection.take() {
            conn.close(0u32.into(), b"bot shutdown");
            let _ = tokio::time::timeout(wait, conn.closed()).await;
        }
        self.send_stream = None;
        self.snapshot_recv = None;
        self.state = SessionState::Disconnected;
    }
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
