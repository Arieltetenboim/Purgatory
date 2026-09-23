# PURGATORY — Repository Quality & Architecture Policy

This document is the concise repository-level policy for quality, ownership, testing, and refactoring. Current implementation facts still come from code, tests, canonical docs, ADRs, and Git history.

## Quality gates

- Windows canonical gate: `./scripts/check.ps1`.
- Linux / macOS canonical gate: `./scripts/check.sh`.
- CI runs the canonical gate and is a safety net, not a replacement for proportional local validation.
- Every meaningful code change should pass the smallest relevant test first, then the affected crate/package tests.
- Run the full repository quality gate when justified by scope and before important integration/push.
- Warnings, formatting failures, test failures, content-validation failures, and compilation failures are not GREEN.

## Tests

- Tests protect runtime behavior, contracts, invariants, lifecycle, and ownership boundaries.
- Stable behavior belongs in owner-oriented behavior/integration tests, not ad-hoc Phase-named suites.
- Closed Phase tests should graduate naturally when touched; do not mass-rename or reorganize historical tests merely to rewrite history.
- Preserve meaningful cross-owner integration tests.
- Authored content—animations, sprites, offsets, VFX timing, presentation values, and similar art data—is mutable data, not a golden behavioral fixture.
- Do not freeze artistic values unless the exact value is an explicit gameplay/product contract.
- Prefer synthetic fixtures when validating engine/runtime semantics.

## Architecture ownership

- Do not refactor because a file is large.
- A hotspot requires both meaningful change pressure and evidence of mixed or incorrect ownership.
- Large composition/authority roots are acceptable when they coordinate responsibilities that must remain atomic.
- Before extracting state or behavior, prove that it has an independently meaningful lifecycle and an existing or clearly justified owner.
- Do not split one lifecycle into multiple maps/managers unless the separation reduces real coupling rather than adding synchronization bookkeeping.
- Prefer existing owners and abstractions over new managers, crates, helpers, services, or parallel concepts.

### GameplayOwner / gameplay.rs

- `GameplayOwner` remains the authoritative owner of `World` and player-binding lifecycle.
- `ConnectionId -> EntityId`, spawn/despawn, authoritative player lifecycle, input-epoch transitions, and state that must change atomically with `World` belong with that owner.
- `PlayerBinding` may contain subsystem state when it has the same lifecycle and must transition atomically with the binding.
- Do not extract persistence or replication state merely because those domains have separate modules.
- For a proposed responsibility, ask: **must this change atomically with `World` or `PlayerBinding`?** If yes, ownership there may be correct. If no, search for the existing domain owner first.

## Dependency boundaries

- Dependency direction follows ownership, not diagram symmetry.
- `content -> simulation` is intentional when validated authored data is translated into simulation-native runtime plans.
- `simulation` remains independent of content files/JSON, UI, rendering, OS runtime, and networking runtime.
- Platform-specific dependencies in shared crates require an intentional, isolated, target-gated responsibility.
- Do not create crates merely to make a dependency graph look cleaner.

## Refactoring

- Refactor only with concrete evidence of maintenance pressure, incorrect ownership, repeated unrelated churn, duplicated lifecycle, correctness risk, or a violated contract.
- A cleaner-looking API or diagram is not sufficient justification.
- Prefer a small probe before structural change.
- If the probe shows current ownership is correct, stop.
- Reverting an unjustified refactor is a successful outcome.
- Do not fix unrelated issues during focused feature work.
- New dependencies or new architectural owners require an explicit rationale in an ADR or PR description.

## Growth discipline

- Search narrowly and use Git diff/history/blame to understand repeated-change hotspots.
- Distinguish **large but cohesive**, **active but correctly owned**, and **actionable mixed ownership**.
- Do not use LOC thresholds, module-count targets, or abstraction count as quality metrics.
- Improve architecture only where evidence shows future change cost or correctness risk will decrease.

## Historical documents

- Preserve historical Phase reports as historical artifacts. Do not rewrite them to match later refactors.
- Update current-state documents instead of laundering new facts into old reports.

## Core principle

**Protect behavior and ownership, not aesthetics. Do not refactor working architecture without evidence that the new boundary is better than the existing one.**
