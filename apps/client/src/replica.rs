//! Client-side replicated world. Distinct from server [`purgatory_simulation::World`].
//!
//! Full snapshots replace the replica atomically after a complete decode.
//! Sequence: first any value is accepted; later only strictly greater sequences
//! apply. Equal is duplicate. Lower is stale. `u32` does not wrap within a
//! session. No rewind.

use std::collections::HashMap;
use std::time::Instant;

use purgatory_protocol::{PlatformSupportId, SnapshotEntity, WireEntityId, WorldSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotDecision {
    Accept,
    Duplicate,
    Stale,
}

#[derive(Clone, Copy, Debug)]
pub struct ReplicatedEntity {
    pub entity_id: WireEntityId,
    #[allow(dead_code)]
    pub kind: purgatory_protocol::ReplicatedKind,
    pub position: [f32; 2],
    #[allow(dead_code)]
    pub velocity: [f32; 2],
}

#[derive(Debug)]
pub struct ReplicatedWorld {
    entities: HashMap<WireEntityId, ReplicatedEntity>,
    local_player: Option<WireEntityId>,
    last_sequence: Option<u32>,
    last_server_tick: u64,
    last_valid_at: Option<Instant>,
    input_epoch: u16,
    last_acknowledged_input_sequence: u32,
    local_grounded: bool,
    local_grounded_on: PlatformSupportId,
    local_ignored_platform: PlatformSupportId,
    continuation_debt: u16,
    pub stale_ignored: u64,
    pub duplicate_ignored: u64,
    pub applied: u64,
}

impl Default for ReplicatedWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplicatedWorld {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            local_player: None,
            last_sequence: None,
            last_server_tick: 0,
            last_valid_at: None,
            input_epoch: 0,
            last_acknowledged_input_sequence: 0,
            local_grounded: false,
            local_grounded_on: PlatformSupportId::NONE,
            local_ignored_platform: PlatformSupportId::NONE,
            continuation_debt: 0,
            stale_ignored: 0,
            duplicate_ignored: 0,
            applied: 0,
        }
    }

    /// `u32` sequences do not wrap within a session. `new <= last` is ignored
    /// (equal is duplicate). The first valid snapshot of any value is accepted.
    #[must_use]
    pub fn classify(last: Option<u32>, next: u32) -> SnapshotDecision {
        match last {
            None => SnapshotDecision::Accept,
            Some(prev) if next == prev => SnapshotDecision::Duplicate,
            Some(prev) if next > prev => SnapshotDecision::Accept,
            Some(_) => SnapshotDecision::Stale,
        }
    }

    /// Decode-complete snapshot only. Replaces the entity map in one assignment
    /// so a failed decode never reaches this function.
    pub fn apply(&mut self, snap: WorldSnapshot) -> SnapshotDecision {
        let decision = Self::classify(self.last_sequence, snap.snapshot_sequence);
        match decision {
            SnapshotDecision::Duplicate => {
                self.duplicate_ignored = self.duplicate_ignored.saturating_add(1);
                return decision;
            }
            SnapshotDecision::Stale => {
                self.stale_ignored = self.stale_ignored.saturating_add(1);
                return decision;
            }
            SnapshotDecision::Accept => {}
        }
        let mut next = HashMap::with_capacity(snap.entities.len());
        for entity in snap.entities {
            next.insert(
                entity.entity_id,
                ReplicatedEntity {
                    entity_id: entity.entity_id,
                    kind: entity.kind,
                    position: entity.position,
                    velocity: entity.velocity,
                },
            );
        }
        self.entities = next;
        self.local_player = Some(snap.local_player_entity);
        self.last_sequence = Some(snap.snapshot_sequence);
        self.last_server_tick = snap.server_tick;
        self.input_epoch = snap.input_epoch;
        self.last_acknowledged_input_sequence = snap.last_acknowledged_input_sequence;
        self.local_grounded = snap.local_grounded;
        self.local_grounded_on = snap.local_grounded_on;
        self.local_ignored_platform = snap.local_ignored_platform;
        self.continuation_debt = snap.continuation_debt;
        self.last_valid_at = Some(Instant::now());
        self.applied = self.applied.saturating_add(1);
        decision
    }

    pub fn clear(&mut self) {
        self.entities.clear();
        self.local_player = None;
        self.last_sequence = None;
        self.last_server_tick = 0;
        self.last_valid_at = None;
        self.input_epoch = 0;
        self.last_acknowledged_input_sequence = 0;
        self.local_grounded = false;
        self.local_grounded_on = PlatformSupportId::NONE;
        self.local_ignored_platform = PlatformSupportId::NONE;
        self.continuation_debt = 0;
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    #[must_use]
    pub fn local_player(&self) -> Option<WireEntityId> {
        self.local_player
    }

    #[must_use]
    pub fn last_sequence(&self) -> Option<u32> {
        self.last_sequence
    }

    #[must_use]
    pub fn last_server_tick(&self) -> u64 {
        self.last_server_tick
    }

    #[must_use]
    pub fn input_epoch(&self) -> u16 {
        self.input_epoch
    }

    #[must_use]
    pub fn last_acknowledged_input_sequence(&self) -> u32 {
        self.last_acknowledged_input_sequence
    }

    #[must_use]
    pub fn local_grounded(&self) -> bool {
        self.local_grounded
    }

    #[must_use]
    pub fn local_grounded_on(&self) -> PlatformSupportId {
        self.local_grounded_on
    }

    #[must_use]
    pub fn local_ignored_platform(&self) -> PlatformSupportId {
        self.local_ignored_platform
    }

    #[must_use]
    pub fn continuation_debt(&self) -> u16 {
        self.continuation_debt
    }

    #[must_use]
    pub fn snapshot_age(&self, now: Instant) -> Option<std::time::Duration> {
        self.last_valid_at
            .map(|at| now.saturating_duration_since(at))
    }

    #[must_use]
    pub fn get(&self, id: WireEntityId) -> Option<ReplicatedEntity> {
        self.entities.get(&id).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = ReplicatedEntity> + '_ {
        self.entities.values().copied()
    }

    #[must_use]
    pub fn local_entity(&self) -> Option<ReplicatedEntity> {
        let id = self.local_player?;
        self.get(id)
    }
}

impl From<SnapshotEntity> for ReplicatedEntity {
    fn from(entity: SnapshotEntity) -> Self {
        Self {
            entity_id: entity.entity_id,
            kind: entity.kind,
            position: entity.position,
            velocity: entity.velocity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_protocol::{ReplicatedKind, decode_world_snapshot, encode_world_snapshot};

    fn entity(index: u32, generation: u32, x: f32) -> SnapshotEntity {
        SnapshotEntity {
            entity_id: WireEntityId { index, generation },
            kind: ReplicatedKind::Player,
            position: [x, 0.0],
            velocity: [0.0, 0.0],
        }
    }

    fn snap(seq: u32, local: WireEntityId, entities: Vec<SnapshotEntity>) -> WorldSnapshot {
        WorldSnapshot::from_poses(seq, u64::from(seq), local, entities)
    }

    #[test]
    fn newer_accepted_stale_and_duplicate_ignored() {
        let mut world = ReplicatedWorld::new();
        let a = WireEntityId {
            index: 1,
            generation: 1,
        };
        assert_eq!(
            world.apply(snap(10, a, vec![entity(1, 1, 1.0)])),
            SnapshotDecision::Accept
        );
        assert_eq!(
            world.apply(snap(12, a, vec![entity(1, 1, 12.0)])),
            SnapshotDecision::Accept
        );
        assert_eq!(world.get(a).unwrap().position[0], 12.0);
        assert_eq!(
            world.apply(snap(11, a, vec![entity(1, 1, 11.0)])),
            SnapshotDecision::Stale
        );
        assert_eq!(world.get(a).unwrap().position[0], 12.0);
        assert_eq!(
            world.apply(snap(12, a, vec![entity(1, 1, 99.0)])),
            SnapshotDecision::Duplicate
        );
        assert_eq!(world.get(a).unwrap().position[0], 12.0);
        assert_eq!(world.stale_ignored, 1);
        assert_eq!(world.duplicate_ignored, 1);
    }

    #[test]
    fn full_snapshot_removes_absent_entities() {
        let mut world = ReplicatedWorld::new();
        let a = WireEntityId {
            index: 1,
            generation: 1,
        };
        let b = WireEntityId {
            index: 2,
            generation: 1,
        };
        world.apply(snap(1, a, vec![entity(1, 1, 0.0), entity(2, 1, 1.0)]));
        assert_eq!(world.len(), 2);
        world.apply(snap(2, a, vec![entity(1, 1, 0.0)]));
        assert!(world.get(b).is_none());
        assert_eq!(world.len(), 1);
    }

    #[test]
    fn generation_reuse_is_a_new_entity() {
        let mut world = ReplicatedWorld::new();
        let g1 = WireEntityId {
            index: 5,
            generation: 1,
        };
        let g2 = WireEntityId {
            index: 5,
            generation: 2,
        };
        world.apply(snap(1, g1, vec![entity(5, 1, 1.0)]));
        world.apply(snap(2, g2, vec![entity(5, 2, 2.0)]));
        assert!(world.get(g1).is_none());
        assert_eq!(world.get(g2).unwrap().position[0], 2.0);
        assert_ne!(g1, g2);
    }

    #[test]
    fn malformed_decode_does_not_mutate_replica() {
        let mut world = ReplicatedWorld::new();
        let a = WireEntityId {
            index: 1,
            generation: 1,
        };
        world.apply(snap(5, a, vec![entity(1, 1, 3.0)]));
        let mut encoded = encode_world_snapshot(&snap(6, a, vec![entity(1, 1, 9.0)])).unwrap();
        encoded.pop();
        assert!(decode_world_snapshot(&encoded).is_err());
        assert_eq!(world.last_sequence(), Some(5));
        assert_eq!(world.get(a).unwrap().position[0], 3.0);
    }

    #[test]
    fn wrap_is_stale() {
        assert_eq!(
            ReplicatedWorld::classify(Some(u32::MAX), 0),
            SnapshotDecision::Stale
        );
        assert_eq!(ReplicatedWorld::classify(None, 0), SnapshotDecision::Accept);
    }

    #[test]
    fn clear_drops_all_state() {
        let mut world = ReplicatedWorld::new();
        let a = WireEntityId {
            index: 1,
            generation: 1,
        };
        world.apply(snap(3, a, vec![entity(1, 1, 0.0)]));
        world.clear();
        assert_eq!(world.len(), 0);
        assert!(world.local_player().is_none());
        assert!(world.last_sequence().is_none());
    }
}
