//! Gameplay spatial query boundary.
//!
//! Implementations live on [`crate::World`] (`query_aabb`, `query_radius`,
//! `entities_near`). The uniform grid is replaceable behind those methods.
//! Queries take a [`WorldAddress`] and never cross Map/Channel/Instance unless
//! the caller passes a different address.

use crate::entity::EntityKind;

/// Optional predicate applied after the spatial candidate set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryFilter {
    Any,
    Kind(EntityKind),
    HasHealth,
    HasInteractable,
}

/// Optional cap on returned entities (slot-index order from the grid).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryLimit {
    pub max: usize,
}

impl QueryLimit {
    #[must_use]
    pub const fn max(max: usize) -> Self {
        Self { max }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Aabb, Health, RuntimeSpawnRequest, Transform, World, WorldAddress};

    fn spawn_generic(world: &mut World, x: f32) -> crate::EntityId {
        world
            .spawn(
                RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                    .with_transform(Transform::from_position([x, 1.0]))
                    .visible(),
            )
            .expect("spawn")
    }

    #[test]
    fn spatial_query_isolates_world_address_and_despawn() {
        let mut world = World::new();
        let a = spawn_generic(&mut world, 0.0);
        let b = spawn_generic(&mut world, 1.0);
        let other = world
            .spawn(
                RuntimeSpawnRequest::transient_at(WorldAddress::new(
                    purgatory_common::MapId::DEV,
                    purgatory_common::ChannelId::from_raw(1),
                    purgatory_common::InstanceId::DEFAULT,
                ))
                .with_transform(Transform::from_position([0.0, 1.0]))
                .visible(),
            )
            .unwrap();
        let hits = world.query_radius(WorldAddress::DEV, [0.0, 1.0], 2.0);
        assert!(hits.contains(&a) && hits.contains(&b));
        assert!(!hits.contains(&other));
        world.despawn(a);
        let after = world.query_aabb(WorldAddress::DEV, Aabb::new([0.5, 1.0], [2.0, 2.0]));
        assert!(!after.contains(&a));
        let capped = world.query_radius_filtered(
            WorldAddress::DEV,
            [0.0, 1.0],
            8.0,
            QueryFilter::Any,
            Some(QueryLimit::max(1)),
        );
        assert_eq!(capped.len(), 1);
    }

    #[test]
    fn health_query_filter_and_same_inputs_stable() {
        let mut world = World::new();
        let a = world
            .spawn(
                RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                    .with_transform(Transform::from_position([0.0, 0.0]))
                    .with_health(Health::full(1.0))
                    .visible(),
            )
            .unwrap();
        let _b = spawn_generic(&mut world, 0.2);
        let first = world.query_radius_filtered(
            WorldAddress::DEV,
            [0.0, 0.0],
            2.0,
            QueryFilter::HasHealth,
            None,
        );
        let second = world.query_radius_filtered(
            WorldAddress::DEV,
            [0.0, 0.0],
            2.0,
            QueryFilter::HasHealth,
            None,
        );
        assert_eq!(first, second);
        assert_eq!(first, vec![a]);
    }
}
