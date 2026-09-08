//! Protocol v21 authoritative world-drop pickup request/result.

use purgatory_common::ItemInstanceId;

use crate::{CodecError, WireEntityId};

pub const PICKUP_REQUEST_BYTES: usize = 4 + 8;
pub const PICKUP_ACCEPTED_BYTES: usize = 4 + 8 + 2;
pub const PICKUP_REJECTED_BYTES: usize = 4 + 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PickupRequest {
    pub seq: u32,
    pub target: WireEntityId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum PickupRejectReason {
    StaleRequest = 1,
    InvalidRequest = 2,
    StateBlocked = 3,
    TargetMissing = 4,
    WrongAddress = 5,
    OutOfRange = 6,
    InventoryFull = 7,
}

impl PickupRejectReason {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::StaleRequest),
            2 => Some(Self::InvalidRequest),
            3 => Some(Self::StateBlocked),
            4 => Some(Self::TargetMissing),
            5 => Some(Self::WrongAddress),
            6 => Some(Self::OutOfRange),
            7 => Some(Self::InventoryFull),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerItem {
    PickupAccepted {
        seq: u32,
        item_instance_id: ItemInstanceId,
        slot: u16,
    },
    PickupRejected {
        seq: u32,
        reason: PickupRejectReason,
    },
}

pub(crate) fn decode_item_instance_id(bytes: &[u8]) -> Result<ItemInstanceId, CodecError> {
    let raw = u64::from_le_bytes(bytes.try_into().map_err(|_| CodecError::Truncated)?);
    Ok(ItemInstanceId::from_raw(raw))
}
