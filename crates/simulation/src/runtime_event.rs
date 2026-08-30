//! Staged typed runtime events. Not a global subscriber bus. Not a Command.

use std::collections::VecDeque;

use crate::action::{ActionEnd, ActionId};
use crate::action_gate::ActionDenialReason;
use crate::command::CommandDenial;
use crate::effect::EffectId;
use crate::entity::EntityId;
use crate::scheduler::TimerId;

/// Authoritative fact that occurred. Distinct from a client command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeEvent {
    EntitySpawned {
        id: EntityId,
    },
    EntityDespawned {
        id: EntityId,
    },
    ActionStarted {
        id: ActionId,
        owner: EntityId,
    },
    ActionEnded {
        id: ActionId,
        owner: EntityId,
        end: ActionEnd,
    },
    ActionRejected {
        owner: EntityId,
        reason: ActionDenialReason,
    },
    EffectApplied {
        id: EffectId,
        target: EntityId,
    },
    EffectExpired {
        id: EffectId,
        target: EntityId,
    },
    EffectRemoved {
        id: EffectId,
        target: EntityId,
    },
    CommandRejected {
        reason: CommandDenial,
    },
    ScheduledFired {
        timer: TimerId,
        token: u32,
    },
    CadenceFired {
        key: u32,
        token: u32,
    },
}

/// Double-buffer queue. `push` during a tick; `commit` drains once.
///
/// Events pushed while draining a commit go to the next commit (no recursive
/// dispatch storm). Order within a commit is push order.
#[derive(Clone, Debug, Default)]
pub struct EventQueue {
    pending: VecDeque<RuntimeEvent>,
    produced: u64,
    processed: u64,
}

impl EventQueue {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn produced(&self) -> u64 {
        self.produced
    }

    #[must_use]
    pub fn processed(&self) -> u64 {
        self.processed
    }

    pub fn push(&mut self, event: RuntimeEvent) {
        self.pending.push_back(event);
        self.produced = self.produced.saturating_add(1);
    }

    /// Take the current pending batch. Further pushes wait for the next commit.
    pub fn commit(&mut self) -> Vec<RuntimeEvent> {
        let out: Vec<RuntimeEvent> = self.pending.drain(..).collect();
        self.processed = self.processed.saturating_add(out.len() as u64);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_is_fifo_and_not_recursive() {
        let mut q = EventQueue::new();
        let a = EntityId::from_raw(1, 1);
        q.push(RuntimeEvent::EntitySpawned { id: a });
        q.push(RuntimeEvent::EntityDespawned { id: a });
        let first = q.commit();
        assert_eq!(first.len(), 2);
        q.push(RuntimeEvent::CadenceFired { key: 0, token: 1 });
        assert_eq!(q.pending_len(), 1);
        let second = q.commit();
        assert_eq!(second.len(), 1);
        assert!(q.commit().is_empty());
    }
}
