# PURGATORY — MASTER EXECUTION PLAN
## Custom 2D MMORPG Engine From Zero

**Purpose of this file:**  
This is the master execution specification for Cursor. Treat it as an instruction file, not as a design essay.

Cursor must create the project from an empty directory and execute the phases in order.  
Do not skip ahead. Do not add speculative systems. Do not replace the chosen stack with a game engine.

---

# 0. FINAL PRODUCT DIRECTION

PURGATORY is a custom-built, native, 2D side-scrolling MMORPG.

The final game direction includes:

- 2D side-scrolling maps.
- Real-time action combat.
- Server-authoritative simulation.
- Many concurrent players distributed across maps/zones/channels.
- Monsters, NPCs, items, skills, loot, progression, quests and chat.
- Paper Doll character presentation with layered equipment sprites.
- Persistent characters and accounts.
- Content that can be added without rewriting core engine systems.
- Native desktop client first.
- Dedicated headless server.
- Strong emphasis on efficiency, observability and scalability.

The first versions are **technical foundations only**.

Early visuals must be placeholders:
- rectangles,
- circles,
- simple colored quads,
- debug text,
- primitive hitboxes.

Do not spend early development time on:
- final sprites,
- animation polish,
- VFX,
- sound,
- final UI,
- game lore implementation,
- quests,
- guilds,
- raids,
- housing,
- marketplace,
- premium currency.

The project must become structurally correct before it becomes visually impressive.

---

# 1. FIXED TECHNOLOGY DECISION

## Primary language: Rust

Use **stable Rust**, **Rust 2024 edition**, managed through `rustup` and `cargo`.

This is a deliberate decision for this project.

Reasons:

1. PURGATORY requires native performance for both client and server.
2. Rust provides memory safety without a garbage collector.
3. Rust's ownership/type system prevents data races in safe Rust and catches many concurrency mistakes at compile time.
4. MMORPG servers are long-running concurrent programs, so memory corruption, use-after-free and race conditions are especially expensive failure classes.
5. The same language can be used for:
   - simulation,
   - dedicated server,
   - client,
   - networking protocol,
   - asset/content tools,
   - test bots,
   - benchmarks.
6. Cargo workspaces make a multi-crate client/server/shared project easy to manage as one repository.
7. Cursor-generated code benefits from a strict compiler that rejects many invalid ownership and threading patterns before runtime.
8. There is no runtime garbage-collection pause to account for in simulation timing.

C++ remains a viable language for custom game engines, but for this project Rust's safety/concurrency advantages outweigh C++'s larger traditional game-engine ecosystem.

**Do not change the project to C++, C#, Java, TypeScript, Unreal, Unity, Godot or Bevy unless the owner explicitly changes this decision.**

---

# 2. LOW-LEVEL STACK

We are building a custom engine.  
Using a low-level library is allowed when it replaces platform boilerplate rather than game architecture.

## 2.1 Build and package management
- Rust stable.
- Rust 2024 edition.
- Cargo virtual workspace.
- One shared `Cargo.lock` committed to Git.
- `rustfmt`.
- `clippy`.

## 2.2 Window and operating-system events
Use **winit**.

Role:
- create native window,
- receive keyboard/mouse/window events,
- provide platform event loop.

winit is not a game engine.

## 2.3 Graphics
Use **wgpu**.

Role:
- cross-platform GPU abstraction,
- native backends such as Vulkan / Direct3D 12 / Metal,
- GPU buffers, textures, shaders and render passes.

We will build our own:
- 2D renderer,
- orthographic camera,
- sprite batching,
- texture/atlas management,
- debug primitive renderer,
- animation presentation layer.

Do not use Bevy or another engine on top of wgpu.

## 2.4 Async/network runtime
Use **Tokio** for asynchronous network and service I/O.

Important:
- Tokio must **not** become the gameplay simulation clock.
- Authoritative game simulation runs on an explicit fixed timestep.
- Async network tasks feed/receive queues around the simulation.

## 2.5 Network transport
Use **Quinn (QUIC)** as the initial transport.

Why:
- Rust-native.
- TLS 1.3 secured connections.
- reliable streams for critical/control messages.
- unreliable unordered datagrams for transient realtime messages.
- avoids writing our own encryption, congestion control and reliable transport from raw UDP.

Initial channel intent:

**Reliable streams**
- authentication/session setup,
- character selection,
- inventory changes,
- loot grants,
- chat,
- map transition control,
- persistence-related acknowledgements.

**Unreliable datagrams**
- input samples,
- movement snapshots,
- transient combat/world state where the newest state supersedes old state.

Do not assume QUIC is permanently optimal.  
Keep the game protocol logically separated from the transport so transport can be replaced after profiling if necessary.

## 2.6 Serialization
Use:
- `serde` for serialization traits,
- `serde_json` for human-editable development content,
- a compact binary serializer for wire messages only after protocol benchmarks.

Do **not** optimize wire serialization before a measured need.

All network messages must include explicit protocol/version semantics.

## 2.7 Diagnostics
Use:
- `tracing`,
- `tracing-subscriber`.

Logs must be structured enough to identify:
- server tick,
- connection/session,
- entity/player,
- subsystem,
- latency,
- packet/message type,
- errors.

## 2.8 ECS decision
Do **not** add Bevy ECS, Specs, Legion, hecs or another ECS library in Phase 0.

Start with a simple internal entity/world model that is easy to measure.

Only move to a formal Entity Component System (ECS) if profiling or system complexity demonstrates a real benefit.

ECS = Entity Component System: an architecture that stores entity data as components and executes logic through systems.

---

# 3. NON-NEGOTIABLE ARCHITECTURAL RULES

1. **Server authoritative**
   - The server owns authoritative world state.
   - Clients send intents/inputs, not authoritative results.

2. **Fixed simulation timestep**
   - Simulation rate is independent of render Frames Per Second (FPS).
   - FPS = Frames Per Second, the number of rendered frames each second.
   - Never make movement speed or combat rules depend on render frame duration.

3. **Headless server from the beginning**
   - The server must never require a window, renderer or GPU.

4. **Presentation is not simulation**
   - Sprites, Paper Doll, animation and effects represent state.
   - They do not define authoritative state.

5. **No default per-entity update loop**
   - Avoid a separate unconditional update/tick function for every entity.
   - Prefer batched/system-level processing and event/timer driven work where appropriate.

6. **Data-driven content**
   - Common content variants are data.
   - Adding a normal monster or item must not require editing engine source.

7. **Explicit ownership**
   - Every major object/resource must have a clear owner and lifetime.

8. **No speculative abstraction**
   - Do not implement plugin systems, scripting VMs, distributed clusters, custom allocators or job systems before they are needed.

9. **Measurement before optimization**
   - Scalability is planned early.
   - Optimization is justified by profiling.

10. **Stable boundaries**
   - Client, server, simulation, protocol, content and tools must not collapse into one codebase with circular dependencies.

---

# 4. REPOSITORY TO CREATE

Create this structure from the empty project directory.

```text
PURGATORY/
├─ Cargo.toml
├─ Cargo.lock
├─ rust-toolchain.toml
├─ rustfmt.toml
├─ .gitignore
├─ README.md
│
├─ apps/
│  ├─ client/
│  │  ├─ Cargo.toml
│  │  └─ src/
│  │     └─ main.rs
│  └─ server/
│     ├─ Cargo.toml
│     └─ src/
│        └─ main.rs
│
├─ crates/
│  ├─ simulation/
│  │  ├─ Cargo.toml
│  │  └─ src/
│  │     └─ lib.rs
│  ├─ protocol/
│  │  ├─ Cargo.toml
│  │  └─ src/
│  │     └─ lib.rs
│  ├─ content/
│  │  ├─ Cargo.toml
│  │  └─ src/
│  │     └─ lib.rs
│  └─ common/
│     ├─ Cargo.toml
│     └─ src/
│        └─ lib.rs
│
├─ tools/
│  ├─ content_validator/
│  │  ├─ Cargo.toml
│  │  └─ src/
│  │     └─ main.rs
│  └─ bot_client/
│     ├─ Cargo.toml
│     └─ src/
│        └─ main.rs
│
├─ content/
│  ├─ definitions/
│  │  ├─ monsters/
│  │  ├─ items/
│  │  ├─ skills/
│  │  └─ maps/
│  └─ dev/
│
├─ assets/
│  └─ dev/
│
├─ docs/
│  ├─ ARCHITECTURE.md
│  ├─ DECISIONS.md
│  ├─ PROTOCOL.md
│  ├─ CONTENT_PIPELINE.md
│  ├─ PERFORMANCE_BUDGETS.md
│  ├─ TEST_GATES.md
│  └─ ROADMAP.md
│
├─ scripts/
│  ├─ check.ps1
│  └─ check.sh
│
└─ tests/
   └─ README.md
```

Do not add more top-level directories without a demonstrated need.

---

# 5. CARGO WORKSPACE DESIGN

The root `Cargo.toml` must be a virtual workspace.

Workspace members:

```text
apps/client
apps/server
crates/common
crates/simulation
crates/protocol
crates/content
tools/content_validator
tools/bot_client
```

Use Cargo resolver version 3.

Use workspace-level:
- edition,
- rust-version if appropriate,
- common dependency versions,
- lints.

The dependency direction must be:

```text
common
  ↑
simulation
  ↑
server

common
  ↑
protocol
  ↑
client/server/bot_client

common
  ↑
content
  ↑
server/client/content_validator
```

`simulation` must not depend on:
- winit,
- wgpu,
- renderer code,
- UI,
- operating-system window APIs.

`server` must not depend on:
- winit,
- wgpu,
- client crate.

---

# 6. PHASE EXECUTION POLICY FOR CURSOR

Execute phases sequentially.

For every phase:

1. Read:
   - this file,
   - `docs/ARCHITECTURE.md`,
   - `docs/DECISIONS.md`,
   - `docs/TEST_GATES.md`.

2. State internally what files are required.

3. Make the smallest implementation that satisfies the phase.

4. Run:
   - formatting,
   - compile/check,
   - clippy,
   - tests.

5. Fix failures before continuing.

6. Update documentation if an architectural decision was introduced.

7. Record the achieved gate in `docs/TEST_GATES.md`.

8. Create a local Git commit if Git user configuration is available.

9. Never push to a remote repository unless explicitly instructed.

10. Continue to the next phase only after the current acceptance gate is green.

If a phase cannot be completed because of a genuine environmental blocker:
- stop at that phase,
- report the exact blocker,
- do not fake completion,
- do not implement later phases.

---

# 7. PHASE 0 — BOOTSTRAP FROM AN EMPTY DIRECTORY

This phase is mandatory and includes folder creation.

## Phase 0A — Environment detection

Check:

```text
git --version
rustc --version
cargo --version
rustup --version
```

If Rust is not installed:
- do not silently install arbitrary third-party toolchains.
- use the official rustup installation path if the environment permits installation.
- use stable Rust.

Create `rust-toolchain.toml` that pins the project to the stable channel and includes:
- rustfmt,
- clippy.

Do not pin to nightly Rust.

### Gate 0A

Must prove:
- `rustc` works,
- `cargo` works,
- `cargo fmt` is available,
- `cargo clippy` is available.

---

## Phase 0B — Initialize repository

If `.git` does not exist:

```text
git init
```

Create `.gitignore` appropriate for Rust and native development.

At minimum ignore:
- `/target/`
- OS/editor temporary files
- local logs
- generated profiling output
- local secrets/certificates

Do not ignore:
- `Cargo.lock`

For this application repository, `Cargo.lock` must be committed.

### Gate 0B

- repository exists,
- root is clean and intentional,
- generated build artifacts are not tracked.

---

## Phase 0C — Create Cargo workspace and all Phase-0 crates

Create the exact repository skeleton from section 4.

The client and server must be separate executables.

The tools must be separate executables.

Create minimal compilable code only.

Expected behavior:

### `purgatory-client`
Print/log:
```text
PURGATORY client bootstrap OK
```

### `purgatory-server`
Print/log:
```text
PURGATORY server bootstrap OK
```

### `purgatory-content-validator`
Print/log:
```text
PURGATORY content validator bootstrap OK
```

### `purgatory-bot-client`
Print/log:
```text
PURGATORY bot client bootstrap OK
```

Libraries must expose one minimal version/build-info function so workspace linkage can be tested.

Do not implement gameplay.

### Gate 0C

These must pass:

```text
cargo check --workspace
cargo build --workspace
cargo test --workspace
```

---

## Phase 0D — Quality gate scripts

Create:

### `scripts/check.ps1`
For Windows.

It must fail on the first failing command and execute:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

### `scripts/check.sh`
Equivalent behavior for Linux/macOS.

The repository quality gate is these scripts.

### Gate 0D

Run the relevant platform script successfully.

---

## Phase 0E — Core documentation

Create all files under `/docs`.

### `ARCHITECTURE.md`
Document:
- authoritative server,
- fixed simulation,
- client/server/shared boundaries,
- dependency direction,
- presentation vs simulation,
- headless-server rule,
- content-driven rule.

### `DECISIONS.md`
Create Architecture Decision Records (ADR).

ADR = Architecture Decision Record: a short record of an architectural choice, its reason and consequences.

Initial locked decisions:
- ADR-0001: Rust 2024.
- ADR-0002: custom engine; no Unreal/Unity/Godot/Bevy.
- ADR-0003: server-authoritative simulation.
- ADR-0004: fixed simulation timestep.
- ADR-0005: winit for platform window/input.
- ADR-0006: wgpu for graphics backend.
- ADR-0007: Tokio for async service/network I/O, not simulation timing.
- ADR-0008: Quinn/QUIC initial network transport.
- ADR-0009: JSON authoring format during early content development.
- ADR-0010: no external ECS initially.

### `PROTOCOL.md`
Start with:
- protocol version field,
- connection/session terminology,
- reliable vs transient message classes,
- rule that client never sends authoritative position/damage/item grants.

No full packet schema yet.

### `CONTENT_PIPELINE.md`
Define the target workflow:

```text
author data
→ validate
→ load into runtime registry
→ map author-facing string ID to runtime ID
→ simulation uses definition
→ client maps presentation references to assets
```

### `PERFORMANCE_BUDGETS.md`
Create a living table for:
- simulation tick duration,
- server tick rate,
- client FPS,
- bytes/sec/player,
- connected players,
- active entities,
- memory,
- CPU.

Values can be `TBD` initially.

### `TEST_GATES.md`
Record every gate in this master document with:
- status,
- command/test,
- date,
- notes.

### `ROADMAP.md`
Mirror the major phases below.

### Gate 0E

A new developer/agent must be able to understand:
- what is being built,
- what is not being built,
- authority boundaries,
- where code belongs,
- how to verify the repository.

---

# 8. PHASE 1 — CUSTOM RUNTIME AND CLOCK

Goal: establish the simulation model before rendering.

Implement in `crates/simulation`.

## Requirements

Create:
- `SimulationClock`,
- fixed timestep accumulator,
- tick counter,
- simulation duration/time representation,
- deterministic tick advancement API.

Initial simulation tick target:

```text
30 Hz
```

This is a starting engineering value, not a permanent promise.

A 30 Hz simulation step is approximately:

```text
33.333 ms
```

Rendering will later run independently.

Do not use wall-clock time directly inside gameplay rules.

Test:
- 1 second of supplied elapsed time produces approximately 30 simulation ticks,
- frame-time variations do not change total simulation progression,
- large elapsed-time spikes are clamped to prevent a spiral of death.

Spiral of death = a condition where a slow frame causes many catch-up updates, making the next frame even slower repeatedly.

### Gate 1

- simulation tests pass,
- no rendering dependency,
- server can drive the simulation clock headlessly.

---

# 9. PHASE 2 — CLIENT WINDOW + PLACEHOLDER RENDERER

Only now add client graphics dependencies.

Use:
- `winit`,
- `wgpu`.

## Requirements

Create a native window.

Render:
- clear background,
- one colored rectangle,
- one debug marker.

Implement:
- orthographic 2D coordinate model,
- logical world coordinates separate from physical pixels,
- resize handling,
- render timing counter.

No sprite system yet.

No game UI.

### Gate 2

- client opens a stable native window,
- window resize works,
- rectangle renders,
- renderer is isolated from simulation,
- server still builds without GPU/window dependencies.

---

# 10. PHASE 3 — INPUT + LOCAL PLACEHOLDER CHARACTER

Implement client input abstraction.

Do not let raw key codes enter simulation rules.

Create semantic actions:
- MoveLeft,
- MoveRight,
- Jump.

Create a placeholder player rectangle.

Add:
- horizontal movement,
- gravity,
- jump,
- simple static platforms,
- AABB collision.

AABB = Axis-Aligned Bounding Box: a rectangle collision shape whose edges stay aligned with the coordinate axes.

Keep collision intentionally simple.

### Gate 3

- character moves on a platform,
- falls under gravity,
- jumps,
- cannot fall through the basic floor,
- movement behavior depends on simulation ticks, not rendering FPS.

---

# 11. PHASE 4 — WORLD AND ENTITY FOUNDATION

Create a simple internal entity model.

Do not install an ECS library.

Minimum concepts:
- `EntityId`,
- entity generation/version or equivalent stale-ID protection,
- transform/position,
- velocity,
- collider,
- player marker,
- simple world storage,
- spawn,
- despawn.

Design for iteration in batches.

Add tests for:
- creating entities,
- deleting entities,
- stale IDs,
- repeated spawn/despawn.

Create a benchmark or dev test with at least:
- 1,000 placeholder entities.

This does not imply 1,000 players on one screen.

### Gate 4

- 1,000 placeholder entities can exist in a test world,
- lifecycle is correct,
- no renderer ownership leaks into world state,
- measured timings are written to performance notes.

---

# 12. PHASE 5 — NETWORK CONNECTION FOUNDATION

Add:
- Tokio,
- Quinn,
- tracing.

Create:
- headless server listener,
- client connection,
- development certificates,
- protocol handshake,
- protocol version,
- connection ID,
- session state.

Development certificates must never be confused with production certificate handling.

Message categories:

### Reliable
- Hello
- Welcome
- DisconnectReason

### Datagram
- PingSample or transient test message

Do not synchronize gameplay yet.

### Gate 5

- one client connects,
- handshake completes,
- incompatible protocol version is rejected cleanly,
- disconnect is detected,
- structured logs show connection lifecycle.

---

# 13. PHASE 6 — AUTHORITATIVE MULTIPLAYER MOVEMENT

This is the first true multiplayer game-system milestone.

Rule:
The client sends input intent.

The client must not send:
```text
"My authoritative X position is 500."
```

Create messages conceptually like:

```text
InputCommand {
    sequence,
    tick_hint,
    horizontal_axis,
    jump_pressed
}
```

Server:
- receives inputs,
- validates input rate/range,
- simulates player movement,
- owns authoritative position,
- publishes snapshots.

Client:
- receives snapshots,
- initially displays server state without prediction.

First make correctness obvious.

### Gate 6

- two clients connect,
- both players exist on server,
- both clients see both placeholder players,
- server positions are authoritative,
- modified client cannot directly set arbitrary world coordinates.

---

# 14. PHASE 7 — CLIENT PREDICTION, RECONCILIATION AND INTERPOLATION

Only after Phase 6 is correct.

Definitions:

**Client-side prediction**  
The local client immediately simulates its own input before the server reply arrives.

**Server reconciliation**  
When authoritative state arrives, the client corrects its prediction and reapplies unacknowledged inputs.

**Interpolation**  
Remote entities are rendered between received snapshots to appear smooth.

Implement:
- input sequence numbers,
- last processed input acknowledgement,
- local prediction,
- reconciliation,
- remote interpolation buffer.

Add network simulation:
- latency,
- jitter,
- packet loss.

### Gate 7

Test at minimum:
- 0 ms artificial latency,
- 80 ms,
- 150 ms,
- modest packet loss.

Record:
- correction frequency,
- correction magnitude,
- bandwidth per player.

---

# 15. PHASE 8 — CONTENT FOUNDATION

Now create actual data-driven definitions.

Use author-facing JSON in `/content/definitions`.

Create:

## MonsterDefinition
Initial fields:
- id,
- debug_name,
- health_max,
- movement_speed,
- half_extents,
- one closed, runtime-supported behavior profile with its required parameters.

Add presentation, ability-loadout, loot-table, and placement references only
with the scoped runtime/content consumer that validates and uses them. Do not
reserve inert JSON fields. Current contract:
`docs/MONSTER_AUTHORING_RUNTIME.md`.

## ItemDefinition
- id,
- display_name,
- category,
- max_stack,
- presentation_id.

## SkillDefinition
- id,
- display_name,
- cooldown_ms,
- range,
- behavior_id,
- presentation_id.

Author IDs are stable strings such as:

```text
monster.slime.green
item.consumable.small_potion
skill.basic.strike
```

At load:
- validate uniqueness,
- validate references,
- assign runtime registry IDs internally if useful.

The server must not load image textures to understand a monster definition.

### Content validator

`tools/content_validator` must:
- scan definitions,
- report duplicate IDs,
- report missing required fields,
- report broken references,
- exit non-zero on invalid content.

### Gate 8

Adding a new simple monster requires:
- a new content definition,
- optionally presentation data,

but **no modification of engine C++/Rust core logic** unless the monster introduces genuinely new behavior.

---

# 16. PHASE 9 — COMBAT CORE

Implement generic server-authoritative combat.

Start with:
- health,
- alive/dead state,
- cooldown,
- rectangular hit test,
- basic melee skill,
- basic projectile placeholder,
- one simple monster.

Client sends:
- attack intent / skill activation request.

Server validates:
- alive,
- cooldown,
- range/rules,
- legal skill,
- target/hit.

Server calculates:
- hit,
- damage,
- death.

Client renders result.

### Gate 9

- client cannot decide damage,
- client cannot bypass cooldown by message spam,
- two clients see the same combat result,
- monster death is authoritative.

---

# 17. PHASE 10 — LOOT, INVENTORY AND PROGRESSION

Implement server-owned:
- inventory,
- item stacks,
- equipment slots,
- XP,
- level,
- loot grants.

All operations must be command/transaction-like.

Never trust:
```text
client: "give me item X"
client: "I gained 500 XP"
```

Instead:
```text
server: enemy died
→ loot rules evaluate
→ server creates grant
→ inventory mutation
→ reliable result sent to client
```

### Gate 10

- duplicate-message tests do not duplicate items,
- reconnect tests do not duplicate rewards,
- invalid item IDs are rejected,
- inventory mutations are server-only.

---

# 18. PHASE 11 — PERSISTENCE BOUNDARY

Do not choose a production database prematurely.

Create a storage interface/trait.

Examples:
- load player,
- save player snapshot,
- create player,
- update durable inventory/progression state.

Initial implementation:
- local development persistence,
- simple durable file or embedded database only if justified.

Gameplay code must depend on the storage abstraction, not database SQL.

### Gate 11

- player disconnects,
- server restarts,
- player data reloads,
- replacing the storage implementation does not require rewriting simulation systems.

---

# 19. PHASE 12 — MAPS, ZONES AND INTEREST MANAGEMENT

Implement maps as server-side world partitions.

Initial world model:
- separate maps,
- portals,
- spawn points.

Prepare for:
- multiple instances/channels of the same map.

Do not broadcast every entity to every player.

Implement interest management.

Interest management = deciding which entities/world updates are relevant enough to send to a particular client.

Initial rules may use:
- same map,
- distance / viewport expansion radius.

### Gate 12

- player on Map A does not receive normal entity updates from Map B,
- moving between maps preserves player durable state,
- bandwidth scales with nearby/relevant entities rather than total world population.

---

# 20. PHASE 13 — SCALABILITY HARNESS

The existing `bot_client` now becomes important.

Implement headless simulated clients that can:
- connect,
- authenticate as test users,
- enter map,
- send movement input,
- idle,
- disconnect.

Do not render bot clients.

Build repeatable load scenarios:
- 10 clients,
- 25,
- 50,
- 100,
- then increase only as useful.

Measure:
- server CPU,
- memory,
- tick duration p50/p95/p99,
- outgoing bytes/sec,
- incoming bytes/sec,
- entity count,
- connection count,
- dropped/late simulation ticks.

p50/p95/p99 = percentile measurements showing the median and tail behavior rather than only the average.

### Gate 13

No arbitrary "MMO scale achieved" claim.

Instead create a baseline report:
- machine specification,
- build mode,
- scenario,
- number of clients,
- map/entity count,
- metrics,
- observed bottleneck.

---

# 21. PHASE 14 — SPRITE AND ASSET PIPELINE

Only after the technical core is healthy.

Implement:
- texture loading,
- sprite representation,
- sprite batching,
- sprite atlas,
- presentation registry,
- asset fallbacks.

Missing asset:
- must show a placeholder,
- must not crash authoritative simulation.

### Gate 14

Replace a placeholder rectangle with a sprite without changing:
- networking,
- world state,
- combat rules,
- persistence.

---

# 22. PHASE 15 — ANIMATION AND PAPER DOLL

Paper Doll = layered character rendering where equipment/body/hair/etc. are separate visual layers.

Implement presentation-only:
- body layer,
- equipment layers,
- animation state mapping,
- directional/frame state if required by art.

Authoritative equipment remains server state.

Client Paper Doll reads replicated equipment state and selects visual layers.

### Gate 15

Changing equipped item:
- changes server equipment state,
- replicates to observers,
- updates visual layer,
- does not alter simulation through the sprite system.

---

# 23. PHASE 16 — CONTENT AUTHORING QUALITY

Improve the workflow for adding content.

Before building a full visual editor, first provide:
- schemas,
- validation,
- templates,
- clear error messages,
- live/dev reload only where safe.

A full custom editor is optional and should be justified by content-production pain.

Target author workflow:

```text
Create definition
→ validate
→ run dev server/client
→ content appears
```

Do not require rebuilding Rust for simple stat/statistical content changes.

---

# 24. PHASE 17 — HARDENING

Add only based on observed needs:

- abuse/rate limits,
- authentication hardening,
- secure production certificates,
- crash reporting,
- metrics export,
- database,
- backups,
- migrations,
- deployment automation,
- zone process strategy,
- multi-server orchestration,
- DDoS-aware infrastructure,
- anti-cheat layers.

Anti-cheat starts with architecture:
- server authority,
- validation,
- rate checks,
- impossible-state detection.

Do not start with invasive anti-cheat software.

---

# 25. PERFORMANCE PRINCIPLES

Cursor must preserve these during all phases.

## 25.1 Avoid per-object hidden work
No system should quietly create one expensive task/thread/timer per entity.

## 25.2 Batch where natural
Examples:
- render sprites in batches,
- process entity categories together,
- coalesce network state,
- avoid allocations in hot simulation loops.

## 25.3 Keep simulation data compact
Do not put rendering objects, strings and asset handles into hot authoritative structures without reason.

## 25.4 Reuse buffers where profiling shows allocation pressure
Do not create a custom allocator in advance.

## 25.5 Network newest-state semantics
For transient movement snapshots, stale data is often less useful than newer data.

Do not build a reliable queue that forces old movement state to block new movement state.

## 25.6 Separate persistence from realtime simulation
Database latency must not directly stall the simulation tick.

---

# 26. CONTENT DESIGN PRINCIPLES

When adding a content system, always answer:

1. What part is data?
2. What part is reusable behavior?
3. What part is authoritative simulation?
4. What part is client presentation?
5. Can a content author create a normal variant without editing core source?
6. Can invalid content be detected before players see it?

If the answer to #5 is repeatedly "no", the content boundary is probably wrong.

---

# 27. THINGS CURSOR MUST NOT DO

Do not:

- install or introduce a full game engine,
- add Bevy as an engine,
- switch language,
- add a microservice architecture,
- add Kubernetes,
- build guilds/quests/raids before the core,
- create final art,
- create a full editor in early phases,
- add an ECS library merely for fashion,
- use raw `unsafe` Rust in normal gameplay code without a documented necessity,
- make client state authoritative,
- tie server code to graphics,
- tie simulation to FPS,
- put database queries in the fixed simulation loop,
- optimize based on guesses,
- refactor unrelated systems during a phase,
- push Git changes remotely without instruction.

---

# 28. DEFINITION OF "DONE" FOR EVERY CURSOR PHASE

A phase is done only when all are true:

```text
[ ] Implementation is scoped to the phase.
[ ] cargo fmt passes.
[ ] cargo check --workspace passes.
[ ] cargo clippy --workspace --all-targets --all-features -- -D warnings passes.
[ ] cargo test --workspace passes.
[ ] Relevant runtime/manual test passes.
[ ] No server dependency on graphics/windowing was introduced.
[ ] Documentation was updated if architecture changed.
[ ] Performance measurements were recorded when the phase affects scale.
[ ] TEST_GATES.md was updated.
```

---

# 29. FIRST EXECUTION INSTRUCTION TO CURSOR

When this file is first given to Cursor:

**Begin immediately at Phase 0.**

1. Inspect the current directory.
2. If it is empty, create the `PURGATORY` repository structure.
3. If the directory already contains files, do not delete them blindly; report any conflict with this specification.
4. Initialize Git if needed.
5. Initialize the Rust Cargo workspace.
6. Create all Phase-0 crates and documentation.
7. Implement the Phase-0 bootstrap executables.
8. Create the quality-gate scripts.
9. Run every Phase-0 acceptance check.
10. Fix all failures.
11. Update `docs/TEST_GATES.md`.
12. Make a local Phase-0 commit if Git identity is configured.
13. Then continue to Phase 1 only if Phase 0 is fully green.

Do not ask the owner to manually create the directories listed in this document.  
Directory and bootstrap creation are part of Cursor's task.

---

# 30. RESEARCH BASIS FOR THE STACK DECISION

The stack was selected after checking current documentation and ecosystem status in August 2026.

Primary references:

- Rust ownership and memory safety:
  https://doc.rust-lang.org/stable/book/ch04-00-understanding-ownership.html

- Rust concurrency:
  https://doc.rust-lang.org/book/ch16-00-concurrency.html

- Cargo workspaces:
  https://doc.rust-lang.org/cargo/reference/workspaces.html

- Tokio:
  https://tokio.rs/

- wgpu:
  https://wgpu.rs/

- winit:
  https://docs.rs/winit/latest/winit/

- Quinn:
  https://quinn-rs.github.io/quinn/quinn.html

- Serde:
  https://serde.rs/

- tracing:
  https://docs.rs/tracing/latest/tracing/

Current evidence supporting the selected direction:

- Rust provides memory safety without a garbage collector.
- Rust's ownership/type system moves many concurrency failures to compile time.
- Cargo workspaces are designed for multiple packages developed together with a shared lockfile and build output.
- Tokio is explicitly designed for fast, scalable network applications.
- wgpu provides a safe cross-platform Rust graphics API over native graphics backends.
- winit provides cross-platform window and event-loop management without imposing a game engine.
- Quinn provides QUIC streams plus unreliable application datagrams, allowing critical and transient realtime traffic to use different delivery semantics within one secured connection.
- Serde provides efficient generic serialization without requiring runtime reflection.
- tracing provides structured diagnostics appropriate for asynchronous systems.

---

# 31. FINAL ENGINEERING PRIORITY ORDER

When two tasks compete, use this priority order:

```text
Correct authority model
→ deterministic/stable simulation structure
→ observability and tests
→ networking correctness
→ scalability measurements
→ content extensibility
→ gameplay breadth
→ visual content
→ polish
```

PURGATORY must first become a correct and measurable multiplayer system.

It can look like rectangles for as long as necessary.
