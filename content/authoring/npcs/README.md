# NPC authoring workspace

This directory contains canonical human-authored NPC source documents for the local NPC Lab.

## Status

FORGE N0 workspace only.

Files under this directory are **not part of the current live runtime content pack** and are not scanned by the current content loader.

Authoring direction:

```text
NPC Lab (local)
-> versioned JSON here
-> validate authoring data
-> project to typed runtime/client definitions later
```

## Source of truth

The JSON files in this directory are the source of truth for NPC Lab content. The tool must not require a private database to preserve NPC work.

## Current layout

```text
content/authoring/npcs/
  README.md
  welcome/
    npc.welcome.traveler_stayed.json
```

Area subdirectories are organizational only. Runtime world identity must not be inferred from a filesystem path.

## Contract

See [`docs/NPC_AUTHORING_CONTRACT.md`](../../../docs/NPC_AUTHORING_CONTRACT.md).

Do not add runtime loader behavior, generic scripting, quest ownership, or final behavior-tree semantics merely because an authoring field exists here.
