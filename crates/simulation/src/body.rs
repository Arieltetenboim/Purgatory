//! Player kinematic and FOOTNOTE contact state.
//!
//! Velocity is retained every tick. Position lives on [`crate::Transform`].

use crate::aabb::Aabb;
use crate::entity::EntityId;
use crate::footnote::ContactEvent;
use crate::transform::Transform;

/// Player collider half-extents in world units.
pub const PLAYER_HALF_EXTENTS: [f32; 2] = [0.4, 0.6];

/// State required by the shared FOOTNOTE collision resolvers.
pub trait CollisionBody {
    fn velocity(&self) -> [f32; 2];
    fn set_velocity(&mut self, velocity: [f32; 2]);
    fn half_extents(&self) -> [f32; 2];
    fn grounded(&self) -> bool;
    fn set_grounded(&mut self, grounded: bool);
    fn grounded_on(&self) -> Option<EntityId>;
    fn set_grounded_on(&mut self, platform: Option<EntityId>);
    fn ignored_platform(&self) -> Option<EntityId>;

    fn aabb(&self, transform: Transform) -> Aabb {
        Aabb::new(transform.position, self.half_extents())
    }
}

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
    /// Per-player movement-speed override. `None` means canonical movement
    /// speed; future gameplay systems may feed this seam without a modifier
    /// framework.
    pub movement_speed_override: Option<f32>,
    /// Per-player jump-speed override. `None` means the canonical jump value.
    pub jump_speed_override: Option<f32>,
    /// Fixed-tick coyote-time countdown after leaving valid ground.
    pub coyote_ticks: u8,
    /// Fixed-tick countdown for a jump pressed before landing.
    pub jump_buffer_ticks: u8,
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
                movement_speed_override: None,
                jump_speed_override: None,
                coyote_ticks: 0,
                jump_buffer_ticks: 0,
            },
        )
    }

    #[must_use]
    pub fn aabb(self, transform: Transform) -> Aabb {
        CollisionBody::aabb(&self, transform)
    }
}

impl CollisionBody for PlayerState {
    fn velocity(&self) -> [f32; 2] {
        self.velocity
    }

    fn set_velocity(&mut self, velocity: [f32; 2]) {
        self.velocity = velocity;
    }

    fn half_extents(&self) -> [f32; 2] {
        self.half_extents
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
        self.ignored_platform
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
