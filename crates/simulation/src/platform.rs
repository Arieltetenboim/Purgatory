//! Static platform collider policy and geometry.
//!
//! Overlap detection is geometric. Blocking is decided by FOOTNOTE via
//! [`crate::footnote::surface_blocks`] (and [`Platform::blocks_approach`] for
//! Solid-only callers).
//!
//! Runtime identity is [`crate::EntityId`], assigned by [`crate::World`].

use crate::aabb::Aabb;
use crate::entity::EntityId;
use crate::transform::Transform;

/// Classification of a platform collider.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformKind {
    /// Blocks every approach.
    Solid,
    /// Pass-through from below; lands from above; never a horizontal wall.
    OneWay,
}

/// Direction of motion used to ask whether a platform should block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Approach {
    Right,
    Left,
    Up,
    Down,
    None,
}

impl Approach {
    #[must_use]
    pub fn from_horizontal_velocity(vx: f32) -> Self {
        if vx > 0.0 {
            Self::Right
        } else if vx < 0.0 {
            Self::Left
        } else {
            Self::None
        }
    }

    #[must_use]
    pub fn from_vertical_velocity(vy: f32) -> Self {
        if vy > 0.0 {
            Self::Up
        } else if vy < 0.0 {
            Self::Down
        } else {
            Self::None
        }
    }
}

/// Platform collider. Position lives on the entity [`Transform`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Platform {
    pub half_extents: [f32; 2],
    pub kind: PlatformKind,
}

impl Platform {
    #[must_use]
    pub const fn solid(half_extents: [f32; 2]) -> Self {
        Self {
            half_extents,
            kind: PlatformKind::Solid,
        }
    }

    #[must_use]
    pub const fn one_way(half_extents: [f32; 2]) -> Self {
        Self {
            half_extents,
            kind: PlatformKind::OneWay,
        }
    }

    #[must_use]
    pub fn aabb(self, transform: Transform) -> Aabb {
        Aabb::new(transform.position, self.half_extents)
    }

    #[must_use]
    pub fn top_surface(self, transform: Transform) -> f32 {
        transform.position[1] + self.half_extents[1]
    }

    #[must_use]
    pub fn min_x(self, transform: Transform) -> f32 {
        self.aabb(transform).min_x()
    }

    #[must_use]
    pub fn max_x(self, transform: Transform) -> f32 {
        self.aabb(transform).max_x()
    }

    #[must_use]
    pub fn min_y(self, transform: Transform) -> f32 {
        self.aabb(transform).min_y()
    }

    #[must_use]
    pub fn max_y(self, transform: Transform) -> f32 {
        self.aabb(transform).max_y()
    }

    /// Solid-only convenience. Prefer [`crate::footnote::surface_blocks`] with
    /// a [`crate::footnote::BlockQuery`] for OneWay-aware response.
    #[must_use]
    pub fn blocks_approach(self, _approach: Approach) -> bool {
        match self.kind {
            PlatformKind::Solid => true,
            PlatformKind::OneWay => false,
        }
    }
}

/// Runtime view of a platform entity. Copyable; not a stored record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlatformView {
    pub id: EntityId,
    pub transform: Transform,
    pub platform: Platform,
}

impl PlatformView {
    #[must_use]
    pub fn aabb(self) -> Aabb {
        self.platform.aabb(self.transform)
    }

    #[must_use]
    pub fn top_surface(self) -> f32 {
        self.platform.top_surface(self.transform)
    }
}

/// Static floor spanning the development viewport.
pub const FLOOR: Platform = Platform::solid([8.0, 0.45]);

/// Floor center. Viewport at 16×9 is y ∈ [-4.5, 4.5].
pub const FLOOR_POSITION: [f32; 2] = [0.0, -3.8];

/// Elevated static Solid platform to the right of spawn.
pub const RAISED_PLATFORM: Platform = Platform::solid([1.5, 0.18]);

/// Raised platform center.
pub const RAISED_PLATFORM_POSITION: [f32; 2] = [3.0, -1.25];

/// First OneWay platform above spawn (jump-through / land / drop).
pub const ONEWAY_A: Platform = Platform::one_way([1.4, 0.12]);

pub const ONEWAY_A_POSITION: [f32; 2] = [-1.5, -1.8];

/// Second OneWay platform for chained landings.
pub const ONEWAY_B: Platform = Platform::one_way([1.2, 0.12]);

pub const ONEWAY_B_POSITION: [f32; 2] = [1.2, -0.4];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_blocks_every_approach() {
        for approach in [
            Approach::Left,
            Approach::Right,
            Approach::Up,
            Approach::Down,
            Approach::None,
        ] {
            assert!(FLOOR.blocks_approach(approach));
            assert!(RAISED_PLATFORM.blocks_approach(approach));
        }
    }

    #[test]
    fn oneway_blocks_approach_is_false_without_query() {
        assert!(!ONEWAY_A.blocks_approach(Approach::Down));
        assert!(!ONEWAY_A.blocks_approach(Approach::Left));
    }

    #[test]
    fn top_surface_is_center_plus_half_height() {
        let transform = Transform::from_position(FLOOR_POSITION);
        assert!(
            (FLOOR.top_surface(transform) - (FLOOR_POSITION[1] + FLOOR.half_extents[1])).abs()
                < f32::EPSILON
        );
    }
}
