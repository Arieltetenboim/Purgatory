# PURGATORY — Project Context & Development Doctrine

## Purpose

This is durable context for AI-assisted development of PURGATORY. It describes how the project should be reasoned about, investigated, and evolved. It is **not** authoritative for current Phase status, exact file layout, protocol version, or completed features; inspect the current repository and canonical docs for those facts.

## Project identity

PURGATORY is a custom-built native 2D side-scrolling MMORPG in Rust. It deliberately avoids general-purpose game engines. The project prioritizes server authority, explicit ownership, correctness, maintainability, scalability, and measurable behavior over rapid feature accumulation.

## Foundational direction

- Rust stable, edition 2024, Cargo workspace.
- Authoritative fixed-tick server simulation.
- Native `winit` + `wgpu` client.
- QUIC networking through Quinn.
- Authored content is separated from simulation/runtime state.
- Runtime entity identity is generational where stale-reference safety matters.
- Replication carries semantic gameplay state, not bones/frames/art transforms.

Exact crate/dependency structure must be read from the current repository.

## Server authority

Gameplay truth belongs to the authoritative simulation. Clients may collect input, predict eligible local behavior, interpolate remotes, and maintain presentation-only state, but prediction is reconciliation—not a second gameplay authority. Entity existence, combat outcomes, authoritative equipment/state transitions, world address, and persistence transitions remain server-owned where applicable.

## Simulation vs presentation

**Gameplay state describes what is happening. Presentation determines how it looks.**

Simulation should not know sprites, bones, animation frames, texture coordinates, rendering APIs, or UI. Presentation consumes semantic runtime state. Do not replicate animation frames or art transforms over the network.

## Authored content is data

Animation files, character sheets, maps, effects, visual timing, and sprite offsets are expected to change. Tests should protect parsers, runtime semantics, ownership, invariants, lifecycle, transformations, and failure behavior—not arbitrary art values unless they are explicit product contracts.

## Ownership before abstraction

Before adding a manager/service/crate/helper/runtime object, ask:

1. Who owns this lifecycle now?
2. Must this state transition atomically with another owner?
3. Does the proposed component have a genuinely independent lifecycle?
4. Does extraction reduce coupling or merely relocate it?
5. Does an existing abstraction already represent the responsibility?

Large files are not automatically architecture problems. A large authority/composition root can be correct when multiple transitions must remain atomic.

## Preserve atomic gameplay ownership

State that changes atomically with the authoritative world/player lifecycle may correctly remain near that lifecycle even when its domain name suggests another module. The useful question is: **must this state change atomically with the authoritative entity/player lifecycle?**

## Dependency direction follows ownership

Lower-level runtime must not depend on authored formats or presentation concerns. Simulation remains independent of JSON/content file formats, rendering, UI, OS window/runtime APIs, client presentation, and network-runtime implementation. Translation from authored content into simulation-native structures may legitimately happen above simulation.

## Reuse foundations

Before adding feature-local timers, event buses, entity-reference formats, spatial lookups, dirty-state systems, or action lifecycles, search for the existing project primitive. Extend proven foundations unless concrete requirements show they are unsuitable.

## Networking and scale

Bandwidth and server work are first-class concerns. Prefer AOI/relevance filtering, per-observer visibility, Enter/Update/Leave semantics, dirty/revision tracking, deltas, cadence tiers, priorities/budgets, baseline/resync handling, bounded queues, semantic state, and justified quantization.

Do not hide pressure by blindly increasing a cap. Measure the failure mode, identify the owner, decide whether the limit or producer is wrong, and change the limit only with evidence.

## Scalability is measured behavior

Do not redesign for hypothetical MMO scale without evidence. Keep hot paths bounded, add observability, run controlled load tests, find where behavior bends, and fix demonstrated bottlenecks. A successful localhost load run is a baseline, not a production-capacity claim.

## Development slices

Preferred pattern:

**Phase goal → focused slice → one behavior proven end-to-end.**

Trace: `input/event → authoritative owner → transformation → consumer → observable result`.

Each slice should have explicit scope/non-goals, acceptance criteria, the smallest necessary tests, preservation of previous GREEN behavior, and a stop boundary.

## Closed phases are foundations

A GREEN phase is established unless new evidence reveals a regression, incorrect assumption, incompatible future requirement, or measurable correctness/scale problem. Do not reopen architecture because a later implementation could look cleaner. Identify the actual owner of a failure before expanding scope.

## Investigation discipline

Start with 1–3 falsifiable hypotheses. Prefer: current failure → relevant tests → symbol/text search → direct owner → direct consumers → recent relevant Git diff/history → adjacent subsystem only if evidence requires it. Search before reading large files. Stop exploring eliminated branches.

## Coding-agent workflow

Use narrowly bounded tasks with objective, known behavior, likely owner, required context, constraints, non-goals, acceptance criteria, proportional tests, and stop/escalation conditions. Stable architecture belongs in repository docs, not giant repeated prompts.

Fresh chats are usually safer for new independent tasks. Escalate model/reasoning cost only for evidence-backed ambiguity such as architecture conflicts, concurrency/network failures, subtle state machines, or high-impact design decisions.

## Testing and GREEN

Preferred order: closest behavior/unit test → affected crate/package → wider workspace when justified → canonical repository gate for integration/closeout. Automated GREEN does not prove visual quality, animation quality, meaningful network load behavior, launcher/process lifecycle, or long-running stability. Use evidence appropriate to the claim.

## Failure and lifecycle behavior

For runtime systems, explicitly consider disconnect, despawn, cancellation, shutdown, map transition, stale references, malformed content, queue pressure, missing resources, persistence failure, and interrupted operations. Fallbacks should be deliberate, observable, and semantically safe.

Runtime references must not silently survive entity reuse. Ask who removes scheduled work, effects/actions/events, and entity-associated state on despawn/disconnect/world transition.

## World/view semantics

Map/channel/instance membership is gameplay/world semantics. Presentation reacts to transitions; it does not define them. Display resolution/render scale must not silently increase gameplay visibility. Any future zoom is an intentional view/gameplay policy.

## Character and animation presentation

Character appearance is layered presentation. Equipment and visual attachments are not necessarily one-to-one. Draw order/remapping should be explicit; source-sheet coordinates are not world transforms. Skeleton/body slots and gameplay equipment slots are different concepts.

Animation runtime needs stable clip/track/sampling/playback/pose semantics. Animation Lab is an authoring tool; tool roughness does not by itself invalidate the shared runtime.

## Combat and placeholders

Combat should compose existing runtime primitives rather than become one monolithic owner. Placeholder visuals/behavior are not evidence that runtime architecture is wrong; conversely, attractive visuals do not prove runtime soundness.

## Issues and evidence

Create GitHub Issues for bounded actionable work with an owner/scope and acceptance criteria—not every speculative idea. When deciding what is true, prefer:

1. executable/reproducible behavior
2. current tests
3. current code
4. canonical repo docs and ADRs
5. recent relevant Git history
6. Phase reports
7. AI conversation memory
8. assumptions

Separate observed/proven facts from inference and unknowns.

## Completion reporting

Default implementation report: **Root cause / Changed / Tests / Remaining risk**. Keep it compact unless a longer record adds durable architectural value.

## Refactoring discipline

Do not opportunistically clean unrelated code, broadly rename APIs, reorganize modules for aesthetics, replace working abstractions, migrate unrelated tests, or build generalized frameworks for one current use. Make the smallest correct repair. Doing nothing is valid when investigation shows current architecture is already correct.

## Current-state rule

Before planning or implementing: inspect root `PHASE`, the relevant `ROADMAP` section, architecture/ADR/test docs, current code, and recent relevant Git changes when useful. Do not reopen decisions explicitly recorded as settled unless new evidence contradicts them.

## Core doctrine

**Search narrowly → identify owner → prove behavior → patch locally → test proportionally → record durable decisions → stop.**

The goal is the smallest architecture that correctly supports known requirements and leaves rational extension paths—not the most architecture.
