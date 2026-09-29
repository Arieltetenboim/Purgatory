# Phase 12A — PostgreSQL foundation

Status: **implemented, pending review, not accepted.** Root `PHASE` stays
`12.entry`. This does not start 12B or 12C. Draft PR #116 stays unmerged.

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

1. Inventory the current data directory (`PURGATORY_DATA_DIR`, otherwise the
   per-user application-data `Purgatory` directory). Supported inputs are
   `identity.json` schema 1 or 2 and `char_<16 hex>.json` schema 1. A `.bak`
   is read only when the destination file is missing. A lone `.tmp` fails
   closed. Any other file fails closed. PR #116 journals and schema v2
   character files are not imported.
2. Create one database. Do not create a database per channel. Create a
   non-superuser runtime role and a separate migration role. Keep both
   passwords out of the repository.
3. Before any production traffic, set and verify:
   - `fsync = on`
   - `synchronous_commit = on` (a replica needs `synchronous_standby_names`
     as well; `off` is not enough for an acknowledged commit)
   - `full_page_writes = on`
   - backups whose restore point is not behind an already acknowledged commit
4. For a remote host, use `sslmode=verify-full` with a private CA or the
   platform trust store, and restrict the network path. A local development
   server and pgAdmin are not that control.
5. Set `PURGATORY_DATABASE_URL` to the runtime role. Set
   `PURGATORY_DATABASE_MIGRATION_URL` when the migration role is separate.
   Neither URL is assumed to be localhost. Optional
   `PURGATORY_DATABASE_SCHEMA` defaults to `public`.
6. Start the server once. The persistence worker, not the 30 Hz tick:
   - applies migration `1` and refuses a changed or unknown history
   - imports the inventoried files in one transaction, or records a fresh
     database when the directory has no identity file
   - writes `durable_writer.json` with `{"schema_version":1,"writer":"postgresql"}`
7. Confirm character counts and ids. After that marker exists, opening the
   file writer fails. Later edits to the JSON files are not imported again
   and are not written by the server. Unset the URL only after an explicit
   repair plan; the marker will refuse the file writer rather than silently
   resume it.
8. A corrupt, unknown, duplicate, or unowned record aborts the import.
   Source files are left in place. No default character replaces them.
   A roster entry with no character file becomes a character row at revision
   1 with default restore and no items, facts, or grants. That is the current
   create-before-first-save case, not a repair of a broken file.

The worker's restore snapshot updates restore fields and revision only. It
does not delete item, fact, or learned-ability rows. On the pre-cutover file
writer, a character file with unknown fields fails to load and is not replaced.

## Tests

PostgreSQL tests are ignored unless this command is used with a dedicated
database:

```text
cargo test -p purgatory-persistence --lib postgres_tests -- --ignored --nocapture
```

`PostgresSettings::for_tests` refuses database names `purgatory_dev`,
`postgres`, `template0`, and `template1` before connecting. Each test creates
and drops a `p12a_` schema. It does not modify `Purgatory_dev`.

## Explicitly not in 12A

Startup ground retirement, account admission, one-active-character fencing,
live Drop, pickup, equip, NPC dialogue actions, ability grants from gameplay,
and client replies. Those belong to 12B and 12C.

## PR #116

Reused, in adapted form: item-instance identity, the six equipment slot names,
inventory capacity 20, content rules for stack limit, equip slot, and retired
definitions, structural rejection of empty quantities and reserved ids, and
fail-closed handling of corrupt records. The restore path was kept from erasing
state it does not own.

Rejected: the file journal, WAL frames, checkpoint manifests, the durable Drop
clock, map-drop restoration, and the WAL crash tests. Those implement the
superseded ADR-0068 rule.
