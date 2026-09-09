# NPC Lab

Status: **FORGE N5 - Dialogue Pools implemented; pending local verification.**

NPC Lab is a local Web authoring tool for PURGATORY NPC content.

Repository JSON remains the source of truth.

## Current surfaces

### IDENTITY

Edits the existing canonical NPC document directly and exposes:

- authored ID and schema (read-only);
- working and optional display names;
- role / area / tags;
- background;
- personality;
- speech style;
- gameplay and narrative purposes;
- relationships;
- design notes.

### DIALOGUE — N3 + N5

Edits `interaction.beats` without requiring raw JSON work.

Supported authoring includes:

- beat ID, title and integer priority;
- explicit ENTRY / CONTINUATION selection role;
- one or more NPC dialogue text lines;
- player choices;
- transition to another beat or conversation end;
- beat notes;
- preserved optional `voice` / `animation` references on existing lines;
- automatic update of choice transitions when a beat ID is renamed;
- validation for duplicate beat/choice IDs and broken `next` references;
- editable Dialogue Pool: `mandatory`, `once`, `repeatable`, `rare`, or `lore`.

The Welcome package content proves multiple context-sensitive paths across the Traveler and Workshop Craftsman NPCs.

### STATE — N4

The Lab exposes a closed typed condition vocabulary:

- Fact;
- NPC Met;
- Dialogue Heard;
- Item Owned;
- Item Equipped.

Choice actions currently include:

- Set Fact;
- Mark NPC Met;
- Give Item;
- Remove Item.

The pure deterministic selection core in `selection.py` evaluates supplied character/world state using these rules:

1. only ENTRY beats participate in top-level selection;
2. all conditions on a beat must match (AND);
3. pool eligibility is applied before priority comparison;
4. the highest integer priority wins;
5. equal priorities preserve authored JSON order.

### Dialogue Pool semantics — N5

The currently frozen pool behavior is intentionally small:

- `mandatory` — normal condition/priority selection; prior dialogue memory does not suppress it;
- `once` — eligible until this NPC beat has been heard once;
- `repeatable` — remains eligible even after being heard;
- `lore` — optional one-shot content; eligible until heard, then yields to other eligible content;
- `rare` — valid authoring metadata, but **not automatically selected yet**. No probability/cadence is invented until real content defines the requirement.

Pool memory uses the existing typed `Dialogue Heard` state rather than introducing a parallel progression fact.

The real Traveler content proves the N5 gate: after the character has met the Traveler and no higher-priority event is active, `lore_roofs` is selected first; after that beat is recorded as heard, `filler_food` becomes the repeatable fallback. No gameplay fact needs to change for this transition.

### SHELL

Raw JSON remains available for inspection and repair. Structured surfaces preserve fields outside their current editing scope.

Canonical NPC authored content is English-only in the current game scope. The local server rejects Hebrew Unicode characters on save.

## Run

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\npc_lab\run.ps1
```

Or:

**Developer Hub -> Content -> NPC Lab -> Launch NPC Lab**

## Tests

Run all focused NPC Lab tests:

```powershell
py -3 -m unittest discover -s .\tools\npc_lab -p "test_*.py"
```

The focused tests cover N4 validation/selection plus N5 pool behavior: one-shot dialogue memory, repeatable fallback, mandatory priority behavior, deliberately deferred rare selection, rejection of unknown pool semantics in the selector, and the real Traveler lore-to-filler path.

## N5 gate

Using the real Traveler NPC:

1. the character has already met `npc.welcome.traveler_stayed`;
2. no higher-priority package/caravan event is active;
3. before `lore_roofs` has been heard, it is selected over repeatable filler;
4. after `lore_roofs` is recorded in `dialogue_heard`, `filler_food` is selected;
5. the transition requires no progression fact mutation;
6. the Pool field can be edited and saved through the Dialogue surface.

N5 does not own editable synthetic-state UI, conversation preview, selection diagnostics / `Why this dialogue?`, graph visualization, runtime dialogue UI/networking, persistence, or NPC runtime integration. Those remain N6+ and N10 work.

## Known narrow limitation

The browser-side editor validates the allowed pool labels and the pure selector rejects unknown explicit pool values. The Python save validator does not yet mirror the pool-label check. This does not affect the structured Pool editor path, but raw JSON can currently reach server save with an unsupported pool label. Keep this as a small validation-hardening follow-up rather than expanding N5 into unrelated server refactoring.
