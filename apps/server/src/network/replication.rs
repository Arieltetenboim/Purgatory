//! Per-observer replication interest, coalescing mailbox, and budgeted v8 frames.
//!
//! Simulation never stores ConnectionId. This module lives on GameplayOwner.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use purgatory_protocol::{
    DomainMask, MAX_GAMEPLAY_SNAPSHOT_BYTES, ObserverAoiDebug, PlatformSupportId, ReplicatedHealth,
    ReplicatedKind, ReplicationFrame, ReplicationRecord, SnapshotEntity, encode_gameplay_frame,
    encode_replication_frame, encode_replication_record,
};
use purgatory_simulation::{
    DomainRevs, EntityId, UpdateFrequencyTier, World, aoi_policy_rects, point_in_aabb,
    staggered_interval_due,
};
use tokio::sync::watch;

use super::snapshot::to_wire_id;

/// Encoded-frame queue cap. If full, sim does not encode-and-drop.
pub const WRITER_QUEUE_CAP: usize = 4;

/// Soft per-frame budget. Must not exceed [`MAX_GAMEPLAY_SNAPSHOT_BYTES`].
pub const REPLICATION_FRAME_BUDGET_BYTES: usize = 4096;

const HIGH_DISTANCE: f32 = 10.0;
const NORMAL_INTERVAL: u64 = 2;
const LOW_INTERVAL: u64 = 4;
const CHURN_WINDOW_TICKS: u64 = 30;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CommittedRevs {
    pub transform: u64,
    pub health: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Life {
    WantEnter {
        since_tick: u64,
    },
    Known {
        revs: CommittedRevs,
        since_tick: u64,
    },
    WantLeave {
        revs: CommittedRevs,
        since_tick: u64,
    },
}

/// Observer-owned hysteresis and committed revisions. Reset on epoch bump.
#[derive(Debug, Default)]
pub struct ObserverReplicationState {
    pub epoch: u32,
    entities: HashMap<EntityId, Life>,
    enter_cursor: usize,
    last_leave_tick: HashMap<EntityId, u64>,
}

#[derive(Clone, Debug)]
pub struct QueuedFrame {
    pub epoch: u32,
    #[allow(dead_code)]
    pub sequence: u32,
    pub payload: Vec<u8>,
}

struct ReplicationSlot {
    queue: VecDeque<QueuedFrame>,
}

/// Shared epoch-tagged writer queue. Sim pushes; connection task pops.
#[derive(Clone)]
pub struct ReplicationPipe {
    slot: Arc<Mutex<ReplicationSlot>>,
    wake: watch::Sender<u64>,
}

impl ReplicationPipe {
    #[must_use]
    pub fn new() -> (Self, watch::Receiver<u64>) {
        let (wake, rx) = watch::channel(0);
        (
            Self {
                slot: Arc::new(Mutex::new(ReplicationSlot {
                    queue: VecDeque::new(),
                })),
                wake,
            },
            rx,
        )
    }

    #[must_use]
    pub fn try_push(&self, frame: QueuedFrame) -> bool {
        let Ok(mut g) = self.slot.lock() else {
            return false;
        };
        if g.queue.len() >= WRITER_QUEUE_CAP {
            return false;
        }
        g.queue.push_back(frame);
        let next = self.wake.borrow().wrapping_add(1);
        let _ = self.wake.send(next);
        true
    }

    pub fn purge_older_than(&self, epoch: u32) -> usize {
        let Ok(mut g) = self.slot.lock() else {
            return 0;
        };
        let before = g.queue.len();
        g.queue.retain(|f| f.epoch >= epoch);
        before.saturating_sub(g.queue.len())
    }

    #[must_use]
    pub fn pop(&self) -> Option<QueuedFrame> {
        self.slot.lock().ok()?.queue.pop_front()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.slot.lock().map(|g| g.queue.len()).unwrap_or(0)
    }

    #[must_use]
    #[cfg(test)]
    pub fn queued_epochs(&self) -> Vec<u32> {
        self.slot
            .lock()
            .map(|g| g.queue.iter().map(|f| f.epoch).collect())
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ReplicationTickStats {
    pub enters: u32,
    pub leaves: u32,
    pub updates: u32,
    pub skipped_unchanged: u32,
    #[allow(dead_code)]
    pub candidates: u32,
    pub known: u32,
    pub pending: u32,
    /// Known entities whose World DomainRevs lag this observer's commit.
    pub pending_updates: u32,
    /// Pending updates held until a later staggered cadence slot.
    pub cadence_deferred: u32,
    pub oldest_pending_ticks: u64,
    pub bytes: u32,
    pub churn_reentry: u32,
    pub queue_depth: u32,
    pub mailbox_merge: u32,
}

impl ObserverReplicationState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Map/address transition: void prior commits, including queue-committed revs.
    pub fn bump_epoch(&mut self) {
        self.epoch = self.epoch.saturating_add(1);
        self.entities.clear();
        self.enter_cursor = 0;
        self.last_leave_tick.clear();
    }

    #[must_use]
    pub fn want_enter_count(&self) -> u32 {
        self.entities
            .values()
            .filter(|l| matches!(l, Life::WantEnter { .. }))
            .count() as u32
    }

    #[must_use]
    pub fn want_leave_count(&self) -> u32 {
        self.entities
            .values()
            .filter(|l| matches!(l, Life::WantLeave { .. }))
            .count() as u32
    }

    #[must_use]
    pub fn known_count(&self) -> usize {
        self.entities
            .values()
            .filter(|l| matches!(l, Life::Known { .. }))
            .count()
    }

    #[must_use]
    #[cfg(test)]
    pub fn committed_revs(&self, id: EntityId) -> Option<CommittedRevs> {
        match self.entities.get(&id)? {
            Life::Known { revs, .. } | Life::WantLeave { revs, .. } => Some(*revs),
            Life::WantEnter { .. } => None,
        }
    }

    #[must_use]
    #[cfg(test)]
    pub fn is_known(&self, id: EntityId) -> bool {
        matches!(self.entities.get(&id), Some(Life::Known { .. }))
    }

    #[must_use]
    pub fn oldest_pending_age(&self, now: u64) -> u64 {
        self.entities
            .values()
            .filter_map(|life| match life {
                Life::WantEnter { since_tick } | Life::WantLeave { since_tick, .. } => {
                    Some(now.saturating_sub(*since_tick))
                }
                Life::Known { .. } => None,
            })
            .max()
            .unwrap_or(0)
    }

    pub fn classify(&mut self, world: &World, observer: EntityId, now_tick: u64) {
        let Some(obs_addr) = world.address_of(observer) else {
            self.entities.clear();
            return;
        };
        let Some(obs_pos) = world.transform_of(observer).map(|t| t.position) else {
            self.entities.clear();
            return;
        };
        let rects = aoi_policy_rects(obs_pos, world.bounds_for(obs_addr));
        let candidates: HashSet<EntityId> =
            world.spatial_candidates(observer).into_iter().collect();

        let tracked: Vec<EntityId> = self.entities.keys().copied().collect();
        for id in tracked {
            if id == observer {
                continue;
            }
            let in_leave = world
                .transform_of(id)
                .is_some_and(|t| point_in_aabb(t.position, rects.leave))
                && world.address_of(id) == Some(obs_addr)
                && candidates.contains(&id);
            let in_enter = world
                .transform_of(id)
                .is_some_and(|t| point_in_aabb(t.position, rects.enter))
                && in_leave;
            match self.entities.get(&id).copied() {
                Some(Life::WantEnter { .. }) if !in_enter => {
                    self.entities.remove(&id);
                }
                Some(Life::Known { revs, .. }) if !in_leave => {
                    self.entities.insert(
                        id,
                        Life::WantLeave {
                            revs,
                            since_tick: now_tick,
                        },
                    );
                }
                Some(Life::WantLeave { revs, since_tick }) if in_enter => {
                    // Leave must go first; stay WantLeave until committed.
                    let _ = (revs, since_tick);
                }
                _ => {}
            }
        }

        for id in &candidates {
            if *id == observer {
                continue;
            }
            if self.entities.contains_key(id) {
                continue;
            }
            let Some(pos) = world.transform_of(*id).map(|t| t.position) else {
                continue;
            };
            if point_in_aabb(pos, rects.enter) {
                self.entities.insert(
                    *id,
                    Life::WantEnter {
                        since_tick: now_tick,
                    },
                );
            }
        }

        if !self.entities.contains_key(&observer) {
            self.entities.insert(
                observer,
                Life::WantEnter {
                    since_tick: now_tick,
                },
            );
        } else if matches!(self.entities.get(&observer), Some(Life::WantLeave { .. })) {
            // Local player is never left for the controlling observer.
            if let Some(Life::WantLeave { revs, .. }) = self.entities.get(&observer).copied() {
                self.entities.insert(
                    observer,
                    Life::Known {
                        revs,
                        since_tick: now_tick,
                    },
                );
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Cadence {
    High,
    Normal,
    Low,
    EventOnly,
}

fn cadence_for(world: &World, observer: EntityId, id: EntityId) -> Cadence {
    if id == observer {
        return Cadence::High;
    }
    if world.player_body_of(id).is_some() {
        let Some(a) = world.transform_of(observer) else {
            return Cadence::Normal;
        };
        let Some(b) = world.transform_of(id) else {
            return Cadence::Normal;
        };
        let dx = a.position[0] - b.position[0];
        let dy = a.position[1] - b.position[1];
        if dx * dx + dy * dy <= HIGH_DISTANCE * HIGH_DISTANCE {
            Cadence::High
        } else {
            Cadence::Normal
        }
    } else {
        match world.replication_of(id).map(|m| m.frequency) {
            Some(UpdateFrequencyTier::Event) => Cadence::EventOnly,
            Some(UpdateFrequencyTier::Low) => Cadence::Low,
            _ => Cadence::EventOnly,
        }
    }
}

fn cadence_allows(cadence: Cadence, tick: u64, id: EntityId, rev_pending: bool) -> bool {
    if !rev_pending {
        return false;
    }
    match cadence {
        Cadence::High | Cadence::EventOnly => true,
        Cadence::Normal => staggered_interval_due(tick, NORMAL_INTERVAL, id.index()),
        Cadence::Low => staggered_interval_due(tick, LOW_INTERVAL, id.index()),
    }
}

fn snapshot_entity(world: &World, id: EntityId) -> Option<SnapshotEntity> {
    if let Some(body) = world.player_body_of(id) {
        return Some(SnapshotEntity {
            entity_id: to_wire_id(id),
            kind: ReplicatedKind::Player,
            position: body.position,
            velocity: body.velocity,
        });
    }
    let interactable = world.interactable_of(id)?;
    let transform = world.transform_of(id)?;
    let kind = match interactable.kind {
        purgatory_simulation::InteractableKind::Portal => ReplicatedKind::Portal,
        _ => ReplicatedKind::Interactable,
    };
    Some(SnapshotEntity {
        entity_id: to_wire_id(id),
        kind,
        position: transform.position,
        velocity: [0.0, 0.0],
    })
}

fn wire_health(world: &World, id: EntityId) -> Option<ReplicatedHealth> {
    world.health_of(id).map(|h| ReplicatedHealth {
        current: h.current,
        max: h.max,
    })
}

fn enter_record(world: &World, id: EntityId) -> Option<ReplicationRecord> {
    Some(ReplicationRecord::Enter {
        entity: snapshot_entity(world, id)?,
        health: wire_health(world, id),
    })
}

fn update_record(world: &World, id: EntityId, last: CommittedRevs) -> Option<ReplicationRecord> {
    let revs = world.domain_revs_of(id)?;
    let transform = revs.transform > last.transform;
    let health = revs.health > last.health;
    if !transform && !health {
        return None;
    }
    let entity = snapshot_entity(world, id)?;
    Some(ReplicationRecord::Update {
        entity_id: to_wire_id(id),
        domains: DomainMask { transform, health },
        position: transform.then_some(entity.position),
        velocity: transform.then_some(entity.velocity),
        health: health.then_some(wire_health(world, id)).flatten(),
    })
}

fn contact_header(world: &World, local: EntityId) -> (bool, PlatformSupportId, PlatformSupportId) {
    let body = world.player_body_of(local);
    let grounded = body.is_some_and(|b| b.grounded);
    let on = body
        .and_then(|b| b.grounded_on)
        .and_then(|id| world.support_id_of(id))
        .map(PlatformSupportId)
        .unwrap_or(PlatformSupportId::NONE);
    let ign = body
        .and_then(|b| b.ignored_platform)
        .and_then(|id| world.support_id_of(id))
        .map(PlatformSupportId)
        .unwrap_or(PlatformSupportId::NONE);
    (grounded, on, ign)
}

#[allow(clippy::too_many_arguments)]
fn header_frame(
    sequence: u32,
    tick: u64,
    local: EntityId,
    world: &World,
    epoch: u32,
    input_epoch: u16,
    ack: u32,
    debt: u16,
    records: Vec<ReplicationRecord>,
) -> ReplicationFrame {
    let (grounded, on, ign) = contact_header(world, local);
    let addr = world.address_of(local);
    ReplicationFrame {
        snapshot_sequence: sequence,
        server_tick: tick,
        local_player_entity: to_wire_id(local),
        input_epoch,
        last_acknowledged_input_sequence: ack,
        local_grounded: grounded,
        local_grounded_on: on,
        local_ignored_platform: ign,
        continuation_debt: debt,
        local_map: addr.map(|a| a.map.raw()).unwrap_or(1),
        local_channel: addr.map(|a| a.channel.raw()).unwrap_or(0),
        local_instance: addr.map(|a| a.instance.raw()).unwrap_or(0),
        observer_baseline_epoch: epoch,
        records,
        aoi_debug: None,
    }
}

fn encoded_len(frame: &ReplicationFrame) -> Option<usize> {
    encode_replication_frame(frame).ok().map(|b| b.len())
}

/// Build at most one frame from observer intent. Commits only if `pipe.try_push` succeeds.
#[allow(clippy::too_many_arguments)]
pub fn publish_observer_frame(
    state: &mut ObserverReplicationState,
    pipe: &ReplicationPipe,
    world: &World,
    observer: EntityId,
    sequence: u32,
    tick: u64,
    input_epoch: u16,
    ack: u32,
    debt: u16,
) -> ReplicationTickStats {
    publish_observer_frame_with_budget(
        state,
        pipe,
        world,
        observer,
        sequence,
        tick,
        input_epoch,
        ack,
        debt,
        REPLICATION_FRAME_BUDGET_BYTES,
    )
}

/// Test/harness entry: same commit rules with an explicit encoded-size budget.
#[allow(clippy::too_many_arguments)]
pub fn publish_observer_frame_with_budget(
    state: &mut ObserverReplicationState,
    pipe: &ReplicationPipe,
    world: &World,
    observer: EntityId,
    sequence: u32,
    tick: u64,
    input_epoch: u16,
    ack: u32,
    debt: u16,
    budget_bytes: usize,
) -> ReplicationTickStats {
    let mut stats = ReplicationTickStats {
        candidates: world.spatial_candidates(observer).len() as u32,
        known: state.known_count() as u32,
        queue_depth: pipe.len() as u32,
        oldest_pending_ticks: state.oldest_pending_age(tick),
        ..ReplicationTickStats::default()
    };
    if pipe.len() >= WRITER_QUEUE_CAP {
        stats.mailbox_merge = 1;
        stats.pending = state.entities.len() as u32;
        return stats;
    }

    state.classify(world, observer, tick);

    let budget = budget_bytes.min(MAX_GAMEPLAY_SNAPSHOT_BYTES as usize);
    let empty = header_frame(
        sequence,
        tick,
        observer,
        world,
        state.epoch,
        input_epoch,
        ack,
        debt,
        Vec::new(),
    );
    let Some(mut total) = encoded_len(&empty) else {
        return stats;
    };

    let mut records: Vec<ReplicationRecord> = Vec::new();
    let mut commit_leaves: Vec<EntityId> = Vec::new();
    let mut commit_enters: Vec<(EntityId, DomainRevs)> = Vec::new();
    let mut commit_updates: Vec<(EntityId, DomainRevs)> = Vec::new();

    let mut leave_ids: Vec<EntityId> = state
        .entities
        .iter()
        .filter_map(|(id, life)| matches!(life, Life::WantLeave { .. }).then_some(*id))
        .collect();
    leave_ids.sort_by_key(|id| (id.index(), id.generation()));
    for id in leave_ids {
        let rec = ReplicationRecord::Leave {
            entity_id: to_wire_id(id),
        };
        let Ok(bytes) = encode_replication_record(&rec) else {
            continue;
        };
        if total + bytes.len() > budget {
            break;
        }
        total += bytes.len();
        records.push(rec);
        commit_leaves.push(id);
        stats.leaves = stats.leaves.saturating_add(1);
    }

    let mut enter_ids: Vec<EntityId> = state
        .entities
        .iter()
        .filter_map(|(id, life)| matches!(life, Life::WantEnter { .. }).then_some(*id))
        .collect();
    enter_ids.sort_by_key(|id| (u8::from(*id != observer), id.index(), id.generation()));
    if !enter_ids.is_empty() {
        let start = state.enter_cursor % enter_ids.len();
        let rotated: Vec<EntityId> = enter_ids
            .iter()
            .cycle()
            .skip(start)
            .take(enter_ids.len())
            .copied()
            .collect();
        let mut advanced = 0usize;
        for id in rotated {
            advanced += 1;
            let Some(rec) = enter_record(world, id) else {
                continue;
            };
            let Ok(bytes) = encode_replication_record(&rec) else {
                continue;
            };
            if total + bytes.len() > budget {
                continue;
            }
            total += bytes.len();
            records.push(rec);
            if let Some(revs) = world.domain_revs_of(id) {
                commit_enters.push((id, revs));
            }
            stats.enters = stats.enters.saturating_add(1);
        }
        state.enter_cursor = state.enter_cursor.wrapping_add(advanced.max(1));
    }

    let mut update_ids: Vec<EntityId> = Vec::new();
    for (id, life) in &state.entities {
        let Life::Known { revs: last, .. } = life else {
            continue;
        };
        let Some(revs) = world.domain_revs_of(*id) else {
            continue;
        };
        let pending = revs.transform > last.transform || revs.health > last.health;
        if !pending {
            continue;
        }
        stats.pending_updates = stats.pending_updates.saturating_add(1);
        let cadence = cadence_for(world, observer, *id);
        if cadence_allows(cadence, tick, *id, true) {
            update_ids.push(*id);
        } else {
            stats.cadence_deferred = stats.cadence_deferred.saturating_add(1);
        }
    }
    update_ids.sort_by_key(|id| (id.index(), id.generation()));
    for id in update_ids {
        if matches!(state.entities.get(&id), Some(Life::WantEnter { .. })) {
            continue;
        }
        let Some(Life::Known { revs: last, .. }) = state.entities.get(&id).copied() else {
            continue;
        };
        let Some(rec) = update_record(world, id, last) else {
            stats.skipped_unchanged = stats.skipped_unchanged.saturating_add(1);
            continue;
        };
        let Ok(bytes) = encode_replication_record(&rec) else {
            continue;
        };
        if total + bytes.len() > budget {
            continue;
        }
        total += bytes.len();
        records.push(rec);
        if let Some(revs) = world.domain_revs_of(id) {
            commit_updates.push((id, revs));
        }
        stats.updates = stats.updates.saturating_add(1);
    }

    let mut frame = header_frame(
        sequence,
        tick,
        observer,
        world,
        state.epoch,
        input_epoch,
        ack,
        debt,
        records,
    );
    frame.aoi_debug = Some(ObserverAoiDebug {
        candidates: u16::try_from(stats.candidates).unwrap_or(u16::MAX),
        known: u16::try_from(state.known_count()).unwrap_or(u16::MAX),
        want_enter: u16::try_from(state.want_enter_count()).unwrap_or(u16::MAX),
        want_leave: u16::try_from(state.want_leave_count()).unwrap_or(u16::MAX),
    });
    let Ok(payload) = encode_replication_frame(&frame) else {
        return stats;
    };
    if encode_gameplay_frame(&payload).is_err() {
        return stats;
    }
    stats.bytes = payload.len() as u32;
    let queued = QueuedFrame {
        epoch: state.epoch,
        sequence,
        payload,
    };
    if !pipe.try_push(queued) {
        stats.mailbox_merge = 1;
        return stats;
    }

    for id in commit_leaves {
        state.entities.remove(&id);
        state.last_leave_tick.insert(id, tick);
    }
    for (id, revs) in commit_enters {
        if state
            .last_leave_tick
            .get(&id)
            .is_some_and(|t| tick.saturating_sub(*t) <= CHURN_WINDOW_TICKS)
        {
            stats.churn_reentry = stats.churn_reentry.saturating_add(1);
        }
        state.entities.insert(
            id,
            Life::Known {
                revs: CommittedRevs {
                    transform: revs.transform,
                    health: revs.health,
                },
                since_tick: tick,
            },
        );
    }
    for (id, revs) in commit_updates {
        if let Some(Life::Known { since_tick, .. }) = state.entities.get(&id).copied() {
            state.entities.insert(
                id,
                Life::Known {
                    revs: CommittedRevs {
                        transform: revs.transform,
                        health: revs.health,
                    },
                    since_tick,
                },
            );
        }
    }
    stats.pending = state
        .entities
        .values()
        .filter(|l| !matches!(l, Life::Known { .. }))
        .count() as u32;
    stats.known = state.known_count() as u32;
    stats.queue_depth = pipe.len() as u32;
    stats.oldest_pending_ticks = state.oldest_pending_age(tick);
    stats
}

#[cfg(test)]
mod tests {
    use super::super::snapshot::from_wire_id;
    use super::*;
    use purgatory_protocol::decode_replication_frame;
    use purgatory_simulation::{PlayerState, Transform, World};

    fn two_players() -> (World, EntityId, EntityId) {
        let mut world = World::footnote_test_stage();
        if let Some(id) = world.player_id() {
            world.despawn(id);
        }
        let floor = world.iter_platforms().next().expect("floor");
        let (t, s) = PlayerState::standing_on_at(
            floor.id,
            floor.top_surface(),
            purgatory_simulation::FOOTNOTE_SPAWN_X,
        );
        let a = world.spawn_player(t, s);
        let (t, s) = PlayerState::standing_on_at(
            floor.id,
            floor.top_surface(),
            purgatory_simulation::FOOTNOTE_SPAWN_X + 1.0,
        );
        let b = world.spawn_player(t, s);
        (world, a, b)
    }

    #[test]
    fn epoch_purge_after_queue_commit_resets_committed_revs() {
        let (world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let stats = publish_observer_frame(&mut state, &pipe, &world, observer, 1, 1, 0, 0, 0);
        assert!(stats.enters >= 2, "observer and nearby remote must Enter");
        assert_eq!(pipe.len(), 1);
        assert!(
            state.is_known(observer) && state.committed_revs(observer).is_some(),
            "queue-commit must mark the observer Known"
        );
        assert!(
            state.is_known(remote) && state.committed_revs(remote).is_some(),
            "queue-commit must mark the remote Known"
        );
        let queued_epoch = pipe.queued_epochs()[0];
        assert_eq!(queued_epoch, 0);

        state.bump_epoch();
        let purged = pipe.purge_older_than(state.epoch);
        assert_eq!(purged, 1, "queued old-epoch frame must be dropped");
        assert_eq!(pipe.len(), 0);
        assert!(
            state.committed_revs(observer).is_none() && state.committed_revs(remote).is_none(),
            "queue-commit must not survive epoch reset"
        );
        assert!(!state.is_known(remote));
        assert!(!state.is_known(observer));

        let stats2 = publish_observer_frame(&mut state, &pipe, &world, observer, 2, 2, 0, 0, 0);
        assert!(
            stats2.enters >= 2,
            "new epoch must send a current baseline, not skip Enters as already-delivered"
        );
        assert_eq!(pipe.len(), 1);
        assert_eq!(pipe.queued_epochs()[0], state.epoch);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        assert_eq!(frame.observer_baseline_epoch, state.epoch);
        assert!(
            !frame
                .records
                .iter()
                .any(|r| matches!(r, ReplicationRecord::Update { .. })),
            "purged queue-commit must not be treated as delivered (no Updates without new Enters)"
        );
        assert!(frame.records.iter().any(|r| matches!(
            r,
            ReplicationRecord::Enter { entity, .. } if entity.entity_id == to_wire_id(observer)
        )));
        assert!(frame.records.iter().any(|r| matches!(
            r,
            ReplicationRecord::Enter { entity, .. } if entity.entity_id == to_wire_id(remote)
        )));
        assert!(state.is_known(observer) && state.is_known(remote));
    }

    #[test]
    fn updates_do_not_precede_enter() {
        let (world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        publish_observer_frame(&mut state, &pipe, &world, observer, 1, 1, 0, 0, 0);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        let mut seen_enter = HashSet::new();
        for rec in &frame.records {
            match rec {
                ReplicationRecord::Enter { entity, .. } => {
                    seen_enter.insert(entity.entity_id);
                }
                ReplicationRecord::Update { entity_id, .. } => {
                    assert!(
                        seen_enter.contains(entity_id) || state.is_known(from_wire_id(*entity_id)),
                        "Update must not precede Enter for {entity_id}"
                    );
                }
                ReplicationRecord::Leave { .. } => {}
            }
        }
        let _ = remote;
    }

    #[test]
    fn deferred_transforms_coalesce_to_latest() {
        let (mut world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        publish_observer_frame(&mut state, &pipe, &world, observer, 1, 1, 0, 0, 0);
        let _ = pipe.pop();
        let start = world.transform_of(remote).unwrap();
        for i in 0..10 {
            let mut t = start;
            t.position[0] += i as f32;
            world.set_transform(remote, t);
        }
        publish_observer_frame(&mut state, &pipe, &world, observer, 2, 2, 0, 0, 0);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        let updates: Vec<_> = frame
            .records
            .iter()
            .filter(|r| matches!(r, ReplicationRecord::Update { entity_id, .. } if *entity_id == to_wire_id(remote)))
            .collect();
        assert!(updates.len() <= 1);
        if let Some(ReplicationRecord::Update { position, .. }) = updates.first() {
            assert_eq!(
                *position,
                Some(world.transform_of(remote).unwrap().position)
            );
        }
    }

    #[test]
    fn hysteresis_band_does_not_enter_until_enter_rect() {
        let mut world = World::footnote_test_stage();
        if let Some(id) = world.player_id() {
            world.despawn(id);
        }
        let floor = world.iter_platforms().next().unwrap();
        let (t, s) = PlayerState::standing_on_at(floor.id, floor.top_surface(), 0.0);
        let observer = world.spawn_player(t, s);
        let rects = world.aoi_rects_for(observer).unwrap();
        let band_x = rects.enter.max_x() + 0.5;
        let (t, s) = PlayerState::standing_on_at(floor.id, floor.top_surface(), band_x);
        let remote = world.spawn_player(t, s);
        let mut state = ObserverReplicationState::new();
        state.classify(&world, observer, 1);
        assert!(
            !matches!(state.entities.get(&remote), Some(Life::WantEnter { .. })),
            "band must not WantEnter"
        );
        let _ = Transform::from_position([0.0, 0.0]);
    }

    #[test]
    fn progressive_enter_does_not_emit_updates_before_known() {
        let mut world = World::footnote_test_stage();
        if let Some(id) = world.player_id() {
            world.despawn(id);
        }
        let floor = world.iter_platforms().next().unwrap();
        let (t, s) = PlayerState::standing_on_at(floor.id, floor.top_surface(), 0.0);
        let observer = world.spawn_player(t, s);
        let mut remotes = Vec::new();
        for i in 0..40 {
            let (t, s) =
                PlayerState::standing_on_at(floor.id, floor.top_surface(), 0.25 * (i as f32 + 1.0));
            remotes.push(world.spawn_player(t, s));
        }
        let empty = header_frame(1, 1, observer, &world, 0, 0, 0, 0, Vec::new());
        let empty_len = encoded_len(&empty).unwrap();
        let sample = enter_record(&world, observer).unwrap();
        let enter_len = encode_replication_record(&sample).unwrap().len();
        let budget = empty_len + 8 * enter_len;
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut entered = HashSet::new();
        let mut saw_update = false;
        for seq in 1..=12 {
            let _ = pipe.pop();
            publish_observer_frame_with_budget(
                &mut state,
                &pipe,
                &world,
                observer,
                seq,
                u64::from(seq),
                0,
                0,
                0,
                budget,
            );
            let Some(queued) = pipe.pop() else {
                continue;
            };
            let frame = decode_replication_frame(&queued.payload).unwrap();
            for rec in &frame.records {
                match rec {
                    ReplicationRecord::Enter { entity, .. } => {
                        entered.insert(entity.entity_id);
                    }
                    ReplicationRecord::Update { entity_id, .. } => {
                        saw_update = true;
                        assert!(
                            entered.contains(entity_id) || state.is_known(from_wire_id(*entity_id)),
                            "Update for {entity_id} before Enter was queue-committed"
                        );
                    }
                    ReplicationRecord::Leave { .. } => {}
                }
            }
        }
        assert!(
            entered.len() >= 9,
            "budget of 8 Enters/frame must still progress, got {}",
            entered.len()
        );
        let _ = (saw_update, remotes);
        assert!(
            state.known_count() >= 9,
            "progressive Enter must commit Known only after queue accept"
        );
    }

    #[test]
    fn first_observe_is_enter_baseline() {
        let (world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let stats = publish_observer_frame(&mut state, &pipe, &world, observer, 1, 1, 0, 0, 0);
        assert!(stats.enters >= 2);
        assert_eq!(stats.updates, 0);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        assert!(frame.records.iter().any(|r| matches!(
            r,
            ReplicationRecord::Enter { entity, .. } if entity.entity_id == to_wire_id(remote)
        )));
        assert!(!frame.records.iter().any(|r| matches!(
            r,
            ReplicationRecord::Update { entity_id, .. } if *entity_id == to_wire_id(remote)
        )));
    }

    #[test]
    fn unchanged_known_does_not_emit_update() {
        let (world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        publish_observer_frame(&mut state, &pipe, &world, observer, 1, 1, 0, 0, 0);
        let _ = pipe.pop();
        let stats = publish_observer_frame(&mut state, &pipe, &world, observer, 2, 2, 0, 0, 0);
        assert_eq!(stats.updates, 0);
        assert_eq!(stats.pending_updates, 0);
        let _ = remote;
    }

    #[test]
    fn mutation_emits_delta_update() {
        let (mut world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        publish_observer_frame(&mut state, &pipe, &world, observer, 1, 1, 0, 0, 0);
        let _ = pipe.pop();
        let mut t = world.transform_of(remote).unwrap();
        t.position[0] += 0.5;
        world.set_transform(remote, t);
        let stats = publish_observer_frame(&mut state, &pipe, &world, observer, 2, 2, 0, 0, 0);
        assert!(stats.updates >= 1);
        assert!(stats.pending_updates >= 1);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        assert!(frame.records.iter().any(|r| matches!(
            r,
            ReplicationRecord::Update { entity_id, .. } if *entity_id == to_wire_id(remote)
        )));
    }

    #[test]
    fn one_observer_commit_does_not_clear_another() {
        let (mut world, a, b) = two_players();
        let (pipe_a, _rx_a) = ReplicationPipe::new();
        let (pipe_b, _rx_b) = ReplicationPipe::new();
        let mut state_a = ObserverReplicationState::new();
        let mut state_b = ObserverReplicationState::new();
        publish_observer_frame(&mut state_a, &pipe_a, &world, a, 1, 1, 0, 0, 0);
        publish_observer_frame(&mut state_b, &pipe_b, &world, b, 1, 1, 0, 0, 0);
        let _ = pipe_a.pop();
        let _ = pipe_b.pop();
        let mut t = world.transform_of(a).unwrap();
        t.position[0] += 0.4;
        world.set_transform(a, t);
        let stats_a = publish_observer_frame(&mut state_a, &pipe_a, &world, a, 2, 2, 0, 0, 0);
        let _ = pipe_a.pop();
        assert!(stats_a.updates >= 1 || stats_a.pending_updates >= 1);
        let stats_b = publish_observer_frame(&mut state_b, &pipe_b, &world, b, 2, 2, 0, 0, 0);
        assert!(
            stats_b.pending_updates >= 1 || stats_b.updates >= 1,
            "observer B still needs the mutated entity after A committed"
        );
        let frame_b = decode_replication_frame(&pipe_b.pop().unwrap().payload).unwrap();
        assert!(frame_b.records.iter().any(|r| matches!(
            r,
            ReplicationRecord::Update { entity_id, .. } if *entity_id == to_wire_id(a)
        )));
    }

    #[test]
    fn leave_then_reenter_sends_baseline_again() {
        let mut world = World::footnote_test_stage();
        if let Some(id) = world.player_id() {
            world.despawn(id);
        }
        let floor = world.iter_platforms().next().unwrap();
        let (t, s) = PlayerState::standing_on_at(
            floor.id,
            floor.top_surface(),
            purgatory_simulation::FOOTNOTE_SPAWN_X,
        );
        let observer = world.spawn_player(t, s);
        let (t, s) = PlayerState::standing_on_at(
            floor.id,
            floor.top_surface(),
            purgatory_simulation::FOOTNOTE_SPAWN_X + 1.0,
        );
        let remote = world.spawn_player(t, s);
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        publish_observer_frame(&mut state, &pipe, &world, observer, 1, 1, 0, 0, 0);
        let _ = pipe.pop();
        assert!(state.is_known(remote));
        let far = floor.top_surface();
        let mut t = world.transform_of(observer).unwrap();
        t.position[0] = -80.0;
        t.position[1] = far;
        world.set_transform(observer, t);
        publish_observer_frame(&mut state, &pipe, &world, observer, 2, 2, 0, 0, 0);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        assert!(frame.records.iter().any(|r| matches!(
            r,
            ReplicationRecord::Leave { entity_id } if *entity_id == to_wire_id(remote)
        )));
        t.position[0] = purgatory_simulation::FOOTNOTE_SPAWN_X;
        world.set_transform(observer, t);
        publish_observer_frame(&mut state, &pipe, &world, observer, 3, 3, 0, 0, 0);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        assert!(frame.records.iter().any(|r| matches!(
            r,
            ReplicationRecord::Enter { entity, .. } if entity.entity_id == to_wire_id(remote)
        )));
    }
}
