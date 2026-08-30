//! Restore resolution vs runtime placement.
//!
//! ```text
//! RestoreIntent
//!     → Restore Resolver
//!     → LogicalRestoreDestination { map, point/checkpoint }
//!     → Runtime Placement Resolver
//!     → WorldAddress { MapId, ChannelId, InstanceId }
//! ```
//!
//! Restore semantics never include ChannelId or runtime InstanceId.
//! Phase 6E placement currently selects `ChannelId::DEFAULT` and
//! `InstanceId::DEFAULT` as a temporary implementation of the placement layer.

use crate::registry::ContentRegistry;
use crate::schema::RestorePolicy;
use purgatory_common::{
    ChannelId, DEFAULT_RESTORE_POINT, InstanceId, MAP_FOOTNOTE_AUTHORED, RestoreIntent,
    WorldAddress,
};

use crate::instantiate::{spawn_point_position, world_address_for_map};

/// Logical restore target. Not a [`WorldAddress`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogicalRestoreDestination {
    pub map_authored: String,
    pub point_id: String,
    pub checkpoint_id: Option<String>,
}

fn fallback_destination() -> LogicalRestoreDestination {
    LogicalRestoreDestination {
        map_authored: MAP_FOOTNOTE_AUTHORED.to_string(),
        point_id: DEFAULT_RESTORE_POINT.to_string(),
        checkpoint_id: None,
    }
}

fn map_has_point(registry: &ContentRegistry, map_authored: &str, point_id: &str) -> bool {
    registry
        .map(map_authored)
        .is_some_and(|m| m.spawn_points.iter().any(|s| s.id == point_id))
}

/// Apply authored restore policy. Invalid content references fall back to
/// `map.dev.footnote` / `default` and log; they never panic.
#[must_use]
pub fn resolve_restore(
    registry: &ContentRegistry,
    intent: &RestoreIntent,
) -> LogicalRestoreDestination {
    let Some(map) = registry.map(&intent.map_authored) else {
        eprintln!(
            "PURGATORY restore unknown map '{}' — falling back to {MAP_FOOTNOTE_AUTHORED}/{DEFAULT_RESTORE_POINT}",
            intent.map_authored
        );
        return fallback_destination();
    };
    match &map.restore {
        RestorePolicy::SafePoint { point_id } => {
            let chosen = if map_has_point(registry, &map.authored_id, &intent.point_id) {
                intent.point_id.clone()
            } else if map_has_point(registry, &map.authored_id, point_id) {
                point_id.clone()
            } else {
                eprintln!(
                    "PURGATORY restore missing point on '{}' — falling back",
                    map.authored_id
                );
                return fallback_destination();
            };
            LogicalRestoreDestination {
                map_authored: map.authored_id.clone(),
                point_id: chosen,
                checkpoint_id: None,
            }
        }
        RestorePolicy::Checkpoint { point_id } => {
            if let Some(cp) = &intent.checkpoint_id
                && map_has_point(registry, &map.authored_id, cp)
            {
                return LogicalRestoreDestination {
                    map_authored: map.authored_id.clone(),
                    point_id: cp.clone(),
                    checkpoint_id: Some(cp.clone()),
                };
            }
            let chosen = if map_has_point(registry, &map.authored_id, point_id) {
                point_id.clone()
            } else {
                eprintln!(
                    "PURGATORY restore missing checkpoint fallback on '{}' — falling back",
                    map.authored_id
                );
                return fallback_destination();
            };
            LogicalRestoreDestination {
                map_authored: map.authored_id.clone(),
                point_id: chosen,
                checkpoint_id: None,
            }
        }
        RestorePolicy::NonReenterable {
            fallback_map,
            fallback_point,
        } => {
            if !map_has_point(registry, fallback_map, fallback_point) {
                eprintln!(
                    "PURGATORY restore non-reenterable fallback '{fallback_map}/{fallback_point}' missing"
                );
                return fallback_destination();
            }
            LogicalRestoreDestination {
                map_authored: fallback_map.clone(),
                point_id: fallback_point.clone(),
                checkpoint_id: None,
            }
        }
    }
}

/// Temporary Phase 6E placement: DEFAULT channel and instance.
#[must_use]
pub fn runtime_placement(
    registry: &ContentRegistry,
    dest: &LogicalRestoreDestination,
) -> Option<(WorldAddress, [f32; 2])> {
    let map = registry.map(&dest.map_authored)?;
    let content = map.content_id;
    let address =
        world_address_for_map(registry, content, ChannelId::DEFAULT, InstanceId::DEFAULT)?;
    let position = spawn_point_position(registry, &dest.map_authored, &dest.point_id)?;
    Some((address, position))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{MapDefinition, MapPlatform, RestorePolicy, SpawnPoint};
    use crate::{ContentDomain, ContentRegistry};
    use purgatory_common::{ContentId, MAP_SECOND_AUTHORED};
    use purgatory_simulation::{PlatformKind, WorldBounds};

    fn sample_map(id: &str, restore: RestorePolicy) -> MapDefinition {
        MapDefinition {
            content_id: ContentId::from_authored(id).unwrap(),
            authored_id: id.into(),
            debug_name: id.into(),
            domain: ContentDomain::Shared,
            bounds: WorldBounds::FOOTNOTE_TEST,
            spawn_points: vec![SpawnPoint {
                id: "default".into(),
                position: [1.0, 2.0],
            }],
            platforms: vec![MapPlatform {
                position: [0.0, 0.0],
                half_extents: [1.0, 0.2],
                kind: PlatformKind::Solid,
            }],
            restore,
        }
    }

    fn registry_two_maps(second_policy: RestorePolicy) -> ContentRegistry {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(
            MAP_FOOTNOTE_AUTHORED,
            RestorePolicy::SafePoint {
                point_id: "default".into(),
            },
        ))
        .unwrap();
        reg.insert_map(sample_map(MAP_SECOND_AUTHORED, second_policy))
            .unwrap();
        reg.finish().unwrap();
        reg
    }

    #[test]
    fn safe_point_restores_map_not_channel() {
        let registry = registry_two_maps(RestorePolicy::SafePoint {
            point_id: "default".into(),
        });
        let intent = RestoreIntent {
            map_authored: MAP_SECOND_AUTHORED.into(),
            point_id: "default".into(),
            checkpoint_id: None,
        };
        let logical = resolve_restore(&registry, &intent);
        assert_eq!(logical.map_authored, MAP_SECOND_AUTHORED);
        let (addr, pos) = runtime_placement(&registry, &logical).unwrap();
        assert_eq!(addr.channel, ChannelId::DEFAULT);
        assert_eq!(addr.instance, InstanceId::DEFAULT);
        assert_eq!(pos, [1.0, 2.0]);
    }

    #[test]
    fn non_reenterable_uses_fallback_map() {
        let registry = registry_two_maps(RestorePolicy::NonReenterable {
            fallback_map: MAP_FOOTNOTE_AUTHORED.into(),
            fallback_point: "default".into(),
        });
        let intent = RestoreIntent {
            map_authored: MAP_SECOND_AUTHORED.into(),
            point_id: "default".into(),
            checkpoint_id: None,
        };
        let logical = resolve_restore(&registry, &intent);
        assert_eq!(logical.map_authored, MAP_FOOTNOTE_AUTHORED);
    }

    #[test]
    fn unknown_map_falls_back() {
        let registry = registry_two_maps(RestorePolicy::SafePoint {
            point_id: "default".into(),
        });
        let intent = RestoreIntent {
            map_authored: "map.dev.missing".into(),
            point_id: "default".into(),
            checkpoint_id: None,
        };
        let logical = resolve_restore(&registry, &intent);
        assert_eq!(logical.map_authored, MAP_FOOTNOTE_AUTHORED);
    }
}
