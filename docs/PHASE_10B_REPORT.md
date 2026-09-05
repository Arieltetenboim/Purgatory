# PHASE 10B REPORT — Target Acquisition & Approach

Status: **IMPLEMENTED**. Root `PHASE` = `10B`. Protocol **v15 unchanged**.

## Root cause

Phase 9E acquired live players and drove the existing ability runtime, but
`World::tick_npcs` only performed deterministic home-bounded NPC motion.
`World::drive_npc_combat` did not provide target-directed movement.

## Implemented scope

The existing live creature now supports:

`live player in acquisition radius → authoritative approach → stop at authored ForwardQuery range → existing ability runtime`

`World::tick_npcs_with_approach` remains the sole owner of NPC transform,
velocity, and heading updates. It performs a per-tick live-player query without
retaining target state. With no valid target it preserves the existing
idle/wander behavior. Dead or despawned targets therefore stop approach on the
next tick, and another live player can be acquired.

Approach stopping uses the live ability's existing `ForwardQuery` AABB, including
its forward-facing direction and authored vertical half-height. A Euclidean
distance threshold is insufficient because a target can be close enough by
distance while still outside the directional hit volume.
No protocol, navigation, client AI, retained aggro, or presentation behavior
was added.

## Changed

- `crates/simulation/src/runtime.rs` — target-directed movement in the existing
  NPC movement owner; approach stopping reuses `forward_query_aabb`, and the
  combat driver no longer writes competing heading state.
- `crates/simulation/src/phase9e_tests.rs` — focused approach, stop, dead,
  despawn, reacquisition, and offset-target hit coverage.
- `apps/server/src/network/gameplay.rs` — passes the authored live ability
  range and vertical half-height into the authoritative NPC movement step.
- `docs/PHASE_10B_REPORT.md` — implementation record.
- `docs/ROADMAP.md` — 10B status.
- `PHASE` — advanced to 10B.

## Verification

Commands run:

```text
cargo test -p purgatory-simulation --lib phase9e
cargo test -p purgatory-server --bin purgatory-server network::gameplay::tests::live_creature_acquires_player_and_uses_basic_strike_runtime
cargo test -p purgatory-simulation --lib
cargo test -p purgatory-server --bin purgatory-server network::gameplay::tests --no-fail-fast
powershell -ExecutionPolicy Bypass -File .\scripts\check.ps1
```

All passed. A normal-session manual observation remains useful for visual and
timing confirmation but was not run in this session.

## Boundary

No Phase 10C work was started.
