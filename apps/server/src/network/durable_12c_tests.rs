    use purgatory_common::{CharacterId, DevLogin, ItemInstanceId};
    use purgatory_persistence::{
        ChannelClaim, CharacterItemLocation, DurableCommand, ItemContentRule,
        ItemOwner, PersistError, PersistenceService, PlaceNewItem, PostgresSettings,
        SessionAdmission, drop_test_schema,
    };
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn sword() -> ContentId {
        purgatory_common::ITEM_PRACTICE_SWORD
    }

    fn headwear() -> ContentId {
        ContentId::from_raw(30_001)
    }

    #[test]
    fn leased_drop_does_not_change_world_before_commit() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let item = owned_debug_sword(&mut owner, actor);
        let character = CharacterId::from_raw(4);
        owner.lease_for_test(id, character, 1, "dev.local", 1, &[item]);

        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 1,
                item_instance_id: item,
            },
        });
        assert!(rx.try_recv().is_err(), "no success before the commit settles");
        assert!(owner.world().inventory_contains(actor, item));
        assert!(owner.world().world_drop_entity_for_item(item).is_none());
        let staged = owner.take_durable_commits();
        assert_eq!(staged.len(), 1);
        let retry = owner.durable_retry(staged[0].token).expect("pending command");
        assert_eq!(retry.command.key, staged[0].command.key);
        owner.settle_durable(
            staged[0].token,
            Ok(purgatory_persistence::DurableCommandResult {
                revisions: vec![(character, 2)],
                minted_item_ids: Vec::new(),
            }),
        );
        assert_eq!(recv_item(&mut rx), ServerItem::DropAccepted { seq: 1 });
        assert!(!owner.world().inventory_contains(actor, item));
        assert!(owner.world().world_drop_entity_for_item(item).is_some());
    }

    #[test]
    fn leased_full_inventory_and_wrong_slot_do_not_stage() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let potion_item = owned_item(&mut owner, actor, headwear());
        let mut items = vec![potion_item];
        while owner.world().inventory_count(actor) < purgatory_simulation::INVENTORY_CAPACITY {
            items.push(owned_debug_sword(&mut owner, actor));
        }
        let position = owner.world().transform_of(actor).unwrap().position;
        let address = owner.world().address_of(actor).unwrap();
        let (ground, entity) = owner
            .world_mut()
            .spawn_world_drop_item(address, position, sword(), 1, 1)
            .unwrap();
        let character = CharacterId::from_raw(5);
        owner.lease_for_test(id, character, 1, "dev.local", 1, &items);
        owner.durable_items.insert(ground);
        owner.apply_input(InputUpdate::Pickup {
            connection_id: id,
            request: PickupRequest {
                seq: 1,
                target: wire_id(entity),
            },
        });
        assert_eq!(
            recv_item(&mut rx),
            ServerItem::PickupRejected {
                seq: 1,
                reason: PickupRejectReason::InventoryFull,
            }
        );
        assert!(owner.take_durable_commits().is_empty());
        assert!(owner.world().world_drop_entity_for_item(ground).is_some());

        owner.apply_input(InputUpdate::Equip {
            connection_id: id,
            request: EquipRequest {
                seq: 1,
                slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
                item_instance_id: potion_item,
            },
        });
        assert_eq!(
            recv_equipment(&mut rx),
            ServerEquipment::Rejected {
                seq: 1,
                reason: EquipmentRejectReason::SlotMismatch,
            }
        );
        assert!(owner.take_durable_commits().is_empty());
        assert!(owner.world().inventory_contains(actor, potion_item));
    }

    #[test]
    fn pending_durable_detach_waits_for_the_outcome() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        let item = owned_debug_sword(&mut owner, actor);
        let character = CharacterId::from_raw(6);
        owner.lease_for_test(id, character, 1, "dev.local", 1, &[item]);
        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 1,
                item_instance_id: item,
            },
        });
        owner.detach(id);
        assert_eq!(owner.entity_of(id), Some(actor));
        let staged = owner.take_durable_commits();
        owner.settle_durable(
            staged[0].token,
            Ok(purgatory_persistence::DurableCommandResult {
                revisions: vec![(character, 2)],
                minted_item_ids: Vec::new(),
            }),
        );
        assert_eq!(owner.entity_of(id), None);
    }

    fn test_settings() -> PostgresSettings {
        let url = std::env::var("PURGATORY_TEST_DATABASE_URL").unwrap_or_default();
        if url.trim().is_empty() {
            panic!("PURGATORY_TEST_DATABASE_URL is unset, so this PostgreSQL test was not executed");
        }
        static SCHEMA_SEQ: AtomicU64 = AtomicU64::new(0);
        let schema = format!(
            "p12a_{}_{}",
            std::process::id(),
            SCHEMA_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        PostgresSettings::for_tests(url, schema).expect("dedicated test database")
    }

    fn unique_dir() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "purgatory-12c-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct Pg {
        service: PersistenceService,
        owner: GameplayOwner,
        login: DevLogin,
        next_connection: u64,
        settings: PostgresSettings,
        dir: PathBuf,
        generation: u64,
    }

    fn with_db(test: impl FnOnce(&mut Pg)) {
        let settings = test_settings();
        let dir = unique_dir();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut service = PersistenceService::open_postgresql(&dir, &settings).unwrap();
            let owner = GameplayOwner::new();
            service.set_durable_content_rules(owner.durable_content_rules());
            let claim = service.claim_channel(1, None).unwrap();
            let ChannelClaim::Claimed { generation, .. } = claim else {
                panic!("expected a claimed channel");
            };
            let mut pg = Pg {
                service,
                owner,
                login: DevLogin::parse("dev.local").unwrap(),
                next_connection: 1,
                settings: settings.clone(),
                dir: dir.clone(),
                generation,
            };
            test(&mut pg);
        }));
        let dropped = drop_test_schema(&settings);
        let _ = std::fs::remove_dir_all(&dir);
        if let Err(err) = dropped {
            eprintln!("PURGATORY postgres test schema cleanup failed: {err}");
        }
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    }

    struct Entered {
        connection: ConnectionId,
        character: CharacterId,
        lease: purgatory_persistence::LeaseAuthority,
        actor: purgatory_simulation::EntityId,
    }

    impl Pg {
        fn enter(&mut self, name: &str) -> Entered {
            let login = self.login.as_str().to_string();
            self.enter_as(&login, name)
        }

        fn enter_as(&mut self, login: &str, name: &str) -> Entered {
            let login = DevLogin::parse(login).unwrap();
            let created = self.service.create_character(&login, name).unwrap();
            let admission = self.service.admit(&login, created.character_id).unwrap();
            let SessionAdmission::Granted {
                authority: Some(lease),
                restore,
            } = admission
            else {
                panic!("expected a granted lease");
            };
            let connection = ConnectionId::from_raw(self.next_connection);
            self.next_connection += 1;
            let (tx, _rx) = tokio::sync::mpsc::channel(8);
            self.owner
                .enter_restored(
                    connection,
                    *restore,
                    Some(lease.clone()),
                    None,
                    Some(tx),
                    None,
                )
                .unwrap();
            let actor = self.owner.entity_of(connection).unwrap();
            Entered {
                connection,
                character: created.character_id,
                lease,
                actor,
            }
        }

        fn seed_item(&mut self, entered: &Entered, definition: ContentId, slot: u16) -> ItemInstanceId {
            let revision = self
                .owner
                .bindings
                .get(&entered.connection)
                .unwrap()
                .committed_revision;
            let command = DurableCommand {
                key: format!("seed-{}-{slot}", entered.character.raw()),
                expected_revisions: vec![(entered.character, revision)],
                place_new: vec![PlaceNewItem {
                    owner: entered.character,
                    definition_content_id: definition,
                    quantity: 1,
                    location: CharacterItemLocation::Inventory { slot },
                }],
                moves: Vec::new(),
                retire: Vec::new(),
                narrative: Vec::new(),
                learned: Vec::new(),
            };
            let result = self
                .service
                .commit_durable_leased(&command, Some(&entered.lease))
                .unwrap();
            let minted = result.minted_item_ids[0];
            let next = result
                .revisions
                .iter()
                .find(|(id, _)| *id == entered.character)
                .unwrap()
                .1;
            let stack_limit = self
                .owner
                .registry
                .item_by_id(definition)
                .unwrap()
                .stack_limit;
            self.owner
                .world_mut()
                .restore_inventory_item(
                    entered.actor,
                    minted,
                    definition,
                    1,
                    stack_limit,
                    slot,
                )
                .unwrap();
            self.owner.lease_for_test(
                entered.connection,
                entered.character,
                next,
                entered.lease.login.as_str(),
                entered.lease.generation,
                &[minted],
            );
            minted
        }

        fn settle_next(&mut self) -> purgatory_persistence::DurableCommandResult {
            let staged = self.owner.take_durable_commits();
            assert_eq!(staged.len(), 1, "expected one staged command");
            let result = self
                .service
                .commit_durable_leased(&staged[0].command, staged[0].lease.as_ref());
            let ok = result.expect("commit");
            self.owner
                .settle_durable(staged[0].token, Ok(ok.clone()));
            ok
        }
    }

    #[test]
    #[ignore]
    fn postgres_12c_drop_pickup_reclaim_and_competition() {
        with_db(|pg| {
            let a = pg.enter("Mira");
            let item = pg.seed_item(&a, sword(), 0);
            pg.owner.apply_input(InputUpdate::Drop {
                connection_id: a.connection,
                request: DropRequest {
                    seq: 1,
                    item_instance_id: item,
                },
            });
            assert!(pg.owner.world().inventory_contains(a.actor, item));
            pg.settle_next();
            assert_eq!(
                pg.service.read_item(item).unwrap().unwrap().owner,
                ItemOwner::Ground
            );
            assert!(pg.owner.world().world_drop_entity_for_item(item).is_some());

            let b = pg.enter_as("dev.other", "Nia");
            move_player_to_entity(&mut pg.owner, b.connection, a.actor);
            let drop = pg.owner.world().world_drop_entity_for_item(item).unwrap();
            pg.owner.apply_input(InputUpdate::Pickup {
                connection_id: a.connection,
                request: PickupRequest {
                    seq: 1,
                    target: wire_id(drop),
                },
            });
            pg.owner.apply_input(InputUpdate::Pickup {
                connection_id: b.connection,
                request: PickupRequest {
                    seq: 1,
                    target: wire_id(drop),
                },
            });
            let staged = pg.owner.take_durable_commits();
            assert_eq!(staged.len(), 1, "the reserved drop accepts one pickup");
            let result = pg
                .service
                .commit_durable_leased(&staged[0].command, staged[0].lease.as_ref())
                .unwrap();
            pg.owner.settle_durable(staged[0].token, Ok(result));
            assert!(pg.owner.world().inventory_contains(a.actor, item));
            assert!(!pg.owner.world().inventory_contains(b.actor, item));
            match pg.service.read_item(item).unwrap().unwrap().owner {
                ItemOwner::Character { character_id, .. } => assert_eq!(character_id, a.character),
                other => panic!("expected A to own the reclaimed item, got {other:?}"),
            }
        });
    }

    fn move_player_to_entity(
        owner: &mut GameplayOwner,
        connection: ConnectionId,
        target: purgatory_simulation::EntityId,
    ) {
        let actor = owner.entity_of(connection).unwrap();
        let position = owner.world().transform_of(target).unwrap().position;
        let address = owner.world().address_of(target).unwrap();
        let _ = owner.world_mut().set_address(actor, address);
        let _ = owner
            .world_mut()
            .set_transform(actor, purgatory_simulation::Transform::from_position(position));
    }

    #[test]
    #[ignore]
    fn postgres_12c_pickup_expiry_crash_and_lost_reply() {
        with_db(|pg| {
            let a = pg.enter("Mira");
            let item = pg.seed_item(&a, sword(), 0);
            pg.owner.apply_input(InputUpdate::Drop {
                connection_id: a.connection,
                request: DropRequest {
                    seq: 1,
                    item_instance_id: item,
                },
            });
            let staged = pg.owner.take_durable_commits();
            let first = pg
                .service
                .commit_durable_leased(&staged[0].command, staged[0].lease.as_ref())
                .unwrap();
            let second = pg
                .service
                .commit_durable_leased(&staged[0].command, staged[0].lease.as_ref())
                .unwrap();
            assert_eq!(first, second, "a lost reply retries the same command key");
            pg.owner.settle_durable(staged[0].token, Ok(second));
            assert!(pg.owner.world().world_drop_entity_for_item(item).is_some());
            assert_eq!(
                pg.owner
                    .world()
                    .inventory_snapshot(a.actor)
                    .into_iter()
                    .filter(|(_, id, _)| *id == item)
                    .count(),
                0
            );

            let b = pg.enter_as("dev.other", "Nia");
            move_player_to_entity(&mut pg.owner, b.connection, a.actor);
            let drop = pg.owner.world().world_drop_entity_for_item(item).unwrap();
            pg.owner.apply_input(InputUpdate::Pickup {
                connection_id: b.connection,
                request: PickupRequest {
                    seq: 1,
                    target: wire_id(drop),
                },
            });
            assert!(
                !pg.owner.stage_ground_expiry(a.connection, item),
                "expiry loses to a reserved pickup"
            );
            pg.settle_next();
            assert!(pg.owner.world().inventory_contains(b.actor, item));
            assert!(!pg.owner.world().inventory_contains(a.actor, item));

            let crashed = item;
            pg.service.release_lease(&a.lease).unwrap();
            pg.service.release_lease(&b.lease).unwrap();
            pg.service.release_channel(1, pg.generation).unwrap();
            let mut restarted =
                PersistenceService::open_postgresql(&pg.dir, &pg.settings).unwrap();
            let claim = restarted.claim_channel(1, None).unwrap();
            assert!(matches!(claim, ChannelClaim::Claimed { .. }));
            match restarted.read_item(crashed).unwrap().unwrap().owner {
                ItemOwner::Character { character_id, .. } => {
                    assert_eq!(character_id, b.character)
                }
                other => panic!("pickup commit must survive restart, got {other:?}"),
            }
            let SessionAdmission::Granted { restore, .. } =
                restarted.admit(&pg.login, a.character).unwrap()
            else {
                panic!("A should be readable after the crash");
            };
            assert!(
                restore
                    .items
                    .iter()
                    .all(|row| row.item_instance_id != crashed),
                "a committed pickup must not remain with A"
            );
            let other = DevLogin::parse("dev.other").unwrap();
            let SessionAdmission::Granted { restore, .. } =
                restarted.admit(&other, b.character).unwrap()
            else {
                panic!("B should be readable after the crash");
            };
            assert_eq!(
                restore
                    .items
                    .iter()
                    .filter(|row| row.item_instance_id == crashed)
                    .count(),
                1
            );
        });
    }

    #[test]
    #[ignore]
    fn postgres_12c_crash_after_drop_retires_ground_without_refund() {
        with_db(|pg| {
            let a = pg.enter("Mira");
            let item = pg.seed_item(&a, sword(), 0);
            pg.owner.apply_input(InputUpdate::Drop {
                connection_id: a.connection,
                request: DropRequest {
                    seq: 1,
                    item_instance_id: item,
                },
            });
            let staged = pg.owner.take_durable_commits();
            pg.service
                .commit_durable_leased(&staged[0].command, staged[0].lease.as_ref())
                .unwrap();
            pg.service.release_lease(&a.lease).unwrap();
            pg.service.release_channel(1, pg.generation).unwrap();
            let mut restarted =
                PersistenceService::open_postgresql(&pg.dir, &pg.settings).unwrap();
            let claim = restarted.claim_channel(1, None).unwrap();
            assert!(matches!(claim, ChannelClaim::Claimed { retired_ground, .. } if retired_ground >= 1));
            assert_eq!(
                restarted.read_item(item).unwrap().unwrap().owner,
                ItemOwner::Retired
            );
            let SessionAdmission::Granted { restore, .. } =
                restarted.admit(&pg.login, a.character).unwrap()
            else {
                panic!("expected A after the ground sweep");
            };
            assert!(restore.items.iter().all(|row| row.item_instance_id != item));
        });
    }

    #[test]
    #[ignore]
    fn postgres_12c_rejection_stale_lease_and_isolated_characters() {
        with_db(|pg| {
            let a = pg.enter("Mira");
            let item = pg.seed_item(&a, sword(), 0);
            pg.owner.apply_input(InputUpdate::Drop {
                connection_id: a.connection,
                request: DropRequest {
                    seq: 1,
                    item_instance_id: item,
                },
            });
            let staged = pg.owner.take_durable_commits();
            let mut stale = a.lease.clone();
            stale.generation = stale.generation.saturating_add(1);
            let err = pg
                .service
                .commit_durable_leased(&staged[0].command, Some(&stale))
                .unwrap_err();
            assert!(matches!(err, PersistError::LeaseLost));
            pg.owner.settle_durable(staged[0].token, Err(err));
            assert!(pg.owner.world().inventory_contains(a.actor, item));
            assert!(matches!(
                pg.service.read_item(item).unwrap().unwrap().owner,
                ItemOwner::Character { .. }
            ));

            let mut rules = pg.owner.durable_content_rules();
            rules
                .insert_item(ItemContentRule {
                    content_id: sword(),
                    stack_limit: 1,
                    equip_slot: Some(purgatory_persistence::DurableEquipmentSlot::Weapon),
                    retired: true,
                })
                .unwrap();
            pg.service.set_durable_content_rules(rules);
            pg.owner.apply_input(InputUpdate::Equip {
                connection_id: a.connection,
                request: EquipRequest {
                    seq: 1,
                    slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
                    item_instance_id: item,
                },
            });
            let staged = pg.owner.take_durable_commits();
            assert_eq!(staged.len(), 1);
            let err = pg
                .service
                .commit_durable_leased(&staged[0].command, staged[0].lease.as_ref())
                .unwrap_err();
            assert!(matches!(err, PersistError::ContentRejected { .. }));
            pg.owner.settle_durable(staged[0].token, Err(err));
            assert!(pg.owner.world().inventory_contains(a.actor, item));
            assert!(pg
                .owner
                .world()
                .equipment_slot(a.actor, purgatory_simulation::EquipmentSlot::Weapon)
                .is_none());

            pg.service.release_lease(&a.lease).unwrap();
            let sibling = pg.service.create_character(&pg.login, "Nia").unwrap();
            let SessionAdmission::Granted { restore, .. } =
                pg.service.admit(&pg.login, sibling.character_id).unwrap()
            else {
                panic!("the second character should admit after A releases");
            };
            assert!(restore.items.iter().all(|row| row.item_instance_id != item));
            let sibling_lease = purgatory_persistence::LeaseAuthority {
                login: pg.login.clone(),
                character_id: sibling.character_id,
                generation: 2,
            };
            pg.service.release_lease(&sibling_lease).unwrap();
            let SessionAdmission::Granted { restore, .. } =
                pg.service.admit(&pg.login, a.character).unwrap()
            else {
                panic!("A should still be isolated from Nia");
            };
            assert_eq!(
                restore
                    .items
                    .iter()
                    .filter(|row| row.item_instance_id == item)
                    .count(),
                1
            );
        });
    }

    #[test]
    #[ignore]
    fn postgres_12c_dialogue_choice_commits_item_fact_and_ability_once() {
        with_db(|pg| {
            let a = pg.enter("Mira");
            move_player_to_content(&mut pg.owner, a.connection, "npc.welcome.traveler_stayed");
            let traveler = find_content(&pg.owner, "npc.welcome.traveler_stayed");
            pg.owner.apply_input(InputUpdate::InteractOpen {
                connection_id: a.connection,
                target: wire_id(traveler),
            });
            let actor = a.actor;
            let active = pg.owner.dialogues.active(actor).expect("dialogue opened");
            let beat_id = pg
                .owner
                .registry
                .npc_dialogue_by_id(active.npc_content_id)
                .and_then(|dialogue| dialogue.beat(active.beat_index))
                .expect("authored beat")
                .id
                .clone();
            let plan = super::super::dialogue::ChoicePlan {
                accepted: active,
                choice_index: 0,
                next: None,
                actions: vec![
                    purgatory_content::DialogueAction::GiveItem {
                        item_authored: "item.package".into(),
                        quantity: 1,
                    },
                    purgatory_content::DialogueAction::SetFact {
                        fact: "welcome.workshop.package_at_inn".into(),
                        value: true,
                    },
                    purgatory_content::DialogueAction::GrantAbility {
                        ability_authored: "skill.movement.dash".into(),
                    },
                ],
            };
            pg.owner
                .stage_dialogue(a.connection, plan.clone(), Some(beat_id.clone()));
            pg.owner
                .stage_dialogue(a.connection, plan, Some(beat_id.clone()));
            let staged = pg.owner.take_durable_commits();
            assert_eq!(staged.len(), 1, "a repeated choice does not enqueue twice");
            let first = pg
                .service
                .commit_durable_leased(&staged[0].command, staged[0].lease.as_ref())
                .unwrap();
            let second = pg
                .service
                .commit_durable_leased(&staged[0].command, staged[0].lease.as_ref())
                .unwrap();
            assert_eq!(first, second);
            pg.owner.settle_durable(staged[0].token, Ok(second));
            assert_eq!(pg.owner.world().inventory_count(actor), 1);
            assert!(pg.owner.narrative.fact(actor, "welcome.workshop.package_at_inn"));
            let ability = pg
                .owner
                .registry
                .ability("skill.movement.dash")
                .unwrap()
                .id;
            assert!(pg.owner.world().ability_granted(actor, ability));
            pg.service.release_lease(&a.lease).unwrap();
            let SessionAdmission::Granted { restore, .. } =
                pg.service.admit(&pg.login, a.character).unwrap()
            else {
                panic!("restore after the dialogue commit");
            };
            assert_eq!(restore.items.len(), 1);
            assert_eq!(
                restore.narrative.facts.get("welcome.workshop.package_at_inn"),
                Some(&true)
            );
            assert!(restore.narrative.learned_abilities.contains(&ability.raw().unwrap()));
            assert!(restore.narrative.dialogue_heard.iter().any(|(_, beat)| beat == &beat_id));
        });
    }

    #[test]
    #[ignore]
    fn postgres_12c_expiry_retires_a_live_drop() {
        with_db(|pg| {
            let a = pg.enter("Mira");
            let item = pg.seed_item(&a, sword(), 0);
            pg.owner.apply_input(InputUpdate::Drop {
                connection_id: a.connection,
                request: DropRequest {
                    seq: 1,
                    item_instance_id: item,
                },
            });
            pg.settle_next();
            assert!(pg.owner.stage_ground_expiry(a.connection, item));
            let drop = pg.owner.world().world_drop_entity_for_item(item).unwrap();
            pg.owner.apply_input(InputUpdate::Pickup {
                connection_id: a.connection,
                request: PickupRequest {
                    seq: 1,
                    target: wire_id(drop),
                },
            });
            pg.settle_next();
            assert!(pg.owner.world().world_drop_entity_for_item(item).is_none());
            assert!(!pg.owner.world().inventory_contains(a.actor, item));
            assert_eq!(
                pg.service.read_item(item).unwrap().unwrap().owner,
                ItemOwner::Retired
            );
        });
    }

    #[test]
    #[ignore]
    fn postgres_12c_equip_and_unequip_round_trip() {
        with_db(|pg| {
            let a = pg.enter("Mira");
            let item = pg.seed_item(&a, sword(), 0);
            pg.owner.apply_input(InputUpdate::Equip {
                connection_id: a.connection,
                request: EquipRequest {
                    seq: 1,
                    slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
                    item_instance_id: item,
                },
            });
            pg.settle_next();
            assert_eq!(
                pg.owner
                    .world()
                    .equipment_slot(a.actor, purgatory_simulation::EquipmentSlot::Weapon),
                Some(sword())
            );
            assert!(matches!(
                pg.service.read_item(item).unwrap().unwrap().owner,
                ItemOwner::Character {
                    location: CharacterItemLocation::Equipped { .. },
                    ..
                }
            ));
            pg.owner.apply_input(InputUpdate::Unequip {
                connection_id: a.connection,
                request: UnequipRequest {
                    seq: 2,
                    slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
                },
            });
            pg.settle_next();
            assert!(pg.owner.world().inventory_contains(a.actor, item));
            assert!(pg
                .owner
                .world()
                .equipment_slot(a.actor, purgatory_simulation::EquipmentSlot::Weapon)
                .is_none());
        });
    }
