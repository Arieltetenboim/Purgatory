# NPC Lab

Status: **FORGE N4 - Conditions & State Selection complete.**

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

### DIALOGUE — N3

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
- validation for duplicate beat/choice IDs and broken `next` references.

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

N4 also provides a pure deterministic selection core in `selection.py`. It evaluates supplied synthetic character/world state using these rules:

1. only ENTRY beats participate in top-level selection;
2. all conditions on a beat must match (AND);
3. the highest integer priority wins;
4. equal priorities preserve authored JSON order.

Pool labels do not affect N4 selection. Their selection/randomization semantics remain N5.

Focused tests use the real Welcome NPC documents to prove that the selected package dialogue changes when the workshop NPC has already been met and when the player carries the package.

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

```powershell
py -3 -m unittest .\tools\npc_lab\test_server.py
```

The focused suite covers typed conditions/actions, explicit beat selection role, continuation transitions, broken references, duplicate beat IDs, English-only validation, path safety, AND condition matching, ENTRY-only selection, deterministic priority ordering, all five N4 condition types, and the real Welcome package selection paths.

## N4 gate

Using the Welcome package content, the evaluator must deterministically select:

- `workshop_package_unknown` at the Traveler when the workshop NPC has not been met;
- `workshop_package_known` after the workshop NPC has been met;
- `package_waiting_first_meeting` at the Workshop Craftsman when the package is still at the inn;
- `package_delivery_first_meeting` when the player reaches the Workshop Craftsman carrying the package for the first time.

N4 does not own dialogue pools, editable synthetic-state UI, selection diagnostics / `Why this dialogue?`, runtime dialogue UI/networking, persistence, or NPC runtime integration. Those remain N5, N6 and N10 work.
