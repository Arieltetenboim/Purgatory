//! Hello / pre-game session handshake. All client bytes are untrusted.

use std::sync::Arc;
use std::time::Instant;

use quinn::{Connection, RecvStream, SendStream};

use purgatory_common::DevLogin;
use purgatory_protocol::{
    ClientControl, DisconnectReason, DisconnectReasonCode, FrontendSessionReady, Hello,
    PROTOCOL_VERSION, ServerControl, ServerDatagram, decode_client_control, decode_client_datagram,
    encode_frame, encode_gameplay_frame, encode_server_control, encode_server_datagram,
    peek_frame_len, validate_hello,
};
use tokio::time::timeout;

use super::abuse::{
    ConnectionAbuse, ControlRateLimit, NetworkAbuseConfig, RateDecision, sanitize_log_text,
};
use super::connection_lifecycle::ConnectionLifecycleBook;
use super::gameplay::EnterError;
use super::gameplay::GameplayTx;
use super::network_pressure::NetworkPressureBook;
use super::replication::ReplicationPipe;
use super::session::{ConnectionSession, SessionLease};
use super::stats::ServerNetStats;

fn us(d: std::time::Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

enum ControlReadError {
    Closed,
    Oversized,
    InvalidLength,
    Decode,
}

pub(crate) async fn handle_incoming(incoming: quinn::Incoming, ctx: super::IncomingDispatch) {
    let super::IncomingDispatch {
        sessions,
        ids,
        abuse,
        stats,
        gameplay,
        persist,
        lifecycle,
        pressure,
        channel_live,
        ..
    } = ctx;
    stats.enter_handshake();
    let connection = match incoming.await {
        Ok(conn) => conn,
        Err(_) => {
            lifecycle.note_transport_accept_fail();
            stats.leave_handshake();
            return;
        }
    };
    let accept_at = Instant::now();
    lifecycle.note_transport_accept_ok();
    let remote = connection.remote_address();

    let handshake = timeout(
        abuse.handshake_timeout,
        handshake_streams(&connection, &stats),
    )
    .await;
    let (mut send, recv, hello) = match handshake {
        Ok(Ok(parts)) => parts,
        Ok(Err(reason)) => {
            lifecycle.note_hello_fail();
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
            lifecycle.note_hello_fail();
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

    let hello_at = Instant::now();
    lifecycle.note_hello_ok(us(accept_at.elapsed()));

    if let Err(reason) = validate_hello(&hello) {
        lifecycle.note_hello_fail();
        stats.leave_handshake();
        stats.note_reject(reason.code);
        println!(
            "handshake rejected {} reason={} detail={}",
            sanitize_log_text(&remote.to_string()),
            reason.code.as_str(),
            sanitize_log_text(&reason.detail)
        );
        let _ = write_server_control(&mut send, &ServerControl::Disconnect(reason.clone())).await;
        connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
        return;
    }

    let connection_id = ids.allocate();
    let login = match DevLogin::parse(&hello.dev_login) {
        Ok(login) => login,
        Err(_) => {
            lifecycle.note_hello_fail();
            stats.leave_handshake();
            let reason = DisconnectReason::new(DisconnectReasonCode::Malformed, "dev_login");
            stats.note_reject(reason.code);
            let _ =
                write_server_control(&mut send, &ServerControl::Disconnect(reason.clone())).await;
            connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
            return;
        }
    };
    if let Some(worker) = persist.as_ref() {
        match worker.user_registered(login.clone()).await {
            Ok(true) => {}
            Ok(false) => {
                lifecycle.note_hello_fail();
                stats.leave_handshake();
                let reason = DisconnectReason::new(DisconnectReasonCode::UnknownUser, "dev_login");
                stats.note_reject(reason.code);
                println!(
                    "handshake rejected {} reason={} detail={}",
                    sanitize_log_text(&remote.to_string()),
                    reason.code.as_str(),
                    sanitize_log_text(&reason.detail)
                );
                let _ = write_server_control(&mut send, &ServerControl::Disconnect(reason.clone()))
                    .await;
                connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
                return;
            }
            Err(err) => {
                eprintln!("PURGATORY user lookup failed: {err}");
                lifecycle.note_hello_fail();
                stats.leave_handshake();
                connection.close(
                    DisconnectReasonCode::ServerShutdown.as_u8().into(),
                    b"storage unavailable",
                );
                return;
            }
        }
    }

    // Historical gameplay integration fixtures opt in only in test binaries.
    // Shipping builds have no legacy entry route, regardless of client_build.
    #[cfg(test)]
    let legacy = hello.client_build.starts_with("legacy-test:");
    #[cfg(test)]
    let (replication, interact_rx, occupancy) = {
        let mut snap_rx = None;
        let mut occupancy = None;
        match (
            persist.as_ref().filter(|_| legacy),
            gameplay.as_ref().filter(|_| legacy),
        ) {
            (Some(persist), Some(tx)) => {
                let character = match persist.resolve(login.clone()).await {
                    Ok(character) => character,
                    Err(err) => {
                        eprintln!("PURGATORY persist resolve failed: {err}");
                        lifecycle.note_enter_fail();
                        stats.leave_handshake();
                        let reason =
                            DisconnectReason::new(DisconnectReasonCode::Malformed, "identity");
                        stats.note_reject(reason.code);
                        let _ = write_server_control(
                            &mut send,
                            &ServerControl::Disconnect(reason.clone()),
                        )
                        .await;
                        connection
                            .close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
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
                        lifecycle.note_enter_fail();
                        stats.leave_handshake();
                        let reason = DisconnectReason::new(
                            DisconnectReasonCode::AlreadyConnected,
                            "character",
                        );
                        stats.note_reject(reason.code);
                        let _ = write_server_control(
                            &mut send,
                            &ServerControl::Disconnect(reason.clone()),
                        )
                        .await;
                        connection
                            .close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
                        return;
                    }
                    Ok(Err(_)) | Err(()) => {
                        lifecycle.note_enter_fail();
                        stats.leave_handshake();
                        let reason =
                            DisconnectReason::new(DisconnectReasonCode::Malformed, "enter");
                        stats.note_reject(reason.code);
                        let _ = write_server_control(
                            &mut send,
                            &ServerControl::Disconnect(reason.clone()),
                        )
                        .await;
                        connection
                            .close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
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

        let (replication, interact_rx) = match snap_rx {
            Some((pipe, wake, i)) => (Some((pipe, wake)), Some(i)),
            None => (None, None),
        };
        (replication, interact_rx, occupancy)
    };
    let roster = match &persist {
        Some(worker) => match worker.roster(login.clone()).await {
            Ok(roster) => roster,
            Err(err) => {
                eprintln!("PURGATORY roster failed: {err}");
                stats.leave_handshake();
                connection.close(
                    DisconnectReasonCode::ServerShutdown.as_u8().into(),
                    b"storage unavailable",
                );
                return;
            }
        },
        None => Vec::new(),
    };
    let ready = FrontendSessionReady {
        connection_id,
        roster,
    };
    let response = ServerControl::FrontendSessionReady(ready);
    #[cfg(test)]
    let response = if legacy {
        ServerControl::Welcome(purgatory_protocol::Welcome {
            protocol_version: PROTOCOL_VERSION,
            connection_id,
            server_tick_rate: purgatory_simulation::TICK_RATE_HZ,
            server_label: "test-only legacy entry".into(),
        })
    } else {
        response
    };
    if write_server_control(&mut send, &response).await.is_err() {
        lifecycle.note_welcome_fail();
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
    let welcome_at = Instant::now();
    lifecycle.note_welcome_ok(us(hello_at.elapsed()));

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
    lifecycle.note_session_accepted(us(welcome_at.elapsed()));
    stats
        .session_created
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
        #[cfg(not(test))]
        replication: None,
        #[cfg(not(test))]
        interact_rx: None,
        #[cfg(test)]
        replication,
        #[cfg(test)]
        interact_rx,
        #[cfg(test)]
        occupancy,
        #[cfg(not(test))]
        occupancy: None,
        login,
        persist,
        lifecycle,
        pressure,
        channel_live,
    })
    .await;
}

/// Releases character occupancy if the connection task is dropped before
/// the normal `send_detach` teardown (panic, abort, or skipped await).
struct OccupancyLease {
    tx: GameplayTx,
    id: purgatory_protocol::ConnectionId,
    armed: bool,
}

impl OccupancyLease {
    fn new(tx: GameplayTx, id: purgatory_protocol::ConnectionId) -> Self {
        Self {
            tx,
            id,
            armed: true,
        }
    }
}

impl Drop for OccupancyLease {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
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
    login: DevLogin,
    persist: Option<super::persist::PersistenceHandle>,
    lifecycle: Arc<ConnectionLifecycleBook>,
    pressure: Arc<NetworkPressureBook>,
    channel_live: Arc<std::sync::atomic::AtomicBool>,
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

async fn activate_owned_character(
    worker: &super::persist::PersistenceHandle,
    tx: &GameplayTx,
    login: &DevLogin,
    connection_id: purgatory_protocol::ConnectionId,
    character_id: purgatory_common::CharacterId,
    channel_live: &std::sync::atomic::AtomicBool,
) -> Result<
    (
        ReplicationPipe,
        tokio::sync::watch::Receiver<u64>,
        tokio::sync::mpsc::Receiver<ServerControl>,
        Option<(
            purgatory_persistence::LeaseAuthority,
            super::lease_clock::LocalLeaseDeadline,
        )>,
    ),
    purgatory_protocol::CharacterEnterRejection,
> {
    use purgatory_persistence::SessionAdmission;
    use purgatory_protocol::CharacterEnterRejection as R;
    use std::sync::atomic::Ordering;
    if !channel_live.load(Ordering::Relaxed) {
        return Err(R::StorageFailure);
    }
    let admit_sent = tokio::time::Instant::now();
    let admission = worker
        .admit(login.clone(), character_id)
        .await
        .map_err(|_| R::StorageFailure)?;
    let (owned, authority, deadline) = match admission {
        SessionAdmission::NotOwned => return Err(R::NotOwned),
        SessionAdmission::Held => {
            let stopped = tx
                .stop_for_reconnect(character_id)
                .await
                .map_err(|_| R::GameplayEnterFailure)?;
            let (old, snapshot) = match stopped {
                Ok(pair) => pair,
                Err(EnterError::Pending | EnterError::Occupied) => return Err(R::Occupied),
                Err(_) => return Err(R::GameplayEnterFailure),
            };
            if let Err(err) = worker.save_leased(snapshot, Some(old.clone())).await {
                eprintln!(
                    "PURGATORY persist reconnect save failed; new generation was not taken: {err}"
                );
                return Err(R::StorageFailure);
            }
            let supersede_sent = tokio::time::Instant::now();
            match worker.supersede(old).await {
                Ok((authority, owned)) => {
                    let deadline = super::lease_clock::LocalLeaseDeadline::from_request(
                        supersede_sent,
                        purgatory_persistence::CHARACTER_LEASE_EXPIRY,
                    );
                    if !deadline.reply_still_authorizes(tokio::time::Instant::now()) {
                        release_unused_lease(worker, Some(authority)).await;
                        eprintln!(
                            "PURGATORY persist supersede reply crossed the local lease deadline; gameplay was not entered"
                        );
                        return Err(R::StorageFailure);
                    }
                    (owned, Some(authority), Some(deadline))
                }
                Err(err) => {
                    eprintln!("PURGATORY persist supersede failed: {err}");
                    return Err(R::StorageFailure);
                }
            }
        }
        SessionAdmission::Granted { authority, restore } => {
            let deadline = authority.as_ref().map(|_| {
                super::lease_clock::LocalLeaseDeadline::from_request(
                    admit_sent,
                    purgatory_persistence::CHARACTER_LEASE_EXPIRY,
                )
            });
            if deadline
                .is_some_and(|bound| !bound.reply_still_authorizes(tokio::time::Instant::now()))
            {
                release_unused_lease(worker, authority).await;
                eprintln!(
                    "PURGATORY persist admit reply crossed the local lease deadline; gameplay was not entered"
                );
                return Err(R::StorageFailure);
            }
            (*restore, authority, deadline)
        }
    };
    if !channel_live.load(Ordering::Relaxed) {
        release_unused_lease(worker, authority).await;
        eprintln!(
            "PURGATORY persist channel authority ended during admission; gameplay was not entered"
        );
        return Err(R::StorageFailure);
    }
    let (pipe, wake) = ReplicationPipe::new();
    let (interact_tx, rx) = tokio::sync::mpsc::channel(16);
    match tx
        .enter_restored(
            connection_id,
            owned,
            authority.clone(),
            deadline,
            Some(pipe.clone()),
            Some(interact_tx),
        )
        .await
    {
        Ok(Ok(())) => {
            let channel_stopped = !channel_live.load(Ordering::Relaxed);
            let lease_stopped = deadline
                .is_some_and(|bound| !bound.reply_still_authorizes(tokio::time::Instant::now()));
            if channel_stopped || lease_stopped {
                if tx.abandon_admission(connection_id).await.is_err() {
                    eprintln!(
                        "PURGATORY persist could not remove a character admitted after authority ended"
                    );
                }
                release_unused_lease(worker, authority).await;
                eprintln!(
                    "PURGATORY persist authority ended during world entry; the character was removed and gameplay was not entered"
                );
                return Err(R::StorageFailure);
            }
            let leased = match authority {
                Some(authority) => {
                    let Some(deadline) = deadline else {
                        release_unused_lease(worker, Some(authority)).await;
                        return Err(R::StorageFailure);
                    };
                    Some((authority, deadline))
                }
                None => None,
            };
            Ok((pipe, wake, rx, leased))
        }
        Ok(Err(EnterError::Occupied | EnterError::Pending)) => {
            release_unused_lease(worker, authority).await;
            Err(R::Occupied)
        }
        Ok(Err(EnterError::AuthorityLost)) => {
            release_unused_lease(worker, authority).await;
            eprintln!(
                "PURGATORY persist world entry was refused because admission had already stopped"
            );
            Err(R::StorageFailure)
        }
        Ok(Err(_)) => {
            release_unused_lease(worker, authority).await;
            Err(R::GameplayEnterFailure)
        }
        Err(()) => {
            release_unused_lease(worker, authority).await;
            Err(R::GameplayEnterFailure)
        }
    }
}

async fn release_unused_lease(
    worker: &super::persist::PersistenceHandle,
    authority: Option<purgatory_persistence::LeaseAuthority>,
) {
    if let Some(authority) = authority
        && let Err(err) = worker.release_lease(authority).await
    {
        eprintln!("PURGATORY persist unused lease release failed: {err}");
    }
}

fn spawn_lease_renewal(
    worker: super::persist::PersistenceHandle,
    gameplay: GameplayTx,
    connection_id: purgatory_protocol::ConnectionId,
    authority: purgatory_persistence::LeaseAuthority,
    deadline: super::lease_clock::LocalLeaseDeadline,
) -> tokio::sync::watch::Sender<bool> {
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let extend = gameplay.clone();
    tokio::spawn(async move {
        let stop = super::lease_clock::supervise_renewal(
            move || {
                let worker = worker.clone();
                let authority = authority.clone();
                async move { worker.renew_lease(authority).await.map_err(|_| ()) }
            },
            deadline,
            purgatory_persistence::CHARACTER_LEASE_RENEWAL,
            stop_rx,
            move |extended| {
                let _ = extend.try_note_lease_deadline(connection_id, extended);
            },
        )
        .await;
        if super::lease_clock::authority_ends(stop) {
            eprintln!(
                "PURGATORY persist lease renewal stopped; gameplay stopped connection={connection_id}"
            );
            let _ = gameplay.lose_authority(connection_id).await;
        }
    });
    stop_tx
}

async fn finish_logout(
    worker: &super::persist::PersistenceHandle,
    tx: &GameplayTx,
    connection_id: purgatory_protocol::ConnectionId,
) -> bool {
    match tx.prepare_logout(connection_id).await {
        Ok(Ok(Some((authority, snapshot)))) => {
            match worker.save_leased(snapshot, Some(authority.clone())).await {
                Ok(()) => {
                    if let Err(err) = worker.release_lease(authority).await {
                        eprintln!("PURGATORY persist lease release failed: {err}");
                    }
                }
                Err(err) => {
                    eprintln!(
                        "PURGATORY persist logout save failed; lease was not released and the save was not confirmed: {err}"
                    );
                }
            }
            true
        }
        Ok(Ok(None)) => true,
        Ok(Err(EnterError::Pending)) => {
            eprintln!(
                "PURGATORY persist logout blocked: durable command still pending connection={connection_id}"
            );
            false
        }
        _ => tx.send_detach(connection_id).await,
    }
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
        mut occupancy,
        login,
        persist,
        lifecycle,
        pressure,
        channel_live,
    } = live;
    let mut active = occupancy.is_some();
    let id = session.connection_id;
    let mut lease_renewal: Option<tokio::sync::watch::Sender<bool>> = None;
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
                    Ok(ClientControl::EnterCharacter { character_id }) => {
                        use purgatory_protocol::CharacterEnterRejection as R;
                        if !matches!(rate.note(Instant::now(), abuse_cfg), RateDecision::Allow) {
                            connection.close(DisconnectReasonCode::Malformed.as_u8().into(), b"rate");
                            break;
                        }
                        // Control processing is serialized: a second request cannot race entry.
                        let result = if active { Err(R::InvalidSelection) } else {
                            match (&persist, &gameplay) {
                                (Some(worker), Some(tx)) => {
                                    match activate_owned_character(
                                        worker,
                                        tx,
                                        &login,
                                        id,
                                        character_id,
                                        &channel_live,
                                    )
                                    .await
                                    {
                                        Ok((pipe, wake, rx, leased)) => {
                                            occupancy = Some(OccupancyLease::new(tx.clone(), id));
                                            replication = Some((pipe, wake));
                                            interact_rx = Some(rx);
                                            active = true;
                                            uni_opened = false;
                                            if let Some((authority, deadline)) = leased {
                                                lease_renewal = Some(spawn_lease_renewal(
                                                    worker.clone(),
                                                    tx.clone(),
                                                    id,
                                                    authority,
                                                    deadline,
                                                ));
                                            }
                                            Ok(())
                                        }
                                        Err(reason) => Err(reason),
                                    }
                                }
                                (None, _) => Err(R::StorageFailure),
                                _ => Err(R::GameplayEnterFailure),
                            }
                        };
                        let response = match result {
                            Ok(()) => ServerControl::Welcome(purgatory_protocol::Welcome {
                                protocol_version: PROTOCOL_VERSION,
                                connection_id: id,
                                server_tick_rate: purgatory_simulation::TICK_RATE_HZ,
                                server_label: "PURGATORY".into(),
                            }),
                            Err(reason) => ServerControl::EnterCharacterRejected(reason),
                        };
                        if write_server_control(&mut send, &response).await.is_err() { break; }
                    }
                    Ok(ClientControl::CreateCharacter { name }) => {
                        if !matches!(rate.note(Instant::now(), abuse_cfg), RateDecision::Allow) {
                            connection.close(DisconnectReasonCode::Malformed.as_u8().into(), b"rate");
                            break;
                        }
                        let result = match &persist {
                            Some(worker) => worker.create_character(login.clone(), name).await,
                            None => purgatory_protocol::CreateCharacterResult::Rejected(purgatory_protocol::CharacterCreateRejection::StorageFailure),
                        };
                        if write_server_control(&mut send, &ServerControl::CreateCharacterResult(result)).await.is_err() { break; }
                    }
                    // No gameplay authority exists in a pre-game session.
                    Ok(_) if !active => {
                        connection.close(DisconnectReasonCode::UnexpectedMessage.as_u8().into(), b"pre-game");
                        break;
                    }
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
                    Ok(ClientControl::DialogueAdvance(request)) => {
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
                                    && !tx.send_dialogue_advance(id, request).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::DialogueChoose(request)) => {
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
                                    && !tx.send_dialogue_choose(id, request).await
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
                    Ok(ClientControl::DevSetSpeed(req)) => {
                        println!(
                            "DEV_SPEED recv connection={id} speed={:?}",
                            req.speed
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
                                    && !tx.send_dev_set_speed(id, req.speed).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::DevSetJump(req)) => {
                        println!(
                            "DEV_JUMP recv connection={id} jump={:?}",
                            req.jump
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
                                    && !tx.send_dev_set_jump(id, req.jump).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::DevSpawnNpc(req)) => {
                        println!(
                            "DEV_NPC_SPAWN recv connection={id} npc={}",
                            req.npc_content_id
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
                                    && !tx
                                        .send_dev_spawn_npc(id, req.npc_content_id)
                                        .await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::DevSpawnMonster(req)) => {
                        println!(
                            "DEV_MONSTER_SPAWN recv connection={id} monster={}",
                            req.monster_content_id
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
                                    && !tx
                                        .send_dev_spawn_monster(id, req.monster_content_id)
                                        .await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::Equip(req)) => {
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
                                    && !tx.send_equip(id, req).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::Unequip(req)) => {
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
                                    && !tx.send_unequip(id, req).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::Pickup(req)) => {
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
                                    && !tx.send_pickup(id, req).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::Drop(req)) => {
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
                                    && !tx.send_drop(id, req).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::DevPresentationOneShot(req)) => {
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
                                    && !tx.send_dev_presentation_oneshot(id, req.kind).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::DevResetPlayer) => {
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
                                    && !tx.send_dev_reset_player(id).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::Respawn) => {
                        match rate.note(Instant::now(), abuse_cfg) {
                            RateDecision::Disconnect => {
                                connection.close(
                                    DisconnectReasonCode::Malformed.as_u8().into(),
                                    b"protocol",
                                );
                                break;
                            }
                            RateDecision::Drop => {}
                            RateDecision::Allow => {
                                if let Some(tx) = &gameplay
                                    && !tx.send_respawn(id).await
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(ClientControl::AbilityActivate(req)) => {
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
                                    && !tx.send_ability_activate(id, req).await
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
                    let age = us(frame.enqueued_at.elapsed());
                    if write_replication_payload(
                        send,
                        &frame.payload,
                        &stats,
                        &pressure,
                        id.get(),
                        age,
                    )
                    .await
                    {
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

    if let Some(tx) = lease_renewal.take() {
        let _ = tx.send(true);
    }
    if let Some(mut lease) = occupancy.take() {
        let logged_out = if let (Some(worker), Some(tx)) = (&persist, &gameplay) {
            finish_logout(worker, tx, id).await
        } else {
            lease.tx.send_detach(id).await
        };
        if logged_out {
            lease.armed = false;
        } else {
            stats
                .lifecycle_handoff_dropped
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
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
    lifecycle.note_disconnect(us(session.connected_since.elapsed()));
    pressure.remove_client(id.get());
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
    pressure: &NetworkPressureBook,
    connection_id: u64,
    queue_age_us: u64,
) -> bool {
    stats
        .snapshot_size_max_bytes
        .fetch_max(payload.len() as u64, std::sync::atomic::Ordering::Relaxed);
    let encode_start = Instant::now();
    let Ok(frame) = encode_gameplay_frame(payload) else {
        stats
            .snapshot_encode_failed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return false;
    };
    let encode_us = us(encode_start.elapsed());
    stats
        .snapshot_encode_time_max_micros
        .fetch_max(encode_us, std::sync::atomic::Ordering::Relaxed);
    let n = frame.len() as u64;
    let drain_start = Instant::now();
    if send.write_all(&frame).await.is_ok() {
        let drain_us = us(drain_start.elapsed());
        pressure.note_write_drain(connection_id, drain_us, n, queue_age_us);
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

#[cfg(test)]
mod admission_race {
    use std::future::Future;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use purgatory_common::{CharacterId, DevLogin};
    use purgatory_persistence::{
        CharacterNarrativeState, LeaseAuthority, OwnedRestore, PersistentCharacter,
        SessionAdmission,
    };
    use purgatory_protocol::{CharacterEnterRejection, ConnectionId, InputCommand, MoveAxis};

    use crate::network::gameplay::{GameplayOwner, InputUpdate, SeqDecision, gameplay_channels};
    use crate::network::lease_clock::LocalLeaseDeadline;
    use crate::network::persist::PersistenceHandle;
    use crate::network::spawn_channel_renewal;

    use super::activate_owned_character;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "purgatory-admit-race-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    async fn advance_for(duration: Duration) {
        tokio::time::advance(duration + Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
    }

    async fn until(label: &str, mut ready: impl FnMut() -> bool) {
        for _ in 0..100 {
            if ready() {
                return;
            }
            // Real sleep, not paused Tokio time: the persistence worker is an
            // OS thread, and a yield can finish before that thread enters.
            std::thread::sleep(Duration::from_millis(2));
            tokio::task::yield_now().await;
        }
        panic!("timed out waiting for {label}");
    }

    /// The channel-renewal hold is set by the persistence worker thread.
    /// Paused Tokio time does not run that thread, so the wait has to observe
    /// real time. Yielding alone times out while the worker is still entering.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn channel_renewal_wait_observes_the_worker_thread() {
        let entered = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&entered);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            flag.store(true, Ordering::SeqCst);
        });
        until("channel renewal held", || entered.load(Ordering::SeqCst)).await;
    }

    fn leased_admission(character_id: CharacterId) -> SessionAdmission {
        SessionAdmission::Granted {
            authority: Some(LeaseAuthority {
                login: DevLogin::parse("dev.local").unwrap(),
                character_id,
                generation: 1,
            }),
            restore: Box::new(OwnedRestore {
                character: PersistentCharacter::new_default(character_id),
                items: Vec::new(),
                narrative: CharacterNarrativeState::default(),
            }),
        }
    }

    fn move_command(connection_id: ConnectionId) -> InputUpdate {
        InputUpdate::Command {
            connection_id,
            command: InputCommand {
                input_epoch: 0,
                sequence: 1,
                move_axis: MoveAxis::Right,
                jump_pressed: false,
                down_held: false,
                portal_held: false,
            },
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn channel_stop_during_admit_does_not_enter_world() {
        let dir = temp_dir("channel");
        let worker = PersistenceHandle::spawn_fixture().unwrap();
        let login = DevLogin::parse("dev.local").unwrap();
        worker.provision_dev_user(login.clone()).await.unwrap();
        let created = worker.create_character(login.clone(), "Alpha".into()).await;
        let purgatory_protocol::CreateCharacterResult::Created { roster } = created else {
            panic!("create character: {created:?}");
        };
        let character_id = roster[0].character_id;
        let admit_hold = worker.hold_next_admit();
        let renew_hold = worker.hold_next_channel_renewal();
        let (tx, mut life_rx, mut input_rx) = gameplay_channels(8, 8);
        let mut owner = GameplayOwner::new();
        let channel_live = Arc::new(AtomicBool::new(true));
        let (_stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        let connection_id = ConnectionId::from_raw(4);
        let mut admission = std::pin::pin!(activate_owned_character(
            &worker,
            &tx,
            &login,
            connection_id,
            character_id,
            &channel_live,
        ));
        let mut outcome = None;
        for _ in 0..100 {
            if admit_hold.entered() {
                break;
            }
            std::future::poll_fn(|cx| {
                if outcome.is_none()
                    && let std::task::Poll::Ready(value) = admission.as_mut().poll(cx)
                {
                    outcome = Some(value);
                }
                std::task::Poll::Ready(())
            })
            .await;
            std::thread::sleep(Duration::from_millis(2));
            tokio::task::yield_now().await;
        }
        assert!(
            admit_hold.entered() && outcome.is_none(),
            "admit reply was not held; finished={}",
            outcome.is_some()
        );
        spawn_channel_renewal(
            worker.clone(),
            tx.clone(),
            channel_live.clone(),
            stop_rx,
            LocalLeaseDeadline::from_request(
                tokio::time::Instant::now(),
                purgatory_persistence::CHANNEL_GENERATION_EXPIRY,
            ),
            0,
            1,
        );
        tokio::task::yield_now().await;
        advance_for(purgatory_persistence::CHANNEL_GENERATION_RENEWAL).await;
        until("channel renewal held", || renew_hold.entered()).await;
        advance_for(
            purgatory_persistence::CHANNEL_GENERATION_EXPIRY
                .saturating_sub(purgatory_persistence::CHANNEL_GENERATION_RENEWAL),
        )
        .await;
        until("channel renewal stop", || {
            !channel_live.load(Ordering::SeqCst)
        })
        .await;
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
        owner.drain(&mut life_rx, &mut input_rx);
        admit_hold.release();
        for _ in 0..100 {
            owner.drain(&mut life_rx, &mut input_rx);
            if outcome.is_some() {
                break;
            }
            std::future::poll_fn(|cx| {
                if outcome.is_none()
                    && let std::task::Poll::Ready(value) = admission.as_mut().poll(cx)
                {
                    outcome = Some(value);
                }
                std::task::Poll::Ready(())
            })
            .await;
            tokio::task::yield_now().await;
        }
        renew_hold.release();
        let result = outcome.expect("admission did not finish after the channel stop");
        let accepted_before = owner.input_accepted;
        tx.input.try_send(move_command(connection_id)).unwrap();
        owner.drain(&mut life_rx, &mut input_rx);
        assert!(
            matches!(result, Err(CharacterEnterRejection::StorageFailure)),
            "channel stop during admit still entered World: ok={} entity={:?}",
            result.is_ok(),
            owner.entity_of(connection_id)
        );
        assert!(owner.entity_of(connection_id).is_none());
        assert_eq!(owner.input_accepted, accepted_before);
        assert_eq!(
            owner.apply_input(move_command(connection_id)),
            SeqDecision::Stale
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn character_deadline_during_enter_does_not_enter_world() {
        let dir = temp_dir("character");
        let worker = PersistenceHandle::spawn_fixture().unwrap();
        let login = DevLogin::parse("dev.local").unwrap();
        let character_id = CharacterId::from_raw(9);
        worker.script_next_admit(leased_admission(character_id));
        let (tx, mut life_rx, mut input_rx) = gameplay_channels(8, 8);
        let mut owner = GameplayOwner::new();
        let channel_live = Arc::new(AtomicBool::new(true));
        let connection_id = ConnectionId::from_raw(5);
        let mut admission = std::pin::pin!(activate_owned_character(
            &worker,
            &tx,
            &login,
            connection_id,
            character_id,
            &channel_live,
        ));
        let mut outcome = None;
        for _ in 0..100 {
            if !life_rx.is_empty() {
                break;
            }
            std::future::poll_fn(|cx| {
                if outcome.is_none()
                    && let std::task::Poll::Ready(value) = admission.as_mut().poll(cx)
                {
                    outcome = Some(value);
                }
                std::task::Poll::Ready(())
            })
            .await;
            // Real sleep, not paused Tokio time: the worker thread has to
            // answer before the deadline is advanced.
            std::thread::sleep(Duration::from_millis(2));
            tokio::task::yield_now().await;
        }
        assert!(
            outcome.is_none() && !life_rx.is_empty(),
            "enter was not queued after the pre-enter deadline check; finished={}",
            outcome.is_some()
        );
        advance_for(purgatory_persistence::CHARACTER_LEASE_EXPIRY).await;
        for _ in 0..100 {
            owner.drain(&mut life_rx, &mut input_rx);
            if outcome.is_some() {
                break;
            }
            std::future::poll_fn(|cx| {
                if outcome.is_none()
                    && let std::task::Poll::Ready(value) = admission.as_mut().poll(cx)
                {
                    outcome = Some(value);
                }
                std::task::Poll::Ready(())
            })
            .await;
            std::thread::sleep(Duration::from_millis(2));
            tokio::task::yield_now().await;
        }
        let result = outcome.expect("admission did not finish after the character deadline");
        let accepted_before = owner.input_accepted;
        tx.input.try_send(move_command(connection_id)).unwrap();
        owner.drain(&mut life_rx, &mut input_rx);
        assert!(
            matches!(result, Err(CharacterEnterRejection::StorageFailure)),
            "expired character deadline still entered World: ok={} entity={:?}",
            result.is_ok(),
            owner.entity_of(connection_id)
        );
        assert!(owner.entity_of(connection_id).is_none());
        assert_eq!(owner.input_accepted, accepted_before);
        assert_eq!(
            owner.apply_input(move_command(connection_id)),
            SeqDecision::Stale
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn equipped_restore(
        character_id: CharacterId,
        content: purgatory_common::ContentId,
        slot: purgatory_persistence::DurableEquipmentSlot,
    ) -> SessionAdmission {
        use purgatory_common::ItemInstanceId;
        use purgatory_persistence::{CharacterItemLocation, ItemOwner, ItemRecord};
        SessionAdmission::Granted {
            authority: Some(LeaseAuthority {
                login: DevLogin::parse("dev.local").unwrap(),
                character_id,
                generation: 1,
            }),
            restore: Box::new(OwnedRestore {
                character: PersistentCharacter::new_default(character_id),
                items: vec![ItemRecord {
                    item_instance_id: ItemInstanceId::from_raw(7),
                    definition_content_id: content,
                    quantity: 1,
                    owner: ItemOwner::Character {
                        character_id,
                        location: CharacterItemLocation::Equipped { slot },
                    },
                }],
                narrative: CharacterNarrativeState::default(),
            }),
        }
    }

    async fn drive_scripted_admission(
        worker: &PersistenceHandle,
        owner: &mut GameplayOwner,
        queues: (
            &mut tokio::sync::mpsc::Receiver<crate::network::gameplay::LifecycleCmd>,
            &mut tokio::sync::mpsc::Receiver<InputUpdate>,
        ),
        tx: &crate::network::gameplay::GameplayTx,
        login: &DevLogin,
        connection_id: ConnectionId,
        character_id: CharacterId,
    ) -> Result<
        (
            crate::network::replication::ReplicationPipe,
            tokio::sync::watch::Receiver<u64>,
            tokio::sync::mpsc::Receiver<purgatory_protocol::ServerControl>,
            Option<(LeaseAuthority, LocalLeaseDeadline)>,
        ),
        CharacterEnterRejection,
    > {
        let channel_live = Arc::new(AtomicBool::new(true));
        let mut admission = std::pin::pin!(activate_owned_character(
            worker,
            tx,
            login,
            connection_id,
            character_id,
            &channel_live,
        ));
        let (life_rx, input_rx) = queues;
        let mut outcome = None;
        for _ in 0..200 {
            owner.drain(life_rx, input_rx);
            std::future::poll_fn(|cx| {
                if outcome.is_none()
                    && let std::task::Poll::Ready(value) = admission.as_mut().poll(cx)
                {
                    outcome = Some(value);
                }
                std::task::Poll::Ready(())
            })
            .await;
            if outcome.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
            tokio::task::yield_now().await;
        }
        outcome.expect("scripted admission did not finish")
    }

    #[tokio::test]
    async fn durable_restore_rejects_a_non_equippable_item_in_weapon() {
        let dir = temp_dir("potion-weapon");
        let worker = PersistenceHandle::spawn_fixture().unwrap();
        let login = DevLogin::parse("dev.local").unwrap();
        worker.provision_dev_user(login.clone()).await.unwrap();
        let created = worker.create_character(login.clone(), "Alpha".into()).await;
        let purgatory_protocol::CreateCharacterResult::Created { roster } = created else {
            panic!("create character: {created:?}");
        };
        let character_id = roster[0].character_id;
        worker.script_next_admit(equipped_restore(
            character_id,
            purgatory_common::ContentId::from_raw(30011),
            purgatory_persistence::DurableEquipmentSlot::Weapon,
        ));
        let (tx, mut life_rx, mut input_rx) = gameplay_channels(8, 8);
        let mut owner = GameplayOwner::new();
        let connection_id = ConnectionId::from_raw(31);
        let result = drive_scripted_admission(
            &worker,
            &mut owner,
            (&mut life_rx, &mut input_rx),
            &tx,
            &login,
            connection_id,
            character_id,
        )
        .await;
        assert!(
            matches!(result, Err(CharacterEnterRejection::GameplayEnterFailure)),
            "non-equippable weapon restore entered: ok={} entity={:?}",
            result.is_ok(),
            owner.entity_of(connection_id)
        );
        assert!(owner.entity_of(connection_id).is_none());
        assert_eq!(worker.release_calls_for_test(), 1);
        worker.shutdown(Duration::from_secs(2), None).await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn durable_restore_rejects_an_equippable_item_in_the_wrong_slot() {
        let dir = temp_dir("cap-weapon");
        let worker = PersistenceHandle::spawn_fixture().unwrap();
        let login = DevLogin::parse("dev.local").unwrap();
        worker.provision_dev_user(login.clone()).await.unwrap();
        let created = worker.create_character(login.clone(), "Alpha".into()).await;
        let purgatory_protocol::CreateCharacterResult::Created { roster } = created else {
            panic!("create character: {created:?}");
        };
        let character_id = roster[0].character_id;
        worker.script_next_admit(equipped_restore(
            character_id,
            purgatory_common::ContentId::from_raw(30001),
            purgatory_persistence::DurableEquipmentSlot::Weapon,
        ));
        let (tx, mut life_rx, mut input_rx) = gameplay_channels(8, 8);
        let mut owner = GameplayOwner::new();
        let connection_id = ConnectionId::from_raw(32);
        let result = drive_scripted_admission(
            &worker,
            &mut owner,
            (&mut life_rx, &mut input_rx),
            &tx,
            &login,
            connection_id,
            character_id,
        )
        .await;
        assert!(
            matches!(result, Err(CharacterEnterRejection::GameplayEnterFailure)),
            "wrong-slot restore entered: ok={} entity={:?}",
            result.is_ok(),
            owner.entity_of(connection_id)
        );
        assert!(owner.entity_of(connection_id).is_none());
        assert_eq!(worker.release_calls_for_test(), 1);
        worker.shutdown(Duration::from_secs(2), None).await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn durable_restore_keeps_valid_equipment() {
        let dir = temp_dir("sword-weapon");
        let worker = PersistenceHandle::spawn_fixture().unwrap();
        let login = DevLogin::parse("dev.local").unwrap();
        worker.provision_dev_user(login.clone()).await.unwrap();
        let created = worker.create_character(login.clone(), "Alpha".into()).await;
        let purgatory_protocol::CreateCharacterResult::Created { roster } = created else {
            panic!("create character: {created:?}");
        };
        let character_id = roster[0].character_id;
        let sword = purgatory_common::ContentId::from_raw(30006);
        worker.script_next_admit(equipped_restore(
            character_id,
            sword,
            purgatory_persistence::DurableEquipmentSlot::Weapon,
        ));
        let (tx, mut life_rx, mut input_rx) = gameplay_channels(8, 8);
        let mut owner = GameplayOwner::new();
        let connection_id = ConnectionId::from_raw(33);
        let result = drive_scripted_admission(
            &worker,
            &mut owner,
            (&mut life_rx, &mut input_rx),
            &tx,
            &login,
            connection_id,
            character_id,
        )
        .await;
        assert!(result.is_ok(), "valid equipment was rejected");
        let actor = owner.entity_of(connection_id).expect("player");
        assert_eq!(
            owner
                .world()
                .equipment_slot(actor, purgatory_simulation::EquipmentSlot::Weapon),
            Some(sword)
        );
        assert_eq!(worker.release_calls_for_test(), 0);
        worker.shutdown(Duration::from_secs(2), None).await;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
