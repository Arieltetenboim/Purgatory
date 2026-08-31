use eframe::egui::{self, Color32, Visuals};

pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.window_fill = Color32::from_rgb(22, 24, 28);
    visuals.panel_fill = Color32::from_rgb(28, 31, 36);
    visuals.extreme_bg_color = Color32::from_rgb(18, 20, 24);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(40, 44, 52);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(52, 58, 68);
    visuals.selection.bg_fill = Color32::from_rgb(46, 90, 120);
    ctx.set_visuals(visuals);
}

pub fn state_color(state: purgatory_dev_runtime::ServerState) -> Color32 {
    use purgatory_dev_runtime::ServerState::*;
    match state {
        Ready => Color32::from_rgb(90, 175, 110),
        Failed => Color32::from_rgb(210, 80, 80),
        Stopped => Color32::from_rgb(140, 145, 155),
        Degraded => Color32::from_rgb(220, 160, 50),
        _ => Color32::from_rgb(210, 175, 70),
    }
}

pub fn muted() -> Color32 {
    Color32::from_rgb(150, 156, 166)
}

pub fn log_color(line: &str) -> Color32 {
    if line.contains("server |") {
        Color32::from_rgb(90, 175, 110)
    } else if line.contains("cargo |") {
        Color32::from_rgb(190, 180, 90)
    } else {
        Color32::from_gray(200)
    }
}
