# Phase 12A — PostgreSQL foundation

Status: **accepted** as the Phase 12A storage foundation (2026-09-29). This is
not a Phase 12 exit. Root `PHASE` moved to `12.12B` when lifecycle review started. 12C is
not started. Draft PR #116 stays unmerged.

## What 12A stores

Migration `crates/persistence/migrations/0001_foundation.sql` is the only
schema version. It stores:

- DEV login to character ownership, display name, and roster position
- character revision, restore intent, and instance-exit reason
- item-instance identity, quantity, definition, and exactly one live location
- temporary ground ownership with no previous-owner column
- retired item rows, which keep the id
- character facts, NPC-met authored ids, dialogue heard, and learned ability ids
- one durable command key, its request, and its result

Dialogue heard is the NPC numeric `ContentId` plus the authored beat id
(`intro`, `ask_road`). A beat index is rejected. v1 files have no facts or
learned grants, and import does not invent any. Channels are not databases
and not schemas. The server uses one schema, normally `public`.

A player drop is a live row with `location_kind = 'ground'`. Retiring that row
clears its owner and never writes it back to a character. The id stays retired.
12A exposes that operation on `commit_durable`. It does not run it at startup,
and it does not wire live Drop, pickup, equip, dialogue, or client replies.

## Cutover

Do these steps in order. Inventory and import come only after the file-writing
server is gone and the source directory is stable.

1. Stop the one legacy file-writing server and wait until that process has
   exited. The supported deployment is that single process, importing
   development `identity.json` and `char_*.json` records. Production player
   data is not imported from files. Do not start a second file writer.
   `try_save` is a queue handoff: `Accepted` and `DeferredLatest` are not
   durable acknowledgements. A file save is durable only after
   `replace_file_recoverable` finishes. `commit_durable` is the path that
   waits for the worker result. Shutdown waits up to
   `persistence_shutdown_timeout` and then discards that wait. A timed-out
   shutdown can leave a queued snapshot unwritten. That snapshot was never
   acknowledged as durable. A completed replace stays in the character file.
   A failed save is logged and leaves the previous file. Import reads those
   files. Killing the process also skips the drain.
2. Leave the data directory unchanged from that exit until character counts
   and identities have been verified in step 8. The directory is
   `PURGATORY_DATA_DIR`, otherwise the per-user application-data `Purgatory`
   directory. Do not edit it by hand during this window.
3. Create one database. Do not create a database per channel. Create a
   non-superuser runtime role and a separate migration role. Keep both
   passwords out of the repository.
4. Before any production traffic, set and verify:
   - `fsync = on`
   - `synchronous_commit = on` (a replica needs `synchronous_standby_names`
     as well; `off` is not enough for an acknowledged commit)
   - `full_page_writes = on`
   - backups whose restore point is not behind an already acknowledged commit
5. For a remote host, use `sslmode=verify-full` with a private CA or the
   platform trust store, and restrict the network path. A local development
   server and pgAdmin are not that control.
6. Set `PURGATORY_DATABASE_URL` to the runtime role. Set
   `PURGATORY_DATABASE_MIGRATION_URL` when the migration role is separate.
   Neither URL is assumed to be localhost. Optional
   `PURGATORY_DATABASE_SCHEMA` defaults to `public`.
7. Start the PostgreSQL server once. The persistence worker, not the 30 Hz
   tick, then:
   - writes `durable_writer.json` before the import transaction commits
   - applies migration `1` as the migration role and refuses a changed or
     unknown history
   - grants the runtime role usage of the schema, read of `schema_migrations`,
     and only the row changes it needs: select/insert/update on meta,
     characters, items, and durable command rows (the retry lock is
     `SELECT FOR UPDATE`); select/insert on users, NPC-met, dialogue heard,
     and learned abilities; select/insert/update/delete on facts
   - inventories the stable directory and imports it in one transaction, or
     records a fresh database when the directory has no identity file
   Supported inputs are `identity.json` schema 1 or 2 and `char_<16 hex>.json`
   schema 1. A `.bak` is read only when the destination file is missing. A
   lone `.tmp` fails closed. Any other file fails closed. PR #116 journals
   and schema v2 character files are not imported.
8. Confirm character counts and ids against that same unchanged directory.
   Only after that verification may the directory change, and only the
   PostgreSQL writer may change durable state. The marker is
   `{"schema_version":1,"writer":"postgresql"}`. After it exists, opening a
   file writer fails, and a file service that is already open refuses the
   next identity or character replace. That check is a safeguard inside this
   process. It is not a cross-process lock, and it does not prove that step 1
   happened. A replace that has already passed the check can still finish.
   Later edits to the JSON files are not imported again. If the marker write
   fails, the import is not committed and the file writer still opens. If
   the process stops after the marker and before commit, the file writer
   stays closed and the next open imports the unchanged files. If a cutover
   row exists and the marker is missing, open fails closed instead of
   ignoring file edits. Unset the URL only after an explicit repair plan.
9. A corrupt, unknown, duplicate, or unowned record aborts the import.
   Source files are left in place. The marker may already exist, so the file
   writer does not accept a repair that PostgreSQL would later skip. No
   default character replaces a corrupt file. A roster entry with no character
   file becomes a character row at revision 1 with default restore and no
   items, facts, or grants. That is the current create-before-first-save case,
   not a repair of a broken file.

After that import, PostgreSQL is the durable authority. The files are not a
second writer. The supported threat model is one legacy file server, stopped
before import, then one PostgreSQL server. There is no supported concurrent
file writer. An unrelated process writing the directory is outside that
deployment and is not a 12A defect. The marker stops a later file-mode open
of this server. It is not a lock against an unrelated process, and 12A does
not need one for the supported cutover. No 12A blocker remains on this point.

The two roles must be distinct non-superusers. The runtime role is not granted
`CREATE` or `DROP`. `PersistenceHandle::commit_durable` sends the command to
the persistence worker and waits for the stored result. A queue handoff is not
success. Gameplay does not call it yet. The simulation tick does not.

A move onto a character slot checks the committed item definition against the
installed content rule, including equip slot. Retire and a temporary ground
park happen before new inserts, so one command can free a slot and fill it, or
swap two slots, and still roll back if two live items would share a final slot.

A restore snapshot older than the committed revision is ignored. Each character
stores `restore_revision`, the revision at which its current restore was
recorded. Import and character creation set it equal to `persistence_revision`.
A durable command advances `persistence_revision` only. The first snapshot
whose revision equals that new character revision records the restore and sets
`restore_revision` forward. That is the collision with gameplay's first save:
detach and `request_save` emit `loaded revision + 1`, which one durable
command may already have consumed. A different restore at a revision that
already has `restore_revision` equal to it is stale and does not overwrite,
including when it arrives last. That is not idempotency and it is not
last-writer-wins. An identical restore at that revision is the idempotent
retry. A newer snapshot still advances both revisions. The snapshot does not
delete item, fact, or learned-ability rows.

A gameplay snapshot whose revision is behind more than one durable command is
older than the committed revision, so 12A ignores it. That is the stale-snapshot
rule, not a broken 12A guarantee. 12A does not wire gameplay. Adopting the
committed revision in the live session before the next restore snapshot is a
mandatory 12B implementation and test gate. It is not a 12A blocker.

A retry looks up the command key before applying current content rules. The
same key and request return the stored result after an item or ability rule
changes. If `COMMIT` returns a storage error, that error is prefixed with
`commit outcome unknown` and the same connection reads `durable_commands`
before any retry applies the command. A matching stored row is returned. A
failed read stays `commit outcome unknown`. A read that finds no row also
stays `commit outcome unknown` (`the command key was not committed and the
connection could not prove it`) and does not apply the command in that call.
The prefix is the contract. The words "was not committed" inside it are not
proof that the server rolled the transaction back.

`unusable_connection_at_the_commit_reply_stays_unknown_until_retry` covers
only the narrower case. `COMMIT` has already returned success. The test then
terminates that backend, so the follow-up read fails, and the worker returns
`commit outcome unknown`. Another attempt on that same connection stays
unknown. A new connection retrying the same key returns the stored result and
does not insert the item again. The persistence worker returns this result
unchanged. The test does not inject a disconnect during `COMMIT`, before the
server's acknowledgement reaches the client. That during-`COMMIT` path is the
storage-error branch above. No separate test injects it.

## Tests

PostgreSQL tests are ignored unless this command is used with a dedicated
database:

```text
cargo test -p purgatory-persistence --lib postgres_tests -- --ignored --nocapture --test-threads=1
```

The Quality Gate workflow runs that command in a separate `PostgreSQL 12A tests`
job against a disposable `postgres:18` service database named
`purgatory_12a_test`. Its password exists only for that service. The existing
quality-gate job is unchanged. A database-test failure fails that job.

`PostgresSettings::for_tests` refuses database names `purgatory_dev`,
`postgres`, `template0`, and `template1` before connecting. Each test creates
and drops a `p12a_` schema. It does not modify `Purgatory_dev`.
`distinct_runtime_role_can_commit_and_cannot_create_tables` creates two
ephemeral non-superuser roles in that database and drops them afterward.

Local PostgreSQL was not used. `psql -w` against local PostgreSQL 18 returned
`fe_sendauth: no password supplied`, and `PURGATORY_TEST_DATABASE_URL` is
unset. They must not be pointed at `Purgatory_dev`. The GitHub Actions job
recorded in [`TEST_GATES.md`](TEST_GATES.md) is the database evidence. Its
workload line is a debug measurement, not a capacity claim.

## Explicitly not in 12A

Startup ground retirement, account admission, one-active-character fencing,
live Drop, pickup, equip, NPC dialogue actions, ability grants from gameplay,
and client replies. Those belong to 12B and 12C.

12B must implement and test session revision advancement: after a durable
command, the live session adopts the committed revision before it submits the
next restore snapshot. A snapshot older than the committed revision is already
ignored by 12A. That ignore is not a 12A defect.

12B also owns snapshot shutdown status. A queue handoff is not a durable
save, and a shutdown that times out must become visible before anything
reports that snapshot as saved. That is not a change to the PostgreSQL
commit path, and it is not a defect in importing the files that already
completed.

## PR #116

Reused, in adapted form: item-instance identity, the six equipment slot names,
inventory capacity 20, content rules for stack limit, equip slot, and retired
definitions, structural rejection of empty quantities and reserved ids, and
fail-closed handling of corrupt records. The restore path was kept from erasing
state it does not own.

Rejected: the file journal, WAL frames, checkpoint manifests, the durable Drop
clock, map-drop restoration, and the WAL crash tests. Those implement the
superseded ADR-0068 rule.
