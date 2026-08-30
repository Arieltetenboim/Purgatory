# Phase 6F report — Runtime gameplay readiness

## Implemented scope

- World-owned runtime primitives: simulation-time scheduler, lean Action table, typed action gate, command preamble, staged `RuntimeEvent` queue, temporary test effects, cadence, scheduled spawn, filtered spatial queries.
- `GameplayOwner::simulate_tick` uses explicit stages on the same `SimulationTick` already published as `server_tick`. FOOTNOTE, persistence worker, and network tasks stay outside this mutation domain.
- Critical due work is intended to complete this tick. `CRITICAL_DRAIN_CEILING` (1024) is a pathological-overload safeguard: remainder carries forward and `scheduler_critical_ceiling_hits` increments. Hitting it can delay gameplay timing; it is not a policy that critical work is deferrable. Deferred work is FIFO with budget 32 and a progress guarantee.
- Action storage is live/terminal only (`Active`, `Completed`, `Cancelled`, `Interrupted`, `Failed`). Request / validate / reject are command-pipeline outcomes and do not create an Action slot.
- `InputGateReason` maps to `ActionDenialReason::TransitionLocked` without rewriting ADR-0042. `InteractionSession` remains distinct from Action.
- Dirty/delta is existing `DomainRevs` plus per-observer `ObserverReplicationState`. Production replication does not `consume_dirty`. Replication Normal/Low cadence is staggered by entity index.
- Load metrics schema **3**: scheduler gauges, ceiling hits, deferred exhaustion, actions, events, spawn queue, cadence due, command rejects, domain-rev advances, observer pending Enter/Update, cadence-deferred Updates. No global “dirty pending” gauge.
- Overlay **Network → Authoritative Replica** (open by default): last-frame Enter/Update/Leave, session totals, Known without Update this frame. Header summary shows session `E / U / L` even when collapsed.
- Opt-in DEV probe: `PURGATORY_RUNTIME_PROBE=1|true|yes|on`. Default off. Map-ready does not spawn automatically.
- Protocol stays **v10**. No combat, skills, inventory, AI, ECS, extra threads, spatial tree, global event bus, or `RuntimeServices` bag.

Error containment:

```text
invalid command → typed reject
stale runtime target → controlled no-op/cancel
expired/cancelled timer → cannot fire twice
missing owner → cleanup
internal impossible invariant → debug_assert according to project rules
```

## Important files

- `crates/simulation/src/{scheduler,action,action_gate,command,runtime_event,effect,cadence,spawn_schedule,query,runtime,runtime_stats,phase6f_tests}.rs`
- `crates/simulation/src/{world,time,input_gate,lib}.rs`
- `apps/server/src/network/{gameplay,replication,stats,metrics_export,mod}.rs`
- `crates/common/src/load_metrics.rs`
- `apps/client/src/{replica,app,debug/overlay,debug/snapshot,camera_follow,local_presentation}.rs`
- `docs/{ARCHITECTURE,DECISIONS,PROTOCOL,ROADMAP,TEST_GATES,PERFORMANCE_BUDGETS}.md`, `README.md`, root `PHASE`
- ADRs 0045–0049 in `docs/DECISIONS.md`

## Tests added or changed

- Scheduler: future tick, same-tick sequence, cancel, stale TimerId, owner cancel, no same-pass recursion, deferred budget/progress/starvation, lane isolation, capacity reject, critical ceiling
- Actions/gate: start/complete, Busy without a second slot, TransitionLocked without a slot, cancel, no double-complete, owner despawn cleanup
- Command preamble: typed reject; Interact is not the exclusive Action slot
- Effects: expire, explicit remove then stale expiry no-op, target despawn
- Spawn schedule: delayed commit, cancel, fresh EntityId (not the despawned id)
- Staged events, cadence EveryN / stagger, filtered spatial query + address isolation + cap
- Replication: first observe Enter; unchanged Known no Update; mutation Update; two observers independent commits; leave then re-enter Enter
- Load metrics schema 3 default datagram still fits the 2048-byte cap
- Existing 6C / 6D / 6E tests kept

## Quality gate

Ran `./scripts/check.ps1` on 2026-08-30. Result: **PURGATORY quality gate OK**.

Commands actually executed, in order:

1. `cargo fmt --all -- --check` — pass
2. `cargo check --workspace` — pass
3. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — pass
4. `cargo test --workspace` — pass (including new 6F simulation tests and server replication observer-independence tests)
5. `cargo run -p purgatory-content-validator -q` — pass (`maps=2 entities=5 defs=7`)

Compiling is not completion; the full gate ran. Extended network soaks remain `#[ignore]` and were not run.

## Manual / runtime verification still required

1. Two clients: first see of a remote/generic is Enter; standing still does not spam Updates; walking out of AOI is Leave; walking back is a new Enter.
2. Overlay **Network → Authoritative Replica** (default open): last-frame Enter/Update/Leave, session totals, and “Known without Update this frame.” Idle ticks often show last-frame 0/0/0; session totals are the durable proof.
3. Camera: stand centered → walk inside the Dead Zone (camera still) → push the right edge (smooth contain, player stays at the right edge) → walk left across the box (camera still) → push the left edge (follow left). No recenter-to-center. No jump on edge crossings.
4. Idle local-player sprite should stay still (server pose is bit-stable; leftover replica velocity must not remainder-extrapolate).
5. With `PURGATORY_RUNTIME_PROBE=1`, one visible Generic appears after ~1 s; with the flag unset, map-ready does not spawn that entity.
6. Transition input barrier and 6E login/restore still behave as before.

Automated tests do not prove windowed two-client replication, probe visibility, or overlay timing.

## Deviations

- Deferred budget lives as scheduler constants (`DEFERRED_DRAIN_BUDGET`) rather than a separate `work.rs` facade.
- Overlay **Network → Authoritative Replica** is default-open so Enter/Update/Leave are visible without expanding a collapsed section. Last-frame counts are often 0 on idle ticks (no records); session totals accumulate.
- Camera Dead Zone is a free-movement box. The camera moves only to contain the player at the pushed edge (same exponential damper; no snap, no recenter-to-center).
- Remainder extra ignores speeds below `EXTRAPOLATE_MIN_SPEED` so leftover replica velocity cannot sawtooth an idle sprite.
- Portal / DevSetChannel still reject on the existing input-gated path and increment 6F command-reject counters there; InteractOpen uses the shared preamble as the primary gate.
- Critical ceiling delay can change gameplay timing under overload. That tradeoff is documented (ADR-0045) rather than turning Critical into a second deferred budget.

## Unresolved / risks

- Hitting `CRITICAL_DRAIN_CEILING` in production would delay correctness-timed work; it is an invariant/metric, not a silent gameplay policy.
- Cadence and scheduler are foundations only. Combat, skills, inventory, buffs-as-content, and NPC AI remain out of scope.
- Channel/Instance placement is still DEFAULT-only (6E).
- Load metrics schema 3 JSON must stay under `METRICS_MAX_DATAGRAM_BYTES` (2048) as field names grow.

Work stopped at Phase 6F. **Do not begin the next phase.**
