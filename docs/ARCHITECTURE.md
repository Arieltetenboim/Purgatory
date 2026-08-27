# Architecture

PURGATORY is a custom 2D side-scrolling MMORPG engine. This document records the structural rules. The master execution specification remains `PURGATORY_CURSOR_MASTER_EXECUTION_PLAN.md`.

## What is being built

- Native desktop client first.
- Dedicated headless server.
- Server-authoritative realtime simulation.
- Shared protocol, content, and common crates.
- Data-driven content added without rewriting core engine systems.

## What is not being built yet

Phase 4 is the world/entity foundation. Phase 4.1 adds a client-only development debug overlay. Phase 4.5 adds the FOOTNOTE movement foundation. Phase 4.6 expands the hard-coded FOOTNOTE development test arena. Phase 4.8 adds camera follow, world bounds, parallax background, and debug harness improvements. Phase 5.0 adds the Quinn/QUIC session foundation (handshake, ConnectionId, RTT, Network debug tab) **without** gameplay replication. Later phases add authoritative multiplayer movement, combat, persistence, maps, and presentation.

Do not add in early phases:

- Unreal, Unity, Godot, Bevy, or another game engine
- final sprites, animation polish, VFX, sound, or final UI
- quests, guilds, raids, housing, marketplace, premium currency
- plugin systems, scripting VMs, clusters, custom allocators, or job systems

## Authority

The server owns authoritative world state.

Clients send intents and inputs. Clients never send authoritative results such as position, damage, loot grants, or XP.

Presentation (sprites, Paper Doll, animation, effects) represents state. It does not define authoritative state.

## Simulation

Timing lives in `crates/simulation`. The public clock API is `SimulationClock`.

Gameplay simulation uses an explicit fixed timestep, independent of render FPS. The initial rate is **30 Hz**. One tick is `1_000_000_000 / 30` nanoseconds (`33_333_333` ns, about 33.333 ms). That interval is the *spacing* between ticks, not a CPU budget.

Authoritative time is discrete:

- `SimulationTick` is the completed tick count.
- `SimulationTime` is `tick_count × TICK_DURATION`.
- Gameplay code must not read `std::time::Instant` or other wall-clock sources.
- The outer loop (server, client, tests) supplies elapsed `Duration` values.

Accumulator:

```text
supplied elapsed time
→ clamp to MAX_CATCH_UP (1 s of supplied elapsed time)
→ add accepted time to the accumulator
→ while accumulator >= tick duration: execute one tick and subtract one step
→ remainder stays in the accumulator
```

Surplus wall time above the catch-up cap is discarded and reported on `ClockUpdate::discarded`. It is not queued for later ticks. Remainder below one tick is retained; it is not interpolation yet.

Tokio is for asynchronous network and service I/O. It is not the gameplay simulation clock.

The server must never require a window, renderer, or GPU. Phase 5.0 keeps a headless 30 Hz `World` ticker independent of packet arrival. Connecting or disconnecting does not change tick rate.

## Networking (Phase 5.0)

Transport is Quinn/QUIC. The game protocol is in `purgatory-protocol` (framing, Hello/Welcome, ConnectionId). Quinn types live only in `apps/server/src/network/` and `apps/client/src/network/`.

```text
winit thread                         purgatory-net Tokio thread
NetworkCommand  mpsc try_send   →    recv() in select!  → Quinn
NetworkEvent    mpsc try_recv   ←    try_send
```

The render loop never `.block_on`s network IO. Async tasks must not call `World::tick`.

**Trust:** all client bytes are untrusted. Parse and validate before any session insert. See [`PROTOCOL.md`](PROTOCOL.md).

Development listen address: `127.0.0.1:5001`. Handshake timeout 5 s. Datagram ping ~1 s, nonce only.

`ConnectionId` (network session) is not `EntityId` (simulation instance). Phase 5.0 does not spawn remote players or send movement.

## Coordinate convention

World space is 2D:

- **+X = right**
- **+Y = up**

Gravity decreases `velocity.y`. One world unit is not one pixel. Speeds are world units per second, integrated as `speed × dt` where `dt` is the fixed tick duration (`TICK_DURATION.as_secs_f32()`), never a render-frame delta.

## Input abstraction

Raw keyboard codes stay in the client (`apps/client/src/input.rs`).

```text
winit KeyCode
→ Action (MoveLeft / MoveRight / MoveDown / Jump)
→ PlayerInput { move_axis: -1|0|1, jump_pressed, down_held }
→ World::tick(dt, input)  // FOOTNOTE
```

`move_axis` and `down_held` are held state. `jump_pressed` is an edge consumed once per simulation tick. Temporary development bindings: A / Left Arrow → MoveLeft, D / Right Arrow → MoveRight, S / Down Arrow → MoveDown, Space → Jump. **Down + Jump** while grounded on a OneWay platform drops through (FOOTNOTE); otherwise Jump is a normal grounded jump.

The physical Backquote / Grave key (`~`) is **not** a gameplay action. It toggles the client development debug overlay and is never stored in `PlayerInput`.

`purgatory-simulation` has no `winit`, `wgpu`, or `egui` dependency.

## World and entities (Phase 4)

`World` is the authoritative simulation container. It is not a global registry.

Runtime identity is [`EntityId`]: `{ index, generation }`. It is cheap to copy. The index is a storage slot, not permanent identity. After despawn, the slot’s generation advances; a later occupant in the same slot has a different `EntityId`. Lookups with a stale ID return `None`. IDs are not memory addresses, Rust references, or UUIDs. They are not serialized on the network in this phase.

**EntityId vs Content ID:** `EntityId` is a temporary runtime instance. A future content ID (for example `monster.slime.green`) is a stable authored definition. Two green slimes will share one content ID and have two different `EntityId`s. Do not mix the two.

Storage is a generational slot vector plus a free list. Live records sit in a contiguous `Vec`. Vacant slots are reused. This is not an ECS and not a general object graph.

`EntityKind` is currently `Player` or `Platform` only. Monster, projectile, loot, and NPC kinds are added when those systems exist.

Every entity has a 2D [`Transform`] (`position`, **+X right, +Y up**). Players also store velocity, grounded, `grounded_on: Option<EntityId>`, and collider half-extents. Platforms store half-extents and `PlatformKind`. Velocity is not forced onto platforms.

`grounded_on` names a platform `EntityId`. If that platform is despawned, the next tick clears grounding and the player falls. Stale IDs are never dereferenced as live records.

Iteration is ordinary loops (`iter`, `iter_kind`, `iter_platforms`). No thread, task, or timer per entity.

The renderer copies AABBs from `World` each frame. It does not own lifecycle and does not keep a second entity database.

## Local movement (FOOTNOTE)

`World` in `crates/simulation` owns the player and platform entities. One fixed tick is orchestrated by **FOOTNOTE** (`crates/simulation/src/footnote/`). Player state always retains **velocity**; position changes only by integrating `velocity × dt` on each axis.

```text
clear stale EntityIds
→ drop-through (OneWay + down_held + jump_pressed) or jump
→ horizontal accel / ground decel / air control
→ gravity
→ integrate X → respond X
→ integrate Y → respond Y (top-surface crossing for OneWay)
→ grounded / ContactEvent / expire ignored platform
```

Three-way boundary:

```text
geometry (AABB overlap)
→ platform / FOOTNOTE surface policy (Solid vs OneWay, BlockQuery)
→ FOOTNOTE movement (accel, momentum, contact, drop-through)
```

- Horizontal motion uses acceleration toward `move_axis × max_speed` (ground vs air rates differ). Idle on ground decelerates via ground friction; air idle preserves horizontal momentum.
- Jump: if grounded and not dropping, set `velocity.y = jump_velocity`. Horizontal velocity is not cleared.
- Gravity: `velocity.y -= gravity * dt`.
- Collision **detection** reports AABB overlap only.
- Collision **response** (Phase 4.7) is **crossing-based normal collision**: previous AABB → proposed AABB must cross a blocking face. One nearest valid crossed surface is selected per axis (order-independent). Stale overlap without a crossing is **not** an ordinary wall hit.
- **Recovery** is a separate exceptional path when the tick begins with meaningful Solid penetration (MTV, capped). It must not run as a silent fallback inside normal resolution.
- Contact uses centralized [`CONTACT_EPSILON`](../crates/simulation/src/contact.rs) (0.001 wu): touching within tolerance is not penetration. Ceiling snaps separate slightly so flush underside contact does not re-enter horizontal resolve.
- FOOTNOTE surface policy unchanged: OneWay is never a wall; Solid may block sides/undersides when crossed.
- `grounded_on: Option<EntityId>`, `ignored_platform` for drop-through, `last_contact: ContactEvent` (None / Landed / LeftGround).

Dev stage platforms: Solid floor, Solid raised, two OneWay platforms. No map loading.

The renderer reads `World` AABBs and converts them through the existing orthographic camera. It does not store an authoritative copy of player position.

## FOOTNOTE

**FOOTNOTE** is the authoritative player/platform movement and contact layer inside `purgatory-simulation`.

Phase 4.5 owns:

- acceleration / deceleration and distinct air control
- momentum preservation across jump and walk-off
- Solid vs OneWay policy
- landing / leaving-platform `ContactEvent`
- drop-through via temporary `ignored_platform` (clears once the collider is fully below that platform’s top/support; also clears on any landing)

It does **not** yet own: moving platforms, ropes, ladders, slopes, ice/conveyor surfaces, wall-jumps, dash, CCD-as-engine, content JSON tuning, or network serialization.

FOOTNOTE must not live in renderer or input mapping. Server Phase 6 will run the same simulation headlessly.

Do not assume every platform is permanently solid from every direction. Geometric overlap is not automatically a collision.

## FOOTNOTE development arena

`World::footnote_test_stage()` is a hard-coded laboratory (not a map loader) with `WorldBounds` (Phase 4.8: roughly twice the Phase-4.6 horizontal span). Regions stay separated by empty space: main Solid/OneWay route, descent, multi-drop, freestyle, momentum/edge, slope approximation, isolated overlap regression. Compact `World::dev_stage()` remains for unit tests.

## World bounds

`WorldBounds { min_x, max_x, min_y, max_y }` lives on `World`. The player is clamped at horizontal (and soft vertical) edges with outward velocity zeroed — no teleport correction. Falling well below `min_y` triggers a **development** respawn at stage spawn (not a final death system). Camera clamping is presentation-only and reads the same bounds.

## Client presentation

Rendering lives only in `apps/client`. The client uses `winit` 0.30.13 and `wgpu` 30.0.1.

```text
platform events (including keyboard)
→ Backquote/~ handled as client debug toggle (never PlayerInput)
→ if overlay open: feed event to egui
→ semantic ActionState unless a text-like egui widget owns the keyboard
→ collect elapsed Duration × debug_time_scale
→ SimulationClock::advance(scaled_elapsed)
→ for each executed tick: World::tick(fixed_dt, PlayerInput)
→ update follow camera; clamp to WorldBounds
→ draw far/mid/near parallax quads → world AABBs → optional FOOTNOTE gizmos
→ optional development egui overlay
```

Rendering continues when zero simulation ticks are due. Frame delta is never the simulation step. Movement executes only inside those fixed ticks.

Logical world coordinates are independent of physical pixels. The FOOTNOTE arena uses a taller logical viewport height (`FOOTNOTE_TEST_VIEWPORT_HEIGHT`). Phase 4.8 camera **follows the local player** and clamps so the viewport stays inside world bounds when the world is larger than the viewport.

**Parallax** (far ≈ 0.15, mid ≈ 0.40, near ≈ 0.70) is presentation-only: `layer_offset = camera_position * factor`. Simulation entities, collision, FOOTNOTE, and the server are unaware of background layers.

On this development machine the client selected:

- adapter: NVIDIA GeForce RTX 4050 Laptop GPU
- backend: Vulkan
- format: Bgra8UnormSrgb
- initial size: 1280×720

Zero-sized / minimized surfaces skip configure and draw. `Lost` / `Outdated` / suboptimal surfaces reconfigure. wgpu 30 reports these through `CurrentSurfaceTexture` rather than the older `SurfaceError` enum; there is no `OutOfMemory` variant on that enum. Validation failures exit. Timeouts skip the frame.

## Development debug overlay (Phase 4.1 / 4.8)

The client hosts an in-window **development** overlay. It is not production game UI and not a second OS window.

- Toggle: physical Backquote / Grave (`~`). Press opens; press again closes. OS key-repeat does not toggle. The egui close button also hides it; `~` reopens it.
- Technology: `egui` 0.36.1 + `egui-winit` 0.36.1 + `egui-wgpu` 0.36.1, integrated directly with the existing winit/wgpu client. **eframe is not used.**
- Crate boundary: egui crates are `purgatory-client` dependencies only. They must not enter `purgatory-simulation`, `purgatory-server`, `purgatory-protocol`, `purgatory-content`, or `purgatory-common`.
- Tabs: **Runtime**, **Player**, **FOOTNOTE**, **World**, **Camera**, **Diagnostics**, **Network**.
- Runtime includes development **time scale** (1.0 / 0.5 / 0.25): scales wall-clock elapsed fed to `SimulationClock` only. `TICK_RATE_HZ` is unchanged. Local-client only; once authoritative networking exists this cannot independently slow the server.
- Gizmo toggles: colliders, velocity, grounded highlight, world bounds, grid, parallax debug.
- Camera tab: position, viewport, follow checkbox, center-on-player.
- Mutation: `DebugAction` values (currently `ResetPlayer`). egui must not poke arbitrary `World` fields.
- Input: movement stays live while the overlay is open except when a text-like egui widget owns key presses.
- Render order: parallax → world primitives → FOOTNOTE/debug gizmos → egui (`LoadOp::Load`).

`Reset Player` returns the test player to the development spawn, clears velocity / ignore / last contact, and restores floor grounding when the floor entity exists. EntityIds are not changed.

## Repository boundaries

This directory is the project root. Do not nest another `PURGATORY/` directory.

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
client / server / bot_client

common
  ↑
content
  ↑
server / client / content_validator
```

`simulation` must not depend on `winit`, `wgpu`, renderer code, UI, or OS window APIs.

`server` must not depend on `winit`, `wgpu`, `egui`, or the client crate.

The existing `Graphic/` directory stays in place and is unused by the runtime until the sprite and Paper Doll phases. Phase 3 still draws colored rectangles only; no textures, sprites, or logos are loaded.

## Content

Common content variants are data.

Authoring flow:

```text
author data
→ validate
→ load into runtime registry
→ map author-facing string ID to runtime ID
→ simulation uses definition
→ client maps presentation references to assets
```

Adding a normal monster or item must not require editing engine source unless the content introduces genuinely new behavior.

## Ownership and processing

Every major object and resource has a clear owner and lifetime.

Avoid a default per-entity update loop. Prefer batched, system-level processing and event/timer driven work.

Measure before optimizing. Scalability is planned early; optimization is justified by profiling.

Git is deferred. This repository currently has no project-local Git integration.
