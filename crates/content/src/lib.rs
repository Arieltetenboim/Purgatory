//! Content loading, validation, and registry. JSON stays in this crate.

mod domain;
mod error;
mod instantiate;
mod loader;
mod registry;
mod restore;
mod schema;

pub use domain::ContentDomain;
pub use error::{ContentError, ValidationIssue};
pub use instantiate::{geometry_plan, map_plan, spawn_point_position, world_address_for_map};
pub use loader::{LoadMode, default_content_root, load_registry};
pub use registry::ContentRegistry;
pub use restore::{LogicalRestoreDestination, resolve_restore, runtime_placement};
pub use schema::{
    CONTENT_SCHEMA_VERSION, EntityDefinition, MapDefinition, MapPlatform, Placement, RestorePolicy,
    SpawnPoint, TransitionRef,
};

/// Cargo package version for this crate.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_common::ContentId;

    #[test]
    fn version_is_nonempty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn common_is_linked() {
        assert!(!purgatory_common::version().is_empty());
    }

    #[test]
    fn full_pack_instantiates_map_a_and_b() {
        use purgatory_common::{ChannelId, InstanceId, MAP_FOOTNOTE_AUTHORED, MAP_SECOND_AUTHORED};
        use purgatory_simulation::World;
        let registry = load_registry(&default_content_root(), LoadMode::Full).expect("pack");
        let mut world = World::new();
        for authored in [MAP_FOOTNOTE_AUTHORED, MAP_SECOND_AUTHORED] {
            let cid = ContentId::from_authored(authored).unwrap();
            let addr =
                world_address_for_map(&registry, cid, ChannelId::DEFAULT, InstanceId::DEFAULT)
                    .unwrap();
            let plan = map_plan(&registry, authored, addr).unwrap();
            world.instantiate_map(&plan).unwrap();
        }
        assert_eq!(world.instantiated_count(), 2);
        let switch = ContentId::from_authored("entity.interactable.switch").unwrap();
        let found = world
            .iter()
            .any(|id| world.content_id_of(id) == Some(switch));
        assert!(found);
        assert!(world.iter().all(|id| world.persistent_id_of(id).is_none()));
    }
}
