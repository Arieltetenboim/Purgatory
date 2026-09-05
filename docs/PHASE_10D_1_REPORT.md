# PHASE 10D-1 REPORT — Player Death Enforcement & Stable Dead Presentation

Status: **INTERNAL SLICE IMPLEMENTED**. Root `PHASE` remains `10B`.
Phase 10D is **not** marked GREEN. Protocol **v15 unchanged**.

## Root cause

The authoritative FOOTNOTE player tick applied movement, jump, and
drop-through input without consulting the player's Health, so a dead player
could continue controlled locomotion or momentum. `PresentationActivity::Dead`
was already selected from Health, but playback mapped it to the one-shot Hurt
clip; `AnimationPlayer` correctly clamped that clip at its final frame.

The first manual normal-session check exposed a second path: the client
prediction `World` had no local Health after replica synchronization. It
therefore ran the shared controller as a non-combat entity, even though the
authoritative server was correctly blocking movement.

## Implemented scope

- `FOOTNOTE::tick_player_with_config` now gates dead player control.
- Dead players have horizontal control velocity cleared and receive neutral
  movement/jump/drop input.
- Gravity, integration, grounding, world bounds, and collision response remain
  active.
- Local prediction now copies the entitled local replicated Health into its
  simulation `World` before prediction and pending-input replay.
- `PresentationActivity::Dead` now selects the established looping Idle clip,
  avoiding the transient Hurt playback path.
- Existing Health authority, ability/action rejection, replication, protocol,
  entity model, and respawn behavior are unchanged.

## Changed

- `crates/simulation/src/footnote/controller.rs`
- `crates/simulation/src/footnote/tests.rs`
- `apps/client/src/prediction.rs`
- `apps/client/src/character_presentation/collection.rs`
- `apps/client/src/character_presentation/tests.rs`
- `docs/PHASE_10D_1_REPORT.md`

## Verification

Commands run:

```text
cargo test -p purgatory-simulation dead_player
cargo test -p purgatory-simulation dead_attacker_cannot_start
cargo test -p purgatory-client dead
cargo test -p purgatory-client dead_replica_health_blocks_local_predicted_locomotion
cargo test -p purgatory-simulation --lib
cargo test -p purgatory-client --bin purgatory-client
./scripts/check.ps1
```

All targeted and package commands passed. The repository quality gate also passed:
`cargo fmt --all -- --check`, workspace check, clippy with denied warnings,
workspace tests, and the content validator.

## Manual proof

The prior normal-session proof failed because prediction lacked Health. The
post-fix normal-session proof has not yet been rerun. It must verify:
creature kills player → held left/right/jump produces no locomotion response.

## Remaining risk

Remote death visibility and the player respawn lifecycle remain out of scope.
No 10D-2 or 10D-3 work was started.
