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
    fn postgres_12c_reconciled_dialogue_resend_does_not_mint_again() {
        with_db(|pg| {
            let a = pg.enter("Mira");
            let (tx, mut rx) = tokio::sync::mpsc::channel(16);
            pg.owner.bindings.get_mut(&a.connection).unwrap().interact = Some(tx);
            move_player_to_content(&mut pg.owner, a.connection, "npc.welcome.traveler_stayed");
            let traveler = find_content(&pg.owner, "npc.welcome.traveler_stayed");
            pg.owner.apply_input(InputUpdate::InteractOpen {
                connection_id: a.connection,
                target: wire_id(traveler),
            });
            while rx.try_recv().is_ok() {}
            let active = pg.owner.dialogues.active(a.actor).expect("dialogue opened");
            let beat_id = pg
                .owner
                .registry
                .npc_dialogue_by_id(active.npc_content_id)
                .and_then(|dialogue| dialogue.beat(active.beat_index))
                .expect("authored beat")
                .id
                .clone();
            let plan = reward_choice(active);
            pg.owner
                .stage_dialogue(a.connection, plan.clone(), Some(beat_id.clone()));
            let staged = pg.owner.take_durable_commits();
            assert_eq!(staged.len(), 1);
            assert!(matches!(
                staged[0].command.place_new[0].location,
                CharacterItemLocation::Inventory { slot: 0 }
            ));
            let committed = pg
                .service
                .commit_durable_leased(&staged[0].command, staged[0].lease.as_ref())
                .unwrap();
            let minted = committed.minted_item_ids[0];
            let stack_limit = pg
                .owner
                .registry
                .item_by_id(headwear())
                .unwrap()
                .stack_limit;
            let (_, slot) = pg
                .owner
                .world_mut()
                .grant_inventory_item(a.actor, headwear(), 1, stack_limit)
                .unwrap();
            assert_eq!(slot, 0, "the reward slot must be occupied so World apply fails");
            pg.owner.settle_durable(staged[0].token, Ok(committed));
            assert!(
                !pg.owner.world().inventory_contains(a.actor, minted),
                "World apply was supposed to fail after the database commit"
            );
            let restore = pg.service.read_owned_restore(a.character).unwrap();
            let revision = restore.character.persistence_revision;
            assert_eq!(restore.items.len(), 1);
            assert_eq!(
                restore.narrative.facts.get("welcome.workshop.package_at_inn"),
                Some(&true)
            );
            assert!(
                restore
                    .narrative
                    .learned_abilities
                    .contains(&purgatory_common::ABILITY_MOVEMENT_DASH.raw().unwrap())
            );
            assert!(pg.owner.complete_reconcile(a.connection, revision, restore));
            let keys = resend_committed_dialogue_choice(
                &mut pg.owner,
                a.connection,
                a.actor,
                active.session_id,
                active.beat_index.raw(),
                plan,
                beat_id,
            );
            assert!(
                keys.is_empty(),
                "resending the committed choice minted a second reward under revision {revision}: {keys:?}"
            );
            assert!(
                dialogue_outcome_was_truthful(&mut rx, active.session_id),
                "reconcile restored the reward without telling the client the choice was resolved"
            );
            assert_reward_state(&pg.owner, a.actor, minted);
            let stored = pg.service.read_owned_restore(a.character).unwrap();
            assert_eq!(stored.items.len(), 1);
            assert_eq!(stored.items[0].item_instance_id, minted);
            assert_eq!(
                stored.narrative.facts.get("welcome.workshop.package_at_inn"),
                Some(&true)
            );
            assert!(
                stored
                    .narrative
                    .learned_abilities
                    .contains(&purgatory_common::ABILITY_MOVEMENT_DASH.raw().unwrap())
            );
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
    fn postgres_12c_restart_does_not_restore_ground_time() {
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
            pg.owner.advance_ground_clock(Duration::from_secs(100));
            pg.service.release_lease(&a.lease).unwrap();
            pg.service.release_channel(1, pg.generation).unwrap();
            let mut restarted =
                PersistenceService::open_postgresql(&pg.dir, &pg.settings).unwrap();
            let claim = restarted.claim_channel(1, None).unwrap();
            assert!(matches!(
                claim,
                ChannelClaim::Claimed {
                    retired_ground, ..
                } if retired_ground >= 1
            ));
            assert_eq!(
                restarted.read_item(item).unwrap().unwrap().owner,
                ItemOwner::Retired
            );
            let SessionAdmission::Granted { restore, .. } =
                restarted.admit(&pg.login, a.character).unwrap()
            else {
                panic!("expected the character after restart");
            };
            assert!(restore.items.iter().all(|row| row.item_instance_id != item));
            assert!(
                GameplayOwner::new()
                    .world()
                    .world_drop_entity_for_item(item)
                    .is_none()
            );
        });
    }

    #[test]
    #[ignore]
    fn postgres_12c_expiry_cannot_retire_a_character_item() {
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
            let drop = pg.owner.world().world_drop_entity_for_item(item).unwrap();
            pg.owner.apply_input(InputUpdate::Pickup {
                connection_id: a.connection,
                request: PickupRequest {
                    seq: 1,
                    target: wire_id(drop),
                },
            });
            pg.settle_next();
            let err = pg
                .service
                .commit_durable_leased(&super::super::durable_play::retire_command(item), None)
                .unwrap_err();
            assert!(matches!(err, PersistError::Conflict { .. }));
            assert!(pg.owner.world().inventory_contains(a.actor, item));
            assert!(matches!(
                pg.service.read_item(item).unwrap().unwrap().owner,
                ItemOwner::Character { .. }
            ));
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

    fn character_restore(
        character: CharacterId,
        items: Vec<purgatory_persistence::ItemRecord>,
    ) -> purgatory_persistence::OwnedRestore {
        purgatory_persistence::OwnedRestore {
            character: purgatory_persistence::PersistentCharacter::new_default(character),
            items,
            narrative: purgatory_persistence::CharacterNarrativeState::default(),
        }
    }

    fn owned_record(
        character: CharacterId,
        item: ItemInstanceId,
        definition: ContentId,
        location: CharacterItemLocation,
    ) -> purgatory_persistence::ItemRecord {
        purgatory_persistence::ItemRecord {
            item_instance_id: item,
            definition_content_id: definition,
            quantity: 1,
            owner: ItemOwner::Character {
                character_id: character,
                location,
            },
        }
    }

    #[test]
    fn reconnect_clears_world_items_before_the_same_ids_return() {
        let mut owner = GameplayOwner::new();
        let character = CharacterId::from_raw(41);
        let inventory_item = ItemInstanceId::from_raw(9_001);
        let equipped_item = ItemInstanceId::from_raw(9_002);
        let restore = character_restore(
            character,
            vec![
                owned_record(
                    character,
                    inventory_item,
                    headwear(),
                    CharacterItemLocation::Inventory { slot: 0 },
                ),
                owned_record(
                    character,
                    equipped_item,
                    sword(),
                    CharacterItemLocation::Equipped {
                        slot: purgatory_persistence::DurableEquipmentSlot::Weapon,
                    },
                ),
            ],
        );
        let lease = purgatory_persistence::LeaseAuthority {
            login: DevLogin::parse("dev.local").unwrap(),
            character_id: character,
            generation: 1,
        };
        owner
            .enter_restored(
                ConnectionId::from_raw(1),
                restore.clone(),
                Some(lease.clone()),
                None,
                None,
                None,
            )
            .unwrap();
        let first_actor = owner.entity_of(ConnectionId::from_raw(1)).unwrap();
        assert!(owner.world().inventory_contains(first_actor, inventory_item));
        assert_eq!(
            owner
                .world()
                .equipped_instance(first_actor, purgatory_simulation::EquipmentSlot::Weapon),
            Some(equipped_item)
        );
        let address = owner.world().address_of(first_actor).unwrap();
        let position = owner.world().transform_of(first_actor).unwrap().position;
        let (ground, _) = owner
            .world_mut()
            .spawn_world_drop_item(address, position, sword(), 1, 1)
            .unwrap();
        owner
            .prepare_logout(ConnectionId::from_raw(1))
            .expect("logout");
        assert!(
            owner.world().item_record(ground).is_some(),
            "a live ground item stays in World when its owner leaves"
        );
        assert!(
            owner.world().item_record(inventory_item).is_none(),
            "logout must drop the inventory record with the actor"
        );
        assert!(owner.world().item_record(equipped_item).is_none());

        let mut partial = restore.clone();
        partial.items[1].quantity = 0;
        let failed = owner.enter_restored(
            ConnectionId::from_raw(2),
            partial,
            Some(lease.clone()),
            None,
            None,
            None,
        );
        assert!(failed.is_err(), "a partial restore must fail closed");
        assert!(
            owner.world().item_record(inventory_item).is_none(),
            "a failed restore must not leave the first item in World"
        );
        assert!(owner.entity_of(ConnectionId::from_raw(2)).is_none());

        owner
            .enter_restored(
                ConnectionId::from_raw(3),
                restore,
                Some(lease),
                None,
                None,
                None,
            )
            .expect("the same item ids can enter after cleanup");
        let actor = owner.entity_of(ConnectionId::from_raw(3)).unwrap();
        assert!(owner.world().inventory_contains(actor, inventory_item));
        assert_eq!(
            owner
                .world()
                .equipped_instance(actor, purgatory_simulation::EquipmentSlot::Weapon),
            Some(equipped_item)
        );
        assert!(owner.world().ability_granted(
            actor,
            purgatory_common::ABILITY_PRACTICE_SWORD_STRIKE
        ));
    }

    #[test]
    fn confirmed_commit_does_not_release_gameplay_when_world_apply_fails() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let item = owned_debug_sword(&mut owner, actor);
        let character = CharacterId::from_raw(42);
        owner.lease_for_test(id, character, 1, "dev.local", 1, &[item]);
        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 1,
                item_instance_id: item,
            },
        });
        let staged = owner.take_durable_commits();
        assert!(owner.world_mut().retire_inventory_instance(actor, item));
        owner.settle_durable(
            staged[0].token,
            Ok(purgatory_persistence::DurableCommandResult {
                revisions: vec![(character, 2)],
                minted_item_ids: Vec::new(),
            }),
        );
        assert!(rx.try_recv().is_err(), "a failed apply must not report success");
        let other = owned_debug_sword(&mut owner, actor);
        owner.durable_items.insert(other);
        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 2,
                item_instance_id: other,
            },
        });
        assert!(
            owner.take_durable_commits().is_empty(),
            "an inconsistent character must not start another mutation"
        );
        assert!(owner.world().inventory_contains(actor, other));
        let committed = character_restore(character, Vec::new());
        assert!(
            owner.complete_reconcile(id, 2, committed),
            "reconcile applies the committed character"
        );
        let mut accepted = false;
        while let Ok(message) = rx.try_recv() {
            match message {
                ServerControl::Item(ServerItem::DropAccepted { seq: 1 }) => accepted = true,
                ServerControl::Item(ServerItem::DropRejected { .. }) => {
                    panic!("a committed drop was described as rolled back: {message:?}")
                }
                ServerControl::Inventory(_) | ServerControl::AbilityGrants(_) => {}
                other => panic!("unexpected reconcile reply: {other:?}"),
            }
        }
        assert!(
            accepted,
            "reconciliation must resolve the committed drop for the client"
        );
        assert!(
            owner.world().world_drop_entity_for_item(item).is_some(),
            "the committed drop is visible before DropAccepted"
        );
        assert!(owner.live_ground.contains_key(&item));
        assert!(
            owner.world().item_record(other).is_none(),
            "an uncommitted runtime item does not survive reconciliation"
        );
        let fresh = owned_debug_sword(&mut owner, actor);
        owner.durable_items.insert(fresh);
        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 2,
                item_instance_id: fresh,
            },
        });
        assert_eq!(owner.take_durable_commits().len(), 1);
    }

    #[test]
    fn two_unknown_commits_stay_on_the_same_key() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let item = owned_debug_sword(&mut owner, actor);
        let character = CharacterId::from_raw(43);
        owner.lease_for_test(id, character, 1, "dev.local", 1, &[item]);
        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 1,
                item_instance_id: item,
            },
        });
        let staged = owner.take_durable_commits();
        let key = staged[0].command.key.clone();
        let token = staged[0].token;
        owner.settle_durable(
            token,
            Err(PersistError::storage(
                "commit outcome unknown: reply was not observed",
            )),
        );
        owner.settle_durable(
            token,
            Err(PersistError::storage(
                "commit outcome unknown: reply was not observed",
            )),
        );
        let again = owner.take_durable_commits();
        assert_eq!(again.len(), 1, "the same command must be resolved again");
        assert_eq!(again[0].token, token);
        assert_eq!(again[0].command.key, key);
        assert!(rx.try_recv().is_err(), "unknown is not success or failure");
        assert!(owner.world().inventory_contains(actor, item));
        owner.settle_durable(
            token,
            Ok(purgatory_persistence::DurableCommandResult {
                revisions: vec![(character, 2)],
                minted_item_ids: Vec::new(),
            }),
        );
        assert_eq!(recv_item(&mut rx), ServerItem::DropAccepted { seq: 1 });
        assert!(owner.world().world_drop_entity_for_item(item).is_some());
        assert!(owner.take_durable_commits().is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn spawn_durable_commits_reports_first_unknown_before_a_blocked_retry() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(191);
        enter_leased(&mut owner, id, 191);
        let actor = owner.entity_of(id).unwrap();
        let item = owned_debug_sword(&mut owner, actor);
        owner
            .world_mut()
            .equip_item(actor, item, purgatory_simulation::EquipmentSlot::Weapon)
            .unwrap();
        let character = CharacterId::from_raw(191);
        owner.lease_for_test(id, character, 1, "dev.local", 1, &[item]);
        owner.apply_input(InputUpdate::Unequip {
            connection_id: id,
            request: UnequipRequest {
                seq: 1,
                slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
            },
        });
        let (persist, mut calls) =
            super::super::persist::PersistenceHandle::scripted_durable_for_test();
        let (tx, mut life_rx, _input_rx) = gameplay_channels(8, 8);
        super::super::spawn_durable_commits(&mut owner, &persist, &tx);
        let (first_command, first_reply) = calls.recv().await;
        let key = first_command.key.clone();

        let before = horizontal(&owner, id);
        assert_eq!(
            owner.apply_input(command_update(id, cmd(1, MoveAxis::Right, false, false))),
            SeqDecision::Accept,
            "an ordinary in-flight command must not stop control"
        );
        owner.simulate_tick(tick_dt());
        assert!(horizontal(&owner, id) > before);
        assert!(owner.world_mut().grant_ability(actor, dash_id()));
        let move_seq = owner.last_received(id).unwrap();
        assert_eq!(
            owner.apply_input(command_update(
                id,
                cmd(move_seq.saturating_add(1), MoveAxis::Right, false, false),
            )),
            SeqDecision::Accept
        );
        assert_eq!(
            owner.apply_input(InputUpdate::AbilityActivate {
                connection_id: id,
                request: AbilityActivateRequest {
                    seq: 1,
                    input_epoch: 0,
                    input_sequence: move_seq.saturating_add(1),
                    ability_id: dash_id(),
                    selected: None,
                },
            }),
            SeqDecision::Accept
        );
        owner.simulate_tick(tick_dt());
        assert!(owner.world().player_dash_of(actor).is_some());

        first_reply
            .send(Err(PersistError::storage(
                "commit outcome unknown: reply was not observed",
            )))
            .unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_millis(300), life_rx.recv())
            .await
            .expect("first unknown must reach GameplayOwner before the blocked retry")
            .expect("lifecycle event");
        let token = match event {
            LifecycleCmd::SettleDurable { token, result } => {
                assert!(result
                    .as_ref()
                    .err()
                    .is_some_and(super::super::durable_play::commit_outcome_unknown));
                owner.settle_durable(token, result);
                token
            }
            _ => panic!("unexpected lifecycle event"),
        };
        super::super::spawn_durable_commits(&mut owner, &persist, &tx);
        let (second_command, second_reply) = calls.recv().await;
        assert_eq!(second_command.key, key, "the retry must use the stored key");
        let held = horizontal(&owner, id);
        assert_eq!(
            owner.apply_input(command_update(
                id,
                cmd(move_seq.saturating_add(2), MoveAxis::Right, false, false),
            )),
            SeqDecision::Stale
        );
        assert_eq!(
            owner.apply_input(InputUpdate::AbilityActivate {
                connection_id: id,
                request: AbilityActivateRequest {
                    seq: 2,
                    input_epoch: 0,
                    input_sequence: move_seq.saturating_add(2),
                    ability_id: purgatory_common::ABILITY_PRACTICE_SWORD_STRIKE,
                    selected: None,
                },
            }),
            SeqDecision::Stale
        );
        owner.simulate_tick(tick_dt());
        assert_eq!(horizontal(&owner, id), held);
        assert!(owner.world().player_dash_of(actor).is_none());
        assert!(
            owner
                .world()
                .ability_granted(actor, purgatory_common::ABILITY_PRACTICE_SWORD_STRIKE),
            "the old World grant still exists while the retry is blocked"
        );
        second_reply
            .send(Ok(purgatory_persistence::DurableCommandResult {
                revisions: vec![(character, 2)],
                minted_item_ids: Vec::new(),
            }))
            .unwrap();
        match life_rx.recv().await.expect("stored result") {
            LifecycleCmd::SettleDurable { token: settled, result } => {
                assert_eq!(settled, token);
                owner.settle_durable(settled, result);
            }
            _ => panic!("unexpected lifecycle event"),
        }
        assert!(owner.world().inventory_contains(actor, item));
        assert!(
            owner
                .world()
                .equipped_instance(actor, purgatory_simulation::EquipmentSlot::Weapon)
                .is_none()
        );
        assert!(owner.take_durable_commits().is_empty());
        assert_eq!(
            owner.apply_input(command_update(
                id,
                cmd(move_seq.saturating_add(2), MoveAxis::Right, false, false),
            )),
            SeqDecision::Accept,
            "a definite stored result resumes control"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn spawn_durable_commits_stops_unlanded_strike_after_first_unknown() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(192);
        enter_leased(&mut owner, id, 192);
        let actor = owner.entity_of(id).unwrap();
        let item = owned_debug_sword(&mut owner, actor);
        owner.lease_for_test(id, CharacterId::from_raw(192), 1, "dev.local", 1, &[item]);
        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 1,
                item_instance_id: item,
            },
        });
        let dummy = arm_strike_for_next_tick(&mut owner, id);
        let (persist, mut calls) =
            super::super::persist::PersistenceHandle::scripted_durable_for_test();
        let (tx, mut life_rx, _input_rx) = gameplay_channels(8, 8);
        super::super::spawn_durable_commits(&mut owner, &persist, &tx);
        let (first, reply) = calls.recv().await;
        reply
            .send(Err(PersistError::storage(
                "commit outcome unknown: reply was not observed",
            )))
            .unwrap();
        match tokio::time::timeout(std::time::Duration::from_millis(300), life_rx.recv())
            .await
            .expect("first unknown before blocked retry")
            .expect("lifecycle event")
        {
            LifecycleCmd::SettleDurable { token, result } => owner.settle_durable(token, result),
            _ => panic!("unexpected lifecycle event"),
        }
        super::super::spawn_durable_commits(&mut owner, &persist, &tx);
        let (second, _blocked_reply) = calls.recv().await;
        assert_eq!(second.key, first.key);
        owner.simulate_tick(tick_dt());
        assert_eq!(
            owner.world().health_of(dummy).unwrap().current,
            10.0,
            "a strike due on the first uncertain tick must not land"
        );
    }

    fn settle_stored(owner: &mut GameplayOwner, token: u64, character: CharacterId) {
        owner.settle_durable(
            token,
            Ok(purgatory_persistence::DurableCommandResult {
                revisions: vec![(character, 2)],
                minted_item_ids: Vec::new(),
            }),
        );
    }

    #[test]
    fn player_drop_timer_starts_when_the_ground_item_appears() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(201);
        enter_leased(&mut owner, id, 201);
        let actor = owner.entity_of(id).unwrap();
        let item = owned_debug_sword(&mut owner, actor);
        let character = CharacterId::from_raw(201);
        owner.lease_for_test(id, character, 1, "dev.local", 1, &[item]);
        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 1,
                item_instance_id: item,
            },
        });
        let staged = owner.take_durable_commits();
        owner.advance_ground_clock(Duration::from_secs(200));
        assert!(
            owner.take_durable_commits().is_empty(),
            "the timer must not run before the committed drop is in World"
        );
        settle_stored(&mut owner, staged[0].token, character);
        assert!(owner.world().world_drop_entity_for_item(item).is_some());
        owner.advance_ground_clock(Duration::from_secs(200) - Duration::from_nanos(1));
        assert!(owner.take_durable_commits().is_empty());
        assert!(owner.world().world_drop_entity_for_item(item).is_some());
        owner.detach(id);
        assert!(owner.bindings.is_empty());
        owner.advance_ground_clock(Duration::from_nanos(1));
        let due = owner.take_durable_commits();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].command.key, format!("retire-{}", item.raw()));
        assert!(due[0].lease.is_none());
        settle_stored(&mut owner, due[0].token, character);
        assert!(owner.world().world_drop_entity_for_item(item).is_none());
    }

    #[test]
    fn player_drop_is_collectible_by_anyone_immediately() {
        let mut owner = GameplayOwner::new();
        let dropper = ConnectionId::from_raw(202);
        let other = ConnectionId::from_raw(203);
        enter_leased(&mut owner, dropper, 202);
        enter_leased(&mut owner, other, 203);
        let dropper_actor = owner.entity_of(dropper).unwrap();
        let first = owned_debug_sword(&mut owner, dropper_actor);
        let second = owned_debug_sword(&mut owner, dropper_actor);
        owner.lease_for_test(dropper, CharacterId::from_raw(202), 1, "dev.local", 1, &[first, second]);
        owner.lease_for_test(other, CharacterId::from_raw(203), 1, "dev.other", 1, &[]);
        for (seq, item) in [(1u32, first), (2, second)] {
            owner.apply_input(InputUpdate::Drop {
                connection_id: dropper,
                request: DropRequest {
                    seq,
                    item_instance_id: item,
                },
            });
            let staged = owner.take_durable_commits();
            settle_stored(&mut owner, staged[0].token, CharacterId::from_raw(202));
        }
        move_player_to_entity(&mut owner, other, dropper_actor);
        let other_target = owner.world().world_drop_entity_for_item(first).unwrap();
        owner.apply_input(InputUpdate::Pickup {
            connection_id: other,
            request: PickupRequest {
                seq: 1,
                target: wire_id(other_target),
            },
        });
        assert_eq!(owner.take_durable_commits().len(), 1, "another character");
        let own_target = owner.world().world_drop_entity_for_item(second).unwrap();
        owner.apply_input(InputUpdate::Pickup {
            connection_id: dropper,
            request: PickupRequest {
                seq: 1,
                target: wire_id(own_target),
            },
        });
        assert_eq!(owner.take_durable_commits().len(), 1, "the former owner");
    }

    #[test]
    fn monster_loot_opens_to_other_characters_at_40_seconds() {
        let mut owner = GameplayOwner::new();
        let killer = ConnectionId::from_raw(204);
        let other = ConnectionId::from_raw(205);
        enter_leased(&mut owner, killer, 204);
        enter_leased(&mut owner, other, 205);
        let killer_id = CharacterId::from_raw(204);
        let actor = owner.entity_of(killer).unwrap();
        owner.lease_for_test(killer, killer_id, 1, "dev.local", 1, &[]);
        owner.lease_for_test(other, CharacterId::from_raw(205), 1, "dev.other", 1, &[]);
        let position = owner.world().transform_of(actor).unwrap().position;
        let address = owner.world().address_of(actor).unwrap();
        move_player_to_entity(&mut owner, other, actor);
        let early = owner
            .manifest_monster_loot(killer_id, address, position, sword(), 1)
            .unwrap();
        owner.apply_input(InputUpdate::Pickup {
            connection_id: killer,
            request: PickupRequest {
                seq: 1,
                target: wire_id(owner.world().world_drop_entity_for_item(early).unwrap()),
            },
        });
        assert_eq!(owner.take_durable_commits().len(), 1, "the killer");
        let late = owner
            .manifest_monster_loot(killer_id, address, position, sword(), 1)
            .unwrap();
        let late_entity = owner.world().world_drop_entity_for_item(late).unwrap();
        owner.apply_input(InputUpdate::Pickup {
            connection_id: other,
            request: PickupRequest {
                seq: 1,
                target: wire_id(late_entity),
            },
        });
        assert!(matches!(
            owner.bindings.get(&other).unwrap().last_pickup_result,
            Some(ServerItem::PickupRejected {
                reason: PickupRejectReason::StateBlocked,
                ..
            })
        ));
        assert!(owner.take_durable_commits().is_empty());
        owner.advance_ground_clock(Duration::from_secs(40) - Duration::from_nanos(1));
        owner.apply_input(InputUpdate::Pickup {
            connection_id: other,
            request: PickupRequest {
                seq: 2,
                target: wire_id(late_entity),
            },
        });
        assert!(owner.take_durable_commits().is_empty());
        owner.advance_ground_clock(Duration::from_nanos(1));
        owner.apply_input(InputUpdate::Pickup {
            connection_id: other,
            request: PickupRequest {
                seq: 3,
                target: wire_id(late_entity),
            },
        });
        assert_eq!(owner.take_durable_commits().len(), 1, "public at 40 seconds");
    }

    #[test]
    fn ground_expiry_is_bounded_and_stops_when_channel_authority_is_stale() {
        let mut owner = GameplayOwner::new();
        let killer = CharacterId::from_raw(206);
        let address = purgatory_common::WorldAddress::DEV;
        let mut items = Vec::new();
        for _ in 0..20 {
            items.push(
                owner
                    .manifest_monster_loot(killer, address, [0.0, 1.0], sword(), 1)
                    .unwrap(),
            );
        }
        owner.advance_ground_clock(Duration::from_secs(200));
        let _ = owner.take_durable_commits();
        let left = items
            .iter()
            .filter(|item| owner.world().world_drop_entity_for_item(**item).is_some())
            .count();
        assert_eq!(left, 12, "one wake retires at most eight ground items");
        let _ = owner.take_durable_commits();
        let left = items
            .iter()
            .filter(|item| owner.world().world_drop_entity_for_item(**item).is_some())
            .count();
        assert_eq!(left, 4);

        let mut stalled = GameplayOwner::new();
        let item = stalled
            .manifest_monster_loot(killer, address, [0.0, 1.0], sword(), 1)
            .unwrap();
        stalled.advance_ground_clock(Duration::from_secs(200));
        stalled.set_channel_deadline(Some(
            super::super::lease_clock::LocalLeaseDeadline::from_request(
                tokio::time::Instant::now(),
                Duration::ZERO,
            ),
        ));
        let _ = stalled.take_durable_commits();
        assert!(stalled.world().world_drop_entity_for_item(item).is_some());
    }

    #[test]
    fn deferred_ground_wake_stays_bounded_until_pickups_resolve() {
        const PENDING: usize = 64;
        let mut owner = GameplayOwner::new();
        let killer = CharacterId::from_raw(208);
        let address = purgatory_common::WorldAddress::DEV;
        let mut items = Vec::new();
        for _ in 0..PENDING {
            items.push(
                owner
                    .manifest_monster_loot(killer, address, [0.0, 1.0], sword(), 1)
                    .unwrap(),
            );
        }
        for item in &items {
            owner.reserved_items.insert(*item);
        }
        owner.advance_ground_clock(Duration::from_secs(200));
        let due: Vec<_> = owner.ground_expiry.keys().copied().collect();
        assert_eq!(due.len(), PENDING);
        for (when, raw) in due {
            let item = purgatory_common::ItemInstanceId::from_raw(raw);
            owner.ground_expiry.remove(&(when, raw));
            owner.ground_deferred.push_back(item);
            owner.ground_deferred_member.insert(item);
        }
        assert_eq!(
            owner.ground_deferred.len(),
            PENDING,
            "setup holds every reserved due item on the deferred queue"
        );
        for _ in 0..3 {
            let _ = owner.take_durable_commits();
            assert!(
                owner.ground_wake_ops as usize <= GROUND_RETIRE_BATCH,
                "one wake inspected {} ground items",
                owner.ground_wake_ops
            );
            assert!(
                items.iter().all(|item| owner.world().world_drop_entity_for_item(*item).is_some()),
                "a pending pickup keeps its ground item"
            );
        }
        owner.reserved_items.clear();
        let mut remaining = PENDING;
        for _ in 0..PENDING {
            let _ = owner.take_durable_commits();
            assert!(
                owner.ground_wake_ops as usize <= GROUND_RETIRE_BATCH,
                "one wake inspected {} ground items after reservations cleared",
                owner.ground_wake_ops
            );
            let now = items
                .iter()
                .filter(|item| owner.world().world_drop_entity_for_item(**item).is_some())
                .count();
            assert!(now <= remaining);
            remaining = now;
            if remaining == 0 {
                break;
            }
        }
        assert_eq!(remaining, 0, "deferred items expire after their reservation ends");
    }

    #[test]
    fn removing_one_ground_item_does_not_scan_its_deadline_bucket() {
        let mut owner = GameplayOwner::new();
        let killer = CharacterId::from_raw(209);
        let address = purgatory_common::WorldAddress::DEV;
        let mut items = Vec::new();
        for _ in 0..64 {
            items.push(
                owner
                    .manifest_monster_loot(killer, address, [0.0, 1.0], sword(), 1)
                    .unwrap(),
            );
        }
        owner.ground_remove_ops = 0;
        owner.forget_ground_item(items[0]);
        assert!(
            owner.ground_remove_ops <= 1,
            "removing one item visited {} same-deadline entries",
            owner.ground_remove_ops
        );
        assert!(owner.world().world_drop_entity_for_item(items[1]).is_some());
        owner.advance_ground_clock(Duration::from_secs(200));
        let _ = owner.take_durable_commits();
        assert!(
            owner.world().world_drop_entity_for_item(items[0]).is_some(),
            "forgetting the timer leaves that drop unscheduled"
        );
        let left = items
            .iter()
            .filter(|item| owner.world().world_drop_entity_for_item(**item).is_some())
            .count();
        assert_eq!(left, 64 - GROUND_RETIRE_BATCH);
    }

    #[test]
    fn dev_spawned_world_item_expires_with_ordinary_ground() {
        let mut owner = GameplayOwner::new();
        let connection = ConnectionId::from_raw(1);
        owner.attach(connection);
        let before: Vec<_> = owner.world().iter().collect();
        owner
            .handle_dev_spawn_item(connection, purgatory_common::ITEM_SMALL_POTION, 1)
            .expect("dev spawn");
        let item = owner
            .world()
            .iter()
            .find_map(|id| {
                if before.contains(&id) {
                    None
                } else {
                    owner.world().item_instance_at_world_drop(id)
                }
            })
            .expect("dev spawn creates a world drop");
        owner.advance_ground_clock(Duration::from_secs(200) - Duration::from_nanos(1));
        let _ = owner.take_durable_commits();
        assert!(owner.world().world_drop_entity_for_item(item).is_some());
        owner.advance_ground_clock(Duration::from_nanos(1));
        let staged = owner.take_durable_commits();
        assert!(
            staged.is_empty(),
            "a developer spawn is not a durable row"
        );
        assert!(
            owner.world().world_drop_entity_for_item(item).is_none(),
            "a developer-spawned world item expires after 200 seconds"
        );
    }

    #[test]
    fn pending_pickup_blocks_expiry_and_a_committed_item_stays() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(207);
        enter_leased(&mut owner, id, 207);
        let actor = owner.entity_of(id).unwrap();
        let item = owned_debug_sword(&mut owner, actor);
        let character = CharacterId::from_raw(207);
        owner.lease_for_test(id, character, 1, "dev.local", 1, &[item]);
        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 1,
                item_instance_id: item,
            },
        });
        let staged = owner.take_durable_commits();
        settle_stored(&mut owner, staged[0].token, character);
        owner.advance_ground_clock(Duration::from_secs(200) - Duration::from_nanos(1));
        let drop = owner.world().world_drop_entity_for_item(item).unwrap();
        owner.apply_input(InputUpdate::Pickup {
            connection_id: id,
            request: PickupRequest {
                seq: 1,
                target: wire_id(drop),
            },
        });
        let pickup = owner.take_durable_commits();
        assert_eq!(pickup.len(), 1);
        owner.advance_ground_clock(Duration::from_nanos(1));
        assert!(
            owner.take_durable_commits().is_empty(),
            "expiry must wait while the pickup result is unknown"
        );
        settle_stored(&mut owner, pickup[0].token, character);
        assert!(owner.world().inventory_contains(actor, item));
        owner.advance_ground_clock(Duration::from_secs(200));
        assert!(owner.take_durable_commits().is_empty());
        assert!(owner.world().inventory_contains(actor, item));
    }

    #[test]
    fn file_mode_ground_expires_with_nobody_connected() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(208);
        owner.attach(id);
        let actor = owner.entity_of(id).unwrap();
        let item = owned_debug_sword(&mut owner, actor);
        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 1,
                item_instance_id: item,
            },
        });
        assert!(owner.world().world_drop_entity_for_item(item).is_some());
        owner.detach(id);
        owner.advance_ground_clock(Duration::from_secs(200) - Duration::from_nanos(1));
        let _ = owner.take_durable_commits();
        assert!(owner.world().world_drop_entity_for_item(item).is_some());
        owner.advance_ground_clock(Duration::from_nanos(1));
        assert!(owner.take_durable_commits().is_empty());
        assert!(owner.world().world_drop_entity_for_item(item).is_none());
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn unknown_commit_blocks_control_until_the_stored_key_is_applied() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(91);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        enter_leased(&mut owner, id, 91);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let item = owned_debug_sword(&mut owner, actor);
        owner
            .world_mut()
            .equip_item(actor, item, purgatory_simulation::EquipmentSlot::Weapon)
            .unwrap();
        let character = CharacterId::from_raw(91);
        owner.lease_for_test(id, character, 1, "dev.local", 1, &[item]);
        assert!(
            owner
                .world()
                .ability_granted(actor, purgatory_common::ABILITY_PRACTICE_SWORD_STRIKE),
            "the equipped sword grants its strike before the unequip"
        );
        owner.apply_input(InputUpdate::Unequip {
            connection_id: id,
            request: UnequipRequest {
                seq: 1,
                slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
            },
        });
        let staged = owner.take_durable_commits();
        let key = staged[0].command.key.clone();
        let token = staged[0].token;
        let before = horizontal(&owner, id);
        assert_eq!(
            owner.apply_input(command_update(
                id,
                cmd(1, MoveAxis::Right, false, false),
            )),
            SeqDecision::Accept,
            "a command that is still waiting for its reply must not pause movement"
        );
        owner.simulate_tick(tick_dt());
        assert!(
            horizontal(&owner, id) > before,
            "pending unequip paused ordinary movement"
        );
        assert!(owner.world_mut().grant_ability(actor, dash_id()));
        let move_seq = owner.last_received(id).unwrap();
        assert_eq!(
            owner.apply_input(command_update(
                id,
                cmd(move_seq.saturating_add(1), MoveAxis::Right, false, false),
            )),
            SeqDecision::Accept
        );
        assert_eq!(
            owner.apply_input(InputUpdate::AbilityActivate {
                connection_id: id,
                request: AbilityActivateRequest {
                    seq: 1,
                    input_epoch: 0,
                    input_sequence: move_seq.saturating_add(1),
                    ability_id: dash_id(),
                    selected: None,
                },
            }),
            SeqDecision::Accept,
            "Dash was rejected while the unequip reply was still in flight"
        );
        owner.simulate_tick(tick_dt());
        assert!(owner.world().player_dash_of(actor).is_some());
        let held = horizontal(&owner, id);

        let unknown = || {
            Err(PersistError::storage(
                "commit outcome unknown: the database connection is closed",
            ))
        };
        owner.settle_durable(token, unknown());
        owner.settle_durable(token, unknown());
        let again = owner.take_durable_commits();
        assert_eq!(again.len(), 1, "a prolonged outage must retry the same command");
        assert_eq!(again[0].token, token);
        assert_eq!(again[0].command.key, key);
        assert_eq!(
            owner.apply_input(command_update(
                id,
                cmd(move_seq.saturating_add(2), MoveAxis::Right, false, false),
            )),
            SeqDecision::Stale,
            "movement used World state while the unequip commit was unknown"
        );
        assert_eq!(
            owner.apply_input(InputUpdate::AbilityActivate {
                connection_id: id,
                request: AbilityActivateRequest {
                    seq: 2,
                    input_epoch: 0,
                    input_sequence: move_seq.saturating_add(2),
                    ability_id: purgatory_common::ABILITY_PRACTICE_SWORD_STRIKE,
                    selected: None,
                },
            }),
            SeqDecision::Stale,
            "the equipped sword's strike was used after its unequip may have committed"
        );
        owner.settle_durable(token, unknown());
        owner.simulate_tick(tick_dt());
        assert_eq!(
            horizontal(&owner, id),
            held,
            "held movement or Dash continued while the commit reply was unknown"
        );
        assert!(owner.world().player_dash_of(actor).is_none());
        assert!(
            owner
                .world()
                .ability_granted(actor, purgatory_common::ABILITY_PRACTICE_SWORD_STRIKE),
            "World still shows the old grant; control must not use it until the key resolves"
        );
        let resolved = owner.take_durable_commits();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].command.key, key);

        let mut wind = GameplayOwner::new();
        let wind_id = ConnectionId::from_raw(92);
        enter_leased(&mut wind, wind_id, 92);
        let wind_actor = wind.entity_of(wind_id).unwrap();
        let wind_item = owned_debug_sword(&mut wind, wind_actor);
        wind.lease_for_test(wind_id, CharacterId::from_raw(92), 1, "dev.local", 1, &[wind_item]);
        wind.apply_input(InputUpdate::Drop {
            connection_id: wind_id,
            request: DropRequest {
                seq: 1,
                item_instance_id: wind_item,
            },
        });
        let wind_staged = wind.take_durable_commits();
        let wind_key = wind_staged[0].command.key.clone();
        let dummy = arm_strike_for_next_tick(&mut wind, wind_id);
        wind.settle_durable(wind_staged[0].token, unknown());
        wind.settle_durable(wind_staged[0].token, unknown());
        wind.simulate_tick(tick_dt());
        assert_eq!(
            wind.world().health_of(dummy).unwrap().current,
            10.0,
            "a scheduled strike landed while the commit reply was unknown"
        );
        let wind_again = wind.take_durable_commits();
        assert_eq!(wind_again[0].command.key, wind_key);

        owner.settle_durable(
            token,
            Ok(purgatory_persistence::DurableCommandResult {
                revisions: vec![(character, 2)],
                minted_item_ids: Vec::new(),
            }),
        );
        owner.settle_durable(
            token,
            Ok(purgatory_persistence::DurableCommandResult {
                revisions: vec![(character, 2)],
                minted_item_ids: Vec::new(),
            }),
        );
        assert!(owner.world().inventory_contains(actor, item));
        assert!(
            owner
                .world()
                .equipped_instance(actor, purgatory_simulation::EquipmentSlot::Weapon)
                .is_none()
        );
        assert!(
            !owner
                .world()
                .ability_granted(actor, purgatory_common::ABILITY_PRACTICE_SWORD_STRIKE),
            "the committed unequip was applied more than once or not at all"
        );
        assert!(owner.take_durable_commits().is_empty());
        let resumed = owner.last_received(id).unwrap().saturating_add(1);
        assert_eq!(
            owner.apply_input(command_update(
                id,
                cmd(resumed, MoveAxis::Right, false, false),
            )),
            SeqDecision::Accept,
            "control did not resume after the stored unequip was applied"
        );
        while rx.try_recv().is_ok() {}
        owner.apply_input(InputUpdate::AbilityActivate {
            connection_id: id,
            request: AbilityActivateRequest {
                seq: 2,
                input_epoch: 0,
                input_sequence: resumed,
                ability_id: purgatory_common::ABILITY_PRACTICE_SWORD_STRIKE,
                selected: None,
            },
        });
        assert_eq!(
            recv_ability(&mut rx),
            ServerAbility::Rejected {
                seq: 2,
                reason: AbilityCommandReject::NotGranted,
            }
        );
    }

    impl Pg {
        fn place_item(
            &mut self,
            entered: &Entered,
            definition: ContentId,
            slot: u16,
            nonce: u64,
        ) -> ItemInstanceId {
            let (revision, lease) = {
                let binding = self.owner.bindings.get(&entered.connection).unwrap();
                (
                    binding.committed_revision,
                    binding.authority.clone().unwrap(),
                )
            };
            let command = DurableCommand {
                key: format!("place-{}-{nonce}", entered.character.raw()),
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
                .commit_durable_leased(&command, Some(&lease))
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
                lease.login.as_str(),
                lease.generation,
                &[minted],
            );
            minted
        }
    }

    fn db_location(pg: &mut Pg, item: ItemInstanceId) -> CharacterItemLocation {
        match pg.service.read_item(item).unwrap().unwrap().owner {
            ItemOwner::Character { location, .. } => location,
            other => panic!("expected character ownership, got {other:?}"),
        }
    }

    fn world_location(
        owner: &GameplayOwner,
        item: ItemInstanceId,
    ) -> purgatory_simulation::ItemLocation {
        owner.world().item_record(item).unwrap().location
    }

    #[test]
    #[ignore]
    fn postgres_12c_reconnect_keeps_committed_items() {
        with_db(|pg| {
            let a = pg.enter("Mira");
            let bag = pg.place_item(&a, headwear(), 0, 1);
            let weapon = pg.place_item(&a, sword(), 1, 2);
            pg.owner.apply_input(InputUpdate::Equip {
                connection_id: a.connection,
                request: EquipRequest {
                    seq: 1,
                    slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
                    item_instance_id: weapon,
                },
            });
            pg.settle_next();
            let bag_owner = pg.service.read_item(bag).unwrap().unwrap().owner;
            let weapon_owner = pg.service.read_item(weapon).unwrap().unwrap().owner;

            pg.owner.prepare_logout(a.connection).unwrap();
            assert!(pg.owner.world().item_record(bag).is_none());
            assert!(pg.owner.world().item_record(weapon).is_none());
            pg.service.release_lease(&a.lease).unwrap();

            let admission = pg.service.admit(&pg.login, a.character).unwrap();
            let SessionAdmission::Granted {
                authority: Some(lease),
                restore,
            } = admission
            else {
                panic!("expected the released character to admit");
            };
            let mut partial = restore.clone();
            partial.items[1].quantity = 0;
            let failed = pg.owner.enter_restored(
                ConnectionId::from_raw(pg.next_connection),
                *partial,
                Some(lease.clone()),
                None,
                None,
                None,
            );
            pg.next_connection += 1;
            assert!(failed.is_err());
            assert!(pg.owner.world().item_record(bag).is_none());
            assert_eq!(pg.service.read_item(bag).unwrap().unwrap().owner, bag_owner);
            assert_eq!(
                pg.service.read_item(weapon).unwrap().unwrap().owner,
                weapon_owner
            );

            let connection = ConnectionId::from_raw(pg.next_connection);
            pg.next_connection += 1;
            pg.owner
                .enter_restored(connection, *restore, Some(lease.clone()), None, None, None)
                .unwrap();
            let actor = pg.owner.entity_of(connection).unwrap();
            assert!(pg.owner.world().inventory_contains(actor, bag));
            assert_eq!(
                pg.owner
                    .world()
                    .equipped_instance(actor, purgatory_simulation::EquipmentSlot::Weapon),
                Some(weapon)
            );
            assert!(pg.owner.world().ability_granted(
                actor,
                purgatory_common::ABILITY_PRACTICE_SWORD_STRIKE
            ));

            let (authority, _) = pg.owner.stop_for_reconnect(a.character).unwrap();
            let (next, again) = pg.service.supersede(&authority).unwrap();
            let reconnected = ConnectionId::from_raw(pg.next_connection);
            pg.owner
                .enter_restored(reconnected, again, Some(next), None, None, None)
                .unwrap();
            let actor = pg.owner.entity_of(reconnected).unwrap();
            assert!(pg.owner.world().inventory_contains(actor, bag));
            assert_eq!(
                pg.owner
                    .world()
                    .equipped_instance(actor, purgatory_simulation::EquipmentSlot::Weapon),
                Some(weapon)
            );
            assert_eq!(pg.service.read_item(bag).unwrap().unwrap().owner, bag_owner);
            assert_eq!(
                pg.service.read_item(weapon).unwrap().unwrap().owner,
                weapon_owner
            );
        });
    }

    #[test]
    #[ignore]
    fn postgres_12c_equipment_replacement_matches_world_and_database() {
        with_db(|pg| {
            let a = pg.enter("Mira");
            let first = pg.place_item(&a, sword(), 0, 1);
            let second = pg.place_item(&a, sword(), 1, 2);
            pg.owner.apply_input(InputUpdate::Equip {
                connection_id: a.connection,
                request: EquipRequest {
                    seq: 1,
                    slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
                    item_instance_id: first,
                },
            });
            pg.settle_next();
            pg.owner.apply_input(InputUpdate::Equip {
                connection_id: a.connection,
                request: EquipRequest {
                    seq: 2,
                    slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
                    item_instance_id: second,
                },
            });
            pg.settle_next();
            assert_eq!(
                db_location(pg, first),
                CharacterItemLocation::Inventory { slot: 1 }
            );
            assert_eq!(
                world_location(&pg.owner, first),
                purgatory_simulation::ItemLocation::Inventory {
                    owner: a.actor,
                    slot: 1
                }
            );
            assert_eq!(
                db_location(pg, second),
                CharacterItemLocation::Equipped {
                    slot: purgatory_persistence::DurableEquipmentSlot::Weapon,
                }
            );
            assert!(pg.owner.world().ability_granted(
                a.actor,
                purgatory_common::ABILITY_PRACTICE_SWORD_STRIKE
            ));

            let mut incoming = None;
            let mut nonce = 10u64;
            for slot in 0..purgatory_simulation::INVENTORY_CAPACITY as u16 {
                if slot == 1 {
                    continue;
                }
                let definition = if slot == 4 { sword() } else { headwear() };
                let item = pg.place_item(&a, definition, slot, nonce);
                nonce += 1;
                if slot == 4 {
                    incoming = Some(item);
                }
            }
            let incoming = incoming.unwrap();
            assert_eq!(
                pg.owner.world().inventory_count(a.actor),
                purgatory_simulation::INVENTORY_CAPACITY
            );
            pg.owner.apply_input(InputUpdate::Equip {
                connection_id: a.connection,
                request: EquipRequest {
                    seq: 3,
                    slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
                    item_instance_id: incoming,
                },
            });
            pg.settle_next();
            assert_eq!(
                db_location(pg, second),
                CharacterItemLocation::Inventory { slot: 4 }
            );
            assert_eq!(
                world_location(&pg.owner, second),
                purgatory_simulation::ItemLocation::Inventory {
                    owner: a.actor,
                    slot: 4
                }
            );
            assert_eq!(
                db_location(pg, incoming),
                CharacterItemLocation::Equipped {
                    slot: purgatory_persistence::DurableEquipmentSlot::Weapon,
                }
            );
            assert_eq!(
                world_location(&pg.owner, incoming),
                purgatory_simulation::ItemLocation::Equipped {
                    owner: a.actor,
                    slot: purgatory_simulation::EquipmentSlot::Weapon,
                }
            );
            assert_eq!(
                pg.owner.world().inventory_count(a.actor),
                purgatory_simulation::INVENTORY_CAPACITY
            );
            assert!(pg.owner.world().ability_granted(
                a.actor,
                purgatory_common::ABILITY_PRACTICE_SWORD_STRIKE
            ));
        });
    }

    #[test]
    #[ignore]
    fn postgres_12c_failed_apply_reconciles_without_a_success_reply() {
        with_db(|pg| {
            let a = pg.enter("Mira");
            let (tx, mut rx) = tokio::sync::mpsc::channel(8);
            pg.owner.bindings.get_mut(&a.connection).unwrap().interact = Some(tx);
            let dropped = pg.place_item(&a, sword(), 0, 1);
            let kept = pg.place_item(&a, headwear(), 1, 2);
            pg.owner.apply_input(InputUpdate::Drop {
                connection_id: a.connection,
                request: DropRequest {
                    seq: 1,
                    item_instance_id: dropped,
                },
            });
            let staged = pg.owner.take_durable_commits();
            let committed = pg
                .service
                .commit_durable_leased(&staged[0].command, staged[0].lease.as_ref())
                .unwrap();
            assert!(pg
                .owner
                .world_mut()
                .retire_inventory_instance(a.actor, dropped));
            pg.owner.settle_durable(staged[0].token, Ok(committed));
            assert!(rx.try_recv().is_err());
            pg.owner.apply_input(InputUpdate::Drop {
                connection_id: a.connection,
                request: DropRequest {
                    seq: 2,
                    item_instance_id: kept,
                },
            });
            assert!(pg.owner.take_durable_commits().is_empty());
            let restore = pg.service.read_owned_restore(a.character).unwrap();
            let revision = restore.character.persistence_revision;
            assert!(pg
                .owner
                .complete_reconcile(a.connection, revision, restore));
            let mut accepted = false;
            while let Ok(message) = rx.try_recv() {
                match message {
                    ServerControl::Item(ServerItem::DropAccepted { seq: 1 }) => accepted = true,
                    ServerControl::Item(ServerItem::DropRejected { .. }) => {
                        panic!("a committed drop was described as rolled back: {message:?}")
                    }
                    ServerControl::Inventory(_) | ServerControl::AbilityGrants(_) => {}
                    other => panic!("unexpected reconcile reply: {other:?}"),
                }
            }
            assert!(accepted, "reconcile must resolve the committed drop");
            let manifested = pg.owner.world().world_drop_entity_for_item(dropped);
            assert!(
                manifested.is_some(),
                "a committed drop must be visible before DropAccepted"
            );
            assert_eq!(
                pg.owner
                    .world()
                    .item_instance_at_world_drop(manifested.unwrap()),
                Some(dropped),
                "the ground item keeps its original instance id"
            );
            assert!(pg.owner.world().inventory_contains(a.actor, kept));
            assert_eq!(
                pg.service.read_item(dropped).unwrap().unwrap().owner,
                ItemOwner::Ground
            );
            pg.owner
                .advance_ground_clock(Duration::from_secs(200) - Duration::from_nanos(1));
            assert!(
                pg.owner.take_durable_commits().is_empty(),
                "the drop stays until 200 seconds"
            );
            assert!(pg.owner.world().world_drop_entity_for_item(dropped).is_some());
            pg.owner.advance_ground_clock(Duration::from_nanos(1));
            let retiring = pg.owner.take_durable_commits();
            assert_eq!(retiring.len(), 1);
            assert_eq!(retiring[0].command.key, format!("retire-{}", dropped.raw()));
            assert!(retiring[0].lease.is_none());
            let retired = pg
                .service
                .commit_durable_leased(&retiring[0].command, retiring[0].lease.as_ref())
                .unwrap();
            pg.owner.settle_durable(retiring[0].token, Ok(retired));
            assert!(pg.owner.world().world_drop_entity_for_item(dropped).is_none());
            assert_eq!(
                pg.service.read_item(dropped).unwrap().unwrap().owner,
                ItemOwner::Retired
            );
            assert!(matches!(
                pg.service.read_item(kept).unwrap().unwrap().owner,
                ItemOwner::Character {
                    location: CharacterItemLocation::Inventory { slot: 1 },
                    ..
                }
            ));
            pg.owner.apply_input(InputUpdate::Drop {
                connection_id: a.connection,
                request: DropRequest {
                    seq: 2,
                    item_instance_id: kept,
                },
            });
            assert_eq!(pg.owner.take_durable_commits().len(), 1);
        });
    }

    #[test]
    #[ignore]
    fn postgres_12c_unknown_outcome_resolves_the_stored_key() {
        with_db(|pg| {
            let a = pg.enter("Mira");
            let item = pg.place_item(&a, sword(), 0, 1);
            pg.owner.apply_input(InputUpdate::Drop {
                connection_id: a.connection,
                request: DropRequest {
                    seq: 1,
                    item_instance_id: item,
                },
            });
            let staged = pg.owner.take_durable_commits();
            let key = staged[0].command.key.clone();
            let stored = pg
                .service
                .commit_durable_leased(&staged[0].command, staged[0].lease.as_ref())
                .unwrap();
            let unknown = || {
                Err(PersistError::storage(
                    "commit outcome unknown: reply was not observed",
                ))
            };
            pg.owner.settle_durable(staged[0].token, unknown());
            pg.owner.settle_durable(staged[0].token, unknown());
            let again = pg.owner.take_durable_commits();
            assert_eq!(again.len(), 1);
            assert_eq!(again[0].command.key, key);
            let resolved = pg
                .service
                .commit_durable_leased(&again[0].command, again[0].lease.as_ref())
                .unwrap();
            assert_eq!(resolved, stored);
            pg.owner.settle_durable(again[0].token, Ok(resolved));
            assert!(pg.owner.world().world_drop_entity_for_item(item).is_some());
            assert!(!pg.owner.world().inventory_contains(a.actor, item));
            assert_eq!(
                pg.service.read_item(item).unwrap().unwrap().owner,
                ItemOwner::Ground
            );
            let other = pg.place_item(&a, headwear(), 0, 2);
            pg.owner.apply_input(InputUpdate::Drop {
                connection_id: a.connection,
                request: DropRequest {
                    seq: 2,
                    item_instance_id: other,
                },
            });
            let next = pg.owner.take_durable_commits();
            assert_eq!(next.len(), 1);
            assert_ne!(next[0].command.key, key);
        });
    }

    fn fail_committed_drop(owner: &mut GameplayOwner, id: ConnectionId) -> CharacterId {
        let actor = owner.entity_of(id).unwrap();
        let item = owned_debug_sword(owner, actor);
        let character = owner.bindings.get(&id).unwrap().character_id.unwrap();
        owner.lease_for_test(id, character, 1, "dev.local", 1, &[item]);
        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 1,
                item_instance_id: item,
            },
        });
        let staged = owner.take_durable_commits();
        assert!(owner.world_mut().retire_inventory_instance(actor, item));
        owner.settle_durable(
            staged[0].token,
            Ok(purgatory_persistence::DurableCommandResult {
                revisions: vec![(character, 2)],
                minted_item_ids: Vec::new(),
            }),
        );
        character
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn inconsistent_character_blocks_new_effects_until_restore_or_stop() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(80);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        enter_leased(&mut owner, id, 80);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let pos = owner.world().transform_of(actor).unwrap().position;
        let address = owner.world().address_of(actor).unwrap();
        let landed = owner
            .world_mut()
            .spawn(
                RuntimeSpawnRequest::transient_at(address)
                    .with_transform(Transform::from_position([pos[0] + 1.0, pos[1]]))
                    .with_health(Health::full(10.0))
                    .visible(),
            )
            .unwrap();
        activate_strike(&mut owner, id, 1, None);
        tick_ability(&mut owner, 8);
        let landed_health = owner.world().health_of(landed).unwrap().current;
        assert!(
            (landed_health - 5.0).abs() < 1e-4,
            "strike did not land before the failed apply: {landed_health}"
        );
        let actor = owner.entity_of(id).unwrap();
        assert!(owner.world_mut().grant_ability(actor, dash_id()));
        let move_seq = owner.last_received(id).unwrap().saturating_add(1);
        owner.apply_input(command_update(
            id,
            cmd(move_seq, MoveAxis::Right, false, false),
        ));
        owner.apply_input(InputUpdate::AbilityActivate {
            connection_id: id,
            request: AbilityActivateRequest {
                seq: 2,
                input_epoch: 0,
                input_sequence: move_seq,
                ability_id: dash_id(),
                selected: None,
            },
        });
        owner.simulate_tick(tick_dt());
        assert!(owner.world().player_dash_of(actor).is_some());
        let held = horizontal(&owner, id);
        let character = fail_committed_drop(&mut owner, id);
        assert_eq!(
            owner.apply_input(command_update(
                id,
                cmd(move_seq.saturating_add(1), MoveAxis::Right, false, false),
            )),
            SeqDecision::Stale,
            "movement input was accepted while the character was inconsistent"
        );
        assert_eq!(
            owner.apply_input(InputUpdate::AbilityActivate {
                connection_id: id,
                request: AbilityActivateRequest {
                    seq: 4,
                    input_epoch: 0,
                    input_sequence: move_seq.saturating_add(1),
                    ability_id: dash_id(),
                    selected: None,
                },
            }),
            SeqDecision::Stale,
            "ability activation was accepted while the character was inconsistent"
        );
        owner.simulate_tick(tick_dt());
        assert_eq!(
            horizontal(&owner, id),
            held,
            "held movement or Dash continued while reconciliation was outstanding"
        );
        assert_eq!(owner.world().health_of(landed).unwrap().current, landed_health);

        let mut wind = GameplayOwner::new();
        let wind_id = ConnectionId::from_raw(81);
        enter_leased(&mut wind, wind_id, 81);
        let pending = arm_strike_for_next_tick(&mut wind, wind_id);
        fail_committed_drop(&mut wind, wind_id);
        wind.simulate_tick(tick_dt());
        assert_eq!(
            wind.world().health_of(pending).unwrap().current,
            10.0,
            "a scheduled strike landed while reconciliation was outstanding"
        );
        wind.lose_authority(wind_id);
        wind.simulate_tick(tick_dt());
        assert_eq!(wind.world().health_of(pending).unwrap().current, 10.0);
        assert_eq!(
            wind.apply_input(command_update(
                wind_id,
                cmd(2, MoveAxis::Right, false, false),
            )),
            SeqDecision::Stale
        );

        let bad = character_restore(
            character,
            vec![owned_record(
                character,
                ItemInstanceId::from_raw(9_991),
                ContentId::from_raw(9_999_991),
                CharacterItemLocation::Inventory { slot: 0 },
            )],
        );
        assert!(!owner.complete_reconcile(id, 2, bad));
        assert_eq!(
            owner.apply_input(command_update(
                id,
                cmd(move_seq.saturating_add(1), MoveAxis::Right, false, false),
            )),
            SeqDecision::Stale,
            "a failed reconciliation read restored control"
        );
        while let Ok(message) = rx.try_recv() {
            assert!(
                !matches!(
                    message,
                    ServerControl::Item(ServerItem::DropRejected {
                        reason: DropRejectReason::StateBlocked,
                        ..
                    })
                ),
                "a committed drop was described as rolled back: {message:?}"
            );
        }
        assert!(owner.complete_reconcile(id, 2, character_restore(character, Vec::new())));
        assert_eq!(
            owner.apply_input(command_update(
                id,
                cmd(move_seq.saturating_add(1), MoveAxis::Right, false, false),
            )),
            SeqDecision::Accept
        );
        owner.lose_authority(id);
        let stopped = horizontal(&owner, id);
        owner.simulate_tick(tick_dt());
        assert_eq!(horizontal(&owner, id), stopped);
        assert_eq!(owner.world().health_of(landed).unwrap().current, landed_health);
    }

    fn next_item(rx: &mut tokio::sync::mpsc::Receiver<ServerControl>) -> ServerItem {
        loop {
            match rx.try_recv().expect("duplicate request disappeared") {
                ServerControl::Item(event) => {
                    assert!(
                        !matches!(
                            event,
                            ServerItem::DropRejected {
                                reason: DropRejectReason::StateBlocked,
                                ..
                            } | ServerItem::PickupRejected {
                                reason: PickupRejectReason::StateBlocked,
                                ..
                            }
                        ),
                        "a committed operation was described as rolled back: {event:?}"
                    );
                    return event;
                }
                ServerControl::Inventory(_) | ServerControl::AbilityGrants(_) => continue,
                other => panic!("expected an item resolution, got {other:?}"),
            }
        }
    }

    fn next_equipment(rx: &mut tokio::sync::mpsc::Receiver<ServerControl>) -> ServerEquipment {
        loop {
            match rx.try_recv().expect("duplicate request disappeared") {
                ServerControl::Equipment(event) => {
                    assert!(
                        !matches!(
                            event,
                            ServerEquipment::Rejected {
                                reason: EquipmentRejectReason::StateBlocked,
                                ..
                            }
                        ),
                        "a committed operation was described as rolled back: {event:?}"
                    );
                    return event;
                }
                ServerControl::Inventory(_) | ServerControl::AbilityGrants(_) => continue,
                other => panic!("expected an equipment resolution, got {other:?}"),
            }
        }
    }

    #[test]
    fn reconciled_client_retry_resolves_the_committed_operation() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(1);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let actor = owner.entity_of(id).unwrap();
        let item = owned_debug_sword(&mut owner, actor);
        let character = CharacterId::from_raw(90);
        owner.lease_for_test(id, character, 1, "dev.local", 1, &[item]);
        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 1,
                item_instance_id: item,
            },
        });
        let staged = owner.take_durable_commits();
        assert!(owner.world_mut().retire_inventory_instance(actor, item));
        owner.settle_durable(
            staged[0].token,
            Ok(purgatory_persistence::DurableCommandResult {
                revisions: vec![(character, 2)],
                minted_item_ids: Vec::new(),
            }),
        );
        assert!(owner.complete_reconcile(id, 2, character_restore(character, Vec::new())));
        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 1,
                item_instance_id: item,
            },
        });
        assert_eq!(next_item(&mut rx), ServerItem::DropAccepted { seq: 1 });
        owner.apply_input(InputUpdate::Drop {
            connection_id: id,
            request: DropRequest {
                seq: 1,
                item_instance_id: item,
            },
        });
        assert_eq!(next_item(&mut rx), ServerItem::DropAccepted { seq: 1 });
        while rx.try_recv().is_ok() {}

        let address = owner.world().address_of(actor).unwrap();
        let position = owner.world().transform_of(actor).unwrap().position;
        let (ground, entity) = owner
            .world_mut()
            .spawn_world_drop_item(address, position, sword(), 1, 1)
            .unwrap();
        owner.durable_items.insert(ground);
        owner.apply_input(InputUpdate::Pickup {
            connection_id: id,
            request: PickupRequest {
                seq: 1,
                target: wire_id(entity),
            },
        });
        let staged = owner.take_durable_commits();
        assert!(owner.world_mut().destroy_world_drop_item(ground));
        owner.settle_durable(
            staged[0].token,
            Ok(purgatory_persistence::DurableCommandResult {
                revisions: vec![(character, 3)],
                minted_item_ids: Vec::new(),
            }),
        );
        assert!(owner.complete_reconcile(
            id,
            3,
            character_restore(
                character,
                vec![owned_record(
                    character,
                    ground,
                    sword(),
                    CharacterItemLocation::Inventory { slot: 0 },
                )],
            ),
        ));
        match next_item(&mut rx) {
            ServerItem::PickupAccepted {
                seq: 1,
                item_instance_id,
                ..
            } => assert_eq!(item_instance_id, ground),
            other => panic!("reconcile did not resolve the committed pickup: {other:?}"),
        }
        while rx.try_recv().is_ok() {}
        owner.apply_input(InputUpdate::Pickup {
            connection_id: id,
            request: PickupRequest {
                seq: 1,
                target: wire_id(entity),
            },
        });
        match next_item(&mut rx) {
            ServerItem::PickupAccepted {
                seq: 1,
                item_instance_id,
                ..
            } => assert_eq!(item_instance_id, ground),
            other => panic!("pickup retry was not the committed result: {other:?}"),
        }
        while rx.try_recv().is_ok() {}

        let weapon = owned_debug_sword(&mut owner, actor);
        owner.durable_items.insert(weapon);
        owner.apply_input(InputUpdate::Equip {
            connection_id: id,
            request: EquipRequest {
                seq: 1,
                slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
                item_instance_id: weapon,
            },
        });
        let staged = owner.take_durable_commits();
        assert!(owner.world_mut().retire_inventory_instance(actor, weapon));
        owner.settle_durable(
            staged[0].token,
            Ok(purgatory_persistence::DurableCommandResult {
                revisions: vec![(character, 4)],
                minted_item_ids: Vec::new(),
            }),
        );
        assert!(owner.complete_reconcile(
            id,
            4,
            character_restore(
                character,
                vec![owned_record(
                    character,
                    weapon,
                    sword(),
                    CharacterItemLocation::Equipped {
                        slot: purgatory_persistence::DurableEquipmentSlot::Weapon,
                    },
                )],
            ),
        ));
        assert_eq!(
            next_equipment(&mut rx),
            ServerEquipment::Accepted { seq: 1 }
        );
        while rx.try_recv().is_ok() {}
        owner.apply_input(InputUpdate::Equip {
            connection_id: id,
            request: EquipRequest {
                seq: 1,
                slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
                item_instance_id: weapon,
            },
        });
        assert_eq!(
            next_equipment(&mut rx),
            ServerEquipment::Accepted { seq: 1 }
        );
    }

    fn reward_choice(active: super::super::dialogue::ActiveDialogue) -> super::super::dialogue::ChoicePlan {
        super::super::dialogue::ChoicePlan {
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
        }
    }

    fn resend_committed_dialogue_choice(
        owner: &mut GameplayOwner,
        id: ConnectionId,
        actor: purgatory_simulation::EntityId,
        session_id: u32,
        beat_index: u32,
        plan: super::super::dialogue::ChoicePlan,
        beat_id: String,
    ) -> Vec<String> {
        let still_on_choice = owner.dialogues.active(actor).is_some_and(|active| {
            active.session_id == session_id && active == plan.accepted
        });
        if still_on_choice {
            owner.stage_dialogue(id, plan, Some(beat_id));
        }
        owner.apply_input(InputUpdate::DialogueChoose {
            connection_id: id,
            request: DialogueChoose {
                session_id,
                beat_index,
                choice_index: 0,
            },
        });
        owner
            .take_durable_commits()
            .into_iter()
            .filter(|submit| !submit.command.place_new.is_empty() || !submit.command.learned.is_empty())
            .map(|submit| submit.command.key)
            .collect()
    }

    fn dialogue_outcome_was_truthful(
        rx: &mut tokio::sync::mpsc::Receiver<ServerControl>,
        session_id: u32,
    ) -> bool {
        let mut truthful = false;
        while let Ok(message) = rx.try_recv() {
            match message {
                ServerControl::DialogueChoiceAccepted(accepted)
                    if accepted.session_id == session_id && accepted.choice_index == 0 =>
                {
                    truthful = true;
                }
                ServerControl::Interact(ServerInteract::Closed {
                    session_id: closed, ..
                }) if closed == session_id => {
                    truthful = true;
                }
                ServerControl::Inventory(_)
                | ServerControl::AbilityGrants(_)
                | ServerControl::DialogueLine(_)
                | ServerControl::Interact(ServerInteract::Opened { .. }) => {}
                other => panic!("unexpected dialogue reconcile reply: {other:?}"),
            }
        }
        truthful
    }

    fn assert_reward_state(
        owner: &GameplayOwner,
        actor: purgatory_simulation::EntityId,
        minted: ItemInstanceId,
    ) {
        assert_eq!(owner.world().inventory_count(actor), 1);
        assert!(owner.world().inventory_contains(actor, minted));
        assert!(owner.narrative.fact(actor, "welcome.workshop.package_at_inn"));
        assert!(owner.world().ability_granted(actor, dash_id()));
    }

    fn reward_restore(
        character: CharacterId,
        minted: ItemInstanceId,
        beat_id: &str,
    ) -> purgatory_persistence::OwnedRestore {
        let mut narrative = purgatory_persistence::CharacterNarrativeState::default();
        narrative
            .facts
            .insert("welcome.workshop.package_at_inn".into(), true);
        narrative
            .learned_abilities
            .insert(purgatory_common::ABILITY_MOVEMENT_DASH.raw().unwrap());
        narrative
            .dialogue_heard
            .insert((20_001, beat_id.to_string()));
        let mut restore = character_restore(
            character,
            vec![owned_record(
                character,
                minted,
                purgatory_common::ITEM_PACKAGE,
                CharacterItemLocation::Inventory { slot: 0 },
            )],
        );
        restore.narrative = narrative;
        restore
    }

    #[test]
    fn reconciled_dialogue_resend_does_not_mint_a_second_reward() {
        let mut owner = GameplayOwner::new();
        let id = ConnectionId::from_raw(93);
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        owner.attach(id);
        owner.bindings.get_mut(&id).unwrap().interact = Some(tx);
        let character = CharacterId::from_raw(93);
        owner.lease_for_test(id, character, 1, "dev.local", 1, &[]);
        move_player_to_content(&mut owner, id, "npc.welcome.traveler_stayed");
        let traveler = find_content(&owner, "npc.welcome.traveler_stayed");
        owner.apply_input(InputUpdate::InteractOpen {
            connection_id: id,
            target: wire_id(traveler),
        });
        while rx.try_recv().is_ok() {}
        let actor = owner.entity_of(id).unwrap();
        let active = owner.dialogues.active(actor).expect("dialogue opened");
        let beat_id = owner
            .registry
            .npc_dialogue_by_id(active.npc_content_id)
            .and_then(|dialogue| dialogue.beat(active.beat_index))
            .expect("authored beat")
            .id
            .clone();
        let plan = reward_choice(active);
        owner.stage_dialogue(id, plan.clone(), Some(beat_id.clone()));
        let staged = owner.take_durable_commits();
        assert_eq!(staged.len(), 1);
        assert!(staged[0].command.key.contains("-r1-"));
        let stack_limit = owner.registry.item_by_id(headwear()).unwrap().stack_limit;
        let (_, slot) = owner
            .world_mut()
            .grant_inventory_item(actor, headwear(), 1, stack_limit)
            .unwrap();
        assert_eq!(slot, 0);
        let minted = ItemInstanceId::from_raw(77_001);
        owner.settle_durable(
            staged[0].token,
            Ok(purgatory_persistence::DurableCommandResult {
                revisions: vec![(character, 2)],
                minted_item_ids: vec![minted],
            }),
        );
        assert!(!owner.world().inventory_contains(actor, minted));
        assert!(owner.complete_reconcile(id, 2, reward_restore(character, minted, &beat_id)));
        let revision = owner.bindings.get(&id).unwrap().committed_revision;
        assert_eq!(revision, 2);
        let keys = resend_committed_dialogue_choice(
            &mut owner,
            id,
            actor,
            active.session_id,
            active.beat_index.raw(),
            plan,
            beat_id,
        );
        assert!(
            keys.iter().all(|key| !key.contains(&format!("-r{revision}-"))),
            "resending the committed choice minted a second reward under revision {revision}: {keys:?}"
        );
        assert!(
            keys.is_empty(),
            "resending the committed choice minted a second reward under revision {revision}: {keys:?}"
        );
        assert!(
            dialogue_outcome_was_truthful(&mut rx, active.session_id),
            "reconcile restored the reward without a truthful client outcome"
        );
        assert_reward_state(&owner, actor, minted);
    }

    #[test]
    fn logout_drops_departed_character_ids_and_keeps_live_ground() {
        let mut owner = GameplayOwner::new();
        let mut departed = Vec::new();
        for index in 0..24u64 {
            let connection = ConnectionId::from_raw(300 + index);
            let character = CharacterId::from_raw(800 + index);
            let item = ItemInstanceId::from_raw(40_000 + index);
            let lease = purgatory_persistence::LeaseAuthority {
                login: DevLogin::parse("dev.local").unwrap(),
                character_id: character,
                generation: 1,
            };
            owner
                .enter_restored(
                    connection,
                    character_restore(
                        character,
                        vec![owned_record(
                            character,
                            item,
                            sword(),
                            CharacterItemLocation::Inventory { slot: 0 },
                        )],
                    ),
                    Some(lease),
                    None,
                    None,
                    None,
                )
                .unwrap();
            assert!(
                owner.durable_items.contains(&item),
                "leased entry did not register the character item"
            );
            departed.push((connection, item));
        }
        let actor = owner.entity_of(ConnectionId::from_raw(300)).unwrap();
        let address = owner.world().address_of(actor).unwrap();
        let position = owner.world().transform_of(actor).unwrap().position;
        let (ground, _) = owner
            .world_mut()
            .spawn_world_drop_item(address, position, sword(), 1, 1)
            .unwrap();
        owner.durable_items.insert(ground);
        for (connection, item) in &departed {
            owner.prepare_logout(*connection).unwrap();
            assert!(owner.world().item_record(*item).is_none());
            assert!(
                !owner.durable_items.contains(item),
                "character item {} remained registered after logout",
                item.raw()
            );
        }
        assert!(owner.durable_items.contains(&ground));
        assert!(owner.world().item_record(ground).is_some());
        assert_eq!(owner.durable_items.len(), 1);
    }
