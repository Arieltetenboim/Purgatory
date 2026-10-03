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
        self.adopt_snapshot_reports();
        self.promote_due_ground();
        self.promote_due_retries(std::time::Instant::now());
        let tokens = std::mem::take(&mut self.durable_outbound);
        let mut held = Vec::new();
        let mut ready = Vec::new();
        for token in tokens {
            let blocked = self
                .durable_pending
                .get(&token)
                .is_some_and(|pending| self.snapshot_blocks_command(&pending.command));
            if blocked {
                held.push(token);
            } else {
                ready.push(token);
            }
        }
        self.durable_outbound.append(&mut held);
        ready
            .into_iter()
            .filter_map(|token| {
                self.align_stored_command(token);
                let pending = self.durable_pending.get(&token)?;
                Some(super::durable_play::DurableSubmit {
                    token,
                    command: pending.command.clone(),
                    lease: pending.lease.clone(),
                })
            })
            .collect()
    }

    /// A snapshot handoff whose revision is still above the committed revision
    /// has not been confirmed. The command waits so it is not queued behind a
    /// write that will change the revision it expects.
    fn snapshot_blocks_command(&self, command: &purgatory_persistence::DurableCommand) -> bool {
        command
            .expected_revisions
            .iter()
            .any(|(character_id, _)| self.snapshot_ahead(*character_id))
    }

    fn snapshot_ahead(&self, character_id: CharacterId) -> bool {
        let Some(connection_id) = self.occupancy.get(&character_id) else {
            return false;
        };
        let Some(binding) = self.bindings.get(connection_id) else {
            return false;
        };
        let committed = binding.committed_revision;
        binding
            .snapshots
            .deferred
            .is_some_and(|revision| revision > committed)
            || binding
                .snapshots
                .queued
                .iter()
                .any(|revision| *revision > committed)
    }

    /// Raise a command that is still local to the revision adopted since it
    /// was built. A command already given to the worker is not rewritten.
    fn align_stored_command(&mut self, token: u64) {
        let Some(pending) = self.durable_pending.get(&token) else {
            return;
        };
        let updates: Vec<(CharacterId, u64, u64)> = pending
            .command
            .expected_revisions
            .iter()
            .filter_map(|(character_id, old)| {
                let connection_id = self.occupancy.get(character_id)?;
                let committed = self.bindings.get(connection_id)?.committed_revision;
                (committed > *old).then_some((*character_id, *old, committed))
            })
            .collect();
        let Some(pending) = self.durable_pending.get_mut(&token) else {
            return;
        };
        for (character_id, old, committed) in updates {
            if let Some(slot) = pending
                .command
                .expected_revisions
                .iter_mut()
                .find(|(id, _)| *id == character_id)
            {
                slot.1 = committed;
            }
            let prefix = format!("c{}-r{old}-", character_id.raw());
            if let Some(rest) = pending.command.key.strip_prefix(&prefix) {
                pending.command.key = format!("c{}-r{}-{rest}", character_id.raw(), committed);
            }
        }
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
            let connection_id = self
                .durable_pending
                .get(&token)
                .and_then(|pending| effect_connection(&pending.effect));
            if let Some(connection_id) = connection_id
                && let Some(binding) = self.bindings.get_mut(&connection_id)
            {
                binding.commit_uncertain = true;
            }
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
        let super::durable_play::DurableEffect::RetireGround { item } = &pending.effect else {
            let Some(connection_id) = effect_connection(&pending.effect) else {
                return;
            };
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
            return;
        };
        self.restore_unretired_ground(*item);
    }

    fn apply_committed(
        &mut self,
        pending: DurablePending,
        committed: &purgatory_persistence::DurableCommandResult,
    ) {
        if matches!(
            pending.effect,
            super::durable_play::DurableEffect::RetireGround { .. }
        ) {
            let applied = self.apply_effect(&pending.effect, committed);
            self.release_reserved(&pending.reserved);
            if !applied && let super::durable_play::DurableEffect::RetireGround { item } = pending.effect {
                self.restore_unretired_ground(item);
            }
            return;
        }
        let Some(connection_id) = effect_connection(&pending.effect) else {
            return;
        };
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
            self.queue_reconcile(connection_id, adopted, pending.reserved, pending.effect);
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
        effect: super::durable_play::DurableEffect,
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
        if let Some(binding) = self.bindings.get_mut(&connection_id) {
            binding.reconcile_required = true;
        }
        self.reconcile_effects.insert(connection_id, effect);
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
    /// Success tells the client the committed drop, pickup, or equipment
    /// outcome. It does not describe that outcome as a rollback. Failure leaves
    /// player control stopped and sends no rejection.
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
        if !self.manifest_reconciled_drop(connection_id, actor) {
            self.reconcile_outbound.push(ReconcileJob {
                connection_id,
                character_id: restore.character.character_id,
                revision,
            });
            if let Some(binding) = self.bindings.get_mut(&connection_id) {
                binding.reconcile_required = true;
            }
            return false;
        }
        if let Some(binding) = self.bindings.get_mut(&connection_id) {
            binding.reconcile_required = false;
        }
        self.acknowledge_reconciled(connection_id);
        self.finish_durable(connection_id, revision);
        true
    }

    /// A committed drop is visible and timed before the client is told it succeeded.
    /// The original instance id is kept. Failure leaves control blocked and sends nothing.
    fn manifest_reconciled_drop(
        &mut self,
        connection_id: ConnectionId,
        actor: EntityId,
    ) -> bool {
        let Some(effect) = self.reconcile_effects.get(&connection_id).cloned() else {
            return true;
        };
        let super::durable_play::DurableEffect::Drop {
            item,
            definition,
            quantity,
            stack_limit,
            ..
        } = effect
        else {
            return true;
        };
        if self.world.world_drop_entity_for_item(item).is_none() {
            let Some(address) = self.world.address_of(actor) else {
                return false;
            };
            let Some(position) = self.world.transform_of(actor).map(|value| value.position) else {
                return false;
            };
            if self
                .world
                .manifest_committed_world_drop(
                    item,
                    address,
                    position,
                    definition,
                    quantity,
                    stack_limit,
                )
                .is_err()
            {
                return false;
            }
        }
        self.durable_items.insert(item);
        self.note_player_ground(item);
        self.world.world_drop_entity_for_item(item).is_some()
    }

    fn acknowledge_reconciled(&mut self, connection_id: ConnectionId) {
        let Some(effect) = self.reconcile_effects.remove(&connection_id) else {
            return;
        };
        match &effect {
            super::durable_play::DurableEffect::Drop { .. }
            | super::durable_play::DurableEffect::Pickup { .. }
            | super::durable_play::DurableEffect::Equip { .. }
            | super::durable_play::DurableEffect::Unequip { .. }
            | super::durable_play::DurableEffect::Dialogue { .. } => {
                self.publish_effect(
                    &effect,
                    &purgatory_persistence::DurableCommandResult {
                        revisions: Vec::new(),
                        minted_item_ids: Vec::new(),
                    },
                );
            }
            super::durable_play::DurableEffect::Heard { .. }
            | super::durable_play::DurableEffect::RetireGround { .. } => {}
        }
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
            super::durable_play::DurableEffect::Drop { connection_id, item, .. } => {
                let applied = self
                    .bindings
                    .get(connection_id)
                    .map(|binding| binding.entity)
                    .is_some()
                    && self.apply_drop(*connection_id, *item).is_ok();
                if applied {
                    self.note_player_ground(*item);
                }
                applied
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
            super::durable_play::DurableEffect::RetireGround { item } => {
                self.forget_ground_item(*item);
                self.durable_items.remove(item);
                self.reserved_visible.remove(item);
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
                let picked = self.world.pickup_world_drop(actor, entity).is_ok();
                if picked {
                    self.note_picked_reserved(item);
                }
                return picked;
            }
            if !self.world.destroy_world_drop_item(item) {
                return false;
            }
            let restored = self
                .world
                .restore_inventory_item(actor, item, definition, quantity, stack_limit, slot)
                .is_ok();
            if restored {
                self.note_picked_reserved(item);
            }
            return restored;
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
            self.forget_ground_item(item);
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
                        if let Some(tx) = &tx {
                            let _ = tx.try_send(ServerControl::DialogueChoiceAccepted(
                                ServerDialogueChoiceAccepted {
                                    session_id: accepted.session_id,
                                    beat_index: accepted.beat_index.raw(),
                                    choice_index,
                                },
                            ));
                        }
                        let _ = self.world.close_interaction(
                            actor,
                            purgatory_simulation::InteractionSessionId(accepted.session_id),
                        );
                        self.finish_dialogue(actor);
                        if let Some(tx) = &tx {
                            let _ = tx.try_send(ServerControl::Interact(ServerInteract::Closed {
                                session_id: accepted.session_id,
                                reason: InteractCloseReason::Requested,
                            }));
                        }
                    }
                    ChoiceResult::Invalid => {
                        if self.dialogues.active(actor) == Some(plan.accepted) {
                            let session_id = plan.accepted.session_id;
                            let _ = self.world.close_interaction(
                                actor,
                                purgatory_simulation::InteractionSessionId(session_id),
                            );
                            self.finish_dialogue(actor);
                            if let Some(tx) = &tx {
                                let _ = tx.try_send(ServerControl::Interact(ServerInteract::Closed {
                                    session_id,
                                    reason: InteractCloseReason::Requested,
                                }));
                            }
                        }
                    }
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
        self.adopt_snapshot_reports();
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
        let Some(record) = self.world.item_record(request.item_instance_id) else {
            self.reject_drop(connection_id, request.seq, DropRejectReason::InvalidRequest);
            return;
        };
        let stack_limit = self
            .registry
            .item_by_id(record.definition)
            .map(|item| item.stack_limit)
            .unwrap_or(record.quantity.max(1));
        let definition = record.definition;
        let quantity = record.quantity;
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
                definition,
                quantity,
                stack_limit,
            },
            reserved: vec![request.item_instance_id],
        });
    }

    fn stage_pickup(&mut self, connection_id: ConnectionId, request: PickupRequest) {
        self.adopt_snapshot_reports();
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
        if !self.ground_collectible(connection_id, item) {
            self.reject_pickup(connection_id, request.seq, PickupRejectReason::StateBlocked);
            return;
        }
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
        let keeps_visible_id =
            self.reserved_visible.contains(&item) || self.durable_items.contains(&item);
        let stack_limit = self
            .registry
            .item_by_id(record.definition)
            .map(|item| item.stack_limit)
            .unwrap_or(record.quantity);
        if !self.begin_durable(connection_id) {
            self.reject_pickup(connection_id, request.seq, PickupRejectReason::StateBlocked);
            return;
        }
        let command = if self.reserved_visible.contains(&item) {
            super::durable_play::pickup_reserved_command(
                character_id,
                revision,
                item,
                record.definition,
                record.quantity,
                slot,
            )
        } else if self.durable_items.contains(&item) {
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
                durable: keeps_visible_id,
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
        self.adopt_snapshot_reports();
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
        self.adopt_snapshot_reports();
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
        self.adopt_snapshot_reports();
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

    /// Retire one live ground item without borrowing a character's command slot.
    /// Ordinary unclaimed ground is not restored.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn stage_ground_expiry(
        &mut self,
        _connection_id: ConnectionId,
        item: purgatory_common::ItemInstanceId,
    ) -> bool {
        if Self::deadline_expired(self.channel_deadline) || self.reserved_items.contains(&item) {
            return false;
        }
        if self.world.world_drop_entity_for_item(item).is_none() {
            return false;
        }
        self.unschedule_ground(item);
        if self.reserved_visible.contains(&item) || self.durable_items.contains(&item) {
            self.enqueue_ground_retire(item);
            return true;
        }
        let _ = self.world.destroy_world_drop_item(item);
        self.live_ground.remove(&item);
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
            binding.snapshots.queued.clear();
            binding.snapshots.deferred = None;
        }
        self.occupancy.insert(character_id, connection_id);
        for item in items {
            self.durable_items.insert(*item);
        }
    }

    /// Advance the channel's ground clock. This does not read the database.
    pub fn advance_ground_clock(&mut self, elapsed: Duration) {
        self.ground_elapsed = self.ground_elapsed.saturating_add(elapsed);
    }

    /// Ask the network loop for another PostgreSQL id range. This does not
    /// touch the database. `simulate_tick` must not call it.
    pub fn begin_id_replenish(&mut self) -> Option<u32> {
        if self.id_reserve_unavailable
            || self.id_replenish_inflight
            || self.id_pool.len() >= ID_RESERVE_LOW_WATER
        {
            return None;
        }
        self.id_replenish_inflight = true;
        #[cfg(test)]
        {
            self.id_reserve_requests = self.id_reserve_requests.saturating_add(1);
        }
        Some(ID_RESERVE_BATCH)
    }

    fn install_reserved_ids(&mut self, ids: Vec<purgatory_common::ItemInstanceId>) {
        self.id_replenish_inflight = false;
        for id in ids {
            if id.raw() == 0
                || self.reserved_visible.contains(&id)
                || self.durable_items.contains(&id)
                || self.id_pool.contains(&id)
            {
                continue;
            }
            self.id_pool.push_back(id);
        }
    }

    fn fail_id_replenish(&mut self, err: &purgatory_persistence::PersistError) {
        self.id_replenish_inflight = false;
        if matches!(err, purgatory_persistence::PersistError::Migration { .. }) {
            self.id_reserve_unavailable = true;
        }
    }

    #[cfg(test)]
    pub fn install_reserved_ids_for_test(&mut self, ids: Vec<purgatory_common::ItemInstanceId>) {
        self.install_reserved_ids(ids);
    }

    fn take_reserved_id(&mut self) -> Option<purgatory_common::ItemInstanceId> {
        self.id_pool.pop_front()
    }

    fn note_picked_reserved(&mut self, item: purgatory_common::ItemInstanceId) {
        self.forget_ground_item(item);
        self.reserved_visible.remove(&item);
        self.durable_items.insert(item);
    }

    fn manifest_reserved_drop(
        &mut self,
        address: purgatory_common::WorldAddress,
        position: [f32; 2],
        definition: ContentId,
        quantity: u32,
        stack_limit: u32,
        origin: GroundOrigin,
    ) -> Result<purgatory_common::ItemInstanceId, ItemRuntimeError> {
        let Some(item) = self.take_reserved_id() else {
            return Err(ItemRuntimeError::SpawnFailed);
        };
        match self.world.manifest_committed_world_drop(
            item,
            address,
            position,
            definition,
            quantity,
            stack_limit,
        ) {
            Ok(_) => {
                self.reserved_visible.insert(item);
                self.track_ground(item, origin);
                Ok(item)
            }
            Err(ItemRuntimeError::SpawnFailed) => {
                self.id_pool.push_front(item);
                Err(ItemRuntimeError::SpawnFailed)
            }
            Err(err) => Err(err),
        }
    }

    /// Spawn one world drop whose pickup is limited to `killer` for 40 seconds.
    /// Without a reserved id, nothing is spawned.
    pub fn manifest_monster_loot(
        &mut self,
        killer: CharacterId,
        address: purgatory_common::WorldAddress,
        position: [f32; 2],
        definition: ContentId,
        quantity: u32,
    ) -> Result<purgatory_common::ItemInstanceId, ItemRuntimeError> {
        let stack_limit = self
            .registry
            .item_by_id(definition)
            .map(|item| item.stack_limit)
            .unwrap_or(quantity.max(1));
        self.manifest_reserved_drop(
            address,
            position,
            definition,
            quantity,
            stack_limit,
            GroundOrigin::MonsterLoot { killer },
        )
    }

    fn next_loot_unit(&mut self) -> u32 {
        self.loot_rng.next_u32()
    }

    #[cfg(test)]
    pub fn seed_loot_rng_for_test(&mut self, seed: u64) {
        self.loot_rng = LootRng::from_seed(seed);
    }

    fn loot_address_open(&self, address: WorldAddress) -> bool {
        if self.admission_stopped || Self::deadline_expired(self.channel_deadline) {
            return false;
        }
        if self.closed_loot_addresses.contains(&address) {
            return false;
        }
        self.registry.map_by_map_id(address.map).is_some()
    }

    /// Stop manifesting unclaimed drops for one map membership.
    /// Items already on the ground keep the existing lifetime.
    ///
    /// Production channel shutdown abandons every address through
    /// [`Self::lose_all_authority`]. This method is the single-address hook;
    /// tests are the current caller.
    #[allow(dead_code)]
    pub fn close_world_address(&mut self, address: WorldAddress) {
        self.closed_loot_addresses.insert(address);
        self.flush_pending_monster_loot();
    }

    fn character_of_entity(&self, entity: EntityId) -> Option<CharacterId> {
        self.bindings
            .values()
            .find(|binding| binding.entity == entity)
            .and_then(|binding| binding.character_id)
    }

    fn plan_monster_loot(&mut self, entity: EntityId, killer: Option<EntityId>) {
        let Some(killer) = killer else {
            println!("MONSTER_LOOT none entity={entity} reason=no_lethal_player");
            return;
        };
        let Some(character_id) = self.character_of_entity(killer) else {
            println!("MONSTER_LOOT none entity={entity} reason=no_character");
            return;
        };
        let Some(content_id) = self.world.content_id_of(entity) else {
            return;
        };
        let Some(drops) = self.monster_drop_entries(content_id) else {
            return;
        };
        if drops.is_empty() {
            return;
        }
        let Some(address) = self.world.address_of(entity) else {
            return;
        };
        let Some(transform) = self.world.transform_of(entity) else {
            return;
        };
        let position = [transform.position[0] + 0.35, transform.position[1]];
        let rolled = purgatory_content::roll_monster_drops(&drops, || self.next_loot_unit());
        if rolled.is_empty() {
            return;
        }
        self.enqueue_monster_loot(PendingMonsterLoot {
            killer: character_id,
            address,
            position,
            remaining: rolled,
        });
    }

    fn monster_drop_entries(
        &self,
        content_id: ContentId,
    ) -> Option<Vec<purgatory_content::MonsterDropEntry>> {
        #[cfg(test)]
        if let Some(drops) = self.monster_drop_overrides.get(&content_id) {
            return Some(drops.clone());
        }
        self.registry
            .monster_by_id(content_id)
            .map(|monster| monster.drops.clone())
    }

    #[cfg(test)]
    pub fn set_monster_drops_for_test(
        &mut self,
        content_id: ContentId,
        drops: Vec<purgatory_content::MonsterDropEntry>,
    ) {
        self.monster_drop_overrides.insert(content_id, drops);
    }

    fn enqueue_monster_loot(&mut self, plan: PendingMonsterLoot) {
        if !self.loot_address_open(plan.address) {
            self.abandon_plan(plan, "address_closed");
            return;
        }
        if self.pending_loot.len() >= MAX_PENDING_MONSTER_LOOT {
            self.abandon_plan(plan, "queue_full");
            return;
        }
        self.pending_loot.push_back(plan);
        self.flush_pending_monster_loot();
    }

    fn abandon_plan(&mut self, plan: PendingMonsterLoot, reason: &str) {
        let remaining = plan.remaining.len() as u64;
        self.loot_abandoned = self.loot_abandoned.saturating_add(remaining);
        println!(
            "MONSTER_LOOT abandoned killer={} remaining={remaining} reason={reason}",
            plan.killer
        );
    }

    fn flush_pending_monster_loot(&mut self) {
        let mut index = 0;
        while index < self.pending_loot.len() {
            if !self.loot_address_open(self.pending_loot[index].address) {
                let plan = self.pending_loot.remove(index).expect("index checked");
                self.abandon_plan(plan, "address_closed");
                continue;
            }
            let mut blocked = false;
            while !self.pending_loot[index].remaining.is_empty() {
                let drop = self.pending_loot[index].remaining[0];
                let killer = self.pending_loot[index].killer;
                let address = self.pending_loot[index].address;
                let position = self.pending_loot[index].position;
                match self.manifest_monster_loot(killer, address, position, drop.item, drop.quantity)
                {
                    Ok(item) => {
                        self.pending_loot[index].remaining.remove(0);
                        self.loot_manifested = self.loot_manifested.saturating_add(1);
                        println!(
                            "MONSTER_LOOT manifested item={item} definition={} quantity={} killer={killer}",
                            drop.item, drop.quantity
                        );
                    }
                    Err(ItemRuntimeError::SpawnFailed) => {
                        self.loot_allocation_deferred =
                            self.loot_allocation_deferred.saturating_add(1);
                        println!(
                            "MONSTER_LOOT deferred killer={killer} remaining={} reason=no_reserved_id",
                            self.pending_loot[index].remaining.len()
                        );
                        blocked = true;
                        break;
                    }
                    Err(error) => {
                        self.loot_allocation_deferred =
                            self.loot_allocation_deferred.saturating_add(1);
                        println!(
                            "MONSTER_LOOT deferred killer={killer} remaining={} reason={error:?}",
                            self.pending_loot[index].remaining.len()
                        );
                        blocked = true;
                        break;
                    }
                }
            }
            if self.pending_loot[index].remaining.is_empty() {
                self.pending_loot.remove(index);
            } else if blocked {
                return;
            } else {
                index += 1;
            }
        }
    }

    #[cfg(test)]
    pub fn pending_loot_len(&self) -> usize {
        self.pending_loot.iter().map(|plan| plan.remaining.len()).sum()
    }

    #[cfg(test)]
    pub fn loot_abandoned_count(&self) -> u64 {
        self.loot_abandoned
    }

    #[cfg(test)]
    pub fn monster_loot_killer(
        &self,
        item: purgatory_common::ItemInstanceId,
    ) -> Option<CharacterId> {
        match self.live_ground.get(&item).map(|live| live.origin) {
            Some(GroundOrigin::MonsterLoot { killer }) => Some(killer),
            _ => None,
        }
    }

    fn note_player_ground(&mut self, item: purgatory_common::ItemInstanceId) {
        if self.world.world_drop_entity_for_item(item).is_some() {
            self.track_ground(item, GroundOrigin::PlayerDrop);
        }
    }

    fn track_ground(&mut self, item: purgatory_common::ItemInstanceId, origin: GroundOrigin) {
        let visible_at = self.ground_elapsed;
        let expires_at = visible_at.saturating_add(GROUND_LIFETIME);
        if let Some(previous) = self.live_ground.insert(
            item,
            LiveGround {
                visible_at,
                expires_at,
                origin,
            },
        ) {
            self.remove_expiry_entry(item, previous.expires_at);
        }
        self.ground_expiry.insert((expires_at, item.raw()), ());
    }

    fn ground_collectible(
        &self,
        connection_id: ConnectionId,
        item: purgatory_common::ItemInstanceId,
    ) -> bool {
        let Some(live) = self.live_ground.get(&item) else {
            return true;
        };
        match live.origin {
            GroundOrigin::PlayerDrop => true,
            GroundOrigin::MonsterLoot { killer } => {
                self.ground_elapsed.saturating_sub(live.visible_at) >= GROUND_EXCLUSIVE_WINDOW
                    || self
                        .bindings
                        .get(&connection_id)
                        .and_then(|binding| binding.character_id)
                        == Some(killer)
            }
        }
    }

    fn promote_due_ground(&mut self) {
        if Self::deadline_expired(self.channel_deadline) {
            return;
        }
        self.ground_wake_ops = 0;
        while (self.ground_wake_ops as usize) < GROUND_RETIRE_BATCH {
            let item = if let Some(item) = self.ground_deferred.pop_front() {
                self.ground_wake_ops = self.ground_wake_ops.saturating_add(1);
                if !self.ground_deferred_member.remove(&item) {
                    continue;
                }
                item
            } else if let Some(item) = self.pop_due_ground() {
                self.ground_wake_ops = self.ground_wake_ops.saturating_add(1);
                item
            } else {
                break;
            };
            if self.reserved_items.contains(&item) {
                self.defer_ground(item);
                continue;
            }
            let _ = self.expire_one(item);
        }
    }

    fn defer_ground(&mut self, item: purgatory_common::ItemInstanceId) {
        if self.ground_deferred_member.insert(item) {
            self.ground_deferred.push_back(item);
        }
    }

    fn pop_due_ground(&mut self) -> Option<purgatory_common::ItemInstanceId> {
        let (&(when, raw), _) = self.ground_expiry.iter().next()?;
        if when > self.ground_elapsed {
            return None;
        }
        self.ground_expiry.remove(&(when, raw));
        Some(purgatory_common::ItemInstanceId::from_raw(raw))
    }

    fn expire_one(&mut self, item: purgatory_common::ItemInstanceId) -> bool {
        if self.world.world_drop_entity_for_item(item).is_none() {
            self.live_ground.remove(&item);
            self.reserved_visible.remove(&item);
            return false;
        }
        if self.reserved_visible.contains(&item) || self.durable_items.contains(&item) {
            self.enqueue_ground_retire(item);
            return true;
        }
        let _ = self.world.destroy_world_drop_item(item);
        self.live_ground.remove(&item);
        true
    }

    fn enqueue_ground_retire(&mut self, item: purgatory_common::ItemInstanceId) {
        let command = if self.reserved_visible.contains(&item) {
            let Some(record) = self.world.item_record(item) else {
                let _ = self.world.destroy_world_drop_item(item);
                self.live_ground.remove(&item);
                self.reserved_visible.remove(&item);
                return;
            };
            super::durable_play::reserved_retire_command(item, record.definition, record.quantity)
        } else {
            super::durable_play::retire_command(item)
        };
        self.enqueue_durable(DurablePending {
            command,
            lease: None,
            effect: super::durable_play::DurableEffect::RetireGround { item },
            reserved: vec![item],
        });
    }

    fn restore_unretired_ground(&mut self, item: purgatory_common::ItemInstanceId) {
        if self.world.world_drop_entity_for_item(item).is_none() {
            self.live_ground.remove(&item);
            return;
        }
        let Some(live) = self.live_ground.get_mut(&item) else {
            return;
        };
        let when = self.ground_elapsed.saturating_add(Duration::from_secs(1));
        live.expires_at = when;
        self.ground_expiry.insert((when, item.raw()), ());
    }

    fn unschedule_ground(&mut self, item: purgatory_common::ItemInstanceId) {
        if let Some(live) = self.live_ground.get(&item).copied() {
            self.remove_expiry_entry(item, live.expires_at);
        }
        self.ground_deferred_member.remove(&item);
    }

    fn forget_ground_item(&mut self, item: purgatory_common::ItemInstanceId) {
        self.ground_remove_ops = 0;
        if let Some(live) = self.live_ground.remove(&item) {
            self.remove_expiry_entry(item, live.expires_at);
        }
        self.ground_deferred_member.remove(&item);
    }

    fn remove_expiry_entry(&mut self, item: purgatory_common::ItemInstanceId, expires_at: Duration) {
        if self
            .ground_expiry
            .remove(&(expires_at, item.raw()))
            .is_some()
        {
            self.ground_remove_ops = self.ground_remove_ops.saturating_add(1);
        }
    }
}
