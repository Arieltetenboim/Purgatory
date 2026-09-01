# Phase 6G.5 — Incremental AOI invalidation

**Status:** implemented and measured — **stop for owner review**. Do **not** close all of 6G. No Phase 7. No cap raise. No replication redesign. No AOI semantics/protocol change.

Root `PHASE` = `6G.5`. Protocol **v10**. Design: [`docs/PHASE_6G5_DESIGN.md`](PHASE_6G5_DESIGN.md). Artifacts: `logs/load/capacity_6g5/20260901_161031/`.

## Implemented model

Replaced global `World::interest_generation` with:

1. **Per-observer dirty set** (`HashSet<EntityId>` of players).
2. **Spatial influence invalidation** on interest-affecting mutations: union of expand(old∪new poses, `AOI_INFLUENCE_HALF_EXTENTS`) on the **same** `WorldAddress`, marking players in that AABB (+ subject if player).
3. `publish_observer_frame` classifies only when never classified or dirty; clears dirty after classify.

`AOI_INFLUENCE_HALF_EXTENTS = [28.5, 17.6]` — FOOTNOTE-measured max leave extent from an observer (edge camera clamp). Guarded by `measure_max_leave_extent_from_observer` / `influence_covers_leave_inverse_on_footnote`.

Velocity-only `bump_transform_rev` still does **not** invalidate.

## Locality proof (required)

| Test | Result |
|---|---|
| `world::local_pose_change_dirties_nearby_players_only` | right-side move does **not** dirty left-side cluster |
| `replication::local_motion_does_not_reclassify_unrelated_observers` | reclassified observers ≤ 4 with 24 far observers; far set not reclassified |
| `replication::steady_interest_skips_reclassify_until_pose_changes` | skip until local dirty |

Unrelated map/channel/instance: invalidation is address-scoped → zero cross-address work.

## Motion ladder (comparable to 6G.4)

Release server + load, seed **4242**, ramp **50 ms**, durations 40/90/120 s for 64/128/256. Domain timings = last ~120-tick artifact window.

### Distributed (`spread` / walker)

| N | status | tick mean/p95/p99/max | AOI mean/p95/p99/max | repl mean/p95/p99/max | CPU% | WS peak MB | enters/leaves/updates | bytes out | starv | handoff | disc | ov |
|---:|---|---|---|---|---:|---:|---|---:|---:|---:|---:|---:|
| 64 | COMPLETE | 2.42 / 4.84 / **5.36** / 5.62 | 1.55 / 3.50 / 4.38 / 4.54 | 0.53 / 1.91 / 2.59 / 3.63 | ~0 | 15.2 | 4223 / 2072 / 194431 | 9.0M | 0 | 0 | 0 | 0 |
| 128 | COMPLETE | 8.20 / 14.10 / **17.66** / 19.49 | 5.10 / 8.06 / 10.18 / 10.88 | 2.29 / 4.83 / 9.29 / 14.39 | 32 | 18.2 | 20448 / 12018 / 1134602 | 45.5M | 0 | 0 | 0 | 1 |
| 256 | WARN | 19.52 / 25.93 / **29.44** / 35.10 | 12.04 / 15.00 / 18.91 / 21.25 | 5.72 / 8.22 / 11.54 / 11.67 | 94 | 26.8 | 59368 / 35381 / 3455890 | 119M | 0 | 0 | **0** | 18 |

Peak-of-window tick p99 (summary): 64 → 6.1; 128 → 20.6; 256 → 41.8. `peak_connected` @256 = **213** (no unexpected disconnects).

### Hotspot (`hotspot` / mixed)

| N | status | tick mean/p95/p99/max | AOI mean/p95/p99/max | repl mean/p95/p99/max | CPU% | WS peak MB | enters/leaves/updates | bytes out | starv | handoff | disc | ov |
|---:|---|---|---|---|---:|---:|---|---:|---:|---:|---:|---:|
| 64 | COMPLETE | 3.02 / 5.53 / **7.39** / 7.59 | 1.50 / 2.77 / 3.66 / 5.77 | 0.91 / 2.36 / 3.62 / 3.94 | 1.5 | 13.6 | 4770 / 2606 / 448739 | 15.7M | 0 | 0 | 0 | 0 |
| 128 | COMPLETE | 10.92 / 16.06 / **18.75** / 20.25 | 5.76 / 8.35 / 9.11 / 9.58 | 3.76 / 7.30 / 11.29 / 13.00 | 72 | 19.2 | 21633 / 13121 / 2841465 | 90.4M | 1 | 0 | 0 | 0 |
| 256 | WARN | 16.49 / 27.39 / **30.66** / 42.01 | 8.49 / 13.90 / **16.22** / 22.04 | 5.64 / 10.11 / 10.37 / 16.25 | 142 | 30.9 | 77753 / 47536 / 9739355 | 286M | 1 | 0 | 0 | **102** |

Peak-of-window tick p99: 64 → 7.5; 128 → 22.8; 256 → 42.8. `peak_connected` @256 = **236**.

Machine-readable: `logs/load/capacity_6g5/20260901_161031/motion_gate_summary.json`.

## vs 6G.4 (same ladder shape)

| Scenario | 6G.4 domain tick/AOI p99 | 6G.5 domain tick/AOI p99 | Notes |
|---|---|---|---|
| distributed@128 | 13.3 / 9.5 | 17.7 / 10.2 | No material AOI win; FOOTNOTE leave influence (~28.5 wu) vs 48 wu map still dirties large fractions under spread motion |
| distributed@256 | 20.2 / 10.6 (45 disc) | 29.4 / 18.9 (**0 disc**) | Timings noisier / higher connected; **disconnect regression cleared** |
| hotspot@128 | 17.6 / 9.3 | 18.8 / 9.1 | Flat (mutual visibility ⇒ large dirty sets are correct) |
| hotspot@256 | **50.4 / 24.2** | **30.7 / 16.2** | **Material AOI + tick improvement**; repl p99 also down (21.7 → 10.4) — cost not shifted into replication |

## Acceptance checklist

| # | Criterion | Result |
|---|---|---|
| 1 | Local movement no longer causes unnecessary world-wide classify | **Met** (unit + address scoping) |
| 2 | AOI correctness unchanged | **Met** (replication hysteresis tests + influence inverse coverage) |
| 3 | High-N motion AOI cost improves materially | **Met for hotspot@256**; limited on distributed FOOTNOTE |
| 4 | Work scales with affected regions/relationships | **Met** where groups are separated beyond influence; hotspot correctly remains dense |
| 5 | Cost not merely moved to replication | **Met** (repl timings not inflated as the AOI sink) |

## Quality gate

Commands run: `cargo fmt --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `cargo run -p purgatory-content-validator -q` → **PASS**.

## Explicit non-goals honored

- No Phase 7
- No pending/admission cap raise
- No replication redesign / protocol change
- No automatic close of all 6G

## Risks / follow-ups for owner

1. FOOTNOTE map width vs leave influence caps how much **distributed** motion can skip; larger maps would amplify locality wins.
2. Invalidation still does a spatial query per pose mutation (cheap vs full classify, but not free under all-movers).
3. Hotspot@256 still WARN (overruns) — improved but not “green idle-like”.
4. Optional next (owner-gated): tighter dirty (exact leave probe on influence candidates), or map-size-aware influence — **not** started here.

## Owner decision needed

Accept 6G.5 as closing the **global invalidation** architectural limit, keep 6G open for remaining capacity/product gates, or request a follow-up pass.
