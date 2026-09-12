# NPC authoring workspace

This directory contains canonical human-authored NPC source documents for the local NPC Lab.

## Status

Canonical NPC authoring workspace and live N10 dialogue source.

Files under this directory are recursively scanned by the current content
loader. Every document is validated; numerically allocated NPCs are projected
into the client-safe presentation registry, and Full server load additionally
creates authoritative dialogue definitions.

Current flow:

```text
NPC Lab (local)
-> versioned JSON here
-> validate authoring data and references
-> project to typed runtime/client definitions
```

## Source of truth

The JSON files in this directory are the source of truth for NPC Lab content. The tool must not require a private database to preserve NPC work.

## Current layout

```text
content/authoring/npcs/
  README.md
  welcome/
    npc.welcome.gate_watchman.json
    npc.welcome.shopkeeper.json
    npc.welcome.traveler_stayed.json
    npc.welcome.workshop_craftsperson.json
```

Area subdirectories are organizational only. Runtime world identity must not be inferred from a filesystem path.

## Contract

See [`docs/NPC_AUTHORING_CONTRACT.md`](../../../docs/NPC_AUTHORING_CONTRACT.md).
The complete runtime projection and ownership contract is
[`docs/NPC_DIALOGUE_RUNTIME.md`](../../../docs/NPC_DIALOGUE_RUNTIME.md).

Do not add generic scripting, quest ownership, or final behavior-tree semantics
merely because an authoring field exists here. Runtime-relevant fields must be
added through an explicit typed validation/projection change.
