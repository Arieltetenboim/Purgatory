---
name: purgatory-implement
description: Implements a scoped PURGATORY phase, sub-phase, feature, or bug fix—inspect existing code and docs, apply the smallest coherent change, add focused tests, run the project quality gate, then stop at the requested boundary. Use when implementing PURGATORY work, executing a phase or sub-phase, applying an implementation plan, or fixing a bug in this repository.
---

# PURGATORY implement

Reusable workflow for a scoped implementation or bug fix.

This skill **complements** [`.cursor/rules/`](../../rules/). Follow those rules and repository documentation. Do not duplicate them, override them, or copy volatile project constants into this skill.

Do **not** automatically redesign architecture or expand scope.

## Workflow

```text
UNDERSTAND → INSPECT → IMPLEMENT → TEST → QUALITY GATE → REPORT → STOP
```

### 1. UNDERSTAND

Read the user's requested scope carefully. If a detailed implementation plan is provided, treat it as the intended scope. Do not redesign it casually.

### 2. INSPECT

Inspect relevant repository documentation and existing code before editing.

Identify, where relevant: existing contracts, invariants, callers, consumers, and tests.

If uncertain because not enough code has been inspected yet, inspect more code first. Do not ask the user as a substitute for investigation.

### 3. IMPLEMENT

Implement the smallest coherent solution that satisfies the requested scope.

Preserve existing architecture and unrelated behavior. Reuse existing types, utilities, and systems. Do not opportunistic-cleanup outside scope.

### 4. TEST

Add or update focused tests for new behavior and important regressions.

### 5. QUALITY GATE

Run the repository-defined quality gate (see `.cursor/rules/` and `docs/TEST_GATES.md`). Do not invent a replacement gate. Compiling is not completion.

### 6. REPORT

State:

- implemented scope
- important files/modules changed
- tests added or changed
- quality-gate results (commands actually run)
- manual/runtime verification still required
- deviations, risks, or unresolved issues
- confirmation that work stopped at the requested phase/scope boundary

Passing automated tests does not prove visual, multiplayer, timing, networking, or runtime behavior when manual observation is part of acceptance.

### 7. STOP

Stop at the requested phase/scope boundary. Do not begin the next phase. Do not pre-build future systems.

## Decision / question policy

Exercise normal engineering judgment for ordinary local implementation details.

Do **not** ask the user questions that can be answered reliably by:

- inspecting the repository,
- reading project documentation,
- tracing existing code,
- examining tests,
- or following an established project convention.

**MUST pause and ask** a concise, focused question when a material uncertainty cannot be safely resolved from existing project evidence.

Examples include:

- ambiguous product/game behavior;
- conflicting requirements;
- contradictory architecture/documentation with no clear authoritative resolution;
- multiple viable architectural solutions with meaningful trade-offs;
- a required change to protocol semantics;
- a required change to persistence or identity models;
- security-sensitive decisions;
- a new major dependency or technology choice;
- a substantial performance-vs-complexity trade-off;
- scope expansion beyond the requested phase;
- a runtime problem whose root cause remains uncertain after reasonable investigation;
- a bug where multiple fundamentally different fixes are plausible and choosing incorrectly could create architectural debt or regressions;
- destructive or difficult-to-reverse changes.

When asking:

1. Explain briefly what was discovered.
2. Explain why the decision cannot be safely inferred.
3. Present the realistic options when known.
4. Explain the important trade-offs.
5. Give a recommendation when evidence supports one.
6. Ask one focused question needed to continue.

Do not dump a large list of speculative questions. Prefer one decision point at a time. Do not use questions as a substitute for repository investigation. Do not ask permission for trivial implementation choices.

## Failure / debugging policy

When implementation or testing encounters a failure:

Do not immediately patch around the symptom.

First investigate:

- reproduction,
- relevant logs/errors,
- execution path,
- ownership/state transitions,
- related tests,
- recent assumptions,
- likely root cause.

If a root cause can be established with reasonable confidence, fix it within scope and verify the fix.

If the root cause remains genuinely uncertain, or the available fixes have materially different architectural consequences, **STOP and ask the user** rather than guessing.

A green quality gate after a workaround is not sufficient evidence that the underlying problem is correctly solved.

## Anti-overengineering

Do not accumulate complexity merely because a theoretically more sophisticated solution exists.

Prefer the simplest solution that:

- satisfies current requirements,
- respects architecture,
- remains testable,
- meets known performance requirements,
- does not create obvious future blockers.

Do not introduce:

- speculative extensibility,
- unnecessary frameworks,
- unnecessary dependencies,
- generic systems without a current use case,
- major refactors without demonstrated value.

Performance optimization should be driven by known hot paths, measurements, clear algorithmic problems, or credible scaling analysis.
