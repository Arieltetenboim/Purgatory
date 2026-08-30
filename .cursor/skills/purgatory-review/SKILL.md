---
name: purgatory-review
description: Reviews PURGATORY changes for correctness, architecture, performance, maintainability, test quality, and documentation consistency. Reports findings by severity and does not modify code unless explicitly asked to fix. Use when reviewing PURGATORY code, a phase implementation, a diff, uncommitted changes, or a pull request.
---

# PURGATORY review

Rigorous engineering review. Complements [`.cursor/rules/`](../../rules/). Follow those rules and repository documentation. Do not duplicate them or copy volatile constants into this skill.

**Default: REVIEW and REPORT.** Do not modify code unless the user explicitly asks to fix findings.

If a finding cannot be resolved from repository evidence, report the decision point. Do not pretend there is one obvious answer, and do not silently pick an architecture.

## Scope

Review the requested change set (diff, phase, files, or PR). Inspect relevant existing code, tests, ADRs, and docs that the change touches.

## Review coverage

### Correctness

- logic errors
- edge cases
- regressions
- state consistency
- lifecycle issues
- error handling
- unsafe assumptions

### Architecture

- violations of documented boundaries
- duplicated or parallel systems
- ownership problems
- incorrect responsibility placement
- protocol/runtime contract drift
- divergence from ADRs or project decisions
- premature abstractions

### Performance / efficiency

Pay particular attention to runtime-sensitive and per-tick code.

Look for:

- unnecessary allocations
- unnecessary cloning/copying
- repeated work
- avoidable serialization/deserialization
- inefficient iteration/query patterns
- accidental O(n²) or worse behavior
- excessive work in hot paths
- unnecessary synchronization/locking
- poor data access patterns
- network bandwidth waste
- excessive snapshot/update work
- avoidable temporary structures
- scaling risks

Do not label something a performance problem merely because another implementation looks theoretically faster.

Distinguish:

- measured/clear performance problems,
- credible scaling risks,
- speculative optimization opportunities.

Prefer evidence.

### Maintainability / code cleanliness

Look for:

- duplicated logic
- confusing ownership
- unclear naming
- oversized functions/modules where decomposition would materially improve reasoning
- unnecessary complexity
- dead or obsolete paths
- fragile coupling
- hidden side effects
- inconsistent error handling
- abstractions that obscure rather than clarify
- comments that contradict implementation
- tests that are difficult to understand or maintain

Do **not** recommend refactoring solely for aesthetics. Do not optimize for "clever" or maximally abstract code. Prefer readable, explicit Rust consistent with the existing project.

### Test quality

Review:

- whether important behavior is actually tested;
- regression coverage;
- edge cases;
- whether tests verify meaningful contracts rather than implementation accidents;
- whether runtime/manual testing is still required.

### Documentation consistency

Check relevant documentation and ADRs where the change affects them.

## Priority

Classify findings by practical importance.

| Severity | Meaning |
|---|---|
| **CRITICAL** | Likely correctness, data integrity, security, protocol, or major architectural failure |
| **HIGH** | Significant bug, regression, scalability problem, or architectural violation |
| **MEDIUM** | Meaningful maintainability, efficiency, testing, or design concern |
| **LOW** | Minor improvement with limited practical impact |

Do not flood the report with cosmetic LOW findings. Focus on issues that materially improve PURGATORY.

For each significant finding include:

- location
- issue
- why it matters
- evidence/reasoning
- recommended direction

Where useful, distinguish:

- **MUST FIX BEFORE CONTINUING**
- **SHOULD FIX**
- **DEFER / MONITOR**

## Decision policy

If the reviewer discovers an issue but cannot determine the correct resolution from repository evidence:

Do not pretend there is one obvious answer.

Explain:

- the uncertainty,
- the available options,
- relevant trade-offs,
- recommended direction if justified.

If the user asked only for review, report the decision point rather than modifying code.

Ask at most one focused decision question at a time. Do not dump speculative questions. Do not ask about issues that inspecting the repository, docs, code, or tests can already answer.

## Anti-overengineering

Do not accumulate complexity merely because a theoretically more sophisticated solution exists.

Prefer the simplest solution that:

- satisfies current requirements,
- respects architecture,
- remains testable,
- meets known performance requirements,
- does not create obvious future blockers.

Do not introduce or recommend:

- speculative extensibility,
- unnecessary frameworks,
- unnecessary dependencies,
- generic systems without a current use case,
- major refactors without demonstrated value.

Performance optimization should be driven by known hot paths, measurements, clear algorithmic problems, or credible scaling analysis. Treat speculative "faster" alternatives as LOW or omit them.

## Report shape

1. Summary (what was reviewed; overall risk)
2. Findings (CRITICAL → LOW; skip empty cosmetic noise)
3. Unresolved decisions (if any)
4. Tests / docs / manual verification still required
5. What was **not** changed (unless the user asked to fix)
