//! Phase 8C: equipment domain lifecycle under authoritative slot writes.

use crate::fixtures::RuntimeFixtures;
use crate::{ContentId, EquipmentSlot, EquipmentState, World};

fn token(n: u64) -> ContentId {
    ContentId::from_token(n)
}

#[test]
fn first_equip_creates_domain_from_none() {
    let mut world = World::new();
    let player = RuntimeFixtures::test_player(&mut world);
    assert!(world.equipment_of(player).is_none());
    assert!(world.set_equipment_slot(player, EquipmentSlot::Weapon, Some(token(7))));
    let state = world.equipment_of(player).unwrap();
    assert_eq!(state.get(EquipmentSlot::Weapon), Some(token(7)));
    assert!(!state.is_empty());
}

#[test]
fn last_unequip_retains_empty_domain() {
    let mut world = World::new();
    let player = RuntimeFixtures::test_player(&mut world);
    assert!(world.set_equipment_slot(player, EquipmentSlot::Weapon, Some(token(7))));
    assert!(world.clear_equipment_slot(player, EquipmentSlot::Weapon));
    let state = world.equipment_of(player).expect("domain remains");
    assert!(state.is_empty());
    assert!(
        world
            .equipment_slot(player, EquipmentSlot::Weapon)
            .is_none()
    );
}

#[test]
fn unequip_without_domain_does_not_create_domain() {
    let mut world = World::new();
    let player = RuntimeFixtures::test_player(&mut world);
    assert!(world.clear_equipment_slot(player, EquipmentSlot::Weapon));
    assert!(world.equipment_of(player).is_none());
}

#[test]
fn idempotent_equip_does_not_dirty_unrelated_domains() {
    let mut world = World::new();
    let player = RuntimeFixtures::test_player(&mut world);
    assert!(world.set_equipment_slot(player, EquipmentSlot::Weapon, Some(token(7))));
    let _ = world.consume_dirty(player);
    let _ = world.consume_equipment_dirty(player);
    let revs = world.domain_revs_of(player).unwrap();
    let transform = revs.transform;
    let health = revs.health;
    assert!(world.set_equipment_slot(player, EquipmentSlot::Weapon, Some(token(7))));
    let dirty = world.dirty_of(player).unwrap();
    assert!(!dirty.equipment);
    assert!(!dirty.transform);
    assert!(!dirty.health);
    let revs = world.domain_revs_of(player).unwrap();
    assert_eq!(revs.transform, transform);
    assert_eq!(revs.health, health);
    assert_eq!(revs.equipment, 1);
}

#[test]
fn real_equip_does_not_dirty_transform_or_health() {
    let mut world = World::new();
    let player = RuntimeFixtures::test_player(&mut world);
    let _ = world.consume_dirty(player);
    let before = world.domain_revs_of(player).unwrap();
    assert!(world.set_equipment_slot(player, EquipmentSlot::Headwear, Some(token(3))));
    let dirty = world.dirty_of(player).unwrap();
    assert!(dirty.equipment);
    assert!(!dirty.transform);
    assert!(!dirty.health);
    let after = world.domain_revs_of(player).unwrap();
    assert_eq!(after.transform, before.transform);
    assert_eq!(after.health, before.health);
    assert!(after.equipment > before.equipment);
}

#[test]
fn missing_entity_equipment_write_fails() {
    let mut world = World::new();
    let player = RuntimeFixtures::test_player(&mut world);
    world.despawn(player);
    assert!(!world.set_equipment_slot(player, EquipmentSlot::Weapon, Some(token(1))));
}

#[test]
fn spawned_empty_domain_is_not_none() {
    let mut world = World::new();
    let id = world
        .spawn(
            crate::spawn::RuntimeSpawnRequest::transient_at(crate::WorldAddress::DEV)
                .with_equipment(EquipmentState::default()),
        )
        .unwrap();
    assert!(world.equipment_of(id).unwrap().is_empty());
    assert!(world.clear_equipment_slot(id, EquipmentSlot::Pants));
    assert!(world.equipment_of(id).unwrap().is_empty());
}
