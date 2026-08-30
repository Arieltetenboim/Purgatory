# PHASE 6.0 REPORT — Runtime Foundation & Replication Contracts

## Status

**GREEN**

## Quality gate

`./scripts/check.ps1` — `PURGATORY quality gate OK` (2026-08-29).

## Architecture introduced

- `WorldAddress` (`MapId` + `ChannelId` + `InstanceId`) in `purgatory-common`. Independent of transform.
- Identity domains: `EntityId` / `RuntimeEntityId`, `ContentId`, `PersistentId`. `ContentId` compact token is not the 6C content contract.
- Lifecycle: `Active` vs `LeftWorld` vs despawn. Address change is not destruction. Observer relevance is not lifecycle.
- Query API on `World` (slot scan, index order): at address / map / instance / near / `relevance_for`.
- Replication metadata: class, priority, frequency tier, state vs event. **No scheduler.**
- SnapshotBuilder consumes `World::relevance_for(RuntimeEntityId)`. GameplayOwner maps `ConnectionId` → entity first.
- Full-world player broadcast remains the current result when everyone shares `WorldAddress::DEV`. It is not the final architecture (ADR-0034).

## Important APIs / types

- `purgatory_common::{WorldAddress, MapId, ChannelId, InstanceId, ContentId, PersistentId}`
- `purgatory_simulation::{RuntimeEntityId, EntityLifecycle, ReplicationClass, ReplicationMeta, World::relevance_for, World::set_address, World::leave_world}`

## Tests added

- `crates/common` WorldAddress equality / map / channel / instance
- `crates/simulation/src/phase60_tests.rs` identity, lifecycle, query, relevance, owner-only
- Server: `snapshot_relevance_excludes_incompatible_world_address`, `builder_consumes_world_relevance_set`
- Client debug snapshot asserts DEV address / no content / no persistent id

## Compatibility

- Protocol **v4** unchanged.
- Existing player snapshots unchanged when all players remain on `WorldAddress::DEV`.
- Platforms remain unreplicated (`ReplicationClass::None`).

## Deferred (intentional)

- Spatial AOI / grid (6D)
- Dirty tracking granularity (6A)
- Frequency scheduling, delta, byte budget
- Optional Transform (6A)
- Content registry representation (6C)
- Persistence (6E)
- Interaction (6B)

## Quality gate

`./scripts/check.ps1` — `PURGATORY quality gate OK` (2026-08-29).
