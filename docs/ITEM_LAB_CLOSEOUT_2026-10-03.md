# Item Lab V1 closeout audit — 2026-10-03

Status: **automated CI repair green; full acceptance still open for remaining manual evidence**. The content-coupled regressions identified below were repaired with isolated fixtures in PR #127. GitHub Actions run `37142280327` passed both required jobs on repair head `5afdc30389742a5c839ac54051c69585421d9720`. This audit
reviews master `8b3ccd5ee6440062f98e599fb987dfb1f20d39db` (`ITEM DONE?`).
PR #125 and the existing-equipment save fix in PR #126 are already merged.
This audit consolidates assets and documentation; it does not change gameplay,
save code, test assertions, protocol, or the phase marker.

## Evidence and limits

- The owner reported that the five-step practice-sword smoke worked, with
  the remaining failure being its inventory/ground icon. After starting Item
  Lab and importing the PNG through `/api/icons`, the owner reported it worked.
  This is owner-reported evidence, not a graphical session executed by this audit.
- The push keeps ContentId **30006** and label
  `equipment.debug.practice_sword`, associates `item.training_sword`, and
  includes `Graphic/items/item.training_sword.png`. The source PNG has the
  same Git blob. The paperdoll is a separate asset, not this 32×32 icon.
- `monster.dev.guaranteed_drop` now drops one practice sword at 100% and uses
  `creature.shroom`. The push includes `placement.mob_003` on `map.map1`.
  This is retained as the owner's development setup. It is not an isolated
  acceptance map and must be considered before shipping playable content.
- A direct API icon import does not prove file-chooser import. The screenshot
  and key/file mismatch history do not prove an upload-handler defect. The
  reserved `item.placeholder` file is ignored by the client, which synthesizes
  that placeholder; its duplicate PNG was removed only after byte equality
  with the retained source and `item.training_sword.png` was verified.
- No new evidence here proves native ItemInstanceId comparison, restart
  recovery on the owner's database, two-player eligibility, new-item creation,
  or both chart hovers. Previous automated and browser results remain attached
  to their actual commits in [the verification record](ITEM_LAB_V1_VERIFICATION.md).
- The runtime-based in-editor drop simulator remains deferred outside V1.
  The expectation chart and RNG tests do not implement that simulator.

## CI blocker history and repair

Both jobs failed in [run 37131696094](https://github.com/Arieltetenboim/Purgatory/actions/runs/37131696094)
on `8b3ccd5`. Their causes are authored-content assumptions in tests:

| Job / test | Observed failure | Required direction |
|---|---|---|
| Quality gate: `inventory_item_hover_tooltip_and_click_selection_share_visible_slot_mapping` | Client suite: 796 passed, 1 failed, 4 ignored. Expected 15 text blocks, got 16 after the sword gained a description. The same test also assumes the technical label is its displayed name. | Use deliberate, isolated item-presentation fixtures. Verify name/description tooltip behavior and visible-slot click mapping without depending on the owner's editable sword. Do not merely change 15 to 16. |
| PostgreSQL 12C: `postgres_12c_monster_death_pickup_survives_restart` | Server 12C: 19 passed, 1 failed. Expected scrap ContentId 30015; actual sword ContentId 30006. Its quantity-2 assumption also conflicts with the authored quantity 1. | Isolate the guaranteed drop fixture while preserving real death, live-channel ID reservation, durable pickup, and the same instance/quantity after restart. Do not skip the test or replace it with an in-memory injection. |

The 12A/12B PostgreSQL step passed. The failed 12C step prevented later
startup verification in that run. These failures do not establish a broken
gameplay save path, but current CI is not green and Item Lab is not fully closed.

### PR #127 repair

- Client: the hover/click mapping test no longer reads the editable practice-sword presentation. It uses an isolated synthetic/unknown item for hit-region and click mapping. A separate synthetic `ItemDefinition` / `ItemPresentation` test verifies display name, description, quantity, category, and stack tooltip semantics.
- Quality follow-up: once the client failure was removed, the canonical gate exposed a second masked server unit test, `authored_death_reserves_one_id_and_pickup_keeps_it`, with the same old scrap ×2 assumption. It now pins the same test-only drop fixture rather than reading the owner's current guaranteed-drop content. The original audit therefore correctly identified two failing CI jobs, but not every content-coupled assertion hidden behind the first Quality failure.
- Server/PostgreSQL: `GameplayOwner` accepts a monster-drop override only in test builds. The PostgreSQL restart proof pins its scrap ×2 fixture there, while production still resolves monster drops from the authored registry. The test still performs a real lethal player hit, production loot planning/roll, reserved ID manifestation, pickup, durable commit, logout, restart, and same-instance verification.
- No production content, catalog ID, database migration, protocol, phase marker, or gameplay rule is changed by this repair.
- Validation is recorded in `TEST_GATES.md`: the canonical Quality gate and PostgreSQL 12A/12B/12C job both passed in run `37142280327`. This closes the automated CI blockers. It does **not** replace the remaining concise Hub → Item Lab → Mob Lab → native kill/pickup/reconnect smoke or the separate icon-chooser check.

This audit ran `./scripts/check.sh`; it stopped at its first command because
`cargo` is not installed in this audit environment. That is **not** a local
quality-gate pass. Focused checks on the consolidation tree:

| Check | Result |
|---|---|
| Item Lab Python discovery | 7 tests passed. |
| Mob Lab Python discovery | 55 tests ran; 51 passed and 4 errored because real `rustfmt` is unavailable. This suite did not pass. |
| `node tools/test_authoring_chart.mjs` | PASS, `authoring chart ok`. |
| Restored art | All 19 Git blob identities match the source branch; all 18 restored PNGs decode successfully. |
| Branch bundle | `git bundle verify` and restoration into a fresh repository with only master ancestry succeeded; all eight saved heads matched. |
| Diff hygiene | `git diff --check` passed. |

These checks do not replace Rust/PostgreSQL CI.

## Current phase and recommended sequence

The authoritative [roadmap](ROADMAP.md) and root `PHASE` place the project in
**ERA II — Combat & First Playable Loop; Phase 12 — Character Continuity**,
marker **`12.12C`**. 12A, 12B, and 12C are accepted. The **Phase 12 exit is
open**. Human-facing `VERSION` remains `0.12.A`; protocol is 34. Tooling work
does not automatically advance those markers. README's stale 12C review and
protocol-31 statements are corrected by this consolidation.

1. Finish this bounded Item Lab closeout: isolate the two failing test
   fixtures, restore both CI jobs, and record one concise final Hub → Item Lab
   → Mob Lab → native kill/pickup/reconnect smoke. Verify the icon chooser
   separately. Keep unexecuted rows explicit rather than asking for the whole
   historical checklist again.
2. Finish the Phase 12 exit: owner-private baseline/resync and failure
   visibility, two-character isolation, a database backup/restore rehearsal,
   representative measured load, and an explicit long-cooldown policy.
3. Then scope a small Quest V1 using the existing NPC dialogue, Item Lab,
   monster drops, and the same acknowledged PostgreSQL transaction boundary.
   A starter collect-and-turn-in quest is a useful first vertical slice.
   Quest identities, attempt/claim rules and exactly-once rewards must be
   specified before implementation; see [Quest readiness](QUEST_DOMAIN_READINESS.md).

This is a recommendation, not authorization to start a new phase or implement
the deferred simulator/quest system. The historical master execution plan
does not override the current roadmap's phase numbering.

## Consolidation and preservation

The [branch archive](archive/branch-cleanup-2026-10-03/README.md) records the
21-branch audit, exact preserved refs, semantic decisions, and recovery steps.
Nineteen missing art/tileset files from `phase12/12c-gameplay-durable` are
restored at their original paths. Its unique Map 3 footholds and spawn are
preserved as archive JSON and in the Git bundle, not activated without Map Lab
projection/runtime verification. The old file-backed writer remains archived
and is not merged into the PostgreSQL runtime.

## Persistence Impact

This consolidation preserves Git history and art and updates status documents.
It does not call an authoring save endpoint, change ContentIds, modify the
catalog or the owner's database, introduce a migration, or change protocol 34.
The retained item/drop edits were already in the owner's `ITEM DONE?` push.
Normal Item Lab/Mob Lab saves continue to use the catalog lock and journal
commit at `AuthoringOperation.finish`; player-owned state remains PostgreSQL.
The governing rules stay in [the persistence and authoring contract](PERSISTENCE_AND_AUTHORING_CONTRACT.md).
