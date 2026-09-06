//! Equipment: runtime behavior and lifecycle tests (promoted from Phase 8).
//
//! Covers:
//! - equipment domain creation/lifecycle
//! - get/set/clear slot behavior
//! - all six independent slots
//! - authoritative dirty masks and domain revisions
//! - idempotent writes
//! - spawn/despawn/missing-entity behavior
//! - empty vs absent equipment domain semantics
//! - idle equipment must not create replication churn

use crate::fixtures::RuntimeFixtures;
use crate::spawn::RuntimeSpawnRequest;
use crate::{
    ContentId, EquipmentDirtyMask, EquipmentSlot, EquipmentState, ReplicationDirtyMask,
    SimulationTick, World, WorldAddress,
};

#[test]
fn default_and_spawned_player_have_no_equipment() {
    let mut world = World::new();
    let player = RuntimeFixtures::test_player(&mut world);
    assert!(EquipmentState::default().is_empty());
    assert!(world.equipment_of(player).is_none());
    for slot in EquipmentSlot::ALL {
        assert!(world.equipment_slot(player, slot).is_none());
    }
    assert_eq!(world.kind(player), Some(crate::EntityKind::Player));
    assert!(world.contains(player));
    assert!(world.transform_of(player).is_some());
    assert!(!world.dirty_of(player).unwrap().equipment);
    assert!(!world.equipment_dirty_of(player).unwrap().any());
    assert_eq!(world.domain_revs_of(player).unwrap().equipment, 0);
}

#[test]
fn empty_equipment_state_does_not_require_placeholder_content() {
    let mut world = World::new();
    let id = world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_equipment(EquipmentState::default()),
        )
        .unwrap();
    let state = world.equipment_of(id).unwrap();
    assert!(state.is_empty());
    for slot in EquipmentSlot::ALL {
        assert!(state.get(slot).is_none());
        assert!(world.equipment_slot(id, slot).is_none());
    }
    assert!(!world.dirty_of(id).unwrap().equipment);
    assert!(!world.equipment_dirty_of(id).unwrap().any());
    assert_eq!(world.domain_revs_of(id).unwrap().equipment, 0);
}

#[test]
fn get_set_clear_each_slot_on_world() {
    let mut world = World::new();
    let id = RuntimeFixtures::transient_replicated(&mut world);
    let _ = world.consume_dirty(id);
    let _ = world.consume_equipment_dirty(id);
    for (i, slot) in EquipmentSlot::ALL.iter().copied().enumerate() {
        let content = ContentId::from_token(50 + i as u64);
        assert!(world.set_equipment_slot(id, slot, Some(content)));
        assert_eq!(world.equipment_slot(id, slot), Some(content));
        assert!(world.clear_equipment_slot(id, slot));
        assert!(world.equipment_slot(id, slot).is_none());
    }
}

#[test]
fn idempotent_set_does_not_dirty() {
    let mut world = World::new();
    let id = RuntimeFixtures::transient_replicated(&mut world);
    let _ = world.consume_dirty(id);
    let _ = world.consume_equipment_dirty(id);
    world.clear_replication_dirty();
    let content = ContentId::from_token(8);
    assert!(world.set_equipment_slot(id, EquipmentSlot::Weapon, Some(content)));
    let _ = world.consume_dirty(id);
    let _ = world.consume_equipment_dirty(id);
    world.clear_replication_dirty();
    let revs = world.domain_revs_of(id).unwrap().equipment;
    assert!(world.set_equipment_slot(id, EquipmentSlot::Weapon, Some(content)));
    assert!(!world.dirty_of(id).unwrap().equipment);
    assert!(!world.equipment_dirty_of(id).unwrap().any());
    assert_eq!(world.domain_revs_of(id).unwrap().equipment, revs);
    assert_eq!(world.replication_dirty_len(), 0);
    assert!(world.set_equipment_slot(id, EquipmentSlot::Boots, None));
    assert!(!world.dirty_of(id).unwrap().equipment);
}

#[test]
fn some_to_none_dirties_only_target_slot() {
    let mut world = World::new();
    let id = RuntimeFixtures::transient_replicated(&mut world);
    let _ = world.consume_dirty(id);
    let _ = world.consume_equipment_dirty(id);
    world.clear_replication_dirty();
    let helm = ContentId::from_token(11);
    let boots = ContentId::from_token(12);
    assert!(world.set_equipment_slot(id, EquipmentSlot::Headwear, Some(helm)));
    assert!(world.set_equipment_slot(id, EquipmentSlot::Boots, Some(boots)));
    let _ = world.consume_dirty(id);
    let _ = world.consume_equipment_dirty(id);
    world.clear_replication_dirty();

    assert!(world.clear_equipment_slot(id, EquipmentSlot::Boots));
    let dirty = world.equipment_dirty_of(id).unwrap();
    assert!(dirty.contains(EquipmentSlot::Boots));
    assert!(!dirty.contains(EquipmentSlot::Headwear));
    assert!(!dirty.contains(EquipmentSlot::Weapon));
    assert!(world.dirty_of(id).unwrap().equipment);
    assert!(!world.dirty_of(id).unwrap().transform);
    assert!(!world.dirty_of(id).unwrap().health);
    assert_eq!(
        world.equipment_slot(id, EquipmentSlot::Headwear),
        Some(helm)
    );
    assert!(world.equipment_slot(id, EquipmentSlot::Boots).is_none());
}

#[test]
fn one_slot_change_does_not_dirty_others() {
    let mut world = World::new();
    let id = RuntimeFixtures::transient_replicated(&mut world);
    let _ = world.consume_dirty(id);
    let _ = world.consume_equipment_dirty(id);
    world.clear_replication_dirty();
    assert!(world.set_equipment_slot(id, EquipmentSlot::Weapon, Some(ContentId::from_token(3))));
    let dirty = world.equipment_dirty_of(id).unwrap();
    assert!(dirty.contains(EquipmentSlot::Weapon));
    for slot in EquipmentSlot::ALL {
        if slot != EquipmentSlot::Weapon {
            assert!(!dirty.contains(slot));
        }
    }
    let drained = world.drain_replication_dirty();
    let mask = drained.get(&id).copied().unwrap();
    assert_eq!(
        mask,
        ReplicationDirtyMask::equipment_only(EquipmentDirtyMask::only(EquipmentSlot::Weapon))
    );
}

#[test]
fn dirty_mask_supports_all_six_slots_independently() {
    let mut world = World::new();
    let id = RuntimeFixtures::transient_replicated(&mut world);
    let _ = world.consume_equipment_dirty(id);
    world.clear_replication_dirty();
    for (i, slot) in EquipmentSlot::ALL.iter().copied().enumerate() {
        assert!(world.set_equipment_slot(id, slot, Some(ContentId::from_token(i as u64 + 1))));
    }
    let dirty = world.equipment_dirty_of(id).unwrap();
    for slot in EquipmentSlot::ALL {
        assert!(dirty.contains(slot));
    }
    let mask = world.drain_replication_dirty().remove(&id).unwrap();
    assert_eq!(mask.equipment.bits(), (1 << EquipmentSlot::COUNT) - 1);
    assert!(!mask.transform);
    assert!(!mask.health);
}

#[test]
fn clearing_and_resetting_dirty_works() {
    let mut world = World::new();
    let id = RuntimeFixtures::transient_replicated(&mut world);
    let _ = world.consume_dirty(id);
    let _ = world.consume_equipment_dirty(id);
    assert!(world.set_equipment_slot(id, EquipmentSlot::Gloves, Some(ContentId::from_token(4))));
    assert!(world.dirty_of(id).unwrap().equipment);
    let taken_flags = world.consume_dirty(id).unwrap();
    assert!(taken_flags.equipment);
    assert!(!world.dirty_of(id).unwrap().equipment);
    let taken_slots = world.consume_equipment_dirty(id).unwrap();
    assert!(taken_slots.contains(EquipmentSlot::Gloves));
    assert!(!world.equipment_dirty_of(id).unwrap().any());
    world.clear_replication_dirty();
    assert_eq!(world.replication_dirty_len(), 0);
}

#[test]
fn idle_all_none_does_not_churn_equipment() {
    let mut world = World::new();
    let player = RuntimeFixtures::test_player(&mut world);
    let _ = world.consume_dirty(player);
    let _ = world.consume_equipment_dirty(player);
    world.clear_replication_dirty();
    world.begin_tick(SimulationTick::from_count(1));
    assert!(!world.dirty_of(player).unwrap().equipment);
    assert!(!world.equipment_dirty_of(player).unwrap().any());
    assert_eq!(world.domain_revs_of(player).unwrap().equipment, 0);
    assert_eq!(world.replication_dirty_len(), 0);
}

#[test]
fn missing_entity_rejects_equipment_write() {
    let mut world = World::new();
    let id = RuntimeFixtures::transient_replicated(&mut world);
    assert!(world.despawn(id));
    assert!(!world.set_equipment_slot(id, EquipmentSlot::Weapon, Some(ContentId::from_token(1))));
}

// Phase 8C lifecycle tests, merged into runtime behavior suite:
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
