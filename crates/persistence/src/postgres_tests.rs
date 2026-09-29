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
    CharacterItemLocation, DurableCommand, DurableContentRules, DurableEquipmentSlot,
    IDENTITY_FILE_NAME, ItemContentRule, ItemOwner, LearnedAbilityWrite, LiveDestination, MoveItem,
    NarrativeWrite, PersistError, PersistenceService, PlaceNewItem,
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

fn open(dir: &Path, settings: &PostgresSettings) -> PersistenceService {
    let mut service = PersistenceService::open_postgresql(dir, settings).unwrap();
    service.set_durable_content_rules(rules());
    service
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn revision_conflict_rolls_the_loser_back() {
    with_db(|dir, settings| {
        let mut first = open(dir, settings);
        let alice = login("alice");
        let entry = first.create_character(&alice, "Alice").unwrap();
        let mut second = PersistenceService::open_postgresql(dir, settings).unwrap();
        second.set_durable_content_rules(rules());
        let winner = DurableCommand {
            key: "winner".into(),
            expected_revisions: vec![(entry.character_id, 1)],
            place_new: vec![place(entry.character_id, 0)],
            moves: Vec::new(),
            retire: Vec::new(),
            narrative: Vec::new(),
            learned: Vec::new(),
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
            })
            .unwrap();
        let item_id = minted.minted_item_ids[0];
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
            })
            .unwrap();
        drop(service);
        let mut service = open(dir, settings);
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
fn invalid_import_preserves_source_and_writes_no_rows() {
    with_db(|dir, settings| {
        let identity = br#"{"schema_version":2,"next_character_id":2,"logins":{"alice":[{"character_id":1,"display_name":"Alice"}]}}"#;
        std::fs::write(dir.join(IDENTITY_FILE_NAME), identity).unwrap();
        let character = dir.join(crate::character_file_name(CharacterId::from_raw(1)));
        let original = br#"{"schema_version":1,"character_id":1,"persistence_revision":4,"restore":{"map_authored":"map.map1","point_id":"default"},"items":[]}"#;
        std::fs::write(&character, original).unwrap();
        let err = match PersistenceService::open_postgresql(dir, settings) {
            Ok(_) => panic!("corrupt import must fail closed"),
            Err(err) => err,
        };
        assert!(
            matches!(
                err,
                PersistError::Json { .. } | PersistError::Corrupt { .. }
            ),
            "{err}"
        );
        assert_eq!(
            std::fs::read(dir.join(IDENTITY_FILE_NAME)).unwrap(),
            identity
        );
        assert_eq!(std::fs::read(&character).unwrap(), original);
        assert!(dir.join(postgres::DURABLE_WRITER_FILE).exists());
        let err = match PersistenceService::open(dir) {
            Ok(_) => panic!("a fenced directory must not accept the file writer"),
            Err(err) => err,
        };
        assert!(matches!(err, PersistError::Migration { .. }), "{err}");
        assert_eq!(postgres::count_table(settings, "dev_users").unwrap(), 0);
        assert_eq!(postgres::count_table(settings, "characters").unwrap(), 0);
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn supported_v1_import_preserves_identity_and_invents_nothing() {
    with_db(|dir, settings| {
        let identity =
            br#"{"schema_version":1,"next_character_id":8,"logins":{"zed":7,"alice":1}}"#;
        std::fs::write(dir.join(IDENTITY_FILE_NAME), identity).unwrap();
        let character = dir.join(crate::character_file_name(CharacterId::from_raw(7)));
        let body = br#"{"schema_version":1,"character_id":7,"persistence_revision":4,"restore":{"map_authored":"map.map2","point_id":"gate","checkpoint_id":"cp"}}"#;
        std::fs::write(&character, body).unwrap();
        let mut service = open(dir, settings);
        assert_eq!(
            std::fs::read(dir.join(IDENTITY_FILE_NAME)).unwrap(),
            identity
        );
        assert_eq!(std::fs::read(&character).unwrap(), body);
        let zed = login("zed");
        let alice = login("alice");
        let loaded = service
            .load_owned_character(&zed, CharacterId::from_raw(7))
            .unwrap()
            .unwrap();
        assert_eq!(loaded.persistence_revision, 4);
        assert_eq!(loaded.restore.map_authored, "map.map2");
        assert_eq!(loaded.restore.point_id, "gate");
        assert!(
            service
                .narrative(CharacterId::from_raw(7))
                .unwrap()
                .facts
                .is_empty()
        );
        assert!(
            service
                .narrative(CharacterId::from_raw(7))
                .unwrap()
                .learned_abilities
                .is_empty()
        );
        let alice_roster = service.roster(&alice).unwrap();
        assert_eq!(alice_roster.len(), 1);
        assert_eq!(alice_roster[0].character_id, CharacterId::from_raw(1));
        assert!(service.item(ItemInstanceId::from_raw(1)).unwrap().is_none());
        let err = match PersistenceService::open(dir) {
            Ok(_) => panic!("file writer must stay closed after postgresql cutover"),
            Err(err) => err,
        };
        assert!(matches!(err, PersistError::Migration { .. }), "{err}");
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
fn file_writer_is_fenced_before_import_commits() {
    with_db(|dir, settings| {
        let identity = br#"{"schema_version":1,"next_character_id":8,"logins":{"zed":7}}"#;
        std::fs::write(dir.join(IDENTITY_FILE_NAME), identity).unwrap();
        let character = dir.join(crate::character_file_name(CharacterId::from_raw(7)));
        let body = br#"{"schema_version":1,"character_id":7,"persistence_revision":4,"restore":{"map_authored":"map.map2","point_id":"gate"}}"#;
        std::fs::write(&character, body).unwrap();
        postgres::fence_file_writer(dir).unwrap();
        let err = match PersistenceService::open(dir) {
            Ok(_) => panic!("file writer must refuse once the cutover fence exists"),
            Err(err) => err,
        };
        assert!(matches!(err, PersistError::Migration { .. }), "{err}");
        let mut service = open(dir, settings);
        let loaded = service
            .load_owned_character(&login("zed"), CharacterId::from_raw(7))
            .unwrap()
            .unwrap();
        assert_eq!(loaded.restore.point_id, "gate");
        std::fs::write(&character, b"{\"schema_version\":1}").unwrap();
        let again = PersistenceService::open_postgresql(dir, settings).unwrap();
        drop(again);
        let loaded = service
            .load_owned_character(&login("zed"), CharacterId::from_raw(7))
            .unwrap()
            .unwrap();
        assert_eq!(loaded.restore.point_id, "gate");
        assert_eq!(loaded.persistence_revision, 4);
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn marker_write_failure_does_not_commit_import() {
    with_db(|dir, settings| {
        postgres::fail_next_marker_write();
        let err = match PersistenceService::open_postgresql(dir, settings) {
            Ok(_) => panic!("marker failure must not open a writer"),
            Err(err) => err,
        };
        assert!(matches!(err, PersistError::Io { .. }), "{err}");
        assert!(!dir.join(postgres::DURABLE_WRITER_FILE).exists());
        assert_eq!(postgres::count_table(settings, "characters").unwrap(), 0);
        let mut files = PersistenceService::open(dir).unwrap();
        files.create_character(&login("alice"), "Alice").unwrap();
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
    with_db(|dir, settings| {
        let roles = postgres::provision_ephemeral_roles(settings).expect("distinct test roles");
        let mut service = PersistenceService::open_postgresql(dir, &roles.runtime).unwrap();
        service.set_durable_content_rules(rules());
        let alice = login("alice");
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
            })
            .unwrap();
        assert_eq!(committed.minted_item_ids.len(), 1);
        assert!(postgres::runtime_cannot_create_table(&roles.runtime).unwrap());
    });
}

#[test]
#[ignore = "requires PURGATORY_TEST_DATABASE_URL and does not use Purgatory_dev"]
fn already_open_file_service_cannot_change_source_files_after_cutover() {
    with_db(|dir, settings| {
        let mut files = PersistenceService::open(dir).unwrap();
        let alice = login("alice");
        let entry = files.create_character(&alice, "Alice").unwrap();
        files
            .save_snapshot(crate::PersistentCharacterSnapshot {
                character_id: entry.character_id,
                persistence_revision: 1,
                restore: RestoreIntent {
                    map_authored: "map.map1".into(),
                    point_id: "default".into(),
                    checkpoint_id: None,
                },
                instance_exit: None,
            })
            .unwrap();
        let identity = std::fs::read(dir.join(IDENTITY_FILE_NAME)).unwrap();
        let character_path = dir.join(crate::character_file_name(entry.character_id));
        let character = std::fs::read(&character_path).unwrap();
        let mut durable = open(dir, settings);
        let save = files.save_snapshot(crate::PersistentCharacterSnapshot {
            character_id: entry.character_id,
            persistence_revision: 2,
            restore: RestoreIntent {
                map_authored: "map.map2".into(),
                point_id: "after-cutover".into(),
                checkpoint_id: None,
            },
            instance_exit: None,
        });
        assert!(
            matches!(save, Err(PersistError::Migration { .. })),
            "{save:?}"
        );
        let created = files.create_character(&login("bob"), "Bob");
        assert!(
            matches!(created, Err(PersistError::Migration { .. })),
            "{created:?}"
        );
        assert_eq!(
            std::fs::read(dir.join(IDENTITY_FILE_NAME)).unwrap(),
            identity
        );
        assert_eq!(std::fs::read(&character_path).unwrap(), character);
        let loaded = durable
            .load_owned_character(&alice, entry.character_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.restore.point_id, "default");
        assert_eq!(loaded.persistence_revision, 1);
        assert!(durable.roster(&login("bob")).unwrap().is_empty());
        assert_eq!(durable.roster(&alice).unwrap().len(), 1);
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
        };
        // The persistence worker returns this Result on its oneshot. It does
        // not decide whether the command was applied.
        service.discard_connection_after_next_commit_for_test();
        let err = service.commit_durable(&command).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("commit outcome unknown"), "{text}");
        assert!(!text.contains("not committed"), "{text}");
        let again = service.commit_durable(&command).unwrap_err();
        let again_text = again.to_string();
        assert!(
            again_text.contains("commit outcome unknown"),
            "{again_text}"
        );
        let mut recovered = open(dir, settings);
        let result = recovered.commit_durable(&command).unwrap();
        assert_eq!(result.revisions, vec![(entry.character_id, 2)]);
        assert_eq!(result.minted_item_ids.len(), 1);
        let loaded = recovered
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
        let repeated = recovered.commit_durable(&command).unwrap();
        assert_eq!(repeated, result);
        assert_eq!(
            postgres::count_table(settings, "item_instances").unwrap(),
            1
        );
    });
}
