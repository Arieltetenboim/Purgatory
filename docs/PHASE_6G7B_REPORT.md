# Phase 6G.7B — Dirty-driven replication fan-out foundation

**Status:** implemented + measured — **stop for owner review**. No AOI changes. No Phase 7. No cap raise. 6G not closed.

Root `PHASE` = `6G.7B`. Design: [`docs/PHASE_6G7B_DESIGN.md`](PHASE_6G7B_DESIGN.md). Artifacts: `logs/load/capacity_6g7b/`.

## 1. Current replication scaling law (pre-6G.7B)

Per observer, every tick after AOI classify:

1. Iterate observers (GameplayOwner bindings).
2. Leave then Enter from Life state (baseline / membership) — unchanged.
3. **Update discovery:** walk **all** `Life::Known`, compare `DomainRevs` vs `CommittedRevs`.
4. Cadence gate → encode `update_record` → byte budget → writer queue.
5. Commit only if queue accepts.

**Discovery law:**

```text
Known relationships scanned / tick ≈ Σ_observers |Known_o|
```

even when only one entity changed. Payload spam was already limited by `DomainRevs`; **traversal** was not.

## 2. Design chosen

```text
authoritative domain bump
  → World replication dirty (transform/health mask)
  → InterestFanoutIndex (entity → Known observers)
  → observer pending set + domain_eligibility_for hook
  → existing cadence / packer / budget / queue
```

Preserved: Enter baseline, Leave, epoch/resync, staggered recovery Known reconcile (~2s/observer), DomainRevs correctness, protocol v10.

Non-goals honored: no party/target rules, no density thresholds, no priority packer, no AOI edits.

## 3. Before/after traversal ratios

Unit proof (`one_dirty_among_many_known_scans_only_pending`): idle Known crowd → **scanned=0**; one remote move → scanned ≪ |Known|.

Ladder (`replication_fanout.json`, seed 4242) — primary ratios:

| Scenario | known_present | known_scanned | scanned/update | present/update (old-law proxy) | scanned/dirty |
|---|---:|---:|---:|---:|---:|
| idle@64 | 904k | **0** | — | — | — |
| idle@128 | 970k | **0** | — | — | — |
| idle@256 | 896k | **0** | — | — | — |
| distributed@64 | 1.17M | 227k | **1.37** | 7.1 | 27.6 |
| distributed@128 | 1.30M | 259k | **1.38** | 6.9 | 29.2 |
| distributed@256 | 1.45M | 299k | **1.36** | 6.6 | 31.2 |
| hotspot@64 | 1.47M | 463k | **1.26** | 4.0 | 33.6 |
| hotspot@128 | 1.68M | 518k | **1.26** | 4.1 | 35.4 |
| hotspot@256 | 1.62M | 489k | **1.26** | 4.2 | 34.9 |

Interpretation:

- **Idle:** discovery work → 0 while Known relationships still exist (old law would scan all present).
- **Motion:** scanned/update ≈ **1.3** (near changed→emit); present/update remains the old O(Known) tax that is no longer paid on the hot path.
- **scanned/dirty ≈ interested fan-out** under mutual visibility (dense hotspot correctly fans widely).

## 4. Before/after timings (domain p99 ms)

| Scenario | 6G.7A tick / AOI / repl | 6G.7B tick / AOI / repl |
|---|---|---|
| distributed@64 | 3.63 / 1.55 / 2.41 | **1.89 / 0.79 / 0.66** |
| distributed@128 | 5.33 / 2.87 / 2.83 | **3.35 / 1.74 / 0.99** |
| distributed@256 | 7.57 / 3.58 / 3.36 | **4.29 / 2.05 / 1.65** |
| hotspot@64 | 13.62 / 3.78 / 11.55* | **4.46 / 1.59 / 1.50** |
| hotspot@128 | 15.99 / 4.07 / 14.94* | **6.07 / 2.64 / 2.44** |
| hotspot@256 | 11.61 / 4.04 / 5.14 | **4.57 / 2.02 / 1.77** |
| idle@256 | (prior idle-era ~few ms) | **2.38 / 1.64 / 0.82** |

\*6G.7A hotspot@64/128 had elevated repl share that run; 6G.7B still cuts repl sharply.

Overruns: **0** on all nine ladder cells. Budget deferred: **0**. Recovery rescues: **0** on these windows.

Machine-readable: `logs/load/capacity_6g7b/summary_20260901_173203/ladder_summary.json`.

## 5. Correctness evidence

| Case | Result |
|---|---|
| Existing replication suite (12) | PASS |
| `one_dirty_among_many_known_scans_only_pending` | scanned ≪ Known; idle scanned=0 |
| `domain_eligibility_hook_can_suppress_health` | hook present |
| Enter before Update / epoch / leave-reenter | unchanged tests PASS |
| Multi-observer commit isolation | shared fan-out index PASS |

Clippy `-D warnings` on common/simulation/server: **PASS**.

## 6. Remaining cost under dense mutual visibility

Hotspot still emits ~many Updates when many entities move in mutual Known sets — **correct**. Fan-out makes discovery ≈ dirty×interested, not observers×Known, but **bytes and encode** remain O(emits). scanned/dirty ≈ 35 at hotspot means dense interest graphs, not wasted full Known walks.

Header frames still built every tick (unchanged). Cadence/budget are coarse; no priority packer yet.

## 7. Recommendation for next policy/budget pass

Authorize a **6G.7C** (or named follow-up) for:

1. Relationship domain policy (self/party/target/stranger) on the existing eligibility hook.
2. Density/pressure-adaptive cadence + byte budget (same protocol).
3. Optional priority packer under bandwidth/tick pressure.

Do **not** close all of 6G until that policy pass is measured or explicitly deferred by owner.

## Quality gate

- Targeted: replication tests + clippy on touched crates — **PASS**
- Full `./scripts/check.ps1` recommended before merge
- Ladder Release evidence — **PASS** (above)

## Explicit non-goals honored

No AOI changes, no party/density system, no Phase 7, no cap raise, 6G remains open.
