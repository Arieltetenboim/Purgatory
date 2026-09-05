//! Minimal authoritative NPC runtime for Phase 7.2 workload.
//!
//! Not AI, pathfinding, content definitions, or FOOTNOTE locomotion.
//! Sim classification remains [`crate::EntityKind::Generic`].

use crate::time::SimulationTick;

/// Flat Strike damage placeholder. Not a combat formula.
pub const STRIKE_DAMAGE: f32 = 1.0;
/// Strike range in world units.
pub const STRIKE_RANGE: f32 = 4.0;
/// Action table duration for Strike (ticks).
pub const STRIKE_DURATION_TICKS: u64 = 3;
/// Default NPC health max for workload spawns.
pub const NPC_HEALTH_MAX: f32 = 20.0;
/// Walk speed for active NPCs (world units / second).
pub const NPC_MOVE_SPEED: f32 = 2.0;
/// Ticks between heading changes when active.
pub const NPC_TURN_PERIOD_TICKS: u64 = 45;
/// Ticks of walk before a short stop.
pub const NPC_WALK_PERIOD_TICKS: u64 = 30;
/// Ticks of idle stop between walks.
pub const NPC_STOP_PERIOD_TICKS: u64 = 15;
/// Pulse tick damage placeholder.
pub const PULSE_DAMAGE: f32 = 1.0;
/// Default pulse period between damage ticks.
pub const PULSE_PERIOD_TICKS: u64 = 10;
/// Default pulse lifetime.
pub const PULSE_DURATION_TICKS: u64 = 30;

/// Optional NPC capability on a Generic entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcState {
    /// Workload type token (not content id).
    pub type_token: u32,
    pub home: [f32; 2],
    pub hotspot_radius: f32,
    /// Unit heading in XZ (2D: x,y world plane).
    pub heading: [f32; 2],
    pub velocity: [f32; 2],
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
        let mut rng_state = if seed == 0 { 1 } else { seed };
        let heading = random_heading(&mut rng_state);
        Self {
            type_token,
            home,
            hotspot_radius: hotspot_radius.max(0.5),
            heading,
            velocity: [0.0, 0.0],
            next_turn_tick: now.saturating_add_ticks(NPC_TURN_PERIOD_TICKS),
            next_mode_tick: now.saturating_add_ticks(NPC_WALK_PERIOD_TICKS),
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

fn random_heading(rng: &mut u32) -> [f32; 2] {
    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
    let angle = (*rng as f32 / u32::MAX as f32) * std::f32::consts::TAU;
    [angle.cos(), angle.sin()]
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
