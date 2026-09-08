//! Owner-private inventory synchronization.

use purgatory_common::{ContentId, ItemInstanceId};

use crate::CodecError;

pub const INVENTORY_CAPACITY: usize = 20;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryEntry {
    pub slot: u16,
    pub item_instance_id: ItemInstanceId,
    pub definition: ContentId,
    pub quantity: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerInventory {
    pub entries: Vec<InventoryEntry>,
}

pub(crate) fn decode_inventory_entry(bytes: &[u8]) -> Result<InventoryEntry, CodecError> {
    if bytes.len() != 22 {
        return Err(CodecError::Truncated);
    }
    Ok(InventoryEntry {
        slot: u16::from_le_bytes(bytes[0..2].try_into().map_err(|_| CodecError::Truncated)?),
        item_instance_id: ItemInstanceId::from_raw(u64::from_le_bytes(
            bytes[2..10].try_into().map_err(|_| CodecError::Truncated)?,
        )),
        definition: ContentId::from_token(u64::from_le_bytes(
            bytes[10..18]
                .try_into()
                .map_err(|_| CodecError::Truncated)?,
        )),
        quantity: u32::from_le_bytes(
            bytes[18..22]
                .try_into()
                .map_err(|_| CodecError::Truncated)?,
        ),
    })
}
