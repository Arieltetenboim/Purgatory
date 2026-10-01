# Phase 12B — Save/Load and lifecycle

Status: **accepted** as the Phase 12B save/load lifecycle (2026-09-30). Not a Phase 12 exit. Root `PHASE` stays `12.12B`. 12C is not started.

PostgreSQL remains the only durable authority. Database work stays on the persistence worker. File mode, selected by an explicit `PersistenceService::open`, has no character lease and keeps the existing local occupancy rule. Server startup requires `PURGATORY_DATABASE_URL`.

## Policy values

| Policy | Value | Where |
|---|---|---|
| Character lease expiry | 60 seconds | `CHARACTER_LEASE_EXPIRY` |
| Character lease renewal | 10 seconds | `CHARACTER_LEASE_RENEWAL` |
| Channel generation expiry | 60 seconds | `CHANNEL_GENERATION_EXPIRY` |
| Channel generation renewal | 10 seconds | `CHANNEL_GENERATION_RENEWAL` |

The durations match. The identifiers do not. A character lease and a channel generation are different rows. Tests assert the constants and that expiry SQL uses `clock_timestamp()` after the row lock. PostgreSQL `now()` is fixed at transaction start, so a lock wait must not be allowed to treat an expired lease as live.

## One active character

`admit` inserts a lease or, once `clock_timestamp()` is past `expires_at`, replaces an expired one and advances the generation. It does not replace an unexpired lease, whether the requested character is the same or different. Release expires the row in place. The next admit uses the following generation, so the generation that just released cannot commit again.

Same-process reconnect:

1. Remove the character from `World` first. Pending durable work blocks this and blocks detach.
2. Await the save under the old generation. A failed save does not take a new generation.
3. `supersede` checks that generation and loads the owned restore in the same transaction.
4. Only then spawn the new session.

A second process cannot perform step 1. Its `admit` stays held until the holder releases the lease or the lease expires. Two database rejections are not treated as two inactive copies: the new session does not become active while the old lease is live.

A different character requires confirmed release after pending commands settle. If the holder crashed, the switch is allowed after expiry. An unexpired lease for another character is not silently replaced.

On renewal failure, or any other loss of lease authority, the session stops accepting gameplay input. A save is not reported successful. `SaveHandoff::Accepted` is only a queue handoff. Logout awaits the save before release. Shutdown returns `Drained`, `TimedOut`, or `WorkerClosed` and does not describe a queued snapshot as saved.

The holding process also keeps a local monotonic deadline. It is the instant the admit, supersede, claim, or renewal request was sent, plus the 60-second policy expiry. Time spent queued before the reply counts against that expiry. If the reply arrives at or after the deadline, gameplay is not entered and a late channel claim refuses to start. The renewal task stops gameplay, or channel admission, when that deadline passes even if the persistence worker has not answered. The database clock is still what another process sees. A renewal that the worker has already started can still commit after the local stop; this process has already stopped accepting input, and the other process can admit only once that database row expires.

Channel authority is checked again after the admit reply, before world entry. `enter_restored` checks the character deadline and the channel deadline before it creates a binding. An already-expired deadline returns `AuthorityLost` and does not spawn an entity, so there is no simulation tick or replication to clean up. That is entry prevented. If `enter_restored` has already returned success and authority then ends before the connection task finishes, that task removes the binding without a save and releases the unused lease through the persistence worker. That second path is cleanup after an entry that was still authorized when World created the binding. A channel stop also sticks on the gameplay owner: an entry command drained after that stop does not create a binding. The character renewal supervisor still starts only after a successful entry.

The simulation thread stores those same deadlines. `apply_input` and `simulate_tick` refuse new input and do not apply held movement once a deadline has passed, even while `LoseAuthority` or `LoseAllAuthority` is still queued. A rejected renewal sets `authority_lost` before that deadline; the next tick also drops held movement. Before the critical scheduler runs, an ended character or channel authority interrupts the player's active ability and clears Dash, so a windup effect due on that tick does not land and Dash does not keep moving. Damage and displacement already integrated on earlier ticks stay. Vertical motion is left to ordinary physics. The simulation thread does not call the database. File mode stores no deadline and keeps its current input and Dash behavior. A successful renewal delivers the extended deadline on the lifecycle channel; until that message is drained, the previous deadline still bounds gameplay. The database clock remains what another process sees.

## Restore

A granted or superseded lease restores the owned character, live owned items, facts, NPC-met, dialogue heard, and learned grants. Derived equipment and grants are rebuilt from those rows. An equipped row is restored only when `authorize_equip` accepts that item's authored equipment facet for that slot. A non-equippable item in weapon, or an equippable item in the wrong slot, fails closed: the attempted `World` entry is removed and the connection task asks the persistence worker to release the unused lease. A valid weapon in the weapon slot still restores. Runtime entity and connection ids are not restored. Unknown content, an empty restore, or a bad item fails closed the same way. Welcome facts are seeded only for a file-mode session, which has no lease.

After `finish_durable`, the live session adopts the committed revision before the next snapshot. A snapshot is still not itself a durable command.

## Ground retirement

Migration `0002_lifecycle.sql` adds `character_leases`, `channel_generations`, and ground channel columns on `item_instances`. `0001_foundation.sql` is unchanged.

Startup claims channel `0` before accepting connections. A busy claim refuses to start and does not retire that channel's drops. A claimed generation retires live ground rows whose stamp is not the new generation. It also retires null-scoped legacy ground, including when another channel generation is live. Ground stamped for a live channel stays on the ground. The sweep is idempotent and does not refund or reuse item ids.

Clean shutdown expires the channel row in place after queued snapshots are attempted. The shutdown deadline includes time spent waiting to enqueue `Shutdown` on the bounded worker queue. If that deadline passes, the result is `TimedOut`, not `Drained`. `TimedOut` does not confirm a write that was already in progress. `flush_deferred_latest` checks `stop_writer` before each deferred write. That is the flush inside `Shutdown`, the flush after an ordinary command, and the flush after the worker loop. After the `Shutdown` flush returns, the worker checks again before channel release. A write that already passed its check may finish, including when the flag is set during that call. The flag can also become true between checks, so one already-admitted write may finish too. The worker is a dedicated thread, so dropping the Tokio runtime does not wait for that thread. A crash, or any exit that does not release, leaves the channel row until expiry. An explicit expiry update then claims the next generation. The crash test does not sleep for 60 seconds. The next claim uses the following generation, so a drop stamped with the released generation is retired and that generation cannot renew or release. Deleting the row would insert generation 1 again.

## What the crash test proves

`process_exit_leaves_a_committed_lease_until_expiry` admits in a child process, waits until that admit has committed, then calls `std::process::abort`. The parent observes the lease still held, then an explicit expiry update, then a later generation. The wait is a condition on a ready file, not a 60-second sleep. Expiry in tests sets `expires_at` with the database clock; it does not sleep for the policy duration.

That proves a committed lease row is still there after process death. It does not prove durability across hardware power loss, torn pages, or a lost `fsync`.

`clean_channel_release_keeps_generation_monotonic_and_retires_stamped_ground` proves a clean release does not reuse generation 1, the stamped drop is retired on the next claim, and the released generation cannot renew or release. Abandoning the new generation without release leaves it busy until an explicit expiry update, which then advances the generation. It does not abort the process.

`unscoped_ground_is_retired_before_admission_while_another_channel_is_live` proves a startup claim retires one unscoped ground row while two other channels are live, leaves those channels' stamped drops on the ground, and only then admits. The unscoped row is created by a test helper that clears a stamp. Production ground writes stamp the one claimed channel.

`stalled_renewal_expires_while_the_worker_reply_is_still_blocked` and `stalled_lease_renewal_stops_gameplay_before_another_world_takes_input` use a paused Tokio clock. They prove the local deadline stops a renewal that has not replied, the old session then rejects input, and a second `World` can accept input. They do not take a second database lease. `queued_reply_time_counts_against_the_deadline` checks the send-instant arithmetic without sleeping. `renewal_reply_extends_from_the_send_instant_not_past_it` proves a successful renewal moves the local deadline to that request's send instant plus 60 seconds.

`channel_stop_during_admit_does_not_enter_world` holds the admit reply after the worker has produced it, then runs the real channel renewal supervisor on a paused clock until that supervisor clears channel authority and its stop has been drained. Releasing the reply does not enter `World`, and input for that connection is stale. `character_deadline_during_enter_does_not_enter_world` queues restored entry after the pre-entry deadline check, then advances the paused clock past the character expiry before the simulation thread drains the command. World entry refuses that command before creating a binding. The character renewal supervisor is not running in that test; the deadline itself is what rejects the entry. Neither test calls `lose_authority` directly.

`expired_character_deadline_rejects_input_before_the_stop_message` and `expired_channel_deadline_rejects_input_before_the_stop_message` accept movement, leave the stop message queued, then advance a paused clock past the deadline. Further input is stale and the next tick does not move the player. `expired_character_deadline_does_not_spawn_before_entry` and `expired_channel_deadline_does_not_spawn_before_entry` call world entry after the deadline and observe no entity and no replication. `file_mode_without_a_deadline_still_simulates_input` still moves after the same clock advance.

`authority_stops_held_movement_after_character_renewal_rejected` and `authority_stops_held_movement_after_channel_renewal_rejected` lose authority while the local deadline is still in the future. New input is stale and the held move does not change position. `authority_stops_dash_on_expired_character_deadline` and `authority_stops_dash_on_expired_channel_deadline` leave the stop queued, advance past the deadline, and the next tick does not continue Dash. The same stop before the deadline is `authority_stops_dash_after_character_renewal_rejected` and `authority_stops_dash_after_channel_renewal_rejected`. `authority_stops_due_windup_on_expired_character_deadline` and `authority_stops_due_windup_on_expired_channel_deadline` have Basic Strike's damage due on that first expired tick; the target stays at full health. `authority_stops_committed_strike_after_character_deadline` keeps damage that already landed. `authority_stops_file_mode_dash_still_moves` still continues Dash after the same clock advance.

`shutdown_times_out_within_the_total_deadline_while_the_worker_is_stalled` fills the worker queue while its thread is blocked, then requires `TimedOut` inside the configured deadline. The child process drops its runtime while that block is still held, then releases the block and checks that queued saves were not written and the writer stopped. That stall is before `Shutdown` is entered. `shutdown_timeout_during_drain_does_not_start_later_writes` pauses the first save drained inside `Shutdown`, with another save and deferred state still pending, and lets the deadline expire before resuming. The result stays `TimedOut`. After resume, the paused save may finish and is not confirmed by that result. The later queued save and the deferred snapshot are not written, and channel release is not started. `shutdown_timeout_during_deferred_flush_does_not_start_later_writes` pauses the first deferred write inside `Shutdown`, with another deferred snapshot already pending, and lets the deadline expire before resuming. Exactly one of those two files exists afterward, channel release does not start, and the result stays `TimedOut`. `shutdown_timeout_during_command_skips_post_command_deferred_flush` pauses an ordinary save, inserts two deferred snapshots during that pause, and lets the deadline expire before resuming. The ordinary save may finish. Neither deferred snapshot is written, and channel release does not start. These stalls are test stalls, not live PostgreSQL queries. They hold the pause until after `TimedOut`, so the next check observes the flag. They do not prove what happens when the flag changes after a check has already admitted the next write. `durable_restore_rejects_a_non_equippable_item_in_weapon` and `durable_restore_rejects_an_equippable_item_in_the_wrong_slot` enter through the real admission handoff. Both remove the entity and ask the worker to release the lease. File mode answers that release with the migration error because it has no lease row. `durable_restore_keeps_valid_equipment` restores `equipment.debug.practice_sword` into the weapon slot and does not release the lease.

`unusable_connection_at_the_commit_reply_stays_unknown_until_retry` still disconnects only after `COMMIT` succeeds. Injecting a fault during `COMMIT` itself remains an evidence limit from 12A.

## Tests

PostgreSQL tests are `#[ignore]`d and run with the existing disposable database job. They do not use `Purgatory_dev`.

- `one_user_has_one_live_character_across_connections` — a second connection is held; it prints `12B_LIFECYCLE admit_us=`
- `stale_generation_cannot_renew_release_commit_or_save`
- `superseded_generation_rejects_a_command_that_was_waiting_on_the_lock`
- `admit_restores_owned_items_facts_and_grants_without_runtime_ids`
- `clock_timestamp_moves_while_now_stays_at_transaction_start`
- `renewal_extends_a_live_lease_and_a_stale_one_cannot`
- `ground_retirement_is_scoped_idempotent_and_does_not_reuse_ids`
- `clean_channel_release_keeps_generation_monotonic_and_retires_stamped_ground`
- `unscoped_ground_is_retired_before_admission_while_another_channel_is_live`
- `process_exit_leaves_a_committed_lease_until_expiry`
- `randomized_lease_steps_keep_one_authority`

Gameplay tests, without a database:

- `pending_durable_blocks_detach_and_same_character_reconnect`
- `same_process_reconnect_removes_the_old_entity_before_the_new_enter`
- `committed_revision_is_visible_on_the_next_snapshot`
- `lost_authority_rejects_gameplay_input`
- `stalled_lease_renewal_stops_gameplay_before_another_world_takes_input`
- `channel_stop_during_admit_does_not_enter_world`
- `character_deadline_during_enter_does_not_enter_world`
- `expired_character_deadline_rejects_input_before_the_stop_message`
- `expired_channel_deadline_rejects_input_before_the_stop_message`
- `expired_character_deadline_does_not_spawn_before_entry`
- `expired_channel_deadline_does_not_spawn_before_entry`
- `file_mode_without_a_deadline_still_simulates_input`
- `authority_stops_held_movement_after_character_renewal_rejected`
- `authority_stops_held_movement_after_channel_renewal_rejected`
- `authority_stops_dash_on_expired_character_deadline`
- `authority_stops_dash_on_expired_channel_deadline`
- `authority_stops_dash_after_character_renewal_rejected`
- `authority_stops_dash_after_channel_renewal_rejected`
- `authority_stops_due_windup_on_expired_character_deadline`
- `authority_stops_due_windup_on_expired_channel_deadline`
- `authority_stops_file_mode_dash_still_moves`
- `authority_stops_committed_strike_after_character_deadline`
- `shutdown_times_out_within_the_total_deadline_while_the_worker_is_stalled`
- `shutdown_timeout_during_drain_does_not_start_later_writes`
- `shutdown_timeout_during_deferred_flush_does_not_start_later_writes`
- `shutdown_timeout_during_command_skips_post_command_deferred_flush`
- `durable_restore_rejects_a_non_equippable_item_in_weapon`
- `durable_restore_rejects_an_equippable_item_in_the_wrong_slot`
- `durable_restore_keeps_valid_equipment`
- `character_occupancy_rejects_second_session_and_reconnect_gets_new_entity` — file mode still rejects a second local session

`network::lease_clock` tests, without a database: `queued_reply_time_counts_against_the_deadline`, `stalled_renewal_expires_while_the_worker_reply_is_still_blocked`, `renewal_reply_extends_from_the_send_instant_not_past_it`.

Policy tests in `lifecycle::policy_tests` lock the 60-second and 10-second values.
