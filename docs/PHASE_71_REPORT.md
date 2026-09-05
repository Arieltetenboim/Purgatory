# Phase 7.1 — Capacity instrumentation and work ownership

**Status:** **complete / closed 2026-09-01.** Phase 7.2 authorized to begin. High-N ramp finding (7.1F) remains harness-owned issuance limitation and is **deferred to Phase 7.3** for deeper isolation — not fixed in 7.2.

Root `PHASE` advanced to `7.2` after this close. Phase 6 remains GREEN (architecture closed). This report records measurement/attribution work only. Localhost figures are **not** player-capacity claims.

Plan: [`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md). Artifacts: `logs/load/capacity_71/`. First validation summary: `logs/load/capacity_71/summary_20260901_191936/phase71_summary.json`.

## What 7.1 is

7.1 extends existing coarse tick-domain and process artifacts into **trustworthy work-ownership attribution**. It does not optimize, raise admission, bump the live UDP metrics schema, or redesign Phase 6 AOI/replication.

Rule: **measure → attribute ownership → report. Do not optimize.**

## Machine / build

| Field | Value |
|---|---|
| Date | 2026-09-01 |
| OS | Windows 11 Home 10.0.26200 |
| CPU | 13th Gen Intel Core i7-13650HX (14C / 20 logical) |
| RAM | ~15.7 GiB |
| Build | Release `purgatory-server` + `purgatory-load` |
| Helper | `scripts/capacity_ladder.ps1`, `scripts/capacity_71_validate.ps1` |
| Artifacts | `logs/load/capacity_71/` |
| Protocol | v10 |
| Live UDP `PURGSTAT` | adopted contract remains **schema 3** in docs; `LOAD_METRICS_SCHEMA_VERSION` left as compiled (**4**, not bumped for 7.1). Domain ownership stays in **file artifacts**. |

## Implemented scope

### Tick ownership (file schema 2)

- Leaf owners under gameplay: scheduler, actions, effects, entity_lifecycle, cadence.
- Leaf owners under replication: discover, policy, encode, enqueue.
- Parents (`gameplay_services`, `replication`) remain on the snapshot for continuity but are **not** summed into remainder.
- `unattributed = total − attributed` on **non-overlapping leaves**. Negative remainder is clamped and counted as `accounting_error`.
- Tick-budget fields: utilization vs `TICK_DURATION`, remaining/overrun, consecutive streak, achieved Hz.
- Percentiles use the same window: last **120** ticks (`TICK_DOMAIN_WINDOW_SAMPLES`).
- `dominant_owner` = highest **mean share** over that window. `worst_spike_owner` = max spike (separate).
- `PURGATORY_CAPACITY_DETAIL`: unset/`1` = child owners (default **on**); `0`/`false`/`off` = coarse 6G.2 leaves only.

### Process / network / saturation

- Process snapshot: raw CPU vs normalized-per-logical-core; Windows `handle_count`; `thread_count` stays `None` (no cheap API).
- `network_pressure.json`: queue depth, `try_push` fails, **write_drain** (write/`write_all`/backpressure latency — **not** automatically QUIC CPU/send cost), queue age, top-8 clients, byte rates.
- Saturation classes (heuristic, not a verdict):
  1. `simulation_tick`
  2. `server_transport_backpressure`
  3. `harness_client`
  4. **`unknown_unattributed`** (default; mixed or insufficient exclusive evidence)

Never force a named class. Mixed evidence → `unknown_unattributed`.

### Connection lifecycle + harness

- Server stages start at transport accept / Hello visibility. Client/harness owns connect-attempt start. Correlate via shared `wall_secs` in the run directory — do not invent a server timestamp for an unobserved stage.
- Harness writes `harness_resources.json` and `harness_connection.json` to the load log dir **and** copies them into the capacity artifact dir (persist parent) when `--persist-root` is set.

### Live Hub

- `capacity_live.json` ~1 Hz from the existing tick-domain flush (not a fatter datagram).
- Hub Validation/Performance compact strip; owner table behind `show_full_details`.
- CPU labels: raw **100 = one logical core busy**; normalized **100 = all logical cores busy**.

## Important files

| Area | Path |
|---|---|
| Accounting + classification | `crates/common/src/capacity_accounting.rs` |
| Tick flush / live JSON | `apps/server/src/network/tick_domains.rs` |
| Network pressure | `apps/server/src/network/network_pressure.rs` |
| Connection histograms | `apps/server/src/network/connection_lifecycle.rs` |
| Handshake `write_all` drain | `apps/server/src/network/handshake.rs` |
| Gameplay / replication splits | `apps/server/src/network/gameplay.rs`, `replication.rs` |
| Drain/apply Instant | `crates/simulation/src/runtime.rs` |
| Hub strip | `apps/dev_hub/src/ui/run_view.rs` |
| Live file reader | `crates/dev_runtime/src/validation.rs` |
| Harness artifacts | `tools/bot_client/src/controller.rs` |
| 7.1F ramp funnel | `connection_ramp.json`; `scripts/capacity_71f_ramp.ps1` |
| Ladder / validate | `scripts/capacity_ladder.ps1`, `scripts/capacity_71_validate.ps1` |

## Tests added or changed

- Remainder identity, overlap → `accounting_error`, dominant-from-means vs spike, exclusive saturation classification (`crates/common` + `tick_domains` unit tests).
- Hub/dev_runtime parse of `capacity_live.json`.
- 6G.6 locality test renamed to `dense_cluster_tiny_move_dirties_only_mover_when_xor_empty` (XOR empty on tiny same-cell move; **not** a 7.1 contract change).
- 7.1F: `compose_ramp_ownership` / `ramp_attainment_pct` funnel statements (`crates/common`).

## Quality gate

`./scripts/check.ps1` — **PASS** after slices 1–5 and again after 7.1F (`PURGATORY quality gate OK`, 2026-09-01). Includes fmt, check, clippy `-D warnings`, workspace tests, content validator.

## Validation (measure only)

Admission stayed **256**. `PREDICTION_PENDING_CAP` unchanged. Replication policy `selective` / population `high` to match 6G.7C hotspot.

### Overhead — Mixed 8, detail 0 vs 1

| Run | Dir | tick p99 | util | dominant | unattributed mean | class |
|---|---|---:|---:|---|---:|---|
| detail 0 | `20260901_191936_mixed-stall_8n` | 0.312 ms | 0.42% | `simulation_movement` | 0.033 ms | `unknown_unattributed` |
| detail 1 | `20260901_192016_mixed-stall_8n` | 0.27 ms | 0.44% | `unattributed` | 0.051 ms | `unknown_unattributed` |

Detail 1 tick p99 was **not** worse than detail 0. Instrumentation overhead is not a distorting factor on this Mixed 8 sample.

Detail 1 remainder is larger because child Instant spans do not cover 100% of parent wall time. That remainder is **visible**, not folded into another owner. `accounting_error_ticks` = 0 on every 7.1 validation run below.

### Baseline / mid / upper (same 30 s / 50 ms ramp as 6G.7C for 128/256)

| Run | Dir | requested N | sessions (end) | tick p99 | util | overruns | dominant | class |
|---|---|---:|---:|---:|---:|---:|---|---|
| Mixed 8 baseline | `20260901_192040_mixed-stall_8n` | 8 | 8 | 0.284 ms | 0.46% | 0 | `unattributed` | `unknown_unattributed` |
| hotspot 128 | `20260901_192103_hotspot_128n` | 128 | **62** | 1.384 ms | 2.0% | 0 | `unattributed` | `unknown_unattributed` |
| hotspot 256 | `20260901_192136_hotspot_256n` | 256 | **63** | 1.078 ms | 2.0% | 0 | `unattributed` | `unknown_unattributed` |

**Occupancy caveat:** 128 and 256 both ended near ~62–63 sessions in 30 s (~2 accepts/s). Server-visible `hello_to_welcome` is ~9–14 ms mean; overall accept rate is much lower (handshake/persist/TLS queue ahead of Hello). These cells **cannot** be compared to 6G.7C hotspot@256 (tick p99 ≈ 4.8 ms at reported full occupancy). They are **not** 128- or 256-capacity results.

At the occupancy that *did* land, tick p99 stayed ~1.1–1.4 ms, utilization ~2%, 0 overruns, write-drain p99 ≈ 0.003 ms, queue depth max 1. Classification stayed `unknown_unattributed` because there was no exclusive sim-overrun, transport-queue, or admission-wall evidence — correct.

Owner shares at hotspot 256 (63 sessions, last 120 ticks): unattributed ~39%, `simulation_movement` ~23%, `spatial_aoi` ~17%, replication leaves together ~18%. Dominant-from-mean is `unattributed`; that is a remainder finding, not a claim that simulation is idle in an absolute sense.

### Admission-wall diagnostic (`count=384`, admission=256)

**This run cannot establish 384-server capacity.** Purpose: prove telemetry/classification stays truthful at the configured wall. Admission stayed 256.

| Pass | Dir | duration / ramp | peak live (harness) | sessions accepted | disconnects | class | overruns |
|---|---|---|---:|---:|---:|---|---:|
| 1 (too short) | `20260901_192210_hotspot_384n` | 20 s / 50 ms | ~51 | 51 | 0 | `unknown_unattributed` | 0 |
| 2 (follow-up) | `20260901_192807_hotspot_384n` | 150 s / 10 ms | ~115 @ 96 s | 165 | 91 | `unknown_unattributed` | 0 |

Pass 2 **still did not hit admission 256**. Peak live bots ≈ 115, then a disconnect wave (`unexpected_disconnects=91`, `run_status=WARN`). End connected ≈ 74. Server `hello_to_welcome` p50 ≈ 19 ms; harness `attempt_to_welcome` p50 ≈ 24 ms; `harness_timeouts=0`; `connect_fail=0`; writer queue never at cap; write-drain p99 = 4 µs; tick p99 = 1.50 ms; utilization ≈ 2.7%.

Classification stayed **`unknown_unattributed`**. That is the correct honesty bar: there was no exclusive admission-wall evidence (`connected < 256`, no refusals), no exclusive sim-overrun evidence, and no exclusive transport-queue evidence. Disconnects alone are **not** forced into `harness_client` without timeout/starvation/admission-wall evidence.

Pass 2 **did** prove the diagnostic’s telemetry purpose: a 384-count connect storm with 91 drops was **not** labeled `simulation_tick`. Tick stayed healthy. Harness JSON landed in the shared capacity dir (`harness_resources.json`, `harness_connection.json`).

`session_to_disconnect` p50 ≈ 89 s on the 91 drops is a later-phase question (idle/keepalive vs harness), not a 7.1 fix.

## 7.1F — Connection ramp attribution

Measure-only. No spawn-rate change, no admission raise, no QUIC/handshake semantic change. Harness **exit 0 is not attainment**.

Artifact: `connection_ramp.json` (+ `.ndjson` 1 Hz). Funnel: `requested → spawn issued → transport → Welcome → world entered → active`, with peaks, stage timings, fail/timeout counts, and contemporaneous server/harness CPU, tick p99, queue/write-drain.

### Diagnostic run

| Field | Value |
|---|---|
| Dir | `logs/load/capacity_71/20260901_194926_hotspot_384n` |
| Workload | hotspot `--count 384` `--duration 60s` `--ramp-ms 10`; admission **256** |
| Harness `run_status` | **COMPLETE** (exit 0) |
| `requested_clients` | 384 |
| `spawn_issued_total` | **114** |
| `controller_ticks` | **114** (one issue per controller tick) |
| `connect_attempts_completed` | 112 (`pending_in_flight` 2) |
| `peak_connection_attempts` | 2 (in-flight peak) |
| `transport_established_total` | 115 |
| `welcome_total` / `world_entered_total` | 115 / 115 |
| `peak_active_clients` | 114 |
| `attainment_pct` | **29.7%** |
| connect fail / harness timeout / admission refused | 0 / 0 / 0 |
| server tick p99 / util | 3.39 ms / 6.0% |
| write_drain p99 / queue depth max | 0.005 ms / 1 |
| server CPU raw / normalized | 61.9% of one core / 3.1% machine |
| harness CPU raw / normalized | 35.7% of one core / 1.8% machine |
| saturation_class | `unknown_unattributed` (no exclusive sim/transport/admission-wall evidence) |

**Measurement errata (pre–funnel schema 2):** `transport` / `Welcome` / `world_entered` in that artifact were `.max(harness, server_lifecycle)`. Server cumulative accepts can exceed harness `spawn_issued` when reconnects/probes exist or when in-flight completes race the drain. The **114 issued / ~1.9 Hz** ownership conclusion is unchanged; do **not** treat 115 downstream as “more completions than issues.” Funnel schema 2 keeps issuance stages harness-owned and records server totals as correlators + `funnel_invariant_*`.

**Ownership statement:**

> 384 were requested, but the harness only created 114 connection attempts within the run window.

Downstream stages (transport / Welcome / world / active) track issued attempts. The missing 270 clients never left the harness spawn queue. Attempt→Welcome p50 ≈ 16 ms; Hello→Welcome p50 ≈ 15 ms; those stages are not the funnel drop.

Controller ticks in 60 s = 114 (~1.9 Hz), not 30 Hz. Issuance is coupled to that loop (at most one `queue_bot_connect` per tick). Deeper isolation of *why* the controller loop is ~2 Hz belongs in **7.3** if pursued; 7.1F does **not** change spawn cadence or bot poll timeouts.

This does **not** block 7.2. It closes the remaining high-N ramp uncertainty: the requested 384 workload was not achieved, and the owner is the **harness connection ramp**, not simulation tick, server transport backpressure, or the admission wall.

## CPU semantics (do not misread Hub)

| Field | Meaning |
|---|---|
| `cpu_utilization_pct` | Raw process CPU. **100 = one logical core fully busy.** |
| `cpu_normalized_per_logical_pct` | Raw / logical CPU count. **100 = all logical cores busy.** |

Mixed 8 live snapshots often show 0.0: 1 Hz sample interval can land on a quiet delta. 128/256/384 showed non-zero raw CPU (384 first pass ~30% of one core / ~1.5% machine-relative during connect storm).

## Known debt (not fixed in 7.1)

- CadenceTable not reaped on despawn
- `dev.probe` persist
- Generic entities still not on the wire (7.2 NPC visibility, not a 7.1 rewrite)
- Large unattributed remainder with detail on (gaps between Instant points, drain/send off the sim-tick leaves)
- High-N occupancy in 30 s matching 6G.7C duration is **not** reached on this host/path because the harness issues **one connect task per controller tick**, and that tick rate falls well below 30 Hz as live bots accumulate (7.1F).

## Explicit non-goals honored

No sharding, multithreading, AOI/replication architecture redesign, cadence/budget production tuning, admission/`PREDICTION_PENDING_CAP` raise, NPC/combat, Hub polish campaign, UDP schema bump.

## Deviations from the written plan

- UDP schema left as compiled (4) rather than documenting a bump; live 7.1 summary uses files.
- `thread_count` omitted (no cheap Windows API).
- First 384 pass used 20 s and did not reach the wall; validate script now uses 150 s / 10 ms ramp for that diagnostic only. 128/256 still use the 6G.7C 30 s / 50 ms recipe so occupancy shortfall is comparable.
- Harness resource JSON is written to the load log dir **and** copied into the capacity artifact dir (persist parent) so Hub/ladder share one folder.
- 6G.6 tiny-move test assertion updated to match XOR locality already in tree.

## Owner close (accepted)

1. Remainder + `unknown_unattributed` default is the intended honesty bar.
2. Occupancy-limited 128/256 cells are accepted as instrumentation evidence, not capacity.
3. Admission-wall diagnostic is accepted as **classification honesty** (wall not reached; class stayed `unknown_unattributed`) rather than a 256-fill proof.
4. Hub compact strip is acceptable without a visual polish pass.
5. 7.1F ramp owner (harness issued 114/384) is accepted; further controller-tick isolation is **deferred to 7.3**.

## Boundary

7.1 is **closed**. 7.2 begins separately (representative gameplay workload). No capacity SLA, no cap raise, no harness issuance fix in 7.1.
