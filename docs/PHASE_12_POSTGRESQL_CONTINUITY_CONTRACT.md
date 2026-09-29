# Phase 12 revised continuity contract — PostgreSQL

Status: **Phase 12 revised design**, 2026-09-29. The product rules
on ordinary ground drops and the choice of PostgreSQL were made after Issue
#10 closed. This document supersedes its file-backed implementation plan and
drop-recovery rule for future work. It does **not** accept 12A.
[`PHASE_12A_POSTGRESQL.md`](PHASE_12A_POSTGRESQL.md) is the pending
implementation record. ADR-0069 records the product change; PR #116 remains a
draft and must not be merged as the Phase 12 foundation.

## Scope and owners

- One user owns multiple `CharacterId`s; at most **one active gameplay
  character per user**. Validate roster ownership at entry. A switch waits
  for the previous character's accepted operations to settle. `DevLogin` is
  only a development identity, not production authentication. Account-level
  assets must have their own explicit owner; character rewards never leak
  through a shared roster.
- `World` is the live authority; PostgreSQL is the **only durable authority**.
  Keep database access in the existing persistence service/worker, off the
  30 Hz simulation tick. Do not keep the file journal as a second writer or
  assume that installing PostgreSQL/pgAdmin has connected the game to it.
- Character-owned durable state includes current restore intent,
  inventory/equipment item instances, meaningful NPC facts, NPC-met and
  dialogue-heard state, and learned ability grants. Derived equipment and
  intrinsic grants are rebuilt from their sources. Existing dialogue choices
  that change items, facts and learned grants commit all of them together.
  Current XP, level, currency, stats and Quest runtime are **not** already
  present; add them through versioned, atomic domain migrations when built.
- A world is a logical isolation boundary for character, economy and later
  world-event state. Only one world exists now. Channels are runtime placement,
  not separate databases. A scheduled event that must survive restart may
  store explicit per-channel state in its own future domain. Physical database
  count/topology and production authentication are separate deployment work.
  If multiple server processes can admit characters, the one-active-character
  fence must be shared and generation-checked; an in-process map alone does
  not enforce the rule across processes.

## Ordinary ground drops and map reset

| Event | Item and world result |
|---|---|
| Player drops an item | Commit the removal from the character and a temporary ground owner **atomically** before reporting success. It can be picked up by A/B/C according to server eligibility rules while the server is running. |
| Monster drops loot | The ordinary ground entity is temporary; killer/party exclusivity and disappearance timers are server-enforced during that run. A pickup must create/transfer durable ownership exactly once before reporting success. |
| Eligible pickup | Commit removal from ground and assignment to one character in one transaction; if expiry or another pickup wins first, reject. |
| Runtime expiry | Retire the unclaimed item; never return a player-dropped item to its previous character. |
| Clean shutdown **or crash** | All ordinary unclaimed ground drops disappear. On recovery, retire any persisted temporary ground owners **before** admitting gameplay; do not refund them or reconstruct their timers. No item ID may be reissued. |
| Map population after restart | Spawn the authored number of ordinary monsters afresh. Do not restore their death count or exact temporary map state. Persist character-owned credited progress separately if a future Quest requires it. |

A temporary ground owner is represented durably **where a previously owned
item leaves a character**, so loss of the runtime drop cannot resurrect the
item in the character. Runtime-only loot may remain transient until its
successful pickup, but minted persistent IDs must never be reused. The
database recovery sweep must be idempotent and fenced from live admission;
an old channel process must not write to a retired ground generation. The
normal online expiry timer is a gameplay timer. No persistent active-server
clock or pause-through-downtime rule is required for ordinary drops. Future
real-time item expiry or scheduled events need their own explicit clock rule.

## Atomicity and recovery

Each accepted economic or earned-state command has one server-validated
outcome, a stable idempotency key and an atomic PostgreSQL transaction. A
transaction changes all involved item owners, character revisions, dialogue
facts and learned grants, and stores the result needed to answer a retry.
Validate owner, content ID, quantity, slot, eligibility, capacity and
expected revision against the **committed** state; lock or conditionally
update the affected rows so two connections cannot both win. Enforce
uniqueness of each live item ID and of durable command/claim keys in storage.
An item that is consumed, merged or retired cannot become a new item later,
including after restart; allocator gaps are allowed. Stack splits mint a
fresh ID and conserve quantity. Cross-character Trade later uses the same
all-or-nothing rule for both participants and every exchanged asset.

The worker reports success only after the transaction commits. A queue
handoff is not success. Storage errors, conflicts and backpressure reject
the command without applying it in `World` or emitting success. Pending
actors/items are fenced until the result is known; detach and entry must
respect that fence. If the connection dies after commit but before reply, a
retry with the same key returns the committed result. If a commit's outcome
is unknown after a connection failure, read its durable command key before
retrying or rejecting; never blindly execute twice. After a process crash,
load the committed database state, clean ordinary ground owners, and only
then admit players. A committed operation that had no runtime application
before the crash is reconstructed from the database; an uncommitted one is
not claimed as successful. A logout UI may say **saved** only after prior
commands have settled and the server acknowledges it.

PostgreSQL transactions replace the custom file write-ahead log (WAL),
checkpoint manifest, file sync protocol and per-commit character-file
rewrites. Maintain PostgreSQL durability settings suitable for acknowledged
character/economic commits, especially `fsync` and synchronous commit;
database backups, restore and replica failover need an explicit recovery
point objective before production. A backup restored behind an already
acknowledged transaction cannot honestly satisfy this contract. Do not
equate an ordinary process-crash test with hardware power-loss or disaster
recovery proof.

Use a dedicated non-superuser runtime role and a separate controlled
migration role; keep credentials out of the repository. A remotely hosted
database requires an authenticated, encrypted connection with server
identity verification and restricted network access. The local development
database and pgAdmin connection are setup tools, not the production security
or availability design. See [PostgreSQL durability](https://www.postgresql.org/docs/18/non-durability.html)
and [TLS client configuration](https://www.postgresql.org/docs/18/libpq-ssl.html).

## Schema evolution and content

The database must model user-to-character ownership, character revisions
and restore state, item-instance identity and location, dialogue state and
learned grants with versioned migrations. An ordinary ground row, if stored,
is temporary state with a recovery-retirement path, not a permanently
restored map snapshot. Do not store runtime `EntityId`, connection,
replication/session state or a second canonical `EquipmentState`. Do not
freeze the entire future character in one irreversible schema: later typed
stats, skills, Quest attempts/claims and item-instance attributes get
explicit migrations and constraints. A Quest definition stays in authored
content; its character attempt, progress and reward claim become durable
when that gameplay is implemented. One turn-in transaction includes claim,
consumption, items, facts, abilities and any later currency/experience.

The cutover must inventory **all** existing v1 file character/identity
records and any 12A-format test data that is to be retained. Validate IDs,
roster ownership, revisions, restore fields and catalog references. Migrate
supported state once, prove counts and identities, and switch to one writer;
no silent blank-character fallback or concurrent file/database dual write.
Already lost NPC facts or learned grants cannot be invented by migration.
Unknown schema, invalid content, duplicate ownership or a missing migration
fails closed and preserves source data for repair. When a persistent NPC
dialogue marker uses an authored position rather than a stable semantic key,
settle its version/retirement policy before storing it as truth.

## Execution gates (replace the old 12A–12C implementation recipe)

1. **12A — database foundation:** versioned PostgreSQL schema/migrations,
   ownership and item-ID invariants, one durable commit/result boundary,
   fail-closed import of supported current records, and a persistence-worker
   integration. Prove crash/retry, consumed-ID nonreuse, owner uniqueness,
   bad-data refusal and connection/storage failure. Do not wire gameplay yet.
   Review PR #116 for reusable domain validation and tests, but replace its
   file journal and persistent ordinary-drop clock; do not merge it as-is.
2. **12B — save/load and lifecycle:** bounded admission and pending-result
   handling, per-user active-character lease/fencing, detach/re-entry barrier,
   startup ground retirement, authoritative restoration and shutdown status.
   Test same-user concurrent A/B entry and commit-before-reply crashes.
3. **12C — existing gameplay:** atomic Drop/pickup/equip/inventory and NPC
   item grants/removals, character-owned facts/met/heard and learned grants
   in the same commit as a dialogue choice. Test A→ground→B, crash after
   Drop with no ground return, crash after pickup, rewards across reconnect,
   eligibility, capacity, retired content and ordinary monster reset.
4. **Phase 12 exit:** prove the existing normal-client path, owner-private
   baseline/resync, failure visibility, two-character isolation, database
   recovery/backup rehearsal and measured load. Map/channel transitions must
   preserve character-owned state. Quests, XP/currency, Trade, advanced
   equipment, production login and persistent scheduled events each need
   their own later gameplay scope; nothing here claims they already work.

This reorders the old proposed 12D–12F gates: existing NPC rewards and
learned state belong in the same atomic gameplay integration, not a later
independent save that can contradict an item grant. Quest *domain planning*
has been recorded in `QUEST_DOMAIN_READINESS.md`; live Quest rewards follow
the durable transaction foundation.

## Before implementation review

- Define the concrete migration source and cutover for existing development
  records and the unmerged 12A test format. Treat the file branch as unmerged
  work, not deployed production data.
- Confirm the policy for damaged/dead logout and long cooldowns; current
  safe-point/full-health entry could permit a logout exploit. This does not
  block the storage-domain design but blocks a full Phase 12 exit claim.
- Measure connection limits, transaction latency, restart cleanup and restore
  time on the intended development workload; do not infer international
  production capacity from a local PostgreSQL installation.
