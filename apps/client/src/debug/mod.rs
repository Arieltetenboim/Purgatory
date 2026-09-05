//! Development-only debug overlay. Not production game UI.
//!
//! Lives in the client. Read-only observation is [`DiagnosticsFrame`].
//! Privileged mutations go through [`DebugCommand`]. Simulation `World`
//! mutations still use [`purgatory_simulation::DebugAction`] internally.
//!
//! A later release profile may compile this module out (for example
//! `cfg(debug_assertions)` or a cargo feature). That switch is not wired yet.

pub(crate) mod aoi_view;
mod camera_debug;
mod capture;
mod chrome;
mod collision_history;
mod command;
mod consumer;
pub(crate) mod entity_inspector;
mod frame;
pub(crate) mod interact_status;
mod overlay;
mod sections;
mod snapshot;
mod ui_state;
mod viz;

#[allow(unused_imports)] // public debug API
pub use camera_debug::{CameraClampReason, CameraMotionDebug};
pub use capture::{gameplay_receives_keyboard, gameplay_receives_pointer};
pub use chrome::has_persistent_dev_warnings;
pub use collision_history::{CollisionHistoryEvent, DiscSubject};
pub use command::DebugCommand;
pub use consumer::DiagnosticsDemand;
pub use frame::{
    CameraDiagnostics, DiagnosticsFrame, NetworkDiagnostics, PresentationDiagnostics,
    RuntimeDiagnostics, WorldDiagnostics,
};
pub use overlay::{ConnectionPaint, DebugOverlay, OverlayInit, is_debug_toggle};
#[allow(unused_imports)] // public debug API
pub use snapshot::{
    PhysicsDiagnostics, RemoteMotionProbe, SkeletonInspectDebug, WorldRosterDiagnostics,
};
#[allow(unused_imports)] // public debug API
pub use ui_state::{
    AnimationProofMode, DebugUiState, RESET_TO_SPAWN_FLASH, reset_action_flash,
    reset_player_uses_replica, sanitize_debug_move_speed,
};
pub use viz::{
    aoi_entity_debug_quads, append_debug_gizmos, camera_deadzone_quads, footnote_debug_quads,
};
