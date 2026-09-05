//! Client-local display configuration.
//!
//! Window size, framebuffer/render size, and UI scale are presentation-only.
//! They must not enter simulation, World, protocol, or the server.
//!
//! Window / output size, internal world render scale, and UI scale are
//! independent. Persistence of these settings is deferred (no local settings
//! file yet). Dynamic resolution / DLSS / FSR are not implemented.

use winit::dpi::{LogicalSize, PhysicalSize};
use winit::window::Window;

/// Physical pixel width × height. Zero on either axis is invalid.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    /// Development default, matching the historical hard-coded client window.
    pub const DEFAULT: Self = Self {
        width: 1280,
        height: 720,
    };

    #[must_use]
    pub const fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            None
        } else {
            Some(Self { width, height })
        }
    }

    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.width > 0 && self.height > 0
    }

    /// `width / height`. `None` when either axis is zero.
    #[must_use]
    pub fn aspect_ratio(self) -> Option<f32> {
        if !self.is_valid() {
            return None;
        }
        Some(self.width as f32 / self.height as f32)
    }

    #[must_use]
    pub fn label(self) -> String {
        format!("{}×{}", self.width, self.height)
    }
}

/// Native window presentation mode.
///
/// Borderless / exclusive fullscreen are deferred. Do not invent a fake
/// fullscreen path; add a variant here when a real apply path exists.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WindowMode {
    #[default]
    Windowed,
}

impl WindowMode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Windowed => "Windowed",
        }
    }
}

/// Multiplier on the OS / winit scale factor. Independent of resolution.
///
/// `1.0` means "use the OS display scale only". Changing framebuffer size
/// must not change this value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiScale(f32);

impl UiScale {
    pub const DEFAULT: Self = Self(1.0);
    pub const MIN: f32 = 0.5;
    pub const MAX: f32 = 2.0;

    #[must_use]
    pub fn new(value: f32) -> Self {
        let v = if value.is_finite() { value } else { 1.0 };
        Self(v.clamp(Self::MIN, Self::MAX))
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Default for UiScale {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// World-pass internal resolution as a percent of the gameplay pixel rect.
///
/// Independent of window size and of camera FOV. 100% matches the gameplay
/// rect; below 100% renders fewer pixels; above 100% supersamples then
/// downsamples on blit. Production default is 200% (integer 2:1) with 4×
/// MSAA and linear downsample. 100% is the performance fallback.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RenderScale {
    percent: u16,
}

impl RenderScale {
    pub const P50: Self = Self { percent: 50 };
    pub const P75: Self = Self { percent: 75 };
    pub const P100: Self = Self { percent: 100 };
    pub const P125: Self = Self { percent: 125 };
    pub const P200: Self = Self { percent: 200 };
    /// Production world default (RF3 close). Fidelity only; not FOV.
    pub const DEFAULT: Self = Self::P200;
    /// 1:1 internal pixels. Still uses world-pass 4× MSAA when supported.
    #[cfg_attr(not(feature = "dev-diagnostics"), allow(dead_code))]
    pub const PERFORMANCE_FALLBACK: Self = Self::P100;

    #[must_use]
    pub const fn percent(self) -> u16 {
        self.percent
    }

    #[must_use]
    pub fn label(self) -> String {
        format!("{}%", self.percent)
    }
}

impl Default for RenderScale {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Supported render-scale presets. Debug UI and a future Settings menu share this list.
/// 150% is not a quality-policy preset. 400% is RF diagnostic only.
pub const RENDER_SCALE_PRESETS: [RenderScale; 5] = [
    RenderScale::P50,
    RenderScale::P75,
    RenderScale::P100,
    RenderScale::P125,
    RenderScale::P200,
];

/// Internal world-target size after scale and GPU-limit fit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InternalRenderSize {
    pub width: u32,
    pub height: u32,
    pub clamped: bool,
}

/// Scale the **gameplay pixel rect**, not the full OS window.
///
/// `50%` of 1920×1080 → 960×540. Zero gameplay size is invalid. Each axis is
/// at least 1. If the requested size exceeds `max_texture_dimension_2d`, both
/// axes are scaled uniformly so the longer side equals the limit (no stretch).
#[must_use]
pub fn internal_render_size(
    gameplay_width: u32,
    gameplay_height: u32,
    scale: RenderScale,
    max_texture_dimension_2d: u32,
) -> Option<InternalRenderSize> {
    if gameplay_width == 0 || gameplay_height == 0 {
        return None;
    }
    let percent = u64::from(scale.percent());
    let requested_w = ((u64::from(gameplay_width) * percent) / 100).max(1) as u32;
    let requested_h = ((u64::from(gameplay_height) * percent) / 100).max(1) as u32;
    Some(fit_internal_render_size(
        requested_w,
        requested_h,
        max_texture_dimension_2d,
    ))
}

fn fit_internal_render_size(width: u32, height: u32, max_dim: u32) -> InternalRenderSize {
    let max_dim = max_dim.max(1);
    let width = width.max(1);
    let height = height.max(1);
    if width <= max_dim && height <= max_dim {
        return InternalRenderSize {
            width,
            height,
            clamped: false,
        };
    }
    let (fit_w, fit_h) = if width >= height {
        let fit_h = ((u64::from(height) * u64::from(max_dim)) / u64::from(width)).max(1) as u32;
        (max_dim, fit_h.min(max_dim))
    } else {
        let fit_w = ((u64::from(width) * u64::from(max_dim)) / u64::from(height)).max(1) as u32;
        (fit_w.min(max_dim), max_dim)
    };
    InternalRenderSize {
        width: fit_w,
        height: fit_h,
        clamped: true,
    }
}

/// Whether the offscreen world target must be rebuilt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldTargetAction {
    Recreate { width: u32, height: u32 },
    Unchanged,
    SkipInvalid,
}

/// Recreate only when the applied internal size actually changes.
#[must_use]
pub fn classify_world_target_resize(
    configured: Option<(u32, u32)>,
    incoming: (u32, u32),
) -> WorldTargetAction {
    if incoming.0 == 0 || incoming.1 == 0 {
        return WorldTargetAction::SkipInvalid;
    }
    if configured == Some(incoming) {
        return WorldTargetAction::Unchanged;
    }
    WorldTargetAction::Recreate {
        width: incoming.0,
        height: incoming.1,
    }
}

/// Local client display intent. Not replicated. Not authoritative world state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplaySettings {
    pub resolution: Resolution,
    pub window_mode: WindowMode,
    pub ui_scale: UiScale,
    pub render_scale: RenderScale,
}

impl DisplaySettings {
    #[must_use]
    pub const fn default_dev() -> Self {
        Self {
            resolution: Resolution::DEFAULT,
            window_mode: WindowMode::Windowed,
            ui_scale: UiScale::DEFAULT,
            render_scale: RenderScale::DEFAULT,
        }
    }

    /// Replace the requested window resolution. Returns whether a window apply
    /// is required. Invalid sizes are rejected; the previous value is kept.
    pub fn set_resolution(&mut self, resolution: Resolution) -> Result<bool, InvalidResolution> {
        if !resolution.is_valid() {
            return Err(InvalidResolution);
        }
        if self.resolution == resolution {
            return Ok(false);
        }
        self.resolution = resolution;
        Ok(true)
    }

    pub fn set_ui_scale(&mut self, scale: UiScale) {
        self.ui_scale = scale;
    }

    /// Replace internal world render scale. Does not change window size.
    /// Returns whether the value changed.
    pub fn set_render_scale(&mut self, scale: RenderScale) -> bool {
        if self.render_scale == scale {
            return false;
        }
        self.render_scale = scale;
        true
    }
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self::default_dev()
    }
}

/// Rejected because width or height was zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidResolution;

/// Supported windowed resolution presets. Not a claim that the monitor native
/// mode matches; windowed resize may still apply.
pub const RESOLUTION_PRESETS: [Resolution; 5] = [
    Resolution {
        width: 1280,
        height: 720,
    },
    Resolution {
        width: 1366,
        height: 768,
    },
    Resolution {
        width: 1600,
        height: 900,
    },
    Resolution {
        width: 1920,
        height: 1080,
    },
    Resolution {
        width: 2560,
        height: 1440,
    },
];

/// What the shared surface-resize path should do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceResizeAction {
    Reconfigure { width: u32, height: u32 },
    Unchanged,
    SkipInvalid,
}

/// Classify an OS / requested framebuffer size against the last configured
/// surface. Preset apply and manual window dragging share this decision.
#[must_use]
pub fn classify_framebuffer_resize(
    configured: (u32, u32),
    incoming: (u32, u32),
) -> SurfaceResizeAction {
    if incoming.0 == 0 || incoming.1 == 0 {
        return SurfaceResizeAction::SkipInvalid;
    }
    if configured == incoming {
        return SurfaceResizeAction::Unchanged;
    }
    SurfaceResizeAction::Reconfigure {
        width: incoming.0,
        height: incoming.1,
    }
}

/// egui `pixels_per_point` = OS scale factor × user UI scale.
#[must_use]
pub fn effective_pixels_per_point(os_scale_factor: f32, ui_scale: UiScale) -> f32 {
    let ppp = os_scale_factor * ui_scale.get();
    if ppp.is_finite() && ppp > 0.0 {
        ppp.clamp(0.25, 8.0)
    } else {
        1.0
    }
}

/// Observed sizes for debug diagnostics. Not settings intent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayDebug {
    pub window_logical: (u32, u32),
    pub framebuffer: (u32, u32),
    pub surface: (u32, u32),
    pub selected: (u32, u32),
    pub aspect_ratio: f32,
    pub scale_factor: f32,
    pub ui_scale: f32,
    pub render_scale_percent: u16,
    pub window_mode: WindowMode,
    pub monitor_native: Option<(u32, u32)>,
    /// World-pass pixel rect (letterbox / pillarbox). `None` if unused.
    pub gameplay_pixel: Option<(u32, u32, u32, u32)>,
    pub internal_render: Option<(u32, u32)>,
    pub internal_render_clamped: bool,
    /// Applied world-pass sample count (1 or 4).
    pub world_msaa_samples: u32,
    pub world_msaa_4x_supported: bool,
}

impl Default for DisplayDebug {
    fn default() -> Self {
        Self {
            window_logical: (0, 0),
            framebuffer: (0, 0),
            surface: (0, 0),
            selected: (0, 0),
            aspect_ratio: 0.0,
            scale_factor: 1.0,
            ui_scale: 1.0,
            render_scale_percent: 200,
            window_mode: WindowMode::Windowed,
            monitor_native: None,
            gameplay_pixel: None,
            internal_render: None,
            internal_render_clamped: false,
            world_msaa_samples: 4,
            world_msaa_4x_supported: false,
        }
    }
}

impl DisplayDebug {
    #[must_use]
    pub fn aspect_label(self) -> String {
        if self.aspect_ratio.is_finite() && self.aspect_ratio > 0.0 {
            format!("{:.4}", self.aspect_ratio)
        } else {
            "—".into()
        }
    }
}

/// Read current window / surface metrics. Does not mutate GPU state.
#[must_use]
pub fn collect_display_debug(
    window: &Window,
    surface: (u32, u32),
    settings: &DisplaySettings,
) -> DisplayDebug {
    let scale = window.scale_factor();
    let physical = window.inner_size();
    let logical: LogicalSize<f64> = physical.to_logical(scale);
    let aspect = Resolution::new(physical.width, physical.height)
        .and_then(Resolution::aspect_ratio)
        .unwrap_or(0.0);
    let monitor_native = window.current_monitor().map(|monitor| {
        let size = monitor.size();
        (size.width, size.height)
    });
    DisplayDebug {
        window_logical: (
            logical.width.round().max(0.0) as u32,
            logical.height.round().max(0.0) as u32,
        ),
        framebuffer: (physical.width, physical.height),
        surface,
        selected: (settings.resolution.width, settings.resolution.height),
        aspect_ratio: aspect,
        scale_factor: scale as f32,
        ui_scale: settings.ui_scale.get(),
        render_scale_percent: settings.render_scale.percent(),
        window_mode: settings.window_mode,
        monitor_native,
        gameplay_pixel: None,
        internal_render: None,
        internal_render_clamped: false,
        world_msaa_samples: 4,
        world_msaa_4x_supported: false,
    }
}

/// Result of applying a display intent to the native window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowFlush {
    Idle,
    Applied(PhysicalSize<u32>),
    PendingEvent,
}

/// Owns display intent and the pending window-apply request.
///
/// Debug UI and a future Settings menu call [`Self::set_resolution`].
/// Only this type talks to `winit` about inner size. The renderer still owns
/// wgpu surface reconfiguration, driven by [`classify_framebuffer_resize`].
#[derive(Clone, Debug)]
pub struct DisplayController {
    settings: DisplaySettings,
    configured_surface: (u32, u32),
    pending_window_resolution: Option<Resolution>,
}

impl DisplayController {
    #[must_use]
    pub fn new() -> Self {
        Self {
            settings: DisplaySettings::default_dev(),
            configured_surface: (0, 0),
            pending_window_resolution: None,
        }
    }

    #[must_use]
    pub fn settings(&self) -> DisplaySettings {
        self.settings
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn configured_surface(&self) -> (u32, u32) {
        self.configured_surface
    }

    /// After the renderer configures a surface (startup or resize apply).
    pub fn note_surface_configured(&mut self, surface: (u32, u32)) {
        if surface.0 > 0 && surface.1 > 0 {
            self.configured_surface = surface;
            self.settings.resolution = Resolution {
                width: surface.0,
                height: surface.1,
            };
        }
    }

    /// UI / Settings intent. Does not touch wgpu.
    pub fn set_resolution(&mut self, resolution: Resolution) -> Result<(), InvalidResolution> {
        if !self.settings.set_resolution(resolution)? {
            let current = self.configured_surface;
            if current == (resolution.width, resolution.height) {
                return Ok(());
            }
        }
        self.pending_window_resolution = Some(resolution);
        Ok(())
    }

    /// Future Settings menu uses this; Debug UI currently shows scale as diagnostics.
    #[allow(dead_code)]
    pub fn set_ui_scale(&mut self, scale: UiScale) {
        self.settings.set_ui_scale(scale);
    }

    /// UI / Settings intent. Does not resize the window or change camera FOV.
    pub fn set_render_scale(&mut self, scale: RenderScale) -> bool {
        self.settings.set_render_scale(scale)
    }

    /// Apply a pending resolution to the native window.
    ///
    /// If winit applies immediately, the new physical size is returned so the
    /// caller can run the same framebuffer path as `WindowEvent::Resized`.
    pub fn flush_window(&mut self, window: &Window) -> WindowFlush {
        let Some(requested) = self.pending_window_resolution.take() else {
            return WindowFlush::Idle;
        };
        let current = window.inner_size();
        if current.width == requested.width && current.height == requested.height {
            return WindowFlush::Idle;
        }
        match window.request_inner_size(PhysicalSize::new(requested.width, requested.height)) {
            Some(applied) => WindowFlush::Applied(applied),
            None => WindowFlush::PendingEvent,
        }
    }

    /// Shared observe path for OS resize, minimize, and preset apply results.
    pub fn observe_framebuffer(&mut self, width: u32, height: u32) -> SurfaceResizeAction {
        let action = classify_framebuffer_resize(self.configured_surface, (width, height));
        if let SurfaceResizeAction::Reconfigure { width, height } = action {
            self.configured_surface = (width, height);
            self.settings.resolution = Resolution { width, height };
            if self
                .pending_window_resolution
                .is_some_and(|pending| pending.width == width && pending.height == height)
            {
                self.pending_window_resolution = None;
            }
        }
        action
    }
}

impl Default for DisplayController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_resolution_is_historical_1280x720() {
        assert_eq!(
            Resolution::DEFAULT,
            Resolution {
                width: 1280,
                height: 720
            }
        );
        assert_eq!(
            DisplaySettings::default_dev().resolution,
            Resolution::DEFAULT
        );
        assert_eq!(
            DisplaySettings::default_dev().window_mode,
            WindowMode::Windowed
        );
        assert_eq!(DisplaySettings::default_dev().ui_scale.get(), 1.0);
        assert_eq!(
            DisplaySettings::default_dev().render_scale,
            RenderScale::DEFAULT
        );
    }

    #[test]
    fn zero_dimensions_are_invalid() {
        assert!(Resolution::new(0, 0).is_none());
        assert!(Resolution::new(0, 1080).is_none());
        assert!(Resolution::new(1920, 0).is_none());
        assert!(Resolution::new(1920, 1080).is_some());
        assert!(
            !Resolution {
                width: 0,
                height: 0
            }
            .is_valid()
        );
        assert!(
            Resolution {
                width: 0,
                height: 1080
            }
            .aspect_ratio()
            .is_none()
        );
        assert!(
            Resolution {
                width: 1920,
                height: 0
            }
            .aspect_ratio()
            .is_none()
        );
    }

    #[test]
    fn aspect_ratio_is_width_over_height() {
        let hd = Resolution::new(1920, 1080).expect("valid");
        let ratio = hd.aspect_ratio().expect("valid");
        assert!((ratio - 16.0 / 9.0).abs() < 1e-5);
        let four_three = Resolution::new(1600, 1200).expect("valid");
        assert!((four_three.aspect_ratio().unwrap() - 4.0 / 3.0).abs() < 1e-5);
        let ultra = Resolution::new(2560, 1080).expect("valid");
        assert!((ultra.aspect_ratio().unwrap() - 2560.0 / 1080.0).abs() < 1e-5);
    }

    #[test]
    fn presets_are_valid_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for preset in RESOLUTION_PRESETS {
            assert!(preset.is_valid(), "{preset:?}");
            assert!(seen.insert((preset.width, preset.height)), "{preset:?}");
        }
        assert_eq!(RESOLUTION_PRESETS.len(), 5);
        assert!(RESOLUTION_PRESETS.contains(&Resolution::DEFAULT));
    }

    #[test]
    fn set_resolution_rejects_zero_and_keeps_previous() {
        let mut settings = DisplaySettings::default_dev();
        assert_eq!(
            settings.set_resolution(Resolution {
                width: 0,
                height: 1080
            }),
            Err(InvalidResolution)
        );
        assert_eq!(settings.resolution, Resolution::DEFAULT);
        assert_eq!(
            settings.set_resolution(Resolution {
                width: 1920,
                height: 0
            }),
            Err(InvalidResolution)
        );
        assert_eq!(settings.resolution, Resolution::DEFAULT);
    }

    #[test]
    fn same_size_does_not_request_apply() {
        let mut settings = DisplaySettings::default_dev();
        assert_eq!(settings.set_resolution(Resolution::DEFAULT), Ok(false));
        let mut controller = DisplayController::new();
        controller.note_surface_configured((1280, 720));
        controller
            .set_resolution(Resolution::DEFAULT)
            .expect("valid");
        assert!(controller.pending_window_resolution.is_none());
    }

    #[test]
    fn new_valid_size_queues_window_apply() {
        let mut controller = DisplayController::new();
        controller.note_surface_configured((1280, 720));
        controller
            .set_resolution(Resolution {
                width: 1920,
                height: 1080,
            })
            .expect("valid");
        assert_eq!(
            controller.pending_window_resolution,
            Some(Resolution {
                width: 1920,
                height: 1080
            })
        );
        assert_eq!(
            controller.settings().resolution,
            Resolution {
                width: 1920,
                height: 1080
            }
        );
    }

    #[test]
    fn classify_same_size_is_unchanged() {
        assert_eq!(
            classify_framebuffer_resize((1920, 1080), (1920, 1080)),
            SurfaceResizeAction::Unchanged
        );
    }

    #[test]
    fn classify_new_valid_size_reconfigures() {
        assert_eq!(
            classify_framebuffer_resize((1280, 720), (1920, 1080)),
            SurfaceResizeAction::Reconfigure {
                width: 1920,
                height: 1080
            }
        );
    }

    #[test]
    fn classify_zero_size_skips_reconfigure() {
        assert_eq!(
            classify_framebuffer_resize((1280, 720), (0, 0)),
            SurfaceResizeAction::SkipInvalid
        );
        assert_eq!(
            classify_framebuffer_resize((1280, 720), (0, 1080)),
            SurfaceResizeAction::SkipInvalid
        );
        assert_eq!(
            classify_framebuffer_resize((1280, 720), (1920, 0)),
            SurfaceResizeAction::SkipInvalid
        );
        let mut controller = DisplayController::new();
        controller.note_surface_configured((1280, 720));
        assert_eq!(
            controller.observe_framebuffer(0, 720),
            SurfaceResizeAction::SkipInvalid
        );
        assert_eq!(controller.configured_surface(), (1280, 720));
        assert_eq!(controller.settings().resolution, Resolution::DEFAULT);
    }

    #[test]
    fn observe_valid_resize_updates_configured_surface() {
        let mut controller = DisplayController::new();
        controller.note_surface_configured((1280, 720));
        assert_eq!(
            controller.observe_framebuffer(1600, 900),
            SurfaceResizeAction::Reconfigure {
                width: 1600,
                height: 900
            }
        );
        assert_eq!(controller.configured_surface(), (1600, 900));
        assert_eq!(
            controller.settings().resolution,
            Resolution {
                width: 1600,
                height: 900
            }
        );
    }

    #[test]
    fn ui_scale_is_independent_of_resolution() {
        let mut settings = DisplaySettings::default_dev();
        settings.set_ui_scale(UiScale::new(1.25));
        assert_eq!(
            settings.set_resolution(Resolution {
                width: 1920,
                height: 1080
            }),
            Ok(true)
        );
        assert!((settings.ui_scale.get() - 1.25).abs() < f32::EPSILON);
        assert_eq!(settings.render_scale, RenderScale::DEFAULT);
        assert!((effective_pixels_per_point(1.5, UiScale::new(1.25)) - 1.875).abs() < 1e-5);
        assert_eq!(effective_pixels_per_point(f32::NAN, UiScale::DEFAULT), 1.0);
        assert_eq!(effective_pixels_per_point(0.0, UiScale::DEFAULT), 1.0);
    }

    #[test]
    fn ui_scale_clamps_non_finite_and_range() {
        assert_eq!(UiScale::new(f32::INFINITY).get(), 1.0);
        assert_eq!(UiScale::new(0.1).get(), 0.5);
        assert_eq!(UiScale::new(9.0).get(), 2.0);
        let mut controller = DisplayController::new();
        controller.set_ui_scale(UiScale::new(1.25));
        assert!((controller.settings().ui_scale.get() - 1.25).abs() < f32::EPSILON);
    }

    #[test]
    fn render_scale_presets_are_unique_and_labeled() {
        let mut seen = std::collections::HashSet::new();
        for preset in RENDER_SCALE_PRESETS {
            assert!(seen.insert(preset.percent()), "{preset:?}");
            assert_eq!(preset.label(), format!("{}%", preset.percent()));
        }
        assert_eq!(
            RENDER_SCALE_PRESETS,
            [
                RenderScale::P50,
                RenderScale::P75,
                RenderScale::P100,
                RenderScale::P125,
                RenderScale::P200,
            ]
        );
        assert_eq!(RenderScale::DEFAULT, RenderScale::P200);
        assert_eq!(RenderScale::PERFORMANCE_FALLBACK, RenderScale::P100);
        assert!(
            !RENDER_SCALE_PRESETS
                .iter()
                .any(|preset| preset.percent() == 150 || preset.percent() == 400)
        );
    }

    #[test]
    fn internal_size_table_for_1280x720() {
        let cases = [
            (RenderScale::P50, 640, 360),
            (RenderScale::P75, 960, 540),
            (RenderScale::P100, 1280, 720),
            (RenderScale::P125, 1600, 900),
            (RenderScale::P200, 2560, 1440),
        ];
        for (scale, w, h) in cases {
            let size = internal_render_size(1280, 720, scale, 8192).expect("valid");
            assert_eq!(
                (size.width, size.height, size.clamped),
                (w, h, false),
                "{scale:?}"
            );
        }
        let p100 = internal_render_size(1280, 720, RenderScale::P100, 8192).expect("valid");
        let p200 = internal_render_size(1280, 720, RenderScale::DEFAULT, 8192).expect("valid");
        assert_ne!(
            (p100.width, p100.height),
            (p200.width, p200.height),
            "200% must recreate a larger internal target than 100%"
        );
        assert_eq!((p200.width, p200.height), (2560, 1440));
    }

    #[test]
    fn internal_size_table_for_1920x1080() {
        let cases = [
            (RenderScale::P50, 960, 540),
            (RenderScale::P75, 1440, 810),
            (RenderScale::P100, 1920, 1080),
            (RenderScale::P125, 2400, 1350),
            (RenderScale::P200, 3840, 2160),
        ];
        for (scale, w, h) in cases {
            let size = internal_render_size(1920, 1080, scale, 8192).expect("valid");
            assert_eq!(
                (size.width, size.height, size.clamped),
                (w, h, false),
                "{scale:?}"
            );
        }
    }

    #[test]
    fn internal_size_uses_gameplay_rect_not_full_window() {
        // 1600×1000 window → 1600×900 gameplay rect (letterbox). 75% of that.
        let size = internal_render_size(1600, 900, RenderScale::P75, 8192).expect("valid");
        assert_eq!((size.width, size.height), (1200, 675));
        assert!(!size.clamped);
        let wrong_full_window =
            internal_render_size(1600, 1000, RenderScale::P75, 8192).expect("valid");
        assert_ne!(
            (wrong_full_window.width, wrong_full_window.height),
            (1200, 675)
        );
    }

    #[test]
    fn internal_size_rejects_zero_gameplay_rect() {
        assert!(internal_render_size(0, 1080, RenderScale::P100, 8192).is_none());
        assert!(internal_render_size(1920, 0, RenderScale::P100, 8192).is_none());
    }

    #[test]
    fn internal_size_clamps_uniformly_to_gpu_limit() {
        let size = internal_render_size(1920, 1080, RenderScale::P200, 1000).expect("valid");
        assert!(size.clamped);
        assert_eq!(size.width, 1000);
        assert_eq!(size.height, 562);
        assert!(size.width <= 1000 && size.height <= 1000);
    }

    #[test]
    fn world_target_resize_skips_invalid_and_duplicate() {
        assert_eq!(
            classify_world_target_resize(Some((1920, 1080)), (0, 1080)),
            WorldTargetAction::SkipInvalid
        );
        assert_eq!(
            classify_world_target_resize(Some((1920, 1080)), (1920, 1080)),
            WorldTargetAction::Unchanged
        );
        assert_eq!(
            classify_world_target_resize(Some((1920, 1080)), (2880, 1620)),
            WorldTargetAction::Recreate {
                width: 2880,
                height: 1620
            }
        );
        assert_eq!(
            classify_world_target_resize(None, (1920, 1080)),
            WorldTargetAction::Recreate {
                width: 1920,
                height: 1080
            }
        );
        let mut last = Some((1920, 1080));
        for _ in 0..8 {
            let action = classify_world_target_resize(last, (1920, 1080));
            assert_eq!(action, WorldTargetAction::Unchanged);
            last = Some((1920, 1080));
        }
    }

    #[test]
    fn render_scale_does_not_resize_window_or_reset_on_resolution_change() {
        let mut controller = DisplayController::new();
        controller.note_surface_configured((1280, 720));
        assert!(controller.set_render_scale(RenderScale::PERFORMANCE_FALLBACK));
        assert!(!controller.set_render_scale(RenderScale::PERFORMANCE_FALLBACK));
        assert!(controller.pending_window_resolution.is_none());
        assert_eq!(controller.configured_surface(), (1280, 720));
        controller
            .set_resolution(Resolution {
                width: 1920,
                height: 1080,
            })
            .expect("valid");
        assert_eq!(
            controller.settings().render_scale,
            RenderScale::PERFORMANCE_FALLBACK
        );
        assert_eq!(
            controller.observe_framebuffer(1920, 1080),
            SurfaceResizeAction::Reconfigure {
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(
            controller.settings().render_scale,
            RenderScale::PERFORMANCE_FALLBACK
        );
        assert_eq!(controller.settings().resolution.width, 1920);
    }
}
