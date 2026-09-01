//! Shared visual tokens for the Developer Hub (eframe).

use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, Visuals};

// --- layout tokens (design system) ---
pub const PAGE_MARGIN: f32 = 14.0;
pub const SECTION_GAP: f32 = 10.0;
pub const CARD_PAD: f32 = 10.0;
pub const CARD_GAP: f32 = 12.0;
pub const CARD_RADIUS: f32 = 8.0;
pub const SIDEBAR_WIDTH: f32 = 200.0;
pub const SIDEBAR_MIN: f32 = 160.0;
pub const SIDEBAR_ITEM_H: f32 = 28.0;
pub const CHART_HEIGHT: f32 = 140.0;
pub const LOG_VIEW_HEIGHT: f32 = 220.0;
pub const BTN_MIN_H: f32 = 28.0;

pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.window_fill = bg();
    visuals.panel_fill = panel();
    visuals.extreme_bg_color = Color32::from_rgb(8, 10, 14);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(28, 34, 42);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(36, 44, 56);
    visuals.selection.bg_fill = accent().gamma_multiply(0.35);
    visuals.window_corner_radius = CornerRadius::same(CARD_RADIUS as u8);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
    visuals.widgets.active.corner_radius = CornerRadius::same(6);
    ctx.set_visuals(visuals);
}

/// Application / content background ~#0B0E14
pub fn bg() -> Color32 {
    Color32::from_rgb(11, 14, 20)
}

/// Sidebar / header panel ~#11161C
pub fn panel() -> Color32 {
    Color32::from_rgb(17, 22, 28)
}

/// Elevated card surface ~#161B22
pub fn card_fill_elevated() -> Color32 {
    Color32::from_rgb(22, 27, 34)
}

pub fn card_stroke() -> Stroke {
    Stroke::new(1.0, Color32::from_rgb(42, 48, 58))
}

pub fn accent() -> Color32 {
    Color32::from_rgb(47, 129, 247)
}

pub fn destructive() -> Color32 {
    Color32::from_rgb(210, 80, 80)
}

pub fn destructive_fill() -> Color32 {
    Color32::from_rgb(90, 32, 36)
}

pub fn success() -> Color32 {
    Color32::from_rgb(63, 185, 120)
}

pub fn muted() -> Color32 {
    Color32::from_rgb(125, 133, 144)
}

pub fn body() -> Color32 {
    Color32::from_rgb(220, 224, 230)
}

pub fn nav_active_fill() -> Color32 {
    Color32::from_rgb(22, 40, 64)
}

pub fn title_font() -> FontId {
    FontId::proportional(22.0)
}

pub fn subtitle_font() -> FontId {
    FontId::proportional(13.0)
}

pub fn section_font() -> FontId {
    FontId::proportional(13.0)
}

pub fn state_font() -> FontId {
    FontId::proportional(15.0)
}

pub fn metric_font() -> FontId {
    FontId::proportional(14.0)
}

pub fn mono_small() -> FontId {
    FontId::monospace(12.0)
}

pub fn state_color(state: purgatory_dev_runtime::ServerState) -> Color32 {
    use purgatory_dev_runtime::ServerState::*;
    match state {
        Ready => success(),
        Failed => destructive(),
        // Mockup treats STOPPED as attention-red, not idle grey.
        Stopped => destructive(),
        Degraded => Color32::from_rgb(220, 160, 50),
        _ => Color32::from_rgb(210, 175, 70),
    }
}

pub fn outcome_color(state: purgatory_dev_runtime::ValidationState) -> Color32 {
    use purgatory_dev_runtime::ValidationState::*;
    match state {
        Passed => success(),
        Failed | OrchestrationFailed => destructive(),
        Cancelled => Color32::from_rgb(180, 150, 90),
        Idle => muted(),
        _ => Color32::from_rgb(210, 175, 70),
    }
}

pub fn log_color(line: &str) -> Color32 {
    if line.contains("server |") {
        success()
    } else if line.contains("client |") {
        Color32::from_rgb(80, 180, 200)
    } else if line.contains("load |") {
        Color32::from_rgb(170, 140, 210)
    } else if line.contains("cargo |") {
        Color32::from_rgb(190, 180, 90)
    } else if line.to_ascii_lowercase().contains("fail")
        || line.to_ascii_lowercase().contains("error")
    {
        destructive()
    } else {
        Color32::from_gray(200)
    }
}
