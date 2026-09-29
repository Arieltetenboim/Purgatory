//! Character persistence. Simulation does not depend on this crate.
//!
//! JSON, filesystem IO, and PostgreSQL sessions belong on the persistence
//! worker, not the 30 Hz simulation thread. When `PURGATORY_DATABASE_URL` is
//! set, PostgreSQL is the only writer. The file roster is then an import
//! source, not a second durable copy.

mod atomic;
mod character;
mod domain;
mod error;
mod identity;
mod lifecycle;
mod postgres;
mod repository;
mod service;

pub use character::{PERSISTENCE_SCHEMA_VERSION, PersistentCharacter, PersistentCharacterSnapshot};
pub use domain::{
    CharacterItemLocation, CharacterNarrativeState, DURABLE_INVENTORY_CAPACITY, DurableCommand,
    DurableCommandResult, DurableContentRules, DurableEquipmentSlot, ItemContentRule, ItemOwner,
    ItemRecord, LearnedAbilityWrite, LiveDestination, MoveItem, NarrativeWrite, PlaceNewItem,
};
pub use error::{CreateCharacterRejection, PersistError};
pub use identity::{
    CharacterRosterEntry, DevIdentityStore, IDENTITY_FILE_NAME, IDENTITY_SCHEMA_VERSION,
    MAX_ROSTER_SIZE,
};
pub use lifecycle::{
    Admission, CHANNEL_GENERATION_EXPIRY, CHANNEL_GENERATION_RENEWAL, CHARACTER_LEASE_EXPIRY,
    CHARACTER_LEASE_RENEWAL, ChannelClaim, LeaseAuthority, LeaseBarrier, OwnedRestore,
};
pub use postgres::PostgresSettings;
pub use repository::{FileCharacterRepository, character_file_name};
pub use service::{PersistenceService, SessionAdmission};

#[cfg(test)]
mod postgres_tests;

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
