use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use purgatory_common::{CharacterId, ContentId, InstanceExitContext, ItemInstanceId};

use crate::character::PersistentCharacter;
use crate::domain::{
    CharacterItemLocation, ClockCheckpointKind, DurableContentRules, DurableEquipmentSlot,
    ItemContentRule, MapDropPosition, MapDropRecord, OwnershipChange, PersistentItem,
};
use crate::error::PersistError;
use crate::journal::testing_wal_path;
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
    repo.save(&character(
        10,
        2,
        vec![item(reserved.first.raw(), 30_011, 2, 0)],
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
fn log_replay_restores_a_checkpoint_that_missed_the_manifest() {
    let dir = temp_dir();
    let repo = open_with_rules(&dir);
    let reserved = repo.reserve_item_instance_ids(1).unwrap();
    repo.save(&character(
        10,
        1,
        vec![item(reserved.first.raw(), 30_011, 1, 0)],
    ))
    .unwrap();
    let manifest = dir.join("durable_manifest.json");
    std::fs::remove_file(&manifest).unwrap();
    drop(repo);
    let repo = open_with_rules(&dir);
    assert_eq!(owners(&repo), vec![(reserved.first.raw(), "character", 10)]);
    assert!(manifest.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn compacted_log_replays_from_the_checkpoint_and_rejects_a_bad_fingerprint() {
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
    let path = repo.path_for(CharacterId::from_raw(10));
    let original = std::fs::read(&path).unwrap();
    let manifest = dir.join("durable_manifest.json");
    let raw = String::from_utf8(std::fs::read(&manifest).unwrap()).unwrap();
    let crc_key = "\"checkpoint_crc\": ";
    let start = raw.find(crc_key).unwrap() + crc_key.len();
    let end = start + raw[start..].find([',', '\n', '}']).unwrap();
    let mut edited = raw.clone();
    edited.replace_range(start..end, "1");
    std::fs::write(&manifest, &edited).unwrap();
    drop(repo);
    assert!(FileCharacterRepository::open(&dir).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(std::fs::read(&manifest).unwrap(), edited.into_bytes());
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
    let manifest = std::fs::read(dir.join("durable_manifest.json")).unwrap();
    let wal = testing_wal_path(&dir);
    let original_perms = std::fs::metadata(&wal).unwrap().permissions();
    let mut readonly = original_perms.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&wal, readonly).unwrap();
    let failed = repo.checkpoint_active_clock(110, ClockCheckpointKind::Periodic);
    std::fs::set_permissions(&wal, original_perms).unwrap();
    assert!(matches!(failed, Err(PersistError::Io { .. })), "{failed:?}");
    assert_eq!(
        std::fs::read(dir.join("durable_manifest.json")).unwrap(),
        manifest
    );
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

#[test]
fn directory_sync_evidence_is_not_a_power_loss_proof() {
    let capability = directory_sync_capability();
    #[cfg(windows)]
    assert_eq!(capability, DirectorySync::FileDataSyncedOnly);
    #[cfg(unix)]
    assert_eq!(capability, DirectorySync::DirectorySynced);
    let _ = capability;
}
