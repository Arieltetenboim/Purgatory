# MMO Runtime Baseline (Phase 6G.2)

Reference freeze for capacity characterization. Localhost figures are **not** player-capacity claims.

## Identity

| Field | Value |
|---|---|
| Phase marker | `6G` (GREEN — architecture closed; production policy tuning deferred) |
| Protocol | v10 |
| Live metrics schema | 4 (UDP `PURGSTAT`; Mixed execution totals; 1 Hz gauges; datagram cap 4096). Capacity domain timings remain **file artifacts only** — Pass 1 did not enlarge the datagram further for domain breakdown. |
| Capacity artifacts schema | tick_domains / process_resources schema 1 (run files only) |
| Build mode for ladders | Release |
| Git | `077fa0c` (+ uncommitted 6G.2 instrumentation) |
| Machine | Windows 11 Home 10.0.26200; Intel i7-13650HX (14C/20T); ~15.7 GiB RAM |
| Date | 2026-09-01 |

## Runtime configuration

| Setting | Value |
|---|---|
| Tick rate | 30 Hz (`TICK_DURATION` ≈ 33.33 ms) |
| Admission (load-mode) | `PURGATORY_ADMISSION_CAP=256` |
| Metrics | `127.0.0.1:5002` |
| Maps | FOOTNOTE Map A + Map B (dev-eager) |
| AOI | view envelope + 2 wu prefetch / leave hysteresis |
| Persist | isolated `PURGATORY_DATA_DIR` under run dir / Hub `logs/dev-tools/hub_server_persist` |
| Capacity artifacts | `PURGATORY_CAPACITY_ARTIFACT_DIR` → `tick_domains.json`, `process_resources.json`, `aoi_locality.json`, `replication_fanout.json`, NDJSON tails |
| Ladder helper | `scripts/capacity_ladder.ps1` (`--ramp-ms 50`) |

## Coarse tick domains (Pass 1)

```text
Tick Total
├─ Commands / Input
├─ Simulation / Movement
├─ Gameplay Services (scheduler/actions/effects + spawn/despawn aggregated)
├─ Spatial / AOI
├─ Replication
└─ Persistence enqueue
```

Process resources recorded: logical CPUs, working set start/end/peak, CPU time and utilization %.

## Scenarios (ladder 32 → 64 → 128 → 256)

| Scenario | Harness / env |
|---|---|
| Idle | `--profile idle --scenario load` |
| Distributed | `--profile walker`, `PURGATORY_LOAD_PLACEMENT=spread` |
| Hotspot (mutual AOI) | `--profile mixed`, `PURGATORY_LOAD_PLACEMENT=hotspot` |
| Churn | `--scenario churn` |
| Gameplay-service burst | `--preset scheduler` (finite) |

Stop rule: crossing 33.33 ms p99 is a **recorded signal**, not an automatic ladder stop. Stop only on operational instability or meaningless evidence.

## Results (Release, 2026-09-01)

Domain timings are **window** percentiles from the last ~120 ticks at artifact flush (not full-run histograms). Peak bots may be below N while ramping; @256 runs often WARN'd with unexpected disconnects before full population.

| Scenario | N | tick p50/p95/p99/max (ms) | dominant domain (p99) | CPU util % | WS peak MB | Notes |
|---|---:|---|---|---:|---:|---|
| idle | 32 | 0.66 / 1.09 / 1.39 / 1.59 | aoi 1.01 | 7.6 | 11.2 | COMPLETE |
| idle | 64 | 2.56 / 3.76 / 5.05 / 5.17 | aoi 3.70 | 52.8 | 13.6 | COMPLETE |
| idle | 128 | 5.94 / 7.95 / 8.70 / 9.56 | aoi 7.12 | 10.7 | 18.2 | COMPLETE |
| idle | 256 | 8.31 / 12.32 / 16.02 / 16.53 | aoi 12.74 | 64.3 | 22.2 | WARN disconnects; 1 overrun |
| distributed | 32 | 0.76 / 1.28 / 1.59 / 1.70 | aoi 1.27 | 0.0 | 11.5 | COMPLETE |
| distributed | 64 | 1.62 / 2.26 / 2.66 / 3.10 | aoi 2.01 | 35.7 | 13.3 | COMPLETE |
| distributed | 128 | 5.82 / 8.92 / 9.73 / 9.93 | aoi 7.49 | 51.5 | 18.2 | COMPLETE |
| distributed | 256 | 7.22 / 10.24 / 10.51 / 10.58 | aoi 7.64 | 68.7 | 22.0 | WARN disconnects |
| hotspot | 32 | 0.99 / 2.01 / 2.22 / 2.28 | aoi 1.39 | 59.1 | 11.6 | COMPLETE |
| hotspot | 64 | 2.03 / 3.52 / 4.74 / 5.04 | aoi 2.94 | 51.8 | 13.7 | COMPLETE |
| hotspot | 128 | 4.53 / 6.54 / 9.32 / 10.12 | aoi 6.44 | 59.1 | 18.9 | COMPLETE |
| hotspot | 256 | 4.76 / 6.60 / 8.30 / 10.36 | aoi 5.70 | 7.6 | 21.0 | WARN disconnects; pop incomplete |
| churn | 32 | 0.73 / 1.08 / 1.13 / 1.64 | aoi 0.76 | 0.0 | 11.9 | COMPLETE |
| churn | 64 | 1.63 / 2.39 / 2.65 / 2.75 | aoi 1.80 | 0.0 | 14.1 | COMPLETE |
| churn | 128 | 4.79 / 7.77 / 9.12 / 9.55 | aoi 6.08 | 4.6 | 18.6 | COMPLETE |
| churn | 256 | 4.23 / 5.69 / 6.92 / 7.21 | aoi 4.81 | 80.6 | 22.6 | COMPLETE (churn pop dynamics) |
| scheduler | 2 | 0.08 / 0.17 / 0.26 / 0.27 | svc 0.13 | 6.1 | 9.2 | service burst finite |
| mixed-stall | 128 | 2.97 / 5.28 / 6.57 / 6.69 | aoi 4.64 | 46.8 | 19.1 | 120s ramp50; WARN disconnects mid-run |
| Mixed soak 30m | 8 | domain end-window 0.77/1.22/1.50/1.55; summary peak-of-window p99 **14.4** / max **101.9** (1 overrun) | aoi (end-window) | 4.7 | 10.7 | **COMPLETE** 1800s; `20260901_080043_soak_official` + harness `20260901_050045_8bots_soak_seed4242_r10925bfd`; disconnects=0; handoff_dropped=0 |

Machine-readable: `logs/load/capacity_6g2/ladder_summary.json`.

## Suspected bottlenecks (evidence only — no redesign in 6G.2)

1. **Spatial / AOI** — dominant domain on nearly every ladder cell at N≥32; at idle@256 AOI p99 ≈ 12.7 ms (~77% of tick p99 16.0 ms).
2. **High-N connect/ramp instability** — unexpected disconnects while approaching ~170–180 sessions on several 256 (and one 128 mixed) runs; operational signal, not a tick-budget claim.
3. **Replication** — secondary to AOI in Pass 1 samples.
4. **Gameplay services** — only dominant on the finite scheduler preset (as intended).

See [`docs/PHASE_6G2_REPORT.md`](PHASE_6G2_REPORT.md).
