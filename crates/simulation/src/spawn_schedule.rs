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

#[cfg(test)]
mod tests {
    use crate::{
        RuntimeSpawnRequest, ScheduleOwner, SimulationTick, Transform, WorkLane, World,
        WorldAddress,
    };

    fn spawn_generic(world: &mut World, x: f32) -> crate::EntityId {
        world
            .spawn(
                RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                    .with_transform(Transform::from_position([x, 1.0]))
                    .visible(),
            )
            .expect("spawn")
    }

    fn tick_world(world: &mut World, tick: u64) {
        world.begin_tick(SimulationTick::from_count(tick));
        world.drain_critical_scheduler();
        let _ = world.commit_runtime_events();
        world.pump_cadence();
        world.drain_deferred_scheduler();
    }

    #[test]
    fn scheduled_spawn_mints_fresh_entity_id() {
        let mut world = World::new();
        world.begin_tick(SimulationTick::from_count(1));
        let first = spawn_generic(&mut world, 0.0);
        world.despawn(first);
        let req = RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
            .with_transform(Transform::from_position([4.0, 1.0]))
            .visible();
        world
            .schedule_spawn(
                req,
                SimulationTick::from_count(3),
                ScheduleOwner::World,
                WorkLane::Deferred,
            )
            .unwrap();
        tick_world(&mut world, 2);
        assert_eq!(world.len(), 0);
        tick_world(&mut world, 3);
        assert_eq!(world.len(), 1);
        let spawned = world
            .iter_kind(crate::entity::EntityKind::Generic)
            .next()
            .unwrap();
        assert_ne!(spawned, first, "respawn must not assume the old EntityId");
        assert!(world.contains(spawned));
        assert!(!world.contains(first));
    }

    #[test]
    fn scheduled_spawn_cancel() {
        let mut world = World::new();
        world.begin_tick(SimulationTick::from_count(1));
        let id = world
            .schedule_spawn(
                RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                    .with_transform(Transform::from_position([1.0, 1.0]))
                    .visible(),
                SimulationTick::from_count(4),
                ScheduleOwner::World,
                WorkLane::Deferred,
            )
            .unwrap();
        assert!(world.cancel_scheduled_spawn(id));
        tick_world(&mut world, 4);
        assert_eq!(world.len(), 0);
    }
}
