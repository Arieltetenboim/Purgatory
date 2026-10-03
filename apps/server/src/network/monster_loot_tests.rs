use purgatory_common::{
    ITEM_DEV_SAMPLE_SCRAP, ITEM_IRON_SCRAP, ITEM_SMALL_POTION, MONSTER_DEV_GUARANTEED_DROP,
    MONSTER_DEV_MIXED_DROPS,
};

fn reserved(raw: u64) -> purgatory_common::ItemInstanceId {
    purgatory_common::ItemInstanceId::from_raw(raw)
}

fn wire(entity: purgatory_simulation::EntityId) -> WireEntityId {
    WireEntityId {
        index: entity.index(),
        generation: entity.generation(),
    }
}

fn session(owner: &mut GameplayOwner, connection: u64, character: u64) -> purgatory_simulation::EntityId {
    let connection_id = ConnectionId::from_raw(connection);
    owner.attach(connection_id);
    owner.lease_for_test(
        connection_id,
        CharacterId::from_raw(character),
        0,
        "dev.local",
        1,
        &[],
    );
    owner.entity_of(connection_id).expect("player")
}

fn spawn_monster(
    owner: &mut GameplayOwner,
    connection: ConnectionId,
    monster: purgatory_common::ContentId,
) -> purgatory_simulation::EntityId {
    owner.apply_input(InputUpdate::DevSpawnMonster {
        connection_id: connection,
        monster_content_id: monster,
    });
    *owner.dev_spawned_monsters.last().expect("spawned monster")
}

fn kill_with(
    owner: &mut GameplayOwner,
    killer: purgatory_simulation::EntityId,
    creature: purgatory_simulation::EntityId,
) {
    let mut health = owner.world().health_of(creature).expect("health");
    health.current = 2.0;
    assert!(owner.world_mut().set_health(creature, health));
    assert!(owner.world_mut().apply_player_damage(killer, creature, 1.0));
    assert!(owner.world().health_of(creature).unwrap().current > 0.0);
    assert!(owner.world_mut().apply_player_damage(killer, creature, 5.0));
    assert!(owner.world().npc_of(creature).unwrap().dead_pending);
    owner.simulate_tick(purgatory_simulation::TICK_DURATION.as_secs_f32());
}

#[test]
fn authored_death_reserves_one_id_and_pickup_keeps_it() {
    let mut owner = GameplayOwner::new();
    let first = ConnectionId::from_raw(1);
    let second = ConnectionId::from_raw(2);
    let _attacker = session(&mut owner, 1, 11);
    let killer = session(&mut owner, 2, 22);
    owner.set_monster_drops_for_test(
        MONSTER_DEV_GUARANTEED_DROP,
        vec![purgatory_content::MonsterDropEntry {
            item: ITEM_DEV_SAMPLE_SCRAP,
            chance_bps: purgatory_content::DROP_CHANCE_BPS_MAX,
            quantity_min: 2,
            quantity_max: 2,
        }],
    );
    let creature = spawn_monster(&mut owner, first, MONSTER_DEV_GUARANTEED_DROP);
    let id = reserved(880_001);
    owner.install_reserved_ids_for_test(vec![id]);

    let mut health = owner.world().health_of(creature).unwrap();
    health.current = 2.0;
    assert!(owner.world_mut().set_health(creature, health));
    let attacker = session_entity(&owner, first);
    assert!(owner.world_mut().apply_player_damage(attacker, creature, 1.0));
    assert!(owner.world().npc_of(creature).unwrap().target.is_some());
    assert!(owner.world_mut().apply_player_damage(killer, creature, 5.0));
    owner.simulate_tick(purgatory_simulation::TICK_DURATION.as_secs_f32());

    let record = owner.world().item_record(id).expect("reserved drop");
    assert_eq!(record.definition, ITEM_DEV_SAMPLE_SCRAP);
    assert_eq!(record.quantity, 2);
    assert_eq!(owner.monster_loot_killer(id), Some(CharacterId::from_raw(22)));
    let drop = owner.world().world_drop_entity_for_item(id).expect("visible");
    let x = owner.world().transform_of(drop).unwrap().position[0];
    assert!(owner.set_player_x(second, x));
    let (picked, _) = owner.apply_pickup(second, wire(drop)).expect("pickup");
    assert_eq!(picked, id);
    let owned = owner.world().inventory_snapshot(killer);
    assert_eq!(owned.len(), 1);
    assert_eq!(owned[0].1, id);
    assert_eq!(owned[0].2.quantity, 2);
    assert!(owner.world().world_drop_entity_for_item(id).is_none());
}

fn session_entity(owner: &GameplayOwner, connection: ConnectionId) -> purgatory_simulation::EntityId {
    owner.entity_of(connection).expect("player")
}

#[test]
fn disconnect_before_manifest_keeps_the_killing_character() {
    let mut owner = GameplayOwner::new();
    let killer_connection = ConnectionId::from_raw(2);
    let other = ConnectionId::from_raw(3);
    session(&mut owner, 2, 22);
    session(&mut owner, 3, 33);
    let killer = owner.entity_of(killer_connection).unwrap();
    let creature = spawn_monster(&mut owner, killer_connection, MONSTER_DEV_GUARANTEED_DROP);
    kill_with(&mut owner, killer, creature);
    assert_eq!(owner.pending_loot_len(), 1);
    owner.detach(killer_connection);
    let id = reserved(880_002);
    owner.install_reserved_ids_for_test(vec![id]);
    owner.simulate_tick(purgatory_simulation::TICK_DURATION.as_secs_f32());
    assert_eq!(owner.pending_loot_len(), 0);
    assert_eq!(owner.monster_loot_killer(id), Some(CharacterId::from_raw(22)));
    let drop = owner.world().world_drop_entity_for_item(id).unwrap();
    let rejected = owner.apply_pickup(other, wire(drop)).unwrap_err();
    assert_eq!(rejected, PickupRejectReason::StateBlocked);
}

#[test]
fn partial_manifest_keeps_only_the_remainder() {
    let mut owner = GameplayOwner::new();
    let connection = ConnectionId::from_raw(1);
    let killer = session(&mut owner, 1, 11);
    let creature = spawn_monster(&mut owner, connection, MONSTER_DEV_MIXED_DROPS);
    let first = reserved(880_011);
    owner.install_reserved_ids_for_test(vec![first]);
    kill_with(&mut owner, killer, creature);
    assert_eq!(owner.pending_loot_len(), 1);
    let first_record = owner.world().item_record(first).expect("first drop");
    assert_eq!(first_record.definition, ITEM_SMALL_POTION);
    assert_eq!(first_record.quantity, 1);
    let second = reserved(880_012);
    owner.install_reserved_ids_for_test(vec![second]);
    owner.simulate_tick(purgatory_simulation::TICK_DURATION.as_secs_f32());
    assert_eq!(owner.pending_loot_len(), 0);
    let second_record = owner.world().item_record(second).expect("remainder");
    assert_eq!(second_record.definition, ITEM_DEV_SAMPLE_SCRAP);
    assert!((1..=3).contains(&second_record.quantity));
    assert!(owner.world().item_record(reserved(880_013)).is_none());
    assert_ne!(first_record.definition, ITEM_IRON_SCRAP);
    assert_ne!(second_record.definition, ITEM_IRON_SCRAP);
}

#[test]
fn closed_address_abandons_unmanifested_drops() {
    let mut owner = GameplayOwner::new();
    let connection = ConnectionId::from_raw(1);
    let killer = session(&mut owner, 1, 11);
    let creature = spawn_monster(&mut owner, connection, MONSTER_DEV_GUARANTEED_DROP);
    let address = owner.world().address_of(creature).unwrap();
    kill_with(&mut owner, killer, creature);
    assert_eq!(owner.pending_loot_len(), 1);
    owner.close_world_address(address);
    assert_eq!(owner.pending_loot_len(), 0);
    assert!(owner.loot_abandoned_count() >= 1);
    owner.install_reserved_ids_for_test(vec![reserved(880_021)]);
    owner.simulate_tick(purgatory_simulation::TICK_DURATION.as_secs_f32());
    assert!(owner.world().item_record(reserved(880_021)).is_none());
}

#[test]
fn empty_table_and_despawn_do_not_drop() {
    let mut owner = GameplayOwner::new();
    let connection = ConnectionId::from_raw(1);
    let killer = session(&mut owner, 1, 11);
    owner.install_reserved_ids_for_test(vec![reserved(880_031)]);
    let crab = spawn_monster(&mut owner, connection, MONSTER_MOSS_CRAB);
    kill_with(&mut owner, killer, crab);
    assert!(owner.world().item_record(reserved(880_031)).is_none());
    assert_eq!(owner.pending_loot_len(), 0);

    let alive = spawn_monster(&mut owner, connection, MONSTER_DEV_GUARANTEED_DROP);
    assert!(owner.world_mut().despawn(alive));
    owner.simulate_tick(purgatory_simulation::TICK_DURATION.as_secs_f32());
    assert!(owner.world().item_record(reserved(880_031)).is_none());
}

fn roll_with_seed(seed: u64, entries: &[purgatory_content::MonsterDropEntry], deaths: usize) -> Vec<purgatory_content::RolledMonsterDrop> {
    let mut owner = GameplayOwner::new();
    owner.seed_loot_rng_for_test(seed);
    let mut rolled = Vec::new();
    for _ in 0..deaths {
        rolled.extend(purgatory_content::roll_monster_drops(entries, || {
            owner.next_loot_unit()
        }));
    }
    rolled
}

#[test]
fn production_rng_quantity_one_to_two_hits_both_endpoints() {
    let entries = [purgatory_content::MonsterDropEntry {
        item: ITEM_DEV_SAMPLE_SCRAP,
        chance_bps: 10_000,
        quantity_min: 1,
        quantity_max: 2,
    }];
    let deaths = 10_000;
    let rolled = roll_with_seed(0xC0FF_EE01_1234_5678, &entries, deaths);
    assert_eq!(rolled.len(), deaths);
    let ones = rolled.iter().filter(|drop| drop.quantity == 1).count();
    let twos = rolled.iter().filter(|drop| drop.quantity == 2).count();
    assert!(ones > 4_000 && twos > 4_000, "ones={ones} twos={twos}");
    let mean = rolled.iter().map(|drop| u64::from(drop.quantity)).sum::<u64>() as f64
        / deaths as f64;
    assert!((mean - 1.5).abs() < 0.05, "mean={mean}");
}

#[test]
fn production_rng_covers_bounds_and_independent_rows() {
    let fixed = [purgatory_content::MonsterDropEntry {
        item: ITEM_SMALL_POTION,
        chance_bps: 10_000,
        quantity_min: 2,
        quantity_max: 2,
    }];
    let fixed_rolls = roll_with_seed(7, &fixed, 100);
    assert!(fixed_rolls.iter().all(|drop| drop.quantity == 2));

    let wide = [purgatory_content::MonsterDropEntry {
        item: ITEM_DEV_SAMPLE_SCRAP,
        chance_bps: 10_000,
        quantity_min: 1,
        quantity_max: 6,
    }];
    let wide_rolls = roll_with_seed(11, &wide, 2_000);
    for quantity in 1..=6 {
        assert!(
            wide_rolls.iter().any(|drop| drop.quantity == quantity),
            "missing {quantity}"
        );
    }

    let rows = [
        purgatory_content::MonsterDropEntry {
            item: ITEM_SMALL_POTION,
            chance_bps: 0,
            quantity_min: 1,
            quantity_max: 2,
        },
        purgatory_content::MonsterDropEntry {
            item: ITEM_DEV_SAMPLE_SCRAP,
            chance_bps: 10_000,
            quantity_min: 1,
            quantity_max: 3,
        },
    ];
    let mixed = roll_with_seed(19, &rows, 1_000);
    assert!(mixed.iter().all(|drop| drop.item == ITEM_DEV_SAMPLE_SCRAP));
    assert!(mixed.iter().any(|drop| drop.quantity == 1));
    assert!(mixed.iter().any(|drop| drop.quantity == 3));

    let rare = [purgatory_content::MonsterDropEntry {
        item: ITEM_IRON_SCRAP,
        chance_bps: 1_000,
        quantity_min: 1,
        quantity_max: 1,
    }];
    let rare_rolls = roll_with_seed(23, &rare, 10_000);
    assert!(
        (700..1_300).contains(&rare_rolls.len()),
        "successes={}",
        rare_rolls.len()
    );
}
