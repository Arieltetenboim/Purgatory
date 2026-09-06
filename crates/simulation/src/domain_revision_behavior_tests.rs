//! Domain revision behavior tests: transform and health revision independence.

use crate::{Health, PlayerInput, World};

#[test]
fn transform_rev_bumps_when_velocity_changes_without_position() {
    let mut world = World::footnote_test_stage();
    let id = world.player_id().expect("player");
    let dt = 1.0 / 30.0;
    for _ in 0..8 {
        world.tick_player(id, dt, PlayerInput::from_buttons(false, true, false));
    }
    let mut last_pos = world.transform_of(id).unwrap().position;
    let mut last_vx = world.player_body_of(id).unwrap().velocity[0];
    let mut last_rev = world.domain_revs_of(id).unwrap().transform;
    assert!(last_vx > 1.0, "setup must be walking");
    for _ in 0..45 {
        world.tick_player(id, dt, PlayerInput::idle());
        let pos = world.transform_of(id).unwrap().position;
        let vx = world.player_body_of(id).unwrap().velocity[0];
        let rev = world.domain_revs_of(id).unwrap().transform;
        if pos == last_pos && vx != last_vx {
            assert!(
                rev > last_rev,
                "velocity-only rest tick must bump transform rev (vx {last_vx} -> {vx})"
            );
            return;
        }
        last_pos = pos;
        last_vx = vx;
        last_rev = rev;
    }
    panic!("never observed a velocity-only rest tick");
}

#[test]
fn transform_rev_bumps_only_on_change() {
    let mut world = World::dev_stage();
    let id = world.player_id().expect("player");
    let first = world.domain_revs_of(id).unwrap().transform;
    let t = world.transform_of(id).unwrap();
    assert!(world.set_transform(id, t));
    assert_eq!(world.domain_revs_of(id).unwrap().transform, first);
    let mut moved = t;
    moved.position[0] += 1.0;
    assert!(world.set_transform(id, moved));
    assert!(world.domain_revs_of(id).unwrap().transform > first);
}

#[test]
fn health_rev_is_independent() {
    let mut world = World::dev_stage();
    let id = world.player_id().expect("player");
    let t0 = world.domain_revs_of(id).unwrap().transform;
    assert!(world.set_health(id, Health::full(10.0)));
    let revs = world.domain_revs_of(id).unwrap();
    assert_eq!(revs.transform, t0);
    assert_eq!(revs.health, 1);
}
