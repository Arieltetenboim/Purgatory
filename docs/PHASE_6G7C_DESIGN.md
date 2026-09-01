# 6G.7C Design — Relationship / density / budget replication policy foundation

## Problem (after 6G.7B)

Discovery is dirty→interested. Dense mutual visibility still correctly fans to many observers.
Remaining question: **what** each observer needs, at what **priority/cadence**, under **density/budget** pressure — same protocol.

## Pipeline

```text
authoritative domain change
  → dirty entity/domain (6G.7B)
  → interested observers (AOI Known / fan-out)
  → observer-specific policy (relation × domain × pressure)
  → priority sort + cadence + per-observer budget
  → existing encode / queue / v10 frames
```

## Four decisions (independent)

| # | Decision | Owner |
|---|---|---|
| 1 | Entity relevance | AOI Enter/Leave/Known (unchanged) |
| 2 | Domain relevance | `domain_eligibility` via relation + pressure |
| 3 | Priority | `ReplicationPriority` for packer ordering under budget |
| 4 | Cadence/detail | interval / stagger; state may coalesce |

## Relationship kinds (extensible)

`Self`, `Party`, `Target`, `NearbyStranger`, `DistantStranger` (+ stubs for future NPC/hostile).
Party/Target resolved only via optional proof overrides — **no full gameplay systems**.

## State vs event

| Class | Examples | Under pressure |
|---|---|---|
| StateLike | transform, health | coalesce, cadence, silent catch-up if ineligible |
| EventLike / lifecycle | Enter, Leave, epoch baseline | Prefer first in frame; never silently drop Leave/Enter |

Unchanged domain → no ordinary delta (preserved).

## Density inputs

- **Content hint:** `PopulationClass` { Low, Medium, High, Extreme } (env/map stub).
- **Runtime pressure:** interested fan-out size, observer Known count, recent bytes, tick overrun hint.

Neither alone dictates policy; both feed `PressureLevel`.

## Modes (same wire protocol)

- `baseline` — 6G.7B behavior (visible ⇒ transform+health).
- `selective` — proof policy: strangers transform-only; distant lower cadence; priority packing; pressure may tighten observer byte budget.

`PURGATORY_REPLICATION_POLICY=baseline|selective` (default selective for characterization; tests pin explicitly).

## Non-goals

Full party/target/combat policy, final production thresholds/bandwidth numbers, second protocol, AOI changes, Phase 7, admission cap raise.
