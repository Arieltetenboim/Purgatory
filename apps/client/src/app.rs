//! Client application loop: platform events, simulation clock, renderer.

use std::sync::Arc;
use std::time::{Duration, Instant};

use purgatory_simulation::{
    Aabb, PlatformKind, PlayerInput, SimulationClock, TICK_DURATION, World,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::debug::{
    CameraMotionDebug, CollisionHistoryEvent, DebugOverlay, DebugSnapshot, DiscSubject,
    OverlayInit, SnapshotExtras, footnote_debug_quads, gameplay_receives_keyboard,
    gameplay_receives_pointer, is_debug_toggle,
};
use crate::input::ActionState;
use crate::network::{ClientEndpointConfig, NetworkHandle};
use crate::platform::{diagnostic_title, window_attributes};
use crate::renderer::{
    Camera, DrawQuad, FOOTNOTE_LOGICAL_HEIGHT, FrameStatus, PARALLAX_FAR, PARALLAX_MID,
    PARALLAX_NEAR, Renderer, is_usable_surface, parallax_debug_quads, parallax_quads,
};

const PLAYER_COLOR: [f32; 4] = [0.19, 0.55, 0.66, 1.0];
const SOLID_FLOOR_COLOR: [f32; 4] = [0.22, 0.28, 0.24, 1.0];
const SOLID_PLATFORM_COLOR: [f32; 4] = [0.45, 0.32, 0.18, 1.0];
const ONEWAY_COLOR: [f32; 4] = [0.55, 0.62, 0.85, 1.0];
const MARKER_COLOR: [f32; 4] = [0.91, 0.69, 0.19, 1.0];

pub fn run() -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|err| format!("event loop: {err}"))?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = ClientApp::new();
    event_loop
        .run_app(&mut app)
        .map_err(|err| format!("run app: {err}"))?;
    if let Some(err) = app.fatal {
        return Err(err);
    }
    Ok(())
}

struct ClientApp {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    debug: Option<DebugOverlay>,
    clock: SimulationClock,
    world: World,
    actions: ActionState,
    last_input: PlayerInput,
    last_instant: Instant,
    last_title_tick: u64,
    fps: f32,
    fatal: Option<String>,
    last_camera_motion: CameraMotionDebug,
    network: Option<NetworkHandle>,
}

impl ClientApp {
    fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            debug: None,
            clock: SimulationClock::new(),
            world: World::footnote_test_stage(),
            actions: ActionState::default(),
            last_input: PlayerInput::idle(),
            last_instant: Instant::now(),
            last_title_tick: 0,
            fps: 0.0,
            fatal: None,
            last_camera_motion: CameraMotionDebug::default(),
            network: match NetworkHandle::start(ClientEndpointConfig::dev()) {
                Ok(handle) => Some(handle),
                Err(err) => {
                    eprintln!("PURGATORY client network init failed: {err}");
                    None
                }
            },
        }
    }

    fn debug_overlay_visible(&self) -> bool {
        self.debug.as_ref().is_some_and(DebugOverlay::is_visible)
    }

    fn time_scale(&self) -> f32 {
        self.debug
            .as_ref()
            .map(|d| d.ui.time_scale)
            .unwrap_or(1.0)
            .clamp(0.01, 1.0)
    }

    fn toggle_debug_overlay(&mut self) {
        if let Some(overlay) = &mut self.debug {
            overlay.toggle();
        }
    }

    fn advance_simulation(&mut self) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_instant);
        self.last_instant = now;
        let seconds = elapsed.as_secs_f32();
        self.fps = if seconds > 0.0 { 1.0 / seconds } else { 0.0 };

        // Development time scale: scale wall elapsed into the clock only.
        // TICK_RATE_HZ / TICK_DURATION are unchanged.
        let scale = self.time_scale();
        let scaled = if scale >= 0.999 {
            elapsed
        } else {
            let nanos = (elapsed.as_secs_f64() * f64::from(scale) * 1_000_000_000.0) as u64;
            Duration::from_nanos(nanos)
        };
        let update = self.clock.advance(scaled);
        let dt = TICK_DURATION.as_secs_f32();
        let detector = self
            .debug
            .as_ref()
            .map(|d| d.ui.position_discontinuity_detector)
            .unwrap_or(false);
        let log_disc = self
            .debug
            .as_ref()
            .map(|d| d.ui.log_discontinuities)
            .unwrap_or(false);
        let verbose = self
            .debug
            .as_ref()
            .map(|d| d.ui.verbose_collision_trace)
            .unwrap_or(false);
        for _ in 0..update.ticks_executed {
            let input = self.actions.consume_tick_input();
            self.last_input = input;
            self.world.tick(dt, input);
            let tick = self.clock.tick().get();
            let motion = self.world.last_motion_debug();
            if verbose && (motion.correction[0].abs() > 1e-5 || motion.correction[1].abs() > 1e-5) {
                eprintln!(
                    "PURGATORY collision trace tick={tick} kind={:?} axis={:?} corr={:?} cand={:?} prev={:?} pos={:?}",
                    motion.response_kind,
                    motion.correction_axis,
                    motion.correction,
                    motion.collision_candidate,
                    motion.previous_position,
                    motion.position
                );
            }
            if detector && motion.discontinuity {
                let ev = CollisionHistoryEvent {
                    tick,
                    subject: DiscSubject::Player,
                    axis: motion.correction_axis,
                    delta: motion.delta,
                    correction: motion.correction,
                    previous_position: motion.previous_position,
                    position: motion.position,
                    velocity: motion.velocity,
                    candidate: motion.collision_candidate,
                    grounded_on: motion.grounded_on,
                    response_kind: motion.response_kind,
                    discontinuity: true,
                };
                if let Some(debug) = &mut self.debug {
                    debug.collision_history.push(ev);
                }
                if log_disc {
                    eprintln!(
                        "PURGATORY player discontinuity tick={tick} delta={:?} corr={:?} cand={:?}",
                        motion.delta, motion.correction, motion.collision_candidate
                    );
                }
            }
        }
    }

    fn update_camera(&mut self) {
        let bounds = self.world.bounds();
        let player_pos = self
            .world
            .player_body()
            .map(|b| b.position)
            .unwrap_or([0.0, 0.0]);
        let follow = self
            .debug
            .as_ref()
            .map(|d| d.ui.camera_follow)
            .unwrap_or(true);
        let center_request = self.debug.as_ref().is_some_and(|d| d.ui.center_on_player);
        if let Some(debug) = &mut self.debug {
            debug.ui.center_on_player = false;
        }
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let mut camera = renderer.camera();
        let previous = camera.position;
        let follow_now = follow || center_request;
        if follow_now {
            camera.follow_clamped(player_pos, bounds);
        } else {
            camera.clamp_to_bounds(bounds);
        }
        let raw_follow = follow_now.then_some(player_pos);
        self.last_camera_motion =
            CameraMotionDebug::record(previous, camera.position, raw_follow, follow_now);
        if self
            .debug
            .as_ref()
            .is_some_and(|d| d.ui.position_discontinuity_detector)
            && self.last_camera_motion.discontinuity
        {
            let tick = self.clock.tick().get();
            let cm = self.last_camera_motion;
            let ev = CollisionHistoryEvent {
                tick,
                subject: DiscSubject::Camera,
                axis: purgatory_simulation::CorrectionAxis::None,
                delta: cm.delta,
                correction: [0.0, 0.0],
                previous_position: cm.previous_position,
                position: camera.position,
                velocity: [0.0, 0.0],
                candidate: None,
                grounded_on: None,
                response_kind: purgatory_simulation::ResponseKind::None,
                discontinuity: true,
            };
            if let Some(debug) = &mut self.debug {
                debug.collision_history.push(ev);
                if debug.ui.log_discontinuities {
                    eprintln!(
                        "PURGATORY camera discontinuity tick={tick} prev={:?} delta={:?}",
                        cm.previous_position, cm.delta
                    );
                }
            }
        }
        renderer.set_camera(camera);
    }

    fn update_title(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let tick = self.clock.tick().get();
        let frames = self.renderer.as_ref().map_or(0, Renderer::frames_drawn);
        if tick == self.last_title_tick && !frames.is_multiple_of(30) {
            return;
        }
        self.last_title_tick = tick;
        let (width, height) = self
            .renderer
            .as_ref()
            .map(Renderer::surface_size)
            .unwrap_or((0, 0));
        window.set_title(&diagnostic_title(width, height, tick, frames));
    }

    fn handle_frame(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(network) = &mut self.network {
            network.poll();
        }
        self.advance_simulation();
        self.update_camera();

        let overlay_open = self.debug_overlay_visible();
        let camera = self.renderer.as_ref().map(Renderer::camera);
        let Some(camera) = camera else {
            return;
        };

        let mut quads = parallax_quads(&camera, self.world.bounds());
        quads.extend(scene_quads(&self.world));

        if overlay_open {
            let ui = self
                .debug
                .as_ref()
                .map(|d| d.ui.clone())
                .unwrap_or_default();
            quads.extend(footnote_debug_quads(&self.world, &ui));
            if ui.show_parallax_debug {
                quads.extend(parallax_debug_quads(&camera));
            }
        }

        let Some(window) = self.window.clone() else {
            return;
        };
        let fps = self.fps;
        let tick = self.clock.tick().get();
        let last_input = self.last_input;
        let mut snapshot = {
            let Some(renderer) = self.renderer.as_ref() else {
                return;
            };
            let cam = renderer.camera();
            DebugSnapshot::capture(
                &self.world,
                tick,
                renderer.frames_drawn(),
                renderer.surface_size(),
                fps,
                last_input,
                SnapshotExtras {
                    stage_name: "footnote_test_stage",
                    camera_position: cam.position,
                    viewport_width: cam.viewport_width,
                    viewport_height: cam.viewport_height,
                    parallax_far: PARALLAX_FAR,
                    parallax_mid: PARALLAX_MID,
                    parallax_near: PARALLAX_NEAR,
                    camera_motion: self.last_camera_motion,
                },
            )
        };
        snapshot.network = self
            .network
            .as_ref()
            .map(|net| net.view.snapshot())
            .unwrap_or_default();

        let mut actions = Vec::new();
        let status = {
            let Some(renderer) = self.renderer.as_mut() else {
                return;
            };
            let overlay = self.debug.as_mut();
            renderer.render(&quads, |pass| {
                let Some(overlay) = overlay else {
                    return Vec::new();
                };
                let (extras, emitted) = overlay.paint(&window, pass, &snapshot);
                actions = emitted;
                extras
            })
        };

        for action in actions {
            self.world.apply_debug_action(action);
        }

        if let Some(debug) = &mut self.debug {
            let connect = debug.ui.network_connect;
            let disconnect = debug.ui.network_disconnect;
            debug.ui.network_connect = false;
            debug.ui.network_disconnect = false;
            if let Some(network) = &self.network {
                if connect {
                    network.request_reconnect();
                }
                if disconnect {
                    network.request_disconnect();
                }
            }
        }

        match status {
            FrameStatus::Drawn | FrameStatus::Skipped => {}
            FrameStatus::NeedsReconfigure => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.reconfigure();
                }
            }
            FrameStatus::DeviceLost => {
                self.fatal = Some("GPU surface validation failure".to_string());
                event_loop.exit();
                return;
            }
        }
        self.update_title();
    }
}

fn scene_quads(world: &World) -> Vec<DrawQuad> {
    let mut quads = Vec::with_capacity(8);
    for view in world.iter_platforms() {
        let color = match view.platform.kind {
            PlatformKind::Solid => {
                if view.platform.half_extents[0] >= 7.0 {
                    SOLID_FLOOR_COLOR
                } else {
                    SOLID_PLATFORM_COLOR
                }
            }
            PlatformKind::OneWay => ONEWAY_COLOR,
            _ => SOLID_PLATFORM_COLOR,
        };
        quads.push(aabb_quad(view.aabb(), color));
    }
    if let Some(player) = world.player_body() {
        quads.push(aabb_quad(player.aabb(), PLAYER_COLOR));
    }
    let b = world.bounds();
    quads.push(DrawQuad {
        center: [b.min_x + 0.6, b.max_y - 0.6],
        size: [0.35, 0.35],
        color: MARKER_COLOR,
    });
    quads
}

fn aabb_quad(aabb: Aabb, color: [f32; 4]) -> DrawQuad {
    DrawQuad {
        center: aabb.center,
        size: aabb.size(),
        color,
    }
}

impl ApplicationHandler for ClientApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = match event_loop.create_window(window_attributes()) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                self.fatal = Some(format!("create window: {err}"));
                event_loop.exit();
                return;
            }
        };

        match Renderer::new(window.clone()) {
            Ok(renderer) => {
                let camera: Camera = renderer.camera();
                let origin_ndc = camera.world_to_ndc([0.0, 0.0]);
                println!(
                    "PURGATORY client window ready adapter='{}' backend={:?} {}x{} format={:?} world_viewport={:.2}x{:.2} logical_height={:.1} origin_ndc=({:.2},{:.2})",
                    renderer.adapter_name(),
                    renderer.backend(),
                    renderer.surface_size().0,
                    renderer.surface_size().1,
                    renderer.surface_format(),
                    camera.viewport_width,
                    camera.viewport_height,
                    FOOTNOTE_LOGICAL_HEIGHT,
                    origin_ndc[0],
                    origin_ndc[1],
                );
                println!(
                    "PURGATORY controls: A/Left=MoveLeft D/Right=MoveRight S/Down=Down Space=Jump | Down+Jump=drop through OneWay | camera follows player"
                );
                println!(
                    "PURGATORY debug overlay: Backquote/~ toggles tabs, gizmos, camera follow, slow-mo"
                );
                let overlay = DebugOverlay::new(
                    &window,
                    OverlayInit {
                        device: renderer.device(),
                        surface_format: renderer.surface_format(),
                        max_texture_side: renderer.max_texture_dimension_2d() as usize,
                    },
                );
                self.debug = Some(overlay);
                self.renderer = Some(renderer);
                self.window = Some(window);
                self.last_instant = Instant::now();
            }
            Err(err) => {
                self.fatal = Some(err);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && is_debug_toggle(key)
        {
            self.toggle_debug_overlay();
            window.request_redraw();
            return;
        }

        let overlay_open = self.debug_overlay_visible();
        if overlay_open && let Some(overlay) = &mut self.debug {
            overlay.on_window_event(&window, &event);
        }

        match event {
            WindowEvent::CloseRequested => {
                println!("PURGATORY client closing");
                self.network = None;
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if is_usable_surface(size.width, size.height)
                    && let Some(renderer) = self.renderer.as_mut()
                {
                    renderer.resize(size.width, size.height);
                    // Keep camera clamped after aspect change.
                    let bounds = self.world.bounds();
                    let mut cam = renderer.camera();
                    cam.clamp_to_bounds(bounds);
                    renderer.set_camera(cam);
                }
                window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let text_like = self
                    .debug
                    .as_ref()
                    .is_some_and(DebugOverlay::wants_keyboard_for_text);
                let pressed = event.state == ElementState::Pressed;
                if gameplay_receives_keyboard(overlay_open, text_like, pressed) {
                    self.actions.apply_key_event(&event);
                }
            }
            WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. } => {
                let _gameplay_mouse = gameplay_receives_pointer(
                    overlay_open,
                    self.debug.as_ref().is_some_and(DebugOverlay::wants_pointer),
                );
            }
            WindowEvent::RedrawRequested => {
                self.handle_frame(event_loop);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::platform::{DEV_WINDOW_HEIGHT, DEV_WINDOW_WIDTH};
    use crate::renderer::{Camera, DEFAULT_LOGICAL_HEIGHT, is_usable_surface};
    use purgatory_simulation::{
        FootnoteConfig, PlayerInput, SimulationClock, TICK_DURATION, World,
    };
    use std::time::Duration;

    #[test]
    fn workspace_crates_are_linked() {
        assert!(!purgatory_common::version().is_empty());
        assert!(!purgatory_protocol::version().is_empty());
        assert!(!purgatory_content::version().is_empty());
        assert!(!purgatory_simulation::version().is_empty());
    }

    #[test]
    fn simulation_advances_without_a_rendered_frame() {
        let mut clock = SimulationClock::new();
        assert_eq!(clock.advance(Duration::from_millis(16)).ticks_executed, 0);
        assert_eq!(clock.advance(Duration::from_millis(20)).ticks_executed, 1);
        assert_eq!(clock.tick().get(), 1);
    }

    #[test]
    fn clock_ticks_drive_world_once_each() {
        let mut clock = SimulationClock::new();
        let mut world = World::footnote_test_stage();
        // Open mid-floor stretch — spawn sits near slope/left boundary.
        if let Some((t, p)) = world.player_parts_mut() {
            t.position[0] = 0.0;
            p.velocity = [0.0, 0.0];
        }
        let start_x = world.player_body().expect("player").position[0];
        let update = clock.advance(Duration::from_secs(1));
        assert_eq!(update.ticks_executed, 30);
        let dt = TICK_DURATION.as_secs_f32();
        let input = PlayerInput::from_buttons(false, true, false);
        for _ in 0..update.ticks_executed {
            world.tick(dt, input);
        }
        let traveled = world.player_body().expect("player").position[0] - start_x;
        let max = FootnoteConfig::DEFAULT.max_ground_speed;
        assert!(
            traveled > max - 0.5 && traveled <= max + 0.05,
            "traveled {traveled}, expected near {max} with accel ramp"
        );
    }

    #[test]
    fn half_time_scale_advances_about_half_the_ticks() {
        let mut full = SimulationClock::new();
        let mut half = SimulationClock::new();
        let wall = Duration::from_secs(1);
        assert_eq!(full.advance(wall).ticks_executed, 30);
        let scaled = Duration::from_secs_f64(wall.as_secs_f64() * 0.5);
        let ticks = half.advance(scaled).ticks_executed;
        assert!(
            (14..=16).contains(&ticks),
            "0.5x over 1s wall should be ~15 ticks, got {ticks}"
        );
    }

    #[test]
    fn quarter_time_scale_advances_about_quarter_ticks() {
        let mut clock = SimulationClock::new();
        let wall = Duration::from_secs(1);
        let scaled = Duration::from_secs_f64(wall.as_secs_f64() * 0.25);
        let ticks = clock.advance(scaled).ticks_executed;
        assert!(
            (7..=8).contains(&ticks),
            "0.25x over 1s wall should be ~7–8 ticks, got {ticks}"
        );
    }

    #[test]
    fn development_viewport_matches_window_aspect() {
        let camera = Camera::from_physical_pixels(DEV_WINDOW_WIDTH, DEV_WINDOW_HEIGHT)
            .expect("dev size is usable");
        assert!((camera.viewport_width - 16.0).abs() < f32::EPSILON);
        assert!((camera.viewport_height - DEFAULT_LOGICAL_HEIGHT).abs() < f32::EPSILON);
        assert!(is_usable_surface(DEV_WINDOW_WIDTH, DEV_WINDOW_HEIGHT));
        assert!(!is_usable_surface(0, DEV_WINDOW_HEIGHT));
    }

    #[test]
    fn scene_quads_follow_simulation_aabbs() {
        let world = World::dev_stage();
        let quads = super::scene_quads(&world);
        assert!(quads.len() >= 5);
        let player = world.player_body().expect("player");
        let player_quad = quads
            .iter()
            .find(|quad| quad.center == player.position)
            .expect("player quad");
        assert_eq!(player_quad.size, player.aabb().size());
    }
}
