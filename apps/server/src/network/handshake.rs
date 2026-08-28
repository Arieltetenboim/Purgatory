//! Hello / Welcome handshake. All client bytes are untrusted.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use quinn::{Connection, RecvStream, SendStream};

use purgatory_protocol::{
    ClientControl, DisconnectReason, DisconnectReasonCode, Hello, PROTOCOL_VERSION, ServerControl,
    ServerDatagram, Welcome, WorldSnapshot, decode_client_control, decode_client_datagram,
    encode_frame, encode_gameplay_frame, encode_server_control, encode_server_datagram,
    encode_world_snapshot, peek_frame_len, validate_hello,
};
use purgatory_simulation::TICK_RATE_HZ;
use tokio::time::timeout;

use super::abuse::{
    ConnectionAbuse, ControlRateLimit, NetworkAbuseConfig, RateDecision, sanitize_log_text,
};
use super::gameplay::GameplayTx;
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

    let handshake = timeout(abuse.handshake_timeout, handshake_streams(&connection)).await;
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
    let snap_rx = if let Some(tx) = &gameplay {
        let (snap_tx, snap_rx) = tokio::sync::watch::channel(None);
        tx.attach_with_snapshots(session.connection_id, Some(snap_tx));
        Some(snap_rx)
    } else {
        None
    };
    println!(
        "handshake accepted connection_id={} protocol={}",
        session.connection_id, session.protocol_version
    );

    serve_connection(LiveSession {
        connection,
        send,
        recv,
        session,
        lease,
        abuse_cfg: abuse,
        stats,
        gameplay,
        snap_rx,
    })
    .await;
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
    snap_rx: Option<tokio::sync::watch::Receiver<Option<WorldSnapshot>>>,
}

async fn handshake_streams(
    connection: &Connection,
) -> Result<(SendStream, RecvStream, Hello), DisconnectReason> {
    let (send, mut recv) = connection
        .accept_bi()
        .await
        .map_err(|_| DisconnectReason::new(DisconnectReasonCode::Malformed, "no control stream"))?;
    let control = match read_client_control(&mut recv).await {
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
        mut snap_rx,
    } = live;
    let id = session.connection_id;
    let remote = session.remote;
    let mut transport_loss = false;
    let mut abuse = ConnectionAbuse::default();
    let mut rate = ControlRateLimit::new(Instant::now());
    let mut input_rate = ControlRateLimit::new(Instant::now());
    let mut snap_send = if snap_rx.is_some() {
        connection.open_uni().await.ok()
    } else {
        None
    };
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
            control = read_client_control(&mut recv) => {
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
                            match &gameplay {
                                Some(tx) if tx.try_input(id, command) => {}
                                Some(_) => {
                                    stats
                                        .input_handoff_dropped
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                }
                                None => {}
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
                            match &gameplay {
                                Some(tx) if tx.try_held_cancel(id) => {}
                                Some(_) => {
                                    stats
                                        .input_handoff_dropped
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                }
                                None => {}
                            }
                        }
                    },
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
                        RateDecision::Allow => match msg {
                            ClientControl::HeldCancel => {}
                            ClientControl::Hello(_) => {
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
                            ClientControl::Input(_) => {}
                        },
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
                match snap_rx.as_mut() {
                    Some(rx) => rx.changed().await.ok(),
                    None => std::future::pending().await,
                }
            } => {
                if changed.is_none() {
                    snap_rx = None;
                    snap_send = None;
                    continue;
                }
                let Some(rx) = snap_rx.as_mut() else {
                    continue;
                };
                let snap = rx.borrow_and_update().clone();
                let Some(snap) = snap else {
                    continue;
                };
                let Some(send) = snap_send.as_mut() else {
                    continue;
                };
                if write_world_snapshot(send, &snap).await {
                    stats
                        .snapshots_sent
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                } else {
                    stats
                        .snapshot_send_failed
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    snap_send = None;
                }
            }
        }
    }

    if let Some(tx) = &gameplay {
        tx.detach(id);
    }
    lease.remove_once();
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
}

async fn read_client_control(recv: &mut RecvStream) -> Result<ClientControl, ControlReadError> {
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

async fn write_world_snapshot(send: &mut SendStream, snap: &WorldSnapshot) -> bool {
    let Ok(payload) = encode_world_snapshot(snap) else {
        return false;
    };
    let Ok(frame) = encode_gameplay_frame(&payload) else {
        return false;
    };
    send.write_all(&frame).await.is_ok()
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
