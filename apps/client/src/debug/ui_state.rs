//! Client-owned development debug UI state (not simulation authority).

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
        }
    }
}

impl DebugUiState {
    pub const TIME_SCALES: [f32; 3] = [1.0, 0.5, 0.25];

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
