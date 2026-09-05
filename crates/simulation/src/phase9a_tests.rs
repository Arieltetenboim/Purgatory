//! Phase 9A ability foundation. Contracts only — not Basic Attack gameplay.

use crate::ability::{
    AbilityActivation, AbilityDefinition, AbilityDelivery, AbilityEffect, AbilityRejectReason,
    AbilityRequest, AbilityTiming, cue_for_ability_cast, cue_for_damage_outcome,
    oneshot_kind_for_cue,
};
use crate::action::{ActionKind, ActionPhase};
use crate::action_gate::ActionGateContext;
use crate::presentation_oneshot::PresentationOneShotKind;
use crate::spawn::RuntimeSpawnRequest;
use crate::time::SimulationTick;
use crate::transform::Transform;
use crate::{Health, World, WorldAddress};
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
    }
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
fn strike_workload_is_not_the_ability_path() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let target = spawn_combatant(&mut world, 1.0, 10.0);
    let action = world
        .request_action(
            crate::ActionRequest {
                actor,
                target,
                kind: ActionKind::Strike,
            },
            ActionGateContext::in_world(),
        )
        .unwrap();
    assert_eq!(action.kind, ActionKind::Strike);
    assert_eq!(action.phase, ActionPhase::Active);
    assert!((world.health_of(target).unwrap().current - 9.0).abs() < 1e-5);
}

#[test]
fn presentation_boundary_uses_phase8_oneshot_vocabulary() {
    assert_eq!(
        oneshot_kind_for_cue(cue_for_ability_cast()),
        Some(PresentationOneShotKind::Attack)
    );
    assert_eq!(
        oneshot_kind_for_cue(cue_for_damage_outcome(false)),
        Some(PresentationOneShotKind::Hurt)
    );
    assert_eq!(oneshot_kind_for_cue(cue_for_damage_outcome(true)), None);
}

#[test]
fn zero_health_is_dead_without_a_dead_component() {
    let mut world = World::new();
    let id = spawn_combatant(&mut world, 0.0, 1.0);
    assert!(world.health_of(id).unwrap().is_alive());
    assert!(world.apply_damage(id, 1.0));
    assert!(world.health_of(id).unwrap().is_dead());
    assert!(
        world.contains(id),
        "player-like generic is not auto-despawned"
    );
}
