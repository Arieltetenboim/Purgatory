//! Explicit development-only simulation mutations.
//!
//! Debug tooling may inspect `World` freely, but it must not poke arbitrary
//! fields. All mutations go through [`DebugAction`].

use crate::body::PLAYER_HALF_EXTENTS;
use crate::entity::EntityId;
use crate::footnote::ContactEvent;
use crate::health::Health;
use crate::platform::{FLOOR, FLOOR_POSITION};
use crate::stage::{FOOTNOTE_SPAWN_X, P0, P0_POSITION};
use crate::transform::Transform;
use crate::world::World;

/// Development command emitted by client debug tooling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugAction {
    /// Return the local test player to the development spawn.
    ResetPlayer,
}

impl World {
    /// Apply one debug command. No-op if the target does not exist.
    pub fn apply_debug_action(&mut self, action: DebugAction) {
        match action {
            DebugAction::ResetPlayer => self.reset_dev_player(),
        }
    }

    /// Restore the local player to the development spawn without changing IDs.
    pub fn reset_dev_player(&mut self) {
        if let Some(id) = self.player_id() {
            self.reset_player_entity(id);
        }
    }

    /// Development reset retained for the existing local/server DEV command.
    pub fn reset_player_entity(&mut self, id: EntityId) {
        let _ = self.restore_player_entity(id, true, false);
    }

    /// Restore a dead player without changing its runtime identity.
    ///
    /// Revive intentionally does not move the player. Respawn adds the
    /// authoritative entry placement below; both use the same reset path.
    pub fn revive_player_entity(&mut self, id: EntityId) -> bool {
        self.restore_player_entity(id, false, true)
    }

    /// Restore a dead player through normal respawn semantics.
    pub fn respawn_player_entity(&mut self, id: EntityId) -> bool {
        self.restore_player_entity(id, true, true)
    }

    fn restore_player_entity(
        &mut self,
        id: EntityId,
        place_at_entry: bool,
        require_dead: bool,
    ) -> bool {
        let health = self.health_of(id);
        if (require_dead && !health.is_some_and(Health::is_dead)) || self.get_player(id).is_none() {
            return false;
        }
        self.clear_restoration_runtime(id);
        let previous = self.transform_of(id).map(|transform| transform.position);
        // Prefer Phase-4.6 P0 floor when present; else compact FLOOR.
        let floor = self
            .iter_platforms()
            .find(|view| {
                view.platform.half_extents == P0.half_extents
                    && (view.transform.position[0] - P0_POSITION[0]).abs() < 0.01
                    && (view.transform.position[1] - P0_POSITION[1]).abs() < 0.01
            })
            .or_else(|| {
                self.iter_platforms()
                    .find(|view| view.platform.half_extents == FLOOR.half_extents)
            });
        let (position, grounded, grounded_on) = if let Some(view) = floor {
            let x = if view.platform.half_extents == P0.half_extents {
                FOOTNOTE_SPAWN_X
            } else {
                -2.0
            };
            (
                [x, view.top_surface() + PLAYER_HALF_EXTENTS[1]],
                true,
                Some(view.id),
            )
        } else {
            let top = FLOOR.top_surface(Transform::from_position(FLOOR_POSITION));
            ([-2.0, top + PLAYER_HALF_EXTENTS[1]], false, None)
        };
        if place_at_entry {
            let Some((transform, player)) = self.player_parts_mut_for(id) else {
                return false;
            };
            transform.position = position;
            player.grounded = grounded;
            player.grounded_on = grounded_on;
        }
        if let Some((_, player)) = self.player_parts_mut_for(id) {
            player.velocity = [0.0, 0.0];
            player.ignored_platform = None;
            player.last_contact = ContactEvent::None;
            player.half_extents = PLAYER_HALF_EXTENTS;
        } else {
            return false;
        }
        if let Some(health) = health.filter(|health| health.is_dead()) {
            self.set_health(id, Health::full(health.max));
        }
        self.clear_presentation_oneshot(id);
        if let Some(previous) = previous {
            self.refresh_spatial(id, previous);
        }
        // Velocity is part of the replicated transform domain even when the
        // respawn position equals the prior position.
        self.bump_transform_rev(id);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::footnote::FootnoteConfig;
    use crate::input::PlayerInput;
    use crate::platform::FLOOR;
    use crate::world::World;

    const DT: f32 = 1.0 / 30.0;

    #[test]
    fn reset_player_restores_spawn_and_clears_velocity() {
        let mut world = World::dev_stage();
        let spawn = world.player_body().expect("player");
        let player_id = spawn.id;
        let platforms: Vec<_> = world.iter_platforms().map(|view| view.id).collect();
        let max = FootnoteConfig::DEFAULT.max_ground_speed;

        for _ in 0..40 {
            world.tick(DT, PlayerInput::from_buttons(false, true, false));
        }
        let moved = world.player_body().expect("player");
        assert!(moved.position[0] > spawn.position[0]);
        assert!((moved.velocity[0] - max).abs() < 0.05);

        world.apply_debug_action(DebugAction::ResetPlayer);
        let reset = world.player_body().expect("player");
        assert_eq!(reset.id, player_id);
        assert!((reset.position[0] - spawn.position[0]).abs() < 1e-4);
        assert!((reset.position[1] - spawn.position[1]).abs() < 1e-4);
        assert_eq!(reset.velocity, [0.0, 0.0]);
        assert!(reset.grounded);
        assert!(reset.ignored_platform.is_none());
        assert_eq!(reset.grounded_on, spawn.grounded_on);
        let after: Vec<_> = world.iter_platforms().map(|view| view.id).collect();
        assert_eq!(after, platforms);
        assert_eq!(
            world
                .iter_platforms()
                .find(|view| view.platform.half_extents == FLOOR.half_extents)
                .map(|view| view.id),
            reset.grounded_on
        );
    }

    #[test]
    fn debug_actions_are_simulation_owned() {
        let mut world = World::dev_stage();
        let id = world.player_id().expect("player");
        world.apply_debug_action(DebugAction::ResetPlayer);
        assert_eq!(world.player_id(), Some(id));
        assert!(world.contains(id));
    }

    #[test]
    fn revive_restores_health_without_respawn_placement() {
        let mut world = World::dev_stage();
        let id = world.player_id().expect("player");
        let before = world.transform_of(id).expect("transform").position;
        let position = [before[0] + 3.0, before[1] + 2.0];
        world.set_transform(id, Transform::from_position(position));
        world.set_health(
            id,
            Health {
                current: 0.0,
                max: 20.0,
            },
        );

        assert!(world.revive_player_entity(id));
        assert_eq!(world.transform_of(id).unwrap().position, position);
        assert_eq!(world.health_of(id).unwrap(), Health::full(20.0));
        assert!(!world.revive_player_entity(id));
    }
}
