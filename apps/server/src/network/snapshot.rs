//! SnapshotBuilder: World → WorldSnapshot. Network code does not inspect
//! simulation internals here.

use purgatory_protocol::{
    PlatformSupportId, ReplicatedKind, SnapshotEntity, WireEntityId, WorldSnapshot,
};
use purgatory_simulation::{EntityId, World};

/// Development cadence: one snapshot per simulation tick (30 Hz). Server-owned,
/// independent of client FPS. Not production bandwidth tuning.
#[allow(dead_code)]
pub const SNAPSHOT_PERIOD_TICKS: u64 = 1;

#[must_use]
pub fn to_wire_id(id: EntityId) -> WireEntityId {
    WireEntityId {
        index: id.index(),
        generation: id.generation(),
    }
}

#[must_use]
pub fn from_wire_id(id: WireEntityId) -> EntityId {
    EntityId::from_raw(id.index, id.generation)
}

/// Build a full snapshot for one recipient from a visibility set.
///
/// Header acknowledgement and local contact are recipient-specific.
#[must_use]
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub fn build(
    snapshot_sequence: u32,
    server_tick: u64,
    local_player: EntityId,
    world: &World,
    visible: &[EntityId],
    input_epoch: u16,
    last_acknowledged_input_sequence: u32,
    continuation_debt: u16,
) -> WorldSnapshot {
    let body = world.player_body_of(local_player);
    let local_grounded = body.is_some_and(|b| b.grounded);
    let local_grounded_on = body
        .and_then(|b| b.grounded_on)
        .and_then(|id| world.support_id_of(id))
        .map(PlatformSupportId)
        .unwrap_or(PlatformSupportId::NONE);
    let local_ignored_platform = body
        .and_then(|b| b.ignored_platform)
        .and_then(|id| world.support_id_of(id))
        .map(PlatformSupportId)
        .unwrap_or(PlatformSupportId::NONE);
    WorldSnapshot {
        snapshot_sequence,
        server_tick,
        local_player_entity: to_wire_id(local_player),
        input_epoch,
        last_acknowledged_input_sequence,
        local_grounded,
        local_grounded_on,
        local_ignored_platform,
        continuation_debt,
        local_map: world
            .address_of(local_player)
            .map(|a| a.map.raw())
            .unwrap_or(1),
        local_channel: world
            .address_of(local_player)
            .map(|a| a.channel.raw())
            .unwrap_or(0),
        local_instance: world
            .address_of(local_player)
            .map(|a| a.instance.raw())
            .unwrap_or(0),
        entities: collect_entities(world, visible),
    }
}

#[must_use]
#[allow(dead_code)]
pub fn collect_entities(world: &World, visible: &[EntityId]) -> Vec<SnapshotEntity> {
    visible
        .iter()
        .filter_map(|&id| {
            if let Some(body) = world.player_body_of(id) {
                return Some(SnapshotEntity {
                    entity_id: to_wire_id(id),
                    kind: ReplicatedKind::Player,
                    position: body.position,
                    velocity: body.velocity,
                });
            }
            if let Some(interactable) = world.interactable_of(id) {
                let transform = world.transform_of(id)?;
                let kind = match interactable.kind {
                    purgatory_simulation::InteractableKind::Portal => ReplicatedKind::Portal,
                    _ => ReplicatedKind::Interactable,
                };
                return Some(SnapshotEntity {
                    entity_id: to_wire_id(id),
                    kind,
                    position: transform.position,
                    velocity: [0.0, 0.0],
                });
            }
            if world.item_instance_at_world_drop(id).is_some() {
                let transform = world.transform_of(id)?;
                return Some(SnapshotEntity {
                    entity_id: to_wire_id(id),
                    kind: ReplicatedKind::Item,
                    position: transform.position,
                    velocity: [0.0, 0.0],
                });
            }
            if world.kind(id) != Some(purgatory_simulation::EntityKind::Generic) {
                return None;
            }
            let transform = world.transform_of(id)?;
            let velocity = world.npc_of(id).map(|n| n.velocity).unwrap_or([0.0, 0.0]);
            Some(SnapshotEntity {
                entity_id: to_wire_id(id),
                kind: ReplicatedKind::Npc,
                position: transform.position,
                velocity,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_simulation::FOOTNOTE_SPAWN_X;

    #[test]
    fn builder_omits_platforms() {
        let mut world = World::footnote_test_stage();
        if let Some(id) = world.player_id() {
            world.despawn(id);
        }
        let floor = world.iter_platforms().next().expect("platform");
        let (transform, state) = purgatory_simulation::PlayerState::standing_on_at(
            floor.id,
            floor.top_surface(),
            FOOTNOTE_SPAWN_X,
        );
        let player = world.spawn_player(transform, state);
        let snap = build(1, 1, player, &world, &[player], 0, 0, 0);
        assert_eq!(snap.entities.len(), 1);
        assert_eq!(snap.local_player_entity, to_wire_id(player));
        assert_eq!(snap.entities[0].kind, ReplicatedKind::Player);
        assert!(snap.local_grounded);
        assert!(!snap.local_grounded_on.is_none());
        assert!(world.iter_platforms().count() >= 1);
    }

    #[test]
    fn visibility_set_filters_entities() {
        let mut world = World::footnote_test_stage();
        if let Some(id) = world.player_id() {
            world.despawn(id);
        }
        let floor = world.iter_platforms().next().expect("platform");
        let (t, s) = purgatory_simulation::PlayerState::standing_on_at(
            floor.id,
            floor.top_surface(),
            FOOTNOTE_SPAWN_X,
        );
        let a = world.spawn_player(t, s);
        let (t, s) = purgatory_simulation::PlayerState::standing_on_at(
            floor.id,
            floor.top_surface(),
            FOOTNOTE_SPAWN_X + 1.0,
        );
        let b = world.spawn_player(t, s);
        let only_a = build(1, 1, a, &world, &[a], 0, 0, 0);
        assert_eq!(only_a.entities.len(), 1);
        assert_eq!(only_a.entities[0].entity_id, to_wire_id(a));
        let both = build(2, 2, b, &world, &[a, b], 0, 0, 0);
        assert_eq!(both.entities.len(), 2);
        assert_eq!(SNAPSHOT_PERIOD_TICKS, 1);
    }

    #[test]
    fn two_recipients_get_distinct_headers() {
        let mut world = World::footnote_test_stage();
        if let Some(id) = world.player_id() {
            world.despawn(id);
        }
        let floor = world.iter_platforms().next().expect("platform");
        let (t, s) = purgatory_simulation::PlayerState::standing_on_at(
            floor.id,
            floor.top_surface(),
            FOOTNOTE_SPAWN_X,
        );
        let a = world.spawn_player(t, s);
        let (t, s) = purgatory_simulation::PlayerState::standing_on_at(
            floor.id,
            floor.top_surface(),
            FOOTNOTE_SPAWN_X + 1.0,
        );
        let b = world.spawn_player(t, s);
        let snap_a = build(1, 1, a, &world, &[a, b], 0, 3, 0);
        let snap_b = build(1, 1, b, &world, &[a, b], 1, 9, 2);
        assert_eq!(snap_a.local_player_entity, to_wire_id(a));
        assert_eq!(snap_b.local_player_entity, to_wire_id(b));
        assert_eq!(snap_a.last_acknowledged_input_sequence, 3);
        assert_eq!(snap_b.last_acknowledged_input_sequence, 9);
        assert_eq!(snap_a.input_epoch, 0);
        assert_eq!(snap_b.input_epoch, 1);
        assert_eq!(snap_b.continuation_debt, 2);
    }

    #[test]
    fn builder_includes_visible_interactables() {
        let mut world = World::footnote_test_stage();
        if let Some(id) = world.player_id() {
            world.despawn(id);
        }
        let floor = world.iter_platforms().next().expect("platform");
        let (transform, state) = purgatory_simulation::PlayerState::standing_on_at(
            floor.id,
            floor.top_surface(),
            FOOTNOTE_SPAWN_X,
        );
        let player = world.spawn_player(transform, state);
        let visible = world.relevance_for(player);
        let snap = build(1, 1, player, &world, &visible, 0, 0, 0);
        assert_eq!(
            snap.entities
                .iter()
                .filter(|e| e.kind == ReplicatedKind::Player)
                .count(),
            1
        );
        assert_eq!(
            snap.entities
                .iter()
                .filter(|e| e.kind == ReplicatedKind::Interactable)
                .count(),
            2,
            "nearby switch + far chest; other-instance portal is not relevant"
        );
        let near = snap
            .entities
            .iter()
            .filter(|e| e.kind == ReplicatedKind::Interactable)
            .min_by(|a, b| {
                a.position[0]
                    .partial_cmp(&b.position[0])
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("near interactable");
        assert!((near.position[0] - (FOOTNOTE_SPAWN_X + 1.6)).abs() < 1e-3);
        assert!(
            near.position[1] > -3.6,
            "fixture must sit on P0, not inside the floor (y={})",
            near.position[1]
        );
    }

    #[test]
    fn builder_consumes_world_relevance_set() {
        let mut world = World::footnote_test_stage();
        if let Some(id) = world.player_id() {
            world.despawn(id);
        }
        let floor = world.iter_platforms().next().expect("platform");
        let (t, s) = purgatory_simulation::PlayerState::standing_on_at(
            floor.id,
            floor.top_surface(),
            FOOTNOTE_SPAWN_X,
        );
        let a = world.spawn_player(t, s);
        let (t, s) = purgatory_simulation::PlayerState::standing_on_at(
            floor.id,
            floor.top_surface(),
            FOOTNOTE_SPAWN_X + 1.0,
        );
        let b = world.spawn_player(t, s);
        world.set_address(
            b,
            purgatory_simulation::WorldAddress::new(
                purgatory_simulation::MapId::DEV,
                purgatory_simulation::ChannelId::DEFAULT,
                purgatory_simulation::InstanceId::from_raw(2),
            ),
        );
        let visible = world.relevance_for(a);
        let snap = build(1, 1, a, &world, &visible, 0, 0, 0);
        assert!(
            snap.entities
                .iter()
                .any(|e| e.entity_id == to_wire_id(a) && e.kind == ReplicatedKind::Player)
        );
        assert!(
            !snap.entities.iter().any(|e| e.entity_id == to_wire_id(b)),
            "other-instance player must not be relevant"
        );
        assert!(world.contains(b));
    }
}
