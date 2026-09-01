# Phase 6G.2 Capacity Characterization Report

**Status:** measure-only Pass 1 **complete** — **stop for owner review**. No 6G.3 redesign in this campaign.

Protocol **v10**. Live UDP metrics schema **4** (Mixed execution totals already in tree). Domain timings remain **file artifacts only**. Root `PHASE` = `6G.2`. ADR-0053.

Localhost figures are **not** player-capacity claims.

## Machine / build

| Field | Value |
|---|---|
| Date | 2026-09-01 |
| OS | Windows 11 Home 10.0.26200 |
| CPU | 13th Gen Intel Core i7-13650HX (14C / 20 logical) |
| RAM | ~15.7 GiB |
| Build | Release `purgatory-server` + `purgatory-load` |
| Git | `077fa0c` (+ uncommitted 6G.2 instrumentation) |
| Helper | `scripts/capacity_ladder.ps1` |
| Artifacts | `logs/load/capacity_6g2/` |
| Summary JSON | `logs/load/capacity_6g2/ladder_summary.json` |

## What landed (code)

1. Coarse tick-domain accounting → `tick_domains.json` / `.ndjson`
2. Process CPU % + working-set start/end/peak → `process_resources.json`
3. `PURGATORY_LOAD_PLACEMENT=hotspot` (tight mutual-AOI pack)
4. Portal classify split: `portal_never_eligible` vs missing transition
5. Hub-spawned server default `PURGATORY_DATA_DIR=logs/dev-tools/hub_server_persist`
6. Docs: ROADMAP 6G.1–6G.4, ADR-0053, `MMO_RUNTIME_BASELINE.md`, TEST_GATES Gate 6G.2

**Hard stops honored:** no AOI/replication rewrite, no pending-cap raise, no Phase 7, no domain fields added to the UDP datagram.

## Quality gate

`./scripts/check.ps1` — **PASS** after instrumentation.

## 6G.2B empirical notes

### 128-load stall (measure only)

Instrumented Release runs at 64/128 (idle + mixed hotspot) kept tick p99 well under 33 ms with **AOI dominant**. A 120s @128 mixed run WARN'd on unexpected disconnects mid-ramp but did **not** hang and had **0 tick overruns** (domain p99 ≈ 6.6 ms). Historical “128 stall” **not reproduced** as a server-domain blow-up. `PREDICTION_PENDING_CAP` stays 128.

### Portal / probe

`portal_never_eligible` classify path unit-covered. Hub default persist under `logs/dev-tools/hub_server_persist` unit-covered.

### Windows exe replace

While server live: overwrite of `purgatory-server.exe` → sharing violation (expected). After Stop: overwrite OK. Evidence: `logs/load/capacity_6g2/windows_exe_replace_evidence.txt`.

## 6G.2C ladder

Full table: [`MMO_RUNTIME_BASELINE.md`](MMO_RUNTIME_BASELINE.md). Idle tick p99 ms: `32 → 1.4`, `64 → 5.1`, `128 → 8.7`, `256 → 16.0` (1 overrun @256 + disconnect WARN).

## Official 30-minute instrumented soak

| Field | Value |
|---|---|
| Status | **COMPLETE** |
| Harness dir | `logs/load/20260901_050045_8bots_soak_seed4242_r10925bfd` |
| Capacity dir | `logs/load/capacity_6g2/20260901_080043_soak_official` |
| Duration | 1800.1 s |
| Peak connected | 8 |
| Unexpected disconnects | 0 |
| `input_handoff_dropped` | 0 |
| Tick overruns | 1 |
| Summary tick work p95/p99/max (peak-of-window) | 5.1 / **14.4** / **101.9** ms |
| Domain end-window tick p99 | 1.50 ms (AOI 0.86) |
| WS peak | 10.7 MB |

Domain end-window is the quiet final minute; summary peak-of-window captures the single spike that produced the overrun.

## Ranked bottlenecks (evidence only)

1. **Spatial / AOI** — dominant on nearly every ladder cell at N≥32. Example: idle@256 AOI p99 ≈ **12.7 ms** of tick p99 **16.0 ms** (~**79%** of tick).
2. **High-N connect/ramp instability** — unexpected disconnects near ~170–180 sessions on several 256-count runs (and one 128 mixed).
3. **Replication** — secondary to AOI in Pass 1 samples.
4. **Rare tick spikes under Mixed soak** — one overrun with max tick work ≈ 102 ms over 30 minutes at N=8 (investigate in 6G.3 only if owner prioritizes tail latency).

## Explicit non-fixes this campaign

- Did not rewrite AOI / spatial index
- Did not raise `PREDICTION_PENDING_CAP`
- Did not add domain fields to UDP metrics
- Did not start Phase 7 / 6G.3

## Owner decision needed

Proceed to **6G.3** only if evidence above justifies a targeted remediation (primary candidate: **AOI fan-out under multi-observer visibility** at high N). Secondary: high-N session ramp disconnects. Tertiary: rare Mixed soak tick spikes.
