# Phase 12 MMORPG continuity scope audit

Status: **review proposal**, 2026-09-28. This audit expands the *planning* view
of Phase 12; it does not change the accepted Issue #10 item durability contract,
start implementation, or claim feature parity with another game.

## What the comparison actually establishes

First-party game documentation shows that an MMORPG character is more than an
inventory file. MapleStory distinguishes character progression and quest gates,
world/channel membership, item tradability and storage eligibility, item/meso
trades, and per-equipment upgrades paid for with mesos. Its Heroic and
Interactive worlds have different transfer rules. FINAL FANTASY XIV separately
stores items and currency with retainers and restricts some operations while
visiting another world. World of Warcraft explicitly distinguishes account
progression/shared bank from character-specific progression. These are product
behaviors and scope boundaries, **not evidence of those games' server storage
implementations**. PURGATORY must define its own rules before building each
feature; it must not quietly copy a particular MapleStory world mode.

First-party references (consulted 2026-09-28):

- [Nexon: Maple Guide, level, quest and advancement gates](https://support-maplestory.nexon.com/hc/en-us/articles/204744405-How-does-Maple-Guide-work)
- [Nexon: worlds, channels and economy isolation](https://support-maplestory.nexon.com/hc/en-us/articles/204737355-What-are-worlds-and-channels-in-MapleStory)
- [Nexon: Heroic and Interactive rules](https://support-maplestory.nexon.com/hc/en-us/articles/22984919392788-What-are-Heroic-Interactive-and-Seasonal-Worlds)
- [Nexon: player trade of items and mesos](https://support-maplestory.nexon.com/hc/en-us/articles/204737455-Can-I-send-my-friend-items-or-mesos)
- [Nexon: storage depends on tradability](https://support-maplestory.nexon.com/hc/en-us/articles/204088469-What-kinds-of-items-can-I-put-in-storage)
- [Nexon: per-equipment enhancement and meso cost](https://support-maplestory.nexon.com/hc/en-us/articles/204088639-How-do-I-enhance-equips-with-Star-Force)
- [Square Enix: retainer item/currency storage](https://na.finalfantasyxiv.com/lodestone/playguide/option_service/additional_retainer/)
- [Blizzard: character-specific versus account-shared progression](https://worldofwarcraft.blizzard.com/en-us/news/24061008/the-war-within-warbands-preview)

## Gap inventory on current `master`

| State / operation | Proven today | Phase 12 continuity requirement | Feature beyond current continuity |
|---|---|---|---|
| User, roster and character selection | A `DevLogin` owns an ordered roster of up to three distinct `CharacterId`s today. Entry checks roster ownership and blocks a second session for the *same character*; separate characters under one DEV login can currently enter concurrently. `DevLogin` is not production authentication. | One logical user owns multiple characters but can control **only one active character at a time**. Recheck ownership on every entry; admit a character only when the user's previous active character and pending detach have settled. Save earned state under the selected CharacterId, never the login string or roster slot; never leak it across characters. | Production authentication, account-wide assets, character deletion/rename, cross-world transfer and any new roster-size policy. |
| Position, map, health and combat timers | Restore intent is durable; entry spawns at a safe point with full health. Ability cooldowns are keyed by runtime actor, and temporary effects are runtime-only. World/channel/entity identity is runtime-only. | Decide and test whether logout/reconnect while damaged or dead heals/revives the character, and whether an active cooldown resets; otherwise the current entry path can bypass those states. Define the safe-point/death policy and timer clock independently of Drop expiry. Keep transient combat state out of records only when its reset is an explicit game rule; test map/channel transitions. | Persistent injuries, penalties, full buff framework or exact logout position only when game rules require them. |
| Item inventory, equipment and map Drop | `World` owns item records; character schema v1 saves none; Drop/pickup/equip acknowledgements precede durable commit. | Issue #10: one durable ownership domain, item IDs, v2 migration, committed results, map-drop eligibility/active timers, equip projection and replay. Cover **every** existing item mutation, including NPC grant/removal. | Advanced stacks, enhancements, binding rules, account storage, marketplaces, full monster loot policy. |
| NPC facts and choices | `NarrativeRuntime` is keyed by ephemeral `EntityId`; entry seeds defaults, detach forgets all facts, met NPCs and heard dialogue. `execute_dialogue_actions` can grant/remove items and change facts in one choice. | Persist character-owned facts/met/heard in a versioned record. A dialogue choice that changes facts, items or learned abilities must commit **one** outcome and acknowledge after commit; reconnect cannot repeat the reward. Validate/retire authored fact/NPC/beat keys when content changes. | Full quest journal, branching campaign and generic quest engine. |
| Learned abilities | `AbilityGrantSource::Learned` is runtime-only; entry grants a basic ability and equipment grants are derived. NPC `GrantAbility` can add a learned grant, then detach loses it. | Persist learned grant IDs independently of intrinsic/equipment grants; rebuild derived grants on entry and show the correct owner-private baseline. Include learned grants in the same transaction as the reward choice. | Skill trees, points, jobs and respec rules. |
| Owner-private client state | I1 began read-only, but current `apps/client/src/app.rs` now routes production-window drag/double-click equip/unequip and Drop to server commands; `E` pickup also exists. Some inventory/grant baseline sends use bounded `try_send` and can fail. The roadmap's read-only/deferred-interaction text describes I1 history, not all current `master` behavior. | Enter/reconnect delivers a full authoritative inventory/equipment/grant baseline **before** readiness. Resync after pressure; a committed action becomes visible once, or returns a clear failure. Test the *existing* normal client interaction paths through save/reconnect. | Item use, full bags/sort UX, shops, cosmetics and advanced interactions beyond the proven loop. |
| Progression and currency | No authoritative XP/level/currency ledger in the inspected character/persistence/gameplay seams. Item domain explicitly keeps currency separate. | Record the boundary and reserve a versioned, atomic way to add such domains. Do **not** mark progression or economy persistent by saving empty placeholder fields. A future reward that grants XP/currency plus items must commit together. | Implement XP/level/stat formulas, meso-like balances, merchants, crafting and trade in separately scoped work. |
| Item-specific growth and restrictions | Current durable proposal is item ID, definition, quantity and location; no mutable upgrade/bind/expiry attributes exist. | Ensure a future versioned item-instance extension/migration can preserve attributes and enforce transfer eligibility at **every** exit (Drop, trade, storage, sale). Do not infer tradability from current category alone. | Item enhancements, random affixes, binding, real-time expiry and cash items. |
| Recovery and operations | Current file-save handoff lacks per-command commit result; `save` can ignore an existing-record read error; clean shutdown does not prove completion. | Issue #10 log/checkpoint proof plus corruption isolation, backup/restore rehearsal, revision conflict diagnostics, bounded queue/load behavior and observable failures. No successful client result for an uncommitted action. | Multi-process database, cross-region migration and live operations tooling only when evidence demands them. |

The source of the largest **newly exposed P0 gap** is the existing NPC choice:
`apps/server/src/network/dialogue_actions.rs` applies facts and item changes;
`apps/server/src/network/gameplay.rs` then grants learned abilities and emits
the accepted dialogue result. The learned-grant call's failure is currently
ignored. `apps/server/src/network/narrative.rs` forgets the actor's facts on
detach. If 12C makes only the item durable, a reconnect can restore the
granted item while losing its associated fact and learned grant; authored
conditions may then allow the reward to be claimed again. Saving these fields
in separate files or at separate times does not close this race. The accepted
Issue #10 document explicitly defers learned abilities and narrative facts,
so it cannot alone certify
**Character Continuity**.

## User, account and character boundary

The user's rule is **one user owns multiple characters**. The current
`crates/persistence/src/identity.rs` roster already implements that shape for
`DevLogin`, with a current limit of three entries. A roster entry contains a
stable CharacterId and a display name; its position and name are not durable
gameplay ownership keys. `load_owned_character` verifies membership before
loading the exact character record. The gameplay `occupancy` map prevents two
connections from controlling the same CharacterId at once. An existing
network test also proves that two different characters in one DEV roster can
be online simultaneously. **The product rule now supersedes this current
behavior: one active character per user.** The current login is a DEV name
supplied by the client, not a verified production user account; do not label
this a secure account system or change the three-character limit by inference.

- **Character-owned:** inventory, equipment, restore intent, narrative
  facts/met/heard, learned abilities, and future character XP/level/stats.
  Character A and B must each recover their own state even under the same
  user. A successful switch back to selection must settle or explicitly
  report pending writes for A before claiming A is saved; entry of B reads
  only B. Neither A nor B becomes enterable in another active gameplay
  session while the user's prior character has unsettled writes.
- **Map-owned:** a dropped item leaves A's inventory and belongs to the map.
  B may pick it up only through the normal server pickup/eligibility rules,
  whether B has the same user or a different one. Switching characters never
  copies an item directly. Its remaining active timer and map ownership
  survive a restart under Issue #10.
- **User/account-owned only when explicitly designed:** roster metadata and
  any future shared bank, achievements, benefits or balances. A shared benefit
  requires its own durable owner, authorization and transaction rules; do not
  infer sharing from a common login or copy character data into the roster.
  Future world/realm ownership must likewise be explicit before cross-world
  movement or a shared economy is offered.
- **Identity consistency:** reject a character load that is not owned by the
  requesting user, an identity/character ID mismatch, duplicate roster IDs,
  or an inconsistent recovery state. Define what happens to orphan records
  before any future delete, rename or account migration; never silently
  assign another user's items to an available roster slot.

**Accepted product rule (2026-09-28):** one user may own several characters,
but only one of them may be active in gameplay at a time. Enforce a user-level
admission lease keyed by the authoritative identity, in addition to the
existing per-CharacterId guard. Two connections racing to enter different
characters of the same user must not both succeed. Retain the lease while
the previous character detaches and its admitted durable commands settle;
reject or defer a new entry until the old state is safe to leave. Fence late
commands from an old connection after a new one enters. A front-end session
at character selection need not itself occupy a gameplay slot. Authentication
of the user belongs to a separate production-login gate; Phase 12 can prove
logical ownership with the current DEV identity without claiming account
security. The three-character roster cap is still a separate product choice.

## Later database migration boundary

**Requirement added 2026-09-28:** the 12A file-backed implementation must be
replaceable by a database without changing item ownership, CharacterId or
ItemInstanceId meaning, gameplay commands, or success/commit semantics. The
current persistence service/worker is already the natural boundary: expose
operations in terms of versioned character/map records, transaction commit,
ID reservation and active-server clock; keep file paths, JSON encoding,
log framing, checkpoint replacement and sync details inside its file-backed
implementation. Keep validation and single-owner invariants independent of
the storage encoding. The accepted Issue #10 log and checkpoint rules still
apply to **this** backend; a future database may implement the same atomic
contract using its own transaction mechanism.

Document a future cutover inventory and consistency boundary covering the
identity roster, characters, map Drops, allocator, active clock and eventually
recoverable command results. A later migration must copy a consistent
committed state, validate IDs, ownership, revisions and pending timer values,
then switch to exactly one authority with an explicit rollback plan. Do not
add a database dependency, speculative generic repository hierarchy, dual
writes or a migration tool in 12A. The specific database product and date of
cutover remain future decisions; if cutover is moved into Phase 12, revisit
the accepted file-log investment before implementation rather than silently
discarding it.

## Recommended Phase 12 gate expansion

Keep 12A–12C as the accepted Issue #10 implementation boundary, then add
explicit gates before declaring Phase 12 complete:

1. **12A — durable domain:** v2 item/character validation and migration,
   recoverable transaction log, map-drop records, stable item IDs and active
   Drop clock. Preserve storage-independent record/commit semantics at the
   existing worker boundary; document the later database cutover inventory.
   No gameplay wiring or database implementation.
2. **12B — save/load lifecycle:** bounded admission, commit/failure reply,
   reconnect barrier, idempotent retry, shutdown/replay and failure visibility.
3. **12C — item paths:** wire pickup, Drop, equip/unequip, inventory and map
   expiry through the transaction domain. Treat NPC item rewards as **not yet
   end-to-end durable** until the combined 12D gate; do not announce Phase 12
   completion from 12C alone.
4. **12D — narrative and learned progression:** migrate existing facts,
   met/heard markers and learned grants to CharacterId-owned durable state;
   atomically commit a dialogue choice's items, facts and ability grants.
   Rebuild grants and evaluate dialogue conditions from restored state.
5. **12E — normal client continuity:** use the existing production-window
   equip/unequip/Drop and `E` pickup paths. Owner-private inventory/grant
   baselines, resync after pressure, visible pending/success/failure and
   reconnect must agree with committed server state. Gate character entry by
   user-level occupancy until the prior character's detach is settled; DEV
   controls are not the acceptance proof.
6. **12F — recovery and acceptance:** backup/restore rehearsal and a normal
   two-client proof of grant→equip→Drop→pickup, dialogue reward→disconnect→
   reconnect/restart, same-user A/B state isolation, another user's rejection,
   duplicate-character entry, concurrent A/B entry rejection for one user,
   switch/save settlement before B enters, crash at commit boundaries, queue
   saturation, retired content, damaged/dead logout and cooldown reset,
   and map/channel handling. Record latency and storage pressure without
   claiming a production capacity from a local smoke test.

**Exit language:** `12A–12C GREEN` means durable items and their listed
commands, not all player progression. **Phase 12 Character Continuity GREEN**
also needs 12D–12F or an explicit, reviewed reduction of its advertised scope.
Trade, currency, XP/level, storage and item enhancement may remain future
features; the roadmap must name them, and PURGATORY must not claim MapleStory
feature parity while they are absent.

## Decisions to settle before the next implementation prompt

1. **Phase 12 completion bar:** recommend durability of *all currently
   player-visible earned state* (items, NPC facts/met/heard, learned abilities,
   restore) plus proof through the existing normal client loop. Defer building new XP, currency,
   Trade, storage and equipment-enhancement systems to named follow-ups.
2. **Character/account/world ownership:** one user owns multiple CharacterIds;
   only one may be active at a time (accepted). The current DEV roster caps
   them at three; confirm separately whether that capacity is an intended
   product rule. `DevLogin` is not production authentication. Specify scope
   per future domain before adding shared bank, currency or cross-character
   benefits.
3. **Death and timers:** current entry uses full health/safe point and resets
   runtime ability cooldowns. Decide explicitly whether logout while damaged
   or dead is free healing/revival and whether cooldowns restart on entry;
   do not accidentally create a logout exploit. Current Dash cooldown is only
   60 ticks, but longer future effects need their own rules. Drop downtime
   pauses per Issue #10; other future item, reward, buff or quest expiry
   clocks require their *own* policy.
4. **Content evolution:** dialogue-heard uses `(NPC ContentId, BeatIndex)`;
   reordering authored beats can change meaning. Require stable semantic
   identifiers or a versioned migration/retirement policy before persistence.
5. **Economy eligibility:** decide later whether the intended world is
   trade-permitting or self-found. Define bind/trade/storage/drop rules per
   item and world mode before shipping those paths; existing player Drop is
   public under Issue #10 unless an explicit future rule restricts it.

These are product decisions and proof gates, not a request to build all of
MapleStory in Phase 12. The first 12A coding task should remain bounded to
the accepted Issue #10 foundation while this wider plan is reviewed.
