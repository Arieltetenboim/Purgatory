# NPC Lab

Status: current local NPC authoring/test tool. N10 consumes its canonical JSON
through the separate typed runtime projection documented in
[`docs/NPC_DIALOGUE_RUNTIME.md`](../../docs/NPC_DIALOGUE_RUNTIME.md).

NPC Lab is a local Web authoring tool for PURGATORY NPC content. Repository JSON remains the source of truth.

## Current surfaces

### IDENTITY

Edits canonical NPC identity/design context: authored ID/schema, names, role/area/tags, background, personality, speech style, gameplay/narrative purposes, relationships and notes.

### DIALOGUE — N3 + N5

Edits `interaction.beats` including beat identity/priority, ENTRY or CONTINUATION role, dialogue, choices, explicit next/end transitions, notes and Dialogue Pool (`mandatory`, `once`, `repeatable`, `rare`, `lore`).

N5 pool semantics remain unchanged: mandatory follows normal selection; once/lore are suppressed after dialogue memory; repeatable remains eligible; rare is valid authoring metadata but deliberately not auto-selected until real cadence requirements exist.

### STATE — N4

Closed typed conditions: Fact, NPC Met, Dialogue Heard, Item Owned and Item Equipped. Choice actions: Set Fact, Mark NPC Met, Give Item and Remove Item.

Top-level selection remains deterministic: ENTRY only, all conditions AND, pool eligibility, highest priority, authored order for equal priority.

### TEST — N6a + N6b

The Test Bench provides editable synthetic character/world state and a conversation preview without launching the game client/server runtime.

Synthetic state fields: boolean Facts (`fact.id=true/false`), NPC Met, Dialogue Heard (`npc.id|beat_id`), Item Owned and Item Equipped.

`Start / Evaluate` sends the current authored NPC document plus synthetic state to the local preview API. The server uses the same `selection.py` evaluator covered by N4/N5/N6 tests; the browser does not own parallel selection logic.

#### Player-facing conversation preview

The central preview is intentionally separated from author/debug metadata. Its content contract is the content the future player dialogue surface is expected to consume:

- authored `design.display_name` only when one exists;
- every authored `lines[].text`, in authored order;
- every authored `choices[].text`, in authored order;
- choice selection follows the authored `next` path and applies the already-defined synthetic actions/state changes.

`working_name`, beat id/title, priority, pool, conditions, action records and diagnostic reasons are **not player-facing content** and stay outside the conversation surface.

When `display_name` is `null`, the Preview does not substitute the author-only `working_name` into the player-facing conversation. The authoring toolbar still identifies which NPC document is under test.

A no-choice beat uses a generic Preview `Continue` control to complete/end the synthetic conversation. That control is Test Bench chrome, not authored dialogue content.

This is a content-fidelity preview, not a frozen visual design for the eventual game dialogue HUD.

#### Why this dialogue? — N6b

Selection diagnostics are secondary authoring information and are collapsed by default. The compact view answers the useful questions first:

- which ENTRY beat won;
- which other ENTRY beats were eligible but lost on priority/authored order;
- which ENTRY beats are blocked, showing only their failed conditions or pool reason;
- how many continuation beats exist outside top-level ENTRY selection.

Full PASS traces are deliberately not shown in the default diagnostic view. CONTINUATION beats are hidden behind their own disclosure because they are not candidates for top-level selection.

Diagnostics come from `selection.explain_entry_selection()` and do not change selection semantics. They apply only to top-level `Evaluate`; explicit `next` transitions remain authored conversation flow.

No graph editor/visualization is part of N6.

### SHELL

Raw JSON remains available for inspection and repair. Structured surfaces preserve fields outside their editing scope. Authored NPC content is English-only in the current game scope.

## Run

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\npc_lab\run.ps1
```

Or:

**Developer Hub -> Content -> NPC Lab -> Launch NPC Lab**

If an older NPC Lab server is still occupying port 8765, stop its terminal/server first and launch again.

## Tests

```powershell
py -3 -m unittest discover -s .\tools\npc_lab -p "test_*.py"
```

The suite covers N4 typed validation/selection, N5 pools, N6a synthetic progression and N6b diagnostics. N6b tests prove diagnostics report the same winner as the selector, expose failed expected/actual condition values, explain one-shot pool rejection, preserve eligible-but-lower-priority status, and keep CONTINUATION beats outside ENTRY selection.

## N6 gate

Using the real Welcome NPC content, the Test Bench must allow currently authored NPC #3 states to be exercised deterministically without launching the game runtime.

For each tested state:

1. the central conversation shows the same authored player-facing text/choices and authored flow the runtime is expected to consume;
2. author/debug metadata does not leak into the player-facing content;
3. the author can inspect a concise explanation of why the top-level ENTRY beat was selected;
4. blocked alternatives expose only decision-relevant failures by default;
5. continuation flow remains distinct from top-level selection diagnostics.

## Known narrow limitations

- synthetic item mutation remains boolean ownership; Give/Remove Item currently supports quantity 1 only;
- `rare` automatic cadence remains intentionally undefined;
- raw-JSON Pool labels are still not mirrored by the legacy Python save validator;
- NPC Lab does not execute or own the N10 game runtime. Persistence and final
  game-client dialogue art remain out of scope; the game consumes saved JSON
  through `purgatory-content` rather than through the Lab Web server.
