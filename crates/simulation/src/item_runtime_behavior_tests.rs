//! Phase 11B1 item runtime canonical-state and world-drop invariants.

use crate::{
    ContentId, INVENTORY_CAPACITY, ItemLocation, ItemRuntimeError, RuntimeSpawnRequest, Transform,
    World, WorldAddress,
};
use purgatory_common::ItemInstanceId;

fn content(token: u64) -> ContentId {
    ContentId::from_token(token)
}

fn spawn_drop_entity(world: &mut World) -> crate::EntityId {
    world
        .spawn(
            RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                .with_transform(Transform::from_position([1.0, 1.0]))
                .visible(),
        )
        .expect("drop entity")
}

fn spawn_drop_for_player(
    world: &mut World,
    position: [f32; 2],
) -> (ItemInstanceId, crate::EntityId) {
    world
        .spawn_world_drop_item(WorldAddress::DEV, position, content(200), 1, 1)
        .expect("spawn pickup drop")
}

#[test]
fn live_instance_has_one_canonical_record_and_one_drop_entity() {
    let mut world = World::new();
    let (item, entity) = world
        .spawn_world_drop_item(WorldAddress::DEV, [0.0, 1.0], content(100), 3, 20)
        .expect("spawn item");
    assert_eq!(world.item_record_count(), 1);
    let record = world.item_record(item).expect("canonical record");
    assert_eq!(record.definition, content(100));
    assert_eq!(record.quantity, 3);
    assert_eq!(record.location, ItemLocation::WorldDrop(entity));
    assert_eq!(world.item_instance_at_world_drop(entity), Some(item));
    assert_eq!(world.world_drop_entity_for_item(item), Some(entity));
}

#[test]
fn duplicate_instance_insertion_is_rejected() {
    let mut world = World::new();
    let forced = ItemInstanceId::from_raw(77);
    world
        .spawn_world_drop_item_with_instance(
            forced,
            WorldAddress::DEV,
            [0.0, 1.0],
            content(1),
            1,
            1,
        )
        .expect("first insert");
    let err = world
        .spawn_world_drop_item_with_instance(
            forced,
            WorldAddress::DEV,
            [2.0, 1.0],
            content(1),
            1,
            1,
        )
        .expect_err("duplicate rejected");
    assert_eq!(err, ItemRuntimeError::DuplicateInstance(forced));
    assert_eq!(world.item_record_count(), 1);
}

#[test]
fn quantity_must_respect_item_definition_contract() {
    let mut world = World::new();
    let err = world
        .spawn_world_drop_item(WorldAddress::DEV, [0.0, 1.0], content(2), 0, 1)
        .expect_err("zero quantity rejected");
    assert_eq!(
        err,
        ItemRuntimeError::InvalidQuantity {
            quantity: 0,
            stack_limit: 1
        }
    );
    let err = world
        .spawn_world_drop_item(WorldAddress::DEV, [0.0, 1.0], content(2), 3, 2)
        .expect_err("over stack rejected");
    assert_eq!(
        err,
        ItemRuntimeError::InvalidQuantity {
            quantity: 3,
            stack_limit: 2
        }
    );
    let err = world
        .spawn_world_drop_item(WorldAddress::DEV, [0.0, 1.0], content(2), 1, 0)
        .expect_err("invalid stack limit rejected");
    assert_eq!(err, ItemRuntimeError::InvalidStackLimit);
    assert_eq!(world.item_record_count(), 0);
}

#[test]
fn two_instances_cannot_claim_the_same_drop_manifestation() {
    let mut world = World::new();
    let drop = spawn_drop_entity(&mut world);
    let a = ItemInstanceId::from_raw(100);
    let b = ItemInstanceId::from_raw(101);
    world
        .bind_world_drop_item_instance(a, content(3), 1, 1, drop)
        .expect("first bind");
    let err = world
        .bind_world_drop_item_instance(b, content(4), 1, 1, drop)
        .expect_err("duplicate drop claim rejected");
    assert_eq!(err, ItemRuntimeError::WorldDropAlreadyClaimed(drop));
    assert_eq!(world.item_instance_at_world_drop(drop), Some(a));
    assert!(world.item_record(b).is_none());
}

#[test]
fn despawned_drop_entities_cannot_remain_valid_item_state() {
    let mut world = World::new();
    let (item, entity) = world
        .spawn_world_drop_item(WorldAddress::DEV, [0.0, 1.0], content(5), 1, 1)
        .expect("spawn item");
    assert!(world.despawn(entity));
    assert!(world.item_record(item).is_none());
    assert!(world.item_instance_at_world_drop(entity).is_none());
    assert!(world.world_drop_entity_for_item(item).is_none());
}

#[test]
fn destroying_world_drop_item_cleans_both_sides() {
    let mut world = World::new();
    let (item, entity) = world
        .spawn_world_drop_item(WorldAddress::DEV, [0.0, 1.0], content(6), 1, 1)
        .expect("spawn item");
    assert!(world.destroy_world_drop_item(item));
    assert!(!world.contains(entity));
    assert!(world.item_record(item).is_none());
    assert!(world.item_instance_at_world_drop(entity).is_none());
}

#[test]
fn failed_creation_keeps_existing_state_unchanged() {
    let mut world = World::new();
    let (item, entity) = world
        .spawn_world_drop_item(WorldAddress::DEV, [0.0, 1.0], content(7), 2, 5)
        .expect("spawn item");
    let before = world.item_record(item).expect("record");
    let err = world
        .spawn_world_drop_item(WorldAddress::DEV, [0.0, 1.0], content(7), 9, 5)
        .expect_err("invalid create rejected");
    assert_eq!(
        err,
        ItemRuntimeError::InvalidQuantity {
            quantity: 9,
            stack_limit: 5
        }
    );
    assert_eq!(world.item_record(item), Some(before));
    assert_eq!(world.world_drop_entity_for_item(item), Some(entity));
    assert_eq!(world.item_record_count(), 1);
}

#[test]
fn failed_mutation_keeps_existing_state_unchanged() {
    let mut world = World::new();
    let (item, _) = world
        .spawn_world_drop_item(WorldAddress::DEV, [0.0, 1.0], content(8), 2, 5)
        .expect("spawn item");
    let before = world.item_record(item).expect("record");
    let err = world
        .set_item_quantity(item, 7, 5)
        .expect_err("invalid mutation rejected");
    assert_eq!(
        err,
        ItemRuntimeError::InvalidQuantity {
            quantity: 7,
            stack_limit: 5
        }
    );
    assert_eq!(world.item_record(item), Some(before));
}

#[test]
fn spawn_path_mints_authoritative_item_instance_ids() {
    let mut world = World::new();
    let (a, _) = world
        .spawn_world_drop_item(WorldAddress::DEV, [0.0, 1.0], content(9), 1, 1)
        .expect("spawn a");
    let (b, _) = world
        .spawn_world_drop_item(WorldAddress::DEV, [1.0, 1.0], content(9), 1, 1)
        .expect("spawn b");
    assert_ne!(a, b);
    assert_ne!(a.raw(), 0);
    assert_ne!(b.raw(), 0);
}

#[test]
fn pickup_moves_item_once_and_despawns_manifestation() {
    let mut world = World::dev_stage();
    let actor = world.player_id().expect("player");
    let position = world
        .transform_of(actor)
        .expect("player transform")
        .position;
    let (item, entity) = spawn_drop_for_player(&mut world, position);

    let result = world.pickup_world_drop(actor, entity).expect("pickup");

    assert_eq!(result, (item, 0));
    assert!(!world.contains(entity));
    assert_eq!(
        world.item_record(item).map(|record| record.location),
        Some(ItemLocation::Inventory {
            owner: actor,
            slot: 0
        })
    );
    assert_eq!(world.inventory_item(actor, 0), Some(item));
    assert_eq!(world.inventory_count(actor), 1);
}

#[test]
fn pickup_rejects_stale_target_without_mutating_inventory() {
    let mut world = World::dev_stage();
    let actor = world.player_id().expect("player");
    let position = world
        .transform_of(actor)
        .expect("player transform")
        .position;
    let (_, entity) = spawn_drop_for_player(&mut world, position);
    assert!(world.despawn(entity));

    assert_eq!(
        world.pickup_world_drop(actor, entity),
        Err(ItemRuntimeError::PickupTargetMissing(entity))
    );
    assert_eq!(world.inventory_count(actor), 0);
}

#[test]
fn pickup_rejects_out_of_range_target_without_mutating_drop() {
    let mut world = World::dev_stage();
    let actor = world.player_id().expect("player");
    let (item, entity) = spawn_drop_for_player(&mut world, [100.0, 100.0]);

    assert_eq!(
        world.pickup_world_drop(actor, entity),
        Err(ItemRuntimeError::PickupOutOfRange)
    );
    assert_eq!(world.item_instance_at_world_drop(entity), Some(item));
    assert_eq!(
        world.item_record(item).map(|record| record.location),
        Some(ItemLocation::WorldDrop(entity))
    );
}

#[test]
fn duplicate_pickup_is_rejected_after_first_success() {
    let mut world = World::dev_stage();
    let actor = world.player_id().expect("player");
    let position = world
        .transform_of(actor)
        .expect("player transform")
        .position;
    let (item, entity) = spawn_drop_for_player(&mut world, position);

    world
        .pickup_world_drop(actor, entity)
        .expect("first pickup");

    assert_eq!(
        world.pickup_world_drop(actor, entity),
        Err(ItemRuntimeError::PickupTargetMissing(entity))
    );
    assert_eq!(world.inventory_item(actor, 0), Some(item));
}

#[test]
fn pickup_rejects_when_minimal_inventory_is_full() {
    let mut world = World::dev_stage();
    let actor = world.player_id().expect("player");
    let position = world
        .transform_of(actor)
        .expect("player transform")
        .position;
    for _ in 0..INVENTORY_CAPACITY {
        let (_, entity) = spawn_drop_for_player(&mut world, position);
        world
            .pickup_world_drop(actor, entity)
            .expect("fill inventory");
    }
    let (item, entity) = spawn_drop_for_player(&mut world, position);

    assert_eq!(
        world.pickup_world_drop(actor, entity),
        Err(ItemRuntimeError::InventoryFull(actor))
    );
    assert_eq!(world.inventory_count(actor), INVENTORY_CAPACITY);
    assert_eq!(world.item_instance_at_world_drop(entity), Some(item));
}

#[test]
fn inventory_move_and_remove_preserve_canonical_instance_identity() {
    let mut world = World::dev_stage();
    let actor = world.player_id().expect("player");
    let position = world.transform_of(actor).expect("transform").position;
    let (item, entity) = spawn_drop_for_player(&mut world, position);
    world.pickup_world_drop(actor, entity).expect("pickup");

    assert_eq!(world.move_inventory_item(actor, 0, 3), Ok(item));
    assert_eq!(world.inventory_item(actor, 0), None);
    assert_eq!(world.inventory_item(actor, 3), Some(item));
    assert_eq!(
        world.item_record(item).map(|record| record.location),
        Some(ItemLocation::Inventory {
            owner: actor,
            slot: 3
        })
    );
    assert_eq!(world.inventory_count(actor), 1);

    let removed = world.remove_inventory_item(actor, 3).expect("remove");
    assert_eq!(removed.0, item);
    assert!(world.item_record(item).is_none());
    assert_eq!(world.inventory_count(actor), 0);
}
