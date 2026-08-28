//! Shared primitives for PURGATORY client, server, and tools.

/// Cargo package version for this crate.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Current master-plan phase. Source of truth: repo-root `PHASE`.
pub fn phase() -> &'static str {
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../PHASE")).trim()
}

/// Compact label for logs, window titles, and the dev launcher.
#[must_use]
pub fn identity() -> String {
    format!("v{}  phase {}", version(), phase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_nonempty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn phase_is_nonempty() {
        assert!(!phase().is_empty());
        assert!(
            phase()
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.'),
            "PHASE must be a simple token like 5.4"
        );
    }
}
