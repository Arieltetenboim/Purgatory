//! Authoritative action lifecycle. Not combat. Not [`crate::InteractionSession`].
//!
//! Request / validate / reject are command-pipeline outcomes. An [`Action`]
//! slot exists only after a start succeeds. Stored phase is the live or
//! terminal runtime state, not every conceptual planning name.

use std::fmt;

use crate::entity::EntityId;

/// Runtime identity for one started action. Not an action type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ActionId {
    index: u32,
    generation: u32,
}

impl ActionId {
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

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.index, self.generation)
    }
}

/// Synthetic 6F kinds only. Real skills are out of scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionKind {
    Test { token: u32 },
}

/// Live or terminal action phase. Pipeline denials never create a slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionPhase {
    /// Started and currently executing.
    Active,
    Completed,
    Cancelled,
    Interrupted,
    Failed,
}

impl ActionPhase {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Active)
    }
}

/// How a live action ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionEnd {
    Completed,
    Cancelled,
    Interrupted,
    Failed,
}

impl From<ActionEnd> for ActionPhase {
    fn from(end: ActionEnd) -> Self {
        match end {
            ActionEnd::Completed => Self::Completed,
            ActionEnd::Cancelled => Self::Cancelled,
            ActionEnd::Interrupted => Self::Interrupted,
            ActionEnd::Failed => Self::Failed,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Action {
    pub id: ActionId,
    pub owner: EntityId,
    pub kind: ActionKind,
    pub phase: ActionPhase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionError {
    MissingOwner,
    Busy,
    UnknownAction,
    Terminal,
}

struct Slot {
    generation: u32,
    action: Option<Action>,
}

/// Live actions keyed by generational [`ActionId`]. At most one Active per owner.
pub struct ActionTable {
    slots: Vec<Slot>,
    free: Vec<u32>,
    by_owner: Vec<(EntityId, ActionId)>,
}

impl Default for ActionTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            by_owner: Vec::new(),
        }
    }

    #[must_use]
    pub fn active_count(&self) -> u32 {
        self.by_owner.len() as u32
    }

    #[must_use]
    pub fn get(&self, id: ActionId) -> Option<Action> {
        let slot = self.slots.get(id.index as usize)?;
        let action = slot.action?;
        (slot.generation == id.generation).then_some(action)
    }

    #[must_use]
    pub fn active_of(&self, owner: EntityId) -> Option<Action> {
        let id = self.by_owner.iter().find(|(e, _)| *e == owner)?.1;
        self.get(id).filter(|a| a.phase == ActionPhase::Active)
    }

    pub fn start(&mut self, owner: EntityId, kind: ActionKind) -> Result<Action, ActionError> {
        if self.active_of(owner).is_some() {
            return Err(ActionError::Busy);
        }
        let (index, generation) = self.allocate();
        let id = ActionId { index, generation };
        let action = Action {
            id,
            owner,
            kind,
            phase: ActionPhase::Active,
        };
        self.slots[index as usize].action = Some(action);
        self.by_owner.push((owner, id));
        Ok(action)
    }

    pub fn end(&mut self, id: ActionId, end: ActionEnd) -> Result<Action, ActionError> {
        let Some(mut action) = self.get(id) else {
            return Err(ActionError::UnknownAction);
        };
        if action.phase.is_terminal() {
            return Err(ActionError::Terminal);
        }
        action.phase = ActionPhase::from(end);
        self.slots[id.index as usize].action = None;
        let next = next_generation(self.slots[id.index as usize].generation);
        self.slots[id.index as usize].generation = next;
        self.free.push(id.index);
        self.by_owner.retain(|&(_, existing)| existing != id);
        Ok(action)
    }

    pub fn drop_owner(&mut self, owner: EntityId) -> Option<Action> {
        let id = self
            .by_owner
            .iter()
            .find(|(e, _)| *e == owner)
            .map(|(_, id)| *id)?;
        self.end(id, ActionEnd::Cancelled).ok()
    }

    fn allocate(&mut self) -> (u32, u32) {
        if let Some(index) = self.free.pop() {
            let generation = self.slots[index as usize].generation;
            (index, generation)
        } else {
            let index = u32::try_from(self.slots.len()).expect("action slot index");
            self.slots.push(Slot {
                generation: 1,
                action: None,
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

    fn owner() -> EntityId {
        EntityId::from_raw(1, 1)
    }

    #[test]
    fn start_complete_is_terminal() {
        let mut t = ActionTable::new();
        let started = t.start(owner(), ActionKind::Test { token: 1 }).unwrap();
        assert_eq!(started.phase, ActionPhase::Active);
        let ended = t.end(started.id, ActionEnd::Completed).unwrap();
        assert_eq!(ended.phase, ActionPhase::Completed);
        assert!(t.get(started.id).is_none());
        assert_eq!(
            t.end(started.id, ActionEnd::Completed),
            Err(ActionError::UnknownAction)
        );
    }

    #[test]
    fn busy_rejects_second_start() {
        let mut t = ActionTable::new();
        t.start(owner(), ActionKind::Test { token: 1 }).unwrap();
        assert_eq!(
            t.start(owner(), ActionKind::Test { token: 2 }),
            Err(ActionError::Busy)
        );
    }

    #[test]
    fn owner_drop_cancels() {
        let mut t = ActionTable::new();
        let started = t.start(owner(), ActionKind::Test { token: 3 }).unwrap();
        let dropped = t.drop_owner(owner()).unwrap();
        assert_eq!(dropped.id, started.id);
        assert_eq!(dropped.phase, ActionPhase::Cancelled);
        assert!(t.active_of(owner()).is_none());
    }
}
