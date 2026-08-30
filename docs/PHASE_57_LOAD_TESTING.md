# Phase 5.7 — Load Testing

Headless multiplayer load / soak / churn infrastructure. Real QUIC bots exercise
the same Hello → Welcome → `InputCommand` → authoritative tick → snapshot path
as the native client. No graphical windows. No Phase 6 gameplay.

## Quick start

1. Start the server (ordinary START uses admission **32**).
2. Developer Tools **Testing → Load Test** → choose count/profile/duration/seed.
3. If the metrics probe shows `admission_cap < count`, confirm **LOAD MODE** restart
   (`PURGATORY_ADMISSION_CAP=256`).
4. Watch the LOAD terminal dashboard (~1 Hz).
5. After the run, **ANALYZE LAST RUN** (uses `logs/load/last_finished.txt`).
6. **OPEN LAST REPORT** for `report/summary.md` and charts.

CLI:

```text
cargo run -p purgatory-bot-client --bin purgatory-load -- --count 10 --profile mixed --duration 2m --seed 1234
```

Safety: `--max-bots` defaults to 32; use `--allow-high-count` (launcher does) when the
server advertises a higher admission cap.

## Architecture

```text
purgatory-load
├─ Controller (ramp / burst / churn)
├─ Shared Quinn Endpoint
├─ BotSession × N  (30 Hz SimulationClock + IntentNet)
├─ UDP poll → LoadMetricsV1 (127.0.0.1:5002)
└─ logs/load/<run>/
```

Bots intentionally omit: winit/wgpu, prediction/replay, impairment, HeldCancel
(idle = Neutral commands). Authority remains on the server.

## Behavior profiles

| Profile | Behavior |
|---|---|
| Idle | Neutral every tick |
| Walker | Left/Right with seeded turns |
| Jumper | Walk + jump edges when `local_grounded` |
| Mixed | Idle / move / jump / drop-through mix |

`BotAction`: `Move`, `Jump`, `DropThrough`, `Idle` (future actions reserved).

Scenarios: `load` (ramp), `burst`, `churn`.

## Entity cap

`MAX_ENTITIES_PER_SNAPSHOT = 256` (mechanical protocol-v4 decode bound; layout
unchanged). `MAX_GAMEPLAY_SNAPSHOT_BYTES = 8192` remains the packet wall.

## Metrics (server-owned)

UDP localhost only: magic `PURGSTAT` + version 1. Missed poll ≠ zero — CSV leaves
cells empty. Tick overrun = **server** tick work > 33.333 ms (recorded; one overrun ≠ WARN).

CSV distinguishes:

| Column family | Meaning |
|---|---|
| `bot_scheduler_*_ms` | Harness wall-clock cadence between controller ticks (not server sim cost) |
| `server_tick_work_*_ms` | Authoritative server tick work from `LoadMetricsV1` |
| `server_*_per_sec` | Rates from counter deltas between 1 Hz samples |
| `server_memory_mb` / `harness_memory_mb` | Working Set (empty if unavailable) |

Run summary aggregates (O(1) memory, per-metric):

| Field | Aggregation |
|---|---|
| `server_tick_work_mean_ms` | **Tick-count-weighted** average of sample window means (weight = Δ`tick_count`; each sample mean is from a ≤120-tick server ring) |
| `server_tick_work_p95/p99_ms` | Peak of **window** percentiles across samples (`tick_percentile_semantics = peak_of_window_percentiles`) — **not** full-run percentiles |
| `server_tick_work_max_ms` | Peak of window max across samples |
| `server_tick_overruns_total` / bytes | Last valid cumulative counter |
| memory start/peak/end | First / max / last valid Working Set |
| queue maxima | Max across valid samples |

Missed polls leave CSV cells empty and do **not** erase prior aggregates.
`server_metrics_ok` is true iff ≥1 successful sample. Health is `none` / `partial`
(miss ratio ≥ 5%) / `healthy`, with sample ok/miss counts in `run_summary.json`.

Dashboard `Out` is **MB/s** (delta), not cumulative total.

Memory: in-process Working Set (`windows-sys`), one syscall per sample.

## Run artifacts

```text
logs/load/YYYYMMDD_HHMMSS_<count>bots_<profile>_seed<seed>/
  config.json
  metrics.csv
  events.ndjson      # sparse lifecycle / failure events (required non-empty on complete runs)
  run_summary.json   # authoritative machine-readable summary + status_reasons[]
  graphs/            # offline PNGs (tick, memory, population, queues, bandwidth, …)
  report/summary.md  # offline human report (lists graphs/)
logs/load/latest.txt
logs/load/last_finished.txt
```

Analyzer: `python tools/analyze_load_run.py --latest`

If matplotlib is missing, the analyzer prints:

```text
charts skipped: matplotlib is not installed.
```

and still writes `report/summary.md`. Chart render failures do not abort the report.
Legacy summaries missing aggregate keys are backfilled from `metrics.csv` when possible.

## Classification

`run_summary.json` always includes `status_reasons` (empty only for COMPLETE).

- **FAILED**: overflow, encode fail, snapshot starvation (≥5 starved samples after ramp), leak
- **WARN**: sustained **server** tick pressure (≥10 overruns or ≥3 consecutive samples with server p99 > 33.333 ms), admission refusals, unexpected disconnects, incomplete/unavailable server metrics
- **ABORTED**: Ctrl+C with summary
- **COMPLETE**: otherwise

Bot scheduler cadence above 33 ms is **not** a WARN by itself.
High population / rising bot_scheduler p95 is not automatic WARN/FAILED.

## Manual matrix (Clean network, impairment Off)

1. 1 → 2 bots (lifecycle)
2. 10 → 25 → 50 (ramp, 2–5 min); try 100 if the machine holds
3. Idle vs Walker vs Mixed at one stable N
4. Churn ~20% / 20 s
5. Burst connect
6. Soak 10–30 min at a stable N

## Phase 6D placement (Scenario A / B / C)

Server env `PURGATORY_LOAD_PLACEMENT` (read at `GameplayOwner` start):

| Value | Scenario | Intent |
|---|---|---|
| `cluster` (default) | A | High local density. Bandwidth should follow local interest, not global N. |
| `spread` | B | Same movement script / input profile / dirty-rate as A. Vary N and spatial spread only. Do not change mutation workload. |
| `maps` / `multimap` | C | Address isolation across Map A/B. |

Metrics schema **2** adds `aoi_enters`, `aoi_leaves`, `aoi_updates`, `aoi_churn_reentry`, `aoi_update_bytes`, `oldest_pending_ticks`, `max_deferred_ticks`, `replication_queue_depth_max`. Rates (enters/sec, bytes/update) are derived from counter deltas. Missed poll ≠ zeros.

Phase 6D Scenario A/B/C evidence (localhost, not capacity): [`docs/PHASE_6D_PERFORMANCE.md`](PHASE_6D_PERFORMANCE.md). Runner: `tools/run_phase6d_characterization.ps1`.

## Limitations

Localhost bots; shared CPU with the server; synthetic behavior; no native render
cost; load-mode admission 256 is a development bound, not production capacity.
Phase 6D replication follows local AOI rather than full-map N×N pose copies; still
not a player-capacity claim.
