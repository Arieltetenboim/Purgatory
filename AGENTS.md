# AGENTS

This repository is **PURGATORY**: a custom-built native 2D side-scrolling MMORPG (Rust Cargo workspace: headless server, native client, shared crates, tools). Server-authoritative. Custom engine — not Unreal, Unity, Godot, or Bevy.

Current runtime behavior, automated tests, and code take precedence. The GitHub Wiki is the human documentation authority; accepted decisions and historical reports are subordinate evidence. Do not invent project facts.

Permanent project rules: [`.cursor/rules/`](.cursor/rules/). Follow them. Do not copy them here.

Scoped workflows: [`.cursor/skills/`](.cursor/skills/) (`purgatory-implement`, `purgatory-review`).

## Documents (consult when relevant)

- [Historical Archive](https://github.com/Arieltetenboim/Purgatory/wiki/Historical-Archive) — original bootstrap execution plan (historical)
- [Engineering Policy](https://github.com/Arieltetenboim/Purgatory/wiki/Engineering-Policy) — durable development/AI reasoning doctrine; current repo evidence still wins
- `README.md` — workspace map, current status, how to run
- [Architecture](https://github.com/Arieltetenboim/Purgatory/wiki/Architecture) — structural rules and ownership
- [Decisions and ADRs](https://github.com/Arieltetenboim/Purgatory/wiki/Decisions-and-ADRs) — Architecture Decision Records
- [Protocol](https://github.com/Arieltetenboim/Purgatory/wiki/Protocol) — wire protocol
- [Roadmap](https://github.com/Arieltetenboim/Purgatory/wiki/Roadmap) — phase order and status
- [Testing and Quality](https://github.com/Arieltetenboim/Purgatory/wiki/Testing-and-Quality) — verification gates
- [Testing and Quality](https://github.com/Arieltetenboim/Purgatory/wiki/Testing-and-Quality) — repository quality policy
- [Diagnostics and Presentation](https://github.com/Arieltetenboim/Purgatory/wiki/Diagnostics-and-Presentation) — living performance budgets
- [Content and Authoring](https://github.com/Arieltetenboim/Purgatory/wiki/Content-and-Authoring) — content authoring boundary
- [Content and Authoring](https://github.com/Arieltetenboim/Purgatory/wiki/Content-and-Authoring) — saving rules for authored content and durable gameplay state
- [Diagnostics and Presentation](https://github.com/Arieltetenboim/Purgatory/wiki/Diagnostics-and-Presentation) — client debug/diagnostics split (D-track)
- root `PHASE` — current phase marker

## Working expectations

- Inspect existing implementation, callers, and tests before modifying them.
- Honor the requested phase/sub-phase boundary. Do not start the next phase until instructed.
- Respect the project-defined quality gate (`./scripts/check.ps1` or `./scripts/check.sh`). Compiling is not completion.
- Changes that write authored content or durable gameplay state follow [Content and Authoring](https://github.com/Arieltetenboim/Purgatory/wiki/Content-and-Authoring) and include a Persistence Impact note. Do not copy that contract here.
- Surface architectural uncertainty rather than silently resolving it.


## Branch policy

- Work from current `master` unless a task explicitly requires a temporary branch.
- `master` is canonical; root `VERSION` carries the human-facing master version.
- `DEVELOPMENT` is a periodic stable checkpoint only. Do not develop independently on it or automatically fast-forward it after every master commit.
- Delete temporary feature/fix/salvage branches after their useful work is merged, salvaged, or proven superseded.
- Before deleting a divergent branch, prove its unique semantic changes are either present on `master` or intentionally rejected/superseded.
