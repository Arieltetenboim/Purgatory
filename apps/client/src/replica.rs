//! Client-side replicated world. Distinct from server [`purgatory_simulation::World`].
//!
//! Protocol v8/v9 applies ordered `ReplicationFrame` records (Enter/Update/Leave).
//! Historical [`WorldSnapshot`] `apply` remains for unit tests that still build
//! full snapshots. `u32` sequences do not wrap within a session. No rewind.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use purgatory_protocol::{
    ObserverAoiDebug, PlatformSupportId, ReplicatedHealth, ReplicationFrame, ReplicationRecord,
    SnapshotEntity, WireEntityId, WorldSnapshot,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum SnapshotDecision {
    Accept,
    Duplicate,
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameDecision {
    Applied { epoch_reset: bool },
    IgnoredOlderEpoch,
}

#[derive(Clone, Copy, Debug)]
pub struct ReplicatedEntity {
    pub entity_id: WireEntityId,
    #[allow(dead_code)]
    pub kind: purgatory_protocol::ReplicatedKind,
    pub position: [f32; 2],
    #[allow(dead_code)]
    pub velocity: [f32; 2],
    pub health: Option<ReplicatedHealth>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplicaLifecycleEvent {
    Entered,
    Left,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplicaLifecycleNote {
    pub entity_id: WireEntityId,
    pub kind: purgatory_protocol::ReplicatedKind,
    pub event: ReplicaLifecycleEvent,
    pub tick: u64,
}

const RECENT_LIFECYCLE_CAP: usize = 24;

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
    local_map: u32,
    local_channel: u32,
    local_instance: u32,
    observer_epoch: u32,
    aoi_debug: Option<ObserverAoiDebug>,
    recent_lifecycle: VecDeque<ReplicaLifecycleNote>,
    pub stale_ignored: u64,
    pub duplicate_ignored: u64,
    pub applied: u64,
    last_frame_enters: u32,
    last_frame_updates: u32,
    last_frame_leaves: u32,
    total_enters: u64,
    total_updates: u64,
    total_leaves: u64,
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
            local_map: 0,
            local_channel: 0,
            local_instance: 0,
            observer_epoch: 0,
            aoi_debug: None,
            recent_lifecycle: VecDeque::new(),
            stale_ignored: 0,
            duplicate_ignored: 0,
            applied: 0,
            last_frame_enters: 0,
            last_frame_updates: 0,
            last_frame_leaves: 0,
            total_enters: 0,
            total_updates: 0,
            total_leaves: 0,
        }
    }

    /// `u32` sequences do not wrap within a session. `new <= last` is ignored
    /// (equal is duplicate). The first valid snapshot of any value is accepted.
    #[must_use]
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
                    health: None,
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
        self.local_map = snap.local_map;
        self.local_channel = snap.local_channel;
        self.local_instance = snap.local_instance;
        self.last_valid_at = Some(Instant::now());
        self.applied = self.applied.saturating_add(1);
        decision
    }

    /// Ordered v8 frame. Older epochs are ignored. A newer epoch clears known
    /// entities, then applies this frame as the new baseline start.
    pub fn apply_frame(&mut self, frame: ReplicationFrame) -> FrameDecision {
        if frame.observer_baseline_epoch < self.observer_epoch {
            self.stale_ignored = self.stale_ignored.saturating_add(1);
            return FrameDecision::IgnoredOlderEpoch;
        }
        let epoch_reset = frame.observer_baseline_epoch > self.observer_epoch;
        if epoch_reset {
            self.entities.clear();
            self.last_sequence = None;
            self.observer_epoch = frame.observer_baseline_epoch;
            self.recent_lifecycle.clear();
        }
        self.apply_frame_header(&frame);
        self.aoi_debug = frame.aoi_debug;
        let mut enters = 0u32;
        let mut updates = 0u32;
        let mut leaves = 0u32;
        for rec in frame.records {
            match rec {
                ReplicationRecord::Enter { entity, health } => {
                    enters = enters.saturating_add(1);
                    self.push_lifecycle(ReplicaLifecycleNote {
                        entity_id: entity.entity_id,
                        kind: entity.kind,
                        event: ReplicaLifecycleEvent::Entered,
                        tick: frame.server_tick,
                    });
                    self.entities.insert(
                        entity.entity_id,
                        ReplicatedEntity {
                            entity_id: entity.entity_id,
                            kind: entity.kind,
                            position: entity.position,
                            velocity: entity.velocity,
                            health,
                        },
                    );
                }
                ReplicationRecord::Update {
                    entity_id,
                    position,
                    velocity,
                    health,
                    ..
                } => {
                    updates = updates.saturating_add(1);
                    if let Some(existing) = self.entities.get_mut(&entity_id) {
                        if let Some(p) = position {
                            existing.position = p;
                        }
                        if let Some(v) = velocity {
                            existing.velocity = v;
                        }
                        if health.is_some() {
                            existing.health = health;
                        }
                    }
                }
                ReplicationRecord::Leave { entity_id } => {
                    leaves = leaves.saturating_add(1);
                    let kind = self
                        .entities
                        .get(&entity_id)
                        .map(|e| e.kind)
                        .unwrap_or(purgatory_protocol::ReplicatedKind::Player);
                    self.push_lifecycle(ReplicaLifecycleNote {
                        entity_id,
                        kind,
                        event: ReplicaLifecycleEvent::Left,
                        tick: frame.server_tick,
                    });
                    self.entities.remove(&entity_id);
                }
            }
        }
        self.last_frame_enters = enters;
        self.last_frame_updates = updates;
        self.last_frame_leaves = leaves;
        self.total_enters = self.total_enters.saturating_add(u64::from(enters));
        self.total_updates = self.total_updates.saturating_add(u64::from(updates));
        self.total_leaves = self.total_leaves.saturating_add(u64::from(leaves));
        self.last_valid_at = Some(Instant::now());
        self.applied = self.applied.saturating_add(1);
        FrameDecision::Applied { epoch_reset }
    }

    fn apply_frame_header(&mut self, frame: &ReplicationFrame) {
        self.local_player = Some(frame.local_player_entity);
        self.last_sequence = Some(frame.snapshot_sequence);
        self.last_server_tick = frame.server_tick;
        self.input_epoch = frame.input_epoch;
        self.last_acknowledged_input_sequence = frame.last_acknowledged_input_sequence;
        self.local_grounded = frame.local_grounded;
        self.local_grounded_on = frame.local_grounded_on;
        self.local_ignored_platform = frame.local_ignored_platform;
        self.continuation_debt = frame.continuation_debt;
        self.local_map = frame.local_map;
        self.local_channel = frame.local_channel;
        self.local_instance = frame.local_instance;
        self.observer_epoch = frame.observer_baseline_epoch;
    }

    /// Presentation view for interpolation / prediction. Not a wire snapshot.
    #[must_use]
    pub fn to_snapshot_view(&self) -> WorldSnapshot {
        let mut entities: Vec<SnapshotEntity> = self
            .entities
            .values()
            .map(|e| SnapshotEntity {
                entity_id: e.entity_id,
                kind: e.kind,
                position: e.position,
                velocity: e.velocity,
            })
            .collect();
        entities.sort_by_key(|e| (e.entity_id.index, e.entity_id.generation));
        WorldSnapshot {
            snapshot_sequence: self.last_sequence.unwrap_or(0),
            server_tick: self.last_server_tick,
            local_player_entity: self.local_player.unwrap_or(WireEntityId {
                index: 0,
                generation: 0,
            }),
            input_epoch: self.input_epoch,
            last_acknowledged_input_sequence: self.last_acknowledged_input_sequence,
            local_grounded: self.local_grounded,
            local_grounded_on: self.local_grounded_on,
            local_ignored_platform: self.local_ignored_platform,
            continuation_debt: self.continuation_debt,
            local_map: self.local_map,
            local_channel: self.local_channel,
            local_instance: self.local_instance,
            entities,
        }
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
        self.local_map = 0;
        self.local_channel = 0;
        self.local_instance = 0;
        self.observer_epoch = 0;
        self.aoi_debug = None;
        self.recent_lifecycle.clear();
        self.last_frame_enters = 0;
        self.last_frame_updates = 0;
        self.last_frame_leaves = 0;
        self.total_enters = 0;
        self.total_updates = 0;
        self.total_leaves = 0;
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
    pub fn observer_address(&self) -> (u32, u32, u32) {
        (self.local_map, self.local_channel, self.local_instance)
    }

    #[must_use]
    pub fn observer_epoch(&self) -> u32 {
        self.observer_epoch
    }

    #[must_use]
    pub fn last_frame_enters(&self) -> u32 {
        self.last_frame_enters
    }

    #[must_use]
    pub fn last_frame_updates(&self) -> u32 {
        self.last_frame_updates
    }

    #[must_use]
    pub fn last_frame_leaves(&self) -> u32 {
        self.last_frame_leaves
    }

    #[must_use]
    pub fn total_enters(&self) -> u64 {
        self.total_enters
    }

    #[must_use]
    pub fn total_updates(&self) -> u64 {
        self.total_updates
    }

    #[must_use]
    pub fn total_leaves(&self) -> u64 {
        self.total_leaves
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
    pub fn aoi_debug(&self) -> Option<ObserverAoiDebug> {
        self.aoi_debug
    }

    pub fn recent_lifecycle(&self) -> impl Iterator<Item = ReplicaLifecycleNote> + '_ {
        self.recent_lifecycle.iter().copied()
    }

    fn push_lifecycle(&mut self, note: ReplicaLifecycleNote) {
        self.recent_lifecycle.push_back(note);
        while self.recent_lifecycle.len() > RECENT_LIFECYCLE_CAP {
            self.recent_lifecycle.pop_front();
        }
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
            health: None,
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

    fn interactable_entity(index: u32, generation: u32, x: f32, y: f32) -> SnapshotEntity {
        SnapshotEntity {
            entity_id: WireEntityId { index, generation },
            kind: ReplicatedKind::Interactable,
            position: [x, y],
            velocity: [0.0, 0.0],
        }
    }

    #[test]
    fn apply_stores_interactable_kind() {
        let mut world = ReplicatedWorld::new();
        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let target = WireEntityId {
            index: 4,
            generation: 1,
        };
        world.apply(snap(
            1,
            local,
            vec![entity(1, 1, 0.0), interactable_entity(4, 1, 1.2, -3.05)],
        ));
        assert_eq!(world.len(), 2);
        let stored = world.get(target).expect("interactable replica");
        assert_eq!(stored.kind, ReplicatedKind::Interactable);
        assert_eq!(stored.position, [1.2, -3.05]);
    }

    #[test]
    fn empty_replica_has_no_snapshot_and_map_id_zero() {
        let world = ReplicatedWorld::new();
        assert!(world.last_sequence().is_none());
        assert_eq!(world.observer_address(), (0, 0, 0));
    }

    fn frame(
        epoch: u32,
        seq: u32,
        local: WireEntityId,
        records: Vec<ReplicationRecord>,
    ) -> ReplicationFrame {
        ReplicationFrame {
            snapshot_sequence: seq,
            server_tick: u64::from(seq),
            local_player_entity: local,
            input_epoch: 0,
            last_acknowledged_input_sequence: 0,
            local_grounded: false,
            local_grounded_on: PlatformSupportId::NONE,
            local_ignored_platform: PlatformSupportId::NONE,
            continuation_debt: 0,
            local_map: 1,
            local_channel: 0,
            local_instance: 0,
            observer_baseline_epoch: epoch,
            records,
            aoi_debug: None,
        }
    }

    #[test]
    fn two_frames_before_poll_both_apply() {
        let mut world = ReplicatedWorld::new();
        let a = WireEntityId {
            index: 1,
            generation: 1,
        };
        let remote = WireEntityId {
            index: 2,
            generation: 1,
        };
        let enter = frame(
            0,
            1,
            a,
            vec![
                ReplicationRecord::Enter {
                    entity: entity(1, 1, 1.0),
                    health: None,
                },
                ReplicationRecord::Enter {
                    entity: entity(2, 1, 2.0),
                    health: None,
                },
            ],
        );
        let update = frame(
            0,
            2,
            a,
            vec![ReplicationRecord::Update {
                entity_id: remote,
                domains: purgatory_protocol::DomainMask {
                    transform: true,
                    health: false,
                },
                position: Some([9.0, 0.0]),
                velocity: Some([0.0, 0.0]),
                health: None,
            }],
        );
        assert!(matches!(
            world.apply_frame(enter),
            FrameDecision::Applied { epoch_reset: false }
        ));
        assert!(matches!(
            world.apply_frame(update),
            FrameDecision::Applied { epoch_reset: false }
        ));
        assert_eq!(world.len(), 2);
        assert_eq!(world.get(remote).unwrap().position[0], 9.0);
    }

    #[test]
    fn apply_frame_stores_aoi_debug_and_recent_leave() {
        let mut world = ReplicatedWorld::new();
        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let remote = WireEntityId {
            index: 2,
            generation: 1,
        };
        let mut enter = frame(
            0,
            10,
            local,
            vec![
                ReplicationRecord::Enter {
                    entity: entity(1, 1, 0.0),
                    health: None,
                },
                ReplicationRecord::Enter {
                    entity: entity(2, 1, 2.0),
                    health: None,
                },
            ],
        );
        enter.aoi_debug = Some(ObserverAoiDebug {
            candidates: 5,
            known: 2,
            want_enter: 1,
            want_leave: 0,
        });
        world.apply_frame(enter);
        assert_eq!(world.last_frame_enters(), 2);
        assert_eq!(world.total_enters(), 2);
        assert_eq!(world.total_leaves(), 0);
        let debug = world.aoi_debug().expect("aoi trailer");
        assert_eq!(debug.candidates, 5);
        assert_eq!(debug.known, 2);
        assert_eq!(debug.want_enter, 1);
        assert_eq!(debug.want_leave, 0);
        assert!(
            world
                .recent_lifecycle()
                .any(|n| n.entity_id == remote && n.event == ReplicaLifecycleEvent::Entered)
        );

        world.apply_frame(frame(
            0,
            11,
            local,
            vec![ReplicationRecord::Leave { entity_id: remote }],
        ));
        assert!(world.get(remote).is_none());
        let left = world
            .recent_lifecycle()
            .find(|n| n.event == ReplicaLifecycleEvent::Left)
            .expect("leave note");
        assert_eq!(left.entity_id, remote);
        assert_eq!(left.kind, ReplicatedKind::Player);
        assert_eq!(world.last_frame_leaves(), 1);
        assert_eq!(world.total_enters(), 2);
        assert_eq!(world.total_leaves(), 1);
    }

    #[test]
    fn update_of_unknown_entity_is_ignored_not_deleted() {
        let mut world = ReplicatedWorld::new();
        let a = WireEntityId {
            index: 1,
            generation: 1,
        };
        world.apply_frame(frame(
            0,
            1,
            a,
            vec![ReplicationRecord::Enter {
                entity: entity(1, 1, 1.0),
                health: None,
            }],
        ));
        world.apply_frame(frame(
            0,
            2,
            a,
            vec![ReplicationRecord::Update {
                entity_id: WireEntityId {
                    index: 99,
                    generation: 1,
                },
                domains: purgatory_protocol::DomainMask {
                    transform: true,
                    health: false,
                },
                position: Some([5.0, 0.0]),
                velocity: Some([0.0, 0.0]),
                health: None,
            }],
        ));
        assert_eq!(world.len(), 1);
        assert_eq!(world.get(a).unwrap().position[0], 1.0);
    }

    #[test]
    fn newer_epoch_resets_then_applies_baseline() {
        let mut world = ReplicatedWorld::new();
        let a = WireEntityId {
            index: 1,
            generation: 1,
        };
        let ghost = WireEntityId {
            index: 8,
            generation: 1,
        };
        world.apply_frame(frame(
            0,
            1,
            a,
            vec![
                ReplicationRecord::Enter {
                    entity: entity(1, 1, 1.0),
                    health: None,
                },
                ReplicationRecord::Enter {
                    entity: entity(8, 1, 80.0),
                    health: None,
                },
            ],
        ));
        assert_eq!(world.len(), 2);
        let decision = world.apply_frame(frame(
            1,
            2,
            a,
            vec![ReplicationRecord::Enter {
                entity: entity(1, 1, 3.0),
                health: None,
            }],
        ));
        assert_eq!(decision, FrameDecision::Applied { epoch_reset: true });
        assert_eq!(world.observer_epoch(), 1);
        assert_eq!(world.len(), 1);
        assert!(world.get(ghost).is_none());
        assert_eq!(world.get(a).unwrap().position[0], 3.0);
        assert_eq!(
            world.apply_frame(frame(0, 99, a, vec![])),
            FrameDecision::IgnoredOlderEpoch
        );
        assert_eq!(world.len(), 1);
    }
}
