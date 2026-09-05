# Phase 7.3 — Capacity Ladder & Bottleneck Isolation

**Status:** **complete / closed 2026-09-01.** Stop before Phase 7.4. Do not begin 7.4 until instructed.

Root `PHASE` remains `7.3`. Phase 6 remains GREEN (architecture closed). Localhost figures are **not** player-capacity claims.

Plan: [`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md). Issuance proof: [`docs/PHASE_73A_ISSUANCE.md`](PHASE_73A_ISSUANCE.md). Artifacts: `logs/load/capacity_73/`. Merged summary: `logs/load/capacity_73/summary_20260901_213932/phase73_summary_merged.json`.

## What 7.3 is

Controlled scaling across players, NPC population, action/Pulse rate, and dense AOI hotspot occupancy under the Phase 7.2 representative workload. Measure attainment, tick ownership, gameplay/replication/network/resources. Classify saturation only with exclusive evidence. No server optimization; no Phase 6 replication-policy tuning; no harness poll redesign unless it blocked ladder evidence (it did not).

## Machine / build

| Field | Value |
|---|---|
| Date | 2026-09-01 |
| OS | Windows 11 Home 10.0.26200 |
| CPU | 13th Gen Intel Core i7-13650HX (14C / 20 logical) |
| RAM | ~15.7 GiB |
| Build | Release `purgatory-server` + `purgatory-load` |
| Helpers | `scripts/capacity_ladder.ps1`, `scripts/capacity_73_ladder.ps1` |
| Artifacts | `logs/load/capacity_73/` |
| Protocol | **v11** |
| Live UDP `PURGSTAT` | **schema 3** |
| Replication policy | `selective` / population `high` |
| Ladder NPC HP override | `health_max=6`, `respawn_delay_ticks=20` (lifecycle churn) |

## Attained test envelope

All cells below attained **100% issuance** (`spawn_issued == requested`, `peak_active == requested` within admission, `funnel_invariant_ok=true`).

| Suite | Cells | Duration |
|---|---|---|
| 7.3B lifecycle proof | mixed@8 | 90s |
| 7.3C player ladder | mixed players 8→16→32→64→128 (NPC=24 fixed) | 45s |
| 7.3D NPC ladder | players=8, NPC 8→24→48→64→96 | 45s |
| 7.3E activity ladder | players=8, NPC=24; action/pulse 40/60 → 20/30 → 8/12 → 4/6 | 45s |
| 7.3F dense hotspot | dense players 8→16→32→64 (NPC=64 dense preset) | 45s |
| Stable soak | mixed@16 | **8m** |

**Envelope end:** mixed@128 and dense@64 remain **below server tick saturation** (util ≤ ~5.4%, overruns **0**). No exclusive `simulation_tick` or `server_transport_backpressure` classification. The tested envelope **ends before server saturation**.

## Harness caveats

1. **Post-ramp `snapshot_starvation`** (known): full O(N)×~1 ms poll after ramp; controller tick p99 grows to ~0.6–1.9 s at 32–128. Recorded separately; **not** server saturation without server evidence.
2. **Harness exit ≠ capacity:** several cells exit FAILED for `portal_transition_missing` / `portal_never_eligible` (mixed-runtime portal role) and/or `snapshot_starvation`. Issuance and server tick evidence remain valid.
3. Issuance path was **not** changed further after 7.3A PASS.

| Cell | harness exit | snapshot_starvation | controller tick p99 (ms) | primary status reason |
|---|---|---|---|---|
| player_8 | 0 | 0 | 257 | COMPLETE |
| player_16 | 1 | 0 | 341 | portal_transition_missing |
| player_32 | 1 | 1 | 633 | portal + starvation |
| player_64 | 1 | 1 | 1219 | starvation (+ portal class) |
| player_128 | 1 | 1 | 1921 | portal_never_eligible + starvation |
| dense_32/64 | 1 | 1 | 611 / 1104 | portal / starvation |

## Lifecycle proof (7.3B)

Wiring: `NpcWorkloadConfig.health_max` + `World::set_npc_respawn_delay_ticks` applied from load pressure; respawn uses entity `health.max`.

Cell `lifecycle_mixed8` (90s, HP=6, delay=20):

| Metric | Value |
|---|---|
| deaths_total | **19** |
| respawns_total | **19** |
| spawn/despawn completed | 19 / 21 |
| health_mutations | 168 |
| actions started | 105 |
| effects active (end) | 1 |
| scheduler_queued end/max | 1 / 5 |
| AOI enter/leave/updates | 1424 / 578 / 51077 |
| cadence leaf p99 | ~0 |
| tick p99 / util | 0.40 ms / 0.68% |
| dominant | `npc_activity` |
| memory end | 10.7 MB |

**PASS:** deaths > 0, respawns > 0, spawn/despawn activity > 0.

## Player scaling (7.3C) — `representative_mixed`, NPC fixed

| N | issued/peak | tick p50/p99 (ms) | util % | dominant | npc_act mean | repl mean | sim_move mean | RSS MB | harness starve |
|---|---|---|---|---|---|---|---|---|---|
| 8 | 8/8 | 0.22 / 0.72 | 0.72 | npc_activity | 0.073 | 0.043 | 0.042 | 10.5 | no |
| 16 | 16/16 | 0.42 / 0.71 | 1.31 | simulation_movement | 0.080 | 0.083 | 0.105 | 11.2 | no |
| 32 | 32/32 | 0.53 / 0.94 | 1.61 | unattributed | 0.094 | 0.118 | 0.098 | 13.1 | yes |
| 64 | 64/64 | 0.79 / 1.41 | 2.48 | unattributed | 0.095 | 0.203 | 0.127 | 15.9 | yes |
| 128 | 128/128 | 1.74 / 2.90 | 5.43 | unattributed | 0.134 | 0.502 | 0.234 | 21.6 | yes |

Shape: tick p99 and replication mean grow **roughly linear to mildly superlinear** with players; NPC cost stays nearly flat (NPC count fixed). No tick overrun cliff.

@128 network (server): ~514 KB/s out (~4.0 KB/player/s), writer queue depth max **1**, write_drain p99 **2** (sample units), push fails **0**. Replication: eligible ~3.6M, emitted ~582k, coalesced ~3.0M, bytes ~26.7 MB, budget deferred **0**.

## NPC scaling (7.3D) — players=8

| NPC cfg | active≈ | updates | npc_activity mean (ms) | tick p99 (ms) | deaths |
|---|---|---|---|---|---|
| 8 | 7 | 8532 | 0.034 | 0.46 | 12 |
| 24 | 18 | 25272 | 0.074 | 0.45 | 6 |
| 48 | 36 | 50456 | 0.155 | 0.72 | 0 |
| 64 | 48 | 67580 | 0.194 | 0.87 | 0 |
| 96 | 72 | 101036 | 0.353 | 1.18 | 0 |

Shape: `npc_activity` vs NPC updates is **roughly linear**; tick p99 follows. Dominant becomes `npc_activity` once population is non-trivial. Deaths drop at higher NPC counts in 45s cells (damage diluted across more targets under HP=6) — configuration observation, not a combat redesign.

## Activity scaling (7.3E) — 8p / 24 NPC

| action/pulse period | actions | health mut | deaths/respawns | tick p99 | npc_act mean |
|---|---|---|---|---|---|
| 40 / 60 | 35 | 52 | 0 / 0 | 0.57 | 0.074 |
| 20 / 30 | 57 | 93 | 6 / 6 | 0.53 | 0.074 |
| 8 / 12 | 123 | 311 | 42 / 42 | 0.52 | 0.094 |
| 4 / 6 | 188 | 589 | 94 / 94 | 0.48 | 0.085 |

Shape: gameplay counters scale with rate (**roughly linear**); tick p99 stays **flat / sublinear** at this occupancy — activity alone does not pressure the 33 ms budget here. Scheduler/effects remain tiny in tick leaves.

## Dense-hotspot scaling (7.3F) — `representative_dense`

| N | issued | tick p99 | util % | dominant | npc_act | repl | deaths | harness starve |
|---|---|---|---|---|---|---|---|---|
| 8 | 8 | 0.91 | 1.48 | npc_activity | 0.284 | 0.070 | 55 | no |
| 16 | 16 | 1.16 | 2.00 | npc_activity | 0.281 | 0.121 | 55 | yes* |
| 32 | 32 | 1.66 | 2.90 | npc_activity | 0.294 | 0.252 | 55 | yes |
| 64 | 64 | 2.27 | 4.42 | npc_activity | 0.390 | 0.411 | 55 | yes |

\*dense_16 recorded starvation sample with exit 0.

Primary combined stress case: **npc_activity** remains dominant; replication grows with observers; still no server tick cliff through 64.

## First reproducible pressure point

**No reproducible server saturation** inside the attained envelope.

Closest **pressure** (not exclusive server class):

| Kind | Where | Evidence |
|---|---|---|
| `harness_client` | players ≥32 (and dense ≥16–32) | controller tick p99 ≫ 33 ms; `snapshot_starvation` samples; harness FAILED |
| Growing replication share | players ↑ | `replication` mean 0.04→0.50 ms @128; still ≪ tick budget; queue depth max 1 |
| Instrumentation gap | high-N mixed | `dominant_owner=unattributed` with ~30–39% unattributed share — not a named cliff |

Saturation class on all cells: `unknown_unattributed` (honest — no exclusive `simulation_tick` / `server_transport_backpressure`).

At the highest measured server point (mixed@128): tick p99 **2.9 ms**, util **5.4%**, overruns **0**, CPU sample noisy/low on short cells, RSS **~22 MB**, outbound ~0.5 MB/s, writer queue healthy.

## Scaling shapes (summary)

| Curve | Shape (qualitative) |
|---|---|
| players → tick p99 | roughly linear / mild superlinear |
| players → server RSS | roughly linear |
| players → replication mean | roughly linear–superlinear |
| NPCs → tick p99 | roughly linear |
| NPCs → npc_activity | roughly linear |
| activity rate → tick p99 | flat / sublinear (at 8p/24npc) |
| activity → deaths/actions | roughly linear |
| outbound bandwidth → queue/drain | no backlog (depth≤1) through 128 |
| workload → memory | mild growth; soak +~1.8 MB / 8 min |

Do **not** infer asymptotic Big-O from these curves.

## Normalized costs (configuration-specific evidence)

Workload: Release, selective/high, HP=6, respawn=20, ~45s cells unless noted.

| Metric | Value | Config |
|---|---|---|
| µs / NPC update (approx.) | ~4.7 | npc_96 @8p: npc_activity mean 0.353 ms / ~75 updates/tick |
| µs / NPC update | ~4.1 | npc_24 @8p: 0.074 ms / ~18 updates/tick |
| bytes / player / sec | ~4019 | mixed@128 network_pressure |
| replication bytes total | 26.7 MB / ~47 s | mixed@128 |
| memory / connected player | ~0.17 MB | mixed@128 end RSS / 128 (shared NPC world) |

These are **not** production SLAs.

## Stable soak (mixed@16, 8 minutes)

Below first harness/server pressure. COMPLETE; overruns 0; funnel ok; no snapshot starvation.

| Metric | Early (~60s) | End (~480s) |
|---|---|---|
| tick p99 (window) | ~0.7–0.9 (steady) | **0.845** |
| tick util % | ~1.2 | 1.26 |
| dominant | npc_activity | npc_activity |
| working set | ~11.6 MB | **12.4 MB** (+~0.8 from 60s; +1.8 from start) |
| CPU util sample | ~7.8% @60s | end-sample noisy (42% last interval) — prefer mid-run |
| scheduler_queued | start 1 | end 2 / max 5 |
| effects_active | — | 1 |
| deaths/respawns | — | 102 / 102 |
| AOI enter/leave | — | 16931 / 8835 |
| cadence_executions_total | 0 | 0 |
| cadence leaf cost | ~0 | ~0 |

**CadenceTable debt:** no cadence consumers in this workload (`cadence_executions_total=0`); cadence leaf stayed ~0. Memory growth is mild and does **not** invalidate ladder evidence. Debt remains known; **not fixed** in 7.3.

## Known debt

- Unreaped `CadenceTable` registrations (prior debt) — not exercised by representative presets.
- High-N harness post-ramp polling / snapshot starvation (measurement hygiene; not optimized here).
- Mixed-runtime portal role failures at some N (harness correctness under dense hotspot timing).
- Unattributed tick share 20–40% at higher N — instrumentation completeness for 7.5, not a capacity cliff.
- Short-cell `process_resources` CPU end samples often 0; use NDJSON mid-run for CPU.

## Remaining uncertainty

- Where **server** tick/replication would cliff beyond 128 mixed / 64 dense on this host.
- Whether WAN/loss changes writer-queue / drain before CPU does (7.4 territory).
- How much portal/harness FAILED masks gameplay fidelity at high N even when server is healthy.

## Exact recommended target for Phase 7.4

**Begin Phase 7.4 — Network Throughput & Backpressure** with:

1. Primary envelope: `representative_mixed` @ **64 and 128** (issuance already proven); dense@32–64 as secondary fan-out stress.
2. Allow **replication policy tuning** and transport/backpressure measurement; **do not** rebuild Phase 6 AOI architecture.
3. Treat harness `snapshot_starvation` / portal FAILED as **non-server** unless server queue/drain/tick evidence concurs.
4. Optional measurement hygiene (out of band or early 7.4 harness-only): reduce post-ramp O(N) poll cost so high-N cells stay COMPLETE without changing server code.
5. Do **not** start 7.5 CPU owner optimization until 7.4 closes network ownership questions for this envelope.

## Implemented scope (code/config for ladders)

- `NpcWorkloadConfig.health_max` + `with_lifecycle_churn`
- `World` respawn delay override; death path uses configured delay + entity health max
- `LoadPressure::arm_npcs` applies health/delay
- `capacity_ladder.ps1` NPC workload overrides + harness artifact copy
- `capacity_73_ladder.ps1` suite runner + `phase73_summary.json`
- Release unused-import cfg gate in `stage.rs` (release warning hygiene)

No server bottleneck optimization. No replication policy change during ladders.

## Tests / quality gate

- Unit: health_max default; configured respawn delay + health max on death
- Full gate: `./scripts/check.ps1` — **PASS** 2026-09-01 (`fmt`, `check`, `clippy -D warnings`, `test --workspace`)

## Deviations

- Activity ladder tick p99 did not rise with rate (expected possible; recorded honestly).
- Some NPC-ladder cells at high count recorded 0 deaths in 45s (dilution); lifecycle proof already established churn.
- Soak chosen at mixed@16 (below harness starvation onset), not at saturated cell (none identified).

## Boundary

Work **stops before Phase 7.4**.
