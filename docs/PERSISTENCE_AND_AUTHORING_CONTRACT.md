# Persistence and authoring contract

This is the entry point for saving rules. It does not replace [`ARCHITECTURE.md`](ARCHITECTURE.md), [`DECISIONS.md`](DECISIONS.md), [`CONTENT_PIPELINE.md`](CONTENT_PIPELINE.md), [`ITEM_DOMAIN.md`](ITEM_DOMAIN.md), [`ITEM_AUTHORING.md`](ITEM_AUTHORING.md), [`MONSTER_DROP_CONTRACT.md`](MONSTER_DROP_CONTRACT.md), or the Phase 12 PostgreSQL contracts. [`PHASE_12_DURABILITY_CONTRACT.md`](PHASE_12_DURABILITY_CONTRACT.md) is historical and superseded. Do not reintroduce its character-file writer, custom gameplay journal, restored ordinary-ground timers, or obsolete import flow.

A line in this document is not an implementation. Each section says whether the behavior is implemented, required, tested, a known gap, or future policy.

## Ownership

| State | Canonical source | Writer / commit boundary | Database migration? |
| --- | --- | --- | --- |
| Item definition and presentation | `content/shared/items`, `content/shared/item_presentation`, `content/shared/equipment`, plus `crates/common/src/content_catalog.rs`, `crates/common/src/lib.rs`, and `content/CONTENT_ID_CATALOG.md` | Item Lab `commit_item_create` / `commit_item_save`. Commit is `AuthoringOperation.finish` after validation. | No |
| Monster drop table | Monster definition JSON under `content/definitions/monsters` | Mob Lab monster save and new-monster allocation. Same finish boundary. | No |
| Author notes and tags | `content/authoring/item_notes/<ContentId>.json` | Included in the Item Lab source revision and the same save. | No |
| Character-owned item identity, quantity, and location | PostgreSQL `item` rows through `purgatory-persistence` | `PersistenceService` transaction for the durable command. Admission into the queue is not this commit. | Only if that durable model changes |
| Character restore, health, and other implemented durable character state | Existing typed snapshot in the persistence service | Existing snapshot/command path. The live session must adopt the committed revision before a later item command. | Only if that model changes |
| Ordinary monster ground loot before pickup | Temporary world entity. The `ItemInstanceId` is reserved in PostgreSQL and then held in the server pool. | `manifest_monster_loot` after `reserve_item_ids` for the live channel generation. No ownership row is written at death. | No |
| Previously character-owned item dropped to the ground | Existing temporary ownership contract | Durable ownership transition through the current schema and recovery policy | Follow the current schema. No new migration in this change. |
| Entity, replication, target, and temporary map state | `World` and the replication/session owners | Runtime lifecycle. Not a database row. | No |
| Future quest progress, crafting outcomes, or economic exchanges | Not implemented. Requires an explicit domain contract first. | An atomic gameplay transaction when that domain is built | Decide when that domain is built |

## Versions

These are different tokens. Changing one does not change the others.

- Authoring source revision: SHA-256 of the editor-managed resource set (item gameplay, presentation, equipment or an absence marker, notes or an absence marker). Monster saves use SHA-256 of the monster file bytes (`X-Source-Revision`).
- Durable character revision: the committed character revision in PostgreSQL. Item commands send the revision the live session has adopted.
- Content schema version: item schema 3, presentation schema 2, equipment schema 2, monster schema 4.
- Database migration version: the persistence service migration history. Creation and migration stay on the explicit workflow. Runtime and migration roles stay separate. There is no automatic destructive migration and no reset-as-repair.
- Network protocol version: 34. This saving and loot correction does not change the wire format.

## Gameplay authority

`World` is the live authority. PostgreSQL, through the existing persistence service, is the only durable gameplay authority. Item Lab and Mob Lab must not read player database credentials or write player rows. There is no client authority and no alternate JSON or SQLite gameplay writer.

The simulation tick does not call the database or the filesystem. Reserved item ids arrive through `begin_id_replenish` and `InstallReservedIds`. `simulate_tick` must not call `begin_id_replenish`.

One deployment database is shared by channels. A channel claim records a generation. `item_id_reservations` are stamped with that channel and generation, and a later generation cannot spend the range. Character writes use the existing lease. Do not invent a database per channel. Details stay in [`PHASE_12_POSTGRESQL_CONTINUITY_CONTRACT.md`](PHASE_12_POSTGRESQL_CONTINUITY_CONTRACT.md) and `crates/persistence`.

### Command stages

1. Admission: the server accepts a command into the session queue.
2. Queued work: a snapshot or item command is waiting on the persistence worker.
3. Database commit: the persistence service transaction has committed.
4. Runtime application: `World` and the session adopt that committed result, including the new character revision.
5. Client success: the reply after runtime application.

A queued snapshot is not a committed save. A green authoring response is not returned until `finish`. A lost or uncertain gameplay reply is resolved through the existing durable command record before the economic effect is retried. The same command identity does not apply twice.

Unknown versions, invalid ownership or content, unsupported migration history, and unsafe restore fail closed. They do not silently delete, clamp, remap, create a blank character, or fall back to old storage.

`ItemInstanceId` is allocated by the persistence service. It is never derived from `EntityId`, `ContentId`, time, or a file counter. Issued and retired instance ids are not reused. Published and retired catalog ids are not reused. Gaps are valid.

One live item instance has one ownership location. Equipment in the world is derived from that record. Authoring does not write a second equipment model into the player database. Category changes, slot changes, and stack reductions that would invalidate saved instances are rejected in the Lab. There is no hidden player-data migration.

Ordinary unclaimed ground loot disappears on channel or server shutdown or crash. It is not restored or refunded. A successfully picked-up item remains character-owned across reconnect and restart. `close_world_address` abandons only an unmanifested remainder; its current callers are tests. Production channel stop uses `lose_all_authority`, which stops admission and then flushes pending loot.

Process-crash recovery of an authoring journal is tested. That is not a filesystem power-loss guarantee and not database disaster recovery. The journal uses per-file `os.replace` and does not fsync.

## Authoring save

`tools/authoring_save.py` is the shared journal for Item Lab and Mob Lab. Callers hold `CatalogWriteLock` (`content/.authoring-catalog.lock`) for the whole read, compare, and publish. The journal does not take that lock. Do not acquire the catalog lock twice on one thread. A timeout is HTTP 503, not success.

Order: validate the request shape and immutable identity; take the lock; recover any publishing journal or refuse; read the resource set; compare the source revision; validate the proposed content; stage every path the operation can change; publish; run the content validator; `finish`. `finish` writes status `complete` and deletes the journal. That is the commit. A crash while status is `publishing` restores every before-image on the next Lab startup or the next locked operation, even if the live bytes already match the after-image. Bytes that match neither image raise `AuthoringRepair` and block further saves. Files outside the operation are left alone. Journals are named by operation id so cleanup does not delete another operation.

The lock coordinates Item Lab and Mob Lab processes. Editors and Git that do not take the lock are not coordinated. Recovery will not overwrite unexpected newer bytes.

Creation registers one catalog id. A retried create with the same label and the same draft returns the existing item and does not allocate again. A provisional id that never reached `finish` is rolled back with the catalog bytes, so it was not published. Icon import checks the destination again under the lock and rejects an existing key or a file that is not a 32×32 RGBA PNG.

Item Lab health includes `workspace`. A matching build on port 8767 from another checkout is a conflict, not a reuse.

Stale drafts return HTTP 409 and stay in the editor. Reload is explicit.

## Loot randomness

Production rolls use `LootRng` (SplitMix64), seeded once from process entropy when `GameplayOwner` is created, and `purgatory_content::uniform_below` for bounded samples. The previous LCG plus remainder collapsed inclusive quantity 1–2 to a single endpoint. Chance and quantity each draw their own unbiased value. 0% never succeeds. 100% always succeeds. Rows are independent. A failed chance does not consume a quantity draw and is not an allocation failure. One death freezes one plan. Refill does not reroll.

Tests may call `seed_loot_rng_for_test`. A future runtime simulator must call `roll_monster_drops` with this generator. It must not keep a second formula. The simulator itself is not implemented. See [`DROP_EXPECTATION_GRAPH.md`](DROP_EXPECTATION_GRAPH.md).

## Compliance inventory

Implemented and covered by `tools/mob_lab/test_authoring_save.py` plus the production handlers:

- Item Lab create, save, and icon import.
- Mob Lab monster save and new-monster catalog allocation, using the same journal and catalog lock.
- Shared expected-drop formula in `tools/authoring_chart.js`.

Not claimed compliant. Do not treat this list as permission to rewrite them in an unrelated change:

- Mob Lab sprite-manifest saves.
- NPC Lab and other authoring tools that write content without this journal.
- Any editor that changes catalog files outside the lock.

## Persistence impact note

A change that writes authored content or durable gameplay state reports:

- Which data changes, and whether it is authored, derived, transient, or durable.
- Who owns it and which existing writer commits it.
- The success boundary, and what is still pending.
- How stale state, concurrent writes, and retries are handled.
- What happens on disconnect, process stop, partial failure, and restart.
- Whether a content, database, or protocol migration is needed, or explicitly not needed.
- Which tests prove those claims.

Credentials stay out of Git, logs, screenshots, printed commands, and this document.
