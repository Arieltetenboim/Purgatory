//! In-window egui overlay. Not a second OS window.

use std::time::{Duration, Instant};

use egui::{Context, ViewportId};
use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor};
use egui_winit::State;
use purgatory_common::ContentId;
use purgatory_content::ContentRegistry;
use purgatory_protocol::ReplicatedHealth;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

use super::chrome::{
    has_persistent_dev_warnings, input_gate_chrome_visible, interaction_chrome_visible,
    persistent_dev_warnings, portal_chrome_visible, target_chrome_visible,
    transition_chrome_visible,
};
use super::command::DebugCommand;

use super::aoi_view::label_ndc;
use super::collision_history::CollisionHistory;
use super::entity_inspector::{
    InspectorAccent, InspectorCategory, InspectorRow, InspectorSource, InspectorView,
};
use super::frame::DiagnosticsFrame;
use super::interact_status::{
    compact_interact_value, format_portal_line, format_target_line, interact_kind_color,
};
use super::sections::{debug_section, draw_expand_collapse};
use super::ui_state::{
    DEBUG_JUMP_SPEED_MAX, DEBUG_JUMP_SPEED_MIN, DEBUG_MOVE_SPEED_MAX, DEBUG_MOVE_SPEED_MIN,
    DebugUiState, RESET_TO_SPAWN_HELP, RESET_TO_SPAWN_LABEL, reset_action_help, reset_action_label,
    reset_player_uses_replica,
};
use crate::renderer::OverlayPass;

const SEC_NOW_POSE: &str = "debug.pose";
const SEC_NOW_NPC_SPAWN: &str = "debug.npc_spawn";
const SEC_NOW_CAMERA: &str = "debug.camera";
const SEC_NOW_REPLICA: &str = "debug.replica";
const SEC_NOW_VIEW: &str = "debug.view";
const SEC_NOW_DISPLAY: &str = "debug.display";
const SEC_RT_FRAME: &str = "runtime.frame";
const SEC_SK_VIEW: &str = "skeleton.view";
const SEC_SK_INSPECT: &str = "skeleton.inspect";
const SEC_SK_EQUIP: &str = "skeleton.equipment";
const SEC_SK_PROOF: &str = "skeleton.proof";
const SEC_PL_POSE: &str = "player.pose";
const SEC_PL_MOTION: &str = "player.motion";
const SEC_FN_CONTACT: &str = "footnote.contact";
const SEC_WD_STAGE: &str = "world.stage";
const SEC_WD_ENTITIES: &str = "world.entities";
const SEC_CM_TRANSFORM: &str = "camera.transform";
const SEC_CM_JITTER: &str = "camera.jitter";
const SEC_CM_PARALLAX: &str = "camera.parallax";
const SEC_DG_DETECTORS: &str = "diagnostics.detectors";
const SEC_DG_HELP: &str = "diagnostics.help";
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
    Debug,
    Runtime,
    Player,
    Skeleton,
    World,
    Camera,
    Diagnostics,
    Network,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NpcSpawnOption {
    content_id: ContentId,
    authored_id: String,
}

struct DebugOverlayResources<'a> {
    history: &'a mut CollisionHistory,
    npc_spawn_options: &'a [NpcSpawnOption],
}

/// Client-owned development overlay.
pub struct DebugOverlay {
    visible: bool,
    ctx: Context,
    winit: State,
    renderer: EguiRenderer,
    tab: DebugTab,
    npc_spawn_options: Vec<NpcSpawnOption>,
    pub ui: DebugUiState,
    pub collision_history: CollisionHistory,
}

impl DebugOverlay {
    pub fn new(window: &Window, pass: OverlayInit<'_>, registry: &ContentRegistry) -> Self {
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
        let npc_spawn_options: Vec<_> = registry
            .iter_npc_dialogue_presentations()
            .map(|npc| NpcSpawnOption {
                content_id: npc.content_id,
                authored_id: npc.authored_id.clone(),
            })
            .collect();
        let mut ui = DebugUiState::from_env();
        ui.selected_debug_npc = npc_spawn_options.first().map(|npc| npc.content_id);
        Self {
            visible: false,
            ctx,
            winit,
            renderer,
            tab: DebugTab::Debug,
            npc_spawn_options,
            ui,
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
        frame: &DiagnosticsFrame,
        visible: &mut bool,
        tab: &mut DebugTab,
        ui_state: &mut DebugUiState,
        resources: DebugOverlayResources<'_>,
        actions: &mut Vec<DebugCommand>,
    ) {
        draw_debug_window(ctx, frame, visible, tab, ui_state, resources, actions);
    }

    /// One egui frame: optional [`ConnectionFrontend::paint`] plus debug overlay.
    pub fn submit_frame(
        &mut self,
        window: &Window,
        pass: OverlayPass<'_>,
        frame: &DiagnosticsFrame,
        connection: Option<ConnectionPaint<'_>>,
        gameplay_health: Option<ReplicatedHealth>,
    ) -> (Vec<wgpu::CommandBuffer>, Vec<DebugCommand>, bool) {
        if connection.is_none()
            && !self.visible
            && !has_persistent_dev_warnings(&self.ui)
            && !self.ui.center_toast_live()
            && gameplay_health.is_none()
        {
            return (Vec::new(), Vec::new(), false);
        }

        let raw_input = self.winit.take_egui_input(window);
        let ppp = crate::display::effective_pixels_per_point(
            window.scale_factor() as f32,
            crate::display::UiScale::new(frame.runtime.display.ui_scale),
        );
        self.ctx.set_pixels_per_point(ppp);
        let mut actions = Vec::new();
        let mut connect_clicked = false;
        let mut visible = self.visible;
        let mut tab = self.tab;
        let mut ui_state = self.ui.clone();
        let history = &mut self.collision_history;
        let npc_spawn_options = &self.npc_spawn_options;
        let mut connection = connection;
        let mut full_output = self.ctx.run_ui(raw_input, |egui_ctx| {
            if let Some(health) = gameplay_health {
                draw_gameplay_hud(egui_ctx, health, &mut actions);
            }
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
                    frame,
                    &mut visible,
                    &mut tab,
                    &mut ui_state,
                    DebugOverlayResources {
                        history,
                        npc_spawn_options,
                    },
                    &mut actions,
                );
                if ui_state.show_entity_labels {
                    draw_world_entity_labels(egui_ctx, frame);
                }
            } else if has_persistent_dev_warnings(&ui_state) {
                draw_dev_warning_area(egui_ctx, &ui_state);
            }
            if ui_state.show_rf_ab {
                draw_rf_ab_captions(egui_ctx, frame.runtime.display.world_msaa_4x_supported);
            }
            draw_center_toast(egui_ctx, &ui_state);
        });
        self.visible = visible;
        self.tab = tab;
        self.ui = ui_state;
        actions.extend(self.ui.drain_commands());
        if connect_clicked {
            actions.push(DebugCommand::Connect);
        }
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

fn draw_gameplay_hud(
    ctx: &Context,
    health: purgatory_protocol::ReplicatedHealth,
    actions: &mut Vec<DebugCommand>,
) {
    let dead = health.current <= 0.0;
    egui::Area::new(egui::Id::new("purgatory-gameplay-hud"))
        .anchor(egui::Align2::LEFT_TOP, [16.0, 16.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(10, 12, 16, 220))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.label(format!(
                        "Health: {:.0} / {:.0}",
                        health.current.max(0.0),
                        health.max.max(0.0)
                    ));
                    let fraction = if health.max > 0.0 {
                        (health.current / health.max).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .desired_width(180.0)
                            .fill(egui::Color32::from_rgb(70, 190, 95)),
                    );
                    if dead && ui.button("Respawn").clicked() {
                        actions.push(DebugCommand::Respawn);
                    }
                });
        });
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
        DebugTab::Debug => &[
            SEC_NOW_NPC_SPAWN,
            SEC_NOW_POSE,
            SEC_NOW_CAMERA,
            SEC_NOW_REPLICA,
            SEC_NOW_VIEW,
            SEC_NOW_DISPLAY,
        ],
        DebugTab::Runtime => &[SEC_RT_FRAME],
        DebugTab::Player => &[SEC_PL_POSE, SEC_PL_MOTION, SEC_FN_CONTACT],
        DebugTab::Skeleton => &[SEC_SK_VIEW, SEC_SK_INSPECT, SEC_SK_EQUIP, SEC_SK_PROOF],
        DebugTab::World => &[SEC_WD_STAGE, SEC_WD_ENTITIES],
        DebugTab::Camera => &[SEC_CM_TRANSFORM, SEC_CM_JITTER, SEC_CM_PARALLAX],
        DebugTab::Diagnostics => &[SEC_DG_DETECTORS, SEC_DG_HELP, SEC_DG_LAST, SEC_DG_HISTORY],
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

fn network_section_default_open(_id: &'static str) -> bool {
    false
}

const INTERACT_FLASH_SECS: f32 = 0.55;

fn note_interact_flash(ui_state: &mut DebugUiState, frame: &DiagnosticsFrame) {
    let kind = frame.network.interact_status.kind;
    if ui_state.last_interact_kind != Some(kind) {
        if let Some(text) = &frame.network.interact_status.last_transition {
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

fn draw_reserved_chrome_line(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    rgb: (u8, u8, u8),
    live: bool,
) {
    let row_h = ui.text_style_height(&egui::TextStyle::Small).max(14.0) + 2.0;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), row_h),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(ui.available_width());
            let label_color = egui::Color32::from_rgb(150, 154, 162);
            ui.add_sized(
                [56.0, row_h],
                egui::Label::new(egui::RichText::new(label).small().color(label_color)),
            );
            let color = if live {
                egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2)
            } else {
                label_color
            };
            ui.add(egui::Label::new(egui::RichText::new(value).small().color(color)).truncate())
                .on_hover_text(value);
        },
    );
}

fn draw_interact_status_strip(
    ui: &mut egui::Ui,
    frame: &DiagnosticsFrame,
    ui_state: &mut DebugUiState,
) {
    note_interact_flash(ui_state, frame);
    let status = &frame.network.interact_status;
    let rgb = interact_kind_color(status.kind);
    let flash_live = ui_state
        .interact_flash_until
        .is_some_and(|until| Instant::now() < until);
    let flash = if flash_live && !ui_state.interact_flash_text.is_empty() {
        Some(ui_state.interact_flash_text.as_str())
    } else {
        None
    };
    draw_reserved_chrome_line(
        ui,
        "Interact",
        &compact_interact_value(&status.interaction_line(), flash),
        rgb,
        interaction_chrome_visible(status.kind, flash_live),
    );
    draw_reserved_chrome_line(
        ui,
        "Target",
        &format_target_line(
            frame.network.interact_nearest.as_deref(),
            frame.network.interact_nearest_distance,
        ),
        (210, 214, 220),
        target_chrome_visible(
            frame.network.interact_nearest.as_deref(),
            frame.network.interact_nearest_distance,
        ),
    );
    draw_reserved_chrome_line(
        ui,
        "Portal",
        &format_portal_line(
            frame.network.interact_nearest_portal.as_deref(),
            frame.network.portal_eligible,
        ),
        (210, 214, 220),
        portal_chrome_visible(
            frame.network.interact_nearest_portal.as_deref(),
            frame.network.portal_eligible,
        ),
    );
}

fn note_channel_flash(ui_state: &mut DebugUiState, frame: &DiagnosticsFrame) {
    let current = frame.world.observer_channel;
    let epoch = frame.network.replica_epoch;
    if ui_state.last_observed_channel != Some(current) {
        if let Some(prev) = ui_state.last_observed_channel {
            ui_state.note_dev_action_flash(&format!("CHANNEL {prev} -> {current}"));
        }
        ui_state.last_observed_channel = Some(current);
    }
    ui_state.last_observed_epoch = Some(epoch);
}

fn draw_world_address_strip(
    ui: &mut egui::Ui,
    frame: &DiagnosticsFrame,
    ui_state: &mut DebugUiState,
) {
    note_channel_flash(ui_state, frame);
    ui.horizontal_wrapped(|ui| {
        ui.label(format!(
            "Map {} · Ch {} · Inst {}",
            frame.world.observer_map, frame.world.observer_channel, frame.world.observer_instance
        ));
        draw_channel_buttons(ui, frame, ui_state);
    });
    if transition_chrome_visible(&frame.world.transition_banner) {
        let rgb = if frame.world.transition_stalled {
            (255, 90, 90)
        } else {
            (90, 200, 255)
        };
        draw_status_chip(
            ui,
            &frame.world.transition_banner,
            rgb,
            frame.world.transition_stalled,
        );
        if !frame.world.transition_missing.is_empty() {
            ui.small(format!("waiting: {}", frame.world.transition_missing));
        }
    }
    if input_gate_chrome_visible(frame.world.input_movement_neutral) {
        draw_status_chip(ui, &frame.world.input_gate_label, (255, 180, 80), true);
        ui.small("authoritative movement neutral: yes");
    }
}

fn draw_compact_status_line(ui: &mut egui::Ui, frame: &DiagnosticsFrame) {
    let net = frame.network.lifecycle;
    ui.horizontal_wrapped(|ui| {
        ui.label(format!(
            "FPS {:.0} · tick {} · {} {}",
            frame.runtime.fps,
            frame.runtime.tick,
            net.state.as_str(),
            format_rtt_ms(net.rtt)
        ));
    });
}

fn draw_dev_warning_chips(ui: &mut egui::Ui, ui_state: &DebugUiState) {
    let warnings = persistent_dev_warnings(ui_state);
    if warnings.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for warning in &warnings {
            draw_status_chip(ui, &warning.label, warning.rgb, true);
        }
    });
}

fn draw_dev_warning_area(ctx: &Context, ui_state: &DebugUiState) {
    egui::Area::new(egui::Id::new("purgatory-dev-warnings"))
        .fixed_pos([12.0, 12.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            draw_dev_warning_chips(ui, ui_state);
        });
}

fn draw_rf_ab_captions(ctx: &Context, msaa_4x: bool) {
    let ppp = ctx.pixels_per_point().max(0.001);
    for (i, panel) in crate::renderer::rf_diag::rf_ab_layout(msaa_4x)
        .into_iter()
        .enumerate()
    {
        let dest = panel.dest;
        let pos = egui::pos2(
            dest.x as f32 / ppp,
            dest.y
                .saturating_sub(crate::renderer::rf_diag::RF_AB_CAPTION) as f32
                / ppp,
        );
        egui::Area::new(egui::Id::new(("purgatory-rf-ab-cap", i)))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_unmultiplied(8, 8, 12, 200))
                    .inner_margin(egui::Margin::symmetric(4, 1))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(panel.label)
                                .size(11.0)
                                .color(egui::Color32::from_rgb(230, 230, 230)),
                        );
                    });
            });
    }
}

fn draw_center_toast(ctx: &Context, ui_state: &DebugUiState) {
    if !ui_state.center_toast_live() {
        return;
    }
    let max_width = ctx.content_rect().width() * 0.70;
    egui::Area::new(egui::Id::new("purgatory-debug-toast"))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, -48.0])
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_max_width(max_width);
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(12, 12, 16, 210))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgb(255, 230, 90),
                ))
                .inner_margin(egui::Margin::symmetric(14, 8))
                .corner_radius(6)
                .show(ui, |ui| {
                    ui.set_max_width(max_width);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&ui_state.dev_action_flash_text)
                                .size(16.0)
                                .color(egui::Color32::from_rgb(255, 230, 90))
                                .strong(),
                        )
                        .wrap(),
                    );
                });
        });
}

fn draw_channel_buttons(ui: &mut egui::Ui, frame: &DiagnosticsFrame, ui_state: &mut DebugUiState) {
    for ch in 0..=purgatory_protocol::DEV_CHANNEL_MAX {
        let selected = frame.world.observer_channel == ch;
        if ui
            .selectable_label(selected, format!("[{ch}]"))
            .on_hover_text("DEV: request server-authoritative Channel change (DevSetChannel)")
            .clicked()
        {
            ui_state.request_channel = Some(ch);
        }
    }
}

fn draw_reset_action(ui: &mut egui::Ui, frame: &DiagnosticsFrame, actions: &mut Vec<DebugCommand>) {
    let replica = reset_player_uses_replica(frame.network.replica_player_pos.is_some());
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(RESET_TO_SPAWN_LABEL)
            .on_hover_text(RESET_TO_SPAWN_HELP)
            .clicked()
        {
            actions.push(DebugCommand::ResetToSpawn);
        }
        if replica
            && ui
                .button(reset_action_label(true))
                .on_hover_text(reset_action_help(true))
                .clicked()
        {
            actions.push(DebugCommand::ResetPlayer);
        }
    });
}

fn world_gizmo_checkbox(ui: &mut egui::Ui, master: &mut bool, flag: &mut bool, label: &str) {
    let was = *flag;
    ui.checkbox(flag, label);
    super::ui_state::arm_master_on_gizmo_enable(master, was, *flag);
}

fn draw_time_scale_buttons(ui: &mut egui::Ui, ui_state: &mut DebugUiState) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Speed");
        for scale in DebugUiState::TIME_SCALES {
            let label = if (scale - 1.0).abs() < 1e-4 {
                "1.0×"
            } else if (scale - 0.5).abs() < 1e-4 {
                "0.5×"
            } else {
                "0.25×"
            };
            if ui
                .selectable_label((ui_state.time_scale - scale).abs() < 1e-4, label)
                .clicked()
            {
                ui_state.time_scale = scale;
            }
        }
    });
}

fn draw_debug_window(
    ctx: &Context,
    frame: &DiagnosticsFrame,
    visible: &mut bool,
    tab: &mut DebugTab,
    ui_state: &mut DebugUiState,
    resources: DebugOverlayResources<'_>,
    actions: &mut Vec<DebugCommand>,
) {
    let DebugOverlayResources {
        history,
        npc_spawn_options,
    } = resources;
    egui::Window::new("PURGATORY DEBUG")
        .open(visible)
        .resizable(true)
        .constrain(true)
        .default_pos([12.0, 12.0])
        .default_width(360.0)
        .show(ctx, |ui| {
            draw_compact_status_line(ui, frame);
            draw_dev_warning_chips(ui, ui_state);
            draw_world_address_strip(ui, frame, ui_state);
            draw_interact_status_strip(ui, frame, ui_state);

            ui.horizontal_wrapped(|ui| {
                for (label, value) in [
                    ("Debug", DebugTab::Debug),
                    ("Runtime", DebugTab::Runtime),
                    ("Player", DebugTab::Player),
                    ("Skeleton", DebugTab::Skeleton),
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
                        DebugTab::Debug => {
                            draw_now_tab(ui, frame, ui_state, npc_spawn_options, actions)
                        }
                        DebugTab::Runtime => draw_runtime_tab(ui, frame, ui_state),
                        DebugTab::Player => draw_player_tab(ui, frame, ui_state, actions),
                        DebugTab::Skeleton => draw_skeleton_tab(ui, frame, ui_state),
                        DebugTab::World => draw_world_tab(ui, frame, ui_state),
                        DebugTab::Camera => draw_camera_tab(ui, frame, ui_state),
                        DebugTab::Diagnostics => draw_diagnostics_tab(ui, frame, ui_state, history),
                        DebugTab::Network => draw_network_tab(ui, frame, ui_state),
                    }
                });
        });
}

fn draw_display_section(
    ui: &mut egui::Ui,
    frame: &DiagnosticsFrame,
    ui_state: &mut DebugUiState,
    section_id: &'static str,
    default_open: bool,
) {
    use crate::display::{RENDER_SCALE_PRESETS, RESOLUTION_PRESETS};

    let d = frame.runtime.display;
    let msaa = match d.world_msaa_samples {
        4 => "4×".to_string(),
        1 => "Off".to_string(),
        n => format!("{n}×"),
    };
    let summary = match d.internal_render {
        Some((w, h)) => format!("{}% {msaa} → {w}×{h}", d.render_scale_percent),
        None => format!("{}% {msaa}", d.render_scale_percent),
    };
    if debug_section(
        ui,
        &mut ui_state.sections,
        section_id,
        default_open,
        "Display",
        Some(&summary),
    ) {
        ui.indent(section_id, |ui| {
            ui.label(format!(
                "Window logical:  {} × {}",
                d.window_logical.0, d.window_logical.1
            ));
            ui.label(format!(
                "Framebuffer:     {} × {}",
                d.framebuffer.0, d.framebuffer.1
            ));
            ui.label(format!("Surface:         {} × {}", d.surface.0, d.surface.1));
            ui.label(format!(
                "Selected:        {} × {}",
                d.selected.0, d.selected.1
            ));
            ui.label(format!("Aspect ratio:    {}", d.aspect_label()));
            ui.label(format!("Scale factor:    {:.2}", d.scale_factor));
            ui.label(format!("UI scale:        {:.2}", d.ui_scale));
            ui.label(format!("Window mode:     {}", d.window_mode.label()));
            ui.label(format!(
                "Gameplay view:   {:.2} × {:.2} wu (locked 16:9)",
                frame.camera.viewport_width, frame.camera.viewport_height
            ));
            match d.gameplay_pixel {
                Some((x, y, w, h)) => {
                    ui.label(format!("Gameplay pixels: {w} × {h} @ ({x},{y})"));
                }
                None => {
                    ui.label("Gameplay pixels: —");
                }
            }
            ui.label(format!("Render scale:     {}%", d.render_scale_percent));
            match d.internal_render {
                Some((w, h)) if d.internal_render_clamped => {
                    ui.label(format!("Internal render:  {w} × {h} (clamped to GPU limit)"));
                }
                Some((w, h)) => {
                    ui.label(format!("Internal render:  {w} × {h}"));
                }
                None => {
                    ui.label("Internal render:  —");
                }
            }
            match d.monitor_native {
                Some((w, h)) => {
                    ui.label(format!("Monitor native:  {w} × {h}"));
                }
                None => {
                    ui.label("Monitor native:  —");
                }
            }
            ui.small(
                "Window size, render scale, and camera FOV are independent. World renders to an offscreen target at Render Scale, then linear-filters into the locked 16:9 gameplay rect. UI stays at native output resolution. Nearest-neighbor remains a future pixel-art option.",
            );
            ui.separator();
            ui.label("Resolution:");
            ui.horizontal_wrapped(|ui| {
                for preset in RESOLUTION_PRESETS {
                    let selected =
                        d.framebuffer == (preset.width, preset.height) || d.surface == (preset.width, preset.height);
                    if ui
                        .selectable_label(selected, preset.label())
                        .clicked()
                    {
                        ui_state.requested_resolution = Some(preset);
                    }
                }
            });
            ui.small("Applies immediately through the client display path. Same path as OS window resize.");
            ui.separator();
            ui.label("Render scale:");
            ui.horizontal_wrapped(|ui| {
                for preset in RENDER_SCALE_PRESETS {
                    let selected = d.render_scale_percent == preset.percent();
                    let label = if preset == crate::display::RenderScale::DEFAULT {
                        format!("{} default", preset.label())
                    } else if preset == crate::display::RenderScale::PERFORMANCE_FALLBACK {
                        format!("{} perf", preset.label())
                    } else {
                        preset.label()
                    };
                    if ui.selectable_label(selected, label).clicked() {
                        ui_state.requested_render_scale = Some(preset);
                    }
                }
            });
            ui.small("Default 200% (integer 2:1 supersample + linear downsample). 100% is the performance fallback. Does not change window size or gameplay view. 150% is not a quality preset. 400% is RF diagnostic only.");
            ui.separator();
            ui.label("World MSAA:");
            ui.horizontal_wrapped(|ui| {
                for mode in crate::renderer::WorldMsaa::ALL {
                    let selected = d.world_msaa_samples == mode.sample_count();
                    let enabled = mode != crate::renderer::WorldMsaa::X4 || d.world_msaa_4x_supported;
                    let label = if mode == crate::renderer::WorldMsaa::X4 && !d.world_msaa_4x_supported
                    {
                        "4× (unsupported)"
                    } else {
                        mode.as_str()
                    };
                    if ui.selectable_label(selected, label).clicked() && enabled {
                        ui_state.requested_world_msaa = Some(mode);
                    }
                }
            });
            ui.small(
                "Production uses 4×. Off (1×) is compatibility/diagnostic fallback only. Does not change camera FOV.",
            );
            ui.separator();
            ui.checkbox(&mut ui_state.show_rf_scene, "RF0 rotated-geometry scene");
            ui.checkbox(
                &mut ui_state.show_rf_ab,
                "RF1.5–RF3 A/B proof (simultaneous)",
            );
            ui.checkbox(
                &mut ui_state.rf_freeze_camera,
                "Freeze camera (RF0 / same path as Camera Frozen)",
            );
            ui.checkbox(
                &mut ui_state.rf_log_vertices,
                "Log RF0 probe screen vertices",
            );
            ui.small(
                "World-fixed quads: static 0°/15°/30°/45°, slow rotate, subpixel translate, both. Independent of animation. Probe is translate+rotate (green).",
            );
            if ui_state.show_rf_ab {
                ui.small(
                    "A/B compositor ignores gameplay Render Scale. RF3 row is 100/200/400% + 4× with linear downsample and identical dest size. Production blit stays linear. 150% is not in this proof.",
                );
                let samples = if d.world_msaa_4x_supported { 4 } else { 1 };
                let diag: Vec<_> = crate::renderer::rf_diag::RF3_SCALE_PERCENTS
                    .iter()
                    .map(|p| {
                        (
                            *p,
                            crate::renderer::rf_diag::rf_ab_panel_source(*p),
                        )
                    })
                    .collect();
                ui.monospace(format!(
                    "RF3 diagnostic RT {}",
                    diag.iter()
                        .map(|(p, s)| format!("{p}% {}×{}", s.0, s.1))
                        .collect::<Vec<_>>()
                        .join("  ")
                ));
                ui.monospace(format!(
                    "diagnostic sample-texels 4×: {}",
                    diag.iter()
                        .map(|(p, s)| format!(
                            "{p}% {}",
                            crate::renderer::rf_diag::rf_ab_sample_cost(*s, samples)
                        ))
                        .collect::<Vec<_>>()
                        .join("  ")
                ));
                let (gw, gh) = d
                    .gameplay_pixel
                    .map(|(_, _, w, h)| (w, h))
                    .unwrap_or((1280, 720));
                let world: Vec<_> = crate::renderer::rf_diag::RF3_SCALE_PERCENTS
                    .iter()
                    .map(|p| {
                        (
                            *p,
                            crate::renderer::rf_diag::rf_ab_scaled_extent(gw, gh, *p),
                        )
                    })
                    .collect();
                ui.monospace(format!(
                    "if world {gw}×{gh}: {}",
                    world
                        .iter()
                        .map(|(p, s)| format!("{p}% {}×{}", s.0, s.1))
                        .collect::<Vec<_>>()
                        .join("  ")
                ));
                ui.monospace(format!(
                    "implied world sample-texels 4×: {}",
                    world
                        .iter()
                        .map(|(p, s)| format!(
                            "{p}% {}",
                            crate::renderer::rf_diag::rf_ab_sample_cost(*s, samples)
                        ))
                        .collect::<Vec<_>>()
                        .join("  ")
                ));
                ui.small(format!(
                    "Whole-frame FPS {:.1} (vsync-capped; not GPU pass time). Timestamp queries are not wired.",
                    frame.runtime.fps
                ));
                if ui_state.show_rf_scene {
                    ui.small("A/B is on: RF0 is not injected into the gameplay world pass.");
                }
            }
            if let Some(proof) = frame.rf {
                ui.label(format!(
                    "Probe cam ({:.5}, {:.5})  t={:.3}s",
                    proof.camera[0], proof.camera[1], proof.elapsed
                ));
                ui.label(format!(
                    "d world {:.6}  ndc {:.6}  ipx {:.5}  out {:.5}",
                    proof.max_d_world,
                    proof.max_d_ndc,
                    proof.max_d_internal,
                    proof.max_d_output
                ));
                for (i, (w, ip)) in proof.world.iter().zip(proof.internal_px.iter()).enumerate() {
                    ui.monospace(format!(
                        "v{i} world ({:.6},{:.6})  ipx ({:.4},{:.4})  Δipx ({:.5},{:.5})",
                        w[0],
                        w[1],
                        ip[0],
                        ip[1],
                        proof.d_internal_px[i][0],
                        proof.d_internal_px[i][1]
                    ));
                }
                ui.small(if proof.max_d_internal > 0.0 && proof.max_d_internal < 0.35 {
                    "Vertices moving continuously (subpixel). Remaining shimmer is raster/sampling or blit."
                } else if proof.max_d_internal == 0.0 {
                    "Vertices stationary this frame."
                } else {
                    "Large vertex jump — inspect camera/transform quantization."
                });
            }
        });
    }
}

fn draw_npc_spawner(
    ui: &mut egui::Ui,
    frame: &DiagnosticsFrame,
    ui_state: &mut DebugUiState,
    npc_spawn_options: &[NpcSpawnOption],
    actions: &mut Vec<DebugCommand>,
) {
    let selected = ui_state
        .selected_debug_npc
        .and_then(|id| npc_spawn_options.iter().find(|npc| npc.content_id == id));
    let selected_label = selected
        .map(|npc| format!("{} [{}]", npc.authored_id, npc.content_id))
        .unwrap_or_else(|| "No runtime NPCs".into());
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NOW_NPC_SPAWN,
        true,
        "DEV NPC Spawner",
        Some(&selected_label),
    ) {
        ui.indent(SEC_NOW_NPC_SPAWN, |ui| {
            egui::ComboBox::from_id_salt("debug.npc_spawn.select")
                .selected_text(&selected_label)
                .show_ui(ui, |ui| {
                    for npc in npc_spawn_options {
                        ui.selectable_value(
                            &mut ui_state.selected_debug_npc,
                            Some(npc.content_id),
                            format!("{} [{}]", npc.authored_id, npc.content_id),
                        );
                    }
                });
            let can_spawn = frame.network.lifecycle.connection_id.is_some()
                && frame.physics.player.is_some()
                && ui_state.selected_debug_npc.is_some();
            if ui
                .add_enabled(can_spawn, egui::Button::new("Spawn at Player"))
                .on_hover_text(
                    "Server resolves the NPC and uses the player's authoritative current position and WorldAddress",
                )
                .clicked()
                && let Some(content_id) = ui_state.selected_debug_npc
            {
                actions.push(DebugCommand::SpawnNpc(content_id));
            }
            ui.small(
                "Runtime NPC content only. Spawned entity is transient, not written to map content or persistence, and remains until server shutdown.",
            );
        });
    }
}

fn draw_now_tab(
    ui: &mut egui::Ui,
    frame: &DiagnosticsFrame,
    ui_state: &mut DebugUiState,
    npc_spawn_options: &[NpcSpawnOption],
    actions: &mut Vec<DebugCommand>,
) {
    draw_time_scale_buttons(ui, ui_state);
    ui.small("Scales wall elapsed into SimulationClock only. Tick rate stays 30 Hz.");
    draw_reset_action(ui, frame, actions);
    draw_npc_spawner(ui, frame, ui_state, npc_spawn_options, actions);
    let pose_summary = frame.physics.player.map(|player| {
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
        SEC_NOW_POSE,
        true,
        "Local player",
        pose_summary.as_deref(),
    ) {
        ui.indent(SEC_NOW_POSE, |ui| {
            if let Some(player) = frame.physics.player {
                ui.label(format!(
                    "Pos ({:.2}, {:.2})  vel ({:.2}, {:.2})  {}",
                    player.position[0],
                    player.position[1],
                    player.velocity[0],
                    player.velocity[1],
                    if player.grounded { "grounded" } else { "air" }
                ));
                if let Some(fn_dbg) = frame.physics.footnote {
                    ui.label(format!(
                        "Input X {}  down {}",
                        fn_dbg.move_axis, fn_dbg.down_held
                    ));
                }
            } else {
                ui.label("No local player.");
            }
        });
    }
    let cam_summary = format!(
        "follow {}/{}",
        if frame.camera.following_x { "X" } else { "—" },
        if frame.camera.following_y { "Y" } else { "—" }
    );
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NOW_CAMERA,
        true,
        "Camera",
        Some(&cam_summary),
    ) {
        ui.indent(SEC_NOW_CAMERA, |ui| {
            ui.label(format!(
                "Following X: {}  Y: {}",
                if frame.camera.following_x {
                    "yes"
                } else {
                    "no"
                },
                if frame.camera.following_y {
                    "yes"
                } else {
                    "no"
                }
            ));
            ui.label(format!(
                "Dead zone half X {:.2}  Y {:.2}",
                frame.camera.deadzone_half_x, frame.camera.deadzone_half_y
            ));
            ui.checkbox(&mut ui_state.camera_follow, "Follow Player");
            if ui.button("Center On Player").clicked() {
                ui_state.center_on_player = true;
            }
        });
    }
    let replica_seq = frame
        .network
        .replica_seq
        .map(|s| s.to_string())
        .unwrap_or_else(|| "—".into());
    let pred_line = frame
        .network
        .pred
        .lead_error
        .map(|e| format!("{e:.2} wu"))
        .unwrap_or_else(|| "—".into());
    let replica_summary = format!(
        "known {} · seq {replica_seq} · pred {}",
        frame.network.replica_entities,
        if frame.network.pred.active {
            "on"
        } else {
            "off"
        }
    );
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NOW_REPLICA,
        true,
        "Replica / prediction",
        Some(&replica_summary),
    ) {
        ui.indent(SEC_NOW_REPLICA, |ui| {
            ui.label(format!(
                "Known {}  snapshot seq {replica_seq}",
                frame.network.replica_entities
            ));
            ui.label(format!(
                "Pred {}  lead {}",
                if frame.network.pred.active {
                    "active"
                } else {
                    "idle"
                },
                pred_line
            ));
        });
    }
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NOW_VIEW,
        true,
        "View / gizmos",
        None,
    ) {
        ui.indent(SEC_NOW_VIEW, |ui| {
            ui.checkbox(
                &mut ui_state.show_placeholder_character,
                "Show Placeholder Character",
            );
            ui.checkbox(&mut ui_state.show_skeleton, "Show Skeleton");
            ui.checkbox(
                &mut ui_state.show_local_player_quad,
                "Show legacy blue AABB",
            );
            ui.separator();
            ui.checkbox(&mut ui_state.show_overlay_gizmos, "Show overlay gizmos");
            if !ui_state.show_overlay_gizmos {
                ui.small("Master off: world gizmos muted. Checking a gizmo below turns the master on.");
            }
            world_gizmo_checkbox(
                ui,
                &mut ui_state.show_overlay_gizmos,
                &mut ui_state.show_colliders,
                "Show Colliders",
            );
            world_gizmo_checkbox(
                ui,
                &mut ui_state.show_overlay_gizmos,
                &mut ui_state.show_velocity,
                "Show Velocity Vector",
            );
            world_gizmo_checkbox(
                ui,
                &mut ui_state.show_overlay_gizmos,
                &mut ui_state.show_grounded_highlight,
                "Highlight Grounded Platform",
            );
            world_gizmo_checkbox(
                ui,
                &mut ui_state.show_overlay_gizmos,
                &mut ui_state.show_world_bounds,
                "Show World Bounds",
            );
            world_gizmo_checkbox(
                ui,
                &mut ui_state.show_overlay_gizmos,
                &mut ui_state.show_grid,
                "Show Grid",
            );
            world_gizmo_checkbox(
                ui,
                &mut ui_state.show_overlay_gizmos,
                &mut ui_state.show_parallax_debug,
                "Show Parallax Debug",
            );
            world_gizmo_checkbox(
                ui,
                &mut ui_state.show_overlay_gizmos,
                &mut ui_state.show_aoi_rects,
                "Show AOI Policy Rects",
            );
            ui.checkbox(&mut ui_state.show_entity_labels, "Show entity labels");
            ui.small("Entity labels are egui chips; they do not use the gizmo master.");
            world_gizmo_checkbox(
                ui,
                &mut ui_state.show_overlay_gizmos,
                &mut ui_state.show_camera_deadzone,
                "Show Camera Dead Zone",
            );
            world_gizmo_checkbox(
                ui,
                &mut ui_state.show_overlay_gizmos,
                &mut ui_state.show_interpolation_gizmos,
                "Show Interpolation Gizmos",
            );
            world_gizmo_checkbox(
                ui,
                &mut ui_state.show_overlay_gizmos,
                &mut ui_state.show_prediction_gizmos,
                "Show Prediction Gizmos",
            );
            ui.small("Interp/prediction gizmos need an active replica. Velocity draws a rest tick when idle.");
        });
    }
    draw_display_section(ui, frame, ui_state, SEC_NOW_DISPLAY, false);
}

fn draw_runtime_tab(ui: &mut egui::Ui, frame: &DiagnosticsFrame, ui_state: &mut DebugUiState) {
    let frame_summary = format!("FPS {:.1} | tick {}", frame.runtime.fps, frame.runtime.tick);
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_RT_FRAME,
        true,
        "Frame / Clock",
        Some(&frame_summary),
    ) {
        ui.indent(SEC_RT_FRAME, |ui| {
            ui.label(format!("Frames: {}", frame.runtime.frames));
            ui.label(format!("Tick: {}", frame.runtime.tick));
            ui.label(format!(
                "Sim rate: {} Hz (fixed)",
                frame.runtime.tick_rate_hz
            ));
            ui.label(format!(
                "Window: {}x{}",
                frame.runtime.window_width, frame.runtime.window_height
            ));
            ui.label(format!("FPS: {:.1}", frame.runtime.fps));
            ui.label(format!("Time scale: {:.2}", ui_state.time_scale));
            ui.small("Speed control: Debug tab. Tick rate stays 30 Hz.");
        });
    }
}

fn draw_skeleton_tab(ui: &mut egui::Ui, frame: &DiagnosticsFrame, ui_state: &mut DebugUiState) {
    if debug_section(ui, &mut ui_state.sections, SEC_SK_VIEW, true, "View", None) {
        ui.indent(SEC_SK_VIEW, |ui| {
            ui.checkbox(
                &mut ui_state.show_placeholder_character,
                "Show Placeholder Character",
            );
            ui.checkbox(
                &mut ui_state.presentation_view_back,
                "Force Back view (8F-B visibility)",
            );
            ui.checkbox(
                &mut ui_state.presentation_force_climb_back,
                "Force ClimbBack activity (8F-C)",
            );
            ui.checkbox(&mut ui_state.show_skeleton, "Show Skeleton");
            ui.checkbox(
                &mut ui_state.show_local_player_quad,
                "Show legacy blue AABB",
            );
            ui.checkbox(
                &mut ui_state.skeleton_debug_preview_2x,
                "2× Debug Preview",
            );
            ui.small("Base presentation: 1.15× about planted feet. 2× Debug Preview is diagnostic only; it does not change the 1.15 baseline, bind, evaluate, or simulation AABB.");
            ui.small("Joints are small opaque circular dots under the placeholders.");
            ui.small("Uncheck legacy blue AABB to hide the cyan body rectangle.");
            ui.small("P4.2 placeholders and 8E attachment debug quads come from CharacterPresentationSet (local and remote share one path). Stage D joints remain the local S2 overlay.");
            ui.small("Force Back / Force ClimbBack apply on this client to every CharacterPresentationSet player (local + remotes this client interpolates). They are not local-only. They do not change the other client's overlay, Stage D joints, or the remote magenta AABB.");
            ui.small("8F-A draw order (far → near): ArmBack → LegBack → Core → LegFront → Head → ArmFront. Attachments emit after their layer's base pieces. No per-item z.");
            ui.small("8F-B Back view hides authored ArmFront/LegFront (and their attachments). ArmBack/LegBack paint in the near Front slots. Force Back is draw-only and does not change activity.");
            ui.small("8F-C ClimbBack sets PresentationView::Back from semantic activity. Force ClimbBack injects that activity on this client's local and remote player entries. Not a network climb.");
            ui.small("8F-D ClimbBack plays content/shared/animations/dev/climb_back.anim (not Idle). Force ClimbBack activity. Rebuild the client after Lab save.");
            ui.small("8F-E equipment debug color follows Side vs Back visual keys (PresentationView). Missing Back is omitted, not Side. Equip cloth cap / plate / boots vs gloves.");
        });
    }
    let skel_summary = frame
        .presentation
        .skeleton_inspect
        .as_ref()
        .map(|s| format!("{} {}", s.index, s.name))
        .unwrap_or_else(|| "—".into());
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_SK_INSPECT,
        false,
        "Inspect",
        Some(&skel_summary),
    ) {
        ui.indent(SEC_SK_INSPECT, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Prev").clicked() {
                    ui_state.skeleton_inspect_index =
                        ui_state.skeleton_inspect_index.saturating_sub(1);
                }
                if ui.button("Next").clicked() {
                    ui_state.skeleton_inspect_index = (ui_state.skeleton_inspect_index + 1).min(15);
                }
            });
            egui::ComboBox::from_id_salt("skeleton.inspect.bone")
                .selected_text(format!(
                    "{} {}",
                    ui_state.skeleton_inspect_index,
                    crate::skeleton_debug::HUMANOID_V0_BONE_LABELS
                        [usize::from(ui_state.skeleton_inspect_index.min(15))]
                ))
                .show_ui(ui, |ui| {
                    for (i, label) in crate::skeleton_debug::HUMANOID_V0_BONE_LABELS
                        .iter()
                        .enumerate()
                    {
                        let i = u8::try_from(i).unwrap_or(0);
                        ui.selectable_value(
                            &mut ui_state.skeleton_inspect_index,
                            i,
                            format!("{i} {label}"),
                        );
                    }
                });
            if let Some(inspect) = &frame.presentation.skeleton_inspect {
                ui.label(format!(
                    "Local  T ({:.3}, {:.3})  R {:.3}",
                    inspect.local_t[0], inspect.local_t[1], inspect.local_r
                ));
                ui.label(format!(
                    "World  T ({:.3}, {:.3})  R {:.3}",
                    inspect.world_t[0], inspect.world_t[1], inspect.world_r
                ));
                match inspect.screen {
                    Some(px) => {
                        ui.label(format!("Screen X {:.1}  Y {:.1}", px[0], px[1]));
                    }
                    None => {
                        ui.label("Screen —");
                    }
                }
                ui.small("Local/World are evaluate (1×). Screen follows the preview-scaled joint.");
            } else {
                ui.label("No presented pose this frame.");
            }
        });
    }
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_SK_EQUIP,
        false,
        "Debug equipment",
        Some(&format!(
            "{} chars / {} bound / hide {:#06x}",
            frame.presentation.characters, frame.presentation.bound, frame.presentation.hidden
        )),
    ) {
        ui.indent(SEC_SK_EQUIP, |ui| {
            ui.small("Sends server-authoritative Equip/Unequip. Placeholders are CharacterPresentationSet debug quads, not ART.");
            ui.label(format!(
                "Visible characters: {}  bound attachments: {}  hidden_base: {:#06x}",
                frame.presentation.characters,
                frame.presentation.bound,
                frame.presentation.hidden
            ));
            ui.separator();
            ui.label("Owned item instances (authoritative Inventory):");
            if frame.network.inventory.is_empty() {
                ui.small("Inventory empty — press E near the gold Item drop.");
            } else {
                for entry in &frame.network.inventory {
                    let selected = ui_state.selected_inventory_item == Some(entry.item_instance_id);
                    if ui
                        .selectable_label(
                            selected,
                            format!(
                                "slot {}  {}  qty {}  id {}",
                                entry.slot, entry.definition, entry.quantity, entry.item_instance_id
                            ),
                        )
                        .clicked()
                    {
                        ui_state.selected_inventory_item = Some(entry.item_instance_id);
                    }
                }
                if ui
                    .button("Equip selected Item (Weapon proof slot)")
                    .clicked()
                {
                    ui_state.request_debug_equip_item = ui_state.selected_inventory_item;
                }
            }
            if frame.presentation.missing.is_empty() {
                ui.label("Missing presentation: none");
            } else {
                for line in &frame.presentation.missing {
                    ui.label(format!("missing: {line}"));
                }
            }
            ui.small("Equip controls use the selected owned ItemInstanceId; no content-only shortcut.");
            ui.horizontal(|ui| {
                for (i, name) in ["Head", "Body", "Pants"].iter().enumerate() {
                    if ui.small_button(format!("−{name}")).clicked() {
                        ui_state.request_debug_unequip_slot = Some(i as u8);
                    }
                }
            });
            ui.horizontal(|ui| {
                for (i, name) in ["Gloves", "Boots", "Weapon"].iter().enumerate() {
                    if ui.small_button(format!("−{name}")).clicked() {
                        ui_state.request_debug_unequip_slot = Some((i + 3) as u8);
                    }
                }
            });
            if ui.small_button("Unequip all").clicked() {
                ui_state.request_debug_unequip_all = true;
            }
        });
    }
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_SK_PROOF,
        true,
        "Proof",
        None,
    ) {
        ui.indent(SEC_SK_PROOF, |ui| {
            ui.horizontal(|ui| {
                ui.label("Front-leg proof:");
                egui::ComboBox::from_id_salt("skeleton.proof.front_leg")
                    .selected_text(ui_state.skeleton_front_leg_proof.as_str())
                    .show_ui(ui, |ui| {
                        for mode in crate::skeleton_debug::FrontLegProof::ALL {
                            ui.selectable_value(
                                &mut ui_state.skeleton_front_leg_proof,
                                mode,
                                mode.as_str(),
                            );
                        }
                    });
            });
            ui.small("Off by default. Foot independent: rotation about the ankle; shin and upper leg stay (no shin→ankle gap).");
            ui.small("Shin carries foot: knee planted; shin + foot placeholders follow; upper leg stays.");
            ui.horizontal(|ui| {
                ui.label("Front-arm proof:");
                egui::ComboBox::from_id_salt("skeleton.proof.front_arm")
                    .selected_text(ui_state.skeleton_front_arm_proof.as_str())
                    .show_ui(ui, |ui| {
                        for mode in crate::skeleton_debug::FrontArmProof::ALL {
                            ui.selectable_value(
                                &mut ui_state.skeleton_front_arm_proof,
                                mode,
                                mode.as_str(),
                            );
                        }
                    });
            });
            ui.small("Off by default. Hand independent: rotation about the wrist; forearm and upper arm stay (no forearm→hand gap).");
            ui.small("Forearm carries hand: elbow planted; forearm + hand placeholders follow; upper arm stays.");
            ui.separator();
            ui.label("Animation proof");
            ui.horizontal(|ui| {
                ui.label("Mode:");
                egui::ComboBox::from_id_salt("skeleton.proof.animation_mode")
                    .selected_text(ui_state.animation_proof_mode.as_str())
                    .show_ui(ui, |ui| {
                        for mode in crate::debug::AnimationProofMode::ALL {
                            ui.selectable_value(
                                &mut ui_state.animation_proof_mode,
                                mode,
                                mode.as_str(),
                            );
                        }
                    });
            });
            ui.label("Animation A1 sample time");
            ui.add_enabled(
                ui_state.animation_proof_mode == crate::debug::AnimationProofMode::ManualA1,
                egui::Slider::new(
                    &mut ui_state.skeleton_a1_sample_t,
                    0.0..=purgatory_animation::A1_HEAD_CLIP_DURATION,
                )
                .text("t"),
            );
            ui.small("Manual A1: explicit sample time on the hard-coded head rotation clip. t=0 is bind-equivalent.");
            ui.separator();
            ui.label("A5 presentation oneshot (authoritative DEV)");
            ui.horizontal(|ui| {
                if ui.button("Attack").clicked() {
                    ui_state.request_presentation_attack = true;
                }
                if ui.button("Hurt").clicked() {
                    ui_state.request_presentation_hurt = true;
                }
            });
            ui.small("Server owns duration + Hurt-interrupts-Attack. Clip end does not grant gameplay authority.");
            if ui_state.animation_proof_mode == crate::debug::AnimationProofMode::PlaybackA2 {
                ui.horizontal(|ui| {
                    if ui.button("Play").clicked() {
                        ui_state.request_animation_play = true;
                    }
                    if ui.button("Pause").clicked() {
                        ui_state.request_animation_pause = true;
                    }
                    if ui.button("Reset").clicked() {
                        ui_state.request_animation_reset = true;
                    }
                    ui.label(if ui_state.animation_player_playing {
                        "playing"
                    } else {
                        "paused"
                    });
                });
                ui.add(
                    egui::Slider::new(&mut ui_state.skeleton_a2_speed, 0.0..=2.0).text("speed"),
                );
                ui.small(format!(
                    "A2 loop player sample t = {:.3} (advanced once/frame; speed applied inside advance)",
                    ui_state.selected_animation_sample_t
                ));
            } else {
                ui.small(format!(
                    "Selected sample t = {:.3} (feeds CharacterPresentationSet + Stage D)",
                    ui_state.selected_animation_sample_t
                ));
            }
            ui.small("A1/A2 diagnostics only (Stage D). They do not drive CharacterPresentationSet A3 Idle/Move playback.");
            ui.small("One resolved sample t per frame drives Stage D joints. A3 uses per-character players for all visible players.");
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

fn gameplay_overlay_rect(ctx: &Context, frame: &DiagnosticsFrame) -> egui::Rect {
    let full = ctx.viewport_rect();
    let Some((x, y, w, h)) = frame.runtime.display.gameplay_pixel else {
        return full;
    };
    let ppp = ctx.pixels_per_point();
    let min = egui::pos2(full.min.x + x as f32 / ppp, full.min.y + y as f32 / ppp);
    egui::Rect::from_min_size(min, egui::vec2(w as f32 / ppp, h as f32 / ppp))
}

fn draw_world_entity_labels(ctx: &Context, frame: &DiagnosticsFrame) {
    // `replica_label_world` is empty unless maps are aligned and the
    // presentation world is ready (idle or DestinationReady). Do not project
    // leftover replica poses through a mismatched camera.
    if frame.world.replica_label_world.is_empty() {
        return;
    }
    let rect = gameplay_overlay_rect(ctx, frame);
    let vw = frame.camera.viewport_width;
    let vh = frame.camera.viewport_height;
    if vw <= 0.0 || vh <= 0.0 {
        return;
    }
    for (i, (text, world, tag)) in frame.world.replica_label_world.iter().enumerate() {
        let ndc = label_ndc(*world, frame.camera.position, [vw, vh]);
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
    frame: &DiagnosticsFrame,
    ui_state: &mut DebugUiState,
    actions: &mut Vec<DebugCommand>,
) {
    let pose_summary = frame.physics.player.map(|player| {
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
            ui.add(
                egui::Slider::new(
                    &mut ui_state.debug_move_speed,
                    DEBUG_MOVE_SPEED_MIN..=DEBUG_MOVE_SPEED_MAX,
                )
                .text("Move speed"),
            );
            if ui.small_button("Reset speed").clicked() {
                ui_state.request_debug_move_speed_reset();
            }
            ui.small(
                "DEV player speed (wu/s). Connected: server-authoritative; prediction uses the same override.",
            );
            ui.add(
                egui::Slider::new(
                    &mut ui_state.debug_jump_speed,
                    DEBUG_JUMP_SPEED_MIN..=DEBUG_JUMP_SPEED_MAX,
                )
                .text("Jump strength"),
            );
            if ui.small_button("Reset jump").clicked() {
                ui_state.request_debug_jump_speed_reset();
            }
            ui.small(
                "DEV jump speed (wu/s). Connected: server-authoritative; prediction uses the same override.",
            );
            ui.horizontal(|ui| {
                ui.label("Headwear");
                for cell in 0..4u8 {
                    if ui
                        .selectable_label(
                            ui_state.headwear_side_cell == cell,
                            format!("{}", cell + 1),
                        )
                        .clicked()
                    {
                        ui_state.headwear_side_cell = cell;
                    }
                }
            });
            ui.small(format!(
                "Compile-embedded Side cell {} ({})",
                ui_state.headwear_side_cell + 1,
                crate::headwear_proof::VISUAL_KEYS[usize::from(ui_state.headwear_side_cell.min(3))]
            ));
            if let Some(player) = frame.physics.player {
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
                if let Some(fn_dbg) = frame.physics.footnote {
                    ui.label(format!("Input X: {}", fn_dbg.move_axis));
                    ui.label(format!("Down held: {}", fn_dbg.down_held));
                }
                draw_reset_action(ui, frame, actions);
            } else {
                ui.label("Entity: none");
            }
        });
    }
    let m = frame.physics.motion;
    let motion_summary = format!("disc {} | {:?}", m.discontinuity, m.response_kind);
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_PL_MOTION,
        false,
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
    draw_footnote_contact_section(ui, frame, ui_state);
}

fn draw_footnote_contact_section(
    ui: &mut egui::Ui,
    frame: &DiagnosticsFrame,
    ui_state: &mut DebugUiState,
) {
    let Some(fn_dbg) = frame.physics.footnote else {
        return;
    };
    let state = if fn_dbg.grounded {
        "Grounded"
    } else {
        "Airborne"
    };
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_FN_CONTACT,
        false,
        "FOOTNOTE contact",
        Some(state),
    ) {
        ui.indent(SEC_FN_CONTACT, |ui| {
            ui.label(format!("State: {state}"));
            ui.label(format!(
                "Position: X {:.3} | Y {:.3}",
                fn_dbg.position[0], fn_dbg.position[1]
            ));
            ui.label(format!(
                "Velocity: X {:.3} | Y {:.3}  |vx| {:.3}",
                fn_dbg.velocity[0], fn_dbg.velocity[1], fn_dbg.horizontal_speed
            ));
            match fn_dbg.grounded_on {
                Some(id) => ui.label(format!("Platform: {id}")),
                None => ui.label("Platform: none"),
            };
            match fn_dbg.platform_kind {
                Some(kind) => ui.label(format!("Platform kind: {kind:?}")),
                None => ui.label("Platform kind: —"),
            };
            ui.label(format!("Last contact: {:?}", fn_dbg.last_contact));
            match fn_dbg.ignored_platform {
                Some(id) => ui.label(format!("Drop-through ignore: {id}")),
                None => ui.label("Drop-through: inactive"),
            };
        });
    }
}

fn draw_world_tab(ui: &mut egui::Ui, frame: &DiagnosticsFrame, ui_state: &mut DebugUiState) {
    let stage_summary = format!(
        "{} | {} ents",
        frame.world.stage_name, frame.world.roster.entity_count
    );
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_WD_STAGE,
        true,
        "Stage",
        Some(&stage_summary),
    ) {
        ui.indent(SEC_WD_STAGE, |ui| {
            ui.label(format!("Entities: {}", frame.world.roster.entity_count));
            ui.label(format!("Players: {}", frame.world.roster.player_count));
            ui.label(format!("Platforms: {}", frame.world.roster.platform_count));
            ui.label(format!("Stage: {}", frame.world.stage_name));
            ui.label(format!("MapId: {}", frame.world.map_id));
            ui.label(format!("Map: {}", frame.world.map_debug_name));
            ui.label(format!("Channel: {}", frame.world.observer_channel));
            ui.label(format!("Instance: {}", frame.world.observer_instance));
            ui.label(format!("WorldAddress: {}", frame.world.observer_address));
            ui.label(format!(
                "Content registry: {} defs",
                frame.world.content_registry_count
            ));
            for label in &frame.world.content_map_labels {
                ui.label(format!("  {label}"));
            }
            let b = frame.world.roster.bounds;
            ui.label(format!(
                "Bounds: X [{:.1}, {:.1}]  Y [{:.1}, {:.1}]",
                b.min_x, b.max_x, b.min_y, b.max_y
            ));
            ui.label(format!("Size: {:.1} × {:.1}", b.width(), b.height()));
            ui.small("Gizmo toggles: Debug tab.");
        });
    }
    let entities_summary = frame.world.inspector.entities_header_summary();
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_WD_ENTITIES,
        false,
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
                draw_inspector_category(ui, &mut ui_state.sections, &frame.world.inspector, category);
            }
        });
    }
}

fn draw_camera_tab(ui: &mut egui::Ui, frame: &DiagnosticsFrame, ui_state: &mut DebugUiState) {
    let cm = frame.camera.motion;
    let transform_summary = format!(
        "({:.2}, {:.2}) disc {}",
        frame.camera.position[0], frame.camera.position[1], cm.discontinuity
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
                frame.camera.position[0], frame.camera.position[1]
            ));
            ui.label(format!(
                "Viewport: {:.2} × {:.2}",
                frame.camera.viewport_width, frame.camera.viewport_height
            ));
            let b = frame.world.roster.bounds;
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
                frame
                    .camera
                    .presented_player_pos
                    .map(|p| format!("({:.3}, {:.3})", p[0], p[1]))
                    .unwrap_or_else(|| "-".into())
            ));
            ui.label(format!(
                "Desired target: ({:.3}, {:.3})",
                frame.camera.desired[0], frame.camera.desired[1]
            ));
            ui.label(format!(
                "Dead Zone half: X {:.2}  Y {:.2}",
                frame.camera.deadzone_half_x, frame.camera.deadzone_half_y
            ));
            ui.label(format!(
                "Smooth time: X {:.2}s  Y {:.2}s",
                frame.camera.smooth_time_x, frame.camera.smooth_time_y
            ));
            ui.label(format!(
                "Following X: {}  Y: {}",
                if frame.camera.following_x {
                    "yes"
                } else {
                    "no"
                },
                if frame.camera.following_y {
                    "yes"
                } else {
                    "no"
                }
            ));
            ui.small("Follow / Center On Player: Debug tab. 180-frame jitter: Jitter Isolation.");
        });
    }
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_CM_JITTER,
        false,
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
            let j = frame.camera.jitter;
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
            ui.small("Dump writes logs/camera_jitter/*.csv");
            ui.small("Frozen camera + still jitter → presentation. Smooth frozen + jitter when following → camera-relative. Raw vs smoothed: if both jitter, 30 Hz stepping is likely.");
        });
    }
    let parallax_summary = format!(
        "far {:.2} / mid {:.2} / near {:.2}",
        frame.camera.parallax_far, frame.camera.parallax_mid, frame.camera.parallax_near
    );
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_CM_PARALLAX,
        false,
        "Parallax",
        Some(&parallax_summary),
    ) {
        ui.indent(SEC_CM_PARALLAX, |ui| {
            ui.label(format!(
                "Parallax: far {:.2} / mid {:.2} / near {:.2}",
                frame.camera.parallax_far, frame.camera.parallax_mid, frame.camera.parallax_near
            ));
            ui.small("Parallax debug markers: Debug tab gizmos.");
        });
    }
}

fn draw_diagnostics_tab(
    ui: &mut egui::Ui,
    frame: &DiagnosticsFrame,
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
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_DG_HELP,
        false,
        "Authority notes",
        None,
    ) {
        ui.indent(SEC_DG_HELP, |ui| {
            ui.small("Client World FOOTNOTE drives local prediction + replay. Server owns authority. Connected Reset to Spawn Point is DevResetPlayer. Reanchor Prediction is client-only.");
        });
    }
    let m = frame.physics.motion;
    let last_summary = format!("{:?} | disc {}", m.response_kind, m.discontinuity);
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_DG_LAST,
        false,
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
        false,
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

fn draw_network_tab(ui: &mut egui::Ui, frame: &DiagnosticsFrame, ui_state: &mut DebugUiState) {
    let net = frame.network.lifecycle;
    let imp = frame.network.impairment;
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
            ui.small("Channel [0]/[1]: compact chrome beside Map / Channel / Instance. Same DevSetChannel path.");
            ui.horizontal(|ui| {
                if net.client_screen != "Connection" {
                    ui.add_enabled_ui(net.can_connect, |ui| {
                        if ui.button("Connect").clicked() {
                            ui_state.network_connect = true;
                        }
                    });
                }
                if ui.button("Disconnect").clicked() {
                    ui_state.network_disconnect = true;
                }
            });
            if net.client_screen == "Connection" {
                ui.small("Connect: Connection Frontend.");
            }
        });
    }
    let aoi_summary = format!(
        "known {} | cand {} | epoch {}",
        frame
            .world
            .aoi_known
            .map(|n| n.to_string())
            .unwrap_or_else(|| frame.network.replica_entities.to_string()),
        frame
            .world
            .aoi_candidates
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into()),
        frame.network.replica_epoch
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
                frame.world.observer_entity.as_deref().unwrap_or("—")
            ));
            ui.label(format!("MapId: {}", frame.world.observer_map));
            ui.label(format!("ChannelId: {}", frame.world.observer_channel));
            ui.label(format!("InstanceId: {}", frame.world.observer_instance));
            ui.label(format!("WorldAddress: {}", frame.world.observer_address));
            ui.label(format!("Enter AOI: {}", frame.world.observer_enter_bounds));
            ui.label(format!("Leave AOI: {}", frame.world.observer_leave_bounds));
            ui.label(format!(
                "Spatial candidates: {}",
                fmt_opt_u16(frame.world.aoi_candidates)
            ));
            ui.label(format!("Known: {}", fmt_opt_u16(frame.world.aoi_known)));
            ui.label(format!(
                "WantEnter: {}",
                fmt_opt_u16(frame.world.aoi_want_enter)
            ));
            ui.label(format!(
                "WantLeave: {}",
                fmt_opt_u16(frame.world.aoi_want_leave)
            ));
            ui.label(format!("Replication epoch: {}", frame.network.replica_epoch));
            ui.label(format!(
                "Players: {}",
                frame.world.inspector
                    .category_summary(InspectorCategory::Players)
            ));
            ui.label(format!(
                "Interactables: {}",
                frame.world.inspector
                    .category_summary(InspectorCategory::Interactables)
            ));
            ui.label(format!(
                "Portals: {}",
                frame.world.inspector
                    .category_summary(InspectorCategory::Portals)
            ));
            ui.label(format!(
                "Platforms: {}",
                frame.world.inspector
                    .category_summary(InspectorCategory::Platforms)
            ));
            ui.small("Mailbox counts are server-authored. WantEnter / spatial candidates that are not Known are not in the replica and are not drawn.");
            ui.small("Band labels (Enter AOI / Leave band / outside leave) are the server view-envelope policy around the local pose (viewport + Dead Zone + prefetch), not a client interest decision.");
            ui.small("ContentId is not on the replica (server placement only). Kinds: Player, Interactable, Portal. World vs Replication rows are labeled separately in World → Entities.");
            ui.small("AOI rects / entity labels: Debug tab gizmos.");
            if frame.world.replica_entity_rows.is_empty() {
                ui.label("Known replica entities: —");
            } else {
                ui.label("Known replica entities:");
                for row in &frame.world.replica_entity_rows {
                    ui.label(format!("  {row}"));
                }
            }
            if frame.world.replica_recent_left.is_empty() {
                ui.label("Recent Left: —");
            } else {
                ui.label("Recent Left (no longer in replica):");
                for row in &frame.world.replica_recent_left {
                    ui.label(format!("  {row}"));
                }
            }
        });
    }
    let interact_summary = frame.network.interact_status.kind.name();
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
                frame.network.interact_status.interaction_line()
            ));
            ui.label(format!(
                "Last transition: {}",
                frame.network.interact_status.details_trail()
            ));
            ui.small("Headline is current session state. Fields below are forensic.");
            ui.label(format!(
                "Nearest interaction target: {} / {}",
                frame.network.interact_nearest.as_deref().unwrap_or("-"),
                frame.network.interact_nearest_distance
                    .map(|d| format!("{d:.2}"))
                    .unwrap_or_else(|| "-".into())
            ));
            ui.label(format!(
                "Nearest portal: {} / {}",
                frame.network.interact_nearest_portal.as_deref().unwrap_or("-"),
                frame.network.interact_nearest_portal_distance
                    .map(|d| format!("{d:.2}"))
                    .unwrap_or_else(|| "-".into())
            ));
            ui.label(format!(
                "Portal eligible: {}",
                frame.network.portal_eligible
            ));
            ui.label(format!(
                "Replica player: {}",
                frame.network.replica_player_pos
                    .map(|p| format!("({:.2}, {:.2})", p[0], p[1]))
                    .unwrap_or_else(|| "-".into())
            ));
            ui.label(format!(
                "Presented player: {}",
                frame.camera.presented_player_pos
                    .map(|p| format!("({:.2}, {:.2})", p[0], p[1]))
                    .unwrap_or_else(|| "-".into())
            ));
            ui.label(format!(
                "Portal position: {}",
                frame.network.nearest_portal_pos
                    .map(|p| format!("({:.2}, {:.2})", p[0], p[1]))
                    .unwrap_or_else(|| "-".into())
            ));
            ui.label(format!("Last request (history): {}", frame.network.interact_last_request));
            ui.label(format!(
                "Last server result (history): {}",
                frame.network.interact_last_result
            ));
            ui.label(format!(
                "Reject reason: {}",
                frame.network.interact_status
                    .reject_reason
                    .as_deref()
                    .unwrap_or("—")
            ));
            ui.label(format!(
                "Close reason: {}",
                frame.network.interact_status
                    .close_reason
                    .as_deref()
                    .unwrap_or("—")
            ));
            ui.label(format!("Session / UI (raw): {}", frame.network.interact_ui));
            ui.small("Replica interactable/portal lists: Network → Authoritative Replica.");
            ui.small("E = generic InteractOpen (edge). Portals are excluded. Up Arrow = PortalActivate if eligible. J = Basic Strike intent (no target). Server validates.");
        });
    }
    let input_summary = format!("seq {}", frame.network.net_input_seq);
    if debug_section(
        ui,
        &mut ui_state.sections,
        SEC_NET_INPUT,
        network_section_default_open(SEC_NET_INPUT),
        "Gameplay Input",
        Some(&input_summary),
    ) {
        ui.indent(SEC_NET_INPUT, |ui| {
            ui.label(format!(
                "Last sequence sent: {}",
                frame.network.net_input_seq
            ));
            ui.label(format!(
                "Input commands sent: {}",
                frame.network.net_input_sent
            ));
            ui.label(format!(
                "Semantic: axis={} jump={} down={}",
                frame.network.net_move_axis, frame.network.net_jump, frame.network.net_down
            ));
            ui.small("Intent only. No position/velocity on the wire.");
        });
    }
    let replica_seq = frame
        .network
        .replica_seq
        .map(|s| s.to_string())
        .unwrap_or_else(|| "—".into());
    let replica_summary = format!(
        "seq {replica_seq} | tick {} | E {} U {} L {}",
        frame.network.replica_tick,
        frame.network.replica_total_enters,
        frame.network.replica_total_updates,
        frame.network.replica_total_leaves
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
            ui.label(format!("Server tick: {}", frame.network.replica_tick));
            ui.label(format!("Replica epoch: {}", frame.network.replica_epoch));
            ui.label(format!("Known entities: {}", frame.network.replica_entities));
            ui.label(format!(
                "Last frame: Enter {} · Update {} · Leave {}",
                frame.network.replica_frame_enters,
                frame.network.replica_frame_updates,
                frame.network.replica_frame_leaves
            ));
            ui.label(format!(
                "Session: Enter {} · Update {} · Leave {}",
                frame.network.replica_total_enters,
                frame.network.replica_total_updates,
                frame.network.replica_total_leaves
            ));
            let unchanged = frame.network.replica_entities
                .saturating_sub(frame.network.replica_frame_updates);
            ui.label(format!(
                "Known without Update this frame: {unchanged}"
            ));
            ui.small(
                "AOI answers WHO may need state. Dirty/delta answers WHAT changed. Unchanged Known entities should not receive Update records.",
            );
            ui.label(format!(
                "Replica generic interactables: {}",
                frame.network.replica_interactables.len()
            ));
            if frame.network.replica_interactables.is_empty() {
                ui.label("Generic interactables: — (none in replica)");
            } else {
                for row in &frame.network.replica_interactables {
                    ui.label(format!("Generic {row}"));
                }
            }
            ui.label(format!(
                "Replica portals: {}",
                frame.network.replica_portals.len()
            ));
            if frame.network.replica_portals.is_empty() {
                ui.label("Portal entities: — (none in replica)");
            } else {
                for row in &frame.network.replica_portals {
                    ui.label(format!("Portal {row}"));
                }
            }
            ui.label(format!(
                "Local EntityId: {}",
                frame.network.replica_local.as_deref().unwrap_or("—")
            ));
            ui.label(format!("Stale ignored: {}", frame.network.replica_stale));
            ui.label(format!("Duplicate ignored: {}", frame.network.replica_duplicate));
            ui.label(format!("Malformed: {}", frame.network.replica_malformed));
            ui.label(format!(
                "Snapshot age: {}",
                frame.network.replica_age_ms
                    .map(|ms| format!("{ms} ms"))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.small("Full snapshots. Latest sequence wins. Local: predicted presentation. Remotes: interpolated.");
        });
    }
    let interp_summary = format!(
        "holds {} | snaps {} | dups {}",
        frame.network.interp.holds, frame.network.interp.snaps, frame.network.interp.dup_skips
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
                if frame.network.interp.enabled { "yes" } else { "no" }
            ));
            ui.label(format!(
                "Delay: {} ticks ({} ms)",
                frame.network.interp.delay_ticks, frame.network.interp.delay_ms
            ));
            ui.label(format!("History depth: {}", frame.network.interp.history_depth));
            ui.label(format!(
                "Estimated server tick: {:.2}",
                frame.network.interp.estimated_server_tick
            ));
            ui.label(format!("Render tick: {:.2}", frame.network.interp.render_tick));
            ui.label(format!(
                "Bracket A/B: {} / {}",
                frame.network.interp.bracket_a_tick
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "—".into()),
                frame.network.interp.bracket_b_tick
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!("Alpha: {:.3}", frame.network.interp.alpha));
            ui.label(format!("Holds (underrun): {}", frame.network.interp.holds));
            ui.label(format!("Snaps (teleport): {}", frame.network.interp.snaps));
            ui.label(format!(
                "Dup skips (stale remotes): {}",
                frame.network.interp.dup_skips
            ));
            ui.label(format!(
                "oldest tick: {}",
                frame.network.interp.oldest_tick
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "newest tick: {}",
                frame.network.interp.newest_tick
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "clamped newest: {}",
                if frame.network.interp.clamped_newest {
                    "yes"
                } else {
                    "no"
                }
            ));
            ui.label(format!(
                "clamped oldest: {}",
                if frame.network.interp.clamped_oldest {
                    "yes"
                } else {
                    "no"
                }
            ));
            ui.label(format!(
                "history reset/reseed count: {}",
                frame.network.interp.reseeds
            ));
            ui.separator();
            ui.label("Remote motion probe (first other player)");
            let probe = frame.network.remote_motion;
            match (probe.entity_index, probe.entity_generation) {
                (Some(index), Some(generation)) => {
                    ui.label(format!(
                        "Entity {}/{} | attachments {} | interp used {}",
                        index,
                        generation,
                        probe.attachments,
                        if probe.used_interp { "yes" } else { "no - replica fallback" }
                    ));
                    ui.label(format!(
                        "Auth: {}",
                        probe
                            .auth
                            .map(|p| format!("({:.3}, {:.3})", p[0], p[1]))
                            .unwrap_or_else(|| "—".into())
                    ));
                    ui.label(format!(
                        "Interp: {}",
                        probe
                            .interp
                            .map(|p| format!("({:.3}, {:.3})", p[0], p[1]))
                            .unwrap_or_else(|| "—".into())
                    ));
                    ui.label(format!(
                        "Presented: {}",
                        probe
                            .presented
                            .map(|p| format!("({:.3}, {:.3})", p[0], p[1]))
                            .unwrap_or_else(|| "—".into())
                    ));
                    ui.label(format!(
                        "Presented root: {}",
                        probe
                            .presented_root
                            .map(|p| format!("({:.3}, {:.3})", p[0], p[1]))
                            .unwrap_or_else(|| "—".into())
                    ));
                    ui.label(format!(
                        "dx auth-interp: {}",
                        probe
                            .dx_auth_interp
                            .map(|d| format!("{d:.4}"))
                            .unwrap_or_else(|| "—".into())
                    ));
                    ui.label(format!(
                        "dx interp-presented: {}",
                        probe
                            .dx_interp_presented
                            .map(|d| format!("{d:.4}"))
                            .unwrap_or_else(|| "—".into())
                    ));
                    ui.label(format!(
                        "AOI band: {}",
                        probe.aoi_band.unwrap_or("—")
                    ));
                    ui.label(format!(
                        "observer distance: {}",
                        probe
                            .observer_distance
                            .map(|d| format!("{d:.2} wu"))
                            .unwrap_or_else(|| "—".into())
                    ));
                    ui.label(format!(
                        "latest auth transform tick: {}",
                        probe
                            .last_auth_transform_tick
                            .map(|t| t.to_string())
                            .unwrap_or_else(|| "—".into())
                    ));
                    ui.label(format!(
                        "effective received gap: {} (max {})",
                        probe
                            .effective_received_gap
                            .map(|g| format!("{g} ticks"))
                            .unwrap_or_else(|| "—".into()),
                        probe
                            .max_received_gap
                            .map(|g| g.to_string())
                            .unwrap_or_else(|| "—".into())
                    ));
                    ui.label(format!(
                        "unique transform ticks: {} ({} .. {}) [{}]",
                        probe.unique_count,
                        probe
                            .unique_oldest_tick
                            .map(|t| t.to_string())
                            .unwrap_or_else(|| "—".into()),
                        probe
                            .unique_newest_tick
                            .map(|t| t.to_string())
                            .unwrap_or_else(|| "—".into()),
                        if probe.unique_history_len == 0 {
                            "—".into()
                        } else {
                            probe.unique_history_ticks[..probe.unique_history_len as usize]
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        }
                    ));
                    ui.label(format!(
                        "entity sample A/B: {} / {}  alpha {:.3}  clamped newest {}",
                        probe
                            .entity_bracket_a
                            .map(|t| t.to_string())
                            .unwrap_or_else(|| "—".into()),
                        probe
                            .entity_bracket_b
                            .map(|t| t.to_string())
                            .unwrap_or_else(|| "—".into()),
                        probe.entity_alpha,
                        if probe.entity_clamped_newest {
                            "yes"
                        } else {
                            "no"
                        }
                    ));
                    ui.label(format!(
                        "history samples with entity: {} (ticks {} .. {})",
                        probe.history_with_entity,
                        probe
                            .entity_oldest_tick
                            .map(|t| t.to_string())
                            .unwrap_or_else(|| "—".into()),
                        probe
                            .entity_newest_tick
                            .map(|t| t.to_string())
                            .unwrap_or_else(|| "—".into())
                    ));
                    ui.small(
                        "If InEnter + received gap 2 + entity clamped newest=no, hitch is not Selective cadence. If received gap ≥4 or entity clamped newest=yes, delay 3 is exhausted.",
                    );
                }
                _ => {
                    ui.label("No remote player in replica.");
                }
            }
            ui.small("Presentation only. Monotonic clock. No extrapolation. Remotes only — local uses prediction.");
        });
    }
    let pred_summary = format!(
        "pending {} | ack {} | debt {}",
        frame.network.pred.pending_count,
        frame.network.pred.last_ack,
        frame.network.pred.continuation_debt
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
                if frame.network.pred.enabled { "yes" } else { "no" },
                if frame.network.pred.active { "yes" } else { "no" }
            ));
            ui.label(format!(
                "Auth pos: {}",
                frame.network.pred.auth_position
                    .map(|p| format!("({:.3}, {:.3})", p[0], p[1]))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Predicted pos: {}",
                frame.network.pred.predicted_position
                    .map(|p| format!("({:.3}, {:.3})", p[0], p[1]))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Predicted vel: {}",
                frame.network.pred.predicted_velocity
                    .map(|v| format!("({:.3}, {:.3})", v[0], v[1]))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Lead error (now vs auth): {}",
                frame.network.pred.lead_error
                    .map(|e| format!("{e:.3} wu"))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Aligned residual (best offset): {}",
                frame.network.pred.aligned_error
                    .map(|e| format!("{e:.3} wu"))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Best temporal offset: {}",
                frame.network.pred.best_temporal_offset
                    .map(|t| format!("{t} ticks"))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Aligned dx/dy: {} / {}",
                frame.network.pred.aligned_dx
                    .map(|e| format!("{e:.3}"))
                    .unwrap_or_else(|| "—".into()),
                frame.network.pred.aligned_dy
                    .map(|e| format!("{e:.3}"))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Auth vel: {}",
                frame.network.pred.auth_velocity
                    .map(|v| format!("({:.3}, {:.3})", v[0], v[1]))
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!("Prediction tick: {}", frame.network.pred.prediction_tick));
            ui.label(format!("Auth snapshot tick: {}", frame.network.pred.auth_server_tick));
            ui.label(format!(
                "Best-match client tick: {}",
                frame.network.pred.best_match_tick
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "—".into())
            ));
            ui.label(format!(
                "Hard snaps / pending / ack: {} / {} / {}",
                frame.network.pred.reset_count, frame.network.pred.pending_count, frame.network.pred.last_ack
            ));
            ui.label(format!(
                "Epoch / continuation_debt / HeldCancel pending: {} / {} / {}",
                frame.network.pred.input_epoch,
                frame.network.pred.continuation_debt,
                if frame.network.pred.cancel_pending {
                    "yes"
                } else {
                    "no"
                }
            ));
            ui.label(format!(
                "Residual divergence streak / max: {} / {:.3}",
                frame.network.pred.consecutive_aligned_divergence, frame.network.pred.max_aligned_error
            ));
            ui.label(format!(
                "Last correction reason: {}",
                frame.network.pred.last_snap_reason.unwrap_or("—")
            ));
            ui.label(format!(
                "Reconcile count: {}",
                frame.network.pred.total_reconciliation_count
            ));
            ui.label(format!(
                "Correction (pre→post restore+replay): last {:.3} / max {:.3} wu",
                frame.network.pred.last_correction_wu, frame.network.pred.max_correction_wu
            ));
            ui.small("Correction is predicted-pose change across restore+replay. Expected lead is auth→replayed predicted. Aligned residual is a separate diagnostic.");
            ui.label(format!(
                "Observed ack delta / jumps / max: {} / {} / {}",
                frame.network.pred.observed_ack_delta, frame.network.pred.observed_ack_jump_count, frame.network.pred.max_observed_ack_delta
            ));
            ui.label(format!(
                "Remainder extra vel: ({:.3}, {:.3})  pending-window stalls: {}",
                frame.network.pred.last_tick_velocity[0], frame.network.pred.last_tick_velocity[1], frame.network.pred.pending_window_stall_ticks
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
    fn every_multi_section_tab_has_expand_collapse_ids() {
        for tab in [
            DebugTab::Debug,
            DebugTab::Player,
            DebugTab::Skeleton,
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
        assert_eq!(tab_section_ids(DebugTab::Runtime).len(), 1);
        assert_eq!(
            tab_section_ids(DebugTab::Debug),
            &[
                SEC_NOW_NPC_SPAWN,
                SEC_NOW_POSE,
                SEC_NOW_CAMERA,
                SEC_NOW_REPLICA,
                SEC_NOW_VIEW,
                SEC_NOW_DISPLAY,
            ]
        );
        assert_eq!(
            reset_action_label(true),
            "Reanchor Prediction",
            "replica_player_pos (local_entity) → reanchor, not spawn"
        );
        assert_eq!(reset_action_label(false), "Reset to Spawn Point");
        assert!(tab_section_ids(DebugTab::Skeleton).contains(&SEC_SK_EQUIP));
        assert!(tab_section_ids(DebugTab::Player).contains(&SEC_FN_CONTACT));
        assert!(tab_section_ids(DebugTab::Diagnostics).contains(&SEC_DG_HELP));
        assert!(!tab_section_ids(DebugTab::World).contains(&"world.view"));
        assert!(!tab_section_ids(DebugTab::Camera).contains(&"camera.follow"));
    }

    #[test]
    fn network_forensic_sections_start_collapsed() {
        for id in [
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
        ] {
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
