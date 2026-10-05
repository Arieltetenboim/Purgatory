//! Death / respawn presentation semantics and recovery gate (Slice C).

use crate::ability::{
    AbilityActivation, AbilityDefinition, AbilityDelivery, AbilityEffect, AbilityRequest,
    AbilityTiming,
};
use crate::action_gate::ActionGateContext;
use crate::health::Health;
use crate::input::PlayerInput;
use crate::presentation_oneshot::{PresentationOneShotKind, RESPAWN_RECOVERY_TICKS};
use crate::time::SimulationTick;
use crate::transform::Transform;
use crate::{ContentId, World};

const DT: f32 = 1.0 / 30.0;

fn sim_tick(world: &mut World, tick: u64, input: PlayerInput) {
    world.begin_tick(SimulationTick::from_count(tick));
    world.tick(DT, input);
}

fn strike_ability() -> AbilityDefinition {
    AbilityDefinition {
        id: ContentId::from_token(10_099),
        timing: AbilityTiming {
            windup_ticks: 1,
            active_ticks: 1,
            recovery_ticks: 1,
            cooldown_ticks: 5,
        },
        activation: AbilityActivation::Independent,
        delivery: AbilityDelivery::ForwardQuery {
            range: 2.0,
            half_height: 1.0,
            max_targets: 1,
        },
        effects: vec![AbilityEffect::Damage { amount: 1.0 }],
        presentation: crate::AbilityPresentation::Attack,
    }
}

fn kill_player(world: &mut World, id: crate::EntityId) {
    world.set_health(
        id,
        Health {
            current: 0.0,
            max: 20.0,
        },
    );
}

#[test]
fn respawn_starts_recovery_oneshot_and_spawn_immunity() {
    let mut world = World::dev_stage();
    let player = world.player_id().expect("player");
    kill_player(&mut world, player);

    assert!(world.respawn_player_entity(player));
    assert!(world.respawn_recovery_active(player));
    assert!(world.damage_immunity_active(player));
    assert_eq!(
        world.presentation_oneshot_of(player).unwrap().kind,
        PresentationOneShotKind::RespawnRecovery
    );
}

#[test]
fn recovery_gate_blocks_movement_and_abilities_until_expired() {
    let mut world = World::dev_stage();
    let player = world.player_id().expect("player");
    kill_player(&mut world, player);
    assert!(world.respawn_player_entity(player));
    let start_x = world.transform_of(player).unwrap().position[0];
    let ability = strike_ability();
    world.grant_ability(player, ability.id);

    sim_tick(&mut world, 1, PlayerInput::from_buttons(false, true, false));
    assert!(world.respawn_recovery_active(player));
    assert!(
        (world.transform_of(player).unwrap().position[0] - start_x).abs() < 1e-4,
        "recovery must suppress horizontal locomotion"
    );
    let denied = world.request_ability(
        AbilityRequest {
            actor: player,
            selected: None,
            definition: &ability,
        },
        ActionGateContext::in_world(),
    );
    assert_eq!(denied, Err(crate::ability::AbilityRejectReason::Busy));

    for tick in 2..=RESPAWN_RECOVERY_TICKS {
        sim_tick(&mut world, tick, PlayerInput::idle());
    }
    assert!(!world.respawn_recovery_active(player));
    assert!(
        world
            .request_ability(
                AbilityRequest {
                    actor: player,
                    selected: None,
                    definition: &ability,
                },
                ActionGateContext::in_world(),
            )
            .is_ok(),
        "abilities resume after recovery"
    );
}

#[test]
fn dead_presentation_oneshot_cleared_on_lethal_damage() {
    let mut world = World::dev_stage();
    let player = world.player_id().expect("player");
    world.set_health(player, Health::full(20.0));
    let _ = world.try_start_presentation_oneshot(player, PresentationOneShotKind::Attack);
    assert!(world.presentation_oneshot_of(player).is_some());

    world.apply_damage(player, 20.0);
    assert!(world.health_of(player).unwrap().is_dead());
    assert!(world.presentation_oneshot_of(player).is_none());
}

#[test]
fn respawn_placement_keeps_recovery_semantics() {
    let mut world = World::dev_stage();
    let player = world.player_id().expect("player");
    kill_player(&mut world, player);
    world.set_transform(player, Transform::from_position([9.0, 3.0]));

    assert!(world.restore_player_for_placement(player, true));
    assert!(world.respawn_recovery_active(player));
}
