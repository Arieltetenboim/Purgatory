# PHASE 9B REPORT — Basic Attack executable path

Status: **GREEN**. Root `PHASE` = `9B`. Protocol **v14 unchanged**. **Phase 9C was not started.**

## Implemented scope

First real server/simulation ability:

`activate skill.basic.strike → ActionTable Windup/Active/Recovery → Active forward query → ordered AbilityEffect::Damage → apply_damage → Health reduction`

- Activation does **not** require a selected target. `Activation ≠ affected entities`.
- Authored `AbilityDefinition` (`content/shared/abilities/skill.basic.strike.json`), not 7.2 `Strike` / `STRIKE_*`.
- Server-authoritative forward AABB query during Active. Zero hits is a valid Active.
- Damage only through `World::execute_ability_effect` → `apply_damage`.
- Health remains optional. Proof combatants attach Health in tests; default player spawn still has none (7.2 isolation).

No protocol, client input, presentation cues, AI, PvP, or 9C work.

## Contract correction (from 9A)

Replaced `AbilityTargeting::None | Single { range }` with:

- `AbilityActivation::{Independent, SelectedEntity}` — who is required **to start**
- `AbilityDelivery::{ForwardQuery { range, half_height, max_targets }, SelectedEntity}` — who is affected **at Active**

Independent + ForwardQuery is Basic Attack. SelectedEntity remains a stub for later targeted skills.

## Files changed

Simulation: `ability.rs`, `runtime.rs`, `body.rs` (`PlayerState.facing_sign`), `footnote/controller.rs`, `health.rs` (`PLAYER_HEALTH_MAX`), `lib.rs`, `phase9a_tests.rs`, `phase9b_tests.rs` (new).

Content: `content/shared/abilities/skill.basic.strike.json`, `crates/content` loader/registry/ability schema, pack test.

Docs: this report, `PHASE`, `ROADMAP.md`, `ARCHITECTURE.md`, `DECISIONS.md` (ADR-0061), `CONTENT_PIPELINE.md`, `PROTOCOL.md`, `TEST_GATES.md`, `README.md`.

## Tests

- `phase9b_tests.rs` — empty-space activate, inside/outside query, no-Health safety, dead attacker, once-per-Active, cooldown, generic id, player+NPC same path.
- `phase9a_tests.rs` — updated to Independent + ForwardQuery; Strike isolation kept.
- `ability.rs` — `forward_query_aabb` front-only.
- `purgatory-content` — pack loads `skill.basic.strike` and runs the activate→Active→damage path.

## Quality gate (focused)

Commands run:

```text
cargo fmt -p purgatory-simulation -p purgatory-content
cargo test -p purgatory-simulation --lib phase9a
cargo test -p purgatory-simulation --lib phase9b
cargo test -p purgatory-simulation --lib ability::
cargo test -p purgatory-simulation --lib strike_workload
cargo test -p purgatory-content --lib pack_loads_basic_strike
cargo test -p purgatory-content --lib workspace_pack_loads
cargo clippy -p purgatory-simulation -p purgatory-content --all-targets -- -D warnings
```

All passed. Full workspace `./scripts/check.ps1` was not run (not required for this slice).

## Deviations

Default `spawn_player` still has no Health so 7.2 NPC `Strike` does not start hitting live players. Proof attaches Health in tests.

## Unresolved / 9C

Not started. Recommended 9C: player command (protocol bump only if required), live-player Health if product wants it (isolate from 7.2 Strike), Attack/Hurt oneshots. Do not add cones/projectiles/LoS/PvP.
