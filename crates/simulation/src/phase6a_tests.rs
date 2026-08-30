//! Phase 6A composition, spawn, mutation, dirty tracking.

use crate::fixtures::RuntimeFixtures;
use crate::spawn::RuntimeSpawnRequest;
use crate::{ContentId, DirtyFlags, EntityKind, Health, Transform, World, WorldAddress};

#[test]
fn composition_allows_multiple_archetypes() {
    let mut world = World::new();
    let player = RuntimeFixtures::test_player(&mut world);
    let mob = RuntimeFixtures::test_mob_like(&mut world);
    let logical = RuntimeFixtures::without_transform(&mut world);
    assert_eq!(world.kind(player), Some(EntityKind::Player));
    assert_eq!(world.kind(mob), Some(EntityKind::Generic));
    assert_eq!(world.kind(logical), Some(EntityKind::Generic));
    assert!(world.transform_of(player).is_some());
    assert!(world.transform_of(logical).is_none());
    assert!(world.health_of(mob).is_some());
}

#[test]
fn systems_query_required_capabilities() {
    let mut world = World::new();
    let player = RuntimeFixtures::test_player(&mut world);
    let _mob = RuntimeFixtures::test_mob_like(&mut world);
    let movable: Vec<_> = world.movable_in_address(WorldAddress::DEV).collect();
    assert_eq!(movable, vec![player]);
    let replicated: Vec<_> = world.replicated_in_address(WorldAddress::DEV).collect();
    assert!(replicated.contains(&player));
}

#[test]
fn content_backed_and_transient_spawn() {
    let mut world = World::new();
    let content = world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([1.0, 1.0]))
                .with_content(ContentId::from_token(7))
                .visible(),
        )
        .unwrap();
    let transient = RuntimeFixtures::transient_replicated(&mut world);
    assert_eq!(world.content_id_of(content), Some(ContentId::from_token(7)));
    assert!(world.content_id_of(transient).is_none());
}

#[test]
fn mutation_marks_dirty_query_does_not() {
    let mut world = World::new();
    let id = RuntimeFixtures::transient_replicated(&mut world);
    let _ = world.consume_dirty(id);
    assert_eq!(world.dirty_of(id), Some(DirtyFlags::default()));
    let _ = world.entities_at(WorldAddress::DEV).count();
    assert_eq!(world.dirty_of(id), Some(DirtyFlags::default()));
    assert!(world.set_transform(id, Transform::from_position([9.0, 1.0])));
    let dirty = world.dirty_of(id).unwrap();
    assert!(dirty.transform);
    assert!(!dirty.health);
    let taken = world.consume_dirty(id).unwrap();
    assert!(taken.transform);
    assert!(!world.dirty_of(id).unwrap().any());
}

#[test]
fn health_dirty_is_separate_from_transform() {
    let mut world = World::new();
    let id = RuntimeFixtures::test_mob_like(&mut world);
    let _ = world.consume_dirty(id);
    assert!(world.set_health(id, Health::full(5.0)));
    let dirty = world.dirty_of(id).unwrap();
    assert!(dirty.health);
    assert!(!dirty.transform);
}

#[test]
fn address_transition_marks_membership_dirty() {
    let mut world = World::new();
    let id = RuntimeFixtures::without_transform(&mut world);
    let _ = world.consume_dirty(id);
    world.set_address(
        id,
        WorldAddress::new(
            purgatory_common::MapId::from_raw(2),
            purgatory_common::ChannelId::DEFAULT,
            purgatory_common::InstanceId::DEFAULT,
        ),
    );
    assert!(world.dirty_of(id).unwrap().membership);
    assert!(world.contains(id));
}

#[test]
fn despawn_is_lifecycle_not_content_mutation() {
    let mut world = World::new();
    let content = ContentId::from_token(3);
    let id = world
        .spawn(RuntimeSpawnRequest::transient_at(WorldAddress::DEV).with_content(content))
        .unwrap();
    assert!(world.despawn(id));
    assert_eq!(content, ContentId::from_token(3));
    assert!(!world.contains(id));
}

#[test]
fn fixtures_other_instance_isolated() {
    let mut world = World::new();
    let here = RuntimeFixtures::transient_replicated(&mut world);
    let there = RuntimeFixtures::in_other_instance(&mut world);
    assert!(!world.relevance_for(here).contains(&there));
}

#[test]
fn interactable_near_finds_capability() {
    let mut world = World::new();
    let id = RuntimeFixtures::test_interactable(&mut world);
    let found: Vec<_> = world
        .interactable_near(WorldAddress::DEV, [2.0, 1.0], 10.0)
        .collect();
    assert_eq!(found, vec![id]);
}

#[test]
fn slot_iteration_order_is_index_order() {
    let mut world = World::new();
    let a = RuntimeFixtures::without_transform(&mut world);
    let b = RuntimeFixtures::without_transform(&mut world);
    let ids: Vec<_> = world.iter().collect();
    assert_eq!(ids, vec![a, b]);
}
