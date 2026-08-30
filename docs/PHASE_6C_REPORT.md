# PHASE 6C REPORT — World + Content Runtime

## Status

**GREEN (automated).** Portal refinement is inside 6C. Portal input routing fix is inside 6C. Manual portal check is **not** user-confirmed.

**PORTAL FADE TIMING READY FOR USER CHECK** — not `MANUAL RUNTIME PASSED`. Empty-replica MapId 0 must not trigger a content lookup.

Do **not** start Phase 6D.

## Quality gate

`./scripts/check.ps1` — **PURGATORY quality gate OK** (2026-08-29). `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo run -p purgatory-content-validator -q`.

## Architecture introduced

- Canonical `ContentId` is the authored string. Compact FNV-1a `u64` is storage only (ADR-0036).
- Shared vs server-only content domains. Client loads `content/shared` only; server loads Full.
- Registry is the sole `Map ContentId ↔ MapId` mapping. `map.dev.footnote` is pinned to `MapId::DEV` (1).
- Typed spawn plans in `purgatory-content`; `purgatory-simulation` stays JSON-free.
- `World::instantiate_map` preflights, then spawns; rollback despawns the batch on spawn failure. `ensure_map` / `destroy_map` are the lazy lifecycle APIs. `GameplayOwner` eagerly instantiates Map A and Map B for the two-map smoke.
- Generic interactables still use E / `InteractOpen`. Portals are excluded from the E nearest-target set. Portals use Up Arrow / `PortalActivate` with a small centered activation zone and a post-arrival reentry lock. Overlay distinguishes nearest generic, nearest portal, and portal eligible. The ~13.3 overlay distance while standing on the triangle is player-to-chest, not a portal-zone mismatch; range constants were not loosened.
- Protocol **v7**: observer `WorldAddress` on snapshots (from v6) plus portal kind and `PortalActivate`. Address change resets client replica/interp/prediction baseline and rebuilds local platforms from shared map geometry. Missing MapId is a hard client failure. Client fade is presentation-only (`FadeOut` 300 ms → black hold 75 ms while the new baseline is applied → `FadeIn` 400 ms) and does not block simulation.

## Important APIs / types

- `purgatory_common::{ContentId::from_authored, MAP_FOOTNOTE_AUTHORED, MAP_SECOND_AUTHORED}`
- `purgatory_content::{ContentRegistry, LoadMode, load_registry, map_plan, geometry_plan, TransitionRef}`
- `purgatory_simulation::{World::instantiate_map, World::ensure_map, World::destroy_map, World::transition_entity, World::validate_portal_activate}`
- `purgatory_protocol::{WorldSnapshot::{local_map, local_channel, local_instance}, ReplicatedKind::Portal, PortalActivate}`

## Tests added

- Content: malformed JSON, unsupported schema version, workspace pack vs FOOTNOTE geometry, shared-vs-full domains, Map A+B instantiate, unresolved/unplaced dest portal
- Simulation: empty-plan preflight, already-instantiated, two addresses of the same map, destroy keeps players, transition, address-filtered collision, authored ContentId without PersistentId, portal zone / E-reject / reentry lock
- Server: A→B arrives at B portal, B→A after zone exit, E does not activate portal, outside zone reject, invalid dest, generic E still opens, Map A relevance drop
- Client: triangle portal quads, E ignores nearer portal and does not send `InteractOpen` for a portal, E still opens switch/chest, Up outside zone sends nothing, Up inside zone sends traversal, fade out/hold/in timings, fade-in waits for baseline, Up edge not held-repeat
- Validator: workspace pack load
- Protocol v7 goldens (v1–v6 frozen)

## Compatibility

- `PROTOCOL_VERSION` **7**. v6 peers are rejected at Hello.
- `World::footnote_test_stage` remains for unit tests. Live server/client paths load the JSON pack.
- Interpolation still presentation-only and still interpolates **players** only.
- Client still must not spawn authoritative interactables/portals from content (replica-only draw).

## Deferred (intentional)

- WindowManager / production UI
- Combat / inventory / MOB AI
- Persistence (6E)
- Spatial AOI / replication scheduler / deltas / byte budgets (6D)
- Effect framework / shaders / particles (fade is a tiny replaceable overlay)

## Smoke (runtime visibility — not yet user-confirmed)

1. Triangle portal visible on Map A (replica entity, not a local fake).
2. Up Arrow outside the portal center does nothing.
3. Up Arrow while centered → old map fades to black (~300 ms), brief black hold, then Map B fades in (~400 ms).
4. Map B appears at the linked portal (not the generic spawn). The new map must not pop in before the overlay is fully black.
5. Holding Up does not bounce back immediately.
6. New Up press after leaving/releasing returns through the linked portal.
7. E interactions still work on switches/chests.

## Boundary

Work stopped at Phase 6C portal fade timing. **Do not begin Phase 6D.**
