//! Build simulation spawn plans from a validated registry. No disk IO.

use crate::error::{ContentError, ValidationIssue};
use crate::registry::ContentRegistry;
use purgatory_common::{ContentId, WorldAddress};
use purgatory_simulation::{
    EquipmentState, Interactable, InteractableKind, MapRuntimePlan, PlanPlatform, Platform,
    RuntimeSpawnRequest, Transform,
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
    let mut placements = Vec::new();
    for place in registry.placements(map_authored) {
        let ent = registry.entity(&place.entity_authored).ok_or_else(|| {
            ContentError::one(ValidationIssue::new(
                map_authored,
                &place.entity_authored,
                "entity",
                "unresolved entity reference",
            ))
        })?;
        let mut req = RuntimeSpawnRequest::transient_at(address)
            .with_transform(Transform::from_position(place.position))
            .with_content(ent.content_id);
        if ent.visible {
            req = req.visible();
        }
        if let Some(kind) = ent.interactable {
            req = req.with_interactable(Interactable::new(kind));
            if kind == InteractableKind::Npc {
                // An equipment domain, even when empty, is the existing wire-visible
                // humanoid presentation facet. Combat NPCs without this facet keep
                // their sprite presentation.
                req = req.with_equipment(EquipmentState::empty());
            }
        }
        placements.push(req);
    }
    Ok(MapRuntimePlan {
        address,
        map_content: map.content_id,
        bounds: map.bounds,
        platforms,
        placements,
    })
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
