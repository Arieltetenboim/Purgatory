# Phase 6G.7A — Exact incremental AOI invalidation

**Status:** implemented + validated — **stop for owner review**. No replication redesign. No Phase 7. No cap raise. 6G not closed.

Root `PHASE` = `6G.7A`. Design: [`docs/PHASE_6G7A_DESIGN.md`](PHASE_6G7A_DESIGN.md). Artifacts: `logs/load/capacity_6g7a/`.

## Model shipped

Movement invalidation:

1. **Influence prefilter** (unchanged extent `[28.5, 17.6]`) — correctness superset.
2. **Enter/leave XOR filter** — dirty observer `Obs` only if  
   `in_enter(old)≠in_enter(new)` **or** `in_leave(old)≠in_leave(new)` for `Obs`’s policy rects.
3. Always dirty the **moving subject** if it is a player (own view moved).

Presence path (spawn/despawn/leave/class/address): dirty prefiltered observers for whom `pos` is in enter **or** leave.

## Correctness proofs (`phase6g7a_tests`)

| Case | Result |
|---|---|
| Tiny same-cell, XOR=0 | **Only mover dirty** |
| X enter boundary | Observer dirtied when XOR ≥ 1 |
| Y-axis motion | Dirty-others == XOR count |
| Diagonal | Dirty-others == XOR count |
| Multi-cell large move | Far cluster clean |
| Separated clusters | Zero cross-talk |
| Dense cluster | Dirty-others == XOR |
| Spawn/despawn | Presence path |
| Address/channel hop | Subject dirty; presence both sides |

Replication tests updated: tiny remote move no longer forces reclassify (DomainRevs still drive Updates).

## Locality metrics (primary success)

Tiny-move unit case: leave/enter XOR = 0 → **≈0 movement-induced observer reclassification** (mover only).

Ladder (Release, seed 4242) — **dirtied per moved entity** vs 6G.6 influence-set:

| Scenario | 6G.6 influence per_move | 6G.7A dirtied per_move | xor per_move | prefilter per_move |
|---|---:|---:|---:|---:|
| distributed@64 | ~33 | **1.60** | 0.61 | 34.0 |
| distributed@128 | ~38 | **1.65** | 0.66 | 40.7 |
| distributed@256 | (high) | **1.80** | 0.81 | 49.5 |
| hotspot@64 | ~37 | **1.46** | 0.47 | 38.6 |
| hotspot@128 | ~46 | **1.53** | 0.53 | 50.3 |
| hotspot@256 | (high) | **1.51** | 0.51 | 51.6 |

AOI work now tracks **boundary XOR (~0.5–0.8)** plus the mover, not the full influence disk (~35–50).

## Before/after timings (domain p99 ms)

| Scenario | 6G.5 / 6G.6 era | 6G.7A tick / AOI / repl |
|---|---|---|
| distributed@64 | ~5–7 / ~3–4 / ~3 | **3.63 / 1.55 / 2.41** |
| distributed@128 | ~12–18 / ~6–10 / ~5–9 | **5.33 / 2.87 / 2.83** |
| distributed@256 | ~24–29 / ~16–19 / ~11 | **7.57 / 3.58 / 3.36** |
| hotspot@64 | ~7–11 / ~4–5 / ~4 | **13.62 / 3.78 / 11.55*** |
| hotspot@128 | ~16–19 / ~9 / ~11 | **15.99 / 4.07 / 14.94*** |
| hotspot@256 | ~30–50 / ~15–24 / ~10–22 | **11.61 / 4.04 / 5.14** |

\*Hotspot@64/128 show higher **replication** share this run (short window / mutual Known updates); **AOI** domain is still sharply down. Hotspot@256 AOI/tick improved materially vs 6G.5 (50/24 → 12/4).

Overruns: **0** on all six ladder cells this pass.

Machine-readable: `logs/load/capacity_6g7a/summary/ladder_summary.json`.

## Remaining AOI scaling

- Prefilter still O(influence players) leave-rect tests per move (~35–50 on FOOTNOTE) — cheap vs classify, but not free.
- Dense mutual visibility: XOR can still dirty many when entities cross many leave edges; correctness requires that.
- Mover always reclassifies (own view) — expected.
- **Replication relevance** (what/when) unchanged — still the bulk of 6G.6 recommendation D’s second half.

## Quality gate

- `phase6g7a` + updated `phase6g6` tiny-move + replication tests: **PASS**
- clippy `-D warnings` on common/simulation/server: **PASS**
- Full workspace `./scripts/check.ps1` recommended before merge

## Explicit non-goals honored

No entity→observer fan-out, relationship domains, density policy, priority packer, Phase 7, cap raise.

## Recommendation for 6G.7B

**Authorize dirty/relevance replication characterization→implementation** (6G.6 Part C/D/E/F):

1. Entity change → interested-observer fan-out (avoid walking all Known every tick when idle).
2. Domain/relationship policy hooks (same protocol).
3. Density/pressure-adaptive cadence/budget — still no second wire protocol.

6G.7A closes the **movement invalidation waste** proven in 6G.6; **do not close all of 6G** until 6G.7B (or owner accepts remaining replication gaps as post-6G).
