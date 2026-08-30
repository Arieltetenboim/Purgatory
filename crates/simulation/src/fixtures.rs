//! Reusable Phase 6 runtime test entities.

use crate::EntityId;
use crate::body::PlayerState;
use crate::health::Health;
use crate::platform::Platform;
use crate::spawn::RuntimeSpawnRequest;
use crate::transform::Transform;
use crate::world::World;
use purgatory_common::ChannelId;
use purgatory_common::{ContentId, InstanceId, MapId, WorldAddress};

/// Builders for tests. Not game content.
pub struct RuntimeFixtures;

impl RuntimeFixtures {
    pub fn test_player(world: &mut World) -> EntityId {
        let floor = world
            .iter_platforms()
            .next()
            .map(|p| (p.id, p.top_surface()));
        let (floor_id, top) = match floor {
            Some(pair) => pair,
            None => {
                let id = world.spawn_platform(
                    Transform::from_position([0.0, 0.0]),
                    Platform::solid([2.0, 0.1]),
                );
                (id, 0.1)
            }
        };
        let (transform, state) = PlayerState::standing_on_at(floor_id, top, 0.0);
        world.spawn_player(transform, state)
    }

    /// Composed non-player: transform + health + visible replication.
    pub fn test_mob_like(world: &mut World) -> EntityId {
        world
            .spawn(
                RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                    .with_transform(Transform::from_position([4.0, 1.0]))
                    .with_content(ContentId::from_token(1001))
                    .visible()
                    .with_health(Health::full(20.0)),
            )
            .expect("mob-like spawn")
    }

    pub fn test_interactable(world: &mut World) -> EntityId {
        world
            .spawn(
                RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                    .with_transform(Transform::from_position([2.0, 1.0]))
                    .with_content(ContentId::from_token(2002))
                    .visible()
                    .with_interactable(crate::Interactable::new(crate::InteractableKind::Switch)),
            )
            .expect("interactable spawn")
    }

    pub fn transient_replicated(world: &mut World) -> EntityId {
        world
            .spawn(
                RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                    .with_transform(Transform::from_position([3.0, 1.0]))
                    .visible(),
            )
            .expect("transient spawn")
    }

    pub fn in_other_instance(world: &mut World) -> EntityId {
        let id = Self::transient_replicated(world);
        world.set_address(
            id,
            WorldAddress::new(MapId::DEV, ChannelId::DEFAULT, InstanceId::from_raw(2)),
        );
        id
    }

    pub fn without_transform(world: &mut World) -> EntityId {
        world
            .spawn(RuntimeSpawnRequest::transient_at(WorldAddress::DEV))
            .expect("logical entity")
    }
}
