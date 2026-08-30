# PURGATORY — MASTER EXECUTION PROMPT
## Phase 6.0 → Phase 6A → Phase 6B
### Runtime Foundation, Runtime Model, Interaction + UI Runtime

You are continuing the PURGATORY project after Phase 5 networking.

This is a LONG AUTONOMOUS EXECUTION TASK.

Work sequentially through:

1. **Phase 6.0 — Runtime Foundation & Replication Contracts**
2. **Phase 6A — Runtime Model**
3. **Phase 6B — Interaction + UI Runtime**

Do not begin Phase 6C.

Do not ask for approval between substeps unless execution is genuinely blocked by missing information that cannot be inferred safely from the repository and project documentation.

Prefer conservative, explicit architecture over speculative abstraction.

Preserve all established architectural constraints.

---

# PROJECT CONTEXT — NON-NEGOTIABLE

PURGATORY is a custom-built 2D side-scrolling MMORPG.

Core architecture already established:

- Rust stable, edition 2024
- Cargo workspace
- authoritative server
- simulation at 30 Hz
- native client using winit/wgpu
- Quinn/QUIC networking
- content represented outside gameplay code where appropriate
- no Unity / Godot / Bevy / Unreal
- server owns authoritative gameplay state
- client presentation/interpolation is not authoritative gameplay state
- existing networking phases must remain intact
- Phase 5 introduced replicated world state, remote interpolation, load testing, telemetry and networking correctness work

Do not break existing behavior to make Phase 6 easier.

Do not remove tests or weaken quality gates.

Do not silently replace established concepts with a new framework.

---

# PHASE 6 PURPOSE

Phase 6 converts the post-networking codebase into a real game runtime on which:

- players
- MOBs
- NPCs
- interaction
- UI
- inventory
- combat
- maps
- persistence
- runtime services

can later be built consistently.

This phase must prevent future systems from directly coupling themselves to:

- raw networking internals
- database identities
- global world scans
- client presentation state
- full-world broadcast replication
- concrete entity subclasses

The runtime must become the stable architectural boundary.

---

# CRITICAL REPLICATION PRINCIPLE

Replication efficiency is a top-level architectural requirement, not a later optimization.

Future gameplay systems MUST NOT assume:

> send full state of everything to everyone every tick.

The architecture must support the following pipeline:

World partition
→ WorldAddress
→ candidate set
→ visibility / AOI
→ relevance
→ dirty/change tracking
→ priority
→ update-frequency policy
→ delta-capable representation
→ byte/packet budget
→ replication frame

Not every optimization above must be fully implemented in this task.

However, the contracts and ownership boundaries MUST make these possible without rewriting gameplay systems later.

Networking must consume runtime replication decisions; gameplay systems must not manually push arbitrary packets.

---

# IMPORTANT TERMINOLOGY

When introducing a new type or subsystem, document its responsibility clearly.

Keep these identities conceptually separate:

### Runtime Entity ID
Temporary identity of a live runtime entity.

Lifetime:
current runtime / server process / entity generation.

Must not implicitly become a database ID or content-template ID.

### Persistent ID
Stable identity of persistent domain data such as a character/account-owned object.

May outlive a server process.

### Content ID
Stable identity of authored content/template definitions.

Example:
`mob.goblin.basic`

A runtime entity may have:

- Runtime ID only
- Runtime ID + Content ID
- Runtime ID + Persistent ID
- Runtime ID + Persistent ID + Content ID

depending on the entity.

Never conflate these identity classes.

---

# EXECUTION RULES

For every phase:

1. Inspect the current implementation before modifying it.
2. Reuse existing abstractions where correct.
3. Refactor only when necessary to establish the required boundary.
4. Add tests before declaring a gate green.
5. Update project documentation and architectural decisions.
6. Run the full quality gate.
7. Produce a concise phase report.
8. Continue automatically to the next requested phase if GREEN.
9. If NOT GREEN because of a real correctness problem:
   - diagnose it
   - fix it if within scope
   - rerun the gate
10. Do not proceed with a known broken foundation.

Do NOT:
- perform speculative performance optimization
- redesign QUIC
- change simulation tick rate
- raise networking limits merely to hide pressure
- implement combat
- implement inventory gameplay
- implement MOB AI
- implement quest systems
- implement persistence/database storage
- implement full map content systems
- implement Paper Doll rendering
- begin Phase 6C

---

# ============================================================
# PHASE 6.0 — RUNTIME FOUNDATION & REPLICATION CONTRACTS
# ============================================================

## Goal

Create the foundational contracts that every later Phase 6 runtime subsystem depends on.

At the end of 6.0 the server must have explicit concepts for:

- runtime identity
- content identity
- persistent identity boundary
- world membership/address
- runtime lifecycle
- runtime query boundary
- visibility/relevance contract
- replication contract
- presentation separation
- debug observability

This phase should be foundational, small enough to reason about, and extensively tested.

---

## 6.0.1 — WorldAddress

Introduce a first-class `WorldAddress` concept.

It must represent logical world placement without requiring future systems to infer placement from entity position.

Design it so it can represent at minimum:

- world / realm-like scope if required
- map
- channel
- instance

Do not overfit exact gameplay naming if current docs already define equivalents.

Required semantics:

- entities in different incompatible addresses are not automatically mutually visible
- changing address is a distinct runtime operation
- address equality / compatibility rules are explicit
- address is independent from X/Y transform
- address is usable by runtime queries and replication filtering

Avoid stringly-typed runtime logic where strong IDs are appropriate.

Add focused tests for:

- same address
- different map
- different channel
- different instance
- address transition
- invalid/stale membership if applicable

---

## 6.0.2 — Identity Separation

Create or formalize strongly separated types/contracts for:

- RuntimeEntityId
- PersistentId or domain-specific persistent identity boundary
- ContentId

If the existing `EntityId { index, generation }` remains the correct runtime ID, preserve its generational safety.

Do not casually rename working public APIs unless the distinction can be made with aliases/newtypes/documentation.

Required tests must prove:

- stale runtime IDs remain rejected
- ContentId cannot accidentally be used as RuntimeEntityId
- Persistent identity is not required for transient entities
- runtime spawn/despawn does not mutate content identity semantics

---

## 6.0.3 — Runtime Lifecycle Contract

Define explicit lifecycle semantics.

At minimum distinguish:

- allocated/spawned
- entered world / active
- address changed
- left observer relevance
- left world
- despawned/destroyed

Important:

`not visible to client X` is NOT equivalent to server despawn.

Likewise:

changing map/channel/instance is not necessarily destruction of the persistent character.

Establish clear APIs and documentation so later systems cannot confuse these states.

---

## 6.0.4 — Runtime Query Boundary

Introduce a central runtime query API or contract.

Gameplay/network systems must not each implement arbitrary global scans.

The design must support evolution toward queries such as:

- entities at WorldAddress
- entities in map
- entities in instance
- entities near position
- entities matching runtime capability/component
- visibility candidates for an observer

Do NOT build a complex spatial index yet unless justified by current scale and existing architecture.

A correct simple implementation behind a stable interface is preferable.

The interface must allow later replacement with spatial partitioning without changing consumers.

---

## 6.0.5 — Visibility / Interest Contract

Establish a runtime-owned visibility/relevance boundary.

Networking should be able to ask conceptually:

> what entities are eligible/relevant for this observer?

The contract must distinguish at least:

1. world-address eligibility
2. visibility/AOI eligibility
3. replication relevance

6.0 does NOT need the final optimized AOI implementation.

However, future optimized filtering must fit behind the contract.

Add tests proving, at minimum:

- different instance → excluded
- different channel → excluded
- compatible world membership → candidate
- observer visibility does not mutate entity lifetime
- transition into/out of relevance is deterministic

---

## 6.0.6 — Replication Contract

Create a formal runtime-to-network replication boundary.

Gameplay state should not emit arbitrary network packets.

Define concepts that can later support:

- replicated vs non-replicated state
- owner-only state
- visible-observer state
- priority
- update frequency tier
- dirty/change tracking
- transient events
- state replication
- baseline/resync
- spawn/despawn visibility lifecycle
- delta-capable representation
- packet/byte budgeting

Do NOT implement every optimization now.

Focus on ownership, metadata/contracts, and an extensible representation.

A future replication scheduler must be able to choose:

- who receives an update
- what fields/state are eligible
- whether update is urgent
- whether it can wait
- whether a newer state supersedes an older one

Document the distinction between:

### State-like information
Examples:
- position
- velocity
- HP
- animation state

Often supersedable/coalescible.

### Edge/event-like information
Examples:
- jump trigger if represented as event
- attack activation
- item pickup
- interaction activation

May require stronger delivery semantics.

Do not conflate state coalescing with safe event loss.

---

## 6.0.7 — Client Presentation Boundary

Preserve the existing architecture:

Server authoritative runtime
→ replication
→ client replicated state
→ interpolation/presentation
→ rendering

Ensure new runtime APIs do not expose client presentation pose as gameplay truth.

Remote interpolation remains presentation-only.

Local camera/presentation logic remains client-side.

---

## 6.0.8 — Debug / Observability

Extend existing debug tooling minimally so developers can inspect, where applicable:

- RuntimeEntityId
- ContentId
- presence/absence of PersistentId
- WorldAddress
- lifecycle state
- visibility/relevance status
- replication classification/metadata

Do not turn 6.0 into a UI redesign.

Keep debug visuals optional and off by default where appropriate.

---

## 6.0.9 — Documentation

Update relevant docs.

At minimum document:

- Phase 6 runtime boundaries
- ID separation
- WorldAddress
- lifecycle semantics
- query ownership
- visibility ownership
- replication contract
- presentation boundary

Add ADR(s) if the repository uses ADRs for architectural commitments.

Explicitly record:

> full-world broadcast replication is not the final replication architecture.

---

## 6.0 Gate

Required tests must include:

- runtime ID stale-generation behavior
- identity separation
- WorldAddress comparisons/transitions
- lifecycle correctness
- query correctness
- visibility eligibility
- relevance transitions
- replication contract classification
- no regression to existing Phase 5 networking behavior

Run:

cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

If GREEN:
continue automatically to Phase 6A.

If NOT GREEN:
fix within scope before proceeding.

Produce a short `PHASE_60_REPORT` artifact/document containing:

- architecture introduced
- files changed
- tests added
- compatibility decisions
- unresolved limitations intentionally deferred
- GREEN/NOT GREEN

---

# ============================================================
# PHASE 6A — RUNTIME MODEL
# ============================================================

## Goal

Build the concrete runtime entity model on top of the 6.0 contracts.

The runtime must support composition instead of a rigid class hierarchy.

We want to be able to represent future:

- player
- MOB
- NPC
- projectile
- interactable
- drop
- trigger
- world object

without creating unrelated parallel entity systems.

Do NOT introduce a third-party ECS framework.

If the existing World storage can evolve cleanly, evolve it.

---

## 6A.1 — Composition Model

Create a lightweight runtime composition system.

An entity should be able to carry zero or more runtime capabilities/components such as:

- Transform
- Velocity
- Movement
- Health
- Replication
- Interactable marker/data
- Character-like identity
- Content reference
- World membership

Do not implement future gameplay behavior unnecessarily.

The core requirement is that systems can operate on required capabilities without requiring concrete inheritance like:

- PlayerEntity
- MobEntity
- NpcEntity

as mutually isolated code families.

Prefer strongly typed components/capabilities.

---

## 6A.2 — Transform and Spatial State

Formalize transform/spatial state as runtime data.

Keep authoritative simulation position separate from rendered/interpolated presentation position.

WorldAddress and Transform are distinct:

- WorldAddress answers WHERE IN THE LOGICAL WORLD
- Transform answers WHERE WITHIN THAT SPACE

Do not encode map/channel/instance into coordinates.

---

## 6A.3 — Character Runtime Foundation

Introduce only the minimal shared runtime foundation needed for character-like entities.

This may include:

- runtime identity
- transform
- movement capability
- basic life/health container if architecturally appropriate
- owner/player association where necessary

Do NOT build:

- leveling
- stats progression
- skills
- equipment
- inventory
- combat resolution

The goal is reusable runtime structure, not game content.

---

## 6A.4 — Content-Backed Runtime Spawn

Create a clean boundary for spawning runtime entities from content definitions.

Conceptually:

ContentId / definition
→ validated runtime spawn request
→ RuntimeEntityId
→ composed runtime entity

Do not require every runtime entity to come from content.

Support transient/programmatic entities too.

Do not implement Phase 6C's full content runtime.

The purpose here is the spawn contract.

---

## 6A.5 — Runtime Mutation Ownership

Define how runtime state is changed.

Avoid uncontrolled public mutation of internal world storage.

Prefer explicit mutation APIs/system access where useful.

Preserve deterministic simulation behavior.

Ensure later gameplay systems can mutate runtime state without bypassing lifecycle/query/replication bookkeeping.

This is especially important for:

- address changes
- component addition/removal if supported
- spawn/despawn
- transform changes
- replication dirty state

---

## 6A.6 — Change / Dirty Tracking Foundation

Introduce a minimal mechanism that allows replication to know that relevant runtime state changed.

Do not build a sophisticated compression system yet.

Requirements:

- state mutation can mark relevant replication state dirty
- unchanged state does not need to masquerade as changed
- dirty tracking is owned centrally enough to remain reliable
- lifecycle changes generate appropriate replication relevance changes

Design so future delta replication can consume it.

Add tests for:

- mutation marks dirty
- read-only query does not mark dirty
- despawn/address transition produces expected lifecycle change
- dirty state can be consumed/reset safely according to the chosen model

---

## 6A.7 — Query Composition

Extend the 6.0 query API so systems can query runtime composition safely.

Examples:

- movable entities in address
- interactable entities near observer
- replicated entities in candidate set

Do not expose storage internals just to make queries easy.

Performance can remain simple at this phase, but API boundaries must be replaceable.

---

## 6A.8 — Determinism / Ordering

Where system ordering matters, document and test it.

Avoid relying on hash-map iteration order for authoritative gameplay outcomes.

Preserve existing deterministic movement/simulation expectations.

If runtime component storage introduces iteration-order ambiguity, establish deterministic iteration or ensure outcomes are order-independent.

---

## 6A.9 — Runtime Test Fixtures

Create reusable test fixtures/builders for runtime entities.

We will need these throughout Phase 6.

Examples:

- test player
- test MOB-like entity
- test interactable
- transient replicated entity
- entities in different addresses

Avoid huge setup duplication across tests.

---

## 6A.10 — Debug Integration

Update debug inspection so composed runtime entities can expose:

- component/capability summary
- WorldAddress
- runtime identity
- dirty/replication state where useful

Keep normal rendering behavior unchanged unless required.

---

## 6A Gate

At minimum prove:

- multiple entity archetypes can exist through composition
- systems can query required capabilities
- runtime ID generation safety remains intact
- WorldAddress membership remains correct
- content-backed spawn contract works
- transient spawn works
- dirty tracking works
- component/runtime mutation does not bypass lifecycle bookkeeping
- deterministic assumptions remain valid
- existing movement/networking tests remain green

Run:

cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

If GREEN:
continue automatically to Phase 6B.

If NOT GREEN:
fix within scope before proceeding.

Produce a short `PHASE_6A_REPORT` containing:

- runtime composition model
- spawn model
- mutation model
- dirty/change model
- query model
- tests
- deferred items
- GREEN/NOT GREEN

---

# ============================================================
# PHASE 6B — INTERACTION + UI RUNTIME
# ============================================================

## Goal

Create the generic runtime interaction path and client UI-state boundary that future NPCs, objects, vendors, dialogs, inventory and world interactions can reuse.

This phase is NOT about building final UI art.

It establishes:

- interactable runtime capability
- interaction discovery
- authoritative interaction validation
- request/result protocol boundary
- generic interaction session/state
- client UI runtime/state
- clean cancellation/lifecycle semantics

We want one interaction architecture, not a separate custom protocol for every future feature.

---

## 6B.1 — Interactable Capability

Introduce a generic runtime interactable capability/component.

It must allow a runtime entity to advertise that it can participate in an interaction.

Do not hardcode only NPCs.

Future interactables may include:

- NPC
- portal
- chest
- dropped object
- switch
- crafting station
- vendor
- quest object

The capability should identify interaction type/intent without embedding full future gameplay logic.

---

## 6B.2 — Interaction Discovery

Provide runtime queries for discovering interaction candidates near an observer/player.

Eligibility must respect:

- compatible WorldAddress
- runtime lifecycle
- visibility/relevance where applicable
- interaction range
- interactable capability

Do not allow interaction with an entity merely because the client submitted its ID.

The server must validate it.

---

## 6B.3 — Authoritative Interaction Request

Define a protocol/runtime request path conceptually like:

client intent
→ server receives request
→ validate player/session ownership
→ resolve RuntimeEntityId
→ validate generation
→ validate WorldAddress
→ validate range/eligibility
→ dispatch runtime interaction
→ produce authoritative result

Do not let client choose arbitrary result/state.

Add rejection reasons that are structured enough for debugging and UI behavior.

Examples:

- target missing
- stale ID
- wrong world address
- out of range
- not interactable
- interaction unavailable
- already closed/invalid session

Do not leak sensitive server internals unnecessarily.

---

## 6B.4 — Interaction Sessions

Some interactions are instantaneous.
Others persist.

Create a minimal generic interaction-session model capable of representing:

- opened
- active
- updated
- closed/cancelled

Examples later:

- dialog
- shop
- storage
- crafting UI

Do NOT implement those systems now.

The session must have clear ownership:

- which player/session owns it
- target entity if relevant
- runtime lifecycle
- cancellation conditions

Changing map/instance, despawning target, disconnecting, or otherwise invalidating prerequisites must close the interaction cleanly.

---

## 6B.5 — Client UI Runtime Boundary

Create a client-side UI runtime/state layer separate from rendering widgets.

The client should be able to represent authoritative UI-related state such as:

- no interaction
- interaction opening
- active interaction
- interaction result/update
- interaction closed/rejected

Do not entangle game-network protocol parsing directly with final visual widgets.

Architecture should remain:

network message
→ client runtime/UI state
→ presentation/UI rendering

This mirrors the gameplay/presentation separation already established elsewhere.

---

## 6B.6 — Minimal Developer UI

Implement only enough developer UI to prove the interaction path.

For example:

- show nearest/selected interactable
- display interaction session state
- display target runtime/content identity
- show authoritative open/reject/close result

Use existing debug/UI infrastructure where possible.

Do not spend significant effort on styling.

This is a runtime proof, not final UX.

---

## 6B.7 — Interaction Input

Integrate a minimal interaction input action.

Do not overload unrelated movement semantics.

Use an explicit action in the input model, even if temporary key mapping is used for development.

Input must flow through the established authoritative request path.

Avoid having the client directly mutate interaction state as if authoritative.

---

## 6B.8 — Visibility / Interaction Relationship

Interaction and replication visibility must cooperate but remain distinct concepts.

A visible entity is not automatically interactable.

An interactable entity must still satisfy:

- world membership
- lifecycle
- range
- capability
- interaction-specific validation

If an active interaction target leaves valid world context or despawns:

- server closes/invalidates the session
- client receives authoritative closure
- stale client state is cleaned up

Add tests.

---

## 6B.9 — UI Replication Semantics

Do not send full UI state every tick.

Interaction/UI state is primarily event/change-driven.

Establish this now.

Examples:

- interaction opened → event/result
- dialog content changed → update
- interaction closed → closure
- unchanged session → no redundant full-state spam

Where state resync may eventually be needed, document the baseline/resync path without building excessive machinery.

This is an early practical application of the Phase 6 replication philosophy.

---

## 6B.10 — Security / Trust Boundary

Treat every client interaction request as untrusted.

Validate:

- ownership
- target existence
- generation
- world address
- distance
- capability
- current lifecycle/session state

Do not trust client-provided:

- distance
- target validity
- interaction outcome
- item/result state
- UI completion state

Add negative tests.

---

## 6B.11 — Protocol Evolution

If protocol messages must change:

- preserve clear versioning rules
- update protocol tests
- update client and server together
- avoid embedding future feature-specific payloads prematurely

Prefer generic interaction request/result envelopes with typed variants where appropriate.

Do not create an untyped JSON-like escape hatch inside the binary protocol merely for convenience.

---

## 6B.12 — Test Scenario

Provide a deterministic developer scenario.

Example:

- local player
- one nearby interactable entity
- one out-of-range interactable
- one non-interactable entity
- optional entity in another instance

Prove:

1. nearby valid interaction opens
2. out-of-range rejects
3. wrong-instance rejects
4. non-interactable rejects
5. target despawn closes active session
6. address transition invalidates interaction
7. disconnect cleanup works
8. stale RuntimeEntityId cannot interact with reused generation

Where practical, include automated integration tests rather than relying only on visual inspection.

---

## 6B.13 — Documentation

Document:

- generic interaction architecture
- authoritative validation
- interaction session lifecycle
- UI runtime/presentation boundary
- replication/event semantics
- cancellation rules
- security assumptions
- what remains intentionally deferred

Future systems such as NPC dialogs, shops and inventory UI must be instructed to reuse this interaction path rather than bypass it.

---

# PHASE 6B GATE

Required correctness:

- server-authoritative interaction
- generic interactable capability
- runtime query-based candidate discovery
- range/world/lifecycle validation
- stale-ID rejection
- interaction session lifecycle
- authoritative close/cancel
- client UI runtime state separate from rendering
- event/change-driven UI replication
- negative security tests
- existing Phase 5 and 6.0/6A tests remain green

Run:

cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

Also perform a developer smoke test demonstrating the 6B interaction scenario if the project supports it.

---

# FINAL MASTER GATE

Only after 6.0, 6A and 6B are individually GREEN:

1. rerun the full workspace quality gate
2. review compiler warnings
3. review public API boundaries
4. verify no Phase 6C implementation accidentally entered scope
5. verify networking still operates
6. verify remote interpolation/presentation boundaries still hold
7. verify runtime visibility contract exists
8. verify no subsystem assumes full-world replication as a permanent rule
9. verify future replication budgeting/delta/frequency policies remain possible
10. verify interaction requests remain server-authoritative

Create/update project documentation summarizing the final state.

---

# FINAL RESPONSE FORMAT

When all requested work is finished, return one final report with:

## Phase 6.0
- status GREEN / NOT GREEN
- architecture added
- important APIs/types
- tests added
- docs/ADRs
- deferred work

## Phase 6A
- status GREEN / NOT GREEN
- composition model
- spawn/mutation/query model
- dirty/change tracking
- tests
- deferred work

## Phase 6B
- status GREEN / NOT GREEN
- interaction architecture
- UI runtime boundary
- protocol changes
- validation/security
- tests
- smoke result
- deferred work

## Full quality gate
- fmt
- check
- clippy
- tests

## Architectural audit
Explicitly answer:

1. Can MOBs/NPCs be added later without creating a separate entity architecture?
2. Can runtime queries later switch to spatial indexing without changing gameplay consumers?
3. Can replication later add AOI tiers, dirty-state filtering, delta encoding and byte budgeting without rewriting gameplay systems?
4. Are Runtime/Persistent/Content identities separated?
5. Does WorldAddress correctly isolate map/channel/instance membership?
6. Can leaving an observer's AOI occur without despawning the server entity?
7. Are interaction requests fully authoritative?
8. Is client UI state separated from UI rendering?
9. Did any Phase 6C work accidentally enter scope?

## Final verdict

Return exactly one of:

- `PHASE 6.0 + 6A + 6B GREEN — STOP BEFORE 6C`
- `NOT GREEN — <specific blocking reason>`

Do not start Phase 6C.
