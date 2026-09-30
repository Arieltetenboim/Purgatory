# Phase 12C — Gameplay durable commands

Status: **in review** (2026-09-30). This is not acceptance, and it is not a Phase 12 exit.
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

Ordinary live ground disappears after 200 seconds. Monster loot is exclusive to
the player who killed the monster for the first 40 seconds, then eligible for
other players. A player-dropped item has no exclusive window. Ordinary unclaimed
ground disappears on shutdown or crash, and its timer is not restored.

That rule is recorded and is not implemented yet. Nothing starts the 200 second
timer or the 40 second killer window. Pickup still accepts any player in range
with a free slot. `postgres_12c_expiry_retires_a_live_drop` retires one live
ground item explicitly. That retire command has no character owner, because a
ground row has none, so the database does not fence it with a character-lease
generation. The simulation thread still requires a leased session and reserves the
item before submitting it. These timers and the killer window are still required
before 12C acceptance.

## Unresolved

A dialogue remove that would split a stack is rejected before commit. The durable
command can place, move, or retire a whole item. It cannot reduce a quantity in place.

## Not in this slice

Quests, Trade, currency, and new gameplay features are not implemented here.
Phase 12 is not complete. A normal client still has to prove reconnect, restart,
and recovery before that exit.

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
