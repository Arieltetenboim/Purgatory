# NPC Lab

Status: **FORGE N3 - Dialogue Authoring complete; N4 condition authoring is in progress.**

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

### STATE — N4 work in progress

The current Lab already exposes a closed typed condition vocabulary:

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

This is authoring/validation work toward N4. **N4 is not considered complete yet** because the Lab does not yet provide the synthetic state evaluator required by the N4 gate to prove which ENTRY beat wins for a supplied character/world state and why.

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

The focused suite covers the current typed condition/action vocabulary, explicit beat selection role, continuation transitions, broken references, duplicate beat IDs, English-only validation and path safety.

## N3 gate

Using the Welcome NPCs:

1. Open the Traveler or Workshop Craftsman in DIALOGUE.
2. Select or create a beat.
3. Edit beat ID/title/priority and ENTRY/CONTINUATION role.
4. Add/edit/remove NPC dialogue text.
5. Add/edit/remove player choices.
6. Point a choice to another beat or END CONVERSATION.
7. Save and reopen the NPC.
8. Confirm the dialogue structure and transitions survive round-trip.
9. Confirm the workshop-package content contains distinct context-sensitive paths for Inn-first and Workshop-first play.

N3 does not own dialogue selection/evaluation, dialogue pools, runtime dialogue UI/networking, presentation playback, localization or NPC runtime integration.
