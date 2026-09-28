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
| Identity and character selection | Persistent roster and exact owned `CharacterId` selection; current login identity is `DevLogin`. | Re-entry uses the selected owner only, blocks duplicate sessions and never leaks one character's state into another. Document account/world ownership for future migration. | Production authentication, character deletion/rename, cross-world transfer. |
| Position, map and health | Restore intent is durable; entry spawns at a safe point with full health. World/channel/entity identity is runtime-only. | State the safe-point and death/respawn policy on logout, reconnect and crash. Keep transient combat state out unless a concrete rule requires it; test map/channel transitions. | Persistent injuries, penalties, buffs or exact logout position only when game rules require them. |
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

## Recommended Phase 12 gate expansion

Keep 12A–12C as the accepted Issue #10 implementation boundary, then add
explicit gates before declaring Phase 12 complete:

1. **12A — durable domain:** v2 item/character validation and migration,
   recoverable transaction log, map-drop records, stable item IDs and active
   Drop clock. No gameplay wiring.
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
   reconnect must agree with committed server state; DEV controls are not the
   acceptance proof.
6. **12F — recovery and acceptance:** backup/restore rehearsal and a normal
   two-client proof of grant→equip→Drop→pickup, dialogue reward→disconnect→
   reconnect/restart, character switching, crash at commit boundaries, queue
   saturation, retired content and map/channel/death handling. Record latency
   and storage pressure without claiming a production capacity from a local
   smoke test.

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
2. **Character/account/world ownership:** current `DevLogin` and selected
   CharacterId are not a production account model. Specify scope per future
   domain before adding shared bank, currency or cross-character benefits.
3. **Death and timers:** current entry uses full health/safe point; state this
   policy explicitly. Drop downtime pauses per Issue #10; other future item,
   reward, buff or quest expiry clocks require their *own* policy.
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
