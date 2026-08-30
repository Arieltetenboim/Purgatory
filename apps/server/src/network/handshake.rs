//! Hello / Welcome handshake. All client bytes are untrusted.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use quinn::{Connection, RecvStream, SendStream};

use purgatory_common::DevLogin;
use purgatory_protocol::{
    ClientControl, DisconnectReason, DisconnectReasonCode, Hello, PROTOCOL_VERSION, ServerControl,
    ServerDatagram, Welcome, decode_client_control, decode_client_datagram, encode_frame,
    encode_gameplay_frame, encode_server_control, encode_server_datagram, peek_frame_len,
    validate_hello,
};
use purgatory_simulation::TICK_RATE_HZ;
use tokio::time::timeout;

use super::abuse::{
    ConnectionAbuse, ControlRateLimit, NetworkAbuseConfig, RateDecision, sanitize_log_text,
};
use super::gameplay::{EnterError, GameplayTx};
use super::persist::PersistenceHandle;
use super::replication::ReplicationPipe;
use super::session::{ConnectionIdAllocator, ConnectionSession, SessionLease, SessionTable};
use super::stats::ServerNetStats;

const SERVER_LABEL: &str = "purgatory-server-dev";

enum ControlReadError {
    Closed,
    Oversized,
    InvalidLength,
    Decode,
}

pub(crate) async fn handle_incoming(
    incoming: quinn::Incoming,
    sessions: Arc<Mutex<SessionTable>>,
    ids: Arc<ConnectionIdAllocator>,
    abuse: NetworkAbuseConfig,
    stats: Arc<ServerNetStats>,
    gameplay: Option<GameplayTx>,
    persist: Option<PersistenceHandle>,
) {
    stats.enter_handshake();
    let connection = match incoming.await {
        Ok(conn) => conn,
        Err(_) => {
            stats.leave_handshake();
            return;
        }
    };
    let remote = connection.remote_address();

    let handshake = timeout(
        abuse.handshake_timeout,
        handshake_streams(&connection, &stats),
    )
    .await;
    let (mut send, recv, hello) = match handshake {
        Ok(Ok(parts)) => parts,
        Ok(Err(reason)) => {
            stats.leave_handshake();
            if reason.code == DisconnectReasonCode::Malformed && reason.detail == "oversize" {
                stats
                    .rejected_oversized
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            stats.note_reject(reason.code);
            println!(
                "handshake rejected {} reason={}",
                sanitize_log_text(&remote.to_string()),
                reason.code.as_str()
            );
            let _ = send_disconnect_best_effort(&connection, &reason).await;
            connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
            return;
        }
        Err(_) => {
            stats.leave_handshake();
            stats.note_reject(DisconnectReasonCode::HandshakeTimeout);
            println!(
                "handshake rejected {} reason=handshake timeout",
                sanitize_log_text(&remote.to_string())
            );
            let reason = DisconnectReason::new(DisconnectReasonCode::HandshakeTimeout, "no hello");
            let _ = send_disconnect_best_effort(&connection, &reason).await;
            connection.close(
                DisconnectReasonCode::HandshakeTimeout.as_u8().into(),
                b"timeout",
            );
            return;
        }
    };

    if let Err(reason) = validate_hello(&hello) {
        stats.leave_handshake();
        stats.note_reject(reason.code);
        println!(
            "handshake rejected {} reason={}",
            sanitize_log_text(&remote.to_string()),
            reason.code.as_str()
        );
        let _ = write_server_control(&mut send, &ServerControl::Disconnect(reason.clone())).await;
        connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
        return;
    }

    let connection_id = ids.allocate();
    let login = match DevLogin::parse(&hello.dev_login) {
        Ok(login) => login,
        Err(_) => {
            stats.leave_handshake();
            let reason = DisconnectReason::new(DisconnectReasonCode::Malformed, "dev_login");
            stats.note_reject(reason.code);
            let _ =
                write_server_control(&mut send, &ServerControl::Disconnect(reason.clone())).await;
            connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
            return;
        }
    };

    let mut snap_rx = None;
    let mut occupancy = None;
    match (persist.as_ref(), gameplay.as_ref()) {
        (Some(persist), Some(tx)) => {
            let character = match persist.resolve(login).await {
                Ok(character) => character,
                Err(err) => {
                    eprintln!("PURGATORY persist resolve failed: {err}");
                    stats.leave_handshake();
                    let reason = DisconnectReason::new(DisconnectReasonCode::Malformed, "identity");
                    stats.note_reject(reason.code);
                    let _ =
                        write_server_control(&mut send, &ServerControl::Disconnect(reason.clone()))
                            .await;
                    connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
                    return;
                }
            };
            let (pipe, wake_rx) = ReplicationPipe::new();
            let (interact_tx, interact_rx) = tokio::sync::mpsc::channel(16);
            match tx
                .enter(
                    connection_id,
                    character,
                    Some(pipe.clone()),
                    Some(interact_tx),
                )
                .await
            {
                Ok(Ok(())) => {
                    occupancy = Some(OccupancyLease::new(tx.clone(), connection_id));
                    snap_rx = Some((pipe, wake_rx, interact_rx));
                }
                Ok(Err(EnterError::Occupied)) => {
                    stats.leave_handshake();
                    let reason =
                        DisconnectReason::new(DisconnectReasonCode::AlreadyConnected, "character");
                    stats.note_reject(reason.code);
                    let _ =
                        write_server_control(&mut send, &ServerControl::Disconnect(reason.clone()))
                            .await;
                    connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
                    return;
                }
                Ok(Err(_)) | Err(()) => {
                    stats.leave_handshake();
                    let reason = DisconnectReason::new(DisconnectReasonCode::Malformed, "enter");
                    stats.note_reject(reason.code);
                    let _ =
                        write_server_control(&mut send, &ServerControl::Disconnect(reason.clone()))
                            .await;
                    connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
                    return;
                }
            }
        }
        (None, Some(tx)) => {
            let (pipe, wake_rx) = ReplicationPipe::new();
            let (interact_tx, interact_rx) = tokio::sync::mpsc::channel(16);
            if !tx.attach_with_snapshots(connection_id, Some(pipe.clone()), Some(interact_tx)) {
                stats
                    .lifecycle_handoff_dropped
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            } else {
                occupancy = Some(OccupancyLease::new(tx.clone(), connection_id));
            }
            snap_rx = Some((pipe, wake_rx, interact_rx));
        }
        _ => {}
    }

    let welcome = Welcome {
        protocol_version: PROTOCOL_VERSION,
        connection_id,
        server_tick_rate: TICK_RATE_HZ,
        server_label: SERVER_LABEL.to_string(),
    };
    if write_server_control(&mut send, &ServerControl::Welcome(welcome))
        .await
        .is_err()
    {
        stats.leave_handshake();
        stats
            .total_rejected
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        println!(
            "handshake rejected {} reason=welcome write failed",
            sanitize_log_text(&remote.to_string())
        );
        connection.close(0u32.into(), b"welcome");
        if let Some(tx) = &gameplay {
            let _ = tx.send_detach(connection_id).await;
        }
        return;
    }

    stats.leave_handshake();
    stats
        .total_accepted
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let session = ConnectionSession {
        connection_id,
        protocol_version: PROTOCOL_VERSION,
        connected_since: Instant::now(),
        remote,
    };
    let lease = SessionLease::insert(sessions, session.clone());
    stats
        .session_created
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    println!(
        "handshake accepted connection_id={} protocol={}",
        session.connection_id, session.protocol_version
    );

    let (replication, interact_rx) = match snap_rx {
        Some((pipe, wake, i)) => (Some((pipe, wake)), Some(i)),
        None => (None, None),
    };

    serve_connection(LiveSession {
        connection,
        send,
        recv,
        session,
        lease,
        abuse_cfg: abuse,
        stats,
        gameplay,
        replication,
        interact_rx,
        occupancy,
    })
    .await;
}

/// Releases character occupancy if the connection task is dropped before
/// the normal `send_detach` teardown (panic, abort, or skipped await).
struct OccupancyLease {
    tx: GameplayTx,
    id: purgatory_protocol::ConnectionId,
}

impl OccupancyLease {
    fn new(tx: GameplayTx, id: purgatory_protocol::ConnectionId) -> Self {
        Self { tx, id }
    }
}

impl Drop for OccupancyLease {
    fn drop(&mut self) {
        if self.tx.try_detach(self.id) {
            return;
        }
        let tx = self.tx.clone();
        let id = self.id;
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = tx.send_detach(id).await;
            });
        }
    }
}

struct LiveSession {
    connection: Connection,
    send: SendStream,
    recv: RecvStream,
    session: ConnectionSession,
    lease: SessionLease,
    abuse_cfg: NetworkAbuseConfig,
    stats: Arc<ServerNetStats>,
    gameplay: Option<GameplayTx>,
    replication: Option<(ReplicationPipe, tokio::sync::watch::Receiver<u64>)>,
    interact_rx: Option<tokio::sync::mpsc::Receiver<ServerControl>>,
    occupancy: Option<OccupancyLease>,
}

async fn handshake_streams(
    connection: &Connection,
    stats: &ServerNetStats,
) -> Result<(SendStream, RecvStream, Hello), DisconnectReason> {
    let (send, mut recv) = connection
        .accept_bi()
        .await
        .map_err(|_| DisconnectReason::new(DisconnectReasonCode::Malformed, "no control stream"))?;
    let control = match read_client_control(&mut recv, stats).await {
        Ok(msg) => msg,
        Err(ControlReadError::Oversized) => {
            return Err(DisconnectReason::new(
                DisconnectReasonCode::Malformed,
                "oversize",
            ));
        }
        Err(_) => {
            return Err(DisconnectReason::new(
                DisconnectReasonCode::Malformed,
                "handshake",
            ));
        }
    };
    let ClientControl::Hello(hello) = control else {
        return Err(DisconnectReason::new(
            DisconnectReasonCode::UnexpectedMessage,
            "handshake",
        ));
    };
    Ok((send, recv, hello))
}

async fn serve_connection(live: LiveSession) {
    let LiveSession {
        connection,
        mut send,
        mut recv,
        session,
        lease,
        abuse_cfg,
        stats,
        gameplay,
        mut replication,
        mut interact_rx,
        occupancy,
    } = live;
    let id = session.connection_id;
    let remote = session.remote;
    let mut transport_loss = false;
    let mut abuse = ConnectionAbuse::default();
    let mut rate = ControlRateLimit::new(Instant::now());
    let mut input_rate = ControlRateLimit::new(Instant::now());
    let want_uni = replication.is_some();
    let mut snap_send = None;
    let mut uni_opened = !want_uni;
    loop {
        tokio::select! {
            biased;
            datagram = connection.read_datagram() => {
                match datagram {
                    Ok(bytes) => {
                        if bytes.len() > abuse_cfg.max_datagram_bytes {
                            if abuse.note_invalid_datagram(abuse_cfg.invalid_datagram_budget) {
                                stats
                                    .malformed
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                connection.close(
                                    DisconnectReasonCode::Malformed.as_u8().into(),
                                    b"protocol",
                                );
                                break;
                            }
                            continue;
                        }
                        match decode_client_datagram(&bytes) {
                            Ok(nonce) => {
                                if let Ok(payload) =
                                    encode_server_datagram(ServerDatagram::Pong { nonce })
                                {
                                    let _ = connection.send_datagram(payload.into());
                                }
                            }
                            Err(_) => {
                                if abuse.note_invalid_datagram(abuse_cfg.invalid_datagram_budget) {
                                    stats
                                        .malformed
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    connection.close(
                                        DisconnectReasonCode::Malformed.as_u8().into(),
                                        b"protocol",
                                    );
                                    break;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        transport_loss = !matches!(
                            err,
                            quinn::ConnectionError::LocallyClosed
                                | quinn::ConnectionError::ApplicationClosed(_)
                        );
                        if matches!(err, quinn::ConnectionError::TimedOut) {
                            transport_loss = true;
                        }
                        break;
                    }
                }
            }
            s = connection.open_uni(), if !uni_opened => {
                // One long-lived replication uni. Opened in this select so a
                // dropped peer is observed via read_datagram instead of hanging
                // occupancy on an exclusive open_uni await.
                snap_send = s.ok();
                uni_opened = true;
            }
            control = read_client_control(&mut recv, &stats) => {
                match control {
                    Ok(ClientControl::Input(command)) => match input_rate.note_input(Instant::now(), abuse_cfg) {
                        RateDecision::Disconnect => {
                            stats
                                .input_rate_limited
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            connection.close(
                                DisconnectReasonCode::Malformed.as_u8().into(),
                                b"protocol",
                            );
                            break;
                        }
                        RateDecision::Drop => {
                            stats
                                .input_rate_limited
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        RateDecision::Allow => {
                            stats
                                .input_received
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if let Some(tx) = &gameplay
                                && !tx.send_input(id, command).await
                            {
                                // Receiver gone (sim shutdown) — not a capacity drop.
                                break;
                            }
                        }
                    },
                    Ok(ClientControl::HeldCancel) => match input_rate.note_input(Instant::now(), abuse_cfg) {
                        RateDecision::Disconnect => {
                            stats
                                .input_rate_limited
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            connection.close(
                                DisconnectReasonCode::Malformed.as_u8().into(),
                                b"protocol",
                            );
                            break;
                        }
                        RateDecision::Drop => {
                            stats
                                .input_rate_limited
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        RateDecision::Allow => {
                            stats
                                .input_received
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if let Some(tx) = &gameplay
                                && !tx.send_held_cancel(id).await
                            {
                                break;
                            }
                        }
                    },
                    Ok(ClientControl::InteractOpen(open)) => {
                        println!(
                            "6B_INTERACT recv InteractOpen connection={id} target={}",
                            open.target
                        );
                        match rate.note(Instant::now(), abuse_cfg) {
                            RateDecision::Disconnect => {
                                stats
                                    .rate_limited
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                connection.close(
                                    DisconnectReasonCode::Malformed.as_u8().into(),
                                    b"protocol",
                                );
                                break;
                            }
                            RateDecision::Drop => {
                                stats
                                    .rate_limited
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                            RateDecision::Allow => {
                                if let Some(tx) = &gameplay
                                    && !tx.send_interact_open(id, open.target).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::InteractClose(close)) => {
                        println!(
                            "6B_INTERACT recv InteractClose connection={id} session={}",
                            close.session_id
                        );
                        match rate.note(Instant::now(), abuse_cfg) {
                            RateDecision::Disconnect => {
                                stats
                                    .rate_limited
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                connection.close(
                                    DisconnectReasonCode::Malformed.as_u8().into(),
                                    b"protocol",
                                );
                                break;
                            }
                            RateDecision::Drop => {
                                stats
                                    .rate_limited
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                            RateDecision::Allow => {
                                if let Some(tx) = &gameplay
                                    && !tx.send_interact_close(id, close.session_id).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::PortalActivate(activate)) => {
                        println!(
                            "6C_PORTAL recv PortalActivate connection={id} target={}",
                            activate.target
                        );
                        match rate.note(Instant::now(), abuse_cfg) {
                            RateDecision::Disconnect => {
                                stats
                                    .rate_limited
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                connection.close(
                                    DisconnectReasonCode::Malformed.as_u8().into(),
                                    b"protocol",
                                );
                                break;
                            }
                            RateDecision::Drop => {
                                stats
                                    .rate_limited
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                            RateDecision::Allow => {
                                if let Some(tx) = &gameplay
                                    && !tx.send_portal_activate(id, activate.target).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::DevSetChannel(req)) => {
                        println!(
                            "6D_CHANNEL recv DevSetChannel connection={id} channel={}",
                            req.channel
                        );
                        match rate.note(Instant::now(), abuse_cfg) {
                            RateDecision::Disconnect => {
                                stats
                                    .rate_limited
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                connection.close(
                                    DisconnectReasonCode::Malformed.as_u8().into(),
                                    b"protocol",
                                );
                                break;
                            }
                            RateDecision::Drop => {
                                stats
                                    .rate_limited
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                            RateDecision::Allow => {
                                if let Some(tx) = &gameplay
                                    && !tx.send_dev_set_channel(id, req.channel).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(msg) => match rate.note(Instant::now(), abuse_cfg) {
                        RateDecision::Disconnect => {
                            stats
                                .rate_limited
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            connection.close(
                                DisconnectReasonCode::Malformed.as_u8().into(),
                                b"protocol",
                            );
                            break;
                        }
                        RateDecision::Drop => {
                            stats
                                .rate_limited
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        RateDecision::Allow => {
                            if let ClientControl::Hello(_) = msg {
                                stats
                                    .unexpected
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                let reason = DisconnectReason::new(
                                    DisconnectReasonCode::UnexpectedMessage,
                                    "protocol",
                                );
                                let _ = write_server_control(
                                    &mut send,
                                    &ServerControl::Disconnect(reason.clone()),
                                )
                                .await;
                                connection.close(
                                    reason.code.as_u8().into(),
                                    reason.code.as_str().as_bytes(),
                                );
                                break;
                            }
                        }
                    },
                    Err(ControlReadError::Oversized) => {
                        stats
                            .rejected_oversized
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        connection.close(
                            DisconnectReasonCode::Malformed.as_u8().into(),
                            b"protocol",
                        );
                        break;
                    }
                    Err(ControlReadError::InvalidLength) => {
                        // A zero-length frame is a structural protocol
                        // violation, not a lost transport.
                        stats
                            .malformed
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        connection.close(
                            DisconnectReasonCode::Malformed.as_u8().into(),
                            b"protocol",
                        );
                        break;
                    }
                    Err(ControlReadError::Closed) => {
                        // The peer stopped talking. Trust the connection's own
                        // close reason instead of assuming loss, so a graceful
                        // client close is not reported as transport loss.
                        transport_loss = !matches!(
                            connection.close_reason(),
                            Some(
                                quinn::ConnectionError::LocallyClosed
                                    | quinn::ConnectionError::ApplicationClosed(_)
                            )
                        );
                        break;
                    }
                    Err(ControlReadError::Decode) => match rate.note(Instant::now(), abuse_cfg) {
                        RateDecision::Disconnect => {
                            stats
                                .rate_limited
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            connection.close(
                                DisconnectReasonCode::Malformed.as_u8().into(),
                                b"protocol",
                            );
                            break;
                        }
                        RateDecision::Drop => {
                            stats
                                .rate_limited
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        RateDecision::Allow => {
                            stats
                                .input_invalid
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if abuse.note_malformed(abuse_cfg.malformed_control_budget) {
                                stats
                                    .malformed
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                connection.close(
                                    DisconnectReasonCode::Malformed.as_u8().into(),
                                    b"protocol",
                                );
                                break;
                            }
                        }
                    },
                }
            }
            changed = async {
                match replication.as_mut() {
                    Some((_, rx)) => rx.changed().await.ok(),
                    None => std::future::pending().await,
                }
            } => {
                if changed.is_none() {
                    break;
                }
                let Some((pipe, _)) = replication.as_ref() else {
                    continue;
                };
                let Some(send) = snap_send.as_mut() else {
                    break;
                };
                let mut write_failed = false;
                while let Some(frame) = pipe.pop() {
                    if write_replication_payload(send, &frame.payload, &stats).await {
                        stats
                            .snapshots_sent
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    } else {
                        stats
                            .snapshot_send_failed
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        write_failed = true;
                        break;
                    }
                }
                if write_failed {
                    break;
                }
            }
            maybe_interact = async {
                match interact_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                match maybe_interact {
                    Some(event) => {
                        if write_server_control(&mut send, &event).await.is_err() {
                            break;
                        }
                    }
                    None => interact_rx = None,
                }
            }
        }
    }

    if let Some(tx) = &gameplay
        && !tx.send_detach(id).await
    {
        stats
            .lifecycle_handoff_dropped
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    lease.remove_once();
    stats
        .session_destroyed
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if transport_loss {
        stats
            .transport_loss
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    } else {
        stats
            .clean_disconnect
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    println!(
        "disconnect connection_id={id} peer={} lived_ms={}",
        sanitize_log_text(&remote.to_string()),
        session.connected_since.elapsed().as_millis()
    );
    drop(occupancy);
}

async fn read_client_control(
    recv: &mut RecvStream,
    stats: &ServerNetStats,
) -> Result<ClientControl, ControlReadError> {
    let mut prefix = [0u8; 4];
    recv.read_exact(&mut prefix)
        .await
        .map_err(|_| ControlReadError::Closed)?;
    let len = match peek_frame_len(&prefix) {
        Ok(len) => len,
        Err(purgatory_protocol::FrameError::InvalidLength(raw))
            if raw > purgatory_protocol::MAX_CONTROL_MESSAGE_BYTES =>
        {
            return Err(ControlReadError::Oversized);
        }
        Err(_) => return Err(ControlReadError::InvalidLength),
    };
    let len_usize = usize::try_from(len).map_err(|_| ControlReadError::Oversized)?;
    let mut payload = vec![0u8; len_usize];
    recv.read_exact(&mut payload)
        .await
        .map_err(|_| ControlReadError::Closed)?;
    stats.bytes_in.fetch_add(
        4u64.saturating_add(len_usize as u64),
        std::sync::atomic::Ordering::Relaxed,
    );
    decode_client_control(&payload).map_err(|_| ControlReadError::Decode)
}

async fn write_server_control(
    send: &mut SendStream,
    msg: &ServerControl,
) -> Result<(), DisconnectReason> {
    let payload = encode_server_control(msg)
        .map_err(|_| DisconnectReason::new(DisconnectReasonCode::Malformed, "encode"))?;
    let frame = encode_frame(&payload)
        .map_err(|_| DisconnectReason::new(DisconnectReasonCode::Malformed, "frame"))?;
    send.write_all(&frame)
        .await
        .map_err(|_| DisconnectReason::new(DisconnectReasonCode::Malformed, "write"))?;
    Ok(())
}

async fn write_replication_payload(
    send: &mut SendStream,
    payload: &[u8],
    stats: &ServerNetStats,
) -> bool {
    let encode_start = std::time::Instant::now();
    let encode_us = u64::try_from(encode_start.elapsed().as_micros()).unwrap_or(u64::MAX);
    stats
        .snapshot_encode_time_max_micros
        .fetch_max(encode_us, std::sync::atomic::Ordering::Relaxed);
    stats
        .snapshot_size_max_bytes
        .fetch_max(payload.len() as u64, std::sync::atomic::Ordering::Relaxed);
    let Ok(frame) = encode_gameplay_frame(payload) else {
        stats
            .snapshot_encode_failed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return false;
    };
    let n = frame.len() as u64;
    if send.write_all(&frame).await.is_ok() {
        stats
            .bytes_out
            .fetch_add(n, std::sync::atomic::Ordering::Relaxed);
        true
    } else {
        false
    }
}

async fn send_disconnect_best_effort(connection: &Connection, reason: &DisconnectReason) {
    if let Ok((mut send, _)) = connection.open_bi().await
        && write_server_control(&mut send, &ServerControl::Disconnect(reason.clone()))
            .await
            .is_ok()
    {
        let _ = send.finish();
    }
}
