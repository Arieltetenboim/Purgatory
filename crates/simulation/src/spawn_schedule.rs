//! Scheduled spawn / respawn requests. Always mint a fresh [`EntityId`] at commit.
//!
//! Does not assume the previous runtime id after a despawn.

use crate::entity::EntityId;
use crate::scheduler::{ScheduleOwner, TimerId};
use crate::spawn::RuntimeSpawnRequest;
use crate::time::SimulationTick;

/// Generational handle for a pending spawn request.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SpawnRequestId {
    index: u32,
    generation: u32,
}

impl SpawnRequestId {
    #[must_use]
    pub const fn from_raw(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Debug)]
pub struct ScheduledSpawn {
    pub id: SpawnRequestId,
    pub request: RuntimeSpawnRequest,
    pub owner: ScheduleOwner,
    pub due: SimulationTick,
    pub timer: TimerId,
}

struct Slot {
    generation: u32,
    spawn: Option<ScheduledSpawn>,
}

pub struct SpawnSchedule {
    slots: Vec<Slot>,
    free: Vec<u32>,
}

impl Default for SpawnSchedule {
    fn default() -> Self {
        Self::new()
    }
}

impl SpawnSchedule {
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    #[must_use]
    pub fn queued_count(&self) -> u32 {
        self.slots.iter().filter(|s| s.spawn.is_some()).count() as u32
    }

    #[must_use]
    pub fn get(&self, id: SpawnRequestId) -> Option<&ScheduledSpawn> {
        let slot = self.slots.get(id.index as usize)?;
        let spawn = slot.spawn.as_ref()?;
        (slot.generation == id.generation).then_some(spawn)
    }

    pub fn insert(&mut self, mut spawn: ScheduledSpawn) -> SpawnRequestId {
        let (index, generation) = self.allocate();
        let id = SpawnRequestId { index, generation };
        spawn.id = id;
        self.slots[index as usize].spawn = Some(spawn);
        id
    }

    pub fn take(&mut self, id: SpawnRequestId) -> Option<ScheduledSpawn> {
        let slot = self.slots.get_mut(id.index as usize)?;
        let spawn = slot.spawn.take()?;
        if slot.generation != id.generation {
            slot.spawn = Some(spawn);
            return None;
        }
        slot.generation = next_generation(slot.generation);
        self.free.push(id.index);
        Some(spawn)
    }

    pub fn set_timer(&mut self, id: SpawnRequestId, timer: TimerId) -> bool {
        let Some(slot) = self.slots.get_mut(id.index as usize) else {
            return false;
        };
        if slot.generation != id.generation {
            return false;
        }
        let Some(spawn) = slot.spawn.as_mut() else {
            return false;
        };
        spawn.timer = timer;
        true
    }

    pub fn drop_owner(&mut self, entity: EntityId) -> Vec<ScheduledSpawn> {
        let ids: Vec<SpawnRequestId> = self
            .slots
            .iter()
            .filter_map(|slot| {
                let spawn = slot.spawn.as_ref()?;
                match spawn.owner {
                    ScheduleOwner::Entity(owner) if owner == entity => Some(spawn.id),
                    _ => None,
                }
            })
            .collect();
        ids.into_iter().filter_map(|id| self.take(id)).collect()
    }

    fn allocate(&mut self) -> (u32, u32) {
        if let Some(index) = self.free.pop() {
            (index, self.slots[index as usize].generation)
        } else {
            let index = u32::try_from(self.slots.len()).expect("spawn-schedule slot");
            self.slots.push(Slot {
                generation: 1,
                spawn: None,
            });
            (index, 1)
        }
    }
}

fn next_generation(current: u32) -> u32 {
    let next = current.wrapping_add(1);
    if next == 0 { 1 } else { next }
}
