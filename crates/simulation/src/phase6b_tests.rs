//! Phase 6B authoritative interaction.

use crate::fixtures::RuntimeFixtures;
use crate::health::Health;
use crate::spawn::RuntimeSpawnRequest;
use crate::{
    INTERACT_RANGE, Interactable, InteractableKind, InteractionCloseReason, InteractionReject,
    InteractionSessionId, InteractionSessionState, Transform, World, WorldAddress,
};

fn actor_and_near(world: &mut World) -> (crate::EntityId, crate::EntityId) {
    let actor = RuntimeFixtures::test_player(world);
    let target = RuntimeFixtures::test_interactable(world);
    (actor, target)
}

#[test]
fn open_nearby_interactable() {
    let mut world = World::new();
    let (actor, target) = actor_and_near(&mut world);
    let session = world.try_open_interaction(actor, target).expect("open");
    assert_eq!(session.state, InteractionSessionState::Opened);
    assert_eq!(
        world.interaction_session_of(actor).unwrap().state,
        InteractionSessionState::Active
    );
}

#[test]
fn reject_out_of_range() {
    let mut world = World::new();
    let actor = RuntimeFixtures::test_player(&mut world);
    let target = world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([INTERACT_RANGE + 8.0, 1.0]))
                .visible()
                .with_interactable(Interactable::new(InteractableKind::Chest)),
        )
        .unwrap();
    assert_eq!(
        world.try_open_interaction(actor, target),
        Err(InteractionReject::OutOfRange)
    );
}

#[test]
fn reject_not_interactable() {
    let mut world = World::new();
    let actor = RuntimeFixtures::test_player(&mut world);
    let dummy = RuntimeFixtures::transient_replicated(&mut world);
    assert_eq!(
        world.try_open_interaction(actor, dummy),
        Err(InteractionReject::NotInteractable)
    );
}

#[test]
fn reject_wrong_address() {
    let mut world = World::new();
    let (actor, target) = actor_and_near(&mut world);
    world.set_address(
        target,
        WorldAddress::new(
            purgatory_common::MapId::DEV,
            purgatory_common::ChannelId::DEFAULT,
            purgatory_common::InstanceId::from_raw(2),
        ),
    );
    assert_eq!(
        world.try_open_interaction(actor, target),
        Err(InteractionReject::WrongAddress)
    );
}

#[test]
fn despawn_closes_session() {
    let mut world = World::new();
    let (actor, target) = actor_and_near(&mut world);
    let _ = world.try_open_interaction(actor, target).unwrap();
    assert!(world.despawn(target));
    assert!(world.interaction_session_of(actor).is_none());
}

#[test]
fn address_change_closes_session() {
    let mut world = World::new();
    let (actor, target) = actor_and_near(&mut world);
    let _ = world.try_open_interaction(actor, target).unwrap();
    world.set_address(
        actor,
        WorldAddress::new(
            purgatory_common::MapId::from_raw(2),
            purgatory_common::ChannelId::DEFAULT,
            purgatory_common::InstanceId::DEFAULT,
        ),
    );
    assert!(world.interaction_session_of(actor).is_none());
}

#[test]
fn stale_generation_cannot_open() {
    let mut world = World::new();
    let actor = RuntimeFixtures::test_player(&mut world);
    let target = RuntimeFixtures::test_interactable(&mut world);
    assert!(world.despawn(target));
    let reused = RuntimeFixtures::test_interactable(&mut world);
    assert_eq!(target.index(), reused.index());
    assert_ne!(target.generation(), reused.generation());
    assert_eq!(
        world.try_open_interaction(actor, target),
        Err(InteractionReject::StaleId)
    );
    assert!(world.try_open_interaction(actor, reused).is_ok());
}

#[test]
fn portal_activate_rejects_wrong_address_after_actor_moves_maps() {
    let mut world = World::new();
    let actor = RuntimeFixtures::test_player(&mut world);
    let actor_pos = world.transform_of(actor).unwrap().position;
    let portal = spawn_portal_at(&mut world, actor_pos);
    world
        .validate_portal_activate(actor, portal)
        .expect("same map");
    world.set_address(
        actor,
        WorldAddress::new(
            purgatory_common::MapId::from_raw(2),
            purgatory_common::ChannelId::DEFAULT,
            purgatory_common::InstanceId::DEFAULT,
        ),
    );
    assert_eq!(
        world.validate_portal_activate(actor, portal),
        Err(InteractionReject::WrongAddress)
    );
}

#[test]
fn stale_portal_generation_cannot_activate_replacement() {
    let mut world = World::new();
    let actor = RuntimeFixtures::test_player(&mut world);
    let actor_pos = world.transform_of(actor).unwrap().position;
    let portal = spawn_portal_at(&mut world, actor_pos);
    assert!(world.despawn(portal));
    let reused = spawn_portal_at(&mut world, actor_pos);
    assert_eq!(portal.index(), reused.index());
    assert_ne!(portal.generation(), reused.generation());
    assert_eq!(
        world.validate_portal_activate(actor, portal),
        Err(InteractionReject::StaleId)
    );
    world
        .validate_portal_activate(actor, reused)
        .expect("fresh portal id");
}

#[test]
fn close_requested_and_invalid_session() {
    let mut world = World::new();
    let (actor, target) = actor_and_near(&mut world);
    let session = world.try_open_interaction(actor, target).unwrap();
    assert!(world.close_interaction(actor, session.id).is_ok());
    assert_eq!(
        world.close_interaction(actor, session.id),
        Err(InteractionReject::InvalidSession)
    );
    assert_eq!(
        world.close_interaction(actor, InteractionSessionId(99)),
        Err(InteractionReject::InvalidSession)
    );
}

#[test]
fn walking_out_of_range_closes_on_maintain() {
    let mut world = World::new();
    let (actor, target) = actor_and_near(&mut world);
    let _ = world.try_open_interaction(actor, target).unwrap();
    assert!(world.set_transform(actor, Transform::from_position([80.0, 1.0])));
    let closed = world.maintain_interaction_sessions();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].1, InteractionCloseReason::OutOfRange);
    assert!(world.interaction_session_of(actor).is_none());
}

#[test]
fn dead_actor_closes_interaction_on_maintain_and_cannot_reopen() {
    let mut world = World::new();
    let (actor, target) = actor_and_near(&mut world);
    assert!(world.set_health(actor, Health::full(20.0)));
    world.try_open_interaction(actor, target).unwrap();

    assert!(world.set_health(
        actor,
        Health {
            current: 0.0,
            max: 20.0,
        },
    ));
    let closed = world.maintain_interaction_sessions();
    assert_eq!(closed.len(), 1);
    assert!(world.interaction_session_of(actor).is_none());
    assert_eq!(
        world.try_open_interaction(actor, target),
        Err(InteractionReject::Unavailable)
    );
}

#[test]
fn dead_target_closes_interaction_on_maintain_and_cannot_reopen() {
    let mut world = World::new();
    let (actor, target) = actor_and_near(&mut world);
    assert!(world.set_health(target, Health::full(20.0)));
    world.try_open_interaction(actor, target).unwrap();

    assert!(world.set_health(
        target,
        Health {
            current: 0.0,
            max: 20.0,
        },
    ));
    let closed = world.maintain_interaction_sessions();
    assert_eq!(closed.len(), 1);
    assert!(world.interaction_session_of(actor).is_none());
    assert_eq!(
        world.try_open_interaction(actor, target),
        Err(InteractionReject::Unavailable)
    );
}

#[test]
fn visible_is_not_sufficient_without_capability() {
    let mut world = World::new();
    let actor = RuntimeFixtures::test_player(&mut world);
    let visible = RuntimeFixtures::transient_replicated(&mut world);
    assert!(world.relevance_for(actor).contains(&visible));
    assert_eq!(
        world.try_open_interaction(actor, visible),
        Err(InteractionReject::NotInteractable)
    );
}

#[test]
fn disconnect_cleanup_uses_close_sessions() {
    let mut world = World::new();
    let (actor, target) = actor_and_near(&mut world);
    let _ = world.try_open_interaction(actor, target).unwrap();
    let closed = world.close_sessions_involving(actor, InteractionCloseReason::Disconnected);
    assert_eq!(closed.len(), 1);
    assert!(world.interaction_session_of(actor).is_none());
}

#[test]
fn footnote_dev_fixtures_enter_player_relevance() {
    use crate::stage::{FOOTNOTE_SPAWN_X, P0, P0_POSITION};
    use crate::{EntityLifecycle, ReplicationClass};

    let mut world = World::footnote_test_stage();
    if let Some(id) = world.player_id() {
        world.despawn(id);
    }
    let floor = world.iter_platforms().next().expect("platform");
    let (transform, state) =
        crate::PlayerState::standing_on_at(floor.id, floor.top_surface(), FOOTNOTE_SPAWN_X);
    let player = world.spawn_player(transform, state);
    let relevant = world.relevance_for(player);
    let interactables: Vec<_> = relevant
        .iter()
        .copied()
        .filter(|&id| world.interactable_of(id).is_some())
        .collect();
    assert_eq!(
        interactables.len(),
        2,
        "nearby + far on DEV; other-instance portal excluded"
    );
    for id in &interactables {
        assert_eq!(
            world.replication_of(*id).map(|m| m.class),
            Some(ReplicationClass::VisibleObservers)
        );
        assert_eq!(world.address_of(*id), Some(WorldAddress::DEV));
        assert_eq!(world.lifecycle_of(*id), Some(EntityLifecycle::Active));
        assert!(world.transform_of(*id).is_some());
    }
    let floor_top = P0.top_surface(Transform::from_position(P0_POSITION));
    let near = interactables
        .iter()
        .copied()
        .min_by(|a, b| {
            let ax = world.transform_of(*a).unwrap().position[0];
            let bx = world.transform_of(*b).unwrap().position[0];
            ax.partial_cmp(&bx).unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("near");
    let pos = world.transform_of(near).unwrap().position;
    assert!((pos[0] - (FOOTNOTE_SPAWN_X + 1.6)).abs() < 1e-3);
    assert!((pos[1] - (floor_top + 0.7)).abs() < 1e-3);
}

fn spawn_portal_at(world: &mut World, position: [f32; 2]) -> crate::EntityId {
    world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position(position))
                .visible()
                .with_interactable(Interactable::new(InteractableKind::Portal)),
        )
        .expect("portal")
}

#[test]
fn e_open_rejects_portal() {
    let mut world = World::new();
    let actor = RuntimeFixtures::test_player(&mut world);
    let actor_pos = world.transform_of(actor).unwrap().position;
    let portal = spawn_portal_at(&mut world, actor_pos);
    assert_eq!(
        world.try_open_interaction(actor, portal),
        Err(InteractionReject::NotInteractable)
    );
}

#[test]
fn portal_activate_requires_center_zone() {
    let mut world = World::new();
    let actor = RuntimeFixtures::test_player(&mut world);
    let actor_pos = world.transform_of(actor).unwrap().position;
    let portal = spawn_portal_at(&mut world, [actor_pos[0] + 1.6, actor_pos[1]]);
    assert_eq!(
        world.validate_portal_activate(actor, portal),
        Err(InteractionReject::OutOfRange)
    );
    let centered = spawn_portal_at(&mut world, actor_pos);
    world
        .validate_portal_activate(actor, centered)
        .expect("in zone");
}

#[test]
fn portal_reentry_lock_blocks_until_up_release() {
    let mut world = World::new();
    let actor = RuntimeFixtures::test_player(&mut world);
    let actor_pos = world.transform_of(actor).unwrap().position;
    let portal = spawn_portal_at(&mut world, actor_pos);
    world.validate_portal_activate(actor, portal).unwrap();
    world.lock_portal_reentry(actor, portal);
    assert_eq!(
        world.validate_portal_activate(actor, portal),
        Err(InteractionReject::ReentryLocked)
    );
    world.maintain_portal_reentry();
    assert_eq!(
        world.validate_portal_activate(actor, portal),
        Err(InteractionReject::ReentryLocked),
        "still in zone must not re-arm"
    );
    assert!(world.release_portal_reentry(actor));
    world
        .validate_portal_activate(actor, portal)
        .expect("unlocked after Up release while still in zone");
}

#[test]
fn portal_reentry_lock_also_clears_on_zone_exit() {
    let mut world = World::new();
    let actor = RuntimeFixtures::test_player(&mut world);
    let actor_pos = world.transform_of(actor).unwrap().position;
    let portal = spawn_portal_at(&mut world, actor_pos);
    world.lock_portal_reentry(actor, portal);
    let mut t = world.transform_of(actor).unwrap();
    t.position[0] = actor_pos[0] + 2.0;
    world.set_transform(actor, t);
    world.maintain_portal_reentry();
    let mut t = world.transform_of(actor).unwrap();
    t.position[0] = actor_pos[0];
    world.set_transform(actor, t);
    world
        .validate_portal_activate(actor, portal)
        .expect("unlocked after leaving zone");
}
