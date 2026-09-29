# Quest domain readiness before Phase 12

Status: **research and design proposal**, 2026-09-29. This document does not
implement quests, allocate a new content ID block, or make NPC rewards durable.
Phase 12A remains an unmerged draft in PR #116. The later revised
[`PostgreSQL continuity contract`](PHASE_12_POSTGRESQL_CONTINUITY_CONTRACT.md)
reconciles the newly chosen rule that ordinary ground drops disappear on clean
shutdown and crash with the database cutover. The original Issue #10 file
contract remains historical; PR #116 must not merge unchanged.

## What the reference servers establish

These are community server implementations, **not Nexon's production server
schemas**. Borrow the separation of concerns, not their tables or their
failure guarantees.

| Source | Observed quest behavior and state | Limit for PURGATORY |
|---|---|---|
| [HeavenMS quest definition](https://github.com/ronancpl/HeavenMS/blob/master/src/server/quest/MapleQuest.java), [requirement types](https://github.com/ronancpl/HeavenMS/blob/master/src/server/quest/MapleQuestRequirementType.java), [action types](https://github.com/ronancpl/HeavenMS/blob/master/src/server/quest/MapleQuestActionType.java) | Quest content has separate start and completion requirements and actions. Status gates start/finish. A started quest may be forfeited; an explicitly repeatable completed quest may start again after its interval. Requirements include NPC, level, prior quest, item, monster count, money and time; actions include item, experience, skill and currency. | `forceStart`/`forceComplete` bypass normal checks, and reward actions run separately from status updates. These are not a safe durability or authorization contract to copy. |
| [HeavenMS SQL](https://github.com/ronancpl/HeavenMS/blob/master/sql/db_database.sql), [character load/save](https://github.com/ronancpl/HeavenMS/blob/master/src/client/MapleCharacter.java), [quest status](https://github.com/ronancpl/HeavenMS/blob/master/src/client/MapleQuestStatus.java) | `queststatus` stores character, quest, state, expiration, forfeits and completion count; `questprogress` stores objectives separately; skills and character stats live elsewhere. Load reconstructs the quest map and its objective progress. | A count and a current status are not enough by themselves to prove that the reward for each repeatable attempt was applied exactly once. Do not copy whole-character delete/reinsert saves into the live command path. |
| [Ms-v206 quest loader](https://github.com/YisusBGamer/Ms-v206/blob/main/src/main/java/net/swordie/ms/loaders/QuestData.java), [Quest](https://github.com/YisusBGamer/Ms-v206/blob/main/src/main/java/net/swordie/ms/client/character/quest/Quest.java), [QuestManager](https://github.com/YisusBGamer/Ms-v206/blob/main/src/main/java/net/swordie/ms/client/character/quest/QuestManager.java), [SQL](https://github.com/YisusBGamer/Ms-v206/blob/main/sql/1%20-%20InitTables_characters.sql) | Authored start/finish NPCs and conditions are distinct from character-owned quest status and objective requirements. Kill counts, item counts, level and money have separate types; start and finish rewards are separate. | Its loader explicitly skips numerous authored condition types; `completeQuest` assumes its caller checked eligibility. A table or type existing in this repo is not proof of complete rule enforcement or crash safety. |
| [Nexon Maple Guide](https://support-maplestory.nexon.com/hc/en-us/articles/204744405-How-does-Maple-Guide-work), [Guild Wars 2 renown hearts](https://wiki.guildwars2.com/wiki/Heart) | Level and prerequisite quests can gate access. Some other MMORPG activities are area participation or repeatable world events rather than NPC-owned quests. | Keep PURGATORY's future per-channel scheduled world events separate from a character's quest record; an event may *contribute* to a quest without being one. |

## Domain ownership and lifecycle

The **QuestDefinition** is immutable, versioned, validated authored content with
a permanent numeric `ContentId` once a Quest domain is explicitly allocated
under `docs/STABLE_NUMERIC_CONTENT_IDS.md`. The definition names optional giver
and turn-in NPC content IDs, start and finish conditions, objectives, repeat
policy, and start/finish actions. NPCs are interaction surfaces: one NPC can
offer several quests, a different NPC may accept turn-in, and some future
quests may be automatically started. `npc_met` and `dialogue_heard` also serve
non-quest narrative and must not automatically become quest status.

The **character** owns quest progress under stable `CharacterId` within its
world. A runtime `EntityId`, connection, channel or NPC entity is not the
owner. A user may own multiple characters but have only one active at a time;
their quest progress is separate. A future account-wide quest/achievement must
be a distinct explicit domain. Definition and player attempt are not the same
entity.

Suggested state vocabulary (server authoritative):

| Event | Durable character result | Rule |
|---|---|---|
| Eligible offer | None; availability is computed from definition and character | A declined offer does not create a quest attempt or consume a reward. |
| Accept at giver | Active attempt with stable quest ID and attempt number | Recheck all start conditions on the server; an already active attempt cannot start twice. |
| Objective event | Bounded progress on the active attempt | Only server-verified events with the intended character attribution count. |
| Objectives met | Still active; `ready to turn in` is computed | Inventory requirements are rechecked when the player finishes. |
| Abandon active attempt | Attempt closed; its counters reset for a later acceptance | Preserve historical completed attempts and claimed rewards. Quest-issued items need an explicit rule before start rewards ship. |
| Finish at turn-in | Completed attempt, claim record and **all** consumptions/rewards in one durable transaction | Recheck location, prerequisites and objectives; preflight inventory capacity. A retry returns the committed result. |
| Repeat when allowed | A fresh attempt, with its own reward claim key | A nonrepeatable completion permanently blocks a second acceptance. Repeatability requires an explicit reset/cooldown/limit rule, never a boolean that silently permits unlimited reward farming. |

The quest's public NPC marker (available/in progress/turn-in/completed/locked)
is a projection of its definition, conditions and character state. It must not
be persisted as another independent truth. Likewise, a dialogue fact that
controls the same quest transition must not independently grant its reward.

Typed conditions should initially include character level *when a real level
system exists*, prior quest completion, current item ownership/quantity and
relevant narrative facts. Condition groups need explicit AND and optional OR
semantics; unknown condition kinds fail validation. Use a stable objective key
within the quest rather than an array position for persistent progress.
Objective examples are server-credited monster kills, item turn-in and talking
to an NPC; equipment or map visits can be added for authored content that uses
them. Ordinary mob spawns reset at restart, while **committed character kill
progress remains**. A scheduled shared world event has its own per-channel
state and only emits verified credit to an active quest.

For a future PostgreSQL implementation, this suggests **logical** records
such as `character_quests` (character, world, quest, active attempt and durable
history), `character_quest_objectives` (attempt and stable objective key) and
`quest_reward_claims` (unique character, quest and attempt/reset key). The
authored QuestDefinition remains in the validated content catalog; the database
holds the player's changing state, not a competing copy of all quest rules.
One PostgreSQL transaction updates those records alongside affected item,
narrative, learned-ability and future currency/progression records. These names
describe responsibilities, **not** a schema migration or a commitment to a
table count, indexes or physical database topology. Do not write quest
progress from every simulation tick.

## Reward and crash boundary

The same transaction must include the finished attempt and its unique claim
key, removal of required items when the definition says they are consumed,
all item instance creation/transfers, narrative facts, learned abilities and
any later experience/currency reward. A failed capacity check or storage write
applies **none** of them; success is visible only after commit. A crash after
commit but before the client reply returns the same result on retry. An
already claimed attempt is never granted again, even if an item is subsequently
traded, consumed or dropped. For repeatable quests the key includes the stable
quest and attempt/reset period, not just the quest ID. Definitions must specify
whether a start reward exists; starting, abandoning and reaccepting cannot
reissue unbounded starter items.

An item carried by a player remains character-owned. On a live Drop, ownership
passes to a temporary map/ground owner; at shutdown/restart, the accepted new
product rule is to retire every remaining ordinary ground item, with no refund
to the former owner. The revised Phase 12 design now specifies ground
retirement on restart; PR #116 still needs rework before shipping either
quest rewards or Drop persistence. Monster drops may be transient until pickup;
no consumed item-instance ID may be reused. There is no need for an active
server clock solely to resurrect ordinary drops under the new rule.

## Existing Welcome content and migration

The [Welcome NPC JSON](../content/authoring/npcs/welcome/) already uses
`set_fact`, `mark_npc_met`, `give_item`, `remove_item` and `grant_ability`
to represent delivery and learning Dash. The server's
[`execute_dialogue_actions`](../apps/server/src/network/dialogue_actions.rs)
mutates narrative and inventory in a choice, while
[`NarrativeRuntime`](../apps/server/src/network/narrative.rs) is keyed by a
temporary actor and forgotten on detach. For example, delivery of
`item.package` changes a fact while removing an item; a different NPC grants
Dash. Treat these as the first *concrete* quest candidates, but inspect each
choice before converting its facts into quest progress. A persistent
`npc_met`/one-time line flag can remain independent narrative state. Do not
replace existing dialogue with a broad quest script engine or silently change
its outcomes.

Until the durable transaction includes character quest state, inventory,
facts and learned grants, a live reward quest cannot honestly promise
reconnect-safe completion. Content can be validated and transitions can be
specified before Phase 12; end-to-end reward activation belongs with the
durable domain. A new character begins with no prior quest history. Existing
DEV saves have no persisted narrative/quest history, so no migration can
truthfully invent which Welcome rewards were previously claimed.

## Chosen order and later Quest acceptance gates

1. **Reconcile Phase 12 design:** the revised PostgreSQL contract settles
   ground reset and the one durable transaction boundary while PR #116 stays
   unmerged. Quest domain research remains a proposal, not a live feature.
2. **Persist currently live state:** implement the revised 12A–12C and prove
   the existing items, restore state, NPC facts/met/heard and learned grants
   survive reconnect/restart as one committed outcome. Do not freeze a schema
   that prevents later typed Quest records.
3. **Return to Quests:** deliberately allocate a Quest content-ID block;
   identify one Welcome delivery quest and one explicitly repeatable test
   quest; define typed requirements, objectives, abandon/retry rules and
   versioned character attempt/claim migrations. Wire them through the same
   acknowledged transaction before offering durable rewards. A generic
   script engine and full journal UI are not prerequisites.
4. **Quest proof when implemented:** two characters under one user cannot share progress; two users
   cannot claim each other's reward; decline and abandon do not pay; prior
   quest/level/item gates reject invalid acceptance; a repeat pays once per
   eligible attempt; retry and crash at each commit/reply boundary cannot pay
   twice; inventory-full completion pays nothing; a killed mob's ordinary
   respawn does not erase committed progress; retiring quest content fails
   closed or follows an explicit migration.

Open product choices before runtime implementation: whether abandon deletes
quest-issued items, whether a claimed quest can be repeated immediately or
only after an authored wall-clock reset/cooldown, and how already owned item
quantities count toward a newly accepted collect objective. Those choices
must be explicit per quest where behavior differs. The immediate ordering
choice is settled: Quest gameplay follows persistence of the already live
earned state. These Quest-specific choices remain open until its runtime step.
