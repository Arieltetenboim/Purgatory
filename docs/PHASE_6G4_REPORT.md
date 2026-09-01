# Phase 6G.4 — Motion / capacity gate (measure only)

**Status:** recorded — **stop for owner review**. No incremental/dirty AOI. No Phase 7. No cap raise. No replication redesign.

Root `PHASE` = `6G.4`. Protocol **v10**. Artifacts: `logs/load/capacity_6g4/20260901_140001/`.

## Classification

### **A. GREEN WITH KNOWN LIMIT**

| Rule | Evidence |
|---|---|
| 128 motion has comfortable tick headroom | distributed@128 domain tick p99 **13.3 ms**; hotspot@128 domain tick p99 **17.6 ms** (summary peak-of-window p99 27.5 / max 34.5 = single-sample spike, not sustained failure). Both **COMPLETE**, disc=0, starv=0, handoff=0, admission=0. |
| No meaningful instability at 128 | No unexpected disconnects, no snapshot starvation streak, no input handoff drops. |
| 256 may degrade as characterized higher-N limit | distributed@256: domain tick still OK (p99 20.2) but **WARN** with **45** ramp disconnects (peak_connected 178). hotspot@256: AOI/tick saturation (domain p99 **50.4**, 18 overruns, starv=1, peak_connected 208). |

Not **B**: 128 motion is not AOI-saturated (headroom to 33.33 ms remains).  
Not **C** as the primary gate label: disconnects appear mainly at **256 distributed** while timings stay healthy there, but 128 is clean and hotspot@256 is AOI/tick-limited — overall gate matches **A**.

## Method

Release `purgatory-server` + `purgatory-load`, seed **4242**, `--ramp-ms 50`, durations 40s/90s/120s for 64/128/256. Scenarios: `distributed` (`PURGATORY_LOAD_PLACEMENT=spread`, walker) and `hotspot` (`hotspot`, mixed). Helper: `scripts/capacity_ladder.ps1`.

Domain timings = last ~120-tick artifact window. Summary p99/max = peak-of-window across the run (can exceed domain end-window).

## Measurements

### Distributed movement

| N | status | tick mean/p95/p99/max | AOI mean/p95/p99/max | repl mean/p95/p99/max | CPU% | WS peak MB | enters/leaves/updates | bytes out | starv | handoff | disc | adm | ov |
|---:|---|---|---|---|---:|---:|---|---:|---:|---:|---:|---:|---:|
| 64 | COMPLETE | 2.27 / 5.34 / **6.16** / 6.40 | 1.45 / 3.13 / 4.02 / 5.26 | 0.54 / 2.32 / 4.16 / 4.46 | 24 | 15.0 | 4227 / 2073 / 191927 | 8.9M | 0 | 0 | 0 | 0 | 0 |
| 128 | COMPLETE | 7.21 / 11.01 / **13.27** / 16.24 | 4.65 / 6.23 / 9.47 / 13.79 | 2.04 / 3.79 / 6.32 / 6.60 | 99 | 18.5 | 19578 / 11166 / 1051243 | 43.1M | 0 | 0 | 0 | 0 | 0 |
| 256 | WARN | 9.97 / 13.32 / **20.19** / 20.85 | 6.08 / 8.61 / 10.61 / 10.97 | 3.26 / 4.91 / 9.23 / … | 87 | 22.0 | 41560 / 27379 / 2272188 | 85.1M | 0 | 0 | **45** | 0 | 0 |

Summary peak-of-window tick: 64 → p99 6.2; 128 → 13.9; 256 → 27.2 (peak_connected **178**).

### Hotspot / mutual-visibility movement

| N | status | tick mean/p95/p99/max | AOI mean/p95/p99/max | repl mean/p95/p99/max | CPU% | WS peak MB | enters/leaves/updates | bytes out | starv | handoff | disc | adm | ov |
|---:|---|---|---|---|---:|---:|---|---:|---:|---:|---:|---:|---:|
| 64 | COMPLETE | … / 4.32 / **6.05** / 6.54 | … / 3.15 / 4.27 / … | … / … / 2.94 / … | 17 | 13.7 | 4789 / 2602 / 441022 | 15.5M | 0 | 0 | 0 | 0 | 0 |
| 128 | COMPLETE | … / 16.73 / **17.64** / 19.2 | … / 8.37 / 9.28 / … | … / … / 9.28 / … | 83 | 19.9 | 24100 / 15413 / 3172633 | 99.0M | 0 | 0 | 0 | 0 | 1 |
| 256 | WARN | … / 27.57 / **50.36** / 60.26 | … / 16.58 / **24.18** / … | … / … / 21.70 / … | 168 | 29.0 | 56389 / 32489 / 6910875 | 208M | **1** | 0 | 0 | 0 | **18** |

Summary peak-of-window tick: 64 → p99 12.0; 128 → 27.5 / max 34.5; 256 → **50.4 / 60.3** (peak_connected **208**).

Machine-readable: `logs/load/capacity_6g4/20260901_140001/motion_gate_summary.json`.

## Interpretation

1. **Post-6G.3 idle win stands**; this gate is about **motion**.
2. At **128**, AOI remains a large share of tick under motion, but sustained p99 stays well under 33.33 ms with operational stability → not an automatic AOI redesign trigger.
3. At **256 hotspot**, continuous movement + mutual visibility saturates AOI/tick (known `interest_generation` fan-out limit).
4. At **256 distributed**, tick timings stay healthier while **ramp disconnects** dominate WARN — a separate operational/session limit if pursued later (**C**-shaped), not a reason to rewrite AOI from this gate alone.

## Explicit non-goals honored

- No incremental/dirty AOI implementation
- No replication redesign
- No pending/admission cap raise
- No Phase 7

## Owner decision needed

Accept **A (GREEN WITH KNOWN LIMIT)** for motion capacity, or escalate a follow-up (dirty-interest AOI and/or 256 ramp-disconnect investigation) as a separate campaign.
