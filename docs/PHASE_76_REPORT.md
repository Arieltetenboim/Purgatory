# Phase 7.6 — Single-Server Capacity Model

**Status:** **complete / closed 2026-09-02.** Stop before Phase 7.7. Do not begin 7.7 until instructed.

Root `PHASE` = `7.6`. Phase 6 remains GREEN. Localhost figures and model projections are **not** player-capacity claims and **not** a production player cap.

Prior: [`PHASE_75_REPORT.md`](PHASE_75_REPORT.md), [`PHASE_74_REPORT.md`](PHASE_74_REPORT.md), [`PHASE_73_REPORT.md`](PHASE_73_REPORT.md). Plan: [`PHASE_7_PLAN.md`](PHASE_7_PLAN.md).

Artifacts:

- Model: `logs/load/capacity_76/model_20260902_001233/phase76_model.json`
- Falsify: `logs/load/capacity_76/falsify_20260902_001314/falsify_compare.json`
- Helpers: [`scripts/capacity_76_model.ps1`](../scripts/capacity_76_model.ps1), [`scripts/capacity_76_falsify.ps1`](../scripts/capacity_76_falsify.ps1)

## What 7.6 is

Evidence-backed **single-server capacity model** from Phases 7.3–7.5 measurements: owner OLS fits, holdout validation, scenario projections, sensitivity, network/memory side-models, four falsification cells. **No** server optimization, multithreading, or sharding.

## Machine / measurement contract

| Field | Value |
|---|---|
| Calibration | 7.5 ladder `summary_20260901_230222` (21 cells: player/npc/activity/dense/overlap) |
| Holdout | 7.5 canonical `summary_20260901_232330` (4 cells) |
| Policy | `selective` / `high`, budget 4096, cadence ×1 |
| Tick budget | 33.333 ms |
| Prefer | Mean owner ms for regression (not p99) |

## 7.6A — Workload variables

| Variable | Proxy | Used in model |
|---|---|---|
| Observers / players | `peak_active` | residual, AOI, movement, memory |
| Active NPCs | `npcs_active` | memory; npc alt form |
| NPC updates/tick | `npc_updates_total / (30×duration)` | **npc primary** |
| Policy / scan work | `repl_scanned / ticks` | **policy + repl_children** |
| Density | dense vs mixed; `scan_per_obs` | projection empirics |
| Outbound | `bytes_out_per_client_s` | network side-model |
| RSS | `server_rss_mb` | memory side-model |
| Residual | `unattributed_mean` | separate residual fit |

Activity rate (Strike/Pulse) was **not** forced into the tick equation: activity ladder tick remains flat/weak.

## 7.6B — Fitted owner models

### Replication policy (sub-owner)

Candidates (calibration MAPE / R²):

| Form | MAPE | R² |
|---|---:|---:|
| `α + β·observers` | 85.4% | 0.565 |
| **`α + β·scanned_per_tick`** | **42.8%** | **0.894** |
| `α + β·observers + γ·scanned` | 48.2% | 0.945 |

**Chosen (simplest adequate):** scanned form

```text
policy_ms ≈ 0.0441 + 0.000326 · scanned_per_tick
```

Not O(N²). Cost tracks pending/scan volume (which itself grows with observers × overlap).

### Replication children (tick composition)

Full discover+policy+encode+enqueue (used in whole-tick sum; policy remains the reported growth owner):

```text
repl_children_ms ≈ 0.0935 + 0.000561 · scanned_per_tick
```

Calibration MAPE 32.3%, R² 0.926.

### NPC activity

Prefer `updates_per_tick` (within 5pp of best MAPE):

```text
npc_ms ≈ -0.0401 + 0.00900 · npc_updates_per_tick
```

Calibration MAPE 24.2%, R² 0.899. Alt `npcs_active` MAPE was slightly better (~22%) but updates form matches the plan preference and work units.

### Spatial AOI / movement

```text
aoi_ms ≈ α + β · observers     (kept; R² 0.732)
move_ms ≈ α + β · observers    (R² 0.576; light term)
```

Scheduler/actions/effects remain near timer noise — no fake precision.

### Residual (separate)

```text
unattr_ms ≈ 0.0244 + 0.00457 · observers
```

Calibration MAPE 6.9%, R² 0.992. **Not** folded into owner coefficients.

## 7.6C — Whole-tick model

```text
tick_mean_ms ≈
    max(0, repl_children_ms)
  + max(0, npc_ms)
  + max(0, aoi_ms)
  + max(0, move_ms)
  + residual(observers)
```

Calibration: tick MAPE **19.5%**, R² **0.906**.

**p99:** empirical p99/mean ratio ≈ **1.72** (median across calib+holdout). Mean regression is **not** a p99 forecaster; use the ratio only as a rough scale.

## 7.6D — Holdout validation (canonical)

| Cell | meas tick | pred tick | err % | meas policy | pred policy | err % |
|---|---:|---:|---:|---:|---:|---:|
| mixed@64 | 2.294 | 1.514 | −34 | 0.624 | 0.384 | −38 |
| mixed@128 | 3.603 | 2.983 | −17 | 0.954 | 0.905 | −5 |
| dense@32 | 2.140 | 1.696 | −21 | 0.533 | 0.409 | −23 |
| dense@64 | 3.292 | 2.761 | −16 | 1.009 | 0.861 | −15 |

Holdout MAPE: tick **22.0%**, policy **20.4%**, npc **28.6%**.

Bias: canonical cells were **warmer** than ladder calibration at the same N (run variance). Model underpredicts those warmer holdouts. Opposite bias appears on cooler falsify cells (overpredict). Treat ±20–35% tick mean error as the honest band inside the envelope.

## 7.6E–G — Projections, headroom, sensitivity

### Sensitivity @ mixed@64 shape (±10%)

| Perturbation | Δ tick | Δ % |
|---|---:|---:|
| +10% players | +0.100 ms | **+6.7%** |
| +10% overlap (scan/obs) | +0.049 ms | +3.3% |
| +10% activity (NPC upd rate) | +0.023 ms | +1.5% |
| +10% NPCs | +0.019 ms | +1.3% |

**Fastest headroom consumer:** player growth (via scan volume / `replication_policy`), then overlap density, then NPC/activity.

### Scenario projections (MODEL ESTIMATES)

Empirics: mixed `scan/obs≈13.7`, dense `≈33.5`, `npc_upd/npc≈1.04`.

| Scenario | Band | P | NPC | Pred util | Dominant | Confidence |
|---|---|---:|---:|---:|---|---|
| player_heavy | measured_shape | 64 | 24 | ~4.4% | replication_policy | high |
| player_heavy | measured_shape | 128 | 24 | ~8–9% | replication_policy | high |
| player_heavy | limited_extrap | 192 | 24 | ~13% | replication_policy | medium |
| player_heavy | limited_extrap | 256 | 24 | ~17% | replication_policy | low |
| npc_heavy | measured_shape | 8 | 96 | ~4% | npc_activity | high |
| dense | measured_shape | 64 | 64 | ~8–10% | replication_policy | high |
| dense | limited_extrap | 96–128 | 64 | rising | replication_policy | low |

Far extrapolation (MODEL ONLY, **unsupported** as capacity): mixed shape util≈50% near ~**992** players; util≈80% near ~**1584** players — **very low confidence**, far beyond measured max 128. Not a claim.

### Headroom bands

| Band | Scope |
|---|---|
| **Measured** | mixed ≤128, dense ≤64, NPC≤96@8p (7.3–7.5) |
| **Interpolated** | interior of that hull (e.g. mixed@48) |
| **Limited extrapolation** | to ~256 players / ~util 20% as model estimate |
| **Unsupported** | util 50–80% player counts above; WAN; production SLAs |

## 7.6H — Network / memory side-models

### Network (supporting)

| Coefficient | Value | Notes |
|---|---|---|
| bytes / client / s | ~3.5–8 KB/s (shape-dependent) | 7.4/7.5 measured |
| bytes / emitted update | ~avg from calib `repl_bytes/emitted` | stable enough for rough BW |
| Cadence ×2 / budget ≤1024 | ~10%+ byte cut (7.4) | policy lever; not in CPU model |

**Do not** project QUIC/OS saturation from localhost (7.4: queue depth max 1).

### Memory

```text
rss_mb ≈ 9.54 + 0.096 · players + 0.015 · npcs
```

Calibration MAPE **1.6%**, R² **0.991**. Stable in the 10–22 MB envelope.

## 7.6I — Operating envelope (not a production cap)

| Scenario | Kind | Players | NPCs | Density | Tick util | Dominant | Net | Confidence |
|---|---|---:|---:|---|---:|---|---|---|
| mixed@64 ladder | measured | 64 | ~18 | mixed | ~3.5% | replication_policy | ~0.23 MB/s | measured |
| mixed@128 ladder | measured | 128 | ~18 | mixed | ~7.7% | replication_policy | ~0.5 MB/s | measured |
| dense@64 | measured | 64 | ~63 | dense | ~10% | replication_policy | ~0.4 MB/s | measured |
| mixed@48 falsify | measured | 48 | ~18 | mixed | ~2.7% | (unattr/policy) | — | measured |
| dense@48 falsify | measured | 48 | ~63 | dense | ~5.7% | replication_policy | — | measured |
| mixed@192 | projected | 192 | 24 | mixed | ~13% | replication_policy | model | medium |
| mixed@256 | projected | 256 | 24 | mixed | ~17% | replication_policy | model | low |

No production player cap is declared.

## 7.6J — Falsification (predict → run)

| Cell | Pred tick | Meas tick | err % | Pred policy | Meas policy | Pred dominant | Meas dominant |
|---|---:|---:|---:|---:|---:|---|---|
| mixed@48 | 1.21 | 0.90 | +35 | 0.26 | 0.20 | replication_policy | unattributed |
| mixed@16 + npc96 | 1.36 | 1.06 | +28 | 0.12 | 0.23 | **npc_activity** | **npc_activity** |
| mixed@32 a4/p6 | 0.99 | 0.73 | +35 | 0.19 | 0.15 | npc_activity | replication_policy |
| dense@48 | 2.12 | 1.90 | **+12** | 0.57 | 0.58 | **replication_policy** | **replication_policy** |

Findings:

1. Dense mid-point matches well (policy −1%, tick +12%).
2. Cool mixed cells: model **overpredicts** tick (~+30%) — opposite of warm canonical underprediction; short-cell variance dominates absolute ms.
3. High-NPC cell: **dominant owner correct** (`npc_activity`); absolute npc ms overpredicted when using configured count × mean upd rate (active_pct / idle NPCs). Prefer measured `npc_updates_per_tick` when available.
4. Activity boost does not create a tick cliff (matches 7.3/7.5).
5. Queue depth remained healthy; util ≤ ~6% on falsify set.

No coefficient rewrite required for envelope conclusions; document NPC projection caveat when only configured counts are known.

## 7.6K — Architecture checkpoint

**Conclusion: (1) Current headroom is sufficient; no scaling redesign justified in the measured envelope.**

With nuance for 7.7:

- **(2)** Under player/density growth, capacity is **likely to become `replication_policy`-limited first** (sensitivity + projections).
- NPC-heavy shapes can flip dominance to `npc_activity` without approaching tick saturation in the tested range.
- Far util≈50% extrapolations are **unsupported** as architectural evidence.

**Do not** introduce multithreading or sharding in 7.6.

## Recommended Phase 7.7 direction

Phase 7.7 should treat redesign as **optional and evidence-gated**:

1. Default path: **no multi-process/world-split redesign** while operating inside the measured/interpolated envelope (util ≪ 50%).
2. If content/design targets require sustained player+density far above mixed@128 / dense@64, prioritize studying **`replication_policy` / scan volume** reduction (still Phase 6 architecture — policy/content locality, not a new replication stack) before process sharding.
3. Revisit parallelism only if a future measured envelope shows a single-thread owner remaining dominant after local opts with util approaching the tick budget.
4. Keep harness/portal caveats and localhost transport non-limits explicit when setting any future production gate (7.8).

## Quality gate

`./scripts/check.ps1` — **PASS** 2026-09-02 (`fmt`, `check`, `clippy -D warnings`, `test --workspace`, content validator). Docs/scripts modeling only (no gameplay semantic change).

## Explicit non-goals honored

No server CPU opt campaign; no transport retune; no multithreading/sharding; no invented production player cap; stop before 7.7.

## Boundary

Work **stops before Phase 7.7**.
