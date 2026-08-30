# PHASE 6D PERFORMANCE EVIDENCE

## Status

**PHASE 6D PERFORMANCE EVIDENCE READY FOR REVIEW**

Automated correctness gate remains GREEN. Manual runtime check is still required.
Do **not** begin Phase 6E. This is localhost characterization, **not** a production-capacity claim.

## Method

- Date: 2026-08-29
- Machine: Windows 11; processor `Intel64 Family 6 Model 183 Stepping 1, GenuineIntel`
- Build: `cargo build --release -p purgatory-server -p purgatory-bot-client`
- Harness: `purgatory-load --scenario load --profile mixed --seed 4242 --duration 45s --ramp-ms 50`
- Server: `PURGATORY_ADMISSION_CAP=256` plus `PURGATORY_LOAD_PLACEMENT=cluster|spread|maps`
- Metrics: off-protocol `LoadMetricsV1` schema **2** polled at 1 Hz (`tools/poll_load_metrics.py`)
- Steady window: samples at ≥90% target sessions, skipping the first 8 s after that (Enter burst)
- `bytes_out` is length-prefixed gameplay uni payload (not QUIC/UDP on-wire size)
- Artifacts: `C:/Users/Ariel/OneDrive/Desktop/Purgatory/1/logs/load/phase6d_20260829_212546`

### Placement (implemented)

| Scenario | Env | Spawn |
|---|---|---|
| A cluster | `cluster` | all at `FOOTNOTE_SPAWN_X` (−19.4) Map A |
| B spread | `spread` | `FOOTNOTE_SPAWN_X + (n % 8) * 5` wu on Map A |
| C maps | `maps` | even index Map A spawn; odd index Map B |

Map A bounds are **48 wu** wide (`[-24, 24]`). AOI enter half-extents are **16 × 9 wu** (leave +2 wu).
Scenario B's eight spawn slots span only **40 wu**, so AOIs still overlap heavily.
Map B is **16 wu** wide, so one observer's leave rect covers essentially the whole map.
Scenario B therefore does **not** create a large empty-AOI remainder on this development stage.

### Metric notes / gaps

- **Known / relevant (exported):** `last_snapshot_entities` is the **max Known count across observers that tick**, not a mean of spatial candidates. Reported as `known_mean` / `known_max` over the steady window.
- **Spatial candidates / observer:** not a LoadMetrics gauge. On Map A, candidates ≈ players inside the leave rect + visible interactables (switch/chest in spawn AOI; portal is outside spawn leave rect). Cluster ≈ all N players + 2 generics. Spread still covers most of Map A.
- Writer queue depth / oldest pending / build time are **run peaks** (monotonic max), not window averages.
- `snapshot_build_time_max_ms` is wall time of `publish_observer_frame` (spatial candidates + classify + encode). Uni `write_all` encode-to-frame is `snapshot_encode_time_max_ms` (payload is already encoded).
- Enter/Update/Leave rates are counter deltas over the steady window (committed to the writer queue, not client ACK).
- `bytes/update` uses `Δaoi_update_bytes / Δaoi_updates` (encoded frame bytes / Update records — includes Enter/Leave bytes in the numerator).

## Scenario A — cluster (dense overlapping AOIs)

Worst case **at spawn**: every bot shares Map A `FOOTNOTE_SPAWN_X`. The `mixed` profile then walks, so density falls as bodies spread, but A still starts as the expensive overlap case and stays the highest Update and outbound rates at each N.

| N | connected | known mean/max | Enter/s | Update/s | Leave/s | churn/s | out MiB/s | KiB/s/client | B/update | build max ms | queue max | oldest pend | enc/send fail | unexp DC | status |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 10 | 10 | 12.6 / 13 | 4.60 | 2084.2 | 5.63 | 0.42 | 0.068 | 6.9 | 33.6 | 3.092 | 1 | 0 | 0/0 | 0 | COMPLETE |
| 25 | 25 | 27.3 / 28 | 25.45 | 12252.0 | 28.95 | 2.97 | 0.345 | 14.1 | 29.3 | 3.452 | 1 | 0 | 0/0 | 0 | COMPLETE |
| 50 | 50 | 51.3 / 53 | 106.37 | 48126.7 | 123.38 | 11.77 | 1.277 | 26.2 | 27.7 | 4.145 | 1 | 0 | 0/0 | 0 | COMPLETE |

Tick / encode (steady-end server gauges; build/encode max are run peaks):

| N | tick mean ms | tick p95 | tick p99 | tick max | overruns | encode max ms | frame size max |
|---|---:|---:|---:|---:|---:|---:|---:|
| 10 | 0.095 | 0.379 | 0.464 | 0.717 | 0 | 0.014 | 376 |
| 25 | 0.324 | 1.228 | 1.840 | 2.202 | 0 | 0.081 | 781 |
| 50 | 0.952 | 4.230 | 5.669 | 7.274 | 0 | 0.040 | 1456 |

## Scenario B — spread (same mixed profile / seed / mutation workload)

Same `mixed` input profile and seed as A. Only spawn X changes. On this 48 wu map the eight 5 wu slots still overlap the 16 wu enter rect, so the relevant set is only weakly reduced versus A.

| N | connected | known mean/max | Enter/s | Update/s | Leave/s | churn/s | out MiB/s | KiB/s/client | B/update | build max ms | queue max | oldest pend | enc/send fail | unexp DC | status |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 10 | 10 | 11.2 / 13 | 5.33 | 1516.0 | 5.51 | 0.31 | 0.054 | 5.5 | 36.4 | 2.429 | 1 | 0 | 0/0 | 0 | COMPLETE |
| 25 | 25 | 24.8 / 27 | 34.00 | 9513.1 | 35.43 | 2.77 | 0.277 | 11.4 | 30.2 | 2.763 | 1 | 0 | 0/0 | 0 | COMPLETE |
| 50 | 50 | 43.6 / 47 | 126.61 | 35801.6 | 132.69 | 10.71 | 0.972 | 19.9 | 28.3 | 4.120 | 1 | 0 | 0/0 | 0 | COMPLETE |

Tick / encode (steady-end server gauges; build/encode max are run peaks):

| N | tick mean ms | tick p95 | tick p99 | tick max | overruns | encode max ms | frame size max |
|---|---:|---:|---:|---:|---:|---:|---:|
| 10 | 0.099 | 0.358 | 0.694 | 0.805 | 0 | 0.004 | 321 |
| 25 | 0.277 | 1.063 | 1.334 | 1.377 | 0 | 0.000 | 677 |
| 50 | 0.947 | 3.791 | 5.827 | 6.949 | 0 | 0.202 | 1249 |

## Scenario C — maps (WorldAddress split)

Odd attachments spawn on Map B. Address isolation should cut cross-map replication. Map B is fully covered by one AOI; Map A half still clusters at spawn.

| N | connected | known mean/max | Enter/s | Update/s | Leave/s | churn/s | out MiB/s | KiB/s/client | B/update | build max ms | queue max | oldest pend | enc/send fail | unexp DC | status |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 10 | 10 | 7.9 / 8 | 0.86 | 1230.9 | 0.86 | 0.11 | 0.047 | 4.8 | 38.7 | 2.305 | 1 | 0 | 0/0 | 0 | COMPLETE |
| 25 | 25 | 15.8 / 16 | 6.93 | 7093.6 | 7.59 | 0.40 | 0.216 | 8.9 | 31.5 | 2.395 | 1 | 0 | 0/0 | 0 | COMPLETE |
| 50 | 50 | 27.1 / 28 | 29.44 | 26032.9 | 34.47 | 2.27 | 0.727 | 14.9 | 29.0 | 4.111 | 1 | 0 | 0/0 | 0 | COMPLETE |

Tick / encode (steady-end server gauges; build/encode max are run peaks):

| N | tick mean ms | tick p95 | tick p99 | tick max | overruns | encode max ms | frame size max |
|---|---:|---:|---:|---:|---:|---:|---:|
| 10 | 0.102 | 0.312 | 0.596 | 1.634 | 0 | 0.062 | 241 |
| 25 | 0.263 | 0.989 | 1.776 | 1.878 | 0 | 0.035 | 457 |
| 50 | 0.621 | 2.715 | 3.289 | 4.288 | 0 | 0.169 | 781 |

## Phase 6C full-visibility baseline vs Phase 6D Scenario B

6C (and 5.7) sent a **full `WorldSnapshot` every tick** to every observer: all same-map players plus map interactables. That payload is reconstructed here from the v7 encoder (`4` byte length prefix + `48` byte header + `25` bytes/entity) at 30 Hz. Surviving 5.7 reports recorded tick work, not `bytes_out`, and this tree can no longer run v7.

6C Map A entity count = N players + 3 interactables. 6D B numbers are **measured** application `bytes_out` in the steady window.

| N | 6C recon MiB/s | 6C recon KiB/s/client | 6D B MiB/s | 6D B KiB/s/client | 6D B known mean | ratio 6D/6C out |
|---:|---:|---:|---:|---:|---:|---:|
| 10 | 0.108 | 11.0 | 0.054 | 5.5 | 11.2 | 0.499 |
| 25 | 0.538 | 22.0 | 0.277 | 11.4 | 24.8 | 0.515 |
| 50 | 1.970 | 40.3 | 0.972 | 19.9 | 43.6 | 0.494 |

6C Scenario C reconstruction (half Map A / half Map B, still full snapshots per address):

| N | 6C-C recon MiB/s | 6D C MiB/s | 6D C KiB/s/client |
|---:|---:|---:|---:|
| 10 | 0.069 | 0.047 | 4.8 |
| 25 | 0.306 | 0.216 | 8.9 |
| 50 | 1.058 | 0.727 | 14.9 |

## Does evidence support bounded per-client bandwidth?

Claim under test:

`global N increases while local relevant set stays approximately bounded → per-client replication bandwidth stays approximately bounded`

**Scenario A (cluster, expected expensive):**
- N=10: KiB/s/client=6.9, known_mean=12.6
- N=25: KiB/s/client=14.1, known_mean=27.3
- N=50: KiB/s/client=26.2, known_mean=51.3
- From N=10 to N=50: per-client KiB/s ×3.76; known_mean ×4.06; N ×5.00

**Scenario B (spread, intended bounded-local test):**
- N=10: KiB/s/client=5.5, known_mean=11.2
- N=25: KiB/s/client=11.4, known_mean=24.8
- N=50: KiB/s/client=19.9, known_mean=43.6
- From N=10 to N=50: per-client KiB/s ×3.62; known_mean ×3.91; N ×5.00

**Scenario C (maps):**
- N=10: KiB/s/client=4.8, known_mean=7.9
- N=25: KiB/s/client=8.9, known_mean=15.8
- N=50: KiB/s/client=14.9, known_mean=27.1
- From N=10 to N=50: per-client KiB/s ×3.12; known_mean ×3.41; N ×5.00

**Not supported on this development stage (Scenario B):** known_mean grew ×3.91 vs N ×5.00. The FOOTNOTE map is only 48 wu wide with a 16 wu enter half-extent, so spreading across 8 slots does not keep the relevant set approximately constant. Per-client bandwidth therefore still tracks local N, which still tracks global N.

**Scenario A is not hidden:** clustered spawn keeps essentially all players inside one leave rect. Known count and Update rate are expected to scale with N. That is the expensive overlap case.

**6D vs 6C still matters even when AOI does not bound:** v8 sends Transform Updates for dirty entities instead of a full pose list every tick, so idle/un-dirty members of the known set are omitted. Compare the 6C reconstruction table above. That is a delta-encoding win, not proof that AOI bounded the set.

## AOI churn

`aoi_churn_reentry` counts re-Enter of an id recently Left (observer known-set hysteresis / walk-out-walk-in). Rates below are steady-window `Δchurn / Δt`.

| Scenario | N | churn/s | Enter/s | Leave/s |
|---|---:|---:|---:|---:|
| A_cluster | 10 | 0.42 | 4.60 | 5.63 |
| A_cluster | 25 | 2.97 | 25.45 | 28.95 |
| A_cluster | 50 | 11.77 | 106.37 | 123.38 |
| B_spread | 10 | 0.31 | 5.33 | 5.51 |
| B_spread | 25 | 2.77 | 34.00 | 35.43 |
| B_spread | 50 | 10.71 | 126.61 | 132.69 |
| C_maps | 10 | 0.11 | 0.86 | 0.86 |
| C_maps | 25 | 0.40 | 6.93 | 7.59 |
| C_maps | 50 | 2.27 | 29.44 | 34.47 |

Placement only sets **spawn X**. The `mixed` profile then walks, so Scenario A does not remain a static pile. Leave/Enter/churn therefore appear in cluster as well as spread: bots walk out of each other's 16 wu enter / 18 wu leave rects on the 48 wu floor. Maps (C) show lower churn because each address has about N/2 bodies and Map B is fully covered by one AOI.

## Failures

Encode failures, uni `write_all` failures, and unexpected disconnects are copied from schema-2 gauges and the harness `run_summary.json`. Non-zero encode/write failures would be a defect; this report does not change gameplay code unless those counters fire.

## Limitations

- Localhost bots share CPU with the server; no native render cost.
- Admission 256 is a development bound.
- 45 s holds are short; not a soak.
- 6C MiB/s is reconstructed from the v7 encoder, not a paired v7 re-run on this binary.
- Do not treat these numbers as MMO player capacity.

## Boundary

Work stopped at Phase 6D evidence. **Do not begin Phase 6E.**

**PHASE 6D PERFORMANCE EVIDENCE READY FOR REVIEW**
