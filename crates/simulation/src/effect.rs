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

/// Synthetic 6F effect. No gameplay modifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectKind {
    Test { token: u32 },
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
