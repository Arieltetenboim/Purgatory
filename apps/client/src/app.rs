//! Client application loop: platform events, simulation clock, renderer.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use purgatory_content::{
    ContentRegistry, LoadMode, default_content_root, geometry_plan, load_registry,
};
use purgatory_protocol::{ReplicatedKind, ReplicationFrame, ServerInteract, move_axis_from_i8};
use purgatory_simulation::{
    Aabb, ChannelId, INTERACT_RANGE, InstanceId, MapId, PLAYER_HALF_EXTENTS, PlatformKind,
    PlayerInput, PlayerState, SimulationClock, World, WorldAddress, in_portal_activation_zone,
};
#[cfg(feature = "dev-diagnostics")]
use purgatory_simulation::{TICK_DURATION, aoi_policy_rects};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::camera_follow::{CameraCommit, CameraFollow};
use crate::character_presentation::{
    CharacterPresentationSet, LocalMotion, PresentationActivity, PresentationEntityKey,
    PresentationOneShotTable, PresentationView, RemoteMotion, SocialNpcMotion,
    apply_climb_back_overlay, equipment_view_from_replica, from_local_with_presentation,
    from_remote_with_presentation, from_social_npc, immunity_flash_visible,
    presentation_debug_quads_with_assets,
};
use crate::choice_bubble::layout_choice_bubble;
#[cfg(feature = "dev-diagnostics")]
use crate::debug::aoi_view::{
    band_label, bind_local_player_label_pose, classify_band, compact_world_space_label,
    replica_entity_debug_rows, semantic_label, world_space_label_entries,
    world_space_labels_eligible,
};
#[cfg(feature = "dev-diagnostics")]
use crate::debug::entity_inspector::{self, WorldEntityInput};
#[cfg(feature = "dev-diagnostics")]
use crate::debug::interact_status::{InteractStatusView, note_kind_change, trail_display};
#[cfg(feature = "dev-diagnostics")]
use crate::debug::{
    CameraDiagnostics, CameraMotionDebug, CollisionHistoryEvent, ConnectionPaint, DebugCommand,
    DebugOverlay, DiagnosticsDemand, DiagnosticsFrame, DiscSubject, NetworkDiagnostics,
    OverlayInit, PresentationDiagnostics, RESET_TO_SPAWN_FLASH, RemoteMotionProbe,
    RuntimeDiagnostics, WorldDiagnostics, WorldRosterDiagnostics, aoi_entity_debug_quads,
    append_debug_gizmos, camera_deadzone_quads, footnote_debug_quads, gameplay_receives_keyboard,
    gameplay_receives_pointer, has_persistent_dev_warnings, is_debug_toggle, reset_action_flash,
    reset_player_uses_replica,
};
use crate::dialogue_runtime::DialogueRuntime;
#[cfg(feature = "dev-diagnostics")]
use crate::display::collect_display_debug;
use crate::display::{DisplayController, SurfaceResizeAction, WindowFlush};
#[cfg(feature = "dev-diagnostics")]
use crate::frontend::ConnectionFrontend;
use crate::input::{ActionState, IntentNet};
use crate::interp::{InterpolationBuffer, PresentationPose, interpolated_or_replica_pose};
#[cfg(feature = "dev-diagnostics")]
use crate::jitter_forensics::{CameraJitterMode, ForensicPush, ForensicTrace};
use crate::lifecycle::{ClientLifecycle, ClientScreen};
use crate::local_presentation::{
    FrameLocalPose, LocalPresentation, PRESENTATION_SNAP_DISTANCE, compose_local_render_pose,
    offset_length, remainder_alpha,
};
use crate::map_fade::{
    DestinationReady, MapFade, MembershipReady, ReadinessFlags, TransitionKind, pose_stable,
    world_interaction_cleared_for_membership,
};
#[cfg(feature = "dev-diagnostics")]
use crate::network::NetworkImpairmentConfig;
use crate::network::{ClientEndpointConfig, NetworkCommand, NetworkHandle};
use crate::npc_presentation::{SpriteAnimationPlayer, SpriteSheet, base_activity, clip_name};
use crate::platform::{diagnostic_title, window_attributes};
use crate::prediction::{LocalPrediction, local_presentation_pose};
#[cfg(feature = "dev-diagnostics")]
use crate::renderer::rf_diag::{RfVertexProof, rf_probe_proof, rf_scene_quads};
use crate::renderer::{
    Camera, DrawQuad, FOOTNOTE_LOGICAL_HEIGHT, FrameStatus, MAX_QUADS, Renderer, TextBlock, UiRect,
    constrained_pixel_viewport, parallax_quads,
};
#[cfg(feature = "dev-diagnostics")]
use crate::renderer::{
    PARALLAX_FAR, PARALLAX_MID, PARALLAX_NEAR, PixelViewport, parallax_debug_quads,
};
#[cfg(feature = "dev-diagnostics")]
use crate::replica::ReplicaLifecycleEvent;
use crate::replica::{FrameDecision, ReplicatedEntity, ReplicatedWorld};
use crate::speech_bubble::{layout_speech_bubble, layout_speech_bubble_with_offset};
use crate::ui_runtime::UIRuntimeState;

const PLAYER_COLOR: [f32; 4] = [0.19, 0.55, 0.66, 1.0];
const REMOTE_PLAYER_COLOR: [f32; 4] = [0.72, 0.32, 0.38, 1.0];
/// Magenta body: distinct from cyan players, brown/green platforms, and the floor.
const INTERACTABLE_BODY_COLOR: [f32; 4] = [0.92, 0.18, 0.72, 1.0];
const INTERACTABLE_CAP_COLOR: [f32; 4] = [1.0, 0.86, 0.12, 1.0];
const PORTAL_COLOR: [f32; 4] = [0.32, 0.92, 0.78, 1.0];
const NPC_DEAD_COLOR: [f32; 4] = [0.38, 0.16, 0.18, 1.0];
const NPC_RESPAWN_COLOR: [f32; 4] = [0.25, 0.95, 0.62, 1.0];
const NPC_STATE_INDICATOR_COLOR: [f32; 4] = [1.0, 0.93, 0.35, 1.0];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NpcVisualCue {
    Normal,
    Hurt,
    Dead,
    Respawn,
}

#[must_use]
fn npc_visual_cue(dead: bool, hurt: bool, respawning: bool) -> NpcVisualCue {
    if dead {
        NpcVisualCue::Dead
    } else if hurt {
        NpcVisualCue::Hurt
    } else if respawning {
        NpcVisualCue::Respawn
    } else {
        NpcVisualCue::Normal
    }
}
/// D5 may pass a live IPC subscriber here. D1 has no IPC consumer.
#[cfg(feature = "dev-diagnostics")]
const DIAGNOSTICS_IPC_SUBSCRIBED: bool = false;
const INTERACTABLE_HALF: [f32; 2] = [0.4, 0.7];
#[cfg(feature = "dev-diagnostics")]
const INTERP_AUTH_GIZMO_COLOR: [f32; 4] = [1.0, 0.85, 0.2, 0.55];
#[cfg(feature = "dev-diagnostics")]
const PREDICT_AUTH_GIZMO_COLOR: [f32; 4] = [0.95, 0.45, 0.15, 0.55];
#[cfg(feature = "dev-diagnostics")]
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
    let mut app = ClientApp::new(registry)?;
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
    asset_runtime: crate::asset_runtime::AssetRuntime,
    character_visual_pack: crate::character_assets::CharacterVisualPack,
    #[cfg(feature = "dev-diagnostics")]
    debug: Option<DebugOverlay>,
    #[cfg(feature = "dev-diagnostics")]
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
    #[cfg(feature = "dev-diagnostics")]
    last_camera_motion: CameraMotionDebug,
    network: Option<NetworkHandle>,
    replica: ReplicatedWorld,
    interp: InterpolationBuffer,
    prediction: LocalPrediction,
    snapshot_malformed: u64,
    #[cfg(feature = "dev-diagnostics")]
    impairment_seed: u64,
    ui_runtime: UIRuntimeState,
    dialogue_runtime: DialogueRuntime,
    cursor_position: Option<[f32; 2]>,
    speech_bubble_hit: Option<crate::renderer::UiRect>,
    choice_bubble_hits: Vec<crate::renderer::UiRect>,
    choice_click_edge: Option<usize>,
    bubble_click_edge: bool,
    dialogue_advance_sent_this_frame: bool,
    trace_replica_apply: bool,
    trace_scene: bool,
    trace_malformed: bool,
    last_replica_interactables: usize,
    last_interact_request: String,
    last_interact_result: String,
    #[cfg(feature = "dev-diagnostics")]
    last_interact_kind: Option<crate::ui_runtime::InteractKind>,
    #[cfg(feature = "dev-diagnostics")]
    interact_last_transition: Option<String>,
    #[cfg(feature = "dev-diagnostics")]
    interact_trail: Vec<&'static str>,
    registry: ContentRegistry,
    last_observer: Option<(u32, u32, u32)>,
    map_fade: MapFade,
    frozen_presentation: Option<FrozenPresentation>,
    camera_follow: CameraFollow,
    pending_camera_commit: Option<CameraCommit>,
    pending_center_on_player: bool,
    local_presentation: LocalPresentation,
    frame_local: FrameLocalPose,
    #[cfg(feature = "dev-diagnostics")]
    jitter_trace: ForensicTrace,
    reconciled_this_frame: bool,
    last_ticks_executed: u32,
    network_frames_this_frame: u32,
    #[cfg(feature = "dev-diagnostics")]
    last_jitter_mode: CameraJitterMode,
    skeleton: crate::skeleton_debug::HumanoidDebug,
    characters: CharacterPresentationSet,
    presentation_oneshots: PresentationOneShotTable,
    npc_sheet: SpriteSheet,
    npc_players:
        HashMap<PresentationEntityKey, (SpriteAnimationPlayer, PresentationActivity, bool)>,
    /// A2 proof playback clock (not Clone; lives on App, not DebugUiState).
    #[cfg(feature = "dev-diagnostics")]
    animation_player: purgatory_animation::AnimationPlayer,
    /// One resolved animation sample `t` for the current frame.
    #[cfg(feature = "dev-diagnostics")]
    selected_animation_sample_t: f32,
    #[cfg(feature = "dev-diagnostics")]
    equipment_seq: u32,
    ability_seq: u32,
    pickup_seq: u32,
    display: DisplayController,
    dev_login: String,
    /// Shipping: auto-connect once when the window is ready on Connection.
    #[cfg(not(feature = "dev-diagnostics"))]
    shipping_connect_requested: bool,
    #[cfg(feature = "dev-diagnostics")]
    rf_elapsed: f32,
    #[cfg(feature = "dev-diagnostics")]
    rf_ab_elapsed: f32,
    #[cfg(feature = "dev-diagnostics")]
    rf_proof: Option<RfVertexProof>,
}

impl ClientApp {
    fn new(registry: ContentRegistry) -> Result<Self, String> {
        let mut asset_runtime = crate::asset_runtime::AssetRuntime::new();
        crate::headwear_proof::register_assets(&mut asset_runtime)?;
        let character_visual_pack =
            crate::character_assets::embedded_character_visual_pack(&mut asset_runtime)
                .map_err(|error| format!("PURGATORY character visual pack error: {error}"))?;
        let npc_sheet = SpriteSheet::red_slime(&mut asset_runtime)
            .map_err(|error| format!("PURGATORY red slime sprite error: {error}"))?;
        Ok(Self {
            window: None,
            renderer: None,
            asset_runtime,
            character_visual_pack,
            #[cfg(feature = "dev-diagnostics")]
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
            #[cfg(feature = "dev-diagnostics")]
            last_camera_motion: CameraMotionDebug::default(),
            lifecycle: ClientLifecycle::new(ClientEndpointConfig::dev().server),
            #[cfg(feature = "dev-diagnostics")]
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
            #[cfg(feature = "dev-diagnostics")]
            impairment_seed: NetworkImpairmentConfig::from_env().seed,
            ui_runtime: UIRuntimeState::Idle,
            dialogue_runtime: DialogueRuntime::default(),
            cursor_position: None,
            speech_bubble_hit: None,
            choice_bubble_hits: Vec::new(),
            choice_click_edge: None,
            bubble_click_edge: false,
            dialogue_advance_sent_this_frame: false,
            trace_replica_apply: false,
            trace_scene: false,
            trace_malformed: false,
            last_replica_interactables: 0,
            last_interact_request: String::new(),
            last_interact_result: String::new(),
            #[cfg(feature = "dev-diagnostics")]
            last_interact_kind: None,
            #[cfg(feature = "dev-diagnostics")]
            interact_last_transition: None,
            #[cfg(feature = "dev-diagnostics")]
            interact_trail: Vec::new(),
            registry,
            last_observer: None,
            map_fade: MapFade::default(),
            frozen_presentation: None,
            camera_follow: CameraFollow::default(),
            pending_camera_commit: None,
            pending_center_on_player: false,
            local_presentation: LocalPresentation::default(),
            frame_local: FrameLocalPose::default(),
            #[cfg(feature = "dev-diagnostics")]
            jitter_trace: ForensicTrace::default(),
            reconciled_this_frame: false,
            last_ticks_executed: 0,
            network_frames_this_frame: 0,
            #[cfg(feature = "dev-diagnostics")]
            last_jitter_mode: CameraJitterMode::Normal,
            skeleton: crate::skeleton_debug::HumanoidDebug::new(),
            characters: CharacterPresentationSet::new(),
            presentation_oneshots: PresentationOneShotTable::new(),
            npc_sheet,
            npc_players: HashMap::new(),
            #[cfg(feature = "dev-diagnostics")]
            animation_player: purgatory_animation::AnimationPlayer::new(),
            #[cfg(feature = "dev-diagnostics")]
            selected_animation_sample_t: 0.0,
            #[cfg(feature = "dev-diagnostics")]
            equipment_seq: 0,
            ability_seq: 0,
            pickup_seq: 0,
            display: DisplayController::new(),
            dev_login: purgatory_common::DEFAULT_DEV_LOGIN.to_string(),
            #[cfg(not(feature = "dev-diagnostics"))]
            shipping_connect_requested: false,
            #[cfg(feature = "dev-diagnostics")]
            rf_elapsed: 0.0,
            #[cfg(feature = "dev-diagnostics")]
            rf_ab_elapsed: 0.0,
            #[cfg(feature = "dev-diagnostics")]
            rf_proof: None,
        })
    }

    #[cfg(feature = "dev-diagnostics")]
    fn debug_overlay_visible(&self) -> bool {
        self.debug.as_ref().is_some_and(DebugOverlay::is_visible)
    }

    #[cfg(feature = "dev-diagnostics")]
    #[must_use]
    fn diagnostics_demand(&self) -> DiagnosticsDemand {
        DiagnosticsDemand::compose(self.debug_overlay_visible(), DIAGNOSTICS_IPC_SUBSCRIBED)
    }

    #[cfg(feature = "dev-diagnostics")]
    fn time_scale(&self) -> f32 {
        self.debug
            .as_ref()
            .map(|d| d.ui.time_scale)
            .unwrap_or(1.0)
            .clamp(0.01, 1.0)
    }

    #[cfg(feature = "dev-diagnostics")]
    fn toggle_debug_overlay(&mut self) {
        if let Some(overlay) = &mut self.debug {
            overlay.toggle();
        }
    }

    fn poll_network(&mut self) {
        #[cfg(feature = "dev-diagnostics")]
        if let Some(debug) = &mut self.debug {
            self.lifecycle.set_log_flags(
                debug.ui.log_network_lifecycle,
                debug.ui.verbose_network_trace,
            );
            if let Some(network) = &self.network {
                network.set_log_flags(
                    debug.ui.log_network_lifecycle,
                    debug.ui.verbose_network_trace,
                );
                network.set_impairment_config(NetworkImpairmentConfig::from_profile(
                    debug.ui.impairment_profile,
                    self.impairment_seed,
                ));
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
                self.dialogue_runtime.apply_interact(*event);
                self.ui_runtime.apply_server(*event);
                self.note_interact_runtime();
                self.last_interact_result = interact_result_label(*event);
                println!("6B_INTERACT ui_state {before} -> {}", self.ui_runtime);
            }
            if let crate::network::state::NetworkEvent::DialogueLine { event, .. } = &event {
                match self
                    .dialogue_runtime
                    .apply_line(*event, self.ui_runtime, &self.registry)
                {
                    Ok(()) => println!(
                        "N10_DIALOGUE client_line session={} npc={} beat={} line={} text={:?}",
                        event.session_id,
                        event.npc_content_id,
                        event.beat_index,
                        event.line_index,
                        self.dialogue_runtime.text(&self.registry)
                    ),
                    Err(reason) => {
                        eprintln!("N10_DIALOGUE rejected client projection: {reason:?}")
                    }
                }
            }
            if let crate::network::state::NetworkEvent::DialogueChoiceAccepted { event, .. } =
                &event
                && let Err(reason) = self
                    .dialogue_runtime
                    .apply_choice_accepted(*event, &self.registry)
            {
                eprintln!("N10_DIALOGUE rejected choice acknowledgement: {reason:?}");
            }
            if let crate::network::state::NetworkEvent::PresentationOneShot { event, .. } = &event {
                println!("A5_ONESHOT client_recv {event:?}");
                self.presentation_oneshots.apply_server_event(*event);
            }
            self.lifecycle.apply(event);
        }
        self.dialogue_runtime
            .clear_if_target_missing(|target| self.replica.get(target).is_some());
        if self.lifecycle.screen() != before {
            self.on_screen_changed();
        }
    }

    fn on_screen_changed(&mut self) {
        self.actions.clear();
        self.last_input = PlayerInput::idle();
        self.intent.reset();
        self.ui_runtime = UIRuntimeState::Idle;
        self.dialogue_runtime.clear();
        self.speech_bubble_hit = None;
        self.choice_bubble_hits.clear();
        self.choice_click_edge = None;
        self.bubble_click_edge = false;
        #[cfg(feature = "dev-diagnostics")]
        {
            self.last_interact_kind = None;
            self.interact_last_transition = None;
            self.interact_trail.clear();
        }
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
            self.characters.clear();
            self.npc_players.clear();
            self.presentation_oneshots.clear();
            #[cfg(feature = "dev-diagnostics")]
            {
                self.equipment_seq = 0;
            }
            self.ability_seq = 0;
            self.pickup_seq = 0;
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

    fn refresh_character_presentation(&mut self, frame_dt: f32) {
        let local_id = self.replica.local_player();
        let server_tick = self.replica.last_server_tick();
        self.presentation_oneshots.expire(server_tick);
        let characters: Vec<_> = self
            .replica
            .iter()
            .filter(|entity| {
                entity.kind == ReplicatedKind::Player || is_humanoid_social_npc(entity)
            })
            .collect();
        let interp_poses: Vec<_> = self.interp.poses().to_vec();
        let mut items = Vec::new();
        for entity in characters {
            let key =
                PresentationEntityKey::new(entity.entity_id.index, entity.entity_id.generation);
            let held = self.characters.facing_of(key);
            let equipment = equipment_view_from_replica(entity.equipment);
            let oneshot = self.presentation_oneshots.activity_of(key, server_tick);
            let dead = entity.health.is_some_and(|h| h.current <= 0.0);
            let climb_back = {
                #[cfg(feature = "dev-diagnostics")]
                {
                    self.debug
                        .as_ref()
                        .map(|d| d.ui.presentation_force_climb_back)
                        .unwrap_or(false)
                }
                #[cfg(not(feature = "dev-diagnostics"))]
                {
                    false
                }
            };
            let mut state = if is_humanoid_social_npc(&entity) {
                let (pose, _) =
                    interpolated_or_replica_pose(&interp_poses, entity.entity_id, entity.position);
                from_social_npc(SocialNpcMotion { pose, equipment }, held)
            } else if Some(entity.entity_id) == local_id {
                // Velocity and grounded must come from the same source. Pairing
                // predicted velocity with lagged replica.local_grounded kept
                // airborne locals on Idle/Move (A4 manual-proof failure).
                let (velocity, grounded) = if self.prediction.active() {
                    self.world
                        .player_body()
                        .map(|b| (b.velocity, b.grounded))
                        .unwrap_or((self.frame_local.velocity, self.replica.local_grounded()))
                } else {
                    (entity.velocity, self.replica.local_grounded())
                };
                from_local_with_presentation(
                    LocalMotion {
                        pose: self.frame_local.presented.unwrap_or(entity.position),
                        velocity,
                        grounded,
                        equipment,
                    },
                    held,
                    oneshot,
                    dead,
                )
            } else {
                let (pose, _) =
                    interpolated_or_replica_pose(&interp_poses, entity.entity_id, entity.position);
                from_remote_with_presentation(
                    RemoteMotion {
                        pose,
                        velocity: entity.velocity,
                        equipment,
                    },
                    held,
                    oneshot,
                    dead,
                )
            };
            if entity.kind == ReplicatedKind::Player && climb_back && oneshot.is_none() && !dead {
                state = apply_climb_back_overlay(state, true);
            }
            items.push((key, state));
        }
        self.characters.sync(items, &self.registry, frame_dt);
    }

    /// Apply Proof UI playback requests, advance A2 player at most once, resolve one sample `t`.
    /// A1/A2 diagnostics only — does not drive CharacterPresentationSet A3 runtime.
    #[cfg(feature = "dev-diagnostics")]
    fn resolve_selected_animation_sample_t(&mut self, frame_dt: f32) {
        if let Some(debug) = self.debug.as_mut() {
            let speed = debug.ui.skeleton_a2_speed;
            let _ = self.animation_player.set_speed(speed);
        }

        let mode = self
            .debug
            .as_ref()
            .map(|d| d.ui.animation_proof_mode)
            .unwrap_or(crate::debug::AnimationProofMode::ManualA1);

        self.selected_animation_sample_t = match mode {
            crate::debug::AnimationProofMode::ManualA1 => self
                .debug
                .as_ref()
                .map(|d| d.ui.skeleton_a1_sample_t)
                .unwrap_or(0.0),
            crate::debug::AnimationProofMode::PlaybackA2 => {
                let clip = purgatory_animation::a1_head_loop_clip();
                let _ = self.animation_player.advance(frame_dt, clip);
                self.animation_player.sample_time(clip)
            }
        };

        if let Some(debug) = self.debug.as_mut() {
            debug.ui.selected_animation_sample_t = self.selected_animation_sample_t;
            debug.ui.animation_player_playing = self.animation_player.playing();
        }
    }

    #[cfg(feature = "dev-diagnostics")]
    fn next_equipment_seq(&mut self) -> u32 {
        self.equipment_seq = self.equipment_seq.saturating_add(1);
        self.equipment_seq
    }

    #[cfg(feature = "dev-diagnostics")]
    fn send_debug_equip(&mut self, authored: &str) {
        let Some(def) = self.registry.equipment(authored) else {
            eprintln!("8E_EQUIP unknown authored id {authored}");
            return;
        };
        if self.network.is_none() {
            return;
        }
        let slot = def.slot as u8;
        let seq = self.next_equipment_seq();
        let sent = self.network.as_ref().is_some_and(|network| {
            network.try_send_equip(purgatory_protocol::EquipRequest {
                seq,
                slot,
                // The diagnostics command predates owner-private inventory.
                // It cannot authorize an item without an inventory instance.
                item_instance_id: purgatory_common::ItemInstanceId::from_raw(0),
            })
        });
        if !sent {
            eprintln!("8E_EQUIP send failed seq={seq} id={authored}");
        }
    }

    #[cfg(feature = "dev-diagnostics")]
    fn send_debug_equip_item(&mut self, item_instance_id: purgatory_common::ItemInstanceId) {
        if !self
            .lifecycle
            .view()
            .inventory
            .iter()
            .any(|entry| entry.item_instance_id == item_instance_id)
        {
            eprintln!("11E_EQUIP item is no longer in synchronized inventory: {item_instance_id}");
            return;
        }
        let seq = self.next_equipment_seq();
        let sent = self.network.as_ref().is_some_and(|network| {
            network.try_send_equip(purgatory_protocol::EquipRequest {
                seq,
                slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
                item_instance_id,
            })
        });
        if !sent {
            eprintln!("11E_EQUIP send failed seq={seq} item={item_instance_id}");
        }
    }

    #[cfg(feature = "dev-diagnostics")]
    fn send_debug_unequip(&mut self, slot: u8) {
        if purgatory_simulation::EquipmentSlot::from_u8(slot).is_none() {
            return;
        }
        if self.network.is_none() {
            return;
        }
        let seq = self.next_equipment_seq();
        let sent = self.network.as_ref().is_some_and(|network| {
            network.try_send_unequip(purgatory_protocol::UnequipRequest { seq, slot })
        });
        if !sent {
            eprintln!("8E_UNEQUIP send failed seq={seq} slot={slot}");
        }
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
        #[cfg(feature = "dev-diagnostics")]
        let mode = self
            .debug
            .as_ref()
            .map(|d| d.ui.camera_jitter_mode)
            .unwrap_or(CameraJitterMode::Normal);
        #[cfg(feature = "dev-diagnostics")]
        if mode != self.last_jitter_mode {
            self.local_presentation.request_snap();
            self.last_jitter_mode = mode;
        }
        #[cfg(feature = "dev-diagnostics")]
        let use_raw = mode.use_raw_presentation();
        #[cfg(not(feature = "dev-diagnostics"))]
        let use_raw = false;
        let render_target = if use_raw {
            predicted
        } else if let Some(pose) = predicted {
            let vel = self.prediction.last_tick_velocity();
            let current_tick = self.prediction.tick_pose().unwrap_or(pose);
            Some(compose_local_render_pose(
                pose,
                self.prediction.prev_tick_pose(),
                current_tick,
                vel,
                self.clock.remainder(),
            ))
        } else {
            None
        };
        let presented = if use_raw {
            render_target
        } else {
            let p = self.local_presentation.step(render_target, dt);
            debug_assert_eq!(p, self.local_presentation.pose());
            p
        };
        let extra_dx = match (predicted, render_target) {
            (Some(p), Some(r)) => r[0] - p[0],
            _ => 0.0,
        };
        let tick_y = self
            .prediction
            .tick_pose()
            .or(predicted)
            .map(|p| p[1])
            .unwrap_or(0.0);
        let prev_y = self
            .prediction
            .prev_tick_pose()
            .map(|p| p[1])
            .unwrap_or(tick_y);
        let delta = if self.reconciled_this_frame {
            self.prediction.last_correction_delta()
        } else {
            [0.0, 0.0]
        };
        self.frame_local = FrameLocalPose {
            predicted,
            presented,
            replica: self.replica.local_entity().map(|e| e.position),
            velocity: self.prediction.last_tick_velocity(),
            extra_dx,
            interp_alpha: remainder_alpha(self.clock.remainder()),
            prev_y,
            tick_y,
            auth_tick: self.replica.last_server_tick(),
            replica_seq: self.replica.last_sequence().unwrap_or(0),
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

    #[cfg(feature = "dev-diagnostics")]
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
        } else if self.dialogue_runtime.is_active() {
            self.actions.discard_dialogue_gameplay_edges();
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

    /// General UI escape. Dialogue uses the existing InteractionSession close
    /// request; there is no dialogue-specific cancellation channel.
    fn poll_escape_request(&mut self) {
        if !self.actions.consume_escape_edge()
            || !self.lifecycle.gameplay_actions_allowed()
            || self.gameplay_input_locked()
        {
            return;
        }
        let UIRuntimeState::Active { session_id, target } = self.ui_runtime else {
            return;
        };
        if self.network.is_none() {
            return;
        }
        self.last_interact_request = format!("EscapeClose {session_id}");
        self.ui_runtime.begin_close(session_id, target);
        self.note_interact_runtime();
        if !self
            .network
            .as_ref()
            .expect("checked")
            .try_send_interact_close(session_id)
        {
            self.last_interact_result = "escape close send failed".into();
        }
    }

    fn try_send_dialogue_advance(&mut self, session_id: u32) -> bool {
        if self.dialogue_advance_sent_this_frame {
            return false;
        }
        let Some(network) = self.network.as_ref() else {
            return false;
        };
        self.dialogue_advance_sent_this_frame = true;
        network.try_send_dialogue_advance(session_id)
    }

    fn try_send_dialogue_choice(&mut self, request: purgatory_protocol::DialogueChoose) -> bool {
        if self.dialogue_advance_sent_this_frame {
            return false;
        }
        let Some(network) = self.network.as_ref() else {
            return false;
        };
        self.dialogue_advance_sent_this_frame = true;
        network.try_send_dialogue_choose(request)
    }

    fn poll_bubble_click_request(&mut self) {
        if let Some(index) = self.choice_click_edge.take() {
            if self.lifecycle.gameplay_actions_allowed()
                && !self.gameplay_input_locked()
                && self.dialogue_runtime.select_choice(index, &self.registry)
                && let Some(request) = self.dialogue_runtime.choice_request(&self.registry)
            {
                self.last_interact_request = format!(
                    "DialogueChoose {}:{}:{}",
                    request.session_id, request.beat_index, request.choice_index
                );
                if !self.try_send_dialogue_choice(request) {
                    self.last_interact_result = "dialogue choice send failed".into();
                }
            }
            return;
        }
        let clicked = std::mem::take(&mut self.bubble_click_edge);
        if !clicked || !self.lifecycle.gameplay_actions_allowed() || self.gameplay_input_locked() {
            return;
        }
        if self.dialogue_runtime.acceptance_pending() {
            return;
        }
        let UIRuntimeState::Active { session_id, .. } = self.ui_runtime else {
            return;
        };
        if self
            .dialogue_runtime
            .active()
            .is_some_and(|dialogue| dialogue.session_id == session_id)
        {
            self.last_interact_request = format!("DialogueClick {session_id}");
            if !self.try_send_dialogue_advance(session_id) {
                self.last_interact_result = "dialogue click send failed".into();
            }
        }
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
                if self.dialogue_runtime.acceptance_pending() {
                    return;
                }
                if self
                    .dialogue_runtime
                    .active()
                    .is_some_and(|dialogue| dialogue.session_id == session_id)
                {
                    if self.dialogue_advance_sent_this_frame {
                        return;
                    }
                    if let Some(request) = self.dialogue_runtime.choice_request(&self.registry) {
                        self.last_interact_request = format!(
                            "DialogueChoose {}:{}:{}",
                            request.session_id, request.beat_index, request.choice_index
                        );
                        if !self.try_send_dialogue_choice(request) {
                            self.last_interact_result = "dialogue choice send failed".into();
                        }
                    } else {
                        self.last_interact_request = format!("DialogueAdvance {session_id}");
                        if !self.try_send_dialogue_advance(session_id) {
                            self.last_interact_result = "dialogue advance send failed".into();
                        }
                    }
                    return;
                }
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
        if let Some((target, distance)) = routing.nearest_item
            && distance <= INTERACT_RANGE
        {
            self.pickup_seq = self.pickup_seq.saturating_add(1);
            let sent = self.network.as_ref().expect("checked").try_send_pickup(
                purgatory_protocol::PickupRequest {
                    seq: self.pickup_seq,
                    target,
                },
            );
            self.last_interact_request = format!("Pickup {target}");
            self.last_interact_result = if sent {
                "pickup sent".into()
            } else {
                "pickup send failed".into()
            };
            return;
        }
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
        #[cfg(feature = "dev-diagnostics")]
        {
            let next = self.ui_runtime.kind();
            if let Some(transition) =
                note_kind_change(self.last_interact_kind, next, &mut self.interact_trail)
            {
                self.interact_last_transition = Some(transition);
            }
            self.last_interact_kind = Some(next);
        }
    }

    fn next_ability_seq(&mut self) -> u32 {
        self.ability_seq = self.ability_seq.saturating_add(1);
        self.ability_seq
    }

    fn poll_ability_request(&mut self) {
        if !self.actions.consume_ability_edge() {
            return;
        }
        if !self.lifecycle.gameplay_actions_allowed() {
            return;
        }
        if self.gameplay_input_locked() {
            return;
        }
        if self.dialogue_runtime.is_active() {
            return;
        }
        if self.network.is_none() {
            return;
        }
        let Some(def) = self.registry.ability("skill.debug.practice_sword_strike") else {
            eprintln!("11E_ABILITY missing skill.debug.practice_sword_strike in pack");
            return;
        };
        let ability_id = def.id;
        let seq = self.next_ability_seq();
        let sent = self
            .network
            .as_ref()
            .expect("checked")
            .try_send_ability_activate(purgatory_protocol::AbilityActivateRequest {
                seq,
                ability_id,
                selected: None,
            });
        if !sent {
            eprintln!("9C_ABILITY send failed seq={seq}");
        }
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
        if self.dialogue_runtime.is_active() {
            println!("6C_PORTAL up_ignored reason=dialogue_active");
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
        if self.send_held_cancel_if_window_full() {
            return;
        }
        let Some(network) = self.network.as_ref() else {
            return;
        };
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

    /// ADR-0031: full send window is a HeldCancel barrier, not silent stall.
    fn send_held_cancel_if_window_full(&mut self) -> bool {
        if !self.prediction.pending_window_full() || self.prediction.cancel_pending() {
            return false;
        }
        let Some(network) = self.network.as_ref() else {
            return false;
        };
        let barrier = self.prediction.capture_cancel_barrier();
        if network.try_send_held_cancel() {
            self.prediction.enter_cancel_pending(barrier);
            return true;
        }
        false
    }

    fn advance_simulation(&mut self) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_instant);
        self.last_instant = now;
        let seconds = elapsed.as_secs_f32();
        self.fps = if seconds > 0.0 { 1.0 / seconds } else { 0.0 };

        // Development time scale: scale wall elapsed into the clock only.
        // TICK_RATE_HZ / TICK_DURATION are unchanged.
        #[cfg(feature = "dev-diagnostics")]
        let scale = self.time_scale();
        #[cfg(not(feature = "dev-diagnostics"))]
        let scale = 1.0_f32;
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
        #[cfg(feature = "dev-diagnostics")]
        let detector = self.debug.as_ref().is_some_and(|d| {
            d.ui.position_discontinuity_detector && self.diagnostics_demand().is_active()
        });
        #[cfg(feature = "dev-diagnostics")]
        let log_disc = self
            .debug
            .as_ref()
            .map(|d| d.ui.log_discontinuities)
            .unwrap_or(false);
        #[cfg(feature = "dev-diagnostics")]
        let verbose = self
            .debug
            .as_ref()
            .is_some_and(|d| d.ui.verbose_collision_trace && self.diagnostics_demand().is_active());
        #[cfg(not(feature = "dev-diagnostics"))]
        let verbose = false;
        if update.ticks_executed > 1 {
            self.prediction
                .on_hitch_discontinuity(&self.replica, &mut self.world, tick_after);
            self.local_presentation.request_snap();
            if self.send_held_cancel_if_window_full() {
                return;
            }
            if !self.prediction.pending_window_full() {
                let input = self.sample_tick_input();
                self.last_input = input;
                self.prediction.tick(&mut self.world, input, tick_after);
                self.send_intent_for_tick_input(input);
            } else {
                self.prediction.note_input_stall();
            }
            return;
        }
        for step in 0..update.ticks_executed {
            if self.send_held_cancel_if_window_full() || self.prediction.pending_window_full() {
                self.prediction.note_input_stall();
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
            #[cfg(feature = "dev-diagnostics")]
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
            #[cfg(not(feature = "dev-diagnostics"))]
            let _ = (tick, motion);
        }
    }

    fn update_camera(&mut self, dt: f32) {
        let bounds = self.world.bounds();
        #[cfg(feature = "dev-diagnostics")]
        let mode = self
            .debug
            .as_ref()
            .map(|d| d.ui.camera_jitter_mode)
            .unwrap_or(CameraJitterMode::Normal);
        #[cfg(feature = "dev-diagnostics")]
        let player_pos = if mode.follow_raw_prediction() {
            self.frame_local.predicted.unwrap_or([0.0, 0.0])
        } else {
            self.frame_local.presented.unwrap_or([0.0, 0.0])
        };
        #[cfg(not(feature = "dev-diagnostics"))]
        let player_pos = self.frame_local.presented.unwrap_or([0.0, 0.0]);
        #[cfg(feature = "dev-diagnostics")]
        let follow = self
            .debug
            .as_ref()
            .map(|d| d.ui.camera_follow)
            .unwrap_or(true);
        #[cfg(not(feature = "dev-diagnostics"))]
        let follow = true;
        #[cfg(feature = "dev-diagnostics")]
        let log_camera_disc = self
            .debug
            .as_ref()
            .is_some_and(|d| d.ui.log_discontinuities);
        #[cfg(feature = "dev-diagnostics")]
        let record_camera_disc = self
            .debug
            .as_ref()
            .is_some_and(|d| d.ui.position_discontinuity_detector)
            && self.diagnostics_demand().is_active();
        let center_request = self.pending_center_on_player;
        self.pending_center_on_player = false;
        let camera_position = {
            let Some(renderer) = self.renderer.as_mut() else {
                return;
            };
            let commit = self.pending_camera_commit.take();
            let mut camera = renderer.camera();
            #[cfg(feature = "dev-diagnostics")]
            let previous = camera.position;
            let follow_now = follow || center_request;
            let snap = center_request || matches!(commit, Some(CameraCommit::SnapToPlayer));
            if matches!(commit, Some(CameraCommit::PreservePose)) {
                self.camera_follow.seed_from(camera.position);
            }
            #[cfg(feature = "dev-diagnostics")]
            let freeze = (mode.freeze_camera()
                || self.debug.as_ref().is_some_and(|d| d.ui.rf_freeze_camera))
                && !snap;
            #[cfg(not(feature = "dev-diagnostics"))]
            let freeze = false;
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
            #[cfg(feature = "dev-diagnostics")]
            {
                self.last_camera_motion =
                    CameraMotionDebug::record(previous, camera.position, raw_follow, follow_now);
            }
            #[cfg(not(feature = "dev-diagnostics"))]
            let _ = raw_follow;
            let camera_position = camera.position;
            renderer.set_camera(camera);
            camera_position
        };
        #[cfg(feature = "dev-diagnostics")]
        if record_camera_disc && self.last_camera_motion.discontinuity {
            let tick = self.clock.tick().get();
            let cm = self.last_camera_motion;
            let ev = CollisionHistoryEvent {
                tick,
                subject: DiscSubject::Camera,
                axis: purgatory_simulation::CorrectionAxis::None,
                delta: cm.delta,
                correction: [0.0, 0.0],
                previous_position: cm.previous_position,
                position: camera_position,
                velocity: [0.0, 0.0],
                candidate: None,
                grounded_on: None,
                response_kind: purgatory_simulation::ResponseKind::None,
                discontinuity: true,
            };
            if let Some(debug) = &mut self.debug {
                debug.collision_history.push(ev);
            }
            if log_camera_disc {
                eprintln!(
                    "PURGATORY camera discontinuity tick={tick} prev={:?} delta={:?}",
                    cm.previous_position, cm.delta
                );
            }
        }
        #[cfg(not(feature = "dev-diagnostics"))]
        let _ = camera_position;
    }

    #[cfg(feature = "dev-diagnostics")]
    fn record_jitter_sample(&mut self, dt: f32) {
        if !self.diagnostics_demand().is_active() {
            return;
        }
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

    #[cfg(feature = "dev-diagnostics")]
    fn dump_jitter_trace(&mut self) {
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

    fn apply_framebuffer_size(&mut self, width: u32, height: u32) {
        match self.display.observe_framebuffer(width, height) {
            SurfaceResizeAction::SkipInvalid | SurfaceResizeAction::Unchanged => {}
            SurfaceResizeAction::Reconfigure { width, height } => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(width, height);
                    let bounds = self.world.bounds();
                    let mut cam = renderer.camera();
                    cam.clamp_to_bounds(bounds);
                    renderer.set_camera(cam);
                }
            }
        }
    }

    fn flush_display_requests(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        match self.display.flush_window(&window) {
            WindowFlush::Idle => {}
            WindowFlush::Applied(size) => {
                self.apply_framebuffer_size(size.width, size.height);
                window.request_redraw();
            }
            WindowFlush::PendingEvent => {
                window.request_redraw();
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
        self.dialogue_advance_sent_this_frame = false;
        self.reconciled_this_frame = false;
        self.network_frames_this_frame = 0;
        self.last_ticks_executed = 0;
        self.poll_network();
        // Canonical client frame wall delta (same span historically used as fade_dt).
        // Prefer this over fade-named values for animation; do not pre-multiply speed.
        let frame_dt = Instant::now()
            .saturating_duration_since(self.last_instant)
            .as_secs_f32();
        self.dialogue_runtime
            .tick(Duration::from_secs_f32(frame_dt.max(0.0)));
        let fade_dt = frame_dt;
        #[cfg(feature = "dev-diagnostics")]
        self.resolve_selected_animation_sample_t(frame_dt);
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
            #[cfg(feature = "dev-diagnostics")]
            self.record_jitter_sample(fade_dt);
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
        self.poll_escape_request();
        self.poll_bubble_click_request();
        self.poll_interact_request();
        self.poll_portal_request();
        self.poll_ability_request();

        #[cfg(not(feature = "dev-diagnostics"))]
        self.maybe_shipping_auto_connect();

        #[cfg(feature = "dev-diagnostics")]
        let overlay_open = self.debug_overlay_visible();
        #[cfg(not(feature = "dev-diagnostics"))]
        let overlay_open = false;
        let on_connection = self.lifecycle.screen() == ClientScreen::Connection;
        if on_connection {
            self.characters.clear();
        }
        let camera = self.renderer.as_ref().map(Renderer::camera);
        let Some(camera) = camera else {
            return;
        };

        let mut quads = Vec::new();
        if !on_connection {
            self.interp.sample(Instant::now());
            self.refresh_character_presentation(frame_dt);
            let local_pose = self.frame_local.presented;
            let predicted_pose = self.frame_local.predicted;
            quads = parallax_quads(&camera, self.world.bounds());
            let hold_source = self.map_fade.holds_source_presentation();
            let replica_live = self.replica_matches_local_map() && !hold_source;
            let remote_buf: Vec<PresentationPose> = if hold_source {
                self.frozen_presentation
                    .as_ref()
                    .map(|f| f.remotes.clone())
                    .unwrap_or_default()
            } else if replica_live {
                self.interp
                    .poses()
                    .iter()
                    .copied()
                    .filter(|pose| {
                        self.replica.iter().any(|e| {
                            e.entity_id == pose.entity_id && e.kind == ReplicatedKind::Player
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let remotes: &[PresentationPose] = &remote_buf;
            quads.extend(scene_quads(
                &self.world,
                local_pose,
                remotes,
                crate::skeleton_debug::CHARACTER_VISUAL_SCALE_1,
                {
                    #[cfg(feature = "dev-diagnostics")]
                    {
                        self.debug
                            .as_ref()
                            .map(|d| d.ui.show_local_player_quad)
                            .unwrap_or(true)
                    }
                    #[cfg(not(feature = "dev-diagnostics"))]
                    {
                        true
                    }
                },
            ));
            if let Some(center) = local_pose {
                #[cfg(feature = "dev-diagnostics")]
                let leg_proof = self
                    .debug
                    .as_ref()
                    .map(|d| d.ui.skeleton_front_leg_proof)
                    .unwrap_or_default();
                #[cfg(not(feature = "dev-diagnostics"))]
                let leg_proof = crate::skeleton_debug::FrontLegProof::Off;
                #[cfg(feature = "dev-diagnostics")]
                let arm_proof = self
                    .debug
                    .as_ref()
                    .map(|d| d.ui.skeleton_front_arm_proof)
                    .unwrap_or_default();
                #[cfg(not(feature = "dev-diagnostics"))]
                let arm_proof = crate::skeleton_debug::FrontArmProof::Off;
                #[cfg(feature = "dev-diagnostics")]
                let sample_t = self.selected_animation_sample_t;
                #[cfg(not(feature = "dev-diagnostics"))]
                let sample_t = 0.0_f32;
                self.skeleton
                    .evaluate_at(center, leg_proof, arm_proof, sample_t);
                let draw_skeleton = {
                    #[cfg(feature = "dev-diagnostics")]
                    {
                        self.debug
                            .as_ref()
                            .map(|d| d.ui.show_skeleton)
                            .unwrap_or(true)
                    }
                    #[cfg(not(feature = "dev-diagnostics"))]
                    {
                        true
                    }
                };
                if draw_skeleton {
                    let preview_scale = {
                        #[cfg(feature = "dev-diagnostics")]
                        {
                            self.debug
                                .as_ref()
                                .map(|d| {
                                    crate::skeleton_debug::character_visual_scale(
                                        d.ui.skeleton_debug_preview_2x,
                                    )
                                })
                                .unwrap_or(crate::skeleton_debug::CHARACTER_VISUAL_SCALE_115)
                        }
                        #[cfg(not(feature = "dev-diagnostics"))]
                        {
                            crate::skeleton_debug::CHARACTER_VISUAL_SCALE_115
                        }
                    };
                    quads.extend(crate::skeleton_debug::skeleton_overlay_quads(
                        purgatory_skeleton::humanoid_v0(),
                        self.skeleton.world(),
                        preview_scale,
                        true,
                        false,
                    ));
                }
            }
            let draw_placeholders = {
                #[cfg(feature = "dev-diagnostics")]
                {
                    self.debug
                        .as_ref()
                        .map(|d| d.ui.show_placeholder_character)
                        .unwrap_or(true)
                }
                #[cfg(not(feature = "dev-diagnostics"))]
                {
                    true
                }
            };
            if draw_placeholders {
                let preview_scale = {
                    #[cfg(feature = "dev-diagnostics")]
                    {
                        self.debug
                            .as_ref()
                            .map(|d| {
                                crate::skeleton_debug::character_visual_scale(
                                    d.ui.skeleton_debug_preview_2x,
                                )
                            })
                            .unwrap_or(crate::skeleton_debug::CHARACTER_VISUAL_SCALE_115)
                    }
                    #[cfg(not(feature = "dev-diagnostics"))]
                    {
                        crate::skeleton_debug::CHARACTER_VISUAL_SCALE_115
                    }
                };
                let force_back = {
                    #[cfg(feature = "dev-diagnostics")]
                    {
                        self.debug
                            .as_ref()
                            .map(|d| d.ui.presentation_view_back)
                            .unwrap_or(false)
                    }
                    #[cfg(not(feature = "dev-diagnostics"))]
                    {
                        false
                    }
                };
                let headwear_cell = {
                    #[cfg(feature = "dev-diagnostics")]
                    {
                        self.debug
                            .as_ref()
                            .map(|d| d.ui.headwear_side_cell)
                            .unwrap_or(0)
                    }
                    #[cfg(not(feature = "dev-diagnostics"))]
                    {
                        0_u8
                    }
                };
                let bone_map = self.characters.bone_map();
                for (key, entry) in self.characters.iter_draw_order() {
                    let health = self
                        .replica
                        .iter()
                        .find(|entity| {
                            entity.entity_id.index == key.index
                                && entity.entity_id.generation == key.generation
                        })
                        .and_then(|entity| entity.health);
                    if !immunity_flash_visible(
                        health.is_none_or(|h| h.current > 0.0),
                        health.is_some_and(|h| h.damage_immunity_active),
                        self.replica.last_server_tick(),
                    ) {
                        continue;
                    }
                    let view = if force_back {
                        PresentationView::Back
                    } else {
                        entry.state().view
                    };
                    quads.extend(presentation_debug_quads_with_assets(
                        bone_map,
                        entry,
                        &self.asset_runtime,
                        &self.character_visual_pack,
                        preview_scale,
                        true,
                        view,
                        headwear_cell,
                    ));
                }
            }
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
            let replica_npc_quads = if hold_source || !replica_live {
                Vec::new()
            } else {
                npc_quads(
                    &self.replica,
                    &self.interp,
                    &self.presentation_oneshots,
                    &self.npc_sheet,
                    &mut self.npc_players,
                    frame_dt,
                )
            };
            let interactable_n = replica_interactable_quads.len() + replica_portal_quads.len();
            quads.extend(replica_interactable_quads);
            quads.extend(replica_portal_quads);
            quads.extend(item_drop_quads(&self.replica));
            quads.extend(replica_npc_quads);
            self.trace_scene_once(&camera, local_pose, interactable_n, quads.len());
            #[cfg(feature = "dev-diagnostics")]
            if overlay_open {
                let ui = self
                    .debug
                    .as_ref()
                    .map(|d| d.ui.clone())
                    .unwrap_or_default();
                if ui.show_overlay_gizmos {
                    let mut gizmos = footnote_debug_quads(&self.world, &ui, local_pose);
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
                        gizmos.extend(aoi_entity_debug_quads(&rows));
                    }
                    if ui.show_parallax_debug {
                        gizmos.extend(parallax_debug_quads(&camera));
                    }
                    if ui.show_interpolation_gizmos && replica_live {
                        gizmos.extend(interpolation_gizmos(&self.replica, self.interp.poses()));
                    }
                    if ui.show_prediction_gizmos && replica_live {
                        gizmos.extend(prediction_gizmos(
                            &self.replica,
                            predicted_pose,
                            self.prediction.active(),
                        ));
                    }
                    if ui.show_camera_deadzone {
                        gizmos.extend(camera_deadzone_quads(
                            camera.position,
                            local_pose.unwrap_or([0.0, 0.0]),
                            self.camera_follow.desired,
                            self.camera_follow.config.half_x,
                            self.camera_follow.config.half_y,
                        ));
                    }
                    append_debug_gizmos(&mut quads, gizmos, MAX_QUADS);
                }
            }
            #[cfg(not(feature = "dev-diagnostics"))]
            let _ = (overlay_open, predicted_pose);
            #[cfg(feature = "dev-diagnostics")]
            self.append_rf_scene(&mut quads, &camera, frame_dt);
        }
        if let Some(fade) = self.map_fade.overlay_quad(&camera) {
            quads.push(fade);
        }

        let viewport = self.renderer.as_ref().and_then(|renderer| {
            let (width, height) = renderer.surface_size();
            constrained_pixel_viewport(width, height)
        });
        let speech_bubble = if on_connection {
            None
        } else {
            self.dialogue_runtime.active().and_then(|dialogue| {
                let text = self.dialogue_runtime.text(&self.registry)?;
                let target = self.replica.get(dialogue.target)?;
                Some(layout_speech_bubble(
                    text,
                    target.position,
                    camera,
                    viewport?,
                ))
            })
        };
        self.speech_bubble_hit = speech_bubble.as_ref().map(|bubble| bubble.hit_bounds);
        let local_player_pose = self.frame_local.presented;
        let choice_bubble = viewport.and_then(|viewport| {
            let choices = self.dialogue_runtime.choices(&self.registry)?;
            Some(layout_choice_bubble(
                choices,
                self.dialogue_runtime.selected_choice(),
                local_player_pose?,
                camera,
                viewport,
            ))
        });
        let player_response = viewport.and_then(|viewport| {
            Some(layout_speech_bubble_with_offset(
                self.dialogue_runtime.player_text()?,
                local_player_pose?,
                camera,
                viewport,
                132.0,
            ))
        });
        self.choice_bubble_hits = choice_bubble
            .as_ref()
            .map(|bubble| bubble.choice_hits.clone())
            .unwrap_or_default();
        let mut ui_rects: Vec<UiRect> = Vec::new();
        let mut ui_text: Vec<TextBlock> = Vec::new();
        if let Some(bubble) = speech_bubble {
            ui_rects.extend(bubble.rects);
            ui_text.push(bubble.text);
        }
        if let Some(bubble) = choice_bubble {
            ui_rects.extend(bubble.rects);
            ui_text.push(bubble.text);
        }
        if let Some(bubble) = player_response {
            ui_rects.extend(bubble.rects);
            ui_text.push(bubble.text);
        }

        let Some(window) = self.window.clone() else {
            return;
        };
        #[cfg(feature = "dev-diagnostics")]
        let demand = self.diagnostics_demand();
        #[cfg(feature = "dev-diagnostics")]
        let persistent = self
            .debug
            .as_ref()
            .is_some_and(|d| has_persistent_dev_warnings(&d.ui));
        #[cfg(feature = "dev-diagnostics")]
        let frame = if demand.is_active() {
            self.assemble_diagnostics_frame(on_connection)
        } else if on_connection || persistent {
            self.display_only_frame(&window)
        } else {
            DiagnosticsFrame::default()
        };

        #[cfg(feature = "dev-diagnostics")]
        let mut actions = Vec::new();
        let status = {
            let Some(renderer) = self.renderer.as_mut() else {
                return;
            };
            #[cfg(feature = "dev-diagnostics")]
            {
                let show_ab = self.debug.as_ref().is_some_and(|d| d.ui.show_rf_ab);
                if show_ab {
                    if frame_dt.is_finite() && frame_dt > 0.0 && frame_dt < 1.0 {
                        self.rf_ab_elapsed += frame_dt;
                    }
                } else {
                    self.rf_ab_elapsed = 0.0;
                }
                renderer.set_rf_ab_proof(show_ab.then_some(self.rf_ab_elapsed));
                let overlay = self.debug.as_mut();
                let frontend = self.frontend.as_ref();
                let server = format!("{}", self.lifecycle.view().server);
                let line = self.lifecycle.view().frontend_status();
                let can_connect = self.lifecycle.can_connect();
                let on_connection = self.lifecycle.screen() == ClientScreen::Connection;
                let login = &mut self.dev_login;
                renderer.render(&quads, &ui_rects, &ui_text, |pass| {
                    let Some(overlay) = overlay else {
                        return Vec::new();
                    };
                    let (extras, commands, _) = overlay.submit_frame(
                        &window,
                        pass,
                        &frame,
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
                        self.replica.local_entity().and_then(|entity| entity.health),
                    );
                    actions = commands;
                    extras
                })
            }
            #[cfg(not(feature = "dev-diagnostics"))]
            {
                let _ = (&window, on_connection);
                renderer.render(&quads, &ui_rects, &ui_text, |_| Vec::new())
            }
        };

        #[cfg(feature = "dev-diagnostics")]
        self.apply_debug_commands(actions);

        self.flush_display_requests();

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

    #[cfg(not(feature = "dev-diagnostics"))]
    fn maybe_shipping_auto_connect(&mut self) {
        if self.shipping_connect_requested {
            return;
        }
        if self.window.is_none() {
            return;
        }
        if self.lifecycle.screen() != ClientScreen::Connection {
            return;
        }
        if !self.lifecycle.can_connect() {
            return;
        }
        self.shipping_connect_requested = true;
        println!(
            "PURGATORY shipping build: auto-connecting with login={} (no Connection Frontend)",
            self.dev_login
        );
        self.request_connect();
    }

    #[cfg(feature = "dev-diagnostics")]
    fn display_only_frame(&self, window: &winit::window::Window) -> DiagnosticsFrame {
        let surface = self
            .renderer
            .as_ref()
            .map(|r| r.surface_size())
            .unwrap_or((0, 0));
        let mut display = collect_display_debug(window, surface, &self.display.settings());
        display.gameplay_pixel = self.renderer.as_ref().and_then(|renderer| {
            renderer
                .gameplay_pixel_viewport()
                .map(|vp| (vp.x, vp.y, vp.width, vp.height))
        });
        if let Some(renderer) = self.renderer.as_ref() {
            display.internal_render = renderer.internal_render_extent();
            display.internal_render_clamped = renderer.internal_render_clamped();
            display.render_scale_percent = renderer.render_scale().percent();
            display.world_msaa_samples = renderer.world_msaa_sample_count();
            display.world_msaa_4x_supported = renderer.msaa_4x_supported();
        }
        DiagnosticsFrame::display_only(display)
    }

    #[cfg(feature = "dev-diagnostics")]
    fn assemble_diagnostics_frame(&mut self, on_connection: bool) -> DiagnosticsFrame {
        use purgatory_simulation::TICK_RATE_HZ;

        let impairment = self
            .network
            .as_mut()
            .map(|n| n.poll_impairment_metrics())
            .unwrap_or_default();

        let Some(window) = self.window.as_ref() else {
            return self.empty_diagnostics_frame();
        };
        let Some(renderer) = self.renderer.as_ref() else {
            return self.empty_diagnostics_frame();
        };
        let cam = renderer.camera();
        let (physics, roster) = WorldRosterDiagnostics::from_world(&self.world, self.last_input);
        let mut display =
            collect_display_debug(window, renderer.surface_size(), &self.display.settings());
        display.gameplay_pixel = renderer
            .gameplay_pixel_viewport()
            .map(|vp| (vp.x, vp.y, vp.width, vp.height));
        display.internal_render = renderer.internal_render_extent();
        display.internal_render_clamped = renderer.internal_render_clamped();
        display.render_scale_percent = renderer.render_scale().percent();
        display.world_msaa_samples = renderer.world_msaa_sample_count();
        display.world_msaa_4x_supported = renderer.msaa_4x_supported();
        let runtime = RuntimeDiagnostics {
            frames: renderer.frames_drawn(),
            tick: self.clock.tick().get(),
            tick_rate_hz: TICK_RATE_HZ,
            window_width: renderer.surface_size().0,
            window_height: renderer.surface_size().1,
            display,
            fps: self.fps,
        };
        let map_id = self.last_observer.map(|(m, _, _)| m).unwrap_or(0);
        let replica_addr = self.replica.observer_address();
        let input_locked = self.gameplay_input_locked();
        let sim_local_pose = self.presented_local_pose();
        let origin = sim_local_pose.unwrap_or([0.0, 0.0]);
        let rects = aoi_policy_rects(origin, self.world.bounds());
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
        let inspector = entity_inspector::build(
            roster.entities.iter().map(WorldEntityInput::from),
            physics.player.map(|p| p.id),
            &rows,
        );
        let mut replica_label_world = world_space_label_entries(
            world_space_labels_eligible(
                self.replica_matches_local_map(),
                self.map_fade.is_idle(),
                self.map_fade.presentation_ready(),
            ),
            &rows,
            |r| compact_world_space_label(r.role).to_string(),
        );
        bind_local_player_label_pose(&mut replica_label_world, self.frame_local.presented);
        let aoi = self.replica.aoi_debug();
        let world = WorldDiagnostics {
            roster,
            stage_name: "content",
            map_id,
            map_debug_name: self
                .registry
                .map_by_map_id(MapId::from_raw(map_id))
                .map(|m| m.debug_name.clone())
                .unwrap_or_else(|| "-".into()),
            observer_address: if self.replica.last_sequence().is_some() {
                format!(
                    "map={} ch={} inst={}",
                    replica_addr.0, replica_addr.1, replica_addr.2
                )
            } else {
                self.last_observer
                    .map(|(m, c, i)| format!("map={m} ch={c} inst={i}"))
                    .unwrap_or_else(|| "-".into())
            },
            observer_map: replica_addr.0,
            observer_channel: replica_addr.1,
            observer_instance: replica_addr.2,
            transition_banner: self.map_fade.debug_banner().unwrap_or_default(),
            transition_missing: if self.map_fade.is_idle() {
                String::new()
            } else {
                self.map_fade.flags().missing_csv(self.map_fade.kind())
            },
            transition_stalled: self.map_fade.stalled(),
            input_gate_label: input_gate_debug_label(
                input_locked,
                self.map_fade.is_idle(),
                matches!(self.map_fade.kind(), TransitionKind::Map),
                self.last_observer,
                replica_addr,
            )
            .into(),
            input_movement_neutral: input_locked,
            content_registry_count: self.registry.definition_count() as u32,
            content_map_labels: self
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
                .collect(),
            observer_entity: self.replica.local_player().map(|id| id.to_string()),
            observer_enter_bounds: format!(
                "[{:.1},{:.1}] x [{:.1},{:.1}]",
                rects.enter.min_x(),
                rects.enter.max_x(),
                rects.enter.min_y(),
                rects.enter.max_y()
            ),
            observer_leave_bounds: format!(
                "[{:.1},{:.1}] x [{:.1},{:.1}]",
                rects.leave.min_x(),
                rects.leave.max_x(),
                rects.leave.min_y(),
                rects.leave.max_y()
            ),
            aoi_candidates: aoi.map(|d| d.candidates),
            aoi_known: aoi.map(|d| d.known),
            aoi_want_enter: aoi.map(|d| d.want_enter),
            aoi_want_leave: aoi.map(|d| d.want_leave),
            replica_entity_rows: rows
                .iter()
                .map(|r| {
                    let recent = r.recent.map(|s| format!(" · {s}")).unwrap_or_default();
                    format!("{} | {}{recent}", r.label, r.band_label)
                })
                .collect(),
            replica_recent_left: self
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
                .collect(),
            replica_label_world,
            inspector,
        };
        let routing = replica_interact_routing(&self.replica);
        let interp = self.interp.diagnostics();
        let pred = self.prediction.diagnostics(&self.world, &self.replica);
        let network = NetworkDiagnostics {
            lifecycle: self.lifecycle.snapshot(),
            inventory: self.lifecycle.view().inventory.clone(),
            net_input_seq: self.intent.sequence,
            net_input_sent: self.intent.commands_sent,
            net_move_axis: self.intent.move_axis.to_i8(),
            net_jump: self.intent.jump_pressed,
            net_down: self.intent.down_held,
            replica_seq: self.replica.last_sequence(),
            replica_tick: self.replica.last_server_tick(),
            replica_entities: self.replica.len() as u32,
            replica_epoch: self.replica.observer_epoch(),
            replica_frame_enters: self.replica.last_frame_enters(),
            replica_frame_updates: self.replica.last_frame_updates(),
            replica_frame_leaves: self.replica.last_frame_leaves(),
            replica_total_enters: self.replica.total_enters(),
            replica_total_updates: self.replica.total_updates(),
            replica_total_leaves: self.replica.total_leaves(),
            replica_local: self.replica.local_player().map(|id| id.to_string()),
            replica_stale: self.replica.stale_ignored,
            replica_duplicate: self.replica.duplicate_ignored,
            replica_malformed: self.snapshot_malformed,
            replica_age_ms: self
                .replica
                .snapshot_age(Instant::now())
                .map(|d| d.as_millis() as u64),
            interact_ui: self.ui_runtime.to_string(),
            interact_status: InteractStatusView::from_runtime(
                &self.ui_runtime,
                self.interact_last_transition.clone(),
                &trail_display(&self.interact_trail),
            ),
            interact_nearest: routing.nearest_interaction.map(|(id, _)| id.to_string()),
            interact_nearest_distance: routing.nearest_interaction.map(|(_, dist)| dist),
            interact_nearest_portal: routing.nearest_portal.map(|(id, _)| id.to_string()),
            interact_nearest_portal_distance: routing.nearest_portal.map(|(_, dist)| dist),
            portal_eligible: routing.portal_eligible,
            interact_last_request: if self.last_interact_request.is_empty() {
                "-".into()
            } else {
                self.last_interact_request.clone()
            },
            interact_last_result: if self.last_interact_result.is_empty() {
                "-".into()
            } else {
                self.last_interact_result.clone()
            },
            replica_interactables: replica_kind_labels(&self.replica, ReplicatedKind::Interactable),
            replica_portals: replica_kind_labels(&self.replica, ReplicatedKind::Portal),
            replica_player_pos: self.replica.local_entity().map(|e| e.position),
            nearest_portal_pos: routing.nearest_portal_pos,
            interp,
            remote_motion: remote_motion_probe(
                &self.replica,
                &self.interp,
                &self.characters,
                self.replica
                    .local_entity()
                    .map(|e| aoi_policy_rects(e.position, self.world.bounds())),
            ),
            pred,
            impairment,
        };
        let camera = CameraDiagnostics {
            position: cam.position,
            viewport_width: cam.viewport_width,
            viewport_height: cam.viewport_height,
            parallax_far: PARALLAX_FAR,
            parallax_mid: PARALLAX_MID,
            parallax_near: PARALLAX_NEAR,
            motion: self.last_camera_motion,
            presented_player_pos: self.frame_local.presented,
            desired: self.camera_follow.desired,
            deadzone_half_x: self.camera_follow.config.half_x,
            deadzone_half_y: self.camera_follow.config.half_y,
            smooth_time_x: self.camera_follow.config.smooth_time_x,
            smooth_time_y: self.camera_follow.config.smooth_time_y,
            following_x: self.camera_follow.following_x,
            following_y: self.camera_follow.following_y,
            jitter: self.jitter_trace.summary(self.local_presentation.offset()),
        };
        let mut presentation = PresentationDiagnostics {
            skeleton_inspect: None,
            characters: self.characters.len(),
            bound: 0,
            hidden: 0,
            missing: Vec::new(),
        };
        if !on_connection
            && self.frame_local.presented.is_some()
            && let Some(renderer) = self.renderer.as_ref()
        {
            let idx = self
                .debug
                .as_ref()
                .map(|d| d.ui.skeleton_inspect_index.min(15))
                .unwrap_or(0);
            let bone = purgatory_skeleton::BoneIndex::from_u8(idx);
            if let (Some(local), Some(world_xf)) = (
                self.skeleton.local().get(bone),
                self.skeleton.world().get(bone),
            ) {
                let preview_scale = self
                    .debug
                    .as_ref()
                    .map(|d| {
                        crate::skeleton_debug::character_visual_scale(
                            d.ui.skeleton_debug_preview_2x,
                        )
                    })
                    .unwrap_or(crate::skeleton_debug::CHARACTER_VISUAL_SCALE_115);
                let root = self
                    .skeleton
                    .world()
                    .get(purgatory_skeleton::ROOT)
                    .map(|t| t.translation)
                    .unwrap_or(world_xf.translation);
                let preview_t = crate::skeleton_debug::scale_about_root(
                    world_xf.translation,
                    root,
                    preview_scale,
                );
                let ndc = renderer.camera().world_to_ndc(preview_t);
                let screen = renderer
                    .gameplay_pixel_viewport()
                    .map(|vp| vp.ndc_to_px(ndc));
                presentation.skeleton_inspect = Some(crate::debug::SkeletonInspectDebug {
                    index: idx,
                    name: crate::skeleton_debug::HUMANOID_V0_BONE_LABELS[usize::from(idx)].into(),
                    local_t: local.translation,
                    local_r: local.rotation,
                    world_t: world_xf.translation,
                    world_r: world_xf.rotation,
                    screen,
                });
            }
        }
        if let Some(id) = self.replica.local_player() {
            let key = PresentationEntityKey::new(id.index, id.generation);
            if let Some(entry) = self.characters.get(key) {
                presentation.bound = entry.bound().len();
                presentation.hidden = entry.hidden_base();
                presentation.missing = entry
                    .missing()
                    .iter()
                    .map(|m| {
                        let label = self.registry.label(m.content_id).unwrap_or("unknown");
                        format!("{:?} {label} {:?}", m.slot, m.reason)
                    })
                    .collect();
            }
        }
        DiagnosticsFrame {
            runtime,
            physics,
            world,
            network,
            camera,
            presentation,
            rf: self.rf_proof,
        }
    }

    #[cfg(feature = "dev-diagnostics")]
    fn empty_diagnostics_frame(&self) -> DiagnosticsFrame {
        let (physics, roster) = WorldRosterDiagnostics::from_world(&self.world, self.last_input);
        DiagnosticsFrame {
            runtime: RuntimeDiagnostics {
                tick: self.clock.tick().get(),
                tick_rate_hz: purgatory_simulation::TICK_RATE_HZ,
                fps: self.fps,
                ..Default::default()
            },
            physics,
            world: WorldDiagnostics {
                roster,
                stage_name: "content",
                map_debug_name: "-".into(),
                observer_address: "-".into(),
                input_gate_label: "INPUT: ACTIVE".into(),
                observer_enter_bounds: "—".into(),
                observer_leave_bounds: "—".into(),
                ..Default::default()
            },
            network: NetworkDiagnostics {
                lifecycle: self.lifecycle.snapshot(),
                interact_ui: "Idle".into(),
                interact_last_request: "-".into(),
                interact_last_result: "-".into(),
                interp: self.interp.diagnostics(),
                pred: self.prediction.diagnostics(&self.world, &self.replica),
                ..Default::default()
            },
            camera: CameraDiagnostics {
                parallax_far: PARALLAX_FAR,
                parallax_mid: PARALLAX_MID,
                parallax_near: PARALLAX_NEAR,
                motion: self.last_camera_motion,
                ..Default::default()
            },
            presentation: PresentationDiagnostics::default(),
            rf: self.rf_proof,
        }
    }

    #[cfg(feature = "dev-diagnostics")]
    fn append_rf_scene(&mut self, quads: &mut Vec<DrawQuad>, camera: &Camera, dt: f32) {
        let show_ab = self.debug.as_ref().is_some_and(|d| d.ui.show_rf_ab);
        let show = self.debug.as_ref().is_some_and(|d| d.ui.show_rf_scene) && !show_ab;
        if !show {
            self.rf_elapsed = 0.0;
            self.rf_proof = None;
            return;
        }
        if dt.is_finite() && dt > 0.0 && dt < 1.0 {
            self.rf_elapsed += dt;
        }
        let remaining = MAX_QUADS.saturating_sub(quads.len());
        quads.extend(rf_scene_quads(self.rf_elapsed).into_iter().take(remaining));
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        let Some((iw, ih)) = renderer.internal_render_extent() else {
            return;
        };
        let Some(output) = renderer.gameplay_pixel_viewport() else {
            return;
        };
        let internal = PixelViewport {
            x: 0,
            y: 0,
            width: iw,
            height: ih,
        };
        let proof = rf_probe_proof(
            self.rf_elapsed,
            camera,
            internal,
            output,
            self.rf_proof.as_ref(),
        );
        if self.debug.as_ref().is_some_and(|d| d.ui.rf_log_vertices) {
            eprintln!("{}", proof.compact_line());
        }
        self.rf_proof = Some(proof);
    }

    #[cfg(feature = "dev-diagnostics")]
    fn apply_debug_commands(&mut self, commands: Vec<DebugCommand>) {
        for command in commands {
            match command {
                DebugCommand::ResetPlayer => {
                    let replica = reset_player_uses_replica(self.replica.local_entity().is_some());
                    if replica {
                        self.prediction.force_reanchor_from_replica(
                            &self.replica,
                            &mut self.world,
                            self.clock.tick().get(),
                        );
                        self.local_presentation.request_snap();
                        println!(
                            "PURGATORY debug: Reanchor Prediction → snap to replica (no server teleport; pose may not move)"
                        );
                    } else {
                        self.world
                            .apply_debug_action(purgatory_simulation::DebugAction::ResetPlayer);
                        self.local_presentation.request_snap();
                        self.pending_center_on_player = true;
                        println!("PURGATORY debug: Reset to Spawn → local spawn reset");
                    }
                    if let Some(debug) = self.debug.as_mut() {
                        debug.ui.note_dev_action_flash(reset_action_flash(replica));
                    }
                }
                DebugCommand::ResetToSpawn => {
                    if self.lifecycle.screen() == ClientScreen::Game {
                        if let Some(network) = &self.network {
                            println!("DEV_RESET send DevResetPlayer");
                            if network.try_send_dev_reset_player() {
                                if let Some(debug) = self.debug.as_mut() {
                                    debug.ui.note_dev_action_flash(RESET_TO_SPAWN_FLASH);
                                }
                            } else {
                                eprintln!("DEV_RESET send failed (input channel full or closed)");
                            }
                        }
                    } else {
                        self.world
                            .apply_debug_action(purgatory_simulation::DebugAction::ResetPlayer);
                        self.local_presentation.request_snap();
                        self.pending_center_on_player = true;
                        println!("PURGATORY debug: Reset to Spawn Point -> local spawn reset");
                        if let Some(debug) = self.debug.as_mut() {
                            debug.ui.note_dev_action_flash(RESET_TO_SPAWN_FLASH);
                        }
                    }
                }
                DebugCommand::Respawn => {
                    if self.lifecycle.screen() == ClientScreen::Game
                        && self
                            .replica
                            .local_entity()
                            .and_then(|entity| entity.health)
                            .is_some_and(|health| health.current <= 0.0)
                        && let Some(network) = &self.network
                    {
                        println!("RESPAWN send");
                        if !network.try_send_respawn() {
                            eprintln!("RESPAWN send failed (input channel full or closed)");
                        }
                    }
                }
                DebugCommand::Connect => self.request_connect(),
                DebugCommand::Disconnect => self.request_disconnect(),
                DebugCommand::SetChannel(channel) => {
                    if let Some(network) = &self.network {
                        println!("6D_CHANNEL send DevSetChannel channel={channel}");
                        if !network.try_send_dev_set_channel(channel) {
                            eprintln!("6D_CHANNEL send failed (input channel full or closed)");
                        }
                    }
                }
                DebugCommand::SetMoveSpeed(speed) => {
                    let local_speed = speed.map(|value| {
                        crate::debug::sanitize_debug_move_speed(f32::from(value) / 100.0)
                    });
                    if let Some(player) = self.world.player_id() {
                        let _ = self.world.set_player_speed_override(player, local_speed);
                    }
                    if self.lifecycle.screen() == ClientScreen::Game
                        && let Some(network) = &self.network
                        && !network.try_send_dev_set_speed(speed)
                    {
                        eprintln!("DEV_SPEED send failed");
                    }
                }
                DebugCommand::SetJumpSpeed(jump) => {
                    let local_jump = jump.map(|value| {
                        crate::debug::sanitize_debug_jump_speed(f32::from(value) / 100.0)
                    });
                    if let Some(player) = self.world.player_id() {
                        let _ = self
                            .world
                            .set_player_jump_speed_override(player, local_jump);
                    }
                    if self.lifecycle.screen() == ClientScreen::Game
                        && let Some(network) = &self.network
                        && !network.try_send_dev_set_jump(jump)
                    {
                        eprintln!("DEV_JUMP send failed");
                    }
                }
                DebugCommand::SpawnNpc(npc_content_id) => {
                    if self.lifecycle.screen() == ClientScreen::Game
                        && let Some(network) = &self.network
                    {
                        println!("DEV_NPC_SPAWN send npc={npc_content_id}");
                        if network.try_send_dev_spawn_npc(npc_content_id) {
                            if let Some(debug) = self.debug.as_mut() {
                                debug.ui.note_dev_action_flash("NPC spawn request sent");
                            }
                        } else {
                            eprintln!("DEV_NPC_SPAWN send failed");
                        }
                    }
                }
                DebugCommand::SetResolution(resolution) => {
                    if self.display.set_resolution(resolution).is_err() {
                        eprintln!(
                            "PURGATORY display: rejected invalid resolution {}x{}",
                            resolution.width, resolution.height
                        );
                    }
                }
                DebugCommand::SetRenderScale(scale) => {
                    if self.display.set_render_scale(scale) {
                        if let Some(renderer) = self.renderer.as_mut() {
                            renderer.set_render_scale(scale);
                            let gameplay = renderer.gameplay_pixel_viewport();
                            let internal = renderer.internal_render_extent();
                            println!(
                                "PURGATORY display: render_scale {}% framebuffer {}x{} gameplay {} internal {} fov_unchanged",
                                scale.percent(),
                                renderer.surface_size().0,
                                renderer.surface_size().1,
                                gameplay
                                    .map(|vp| format!("{}x{}", vp.width, vp.height))
                                    .unwrap_or_else(|| "—".into()),
                                internal
                                    .map(|(w, h)| format!("{w}x{h}"))
                                    .unwrap_or_else(|| "—".into()),
                            );
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                }
                DebugCommand::SetWorldMsaa(mode) => {
                    if let Some(renderer) = self.renderer.as_mut() {
                        let before = renderer.camera().view_proj_column_major();
                        if renderer.set_world_msaa(mode) {
                            let after = renderer.camera().view_proj_column_major();
                            println!(
                                "PURGATORY display: world_msaa {} samples={} 4x_supported={} fov_unchanged={}",
                                renderer.world_msaa().as_str(),
                                renderer.world_msaa_sample_count(),
                                renderer.msaa_4x_supported(),
                                before == after
                            );
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                }
                DebugCommand::PresentationAttack => {
                    if let Some(network) = &self.network {
                        println!("A5_ONESHOT send Attack");
                        if !network.try_send_dev_presentation_oneshot(1) {
                            eprintln!(
                                "A5_ONESHOT send Attack failed (input channel full or closed)"
                            );
                        }
                    }
                }
                DebugCommand::PresentationHurt => {
                    if let Some(network) = &self.network {
                        println!("A5_ONESHOT send Hurt");
                        if !network.try_send_dev_presentation_oneshot(2) {
                            eprintln!("A5_ONESHOT send Hurt failed (input channel full or closed)");
                        }
                    }
                }
                DebugCommand::Equip(authored) => self.send_debug_equip(authored),
                DebugCommand::EquipItem(item) => self.send_debug_equip_item(item),
                DebugCommand::UnequipSlot(slot) => self.send_debug_unequip(slot),
                DebugCommand::UnequipAll => {
                    for slot in 0..purgatory_simulation::EquipmentSlot::COUNT as u8 {
                        self.send_debug_unequip(slot);
                    }
                }
                DebugCommand::AnimationPlay => self.animation_player.set_playing(true),
                DebugCommand::AnimationPause => self.animation_player.set_playing(false),
                DebugCommand::AnimationReset => self.animation_player.reset(),
                DebugCommand::DumpJitterTrace => self.dump_jitter_trace(),
                DebugCommand::ImpairmentStall { ms } => {
                    if let Some(network) = &self.network {
                        let _ =
                            network.try_trigger_input_stall(Duration::from_millis(u64::from(ms)));
                    }
                }
                DebugCommand::ResetImpairmentMetrics => {
                    if let Some(network) = &self.network {
                        let _ = network.try_reset_impairment_metrics();
                    }
                }
                DebugCommand::ClearNetworkHistory => self.lifecycle.clear_history(),
                DebugCommand::CenterOnPlayer => self.pending_center_on_player = true,
            }
        }
    }
}

#[cfg(feature = "dev-diagnostics")]
fn remote_motion_probe(
    replica: &crate::replica::ReplicatedWorld,
    interp: &crate::interp::InterpolationBuffer,
    characters: &CharacterPresentationSet,
    rects: Option<purgatory_simulation::AoiRects>,
) -> RemoteMotionProbe {
    let local = replica.local_player();
    let Some(entity) = replica
        .iter()
        .find(|entity| entity.kind == ReplicatedKind::Player && Some(entity.entity_id) != local)
    else {
        return RemoteMotionProbe::default();
    };
    let (interp_pos, used_interp) =
        interpolated_or_replica_pose(interp.poses(), entity.entity_id, entity.position);
    let key = PresentationEntityKey::new(entity.entity_id.index, entity.entity_id.generation);
    let entry = characters.get(key);
    let presented = entry.map(|e| e.state().pose);
    let presented_root = entry.map(|e| e.prepared().input.root_position);
    let (history_with_entity, entity_oldest_tick, entity_newest_tick) =
        interp.remote_history_span(entity.entity_id);
    let unique = interp.remote_unique_transform_span(entity.entity_id);
    let bracket = interp.remote_entity_bracket(entity.entity_id);
    let aoi_band = rects.map(|r| band_label(classify_band(entity.position, r)));
    let observer_distance = local.and_then(|id| replica.get(id)).map(|local_e| {
        let dx = entity.position[0] - local_e.position[0];
        let dy = entity.position[1] - local_e.position[1];
        (dx * dx + dy * dy).sqrt()
    });
    RemoteMotionProbe {
        entity_index: Some(entity.entity_id.index),
        entity_generation: Some(entity.entity_id.generation),
        attachments: entry.map(|e| e.bound().len() as u32).unwrap_or(0),
        used_interp,
        auth: Some(entity.position),
        interp: Some(interp_pos),
        presented,
        presented_root,
        dx_auth_interp: Some(entity.position[0] - interp_pos[0]),
        dx_interp_presented: presented.map(|p| interp_pos[0] - p[0]),
        aoi_band,
        observer_distance,
        last_auth_transform_tick: (entity.last_transform_tick > 0)
            .then_some(entity.last_transform_tick),
        effective_received_gap: if entity.last_transform_gap > 0 {
            Some(entity.last_transform_gap)
        } else {
            unique.last_gap
        },
        max_received_gap: (entity.max_transform_gap > 0).then_some(entity.max_transform_gap),
        unique_history_ticks: unique.ticks,
        unique_history_len: unique.ticks_len,
        unique_count: unique.unique_count,
        unique_oldest_tick: unique.oldest_tick,
        unique_newest_tick: unique.newest_tick,
        entity_bracket_a: bracket.sample_a,
        entity_bracket_b: bracket.sample_b,
        entity_alpha: bracket.alpha,
        entity_clamped_newest: bracket.clamped_newest,
        history_with_entity,
        entity_oldest_tick,
        entity_newest_tick,
    }
}

fn capture_source_presentation(
    interp: &crate::interp::InterpolationBuffer,
    replica: &crate::replica::ReplicatedWorld,
) -> FrozenPresentation {
    let mut remotes = interp
        .poses()
        .iter()
        .copied()
        .filter(|pose| {
            replica
                .get(pose.entity_id)
                .is_some_and(|e| e.kind == ReplicatedKind::Player)
        })
        .collect::<Vec<_>>();
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
#[allow(dead_code)] // used by unit tests below
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
    _local_preview_scale: f32,
    show_local_player: bool,
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
    if show_local_player && let Some(position) = local_pose {
        let center = crate::skeleton_debug::preview_local_player_center(
            position,
            crate::skeleton_debug::CHARACTER_VISUAL_SCALE_1,
        );
        let size = crate::skeleton_debug::preview_local_player_size(
            crate::skeleton_debug::CHARACTER_VISUAL_SCALE_1,
        );
        quads.push(DrawQuad::rect(center, size, PLAYER_COLOR));
    }
    for pose in remote_poses {
        // NPC poses are drawn via npc_quads; remotes here are remote players only.
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

/// The existing optional equipment domain is the wire-visible humanoid facet.
/// `Some(empty)` is intentionally distinct from no equipment domain.
fn is_humanoid_social_npc(entity: &ReplicatedEntity) -> bool {
    entity.kind == ReplicatedKind::Npc && entity.equipment.is_some()
}

fn npc_quads(
    replica: &crate::replica::ReplicatedWorld,
    interp: &crate::interp::InterpolationBuffer,
    oneshots: &PresentationOneShotTable,
    sheet: &crate::npc_presentation::SpriteSheet,
    players: &mut HashMap<
        PresentationEntityKey,
        (
            crate::npc_presentation::SpriteAnimationPlayer,
            PresentationActivity,
            bool,
        ),
    >,
    frame_dt: f32,
) -> Vec<DrawQuad> {
    let poses = interp.poses();
    let server_tick = replica.last_server_tick();
    let mut visible = std::collections::HashSet::new();
    let quads = replica
        .iter()
        .filter(|entity| entity.kind == ReplicatedKind::Npc && !is_humanoid_social_npc(entity))
        .flat_map(|entity| {
            let position = poses
                .iter()
                .find(|p| p.entity_id == entity.entity_id)
                .map(|p| p.position)
                .unwrap_or(entity.position);
            let aabb = Aabb::new(position, [0.35, 0.55]);
            let key =
                PresentationEntityKey::new(entity.entity_id.index, entity.entity_id.generation);
            let dead = entity.health.is_some_and(|health| health.current <= 0.0);
            let hurt = !dead
                && oneshots.activity_of(key, server_tick)
                    == Some(crate::character_presentation::PresentationActivity::Hurt);
            let respawning = !dead
                && replica.recent_lifecycle().any(|note| {
                    note.entity_id == entity.entity_id
                        && note.kind == ReplicatedKind::Npc
                        && note.event == crate::replica::ReplicaLifecycleEvent::Entered
                        && server_tick.saturating_sub(note.tick) <= 12
                });
            let cue = npc_visual_cue(dead, hurt, respawning);
            let key =
                PresentationEntityKey::new(entity.entity_id.index, entity.entity_id.generation);
            visible.insert(key);
            if cue == NpcVisualCue::Dead {
                let mut quads = vec![aabb_quad(aabb, NPC_DEAD_COLOR)];
                quads.push(DrawQuad::rect(
                    [position[0], position[1] + 0.48],
                    [0.55, 0.12],
                    NPC_DEAD_COLOR,
                ));
                return quads;
            }
            let one_shot = oneshots.activity_of(key, server_tick);
            let requested = base_activity(entity.velocity[0], one_shot);
            let state = players.entry(key).or_insert_with(|| {
                (
                    SpriteAnimationPlayer::new(),
                    if requested == PresentationActivity::Hurt {
                        PresentationActivity::Idle
                    } else {
                        requested
                    },
                    entity.velocity[0] < -0.01,
                )
            });
            let flash = requested == PresentationActivity::Hurt;
            let underlying = if flash { state.1 } else { requested };
            state.0.set_clip(clip_name(underlying));
            if state.1 != underlying {
                state.1 = underlying;
            }
            if entity.velocity[0] > 0.01 {
                state.2 = false;
            } else if entity.velocity[0] < -0.01 {
                state.2 = true;
            }
            let clip = sheet.clip(clip_name(state.1));
            state.0.advance(clip, frame_dt);
            let frame = state.0.frame(clip).unwrap_or(8);
            let mut quads = vec![sheet.quad(position, frame, flash, state.2)];
            if cue == NpcVisualCue::Hurt {
                quads.push(DrawQuad::rect(
                    [position[0], position[1] + 0.48],
                    [0.55, 0.12],
                    NPC_STATE_INDICATOR_COLOR,
                ));
            } else if cue == NpcVisualCue::Dead {
                quads.push(DrawQuad::rect(
                    [position[0], position[1] + 0.48],
                    [0.55, 0.12],
                    NPC_DEAD_COLOR,
                ));
            } else if cue == NpcVisualCue::Respawn {
                quads.push(DrawQuad::triangle(
                    [position[0], position[1] + 0.52],
                    [0.5, 0.28],
                    NPC_RESPAWN_COLOR,
                ));
            }
            quads
        })
        .collect::<Vec<_>>();
    players.retain(|key, _| visible.contains(key));
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

fn item_drop_quads(replica: &ReplicatedWorld) -> Vec<DrawQuad> {
    replica
        .iter()
        .filter(|entity| entity.kind == ReplicatedKind::Item)
        .map(|entity| {
            DrawQuad::rect(
                [entity.position[0], entity.position[1] + 0.22],
                [0.42, 0.42],
                [0.95, 0.78, 0.18, 1.0],
            )
        })
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

#[cfg(feature = "dev-diagnostics")]
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
    nearest_interaction: Option<(purgatory_protocol::WireEntityId, f32)>,
    nearest_item: Option<(purgatory_protocol::WireEntityId, f32)>,
    nearest_portal: Option<(purgatory_protocol::WireEntityId, f32)>,
    #[cfg_attr(not(feature = "dev-diagnostics"), allow(dead_code))]
    nearest_portal_pos: Option<[f32; 2]>,
    portal_eligible: bool,
    activate_portal: Option<purgatory_protocol::WireEntityId>,
}

fn replica_interact_routing(replica: &ReplicatedWorld) -> ReplicaInteractRouting {
    let nearest_portal = advisory_nearest_portal(replica);
    let activate_portal = advisory_centered_portal(replica);
    ReplicaInteractRouting {
        nearest_interaction: advisory_nearest_interaction_target(replica),
        nearest_item: advisory_nearest_item(replica),
        nearest_portal: nearest_portal.map(|(id, dist, _)| (id, dist)),
        nearest_portal_pos: nearest_portal.map(|(_, _, pos)| pos),
        portal_eligible: activate_portal.is_some(),
        activate_portal,
    }
}

fn log_interact_routing(prefix: &str, routing: &ReplicaInteractRouting) {
    if let Some((id, distance)) = routing.nearest_item {
        println!("{prefix} nearest item={id} distance={distance:.3}");
    }
    match routing.nearest_interaction {
        Some((id, distance)) => {
            println!("{prefix} nearest interaction target={id} distance={distance:.3}");
        }
        None => println!("{prefix} nearest interaction target=none distance=n/a"),
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

/// E send path: nearest generic interactable or Social NPC inside [`INTERACT_RANGE`].
/// Portals are never candidates.
fn interact_open_send_target(
    routing: &ReplicaInteractRouting,
) -> Option<purgatory_protocol::WireEntityId> {
    routing
        .nearest_interaction
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
    nearest_matching(replica, |entity| entity.kind == kind)
}

fn nearest_matching(
    replica: &ReplicatedWorld,
    predicate: impl Fn(&ReplicatedEntity) -> bool,
) -> Option<(purgatory_protocol::WireEntityId, f32, [f32; 2])> {
    let local = replica.local_entity()?;
    replica
        .iter()
        .filter(|entity| predicate(entity) && entity.entity_id != local.entity_id)
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

/// Advisory nearest interaction target. Portals and combat NPCs are excluded.
fn advisory_nearest_interaction_target(
    replica: &ReplicatedWorld,
) -> Option<(purgatory_protocol::WireEntityId, f32)> {
    nearest_matching(replica, |entity| {
        entity.kind == ReplicatedKind::Interactable || is_humanoid_social_npc(entity)
    })
    .map(|(id, dist, _)| (id, dist))
}

fn advisory_nearest_item(
    replica: &ReplicatedWorld,
) -> Option<(purgatory_protocol::WireEntityId, f32)> {
    nearest_of_kind(replica, ReplicatedKind::Item).map(|(id, dist, _)| (id, dist))
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
#[cfg(feature = "dev-diagnostics")]
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
#[cfg(feature = "dev-diagnostics")]
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

        match Renderer::new(window.clone(), self.asset_runtime.resources()) {
            Ok(mut renderer) => {
                renderer.set_render_scale(self.display.settings().render_scale);
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
                #[cfg(feature = "dev-diagnostics")]
                {
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
                        &self.registry,
                    );
                    let frontend = ConnectionFrontend::load(overlay.context());
                    self.debug = Some(overlay);
                    self.frontend = Some(frontend);
                }
                #[cfg(not(feature = "dev-diagnostics"))]
                {
                    println!(
                        "PURGATORY shipping client: no Connection Frontend / Debug overlay; will auto-connect once."
                    );
                    println!(
                        "PURGATORY controls (in Game): A/Left=MoveLeft D/Right=MoveRight S/Down=Down Space=Jump | Down+Jump=drop through OneWay | camera follows player"
                    );
                }
                let surface = renderer.surface_size();
                self.renderer = Some(renderer);
                self.window = Some(window);
                self.display.note_surface_configured(surface);
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
            && {
                #[cfg(feature = "dev-diagnostics")]
                {
                    is_debug_toggle(key)
                }
                #[cfg(not(feature = "dev-diagnostics"))]
                {
                    let _ = key;
                    false
                }
            }
        {
            #[cfg(feature = "dev-diagnostics")]
            {
                self.toggle_debug_overlay();
                window.request_redraw();
            }
            return;
        }

        #[cfg(feature = "dev-diagnostics")]
        {
            let overlay_open = self.debug_overlay_visible();
            let on_connection = self.lifecycle.screen() == ClientScreen::Connection;
            let gameplay_hud_visible = self
                .replica
                .local_entity()
                .and_then(|entity| entity.health)
                .is_some();
            let feed_overlay = overlay_open
                || on_connection
                || gameplay_hud_visible
                || matches!(
                    event,
                    WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }
                );
            if feed_overlay && let Some(overlay) = &mut self.debug {
                overlay.on_window_event(&window, &event);
            }
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
                self.apply_framebuffer_size(size.width, size.height);
                window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let size = window.inner_size();
                self.apply_framebuffer_size(size.width, size.height);
                window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                #[cfg(feature = "dev-diagnostics")]
                let text_like = self
                    .debug
                    .as_ref()
                    .is_some_and(DebugOverlay::wants_keyboard_for_text);
                #[cfg(feature = "dev-diagnostics")]
                let overlay_open = self.debug_overlay_visible();
                #[cfg(feature = "dev-diagnostics")]
                let pressed = event.state == ElementState::Pressed;
                #[cfg(feature = "dev-diagnostics")]
                let receives = gameplay_receives_keyboard(overlay_open, text_like, pressed);
                #[cfg(not(feature = "dev-diagnostics"))]
                let receives = true;
                if self.lifecycle.gameplay_actions_allowed() && receives {
                    let dialogue_navigation = event.state == ElementState::Pressed
                        && !event.repeat
                        && self.dialogue_runtime.choices(&self.registry).is_some()
                        && match event.physical_key {
                            PhysicalKey::Code(KeyCode::ArrowUp) => {
                                self.dialogue_runtime.select_next(&self.registry, -1)
                            }
                            PhysicalKey::Code(KeyCode::ArrowDown) => {
                                self.dialogue_runtime.select_next(&self.registry, 1)
                            }
                            _ => false,
                        };
                    // Latch ActionState only. Intent is emitted on the sim tick
                    // that consumes the same PlayerInput as prediction.
                    if !dialogue_navigation {
                        self.actions.apply_key_event(&event);
                    } else {
                        window.request_redraw();
                    }
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
            WindowEvent::CursorMoved { position, .. } => {
                #[cfg(feature = "dev-diagnostics")]
                let gameplay_mouse = gameplay_receives_pointer(
                    self.debug_overlay_visible(),
                    self.debug.as_ref().is_some_and(DebugOverlay::wants_pointer),
                );
                #[cfg(not(feature = "dev-diagnostics"))]
                let gameplay_mouse = true;
                if self.lifecycle.gameplay_actions_allowed() && gameplay_mouse {
                    self.cursor_position = Some([position.x as f32, position.y as f32]);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                #[cfg(feature = "dev-diagnostics")]
                let gameplay_mouse = gameplay_receives_pointer(
                    self.debug_overlay_visible(),
                    self.debug.as_ref().is_some_and(DebugOverlay::wants_pointer),
                );
                #[cfg(not(feature = "dev-diagnostics"))]
                let gameplay_mouse = true;
                if self.lifecycle.gameplay_actions_allowed()
                    && gameplay_mouse
                    && state == ElementState::Pressed
                    && button == MouseButton::Left
                {
                    if let Some(cursor) = self.cursor_position {
                        if let Some(index) = self
                            .choice_bubble_hits
                            .iter()
                            .position(|bounds| bounds.contains(cursor))
                        {
                            self.choice_click_edge = Some(index);
                            window.request_redraw();
                        } else if self
                            .speech_bubble_hit
                            .is_some_and(|bounds| bounds.contains(cursor))
                        {
                            self.bubble_click_edge = true;
                            window.request_redraw();
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { .. } => {}
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
    fn npc_visual_cues_prioritize_lifecycle_states() {
        assert_eq!(
            super::npc_visual_cue(false, false, false),
            super::NpcVisualCue::Normal
        );
        assert_eq!(
            super::npc_visual_cue(false, true, false),
            super::NpcVisualCue::Hurt
        );
        assert_eq!(
            super::npc_visual_cue(false, false, true),
            super::NpcVisualCue::Respawn
        );
        assert_eq!(
            super::npc_visual_cue(true, true, true),
            super::NpcVisualCue::Dead
        );
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
    fn development_default_window_uses_locked_16_by_9_view() {
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
        let quads = super::scene_quads(&world, local_pose, &[], 1.0, true);
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
    fn scene_quads_local_aabb_stays_simulation_size() {
        use purgatory_simulation::PLAYER_HALF_EXTENTS;

        let world = World::dev_stage();
        let local_pose = [1.5, 2.5];
        let shown_1 = super::scene_quads(&world, Some(local_pose), &[], 1.0, true);
        let shown_110 = super::scene_quads(
            &world,
            Some(local_pose),
            &[],
            crate::skeleton_debug::CHARACTER_VISUAL_SCALE_115,
            true,
        );
        let expect_size = [PLAYER_HALF_EXTENTS[0] * 2.0, PLAYER_HALF_EXTENTS[1] * 2.0];
        let q1 = shown_1
            .iter()
            .find(|quad| quad.center == local_pose)
            .expect("local player quad");
        let q110 = shown_110
            .iter()
            .find(|quad| quad.center == local_pose)
            .expect("local player quad at 1.15 argument");
        assert_eq!(q1.size, expect_size);
        assert_eq!(q110.size, expect_size);
        assert_eq!(q1.center, q110.center);
    }

    #[test]
    fn scene_quads_omit_local_when_pose_absent() {
        let world = World::dev_stage();
        let quads = super::scene_quads(&world, None, &[], 1.0, true);
        let world_player = world.player_body().expect("local world player");
        assert!(
            quads
                .iter()
                .all(|quad| quad.center != world_player.position),
            "no local pose means no local player quad"
        );
    }

    #[test]
    fn scene_quads_can_hide_local_player_body_quad() {
        let world = World::dev_stage();
        let local_pose = Some([1.5, 2.5]);
        let shown = super::scene_quads(&world, local_pose, &[], 1.0, true);
        let hidden = super::scene_quads(&world, local_pose, &[], 1.0, false);
        assert!(shown.iter().any(|quad| quad.center == [1.5, 2.5]));
        assert!(hidden.iter().all(|quad| quad.center != [1.5, 2.5]));
        assert_eq!(shown.len(), hidden.len() + 1);
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
            super::advisory_nearest_interaction_target(&replica).map(|(id, _)| id),
            Some(target)
        );
        let dist = super::advisory_nearest_interaction_target(&replica)
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
            routing.nearest_interaction.map(|(id, _)| id),
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

    fn replica_from_records(
        local: purgatory_protocol::WireEntityId,
        records: Vec<purgatory_protocol::ReplicationRecord>,
    ) -> crate::replica::ReplicatedWorld {
        let mut replica = crate::replica::ReplicatedWorld::new();
        let decision = replica.apply_frame(purgatory_protocol::ReplicationFrame {
            snapshot_sequence: 1,
            server_tick: 1,
            local_player_entity: local,
            input_epoch: 0,
            last_acknowledged_input_sequence: 0,
            local_grounded: false,
            local_grounded_on: purgatory_protocol::PlatformSupportId::NONE,
            local_ignored_platform: purgatory_protocol::PlatformSupportId::NONE,
            continuation_debt: 0,
            local_map: 1,
            local_channel: 0,
            local_instance: 0,
            observer_baseline_epoch: 0,
            records,
            aoi_debug: None,
        });
        assert!(matches!(
            decision,
            crate::replica::FrameDecision::Applied { epoch_reset: false }
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
    fn social_npc_is_humanoid_e_target_without_magenta_quad() {
        use purgatory_protocol::{
            ReplicatedEquipment, ReplicatedKind, ReplicationRecord, WireEntityId,
        };

        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let combat_npc = WireEntityId {
            index: 8,
            generation: 1,
        };
        let traveler = WireEntityId {
            index: 9,
            generation: 1,
        };
        let replica = replica_from_records(
            local,
            vec![
                ReplicationRecord::Enter {
                    entity: pose(local, ReplicatedKind::Player, [-19.4, -2.9]),
                    health: None,
                    equipment: None,
                },
                ReplicationRecord::Enter {
                    entity: pose(combat_npc, ReplicatedKind::Npc, [-19.0, -2.9]),
                    health: None,
                    equipment: None,
                },
                ReplicationRecord::Enter {
                    entity: pose(traveler, ReplicatedKind::Npc, [-17.8, -2.9]),
                    health: None,
                    equipment: Some(ReplicatedEquipment::empty()),
                },
            ],
        );

        assert!(super::is_humanoid_social_npc(
            &replica.get(traveler).expect("Traveler replica")
        ));
        assert!(!super::is_humanoid_social_npc(
            &replica.get(combat_npc).expect("combat NPC replica")
        ));
        assert_eq!(
            super::interact_open_send_target(&super::replica_interact_routing(&replica)),
            Some(traveler),
            "a closer combat NPC must not hide the Social NPC E target"
        );
        assert!(
            super::interactable_quads(&replica).is_empty(),
            "ReplicatedKind::Npc must not use the magenta generic-interactable draw path"
        );
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
        assert_eq!(routing.nearest_interaction.map(|(id, _)| id), Some(chest));
        let generic_dist = routing.nearest_interaction.unwrap().1;
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
        assert!(super::advisory_nearest_interaction_target(&replica).is_none());
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
        assert!(super::advisory_nearest_interaction_target(&replica).is_none());
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

    #[cfg(feature = "dev-diagnostics")]
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
        let with_remotes = super::scene_quads(&world, Some([-8.0, -3.0]), &remotes, 1.0, true);
        let without = super::scene_quads(&world, Some([-8.0, -3.0]), &[], 1.0, true);
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
