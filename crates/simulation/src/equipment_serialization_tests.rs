//! Equipment: serialization contract tests (promoted from Phase 8).
//
//! Covers:
//! - full equipment encode/decode
//! - delta equip/unequip encode/decode
//! - token zero remains valid equipped content, not an empty sentinel
//! - existing exact encoded-size contracts

use crate::{
    ContentId, EQUIPMENT_DELTA_EQUIP_BYTES, EQUIPMENT_DELTA_UNEQUIP_BYTES,
    EQUIPMENT_FULL_EMPTY_BYTES, EquipmentDelta, EquipmentSlot, EquipmentState,
    decode_equipment_delta, decode_equipment_full, encode_equipment_delta, encode_equipment_full,
};

#[test]
fn token_zero_is_equipped_content_not_sentinel_empty() {
    let mut world = crate::World::new();
    let id = crate::fixtures::RuntimeFixtures::transient_replicated(&mut world);
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
