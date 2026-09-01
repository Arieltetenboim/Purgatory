# 6G.7A Design — Exact incremental AOI movement invalidation

## Problem (from 6G.6)

Full-influence invalidation marks every player inside expand(old∪new, `AOI_INFLUENCE_HALF_EXTENTS`).
A tiny same-cell move can dirty dozens of observers while **enter/leave XOR is empty**.

## Model

Keep `AOI_INFLUENCE_HALF_EXTENTS` as a **correctness prefilter** (superset of observers who could possibly care). Do not shrink it.

### Motion (`old → new` on one `WorldAddress`)

1. Query players in expand(old∪new, influence halves).
2. Always dirty `subject` if it is a player (observer’s own view moved).
3. For each other prefiltered player `Obs`, compute `aoi_policy_rects(Obs)` and dirty iff:

   `in_enter(old) ≠ in_enter(new)` **OR** `in_leave(old) ≠ in_leave(new)`

   (enter and leave both matter for WantEnter / Known / WantLeave).

Observers deep inside both rects, or outside both, are skipped.

### Presence (spawn / despawn / leave / class / address hop)

Treat as appear or disappear at `pos`: dirty prefiltered players for whom `pos` lies in enter **or** leave (relationship can begin or end). Map/address change: presence-invalidate at old address and new address.

## Correctness

Influence prefilter ⊇ leave/enter XOR set (proven by FOOTNOTE max leave extent ≤ influence halves). XOR filter only removes observers that cannot change membership for this pose pair.

## Non-goals (6G.7B+)

Entity→observer replication fan-out, relationship domains, density/priority packers.
