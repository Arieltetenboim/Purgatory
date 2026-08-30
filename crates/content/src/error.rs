//! Structured content validation errors. Never panic on ordinary bad content.

use std::fmt;
use std::path::PathBuf;

/// One validation or load failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationIssue {
    pub source: String,
    pub definition: String,
    pub field: String,
    pub reason: String,
}

impl ValidationIssue {
    #[must_use]
    pub fn new(
        source: impl Into<String>,
        definition: impl Into<String>,
        field: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            definition: definition.into(),
            field: field.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}] {}: {}",
            self.source, self.definition, self.field, self.reason
        )
    }
}

/// Load/validation failure set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentError {
    pub issues: Vec<ValidationIssue>,
}

impl ContentError {
    #[must_use]
    pub fn one(issue: ValidationIssue) -> Self {
        Self {
            issues: vec![issue],
        }
    }

    #[must_use]
    pub fn from_io(path: &std::path::Path, err: &std::io::Error) -> Self {
        Self::one(ValidationIssue::new(
            path.display().to_string(),
            "-",
            "io",
            err.to_string(),
        ))
    }

    #[must_use]
    pub fn from_path(
        path: PathBuf,
        definition: &str,
        field: &str,
        reason: impl Into<String>,
    ) -> Self {
        Self::one(ValidationIssue::new(
            path.display().to_string(),
            definition,
            field,
            reason,
        ))
    }
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, issue) in self.issues.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{issue}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ContentError {}
