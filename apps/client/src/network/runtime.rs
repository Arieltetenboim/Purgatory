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
use std::time::{Duration, Instant};

use quinn::crypto::rustls::QuicClientConfig;
use quinn::{ClientConfig, Connection, Endpoint, RecvStream, SendStream, TransportConfig, VarInt};
use tokio::sync::{mpsc, watch};

use purgatory_common::impairment::{
    EnqueueError, INPUT_DRAIN_PER_TURN, ImpairmentHarness, ImpairmentMetricsSnapshot,
    NetworkImpairmentConfig, SNAPSHOT_DRAIN_PER_TURN, duration_ns,
};

use purgatory_protocol::{
    ALPN_PROTOCOL, ClientControl, HANDSHAKE_TIMEOUT, Hello, InputCommand, PING_INTERVAL,
    PROTOCOL_VERSION, ReplicationFrame, ServerControl, ServerDatagram, decode_replication_frame,
    decode_server_control, decode_server_datagram, encode_client_control, encode_client_datagram,
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
const STALL_CAP: usize = 8;
const RESET_CAP: usize = 4;
/// Bounded replica ingress. The reader awaits when full so Enter/Leave cannot drop.
const FRAME_CAP: usize = 64;
/// Oldest outstanding ping is dropped when this many wait for a Pong.
const MAX_OUTSTANDING_PINGS: usize = 4;

fn ns_since(origin: Instant) -> u64 {
    u64::try_from(origin.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[derive(Clone, Copy, Debug, Default)]
struct Control {
    shutdown: bool,
    disconnect_epoch: u64,
}

enum RuntimeCommand {
    Connect {
        attempt_id: ConnectionAttemptId,
        epoch: u64,
        dev_login: String,
    },
}

/// Gameplay messages on the reliable control stream. Drained in order; never
/// coalesced. `HeldCancel` is not an `InputCommand` and has no sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClientGameplayMsg {
    Input(InputCommand),
    HeldCancel,
    InteractOpen(purgatory_protocol::WireEntityId),
    InteractClose(u32),
    PortalActivate(purgatory_protocol::WireEntityId),
    DevSetChannel(u32),
    DevSetSpeed(Option<u16>),
    DevSetJump(Option<u16>),
    #[allow(dead_code)]
    Equip(purgatory_protocol::EquipRequest),
    #[allow(dead_code)]
    Unequip(purgatory_protocol::UnequipRequest),
    #[allow(dead_code)]
    Pickup(purgatory_protocol::PickupRequest),
    DevPresentationOneShot(u8),
    DevResetPlayer,
    Respawn,
    AbilityActivate(purgatory_protocol::AbilityActivateRequest),
}

struct ImpairmentNet {
    config_rx: watch::Receiver<NetworkImpairmentConfig>,
    stall_rx: mpsc::Receiver<Duration>,
    reset_rx: mpsc::Receiver<()>,
    metrics_tx: watch::Sender<ImpairmentMetricsSnapshot>,
    stall_dropped: Arc<AtomicU64>,
    harness: ImpairmentHarness<ClientGameplayMsg, ReplicationFrame>,
    origin: Instant,
}

impl ImpairmentNet {
    fn now_ns(&self) -> u64 {
        ns_since(self.origin)
    }

    fn sync_control(&mut self) {
        if self.config_rx.has_changed().unwrap_or(false) {
            let cfg = *self.config_rx.borrow_and_update();
            self.harness.apply_config(cfg, self.now_ns());
        }
        while let Ok(dur) = self.stall_rx.try_recv() {
            self.harness
                .begin_input_stall(self.now_ns(), duration_ns(dur));
        }
        while self.reset_rx.try_recv().is_ok() {
            self.harness.reset_metrics();
        }
        self.harness.tick_auto_stall(self.now_ns());
        self.publish_metrics();
    }

    fn publish_metrics(&self) {
        let mut snap = self.harness.metrics(self.now_ns());
        snap.stall_trigger_dropped = self.stall_dropped.load(Ordering::Relaxed);
        let _ = self.metrics_tx.send(snap);
    }

    fn enqueue_input(&mut self, msg: ClientGameplayMsg) -> Result<(), EnqueueError> {
        let now = self.now_ns();
        self.harness.input_mut().enqueue(msg, now)
    }

    fn should_queue_input(&self) -> bool {
        let now = self.now_ns();
        self.harness.input().should_delay(now) || !self.harness.input().is_empty()
    }

    fn should_queue_snapshot(&self) -> bool {
        let now = self.now_ns();
        self.harness.snapshot().should_delay(now) || !self.harness.snapshot().is_empty()
    }

    fn enqueue_snapshot(&mut self, snap: ReplicationFrame) -> Result<(), EnqueueError> {
        let now = self.now_ns();
        self.harness.snapshot_mut().enqueue(snap, now)
    }
}

struct EventSink {
    lifecycle: mpsc::Sender<NetworkEvent>,
    telemetry: mpsc::Sender<NetworkEvent>,
    telemetry_dropped: Arc<AtomicU64>,
    verbose: Arc<AtomicBool>,
    frames: mpsc::Sender<ReplicationFrame>,
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
                NetworkEvent::Interact { attempt_id, event } => {
                    self.trace(&format!("attempt={attempt_id} Interact {event:?}"));
                }
                NetworkEvent::Equipment { attempt_id, event } => {
                    self.trace(&format!("attempt={attempt_id} Equipment {event:?}"));
                }
                NetworkEvent::Item { attempt_id, event } => {
                    self.trace(&format!("attempt={attempt_id} Item {event:?}"));
                }
                NetworkEvent::Inventory { attempt_id, event } => {
                    self.trace(&format!("attempt={attempt_id} Inventory {event:?}"));
                }
                NetworkEvent::PresentationOneShot { attempt_id, event } => {
                    self.trace(&format!(
                        "attempt={attempt_id} PresentationOneShot {event:?}"
                    ));
                }
                NetworkEvent::Ability { attempt_id, event } => {
                    self.trace(&format!("attempt={attempt_id} Ability {event:?}"));
                }
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

    async fn push_frame(&self, frame: ReplicationFrame) {
        if self.verbose() {
            self.trace(&format!(
                "replication frame seq={} tick={} epoch={} records={}",
                frame.snapshot_sequence,
                frame.server_tick,
                frame.observer_baseline_epoch,
                frame.records.len()
            ));
        }
        let _ = self.frames.send(frame).await;
    }

    fn note_malformed_snapshot(&self) {
        self.snapshot_malformed.fetch_add(1, Ordering::Relaxed);
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
    frames: mpsc::Receiver<ReplicationFrame>,
    snapshot_malformed: Arc<AtomicU64>,
    impairment_config: watch::Sender<NetworkImpairmentConfig>,
    stall_tx: mpsc::Sender<Duration>,
    reset_metrics_tx: mpsc::Sender<()>,
    impairment_metrics: watch::Receiver<ImpairmentMetricsSnapshot>,
    stall_dropped: Arc<AtomicU64>,
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
        let (frame_tx, frame_rx) = mpsc::channel(FRAME_CAP);
        let snapshot_malformed = Arc::new(AtomicU64::new(0));
        #[cfg(feature = "dev-diagnostics")]
        let initial_impairment = NetworkImpairmentConfig::from_env();
        #[cfg(not(feature = "dev-diagnostics"))]
        let initial_impairment = NetworkImpairmentConfig::off(0);
        let (imp_cfg_tx, imp_cfg_rx) = watch::channel(initial_impairment);
        let (stall_tx, stall_rx) = mpsc::channel(STALL_CAP);
        let (reset_tx, reset_rx) = mpsc::channel(RESET_CAP);
        let (imp_metrics_tx, imp_metrics_rx) = watch::channel(ImpairmentMetricsSnapshot::default());
        let stall_dropped = Arc::new(AtomicU64::new(0));
        let stall_dropped_thread = Arc::clone(&stall_dropped);
        let sink = EventSink {
            lifecycle: life_tx,
            telemetry: tel_tx,
            telemetry_dropped: Arc::clone(&telemetry_dropped),
            verbose: Arc::clone(&log_verbose),
            frames: frame_tx,
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
                    ImpairmentNet {
                        config_rx: imp_cfg_rx,
                        stall_rx,
                        reset_rx,
                        metrics_tx: imp_metrics_tx,
                        stall_dropped: stall_dropped_thread,
                        harness: ImpairmentHarness::new(initial_impairment),
                        origin: Instant::now(),
                    },
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
            frames: frame_rx,
            snapshot_malformed,
            impairment_config: imp_cfg_tx,
            stall_tx,
            reset_metrics_tx: reset_tx,
            impairment_metrics: imp_metrics_rx,
            stall_dropped,
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
            NetworkCommand::Connect {
                attempt_id,
                dev_login,
            } => {
                let epoch = self.control.borrow().disconnect_epoch;
                self.commands
                    .try_send(RuntimeCommand::Connect {
                        attempt_id,
                        epoch,
                        dev_login,
                    })
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

    pub fn try_send_interact_open(&self, target: purgatory_protocol::WireEntityId) -> bool {
        self.input
            .try_send(ClientGameplayMsg::InteractOpen(target))
            .is_ok()
    }

    pub fn try_send_interact_close(&self, session_id: u32) -> bool {
        self.input
            .try_send(ClientGameplayMsg::InteractClose(session_id))
            .is_ok()
    }

    pub fn try_send_portal_activate(&self, target: purgatory_protocol::WireEntityId) -> bool {
        self.input
            .try_send(ClientGameplayMsg::PortalActivate(target))
            .is_ok()
    }

    pub fn try_send_dev_set_channel(&self, channel: u32) -> bool {
        self.input
            .try_send(ClientGameplayMsg::DevSetChannel(channel))
            .is_ok()
    }

    pub fn try_send_dev_set_speed(&self, speed: Option<u16>) -> bool {
        self.input
            .try_send(ClientGameplayMsg::DevSetSpeed(speed))
            .is_ok()
    }

    pub fn try_send_dev_set_jump(&self, jump: Option<u16>) -> bool {
        self.input
            .try_send(ClientGameplayMsg::DevSetJump(jump))
            .is_ok()
    }

    #[allow(dead_code)]
    pub fn try_send_equip(&self, request: purgatory_protocol::EquipRequest) -> bool {
        self.input
            .try_send(ClientGameplayMsg::Equip(request))
            .is_ok()
    }

    #[allow(dead_code)]
    pub fn try_send_unequip(&self, request: purgatory_protocol::UnequipRequest) -> bool {
        self.input
            .try_send(ClientGameplayMsg::Unequip(request))
            .is_ok()
    }

    #[allow(dead_code)]
    pub fn try_send_pickup(&self, request: purgatory_protocol::PickupRequest) -> bool {
        self.input
            .try_send(ClientGameplayMsg::Pickup(request))
            .is_ok()
    }

    pub fn try_send_dev_presentation_oneshot(&self, kind: u8) -> bool {
        self.input
            .try_send(ClientGameplayMsg::DevPresentationOneShot(kind))
            .is_ok()
    }

    pub fn try_send_dev_reset_player(&self) -> bool {
        self.input
            .try_send(ClientGameplayMsg::DevResetPlayer)
            .is_ok()
    }

    pub fn try_send_respawn(&self) -> bool {
        self.input.try_send(ClientGameplayMsg::Respawn).is_ok()
    }

    pub fn try_send_ability_activate(
        &self,
        request: purgatory_protocol::AbilityActivateRequest,
    ) -> bool {
        self.input
            .try_send(ClientGameplayMsg::AbilityActivate(request))
            .is_ok()
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

    /// Drain every pending replication frame in stream order. Never latest-wins.
    pub fn poll_frames(&mut self) -> Vec<ReplicationFrame> {
        let mut out = Vec::new();
        while let Ok(frame) = self.frames.try_recv() {
            out.push(frame);
        }
        out
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

    /// Latest-wins config. Unchanged values do not wake the net thread.
    pub fn set_impairment_config(&self, config: NetworkImpairmentConfig) {
        self.impairment_config.send_if_modified(|current| {
            if *current == config {
                false
            } else {
                *current = config;
                true
            }
        });
    }

    /// Imperative stall. Dropped if the stall channel is full.
    pub fn try_trigger_input_stall(&self, duration: Duration) -> bool {
        if self.stall_tx.try_send(duration).is_ok() {
            true
        } else {
            self.stall_dropped.fetch_add(1, Ordering::Relaxed);
            false
        }
    }

    pub fn try_reset_impairment_metrics(&self) -> bool {
        self.reset_metrics_tx.try_send(()).is_ok()
    }

    #[must_use]
    pub fn poll_impairment_metrics(&mut self) -> ImpairmentMetricsSnapshot {
        let mut snap = *self.impairment_metrics.borrow_and_update();
        snap.stall_trigger_dropped = snap
            .stall_trigger_dropped
            .max(self.stall_dropped.load(Ordering::Relaxed));
        snap
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
    mut impairment: ImpairmentNet,
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
                    Some(RuntimeCommand::Connect {
                        attempt_id,
                        epoch,
                        dev_login,
                    }) => {
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
                            &dev_login,
                            &mut commands,
                            &mut inputs,
                            &events,
                            &mut control,
                            &mut impairment,
                        )
                        .await;
                        drain_inputs(&mut inputs);
                        impairment.harness.clear_queues();
                        impairment.publish_metrics();
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
            _ = impairment.config_rx.changed() => {
                impairment.sync_control();
            }
            stall = impairment.stall_rx.recv() => {
                if let Some(dur) = stall {
                    impairment.harness.begin_input_stall(impairment.now_ns(), duration_ns(dur));
                    impairment.publish_metrics();
                }
            }
            reset = impairment.reset_rx.recv() => {
                if reset.is_some() {
                    impairment.harness.reset_metrics();
                    impairment.publish_metrics();
                }
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
    dev_login: &str,
    commands: &mut mpsc::Receiver<RuntimeCommand>,
    inputs: &mut mpsc::Receiver<ClientGameplayMsg>,
    events: &EventSink,
    control: &mut watch::Receiver<Control>,
    impairment: &mut ImpairmentNet,
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
        connection, attempt_id, epoch, dev_login, commands, inputs, events, control, impairment,
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
    dev_login: &str,
    commands: &mut mpsc::Receiver<RuntimeCommand>,
    inputs: &mut mpsc::Receiver<ClientGameplayMsg>,
    events: &EventSink,
    control: &mut watch::Receiver<Control>,
    impairment: &mut ImpairmentNet,
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
        dev_login: dev_login.to_string(),
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
        Ok(ServerControl::Interact(_)) => {
            connection.close(0u32.into(), b"handshake");
            return Err(NetworkEvent::Disconnected {
                attempt_id,
                kind: NetworkFailureKind::UnexpectedMessage,
            });
        }
        Ok(ServerControl::Equipment(_)) => {
            connection.close(0u32.into(), b"handshake");
            return Err(NetworkEvent::Disconnected {
                attempt_id,
                kind: NetworkFailureKind::UnexpectedMessage,
            });
        }
        Ok(ServerControl::Item(_)) => {
            connection.close(0u32.into(), b"handshake");
            return Err(NetworkEvent::Disconnected {
                attempt_id,
                kind: NetworkFailureKind::UnexpectedMessage,
            });
        }
        Ok(ServerControl::Inventory(_)) => {
            connection.close(0u32.into(), b"handshake");
            return Err(NetworkEvent::Disconnected {
                attempt_id,
                kind: NetworkFailureKind::UnexpectedMessage,
            });
        }
        Ok(ServerControl::PresentationOneShot(_)) => {
            connection.close(0u32.into(), b"handshake");
            return Err(NetworkEvent::Disconnected {
                attempt_id,
                kind: NetworkFailureKind::UnexpectedMessage,
            });
        }
        Ok(ServerControl::Ability(_)) => {
            connection.close(0u32.into(), b"handshake");
            return Err(NetworkEvent::Disconnected {
                attempt_id,
                kind: NetworkFailureKind::UnexpectedMessage,
            });
        }
        Err(kind) => {
            connection.close(0u32.into(), b"handshake");
            return Err(NetworkEvent::Disconnected { attempt_id, kind });
        }
    }

    drain_inputs(inputs);
    live_loop(
        connection, send, recv, attempt_id, epoch, commands, inputs, events, control, impairment,
    )
    .await
}

fn to_control(msg: ClientGameplayMsg) -> ClientControl {
    match msg {
        ClientGameplayMsg::Input(command) => ClientControl::Input(command),
        ClientGameplayMsg::HeldCancel => ClientControl::HeldCancel,
        ClientGameplayMsg::InteractOpen(target) => {
            ClientControl::InteractOpen(purgatory_protocol::InteractOpen { target })
        }
        ClientGameplayMsg::InteractClose(session_id) => {
            ClientControl::InteractClose(purgatory_protocol::InteractClose { session_id })
        }
        ClientGameplayMsg::PortalActivate(target) => {
            ClientControl::PortalActivate(purgatory_protocol::PortalActivate { target })
        }
        ClientGameplayMsg::DevSetChannel(channel) => {
            ClientControl::DevSetChannel(purgatory_protocol::DevSetChannel { channel })
        }
        ClientGameplayMsg::DevSetSpeed(speed) => {
            ClientControl::DevSetSpeed(purgatory_protocol::DevSetSpeed { speed })
        }
        ClientGameplayMsg::DevSetJump(jump) => {
            ClientControl::DevSetJump(purgatory_protocol::DevSetJump { jump })
        }
        ClientGameplayMsg::Equip(request) => ClientControl::Equip(request),
        ClientGameplayMsg::Unequip(request) => ClientControl::Unequip(request),
        ClientGameplayMsg::Pickup(request) => ClientControl::Pickup(request),
        ClientGameplayMsg::DevPresentationOneShot(kind) => {
            ClientControl::DevPresentationOneShot(purgatory_protocol::DevPresentationOneShot {
                kind,
            })
        }
        ClientGameplayMsg::DevResetPlayer => ClientControl::DevResetPlayer,
        ClientGameplayMsg::Respawn => ClientControl::Respawn,
        ClientGameplayMsg::AbilityActivate(request) => ClientControl::AbilityActivate(request),
    }
}

async fn drain_due_inputs(
    send: &mut SendStream,
    impairment: &mut ImpairmentNet,
    attempt_id: ConnectionAttemptId,
) -> Result<(), NetworkEvent> {
    let now = impairment.now_ns();
    let due = impairment
        .harness
        .input_mut()
        .poll_due(now, INPUT_DRAIN_PER_TURN);
    for msg in due {
        write_client_control(send, &to_control(msg))
            .await
            .map_err(|kind| NetworkEvent::Disconnected { attempt_id, kind })?;
    }
    Ok(())
}

async fn drain_due_snapshots(events: &EventSink, impairment: &mut ImpairmentNet) {
    let now = impairment.now_ns();
    let due = impairment
        .harness
        .snapshot_mut()
        .poll_due(now, SNAPSHOT_DRAIN_PER_TURN);
    for frame in due {
        events.push_frame(frame).await;
    }
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
    impairment: &mut ImpairmentNet,
) -> Result<AfterSession, NetworkEvent> {
    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut nonce: u64 = 1;
    let mut outstanding: VecDeque<(u64, Instant)> = VecDeque::new();
    let mut snap_recv: Option<RecvStream> = None;
    let mut uni_accepted = false;

    loop {
        impairment.sync_control();
        drain_due_inputs(&mut send, impairment, attempt_id).await?;
        drain_due_snapshots(events, impairment).await;
        impairment.publish_metrics();
        let now = impairment.now_ns();
        let more_due =
            impairment.harness.input().has_due(now) || impairment.harness.snapshot().has_due(now);
        let wake = if more_due {
            Some(Duration::ZERO)
        } else {
            impairment
                .harness
                .next_wake_ns(now)
                .map(|ns| Duration::from_nanos(ns.saturating_sub(now)))
        };

        let waiting_uni = !uni_accepted;
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
                    if impairment.should_queue_input() {
                        if let Err(EnqueueError::Overflow) = impairment.enqueue_input(msg) {
                            connection.close(0u32.into(), b"impair");
                            return Err(NetworkEvent::Disconnected {
                                attempt_id,
                                kind: NetworkFailureKind::InternalNetworkError,
                            });
                        }
                    } else if let Err(kind) =
                        write_client_control(&mut send, &to_control(msg)).await
                    {
                        return Err(NetworkEvent::Disconnected { attempt_id, kind });
                    }
                }
            }
            _ = impairment.config_rx.changed() => {
                impairment.sync_control();
            }
            stall = impairment.stall_rx.recv() => {
                if let Some(dur) = stall {
                    impairment
                        .harness
                        .begin_input_stall(impairment.now_ns(), duration_ns(dur));
                }
            }
            reset = impairment.reset_rx.recv() => {
                if reset.is_some() {
                    impairment.harness.reset_metrics();
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
                    Ok(ServerControl::Interact(event)) => {
                        events
                            .emit(
                                NetworkEvent::Interact { attempt_id, event },
                                control,
                            )
                            .await;
                    }
                    Ok(ServerControl::Equipment(event)) => {
                        events
                            .emit(
                                NetworkEvent::Equipment { attempt_id, event },
                                control,
                            )
                            .await;
                    }
                    Ok(ServerControl::Item(event)) => {
                        events
                            .emit(NetworkEvent::Item { attempt_id, event }, control)
                            .await;
                    }
                    Ok(ServerControl::Inventory(event)) => {
                        events
                            .emit(NetworkEvent::Inventory { attempt_id, event }, control)
                            .await;
                    }
                    Ok(ServerControl::PresentationOneShot(event)) => {
                        events
                            .emit(
                                NetworkEvent::PresentationOneShot { attempt_id, event },
                                control,
                            )
                            .await;
                    }
                    Ok(ServerControl::Ability(event)) => {
                        events
                            .emit(
                                NetworkEvent::Ability { attempt_id, event },
                                control,
                            )
                            .await;
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
                        // A second replication uni in one session is protocol-illegal.
                        snap_recv = Some(stream);
                        uni_accepted = true;
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
                    read_replication_frame(stream, events.verbose()).await
                } else {
                    std::future::pending::<Result<ReplicationFrame, SnapshotReadError>>().await
                }
            } => {
                match snap {
                    Ok(frame) => {
                        if impairment.should_queue_snapshot() {
                            let _ = impairment.enqueue_snapshot(frame);
                        } else {
                            events.push_frame(frame).await;
                        }
                    }
                    Err(SnapshotReadError::Closed) => {
                        return Err(NetworkEvent::Disconnected {
                            attempt_id,
                            kind: NetworkFailureKind::TransportLost,
                        });
                    }
                    Err(SnapshotReadError::Malformed) => {
                        events.note_malformed_snapshot();
                    }
                    Err(SnapshotReadError::FramingBroken) => {
                        events.note_malformed_snapshot();
                        return Err(NetworkEvent::Disconnected {
                            attempt_id,
                            kind: NetworkFailureKind::MalformedMessage,
                        });
                    }
                    Err(SnapshotReadError::Disconnected(kind)) => {
                        return Err(NetworkEvent::Disconnected { attempt_id, kind });
                    }
                }
            }
            _ = async {
                match wake {
                    Some(d) if d.is_zero() => tokio::task::yield_now().await,
                    Some(d) => tokio::time::sleep(d).await,
                    None => std::future::pending::<()>().await,
                }
            } => {}
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

async fn read_replication_frame(
    recv: &mut RecvStream,
    verbose: bool,
) -> Result<ReplicationFrame, SnapshotReadError> {
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
    decode_replication_frame(&payload).map_err(|_| SnapshotReadError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::replica::{FrameDecision, ReplicatedWorld};
    use purgatory_protocol::{
        ConnectionId, PlatformSupportId, ReplicatedKind, ReplicationFrame, ReplicationRecord,
        SnapshotEntity, Welcome, WireEntityId, encode_gameplay_frame, encode_replication_frame,
        encode_server_control,
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
                frames: mpsc::channel(FRAME_CAP).0,
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
                frames: mpsc::channel(FRAME_CAP).0,
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
                dev_login: "dev.local".into(),
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
                dev_login: "dev.local".into(),
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
            dev_login: "dev.local".into(),
        }));
        let mut accepted = 0;
        for i in 0..64 {
            if handle.try_send(NetworkCommand::Connect {
                attempt_id: ConnectionAttemptId::from_raw(i + 2),
                dev_login: "dev.local".into(),
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
            attempt_id: attempt,
            dev_login: "dev.local".into(),
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
            attempt_id: ConnectionAttemptId::from_raw(1),
            dev_login: "dev.local".into(),
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
                dev_login: "dev.local".into(),
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
            attempt_id: ConnectionAttemptId::from_raw(1),
            dev_login: "dev.local".into(),
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
        TwoFrames,
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

    fn sample_replication_frame() -> ReplicationFrame {
        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        ReplicationFrame {
            snapshot_sequence: 1,
            server_tick: 10,
            local_player_entity: local,
            input_epoch: 0,
            last_acknowledged_input_sequence: 0,
            local_grounded: false,
            local_grounded_on: PlatformSupportId::NONE,
            local_ignored_platform: PlatformSupportId::NONE,
            continuation_debt: 0,
            local_map: 1,
            local_channel: 0,
            local_instance: 0,
            observer_baseline_epoch: 0,
            records: vec![ReplicationRecord::Enter {
                entity: SnapshotEntity {
                    entity_id: local,
                    kind: ReplicatedKind::Player,
                    position: [3.0, 4.0],
                    velocity: [0.0, 0.0],
                },
                health: None,
                equipment: None,
            }],
            aoi_debug: None,
        }
    }

    fn sample_followup_update() -> ReplicationFrame {
        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let mut frame = sample_replication_frame();
        frame.snapshot_sequence = 2;
        frame.server_tick = 11;
        frame.records = vec![ReplicationRecord::Update {
            entity_id: local,
            domains: purgatory_protocol::DomainMask {
                transform: true,
                health: false,
                equipment: false,
            },
            position: Some([9.0, 4.0]),
            velocity: Some([0.0, 0.0]),
            health: None,
            equipment: None,
        }];
        frame
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
        write_snapshot_frames(connection, &[sample_replication_frame()]).await
    }

    async fn write_snapshot_frames(connection: &Connection, frames: &[ReplicationFrame]) -> bool {
        let Ok(mut uni) = connection.open_uni().await else {
            return false;
        };
        for frame in frames {
            let Ok(payload) = encode_replication_frame(frame) else {
                return false;
            };
            let Ok(encoded) = encode_gameplay_frame(&payload) else {
                return false;
            };
            if uni.write_all(&encoded).await.is_err() {
                return false;
            }
        }
        true
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
            UniAfterWelcome::TwoFrames => {
                tokio::time::sleep(Duration::from_millis(200)).await;
                let _ = write_snapshot_frames(
                    &connection,
                    &[sample_replication_frame(), sample_followup_update()],
                )
                .await;
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
            attempt_id: ConnectionAttemptId::from_raw(1),
            dev_login: "dev.local".into(),
        }));
        (server, handle)
    }

    /// Real app path: `NetworkHandle` → Connect → Welcome → uni → first frame.
    fn wait_connected_frames(
        handle: &mut NetworkHandle,
        limit: Duration,
        min_frames: usize,
    ) -> (bool, Vec<ReplicationFrame>) {
        let deadline = Instant::now() + limit;
        let mut connected = false;
        let mut frames = Vec::new();
        while Instant::now() < deadline {
            handle.poll(|event| {
                if matches!(event, NetworkEvent::Connected { .. }) {
                    connected = true;
                }
            });
            frames.extend(handle.poll_frames());
            if connected && frames.len() >= min_frames {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        (connected, frames)
    }

    fn assert_replica_from_first_frame(frame: ReplicationFrame) {
        let mut replica = ReplicatedWorld::new();
        replica.apply_frame(frame.clone());
        assert_eq!(replica.len(), 1, "replica must contain the local player");
        assert_eq!(replica.local_player(), Some(frame.local_player_entity));
        assert!(
            replica.get(frame.local_player_entity).is_some(),
            "local player entity must be in the replica map"
        );
        assert_eq!(replica.last_sequence(), Some(1));
    }

    #[test]
    fn live_loop_applies_snapshot_when_uni_arrives_after_welcome() {
        let (mut server, mut handle) = connect_scripted(UniAfterWelcome::Delayed);
        let (connected, frames) = wait_connected_frames(&mut handle, Duration::from_secs(5), 1);
        assert!(connected, "Welcome must emit Connected before snapshots");
        let snap = frames
            .into_iter()
            .next()
            .expect("first ReplicationFrame on delayed uni (live_loop must not panic)");
        assert_replica_from_first_frame(snap);
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
        let (connected, frames) = wait_connected_frames(&mut handle, Duration::from_secs(5), 1);
        assert!(connected, "Welcome must emit Connected");
        let snap = frames
            .into_iter()
            .next()
            .expect("first ReplicationFrame on immediate uni (live_loop must not panic)");
        assert_replica_from_first_frame(snap);
        join_on_drop(
            handle,
            Duration::from_secs(5),
            "immediate-uni snapshot session",
        );
        server.stop();
    }

    #[test]
    fn live_loop_applies_both_frames_when_two_arrive_before_poll() {
        let (mut server, mut handle) = connect_scripted(UniAfterWelcome::TwoFrames);
        let (connected, frames) = wait_connected_frames(&mut handle, Duration::from_secs(5), 2);
        assert!(connected);
        assert!(
            frames.len() >= 2,
            "watch latest-wins must not drop the Enter before the Update, got {}",
            frames.len()
        );
        let mut replica = ReplicatedWorld::new();
        for frame in frames {
            assert!(matches!(
                replica.apply_frame(frame),
                FrameDecision::Applied { .. }
            ));
        }
        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        assert_eq!(replica.get(local).unwrap().position[0], 9.0);
        join_on_drop(
            handle,
            Duration::from_secs(5),
            "two-frame replication session",
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

    #[test]
    fn live_loop_services_control_during_impaired_input_burst() {
        let src = include_str!("runtime.rs");
        assert!(src.contains("INPUT_DRAIN_PER_TURN"));
        assert!(src.contains("SNAPSHOT_DRAIN_PER_TURN"));
        assert!(src.contains("config_rx.changed"));
        assert!(src.contains("stall_rx.recv"));
        assert!(src.contains("drain_due_inputs"));
        assert!(src.contains("session_cut"));
    }
}
