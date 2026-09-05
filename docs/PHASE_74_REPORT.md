# Phase 7.4 — Network Throughput & Backpressure

**Status:** **complete / closed 2026-09-01.** Stop before Phase 7.5. Do not begin 7.5 until instructed.

Root `PHASE` = `7.4`. Phase 6 remains GREEN (architecture closed). Localhost figures are **not** player-capacity claims.

Plan: [`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md). Prior ladder: [`docs/PHASE_73_REPORT.md`](PHASE_73_REPORT.md). Artifacts: `logs/load/capacity_74/`. Summary: latest `logs/load/capacity_74/summary_*/phase74_summary.json`.

## What 7.4 is

Characterize and harden the **outbound path** under representative workloads; exercise existing byte-budget / cadence policy knobs; introduce a controlled slow-receiver diagnostic. **Measure → identify limit → tune only where justified → re-measure.** No Phase 6 AOI/fan-out/packer redesign. No broad “fix server performance” campaign.

## Machine / build

| Field | Value |
|---|---|
| Date | 2026-09-01 |
| OS | Windows 11 Home 10.0.26200 |
| CPU | 13th Gen Intel Core i7-13650HX (14C / 20 logical) |
| Build | Release `purgatory-server` + `purgatory-load` |
| Helper | `scripts/capacity_74_ladder.ps1` (+ `capacity_ladder.ps1` knobs) |
| Policy baseline | `selective` / `high` |
| Lifecycle override | `health_max=6`, `respawn_delay=20` (from 7.3) |
| Harness hygiene | `--post-ramp-thin` for N≥32–64 cells |

## Implemented scope

### 7.4A — Outbound path ownership

`network_pressure.json` **schema 2** adds:

`frames_publish_attempt → frames_encoded → frames_enqueued → frames_drained`, plus `bytes_encoded` / `bytes_drained` / `write_calls` / `enqueue_attempts` / `queued_frames_current`.

Note on every snapshot: **`write_drain` is write/drain/backpressure latency, not QUIC CPU.**

Tick leaves remain encode/enqueue CPU; drain stays off-tick in the pressure book.

### 7.4B — Slow-client diagnostic

Harness: `--slow-drain-count N` / `PURGATORY_LOAD_SLOW_DRAIN_COUNT` — lowest `bot_id` bots skip uni snapshot drain (receiver pressure). Reuses no new emulator.

### 7.4C–D — Policy knobs (existing architecture)

| Env / flag | Role |
|---|---|
| `PURGATORY_REPLICATION_FRAME_BUDGET_BYTES` | Soft frame budget base (clamped 512…hard max) |
| `PURGATORY_REPLICATION_STRANGER_CADENCE_MULT` | Extra stranger cadence multiplier (1–8) |
| Existing | `PURGATORY_REPLICATION_POLICY`, `PURGATORY_POPULATION_CLASS` |

### 7.4H — Harness hygiene (measurement only)

`--post-ramp-thin` / `PURGATORY_LOAD_POST_RAMP_THIN=1`: after ramp, rotate ≤16 bots × drain 4 instead of O(N)×8. **Reported separately** from server policy. Does not eliminate portal-role FAILED reasons.

## Tested throughput envelope

All cells: **100% issuance**, `funnel_invariant_ok` (where recorded).

| Suite | Cells |
|---|---|
| Baseline | mixed@64, mixed@128 |
| Byte budget | mixed@64 × {4096, 2048, 1024, 512} |
| Cadence mult | mixed@64 × {1, 2, 4} |
| Slow drain | mixed@64 / 4 slow; mixed@32 / 2 slow |
| Dense | dense@32, dense@64 |
| Candidate | mixed@64 + dense@32 with budget 2048 + cadence×2 |
| Soak | mixed@64 / **8m** with candidate knobs |

## Peak outbound bandwidth

| Cell | bytes_out/s | bytes/client/s | tick p99 | util % | queue depth max | push fails |
|---|---|---|---|---|---|---|
| mixed@64 baseline | ~224 KB/s | ~3584 | 2.11 | 3.4 | **1** | **0** |
| mixed@128 baseline | **~534 KB/s** | ~4269 | 5.08 | 8.6 | **1** | **0** |
| dense@64 | ~396 KB/s | ~6342 | 3.96 | 6.8 | **1** | **0** |
| dense@32 | ~263 KB/s | ~8424 | 1.96 | 3.7 | **1** | **0** |

Peak observed in this envelope: **~0.53 MB/s** aggregate outbound (mixed@128). Per-client ~3.5–8.4 KB/s depending on hotspot density.

## Outbound path ownership (evidence)

Typical mixed@64: `frames_encoded ≈ frames_enqueued ≈ frames_drained` (e.g. 84695 / 84695 / 84663). Drain p99 **2–6 µs**, queue age p99 **≲300 µs**. Encode/enqueue tick leaves remain small (encode mean ~0.03–0.18 ms).

## Queue depth / age behavior

Across **all** 7.4 cells: `writer_queue_depth_max = 1`, `writer_queue_push_fail_total = 0`. No unbounded growth. Cap (4) never bound.

## Slow-client behavior

| Observation | Result |
|---|---|
| Intentional non-drain bots | Harness may classify those bots as `snapshot_starvation` — **expected**, not server saturation |
| Writer queue | Still max depth **1**, push fails **0** on localhost |
| Tick | No global tick stall (util ~2–4%, overruns 0) |
| Healthy peers | Continued issuance/attainment 100% |
| Memory | No runaway (RSS ~13–16 MB on 32–64 cells) |

**Finding:** On **localhost**, Quinn/OS send buffering absorbs short receiver stalls; the application writer queue does **not** fill within 45s. True transport backpressure under flow-control exhaustion was **not** reproduced here. Isolation property still holds: slow bots did not stall the sim tick. WAN/loss remains an open uncertainty for 7.5+ / field conditions.

## First transport / backpressure pressure point

**None inside the tested envelope.**

Saturation class stayed `unknown_unattributed` on every cell. Queue never at cap; drain ≪ tick budget; no exclusive `server_transport_backpressure` evidence. **Server transport limit remains beyond mixed@128 / dense@64 on this host.**

Harness portal FAILED / intentional slow-bot starvation remain **non-server**.

## Byte-budget results (mixed@64)

| Soft budget | budget_deferred | priority_deferred | bytes/client/s | tick p99 |
|---|---|---|---|---|
| 4096 (default) | 0 | 0 | ~3516 | 2.04 |
| 2048 | 0 | 0 | ~3886 | 2.00 |
| 1024 | **552** | **552** | ~3499 | 2.06 |
| 512 | **1346** | **1346** | ~3439 | 2.43 |

Budget **begins to bind near 1024** under representative_mixed@64. Default 4096 still does not bind (matches 6G/7.3). Self/critical packing path unchanged (architecture). No recovery/resync storm observed (`recovery` remained quiet).

## Cadence results (mixed@64)

| Mult | cadence_deferred | bytes/client/s | tick p99 | encode mean |
|---|---|---|---|---|
| ×1 | ~1.09M | ~3496 | 1.82 | 0.042 |
| ×2 | ~1.30M | ~3145 (−10%) | 1.98 | 0.031 |
| ×4 | ~1.57M | ~2546 (−27%) | 2.12 | 0.024 |

×2–×4 reduce outbound bytes via stranger cadence stretch; tick cost flat. **Provisional** bandwidth lever — not a silent correctness free lunch (stale stranger transforms increase).

## Baseline vs candidate

**Candidate (provisional):** `selective` + `high` + soft budget **2048** + stranger cadence **×2**.

| Workload | Baseline bytes/cli/s | Candidate | Tick p99 | Queue |
|---|---|---|---|---|
| mixed@64 | ~3584 | ~3123 (−13%) | 2.11 → 2.10 | depth 1 / fails 0 |
| dense@32 | ~8424 | ~6047 (−28%) | 1.96 → 2.43 | depth 1 / fails 0 |

Cost: modest bandwidth drop; tick still ≪ budget. Quality: Enter/Leave counters continue; no budget bind at 2048; cadence_deferred rises as designed. **Not** a production SLA — gameplay-facing staleness of distant strangers needs later playtest.

## Selected candidate production policy

| Knob | Value | Status |
|---|---|---|
| Policy mode | `selective` | **Measured** (6G.7C + 7.4) |
| Population class | `high` for load; default `low` for quiet | **Measured** |
| Soft frame budget | **4096** default | **Measured** — keep; 2048 optional under bandwidth pressure |
| Soft budget under constrained bandwidth | 1024–2048 | **Provisional** — binds at ≤1024 on mixed@64 |
| Stranger cadence mult | **1** default; **2** optional | **Provisional** — ~10% byte cut @×2 |
| Writer queue cap | **4** | **Measured** — never bound in 7.4 |
| Critical/self | every tick (existing) | **Measured** — preserve |
| Stranger health | suppressed (existing selective) | **Measured** |
| Slow-client | per-pipe coalesce; no disconnect-on-full yet | **Measured** on localhost (queue never fills) |
| Disconnect pathological peer | write_all failure tears session (existing) | Unchanged |

Do **not** call provisional values final production SLA.

## Memory / backlog

| Cell | start → peak → end (MB) | Notes |
|---|---|---|
| soak_mixed64 (8m) | 12.1 → 15.8 → 15.1 | Stable; no unbounded growth |
| All ladder cells | ~13–22 MB end | Cleanup after disconnect not stressed (no disconnect storms) |

Backlog: frames_enqueued ≈ frames_drained; queue does not accumulate.

## Soak result

mixed@64 / 8m / candidate knobs: tick p99 **1.66**, util **3.3%**, queue depth max **1**, push fails **0**, memory stable. Harness exit FAILED for portal/occupancy reasons — **not** transport saturation. Snapshot starvation sample present under thin-poll + portal — recorded as harness caveat.

## Backpressure order verification

| Principle | Status in envelope |
|---|---|
| Preserve authoritative simulation | **OK** — util ≪ 100%, overruns 0 |
| Preserve critical/self | **OK** — selective self cadence 1 unchanged |
| Defer/coalesce lower priority | **OK** — cadence_deferred / budget_deferred when knobs bind |
| Avoid unbounded queues | **OK** — depth max 1 |
| Isolate slow clients | **OK** for tick; localhost did not stress per-pipe fill |
| Disconnect pathological | Existing write-fail path; not newly triggered |

No silent Enter/Leave ordering violations observed in counters.

## Harness caveat (separate)

- Post-ramp thin poll used for N≥32/64 network cells (measurement hygiene).
- Many exits still FAILED (`portal_*`, occupancy) — **not** capacity.
- Intentional slow-drain bots can inflate `snapshot_starvation` — exclude from server conclusions.

## Remaining uncertainty

- WAN / packet loss / small QUIC windows — when writer queue actually binds.
- Gameplay-visible staleness under cadence×2 / budget≤1024 (needs playtest).
- Whether longer slow-drain soaks eventually fill Quinn FC on localhost.
- Unattributed tick share under high-N (7.5 CPU ownership).

## Recommended Phase 7.5 optimization target

**Simulation / CPU scaling of measured owners**, especially:

1. `replication` / discover-policy-encode path cost as observers grow (still ≪ tick budget but rising @128).
2. Unattributed share reduction for cleaner ownership.
3. Optional: field/WAN slow-client follow-up if transport backpressure remains invisible on localhost.

Do **not** start 7.5 with a transport-queue optimization — **no transport cliff was found**.

## Quality gate

`./scripts/check.ps1` — **PASS** 2026-09-01 (`fmt`, `check`, `clippy -D warnings`, `test --workspace`, content validator).

## Boundary

Work **stops before Phase 7.5**. No AOI/fan-out architecture redesign.
