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
#[allow(dead_code)]
pub fn from_wire_id(id: WireEntityId) -> EntityId {
    EntityId::from_raw(id.index, id.generation)
}

/// Build a full snapshot for one recipient from a visibility set.
///
/// Header acknowledgement and local contact are recipient-specific.
#[must_use]
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
        entities: collect_entities(world, visible),
    }
}

#[must_use]
pub fn collect_entities(world: &World, visible: &[EntityId]) -> Vec<SnapshotEntity> {
    visible
        .iter()
        .filter_map(|&id| {
            let body = world.player_body_of(id)?;
            Some(SnapshotEntity {
                entity_id: to_wire_id(id),
                kind: ReplicatedKind::Player,
                position: body.position,
                velocity: body.velocity,
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
}
