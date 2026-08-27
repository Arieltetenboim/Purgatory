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
pub use surface::{BlockQuery, surface_blocks};

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
        max_ground_speed: 6.0,
        ground_acceleration: 40.0,
        ground_deceleration: 50.0,
        air_acceleration: 20.0,
        max_air_speed: 6.0,
        gravity: 36.0,
        jump_velocity: 13.0,
    };
}

impl Default for FootnoteConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}
