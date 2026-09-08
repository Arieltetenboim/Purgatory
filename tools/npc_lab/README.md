# NPC Lab

Status: **FORGE N2 - Identity & Character Design (local working slice).**

NPC Lab is a local Web authoring tool for PURGATORY NPC content.

Repository JSON remains the source of truth.

## N2

The **IDENTITY** surface is the default authoring view and edits the existing
canonical NPC document directly.

It exposes:

- authored ID and schema (read-only in N2);
- working name;
- optional display name;
- role;
- area metadata;
- tags;
- background;
- personality;
- speech style;
- gameplay purposes;
- narrative purposes;
- relationships;
- design notes.

The **SHELL** surface remains available for raw JSON inspection/editing.

Fields outside the N2 surface, including `interaction.beats`, remain in the
same document and are preserved by form edits.

Canonical NPC authored content is English-only in the current game scope.
The local server rejects Hebrew Unicode characters on save.

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

## N2 gate

Using `npc.welcome.traveler_stayed`:

1. Open the NPC in IDENTITY.
2. Edit one scalar field and one list field.
3. Add/edit/remove a relationship.
4. Save.
5. Reopen and confirm identity/design data.
6. Open SHELL and confirm existing `interaction.beats` remain present.

Still out of scope: authored-ID rename/move, dialogue authoring, state/conditions,
behavior, presentation, voice, localization, runtime integration.
