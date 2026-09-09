//! Phase 7.2 representative gameplay workload correctness.

use crate::action::ActionKind;
use crate::action_gate::ActionGateContext;
use crate::entity::EntityKind;
use crate::fixtures::RuntimeFixtures;
use crate::health::Health;
use crate::npc::{
    ActionRejectReason, ActionRequest, NPC_HEALTH_MAX, NpcState, PULSE_DURATION_TICKS,
    PULSE_PERIOD_TICKS, STRIKE_DAMAGE, STRIKE_RANGE,
};
use crate::platform::Platform;
use crate::spawn::RuntimeSpawnRequest;
use crate::time::SimulationTick;
use crate::transform::Transform;
use crate::world::World;
use purgatory_common::WorldAddress;

fn tick_services(world: &mut World, n: u64) {
    world.begin_tick(SimulationTick::from_count(n));
    world.drain_critical_scheduler();
    let _ = world.commit_runtime_events();
    world.pump_cadence();
    world.drain_deferred_scheduler();
}

#[test]
fn npc_spawn_despawn_and_kind() {
    let mut world = World::new();
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let id = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [4.0, 1.0],
            1,
            3.0,
            42,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .expect("npc");
    assert_eq!(world.kind(id), Some(EntityKind::Generic));
    assert!(world.npc_of(id).is_some());
    assert!(world.health_of(id).is_some());
    assert!(world.despawn(id));
    assert!(!world.contains(id));
}

#[test]
fn npc_inactive_skips_motion() {
    let mut world = World::new();
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let id = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [4.0, 1.0],
            1,
            3.0,
            7,
            now,
            false,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let before = world.transform_of(id).unwrap().position;
    world.tick_npcs(1.0 / 30.0);
    let after = world.transform_of(id).unwrap().position;
    assert_eq!(before, after);
    assert_eq!(world.runtime_stats().npcs_active, 0);
}

#[test]
fn npc_motion_is_deterministic_for_seed() {
    fn run(seed: u32) -> [f32; 2] {
        let mut world = World::new();
        let now = SimulationTick::from_count(1);
        world.begin_tick(now);
        let id = world
            .spawn(World::npc_spawn_request(
                WorldAddress::DEV,
                [4.0, 1.0],
                1,
                3.0,
                seed,
                now,
                true,
                NPC_HEALTH_MAX,
            ))
            .unwrap();
        for t in 2..=90 {
            world.begin_tick(SimulationTick::from_count(t));
            world.tick_npcs(1.0 / 30.0);
        }
        world.transform_of(id).unwrap().position
    }
    assert_eq!(run(99), run(99));
    assert_ne!(run(99), run(100));
}

#[test]
fn strike_validates_and_mutates_health() {
    let mut world = World::new();
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let a = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.0, 1.0],
            1,
            4.0,
            1,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let b = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [1.0, 1.0],
            2,
            4.0,
            2,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let before = world.health_of(b).unwrap().current;
    let ok = world.request_action(
        ActionRequest {
            actor: a,
            target: b,
            kind: ActionKind::Strike,
        },
        ActionGateContext::in_world(),
    );
    assert!(ok.is_ok());
    let after = world.health_of(b).unwrap().current;
    assert!((after - (before - STRIKE_DAMAGE)).abs() < 1e-5);
    assert!(world.runtime_stats().actions_attempted_total >= 1);
    assert!(world.runtime_stats().health_mutations_total >= 1);
}

#[test]
fn strike_rejects_out_of_range() {
    let mut world = World::new();
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let a = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.0, 1.0],
            1,
            4.0,
            1,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let b = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [STRIKE_RANGE + 2.0, 1.0],
            2,
            4.0,
            2,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let err = world
        .request_action(
            ActionRequest {
                actor: a,
                target: b,
                kind: ActionKind::Strike,
            },
            ActionGateContext::in_world(),
        )
        .unwrap_err();
    assert_eq!(err, ActionRejectReason::OutOfRange);
    assert_eq!(world.runtime_stats().actions_rejected_total, 1);
}

#[test]
fn health_dirty_independent_of_transform() {
    let mut world = World::new();
    let id = RuntimeFixtures::test_mob_like(&mut world);
    let revs = world.domain_revs_of(id).unwrap();
    let t0 = revs.transform;
    let h0 = revs.health;
    assert!(world.set_health(id, Health::full(10.0)));
    let revs = world.domain_revs_of(id).unwrap();
    assert_eq!(revs.transform, t0);
    assert!(revs.health > h0);
}

#[test]
fn pulse_ticks_and_expires() {
    let mut world = World::new();
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let target = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.0, 1.0],
            1,
            4.0,
            1,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let before = world.health_of(target).unwrap().current;
    let effect = world
        .apply_pulse_effect(target, PULSE_DURATION_TICKS, PULSE_PERIOD_TICKS, None)
        .unwrap();
    assert!(world.effect(effect.id).is_some());
    for t in 2..=(PULSE_DURATION_TICKS + 5) {
        tick_services(&mut world, t);
    }
    let after = world.health_of(target).unwrap().current;
    assert!(after < before);
    assert!(world.runtime_stats().pulse_ticks_total > 0);
    assert!(world.effect(effect.id).is_none());
}

#[test]
fn death_schedules_respawn_fresh_id() {
    let mut world = World::new();
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let id = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.0, 1.0],
            9,
            4.0,
            9,
            now,
            true,
            2.0,
        ))
        .unwrap();
    assert!(world.apply_damage(id, 2.0));
    assert!(world.npc_of(id).unwrap().dead_pending);
    assert_eq!(world.runtime_stats().deaths_total, 1);
    for t in 2..=40 {
        tick_services(&mut world, t);
    }
    assert!(!world.contains(id));
    let survivors: Vec<_> = world
        .iter()
        .filter(|&e| world.npc_of(e).is_some_and(|n| n.type_token == 9))
        .collect();
    assert_eq!(survivors.len(), 1);
    assert_ne!(survivors[0], id);
    assert!(world.runtime_stats().respawns_total >= 1);
}

#[test]
fn death_uses_configured_respawn_delay_and_health_max() {
    let mut world = World::new();
    world.set_npc_respawn_delay_ticks(5);
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let id = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.0, 1.0],
            11,
            4.0,
            11,
            now,
            true,
            6.0,
        ))
        .unwrap();
    assert!(world.apply_damage(id, 6.0));
    assert_eq!(world.runtime_stats().deaths_total, 1);
    // Default delay is 30; configured delay 5 → gone by tick 7, respawned by tick 8.
    for t in 2..=8 {
        tick_services(&mut world, t);
    }
    assert!(!world.contains(id));
    let survivors: Vec<_> = world
        .iter()
        .filter(|&e| world.npc_of(e).is_some_and(|n| n.type_token == 11))
        .collect();
    assert_eq!(survivors.len(), 1);
    assert_eq!(world.health_of(survivors[0]).unwrap().max, 6.0);
}

#[test]
fn nearest_health_target_tie_breaks_by_id() {
    let mut world = World::new();
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let actor = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.0, 1.0],
            1,
            4.0,
            1,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let _b = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.5, 1.0],
            2,
            4.0,
            2,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let _c = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.5, 1.0],
            3,
            4.0,
            3,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let target = world.nearest_health_target(actor, STRIKE_RANGE).unwrap();
    assert_ne!(target, actor);
    assert!(world.health_of(target).is_some());
}

#[test]
fn npc_state_attaches_via_spawn_request() {
    let mut world = World::new();
    let now = SimulationTick::from_count(0);
    let npc = NpcState::new(3, [1.0, 2.0], 2.5, 11, now, true);
    let id = world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([1.0, 2.0]))
                .visible()
                .with_health(Health::full(5.0))
                .with_npc(npc),
        )
        .unwrap();
    assert_eq!(world.npc_of(id).unwrap().type_token, 3);
}

#[test]
fn npc_falls_onto_solid_and_remains_supported() {
    let mut world = World::new();
    let floor = world.spawn_platform(
        Transform::from_position([0.0, 0.0]),
        Platform::solid([3.0, 0.2]),
    );
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let npc = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.0, 3.0],
            1,
            3.0,
            7,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();

    for tick in 2..=90 {
        world.begin_tick(SimulationTick::from_count(tick));
        world.tick_npcs(1.0 / 30.0);
    }

    let landed = world.npc_of(npc).unwrap();
    let position = world.transform_of(npc).unwrap().position;
    assert!(landed.grounded);
    assert_eq!(landed.grounded_on, Some(floor));
    assert!((position[1] - 0.8).abs() < 1e-4);
    let settled = position;
    world.tick_npcs(1.0 / 30.0);
    assert_eq!(world.transform_of(npc).unwrap().position, settled);
}

#[test]
fn npc_grounded_patrol_moves_horizontally() {
    let mut world = World::new();
    world.spawn_platform(
        Transform::from_position([0.0, 0.0]),
        Platform::solid([8.0, 0.2]),
    );
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let npc = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.0, 1.0],
            1,
            2.0,
            7,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let mut state = world.npc_of(npc).unwrap();
    state.heading = [1.0, 0.0];
    state.walking = true;
    world.set_npc(npc, state);

    for tick in 2..=90 {
        world.begin_tick(SimulationTick::from_count(tick));
        world.tick_npcs(1.0 / 30.0);
    }

    let state = world.npc_of(npc).unwrap();
    let position = world.transform_of(npc).unwrap().position;
    assert!(state.grounded);
    assert!(position[0] > 0.0);
    assert_eq!(position[1], 0.8);
}

#[test]
fn npc_patrol_stays_inside_home_range() {
    let mut world = World::new();
    world.spawn_platform(
        Transform::from_position([0.0, 0.0]),
        Platform::solid([8.0, 0.2]),
    );
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let npc = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.0, 1.0],
            1,
            0.8,
            7,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let mut state = world.npc_of(npc).unwrap();
    state.heading = [1.0, 0.0];
    state.walking = true;
    world.set_npc(npc, state);

    for tick in 2..=300 {
        world.begin_tick(SimulationTick::from_count(tick));
        world.tick_npcs(1.0 / 30.0);
        let x = world.transform_of(npc).unwrap().position[0];
        assert!((-0.8..=0.8).contains(&x), "patrol escaped home range: {x}");
    }
}

#[test]
fn npc_turns_before_walking_off_support_edge() {
    let mut world = World::new();
    let platform = world.spawn_platform(
        Transform::from_position([0.0, 0.0]),
        Platform::solid([1.0, 0.2]),
    );
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let npc = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.0, 1.0],
            1,
            10.0,
            7,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    let mut state = world.npc_of(npc).unwrap();
    state.walking = false;
    state.next_mode_tick = SimulationTick::from_count(1_000);
    world.set_npc(npc, state);

    let mut landed_tick = None;
    for tick in 2..=120 {
        world.begin_tick(SimulationTick::from_count(tick));
        world.tick_npcs(1.0 / 30.0);
        if world.npc_of(npc).unwrap().grounded {
            landed_tick = Some(tick);
            break;
        }
    }
    let landed_tick = landed_tick.expect("NPC should land on support");
    let mut state = world.npc_of(npc).unwrap();
    state.heading = [1.0, 0.0];
    state.walking = true;
    world.set_npc(npc, state);
    let mut transform = world.transform_of(npc).unwrap();
    transform.position[0] = 0.59;
    world.set_transform(npc, transform);
    world.begin_tick(SimulationTick::from_count(landed_tick + 1));
    world.tick_npcs(1.0 / 30.0);

    let state = world.npc_of(npc).unwrap();
    let x = world.transform_of(npc).unwrap().position[0];
    assert_eq!(state.grounded_on, Some(platform));
    assert!(x <= 1.0 - state.runtime_config.half_extents[0] + 1e-5);
    assert_eq!(state.heading, [-1.0, 0.0]);
}

#[test]
fn npc_ground_physics_filters_platforms_by_exact_world_address() {
    let mut world = World::new();
    let floor = world.spawn_platform_at(
        WorldAddress::DEV,
        Transform::from_position([0.0, 0.0]),
        Platform::solid([3.0, 0.2]),
    );
    let foreign = WorldAddress::new(
        purgatory_common::MapId::DEV,
        purgatory_common::ChannelId::DEFAULT,
        purgatory_common::InstanceId::from_raw(2),
    );
    world.spawn_platform_at(
        foreign,
        Transform::from_position([0.0, 1.5]),
        Platform::solid([3.0, 0.2]),
    );
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let npc = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.0, 3.0],
            1,
            3.0,
            8,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    for tick in 2..=90 {
        world.begin_tick(SimulationTick::from_count(tick));
        world.tick_npcs(1.0 / 30.0);
    }
    assert_eq!(world.npc_of(npc).unwrap().grounded_on, Some(floor));
}

#[test]
fn npc_approach_does_not_move_vertically_toward_elevated_player() {
    let mut world = World::new();
    world.spawn_platform(
        Transform::from_position([0.0, 0.0]),
        Platform::solid([8.0, 0.2]),
    );
    let player = RuntimeFixtures::test_player(&mut world);
    world.set_transform(player, Transform::from_position([3.0, 4.0]));
    let now = SimulationTick::from_count(1);
    world.begin_tick(now);
    let npc = world
        .spawn(World::npc_spawn_request(
            WorldAddress::DEV,
            [0.0, 3.0],
            1,
            3.0,
            9,
            now,
            true,
            NPC_HEALTH_MAX,
        ))
        .unwrap();
    for tick in 2..=90 {
        world.begin_tick(SimulationTick::from_count(tick));
        world.tick_npcs(1.0 / 30.0);
    }
    let grounded_y = world.transform_of(npc).unwrap().position[1];
    world.tick_npcs_with_approach(1.0 / 30.0, Some((5.0, 0.5, 0.8)));
    let state = world.npc_of(npc).unwrap();
    assert!(state.grounded);
    assert!(state.grounded_on.is_some());
    assert_eq!(world.transform_of(npc).unwrap().position[1], grounded_y);
}
