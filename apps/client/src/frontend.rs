//! Animated alpha-stage Connection Frontend.
//!
//! This remains presentation-only: connection lifecycle and identity stay owned by ClientApp /
//! ClientLifecycle. The frontend owns only visual timing and egui textures.

use std::time::{Duration, Instant};

use egui::{
    Align2, Color32, ColorImage, Context, FontId, Id, Order, Pos2, Rect, Response, Sense, Stroke,
    TextureHandle, TextureOptions, Vec2,
};
use purgatory_common::DevLogin;
use serde::Deserialize;

use crate::assets::connection_logo_path;

const UI_ATLAS_PNG: &[u8] = include_bytes!("../../../Graphic/ui/ATLAS.png");
const UI_ATLAS_METADATA: &str = include_str!("../../../Graphic/ui/ATLAS.ui.json");

const FOREGROUND_FADE_OUT: Duration = Duration::from_millis(280);
const LOGO_FADE_START_SECONDS: f32 = 0.08;
const LOGO_FADE_END_SECONDS: f32 = 0.85;
const CONTROLS_FADE_START_SECONDS: f32 = 0.30;
const CONTROLS_FADE_END_SECONDS: f32 = 1.05;
const LOGO_FLOAT_AMPLITUDE_POINTS: f32 = 4.0;
const LOGO_FLOAT_PERIOD_SECONDS: f32 = 17.6;
const LOGO_FLOAT_X_AMPLITUDE_POINTS: f32 = 3.0;
const LOGO_FLOAT_X_PERIOD_SECONDS: f32 = 26.8;

/// In-window connection screen. Owns presentation textures for the process lifetime.
pub struct ConnectionFrontend {
    logo: Option<TextureHandle>,
    buttons: Option<ButtonAtlas>,
    entered_at: Instant,
    phase: FrontendPhase,
}

#[derive(Clone, Copy, Debug)]
enum FrontendPhase {
    Ready,
    Starting { started_at: Instant },
    AwaitingConnection,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
enum AlphaButtonStyle {
    Primary,
    Secondary,
    Danger,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct SourceRectPx {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct ButtonStates {
    normal: SourceRectPx,
    hover: SourceRectPx,
    pressed: SourceRectPx,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct HorizontalInsetsPx {
    left: u32,
    right: u32,
}

#[derive(Debug, Deserialize)]
struct ButtonStyles {
    beige: ButtonStates,
    red: ButtonStates,
}

#[derive(Debug, Deserialize)]
struct ButtonAtlasMetadata {
    dimensions_px: [u32; 2],
    buttons: ButtonStyles,
    button_slice_px: HorizontalInsetsPx,
}

struct ButtonAtlas {
    texture: TextureHandle,
    dimensions_px: [u32; 2],
    beige: ButtonStates,
    red: ButtonStates,
    slice_px: HorizontalInsetsPx,
}

impl ConnectionFrontend {
    /// Decode frontend textures once. Missing/invalid assets warn and retain usable fallbacks.
    #[must_use]
    pub fn load(ctx: &Context) -> Self {
        Self {
            logo: load_logo(ctx),
            buttons: ButtonAtlas::load(ctx),
            entered_at: Instant::now(),
            phase: FrontendPhase::Ready,
        }
    }

    /// Draw the alpha Connection Frontend.
    ///
    /// Returns true once, after the short foreground fade, when a connection attempt should begin.
    pub fn paint(
        &mut self,
        ctx: &Context,
        server: &str,
        login: &mut String,
        status: &str,
        can_connect: bool,
    ) -> bool {
        let now = Instant::now();
        if matches!(self.phase, FrontendPhase::AwaitingConnection) && can_connect {
            // A failed/rejected/unsent attempt returned ownership to the Connection screen.
            self.phase = FrontendPhase::Ready;
            self.entered_at = now;
        }

        let elapsed = now.duration_since(self.entered_at).as_secs_f32();
        let logo_alpha = fade_window(elapsed, LOGO_FADE_START_SECONDS, LOGO_FADE_END_SECONDS);
        let controls_alpha = fade_window(
            elapsed,
            CONTROLS_FADE_START_SECONDS,
            CONTROLS_FADE_END_SECONDS,
        );
        let foreground_exit_alpha = match self.phase {
            FrontendPhase::Ready => 1.0,
            FrontendPhase::Starting { started_at } => {
                1.0 - smoothstep01(
                    now.duration_since(started_at).as_secs_f32()
                        / FOREGROUND_FADE_OUT.as_secs_f32(),
                )
            }
            FrontendPhase::AwaitingConnection => 0.0,
        };

        let mut connect = false;
        if let FrontendPhase::Starting { started_at } = self.phase
            && now.duration_since(started_at) >= FOREGROUND_FADE_OUT
        {
            self.phase = FrontendPhase::AwaitingConnection;
            connect = true;
        }

        ctx.request_repaint_after(Duration::from_millis(16));
        let screen = ctx.content_rect();
        egui::Area::new(Id::new("purgatory-connection-frontend"))
            .order(Order::Middle)
            .fixed_pos(screen.min)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_width(screen.width());
                ui.set_height(screen.height());

                ui.vertical_centered(|ui| {
                    ui.add_space((screen.height() * 0.105).clamp(32.0, 104.0));
                    ui.scope(|ui| {
                        ui.set_opacity((logo_alpha * foreground_exit_alpha).clamp(0.0, 1.0));
                        let float_offset = logo_float_offset(elapsed);
                        paint_logo_or_title(ui, self.logo.as_ref(), float_offset);
                    });

                    ui.add_space((screen.height() * 0.052).clamp(22.0, 54.0));
                    ui.scope(|ui| {
                        ui.set_opacity((controls_alpha * foreground_exit_alpha).clamp(0.0, 1.0));
                        paint_login_controls(
                            ui,
                            self.buttons.as_ref(),
                            login,
                            can_connect && matches!(self.phase, FrontendPhase::Ready),
                            &mut self.phase,
                            now,
                        );
                    });

                    match self.phase {
                        FrontendPhase::Starting { .. } => {
                            ui.add_space(12.0);
                            ui.label(
                                egui::RichText::new("Entering…")
                                    .size(13.0)
                                    .color(Color32::from_gray(205)),
                            );
                        }
                        FrontendPhase::AwaitingConnection => {
                            ui.add_space(12.0);
                            ui.label(
                                egui::RichText::new(status)
                                    .size(13.0)
                                    .color(Color32::from_gray(205)),
                            );
                        }
                        FrontendPhase::Ready if !can_connect => {
                            ui.add_space(12.0);
                            ui.label(
                                egui::RichText::new(status)
                                    .size(13.0)
                                    .color(Color32::from_gray(205)),
                            );
                        }
                        FrontendPhase::Ready => {}
                    }
                });
            });

        paint_dev_strip(
            ctx,
            server,
            status,
            (controls_alpha * foreground_exit_alpha).max(0.35),
        );
        connect
    }
}

fn paint_login_controls(
    ui: &mut egui::Ui,
    buttons: Option<&ButtonAtlas>,
    login: &mut String,
    can_start: bool,
    phase: &mut FrontendPhase,
    now: Instant,
) {
    let login_valid = DevLogin::parse(login).is_ok();
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(15, 17, 21, 220))
        .stroke(Stroke::new(
            1.0,
            if login_valid {
                Color32::from_rgba_unmultiplied(147, 165, 176, 185)
            } else {
                Color32::from_rgb(176, 74, 66)
            },
        ))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(268.0);
            ui.label(
                egui::RichText::new("USERNAME")
                    .size(10.0)
                    .strong()
                    .color(Color32::from_gray(165)),
            );
            ui.add_space(2.0);
            ui.add_sized(
                [268.0, 26.0],
                egui::TextEdit::singleline(login)
                    .frame(egui::Frame::NONE)
                    .char_limit(32)
                    .hint_text("Enter username"),
            );
        });

    if !login.is_empty() && !login_valid {
        ui.add_space(5.0);
        ui.label(
            egui::RichText::new("2–32 chars: a–z, 0–9, _ or .")
                .size(11.0)
                .color(Color32::from_rgb(218, 150, 140)),
        );
    }

    ui.add_space(16.0);
    let enabled = can_start && login_valid;
    let response = match buttons {
        Some(buttons) => buttons.button(
            ui,
            "ENTER PURGATORY",
            238.0,
            AlphaButtonStyle::Primary,
            enabled,
        ),
        None => ui.add_enabled(
            enabled,
            egui::Button::new("ENTER PURGATORY").min_size(egui::vec2(238.0, 36.0)),
        ),
    };
    let enter_pressed = ui.input(|input| input.key_pressed(egui::Key::Enter));
    if enabled && (response.clicked() || enter_pressed) {
        *phase = FrontendPhase::Starting { started_at: now };
    }
}

fn paint_dev_strip(ctx: &Context, server: &str, status: &str, opacity: f32) {
    egui::Area::new(Id::new("purgatory-connection-dev-strip"))
        .order(Order::Foreground)
        .anchor(Align2::LEFT_BOTTOM, [14.0, -14.0])
        .show(ctx, |ui| {
            ui.set_opacity(opacity.clamp(0.0, 1.0));
            ui.set_min_width(320.0);
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(6, 7, 9, 185))
                .stroke(Stroke::new(
                    1.0,
                    Color32::from_rgba_unmultiplied(135, 145, 158, 90),
                ))
                .corner_radius(3.0)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_min_width(360.0);
                    ui.collapsing(
                        egui::RichText::new("DEV")
                            .strong()
                            .size(11.0)
                            .color(Color32::from_gray(205)),
                        |ui| {
                            ui.small("Diagnostics: ON  (~ toggles full overlay)");
                            ui.small(format!("Server: {server}"));
                            ui.small(format!("Status: {status}"));
                            ui.small(format!(
                                "Client: v{}  Protocol: {}",
                                purgatory_common::version(),
                                purgatory_protocol::PROTOCOL_VERSION
                            ));
                        },
                    );
                });
        });
}

impl ButtonAtlas {
    fn load(ctx: &Context) -> Option<Self> {
        let metadata: ButtonAtlasMetadata = match serde_json::from_str(UI_ATLAS_METADATA) {
            Ok(metadata) => metadata,
            Err(err) => {
                eprintln!("PURGATORY warning: frontend UI atlas metadata invalid: {err}");
                return None;
            }
        };
        let texture = load_embedded_texture(ctx, "purgatory-frontend-ui-atlas", UI_ATLAS_PNG)?;
        Some(Self {
            texture,
            dimensions_px: metadata.dimensions_px,
            beige: metadata.buttons.beige,
            red: metadata.buttons.red,
            slice_px: metadata.button_slice_px,
        })
    }

    fn button(
        &self,
        ui: &mut egui::Ui,
        label: &str,
        width: f32,
        style: AlphaButtonStyle,
        enabled: bool,
    ) -> Response {
        let height = 36.0;
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), sense);
        let pointer_down = ui.input(|input| input.pointer.primary_down());
        let states = match style {
            AlphaButtonStyle::Primary | AlphaButtonStyle::Secondary => self.beige,
            AlphaButtonStyle::Danger => self.red,
        };
        let source = if enabled && response.hovered() && pointer_down {
            states.pressed
        } else if enabled && response.hovered() {
            states.hover
        } else {
            states.normal
        };
        let tint_alpha = match style {
            AlphaButtonStyle::Secondary => 0.82,
            _ if enabled => 1.0,
            _ => 0.46,
        };
        self.paint_three_slice(ui.painter(), rect, source, tint_alpha);
        ui.painter().text(
            rect.center()
                + egui::vec2(
                    0.0,
                    if pointer_down && response.hovered() {
                        1.0
                    } else {
                        0.0
                    },
                ),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(13.5),
            Color32::from_rgba_unmultiplied(
                16,
                20,
                27,
                (255.0 * if enabled { 1.0 } else { 0.58 }) as u8,
            ),
        );
        response
    }

    fn paint_three_slice(
        &self,
        painter: &egui::Painter,
        destination: Rect,
        source: SourceRectPx,
        opacity: f32,
    ) {
        let source_left = self.slice_px.left.min(source.width);
        let source_right = self
            .slice_px
            .right
            .min(source.width.saturating_sub(source_left));
        let source_middle = source
            .width
            .saturating_sub(source_left + source_right)
            .max(1);
        let target_scale = destination.height() / source.height.max(1) as f32;
        let left_width = (source_left as f32 * target_scale).min(destination.width() * 0.5);
        let right_width = (source_right as f32 * target_scale).min(destination.width() * 0.5);
        let middle_width = (destination.width() - left_width - right_width).max(0.0);
        let tint = Color32::from_white_alpha((opacity.clamp(0.0, 1.0) * 255.0) as u8);

        let left_dest = Rect::from_min_size(
            destination.min,
            egui::vec2(left_width, destination.height()),
        );
        let middle_dest = Rect::from_min_size(
            Pos2::new(destination.min.x + left_width, destination.min.y),
            egui::vec2(middle_width, destination.height()),
        );
        let right_dest = Rect::from_min_size(
            Pos2::new(destination.max.x - right_width, destination.min.y),
            egui::vec2(right_width, destination.height()),
        );

        let left_src = SourceRectPx {
            width: source_left.max(1),
            ..source
        };
        let middle_src = SourceRectPx {
            x: source.x + source_left,
            width: source_middle,
            ..source
        };
        let right_src = SourceRectPx {
            x: source.x + source.width.saturating_sub(source_right),
            width: source_right.max(1),
            ..source
        };

        painter.image(
            self.texture.id(),
            left_dest,
            source_uv(left_src, self.dimensions_px),
            tint,
        );
        if middle_width > 0.0 {
            painter.image(
                self.texture.id(),
                middle_dest,
                source_uv(middle_src, self.dimensions_px),
                tint,
            );
        }
        painter.image(
            self.texture.id(),
            right_dest,
            source_uv(right_src, self.dimensions_px),
            tint,
        );
    }
}

fn source_uv(source: SourceRectPx, atlas: [u32; 2]) -> Rect {
    let w = atlas[0].max(1) as f32;
    let h = atlas[1].max(1) as f32;
    Rect::from_min_max(
        Pos2::new(source.x as f32 / w, source.y as f32 / h),
        Pos2::new(
            (source.x + source.width) as f32 / w,
            (source.y + source.height) as f32 / h,
        ),
    )
}

fn paint_logo_or_title(ui: &mut egui::Ui, logo: Option<&TextureHandle>, float_offset: Vec2) {
    if let Some(logo) = logo {
        let size = logo.size_vec2();
        if size.x > 1.0 && size.y > 1.0 {
            let max_w = (ui.available_width() * 0.72).clamp(360.0, 1035.0);
            let max_h = (ui.available_height() * 0.375).clamp(123.0, 345.0);
            let scale = (max_w / size.x).min(max_h / size.y);
            let draw = egui::vec2(size.x * scale, size.y * scale);
            let (allocated, _) = ui.allocate_exact_size(draw, Sense::hover());
            let rect = allocated.translate(float_offset);
            ui.painter().image(
                logo.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            return;
        }
    }
    ui.heading("PURGATORY");
}

fn load_logo(ctx: &Context) -> Option<TextureHandle> {
    let path = connection_logo_path();
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!(
                "PURGATORY warning: connection logo missing at {}: {err}",
                path.display()
            );
            return None;
        }
    };
    load_texture(ctx, "purgatory-connection-logo", &bytes)
}

fn load_embedded_texture(ctx: &Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    load_texture(ctx, name, bytes)
}

fn load_texture(ctx: &Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let image = match image::load_from_memory(bytes) {
        Ok(img) => img.to_rgba8(),
        Err(err) => {
            eprintln!("PURGATORY warning: frontend texture '{name}' decode failed: {err}");
            return None;
        }
    };
    let size = [image.width() as usize, image.height() as usize];
    let color = ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture(name, color, TextureOptions::LINEAR))
}

fn fade_window(elapsed: f32, start: f32, end: f32) -> f32 {
    if end <= start {
        return (elapsed >= end) as u8 as f32;
    }
    smoothstep01((elapsed - start) / (end - start))
}

fn smoothstep01(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn logo_float_offset(elapsed: f32) -> Vec2 {
    let x = LOGO_FLOAT_X_AMPLITUDE_POINTS
        * (elapsed * std::f32::consts::TAU / LOGO_FLOAT_X_PERIOD_SECONDS + 1.13).sin()
        + 1.1 * (elapsed * std::f32::consts::TAU / 12.4 + 0.38).sin();
    let y = LOGO_FLOAT_AMPLITUDE_POINTS
        * (elapsed * std::f32::consts::TAU / LOGO_FLOAT_PERIOD_SECONDS).sin()
        + 1.2 * (elapsed * std::f32::consts::TAU / 35.6 + 0.47).sin();
    egui::vec2(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_curve_is_clamped_and_monotonic() {
        assert_eq!(smoothstep01(-1.0), 0.0);
        assert_eq!(smoothstep01(2.0), 1.0);
        let samples = [0.0, 0.2, 0.4, 0.6, 0.8, 1.0].map(smoothstep01);
        assert!(samples.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn logo_float_is_small_and_bounded() {
        for step in 0..=120 {
            let t = step as f32 * 0.1;
            let offset = logo_float_offset(t);
            assert!(offset.x.abs() <= LOGO_FLOAT_X_AMPLITUDE_POINTS + 1.101);
            assert!(offset.y.abs() <= LOGO_FLOAT_AMPLITUDE_POINTS + 1.201);
        }
    }

    #[test]
    fn fade_window_respects_delay() {
        assert_eq!(fade_window(0.10, 0.30, 1.0), 0.0);
        assert_eq!(fade_window(1.10, 0.30, 1.0), 1.0);
    }
}
