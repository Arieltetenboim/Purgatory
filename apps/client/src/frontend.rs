//! Development Connection Frontend. Not production UI. Not the debug overlay.

use egui::{ColorImage, Context, Id, TextureHandle, TextureOptions};

use crate::assets::connection_logo_path;

/// In-window connection screen. Owns the logo texture for the process lifetime.
pub struct ConnectionFrontend {
    logo: Option<TextureHandle>,
}

impl ConnectionFrontend {
    /// Decode the logo once. Missing/invalid PNG logs a single warning.
    #[must_use]
    pub fn load(ctx: &Context) -> Self {
        Self {
            logo: load_logo(ctx),
        }
    }

    /// Draw the connection panel. Returns true when CONNECT is clicked.
    pub fn paint(
        &self,
        ctx: &Context,
        server: &str,
        login: &mut String,
        status: &str,
        can_connect: bool,
    ) -> bool {
        let mut connect = false;
        let screen = ctx.content_rect();
        egui::Area::new(Id::new("purgatory-connection-frontend"))
            .fixed_pos(screen.min)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_min_size(screen.size());
                ui.painter()
                    .rect_filled(screen, 0.0, egui::Color32::from_rgb(8, 8, 10));
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    paint_logo_or_title(ui, self.logo.as_ref());
                    ui.add_space(28.0);
                    ui.label("Server:");
                    ui.label(server);
                    ui.add_space(12.0);
                    ui.label("DEV login:");
                    ui.add_sized(
                        [220.0, 24.0],
                        egui::TextEdit::singleline(login).char_limit(32),
                    );
                    ui.add_space(16.0);
                    ui.add_enabled_ui(can_connect, |ui| {
                        if ui
                            .add_sized([180.0, 36.0], egui::Button::new("CONNECT"))
                            .clicked()
                        {
                            connect = true;
                        }
                    });
                    ui.add_space(16.0);
                    ui.label(format!("Status: {status}"));
                });
            });
        connect
    }
}

fn paint_logo_or_title(ui: &mut egui::Ui, logo: Option<&TextureHandle>) {
    if let Some(logo) = logo {
        let size = logo.size_vec2();
        if size.x > 1.0 && size.y > 1.0 {
            let max_w = (ui.available_width() * 0.72).clamp(160.0, 720.0);
            let max_h = (ui.available_height() * 0.28).clamp(72.0, 220.0);
            let scale = (max_w / size.x).min(max_h / size.y);
            let draw = egui::vec2(size.x * scale, size.y * scale);
            let tex = egui::load::SizedTexture::from_handle(logo);
            ui.add(
                egui::Image::from_texture(tex)
                    .fit_to_exact_size(draw)
                    .maintain_aspect_ratio(true),
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
    let image = match image::load_from_memory(&bytes) {
        Ok(img) => img.to_rgba8(),
        Err(err) => {
            eprintln!("PURGATORY warning: connection logo decode failed: {err}");
            return None;
        }
    };
    let size = [image.width() as usize, image.height() as usize];
    let color = ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture("purgatory-connection-logo", color, TextureOptions::LINEAR))
}
