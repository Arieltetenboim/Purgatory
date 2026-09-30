impl GameplayOwner {
    fn stages_durable(&self, connection_id: ConnectionId) -> bool {
        self.bindings.get(&connection_id).is_some_and(|binding| {
            binding.authority.is_some()
                && binding.character_id.is_some()
                && binding.committed_revision > 0
        })
    }

    fn client_open(&self, connection_id: ConnectionId) -> bool {
        self.bindings
            .get(&connection_id)
            .is_some_and(|binding| !binding.authority_lost)
    }

    fn first_free_inventory_slot(&self, actor: EntityId) -> Option<u16> {
        let used: HashSet<u16> = self
            .world
            .inventory_snapshot(actor)
            .into_iter()
            .map(|(slot, _, _)| slot)
            .collect();
        (0..purgatory_simulation::INVENTORY_CAPACITY as u16).find(|slot| !used.contains(slot))
    }

    fn inventory_slot_of(&self, item: purgatory_common::ItemInstanceId) -> Option<u16> {
        match self.world.item_record(item)?.location {
            purgatory_simulation::ItemLocation::Inventory { slot, .. } => Some(slot),
            _ => None,
        }
    }

    fn durable_context(
        &self,
        connection_id: ConnectionId,
    ) -> Option<(
        EntityId,
        CharacterId,
        u64,
        purgatory_persistence::LeaseAuthority,
    )> {
        let binding = self.bindings.get(&connection_id)?;
        Some((
            binding.entity,
            binding.character_id?,
            binding.committed_revision,
            binding.authority.clone()?,
        ))
    }

    fn reject_drop(&mut self, connection_id: ConnectionId, seq: u32, reason: DropRejectReason) {
        let event = ServerItem::DropRejected { seq, reason };
        let tx = self
            .bindings
            .get(&connection_id)
            .and_then(|binding| binding.interact.clone());
        if let Some(binding) = self.bindings.get_mut(&connection_id) {
            binding.last_drop_seq = Some(seq);
            binding.last_drop_result = Some(event);
        }
        Self::send_pickup_result(tx.as_ref(), event);
    }

    fn reject_pickup(
        &mut self,
        connection_id: ConnectionId,
        seq: u32,
        reason: PickupRejectReason,
    ) {
        let event = ServerItem::PickupRejected { seq, reason };
        let tx = self
            .bindings
            .get(&connection_id)
            .and_then(|binding| binding.interact.clone());
        if let Some(binding) = self.bindings.get_mut(&connection_id) {
            binding.last_pickup_seq = Some(seq);
            binding.last_pickup_result = Some(event);
        }
        Self::send_pickup_result(tx.as_ref(), event);
    }

    fn reject_equipment(
        &mut self,
        connection_id: ConnectionId,
        seq: u32,
        reason: EquipmentRejectReason,
    ) {
        let event = ServerEquipment::Rejected { seq, reason };
        let tx = self
            .bindings
            .get(&connection_id)
            .and_then(|binding| binding.interact.clone());
        if let Some(binding) = self.bindings.get_mut(&connection_id) {
            binding.last_equipment_seq = Some(seq);
            binding.last_equipment_result = Some(event);
        }
        Self::send_equipment_result(tx.as_ref(), event);
    }

    fn note_inflight_seq(&mut self, connection_id: ConnectionId, kind: InFlight, seq: u32) {
        let Some(binding) = self.bindings.get_mut(&connection_id) else {
            return;
        };
        match kind {
            InFlight::Drop => {
                binding.last_drop_seq = Some(seq);
                binding.last_drop_result = None;
            }
            InFlight::Pickup => {
                binding.last_pickup_seq = Some(seq);
                binding.last_pickup_result = None;
            }
            InFlight::Equipment => {
                binding.last_equipment_seq = Some(seq);
                binding.last_equipment_result = None;
            }
        }
    }

    fn enqueue_durable(&mut self, pending: DurablePending) {
        let token = self.durable_tokens;
        self.durable_tokens = self.durable_tokens.saturating_add(1);
        for item in &pending.reserved {
            self.reserved_items.insert(*item);
        }
        self.durable_pending.insert(token, pending);
        self.durable_outbound.push(token);
    }

    fn release_reserved(&mut self, items: &[purgatory_common::ItemInstanceId]) {
        for item in items {
            self.reserved_items.remove(item);
        }
    }

    fn abandon_staged(&mut self, connection_id: ConnectionId) {
        let revision = self
            .bindings
            .get(&connection_id)
            .map(|binding| binding.committed_revision)
            .unwrap_or(0);
        self.finish_durable(connection_id, revision);
    }

    pub fn take_durable_commits(&mut self) -> Vec<super::durable_play::DurableSubmit> {
        self.promote_due_retries(std::time::Instant::now());
        let tokens = std::mem::take(&mut self.durable_outbound);
        tokens
            .into_iter()
            .filter_map(|token| {
                let pending = self.durable_pending.get(&token)?;
                Some(super::durable_play::DurableSubmit {
                    token,
                    command: pending.command.clone(),
                    lease: pending.lease.clone(),
                })
            })
            .collect()
    }

    /// Submit the same command again after an unknown commit outcome.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn durable_retry(&self, token: u64) -> Option<super::durable_play::DurableSubmit> {
        let pending = self.durable_pending.get(&token)?;
        Some(super::durable_play::DurableSubmit {
            token,
            command: pending.command.clone(),
            lease: pending.lease.clone(),
        })
    }

    fn unknown_backoff() -> std::time::Duration {
        if cfg!(test) {
            std::time::Duration::ZERO
        } else {
            std::time::Duration::from_millis(200)
        }
    }

    fn promote_due_retries(&mut self, now: std::time::Instant) {
        let due: Vec<u64> = self
            .durable_retry_at
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(token, _)| *token)
            .collect();
        for token in due {
            self.durable_retry_at.remove(&token);
            if self.durable_pending.contains_key(&token) && !self.durable_outbound.contains(&token)
            {
                self.durable_outbound.push(token);
            }
        }
    }

    pub fn settle_durable(
        &mut self,
        token: u64,
        result: Result<
            purgatory_persistence::DurableCommandResult,
            purgatory_persistence::PersistError,
        >,
    ) {
        if !self.durable_pending.contains_key(&token) {
            return;
        }
        if result
            .as_ref()
            .err()
            .is_some_and(super::durable_play::commit_outcome_unknown)
        {
            self.durable_retry_at.insert(
                token,
                std::time::Instant::now() + Self::unknown_backoff(),
            );
            return;
        }
        let Some(pending) = self.durable_pending.remove(&token) else {
            return;
        };
        self.durable_outbound.retain(|queued| *queued != token);
        self.durable_retry_at.remove(&token);
        match result {
            Ok(committed) => self.apply_committed(pending, &committed),
            Err(_) => {
                self.release_reserved(&pending.reserved);
                self.fail_committed(pending);
            }
        }
    }

    fn fail_committed(&mut self, pending: DurablePending) {
        let connection_id = effect_connection(&pending.effect);
        match &pending.effect {
            super::durable_play::DurableEffect::Drop { seq, .. } => {
                self.reject_drop(connection_id, *seq, DropRejectReason::StateBlocked);
            }
            super::durable_play::DurableEffect::Pickup { seq, .. } => {
                self.reject_pickup(connection_id, *seq, PickupRejectReason::StateBlocked);
            }
            super::durable_play::DurableEffect::Equip { seq, .. }
            | super::durable_play::DurableEffect::Unequip { seq, .. } => {
                self.reject_equipment(connection_id, *seq, EquipmentRejectReason::StateBlocked);
            }
            super::durable_play::DurableEffect::Dialogue { .. }
            | super::durable_play::DurableEffect::Heard { .. }
            | super::durable_play::DurableEffect::RetireGround { .. } => {}
        }
        self.abandon_staged(connection_id);
    }

    fn apply_committed(
        &mut self,
        pending: DurablePending,
        committed: &purgatory_persistence::DurableCommandResult,
    ) {
        let connection_id = effect_connection(&pending.effect);
        let adopted = self
            .bindings
            .get(&connection_id)
            .and_then(|binding| {
                committed.revisions.iter().find_map(|(id, revision)| {
                    (binding.character_id == Some(*id)).then_some(*revision)
                })
            })
            .unwrap_or_else(|| {
                self.bindings
                    .get(&connection_id)
                    .map(|binding| binding.committed_revision)
                    .unwrap_or(0)
            });
        let applied = self.apply_effect(&pending.effect, committed);
        if !applied {
            self.queue_reconcile(connection_id, adopted, pending.reserved);
            return;
        }
        self.release_reserved(&pending.reserved);
        if self.client_open(connection_id) {
            self.publish_effect(&pending.effect, committed);
        }
        self.finish_durable(connection_id, adopted);
    }

    fn queue_reconcile(
        &mut self,
        connection_id: ConnectionId,
        revision: u64,
        reserved: Vec<purgatory_common::ItemInstanceId>,
    ) {
        let Some(character_id) = self
            .bindings
            .get(&connection_id)
            .and_then(|binding| binding.character_id)
        else {
            self.release_reserved(&reserved);
            self.abandon_staged(connection_id);
            return;
        };
        self.reconcile_reserved.insert(connection_id, reserved);
        self.reconcile_outbound.push(ReconcileJob {
            connection_id,
            character_id,
            revision,
        });
    }

    pub fn take_reconcile_jobs(&mut self) -> Vec<(ConnectionId, CharacterId, u64)> {
        std::mem::take(&mut self.reconcile_outbound)
            .into_iter()
            .map(|job| (job.connection_id, job.character_id, job.revision))
            .collect()
    }

    /// Replace the live character with the committed database snapshot.
    /// Success does not acknowledge the original request. Failure leaves the
    /// character reserved and sends no success.
    pub fn complete_reconcile(
        &mut self,
        connection_id: ConnectionId,
        revision: u64,
        restore: purgatory_persistence::OwnedRestore,
    ) -> bool {
        let matches = self.bindings.get(&connection_id).is_some_and(|binding| {
            binding.character_id == Some(restore.character.character_id)
        });
        if !matches {
            return false;
        }
        let actor = self.bindings.get(&connection_id).map(|binding| binding.entity);
        let Some(actor) = actor else {
            return false;
        };
        self.clear_live_character(actor);
        if self.apply_durable_restore(connection_id, &restore).is_err() {
            self.clear_live_character(actor);
            return false;
        }
        self.reconcile_outbound
            .retain(|job| job.connection_id != connection_id);
        if let Some(reserved) = self.reconcile_reserved.remove(&connection_id) {
            self.release_reserved(&reserved);
        }
        self.finish_durable(connection_id, revision);
        true
    }

    fn clear_live_character(&mut self, actor: EntityId) {
        for id in self.world.clear_character_durable_runtime(actor) {
            self.durable_items.remove(&id);
        }
        self.narrative.reset_actor(actor);
    }

    fn apply_effect(
        &mut self,
        effect: &super::durable_play::DurableEffect,
        committed: &purgatory_persistence::DurableCommandResult,
    ) -> bool {
        match effect {
            super::durable_play::DurableEffect::Drop {
                connection_id, item, ..
            } => {
                let Some(actor) = self.bindings.get(connection_id).map(|binding| binding.entity)
                else {
                    return false;
                };
                self.apply_drop(*connection_id, *item).is_ok() && {
                    let _ = actor;
                    true
                }
            }
            super::durable_play::DurableEffect::Pickup {
                connection_id,
                item,
                slot,
                durable,
                definition,
                quantity,
                stack_limit,
                ..
            } => self.apply_pickup_commit(
                *connection_id,
                PickupCommit {
                    item: *item,
                    slot: *slot,
                    durable: *durable,
                    definition: *definition,
                    quantity: *quantity,
                    stack_limit: *stack_limit,
                },
                committed,
            ),
            super::durable_play::DurableEffect::Equip {
                connection_id,
                item,
                slot,
                ..
            } => self
                .bindings
                .get(connection_id)
                .map(|binding| binding.entity)
                .is_some_and(|actor| self.world.equip_item(actor, *item, *slot).is_ok()),
            super::durable_play::DurableEffect::Unequip {
                connection_id, slot, ..
            } => self
                .bindings
                .get(connection_id)
                .map(|binding| binding.entity)
                .is_some_and(|actor| self.world.unequip_item(actor, *slot).is_ok()),
            super::durable_play::DurableEffect::RetireGround { item, .. } => {
                self.durable_items.remove(item);
                let _ = self.world.destroy_world_drop_item(*item);
                true
            }
            super::durable_play::DurableEffect::Dialogue {
                connection_id,
                plan,
                beat_id,
                gives,
                retires,
            } => {
                let matches_beat = self
                    .registry
                    .npc_dialogue_by_id(plan.accepted.npc_content_id)
                    .and_then(|dialogue| dialogue.beat(plan.accepted.beat_index))
                    .is_some_and(|beat| beat.id == *beat_id);
                matches_beat
                    && self.apply_dialogue_commit(*connection_id, plan, gives, retires, committed)
            }
            super::durable_play::DurableEffect::Heard {
                connection_id,
                npc_content_id,
                beat_index,
                ..
            } => {
                let Some(actor) = self.bindings.get(connection_id).map(|binding| binding.entity)
                else {
                    return false;
                };
                self.narrative
                    .mark_dialogue_heard(actor, *npc_content_id, *beat_index);
                true
            }
        }
    }

    fn apply_pickup_commit(
        &mut self,
        connection_id: ConnectionId,
        pickup: PickupCommit,
        committed: &purgatory_persistence::DurableCommandResult,
    ) -> bool {
        let PickupCommit {
            item,
            slot,
            durable,
            definition,
            quantity,
            stack_limit,
        } = pickup;
        let Some(actor) = self.bindings.get(&connection_id).map(|binding| binding.entity) else {
            return false;
        };
        if durable {
            let Some(entity) = self.world.world_drop_entity_for_item(item) else {
                return false;
            };
            if self.first_free_inventory_slot(actor) == Some(slot) {
                return self.world.pickup_world_drop(actor, entity).is_ok();
            }
            if !self.world.destroy_world_drop_item(item) {
                return false;
            }
            return self
                .world
                .restore_inventory_item(actor, item, definition, quantity, stack_limit, slot)
                .is_ok();
        }
        let Some(minted) = committed.minted_item_ids.first().copied() else {
            return false;
        };
        if !self.world.destroy_world_drop_item(item) {
            return false;
        }
        let restored = self
            .world
            .restore_inventory_item(actor, minted, definition, quantity, stack_limit, slot)
            .is_ok();
        if restored {
            self.durable_items.insert(minted);
        }
        restored
    }

    fn apply_dialogue_commit(
        &mut self,
        connection_id: ConnectionId,
        plan: &super::dialogue::ChoicePlan,
        gives: &[(ContentId, u32, u32, u16)],
        retires: &[purgatory_common::ItemInstanceId],
        committed: &purgatory_persistence::DurableCommandResult,
    ) -> bool {
        let Some(actor) = self.bindings.get(&connection_id).map(|binding| binding.entity) else {
            return false;
        };
        if committed.minted_item_ids.len() != gives.len() {
            return false;
        }
        for item in retires {
            if !self.world.retire_inventory_instance(actor, *item) {
                return false;
            }
            self.durable_items.remove(item);
        }
        for (minted, (definition, quantity, stack_limit, slot)) in
            committed.minted_item_ids.iter().zip(gives.iter())
        {
            if self
                .world
                .restore_inventory_item(
                    actor,
                    *minted,
                    *definition,
                    *quantity,
                    *stack_limit,
                    *slot,
                )
                .is_err()
            {
                return false;
            }
            self.durable_items.insert(*minted);
        }
        for ability in plan
            .actions
            .iter()
            .filter_map(|action| match action {
                purgatory_content::DialogueAction::GrantAbility { ability_authored } => {
                    self.registry.ability(ability_authored).map(|ability| ability.id)
                }
                _ => None,
            })
            .collect::<Vec<_>>()
        {
            let _ = self.world.grant_learned_ability(actor, ability);
        }
        for action in &plan.actions {
            match action {
                purgatory_content::DialogueAction::SetFact { fact, value } => {
                    self.narrative.set_fact(actor, fact, *value);
                }
                purgatory_content::DialogueAction::MarkNpcMet { npc_authored } => {
                    self.narrative.mark_npc_met(actor, npc_authored);
                }
                _ => {}
            }
        }
        self.narrative.mark_dialogue_heard(
            actor,
            plan.accepted.npc_content_id,
            plan.accepted.beat_index,
        );
        true
    }

    fn publish_effect(
        &mut self,
        effect: &super::durable_play::DurableEffect,
        _committed: &purgatory_persistence::DurableCommandResult,
    ) {
        match effect {
            super::durable_play::DurableEffect::Drop {
                connection_id, seq, ..
            } => {
                let event = ServerItem::DropAccepted { seq: *seq };
                let tx = self
                    .bindings
                    .get(connection_id)
                    .and_then(|binding| binding.interact.clone());
                if let Some(binding) = self.bindings.get_mut(connection_id) {
                    binding.last_drop_result = Some(event);
                }
                Self::send_pickup_result(tx.as_ref(), event);
                self.send_inventory_snapshot(*connection_id);
            }
            super::durable_play::DurableEffect::Pickup {
                connection_id,
                seq,
                item,
                slot,
                durable,
                ..
            } => {
                let shown = if *durable {
                    *item
                } else {
                    self.bindings
                        .get(connection_id)
                        .and_then(|binding| {
                            self.world
                                .inventory_snapshot(binding.entity)
                                .into_iter()
                                .find(|(found, _, _)| *found == *slot)
                                .map(|(_, id, _)| id)
                        })
                        .unwrap_or(*item)
                };
                let event = ServerItem::PickupAccepted {
                    seq: *seq,
                    item_instance_id: shown,
                    slot: *slot,
                };
                let tx = self
                    .bindings
                    .get(connection_id)
                    .and_then(|binding| binding.interact.clone());
                if let Some(binding) = self.bindings.get_mut(connection_id) {
                    binding.last_pickup_result = Some(event);
                }
                Self::send_pickup_result(tx.as_ref(), event);
                self.send_inventory_snapshot(*connection_id);
            }
            super::durable_play::DurableEffect::Equip {
                connection_id, seq, ..
            }
            | super::durable_play::DurableEffect::Unequip {
                connection_id, seq, ..
            } => {
                let event = ServerEquipment::Accepted { seq: *seq };
                let tx = self
                    .bindings
                    .get(connection_id)
                    .and_then(|binding| binding.interact.clone());
                if let Some(binding) = self.bindings.get_mut(connection_id) {
                    binding.last_equipment_result = Some(event);
                }
                Self::send_equipment_result(tx.as_ref(), event);
                self.send_inventory_snapshot(*connection_id);
                self.send_ability_grants(*connection_id);
            }
            super::durable_play::DurableEffect::Dialogue {
                connection_id, plan, ..
            } => {
                let Some(actor) = self.bindings.get(connection_id).map(|binding| binding.entity)
                else {
                    return;
                };
                let tx = self
                    .bindings
                    .get(connection_id)
                    .and_then(|binding| binding.interact.clone());
                let result = self.dialogues.commit_choice(plan.clone());
                match result {
                    ChoiceResult::Continue {
                        accepted,
                        choice_index,
                        next,
                    } => {
                        if let Some(tx) = tx {
                            let _ = tx.try_send(ServerControl::DialogueChoiceAccepted(
                                ServerDialogueChoiceAccepted {
                                    session_id: accepted.session_id,
                                    beat_index: accepted.beat_index.raw(),
                                    choice_index,
                                },
                            ));
                            let _ = tx.try_send(ServerControl::DialogueLine(dialogue_line(next)));
                        }
                    }
                    ChoiceResult::Complete {
                        accepted,
                        choice_index,
                    } => {
                        if let Some(tx) = tx {
                            let _ = tx.try_send(ServerControl::DialogueChoiceAccepted(
                                ServerDialogueChoiceAccepted {
                                    session_id: accepted.session_id,
                                    beat_index: accepted.beat_index.raw(),
                                    choice_index,
                                },
                            ));
                            let _ = self.world.close_interaction(
                                actor,
                                purgatory_simulation::InteractionSessionId(accepted.session_id),
                            );
                            self.finish_dialogue(actor);
                            let _ = tx.try_send(ServerControl::Interact(ServerInteract::Closed {
                                session_id: accepted.session_id,
                                reason: InteractCloseReason::Requested,
                            }));
                        }
                    }
                    ChoiceResult::Invalid => {}
                }
                self.send_inventory_snapshot(*connection_id);
                self.send_ability_grants(*connection_id);
            }
            super::durable_play::DurableEffect::Heard {
                connection_id,
                session_id,
                ..
            } => {
                let Some(actor) = self.bindings.get(connection_id).map(|binding| binding.entity)
                else {
                    return;
                };
                let tx = self
                    .bindings
                    .get(connection_id)
                    .and_then(|binding| binding.interact.clone());
                let _ = self.world.close_interaction(
                    actor,
                    purgatory_simulation::InteractionSessionId(*session_id),
                );
                self.finish_dialogue(actor);
                if let Some(tx) = tx {
                    let _ = tx.try_send(ServerControl::Interact(ServerInteract::Closed {
                        session_id: *session_id,
                        reason: InteractCloseReason::Requested,
                    }));
                }
            }
            super::durable_play::DurableEffect::RetireGround { .. } => {}
        }
    }

    fn stage_drop(&mut self, connection_id: ConnectionId, request: DropRequest) {
        let Some(actor) = self.command_actor(connection_id, CommandClass::Drop).ok() else {
            self.reject_drop(connection_id, request.seq, DropRejectReason::StateBlocked);
            return;
        };
        if !self
            .world
            .inventory_contains(actor, request.item_instance_id)
        {
            self.reject_drop(
                connection_id,
                request.seq,
                DropRejectReason::ItemNotInInventory,
            );
            return;
        }
        if !self.durable_items.contains(&request.item_instance_id)
            || self.reserved_items.contains(&request.item_instance_id)
        {
            self.reject_drop(connection_id, request.seq, DropRejectReason::InvalidRequest);
            return;
        }
        let Some((_, character_id, revision, lease)) = self.durable_context(connection_id) else {
            self.reject_drop(connection_id, request.seq, DropRejectReason::StateBlocked);
            return;
        };
        if !self.begin_durable(connection_id) {
            self.reject_drop(connection_id, request.seq, DropRejectReason::StateBlocked);
            return;
        }
        let command = super::durable_play::drop_command(
            character_id,
            revision,
            request.item_instance_id,
        );
        self.note_inflight_seq(connection_id, InFlight::Drop, request.seq);
        self.enqueue_durable(DurablePending {
            command,
            lease: Some(lease),
            effect: super::durable_play::DurableEffect::Drop {
                connection_id,
                seq: request.seq,
                item: request.item_instance_id,
            },
            reserved: vec![request.item_instance_id],
        });
    }

    fn stage_pickup(&mut self, connection_id: ConnectionId, request: PickupRequest) {
        let Some(actor) = self
            .command_actor(connection_id, CommandClass::Pickup)
            .ok()
        else {
            self.reject_pickup(connection_id, request.seq, PickupRejectReason::StateBlocked);
            return;
        };
        let target = super::snapshot::from_wire_id(request.target);
        let Some(item) = self.world.item_instance_at_world_drop(target) else {
            self.reject_pickup(connection_id, request.seq, PickupRejectReason::TargetMissing);
            return;
        };
        if self.reserved_items.contains(&item) {
            self.reject_pickup(connection_id, request.seq, PickupRejectReason::StateBlocked);
            return;
        }
        let Some(actor_position) = self.world.transform_of(actor).map(|transform| transform.position)
        else {
            self.reject_pickup(connection_id, request.seq, PickupRejectReason::TargetMissing);
            return;
        };
        let Some(target_position) = self
            .world
            .transform_of(target)
            .map(|transform| transform.position)
        else {
            self.reject_pickup(connection_id, request.seq, PickupRejectReason::TargetMissing);
            return;
        };
        if self.world.address_of(actor) != self.world.address_of(target) {
            self.reject_pickup(connection_id, request.seq, PickupRejectReason::WrongAddress);
            return;
        }
        let dx = actor_position[0] - target_position[0];
        let dy = actor_position[1] - target_position[1];
        if dx * dx + dy * dy > purgatory_simulation::INTERACT_RANGE * purgatory_simulation::INTERACT_RANGE
        {
            self.reject_pickup(connection_id, request.seq, PickupRejectReason::OutOfRange);
            return;
        }
        let Some(slot) = self.first_free_inventory_slot(actor) else {
            self.reject_pickup(connection_id, request.seq, PickupRejectReason::InventoryFull);
            return;
        };
        let Some(record) = self.world.item_record(item) else {
            self.reject_pickup(connection_id, request.seq, PickupRejectReason::TargetMissing);
            return;
        };
        let Some((_, character_id, revision, lease)) = self.durable_context(connection_id) else {
            self.reject_pickup(connection_id, request.seq, PickupRejectReason::StateBlocked);
            return;
        };
        let durable = self.durable_items.contains(&item);
        let stack_limit = self
            .registry
            .item_by_id(record.definition)
            .map(|item| item.stack_limit)
            .unwrap_or(record.quantity);
        if !self.begin_durable(connection_id) {
            self.reject_pickup(connection_id, request.seq, PickupRejectReason::StateBlocked);
            return;
        }
        let command = if durable {
            super::durable_play::pickup_command(character_id, revision, item, slot)
        } else {
            super::durable_play::pickup_place_command(
                character_id,
                revision,
                item,
                record.definition,
                record.quantity,
                slot,
            )
        };
        self.note_inflight_seq(connection_id, InFlight::Pickup, request.seq);
        self.enqueue_durable(DurablePending {
            command,
            lease: Some(lease),
            effect: super::durable_play::DurableEffect::Pickup {
                connection_id,
                seq: request.seq,
                item,
                slot,
                durable,
                definition: record.definition,
                quantity: record.quantity,
                stack_limit,
            },
            reserved: vec![item],
        });
    }

    fn stage_equipment(
        &mut self,
        connection_id: ConnectionId,
        seq: u32,
        slot_raw: u8,
        item_instance_id: Option<purgatory_common::ItemInstanceId>,
    ) {
        let Some(actor) = self
            .command_actor(connection_id, CommandClass::Equipment)
            .ok()
        else {
            self.reject_equipment(connection_id, seq, EquipmentRejectReason::StateBlocked);
            return;
        };
        if self.world.kind(actor) != Some(EntityKind::Player) {
            self.reject_equipment(connection_id, seq, EquipmentRejectReason::InvalidRequest);
            return;
        }
        let Some(slot) = EquipmentSlot::from_u8(slot_raw) else {
            self.reject_equipment(connection_id, seq, EquipmentRejectReason::InvalidRequest);
            return;
        };
        let Some((_, character_id, revision, lease)) = self.durable_context(connection_id) else {
            self.reject_equipment(connection_id, seq, EquipmentRejectReason::StateBlocked);
            return;
        };
        let (command, effect, reserved) = if let Some(item) = item_instance_id {
            let Some(record) = self.world.item_record(item) else {
                self.reject_equipment(connection_id, seq, EquipmentRejectReason::InvalidRequest);
                return;
            };
            if !self.world.inventory_contains(actor, item)
                || !self.durable_items.contains(&item)
                || self.reserved_items.contains(&item)
            {
                self.reject_equipment(connection_id, seq, EquipmentRejectReason::InvalidRequest);
                return;
            }
            match authorize_equip(&self.registry, slot, record.definition) {
                Ok(()) => {}
                Err(EquipmentAuthError::UnknownContent) => {
                    self.reject_equipment(connection_id, seq, EquipmentRejectReason::UnknownContent);
                    return;
                }
                Err(EquipmentAuthError::SlotMismatch) => {
                    self.reject_equipment(connection_id, seq, EquipmentRejectReason::SlotMismatch);
                    return;
                }
            }
            let inventory_slot = self.inventory_slot_of(item).unwrap_or(0);
            let displaced = self.world.equipped_instance(actor, slot).map(|equipped| {
                (equipped, inventory_slot)
            });
            if let Some((equipped, _)) = displaced
                && (!self.durable_items.contains(&equipped) || self.reserved_items.contains(&equipped))
            {
                self.reject_equipment(connection_id, seq, EquipmentRejectReason::InvalidRequest);
                return;
            }
            let mut reserved = vec![item];
            if let Some((equipped, _)) = displaced {
                reserved.push(equipped);
            }
            (
                super::durable_play::equip_command(character_id, revision, item, slot, displaced),
                super::durable_play::DurableEffect::Equip {
                    connection_id,
                    seq,
                    item,
                    slot,
                },
                reserved,
            )
        } else {
            let Some(item) = self.world.equipped_instance(actor, slot) else {
                self.reject_equipment(connection_id, seq, EquipmentRejectReason::StateBlocked);
                return;
            };
            if !self.durable_items.contains(&item) || self.reserved_items.contains(&item) {
                self.reject_equipment(connection_id, seq, EquipmentRejectReason::InvalidRequest);
                return;
            }
            let Some(inventory_slot) = self.first_free_inventory_slot(actor) else {
                self.reject_equipment(connection_id, seq, EquipmentRejectReason::StateBlocked);
                return;
            };
            (
                super::durable_play::unequip_command(
                    character_id,
                    revision,
                    item,
                    slot,
                    inventory_slot,
                ),
                super::durable_play::DurableEffect::Unequip {
                    connection_id,
                    seq,
                    slot,
                },
                vec![item],
            )
        };
        if !self.begin_durable(connection_id) {
            self.reject_equipment(connection_id, seq, EquipmentRejectReason::StateBlocked);
            return;
        }
        self.note_inflight_seq(connection_id, InFlight::Equipment, seq);
        self.enqueue_durable(DurablePending {
            command,
            lease: Some(lease),
            effect,
            reserved,
        });
    }

    fn whole_stacks(
        &self,
        actor: EntityId,
        definition: ContentId,
        mut quantity: u32,
    ) -> Option<Vec<purgatory_common::ItemInstanceId>> {
        let mut stacks: Vec<(u16, purgatory_common::ItemInstanceId, u32)> = self
            .world
            .inventory_snapshot(actor)
            .into_iter()
            .filter_map(|(slot, id, record)| {
                (record.definition == definition).then_some((slot, id, record.quantity))
            })
            .collect();
        stacks.sort_by_key(|(slot, _, _)| *slot);
        let mut chosen = Vec::new();
        for (_, id, stack_quantity) in stacks {
            if stack_quantity <= quantity && !self.reserved_items.contains(&id) {
                quantity -= stack_quantity;
                chosen.push(id);
            }
            if quantity == 0 {
                break;
            }
        }
        (quantity == 0).then_some(chosen)
    }

    fn stage_dialogue(
        &mut self,
        connection_id: ConnectionId,
        plan: super::dialogue::ChoicePlan,
        beat_id: Option<String>,
    ) {
        let Some(beat_id) = beat_id.filter(|id| !id.is_empty()) else {
            return;
        };
        let Some(actor) = self.bindings.get(&connection_id).map(|binding| binding.entity) else {
            return;
        };
        let Some((_, character_id, revision, lease)) = self.durable_context(connection_id) else {
            return;
        };
        let preview = match preview_dialogue_actions(&plan.actions, actor, &self.registry, &self.world)
        {
            Ok(preview) => preview,
            Err(error) => {
                println!(
                    "N10_DIALOGUE action rejected actor={actor} error={error:?}"
                );
                return;
            }
        };
        let mut retires = Vec::new();
        for (definition, quantity) in &preview.removes {
            let Some(stacks) = self.whole_stacks(actor, *definition, *quantity) else {
                println!(
                    "N10_DIALOGUE action rejected actor={actor} error=partial stack is not a durable retire"
                );
                return;
            };
            for item in stacks {
                if !self.durable_items.contains(&item) {
                    println!(
                        "N10_DIALOGUE action rejected actor={actor} error=item is not durable"
                    );
                    return;
                }
                retires.push(item);
            }
        }
        let mut occupied: HashSet<u16> = self
            .world
            .inventory_snapshot(actor)
            .into_iter()
            .filter(|(_, id, _)| !retires.contains(id))
            .map(|(slot, _, _)| slot)
            .collect();
        let mut gives = Vec::new();
        for (definition, quantity, stack_limit) in preview.gives {
            let Some(slot) = (0..purgatory_simulation::INVENTORY_CAPACITY as u16)
                .find(|slot| !occupied.contains(slot))
            else {
                println!("N10_DIALOGUE action rejected actor={actor} error=inventory full");
                return;
            };
            occupied.insert(slot);
            gives.push((definition, quantity, stack_limit, slot));
        }
        if !self.begin_durable(connection_id) {
            return;
        }
        let command = super::durable_play::dialogue_command(super::durable_play::DialogueCommandParts {
            character_id,
            revision,
            npc_content_id: plan.accepted.npc_content_id,
            beat_id: beat_id.clone(),
            choice_index: plan.choice_index,
            places: gives
                .iter()
                .map(|(definition, quantity, _, slot)| purgatory_persistence::PlaceNewItem {
                    owner: character_id,
                    definition_content_id: *definition,
                    quantity: *quantity,
                    location: purgatory_persistence::CharacterItemLocation::Inventory { slot: *slot },
                })
                .collect(),
            retire: retires.clone(),
            facts: preview.facts,
            npcs_met: preview.npcs_met,
            learned: preview.abilities,
        });
        if command.key.len() > 128 {
            self.abandon_staged(connection_id);
            return;
        }
        self.enqueue_durable(DurablePending {
            command,
            lease: Some(lease),
            effect: super::durable_play::DurableEffect::Dialogue {
                connection_id,
                plan,
                beat_id,
                gives,
                retires: retires.clone(),
            },
            reserved: retires,
        });
    }

    fn stage_heard(
        &mut self,
        connection_id: ConnectionId,
        active: ActiveDialogue,
        beat_id: Option<String>,
    ) {
        let Some(beat_id) = beat_id.filter(|id| !id.is_empty()) else {
            return;
        };
        let Some((_, character_id, revision, lease)) = self.durable_context(connection_id) else {
            return;
        };
        if !self.begin_durable(connection_id) {
            return;
        }
        let command = super::durable_play::heard_command(
            character_id,
            revision,
            active.npc_content_id,
            &beat_id,
        );
        self.enqueue_durable(DurablePending {
            command,
            lease: Some(lease),
            effect: super::durable_play::DurableEffect::Heard {
                connection_id,
                npc_content_id: active.npc_content_id,
                beat_index: active.beat_index,
                session_id: active.session_id,
            },
            reserved: Vec::new(),
        });
    }

    /// Retire one live ground item. Ordinary unclaimed ground is not restored.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn stage_ground_expiry(
        &mut self,
        connection_id: ConnectionId,
        item: purgatory_common::ItemInstanceId,
    ) -> bool {
        if !self.stages_durable(connection_id) || self.reserved_items.contains(&item) {
            return false;
        }
        if self.world.world_drop_entity_for_item(item).is_none() {
            return false;
        }
        if !self.begin_durable(connection_id) {
            return false;
        }
        self.enqueue_durable(DurablePending {
            command: super::durable_play::retire_command(item),
            lease: None,
            effect: super::durable_play::DurableEffect::RetireGround {
                connection_id,
                item,
            },
            reserved: vec![item],
        });
        true
    }

    #[cfg(test)]
    pub fn lease_for_test(
        &mut self,
        connection_id: ConnectionId,
        character_id: CharacterId,
        revision: u64,
        login: &str,
        generation: u64,
        items: &[purgatory_common::ItemInstanceId],
    ) {
        let login = purgatory_common::DevLogin::parse(login).expect("test login");
        self.set_authority(
            connection_id,
            purgatory_persistence::LeaseAuthority {
                login,
                character_id,
                generation,
            },
        );
        if let Some(binding) = self.bindings.get_mut(&connection_id) {
            binding.character_id = Some(character_id);
            binding.committed_revision = revision;
            binding.persistence_revision = revision;
        }
        self.occupancy.insert(character_id, connection_id);
        for item in items {
            self.durable_items.insert(*item);
        }
    }

}
