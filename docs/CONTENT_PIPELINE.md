# Content pipeline

Target authoring workflow:

```text
author data
→ validate
→ load into runtime registry
→ map author-facing string ID to runtime ID
→ simulation uses definition
→ client maps presentation references to assets
```

## Authoring location

Human-editable JSON lives under `/content`:

- `shared/maps/` — client-safe map geometry, bounds, spawn points
- `shared/entities/` — client-safe entity definitions (none required for 6C)
- `server/entities/` — server-only entities (interactables, portals with `transition: { map, portal }`)
- `server/placements/` — server-only placement lists keyed by map authored id

JSON must not contain numeric `MapId`, channel, or instance. The registry assigns `MapId` (FOOTNOTE / `map.dev.footnote` is pinned to `MapId` 1).

Each map authors a **restore** policy (`safe_point`, `checkpoint`, or `non_reenterable`). That is restore semantics, not a `WorldAddress`. Channel and runtime Instance are assigned by a separate placement layer (Phase 6E currently uses DEFAULT). See ADR-0044.

Development-only hard-coded fixtures (`World::footnote_test_stage`) remain for unit tests. Live server/client paths load this pack.

Presentation assets will later live under `/assets`. `/assets/dev` is reserved for placeholder/dev assets.

Presentation assets will later live under `/assets`. `/assets/dev` is reserved for placeholder/dev assets.

The existing `Graphic/` directory is not part of this pipeline yet. Phase 5.0B loads **only** `Graphic/LOGO.png` for the Connection Frontend (temporary filesystem path). Do not import, scan, or otherwise connect the rest of `Graphic/` to the runtime.

## IDs

Author IDs are stable strings, for example:

```text
monster.slime.green
item.consumable.small_potion
skill.basic.strike
```

At load time the runtime may assign compact internal IDs. Simulation uses definitions, not authoring files, on the hot path.

## Validation

`purgatory-content-validator` loads the workspace pack (`LoadMode::Full`) and:

- scans definitions
- reports duplicate IDs
- reports missing required fields
- reports broken references
- exits non-zero on invalid content

The quality gate runs it after `cargo test`. Invalid content must be detected before players see it.

## Runtime rules

- The server must not load image textures to understand a definition.
- Adding a normal monster, item, or skill is data plus optional presentation data.
- Engine source changes are required only when content introduces genuinely new behavior.
- Simple stat changes must not require rebuilding Rust once live/dev reload exists and is safe.

Portal links are content data: `transition: { "map": "<dest map authored id>", "portal": "<dest portal entity authored id>" }`. Arrival is at the linked portal, not the map's generic spawn. The destination entity does not need a reverse `transition` (one-way portals).

## Questions every content system must answer

1. What part is data?
2. What part is reusable behavior?
3. What part is authoritative simulation?
4. What part is client presentation?
5. Can a content author create a normal variant without editing core source?
6. Can invalid content be detected before players see it?
