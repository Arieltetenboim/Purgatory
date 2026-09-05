//! Player kinematic and FOOTNOTE contact state.
//!
//! Velocity is retained every tick. Position lives on [`crate::Transform`].

use crate::aabb::Aabb;
use crate::entity::EntityId;
use crate::footnote::ContactEvent;
use crate::transform::Transform;

/// Player collider half-extents in world units.
pub const PLAYER_HALF_EXTENTS: [f32; 2] = [0.4, 0.6];

/// Movement state stored on a player entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerState {
    pub velocity: [f32; 2],
    pub grounded: bool,
    /// Platform entity currently supporting the player, if any.
    pub grounded_on: Option<EntityId>,
    /// OneWay platform temporarily ignored after drop-through.
    pub ignored_platform: Option<EntityId>,
    /// Most recent land / leave transition for this tick.
    pub last_contact: ContactEvent,
    pub half_extents: [f32; 2],
    /// Last non-zero horizontal intent. `1` right, `-1` left. Not on the wire.
    pub facing_sign: i8,
}

impl PlayerState {
    #[must_use]
    pub fn standing_on(floor: EntityId, floor_top: f32) -> (Transform, Self) {
        Self::standing_on_at(floor, floor_top, -2.0)
    }

    #[must_use]
    pub fn standing_on_at(floor: EntityId, floor_top: f32, x: f32) -> (Transform, Self) {
        let half = PLAYER_HALF_EXTENTS;
        (
            Transform::from_position([x, floor_top + half[1]]),
            Self {
                velocity: [0.0, 0.0],
                grounded: true,
                grounded_on: Some(floor),
                ignored_platform: None,
                last_contact: ContactEvent::None,
                half_extents: half,
                facing_sign: 1,
            },
        )
    }

    #[must_use]
    pub fn aabb(self, transform: Transform) -> Aabb {
        Aabb::new(transform.position, self.half_extents)
    }
}

/// Copied player snapshot for tests and presentation. Not a second world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerBody {
    pub id: EntityId,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub grounded: bool,
    pub grounded_on: Option<EntityId>,
    pub ignored_platform: Option<EntityId>,
    pub last_contact: ContactEvent,
    pub half_extents: [f32; 2],
}

impl PlayerBody {
    #[must_use]
    pub fn aabb(self) -> Aabb {
        Aabb::new(self.position, self.half_extents)
    }
}
