//! Content loading, validation, and registry. JSON stays in this crate.

mod ability;
mod dialogue;
mod domain;
mod equipment;
mod error;
mod instantiate;
mod item;
mod loader;
#[cfg(feature = "map-authoring")]
mod map_compiler;
mod map_environment_authoring;
mod map_gameplay_authoring;
mod map_presentation;
mod monster;
mod registry;
mod restore;
mod schema;

pub use ability::ABILITY_CONTENT_SCHEMA_VERSION;
pub use dialogue::{
    DialogueAction, DialogueBeat, DialogueBeatIndex, DialogueChoice, DialogueCondition,
    DialogueConditionState, DialogueLine, DialoguePool, DialoguePresentationBeat,
    DialoguePresentationChoice, DialoguePresentationLine, DialogueSelectionRole,
    NPC_DIALOGUE_SCHEMA_VERSION, NpcDialogueDefinition, NpcDialoguePresentation,
};
pub use domain::ContentDomain;
pub use equipment::{
    AnchorPoint, BoneTarget, CORRECTION_OFFSET_MAX_PX, CORRECTION_ROTATION_MAX_DEG,
    CorrectionOffset, CoverageMode, EQUIPMENT_CONTENT_SCHEMA_VERSION, EquipmentAuthError,
    EquipmentDefinition, EquipmentPresentation, PresentationAttachment, PresentationCompleteness,
    ViewVariant, ViewVisuals, authorize_equip, bone_allows_anchor, slot_allows_anchor,
    slot_allows_bone, validate_equipment_definition, validate_equipment_presentation,
};
pub use error::{ContentError, ValidationIssue};
pub use instantiate::{
    entity_spawn_request, geometry_plan, map_plan, monster_spawn_request, spawn_point_position,
    world_address_for_map,
};
pub use item::{
    ITEM_CONTENT_SCHEMA_VERSION, ITEM_PRESENTATION_SCHEMA_VERSION, ItemCategory, ItemDefinition,
    ItemPresentation, is_stackable, validate_item_definition, validate_item_presentation,
};
pub use loader::{
    LoadMode, default_content_root, load_placement_file, load_registry, serialize_placements_v2,
};
#[cfg(feature = "map-authoring")]
pub use map_compiler::{
    CANONICAL_MAP_TILE_PX, MAP_AUTHORING_SCHEMA_VERSION, MIN_MAP_HEIGHT_WU, MIN_MAP_WIDTH_WU,
    MapAuthoringSource, compile_tiled_map, compile_tiled_map_with_ppu, load_map_authoring,
    serialize_map_pretty, validate_canonical_map_grid,
};
pub use map_environment_authoring::{
    CLOUDS_PER_VIEWPORT_AT_FULL_DENSITY, CloudFieldAuthoring, CloudFieldPresentation,
    CloudInstanceSpec, CloudStackPosition, MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION,
    MAP_ENVIRONMENT_PRESENTATION_SCHEMA_VERSION, MAX_CLOUDS_PER_FIELD, MapEnvironmentAuthoring,
    MapEnvironmentPresentation, ParallaxDepth, ParallaxFillMode, ParallaxLayer, SkyGradient,
    cloud_field_seed, cloud_instance_count, cloud_instance_specs, validate_cloud_field,
};
#[cfg(feature = "map-authoring")]
pub use map_environment_authoring::{
    compile_map_environment, resolve_png_asset_folder, serialize_map_environment_pretty,
};
pub use map_gameplay_authoring::{
    FootholdKind, FootholdPath, GameplaySpawnPoint, MAP_GAMEPLAY_AUTHORING_SCHEMA_VERSION,
    MapGameplayAuthoring,
};
pub use map_presentation::{
    MAP_PRESENTATION_SCHEMA_VERSION, MapPresentation, PresentationAsset, PresentationLayer,
    PresentationLayerKind, PresentationSprite, TileTransform,
};
pub use monster::{
    MONSTER_CONTENT_SCHEMA_VERSION, MonsterBehavior, MonsterDefinition, validate_monster_definition,
};
pub use registry::ContentRegistry;
pub use restore::{LogicalRestoreDestination, resolve_restore, runtime_placement};
pub use schema::{
    CONTENT_SCHEMA_VERSION, EntityDefinition, MapDefinition, MapPlatform, PLACEMENT_SCHEMA_VERSION,
    Placement, PlacementKind, PortalLink, RestorePolicy, SpawnPoint, TransitionRef,
    portal_runtime_authored,
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
    fn full_pack_instantiates_map1_and_map2() {
        use purgatory_common::{
            ChannelId, InstanceId, MAP1, MAP1_AUTHORED, MAP2, MAP2_AUTHORED,
            content_catalog::NPC_WELCOME_GATE_WATCHMAN,
        };
        use purgatory_simulation::{InteractableKind, World};
        let registry = load_registry(&default_content_root(), LoadMode::Full).expect("pack");
        let mut world = World::new();
        for (authored, cid) in [(MAP1_AUTHORED, MAP1), (MAP2_AUTHORED, MAP2)] {
            let addr =
                world_address_for_map(&registry, cid, ChannelId::DEFAULT, InstanceId::DEFAULT)
                    .unwrap();
            let plan = map_plan(&registry, authored, addr).unwrap();
            world.instantiate_map(&plan).unwrap();
        }
        assert_eq!(world.instantiated_count(), 2);
        let gate_watchman = world
            .iter()
            .find(|&id| world.content_id_of(id) == Some(NPC_WELCOME_GATE_WATCHMAN))
            .expect("live MAP1 Gate Watchman");
        assert_eq!(
            world.interactable_of(gate_watchman).map(|cap| cap.kind),
            Some(InteractableKind::Npc)
        );
        assert!(world.equipment_of(gate_watchman).is_some());
        let map2_portal = registry
            .portal_content_id(MAP2_AUTHORED, "portal.001")
            .expect("MAP2 portal content");
        assert!(
            world
                .iter()
                .any(|id| world.content_id_of(id) == Some(map2_portal))
        );
        assert!(world.iter().all(|id| world.persistent_id_of(id).is_none()));
    }

    #[test]
    fn allocated_social_npc_builds_a_transient_humanoid_spawn_request() {
        use purgatory_common::NPC_WELCOME_TRAVELER_STAYED;
        use purgatory_simulation::{InteractableKind, WorldAddress};

        let registry = load_registry(&default_content_root(), LoadMode::Full).expect("pack");
        let position = [4.5, 2.25];
        let request = entity_spawn_request(
            &registry,
            NPC_WELCOME_TRAVELER_STAYED,
            WorldAddress::DEV,
            position,
        )
        .expect("spawnable Traveler");

        assert_eq!(request.address, WorldAddress::DEV);
        assert_eq!(
            request.transform.map(|value| value.position),
            Some(position)
        );
        assert_eq!(request.content_id, Some(NPC_WELCOME_TRAVELER_STAYED));
        assert!(request.persistent_id.is_none());
        assert_eq!(
            request.interactable.map(|value| value.kind),
            Some(InteractableKind::Npc)
        );
        assert!(request.equipment.is_some_and(|state| state.is_empty()));
    }

    #[test]
    fn pack_loads_basic_strike_ability() {
        use purgatory_simulation::{
            AbilityActivation, AbilityDelivery, AbilityEffect, AbilityRequest, ActionGateContext,
            ActionPhase, Health, RuntimeSpawnRequest, SimulationTick, Transform, World,
            WorldAddress,
        };
        let registry = load_registry(&default_content_root(), LoadMode::Shared).expect("pack");
        let def = registry
            .ability("skill.basic.strike")
            .expect("authored basic strike")
            .clone();
        assert_eq!(
            def.id,
            ContentId::from_authored("skill.basic.strike").unwrap()
        );
        assert_eq!(def.activation, AbilityActivation::Independent);
        assert_eq!(
            def.delivery,
            AbilityDelivery::ForwardQuery {
                range: 1.5,
                half_height: 0.8,
                max_targets: 8
            }
        );
        assert_eq!(def.effects, vec![AbilityEffect::Damage { amount: 5.0 }]);

        let mut world = World::new();
        world.begin_tick(SimulationTick::from_count(1));
        let actor = world
            .spawn(
                RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                    .with_transform(Transform::from_position([0.0, 1.0]))
                    .with_health(Health::full(10.0))
                    .visible(),
            )
            .unwrap();
        let foe = world
            .spawn(
                RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                    .with_transform(Transform::from_position([1.0, 1.0]))
                    .with_health(Health::full(10.0))
                    .visible(),
            )
            .unwrap();
        world
            .request_ability(
                AbilityRequest {
                    actor,
                    selected: None,
                    definition: &def,
                },
                ActionGateContext::in_world(),
            )
            .unwrap();
        world.begin_tick(SimulationTick::from_count(4));
        world.drain_critical_scheduler();
        assert_eq!(
            world.active_action(actor).unwrap().phase,
            ActionPhase::Active
        );
        assert!((world.health_of(foe).unwrap().current - 5.0).abs() < 1e-5);
    }
}
