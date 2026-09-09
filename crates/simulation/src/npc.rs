//! Authoritative NPC capability state and workload parameters.
//!
//! [`NpcState`] carries the runtime state used by the simulation's NPC driver:
//! deterministic locomotion, shared [`CollisionBody`] grounding data, target
//! selection, and combat-adjacent lifecycle/contact state. The driver and
//! damage/action orchestration live in [`crate::World`]; this module is not a
//! full AI, pathfinding, or authored monster-definition system.
//!
//! NPCs remain capability-composed [`crate::EntityKind::Generic`] entities.

use crate::body::CollisionBody;
use crate::entity::EntityId;
use crate::footnote::ContactEvent;
use crate::time::SimulationTick;

/// Flat Strike damage placeholder. Ten strikes defeat the default 20 Health NPC.
pub const STRIKE_DAMAGE: f32 = 2.0;
/// Contact damage dealt by a chasing NPC on collider overlap.
pub const CONTACT_DAMAGE: f32 = 1.0;
/// Strike range in world units.
pub const STRIKE_RANGE: f32 = 4.0;
/// Action table duration for Strike (ticks).
pub const STRIKE_DURATION_TICKS: u64 = 3;
/// Default NPC health max for workload spawns.
pub const NPC_HEALTH_MAX: f32 = 20.0;
/// Pulse tick damage placeholder.
pub const PULSE_DAMAGE: f32 = 1.0;
/// Default pulse period between damage ticks.
pub const PULSE_PERIOD_TICKS: u64 = 10;
/// Default pulse lifetime.
pub const PULSE_DURATION_TICKS: u64 = 30;

/// Per-NPC locomotion and collision-body parameters.
///
/// This is simulation runtime data. It deliberately has no authored-content
/// identity or monster-specific behavior.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcRuntimeConfig {
    /// Horizontal movement speed in world units per second.
    pub movement_speed: f32,
    /// Axis-aligned collision-body half-extents in world units.
    pub half_extents: [f32; 2],
    /// Ticks between patrol heading changes while active.
    pub turn_period_ticks: u64,
    /// Ticks spent walking before a patrol stop.
    pub walk_period_ticks: u64,
    /// Ticks spent idle between patrol walks.
    pub stop_period_ticks: u64,
}

impl Default for NpcRuntimeConfig {
    fn default() -> Self {
        Self {
            movement_speed: 2.0,
            half_extents: [0.4, 0.6],
            turn_period_ticks: 45,
            walk_period_ticks: 30,
            stop_period_ticks: 15,
        }
    }
}

/// Optional NPC capability on a Generic entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcState {
    /// Workload type token (not content id).
    pub type_token: u32,
    pub home: [f32; 2],
    pub hotspot_radius: f32,
    /// Persistent live-player target, when the NPC has acquired one.
    pub target: Option<EntityId>,
    /// Unit heading in XZ (2D: x,y world plane).
    pub heading: [f32; 2],
    pub velocity: [f32; 2],
    pub grounded: bool,
    pub grounded_on: Option<EntityId>,
    pub last_contact: ContactEvent,
    pub runtime_config: NpcRuntimeConfig,
    pub next_turn_tick: SimulationTick,
    pub next_mode_tick: SimulationTick,
    pub walking: bool,
    pub active: bool,
    /// Health reached zero; awaiting despawn/respawn.
    pub dead_pending: bool,
    /// Seed stream for deterministic decisions.
    pub rng_state: u32,
}

impl NpcState {
    #[must_use]
    pub fn new(
        type_token: u32,
        home: [f32; 2],
        hotspot_radius: f32,
        seed: u32,
        now: SimulationTick,
        active: bool,
    ) -> Self {
        Self::new_with_runtime_config(
            type_token,
            home,
            hotspot_radius,
            seed,
            now,
            active,
            NpcRuntimeConfig::default(),
        )
    }

    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_runtime_config(
        type_token: u32,
        home: [f32; 2],
        hotspot_radius: f32,
        seed: u32,
        now: SimulationTick,
        active: bool,
        runtime_config: NpcRuntimeConfig,
    ) -> Self {
        let mut rng_state = if seed == 0 { 1 } else { seed };
        let heading = random_heading(&mut rng_state);
        Self {
            type_token,
            home,
            hotspot_radius: hotspot_radius.max(0.5),
            target: None,
            heading,
            velocity: [0.0, 0.0],
            grounded: false,
            grounded_on: None,
            last_contact: ContactEvent::None,
            runtime_config,
            next_turn_tick: now.saturating_add_ticks(runtime_config.turn_period_ticks),
            next_mode_tick: now.saturating_add_ticks(runtime_config.walk_period_ticks),
            walking: active,
            active,
            dead_pending: false,
            rng_state,
        }
    }

    pub fn advance_rng(&mut self) -> u32 {
        // LCG (Numerical Recipes). Deterministic across platforms for u32.
        self.rng_state = self
            .rng_state
            .wrapping_mul(1664525)
            .wrapping_add(1013904223);
        self.rng_state
    }
}

impl CollisionBody for NpcState {
    fn velocity(&self) -> [f32; 2] {
        self.velocity
    }

    fn set_velocity(&mut self, velocity: [f32; 2]) {
        self.velocity = velocity;
    }

    fn half_extents(&self) -> [f32; 2] {
        self.runtime_config.half_extents
    }

    fn grounded(&self) -> bool {
        self.grounded
    }

    fn set_grounded(&mut self, grounded: bool) {
        self.grounded = grounded;
    }

    fn grounded_on(&self) -> Option<EntityId> {
        self.grounded_on
    }

    fn set_grounded_on(&mut self, platform: Option<EntityId>) {
        self.grounded_on = platform;
    }

    fn ignored_platform(&self) -> Option<EntityId> {
        None
    }
}

fn random_heading(rng: &mut u32) -> [f32; 2] {
    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
    [if *rng < u32::MAX / 2 { 1.0 } else { -1.0 }, 0.0]
}

/// Simulation-level action request. Not a wire `ClientControl`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionRequest {
    pub actor: crate::entity::EntityId,
    pub target: crate::entity::EntityId,
    pub kind: crate::action::ActionKind,
}

/// Why an action request was rejected (workload counters).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionRejectReason {
    MissingActor,
    MissingTarget,
    ActorDead,
    TargetDead,
    OutOfRange,
    Busy,
    Gate(crate::action_gate::ActionDenialReason),
    UnsupportedKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcg_is_deterministic() {
        let mut a = NpcState::new(1, [0.0, 0.0], 2.0, 42, SimulationTick::from_count(0), true);
        let mut b = NpcState::new(1, [0.0, 0.0], 2.0, 42, SimulationTick::from_count(0), true);
        assert_eq!(a.advance_rng(), b.advance_rng());
        assert_eq!(a.advance_rng(), b.advance_rng());
    }
}
