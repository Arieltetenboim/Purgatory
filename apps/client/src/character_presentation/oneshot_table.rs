//! Client-side table of authoritative presentation oneshots (A5).
//!
//! Populated from reliable `ServerPresentationOneShot` control events. Cleared
//! by `until_tick` using the replica server tick — never by clip completion.

use std::collections::HashMap;

use purgatory_protocol::{ServerPresentationOneShot, WireEntityId};

use super::collection::PresentationEntityKey;
use super::state::PresentationActivity;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveOneShot {
    activity: PresentationActivity,
    until_tick: u64,
}

/// Per-entity authoritative Attack/Hurt overlay. Independent of AnimationPlayer.
#[derive(Clone, Debug, Default)]
pub struct PresentationOneShotTable {
    by_entity: HashMap<PresentationEntityKey, ActiveOneShot>,
}

impl PresentationOneShotTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.by_entity.clear();
    }

    pub fn apply_server_event(&mut self, event: ServerPresentationOneShot) {
        let key = key_from_wire(event.entity);
        if event.kind == 0 {
            self.by_entity.remove(&key);
            return;
        }
        let Some(activity) = activity_from_kind(event.kind) else {
            return;
        };
        self.by_entity.insert(
            key,
            ActiveOneShot {
                activity,
                until_tick: u64::from(event.until_tick),
            },
        );
    }

    /// Drop expired entries using the latest known server tick.
    pub fn expire(&mut self, server_tick: u64) {
        self.by_entity
            .retain(|_, active| server_tick < active.until_tick);
    }

    #[must_use]
    pub fn activity_of(
        &self,
        key: PresentationEntityKey,
        server_tick: u64,
    ) -> Option<PresentationActivity> {
        self.by_entity.get(&key).and_then(|active| {
            if server_tick < active.until_tick {
                Some(active.activity)
            } else {
                None
            }
        })
    }
}

#[must_use]
fn key_from_wire(id: WireEntityId) -> PresentationEntityKey {
    PresentationEntityKey::new(id.index, id.generation)
}

#[must_use]
fn activity_from_kind(kind: u8) -> Option<PresentationActivity> {
    match kind {
        1 => Some(PresentationActivity::Attack),
        2 => Some(PresentationActivity::Hurt),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_and_expire() {
        let mut table = PresentationOneShotTable::new();
        table.apply_server_event(ServerPresentationOneShot {
            entity: WireEntityId {
                index: 1,
                generation: 1,
            },
            kind: 1,
            until_tick: 20,
        });
        let key = PresentationEntityKey::new(1, 1);
        assert_eq!(
            table.activity_of(key, 19),
            Some(PresentationActivity::Attack)
        );
        table.expire(20);
        assert_eq!(table.activity_of(key, 20), None);
    }

    #[test]
    fn hurt_replaces_attack() {
        let mut table = PresentationOneShotTable::new();
        let entity = WireEntityId {
            index: 2,
            generation: 1,
        };
        table.apply_server_event(ServerPresentationOneShot {
            entity,
            kind: 1,
            until_tick: 50,
        });
        table.apply_server_event(ServerPresentationOneShot {
            entity,
            kind: 2,
            until_tick: 40,
        });
        assert_eq!(
            table.activity_of(PresentationEntityKey::new(2, 1), 30),
            Some(PresentationActivity::Hurt)
        );
    }

    #[test]
    fn duplicate_server_event_does_not_duplicate_state() {
        let mut table = PresentationOneShotTable::new();
        let entity = WireEntityId {
            index: 3,
            generation: 1,
        };
        let event = ServerPresentationOneShot {
            entity,
            kind: 1,
            until_tick: 40,
        };
        table.apply_server_event(event);
        table.apply_server_event(event);
        assert_eq!(table.by_entity.len(), 1);
        assert_eq!(
            table.activity_of(PresentationEntityKey::new(3, 1), 20),
            Some(PresentationActivity::Attack)
        );
    }

    #[test]
    fn clear_kind_removes_oneshot() {
        let mut table = PresentationOneShotTable::new();
        let entity = WireEntityId {
            index: 4,
            generation: 1,
        };
        table.apply_server_event(ServerPresentationOneShot {
            entity,
            kind: 2,
            until_tick: 50,
        });
        table.apply_server_event(ServerPresentationOneShot {
            entity,
            kind: 0,
            until_tick: 0,
        });
        assert_eq!(
            table.activity_of(PresentationEntityKey::new(4, 1), 10),
            None
        );
    }
}
