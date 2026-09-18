//! DEV-only loopback control plane for Developer Hub server commands.
//!
//! The Hub never mutates `World` directly and never pretends to be a gameplay
//! client. Requests are accepted only on localhost and are handed to the
//! existing bounded gameplay channel owned by [`super::gameplay::GameplayOwner`].

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use purgatory_common::{
    ContentId, DEFAULT_DEV_ADMIN_PORT, DEV_ADMIN_MAX_LINE_BYTES, DEV_ADMIN_PORT_ENV,
    DevAdminContentEntry, DevAdminPlayer, DevAdminRequest, DevAdminResponse, DevAdminSnapshot,
};
use purgatory_content::{
    DialogueAction, DialogueCondition, LoadMode, default_content_root, load_registry,
};
use purgatory_protocol::ConnectionId;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use super::gameplay::{DevNarrativeCommand, GameplayTx};
use super::session::{SessionTable, lock_sessions};

#[derive(Clone)]
struct AdminCatalog {
    npcs: Arc<Vec<DevAdminContentEntry>>,
    items: Arc<Vec<DevAdminContentEntry>>,
    facts: Arc<Vec<String>>,
}

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

    let mut items = registry
        .iter_items()
        .filter_map(|item| {
            let content_id = u32::try_from(item.content_id.token()).ok()?;
            Some(DevAdminContentEntry {
                content_id,
                authored_id: item.authored_id.clone(),
            })
        })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| a.authored_id.cmp(&b.authored_id));

    let mut facts = BTreeSet::new();
    for dialogue in registry.iter_npc_dialogues() {
        for beat in &dialogue.beats {
            for condition in &beat.conditions {
                if let DialogueCondition::Fact { fact, .. } = condition {
                    facts.insert(fact.clone());
                }
            }
            for choice in &beat.choices {
                for action in &choice.actions {
                    if let DialogueAction::SetFact { fact, .. } = action {
                        facts.insert(fact.clone());
                    }
                }
            }
        }
    }

    let catalog = AdminCatalog {
        npcs: Arc::new(npcs),
        items: Arc::new(items),
        facts: Arc::new(facts.into_iter().collect()),
    };

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
            let catalog = catalog.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_connection(stream, gameplay, sessions, catalog).await {
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
    catalog: AdminCatalog,
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
            Ok(request) => dispatch(request, &gameplay, &sessions, &catalog).await,
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
    catalog: &AdminCatalog,
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
                    npcs: catalog.npcs.as_ref().clone(),
                    items: catalog.items.as_ref().clone(),
                    facts: catalog.facts.as_ref().clone(),
                },
            }
        }
        DevAdminRequest::SpawnNpc {
            connection_id,
            npc_content_id,
        } => {
            if !session_exists(sessions, connection_id) {
                return inactive(connection_id);
            }
            if !catalog
                .npcs
                .iter()
                .any(|npc| npc.content_id == npc_content_id)
            {
                return DevAdminResponse::command_err(format!(
                    "ContentId {npc_content_id} is not an authored NPC"
                ));
            }
            accepted(
                gameplay
                    .send_dev_spawn_npc(
                        ConnectionId::from_raw(connection_id),
                        ContentId::from_raw(npc_content_id),
                    )
                    .await,
                format!("Spawn NPC {npc_content_id} near connection {connection_id}"),
            )
        }
        DevAdminRequest::SpawnItem {
            connection_id,
            item_content_id,
            quantity,
        } => {
            if !session_exists(sessions, connection_id) {
                return inactive(connection_id);
            }
            if quantity == 0 {
                return DevAdminResponse::command_err("item quantity must be greater than zero");
            }
            if !catalog
                .items
                .iter()
                .any(|item| item.content_id == item_content_id)
            {
                return DevAdminResponse::command_err(format!(
                    "ContentId {item_content_id} is not an authored item"
                ));
            }
            match gameplay
                .send_dev_spawn_item(
                    ConnectionId::from_raw(connection_id),
                    ContentId::from_raw(item_content_id),
                    quantity,
                )
                .await
            {
                Ok(()) => {
                    let message = format!(
                        "Spawn Item {item_content_id} x{quantity} near connection {connection_id}"
                    );
                    println!("DEV_ADMIN {message}");
                    DevAdminResponse::command_ok(message)
                }
                Err(message) => DevAdminResponse::command_err(message),
            }
        }
        DevAdminRequest::SetNarrativeFact {
            connection_id,
            fact_id,
            value,
        } => {
            if !session_exists(sessions, connection_id) {
                return inactive(connection_id);
            }
            narrative_result(
                gameplay
                    .send_dev_narrative(
                        ConnectionId::from_raw(connection_id),
                        DevNarrativeCommand::SetFact {
                            fact_id: fact_id.clone(),
                            value,
                        },
                    )
                    .await,
                format!("Set fact {fact_id}={value} for connection {connection_id}"),
            )
        }
        DevAdminRequest::ClearNarrativeFact {
            connection_id,
            fact_id,
        } => {
            if !session_exists(sessions, connection_id) {
                return inactive(connection_id);
            }
            narrative_result(
                gameplay
                    .send_dev_narrative(
                        ConnectionId::from_raw(connection_id),
                        DevNarrativeCommand::ClearFact {
                            fact_id: fact_id.clone(),
                        },
                    )
                    .await,
                format!("Clear fact {fact_id} for connection {connection_id}"),
            )
        }
        DevAdminRequest::MarkNpcMet {
            connection_id,
            npc_authored_id,
        } => {
            if !session_exists(sessions, connection_id) {
                return inactive(connection_id);
            }
            narrative_result(
                gameplay
                    .send_dev_narrative(
                        ConnectionId::from_raw(connection_id),
                        DevNarrativeCommand::MarkNpcMet {
                            npc_authored_id: npc_authored_id.clone(),
                        },
                    )
                    .await,
                format!("Mark NPC met {npc_authored_id} for connection {connection_id}"),
            )
        }
        DevAdminRequest::ResetNarrative { connection_id } => {
            if !session_exists(sessions, connection_id) {
                return inactive(connection_id);
            }
            narrative_result(
                gameplay
                    .send_dev_narrative(
                        ConnectionId::from_raw(connection_id),
                        DevNarrativeCommand::Reset,
                    )
                    .await,
                format!("Reset narrative state for connection {connection_id}"),
            )
        }
        DevAdminRequest::ResetPlayer { connection_id } => {
            if !session_exists(sessions, connection_id) {
                return inactive(connection_id);
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
                return inactive(connection_id);
            }
            accepted(
                gameplay
                    .send_dev_set_channel(ConnectionId::from_raw(connection_id), channel)
                    .await,
                format!("Set connection {connection_id} channel to {channel}"),
            )
        }
    }
}

fn narrative_result(result: Result<(), String>, message: String) -> DevAdminResponse {
    match result {
        Ok(()) => {
            println!("DEV_ADMIN {message}");
            DevAdminResponse::command_ok(message)
        }
        Err(error) => DevAdminResponse::command_err(error),
    }
}

fn inactive(connection_id: u64) -> DevAdminResponse {
    DevAdminResponse::command_err(format!("connection {connection_id} is not active"))
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
