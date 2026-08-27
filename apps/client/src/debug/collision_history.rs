//! Bounded collision / discontinuity event history for the debug overlay.

use purgatory_simulation::{CorrectionAxis, EntityId, ResponseKind};

pub const COLLISION_HISTORY_CAP: usize = 16;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DiscSubject {
    #[default]
    Player,
    Camera,
}

#[derive(Clone, Copy, Debug)]
pub struct CollisionHistoryEvent {
    pub tick: u64,
    pub subject: DiscSubject,
    pub axis: CorrectionAxis,
    pub delta: [f32; 2],
    pub correction: [f32; 2],
    pub previous_position: [f32; 2],
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub candidate: Option<EntityId>,
    pub grounded_on: Option<EntityId>,
    pub response_kind: ResponseKind,
    #[allow(dead_code)] // stored for inspector / future filters
    pub discontinuity: bool,
}

/// Fixed-capacity ring buffer of recent diagnostic events.
#[derive(Clone, Debug, Default)]
pub struct CollisionHistory {
    buf: [Option<CollisionHistoryEvent>; COLLISION_HISTORY_CAP],
    next: usize,
    len: usize,
}

impl CollisionHistory {
    pub fn clear(&mut self) {
        self.buf = [None; COLLISION_HISTORY_CAP];
        self.next = 0;
        self.len = 0;
    }

    pub fn push(&mut self, event: CollisionHistoryEvent) {
        self.buf[self.next] = Some(event);
        self.next = (self.next + 1) % COLLISION_HISTORY_CAP;
        self.len = (self.len + 1).min(COLLISION_HISTORY_CAP);
    }

    /// Newest-first iterator.
    pub fn iter_newest_first(&self) -> impl Iterator<Item = CollisionHistoryEvent> + '_ {
        let len = self.len;
        let next = self.next;
        (0..len).filter_map(move |i| {
            let idx = (next + COLLISION_HISTORY_CAP - 1 - i) % COLLISION_HISTORY_CAP;
            self.buf[idx]
        })
    }

    #[must_use]
    pub fn latest(&self) -> Option<CollisionHistoryEvent> {
        self.iter_newest_first().next()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }
}
