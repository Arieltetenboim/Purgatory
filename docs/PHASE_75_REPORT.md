# Phase 7.5 — CPU / Owner Scaling & Measured Optimization

**Status:** **complete / closed 2026-09-01.** Stop before Phase 7.6. Do not begin 7.6 until instructed.

Root `PHASE` = `7.5`. Phase 6 remains GREEN (architecture closed). Localhost figures are **not** player-capacity claims.

Plan: [`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md). Prior: [`docs/PHASE_74_REPORT.md`](PHASE_74_REPORT.md), [`docs/PHASE_73_REPORT.md`](PHASE_73_REPORT.md). Artifacts: `logs/load/capacity_75/`. Summary: latest `logs/load/capacity_75/summary_*/phase75_summary.json`.

## What 7.5 is

Measure tick-owner scaling under the representative 7.3/7.4 workloads; close Instant accounting gaps so `unattributed` is understood; optimize **only** owners with measured evidence. **Do not** optimize transport queues (7.4 closed with no cliff).

Governing rule: **measure → isolate → optimize proven owners → re-measure.**

## Machine / build

| Field | Value |
|---|---|
| Date | 2026-09-01 |
| OS | Windows 11 Home 10.0.26200 |
| CPU | 13th Gen Intel Core i7-13650HX (14C / 20 logical) |
| Build | Release `purgatory-server` + `purgatory-load` |
| Helper | `scripts/capacity_75_ladder.ps1` (+ `capacity_ladder.ps1`) |
| Policy baseline | `selective` / `high`, soft budget **4096**, stranger cadence **×1** |
| Lifecycle | `health_max=6`, `respawn_delay=20` |
| Detail | `PURGATORY_CAPACITY_DETAIL=1` |
| Harness hygiene | `--post-ramp-thin` for N≥32 |

## Implemented scope

### 7.5A — Ownership accounting audit (accepted)

Proven Instant gaps that inflated detail-mode remainder:

1. Post-`try_push` Known/Leave/pending commit was only in parent `replicate_us`, which detail mode **excludes** from attributed leaves → remainder.
2. Per-observer prep + post-frame counter/pressure updates sat outside child Instant spans.
3. `begin_tick` / gauge refresh was untimed.

**Changes (measurement correctness, not a CPU “win”):**

- Extend `enqueue_us` through post-push commit in [`replication.rs`](../apps/server/src/network/replication.rs).
- Attribute observer prep + post-frame glue + session-queue scan to `replication_enqueue`.
- Fold `begin_tick` into `entity_lifecycle` (detail) / `gameplay_services` (coarse).
- Include observer-id collection in `replication_discover`.
- Drop unnecessary `.collect::<Vec<_>>()` on fan-out observer iteration (discover path; no CPU claim).

Unattributed share on mixed@64/@128 fell from **~30–39% (7.3)** to **~23–25% (7.5)**. Dominant owner shifted from `unattributed` to **`replication_policy`** at high player counts — remainder was partially mis-bucketed handoff work.

**Residual ~15–25% unattributed:** Instant hierarchy gaps + microsecond truncation on many short spans. Bounded; does **not** grow as a named cliff. **Do not optimize the unattributed bucket.**

### 7.5B–E — CPU ladders, normalized costs, decomposition, shapes

Suites: audit, player, NPC, activity, dense, overlap (hotspot radius), canonical. No replication-policy change on baseline cells.

### 7.5G–I — Optimizations

**No CPU owner optimization accepted.** Absolute tick headroom remains large (peak util ~10.6% at dense@64; mixed@128 util ~7.7–7.8%). Manufacturing a micro-opt on a sub-millisecond owner would invent work. Correctness gate stayed green with accounting-only changes.

## Owner scaling table (player ladder — mixed, NPC fixed)

| N | tick p99 (ms) | util % | dominant | unattr share % | repl children mean | policy mean | npc mean | AOI mean |
|---|---:|---:|---|---:|---:|---:|---:|---:|
| 8 | 0.50 | 0.80 | npc_activity | 19.1 | 0.057 | 0.023 | 0.072 | 0.021 |
| 16 | 0.74 | 1.21 | simulation_movement | 19.6 | 0.116 | 0.059 | 0.074 | 0.043 |
| 32 | 1.64 | 2.44 | replication_policy | 20.4 | 0.335 | 0.177 | 0.105 | 0.081 |
| 64 | 1.86 | 3.47 | replication_policy | 25.4 | 0.532 | 0.306 | 0.101 | 0.089 |
| 128 | 5.33 | 7.73 | replication_policy | 23.3 | 1.219 | 0.633 | 0.158 | 0.341 |

Issuance 100% where recorded. Overruns **0**. Writer queue depth max **1**, push fails **0** (unchanged from 7.4).

## Unattributed explanation

| Claim | Evidence |
|---|---|
| Partly Instant hierarchy / leaf exclusion | Closing enqueue commit + outer glue cut share ~30–39% → ~23–25% and demoted `unattributed` as dominant |
| Residual is timer noise | Share stable ~15–25% across player N; drops to ~5–10% when `npc_activity` dominates absolute time |
| Not a capacity cliff | Tick util ≪ 100%; overruns 0; no exclusive `simulation_tick` class |

## Replication CPU decomposition

| Sub-owner | Role | mixed@128 mean (ms) | dense@64 mean (ms) |
|---|---|---:|---:|
| `replication_discover` | dirty → pending fan-out | 0.18 | (in children sum) |
| **`replication_policy`** | eligibility / cadence / rank | **0.63** | **1.07** |
| `replication_encode` | record/frame encode | 0.16 | — |
| `replication_enqueue` | push + commit + outer glue | 0.24 | — |

**Primary growth variable:** observer count × pending update work (policy loop). Emitted-normalized cost stays roughly flat (~2.9–3.1 ms per 1000 emitted on the player ladder) — cost tracks **observers / pending scans**, not bytes alone.

Do **not** redesign AOI / `InterestFanoutIndex` / selective policy / packer from this envelope.

## Normalized costs (configuration-specific)

| Metric | Approx. | Notes |
|---|---|---|
| repl µs / observer | ~7–10 (mixed); ~25–30 (dense) | Dense overlap raises per-observer cost |
| repl µs / 1000 emitted | ~2900–3100 | Stable across player 8→128 |
| npc µs / 1000 updates | ~5.9k → ~8.9k (NPC 8→96) | Mild worsening with N (investigate in 7.6 model; absolute still small) |
| CPU % / player (mid-run) | noisy on short cells | Prefer mid NDJSON; end samples unreliable |

These are **not** production SLAs.

## Scaling shapes

| Curve | Shape |
|---|---|
| players → tick p99 | roughly linear → mild/strong superlinear 64→128 (~2.9× for 2×) |
| players → replication_policy | roughly linear–mild superlinear |
| players → spatial_aoi | mild superlinear at 128 |
| NPCs → npc_activity | roughly linear–mild superlinear |
| activity rate → tick | flat / weak (matches 7.3) |
| hotspot radius → repl | mild increase (r4→r16) |
| memory | mild linear with players (~10–22 MB) |

No cliff inside the tested envelope. Superlinear 64→128 is **observer×pending cross-product pressure**, not a transport or tick overrun cliff.

## Optimizations attempted

| Change | Kind | Result |
|---|---|---|
| Instant span close (enqueue commit, outer glue, begin_tick) | accounting | Accepted — clearer ownership |
| Remove discover `collect::<Vec<_>>()` | tiny alloc hygiene | Accepted as hygiene; **no CPU win claimed** |
| Policy/encode algorithmic redesign | CPU opt | **Not attempted** — owners remain cheap vs 33 ms budget |

## Canonical benchmark comparison (vs 7.3 same policy)

Canonical suite (`summary_20260901_232330`) after accounting close + discover hygiene:

| Cell | 7.3 tick p99 | 7.5 canonical p99 | util % | 7.5 dominant | unattr % | repl children | bytes_out/s | queue max |
|---|---:|---:|---:|---|---:|---:|---:|---:|
| mixed@64 | 1.41 | 4.70 | 6.9 | replication_policy | 15.6 | 1.10 | ~260 KB/s | 1 |
| mixed@128 | 2.90 | 6.49 | 10.8 | replication_policy | 18.0 | 1.80 | ~587 KB/s | 1 |
| dense@32 | 1.66 | 5.19 | 6.4 | npc_activity | 8.6 | 0.91 | ~236 KB/s | 1 |
| dense@64 | 2.27 | 6.38 | 9.9 | replication_policy | 10.3 | 1.63 | ~432 KB/s | 1 |

Player-ladder cells earlier in the same session were cooler (mixed@64 p99 ~1.9, mixed@128 ~5.3). Short-cell tick p99 is **noisy**; do not treat canonical vs 7.3 deltas as an accounting regression. Push fails 0; RSS ~14–22 MB. Capacity conclusion unchanged: **comfortable headroom** (util ≤ ~11%).

## Parallelism verdict (7.5K)

**1. No — tested envelope remains comfortably below CPU/tick limits.**

- Peak measured util ~10.8% (canonical mixed@128); dense@64 ~9.9–10.6%.
- Tick p99 ≪ 33.3 ms spacing; overruns 0.
- Dominant growth owner `replication_policy` is still ~0.6–1.1 ms mean — not a structural parallelization candidate yet.
- Multithreading / sharding remains deferred to 7.7 **only if** a future envelope after 7.6 modeling shows a single-thread ceiling.

## Remaining CPU headroom

| Cell | tick util % | approx. headroom to 100% util |
|---|---:|---|
| mixed@64 (ladder / canonical) | ~3.5 / ~6.9 | ~14–28× |
| mixed@128 (ladder / canonical) | ~7.8 / ~10.8 | ~9–13× |
| dense@64 | ~9.9–10.6 | ~9–10× |

Headroom is **not** a capacity SLA (harness, portals, WAN, and longer soaks remain separate).

## Quality gate

`./scripts/check.ps1` — **PASS** 2026-09-01 (`fmt`, `check`, `clippy -D warnings`, `test --workspace`).

## Recommended Phase 7.6 input

**Begin Phase 7.6 — Single-Server Capacity Model** with:

1. Fit measured coefficients: players × NPCs × density × activity → tick p99 / CPU / RSS / outbound bytes from 7.3–7.5 ladders.
2. Treat `replication_policy` (observer × pending) and `npc_activity` as primary CPU drivers; residual unattributed as bounded noise (~20%).
3. Transport remains **non-limiting** on localhost through mixed@128 / dense@64 (7.4).
4. Do **not** introduce multithreading in 7.6.
5. Produce operating envelopes and headroom statements — do not invent targets before the model.

## Explicit non-goals honored

No transport-queue redesign; no Phase 6 AOI/fan-out/packer redesign; no multithreading; no invented SLA; harness portal FAILED / snapshot_starvation not treated as server CPU.

## Boundary

Work **stops before Phase 7.6**.
