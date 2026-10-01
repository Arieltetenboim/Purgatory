# Phase 12C — Gameplay durable commands

Status: **accepted 2026-10-01** after PR #121 merged at `19f19c2`. This is not a Phase 12 exit.
Root `PHASE` = `12.12C`.

PostgreSQL remains the durable authority. `World` remains the live gameplay authority.
The persistence worker from 12A still owns database calls. Leases, fencing, revision
adoption, and lifecycle from 12B are unchanged. The simulation tick does not call
the database.

## What this slice wires

A leased session validates and reserves the actor, items, slots, and dialogue choice,
then submits one `DurableCommand` through the persistence worker. `World` changes
and the success reply are sent only after that command returns `Ok`. A failed or
unknown outcome does not send success and does not grant an uncommitted reward.
File mode, which has no lease, keeps the previous immediate `World` path.

The command key is `c{character}-r{revision}-{operation}`. It is not the connection
sequence number. A retry of the same key returns the stored result. After a successful
command the character revision advances, so a later real action is a different key.

Dialogue choices store the authored beat id. The content loader already rejects
duplicate beat ids, so the array index stays a session cache and is not the durable
identity. One choice commits its item changes, facts, NPC-met and heard markers,
and learned ability together. No shipped beat contains an item change, a fact, and
a learned ability in the same choice; the PostgreSQL test composes those three
actions on an authored beat id and says so.

Player drops move an existing item to temporary ground ownership. That item belongs
to the map and is open to every player immediately. Ordinary unclaimed ground is
retired on the next channel claim after shutdown or crash, without refund and
without reusing the item id. Monster bodies are not stored. A new process builds
them again from authored content.

## Ground lifetime

Ordinary live ground disappears 200 seconds after it becomes visible in the running
channel. Monster loot is collectible only by the killing character while elapsed
time is less than 40 seconds. At 40 seconds it becomes collectible by other
eligible characters. A player-dropped item belongs to the map and is collectible
immediately, including by its former owner. Ordinary unclaimed ground disappears
on shutdown or crash. Its timer is not restored, the item is not refunded, and a
retired item-instance id is not reused.

The channel clock advances from the network loop's elapsed time. Tests advance
that clock directly. A durable player drop starts its timer when the committed
ground manifestation appears in `World`. If that apply fails, reconciliation
places the same item-instance id on the ground and starts the timer before
`DropAccepted`. When that placement cannot be done, the client is not told the
drop succeeded and control stays blocked. The item is not refunded. Expiry is
one map entry per item, ordered by due time. One wake inspects at most eight
due or deferred items and submits at most that many retires, through the
persistence worker. A deferred item expires on a later wake after its pickup
reservation ends. Removing one item does not scan the other items that share
its deadline. The simulation tick does not call the database. A stale channel
deadline does not retire ground. A pickup that is still waiting on the database
keeps the item reserved, so expiry does not retire it. A retire of an item
already committed to a character conflicts.

A developer-spawned world item is ordinary visible ground. It uses the same
200 second timer and is collectible immediately. Its id comes from the reserved
durable range. Expiry inserts that id as a retired row. It is not an epoch mint.

`manifest_monster_loot` is the authoritative spawn and eligibility path for a
future loot drop. No loot table is authored, and monster death does not call it.
Both that path and a developer-spawned collectible item take an id that
PostgreSQL has already reserved. Collectible developer spawns require
PostgreSQL. File mode cannot reserve an id, so those spawns do not appear.
If the local range is empty, nothing is spawned. The network loop replenishes
the range through the persistence worker. The simulation tick does not. A crash
may waste ids that were reserved and never shown. A visible id and a retired id
are not issued again.

The reservation and the counter advance are one transaction. The row records
the channel generation that issued the range. At commit, an id is accepted only
when that live generation has a covering row. An unused id from another
channel, an unissued gap below the counter, and a range issued to a previous
generation are rejected and write no item row. Renewal keeps the same
generation, so its unused ids remain spendable. A new generation receives a
later disjoint range.

Pickup of that visible id inserts the same id. It does not call `place_new`.
The command rejects an id that was not reserved for this channel generation, an
id that already exists, a retired id, a stale channel generation, and a second
pickup of the same id.
The same command key still returns the stored result. An unknown outcome stays
unknown until that key is retried. Expiry of an unpicked reserved id writes a
retired stub under the claimed channel generation. A later pickup of that id
conflicts. A retire aimed at an id already committed to a character still
conflicts, because that command has no character revision.

A player drop keeps one item-instance id from inventory, through ground, through
pickup, and back to character ownership. `postgres_12c_drop_pickup_reclaim_and_competition`
reads that same id as `ItemOwner::Ground` and then as that character's item.
Dialogue rewards still use `place_new` and the same counter, so those mints stay
outside every reserved range. The in-memory fixture has no durable allocator and
does not substitute an epoch id. Issue #122 remains the ownership-history ledger, not
this allocator. Issue #123 remains the later ground-capacity policy.

A test can also retire one live ground item explicitly. That retire command has
no character owner, because a ground row has none, so the database does not fence
it with a character-lease generation. It does require the claimed channel
generation. The simulation thread still requires a leased session only when a
player action reserves the item. Channel expiry does not.

## Local PostgreSQL startup and host move

The operator steps are [`POSTGRESQL_LOCAL_RUN.md`](POSTGRESQL_LOCAL_RUN.md)
and [`POSTGRESQL_HOST_MOVE.md`](POSTGRESQL_HOST_MOVE.md).

Server startup requires PostgreSQL. Local development reads
`config/local/database.env` when `PURGATORY_DATABASE_URL` is unset. That
git-ignored file sets the runtime URL, migration URL, schema, and
`PURGATORY_DEPLOYMENT_ID`. Developer Hub loads it on every launch. Check and
Start Server use the runtime role and do not require the administrator
password. `PURGATORY_DATABASE_SCHEMA` defaults to `public` only when the URL
is supplied by the environment and the schema variable is absent. A deployment
id is 1 to 64 characters: ASCII letters, digits, `.`, `_`, and `-`. The local
file must use `purgatory-dev`.

Database creation is a Developer Hub operation, or the headless
`--database-create` command, against the pinned local database `Purgatory_dev`.
`--bootstrap-postgresql`, `import_legacy_postgresql`, and the file writer are
superseded by ADR-0073. Historical import steps in
[`PHASE_12A_POSTGRESQL.md`](PHASE_12A_POSTGRESQL.md) are not current setup.

A later start is `cargo run -p purgatory-server` with the same URL, schema, and
deployment id. It reopens an initialized database and does not apply migrations.
It refuses a missing schema, a missing deployment row, a different deployment
id, or a migration history that does not match the binary.

A missing URL, a failed connection, the wrong schema or deployment id, or
unsupported migration history stops startup. There is no file writer.

Moving the database to another host keeps the committed rows. Point
`PURGATORY_DATABASE_URL` at the restored database and use the same
`PURGATORY_DEPLOYMENT_ID`. Do not copy `durable_writer.json`. A mistaken URL or
schema fails closed because its stored deployment id does not match.

Collectible developer spawns still require this PostgreSQL writer. The in-memory
fixture cannot reserve an item id, so those spawns do not appear.

## Unresolved

A dialogue remove that would split a stack is rejected before commit. The durable
command can place, move, or retire a whole item. It cannot reduce a quantity in place.

## Not in this slice

Quests, Trade, currency, and new gameplay features are not implemented here.
Phase 12 is not complete. The normal-client reconnect, clean restart, and
committed-state process-crash checks below are now observed. The separate
Phase 12 exit gates remain open.

## Review fixes still in review

Logout and same-process reconnect remove the actor's inventory and equipment
records from `World` with the actor. A partially failed restore clears those
records before the connection is released. Committed database ownership is left
as it was. A live ground manifestation is not removed with the player.

Equipment replacement puts the displaced item in the incoming item's inventory
slot, including when that is the only free slot. After acknowledgement, the
database row and `World` use that same item id, location, and slot, and the
derived equipment grant follows the equipped item.

A confirmed commit whose `World` application fails does not send success and
does not open the character for another mutation. Recovery reads the committed
character and replaces the live items, narrative, and derived grants from that
snapshot. While that read is outstanding or has failed, new player input, held
movement, Dash, ability activation, and a scheduled ability effect are stopped.
Effects that already landed stay. Restoring the committed character resumes
control. Stopping the session also leaves those new effects unapplied. The
simulation tick still does not call the database.

After that restore, the original Drop, pickup, or equipment request is answered
with the committed result. A retry of the same sequence repeats that result.
The commit is not described as rolled back, and the retry is not left unanswered.

An unknown commit outcome keeps the original command key reserved. The first
unknown result reaches GameplayOwner before any retry. The next resolution
submits that same key through the persistence worker. The same persistence worker opens a new
database connection when the previous one died after `COMMIT`, reads the stored
key, and returns that result once. If the database cannot be reached, the retry
stays unknown instead of rejecting the committed command. Two unknown results
do not invent a new key. While the reply is unknown, new input, held movement,
Dash, ability activation, and a scheduled ability effect do not use the old
`World`. An equipped grant cannot be used after its unequip may have committed.
The original key is applied once when a definite result returns, and control
resumes from that result. A command that is still in flight, and has not lost
its reply, does not pause gameplay. The simulation tick still does not call
the database.

A confirmed dialogue choice whose `World` application fails is restored with
the committed item, fact, and learned ability. Reconciliation then resolves
that choice in the dialogue session. The client receives the accepted choice,
or the session is closed and the restored state is sent. Resending the choice
in that session does not mint a second reward under the new character revision.

Logout removes a character's item ids from the server's durable-item set when
those records are no longer in `World`. A ground item that is still manifested
stays in the set.

## Verification recorded for this review

`./scripts/check.ps1` exited 0 on this Windows machine (about 206s). Persistence
lib tests were 38 passed and 29 ignored. Server bin tests were 337 passed and
16 ignored. The ignored server tests include the seven `postgres_12c` cases.

A disposable `postgres:18` container, database `purgatory_12a_test`, not
`Purgatory_dev`, then ran the 29 ignored persistence tests (29 passed in 18.21s;
`admit_us=19549`; workload `total_ms=1590`, `mean_us=17673`, debug profile, not
a capacity claim) and the seven ignored server `postgres_12c` tests (7 passed
in 2.46s).

Server failure injection rejects a stale lease generation and a retired content
rule before the item moves. It does not kill an in-flight SQL statement. The
persistence suite still owns the unknown-commit connection cases. A lost gameplay
reply is a second commit of the same key, applied once.

The review-fix run of `./scripts/check.ps1` also exited 0. Persistence lib tests
were 38 passed and 29 ignored. Server bin tests were 340 passed and 20 ignored.
Simulation lib tests were 431 passed. The same disposable database then passed
the 29 ignored persistence tests in 18.25s (`admit_us=61546`; workload
`total_ms=1510`, `mean_us=16787`, debug profile, not a capacity claim) and 11
ignored server `postgres_12c` tests in 4.33s.

The recovery run of `./scripts/check.ps1` exited 0 in about 159s. Persistence
lib tests were 38 passed and 30 ignored. Server bin tests were 343 passed and
20 ignored. Simulation lib tests were 431 passed. Before those fixes, the same
worker returned `commit outcome unknown: the database connection is closed`,
movement input stayed accepted while the character was inconsistent, a Drop
retry after reconcile produced no reply, and leased logout left item 40000
registered. The same disposable database then passed 30 ignored persistence
tests in 18.45s (`admit_us=16957`; workload `total_ms=1737`, `mean_us=19302`,
debug profile, not a capacity claim) and 11 ignored server `postgres_12c` tests
in 4.21s. Server tests still do not kill an in-flight SQL statement. The
connection-loss cases live in the persistence suite, after `COMMIT`. No live
ground-item expiry duration is chosen.

The unknown-reply and dialogue-resync run of `./scripts/check.ps1` exited 0
in about 129s. Persistence lib tests were 38 passed and 30 ignored. Server bin
tests were 345 passed and 21 ignored. Simulation lib tests were 431 passed.
Before those fixes, movement input stayed `Accept` while an unequip reply was
unknown, and resending the restored dialogue choice staged
`c93-r2-talk-20001-intro-0`. The same disposable database then passed 30
ignored persistence tests in 18.03s (`admit_us=17280`; workload
`total_ms=1546`, `mean_us=17179`, debug profile, not a capacity claim) and 12
ignored server `postgres_12c` tests in 4.31s. The new server case commits a
dialogue reward, fails the `World` apply, restores it, and proves a resend does
not mint a second item, fact, or learned ability. No live ground-item expiry
duration is chosen. Phase 12C remains in review.

The first-unknown run of `./scripts/check.ps1` exited 0 in about 246s.
Persistence lib tests were 38 passed and 30 ignored. Server bin tests were
347 passed and 21 ignored in 2.76s. Simulation lib tests were 431 passed.
Commit `8261f49` is test-only. GitHub Actions run
[36769949858](https://github.com/Arieltetenboim/Purgatory/actions/runs/36769949858)
failed the quality gate and passed the PostgreSQL job. Locally, before this
fix, both new tests timed out at 300ms: `first unknown must reach GameplayOwner
before the blocked retry` and `first unknown before blocked retry`. The Dash
case first failed because its ability used an already acknowledged input
sequence; that anchor now follows the movement sequence, and Dash starts before
the unknown reply. After the submit task reports the first unknown before
retrying, both tests pass. A disposable `postgres:18` container
`purgatory-12c-unknown`, database `purgatory_12a_test` on `127.0.0.1:5433`,
not `Purgatory_dev`, then passed 30 ignored persistence tests in 26.90s
(`admit_us=16080`; workload `total_ms=2876`, `mean_us=31960`, debug profile,
not a capacity claim) and 12 ignored server `postgres_12c` tests in 6.46s.
The 200 second ground lifetime and 40 second killer window are recorded and
are not implemented. Phase 12C remains in review.

The ground-lifetime run of `./scripts/check.ps1` exited 0 in about 146s.
Persistence lib tests were 38 passed and 31 ignored. Server bin tests were
353 passed and 23 ignored in 2.61s. Simulation lib tests were 431 passed.
A durable drop does not expire before its committed manifestation appears.
At 200 seconds minus one nanosecond the drop remains; at 200 seconds, with no
connected player, the channel queues `retire-{item}`. Another character and the
former owner can both stage a pickup immediately. Monster loot rejects another
character until exactly 40 seconds. One wake removes eight of twenty due runtime
drops. An expired channel deadline leaves the drop in place. A pickup that is
still waiting blocks expiry, and the committed item stays in inventory. A
disposable `postgres:18` container `purgatory-12c-ground`, database
`purgatory_12a_test` on `127.0.0.1:5433`, not `Purgatory_dev`, then passed 31
ignored persistence tests in 19.24s (`admit_us=16847`; workload `total_ms=1664`,
`mean_us=18490`, debug profile, not a capacity claim) and 14 ignored server
`postgres_12c` tests in 5.20s. The new cases are
`expired_channel_cannot_retire_live_ground`,
`postgres_12c_restart_does_not_restore_ground_time`, and
`postgres_12c_expiry_cannot_retire_a_character_item`. Monster death does not
spawn loot. Phase 12C remains in review.

Before the manifestation fix, `postgres_12c_failed_apply_reconciles_without_a_success_reply`
sent `DropAccepted` while the world drop was missing. Before the wake bound,
one wake inspected 64 deferred items, and removing one item visited 64
same-deadline entries. A developer-spawned world item was still present after
200 seconds. After the fix, reconciliation places that committed item id on
the ground and starts its timer before `DropAccepted`; at 200 seconds the same
id is retired. One wake inspects at most eight deferred items, and those items
expire after their pickup reservation ends. Removing one item visits one map
entry. A developer-spawned world item expires locally at 200 seconds and does
not submit a durable retire. The review run of `./scripts/check.ps1` exited 0
in about 128s. Persistence lib tests were 38 passed and 31 ignored. Server bin
tests were 356 passed and 23 ignored in 2.50s. Simulation lib tests were 431
passed. A disposable `postgres:18` container `purgatory-12c-review`, database
`purgatory_12a_test` on `127.0.0.1:5433`, not `Purgatory_dev`, then passed 31
ignored persistence tests in 18.24s (`admit_us=17202`; workload `total_ms=1676`,
`mean_us=18627`, debug profile, not a capacity claim) and 14 ignored server
`postgres_12c` tests in 5.35s. Monster death does not spawn loot. Phase 12C
remains in review.

GitHub Actions run [36833503044](https://github.com/Arieltetenboim/Purgatory/actions/runs/36833503044)
on `e0e090a` passed the PostgreSQL job and failed the quality gate in
`channel_stop_during_admit_does_not_enter_world` with
`timed out waiting for channel renewal held`. That wait only yielded. The
renewal hold is set by the persistence worker thread, and paused Tokio time
does not run that thread. The same panic is reproduced by
`channel_renewal_wait_observes_the_worker_thread` in 0.01s, before the wait
sleeps. It is not an admission race and not a product regression. After the
wait sleeps 2ms per attempt, the same way the character-deadline test already
waits for the worker, both tests pass. The admission test still requires
`StorageFailure` and no world entity. The follow-up run of `./scripts/check.ps1`
exited 0 in about 132s. Persistence lib tests were 38 passed and 31 ignored.
Server bin tests were 357 passed and 23 ignored in 2.63s. Simulation lib tests
were 431 passed. A disposable `postgres:18` container `purgatory-12c-admit`,
database `purgatory_12a_test` on `127.0.0.1:5433`, not `Purgatory_dev`, then
passed 31 ignored persistence tests in 18.75s (`admit_us=18526`; workload
`total_ms=1718`, `mean_us=19094`, debug profile, not a capacity claim) and 14
ignored server `postgres_12c` tests in 5.40s. Monster-loot pickup still mints
a new durable id. That allocator choice is not made here. Phase 12C remains
in review.

Before the reserved-id change, `monster_loot_pickup_replaces_the_visible_id`
replaced visible id `3620915472163143681` with minted id `900001`, and
`dev_spawned_pickup_replaces_the_visible_id` replaced `933935450194706433`
with `900002`. After it, both tests keep the reserved visible id. PostgreSQL
reserves a monotonic range before those ids can appear. The network loop
replenishes that range. `simulate_tick_does_not_request_item_ids` shows the
simulation tick does not. An empty pool spawns nothing.
`reserved_ids_stay_disjoint_across_connections_and_restarts` and
`reserved_pickup_keeps_its_id_against_retry_expiry_and_reuse` cover disjoint
ranges, restart, retry, a competing pickup, expiry, and an unreserved id.
`postgres_12c_reserved_loot_pickup_keeps_the_visible_id` keeps that id through
pickup and retires the unpicked id. `./scripts/check.ps1` exited 0 in about
264s. Persistence lib tests were 39 passed and 33 ignored. Server bin tests
were 362 passed and 24 ignored in 2.44s. Simulation lib tests were 431 passed.
A disposable `postgres:18` container `purgatory-12c-ids`, database
`purgatory_12a_test` on `127.0.0.1:5433`, not `Purgatory_dev`, then passed 33
ignored persistence tests in 32.49s (`admit_us=26091`; workload `total_ms=3275`,
`mean_us=36393`, debug profile, not a capacity claim) and 15 ignored server
`postgres_12c` tests in 10.99s. The container was removed. File mode cannot
reserve ids. Monster death does not spawn loot. Phase 12C remains in review.

Before the range check, an id below `next_item_instance_id` with no item row
was enough. `another_channels_unused_id_cannot_be_inserted` inserted unused id
`2` from channel 1 through channel 2. `unissued_gap_below_the_counter_cannot_be_inserted`
inserted unissued id `20` after the counter had been moved to `40` with no
reservation. `previous_channel_generation_cannot_spend_its_unused_ids` let
generation 2 insert id `1`, which generation 1 had reserved. After migration
`0003_item_id_reservations.sql`, those three reject the insert and write no
row. The issuing generation can still spend its own id. A new generation
receives a later range and can spend that. A restarted connection cannot spend
an unused id from the previous process. `./scripts/check.ps1` exited 0 in
about 140s. Persistence lib tests were 39 passed and 36 ignored. Server bin
tests were 362 passed and 24 ignored in 2.64s. Simulation lib tests were 431
passed. A disposable `postgres:18` container `purgatory-12c-range`, database
`purgatory_12a_test` on `127.0.0.1:5433`, not `Purgatory_dev`, then passed 36
ignored persistence tests in 21.06s (`admit_us=17338`; workload `total_ms=1821`,
`mean_us=20243`, debug profile, not a capacity claim) and 15 ignored server
`postgres_12c` tests in 5.87s. The container was removed. Collectible developer
spawns require PostgreSQL; file mode does not show them. Phase 12C remains in
review.

Local development reads `config/local/database.env` from the repository root.
The file is gitignored. `DEV_HUB.BAT` and Developer Hub load it on every launch,
including double-click. Check and Start Server use the runtime role. Create and
Reset ask for the administrator password in Hub and do not store it. Connection
URLs and passwords are redacted from Hub logs and process output.
`./scripts/check.ps1` exited 0 in about 180s. Persistence lib tests were 23
passed and 38 ignored. Server bin tests were 359 passed and 24 ignored in
2.49s. Simulation lib tests were 431 passed. A disposable `postgres:18`
container `purgatory-12c-localcfg`, database `purgatory_12a_test` on
`127.0.0.1:5433`, not `Purgatory_dev`, then passed 37 ignored persistence tests
in 25.80s, the disposable create/reset test in 2.18s, 15 ignored server
`postgres_12c` tests in 6.71s, and the startup test in 3.01s. The container was
removed. `Purgatory_dev` was not created, reset, or connected to. Phase 12C
remains in review.

Stored current HP is implemented and still pending real-client verification. Logout,
disconnect, and server stop commit the live value. The next login restores
that value in gameplay state. Map travel and equipment saves do not copy an
older HP over a newer one. A character row with NULL `current_health` has no
stored HP yet and loads at full maximum. Respawn still sets full HP. Revival
is unchanged and is not derived from the login or respawn rule. Startup still
does not apply a pending migration. Create on an already initialized database
applies that tail without deleting users, characters, or items.
`./scripts/check.ps1` exited 0 in about 387s. Persistence lib tests were 24
passed and 40 ignored. Server bin tests were 359 passed and 27 ignored in
2.62s. Simulation lib tests were 431 passed. A disposable `postgres:18`
container, database `purgatory_12a_test` on `127.0.0.1:5433`, not
`Purgatory_dev`, then passed 40 ignored persistence tests in 28.63s and 18
ignored server `postgres_12c` tests in 1.65s. The container was removed.
`Purgatory_dev` was not created, reset, or connected to. The running local
server held the game port, so the process-level startup test was not repeated.
Phase 12C remains in review.

## Same-session snapshot revision

An HP or portal snapshot may commit a newer `persistence_revision` while the
player is still in the session. `committed_revision` stays at the revision
loaded at entry or returned by the last durable command until the persistence
worker reports that the snapshot write finished. Queue acceptance and a
deferred latest snapshot are not commits, and neither one changes
`committed_revision`. A failed write does not change it either.

A drop, pickup, equip, or dialogue command is not submitted while a queued or
deferred snapshot revision is still newer than `committed_revision`. After a
successful report, that command's expected revision is rewritten to the adopted
value before submission. A command already given to the worker keeps the
revision it carried. A later snapshot at or below that command result is
overtaken and does not move `committed_revision` again.

`postgres_12c_same_session_snapshot_then_equip_and_drop` stays in one session.
It takes damage, waits until that HP snapshot is both committed and adopted,
then equips. It repeats the wait for a portal save, then drops another item.
Both durable commands and their live `World` results succeed. The test does
not reconnect between the snapshot and the command.

Before the fix, that test failed with equip expecting revision 3 after the HP
snapshot had committed revision 4. `./scripts/check.ps1` exited 0 in about
131s. Persistence lib tests were 24 passed and 40 ignored. Server bin tests
were 364 passed and 28 ignored in 2.70s. Simulation lib tests were 431 passed.
A disposable `postgres:18` container `purgatory-12c-revision`, database
`purgatory_12a_test` on `127.0.0.1:5433`, not `Purgatory_dev`, then passed 39
ignored persistence `postgres_tests` in 48.70s and 19 ignored server
`postgres_12c` tests in 17.01s, and
`disposable_database_create_reset_and_failed_recreate` in 2.77s. The container was removed. `Purgatory_dev` was
not created, reset, or connected to. UDP port 5001 was held by the already
running local server, so the process-level startup test was not repeated.
Phase 12C remains in review.


## Acceptance and Phase 12 exit boundary — 2026-10-01

PR #121 merged into `master` at `19f19c2` with 12C head `0b45929`.
The final GitHub Quality Gate on that head passed. The recorded local quality
gate and disposable PostgreSQL suites above passed after the same-session
revision fix. The owner tested the normal client against the initialized
`Purgatory_dev`: authorized entry and unauthorized rejection; map, equipped
sword, and current HP across logout/login; the same state after clean server
restart; and committed HP, map, and sword after forcibly terminating and
restarting the server. After damage and after a portal transition, item actions
also succeeded in the *same session*, without reconnecting. These are manual
observations, not automated proof of every timing or failure interleaving.

12C accepts the durable integration of existing gameplay on that evidence.
The process-crash check began only after the HP row was visible in PostgreSQL;
it does not claim survival of a queued, uncommitted snapshot. Acknowledged
command recovery and failure injection remain covered by their separate tests.
Historical sections above describing 12C as in review record the state at the
time of those runs, not its current status.

**Phase 12 remains open.** Its separate exit requires owner-private
baseline/resync and failure-visibility proof, two-character isolation, a
database recovery/backup rehearsal, and measured load under the intended
workload (see the revised continuity contract). A policy for long cooldowns
across logout/restart is still unresolved. Production authentication, quests,
trade, currency, and persistent scheduled events are outside the current
Phase 12 implementation scope. No hardware power-loss or disaster-recovery
claim follows from the process-crash test.
