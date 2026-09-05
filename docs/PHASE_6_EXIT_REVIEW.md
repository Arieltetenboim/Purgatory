# Phase 6 Exit Review

This review is the Phase 6G stop condition. It does **not** start Phase 7.

**Owner close (2026-09-01):** **6G = GREEN — architecture closed, production policy tuning deferred.** Architectural chain: change-driven → localized AOI → dirty fan-out → observer-specific policy → priority/cadence/budget. Evidence and known deferred tuning: [`docs/PHASE_6G7C_REPORT.md`](PHASE_6G7C_REPORT.md).

## Hard correctness (FAIL if broken)

Covered by in-process and harness classification, not by invented performance thresholds:

- Duplicate active Character is rejected (`AlreadyConnected` / occupancy).
- Cross-`WorldAddress` query/visibility isolation.
- Stale `EntityId` does not address a reused slot.
- Newer persistence revision is not overwritten by stale saves.
- Shutdown drain of pending saves in an isolated directory (never `%LOCALAPPDATA%\Purgatory` in tests).
- Observer A commit does not clear observer B (6D/6F contract; not re-litigated here).
- Panic / state corruption on malformed persist or load-validation JSON is a fail.

Performance numbers without existing budgets remain **observations/baselines**.

## Generic runtime spine vs Player/Character

Verified in `phase6g_tests` (and existing 6F tests). Do **not** implement a MOB.

| Assumption to challenge | Evidence |
|---|---|
| Entity runtime ≠ Character runtime | Generic spawn/action/effect/query with no `CharacterId` |
| Action owner ≠ necessarily network Player | `try_start_action` on Generic + `ActionGateContext::in_world` |
| Scheduler owner ≠ necessarily Character | `ScheduleOwner::World` and `ScheduleOwner::Entity` |
| Runtime event producer ≠ necessarily client command | `RaiseEvent` / cadence fire |
| Replication entity ≠ necessarily Player | visible Generic in `replicated_in_address` |

A future MOB must consume this spine without a fake account/session/Character.

## What 6G proved automatically

- Load-mode-gated synthetic pressure does not run on a default production server env.
- Same-login reconnect tracks `EntityId` change from replica; Welcome still has no `CharacterId`.
- Isolated persist + failure injection stay in temp/run dirs.
- Critical ceiling remains 1024; remainder carries; tests do not lower it.
- MixedRuntime is the named canonical workload (`--preset mixed`).
- CLI owns validation semantics; Developer Tools forwards argv/env (`--print-server-env`).

## What remains evidence / manual

- ~30 minute Mixed soak (configurable; not a magic architecture number).
- Scale ladder with recorded bot **and** synthetic counts; start from 5.7/6D 10/25/50.
- Windows: Stop owned server process so cargo can replace `purgatory-server.exe` (workspace-path identified stop, not machine-wide `taskkill`).
- Ready vs scenario health: artifacts use `failure_class` (`timeout`, `correctness`, `interrupted`, …). Ready must not be turned into a red blob that hides the class.
- Open 6B–6F client presentation checks.

## Stop rule (refinement 18)

Stop now: correctness gates in CI, cleanup tests, Mixed/scale **characterized as the harness and in-process suite**, soak **not required inside `cargo test`**, major queues have intentional policies, this review finds **no core rewrite blocker**. Phase 6 architecture is closed.

Measurable production-policy tuning (cadence/budget thresholds, slow-client degradation) is deferred to Phase 7.4 and must not reopen the Phase 6 replication architecture. Representative gameplay workload is Phase 7.2, not a reason to delay instrumentation (7.1).

## Phase 7 is not started

No MOB product, combat system, skills, inventory, SQL, ECS, or distributed architecture was added in 6G. Post-6G Phase 7 is **capacity, parallelism & production scaling** ([`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md)), not gameplay vocabulary and not a second AOI/replication-architecture phase.
