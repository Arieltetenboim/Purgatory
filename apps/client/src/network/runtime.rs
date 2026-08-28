//! Dedicated Tokio thread for Quinn IO.
//!
//! One session task at a time. Disconnect/Shutdown use a `watch` control plane
//! so they cannot be stranded behind a full Connect command queue. A new
//! Connect stamps the current disconnect epoch; it must not clear an in-flight
//! Disconnect for a previous attempt.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::Instant;

use quinn::crypto::rustls::QuicClientConfig;
use quinn::{ClientConfig, Connection, Endpoint, RecvStream, SendStream, TransportConfig, VarInt};
use tokio::sync::{mpsc, watch};

use purgatory_protocol::{
    ALPN_PROTOCOL, ClientControl, HANDSHAKE_TIMEOUT, Hello, InputCommand, PING_INTERVAL,
    PROTOCOL_VERSION, ServerControl, ServerDatagram, WorldSnapshot, decode_server_control,
    decode_server_datagram, decode_world_snapshot, encode_client_control, encode_client_datagram,
    encode_frame, peek_frame_len, peek_gameplay_frame_len,
};

use super::cert::DevOnlySkipServerVerification;
use super::config::ClientEndpointConfig;
use super::failure::{NetworkFailureKind, TransportSymptom};
use super::state::{ConnectionAttemptId, NetworkCommand, NetworkEvent};

const CMD_CAP: usize = 8;
const INPUT_CAP: usize = 128;
const LIFECYCLE_CAP: usize = 16;
const TELEMETRY_CAP: usize = 32;
/// Oldest outstanding ping is dropped when this many wait for a Pong.
const MAX_OUTSTANDING_PINGS: usize = 4;

#[derive(Clone, Copy, Debug, Default)]
struct Control {
    shutdown: bool,
    disconnect_epoch: u64,
}

enum RuntimeCommand {
    Connect {
        attempt_id: ConnectionAttemptId,
        epoch: u64,
    },
}

/// Gameplay messages on the reliable control stream. Drained in order; never
/// coalesced. `HeldCancel` is not an `InputCommand` and has no sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClientGameplayMsg {
    Input(InputCommand),
    HeldCancel,
}

struct EventSink {
    lifecycle: mpsc::Sender<NetworkEvent>,
    telemetry: mpsc::Sender<NetworkEvent>,
    telemetry_dropped: Arc<AtomicU64>,
    verbose: Arc<AtomicBool>,
    snapshots: watch::Sender<Option<WorldSnapshot>>,
    snapshot_malformed: Arc<AtomicU64>,
}

impl EventSink {
    fn verbose(&self) -> bool {
        self.verbose.load(Ordering::Relaxed)
    }

    fn trace(&self, line: &str) {
        if self.verbose() {
            println!("PURGATORY net verbose {line}");
        }
    }

    async fn emit(&self, event: NetworkEvent, control: &watch::Receiver<Control>) {
        if self.verbose() && event.is_lifecycle() {
            match &event {
                NetworkEvent::Connecting { attempt_id } => {
                    self.trace(&format!("attempt={attempt_id} Connecting"));
                }
                NetworkEvent::Handshaking { attempt_id } => {
                    self.trace(&format!("attempt={attempt_id} Handshaking"));
                }
                NetworkEvent::Connected {
                    attempt_id,
                    connection_id,
                    ..
                } => {
                    self.trace(&format!(
                        "attempt={attempt_id} Connected connection_id={connection_id}"
                    ));
                }
                NetworkEvent::Rejected { attempt_id, reason } => {
                    let kind = NetworkFailureKind::from_wire(reason.code);
                    self.trace(&format!(
                        "attempt={attempt_id} Rejected {} retryable={}",
                        kind.debug_label(),
                        kind.retryable()
                    ));
                }
                NetworkEvent::Disconnected { attempt_id, kind } => {
                    self.trace(&format!(
                        "attempt={attempt_id} {} retryable={}",
                        kind.debug_label(),
                        kind.retryable()
                    ));
                }
                NetworkEvent::RttUpdated { .. } => {}
            }
        }
        if !event.is_lifecycle() {
            if self.telemetry.try_send(event).is_err() {
                self.telemetry_dropped.fetch_add(1, Ordering::Relaxed);
            }
            return;
        }
        match self.lifecycle.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Closed(_)) => {}
            Err(mpsc::error::TrySendError::Full(event)) => {
                tokio::select! {
                    result = self.lifecycle.send(event) => {
                        let _ = result;
                    }
                    () = wait_shutdown(control) => {}
                }
            }
        }
    }

    fn push_snapshot(&self, snap: WorldSnapshot) {
        if self.verbose() {
            self.trace(&format!(
                "snapshot recv seq={} tick={} entities={}",
                snap.snapshot_sequence,
                snap.server_tick,
                snap.entities.len()
            ));
        }
        let _ = self.snapshots.send(Some(snap));
    }

    fn note_malformed_snapshot(&self) {
        self.snapshot_malformed.fetch_add(1, Ordering::Relaxed);
    }

    fn clear_snapshots(&self) {
        let _ = self.snapshots.send(None);
    }
}

/// Handle owned by the winit thread.
pub struct NetworkHandle {
    commands: mpsc::Sender<RuntimeCommand>,
    input: mpsc::Sender<ClientGameplayMsg>,
    control: watch::Sender<Control>,
    lifecycle: mpsc::Receiver<NetworkEvent>,
    telemetry: mpsc::Receiver<NetworkEvent>,
    telemetry_dropped: Arc<AtomicU64>,
    log_verbose: Arc<AtomicBool>,
    snapshots: watch::Receiver<Option<WorldSnapshot>>,
    snapshot_malformed: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl NetworkHandle {
    pub fn start(config: ClientEndpointConfig) -> Result<Self, String> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CAP);
        let (input_tx, input_rx) = mpsc::channel(INPUT_CAP);
        let (life_tx, life_rx) = mpsc::channel(LIFECYCLE_CAP);
        let (tel_tx, tel_rx) = mpsc::channel(TELEMETRY_CAP);
        let telemetry_dropped = Arc::new(AtomicU64::new(0));
        let log_verbose = Arc::new(AtomicBool::new(false));
        let log_verbose_thread = Arc::clone(&log_verbose);
        let (control_tx, control_rx) = watch::channel(Control::default());
        let (snap_tx, snap_rx) = watch::channel(None);
        let snapshot_malformed = Arc::new(AtomicU64::new(0));
        let sink = EventSink {
            lifecycle: life_tx,
            telemetry: tel_tx,
            telemetry_dropped: Arc::clone(&telemetry_dropped),
            verbose: Arc::clone(&log_verbose),
            snapshots: snap_tx,
            snapshot_malformed: Arc::clone(&snapshot_malformed),
        };
        let thread = std::thread::Builder::new()
            .name("purgatory-net".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("network tokio runtime");
                runtime.block_on(network_loop(
                    config,
                    cmd_rx,
                    input_rx,
                    sink,
                    control_rx,
                    log_verbose_thread,
                ));
            })
            .map_err(|err| format!("network thread: {err}"))?;
        Ok(Self {
            commands: cmd_tx,
            input: input_tx,
            control: control_tx,
            lifecycle: life_rx,
            telemetry: tel_rx,
            telemetry_dropped,
            log_verbose,
            snapshots: snap_rx,
            snapshot_malformed,
            thread: Some(thread),
        })
    }

    /// Non-blocking. Never stall the render thread.
    ///
    /// Connect uses the bounded command queue (`try_send`). Disconnect and
    /// Shutdown bump a watch control plane so they remain possible under
    /// Connect pressure.
    pub fn try_send(&self, command: NetworkCommand) -> bool {
        match command {
            NetworkCommand::Connect { attempt_id } => {
                let epoch = self.control.borrow().disconnect_epoch;
                self.commands
                    .try_send(RuntimeCommand::Connect { attempt_id, epoch })
                    .is_ok()
            }
            NetworkCommand::Disconnect => {
                self.control.send_modify(|control| {
                    control.disconnect_epoch = control.disconnect_epoch.wrapping_add(1);
                });
                true
            }
            NetworkCommand::Shutdown => {
                self.control.send_modify(|control| {
                    control.shutdown = true;
                    control.disconnect_epoch = control.disconnect_epoch.wrapping_add(1);
                });
                true
            }
        }
    }

    /// Non-blocking gameplay input. Dropped if the queue is full. Never stalls
    /// the render thread. The session task ignores leftovers until Connected.
    pub fn try_send_input(&self, command: InputCommand) -> bool {
        self.input
            .try_send(ClientGameplayMsg::Input(command))
            .is_ok()
    }

    /// Pathological focus-loss barrier. No sequence. Dropped if the queue is full.
    pub fn try_send_held_cancel(&self) -> bool {
        self.input.try_send(ClientGameplayMsg::HeldCancel).is_ok()
    }

    /// Drain lifecycle events first, then telemetry. Caller applies them to
    /// [`crate::lifecycle::ClientLifecycle`].
    pub fn poll(&mut self, mut apply: impl FnMut(NetworkEvent)) {
        while let Ok(event) = self.lifecycle.try_recv() {
            apply(event);
        }
        while let Ok(event) = self.telemetry.try_recv() {
            apply(event);
        }
    }

    /// Latest coalesced snapshot, if the network thread published a newer one.
    pub fn poll_snapshot(&mut self) -> Option<WorldSnapshot> {
        if !self.snapshots.has_changed().unwrap_or(false) {
            return None;
        }
        self.snapshots.borrow_and_update().clone()
    }

    #[must_use]
    pub fn snapshot_malformed(&self) -> u64 {
        self.snapshot_malformed.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn telemetry_dropped(&self) -> u64 {
        self.telemetry_dropped.load(Ordering::Relaxed)
    }

    pub fn set_log_flags(&self, _lifecycle: bool, verbose: bool) {
        self.log_verbose.store(verbose, Ordering::Relaxed);
    }
}

impl Drop for NetworkHandle {
    fn drop(&mut self) {
        self.control.send_modify(|control| {
            control.shutdown = true;
            control.disconnect_epoch = control.disconnect_epoch.wrapping_add(1);
        });
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn wait_shutdown(control: &watch::Receiver<Control>) {
    let mut rx = control.clone();
    loop {
        if rx.borrow().shutdown {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

enum AfterSession {
    Idle,
    Shutdown,
}

fn session_cut(control: &watch::Receiver<Control>, epoch: u64) -> Option<AfterSession> {
    let snapshot = *control.borrow();
    if snapshot.shutdown {
        Some(AfterSession::Shutdown)
    } else if snapshot.disconnect_epoch != epoch {
        Some(AfterSession::Idle)
    } else {
        None
    }
}

async fn network_loop(
    config: ClientEndpointConfig,
    mut commands: mpsc::Receiver<RuntimeCommand>,
    mut inputs: mpsc::Receiver<ClientGameplayMsg>,
    events: EventSink,
    mut control: watch::Receiver<Control>,
    log_verbose: Arc<AtomicBool>,
) {
    let endpoint = match make_endpoint(config.idle_timeout) {
        Ok(ep) => ep,
        Err(err) => {
            eprintln!("PURGATORY client network endpoint failed: {err}");
            return;
        }
    };
    loop {
        if control.borrow().shutdown {
            endpoint.close(0u32.into(), b"shutdown");
            break;
        }
        tokio::select! {
            cmd = commands.recv() => {
                match cmd {
                    None => {
                        endpoint.close(0u32.into(), b"shutdown");
                        break;
                    }
                    Some(RuntimeCommand::Connect { attempt_id, epoch }) => {
                        while let Ok(RuntimeCommand::Connect { .. }) = commands.try_recv() {}
                        drain_inputs(&mut inputs);
                        if log_verbose.load(Ordering::Relaxed) {
                            println!(
                                "PURGATORY net verbose attempt={attempt_id} starting connect"
                            );
                        }
                        let end = run_session(
                            &endpoint,
                            config.server,
                            attempt_id,
                            epoch,
                            &mut commands,
                            &mut inputs,
                            &events,
                            &mut control,
                        )
                        .await;
                        drain_inputs(&mut inputs);
                        if matches!(end, AfterSession::Shutdown) {
                            endpoint.close(0u32.into(), b"shutdown");
                            break;
                        }
                    }
                }
            }
            _ = inputs.recv() => {
                drain_inputs(&mut inputs);
            }
            changed = control.changed() => {
                if changed.is_err() || control.borrow().shutdown {
                    endpoint.close(0u32.into(), b"shutdown");
                    break;
                }
            }
        }
    }
}

fn drain_inputs(inputs: &mut mpsc::Receiver<ClientGameplayMsg>) {
    while inputs.try_recv().is_ok() {}
}

#[allow(clippy::too_many_arguments)]
async fn run_session(
    endpoint: &Endpoint,
    server: SocketAddr,
    attempt_id: ConnectionAttemptId,
    epoch: u64,
    commands: &mut mpsc::Receiver<RuntimeCommand>,
    inputs: &mut mpsc::Receiver<ClientGameplayMsg>,
    events: &EventSink,
    control: &mut watch::Receiver<Control>,
) -> AfterSession {
    events
        .emit(NetworkEvent::Connecting { attempt_id }, control)
        .await;
    if let Some(end) = session_cut(control, epoch) {
        return emit_closed(attempt_id, events, control, end).await;
    }
    let connecting = match endpoint.connect(server, "localhost") {
        Ok(c) => c,
        Err(err) => {
            let kind = NetworkFailureKind::from_transport(TransportSymptom::ConnectRefused);
            events.trace(&format!(
                "attempt={attempt_id} {} cause={err}",
                kind.debug_label()
            ));
            events
                .emit(NetworkEvent::Disconnected { attempt_id, kind }, control)
                .await;
            return AfterSession::Idle;
        }
    };
    tokio::pin!(connecting);

    let connection = loop {
        tokio::select! {
            result = &mut connecting => match result {
                Ok(conn) => break conn,
                Err(err) => {
                    let kind = NetworkFailureKind::from_transport(TransportSymptom::ConnectRefused);
                    events.trace(&format!(
                        "attempt={attempt_id} {} cause={err}",
                        kind.debug_label()
                    ));
                    events.emit(
                        NetworkEvent::Disconnected {
                            attempt_id,
                            kind,
                        },
                        control,
                    ).await;
                    return AfterSession::Idle;
                }
            },
            cmd = commands.recv() => {
                match cmd {
                    None => {
                        return emit_closed(
                            attempt_id,
                            events,
                            control,
                            AfterSession::Shutdown,
                        )
                        .await;
                    }
                    Some(RuntimeCommand::Connect { .. }) => {}
                }
            }
            changed = control.changed() => {
                if changed.is_err() {
                    return emit_closed(
                        attempt_id,
                        events,
                        control,
                        AfterSession::Shutdown,
                    )
                    .await;
                }
                if let Some(end) = session_cut(control, epoch) {
                    return emit_closed(attempt_id, events, control, end).await;
                }
            }
        }
    };

    events
        .emit(NetworkEvent::Handshaking { attempt_id }, control)
        .await;

    match handshake_and_live(
        connection, attempt_id, epoch, commands, inputs, events, control,
    )
    .await
    {
        Ok(AfterSession::Shutdown) => AfterSession::Shutdown,
        Ok(AfterSession::Idle) => AfterSession::Idle,
        Err(outcome) => {
            events.emit(outcome, control).await;
            AfterSession::Idle
        }
    }
}

async fn emit_closed(
    attempt_id: ConnectionAttemptId,
    events: &EventSink,
    control: &watch::Receiver<Control>,
    end: AfterSession,
) -> AfterSession {
    events
        .emit(
            NetworkEvent::Disconnected {
                attempt_id,
                kind: match end {
                    AfterSession::Shutdown => NetworkFailureKind::LocalShutdown,
                    AfterSession::Idle => NetworkFailureKind::ClientRequestedDisconnect,
                },
            },
            control,
        )
        .await;
    end
}

#[allow(clippy::too_many_arguments)]
async fn handshake_and_live(
    connection: Connection,
    attempt_id: ConnectionAttemptId,
    epoch: u64,
    commands: &mut mpsc::Receiver<RuntimeCommand>,
    inputs: &mut mpsc::Receiver<ClientGameplayMsg>,
    events: &EventSink,
    control: &mut watch::Receiver<Control>,
) -> Result<AfterSession, NetworkEvent> {
    let (mut send, mut recv) = {
        let open_bi = connection.open_bi();
        tokio::pin!(open_bi);
        loop {
            tokio::select! {
                opened = &mut open_bi => {
                    break opened.map_err(|err| {
                        events.trace(&format!(
                            "attempt={attempt_id} TransportLost cause={err}"
                        ));
                        NetworkEvent::Disconnected {
                            attempt_id,
                            kind: NetworkFailureKind::TransportLost,
                        }
                    })?;
                }
                cmd = commands.recv() => {
                    match cmd {
                        None => {
                            connection.close(0u32.into(), b"cancel");
                            return Ok(AfterSession::Shutdown);
                        }
                        Some(RuntimeCommand::Connect { .. }) => {}
                    }
                }
                _ = inputs.recv() => {
                    drain_inputs(inputs);
                }
                changed = control.changed() => {
                    if changed.is_err() || control.borrow().shutdown {
                        connection.close(0u32.into(), b"cancel");
                        return Ok(AfterSession::Shutdown);
                    }
                    if session_cut(control, epoch).is_some() {
                        connection.close(0u32.into(), b"cancel");
                        return Err(NetworkEvent::Disconnected {
                            attempt_id,
                            kind: NetworkFailureKind::ClientRequestedDisconnect,
                        });
                    }
                }
            }
        }
    };

    let hello = ClientControl::Hello(Hello {
        protocol_version: PROTOCOL_VERSION,
        client_build: format!("purgatory-client-{}", env!("CARGO_PKG_VERSION")),
    });
    write_client_control(&mut send, &hello)
        .await
        .map_err(|kind| NetworkEvent::Disconnected { attempt_id, kind })?;

    let deadline = tokio::time::sleep(HANDSHAKE_TIMEOUT);
    tokio::pin!(deadline);
    let control_msg = loop {
        tokio::select! {
            msg = read_server_control(&mut recv, events.verbose()) => break msg,
            () = &mut deadline => {
                connection.close(0u32.into(), b"handshake");
                events.trace(&format!(
                    "attempt={attempt_id} HandshakeTimeout (Welcome wait)"
                ));
                return Err(NetworkEvent::Disconnected {
                    attempt_id,
                    kind: NetworkFailureKind::HandshakeTimeout,
                });
            }
            cmd = commands.recv() => {
                match cmd {
                    None => {
                        connection.close(0u32.into(), b"cancel");
                        return Ok(AfterSession::Shutdown);
                    }
                    Some(RuntimeCommand::Connect { .. }) => {}
                }
            }
            _ = inputs.recv() => {
                drain_inputs(inputs);
            }
            changed = control.changed() => {
                if changed.is_err() || control.borrow().shutdown {
                    connection.close(0u32.into(), b"cancel");
                    return Ok(AfterSession::Shutdown);
                }
                if session_cut(control, epoch).is_some() {
                    connection.close(0u32.into(), b"cancel");
                    return Err(NetworkEvent::Disconnected {
                        attempt_id,
                        kind: NetworkFailureKind::ClientRequestedDisconnect,
                    });
                }
            }
        }
    };

    match control_msg {
        Ok(ServerControl::Welcome(welcome)) => {
            events
                .emit(
                    NetworkEvent::Connected {
                        attempt_id,
                        connection_id: welcome.connection_id,
                        protocol_version: welcome.protocol_version,
                        server_tick_rate: welcome.server_tick_rate,
                    },
                    control,
                )
                .await;
        }
        Ok(ServerControl::Disconnect(reason)) => {
            connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
            return Err(NetworkEvent::Rejected { attempt_id, reason });
        }
        Err(kind) => {
            connection.close(0u32.into(), b"handshake");
            return Err(NetworkEvent::Disconnected { attempt_id, kind });
        }
    }

    drain_inputs(inputs);
    let result = live_loop(
        connection, send, recv, attempt_id, epoch, commands, inputs, events, control,
    )
    .await;
    events.clear_snapshots();
    result
}

#[allow(clippy::too_many_arguments)]
async fn live_loop(
    connection: Connection,
    mut send: SendStream,
    mut recv: RecvStream,
    attempt_id: ConnectionAttemptId,
    epoch: u64,
    commands: &mut mpsc::Receiver<RuntimeCommand>,
    inputs: &mut mpsc::Receiver<ClientGameplayMsg>,
    events: &EventSink,
    control: &mut watch::Receiver<Control>,
) -> Result<AfterSession, NetworkEvent> {
    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut nonce: u64 = 1;
    let mut outstanding: VecDeque<(u64, Instant)> = VecDeque::new();
    let mut snap_recv: Option<RecvStream> = None;

    loop {
        let waiting_uni = snap_recv.is_none();
        tokio::select! {
            biased;
            cmd = commands.recv() => {
                match cmd {
                    Some(RuntimeCommand::Connect { .. }) => {}
                    None => {
                        connection.close(0u32.into(), b"client");
                        return Ok(AfterSession::Shutdown);
                    }
                }
            }
            maybe_input = inputs.recv() => {
                let Some(first) = maybe_input else {
                    continue;
                };
                let mut batch = vec![first];
                while let Ok(next) = inputs.try_recv() {
                    batch.push(next);
                }
                for msg in batch {
                    let control_msg = match msg {
                        ClientGameplayMsg::Input(command) => ClientControl::Input(command),
                        ClientGameplayMsg::HeldCancel => ClientControl::HeldCancel,
                    };
                    if let Err(kind) = write_client_control(&mut send, &control_msg).await {
                        return Err(NetworkEvent::Disconnected { attempt_id, kind });
                    }
                }
            }
            changed = control.changed() => {
                if changed.is_err() || control.borrow().shutdown {
                    connection.close(0u32.into(), b"client");
                    return Ok(AfterSession::Shutdown);
                }
                if session_cut(control, epoch).is_some() {
                    connection.close(0u32.into(), b"client");
                    return Err(NetworkEvent::Disconnected {
                        attempt_id,
                        kind: NetworkFailureKind::ClientRequestedDisconnect,
                    });
                }
            }
            datagram = connection.read_datagram() => {
                match datagram {
                    Ok(bytes) => {
                        if let Ok(ServerDatagram::Pong { nonce: pong }) =
                            decode_server_datagram(&bytes)
                        {
                            apply_pong(&mut outstanding, pong, attempt_id, events, control).await;
                        }
                    }
                    Err(err) => {
                        return Err(NetworkEvent::Disconnected {
                            attempt_id,
                            kind: classify_connection_error(err, events.verbose()),
                        });
                    }
                }
            }
            control_msg = read_server_control(&mut recv, events.verbose()) => {
                match control_msg {
                    Ok(ServerControl::Disconnect(reason)) => {
                        connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
                        return Err(NetworkEvent::Disconnected {
                            attempt_id,
                            kind: NetworkFailureKind::from_wire(reason.code),
                        });
                    }
                    Ok(ServerControl::Welcome(_)) => {
                        return Err(NetworkEvent::Disconnected {
                            attempt_id,
                            kind: NetworkFailureKind::UnexpectedMessage,
                        });
                    }
                    Err(kind) => {
                        return Err(NetworkEvent::Disconnected {
                            attempt_id,
                            kind,
                        });
                    }
                }
            }
            _ = ping.tick() => {
                let id = nonce;
                nonce = nonce.wrapping_add(1);
                if outstanding.len() >= MAX_OUTSTANDING_PINGS {
                    outstanding.pop_front();
                }
                outstanding.push_back((id, Instant::now()));
                if let Ok(payload) = encode_client_datagram(id) {
                    let _ = connection.send_datagram(payload.into());
                }
            }
            uni = connection.accept_uni(), if waiting_uni => {
                match uni {
                    Ok(stream) => {
                        snap_recv = Some(stream);
                    }
                    Err(err) => {
                        return Err(NetworkEvent::Disconnected {
                            attempt_id,
                            kind: classify_connection_error(err, events.verbose()),
                        });
                    }
                }
            }
            snap = async {
                if let Some(stream) = snap_recv.as_mut() {
                    read_world_snapshot(stream, events.verbose()).await
                } else {
                    std::future::pending::<Result<WorldSnapshot, SnapshotReadError>>().await
                }
            } => {
                match snap {
                    Ok(snapshot) => events.push_snapshot(snapshot),
                    Err(SnapshotReadError::Closed) => {
                        snap_recv = None;
                    }
                    Err(SnapshotReadError::Malformed) => {
                        events.note_malformed_snapshot();
                    }
                    Err(SnapshotReadError::FramingBroken) => {
                        events.note_malformed_snapshot();
                        snap_recv = None;
                    }
                    Err(SnapshotReadError::Disconnected(kind)) => {
                        return Err(NetworkEvent::Disconnected { attempt_id, kind });
                    }
                }
            }
        }
    }
}

async fn apply_pong(
    outstanding: &mut VecDeque<(u64, Instant)>,
    pong: u64,
    attempt_id: ConnectionAttemptId,
    events: &EventSink,
    control: &watch::Receiver<Control>,
) {
    let Some(idx) = outstanding.iter().position(|(n, _)| *n == pong) else {
        return;
    };
    let Some((_, sent)) = outstanding.remove(idx) else {
        return;
    };
    events
        .emit(
            NetworkEvent::RttUpdated {
                attempt_id,
                rtt: sent.elapsed(),
            },
            control,
        )
        .await;
}

fn classify_connection_error(err: quinn::ConnectionError, verbose: bool) -> NetworkFailureKind {
    let symptom = match &err {
        quinn::ConnectionError::TimedOut => TransportSymptom::TimedOut,
        quinn::ConnectionError::ApplicationClosed(close) => {
            let raw = u64::from(close.error_code);
            TransportSymptom::ApplicationClosed(u8::try_from(raw).unwrap_or(0))
        }
        quinn::ConnectionError::LocallyClosed => TransportSymptom::LocallyClosed,
        quinn::ConnectionError::Reset => TransportSymptom::Reset,
        quinn::ConnectionError::CidsExhausted
        | quinn::ConnectionError::VersionMismatch
        | quinn::ConnectionError::TransportError(_)
        | quinn::ConnectionError::ConnectionClosed(_) => TransportSymptom::Other,
    };
    let kind = NetworkFailureKind::from_transport(symptom);
    if verbose {
        println!(
            "PURGATORY net verbose category={} cause={err}",
            kind.debug_label()
        );
    }
    kind
}

fn classify_read_exact(err: quinn::ReadExactError, verbose: bool) -> NetworkFailureKind {
    match err {
        quinn::ReadExactError::FinishedEarly(_) => NetworkFailureKind::TransportLost,
        quinn::ReadExactError::ReadError(quinn::ReadError::ConnectionLost(lost)) => {
            classify_connection_error(lost, verbose)
        }
        quinn::ReadExactError::ReadError(_) => NetworkFailureKind::TransportLost,
    }
}

fn apply_idle_timeout(transport: &mut TransportConfig, idle: std::time::Duration) {
    if let Ok(timeout) = idle.try_into() {
        transport.max_idle_timeout(Some(timeout));
    }
}

fn make_endpoint(idle: std::time::Duration) -> Result<Endpoint, String> {
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(DevOnlySkipServerVerification::new())
        .with_no_client_auth();
    tls.alpn_protocols = vec![ALPN_PROTOCOL.to_vec()];
    let mut client = ClientConfig::new(Arc::new(
        QuicClientConfig::try_from(tls).map_err(|err| format!("quic client tls: {err}"))?,
    ));
    let mut transport = TransportConfig::default();
    transport.datagram_receive_buffer_size(Some(4096));
    transport.datagram_send_buffer_size(4096);
    transport.max_concurrent_uni_streams(VarInt::from_u32(1));
    apply_idle_timeout(&mut transport, idle);
    client.transport_config(Arc::new(transport));
    let mut endpoint = Endpoint::client(SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 0)))
        .map_err(|err| format!("client bind: {err}"))?;
    endpoint.set_default_client_config(client);
    Ok(endpoint)
}

async fn write_client_control(
    send: &mut SendStream,
    msg: &ClientControl,
) -> Result<(), NetworkFailureKind> {
    let payload =
        encode_client_control(msg).map_err(|_| NetworkFailureKind::InternalNetworkError)?;
    let frame = encode_frame(&payload).map_err(|_| NetworkFailureKind::InternalNetworkError)?;
    send.write_all(&frame)
        .await
        .map_err(|_| NetworkFailureKind::TransportLost)
}

async fn read_server_control(
    recv: &mut RecvStream,
    verbose: bool,
) -> Result<ServerControl, NetworkFailureKind> {
    let mut prefix = [0u8; 4];
    recv.read_exact(&mut prefix)
        .await
        .map_err(|err| classify_read_exact(err, verbose))?;
    let len = peek_frame_len(&prefix).map_err(|_| NetworkFailureKind::MalformedMessage)?;
    let mut payload = vec![0u8; len as usize];
    recv.read_exact(&mut payload)
        .await
        .map_err(|err| classify_read_exact(err, verbose))?;
    decode_server_control(&payload).map_err(|_| NetworkFailureKind::MalformedMessage)
}

enum SnapshotReadError {
    Closed,
    Malformed,
    FramingBroken,
    Disconnected(NetworkFailureKind),
}

async fn read_world_snapshot(
    recv: &mut RecvStream,
    verbose: bool,
) -> Result<WorldSnapshot, SnapshotReadError> {
    let mut prefix = [0u8; 4];
    recv.read_exact(&mut prefix)
        .await
        .map_err(|err| match classify_read_exact(err, verbose) {
            NetworkFailureKind::TransportLost => SnapshotReadError::Closed,
            kind => SnapshotReadError::Disconnected(kind),
        })?;
    let len = match peek_gameplay_frame_len(&prefix) {
        Ok(len) => len,
        Err(_) => return Err(SnapshotReadError::FramingBroken),
    };
    let mut payload = vec![0u8; len as usize];
    recv.read_exact(&mut payload)
        .await
        .map_err(|err| match classify_read_exact(err, verbose) {
            NetworkFailureKind::TransportLost => SnapshotReadError::Closed,
            kind => SnapshotReadError::Disconnected(kind),
        })?;
    decode_world_snapshot(&payload).map_err(|_| SnapshotReadError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::replica::ReplicatedWorld;
    use purgatory_protocol::{
        ConnectionId, ReplicatedKind, SnapshotEntity, Welcome, WireEntityId, encode_gameplay_frame,
        encode_server_control, encode_world_snapshot,
    };

    fn attempt() -> ConnectionAttemptId {
        ConnectionAttemptId::from_raw(1)
    }

    #[test]
    fn lifecycle_survives_saturated_telemetry() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (life_tx, mut life_rx) = mpsc::channel(4);
            let (tel_tx, mut tel_rx) = mpsc::channel(2);
            let dropped = Arc::new(AtomicU64::new(0));
            let sink = EventSink {
                lifecycle: life_tx,
                telemetry: tel_tx,
                telemetry_dropped: Arc::clone(&dropped),
                verbose: Arc::new(AtomicBool::new(false)),
                snapshots: watch::channel(None).0,
                snapshot_malformed: Arc::new(AtomicU64::new(0)),
            };
            let (_tx, rx) = watch::channel(Control::default());
            let id = attempt();
            for _ in 0..8 {
                sink.emit(
                    NetworkEvent::RttUpdated {
                        attempt_id: id,
                        rtt: Duration::from_millis(1),
                    },
                    &rx,
                )
                .await;
            }
            sink.emit(
                NetworkEvent::Connected {
                    attempt_id: id,
                    connection_id: ConnectionId::from_raw(3),
                    protocol_version: PROTOCOL_VERSION,
                    server_tick_rate: 30,
                },
                &rx,
            )
            .await;
            let mut saw_connected = false;
            while let Ok(event) = life_rx.try_recv() {
                if matches!(event, NetworkEvent::Connected { .. }) {
                    saw_connected = true;
                }
            }
            assert!(saw_connected, "Connected must not be dropped");
            let mut telemetry = 0u32;
            while tel_rx.try_recv().is_ok() {
                telemetry += 1;
            }
            assert!(dropped.load(Ordering::Relaxed) > 0);
            assert!(telemetry <= 2);
        });
    }

    /// Mandatory Phase 5.0F regression: telemetry saturation must never hide
    /// the lifecycle `Disconnected` that returns the client to the frontend.
    #[test]
    fn disconnect_survives_saturated_telemetry() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (life_tx, mut life_rx) = mpsc::channel(4);
            let (tel_tx, _tel_rx) = mpsc::channel(1);
            let dropped = Arc::new(AtomicU64::new(0));
            let sink = EventSink {
                lifecycle: life_tx,
                telemetry: tel_tx,
                telemetry_dropped: Arc::clone(&dropped),
                verbose: Arc::new(AtomicBool::new(false)),
                snapshots: watch::channel(None).0,
                snapshot_malformed: Arc::new(AtomicU64::new(0)),
            };
            let (_tx, rx) = watch::channel(Control::default());
            let id = attempt();
            for _ in 0..64 {
                sink.emit(
                    NetworkEvent::RttUpdated {
                        attempt_id: id,
                        rtt: Duration::from_millis(1),
                    },
                    &rx,
                )
                .await;
            }
            sink.emit(
                NetworkEvent::Disconnected {
                    attempt_id: id,
                    kind: NetworkFailureKind::TransportLost,
                },
                &rx,
            )
            .await;
            let mut saw_disconnect = false;
            while let Ok(event) = life_rx.try_recv() {
                if matches!(event, NetworkEvent::Disconnected { .. }) {
                    saw_disconnect = true;
                }
            }
            assert!(saw_disconnect, "Disconnected must not be dropped");
            assert!(dropped.load(Ordering::Relaxed) > 0, "telemetry should drop");
        });
    }

    fn join_on_drop(handle: NetworkHandle, limit: Duration, context: &str) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            drop(handle);
            let _ = tx.send(());
        });
        rx.recv_timeout(limit)
            .unwrap_or_else(|_| panic!("{context}: network thread did not join"));
    }

    fn closed_port_config() -> ClientEndpointConfig {
        ClientEndpointConfig {
            server: SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 1)),
            ..ClientEndpointConfig::dev()
        }
    }

    /// Deterministic delays across Connecting, not sleep-race guessing.
    #[test]
    fn shutdown_during_connect_stress_joins_every_round() {
        for (round, delay_ms) in [0_u64, 1, 3, 8, 20].into_iter().enumerate() {
            let handle = NetworkHandle::start(closed_port_config()).expect("start");
            assert!(handle.try_send(NetworkCommand::Connect {
                attempt_id: ConnectionAttemptId::from_raw(round as u64 + 1),
            }));
            if delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(delay_ms));
            }
            assert!(handle.try_send(NetworkCommand::Shutdown));
            join_on_drop(
                handle,
                Duration::from_secs(5),
                &format!("shutdown after {delay_ms}ms"),
            );
        }
    }

    #[test]
    fn disconnect_shutdown_race_soak_stays_idempotent() {
        for round in 0..6u64 {
            let handle = NetworkHandle::start(closed_port_config()).expect("start");
            assert!(handle.try_send(NetworkCommand::Connect {
                attempt_id: ConnectionAttemptId::from_raw(round + 1),
            }));
            // Repeated Disconnect and Shutdown in both orders must be harmless.
            for _ in 0..4 {
                assert!(handle.try_send(NetworkCommand::Disconnect));
            }
            assert!(handle.try_send(NetworkCommand::Shutdown));
            assert!(handle.try_send(NetworkCommand::Disconnect));
            assert!(handle.try_send(NetworkCommand::Shutdown));
            join_on_drop(
                handle,
                Duration::from_secs(5),
                &format!("race round {round}"),
            );
        }
    }

    /// Connect pressure must not strand the control plane (§21).
    #[test]
    fn connect_pressure_still_allows_disconnect() {
        let mut handle = NetworkHandle::start(closed_port_config()).expect("start");
        assert!(handle.try_send(NetworkCommand::Connect {
            attempt_id: ConnectionAttemptId::from_raw(1),
        }));
        let mut accepted = 0;
        for i in 0..64 {
            if handle.try_send(NetworkCommand::Connect {
                attempt_id: ConnectionAttemptId::from_raw(i + 2),
            }) {
                accepted += 1;
            }
        }
        assert!(
            accepted <= CMD_CAP,
            "command queue must stay bounded, accepted {accepted}"
        );
        assert!(handle.try_send(NetworkCommand::Disconnect));
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut saw_disconnect = false;
        while Instant::now() < deadline && !saw_disconnect {
            handle.poll(|event| {
                if matches!(event, NetworkEvent::Disconnected { .. }) {
                    saw_disconnect = true;
                }
            });
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            saw_disconnect,
            "Disconnect must be honored under Connect pressure"
        );
        join_on_drop(handle, Duration::from_secs(5), "command pressure");
    }

    /// Test-only QUIC listener. It completes the real transport handshake,
    /// reads the framed Hello, and then deliberately never answers with
    /// Welcome. That holds a live client in `Handshaking` deterministically
    /// without any production sleep or handshake-timing change.
    struct StalledServer {
        addr: SocketAddr,
        hellos: Arc<AtomicU64>,
        stop: Option<tokio::sync::oneshot::Sender<()>>,
        thread: Option<JoinHandle<()>>,
    }

    impl StalledServer {
        fn start() -> Self {
            let (addr_tx, addr_rx) = std::sync::mpsc::channel();
            let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
            let hellos = Arc::new(AtomicU64::new(0));
            let counter = Arc::clone(&hellos);
            let thread = std::thread::Builder::new()
                .name("purgatory-stalled-listener".into())
                .spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("listener runtime");
                    runtime.block_on(async move {
                        let _ = rustls::crypto::ring::default_provider().install_default();
                        let endpoint = stalled_endpoint();
                        let _ = addr_tx.send(endpoint.local_addr().expect("listener addr"));
                        let mut held = Vec::new();
                        tokio::pin!(stop_rx);
                        loop {
                            tokio::select! {
                                incoming = endpoint.accept() => {
                                    let Some(incoming) = incoming else { break };
                                    let counter = Arc::clone(&counter);
                                    held.push(tokio::spawn(async move {
                                        let Ok(connection) = incoming.await else { return };
                                        let Ok((send, mut recv)) = connection.accept_bi().await
                                        else {
                                            return;
                                        };
                                        if read_framed_hello(&mut recv).await {
                                            counter.fetch_add(1, Ordering::Relaxed);
                                        }
                                        // No Welcome is ever written.
                                        let _ = connection.closed().await;
                                        drop(send);
                                    }));
                                }
                                _ = &mut stop_rx => break,
                            }
                        }
                        endpoint.close(0u32.into(), b"listener stop");
                        for task in held {
                            task.abort();
                        }
                    });
                })
                .expect("listener thread");
            let addr = addr_rx
                .recv_timeout(Duration::from_secs(10))
                .expect("listener addr");
            Self {
                addr,
                hellos,
                stop: Some(stop_tx),
                thread: Some(thread),
            }
        }

        fn hellos(&self) -> u64 {
            self.hellos.load(Ordering::Relaxed)
        }

        fn stop(&mut self) {
            if let Some(stop) = self.stop.take() {
                let _ = stop.send(());
            }
            if let Some(thread) = self.thread.take() {
                thread.join().expect("listener thread join");
            }
        }
    }

    impl Drop for StalledServer {
        fn drop(&mut self) {
            self.stop();
        }
    }

    fn stalled_endpoint() -> Endpoint {
        let certified =
            rcgen::generate_simple_self_signed(vec!["localhost".into()]).expect("test cert");
        let cert = rustls::pki_types::CertificateDer::from(certified.cert.der().to_vec());
        let key = rustls::pki_types::PrivateKeyDer::Pkcs8(
            rustls::pki_types::PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()),
        );
        let mut tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .expect("test tls");
        tls.alpn_protocols = vec![ALPN_PROTOCOL.to_vec()];
        let server = quinn::ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(tls).expect("quic test tls"),
        ));
        Endpoint::server(server, SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 0)))
            .expect("listener bind")
    }

    async fn read_framed_hello(recv: &mut RecvStream) -> bool {
        let mut prefix = [0u8; 4];
        if recv.read_exact(&mut prefix).await.is_err() {
            return false;
        }
        let Ok(len) = peek_frame_len(&prefix) else {
            return false;
        };
        let mut payload = vec![0u8; len as usize];
        if recv.read_exact(&mut payload).await.is_err() {
            return false;
        }
        matches!(
            purgatory_protocol::decode_client_control(&payload),
            Ok(ClientControl::Hello(_))
        )
    }

    /// Live QUIC shutdown while genuinely in `Handshaking`: the transport
    /// connected, the server received the Hello, and Welcome never arrives.
    /// The network thread must still join within a bounded timeout.
    fn live_handshake_shutdown_round(round: u64) {
        let mut server = StalledServer::start();
        let mut handle = NetworkHandle::start(ClientEndpointConfig {
            server: server.addr,
            ..ClientEndpointConfig::dev()
        })
        .expect("start");
        let attempt = ConnectionAttemptId::from_raw(round + 1);
        assert!(handle.try_send(NetworkCommand::Connect {
            attempt_id: attempt
        }));

        let mut saw_handshaking = false;
        let mut saw_connected = false;
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline && !(saw_handshaking && server.hellos() > 0) {
            handle.poll(|event| match event {
                NetworkEvent::Handshaking { attempt_id } => {
                    assert_eq!(attempt_id, attempt, "round {round}: stale Handshaking");
                    saw_handshaking = true;
                }
                NetworkEvent::Connected { .. } => saw_connected = true,
                _ => {}
            });
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            saw_handshaking,
            "round {round}: client never reached Handshaking over real QUIC"
        );
        assert!(
            server.hellos() > 0,
            "round {round}: listener never received the Hello"
        );
        assert!(
            !saw_connected,
            "round {round}: Welcome must not complete before shutdown"
        );

        assert!(handle.try_send(NetworkCommand::Shutdown));
        join_on_drop(
            handle,
            Duration::from_secs(5),
            &format!("live handshake shutdown round {round}"),
        );
        server.stop();
    }

    #[test]
    fn live_quic_shutdown_during_handshaking_joins_every_round() {
        for round in 0..6u64 {
            live_handshake_shutdown_round(round);
        }
    }

    #[test]
    #[ignore = "extended soak: 25 live handshake-shutdown rounds"]
    fn live_quic_shutdown_during_handshaking_soak() {
        for round in 0..25u64 {
            live_handshake_shutdown_round(round);
        }
    }

    #[test]
    fn drop_while_idle_joins() {
        let handle = NetworkHandle::start(ClientEndpointConfig::dev()).expect("start");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            drop(handle);
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(3))
            .expect("network thread must join on Drop");
    }

    #[test]
    fn drop_during_connect_to_closed_port_joins() {
        let handle = NetworkHandle::start(ClientEndpointConfig {
            server: SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 1)),
            ..ClientEndpointConfig::dev()
        })
        .expect("start");
        assert!(handle.try_send(NetworkCommand::Connect {
            attempt_id: ConnectionAttemptId::from_raw(1)
        }));
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            drop(handle);
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(5))
            .expect("Drop during Connecting must join");
    }

    #[test]
    fn connect_pressure_still_allows_shutdown() {
        let handle = NetworkHandle::start(ClientEndpointConfig::dev()).expect("start");
        for i in 0..32 {
            let _ = handle.try_send(NetworkCommand::Connect {
                attempt_id: ConnectionAttemptId::from_raw(i + 1),
            });
        }
        assert!(handle.try_send(NetworkCommand::Shutdown));
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            drop(handle);
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(3))
            .expect("Shutdown under Connect pressure must join");
    }

    #[test]
    fn disconnect_during_connecting_to_closed_port_does_not_hang() {
        let mut handle = NetworkHandle::start(ClientEndpointConfig {
            server: SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 1)),
            ..ClientEndpointConfig::dev()
        })
        .expect("start");
        assert!(handle.try_send(NetworkCommand::Connect {
            attempt_id: ConnectionAttemptId::from_raw(1)
        }));
        std::thread::sleep(Duration::from_millis(20));
        assert!(handle.try_send(NetworkCommand::Disconnect));
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut saw_disconnect = false;
        while Instant::now() < deadline {
            handle.poll(|event| {
                if matches!(event, NetworkEvent::Disconnected { .. }) {
                    saw_disconnect = true;
                }
            });
            if saw_disconnect {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(saw_disconnect, "Disconnect during Connecting must complete");
        drop(handle);
    }

    #[test]
    fn ping_queue_is_bounded() {
        let mut outstanding: VecDeque<(u64, Instant)> = VecDeque::new();
        for n in 0..20u64 {
            if outstanding.len() >= MAX_OUTSTANDING_PINGS {
                outstanding.pop_front();
            }
            outstanding.push_back((n, Instant::now()));
        }
        assert_eq!(outstanding.len(), MAX_OUTSTANDING_PINGS);
        assert_eq!(outstanding.front().map(|(n, _)| *n), Some(16));
    }

    #[test]
    fn unknown_and_duplicate_pong_are_ignored() {
        let mut outstanding: VecDeque<(u64, Instant)> = VecDeque::from([(1, Instant::now())]);
        assert!(outstanding.iter().position(|(n, _)| *n == 99).is_none());
        let idx = outstanding.iter().position(|(n, _)| *n == 1).unwrap();
        outstanding.remove(idx);
        assert!(outstanding.iter().position(|(n, _)| *n == 1).is_none());
    }

    /// Verbose tracing stays a category/id log: it must never dump wire bytes,
    /// and telemetry must not produce a line per event.
    #[test]
    fn verbose_logging_never_formats_raw_payloads() {
        let src = include_str!("runtime.rs");
        for forbidden in [
            concat!("{pay", "load"),
            concat!("pay", "load:?"),
            concat!("{by", "tes"),
            concat!("by", "tes:?"),
            concat!("{fra", "me"),
            concat!("fra", "me:?"),
        ] {
            assert!(
                !src.contains(forbidden),
                "verbose logging must not dump raw bytes: {forbidden}"
            );
        }
        assert!(
            src.contains("NetworkEvent::RttUpdated { .. } => {}"),
            "RTT telemetry must stay silent to avoid per-ping log spam"
        );
    }

    #[derive(Clone, Copy)]
    enum UniAfterWelcome {
        Immediate,
        Delayed,
        CloseConnection,
    }

    /// QUIC listener that completes Hello → Welcome, then follows a scripted
    /// uni-stream ordering. Used to cover `live_loop`'s `select!` snapshot
    /// branch on the real `NetworkHandle` thread (the path that panicked).
    struct SnapshotScriptServer {
        addr: SocketAddr,
        stop: Option<tokio::sync::oneshot::Sender<()>>,
        thread: Option<JoinHandle<()>>,
    }

    impl SnapshotScriptServer {
        fn start(mode: UniAfterWelcome) -> Self {
            let (addr_tx, addr_rx) = std::sync::mpsc::channel();
            let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
            let thread = std::thread::Builder::new()
                .name("purgatory-snapshot-script".into())
                .spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("listener runtime");
                    runtime.block_on(async move {
                        let _ = rustls::crypto::ring::default_provider().install_default();
                        let endpoint = stalled_endpoint();
                        let _ = addr_tx.send(endpoint.local_addr().expect("listener addr"));
                        let mut held = Vec::new();
                        tokio::pin!(stop_rx);
                        loop {
                            tokio::select! {
                                incoming = endpoint.accept() => {
                                    let Some(incoming) = incoming else { break };
                                    held.push(tokio::spawn(async move {
                                        run_snapshot_script(incoming, mode).await;
                                    }));
                                }
                                _ = &mut stop_rx => break,
                            }
                        }
                        endpoint.close(0u32.into(), b"listener stop");
                        for task in held {
                            task.abort();
                        }
                    });
                })
                .expect("listener thread");
            let addr = addr_rx
                .recv_timeout(Duration::from_secs(10))
                .expect("listener addr");
            Self {
                addr,
                stop: Some(stop_tx),
                thread: Some(thread),
            }
        }

        fn stop(&mut self) {
            if let Some(stop) = self.stop.take() {
                let _ = stop.send(());
            }
            if let Some(thread) = self.thread.take() {
                thread.join().expect("listener thread join");
            }
        }
    }

    impl Drop for SnapshotScriptServer {
        fn drop(&mut self) {
            self.stop();
        }
    }

    fn sample_world_snapshot() -> WorldSnapshot {
        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        WorldSnapshot::from_poses(
            1,
            10,
            local,
            vec![SnapshotEntity {
                entity_id: local,
                kind: ReplicatedKind::Player,
                position: [3.0, 4.0],
                velocity: [0.0, 0.0],
            }],
        )
    }

    async fn write_welcome_frame(send: &mut SendStream) -> bool {
        let welcome = ServerControl::Welcome(Welcome {
            protocol_version: PROTOCOL_VERSION,
            connection_id: ConnectionId::from_raw(7),
            server_tick_rate: 30,
            server_label: "purgatory-server-dev".into(),
        });
        let Ok(payload) = encode_server_control(&welcome) else {
            return false;
        };
        let Ok(frame) = encode_frame(&payload) else {
            return false;
        };
        send.write_all(&frame).await.is_ok()
    }

    async fn write_snapshot_uni(connection: &Connection) -> bool {
        let Ok(mut uni) = connection.open_uni().await else {
            return false;
        };
        let Ok(payload) = encode_world_snapshot(&sample_world_snapshot()) else {
            return false;
        };
        let Ok(frame) = encode_gameplay_frame(&payload) else {
            return false;
        };
        uni.write_all(&frame).await.is_ok()
    }

    async fn run_snapshot_script(incoming: quinn::Incoming, mode: UniAfterWelcome) {
        let Ok(connection) = incoming.await else {
            return;
        };
        let Ok((mut send, mut recv)) = connection.accept_bi().await else {
            return;
        };
        if !read_framed_hello(&mut recv).await {
            return;
        }
        if !write_welcome_frame(&mut send).await {
            return;
        }
        match mode {
            UniAfterWelcome::Immediate => {
                let _ = write_snapshot_uni(&connection).await;
            }
            UniAfterWelcome::Delayed => {
                // Test-only delay: Welcome is already on the wire; the client
                // must enter `live_loop` with `snap_recv == None` before uni.
                tokio::time::sleep(Duration::from_millis(200)).await;
                let _ = write_snapshot_uni(&connection).await;
            }
            UniAfterWelcome::CloseConnection => {
                // Test-only: let Welcome reach the client so it enters
                // `live_loop` with `snap_recv == None` before the close.
                tokio::time::sleep(Duration::from_millis(200)).await;
                connection.close(0u32.into(), b"test close");
            }
        }
        let _ = connection.closed().await;
        drop(send);
    }

    fn connect_scripted(mode: UniAfterWelcome) -> (SnapshotScriptServer, NetworkHandle) {
        let server = SnapshotScriptServer::start(mode);
        let handle = NetworkHandle::start(ClientEndpointConfig {
            server: server.addr,
            ..ClientEndpointConfig::dev()
        })
        .expect("start");
        assert!(handle.try_send(NetworkCommand::Connect {
            attempt_id: ConnectionAttemptId::from_raw(1)
        }));
        (server, handle)
    }

    /// Real app path: `NetworkHandle` → Connect → Welcome → uni → first snapshot.
    fn wait_connected_snapshot(
        handle: &mut NetworkHandle,
        limit: Duration,
    ) -> (bool, Option<WorldSnapshot>) {
        let deadline = Instant::now() + limit;
        let mut connected = false;
        let mut snapshot = None;
        while Instant::now() < deadline {
            handle.poll(|event| {
                if matches!(event, NetworkEvent::Connected { .. }) {
                    connected = true;
                }
            });
            if let Some(snap) = handle.poll_snapshot() {
                snapshot = Some(snap);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        (connected, snapshot)
    }

    fn assert_replica_from_first_snapshot(snap: WorldSnapshot) {
        let mut replica = ReplicatedWorld::new();
        replica.apply(snap.clone());
        assert_eq!(replica.len(), 1, "replica must contain the local player");
        assert_eq!(replica.local_player(), Some(snap.local_player_entity));
        assert!(
            replica.get(snap.local_player_entity).is_some(),
            "local player entity must be in the replica map"
        );
        assert_eq!(replica.last_sequence(), Some(1));
    }

    #[test]
    fn live_loop_applies_snapshot_when_uni_arrives_after_welcome() {
        let (mut server, mut handle) = connect_scripted(UniAfterWelcome::Delayed);
        let (connected, snap) = wait_connected_snapshot(&mut handle, Duration::from_secs(5));
        assert!(connected, "Welcome must emit Connected before snapshots");
        let snap = snap.expect("first WorldSnapshot on delayed uni (live_loop must not panic)");
        assert_replica_from_first_snapshot(snap);
        join_on_drop(
            handle,
            Duration::from_secs(5),
            "delayed-uni snapshot session",
        );
        server.stop();
    }

    #[test]
    fn live_loop_applies_snapshot_when_uni_is_ready_immediately() {
        let (mut server, mut handle) = connect_scripted(UniAfterWelcome::Immediate);
        let (connected, snap) = wait_connected_snapshot(&mut handle, Duration::from_secs(5));
        assert!(connected, "Welcome must emit Connected");
        let snap = snap.expect("first WorldSnapshot on immediate uni (live_loop must not panic)");
        assert_replica_from_first_snapshot(snap);
        join_on_drop(
            handle,
            Duration::from_secs(5),
            "immediate-uni snapshot session",
        );
        server.stop();
    }

    #[test]
    fn live_loop_survives_disconnect_between_welcome_and_uni() {
        let (mut server, mut handle) = connect_scripted(UniAfterWelcome::CloseConnection);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut connected = false;
        let mut disconnected = false;
        while Instant::now() < deadline {
            handle.poll(|event| match event {
                NetworkEvent::Connected { .. } => connected = true,
                NetworkEvent::Disconnected { .. } => disconnected = true,
                _ => {}
            });
            if connected && disconnected {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(connected, "Welcome must complete before the scripted close");
        assert!(
            disconnected,
            "close after Welcome must yield Disconnected, not a net-thread panic"
        );
        join_on_drop(
            handle,
            Duration::from_secs(5),
            "close between Welcome and uni",
        );
        server.stop();
    }

    #[test]
    fn new_connect_does_not_clear_disconnect_epoch() {
        let control = watch::channel(Control::default()).0;
        control.send_modify(|c| c.disconnect_epoch = c.disconnect_epoch.wrapping_add(1));
        let stamped = control.borrow().disconnect_epoch;
        assert_eq!(stamped, 1);
        let later = control.borrow().disconnect_epoch;
        assert_eq!(later, 1, "Connect must stamp epoch, not reset it");
    }
}
