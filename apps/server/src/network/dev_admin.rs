//! DEV-only loopback control plane for Developer Hub server commands.
//!
//! The Hub never mutates `World` directly and never pretends to be a gameplay
//! client. Requests are accepted only on localhost and are handed to the
//! existing bounded gameplay channel owned by [`super::gameplay::GameplayOwner`].

use std::sync::{Arc, Mutex};

use purgatory_common::{
    ContentId, DEFAULT_DEV_ADMIN_PORT, DEV_ADMIN_MAX_LINE_BYTES, DEV_ADMIN_PORT_ENV,
    DevAdminContentEntry, DevAdminPlayer, DevAdminRequest, DevAdminResponse, DevAdminSnapshot,
};
use purgatory_content::{LoadMode, default_content_root, load_registry};
use purgatory_protocol::ConnectionId;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use super::gameplay::GameplayTx;
use super::session::{SessionTable, lock_sessions};

pub(crate) fn spawn(
    gameplay: GameplayTx,
    sessions: Arc<Mutex<SessionTable>>,
) -> Result<(), String> {
    let port = std::env::var(DEV_ADMIN_PORT_ENV)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_DEV_ADMIN_PORT);

    let registry = load_registry(&default_content_root(), LoadMode::Full)
        .map_err(|err| format!("dev admin content: {err}"))?;
    let mut npcs = registry
        .iter_npc_dialogue_presentations()
        .filter_map(|npc| {
            let content_id = u32::try_from(npc.content_id.token()).ok()?;
            Some(DevAdminContentEntry {
                content_id,
                authored_id: npc.authored_id.clone(),
            })
        })
        .collect::<Vec<_>>();
    npcs.sort_by(|a, b| a.authored_id.cmp(&b.authored_id));
    let npcs = Arc::new(npcs);

    tokio::spawn(async move {
        let addr = format!("127.0.0.1:{port}");
        let listener = match TcpListener::bind(&addr).await {
            Ok(listener) => listener,
            Err(err) => {
                eprintln!("DEV_ADMIN disabled bind={addr} error={err}");
                return;
            }
        };
        println!("DEV_ADMIN listening on {addr}");

        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(pair) => pair,
                Err(err) => {
                    eprintln!("DEV_ADMIN accept error={err}");
                    continue;
                }
            };
            if !peer.ip().is_loopback() {
                eprintln!("DEV_ADMIN reject non-loopback peer={peer}");
                continue;
            }
            let gameplay = gameplay.clone();
            let sessions = Arc::clone(&sessions);
            let npcs = Arc::clone(&npcs);
            tokio::spawn(async move {
                if let Err(err) = handle_connection(stream, gameplay, sessions, npcs).await {
                    eprintln!("DEV_ADMIN connection error={err}");
                }
            });
        }
    });

    Ok(())
}

async fn handle_connection(
    stream: TcpStream,
    gameplay: GameplayTx,
    sessions: Arc<Mutex<SessionTable>>,
    npcs: Arc<Vec<DevAdminContentEntry>>,
) -> Result<(), String> {
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|err| format!("read: {err}"))?
    {
        if line.len() > DEV_ADMIN_MAX_LINE_BYTES {
            write_response(
                &mut write_half,
                &DevAdminResponse::command_err("request too large"),
            )
            .await?;
            continue;
        }
        let response = match serde_json::from_str::<DevAdminRequest>(&line) {
            Ok(request) => dispatch(request, &gameplay, &sessions, &npcs).await,
            Err(err) => DevAdminResponse::command_err(format!("invalid request: {err}")),
        };
        write_response(&mut write_half, &response).await?;
    }
    Ok(())
}

async fn write_response(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    response: &DevAdminResponse,
) -> Result<(), String> {
    let mut encoded = serde_json::to_vec(response).map_err(|err| format!("encode: {err}"))?;
    encoded.push(b'\n');
    writer
        .write_all(&encoded)
        .await
        .map_err(|err| format!("write: {err}"))
}

async fn dispatch(
    request: DevAdminRequest,
    gameplay: &GameplayTx,
    sessions: &Arc<Mutex<SessionTable>>,
    npcs: &[DevAdminContentEntry],
) -> DevAdminResponse {
    match request {
        DevAdminRequest::Snapshot => {
            let players = lock_sessions(sessions)
                .connection_ids()
                .into_iter()
                .map(|id| DevAdminPlayer {
                    connection_id: id.get(),
                })
                .collect();
            DevAdminResponse::Snapshot {
                snapshot: DevAdminSnapshot {
                    players,
                    npcs: npcs.to_vec(),
                },
            }
        }
        DevAdminRequest::SpawnNpc {
            connection_id,
            npc_content_id,
        } => {
            if !session_exists(sessions, connection_id) {
                return DevAdminResponse::command_err(format!(
                    "connection {connection_id} is not active"
                ));
            }
            let id = ContentId::from_raw(npc_content_id);
            if !npcs.iter().any(|npc| npc.content_id == npc_content_id) {
                return DevAdminResponse::command_err(format!(
                    "ContentId {npc_content_id} is not an authored NPC"
                ));
            }
            accepted(
                gameplay
                    .send_dev_spawn_npc(ConnectionId::from_raw(connection_id), id)
                    .await,
                format!("Spawn NPC {npc_content_id} near connection {connection_id}"),
            )
        }
        DevAdminRequest::ResetPlayer { connection_id } => {
            if !session_exists(sessions, connection_id) {
                return DevAdminResponse::command_err(format!(
                    "connection {connection_id} is not active"
                ));
            }
            accepted(
                gameplay
                    .send_dev_reset_player(ConnectionId::from_raw(connection_id))
                    .await,
                format!("Reset connection {connection_id} to map spawn"),
            )
        }
        DevAdminRequest::SetChannel {
            connection_id,
            channel,
        } => {
            if !session_exists(sessions, connection_id) {
                return DevAdminResponse::command_err(format!(
                    "connection {connection_id} is not active"
                ));
            }
            accepted(
                gameplay
                    .send_dev_set_channel(ConnectionId::from_raw(connection_id), channel)
                    .await,
                format!("Set connection {connection_id} channel to {channel}"),
            )
        }
        DevAdminRequest::SetSpeed {
            connection_id,
            hundredths,
        } => {
            if !session_exists(sessions, connection_id) {
                return DevAdminResponse::command_err(format!(
                    "connection {connection_id} is not active"
                ));
            }
            accepted(
                gameplay
                    .send_dev_set_speed(ConnectionId::from_raw(connection_id), hundredths)
                    .await,
                format!("Set connection {connection_id} speed override to {hundredths:?}"),
            )
        }
        DevAdminRequest::SetJump {
            connection_id,
            hundredths,
        } => {
            if !session_exists(sessions, connection_id) {
                return DevAdminResponse::command_err(format!(
                    "connection {connection_id} is not active"
                ));
            }
            accepted(
                gameplay
                    .send_dev_set_jump(ConnectionId::from_raw(connection_id), hundredths)
                    .await,
                format!("Set connection {connection_id} jump override to {hundredths:?}"),
            )
        }
    }
}

fn session_exists(sessions: &Arc<Mutex<SessionTable>>, raw: u64) -> bool {
    lock_sessions(sessions)
        .connection_ids()
        .into_iter()
        .any(|id| id.get() == raw)
}

fn accepted(ok: bool, message: String) -> DevAdminResponse {
    if ok {
        println!("DEV_ADMIN {message}");
        DevAdminResponse::command_ok(message)
    } else {
        DevAdminResponse::command_err("gameplay command queue closed")
    }
}
