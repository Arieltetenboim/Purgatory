# PURGATORY — mini wiki

A compact orientation to the project. For authoritative current status, start at [docs/README.md](README.md).

## What it is

PURGATORY is a custom-built native **2D side-scrolling MMORPG** written in Rust. It does not use a general-purpose game engine.

Core direction:

- server-authoritative gameplay
- fixed-tick simulation
- native desktop client with `winit` + `wgpu`
- QUIC networking through Quinn
- PostgreSQL durable game state
- authored JSON/content separated from runtime state
- semantic replication rather than frame/bone replication
- dedicated authoring Labs for maps, characters, animation, NPCs, monsters, and items

## Runtime path

`client intent → server validation / GameplayOwner → World simulation → relevance & replication → client presentation`

`GameplayOwner` owns the authoritative `World` and player-binding lifecycle. The client may predict or interpolate presentation, but it is not a second gameplay authority.

## Main repository areas

| Path | Purpose |
|---|---|
| `apps/client` | native game client |
| `apps/server` | authoritative headless server |
| `apps/dev_hub` | Developer Hub and process/tool orchestration |
| `crates/simulation` | gameplay/world simulation |
| `crates/protocol` | client/server wire contract |
| `crates/content` | validated authored content |
| `crates/persistence` | PostgreSQL durability |
| `crates/animation` | animation runtime |
| `crates/skeleton` | character skeleton/pose math |
| `tools/map_lab` | map authoring/compiler workflow |
| `tools/Character part lab` | Character Lab |
| `tools/animation_lab` | animation authoring |
| `tools/npc_lab` | NPC dialogue/authoring |
| `tools/mob_lab` | monster/drop authoring |
| `tools/item_lab` | item metadata/presentation/icon authoring |

## Current state — 2026-10-03

- Version: `0.12.A`
- Phase marker: `12.12C`
- Protocol: v34
- 12A PostgreSQL foundation: accepted
- 12B save/load lifecycle: accepted
- 12C durable gameplay commands: accepted
- Phase 12 exit: **open**
- Item Lab V1 + Mob Lab drops + runtime loot: integrated
- Production UI foundations: inventory, Settings, and Glyphon text are merged

The exact remaining Phase 12 exit gates live in [ROADMAP.md](ROADMAP.md) and [PHASE_12_ENTRY.md](PHASE_12_ENTRY.md).

## Persistence

PostgreSQL is the only game database. The native client never connects directly to it. The server talks to persistence through the persistence worker.

Ordinary unclaimed ground is runtime state and is not restored after shutdown/crash under the current policy. Character-owned durable mutations use the PostgreSQL command/snapshot path.

Local setup: [POSTGRESQL_LOCAL_RUN.md](POSTGRESQL_LOCAL_RUN.md).

## Content and tests

Authored content is mutable data. A test of engine/runtime behavior should not silently depend on today’s sword name, description, animation offset, or monster drop table unless that exact authored value is the contract being tested.

Prefer synthetic/isolated fixtures for runtime semantics. Use real content only when the purpose of the test is to validate that content/integration.

## Working rule

`search narrowly → find the owner → prove the behavior → patch locally → test proportionally → record durable decisions → stop`

Before adding a manager, service, crate, or parallel mechanism, check whether an existing owner already owns that lifecycle.

## Where to go next

- Current work: [ROADMAP.md](ROADMAP.md)
- Architecture: [ARCHITECTURE.md](ARCHITECTURE.md)
- Decisions: [DECISIONS.md](DECISIONS.md)
- Quality/testing rules: [QUALITY.md](QUALITY.md)
- Test evidence: [TEST_GATES.md](TEST_GATES.md)
- Persistence/authoring rules: [PERSISTENCE_AND_AUTHORING_CONTRACT.md](PERSISTENCE_AND_AUTHORING_CONTRACT.md)
- Project development doctrine: [PURGATORY_PROJECT_CONTEXT.md](PURGATORY_PROJECT_CONTEXT.md)
