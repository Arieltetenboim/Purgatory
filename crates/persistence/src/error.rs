use std::fmt;
use std::io;
use std::path::PathBuf;

/// Expected create rejections, distinct from corrupt state and storage failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreateCharacterRejection {
    InvalidName(purgatory_common::CharacterNameError),
    RosterFull,
    NameTaken,
}

/// Persistence failure. Never panic on corrupt files.
#[derive(Debug)]
pub enum PersistError {
    CreateRejected(CreateCharacterRejection),
    CharacterIdsExhausted,
    CompatibilityNamesExhausted,
    Migration {
        path: PathBuf,
        reason: String,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Json {
        path: PathBuf,
        source: String,
    },
    Schema {
        path: PathBuf,
        found: u32,
    },
    Corrupt {
        path: PathBuf,
        reason: String,
    },
    /// Committed rows disagree with each other, or a retry key was reused for
    /// a different command.
    Integrity {
        path: PathBuf,
        reason: String,
    },
    /// Item, ability, or narrative content rules reject the command.
    ContentRejected {
        path: PathBuf,
        reason: String,
    },
    /// Expected revision, live owner, or uniqueness check lost the race.
    Conflict {
        path: PathBuf,
        reason: String,
    },
    /// PostgreSQL reported a failure. The text must not include credentials.
    Storage {
        reason: String,
    },
    /// The durable item-id high water cannot advance.
    ItemIdsExhausted,
    /// The caller's character-lease generation is not the live authority.
    /// This is a definite rejection, not an unknown commit.
    LeaseLost,
}

impl PersistError {
    #[must_use]
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    #[must_use]
    pub fn json(path: impl Into<PathBuf>, source: impl fmt::Display) -> Self {
        Self::Json {
            path: path.into(),
            source: source.to_string(),
        }
    }

    #[must_use]
    pub fn schema(path: impl Into<PathBuf>, found: u32) -> Self {
        Self::Schema {
            path: path.into(),
            found,
        }
    }

    #[must_use]
    pub fn corrupt(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::Corrupt {
            path: path.into(),
            reason: reason.into(),
        }
    }

    #[must_use]
    pub fn migration(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::Migration {
            path: path.into(),
            reason: reason.into(),
        }
    }

    #[must_use]
    pub fn integrity(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::Integrity {
            path: path.into(),
            reason: reason.into(),
        }
    }

    #[must_use]
    pub fn content(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::ContentRejected {
            path: path.into(),
            reason: reason.into(),
        }
    }

    #[must_use]
    pub fn conflict(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::Conflict {
            path: path.into(),
            reason: reason.into(),
        }
    }

    #[must_use]
    pub fn storage(reason: impl Into<String>) -> Self {
        Self::Storage {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for PersistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateRejected(reason) => write!(f, "character creation rejected: {reason:?}"),
            Self::CharacterIdsExhausted => f.write_str("character id namespace exhausted"),
            Self::CompatibilityNamesExhausted => {
                f.write_str("compatibility name namespace exhausted")
            }
            Self::Migration { path, reason } => {
                write!(f, "{}: migration failed: {reason}", path.display())
            }
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Json { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Schema { path, found } => {
                write!(f, "{}: unsupported schema_version {found}", path.display())
            }
            Self::Corrupt { path, reason } => write!(f, "{}: {reason}", path.display()),
            Self::Integrity { path, reason } => {
                write!(f, "{}: integrity error: {reason}", path.display())
            }
            Self::ContentRejected { path, reason } => {
                write!(f, "{}: content rejected: {reason}", path.display())
            }
            Self::Conflict { path, reason } => {
                write!(f, "{}: conflict: {reason}", path.display())
            }
            Self::Storage { reason } => write!(f, "postgresql: {reason}"),
            Self::ItemIdsExhausted => f.write_str("item instance id namespace exhausted"),
            Self::LeaseLost => f.write_str("postgresql: lease authority lost"),
        }
    }
}

impl std::error::Error for PersistError {}
