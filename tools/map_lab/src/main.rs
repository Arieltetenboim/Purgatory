//! FORGE W1.3A Map Lab: faithful canonical-map preview and scale calibration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, Stroke, TextureHandle, Vec2};
use purgatory_content::{MapPresentation, PresentationSprite, TileTransform};
use purgatory_map_lab::MapLabDocument;

const CAMERA_HEIGHT_WU: f32 = purgatory_simulation::FOOTNOTE_TEST_VIEWPORT_HEIGHT;
const CAMERA_WIDTH_WU: f32 = CAMERA_HEIGHT_WU * purgatory_simulation::AOI_VIEWPORT_ASPECT;
const PLAYER_PRESENTATION_SCALE: f32 = 1.15;
const PLAYER_REFERENCE_SIZE_WU: [f32; 2] = [
    purgatory_simulation::PLAYER_HALF_EXTENTS[0] * 2.0 * PLAYER_PRESENTATION_SCALE,
    purgatory_simulation::PLAYER_HALF_EXTENTS[1] * 2.0 * PLAYER_PRESENTATION_SCALE,
];

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1500.0, 900.0])
            .with_min_inner_size([1100.0, 700.0])
            .with_resizable(true)
            .with_title("PURGATORY Map Lab"),
        ..Default::default()
    };
    eframe::run_native(
        "PURGATORY Map Lab",
        options,
        Box::new(|cc| Ok(Box::new(MapLabApp::new(&cc.egui_ctx)))),
    )
}

struct MapLabApp {
    path_text: String,
    document: Option<MapLabDocument>,
    textures: HashMap<String, TextureHandle>,
    preview_layers: Vec<bool>,
    ppu_text: String,
    compiled_ppu: Option<f32>,
    status: String,
    zoom: f32,
    pan: Vec2,
    fit_requested: bool,
}

impl MapLabApp {
    fn new(ctx: &egui::Context) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/authoring/maps/map.dev.footnote.purgatory-map.json");
        let mut app = Self {
            path_text: path.display().to_string(),
            document: None,
            textures: HashMap::new(),
            preview_layers: Vec::new(),
            ppu_text: String::new(),
            compiled_ppu: None,
            status: "Not compiled".to_owned(),
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit_requested: true,
        };
        app.open(ctx, &path);
        app
    }

    fn open(&mut self, ctx: &egui::Context, path: &Path) {
        self.clear_compiled("Compiling...");
        match MapLabDocument::open(path) {
            Ok(document) => {
                self.ppu_text = document.source.pixels_per_world_unit.to_string();
                self.install_document(ctx, document);
            }
            Err(error) => self.status = format!("COMPILE ERROR\n{error}"),
        }
    }

    fn reload(&mut self, ctx: &egui::Context) {
        let path = PathBuf::from(self.path_text.trim());
        self.open(ctx, &path);
    }

    fn apply_ppu(&mut self, ctx: &egui::Context) {
        let Ok(ppu) = self.ppu_text.trim().parse::<f32>() else {
            self.clear_compiled("COMPILE ERROR\nPPU must be a number");
            return;
        };
        let Some(mut document) = self.document.take() else {
            self.status = "COMPILE ERROR\nOpen a valid sidecar first".to_owned();
            return;
        };
        if let Err(error) = document.recompile(ppu) {
            self.clear_compiled(&format!("COMPILE ERROR\n{error}"));
            return;
        }
        self.install_document(ctx, document);
    }

    fn install_document(&mut self, ctx: &egui::Context, document: MapLabDocument) {
        match load_textures(ctx, &document) {
            Ok(textures) => {
                self.path_text = document.sidecar_path.display().to_string();
                self.preview_layers = document
                    .presentation
                    .layers
                    .iter()
                    .map(|layer| layer.visible)
                    .collect();
                self.compiled_ppu = Some(document.presentation.pixels_per_world_unit);
                self.status = format!(
                    "GREEN · schema v{} · {} layers · {} sprites",
                    document.presentation.schema_version,
                    document.presentation.layers.len(),
                    document
                        .presentation
                        .layers
                        .iter()
                        .map(|layer| layer.sprites.len())
                        .sum::<usize>()
                );
                self.document = Some(document);
                self.textures = textures;
                self.fit_requested = true;
            }
            Err(error) => self.clear_compiled(&format!("ASSET ERROR\n{error}")),
        }
    }

    fn clear_compiled(&mut self, status: &str) {
        self.document = None;
        self.textures.clear();
        self.preview_layers.clear();
        self.compiled_ppu = None;
        self.status = status.to_owned();
    }

    fn export_artifact(&mut self) {
        let Some(document) = &self.document else {
            self.status = "EXPORT ERROR\nNo current compiled output".to_owned();
            return;
        };
        let output = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/map-lab")
            .join(format!(
                "{}.map-presentation-v1.json",
                document.presentation.map_authored
            ));
        let result = (|| {
            let parent = output.parent().ok_or("invalid output path")?;
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
            let bytes = document.canonical_json()?;
            std::fs::write(&output, bytes)
                .map_err(|error| format!("write {}: {error}", output.display()))
        })();
        self.status = match result {
            Ok(()) => format!("EXPORTED\n{}", output.display()),
            Err(error) => format!("EXPORT ERROR\n{error}"),
        };
    }

    fn dirty(&self) -> bool {
        let Some(compiled) = self.compiled_ppu else {
            return false;
        };
        self.ppu_text
            .trim()
            .parse::<f32>()
            .is_ok_and(|edited| (edited - compiled).abs() > f32::EPSILON)
    }
}

impl eframe::App for MapLabApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("map_lab_top")
            .exact_size(54.0)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.heading("MAP LAB");
                    ui.separator();
                    ui.add(
                        egui::TextEdit::singleline(&mut self.path_text)
                            .desired_width(560.0)
                            .hint_text("PURGATORY map sidecar"),
                    );
                    if ui.button("Open / Reload").clicked() {
                        self.reload(ui.ctx());
                    }
                    if ui.button("Fit Map").clicked() {
                        self.fit_requested = true;
                    }
                    if ui.button("Export Canonical JSON").clicked() {
                        self.export_artifact();
                    }
                    ui.separator();
                    ui.colored_label(
                        if self.document.is_some() {
                            Color32::LIGHT_GREEN
                        } else {
                            Color32::LIGHT_RED
                        },
                        if self.dirty() {
                            "DIRTY · not compiled"
                        } else {
                            self.status.lines().next().unwrap_or("Not compiled")
                        },
                    );
                });
            });

        egui::Panel::left("map_lab_layers")
            .resizable(true)
            .default_size(210.0)
            .min_size(160.0)
            .show(ui, |ui| {
                ui.heading("LAYERS");
                ui.label("Preview-only visibility");
                ui.separator();
                if let Some(document) = &self.document {
                    for (index, layer) in document.presentation.layers.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.preview_layers[index], "");
                            ui.label(&layer.name);
                        });
                        ui.small(format!(
                            "{:?} · {} sprite{}{}",
                            layer.kind,
                            layer.sprites.len(),
                            if layer.sprites.len() == 1 { "" } else { "s" },
                            if layer.visible {
                                ""
                            } else {
                                " · source hidden"
                            }
                        ));
                        ui.add_space(5.0);
                    }
                } else {
                    ui.label("No current compiler output.");
                }
            });

        egui::Panel::right("map_lab_info")
            .resizable(true)
            .default_size(270.0)
            .min_size(230.0)
            .show(ui, |ui| {
                ui.heading("MAP INFO");
                ui.separator();
                if let Some(document) = &self.document {
                    let map = &document.presentation;
                    ui.label(format!("Map: {}", map.map_authored));
                    ui.label(format!(
                        "TMX: {} × {} px",
                        map.visual_extent_px[0], map.visual_extent_px[1]
                    ));
                    ui.horizontal(|ui| {
                        ui.label("PPU");
                        ui.add(egui::TextEdit::singleline(&mut self.ppu_text).desired_width(90.0));
                    });
                    if ui.button("Apply PPU to Preview").clicked() {
                        self.apply_ppu(ui.ctx());
                    }
                    ui.small(
                        "Calibration is in memory. Edit the sidecar deliberately to persist it.",
                    );
                    let width = map.world_bounds[2] - map.world_bounds[0];
                    let height = map.world_bounds[3] - map.world_bounds[1];
                    ui.label(format!("World: {width:.3} × {height:.3} wu"));
                    ui.separator();
                    ui.label(format!(
                        "Camera: {:.3} × {:.3} wu",
                        CAMERA_WIDTH_WU, CAMERA_HEIGHT_WU
                    ));
                    ui.label(format!(
                        "Player ref: {:.3} × {:.3} wu",
                        PLAYER_REFERENCE_SIZE_WU[0], PLAYER_REFERENCE_SIZE_WU[1]
                    ));
                    ui.small("Player reference = current 1.15× presentation-scaled gameplay body.");
                }
                ui.separator();
                ui.heading("COMPILER / VALIDATION");
                ui.add(egui::Label::new(&self.status).wrap().selectable(true));
            });

        egui::Panel::bottom("map_lab_status")
            .exact_size(28.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Zoom {:.0}%", self.zoom * 100.0));
                    ui.separator();
                    ui.label(format!("Pan {:.0}, {:.0}px", self.pan.x, self.pan.y));
                    ui.separator();
                    ui.label("Drag to pan · wheel to zoom · source files are not rewritten");
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            self.preview(ui);
        });
    }
}

impl MapLabApp {
    fn preview(&mut self, ui: &mut egui::Ui) {
        let (canvas, response) = ui.allocate_exact_size(ui.available_size(), Sense::drag());
        ui.painter()
            .rect_filled(canvas, 0.0, Color32::from_rgb(24, 27, 32));
        if response.dragged() {
            self.pan += ui.input(|input| input.pointer.delta());
        }
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll.abs() > f32::EPSILON {
                self.zoom = (self.zoom * (scroll * 0.0015).exp()).clamp(0.1, 12.0);
            }
        }
        if self.fit_requested {
            self.zoom = 1.0;
            self.pan = Vec2::ZERO;
            self.fit_requested = false;
        }

        let Some(document) = &self.document else {
            ui.painter().text(
                canvas.center(),
                egui::Align2::CENTER_CENTER,
                "No current canonical map\nSee compiler status",
                egui::FontId::proportional(18.0),
                Color32::LIGHT_RED,
            );
            return;
        };
        let map = &document.presentation;
        let world_width = map.world_bounds[2] - map.world_bounds[0];
        let world_height = map.world_bounds[3] - map.world_bounds[1];
        let base_scale =
            ((canvas.width() - 40.0) / world_width).min((canvas.height() - 40.0) / world_height);
        let scale = base_scale.max(0.01) * self.zoom;
        let center = canvas.center() + self.pan;
        let to_screen = |world: [f32; 2]| {
            Pos2::new(
                center.x + (world[0] - world_width * 0.5) * scale,
                center.y - (world[1] - world_height * 0.5) * scale,
            )
        };
        let map_rect = Rect::from_two_pos(
            to_screen([0.0, world_height]),
            to_screen([world_width, 0.0]),
        );
        let map_clip = map_rect.intersect(canvas);
        ui.painter()
            .rect_filled(map_rect, 0.0, Color32::from_rgb(10, 12, 15));
        let map_painter = ui.painter().with_clip_rect(map_clip);

        for (index, layer) in map.layers.iter().enumerate() {
            if !self.preview_layers.get(index).copied().unwrap_or(false) {
                continue;
            }
            for sprite in &layer.sprites {
                if sprite.visible {
                    paint_sprite(
                        &map_painter,
                        sprite,
                        layer.opacity,
                        &self.textures,
                        &to_screen,
                    );
                }
            }
        }

        let camera_center = [world_width * 0.5, world_height * 0.5];
        let camera_rect = world_rect(
            camera_center,
            [CAMERA_WIDTH_WU, CAMERA_HEIGHT_WU],
            &to_screen,
        );
        ui.painter().rect_filled(
            camera_rect.intersect(canvas),
            0.0,
            Color32::from_rgba_unmultiplied(60, 160, 255, 22),
        );
        ui.painter().rect_stroke(
            camera_rect,
            0.0,
            Stroke::new(2.0, Color32::from_rgb(80, 180, 255)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            camera_rect.left_top() + Vec2::new(5.0, 5.0),
            egui::Align2::LEFT_TOP,
            format!("GAME CAMERA {:.3} × 13 wu", CAMERA_WIDTH_WU),
            egui::FontId::monospace(12.0),
            Color32::from_rgb(120, 205, 255),
        );

        let player_center = [world_width * 0.5, PLAYER_REFERENCE_SIZE_WU[1] * 0.5];
        let player_rect = world_rect(player_center, PLAYER_REFERENCE_SIZE_WU, &to_screen);
        ui.painter().rect_filled(
            player_rect,
            3.0,
            Color32::from_rgba_unmultiplied(255, 220, 100, 150),
        );
        ui.painter().rect_stroke(
            player_rect,
            3.0,
            Stroke::new(1.5, Color32::from_rgb(255, 230, 120)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            player_rect.center_top() - Vec2::new(0.0, 4.0),
            egui::Align2::CENTER_BOTTOM,
            "PLAYER SCALE",
            egui::FontId::monospace(11.0),
            Color32::from_rgb(255, 230, 120),
        );
        ui.painter().rect_stroke(
            map_rect,
            0.0,
            Stroke::new(2.0, Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }
}

fn load_textures(
    ctx: &egui::Context,
    document: &MapLabDocument,
) -> Result<HashMap<String, TextureHandle>, String> {
    let graphic = find_graphic_root(&document.sidecar_path)?;
    let mut textures = HashMap::new();
    for asset in &document.presentation.assets {
        let path = graphic.join(&asset.source_path);
        let image = image::open(&path)
            .map_err(|error| format!("decode {}: {error}", path.display()))?
            .to_rgba8();
        if [image.width(), image.height()] != asset.image_size_px {
            return Err(format!(
                "{} dimensions changed since compile",
                path.display()
            ));
        }
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        );
        textures.insert(
            asset.id.clone(),
            ctx.load_texture(&asset.id, color, egui::TextureOptions::NEAREST),
        );
    }
    Ok(textures)
}

fn find_graphic_root(sidecar: &Path) -> Result<PathBuf, String> {
    sidecar
        .ancestors()
        .map(|ancestor| ancestor.join("Graphic"))
        .find(|path| path.is_dir())
        .ok_or_else(|| format!("cannot locate Graphic/ above {}", sidecar.display()))
}

fn paint_sprite(
    painter: &egui::Painter,
    sprite: &PresentationSprite,
    layer_opacity: f32,
    textures: &HashMap<String, TextureHandle>,
    to_screen: &impl Fn([f32; 2]) -> Pos2,
) {
    let Some(texture) = textures.get(&sprite.asset_id) else {
        return;
    };
    let [w, h] = sprite.size_world;
    let [cx, cy] = sprite.position_world;
    let positions = [
        to_screen([cx - w * 0.5, cy - h * 0.5]),
        to_screen([cx + w * 0.5, cy - h * 0.5]),
        to_screen([cx + w * 0.5, cy + h * 0.5]),
        to_screen([cx - w * 0.5, cy + h * 0.5]),
    ];
    let [x, y, width, height] = sprite.source_rect_px;
    let [image_width, image_height] = [texture.size()[0] as f32, texture.size()[1] as f32];
    let u0 = x as f32 / image_width;
    let v0 = y as f32 / image_height;
    let u1 = (x + width) as f32 / image_width;
    let v1 = (y + height) as f32 / image_height;
    let uv = transformed_uv(sprite.transform)
        .map(|[u, v]| Pos2::new(u0 + (u1 - u0) * u, v0 + (v1 - v0) * v));
    let alpha = (layer_opacity * sprite.opacity * 255.0).round() as u8;
    let mut mesh = egui::Mesh::with_texture(texture.id());
    for index in 0..4 {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: positions[index],
            uv: uv[index],
            color: Color32::from_white_alpha(alpha),
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(mesh);
}

fn transformed_uv(transform: TileTransform) -> [[f32; 2]; 4] {
    [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]].map(|mut uv| {
        if transform.flip_diagonal {
            uv.swap(0, 1);
        }
        if transform.flip_horizontal {
            uv[0] = 1.0 - uv[0];
        }
        if transform.flip_vertical {
            uv[1] = 1.0 - uv[1];
        }
        uv
    })
}

fn world_rect(center: [f32; 2], size: [f32; 2], to_screen: &impl Fn([f32; 2]) -> Pos2) -> Rect {
    Rect::from_two_pos(
        to_screen([center[0] - size[0] * 0.5, center[1] + size[1] * 0.5]),
        to_screen([center[0] + size[0] * 0.5, center[1] - size[1] * 0.5]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_and_player_references_use_repo_owned_scale() {
        assert!((CAMERA_WIDTH_WU - 23.111_11).abs() < 1e-4);
        assert_eq!(CAMERA_HEIGHT_WU, 13.0);
        assert_eq!(PLAYER_REFERENCE_SIZE_WU, [0.92, 1.38]);
    }

    #[test]
    fn diagonal_transform_precedes_horizontal_and_vertical() {
        let transformed = transformed_uv(TileTransform {
            flip_horizontal: true,
            flip_vertical: false,
            flip_diagonal: true,
        });
        assert_eq!(transformed[0], [0.0, 0.0]);
        assert_eq!(transformed[2], [1.0, 1.0]);
    }
}
