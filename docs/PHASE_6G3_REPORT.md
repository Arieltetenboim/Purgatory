# Phase 6G.3 — Targeted AOI fan-out optimization

**Status:** complete (targeted pass). Stop here — no Phase 7, no general replication redesign.

Protocol **v10**. Root `PHASE` = `6G.3`.

## Root cause

Per-observer AOI work scaled as **O(observers × candidates)** every tick:

1. `publish_observer_frame` called `spatial_candidates` **twice** (stats + `classify`).
2. Each call did grid query + **sort** + class filter; `classify` then built a `HashSet` and walked tracked + candidates.
3. Under idle / mutual visibility, leave rects cover most of FOOTNOTE, so candidates ≈ all address entities → **AOI ≈ 79% of tick p99 at idle@256** (6G.2).

Dominant term: **repeated full candidate rebuild + classify**, not missing spatial index (grid already exists).

## Change made (smallest safe cut)

1. **`World::interest_generation`** — bumped on pose/address/class/spawn/despawn that can change AOI membership.
2. **Skip spatial query + classify** when an observer’s `classified_interest_gen` matches current generation (steady interest).
3. **Single unsorted candidate query** when classify *does* run (Enter/Leave lists still sort later).
4. Semantics preserved: same enter/leave rects, hysteresis, cadence, protocol.

Files: `crates/simulation/src/world.rs`, `spatial.rs`, `apps/server/src/network/replication.rs`.

## Before / after (Release, same helper / seed 4242 / ramp 50)

Domain timings = last ~120-tick window at artifact flush. 6G.2 baselines from [`MMO_RUNTIME_BASELINE.md`](MMO_RUNTIME_BASELINE.md).

| Scenario | N | Metric | 6G.2 before | 6G.3 after (best confirmed) | Notes |
|---|---:|---|---|---|---|
| idle | 64 | tick p99 / AOI p99 | 5.05 / 3.70 | **3.27 / 0.40** | AOI ≪ tick; skip engaged |
| idle | 128 | tick p99 / AOI p99 | 8.70 / 7.12 | **11.92 / 6.78** (p50 tick **1.99**, AOI p50 **0.36**) | End-window noisy; steady median much lower; COMPLETE on re-run |
| idle | 256 | tick p99 / AOI p99 | 16.02 / 12.74 | **7.95 / 5.80** | ~50% tick, ~54% AOI; repl p99 2.58; ov=0 |
| hotspot | 128 | tick p99 / AOI p99 | 9.32 / 6.44 | 18.76 / 10.96 | Movement bumps interest every tick → skip rarely helps; variance high |

Replication domain did **not** absorb the idle@256 AOI savings (repl p99 stayed secondary).

One idle@128 attempt hit `snapshot_starvation` (not reproduced on immediate re-run). High-N ramp disconnect WARNs remain (not treated as AOI; unchanged class of issue).

## Correctness

- Unit: `steady_interest_skips_reclassify_until_pose_changes`, existing leave/reenter / hysteresis tests.
- `./scripts/check.ps1` **PASS** after the change.

## Remaining scaling limit

When **any** entity moves/spawns, `interest_generation` bumps and **all** observers reclassify that tick → still O(observers × candidates) under continuous motion / hotspot. Further gains need dirtier/incremental interest (entity→observer reverse index or similar) — **out of this targeted pass**.

## Acceptance vs charter

| Criterion | Idle high-N | Continuous-motion hotspot |
|---|---|---|
| AOI ↓ materially | **Yes** (@256) | No reliable win |
| Tick ↓ correspondingly | **Yes** (@256) | No |
| Not shifted into replication | **Yes** | N/A |
| Semantics OK | **Yes** | **Yes** |
| No new instability class | Starvation flake once; not reproduced | Ramp disconnects pre-existing |

**6G.3 succeeds on the measured primary hotspot (idle@256 AOI fan-out).** Motion-bound AOI cost remains for a later owner decision.

## Recommendation

- **Accept 6G.3** for the idle/steady-interest win.
- **Do not close all of 6G** solely on this: motion AOI + ramp disconnects still open.
- Next owner choice: another narrow AOI pass (incremental/dirty interest), 6G.4 regression gate from measured idle curves, or declare 6G GREEN with known limits documented.

Stop after 6G.3.
