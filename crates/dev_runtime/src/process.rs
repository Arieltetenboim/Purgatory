//! Discovery, origin, and tracking are distinct. Do not treat a scan hit as a spawned process.

use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessOrigin {
    /// This session started the process.
    Spawned,
    /// This session attached to a process it did not start.
    Adopted,
}

/// Workspace `target\` scan result. Not tracked until explicitly spawned or adopted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredProcess {
    pub pid: u32,
    pub exe_path: PathBuf,
}

/// A process this session tracks. Tracking is not the same as OS exclusive ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackedProcess {
    pub pid: u32,
    pub origin: ProcessOrigin,
    pub exe_path: PathBuf,
}

impl ProcessOrigin {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Spawned => "Spawned",
            Self::Adopted => "Adopted",
        }
    }
}

/// Whether a child must die with the Hub session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessLifetime {
    /// Cargo / probe: cancellable; Hub Drop may kill.
    Session,
    /// Dedicated server: survives Hub exit.
    Detached,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerState {
    Stopped,
    Building,
    Starting,
    Verifying,
    Ready,
    Degraded,
    Stopping,
    Failed,
}

impl ServerState {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "Stopped",
            Self::Building => "Building",
            Self::Starting => "Starting",
            Self::Verifying => "Verifying",
            Self::Ready => "Ready",
            Self::Degraded => "Degraded",
            Self::Stopping => "Stopping",
            Self::Failed => "Failed",
        }
    }

    #[must_use]
    pub fn uses_live_process(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Verifying | Self::Ready | Self::Degraded
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    Unknown,
    Pass,
    Fail,
}

impl CheckStatus {
    #[must_use]
    pub fn as_ui(self) -> &'static str {
        match self {
            Self::Unknown => "-",
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenerDiag {
    Unknown,
    Yes,
    No,
}

impl ListenerDiag {
    #[must_use]
    pub fn as_ui(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Yes => "yes",
            Self::No => "no",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerLaunchOptions {
    pub extra_env: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildReason {
    StartServer,
    ProbePrep,
    ValidatePrep,
}

impl BuildReason {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StartServer => "start-server",
            Self::ProbePrep => "probe-prep",
            Self::ValidatePrep => "runtime-val-prep",
        }
    }
}
