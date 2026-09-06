# AGENTS

This repository is **PURGATORY**: a custom-built native 2D side-scrolling MMORPG (Rust Cargo workspace: headless server, native client, shared crates, tools). Server-authoritative. Custom engine — not Unreal, Unity, Godot, or Bevy.

Repository **code and documentation** are authoritative. Do not invent project facts.

Permanent project rules: [`.cursor/rules/`](.cursor/rules/). Follow them. Do not copy them here.

Scoped workflows: [`.cursor/skills/`](.cursor/skills/) (`purgatory-implement`, `purgatory-review`).

## Documents (consult when relevant)

- `PURGATORY_CURSOR_MASTER_EXECUTION_PLAN.md` — master execution specification
- `README.md` — workspace map, current status, how to run
- `docs/ARCHITECTURE.md` — structural rules and ownership
- `docs/DECISIONS.md` — Architecture Decision Records
- `docs/PROTOCOL.md` — wire protocol
- `docs/ROADMAP.md` — phase order and status
- `docs/TEST_GATES.md` — verification gates
- `docs/QUALITY.md` — repository quality policy
- `docs/PERFORMANCE_BUDGETS.md` — living performance budgets
- `docs/CONTENT_PIPELINE.md` — content authoring boundary
- `docs/DIAGNOSTICS_ARCHITECTURE.md` — client debug/diagnostics split (D-track)
- root `PHASE` — current phase marker

## Working expectations

- Inspect existing implementation, callers, and tests before modifying them.
- Honor the requested phase/sub-phase boundary. Do not start the next phase until instructed.
- Respect the project-defined quality gate (`./scripts/check.ps1` or `./scripts/check.sh`). Compiling is not completion.
- Surface architectural uncertainty rather than silently resolving it.
