# PHASE 9E REPORT — Minimal Creature Combat Driver

Status: **GREEN**. Root `PHASE` = `9E`. Protocol **v15 unchanged**.

## Implemented scope

The existing live NPC composition now supports:

`live creature → living player acquisition → request Basic Strike → Ability Runtime → damage → Hurt/Dead`

NPC acquisition is deterministic nearest-player selection within a small same-address radius. The driver updates authoritative NPC facing and calls `World::request_ability` with no selected target. Ability lifecycle, ForwardQuery delivery, timing, cooldown, and effects remain in the existing runtime.

One stationary visible combat creature is spawned through the normal server runtime spawn path when a live session is attached or entered. It has Transform, Health, `NpcState`, and the authored `skill.basic.strike` grant.

## Changed

- `crates/simulation/src/runtime.rs` — player-only acquisition and minimal NPC combat driver.
- `crates/simulation/src/phase9e_tests.rs` — focused acquisition, lifecycle, damage, death, and regression tests.
- `apps/server/src/network/gameplay.rs` — one live creature spawn, authored ability lookup, and per-tick driver wiring.

No protocol or ability/effect contract changed. No direct AI damage path was added. No creature-specific presentation architecture was added; current generic NPC geometry remains the presentation proof.

## Verification

Commands run:

```text
cargo test -p purgatory-simulation --lib phase9
cargo test -p purgatory-server --bin purgatory-server network::gameplay::tests --no-fail-fast
cargo test -p purgatory-server --bin purgatory-server network::gameplay::tests::live_creature_acquires_player_and_uses_basic_strike_runtime
cargo clippy -p purgatory-simulation -p purgatory-server --all-targets --all-features -- -D warnings
```

All passed. Full workspace validation was not run because no dependency or protocol change was required.

## Remaining risk / next slice

Manual client runtime verification remains useful: approach the creature in a normal development session and observe replicated Health plus existing Hurt/Dead semantics. Generic NPC skeleton/presentation is intentionally out of scope.

Recommended next slice: only proceed with the next explicitly authorized phase; do not expand 9E into chase, targeting frameworks, loot, respawn redesign, or NPC-specific art.
