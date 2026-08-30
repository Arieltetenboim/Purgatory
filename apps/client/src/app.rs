//! Client application loop: platform events, simulation clock, renderer.

use std::sync::Arc;
use std::time::{Duration, Instant};

use purgatory_content::{
    ContentRegistry, LoadMode, default_content_root, geometry_plan, load_registry,
};
use purgatory_protocol::{ReplicatedKind, ReplicationFrame, ServerInteract, move_axis_from_i8};
use purgatory_simulation::{
    Aabb, ChannelId, INTERACT_RANGE, InstanceId, MapId, PLAYER_HALF_EXTENTS, PlatformKind,
    PlayerInput, PlayerState, SimulationClock, TICK_DURATION, World, WorldAddress,
    aoi_policy_rects, in_portal_activation_zone,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::camera_follow::{CameraCommit, CameraFollow};
use crate::debug::aoi_view::{
    bind_local_player_label_pose, compact_world_space_label, replica_entity_debug_rows,
    semantic_label, world_space_label_entries, world_space_labels_eligible,
};
use crate::debug::entity_inspector::{self, WorldEntityInput};
use crate::debug::interact_status::{InteractStatusView, note_kind_change, trail_display};
use crate::debug::{
    CameraMotionDebug, CollisionHistoryEvent, ConnectionPaint, DebugOverlay, DebugSnapshot,
    DiscSubject, OverlayInit, SnapshotExtras, aoi_entity_debug_quads, camera_deadzone_quads,
    footnote_debug_quads, gameplay_receives_keyboard, gameplay_receives_pointer, is_debug_toggle,
};
use crate::frontend::ConnectionFrontend;
use crate::input::{ActionState, IntentNet};
use crate::interp::{InterpolationBuffer, PresentationPose};
use crate::jitter_forensics::{CameraJitterMode, ForensicPush, ForensicTrace};
use crate::lifecycle::{ClientLifecycle, ClientScreen};
use crate::local_presentation::{
    FrameLocalPose, LocalPresentation, PRESENTATION_SNAP_DISTANCE, extrapolate_tick_pose,
    offset_length,
};
use crate::map_fade::{
    DestinationReady, MapFade, MembershipReady, ReadinessFlags, TransitionKind, pose_stable,
    world_interaction_cleared_for_membership,
};
use crate::network::{
    ClientEndpointConfig, NetworkCommand, NetworkHandle, NetworkImpairmentConfig,
};
use crate::platform::{diagnostic_title, window_attributes};
use crate::prediction::{LocalPrediction, local_presentation_pose};
use crate::renderer::{
    Camera, DrawQuad, FOOTNOTE_LOGICAL_HEIGHT, FrameStatus, MAX_QUADS, PARALLAX_FAR, PARALLAX_MID,
    PARALLAX_NEAR, Renderer, is_usable_surface, parallax_debug_quads, parallax_quads,
};
use crate::replica::{FrameDecision, ReplicaLifecycleEvent, ReplicatedWorld};
use crate::ui_runtime::UIRuntimeState;

const PLAYER_COLOR: [f32; 4] = [0.19, 0.55, 0.66, 1.0];
const REMOTE_PLAYER_COLOR: [f32; 4] = [0.72, 0.32, 0.38, 1.0];
/// Magenta body: distinct from cyan players, brown/green platforms, and the floor.
const INTERACTABLE_BODY_COLOR: [f32; 4] = [0.92, 0.18, 0.72, 1.0];
const INTERACTABLE_CAP_COLOR: [f32; 4] = [1.0, 0.86, 0.12, 1.0];
const PORTAL_COLOR: [f32; 4] = [0.32, 0.92, 0.78, 1.0];
const INTERACTABLE_HALF: [f32; 2] = [0.4, 0.7];
const INTERP_AUTH_GIZMO_COLOR: [f32; 4] = [1.0, 0.85, 0.2, 0.55];
const PREDICT_AUTH_GIZMO_COLOR: [f32; 4] = [0.95, 0.45, 0.15, 0.55];
const PREDICT_POSE_GIZMO_COLOR: [f32; 4] = [0.25, 0.85, 0.55, 0.55];
const SOLID_FLOOR_COLOR: [f32; 4] = [0.22, 0.28, 0.24, 1.0];
const SOLID_PLATFORM_COLOR: [f32; 4] = [0.45, 0.32, 0.18, 1.0];
const ONEWAY_COLOR: [f32; 4] = [0.55, 0.62, 0.85, 1.0];
const MARKER_COLOR: [f32; 4] = [0.91, 0.69, 0.19, 1.0];

/// Source-map remotes/interactables/portals held until FadeOut is fully black.
/// Authoritative replica may already be the destination epoch.
#[derive(Clone, Debug, Default)]
struct FrozenPresentation {
    remotes: Vec<PresentationPose>,
    interactable_quads: Vec<DrawQuad>,
    portal_quads: Vec<DrawQuad>,
}

pub fn run() -> Result<(), String> {
    let registry = load_registry(&default_content_root(), LoadMode::Shared)
        .map_err(|err| format!("PURGATORY client content invalid:\n{err}"))?;
    let event_loop = EventLoop::new().map_err(|err| format!("event loop: {err}"))?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = ClientApp::new(registry);
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
    impairment_seed: u64,
    ui_runtime: UIRuntimeState,
    trace_replica_apply: bool,
    trace_scene: bool,
    trace_malformed: bool,
    last_replica_interactables: usize,
    last_interact_request: String,
    last_interact_result: String,
    last_interact_kind: Option<crate::ui_runtime::InteractKind>,
    interact_last_transition: Option<String>,
    interact_trail: Vec<&'static str>,
    registry: ContentRegistry,
    last_observer: Option<(u32, u32, u32)>,
    map_fade: MapFade,
    frozen_presentation: Option<FrozenPresentation>,
    camera_follow: CameraFollow,
    pending_camera_commit: Option<CameraCommit>,
    local_presentation: LocalPresentation,
    frame_local: FrameLocalPose,
    jitter_trace: ForensicTrace,
    reconciled_this_frame: bool,
    last_ticks_executed: u32,
    network_frames_this_frame: u32,
    last_jitter_mode: CameraJitterMode,
    dev_login: String,
}

impl ClientApp {
    fn new(registry: ContentRegistry) -> Self {
        Self {
            window: None,
            renderer: None,
            debug: None,
            clock: SimulationClock::new(),
            world: World::new(),
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
            impairment_seed: NetworkImpairmentConfig::from_env().seed,
            ui_runtime: UIRuntimeState::Idle,
            trace_replica_apply: false,
            trace_scene: false,
            trace_malformed: false,
            last_replica_interactables: 0,
            last_interact_request: String::new(),
            last_interact_result: String::new(),
            last_interact_kind: None,
            interact_last_transition: None,
            interact_trail: Vec::new(),
            registry,
            last_observer: None,
            map_fade: MapFade::default(),
            frozen_presentation: None,
            camera_follow: CameraFollow::default(),
            pending_camera_commit: None,
            local_presentation: LocalPresentation::default(),
            frame_local: FrameLocalPose::default(),
            jitter_trace: ForensicTrace::default(),
            reconciled_this_frame: false,
            last_ticks_executed: 0,
            network_frames_this_frame: 0,
            last_jitter_mode: CameraJitterMode::Normal,
            dev_login: purgatory_common::DEFAULT_DEV_LOGIN.to_string(),
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
                network.set_impairment_config(NetworkImpairmentConfig::from_profile(
                    debug.ui.impairment_profile,
                    self.impairment_seed,
                ));
                if let Some(ms) = debug.ui.impairment_stall_ms.take() {
                    let _ = network.try_trigger_input_stall(Duration::from_millis(u64::from(ms)));
                }
                if debug.ui.reset_impairment_metrics {
                    let _ = network.try_reset_impairment_metrics();
                    debug.ui.reset_impairment_metrics = false;
                }
            }
        }
        let before = self.lifecycle.screen();
        let mut events = Vec::new();
        let mut dropped = 0;
        if let Some(network) = &mut self.network {
            dropped = network.telemetry_dropped();
            self.snapshot_malformed = network.snapshot_malformed();
            network.poll(|event| events.push(event));
            for frame in network.poll_frames() {
                self.maybe_capture_source_presentation(&frame);
                match self.replica.apply_frame(frame) {
                    FrameDecision::IgnoredOlderEpoch => {}
                    FrameDecision::Applied { epoch_reset } => {
                        if epoch_reset {
                            self.interp.clear();
                            println!(
                                "6D_POSE epoch_reset epoch={} tick={} seq={:?} replica_self={:?} pred_active={} last_observer={:?} new_observer={:?} world_pose={:?} replica_pose={:?}",
                                self.replica.observer_epoch(),
                                self.replica.last_server_tick(),
                                self.replica.last_sequence(),
                                self.replica.local_player(),
                                self.prediction.active(),
                                self.last_observer,
                                self.replica.observer_address(),
                                self.world.player_body().map(|b| b.position),
                                self.replica.local_entity().map(|e| e.position),
                            );
                        }
                        self.intent.set_epoch(self.replica.input_epoch());
                        if let Err(err) = self.apply_observer_baseline_if_due() {
                            eprintln!("{err}");
                            self.fatal = Some(err);
                            return;
                        }
                        if self.replica_matches_local_map() {
                            let view = self.replica.to_snapshot_view();
                            self.interp.push(&view);
                            self.prediction.sync_from_replica(
                                &self.replica,
                                &mut self.world,
                                self.clock.tick().get(),
                            );
                            self.apply_presentation_sync_hint();
                            self.reconciled_this_frame = true;
                            self.network_frames_this_frame =
                                self.network_frames_this_frame.saturating_add(1);
                            self.trace_replica_apply_once();
                            self.note_replica_interactable_lifetime();
                        }
                    }
                }
            }
            if self.snapshot_malformed > 0 && !self.trace_malformed {
                self.trace_malformed = true;
                println!(
                    "6B_TRACE snapshot_malformed count={}",
                    self.snapshot_malformed
                );
            }
        }
        self.lifecycle.set_events_dropped(dropped);
        for event in events {
            if let crate::network::state::NetworkEvent::Interact { event, .. } = &event {
                let before = self.ui_runtime.to_string();
                println!("6B_INTERACT client_recv {event:?}");
                self.ui_runtime.apply_server(*event);
                self.note_interact_runtime();
                self.last_interact_result = interact_result_label(*event);
                println!("6B_INTERACT ui_state {before} -> {}", self.ui_runtime);
            }
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
        self.ui_runtime = UIRuntimeState::Idle;
        self.last_interact_kind = None;
        self.interact_last_transition = None;
        self.interact_trail.clear();
        self.last_interact_request.clear();
        self.last_interact_result.clear();
        if self.lifecycle.screen() != ClientScreen::Game {
            if let Some(network) = &mut self.network {
                let _ = network.poll_frames();
            }
            let before = replica_interactable_count(&self.replica);
            self.replica.clear();
            self.interp.clear();
            self.prediction.clear();
            self.local_presentation.clear();
            self.frame_local = FrameLocalPose::default();
            self.last_observer = None;
            self.frozen_presentation = None;
            self.pending_camera_commit = None;
            if before > 0 {
                println!(
                    "6B_TRACE replica_cleared screen={:?} lost_interactables={before}",
                    self.lifecycle.screen()
                );
            }
            self.trace_replica_apply = false;
            self.trace_scene = false;
            self.trace_malformed = false;
            self.last_replica_interactables = 0;
        }
        rebase_wall_clock(&mut self.last_instant, Instant::now());
    }

    fn replica_matches_local_map(&self) -> bool {
        self.last_observer == Some(self.replica.observer_address())
    }

    /// Capture source remotes before a dest-epoch `apply_frame` clears them.
    /// Does not delay replica apply, simulation, or networking.
    fn maybe_capture_source_presentation(&mut self, frame: &ReplicationFrame) {
        if self.frozen_presentation.is_some() {
            return;
        }
        if frame.observer_baseline_epoch <= self.replica.observer_epoch() {
            return;
        }
        if self.last_observer != Some(self.replica.observer_address()) {
            return;
        }
        self.frozen_presentation = Some(capture_source_presentation(&self.interp, &self.replica));
    }

    fn release_frozen_presentation_if_committed(&mut self) {
        if self.map_fade.is_fully_black() || self.map_fade.is_idle() || self.map_fade.is_fading_in()
        {
            self.frozen_presentation = None;
        }
    }

    fn presented_local_pose(&self) -> Option<[f32; 2]> {
        local_presentation_pose(
            &self.prediction,
            &self.world,
            &self.replica,
            self.replica_matches_local_map(),
        )
    }

    /// Predicted/replica source pose. Not the drawn pose.
    fn predicted_local_pose(&self) -> Option<[f32; 2]> {
        self.presented_local_pose()
    }

    fn apply_presentation_sync_hint(&mut self) {
        if !self.prediction.active() {
            self.local_presentation.clear();
            return;
        }
        if self.prediction.last_sync_hard_snap()
            || offset_length(self.prediction.last_correction_delta()) >= PRESENTATION_SNAP_DISTANCE
        {
            self.local_presentation.request_snap();
            return;
        }
        self.local_presentation
            .absorb_correction(self.prediction.last_correction_delta());
    }

    fn finalize_local_presentation(&mut self, dt: f32) {
        let predicted = self.predicted_local_pose();
        let mode = self
            .debug
            .as_ref()
            .map(|d| d.ui.camera_jitter_mode)
            .unwrap_or(CameraJitterMode::Normal);
        if mode != self.last_jitter_mode {
            self.local_presentation.request_snap();
            self.last_jitter_mode = mode;
        }
        let render_target = if mode.use_raw_presentation() {
            predicted
        } else if let Some(pose) = predicted {
            let vel = self
                .prediction
                .predicted_velocity(&self.world)
                .unwrap_or([0.0, 0.0]);
            Some(extrapolate_tick_pose(pose, vel, self.clock.remainder()))
        } else {
            None
        };
        let presented = if mode.use_raw_presentation() {
            render_target
        } else {
            let p = self.local_presentation.step(render_target, dt);
            debug_assert_eq!(p, self.local_presentation.pose());
            p
        };
        let delta = if self.reconciled_this_frame {
            self.prediction.last_correction_delta()
        } else {
            [0.0, 0.0]
        };
        self.frame_local = FrameLocalPose {
            predicted,
            presented,
            replica: self.replica.local_entity().map(|e| e.position),
            correction_delta: delta,
            reconciled: self.reconciled_this_frame,
        };
    }

    fn apply_observer_baseline_if_due(&mut self) -> Result<(), String> {
        let observer = self.replica.observer_address();
        match map_transition_step(
            self.replica.last_sequence().is_some(),
            self.last_observer,
            observer,
            self.map_fade.is_idle(),
            self.map_fade.allows_baseline_swap(),
            self.replica.local_entity().is_some(),
        ) {
            MapTransitionStep::None | MapTransitionStep::WaitFade => Ok(()),
            MapTransitionStep::StartFadeOut => {
                if self.map_fade.begin_transition(
                    TransitionKind::Map,
                    self.last_observer.unwrap_or(observer),
                    observer,
                    self.presented_local_pose(),
                ) {
                    println!(
                        "6C_PORTAL fade_start reason=authoritative_transition from={:?} to={:?} epoch={} tick={} seq={:?}",
                        self.last_observer,
                        observer,
                        self.replica.observer_epoch(),
                        self.replica.last_server_tick(),
                        self.replica.last_sequence()
                    );
                    println!(
                        "6D_LABEL hide maps_aligned=false fade=FadeOut epoch={} reason=unaligned_presentation_world",
                        self.replica.observer_epoch()
                    );
                }
                Ok(())
            }
            MapTransitionStep::StartMembershipFadeOut => {
                if self.map_fade.begin_transition(
                    TransitionKind::Membership,
                    self.last_observer.unwrap_or(observer),
                    observer,
                    self.presented_local_pose(),
                ) {
                    println!(
                        "6D_CHANNEL fade_start from={:?} to={:?} epoch={} pose={:?}",
                        self.last_observer,
                        observer,
                        self.replica.observer_epoch(),
                        self.presented_local_pose()
                    );
                }
                Ok(())
            }
            MapTransitionStep::RetargetAddress => self.retarget_local_address(observer),
            MapTransitionStep::FirstBaseline | MapTransitionStep::SwapBaseline => {
                self.rebuild_local_map(observer)
            }
        }
    }

    fn retarget_local_address(&mut self, observer: (u32, u32, u32)) -> Result<(), String> {
        let Some(last) = self.last_observer else {
            return self.rebuild_local_map(observer);
        };
        if last.0 != observer.0 {
            return self.rebuild_local_map(observer);
        }
        let from = WorldAddress::new(
            MapId::from_raw(last.0),
            ChannelId::from_raw(last.1),
            InstanceId::from_raw(last.2),
        );
        let to = WorldAddress::new(
            MapId::from_raw(observer.0),
            ChannelId::from_raw(observer.1),
            InstanceId::from_raw(observer.2),
        );
        if !self.world.rebind_map_address(from, to) {
            println!(
                "6D_CHANNEL retarget_failed from={from} to={to} epoch={} (no geometry rebuild during membership fade)",
                self.replica.observer_epoch()
            );
            return Ok(());
        }
        self.last_observer = Some(observer);
        self.pending_camera_commit = Some(CameraCommit::PreservePose);
        println!(
            "6D_CHANNEL retarget from={from} to={to} epoch={} fade={} pose={:?}",
            self.replica.observer_epoch(),
            self.map_fade.debug_phase(),
            self.world.player_body().map(|b| b.position)
        );
        Ok(())
    }

    fn rebuild_local_map(&mut self, observer: (u32, u32, u32)) -> Result<(), String> {
        let Some(auth) = self.replica.local_entity() else {
            println!(
                "6D_POSE swap_deferred reason=no_self_enter epoch={} observer={:?}",
                self.replica.observer_epoch(),
                observer
            );
            return Ok(());
        };
        let address = WorldAddress::new(
            MapId::from_raw(observer.0),
            ChannelId::from_raw(observer.1),
            InstanceId::from_raw(observer.2),
        );
        let plan = geometry_plan(&self.registry, address.map, address).map_err(|err| {
            format!(
                "PURGATORY client missing map content for MapId {}: {err}",
                observer.0
            )
        })?;
        self.world = World::new();
        self.world.instantiate_map(&plan).map_err(|err| {
            format!(
                "PURGATORY client failed to instantiate MapId {}: {err:?}",
                observer.0
            )
        })?;
        let Some(seeded) = spawn_local_player_from_replica(&mut self.world, address, &self.replica)
        else {
            println!(
                "6D_POSE swap_deferred reason=spawn_failed epoch={} auth=({:.3},{:.3})",
                self.replica.observer_epoch(),
                auth.position[0],
                auth.position[1]
            );
            return Ok(());
        };
        self.interp.clear();
        self.prediction.clear();
        self.prediction
            .sync_from_replica(&self.replica, &mut self.world, self.clock.tick().get());
        self.local_presentation.request_snap();
        let local_pose = self
            .world
            .player_body()
            .map(|b| b.position)
            .unwrap_or(seeded);
        self.last_observer = Some(observer);
        self.pending_camera_commit = Some(CameraCommit::SnapToPlayer);
        let epoch = self.replica.observer_epoch();
        println!(
            "6C_PORTAL map_ready MapId={} address={} epoch={} tick={} fade_swap=true",
            observer.0,
            address,
            epoch,
            self.replica.last_server_tick()
        );
        println!(
            "6D_POSE map_swapped epoch={} pose=({:.3},{:.3}) auth=({:.3},{:.3}) pred_active={}",
            epoch,
            local_pose[0],
            local_pose[1],
            auth.position[0],
            auth.position[1],
            self.prediction.active()
        );
        println!(
            "6C_TRACE local_map MapId={} address={} name={}",
            observer.0,
            address,
            self.registry
                .map_by_map_id(address.map)
                .map(|m| m.debug_name.as_str())
                .unwrap_or("-")
        );
        Ok(())
    }

    fn maybe_notify_transition_ready(&mut self) {
        if self.map_fade.is_idle() {
            return;
        }
        let flags = self.readiness_flags();
        self.map_fade.set_flags(flags);
        match self.map_fade.kind() {
            TransitionKind::Map => {
                if flags.map_complete() && self.map_fade.destination_ready().is_none() {
                    let local_pose = self.presented_local_pose().unwrap_or([0.0, 0.0]);
                    self.map_fade.notify_destination_ready(DestinationReady {
                        epoch: self.replica.observer_epoch(),
                        local_pose,
                    });
                    println!(
                        "6D_POSE destination_ready epoch={} pose=({:.3},{:.3}) pred_active={}",
                        self.replica.observer_epoch(),
                        local_pose[0],
                        local_pose[1],
                        self.prediction.active()
                    );
                    println!(
                        "6D_LABEL show maps_aligned=true fade={} dest_ready=true epoch={} pose=({:.3},{:.3})",
                        self.map_fade.debug_phase(),
                        self.replica.observer_epoch(),
                        local_pose[0],
                        local_pose[1]
                    );
                }
            }
            TransitionKind::Membership => {
                if flags.membership_complete() && self.map_fade.membership_ready().is_none() {
                    let addr = self.replica.observer_address();
                    self.map_fade.notify_membership_ready(MembershipReady {
                        epoch: self.replica.observer_epoch(),
                        address: addr,
                    });
                    println!(
                        "6D_CHANNEL membership_ready epoch={} address={:?} pose={:?}",
                        self.replica.observer_epoch(),
                        addr,
                        self.presented_local_pose()
                    );
                }
            }
        }
    }

    fn readiness_flags(&self) -> ReadinessFlags {
        let observer = self.replica.observer_address();
        let dest = self.map_fade.dest_address();
        let dest_set = dest != (0, 0, 0);
        let observer_accepted = !dest_set || observer == dest;
        let presentation_matches = self.last_observer == Some(observer);
        let self_entity = self.replica.local_entity();
        let local_pose = self.presented_local_pose();
        let auth_pose = self_entity.map(|e| e.position);
        let pose_ok = match (
            self.map_fade.kind(),
            self.map_fade.pose_at_start(),
            local_pose,
        ) {
            (TransitionKind::Membership, Some(start), Some(now)) => pose_stable(start, now),
            (TransitionKind::Map, _, Some(now)) => {
                auth_pose.is_some_and(|auth| pose_stable(auth, now))
            }
            _ => local_pose.is_some(),
        };
        ReadinessFlags {
            observer_accepted,
            epoch_applied: self.replica.last_sequence().is_some() && observer_accepted,
            geometry_ready: presentation_matches
                && self
                    .world
                    .iter_kind(purgatory_simulation::EntityKind::Platform)
                    .count()
                    > 0,
            self_baseline: self_entity.is_some(),
            local_seeded: pose_ok,
            prediction_synced: presentation_matches,
            camera_ready: presentation_matches || self.renderer.is_none(),
            old_replicas_cleared: presentation_matches || self.interp.poses().is_empty(),
            presentation_matches,
            session_closed: world_interaction_cleared_for_membership(self.ui_runtime.kind().name()),
        }
    }

    fn trace_replica_apply_once(&mut self) {
        if self.trace_replica_apply {
            return;
        }
        self.trace_replica_apply = true;
        let mut n = 0u32;
        for entity in self.replica.iter() {
            if entity.kind != ReplicatedKind::Interactable && entity.kind != ReplicatedKind::Portal
            {
                continue;
            }
            n += 1;
            println!(
                "6B_TRACE replica_apply id={} kind={:?} pos=({:.3},{:.3})",
                entity.entity_id, entity.kind, entity.position[0], entity.position[1]
            );
        }
        if n == 0 {
            println!(
                "6B_TRACE replica_apply interactable_count=0 entities={} seq={:?} malformed={}",
                self.replica.len(),
                self.replica.last_sequence(),
                self.snapshot_malformed
            );
        }
    }

    fn note_replica_interactable_lifetime(&mut self) {
        let n = replica_interactable_count(&self.replica);
        if self.last_replica_interactables > 0 && n == 0 {
            println!(
                "6B_TRACE replica_lost_interactables before={} after=0 seq={:?} screen={:?}",
                self.last_replica_interactables,
                self.replica.last_sequence(),
                self.lifecycle.screen()
            );
        }
        self.last_replica_interactables = n;
    }

    fn trace_scene_once(
        &mut self,
        camera: &Camera,
        local_pose: Option<[f32; 2]>,
        interactable_quads: usize,
        total_quads_before_upload: usize,
    ) {
        if self.trace_scene {
            return;
        }
        if self.replica.applied == 0 {
            return;
        }
        self.trace_scene = true;
        let mut n = 0u32;
        for entity in self.replica.iter() {
            if entity.kind != ReplicatedKind::Interactable && entity.kind != ReplicatedKind::Portal
            {
                continue;
            }
            n += 1;
            let ndc = camera.world_to_ndc(entity.position);
            println!(
                "6B_TRACE scene_quad id={} pos=({:.3},{:.3}) ndc=({:.3},{:.3}) camera=({:.3},{:.3}) player={:?} viewport=({:.3},{:.3})",
                entity.entity_id,
                entity.position[0],
                entity.position[1],
                ndc[0],
                ndc[1],
                camera.position[0],
                camera.position[1],
                local_pose,
                camera.viewport_width,
                camera.viewport_height
            );
        }
        if n == 0 {
            println!(
                "6B_TRACE scene_quad interactable_count=0 camera=({:.3},{:.3}) player={:?}",
                camera.position[0], camera.position[1], local_pose
            );
        }
        println!("6B_TRACE rendered_interactable_quads={interactable_quads}");
        if total_quads_before_upload > MAX_QUADS {
            println!("6B_TRACE quad_truncation total={total_quads_before_upload} max={MAX_QUADS}");
        }
    }

    fn request_connect(&mut self) {
        let Some(id) = self.lifecycle.try_begin_connect() else {
            return;
        };
        println!("PURGATORY connect requested attempt={id}");
        let sent = self.network.as_ref().is_some_and(|net| {
            net.try_send(NetworkCommand::Connect {
                attempt_id: id,
                dev_login: self.dev_login.clone(),
            })
        });
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

    #[must_use]
    fn gameplay_input_locked(&self) -> bool {
        transition_gameplay_input_locked(
            self.map_fade.gameplay_input_locked(),
            self.last_observer,
            self.replica.observer_address(),
            self.replica.last_sequence().is_some(),
        )
    }

    fn sample_tick_input(&mut self) -> PlayerInput {
        if self.gameplay_input_locked() {
            self.actions.discard_locked_edges();
            if let Some((_, player)) = self.world.player_parts_mut() {
                player.velocity = [0.0, 0.0];
            }
            PlayerInput::idle()
        } else {
            self.actions.consume_tick_input()
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
        let Some(command) = self.intent.emit_tick_with_portal(
            move_axis_from_i8(input.move_axis),
            input.jump_pressed,
            input.down_held,
            self.actions.portal_held(),
        ) else {
            return;
        };
        if !self.prediction.try_push_pending(command) {
            return;
        }
        let _ = network.try_send_input(command);
    }

    fn poll_interact_request(&mut self) {
        if !self.actions.consume_interact_edge() {
            return;
        }
        println!("6B_INTERACT input_e pressed=true");
        if !self.lifecycle.gameplay_actions_allowed() {
            self.last_interact_request = "ignored (not in Game)".into();
            println!("6B_INTERACT ignored screen={:?}", self.lifecycle.screen());
            return;
        }
        if self.gameplay_input_locked() {
            self.last_interact_request = "ignored (transition input lock)".into();
            println!("6B_INTERACT ignored reason=transition_input_lock");
            return;
        }
        if self.network.is_none() {
            self.last_interact_request = "ignored (no network)".into();
            println!("6B_INTERACT ignored reason=no_network");
            return;
        }
        match self.ui_runtime {
            UIRuntimeState::Opening { .. } | UIRuntimeState::Closing { .. } => {
                self.last_interact_request = "ignored (in flight)".into();
                println!("6B_INTERACT ignored reason=in_flight");
                return;
            }
            UIRuntimeState::Active { session_id, target } => {
                self.last_interact_request = format!("Close {session_id}");
                self.ui_runtime.begin_close(session_id, target);
                self.note_interact_runtime();
                println!("6B_INTERACT send InteractClose session={session_id}");
                if !self
                    .network
                    .as_ref()
                    .expect("checked")
                    .try_send_interact_close(session_id)
                {
                    self.last_interact_result = "send failed".into();
                    println!("6B_INTERACT send failed InteractClose");
                }
                return;
            }
            UIRuntimeState::Idle
            | UIRuntimeState::Rejected { .. }
            | UIRuntimeState::Closed { .. } => {}
        }
        let routing = replica_interact_routing(&self.replica);
        log_interact_routing("6B_INTERACT", &routing);
        match interact_open_send_target(&routing) {
            Some(target) => {
                self.last_interact_request = format!("Open {target}");
                self.ui_runtime.begin_open(target);
                self.note_interact_runtime();
                println!("6B_INTERACT send InteractOpen target={target}");
                println!("6B_INTERACT ui_state Idle -> {}", self.ui_runtime);
                if !self
                    .network
                    .as_ref()
                    .expect("checked")
                    .try_send_interact_open(target)
                {
                    self.last_interact_result = "send failed".into();
                    println!("6B_INTERACT send failed InteractOpen target={target}");
                }
            }
            None => {
                println!("6B_INTERACT send InteractOpen skipped (no in-range generic)");
                self.last_interact_request = "Open (no in-range generic)".into();
                self.last_interact_result = "none".into();
            }
        }
    }

    fn note_interact_runtime(&mut self) {
        let next = self.ui_runtime.kind();
        if let Some(transition) =
            note_kind_change(self.last_interact_kind, next, &mut self.interact_trail)
        {
            self.interact_last_transition = Some(transition);
        }
        self.last_interact_kind = Some(next);
    }

    fn poll_portal_request(&mut self) {
        if !self.actions.consume_portal_edge() {
            return;
        }
        println!(
            "6C_PORTAL up_pressed eligible_check fade_alpha={:.3} epoch={} tick={} seq={:?}",
            self.map_fade.alpha(),
            self.replica.observer_epoch(),
            self.replica.last_server_tick(),
            self.replica.last_sequence()
        );
        if !self.lifecycle.gameplay_actions_allowed() {
            println!("6C_PORTAL up_ignored reason=not_in_game");
            return;
        }
        if self.gameplay_input_locked() {
            println!("6C_PORTAL up_ignored reason=input_gated");
            return;
        }
        if !self.map_fade.is_idle() {
            println!("6C_PORTAL up_ignored reason=fade_in_progress");
            return;
        }
        let Some(network) = self.network.as_ref() else {
            return;
        };
        let routing = replica_interact_routing(&self.replica);
        log_interact_routing("6C_PORTAL", &routing);
        match portal_activate_send_target(&routing) {
            Some(target) => {
                println!(
                    "6C_PORTAL send PortalActivate target={target} fade=idle presented={:?} replica={:?} pred_active={}",
                    self.presented_local_pose(),
                    self.replica.local_entity().map(|e| e.position),
                    self.prediction.active()
                );
                if !network.try_send_portal_activate(target) {
                    self.last_interact_result = "portal send failed".into();
                    println!("6C_PORTAL send_failed target={target}");
                }
            }
            None => {
                println!("6C_PORTAL send_skipped reason=not_in_activation_zone");
            }
        }
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
        self.last_ticks_executed = update.ticks_executed;
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
            self.local_presentation.request_snap();
            if !self.prediction.pending_window_full() {
                let input = self.sample_tick_input();
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
            let input = self.sample_tick_input();
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

    fn update_camera(&mut self, dt: f32) {
        let bounds = self.world.bounds();
        let mode = self
            .debug
            .as_ref()
            .map(|d| d.ui.camera_jitter_mode)
            .unwrap_or(CameraJitterMode::Normal);
        let player_pos = if mode.follow_raw_prediction() {
            self.frame_local.predicted.unwrap_or([0.0, 0.0])
        } else {
            self.frame_local.presented.unwrap_or([0.0, 0.0])
        };
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
        let commit = self.pending_camera_commit.take();
        let mut camera = renderer.camera();
        let previous = camera.position;
        let follow_now = follow || center_request;
        let snap = center_request || matches!(commit, Some(CameraCommit::SnapToPlayer));
        if matches!(commit, Some(CameraCommit::PreservePose)) {
            self.camera_follow.seed_from(camera.position);
        }
        let freeze = mode.freeze_camera() && !snap;
        let raw_follow = if freeze {
            self.camera_follow.seed_from(camera.position);
            Some(self.camera_follow.desired)
        } else if follow_now {
            if snap {
                self.camera_follow.snap(&mut camera, player_pos, bounds);
            } else {
                self.camera_follow.step(&mut camera, player_pos, bounds, dt);
            }
            Some(self.camera_follow.desired)
        } else {
            camera.clamp_to_bounds(bounds);
            self.camera_follow.seed_from(camera.position);
            None
        };
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

    fn record_jitter_sample(&mut self, dt: f32) {
        let renderer = self.renderer.as_ref();
        let camera = renderer.map(Renderer::camera).unwrap_or(Camera {
            position: [0.0, 0.0],
            viewport_width: 16.0,
            viewport_height: 9.0,
        });
        let window_w = renderer
            .map(|r| r.surface_size().0 as f32)
            .unwrap_or(1280.0);
        let tick_alpha = TICK_DURATION.as_secs_f32().max(1e-6);
        let alpha = self.clock.remainder().as_secs_f32() / tick_alpha;
        self.jitter_trace.push(
            self.frame_local,
            ForensicPush {
                camera: camera.position,
                desired: self.camera_follow.desired,
                following_x: self.camera_follow.following_x,
                offset: self.local_presentation.offset(),
                dt,
                ticks: self.last_ticks_executed,
                net_frames: self.network_frames_this_frame,
                epoch: self.replica.observer_epoch(),
                replica_seq: self.replica.last_sequence().unwrap_or(0),
                fade: self.map_fade.debug_phase(),
                tick_alpha: alpha,
                viewport_width: camera.viewport_width,
                window_width: window_w,
            },
        );
    }

    fn maybe_dump_jitter_trace(&mut self) {
        if !self.debug.as_ref().is_some_and(|d| d.ui.dump_jitter_trace) {
            return;
        }
        if let Some(debug) = self.debug.as_mut() {
            debug.ui.dump_jitter_trace = false;
        }
        match self.jitter_trace.dump_csv() {
            Ok(path) => {
                let summary = self.jitter_trace.summary(self.local_presentation.offset());
                let msg = format!(
                    "wrote {} (n≤180 max|Δscr|={:.4} mean|Δpres| tick={:.4} idle={:.4})",
                    path.display(),
                    summary.max_d_screen_x,
                    summary.mean_d_presented_on_tick,
                    summary.mean_d_presented_idle
                );
                if let Some(debug) = self.debug.as_mut() {
                    debug.ui.last_jitter_dump = msg;
                }
            }
            Err(err) => {
                if let Some(debug) = self.debug.as_mut() {
                    debug.ui.last_jitter_dump = format!("dump failed: {err}");
                }
            }
        }
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
        self.reconciled_this_frame = false;
        self.network_frames_this_frame = 0;
        self.last_ticks_executed = 0;
        self.poll_network();
        let fade_dt = Instant::now()
            .saturating_duration_since(self.last_instant)
            .as_secs_f32();
        if simulation_should_advance(self.lifecycle.screen()) {
            self.advance_simulation();
        } else {
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(self.last_instant);
            rebase_wall_clock(&mut self.last_instant, now);
            let seconds = elapsed.as_secs_f32();
            self.fps = if seconds > 0.0 { 1.0 / seconds } else { 0.0 };
        }
        let fade_before = self.map_fade.debug_phase();
        if let Some(stall) = self.map_fade.tick(fade_dt) {
            println!("{stall}");
        }
        if let Err(err) = self.apply_observer_baseline_if_due() {
            eprintln!("{err}");
            self.fatal = Some(err);
            return;
        }
        self.release_frozen_presentation_if_committed();
        if simulation_should_advance(self.lifecycle.screen()) {
            self.finalize_local_presentation(fade_dt);
            self.update_camera(fade_dt);
            self.record_jitter_sample(fade_dt);
            self.maybe_dump_jitter_trace();
        }
        self.maybe_notify_transition_ready();
        if fade_before != "FadeIn" && self.map_fade.debug_phase() == "FadeIn" {
            let pose = self.presented_local_pose();
            println!(
                "6D_POSE fade_in_visible epoch={} pose={:?} ready={:?} fade_alpha={:.3}",
                self.replica.observer_epoch(),
                pose,
                self.map_fade.destination_ready(),
                self.map_fade.alpha()
            );
            println!(
                "6D_INPUT unlock reason=fade_in kind={:?}",
                self.map_fade.kind()
            );
        }
        self.poll_interact_request();
        self.poll_portal_request();

        let overlay_open = self.debug_overlay_visible();
        let on_connection = self.lifecycle.screen() == ClientScreen::Connection;
        let camera = self.renderer.as_ref().map(Renderer::camera);
        let Some(camera) = camera else {
            return;
        };

        let mut quads = Vec::new();
        if !on_connection {
            self.interp.sample(Instant::now());
            let local_pose = self.frame_local.presented;
            let predicted_pose = self.frame_local.predicted;
            quads = parallax_quads(&camera, self.world.bounds());
            let hold_source = self.map_fade.holds_source_presentation();
            let replica_live = self.replica_matches_local_map() && !hold_source;
            let remotes: &[crate::interp::PresentationPose] = visible_remote_poses(
                hold_source,
                self.frozen_presentation.as_ref(),
                replica_live,
                &self.interp,
            );
            quads.extend(scene_quads(&self.world, local_pose, remotes));
            let replica_interactable_quads = visible_interactable_quads(
                hold_source,
                self.frozen_presentation.as_ref(),
                replica_live,
                &self.replica,
            );
            let replica_portal_quads = visible_portal_quads(
                hold_source,
                self.frozen_presentation.as_ref(),
                replica_live,
                &self.replica,
            );
            let interactable_n = replica_interactable_quads.len() + replica_portal_quads.len();
            quads.extend(replica_interactable_quads);
            quads.extend(replica_portal_quads);
            self.trace_scene_once(&camera, local_pose, interactable_n, quads.len());
            if overlay_open {
                let ui = self
                    .debug
                    .as_ref()
                    .map(|d| d.ui.clone())
                    .unwrap_or_default();
                quads.extend(footnote_debug_quads(&self.world, &ui, local_pose));
                if replica_live {
                    let origin = local_pose.unwrap_or([0.0, 0.0]);
                    let rects = aoi_policy_rects(origin, self.world.bounds());
                    let interp_poses = self.interp.poses();
                    let rows = replica_entity_debug_rows(&self.replica, rects, |id| {
                        interp_poses
                            .iter()
                            .find(|p| p.entity_id == id)
                            .map(|p| p.position)
                            .or_else(|| {
                                if self.replica.local_player() == Some(id) {
                                    local_pose
                                } else {
                                    None
                                }
                            })
                    });
                    quads.extend(aoi_entity_debug_quads(&rows));
                }
                if ui.show_parallax_debug {
                    quads.extend(parallax_debug_quads(&camera));
                }
                if ui.show_interpolation_gizmos && replica_live {
                    quads.extend(interpolation_gizmos(&self.replica, self.interp.poses()));
                }
                if ui.show_prediction_gizmos && replica_live {
                    quads.extend(prediction_gizmos(
                        &self.replica,
                        predicted_pose,
                        self.prediction.active(),
                    ));
                }
                if ui.show_camera_deadzone {
                    quads.extend(camera_deadzone_quads(
                        camera.position,
                        local_pose.unwrap_or([0.0, 0.0]),
                        self.camera_follow.desired,
                        self.camera_follow.config.half_x,
                        self.camera_follow.config.half_y,
                    ));
                }
            }
        }
        if let Some(fade) = self.map_fade.overlay_quad(&camera) {
            quads.push(fade);
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
                    stage_name: "content",
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
        snapshot.map_id = self.last_observer.map(|(m, _, _)| m).unwrap_or(0);
        snapshot.map_debug_name = self
            .registry
            .map_by_map_id(MapId::from_raw(snapshot.map_id))
            .map(|m| m.debug_name.clone())
            .unwrap_or_else(|| "-".into());
        let replica_addr = self.replica.observer_address();
        snapshot.observer_map = replica_addr.0;
        snapshot.observer_channel = replica_addr.1;
        snapshot.observer_instance = replica_addr.2;
        snapshot.transition_banner = self.map_fade.debug_banner().unwrap_or_default();
        snapshot.transition_missing = if self.map_fade.is_idle() {
            String::new()
        } else {
            self.map_fade.flags().missing_csv(self.map_fade.kind())
        };
        snapshot.transition_stalled = self.map_fade.stalled();
        let input_locked = self.gameplay_input_locked();
        snapshot.input_gate_label = input_gate_debug_label(
            input_locked,
            self.map_fade.is_idle(),
            matches!(self.map_fade.kind(), TransitionKind::Map),
            self.last_observer,
            replica_addr,
        )
        .into();
        snapshot.input_movement_neutral = input_locked;
        snapshot.observer_address = if self.replica.last_sequence().is_some() {
            format!(
                "map={} ch={} inst={}",
                replica_addr.0, replica_addr.1, replica_addr.2
            )
        } else {
            self.last_observer
                .map(|(m, c, i)| format!("map={m} ch={c} inst={i}"))
                .unwrap_or_else(|| "-".into())
        };
        snapshot.content_registry_count = self.registry.definition_count() as u32;
        snapshot.content_map_labels = self
            .registry
            .iter_maps()
            .map(|m| {
                format!(
                    "{} => MapId {}",
                    m.authored_id,
                    self.registry
                        .map_id(m.content_id)
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".into())
                )
            })
            .collect();
        snapshot.network = self.lifecycle.snapshot();
        snapshot.net_input_seq = self.intent.sequence;
        snapshot.net_input_sent = self.intent.commands_sent;
        snapshot.net_move_axis = self.intent.move_axis.to_i8();
        snapshot.net_jump = self.intent.jump_pressed;
        snapshot.net_down = self.intent.down_held;
        snapshot.replica_seq = self.replica.last_sequence();
        snapshot.replica_tick = self.replica.last_server_tick();
        snapshot.replica_entities = self.replica.len() as u32;
        snapshot.replica_epoch = self.replica.observer_epoch();
        snapshot.replica_frame_enters = self.replica.last_frame_enters();
        snapshot.replica_frame_updates = self.replica.last_frame_updates();
        snapshot.replica_frame_leaves = self.replica.last_frame_leaves();
        snapshot.replica_total_enters = self.replica.total_enters();
        snapshot.replica_total_updates = self.replica.total_updates();
        snapshot.replica_total_leaves = self.replica.total_leaves();
        snapshot.replica_local = self.replica.local_player().map(|id| id.to_string());
        snapshot.observer_entity = self.replica.local_player().map(|id| id.to_string());
        let sim_local_pose = self.presented_local_pose();
        let origin = sim_local_pose.unwrap_or([0.0, 0.0]);
        let rects = aoi_policy_rects(origin, self.world.bounds());
        snapshot.observer_enter_bounds = format!(
            "[{:.1},{:.1}] x [{:.1},{:.1}]",
            rects.enter.min_x(),
            rects.enter.max_x(),
            rects.enter.min_y(),
            rects.enter.max_y()
        );
        snapshot.observer_leave_bounds = format!(
            "[{:.1},{:.1}] x [{:.1},{:.1}]",
            rects.leave.min_x(),
            rects.leave.max_x(),
            rects.leave.min_y(),
            rects.leave.max_y()
        );
        if let Some(dbg) = self.replica.aoi_debug() {
            snapshot.aoi_candidates = Some(dbg.candidates);
            snapshot.aoi_known = Some(dbg.known);
            snapshot.aoi_want_enter = Some(dbg.want_enter);
            snapshot.aoi_want_leave = Some(dbg.want_leave);
        }
        let interp_poses = self.interp.poses();
        let rows = replica_entity_debug_rows(&self.replica, rects, |id| {
            if self.replica.local_player() == Some(id) {
                self.frame_local.presented.or(sim_local_pose)
            } else {
                interp_poses
                    .iter()
                    .find(|p| p.entity_id == id)
                    .map(|p| p.position)
            }
        });
        snapshot.inspector = entity_inspector::build(
            snapshot.entities.iter().map(WorldEntityInput::from),
            snapshot.player.map(|p| p.id),
            &rows,
        );
        snapshot.replica_entity_rows = rows
            .iter()
            .map(|r| {
                let recent = r.recent.map(|s| format!(" · {s}")).unwrap_or_default();
                format!("{} | {}{recent}", r.label, r.band_label)
            })
            .collect();
        let labels_eligible = world_space_labels_eligible(
            self.replica_matches_local_map(),
            self.map_fade.is_idle(),
            self.map_fade.presentation_ready(),
        );
        snapshot.replica_label_world = world_space_label_entries(labels_eligible, &rows, |r| {
            compact_world_space_label(r.role).to_string()
        });
        bind_local_player_label_pose(
            &mut snapshot.replica_label_world,
            self.frame_local.presented,
        );
        snapshot.replica_recent_left = self
            .replica
            .recent_lifecycle()
            .filter(|n| n.event == ReplicaLifecycleEvent::Left)
            .map(|n| {
                format!(
                    "{} Left @ tick {}",
                    semantic_label(n.kind, n.entity_id, false),
                    n.tick
                )
            })
            .collect();
        snapshot.replica_stale = self.replica.stale_ignored;
        snapshot.replica_duplicate = self.replica.duplicate_ignored;
        snapshot.replica_malformed = self.snapshot_malformed;
        snapshot.interact_ui = self.ui_runtime.to_string();
        snapshot.interact_status = InteractStatusView::from_runtime(
            &self.ui_runtime,
            self.interact_last_transition.clone(),
            &trail_display(&self.interact_trail),
        );
        snapshot.interact_last_request = if self.last_interact_request.is_empty() {
            "-".into()
        } else {
            self.last_interact_request.clone()
        };
        snapshot.interact_last_result = if self.last_interact_result.is_empty() {
            "-".into()
        } else {
            self.last_interact_result.clone()
        };
        let routing = replica_interact_routing(&self.replica);
        snapshot.interact_nearest = routing.nearest_generic.map(|(id, _)| id.to_string());
        snapshot.interact_nearest_distance = routing.nearest_generic.map(|(_, dist)| dist);
        snapshot.interact_nearest_portal = routing.nearest_portal.map(|(id, _)| id.to_string());
        snapshot.interact_nearest_portal_distance = routing.nearest_portal.map(|(_, dist)| dist);
        snapshot.portal_eligible = routing.portal_eligible;
        snapshot.replica_interactables =
            replica_kind_labels(&self.replica, ReplicatedKind::Interactable);
        snapshot.replica_portals = replica_kind_labels(&self.replica, ReplicatedKind::Portal);
        snapshot.replica_player_pos = self.replica.local_entity().map(|e| e.position);
        snapshot.presented_player_pos = self.frame_local.presented;
        snapshot.camera_desired = self.camera_follow.desired;
        snapshot.camera_deadzone_half_x = self.camera_follow.config.half_x;
        snapshot.camera_deadzone_half_y = self.camera_follow.config.half_y;
        snapshot.camera_smooth_time_x = self.camera_follow.config.smooth_time_x;
        snapshot.camera_smooth_time_y = self.camera_follow.config.smooth_time_y;
        snapshot.camera_following_x = self.camera_follow.following_x;
        snapshot.camera_following_y = self.camera_follow.following_y;
        snapshot.jitter = self.jitter_trace.summary(self.local_presentation.offset());
        snapshot.nearest_portal_pos = routing.nearest_portal_pos;
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
        snapshot.pred_reconcile_count = pred.total_reconciliation_count;
        snapshot.pred_last_correction = pred.last_correction_wu;
        snapshot.pred_max_correction = pred.max_correction_wu;
        snapshot.pred_ack_delta = pred.observed_ack_delta;
        snapshot.pred_ack_jump_count = pred.observed_ack_jump_count;
        snapshot.pred_max_ack_delta = pred.max_observed_ack_delta;
        if let Some(network) = &mut self.network {
            snapshot.impairment = network.poll_impairment_metrics();
        }

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
            let login = &mut self.dev_login;
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
                            login,
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
                self.local_presentation.request_snap();
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
            let requested_channel = debug.ui.request_channel.take();
            if connect {
                self.request_connect();
            }
            if disconnect {
                self.request_disconnect();
            }
            if let Some(channel) = requested_channel
                && let Some(network) = &self.network
            {
                println!("6D_CHANNEL send DevSetChannel channel={channel}");
                if !network.try_send_dev_set_channel(channel) {
                    eprintln!("6D_CHANNEL send failed (input channel full or closed)");
                }
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

fn capture_source_presentation(
    interp: &crate::interp::InterpolationBuffer,
    replica: &crate::replica::ReplicatedWorld,
) -> FrozenPresentation {
    let mut remotes = interp.poses().to_vec();
    if remotes.is_empty() {
        remotes = replica
            .iter()
            .filter(|entity| {
                entity.kind == ReplicatedKind::Player
                    && replica.local_player() != Some(entity.entity_id)
            })
            .map(|entity| PresentationPose {
                entity_id: entity.entity_id,
                position: entity.position,
            })
            .collect();
    }
    FrozenPresentation {
        remotes,
        interactable_quads: interactable_quads(replica),
        portal_quads: portal_quads(replica),
    }
}

#[must_use]
fn visible_remote_poses<'a>(
    hold_source: bool,
    frozen: Option<&'a FrozenPresentation>,
    replica_live: bool,
    interp: &'a crate::interp::InterpolationBuffer,
) -> &'a [PresentationPose] {
    if hold_source {
        frozen.map(|f| f.remotes.as_slice()).unwrap_or(&[])
    } else if replica_live {
        interp.poses()
    } else {
        &[]
    }
}

fn visible_interactable_quads(
    hold_source: bool,
    frozen: Option<&FrozenPresentation>,
    replica_live: bool,
    replica: &crate::replica::ReplicatedWorld,
) -> Vec<DrawQuad> {
    if hold_source {
        frozen
            .map(|f| f.interactable_quads.clone())
            .unwrap_or_default()
    } else if replica_live {
        interactable_quads(replica)
    } else {
        Vec::new()
    }
}

fn visible_portal_quads(
    hold_source: bool,
    frozen: Option<&FrozenPresentation>,
    replica_live: bool,
    replica: &crate::replica::ReplicatedWorld,
) -> Vec<DrawQuad> {
    if hold_source {
        frozen.map(|f| f.portal_quads.clone()).unwrap_or_default()
    } else if replica_live {
        portal_quads(replica)
    } else {
        Vec::new()
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
    quads.push(DrawQuad::rect(
        [b.min_x + 0.6, b.max_y - 0.6],
        [0.35, 0.35],
        MARKER_COLOR,
    ));
    quads
}

fn interactable_marker_at(position: [f32; 2]) -> [DrawQuad; 2] {
    let aabb = Aabb::new(position, INTERACTABLE_HALF);
    [
        aabb_quad(aabb, INTERACTABLE_BODY_COLOR),
        DrawQuad::rect(
            [position[0], position[1] + INTERACTABLE_HALF[1] + 0.16],
            [0.36, 0.32],
            INTERACTABLE_CAP_COLOR,
        ),
    ]
}

fn interactable_quads(replica: &ReplicatedWorld) -> Vec<DrawQuad> {
    replica
        .iter()
        .filter(|entity| entity.kind == ReplicatedKind::Interactable)
        .flat_map(|entity| interactable_marker_at(entity.position))
        .collect()
}

fn portal_quads(replica: &ReplicatedWorld) -> Vec<DrawQuad> {
    replica
        .iter()
        .filter(|entity| entity.kind == ReplicatedKind::Portal)
        .map(|entity| DrawQuad::triangle(entity.position, [0.9, 1.4], PORTAL_COLOR))
        .collect()
}

fn replica_interactable_count(replica: &ReplicatedWorld) -> usize {
    replica
        .iter()
        .filter(|entity| entity.kind == ReplicatedKind::Interactable)
        .count()
}

/// Rebuild local map geometry only after a real snapshot observer address exists.
/// An empty replica defaults to MapId 0, which is not a registry map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MapTransitionStep {
    None,
    FirstBaseline,
    StartFadeOut,
    StartMembershipFadeOut,
    WaitFade,
    SwapBaseline,
    RetargetAddress,
}

fn map_transition_step(
    replica_has_snapshot: bool,
    last_observer: Option<(u32, u32, u32)>,
    observer: (u32, u32, u32),
    fade_idle: bool,
    allows_swap: bool,
    self_enter_ready: bool,
) -> MapTransitionStep {
    if !replica_has_snapshot {
        return MapTransitionStep::None;
    }
    if last_observer == Some(observer) {
        return MapTransitionStep::None;
    }
    if last_observer.is_none() {
        if self_enter_ready {
            return MapTransitionStep::FirstBaseline;
        }
        return MapTransitionStep::None;
    }
    let last = last_observer.expect("checked");
    if last.0 == observer.0 {
        if fade_idle {
            return MapTransitionStep::StartMembershipFadeOut;
        }
        if allows_swap && self_enter_ready {
            return MapTransitionStep::RetargetAddress;
        }
        return MapTransitionStep::WaitFade;
    }
    if fade_idle {
        return MapTransitionStep::StartFadeOut;
    }
    if allows_swap && self_enter_ready {
        return MapTransitionStep::SwapBaseline;
    }
    MapTransitionStep::WaitFade
}

#[must_use]
fn transition_gameplay_input_locked(
    fade_locked: bool,
    last_observer: Option<(u32, u32, u32)>,
    replica_observer: (u32, u32, u32),
    replica_has_sequence: bool,
) -> bool {
    if fade_locked {
        return true;
    }
    replica_has_sequence && last_observer.is_some_and(|last| last != replica_observer)
}

#[must_use]
fn input_gate_debug_label(
    locked: bool,
    fade_idle: bool,
    fade_kind_map: bool,
    last_observer: Option<(u32, u32, u32)>,
    replica_observer: (u32, u32, u32),
) -> &'static str {
    if !locked {
        return "INPUT: ACTIVE";
    }
    if !fade_idle {
        return if fade_kind_map {
            "INPUT: LOCKED · MAP TRANSITION"
        } else {
            "INPUT: LOCKED · CHANNEL TRANSITION"
        };
    }
    if last_observer.is_some_and(|last| last.0 != replica_observer.0) {
        "INPUT: LOCKED · MAP TRANSITION"
    } else {
        "INPUT: LOCKED · CHANNEL TRANSITION"
    }
}

/// Seed the client World player from the observer Known self Enter. Never uses
/// the first platform's center as a stand-in destination pose.
fn spawn_local_player_from_replica(
    world: &mut World,
    address: WorldAddress,
    replica: &ReplicatedWorld,
) -> Option<[f32; 2]> {
    let auth = replica.local_entity()?;
    let x = auth.position[0];
    let floor = world
        .iter_platforms()
        .find(|v| {
            world
                .address_of(v.id)
                .is_some_and(|a| a.compatible_with(address))
                && x >= v.aabb().min_x()
                && x <= v.aabb().max_x()
        })
        .or_else(|| {
            world.iter_platforms().find(|v| {
                world
                    .address_of(v.id)
                    .is_some_and(|a| a.compatible_with(address))
            })
        })?;
    let (transform, state) = PlayerState::standing_on_at(floor.id, floor.top_surface(), x);
    let _ = world.spawn_player_at(address, transform, state);
    world.restore_player_sim_state(
        auth.position,
        auth.velocity,
        replica.local_grounded(),
        replica.local_grounded_on().get(),
        replica.local_ignored_platform().get(),
    );
    world.player_body().map(|b| b.position)
}

fn replica_kind_labels(replica: &ReplicatedWorld, kind: ReplicatedKind) -> Vec<String> {
    replica
        .iter()
        .filter(|entity| entity.kind == kind)
        .map(|entity| {
            format!(
                "{} ({:.1}, {:.1})",
                entity.entity_id, entity.position[0], entity.position[1]
            )
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default)]
struct ReplicaInteractRouting {
    nearest_generic: Option<(purgatory_protocol::WireEntityId, f32)>,
    nearest_portal: Option<(purgatory_protocol::WireEntityId, f32)>,
    nearest_portal_pos: Option<[f32; 2]>,
    portal_eligible: bool,
    activate_portal: Option<purgatory_protocol::WireEntityId>,
}

fn replica_interact_routing(replica: &ReplicatedWorld) -> ReplicaInteractRouting {
    let nearest_portal = advisory_nearest_portal(replica);
    let activate_portal = advisory_centered_portal(replica);
    ReplicaInteractRouting {
        nearest_generic: advisory_nearest_generic(replica),
        nearest_portal: nearest_portal.map(|(id, dist, _)| (id, dist)),
        nearest_portal_pos: nearest_portal.map(|(_, _, pos)| pos),
        portal_eligible: activate_portal.is_some(),
        activate_portal,
    }
}

fn log_interact_routing(prefix: &str, routing: &ReplicaInteractRouting) {
    match routing.nearest_generic {
        Some((id, distance)) => {
            println!("{prefix} nearest generic interactable={id} distance={distance:.3}");
        }
        None => println!("{prefix} nearest generic interactable=none distance=n/a"),
    }
    match routing.nearest_portal {
        Some((id, distance)) => {
            println!(
                "{prefix} nearest portal={id} distance={distance:.3} eligible={}",
                routing.portal_eligible
            );
        }
        None => println!(
            "{prefix} nearest portal=none distance=n/a eligible={}",
            routing.portal_eligible
        ),
    }
}

/// E send path: nearest generic interactable inside [`INTERACT_RANGE`].
/// Portals are never candidates.
fn interact_open_send_target(
    routing: &ReplicaInteractRouting,
) -> Option<purgatory_protocol::WireEntityId> {
    routing
        .nearest_generic
        .and_then(|(id, dist)| (dist <= INTERACT_RANGE).then_some(id))
}

/// Up Arrow send path: replica player center inside the portal activation zone.
fn portal_activate_send_target(
    routing: &ReplicaInteractRouting,
) -> Option<purgatory_protocol::WireEntityId> {
    routing.activate_portal
}

fn nearest_of_kind(
    replica: &ReplicatedWorld,
    kind: ReplicatedKind,
) -> Option<(purgatory_protocol::WireEntityId, f32, [f32; 2])> {
    let local = replica.local_entity()?;
    replica
        .iter()
        .filter(|entity| entity.kind == kind && entity.entity_id != local.entity_id)
        .min_by(|a, b| {
            let da = dist2(local.position, a.position);
            let db = dist2(local.position, b.position);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|entity| {
            (
                entity.entity_id,
                dist2(local.position, entity.position).sqrt(),
                entity.position,
            )
        })
}

/// Advisory nearest replica generic interactable. Portals are excluded.
fn advisory_nearest_generic(
    replica: &ReplicatedWorld,
) -> Option<(purgatory_protocol::WireEntityId, f32)> {
    nearest_of_kind(replica, ReplicatedKind::Interactable).map(|(id, dist, _)| (id, dist))
}

fn advisory_nearest_portal(
    replica: &ReplicatedWorld,
) -> Option<(purgatory_protocol::WireEntityId, f32, [f32; 2])> {
    nearest_of_kind(replica, ReplicatedKind::Portal)
}

/// Advisory only: server still validates the activation zone.
fn advisory_centered_portal(replica: &ReplicatedWorld) -> Option<purgatory_protocol::WireEntityId> {
    let local = replica.local_entity()?;
    replica
        .iter()
        .filter(|entity| {
            entity.kind == ReplicatedKind::Portal
                && entity.entity_id != local.entity_id
                && in_portal_activation_zone(local.position, entity.position)
        })
        .min_by(|a, b| {
            let da = dist2(local.position, a.position);
            let db = dist2(local.position, b.position);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|entity| entity.entity_id)
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}

fn interact_result_label(event: ServerInteract) -> String {
    match event {
        ServerInteract::Opened { session_id, .. } => format!("Opened #{session_id}"),
        ServerInteract::Updated { session_id, .. } => format!("Updated #{session_id}"),
        ServerInteract::Rejected { reason, .. } => format!("Rejected / {reason}"),
        ServerInteract::Closed { session_id, reason } => {
            format!("Closed #{session_id} / {reason}")
        }
    }
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
    DrawQuad::rect(aabb.center, aabb.size(), color)
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.fatal.is_some() {
            event_loop.exit();
            return;
        }
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
    fn interactable_quads_mark_replica_interactables() {
        use crate::replica::ReplicatedWorld;
        use purgatory_protocol::{ReplicatedKind, SnapshotEntity, WireEntityId, WorldSnapshot};

        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let target = WireEntityId {
            index: 7,
            generation: 1,
        };
        let mut replica = ReplicatedWorld::new();
        replica.apply(WorldSnapshot::from_poses(
            1,
            1,
            local,
            vec![
                SnapshotEntity {
                    entity_id: local,
                    kind: ReplicatedKind::Player,
                    position: [-19.4, -3.0],
                    velocity: [0.0, 0.0],
                },
                SnapshotEntity {
                    entity_id: target,
                    kind: ReplicatedKind::Interactable,
                    position: [-18.2, -3.05],
                    velocity: [0.0, 0.0],
                },
            ],
        ));
        let quads = super::interactable_quads(&replica);
        assert_eq!(quads.len(), 2, "body + cap");
        assert!(
            quads
                .iter()
                .any(|q| q.center == [-18.2, -3.05] && q.color == super::INTERACTABLE_BODY_COLOR)
        );
        let labels = super::replica_kind_labels(&replica, ReplicatedKind::Interactable);
        assert_eq!(labels.len(), 1);
        assert!(labels[0].starts_with("7:1"));
        assert_eq!(
            super::advisory_nearest_generic(&replica).map(|(id, _)| id),
            Some(target)
        );
        let dist = super::advisory_nearest_generic(&replica)
            .map(|(_, d)| d)
            .unwrap();
        assert!((dist - 1.201).abs() < 0.02);
        let routing = super::replica_interact_routing(&replica);
        assert_eq!(super::interact_open_send_target(&routing), Some(target));
        assert!(super::advisory_centered_portal(&replica).is_none());
    }

    #[test]
    fn portal_quads_are_triangles_and_e_ignores_them() {
        use crate::replica::ReplicatedWorld;
        use purgatory_protocol::{ReplicatedKind, SnapshotEntity, WireEntityId, WorldSnapshot};

        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let portal = WireEntityId {
            index: 9,
            generation: 1,
        };
        let switch = WireEntityId {
            index: 7,
            generation: 1,
        };
        let mut replica = ReplicatedWorld::new();
        replica.apply(WorldSnapshot::from_poses(
            1,
            1,
            local,
            vec![
                SnapshotEntity {
                    entity_id: local,
                    kind: ReplicatedKind::Player,
                    position: [6.0, -3.0],
                    velocity: [0.0, 0.0],
                },
                SnapshotEntity {
                    entity_id: portal,
                    kind: ReplicatedKind::Portal,
                    position: [6.0, -2.9],
                    velocity: [0.0, 0.0],
                },
                SnapshotEntity {
                    entity_id: switch,
                    kind: ReplicatedKind::Interactable,
                    position: [8.5, -2.9],
                    velocity: [0.0, 0.0],
                },
            ],
        ));
        let triangles = super::portal_quads(&replica);
        assert_eq!(triangles.len(), 1);
        assert!(triangles[0].triangle);
        assert_eq!(triangles[0].color, super::PORTAL_COLOR);
        assert_eq!(super::interactable_quads(&replica).len(), 2);
        let routing = super::replica_interact_routing(&replica);
        assert_eq!(
            routing.nearest_generic.map(|(id, _)| id),
            Some(switch),
            "E must not target a nearer portal"
        );
        assert_ne!(
            super::interact_open_send_target(&routing),
            Some(portal),
            "E must never send InteractOpen for a portal"
        );
        assert_eq!(super::advisory_centered_portal(&replica), Some(portal));
        assert!(routing.portal_eligible);
        assert_eq!(super::portal_activate_send_target(&routing), Some(portal));
        replica.apply(WorldSnapshot::from_poses(
            2,
            2,
            local,
            vec![
                SnapshotEntity {
                    entity_id: local,
                    kind: ReplicatedKind::Player,
                    position: [4.0, -3.0],
                    velocity: [0.0, 0.0],
                },
                SnapshotEntity {
                    entity_id: portal,
                    kind: ReplicatedKind::Portal,
                    position: [6.0, -2.9],
                    velocity: [0.0, 0.0],
                },
            ],
        ));
        assert!(
            super::advisory_centered_portal(&replica).is_none(),
            "nearest portal is not enough"
        );
        let routing = super::replica_interact_routing(&replica);
        assert!(!routing.portal_eligible);
        assert!(super::portal_activate_send_target(&routing).is_none());
    }

    fn replica_from_poses(
        local: purgatory_protocol::WireEntityId,
        entities: Vec<purgatory_protocol::SnapshotEntity>,
    ) -> crate::replica::ReplicatedWorld {
        let mut replica = crate::replica::ReplicatedWorld::new();
        replica.apply(purgatory_protocol::WorldSnapshot::from_poses(
            1, 1, local, entities,
        ));
        replica
    }

    fn pose(
        id: purgatory_protocol::WireEntityId,
        kind: purgatory_protocol::ReplicatedKind,
        position: [f32; 2],
    ) -> purgatory_protocol::SnapshotEntity {
        purgatory_protocol::SnapshotEntity {
            entity_id: id,
            kind,
            position,
            velocity: [0.0, 0.0],
        }
    }

    #[test]
    fn e_near_portal_does_not_send_interact_open_for_portal() {
        use purgatory_protocol::{ReplicatedKind, WireEntityId};
        use purgatory_simulation::in_portal_activation_zone;

        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let portal = WireEntityId {
            index: 9,
            generation: 1,
        };
        let chest = WireEntityId {
            index: 8,
            generation: 1,
        };
        let player = [6.0, -3.0];
        let portal_pos = [6.0, -2.9];
        let chest_pos = [-7.4, -2.9];
        let replica = replica_from_poses(
            local,
            vec![
                pose(local, ReplicatedKind::Player, player),
                pose(portal, ReplicatedKind::Portal, portal_pos),
                pose(chest, ReplicatedKind::Interactable, chest_pos),
            ],
        );
        let routing = super::replica_interact_routing(&replica);
        assert_eq!(routing.nearest_generic.map(|(id, _)| id), Some(chest));
        let generic_dist = routing.nearest_generic.unwrap().1;
        assert!(
            (generic_dist - 13.4).abs() < 0.2,
            "standing on the triangle, nearest generic is the chest (~13.4), not the portal; got {generic_dist}"
        );
        assert!(super::interact_open_send_target(&routing).is_none());
        assert_ne!(super::interact_open_send_target(&routing), Some(portal));
        assert_eq!(routing.nearest_portal.map(|(id, _)| id), Some(portal));
        assert!(routing.portal_eligible);
        assert_eq!(super::portal_activate_send_target(&routing), Some(portal));
        assert!(in_portal_activation_zone(player, portal_pos));
        assert_eq!(routing.nearest_portal_pos, Some(portal_pos));
    }

    #[test]
    fn e_still_sends_interact_open_for_switch_and_chest() {
        use purgatory_protocol::{ReplicatedKind, WireEntityId};

        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let switch = WireEntityId {
            index: 7,
            generation: 1,
        };
        let chest = WireEntityId {
            index: 8,
            generation: 1,
        };
        let portal = WireEntityId {
            index: 9,
            generation: 1,
        };
        let replica = replica_from_poses(
            local,
            vec![
                pose(local, ReplicatedKind::Player, [-19.4, -3.0]),
                pose(switch, ReplicatedKind::Interactable, [-17.8, -2.9]),
                pose(chest, ReplicatedKind::Interactable, [-7.4, -2.9]),
                pose(portal, ReplicatedKind::Portal, [6.0, -2.9]),
            ],
        );
        let routing = super::replica_interact_routing(&replica);
        assert_eq!(super::interact_open_send_target(&routing), Some(switch));
        assert!(!routing.portal_eligible);
        assert!(super::portal_activate_send_target(&routing).is_none());

        let replica = replica_from_poses(
            local,
            vec![
                pose(local, ReplicatedKind::Player, [-7.4, -3.0]),
                pose(switch, ReplicatedKind::Interactable, [-17.8, -2.9]),
                pose(chest, ReplicatedKind::Interactable, [-7.4, -2.9]),
                pose(portal, ReplicatedKind::Portal, [6.0, -2.9]),
            ],
        );
        let routing = super::replica_interact_routing(&replica);
        assert_eq!(super::interact_open_send_target(&routing), Some(chest));
        assert!(!routing.portal_eligible);
    }

    #[test]
    fn up_outside_portal_zone_sends_nothing_up_inside_sends_traversal() {
        use purgatory_protocol::{ReplicatedKind, WireEntityId};

        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let portal = WireEntityId {
            index: 9,
            generation: 1,
        };
        let outside = replica_from_poses(
            local,
            vec![
                pose(local, ReplicatedKind::Player, [4.0, -3.0]),
                pose(portal, ReplicatedKind::Portal, [6.0, -2.9]),
            ],
        );
        let routing = super::replica_interact_routing(&outside);
        assert_eq!(routing.nearest_portal.map(|(id, _)| id), Some(portal));
        assert!(!routing.portal_eligible);
        assert!(super::portal_activate_send_target(&routing).is_none());

        let inside = replica_from_poses(
            local,
            vec![
                pose(local, ReplicatedKind::Player, [6.0, -3.0]),
                pose(portal, ReplicatedKind::Portal, [6.0, -2.9]),
            ],
        );
        let routing = super::replica_interact_routing(&inside);
        assert!(routing.portal_eligible);
        assert_eq!(super::portal_activate_send_target(&routing), Some(portal));
        assert!(super::interact_open_send_target(&routing).is_none());
    }

    #[test]
    fn client_portal_zone_agrees_with_shared_activation_math() {
        use purgatory_protocol::{ReplicatedKind, WireEntityId};
        use purgatory_simulation::in_portal_activation_zone;

        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let portal = WireEntityId {
            index: 9,
            generation: 1,
        };
        let player = [6.0, -3.0];
        let portal_pos = [6.0, -2.9];
        let replica = replica_from_poses(
            local,
            vec![
                pose(local, ReplicatedKind::Player, player),
                pose(portal, ReplicatedKind::Portal, portal_pos),
            ],
        );
        let replica_player = replica.local_entity().unwrap().position;
        let replica_portal = replica
            .iter()
            .find(|e| e.kind == ReplicatedKind::Portal)
            .unwrap()
            .position;
        assert_eq!(replica_player, player);
        assert_eq!(replica_portal, portal_pos);
        assert_eq!(
            in_portal_activation_zone(replica_player, replica_portal),
            super::replica_interact_routing(&replica).portal_eligible
        );
    }

    #[test]
    fn empty_replica_has_no_advisory_interact_target() {
        use crate::replica::ReplicatedWorld;
        let replica = ReplicatedWorld::new();
        assert!(super::advisory_nearest_generic(&replica).is_none());
    }

    #[test]
    fn advisory_nearest_ignores_players() {
        use crate::replica::ReplicatedWorld;
        use purgatory_protocol::{ReplicatedKind, SnapshotEntity, WireEntityId, WorldSnapshot};

        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let remote = WireEntityId {
            index: 2,
            generation: 1,
        };
        let mut replica = ReplicatedWorld::new();
        replica.apply(WorldSnapshot::from_poses(
            1,
            1,
            local,
            vec![
                SnapshotEntity {
                    entity_id: local,
                    kind: ReplicatedKind::Player,
                    position: [0.0, 0.0],
                    velocity: [0.0, 0.0],
                },
                SnapshotEntity {
                    entity_id: remote,
                    kind: ReplicatedKind::Player,
                    position: [1.0, 0.0],
                    velocity: [0.0, 0.0],
                },
            ],
        ));
        assert!(super::advisory_nearest_generic(&replica).is_none());
    }

    #[test]
    fn interact_result_label_keeps_reject_visible() {
        use purgatory_protocol::{InteractRejectReason, ServerInteract, WireEntityId};
        let label = super::interact_result_label(ServerInteract::Rejected {
            target: WireEntityId {
                index: 42,
                generation: 0,
            },
            reason: InteractRejectReason::OutOfRange,
        });
        assert_eq!(label, "Rejected / OutOfRange");
        let opened = super::interact_result_label(ServerInteract::Opened {
            session_id: 7,
            target: WireEntityId {
                index: 42,
                generation: 0,
            },
        });
        assert_eq!(opened, "Opened #7");
    }

    #[test]
    fn empty_replica_does_not_draw_local_interactable_substitutes() {
        use crate::replica::ReplicatedWorld;

        let world = World::footnote_test_stage();
        let replica = ReplicatedWorld::new();
        assert_eq!(super::interactable_quads(&replica).len(), 0);
        let local_dev = world
            .iter()
            .filter(|&id| {
                world.interactable_of(id).is_some()
                    && world.address_of(id) == Some(purgatory_simulation::WorldAddress::DEV)
            })
            .count();
        assert_eq!(
            local_dev, 2,
            "stage still contains DEV fixtures; they must not be drawn without replica"
        );
    }

    #[test]
    fn nearby_dev_interactable_projects_on_screen_at_spawn() {
        let world = World::footnote_test_stage();
        let player = world.player_body().expect("spawned player").position;
        let near = world
            .iter()
            .filter(|&id| {
                world.interactable_of(id).is_some()
                    && world.address_of(id) == Some(purgatory_simulation::WorldAddress::DEV)
            })
            .filter_map(|id| world.transform_of(id).map(|t| t.position))
            .min_by(|a, b| a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal))
            .expect("nearby fixture");
        assert!((near[0] - (purgatory_simulation::FOOTNOTE_SPAWN_X + 1.6)).abs() < 0.05);
        let mut camera = Camera::footnote_test_dev();
        camera.follow_clamped(player, world.bounds());
        let ndc = camera.world_to_ndc(near);
        assert!(
            ndc[0].abs() <= 1.0 && ndc[1].abs() <= 1.0,
            "nearby fixture must be on-camera at spawn ndc=({:.3},{:.3}) pos={near:?} camera={:?} player={player:?}",
            ndc[0],
            ndc[1],
            camera.position
        );
    }

    #[test]
    fn empty_replica_does_not_request_map_id_zero_baseline() {
        assert_eq!(
            super::map_transition_step(false, None, (0, 0, 0), true, true, true),
            super::MapTransitionStep::None
        );
        assert_eq!(
            super::map_transition_step(true, None, (1, 0, 0), true, true, true),
            super::MapTransitionStep::FirstBaseline,
            "first accepted snapshot must rebuild immediately"
        );
        assert_eq!(
            super::map_transition_step(true, None, (1, 0, 0), true, true, false),
            super::MapTransitionStep::None,
            "first baseline waits for self Enter"
        );
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (2, 0, 0), true, true, true),
            super::MapTransitionStep::StartFadeOut,
            "accepted address change starts FadeOut before swapping"
        );
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (2, 0, 0), false, false, true),
            super::MapTransitionStep::WaitFade,
            "defer swap until fade is fully black"
        );
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (2, 0, 0), false, true, false),
            super::MapTransitionStep::WaitFade,
            "defer swap until new-epoch self Enter exists"
        );
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (2, 0, 0), false, true, true),
            super::MapTransitionStep::SwapBaseline
        );
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (1, 0, 0), true, true, true),
            super::MapTransitionStep::None
        );
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (1, 1, 0), true, true, true),
            super::MapTransitionStep::StartMembershipFadeOut,
            "authoritative channel change starts membership fade; button click does not"
        );
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (1, 1, 0), false, false, true),
            super::MapTransitionStep::WaitFade,
            "membership FadeOut stays obscured until black hold"
        );
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (1, 1, 0), false, true, false),
            super::MapTransitionStep::WaitFade,
            "MembershipReady path waits for self Enter before retarget"
        );
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (1, 1, 0), false, true, true),
            super::MapTransitionStep::RetargetAddress,
            "black hold + self Enter retargets; does not SwapBaseline/rebuild"
        );
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (1, 0, 2), true, true, true),
            super::MapTransitionStep::StartMembershipFadeOut,
            "same MapId instance change uses membership fade, not portal map fade"
        );
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (1, 0, 2), false, true, true),
            super::MapTransitionStep::RetargetAddress
        );
    }

    #[test]
    fn channel_request_without_observer_change_does_not_start_fade() {
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (1, 0, 0), true, true, true),
            super::MapTransitionStep::None,
            "DEV button/request is not a readiness or fade trigger"
        );
    }

    #[test]
    fn locked_or_ineligible_up_does_not_start_fade() {
        let fade = crate::map_fade::MapFade::default();
        assert!(fade.is_idle());
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (1, 0, 0), true, true, true),
            super::MapTransitionStep::None
        );
    }

    #[test]
    fn accepted_dest_locks_gameplay_input_before_fade_starts() {
        assert!(super::transition_gameplay_input_locked(
            false,
            Some((1, 0, 0)),
            (2, 0, 0),
            true
        ));
        assert!(super::transition_gameplay_input_locked(
            false,
            Some((1, 0, 0)),
            (1, 1, 0),
            true
        ));
        assert!(
            !super::transition_gameplay_input_locked(false, Some((1, 0, 0)), (1, 0, 0), true),
            "aligned idle presentation is not locked"
        );
        assert!(
            !super::transition_gameplay_input_locked(false, None, (1, 0, 0), true),
            "first baseline is not a transition barrier"
        );
    }

    #[test]
    fn fade_in_unlocks_gameplay_input() {
        assert!(super::transition_gameplay_input_locked(
            true,
            Some((2, 0, 0)),
            (2, 0, 0),
            true
        ));
        assert!(
            !super::transition_gameplay_input_locked(false, Some((2, 0, 0)), (2, 0, 0), true),
            "FadeIn / idle aligned dest is unlocked"
        );
    }

    #[test]
    fn input_gate_debug_label_matches_kind() {
        assert_eq!(
            super::input_gate_debug_label(false, true, true, Some((1, 0, 0)), (1, 0, 0)),
            "INPUT: ACTIVE"
        );
        assert_eq!(
            super::input_gate_debug_label(true, false, true, Some((1, 0, 0)), (2, 0, 0)),
            "INPUT: LOCKED · MAP TRANSITION"
        );
        assert_eq!(
            super::input_gate_debug_label(true, false, false, Some((1, 0, 0)), (1, 1, 0)),
            "INPUT: LOCKED · CHANNEL TRANSITION"
        );
    }

    #[test]
    fn spawn_from_replica_uses_self_enter_not_first_platform_center() {
        use crate::replica::ReplicatedWorld;
        use purgatory_content::{LoadMode, default_content_root, geometry_plan, load_registry};
        use purgatory_protocol::{ReplicatedKind, SnapshotEntity, WireEntityId, WorldSnapshot};
        use purgatory_simulation::{ChannelId, InstanceId, World, WorldAddress};

        let registry = load_registry(&default_content_root(), LoadMode::Shared).expect("registry");
        let content = purgatory_common::ContentId::from_authored("map.dev.second").expect("id");
        let map_id = registry.map_id(content).expect("Map B");
        let address = WorldAddress::new(map_id, ChannelId::DEFAULT, InstanceId::DEFAULT);
        let plan = geometry_plan(&registry, map_id, address).expect("plan");
        let mut world = World::new();
        world.instantiate_map(&plan).expect("instantiate");
        let floor_center = world
            .iter_platforms()
            .next()
            .expect("floor")
            .transform
            .position[0];
        let dest = [10.0, -2.3];
        let local = WireEntityId {
            index: 7,
            generation: 1,
        };
        let mut replica = ReplicatedWorld::new();
        let _ = replica.apply(WorldSnapshot::from_poses(
            1,
            1,
            local,
            vec![SnapshotEntity {
                entity_id: local,
                kind: ReplicatedKind::Player,
                position: dest,
                velocity: [0.0, 0.0],
            }],
        ));
        let seeded =
            super::spawn_local_player_from_replica(&mut world, address, &replica).expect("seeded");
        assert!(
            (seeded[0] - dest[0]).abs() < 0.05,
            "must seed dest x={}, got {} (floor center {})",
            dest[0],
            seeded[0],
            floor_center
        );
        assert!((seeded[0] - floor_center).abs() > 0.5);
    }

    #[test]
    fn destination_ready_pose_matches_seeded_self_enter() {
        let mut fade = crate::map_fade::MapFade::default();
        fade.begin_out();
        fade.tick(0.300 + 0.01);
        fade.tick(0.075 + 0.01);
        assert!(fade.debug_phase() == "Hold");
        let ready = crate::map_fade::DestinationReady {
            epoch: 4,
            local_pose: [10.0, -2.3],
        };
        fade.notify_destination_ready(ready);
        fade.tick(0.0);
        assert_eq!(fade.debug_phase(), "FadeIn");
        assert_eq!(fade.destination_ready(), Some(ready));
    }

    #[test]
    fn membership_ready_false_stays_obscured() {
        let mut fade = crate::map_fade::MapFade::default();
        fade.begin_transition(
            crate::map_fade::TransitionKind::Membership,
            (1, 0, 0),
            (1, 1, 0),
            Some([-8.0, -3.0]),
        );
        fade.tick(0.200 + 0.01);
        fade.tick(0.050 + 1.0);
        assert_eq!(fade.debug_phase(), "Hold");
        assert!((fade.alpha() - 1.0).abs() < 1e-4);
        assert!(fade.membership_ready().is_none());
    }

    #[test]
    fn scene_quads_omit_remotes_when_caller_passes_empty() {
        use crate::interp::PresentationPose;
        use purgatory_protocol::WireEntityId;

        let world = World::dev_stage();
        let remotes = [PresentationPose {
            entity_id: WireEntityId {
                index: 2,
                generation: 1,
            },
            position: [4.0, -2.9],
        }];
        let with_remotes = super::scene_quads(&world, Some([-8.0, -3.0]), &remotes);
        let without = super::scene_quads(&world, Some([-8.0, -3.0]), &[]);
        assert!(with_remotes.len() > without.len());
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

    #[test]
    fn frozen_source_remotes_draw_during_visible_fade_out() {
        use crate::interp::{InterpolationBuffer, PresentationPose};
        use purgatory_protocol::WireEntityId;

        let frozen = super::FrozenPresentation {
            remotes: vec![PresentationPose {
                entity_id: WireEntityId {
                    index: 4,
                    generation: 1,
                },
                position: [2.0, -3.0],
            }],
            interactable_quads: vec![crate::renderer::DrawQuad::rect(
                [-18.0, -3.0],
                [0.8, 1.4],
                super::INTERACTABLE_BODY_COLOR,
            )],
            portal_quads: vec![crate::renderer::DrawQuad::triangle(
                [6.0, -2.9],
                [0.9, 1.4],
                super::PORTAL_COLOR,
            )],
        };
        let interp = InterpolationBuffer::new();
        let remotes = super::visible_remote_poses(true, Some(&frozen), false, &interp);
        assert_eq!(remotes.len(), 1, "source remote must remain during FadeOut");
        assert_eq!(remotes[0].position, [2.0, -3.0]);
        let interactables = super::visible_interactable_quads(
            true,
            Some(&frozen),
            false,
            &crate::replica::ReplicatedWorld::new(),
        );
        assert_eq!(interactables.len(), 1);
        let portals = super::visible_portal_quads(
            true,
            Some(&frozen),
            false,
            &crate::replica::ReplicatedWorld::new(),
        );
        assert_eq!(portals.len(), 1);
    }

    #[test]
    fn dest_replicas_are_not_drawn_until_blackout_commit() {
        use crate::interp::InterpolationBuffer;

        let frozen = super::FrozenPresentation::default();
        let interp = InterpolationBuffer::new();
        let remotes = super::visible_remote_poses(false, Some(&frozen), false, &interp);
        assert!(
            remotes.is_empty(),
            "unaligned dest interp must not draw before commit"
        );
        let interactables = super::visible_interactable_quads(
            false,
            Some(&frozen),
            false,
            &crate::replica::ReplicatedWorld::new(),
        );
        assert!(interactables.is_empty());
    }

    #[test]
    fn membership_fade_keeps_source_until_fully_black() {
        let mut fade = crate::map_fade::MapFade::default();
        fade.begin_transition(
            crate::map_fade::TransitionKind::Membership,
            (1, 0, 0),
            (1, 1, 0),
            Some([-8.0, -3.0]),
        );
        fade.tick(crate::map_fade::MEMBERSHIP_FADE_OUT_SEC * 0.5);
        assert!(fade.holds_source_presentation());
        assert!(!fade.is_fully_black());
        fade.tick(crate::map_fade::MEMBERSHIP_FADE_OUT_SEC);
        assert!(fade.is_fully_black());
        assert!(!fade.holds_source_presentation());
        assert!(fade.allows_baseline_swap());
    }

    #[test]
    fn map_swap_is_blocked_while_fade_out_is_visible() {
        assert_eq!(
            super::map_transition_step(true, Some((1, 0, 0)), (2, 0, 0), false, false, true),
            super::MapTransitionStep::WaitFade
        );
        let mut fade = crate::map_fade::MapFade::default();
        fade.begin_transition(
            crate::map_fade::TransitionKind::Map,
            (1, 0, 0),
            (2, 0, 0),
            Some([-8.0, -3.0]),
        );
        fade.notify_destination_ready(crate::map_fade::DestinationReady {
            epoch: 9,
            local_pose: [10.0, -2.3],
        });
        fade.tick(crate::map_fade::MAP_FADE_OUT_SEC * 0.3);
        assert!(
            !fade.allows_baseline_swap(),
            "early DestinationReady must not swap geometry during visible FadeOut"
        );
        fade.tick(crate::map_fade::MAP_FADE_OUT_SEC);
        assert!(fade.is_fully_black());
        assert!(fade.allows_baseline_swap());
    }
}
