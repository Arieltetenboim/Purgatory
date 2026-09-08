//! Client-owned development debug UI state (not simulation authority).

use super::sections::DebugSectionMap;
use crate::network::ImpairmentProfile;
use purgatory_common::ItemInstanceId;

/// Skeleton Proof animation source. Default remains the A1 manual slider.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AnimationProofMode {
    #[default]
    ManualA1,
    PlaybackA2,
}

impl AnimationProofMode {
    pub const ALL: [Self; 2] = [Self::ManualA1, Self::PlaybackA2];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ManualA1 => "Manual A1",
            Self::PlaybackA2 => "Playback A2",
        }
    }
}

/// Presentation / harness toggles controlled by the debug overlay.
#[derive(Clone, Debug)]
pub struct DebugUiState {
    pub time_scale: f32,
    pub camera_follow: bool,
    pub center_on_player: bool,
    /// Debug UI → [`crate::display::DisplayController::set_resolution`].
    pub requested_resolution: Option<crate::display::Resolution>,
    /// Debug UI → [`crate::display::DisplayController::set_render_scale`].
    pub requested_render_scale: Option<crate::display::RenderScale>,
    /// Master switch for overlay-open world gizmos (entity outlines, colliders, AOI, …).
    /// Independent of skeleton debug draw.
    pub show_overlay_gizmos: bool,
    pub show_colliders: bool,
    pub show_velocity: bool,
    pub show_grounded_highlight: bool,
    pub show_world_bounds: bool,
    pub show_grid: bool,
    pub show_parallax_debug: bool,
    /// When true, record player/camera discontinuity events into history.
    pub position_discontinuity_detector: bool,
    /// When true (and detector on), print discontinuity events to stderr.
    pub log_discontinuities: bool,
    /// When true, enable verbose collision candidate logging (dev-only).
    pub verbose_collision_trace: bool,
    pub network_connect: bool,
    pub network_disconnect: bool,
    /// DEV overlay Channel request. Sent as server-authoritative DevSetChannel.
    pub request_channel: Option<u32>,
    /// DEV A5 presentation Attack oneshot request.
    pub request_presentation_attack: bool,
    /// DEV A5 presentation Hurt oneshot request.
    pub request_presentation_hurt: bool,
    pub last_observed_channel: Option<u32>,
    pub last_observed_epoch: Option<u32>,
    pub log_network_lifecycle: bool,
    pub verbose_network_trace: bool,
    pub clear_network_history: bool,
    /// Show authoritative remote positions vs interpolated remotes (default OFF).
    pub show_interpolation_gizmos: bool,
    /// Show authoritative vs predicted local markers (default OFF).
    pub show_prediction_gizmos: bool,
    /// Presentation-only copy of server view-envelope enter/leave rects.
    pub show_aoi_rects: bool,
    /// Camera Dead Zone / center gizmo (overlay only).
    pub show_camera_deadzone: bool,
    /// Draw Stage D skeleton joints/connections (independent of overlay visibility).
    pub show_skeleton: bool,
    /// Draw P4 placeholder body pieces. Independent of skeleton lines and simulation AABB.
    pub show_placeholder_character: bool,
    /// Overlay 8F-B proof: force [`crate::character_presentation::PresentationView::Back`]
    /// at draw time for every CharacterPresentationSet entry on this client
    /// (local predicted + remote interpolated). Does not change activity.
    /// Not replicated to other clients.
    pub presentation_view_back: bool,
    /// Overlay 8F-C proof: inject ClimbBack into every player entry this client
    /// presents (local + remotes). Not a World/network climb. Not replicated.
    pub presentation_force_climb_back: bool,
    /// Legacy local-player cyan AABB. Independent of simulation and skeleton draw.
    pub show_local_player_quad: bool,
    /// Selected Humanoid v0 bone index for overlay local/world/screen readout (0..=15).
    pub skeleton_inspect_index: u8,
    /// Front-leg proof pose. Default Off. Not an animation system.
    pub skeleton_front_leg_proof: crate::skeleton_debug::FrontLegProof,
    /// Front-arm proof pose. Default Off. Not an animation system.
    pub skeleton_front_arm_proof: crate::skeleton_debug::FrontArmProof,
    /// A1 animation sample time (explicit `t` over the hard-coded head clip). No auto-play.
    pub skeleton_a1_sample_t: f32,
    /// Manual A1 slider vs A2 AnimationPlayer loop proof. Default ManualA1.
    pub animation_proof_mode: AnimationProofMode,
    /// A2 playback speed (player applies `dt × speed` in `advance`).
    pub skeleton_a2_speed: f32,
    /// Overlay → App: set AnimationPlayer playing.
    pub request_animation_play: bool,
    /// Overlay → App: pause AnimationPlayer.
    pub request_animation_pause: bool,
    /// Overlay → App: reset AnimationPlayer elapsed.
    pub request_animation_reset: bool,
    /// Last resolved sample `t` for the frame (App writes; Proof UI reads).
    pub selected_animation_sample_t: f32,
    /// Last known playing flag mirrored from App for Proof UI.
    pub animation_player_playing: bool,
    /// Diagnostic 2× presentation preview about planted feet. Off = 1.15× baseline.
    /// Does not write bind / Local / World pose / AABB.
    pub skeleton_debug_preview_2x: bool,
    /// RF0 isolated rotated/translating quads (existing world pass).
    pub show_rf_scene: bool,
    /// RF1.5/RF2 simultaneous compositor (does not use gameplay Render Scale).
    pub show_rf_ab: bool,
    /// Freeze follow camera while RF0 is inspected. Same path as Camera Frozen.
    pub rf_freeze_camera: bool,
    /// Print the RF0 probe's screen-space vertices each frame.
    pub rf_log_vertices: bool,
    /// Debug UI → [`crate::renderer::Renderer::set_world_msaa`].
    pub requested_world_msaa: Option<crate::renderer::WorldMsaa>,
    /// DEV isolation for camera/presentation jitter forensics.
    pub camera_jitter_mode: crate::jitter_forensics::CameraJitterMode,
    pub dump_jitter_trace: bool,
    pub last_jitter_dump: String,
    /// World-space semantic labels for replica entities (overlay only).
    pub show_entity_labels: bool,
    pub last_interact_kind: Option<crate::ui_runtime::InteractKind>,
    pub interact_flash_text: String,
    pub interact_flash_until: Option<std::time::Instant>,
    pub impairment_profile: ImpairmentProfile,
    pub impairment_stall_ms: Option<u32>,
    pub reset_impairment_metrics: bool,
    /// DEV overlay: equip authored equipment id (`equipment.debug.*`).
    pub request_debug_equip: Option<&'static str>,
    pub request_debug_equip_item: Option<ItemInstanceId>,
    pub selected_inventory_item: Option<ItemInstanceId>,
    /// DEV overlay: unequip slot index `0..=5`.
    pub request_debug_unequip_slot: Option<u8>,
    /// DEV overlay: unequip every slot.
    pub request_debug_unequip_all: bool,
    /// Player tab: local FOOTNOTE max ground/air speed (wu/s).
    pub debug_move_speed: f32,
    pub(crate) debug_move_speed_reset_requested: bool,
    pub(crate) last_sent_debug_move_speed: Option<f32>,
    /// Player tab: local FOOTNOTE jump speed (wu/s).
    pub debug_jump_speed: f32,
    pub(crate) debug_jump_speed_reset_requested: bool,
    pub(crate) last_sent_debug_jump_speed: Option<f32>,
    /// Shared cadence gate for continuous DEV tuning controls.
    pub(crate) debug_tuning_next_send_at: Option<std::time::Instant>,
    /// Round-robin tie-breaker when both continuous controls are pending.
    pub(crate) debug_tuning_send_speed_next: bool,
    /// Player tab: Headwear Side atlas cell `0..=3` (HEADWEAR 1–4).
    pub headwear_side_cell: u8,
    pub sections: DebugSectionMap,
    /// Center-screen toast for Reset / Reanchor / Channel. Not drawn in the Debug window.
    pub dev_action_flash_text: String,
    pub dev_action_flash_until: Option<std::time::Instant>,
}

impl Default for DebugUiState {
    fn default() -> Self {
        Self {
            time_scale: 1.0,
            camera_follow: true,
            center_on_player: false,
            requested_resolution: None,
            requested_render_scale: None,
            show_overlay_gizmos: false,
            show_colliders: false,
            show_velocity: true,
            show_grounded_highlight: true,
            show_world_bounds: false,
            show_grid: false,
            show_parallax_debug: false,
            position_discontinuity_detector: false,
            log_discontinuities: false,
            verbose_collision_trace: false,
            network_connect: false,
            network_disconnect: false,
            request_channel: None,
            request_presentation_attack: false,
            request_presentation_hurt: false,
            last_observed_channel: None,
            last_observed_epoch: None,
            log_network_lifecycle: false,
            verbose_network_trace: false,
            clear_network_history: false,
            show_interpolation_gizmos: false,
            show_prediction_gizmos: false,
            show_aoi_rects: true,
            show_camera_deadzone: false,
            show_skeleton: false,
            show_placeholder_character: true,
            presentation_view_back: false,
            presentation_force_climb_back: false,
            show_local_player_quad: true,
            skeleton_inspect_index: 0,
            skeleton_front_leg_proof: crate::skeleton_debug::FrontLegProof::Off,
            skeleton_front_arm_proof: crate::skeleton_debug::FrontArmProof::Off,
            skeleton_a1_sample_t: 0.0,
            animation_proof_mode: AnimationProofMode::ManualA1,
            skeleton_a2_speed: 1.0,
            request_animation_play: false,
            request_animation_pause: false,
            request_animation_reset: false,
            selected_animation_sample_t: 0.0,
            animation_player_playing: false,
            skeleton_debug_preview_2x: false,
            show_rf_scene: false,
            show_rf_ab: false,
            rf_freeze_camera: false,
            rf_log_vertices: false,
            requested_world_msaa: None,
            camera_jitter_mode: crate::jitter_forensics::CameraJitterMode::Normal,
            dump_jitter_trace: false,
            last_jitter_dump: String::new(),
            show_entity_labels: false,
            last_interact_kind: None,
            interact_flash_text: String::new(),
            interact_flash_until: None,
            impairment_profile: ImpairmentProfile::Off,
            impairment_stall_ms: None,
            reset_impairment_metrics: false,
            request_debug_equip: None,
            request_debug_equip_item: None,
            selected_inventory_item: None,
            request_debug_unequip_slot: None,
            request_debug_unequip_all: false,
            debug_move_speed: DEBUG_MOVE_SPEED_DEFAULT,
            debug_move_speed_reset_requested: false,
            last_sent_debug_move_speed: None,
            debug_jump_speed: DEBUG_JUMP_SPEED_DEFAULT,
            debug_jump_speed_reset_requested: false,
            last_sent_debug_jump_speed: None,
            debug_tuning_next_send_at: None,
            debug_tuning_send_speed_next: true,
            headwear_side_cell: 0,
            sections: DebugSectionMap::default(),
            dev_action_flash_text: String::new(),
            dev_action_flash_until: None,
        }
    }
}

impl DebugUiState {
    pub const TIME_SCALES: [f32; 3] = [1.0, 0.5, 0.25];

    pub fn request_debug_move_speed_reset(&mut self) {
        self.debug_move_speed = DEBUG_MOVE_SPEED_DEFAULT;
        self.debug_move_speed_reset_requested = true;
    }

    pub fn request_debug_jump_speed_reset(&mut self) {
        self.debug_jump_speed = DEBUG_JUMP_SPEED_DEFAULT;
        self.debug_jump_speed_reset_requested = true;
    }

    /// Overlay defaults, then honor launcher env (`PURGATORY_NET_LOG` / `PURGATORY_NET_VERBOSE`).
    #[must_use]
    pub fn from_env() -> Self {
        let mut ui = Self::default();
        let log = std::env::var_os("PURGATORY_NET_LOG").is_some();
        let verbose = std::env::var_os("PURGATORY_NET_VERBOSE").is_some();
        if log || verbose {
            ui.log_network_lifecycle = true;
        }
        if verbose {
            ui.verbose_network_trace = true;
        }
        ui.impairment_profile = crate::network::NetworkImpairmentConfig::from_env().profile;
        ui
    }

    pub fn note_dev_action_flash(&mut self, text: &str) {
        self.dev_action_flash_text = text.to_string();
        self.dev_action_flash_until =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
    }

    #[must_use]
    pub fn center_toast_live(&self) -> bool {
        !self.dev_action_flash_text.is_empty()
            && self
                .dev_action_flash_until
                .is_some_and(|until| std::time::Instant::now() < until)
    }

    #[must_use]
    pub fn time_scale_label(&self) -> &'static str {
        if (self.time_scale - 1.0).abs() < 1e-4 {
            "1.0x"
        } else if (self.time_scale - 0.5).abs() < 1e-4 {
            "0.5x"
        } else if (self.time_scale - 0.25).abs() < 1e-4 {
            "0.25x"
        } else {
            "custom"
        }
    }
}

/// Rising-edge on a world-gizmo child turns the master gate on so the Debug
/// tab checkbox is not a silent no-op. Master-off still mutes GPU gizmos.
pub fn arm_master_on_gizmo_enable(master: &mut bool, was_enabled: bool, now_enabled: bool) {
    if now_enabled && !was_enabled {
        *master = true;
    }
}

/// Replica-present action re-anchors prediction. No replica → local spawn reset.
#[must_use]
pub fn reset_player_uses_replica(replica_has_local: bool) -> bool {
    replica_has_local
}

#[must_use]
pub fn reset_action_label(replica_has_local: bool) -> &'static str {
    if replica_has_local {
        "Reanchor Prediction"
    } else {
        "Reset to Spawn Point"
    }
}

pub const RESET_TO_SPAWN_LABEL: &str = "Reset to Spawn Point";
pub const RESET_TO_SPAWN_HELP: &str = "Connected: server-authoritative spawn reset (DevResetPlayer). Offline: local World spawn and camera center.";
pub const RESET_TO_SPAWN_FLASH: &str = "RESET TO SPAWN POINT";

pub const DEBUG_MOVE_SPEED_DEFAULT: f32 = 4.0;
pub const DEBUG_MOVE_SPEED_MIN: f32 = 0.5;
pub const DEBUG_MOVE_SPEED_MAX: f32 = 24.0;
pub const DEBUG_JUMP_SPEED_DEFAULT: f32 = 13.0;
pub const DEBUG_JUMP_SPEED_MIN: f32 = 1.0;
pub const DEBUG_JUMP_SPEED_MAX: f32 = 30.0;
pub const DEBUG_TUNING_SEND_INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);

#[must_use]
pub fn sanitize_debug_move_speed(speed: f32) -> f32 {
    if !speed.is_finite() {
        return DEBUG_MOVE_SPEED_DEFAULT;
    }
    speed.clamp(DEBUG_MOVE_SPEED_MIN, DEBUG_MOVE_SPEED_MAX)
}

#[must_use]
pub fn sanitize_debug_jump_speed(speed: f32) -> f32 {
    if !speed.is_finite() {
        return DEBUG_JUMP_SPEED_DEFAULT;
    }
    speed.clamp(DEBUG_JUMP_SPEED_MIN, DEBUG_JUMP_SPEED_MAX)
}

#[must_use]
pub fn reset_action_help(replica_has_local: bool) -> &'static str {
    if replica_has_local {
        "Snaps local prediction to the authoritative replica. Not a server teleport. Pose may not move if already aligned."
    } else {
        RESET_TO_SPAWN_HELP
    }
}

#[must_use]
pub fn reset_action_flash(replica_has_local: bool) -> &'static str {
    if replica_has_local {
        "REANCHOR"
    } else {
        RESET_TO_SPAWN_FLASH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabling_a_world_gizmo_arms_the_master_gate() {
        let mut master = false;
        arm_master_on_gizmo_enable(&mut master, false, true);
        assert!(master);
        arm_master_on_gizmo_enable(&mut master, true, false);
        assert!(master, "unchecking a child must not mute remaining gizmos");
    }

    #[test]
    fn reset_path_follows_replica_presence_not_prediction_active() {
        assert!(reset_player_uses_replica(true));
        assert!(!reset_player_uses_replica(false));
        assert_eq!(reset_action_label(true), "Reanchor Prediction");
        assert_eq!(reset_action_label(false), RESET_TO_SPAWN_LABEL);
        assert!(reset_action_help(true).contains("authoritative replica"));
        assert!(reset_action_help(false).contains("spawn"));
        assert_eq!(reset_action_flash(true), "REANCHOR");
        assert_eq!(reset_action_flash(false), RESET_TO_SPAWN_FLASH);
    }

    #[test]
    fn dev_action_flash_is_observable() {
        let mut ui = DebugUiState::default();
        assert!(!ui.center_toast_live());
        ui.note_dev_action_flash("REANCHOR");
        assert_eq!(ui.dev_action_flash_text, "REANCHOR");
        assert!(ui.center_toast_live());
        assert!(
            ui.dev_action_flash_until
                .is_some_and(|until| until > std::time::Instant::now())
        );
    }

    #[test]
    fn toast_strings_are_ascii() {
        assert!(
            RESET_TO_SPAWN_FLASH.bytes().all(|b| b.is_ascii()),
            "egui default font has no Unicode arrows"
        );
        assert!(reset_action_flash(true).bytes().all(|b| b.is_ascii()));
        assert!(reset_action_flash(false).bytes().all(|b| b.is_ascii()));
        let channel = format!("CHANNEL {} -> {}", 0, 1);
        assert!(channel.bytes().all(|b| b.is_ascii()), "{channel}");
    }

    #[test]
    fn debug_move_speed_sanitizes_and_defaults_match_footnote() {
        assert_eq!(
            sanitize_debug_move_speed(DEBUG_MOVE_SPEED_DEFAULT),
            DEBUG_MOVE_SPEED_DEFAULT
        );
        assert_eq!(sanitize_debug_move_speed(0.0), DEBUG_MOVE_SPEED_MIN);
        assert_eq!(sanitize_debug_move_speed(100.0), DEBUG_MOVE_SPEED_MAX);
        assert_eq!(
            sanitize_debug_move_speed(f32::NAN),
            DEBUG_MOVE_SPEED_DEFAULT
        );
        let ui = DebugUiState::default();
        assert!((ui.debug_move_speed - DEBUG_MOVE_SPEED_DEFAULT).abs() < f32::EPSILON);
        assert_eq!(ui.headwear_side_cell, 0);
        assert!(
            (DEBUG_MOVE_SPEED_DEFAULT
                - purgatory_simulation::FootnoteConfig::DEFAULT.max_ground_speed)
                .abs()
                < f32::EPSILON
        );
    }
}
