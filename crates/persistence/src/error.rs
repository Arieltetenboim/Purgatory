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
    Migration { path: PathBuf, reason: String },
    Io { path: PathBuf, source: io::Error },
    Json { path: PathBuf, source: String },
    Schema { path: PathBuf, found: u32 },
    Corrupt { path: PathBuf, reason: String },
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
        }
    }
}

impl std::error::Error for PersistError {}
