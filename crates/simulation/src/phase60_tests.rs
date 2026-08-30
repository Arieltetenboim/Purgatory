//! Phase 6.0 runtime foundation: identity, WorldAddress, query, relevance.

use crate::{
    ChannelId, ContentId, EntityId, EntityKind, EntityLifecycle, InstanceId, MapId, PersistentId,
    Platform, ReplicationClass, ReplicationMeta, RuntimeEntityId, Transform, World, WorldAddress,
};

fn tiny_platform() -> (Transform, Platform) {
    (
        Transform::from_position([0.0, 0.0]),
        Platform::solid([0.1, 0.1]),
    )
}

fn spawn_player_at(world: &mut World, x: f32) -> EntityId {
    let floor = world
        .iter_platforms()
        .next()
        .map(|p| (p.id, p.top_surface()));
    let (floor_id, floor_top) = match floor {
        Some(pair) => pair,
        None => {
            let (t, plat) = tiny_platform();
            let id = world.spawn_platform(t, plat);
            (id, 0.1)
        }
    };
    let (transform, state) = crate::PlayerState::standing_on_at(floor_id, floor_top, x);
    world.spawn_player(transform, state)
}

#[test]
fn runtime_entity_id_is_entity_id() {
    let mut world = World::new();
    let (t, p) = tiny_platform();
    let id: RuntimeEntityId = world.spawn_platform(t, p);
    let as_entity: EntityId = id;
    assert!(world.contains(as_entity));
}

#[test]
fn stale_generation_rejected_after_despawn() {
    let mut world = World::new();
    let (t, p) = tiny_platform();
    let id = world.spawn_platform(t, p);
    assert!(world.despawn(id));
    assert!(!world.contains(id));
    assert!(world.address_of(id).is_none());
    assert!(world.lifecycle_of(id).is_none());
    assert!(world.relevance_for(id).is_empty());
}

#[test]
fn content_id_cannot_be_used_as_runtime_id() {
    let content = ContentId::from_token(42);
    let mut world = World::new();
    let (t, p) = tiny_platform();
    let runtime = world.spawn_platform(t, p);
    assert!(world.set_content_id(runtime, Some(content)));
    assert_eq!(world.content_id_of(runtime), Some(content));
    assert_ne!(u64::from(runtime.index()), content.token());
}

#[test]
fn transient_entities_need_no_persistent_id() {
    let mut world = World::new();
    let (t, p) = tiny_platform();
    let id = world.spawn_platform(t, p);
    assert!(world.persistent_id_of(id).is_none());
    assert!(world.contains(id));
}

#[test]
fn spawn_despawn_does_not_mutate_content_identity_value() {
    let content = ContentId::from_token(99);
    let mut world = World::new();
    let (t, p) = tiny_platform();
    let id = world.spawn_platform(t, p);
    assert!(world.set_content_id(id, Some(content)));
    assert_eq!(content, ContentId::from_token(99));
    assert!(world.despawn(id));
    assert_eq!(content, ContentId::from_token(99));
    let again = world.spawn_platform(t, p);
    assert!(world.content_id_of(again).is_none());
    assert_eq!(content.token(), 99);
}

#[test]
fn persistent_id_is_optional_and_not_required_on_spawn() {
    let mut world = World::new();
    let (t, p) = tiny_platform();
    let id = world.spawn_platform(t, p);
    assert!(world.set_persistent_id(id, Some(PersistentId::from_token(5))));
    assert_eq!(world.persistent_id_of(id).map(PersistentId::token), Some(5));
}

#[test]
fn default_spawn_uses_dev_world_address() {
    let mut world = World::new();
    let (t, p) = tiny_platform();
    let id = world.spawn_platform(t, p);
    assert_eq!(world.address_of(id), Some(WorldAddress::DEV));
    assert_eq!(world.lifecycle_of(id), Some(EntityLifecycle::Active));
}

#[test]
fn address_transition_is_not_despawn() {
    let mut world = World::new();
    let (t, p) = tiny_platform();
    let id = world.spawn_platform(t, p);
    let other = WorldAddress::new(MapId::from_raw(2), ChannelId::DEFAULT, InstanceId::DEFAULT);
    assert!(world.set_address(id, other));
    assert!(world.contains(id));
    assert_eq!(world.address_of(id), Some(other));
    assert_eq!(world.lifecycle_of(id), Some(EntityLifecycle::Active));
}

#[test]
fn leave_world_is_not_despawn() {
    let mut world = World::new();
    let (t, p) = tiny_platform();
    let id = world.spawn_platform(t, p);
    assert!(world.leave_world(id));
    assert!(world.contains(id));
    assert_eq!(world.lifecycle_of(id), Some(EntityLifecycle::LeftWorld));
    assert_eq!(world.entities_at(WorldAddress::DEV).count(), 0);
    assert!(world.relevance_for(id).is_empty());
}

#[test]
fn query_filters_map_channel_instance() {
    let mut world = World::new();
    let (t, p) = tiny_platform();
    let a = world.spawn_platform(t, p);
    let b = world.spawn_platform(t, p);
    let c = world.spawn_platform(t, p);
    world.set_address(
        b,
        WorldAddress::new(MapId::from_raw(2), ChannelId::DEFAULT, InstanceId::DEFAULT),
    );
    world.set_address(
        c,
        WorldAddress::new(MapId::DEV, ChannelId::from_raw(3), InstanceId::DEFAULT),
    );
    let at_dev: Vec<_> = world.entities_at(WorldAddress::DEV).collect();
    assert_eq!(at_dev, vec![a]);
    assert_eq!(world.entities_in_map(MapId::from_raw(2)).count(), 1);
    world.set_address(
        c,
        WorldAddress::new(MapId::DEV, ChannelId::DEFAULT, InstanceId::from_raw(4)),
    );
    assert_eq!(
        world.entities_in_instance(InstanceId::from_raw(4)).count(),
        1
    );
}

#[test]
fn entities_near_uses_transform_within_address() {
    let mut world = World::new();
    let near = world.spawn_platform(
        Transform::from_position([1.0, 0.0]),
        Platform::solid([0.1, 0.1]),
    );
    let far = world.spawn_platform(
        Transform::from_position([50.0, 0.0]),
        Platform::solid([0.1, 0.1]),
    );
    let found: Vec<_> = world
        .entities_near(WorldAddress::DEV, [1.0, 0.0], 2.0)
        .collect();
    assert!(found.contains(&near));
    assert!(!found.contains(&far));
}

#[test]
fn different_instance_excluded_from_relevance() {
    let mut world = World::footnote_test_stage();
    let observer = spawn_player_at(&mut world, 0.0);
    let other = spawn_player_at(&mut world, 1.0);
    world.set_address(
        other,
        WorldAddress::new(MapId::DEV, ChannelId::DEFAULT, InstanceId::from_raw(2)),
    );
    let set = world.relevance_for(observer);
    assert!(set.contains(&observer));
    assert!(!set.contains(&other));
    assert!(world.contains(other));
}

#[test]
fn different_channel_excluded_from_relevance() {
    let mut world = World::footnote_test_stage();
    let observer = spawn_player_at(&mut world, 0.0);
    let other = spawn_player_at(&mut world, 1.0);
    world.set_address(
        other,
        WorldAddress::new(MapId::DEV, ChannelId::from_raw(1), InstanceId::DEFAULT),
    );
    let set = world.relevance_for(observer);
    assert!(!set.contains(&other));
}

#[test]
fn compatible_membership_is_a_candidate() {
    let mut world = World::footnote_test_stage();
    let a = spawn_player_at(&mut world, 0.0);
    let b = spawn_player_at(&mut world, 1.0);
    let set = world.relevance_for(a);
    assert!(set.contains(&a));
    assert!(set.contains(&b));
}

#[test]
fn observer_visibility_does_not_despawn() {
    let mut world = World::footnote_test_stage();
    let observer = spawn_player_at(&mut world, 0.0);
    let other = spawn_player_at(&mut world, 1.0);
    world.set_address(
        other,
        WorldAddress::new(MapId::DEV, ChannelId::DEFAULT, InstanceId::from_raw(2)),
    );
    let _ = world.relevance_for(observer);
    assert!(world.contains(other));
    assert_eq!(world.lifecycle_of(other), Some(EntityLifecycle::Active));
    assert_eq!(world.kind(other), Some(EntityKind::Player));
}

#[test]
fn relevance_transition_is_deterministic() {
    let mut world = World::footnote_test_stage();
    let observer = spawn_player_at(&mut world, 0.0);
    let other = spawn_player_at(&mut world, 1.0);
    assert!(world.relevance_for(observer).contains(&other));
    world.set_address(
        other,
        WorldAddress::new(MapId::DEV, ChannelId::DEFAULT, InstanceId::from_raw(2)),
    );
    assert!(!world.relevance_for(observer).contains(&other));
    world.set_address(other, WorldAddress::DEV);
    assert!(world.relevance_for(observer).contains(&other));
}

#[test]
fn platforms_are_not_replicated_by_default() {
    let mut world = World::new();
    let (t, p) = tiny_platform();
    let plat = world.spawn_platform(t, p);
    let player = spawn_player_at(&mut world, 0.0);
    assert_eq!(
        world.replication_of(plat).map(|m| m.class),
        Some(ReplicationClass::None)
    );
    assert_eq!(
        world.replication_of(player).map(|m| m.class),
        Some(ReplicationClass::VisibleObservers)
    );
    let set = world.relevance_for(player);
    assert!(!set.contains(&plat));
    assert!(set.contains(&player));
}

#[test]
fn owner_only_is_relevant_only_to_self() {
    let mut world = World::footnote_test_stage();
    let a = spawn_player_at(&mut world, 0.0);
    let b = spawn_player_at(&mut world, 1.0);
    world.set_replication(b, ReplicationMeta::owner_only());
    assert!(!world.relevance_for(a).contains(&b));
    assert!(world.relevance_for(b).contains(&b));
}

#[test]
fn stale_set_address_fails() {
    let mut world = World::new();
    let (t, p) = tiny_platform();
    let id = world.spawn_platform(t, p);
    assert!(world.despawn(id));
    assert!(!world.set_address(id, WorldAddress::DEV));
}
