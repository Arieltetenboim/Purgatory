# Architecture Decision Records

ADR = Architecture Decision Record: a short record of an architectural choice, its reason, and consequences.

Status values: `accepted`, `superseded`, `deferred`.

## Process notes

These owner clarifications apply to Phase 0 execution and do not replace the locked ADRs below.

- The current folder is the project root. Do not create a nested `PURGATORY/` directory.
- Leave `Graphic/` exactly where it is. Do not move, rename, modify, import, scan, or connect it to the runtime until later visual-content phases.
- Git is deferred. Do not run `git init`, create commits, or interact with any parent Git repository.

## ADR-0001: Rust 2024

- Status: accepted
- Decision: Use stable Rust, 2024 edition, managed with `rustup` and Cargo.
- Reason: Native performance, memory safety without GC, compile-time concurrency checks, one language for simulation, server, client, protocol, tools, bots, and benchmarks.
- Consequences: The workspace uses a strict compiler. Nightly Rust is not used. Language changes require an explicit owner decision.

## ADR-0002: Custom engine; no Unreal/Unity/Godot/Bevy

- Status: accepted
- Decision: Build a custom engine. Low-level libraries may replace platform boilerplate. Game engines are forbidden unless the owner changes this decision.
- Reason: PURGATORY needs explicit control over authority, simulation timing, networking, and content boundaries.
- Consequences: Windowing uses `winit`. Graphics use `wgpu`. Those libraries are not a game engine.

## ADR-0003: Server-authoritative simulation

- Status: accepted
- Decision: The server owns world state. Clients send intents, not results.
- Reason: Cheating, desync, and duplicated rewards are expensive failure classes in an MMORPG.
- Consequences: Combat, movement, loot, and progression are validated and applied on the server.

## ADR-0004: Fixed simulation timestep

- Status: accepted
- Decision: Simulation rate is independent of render FPS. Initial engineering target is 30 Hz, implemented as integer nanoseconds: `TICK_DURATION_NANOS = 1_000_000_000 / 30` (`33_333_333` ns). Authoritative time is `SimulationTick` plus `SimulationTime = tick_count × TICK_DURATION`.
- Reason: Movement and combat must not change with frame duration. Floating-point absolute time is a poor identity for ticks.
- Consequences: Rendering may later interpolate from the accumulator remainder. Wall-clock time is not used inside gameplay rules. Callers supply elapsed `Duration`. Thirty ticks equal `999_999_990` ns of simulation time; one second of supplied elapsed time still produces exactly thirty ticks, with `10` ns remaining in the accumulator. The 33.33 ms wall interval is not a CPU budget.

## ADR-0005: winit for platform window/input

- Status: accepted
- Decision: The client uses `winit` for native window creation and OS events.
- Reason: Cross-platform window and input without imposing a game engine.
- Consequences: `winit` is a client dependency only. It must not enter `simulation` or `server`.

## ADR-0006: wgpu for graphics backend

- Status: accepted
- Decision: The client uses `wgpu` 30.0.1 for GPU access, with a custom primitive renderer. Game presentation is not built on egui, Bevy, macroquad, pixels, ggez, or SDL. Phase 4.1 adds egui as **client-only development tooling** over this same wgpu surface (see ADR-0016); that does not replace this renderer and is not production UI.
- Reason: Cross-platform native backends (Vulkan / Direct3D 12 / Metal) with a Rust API, without adopting a game engine.
- Consequences: The project builds its own 2D renderer, camera, batching, atlas management, and debug primitives later. Phase 3 still draws colored rectangles only. `winit`/`wgpu` are client dependencies only.

## ADR-0007: Tokio for async service/network I/O, not simulation timing

- Status: accepted
- Decision: Tokio handles asynchronous network and service I/O. Authoritative simulation uses an explicit fixed timestep.
- Reason: Network I/O is asynchronous. Gameplay rules must stay deterministic with respect to ticks.
- Consequences: Async tasks feed and receive queues around simulation. Tokio must not become the simulation clock.

## ADR-0008: Quinn/QUIC initial network transport

- Status: accepted
- Decision: Quinn (QUIC) is the initial transport.
- Reason: TLS 1.3, reliable streams for critical messages, unreliable datagrams for transient realtime state, without writing raw UDP encryption and congestion control first.
- Consequences: The game protocol stays logically separate from transport so transport can be replaced after profiling.

## ADR-0009: JSON authoring format during early content development

- Status: accepted
- Decision: Human-editable JSON is the early content format. Compact binary serialization is for wire messages only after protocol benchmarks.
- Reason: Authors can add definitions without a custom editor. Do not optimize serialization before a measured need.
- Consequences: `serde` / `serde_json` are expected when content loading is implemented. All network messages include protocol/version semantics.

## ADR-0010: No external ECS initially

- Status: accepted
- Decision: Do not add Bevy ECS, Specs, Legion, hecs, or another ECS library in Phase 0.
- Reason: Start with a simple internal entity/world model that is easy to measure.
- Consequences: A formal ECS is introduced only if profiling or system complexity demonstrates a real benefit.

ECS = Entity Component System: an architecture that stores entity data as components and executes logic through systems.

## ADR-0011: Catch-up clamp (spiral-of-death protection)

- Status: accepted
- Decision: One `SimulationClock::advance` call accepts at most `MAX_CATCH_UP = 1 s` of supplied elapsed time. Surplus is discarded and returned on `ClockUpdate::discarded`, not stored in the accumulator. From an empty accumulator this yields `MAX_CATCH_UP_TICKS = 30` ticks. A leftover remainder plus 1 s can yield at most `MAX_TICKS_PER_ADVANCE = 31` ticks so the remainder stays below one tick.
- Reason: A large elapsed-time spike must not enqueue unbounded catch-up ticks. Catch-up work that makes the next update even later is a spiral of death. Capping at 1 s preserves the 30 Hz contract that one second of supplied time produces thirty ticks.
- Consequences: After a hitch longer than one second, the simulation skips the extra wall time instead of replaying it. The world does not jump by the full stall; it advances at most about one second of simulation and continues. Integer truncation means 30 ticks equal `999_999_990` ns; the leftover `10` ns from a 1 s sample stays in the accumulator. This cap may be lowered after gameplay ticks have a real CPU cost. Do not treat discarded time as an error in Phase 1.

## ADR-0012: Client-only custom 2D renderer on winit + wgpu

- Status: accepted
- Decision: Phase 2 draws with a small client renderer built directly on `winit` 0.30.13 and `wgpu` 30.0.1. World space is orthographic and independent of physical pixels. GPU init uses `pollster` on the client only.
- Reason: Prove the native window and GPU path without a game framework and without leaking graphics into simulation or the server.
- Consequences: Workspace `rust-version` is 1.87.0 because wgpu 30's MSRV is 1.87. Phase 4.1 raises it to 1.95.0 for egui 0.36 (ADR-0016). Shaders live under `apps/client/src/renderer/shaders/`. Interpolation, sprites, textures, and camera follow are out of scope. wgpu 30 surface acquisition uses `CurrentSurfaceTexture` (`Success`, `Suboptimal`, `Timeout`, `Occluded`, `Outdated`, `Lost`, `Validation`); present is `Queue::present`. Phase 3 uploads simulation AABBs as colored quads each frame; the renderer does not own movement state.

## ADR-0013: Semantic input and simulation-owned 2D movement

- Status: accepted
- Decision: The client maps `winit` keys to semantic `Action` values and a compact `PlayerInput`. Authoritative placeholder movement (horizontal speed, gravity, grounded jump, AABB platforms) lives in `purgatory-simulation`. Phase 4 stores that state on entities in `World`. The renderer presents resulting AABBs. No physics engine is used.
- Reason: Keyboard scancodes and GPU types must not enter gameplay rules. Movement must be expressed in world units per second so a later clock-rate change (for example 30 Hz → 40 Hz) does not require rewriting speeds. Axis-separated AABB is enough to prove floor, landing, and walk-off without slopes or CCD.
- Consequences: Jump is an edge (`jump_pressed`), not a held-every-frame command. There is no coyote time, jump buffering, double jump, or variable jump height. Phase 3 is local/single-player only. Camera remains static. Numeric tuning (`MOVE_SPEED`, `GRAVITY`, `JUMP_VELOCITY`) is centralized development configuration, not a separate ADR.

## ADR-0014: FOOTNOTE will own platform interaction

- Status: accepted (foundation implemented in Phase 4.5; see ADR-0017)
- Decision: Advanced platform interaction belongs to a **FOOTNOTE** layer inside simulation, not the renderer or input mapping. The reserved boundary is: `Platform` (kind, `top_surface`, policy via `BlockQuery` / `surface_blocks`), overlap **detection** separate from **response**, locomotion steps kept apart, and `grounded_on: Option<EntityId>`. Velocity is retained.
- Reason: Later rules (one-way platforms, drop-through, friction, moving surfaces, ropes/ladders) must plug into policy and response without rewriting keyboard handling or GPU code, and without treating every AABB overlap as a permanent solid collision.
- Consequences: Phase 4.5 implements Solid + OneWay, accel/air control, drop-through, and contact events. Moving platforms, ropes, ladders, slopes, and ice/conveyor are still out of scope. `PlatformKind` remains `#[non_exhaustive]`. Geometric overlap is not a collision decision.

## ADR-0015: Generational EntityId and slot-vector World

- Status: accepted
- Decision: Runtime instances are identified by `EntityId { index, generation }` stored in a `World`-owned generational slot vector with a free list. Lookups require a matching generation. This is not an ECS, not UUIDs, and not pointer identity. `EntityKind` is a small enum (`Player`, `Platform`). Transform is 2D position in world units.
- Reason: Spawn/despawn must be safe. Reusing a slot must not revive a stale handle. Later networking may map `EntityId` to a wire identity; the ID must not be a memory address. Content IDs are a different namespace and are not introduced here.
- Consequences: Stale IDs fail cleanly (`contains` / lookup `None`). `grounded_on` is an `EntityId`; despawned supports are cleared on the next tick. No external ECS crate. EntityId is not serialized on the network in Phase 4. Dummy Monster/Projectile/Loot/NPC kinds are not added until those systems exist.

## ADR-0016: Development overlay uses egui on the existing winit/wgpu client

- Status: accepted
- Decision: Phase 4.1 development diagnostics use `egui` 0.36.1 with `egui-winit` 0.36.1 and `egui-wgpu` 0.36.1, integrated directly into the existing client window. Do not use eframe. Do not add a second native window. egui is **not** the future production game UI.
- Reason: Development needs a reusable in-game inspector (runtime, world, player, later FOOTNOTE/network/entity tools) without adopting a game UI framework or leaking UI crates into simulation/server.
- Consequences: Overlay toggle is the physical Backquote key. Inspection reads a per-frame snapshot. Simulation mutations go through explicit `DebugAction` values. Gameplay keyboard stays live while the overlay is open; egui steals key presses only for text-like widgets. Phase 4.5 adds FOOTNOTE panel fields and overlay-gated in-world gizmos (client presentation only). `purgatory-simulation` and `purgatory-server` must not depend on egui. Workspace `rust-version` is 1.95.0 (egui 0.36 MSRV). A later release profile may compile the overlay out; that flag is not required in this phase.

## ADR-0017: FOOTNOTE movement and contact architecture

- Status: accepted
- Decision: Phase 4.5 places authoritative locomotion in `purgatory-simulation::footnote`. Geometry detects AABB overlap; FOOTNOTE surface policy (`BlockQuery` / `surface_blocks`) decides blocking for `Solid` and `OneWay`; the FOOTNOTE controller owns acceleration, air control, jump, gravity, contact transitions (`ContactEvent`), and drop-through (`ignored_platform`). Tuning lives in `FootnoteConfig` (units/s and units/s²). Client maps `down_held`; drop-through is derived as Down+Jump on OneWay only.
- Reason: Side-scroller feel and future MapleStory-like traversal need continuous velocity, distinct ground/air rates, and non-solid platforms without merging movement into the renderer or a physics engine.
- Consequences: Snap-to-`MOVE_SPEED` is gone. Momentum survives jump and walk-off. OneWay is not a wall; Solid still blocks sides/undersides. Downward landings (Solid and OneWay) require a top-surface crossing from above — geometric overlap alone is not a land. Vertical resolution applies **one** nearest valid surface per axis step so overlapping platforms cannot chain-snap / teleport the player. `ignored_platform` is temporary for the originating drop only: it clears when the player collider is fully below that platform’s **top/support** (not the platform bottom / floor), and also clears on any subsequent landing. Stale `EntityId`s for grounded/ignored platforms clear safely. No FOOTNOTE network serialization in this phase. Server runs the same tick headlessly.

## ADR-0018: FOOTNOTE development test arena is hard-coded

- Status: accepted
- Decision: Phase 4.6 ships `World::footnote_test_stage()` as a single hard-coded development arena (Solid + OneWay platforms, descent stack, multi-drop, freestyle, momentum, stepped slope approximation). Client and server instantiate this stage. Compact `World::dev_stage()` remains for small unit tests.
- Reason: Manual FOOTNOTE validation needs a richer layout without introducing map loading, content JSON, camera follow, or slope physics.
- Consequences: Static camera logical height may be enlarged for this stage only (`FOOTNOTE_TEST_VIEWPORT_HEIGHT`). Phase 4.8 adds player-follow camera + world bounds (presentation clamp + simulation player containment) without introducing map loading. No rotated colliders. No Phase 5 networking. Reset Player returns to P0 spawn without rebuilding the world.

## ADR-0019: Camera, world bounds, and local debug time scale

- Status: accepted
- Decision: Phase 4.8 separates **world bounds** (simulation/`World`) from **camera** (client presentation). The camera follows the local player and clamps to `WorldBounds` so the viewport does not show outside-map void when the world is larger than the view. Procedural parallax layers are client-only (`offset = camera * factor`). Debug time scale (1.0 / 0.5 / 0.25) multiplies wall elapsed before `SimulationClock::advance`; fixed `TICK_DURATION` / 30 Hz are unchanged.
- Reason: Camera follow and boundary tests need a wider map and explicit bounds without coupling simulation to rendering. Slow-mo is a local development harness, not a physics retune.
- Consequences: Server remains headless and unaware of camera/parallax/egui. Once authoritative networking exists, local time scale must not claim to slow the server — keep it offline/local or later make it server-controlled. No free-look camera keyboard in this phase.

## ADR-0020: Crossing-based collision vs exceptional recovery

- Status: accepted
- Decision: Phase 4.7 splits **normal collision** (travel this tick crossed a blocking surface; one nearest candidate per axis) from **depenetration recovery** (start-of-tick meaningful Solid penetration; MTV, capped). Normal horizontal resolution must not shove to nearest/far faces on stale overlap. `CONTACT_EPSILON` (0.001 wu) is the shared touch-vs-penetrate tolerance.
- Reason: Underside head-bonk + residual horizontal velocity against freestyle Solid Entity 16 produced ±1.6 teleports via nearest-face depenetration without an X crossing.
- Consequences: Invalid embeds may briefly remain until recovery or vertical resolution; intentional recovery covers bad spawn/debug cases. Discontinuity detector defaults OFF; console logging is a separate client toggle. No physics engine.

## ADR-0021: Quinn/QUIC networking foundation (Phase 5.0)

- Status: accepted
- Decision: Client and dedicated server connect with Quinn (QUIC) over Tokio. Control messages use a client-opened bidirectional stream with explicit `u32` little-endian length prefix (`MAX_CONTROL_MESSAGE_BYTES = 4096`). Handshake is versioned Hello/Welcome (`PROTOCOL_VERSION = 1`). The server assigns `ConnectionId`. Development TLS is a startup-generated self-signed cert plus a client verifier named `DevOnlySkipServerVerification`. RTT uses datagram Ping/Pong carrying a nonce only; the client measures `Instant` locally. Wire `DisconnectReasonCode` values are peer/server-sent only; local connect/transport/close errors are `LocalConnectionError`. UI→network commands use bounded `tokio::sync::mpsc` (`try_send` from winit, `recv` on Tokio). Network→UI is bounded and non-blocking (`try_recv` on the render thread). No gameplay replication in this phase.
- Reason: ADR-0008 already selected QUIC. Phase 5.0 must prove a live session, version rejection, and a debug Network tab without introducing client-authoritative movement or mixing local errors into the wire enum.
- Consequences: `purgatory-protocol` stays free of Quinn/Tokio. Simulation stays free of networking. Packet arrival does not modify `World`. Production PKI, auth, rate limits, and movement snapshots are later phases. Do not start Phase 5.1 from this ADR.
