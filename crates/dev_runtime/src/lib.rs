//! GUI-independent Developer Hub orchestration.
//!
//! This crate must not depend on egui, winit, wgpu, Quinn, or `purgatory-client`.

pub mod backend;
pub mod config;
pub mod health;
pub mod identity;
pub mod instance;
pub mod job;
pub mod log_buffer;
pub mod paths;
pub mod process;
pub mod session;
pub mod validation;

pub use backend::{CapturedOutput, ProcessBackend, SpawnSpec, StdProcessBackend};
pub use config::{
    ACTIVITY_LOG_CAP, ACTIVITY_VIEW_LINES, HUB_MUTEX_NAME, LISTEN_HOST, LISTEN_PORT, METRICS_PORT,
};
pub use health::{HealthSource, StdHealthSource};
pub use identity::CodeIdentity;
pub use job::{CommandOutcome, HubCommand, JobOp, JobPhase};
pub use paths::WorkspacePaths;
pub use process::{
    CheckStatus, ListenerDiag, ProcessLifetime, ProcessOrigin, ServerLaunchOptions, ServerState,
};
pub use session::{HubSession, HubSnapshot, LiveHubSession};
pub use validation::{
    ValidationDuration, ValidationLastResult, ValidationLiveStatus, ValidationPreset,
    ValidationSpec, ValidationState,
};
