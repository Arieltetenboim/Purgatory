//! Development-only debug overlay. Not production game UI.
//!
//! Lives in the client. Authoritative simulation is read through a snapshot
//! and mutated only via [`purgatory_simulation::DebugAction`].
//!
//! A later release profile may compile this module out (for example
//! `cfg(debug_assertions)` or a cargo feature). That switch is not wired yet.

pub(crate) mod agent_log;
pub(crate) mod aoi_view;
mod camera_debug;
mod capture;
mod collision_history;
pub(crate) mod entity_inspector;
pub(crate) mod interact_status;
mod overlay;
mod sections;
mod snapshot;
mod ui_state;
mod viz;

#[allow(unused_imports)] // public debug API
pub use camera_debug::{CameraClampReason, CameraMotionDebug};
pub use capture::{gameplay_receives_keyboard, gameplay_receives_pointer};
pub use collision_history::{CollisionHistoryEvent, DiscSubject};
pub use overlay::{ConnectionPaint, DebugOverlay, OverlayInit, is_debug_toggle};
pub use snapshot::{DebugSnapshot, SnapshotExtras};
#[allow(unused_imports)] // public debug API
pub use ui_state::DebugUiState;
pub use viz::{aoi_entity_debug_quads, camera_deadzone_quads, footnote_debug_quads};
