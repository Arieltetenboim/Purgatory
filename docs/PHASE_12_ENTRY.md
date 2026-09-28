# Phase 12 entry — Character Continuity

Status: **design accepted; 12A implementation pending review** (2026-09-28). Root `PHASE` = `12.entry`.
Phase 11 is closed. 12A is not a GREEN gate. 12B and 12C are not started.

## Entry evidence and first gate

- Issues #87, #88, #89 and #24 are closed. The current `master` has fail-closed
  character load, a bounded save handoff with latest-per-character pressure
  coalescing, and numeric catalog IDs for first-party item definitions.
- `PersistentCharacter` still has schema v1 with CharacterId, revision and
  restore/instance-exit state. Inventory/equipment are runtime-owned and not
  durable. Roster/selected-character entry already exist; do not rebuild them.
- Issue #10 is the **first Phase 12 step**. Its accepted design is
  [`PHASE_12_DURABILITY_CONTRACT.md`](PHASE_12_DURABILITY_CONTRACT.md):
  implement and verify it before promising durable inventory. This entry
  itself still implements no durable items.

## Issue #10 acceptance boundary

Trace authoritative runtime inventory/equipment through the single snapshot
projection, persistence worker, character record and enter-world restoration.
Specify the durable fields and identities, v1 migration, inventory/equipment
atomicity, revision ordering/coalescing, save acknowledgement and logout
guarantee, missing/retired content-ID policy, and fail-closed recovery. Explicitly
define reconnect, logout/login, clean restart, process crash, map/channel
transition and later character selection semantics. Distinguish queued,
written and crash-durable states; do not infer one from another.

Keep `EntityId`, `ConnectionId`, session/replication state, runtime
`WorldAddress` and coordinates out of the **character** record. Map-owned
Drop has a separate durable record with stable map-space identity, position,
pickup eligibility and active-time expiry; never persist its runtime entity.
Reuse the existing persistence worker and CharacterId ownership; no filesystem or
serialization work on the 30 Hz simulation thread. The design should name the
smallest implementation steps and their focused tests. An unresolved ownership
or durability conflict stops implementation for an explicit decision.

## Planned implementation stages

| Stage | Intended capability | Entry condition |
|---|---|---|
| 12A | Persistent Character State | Issue #10 contract accepted |
| 12B | Save / Load | 12A boundary and failure semantics verified |
| 12C | Inventory & Equipment Persistence | Numeric item IDs and atomic snapshot/recovery contract verified |

The stage names are roadmap intent, not a promise that the design must implement
them in this exact technical split. Update the roadmap and gates as each bounded
step is accepted. The FORGE W map pipeline remains a parallel track; no map
runtime acceptance is implied by this phase entry.
