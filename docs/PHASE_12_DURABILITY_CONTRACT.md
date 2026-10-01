# Phase 12 durability contract — Issue #10

Status: **historical accepted Issue #10 design, superseded for new work by**
[`PHASE_12_POSTGRESQL_CONTINUITY_CONTRACT.md`](PHASE_12_POSTGRESQL_CONTINUITY_CONTRACT.md).
It records why PR #116 used a file-backed log and restored ground Drops; those
requirements are no longer the product direction. Do not use its 12A–12C
recipe as the implementation gate or merge PR #116 unchanged. It was accepted
on 2026-09-28 and never claimed that item state was already durable.
ADR-0068 records that former decision. Phase 11's
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
An owned item can also be dropped by one player and picked up by another.
That is a transfer through map ownership between two characters, not a
single-character save. Future trade has the same atomicity requirement.

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
`WorldAddress`, runtime Channel/Instance, session input, replication state or
presentation state. `CharacterId` is the owner, independent of the runtime
player entity. Once dropped, the item belongs to the **map**, not its former
character. A separate durable map-drop record stores the item ID, stable map
content ID, logical map-space key and drop ID, position, expiry and pickup
eligibility measured in **durable active-server time** (including eligible
CharacterIds and when a restricted drop becomes public). The logical
map-space key distinguishes persistent channels/instances of the same map;
it must survive restart or have an explicit recovery mapping. The
drop's `EntityId` and runtime channel/instance IDs are not durable identity.
Player Drop is open to A, B or C unless an explicit game rule limits it.
Monster Drop can restrict pickup to the eligible killer/party for a
configured interval, then become public; the example of one minute is not a
hardcoded rule. Eligibility is server-checked at pickup and survives restart.
Learned abilities, narrative facts, currency, character stats and account-wide
inventory need separate
scoped contracts; schema v2 does not silently promise them. When currency
becomes tradable, it must enter the same atomic transfer domain.

The existing persistence service is the sole writer of character records,
map drops and the durable item-ID allocator. Before minting any new item,
it must durably reserve a monotonic range of `ItemInstanceId` values; gameplay
receives an already reserved range and never waits for file I/O in the 30 Hz
tick. Gaps after a crash are valid; reuse is not. Exhaustion or inability to
replenish a range blocks new item creation. Reservation is recorded in the
same durable transaction domain before an ID is issued, and that record names
the channel generation that may spend the range. A later generation cannot
spend it. Recovery fails closed.
A restart scan of only live records or a random/time epoch is not equivalent.
Loaded character and map-drop records must reject duplicate active item IDs.

## One transaction, one ownership result

The persistence worker serializes a **write-ahead transaction log** as the
durable authority for item ownership, map drops, item-ID reservations and
affected character revisions/restore state. Each transaction has a monotonic
transaction ID, an integrity-checked framed payload and complete post-state
for every affected character and map drop. The worker appends and syncs the
whole transaction before acknowledging it; incomplete tail records are not
commits. The v2 character files and map-drop index are derived checkpoints with
an applied transaction ID, never independently authoritative writes. Recovery
replays committed transactions after the checkpoint idempotently. Corruption
of a committed transaction or an inconsistent checkpoint fails closed; safe
checkpoint compaction retains the log until the replacement and its directory
entry are synced. Never discard a transaction until **every** affected
character/map-drop checkpoint has applied it, or a synced global checkpoint
manifest proves an equivalent consistent replay boundary. The implementation
must prove the append/sync/rename
boundaries on its supported filesystems before claiming power-loss durability.

For each command, reserve its affected item IDs and character actors, validate
the proposed `World` transition, project immutable complete post-state at a
tick boundary, and enqueue one transaction. No other mutation of reserved
state may overtake it. An acknowledged commit applies/releases the transition
in `World` and only then emits a successful gameplay result and replication.
On storage failure or bounded-queue exhaustion, release the reservation and
reject the command without applying the transition or reporting success. The
30 Hz simulation tick does not wait for disk; pending commands complete through
an asynchronous worker acknowledgement. A failed projection is an invariant
failure and cannot produce a partial record. The implementation must also
define how it settles a committed transaction if the server stops between
disk acknowledgement and runtime application.

Every committed durable change (pickup, equip/unequip, move, consumption,
grant, Drop, restore) advances the affected character revisions strictly;
cross-character transfer advances both in **one** log transaction. No revision
bump for rejected/duplicate commands. Revision overflow fails closed. Older
revisions are stale; an equal revision with different content is an integrity
error. Complete inventory and equipment state are projected together, and
`EquipmentState` is rebuilt from it. The existing latest-snapshot coalescing
may optimize checkpoint writes only: it must not discard uncommitted
transactions, transfer legs or pending result acknowledgements. Bound pending
transactions by admitted actors/items; backpressure rejects new commands
before state changes. The log and checkpoint are one logical authority, while
`World` remains the live gameplay authority.

The same rules apply to every economic item mutation, not just Drop: a grant
creates explicit quantity, consumption or expiry destroys it, and moves,
equips, swaps and future transfers conserve it. A stack split reserves a new
ID and preserves total quantity; a merge retires the consumed ID. Check
ownership, slots, capacity, quantity and content before committing, and
record the complete outcome atomically. Each economic command needs a stable
idempotency key and a recoverable result: reconnect/retry after a commit but
before its reply returns the previous result instead of executing again.
When shops, trades, mail, crafting or currency are implemented, any operation
that exchanges assets must commit all affected assets and owners together.
This contract does not implement those later gameplay features.

**Timer recovery:** the existing `World` timer scheduler runs on simulation
ticks. Persist a monotonic active-server tick clock through the worker and
record each Drop's expiry/public-eligibility tick against that clock. The
clock advances only while the server is running, so downtime consumes **zero**
Drop time. Checkpoint clock progress at least once per second while active;
if persistence cannot keep that bound, pause durable Drop timers and report a
storage fault. After a crash, resume from the last committed clock value:
the remaining lifetime and eligibility window can gain at most one second,
never reset to their full durations or lose offline time. A clean shutdown
syncs the final clock. Tests must prove the bound across crash/restart and
storage-pressure cases. This is the proposed precision of "the time left
before the crash" without synchronous disk I/O on every 30 Hz tick.

**Transfer rules, not a full Drop feature specification:** Player Drop
atomically moves an item from A to the map. The runtime map-drop entity is
visible/claimable after commit. Pickup atomically moves it from the map to an
eligible claimant (A, B or C) and removes that entity. Monster Drop creates
map ownership with its configured claimant window and expiry; future loot
rules may vary, but no pickup can bypass server eligibility. At its active-time
expiry tick, an unclaimed map item is **deleted**, never refunded to its
former owner. Pickup and expiry race through one serialized commit order, so
exactly one wins. An unexpired drop is recreated in its logical map-space
after a restart with the remaining active time and original eligibility rules;
downtime does not advance its timer. If the map cannot load, retain the record
and fail closed until the map is repaired or explicitly migrated; do not
silently return or delete it.
Future Trade uses one transaction for all exchanged items and both character
post-states, so no half swap is acknowledged. Trade UI, currency and economy
rules remain later work; the atomic transfer primitive is required now.

## Commit, recovery and lifecycle guarantees

| Event | Contract after implementation |
|---|---|
| Handoff accepted/deferred | Transaction is held in process memory; **no disk guarantee or gameplay success**. |
| Worker transaction commit acknowledgement | The entire transaction is synced and replayable; all affected character revisions and item locations commit together. Successful gameplay feedback follows this acknowledgement and runtime application. |
| Write failure / worker closed / shutdown timeout | No success acknowledgement. Reject pending mutations and report failure; block stale re-entry while unsettled state exists, never default the character. |
| Normal reconnect after completed detach | Enter after earlier committed transactions are settled, load the latest checkpoint plus log replay, and restore the same owned item IDs into the new actor. |
| Logout / return to login | A user-interface claim of **saved** requires settlement of all prior committed mutations and a server acknowledgement. The current disconnect flow needs that control path. |
| Clean server shutdown | Stop admission, settle/reject pending commands, sync the committed log and report success/failure within the configured bound. |
| Process crash | Replay complete committed transactions once: an acknowledged item is in exactly one character or map location, unless committed expiry deleted it. Uncommitted pending commands were never reported successful. Recreate unexpired drops with their last durable remaining active time and eligibility; downtime consumes no timer time. |
| Map/channel transition | Keep the same CharacterId, item instances and authoritative actor ownership. Snapshot restore changes with the full item set; do not serialize runtime topology. |
| Later selection of another character | Load exactly the selected owned CharacterId; no inventory inheritance from the prior character or DEV login roster. |

The simulation tick never waits for disk. A pending detach must retain a
per-Character re-entry barrier until all admitted commands settle; releasing
occupancy before that barrier exists can load an older state into a second
actor. After successful restoration, rebuild
canonical item records, inventory slots, `EquipmentState` and derived grants
before publishing owner inventory/replication readiness. A failed restore
must leave no spawned partial actor, items, occupancy or client Welcome.
Detach must clean the old actor's item records only after pending mutations
settle, while keeping its re-entry barrier until then. This is runtime cleanup,
not an alternative durable item owner.

The current tmp/bak replacement syncs the temporary file, but does not provide
an explicit directory-sync or hardware power-loss guarantee. The proposed log
must add the sync and recovery proof; until then, do not label a mere enqueue,
completed rename or graceful disconnect as power-loss durable.

## Validation, migration and content changes

- On an existing v1 character, validate identity and restore fields, project
  `items=[]`, and commit its migration through the worker before entry. Preserve
  the CharacterId and restore intent; advance revision once for migration,
  with overflow/error handled explicitly. A missing record creates a v2
  default only for an owned character. Identity roster schema v2 is separate
  and unchanged.
- Unknown/future schema, malformed JSON, mismatch of file/CharacterId,
  duplicate IDs or slots, invalid quantity/capacity, unsupported location, or
  an impossible equipment projection fails closed and preserves original
  bytes. `save` must propagate an existing-record read/validation error rather
  than replace it. Validate item IDs across characters and map drops after replay.
  Failed migration is not a blank-character fallback.
- A missing/retired item `ContentId`, wrong numeric domain, missing equipment
  facet or changed stack limit invalidating saved quantity blocks entry for
  that character and preserves the record. Content restoration or an explicit
  versioned migration is required; no silent deletion, remapping or clamping.
- Validate numeric IDs against the stable catalog/retirement ledger; do not
  write legacy authored-string/Fowler–Noll–Vo (FNV) hash tokens into v2. No wire-format
  change follows from this file schema alone; review protocol version and
  golden vectors if acknowledgement or owner-private events change bytes.

## Smallest implementation and proof gates

1. **12A — durable domain:** v2 validation/migration, fail-closed `save`,
   durable ID reservation, framed transaction log, checkpoint/replay and
   map-drop schema and durable active-server clock. Prove single owner after
   restart, timer recovery within the one-second bound, bytes preserved on
   errors, allocator non-reuse, content validation and crash/sync boundaries.
   No item gameplay wiring yet.
2. **12B — commit/lifecycle:** bounded transaction admission, per-command
   commit/failure acknowledgement, recoverable idempotency keys, actor/item
   reservation, revision ordering, detach barrier and shutdown result. Test
   reply-lost/retry, pressure, write failure, stale/equal revision, reconnect
   race, shutdown timeout and crash between
   commit and runtime apply. Review protocol changes for pending/success/error
   replies and saved logout separately.
3. **12C — existing gameplay integration:** wire current player Drop, pickup,
   equip/unequip and inventory moves to atomic transactions. Restore owned
   items, map drops and grants before readiness. Cover A→Drop→restart→B
   pickup, A→Drop→A pickup, A→Drop→expiry, paused-downtime timer recovery,
   pickup/expiry races, eligibility boundaries, same-map multi-channel
   isolation, full inventory,
   multi-character isolation, map unavailable and crash at transaction/checkpoint
   boundaries. This gate specifies the
   durability/ownership hooks for monster Drop, without building its full
   spawning, party entitlement or loot-rule system. Future Trade must use
   the same atomic multi-character primitive and prove all-or-nothing swaps.
