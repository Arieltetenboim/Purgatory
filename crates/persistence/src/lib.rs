//! File-backed character persistence. Simulation does not depend on this crate.
//!
//! JSON serialization and filesystem IO belong on the persistence worker, not
//! the 30 Hz simulation thread.

mod atomic;
mod character;
mod error;
mod identity;
mod repository;
mod service;

pub use character::{PERSISTENCE_SCHEMA_VERSION, PersistentCharacter, PersistentCharacterSnapshot};
pub use error::PersistError;
pub use identity::{DevIdentityStore, IDENTITY_FILE_NAME, IDENTITY_SCHEMA_VERSION};
pub use repository::{FileCharacterRepository, character_file_name};
pub use service::PersistenceService;

/// Cargo package version for this crate.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_nonempty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn crate_manifest_excludes_sim_and_transport() {
        let manifest = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
        for forbidden in [
            "tokio",
            "quinn",
            "winit",
            "wgpu",
            "egui",
            "purgatory-simulation",
        ] {
            let present = manifest.lines().any(|line| {
                let trimmed = line.trim();
                trimmed.starts_with(forbidden) && (trimmed.contains('=') || trimmed.contains('{'))
            });
            assert!(
                !present,
                "{forbidden} must not appear as a purgatory-persistence dependency"
            );
        }
    }
}
