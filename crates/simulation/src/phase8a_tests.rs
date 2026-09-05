//! Phase 8A: authoritative equipment data model and slot dirty tracking.

use crate::fixtures::RuntimeFixtures;
use crate::spawn::RuntimeSpawnRequest;
use crate::{
    ContentId, EQUIPMENT_DELTA_EQUIP_BYTES, EQUIPMENT_DELTA_UNEQUIP_BYTES,
    EQUIPMENT_FULL_EMPTY_BYTES, EquipmentDelta, EquipmentDirtyMask, EquipmentSlot, EquipmentState,
    ReplicationDirtyMask, SimulationTick, World, WorldAddress, decode_equipment_delta,
    decode_equipment_full, encode_equipment_delta, encode_equipment_full,
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
fn token_zero_is_equipped_content_not_sentinel_empty() {
    let mut world = World::new();
    let id = RuntimeFixtures::transient_replicated(&mut world);
    let zero = ContentId::from_token(0);
    assert!(world.set_equipment_slot(id, EquipmentSlot::Pants, Some(zero)));
    assert_eq!(world.equipment_slot(id, EquipmentSlot::Pants), Some(zero));
    assert!(world.equipment_slot(id, EquipmentSlot::Weapon).is_none());
    let encoded = encode_equipment_full(&world.equipment_of(id).unwrap());
    let (decoded, rest) = decode_equipment_full(&encoded).unwrap();
    assert!(rest.is_empty());
    assert_eq!(decoded.get(EquipmentSlot::Pants), Some(zero));
    assert!(decoded.get(EquipmentSlot::Weapon).is_none());
}

#[test]
fn full_and_delta_roundtrip_sizes() {
    let empty = EquipmentState::default();
    let full_empty = encode_equipment_full(&empty);
    assert_eq!(full_empty.len(), EQUIPMENT_FULL_EMPTY_BYTES);
    let (decoded, rest) = decode_equipment_full(&full_empty).unwrap();
    assert!(rest.is_empty());
    assert_eq!(decoded, empty);

    let mut state = EquipmentState::empty();
    let id = ContentId::from_token(99);
    assert!(state.set(EquipmentSlot::Weapon, Some(id)));
    let full = encode_equipment_full(&state);
    assert_eq!(full.len(), EQUIPMENT_FULL_EMPTY_BYTES + 8);
    let (decoded, _) = decode_equipment_full(&full).unwrap();
    assert_eq!(decoded, state);

    let delta = EquipmentDelta::single(EquipmentSlot::Weapon, Some(id));
    let bytes = encode_equipment_delta(&delta);
    assert_eq!(bytes.len(), EQUIPMENT_DELTA_EQUIP_BYTES);
    let (decoded, _) = decode_equipment_delta(&bytes).unwrap();
    assert_eq!(decoded.get(EquipmentSlot::Weapon), Some(Some(id)));
    assert!(decoded.get(EquipmentSlot::Headwear).is_none());

    let unequip = EquipmentDelta::single(EquipmentSlot::Weapon, None);
    assert_eq!(
        encode_equipment_delta(&unequip).len(),
        EQUIPMENT_DELTA_UNEQUIP_BYTES
    );
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
