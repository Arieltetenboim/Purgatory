//! Authoritative equipment slots. Presentation, inventory, and combat are out of scope.
//!
//! Slot values are [`Option<ContentId>`]. [`None`] is genuinely empty — not a
//! sentinel content id. Content-pack slot compatibility is validated in
//! `purgatory-content` (Phase 8B). Encode helpers are a measurable
//! representation, not protocol messages.

use purgatory_common::ContentId;

/// Fixed v1 equipment slots. Dense `0..5`. Do not add slots in 8A.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum EquipmentSlot {
    Headwear = 0,
    Bodywear = 1,
    Pants = 2,
    Gloves = 3,
    Boots = 4,
    Weapon = 5,
}

impl EquipmentSlot {
    pub const COUNT: usize = 6;
    pub const ALL: [Self; Self::COUNT] = [
        Self::Headwear,
        Self::Bodywear,
        Self::Pants,
        Self::Gloves,
        Self::Boots,
        Self::Weapon,
    ];

    const VALID_BITS: u8 = (1 << Self::COUNT) - 1;

    #[must_use]
    pub const fn index(self) -> usize {
        self as u8 as usize
    }

    #[must_use]
    pub const fn bit(self) -> u8 {
        1 << (self as u8)
    }

    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Headwear),
            1 => Some(Self::Bodywear),
            2 => Some(Self::Pants),
            3 => Some(Self::Gloves),
            4 => Some(Self::Boots),
            5 => Some(Self::Weapon),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Headwear => "headwear",
            Self::Bodywear => "bodywear",
            Self::Pants => "pants",
            Self::Gloves => "gloves",
            Self::Boots => "boots",
            Self::Weapon => "weapon",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "headwear" => Some(Self::Headwear),
            "bodywear" => Some(Self::Bodywear),
            "pants" => Some(Self::Pants),
            "gloves" => Some(Self::Gloves),
            "boots" => Some(Self::Boots),
            "weapon" => Some(Self::Weapon),
            _ => None,
        }
    }
}

impl std::fmt::Display for EquipmentSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Slot-level dirty bits. One bit per [`EquipmentSlot`]; fits in `u8`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EquipmentDirtyMask {
    bits: u8,
}

impl EquipmentDirtyMask {
    #[must_use]
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    #[must_use]
    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !EquipmentSlot::VALID_BITS != 0 {
            None
        } else {
            Some(Self { bits })
        }
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.bits
    }

    #[must_use]
    pub const fn any(self) -> bool {
        self.bits != 0
    }

    #[must_use]
    pub const fn contains(self, slot: EquipmentSlot) -> bool {
        self.bits & slot.bit() != 0
    }

    pub fn mark(&mut self, slot: EquipmentSlot) {
        self.bits |= slot.bit();
    }

    pub fn merge(&mut self, other: Self) {
        self.bits |= other.bits;
    }

    pub fn clear(&mut self) {
        self.bits = 0;
    }

    pub fn take(&mut self) -> Self {
        core::mem::take(self)
    }

    #[must_use]
    pub const fn only(slot: EquipmentSlot) -> Self {
        Self { bits: slot.bit() }
    }
}

/// Fixed six-slot equipment. All-[`None`] is the valid default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EquipmentState {
    slots: [Option<ContentId>; EquipmentSlot::COUNT],
}

impl EquipmentState {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            slots: [None; EquipmentSlot::COUNT],
        }
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    #[must_use]
    pub fn get(self, slot: EquipmentSlot) -> Option<ContentId> {
        self.slots[slot.index()]
    }

    /// Returns `true` when the stored value changed.
    pub fn set(&mut self, slot: EquipmentSlot, value: Option<ContentId>) -> bool {
        let i = slot.index();
        if self.slots[i] == value {
            false
        } else {
            self.slots[i] = value;
            true
        }
    }

    /// Returns `true` when the slot was not already empty.
    pub fn clear(&mut self, slot: EquipmentSlot) -> bool {
        self.set(slot, None)
    }

    /// Occupied-slot bits (not dirty). Bit set means [`Some`].
    #[must_use]
    pub fn occupied_mask(self) -> EquipmentDirtyMask {
        let mut mask = EquipmentDirtyMask::empty();
        for slot in EquipmentSlot::ALL {
            if self.get(slot).is_some() {
                mask.mark(slot);
            }
        }
        mask
    }
}

/// Slot-oriented delta. Absent mask bits are not part of this payload.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EquipmentDelta {
    mask: EquipmentDirtyMask,
    values: [Option<ContentId>; EquipmentSlot::COUNT],
}

impl EquipmentDelta {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            mask: EquipmentDirtyMask::empty(),
            values: [None; EquipmentSlot::COUNT],
        }
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        !self.mask.any()
    }

    #[must_use]
    pub fn mask(self) -> EquipmentDirtyMask {
        self.mask
    }

    /// `None` if `slot` is not in this delta. `Some(None)` means unequipped.
    #[must_use]
    pub fn get(self, slot: EquipmentSlot) -> Option<Option<ContentId>> {
        if self.mask.contains(slot) {
            Some(self.values[slot.index()])
        } else {
            None
        }
    }

    pub fn set(&mut self, slot: EquipmentSlot, value: Option<ContentId>) {
        self.mask.mark(slot);
        self.values[slot.index()] = value;
    }

    #[must_use]
    pub fn single(slot: EquipmentSlot, value: Option<ContentId>) -> Self {
        let mut delta = Self::empty();
        delta.set(slot, value);
        delta
    }

    #[must_use]
    pub fn from_changed(state: &EquipmentState, dirty: EquipmentDirtyMask) -> Self {
        let mut delta = Self {
            mask: dirty,
            values: [None; EquipmentSlot::COUNT],
        };
        for slot in EquipmentSlot::ALL {
            if dirty.contains(slot) {
                delta.values[slot.index()] = state.get(slot);
            }
        }
        delta
    }
}

/// Measurement codec error. Not a protocol [`purgatory_protocol`] failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EquipmentCodecError {
    Truncated,
    InvalidMask,
    InvalidPresence,
}

/// All-empty full state is one occupied-mask byte.
pub const EQUIPMENT_FULL_EMPTY_BYTES: usize = 1;
/// One-slot unequip delta: changed-mask + presence `0`.
pub const EQUIPMENT_DELTA_UNEQUIP_BYTES: usize = 2;
/// One-slot equip delta: changed-mask + presence `1` + `ContentId` token.
pub const EQUIPMENT_DELTA_EQUIP_BYTES: usize = 10;
/// Full state with every slot occupied: mask + six tokens.
pub const EQUIPMENT_FULL_ALL_OCCUPIED_BYTES: usize = 1 + EquipmentSlot::COUNT * 8;

pub fn encode_equipment_full(state: &EquipmentState) -> Vec<u8> {
    let occupied = state.occupied_mask();
    let mut out = Vec::with_capacity(EQUIPMENT_FULL_ALL_OCCUPIED_BYTES);
    out.push(occupied.bits());
    for slot in EquipmentSlot::ALL {
        if let Some(id) = state.get(slot) {
            out.extend_from_slice(&id.token().to_le_bytes());
        }
    }
    out
}

pub fn decode_equipment_full(bytes: &[u8]) -> Result<(EquipmentState, &[u8]), EquipmentCodecError> {
    let (&mask_bits, rest) = bytes.split_first().ok_or(EquipmentCodecError::Truncated)?;
    let occupied =
        EquipmentDirtyMask::from_bits(mask_bits).ok_or(EquipmentCodecError::InvalidMask)?;
    let mut state = EquipmentState::empty();
    let mut rest = rest;
    for slot in EquipmentSlot::ALL {
        if !occupied.contains(slot) {
            continue;
        }
        let token_bytes = rest
            .get(..8)
            .ok_or(EquipmentCodecError::Truncated)?
            .try_into()
            .map_err(|_| EquipmentCodecError::Truncated)?;
        rest = &rest[8..];
        state.slots[slot.index()] = Some(ContentId::from_token(u64::from_le_bytes(token_bytes)));
    }
    Ok((state, rest))
}

pub fn encode_equipment_delta(delta: &EquipmentDelta) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + EquipmentSlot::COUNT * 9);
    out.push(delta.mask.bits());
    for slot in EquipmentSlot::ALL {
        if !delta.mask.contains(slot) {
            continue;
        }
        match delta.values[slot.index()] {
            None => out.push(0),
            Some(id) => {
                out.push(1);
                out.extend_from_slice(&id.token().to_le_bytes());
            }
        }
    }
    out
}

pub fn decode_equipment_delta(
    bytes: &[u8],
) -> Result<(EquipmentDelta, &[u8]), EquipmentCodecError> {
    let (&mask_bits, rest) = bytes.split_first().ok_or(EquipmentCodecError::Truncated)?;
    let mask = EquipmentDirtyMask::from_bits(mask_bits).ok_or(EquipmentCodecError::InvalidMask)?;
    let mut delta = EquipmentDelta {
        mask,
        values: [None; EquipmentSlot::COUNT],
    };
    let mut rest = rest;
    for slot in EquipmentSlot::ALL {
        if !mask.contains(slot) {
            continue;
        }
        let (&presence, after) = rest.split_first().ok_or(EquipmentCodecError::Truncated)?;
        rest = after;
        match presence {
            0 => delta.values[slot.index()] = None,
            1 => {
                let token_bytes = rest
                    .get(..8)
                    .ok_or(EquipmentCodecError::Truncated)?
                    .try_into()
                    .map_err(|_| EquipmentCodecError::Truncated)?;
                rest = &rest[8..];
                delta.values[slot.index()] =
                    Some(ContentId::from_token(u64::from_le_bytes(token_bytes)));
            }
            _ => return Err(EquipmentCodecError::InvalidPresence),
        }
    }
    Ok((delta, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_all_none() {
        let state = EquipmentState::default();
        assert!(state.is_empty());
        for slot in EquipmentSlot::ALL {
            assert!(state.get(slot).is_none());
        }
        assert!(!state.occupied_mask().any());
    }

    #[test]
    fn get_set_clear_each_slot() {
        let mut state = EquipmentState::empty();
        for (i, slot) in EquipmentSlot::ALL.iter().copied().enumerate() {
            let id = ContentId::from_token(100 + i as u64);
            assert!(state.set(slot, Some(id)));
            assert_eq!(state.get(slot), Some(id));
            assert!(state.clear(slot));
            assert!(state.get(slot).is_none());
        }
        assert!(state.is_empty());
    }

    #[test]
    fn idempotent_set_does_not_report_change() {
        let mut state = EquipmentState::empty();
        let id = ContentId::from_token(7);
        assert!(state.set(EquipmentSlot::Weapon, Some(id)));
        assert!(!state.set(EquipmentSlot::Weapon, Some(id)));
        assert!(!state.set(EquipmentSlot::Boots, None));
        assert!(!state.clear(EquipmentSlot::Headwear));
    }

    #[test]
    fn dirty_mask_marks_only_target_slot() {
        let mut dirty = EquipmentDirtyMask::empty();
        dirty.mark(EquipmentSlot::Weapon);
        assert!(dirty.contains(EquipmentSlot::Weapon));
        for slot in EquipmentSlot::ALL {
            if slot != EquipmentSlot::Weapon {
                assert!(!dirty.contains(slot));
            }
        }
        dirty.mark(EquipmentSlot::Boots);
        assert!(dirty.contains(EquipmentSlot::Boots));
        assert!(dirty.contains(EquipmentSlot::Weapon));
        assert!(!dirty.contains(EquipmentSlot::Gloves));
    }

    #[test]
    fn dirty_mask_independent_bits_and_take() {
        let mut dirty = EquipmentDirtyMask::empty();
        for slot in EquipmentSlot::ALL {
            dirty.mark(slot);
            assert!(dirty.contains(slot));
        }
        assert_eq!(dirty.bits(), EquipmentSlot::VALID_BITS);
        let taken = dirty.take();
        assert_eq!(taken.bits(), EquipmentSlot::VALID_BITS);
        assert!(!dirty.any());
        dirty.merge(EquipmentDirtyMask::only(EquipmentSlot::Pants));
        dirty.clear();
        assert!(!dirty.any());
        assert!(EquipmentDirtyMask::from_bits(1 << 6).is_none());
    }

    #[test]
    fn token_zero_is_not_empty() {
        let mut state = EquipmentState::empty();
        let zero = ContentId::from_token(0);
        assert!(state.set(EquipmentSlot::Bodywear, Some(zero)));
        assert_eq!(state.get(EquipmentSlot::Bodywear), Some(zero));
        assert!(!state.is_empty());
        assert!(state.get(EquipmentSlot::Weapon).is_none());
    }

    #[test]
    fn full_roundtrip_empty_and_sizes() {
        let empty = EquipmentState::default();
        let bytes = encode_equipment_full(&empty);
        assert_eq!(bytes.len(), EQUIPMENT_FULL_EMPTY_BYTES);
        let (decoded, rest) = decode_equipment_full(&bytes).unwrap();
        assert!(rest.is_empty());
        assert_eq!(decoded, empty);

        let mut full = EquipmentState::empty();
        for (i, slot) in EquipmentSlot::ALL.iter().copied().enumerate() {
            assert!(full.set(slot, Some(ContentId::from_token(i as u64 + 1))));
        }
        let occupied = encode_equipment_full(&full);
        assert_eq!(occupied.len(), EQUIPMENT_FULL_ALL_OCCUPIED_BYTES);
        let (decoded, rest) = decode_equipment_full(&occupied).unwrap();
        assert!(rest.is_empty());
        assert_eq!(decoded, full);
    }

    #[test]
    fn delta_roundtrip_one_slot_without_resending_all() {
        let id = ContentId::from_token(42);
        let delta = EquipmentDelta::single(EquipmentSlot::Weapon, Some(id));
        let bytes = encode_equipment_delta(&delta);
        assert_eq!(bytes.len(), EQUIPMENT_DELTA_EQUIP_BYTES);
        let (decoded, rest) = decode_equipment_delta(&bytes).unwrap();
        assert!(rest.is_empty());
        assert_eq!(decoded.get(EquipmentSlot::Weapon), Some(Some(id)));
        assert!(decoded.get(EquipmentSlot::Boots).is_none());

        let unequip = EquipmentDelta::single(EquipmentSlot::Boots, None);
        let bytes = encode_equipment_delta(&unequip);
        assert_eq!(bytes.len(), EQUIPMENT_DELTA_UNEQUIP_BYTES);
        let (decoded, _) = decode_equipment_delta(&bytes).unwrap();
        assert_eq!(decoded.get(EquipmentSlot::Boots), Some(None));
        assert!(decoded.get(EquipmentSlot::Weapon).is_none());
    }

    #[test]
    fn slot_name_roundtrip() {
        for slot in EquipmentSlot::ALL {
            assert_eq!(EquipmentSlot::parse(slot.as_str()), Some(slot));
            assert_eq!(slot.to_string(), slot.as_str());
        }
        assert!(EquipmentSlot::parse("cape").is_none());
        assert!(EquipmentSlot::parse("offhand").is_none());
    }

    #[test]
    fn from_changed_uses_current_state_for_dirty_slots_only() {
        let mut state = EquipmentState::empty();
        assert!(state.set(EquipmentSlot::Weapon, Some(ContentId::from_token(9))));
        assert!(state.set(EquipmentSlot::Headwear, Some(ContentId::from_token(3))));
        let dirty = EquipmentDirtyMask::only(EquipmentSlot::Weapon);
        let delta = EquipmentDelta::from_changed(&state, dirty);
        assert_eq!(
            delta.get(EquipmentSlot::Weapon),
            Some(Some(ContentId::from_token(9)))
        );
        assert!(delta.get(EquipmentSlot::Headwear).is_none());
    }
}
