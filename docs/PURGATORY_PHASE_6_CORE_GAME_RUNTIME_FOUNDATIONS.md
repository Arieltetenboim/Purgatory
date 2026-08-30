# PURGATORY — Phase 6: Core Game Runtime Foundations

**Status:** 6.0 contracts in progress / 6A–6F later  
**Prerequisite:** Phase 5 GREEN (5.7 steady-state isolation)  
**Purpose:** Establish the reusable game-runtime foundations on top of which MOBs, NPCs, UI, inventory, combat, maps, persistence, and later MMORPG systems can be added without repeatedly changing the core architecture.

Phase 6.0 (this repository) pulls **query / visibility / replication contracts** forward. Heavy spatial AOI, tiers, scheduling, and byte budgeting remain **6D**. Phase 6B in the master prompt is the authoritative interaction/session path, not WindowManager/mouse inspector.

---

## 1. Phase 6 Goal

Phase 6 converts PURGATORY from a networked movement/simulation foundation into a **real game runtime**.

The final success criterion is not merely “two maps work” or “a window can open.” The architectural gate is:

> **A new gameplay entity or gameplay system can be added without rewriting the Networking core, World core, Persistence core, UI core, or Interaction core.**

Phase 6 therefore builds **reusable seams and contracts**, not full gameplay systems.

---

## 2. Core Principles

### 2.1 Server authority remains unchanged
Gameplay truth remains authoritative on the server. Client-side interaction, selection, hover, UI presentation, and local presentation state must not become authoritative gameplay state.

### 2.2 Composition over rigid entity hierarchies
Do not build a rigid hierarchy such as `PlayerEntity -> MobEntity -> NpcEntity`.

Prefer entities identified by `EntityId` and composed from reusable capabilities/components such as:

- Transform
- World membership
- Selectable
- Inspectable
- Interactable
- Health
- Future behavior/state components

Phase 6 does **not** require converting the entire project into a full ECS (Entity Component System). It does require avoiding architecture that prevents composition later.

### 2.3 Separate ID domains
Keep these concepts explicitly distinct:

- **Runtime IDs** — e.g. `EntityId`; valid while a runtime entity exists.
- **Persistent IDs** — e.g. `CharacterId`; survive server restarts.
- **Content IDs** — e.g. `MapId`, `MobDefinitionId`, `ItemDefinitionId`; identify definitions/configuration.

Never use one category as a substitute for another.

### 2.4 World location is a first-class concept
Introduce a world-scope/address model early, conceptually:

```text
WorldAddress
├── MapId
├── ChannelId
└── InstanceId
```

Initially, Channel and Instance may always use default values. The purpose is to prevent later systems from assuming that `MapId` alone uniquely identifies a world.

### 2.5 Build scalability seams, not premature large-scale systems
Phase 6 should prepare replaceable interfaces for:

- spatial lookup
- visibility / Area of Interest
- persistence backend
- content definitions
- scheduling

The first implementations may be simple. They must not lock future systems into simple implementations.

### 2.6 No blocking persistence in the simulation loop
Game simulation must never wait on file writes.

### 2.7 Content should be data-driven
Maps and later MOBs, NPCs, items, skills, shops and quests should be defined through content definitions rather than being hard-coded into Rust gameplay logic.

---

# 3. Phase 6 Structure

Phase 6 is divided into six architectural layers:

- **6A — Runtime Model**
- **6B — Interaction + UI Runtime**
- **6C — World + Content Runtime**
- **6D — Runtime Query + Visibility**
- **6E — Character + Persistence**
- **6F — Runtime Services + Hardening**

Each layer has its own acceptance gate.

---

# 6A — Runtime Model

## Objective

Define the common language and lifecycle of the game runtime before introducing MOB/NPC/combat-specific concepts.

## Scope

### WorldAddress
Introduce the common world-scope representation:

- Map
- Channel
- Instance

Default Channel/Instance values are acceptable initially.

### ID taxonomy
Formalize the separation between:

- EntityId
- CharacterId
- Content definition IDs
- Future AccountId / ItemInstanceId categories

### Entity composition rules
Ensure entities can gain reusable capabilities without requiring dedicated class-like hierarchies.

### Runtime lifecycle
Define consistent lifecycle rules for:

- entity creation
- entity spawn
- entity world membership
- world transfer
- despawn
- disconnect cleanup
- stale generational EntityId handling

### Typed Commands and Events

Separate:

**Command** — request to perform something.

Examples:

```text
InteractWith(EntityId)
RequestMapTransition(...)
```

**Event** — something that has already happened.

Examples:

```text
EntitySpawned
EntityDespawned
MapEntered
MapLeft
CharacterLoaded
CharacterSaved
```

Gameplay systems must not grow into a chain of direct system-to-system calls.

### Deterministic system ordering
Establish explicit runtime ordering where order matters instead of allowing incidental execution order to become gameplay behavior.

## Gate 6A

It must be possible to:

1. create a generic entity;
2. assign it to a WorldAddress;
3. move/remove it;
4. observe correct typed lifecycle events;
5. reject stale EntityIds;
6. do all of the above without Player/MOB/NPC-specific branches in the runtime core.

---

# 6B — Interaction + UI Runtime

> **Executed as:** authoritative `Interactable` + `InteractionSession` + protocol v5 + client `UIRuntimeState`. See [`docs/PHASE_6B_REPORT.md`](PHASE_6B_REPORT.md). Mouse picking, Entity Inspector, and `WindowManager` in the sections below remain **later client UI work**, not the 6B gate that closed.

## Objective

Create reusable client-side interaction and window foundations without yet implementing gameplay-specific mouse behavior or complete gameplay windows.

---

## Interaction Foundation

Support:

- cursor position
- pointer press/release
- left/right button state
- Screen Space to World Space conversion
- camera-aware coordinates
- world hit testing
- hovered entity
- selected entity
- clearing invalid selections
- clear boundary between UI input and world input

Mouse interaction is intended primarily for **inspection and interaction**, not as the planned attack or pickup mechanism.

The interaction layer must not know what a MOB, NPC or Player specifically is.

---

## Interaction capabilities

Begin with a small semantic set:

```text
Selectable
Inspectable
Interactable
```

Do not prematurely create dozens of specialized capabilities such as Talkable, Questable, Lootable, Tradable, etc.

---

## UI Framework Foundation

Create reusable UI infrastructure rather than implementing Inventory/Character windows independently.

Conceptual structure:

```text
UiManager
│
├── WindowManager
├── InputRouter
├── FocusManager
└── Window instances
```

### WindowManager responsibilities

- open
- close
- toggle
- open-state query
- focus
- bring-to-front
- z-order
- multiple simultaneous windows

### Reusable WindowFrame

Common window behavior/layout should be reusable:

- title
- close control
- position
- size
- content area
- optional dragging
- consistent input/focus behavior

Prefer composition over a large inheritance-heavy `BaseWindow`.

### Input routing

Required rule:

```text
Pointer input
     │
     ▼
Did UI consume it?
 ├── Yes → stop
 └── No  → world interaction
```

Keyboard focus must similarly avoid leaking UI input into gameplay controls.

---

## Initial test UI

Use a simple **Entity Inspector** window that can display:

- EntityId
- entity kind/capabilities
- world position
- WorldAddress
- generation/runtime diagnostics

It is a developer/runtime validation tool, not final game UI.

## Gate 6B

This complete path must work:

```text
Mouse
→ world hit test
→ entity selection
→ Entity Inspector
```

It must continue to work correctly while the camera moves and must not allow a UI click to fall through into the world.

---

# 6C — World + Content Runtime

## Objective

Stop treating the current test world as the only world and establish a data-driven content/runtime boundary.

---

## Content Registry

Introduce a runtime registry for validated content definitions.

Initial focus:

- map definitions
- spawn definitions
- transition definitions

The architecture must be suitable for later extension to:

```text
maps/
mobs/
npcs/
items/
skills/
shops/
quests/
```

Gameplay code should reference content by typed Content IDs instead of duplicating hard-coded values.

---

## Content validation

Invalid content should fail clearly during load/validation where possible.

Examples:

- duplicate IDs
- unknown destination map
- missing spawn point
- invalid references
- malformed definitions

Do not defer obvious configuration errors until a player happens to trigger them.

---

## Map definitions

Create explicit map definitions.

Initial test content:

```text
Map A — existing development/test map
Map B — minimal second test map
```

Map B does not need production art. It needs enough unique geometry/presentation to make transitions obvious.

---

## Map transitions

Server-authoritative flow:

```text
Player reaches transition
→ server validates
→ leave old world scope
→ old entities are removed from client presentation
→ assign destination WorldAddress
→ spawn at destination spawn point
→ destination entities replicate
```

Transition cleanup must account for existing Phase 5 systems, including relevant transient state such as:

- interpolation history
- selection / hover
- stale replicated entities
- platform support / FOOTNOTE contact
- prediction/reconciliation transient state
- camera state as appropriate

---

## Entity world membership

Entities belong to a WorldAddress.

Clients must not receive entities that are outside their relevant world scope.

---

## Channel / Instance foundation

Only the model/foundation is required now.

Initially:

```text
ChannelId  = DEFAULT
InstanceId = DEFAULT
```

Advanced allocation, population limits, dynamic channels and distributed world hosting are explicitly deferred.

## Gate 6C

The following path must be content-driven and server-authoritative:

```text
content files
→ validated Content Registry
→ Map A runtime
→ transition
→ Map B runtime
→ transition
→ Map A
```

Map-specific behavior must not be hard-coded into networking/gameplay core logic.

---

# 6D — Runtime Query + Visibility

## Objective

Provide shared query and visibility APIs so future systems do not each scan the entire world or invent their own spatial logic.

---

## Spatial Query API

Provide a common interface conceptually supporting:

```text
query_point(...)
query_radius(...)
query_aabb(...)
```

AABB = Axis-Aligned Bounding Box, a rectangle aligned to the world axes used for efficient overlap/query tests.

The first implementation is a **World-owned uniform grid** per live `WorldAddress` (`SPATIAL_CELL_SIZE_WU = 4.0` wu, tunable). Callers use `query_point` / `query_radius` / `query_aabb` and `World::spatial_candidates`. Do not scan the entire world independently.

AOI enter/leave rectangles are **server interest policy** (`[16, 9]` wu half-extents + `2` wu leave margin), not client 16:9. Hysteresis lives in `ObserverReplicationState` only.

Protocol **v8** sends `ReplicationFrame` (Enter/Update/Leave) on the existing one-uni-stream transport. See ADR-0038, ADR-0039, and [`docs/PHASE_6D_REPORT.md`](PHASE_6D_REPORT.md).

The important requirement is that callers depend on the **query interface**, not the implementation.

Future internal implementations may use:

- uniform grid
- spatial hash
- quadtree
- another spatial index

without rewriting MOB, combat, interaction or visibility code.

---

## Shared use cases

The same query layer should eventually support:

- pointer picking
- nearby MOB detection
- attack-radius queries
- portal/trigger overlap
- local entity discovery
- visibility candidates

---

## Visibility abstraction

Introduce an explicit visibility/interest boundary.

AOI = **Area of Interest**, the subset of world entities relevant enough to a client to replicate/render.

Do **not** implement sophisticated AOI partitioning yet.

Phase 6D implements interest-policy rectangles plus a uniform grid. Networking code must not permanently encode “send every entity in the entire map.”

Initial implementation may effectively mean:

> all relevant entities in the same WorldAddress.

But networking code should not permanently encode:

> send every entity in the entire map.

This allows future distance/region-based visibility without rewriting replication consumers.

## Gate 6D

Interaction, replication and future gameplay systems must request world candidates through shared APIs rather than performing independent unbounded world scans.

Automated verification for this repository's 6D implementation is recorded in [`docs/PHASE_6D_REPORT.md`](PHASE_6D_REPORT.md) and [`docs/TEST_GATES.md`](TEST_GATES.md). Load characterization: [`docs/PHASE_6D_PERFORMANCE.md`](PHASE_6D_PERFORMANCE.md). Manual runtime check remains required. Do not begin 6E until that check is done.

---

# 6E — Character + Persistence

## Objective

Create the first real persistent game-domain boundary and file-backed persistence while keeping storage replaceable.

---

## Domain separation

Explicitly separate:

```text
Connection
≠ Session
≠ Player Entity
≠ Character
≠ future Account
```

A QUIC connection must never become the identity of a persistent character.

---

## CharacterData

Create a minimal persistent domain model containing only fields that Phase 6 actually needs.

Possible initial fields:

- CharacterId
- name/test identity
- current/default WorldAddress or safe map
- persistent spawn/location information where appropriate
- schema version

Currency, inventory, equipment, progression, quests, etc. should be added when their systems arrive rather than pre-building them without semantics.

---

## Persistence interface

Gameplay must depend on a repository/service boundary such as:

```text
load_character(...)
save_character(...)
create_character(...)
```

It must never directly open character files.

Initial backend:

```text
FileCharacterRepository
```

Future backend:

```text
SqlCharacterRepository
```

SQL = Structured Query Language, the standard language family used by relational databases such as PostgreSQL and MySQL.

The future transition from files to a database must be primarily a persistence-layer change, not a gameplay rewrite.

---

## File-backed persistence requirements

### No in-memory-only persistence stage
Phase 6 goes directly to real file-backed storage.

Runtime copies of loaded state will naturally exist in memory while the game is running, but the persistence implementation itself is file-backed from the start.

### Dirty state / save queue
Persistent changes should mark data dirty and flow through a persistence queue/service.

Disk I/O must not block the 30 Hz simulation.

### Atomic writes
Do not directly overwrite the only valid character file.

Use a safe strategy conceptually similar to:

```text
write temporary file
→ flush/validate
→ atomic replace/rename
```

to reduce corruption risk from crashes/interruption.

### Schema version
Persist an explicit schema version from the first format.

Example:

```text
schema_version: 1
```

### Migration seam
No complex migration system is required yet, but the format and repository boundary must allow:

```text
v1 → v2 → v3
```

later.

### Error/corruption handling
Define behavior for:

- missing character
- malformed file
- unsupported schema
- failed write
- partially recoverable persistence errors

Do not crash the entire game server for a recoverable character-storage failure.

---

## Save/load lifecycle

Initial flow:

```text
connect/session setup
→ load character
→ spawn Player Entity
→ enter saved/default WorldAddress
```

Save triggers may initially include:

- meaningful persistent change
- map/world transition where relevant
- disconnect
- server shutdown
- later periodic safety save

Never save every simulation tick.

---

## Restart gate

Required persistence test:

```text
start server
→ load/join character
→ change persistent state
→ transition map
→ save
→ stop server
→ restart server
→ reconnect/load
```

Expected result: persistent state is restored correctly.

## Gate 6E

Persistence must survive server restart, avoid simulation-thread disk blocking, and remain isolated behind a replaceable repository contract.

---

# 6F — Runtime Services + Hardening

## Objective

Add the shared runtime services and diagnostics needed before Phase 7 begins adding real gameplay entities.

---

## Gameplay Scheduler / Timers

The 30 Hz simulation clock exists, but future gameplay requires a shared timing service.

Examples:

- MOB respawn
- cooldown
- temporary effect
- delayed NPC behavior
- timed trigger

Provide a small deterministic scheduling foundation conceptually supporting:

```text
schedule_at(...)
schedule_after(...)
cancel(...)
```

Do not build a full scripting/event scheduler.

Individual gameplay systems should not invent unrelated timer implementations.

---

## Error / Failure Model

Define categories such as:

- recoverable gameplay/runtime error
- player/session error
- content/configuration error
- persistence error
- fatal server error

Examples that must have deliberate behavior:

- unknown map
- missing spawn
- invalid ContentId
- entity despawned during interaction
- persistence write failure
- illegal transition request

Avoid indiscriminate `unwrap()` behavior in runtime paths.

---

## Observability

Every major Phase 6 subsystem should expose useful developer diagnostics.

Examples:

```text
WORLD
entities
loaded maps
active world scopes

INTERACTION
hovered entity
selected entity

PERSISTENCE
dirty characters
pending saves
last save
save failures

SPATIAL
queries/tick
candidate counts

VISIBILITY
visible entities / available entities

RUNTIME
scheduled timers/events
queue sizes
```

Observability is part of the runtime contract, not an afterthought.

---

## Performance / scalability constraints

Extend `PERFORMANCE_BUDGETS.md` with architectural budgets.

Do not invent arbitrary production player-count promises yet.

Required principles include:

- maintain the 30 Hz authoritative simulation target;
- disk I/O never blocks simulation;
- no unbounded gameplay/event queues;
- no unbounded history collections;
- no mandatory full-world replication per client;
- no requirement that every future spatial query scans every world entity;
- collections with potentially growing lifetime must have explicit bounds/lifecycle;
- content definitions should be shared, not duplicated per runtime entity where unnecessary.

---

## Synthetic load / test harness

Add developer/test capability to create synthetic entities/load without requiring real game content or art.

Examples:

```text
spawn test entities
generate repeated transitions
create multiple runtime entities
exercise visibility/spatial queries
```

Use bots/test clients where appropriate.

The purpose is to discover architecture problems before real MOB/content volume exists.

---

## Graceful server shutdown

Introduce deliberate server shutdown lifecycle:

```text
stop accepting new work
→ resolve/flush required persistence work
→ close sessions/world runtime cleanly
→ exit
```

A normal controlled server shutdown must not behave like an unexpected process crash.

---

## Integration / Soak Gate

Before Phase 7:

- multiple clients
- join/leave
- Map A ↔ Map B repeatedly
- clients in different WorldAddresses
- selection during despawn
- UI open during transitions
- disconnect/reconnect
- server restart + character restore
- persistence failure handling
- synthetic entity load
- spatial/visibility diagnostics
- scheduler behavior
- graceful shutdown

## Gate 6F

All Phase 6 layers operate together without growing unbounded state, blocking the simulation loop, or requiring gameplay-specific exceptions in shared runtime services.

---

# 4. Phase 6 Final Acceptance Gate

Phase 6 is GREEN only when the architecture demonstrates that a future gameplay entity/system can reuse the runtime instead of modifying its foundations.

The Phase 7 MOB should conceptually require mainly:

```text
ADD
├── MobDefinition
├── Health
├── Mob state/behavior
└── MobSystem

REUSE
├── EntityId
├── WorldAddress
├── Content Registry
├── lifecycle
├── Commands / Events
├── Spatial Queries
├── Scheduler
├── Visibility / Replication boundary
├── Selection / Inspection
├── UI framework
└── Diagnostics
```

If adding the first MOB requires rewriting the World, Networking, Interaction, UI, Persistence or Content foundations, Phase 6 should be considered architecturally incomplete.

---

# 5. Explicitly Out of Scope for Phase 6

Do not allow Phase 6 to expand into implementation of:

- combat framework
- detailed stats framework
- inventory system
- item/equipment system
- quests
- NPC dialogue engine
- advanced MOB AI
- behavior trees
- pathfinding
- production SQL/database backend
- authentication/account service
- distributed game servers
- dynamic channel allocation
- production-scale AOI partitioning
- scripting language
- Paper Doll / final graphics pipeline
- real game content

Phase 6 should create the **attachment points** required by these systems, not build the systems themselves.

---

# 6. Phase 6 Completion State

At completion, PURGATORY should have the following runtime stack:

```text
                   PURGATORY
                       │
       ┌───────────────┼────────────────┐
       │               │                │
 Interaction          UI           Runtime Services
       │               │          Events / Timers /
       └───────┬───────┘          Diagnostics
               │                       │
               ▼                       │
        World Runtime ─────────────────┘
               │
       WorldAddress / Lifecycle
               │
       Content Registry
               │
     Spatial + Visibility APIs
               │
               ▼
           Character
               │
      Persistence Boundary
               │
      File-backed Repository
```

This is the foundation on which Phase 7 and later gameplay systems are expected to build.

---

## 7. Guiding Rule

> **Phase 6 should build the grammar of the game runtime, not the gameplay vocabulary.**

MOBs, NPCs, combat, inventory and items come later. Phase 6 makes sure they can all speak the same runtime language.
