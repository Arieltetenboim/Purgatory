//! Generic temporary-effect lifetime. Not a buff/stat/combat system.
//!
//! Expiry uses the shared [`crate::Scheduler`]. Stale expiry after remove is a no-op.

use std::fmt;

use crate::entity::EntityId;
use crate::scheduler::TimerId;
use crate::time::SimulationTick;

/// Generational effect identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct EffectId {
    index: u32,
    generation: u32,
}

impl EffectId {
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

impl fmt::Display for EffectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.index, self.generation)
    }
}

/// Synthetic 6F effect plus Phase 7.2 Pulse. No buff/stat framework.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectKind {
    Test {
        token: u32,
    },
    /// Periodic flat damage (`PULSE_DAMAGE`) until expiry.
    Pulse {
        period_ticks: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TempEffect {
    pub id: EffectId,
    pub kind: EffectKind,
    pub owner: Option<EntityId>,
    pub source: Option<EntityId>,
    pub target: EntityId,
    pub expire_at: SimulationTick,
    pub timer: TimerId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectError {
    TargetMissing,
    UnknownEffect,
}

struct Slot {
    generation: u32,
    effect: Option<TempEffect>,
}

pub struct EffectTable {
    slots: Vec<Slot>,
    free: Vec<u32>,
}

impl Default for EffectTable {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    #[must_use]
    pub fn active_count(&self) -> u32 {
        self.slots.iter().filter(|s| s.effect.is_some()).count() as u32
    }

    #[must_use]
    pub fn get(&self, id: EffectId) -> Option<TempEffect> {
        let slot = self.slots.get(id.index as usize)?;
        let effect = slot.effect?;
        (slot.generation == id.generation).then_some(effect)
    }

    pub fn insert(&mut self, draft: TempEffect) -> EffectId {
        let (index, generation) = self.allocate();
        let mut effect = draft;
        let id = EffectId { index, generation };
        effect.id = id;
        self.slots[index as usize].effect = Some(effect);
        id
    }

    /// Remove if live. Stale id is a controlled no-op (`None`).
    pub fn remove(&mut self, id: EffectId) -> Option<TempEffect> {
        let slot = self.slots.get_mut(id.index as usize)?;
        let effect = slot.effect.take()?;
        if slot.generation != id.generation {
            slot.effect = Some(effect);
            return None;
        }
        slot.generation = next_generation(slot.generation);
        self.free.push(id.index);
        Some(effect)
    }

    pub fn set_timer(&mut self, id: EffectId, timer: TimerId) -> bool {
        let Some(slot) = self.slots.get_mut(id.index as usize) else {
            return false;
        };
        if slot.generation != id.generation {
            return false;
        }
        let Some(effect) = slot.effect.as_mut() else {
            return false;
        };
        effect.timer = timer;
        true
    }

    pub fn drop_target(&mut self, target: EntityId) -> Vec<TempEffect> {
        let ids: Vec<EffectId> = self
            .slots
            .iter()
            .filter_map(|slot| {
                let effect = slot.effect?;
                (effect.target == target).then_some(effect.id)
            })
            .collect();
        ids.into_iter().filter_map(|id| self.remove(id)).collect()
    }

    fn allocate(&mut self) -> (u32, u32) {
        if let Some(index) = self.free.pop() {
            (index, self.slots[index as usize].generation)
        } else {
            let index = u32::try_from(self.slots.len()).expect("effect slot index");
            self.slots.push(Slot {
                generation: 1,
                effect: None,
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
    use super::*;
    use crate::{RuntimeSpawnRequest, SimulationTick, Transform, World, WorldAddress};

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
    fn effect_apply_expire_remove() {
        let mut world = World::new();
        let target = spawn_generic(&mut world, 2.0);
        world.begin_tick(SimulationTick::from_count(10));
        let effect = world
            .apply_test_effect(target, 2, EffectKind::Test { token: 9 }, None)
            .unwrap();
        assert!(world.effect(effect.id).is_some());
        tick_world(&mut world, 11);
        assert!(world.effect(effect.id).is_some());
        tick_world(&mut world, 12);
        assert!(world.effect(effect.id).is_none());
    }

    #[test]
    fn effect_explicit_remove_stale_expiry_is_noop() {
        let mut world = World::new();
        let target = spawn_generic(&mut world, 2.0);
        world.begin_tick(SimulationTick::from_count(1));
        let effect = world
            .apply_test_effect(target, 5, EffectKind::Test { token: 1 }, None)
            .unwrap();
        assert!(world.remove_effect(effect.id).is_some());
        assert!(world.remove_effect(effect.id).is_none());
        tick_world(&mut world, 6);
        assert!(world.effect(effect.id).is_none());
    }

    #[test]
    fn effect_target_despawn_cleans_up() {
        let mut world = World::new();
        let target = spawn_generic(&mut world, 3.0);
        world.begin_tick(SimulationTick::from_count(1));
        let effect = world
            .apply_test_effect(target, 30, EffectKind::Test { token: 2 }, None)
            .unwrap();
        world.despawn(target);
        assert!(world.effect(effect.id).is_none());
        tick_world(&mut world, 31);
    }
}
