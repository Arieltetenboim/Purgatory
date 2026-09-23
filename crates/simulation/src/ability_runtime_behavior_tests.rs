//! Ability runtime behavior tests (Phase 9 -> permanent).
//! Generic ability lifecycle, effects, cooldown and grants.

use crate::ability::{
    AbilityActivation, AbilityDefinition, AbilityDelivery, AbilityEffect, AbilityId,
    AbilityRejectReason, AbilityRequest, AbilityTiming,
};
use crate::action::{ActionKind, ActionPhase};
use crate::action_gate::ActionGateContext;
use crate::platform::Platform;
use crate::spawn::RuntimeSpawnRequest;
use crate::time::SimulationTick;
use crate::transform::Transform;
use crate::{Health, PlayerInput, PlayerState, PresentationOneShotKind, World, WorldAddress};
use purgatory_common::ContentId;

fn tick_critical(world: &mut World, tick: u64) {
    world.begin_tick(SimulationTick::from_count(tick));
    world.drain_critical_scheduler();
}

fn spawn_combatant(world: &mut World, x: f32, hp: f32) -> crate::EntityId {
    world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([x, 1.0]))
                .with_health(Health::full(hp))
                .visible(),
        )
        .expect("spawn")
}

fn sample_ability() -> AbilityDefinition {
    AbilityDefinition {
        id: ContentId::from_authored("skill.basic.strike").unwrap(),
        timing: AbilityTiming {
            windup_ticks: 2,
            active_ticks: 1,
            recovery_ticks: 2,
            cooldown_ticks: 8,
        },
        activation: AbilityActivation::Independent,
        delivery: AbilityDelivery::ForwardQuery {
            range: 4.0,
            half_height: 1.0,
            max_targets: 8,
        },
        effects: vec![AbilityEffect::Damage { amount: 3.0 }],
        presentation: crate::AbilityPresentation::Attack,
    }
}

fn dash_ability() -> AbilityDefinition {
    AbilityDefinition {
        id: ContentId::from_authored("skill.movement.dash").unwrap(),
        timing: AbilityTiming {
            windup_ticks: 0,
            active_ticks: 5,
            recovery_ticks: 3,
            cooldown_ticks: 60,
        },
        activation: AbilityActivation::Independent,
        delivery: AbilityDelivery::SelfTarget,
        effects: vec![AbilityEffect::Dash {
            speed: 9.0,
            duration_ticks: 5,
        }],
        presentation: crate::AbilityPresentation::Dash,
    }
}

fn dash_stage(with_wall: bool) -> (World, crate::EntityId) {
    let mut world = World::new();
    let floor = world.spawn_platform(
        Transform::from_position([0.0, 0.0]),
        Platform::solid([10.0, 0.5]),
    );
    if with_wall {
        world.spawn_platform(
            Transform::from_position([1.4, 1.2]),
            Platform::solid([0.2, 2.0]),
        );
    }
    let (transform, player) = PlayerState::standing_on_at(floor, 0.5, 0.0);
    let actor = world.spawn_player(transform, player);
    world.set_health(actor, Health::full(20.0));
    (world, actor)
}

#[test]
fn player_and_platform_have_no_combat_component_by_default() {
    let mut world = World::new();
    let player = crate::fixtures::RuntimeFixtures::test_player(&mut world);
    assert!(world.health_of(player).is_none());
    assert!(world.active_action(player).is_none());
    assert!(world.ability_cooldown_ready(player, ContentId::from_token(1)));
}

#[test]
fn windup_does_not_apply_damage_until_active() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let target = spawn_combatant(&mut world, 1.0, 10.0);
    let def = sample_ability();
    let started = world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &def,
            },
            ActionGateContext::in_world(),
        )
        .expect("start");
    assert_eq!(started.phase, ActionPhase::Windup);
    assert_eq!(started.kind, ActionKind::Ability { id: def.id });
    assert_eq!(world.health_of(target).unwrap().current, 10.0);
    assert!(world.active_action(actor).is_some());

    tick_critical(&mut world, 3);
    let live = world.active_action(actor).unwrap();
    assert_eq!(live.phase, ActionPhase::Active);
    assert!((world.health_of(target).unwrap().current - 7.0).abs() < 1e-5);
}

#[test]
fn ability_damage_uses_effect_boundary_not_set_health() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let target = spawn_combatant(&mut world, 1.0, 10.0);
    let applied =
        world.execute_ability_effect(actor, target, AbilityEffect::Damage { amount: 4.0 });
    assert!(applied);
    assert!((world.health_of(target).unwrap().current - 6.0).abs() < 1e-5);
    assert!(world.health_of(target).unwrap().is_alive());
}

#[test]
fn missing_health_is_nonparticipant_not_dead() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let bare = world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([1.0, 1.0]))
                .visible(),
        )
        .expect("bare");
    assert!(world.health_of(bare).is_none());
    let def = sample_ability();
    world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &def,
            },
            ActionGateContext::in_world(),
        )
        .expect("activation does not require a target");
    tick_critical(&mut world, 3);
    assert!(world.health_of(bare).is_none());
}

#[test]
fn cooldown_blocks_second_start_on_same_ability() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let def = sample_ability();
    world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &def,
            },
            ActionGateContext::in_world(),
        )
        .unwrap();
    assert!(!world.ability_cooldown_ready(actor, def.id));
    let denied = world.request_ability(
        AbilityRequest {
            actor,
            selected: None,
            definition: &def,
        },
        ActionGateContext::in_world(),
    );
    assert_eq!(denied, Err(AbilityRejectReason::OnCooldown));
}

#[test]
fn live_recovery_still_occupies_exclusive_action_slot() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let def = sample_ability();
    world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &def,
            },
            ActionGateContext::in_world(),
        )
        .unwrap();
    tick_critical(&mut world, 3);
    tick_critical(&mut world, 4);
    let live = world.active_action(actor).unwrap();
    assert_eq!(live.phase, ActionPhase::Recovery);
    let busy = world.try_start_action(
        actor,
        ActionKind::Test { token: 1 },
        ActionGateContext::in_world(),
    );
    assert_eq!(busy, Err(crate::ActionDenialReason::Busy));
    tick_critical(&mut world, 6);
    assert!(world.active_action(actor).is_none());
}

#[test]
fn grant_is_explicit_and_dropped_on_despawn() {
    let mut world = World::new();
    let owner = world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([0.0, 1.0]))
                .visible(),
        )
        .unwrap();
    let strike_id = AbilityId::from(ContentId::from_authored("skill.basic.strike").unwrap());
    assert!(!world.ability_granted(owner, strike_id));
    assert!(world.grant_ability(owner, strike_id));
    assert!(world.ability_granted(owner, strike_id));
    assert!(world.despawn(owner));
    assert!(!world.ability_granted(owner, strike_id));
}

#[test]
fn dash_is_ground_only_and_starts_semantic_dash_presentation() {
    let (mut world, actor) = dash_stage(false);
    let definition = dash_ability();
    world.player_parts_mut_for(actor).unwrap().1.grounded = false;
    assert_eq!(
        world.request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &definition,
            },
            ActionGateContext::in_world(),
        ),
        Err(AbilityRejectReason::RequiresGrounded)
    );

    world.player_parts_mut_for(actor).unwrap().1.grounded = true;
    world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &definition,
            },
            ActionGateContext::in_world(),
        )
        .expect("grounded dash starts");
    assert!(world.player_dash_of(actor).is_some());
    assert_eq!(
        world.presentation_oneshot_of(actor).unwrap().kind,
        PresentationOneShotKind::Dash
    );
}

#[test]
fn dash_locks_direction_ignores_jump_and_stops_after_authored_ticks() {
    let (mut world, actor) = dash_stage(false);
    let definition = dash_ability();
    world.note_player_horizontal_intent(actor, -1);
    world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &definition,
            },
            ActionGateContext::in_world(),
        )
        .unwrap();

    let dt = 1.0 / 30.0;
    for _ in 0..5 {
        world.tick_player(
            actor,
            dt,
            PlayerInput::from_buttons(false, true, true),
        );
    }
    let body = world.player_body_of(actor).unwrap();
    assert!((body.position[0] + 1.5).abs() < 1e-4);
    assert_eq!(body.velocity, [0.0, 0.0]);
    assert!(body.grounded);
    assert!(world.player_dash_of(actor).is_none());
}

#[test]
fn dash_locks_facing_against_mid_dash_horizontal_input() {
    let (mut world, actor) = dash_stage(false);
    world.note_player_horizontal_intent(actor, -1);
    assert!(world.start_player_dash(actor, 9.0, 5));

    world.note_player_horizontal_intent(actor, 1);
    let (_, player) = world.player_parts_mut_for(actor).expect("player");
    assert_eq!(player.facing_sign, -1, "Dash must own facing until movement ends");
    assert_eq!(player.dash.expect("Dash active").direction, -1);

    world.tick_player(
        actor,
        1.0 / 30.0,
        PlayerInput::from_buttons(false, true, false),
    );
    let (_, player) = world.player_parts_mut_for(actor).expect("player");
    assert_eq!(player.facing_sign, -1);
    assert!(player.velocity[0] < 0.0, "opposite held input must not steer Dash");
}

#[test]
fn dash_movement_blocks_other_abilities_even_without_action_table_state() {
    let (mut world, actor) = dash_stage(false);
    assert!(world.start_player_dash(actor, 9.0, 5));
    assert!(world.active_action(actor).is_none(), "fixture isolates movement-state gate");

    let strike = sample_ability();
    assert_eq!(
        world.request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &strike,
            },
            ActionGateContext::in_world(),
        ),
        Err(AbilityRejectReason::Busy)
    );
}

#[test]
fn dash_uses_normal_collision_and_stops_at_wall() {
    let (mut world, actor) = dash_stage(true);
    let definition = dash_ability();
    world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &definition,
            },
            ActionGateContext::in_world(),
        )
        .unwrap();
    for _ in 0..5 {
        world.tick_player(actor, 1.0 / 30.0, PlayerInput::idle());
    }
    let body = world.player_body_of(actor).unwrap();
    assert!(body.position[0] <= 0.8001, "wall stop x={}", body.position[0]);
    assert_eq!(body.velocity[0], 0.0);
    assert!(world.player_dash_of(actor).is_none());
}

#[test]
fn dash_clears_horizontal_drive_when_support_ends() {
    let mut world = World::new();
    let floor = world.spawn_platform(
        Transform::from_position([0.0, 0.0]),
        Platform::solid([0.7, 0.5]),
    );
    let (transform, player) = PlayerState::standing_on_at(floor, 0.5, 0.0);
    let actor = world.spawn_player(transform, player);
    world.set_health(actor, Health::full(20.0));
    let definition = dash_ability();
    world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &definition,
            },
            ActionGateContext::in_world(),
        )
        .unwrap();

    for _ in 0..5 {
        world.tick_player(actor, 1.0 / 30.0, PlayerInput::idle());
    }
    let body = world.player_body_of(actor).unwrap();
    assert!(!body.grounded);
    assert_eq!(body.velocity[0], 0.0);
    assert!(world.player_dash_of(actor).is_none());
}

#[test]
fn actual_damage_interrupts_dash_action_and_movement() {
    let (mut world, actor) = dash_stage(false);
    let definition = dash_ability();
    world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &definition,
            },
            ActionGateContext::in_world(),
        )
        .unwrap();
    let _ = world.consume_dirty(actor);
    assert!(world.apply_damage(actor, 1.0));
    assert!(world.active_action(actor).is_none());
    assert!(world.player_dash_of(actor).is_none());
    assert_eq!(world.player_body_of(actor).unwrap().velocity[0], 0.0);
    let dirty = world.dirty_of(actor).expect("player dirty flags");
    assert!(dirty.transform, "Dash interruption must replicate zero velocity");
    assert!(dirty.health);
    assert_eq!(
        world.presentation_oneshot_of(actor).unwrap().kind,
        PresentationOneShotKind::Hurt
    );
}

#[test]
fn actual_damage_interrupts_dash_recovery_after_movement_ends() {
    let (mut world, actor) = dash_stage(false);
    let definition = dash_ability();
    world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &definition,
            },
            ActionGateContext::in_world(),
        )
        .unwrap();
    for _ in 0..5 {
        world.tick_player(actor, 1.0 / 30.0, PlayerInput::idle());
    }
    tick_critical(&mut world, 5);
    assert!(world.player_dash_of(actor).is_none());
    assert_eq!(
        world.active_action(actor).unwrap().phase,
        ActionPhase::Recovery
    );

    assert!(world.apply_damage(actor, 1.0));
    assert!(world.active_action(actor).is_none());
    assert_eq!(
        world.presentation_oneshot_of(actor).unwrap().kind,
        PresentationOneShotKind::Hurt
    );
}
