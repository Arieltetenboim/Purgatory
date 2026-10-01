//! Character persistence. Simulation does not depend on this crate.
//!
//! PostgreSQL sessions belong on the persistence worker, not the 30 Hz
//! simulation thread. Server startup only reopens an initialized database.
//! Creating, resetting, and provisioning development users are separate
//! administration operations. Game character state is not written to files.

mod admin;
mod character;
mod domain;
mod error;
mod identity;
mod lifecycle;
mod local_config;
mod memory;
mod postgres;
mod service;

pub use admin::{
    AdminAudience, AdminFailure, AdminOutcome, DatabaseAdminRequest, DatabaseInspection,
    DatabaseStatus, LOCAL_DEV_DATABASE, LOCAL_DEV_DEPLOYMENT_ID, add_user as add_development_user,
    create as create_database, inspect as inspect_database,
    inspect_runtime as inspect_runtime_database, reset as reset_database,
};
pub use character::{
    PERSISTENCE_SCHEMA_VERSION, PersistentCharacter, PersistentCharacterSnapshot,
    current_from_milli, health_milli,
};
pub use domain::{
    CharacterItemLocation, CharacterNarrativeState, DURABLE_INVENTORY_CAPACITY, DurableCommand,
    DurableCommandResult, DurableContentRules, DurableEquipmentSlot, ItemContentRule, ItemOwner,
    ItemRecord, LearnedAbilityWrite, LiveDestination, MoveItem, NarrativeWrite, PlaceNewItem,
    ReservedItemOutcome, ReservedItemUse,
};
pub use error::{CreateCharacterRejection, PersistError};
pub use identity::{CharacterRosterEntry, MAX_ROSTER_SIZE};
pub use lifecycle::{
    Admission, CHANNEL_GENERATION_EXPIRY, CHANNEL_GENERATION_RENEWAL, CHARACTER_LEASE_EXPIRY,
    CHARACTER_LEASE_RENEWAL, ChannelClaim, LeaseAuthority, LeaseBarrier, OwnedRestore,
};
pub use local_config::{
    LocalDatabaseConfig, LocalDatabaseConfigError, RELATIVE_PATH as LOCAL_DATABASE_FILE,
    redact_connection_text,
};
pub use postgres::{PostgresSettings, drop_test_schema};
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
