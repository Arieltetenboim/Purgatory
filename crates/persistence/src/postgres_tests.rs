//! PostgreSQL integration tests.
//!
//! They are `#[ignore]`d so the default quality gate does not treat a missing
//! database as a pass. Run them with `PURGATORY_TEST_DATABASE_URL` pointed at a
//! dedicated database (not `Purgatory_dev`):
//!
//! `cargo test -p purgatory-persistence --lib postgres_tests -- --ignored --nocapture`

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use purgatory_common::{CharacterId, ContentId, DevLogin, ItemInstanceId, RestoreIntent};

use crate::postgres::{self, PostgresSettings};
use crate::{
    ChannelClaim, CharacterItemLocation, DurableCommand, DurableContentRules, DurableEquipmentSlot,
    ItemContentRule, ItemOwner, LearnedAbilityWrite, LeaseAuthority, LeaseBarrier, LiveDestination,
    MoveItem, NarrativeWrite, PersistError, PersistenceService, PersistentCharacterSnapshot,
    PlaceNewItem, ReservedItemOutcome, ReservedItemUse, SessionAdmission,
};

static DB_LOCK: Mutex<()> = Mutex::new(());
static SCHEMA_SEQ: AtomicU64 = AtomicU64::new(0);

fn test_settings() -> PostgresSettings {
    let url = std::env::var("PURGATORY_TEST_DATABASE_URL").unwrap_or_default();
    if url.trim().is_empty() {
        panic!("PURGATORY_TEST_DATABASE_URL is unset, so this PostgreSQL test was not executed");
    }
    let schema = format!(
        "p12a_{}_{}",
        std::process::id(),
        SCHEMA_SEQ.fetch_add(1, Ordering::Relaxed)
    );
    PostgresSettings::for_tests(url, schema).expect("dedicated test database and disposable schema")
}

fn unique_dir() -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "purgatory-pg-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn with_db(test: impl FnOnce(&Path, &PostgresSettings)) {
    let settings = test_settings();
    let _guard = DB_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let dir = unique_dir();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| test(&dir, &settings)));
    let dropped = postgres::drop_test_schema(&settings);
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(err) = dropped {
        eprintln!("PURGATORY postgres test schema cleanup failed: {err}");
    }
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

fn rules() -> DurableContentRules {
    let mut rules = DurableContentRules::new();
    rules
        .insert_item(ItemContentRule {
            content_id: ContentId::from_raw(30_011),
            stack_limit: 20,
            equip_slot: None,
            retired: false,
        })
        .unwrap();
    rules
        .insert_item(ItemContentRule {
            content_id: ContentId::from_raw(30_001),
            stack_limit: 1,
            equip_slot: Some(DurableEquipmentSlot::Weapon),
            retired: false,
        })
        .unwrap();
    rules
        .insert_ability(ContentId::from_raw(40_001), false)
        .unwrap();
    rules
}

fn login(name: &str) -> DevLogin {
    DevLogin::parse(name).unwrap()
}

fn place(owner: CharacterId, slot: u16) -> PlaceNewItem {
    PlaceNewItem {
        owner,
        definition_content_id: ContentId::from_raw(30_011),
        quantity: 1,
        location: CharacterItemLocation::Inventory { slot },
    }
}

fn allow_players(service: &mut PersistenceService) {
    for name in ["alice", "bob", "zed"] {
        service.provision_dev_user(&login(name)).unwrap();
    }
}

fn open(_dir: &Path, settings: &PostgresSettings) -> PersistenceService {
    if let Err(err) = PersistenceService::bootstrap_postgresql(settings) {
        assert!(err.to_string().contains("already bootstrapped"), "{err}");
    }
    let mut service = PersistenceService::open_postgresql(settings).unwrap();
    service.set_durable_content_rules(rules());
    allow_players(&mut service);
    service
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn revision_conflict_rolls_the_loser_back() {
    with_db(|dir, settings| {
        let mut first = open(dir, settings);
        let alice = login("alice");
        let entry = first.create_character(&alice, "Alice").unwrap();
        let mut second = PersistenceService::open_postgresql(settings).unwrap();
        second.set_durable_content_rules(rules());
        let winner = DurableCommand {
            key: "winner".into(),
            expected_revisions: vec![(entry.character_id, 1)],
            place_new: vec![place(entry.character_id, 0)],
            moves: Vec::new(),
            retire: Vec::new(),
            narrative: Vec::new(),
            learned: Vec::new(),

            reserved_uses: Vec::new(),
        };
        let mut loser = winner.clone();
        loser.key = "loser".into();
        let committed = first.commit_durable(&winner).unwrap();
        let err = second.commit_durable(&loser).unwrap_err();
        assert!(matches!(err, PersistError::Conflict { .. }), "{err}");
        assert_eq!(committed.minted_item_ids.len(), 1);
        assert!(second.item(committed.minted_item_ids[0]).unwrap().is_some());
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            1
        );
        assert_eq!(
            postgres::count_table(settings, "durable_commands").unwrap(),
            1
        );
        assert_eq!(
            first
                .load_owned_character(&alice, entry.character_id)
                .unwrap()
                .unwrap()
                .persistence_revision,
            2
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn same_slot_conflict_rolls_back_the_whole_command() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let err = service
            .commit_durable(&DurableCommand {
                key: "double-slot".into(),
                expected_revisions: vec![(entry.character_id, 1)],
                place_new: vec![place(entry.character_id, 0), place(entry.character_id, 0)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: vec![NarrativeWrite::SetFact {
                    character_id: entry.character_id,
                    fact_key: "fact.road".into(),
                    value: true,
                }],
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap_err();
        assert!(matches!(err, PersistError::Conflict { .. }), "{err}");
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            0
        );
        assert_eq!(
            postgres::count_table(settings, "character_facts").unwrap(),
            0
        );
        assert_eq!(
            postgres::count_table(settings, "durable_commands").unwrap(),
            0
        );
        assert_eq!(
            service
                .load_owned_character(&alice, entry.character_id)
                .unwrap()
                .unwrap()
                .persistence_revision,
            1
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn retired_ids_survive_reconnect_and_are_not_reused() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let minted = service
            .commit_durable(&DurableCommand {
                key: "mint".into(),
                expected_revisions: vec![(entry.character_id, 1)],
                place_new: vec![place(entry.character_id, 0)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap();
        let item_id = minted.minted_item_ids[0];
        service.claim_channel(0, None).unwrap();
        service
            .commit_durable(&DurableCommand {
                key: "drop".into(),
                expected_revisions: vec![(entry.character_id, 2)],
                place_new: Vec::new(),
                moves: vec![MoveItem {
                    item_instance_id: item_id,
                    to: LiveDestination::Ground,
                }],
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap();
        service
            .commit_durable(&DurableCommand {
                key: "expire".into(),
                expected_revisions: Vec::new(),
                place_new: Vec::new(),
                moves: Vec::new(),
                retire: vec![item_id],
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap();
        drop(service);
        let mut service = open(dir, settings);
        if let crate::ChannelClaim::Busy { generation, .. } =
            service.claim_channel(0, None).unwrap()
        {
            service.remember_channel_for_test(0, generation);
        }
        let retired = service.item(item_id).unwrap().unwrap();
        assert_eq!(retired.owner, ItemOwner::Retired);
        let again = service
            .commit_durable(&DurableCommand {
                key: "mint-again".into(),
                expected_revisions: vec![(entry.character_id, 3)],
                place_new: vec![place(entry.character_id, 0)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap();
        assert_ne!(again.minted_item_ids[0], item_id);
        assert_eq!(
            service.item(item_id).unwrap().unwrap().owner,
            ItemOwner::Retired
        );
        let err = service
            .commit_durable(&DurableCommand {
                key: "reuse".into(),
                expected_revisions: Vec::new(),
                place_new: Vec::new(),
                moves: vec![MoveItem {
                    item_instance_id: item_id,
                    to: LiveDestination::Ground,
                }],
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap_err();
        assert!(matches!(err, PersistError::Conflict { .. }), "{err}");
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn lost_reply_retry_returns_the_committed_result() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let command = DurableCommand {
            key: "grant-once".into(),
            expected_revisions: vec![(entry.character_id, 1)],
            place_new: vec![place(entry.character_id, 1)],
            moves: Vec::new(),
            retire: Vec::new(),
            narrative: vec![NarrativeWrite::MarkDialogueHeard {
                character_id: entry.character_id,
                npc_content_id: ContentId::from_raw(20_002),
                beat_id: "intro".into(),
            }],
            learned: vec![LearnedAbilityWrite {
                character_id: entry.character_id,
                ability_content_id: ContentId::from_raw(40_001),
            }],

            reserved_uses: Vec::new(),
        };
        let first = service.commit_durable(&command).unwrap();
        let second = service.commit_durable(&command).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            1
        );
        assert_eq!(
            postgres::count_table(settings, "character_learned_abilities").unwrap(),
            1
        );
        let mut changed = command.clone();
        changed.place_new[0].location = CharacterItemLocation::Inventory { slot: 2 };
        let err = service.commit_durable(&changed).unwrap_err();
        assert!(matches!(err, PersistError::Integrity { .. }), "{err}");
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            1
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn ownership_is_isolated_and_restore_does_not_erase_items() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let bob = login("bob");
        let alice_entry = service.create_character(&alice, "Alice").unwrap();
        let bob_entry = service.create_character(&bob, "Bobby").unwrap();
        let committed = service
            .commit_durable(&DurableCommand {
                key: "alice-kit".into(),
                expected_revisions: vec![(alice_entry.character_id, 1)],
                place_new: vec![place(alice_entry.character_id, 0)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: vec![NarrativeWrite::SetFact {
                    character_id: alice_entry.character_id,
                    fact_key: "fact.road".into(),
                    value: true,
                }],
                learned: vec![LearnedAbilityWrite {
                    character_id: alice_entry.character_id,
                    ability_content_id: ContentId::from_raw(40_001),
                }],

                reserved_uses: Vec::new(),
            })
            .unwrap();
        assert!(
            service
                .load_owned_character(&bob, alice_entry.character_id)
                .unwrap()
                .is_none()
        );
        let err = service
            .commit_durable(&DurableCommand {
                key: "bob-takes".into(),
                expected_revisions: vec![(bob_entry.character_id, 1)],
                place_new: Vec::new(),
                moves: vec![MoveItem {
                    item_instance_id: committed.minted_item_ids[0],
                    to: LiveDestination::Character {
                        character_id: bob_entry.character_id,
                        location: CharacterItemLocation::Inventory { slot: 0 },
                    },
                }],
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap_err();
        assert!(matches!(err, PersistError::Conflict { .. }), "{err}");
        let item = service.item(committed.minted_item_ids[0]).unwrap().unwrap();
        assert_eq!(
            item.owner,
            ItemOwner::Character {
                character_id: alice_entry.character_id,
                location: CharacterItemLocation::Inventory { slot: 0 },
            }
        );
        service
            .save_snapshot(crate::PersistentCharacterSnapshot {
                character_id: alice_entry.character_id,
                persistence_revision: 3,
                restore: RestoreIntent {
                    map_authored: "map.map2".into(),
                    point_id: "gate".into(),
                    checkpoint_id: None,
                },
                instance_exit: None,
                current_health_milli: None,
                health_revision: 0,
            })
            .unwrap();
        let restored = service
            .load_owned_character(&alice, alice_entry.character_id)
            .unwrap()
            .unwrap();
        assert_eq!(restored.restore.map_authored, "map.map2");
        assert_eq!(restored.persistence_revision, 3);
        assert_eq!(
            service
                .item(committed.minted_item_ids[0])
                .unwrap()
                .unwrap()
                .owner,
            item.owner
        );
        let narrative = service.narrative(alice_entry.character_id).unwrap();
        assert_eq!(narrative.facts.get("fact.road"), Some(&true));
        assert!(narrative.learned_abilities.contains(&40_001));
        assert!(
            service
                .narrative(bob_entry.character_id)
                .unwrap()
                .facts
                .is_empty()
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn representative_command_workload_is_measured_not_a_capacity_claim() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let cycles = 30u64;
        let started = Instant::now();
        let mut revision = 1u64;
        let mut seen = BTreeSet::new();
        service.claim_channel(0, None).unwrap();
        for index in 0..cycles {
            let minted = service
                .commit_durable(&DurableCommand {
                    key: format!("work-mint-{index}"),
                    expected_revisions: vec![(entry.character_id, revision)],
                    place_new: vec![place(entry.character_id, 0)],
                    moves: Vec::new(),
                    retire: Vec::new(),
                    narrative: Vec::new(),
                    learned: Vec::new(),

                    reserved_uses: Vec::new(),
                })
                .unwrap();
            revision += 1;
            let item_id = minted.minted_item_ids[0];
            assert!(seen.insert(item_id.raw()));
            service
                .commit_durable(&DurableCommand {
                    key: format!("work-ground-{index}"),
                    expected_revisions: vec![(entry.character_id, revision)],
                    place_new: Vec::new(),
                    moves: vec![MoveItem {
                        item_instance_id: item_id,
                        to: LiveDestination::Ground,
                    }],
                    retire: Vec::new(),
                    narrative: Vec::new(),
                    learned: Vec::new(),

                    reserved_uses: Vec::new(),
                })
                .unwrap();
            revision += 1;
            service
                .commit_durable(&DurableCommand {
                    key: format!("work-retire-{index}"),
                    expected_revisions: Vec::new(),
                    place_new: Vec::new(),
                    moves: Vec::new(),
                    retire: vec![item_id],
                    narrative: Vec::new(),
                    learned: Vec::new(),

                    reserved_uses: Vec::new(),
                })
                .unwrap();
            assert_eq!(
                service.item(item_id).unwrap().unwrap().owner,
                ItemOwner::Retired
            );
        }
        let elapsed = started.elapsed();
        let commands = cycles * 3;
        eprintln!(
            "PURGATORY 12A workload: {commands} commits, {cycles} place/ground/retire cycles, total_ms={}, mean_us={}, debug_or_current_profile, not a production capacity claim",
            elapsed.as_millis(),
            elapsed.as_micros() / u128::from(commands)
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn moving_a_non_equippable_item_into_weapon_is_rejected() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let placed = service
            .commit_durable(&DurableCommand {
                key: "stack".into(),
                expected_revisions: vec![(entry.character_id, 1)],
                place_new: vec![place(entry.character_id, 0)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap();
        let item_id = placed.minted_item_ids[0];
        let err = service
            .commit_durable(&DurableCommand {
                key: "equip-stack".into(),
                expected_revisions: vec![(entry.character_id, 2)],
                place_new: Vec::new(),
                moves: vec![MoveItem {
                    item_instance_id: item_id,
                    to: LiveDestination::Character {
                        character_id: entry.character_id,
                        location: CharacterItemLocation::Equipped {
                            slot: DurableEquipmentSlot::Weapon,
                        },
                    },
                }],
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap_err();
        assert!(matches!(err, PersistError::ContentRejected { .. }), "{err}");
        assert_eq!(
            service.item(item_id).unwrap().unwrap().owner,
            ItemOwner::Character {
                character_id: entry.character_id,
                location: CharacterItemLocation::Inventory { slot: 0 },
            }
        );
        assert_eq!(
            service
                .load_owned_character(&alice, entry.character_id)
                .unwrap()
                .unwrap()
                .persistence_revision,
            2
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn gameplay_save_at_the_command_revision_keeps_its_restore() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        service
            .commit_durable(&DurableCommand {
                key: "advance".into(),
                expected_revisions: vec![(entry.character_id, 1)],
                place_new: vec![place(entry.character_id, 0)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap();
        // Gameplay loads revision 1, then detach and request_save both emit
        // loaded+1. A durable command consumes that same next revision.
        service
            .save_snapshot(crate::PersistentCharacterSnapshot {
                character_id: entry.character_id,
                persistence_revision: 2,
                restore: RestoreIntent {
                    map_authored: "map.map2".into(),
                    point_id: "gate".into(),
                    checkpoint_id: None,
                },
                instance_exit: None,
                current_health_milli: None,
                health_revision: 0,
            })
            .unwrap();
        let loaded = service
            .load_owned_character(&alice, entry.character_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.persistence_revision, 2);
        assert_eq!(loaded.restore.point_id, "gate");
        service
            .save_snapshot(crate::PersistentCharacterSnapshot {
                character_id: entry.character_id,
                persistence_revision: 1,
                restore: RestoreIntent {
                    map_authored: "map.map1".into(),
                    point_id: "stale".into(),
                    checkpoint_id: None,
                },
                instance_exit: None,
                current_health_milli: None,
                health_revision: 0,
            })
            .unwrap();
        let loaded = service
            .load_owned_character(&alice, entry.character_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.restore.point_id, "gate");
        assert_eq!(loaded.persistence_revision, 2);
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn lost_commit_reply_and_later_rule_change_return_the_stored_result() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let command = DurableCommand {
            key: "grant-once".into(),
            expected_revisions: vec![(entry.character_id, 1)],
            place_new: vec![place(entry.character_id, 1)],
            moves: Vec::new(),
            retire: Vec::new(),
            narrative: Vec::new(),
            learned: vec![LearnedAbilityWrite {
                character_id: entry.character_id,
                ability_content_id: ContentId::from_raw(40_001),
            }],

            reserved_uses: Vec::new(),
        };
        service.hide_next_commit_reply_for_test();
        let hidden = service.commit_durable(&command).unwrap();
        assert_eq!(hidden.minted_item_ids.len(), 1);
        assert!(service.item(hidden.minted_item_ids[0]).unwrap().is_some());
        let mut retired = rules();
        retired
            .insert_item(ItemContentRule {
                content_id: ContentId::from_raw(30_011),
                stack_limit: 20,
                equip_slot: None,
                retired: true,
            })
            .unwrap();
        retired
            .insert_ability(ContentId::from_raw(40_001), true)
            .unwrap();
        service.set_durable_content_rules(retired);
        let retry = service.commit_durable(&command).unwrap();
        assert_eq!(hidden, retry);
        let err = service
            .commit_durable(&DurableCommand {
                key: "after-rule-change".into(),
                expected_revisions: vec![(entry.character_id, retry.revisions[0].1)],
                place_new: vec![place(entry.character_id, 2)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap_err();
        assert!(matches!(err, PersistError::ContentRejected { .. }), "{err}");
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn retire_and_reward_can_share_a_slot_and_two_items_can_swap() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let first = service
            .commit_durable(&DurableCommand {
                key: "two".into(),
                expected_revisions: vec![(entry.character_id, 1)],
                place_new: vec![place(entry.character_id, 0), place(entry.character_id, 1)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap();
        let left = first.minted_item_ids[0];
        let right = first.minted_item_ids[1];
        service
            .commit_durable(&DurableCommand {
                key: "swap".into(),
                expected_revisions: vec![(entry.character_id, 2)],
                place_new: Vec::new(),
                moves: vec![
                    MoveItem {
                        item_instance_id: left,
                        to: LiveDestination::Character {
                            character_id: entry.character_id,
                            location: CharacterItemLocation::Inventory { slot: 1 },
                        },
                    },
                    MoveItem {
                        item_instance_id: right,
                        to: LiveDestination::Character {
                            character_id: entry.character_id,
                            location: CharacterItemLocation::Inventory { slot: 0 },
                        },
                    },
                ],
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap();
        assert_eq!(
            service.item(left).unwrap().unwrap().owner,
            ItemOwner::Character {
                character_id: entry.character_id,
                location: CharacterItemLocation::Inventory { slot: 1 },
            }
        );
        assert_eq!(
            service.item(right).unwrap().unwrap().owner,
            ItemOwner::Character {
                character_id: entry.character_id,
                location: CharacterItemLocation::Inventory { slot: 0 },
            }
        );
        let replaced = service
            .commit_durable(&DurableCommand {
                key: "replace-slot".into(),
                expected_revisions: vec![(entry.character_id, 3)],
                place_new: vec![place(entry.character_id, 0)],
                moves: Vec::new(),
                retire: vec![right],
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap();
        assert_eq!(
            service.item(right).unwrap().unwrap().owner,
            ItemOwner::Retired
        );
        assert_eq!(
            service
                .item(replaced.minted_item_ids[0])
                .unwrap()
                .unwrap()
                .owner,
            ItemOwner::Character {
                character_id: entry.character_id,
                location: CharacterItemLocation::Inventory { slot: 0 },
            }
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn distinct_runtime_role_can_commit_and_cannot_create_tables() {
    with_db(|_dir, settings| {
        let roles = postgres::provision_ephemeral_roles(settings).expect("distinct test roles");
        PersistenceService::bootstrap_postgresql(&roles.runtime).expect("migration-role bootstrap");
        let migration = PostgresSettings {
            url: roles.runtime.migration_url.clone().expect("migration url"),
            migration_url: None,
            schema: roles.runtime.schema.clone(),
            deployment_id: roles.runtime.deployment_id.clone(),
        };
        {
            let mut admin = PersistenceService::open_postgresql(&migration).expect("migrator");
            assert!(admin.provision_dev_user(&login("alice")).unwrap());
        }
        let mut service = PersistenceService::open_postgresql(&roles.runtime).unwrap();
        service.set_durable_content_rules(rules());
        let alice = login("alice");
        let denied = service.provision_dev_user(&login("bob")).unwrap_err();
        assert!(
            denied
                .to_string()
                .to_ascii_lowercase()
                .contains("permission denied"),
            "{denied}"
        );
        let entry = service.create_character(&alice, "Alice").unwrap();
        let committed = service
            .commit_durable(&DurableCommand {
                key: "runtime".into(),
                expected_revisions: vec![(entry.character_id, 1)],
                place_new: vec![place(entry.character_id, 0)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: vec![NarrativeWrite::SetFact {
                    character_id: entry.character_id,
                    fact_key: "fact.road".into(),
                    value: true,
                }],
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap();
        assert_eq!(committed.minted_item_ids.len(), 1);
        assert!(postgres::runtime_cannot_create_table(&roles.runtime).unwrap());
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn fresh_database_admits_only_provisioned_users() {
    with_db(|_dir, settings| {
        PersistenceService::bootstrap_postgresql(settings).unwrap();
        let mut service = PersistenceService::open_postgresql(settings).unwrap();
        let probe = login("dev.probe");
        assert!(!service.user_registered(&probe).unwrap());
        let err = service.provision_dev_user(&probe).unwrap_err();
        assert!(err.to_string().contains("not a player account"), "{err}");
        let created = service.create_character(&probe, "Probe").unwrap_err();
        assert!(
            matches!(
                created,
                PersistError::CreateRejected(crate::error::CreateCharacterRejection::Unregistered)
            ),
            "{created}"
        );
        assert_eq!(postgres::count_table(settings, "dev_users").unwrap(), 0);
        let player = login("dev.player");
        assert!(!service.user_registered(&player).unwrap());
        let rejected = service.create_character(&player, "Player").unwrap_err();
        assert!(
            matches!(
                rejected,
                PersistError::CreateRejected(crate::error::CreateCharacterRejection::Unregistered)
            ),
            "{rejected}"
        );
        assert!(service.provision_dev_user(&player).unwrap());
        assert!(service.create_character(&player, "Player").is_ok());
        assert_eq!(postgres::count_table(settings, "dev_users").unwrap(), 1);
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn later_equal_revision_restore_does_not_replace_the_recorded_one() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        service
            .commit_durable(&DurableCommand {
                key: "advance".into(),
                expected_revisions: vec![(entry.character_id, 1)],
                place_new: vec![place(entry.character_id, 0)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap();
        service
            .save_snapshot(crate::PersistentCharacterSnapshot {
                character_id: entry.character_id,
                persistence_revision: 2,
                restore: RestoreIntent {
                    map_authored: "map.map2".into(),
                    point_id: "gate".into(),
                    checkpoint_id: None,
                },
                instance_exit: None,
                current_health_milli: None,
                health_revision: 0,
            })
            .unwrap();
        service
            .save_snapshot(crate::PersistentCharacterSnapshot {
                character_id: entry.character_id,
                persistence_revision: 2,
                restore: RestoreIntent {
                    map_authored: "map.map1".into(),
                    point_id: "default".into(),
                    checkpoint_id: None,
                },
                instance_exit: None,
                current_health_milli: None,
                health_revision: 0,
            })
            .unwrap();
        let loaded = service
            .load_owned_character(&alice, entry.character_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.restore.point_id, "gate");
        assert_eq!(loaded.persistence_revision, 2);
        service
            .save_snapshot(crate::PersistentCharacterSnapshot {
                character_id: entry.character_id,
                persistence_revision: 3,
                restore: RestoreIntent {
                    map_authored: "map.map3".into(),
                    point_id: "town".into(),
                    checkpoint_id: None,
                },
                instance_exit: None,
                current_health_milli: None,
                health_revision: 0,
            })
            .unwrap();
        let loaded = service
            .load_owned_character(&alice, entry.character_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.restore.point_id, "town");
        assert_eq!(loaded.persistence_revision, 3);
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn unusable_connection_at_the_commit_reply_stays_unknown_until_retry() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let command = DurableCommand {
            key: "once".into(),
            expected_revisions: vec![(entry.character_id, 1)],
            place_new: vec![place(entry.character_id, 0)],
            moves: Vec::new(),
            retire: Vec::new(),
            narrative: Vec::new(),
            learned: Vec::new(),

            reserved_uses: Vec::new(),
        };
        // Scope: COMMIT has already succeeded. The hook then closes that
        // connection before the reply is read. This does not fail during COMMIT.
        // The worker returns this Result on its oneshot and does not decide
        // whether the command was applied.
        service.discard_connection_after_next_commit_for_test();
        let err = service.commit_durable(&command).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("commit outcome unknown"), "{text}");
        assert!(!text.contains("not committed"), "{text}");
        let result = service.commit_durable(&command).unwrap();
        assert_eq!(result.revisions, vec![(entry.character_id, 2)]);
        assert_eq!(result.minted_item_ids.len(), 1);
        let loaded = service
            .load_owned_character(&alice, entry.character_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.persistence_revision, 2);
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            1
        );
        assert_eq!(
            postgres::count_table(settings, "durable_commands").unwrap(),
            1
        );
        let repeated = service.commit_durable(&command).unwrap();
        assert_eq!(repeated, result);
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            1
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn same_worker_reconnects_after_a_lost_commit_reply() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let command = DurableCommand {
            key: "same-worker".into(),
            expected_revisions: vec![(entry.character_id, 1)],
            place_new: vec![place(entry.character_id, 0)],
            moves: Vec::new(),
            retire: Vec::new(),
            narrative: Vec::new(),
            learned: Vec::new(),

            reserved_uses: Vec::new(),
        };
        service.discard_connection_after_next_commit_for_test();
        let lost = service.commit_durable(&command).unwrap_err();
        let lost_text = lost.to_string();
        assert!(lost_text.contains("commit outcome unknown"), "{lost_text}");
        assert!(!lost_text.contains("not committed"), "{lost_text}");
        service.fail_next_reconnects_for_test(1);
        let blocked = service.commit_durable(&command).unwrap_err();
        let blocked_text = blocked.to_string();
        assert!(
            blocked_text.contains("commit outcome unknown"),
            "a committed command was rejected as a definite failure: {blocked_text}"
        );
        assert!(
            !matches!(blocked, PersistError::Conflict { .. }),
            "{blocked_text}"
        );
        let result = service
            .commit_durable(&command)
            .expect("the same worker must reconnect and return the committed result");
        assert_eq!(result.revisions, vec![(entry.character_id, 2)]);
        assert_eq!(result.minted_item_ids.len(), 1);
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            1
        );
        assert_eq!(
            postgres::count_table(settings, "durable_commands").unwrap(),
            1
        );
        let repeated = service.commit_durable(&command).unwrap();
        assert_eq!(repeated, result);
        let bogus = LeaseAuthority {
            login: login("alice"),
            character_id: entry.character_id,
            generation: 99,
        };
        let leased = service
            .commit_durable_leased(&command, Some(&bogus))
            .expect("a stored command is not re-rejected by a later lease");
        assert_eq!(leased, result);
        let mut changed = command.clone();
        changed.place_new[0].location = CharacterItemLocation::Inventory { slot: 4 };
        let mismatch = service.commit_durable(&changed).unwrap_err();
        assert!(
            mismatch
                .to_string()
                .contains("reused for a different command"),
            "{mismatch}"
        );
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            1
        );
    });
}

fn granted(admission: SessionAdmission) -> (LeaseAuthority, crate::OwnedRestore) {
    match admission {
        SessionAdmission::Granted {
            authority: Some(authority),
            restore,
        } => (authority, *restore),
        other => panic!("expected a granted lease, got {other:?}"),
    }
}

fn mint(id: CharacterId, revision: u64, key: &str) -> DurableCommand {
    DurableCommand {
        key: key.into(),
        expected_revisions: vec![(id, revision)],
        place_new: vec![place(id, 0)],
        moves: Vec::new(),
        retire: Vec::new(),
        narrative: Vec::new(),
        learned: Vec::new(),
        reserved_uses: Vec::new(),
    }
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn one_user_has_one_live_character_across_connections() {
    with_db(|dir, settings| {
        let mut holder = open(dir, settings);
        let mut other = open(dir, settings);
        let alice = login("alice");
        let bob = login("bob");
        let alpha = holder.create_character(&alice, "Alpha").unwrap();
        let beta = holder.create_character(&alice, "Beta").unwrap();
        let outsider = holder.create_character(&bob, "Other").unwrap();
        let started = Instant::now();
        let (alpha_lease, _) = granted(holder.admit(&alice, alpha.character_id).unwrap());
        eprintln!("12B_LIFECYCLE admit_us={}", started.elapsed().as_micros());
        assert!(matches!(
            other.admit(&alice, beta.character_id).unwrap(),
            SessionAdmission::Held
        ));
        assert!(matches!(
            other.admit(&alice, alpha.character_id).unwrap(),
            SessionAdmission::Held
        ));
        let (bob_lease, _) = granted(other.admit(&bob, outsider.character_id).unwrap());
        assert_ne!(alpha_lease.login, bob_lease.login);
        holder.release_lease(&alpha_lease).unwrap();
        let (beta_lease, _) = granted(other.admit(&alice, beta.character_id).unwrap());
        assert_eq!(beta_lease.character_id, beta.character_id);
        assert!(beta_lease.generation > alpha_lease.generation);
        other.expire_lease_for_test(&alice).unwrap();
        let (after_expiry, _) = granted(holder.admit(&alice, alpha.character_id).unwrap());
        assert_eq!(after_expiry.character_id, alpha.character_id);
        assert!(after_expiry.generation > beta_lease.generation);
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn stale_generation_cannot_renew_release_commit_or_save() {
    with_db(|dir, settings| {
        let mut holder = open(dir, settings);
        let mut rival = open(dir, settings);
        let alice = login("alice");
        let entry = holder.create_character(&alice, "Alpha").unwrap();
        let (lease, restore) = granted(holder.admit(&alice, entry.character_id).unwrap());
        let (next, _) = holder.supersede(&lease).unwrap();
        assert!(matches!(
            rival.renew_lease(&lease),
            Err(PersistError::LeaseLost)
        ));
        assert!(matches!(
            rival.release_lease(&lease),
            Err(PersistError::LeaseLost)
        ));
        let err = holder
            .commit_durable_leased(&mint(entry.character_id, 1, "stale"), Some(&lease))
            .unwrap_err();
        assert!(matches!(err, PersistError::LeaseLost), "{err}");
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            0
        );
        let mut snapshot = PersistentCharacterSnapshot::from_character(&restore.character);
        snapshot.persistence_revision = restore.character.persistence_revision + 1;
        snapshot.restore.point_id = "moved".into();
        assert!(matches!(
            holder.save_snapshot_leased(snapshot, Some(&lease)),
            Err(PersistError::LeaseLost)
        ));
        let committed = holder
            .commit_durable_leased(&mint(entry.character_id, 1, "fresh"), Some(&next))
            .unwrap();
        assert_eq!(committed.revisions, vec![(entry.character_id, 2)]);
        let retry = rival
            .commit_durable_leased(&mint(entry.character_id, 1, "fresh"), Some(&lease))
            .unwrap();
        assert_eq!(retry, committed);
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            1
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn superseded_generation_rejects_a_command_that_was_waiting_on_the_lock() {
    with_db(|dir, settings| {
        let mut holder = open(dir, settings);
        let alice = login("alice");
        let entry = holder.create_character(&alice, "Alpha").unwrap();
        let (lease, _) = granted(holder.admit(&alice, entry.character_id).unwrap());
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        holder.set_lease_barrier(LeaseBarrier {
            entered: entered_tx,
            release: release_rx,
        });
        let old = lease.clone();
        let character_id = entry.character_id;
        let current = lease;
        let locking =
            std::thread::spawn(move || holder.supersede(&current).map(|(next, _)| (holder, next)));
        entered_rx.recv().expect("supersede reached the lease lock");
        let waiter_dir = dir.to_path_buf();
        let waiter_settings = settings.clone();
        let waiting = std::thread::spawn(move || {
            let mut service = open(&waiter_dir, &waiter_settings);
            service.commit_durable_leased(&mint(character_id, 1, "queued"), Some(&old))
        });
        let mut observer = open(dir, settings);
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while observer.sessions_waiting_on_a_lock().unwrap() == 0 {
            assert!(
                Instant::now() < deadline,
                "queued command did not wait on the lease"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        release_tx.send(()).unwrap();
        let (mut holder, next) = locking.join().unwrap().unwrap();
        let err = waiting.join().unwrap().unwrap_err();
        assert!(matches!(err, PersistError::LeaseLost), "{err}");
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            0
        );
        holder
            .commit_durable_leased(&mint(character_id, 1, "after"), Some(&next))
            .unwrap();
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            1
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn admit_restores_owned_items_facts_and_grants_without_runtime_ids() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let missing = CharacterId::from_raw(99);
        assert!(matches!(
            service.admit(&alice, missing).unwrap(),
            SessionAdmission::NotOwned
        ));
        let entry = service.create_character(&alice, "Alpha").unwrap();
        let id = entry.character_id;
        service
            .commit_durable(&DurableCommand {
                key: "bundle".into(),
                expected_revisions: vec![(id, 1)],
                place_new: vec![place(id, 0)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: vec![
                    NarrativeWrite::SetFact {
                        character_id: id,
                        fact_key: "met.inn".into(),
                        value: true,
                    },
                    NarrativeWrite::MarkNpcMet {
                        character_id: id,
                        npc_authored: "npc.inn".into(),
                    },
                    NarrativeWrite::MarkDialogueHeard {
                        character_id: id,
                        npc_content_id: ContentId::from_raw(20_001),
                        beat_id: "intro".into(),
                    },
                ],
                learned: vec![LearnedAbilityWrite {
                    character_id: id,
                    ability_content_id: ContentId::from_raw(40_001),
                }],

                reserved_uses: Vec::new(),
            })
            .unwrap();
        let (lease, restore) = granted(service.admit(&alice, id).unwrap());
        assert_eq!(restore.character.character_id, id);
        assert_eq!(restore.character.persistence_revision, 2);
        assert_eq!(restore.items.len(), 1);
        assert!(restore.items[0].item_instance_id.raw() != 0);
        assert!(
            restore
                .narrative
                .facts
                .get("met.inn")
                .copied()
                .unwrap_or(false)
        );
        assert!(restore.narrative.npcs_met.contains("npc.inn"));
        assert!(
            restore
                .narrative
                .dialogue_heard
                .contains(&(20_001, "intro".into()))
        );
        assert!(restore.narrative.learned_abilities.contains(&40_001));
        let mut snapshot = PersistentCharacterSnapshot::from_character(&restore.character);
        snapshot.restore.point_id = "after-command".into();
        service
            .save_snapshot_leased(snapshot.clone(), Some(&lease))
            .unwrap();
        stale_snapshot(&mut service, &lease, &restore, 2, "older");
        let loaded = service.load_owned_character(&alice, id).unwrap().unwrap();
        assert_eq!(loaded.restore.point_id, "after-command");
        assert!(matches!(
            service.admit(&login("bob"), id).unwrap(),
            SessionAdmission::NotOwned
        ));
        service.release_lease(&lease).unwrap();
        let (_, again) = granted(service.admit(&alice, id).unwrap());
        assert_eq!(again.character.restore.point_id, "after-command");
        assert_eq!(again.items.len(), 1);
        let _ = snapshot;
    });
}

fn stale_snapshot(
    service: &mut PersistenceService,
    lease: &LeaseAuthority,
    restore: &crate::OwnedRestore,
    revision: u64,
    point: &str,
) {
    let mut stale = PersistentCharacterSnapshot::from_character(&restore.character);
    stale.persistence_revision = revision;
    stale.restore.point_id = point.into();
    service.save_snapshot_leased(stale, Some(lease)).unwrap();
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn clock_timestamp_moves_while_now_stays_at_transaction_start() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let (now_fixed, clock_moved) = service.clock_moved_inside_one_statement().unwrap();
        assert!(now_fixed, "now() must stay at transaction start");
        assert!(
            clock_moved,
            "clock_timestamp() must move during the statement"
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn renewal_extends_a_live_lease_and_a_stale_one_cannot() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alpha").unwrap();
        let (lease, _) = granted(service.admit(&alice, entry.character_id).unwrap());
        service.renew_lease(&lease).unwrap();
        assert!(matches!(
            open(dir, settings)
                .admit(&alice, entry.character_id)
                .unwrap(),
            SessionAdmission::Held
        ));
        service.expire_lease_for_test(&alice).unwrap();
        assert!(matches!(
            service.renew_lease(&lease),
            Err(PersistError::LeaseLost)
        ));
    });
}

fn drop_item(
    service: &mut PersistenceService,
    id: CharacterId,
    item: ItemInstanceId,
    revision: u64,
) {
    service
        .commit_durable(&DurableCommand {
            key: format!("ground-{revision}"),
            expected_revisions: vec![(id, revision)],
            place_new: Vec::new(),
            moves: vec![MoveItem {
                item_instance_id: item,
                to: LiveDestination::Ground,
            }],
            retire: Vec::new(),
            narrative: Vec::new(),
            learned: Vec::new(),

            reserved_uses: Vec::new(),
        })
        .unwrap();
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn ground_retirement_is_scoped_idempotent_and_does_not_reuse_ids() {
    with_db(|dir, settings| {
        let mut channel_a = open(dir, settings);
        let mut channel_b = open(dir, settings);
        let alice = login("alice");
        let entry = channel_a.create_character(&alice, "Alpha").unwrap();
        let id = entry.character_id;
        let ChannelClaim::Claimed {
            generation: gen_a, ..
        } = channel_a.claim_channel(1, None).unwrap()
        else {
            panic!("channel 1 should be claimed");
        };
        let ChannelClaim::Claimed {
            generation: gen_b, ..
        } = channel_b.claim_channel(2, None).unwrap()
        else {
            panic!("channel 2 should be claimed");
        };
        let first = channel_a
            .commit_durable(&mint(id, 1, "a1"))
            .unwrap()
            .minted_item_ids[0];
        drop_item(&mut channel_a, id, first, 2);
        let second = channel_a
            .commit_durable(&mint(id, 3, "a2"))
            .unwrap()
            .minted_item_ids[0];
        drop_item(&mut channel_a, id, second, 4);
        let kept = channel_b
            .commit_durable(&mint(id, 5, "b1"))
            .unwrap()
            .minted_item_ids[0];
        drop_item(&mut channel_b, id, kept, 6);
        assert_eq!(
            channel_a.item(first).unwrap().unwrap().owner,
            ItemOwner::Ground
        );
        channel_a.expire_channel_for_test(1).unwrap();
        let ChannelClaim::Claimed {
            generation: gen_next,
            retired_ground,
            ..
        } = channel_a.claim_channel(1, Some(1)).unwrap()
        else {
            panic!("expired channel should be claimable");
        };
        assert!(gen_next > gen_a);
        assert_eq!(retired_ground, 1);
        let rest = channel_a.sweep_channel(1, gen_next, None).unwrap();
        assert_eq!(rest, 1);
        assert_eq!(channel_a.sweep_channel(1, gen_next, None).unwrap(), 0);
        assert_eq!(
            channel_a.item(first).unwrap().unwrap().owner,
            ItemOwner::Retired
        );
        assert_eq!(
            channel_a.item(second).unwrap().unwrap().owner,
            ItemOwner::Retired
        );
        assert_eq!(
            channel_b.item(kept).unwrap().unwrap().owner,
            ItemOwner::Ground
        );
        assert!(matches!(
            channel_a.sweep_channel(1, gen_a, None),
            Err(PersistError::Conflict { .. })
        ));
        channel_a.remember_channel_for_test(1, gen_a);
        let stale = channel_a.commit_durable(&DurableCommand {
            key: "stale-write".into(),
            expected_revisions: vec![(id, 7)],
            place_new: Vec::new(),
            moves: vec![MoveItem {
                item_instance_id: kept,
                to: LiveDestination::Ground,
            }],
            retire: Vec::new(),
            narrative: Vec::new(),
            learned: Vec::new(),

            reserved_uses: Vec::new(),
        });
        assert!(
            matches!(stale, Err(PersistError::Conflict { .. })),
            "{stale:?}"
        );
        assert_eq!(
            channel_b.item(kept).unwrap().unwrap().owner,
            ItemOwner::Ground
        );
        let _ = gen_b;
        let fresh = channel_b
            .commit_durable(&mint(id, 7, "fresh-id"))
            .unwrap()
            .minted_item_ids[0];
        assert_ne!(fresh, first);
        assert_ne!(fresh, second);
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn clean_channel_release_keeps_generation_monotonic_and_retires_stamped_ground() {
    with_db(|dir, settings| {
        let mut holder = open(dir, settings);
        let alice = login("alice");
        let id = holder
            .create_character(&alice, "Alpha")
            .unwrap()
            .character_id;
        let ChannelClaim::Claimed {
            generation: first, ..
        } = holder.claim_channel(0, None).unwrap()
        else {
            panic!("channel 0 should be claimed");
        };
        let item = holder
            .commit_durable(&mint(id, 1, "stamped"))
            .unwrap()
            .minted_item_ids[0];
        drop_item(&mut holder, id, item, 2);
        holder.release_channel(0, first).unwrap();
        assert!(matches!(
            holder.renew_channel(0, first),
            Err(PersistError::LeaseLost)
        ));
        assert!(matches!(
            holder.release_channel(0, first),
            Err(PersistError::LeaseLost)
        ));
        drop(holder);

        let mut restarted = open(dir, settings);
        let ChannelClaim::Claimed {
            generation: second,
            retired_ground,
            ..
        } = restarted.claim_channel(0, None).unwrap()
        else {
            panic!("restart after clean release should claim");
        };
        assert!(
            second > first,
            "generation reused after clean release: {first} then {second}"
        );
        assert!(retired_ground >= 1);
        assert_eq!(
            restarted.item(item).unwrap().unwrap().owner,
            ItemOwner::Retired
        );
        let mut stale = open(dir, settings);
        assert!(matches!(
            stale.renew_channel(0, first),
            Err(PersistError::LeaseLost)
        ));
        assert!(matches!(
            stale.release_channel(0, first),
            Err(PersistError::LeaseLost)
        ));

        drop(restarted);
        let mut crashed = open(dir, settings);
        assert!(matches!(
            crashed.claim_channel(0, None).unwrap(),
            ChannelClaim::Busy { generation, .. } if generation == second
        ));
        crashed.expire_channel_for_test(0).unwrap();
        let ChannelClaim::Claimed {
            generation: third, ..
        } = crashed.claim_channel(0, None).unwrap()
        else {
            panic!("expired channel should be claimable");
        };
        assert!(third > second);
        assert!(matches!(
            crashed.renew_channel(0, second),
            Err(PersistError::LeaseLost)
        ));
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn unscoped_ground_is_retired_before_admission_while_another_channel_is_live() {
    with_db(|dir, settings| {
        let mut live_a = open(dir, settings);
        let mut live_b = open(dir, settings);
        let mut starter = open(dir, settings);
        let alice = login("alice");
        let id = live_a
            .create_character(&alice, "Alpha")
            .unwrap()
            .character_id;
        let ChannelClaim::Claimed {
            generation: gen_a, ..
        } = live_a.claim_channel(1, None).unwrap()
        else {
            panic!("channel 1 should be claimed");
        };
        let ChannelClaim::Claimed {
            generation: gen_b, ..
        } = live_b.claim_channel(2, None).unwrap()
        else {
            panic!("channel 2 should be claimed");
        };
        let kept_b = live_b
            .commit_durable(&mint(id, 1, "kept-b"))
            .unwrap()
            .minted_item_ids[0];
        drop_item(&mut live_b, id, kept_b, 2);
        let kept_a = live_a
            .commit_durable(&mint(id, 3, "kept-a"))
            .unwrap()
            .minted_item_ids[0];
        drop_item(&mut live_a, id, kept_a, 4);
        let old = live_a
            .commit_durable(&mint(id, 5, "unscoped"))
            .unwrap()
            .minted_item_ids[0];
        drop_item(&mut live_a, id, old, 6);
        live_a.unstamp_ground_for_test(old).unwrap();
        assert_eq!(live_a.item(old).unwrap().unwrap().owner, ItemOwner::Ground);

        let ChannelClaim::Claimed { retired_ground, .. } = starter.claim_channel(0, None).unwrap()
        else {
            panic!("startup claim should proceed while other channels are live");
        };
        assert!(
            retired_ground >= 1,
            "unscoped ground was not retired before admission"
        );
        assert_eq!(
            starter.item(old).unwrap().unwrap().owner,
            ItemOwner::Retired
        );
        assert_eq!(
            live_a.item(kept_a).unwrap().unwrap().owner,
            ItemOwner::Ground
        );
        assert_eq!(
            live_b.item(kept_b).unwrap().unwrap().owner,
            ItemOwner::Ground
        );
        assert!(matches!(
            starter.admit(&alice, id).unwrap(),
            SessionAdmission::Granted { .. }
        ));
        assert_eq!(
            starter.item(old).unwrap().unwrap().owner,
            ItemOwner::Retired
        );
        assert_eq!(
            live_b.item(kept_b).unwrap().unwrap().owner,
            ItemOwner::Ground
        );
        live_a.renew_channel(1, gen_a).unwrap();
        live_b.renew_channel(2, gen_b).unwrap();
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn process_exit_leaves_a_committed_lease_until_expiry() {
    if std::env::var("P12B_CRASH_CHILD").ok().as_deref() == Some("1") {
        crash_child_admits_and_aborts();
        return;
    }
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alpha").unwrap();
        drop(service);
        let ready = dir.join("crash-ready");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "postgres_tests::process_exit_leaves_a_committed_lease_until_expiry",
                "--exact",
                "--ignored",
                "--test-threads=1",
            ])
            .env("P12B_CRASH_CHILD", "1")
            .env(
                "PURGATORY_TEST_DATABASE_URL",
                std::env::var("PURGATORY_TEST_DATABASE_URL").unwrap(),
            )
            .env("P12B_SCHEMA", &settings.schema)
            .env("P12B_DIR", dir)
            .env("P12B_CHARACTER", entry.character_id.raw().to_string())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        while !ready.exists() {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("child exited before admit: {status}");
            }
            assert!(Instant::now() < deadline, "child did not admit");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let status = child.wait().unwrap();
        assert!(
            !status.success(),
            "child must abort after the commit; status {status}. This proves a committed lease survives process death. It does not prove durability across hardware power loss."
        );
        let mut survivor = open(dir, settings);
        assert!(matches!(
            survivor.admit(&alice, entry.character_id).unwrap(),
            SessionAdmission::Held
        ));
        survivor.expire_lease_for_test(&alice).unwrap();
        let (lease, _) = granted(survivor.admit(&alice, entry.character_id).unwrap());
        assert!(lease.generation > 1);
    });
}

fn crash_child_admits_and_aborts() {
    let settings = PostgresSettings::for_tests(
        std::env::var("PURGATORY_TEST_DATABASE_URL").unwrap(),
        std::env::var("P12B_SCHEMA").unwrap(),
    )
    .unwrap();
    let dir = PathBuf::from(std::env::var("P12B_DIR").unwrap());
    let character =
        CharacterId::from_raw(std::env::var("P12B_CHARACTER").unwrap().parse().unwrap());
    let mut service = open(&dir, &settings);
    let _ = granted(
        service
            .admit(&login("alice"), character)
            .expect("child admit"),
    );
    std::fs::write(dir.join("crash-ready"), b"ready").unwrap();
    std::process::abort();
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn randomized_lease_steps_keep_one_authority() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let alpha = service.create_character(&alice, "Alpha").unwrap();
        let beta = service.create_character(&alice, "Beta").unwrap();
        let mut state = 0x12B_u64;
        let mut authority: Option<LeaseAuthority> = None;
        for _ in 0..24 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            match state % 5 {
                0 | 1 => {
                    let id = if state.is_multiple_of(2) {
                        alpha.character_id
                    } else {
                        beta.character_id
                    };
                    match service.admit(&alice, id).unwrap() {
                        SessionAdmission::Granted {
                            authority: Some(next),
                            ..
                        } => {
                            if let Some(previous) = &authority {
                                assert!(next.generation > previous.generation);
                            }
                            authority = Some(next);
                        }
                        SessionAdmission::Held => {
                            assert!(authority.is_some());
                        }
                        SessionAdmission::NotOwned => panic!("owned character was refused"),
                        SessionAdmission::Granted {
                            authority: None, ..
                        } => {
                            panic!("postgres admit omitted the lease")
                        }
                    }
                }
                2 => {
                    if let Some(current) = authority.clone()
                        && service.renew_lease(&current).is_err()
                    {
                        authority = None;
                    }
                }
                3 => {
                    if let Some(current) = authority.take() {
                        service.release_lease(&current).unwrap();
                    }
                }
                _ => {
                    if authority.is_some() {
                        service.expire_lease_for_test(&alice).unwrap();
                    }
                }
            }
        }
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn expired_channel_cannot_retire_live_ground() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alpha").unwrap();
        let id = entry.character_id;
        let ChannelClaim::Claimed { .. } = service.claim_channel(1, None).unwrap() else {
            panic!("channel 1 should be claimed");
        };
        let item = service
            .commit_durable(&mint(id, 1, "ground"))
            .unwrap()
            .minted_item_ids[0];
        drop_item(&mut service, id, item, 2);
        service.expire_channel_for_test(1).unwrap();
        let err = service
            .commit_durable(&DurableCommand {
                key: format!("retire-{}", item.raw()),
                expected_revisions: Vec::new(),
                place_new: Vec::new(),
                moves: Vec::new(),
                retire: vec![item],
                narrative: Vec::new(),
                learned: Vec::new(),

                reserved_uses: Vec::new(),
            })
            .unwrap_err();
        assert!(matches!(err, PersistError::Conflict { .. }));
        assert_eq!(
            service.item(item).unwrap().unwrap().owner,
            ItemOwner::Ground
        );
    });
}

fn reserved_inventory(
    item: ItemInstanceId,
    owner: CharacterId,
    revision: u64,
    key: &str,
    slot: u16,
) -> DurableCommand {
    DurableCommand {
        key: key.into(),
        expected_revisions: vec![(owner, revision)],
        place_new: Vec::new(),
        moves: Vec::new(),
        retire: Vec::new(),
        narrative: Vec::new(),
        learned: Vec::new(),
        reserved_uses: vec![ReservedItemUse {
            item_instance_id: item,
            definition_content_id: ContentId::from_raw(30_011),
            quantity: 1,
            outcome: ReservedItemOutcome::Inventory { owner, slot },
        }],
    }
}

fn reserved_retired(item: ItemInstanceId) -> DurableCommand {
    DurableCommand {
        key: format!("retire-{}", item.raw()),
        expected_revisions: Vec::new(),
        place_new: Vec::new(),
        moves: Vec::new(),
        retire: Vec::new(),
        narrative: Vec::new(),
        learned: Vec::new(),
        reserved_uses: vec![ReservedItemUse {
            item_instance_id: item,
            definition_content_id: ContentId::from_raw(30_011),
            quantity: 1,
            outcome: ReservedItemOutcome::Retired,
        }],
    }
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn reserved_ids_stay_disjoint_across_connections_and_restarts() {
    with_db(|dir, settings| {
        let mut first = open(dir, settings);
        let mut second = open(dir, settings);
        let ChannelClaim::Claimed { .. } = first.claim_channel(1, None).unwrap() else {
            panic!("channel 1 should be claimed");
        };
        let ChannelClaim::Claimed { .. } = second.claim_channel(2, None).unwrap() else {
            panic!("channel 2 should be claimed");
        };
        let left = first.reserve_item_ids(4).unwrap();
        let right = second.reserve_item_ids(4).unwrap();
        assert_eq!(left.len(), 4);
        assert_eq!(right.len(), 4);
        assert!(left.iter().all(|id| !right.contains(id)));
        assert!(
            left.windows(2)
                .all(|pair| pair[0].raw() + 1 == pair[1].raw())
        );
        let alice = login("alice");
        let entry = first.create_character(&alice, "Alice").unwrap();
        let minted = first
            .commit_durable(&mint(entry.character_id, 1, "after-reserve"))
            .unwrap()
            .minted_item_ids[0];
        assert!(!left.contains(&minted) && !right.contains(&minted));
        drop(first);
        drop(second);
        let mut restarted = open(dir, settings);
        let ChannelClaim::Claimed { .. } = restarted.claim_channel(3, None).unwrap() else {
            panic!("channel 3 should be claimed while 1 and 2 stay live");
        };
        let again = restarted.reserve_item_ids(4).unwrap();
        assert!(
            again
                .iter()
                .all(|id| !left.contains(id) && !right.contains(id))
        );
        assert!(!again.contains(&minted));
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn reserved_pickup_keeps_its_id_against_retry_expiry_and_reuse() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let ChannelClaim::Claimed { .. } = service.claim_channel(1, None).unwrap() else {
            panic!("channel 1 should be claimed");
        };
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let ids = service.reserve_item_ids(3).unwrap();
        let pickup = reserved_inventory(ids[0], entry.character_id, 1, "pickup-visible", 0);
        let first = service.commit_durable(&pickup).unwrap();
        assert_eq!(first.minted_item_ids, vec![ids[0]]);
        let retry = service.commit_durable(&pickup).unwrap();
        assert_eq!(retry, first);
        assert_eq!(
            service.item(ids[0]).unwrap().unwrap().owner,
            ItemOwner::Character {
                character_id: entry.character_id,
                location: CharacterItemLocation::Inventory { slot: 0 },
            }
        );
        let competing = reserved_inventory(ids[0], entry.character_id, 2, "pickup-competitor", 1);
        let err = service.commit_durable(&competing).unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
        let err = service
            .commit_durable(&DurableCommand {
                key: format!("retire-{}", ids[0].raw()),
                expected_revisions: Vec::new(),
                place_new: Vec::new(),
                moves: Vec::new(),
                retire: vec![ids[0]],
                narrative: Vec::new(),
                learned: Vec::new(),
                reserved_uses: Vec::new(),
            })
            .unwrap_err();
        assert!(matches!(err, PersistError::Conflict { .. }), "{err}");
        assert!(matches!(
            service.item(ids[0]).unwrap().unwrap().owner,
            ItemOwner::Character { .. }
        ));

        let retired = service.commit_durable(&reserved_retired(ids[1])).unwrap();
        assert_eq!(retired.minted_item_ids, vec![ids[1]]);
        assert_eq!(
            service.item(ids[1]).unwrap().unwrap().owner,
            ItemOwner::Retired
        );
        let late = reserved_inventory(ids[1], entry.character_id, 2, "pickup-retired", 1);
        let err = service.commit_durable(&late).unwrap_err();
        assert!(err.to_string().contains("retired"), "{err}");

        let unreserved = ItemInstanceId::from_raw(9_000_000);
        let err = service
            .commit_durable(&reserved_inventory(
                unreserved,
                entry.character_id,
                2,
                "pickup-unreserved",
                1,
            ))
            .unwrap_err();
        assert!(err.to_string().contains("not reserved"), "{err}");
        assert!(service.item(unreserved).unwrap().is_none());
        assert!(service.item(ids[2]).unwrap().is_none());

        drop(service);
        let mut restarted = open(dir, settings);
        let ChannelClaim::Claimed { .. } = restarted.claim_channel(2, None).unwrap() else {
            panic!("channel 2 should be claimed while channel 1 stays live");
        };
        let next = restarted.reserve_item_ids(3).unwrap();
        assert!(next.iter().all(|id| !ids.contains(id)));
        let err = restarted
            .commit_durable(&reserved_inventory(
                ids[2],
                entry.character_id,
                2,
                "spend-wasted-after-restart",
                1,
            ))
            .unwrap_err();
        assert!(err.to_string().contains("not reserved"), "{err}");
        assert!(restarted.item(ids[2]).unwrap().is_none());
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn another_channels_unused_id_cannot_be_inserted() {
    with_db(|dir, settings| {
        let mut holder = open(dir, settings);
        let mut other = open(dir, settings);
        let ChannelClaim::Claimed { .. } = holder.claim_channel(1, None).unwrap() else {
            panic!("channel 1 should be claimed");
        };
        let ChannelClaim::Claimed { .. } = other.claim_channel(2, None).unwrap() else {
            panic!("channel 2 should be claimed");
        };
        let alice = login("alice");
        let entry = holder.create_character(&alice, "Alice").unwrap();
        let issued = holder.reserve_item_ids(4).unwrap();
        let stolen = issued[1];
        let err = other.commit_durable(&reserved_inventory(
            stolen,
            entry.character_id,
            1,
            "steal-other-range",
            0,
        ));
        assert!(
            err.is_err(),
            "channel 2 inserted unused id {} from channel 1's range",
            stolen.raw()
        );
        let err = err.unwrap_err();
        assert!(err.to_string().contains("not reserved"), "{err}");
        assert!(other.item(stolen).unwrap().is_none());
        holder
            .commit_durable(&reserved_inventory(
                issued[0],
                entry.character_id,
                1,
                "spend-own-range",
                0,
            ))
            .unwrap();
        assert!(holder.item(issued[0]).unwrap().is_some());
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn unissued_gap_below_the_counter_cannot_be_inserted() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let ChannelClaim::Claimed { .. } = service.claim_channel(1, None).unwrap() else {
            panic!("channel 1 should be claimed");
        };
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        service.leave_unissued_item_gap_for_test(40).unwrap();
        let gap = ItemInstanceId::from_raw(20);
        let err = service.commit_durable(&reserved_inventory(
            gap,
            entry.character_id,
            1,
            "insert-unissued-gap",
            0,
        ));
        assert!(
            err.is_err(),
            "inserted unissued id {} while the counter was already past it",
            gap.raw()
        );
        let err = err.unwrap_err();
        assert!(err.to_string().contains("not reserved"), "{err}");
        assert!(service.item(gap).unwrap().is_none());
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn previous_channel_generation_cannot_spend_its_unused_ids() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let ChannelClaim::Claimed { generation, .. } = service.claim_channel(1, None).unwrap()
        else {
            panic!("channel 1 should be claimed");
        };
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let issued = service.reserve_item_ids(4).unwrap();
        service.expire_channel_for_test(1).unwrap();
        let ChannelClaim::Claimed {
            generation: renewed,
            ..
        } = service.claim_channel(1, None).unwrap()
        else {
            panic!("expired channel 1 should be claimable");
        };
        assert!(renewed > generation);
        let err = service.commit_durable(&reserved_inventory(
            issued[0],
            entry.character_id,
            1,
            "spend-previous-generation",
            0,
        ));
        assert!(
            err.is_err(),
            "generation {renewed} inserted id {} issued to generation {generation}",
            issued[0].raw()
        );
        let err = err.unwrap_err();
        assert!(err.to_string().contains("not reserved"), "{err}");
        assert!(service.item(issued[0]).unwrap().is_none());
        let fresh = service.reserve_item_ids(2).unwrap();
        assert!(fresh.iter().all(|id| !issued.contains(id)));
        service
            .commit_durable(&reserved_inventory(
                fresh[0],
                entry.character_id,
                1,
                "spend-current-generation",
                0,
            ))
            .unwrap();
        assert!(service.item(fresh[0]).unwrap().is_some());
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn fresh_bootstrap_with_legacy_files_does_not_import_or_write() {
    with_db(|dir, settings| {
        let identity =
            br#"{"schema_version":2,"next_character_id":8,"logins":{"zed":[{"character_id":7,"display_name":"Zed"}]}}"#;
        let character = dir.join("char_0000000000000007.json");
        let body = br#"{"schema_version":1,"character_id":7,"persistence_revision":4,"restore":{"map_authored":"map.map2","point_id":"gate"}}"#;
        std::fs::write(dir.join("identity.json"), identity).unwrap();
        std::fs::write(&character, body).unwrap();
        let mut names_before = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names_before.sort();
        PersistenceService::bootstrap_postgresql(settings).unwrap();
        let mut service = PersistenceService::open_postgresql(settings).unwrap();
        service.set_durable_content_rules(rules());
        assert_eq!(std::fs::read(dir.join("identity.json")).unwrap(), identity);
        assert_eq!(std::fs::read(&character).unwrap(), body);
        let mut names_after = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        names_after.sort();
        assert_eq!(names_before, names_after);
        assert!(
            !dir.join("durable_writer.json").exists(),
            "fresh bootstrap wrote durable_writer.json"
        );
        assert!(
            !service.user_registered(&login("zed")).unwrap(),
            "fresh bootstrap imported a legacy character"
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn restart_preserves_ownership_without_a_local_marker() {
    with_db(|dir, settings| {
        PersistenceService::bootstrap_postgresql(settings).unwrap();
        let mut service = PersistenceService::open_postgresql(settings).unwrap();
        service.set_durable_content_rules(rules());
        allow_players(&mut service);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let committed = service
            .commit_durable(&DurableCommand {
                key: "owned".into(),
                expected_revisions: vec![(entry.character_id, 1)],
                place_new: vec![place(entry.character_id, 0)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),
                reserved_uses: Vec::new(),
            })
            .unwrap();
        let item = committed.minted_item_ids[0];
        drop(service);
        assert!(!dir.join("durable_writer.json").exists());
        let mut restarted = PersistenceService::open_postgresql(settings).unwrap();
        restarted.set_durable_content_rules(rules());
        assert!(!dir.join("durable_writer.json").exists());
        assert_eq!(
            restarted.roster(&alice).unwrap()[0].character_id,
            entry.character_id
        );
        assert_eq!(
            restarted.item(item).unwrap().unwrap().owner,
            ItemOwner::Character {
                character_id: entry.character_id,
                location: CharacterItemLocation::Inventory { slot: 0 },
            }
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn duplicate_bootstrap_cannot_erase_progress() {
    with_db(|_dir, settings| {
        PersistenceService::bootstrap_postgresql(settings).unwrap();
        let mut service = PersistenceService::open_postgresql(settings).unwrap();
        service.set_durable_content_rules(rules());
        allow_players(&mut service);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let committed = service
            .commit_durable(&DurableCommand {
                key: "keep".into(),
                expected_revisions: vec![(entry.character_id, 1)],
                place_new: vec![place(entry.character_id, 0)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),
                reserved_uses: Vec::new(),
            })
            .unwrap();
        let item = committed.minted_item_ids[0];
        drop(service);
        let err = PersistenceService::bootstrap_postgresql(settings).unwrap_err();
        assert!(err.to_string().contains("already bootstrapped"), "{err}");
        let mut again = PersistenceService::open_postgresql(settings).unwrap();
        again.set_durable_content_rules(rules());
        assert_eq!(
            again.roster(&alice).unwrap()[0].character_id,
            entry.character_id
        );
        assert_eq!(
            again.item(item).unwrap().unwrap().owner,
            ItemOwner::Character {
                character_id: entry.character_id,
                location: CharacterItemLocation::Inventory { slot: 0 },
            }
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn wrong_database_identity_fails_closed() {
    with_db(|_dir, settings| {
        PersistenceService::bootstrap_postgresql(settings).unwrap();
        let mut service = PersistenceService::open_postgresql(settings).unwrap();
        service.set_durable_content_rules(rules());
        allow_players(&mut service);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        drop(service);
        let wrong_id = settings
            .clone()
            .with_deployment_id("other-world".into())
            .unwrap();
        let err = match PersistenceService::open_postgresql(&wrong_id) {
            Ok(_) => panic!("wrong deployment identity opened"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("does not match"), "{err}");
        let mut wrong_schema = settings.clone();
        wrong_schema.schema = format!("{}_missing", settings.schema);
        let err = match PersistenceService::open_postgresql(&wrong_schema) {
            Ok(_) => panic!("missing schema opened"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("not bootstrapped"), "{err}");
        let mut again = PersistenceService::open_postgresql(settings).unwrap();
        assert_eq!(
            again.roster(&alice).unwrap()[0].character_id,
            entry.character_id
        );
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn restored_database_starts_on_another_host_without_a_local_marker() {
    with_db(|_dir, settings| {
        PersistenceService::bootstrap_postgresql(settings).unwrap();
        let mut service = PersistenceService::open_postgresql(settings).unwrap();
        service.set_durable_content_rules(rules());
        allow_players(&mut service);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        let committed = service
            .commit_durable(&DurableCommand {
                key: "moved".into(),
                expected_revisions: vec![(entry.character_id, 1)],
                place_new: vec![place(entry.character_id, 0)],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),
                reserved_uses: Vec::new(),
            })
            .unwrap();
        let item = committed.minted_item_ids[0];
        drop(service);
        let other_host = unique_dir();
        assert!(!other_host.join("durable_writer.json").exists());
        let mut restored = PersistenceService::open_postgresql(settings).unwrap();
        restored.set_durable_content_rules(rules());
        assert!(!other_host.join("durable_writer.json").exists());
        assert_eq!(
            restored.roster(&alice).unwrap()[0].character_id,
            entry.character_id
        );
        assert_eq!(
            restored.item(item).unwrap().unwrap().owner,
            ItemOwner::Character {
                character_id: entry.character_id,
                location: CharacterItemLocation::Inventory { slot: 0 },
            }
        );
        let _ = std::fs::remove_dir_all(other_host);
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn current_health_migration_keeps_existing_rows() {
    with_db(|_dir, settings| {
        let name = postgres::database_name(&settings.url).unwrap();
        assert!(
            !name.eq_ignore_ascii_case("purgatory_dev"),
            "refusing Purgatory_dev"
        );
        let mut client = postgres::install_pre_health_schema(settings).unwrap();
        postgres::bootstrap_empty(&mut client, &settings.deployment_id).unwrap();
        let character_id = 1u64.to_be_bytes();
        let item_id = 1u64.to_be_bytes();
        client
            .execute("INSERT INTO dev_users (login) VALUES ($1)", &[&"alice"])
            .unwrap();
        client
            .execute(
                "INSERT INTO characters (
                    character_id, owner_login, display_name, name_key, roster_position,
                    persistence_revision, restore_revision, restore_map_authored, restore_point_id,
                    restore_checkpoint_id, instance_exit_reason
                 ) VALUES ($1, $2, $3, $4, $5, $6, $6, $7, $8, NULL, NULL)",
                &[
                    &character_id.as_slice() as &(dyn ::postgres::types::ToSql + Sync),
                    &"alice",
                    &"Mira",
                    &"mira",
                    &0i32,
                    &1i64,
                    &"map.map2",
                    &"default",
                ],
            )
            .unwrap();
        client
            .execute(
                "INSERT INTO item_instances (
                    item_instance_id, definition_content_id, quantity, state,
                    owner_character_id, location_kind, inventory_slot, equipment_slot
                 ) VALUES ($1, $2, $3, 'live', $4, 'equipped', NULL, 'weapon')",
                &[
                    &item_id.as_slice() as &(dyn ::postgres::types::ToSql + Sync),
                    &30_001i32,
                    &1i32,
                    &character_id.as_slice(),
                ],
            )
            .unwrap();
        let users_before = postgres::count_table(settings, "dev_users").unwrap();
        let characters_before = postgres::count_table(settings, "characters").unwrap();
        let items_before = postgres::count_table(settings, "item_instances").unwrap();
        assert_eq!((users_before, characters_before, items_before), (1, 1, 1));
        let startup = postgres::require_exact_migrations(&mut client).unwrap_err();
        assert!(
            startup
                .to_string()
                .contains("unsupported postgresql migration history"),
            "{startup}"
        );

        postgres::migrate(&mut client).unwrap();
        postgres::require_exact_migrations(&mut client).unwrap();
        assert_eq!(postgres::count_table(settings, "dev_users").unwrap(), 1);
        assert_eq!(postgres::count_table(settings, "characters").unwrap(), 1);
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            1
        );
        let unchanged: i64 = client
            .query_one(
                "SELECT count(*)::bigint FROM characters
                 WHERE current_health IS NULL AND health_revision = 0
                   AND restore_map_authored = 'map.map2' AND restore_point_id = 'default'",
                &[],
            )
            .unwrap()
            .get(0);
        assert_eq!(unchanged, 1);
        let equipped: i64 = client
            .query_one(
                "SELECT count(*)::bigint FROM item_instances
                 WHERE state = 'live' AND location_kind = 'equipped' AND equipment_slot = 'weapon'",
                &[],
            )
            .unwrap()
            .get(0);
        assert_eq!(equipped, 1);

        let mut service = PersistenceService::open_postgresql(settings).unwrap();
        let loaded = service
            .load_owned_character(&login("alice"), CharacterId::from_raw(1))
            .unwrap()
            .unwrap();
        assert_eq!(loaded.current_health_milli, None);
        assert_eq!(loaded.health_revision, 0);
        assert_eq!(loaded.restore.map_authored, "map.map2");
        assert!(service.item(ItemInstanceId::from_raw(1)).unwrap().is_some());
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn older_map_or_equipment_write_does_not_raise_stored_health() {
    with_db(|dir, settings| {
        let mut service = open(dir, settings);
        let alice = login("alice");
        let entry = service.create_character(&alice, "Alice").unwrap();
        service
            .save_snapshot(PersistentCharacterSnapshot {
                character_id: entry.character_id,
                persistence_revision: 2,
                restore: RestoreIntent {
                    map_authored: "map.map1".into(),
                    point_id: "default".into(),
                    checkpoint_id: None,
                },
                instance_exit: None,
                current_health_milli: Some(15_000),
                health_revision: 2,
            })
            .unwrap();
        service
            .save_snapshot(PersistentCharacterSnapshot {
                character_id: entry.character_id,
                persistence_revision: 3,
                restore: RestoreIntent {
                    map_authored: "map.map2".into(),
                    point_id: "default".into(),
                    checkpoint_id: None,
                },
                instance_exit: None,
                current_health_milli: Some(20_000),
                health_revision: 1,
            })
            .unwrap();
        let loaded = service
            .load_owned_character(&alice, entry.character_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.restore.map_authored, "map.map2");
        assert_eq!(loaded.current_health_milli, Some(15_000));
        assert_eq!(loaded.health_revision, 2);

        service
            .commit_durable(&DurableCommand {
                key: "equip-after-health".into(),
                expected_revisions: vec![(entry.character_id, 3)],
                place_new: vec![PlaceNewItem {
                    owner: entry.character_id,
                    definition_content_id: ContentId::from_raw(30_001),
                    quantity: 1,
                    location: CharacterItemLocation::Equipped {
                        slot: DurableEquipmentSlot::Weapon,
                    },
                }],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),
                reserved_uses: Vec::new(),
            })
            .unwrap();
        let loaded = service
            .load_owned_character(&alice, entry.character_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.persistence_revision, 4);
        assert_eq!(loaded.current_health_milli, Some(15_000));
        assert_eq!(loaded.health_revision, 2);

        service
            .save_snapshot(PersistentCharacterSnapshot {
                character_id: entry.character_id,
                persistence_revision: 2,
                restore: RestoreIntent {
                    map_authored: "map.map1".into(),
                    point_id: "default".into(),
                    checkpoint_id: None,
                },
                instance_exit: None,
                current_health_milli: Some(4_000),
                health_revision: 3,
            })
            .unwrap();
        let loaded = service
            .load_owned_character(&alice, entry.character_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.restore.map_authored, "map.map2");
        assert_eq!(loaded.current_health_milli, Some(4_000));
        assert_eq!(loaded.health_revision, 3);
    });
}
