//! Client-owned development debug UI state (not simulation authority).

use super::sections::DebugSectionMap;
use crate::network::ImpairmentProfile;

/// Presentation / harness toggles controlled by the debug overlay.
#[derive(Clone, Debug)]
pub struct DebugUiState {
    pub time_scale: f32,
    pub camera_follow: bool,
    pub center_on_player: bool,
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
    pub last_observed_channel: Option<u32>,
    pub last_observed_epoch: Option<u32>,
    pub channel_flash_from: Option<u32>,
    pub channel_flash_to: Option<u32>,
    pub channel_flash_epoch_from: Option<u32>,
    pub channel_flash_epoch_to: Option<u32>,
    pub channel_flash_until: Option<std::time::Instant>,
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
    pub sections: DebugSectionMap,
}

impl Default for DebugUiState {
    fn default() -> Self {
        Self {
            time_scale: 1.0,
            camera_follow: true,
            center_on_player: false,
            show_colliders: true,
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
            last_observed_channel: None,
            last_observed_epoch: None,
            channel_flash_from: None,
            channel_flash_to: None,
            channel_flash_epoch_from: None,
            channel_flash_epoch_to: None,
            channel_flash_until: None,
            log_network_lifecycle: false,
            verbose_network_trace: false,
            clear_network_history: false,
            show_interpolation_gizmos: false,
            show_prediction_gizmos: false,
            show_aoi_rects: true,
            show_camera_deadzone: true,
            camera_jitter_mode: crate::jitter_forensics::CameraJitterMode::Normal,
            dump_jitter_trace: false,
            last_jitter_dump: String::new(),
            show_entity_labels: true,
            last_interact_kind: None,
            interact_flash_text: String::new(),
            interact_flash_until: None,
            impairment_profile: ImpairmentProfile::Off,
            impairment_stall_ms: None,
            reset_impairment_metrics: false,
            sections: DebugSectionMap::default(),
        }
    }
}

impl DebugUiState {
    pub const TIME_SCALES: [f32; 3] = [1.0, 0.5, 0.25];

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
