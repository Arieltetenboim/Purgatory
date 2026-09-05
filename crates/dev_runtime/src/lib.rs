//! GUI-independent Developer Hub orchestration.
//!
//! This crate must not depend on egui, winit, wgpu, Quinn, or `purgatory-client`.

pub mod backend;
pub mod config;
pub mod health;
pub mod identity;
pub mod instance;
pub mod job;
pub mod load;
pub mod log_buffer;
pub mod log_tail;
pub mod metrics_series;
pub mod paths;
pub mod phase78;
pub mod process;
pub mod session;
pub mod settings;
pub mod validation;

pub use backend::{CapturedOutput, ProcessBackend, SpawnSpec, StdProcessBackend};
pub use config::{
    ACTIVITY_LOG_CAP, ACTIVITY_VIEW_LINES, HUB_MUTEX_NAME, LISTEN_HOST, LISTEN_PORT, METRICS_PORT,
};
pub use health::{HealthSource, StdHealthSource};
pub use identity::CodeIdentity;
pub use job::{CommandOutcome, HubCommand, JobOp, JobPhase};
pub use load::{LoadDuration, LoadLastResult, LoadProfile, LoadScenario, LoadSpec, LoadState};
pub use metrics_series::{
    METRICS_SERIES_CAP, MetricsSeries, MetricsSeriesSample, read_metrics_series,
};
pub use paths::WorkspacePaths;
pub use phase78::{
    Phase78CellBrief, Phase78FindingBrief, Phase78GateBrief, Phase78ReadStatus, capacity_78_root,
    latest_phase78_gate_dir, read_latest_phase78_gate, read_phase78_gate_summary, strip_utf8_bom,
};
pub use process::{
    CheckStatus, ListenerDiag, ProcessLifetime, ProcessOrigin, ServerLaunchOptions, ServerState,
};
pub use purgatory_common::{CapacityLiveSnapshot, SaturationClass, TickOwnerId};
pub use session::{HubSession, HubSnapshot, LiveHubSession};
pub use settings::{BuildProfile, LogLevel};
pub use validation::{
    RunSummaryBrief, StatusReasonView, ValidationDuration, ValidationLastResult,
    ValidationLiveStatus, ValidationPreset, ValidationSpec, ValidationState, read_capacity_live,
};
