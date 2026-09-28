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
| `commit_ownership` | One ordered transaction with the complete post-state of every affected character and map drop. Acknowledged only after the frame is synced. |
| `reserve_item_instance_ids` | Durably advance the item-id high water, then return that range. Unused ids are gaps. Issued ids are not reused. |
| `checkpoint_active_clock` | Advance the monotonic active-server tick clock. Periodic checkpoints may move at most 30 ticks (one second at the simulation rate). Shutdown may sync any forward tick. |
| `map_drops` / character load | Read the recovered state. Retired or unknown content fails the read and leaves the files in place. |

A commit result is the transaction id plus the committed clock and id high
water. Gameplay success replies are 12B. The 30 Hz tick must not call these
operations; they perform file I/O.

## File backend, kept behind that boundary

The journal in `crates/persistence` owns:

- `char_{id}.json` schema v2 checkpoints
- `map_drops.json` and `durable_manifest.json`
- `ownership.wal` (`PGWAL001` header, length-prefixed JSON frames, CRC-32)
- tmp/bak replacement, `File::sync_all`, and directory sync where the platform allows it

Torn tails are discarded. A checksum failure before the end of the log fails
closed. Checkpoints are rebuilt from committed frames. After compaction the log
prefix is replaced only once the manifest fingerprint matches the checkpoint.
None of those file names or frame bytes are part of the domain contract.

## What a later consistent migration must copy and validate

Copy one committed image, not a mix of an old checkpoint and a newer log tail.
Validate it before switching the single authority. Do not dual-write.

| Data | Validate |
|---|---|
| Identity roster (`identity.json` schema v2) | Login keys, roster order, character ids, display names, `next_character_id` |
| Character checkpoints | Schema v2, `CharacterId`, revision, restore intent, instance exit, owned items |
| Map drops | Item id, definition, quantity, map content id, map-space key, drop id, position, expiry tick, public-eligibility tick, eligible characters |
| Item-id allocator | `reserved_through` is at least every live item id; the next reservation starts after it |
| Active-server clock | Committed tick, and each drop's remaining expiry and restricted window against that tick |
| Ownership log through the manifest's applied transaction | Contiguous transaction ids, checksums, and a fingerprint match with the checkpoint. A suffix newer than the manifest must be replayed or included in the image |
| Recoverable command results | Not stored in 12A. 12B must add them to this inventory before a cutover that claims idempotent retries |

Also check that each item id has one owner, revisions move forward inside the
image, and content ids are numeric catalog ids that the installed rules still
accept. Orphan or retired records fail the migration; they are not rewritten
as empty characters.

The database product, the cutover date, and any change that would retire this
file log are separate decisions. 12A does not add a database, a second writer,
or a generic repository trait.

## Crash and sync evidence

Tests reopen a store after complete frames, torn tails, a missing manifest, and
a compacted log. They do not pull power or inspect a physical disk cache.

`File::sync_all` is called on the log frame and on checkpoint temp files before
replace. On Unix the parent directory is also synced. On Windows the safe
standard library has no directory fsync, so `directory_sync_capability` is
`FileDataSyncedOnly`. A successful sync return is not proof against write-cache
loss or torn sectors. Do not describe a queued save, a rename, or a graceful
disconnect as power-loss durable.

If the process stops after the frame is synced and before the checkpoint files
are replaced, the next open replays the frame. If `commit` returns an error
because the checkpoint write failed after that sync, the frame is still
committed and the next open applies it. 12B owns the gameplay reply for that
window.
