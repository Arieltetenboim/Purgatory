# Phase 12 durability contract — Issue #10

Status: **proposed for owner review**, 2026-09-28. This is a design for the
first Character Continuity implementation, not a claim that item state is
currently durable. ADR-0068 records the ownership decision. Phase 11's
[`ITEM_DOMAIN.md`](ITEM_DOMAIN.md) remains the runtime item contract.

Evidence path: `crates/simulation/src/item_runtime.rs` and `world.rs` own items;
`apps/server/src/network/gameplay.rs` enters/detaches and projects saves;
`apps/server/src/network/persist.rs` handles delivery;
`crates/persistence/src/{character,repository,atomic}.rs` defines the record
and file replacement. The contract below follows these current paths.

## Current seam and gaps

`World` owns `ItemInstanceId -> ItemRecord { definition, quantity, location }`.
`ItemLocation` is one of world drop, inventory slot, or equipped slot; the
six-slot `EquipmentState` is a content-only gameplay/replication projection.
The server currently snapshots only CharacterId, revision, restore intent and
instance exit. `PersistentCharacter` schema v1 contains no owned items.
Selected-character entry loads the exact owned record on the persistence worker,
then spawns a player; it does not restore items. Accepted pickup/equip/drop
responses do not request a durable item save. Detach sends a restore-only save,
releases occupancy, and despawns the actor without waiting for a disk result.

The #88 load path rejects malformed/unsupported records, but
`FileCharacterRepository::save` currently ignores errors from reading the
existing record before replacement. The #89 handoff distinguishes accepted,
deferred and closed queue states and counts write failures, but has no per-save
success/failure acknowledgement; its deferred map can grow with distinct
CharacterIds, and failed writes are logged after removing the pending snapshot.
Shutdown ignores its timeout/result. None of these paths proves a successful
character save. `ItemInstanceId` minting currently mixes wall-clock time and
process ID into an epoch, then increments a counter; it does not prove global
non-reuse across restarts. These are implementation gates, not guarantees to
infer from the closed prerequisite issues.

`World::despawn` currently removes world-drop records for a despawned entity,
but does not remove that player's inventory/equipped records. Restoring the
same item IDs into a new actor therefore also requires explicit old-owner
cleanup after snapshot handoff and before re-entry; despawn alone is not proof
that the old runtime ownership disappeared.

## Durable record and ownership

The next character-file schema is **v2**, retaining v1's `character_id`,
`persistence_revision`, `restore`, and `instance_exit`. Add one `items` array of
character-owned records, conceptually:

```text
items: [
  { item_instance_id: u64,
    definition_content_id: u64, // numeric Item block, currently 30xxx
    quantity: u32,
    location: Inventory { slot: u16 } | Equipped { slot: EquipmentSlot } }
]
```

The exact serialization spelling is an implementation detail; the identity,
validation and migration semantics are not. Each item instance appears once,
has positive quantity within its current definition's stack limit, and occupies
one valid location. Inventory slots are unique and below the current capacity
of 20. Equipment slots are unique and among the existing six; an equipped
record must resolve to an equippable definition authorized for that slot.
Item and equipment facets share the same stable numeric `ContentId`. Serialize
in a deterministic order for review and repeatable tests. Do not persist a
second `EquipmentState`: rebuild its slot/content projection and equipment
ability grants from the canonical item records when entering the world.

Persist `ItemInstanceId` unchanged, but never `EntityId`, `ConnectionId`,
`WorldAddress`, runtime Channel/Instance, exact position, world-drop entity,
session input, replication state or presentation state. `CharacterId` is the
owner, independent of the runtime player entity. World drops remain outside
this character record. Learned abilities, narrative facts, currency, character
stats and account-wide inventory need separate scoped contracts; schema v2
does not silently promise them.

The existing persistence service is the sole writer of character records and
the durable item-ID allocator. Before minting any new item, it must durably
reserve a monotonic range of `ItemInstanceId` values; gameplay receives an
already reserved range and never waits for file I/O in the 30 Hz tick. Gaps
after a crash are valid; reuse is not. Exhaustion or inability to replenish a
range blocks new item creation. The allocator may use a separate small file
owned by the same worker, but the reservation must commit before an ID is
issued, and its recovery must fail closed. A restart scan of only live records
or a random/time epoch is not an equivalent non-reuse guarantee. Loaded
character records must also reject duplicate item IDs among active characters.

## One snapshot, one revision

After a successful item mutation, the simulation owner projects one **complete**
character snapshot from `World` and the binding's restore state at one tick
boundary. It selects inventory and equipped records for that actor, checks
unique IDs/slots, quantities and correspondence with `EquipmentState`, then
hands an owned immutable value to the existing persistence service. Never
write inventory and equipment as two independently committed records. A failed
projection is an explicit invariant failure; it must not produce a partial
snapshot. Mutations of restore and item state use the same revision sequence.

Issue a new strictly increasing revision after every accepted mutation that
changes durable state (pickup, equip/unequip, inventory move, consumption or
grant, character-owned drop, restore change). No revision bump for rejected or
duplicate commands. Detach and clean shutdown submit the latest full snapshot,
even if no new mutation occurred. Revision overflow fails closed rather than
using a saturating value indefinitely.

The worker keeps the greatest pending revision per CharacterId and can coalesce
older full snapshots. A revision below the committed one is stale; an equal
revision is idempotent only if the payload is identical, otherwise it is an
integrity error. An acknowledgement for revision `N` means the *whole* record
at revision `N` or a newer full snapshot committed successfully. Queue pressure
must remain bounded by admitted/pending characters; if retention is exhausted,
the server stops accepting further durable mutations for affected characters
and exposes a failure instead of silently losing a save. Failed writes retain
the latest pending snapshot for a bounded retry/recovery path and propagate
failure to the owner. Capacity must be checked before accepting a mutation;
an already applied change cannot be rolled back merely because the queue is
full. A projection failure suspends durable mutations for that actor and
retains its live state for diagnosis rather than acknowledging a partial save.
This does not add a second gameplay authority.

## Commit, recovery and lifecycle guarantees

| Event | Contract after implementation |
|---|---|
| Handoff accepted/deferred | Snapshot is held in process memory; **no disk guarantee**. |
| Worker replacement returns success | The character file has passed validation and recoverable tmp/bak replacement; the worker may acknowledge its revision. This is process-crash recovery, not a power-loss guarantee. |
| Write failure / worker closed / shutdown timeout | No success acknowledgement. Retain or report the unsaved state and block stale re-entry while the process lives; do not turn the failure into a default character. |
| Normal reconnect after completed detach | Enter only after the prior session's latest revision has committed, then load that record and restore the same item IDs into the new actor. |
| Logout / return to login | A user-interface claim of **saved** requires a server acknowledgement for the latest revision. The current disconnect control flow has no such acknowledgement, so its presentation must not imply this guarantee until the required control path is added. |
| Clean server shutdown | Flush latest snapshots and report success/failure within the configured bound; only a successful worker acknowledgement proves completion. |
| Process crash | Recover the last committed valid character record. Mutations accepted by gameplay but not yet committed may roll back. Never merge a partial inventory with a newer equipment state. |
| Map/channel transition | Keep the same CharacterId, item instances and authoritative actor ownership. Snapshot restore changes with the full item set; do not serialize runtime topology. |
| Later selection of another character | Load exactly the selected owned CharacterId; no inventory inheritance from the prior character or DEV login roster. |

The simulation tick never waits for disk. A pending detach must retain a
per-Character re-entry barrier and the complete snapshot until commit or an
explicit storage failure; releasing occupancy before that barrier exists can
load an older file into a second actor. After successful restoration, rebuild
canonical item records, inventory slots, `EquipmentState` and derived grants
before publishing owner inventory/replication readiness. A failed restore
must leave no spawned partial actor, items, occupancy or client Welcome.
Detach must clean the old actor's item records after a complete snapshot is
retained, while keeping its re-entry barrier until commit. This is runtime
cleanup, not an alternative durable item owner.

The current tmp/bak replacement syncs the temporary file, but does not provide
an explicit directory-sync or hardware power-loss guarantee. Do not label a
mere enqueue, completed rename or graceful disconnect as power-loss durable.

## Validation, migration and content changes

- On an existing v1 character, validate identity and restore fields, project
  `items=[]`, and commit a v2 record through the worker before entry. Preserve
  the CharacterId and restore intent; advance revision once for migration,
  with overflow/error handled explicitly. A missing record creates a v2
  default only for an owned character. Identity roster schema v2 is separate
  and unchanged.
- Unknown/future schema, malformed JSON, mismatch of file/CharacterId,
  duplicate IDs or slots, invalid quantity/capacity, unsupported location, or
  an impossible equipment projection fails closed and preserves original
  bytes. `save` must propagate an existing-record read/validation error rather
  than replace it. Failed migration is not a blank-character fallback.
- A missing/retired item `ContentId`, wrong numeric domain, missing equipment
  facet or changed stack limit invalidating saved quantity blocks entry for
  that character and preserves the record. Content restoration or an explicit
  versioned migration is required; no silent deletion, remapping or clamping.
- Validate numeric IDs against the stable catalog/retirement ledger; do not
  write legacy authored-string/Fowler–Noll–Vo (FNV) hash tokens into v2. No wire-format
  change follows from this file schema alone; review protocol version and
  golden vectors if acknowledgement or owner-private events change bytes.

## Smallest implementation and proof gates

1. **12A — record/allocator safety:** v2 parser, validation, v1 migration,
   deterministic item projection type, fail-closed `save`, durable ID-range
   reservation and restart recovery. Tests cover bytes preserved on every
   error, v1 identity/revision preservation, allocator gaps/non-reuse and
   missing/retired/wrong-domain IDs. No item gameplay wiring yet.
2. **12B — delivery/lifecycle:** bounded per-character latest snapshot,
   commit/failure acknowledgement, retry/pressure limit, detach re-entry
   barrier and shutdown result. Saturation, write failure, stale/equal
   revision, reconnect race, timeout and process-restart tests prove the
   stated guarantees. Introduce an explicit logout acknowledgement only if
   the UI promises a saved logout; review any protocol change separately.
3. **12C — World integration:** snapshot on every accepted durable mutation;
   atomically project/restore inventory and equipment, rebuild grants and
   owner-private presentation before Welcome. Test pickup→equip→disconnect→
   reconnect/restart, failed equip rollback, full inventory, multi-character
   isolation, old-owner cleanup/re-entry, map/channel transition and crash at
   each file-replacement boundary. Keep world drops transient unless separately
   approved.

### Owner decision before 12C

Dropping an owned item moves it to a transient world drop. If its removal from
the character file commits and the process crashes, the world drop is lost.
Likewise, an accepted pickup that has not committed can vanish on a crash if
its transient source drop is not recreated. These are item-loss windows, not
merely a visual rollback.
The proposed narrow Phase 12 policy accepts this explicit loss on a voluntary
drop and the acknowledged-but-uncommitted pickup window; it does not promise
persistent world loot. If that is unacceptable, a durable transaction/ack
boundary for pickup and world-drop persistence needs a separate design gate;
do not conceal it inside the character record. The owner must accept the
policy before 12C ships.
