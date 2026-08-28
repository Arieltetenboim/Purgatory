//! In-window egui overlay. Not a second OS window.

use egui::{Context, ViewportId};
use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor};
use egui_winit::State;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

use purgatory_simulation::DebugAction;

use super::collision_history::CollisionHistory;
use super::snapshot::DebugSnapshot;
use super::ui_state::DebugUiState;
use crate::renderer::OverlayPass;

/// Arguments for drawing the Connection Frontend during an egui frame.
pub struct ConnectionPaint<'a> {
    pub frontend: &'a crate::frontend::ConnectionFrontend,
    pub server: &'a str,
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
        let mut full_output = self.ctx.run_ui(raw_input, |egui_ctx| {
            if let Some(paint) = &connection {
                connect_clicked =
                    paint
                        .frontend
                        .paint(egui_ctx, paint.server, paint.status, paint.can_connect);
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
                .show(ui, |ui| match *tab {
                    DebugTab::Runtime => draw_runtime_tab(ui, snapshot, ui_state),
                    DebugTab::Player => draw_player_tab(ui, snapshot, actions),
                    DebugTab::Footnote => draw_footnote_tab(ui, snapshot),
                    DebugTab::World => draw_world_tab(ui, snapshot, ui_state),
                    DebugTab::Camera => draw_camera_tab(ui, snapshot, ui_state),
                    DebugTab::Diagnostics => draw_diagnostics_tab(ui, snapshot, ui_state, history),
                    DebugTab::Network => draw_network_tab(ui, snapshot, ui_state),
                });
        });
}

fn draw_runtime_tab(ui: &mut egui::Ui, snapshot: &DebugSnapshot, ui_state: &mut DebugUiState) {
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
    ui.separator();
    ui.label("Gizmo toggles");
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
}

fn draw_player_tab(ui: &mut egui::Ui, snapshot: &DebugSnapshot, actions: &mut Vec<DebugAction>) {
    ui.colored_label(
        egui::Color32::from_rgb(220, 160, 60),
        "LOCAL DEV / NON-AUTHORITATIVE",
    );
    ui.small("Client World FOOTNOTE drives local prediction + Phase 5.5 replay (tick_player). Server owns authority.");
    ui.separator();
    if let Some(player) = snapshot.player {
        ui.label(format!("Entity: {}", player.id));
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
        let m = snapshot.motion;
        ui.separator();
        ui.label("Motion (last sim tick)");
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
        if ui.button("Reset Player").clicked() {
            actions.push(DebugAction::ResetPlayer);
        }
        ui.small(
            "DEV: when connected, snaps local prediction to the authoritative replica (not FOOTNOTE spawn). Offline: local World spawn reset. Never mutates ReplicatedWorld or the server.",
        );
    } else {
        ui.label("Entity: none");
    }
}

fn draw_footnote_tab(ui: &mut egui::Ui, snapshot: &DebugSnapshot) {
    if let Some(fn_dbg) = snapshot.footnote {
        let state = if fn_dbg.grounded {
            "Grounded"
        } else {
            "Airborne"
        };
        ui.label(format!("State: {state}"));
        match fn_dbg.grounded_on {
            Some(id) => ui.label(format!("Platform: {id}")),
            None => ui.label("Platform: none"),
        };
        match fn_dbg.platform_kind {
            Some(kind) => ui.label(format!("Platform kind: {kind:?}")),
            None => ui.label("Platform kind: —"),
        };
        ui.label(format!(
            "Position: X {:.3} | Y {:.3}",
            fn_dbg.position[0], fn_dbg.position[1]
        ));
        ui.label(format!(
            "Velocity: ({:.3}, {:.3})",
            fn_dbg.velocity[0], fn_dbg.velocity[1]
        ));
        ui.label(format!("|vx|: {:.3}", fn_dbg.horizontal_speed));
        ui.label(format!("Input X: {}", fn_dbg.move_axis));
        ui.label(format!("Down held: {}", fn_dbg.down_held));
        match fn_dbg.ignored_platform {
            Some(id) => ui.label(format!("Drop-through ignore: {id}")),
            None => ui.label("Drop-through: inactive"),
        };
        ui.label(format!("Last contact: {:?}", fn_dbg.last_contact));
    } else {
        ui.label("No player.");
    }
}

fn draw_world_tab(ui: &mut egui::Ui, snapshot: &DebugSnapshot, ui_state: &mut DebugUiState) {
    ui.label(format!("Entities: {}", snapshot.entity_count));
    ui.label(format!("Players: {}", snapshot.player_count));
    ui.label(format!("Platforms: {}", snapshot.platform_count));
    ui.label(format!("Stage: {}", snapshot.stage_name));
    let b = snapshot.world_bounds;
    ui.label(format!(
        "Bounds: X [{:.1}, {:.1}]  Y [{:.1}, {:.1}]",
        b.min_x, b.max_x, b.min_y, b.max_y
    ));
    ui.label(format!("Size: {:.1} × {:.1}", b.width(), b.height()));
    ui.checkbox(&mut ui_state.show_world_bounds, "Show World Bounds");
    ui.checkbox(&mut ui_state.show_grid, "Show Grid");
    ui.collapsing("Entities", |ui| {
        for (id, kind) in &snapshot.entities {
            ui.label(format!("[{id}] {kind}"));
        }
    });
}

fn draw_camera_tab(ui: &mut egui::Ui, snapshot: &DebugSnapshot, ui_state: &mut DebugUiState) {
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
    let cm = snapshot.camera_motion;
    ui.label(format!(
        "Prev: ({:.3}, {:.3})",
        cm.previous_position[0], cm.previous_position[1]
    ));
    ui.label(format!(
        "Cam Δ: ({:.3}, {:.3})  disc {}",
        cm.delta[0], cm.delta[1], cm.discontinuity
    ));
    ui.label(format!("Clamp: {:?}", cm.clamp_reason));
    ui.checkbox(&mut ui_state.camera_follow, "Follow Player");
    if ui.button("Center On Player").clicked() {
        ui_state.center_on_player = true;
    }
    ui.label(format!(
        "Parallax: far {:.2} / mid {:.2} / near {:.2}",
        snapshot.parallax_far, snapshot.parallax_mid, snapshot.parallax_near
    ));
    ui.checkbox(&mut ui_state.show_parallax_debug, "Show Parallax Debug");
}

fn draw_diagnostics_tab(
    ui: &mut egui::Ui,
    snapshot: &DebugSnapshot,
    ui_state: &mut DebugUiState,
    history: &mut CollisionHistory,
) {
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
    ui.separator();
    ui.label(format!(
        "History: {} / {}",
        history.len(),
        super::collision_history::COLLISION_HISTORY_CAP
    ));
    let m = snapshot.motion;
    ui.label(format!(
        "Last tick response: {:?}  axis {:?}",
        m.response_kind, m.correction_axis
    ));
    ui.label(format!(
        "Last Δ ({:.3},{:.3}) corr ({:.3},{:.3}) disc {}",
        m.delta[0], m.delta[1], m.correction[0], m.correction[1], m.discontinuity
    ));
    ui.separator();
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
}

fn draw_network_tab(ui: &mut egui::Ui, snapshot: &DebugSnapshot, ui_state: &mut DebugUiState) {
    let net = snapshot.network;
    ui.heading("Connection");
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
    ui.separator();
    ui.heading("RTT");
    fn ms(d: Option<std::time::Duration>) -> String {
        d.map(|rtt| format!("{:.2} ms", rtt.as_secs_f64() * 1000.0))
            .unwrap_or_else(|| "—".into())
    }
    ui.label(format!("Latest: {}", ms(net.rtt)));
    ui.label(format!("Min: {}", ms(net.rtt_min)));
    ui.label(format!("Max: {}", ms(net.rtt_max)));
    ui.label(format!("EWMA: {}", ms(net.rtt_ewma)));
    ui.separator();
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
    ui.separator();
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
    ui.separator();
    ui.heading("Gameplay input (intent)");
    ui.label(format!("Last sequence sent: {}", snapshot.net_input_seq));
    ui.label(format!("Input commands sent: {}", snapshot.net_input_sent));
    ui.label(format!(
        "Semantic: axis={} jump={} down={}",
        snapshot.net_move_axis, snapshot.net_jump, snapshot.net_down
    ));
    ui.small("Intent only. No position/velocity on the wire.");
    ui.separator();
    ui.heading("Authoritative replica");
    ui.label(format!(
        "Snapshot seq: {}",
        snapshot
            .replica_seq
            .map(|s| s.to_string())
            .unwrap_or_else(|| "—".into())
    ));
    ui.label(format!("Server tick: {}", snapshot.replica_tick));
    ui.label(format!(
        "Replicated entities: {}",
        snapshot.replica_entities
    ));
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
    ui.separator();
    ui.heading("Remote interpolation");
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
    ui.separator();
    ui.heading("Local prediction");
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
    ui.small(
        "Phase 5.5: restore durable state, drop commands ≤ ack, replay unacked via tick_player. Late-collapse is intentional authoritative input compaction.",
    );
    ui.small(
        "Pending > 0: no 8 wu / vertical failsafe. Empty pending may still hard-snap. DriftCorrection is not used. Gizmos default OFF.",
    );
    ui.small("Gizmo legend (Prediction Gizmos ON):");
    ui.small("• Orange marker = Authoritative replica pose (server snapshot)");
    ui.small("• Green marker = Predicted local pose (rendered player)");
    ui.small("Cyan player quad = predicted presentation. Camera follows predicted pose.");
    ui.separator();
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
    ui.small("DEV ONLY: self-signed cert + skip-verify. Authoritative snapshots; remotes interpolated; local predicted.");
    ui.separator();
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
}
