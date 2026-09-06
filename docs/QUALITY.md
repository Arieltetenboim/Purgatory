# PURGATORY — Repository Quality Policy (v1)

This document records the repository-level quality policy for PURGATORY. It is deliberately concise and prescriptive.

- Canonical quality gate
  - Windows: `./scripts/check.ps1`
  - Linux / macOS: `./scripts/check.sh`
  - CI runs the canonical gate (`.github/workflows/quality.yml`).

- Failures
  - Warnings are treated as failures (Clippy `-D warnings`).
  - The canonical gate fails fast on the first failing command.

- Before pushing
  - The canonical gate must pass locally before pushing.
  - Local validation scope may be proportional: prefer running the closest package-level/unit tests during iterative development, but run the full canonical gate before push.

- Tests and history
  - Stable contracts belong in owner-oriented behavior/integration tests, not ad-hoc Phase-named suites.
  - Do not mass-rename historical tests to rewrite history. Tests graduate naturally when touched.
  - Keep integration tests that validate meaningful cross-owner composition.

- Changes and ownership
  - New dependencies or new architectural owners require an explicit rationale recorded in ADRs or a PR description.
  - Do not fix unrelated issues during focused feature work.

- Large files and refactors
  - Large file size alone is not a refactor trigger; repeated change across mixed ownership is a stronger signal.

- Historical reports
  - Preserve historical Phase reports as historical artifacts. Do not rewrite them to match later refactors.

- Authored content is mutable
  - Authored content (animations, sprites, VFX timings, presentation data) is mutable data, not a golden fixture.
  - Tests must validate schema, loading, runtime compatibility, and explicit invariants, but must not freeze artistic values (exact rotations, timing, or choreography) unless those values are an explicit product contract.

