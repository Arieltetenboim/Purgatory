# 6G.7B Design — Dirty-driven replication fan-out foundation

## Problem (from 6G.6 / 6G.7A)

`DomainRevs` already prevent unchanged transform/health **payload** spam. Discovery still walks **every Known** entity for **every observer** each tick:

```text
discovery ≈ Σ_observers |Known_o|   (even when only one entity changed)
```

Target for the sparse-dirty case:

```text
discovery ≈ changed entities → interested observers
```

## Current loop (pre-6G.7B)

Per observer, per tick (`publish_observer_frame`):

1. **AOI classify** if dirty (unchanged by this pass).
2. **Leave** then **Enter** from Life state (baseline / membership) — budgeted encode.
3. **Update discovery:** iterate **all** `Life::Known`, compare `World::domain_revs` vs `CommittedRevs`.
4. **Cadence** gate pending updates (`High` / staggered Normal/Low).
5. **Serialize** `update_record` (domains with rev lag only) and apply **byte budget**.
6. **Commit** only if writer queue accepts the frame.

## Chosen model

```text
authoritative domain bump
  → World replication dirty (per entity, per domain mask)
  → InterestFanoutIndex (entity → observers with Known)
  → observer pending set + domain eligibility hook
  → existing cadence / packer / budget / queue
```

### Preserved separate paths

| Path | Mechanism |
|---|---|
| Enter / baseline | `WantEnter` → full Enter snapshot; commits `CommittedRevs` |
| Leave / despawn | `WantLeave` → Leave record; drops fan-out edge |
| Epoch / reconnect | `bump_epoch` clears Life + fan-out edges for observer |
| Recovery | Staggered full Known rev-reconcile (safety net; not the hot path) |

### Domain granularity

Dirty mask today: **transform**, **health** (wire domains). Unchanged domain does not create emit work. Mechanism is ready for more domains later without inventing gameplay content policy.

### Observer relevance hook

`ObserverRelationKind` + `domain_eligibility_for` — today Self/VisibleOther both allow transform+health. Ineligible lagged domains **catch up commit without emit** (visibility ≠ entitlement).

### Density / adaptive (non-goal this pass)

Cadence, budget, and eligibility remain pluggable call sites. No second protocol; no population-class packer yet.

## Non-goals

Party/target rules, density thresholds, priority packer, dynamic per-client bandwidth controller, AOI changes, Phase 7, cap raise, closing all of 6G.

## Metrics

Capacity artifact `replication_fanout.json`: Known present vs scanned, dirty entities/domains, interested observers, emits, serialize attempts, budget deferrals, recovery rescues.
