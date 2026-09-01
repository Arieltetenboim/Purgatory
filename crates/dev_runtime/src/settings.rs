//! Hub settings that apply to newly spawned processes.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BuildProfile {
    #[default]
    Debug,
    Release,
}

impl BuildProfile {
    pub const ALL: [Self; 2] = [Self::Debug, Self::Release];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }

    #[must_use]
    pub fn cargo_release_flag(self) -> bool {
        matches!(self, Self::Release)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogLevel {
    #[default]
    Default,
    Debug,
    Trace,
}

impl LogLevel {
    pub const ALL: [Self; 3] = [Self::Default, Self::Debug, Self::Trace];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Debug => "Debug",
            Self::Trace => "Trace",
        }
    }

    /// Extra env for **new** processes only (PowerShell Get-ChildEnvironment).
    #[must_use]
    pub fn child_env(self) -> Vec<(String, String)> {
        match self {
            Self::Default => Vec::new(),
            Self::Debug => vec![
                (
                    "RUST_LOG".to_string(),
                    "debug,quinn=debug,quinn_proto=warn,rustls=warn".to_string(),
                ),
                ("PURGATORY_NET_LOG".to_string(), "1".to_string()),
            ],
            Self::Trace => vec![
                (
                    "RUST_LOG".to_string(),
                    "trace,quinn=debug,quinn_proto=debug,rustls=warn".to_string(),
                ),
                ("PURGATORY_NET_LOG".to_string(), "1".to_string()),
                ("PURGATORY_NET_VERBOSE".to_string(), "1".to_string()),
            ],
        }
    }
}
