//! Authoritative world: generational slot storage for runtime entities.
//!
//! Not an ECS. Not a global registry. One [`World`] owns its entities.
//! [`EntityId`] is index + generation; reusing a slot never resurrects a
//! stale ID. Pointers and memory addresses are not used as identity.

use std::collections::{HashMap, HashSet};

use crate::aabb::Aabb;
use crate::ability::{AbilityGrantTable, AbilityRuntimeTable, CooldownTable};
use crate::action::ActionTable;
use crate::aoi::{AOI_INFLUENCE_HALF_EXTENTS, AoiRects, aoi_policy_rects, point_in_aabb};
use crate::body::{PlayerBody, PlayerState};
use crate::bounds::WorldBounds;
use crate::cadence::CadenceTable;
use crate::dirty::DirtyFlags;
use crate::domain::{DomainRevs, ReplicationDirtyMask};
use crate::effect::EffectTable;
use crate::entity::{EntityId, EntityKind};
use crate::equipment::{EquipmentDirtyMask, EquipmentSlot, EquipmentState};
use crate::health::Health;
use crate::interactable::{
    INTERACT_RANGE, Interactable, InteractableKind, in_portal_activation_zone,
};
use crate::interaction::{
    InteractionCloseReason, InteractionReject, InteractionSession, InteractionSessionId,
    InteractionSessionState,
};
use crate::interest_locality::InterestLocalityAccounting;
use crate::lifecycle::EntityLifecycle;
use crate::map_runtime::InstantiatedMap;
use crate::motion_debug::PlayerMotionDebug;
use crate::npc::NpcState;
use crate::platform::{
    FLOOR, FLOOR_POSITION, ONEWAY_A, ONEWAY_A_POSITION, ONEWAY_B, ONEWAY_B_POSITION, Platform,
    PlatformView, RAISED_PLATFORM, RAISED_PLATFORM_POSITION,
};
use crate::presentation_oneshot::{
    PresentationOneShot, PresentationOneShotError, PresentationOneShotKind, oneshot_if_active,
    try_start_oneshot,
};
use crate::replication::{ReplicationClass, ReplicationMeta};
use crate::runtime_event::EventQueue;
use crate::runtime_stats::RuntimeStats;
use crate::scheduler::Scheduler;
use crate::spatial::SpatialIndex;
use crate::spawn::RuntimeSpawnRequest;
use crate::spawn_schedule::SpawnSchedule;
use crate::time::SimulationTick;
use crate::transform::Transform;
use purgatory_common::{ContentId, PersistentId, WorldAddress};

struct Slot {
    generation: u32,
    data: Option<EntityData>,
}

pub(crate) struct EntityData {
    transform: Option<Transform>,
    address: WorldAddress,
    lifecycle: EntityLifecycle,
    content_id: Option<ContentId>,
    persistent_id: Option<PersistentId>,
    replication: ReplicationMeta,
    player: Option<PlayerState>,
    platform: Option<Platform>,
    health: Option<Health>,
    pub(crate) damage_immunity_until: Option<SimulationTick>,
    interactable: Option<Interactable>,
    npc: Option<NpcState>,
    equipment: Option<EquipmentState>,
    equipment_dirty: EquipmentDirtyMask,
    /// Authoritative Attack/Hurt presentation oneshot (A5). Not replicated as bones.
    presentation_oneshot: Option<PresentationOneShot>,
    dirty: DirtyFlags,
    domain_revs: DomainRevs,
}

impl EntityData {
    fn kind(&self) -> EntityKind {
        if self.player.is_some() {
            EntityKind::Player
        } else if self.platform.is_some() {
            EntityKind::Platform
        } else {
            EntityKind::Generic
        }
    }
}

/// Simulation container. Owns entity lifecycle.
pub struct World {
    slots: Vec<Slot>,
    free: Vec<u32>,
    live: u32,
    player: Option<EntityId>,
    bounds: WorldBounds,
    last_motion: PlayerMotionDebug,
    footnote_config: crate::footnote::FootnoteConfig,
    next_support_id: u16,
    interaction_sessions: Vec<InteractionSession>,
    next_interaction_session_id: u32,
    pub(crate) instantiated: HashMap<WorldAddress, InstantiatedMap>,
    portal_reentry: HashMap<EntityId, EntityId>,
    spatial: SpatialIndex,
    /// Observers (players) whose AOI classify is stale (6G.5 incremental dirty).
    interest_dirty_observers: HashSet<EntityId>,
    /// 6G.6 characterization: observers dirtied per invalidation.
    interest_locality: InterestLocalityAccounting,
    /// Entities with transform/health/equipment domain bumps since last drain (6G.7B).
    replication_dirty: HashMap<EntityId, ReplicationDirtyMask>,
    pub(crate) tick: SimulationTick,
    pub(crate) scheduler: Scheduler,
    pub(crate) actions: ActionTable,
    pub(crate) ability_runtime: AbilityRuntimeTable,
    pub(crate) cooldowns: CooldownTable,
    pub(crate) ability_grants: AbilityGrantTable,
    pub(crate) effects: EffectTable,
    pub(crate) events: EventQueue,
    pub(crate) spawn_schedule: SpawnSchedule,
    pub(crate) cadence: CadenceTable,
    pub(crate) runtime_stats: RuntimeStats,
    /// Load/workload override for NPC death→respawn delay. `0` = simulation default (30).
    pub(crate) npc_respawn_delay_ticks: u64,
}

impl Default for World {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
            player: None,
            bounds: WorldBounds::DEV_COMPACT,
            last_motion: PlayerMotionDebug::default(),
            footnote_config: crate::footnote::FootnoteConfig::DEFAULT,
            next_support_id: 1,
            interaction_sessions: Vec::new(),
            next_interaction_session_id: 1,
            instantiated: HashMap::new(),
            portal_reentry: HashMap::new(),
            spatial: SpatialIndex::default(),
            interest_dirty_observers: HashSet::new(),
            interest_locality: InterestLocalityAccounting::default(),
            replication_dirty: HashMap::new(),
            tick: SimulationTick::ZERO,
            scheduler: Scheduler::new(),
            actions: ActionTable::new(),
            ability_runtime: AbilityRuntimeTable::new(),
            cooldowns: CooldownTable::new(),
            ability_grants: AbilityGrantTable::new(),
            effects: EffectTable::new(),
            events: EventQueue::new(),
            spawn_schedule: SpawnSchedule::new(),
            cadence: CadenceTable::new(),
            runtime_stats: RuntimeStats::default(),
            npc_respawn_delay_ticks: 0,
        }
    }
}

impl World {
    /// True when this observer's AOI membership may be stale (6G.5).
    #[must_use]
    pub fn interest_observer_dirty(&self, observer: EntityId) -> bool {
        self.interest_dirty_observers.contains(&observer)
    }

    /// Clear dirty after a successful classify for `observer`.
    pub fn clear_interest_observer_dirty(&mut self, observer: EntityId) {
        self.interest_dirty_observers.remove(&observer);
    }

    /// Mark a single observer dirty (e.g. first bind / tests).
    pub fn mark_interest_observer_dirty(&mut self, observer: EntityId) {
        if self.kind(observer) == Some(EntityKind::Player) {
            self.interest_dirty_observers.insert(observer);
        }
    }

    /// Record a transform/health/equipment domain change for replication fan-out (6G.7B).
    pub fn mark_replication_dirty(&mut self, id: EntityId, mask: ReplicationDirtyMask) {
        if !mask.any() || !self.contains(id) {
            return;
        }
        self.replication_dirty.entry(id).or_default().merge(mask);
    }

    /// Drain pending replication dirty set (consumed once per publish pass).
    #[must_use]
    pub fn drain_replication_dirty(&mut self) -> HashMap<EntityId, ReplicationDirtyMask> {
        std::mem::take(&mut self.replication_dirty)
    }

    /// Peek pending replication dirty without consuming.
    pub fn replication_dirty_iter(
        &self,
    ) -> impl Iterator<Item = (EntityId, ReplicationDirtyMask)> + '_ {
        self.replication_dirty.iter().map(|(id, mask)| (*id, *mask))
    }

    /// Clear pending replication dirty after fan-out enqueue completes.
    pub fn clear_replication_dirty(&mut self) {
        self.replication_dirty.clear();
    }

    /// Peek dirty count without draining (tests / diagnostics).
    #[must_use]
    pub fn replication_dirty_len(&self) -> usize {
        self.replication_dirty.len()
    }

    /// How many observers are currently marked dirty.
    #[must_use]
    pub fn interest_dirty_observer_count(&self) -> usize {
        self.interest_dirty_observers.len()
    }

    /// Snapshot of dirty observer ids (sorted for tests).
    #[must_use]
    pub fn interest_dirty_observers_sorted(&self) -> Vec<EntityId> {
        let mut ids: Vec<EntityId> = self.interest_dirty_observers.iter().copied().collect();
        ids.sort_by_key(|id| (id.index(), id.generation()));
        ids
    }

    /// Snapshot interest-invalidation locality counters (6G.6).
    #[must_use]
    pub fn interest_locality_snapshot(&self) -> purgatory_common::InterestLocalitySnapshot {
        self.interest_locality.snapshot()
    }

    /// Reset locality accounting (tests / run boundaries).
    pub fn reset_interest_locality(&mut self) {
        self.interest_locality.reset();
    }

    /// Invalidate observers for a pose pair (6G.7A: enter/leave XOR after influence prefilter).
    fn invalidate_interest_motion(
        &mut self,
        address: WorldAddress,
        old: [f32; 2],
        new: [f32; 2],
        subject: Option<EntityId>,
    ) {
        let half = AOI_INFLUENCE_HALF_EXTENTS;
        let cell_size = self.spatial.cell_size();
        let c0 = crate::spatial::cell_of(old, cell_size);
        let c1 = crate::spatial::cell_of(new, cell_size);
        let cell_crossed = c0 != c1;
        let cells_touched = if cell_crossed { 2 } else { 1 };

        let min_x = old[0].min(new[0]) - half[0];
        let min_y = old[1].min(new[1]) - half[1];
        let max_x = old[0].max(new[0]) + half[0];
        let max_y = old[1].max(new[1]) + half[1];
        let aabb = Aabb::from_min_max(min_x, min_y, max_x, max_y);
        let found = self.spatial.query_aabb_unsorted(address, aabb);
        let bounds = self.bounds_for(address);

        let mut prefilter = 0u32;
        let mut xor_hits = 0u32;
        let mut subject_marked = false;

        for id in found {
            if self.kind(id) != Some(EntityKind::Player) {
                continue;
            }
            prefilter = prefilter.saturating_add(1);
            if Some(id) == subject {
                self.interest_dirty_observers.insert(id);
                subject_marked = true;
                continue;
            }
            let Some(obs_pos) = self.transform_of(id).map(|t| t.position) else {
                continue;
            };
            let rects = aoi_policy_rects(obs_pos, bounds);
            let old_enter = point_in_aabb(old, rects.enter);
            let new_enter = point_in_aabb(new, rects.enter);
            let old_leave = point_in_aabb(old, rects.leave);
            let new_leave = point_in_aabb(new, rects.leave);
            if old_enter != new_enter || old_leave != new_leave {
                xor_hits = xor_hits.saturating_add(1);
                self.interest_dirty_observers.insert(id);
            }
        }

        if let Some(id) = subject
            && self.kind(id) == Some(EntityKind::Player)
        {
            if !subject_marked {
                prefilter = prefilter.saturating_add(1);
            }
            self.interest_dirty_observers.insert(id);
            subject_marked = true;
        }

        let marked = xor_hits.saturating_add(u32::from(subject_marked));
        self.interest_locality.record_invalidation(
            marked,
            cell_crossed,
            cells_touched,
            true,
            prefilter,
            xor_hits,
        );
    }

    /// Appear/disappear/class at `pos`: dirty observers that could gain/lose the entity.
    fn invalidate_interest_presence(
        &mut self,
        address: WorldAddress,
        pos: [f32; 2],
        subject: Option<EntityId>,
    ) {
        let half = AOI_INFLUENCE_HALF_EXTENTS;
        let aabb = Aabb::from_min_max(
            pos[0] - half[0],
            pos[1] - half[1],
            pos[0] + half[0],
            pos[1] + half[1],
        );
        let found = self.spatial.query_aabb_unsorted(address, aabb);
        let bounds = self.bounds_for(address);
        let cell = crate::spatial::cell_of(pos, self.spatial.cell_size());
        let _ = cell;

        let mut prefilter = 0u32;
        let mut hits = 0u32;
        let mut subject_seen = false;
        for id in found {
            if self.kind(id) != Some(EntityKind::Player) {
                continue;
            }
            prefilter = prefilter.saturating_add(1);
            if Some(id) == subject {
                subject_seen = true;
                self.interest_dirty_observers.insert(id);
                hits = hits.saturating_add(1);
                continue;
            }
            let Some(obs_pos) = self.transform_of(id).map(|t| t.position) else {
                continue;
            };
            let rects = aoi_policy_rects(obs_pos, bounds);
            if point_in_aabb(pos, rects.enter) || point_in_aabb(pos, rects.leave) {
                self.interest_dirty_observers.insert(id);
                hits = hits.saturating_add(1);
            }
        }
        if let Some(id) = subject
            && self.kind(id) == Some(EntityKind::Player)
            && !subject_seen
        {
            prefilter = prefilter.saturating_add(1);
            self.interest_dirty_observers.insert(id);
            hits = hits.saturating_add(1);
        }
        self.interest_locality
            .record_invalidation(hits, false, 1, true, prefilter, hits);
    }

    /// Invalidate around an entity's current address/pose (class/meta changes).
    fn invalidate_interest_entity(&mut self, id: EntityId) {
        let Some(data) = self.slot_live(id) else {
            return;
        };
        let address = data.address;
        let pos = data.transform.map(|t| t.position);
        if let Some(p) = pos {
            self.invalidate_interest_presence(address, p, Some(id));
        } else {
            self.mark_interest_observer_dirty(id);
        }
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn bounds(&self) -> WorldBounds {
        self.bounds
    }

    #[must_use]
    pub fn bounds_for(&self, address: WorldAddress) -> WorldBounds {
        self.instantiated
            .get(&address)
            .map(|m| m.bounds)
            .unwrap_or(self.bounds)
    }

    pub fn set_bounds(&mut self, bounds: WorldBounds) {
        self.bounds = bounds;
    }

    /// Last FOOTNOTE tick motion diagnostics (development discontinuity detector).
    #[must_use]
    pub fn last_motion_debug(&self) -> PlayerMotionDebug {
        self.last_motion
    }

    pub(crate) fn set_last_motion_debug(&mut self, debug: PlayerMotionDebug) {
        self.last_motion = debug;
    }

    /// FOOTNOTE locomotion used by [`Self::tick`] / [`Self::tick_predicted_player`].
    #[must_use]
    pub fn footnote_config(&self) -> crate::footnote::FootnoteConfig {
        self.footnote_config
    }

    /// Replace FOOTNOTE locomotion for subsequent ticks. Tests and DEV overlay only.
    pub fn set_footnote_config(&mut self, config: crate::footnote::FootnoteConfig) {
        self.footnote_config = config;
    }

    /// Phase-4.5 compact development stage (unit tests / small sandbox).
    #[must_use]
    pub fn dev_stage() -> Self {
        let mut world = Self::new();
        world.set_bounds(WorldBounds::DEV_COMPACT);
        let floor = world.spawn_platform(Transform::from_position(FLOOR_POSITION), FLOOR);
        let _raised = world.spawn_platform(
            Transform::from_position(RAISED_PLATFORM_POSITION),
            RAISED_PLATFORM,
        );
        let _oneway_a = world.spawn_platform(Transform::from_position(ONEWAY_A_POSITION), ONEWAY_A);
        let _oneway_b = world.spawn_platform(Transform::from_position(ONEWAY_B_POSITION), ONEWAY_B);
        let floor_top = FLOOR.top_surface(Transform::from_position(FLOOR_POSITION));
        let (transform, player) = PlayerState::standing_on(floor, floor_top);
        world.spawn_player(transform, player);
        world
    }

    #[must_use]
    pub fn len(&self) -> u32 {
        self.live
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    #[must_use]
    pub fn contains(&self, id: EntityId) -> bool {
        self.slot_live(id).is_some()
    }

    #[must_use]
    pub fn kind(&self, id: EntityId) -> Option<EntityKind> {
        Some(self.slot_live(id)?.kind())
    }

    #[must_use]
    pub fn address_of(&self, id: EntityId) -> Option<WorldAddress> {
        Some(self.slot_live(id)?.address)
    }

    /// Explicit membership change. Does not despawn. Re-enters [`EntityLifecycle::Active`].
    ///
    /// Relocates the spatial grid. Closes world-bound [`InteractionSession`]
    /// rows involving `id` with [`InteractionCloseReason::AddressChanged`].
    /// That is not a generic session closer: identity/social sessions must not
    /// share this path.
    pub fn set_address(&mut self, id: EntityId, address: WorldAddress) -> bool {
        let old = self
            .slot_live(id)
            .map(|d| (d.address, d.transform.map(|t| t.position)));
        let mut bumped = false;
        {
            let Some(data) = self.slot_live_mut(id) else {
                return false;
            };
            if data.address != address {
                data.domain_revs.bump_membership();
                bumped = true;
            }
            data.address = address;
            data.lifecycle = EntityLifecycle::Active;
            data.dirty.membership = true;
        }
        if bumped {
            self.note_domain_rev();
            if let Some((old_addr, Some(pos))) = old {
                self.spatial.relocate(id, address, pos);
                if old_addr != address {
                    self.invalidate_interest_presence(old_addr, pos, Some(id));
                }
                self.invalidate_interest_presence(address, pos, Some(id));
            } else {
                self.mark_interest_observer_dirty(id);
            }
        } else if let Some((_, Some(pos))) = old {
            self.spatial.relocate(id, address, pos);
        }
        self.close_sessions_involving(id, InteractionCloseReason::AddressChanged);
        true
    }

    /// Move every live entity (and the instantiated-map record) from `from` to `to`.
    ///
    /// Same-Map Channel/Instance retarget for client presentation. Does not
    /// despawn. Returns false if `to` is already instantiated or maps differ.
    pub fn rebind_map_address(&mut self, from: WorldAddress, to: WorldAddress) -> bool {
        if from == to {
            return true;
        }
        if from.map != to.map {
            return false;
        }
        if self.instantiated.contains_key(&to) {
            return false;
        }
        if let Some(mut record) = self.instantiated.remove(&from) {
            record.address = to;
            self.instantiated.insert(to, record);
        }
        let ids: Vec<EntityId> = self
            .iter()
            .filter(|&id| self.address_of(id) == Some(from))
            .collect();
        for id in ids {
            self.set_address(id, to);
        }
        true
    }

    #[must_use]
    pub fn lifecycle_of(&self, id: EntityId) -> Option<EntityLifecycle> {
        Some(self.slot_live(id)?.lifecycle)
    }

    /// Leave world membership without destroying the slot. Not a despawn.
    pub fn leave_world(&mut self, id: EntityId) -> bool {
        let prev = self
            .slot_live(id)
            .map(|d| (d.address, d.transform.map(|t| t.position)));
        {
            let Some(data) = self.slot_live_mut(id) else {
                return false;
            };
            data.lifecycle = EntityLifecycle::LeftWorld;
            data.dirty.membership = true;
            data.domain_revs.bump_membership();
        }
        self.note_domain_rev();
        self.spatial.remove(id);
        if let Some((addr, Some(pos))) = prev {
            self.invalidate_interest_presence(addr, pos, Some(id));
        } else {
            self.mark_interest_observer_dirty(id);
        }
        self.close_sessions_involving(id, InteractionCloseReason::AddressChanged);
        true
    }

    #[must_use]
    pub fn content_id_of(&self, id: EntityId) -> Option<ContentId> {
        self.slot_live(id)?.content_id
    }

    /// Content identity is independent of spawn/despawn of the runtime slot.
    pub fn set_content_id(&mut self, id: EntityId, content_id: Option<ContentId>) -> bool {
        let Some(data) = self.slot_live_mut(id) else {
            return false;
        };
        data.content_id = content_id;
        true
    }

    #[must_use]
    pub fn persistent_id_of(&self, id: EntityId) -> Option<PersistentId> {
        self.slot_live(id)?.persistent_id
    }

    pub fn set_persistent_id(&mut self, id: EntityId, persistent_id: Option<PersistentId>) -> bool {
        let Some(data) = self.slot_live_mut(id) else {
            return false;
        };
        data.persistent_id = persistent_id;
        true
    }

    #[must_use]
    pub fn replication_of(&self, id: EntityId) -> Option<ReplicationMeta> {
        Some(self.slot_live(id)?.replication)
    }

    pub fn set_replication(&mut self, id: EntityId, replication: ReplicationMeta) -> bool {
        let mut bumped = false;
        {
            let Some(data) = self.slot_live_mut(id) else {
                return false;
            };
            if data.replication != replication {
                data.domain_revs.bump_replication();
                bumped = true;
            }
            data.replication = replication;
            data.dirty.replication = true;
        }
        if bumped {
            self.note_domain_rev();
            self.invalidate_interest_entity(id);
        }
        true
    }

    #[must_use]
    pub fn transform_of(&self, id: EntityId) -> Option<Transform> {
        self.slot_live(id)?.transform
    }

    pub fn set_transform(&mut self, id: EntityId, transform: Transform) -> bool {
        let prev = self
            .slot_live(id)
            .and_then(|d| d.transform.map(|t| t.position));
        let address = self.slot_live(id).map(|d| d.address);
        let lifecycle = self.slot_live(id).map(|d| d.lifecycle);
        let mut bumped = false;
        {
            let Some(data) = self.slot_live_mut(id) else {
                return false;
            };
            if prev != Some(transform.position) {
                data.domain_revs.bump_transform();
                bumped = true;
            }
            data.transform = Some(transform);
            data.dirty.transform = true;
        }
        if bumped {
            self.note_domain_rev();
            self.mark_replication_dirty(id, ReplicationDirtyMask::transform_only());
            let old = prev.unwrap_or(transform.position);
            if let Some(address) = address {
                self.invalidate_interest_motion(address, old, transform.position, Some(id));
            } else {
                self.mark_interest_observer_dirty(id);
            }
        }
        if lifecycle == Some(EntityLifecycle::Active)
            && let Some(address) = address
        {
            self.spatial.relocate(id, address, transform.position);
        }
        true
    }

    pub fn clear_transform(&mut self, id: EntityId) -> bool {
        let prev = self
            .slot_live(id)
            .map(|d| (d.address, d.transform.map(|t| t.position)));
        let mut bumped = false;
        {
            let Some(data) = self.slot_live_mut(id) else {
                return false;
            };
            if data.transform.is_some() {
                data.domain_revs.bump_transform();
                bumped = true;
            }
            data.transform = None;
            data.dirty.transform = true;
        }
        if bumped {
            self.note_domain_rev();
            self.mark_replication_dirty(id, ReplicationDirtyMask::transform_only());
        }
        self.spatial.remove(id);
        if bumped {
            if let Some((addr, Some(pos))) = prev {
                self.invalidate_interest_presence(addr, pos, Some(id));
            } else {
                self.mark_interest_observer_dirty(id);
            }
        }
        true
    }

    #[must_use]
    pub fn health_of(&self, id: EntityId) -> Option<Health> {
        self.slot_live(id)?.health
    }

    #[must_use]
    pub fn damage_immunity_active(&self, id: EntityId) -> bool {
        self.slot_live(id)
            .and_then(|data| data.damage_immunity_until)
            .is_some_and(|until| self.tick < until)
    }

    pub(crate) fn expire_damage_immunity(&mut self) {
        let expired: Vec<EntityId> = self
            .iter()
            .filter(|&id| {
                self.slot_live(id)
                    .and_then(|data| data.damage_immunity_until)
                    .is_some_and(|until| until <= self.tick)
            })
            .collect();
        for id in expired {
            let mut bumped = false;
            if let Some(data) = self.slot_live_mut(id) {
                data.damage_immunity_until = None;
                data.domain_revs.bump_health();
                bumped = true;
            }
            if bumped {
                self.note_domain_rev();
                self.mark_replication_dirty(id, ReplicationDirtyMask::health_only());
            }
        }
    }

    pub fn set_health(&mut self, id: EntityId, health: Health) -> bool {
        let mut bumped = false;
        {
            let Some(data) = self.slot_live_mut(id) else {
                return false;
            };
            if data.health != Some(health) {
                data.domain_revs.bump_health();
                bumped = true;
            }
            data.health = Some(health);
            data.dirty.health = true;
        }
        if bumped {
            self.note_domain_rev();
            self.mark_replication_dirty(id, ReplicationDirtyMask::health_only());
            self.runtime_stats.health_mutations_total =
                self.runtime_stats.health_mutations_total.saturating_add(1);
        }
        true
    }

    #[must_use]
    pub fn equipment_of(&self, id: EntityId) -> Option<EquipmentState> {
        self.slot_live(id)?.equipment
    }

    #[must_use]
    pub fn equipment_slot(&self, id: EntityId, slot: EquipmentSlot) -> Option<ContentId> {
        self.slot_live(id)?.equipment?.get(slot)
    }

    #[must_use]
    pub fn equipment_dirty_of(&self, id: EntityId) -> Option<EquipmentDirtyMask> {
        Some(self.slot_live(id)?.equipment_dirty)
    }

    /// Consume and reset slot-level equipment dirty bits.
    pub fn consume_equipment_dirty(&mut self, id: EntityId) -> Option<EquipmentDirtyMask> {
        Some(self.slot_live_mut(id)?.equipment_dirty.take())
    }

    /// Write one slot. Idempotent writes do not dirty. [`None`] is empty, not a sentinel id.
    pub fn set_equipment_slot(
        &mut self,
        id: EntityId,
        slot: EquipmentSlot,
        value: Option<ContentId>,
    ) -> bool {
        let mut changed_mask = EquipmentDirtyMask::empty();
        {
            let Some(data) = self.slot_live_mut(id) else {
                return false;
            };
            let current = data.equipment.and_then(|state| state.get(slot));
            if current == value {
                return true;
            }
            let state = data.equipment.get_or_insert_with(EquipmentState::empty);
            let _ = state.set(slot, value);
            data.dirty.equipment = true;
            data.equipment_dirty.mark(slot);
            data.domain_revs.bump_equipment();
            changed_mask.mark(slot);
        }
        self.note_domain_rev();
        self.mark_replication_dirty(id, ReplicationDirtyMask::equipment_only(changed_mask));
        true
    }

    pub fn clear_equipment_slot(&mut self, id: EntityId, slot: EquipmentSlot) -> bool {
        self.set_equipment_slot(id, slot, None)
    }

    /// Active presentation oneshot at the world's current tick, if any.
    #[must_use]
    pub fn presentation_oneshot_of(&self, id: EntityId) -> Option<PresentationOneShot> {
        let data = self.slot_live(id)?;
        oneshot_if_active(data.presentation_oneshot, self.tick)
    }

    /// Start or interrupt a presentation oneshot using A5 policy. Returns the live state.
    pub fn try_start_presentation_oneshot(
        &mut self,
        id: EntityId,
        kind: PresentationOneShotKind,
    ) -> Result<PresentationOneShot, PresentationOneShotError> {
        if !self.contains(id) {
            return Err(PresentationOneShotError::BlockedByHurt);
        }
        let now = self.tick;
        let current = self.slot_live(id).and_then(|d| d.presentation_oneshot);
        let next = try_start_oneshot(current, kind, now)?;
        if let Some(data) = self.slot_live_mut(id) {
            data.presentation_oneshot = Some(next);
        }
        self.events.push(
            crate::runtime_event::RuntimeEvent::PresentationOneShotStarted {
                entity: id,
                kind: next.kind,
                until_tick: next.until_tick.get(),
            },
        );
        Ok(next)
    }

    /// Clear any live Attack/Hurt oneshot. Used when lethal damage takes over as Dead.
    pub fn clear_presentation_oneshot(&mut self, id: EntityId) -> bool {
        let Some(data) = self.slot_live_mut(id) else {
            return false;
        };
        if data.presentation_oneshot.is_none() {
            return false;
        }
        data.presentation_oneshot = None;
        self.events
            .push(crate::runtime_event::RuntimeEvent::PresentationOneShotCleared { entity: id });
        true
    }

    /// Expire finished oneshots. Call from the sim tick loop; clip end is irrelevant.
    pub fn expire_presentation_oneshots(&mut self) {
        let now = self.tick;
        for slot in &mut self.slots {
            let Some(data) = slot.data.as_mut() else {
                continue;
            };
            if oneshot_if_active(data.presentation_oneshot, now).is_none() {
                data.presentation_oneshot = None;
            }
        }
    }

    #[must_use]
    pub fn npc_of(&self, id: EntityId) -> Option<NpcState> {
        self.slot_live(id)?.npc
    }

    pub fn set_npc(&mut self, id: EntityId, npc: NpcState) -> bool {
        let Some(data) = self.slot_live_mut(id) else {
            return false;
        };
        data.npc = Some(npc);
        true
    }

    /// Load/workload override for NPC death→respawn delay ticks. `0` restores default (30).
    pub fn set_npc_respawn_delay_ticks(&mut self, delay: u64) {
        self.npc_respawn_delay_ticks = delay;
    }

    #[must_use]
    pub fn npc_respawn_delay_ticks(&self) -> u64 {
        self.npc_respawn_delay_ticks
    }

    #[must_use]
    pub fn domain_revs_of(&self, id: EntityId) -> Option<DomainRevs> {
        Some(self.slot_live(id)?.domain_revs)
    }

    #[must_use]
    pub fn dirty_of(&self, id: EntityId) -> Option<DirtyFlags> {
        Some(self.slot_live(id)?.dirty)
    }

    /// Consume and reset domain dirty flags. Read-only queries do not call this.
    pub fn consume_dirty(&mut self, id: EntityId) -> Option<DirtyFlags> {
        Some(self.slot_live_mut(id)?.dirty.take())
    }

    /// Spawn from a composition request. Transient entities need no content id.
    pub fn spawn(&mut self, mut request: RuntimeSpawnRequest) -> Option<EntityId> {
        if let Some(platform) = request.platform.as_mut()
            && platform.support_id == 0
        {
            if self.next_support_id == 0 {
                self.next_support_id = 1;
            }
            platform.support_id = self.next_support_id;
            self.next_support_id = match self.next_support_id.checked_add(1) {
                Some(next) if next != 0 => next,
                _ => u16::MAX,
            };
        }
        let is_player = request.player.is_some();
        let id = self.allocate(entity_from_request(request));
        if is_player
            && self
                .player
                .filter(|existing| self.contains(*existing))
                .is_none()
        {
            self.player = Some(id);
        }
        self.note_entity_spawned(id);
        Some(id)
    }

    pub fn movable_in_address(&self, address: WorldAddress) -> impl Iterator<Item = EntityId> + '_ {
        self.entities_at(address)
            .filter(|&id| self.slot_live(id).is_some_and(|d| d.player.is_some()))
    }

    pub fn replicated_in_address(
        &self,
        address: WorldAddress,
    ) -> impl Iterator<Item = EntityId> + '_ {
        self.entities_at(address).filter(|&id| {
            matches!(
                self.replication_of(id).map(|m| m.class),
                Some(ReplicationClass::VisibleObservers | ReplicationClass::OwnerOnly)
            )
        })
    }

    /// Active interactables near `position` in `address`. Range checks require Transform.
    pub fn interactable_near(
        &self,
        address: WorldAddress,
        position: [f32; 2],
        radius: f32,
    ) -> impl Iterator<Item = EntityId> + '_ {
        self.entities_near(address, position, radius)
            .filter(|&id| self.interactable_of(id).is_some())
    }

    #[must_use]
    pub fn interactable_of(&self, id: EntityId) -> Option<Interactable> {
        self.slot_live(id)?.interactable
    }

    pub fn set_interactable(&mut self, id: EntityId, interactable: Option<Interactable>) -> bool {
        {
            let Some(data) = self.slot_live_mut(id) else {
                return false;
            };
            data.interactable = interactable;
            data.dirty.replication = true;
            data.domain_revs.bump_replication();
        }
        self.note_domain_rev();
        true
    }

    #[must_use]
    pub fn interaction_session_of(&self, actor: EntityId) -> Option<InteractionSession> {
        self.interaction_sessions
            .iter()
            .copied()
            .find(|s| s.actor == actor && s.state != InteractionSessionState::Closed)
    }

    /// Authoritative open. Client-supplied distance/validity is ignored.
    pub fn try_open_interaction(
        &mut self,
        actor: EntityId,
        target: EntityId,
    ) -> Result<InteractionSession, InteractionReject> {
        self.validate_interaction(actor, target)?;
        if let Some(existing) = self.interaction_session_of(actor) {
            if existing.target == target {
                let mut session = existing;
                session.state = InteractionSessionState::Updated;
                self.upsert_session(session);
                return Ok(session);
            }
            self.close_interaction(actor, existing.id).map(|_| ()).ok();
        }
        let id = InteractionSessionId(self.next_interaction_session_id);
        self.next_interaction_session_id =
            self.next_interaction_session_id.saturating_add(1).max(1);
        let session = InteractionSession {
            id,
            actor,
            target,
            state: InteractionSessionState::Opened,
        };
        self.interaction_sessions.push(session);
        let mut active = session;
        active.state = InteractionSessionState::Active;
        self.upsert_session(active);
        Ok(session)
    }

    pub fn close_interaction(
        &mut self,
        actor: EntityId,
        session_id: InteractionSessionId,
    ) -> Result<InteractionSession, InteractionReject> {
        let Some(pos) = self
            .interaction_sessions
            .iter()
            .position(|s| s.id == session_id)
        else {
            return Err(InteractionReject::InvalidSession);
        };
        if self.interaction_sessions[pos].actor != actor {
            return Err(InteractionReject::InvalidSession);
        }
        let mut session = self.interaction_sessions.remove(pos);
        session.state = InteractionSessionState::Closed;
        Ok(session)
    }

    /// Close [`InteractionSession`] rows involving `entity` (actor or target).
    ///
    /// World-bound interactables only. Not a catch-all for every player-related
    /// session. Whisper / friends / party / guild must not call this.
    pub fn close_sessions_involving(
        &mut self,
        entity: EntityId,
        reason: InteractionCloseReason,
    ) -> Vec<(InteractionSession, InteractionCloseReason)> {
        let mut closed = Vec::new();
        let mut remain = Vec::new();
        for mut session in self.interaction_sessions.drain(..) {
            if session.actor == entity || session.target == entity {
                session.state = InteractionSessionState::Closed;
                closed.push((session, reason));
            } else {
                remain.push(session);
            }
        }
        self.interaction_sessions = remain;
        closed
    }

    /// Close sessions that failed range/address/lifecycle after a tick.
    pub fn maintain_interaction_sessions(
        &mut self,
    ) -> Vec<(InteractionSession, InteractionCloseReason)> {
        let ids: Vec<_> = self.interaction_sessions.iter().map(|s| s.id).collect();
        let mut closed = Vec::new();
        for sid in ids {
            let Some(session) = self
                .interaction_sessions
                .iter()
                .copied()
                .find(|s| s.id == sid)
            else {
                continue;
            };
            let reason = match self.validate_interaction(session.actor, session.target) {
                Ok(()) => continue,
                Err(InteractionReject::TargetMissing | InteractionReject::StaleId) => {
                    InteractionCloseReason::TargetGone
                }
                Err(InteractionReject::WrongAddress) => InteractionCloseReason::AddressChanged,
                Err(InteractionReject::OutOfRange) => InteractionCloseReason::OutOfRange,
                Err(_) => InteractionCloseReason::TargetGone,
            };
            if let Ok(closed_session) = self.close_interaction(session.actor, sid) {
                closed.push((closed_session, reason));
            }
        }
        closed
    }

    fn validate_interaction(
        &self,
        actor: EntityId,
        target: EntityId,
    ) -> Result<(), InteractionReject> {
        let actor_data = self.slot_live(actor).ok_or(self.classify_missing(actor))?;
        if actor_data.player.is_none() || actor_data.lifecycle != EntityLifecycle::Active {
            return Err(InteractionReject::Unavailable);
        }
        let actor_pos = actor_data
            .transform
            .ok_or(InteractionReject::Unavailable)?
            .position;
        let actor_addr = actor_data.address;
        let target_data = match self.slot_live(target) {
            Some(data) => data,
            None => return Err(self.classify_missing(target)),
        };
        if target_data.lifecycle != EntityLifecycle::Active {
            return Err(InteractionReject::Unavailable);
        }
        if target_data.interactable.is_none() {
            return Err(InteractionReject::NotInteractable);
        }
        if target_data
            .interactable
            .is_some_and(|cap| cap.kind == InteractableKind::Portal)
        {
            return Err(InteractionReject::NotInteractable);
        }
        if !target_data.address.compatible_with(actor_addr) {
            return Err(InteractionReject::WrongAddress);
        }
        let target_pos = target_data
            .transform
            .ok_or(InteractionReject::NotInteractable)?
            .position;
        let dx = actor_pos[0] - target_pos[0];
        let dy = actor_pos[1] - target_pos[1];
        if dx * dx + dy * dy > INTERACT_RANGE * INTERACT_RANGE {
            return Err(InteractionReject::OutOfRange);
        }
        Ok(())
    }

    fn classify_missing(&self, id: EntityId) -> InteractionReject {
        let index = id.index() as usize;
        match self.slots.get(index) {
            Some(slot) if slot.generation != id.generation() || slot.data.is_none() => {
                InteractionReject::StaleId
            }
            Some(_) => InteractionReject::TargetMissing,
            None => InteractionReject::TargetMissing,
        }
    }

    /// Live entity at `address` whose content id matches.
    #[must_use]
    pub fn entity_with_content_at(
        &self,
        address: WorldAddress,
        content: ContentId,
    ) -> Option<EntityId> {
        self.entities_at(address)
            .find(|&id| self.content_id_of(id) == Some(content))
    }

    /// Authoritative portal travel check. Does not move the actor.
    pub fn validate_portal_activate(
        &self,
        actor: EntityId,
        target: EntityId,
    ) -> Result<(), InteractionReject> {
        let actor_data = self.slot_live(actor).ok_or(self.classify_missing(actor))?;
        if actor_data.player.is_none() || actor_data.lifecycle != EntityLifecycle::Active {
            return Err(InteractionReject::Unavailable);
        }
        let actor_pos = actor_data
            .transform
            .ok_or(InteractionReject::Unavailable)?
            .position;
        let actor_addr = actor_data.address;
        let target_data = match self.slot_live(target) {
            Some(data) => data,
            None => return Err(self.classify_missing(target)),
        };
        if target_data.lifecycle != EntityLifecycle::Active {
            return Err(InteractionReject::Unavailable);
        }
        let Some(cap) = target_data.interactable else {
            return Err(InteractionReject::NotInteractable);
        };
        if cap.kind != InteractableKind::Portal {
            return Err(InteractionReject::NotInteractable);
        }
        if !target_data.address.compatible_with(actor_addr) {
            return Err(InteractionReject::WrongAddress);
        }
        let target_pos = target_data
            .transform
            .ok_or(InteractionReject::NotInteractable)?
            .position;
        if !in_portal_activation_zone(actor_pos, target_pos) {
            return Err(InteractionReject::OutOfRange);
        }
        if self.portal_reentry.get(&actor) == Some(&target) {
            return Err(InteractionReject::ReentryLocked);
        }
        Ok(())
    }

    /// After arrival, the destination portal cannot fire until Up is released
    /// (or the actor leaves its activation zone). Zone exit is not required.
    pub fn lock_portal_reentry(&mut self, actor: EntityId, portal: EntityId) {
        self.portal_reentry.insert(actor, portal);
    }

    /// Up released: re-arm the locked destination portal while still standing on it.
    pub fn release_portal_reentry(&mut self, actor: EntityId) -> bool {
        self.portal_reentry.remove(&actor).is_some()
    }

    #[must_use]
    pub fn portal_reentry_locked(&self, actor: EntityId, portal: EntityId) -> bool {
        self.portal_reentry.get(&actor) == Some(&portal)
    }

    /// Drop arrival locks when the actor leaves the locked portal zone, or when
    /// the portal/actor is gone. Does not wait for a new Up press.
    pub fn maintain_portal_reentry(&mut self) {
        let actors: Vec<EntityId> = self.portal_reentry.keys().copied().collect();
        for actor in actors {
            let Some(&portal) = self.portal_reentry.get(&actor) else {
                continue;
            };
            let Some(actor_pos) = self.transform_of(actor).map(|t| t.position) else {
                self.portal_reentry.remove(&actor);
                continue;
            };
            let Some(portal_data) = self.slot_live(portal) else {
                self.portal_reentry.remove(&actor);
                continue;
            };
            let Some(portal_pos) = portal_data.transform.map(|t| t.position) else {
                self.portal_reentry.remove(&actor);
                continue;
            };
            if !in_portal_activation_zone(actor_pos, portal_pos) {
                self.portal_reentry.remove(&actor);
            }
        }
    }

    fn upsert_session(&mut self, session: InteractionSession) {
        if let Some(existing) = self
            .interaction_sessions
            .iter_mut()
            .find(|s| s.id == session.id)
        {
            *existing = session;
        } else {
            self.interaction_sessions.push(session);
        }
    }

    /// Live active entities at `address`, slot-index order.
    pub fn entities_at(&self, address: WorldAddress) -> impl Iterator<Item = EntityId> + '_ {
        self.iter_active().filter(move |&id| {
            self.address_of(id)
                .is_some_and(|a| a.compatible_with(address))
        })
    }

    pub fn entities_in_map(
        &self,
        map: purgatory_common::MapId,
    ) -> impl Iterator<Item = EntityId> + '_ {
        self.iter_active()
            .filter(move |&id| self.address_of(id).is_some_and(|a| a.map == map))
    }

    pub fn entities_in_instance(
        &self,
        instance: purgatory_common::InstanceId,
    ) -> impl Iterator<Item = EntityId> + '_ {
        self.iter_active()
            .filter(move |&id| self.address_of(id).is_some_and(|a| a.instance == instance))
    }

    /// Active entities at `address` whose transform is within `radius`.
    pub fn entities_near(
        &self,
        address: WorldAddress,
        position: [f32; 2],
        radius: f32,
    ) -> impl Iterator<Item = EntityId> + '_ {
        let r2 = radius * radius;
        let candidates = self.spatial.query_radius(address, position, radius);
        candidates.into_iter().filter(move |&id| {
            self.lifecycle_of(id) == Some(EntityLifecycle::Active)
                && self.address_of(id) == Some(address)
                && self.transform_of(id).is_some_and(|t| {
                    let dx = t.position[0] - position[0];
                    let dy = t.position[1] - position[1];
                    dx * dx + dy * dy <= r2
                })
        })
    }

    /// World-owned query: entities whose **points** lie in `aabb` at `address`.
    #[must_use]
    pub fn query_aabb(&self, address: WorldAddress, aabb: crate::aabb::Aabb) -> Vec<EntityId> {
        self.query_aabb_inner(address, aabb, true)
    }

    /// Like [`Self::query_aabb`] but skips the deterministic sort (replication AOI hot path).
    #[must_use]
    pub fn query_aabb_unsorted(
        &self,
        address: WorldAddress,
        aabb: crate::aabb::Aabb,
    ) -> Vec<EntityId> {
        self.query_aabb_inner(address, aabb, false)
    }

    fn query_aabb_inner(
        &self,
        address: WorldAddress,
        aabb: crate::aabb::Aabb,
        sorted: bool,
    ) -> Vec<EntityId> {
        let raw = if sorted {
            self.spatial.query_aabb(address, aabb)
        } else {
            self.spatial.query_aabb_unsorted(address, aabb)
        };
        let mut out: Vec<EntityId> = raw
            .into_iter()
            .filter(|&id| {
                self.lifecycle_of(id) == Some(EntityLifecycle::Active)
                    && self.address_of(id) == Some(address)
                    && self
                        .transform_of(id)
                        .is_some_and(|t| point_in_aabb(t.position, aabb))
            })
            .collect();
        if sorted {
            out.sort_by_key(|id| (id.index(), id.generation()));
        }
        out
    }

    #[must_use]
    pub fn query_point(&self, address: WorldAddress, position: [f32; 2]) -> Vec<EntityId> {
        self.query_aabb(address, crate::aabb::Aabb::new(position, [0.0, 0.0]))
    }

    #[must_use]
    pub fn query_radius(
        &self,
        address: WorldAddress,
        position: [f32; 2],
        radius: f32,
    ) -> Vec<EntityId> {
        self.entities_near(address, position, radius).collect()
    }

    /// Policy enter/leave rects for an observer entity. Geometry only; no hysteresis.
    #[must_use]
    pub fn aoi_rects_for(&self, observer: EntityId) -> Option<AoiRects> {
        let transform = self.transform_of(observer)?;
        let address = self.address_of(observer)?;
        Some(aoi_policy_rects(
            transform.position,
            self.bounds_for(address),
        ))
    }

    /// Class-visible entities in the observer **leave** rect. No hysteresis. No ConnectionId.
    #[must_use]
    pub fn spatial_candidates(&self, observer: EntityId) -> Vec<EntityId> {
        self.spatial_candidates_inner(observer, true)
    }

    /// Unsorted variant for the replication classify hot path (Enter/Leave lists sort later).
    #[must_use]
    pub fn spatial_candidates_unsorted(&self, observer: EntityId) -> Vec<EntityId> {
        self.spatial_candidates_inner(observer, false)
    }

    fn spatial_candidates_inner(&self, observer: EntityId, sorted: bool) -> Vec<EntityId> {
        let Some(obs_addr) = self.address_of(observer) else {
            return Vec::new();
        };
        if self.lifecycle_of(observer) != Some(EntityLifecycle::Active) {
            return Vec::new();
        }
        let Some(rects) = self.aoi_rects_for(observer) else {
            return self.owner_only_self(observer);
        };
        let mut out = Vec::new();
        let query = if sorted {
            self.query_aabb(obs_addr, rects.leave)
        } else {
            self.query_aabb_unsorted(obs_addr, rects.leave)
        };
        for id in query {
            if !self.class_visible_to(observer, id) {
                continue;
            }
            out.push(id);
        }
        if self.class_visible_to(observer, observer) && !out.contains(&observer) {
            out.push(observer);
        }
        if sorted {
            out.sort_by_key(|id| (id.index(), id.generation()));
        }
        out
    }

    fn owner_only_self(&self, observer: EntityId) -> Vec<EntityId> {
        if self.class_visible_to(observer, observer) {
            vec![observer]
        } else {
            Vec::new()
        }
    }

    fn class_visible_to(&self, observer: EntityId, id: EntityId) -> bool {
        match self.replication_of(id).map(|m| m.class) {
            Some(ReplicationClass::VisibleObservers) => self.transform_of(id).is_some(),
            Some(ReplicationClass::OwnerOnly) => id == observer,
            Some(ReplicationClass::None) | None => false,
        }
    }

    /// Non-hysteretic spatial candidates (leave-rect + class). Not AOI membership.
    ///
    /// Observer-history Enter/Stay/Leave lives in the gameplay replication owner.
    #[must_use]
    pub fn relevance_for(&self, observer: EntityId) -> Vec<EntityId> {
        self.spatial_candidates(observer)
    }

    fn iter_active(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.iter()
            .filter(|&id| self.lifecycle_of(id) == Some(EntityLifecycle::Active))
    }

    #[must_use]
    pub fn player_id(&self) -> Option<EntityId> {
        self.player.filter(|id| self.contains(*id))
    }

    #[must_use]
    pub fn player_body_of(&self, id: EntityId) -> Option<PlayerBody> {
        let data = self.slot_live(id)?;
        let player = data.player.as_ref()?;
        let position = data.transform?.position;
        Some(PlayerBody {
            id,
            position,
            velocity: player.velocity,
            grounded: player.grounded,
            grounded_on: player.grounded_on,
            ignored_platform: player.ignored_platform,
            last_contact: player.last_contact,
            half_extents: player.half_extents,
        })
    }

    pub fn player_body(&self) -> Option<PlayerBody> {
        let id = self.player_id()?;
        self.player_body_of(id)
    }

    pub fn spawn_player(&mut self, transform: Transform, player: PlayerState) -> EntityId {
        self.spawn_player_at(WorldAddress::DEV, transform, player)
    }

    pub fn spawn_player_at(
        &mut self,
        address: WorldAddress,
        transform: Transform,
        player: PlayerState,
    ) -> EntityId {
        self.spawn(RuntimeSpawnRequest {
            address,
            transform: Some(transform),
            content_id: None,
            persistent_id: None,
            replication: ReplicationMeta::visible_observers(),
            player: Some(player),
            platform: None,
            health: None,
            interactable: None,
            npc: None,
            equipment: None,
        })
        .expect("player spawn")
    }

    pub fn spawn_platform(&mut self, transform: Transform, platform: Platform) -> EntityId {
        self.spawn_platform_at(WorldAddress::DEV, transform, platform)
    }

    pub fn spawn_platform_at(
        &mut self,
        address: WorldAddress,
        transform: Transform,
        platform: Platform,
    ) -> EntityId {
        self.spawn(
            RuntimeSpawnRequest::transient_at(address)
                .with_transform(transform)
                .with_platform(platform),
        )
        .expect("platform spawn")
    }

    pub(crate) fn set_transform_position(&mut self, id: EntityId, position: [f32; 2]) -> bool {
        let address = self.address_of(id);
        let lifecycle = self.lifecycle_of(id);
        let prev = self.transform_of(id).map(|t| t.position);
        let mut bumped = false;
        {
            let Some(data) = self.slot_live_mut(id) else {
                return false;
            };
            let Some(current) = data.transform else {
                return false;
            };
            if current.position != position {
                data.domain_revs.bump_transform();
                data.dirty.transform = true;
                bumped = true;
            }
            if let Some(transform) = data.transform.as_mut() {
                transform.position = position;
            }
        }
        if bumped {
            self.note_domain_rev();
            self.mark_replication_dirty(id, ReplicationDirtyMask::transform_only());
            if let (Some(address), Some(old)) = (address, prev) {
                self.invalidate_interest_motion(address, old, position, Some(id));
            } else {
                self.mark_interest_observer_dirty(id);
            }
        }
        if lifecycle == Some(EntityLifecycle::Active)
            && let Some(address) = address
        {
            self.spatial.relocate(id, address, position);
        }
        true
    }

    /// Stage-local support id → live platform entity. `0` is never a valid id.
    #[must_use]
    pub fn platform_entity_by_support_id(&self, support_id: u16) -> Option<EntityId> {
        if support_id == 0 {
            return None;
        }
        self.iter_platforms()
            .find(|view| view.platform.support_id == support_id)
            .map(|view| view.id)
    }

    #[must_use]
    pub fn support_id_of(&self, id: EntityId) -> Option<u16> {
        let id = self.get_platform(id)?.1.support_id;
        (id != 0).then_some(id)
    }

    /// Restore durable local-player FOOTNOTE state for reconciliation.
    /// `last_contact` is always cleared (transient). Unresolvable support ids
    /// clear that contact field rather than probing geometry.
    pub fn restore_player_sim_state(
        &mut self,
        position: [f32; 2],
        velocity: [f32; 2],
        grounded: bool,
        grounded_on_support: Option<u16>,
        ignored_support: Option<u16>,
    ) {
        let grounded_on = grounded_on_support.and_then(|id| self.platform_entity_by_support_id(id));
        let ignored_platform =
            ignored_support.and_then(|id| self.platform_entity_by_support_id(id));
        let previous = {
            let Some((transform, player)) = self.player_parts_mut() else {
                return;
            };
            let previous = transform.position;
            transform.position = position;
            player.velocity = velocity;
            player.ignored_platform = ignored_platform;
            player.last_contact = crate::footnote::ContactEvent::None;
            if grounded {
                if let Some(on) = grounded_on {
                    player.grounded = true;
                    player.grounded_on = Some(on);
                } else {
                    player.grounded = false;
                    player.grounded_on = None;
                }
            } else {
                player.grounded = false;
                player.grounded_on = None;
            }
            previous
        };
        if let Some(id) = self.player_id() {
            self.refresh_spatial(id, previous);
        }
    }

    /// Local-player FOOTNOTE step. Reconciliation and prediction must use this
    /// (or [`Self::tick_player`]), never a whole-world tick that could later
    /// re-simulate mobs/NPCs/projectiles.
    pub fn tick_predicted_player(&mut self, dt_seconds: f32, input: crate::PlayerInput) {
        if let Some(id) = self.player_id() {
            self.tick_player(id, dt_seconds, input);
        }
    }

    /// Despawn a live entity. Stale IDs return `false`.
    pub fn despawn(&mut self, id: EntityId) -> bool {
        let Some(index) = self.live_index(id) else {
            return false;
        };
        let interest = self
            .slot_live(id)
            .map(|d| (d.address, d.transform.map(|t| t.position)));
        self.cleanup_owned_runtime(id);
        self.spatial.remove(id);
        if let Some((addr, Some(pos))) = interest {
            self.invalidate_interest_presence(addr, pos, None);
        }
        self.interest_dirty_observers.remove(&id);
        self.replication_dirty.remove(&id);
        self.close_sessions_involving(id, InteractionCloseReason::TargetGone);
        self.portal_reentry
            .retain(|&actor, portal| actor != id && *portal != id);
        let slot = &mut self.slots[index];
        slot.data = None;
        slot.generation = next_generation(slot.generation);
        self.free.push(id.index());
        self.live = self.live.saturating_sub(1);
        if self.player == Some(id) {
            let next = self.iter_kind(EntityKind::Player).next();
            self.player = next;
        }
        true
    }

    /// Resync the spatial index after a leaked Transform write.
    ///
    /// Prefer [`Self::set_transform`] / [`Self::set_transform_position`]. Call this
    /// after [`Self::player_parts_mut_for`] if position may have changed.
    pub fn refresh_spatial(&mut self, id: EntityId, previous_position: [f32; 2]) {
        let info = self
            .slot_live(id)
            .map(|d| (d.transform, d.address, d.lifecycle));
        let Some((transform, address, lifecycle)) = info else {
            self.spatial.remove(id);
            return;
        };
        let Some(t) = transform else {
            self.spatial.remove(id);
            return;
        };
        let mut bumped = false;
        if t.position != previous_position
            && let Some(data) = self.slot_live_mut(id)
        {
            data.domain_revs.bump_transform();
            data.dirty.transform = true;
            bumped = true;
        }
        if bumped {
            self.note_domain_rev();
            self.mark_replication_dirty(id, ReplicationDirtyMask::transform_only());
            self.invalidate_interest_motion(address, previous_position, t.position, Some(id));
        }
        if lifecycle == EntityLifecycle::Active {
            self.spatial.relocate(id, address, t.position);
        } else {
            self.spatial.remove(id);
            if !bumped {
                self.invalidate_interest_motion(address, previous_position, t.position, Some(id));
            }
        }
    }

    /// Replicated transform domain includes velocity. Call when FOOTNOTE vx/vy
    /// changes without a position delta (idle stop, wall clamp).
    pub(crate) fn bump_transform_rev(&mut self, id: EntityId) {
        {
            let Some(data) = self.slot_live_mut(id) else {
                return;
            };
            data.domain_revs.bump_transform();
            data.dirty.transform = true;
        }
        self.note_domain_rev();
        self.mark_replication_dirty(id, ReplicationDirtyMask::transform_only());
    }

    #[must_use]
    pub fn spatial_contains(&self, id: EntityId, position: [f32; 2]) -> bool {
        let Some(address) = self.address_of(id) else {
            return false;
        };
        self.spatial.contains(id, address, position)
    }

    pub fn player_parts_mut(&mut self) -> Option<(&mut Transform, &mut PlayerState)> {
        let id = self.player_id()?;
        self.player_parts_mut_for(id)
    }

    /// Mutable player parts. Does **not** keep the spatial index in sync.
    /// Call [`Self::refresh_spatial`] after writes, or use transform helpers.
    pub fn player_parts_mut_for(
        &mut self,
        id: EntityId,
    ) -> Option<(&mut Transform, &mut PlayerState)> {
        let index = self.live_index(id)?;
        let data = self.slots[index].data.as_mut()?;
        let transform = data.transform.as_mut()?;
        let player = data.player.as_mut()?;
        Some((transform, player))
    }

    /// Immutable lookup. Stale IDs yield `None`.
    #[must_use]
    pub fn get_player(&self, id: EntityId) -> Option<(&Transform, &PlayerState)> {
        match self.slot_live(id)? {
            EntityData {
                transform: Some(transform),
                player: Some(player),
                ..
            } => Some((transform, player)),
            _ => None,
        }
    }

    #[must_use]
    pub fn get_platform(&self, id: EntityId) -> Option<(&Transform, &Platform)> {
        match self.slot_live(id)? {
            EntityData {
                transform: Some(transform),
                platform: Some(platform),
                ..
            } => Some((transform, platform)),
            _ => None,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.data.as_ref()?;
            Some(EntityId::new(index as u32, slot.generation))
        })
    }

    pub fn iter_kind(&self, kind: EntityKind) -> impl Iterator<Item = EntityId> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter_map(move |(index, slot)| {
                let data = slot.data.as_ref()?;
                let actual = data.kind();
                (actual == kind).then_some(EntityId::new(index as u32, slot.generation))
            })
    }

    pub fn iter_platforms(&self) -> impl Iterator<Item = PlatformView> + '_ {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            let data = slot.data.as_ref()?;
            let platform = data.platform?;
            let transform = data.transform?;
            Some(PlatformView {
                id: EntityId::new(index as u32, slot.generation),
                transform,
                platform,
            })
        })
    }

    pub fn collect_platforms(&self) -> Vec<PlatformView> {
        self.iter_platforms().collect()
    }

    /// If `grounded_on` names a despawned entity, clear grounding so the next
    /// tick applies gravity instead of trusting a stale ID.
    pub fn clear_stale_grounding(&mut self) {
        if let Some(id) = self.player_id() {
            self.clear_stale_grounding_for(id);
        }
    }

    pub fn clear_stale_grounding_for(&mut self, id: EntityId) {
        let on = match self.get_player(id) {
            Some((_, player)) => player.grounded_on,
            None => return,
        };
        let Some(on) = on else {
            return;
        };
        if self.contains(on) && self.kind(on) == Some(EntityKind::Platform) {
            return;
        }
        if let Some((_, player)) = self.player_parts_mut_for(id) {
            player.grounded = false;
            player.grounded_on = None;
        }
    }

    fn allocate(&mut self, data: EntityData) -> EntityId {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            debug_assert!(slot.data.is_none());
            let generation = slot.generation;
            slot.data = Some(data);
            self.live = self.live.saturating_add(1);
            let id = EntityId::new(index, generation);
            self.spatial_insert_if_active(id);
            id
        } else {
            let index = u32::try_from(self.slots.len()).expect("entity index fits u32");
            self.slots.push(Slot {
                generation: 1,
                data: Some(data),
            });
            self.live = self.live.saturating_add(1);
            let id = EntityId::new(index, 1);
            self.spatial_insert_if_active(id);
            id
        }
    }

    fn spatial_insert_if_active(&mut self, id: EntityId) {
        let Some(data) = self.slot_live(id) else {
            return;
        };
        if data.lifecycle != EntityLifecycle::Active {
            return;
        }
        let Some(transform) = data.transform else {
            return;
        };
        let address = data.address;
        self.spatial.insert(id, address, transform.position);
        self.invalidate_interest_presence(address, transform.position, Some(id));
    }

    fn live_index(&self, id: EntityId) -> Option<usize> {
        let index = id.index() as usize;
        let slot = self.slots.get(index)?;
        if slot.data.is_some() && slot.generation == id.generation() {
            Some(index)
        } else {
            None
        }
    }

    fn slot_live(&self, id: EntityId) -> Option<&EntityData> {
        let index = self.live_index(id)?;
        self.slots[index].data.as_ref()
    }

    pub(crate) fn slot_live_mut(&mut self, id: EntityId) -> Option<&mut EntityData> {
        let index = self.live_index(id)?;
        self.slots[index].data.as_mut()
    }

    /// FOOTNOTE-stage 6B developer fixtures. Not a map loader.
    ///
    /// Nearby switch stands on P0 beside spawn so the client marker is on-camera.
    pub fn spawn_dev_interaction_fixtures(&mut self) {
        use crate::interactable::InteractableKind;
        use crate::stage::{FOOTNOTE_SPAWN_X, P0, P0_POSITION};
        let floor_top = P0.top_surface(Transform::from_position(P0_POSITION));
        let y = floor_top + DEV_INTERACTABLE_HALF_Y;
        let _near = self.spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([FOOTNOTE_SPAWN_X + 1.6, y]))
                .with_content(purgatory_common::ContentId::from_token(6101))
                .visible()
                .with_interactable(Interactable::new(InteractableKind::Switch)),
        );
        let _far = self.spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([FOOTNOTE_SPAWN_X + 12.0, y]))
                .with_content(purgatory_common::ContentId::from_token(6102))
                .visible()
                .with_interactable(Interactable::new(InteractableKind::Chest)),
        );
        if let Some(other) = self.spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([FOOTNOTE_SPAWN_X + 1.2, y]))
                .with_content(purgatory_common::ContentId::from_token(6103))
                .visible()
                .with_interactable(Interactable::new(InteractableKind::Portal)),
        ) {
            self.set_address(
                other,
                WorldAddress::new(
                    purgatory_common::MapId::DEV,
                    purgatory_common::ChannelId::DEFAULT,
                    purgatory_common::InstanceId::from_raw(2),
                ),
            );
        }
    }
}

/// Half-height of the 6B developer interactable marker. Presentation size
/// on the client matches this so the fixture sits on P0, not inside it.
const DEV_INTERACTABLE_HALF_Y: f32 = 0.7;

fn entity_from_request(request: RuntimeSpawnRequest) -> EntityData {
    let equipment = request.equipment;
    let equipment_occupied = equipment.is_some_and(|e| !e.is_empty());
    let equipment_dirty = if equipment_occupied {
        equipment
            .map(EquipmentState::occupied_mask)
            .unwrap_or_default()
    } else {
        EquipmentDirtyMask::empty()
    };
    EntityData {
        transform: request.transform,
        address: request.address,
        lifecycle: EntityLifecycle::Active,
        content_id: request.content_id,
        persistent_id: request.persistent_id,
        replication: request.replication,
        player: request.player,
        platform: request.platform,
        health: request.health,
        damage_immunity_until: None,
        interactable: request.interactable,
        npc: request.npc,
        equipment,
        equipment_dirty,
        presentation_oneshot: None,
        dirty: DirtyFlags {
            membership: true,
            replication: true,
            transform: request.transform.is_some(),
            health: request.health.is_some(),
            equipment: equipment_occupied,
        },
        domain_revs: DomainRevs {
            transform: u64::from(request.transform.is_some()),
            health: u64::from(request.health.is_some()),
            membership: 1,
            replication: 1,
            equipment: u64::from(equipment_occupied),
        },
    }
}

fn next_generation(current: u32) -> u32 {
    let next = current.wrapping_add(1);
    if next == 0 { 1 } else { next }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn tiny_platform() -> (Transform, Platform) {
        (
            Transform::from_position([0.0, 0.0]),
            Platform::solid([0.1, 0.1]),
        )
    }

    #[test]
    fn spawn_returns_valid_id() {
        let mut world = World::new();
        let (transform, platform) = tiny_platform();
        let id = world.spawn_platform(transform, platform);
        assert!(world.contains(id));
        assert_eq!(world.kind(id), Some(EntityKind::Platform));
        assert_eq!(world.len(), 1);
    }

    #[test]
    fn lookup_by_valid_id_succeeds() {
        let mut world = World::new();
        let (transform, platform) = tiny_platform();
        let id = world.spawn_platform(transform, platform);
        let (found, _) = world.get_platform(id).expect("live");
        assert_eq!(found.position, transform.position);
    }

    #[test]
    fn despawn_removes_entity() {
        let mut world = World::new();
        let (transform, platform) = tiny_platform();
        let id = world.spawn_platform(transform, platform);
        assert!(world.despawn(id));
        assert!(!world.contains(id));
        assert!(world.get_platform(id).is_none());
        assert_eq!(world.len(), 0);
    }

    #[test]
    fn stale_id_lookup_fails() {
        let mut world = World::new();
        let (transform, platform) = tiny_platform();
        let id = world.spawn_platform(transform, platform);
        assert!(world.despawn(id));
        assert!(!world.contains(id));
        assert!(world.get_platform(id).is_none());
        assert!(world.kind(id).is_none());
    }

    #[test]
    fn reused_slot_has_different_identity() {
        let mut world = World::new();
        let (transform, platform) = tiny_platform();
        let a = world.spawn_platform(transform, platform);
        assert!(world.despawn(a));
        let b = world.spawn_platform(transform, platform);
        assert_eq!(a.index(), b.index());
        assert_ne!(a.generation(), b.generation());
        assert_ne!(a, b);
        assert!(!world.contains(a));
        assert!(world.contains(b));
    }

    #[test]
    fn entity_count_tracks_spawn_despawn() {
        let mut world = World::new();
        let (transform, platform) = tiny_platform();
        let a = world.spawn_platform(transform, platform);
        let b = world.spawn_platform(transform, platform);
        assert_eq!(world.len(), 2);
        assert!(world.despawn(a));
        assert_eq!(world.len(), 1);
        assert!(world.despawn(b));
        assert_eq!(world.len(), 0);
        assert!(!world.despawn(a));
    }

    #[test]
    fn repeated_spawn_despawn_does_not_corrupt_storage() {
        let mut world = World::new();
        let (transform, platform) = tiny_platform();
        let mut last = None;
        for _ in 0..64 {
            let id = world.spawn_platform(transform, platform);
            assert!(world.contains(id));
            assert_eq!(world.len(), 1);
            assert!(world.despawn(id));
            assert!(!world.contains(id));
            last = Some(id);
        }
        let live = world.spawn_platform(transform, platform);
        assert_eq!(world.len(), 1);
        assert_ne!(live, last.unwrap());
        assert!(world.contains(live));
    }

    #[test]
    fn iter_filters_by_kind() {
        let world = World::dev_stage();
        assert_eq!(world.iter_kind(EntityKind::Player).count(), 1);
        assert_eq!(world.iter_kind(EntityKind::Platform).count(), 4);
        assert_eq!(world.len(), 5);
    }

    #[test]
    fn despawning_support_platform_clears_grounded_on() {
        let mut world = World::dev_stage();
        let player = world.player_body().expect("player");
        let floor = player.grounded_on.expect("spawned on floor");
        assert!(world.contains(floor));
        assert!(world.despawn(floor));
        world.clear_stale_grounding();
        let player = world.player_body().expect("player");
        assert!(!player.grounded);
        assert!(player.grounded_on.is_none());
        assert!(world.get_platform(floor).is_none());
    }

    #[test]
    fn scale_one_thousand_entities_lifecycle() {
        const COUNT: u32 = 1000;
        let mut world = World::new();
        let (transform, platform) = tiny_platform();
        let mut ids = Vec::with_capacity(COUNT as usize);

        let spawn_start = Instant::now();
        for _ in 0..COUNT {
            ids.push(world.spawn_platform(transform, platform));
        }
        let spawn_time = spawn_start.elapsed();
        assert_eq!(world.len(), COUNT);

        let iter_start = Instant::now();
        let iterated = world.iter().count();
        let iter_time = iter_start.elapsed();
        assert_eq!(iterated as u32, COUNT);

        let despawn_start = Instant::now();
        for id in &ids {
            assert!(world.despawn(*id));
        }
        let despawn_time = despawn_start.elapsed();
        assert_eq!(world.len(), 0);
        for id in &ids {
            assert!(!world.contains(*id));
        }

        eprintln!(
            "PURGATORY Phase 4 scale COUNT={COUNT} spawn={spawn_time:?} iter={iter_time:?} despawn={despawn_time:?} build=debug host=windows-msvc"
        );
    }

    #[test]
    fn ten_thousand_entities_storage_correctness() {
        const COUNT: u32 = 10_000;
        let mut world = World::new();
        let (transform, platform) = tiny_platform();
        let mut ids = Vec::with_capacity(COUNT as usize);
        for _ in 0..COUNT {
            ids.push(world.spawn_platform(transform, platform));
        }
        assert_eq!(world.len(), COUNT);
        assert!(world.contains(ids[0]));
        assert!(world.contains(ids[COUNT as usize - 1]));
        for id in ids.drain(..) {
            assert!(world.despawn(id));
            assert!(!world.contains(id));
        }
        assert!(world.is_empty());
        let again = world.spawn_platform(transform, platform);
        assert!(world.contains(again));
        assert_eq!(world.len(), 1);
    }

    #[test]
    fn two_players_tick_independently() {
        let mut world = World::dev_stage();
        let a = world.player_id().expect("primary");
        let floor_view = world.iter_platforms().next().expect("floor");
        let floor = floor_view.id;
        let top = floor_view.top_surface();
        let (transform, state) = PlayerState::standing_on_at(floor, top, 2.0);
        let b = world.spawn_player(transform, state);
        assert_ne!(a, b);
        let ax0 = world.player_body_of(a).expect("a").position[0];
        let bx0 = world.player_body_of(b).expect("b").position[0];
        let dt = crate::TICK_DURATION.as_secs_f32();
        for _ in 0..10 {
            world.tick_player(a, dt, crate::PlayerInput::from_buttons(false, true, false));
            world.tick_player(b, dt, crate::PlayerInput::from_buttons(true, false, false));
        }
        let ax1 = world.player_body_of(a).expect("a").position[0];
        let bx1 = world.player_body_of(b).expect("b").position[0];
        assert!(ax1 > ax0, "A should move right");
        assert!(bx1 < bx0, "B should move left");
        world.despawn(a);
        assert!(!world.contains(a));
        assert!(world.contains(b));
        assert_eq!(world.player_id(), Some(b));
    }

    #[test]
    fn local_pose_change_dirties_nearby_players_only() {
        let mut world = World::footnote_test_stage();
        if let Some(id) = world.player_id() {
            world.despawn(id);
        }
        let floor = world.iter_platforms().next().expect("floor");
        let top = floor.top_surface();
        let left_x = world.bounds().min_x + 1.0;
        let right_x = world.bounds().max_x - 1.0;
        let mut far = Vec::new();
        for i in 0..16 {
            let (t, s) = PlayerState::standing_on_at(floor.id, top, left_x + (i as f32) * 0.05);
            far.push(world.spawn_player(t, s));
        }
        let (t, s) = PlayerState::standing_on_at(floor.id, top, right_x);
        let mover = world.spawn_player(t, s);
        // Clear spawn dirty so the measurement is only the move.
        for id in far.iter().copied().chain(std::iter::once(mover)) {
            world.clear_interest_observer_dirty(id);
        }
        assert_eq!(world.interest_dirty_observer_count(), 0);

        let mut t = world.transform_of(mover).unwrap();
        t.position[0] -= 0.3;
        world.set_transform(mover, t);

        let dirty = world.interest_dirty_observers_sorted();
        assert!(dirty.contains(&mover));
        for id in &far {
            assert!(
                !dirty.contains(id),
                "unrelated left-side player {id:?} dirtied by right-side move"
            );
        }
    }
}
