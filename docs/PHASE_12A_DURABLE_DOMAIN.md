# Phase 12A durable domain

Status: **implementation pending review**. This is not a GREEN gate. Pickup, Drop,
equip, dialogue, client replies and account sessions are not connected. The
network protocol, root `PHASE` and `VERSION` are unchanged.

The accepted rules remain [`PHASE_12_DURABILITY_CONTRACT.md`](PHASE_12_DURABILITY_CONTRACT.md)
and ADR-0068. This note records the file-backed boundary and the later
database cutover inventory.

## Domain operations

`PersistenceService` is the only production writer, and the existing persistence
worker is its only production caller. The operations are versioned
(`DURABLE_DOMAIN_VERSION`) and do not take paths:

| Operation | Meaning |
|---|---|
| Character load / v1 migration | Validate a v1 record, commit schema v2 with the same identity and restore intent, and advance revision once. Invalid bytes stay in place. |
| `save_snapshot` | Update restore, revision and instance exit. Committed items stay on the character until 12C sends them in the snapshot. |
| `commit_ownership` | One ordered transaction with the complete post-state of every affected character and map drop. Acknowledged only after the frame is synced. A new item must use an id that is reserved and not yet issued; an id that leaves every owner (consumed, merged, expired) is retired. |
| `reserve_item_instance_ids` | Durably advance the item-id high water, then return that range. Ids a process reserved and never used are abandoned when the store reopens; they stay gaps. No id becomes a new item twice. |
| `checkpoint_active_clock` | Advance the monotonic active-server tick clock. Periodic checkpoints may move at most 30 ticks (one second at the simulation rate). Shutdown may sync any forward tick. |
| `map_drops` / character load | Read the recovered state. Retired or unknown content fails the read and leaves the files in place. |

A commit result is the transaction id plus the committed clock and id high
water. Gameplay success replies are 12B. The 30 Hz tick must not call these
operations; they perform file I/O.

## File backend, kept behind that boundary

The journal in `crates/persistence` owns:

- `ownership.wal` (`PGWAL001` header, length-prefixed JSON frames, CRC-32)
- `char_{id}.json` schema v2 checkpoints and `map_drops.json`; each records the
  transaction that last changed it
- `durable_manifest.json`: the applied transaction, clock, id high water,
  unissued reserved ids, and the version and CRC of every checkpoint file,
  under its own CRC
- `.next` staging files, `File::sync_all`, and directory sync where the
  platform allows it

None of those names or bytes are part of the domain contract.

**Commit.** The repository recovers once at open and keeps the state in memory
with an item-owner index. A commit checks only the records it names, appends
one frame, and syncs it. It writes no checkpoint. A failed write is truncated
back to the last acknowledged length. If that truncation also fails, the store
refuses further writes until it is reopened. The store also refuses to append
when the log length on disk differs from what it last wrote.

**Checkpoint.** Each maintenance call does one phase, and at most 64 files:

1. Stage changed characters as synced `.next` files.
2. Write the manifest. This is the commit point. It names every checkpoint
   file, including ones that did not change.
3. Rename staged files into place, 64 per call.
4. Replace the log with one that starts after the manifest. Frames committed
   after the manifest, including a clock checkpoint during the install, are
   copied into the replacement. `rename` does that in one step, so the log
   name is never left empty.

The worker runs one phase when the log reaches 4096 frames or 8 MiB, then one
phase after each later command until the checkpoint is caught up. A clean
shutdown runs every remaining phase before it returns.

**Recovery.** Every file is read and checked before anything on disk changes:

- A checkpoint file is accepted only if its bytes match the manifest's CRC and
  version. A matching `.next` file is rolled forward. A `.next` file that the
  manifest does not name is left over from an uncommitted checkpoint and is
  deleted.
- The log must cover every transaction after the manifest. Suffix transactions
  are replayed with the same checks as a commit.
- Only an incomplete final frame is a torn tail, and only when its length
  field is a size the log would actually write (1 through 16 MiB) but the
  payload is short. A length of 0 or above that limit fails closed and is not
  truncated, even when the file ends before that many bytes and a later
  committed frame follows it. A complete frame that fails its checksum or does
  not parse fails closed, whether or not it is last.
- If `ownership.wal` is missing but a staged replacement exists, as can happen
  on a filesystem where replacing a file takes two steps, the staged log is
  installed only if it starts exactly after the manifest. A missing log with a
  manifest and no staged log fails closed.
- A v2 character file that the manifest does not list, a missing checkpoint
  file, or a tampered manifest fails closed without rewriting anything.

v1 records migrate at open in batches of 256 characters per transaction. The
old v1 file stays on disk until the first checkpoint that includes that
character. Reopen checks each old file against the first logged image of that
character. Those images are indexed once while the log is scanned.

## What a later consistent migration must copy and validate

Copy one committed image, not a mix of an old checkpoint and a newer log tail.
Validate it before switching the single authority. Do not dual-write.

| Data | Validate |
|---|---|
| Identity roster (`identity.json` schema v2) | Login keys, roster order, character ids, display names, `next_character_id` |
| Character checkpoints | Schema v2, `CharacterId`, revision, restore intent, instance exit, owned items |
| Map drops | Item id, definition, quantity, map content id, map-space key, drop id, position, expiry tick, public-eligibility tick, eligible characters |
| Item-id allocator | `reserved_through` is at least every live item id; the next reservation starts after it. Retired and abandoned ids below it are never issued again |
| Active-server clock | Committed tick, and each drop's remaining expiry and restricted window against that tick |
| Ownership log and manifest | Contiguous transaction ids and checksums; every checkpoint file matches the manifest's version and CRC. A suffix newer than the manifest must be replayed or included in the image |
| Recoverable command results | Not stored in 12A. 12B must add them to this inventory before a cutover that claims idempotent retries |

Also check that each item id has one owner, revisions move forward inside the
image, and content ids are numeric catalog ids that the installed rules still
accept. Orphan or retired records fail the migration; they are not rewritten
as empty characters.

The database product, the cutover date, and any change that would retire this
file log are separate decisions. 12A does not add a database, a second writer,
or a generic repository trait.

## Proven by tests

`crates/persistence/src/durable_tests.rs` stops the store at named crash
points, drops it, and reopens from disk:

- after checkpoint files are staged and before the manifest, both on the first
  checkpoint and after an earlier compaction
- after the manifest and before the staged files are installed
- after the replacement log is staged, including a missing `ownership.wal`
- an incomplete final frame, a complete final frame with a bad checksum, a
  frame length above the maximum with a later committed frame after it, and a
  corrupt frame before later frames
- an install stopped between batches, including a clock commit during that
  install
- a v1 file that does not match its first committed migration image
- a tampered checkpoint file, a tampered manifest, a deleted manifest, and a
  missing log

In each case the reopened store either replays to the committed state, where
every item has exactly one owner (a character or a map), or fails closed with
the bytes preserved. Retired and merged ids stay retired across restart and
compaction. A clock or single-character commit rewrites no other character's
checkpoint.

Per-commit cost no longer depends on how many characters are stored. The
timings are in [`PERFORMANCE_BUDGETS.md`](PERFORMANCE_BUDGETS.md).

## Limits

- Crash tests stop the process at chosen points in the code. They do not pull
  power, reorder writes in a disk cache, or tear sectors.
- `File::sync_all` is called on every log frame and on every staged file before
  the manifest names it. On Unix the parent directory is synced after renames.
  On Windows the safe standard library has no directory fsync, so
  `directory_sync_capability` is `FileDataSyncedOnly`: the ordering of renames
  after power loss rests on NTFS metadata journaling, not on an explicit sync.
  Do not describe a queued save, a rename, or a graceful disconnect as
  power-loss durable.
- If the filesystem zero-fills 8 or more bytes at the end of the log after
  power loss, they read as a complete zero-length frame. The store fails closed
  and needs an operator. Treating that as a torn tail could also discard a
  committed frame that was zeroed.
- If a failed append cannot be truncated, the frame may be replayed on the next
  open even though the caller saw an error. The store refuses further writes
  until then. 12B owns the gameplay reply for that window.
- One worker step at 10,000 dirty characters stayed under the one-second clock
  bound (slowest measured step 307 ms). The manifest write grows with the
  number of stored characters and was 12 ms at 10,000; it is still one call.
  A clean shutdown runs every step before returning: checkpointing all 10,000
  characters took about 15 seconds. Opening the store still reads every
  character file: about 0.94 seconds for 10,000 migrated characters before
  their first checkpoint, and about 1.0 second after it. Those startup and
  shutdown costs are the unresolved capacity limit of this file backend.
- The repository assumes it is the directory's only writer. The log-length
  check catches another writer's appends; it is not a lock.
