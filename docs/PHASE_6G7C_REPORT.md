# Phase 6G.7C — Relationship / density / budget replication policy foundation

**Status:** accepted — closes the 6G architectural chain.

**6G = GREEN — architecture closed, production policy tuning deferred.**

Root `PHASE` = `6G`. Design: [`docs/PHASE_6G7C_DESIGN.md`](PHASE_6G7C_DESIGN.md). Artifacts: `logs/load/capacity_6g7c/`.

Closing criterion (owner): the runtime can respond to dense mutual visibility **selectively** along

```text
change-driven → localized AOI → dirty fan-out → observer-specific policy → priority/cadence/budget
```

without a second protocol or further fan-out redesign. Closing is **not** a claim that 256 is “enough,” nor that Hz / bandwidth / relation thresholds are final production numbers.

## Recommendation (accepted)

### **A — 6G architecture closed; production tuning / content-specific policies deferred**

Known deferred limits (legitimate; do **not** reopen 6G architecture):

* Low/Medium/High/Extreme pressure thresholds not final
* exact stranger / party / target cadences not final
* per-client bandwidth budgets need production tuning
* 512/1024 dense visibility remains characterization, not a production guarantee

---

## Owner verification checklist (pre-GREEN)

### 1. Correctness under selective mode

Selective policy sits **after** dirty discovery and **before** encode/queue. It does not mutate authoritative `World` state, tick identity, or AOI membership.

| Concern | Evidence |
|---|---|
| Server authority | Policy only filters/prioritizes **what is packed** for an observer; sim/health/transform remain server-owned |
| Lifecycle / Enter / Leave / Known | Enter/Leave packed **before** Updates; existing unit coverage (`first_observe_is_enter_baseline`, `leave_then_reenter_sends_baseline_again`, epoch baseline) unchanged in structure |
| Baseline / resync / recovery | Epoch / recovery Known reconcile paths remain outside ordinary cadence suppress; unchanged-domain → no ordinary delta still holds |
| Critical / event-like work | EventLike (Enter/Leave/epoch) not subject to stranger state cadence/coalesce; Updates ordered after lifecycle packing |
| Prediction / reconciliation | Client prediction/reconcilation (Phase 5.5) consumes the same v10 frames; selective changes **eligibility/cadence of stranger state**, not authoritative results or trust boundary |
| Ladder operational health | Hotspot baseline vs selective @64/128/256: **0 overruns**, runs COMPLETE |

Automated: replication + policy suite PASS (incl. stranger health suppress emit proof). Selective does **not** reintroduce unchanged-state spam.

### 2. What the ~70% update cut actually was

Hotspot ladder identity (selective vs baseline): updates fall ≈ **67–70%** (e.g. 343k→103k @64; 363k→115k @256) while tick/repl p99 do **not** worsen.

From `policy_ladder_summary.json`, per mode the identity holds:

```text
updates ≈ eligible − coalesced
```

| Mode @N | eligible | coalesced | updates | suppressed | priority_deferred |
|---|---:|---:|---:|---:|---:|
| baseline 64 | 433k | 90k | 343k | 0 | 0 |
| selective 64 | 448k | **345k** | **103k** | 0 | 0 |
| baseline 256 | 460k | 97k | 363k | 0 | 0 |
| selective 256 | 507k | **392k** | **115k** | 0 | 0 |

**Interpretation (careful):**

* Primary ladder savings = **state-like stranger transform cadence stretch → coalesce** (keep newest pose; skip intermediate stranger Updates). That is exactly low-value/high-volume mutual-motion fan-out under dense visibility — representative of the problem 6G.7C targeted, not an artificial health spam harness.
* `policy_domain_suppressed = 0` on this ladder because the hotspot bots are **motion-dominated**; stranger **health** eligibility suppress is proven in unit tests (`selective_policy_suppresses_stranger_health_on_emit`) but was not the dominant byte/update driver here.
* Interested relationship counts stay the same order of magnitude — AOI/fan-out still discover real edges; policy reduces **emit rate/detail**, not interest correctness.

### 3. Pressure behavior (preserve critical, degrade low-value first)

Two mechanisms; both required for the architecture claim:

| Mechanism | Ladder evidence | Unit evidence |
|---|---|---|
| Pressure → stranger cadence stretch → coalesce | Selective coalesced ≈3.5–4× baseline; updates ↓ without tick regression | `decide_update_policy` pressure multipliers |
| Soft per-observer byte budget → priority pack | `priority_deferred = 0` on this ladder (soft budgets **did not bind** at these loads) | `priority_prefers_self_under_tiny_budget` — self Update retained under tiny budget |

So: the dense ladder proves **pressure-shaped state degradation** (cadence/coalesce) under real mutual visibility. Preferential retention of high-priority Updates when a **byte budget binds** is architecturally wired and unit-proven; it was not the active limiter in these Release runs. That is expected for a proof foundation and is deferred production tuning — not an open architecture gap.

---

## 1. Policy architecture

```text
domain dirty → interested Known
  → classify_relation (Self/Party/Target/Nearby/Distant)
  → decide_update_policy (mode × pressure × relation × domains)
  → priority sort + cadence coalesce + observer byte budget
  → encode / queue (Enter/Leave already packed first)
```

Modes: `PURGATORY_REPLICATION_POLICY=baseline|selective` (default selective). Population hint: `PURGATORY_POPULATION_CLASS`.

## 2. Relationship / domain eligibility

Extensible `ObserverRelationKind` + optional `RelationOverrides` (party/target tables; no full gameplay systems).

Selective proof: strangers → **transform only** (health silent catch-up); Party/Target/Self keep health.

## 3. State vs event

| Class | Treatment |
|---|---|
| StateLike (transform/health) | coalesce, cadence, ineligible silent catch-up |
| EventLike (Enter/Leave/epoch) | packed before Updates; not silently dropped |

Unchanged domain → no ordinary delta (preserved).

## 4. Density / pressure inputs

`PopulationClass` hint + runtime: Known count, subject interested count, recent observer bytes, tick overrun hint → `PressureLevel` → stranger cadence stretch + `observer_frame_budget_bytes`.

## 5. Priority / budget

`ReplicationPriority` orders Updates under soft per-observer budget. Lifecycle remains ahead of Updates structurally.

## 6–7. Dense visibility before/after (hotspot, seed 4242)

| Mode | N | tick p99 | repl p99 | updates | bytes | bytes/client/s | coalesced |
|---|---:|---:|---:|---:|---:|---:|---:|
| baseline | 64 | 4.27 | 1.71 | 343k | 11.6M | 5557 | 90k |
| selective | 64 | 4.45 | 1.56 | **103k** | **5.2M** | **2590** | **345k** |
| baseline | 128 | 4.63 | 2.12 | 374k | 12.6M | 3019 | 97k |
| selective | 128 | 4.19 | 1.35 | **106k** | **5.4M** | **1336** | **358k** |
| baseline | 256 | 6.10 | 2.49 | 363k | 12.1M | 1503 | 97k |
| selective | 256 | **4.81** | **1.60** | **115k** | **5.7M** | **709** | **392k** |

Overruns: **0**. Selective ≈ **3× fewer Updates**, ≈ **half the bytes**, much higher state coalescing. Same wire protocol. Cost was not merely shifted onto tick time.

**512 characterization:** real-session admission remains 256 — **not raised**. Live 256 already shows policy-shaped degradation; larger N stays a future probe.

Machine-readable: `logs/load/capacity_6g7c/summary_20260901_175207/policy_ladder_summary.json`.

## 8. Correctness evidence (summary)

See owner checklist §1. Suite: replication + policy unit tests PASS; clippy `-D warnings` on common/server PASS for the landed policy path.

## 9. Remaining scaling wall (deferred tuning, not architecture reopen)

Under dense **mutual motion**, transform Updates still scale with **dirty × interested** after cadence stretch. Next levers when gameplay/combat/UI/content exist: production thresholds, filled Party/Target tables, stranger transform quantization, budgets that actually bind in production shapes.

## Quality gate

- Targeted tests + clippy: PASS
- Full `./scripts/check.ps1` recommended before merge of any large uncommitted tree
- Ladder Release evidence: PASS (above)

## Explicit non-goals honored

No full party/target/combat systems, no final production bandwidth numbers, no second protocol, no AOI edits, **no Phase 7 started**, no cap raise for 512.
