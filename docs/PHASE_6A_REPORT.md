# PHASE 6A REPORT — Runtime Model

## Status

**GREEN**

## Quality gate

`./scripts/check.ps1` — `PURGATORY quality gate OK` (2026-08-29).

## Architecture introduced

- Runtime entities are composed optional capabilities on the existing generational slot vector. Not an ECS. Not class families (`PlayerEntity` / `MobEntity`).
- **Base (every live entity):** `WorldAddress` + lifecycle.
- **Optional:** `Transform`, `PlayerState`, `Platform`, `Health`, `ContentId`, `PersistentId`, `ReplicationMeta`.
- `EntityKind` is derived: player capability → Player, else platform → Platform, else Generic.
- `Transform` is not required. Logical / service entities may exist without a pose. Near-position queries skip them.
- Spawn goes through `RuntimeSpawnRequest`. Content-backed spawn attaches `ContentId`; transient spawn remains valid with neither content nor persistent id.
- Dirty tracking is per domain (`transform` / `health` / `membership` / `replication`), not `entity_dirty: bool`. Mutations mark; read-only queries do not. `consume_dirty` resets.
- Query extensions: `movable_in_address` (player capability), `replicated_in_address`, `interactable_near` (empty stub until 6B).
- Frequency / priority remain metadata only. No scheduler, no protocol bump.

## Important APIs / types

- `purgatory_simulation::{Health, DirtyFlags, RuntimeSpawnRequest, EntityKind::Generic}`
- `World::{spawn, set_transform, clear_transform, health_of, set_health, dirty_of, consume_dirty, movable_in_address, replicated_in_address, interactable_near}`

## Tests added

- `crates/simulation/src/phase6a_tests.rs` — composition, capability queries, content vs transient spawn, dirty vs query, health vs transform dirty, address membership dirty, despawn vs content identity, other-instance isolation, `interactable_near` stub, slot-index order
- Reusable `RuntimeFixtures` (test-only): player, mob-like, interactable, transient replicated, other instance, without transform

## Compatibility

- Protocol **v4** unchanged.
- `spawn_player` / `spawn_platform` still attach `WorldAddress::DEV`.
- Existing FOOTNOTE / Phase 5 / 6.0 tests remain on the same slot-vector World.

## Deferred (intentional)

- `Interactable` capability and `interactable_near` implementation (6B)
- Spatial AOI / scheduling / byte budgets (6D)
- Content registry / files (6C)
- Persistence (6E)

## Boundary

Work stopped at Phase 6A. Phase 6B may begin after this gate is GREEN.
