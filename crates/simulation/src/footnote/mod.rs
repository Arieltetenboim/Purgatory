//! FOOTNOTE: authoritative player/platform movement and contact policy.
//!
//! Geometry detects overlap. Platform policy decides blocking. FOOTNOTE owns
//! acceleration, air control, momentum, landing, one-way platforms, and
//! drop-through. This crate must not depend on winit, wgpu, or egui.

mod contact;
mod controller;
mod surface;

#[cfg(test)]
mod tests;

pub use contact::ContactEvent;
pub(crate) use controller::glue_to_support;
pub use surface::{BlockQuery, surface_blocks};

/// Fixed-tick grace window for a jump after leaving valid ground.
pub const COYOTE_TICKS: u8 = 3;
/// Fixed-tick window in which a pre-landing jump press is retained.
pub const JUMP_BUFFER_TICKS: u8 = 3;

/// Compile-time FOOTNOTE tuning. Units are world units / second or / second².
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootnoteConfig {
    pub max_ground_speed: f32,
    pub ground_acceleration: f32,
    pub ground_deceleration: f32,
    pub air_acceleration: f32,
    pub max_air_speed: f32,
    pub gravity: f32,
    pub jump_velocity: f32,
}

impl FootnoteConfig {
    /// Development defaults. Not MapleStory-tuned; rates so max takes several ticks.
    pub const DEFAULT: Self = Self {
        max_ground_speed: 4.0,
        ground_acceleration: 24.0,
        ground_deceleration: 30.0,
        air_acceleration: 10.0,
        max_air_speed: 4.2,
        gravity: 36.0,
        jump_velocity: 13.0,
    };

    /// Same locomotion as [`Self::DEFAULT`], with matching ground/air max speed.
    #[must_use]
    pub const fn with_move_speed(speed: f32) -> Self {
        let mut cfg = Self::DEFAULT;
        cfg.max_ground_speed = speed;
        cfg.max_air_speed = speed;
        cfg
    }

    /// Keep the current locomotion tuning while applying one player's
    /// explicit speed override.
    #[must_use]
    pub const fn with_speed_override(self, speed: f32) -> Self {
        let mut cfg = self;
        cfg.max_ground_speed = speed;
        cfg.max_air_speed = speed;
        cfg
    }

    /// Keep the current locomotion tuning while applying one player's
    /// explicit jump-speed override.
    #[must_use]
    pub const fn with_jump_speed_override(self, speed: f32) -> Self {
        let mut cfg = self;
        cfg.jump_velocity = speed;
        cfg
    }
}

impl Default for FootnoteConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}
