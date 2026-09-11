//! Authoritative runtime item state owned by simulation [`crate::World`].

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::entity::EntityId;
use purgatory_common::{ContentId, ItemInstanceId};

pub const INVENTORY_CAPACITY: usize = 20;

/// Exclusive authoritative item location for the currently implemented slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemLocation {
    WorldDrop(EntityId),
    Inventory {
        owner: EntityId,
        slot: u16,
    },
    Equipped {
        owner: EntityId,
        slot: crate::equipment::EquipmentSlot,
    },
}

/// Canonical runtime item record keyed by [`ItemInstanceId`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemRecord {
    pub definition: ContentId,
    pub quantity: u32,
    pub location: ItemLocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemRuntimeError {
    SpawnFailed,
    InvalidStackLimit,
    InvalidQuantity {
        quantity: u32,
        stack_limit: u32,
    },
    DuplicateInstance(ItemInstanceId),
    MissingItem(ItemInstanceId),
    MissingWorldDropEntity(EntityId),
    WorldDropAlreadyClaimed(EntityId),
    InvalidPickupActor,
    InvalidInventoryOwner,
    PickupTargetMissing(EntityId),
    PickupWrongAddress,
    PickupOutOfRange,
    InventoryFull(EntityId),
    InsufficientInventoryQuantity {
        owner: EntityId,
        definition: ContentId,
        requested: u32,
        available: u32,
    },
    ItemNotInInventory {
        owner: EntityId,
        item: ItemInstanceId,
    },
    EquipmentEmpty {
        owner: EntityId,
        slot: crate::equipment::EquipmentSlot,
    },
}

#[derive(Debug)]
pub(crate) struct ItemRuntimeState {
    records: HashMap<ItemInstanceId, ItemRecord>,
    world_drops: HashMap<EntityId, ItemInstanceId>,
    inventories: HashMap<EntityId, Vec<Option<ItemInstanceId>>>,
    mint_epoch: u32,
    next_counter: u32,
}

impl Default for ItemRuntimeState {
    fn default() -> Self {
        Self {
            records: HashMap::new(),
            world_drops: HashMap::new(),
            inventories: HashMap::new(),
            mint_epoch: initial_mint_epoch(),
            next_counter: 1,
        }
    }
}

impl ItemRuntimeState {
    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub(crate) fn record(&self, id: ItemInstanceId) -> Option<ItemRecord> {
        self.records.get(&id).copied()
    }

    #[must_use]
    pub(crate) fn world_drop_item(&self, entity: EntityId) -> Option<ItemInstanceId> {
        self.world_drops.get(&entity).copied()
    }

    #[must_use]
    pub(crate) fn world_drop_entity_for_item(&self, id: ItemInstanceId) -> Option<EntityId> {
        let record = self.records.get(&id)?;
        match record.location {
            ItemLocation::WorldDrop(entity) => Some(entity),
            ItemLocation::Inventory { .. } | ItemLocation::Equipped { .. } => None,
        }
    }

    #[must_use]
    pub(crate) fn inventory_slot(&self, owner: EntityId, slot: u16) -> Option<ItemInstanceId> {
        self.inventories
            .get(&owner)
            .and_then(|slots| slots.get(usize::from(slot)))
            .copied()
            .flatten()
    }

    #[must_use]
    pub(crate) fn inventory_count(&self, owner: EntityId) -> usize {
        self.inventories
            .get(&owner)
            .map(|slots| slots.iter().filter(|slot| slot.is_some()).count())
            .unwrap_or(0)
    }

    pub(crate) fn inventory_snapshot(
        &self,
        owner: EntityId,
    ) -> Vec<(u16, ItemInstanceId, ItemRecord)> {
        self.inventories
            .get(&owner)
            .into_iter()
            .flat_map(|slots| slots.iter().enumerate())
            .filter_map(|(slot, item)| {
                let id = (*item)?;
                let slot = u16::try_from(slot).ok()?;
                Some((slot, id, self.records.get(&id).copied()?))
            })
            .collect()
    }

    pub(crate) fn inventory_contains(&self, owner: EntityId, id: ItemInstanceId) -> bool {
        self.inventories
            .get(&owner)
            .is_some_and(|slots| slots.iter().flatten().any(|candidate| *candidate == id))
    }

    #[must_use]
    pub(crate) fn equipped_item(
        &self,
        owner: EntityId,
        slot: crate::equipment::EquipmentSlot,
    ) -> Option<ItemInstanceId> {
        self.records.iter().find_map(|(id, record)| {
            (record.location == ItemLocation::Equipped { owner, slot }).then_some(*id)
        })
    }

    #[must_use]
    pub(crate) fn first_inventory_slot(&self, owner: EntityId) -> Option<u16> {
        let slots = self
            .inventories
            .get(&owner)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        slots
            .iter()
            .position(Option::is_none)
            .and_then(|slot| u16::try_from(slot).ok())
            .or_else(|| (slots.len() < INVENTORY_CAPACITY).then_some(slots.len() as u16))
            .filter(|slot| usize::from(*slot) < INVENTORY_CAPACITY)
    }

    pub(crate) fn mint_item_instance_id(&mut self) -> ItemInstanceId {
        loop {
            if self.next_counter == 0 {
                self.next_counter = 1;
                self.mint_epoch = self.mint_epoch.wrapping_add(1).max(1);
            }
            let raw = (u64::from(self.mint_epoch) << 32) | u64::from(self.next_counter);
            self.next_counter = self.next_counter.wrapping_add(1);
            let id = ItemInstanceId::from_raw(raw);
            if !self.records.contains_key(&id) {
                return id;
            }
        }
    }

    pub(crate) fn validate_quantity(
        quantity: u32,
        stack_limit: u32,
    ) -> Result<(), ItemRuntimeError> {
        if stack_limit == 0 {
            return Err(ItemRuntimeError::InvalidStackLimit);
        }
        if quantity == 0 || quantity > stack_limit {
            return Err(ItemRuntimeError::InvalidQuantity {
                quantity,
                stack_limit,
            });
        }
        Ok(())
    }

    pub(crate) fn bind_world_drop(
        &mut self,
        id: ItemInstanceId,
        definition: ContentId,
        quantity: u32,
        stack_limit: u32,
        world_drop_entity: EntityId,
        entity_exists: bool,
    ) -> Result<(), ItemRuntimeError> {
        Self::validate_quantity(quantity, stack_limit)?;
        if self.records.contains_key(&id) {
            return Err(ItemRuntimeError::DuplicateInstance(id));
        }
        if !entity_exists {
            return Err(ItemRuntimeError::MissingWorldDropEntity(world_drop_entity));
        }
        if self.world_drops.contains_key(&world_drop_entity) {
            return Err(ItemRuntimeError::WorldDropAlreadyClaimed(world_drop_entity));
        }
        let record = ItemRecord {
            definition,
            quantity,
            location: ItemLocation::WorldDrop(world_drop_entity),
        };
        self.records.insert(id, record);
        self.world_drops.insert(world_drop_entity, id);
        Ok(())
    }

    pub(crate) fn bind_inventory(
        &mut self,
        id: ItemInstanceId,
        definition: ContentId,
        quantity: u32,
        stack_limit: u32,
        owner: EntityId,
        slot: u16,
    ) -> Result<(), ItemRuntimeError> {
        Self::validate_quantity(quantity, stack_limit)?;
        if self.records.contains_key(&id) {
            return Err(ItemRuntimeError::DuplicateInstance(id));
        }
        let slots = self
            .inventories
            .entry(owner)
            .or_insert_with(|| vec![None; INVENTORY_CAPACITY]);
        let Some(destination) = slots.get_mut(usize::from(slot)) else {
            return Err(ItemRuntimeError::InventoryFull(owner));
        };
        if destination.is_some() {
            return Err(ItemRuntimeError::InventoryFull(owner));
        }
        *destination = Some(id);
        self.records.insert(
            id,
            ItemRecord {
                definition,
                quantity,
                location: ItemLocation::Inventory { owner, slot },
            },
        );
        Ok(())
    }

    pub(crate) fn set_quantity(
        &mut self,
        id: ItemInstanceId,
        quantity: u32,
        stack_limit: u32,
    ) -> Result<(), ItemRuntimeError> {
        Self::validate_quantity(quantity, stack_limit)?;
        let Some(record) = self.records.get_mut(&id) else {
            return Err(ItemRuntimeError::MissingItem(id));
        };
        record.quantity = quantity;
        Ok(())
    }

    pub(crate) fn remove_by_world_drop(
        &mut self,
        entity: EntityId,
    ) -> Option<(ItemInstanceId, ItemRecord)> {
        let id = self.world_drops.remove(&entity)?;
        let record = self.records.remove(&id)?;
        Some((id, record))
    }

    pub(crate) fn move_world_drop_to_inventory(
        &mut self,
        entity: EntityId,
        owner: EntityId,
        slot: u16,
    ) -> Result<ItemInstanceId, ItemRuntimeError> {
        let id = self
            .world_drops
            .get(&entity)
            .copied()
            .ok_or(ItemRuntimeError::PickupTargetMissing(entity))?;
        let Some(record) = self.records.get_mut(&id) else {
            return Err(ItemRuntimeError::MissingItem(id));
        };
        if record.location != ItemLocation::WorldDrop(entity) {
            return Err(ItemRuntimeError::PickupTargetMissing(entity));
        }
        let slots = self
            .inventories
            .entry(owner)
            .or_insert_with(|| vec![None; INVENTORY_CAPACITY]);
        let Some(destination) = slots.get_mut(usize::from(slot)) else {
            return Err(ItemRuntimeError::InventoryFull(owner));
        };
        if destination.is_some() {
            return Err(ItemRuntimeError::InventoryFull(owner));
        }
        *destination = Some(id);
        self.world_drops.remove(&entity);
        record.location = ItemLocation::Inventory { owner, slot };
        Ok(id)
    }

    pub(crate) fn move_inventory_item(
        &mut self,
        owner: EntityId,
        from: u16,
        to: u16,
    ) -> Result<ItemInstanceId, ItemRuntimeError> {
        let slots = self
            .inventories
            .get_mut(&owner)
            .ok_or(ItemRuntimeError::InventoryFull(owner))?;
        let from_index = usize::from(from);
        let to_index = usize::from(to);
        let Some(Some(id)) = slots.get(from_index).copied() else {
            return Err(ItemRuntimeError::MissingItem(ItemInstanceId::from_raw(0)));
        };
        let Some(destination) = slots.get(to_index) else {
            return Err(ItemRuntimeError::InventoryFull(owner));
        };
        if destination.is_some() {
            return Err(ItemRuntimeError::InventoryFull(owner));
        }
        slots[from_index] = None;
        slots[to_index] = Some(id);
        if let Some(record) = self.records.get_mut(&id) {
            record.location = ItemLocation::Inventory { owner, slot: to };
        }
        Ok(id)
    }

    pub(crate) fn move_inventory_to_equipment(
        &mut self,
        owner: EntityId,
        item: ItemInstanceId,
        slot: crate::equipment::EquipmentSlot,
    ) -> Result<Option<ItemInstanceId>, ItemRuntimeError> {
        let slots = self
            .inventories
            .get(&owner)
            .ok_or(ItemRuntimeError::ItemNotInInventory { owner, item })?;
        let source = slots
            .iter()
            .position(|candidate| *candidate == Some(item))
            .ok_or(ItemRuntimeError::ItemNotInInventory { owner, item })?;
        let replaced = self.equipped_item(owner, slot);
        let replacement_destination = replaced.and_then(|_| {
            slots
                .iter()
                .enumerate()
                .find_map(|(index, candidate)| {
                    (candidate.is_none() && index != source).then_some(index)
                })
                .or_else(|| (slots.len() < INVENTORY_CAPACITY).then_some(slots.len()))
        });
        if replaced.is_some() && replacement_destination.is_none() {
            return Err(ItemRuntimeError::InventoryFull(owner));
        }

        let slots = self
            .inventories
            .get_mut(&owner)
            .expect("inventory presence checked above");
        slots[source] = None;
        if let Some(old_item) = replaced {
            let destination =
                replacement_destination.expect("replacement destination checked above");
            if destination == slots.len() {
                slots.push(Some(old_item));
            } else {
                slots[destination] = Some(old_item);
            }
            let record = self
                .records
                .get_mut(&old_item)
                .expect("equipped item has a canonical record");
            record.location = ItemLocation::Inventory {
                owner,
                slot: u16::try_from(destination).expect("inventory capacity fits u16"),
            };
        }
        let record = self
            .records
            .get_mut(&item)
            .expect("inventory item has a canonical record");
        record.location = ItemLocation::Equipped { owner, slot };
        Ok(replaced)
    }

    pub(crate) fn move_equipment_to_inventory(
        &mut self,
        owner: EntityId,
        slot: crate::equipment::EquipmentSlot,
    ) -> Result<ItemInstanceId, ItemRuntimeError> {
        let item = self
            .equipped_item(owner, slot)
            .ok_or(ItemRuntimeError::EquipmentEmpty { owner, slot })?;
        let destination = self
            .first_inventory_slot(owner)
            .ok_or(ItemRuntimeError::InventoryFull(owner))?;
        let slots = self
            .inventories
            .entry(owner)
            .or_insert_with(|| vec![None; INVENTORY_CAPACITY]);
        if usize::from(destination) == slots.len() {
            slots.push(Some(item));
        } else {
            slots[usize::from(destination)] = Some(item);
        }
        let record = self
            .records
            .get_mut(&item)
            .expect("equipped item has a canonical record");
        record.location = ItemLocation::Inventory {
            owner,
            slot: destination,
        };
        Ok(item)
    }

    pub(crate) fn remove_inventory_item(
        &mut self,
        owner: EntityId,
        slot: u16,
    ) -> Option<(ItemInstanceId, ItemRecord)> {
        let slots = self.inventories.get_mut(&owner)?;
        let id = slots.get_mut(usize::from(slot))?.take()?;
        let record = self.records.remove(&id)?;
        Some((id, record))
    }

    pub(crate) fn remove_inventory_quantity(
        &mut self,
        owner: EntityId,
        definition: ContentId,
        quantity: u32,
    ) -> Result<Vec<ItemInstanceId>, ItemRuntimeError> {
        if quantity == 0 {
            return Err(ItemRuntimeError::InvalidQuantity {
                quantity,
                stack_limit: u32::MAX,
            });
        }
        let matching: Vec<(u16, ItemInstanceId, u32)> = self
            .inventory_snapshot(owner)
            .into_iter()
            .filter_map(|(slot, id, record)| {
                (record.definition == definition).then_some((slot, id, record.quantity))
            })
            .collect();
        let available = matching
            .iter()
            .fold(0u32, |total, (_, _, amount)| total.saturating_add(*amount));
        if available < quantity {
            return Err(ItemRuntimeError::InsufficientInventoryQuantity {
                owner,
                definition,
                requested: quantity,
                available,
            });
        }

        let mut remaining = quantity;
        let mut destroyed = Vec::new();
        for (slot, id, amount) in matching {
            if remaining == 0 {
                break;
            }
            if amount <= remaining {
                remaining -= amount;
                let removed = self
                    .remove_inventory_item(owner, slot)
                    .expect("preflight inventory record remains present");
                debug_assert_eq!(removed.0, id);
                destroyed.push(id);
            } else {
                self.records
                    .get_mut(&id)
                    .expect("preflight inventory record remains present")
                    .quantity = amount - remaining;
                remaining = 0;
            }
        }
        debug_assert_eq!(remaining, 0);
        Ok(destroyed)
    }
}

fn initial_mint_epoch() -> u32 {
    let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos(),
        Err(_) => 1,
    };
    let mixed = nanos ^ (u128::from(std::process::id()) << 32);
    let folded = (mixed as u64) ^ ((mixed >> 64) as u64);
    ((folded & 0xffff_ffff) as u32).max(1)
}
