# Phase 6G.6 — AOI locality + relevance-driven replication characterization

**Status:** characterization recorded — **stop for owner review**. No Phase 7. No cap raise. No large replication redesign. Instrumentation only (+ small metric helpers).

Root `PHASE` = `6G.6`. Protocol **v10**. Artifacts: `logs/load/capacity_6g6/`.

## Recommendation

### **D — Both AOI invalidation and replication relevance need another core pass before 6G closes**

| Layer | Verdict |
|---|---|
| Spatial (who) | 6G.5 fixed **global** invalidation; remaining waste is **movement-invalidation envelope ≫ actual membership-change set** |
| Property/event (what) | Updates are partly change-driven (`DomainRevs`), but architecture cannot yet express relationship-based domains or entity→interested-observer fan-out |
| Frequency/budget (when) | Cadence/budget exist in coarse form; not density/pressure adaptive; no priority packer |

Closing 6G now would leave both “tiny move dirties leave-stable observers” and “dense mutual visibility still sends full Known transform traffic” unresolved as *architectural* gaps.

---

## Part A — Spatial invalidation efficiency

### Instrumentation added

- `InterestLocalityAccounting` on `World` → capacity artifact `aoi_locality.json`
- Classify stats: `membership_transitions`, `classify_unchanged` on `ReplicationTickStats`
- Targeted scenarios in `crates/simulation/src/phase6g6_tests.rs`

### Targeted locality results (decisive)

| Scenario | Result |
|---|---|
| Tiny same-cell move | **dirtied=21, exact leave-XOR observers=0** → pure wasted invalidation |
| Cell boundary cross | Far cluster not dirtied (locality preserved vs opposite side) |
| Separated clusters | Zero cross-talk |
| Dense mutual cluster | Dirties most of cluster (correct) |
| Mover + stationary crowd | `dirtied_per_move ≈ 31` when crowd is in influence |
| AOI enter/leave boundary step | Observer dirtied / leave-XOR can be non-zero (correct path) |

**Wasted classification ratio (tiny move):** influence set / membership-changing observers = **∞** (21 / 0). This is the core Part B finding.

### Ladder (characterization runs, seed 4242)

First full ladder (30–60 s) wrote `aoi_locality.json` while “dirtied” briefly meant *newly inserted* dirty bits (under-reports once observers stay dirty). Spot re-runs after the fix count **full influence-set size** per move:

| Scenario (spot) | tick/AOI p99 | influence mean/p95/p99/max | per_moved | same_cell / cross |
|---|---|---|---:|---|
| distributed@64 | 7.0 / 3.4 | 48.8 / 64 / 64 / 64 | **32.8** | 6942 / 330 |
| hotspot@64 | 10.9 / 5.2 | 46.0 / 64 / 64 / 64 | **37.5** | 9872 / 356 |
| distributed@128 | 6.7 / 4.4 | 61.0 / 78 / 79 / 79 | **38.1** | 9302 / 443 |
| hotspot@128 | 15.8 / 7.3 | 61.8 / 87 / 87 / 87 | **45.7** | 16131 / 556 |

Earlier full N=256 ladder timings (domain p99): distributed 24.0/16.0/11.2; hotspot 29.4/14.8/10.0 — see `locality_ladder_summary.json`.

**Dominant distributed-motion cost:** ~95% of moves are **same-cell**; each still expands full `AOI_INFLUENCE_HALF_EXTENTS` and marks a large influence player set → classify churn without membership change (see tiny-move test: dirtied=21, leave-XOR=0).

Machine-readable: `logs/load/capacity_6g6/20260901_164045/locality_ladder_summary.json` + later `*_n/aoi_locality.json` spot dirs.

---

## Part B — Is the influence envelope too conservative?

**Yes, as a *movement invalidation* envelope — not as a *relevance extent*.**

| Question | Answer |
|---|---|
| Relevance extent (how far can an entity be relevant?) | Leave-sized; measured max observer→leave extent ≈ **28.4 × 17.5** → current `[28.5, 17.6]` is **correctness-safe** |
| Movement invalidation (who can change membership for old→new?) | Observers where `in_leave(old) XOR in_leave(new)`, plus the mover | 
| Small displacement | Exact XOR set often **empty**; full influence still marks tens of players |

**Do not shrink the constant alone** without a proven replacement. Candidates (not implemented):

1. **Exact leave-XOR** after a spatial prefilter (safe, O(prefilter players) leave tests)
2. **Swept boundary band** around the leave frontier for old→new segment
3. **Same-cell micro-move skip** when displacement cannot cross any leave edge (harder to prove with clamped view envelopes)

Proof bar: property test that influence/XOR supersets membership transitions for random FOOTNOTE poses (extend `influence_covers_leave_inverse_on_footnote`).

---

## Part C — Dirty / event-driven replication audit

### Already change-driven

| Mechanism | Scope |
|---|---|
| `DomainRevs` (transform/health/membership/replication) | Per-entity domain generations |
| Per-observer `CommittedRevs` | Observer compares world revs → pending Update |
| `update_record` | Emits only domains with rev lag; stationary Known → **no Update payload** |
| Velocity-only `bump_transform_rev` | Can dirty transform without AOI invalidate (correct split) |
| Enter baseline | Full snapshot on Enter only |

### Still periodic / broadcast-shaped

| Behavior | Issue |
|---|---|
| Every observer, every tick: walk **all Known** for rev compare | O(known) inspect even when world idle for that observer |
| Cadence (`EveryTick` / Normal / Low / Event) | Time-stagger deferral, not “notify only interested on change” |
| No entity→observer reverse index | Entity move cannot push to interested set without dirty AOI + per-observer classify/publish |
| Header frame every tick | Always builds a frame path (budget/queue may still skip work) |
| Skill/action/events | Not a general change-notification bus; gameplay events ≠ replication dirty graph |

### Dirty tracking shape today

- **Global:** removed for AOI (6G.5)
- **Per-entity domain:** `DomainRevs` ✓
- **Per-observer commit cursor:** `CommittedRevs` / Life state ✓
- **Per-observer AOI dirty:** HashSet ✓
- **Missing:** entity-component dirty → interested-observer fan-out queue

Baselines/resync/epoch bump must remain — change notification is for incremental path only.

---

## Part D — Relevance is not binary

| Capability | Today | Gap |
|---|---|---|
| Entity relevance (exists in interest) | Enter/Leave/Known + class VisibleObservers/OwnerOnly/None | Coarse class only |
| Domain relevance per observer relationship | `DomainMask { transform, health }` on Update; Enter always pose+health | **No** self/party/target/stranger policy; no “pose only for strangers” |
| Architecture expressiveness | Could grow masks + filters on `update_record` / Enter builders keyed by relationship tags | Needs relationship graph (party/target) + policy table — **not present** |

Conclusion: protocol can *eventually* carry selective domains; **server policy architecture cannot yet** express Part D examples cleanly.

---

## Part E — Density-aware policy (same protocol)

| Input available today | Use |
|---|---|
| AOI candidate / known counts | Partial (`ObserverAoiDebug`, stats) |
| Byte budget / queue depth | Soft frame budget, writer queue cap |
| Tick overruns | Capacity artifacts |
| Map population class | **Not** modeled |
| Adaptive cadence/priority under density | **Not** implemented |

Feasible design (future, same wire semantics): map content class × live density × byte/tick pressure → cadence tier + domain eligibility + priority — **no second protocol**. Critical Enter/Leave/self must not yield.

---

## Part F — Priority model (characterization only)

Needed packing order under pressure (not implemented):

1. lifecycle Enter/Leave / epoch baseline  
2. self state  
3. party/target  
4. nearby movement (pending transform)  
5. visible actions/events  
6. ordinary stranger  
7. cosmetic  

Today: leave→enter→update order with soft byte budget; cadence deferral only. **No explicit priority packer** — justified only after B/C reduce waste.

---

## Three scaling problems (do not conflate)

1. **Who** — AOI + invalidation locality (6G.5 done; **6G.6 shows remaining envelope waste**)  
2. **What** — domain/relationship relevance (**architectural gap**)  
3. **When** — cadence/priority/budget (**coarse only**)

A map with 500 mutually visible players cannot be fixed by AOI exclusion alone — needs (2)+(3).

---

## Correctness constraints (honored)

Server authority, AOI semantics, Enter/Leave/Known, baseline/resync, protocol v10, map/channel isolation unchanged. No Phase 7. No cap raise.

## Quality gate

- `phase6g6` tests PASS; replication tests PASS  
- clippy `-D warnings` on common/simulation/server PASS  
- Full `./scripts/check.ps1` recommended before merge of any follow-up implementation

## Owner decision

Accept **D** and authorize a follow-up (suggested order):

1. **Targeted AOI movement invalidation** (exact leave-XOR / swept band; keep full envelope as fallback)  
2. **Dirty/relevance replication pass** (entity→interested fan-out + domain policy hooks; still no Phase 7 gameplay rules)

Or reject and request a different recommendation framing.
