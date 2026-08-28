//! Authoritative world: generational slot storage for runtime entities.
//!
//! Not an ECS. Not a global registry. One [`World`] owns its entities.
//! [`EntityId`] is index + generation; reusing a slot never resurrects a
//! stale ID. Pointers and memory addresses are not used as identity.

use crate::body::{PlayerBody, PlayerState};
use crate::bounds::WorldBounds;
use crate::entity::{EntityId, EntityKind};
use crate::motion_debug::PlayerMotionDebug;
use crate::platform::{
    FLOOR, FLOOR_POSITION, ONEWAY_A, ONEWAY_A_POSITION, ONEWAY_B, ONEWAY_B_POSITION, Platform,
    PlatformView, RAISED_PLATFORM, RAISED_PLATFORM_POSITION,
};
use crate::transform::Transform;

struct Slot {
    generation: u32,
    data: Option<EntityData>,
}

struct EntityData {
    transform: Transform,
    payload: Payload,
}

enum Payload {
    Player(PlayerState),
    Platform(Platform),
}

/// Simulation container. Owns entity lifecycle.
pub struct World {
    slots: Vec<Slot>,
    free: Vec<u32>,
    live: u32,
    player: Option<EntityId>,
    bounds: WorldBounds,
    last_motion: PlayerMotionDebug,
    next_support_id: u16,
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
            next_support_id: 1,
        }
    }
}

impl World {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn bounds(&self) -> WorldBounds {
        self.bounds
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
        let data = self.slot_live(id)?;
        Some(match data.payload {
            Payload::Player(_) => EntityKind::Player,
            Payload::Platform(_) => EntityKind::Platform,
        })
    }

    #[must_use]
    pub fn player_id(&self) -> Option<EntityId> {
        self.player.filter(|id| self.contains(*id))
    }

    #[must_use]
    pub fn player_body_of(&self, id: EntityId) -> Option<PlayerBody> {
        let data = self.slot_live(id)?;
        let Payload::Player(player) = data.payload else {
            return None;
        };
        Some(PlayerBody {
            id,
            position: data.transform.position,
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
        let id = self.allocate(EntityData {
            transform,
            payload: Payload::Player(player),
        });
        if self
            .player
            .filter(|existing| self.contains(*existing))
            .is_none()
        {
            self.player = Some(id);
        }
        id
    }

    pub fn spawn_platform(&mut self, transform: Transform, mut platform: Platform) -> EntityId {
        if self.next_support_id == 0 {
            self.next_support_id = 1;
        }
        platform.support_id = self.next_support_id;
        self.next_support_id = match self.next_support_id.checked_add(1) {
            Some(next) if next != 0 => next,
            _ => u16::MAX,
        };
        self.allocate(EntityData {
            transform,
            payload: Payload::Platform(platform),
        })
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
        let Some((transform, player)) = self.player_parts_mut() else {
            return;
        };
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

    pub fn player_parts_mut(&mut self) -> Option<(&mut Transform, &mut PlayerState)> {
        let id = self.player_id()?;
        self.player_parts_mut_for(id)
    }

    pub fn player_parts_mut_for(
        &mut self,
        id: EntityId,
    ) -> Option<(&mut Transform, &mut PlayerState)> {
        let index = self.live_index(id)?;
        match self.slots[index].data.as_mut()? {
            EntityData {
                transform,
                payload: Payload::Player(player),
            } => Some((transform, player)),
            _ => None,
        }
    }

    /// Immutable lookup. Stale IDs yield `None`.
    #[must_use]
    pub fn get_player(&self, id: EntityId) -> Option<(&Transform, &PlayerState)> {
        match self.slot_live(id)? {
            EntityData {
                transform,
                payload: Payload::Player(player),
            } => Some((transform, player)),
            _ => None,
        }
    }

    #[must_use]
    pub fn get_platform(&self, id: EntityId) -> Option<(&Transform, &Platform)> {
        match self.slot_live(id)? {
            EntityData {
                transform,
                payload: Payload::Platform(platform),
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
                let actual = match data.payload {
                    Payload::Player(_) => EntityKind::Player,
                    Payload::Platform(_) => EntityKind::Platform,
                };
                (actual == kind).then_some(EntityId::new(index as u32, slot.generation))
            })
    }

    pub fn iter_platforms(&self) -> impl Iterator<Item = PlatformView> + '_ {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            let data = slot.data.as_ref()?;
            let Payload::Platform(platform) = data.payload else {
                return None;
            };
            Some(PlatformView {
                id: EntityId::new(index as u32, slot.generation),
                transform: data.transform,
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
            EntityId::new(index, generation)
        } else {
            let index = u32::try_from(self.slots.len()).expect("entity index fits u32");
            self.slots.push(Slot {
                generation: 1,
                data: Some(data),
            });
            self.live = self.live.saturating_add(1);
            EntityId::new(index, 1)
        }
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
}
