# Phase 6G — Runtime Hardening, Integrated Scale Validation

Phase marker: `6G`. Protocol **v10**. Metrics schema **3** (not bumped). Do **not** begin Phase 7 (MOB).

## Implemented scope

Integrated validation of Phase 6A–6F: typed `LoadScenario` / presets, isolated persist, same-login reconnect and `AlreadyConnected` via existing Welcome/`EntityId` (no protocol fields), env-gated synthetic pressure, in-process correctness gates, persistence failure injection in temp dirs, Developer Tools Runtime Validation that forwards the same CLI, queue inventory without arbitrary caps, and a Phase 6 exit review.

Synthetic pressure is test infrastructure. The server applies `PURGATORY_LOAD_VALIDATION` only when load-mode is on (`PURGATORY_ADMISSION_CAP` set). Production `World` tick does not read that env.

MixedRuntime (`purgatory-load --preset mixed`) is the canonical Phase 6 regression workload. An 8-bot mixed run splits into **1 portal**, **2 churn**, and **5 persistent movers** (persistent baseline **6**). `--duration` is the real-client soak length, not a wall-clock wait after bots have already disconnected. Portal success is an accepted authoritative map/address transition after walking into the activation zone — not a timer `PortalActivate`. Churn reconnects only churn-role bots for the full duration.

## Mixed soak guarantees

A passing mixed/soak Runtime Validation means:

- Persistent real QUIC clients stayed at the role-plan baseline for essentially the whole duration (time below baseline is gated; a one-sample dip is not an instant fail).
- Real-client connected-seconds cover at least half of `persistent_target × duration`.
- At least one portal bot completed an authoritative transition (rejects/`out_of_range` may still appear as negative evidence).
- Server AOI enter/update counters moved after ramp when metrics were observed.
- Churn-role bots disconnected/reconnected during the run (when the plan includes churn and duration ≥ 12s).
- Existing 6F/6G synthetic runtime-service pressure and classify overflow/starvation/encode gates still apply.

Harness artifact extras live in `run_summary.json` (`soak`, `requested_duration_secs`) without a metrics schema bump. `logs/load/current_run.txt` is the in-progress pointer; `last_finished.txt` is written only when the summary is complete; `last_runtime_validation.txt` records the last `--preset` run. Run directory timestamps are leap-year civil dates.

Official Developer Tools Runtime Validation always restarts a clean load-mode server with a new isolated persist root and refuses a second concurrent harness.

## Important files

- `crates/common/src/load_validation.rs` — namespaced JSON config
- `tools/bot_client/` — presets, roles (persistent/churn/portal), replica-guided portal nav, reconnect, classify, `--print-server-env`, `--persist-root`
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

`./scripts/check.ps1` (2026-08-31): `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo run -p purgatory-content-validator -q` — all passed after the mixed soak correctness fix. Recorded in [`docs/TEST_GATES.md`](TEST_GATES.md) Gate 6G.

Prerequisite for that workspace run: four `purgatory-client` camera/presentation tests still used 2.0-era hardcoded X positions after `DEAD_ZONE_HALF_X` became `3.0`. Those tests now express positions relative to the constant (same assertions). 6G did not retune the camera.

## Manual / runtime verification still required

- Developer Tools: RUNTIME VAL **smoke (20s)** first, then mixed 20s, then the official 30-minute mixed soak. Confirm persistent real clients stay up, churn continues, and portal transitions ≥ 1.
- Windows process ownership: Stop owned server → `cargo` can replace `target/debug/purgatory-server.exe` without Access Denied
- Optional evidence: configurable Mixed soak (preset default 30 minutes is **evidence**, not a magic quality threshold)
- Scale ladder: start from Phase 5.7 / 6D populations (10 / 25 / 50); record **bot count and synthetic entity count**; climb only while localhost remains meaningful
- Two-client presentation checks left open from 6B–6F remain manual

## Deviations from the original plan

- No metrics schema 4: schema 3 already exports scheduler, AOI, actions, events, spawn queue, cadence, rejects, observer pending. Missing candidates (`effects_active`, persist save counters, occupancy, non-player entity count) were inventoried and not added.
- Welcome still has no `CharacterId`. Same-login identity is occupancy / `AlreadyConnected` / in-process tests. `EntityId` comes from `ReplicationFrame.local_player_entity`.
- Portal churn uses existing `ReplicatedKind::Portal` replica data; no test-only protocol. In-process portal ordering remains in earlier phase tests.
- Soak duration is `--duration` overlayable; 30 minutes is the soak **preset default**, not the only soak definition. Mixed/soak duration is real-client continuity, not “harness process stayed alive after bots dropped.”
- Mixed 8-bot composition is 1 portal + 2 churn + 5 movers (baseline 6). Portal activate is in-zone only; `out_of_range` does not satisfy the portal gate.
- An explicit `--duration` overlay without `--timeout` now raises the wall-clock timeout in Rust so mixed/soak GUI duration combos cannot fail with `timeout must be >= duration`. Developer Tools does not invent `--timeout`.
- Official Runtime Validation always restarts an isolated load-mode server. Concurrent Runtime Validation is blocked. `last_finished` ignores in-progress `current_run` and future-dated folder names (the old `days/365` timestamp bug formatted 2026-08-31 as `20260916`).
- The load-harness loop no longer uses `biased` 30 Hz ticks that starved ramp and 1 Hz metrics (a 20s soak connected only ~4 of 8 bots and recorded `peak_connected=0`). Remaining bots spawn from the tick path. Portal bots dwell Neutral only when the replica is centered and not just rejected; after `OutOfRange` they keep the approach axis instead of freezing just outside the server AABB. Server `out_of_range` is unchanged.
- Serialized `await` handshakes on the tick path caused ~3s hitch catch-up (60–90 snapshot polls) and counted occupancy as one unit per sample. Connects now overlap; hitch catch-up is capped to one I/O pass; `connected_seconds` is `persistent × dt`. Developer Tools Rebuild skips `purgatory-server.exe` while it is running (same as the client skip) so cargo does not hit Access Denied.
- Scale ladder 8/25/50/100 is a starting point; 5.7/6D evidence preferred 10/25/50. Composition must be recorded.
- Client camera/presentation tests that still assumed `DEAD_ZONE_HALF_X = 2.0` were updated to use the current constant so the official workspace gate could run. Not a 6G camera change.

## Unresolved issues / risks

- `purgatory-load --probe` login `dev.probe` can still touch default persist if `PURGATORY_DATA_DIR` is unset (known debt).
- Developer Tools Runtime Validation must use a 6G `purgatory-load.exe` (`--preset`). A Ready probe only proves `--probe` exists; a 6F binary still exits 2 on Mixed. The GUI now rebuilds the load package when `--print-server-env` rejects the argv.
- CadenceTable registrations are World-scoped and not reaped on entity despawn (register-once load pressure is finite).
- Long Mixed soak and high-population ladder are evidence runs, not part of `cargo test --workspace`.
- Performance opportunities (snapshot fan-out, etc.) are documented for later; 6G does not endless-optimize.
- **128-client load stall (pending, empirical):** previously observed load-only stall/pending behavior around the 128-client case. Not reproduced in the local-player presentation correctness pass. Do not treat standing jitter or falling remainder-Y as the same bug. `PREDICTION_PENDING_CAP` stays 128; `pending_window_stall_ticks` is a client counter for the upcoming capacity phase only.

Local-player standing X jitter (~0.02 wu while idle) and falling remainder-Y floor clip were addressed on the client presentation path (last locally executed tick velocity for remainder extra; falling extra keeps tick Y; `rest_lead` / `landing_lead` skip restore). Quiet-play Δx was not observed in a follow-up client run; landing no longer sinks. Local Y is now lerped between consecutive predicted FOOTNOTE tick poses (up to one tick of vertical visual delay) so the jump arc is continuous without ballistic extra or renderer collision.

## Phase boundary

Work stopped at Phase 6G. Phase 7 (MOB / combat AI) was not started. A MOB must be able to use the generic runtime spine without a fake Character/account/session; that is an exit-review finding, not an implementation task.
