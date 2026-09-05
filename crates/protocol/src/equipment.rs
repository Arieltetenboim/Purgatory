//! Protocol v12 equipment request + replication payloads.
//!
//! Presentation (bones, anchors, visuals, coverage) is never on the wire.
//! Slot identity is `u8` `0..=5` matching simulation [`EquipmentSlot`] dense
//! indices. Content is a [`ContentId`] token.

use purgatory_common::ContentId;

use crate::CodecError;

pub const EQUIPMENT_SLOT_COUNT: usize = 6;
const VALID_SLOT_BITS: u8 = (1 << EQUIPMENT_SLOT_COUNT) - 1;

/// Wire sizes (payload only, no control tag).
pub const EQUIP_REQUEST_BYTES: usize = 4 + 1 + 8;
pub const UNEQUIP_REQUEST_BYTES: usize = 4 + 1;
pub const EQUIPMENT_ACCEPTED_BYTES: usize = 4;
pub const EQUIPMENT_REJECTED_BYTES: usize = 4 + 1;
/// Full state, all slots empty (occupied-mask only).
pub const EQUIPMENT_FULL_EMPTY_BYTES: usize = 1;
/// Full state, one occupied slot.
pub const EQUIPMENT_FULL_ONE_OCCUPIED_BYTES: usize = 1 + 8;
/// Full state, six occupied slots.
pub const EQUIPMENT_FULL_ALL_OCCUPIED_BYTES: usize = 1 + EQUIPMENT_SLOT_COUNT * 8;
/// One-slot equip delta: changed-mask + presence + token.
pub const EQUIPMENT_DELTA_EQUIP_BYTES: usize = 10;
/// One-slot unequip delta: changed-mask + presence `0`.
pub const EQUIPMENT_DELTA_UNEQUIP_BYTES: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EquipRequest {
    pub seq: u32,
    pub slot: u8,
    pub content_id: ContentId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnequipRequest {
    pub seq: u32,
    pub slot: u8,
}

/// Compact reject reason. Unknown wire values are [`CodecError::InvalidValue`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum EquipmentRejectReason {
    UnknownContent = 1,
    SlotMismatch = 2,
    StaleRequest = 3,
    InvalidRequest = 4,
    StateBlocked = 5,
}

impl EquipmentRejectReason {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::UnknownContent),
            2 => Some(Self::SlotMismatch),
            3 => Some(Self::StaleRequest),
            4 => Some(Self::InvalidRequest),
            5 => Some(Self::StateBlocked),
            _ => None,
        }
    }
}

impl std::fmt::Display for EquipmentRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::UnknownContent => "UnknownContent",
            Self::SlotMismatch => "SlotMismatch",
            Self::StaleRequest => "StaleRequest",
            Self::InvalidRequest => "InvalidRequest",
            Self::StateBlocked => "StateBlocked",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerEquipment {
    Accepted {
        seq: u32,
    },
    Rejected {
        seq: u32,
        reason: EquipmentRejectReason,
    },
}

/// Authoritative six-slot state. Domain presence is `Option<Self>` on Enter.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReplicatedEquipment {
    slots: [Option<ContentId>; EQUIPMENT_SLOT_COUNT],
}

impl ReplicatedEquipment {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            slots: [None; EQUIPMENT_SLOT_COUNT],
        }
    }

    #[must_use]
    pub fn get(self, slot: u8) -> Option<ContentId> {
        self.slots.get(usize::from(slot)).copied().flatten()
    }

    pub fn set(&mut self, slot: u8, value: Option<ContentId>) -> bool {
        let Some(slot) = self.slots.get_mut(usize::from(slot)) else {
            return false;
        };
        if *slot == value {
            return false;
        }
        *slot = value;
        true
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    pub fn apply_delta(&mut self, delta: &ReplicatedEquipmentDelta) {
        for slot in 0..EQUIPMENT_SLOT_COUNT as u8 {
            if let Some(value) = delta.get(slot) {
                let _ = self.set(slot, value);
            }
        }
    }
}

/// Slot-oriented delta. Absent mask bits are not in this payload.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReplicatedEquipmentDelta {
    mask: u8,
    values: [Option<ContentId>; EQUIPMENT_SLOT_COUNT],
}

impl ReplicatedEquipmentDelta {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            mask: 0,
            values: [None; EQUIPMENT_SLOT_COUNT],
        }
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.mask == 0
    }

    #[must_use]
    pub const fn mask(self) -> u8 {
        self.mask
    }

    /// `None` if `slot` is not in this delta. `Some(None)` means unequipped.
    #[must_use]
    pub fn get(self, slot: u8) -> Option<Option<ContentId>> {
        if self.mask & (1 << slot) == 0 {
            return None;
        }
        Some(self.values[usize::from(slot)])
    }

    pub fn set(&mut self, slot: u8, value: Option<ContentId>) {
        if usize::from(slot) >= EQUIPMENT_SLOT_COUNT {
            return;
        }
        self.mask |= 1 << slot;
        self.values[usize::from(slot)] = value;
    }

    #[must_use]
    pub fn single(slot: u8, value: Option<ContentId>) -> Self {
        let mut delta = Self::empty();
        delta.set(slot, value);
        delta
    }
}

#[must_use]
pub fn slot_valid(slot: u8) -> bool {
    usize::from(slot) < EQUIPMENT_SLOT_COUNT
}

pub fn encode_equipment_full(state: &ReplicatedEquipment) -> Vec<u8> {
    let mut mask = 0u8;
    for (i, slot) in state.slots.iter().enumerate() {
        if slot.is_some() {
            mask |= 1 << i;
        }
    }
    let mut out = Vec::with_capacity(EQUIPMENT_FULL_ALL_OCCUPIED_BYTES);
    out.push(mask);
    for id in state.slots.into_iter().flatten() {
        out.extend_from_slice(&id.token().to_le_bytes());
    }
    out
}

pub fn decode_equipment_full(bytes: &[u8]) -> Result<(ReplicatedEquipment, &[u8]), CodecError> {
    let (&mask, rest) = bytes.split_first().ok_or(CodecError::Truncated)?;
    if mask & !VALID_SLOT_BITS != 0 {
        return Err(CodecError::InvalidValue);
    }
    let mut state = ReplicatedEquipment::empty();
    let mut rest = rest;
    for i in 0..EQUIPMENT_SLOT_COUNT {
        if mask & (1 << i) == 0 {
            continue;
        }
        let token_bytes: [u8; 8] = rest
            .get(..8)
            .ok_or(CodecError::Truncated)?
            .try_into()
            .map_err(|_| CodecError::Truncated)?;
        rest = &rest[8..];
        state.slots[i] = Some(ContentId::from_token(u64::from_le_bytes(token_bytes)));
    }
    Ok((state, rest))
}

pub fn encode_equipment_delta(delta: &ReplicatedEquipmentDelta) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + EQUIPMENT_SLOT_COUNT * 9);
    out.push(delta.mask);
    for i in 0..EQUIPMENT_SLOT_COUNT {
        if delta.mask & (1 << i) == 0 {
            continue;
        }
        match delta.values[i] {
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
) -> Result<(ReplicatedEquipmentDelta, &[u8]), CodecError> {
    let (&mask, rest) = bytes.split_first().ok_or(CodecError::Truncated)?;
    if mask & !VALID_SLOT_BITS != 0 {
        return Err(CodecError::InvalidValue);
    }
    let mut delta = ReplicatedEquipmentDelta {
        mask,
        values: [None; EQUIPMENT_SLOT_COUNT],
    };
    let mut rest = rest;
    for i in 0..EQUIPMENT_SLOT_COUNT {
        if mask & (1 << i) == 0 {
            continue;
        }
        if rest.is_empty() {
            return Err(CodecError::Truncated);
        }
        match rest[0] {
            0 => {
                delta.values[i] = None;
                rest = &rest[1..];
            }
            1 => {
                let token_bytes: [u8; 8] = rest
                    .get(1..9)
                    .ok_or(CodecError::Truncated)?
                    .try_into()
                    .map_err(|_| CodecError::Truncated)?;
                rest = &rest[9..];
                delta.values[i] = Some(ContentId::from_token(u64::from_le_bytes(token_bytes)));
            }
            _ => return Err(CodecError::InvalidValue),
        }
    }
    Ok((delta, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_payload_sizes() {
        assert_eq!(EQUIP_REQUEST_BYTES, 13);
        assert_eq!(UNEQUIP_REQUEST_BYTES, 5);
        assert_eq!(EQUIPMENT_ACCEPTED_BYTES, 4);
        assert_eq!(EQUIPMENT_REJECTED_BYTES, 5);
    }

    #[test]
    fn full_empty_and_occupied_sizes() {
        let empty = ReplicatedEquipment::empty();
        assert_eq!(
            encode_equipment_full(&empty).len(),
            EQUIPMENT_FULL_EMPTY_BYTES
        );
        let mut one = ReplicatedEquipment::empty();
        one.set(5, Some(ContentId::from_token(1)));
        assert_eq!(
            encode_equipment_full(&one).len(),
            EQUIPMENT_FULL_ONE_OCCUPIED_BYTES
        );
        let mut full = ReplicatedEquipment::empty();
        for i in 0..EQUIPMENT_SLOT_COUNT as u8 {
            full.set(i, Some(ContentId::from_token(u64::from(i) + 1)));
        }
        assert_eq!(
            encode_equipment_full(&full).len(),
            EQUIPMENT_FULL_ALL_OCCUPIED_BYTES
        );
    }

    #[test]
    fn delta_one_slot_sizes() {
        let equip = ReplicatedEquipmentDelta::single(5, Some(ContentId::from_token(9)));
        assert_eq!(
            encode_equipment_delta(&equip).len(),
            EQUIPMENT_DELTA_EQUIP_BYTES
        );
        let unequip = ReplicatedEquipmentDelta::single(4, None);
        assert_eq!(
            encode_equipment_delta(&unequip).len(),
            EQUIPMENT_DELTA_UNEQUIP_BYTES
        );
    }

    #[test]
    fn full_roundtrip_token_zero_is_occupied() {
        let mut state = ReplicatedEquipment::empty();
        state.set(1, Some(ContentId::from_token(0)));
        let bytes = encode_equipment_full(&state);
        let (decoded, rest) = decode_equipment_full(&bytes).unwrap();
        assert!(rest.is_empty());
        assert_eq!(decoded.get(1), Some(ContentId::from_token(0)));
        assert!(decoded.get(0).is_none());
    }

    #[test]
    fn delta_does_not_include_unchanged_slots() {
        let delta = ReplicatedEquipmentDelta::single(5, Some(ContentId::from_token(3)));
        assert!(delta.get(0).is_none());
        assert_eq!(delta.get(5), Some(Some(ContentId::from_token(3))));
    }
}
