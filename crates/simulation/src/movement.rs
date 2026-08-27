//! Compatibility façade for FOOTNOTE tuning constants.
//!
//! Authoritative locomotion lives in [`crate::footnote`].

use crate::footnote::FootnoteConfig;

/// Horizontal max ground speed (world units / second). Alias of FOOTNOTE default.
pub const MOVE_SPEED: f32 = FootnoteConfig::DEFAULT.max_ground_speed;

/// Downward acceleration (world units / second²).
pub const GRAVITY: f32 = FootnoteConfig::DEFAULT.gravity;

/// Instantaneous upward jump velocity (world units / second).
pub const JUMP_VELOCITY: f32 = FootnoteConfig::DEFAULT.jump_velocity;
