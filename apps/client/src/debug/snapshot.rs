//! Lightweight presentation snapshot. Not authoritative world state.

use purgatory_simulation::{
    ContactEvent, EntityId, EntityKind, PlatformKind, PlayerInput, PlayerMotionDebug, TICK_RATE_HZ,
    World, WorldBounds,
};

use super::camera_debug::CameraMotionDebug;
use crate::network::NetworkSnapshot;

/// Read-only player fields copied for one debug frame.
#[derive(Clone, Copy, Debug)]
pub struct PlayerDebug {
    pub id: EntityId,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub grounded: bool,
    pub grounded_on: Option<EntityId>,
    pub ignored_platform: Option<EntityId>,
    pub last_contact: ContactEvent,
    pub grounded_kind: Option<PlatformKind>,
}

/// FOOTNOTE section of the debug snapshot.
#[derive(Clone, Copy, Debug)]
pub struct FootnoteDebug {
    pub grounded: bool,
    pub grounded_on: Option<EntityId>,
    pub platform_kind: Option<PlatformKind>,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub horizontal_speed: f32,
    pub move_axis: i8,
    pub down_held: bool,
    pub ignored_platform: Option<EntityId>,
    pub last_contact: ContactEvent,
}

/// Read-only copy of values the debug overlay may display.
#[derive(Clone, Debug)]
pub struct DebugSnapshot {
    pub frames: u64,
    pub tick: u64,
    pub tick_rate_hz: u32,
    pub window_width: u32,
    pub window_height: u32,
    pub fps: f32,
    pub entity_count: u32,
    pub player_count: u32,
    pub platform_count: u32,
    pub stage_name: &'static str,
    pub world_bounds: WorldBounds,
    pub camera_position: [f32; 2],
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub parallax_far: f32,
    pub parallax_mid: f32,
    pub parallax_near: f32,
    pub player: Option<PlayerDebug>,
    pub footnote: Option<FootnoteDebug>,
    pub motion: PlayerMotionDebug,
    pub camera_motion: CameraMotionDebug,
    pub entities: Vec<(EntityId, EntityKind)>,
    pub network: NetworkSnapshot,
    pub net_input_seq: u32,
    pub net_input_sent: u64,
    pub net_move_axis: i8,
    pub net_jump: bool,
    pub net_down: bool,
    pub replica_seq: Option<u32>,
    pub replica_tick: u64,
    pub replica_entities: u32,
    pub replica_local: Option<String>,
    pub replica_stale: u64,
    pub replica_duplicate: u64,
    pub replica_malformed: u64,
    pub replica_age_ms: Option<u64>,
    pub interp_enabled: bool,
    pub interp_delay_ticks: u64,
    pub interp_delay_ms: u64,
    pub interp_history_depth: u32,
    pub interp_estimated_tick: f64,
    pub interp_render_tick: f64,
    pub interp_bracket_a: Option<u64>,
    pub interp_bracket_b: Option<u64>,
    pub interp_alpha: f32,
    pub interp_holds: u64,
    pub interp_snaps: u64,
    pub pred_enabled: bool,
    pub pred_active: bool,
    pub pred_auth_pos: Option<[f32; 2]>,
    pub pred_auth_vel: Option<[f32; 2]>,
    pub pred_pos: Option<[f32; 2]>,
    pub pred_vel: Option<[f32; 2]>,
    pub pred_error: Option<f32>,
    pub pred_lead_error: Option<f32>,
    pub pred_aligned_error: Option<f32>,
    pub pred_aligned_dx: Option<f32>,
    pub pred_aligned_dy: Option<f32>,
    pub pred_best_offset: Option<i64>,
    pub pred_tick: u64,
    pub pred_auth_tick: u64,
    pub pred_best_match_tick: Option<u64>,
    pub pred_resets: u64,
    pub pred_drift_corrections: u64,
    pub pred_aligned_divergence: u32,
    pub pred_max_aligned: f32,
    pub pred_last_snap: Option<String>,
    pub pred_pending: u32,
    pub pred_ack: u32,
    pub pred_epoch: u16,
    pub pred_debt: u16,
    pub pred_cancel_pending: bool,
}

pub struct SnapshotExtras {
    pub stage_name: &'static str,
    pub camera_position: [f32; 2],
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub parallax_far: f32,
    pub parallax_mid: f32,
    pub parallax_near: f32,
    pub camera_motion: CameraMotionDebug,
}

impl DebugSnapshot {
    #[must_use]
    pub fn capture(
        world: &World,
        tick: u64,
        frames: u64,
        window_size: (u32, u32),
        fps: f32,
        input: PlayerInput,
        extras: SnapshotExtras,
    ) -> Self {
        let player = world.player_body().map(|body| {
            let grounded_kind = body
                .grounded_on
                .and_then(|id| world.get_platform(id).map(|(_, p)| p.kind));
            PlayerDebug {
                id: body.id,
                position: body.position,
                velocity: body.velocity,
                grounded: body.grounded,
                grounded_on: body.grounded_on,
                ignored_platform: body.ignored_platform,
                last_contact: body.last_contact,
                grounded_kind,
            }
        });
        let footnote = player.map(|p| FootnoteDebug {
            grounded: p.grounded,
            grounded_on: p.grounded_on,
            platform_kind: p.grounded_kind,
            position: p.position,
            velocity: p.velocity,
            horizontal_speed: p.velocity[0].abs(),
            move_axis: input.move_axis,
            down_held: input.down_held,
            ignored_platform: p.ignored_platform,
            last_contact: p.last_contact,
        });
        Self {
            frames,
            tick,
            tick_rate_hz: TICK_RATE_HZ,
            window_width: window_size.0,
            window_height: window_size.1,
            fps,
            entity_count: world.len(),
            player_count: world.iter_kind(EntityKind::Player).count() as u32,
            platform_count: world.iter_kind(EntityKind::Platform).count() as u32,
            stage_name: extras.stage_name,
            world_bounds: world.bounds(),
            camera_position: extras.camera_position,
            viewport_width: extras.viewport_width,
            viewport_height: extras.viewport_height,
            parallax_far: extras.parallax_far,
            parallax_mid: extras.parallax_mid,
            parallax_near: extras.parallax_near,
            player,
            footnote,
            motion: world.last_motion_debug(),
            camera_motion: extras.camera_motion,
            entities: world
                .iter()
                .map(|id| (id, world.kind(id).expect("live entity has a kind")))
                .collect(),
            network: NetworkSnapshot::default(),
            net_input_seq: 0,
            net_input_sent: 0,
            net_move_axis: input.move_axis,
            net_jump: input.jump_pressed,
            net_down: input.down_held,
            replica_seq: None,
            replica_tick: 0,
            replica_entities: 0,
            replica_local: None,
            replica_stale: 0,
            replica_duplicate: 0,
            replica_malformed: 0,
            replica_age_ms: None,
            interp_enabled: false,
            interp_delay_ticks: 0,
            interp_delay_ms: 0,
            interp_history_depth: 0,
            interp_estimated_tick: 0.0,
            interp_render_tick: 0.0,
            interp_bracket_a: None,
            interp_bracket_b: None,
            interp_alpha: 0.0,
            interp_holds: 0,
            interp_snaps: 0,
            pred_enabled: false,
            pred_active: false,
            pred_auth_pos: None,
            pred_auth_vel: None,
            pred_pos: None,
            pred_vel: None,
            pred_error: None,
            pred_lead_error: None,
            pred_aligned_error: None,
            pred_aligned_dx: None,
            pred_aligned_dy: None,
            pred_best_offset: None,
            pred_tick: 0,
            pred_auth_tick: 0,
            pred_best_match_tick: None,
            pred_resets: 0,
            pred_drift_corrections: 0,
            pred_aligned_divergence: 0,
            pred_max_aligned: 0.0,
            pred_last_snap: None,
            pred_pending: 0,
            pred_ack: 0,
            pred_epoch: 0,
            pred_debt: 0,
            pred_cancel_pending: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_simulation::World;

    fn extras() -> SnapshotExtras {
        SnapshotExtras {
            stage_name: "dev_stage",
            camera_position: [0.0, 0.0],
            viewport_width: 16.0,
            viewport_height: 9.0,
            parallax_far: 0.15,
            parallax_mid: 0.4,
            parallax_near: 0.7,
            camera_motion: CameraMotionDebug::default(),
        }
    }

    #[test]
    fn snapshot_reads_world_without_owning_it() {
        let world = World::dev_stage();
        let body = world.player_body().expect("player");
        let snapshot = DebugSnapshot::capture(
            &world,
            7,
            3,
            (1280, 720),
            60.0,
            PlayerInput::idle(),
            extras(),
        );
        assert_eq!(snapshot.tick, 7);
        assert_eq!(snapshot.entity_count, world.len());
        assert_eq!(snapshot.platform_count, 4);
        let player = snapshot.player.expect("player");
        assert_eq!(player.id, body.id);
        let footnote = snapshot.footnote.expect("footnote");
        assert!(footnote.grounded);
        assert_eq!(footnote.move_axis, 0);
    }

    #[test]
    fn snapshot_module_has_no_windowing_tokens() {
        let src = include_str!("snapshot.rs");
        assert!(!src.contains(concat!("use w", "gpu")));
        assert!(!src.contains(concat!("use w", "init")));
        assert!(!src.contains(concat!("use e", "gui")));
    }
}
