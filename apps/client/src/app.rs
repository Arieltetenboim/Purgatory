//! Client application loop: platform events, simulation clock, renderer.

use std::sync::Arc;
use std::time::{Duration, Instant};

use purgatory_simulation::{
    Aabb, PLAYER_HALF_EXTENTS, PlatformKind, PlayerInput, SimulationClock, World,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::debug::{
    CameraMotionDebug, CollisionHistoryEvent, ConnectionPaint, DebugOverlay, DebugSnapshot,
    DiscSubject, OverlayInit, SnapshotExtras, footnote_debug_quads, gameplay_receives_keyboard,
    gameplay_receives_pointer, is_debug_toggle,
};
use crate::frontend::ConnectionFrontend;
use crate::input::{ActionState, IntentNet};
use crate::interp::{InterpolationBuffer, PresentationPose};
use crate::lifecycle::{ClientLifecycle, ClientScreen};
use crate::network::{ClientEndpointConfig, NetworkCommand, NetworkHandle};
use crate::platform::{diagnostic_title, window_attributes};
use crate::prediction::{LocalPrediction, local_presentation_pose};
use crate::renderer::{
    Camera, DrawQuad, FOOTNOTE_LOGICAL_HEIGHT, FrameStatus, PARALLAX_FAR, PARALLAX_MID,
    PARALLAX_NEAR, Renderer, is_usable_surface, parallax_debug_quads, parallax_quads,
};
use crate::replica::{ReplicatedWorld, SnapshotDecision};

const PLAYER_COLOR: [f32; 4] = [0.19, 0.55, 0.66, 1.0];
const REMOTE_PLAYER_COLOR: [f32; 4] = [0.72, 0.32, 0.38, 1.0];
const INTERP_AUTH_GIZMO_COLOR: [f32; 4] = [1.0, 0.85, 0.2, 0.55];
const PREDICT_AUTH_GIZMO_COLOR: [f32; 4] = [0.95, 0.45, 0.15, 0.55];
const PREDICT_POSE_GIZMO_COLOR: [f32; 4] = [0.25, 0.85, 0.55, 0.55];
const SOLID_FLOOR_COLOR: [f32; 4] = [0.22, 0.28, 0.24, 1.0];
const SOLID_PLATFORM_COLOR: [f32; 4] = [0.45, 0.32, 0.18, 1.0];
const ONEWAY_COLOR: [f32; 4] = [0.55, 0.62, 0.85, 1.0];
const MARKER_COLOR: [f32; 4] = [0.91, 0.69, 0.19, 1.0];

pub fn run() -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|err| format!("event loop: {err}"))?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = ClientApp::new();
    event_loop
        .run_app(&mut app)
        .map_err(|err| format!("run app: {err}"))?;
    if let Some(err) = app.fatal {
        return Err(err);
    }
    Ok(())
}

struct ClientApp {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    debug: Option<DebugOverlay>,
    frontend: Option<ConnectionFrontend>,
    lifecycle: ClientLifecycle,
    clock: SimulationClock,
    world: World,
    actions: ActionState,
    last_input: PlayerInput,
    intent: IntentNet,
    last_instant: Instant,
    last_title_tick: u64,
    fps: f32,
    fatal: Option<String>,
    last_camera_motion: CameraMotionDebug,
    network: Option<NetworkHandle>,
    replica: ReplicatedWorld,
    interp: InterpolationBuffer,
    prediction: LocalPrediction,
    snapshot_malformed: u64,
}

impl ClientApp {
    fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            debug: None,
            clock: SimulationClock::new(),
            world: World::footnote_test_stage(),
            actions: ActionState::default(),
            last_input: PlayerInput::idle(),
            intent: IntentNet::default(),
            last_instant: Instant::now(),
            last_title_tick: 0,
            fps: 0.0,
            fatal: None,
            last_camera_motion: CameraMotionDebug::default(),
            lifecycle: ClientLifecycle::new(ClientEndpointConfig::dev().server),
            frontend: None,
            network: match NetworkHandle::start(ClientEndpointConfig::dev()) {
                Ok(handle) => Some(handle),
                Err(err) => {
                    eprintln!("PURGATORY client network init failed: {err}");
                    None
                }
            },
            replica: ReplicatedWorld::new(),
            interp: InterpolationBuffer::new(),
            prediction: LocalPrediction::new(),
            snapshot_malformed: 0,
        }
    }

    fn debug_overlay_visible(&self) -> bool {
        self.debug.as_ref().is_some_and(DebugOverlay::is_visible)
    }

    fn time_scale(&self) -> f32 {
        self.debug
            .as_ref()
            .map(|d| d.ui.time_scale)
            .unwrap_or(1.0)
            .clamp(0.01, 1.0)
    }

    fn toggle_debug_overlay(&mut self) {
        if let Some(overlay) = &mut self.debug {
            overlay.toggle();
        }
    }

    fn poll_network(&mut self) {
        if let Some(debug) = &mut self.debug {
            self.lifecycle.set_log_flags(
                debug.ui.log_network_lifecycle,
                debug.ui.verbose_network_trace,
            );
            if debug.ui.clear_network_history {
                self.lifecycle.clear_history();
                debug.ui.clear_network_history = false;
            }
            if let Some(network) = &self.network {
                network.set_log_flags(
                    debug.ui.log_network_lifecycle,
                    debug.ui.verbose_network_trace,
                );
            }
        }
        let before = self.lifecycle.screen();
        let mut events = Vec::new();
        let mut dropped = 0;
        if let Some(network) = &mut self.network {
            dropped = network.telemetry_dropped();
            self.snapshot_malformed = network.snapshot_malformed();
            network.poll(|event| events.push(event));
            if let Some(snap) = network.poll_snapshot()
                && self.replica.apply(snap.clone()) == SnapshotDecision::Accept
            {
                self.interp.push(&snap);
                self.intent.set_epoch(self.replica.input_epoch());
                self.prediction.sync_from_replica(
                    &self.replica,
                    &mut self.world,
                    self.clock.tick().get(),
                );
            }
        }
        self.lifecycle.set_events_dropped(dropped);
        for event in events {
            self.lifecycle.apply(event);
        }
        if self.lifecycle.screen() != before {
            self.on_screen_changed();
        }
    }

    fn on_screen_changed(&mut self) {
        self.actions.clear();
        self.last_input = PlayerInput::idle();
        self.intent.reset();
        self.replica.clear();
        self.interp.clear();
        self.prediction.clear();
        rebase_wall_clock(&mut self.last_instant, Instant::now());
    }

    fn request_connect(&mut self) {
        let Some(id) = self.lifecycle.try_begin_connect() else {
            return;
        };
        println!("PURGATORY connect requested attempt={id}");
        let sent = self
            .network
            .as_ref()
            .is_some_and(|net| net.try_send(NetworkCommand::Connect { attempt_id: id }));
        if !sent {
            eprintln!("PURGATORY connection failed attempt={id} reason=command dropped");
            self.lifecycle.fail_unsent_connect();
        }
    }

    fn request_disconnect(&mut self) {
        let before = self.lifecycle.screen();
        let should_send = self.lifecycle.request_disconnect();
        if should_send && let Some(network) = &self.network {
            let _ = network.try_send(NetworkCommand::Disconnect);
        }
        if self.lifecycle.screen() != before {
            self.on_screen_changed();
        }
    }

    /// Send one per-tick command from the PlayerInput sample applied on this sim tick.
    fn send_intent_for_tick_input(&mut self, input: PlayerInput) {
        if !self.lifecycle.gameplay_actions_allowed() {
            return;
        }
        if !self.prediction.active() {
            return;
        }
        let Some(network) = self.network.as_ref() else {
            return;
        };
        self.intent.set_epoch(self.replica.input_epoch());
        let Some(command) = self.intent.consider_tick_input(input) else {
            return;
        };
        if !self.prediction.try_push_pending(command) {
            return;
        }
        let _ = network.try_send_input(command);
    }

    /// Focus-loss: ActionState is already released. Pair a Neutral clock step
    /// with a command, or send HeldCancel when the send window is full.
    fn on_focus_loss_input(&mut self) {
        if !self.lifecycle.gameplay_actions_allowed() {
            return;
        }
        let Some(network) = self.network.as_ref() else {
            return;
        };
        if self.prediction.pending_window_full() {
            let barrier = self.prediction.capture_cancel_barrier();
            if network.try_send_held_cancel() {
                self.prediction.enter_cancel_pending(barrier);
            }
            return;
        }
        self.clock.force_step();
        let input = PlayerInput::idle();
        self.last_input = input;
        let tick = self.clock.tick().get();
        self.prediction.tick(&mut self.world, input, tick);
        self.intent.set_epoch(self.replica.input_epoch());
        if let Some(command) = self.intent.emit_neutral()
            && self.prediction.try_push_pending(command)
        {
            let _ = network.try_send_input(command);
        }
    }

    fn advance_simulation(&mut self) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_instant);
        self.last_instant = now;
        let seconds = elapsed.as_secs_f32();
        self.fps = if seconds > 0.0 { 1.0 / seconds } else { 0.0 };

        // Development time scale: scale wall elapsed into the clock only.
        // TICK_RATE_HZ / TICK_DURATION are unchanged.
        let scale = self.time_scale();
        let scaled = if scale >= 0.999 {
            elapsed
        } else {
            let nanos = (elapsed.as_secs_f64() * f64::from(scale) * 1_000_000_000.0) as u64;
            Duration::from_nanos(nanos)
        };
        let update = self.clock.advance(scaled);
        let tick_after = self.clock.tick().get();
        let tick_base = tick_after.saturating_sub(u64::from(update.ticks_executed));
        let detector = self
            .debug
            .as_ref()
            .map(|d| d.ui.position_discontinuity_detector)
            .unwrap_or(false);
        let log_disc = self
            .debug
            .as_ref()
            .map(|d| d.ui.log_discontinuities)
            .unwrap_or(false);
        let verbose = self
            .debug
            .as_ref()
            .map(|d| d.ui.verbose_collision_trace)
            .unwrap_or(false);
        if update.ticks_executed > 1 {
            self.prediction
                .on_hitch_discontinuity(&self.replica, &mut self.world, tick_after);
            if !self.prediction.pending_window_full() {
                let input = self.actions.consume_tick_input();
                self.last_input = input;
                self.prediction.tick(&mut self.world, input, tick_after);
                self.send_intent_for_tick_input(input);
            }
            return;
        }
        for step in 0..update.ticks_executed {
            if self.prediction.pending_window_full() {
                continue;
            }
            // One sample per tick: predict locally, then send the same sample.
            let input = self.actions.consume_tick_input();
            self.last_input = input;
            let client_tick = tick_base.saturating_add(u64::from(step) + 1);
            self.prediction.tick(&mut self.world, input, client_tick);
            self.send_intent_for_tick_input(input);
            let tick = client_tick;
            let motion = self.world.last_motion_debug();
            if verbose && (motion.correction[0].abs() > 1e-5 || motion.correction[1].abs() > 1e-5) {
                eprintln!(
                    "PURGATORY collision trace tick={tick} kind={:?} axis={:?} corr={:?} cand={:?} prev={:?} pos={:?}",
                    motion.response_kind,
                    motion.correction_axis,
                    motion.correction,
                    motion.collision_candidate,
                    motion.previous_position,
                    motion.position
                );
            }
            if detector && motion.discontinuity {
                let ev = CollisionHistoryEvent {
                    tick,
                    subject: DiscSubject::Player,
                    axis: motion.correction_axis,
                    delta: motion.delta,
                    correction: motion.correction,
                    previous_position: motion.previous_position,
                    position: motion.position,
                    velocity: motion.velocity,
                    candidate: motion.collision_candidate,
                    grounded_on: motion.grounded_on,
                    response_kind: motion.response_kind,
                    discontinuity: true,
                };
                if let Some(debug) = &mut self.debug {
                    debug.collision_history.push(ev);
                }
                if log_disc {
                    eprintln!(
                        "PURGATORY player discontinuity tick={tick} delta={:?} corr={:?} cand={:?}",
                        motion.delta, motion.correction, motion.collision_candidate
                    );
                }
            }
        }
    }

    fn update_camera(&mut self) {
        let bounds = self.world.bounds();
        let player_pos = local_presentation_pose(&self.prediction, &self.world, &self.replica)
            .unwrap_or([0.0, 0.0]);
        let follow = self
            .debug
            .as_ref()
            .map(|d| d.ui.camera_follow)
            .unwrap_or(true);
        let center_request = self.debug.as_ref().is_some_and(|d| d.ui.center_on_player);
        if let Some(debug) = &mut self.debug {
            debug.ui.center_on_player = false;
        }
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let mut camera = renderer.camera();
        let previous = camera.position;
        let follow_now = follow || center_request;
        if follow_now {
            camera.follow_clamped(player_pos, bounds);
        } else {
            camera.clamp_to_bounds(bounds);
        }
        let raw_follow = follow_now.then_some(player_pos);
        self.last_camera_motion =
            CameraMotionDebug::record(previous, camera.position, raw_follow, follow_now);
        if self
            .debug
            .as_ref()
            .is_some_and(|d| d.ui.position_discontinuity_detector)
            && self.last_camera_motion.discontinuity
        {
            let tick = self.clock.tick().get();
            let cm = self.last_camera_motion;
            let ev = CollisionHistoryEvent {
                tick,
                subject: DiscSubject::Camera,
                axis: purgatory_simulation::CorrectionAxis::None,
                delta: cm.delta,
                correction: [0.0, 0.0],
                previous_position: cm.previous_position,
                position: camera.position,
                velocity: [0.0, 0.0],
                candidate: None,
                grounded_on: None,
                response_kind: purgatory_simulation::ResponseKind::None,
                discontinuity: true,
            };
            if let Some(debug) = &mut self.debug {
                debug.collision_history.push(ev);
                if debug.ui.log_discontinuities {
                    eprintln!(
                        "PURGATORY camera discontinuity tick={tick} prev={:?} delta={:?}",
                        cm.previous_position, cm.delta
                    );
                }
            }
        }
        renderer.set_camera(camera);
    }

    fn update_title(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let tick = self.clock.tick().get();
        let frames = self.renderer.as_ref().map_or(0, Renderer::frames_drawn);
        if tick == self.last_title_tick && !frames.is_multiple_of(30) {
            return;
        }
        self.last_title_tick = tick;
        let (width, height) = self
            .renderer
            .as_ref()
            .map(Renderer::surface_size)
            .unwrap_or((0, 0));
        window.set_title(&diagnostic_title(width, height, tick, frames));
    }

    fn handle_frame(&mut self, event_loop: &ActiveEventLoop) {
        self.poll_network();
        if simulation_should_advance(self.lifecycle.screen()) {
            self.advance_simulation();
            self.update_camera();
        } else {
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(self.last_instant);
            rebase_wall_clock(&mut self.last_instant, now);
            let seconds = elapsed.as_secs_f32();
            self.fps = if seconds > 0.0 { 1.0 / seconds } else { 0.0 };
        }

        let overlay_open = self.debug_overlay_visible();
        let on_connection = self.lifecycle.screen() == ClientScreen::Connection;
        let camera = self.renderer.as_ref().map(Renderer::camera);
        let Some(camera) = camera else {
            return;
        };

        let mut quads = Vec::new();
        if !on_connection {
            self.interp.sample(Instant::now());
            let local_pose = local_presentation_pose(&self.prediction, &self.world, &self.replica);
            quads = parallax_quads(&camera, self.world.bounds());
            quads.extend(scene_quads(&self.world, local_pose, self.interp.poses()));
            if overlay_open {
                let ui = self
                    .debug
                    .as_ref()
                    .map(|d| d.ui.clone())
                    .unwrap_or_default();
                quads.extend(footnote_debug_quads(&self.world, &ui, local_pose));
                if ui.show_parallax_debug {
                    quads.extend(parallax_debug_quads(&camera));
                }
                if ui.show_interpolation_gizmos {
                    quads.extend(interpolation_gizmos(&self.replica, self.interp.poses()));
                }
                if ui.show_prediction_gizmos {
                    quads.extend(prediction_gizmos(
                        &self.replica,
                        local_pose,
                        self.prediction.active(),
                    ));
                }
            }
        }

        let Some(window) = self.window.clone() else {
            return;
        };
        let fps = self.fps;
        let tick = self.clock.tick().get();
        let last_input = self.last_input;
        let mut snapshot = {
            let Some(renderer) = self.renderer.as_ref() else {
                return;
            };
            let cam = renderer.camera();
            DebugSnapshot::capture(
                &self.world,
                tick,
                renderer.frames_drawn(),
                renderer.surface_size(),
                fps,
                last_input,
                SnapshotExtras {
                    stage_name: "footnote_test_stage",
                    camera_position: cam.position,
                    viewport_width: cam.viewport_width,
                    viewport_height: cam.viewport_height,
                    parallax_far: PARALLAX_FAR,
                    parallax_mid: PARALLAX_MID,
                    parallax_near: PARALLAX_NEAR,
                    camera_motion: self.last_camera_motion,
                },
            )
        };
        snapshot.network = self.lifecycle.snapshot();
        snapshot.net_input_seq = self.intent.sequence;
        snapshot.net_input_sent = self.intent.commands_sent;
        snapshot.net_move_axis = self.intent.move_axis.to_i8();
        snapshot.net_jump = self.intent.jump_pressed;
        snapshot.net_down = self.intent.down_held;
        snapshot.replica_seq = self.replica.last_sequence();
        snapshot.replica_tick = self.replica.last_server_tick();
        snapshot.replica_entities = self.replica.len() as u32;
        snapshot.replica_local = self.replica.local_player().map(|id| id.to_string());
        snapshot.replica_stale = self.replica.stale_ignored;
        snapshot.replica_duplicate = self.replica.duplicate_ignored;
        snapshot.replica_malformed = self.snapshot_malformed;
        snapshot.replica_age_ms = self
            .replica
            .snapshot_age(Instant::now())
            .map(|d| d.as_millis() as u64);
        let interp = self.interp.diagnostics();
        snapshot.interp_enabled = interp.enabled;
        snapshot.interp_delay_ticks = interp.delay_ticks;
        snapshot.interp_delay_ms = interp.delay_ms;
        snapshot.interp_history_depth = interp.history_depth;
        snapshot.interp_estimated_tick = interp.estimated_server_tick;
        snapshot.interp_render_tick = interp.render_tick;
        snapshot.interp_bracket_a = interp.bracket_a_tick;
        snapshot.interp_bracket_b = interp.bracket_b_tick;
        snapshot.interp_alpha = interp.alpha;
        snapshot.interp_holds = interp.holds;
        snapshot.interp_snaps = interp.snaps;
        let pred = self.prediction.diagnostics(&self.world, &self.replica);
        snapshot.pred_enabled = pred.enabled;
        snapshot.pred_active = pred.active;
        snapshot.pred_auth_pos = pred.auth_position;
        snapshot.pred_pos = pred.predicted_position;
        snapshot.pred_vel = pred.predicted_velocity;
        snapshot.pred_error = pred.lead_error;
        snapshot.pred_lead_error = pred.lead_error;
        snapshot.pred_aligned_error = pred.aligned_error;
        snapshot.pred_aligned_dx = pred.aligned_dx;
        snapshot.pred_aligned_dy = pred.aligned_dy;
        snapshot.pred_best_offset = pred.best_temporal_offset;
        snapshot.pred_tick = pred.prediction_tick;
        snapshot.pred_auth_tick = pred.auth_server_tick;
        snapshot.pred_best_match_tick = pred.best_match_tick;
        snapshot.pred_resets = pred.reset_count;
        snapshot.pred_drift_corrections = pred.drift_correction_count;
        snapshot.pred_aligned_divergence = pred.consecutive_aligned_divergence;
        snapshot.pred_max_aligned = pred.max_aligned_error;
        snapshot.pred_last_snap = pred.last_snap_reason.map(str::to_string);
        snapshot.pred_auth_vel = pred.auth_velocity;
        snapshot.pred_pending = pred.pending_count;
        snapshot.pred_ack = pred.last_ack;
        snapshot.pred_epoch = pred.input_epoch;
        snapshot.pred_debt = pred.continuation_debt;
        snapshot.pred_cancel_pending = pred.cancel_pending;

        let mut actions = Vec::new();
        let mut connect_clicked = false;
        let status = {
            let Some(renderer) = self.renderer.as_mut() else {
                return;
            };
            let overlay = self.debug.as_mut();
            let frontend = self.frontend.as_ref();
            let server = format!("{}", self.lifecycle.view().server);
            let line = self.lifecycle.view().frontend_status();
            let can_connect = self.lifecycle.can_connect();
            let on_connection = self.lifecycle.screen() == ClientScreen::Connection;
            renderer.render(&quads, |pass| {
                let Some(overlay) = overlay else {
                    return Vec::new();
                };
                let (extras, emitted, connect) = overlay.submit_frame(
                    &window,
                    pass,
                    &snapshot,
                    if on_connection {
                        frontend.map(|frontend| ConnectionPaint {
                            frontend,
                            server: &server,
                            status: line,
                            can_connect,
                        })
                    } else {
                        None
                    },
                );
                actions = emitted;
                connect_clicked = connect;
                extras
            })
        };

        for action in actions {
            if matches!(action, purgatory_simulation::DebugAction::ResetPlayer)
                && self.prediction.active()
                && self.replica.local_entity().is_some()
            {
                // Networked: snap prediction to authority — do not FOOTNOTE-spawn
                // while the orange replica gizmo stays elsewhere.
                self.prediction.force_reanchor_from_replica(
                    &self.replica,
                    &mut self.world,
                    self.clock.tick().get(),
                );
            } else {
                self.world.apply_debug_action(action);
            }
        }

        if connect_clicked {
            self.request_connect();
        }

        if let Some(debug) = &mut self.debug {
            let connect = debug.ui.network_connect;
            let disconnect = debug.ui.network_disconnect;
            debug.ui.network_connect = false;
            debug.ui.network_disconnect = false;
            if connect {
                self.request_connect();
            }
            if disconnect {
                self.request_disconnect();
            }
        }

        match status {
            FrameStatus::Drawn | FrameStatus::Skipped => {}
            FrameStatus::NeedsReconfigure => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.reconfigure();
                }
            }
            FrameStatus::DeviceLost => {
                self.fatal = Some("GPU surface validation failure".to_string());
                event_loop.exit();
                return;
            }
        }
        self.update_title();
    }
}

fn scene_quads(
    world: &World,
    local_pose: Option<[f32; 2]>,
    remote_poses: &[PresentationPose],
) -> Vec<DrawQuad> {
    let mut quads = Vec::with_capacity(8);
    for view in world.iter_platforms() {
        let color = match view.platform.kind {
            PlatformKind::Solid => {
                if view.platform.half_extents[0] >= 7.0 {
                    SOLID_FLOOR_COLOR
                } else {
                    SOLID_PLATFORM_COLOR
                }
            }
            PlatformKind::OneWay => ONEWAY_COLOR,
            _ => SOLID_PLATFORM_COLOR,
        };
        quads.push(aabb_quad(view.aabb(), color));
    }
    if let Some(position) = local_pose {
        let aabb = Aabb::new(position, PLAYER_HALF_EXTENTS);
        quads.push(aabb_quad(aabb, PLAYER_COLOR));
    }
    for pose in remote_poses {
        let aabb = Aabb::new(pose.position, PLAYER_HALF_EXTENTS);
        quads.push(aabb_quad(aabb, REMOTE_PLAYER_COLOR));
    }
    let b = world.bounds();
    quads.push(DrawQuad {
        center: [b.min_x + 0.6, b.max_y - 0.6],
        size: [0.35, 0.35],
        color: MARKER_COLOR,
    });
    quads
}

/// Authoritative remote positions as small markers (presentation gizmos only).
fn interpolation_gizmos(
    replica: &ReplicatedWorld,
    remote_poses: &[PresentationPose],
) -> Vec<DrawQuad> {
    let local = replica.local_player();
    let mut quads = Vec::new();
    for entity in replica.iter() {
        if Some(entity.entity_id) == local {
            continue;
        }
        let aabb = Aabb::new(entity.position, [0.15, 0.15]);
        quads.push(aabb_quad(aabb, INTERP_AUTH_GIZMO_COLOR));
    }
    for pose in remote_poses {
        let aabb = Aabb::new(pose.position, [0.12, 0.12]);
        quads.push(aabb_quad(aabb, REMOTE_PLAYER_COLOR));
    }
    quads
}

/// Authoritative vs predicted local markers (presentation gizmos only; default OFF).
fn prediction_gizmos(
    replica: &ReplicatedWorld,
    local_pose: Option<[f32; 2]>,
    prediction_active: bool,
) -> Vec<DrawQuad> {
    let mut quads = Vec::new();
    if let Some(auth) = replica.local_entity() {
        let aabb = Aabb::new(auth.position, [0.18, 0.18]);
        quads.push(aabb_quad(aabb, PREDICT_AUTH_GIZMO_COLOR));
    }
    if prediction_active && let Some(pose) = local_pose {
        let aabb = Aabb::new(pose, [0.14, 0.14]);
        quads.push(aabb_quad(aabb, PREDICT_POSE_GIZMO_COLOR));
    }
    quads
}

fn aabb_quad(aabb: Aabb, color: [f32; 4]) -> DrawQuad {
    DrawQuad {
        center: aabb.center,
        size: aabb.size(),
        color,
    }
}

impl ApplicationHandler for ClientApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = match event_loop.create_window(window_attributes()) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                self.fatal = Some(format!("create window: {err}"));
                event_loop.exit();
                return;
            }
        };

        match Renderer::new(window.clone()) {
            Ok(renderer) => {
                let camera: Camera = renderer.camera();
                let origin_ndc = camera.world_to_ndc([0.0, 0.0]);
                println!(
                    "PURGATORY client window ready adapter='{}' backend={:?} {}x{} format={:?} world_viewport={:.2}x{:.2} logical_height={:.1} origin_ndc=({:.2},{:.2})",
                    renderer.adapter_name(),
                    renderer.backend(),
                    renderer.surface_size().0,
                    renderer.surface_size().1,
                    renderer.surface_format(),
                    camera.viewport_width,
                    camera.viewport_height,
                    FOOTNOTE_LOGICAL_HEIGHT,
                    origin_ndc[0],
                    origin_ndc[1],
                );
                println!(
                    "PURGATORY connection frontend: CONNECT to 127.0.0.1:5001. No auto-connect."
                );
                println!(
                    "PURGATORY controls (in Game): A/Left=MoveLeft D/Right=MoveRight S/Down=Down Space=Jump | Down+Jump=drop through OneWay | camera follows player"
                );
                println!(
                    "PURGATORY debug overlay: Backquote/~ toggles tabs, gizmos, camera follow, slow-mo"
                );
                let overlay = DebugOverlay::new(
                    &window,
                    OverlayInit {
                        device: renderer.device(),
                        surface_format: renderer.surface_format(),
                        max_texture_side: renderer.max_texture_dimension_2d() as usize,
                    },
                );
                let frontend = ConnectionFrontend::load(overlay.context());
                self.debug = Some(overlay);
                self.frontend = Some(frontend);
                self.renderer = Some(renderer);
                self.window = Some(window);
                self.last_instant = Instant::now();
                println!("PURGATORY client frontend ready");
            }
            Err(err) => {
                self.fatal = Some(err);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && is_debug_toggle(key)
        {
            self.toggle_debug_overlay();
            window.request_redraw();
            return;
        }

        let overlay_open = self.debug_overlay_visible();
        let on_connection = self.lifecycle.screen() == ClientScreen::Connection;
        if (overlay_open || on_connection)
            && let Some(overlay) = &mut self.debug
        {
            overlay.on_window_event(&window, &event);
        }

        match event {
            WindowEvent::CloseRequested => {
                println!("PURGATORY client closing");
                if let Some(network) = &self.network {
                    let _ = network.try_send(NetworkCommand::Shutdown);
                }
                self.network = None;
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if is_usable_surface(size.width, size.height)
                    && let Some(renderer) = self.renderer.as_mut()
                {
                    renderer.resize(size.width, size.height);
                    // Keep camera clamped after aspect change.
                    let bounds = self.world.bounds();
                    let mut cam = renderer.camera();
                    cam.clamp_to_bounds(bounds);
                    renderer.set_camera(cam);
                }
                window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let text_like = self
                    .debug
                    .as_ref()
                    .is_some_and(DebugOverlay::wants_keyboard_for_text);
                let pressed = event.state == ElementState::Pressed;
                if self.lifecycle.gameplay_actions_allowed()
                    && gameplay_receives_keyboard(overlay_open, text_like, pressed)
                {
                    // Latch ActionState only. Intent is emitted on the sim tick
                    // that consumes the same PlayerInput as prediction.
                    self.actions.apply_key_event(&event);
                }
            }
            WindowEvent::Focused(focused) => {
                if !focused {
                    // Missed key-ups while unfocused leave ActionState latched.
                    // Clear held input and push Neutral immediately.
                    self.actions.release_on_focus_loss();
                    self.on_focus_loss_input();
                }
            }
            WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. } => {
                let _gameplay_mouse = gameplay_receives_pointer(
                    overlay_open,
                    self.debug.as_ref().is_some_and(DebugOverlay::wants_pointer),
                );
            }
            WindowEvent::RedrawRequested => {
                self.handle_frame(event_loop);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Game simulation advances only on the Game screen. Connection frontend
/// never feeds elapsed time into [`SimulationClock`].
#[must_use]
fn simulation_should_advance(screen: ClientScreen) -> bool {
    matches!(screen, ClientScreen::Game)
}

fn rebase_wall_clock(last_instant: &mut Instant, now: Instant) {
    *last_instant = now;
}

#[cfg(test)]
mod tests {
    use crate::platform::{DEV_WINDOW_HEIGHT, DEV_WINDOW_WIDTH};
    use crate::renderer::{Camera, DEFAULT_LOGICAL_HEIGHT, is_usable_surface};
    use purgatory_simulation::{
        FootnoteConfig, PlayerInput, SimulationClock, TICK_DURATION, World,
    };
    use std::time::Duration;

    #[test]
    fn workspace_crates_are_linked() {
        assert!(!purgatory_common::version().is_empty());
        assert!(!purgatory_protocol::version().is_empty());
        assert!(!purgatory_content::version().is_empty());
        assert!(!purgatory_simulation::version().is_empty());
    }

    #[test]
    fn simulation_advances_without_a_rendered_frame() {
        let mut clock = SimulationClock::new();
        assert_eq!(clock.advance(Duration::from_millis(16)).ticks_executed, 0);
        assert_eq!(clock.advance(Duration::from_millis(20)).ticks_executed, 1);
        assert_eq!(clock.tick().get(), 1);
    }

    #[test]
    fn clock_ticks_drive_world_once_each() {
        let mut clock = SimulationClock::new();
        let mut world = World::footnote_test_stage();
        // Open mid-floor stretch — spawn sits near slope/left boundary.
        if let Some((t, p)) = world.player_parts_mut() {
            t.position[0] = 0.0;
            p.velocity = [0.0, 0.0];
        }
        let start_x = world.player_body().expect("player").position[0];
        let update = clock.advance(Duration::from_secs(1));
        assert_eq!(update.ticks_executed, 30);
        let dt = TICK_DURATION.as_secs_f32();
        let input = PlayerInput::from_buttons(false, true, false);
        for _ in 0..update.ticks_executed {
            world.tick(dt, input);
        }
        let traveled = world.player_body().expect("player").position[0] - start_x;
        let max = FootnoteConfig::DEFAULT.max_ground_speed;
        assert!(
            traveled > max - 0.5 && traveled <= max + 0.05,
            "traveled {traveled}, expected near {max} with accel ramp"
        );
    }

    #[test]
    fn half_time_scale_advances_about_half_the_ticks() {
        let mut full = SimulationClock::new();
        let mut half = SimulationClock::new();
        let wall = Duration::from_secs(1);
        assert_eq!(full.advance(wall).ticks_executed, 30);
        let scaled = Duration::from_secs_f64(wall.as_secs_f64() * 0.5);
        let ticks = half.advance(scaled).ticks_executed;
        assert!(
            (14..=16).contains(&ticks),
            "0.5x over 1s wall should be ~15 ticks, got {ticks}"
        );
    }

    #[test]
    fn quarter_time_scale_advances_about_quarter_ticks() {
        let mut clock = SimulationClock::new();
        let wall = Duration::from_secs(1);
        let scaled = Duration::from_secs_f64(wall.as_secs_f64() * 0.25);
        let ticks = clock.advance(scaled).ticks_executed;
        assert!(
            (7..=8).contains(&ticks),
            "0.25x over 1s wall should be ~7–8 ticks, got {ticks}"
        );
    }

    #[test]
    fn development_viewport_matches_window_aspect() {
        let camera = Camera::from_physical_pixels(DEV_WINDOW_WIDTH, DEV_WINDOW_HEIGHT)
            .expect("dev size is usable");
        assert!((camera.viewport_width - 16.0).abs() < f32::EPSILON);
        assert!((camera.viewport_height - DEFAULT_LOGICAL_HEIGHT).abs() < f32::EPSILON);
        assert!(is_usable_surface(DEV_WINDOW_WIDTH, DEV_WINDOW_HEIGHT));
        assert!(!is_usable_surface(0, DEV_WINDOW_HEIGHT));
    }

    #[test]
    fn scene_quads_use_presentation_local_and_static_platforms() {
        use purgatory_simulation::PLAYER_HALF_EXTENTS;

        let world = World::dev_stage();
        let local_pose = Some([1.5, 2.5]);
        let quads = super::scene_quads(&world, local_pose, &[]);
        assert!(quads.len() >= 5);
        let player_quad = quads
            .iter()
            .find(|quad| quad.center == [1.5, 2.5])
            .expect("presentation local player quad");
        assert_eq!(
            player_quad.size,
            [PLAYER_HALF_EXTENTS[0] * 2.0, PLAYER_HALF_EXTENTS[1] * 2.0]
        );
    }

    #[test]
    fn scene_quads_omit_local_when_pose_absent() {
        let world = World::dev_stage();
        let quads = super::scene_quads(&world, None, &[]);
        let world_player = world.player_body().expect("local world player");
        assert!(
            quads
                .iter()
                .all(|quad| quad.center != world_player.position),
            "no local pose means no local player quad"
        );
    }

    #[test]
    fn frontend_idle_does_not_catch_up_simulation() {
        use super::{rebase_wall_clock, simulation_should_advance};
        use crate::lifecycle::ClientScreen;
        use std::time::Instant;

        let mut clock = SimulationClock::new();
        let t0 = Instant::now();
        let mut last = t0;
        let after_idle = t0 + Duration::from_secs(60);
        if !simulation_should_advance(ClientScreen::Connection) {
            rebase_wall_clock(&mut last, after_idle);
        }
        rebase_wall_clock(&mut last, after_idle);
        let first = after_idle + Duration::from_millis(16);
        let elapsed = first.saturating_duration_since(last);
        let update = clock.advance(elapsed);
        assert!(
            update.ticks_executed <= 1,
            "ticks {}",
            update.ticks_executed
        );
        assert_eq!(update.discarded, Duration::ZERO);
    }
}
