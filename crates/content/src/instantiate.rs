//! Build simulation spawn plans from a validated registry. No disk IO.

use crate::error::{ContentError, ValidationIssue};
use crate::map_gameplay_authoring::FootholdKind;
use crate::monster::{MonsterBehavior, MonsterDefinition};
use crate::registry::ContentRegistry;
use crate::schema::{EntityDefinition, PlacementKind};
use purgatory_common::{ContentId, WorldAddress};
use purgatory_simulation::{
    CONTACT_EPSILON, EquipmentState, Interactable, InteractableKind, MapRuntimePlan,
    NpcApproachBounds, NpcRuntimeConfig, PLAYER_HALF_EXTENTS, PlanPlatform, Platform,
    RuntimeSpawnRequest, SimulationTick, Transform, World,
};

pub fn map_plan(
    registry: &ContentRegistry,
    map_authored: &str,
    address: WorldAddress,
) -> Result<MapRuntimePlan, ContentError> {
    let map = registry.map(map_authored).ok_or_else(|| {
        ContentError::one(ValidationIssue::new(
            map_authored,
            map_authored,
            "map",
            "unknown map",
        ))
    })?;
    let expected = registry.map_id(map.content_id).ok_or_else(|| {
        ContentError::one(ValidationIssue::new(
            map_authored,
            map_authored,
            "map_id",
            "registry has no MapId for this map",
        ))
    })?;
    if address.map != expected {
        return Err(ContentError::one(ValidationIssue::new(
            map_authored,
            map_authored,
            "address.map",
            "WorldAddress.map does not match registry MapId",
        )));
    }
    let mut platforms = Vec::new();
    for p in &map.platforms {
        platforms.push(PlanPlatform {
            position: p.position,
            platform: match p.kind {
                purgatory_simulation::PlatformKind::Solid => Platform::solid(p.half_extents),
                purgatory_simulation::PlatformKind::OneWay => Platform::one_way(p.half_extents),
                _ => Platform::solid(p.half_extents),
            },
            content_id: Some(map.content_id),
        });
    }
    for path in &map.foothold_paths {
        let kind = match path.kind {
            FootholdKind::OneWay => purgatory_simulation::PlatformKind::OneWay,
            FootholdKind::Solid => purgatory_simulation::PlatformKind::Solid,
        };
        for pair in path.points.windows(2) {
            let start = pair[0];
            let end = pair[1];
            let midpoint = [(start[0] + end[0]) * 0.5, (start[1] + end[1]) * 0.5];
            let local_start = [start[0] - midpoint[0], start[1] - midpoint[1]];
            let local_end = [end[0] - midpoint[0], end[1] - midpoint[1]];
            platforms.push(PlanPlatform {
                position: midpoint,
                platform: Platform::segment(local_start, local_end, kind, path.drop_through),
                content_id: Some(map.content_id),
            });
        }
    }
    let mut placements = Vec::new();
    for (placement_index, place) in registry.placements(map_authored).iter().enumerate() {
        match place.kind {
            PlacementKind::Entity => {
                let entity = registry.entity(&place.content_authored).ok_or_else(|| {
                    ContentError::one(ValidationIssue::new(
                        map_authored,
                        &place.content_authored,
                        "content",
                        "unresolved entity reference",
                    ))
                })?;
                placements.push(spawn_request_for_entity(entity, address, place.position));
            }
            PlacementKind::Monster => {
                let definition = registry.monster(&place.content_authored).ok_or_else(|| {
                    ContentError::one(ValidationIssue::new(
                        map_authored,
                        &place.content_authored,
                        "content",
                        "unresolved monster reference",
                    ))
                })?;
                let ordinal = u32::try_from(placement_index.saturating_add(1)).unwrap_or(u32::MAX);
                let seed = (definition.content_id.token() as u32)
                    .wrapping_add(ordinal.wrapping_mul(0x9E37_79B9));
                placements.push(spawn_request_for_monster(
                    definition,
                    address,
                    place.position,
                    seed,
                    SimulationTick::from_count(0),
                ));
            }
            PlacementKind::Portal => {
                let content_id =
                    ContentId::from_authored(&place.content_authored).map_err(|_| {
                        ContentError::one(ValidationIssue::new(
                            map_authored,
                            &place.id,
                            "id",
                            "portal runtime identity is invalid",
                        ))
                    })?;
                placements.push(
                    RuntimeSpawnRequest::transient_at(address)
                        .with_transform(Transform::from_position(place.position))
                        .with_content(content_id)
                        .visible()
                        .with_interactable(Interactable::new(InteractableKind::Portal)),
                );
            }
        }
    }
    Ok(MapRuntimePlan {
        address,
        map_content: map.content_id,
        bounds: map.bounds,
        platforms,
        placements,
    })
}

/// Build one authored Monster runtime request from a floor/contact point.
pub fn monster_spawn_request(
    registry: &ContentRegistry,
    content_id: ContentId,
    address: WorldAddress,
    floor_position: [f32; 2],
    seed: u32,
    now: SimulationTick,
) -> Result<RuntimeSpawnRequest, ContentError> {
    let definition = registry.monster_by_id(content_id).ok_or_else(|| {
        ContentError::one(ValidationIssue::new(
            format!("content_id={content_id}"),
            "-",
            "monster",
            "ContentId does not resolve to a Monster definition",
        ))
    })?;
    Ok(spawn_request_for_monster(
        definition,
        address,
        floor_position,
        seed,
        now,
    ))
}

fn spawn_request_for_monster(
    definition: &MonsterDefinition,
    address: WorldAddress,
    floor_position: [f32; 2],
    seed: u32,
    now: SimulationTick,
) -> RuntimeSpawnRequest {
    const MONSTER_RUNTIME_TYPE_TOKEN: u32 = 9_000;

    let spawn_position = [
        floor_position[0],
        floor_position[1] + definition.collision_bounds.bottom,
    ];
    let approach_bounds = match definition.behavior {
        MonsterBehavior::ChaseContactWhenAttacked => Some(NpcApproachBounds {
            left: (definition.collision_bounds.left + PLAYER_HALF_EXTENTS[0] - CONTACT_EPSILON)
                .max(0.0),
            right: (definition.collision_bounds.right + PLAYER_HALF_EXTENTS[0] - CONTACT_EPSILON)
                .max(0.0),
            bottom: (definition.collision_bounds.bottom + PLAYER_HALF_EXTENTS[1] - CONTACT_EPSILON)
                .max(0.0),
            top: (definition.collision_bounds.top + PLAYER_HALF_EXTENTS[1] - CONTACT_EPSILON)
                .max(0.0),
        }),
    };
    let runtime_config = NpcRuntimeConfig {
        movement_speed: definition.movement_speed,
        half_extents: definition.collision_bounds.half_extents(),
        collision_center_offset: definition.collision_bounds.center_offset(),
        approach_bounds,
        ..NpcRuntimeConfig::default()
    };
    let mut request = World::npc_spawn_request_with_runtime_config(
        address,
        spawn_position,
        MONSTER_RUNTIME_TYPE_TOKEN,
        definition.home_leash_radius,
        seed,
        now,
        true,
        definition.health_max,
        runtime_config,
    )
    .with_content(definition.content_id);
    if let Some(npc) = request.npc.as_mut() {
        // Preserve the existing authored-Monster DEV behavior: spawn idle,
        // then enter the deterministic patrol schedule.
        npc.walking = false;
    }
    request
}

/// Build one runtime entity from validated content at a caller-supplied
/// authoritative address and position. This performs no disk I/O and does not
/// assign persistence identity.
pub fn entity_spawn_request(
    registry: &ContentRegistry,
    content_id: ContentId,
    address: WorldAddress,
    position: [f32; 2],
) -> Result<RuntimeSpawnRequest, ContentError> {
    let entity = registry.entity_by_id(content_id).ok_or_else(|| {
        ContentError::one(ValidationIssue::new(
            format!("content_id={content_id}"),
            "-",
            "entity",
            "ContentId does not resolve to an entity definition",
        ))
    })?;
    Ok(spawn_request_for_entity(entity, address, position))
}

fn spawn_request_for_entity(
    entity: &EntityDefinition,
    address: WorldAddress,
    position: [f32; 2],
) -> RuntimeSpawnRequest {
    let mut request = RuntimeSpawnRequest::transient_at(address)
        .with_transform(Transform::from_position(position))
        .with_content(entity.content_id);
    if entity.visible {
        request = request.visible();
    }
    if let Some(kind) = entity.interactable {
        request = request.with_interactable(Interactable::new(kind));
        if kind == InteractableKind::Npc {
            // An equipment domain, even when empty, is the existing wire-visible
            // humanoid presentation facet. Combat NPCs without this facet keep
            // their sprite presentation.
            request = request.with_equipment(EquipmentState::empty());
        }
    }
    request
}

/// Shared-geometry plan (no server placements). Client prediction uses this.
pub fn geometry_plan(
    registry: &ContentRegistry,
    map: purgatory_common::MapId,
    address: WorldAddress,
) -> Result<MapRuntimePlan, ContentError> {
    let def = registry.map_by_map_id(map).ok_or_else(|| {
        ContentError::one(ValidationIssue::new(
            format!("map_id={map}"),
            "-",
            "map",
            "MapId is not in this registry",
        ))
    })?;
    let mut plan = map_plan(registry, &def.authored_id, address)?;
    plan.placements.clear();
    Ok(plan)
}

#[must_use]
pub fn spawn_point_position(
    registry: &ContentRegistry,
    map_authored: &str,
    spawn: &str,
) -> Option<[f32; 2]> {
    registry
        .map(map_authored)?
        .spawn_points
        .iter()
        .find(|s| s.id == spawn)
        .map(|s| s.position)
}

#[must_use]
pub fn world_address_for_map(
    registry: &ContentRegistry,
    map_content: ContentId,
    channel: purgatory_common::ChannelId,
    instance: purgatory_common::InstanceId,
) -> Option<WorldAddress> {
    let map = registry.map_id(map_content)?;
    Some(WorldAddress::new(map, channel, instance))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monster::{MonsterCollisionBounds, MonsterDefinition};
    use purgatory_common::ContentId;

    #[test]
    fn authored_monster_projection_uses_floor_position_and_definition_stats() {
        let definition = MonsterDefinition {
            content_id: ContentId::from_authored("monster.synthetic").unwrap(),
            authored_id: "monster.synthetic".into(),
            debug_name: "Synthetic".into(),
            health_max: 37.0,
            collision_bounds: MonsterCollisionBounds {
                left: 0.3,
                right: 0.5,
                bottom: 0.4,
                top: 0.8,
            },
            movement_speed: 1.75,
            behavior: MonsterBehavior::ChaseContactWhenAttacked,
            home_leash_radius: 6.5,
        };

        let request = spawn_request_for_monster(
            &definition,
            WorldAddress::DEV,
            [3.0, 2.0],
            123,
            SimulationTick::from_count(7),
        );

        assert_eq!(
            request.transform.map(|transform| transform.position),
            Some([3.0, 2.4])
        );
        assert_eq!(request.content_id, Some(definition.content_id));
        assert_eq!(request.health.map(|health| health.max), Some(37.0));
        let npc = request.npc.expect("monster NPC capability");
        assert_eq!(npc.home, [3.0, 2.4]);
        assert_eq!(npc.hotspot_radius, 6.5);
        assert_eq!(npc.runtime_config.movement_speed, 1.75);
        assert_eq!(npc.runtime_config.half_extents, [0.4, 0.6]);
        assert_eq!(npc.runtime_config.collision_center_offset, [0.1, 0.2]);
        assert!(npc.runtime_config.approach_bounds.is_some());
        assert!(!npc.walking);
        assert!(npc.active);
    }
}
