//! Authoritative simulation core.
//!
//! This crate must remain free of windowing, GPU, networking, and client
//! presentation dependencies. Gameplay systems must not read wall-clock time;
//! callers supply elapsed [`std::time::Duration`] values to [`SimulationClock`].

mod aabb;
mod body;
mod bounds;
mod clock;
mod collision;
mod contact;
mod debug_action;
mod entity;
mod footnote;
mod input;
mod motion_debug;
mod movement;
#[cfg(test)]
mod phase47_tests;
mod platform;
mod stage;
mod time;
mod transform;
mod world;

pub use aabb::Aabb;
pub use body::{PLAYER_HALF_EXTENTS, PlayerBody, PlayerState};
pub use bounds::WorldBounds;
pub use clock::{ClockConfig, ClockUpdate, SimulationClock};
pub use collision::{
    Overlap, RecoveryResult, VerticalContact, detect_overlap, detect_overlaps,
    recover_solid_penetration,
};
pub use contact::{CONTACT_EPSILON, MAX_RECOVERY_TRANSLATION, RECOVERY_PENETRATION_MIN};
pub use debug_action::DebugAction;
pub use entity::{EntityId, EntityKind};
pub use footnote::{BlockQuery, ContactEvent, FootnoteConfig, surface_blocks};
pub use input::PlayerInput;
pub use motion_debug::{CorrectionAxis, PlayerMotionDebug, ResponseKind};
pub use movement::{GRAVITY, JUMP_VELOCITY, MOVE_SPEED};
pub use platform::{
    Approach, FLOOR, FLOOR_POSITION, ONEWAY_A, ONEWAY_A_POSITION, ONEWAY_B, ONEWAY_B_POSITION,
    Platform, PlatformKind, PlatformView, RAISED_PLATFORM, RAISED_PLATFORM_POSITION,
};
pub use stage::{FOOTNOTE_SPAWN_X, FOOTNOTE_TEST_VIEWPORT_HEIGHT, P0, P0_POSITION};
pub use time::{
    MAX_CATCH_UP, MAX_CATCH_UP_NANOS, MAX_CATCH_UP_TICKS, MAX_TICKS_PER_ADVANCE, SimulationTick,
    SimulationTime, TICK_DURATION, TICK_DURATION_NANOS, TICK_RATE_HZ,
};
pub use transform::Transform;
pub use world::World;

/// Cargo package version for this crate.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_nonempty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn common_is_linked() {
        assert!(!purgatory_common::version().is_empty());
    }

    #[test]
    fn movement_sources_do_not_import_winit_or_wgpu() {
        for src in [
            include_str!("aabb.rs"),
            include_str!("body.rs"),
            include_str!("bounds.rs"),
            include_str!("collision.rs"),
            include_str!("contact.rs"),
            include_str!("debug_action.rs"),
            include_str!("entity.rs"),
            include_str!("footnote/mod.rs"),
            include_str!("footnote/contact.rs"),
            include_str!("footnote/controller.rs"),
            include_str!("footnote/surface.rs"),
            include_str!("input.rs"),
            include_str!("motion_debug.rs"),
            include_str!("movement.rs"),
            include_str!("platform.rs"),
            include_str!("stage.rs"),
            include_str!("transform.rs"),
            include_str!("world.rs"),
        ] {
            assert!(
                !src.contains("use winit") && !src.contains("extern crate winit"),
                "simulation must not import winit"
            );
            assert!(
                !src.contains("use wgpu") && !src.contains("extern crate wgpu"),
                "simulation must not import wgpu"
            );
            assert!(
                !src.contains("use egui") && !src.contains("extern crate egui"),
                "simulation must not import egui"
            );
        }
    }
}
