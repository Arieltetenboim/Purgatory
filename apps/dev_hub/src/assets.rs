//! Hub asset paths. Temporary filesystem lookup for development.

use std::path::PathBuf;

use egui::{ColorImage, Context, TextureHandle, TextureOptions};

/// Development path to the Hub brand logo (`Graphic/LOGO.png`).
///
/// Packaged builds should change this function only — not paint call sites.
#[must_use]
pub fn hub_logo_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Graphic/LOGO.png")
}

/// Decode the logo once. Missing/invalid PNG logs a single warning.
#[must_use]
pub fn load_logo(ctx: &Context) -> Option<TextureHandle> {
    let path = hub_logo_path();
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!(
                "PURGATORY warning: Hub logo missing at {}: {err}",
                path.display()
            );
            return None;
        }
    };
    let image = match image::load_from_memory(&bytes) {
        Ok(img) => img.to_rgba8(),
        Err(err) => {
            eprintln!("PURGATORY warning: Hub logo decode failed: {err}");
            return None;
        }
    };
    let size = [image.width() as usize, image.height() as usize];
    let color = ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture("purgatory-hub-logo", color, TextureOptions::LINEAR))
}

/// Paint the logo fitted inside `max_w` × `max_h`, preserving aspect ratio.
/// Falls back to a text title when the texture is unavailable.
pub fn paint_logo(
    ui: &mut egui::Ui,
    logo: Option<&TextureHandle>,
    max_w: f32,
    max_h: f32,
    fallback: &str,
) {
    if let Some(logo) = logo {
        let size = logo.size_vec2();
        if size.x > 1.0 && size.y > 1.0 {
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
    ui.label(
        egui::RichText::new(fallback)
            .font(crate::theme::section_font())
            .strong()
            .color(crate::theme::body()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_logo_path_points_at_graphic_logo() {
        let path = hub_logo_path();
        assert!(
            path.ends_with("Graphic/LOGO.png") || path.ends_with("Graphic\\LOGO.png"),
            "{}",
            path.display()
        );
        assert!(path.is_file(), "expected logo file at {}", path.display());
    }
}
