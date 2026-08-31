//! In-window egui overlay. Not a second OS window.

use std::time::{Duration, Instant};

use egui::{Context, ViewportId};
use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor};
use egui_winit::State;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

use purgatory_simulation::DebugAction;

use super::aoi_view::label_ndc;
use super::collision_history::CollisionHistory;
use super::entity_inspector::{
    InspectorAccent, InspectorCategory, InspectorRow, InspectorSource, InspectorView,
};
use super::interact_status::{format_portal_line, format_target_line, interact_kind_color};
use super::sections::{debug_section, draw_expand_collapse};
use super::snapshot::DebugSnapshot;
use super::ui_state::DebugUiState;
use crate::renderer::OverlayPass;

const SEC_RT_FRAME: &str = "runtime.frame";
const SEC_RT_GIZMOS: &str = "runtime.gizmos";
const SEC_PL_POSE: &str = "player.pose";
const SEC_PL_MOTION: &str = "player.motion";
const SEC_FN_CONTACT: &str = "footnote.contact";
const SEC_FN_POSE: &str = "footnote.pose";
const SEC_FN_INPUT: &str = "footnote.input";
const SEC_WD_STAGE: &str = "world.stage";
const SEC_WD_VIEW: &str = "world.view";
const SEC_WD_ENTITIES: &str = "world.entities";
const SEC_CM_TRANSFORM: &str = "camera.transform";
const SEC_CM_FOLLOW: &str = "camera.follow";
const SEC_CM_JITTER: &str = "camera.jitter";
const SEC_CM_PARALLAX: &str = "camera.parallax";
const SEC_DG_DETECTORS: &str = "diagnostics.detectors";
const SEC_DG_LAST: &str = "diagnostics.last";
const SEC_DG_HISTORY: &str = "diagnostics.history";
const SEC_NET_CONN: &str = "network.connection";
const SEC_NET_AOI: &str = "network.aoi";
const SEC_NET_INTERACT: &str = "network.interact";
const SEC_NET_INPUT: &str = "network.input";
const SEC_NET_REPLICA: &str = "network.replica";
const SEC_NET_INTERP: &str = "network.interp";
const SEC_NET_PRED: &str = "network.prediction";
const SEC_NET_IMPAIR: &str = "network.impairment";
const SEC_NET_FAIL: &str = "network.failure";
const SEC_NET_HIST: &str = "network.history";

/// Arguments for drawing the Connection Frontend during an egui frame.
pub struct ConnectionPaint<'a> {
    pub frontend: &'a crate::frontend::ConnectionFrontend,
    pub server: &'a str,
    pub login: &'a mut String,
    pub status: &'a str,
    pub can_connect: bool,
}

/// GPU handles needed to construct the overlay. wgpu types only; no egui in `gpu.rs`.
pub struct OverlayInit<'a> {
    pub device: &'a wgpu::Device,
    pub surface_format: wgpu::TextureFormat,
    pub max_texture_side: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DebugTab {
    Runtime,
    Player,
    Footnote,
    World,
    Camera,
    Diagnostics,
    Network,
}

/// Client-owned development overlay.
pub struct DebugOverlay {
    visible: bool,
    ctx: Context,
    winit: State,
    renderer: EguiRenderer,
    tab: DebugTab,
    pub ui: DebugUiState,
    pub collision_history: CollisionHistory,
}

impl DebugOverlay {
    pub fn new(window: &Window, pass: OverlayInit<'_>) -> Self {
        let ctx = Context::default();
        ctx.set_embed_viewports(true);
        let winit = State::new(
            ctx.clone(),
            ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            Some(pass.max_texture_side),
        );
        let renderer =
            EguiRenderer::new(pass.device, pass.surface_format, RendererOptions::default());
        Self {
            visible: false,
            ctx,
            winit,
            renderer,
            tab: DebugTab::Runtime,
            ui: DebugUiState::from_env(),
            collision_history: CollisionHistory::default(),
        }
    }

    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    #[must_use]
    pub fn wants_keyboard_for_text(&self) -> bool {
        self.visible && self.ctx.text_edit_focused()
    }

    #[must_use]
    pub fn context(&self) -> &Context {
        &self.ctx
    }

    #[must_use]
    pub fn wants_pointer(&self) -> bool {
        self.ctx.egui_wants_pointer_input()
    }

    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) {
        let response = self.winit.on_window_event(window, event);
        if response.repaint {
            window.request_redraw();
        }
    }

    /// Debug overlay widgets only. Not the Connection Frontend.
    fn paint(
        ctx: &Context,
        snapshot: &DebugSnapshot,
        visible: &mut bool,
        tab: &mut DebugTab,
        ui_state: &mut DebugUiState,
        history: &mut CollisionHistory,
        actions: &mut Vec<DebugAction>,
    ) {
        draw_debug_window(ctx, snapshot, visible, tab, ui_state, history, actions);
    }

    /// One egui frame: optional [`ConnectionFrontend::paint`] plus debug overlay.
    pub fn submit_frame(
        &mut self,
        window: &Window,
        pass: OverlayPass<'_>,
        snapshot: &DebugSnapshot,
        connection: Option<ConnectionPaint<'_>>,
    ) -> (Vec<wgpu::CommandBuffer>, Vec<DebugAction>, bool) {
        if connection.is_none() && !self.visible {
            return (Vec::new(), Vec::new(), false);
        }

        let raw_input = self.winit.take_egui_input(window);
        let mut actions = Vec::new();
        let mut connect_clicked = false;
        let mut visible = self.visible;
        let mut tab = self.tab;
        let mut ui_state = self.ui.clone();
        let history = &mut self.collision_history;
        let mut connection = connection;
        let mut full_output = self.ctx.run_ui(raw_input, |egui_ctx| {
            if let Some(paint) = connection.as_mut() {
                connect_clicked = paint.frontend.paint(
                    egui_ctx,
                    paint.server,
                    paint.login,
                    paint.status,
                    paint.can_connect,
                );
            }
            if visible {
                Self::paint(
                    egui_ctx,
                    snapshot,
                    &mut visible,
                    &mut tab,
                    &mut ui_state,
                    history,
                    &mut actions,
                );
                if ui_state.show_entity_labels {
                    draw_world_entity_labels(egui_ctx, snapshot);
                }
            }
        });
        self.visible = visible;
        self.tab = tab;
        self.ui = ui_state;
        self.winit
            .handle_platform_output(window, full_output.platform_output);

        for (id, image_deltas) in full_output.textures_delta.set.drain() {
            for image_delta in image_deltas {
                self.renderer
                    .update_texture(pass.device, pass.queue, id, &image_delta);
            }
        }

        let primitives = self
            .ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen = ScreenDescriptor {
            size_in_pixels: [pass.width, pass.height],
            pixels_per_point: full_output.pixels_per_point,
        };
        let extra = self.renderer.update_buffers(
            pass.device,
            pass.queue,
            pass.encoder,
            &primitives,
            &screen,
        );

        {
            let render_pass = pass.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("purgatory-debug-egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: pass.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.renderer
                .render(&mut render_pass.forget_lifetime(), &primitives, &screen);
        }

        for id in full_output.textures_delta.free.drain() {
            self.renderer.free_texture(&id);
        }

        (extra, actions, connect_clicked)
    }
}

#[must_use]
pub fn is_debug_toggle(event: &KeyEvent) -> bool {
    is_debug_toggle_key(
        event.physical_key,
        event.state == ElementState::Pressed,
        event.repeat,
    )
}

#[must_use]
pub fn is_debug_toggle_key(physical_key: PhysicalKey, pressed: bool, repeat: bool) -> bool {
    pressed && !repeat && matches!(physical_key, PhysicalKey::Code(KeyCode::Backquote))
}

fn tab_section_ids(tab: DebugTab) -> &'static [&'static str] {
    match tab {
        DebugTab::Runtime => &[SEC_RT_FRAME, SEC_RT_GIZMOS],
        DebugTab::Player => &[SEC_PL_POSE, SEC_PL_MOTION],
        DebugTab::Footnote => &[SEC_FN_CONTACT, SEC_FN_POSE, SEC_FN_INPUT],
        DebugTab::World => &[SEC_WD_STAGE, SEC_WD_VIEW, SEC_WD_ENTITIES],
        DebugTab::Camera => &[
            SEC_CM_TRANSFORM,
            SEC_CM_FOLLOW,
            SEC_CM_JITTER,
            SEC_CM_PARALLAX,
        ],
        DebugTab::Diagnostics => &[SEC_DG_DETECTORS, SEC_DG_LAST, SEC_DG_HISTORY],
        DebugTab::Network => &[
            SEC_NET_CONN,
            SEC_NET_AOI,
            SEC_NET_INTERACT,
            SEC_NET_INPUT,
            SEC_NET_REPLICA,
            SEC_NET_INTERP,
            SEC_NET_PRED,
            SEC_NET_IMPAIR,
            SEC_NET_FAIL,
            SEC_NET_HIST,
        ],
    }
}

fn format_rtt_ms(d: Option<std::time::Duration>) -> String {
    d.map(|rtt| format!("{:.2} ms", rtt.as_secs_f64() * 1000.0))
        .unwrap_or_else(|| "—".into())
}

fn format_imp_ms(ns: u64) -> String {
    format!("{} ms", purgatory_common::impairment::ns_to_ms(ns))
}

fn fmt_opt_u16(value: Option<u16>) -> String {
    value.map(|n| n.to_string()).unwrap_or_else(|| "—".into())
}

fn network_section_default_open(id: &'static str) -> bool {
    matches!(
        id,
        SEC_NET_AOI
            | SEC_NET_INTERACT
            | SEC_NET_INTERP
            | SEC_NET_PRED
            | SEC_NET_IMPAIR
            | SEC_NET_REPLICA
    )
}

const INTERACT_FLASH_SECS: f32 = 0.55;

fn note_interact_flash(ui_state: &mut DebugUiState, snapshot: &DebugSnapshot) {
    let kind = snapshot.interact_status.kind;
    if ui_state.last_interact_kind != Some(kind) {
        if let Some(text) = &snapshot.interact_status.last_transition {
            ui_state.interact_flash_text = text.clone();
            ui_state.interact_flash_until =
                Some(Instant::now() + Duration::from_secs_f32(INTERACT_FLASH_SECS));
        }
        ui_state.last_interact_kind = Some(kind);
    }
}

fn draw_status_chip(ui: &mut egui::Ui, text: &str, rgb: (u8, u8, u8), emphasize: bool) {
    let fill = if emphasize {
        egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2)
    } else {
        egui::Color32::from_rgba_unmultiplied(rgb.0, rgb.1, rgb.2, 40)
    };
    let stroke = egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2);
    let text_color = if emphasize {
        egui::Color32::from_rgb(12, 12, 14)
    } else {
        egui::Color32::from_rgb(245, 245, 248)
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.5, stroke))
        .inner_margin(egui::Margin::symmetric(6, 2))
        .corner_radius(3)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).color(text_color).strong());
        });
}

fn draw_interact_status_strip(
    ui: &mut egui::Ui,
    snapshot: &DebugSnapshot,
    ui_state: &mut DebugUiState,
) {
    note_interact_flash(ui_state, snapshot);
    let status = &snapshot.interact_status;
    let rgb = interact_kind_color(status.kind);
    let flash_live = ui_state
        .interact_flash_until
        .is_some_and(|until| Instant::now() < until);
    ui.horizontal_wrapped(|ui| {
        ui.strong("INTERACTION");
        draw_status_chip(ui, &status.interaction_line(), rgb, flash_live);
        if flash_live && !ui_state.interact_flash_text.is_empty() {
            draw_status_chip(ui, &ui_state.interact_flash_text, (255, 230, 90), true);
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.strong("TARGET");
        ui.label(format_target_line(
            snapshot.interact_nearest.as_deref(),
            snapshot.interact_nearest_distance,
        ));
        ui.separator();
        ui.strong("PORTAL");
        ui.label(format_portal_line(
            snapshot.interact_nearest_portal.as_deref(),
            snapshot.portal_eligible,
        ));
    });
}

fn note_channel_flash(ui_state: &mut DebugUiState, snapshot: &DebugSnapshot) {
    let current = snapshot.observer_channel;
    let epoch = snapshot.replica_epoch;
    if ui_state.last_observed_channel != Some(current) {
        if let Some(prev) = ui_state.last_observed_channel {
            ui_state.channel_flash_from = Some(prev);
            ui_state.channel_flash_to = Some(current);
            ui_state.channel_flash_epoch_from = ui_state.last_observed_epoch;
            ui_state.channel_flash_epoch_to = Some(epoch);
            ui_state.channel_flash_until = Some(Instant::now() + Duration::from_secs(3));
        }
        ui_state.last_observed_channel = Some(current);
    }
    ui_state.last_observed_epoch = Some(epoch);
}

fn draw_world_address_strip(
    ui: &mut egui::Ui,
    snapshot: &DebugSnapshot,
    ui_state: &mut DebugUiState,
) {
    note_channel_flash(ui_state, snapshot);
    ui.horizontal_wrapped(|ui| {
        ui.strong("WORLD");
        ui.label(format!(
            "Map: {}   Channel: {}   Instance: {}",
            snapshot.observer_map, snapshot.observer_channel, snapshot.observer_instance
        ));
    });
    ui.horizontal_wrapped(|ui| {
        ui.strong("Channel:");
        for ch in 0..=purgatory_protocol::DEV_CHANNEL_MAX {
            let selected = snapshot.observer_channel == ch;
            if ui
                .selectable_label(selected, format!("[{ch}]"))
                .on_hover_text("DEV: request server-authoritative Channel change")
                .clicked()
            {
                ui_state.request_channel = Some(ch);
            }
        }
        ui.separator();
        ui.label(format!(
            "map={}  ch={}  inst={}",
            snapshot.observer_map, snapshot.observer_channel, snapshot.observer_instance
        ));
    });
    if !snapshot.transition_banner.is_empty() {
        let rgb = if snapshot.transition_stalled {
            (255, 90, 90)
        } else {
            (90, 200, 255)
        };
        draw_status_chip(
            ui,
            &snapshot.transition_banner,
            rgb,
            snapshot.transition_stalled,
        );
        if !snapshot.transition_missing.is_empty() {
            ui.small(format!("waiting: {}", snapshot.transition_missing));
        }
    }
    let locked = snapshot.input_movement_neutral;
    draw_status_chip(
        ui,
        &snapshot.input_gate_label,
        if locked {
            (255, 180, 80)
        } else {
            (140, 200, 140)
        },
        locked,
    );
    if locked {
        ui.small("authoritative movement neutral: yes");
    }
    let flash_live = ui_state
        .channel_flash_until
        .is_some_and(|until| Instant::now() < until);
    if flash_live
        && let (Some(from), Some(to)) = (ui_state.channel_flash_from, ui_state.channel_flash_to)
    {
        let epoch = match (
            ui_state.channel_flash_epoch_from,
            ui_state.channel_flash_epoch_to,
        ) {
            (Some(e0), Some(e1)) => format!(" epoch {e0} → {e1}"),
            _ => String::new(),
        };
        draw_status_chip(
            ui,
            &format!("CHANNEL TRANSITION  {from} → {to}{epoch}"),
            (90, 200, 255),
            true,
        );
    }
}

fn draw_debug_window(
    ctx: &Context,
    snapshot: &DebugSnapshot,
    visible: &mut bool,
    tab: &mut DebugTab,
    ui_state: &mut DebugUiState,
    history: &mut CollisionHistory,
    actions: &mut Vec<DebugAction>,
) {
    egui::Window::new("PURGATORY DEBUG")
        .open(visible)
        .resizable(true)
        .constrain(true)
        .default_pos([12.0, 12.0])
        .default_width(360.0)
        .show(ctx, |ui| {
            if (ui_state.time_scale - 1.0).abs() > 1e-4 {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 180, 60),
                    format!("DEV TIME SCALE: {}", ui_state.time_scale_label()),
                );
                ui.separator();
            }

            draw_interact_status_strip(ui, snapshot, ui_state);
            draw_world_address_strip(ui, snapshot, ui_state);

            ui.horizontal(|ui| {
                for (label, value) in [
                    ("Runtime", DebugTab::Runtime),
                    ("Player", DebugTab::Player),
                    ("FOOTNOTE", DebugTab::Footnote),
                    ("World", DebugTab::World),
                    ("Camera", DebugTab::Camera),
                    ("Diagnostics", DebugTab::Diagnostics),
                    ("Network", DebugTab::Network),
                ] {
                    if ui.selectable_label(*tab == value, label).clicked() {
                        *tab = value;
                    }
                }
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt(format!("debug-tab-{:?}", *tab))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    draw_expand_collapse(ui, &mut ui_state.sections, tab_section_ids(*tab));
                    match *tab {
                        DebugTab::Runtime => draw_runtime_tab(ui, snapshot, ui_state),
                        DebugTab::Player => draw_player_tab(ui, snapshot, ui_state, actions),
                        DebugTab::Footnote => draw_footnote_tab(ui, snapshot, ui_state),
                        DebugTab::World => draw_world_tab(ui, snapshot, ui_state),
                        DebugTab::Camera => draw_camera_tab(ui, snapshot, ui_state),
                        DebugTab::Diagnostics => {
                            draw_diagnostics_tab(ui, snapshot, ui_state, history)
                        }
                        DebugTab::Network => draw_network_tab(ui, snapshot, ui_state),
                    }
                });
        });
}

fn draw_runtime_tab(ui: &mut egui::Ui, snapshot: &DebugSnapshot, ui_state: &mut DebugUiState) {
    let frame_summary = format!("FPS {:.1} | tick {}", snapshot.fps, snapshot.tick);
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_RT_FRAME,
        true,
        "Frame / Clock",
        Some(&frame_summary),
    ) {
        ui.indent(SEC_RT_FRAME, |ui| {
            ui.label(format!("Frames: {}", snapshot.frames));
            ui.label(format!("Tick: {}", snapshot.tick));
            ui.label(format!("Sim rate: {} Hz (fixed)", snapshot.tick_rate_hz));
            ui.label(format!(
                "Window: {}x{}",
                snapshot.window_width, snapshot.window_height
            ));
            ui.label(format!("FPS: {:.1}", snapshot.fps));
            ui.label(format!("Time scale: {:.2}", ui_state.time_scale));
            ui.horizontal(|ui| {
                ui.label("Simulation Speed:");
                for scale in DebugUiState::TIME_SCALES {
                    let label = if (scale - 1.0).abs() < 1e-4 {
                        "1.0x"
                    } else if (scale - 0.5).abs() < 1e-4 {
                        "0.5x"
                    } else {
                        "0.25x"
                    };
                    if ui
                        .selectable_label((ui_state.time_scale - scale).abs() < 1e-4, label)
                        .clicked()
                    {
                        ui_state.time_scale = scale;
                    }
                }
            });
            ui.small("Scales wall elapsed into SimulationClock only. Tick rate stays 30 Hz.");
        });
    }
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_RT_GIZMOS,
        true,
        "Gizmo toggles",
        None,
    ) {
        ui.indent(SEC_RT_GIZMOS, |ui| {
            ui.checkbox(&mut ui_state.show_colliders, "Show Colliders");
            ui.checkbox(&mut ui_state.show_velocity, "Show Velocity Vector");
            ui.checkbox(
                &mut ui_state.show_grounded_highlight,
                "Highlight Grounded Platform",
            );
            ui.checkbox(&mut ui_state.show_world_bounds, "Show World Bounds");
            ui.checkbox(&mut ui_state.show_grid, "Show Grid");
            ui.checkbox(&mut ui_state.show_parallax_debug, "Show Parallax Debug");
            ui.checkbox(
                &mut ui_state.show_interpolation_gizmos,
                "Show Interpolation Gizmos",
            );
            ui.checkbox(
                &mut ui_state.show_prediction_gizmos,
                "Show Prediction Gizmos",
            );
            ui.checkbox(&mut ui_state.show_aoi_rects, "Show AOI Policy Rects");
            ui.checkbox(&mut ui_state.show_camera_deadzone, "Show Camera Dead Zone");
            ui.checkbox(&mut ui_state.show_entity_labels, "Show entity labels");
        });
    }
}

fn draw_outlined_debug_label(ui: &mut egui::Ui, text: &str, fill: egui::Color32) {
    let font = egui::FontId::proportional(16.0);
    let outline = egui::Color32::from_rgb(0, 0, 0);
    let shadow = egui::Color32::from_black_alpha(200);
    let galley_size = ui.ctx().fonts_mut(|f| {
        f.layout(text.to_string(), font.clone(), fill, f32::INFINITY)
            .size()
    });
    let pad = egui::vec2(7.0, 3.0);
    let (rect, _) = ui.allocate_exact_size(galley_size + pad * 2.0, egui::Sense::hover());
    let origin = rect.min + pad;
    let painter = ui.painter();
    let offsets = [
        [-2.0, 0.0],
        [2.0, 0.0],
        [0.0, -2.0],
        [0.0, 2.0],
        [-2.0, -2.0],
        [2.0, -2.0],
        [-2.0, 2.0],
        [2.0, 2.0],
        [-1.0, -2.0],
        [1.0, -2.0],
        [-1.0, 2.0],
        [1.0, 2.0],
    ];
    for [dx, dy] in offsets {
        painter.text(
            origin + egui::vec2(dx, dy),
            egui::Align2::LEFT_TOP,
            text,
            font.clone(),
            outline,
        );
    }
    painter.text(
        origin + egui::vec2(1.5, 2.0),
        egui::Align2::LEFT_TOP,
        text,
        font.clone(),
        shadow,
    );
    painter.text(origin, egui::Align2::LEFT_TOP, text, font, fill);
}

fn draw_world_entity_labels(ctx: &Context, snapshot: &DebugSnapshot) {
    // `replica_label_world` is empty unless maps are aligned and the
    // presentation world is ready (idle or DestinationReady). Do not project
    // leftover replica poses through a mismatched camera.
    if snapshot.replica_label_world.is_empty() {
        return;
    }
    let rect = ctx.viewport_rect();
    let vw = snapshot.viewport_width;
    let vh = snapshot.viewport_height;
    if vw <= 0.0 || vh <= 0.0 {
        return;
    }
    for (i, (text, world, tag)) in snapshot.replica_label_world.iter().enumerate() {
        let ndc = label_ndc(*world, snapshot.camera_position, [vw, vh]);
        if ndc[0].abs() > 1.25 || ndc[1].abs() > 1.25 {
            continue;
        }
        let x = (ndc[0] * 0.5 + 0.5) * rect.width() + rect.min.x;
        let y = (0.5 - ndc[1] * 0.5) * rect.height() + rect.min.y - 30.0;
        let accent = match tag {
            0 => egui::Color32::from_rgb(80, 220, 255),
            1 => egui::Color32::from_rgb(255, 95, 115),
            2 => egui::Color32::from_rgb(255, 160, 50),
            _ => egui::Color32::from_rgb(70, 235, 185),
        };
        egui::Area::new(egui::Id::new(("aoi_entity_label", i)))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(x, y))
            .interactable(false)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_unmultiplied(6, 8, 12, 235))
                    .stroke(egui::Stroke::new(2.0, accent))
                    .inner_margin(egui::Margin::symmetric(5, 2))
                    .corner_radius(3)
                    .show(ui, |ui| {
                        draw_outlined_debug_label(ui, text, egui::Color32::from_rgb(255, 255, 255));
                    });
            });
    }
}

fn inspector_accent_color(accent: InspectorAccent) -> egui::Color32 {
    match accent {
        InspectorAccent::LocalPlayer => egui::Color32::from_rgb(80, 220, 255),
        InspectorAccent::RemotePlayer => egui::Color32::from_rgb(255, 95, 115),
        InspectorAccent::Interactable => egui::Color32::from_rgb(255, 160, 50),
        InspectorAccent::Portal => egui::Color32::from_rgb(70, 235, 185),
        InspectorAccent::Platform => egui::Color32::from_rgb(150, 155, 165),
        InspectorAccent::Other => egui::Color32::from_rgb(180, 175, 160),
    }
}

fn draw_inspector_row(ui: &mut egui::Ui, row: &InspectorRow) {
    ui.horizontal(|ui| {
        ui.colored_label(inspector_accent_color(row.accent), "●");
        ui.strong(&row.title);
        match row.source {
            InspectorSource::World => {
                ui.small("World");
            }
            InspectorSource::Replica => {
                ui.small("Replication");
            }
            InspectorSource::LocalPlayer => {}
        }
    });
    ui.indent(
        format!(
            "insp:{}:{}:{}:{:?}",
            row.title, row.sort_index, row.sort_generation, row.source
        ),
        |ui| {
            if row.source == InspectorSource::LocalPlayer {
                ui.label(format!(
                    "server RuntimeEntityId: {}",
                    row.server_runtime_entity_id.as_deref().unwrap_or("—")
                ));
                ui.label(format!(
                    "client World EntityId: {}",
                    row.client_world_entity_id.as_deref().unwrap_or("—")
                ));
            }
            ui.label(format!("World: {}", row.world_line));
            ui.label(format!("Replication: {}", row.replication_line));
        },
    );
}

fn draw_inspector_rows(ui: &mut egui::Ui, rows: &[InspectorRow]) {
    if rows.is_empty() {
        ui.label("—");
        return;
    }
    for row in rows {
        draw_inspector_row(ui, row);
    }
}

fn draw_inspector_category(
    ui: &mut egui::Ui,
    sections: &mut super::sections::DebugSectionMap,
    view: &InspectorView,
    category: InspectorCategory,
) {
    let id = InspectorView::category_section_id(category);
    let title = match category {
        InspectorCategory::Players => "Players",
        InspectorCategory::Interactables => "Interactables",
        InspectorCategory::Portals => "Portals",
        InspectorCategory::Platforms => "Platforms",
        InspectorCategory::Other => "Other",
    };
    let summary = view.category_summary(category);
    if !debug_section(
        ui,
        sections,
        id,
        InspectorView::category_default_open(category),
        title,
        Some(&summary),
    ) {
        return;
    }
    ui.indent(id, |ui| {
        if category == InspectorCategory::Players {
            let local: Vec<&InspectorRow> = view
                .players
                .iter()
                .filter(|r| r.accent == InspectorAccent::LocalPlayer)
                .collect();
            let remote: Vec<&InspectorRow> = view
                .players
                .iter()
                .filter(|r| r.accent != InspectorAccent::LocalPlayer)
                .collect();
            let local_summary = if local.len() == 1 {
                format!(
                    "server RuntimeEntityId {} · client World EntityId {}",
                    local[0].server_runtime_entity_id.as_deref().unwrap_or("—"),
                    local[0].client_world_entity_id.as_deref().unwrap_or("—"),
                )
            } else {
                local.len().to_string()
            };
            if debug_section(
                ui,
                sections,
                InspectorView::players_local_section_id(),
                true,
                "Local Player",
                Some(&local_summary),
            ) {
                ui.indent(InspectorView::players_local_section_id(), |ui| {
                    if local.is_empty() {
                        ui.label("—");
                    } else {
                        for row in local {
                            draw_inspector_row(ui, row);
                        }
                    }
                });
            }
            if debug_section(
                ui,
                sections,
                InspectorView::players_remote_section_id(),
                true,
                "Remote Players",
                Some(&remote.len().to_string()),
            ) {
                ui.indent(InspectorView::players_remote_section_id(), |ui| {
                    if remote.is_empty() {
                        ui.label("—");
                    } else {
                        for row in remote {
                            draw_inspector_row(ui, row);
                        }
                    }
                });
            }
        } else {
            draw_inspector_rows(ui, view.rows(category));
        }
    });
}

fn draw_player_tab(
    ui: &mut egui::Ui,
    snapshot: &DebugSnapshot,
    ui_state: &mut DebugUiState,
    actions: &mut Vec<DebugAction>,
) {
    ui.colored_label(
        egui::Color32::from_rgb(220, 160, 60),
        "LOCAL DEV / NON-AUTHORITATIVE",
    );
    ui.small("Client World FOOTNOTE drives local prediction + Phase 5.5 replay (tick_player). Server owns authority.");
    let pose_summary = snapshot.player.map(|player| {
        format!(
            "({:.2}, {:.2}) {}",
            player.position[0],
            player.position[1],
            if player.grounded { "grounded" } else { "air" }
        )
    });
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_PL_POSE,
        true,
        "Pose",
        pose_summary.as_deref(),
    ) {
        ui.indent(SEC_PL_POSE, |ui| {
            if let Some(player) = snapshot.player {
                ui.label(format!("Entity: {}", player.id));
                ui.label(format!("WorldAddress: {}", player.address));
                ui.label(format!("Lifecycle: {}", player.lifecycle));
                ui.label(format!(
                    "ContentId: {}",
                    player
                        .content_id
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "none".into())
                ));
                ui.label(format!(
                    "PersistentId: {}",
                    if player.has_persistent_id {
                        "present"
                    } else {
                        "none"
                    }
                ));
                ui.label(format!("Replication: {}", player.replication));
                ui.label(format!(
                    "Position: X {:.3} | Y {:.3}",
                    player.position[0], player.position[1]
                ));
                ui.label(format!(
                    "Velocity: X {:.3} | Y {:.3}",
                    player.velocity[0], player.velocity[1]
                ));
                ui.label(format!("Grounded: {}", player.grounded));
                match player.grounded_on {
                    Some(id) => ui.label(format!("Grounded on: {id}")),
                    None => ui.label("Grounded on: none"),
                };
                match player.grounded_kind {
                    Some(kind) => ui.label(format!("Grounded kind: {kind:?}")),
                    None => ui.label("Grounded kind: —"),
                };
                if let Some(fn_dbg) = snapshot.footnote {
                    ui.label(format!("Input X: {}", fn_dbg.move_axis));
                    ui.label(format!("Down held: {}", fn_dbg.down_held));
                }
                if ui.button("Reset Player").clicked() {
                    actions.push(DebugAction::ResetPlayer);
                }
                ui.small(
                    "DEV: when connected, snaps local prediction to the authoritative replica (not FOOTNOTE spawn). Offline: local World spawn reset. Never mutates ReplicatedWorld or the server.",
                );
            } else {
                ui.label("Entity: none");
            }
        });
    }
    let m = snapshot.motion;
    let motion_summary = format!("disc {} | {:?}", m.discontinuity, m.response_kind);
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_PL_MOTION,
        true,
        "Motion (last sim tick)",
        Some(&motion_summary),
    ) {
        ui.indent(SEC_PL_MOTION, |ui| {
            ui.label(format!(
                "Prev: ({:.3}, {:.3})",
                m.previous_position[0], m.previous_position[1]
            ));
            ui.label(format!(
                "Delta: ({:.3}, {:.3})  |Δ| {:.3}  max {:.3}",
                m.delta[0],
                m.delta[1],
                m.delta_length(),
                m.expected_max_delta
            ));
            ui.label(format!("Discontinuity: {}", m.discontinuity));
            ui.label(format!(
                "Correction: ({:.3}, {:.3})  axis {:?}  kind {:?}",
                m.correction[0], m.correction[1], m.correction_axis, m.response_kind
            ));
            match m.collision_candidate {
                Some(id) => ui.label(format!("Collision candidate: {id}")),
                None => ui.label("Collision candidate: none"),
            };
        });
    }
}

fn draw_footnote_tab(ui: &mut egui::Ui, snapshot: &DebugSnapshot, ui_state: &mut DebugUiState) {
    if let Some(fn_dbg) = snapshot.footnote {
        let state = if fn_dbg.grounded {
            "Grounded"
        } else {
            "Airborne"
        };
        if debug_section(
            ui,
            &mut ui_state.sections,
            SEC_FN_CONTACT,
            true,
            "Contact",
            Some(state),
        ) {
            ui.indent(SEC_FN_CONTACT, |ui| {
                ui.label(format!("State: {state}"));
                match fn_dbg.grounded_on {
                    Some(id) => ui.label(format!("Platform: {id}")),
                    None => ui.label("Platform: none"),
                };
                match fn_dbg.platform_kind {
                    Some(kind) => ui.label(format!("Platform kind: {kind:?}")),
                    None => ui.label("Platform kind: —"),
                };
                ui.label(format!("Last contact: {:?}", fn_dbg.last_contact));
            });
        }
        let pose_summary = format!(
            "({:.2}, {:.2}) |vx| {:.2}",
            fn_dbg.position[0], fn_dbg.position[1], fn_dbg.horizontal_speed
        );
        if debug_section(
            ui,
            &mut ui_state.sections,
            SEC_FN_POSE,
            true,
            "Pose",
            Some(&pose_summary),
        ) {
            ui.indent(SEC_FN_POSE, |ui| {
                ui.label(format!(
                    "Position: X {:.3} | Y {:.3}",
                    fn_dbg.position[0], fn_dbg.position[1]
                ));
                ui.label(format!(
                    "Velocity: ({:.3}, {:.3})",
                    fn_dbg.velocity[0], fn_dbg.velocity[1]
                ));
                ui.label(format!("|vx|: {:.3}", fn_dbg.horizontal_speed));
            });
        }
        let drop = match fn_dbg.ignored_platform {
            Some(_) => "drop-through",
            None => "idle",
        };
        let input_summary = format!("axis {} | {}", fn_dbg.move_axis, drop);
        if debug_section(
            ui,
            &mut ui_state.sections,
            SEC_FN_INPUT,
            true,
            "Input",
            Some(&input_summary),
        ) {
            ui.indent(SEC_FN_INPUT, |ui| {
                ui.label(format!("Input X: {}", fn_dbg.move_axis));
                ui.label(format!("Down held: {}", fn_dbg.down_held));
                match fn_dbg.ignored_platform {
                    Some(id) => ui.label(format!("Drop-through ignore: {id}")),
                    None => ui.label("Drop-through: inactive"),
                };
            });
        }
    } else {
        ui.label("No player.");
    }
}

fn draw_world_tab(ui: &mut egui::Ui, snapshot: &DebugSnapshot, ui_state: &mut DebugUiState) {
    let stage_summary = format!("{} | {} ents", snapshot.stage_name, snapshot.entity_count);
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_WD_STAGE,
        true,
        "Stage",
        Some(&stage_summary),
    ) {
        ui.indent(SEC_WD_STAGE, |ui| {
            ui.label(format!("Entities: {}", snapshot.entity_count));
            ui.label(format!("Players: {}", snapshot.player_count));
            ui.label(format!("Platforms: {}", snapshot.platform_count));
            ui.label(format!("Stage: {}", snapshot.stage_name));
            ui.label(format!("MapId: {}", snapshot.map_id));
            ui.label(format!("Map: {}", snapshot.map_debug_name));
            ui.label(format!("Channel: {}", snapshot.observer_channel));
            ui.label(format!("Instance: {}", snapshot.observer_instance));
            ui.label(format!("WorldAddress: {}", snapshot.observer_address));
            ui.label(format!(
                "Content registry: {} defs",
                snapshot.content_registry_count
            ));
            for label in &snapshot.content_map_labels {
                ui.label(format!("  {label}"));
            }
            let b = snapshot.world_bounds;
            ui.label(format!(
                "Bounds: X [{:.1}, {:.1}]  Y [{:.1}, {:.1}]",
                b.min_x, b.max_x, b.min_y, b.max_y
            ));
            ui.label(format!("Size: {:.1} × {:.1}", b.width(), b.height()));
        });
    }
    if debug_section(ui, &mut ui_state.sections, SEC_WD_VIEW, true, "View", None) {
        ui.indent(SEC_WD_VIEW, |ui| {
            ui.checkbox(&mut ui_state.show_world_bounds, "Show World Bounds");
            ui.checkbox(&mut ui_state.show_grid, "Show Grid");
        });
    }
    let entities_summary = snapshot.inspector.entities_header_summary();
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_WD_ENTITIES,
        true,
        "Entities",
        Some(&entities_summary),
    ) {
        ui.indent(SEC_WD_ENTITIES, |ui| {
            ui.small("Replica rows are observer Known-set. World rows are local client World slots. Identity namespaces are not merged.");
            ui.small("Spatial candidates / WantEnter are mailbox counts on Observer AOI, not per-entity rows.");
            for category in [
                InspectorCategory::Players,
                InspectorCategory::Interactables,
                InspectorCategory::Portals,
                InspectorCategory::Platforms,
                InspectorCategory::Other,
            ] {
                draw_inspector_category(ui, &mut ui_state.sections, &snapshot.inspector, category);
            }
        });
    }
}

fn draw_camera_tab(ui: &mut egui::Ui, snapshot: &DebugSnapshot, ui_state: &mut DebugUiState) {
    let cm = snapshot.camera_motion;
    let transform_summary = format!(
        "({:.2}, {:.2}) disc {}",
        snapshot.camera_position[0], snapshot.camera_position[1], cm.discontinuity
    );
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_CM_TRANSFORM,
        true,
        "Transform",
        Some(&transform_summary),
    ) {
        ui.indent(SEC_CM_TRANSFORM, |ui| {
            ui.label(format!(
                "Camera: X {:.3} | Y {:.3}",
                snapshot.camera_position[0], snapshot.camera_position[1]
            ));
            ui.label(format!(
                "Viewport: {:.2} × {:.2}",
                snapshot.viewport_width, snapshot.viewport_height
            ));
            let b = snapshot.world_bounds;
            ui.label(format!(
                "World bounds: [{:.1},{:.1}]×[{:.1},{:.1}]",
                b.min_x, b.max_x, b.min_y, b.max_y
            ));
            ui.label(format!(
                "Prev: ({:.3}, {:.3})",
                cm.previous_position[0], cm.previous_position[1]
            ));
            ui.label(format!(
                "Cam Δ: ({:.3}, {:.3})  disc {}",
                cm.delta[0], cm.delta[1], cm.discontinuity
            ));
            ui.label(format!("Clamp: {:?}", cm.clamp_reason));
            ui.label(format!(
                "Player (presentation): {}",
                snapshot
                    .presented_player_pos
                    .map(|p| format!("({:.3}, {:.3})", p[0], p[1]))
                    .unwrap_or_else(|| "-".into())
            ));
            ui.label(format!(
                "Desired target: ({:.3}, {:.3})",
                snapshot.camera_desired[0], snapshot.camera_desired[1]
            ));
            ui.label(format!(
                "Dead Zone half: X {:.2}  Y {:.2}",
                snapshot.camera_deadzone_half_x, snapshot.camera_deadzone_half_y
            ));
            ui.label(format!(
                "Smooth time: X {:.2}s  Y {:.2}s",
                snapshot.camera_smooth_time_x, snapshot.camera_smooth_time_y
            ));
            ui.label(format!(
                "Following X: {}  Y: {}",
                if snapshot.camera_following_x {
                    "yes"
                } else {
                    "no"
                },
                if snapshot.camera_following_y {
                    "yes"
                } else {
                    "no"
                }
            ));
            let j = snapshot.jitter;
            ui.label(format!(
                "Jitter pred X {:.3}  replica X {:.3}  presented X {:.3}",
                j.pred_x, j.replica_x, j.presented_x
            ));
            ui.label(format!(
                "Cam X {:.3}  desired X {:.3}  screen X {:.3}",
                j.cam_x, j.desired_x, j.screen_x
            ));
            ui.label(format!(
                "Corr {:.3} wu  extra Δx {:.4}  vx {:.3}  auth tick {}  this frame {}  follow X {}  offset ({:.3}, {:.3})",
                j.corr_mag,
                j.extra_dx,
                j.velocity[0],
                j.auth_tick,
                if j.reconciled { "yes" } else { "no" },
                if j.following_x { "yes" } else { "no" },
                j.offset[0],
                j.offset[1]
            ));
            ui.label(format!(
                "Y lerp α {:.3}  prev Y {:.3}  tick Y {:.3}  presented Y {:.3}  vy {:.3}",
                j.interp_alpha, j.prev_y, j.tick_y, j.presented_y, j.velocity[1]
            ));
            ui.label(format!(
                "Screen-X range (180f) {:.3}  follow-X flips {}",
                j.screen_range, j.follow_flips
            ));
            ui.label(format!(
                "max|Δ| pred {:.4}  presented {:.4}  cam {:.4}  screen {:.4}  desired {:.4}",
                j.max_d_pred_x,
                j.max_d_presented_x,
                j.max_d_camera_x,
                j.max_d_screen_x,
                j.max_d_desired_x
            ));
            ui.label(format!(
                "mean|Δpresented| tick-frames {:.4} ({})  idle-frames {:.4} ({})",
                j.mean_d_presented_on_tick, j.tick_frames, j.mean_d_presented_idle, j.idle_frames
            ));
            ui.small(
                "Forensics: Camera tab → Jitter Isolation. Dump writes logs/camera_jitter/*.csv",
            );
        });
    }
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_CM_FOLLOW,
        true,
        "Follow",
        None,
    ) {
        ui.indent(SEC_CM_FOLLOW, |ui| {
            ui.checkbox(&mut ui_state.camera_follow, "Follow Player");
            if ui.button("Center On Player").clicked() {
                ui_state.center_on_player = true;
            }
        });
    }
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_CM_JITTER,
        true,
        "Jitter Isolation",
        Some(ui_state.camera_jitter_mode.as_str()),
    ) {
        ui.indent(SEC_CM_JITTER, |ui| {
            ui.small("DEV diagnosis only. Not product camera behavior.");
            egui::ComboBox::from_id_salt("camera.jitter.mode")
                .selected_text(ui_state.camera_jitter_mode.as_str())
                .show_ui(ui, |ui| {
                    for mode in crate::jitter_forensics::CameraJitterMode::ALL {
                        ui.selectable_value(
                            &mut ui_state.camera_jitter_mode,
                            mode,
                            mode.as_str(),
                        );
                    }
                });
            if ui.button("Dump Camera Jitter Trace").clicked() {
                ui_state.dump_jitter_trace = true;
            }
            if !ui_state.last_jitter_dump.is_empty() {
                ui.small(&ui_state.last_jitter_dump);
            }
            ui.small("Frozen camera + still jitter → presentation. Smooth frozen + jitter when following → camera-relative. Raw vs smoothed: if both jitter, 30 Hz stepping is likely.");
        });
    }
    let parallax_summary = format!(
        "far {:.2} / mid {:.2} / near {:.2}",
        snapshot.parallax_far, snapshot.parallax_mid, snapshot.parallax_near
    );
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_CM_PARALLAX,
        true,
        "Parallax",
        Some(&parallax_summary),
    ) {
        ui.indent(SEC_CM_PARALLAX, |ui| {
            ui.label(format!(
                "Parallax: far {:.2} / mid {:.2} / near {:.2}",
                snapshot.parallax_far, snapshot.parallax_mid, snapshot.parallax_near
            ));
            ui.checkbox(&mut ui_state.show_parallax_debug, "Show Parallax Debug");
        });
    }
}

fn draw_diagnostics_tab(
    ui: &mut egui::Ui,
    snapshot: &DebugSnapshot,
    ui_state: &mut DebugUiState,
    history: &mut CollisionHistory,
) {
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_DG_DETECTORS,
        true,
        "Detectors",
        None,
    ) {
        ui.indent(SEC_DG_DETECTORS, |ui| {
            ui.checkbox(
                &mut ui_state.position_discontinuity_detector,
                "Position Discontinuity Detector",
            );
            ui.checkbox(
                &mut ui_state.log_discontinuities,
                "Log Discontinuities to Console",
            );
            ui.checkbox(
                &mut ui_state.verbose_collision_trace,
                "Verbose Collision Trace",
            );
            ui.label("Defaults: all OFF (no console spam).");
            if ui.button("Clear History").clicked() {
                history.clear();
            }
        });
    }
    let m = snapshot.motion;
    let last_summary = format!("{:?} | disc {}", m.response_kind, m.discontinuity);
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_DG_LAST,
        true,
        "Last tick",
        Some(&last_summary),
    ) {
        ui.indent(SEC_DG_LAST, |ui| {
            ui.label(format!(
                "History: {} / {}",
                history.len(),
                super::collision_history::COLLISION_HISTORY_CAP
            ));
            ui.label(format!(
                "Last tick response: {:?}  axis {:?}",
                m.response_kind, m.correction_axis
            ));
            ui.label(format!(
                "Last Δ ({:.3},{:.3}) corr ({:.3},{:.3}) disc {}",
                m.delta[0], m.delta[1], m.correction[0], m.correction[1], m.discontinuity
            ));
        });
    }
    let hist_summary = format!(
        "{} / {}",
        history.len(),
        super::collision_history::COLLISION_HISTORY_CAP
    );
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_DG_HISTORY,
        true,
        "Recent events",
        Some(&hist_summary),
    ) {
        ui.indent(SEC_DG_HISTORY, |ui| {
            ui.label("Recent events (newest first):");
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for ev in history.iter_newest_first() {
                        let subj = match ev.subject {
                            super::collision_history::DiscSubject::Player => "Player",
                            super::collision_history::DiscSubject::Camera => "Camera",
                        };
                        let cand = ev
                            .candidate
                            .map(|id| format!("{id}"))
                            .unwrap_or_else(|| "—".into());
                        ui.label(format!(
                            "Tick {} | {subj} | {:?} | Δ {:.2} | cand {cand} | {:?} | disc {}",
                            ev.tick,
                            ev.axis,
                            (ev.delta[0] * ev.delta[0] + ev.delta[1] * ev.delta[1]).sqrt(),
                            ev.response_kind,
                            ev.discontinuity
                        ));
                    }
                });
            if let Some(ev) = history.latest() {
                ui.separator();
                ui.label("Latest detail:");
                ui.label(format!("prev {:?}", ev.previous_position));
                ui.label(format!("pos  {:?}", ev.position));
                ui.label(format!("vel  {:?}", ev.velocity));
                ui.label(format!("corr {:?}", ev.correction));
                ui.label(format!("grounded_on {:?}", ev.grounded_on));
                ui.label(format!("discontinuity {}", ev.discontinuity));
            }
        });
    }
}

fn draw_network_tab(ui: &mut egui::Ui, snapshot: &DebugSnapshot, ui_state: &mut DebugUiState) {
    let net = snapshot.network;
    let imp = snapshot.impairment;
    let conn_summary = format!("{} | {}", net.state.as_str(), format_rtt_ms(net.rtt));
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NET_CONN,
        network_section_default_open(SEC_NET_CONN),
        "Connection",
        Some(&conn_summary),
    ) {
        ui.indent(SEC_NET_CONN, |ui| {
            ui.label(format!("Screen: {}", net.client_screen));
            ui.label(format!("NetworkState: {}", net.state.as_str()));
            ui.label(format!("Attempt: {}", net.attempt_id));
            match net.connection_id {
                Some(id) => ui.label(format!("ConnectionId: {id}")),
                None => ui.label("ConnectionId: —"),
            };
            match net.protocol_version {
                Some(v) => ui.label(format!("Protocol: {v}")),
                None => ui.label("Protocol: —"),
            };
            ui.label(format!("Server: {}:{}", net.server_host, net.server_port));
            ui.label(format!("Transport: {}", net.transport));
            match net.connected_for {
                Some(dur) => ui.label(format!("Connected for: {:.1} s", dur.as_secs_f32())),
                None => ui.label("Connected for: —"),
            };
            ui.heading("RTT");
            ui.label(format!("Latest: {}", format_rtt_ms(net.rtt)));
            ui.label(format!("Min: {}", format_rtt_ms(net.rtt_min)));
            ui.label(format!("Max: {}", format_rtt_ms(net.rtt_max)));
            ui.label(format!("EWMA: {}", format_rtt_ms(net.rtt_ewma)));
            ui.small("Measured Ping/Pong RTT. Unimpaired. Not artificial delay.");
            ui.small("DEV ONLY: self-signed cert + skip-verify. Authoritative snapshots; remotes interpolated; local predicted.");
            ui.horizontal(|ui| {
                ui.add_enabled_ui(net.can_connect, |ui| {
                    if ui.button("Connect").clicked() {
                        ui_state.network_connect = true;
                    }
                });
                if ui.button("Disconnect").clicked() {
                    ui_state.network_disconnect = true;
                }
            });
        });
    }
    let aoi_summary = format!(
        "known {} | cand {} | epoch {}",
        snapshot
            .aoi_known
            .map(|n| n.to_string())
            .unwrap_or_else(|| snapshot.replica_entities.to_string()),
        snapshot
            .aoi_candidates
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into()),
        snapshot.replica_epoch
    );
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NET_AOI,
        network_section_default_open(SEC_NET_AOI),
        "Observer AOI",
        Some(&aoi_summary),
    ) {
        ui.indent(SEC_NET_AOI, |ui| {
            ui.label(format!(
                "Observer RuntimeEntityId: {}",
                snapshot.observer_entity.as_deref().unwrap_or("—")
            ));
            ui.label(format!("MapId: {}", snapshot.observer_map));
            ui.label(format!("ChannelId: {}", snapshot.observer_channel));
            ui.label(format!("InstanceId: {}", snapshot.observer_instance));
            ui.label(format!("WorldAddress: {}", snapshot.observer_address));
            ui.horizontal(|ui| {
                ui.strong("Channel:");
                for ch in 0..=purgatory_protocol::DEV_CHANNEL_MAX {
                    let selected = snapshot.observer_channel == ch;
                    if ui.selectable_label(selected, format!("[{ch}]")).clicked() {
                        ui_state.request_channel = Some(ch);
                    }
                }
            });
            ui.label(format!("Enter AOI: {}", snapshot.observer_enter_bounds));
            ui.label(format!("Leave AOI: {}", snapshot.observer_leave_bounds));
            ui.label(format!(
                "Spatial candidates: {}",
                fmt_opt_u16(snapshot.aoi_candidates)
            ));
            ui.label(format!("Known: {}", fmt_opt_u16(snapshot.aoi_known)));
            ui.label(format!(
                "WantEnter: {}",
                fmt_opt_u16(snapshot.aoi_want_enter)
            ));
            ui.label(format!(
                "WantLeave: {}",
                fmt_opt_u16(snapshot.aoi_want_leave)
            ));
            ui.label(format!("Replication epoch: {}", snapshot.replica_epoch));
            ui.label(format!(
                "Players: {}",
                snapshot
                    .inspector
                    .category_summary(InspectorCategory::Players)
            ));
            ui.label(format!(
                "Interactables: {}",
                snapshot
                    .inspector
                    .category_summary(InspectorCategory::Interactables)
            ));
            ui.label(format!(
                "Portals: {}",
                snapshot
                    .inspector
                    .category_summary(InspectorCategory::Portals)
            ));
            ui.label(format!(
                "Platforms: {}",
                snapshot
                    .inspector
                    .category_summary(InspectorCategory::Platforms)
            ));
            ui.small("Mailbox counts are server-authored. WantEnter / spatial candidates that are not Known are not in the replica and are not drawn.");
            ui.small("Band labels (Enter AOI / Leave band / outside leave) are the server view-envelope policy around the local pose (viewport + Dead Zone + prefetch), not a client interest decision.");
            ui.small("ContentId is not on the replica (server placement only). Kinds: Player, Interactable, Portal. World vs Replication rows are labeled separately in World → Entities.");
            ui.checkbox(&mut ui_state.show_aoi_rects, "Show AOI Policy Rects");
            ui.checkbox(&mut ui_state.show_entity_labels, "Show entity labels");
            if snapshot.replica_entity_rows.is_empty() {
                ui.label("Known replica entities: —");
            } else {
                ui.label("Known replica entities:");
                for row in &snapshot.replica_entity_rows {
                    ui.label(format!("  {row}"));
                }
            }
            if snapshot.replica_recent_left.is_empty() {
                ui.label("Recent Left: —");
            } else {
                ui.label("Recent Left (no longer in replica):");
                for row in &snapshot.replica_recent_left {
                    ui.label(format!("  {row}"));
                }
            }
        });
    }
    let interact_summary = snapshot.interact_status.kind.name();
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NET_INTERACT,
        network_section_default_open(SEC_NET_INTERACT),
        "Interaction",
        Some(interact_summary),
    ) {
        ui.indent(SEC_NET_INTERACT, |ui| {
            ui.label(format!(
                "Current: {}",
                snapshot.interact_status.interaction_line()
            ));
            ui.label(format!(
                "Last transition: {}",
                snapshot.interact_status.details_trail()
            ));
            ui.small("Headline is current session state. Fields below are forensic.");
            ui.label(format!(
                "Nearest generic interactable: {} / {}",
                snapshot.interact_nearest.as_deref().unwrap_or("-"),
                snapshot
                    .interact_nearest_distance
                    .map(|d| format!("{d:.2}"))
                    .unwrap_or_else(|| "-".into())
            ));
            ui.label(format!(
                "Nearest portal: {} / {}",
                snapshot.interact_nearest_portal.as_deref().unwrap_or("-"),
                snapshot
                    .interact_nearest_portal_distance
                    .map(|d| format!("{d:.2}"))
                    .unwrap_or_else(|| "-".into())
            ));
            ui.label(format!(
                "Portal eligible: {}",
                snapshot.portal_eligible
            ));
            ui.label(format!(
                "Replica player: {}",
                snapshot
                    .replica_player_pos
                    .map(|p| format!("({:.2}, {:.2})", p[0], p[1]))
                    .unwrap_or_else(|| "-".into())
            ));
            ui.label(format!(
                "Presented player: {}",
                snapshot
                    .presented_player_pos
                    .map(|p| format!("({:.2}, {:.2})", p[0], p[1]))
                    .unwrap_or_else(|| "-".into())
            ));
            ui.label(format!(
                "Portal position: {}",
                snapshot
                    .nearest_portal_pos
                    .map(|p| format!("({:.2}, {:.2})", p[0], p[1]))
                    .unwrap_or_else(|| "-".into())
            ));
            ui.label(format!("Last request (history): {}", snapshot.interact_last_request));
            ui.label(format!(
                "Last server result (history): {}",
                snapshot.interact_last_result
            ));
            ui.label(format!(
                "Reject reason: {}",
                snapshot
                    .interact_status
                    .reject_reason
                    .as_deref()
                    .unwrap_or("—")
            ));
            ui.label(format!(
                "Close reason: {}",
                snapshot
                    .interact_status
                    .close_reason
                    .as_deref()
                    .unwrap_or("—")
            ));
            ui.label(format!("Session / UI (raw): {}", snapshot.interact_ui));
            ui.label(format!(
                "Replica interactables: {}",
                snapshot.replica_interactables.len()
            ));
            if snapshot.replica_interactables.is_empty() {
                ui.small("No generic E target in replica.");
            } else {
                for row in &snapshot.replica_interactables {
                    ui.label(format!("Generic {row}"));
                }
            }
            ui.label(format!("Replica portals: {}", snapshot.replica_portals.len()));
            if snapshot.replica_portals.is_empty() {
                ui.small("No portal in replica. Up Arrow will not send PortalActivate.");
            } else {
                for row in &snapshot.replica_portals {
                    ui.label(format!("Portal {row}"));
                }
            }
            ui.small("E = generic InteractOpen (edge). Portals are excluded. Up Arrow = PortalActivate if eligible. Server validates.");
        });
    }
    let input_summary = format!("seq {}", snapshot.net_input_seq);
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NET_INPUT,
        network_section_default_open(SEC_NET_INPUT),
        "Gameplay Input",
        Some(&input_summary),
    ) {
        ui.indent(SEC_NET_INPUT, |ui| {
            ui.label(format!("Last sequence sent: {}", snapshot.net_input_seq));
            ui.label(format!("Input commands sent: {}", snapshot.net_input_sent));
            ui.label(format!(
                "Semantic: axis={} jump={} down={}",
                snapshot.net_move_axis, snapshot.net_jump, snapshot.net_down
            ));
            ui.small("Intent only. No position/velocity on the wire.");
        });
    }
    let replica_seq = snapshot
        .replica_seq
        .map(|s| s.to_string())
        .unwrap_or_else(|| "—".into());
    let replica_summary = format!(
        "seq {replica_seq} | tick {} | E {} U {} L {}",
        snapshot.replica_tick,
        snapshot.replica_total_enters,
        snapshot.replica_total_updates,
        snapshot.replica_total_leaves
    );
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NET_REPLICA,
        network_section_default_open(SEC_NET_REPLICA),
        "Authoritative Replica",
        Some(&replica_summary),
    ) {
        ui.indent(SEC_NET_REPLICA, |ui| {
            ui.label(format!("Snapshot seq: {replica_seq}"));
            ui.label(format!("Server tick: {}", snapshot.replica_tick));
            ui.label(format!("Replica epoch: {}", snapshot.replica_epoch));
            ui.label(format!("Known entities: {}", snapshot.replica_entities));
            ui.label(format!(
                "Last frame: Enter {} · Update {} · Leave {}",
                snapshot.replica_frame_enters,
                snapshot.replica_frame_updates,
                snapshot.replica_frame_leaves
            ));
            ui.label(format!(
                "Session: Enter {} · Update {} · Leave {}",
                snapshot.replica_total_enters,
                snapshot.replica_total_updates,
                snapshot.replica_total_leaves
            ));
            let unchanged = snapshot
                .replica_entities
                .saturating_sub(snapshot.replica_frame_updates);
            ui.label(format!(
                "Known without Update this frame: {unchanged}"
            ));
            ui.small(
                "AOI answers WHO may need state. Dirty/delta answers WHAT changed. Unchanged Known entities should not receive Update records.",
            );
            ui.label(format!(
                "Replica generic interactables: {}",
                snapshot.replica_interactables.len()
            ));
            if snapshot.replica_interactables.is_empty() {
                ui.label("Generic interactables: — (none in replica)");
            } else {
                for row in &snapshot.replica_interactables {
                    ui.label(format!("Generic {row}"));
                }
            }
            ui.label(format!(
                "Replica portals: {}",
                snapshot.replica_portals.len()
            ));
            if snapshot.replica_portals.is_empty() {
                ui.label("Portal entities: — (none in replica)");
            } else {
                for row in &snapshot.replica_portals {
                    ui.label(format!("Portal {row}"));
                }
            }
            ui.label(format!(
                "Local EntityId: {}",
                snapshot.replica_local.as_deref().unwrap_or("—")
            ));
            ui.label(format!("Stale ignored: {}", snapshot.replica_stale));
            ui.label(format!("Duplicate ignored: {}", snapshot.replica_duplicate));
            ui.label(format!("Malformed: {}", snapshot.replica_malformed));
            ui.label(format!(
                "Snapshot age: {}",
                snapshot
                    .replica_age_ms
                    .map(|ms| format!("{ms} ms"))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.small("Full snapshots. Latest sequence wins. Local: predicted presentation. Remotes: interpolated.");
        });
    }
    let interp_summary = format!(
        "holds {} | snaps {}",
        snapshot.interp_holds, snapshot.interp_snaps
    );
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NET_INTERP,
        network_section_default_open(SEC_NET_INTERP),
        "Remote Interpolation",
        Some(&interp_summary),
    ) {
        ui.indent(SEC_NET_INTERP, |ui| {
            ui.label(format!(
                "Enabled: {}",
                if snapshot.interp_enabled { "yes" } else { "no" }
            ));
            ui.label(format!(
                "Delay: {} ticks ({} ms)",
                snapshot.interp_delay_ticks, snapshot.interp_delay_ms
            ));
            ui.label(format!("History depth: {}", snapshot.interp_history_depth));
            ui.label(format!(
                "Estimated server tick: {:.2}",
                snapshot.interp_estimated_tick
            ));
            ui.label(format!("Render tick: {:.2}", snapshot.interp_render_tick));
            ui.label(format!(
                "Bracket A/B: {} / {}",
                snapshot
                    .interp_bracket_a
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "—".into()),
                snapshot
                    .interp_bracket_b
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!("Alpha: {:.3}", snapshot.interp_alpha));
            ui.label(format!("Holds (underrun): {}", snapshot.interp_holds));
            ui.label(format!("Snaps (teleport): {}", snapshot.interp_snaps));
            ui.small("Presentation only. Monotonic clock. No extrapolation. Remotes only — local uses prediction.");
        });
    }
    let pred_summary = format!(
        "pending {} | ack {} | debt {}",
        snapshot.pred_pending, snapshot.pred_ack, snapshot.pred_debt
    );
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NET_PRED,
        network_section_default_open(SEC_NET_PRED),
        "Local Prediction / Reconciliation",
        Some(&pred_summary),
    ) {
        ui.indent(SEC_NET_PRED, |ui| {
            ui.label(format!(
                "Enabled / active: {} / {}",
                if snapshot.pred_enabled { "yes" } else { "no" },
                if snapshot.pred_active { "yes" } else { "no" }
            ));
            ui.label(format!(
                "Auth pos: {}",
                snapshot
                    .pred_auth_pos
                    .map(|p| format!("({:.3}, {:.3})", p[0], p[1]))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Predicted pos: {}",
                snapshot
                    .pred_pos
                    .map(|p| format!("({:.3}, {:.3})", p[0], p[1]))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Predicted vel: {}",
                snapshot
                    .pred_vel
                    .map(|v| format!("({:.3}, {:.3})", v[0], v[1]))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Lead error (now vs auth): {}",
                snapshot
                    .pred_lead_error
                    .or(snapshot.pred_error)
                    .map(|e| format!("{e:.3} wu"))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Aligned residual (best offset): {}",
                snapshot
                    .pred_aligned_error
                    .map(|e| format!("{e:.3} wu"))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Best temporal offset: {}",
                snapshot
                    .pred_best_offset
                    .map(|t| format!("{t} ticks"))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Aligned dx/dy: {} / {}",
                snapshot
                    .pred_aligned_dx
                    .map(|e| format!("{e:.3}"))
                    .unwrap_or_else(|| "—".into()),
                snapshot
                    .pred_aligned_dy
                    .map(|e| format!("{e:.3}"))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Auth vel: {}",
                snapshot
                    .pred_auth_vel
                    .map(|v| format!("({:.3}, {:.3})", v[0], v[1]))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!("Prediction tick: {}", snapshot.pred_tick));
            ui.label(format!("Auth snapshot tick: {}", snapshot.pred_auth_tick));
            ui.label(format!(
                "Best-match client tick: {}",
                snapshot
                    .pred_best_match_tick
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Hard snaps / pending / ack: {} / {} / {}",
                snapshot.pred_resets, snapshot.pred_pending, snapshot.pred_ack
            ));
            ui.label(format!(
                "Epoch / continuation_debt / HeldCancel pending: {} / {} / {}",
                snapshot.pred_epoch,
                snapshot.pred_debt,
                if snapshot.pred_cancel_pending {
                    "yes"
                } else {
                    "no"
                }
            ));
            ui.label(format!(
                "Residual divergence streak / max: {} / {:.3}",
                snapshot.pred_aligned_divergence, snapshot.pred_max_aligned
            ));
            ui.label(format!(
                "Last correction reason: {}",
                snapshot.pred_last_snap.as_deref().unwrap_or("—")
            ));
            ui.label(format!(
                "Reconcile count: {}",
                snapshot.pred_reconcile_count
            ));
            ui.label(format!(
                "Correction (pre→post restore+replay): last {:.3} / max {:.3} wu",
                snapshot.pred_last_correction, snapshot.pred_max_correction
            ));
            ui.small("Correction is predicted-pose change across restore+replay. Expected lead is auth→replayed predicted. Aligned residual is a separate diagnostic.");
            ui.label(format!(
                "Observed ack delta / jumps / max: {} / {} / {}",
                snapshot.pred_ack_delta, snapshot.pred_ack_jump_count, snapshot.pred_max_ack_delta
            ));
            ui.label(format!(
                "Remainder extra vel: ({:.3}, {:.3})  pending-window stalls: {}",
                snapshot.pred_tick_vel[0], snapshot.pred_tick_vel[1], snapshot.pred_pending_stall
            ));
            ui.small("Ack delta is client-observed advancement between accepted snapshots. It is not late-collapse (delayed/skipped snapshots can jump ack).");
            ui.small(
                "Phase 5.5: restore durable state, drop commands ≤ ack, replay unacked via tick_player. Late-collapse is intentional authoritative input compaction.",
            );
            ui.small(
                "Pending > 0: no 8 wu / vertical failsafe. Empty pending may still hard-snap. DriftCorrection is not used. Gizmos default OFF.",
            );
            ui.small("Gizmo legend (Prediction Gizmos ON):");
            ui.small("• Orange marker = Authoritative replica pose (server snapshot)");
            ui.small("• Green marker = Predicted local pose (simulation/prediction)");
            ui.small("Cyan player quad = local presentation pose. Camera follows that same pose.");
        });
    }
    let impair_summary = format!(
        "{:?} | {}±{} ms",
        imp.profile,
        purgatory_common::impairment::ns_to_ms(imp.input_base_delay_ns),
        purgatory_common::impairment::ns_to_ms(imp.input_jitter_ns)
    );
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NET_IMPAIR,
        network_section_default_open(SEC_NET_IMPAIR),
        "Network Impairment (dev)",
        Some(&impair_summary),
    ) {
        ui.indent(SEC_NET_IMPAIR, |ui| {
            ui.label(format!(
                "Enabled / profile / seed: {} / {} / {:#x}",
                if imp.enabled { "yes" } else { "no" },
                imp.profile.as_str(),
                imp.seed
            ));
            ui.horizontal(|ui| {
                for profile in crate::network::ImpairmentProfile::ALL {
                    if ui
                        .selectable_label(ui_state.impairment_profile == profile, profile.as_str())
                        .clicked()
                    {
                        ui_state.impairment_profile = profile;
                    }
                }
            });
            ui.label(format!(
                "Configured one-way input delay: {} ± {}",
                format_imp_ms(imp.input_base_delay_ns),
                format_imp_ms(imp.input_jitter_ns)
            ));
            ui.label(format!(
                "Configured one-way snapshot delay: {} ± {}",
                format_imp_ms(imp.snapshot_base_delay_ns),
                format_imp_ms(imp.snapshot_jitter_ns)
            ));
            ui.label(format!(
                "Configured artificial RTT (approx): {} — not measured ping",
                format_imp_ms(imp.configured_artificial_rtt_ns)
            ));
            ui.label(format!(
                "Stalled / stall age: {} / {}",
                if imp.input_stalled { "yes" } else { "no" },
                format_imp_ms(imp.stall_age_ns)
            ));
            ui.label(format!(
                "Input delayed / queue / largest: {} / {} / {}",
                imp.input_messages_delayed,
                imp.input_queue_depth,
                format_imp_ms(imp.largest_input_delay_ns)
            ));
            ui.label(format!(
                "Snapshot delayed / queue / largest: {} / {} / {}",
                imp.snapshot_messages_delayed,
                imp.snapshot_queue_depth,
                format_imp_ms(imp.largest_snapshot_delay_ns)
            ));
            ui.label(format!(
                "Snapshot overflow skips / application skips: {} / {}",
                imp.snapshot_overflow_skips, imp.snapshot_application_skips
            ));
            ui.label(format!(
                "Keep every N / stall triggers dropped: {} / {}",
                imp.snapshot_keep_every_n
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "off".into()),
                imp.stall_trigger_dropped
            ));
            ui.horizontal(|ui| {
                for ms in [250_u32, 500, 1000] {
                    if ui.button(format!("Stall input {ms} ms")).clicked() {
                        ui_state.impairment_stall_ms = Some(ms);
                    }
                }
            });
            if ui.button("Reset impairment metrics").clicked() {
                ui_state.reset_impairment_metrics = true;
            }
            ui.small("Delays are one-way artificial delivery delay. Snapshot delay is post-uni-read application delivery, not QUIC flow-control. Profile change does not reset metrics. Off flushes queued items through the bounded FIFO drain.");
        });
    }
    let fail_summary = match net.last_failure {
        Some(kind) if !kind.is_benign() => kind.debug_label().to_string(),
        Some(_) => "ok".into(),
        None => "—".into(),
    };
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NET_FAIL,
        network_section_default_open(SEC_NET_FAIL),
        "Failure / Counters",
        Some(&fail_summary),
    ) {
        ui.indent(SEC_NET_FAIL, |ui| {
            ui.heading("Failure");
            match net.last_failure {
                Some(kind) if !kind.is_benign() => {
                    ui.label(format!("Category: {}", kind.debug_label()));
                    ui.label(format!(
                        "Retryable: {}",
                        if net.last_failure_retryable {
                            "yes"
                        } else {
                            "no"
                        }
                    ));
                    ui.label(format!("Reason: {}", kind.frontend_status()));
                    ui.label(format!("Debug: {}", net.last_status));
                }
                Some(kind) => {
                    ui.label(format!("Last: {} (not a failure)", kind.debug_label()));
                    ui.label("Retryable: no");
                    ui.label(format!("Debug: {}", net.last_status));
                }
                None => {
                    ui.label("Category: —");
                    ui.label("Retryable: —");
                }
            };
            ui.heading("Counters");
            ui.label(format!("Lifecycle events: {}", net.lifecycle_events));
            ui.label(format!("Telemetry events: {}", net.telemetry_events));
            ui.label(format!("Dropped telemetry: {}", net.events_dropped));
            ui.label(format!(
                "Stale events ignored: {}",
                net.stale_events_ignored
            ));
            ui.label(format!("Reconnect attempts: {}", net.reconnect_attempts));
            ui.label(format!(
                "Messages tx/rx: {} / {}",
                net.messages_tx, net.messages_rx
            ));
        });
    }
    let hist_summary = format!(
        "{} / {}",
        net.history_len,
        crate::network::NETWORK_HISTORY_CAP
    );
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NET_HIST,
        network_section_default_open(SEC_NET_HIST),
        "Network History / Logging",
        Some(&hist_summary),
    ) {
        ui.indent(SEC_NET_HIST, |ui| {
            ui.checkbox(&mut ui_state.log_network_lifecycle, "Log Network Lifecycle");
            ui.checkbox(&mut ui_state.verbose_network_trace, "Verbose Network Trace");
            ui.label("Defaults: both OFF.");
            if ui.button("Clear Network History").clicked() {
                ui_state.clear_network_history = true;
            }
            ui.label(format!(
                "History: {} / {}",
                net.history_len,
                crate::network::NETWORK_HISTORY_CAP
            ));
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| {
                    for slot in net.history.iter().flatten() {
                        ui.label(slot.summary());
                    }
                });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{KeyCode, PhysicalKey};

    #[test]
    fn backquote_press_toggles_and_repeat_does_not() {
        let backquote = PhysicalKey::Code(KeyCode::Backquote);
        assert!(is_debug_toggle_key(backquote, true, false));
        assert!(!is_debug_toggle_key(backquote, true, true));
        assert!(!is_debug_toggle_key(backquote, false, false));
        assert!(!is_debug_toggle_key(
            PhysicalKey::Code(KeyCode::Space),
            true,
            false
        ));
    }

    #[test]
    fn every_tab_has_expand_collapse_sections() {
        for tab in [
            DebugTab::Runtime,
            DebugTab::Player,
            DebugTab::Footnote,
            DebugTab::World,
            DebugTab::Camera,
            DebugTab::Diagnostics,
            DebugTab::Network,
        ] {
            assert!(
                tab_section_ids(tab).len() >= 2,
                "{tab:?} should expose Expand all / Collapse all"
            );
        }
    }

    #[test]
    fn network_phase_56_defaults_open_prediction_impairment_interp() {
        assert!(network_section_default_open(SEC_NET_AOI));
        assert!(network_section_default_open(SEC_NET_INTERACT));
        assert!(network_section_default_open(SEC_NET_PRED));
        assert!(network_section_default_open(SEC_NET_IMPAIR));
        assert!(network_section_default_open(SEC_NET_INTERP));
        assert!(network_section_default_open(SEC_NET_REPLICA));
        for id in [SEC_NET_CONN, SEC_NET_INPUT, SEC_NET_FAIL, SEC_NET_HIST] {
            assert!(
                !network_section_default_open(id),
                "{id} should start collapsed"
            );
        }
    }

    #[test]
    fn world_entities_open_by_default_platforms_collapsed() {
        assert!(InspectorView::category_default_open(
            InspectorCategory::Players
        ));
        assert!(InspectorView::category_default_open(
            InspectorCategory::Interactables
        ));
        assert!(InspectorView::category_default_open(
            InspectorCategory::Portals
        ));
        assert!(!InspectorView::category_default_open(
            InspectorCategory::Platforms
        ));
        assert!(!InspectorView::category_default_open(
            InspectorCategory::Other
        ));
    }
}
