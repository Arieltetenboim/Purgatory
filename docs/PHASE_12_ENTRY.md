# Phase 12 entry — Character Continuity

Status: **revised design accepted; 12A pending review** (2026-09-29).
Root `PHASE` = `12.entry`. Phase 11 is closed. 12A is implemented and pending
review in [`PHASE_12A_POSTGRESQL.md`](PHASE_12A_POSTGRESQL.md). 12B and 12C
are not started. This entry does not accept Phase 12.

## Entry evidence and first gate

- Issues #87, #88, #89 and #24 are closed. The current `master` has fail-closed
  character load, a bounded save handoff with latest-per-character pressure
  coalescing, and numeric catalog IDs for first-party item definitions.
- `PersistentCharacter` still has schema v1 with CharacterId, revision and
  restore/instance-exit state. Inventory/equipment are runtime-owned and not
  durable. Roster/selected-character entry already exist; do not rebuild them.
- Issue #10 was the **first Phase 12 design step**. Its accepted design is
  preserved as history in [`PHASE_12_DURABILITY_CONTRACT.md`](PHASE_12_DURABILITY_CONTRACT.md).
  Later product decisions supersede its file-backed log and map-drop recovery.
  The revised implementation contract is now
  [`PHASE_12_POSTGRESQL_CONTINUITY_CONTRACT.md`](PHASE_12_POSTGRESQL_CONTINUITY_CONTRACT.md).
  Draft PR #116 cannot be merged unchanged. The 12A writer is pending review
  and is not accepted by this entry.

## Acceptance boundary carried into the revised design

Trace authoritative runtime inventory/equipment through the single snapshot
projection, persistence worker, character record and enter-world restoration.
Specify the durable fields and identities, v1 migration, inventory/equipment
atomicity, revision ordering/coalescing, save acknowledgement and logout
guarantee, missing/retired content-ID policy, and fail-closed recovery. Explicitly
define reconnect, logout/login, clean restart, process crash, map/channel
transition and later character selection semantics. Distinguish queued,
written and crash-durable states; do not infer one from another.

Keep `EntityId`, `ConnectionId`, session/replication state and runtime
`WorldAddress` out of the **character** record. Ordinary ground ownership is
temporary and is retired at startup, even after a crash. Restore current
character-owned items, NPC facts and learned grants together. Reuse the
existing persistence worker and CharacterId ownership; no database work on
the 30 Hz simulation thread. Follow the revised contract for implementation
steps and focused tests.

## Planned implementation stages

| Stage | Intended capability | Entry condition |
|---|---|---|
| 12A | PostgreSQL durable foundation | Pending review; not an entry condition for 12B until verified |
| 12B | Save / Load and lifecycle | 12A transaction and failure semantics verified |
| 12C | Existing item and NPC-earned state integration | Atomic commit/recovery contract and lifecycle verified |

The stage names are roadmap intent, not a promise that the design must implement
them in this exact technical split. Update the roadmap and gates as each bounded
step is accepted. The FORGE W map pipeline remains a parallel track; no map
runtime acceptance is implied by this phase entry.
