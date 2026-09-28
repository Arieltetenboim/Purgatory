use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use purgatory_common::{CharacterId, ContentId, InstanceExitContext, ItemInstanceId};

use crate::character::PersistentCharacter;
use crate::domain::{
    CharacterItemLocation, ClockCheckpointKind, DurableContentRules, DurableEquipmentSlot,
    ItemContentRule, MapDropPosition, MapDropRecord, OwnershipChange, PersistentItem,
};
use crate::error::PersistError;
use crate::journal::{CrashPoint, testing_crash_armed, testing_crash_at, testing_wal_path};
use crate::{DirectorySync, FileCharacterRepository, directory_sync_capability};

fn temp_dir() -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "purgatory-durable-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
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
            content_id: ContentId::from_raw(30_006),
            stack_limit: 1,
            equip_slot: Some(DurableEquipmentSlot::Weapon),
            retired: false,
        })
        .unwrap();
    rules
        .insert_map(ContentId::from_raw(50_001), false)
        .unwrap();
    rules
}

fn open_with_rules(dir: &Path) -> FileCharacterRepository {
    let mut repo = FileCharacterRepository::open(dir).unwrap();
    repo.set_durable_content_rules(rules());
    repo
}

fn character(id: u64, revision: u64, items: Vec<PersistentItem>) -> PersistentCharacter {
    let mut character = PersistentCharacter::new_default(CharacterId::from_raw(id));
    character.persistence_revision = revision;
    character.items = items;
    character
}

fn item(id: u64, content: u32, quantity: u32, slot: u16) -> PersistentItem {
    PersistentItem {
        item_instance_id: ItemInstanceId::from_raw(id),
        definition_content_id: ContentId::from_raw(content),
        quantity,
        location: CharacterItemLocation::Inventory { slot },
    }
}

fn owners(repo: &FileCharacterRepository) -> Vec<(u64, &'static str, u64)> {
    let mut found = Vec::new();
    for id in [10u64, 11] {
        if let Some(character) = repo.load(CharacterId::from_raw(id)).unwrap() {
            for owned in character.items {
                found.push((owned.item_instance_id.raw(), "character", id));
            }
        }
    }
    for drop in repo.map_drops().unwrap() {
        found.push((drop.item_instance_id.raw(), "map", drop.drop_id));
    }
    found.sort_by_key(|entry| entry.0);
    found
}

#[test]
fn valid_v1_migrates_once_and_preserves_identity_and_restore() {
    let dir = temp_dir();
    let id = CharacterId::from_raw(7);
    let path = dir.join(crate::character_file_name(id));
    std::fs::write(
        &path,
        r#"{"schema_version":1,"character_id":7,"persistence_revision":4,"restore":{"map_authored":"map.map2","point_id":"gate","checkpoint_id":"c1"},"instance_exit":{"reason":"logout"}}"#,
    )
    .unwrap();
    let repo = FileCharacterRepository::open(&dir).unwrap();
    let loaded = repo.load(id).unwrap().unwrap();
    assert_eq!(loaded.schema_version, 2);
    assert_eq!(loaded.character_id, id);
    assert_eq!(loaded.persistence_revision, 5);
    assert_eq!(loaded.restore.map_authored, "map.map2");
    assert_eq!(loaded.restore.point_id, "gate");
    assert_eq!(loaded.restore.checkpoint_id.as_deref(), Some("c1"));
    assert_eq!(
        loaded.instance_exit,
        Some(InstanceExitContext {
            reason: Some("logout".into())
        })
    );
    assert!(loaded.items.is_empty());
    drop(repo);
    let again = FileCharacterRepository::open(&dir)
        .unwrap()
        .load(id)
        .unwrap()
        .unwrap();
    assert_eq!(again.persistence_revision, 5);
    assert!(again.items.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_v1_is_not_replaced_with_a_blank_character() {
    let dir = temp_dir();
    let id = CharacterId::from_raw(8);
    let path = dir.join(crate::character_file_name(id));
    let original = br#"{"schema_version":1,"character_id":8,"persistence_revision":4,"restore":{"map_authored":"","point_id":"gate"}}"#.to_vec();
    std::fs::write(&path, &original).unwrap();
    assert!(FileCharacterRepository::open(&dir).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn revision_overflow_keeps_the_v1_file() {
    let dir = temp_dir();
    let id = CharacterId::from_raw(9);
    let path = dir.join(crate::character_file_name(id));
    let original = format!(
        r#"{{"schema_version":1,"character_id":9,"persistence_revision":{},"restore":{{"map_authored":"map.map1","point_id":"default"}}}}"#,
        u64::MAX
    );
    std::fs::write(&path, &original).unwrap();
    assert!(matches!(
        FileCharacterRepository::open(&dir),
        Err(PersistError::Migration { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), original.into_bytes());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_does_not_replace_an_unreadable_record() {
    let dir = temp_dir();
    let repo = FileCharacterRepository::open(&dir).unwrap();
    let id = CharacterId::from_raw(15);
    let path = repo.path_for(id);
    let original = b"{not json".to_vec();
    std::fs::write(&path, &original).unwrap();
    let err = repo
        .save(&PersistentCharacter::new_default(id))
        .unwrap_err();
    assert!(matches!(err, PersistError::Json { .. }), "{err}");
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_content_is_rejected_without_a_commit() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    let before = std::fs::read(testing_wal_path(&dir)).unwrap();
    let unknown = character(10, 1, vec![item(reserved.first.raw(), 30_099, 1, 0)]);
    assert!(matches!(
        repo.save(&unknown),
        Err(PersistError::ContentRejected { .. })
    ));
    let over_stack = character(10, 1, vec![item(reserved.first.raw(), 30_011, 21, 0)]);
    assert!(matches!(
        repo.save(&over_stack),
        Err(PersistError::ContentRejected { .. })
    ));
    let mut wrong_slot = character(10, 1, vec![item(reserved.first.raw(), 30_006, 1, 0)]);
    wrong_slot.items[0].location = CharacterItemLocation::Equipped {
        slot: DurableEquipmentSlot::Headwear,
    };
    assert!(matches!(
        repo.save(&wrong_slot),
        Err(PersistError::ContentRejected { .. })
    ));
    let mut wrong_domain = character(10, 1, vec![item(reserved.first.raw(), 30_011, 1, 0)]);
    wrong_domain.items[0].definition_content_id = ContentId::from_raw(40_001);
    assert!(matches!(
        repo.save(&wrong_domain),
        Err(PersistError::ContentRejected { .. })
    ));
    assert_eq!(std::fs::read(testing_wal_path(&dir)).unwrap(), before);
    assert!(repo.load(CharacterId::from_raw(10)).unwrap().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn retired_content_blocks_load_and_preserves_the_record() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    repo.save(&character(
        10,
        1,
        vec![item(reserved.first.raw(), 30_011, 2, 0)],
    ))
    .unwrap();
    repo.compact_durable_log().unwrap();
    let path = repo.path_for(CharacterId::from_raw(10));
    let bytes = std::fs::read(&path).unwrap();
    drop(repo);
    let mut retired = rules();
    retired
        .insert_item(ItemContentRule {
            content_id: ContentId::from_raw(30_011),
            stack_limit: 20,
            equip_slot: None,
            retired: true,
        })
        .unwrap();
    let mut repo = FileCharacterRepository::open(&dir).unwrap();
    repo.set_durable_content_rules(retired);
    assert!(matches!(
        repo.load(CharacterId::from_raw(10)),
        Err(PersistError::ContentRejected { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn duplicate_item_ids_fail_before_a_second_owner_is_written() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    let owned = item(reserved.first.raw(), 30_011, 1, 0);
    repo.save(&character(10, 1, vec![owned.clone()])).unwrap();
    let wal = std::fs::read(testing_wal_path(&dir)).unwrap();
    let duplicate = OwnershipChange {
        characters: vec![
            character(10, 2, vec![owned.clone()]),
            character(11, 1, vec![owned]),
        ],
        drops_upsert: Vec::new(),
        drops_remove: Vec::new(),
    };
    assert!(matches!(
        repo.commit_ownership(duplicate),
        Err(PersistError::Integrity { .. })
    ));
    assert_eq!(std::fs::read(testing_wal_path(&dir)).unwrap(), wal);
    drop(repo);
    let repo = open_with_rules(&dir);
    assert_eq!(owners(&repo), vec![(reserved.first.raw(), "character", 10)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hand_edited_duplicate_ids_fail_closed_without_rewriting() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(2).unwrap();
    repo.save(&character(
        10,
        1,
        vec![item(reserved.first.raw(), 30_011, 1, 0)],
    ))
    .unwrap();
    repo.save(&character(
        11,
        1,
        vec![item(reserved.first.raw() + 1, 30_011, 1, 0)],
    ))
    .unwrap();
    repo.compact_durable_log().unwrap();
    drop(repo);
    let path_b = dir.join(crate::character_file_name(CharacterId::from_raw(11)));
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path_b).unwrap()).unwrap();
    value["items"][0]["item_instance_id"] = serde_json::json!(reserved.first.raw());
    let edited = serde_json::to_vec_pretty(&value).unwrap();
    std::fs::write(&path_b, &edited).unwrap();
    assert!(FileCharacterRepository::open(&dir).is_err());
    assert_eq!(std::fs::read(&path_b).unwrap(), edited);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn item_has_one_owner_across_character_map_and_restart() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    repo.checkpoint_active_clock(20, ClockCheckpointKind::Periodic)
        .unwrap();
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    let id = reserved.first.raw();
    repo.save(&character(10, 1, vec![item(id, 30_011, 4, 3)]))
        .unwrap();
    repo.save(&character(11, 1, Vec::new())).unwrap();
    let map_drop = MapDropRecord {
        item_instance_id: reserved.first,
        definition_content_id: ContentId::from_raw(30_011),
        quantity: 4,
        map_content_id: ContentId::from_raw(50_001),
        map_space_key: "map1.channel-a".into(),
        drop_id: 77,
        position: MapDropPosition {
            x_milli: -1500,
            y_milli: 250,
        },
        expiry_tick: 1_000,
        public_at_tick: 80,
        eligible_character_ids: vec![CharacterId::from_raw(10), CharacterId::from_raw(11)],
    };
    repo.commit_ownership(OwnershipChange {
        characters: vec![character(10, 2, Vec::new())],
        drops_upsert: vec![map_drop],
        drops_remove: Vec::new(),
    })
    .unwrap();
    drop(repo);
    let repo = open_with_rules(&dir);
    assert_eq!(owners(&repo), vec![(id, "map", 77)]);
    let loaded = repo.map_drops().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].quantity, 4);
    assert_eq!(
        loaded[0].remaining_expiry_ticks(repo.active_clock_tick().unwrap()),
        980
    );
    assert_eq!(
        loaded[0].remaining_restricted_ticks(repo.active_clock_tick().unwrap()),
        60
    );
    repo.commit_ownership(OwnershipChange {
        characters: vec![character(11, 2, vec![item(id, 30_011, 4, 1)])],
        drops_upsert: Vec::new(),
        drops_remove: vec![reserved.first],
    })
    .unwrap();
    drop(repo);
    let repo = open_with_rules(&dir);
    assert_eq!(owners(&repo), vec![(id, "character", 11)]);
    assert!(repo.map_drops().unwrap().is_empty());
    assert!(
        repo.load(CharacterId::from_raw(10))
            .unwrap()
            .unwrap()
            .items
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reserved_ids_are_not_reused_after_restart() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let first = repo.reserve_item_instance_ids(3).unwrap();
    assert_eq!(first.first.raw(), 1);
    assert_eq!(first.count, 3);
    drop(repo);
    let repo = open_with_rules(&dir);
    let next = repo.reserve_item_instance_ids(1).unwrap();
    assert_eq!(next.first.raw(), 4);
    assert!(!first.contains(next.first));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn incomplete_log_tail_is_not_a_commit() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    repo.save(&character(
        10,
        1,
        vec![item(reserved.first.raw(), 30_011, 1, 0)],
    ))
    .unwrap();
    let wal = testing_wal_path(&dir);
    let good = std::fs::read(&wal).unwrap();
    std::fs::write(&wal, [good.clone(), b"torn".to_vec()].concat()).unwrap();
    drop(repo);
    let repo = open_with_rules(&dir);
    assert_eq!(owners(&repo), vec![(reserved.first.raw(), "character", 10)]);
    let repaired = std::fs::read(&wal).unwrap();
    assert_eq!(repaired, good);
    assert!(!repaired.ends_with(b"torn"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn corrupt_committed_frame_fails_closed_and_keeps_checkpoints() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    repo.save(&character(
        10,
        1,
        vec![item(reserved.first.raw(), 30_011, 1, 0)],
    ))
    .unwrap();
    repo.compact_durable_log().unwrap();
    repo.save(&character(
        10,
        2,
        vec![item(reserved.first.raw(), 30_011, 2, 0)],
    ))
    .unwrap();
    repo.save(&character(
        10,
        3,
        vec![item(reserved.first.raw(), 30_011, 3, 0)],
    ))
    .unwrap();
    let character_path = repo.path_for(CharacterId::from_raw(10));
    let before = std::fs::read(&character_path).unwrap();
    let wal = testing_wal_path(&dir);
    let mut bytes = std::fs::read(&wal).unwrap();
    let len = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    let frame_end = 24 + len + 4;
    assert!(bytes.len() > frame_end, "need a later committed frame");
    bytes[24] ^= 0xff;
    std::fs::write(&wal, &bytes).unwrap();
    drop(repo);
    assert!(matches!(
        FileCharacterRepository::open(&dir),
        Err(PersistError::Integrity { .. })
    ));
    assert_eq!(std::fs::read(&character_path).unwrap(), before);
    assert_eq!(std::fs::read(&wal).unwrap(), bytes);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn first_checkpoint_crash_before_manifest_replays_the_whole_log() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    repo.save(&character(
        10,
        1,
        vec![item(reserved.first.raw(), 30_011, 1, 0)],
    ))
    .unwrap();
    testing_crash_at(CrashPoint::CheckpointDataBeforeManifest);
    assert!(repo.compact_durable_log().is_err());
    let staged = dir.join("char_000000000000000a.json.next");
    assert!(staged.exists(), "the crash leaves the staged checkpoint");
    drop(repo);
    let repo = open_with_rules(&dir);
    assert_eq!(owners(&repo), vec![(reserved.first.raw(), "character", 10)]);
    assert!(!staged.exists());
    assert!(!dir.join("durable_manifest.json").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn checkpoint_crash_after_manifest_rolls_staged_files_forward() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    let id = reserved.first.raw();
    repo.save(&character(10, 1, vec![item(id, 30_011, 1, 0)]))
        .unwrap();
    repo.compact_durable_log().unwrap();
    repo.commit_ownership(OwnershipChange {
        characters: vec![character(10, 2, Vec::new())],
        drops_upsert: vec![map_drop(reserved.first, 9)],
        drops_remove: Vec::new(),
    })
    .unwrap();
    testing_crash_at(CrashPoint::CheckpointManifestBeforeInstall);
    assert!(repo.compact_durable_log().is_err());
    assert!(
        repo.save(&character(10, 3, Vec::new())).is_err(),
        "a store that failed after its manifest must be reopened"
    );
    let staged = dir.join("char_000000000000000a.json.next");
    assert!(staged.exists());
    drop(repo);
    let repo = open_with_rules(&dir);
    assert!(!staged.exists());
    assert_eq!(owners(&repo), vec![(id, "map", 9)]);
    let on_disk: PersistentCharacter =
        serde_json::from_slice(&std::fs::read(repo.path_for(CharacterId::from_raw(10))).unwrap())
            .unwrap();
    assert_eq!(on_disk.persistence_revision, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_log_after_a_checkpoint_fails_closed() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    repo.save(&character(10, 1, Vec::new())).unwrap();
    repo.compact_durable_log().unwrap();
    drop(repo);
    std::fs::remove_file(testing_wal_path(&dir)).unwrap();
    let path = dir.join(crate::character_file_name(CharacterId::from_raw(10)));
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(
        FileCharacterRepository::open(&dir),
        Err(PersistError::Integrity { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn staged_log_that_does_not_follow_the_manifest_is_rejected() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    repo.save(&character(10, 1, Vec::new())).unwrap();
    testing_crash_at(CrashPoint::LogReplacementBeforeInstall);
    assert!(repo.compact_durable_log().is_err());
    drop(repo);
    let wal = testing_wal_path(&dir);
    let staged = dir.join("ownership.wal.next");
    let mut header = std::fs::read(&staged).unwrap();
    header[12..20].copy_from_slice(&7u64.to_le_bytes());
    std::fs::write(&staged, &header).unwrap();
    std::fs::remove_file(&wal).unwrap();
    assert!(matches!(
        FileCharacterRepository::open(&dir),
        Err(PersistError::Integrity { .. })
    ));
    assert_eq!(std::fs::read(&staged).unwrap(), header);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn deleted_manifest_with_checkpoint_files_fails_closed() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    repo.save(&character(10, 1, Vec::new())).unwrap();
    repo.compact_durable_log().unwrap();
    drop(repo);
    std::fs::remove_file(dir.join("durable_manifest.json")).unwrap();
    assert!(matches!(
        FileCharacterRepository::open(&dir),
        Err(PersistError::Integrity { .. })
    ));
    assert!(
        dir.join(crate::character_file_name(CharacterId::from_raw(10)))
            .exists()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn compacted_log_replays_from_the_checkpoint_and_rejects_edited_checkpoints() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    let id = reserved.first.raw();
    repo.save(&character(10, 1, vec![item(id, 30_011, 1, 0)]))
        .unwrap();
    repo.compact_durable_log().unwrap();
    drop(repo);
    let repo = open_with_rules(&dir);
    assert_eq!(owners(&repo), vec![(id, "character", 10)]);
    let next = repo.reserve_item_instance_ids(1).unwrap();
    assert_eq!(next.first.raw(), id + 1);
    drop(repo);

    let path = dir.join(crate::character_file_name(CharacterId::from_raw(10)));
    let original = std::fs::read(&path).unwrap();
    let edited = String::from_utf8(original.clone())
        .unwrap()
        .replace("\"default\"", "\"elsewhere\"");
    std::fs::write(&path, &edited).unwrap();
    assert!(matches!(
        FileCharacterRepository::open(&dir),
        Err(PersistError::Integrity { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), edited.as_bytes());
    std::fs::write(&path, &original).unwrap();

    let manifest = dir.join("durable_manifest.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    value["clock_tick"] = serde_json::json!(999);
    let tampered = serde_json::to_vec(&value).unwrap();
    std::fs::write(&manifest, &tampered).unwrap();
    assert!(matches!(
        FileCharacterRepository::open(&dir),
        Err(PersistError::Integrity { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(std::fs::read(&manifest).unwrap(), tampered);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_wal_write_does_not_acknowledge_or_create_a_character() {
    let dir = temp_dir();
    std::fs::create_dir(dir.join("ownership.wal")).unwrap();
    let repo = FileCharacterRepository::open(&dir);
    assert!(matches!(repo, Err(PersistError::Io { .. })));
    assert!(std::fs::read_dir(&dir).unwrap().all(|entry| {
        entry.unwrap().file_name().to_string_lossy() != "char_000000000000000a.json"
    }));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clock_failure_does_not_advance_drop_time_and_the_bound_holds() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    repo.checkpoint_active_clock(100, ClockCheckpointKind::Shutdown)
        .unwrap();
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    repo.save(&PersistentCharacter::new_default(CharacterId::from_raw(10)))
        .unwrap();
    repo.commit_ownership(OwnershipChange {
        characters: vec![character(10, 2, Vec::new())],
        drops_upsert: vec![MapDropRecord {
            item_instance_id: reserved.first,
            definition_content_id: ContentId::from_raw(30_011),
            quantity: 1,
            map_content_id: ContentId::from_raw(50_001),
            map_space_key: "map1.default".into(),
            drop_id: 5,
            position: MapDropPosition {
                x_milli: 0,
                y_milli: 0,
            },
            expiry_tick: 1_000,
            public_at_tick: 400,
            eligible_character_ids: vec![CharacterId::from_raw(10)],
        }],
        drops_remove: Vec::new(),
    })
    .unwrap();
    let observed_before_crash = 129u64;
    assert!(observed_before_crash - 100 <= crate::ACTIVE_SERVER_TICKS_PER_SECOND);
    let err = repo
        .checkpoint_active_clock(131, ClockCheckpointKind::Periodic)
        .unwrap_err();
    assert!(matches!(
        err,
        PersistError::ClockBound {
            committed_tick: 100,
            requested_tick: 131
        }
    ));
    assert_eq!(repo.active_clock_tick().unwrap(), 100);
    let wal = testing_wal_path(&dir);
    let committed_log = std::fs::read(&wal).unwrap();
    let original_perms = std::fs::metadata(&wal).unwrap().permissions();
    let mut readonly = original_perms.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&wal, readonly).unwrap();
    let failed = repo.checkpoint_active_clock(110, ClockCheckpointKind::Periodic);
    std::fs::set_permissions(&wal, original_perms).unwrap();
    assert!(matches!(failed, Err(PersistError::Io { .. })), "{failed:?}");
    assert_eq!(std::fs::read(&wal).unwrap(), committed_log);
    assert_eq!(repo.active_clock_tick().unwrap(), 100);
    drop(repo);
    let repo = open_with_rules(&dir);
    let clock = repo.active_clock_tick().unwrap();
    assert_eq!(clock, 100);
    let loaded_drop = &repo.map_drops().unwrap()[0];
    let remaining = loaded_drop.remaining_expiry_ticks(clock);
    let regained = remaining - loaded_drop.remaining_expiry_ticks(observed_before_crash);
    assert_eq!(remaining, 900);
    assert!(regained <= crate::ACTIVE_SERVER_TICKS_PER_SECOND);
    assert_eq!(
        loaded_drop.remaining_restricted_ticks(clock)
            - loaded_drop.remaining_restricted_ticks(observed_before_crash),
        regained
    );
    drop(repo);
    let repo = open_with_rules(&dir);
    assert_eq!(repo.active_clock_tick().unwrap(), 100);
    assert_eq!(
        repo.map_drops().unwrap()[0].remaining_expiry_ticks(100),
        900
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn equal_revision_with_different_content_is_an_integrity_error() {
    let dir = temp_dir();
    let repo = FileCharacterRepository::open(&dir).unwrap();
    let mut character = PersistentCharacter::new_default(CharacterId::from_raw(21));
    character.persistence_revision = 3;
    repo.save(&character).unwrap();
    character.restore.point_id = "other".into();
    let err = repo.save(&character).unwrap_err();
    assert!(matches!(err, PersistError::Integrity { .. }), "{err}");
    let loaded = repo.load(CharacterId::from_raw(21)).unwrap().unwrap();
    assert_eq!(loaded.restore.point_id, "default");
    let _ = std::fs::remove_dir_all(&dir);
}

fn map_drop(id: ItemInstanceId, drop_id: u64) -> MapDropRecord {
    MapDropRecord {
        item_instance_id: id,
        definition_content_id: ContentId::from_raw(30_011),
        quantity: 1,
        map_content_id: ContentId::from_raw(50_001),
        map_space_key: "map1.default".into(),
        drop_id,
        position: MapDropPosition {
            x_milli: 0,
            y_milli: 0,
        },
        expiry_tick: 10_000,
        public_at_tick: 0,
        eligible_character_ids: Vec::new(),
    }
}

#[test]
fn compacted_checkpoint_crash_before_manifest_replays_the_committed_log() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    let id = reserved.first.raw();
    repo.save(&character(10, 1, vec![item(id, 30_011, 1, 0)]))
        .unwrap();
    repo.save(&character(11, 1, Vec::new())).unwrap();
    repo.compact_durable_log().unwrap();
    testing_crash_at(CrashPoint::CheckpointDataBeforeManifest);
    let _ = repo.commit_ownership(OwnershipChange {
        characters: vec![character(10, 2, Vec::new())],
        drops_upsert: vec![map_drop(reserved.first, 77)],
        drops_remove: Vec::new(),
    });
    let _ = repo.compact_durable_log();
    assert!(
        !testing_crash_armed(),
        "the checkpoint crash point must run"
    );
    drop(repo);
    let repo = open_with_rules(&dir);
    assert_eq!(owners(&repo), vec![(id, "map", 77)]);
    repo.commit_ownership(OwnershipChange {
        characters: vec![character(11, 2, vec![item(id, 30_011, 1, 0)])],
        drops_upsert: Vec::new(),
        drops_remove: vec![reserved.first],
    })
    .unwrap();
    drop(repo);
    let repo = open_with_rules(&dir);
    assert_eq!(owners(&repo), vec![(id, "character", 11)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn compaction_crash_while_the_log_is_replaced_recovers_the_surviving_log() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    let id = reserved.first.raw();
    repo.save(&character(10, 1, vec![item(id, 30_011, 1, 0)]))
        .unwrap();
    repo.save(&character(10, 2, vec![item(id, 30_011, 2, 0)]))
        .unwrap();
    testing_crash_at(CrashPoint::LogReplacementBeforeInstall);
    assert!(repo.compact_durable_log().is_err());
    assert!(!testing_crash_armed());
    drop(repo);
    // A filesystem whose replace is not one atomic step may drop the old name
    // before the new one appears.
    let _ = std::fs::remove_file(testing_wal_path(&dir));
    let repo = open_with_rules(&dir);
    let loaded = repo.load(CharacterId::from_raw(10)).unwrap().unwrap();
    assert_eq!(loaded.persistence_revision, 2);
    assert_eq!(loaded.items[0].quantity, 2);
    assert_eq!(
        repo.reserve_item_instance_ids(1).unwrap().first.raw(),
        id + 1
    );
    assert!(testing_wal_path(&dir).exists());
    drop(repo);
    let repo = open_with_rules(&dir);
    assert_eq!(owners(&repo), vec![(id, "character", 10)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn complete_final_frame_with_a_bad_checksum_fails_closed_without_truncation() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    repo.save(&character(
        10,
        1,
        vec![item(reserved.first.raw(), 30_011, 1, 0)],
    ))
    .unwrap();
    drop(repo);
    let wal = testing_wal_path(&dir);
    let mut bytes = std::fs::read(&wal).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x5a;
    std::fs::write(&wal, &bytes).unwrap();
    assert!(matches!(
        FileCharacterRepository::open(&dir),
        Err(PersistError::Integrity { .. })
    ));
    assert_eq!(std::fs::read(&wal).unwrap(), bytes);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn consumed_and_merged_item_ids_are_never_new_items_again() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(3).unwrap();
    let [kept, merged, fresh] = [0, 1, 2].map(|offset| reserved.first.raw() + offset);
    repo.save(&character(
        10,
        1,
        vec![item(kept, 30_011, 2, 0), item(merged, 30_011, 3, 1)],
    ))
    .unwrap();
    repo.save(&character(10, 2, vec![item(kept, 30_011, 5, 0)]))
        .unwrap();
    repo.save(&character(10, 3, Vec::new())).unwrap();
    let resurrect = |repo: &FileCharacterRepository, revision: u64, id: u64| {
        repo.commit_ownership(OwnershipChange::character(character(
            11,
            revision,
            vec![item(id, 30_011, 1, 0)],
        )))
    };
    for retired in [merged, kept] {
        assert!(matches!(
            resurrect(&repo, 1, retired),
            Err(PersistError::Integrity { .. })
        ));
    }
    resurrect(&repo, 1, fresh).unwrap();
    drop(repo);
    for compact in [false, true] {
        let repo = open_with_rules(&dir);
        if compact {
            repo.compact_durable_log().unwrap();
            drop(repo);
            let reopened = open_with_rules(&dir);
            for retired in [merged, kept] {
                assert!(matches!(
                    resurrect(&reopened, 2, retired),
                    Err(PersistError::Integrity { .. })
                ));
            }
        } else {
            for retired in [merged, kept] {
                assert!(matches!(
                    resurrect(&repo, 2, retired),
                    Err(PersistError::Integrity { .. })
                ));
            }
        }
    }
    let repo = open_with_rules(&dir);
    assert_eq!(owners(&repo), vec![(fresh, "character", 11)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clock_and_single_character_commits_do_not_rewrite_other_checkpoints() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    for id in [10, 11, 12] {
        repo.save(&character(id, 1, Vec::new())).unwrap();
    }
    repo.compact_durable_log().unwrap();
    let untouched: Vec<_> = [11u64, 12]
        .into_iter()
        .map(|id| {
            let path = repo.path_for(CharacterId::from_raw(id));
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    for tick in 1..=3 {
        repo.checkpoint_active_clock(tick * 10, ClockCheckpointKind::Periodic)
            .unwrap();
    }
    repo.save(&character(10, 2, Vec::new())).unwrap();
    for (path, bytes) in &untouched {
        assert_eq!(&std::fs::read(path).unwrap(), bytes);
    }
    repo.compact_durable_log().unwrap();
    for (path, bytes) in &untouched {
        assert_eq!(&std::fs::read(path).unwrap(), bytes);
    }
    drop(repo);
    let repo = open_with_rules(&dir);
    assert_eq!(repo.active_clock_tick().unwrap(), 30);
    assert_eq!(
        repo.load(CharacterId::from_raw(10))
            .unwrap()
            .unwrap()
            .persistence_revision,
        2
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn log_changed_outside_the_store_is_not_appended_to() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    repo.save(&character(10, 1, Vec::new())).unwrap();
    let wal = testing_wal_path(&dir);
    let mut bytes = std::fs::read(&wal).unwrap();
    bytes.extend_from_slice(b"xx");
    std::fs::write(&wal, &bytes).unwrap();
    assert!(matches!(
        repo.save(&character(10, 2, Vec::new())),
        Err(PersistError::Integrity { .. })
    ));
    assert_eq!(std::fs::read(&wal).unwrap(), bytes);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn many_v1_records_migrate_in_batches_and_checkpoint_as_v2() {
    let dir = temp_dir();
    let ids: Vec<u64> = (1..=300).collect();
    for &id in &ids {
        std::fs::write(
            dir.join(crate::character_file_name(CharacterId::from_raw(id))),
            format!(
                r#"{{"schema_version":1,"character_id":{id},"persistence_revision":{id},"restore":{{"map_authored":"map.map1","point_id":"default"}}}}"#
            ),
        )
        .unwrap();
    }
    let repo = FileCharacterRepository::open(&dir).unwrap();
    drop(repo);
    let repo = FileCharacterRepository::open(&dir).unwrap();
    for &id in &ids {
        let loaded = repo.load(CharacterId::from_raw(id)).unwrap().unwrap();
        assert_eq!(loaded.persistence_revision, id + 1);
    }
    repo.compact_durable_log().unwrap();
    drop(repo);
    let first: PersistentCharacter = serde_json::from_slice(
        &std::fs::read(dir.join(crate::character_file_name(CharacterId::from_raw(1)))).unwrap(),
    )
    .unwrap();
    assert_eq!(first.schema_version, 2);
    assert_eq!(first.persistence_revision, 2);
    let repo = FileCharacterRepository::open(&dir).unwrap();
    assert_eq!(
        repo.load(CharacterId::from_raw(300))
            .unwrap()
            .unwrap()
            .persistence_revision,
        301
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn staged_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".next")
        })
        .count()
}

#[test]
fn incremental_checkpoint_stages_bounded_batches_and_restages_changed_characters() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    for id in 10..20 {
        repo.save(&character(id, 1, Vec::new())).unwrap();
    }
    repo.compact_durable_log().unwrap();
    for id in 10..15 {
        repo.save(&character(id, 2, Vec::new())).unwrap();
    }
    assert!(!repo.testing_checkpoint_step(2).unwrap());
    assert_eq!(staged_files(&dir), 2);
    assert!(!repo.testing_checkpoint_step(2).unwrap());
    assert_eq!(staged_files(&dir), 4);
    repo.save(&character(10, 3, Vec::new())).unwrap();
    assert!(repo.testing_checkpoint_step(10).unwrap());
    assert_eq!(staged_files(&dir), 0);
    assert_eq!(std::fs::metadata(testing_wal_path(&dir)).unwrap().len(), 20);
    drop(repo);
    let on_disk: PersistentCharacter = serde_json::from_slice(
        &std::fs::read(dir.join(crate::character_file_name(CharacterId::from_raw(10)))).unwrap(),
    )
    .unwrap();
    assert_eq!(on_disk.persistence_revision, 3);
    let repo = open_with_rules(&dir);
    for (id, revision) in [(10, 3), (11, 2), (14, 2), (15, 1), (19, 1)] {
        assert_eq!(
            repo.load(CharacterId::from_raw(id))
                .unwrap()
                .unwrap()
                .persistence_revision,
            revision
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Cost probe for representative stored-character counts. Not a gate.
/// `PURGATORY_DURABLE_PROBE_COUNTS=100,1000,10000 cargo test -p
/// purgatory-persistence --release durable_cost_probe -- --ignored --nocapture`
#[test]
#[ignore = "timing probe; run explicitly in release"]
fn durable_cost_probe() {
    use std::time::Instant;
    let counts =
        std::env::var("PURGATORY_DURABLE_PROBE_COUNTS").unwrap_or_else(|_| "100,1000".into());
    for count in counts
        .split(',')
        .map(|raw| raw.trim().parse::<u64>().unwrap())
    {
        let dir = temp_dir();
        let repo = open_with_rules(&dir);
        let started = Instant::now();
        let mut next = 1u64;
        while next <= count {
            let end = (next + 511).min(count);
            let characters = (next..=end)
                .map(|id| character(id + 1_000, 1, Vec::new()))
                .collect();
            repo.commit_ownership(OwnershipChange {
                characters,
                drops_upsert: Vec::new(),
                drops_remove: Vec::new(),
            })
            .unwrap();
            next = end + 1;
        }
        let populate = started.elapsed();
        let rounds = 5u32;
        let started = Instant::now();
        for tick in 1..=u64::from(rounds) {
            repo.checkpoint_active_clock(tick, ClockCheckpointKind::Periodic)
                .unwrap();
        }
        let clock = started.elapsed() / rounds;
        let started = Instant::now();
        for revision in 2..=u64::from(rounds) + 1 {
            repo.save(&character(1_001, revision, Vec::new())).unwrap();
        }
        let save = started.elapsed() / rounds;
        drop(repo);
        let started = Instant::now();
        let repo = open_with_rules(&dir);
        let reopen = started.elapsed();
        let started = Instant::now();
        repo.compact_durable_log().unwrap();
        let compact = started.elapsed();
        drop(repo);
        let started = Instant::now();
        let repo = open_with_rules(&dir);
        let reopen_compacted = started.elapsed();
        let changed = count.min(200);
        for id in 1..=changed {
            repo.save(&character(id + 1_000, 10, Vec::new())).unwrap();
        }
        let started = Instant::now();
        repo.testing_checkpoint_step(crate::journal::CHECKPOINT_STAGE_BATCH)
            .unwrap();
        let step = started.elapsed();
        let started = Instant::now();
        repo.compact_durable_log().unwrap();
        let finish = started.elapsed();
        println!(
            "DURABLE_PROBE|characters={count}|populate_ms={:.1}|clock_commit_ms={:.2}|character_save_ms={:.2}|reopen_ms={:.1}|full_checkpoint_ms={:.1}|reopen_after_checkpoint_ms={:.1}|stage_step_ms={:.1}|finish_{changed}_changed_ms={:.1}",
            populate.as_secs_f64() * 1e3,
            clock.as_secs_f64() * 1e3,
            save.as_secs_f64() * 1e3,
            reopen.as_secs_f64() * 1e3,
            compact.as_secs_f64() * 1e3,
            reopen_compacted.as_secs_f64() * 1e3,
            step.as_secs_f64() * 1e3,
            finish.as_secs_f64() * 1e3,
        );
        drop(repo);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn directory_sync_evidence_is_not_a_power_loss_proof() {
    let capability = directory_sync_capability();
    #[cfg(windows)]
    assert_eq!(capability, DirectorySync::FileDataSyncedOnly);
    #[cfg(unix)]
    assert_eq!(capability, DirectorySync::DirectorySynced);
    let _ = capability;
}
