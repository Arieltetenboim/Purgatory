//! Explicit lifecycle jobs. One running operation at a time; late results are ignored.

use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct JobId(pub u64);

impl JobId {
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobOp {
    Build,
    Start,
    Probe,
    Stop,
    Validate,
}

impl JobOp {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Start => "start",
            Self::Probe => "probe",
            Self::Stop => "stop",
            Self::Validate => "validate",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobPhase {
    Idle,
    Running { id: JobId, op: JobOp },
    Cancelling { id: JobId, op: JobOp },
}

impl JobPhase {
    #[must_use]
    pub fn running_id(self) -> Option<JobId> {
        match self {
            Self::Idle => None,
            Self::Running { id, .. } | Self::Cancelling { id, .. } => Some(id),
        }
    }

    #[must_use]
    pub fn blocks_start(self) -> bool {
        matches!(
            self,
            Self::Running {
                op: JobOp::Build | JobOp::Start | JobOp::Probe | JobOp::Validate,
                ..
            } | Self::Cancelling { .. }
        )
    }

    #[must_use]
    pub fn is_stop(self) -> bool {
        matches!(
            self,
            Self::Running {
                op: JobOp::Stop,
                ..
            } | Self::Cancelling {
                op: JobOp::Stop,
                ..
            }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HubCommand {
    Start,
    Stop,
    Restart,
    StartValidation {
        spec: crate::validation::ValidationSpec,
    },
    StopValidation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandOutcome {
    Accepted,
    Ignored,
}
