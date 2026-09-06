//! Combat presentation behavior tests (Phase 9D -> permanent).
//! Attack/Hurt/Dead semantic presentation behavior.

use crate::ability::{
    AbilityActivation, AbilityDefinition, AbilityDelivery, AbilityEffect, AbilityRequest,
    AbilityTiming, cue_for_ability_cast, cue_for_damage_outcome, oneshot_kind_for_cue,
};
use crate::action_gate::ActionGateContext;
use crate::presentation_oneshot::PresentationOneShotKind;
use crate::runtime_event::RuntimeEvent;
use crate::spawn::RuntimeSpawnRequest;
use crate::time::SimulationTick;
use crate::transform::Transform;
use crate::{Health, World, WorldAddress};
use purgatory_common::ContentId;

fn strike_like(id: &str) -> AbilityDefinition {
    AbilityDefinition {
        id: ContentId::from_authored(id).unwrap(),
        timing: AbilityTiming {
            windup_ticks: 3,
            active_ticks: 2,
            recovery_ticks: 4,
            cooldown_ticks: 12,
        },
        activation: AbilityActivation::Independent,
        delivery: AbilityDelivery::ForwardQuery {
            range: 1.5,
            half_height: 0.8,
            max_targets: 8,
        },
        effects: vec![AbilityEffect::Damage { amount: 5.0 }],
    }
}

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

fn drain_oneshot_starts(world: &mut World) -> Vec<(crate::EntityId, PresentationOneShotKind)> {
    world
        .commit_runtime_events()
        .into_iter()
        .filter_map(|e| match e {
            RuntimeEvent::PresentationOneShotStarted { entity, kind, .. } => Some((entity, kind)),
            _ => None,
        })
        .collect()
}

#[test]
fn ability_execution_produces_one_attack_cue() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let def = strike_like("skill.test.cleave");
    world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &def,
            },
            ActionGateContext::in_world(),
        )
        .expect("start");
    let starts = drain_oneshot_starts(&mut world);
    assert_eq!(starts, vec![(actor, PresentationOneShotKind::Attack)]);
    assert_eq!(
        world.presentation_oneshot_of(actor).map(|o| o.kind),
        Some(PresentationOneShotKind::Attack)
    );
}

#[test]
fn empty_ability_execution_still_produces_attack() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let def = strike_like("skill.test.empty_swing");
    world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &def,
            },
            ActionGateContext::in_world(),
        )
        .expect("empty space is valid");
    tick_critical(&mut world, 4);
    assert_eq!(
        world.presentation_oneshot_of(actor).map(|o| o.kind),
        Some(PresentationOneShotKind::Attack),
        "empty Active must not clear Attack"
    );
    assert_eq!(world.health_of(actor).unwrap().current, 10.0);
}

#[test]
fn authoritative_damage_produces_hurt() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let target = spawn_combatant(&mut world, 1.0, 10.0);
    let _ = world.commit_runtime_events();
    assert!(world.apply_damage(target, 3.0));
    let starts = drain_oneshot_starts(&mut world);
    assert_eq!(starts, vec![(target, PresentationOneShotKind::Hurt)]);
    assert!((world.health_of(target).unwrap().current - 7.0).abs() < 1e-5);
}

#[test]
fn no_damage_produces_no_hurt() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let bare = world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([1.0, 1.0]))
                .visible(),
        )
        .unwrap();
    let _ = world.commit_runtime_events();
    assert!(!world.apply_damage(bare, 5.0));
    assert!(drain_oneshot_starts(&mut world).is_empty());
}

#[test]
fn lethal_damage_clears_oneshot_and_is_dead_state() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let target = spawn_combatant(&mut world, 1.0, 5.0);
    let _ = world.try_start_presentation_oneshot(target, PresentationOneShotKind::Attack);
    let _ = world.commit_runtime_events();
    assert!(world.apply_damage(target, 5.0));
    let events = world.commit_runtime_events();
    assert!(events.iter().any(
        |e| matches!(e, RuntimeEvent::PresentationOneShotCleared { entity } if *entity == target)
    ));
    assert!(
        !events.iter().any(|e| matches!(
            e,
            RuntimeEvent::PresentationOneShotStarted {
                kind: PresentationOneShotKind::Hurt,
                ..
            }
        )),
        "lethal must not leave a transient Hurt"
    );
    assert!(world.health_of(target).unwrap().is_dead());
    assert!(world.presentation_oneshot_of(target).is_none());
}

#[test]
fn already_dead_damage_does_not_restart_hurt() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let target = spawn_combatant(&mut world, 1.0, 1.0);
    assert!(world.apply_damage(target, 1.0));
    let _ = world.commit_runtime_events();
    assert!(world.apply_damage(target, 1.0));
    assert!(
        drain_oneshot_starts(&mut world).is_empty(),
        "dead target must not restart Hurt"
    );
}

#[test]
fn ability_hit_path_emits_attack_then_hurt_not_tied_to_basic_strike_id() {
    let mut world = World::new();
    tick_critical(&mut world, 1);
    let actor = spawn_combatant(&mut world, 0.0, 10.0);
    let target = spawn_combatant(&mut world, 1.0, 10.0);
    let def = strike_like("skill.test.forward_cleave");
    world
        .request_ability(
            AbilityRequest {
                actor,
                selected: None,
                definition: &def,
            },
            ActionGateContext::in_world(),
        )
        .expect("start");
    let after_start = drain_oneshot_starts(&mut world);
    assert_eq!(after_start, vec![(actor, PresentationOneShotKind::Attack)]);
    tick_critical(&mut world, 4);
    let after_active = drain_oneshot_starts(&mut world);
    assert_eq!(after_active, vec![(target, PresentationOneShotKind::Hurt)]);
    assert!((world.health_of(target).unwrap().current - 5.0).abs() < 1e-5);
}

#[test]
fn presentation_cues_remain_semantic_not_delivery_specific() {
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
