//! Per-observer replication interest, coalescing mailbox, and budgeted v8 frames.
//!
//! Simulation never stores ConnectionId. This module lives on GameplayOwner.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use purgatory_protocol::{
    DomainMask, MAX_GAMEPLAY_SNAPSHOT_BYTES, ObserverAoiDebug, PlatformSupportId,
    ReplicatedEquipment, ReplicatedEquipmentDelta, ReplicatedHealth, ReplicatedKind,
    ReplicationFrame, ReplicationRecord, SnapshotEntity, encode_gameplay_frame,
    encode_replication_frame, encode_replication_record,
};
use purgatory_simulation::{
    EntityId, EquipmentDirtyMask, EquipmentSlot, EquipmentState, ReplicationDirtyMask,
    UpdateFrequencyTier, World, aoi_policy_rects, point_in_aabb, staggered_interval_due,
};
use tokio::sync::watch;

use super::replication_policy::{
    DomainEligibility, PolicyContext, PolicyMode, PopulationClass, RelationOverrides,
    ReplicationPriority, classify_relation, decide_update_policy, observer_frame_budget_bytes,
};
#[cfg(test)]
use super::replication_policy::{ObserverRelationKind, PressureLevel};
use super::snapshot::{from_wire_id, to_wire_id};

/// Encoded-frame queue cap. If full, sim does not encode-and-drop.
pub const WRITER_QUEUE_CAP: usize = 4;

/// Soft per-frame budget. Must not exceed [`MAX_GAMEPLAY_SNAPSHOT_BYTES`].
/// Override with `PURGATORY_REPLICATION_FRAME_BUDGET_BYTES` (clamped to [512, hard max]).
pub const REPLICATION_FRAME_BUDGET_BYTES: usize = 4096;
pub const REPLICATION_FRAME_BUDGET_ENV: &str = "PURGATORY_REPLICATION_FRAME_BUDGET_BYTES";

/// Soft byte budget base for this process (env override or default).
#[must_use]
pub fn replication_frame_budget_base() -> usize {
    std::env::var(REPLICATION_FRAME_BUDGET_ENV)
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(REPLICATION_FRAME_BUDGET_BYTES)
        .clamp(512, MAX_GAMEPLAY_SNAPSHOT_BYTES as usize)
}

const LOW_INTERVAL: u64 = 4;
const CHURN_WINDOW_TICKS: u64 = 30;
/// Staggered full Known rev-reconcile interval (≈2s at 30 Hz). Safety net only.
const RECOVERY_SCAN_INTERVAL: u64 = 60;

/// Inputs for 6G.7C policy during one observer publish.
#[derive(Clone, Copy, Debug)]
pub struct PublishPolicyInput<'a> {
    pub mode: PolicyMode,
    pub population: PopulationClass,
    pub overrides: &'a RelationOverrides,
    pub recent_observer_bytes: u32,
    pub tick_overrun_hint: bool,
}

impl<'a> PublishPolicyInput<'a> {
    #[must_use]
    #[cfg(test)]
    pub fn baseline(overrides: &'a RelationOverrides) -> Self {
        Self {
            mode: PolicyMode::Baseline,
            population: PopulationClass::Low,
            overrides,
            recent_observer_bytes: 0,
            tick_overrun_hint: false,
        }
    }
}

/// Compatibility wrapper used by older call sites / tests.
#[cfg(test)]
#[must_use]
pub fn domain_eligibility_for(
    relation: ObserverRelationKind,
    dirty: ReplicationDirtyMask,
) -> DomainEligibility {
    let ctx = PolicyContext {
        mode: PolicyMode::Selective,
        population: PopulationClass::Low,
        pressure: PressureLevel::Calm,
        observer_known: 0,
        subject_interested: 0,
    };
    decide_update_policy(ctx, relation, dirty).eligibility
}

/// Entity → observers that currently have the subject in `Life::Known`.
#[derive(Debug, Default)]
pub struct InterestFanoutIndex {
    observers_of: HashMap<EntityId, HashSet<EntityId>>,
}

impl InterestFanoutIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_known(&mut self, observer: EntityId, subject: EntityId) {
        self.observers_of
            .entry(subject)
            .or_default()
            .insert(observer);
    }

    pub fn remove_known(&mut self, observer: EntityId, subject: EntityId) {
        let Some(set) = self.observers_of.get_mut(&subject) else {
            return;
        };
        set.remove(&observer);
        if set.is_empty() {
            self.observers_of.remove(&subject);
        }
    }

    pub fn clear_observer(&mut self, observer: EntityId) {
        let subjects: Vec<EntityId> = self.observers_of.keys().copied().collect();
        for subject in subjects {
            self.remove_known(observer, subject);
        }
    }

    pub fn clear_subject(&mut self, subject: EntityId) {
        self.observers_of.remove(&subject);
    }

    #[must_use]
    pub fn observer_count(&self, subject: EntityId) -> usize {
        self.observers_of
            .get(&subject)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    pub fn observers(&self, subject: EntityId) -> impl Iterator<Item = EntityId> + '_ {
        self.observers_of
            .get(&subject)
            .into_iter()
            .flat_map(|s| s.iter().copied())
    }
}

/// Enqueue pending updates from the current world dirty set (does not clear).
///
/// Call [`World::clear_replication_dirty`] once after all observers are enqueued.
#[cfg(test)]
#[must_use]
pub fn distribute_replication_dirty(
    world: &World,
    fanout: &InterestFanoutIndex,
    mut enqueue: impl FnMut(EntityId, EntityId, ReplicationDirtyMask),
) -> (u32, u32, u32, u32) {
    let overrides = RelationOverrides::default();
    let mut dirty_entities = 0u32;
    let mut dirty_transform = 0u32;
    let mut dirty_health = 0u32;
    let mut interested = 0u32;
    for (subject, mask) in world.replication_dirty_iter() {
        dirty_entities = dirty_entities.saturating_add(1);
        if mask.transform {
            dirty_transform = dirty_transform.saturating_add(1);
        }
        if mask.health {
            dirty_health = dirty_health.saturating_add(1);
        }
        for observer in fanout.observers(subject) {
            let relation = classify_relation(world, observer, subject, &overrides);
            let ctx = PolicyContext {
                mode: PolicyMode::Baseline,
                population: PopulationClass::Low,
                pressure: PressureLevel::Calm,
                observer_known: 0,
                subject_interested: fanout.observer_count(subject) as u32,
            };
            let decision = decide_update_policy(ctx, relation, mask);
            if !decision.eligibility.any() && !decision.suppress_emit {
                continue;
            }
            // Always enqueue when any domain lagged; policy filters at emit time.
            interested = interested.saturating_add(1);
            enqueue(observer, subject, mask);
        }
    }
    (dirty_entities, dirty_transform, dirty_health, interested)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CommittedRevs {
    pub transform: u64,
    pub health: u64,
    pub damage_immunity_active: bool,
    pub equipment: u64,
    pub equipment_state: Option<EquipmentState>,
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
    /// Health lifecycle state last delivered to this observer. This is
    /// distinct from committed revisions so selective ordinary health
    /// coalescing remains unchanged.
    known_dead: HashSet<EntityId>,
    enter_cursor: usize,
    last_leave_tick: HashMap<EntityId, u64>,
    /// True after at least one successful classify since bind/epoch (6G.5).
    interest_classified: bool,
    /// Subjects with pending domain work (dirty fan-out or recovery) (6G.7B).
    pending_update_ids: HashSet<EntityId>,
    last_recovery_tick: u64,
}

#[derive(Clone, Debug)]
pub struct QueuedFrame {
    pub epoch: u32,
    #[allow(dead_code)]
    pub sequence: u32,
    pub payload: Vec<u8>,
    /// Sim enqueue Instant. Age at pop is queue wait, not send CPU.
    pub enqueued_at: Instant,
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
    /// Micros spent in spatial query + AOI classify (6G.2 capacity).
    pub aoi_us: u64,
    /// Micros spent encoding/pushing the frame after classify.
    pub replicate_us: u64,
    /// Policy / cadence / rank (non-overlapping with encode/enqueue).
    pub policy_us: u64,
    /// Record + frame serialization.
    pub encode_us: u64,
    /// `try_push` handoff. Not QUIC send.
    pub enqueue_us: u64,
    /// 1 when this observer ran spatial query + classify this tick (6G.5).
    pub classified: u32,
    /// Known→WantLeave or new WantEnter transitions during classify (6G.6).
    pub membership_transitions: u32,
    /// Classify ran but produced zero membership transitions (wasted classify).
    pub classify_unchanged: u32,
    /// |Known| relationships for this observer (baseline scan volume).
    pub known_relationships_present: u32,
    /// Relationships examined for update discovery this tick (6G.7B).
    pub known_relationships_scanned: u32,
    /// Update encode attempts (includes budget rejects).
    pub serialize_attempts: u32,
    /// Pending updates skipped due to frame byte budget.
    pub budget_deferred_updates: u32,
    /// Lagging Known subjects found only by recovery scan.
    pub recovery_rescues: u32,
    /// Interested edges that remain eligible after policy (6G.7C).
    pub policy_eligible: u32,
    /// Health (or other) domains suppressed by relation policy.
    pub policy_domain_suppressed: u32,
    /// Updates deferred because lower priority lost the byte race.
    pub priority_deferred: u32,
    /// State updates held for coalescing/cadence (same as cadence_deferred; explicit).
    pub state_coalesced: u32,
    /// 1 when a gameplay payload was encoded this publish.
    pub frame_encoded: u32,
    /// 1 when `try_push` accepted the frame.
    pub frame_enqueued: u32,
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
        self.known_dead.clear();
        self.enter_cursor = 0;
        self.last_leave_tick.clear();
        self.interest_classified = false;
        self.pending_update_ids.clear();
        self.last_recovery_tick = 0;
    }

    pub fn queue_pending_update(&mut self, subject: EntityId) {
        self.pending_update_ids.insert(subject);
    }

    #[must_use]
    #[cfg(test)]
    pub fn pending_update_count(&self) -> usize {
        self.pending_update_ids.len()
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
    fn knows_dead_health(&self, id: EntityId) -> bool {
        self.known_dead.contains(&id)
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

    /// True when this observer's interest may be stale (never classified or dirty).
    #[must_use]
    pub fn needs_classify(&self, world: &World, observer: EntityId) -> bool {
        !self.interest_classified || world.interest_observer_dirty(observer)
    }

    pub fn classify_with_candidates(
        &mut self,
        world: &mut World,
        observer: EntityId,
        now_tick: u64,
        candidates: &[EntityId],
        fanout: &mut InterestFanoutIndex,
    ) -> u32 {
        let mut membership_transitions = 0u32;
        let Some(obs_addr) = world.address_of(observer) else {
            fanout.clear_observer(observer);
            self.entities.clear();
            self.pending_update_ids.clear();
            self.interest_classified = false;
            world.clear_interest_observer_dirty(observer);
            return 0;
        };
        let Some(obs_pos) = world.transform_of(observer).map(|t| t.position) else {
            fanout.clear_observer(observer);
            self.entities.clear();
            self.pending_update_ids.clear();
            self.interest_classified = false;
            world.clear_interest_observer_dirty(observer);
            return 0;
        };
        let rects = aoi_policy_rects(obs_pos, world.bounds_for(obs_addr));
        let candidate_set: HashSet<EntityId> = candidates.iter().copied().collect();

        let tracked: Vec<EntityId> = self.entities.keys().copied().collect();
        for id in tracked {
            if id == observer {
                continue;
            }
            let in_leave = world
                .transform_of(id)
                .is_some_and(|t| point_in_aabb(t.position, rects.leave))
                && world.address_of(id) == Some(obs_addr)
                && candidate_set.contains(&id);
            let in_enter = world
                .transform_of(id)
                .is_some_and(|t| point_in_aabb(t.position, rects.enter))
                && in_leave;
            match self.entities.get(&id).copied() {
                Some(Life::WantEnter { .. }) if !in_enter => {
                    self.entities.remove(&id);
                    membership_transitions = membership_transitions.saturating_add(1);
                }
                Some(Life::Known { revs, .. }) if !in_leave => {
                    fanout.remove_known(observer, id);
                    self.pending_update_ids.remove(&id);
                    self.entities.insert(
                        id,
                        Life::WantLeave {
                            revs,
                            since_tick: now_tick,
                        },
                    );
                    membership_transitions = membership_transitions.saturating_add(1);
                }
                Some(Life::WantLeave { revs, since_tick }) if in_enter => {
                    // Leave must go first; stay WantLeave until committed.
                    let _ = (revs, since_tick);
                }
                _ => {}
            }
        }

        for id in candidates {
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
                membership_transitions = membership_transitions.saturating_add(1);
            }
        }

        if !self.entities.contains_key(&observer) {
            self.entities.insert(
                observer,
                Life::WantEnter {
                    since_tick: now_tick,
                },
            );
            membership_transitions = membership_transitions.saturating_add(1);
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
                fanout.insert_known(observer, observer);
            }
        }
        self.interest_classified = true;
        world.clear_interest_observer_dirty(observer);
        membership_transitions
    }
}

#[derive(Clone, Copy)]
enum Cadence {
    EventOnly,
    Low,
}

fn cadence_for(world: &World, id: EntityId) -> Cadence {
    match world.replication_of(id).map(|m| m.frequency) {
        Some(UpdateFrequencyTier::Low) => Cadence::Low,
        _ => Cadence::EventOnly,
    }
}

fn cadence_allows(cadence: Cadence, tick: u64, id: EntityId, rev_pending: bool) -> bool {
    if !rev_pending {
        return false;
    }
    match cadence {
        Cadence::EventOnly => true,
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
    if let Some(interactable) = world.interactable_of(id) {
        let transform = world.transform_of(id)?;
        let kind = match interactable.kind {
            purgatory_simulation::InteractableKind::Portal => ReplicatedKind::Portal,
            _ => ReplicatedKind::Interactable,
        };
        return Some(SnapshotEntity {
            entity_id: to_wire_id(id),
            kind,
            position: transform.position,
            velocity: [0.0, 0.0],
        });
    }
    // Visible Generic / NPC: Transform required. Never emit Platform as Npc.
    if world.kind(id) != Some(purgatory_simulation::EntityKind::Generic) {
        return None;
    }
    let transform = world.transform_of(id)?;
    let velocity = world.npc_of(id).map(|n| n.velocity).unwrap_or([0.0, 0.0]);
    Some(SnapshotEntity {
        entity_id: to_wire_id(id),
        kind: ReplicatedKind::Npc,
        position: transform.position,
        velocity,
    })
}

fn wire_health(world: &World, id: EntityId) -> Option<ReplicatedHealth> {
    world.health_of(id).map(|h| ReplicatedHealth {
        current: h.current,
        max: h.max,
        damage_immunity_active: world.damage_immunity_active(id),
    })
}

fn wire_equipment_state(state: EquipmentState) -> ReplicatedEquipment {
    let mut out = ReplicatedEquipment::empty();
    for slot in EquipmentSlot::ALL {
        let _ = out.set(slot.index() as u8, state.get(slot));
    }
    out
}

fn equipment_slot_delta(
    from: Option<EquipmentState>,
    to: Option<EquipmentState>,
) -> Option<ReplicatedEquipmentDelta> {
    let from = from.unwrap_or_else(EquipmentState::empty);
    let to = to.unwrap_or_else(EquipmentState::empty);
    let mut delta = ReplicatedEquipmentDelta::empty();
    for slot in EquipmentSlot::ALL {
        if from.get(slot) != to.get(slot) {
            delta.set(slot.index() as u8, to.get(slot));
        }
    }
    (!delta.is_empty()).then_some(delta)
}

fn committed_from_world(world: &World, id: EntityId) -> Option<CommittedRevs> {
    let revs = world.domain_revs_of(id)?;
    Some(CommittedRevs {
        transform: revs.transform,
        health: revs.health,
        damage_immunity_active: world.damage_immunity_active(id),
        equipment: revs.equipment,
        equipment_state: world.equipment_of(id),
    })
}

fn enter_record(world: &World, id: EntityId) -> Option<ReplicationRecord> {
    Some(ReplicationRecord::Enter {
        entity: snapshot_entity(world, id)?,
        health: wire_health(world, id),
        equipment: world.equipment_of(id).map(wire_equipment_state),
    })
}

struct UpdateReconcile {
    /// `None` = ineligible domains caught up without wire payload.
    record: Option<ReplicationRecord>,
    next: CommittedRevs,
}

fn reconcile_update(
    world: &World,
    id: EntityId,
    last: CommittedRevs,
    allow: DomainEligibility,
) -> Option<UpdateReconcile> {
    let revs = world.domain_revs_of(id)?;
    let current_equipment = world.equipment_of(id);
    let transform = revs.transform > last.transform && allow.transform;
    let immunity = world.damage_immunity_active(id);
    let health = (revs.health > last.health || immunity != last.damage_immunity_active)
        && allow.health;
    let equipment_lag = revs.equipment > last.equipment;
    let equipment_delta = if equipment_lag && allow.equipment {
        equipment_slot_delta(last.equipment_state, current_equipment)
    } else {
        None
    };
    let equipment = equipment_delta.is_some();
    let silent_transform = revs.transform > last.transform && !allow.transform;
    let silent_health = revs.health > last.health && !allow.health;
    let silent_equipment = equipment_lag && !equipment;
    if !transform
        && !health
        && !equipment
        && !silent_transform
        && !silent_health
        && !silent_equipment
    {
        return None;
    }
    let next = CommittedRevs {
        transform: if transform || silent_transform {
            revs.transform
        } else {
            last.transform
        },
        health: if health || silent_health {
            revs.health
        } else {
            last.health
        },
        damage_immunity_active: if health || silent_health {
            immunity
        } else {
            last.damage_immunity_active
        },
        equipment: if equipment_lag {
            revs.equipment
        } else {
            last.equipment
        },
        equipment_state: if equipment_lag {
            current_equipment
        } else {
            last.equipment_state
        },
    };
    if !transform && !health && !equipment {
        return Some(UpdateReconcile { record: None, next });
    }
    let entity = snapshot_entity(world, id)?;
    Some(UpdateReconcile {
        record: Some(ReplicationRecord::Update {
            entity_id: to_wire_id(id),
            domains: DomainMask {
                transform,
                health,
                equipment,
            },
            position: transform.then_some(entity.position),
            velocity: transform.then_some(entity.velocity),
            health: health.then_some(wire_health(world, id)).flatten(),
            equipment: equipment_delta,
        }),
        next,
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
    world: &mut World,
    fanout: &mut InterestFanoutIndex,
    observer: EntityId,
    sequence: u32,
    tick: u64,
    input_epoch: u16,
    ack: u32,
    debt: u16,
    policy: PublishPolicyInput<'_>,
) -> ReplicationTickStats {
    let pressure = PolicyContext::classify_pressure(
        policy.population,
        state.known_count() as u32,
        0,
        policy.recent_observer_bytes,
        policy.tick_overrun_hint,
    );
    let budget = observer_frame_budget_bytes(replication_frame_budget_base(), pressure);
    publish_observer_frame_with_budget(
        state,
        pipe,
        world,
        fanout,
        observer,
        sequence,
        tick,
        input_epoch,
        ack,
        debt,
        budget,
        policy,
    )
}

/// Test/harness entry: same commit rules with an explicit encoded-size budget.
#[allow(clippy::too_many_arguments)]
pub fn publish_observer_frame_with_budget(
    state: &mut ObserverReplicationState,
    pipe: &ReplicationPipe,
    world: &mut World,
    fanout: &mut InterestFanoutIndex,
    observer: EntityId,
    sequence: u32,
    tick: u64,
    input_epoch: u16,
    ack: u32,
    debt: u16,
    budget_bytes: usize,
    policy: PublishPolicyInput<'_>,
) -> ReplicationTickStats {
    let aoi_t0 = std::time::Instant::now();
    let mut stats = ReplicationTickStats {
        known: state.known_count() as u32,
        known_relationships_present: state.known_count() as u32,
        queue_depth: pipe.len() as u32,
        oldest_pending_ticks: state.oldest_pending_age(tick),
        ..ReplicationTickStats::default()
    };
    if pipe.len() >= WRITER_QUEUE_CAP {
        stats.mailbox_merge = 1;
        stats.pending = state.entities.len() as u32;
        stats.candidates = state.entities.len() as u32;
        stats.aoi_us = u64::try_from(aoi_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
        return stats;
    }

    if state.needs_classify(world, observer) {
        let candidates = world.spatial_candidates_unsorted(observer);
        stats.candidates = candidates.len() as u32;
        let transitions =
            state.classify_with_candidates(world, observer, tick, &candidates, fanout);
        stats.classified = 1;
        stats.membership_transitions = transitions;
        if transitions == 0 {
            stats.classify_unchanged = 1;
        }
    } else {
        // Steady interest: skip spatial query + classify (6G.3 / 6G.5).
        stats.candidates = state.entities.len() as u32;
    }
    stats.known = state.known_count() as u32;
    stats.known_relationships_present = stats.known;
    stats.aoi_us = u64::try_from(aoi_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
    let repl_t0 = std::time::Instant::now();
    let mut encode_t0 = repl_t0;

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
        stats.replicate_us = u64::try_from(repl_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
        return stats;
    };

    let mut records: Vec<ReplicationRecord> = Vec::new();
    let mut commit_leaves: Vec<EntityId> = Vec::new();
    let mut commit_enters: Vec<(EntityId, CommittedRevs)> = Vec::new();
    let mut commit_updates: Vec<(EntityId, CommittedRevs)> = Vec::new();
    let mut clear_pending: Vec<EntityId> = Vec::new();
    let mut keep_pending: Vec<EntityId> = Vec::new();

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
            if let Some(revs) = committed_from_world(world, id) {
                commit_enters.push((id, revs));
            }
            stats.enters = stats.enters.saturating_add(1);
        }
        state.enter_cursor = state.enter_cursor.wrapping_add(advanced.max(1));
    }
    stats.encode_us = u64::try_from(encode_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
    let policy_t0 = std::time::Instant::now();

    // Staggered recovery: full Known rev reconcile (not the hot path).
    if staggered_interval_due(tick, RECOVERY_SCAN_INTERVAL, observer.index()) {
        state.last_recovery_tick = tick;
        for (id, life) in &state.entities {
            let Life::Known { revs: last, .. } = life else {
                continue;
            };
            let Some(revs) = world.domain_revs_of(*id) else {
                continue;
            };
            if (revs.transform > last.transform
                || revs.health > last.health
                || world.damage_immunity_active(*id) != last.damage_immunity_active
                || revs.equipment > last.equipment)
                && state.pending_update_ids.insert(*id)
            {
                stats.recovery_rescues = stats.recovery_rescues.saturating_add(1);
            }
        }
    }

    let pending_ids: Vec<EntityId> = state.pending_update_ids.iter().copied().collect();
    let mut ranked: Vec<(ReplicationPriority, EntityId, DomainEligibility)> = Vec::new();
    let known_n = state.known_count() as u32;
    for id in pending_ids {
        stats.known_relationships_scanned = stats.known_relationships_scanned.saturating_add(1);
        let Some(Life::Known { revs: last, .. }) = state.entities.get(&id).copied() else {
            clear_pending.push(id);
            continue;
        };
        let Some(revs) = world.domain_revs_of(id) else {
            clear_pending.push(id);
            continue;
        };
        let pending = revs.transform > last.transform
            || revs.health > last.health
            || world.damage_immunity_active(id) != last.damage_immunity_active
            || revs.equipment > last.equipment;
        if !pending {
            clear_pending.push(id);
            continue;
        }
        stats.pending_updates = stats.pending_updates.saturating_add(1);
        let relation = classify_relation(world, observer, id, policy.overrides);
        let interested = fanout.observer_count(id) as u32;
        let pressure = PolicyContext::classify_pressure(
            policy.population,
            known_n,
            interested,
            policy.recent_observer_bytes,
            policy.tick_overrun_hint,
        );
        let ctx = PolicyContext {
            mode: policy.mode,
            population: policy.population,
            pressure,
            observer_known: known_n,
            subject_interested: interested,
        };
        let dirty = ReplicationDirtyMask {
            transform: revs.transform > last.transform,
            health: revs.health > last.health,
            equipment: if revs.equipment > last.equipment {
                EquipmentDirtyMask::from_bits((1 << EquipmentSlot::COUNT) - 1)
                    .unwrap_or_else(EquipmentDirtyMask::empty)
            } else {
                EquipmentDirtyMask::empty()
            },
        };
        let mut decision = decide_update_policy(ctx, relation, dirty);
        // Dead is a persistent semantic state derived from authoritative
        // Health. Selective stranger policy may coalesce ordinary health
        // changes, but it must not silently catch up the lethal transition:
        // an existing observer needs the Health <= 0 update to resolve
        // PresentationActivity::Dead. Enter records already carry the same
        // Health value for late observers and baseline rebuilds.
        let lethal_health =
            dirty.health && world.health_of(id).is_some_and(|health| health.is_dead());
        let restoring_health = dirty.health
            && world.health_of(id).is_some_and(|health| health.is_alive())
            && state.knows_dead_health(id);
        if lethal_health || restoring_health {
            decision.eligibility.health = true;
            decision.cadence_interval = 1;
            decision.suppress_emit = false;
        }
        if decision.suppress_emit || !decision.eligibility.any() {
            // Ineligible domains: silent catch-up without emit.
            if dirty.health && !decision.eligibility.health {
                stats.policy_domain_suppressed = stats.policy_domain_suppressed.saturating_add(1);
            }
            let allow = decision.eligibility;
            if let Some(reconcile) = reconcile_update(world, id, last, allow) {
                commit_updates.push((id, reconcile.next));
            }
            clear_pending.push(id);
            continue;
        }
        stats.policy_eligible = stats.policy_eligible.saturating_add(1);
        let interval = decision.cadence_interval.max(1);
        let force_equipment = dirty.equipment.any();
        let due = if force_equipment || interval <= 1 {
            true
        } else {
            staggered_interval_due(tick, interval, id.index())
        };
        // Non-player entities keep authored frequency tiers as a floor.
        // Player cadence is `decide_update_policy` only (Nearby = enter AABB).
        let legacy_ok = force_equipment
            || world.player_body_of(id).is_some()
            || cadence_allows(cadence_for(world, id), tick, id, true);
        if due && legacy_ok {
            if dirty.health && !decision.eligibility.health {
                stats.policy_domain_suppressed = stats.policy_domain_suppressed.saturating_add(1);
            }
            ranked.push((decision.priority, id, decision.eligibility));
        } else {
            stats.cadence_deferred = stats.cadence_deferred.saturating_add(1);
            stats.state_coalesced = stats.state_coalesced.saturating_add(1);
            keep_pending.push(id);
        }
    }
    ranked.sort_by_key(|(prio, id, _)| {
        (
            *prio,
            u8::from(*id != observer),
            id.index(),
            id.generation(),
        )
    });
    stats.policy_us = u64::try_from(policy_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
    encode_t0 = std::time::Instant::now();
    for (_prio, id, allow) in ranked {
        let Some(Life::Known { revs: last, .. }) = state.entities.get(&id).copied() else {
            clear_pending.push(id);
            continue;
        };
        let Some(reconcile) = reconcile_update(world, id, last, allow) else {
            stats.skipped_unchanged = stats.skipped_unchanged.saturating_add(1);
            clear_pending.push(id);
            continue;
        };
        let Some(rec) = reconcile.record else {
            commit_updates.push((id, reconcile.next));
            clear_pending.push(id);
            continue;
        };
        stats.serialize_attempts = stats.serialize_attempts.saturating_add(1);
        let Ok(bytes) = encode_replication_record(&rec) else {
            keep_pending.push(id);
            continue;
        };
        if total + bytes.len() > budget {
            stats.budget_deferred_updates = stats.budget_deferred_updates.saturating_add(1);
            stats.priority_deferred = stats.priority_deferred.saturating_add(1);
            keep_pending.push(id);
            continue;
        }
        total += bytes.len();
        records.push(rec);
        commit_updates.push((id, reconcile.next));
        clear_pending.push(id);
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
        stats.encode_us = stats
            .encode_us
            .saturating_add(u64::try_from(encode_t0.elapsed().as_micros()).unwrap_or(u64::MAX));
        stats.replicate_us = u64::try_from(repl_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
        return stats;
    };
    if encode_gameplay_frame(&payload).is_err() {
        stats.encode_us = stats
            .encode_us
            .saturating_add(u64::try_from(encode_t0.elapsed().as_micros()).unwrap_or(u64::MAX));
        stats.replicate_us = u64::try_from(repl_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
        return stats;
    }
    stats.encode_us = stats
        .encode_us
        .saturating_add(u64::try_from(encode_t0.elapsed().as_micros()).unwrap_or(u64::MAX));
    stats.bytes = payload.len() as u32;
    stats.frame_encoded = 1;
    let queued = QueuedFrame {
        epoch: state.epoch,
        sequence,
        payload,
        enqueued_at: Instant::now(),
    };
    let enqueue_t0 = std::time::Instant::now();
    if !pipe.try_push(queued) {
        stats.mailbox_merge = 1;
        stats.enqueue_us = u64::try_from(enqueue_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
        stats.replicate_us = u64::try_from(repl_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
        return stats;
    }
    stats.frame_enqueued = 1;
    // Phase 7.5A: keep enqueue Instant open through post-push Known/Leave/pending
    // commit so detail-mode remainder does not absorb that handoff work.

    for id in commit_leaves {
        fanout.remove_known(observer, id);
        state.pending_update_ids.remove(&id);
        state.entities.remove(&id);
        state.known_dead.remove(&id);
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
                revs,
                since_tick: tick,
            },
        );
        if world.health_of(id).is_some_and(|health| health.is_dead()) {
            state.known_dead.insert(id);
        } else {
            state.known_dead.remove(&id);
        }
        fanout.insert_known(observer, id);
    }
    for (id, next) in commit_updates {
        if let Some(Life::Known { since_tick, .. }) = state.entities.get(&id).copied() {
            state.entities.insert(
                id,
                Life::Known {
                    revs: next,
                    since_tick,
                },
            );
        }
    }
    for record in &frame.records {
        if let ReplicationRecord::Update {
            entity_id,
            domains,
            health: Some(health),
            ..
        } = record
            && domains.health
        {
            let id = from_wire_id(*entity_id);
            if health.current <= 0.0 {
                state.known_dead.insert(id);
            } else {
                state.known_dead.remove(&id);
            }
        }
    }
    for id in clear_pending {
        state.pending_update_ids.remove(&id);
    }
    for id in keep_pending {
        state.pending_update_ids.insert(id);
    }
    stats.pending = state
        .entities
        .values()
        .filter(|l| !matches!(l, Life::Known { .. }))
        .count() as u32;
    stats.known = state.known_count() as u32;
    stats.queue_depth = pipe.len() as u32;
    stats.oldest_pending_ticks = state.oldest_pending_age(tick);
    stats.enqueue_us = u64::try_from(enqueue_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
    stats.replicate_us = u64::try_from(repl_t0.elapsed().as_micros()).unwrap_or(u64::MAX);
    stats
}

#[cfg(test)]
mod tests {
    use super::super::snapshot::from_wire_id;
    use super::*;
    use purgatory_protocol::{ReplicatedKind, decode_replication_frame};
    use purgatory_simulation::{
        ContentId, EquipmentSlot, PlayerState, SimulationTick, Transform, World,
    };

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

    fn publish(
        state: &mut ObserverReplicationState,
        pipe: &ReplicationPipe,
        world: &mut World,
        fanout: &mut InterestFanoutIndex,
        observer: EntityId,
        sequence: u32,
        tick: u64,
    ) -> ReplicationTickStats {
        let overrides = RelationOverrides::default();
        let _ = distribute_replication_dirty(world, fanout, |obs, subject, _mask| {
            if obs == observer {
                state.queue_pending_update(subject);
            }
        });
        world.clear_replication_dirty();
        publish_observer_frame(
            state,
            pipe,
            world,
            fanout,
            observer,
            sequence,
            tick,
            0,
            0,
            0,
            PublishPolicyInput::baseline(&overrides),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn publish_budget(
        state: &mut ObserverReplicationState,
        pipe: &ReplicationPipe,
        world: &mut World,
        fanout: &mut InterestFanoutIndex,
        observer: EntityId,
        sequence: u32,
        tick: u64,
        budget: usize,
    ) -> ReplicationTickStats {
        let overrides = RelationOverrides::default();
        let _ = distribute_replication_dirty(world, fanout, |obs, subject, _mask| {
            if obs == observer {
                state.queue_pending_update(subject);
            }
        });
        world.clear_replication_dirty();
        publish_observer_frame_with_budget(
            state,
            pipe,
            world,
            fanout,
            observer,
            sequence,
            tick,
            0,
            0,
            0,
            budget,
            PublishPolicyInput::baseline(&overrides),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn publish_mode(
        state: &mut ObserverReplicationState,
        pipe: &ReplicationPipe,
        world: &mut World,
        fanout: &mut InterestFanoutIndex,
        observer: EntityId,
        sequence: u32,
        tick: u64,
        mode: PolicyMode,
    ) -> ReplicationTickStats {
        let overrides = RelationOverrides::default();
        let _ = distribute_replication_dirty(world, fanout, |obs, subject, _mask| {
            if obs == observer {
                state.queue_pending_update(subject);
            }
        });
        world.clear_replication_dirty();
        publish_observer_frame(
            state,
            pipe,
            world,
            fanout,
            observer,
            sequence,
            tick,
            0,
            0,
            0,
            PublishPolicyInput {
                mode,
                population: PopulationClass::Low,
                overrides: &overrides,
                recent_observer_bytes: 0,
                tick_overrun_hint: false,
            },
        )
    }

    fn drain_pipe(pipe: &ReplicationPipe) {
        while pipe.pop().is_some() {}
    }

    struct EmitGapRun {
        relation: ObserverRelationKind,
        configured_interval: u64,
        legacy_high: bool,
        in_enter: bool,
        mean_dist: f32,
        emit_ticks: Vec<u64>,
        gaps: Vec<u64>,
        cadence_deferred: u32,
        budget_deferred: u32,
        coalesced: u32,
        priority_deferred: u32,
    }

    impl EmitGapRun {
        fn median_gap(&self) -> u64 {
            if self.gaps.is_empty() {
                return 0;
            }
            let mut sorted = self.gaps.clone();
            sorted.sort_unstable();
            sorted[sorted.len() / 2]
        }

        fn max_gap(&self) -> u64 {
            self.gaps.iter().copied().max().unwrap_or(0)
        }
    }

    /// Continuously-moving Known remote at a fixed observer offset. Measures
    /// actual transform-emit ticks, not configured cadence alone.
    fn measure_transform_emit_gaps(dx: f32, mode: PolicyMode, motion_ticks: u64) -> EmitGapRun {
        let (mut world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        let mut seq = 1u32;
        let mut tick = 1u64;
        for _ in 0..4 {
            let _ = publish_mode(
                &mut state,
                &pipe,
                &mut world,
                &mut fanout,
                observer,
                seq,
                tick,
                mode,
            );
            seq = seq.saturating_add(1);
            tick = tick.saturating_add(1);
        }
        drain_pipe(&pipe);
        assert!(
            matches!(state.entities.get(&remote), Some(Life::Known { .. })),
            "remote must be Known before the distance walk"
        );

        let observer_pos = world.transform_of(observer).unwrap().position;
        let floor = world.iter_platforms().next().unwrap();
        let target_x = observer_pos[0] + dx;
        let (placed, _) = PlayerState::standing_on_at(floor.id, floor.top_surface(), target_x);
        world.set_transform(remote, placed);
        let _ = publish_mode(
            &mut state,
            &pipe,
            &mut world,
            &mut fanout,
            observer,
            seq,
            tick,
            mode,
        );
        seq = seq.saturating_add(1);
        tick = tick.saturating_add(1);
        drain_pipe(&pipe);

        let rects = world.aoi_rects_for(observer).unwrap();
        let remote_pos = world.transform_of(remote).unwrap().position;
        let in_enter = point_in_aabb(remote_pos, rects.enter);
        let relation = classify_relation(&world, observer, remote, &RelationOverrides::default());
        let configured_interval = decide_update_policy(
            PolicyContext {
                mode,
                population: PopulationClass::Low,
                pressure: PressureLevel::Calm,
                observer_known: 2,
                subject_interested: 1,
            },
            relation,
            ReplicationDirtyMask {
                transform: true,
                ..ReplicationDirtyMask::default()
            },
        )
        .cadence_interval;
        let dist0 = {
            let d0 = remote_pos[0] - observer_pos[0];
            let d1 = remote_pos[1] - observer_pos[1];
            (d0 * d0 + d1 * d1).sqrt()
        };
        let legacy_high = dist0 <= 10.0;

        let mut emit_ticks = Vec::new();
        let mut cadence_deferred = 0u32;
        let mut budget_deferred = 0u32;
        let mut coalesced = 0u32;
        let mut priority_deferred = 0u32;
        let mut dist_sum = 0.0f32;
        let mut dir = 1.0f32;
        for _ in 0..motion_ticks {
            let mut t = world.transform_of(remote).unwrap();
            t.position[0] += 0.12 * dir;
            if (t.position[0] - target_x).abs() > 0.35 {
                dir *= -1.0;
            }
            world.set_transform(remote, t);
            let rp = world.transform_of(remote).unwrap().position;
            let d0 = rp[0] - observer_pos[0];
            let d1 = rp[1] - observer_pos[1];
            dist_sum += (d0 * d0 + d1 * d1).sqrt();
            let stats = publish_mode(
                &mut state,
                &pipe,
                &mut world,
                &mut fanout,
                observer,
                seq,
                tick,
                mode,
            );
            cadence_deferred = cadence_deferred.saturating_add(stats.cadence_deferred);
            budget_deferred = budget_deferred.saturating_add(stats.budget_deferred_updates);
            coalesced = coalesced.saturating_add(stats.state_coalesced);
            priority_deferred = priority_deferred.saturating_add(stats.priority_deferred);
            if let Some(frame) = pipe.pop() {
                let decoded = decode_replication_frame(&frame.payload).unwrap();
                let emitted = decoded.records.iter().any(|r| {
                    matches!(
                        r,
                        ReplicationRecord::Update {
                            entity_id,
                            position: Some(_),
                            ..
                        } if *entity_id == to_wire_id(remote)
                    )
                });
                if emitted {
                    emit_ticks.push(tick);
                }
            }
            seq = seq.saturating_add(1);
            tick = tick.saturating_add(1);
        }
        let gaps: Vec<u64> = emit_ticks.windows(2).map(|w| w[1] - w[0]).collect();
        EmitGapRun {
            relation,
            configured_interval,
            legacy_high,
            in_enter,
            mean_dist: dist_sum / motion_ticks as f32,
            emit_ticks,
            gaps,
            cadence_deferred,
            budget_deferred,
            coalesced,
            priority_deferred,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn publish_shared(
        states: &mut HashMap<EntityId, ObserverReplicationState>,
        pipe: &ReplicationPipe,
        world: &mut World,
        fanout: &mut InterestFanoutIndex,
        observer: EntityId,
        sequence: u32,
        tick: u64,
        clear_dirty: bool,
    ) -> ReplicationTickStats {
        let overrides = RelationOverrides::default();
        let _ = distribute_replication_dirty(world, fanout, |obs, subject, _mask| {
            if let Some(state) = states.get_mut(&obs) {
                state.queue_pending_update(subject);
            }
        });
        if clear_dirty {
            world.clear_replication_dirty();
        }
        let state = states.get_mut(&observer).unwrap();
        publish_observer_frame(
            state,
            pipe,
            world,
            fanout,
            observer,
            sequence,
            tick,
            0,
            0,
            0,
            PublishPolicyInput::baseline(&overrides),
        )
    }

    #[test]
    fn epoch_purge_after_queue_commit_resets_committed_revs() {
        let (mut world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        let stats = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
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
        fanout.clear_observer(observer);
        let purged = pipe.purge_older_than(state.epoch);
        assert_eq!(purged, 1, "queued old-epoch frame must be dropped");
        assert_eq!(pipe.len(), 0);
        assert!(
            state.committed_revs(observer).is_none() && state.committed_revs(remote).is_none(),
            "queue-commit must not survive epoch reset"
        );
        assert!(!state.is_known(remote));
        assert!(!state.is_known(observer));

        let stats2 = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 2, 2);
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
        let (mut world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
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
        let mut fanout = InterestFanoutIndex::new();
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();
        let start = world.transform_of(remote).unwrap();
        for i in 0..10 {
            let mut t = start;
            t.position[0] += i as f32;
            world.set_transform(remote, t);
        }
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 2, 2);
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
        let mut fanout = InterestFanoutIndex::new();
        let candidates = world.spatial_candidates_unsorted(observer);
        state.classify_with_candidates(&mut world, observer, 1, &candidates, &mut fanout);
        assert!(
            !matches!(state.entities.get(&remote), Some(Life::WantEnter { .. })),
            "band must not WantEnter"
        );
        let _ = Transform::from_position([0.0, 0.0]);
    }

    #[test]
    fn enter_rect_stranger_is_nearby_beyond_ten_wu() {
        let (mut world, observer, remote) = two_players();
        let observer_x = world.transform_of(observer).unwrap().position[0];
        let floor = world.iter_platforms().next().unwrap();
        let far_x = observer_x + 12.0;
        let (t, s) = PlayerState::standing_on_at(floor.id, floor.top_surface(), far_x);
        world.set_transform(remote, t);
        let _ = s;
        let dx = far_x - observer_x;
        assert!(dx > 10.0, "must be outside the old 10 wu radius");
        let rects = world.aoi_rects_for(observer).unwrap();
        assert!(
            point_in_aabb([far_x, t.position[1]], rects.enter),
            "fixture must remain inside enter"
        );
        let kind = classify_relation(&world, observer, remote, &RelationOverrides::default());
        assert_eq!(kind, ObserverRelationKind::NearbyStranger);
        let d = decide_update_policy(
            PolicyContext {
                mode: PolicyMode::Selective,
                population: PopulationClass::Low,
                pressure: PressureLevel::Calm,
                observer_known: 2,
                subject_interested: 1,
            },
            kind,
            ReplicationDirtyMask {
                transform: true,
                ..ReplicationDirtyMask::default()
            },
        );
        assert_eq!(d.cadence_interval, 2);
    }

    #[test]
    fn leave_band_stranger_is_distant() {
        let (mut world, observer, remote) = two_players();
        let rects = world.aoi_rects_for(observer).unwrap();
        let band_x = rects.enter.max_x() + 0.5;
        assert!(point_in_aabb(
            [band_x, world.transform_of(remote).unwrap().position[1]],
            rects.leave
        ));
        let floor = world.iter_platforms().next().unwrap();
        let (t, s) = PlayerState::standing_on_at(floor.id, floor.top_surface(), band_x);
        world.set_transform(remote, t);
        let _ = s;
        let kind = classify_relation(&world, observer, remote, &RelationOverrides::default());
        assert_eq!(kind, ObserverRelationKind::DistantStranger);
        let d = decide_update_policy(
            PolicyContext {
                mode: PolicyMode::Selective,
                population: PopulationClass::Low,
                pressure: PressureLevel::Calm,
                observer_known: 2,
                subject_interested: 1,
            },
            kind,
            ReplicationDirtyMask {
                transform: true,
                ..ReplicationDirtyMask::default()
            },
        );
        assert_eq!(d.cadence_interval, 4);
    }

    #[test]
    fn selective_nearby_transform_emit_gap_does_not_grow_with_distance_inside_enter() {
        let close = measure_transform_emit_gaps(2.0, PolicyMode::Selective, 24);
        let medium = measure_transform_emit_gaps(8.0, PolicyMode::Selective, 24);
        let far = measure_transform_emit_gaps(12.0, PolicyMode::Selective, 24);
        assert!(close.in_enter && medium.in_enter && far.in_enter);
        assert_eq!(close.relation, ObserverRelationKind::NearbyStranger);
        assert_eq!(medium.relation, ObserverRelationKind::NearbyStranger);
        assert_eq!(far.relation, ObserverRelationKind::NearbyStranger);
        assert_eq!(close.configured_interval, 2);
        assert_eq!(medium.configured_interval, 2);
        assert_eq!(far.configured_interval, 2);
        assert!(close.legacy_high && medium.legacy_high && !far.legacy_high);
        assert!(far.mean_dist > 10.0);
        assert!(close.mean_dist < 10.0);
        assert!(
            close.budget_deferred == 0 && far.budget_deferred == 0,
            "two-entity walk must not be byte-budget deferred"
        );
        assert!(
            close.priority_deferred == 0 && far.priority_deferred == 0,
            "two-entity walk must not lose the priority packer"
        );
        assert!(
            close.gaps.len() >= 6 && far.gaps.len() >= 6,
            "close emits={:?} far emits={:?}",
            close.emit_ticks,
            far.emit_ticks
        );
        assert_eq!(
            close.median_gap(),
            far.median_gap(),
            "Selective Nearby configured interval 2: actual gap must not grow with distance. close median={} max={} ticks={:?} deferred={} coalesced={}; far median={} max={} ticks={:?} deferred={} coalesced={}",
            close.median_gap(),
            close.max_gap(),
            close.emit_ticks,
            close.cadence_deferred,
            close.coalesced,
            far.median_gap(),
            far.max_gap(),
            far.emit_ticks,
            far.cadence_deferred,
            far.coalesced
        );
        assert_eq!(close.median_gap(), 2);
        assert_eq!(far.median_gap(), 2);
        assert!(
            close.max_gap() <= 2 && far.max_gap() <= 2,
            "no missed cadence slot inside enter: close max={} far max={}",
            close.max_gap(),
            far.max_gap()
        );
        let _ = medium;
    }

    #[test]
    fn selective_leave_band_transform_emit_gap_is_configured_four() {
        let (world, observer, _) = two_players();
        let rects = world.aoi_rects_for(observer).unwrap();
        let dx = rects.enter.max_x() - world.transform_of(observer).unwrap().position[0] + 0.5;
        drop(world);
        let run = measure_transform_emit_gaps(dx, PolicyMode::Selective, 24);
        assert!(
            !run.in_enter,
            "fixture must sit in leave hysteresis, not enter"
        );
        assert_eq!(run.relation, ObserverRelationKind::DistantStranger);
        assert_eq!(run.configured_interval, 4);
        assert!(run.gaps.len() >= 3, "leave-band emits={:?}", run.emit_ticks);
        assert_eq!(
            run.median_gap(),
            4,
            "Distant configured 4: actual median={} max={} ticks={:?} deferred={}",
            run.median_gap(),
            run.max_gap(),
            run.emit_ticks,
            run.cadence_deferred
        );
    }

    #[test]
    fn baseline_nearby_in_enter_emits_every_tick_inside_and_beyond_ten_wu() {
        let close = measure_transform_emit_gaps(2.0, PolicyMode::Baseline, 24);
        let far = measure_transform_emit_gaps(12.0, PolicyMode::Baseline, 24);
        assert_eq!(close.configured_interval, 1);
        assert_eq!(far.configured_interval, 1);
        assert!(close.in_enter && far.in_enter);
        assert_eq!(close.relation, ObserverRelationKind::NearbyStranger);
        assert_eq!(far.relation, ObserverRelationKind::NearbyStranger);
        assert!(close.legacy_high && !far.legacy_high);
        assert_eq!(
            close.median_gap(),
            1,
            "Baseline Nearby must emit every tick close: ticks={:?}",
            close.emit_ticks
        );
        assert_eq!(
            far.median_gap(),
            1,
            "Baseline Nearby must emit every tick far-in-enter: ticks={:?}",
            far.emit_ticks
        );
    }

    #[test]
    fn want_leave_is_not_cancelled_by_reenter_before_commit() {
        let (mut world, observer, remote) = two_players();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        let candidates = world.spatial_candidates_unsorted(observer);
        state.classify_with_candidates(&mut world, observer, 1, &candidates, &mut fanout);
        let (pipe, _rx) = ReplicationPipe::new();
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();
        assert!(matches!(
            state.entities.get(&remote),
            Some(Life::Known { .. })
        ));
        let start = world.transform_of(observer).unwrap().position;
        let mut ot = world.transform_of(observer).unwrap();
        ot.position[0] = start[0] + 80.0;
        world.set_transform(observer, ot);
        let candidates = world.spatial_candidates_unsorted(observer);
        state.classify_with_candidates(&mut world, observer, 2, &candidates, &mut fanout);
        assert!(
            matches!(state.entities.get(&remote), Some(Life::WantLeave { .. })),
            "outside leave must WantLeave"
        );
        world.set_transform(observer, Transform::from_position(start));
        let candidates = world.spatial_candidates_unsorted(observer);
        state.classify_with_candidates(&mut world, observer, 3, &candidates, &mut fanout);
        assert!(
            matches!(state.entities.get(&remote), Some(Life::WantLeave { .. })),
            "re-enter must not cancel uncommitted WantLeave"
        );
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
        let mut fanout = InterestFanoutIndex::new();
        let mut entered = HashSet::new();
        let mut saw_update = false;
        for seq in 1..=12 {
            let _ = pipe.pop();
            publish_budget(
                &mut state,
                &pipe,
                &mut world,
                &mut fanout,
                observer,
                seq,
                u64::from(seq),
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
        let (mut world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        let stats = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
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
    fn npc_generic_enters_as_replicated_kind_npc() {
        let (mut world, observer, _remote) = two_players();
        let npc = world
            .spawn(World::npc_spawn_request(
                world.address_of(observer).unwrap(),
                world.transform_of(observer).unwrap().position,
                1,
                3.0,
                42,
                SimulationTick::from_count(1),
                true,
                20.0,
            ))
            .expect("npc");
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        let stats = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        assert!(stats.enters >= 1);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        assert!(frame.records.iter().any(|r| matches!(
            r,
            ReplicationRecord::Enter { entity, health: Some(_), .. }
                if entity.entity_id == to_wire_id(npc)
                    && entity.kind == ReplicatedKind::Npc
        )));
        // Leave path: move observer far away so NPC leaves relevance.
        let mut t = world.transform_of(observer).unwrap();
        t.position[0] += 200.0;
        world.set_transform(observer, t);
        let _ = pipe.pop();
        let leave_stats = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 2, 2);
        assert!(leave_stats.leaves >= 1 || leave_stats.enters == 0);
    }

    #[test]
    fn unchanged_known_does_not_emit_update() {
        let (mut world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();
        let stats = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 2, 2);
        assert_eq!(stats.updates, 0);
        assert_eq!(stats.pending_updates, 0);
        let _ = remote;
    }

    #[test]
    fn mutation_emits_delta_update() {
        let (mut world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();
        let mut t = world.transform_of(remote).unwrap();
        t.position[0] += 0.5;
        world.set_transform(remote, t);
        let stats = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 2, 2);
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
        let mut fanout = InterestFanoutIndex::new();
        publish(&mut state_a, &pipe_a, &mut world, &mut fanout, a, 1, 1);
        publish(&mut state_b, &pipe_b, &mut world, &mut fanout, b, 1, 1);
        let _ = pipe_a.pop();
        let _ = pipe_b.pop();
        let mut t = world.transform_of(a).unwrap();
        t.position[0] += 0.4;
        world.set_transform(a, t);
        let _ = distribute_replication_dirty(&world, &fanout, |obs, subject, _mask| {
            if obs == a {
                state_a.queue_pending_update(subject);
            } else if obs == b {
                state_b.queue_pending_update(subject);
            }
        });
        world.clear_replication_dirty();
        let overrides = RelationOverrides::default();
        let policy = PublishPolicyInput::baseline(&overrides);
        let stats_a = publish_observer_frame(
            &mut state_a,
            &pipe_a,
            &mut world,
            &mut fanout,
            a,
            2,
            2,
            0,
            0,
            0,
            policy,
        );
        let _ = pipe_a.pop();
        assert!(stats_a.updates >= 1 || stats_a.pending_updates >= 1);
        let stats_b = publish_observer_frame(
            &mut state_b,
            &pipe_b,
            &mut world,
            &mut fanout,
            b,
            2,
            2,
            0,
            0,
            0,
            policy,
        );
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
        let mut fanout = InterestFanoutIndex::new();
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();
        assert!(state.is_known(remote));
        let far = floor.top_surface();
        let mut t = world.transform_of(observer).unwrap();
        // 6G interest is a clamped view envelope, not a player-centered radius.
        // x = -80 clamps to the left wall and still sees spawn; the right edge does not.
        t.position[0] = world.bounds().max_x - 0.4;
        t.position[1] = far;
        world.set_transform(observer, t);
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 2, 2);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        assert!(frame.records.iter().any(|r| matches!(
            r,
            ReplicationRecord::Leave { entity_id } if *entity_id == to_wire_id(remote)
        )));
        t.position[0] = purgatory_simulation::FOOTNOTE_SPAWN_X;
        world.set_transform(observer, t);
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 3, 3);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        assert!(frame.records.iter().any(|r| matches!(
            r,
            ReplicationRecord::Enter { entity, .. } if entity.entity_id == to_wire_id(remote)
        )));
    }

    #[test]
    fn steady_interest_skips_reclassify_until_membership_can_change() {
        let (mut world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        let stats1 = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();
        assert_eq!(stats1.classified, 1);
        assert!(state.is_known(remote) || state.want_enter_count() > 0 || state.is_known(observer));
        assert!(!state.needs_classify(&world, observer));
        assert!(!world.interest_observer_dirty(observer));
        // Second tick: no world pose/membership change → skip classify.
        let known_before = state.known_count();
        let stats2 = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 2, 2);
        let _ = pipe.pop();
        assert_eq!(stats2.classified, 0);
        assert!(!state.needs_classify(&world, observer));
        assert_eq!(state.known_count(), known_before);
        // Tiny remote move inside leave: 6G.7A must NOT dirty observer (transform
        // updates still flow via DomainRevs without reclassify).
        let mut t = world.transform_of(remote).unwrap();
        t.position[0] += 0.5;
        world.set_transform(remote, t);
        assert!(!world.interest_observer_dirty(observer));
        assert!(!state.needs_classify(&world, observer));
        let stats3 = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 3, 3);
        assert_eq!(stats3.classified, 0);
        // Drive remote far enough to exit leave → observer must reclassify.
        let mut t = world.transform_of(remote).unwrap();
        t.position[0] = world.bounds().max_x - 0.5;
        world.set_transform(remote, t);
        assert!(world.interest_observer_dirty(observer));
        assert!(state.needs_classify(&world, observer));
        let stats4 = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 4, 4);
        assert_eq!(stats4.classified, 1);
        assert!(!state.needs_classify(&world, observer));
    }

    #[test]
    fn local_motion_does_not_reclassify_unrelated_observers() {
        let mut world = World::footnote_test_stage();
        if let Some(id) = world.player_id() {
            world.despawn(id);
        }
        let floor = world.iter_platforms().next().expect("floor");
        let top = floor.top_surface();
        let left_x = world.bounds().min_x + 1.0;
        let right_x = world.bounds().max_x - 1.0;
        let mut left_observers = Vec::new();
        for i in 0..24 {
            let (t, s) = PlayerState::standing_on_at(floor.id, top, left_x + (i as f32) * 0.05);
            left_observers.push(world.spawn_player(t, s));
        }
        let (t, s) = PlayerState::standing_on_at(floor.id, top, right_x);
        let mover = world.spawn_player(t, s);
        let (t, s) = PlayerState::standing_on_at(floor.id, top, right_x - 0.5);
        let near = world.spawn_player(t, s);

        let (pipe, _rx) = ReplicationPipe::new();
        let mut fanout = InterestFanoutIndex::new();
        let mut states: HashMap<EntityId, ObserverReplicationState> = HashMap::new();
        for id in left_observers.iter().copied().chain([mover, near]) {
            states.insert(id, ObserverReplicationState::new());
        }
        // Baseline classify everyone.
        let ids: Vec<_> = states.keys().copied().collect();
        for (i, id) in ids.iter().copied().enumerate() {
            let _ = publish_shared(
                &mut states,
                &pipe,
                &mut world,
                &mut fanout,
                id,
                1,
                1,
                i + 1 == ids.len(),
            );
            let _ = pipe.pop();
        }
        assert_eq!(world.interest_dirty_observer_count(), 0);

        let mut t = world.transform_of(mover).unwrap();
        t.position[0] -= 0.25;
        world.set_transform(mover, t);

        let dirty = world.interest_dirty_observers_sorted();
        assert!(dirty.contains(&mover), "mover must be dirty as an observer");
        // Nearby observer is dirtied only when enter/leave XOR is non-empty (6G.7A).
        for id in &left_observers {
            assert!(
                !dirty.contains(id),
                "left-side observer {id:?} must not be reclassified for right-side motion"
            );
        }

        let mut reclassified = 0u32;
        let ids: Vec<_> = states.keys().copied().collect();
        for (i, id) in ids.iter().copied().enumerate() {
            let stats = publish_shared(
                &mut states,
                &pipe,
                &mut world,
                &mut fanout,
                id,
                2,
                2,
                i + 1 == ids.len(),
            );
            let _ = pipe.pop();
            reclassified = reclassified.saturating_add(stats.classified);
        }
        assert!(
            reclassified < left_observers.len() as u32,
            "reclassified={reclassified} must be local, not all {} left observers",
            left_observers.len()
        );
        assert!(
            reclassified <= 4,
            "expected a small local dirty set, got {reclassified}"
        );
        let _ = near;
    }

    #[test]
    fn one_dirty_among_many_known_scans_only_pending() {
        let (mut world, observer, remote) = two_players();
        let floor = world.iter_platforms().next().unwrap();
        let top = floor.top_surface();
        let base = purgatory_simulation::FOOTNOTE_SPAWN_X + 2.0;
        let mut crowd = Vec::new();
        for i in 0..20 {
            let (t, s) = PlayerState::standing_on_at(floor.id, top, base + (i as f32) * 0.15);
            crowd.push(world.spawn_player(t, s));
        }
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        let _ = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();
        let known = state.known_count();
        assert!(known >= 20, "crowd must be Known, got {known}");

        // Stationary crowd: no dirty → scan only recovery/pending (empty).
        let idle = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 2, 2);
        let _ = pipe.pop();
        assert_eq!(idle.updates, 0);
        assert_eq!(idle.known_relationships_present, known as u32);
        assert_eq!(
            idle.known_relationships_scanned, 0,
            "idle tick must not scan all Known"
        );

        let mut t = world.transform_of(remote).unwrap();
        t.position[0] += 0.35;
        world.set_transform(remote, t);
        let stats = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 3, 3);
        assert!(stats.updates >= 1);
        assert!(
            stats.known_relationships_scanned <= 4,
            "one dirty should not scan full Known ({}); scanned={}",
            known,
            stats.known_relationships_scanned
        );
        assert!(
            (stats.known_relationships_scanned as usize) < known / 2,
            "scanned must be << present Known"
        );
        assert_eq!(fanout.observer_count(remote), 1);
        assert!(state.pending_update_count() <= 2);
        let _ = crowd;
    }

    #[test]
    fn domain_eligibility_hook_can_suppress_health() {
        let allow = domain_eligibility_for(
            ObserverRelationKind::NearbyStranger,
            ReplicationDirtyMask {
                transform: true,
                health: true,
                ..ReplicationDirtyMask::default()
            },
        );
        assert!(allow.transform);
        assert!(!allow.health);
        assert!(allow.any());
    }

    #[test]
    fn selective_policy_suppresses_stranger_health_on_emit() {
        let (mut world, observer, remote) = two_players();
        // Ensure remote has health so domain can dirty.
        let _ = world.set_health(remote, purgatory_simulation::Health::full(100.0));
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        let overrides = RelationOverrides::default();
        let _ = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();
        let _ = world.set_health(
            remote,
            purgatory_simulation::Health {
                current: 50.0,
                max: 100.0,
            },
        );
        let _ = distribute_replication_dirty(&world, &fanout, |obs, subject, _| {
            if obs == observer {
                state.queue_pending_update(subject);
            }
        });
        world.clear_replication_dirty();
        let policy = PublishPolicyInput {
            mode: PolicyMode::Selective,
            population: PopulationClass::High,
            overrides: &overrides,
            recent_observer_bytes: 3000,
            tick_overrun_hint: false,
        };
        let stats = publish_observer_frame(
            &mut state,
            &pipe,
            &mut world,
            &mut fanout,
            observer,
            2,
            2,
            0,
            0,
            0,
            policy,
        );
        assert!(stats.policy_domain_suppressed >= 1 || stats.updates == 0);
        if let Some(frame) = pipe.pop() {
            let decoded = decode_replication_frame(&frame.payload).unwrap();
            for rec in &decoded.records {
                if let ReplicationRecord::Update {
                    entity_id,
                    domains,
                    health,
                    ..
                } = rec
                    && *entity_id == to_wire_id(remote)
                {
                    assert!(!domains.health, "stranger health must be policy-suppressed");
                    assert!(health.is_none());
                }
            }
        }
    }

    #[test]
    fn selective_policy_emits_lethal_health_to_existing_observer() {
        let (mut world, observer, remote) = two_players();
        let _ = world.set_health(remote, purgatory_simulation::Health::full(100.0));
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        let overrides = RelationOverrides::default();
        let _ = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();

        assert!(world.apply_damage(remote, 100.0));
        let _ = distribute_replication_dirty(&world, &fanout, |obs, subject, _| {
            if obs == observer {
                state.queue_pending_update(subject);
            }
        });
        world.clear_replication_dirty();
        let stats = publish_observer_frame(
            &mut state,
            &pipe,
            &mut world,
            &mut fanout,
            observer,
            2,
            2,
            0,
            0,
            0,
            PublishPolicyInput {
                mode: PolicyMode::Selective,
                population: PopulationClass::High,
                overrides: &overrides,
                recent_observer_bytes: 3000,
                tick_overrun_hint: false,
            },
        );
        assert_eq!(stats.policy_domain_suppressed, 0);
        let frame = decode_replication_frame(&pipe.pop().expect("lethal update").payload).unwrap();
        assert!(frame.records.iter().any(|record| matches!(
            record,
            ReplicationRecord::Update {
                entity_id,
                domains,
                health: Some(ReplicatedHealth { current, .. }),
                ..
            } if *entity_id == to_wire_id(remote) && domains.health && *current <= 0.0
        )));
    }

    #[test]
    fn selective_policy_emits_respawn_health_to_existing_observer() {
        let (mut world, observer, remote) = two_players();
        let _ = world.set_health(remote, purgatory_simulation::Health::full(100.0));
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        let overrides = RelationOverrides::default();
        let _ = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();

        assert!(world.apply_damage(remote, 100.0));
        let _ = distribute_replication_dirty(&world, &fanout, |obs, subject, _| {
            if obs == observer {
                state.queue_pending_update(subject);
            }
        });
        world.clear_replication_dirty();
        let _ = publish_observer_frame(
            &mut state,
            &pipe,
            &mut world,
            &mut fanout,
            observer,
            2,
            2,
            0,
            0,
            0,
            PublishPolicyInput {
                mode: PolicyMode::Selective,
                population: PopulationClass::High,
                overrides: &overrides,
                recent_observer_bytes: 3000,
                tick_overrun_hint: false,
            },
        );
        let dead = decode_replication_frame(&pipe.pop().expect("dead update").payload).unwrap();
        assert!(dead.records.iter().any(|record| matches!(
            record,
            ReplicationRecord::Update {
                entity_id,
                domains,
                health: Some(ReplicatedHealth { current, .. }),
                ..
            } if *entity_id == to_wire_id(remote) && domains.health && *current <= 0.0
        )));

        assert!(world.respawn_player_entity(remote));
        let _ = distribute_replication_dirty(&world, &fanout, |obs, subject, _| {
            if obs == observer {
                state.queue_pending_update(subject);
            }
        });
        world.clear_replication_dirty();
        let _ = publish_observer_frame(
            &mut state,
            &pipe,
            &mut world,
            &mut fanout,
            observer,
            3,
            3,
            0,
            0,
            0,
            PublishPolicyInput {
                mode: PolicyMode::Selective,
                population: PopulationClass::High,
                overrides: &overrides,
                recent_observer_bytes: 3000,
                tick_overrun_hint: false,
            },
        );
        let alive = decode_replication_frame(&pipe.pop().expect("respawn update").payload).unwrap();
        assert!(alive.records.iter().any(|record| matches!(
            record,
            ReplicationRecord::Update {
                entity_id,
                domains,
                health: Some(ReplicatedHealth { current, .. }),
                ..
            } if *entity_id == to_wire_id(remote) && domains.health && *current > 0.0
        )));
    }

    #[test]
    fn dead_health_is_in_late_baseline_and_epoch_resync() {
        let (mut world, observer, remote) = two_players();
        let _ = world.set_health(remote, purgatory_simulation::Health::full(100.0));
        assert!(world.apply_damage(remote, 100.0));
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        let overrides = RelationOverrides::default();
        let policy = PublishPolicyInput::baseline(&overrides);

        publish_observer_frame(
            &mut state,
            &pipe,
            &mut world,
            &mut fanout,
            observer,
            1,
            1,
            0,
            0,
            0,
            policy,
        );
        let late_baseline =
            decode_replication_frame(&pipe.pop().expect("late baseline").payload).unwrap();
        assert!(late_baseline.records.iter().any(|record| matches!(
            record,
            ReplicationRecord::Enter {
                entity,
                health: Some(ReplicatedHealth { current, .. }),
                ..
            } if entity.entity_id == to_wire_id(remote) && *current <= 0.0
        )));

        state.bump_epoch();
        pipe.purge_older_than(state.epoch);
        publish_observer_frame(
            &mut state,
            &pipe,
            &mut world,
            &mut fanout,
            observer,
            2,
            2,
            0,
            0,
            0,
            policy,
        );
        let resync = decode_replication_frame(&pipe.pop().expect("epoch resync").payload).unwrap();
        assert_eq!(resync.observer_baseline_epoch, state.epoch);
        assert!(resync.records.iter().any(|record| matches!(
            record,
            ReplicationRecord::Enter {
                entity,
                health: Some(ReplicatedHealth { current, .. }),
                ..
            } if entity.entity_id == to_wire_id(remote) && *current <= 0.0
        )));
    }

    #[test]
    fn priority_prefers_self_under_tiny_budget() {
        let (mut world, observer, remote) = two_players();
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        let overrides = RelationOverrides::default();
        let _ = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();
        let mut t = world.transform_of(observer).unwrap();
        t.position[0] += 0.2;
        world.set_transform(observer, t);
        let mut t = world.transform_of(remote).unwrap();
        t.position[0] += 0.2;
        world.set_transform(remote, t);
        let _ = distribute_replication_dirty(&world, &fanout, |obs, subject, _| {
            if obs == observer {
                state.queue_pending_update(subject);
            }
        });
        world.clear_replication_dirty();
        // Tiny budget: header + one update only.
        let empty_len = {
            let empty = header_frame(1, 1, observer, &world, 0, 0, 0, 0, Vec::new());
            encoded_len(&empty).unwrap()
        };
        let policy = PublishPolicyInput::baseline(&overrides);
        let stats = publish_observer_frame_with_budget(
            &mut state,
            &pipe,
            &mut world,
            &mut fanout,
            observer,
            2,
            2,
            0,
            0,
            0,
            empty_len + 40,
            policy,
        );
        assert!(stats.updates >= 1);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        let first_update = frame.records.iter().find_map(|r| match r {
            ReplicationRecord::Update { entity_id, .. } => Some(*entity_id),
            _ => None,
        });
        assert_eq!(
            first_update,
            Some(to_wire_id(observer)),
            "self should win priority under budget pressure"
        );
    }

    #[test]
    fn enter_carries_current_full_equipment() {
        let (mut world, observer, remote) = two_players();
        let sword = ContentId::from_token(42);
        assert!(world.set_equipment_slot(remote, EquipmentSlot::Weapon, Some(sword)));
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        let rec = frame.records.iter().find(|r| {
            matches!(
                r,
                ReplicationRecord::Enter { entity, .. } if entity.entity_id == to_wire_id(remote)
            )
        });
        let Some(ReplicationRecord::Enter {
            equipment: Some(eq),
            ..
        }) = rec
        else {
            panic!("expected Enter with equipment domain, got {rec:?}");
        };
        assert_eq!(eq.get(EquipmentSlot::Weapon as u8), Some(sword));
        assert!(eq.get(EquipmentSlot::Headwear as u8).is_none());
    }

    #[test]
    fn equipment_mutation_emits_slot_delta_not_full_resend() {
        let (mut world, observer, remote) = two_players();
        let sword = ContentId::from_token(42);
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();
        assert!(world.set_equipment_slot(remote, EquipmentSlot::Weapon, Some(sword)));
        let stats = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 2, 2);
        assert!(stats.updates >= 1);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        let update = frame.records.iter().find(|r| {
            matches!(
                r,
                ReplicationRecord::Update { entity_id, .. } if *entity_id == to_wire_id(remote)
            )
        });
        let Some(ReplicationRecord::Update {
            domains,
            equipment: Some(delta),
            position,
            health,
            ..
        }) = update
        else {
            panic!("expected equipment Update, got {update:?}");
        };
        assert!(domains.equipment);
        assert!(!domains.health);
        assert!(health.is_none());
        assert!(position.is_none() || !domains.transform);
        assert_eq!(
            delta.mask() & EquipmentSlot::Weapon.bit(),
            EquipmentSlot::Weapon.bit()
        );
        assert_eq!(delta.get(EquipmentSlot::Weapon as u8), Some(Some(sword)));
        assert!(delta.get(EquipmentSlot::Headwear as u8).is_none());
    }

    #[test]
    fn stable_equipment_emits_no_update() {
        let (mut world, observer, remote) = two_players();
        let sword = ContentId::from_token(42);
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();
        assert!(world.set_equipment_slot(remote, EquipmentSlot::Weapon, Some(sword)));
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 2, 2);
        let _ = pipe.pop();
        let stats = publish(&mut state, &pipe, &mut world, &mut fanout, observer, 3, 3);
        assert_eq!(stats.updates, 0);
        assert_eq!(stats.pending_updates, 0);
        if let Some(queued) = pipe.pop() {
            let frame = decode_replication_frame(&queued.payload).unwrap();
            assert!(!frame.records.iter().any(|r| matches!(
                r,
                ReplicationRecord::Update { domains, .. } if domains.equipment
            )));
        }
    }

    #[test]
    fn leave_reenter_reconstructs_equipment_from_baseline() {
        let (mut world, observer, remote) = two_players();
        let sword = ContentId::from_token(99);
        assert!(world.set_equipment_slot(remote, EquipmentSlot::Weapon, Some(sword)));
        let (pipe, _rx) = ReplicationPipe::new();
        let mut state = ObserverReplicationState::new();
        let mut fanout = InterestFanoutIndex::new();
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 1, 1);
        let _ = pipe.pop();
        let mut t = world.transform_of(observer).unwrap();
        t.position[0] += 200.0;
        world.set_transform(observer, t);
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 2, 2);
        let _ = pipe.pop();
        t.position[0] -= 200.0;
        world.set_transform(observer, t);
        publish(&mut state, &pipe, &mut world, &mut fanout, observer, 3, 3);
        let frame = decode_replication_frame(&pipe.pop().unwrap().payload).unwrap();
        let rec = frame.records.iter().find(|r| {
            matches!(
                r,
                ReplicationRecord::Enter { entity, .. } if entity.entity_id == to_wire_id(remote)
            )
        });
        let Some(ReplicationRecord::Enter {
            equipment: Some(eq),
            ..
        }) = rec
        else {
            panic!("re-enter must carry current equipment, got {rec:?}");
        };
        assert_eq!(eq.get(EquipmentSlot::Weapon as u8), Some(sword));
    }
}
