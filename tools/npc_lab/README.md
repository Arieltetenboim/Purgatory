# NPC Lab

Status: **FORGE N6a - Synthetic Test Bench + Conversation Preview implemented; pending local verification.**

NPC Lab is a local Web authoring tool for PURGATORY NPC content. Repository JSON remains the source of truth.

## Current surfaces

### IDENTITY

Edits canonical NPC identity/design context: authored ID/schema, names, role/area/tags, background, personality, speech style, gameplay/narrative purposes, relationships and notes.

### DIALOGUE — N3 + N5

Edits `interaction.beats` including:

- beat ID/title/integer priority;
- ENTRY / CONTINUATION role;
- dialogue lines and player choices;
- explicit next-beat / end transitions;
- beat notes and preserved presentation refs;
- Dialogue Pool: `mandatory`, `once`, `repeatable`, `rare`, `lore`.

N5 pool semantics:

- `mandatory` — normal condition/priority selection;
- `once` — suppressed after this NPC beat has been heard;
- `repeatable` — remains eligible after being heard;
- `lore` — optional one-shot content;
- `rare` — valid authoring metadata but deliberately not auto-selected until a real cadence/probability requirement exists.

### STATE — N4

Closed typed conditions:

- Fact;
- NPC Met;
- Dialogue Heard;
- Item Owned;
- Item Equipped.

Choice actions:

- Set Fact;
- Mark NPC Met;
- Give Item;
- Remove Item.

Top-level selection remains deterministic: ENTRY only, all conditions AND, pool eligibility, highest priority, authored order for equal priority.

### TEST — N6a

The Test Bench provides editable synthetic character/world state and a conversation-facing preview without launching the game client/server runtime.

Synthetic state fields:

- boolean Facts (`fact.id=true/false`);
- NPC Met;
- Dialogue Heard (`npc.id|beat_id`);
- Item Owned;
- Item Equipped.

`Evaluate` sends the current authored NPC document plus synthetic state to the local N6a preview API. The server uses the same Python `selection.py` evaluator already covered by N4/N5 tests; the browser does not own a parallel selection implementation.

The preview shows:

- NPC working/display name;
- selected beat id, priority and pool;
- authored NPC dialogue text;
- player choices.

Choosing an option synthetically:

1. records the current beat in `dialogue_heard`;
2. applies the existing closed action vocabulary to synthetic state;
3. follows explicit `next` directly when present;
4. ends the synthetic conversation when `next` is null.

A beat with no choices exposes `Continue / End`, which records the beat as heard and ends the current preview conversation.

N6a intentionally does **not** add selection diagnostics / `Why this dialogue?`, rejected-condition explanations, graphs, runtime networking, persistence or game-client dialogue UI. Those remain later N6/N10 work.

Synthetic item mutation is intentionally boolean ownership only. `Give Item` / `Remove Item` support quantity `1`; stack/count simulation is deferred until real authored content requires it.

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

Run all focused NPC Lab tests:

```powershell
py -3 -m unittest discover -s .\tools\npc_lab -p "test_*.py"
```

The suite covers N4 typed validation/selection, N5 pools, and N6a synthetic progression including dialogue memory, explicit continuation flow, first-meeting `Mark NPC Met`, and the real Traveler package `Give Item` + `Set Fact` actions.

## N6a gate

Using the real Welcome NPC content, the local Test Bench must prove without launching the game runtime that:

1. synthetic state selects the expected ENTRY beat;
2. the selected NPC text and choices are visible as a conversation preview;
3. choosing `ask_place` from the Traveler intro marks the Traveler as met and previews `intro_place`;
4. choosing `take_package` from `workshop_package_unknown` adds `item.package`, sets `welcome.workshop.package_at_inn=false`, records the beat as heard, and ends the conversation;
5. completing `lore_roofs` records it as heard so a subsequent Evaluate can select repeatable `filler_food`.

## Known narrow limitation

The browser-side Pool editor validates allowed pool labels and the selector rejects unknown explicit values. The legacy Python save validator still does not mirror that Pool-label check for raw JSON edits. Keep this as a small validation-hardening follow-up rather than expanding N6a into unrelated server refactoring.
