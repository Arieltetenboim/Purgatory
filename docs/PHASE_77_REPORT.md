# Phase 7.7 — Scaling Architecture Decision & Readiness

**Status:** complete  
**Date:** 2026-09-02  
**Root `PHASE`:** `7.7`  
**Predecessor:** [`PHASE_76_REPORT.md`](PHASE_76_REPORT.md)  
**ADR:** [ADR-0056](DECISIONS.md#adr-0056-single-process-authoritative-world-remains-canonical-until-measured-triggers)  

## Verdict

**Remain single-process / single `World` owner. No scaling implementation is justified.**

Evidence from 7.3–7.6 shows comfortable tick headroom, no demonstrated CPU/transport/memory/simulation saturation inside the tested envelope, and a projected first limiter of `replication_policy` — not a requirement to ship parallelism, sharding, zone servers, or multi-process worlds now.

Phase 7.7 is an architecture decision and readiness inventory. **No multithreading, parallel replication, sharding, channels product, or distributed simulation was implemented.**

---

## 7.7A — Architecture checkpoint (reconfirm)

### Measured results that matter

| Fact | Source |
|---|---|
| Peak util ≲ **11%** of tick spacing through mixed@128 / dense@64 | 7.5 / 7.6 |
| No CPU, transport, memory, or simulation saturation demonstrated in tested envelope | 7.3–7.6 |
| Dominant growth owner under player/density growth: **`replication_policy`** | 7.5 Instant close; 7.6 sensitivity |
| Parallelism **not** justified | 7.5 / 7.6 |
| Capacity model directional only (~20–22% holdout MAPE; cooler mixed overpredicts) | 7.6 |
| Far util≈50–80% extrapolations are **unsupported** capacity claims | 7.6 |

### Does measured evidence justify any of these now?

| Mechanism | Justified now? |
|---|---|
| Intra-tick simulation parallelism | **No** |
| Parallel replication | **No** |
| Multiple simulation workers | **No** |
| Multiple world processes | **No** |
| Zone ownership / spatial split | **No** |
| Production channels/instances as scaling | **No** (product may need them later for other reasons) |
| Cross-process player handoff | **No** |
| Horizontal multi-process scaling | **No** |

Future projections and product wishes are **not** measured saturation.

### Limit classes (kept separate)

| Class | Statement |
|---|---|
| **Measured limit** | Inside Phase 7 ladders (through mixed@128 / dense@64 and related cells), **no resource class saturates** the 30 Hz tick budget. Envelope is comfortably under budget. |
| **Projected likely limit** | As player count / observer overlap grows, **`replication_policy` (scan volume)** is the first projected CPU owner to become material — still far from tick saturation in the measured band. NPC-heavy shapes can flip dominance to `npc_activity` without approaching saturation in the tested range. |
| **Unknown / unmeasured** | Production WAN transport; multi-region; true production channel allocation load; handoff under failure; process crash recovery for multi-owner; memory at scales far beyond ladder WS peaks; harness-limited high-N connect cliffs vs server tick cliffs. |

---

## 7.7B — Scaling trigger contract

Reopen scaling architecture only when **measurable** Phase 7 telemetry (or an explicit product requirement) meets a trigger class below. Do **not** invent a final production player cap or a hard util % here; numeric operating envelopes are outputs of **7.8** and living budgets, not of 7.7.

All performance triggers require:

1. **Representative workload** (canonical MixedRuntime / dense / documented ladder shape — not idle-only).
2. **Reproducibility** across repeated runs or ladder cells.
3. **Healthy harness** when blaming the server (issuance funnel, disconnect class attribution).
4. **Dominant owner known** from Instant / tick-domain artifacts.
5. **Local optimization exhausted or insufficient** for that owner (policy/content locality, scan reduction, accounting-proven hotspots) before redesign.

### CPU / tick trigger

Authorize reconsidering parallelism or process split when **all** hold:

- Sustained representative `tick` (or Instant whole-tick) utilization approaches the **operational headroom** agreed in Phase 7.8 / `PERFORMANCE_BUDGETS.md` (not yet a frozen % in this report).
- p95/p99 tick degradation is reproducible and material vs tick spacing (`TICK_RATE_HZ` / 30 Hz).
- Dominant Instant leaf is identified (`replication_policy`, `npc_activity`, etc.).
- Reasonable local opts for that owner are done or proven insufficient.

Telemetry: `capacity_live.json`, Instant owner leaves, `tick_domains`, ladder summaries.

### Replication trigger

Authorize replication-local redesign (including staged parallel publish) when:

- `replication_policy` remains dominant after local scan/policy opts, **and**
- policy cost scales materially worse than the served workload (scanned_per_tick / observers / overlap evidence), **and**
- CPU/tick trigger conditions are approaching (or policy alone threatens the operational headroom).

Do not parallelize replication solely because it is the largest leaf while util remains small.

### Transport trigger

Authorize transport/routing redesign when:

- Persistent queue / backpressure growth with **healthy harness** and **healthy simulation** (tick util still comfortable),
- Evidence from `network_pressure`, session queues, enqueue reject / depth metrics (7.4 lineage).

Localhost non-limits remain explicit: do not treat lab transport as WAN capacity.

### Memory trigger

Authorize memory-driven redesign when measured per-process / per-world RSS (or equivalent) becomes the limiting resource under representative load — not peak WS on a short ladder alone. Use `process_resources` / 7.6 memory model as directional only.

### Product / world trigger (separate from performance)

Authorize **product** multi-world / channel / instance work when design requires:

- separate channels for population isolation,
- private instances / dungeons,
- geographically or logically independent worlds,
- fault isolation of independent content topologies,

**even if** the single process still has CPU headroom.

Keep product scaling **orthogonal** to performance scaling. Product need does not invent a CPU cliff.

---

## 7.7C — Candidate architectures (design only)

### Option 1 — Keep single-process / single-owner (baseline; **selected**)

**Current:** one process → one `GameplayOwner` → one `World`; many `WorldAddress`es (map×channel×instance) inside that World.

| | |
|---|---|
| Strengths | Simplest correctness; single tick owner; existing AOI/replication/persist contracts; ops = one binary |
| Remaining headroom | Large in measured envelope (util ≲11% at peak ladder cells) |
| Operational simplicity | Highest |
| Likely first future limiter | `replication_policy` scan volume |

### Option 2 — Local replication parallelism

Parallelize observer build/encode only if `replication_policy` (+ publish path) becomes CPU-bound after local opts.

| Concern | Assessment |
|---|---|
| Independent work units | Per-`ConnectionId` / observer: `ObserverReplicationState`, pipe encode/enqueue |
| Immutable / shared inputs | Need frozen dirty discover + tick-frozen fanout snapshot / observer_count |
| Output merge / order | Serial merge for `InterestFanoutIndex` Enter/Leave commits |
| Enter/Update/Leave | Must preserve Leave→Enter→Update; commit-after-enqueue |
| Epoch / dirty | Dirty drain once per tick stays serial; epoch bumps stay owner-serial |
| Determinism | Mid-classify `fanout.remove_known` couples observers today — must defer |
| Sync overhead | Barrier + merge may erase gains until policy ms is large |

**Do not implement now.** Feasibility detail: §7.7G.

### Option 3 — World / zone ownership across workers or processes

Partition authoritative spatial ownership.

| Concern | Assessment |
|---|---|
| Ownership rules | Who owns an entity at a coordinate / address? |
| Entity migration | Move authority + replication Known rebuild |
| Cross-zone visibility | Overlap strips or proxy observers |
| Player handoff | Session stays; entity ownership moves |
| Cross-boundary interactions | Combat, portals, AOI at edges |
| Failure isolation | Partial — neighbors still coupled |
| Consistency | High risk without careful single-owner invariants |

**Much harder** than independent instances. Not justified by evidence. Prefer only if product requires contiguous shared space beyond one process after cheaper options fail.

### Option 4 — Channels / instances (independent world copies)

Logical isolation via `WorldAddress` (already typed) or separate `World`s / processes per independent topology.

| | |
|---|---|
| vs zone split | Far easier: little/no live spatial handoff if instances do not share players mid-tick |
| Alignment | `WorldAddress` / ADR-0040 already isolates relevance; multi-map in one World exists |
| Gaps | Production channel allocation, persist restore of channel/instance, multi-process routing |

Strong candidate as **first horizontal unit** when product or process-local capacity needs isolation — still **not** required for CPU today.

### Option 5 — Multi-process horizontal scaling

Multiple authoritative server processes + outer session/routing layer.

| Concern | Assessment |
|---|---|
| World ownership | One World (or address set) per process initially |
| Connection routing | Frontend must pin session → process |
| Persistence | Character writes remain single-writer per CharacterId |
| Discovery / handoff | New operational surface |
| Failure | Process crash = that world’s players; avoid dual ownership |

Adopt **after** (or with) independent channels/instances when one process’s measured envelope is insufficient — not before.

---

## 7.7D — Preferred evolutionary path

Selected progression (minimize rewrite, sync, correctness risk, ops cost):

```text
single process / single World owner
  → local owner optimization (policy / scan / content locality; still Phase 6 architecture)
  → optional staged replication parallelism (only if replication CPU trigger fires)
  → independent channels / instances as horizontal unit (product and/or capacity)
  → multiple authoritative world processes (one independent topology per process)
  → cross-world / zone handoff only if contiguous shared space truly requires it
```

**Why this path:**

1. Preserves ADR-0003 server authority and today’s single tick owner as long as possible.
2. Keeps Phase 6 AOI/replication architecture closed; opts stay inside policy/ownership.
3. Replication parallelism is a **local** change with an explicit commit stage — cheaper than sharding.
4. Channels/instances reuse `WorldAddress` isolation already in the codebase; avoid inventing zone strips first.
5. Multi-process and zone handoff add failure and dual-ownership risk; defer until evidence or product forces them.

---

## 7.7E — Ownership model (preferred future = evolve from today)

Invariant: **server-authoritative**; clients never author authoritative results.

| Domain | Ownership class | Owner today / preferred future |
|---|---|---|
| World (simulation container) | **Exactly one owner** per authoritative topology | `GameplayOwner.world` in one process; future: one owner process per independent World/instance set |
| Entity | **Exactly one owner** | Owning `World`; migration must be exclusive handoff |
| Player / session | **Exactly one owner** for binding | `ConnectionId` → `PlayerBinding` on gameplay owner; network session may be routed by outer layer but gameplay binding is singular |
| Simulation tick | **Exactly one owner** | Single threaded clock/`World::tick` per World; no dual tickers |
| Replication observer state | **Exactly one owner** per observer | Per-binding `ObserverReplicationState`; fanout index is shared mutable under that World’s publish owner |
| Scheduler | **Exactly one owner** | Simulation scheduler inside World (ADR-0045) |
| Actions / effects | **Exactly one owner** | Simulation on owning World |
| Map / world instance | **Exactly one owner** | `WorldAddress` membership inside owning World; production instance lifecycle deferred |
| Persistence writes | **Exactly one writer** per `CharacterId` | Persistence path on owning process; no concurrent writers |
| Network connection | **Exactly one owner** | Quinn session task + SessionTable; must not tick World |

**Shared immutable state** (allowed): content definitions, protocol codecs, read-only snapshots frozen for a parallel build stage.

**Coordinated state** (avoid until required): cross-process entity authority, consensus, distributed dirty sets.

Never “shared server state” without naming the single owner or the freeze/merge contract.

---

## 7.7F — Cross-boundary contracts (conceptual only)

If multiple workers/processes/worlds appear, these contracts must exist **before** implementation. **No protocol messages added in 7.7.**

| Contract | Purpose |
|---|---|
| Entity migration | Exclusive transfer of EntityId authority + component blob; source forgets before dest applies |
| Player transfer | Rebind ConnectionId → dest World/process; input epoch bump; no dual binding |
| Destination reservation | Dest admits capacity/slot before source releases |
| Handoff acknowledgement | Two-phase: reserve → migrate → ack; timeout → rollback |
| Failure / rollback | Prefer cancel migration over dual ownership; player may need reconnect/resync |
| Observer rebuild | Epoch bump; clear Known; baseline Enter on dest |
| Baseline / resync | Client treats address/epoch change as full relevance reset (already familiar from Channel/Portal) |
| Persistence sequencing | CharacterId writes ordered; migrate must not interleave two writers |

Exposing this list early shows why **zone handoff is expensive** relative to independent instances.

---

## 7.7G — Replication parallelism feasibility

### Current dependencies (inventory)

| Piece | Role |
|---|---|
| Dirty discover | Serial `drain_replication_dirty` → fan to Known via `InterestFanoutIndex` → `pending_update_ids` |
| `InterestFanoutIndex` | Shared mutable reverse index; classify may `remove_known` mid-loop |
| Observer pending / Known / Want* | Per-observer mutable |
| Cadence / policy / budget | Per-observer + live `fanout.observer_count` reads |
| Enter/Update/Leave order | Leave → Enter → Update; commit after successful enqueue |
| Frame encode / enqueue | Per `ReplicationPipe` |

### What could run independently?

Theoretically: per-observer classify **plan** + encode + enqueue **if** inputs are frozen and fanout mutations are deferred.

### Verdict

**Feasible with an explicit commit stage** (serial discover → parallel plan/encode → serial fanout/World merge).

Not structurally “drop-in parallel `for cid`”. Mid-classify fanout mutation and live `observer_count` make unordered parallel loops **unsafe** under current ownership.

Also: **unnecessary until much higher measured load** — util remains ≪ tick budget; first response to policy pressure should be **local scan/policy optimization**, not threads.

---

## 7.7H — Channels / instances readiness

### Answers

| Question | Answer |
|---|---|
| Can one process own multiple independent worlds **today**? | **One `World`** already hosts many `WorldAddress`es (maps/channels/instances). **Not** multiple `World` structs / processes. |
| Globals that assume one World? | Single `GameplayOwner` / `World` per process; `map_a`/`map_b` DEFAULT topology helpers; restore → DEFAULT channel/instance; load pressure on map_a |
| Session / MapId support for multiple maps? | Entity carries `WorldAddress`; portals `ensure_map`; SessionTable is connection-only (no map) |
| Routing requirements for multi-process? | Outer pin ConnectionId → process; character placement chooses topology; no such router today |
| Process-global state? | Content registry, listen socket, SessionTable, persist dir, metrics — OK global |
| World-instance-local? | Entities, spatial grid, AOI/fanout, scheduler, actions, observer Known |

**Assessment:** Channels/instances are **naturally aligned** with existing `WorldAddress` contracts (ADR-0034/0040/0044). They are the **preferred first horizontal scaling unit** when isolation is needed — still deferred as product/ops work, not as a 7.7 implementation.

Production channel allocation, `MapInstanceId` as a distinct runtime type, and persist of channel/instance remain **gaps** (documented, not built).

---

## 7.7I — Failure model (prefer easy boundaries)

| Architecture | Crash / partition behavior | Dual-ownership risk | Preference |
|---|---|---|---|
| Single process | Process down ⇒ all players reconnect; one authority | None | **Best** today |
| Replication parallelism | Same process; bug risk is races/stale Known | Low if serial commit correct | OK when triggered |
| Channels/instances (in-process) | Same process; isolation is logical | Low | Good product unit |
| Channels/instances (multi-process) | One topology dies; others live | Avoid if routing wrong | Good **if** no live handoff |
| Zone partitioning | Neighbor loss; edge interactions fail; handoff races | **High** | Avoid until required |
| Multi-process + handoff | Lost ack ⇒ duplicate or drop entity | **High** without strict protocol | Last resort |

**Do not** introduce consensus / distributed DB machinery without a concrete requirement. Prefer reconnect + authoritative resync over split-brain repair.

---

## 7.7J — Decision matrix

Scores are **qualitative** from 7.3–7.6 evidence. Capacity benefit is **projected**, not measured saturation relief.

| Architecture | Need now | Complexity | Capacity benefit | Correctness risk | Operational cost | Trigger |
|---|---|---|---|---|---|---|
| Current single process | **Yes (canonical)** | Low | N/A (baseline) | Low | Low | Remains default until triggers fire |
| Local owner opts (policy/scan) | Not yet (headroom) | Low–med | Projected first win on policy | Low | Low | Policy dominant + approaching headroom |
| Replication parallelism | **No** | Med | Projected if policy CPU-bound | Med (ordering/fanout) | Low–med | Replication + CPU triggers after local opts |
| Channels/instances | **No** (product later) | Med | Isolation / pop split; not proven CPU fix | Low–med | Med | Product need **or** process envelope exhausted |
| Zone partitioning | **No** | High | Speculative | High | High | Only if contiguous shared space exceeds process after above |
| Multi-process worlds | **No** | High | Horizontal when one process saturated | Med–high | High | Measured process limit + independent topologies |

---

## 7.7K — Recommended architecture state (explicit decision)

### Decision

> **Remain single-process. No scaling implementation is justified.**

Recorded as **ADR-0056**.

### Documented consequences

1. **Current architecture remains canonical:** one process, one `GameplayOwner`, one authoritative `World` tick owner; multi-address membership inside that World.
2. **Likely first future limiter:** `replication_policy` (scan volume / overlap growth).
3. **Preferred first scaling response:** local policy/scan/content-locality optimization; then staged replication parallelism only if CPU+replication triggers fire.
4. **Trigger that authorizes implementing parallelism / multi-process:** §7.7B contract (measurable, reproducible, owner-known, local opts insufficient / product need).
5. **Preferred next horizontal scaling unit:** independent **channels/instances** (aligned with `WorldAddress`), then multi-process independent worlds; zone handoff last.
6. **Must NOT be built yet:** multithreaded simulation, parallel replication ship, sharding, zone servers, cross-process handoff, service mesh, consensus, production channel allocator, new routing infrastructure, speculative capacity opts without owner evidence.

---

## 7.7L — Architecture readiness backlog (deferred; do not implement now)

| Item | Class |
|---|---|
| Formalize “one GameplayOwner / one World per process” as the documented scaling unit; keep multi-address inside World | `required before scaling` (docs invariant — done via ADR-0056) |
| Freeze dirty discover + fanout snapshot before any parallel observer build; separate plan vs commit | `only if architecture X` — **X = replication parallelism** |
| Tick-frozen `observer_count` (or equivalent) for policy if parallelizing classify | `only if architecture X` — replication parallelism |
| Clarify session→topology routing for multi-process (ConnectionId pin) | `required before scaling` — multi-process / multi-World |
| Production channel allocation policy + persist of ChannelId/InstanceId (ADR-0040/0044 gaps) | `required before scaling` — production channels/instances product |
| Handoff identity contracts (reserve/ack/rollback) before any zone/process migrate | `required before scaling` — zone or live player migrate |
| Remove / quarantine accidental process-global assumptions that block multi-World (`map_a`-only helpers, single bounds caveats) | `nice-to-have` until multi-World chosen; `required before scaling` if multi-World in one process |
| Distinct runtime `MapInstanceId` type if product instances outgrow address triple | `only if architecture X` — rich instance product |
| Split immutable snapshot/build from mutable replication commit in code structure | `nice-to-have` now; `required before scaling` for replication parallelism |

---

## Proof strategy (for future reopen)

1. Reproduce representative ladder cell with Instant owners + `capacity_live`.
2. Show trigger class from §7.7B with artifacts under `logs/load/`.
3. Demonstrate local opts insufficient (or product trigger documented).
4. Only then open an implementation phase with explicit scope (parallelism **or** channels **or** multi-process — not all at once).

---

## Quality gate

`./scripts/check.ps1` — **PASS** 2026-09-02 (`fmt`, `check`, `clippy -D warnings`, `test --workspace`, content validator). Docs/ADR only (no gameplay semantic change).

## Explicit non-goals honored

No multithreading, parallel replication, sharding, zone servers, handoff protocol, distributed state, consensus, production channels, or capacity optimization campaign.

## Boundary

Work **stops before Phase 7.8**.

## Recommended Phase 7.8 direction

Phase 7.8 should define the **production performance gate** from measured envelopes and living budgets:

1. Freeze operational headroom / tick p95–p99 expectations **as policy**, citing 7.3–7.6 evidence (not unsupported 50–80% util claims).
2. Codify which artifacts and workloads constitute a pass/fail production gate.
3. Keep ADR-0056: gate ≠ authorization to shard; gate may cite §7.7B triggers for a **future** implementation phase.
4. Do not start gameplay-vocabulary work from 7.8.
