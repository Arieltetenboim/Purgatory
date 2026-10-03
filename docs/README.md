# PURGATORY docs — current authority

Rebaseline: **2026-10-03**.

This directory contains both current contracts and historical development evidence.
Do not treat every old Phase report as a statement about current `master`.

## Current snapshot

- Root phase marker: `12.12C`.
- Human-facing version: `0.12.A`.
- Network protocol: **v34**.
- Phase 12A, 12B, and 12C are accepted.
- The separate **Phase 12 exit remains open**.
- PostgreSQL is the only game database.
- Item Lab V1, Mob Lab drop authoring, and runtime monster loot are integrated.
- The two CI jobs that regressed on 2026-10-03 were traced to tests depending on editable authored content. PR #127 isolated those fixtures; both canonical Quality and PostgreSQL jobs passed in run `37142280327`. The remaining Item Lab closure evidence is manual and stays explicit in [the Item Lab closeout audit](ITEM_LAB_CLOSEOUT_2026-10-03.md).

## Read these for “what is true now”

1. Root [`PHASE`](../PHASE) — exact gameplay-phase marker.
2. Root [`README.md`](../README.md) — short current project entry point.
3. [`ROADMAP.md`](ROADMAP.md) — current development sequence and open gates.
4. [`ARCHITECTURE.md`](ARCHITECTURE.md) — current architecture.
5. [`DECISIONS.md`](DECISIONS.md) — accepted Architecture Decision Records (ADRs).
6. [`QUALITY.md`](QUALITY.md) — repository quality, testing, ownership, and refactoring policy.
7. [`TEST_GATES.md`](TEST_GATES.md) — validation evidence. Older entries keep the status that was true when they were recorded.
8. [`PERSISTENCE_AND_AUTHORING_CONTRACT.md`](PERSISTENCE_AND_AUTHORING_CONTRACT.md) — current saving/authoring boundary.
9. [`PHASE_12_POSTGRESQL_CONTINUITY_CONTRACT.md`](PHASE_12_POSTGRESQL_CONTINUITY_CONTRACT.md) and [`PHASE_12_ENTRY.md`](PHASE_12_ENTRY.md) — current Phase 12 contract and remaining exit boundary.
10. [`WIKI.md`](WIKI.md) — compact orientation only; it deliberately does not replace the sources above.

An **ADR (Architecture Decision Record)** records an important architectural decision and its rationale.

## Historical/reference documents

Phase reports, old plans, acceptance matrices, archived branch material, and dated research documents are evidence of what was implemented, tested, or believed at that time. They are intentionally **not rewritten** to pretend later decisions were already known.

Examples include:

- `PHASE_*_REPORT.md`
- older `PHASE_*_PLAN.md` documents
- dated verification/acceptance records
- `archive/`
- the root `PURGATORY_CURSOR_MASTER_EXECUTION_PLAN.md`

When historical text conflicts with the current snapshot, current code/tests and the canonical documents above win.

## Evidence order

For a current claim, prefer:

1. reproducible runtime behavior
2. current tests
3. current code
4. current canonical docs / ADRs
5. recent relevant Git history
6. historical reports
7. conversation memory or assumptions

If a fact cannot be established from those sources, report it as unknown rather than filling the gap.

## Documentation update rule

- Update current-state documents when behavior or policy changes.
- Preserve historical reports unless correcting an actual factual/typographical error in that historical record.
- Do not copy the same mutable status into many files.
- Put durable architecture in `ARCHITECTURE.md` / `DECISIONS.md`, current sequencing in `ROADMAP.md`, and test evidence in `TEST_GATES.md`.
- Keep this index and `WIKI.md` small.
