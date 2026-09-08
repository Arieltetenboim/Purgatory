# NPC Lab

Status: FORGE N0 workspace reservation.

NPC Lab will be a **local Web authoring tool** for PURGATORY NPC content.

No executable UI is implemented in N0.

## Intended ownership

```text
local NPC Lab UI
-> content/authoring/npcs/*.json
-> authoring validation
-> runtime projection later
```

The repository JSON remains the source of truth. NPC Lab must not require a private database or hosted service.

## Planned early slices

- N1 — Local Web Shell
- N2 — Identity & Character Design
- N3 — Dialogue Authoring
- N4 — Conditions & State Selection
- N5 — Dialogue Pools
- N6 — Simulation / Test Bench

Behavior, presentation, voice, and runtime integration follow only after real NPC content proves the required vocabulary.

See:

- `docs/NPC_AUTHORING_CONTRACT.md`
- GitHub Issue #17 — `[FORGE / N] NPC Lab — local authoring tool roadmap`

## N0 boundary

Do not add framework dependencies, Web build tooling, a server process, runtime loader integration, or UI code in N0.
