# Phase 6G — Runtime Hardening, Integrated Scale Validation

Phase marker: `6G`. Protocol **v10**. Metrics schema **3** (not bumped). Do **not** begin Phase 7 (MOB).

## Implemented scope

Integrated validation of Phase 6A–6F: typed `LoadScenario` / presets, isolated persist, same-login reconnect and `AlreadyConnected` via existing Welcome/`EntityId` (no protocol fields), env-gated synthetic pressure, in-process correctness gates, persistence failure injection in temp dirs, Developer Tools Runtime Validation that forwards the same CLI, queue inventory without arbitrary caps, and a Phase 6 exit review.

Synthetic pressure is test infrastructure. The server applies `PURGATORY_LOAD_VALIDATION` only when load-mode is on (`PURGATORY_ADMISSION_CAP` set). Production `World` tick does not read that env.

MixedRuntime (`purgatory-load --preset mixed`) is the canonical Phase 6 regression workload.

## Important files

- `crates/common/src/load_validation.rs` — namespaced JSON config
- `tools/bot_client/` — presets, reconnect, portal-from-replica, classify, `--print-server-env`, `--persist-root`
- `apps/server/src/network/load_pressure.rs` — load-mode applicator using existing World APIs
- `crates/simulation/src/phase6g_tests.rs` — ceiling, cleanup, temporal order, cases A–F
- `crates/persistence/src/repository.rs` / `identity.rs` — schema/bak/temp-dir failure injection
- `apps/server/src/network/persist.rs` — shutdown drain in a temp dir
- `tools/dev/features/runtime_validation.ps1` — GUI argv/env only
- `tools/analyze_load_run.py` — start/end/max + growth flag (not leak statistics)
- `docs/PHASE_6G_QUEUE_INVENTORY.md` — queue lifecycle before any new caps
- `docs/PHASE_6_EXIT_REVIEW.md`

## Tests added or changed

- `purgatory-bot-client` scenario/classify/log tests
- `purgatory-common` load-validation parse/load-mode gating
- `purgatory-server` `load_pressure` + persist shutdown drain
- `purgatory-simulation` `phase6g_tests`
- `purgatory-persistence` unsupported schema / bak / temp-dir guards

## Quality-gate results

`./scripts/check.ps1` (2026-08-30): `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo run -p purgatory-content-validator -q` — all passed. Recorded in [`docs/TEST_GATES.md`](TEST_GATES.md) Gate 6G.

Prerequisite for that workspace run: four `purgatory-client` camera/presentation tests still used 2.0-era hardcoded X positions after `DEAD_ZONE_HALF_X` became `3.0`. Those tests now express positions relative to the constant (same assertions). 6G did not retune the camera.

## Manual / runtime verification still required

- Developer Tools: RUNTIME VAL with `--preset mixed` against a Ready load-mode server
- Windows process ownership: Stop owned server → `cargo` can replace `target/debug/purgatory-server.exe` without Access Denied
- Optional evidence: configurable Mixed soak (preset default 30 minutes is **evidence**, not a magic quality threshold)
- Scale ladder: start from Phase 5.7 / 6D populations (10 / 25 / 50); record **bot count and synthetic entity count**; climb only while localhost remains meaningful
- Two-client presentation checks left open from 6B–6F remain manual

## Deviations from the original plan

- No metrics schema 4: schema 3 already exports scheduler, AOI, actions, events, spawn queue, cadence, rejects, observer pending. Missing candidates (`effects_active`, persist save counters, occupancy, non-player entity count) were inventoried and not added.
- Welcome still has no `CharacterId`. Same-login identity is occupancy / `AlreadyConnected` / in-process tests. `EntityId` comes from `ReplicationFrame.local_player_entity`.
- Portal churn uses existing `ReplicatedKind::Portal` replica data; no test-only protocol. In-process portal ordering remains in earlier phase tests.
- Soak duration is `--duration` overlayable; 30 minutes is the soak **preset default**, not the only soak definition.
- Scale ladder 8/25/50/100 is a starting point; 5.7/6D evidence preferred 10/25/50. Composition must be recorded.
- Client camera/presentation tests that still assumed `DEAD_ZONE_HALF_X = 2.0` were updated to use the current constant so the official workspace gate could run. Not a 6G camera change.

## Unresolved issues / risks

- `purgatory-load --probe` login `dev.probe` can still touch default persist if `PURGATORY_DATA_DIR` is unset (known debt).
- Developer Tools Runtime Validation must use a 6G `purgatory-load.exe` (`--preset`). A Ready probe only proves `--probe` exists; a 6F binary still exits 2 on Mixed. The GUI now rebuilds the load package when `--print-server-env` rejects the argv.
- CadenceTable registrations are World-scoped and not reaped on entity despawn (register-once load pressure is finite).
- Long Mixed soak and high-population ladder are evidence runs, not part of `cargo test --workspace`.
- Performance opportunities (snapshot fan-out, etc.) are documented for later; 6G does not endless-optimize.

## Phase boundary

Work stopped at Phase 6G. Phase 7 (MOB / combat AI) was not started. A MOB must be able to use the generic runtime spine without a fake Character/account/session; that is an exit-review finding, not an implementation task.
