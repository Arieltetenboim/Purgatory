//! Phase 10D-3 player death restoration foundation.

use crate::ability::{
    AbilityActivation, AbilityDefinition, AbilityDelivery, AbilityEffect, AbilityRequest,
    AbilityTiming,
};
use crate::action_gate::ActionGateContext;
use crate::health::Health;
use crate::transform::Transform;
use crate::{ContentId, EntityId, World};

fn restoration_ability() -> AbilityDefinition {
    AbilityDefinition {
        id: ContentId::from_token(10_003),
        timing: AbilityTiming {
            windup_ticks: 2,
            active_ticks: 1,
            recovery_ticks: 2,
            cooldown_ticks: 20,
        },
        activation: AbilityActivation::Independent,
        delivery: AbilityDelivery::ForwardQuery {
            range: 2.0,
            half_height: 1.0,
            max_targets: 1,
        },
        effects: vec![AbilityEffect::Damage { amount: 1.0 }],
    }
}

#[test]
fn respawn_restores_all_player_runtime_state_and_keeps_entity_id() {
    let mut world = World::dev_stage();
    let player = world.player_id().expect("player");
    world.set_health(player, Health::full(20.0));
    let definition = restoration_ability();
    world.grant_ability(player, definition.id);
    world
        .request_ability(
            AbilityRequest {
                actor: player,
                selected: None,
                definition: &definition,
            },
            ActionGateContext::in_world(),
        )
        .expect("ability starts");
    assert!(world.active_action(player).is_some());
    assert!(!world.ability_cooldown_ready(player, definition.id));

    let moved = [4.0, 8.0];
    world.set_transform(player, Transform::from_position(moved));
    if let Some((_, state)) = world.player_parts_mut_for(player) {
        state.velocity = [3.0, -4.0];
        state.grounded = false;
        state.grounded_on = None;
        state.ignored_platform = Some(EntityId::new(99, 1));
        state.last_contact = crate::footnote::ContactEvent::Landed { platform: player };
    }
    world.set_health(
        player,
        Health {
            current: 0.0,
            max: 20.0,
        },
    );

    assert!(world.respawn_player_entity(player));
    assert_eq!(world.player_id(), Some(player));
    assert_eq!(world.health_of(player), Some(Health::full(20.0)));
    assert!(world.active_action(player).is_none());
    assert!(world.ability_cooldown_ready(player, definition.id));
    let body = world.player_body_of(player).expect("restored body");
    assert_eq!(body.velocity, [0.0, 0.0]);
    assert!(body.grounded);
    assert!(body.grounded_on.is_some());
    assert!(body.ignored_platform.is_none());
    assert_eq!(body.last_contact, crate::footnote::ContactEvent::None);
    assert_ne!(body.position, moved);
    assert!(!world.respawn_player_entity(player));
}
