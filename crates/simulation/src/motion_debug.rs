//! Per-tick player motion diagnostics for development discontinuity detection.
//!
//! Lightweight; written by FOOTNOTE each tick. Presentation may read it.
//! Not networked. Not a gameplay authority channel.
//! Console logging is a **client** concern (debug toggles); this module only
//! records structured facts.

use crate::entity::EntityId;

/// Axis that applied a collision/bound correction this tick.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CorrectionAxis {
    #[default]
    None,
    Horizontal,
    Vertical,
    WorldBound,
}

/// Whether the correction came from ordinary crossing collision or recovery.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ResponseKind {
    #[default]
    None,
    /// Travel this tick crossed a blocking surface.
    Normal,
    /// Start-of-tick exceptional Solid penetration recovery.
    Recovery,
}

/// Snapshot of one player integration tick (debug / regression aid).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerMotionDebug {
    pub previous_position: [f32; 2],
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub delta: [f32; 2],
    pub expected_max_delta: f32,
    pub discontinuity: bool,
    pub grounded_on: Option<EntityId>,
    pub collision_candidate: Option<EntityId>,
    pub correction: [f32; 2],
    pub correction_axis: CorrectionAxis,
    pub response_kind: ResponseKind,
}

impl PlayerMotionDebug {
    /// Maximum plausible travel this tick: `|v|×dt` plus a small correction budget.
    #[must_use]
    pub fn expected_max_step(velocity: [f32; 2], dt: f32) -> f32 {
        let speed = (velocity[0] * velocity[0] + velocity[1] * velocity[1]).sqrt();
        // One normal collision correction / glue / ceiling bias budget.
        const CORRECTION_TOLERANCE: f32 = 0.2;
        speed * dt + CORRECTION_TOLERANCE
    }

    #[must_use]
    pub fn delta_length(self) -> f32 {
        (self.delta[0] * self.delta[0] + self.delta[1] * self.delta[1]).sqrt()
    }
}
