//! Domain diagnostics composition. Read-only. Not commands. Not a second protocol.

use super::camera_debug::CameraMotionDebug;
use super::entity_inspector::InspectorView;
use super::interact_status::InteractStatusView;
use super::snapshot::{
    PhysicsDiagnostics, RemoteMotionProbe, SkeletonInspectDebug, WorldRosterDiagnostics,
};
use crate::display::DisplayDebug;
use crate::interp::InterpDiagnostics;
use crate::jitter_forensics::JitterSummary;
use crate::network::{ImpairmentMetricsSnapshot, NetworkSnapshot};
use crate::prediction::PredictionDiagnostics;
use crate::renderer::rf_diag::RfVertexProof;

/// Runtime / display now-state. Reuses [`DisplayDebug`].
#[derive(Clone, Copy, Debug, Default)]
pub struct RuntimeDiagnostics {
    pub frames: u64,
    pub tick: u64,
    pub tick_rate_hz: u32,
    pub window_width: u32,
    pub window_height: u32,
    pub display: DisplayDebug,
    pub fps: f32,
}

/// Stage, observer address, inspector, AOI presentation rows.
#[derive(Debug, Default)]
pub struct WorldDiagnostics {
    pub roster: WorldRosterDiagnostics,
    pub stage_name: &'static str,
    pub map_id: u32,
    pub map_debug_name: String,
    pub observer_address: String,
    pub observer_map: u32,
    pub observer_channel: u32,
    pub observer_instance: u32,
    pub transition_banner: String,
    pub transition_missing: String,
    pub transition_stalled: bool,
    pub input_gate_label: String,
    pub input_movement_neutral: bool,
    pub content_registry_count: u32,
    pub content_map_labels: Vec<String>,
    pub observer_entity: Option<String>,
    pub observer_enter_bounds: String,
    pub observer_leave_bounds: String,
    pub aoi_candidates: Option<u16>,
    pub aoi_known: Option<u16>,
    pub aoi_want_enter: Option<u16>,
    pub aoi_want_leave: Option<u16>,
    pub replica_entity_rows: Vec<String>,
    pub replica_recent_left: Vec<String>,
    pub replica_label_world: Vec<(String, [f32; 2], u8)>,
    pub inspector: InspectorView,
}

/// Network + replica + interp + prediction + interaction. Reuses subsystem diagnostics types.
#[derive(Debug, Default)]
pub struct NetworkDiagnostics {
    pub lifecycle: NetworkSnapshot,
    pub inventory: Vec<purgatory_protocol::InventoryEntry>,
    pub net_input_seq: u32,
    pub net_input_sent: u64,
    pub net_move_axis: i8,
    pub net_jump: bool,
    pub net_down: bool,
    pub replica_seq: Option<u32>,
    pub replica_tick: u64,
    pub replica_entities: u32,
    pub replica_epoch: u32,
    pub replica_frame_enters: u32,
    pub replica_frame_updates: u32,
    pub replica_frame_leaves: u32,
    pub replica_total_enters: u64,
    pub replica_total_updates: u64,
    pub replica_total_leaves: u64,
    pub replica_local: Option<String>,
    pub replica_stale: u64,
    pub replica_duplicate: u64,
    pub replica_malformed: u64,
    pub replica_age_ms: Option<u64>,
    pub interact_ui: String,
    pub interact_status: InteractStatusView,
    pub interact_nearest: Option<String>,
    pub interact_nearest_distance: Option<f32>,
    pub interact_nearest_portal: Option<String>,
    pub interact_nearest_portal_distance: Option<f32>,
    pub portal_eligible: bool,
    pub interact_last_request: String,
    pub interact_last_result: String,
    pub replica_interactables: Vec<String>,
    pub replica_portals: Vec<String>,
    pub replica_player_pos: Option<[f32; 2]>,
    pub nearest_portal_pos: Option<[f32; 2]>,
    pub interp: InterpDiagnostics,
    pub remote_motion: RemoteMotionProbe,
    pub pred: PredictionDiagnostics,
    pub impairment: ImpairmentMetricsSnapshot,
}

/// Camera follow + optional jitter summary.
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraDiagnostics {
    pub position: [f32; 2],
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub parallax_far: f32,
    pub parallax_mid: f32,
    pub parallax_near: f32,
    pub motion: CameraMotionDebug,
    pub presented_player_pos: Option<[f32; 2]>,
    pub desired: [f32; 2],
    pub deadzone_half_x: f32,
    pub deadzone_half_y: f32,
    pub smooth_time_x: f32,
    pub smooth_time_y: f32,
    pub following_x: bool,
    pub following_y: bool,
    pub jitter: JitterSummary,
}

/// Skeleton inspect + character-presentation counts.
#[derive(Debug, Default)]
pub struct PresentationDiagnostics {
    pub skeleton_inspect: Option<SkeletonInspectDebug>,
    pub characters: usize,
    pub bound: usize,
    pub hidden: u16,
    pub missing: Vec<String>,
}

/// Read-only frame composed from subsystem diagnostics. No commands, no view toggles.
#[derive(Debug, Default)]
pub struct DiagnosticsFrame {
    pub runtime: RuntimeDiagnostics,
    pub physics: PhysicsDiagnostics,
    pub world: WorldDiagnostics,
    pub network: NetworkDiagnostics,
    pub camera: CameraDiagnostics,
    pub presentation: PresentationDiagnostics,
    pub rf: Option<RfVertexProof>,
}

impl DiagnosticsFrame {
    /// Cheap stub for Connection Frontend egui scale when no diagnostics consumer is active.
    #[must_use]
    pub fn display_only(display: DisplayDebug) -> Self {
        Self {
            runtime: RuntimeDiagnostics {
                display,
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_simulation::{PlayerInput, TICK_RATE_HZ, World};

    #[test]
    fn frame_preserves_physics_roster_interp_and_pred() {
        let world = World::dev_stage();
        let (physics, roster) =
            crate::debug::WorldRosterDiagnostics::from_world(&world, PlayerInput::idle());
        let id = physics.player.expect("player").id;
        let frame = DiagnosticsFrame {
            runtime: RuntimeDiagnostics {
                frames: 3,
                tick: 7,
                tick_rate_hz: TICK_RATE_HZ,
                fps: 60.0,
                ..Default::default()
            },
            physics,
            world: WorldDiagnostics {
                roster,
                stage_name: "content",
                ..Default::default()
            },
            network: NetworkDiagnostics {
                interp: crate::interp::InterpDiagnostics {
                    holds: 4,
                    estimated_server_tick: 9.0,
                    ..Default::default()
                },
                pred: crate::prediction::PredictionDiagnostics {
                    lead_error: Some(1.25),
                    last_snap_reason: Some("test"),
                    pending_window_stall_ticks: 2,
                    ..Default::default()
                },
                ..Default::default()
            },
            camera: CameraDiagnostics {
                position: [1.0, 2.0],
                ..Default::default()
            },
            presentation: PresentationDiagnostics::default(),
            rf: None,
        };
        assert_eq!(frame.runtime.tick, 7);
        assert_eq!(frame.runtime.frames, 3);
        assert_eq!(frame.physics.player.expect("player").id, id);
        assert_eq!(frame.network.interp.holds, 4);
        assert!((frame.network.interp.estimated_server_tick - 9.0).abs() < f64::EPSILON);
        assert_eq!(frame.network.pred.lead_error, Some(1.25));
        assert_eq!(frame.network.pred.last_snap_reason, Some("test"));
        assert_eq!(frame.network.pred.pending_window_stall_ticks, 2);
        assert_eq!(frame.camera.position, [1.0, 2.0]);
        assert_eq!(frame.world.roster.entity_count, world.len());
    }

    #[test]
    fn display_only_fills_runtime_display() {
        let display = DisplayDebug {
            ui_scale: 1.25,
            ..Default::default()
        };
        let frame = DiagnosticsFrame::display_only(display);
        assert!((frame.runtime.display.ui_scale - 1.25).abs() < f32::EPSILON);
        assert!(frame.physics.player.is_none());
    }
}
