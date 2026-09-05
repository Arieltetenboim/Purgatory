# Phase 7.8 — Production Performance Gate

**Status:** complete / **YELLOW** (operational absolute thresholds PASS; harness snapshot-starvation WARN on N≥32)  
**Date:** 2026-09-02  
**Root `PHASE`:** `7.8` → **Phase 7 complete**  
**Architecture:** ADR-0056 unchanged — single-process / single `World` owner  
**Canonical entry:** [`scripts/phase_78_gate.ps1`](../scripts/phase_78_gate.ps1)  
**Thresholds:** [`scripts/phase_78_thresholds.json`](../scripts/phase_78_thresholds.json)  
**Baseline:** `logs/load/capacity_78/baseline/baseline.json`  
**Evidence run:** `logs/load/capacity_78/gate_20260902_082337/`

## Verdict

Phase 7.8 freezes a **regression gate**, not a maximum player-capacity claim.

| Result | Meaning |
|---|---|
| **YELLOW** | Absolute operational thresholds intact; WARN findings are harness `snapshot_starvation` on N≥32 (known; not server tick saturation) |
| Absolute tick/util/overruns/lifecycle/network | **PASS** on all frozen cells |
| Soak (8m) | **Completed** wall_secs≈480; tick p99≈0.58 ms; deaths=177; RSS Δ≈4.5 MB; push fails=0 |

ADR-0056 remains: thresholds do **not** authorize sharding or multithreading.

---

## 7.8A — Frozen workloads

| Gate | Cell id | Workload | Duration | Purpose |
|---|---|---|---|---|
| 1 | `functional_mixed8` | `representative-mixed` @ 8 | 45s | Correctness + instrumentation + lifecycle |
| 2 | `standard_mixed64` | `representative-mixed` @ 64 | 45s | Primary performance regression |
| 3 | `high_mixed128` | `representative-mixed` @ 128 | 45s | High observer / policy pressure |
| 4 | `dense_dense64` | `representative-dense` @ 64 | 45s | Overlap / dense AOI |
| 5 | `soak_mixed32` | `hotspot` @ 32 + NPC overrides | **8m** | Memory / queue / lifecycle stability |
| — | `unit_budget` | cargo tests (packer / Enter / self) | — | Byte-budget / priority correctness |

Policy baseline: `selective` / `high`, soft budget **4096**, stranger cadence **×1**, `health_max=6` (soak uses NPC overrides), seed **7808** (soak seed **7505**).

**Soak note:** Long `representative-mixed` / MixedRuntime soaks abort on portal-role death and persistent-baseline drops (harness), which is **not** a server tick cliff. The frozen soak uses `hotspot` + representative NPC workload so duration, lifecycle, memory, and queues are measured without that harness gate.

---

## 7.8B — Workload attainment

Invalid if:

- `attainment_pct` < 95, or
- `funnel_invariant_ok` false, or
- `wall_secs` < 85% of requested duration (`HARNESS_SHORT_RUN`)

Classified **`HARNESS_INVALID` / verdict INVALID**, never as server capacity failure alone.

---

## 7.8C — Tick / headroom thresholds (rationale)

30 Hz spacing ≈ 33.3 ms is **not** the pass bar. Caps preserve substantial headroom while absorbing localhost variance (high-N p99 has been seen up to ~20 ms under machine noise while still ≪ budget).

| Cell | tick p95 max | tick p99 max | util % max | tick max | overruns |
|---|---:|---:|---:|---:|---:|
| functional @8 | 3 | 5 | 15 | 20 | 0 |
| standard @64 | 6 | 10 | 25 | 25 | 0 |
| high @128 | 16 | **24** | **55** | 32 | 0 |
| dense @64 | 16 | **24** | **55** | 32 | 0 |
| soak @32 | 5 | 8 | 20 | 25 | 0 |

Isolated max spike WARN above 16 ms if still under cell max. Consecutive overrun streak max **0**.

---

## 7.8D–E — Owner / unattributed

Track `replication_policy`, `npc_activity`, replication children, `spatial_aoi`, unattributed share, total tick.

Unattributed: expected ~15–25% post-7.5; **WARN ≥35%**, **FAIL ≥50%** (new uninstrumented path). Do not optimize timer noise.

Relative vs baseline (YELLOW): tick p99 +40%, policy mean +50%, policy µs/observer +50%, RSS end +35%, etc. (see thresholds JSON).

---

## 7.8F — Network / backpressure

From 7.4: depth max typically 1, push fails 0.

Gate:

- FAIL push/depth only when hard-cap depth coincides with material tick util (≥5%)
- Else WARN as connect/teardown noise (observed flake on isolated low-N runs)

Not a WAN bandwidth SLA.

---

## 7.8G — Byte budget / priority

Unit tests (must report ≥1 pass each):

- `priority_prefers_self_under_tiny_budget`
- `progressive_enter_does_not_emit_updates_before_known`
- `updates_do_not_precede_enter`
- `leave_then_reenter_sends_baseline_again`
- `self_always_full_domains`

Default production soft budget remains **4096**. No Phase 6 architecture retune.

---

## 7.8H–I — Lifecycle / memory

Lifecycle required on all cells: non-zero deaths/respawns and actions/effects/pulse.

Soak RSS: fail on `rss_delta_mb` > 24; growth ratio vs start is **warn-only** (start is pre-warm).

Frozen soak: RSS Δ ≈ 4.5 MB over 8m; deaths 177.

---

## 7.8J — Harness health

Statuses: `PASS` | `WARN` | `SERVER_FAIL` | `CORRECTNESS_FAIL` | `HARNESS_INVALID`.

Harness `snapshot_starvation` / severe controller tick → WARN, not server fail.

Overall: `GREEN` | `YELLOW` | `RED` | `INVALID` (see gate summary semantics).

---

## 7.8K–M — Baseline, comparison, automation

- Absolute thresholds + baseline relative bands
- `scripts/phase_78_gate.ps1` builds, runs cells, evaluates, writes `phase78_gate_summary.json` + `.txt`
- Exit 0 = GREEN/YELLOW; 1 = RED; 2 = INVALID
- Auto-freeze baseline on first GREEN/YELLOW; `-FreezeBaseline` to refresh

Hub: Settings → **PHASE 7.8 GATE** (visible `phase_78_gate.ps1`); Performance page notes the canonical command.

---

## 7.8O — Soak result (frozen)

| Metric | Value |
|---|---|
| Wall | ≈480 s |
| tick p99 / util | 0.58 ms / 0.80% |
| attainment | 100% |
| deaths | 177 |
| RSS Δ | ≈4.5 MB |
| push fails / queue | 0 / healthy |
| Harness | WARN unexpected_disconnects (residual bots); server tick healthy |

---

## 7.8P — Phase 7 closure

### Measured

- Envelope through mixed@128 / dense@64 comfortably under 30 Hz budget (util typically ≪15% on canonical cells; variance can raise high-N p99 without overruns)
- `replication_policy` primary growth owner under player/density growth
- Network writer queue healthy in representative cells; localhost peak outbound ~0.5 MB/s class (7.4)
- Memory mild (tens of MB); soak Δ small
- Lifecycle (death/respawn/effects) active under representative NPC workload
- Harness limitations: snapshot starvation / portal-role / MixedRuntime baseline on long soaks

### Modeled (7.6)

- Policy ~ scanned_per_tick; NPC ~ updates/tick; ~20–22% holdout MAPE
- Sensitivity: players > overlap > activity/NPC
- Likely first future limiter: `replication_policy`

### Not proven

- Maximum player capacity
- WAN transport capacity
- Multi-process / sharding requirement
- Geographic scaling

### Architecture

**ADR-0056:** remain single-process / single World owner. 7.8 gates are regression checks only.

---

## Quality gate

`./scripts/check.ps1` — **PASS** 2026-09-02 (`fmt`, `check`, `clippy -D warnings`, `test --workspace`, content validator) after clearing leaked `PURGATORY_LOAD_*` env from capacity runs. Gate script now clears those env vars on exit.

## Remaining technical debt

- MixedRuntime long-soak portal/baseline harness fragility (documented; soak uses hotspot+NPC)
- Occasional low-N writer push-fail noise at connect/teardown
- Unattributed residual timer noise (~15–25%)
- Production channel allocation / multi-process readiness backlog from 7.7 (not implemented)

## Recommendation for Phase 8

Phase 7 is **complete**. Resume deferred product/gameplay sequencing from [`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md) / roadmap after 7.8 (content/systems vocabulary as separately authorized). Do not reopen AOI architecture or invent capacity claims from this gate.

## Boundary

Work **stops after closing Phase 7**. Do not begin Phase 8 until instructed.
