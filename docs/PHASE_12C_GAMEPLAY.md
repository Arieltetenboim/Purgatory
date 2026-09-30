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

Player drops move an existing item to temporary ground ownership. Another eligible
player may pick it up while the channel is live. Ordinary unclaimed ground is
retired on the next channel claim after shutdown or crash, without refund and
without reusing the item id. Monster bodies are not stored. A new process builds
them again from authored content.

## Unresolved

There is no authored duration for a live ground-item expiry timer. This slice does
not invent one and does not restore a timer after restart. A test can retire one
live ground item explicitly. That retire command has no character owner, because a
ground row has none, so the database does not fence it with a character-lease
generation. The simulation thread still requires a leased session and reserves the
item before submitting it.

A dialogue remove that would split a stack is rejected before commit. The durable
command can place, move, or retire a whole item. It cannot reduce a quantity in place.

## Not in this slice

Quests, Trade, currency, and new gameplay features are not implemented here.
Phase 12 is not complete. A normal client still has to prove reconnect, restart,
and recovery before that exit.

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
