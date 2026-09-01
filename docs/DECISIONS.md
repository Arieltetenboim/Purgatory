# Architecture Decision Records

ADR = Architecture Decision Record: a short record of an architectural choice, its reason, and consequences.

Status values: `accepted`, `superseded`, `deferred`.

## Process notes

These owner clarifications apply to Phase 0 execution and do not replace the locked ADRs below.

- The current folder is the project root. Do not create a nested `PURGATORY/` directory.
- Leave `Graphic/` exactly where it is. Do not move, rename, modify, import, scan, or connect it to the runtime until later visual-content phases, except the Phase 5.0B Connection Frontend loading **only** `Graphic/LOGO.png` (temporary path).
- Phase 0 deferred Git (`git init` was not run then). The workspace is now a Git repository at this project root. Do not treat a parent folder as the project repo.

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
- Decision: Client and dedicated server connect with Quinn (QUIC) over Tokio. Control messages use a client-opened bidirectional stream with explicit `u32` little-endian length prefix (`MAX_CONTROL_MESSAGE_BYTES = 4096`). Handshake is versioned Hello/Welcome (`PROTOCOL_VERSION = 1`). The server assigns `ConnectionId`. Development TLS is a startup-generated self-signed cert plus a client verifier named `DevOnlySkipServerVerification`. RTT uses datagram Ping/Pong carrying a nonce only; the client measures `Instant` locally. Wire `DisconnectReasonCode` values are peer/server-sent only; local connect/transport/close errors stay off the wire. UI→network commands use bounded `tokio::sync::mpsc` (`try_send` from winit, `recv` on Tokio). Network→UI is bounded and non-blocking (`try_recv` on the render thread). No gameplay replication in this phase.
- Reason: ADR-0008 already selected QUIC. Phase 5.0 must prove a live session, version rejection, and a debug Network tab without introducing client-authoritative movement or mixing local errors into the wire enum.
- Consequences: `purgatory-protocol` stays free of Quinn/Tokio. Simulation stays free of networking. Packet arrival does not modify `World`. Production PKI, auth, rate limits, and movement snapshots are later phases. Phase 5.0D (ADR-0024) unifies local failures as `NetworkFailureKind`. Do not start Phase 5.1 from this ADR.

## ADR-0022: Connection Frontend and client connection lifecycle (Phase 5.0B)

- Status: accepted
- Decision: The desktop client starts on an in-window Connection Frontend (`ClientScreen::Connection`) with `ConnectionState::Disconnected`. Connect is explicit. Each attempt has a client-local `ConnectionAttemptId` (not on the wire). Network events carry that id; stale ids are ignored. `Connected`/Welcome is legal only from `Handshaking` of the active attempt. Disconnect retires the attempt immediately on the client so a delayed Welcome cannot enter Game. Gameplay input and `SimulationClock` advance only on `ClientScreen::Game`. The Connection Frontend is `ConnectionFrontend::paint`, separate from `DebugOverlay::paint`, sharing one egui context. `Graphic/LOGO.png` is loaded once into a texture handle via a path helper. Bounded mpsc remains; skipping RTT near capacity is a mitigation, not a guarantee.
- Reason: Phase 5.0 auto-connected into the game/debug screen. That mixed presentation with connection progress and allowed delayed events to mutate newer attempts.
- Consequences: No login/auth/channel/character UI. No protocol messages added. No gameplay replication. Phase 5.0C provides lossless (priority) delivery for lifecycle-critical events.

## ADR-0023: Connection lifecycle and race hardening (Phase 5.0C)

- Status: accepted
- Decision: Lifecycle-critical network events travel a dedicated bounded channel and are not silently dropped under RTT pressure. Telemetry (`RttUpdated`) is a separate droppable channel. Connect remains a bounded `try_send` command queue. Disconnect/Shutdown bump a watch control plane (disconnect epoch + shutdown flag) so they cannot be stranded behind Connect pressure. Each Connect command stamps the epoch at send time; a later Disconnect cancels that attempt without being cleared by a newer Connect. The client Tokio thread owns one session task at a time. The server accept loop spawns a bounded per-connection task (cap 32, else `Incoming::refuse`) and removes sessions exactly once via `SessionLease` (`std::sync::Mutex` so Drop can lock). Outstanding ping nonces are capped at 4 (drop oldest). Stale/unknown/duplicate Pongs are ignored. Invalid `ConnectionState` transitions are ignored. `ConnectionId` remains a monotonic server `AtomicU64`; wraparound is out of scope.
- Reason: A single mixed event queue could drop Welcome/Disconnect under telemetry saturation. A single command queue could drop Shutdown behind Connect spam. Handshake-on-accept would serialize clients. Session remove only at the happy path could leak ids after panic.
- Consequences: No soak beyond modest churn (5.0F). No gameplay replication (5.1). True OS process-kill cleanup waits for the explicit idle timeout.

## ADR-0024: Network diagnostics and failure semantics (Phase 5.0D)

- Status: accepted
- Decision: Map every network outcome to `NetworkFailureKind`. Keep `DisconnectReasonCode` as the only on-wire rejection vocabulary. Local-only kinds (ConnectFailed, TransportLost, IdleTimeout, ClientRequestedDisconnect, LocalShutdown, InternalNetworkError) are never serialized. Retryability is a property of the kind, not an auto-reconnect loop. One frontend-status function serves Connection Frontend and debug. Quinn errors are classified at the runtime boundary via `TransportSymptom`. Diagnostic history is a 48-slot ring of lifecycle/failure events; RTT is statistics only (latest/min/max/EWMA). Logging toggles default OFF. `IDLE_TIMEOUT` is 15 s in `purgatory-protocol` and applied to Quinn `max_idle_timeout` on both client and server. Handshake timeout remains 5 s. Graceful dedicated-server exit closes with `ServerShutdown`. Wire details are bounded labels, not paths or panic text.
- Reason: 5.0C made lifecycle converge; operators still could not tell ConnectFailed from TransportLost from a manual disconnect, and idle timeout was an implicit Quinn default.
- Consequences: No packet inspector. No AFK timeout. No map/channel/instance system. `ConnectionId` ≠ `EntityId` ≠ future `MapInstanceId` (documented only). Phase 5.0E adds abuse policy. Do not start 5.0F or 5.1 from this ADR.

## ADR-0025: Network security and abuse foundation (Phase 5.0E)

- Status: accepted
- Decision: Treat every remote peer as untrusted after QUIC connect and after Hello/Welcome. Centralize development limits in server-only `NetworkAbuseConfig`. Validate control-frame length before allocating payload. Restrict handshake to one bidi stream and one Hello; insert `SessionTable` only after a valid Welcome write. Cap concurrent handshake/session tasks (default 32) and refuse excess. Per-connection bounded counters: malformed complete control tags (budget), invalid datagrams (budget), oversized frames (immediate close), control-message rate (server `Instant` token window). Wire reject text is generic. Parser tests include truncation/oversize matrices and a deterministic random corpus. Mutexes are not held across `.await`; poison recovers with `into_inner`. DEV-only skip-verify remains explicitly named.
- Reason: 5.0D made failures observable; peers could still waste tasks, streams, and decode work without a single policy object or budgets.
- Consequences: Not a DDoS product, IP ban list, or gameplay anti-cheat. Phase 5.1 must still validate semantic gameplay. Phase 5.0F soaks this policy; do not start 5.1 from this ADR.

## ADR-0026: Convergence-to-baseline as the network stability criterion (Phase 5.0F)

- Status: accepted
- Decision: Judge the network foundation by convergence, not survival. Separate **active gauges** (`active_sessions`, `active_handshakes`, `inflight_connection_tasks`, admission permits in use) from **cumulative counters** and **high-water marks** (`max_sessions`, `max_inflight`, `max_handshakes`); after any bounded soak/stress/chaos scenario, gauges must return to baseline and a healthy probe (connect → Welcome → ping/pong → clean disconnect) must pass. Assert convergence with a bounded polling helper (`wait_until_baseline`) that fails with a gauge and counter report, never with a bare sleep. Drive chaos from fixed seeds through a test-only operation vocabulary and report the seed and operation index on failure. Keep the CI suite short and move heavier soaks behind `#[ignore]` plus `scripts/network_soak.ps1`.
- Reason: A server that merely does not crash under pressure can still leak sessions, permits, tasks, or stale state. The load-bearing property before gameplay networking is that pressure leaves no residue. Sleep-based assertions and entropy-seeded chaos would make that property unfalsifiable.
- Consequences: Localhost soak results are explicitly **not** capacity claims; capacity requires gameplay state, replication, AI, persistence, combat, and bandwidth profiling. Limits are never raised to make a soak pass, and no production sleeps are added — a failing soak means a root cause to find. The soak surfaced two diagnostic classification fixes (graceful client close is never transport loss; a zero-length frame is malformed). The chaos runner is test-only and must not ship in the server runtime. Phase 5.1 is a later ADR.

## ADR-0027: Authoritative intent-only input (Phase 5.1)

- Status: accepted
- Decision: Bump `PROTOCOL_VERSION` from 1 to 2 and add `InputCommand` `{ sequence: u32, move_axis, jump_pressed, down_held }`. The client never sends position, velocity, grounded state, collision results, platform id, `EntityId`, or `ConnectionId` as identity. The server maps `ConnectionId → EntityId`, owns FOOTNOTE movement, and despawns on disconnect/loss. Sequence is a no-wrap-within-session `u32` (first accept, duplicate ignore, stale ignore). Held state is latest-wins; jump is a coalesced one-shot edge. Connection tasks hand off through bounded channels; they never lock `World`. Gameplay input has its own rate policy, separate from control-message rate. Local client FOOTNOTE may remain for development visuals and is marked non-authoritative. No snapshots, prediction, or remote players.
- Reason: Phase 5.0 proved sessions. Gameplay authority must start with intent, not client-computed physics, and must reuse existing FOOTNOTE rather than a second movement path.
- Consequences: Old protocol clients are rejected. Two clients still do not see each other. Production anti-cheat, accounts, channels, and MapInstance are later.

## ADR-0028: Authoritative world snapshots (Phase 5.2)

- Status: accepted
- Decision: Bump `PROTOCOL_VERSION` from 2 to 3 and add full `WorldSnapshot` `{ snapshot_sequence, server_tick, local_player_entity, entities[] }` on a server-initiated unidirectional stream. Bounds are independent (`MAX_GAMEPLAY_SNAPSHOT_BYTES`, `MAX_ENTITIES_PER_SNAPSHOT`). Decode validates count then allocates. SnapshotBuilder reads World; network tasks never lock World. Per-connection `watch` coalesces to latest. Client `ReplicatedWorld` applies atomically; renderer uses replica positions. Static map geometry is not replicated. No interpolation, prediction, or reconciliation.
- Reason: Two clients must see the same server-owned players before prediction exists. Full snapshots keep despawn/generation correct without delta complexity.
- Consequences: Motion looks stepped until Phase 5.3. v1/v2 goldens stay frozen. Future MapInstance/interest management replaces the Phase 5.2 visibility set.

## ADR-0029: Remote entity interpolation (Phase 5.3)

- Status: accepted
- Decision: Add client-only remote interpolation from a bounded snapshot history on a delayed monotonic server-tick timeline. Protocol version stays **3** (no wire change). Delay = 3 snapshot intervals (`INTERPOLATION_DELAY_TICKS`). History cap = 16. Local player was direct authoritative replica rendering in 5.3 (superseded for local presentation by ADR-0030). Remotes lerp between bracket samples; no extrapolation (hold on underrun). Estimated/render ticks never move backward under arrival jitter. Despawn/spawn follow the render timeline, not packet arrival. Generation reuse never cross-lerps. Snap distance 8 wu is presentation tuning in `interp.rs` only. Server snapshot cadence and latest-wins architecture unchanged.
- Reason: 5.2 remotes look stepped. Smoothing belongs in client presentation so the server stays scalable and authority stays intact before prediction (5.4).
- Consequences: Remotes are intentionally ~100 ms behind estimated server time. Diagnostics expose delay, depth, brackets, alpha, holds, snaps.

## ADR-0030: Local player prediction (Phase 5.4)

- Status: accepted
- Decision: Add client-only local prediction using the shared FOOTNOTE simulation (`World::tick` / same movement rules and stage geometry). Protocol stays **3**. `LocalPrediction` owns lifecycle metadata; predicted body lives on the client `World` player and never writes into `ReplicatedWorld`. Render and camera use `local_presentation_pose` (predicted when active). Remotes remain on Phase 5.3 interpolation. **Lead error** (pred-now vs latest lagged auth) is expected RTT×speed and is **not** treated as desync. **Aligned residual** is measured by searching the predicted history ring for the sample that best matches the snapshot in position *and* velocity (velocity scaled by one tick), restricted to `PREDICTION_MAX_PLAUSIBLE_LAG_TICKS` (16) so a pose abandoned seconds ago cannot pose as lag. A near-zero residual at some offset means temporal lead/lag and is left alone; what no plausible offset explains is divergence. The earlier `client0 + (server − server0)` tick map is removed: it assumed zero pipeline delay, so it reported raw lead as same-tick error (runtime logs showed lead ≡ aligned and a pending-input count pinned at 0). Re-anchor uses aligned residual ≥ 8 wu, lead ≥ 8 wu as last-resort safety, settled-auth vertical path-split (`|vy|≤0.15` and aligned `|dy|≥2`), plus first-local / generation / clear. Below those, a residual ≥ `PREDICTION_ALIGNED_RESIDUAL_WU` (0.05) held for `PREDICTION_DIVERGENCE_SNAPSHOTS` (4) snapshots applies a **drift correction**: the body is shifted by the measured divergence, keeping the lead, instead of teleporting onto the older authoritative pose. Raw distance never triggers a correction at any threshold. **InputCommand is emitted on the same fixed sim tick that applies that `PlayerInput` to prediction** (keydown only latches `ActionState`; focus-loss still flushes Neutral). Ordinary RTT lead is not soft-smoothed. No pending-input replay, no ack rewind, no client-position messages, no remote prediction.
- Reason: Local responsiveness must not wait on snapshot RTT, without weakening server authority or prematurely building Phase 5.5 reconciliation. Tick-correct compares prevent hard-snaps driven by phase lag alone. Tick-aligned intent prevents systematic ±1 tick press/jump/release offsets that diverge collision/grounded paths.
- Consequences: Visible orange/auth lag behind cyan/predicted is normal lead. Residual **aligned** error from network RTT / unacked inputs is the intended Phase 5.5 reconciliation input — not for masking deterministic input-timing bugs. Because `InputCommand` carries no tick stamp, an input held for N client ticks is applied for N±1 server ticks depending on arrival phase (measured: 194 of 1069 intervals, deltas ±1 in equal numbers); the resulting sub-tick position error is what drift correction bounds. Per-tick input acknowledgement / replay in Phase 5.5 removes the error at its source. Diagnostics expose five separate metrics: raw lead, best temporal offset, aligned residual, consecutive residual-divergence count, correction reason. Optional prediction gizmos default OFF.

## ADR-0031: Authoritative input acknowledgement and local reconciliation (Phase 5.5)

- Status: accepted
- Decision: Bump `PROTOCOL_VERSION` from 3 to 4. The client emits one `InputCommand` per predicted `SimulationClock` step, identified by `(input_epoch, sequence)`. The server always runs `tick_player` at 30 Hz: **Consumed** (pop one queued command) or **Continuation** (empty queue: last held axis/down, `jump_pressed = false`). After Continuation starvation, **late-collapse** compact a prefix of delayed commands into one `tick_player` (latest held + `jump_pressed` OR) and acknowledge that prefix. Continuation debt (`unmatched_continuation_ticks`) saturates and never wraps; remainder is `K - N`. `last_acknowledged_input_sequence` is the accounted replay boundary, not one physics step per sequence. Client reconcile: restore durable FOOTNOTE state (`PlatformSupportId` contact; `last_contact` is transient and not on the wire), drop commands `<= ack`, replay remaining via `tick_player` / `tick_predicted_player`. Hitch (`ticks_executed > 1`) is a prediction discontinuity: clear pose history, keep pending, restore+replay, emit one new command. Focus-loss releases `ActionState` immediately; the normal path is one forced Neutral clock step; a full send window sends `HeldCancel` with an **immutable** `(epoch, target_sequence)` barrier captured at send — do not re-read mutating last-sent. Server HeldCancel is idempotent. DriftCorrection is not used. Failsafes while pending is non-empty skip 8 wu / VerticalSettled / LeadSafety. **Late-collapse is intentional authoritative input compaction:** intermediate historical held commands may be acknowledged without receiving individual physics steps.
- Reason: Per-tick acknowledgement removes N±1 arrival-phase error at the source while keeping the server authoritative and the simulation clock moving under HOL delay.
- Consequences: A jump that was valid when predicted can be invalid when late-collapsed because grounded state changed; that is a legitimate authoritative correction, not reconciliation failure. **Future actions (dash, skills, charged attacks) require explicit late-arrival semantics.** The `jump_pressed` OR/latch used in late-collapse is **not** a generic skill-action policy and must not be copied forward by default. Epoch bump is server-owned (attach/replace/teleport/respawn later); it does not wrap. At `u16::MAX` the next bump disconnects.

## ADR-0032: Development network impairment lab (Phase 5.6)

- Status: accepted
- Decision: Add a client-owned, development-only deterministic impairment scheduler (`purgatory-common::impairment`) at the network-delivery boundary. Input delay/stall is applied after prediction/pending append and before `write_client_control` on the long-lived bidirectional stream. Snapshot delay is applied after a successful uni-stream read and before gameplay `push_snapshot`. Protocol stays **v4**. Impairment is off by default. Config/profile uses `watch` (latest-wins). Imperative `TriggerInputStall` uses a small `mpsc`. Off makes queued items immediately eligible but they still drain through bounded FIFO (8 input / 4 snapshot per live-loop turn) so a post-stall burst cannot monopolize the net thread. Independent LCG streams (input jitter, snapshot jitter, auto-stall) are derived from one master seed. Observed delay metrics are actual queue residence, including stall time. Profile/Off changes do not reset accumulated metrics; reset is a separate action. Correction magnitude is pre-reconcile predicted → post restore+replay predicted. Expected lead remains auth → replayed predicted with pending. Aligned residual stays diagnostic only. Client-observed ack delta is **not** a late-collapse proxy. True `late_collapse_count` stays server-side. Ping/Pong RTT is never impaired.
- Reason: Measure the Phase 5.5 envelope under realistic reliable-stream delivery problems (delay, jitter, HOL stall/burst) without corrupting command identity or blocking simulation/render threads.
- Consequences: Snapshot impairment is **not** full QUIC flow-control/send-buffer stall. Interpolation stress beyond the fixed 3-tick buffer is measured, not treated as an automatic 5.6 failure. Phase 5.7 load harness follows (ADR-0033).

## ADR-0033: Multiplayer load, soak, churn & scaling validation (Phase 5.7)

- Status: accepted
- Decision: Extend `tools/bot_client` into a real headless QUIC load harness (`purgatory-load`) that shares Quinn Hello/Welcome/`InputCommand`/snapshot decode with the game path, without depending on `purgatory-client` (no winit/wgpu/prediction/HeldCancel). Raise `MAX_ENTITIES_PER_SNAPSHOT` **64 → 256** as a mechanical protocol-v4 decode bound (layout unchanged; `MAX_GAMEPLAY_SNAPSHOT_BYTES` stays 8192). Default admission remains 32; development **load mode** sets `PURGATORY_ADMISSION_CAP=256` and scales lifecycle/input channels. Export server metrics on localhost UDP `127.0.0.1:5002` (magic `PURGSTAT` + version 1); DTO is `LoadMetricsV1` via serde in `purgatory-common` (off-protocol). Missed poll ≠ zeros. Memory is in-process Windows Working Set (`windows-sys`), not PowerShell. Raw run artifacts under `logs/load/` with `latest.txt` (newest created) vs `last_finished.txt` (last flushed summary); charts are offline (`tools/analyze_load_run.py`). Classification: one tick overrun ≠ WARN; starvation = no decode >1s; high N / rising p95 is not automatic PASS/WARN/FAILED. No player-count PASS claim.
- Reason: Phase 5.6 measured delivery under impairment; Phase 5.7 must measure multi-session pressure with truthful server metrics and reproducible bot behavior before Phase 6 gameplay breadth.
- Consequences: Load-mode 256 is a development bound, not production capacity. Snapshot build remains O(N²) pose copies on the sim thread. This ADR does not define Phase 6. Phase 5 closed **GREEN** after 5.7 steady-state isolation; remaining fan-out / pose-copy / interest work is a Phase 6 follow-up.

## ADR-0034: Phase 6.0 runtime identity, WorldAddress, query and replication contracts

- Status: accepted
- Decision: Introduce first-class `WorldAddress` (map + channel + instance) and separated identity types (`EntityId` / `RuntimeEntityId`, `ContentId`, `PersistentId`). `World` owns lifecycle, membership, a simple slot-scan query API, and visibility/relevance. Networking asks `World::relevance_for(RuntimeEntityId)` — simulation never takes `ConnectionId`. Replication class / priority / frequency / state-vs-event are **policy metadata only**; 6.0 does not schedule, delta-encode, or budget packets. Full-world player broadcast is the current snapshot implementation behind that contract, not the final architecture. `ContentId` compact storage is not the Phase 6C content-architecture contract. Phase 6A made `Transform` an optional capability on the same slot vector.
- Reason: Later MOBs, interaction, maps, and AOI must not couple to raw networking, full-world scans, or mixed identity namespaces. Contracts must exist before gameplay breadth.
- Consequences: Protocol stays v4 in 6.0/6A. Phase 6A delivered optional Transform and per-domain dirty flags on the same slot vector. Heavy spatial AOI, delta encoding, frequency scheduling, and byte budgets remain Phase 6D / later. Observer “left relevance” is not despawn. Address change is not character destruction.

## ADR-0035: Authoritative interaction sessions and protocol v5

- Status: accepted
- Decision: Interaction is server-authoritative. `World` owns `Interactable` capability and `InteractionSession` domain state (not a UI window). GameplayOwner maps `ConnectionId` → `RuntimeEntityId` then calls `World::try_open_interaction`. Protocol version **5** adds reliable control envelopes (`InteractOpen` / `InteractClose` / Opened / Rejected / Updated / Closed). Snapshot `ReplicatedKind::Interactable` exists so the client can present an **advisory** nearest target. Client `UIRuntimeState` is presentation only. Client-supplied distance, validity, or outcome is ignored.
- Reason: Future NPC / portal / chest / vendor flows must share one validation path. Equating a session with a UI window would block interactions that have no widgets.
- Consequences: v4 peers are rejected at handshake. Interpolation remains presentation-only and still interpolates players only. Dialog, shop, inventory, and production UI are later phases. Spatial AOI remains 6D.

## ADR-0036: Authored ContentId, split content domains, and protocol v6 observer address

- Status: accepted
- Decision: The canonical `ContentId` is the authored string (`map.dev.footnote`). Compact `u64` storage is FNV-1a of that UTF-8 string and is an implementation detail. Shared content (`content/shared`) and server-only content (`content/server`) are logically separate domains; the client loads shared only. The content registry is the sole `Map ContentId ↔ MapId` mapping (JSON must not contain numeric `MapId`). Map instantiate preflights then mutates, with spawn-batch rollback as a safety net. Runtime APIs are lazy `ensure_map` / `destroy_map`; eager Map A/B instantiate is a development convenience in `GameplayOwner`. Portal travel is driven by entity `transition` metadata, not `InteractableKind` matching in `GameplayOwner`. Protocol **v6** adds observer `local_map` / `local_channel` / `local_instance` on `WorldSnapshot`; a change is a client replica / interpolation / prediction baseline boundary. Missing MapId/content on the client is a hard failure.
- Reason: Phase 6C must stop treating the hard-coded FOOTNOTE arena as the only world without coupling gameplay to growing `InteractableKind` matches or leaking server placements onto the client.
- Consequences: v5 peers are rejected at handshake. Schema v1 is minimal (maps, spawn points, platforms, server placements, optional interactable + transition). WindowManager, combat, inventory, MOB AI, persistence, spatial AOI, and replication scheduling remain later phases.

## ADR-0037: Portals, linked destinations, and protocol v7

- Status: accepted
- Decision: Map-transition interactables are **Portals**. They remain authoritative replicated runtime entities (`ReplicatedKind::Portal`). Activation is a dedicated edge-triggered `PortalActivate` control (Up Arrow on the client). Generic interactables keep `InteractOpen` (E). The server validates live id/generation, same `WorldAddress`, a small centered activation zone, and a content-driven linked destination portal. Arrival is at that destination portal, not a map spawn point. Links are one-way unless the destination also authors a back-link. After arrival, a reentry lock blocks the destination portal until **Up is released** (optional zone-exit is an additional clear path, not required). Client fade is presentation-only (`FadeOut → baseline → FadeIn`) and starts only from an **accepted** observer `WorldAddress` change — not from sending `PortalActivate`. FadeIn cannot start until `DestinationReady` (new-epoch self baseline applied, local prediction and camera seeded from that destination pose). It does not block simulation.
- Reason: E-range nearest-target is the wrong contract for map travel. Hard-coding Map A/B in `GameplayOwner` would block one-way portals and extra maps.
- Consequences: Protocol **v7** (incompatible with v6). `GameplayOwner` still does not `match InteractableKind` for travel; it reads `transition.{map,portal}`. Spatial AOI is Phase 6D (ADR-0038 / ADR-0039). Portal travel **preserves** the actor's current `ChannelId` and `InstanceId` (no authored override exists yet). Client fade and map-geometry rebuild are **MapId** presentation behavior; a Channel-only `WorldAddress` change is a replication boundary without Portal fade (ADR-0040).

## ADR-0038: Spatial grid and server interest-policy AOI (Phase 6D)

- Status: accepted
- Decision: `World` owns a uniform grid per live `WorldAddress`. Cell size is configuration (`SPATIAL_CELL_SIZE_WU = 4.0` wu initially, not an architectural invariant). Transform insert/remove/relocate keep the grid in sync. `World::spatial_candidates` returns leave-rect + class-visible entities with no hysteresis and no `ConnectionId`. Enter/leave rectangles are **server interest policy**. Phase 6G replaced the initial player-centered `[16, 9]` half-extents with a **validated visible-view envelope**: legal camera centers from the observer pose + FOOTNOTE viewport (`FOOTNOTE_TEST_VIEWPORT_HEIGHT` × 16:9) + client Dead Zone half-extents `[3, 4]`, clamped with the same rule as presentation `clamp_camera_center`, then expanded by `AOI_PREFETCH_MARGIN = 2` wu (enter before the visible edge) and an extra `AOI_LEAVE_MARGIN = 2` wu (leave only after that envelope). The client camera is not a network authority; a modified client cannot request an arbitrary distant region. Hysteresis lives only in `ObserverReplicationState`. VisibleObservers that participate in AOI must have Transform; missing Transform is not globally visible. OwnerOnly may be non-spatial via the owner path.
- Reason: Full-map broadcast and independent world scans do not scale. Player-centered `[16, 9]` did not cover a clamped FOOTNOTE camera (`world_viewport≈24.89×14` with the player offset to a screen edge). Encoding a client-sent camera pose would let a modified client request distant map regions.
- Consequences: `relevance_for` remains a non-hysteretic alias of spatial candidates. Grid internals stay in simulation; the client overlay may draw policy rects for debugging only. Interest still does not replicate the entire map.

## ADR-0039: Protocol v8 replication frames and coalescing mailbox (Phase 6D)

- Status: accepted
- Decision: Keep **one long-lived ordered QUIC uni stream** per connection. Replace server `watch::send(WorldSnapshot)` and client latest-wins snapshot watch with a coalescing intent mailbox, a bounded epoch-tagged writer queue (cap 4), and `ReplicationFrame` (tag 16: Enter/Update/Leave, `observer_baseline_epoch`, optional health). Per-domain `u64` World revisions; observer `last_committed_rev` advances when the writer queue **accepts** a record, not on client ACK. There is no application ACK in 6D. EventOnly means no periodic resend; domain-rev changes still send. Map/`WorldAddress` transition — including a Channel-only change — bumps epoch: discard intent, purge queued frames with older epoch, client ignores older epoch and resets on newer. Frame builder admits records using pre-commit encoded size against `REPLICATION_FRAME_BUDGET_BYTES` (4096) capped by `MAX_GAMEPLAY_SNAPSHOT_BYTES` (8192). Progressive Enter: Updates cannot overtake an uncommitted Enter. `write_all` failure tears down the session; do not reopen a sibling uni or skip the failed frame.
- Reason: Latest-wins `watch` drops Enter/Leave lifecycle. Dirty flags alone cannot express cadence, deferral, or budget. Cross-stream uni would reorder Enter/Update.
- Consequences: Protocol **v8** (incompatible with v7). Health on the wire is a multi-domain delta proof, not the combat/stat model. Persistence remains 6E. Protocol **v9** adds DEV-only `DevSetChannel` without changing the `ReplicationFrame` layout (ADR-0040).

## ADR-0040: Channel transition is a WorldAddress boundary (Phase 6D)

- Status: accepted
- Decision: `WorldAddress = MapId + ChannelId + InstanceId`. All three isolate replication relevance and spatial-grid membership. A Channel change on the same Map is an authoritative membership change (`World::set_address`), not entity destruction, not reconnect, and not a Portal. The DEV overlay sends `ClientControl::DevSetChannel` (protocol **v9**, tag 17). No prior typed DEV-control envelope existed that could carry `SetChannel`; a generic string/debug command bus was rejected. The client must not mutate WorldAddress locally. Flow: DEV click → typed request → server validates / `ensure_map` / `set_address` → new epoch/address arrives → client mirrors presentation membership. The server validates Channel `0..=DEV_CHANNEL_MAX` (currently 1), preserves MapId and InstanceId, rematches `grounded_on` onto destination-channel geometry, bumps observer epoch, purges older queued frames, and publishes a fresh Enter baseline. Client presentation: MapId change uses map fade + geometry rebuild gated by `DestinationReady`. Same-Map Channel/Instance change uses a shorter membership fade + `World::rebind_map_address` (no geometry rebuild, no teleport) gated by `MembershipReady` (ADR-0041). FadeIn is readiness-driven, not timer-revealed. Portal travel preserves current ChannelId/InstanceId unless authored transition metadata later overrides (none exists).
  **WorldAddress boundary ≠ social identity boundary.** `InteractionCloseReason::AddressChanged` invalidates world-bound `InteractionSession` (switch/chest/portal/NPC-like interactables when the target is no longer WorldAddress-compatible). Replication Known/AOI and interpolation tied to old relevance also reset. That is not a generic “close every player-related session” rule. Future whisper, friends, party/guild chat, and social presence must not be keyed to Channel membership; a player changing Channel must still be able to communicate with a player in another Channel. Those systems are not implemented in 6D. Prefer the explicit `InteractionSession` / `InteractionCloseReason::AddressChanged` types over a catch-all closer. Channel persistence across login/restart, production channel UI, allocation, and load-balancing are **not** decided here (Phase 6E+).
- Reason: Map portals already exercised MapId. Channel isolation had to be proven independently, including two players at the same coordinates. Treating any WorldAddress change as a map teleport would reload geometry and start fade incorrectly. Tying identity/social sessions to Channel would make cross-channel whisper/party impossible without a later rewrite.
- Consequences: v8 peers are rejected at handshake. DEV Channel buttons exist only while the debug overlay is available. No instance-management UX. No production WindowManager channel selector. Social/identity systems remain later phases and must use their own session types. Client `DevSetChannel` must also emit `Interact Closed / AddressChanged` for a live world-bound session.

## ADR-0041: Transition presentation is readiness-gated (Phase 6D)

- Status: accepted
- Decision: FadeOut/FadeIn is a **transition readiness contract**, not a timer that reveals half-prepared state. `Idle → FadeOut → Hold (black / waiting) → FadeIn → Idle`. Fade duration is presentation timing; FadeIn starts only when destination presentation is actually ready. Simulation continues while the screen is obscured. Readiness comes from authoritative/client-applied state, never from DEV button/input intent.
  - **Map** (`TransitionKind::Map`): FadeOut 300 ms, min black hold 75 ms, FadeIn 400 ms. Reveal waits for existing `DestinationReady` (accepted dest WorldAddress, new epoch/baseline, dest geometry, self Enter, local player seeded from dest pose, prediction synced, camera following that pose, old-map interp/replicas not visible).
  - **Membership** (`TransitionKind::Membership`, Channel and/or Instance, same MapId): FadeOut 200 ms, min black hold 50 ms, FadeIn 250 ms. Not a map load: no geometry rebuild, no teleport, no camera reset, no portal dest logic. Reveal waits for `MembershipReady` (accepted Channel/Instance, new observer epoch/address, old Known/replicas gone, dest Enter begun, local pose stable, presentation WorldAddress matches observer, world-bound `InteractionSession` closed with `AddressChanged` if the target is no longer WorldAddress-compatible). Future Instance UI can reuse this gate. Do not hard-code Map A/B or Channel 0/1.
  - If readiness takes longer than the visual minimum, remain black. A DEV stall (5 s) logs missing flags and shows a warning; it does not fake success or reveal inconsistent state. Production recovery UX is not 6D.
  - **Blackout is the presentation commit boundary.** Authoritative simulation, networking, and replica apply may proceed immediately when the dest epoch/address arrives. The **visible** presentation world (source remotes, interactables, portals, map geometry, local pose) stays on the source until FadeOut has reached fully black (`MapFade::is_fully_black()`, the Hold phase). Then membership/map presentation commits. Do not use an extra fixed delay. Map destinations snap the camera; Channel/Instance preserves it.
- Reason: Timer-revealed swaps mixed old/new coordinate spaces (portal pose/label bugs). Channel change left world-bound interaction UI open because Closed was not sent. Map and membership readiness are different contracts sharing one small presentation gate. Applying dest-epoch replica during visible FadeOut made remotes disappear before black.
- Consequences: Overlay banners such as `MAP TRANSITION · Waiting DestinationReady` and `CHANNEL TRANSITION · Waiting MembershipReady`. Fade does not conceal correctness bugs: missing readiness stays black and is visible in DEV. ADR-0040 Channel isolation is unchanged; this ADR only gates presentation. A client-side frozen source presentation (not a second World) covers the visible FadeOut after dest replica apply. Gameplay input during the obscured window is a separate authority barrier (ADR-0042), not a presentation timer.

## ADR-0042: Transition is a gameplay input barrier (Phase 6D)

- Status: accepted
- Decision: A WorldAddress / Map / Channel transition is a **gameplay input barrier**, not merely a visual fade. On authoritative transition accept the server: (1) bumps input epoch (invalidating old-epoch commands), (2) clears queued commands and last held axis/down, (3) zeroes player velocity, (4) marks the session input-gated. While gated, `take_for_tick` returns idle and acks any new-epoch commands without adopting their held movement; `PortalActivate`, `InteractOpen`, and `DevSetChannel` are rejected (`Unavailable` / no-op). The gate lasts the presentation FadeOut + minimum Hold window in simulation ticks (earliest honest FadeIn; durations match ADR-0041). The client independently discards discrete edges and samples idle movement until FadeIn begins (`DestinationReady` / `MembershipReady` then FadeIn). Held Left/Right remain latched in `ActionState` and may resume after unlock without a new key press. Held Up does not bounce (existing reentry + edge semantics). Jump / Interact / Portal edges that occur while locked are not replayed. Map and Channel share this gate mechanism but remain distinct transitions (no geometry rebuild / teleport on Channel). This is not the Phase 6F action-state system; `InputGateReason` is the absorb point.
- Reason: Held movement continued from last `SessionInput` / new-epoch commands during FadeOut and blackout, so the first visible destination frame could show the player already away from the linked Portal. Client-only send suppression cannot stop a stale or malicious client.
- Consequences: No protocol version change. Honest clients stay at the dest Portal pose until FadeIn. Remotes do not observe hidden travel during the server-recognized barrier. Phase 6F maps `InputGateReason` to `ActionDenialReason::TransitionLocked` without changing the remaining-tick lock or the accept-time neutralize + reject contract.

## ADR-0043: DEV login lookup and server-minted CharacterId (Phase 6E)

- Status: accepted
- Decision: Persistent player identity is `CharacterId`, a server-minted `u64` distinct from `ConnectionId`, `EntityId`, `ContentId`, and `PersistentId`. `PersistentId` remains an optional runtime-entity placeholder and is **not** constructed from `CharacterId::raw()`. Until a real account system exists, Hello (protocol **v10**) carries `dev_login`: a DEV-only lookup string. Validation is `length 2..=32`, charset `[a-z0-9_.]`, reject `..`, path separators, and leading/trailing `.`. No normalization; the exact string is the lookup key. It is never used as a filesystem path. Same login maps to the same `CharacterId` across reconnect and process restart. First login allocates the next id under a single persistence-worker critical section (lookup → allocate → persist mapping). A live Character may occupy at most one session; a second login is rejected with `DisconnectReasonCode::AlreadyConnected` (no kick, no duplicate). Reconnect always receives a fresh `EntityId`. Welcome is sent only after identity resolve, character load/create, restore, occupancy reservation, runtime placement, spawn, bind, and replication-ready. The 6D ephemeral `GameplayOwner::attach` path remains for unit tests; live QUIC uses Enter.
- Reason: Reconnect and restart must restore a Character without inventing accounts, hashing logins into ids, or aliasing `PersistentId`. Occupancy must not interleave two sessions into the same Character.
- Consequences: Protocol **v10**. File-backed `identity.json` plus `char_{id:016x}.json`. The runtime data root is **not** the source/install tree: Windows default `%LOCALAPPDATA%\Purgatory\`; `PURGATORY_DATA_DIR` is an explicit override. The server does not require the repository directory to be writable. Bots use unique `bot.{id:04}` logins. Client Connection Frontend has a DEV login field (default `dev.local`) and must not store authoritative character state.

## ADR-0044: RestoreIntent is not WorldAddress (Phase 6E)

- Status: accepted
- Decision: Persistent restore is a content-authored **RestoreIntent** `{ map_authored, point_id, optional checkpoint_id }` plus optional `InstanceExitContext` (not a runtime `InstanceId`). It does not store `ChannelId`, runtime `InstanceId`, `WorldAddress`, `EntityId`, `ConnectionId`, or exact coordinates. Resolution is two layers: Restore Resolver → `LogicalRestoreDestination` → Runtime Placement Resolver → `WorldAddress`. Phase 6E placement currently selects `ChannelId::DEFAULT` and `InstanceId::DEFAULT` as a **temporary** placement implementation. Map restore policy is authored (`safe_point` / `checkpoint` / `non_reenterable`) with fallback to `map.dev.footnote` / `default` on invalid content refs. Persistent mutation bumps `persistence_revision` and requests a generic save; Portal is the first caller, not a portal-specific save system. The simulation thread `try_send`s an owned `PersistentCharacterSnapshot`; the persistence worker serializes JSON and performs crash-safe recoverable file replacement (tmp → sync → dest→`.bak` → tmp→dest → delete `.bak` on Windows). Stale revisions do not overwrite newer ones. Graceful shutdown attempts a **named/configurable bounded** persistence drain (`DEFAULT_PERSISTENCE_SHUTDOWN_TIMEOUT`, overridable), not a hard-coded architectural 2-second invariant.
- Reason: Coupling restore to runtime topology (Channel/Instance) would make content persistence own allocation policy that does not exist yet. Serialization on the 30 Hz sim thread would violate the simulation/IO boundary.
- Consequences: Reconnect after a Channel change restores DEFAULT channel of the restore map. Instance rejoin is a future `InstanceExitContext` extension, not persisted `InstanceId`.

## ADR-0045: Simulation-time scheduler with two lanes (Phase 6F)

- Status: accepted
- Decision: Authoritative timers live in `World` as a generational `TimerId` scheduler keyed by `SimulationTick`, not wall clocks or Tokio. Owners are `World` or `Entity(EntityId)`. Lanes are `Critical` and `Deferred`. Critical due work is intended to complete in the required tick. A `CRITICAL_DRAIN_CEILING` (1024) is a pathological-overload safeguard: leftover due work carries forward and `scheduler_critical_ceiling_hits` increments. Hitting the ceiling can delay gameplay timing; it is not a policy that critical work is deferrable. Deferred work is budgeted FIFO (`DEFERRED_DRAIN_BUDGET` = 32) with a progress guarantee (at least one due job per drain while any remain). Live slots are capped (`SCHEDULER_CAPACITY` = 4096). Newly scheduled work from a callback is not eligible in the same drain pass. Cancelled or expired `TimerId`s cannot fire twice. Owner despawn cancels owned jobs. Stale `EntityId` targets are a controlled no-op at fire time.
- Reason: Future gameplay must not invent parallel timers. An unbounded Critical drain would stall the 30 Hz tick under overload. A hard ceiling that silently reclassifies correctness-timed work as deferrable would change gameplay; the ceiling is therefore an explicit invariant failure, not a second budget.
- Consequences: No protocol bump. Scheduler kinds in 6F are a closed enum (`ExpireEffect`, `CompleteAction`, `SpawnDue`, `DespawnEntity`, `TestProbe`, `RaiseEvent`). Combat/skills are out of scope.

## ADR-0046: Action lifecycle and gates; InteractionSession stays distinct (Phase 6F)

- Status: accepted
- Decision: An `Action` slot exists only after start succeeds. Request / validate / reject are command-pipeline outcomes, not stored `ActionPhase` variants. Stored phase is `Active` or a terminal (`Completed`, `Cancelled`, `Interrupted`, `Failed`). Identity is generational `ActionId`. Foundational policy: at most one Active action per owner. `ActionDenialReason` is typed (`MissingOwner`, `Busy`, `TransitionLocked`, `Disconnected`); there is no `can_act: bool`. `InputGateReason` is absorbed as `TransitionLocked` without rewriting ADR-0042. `InteractionSession` remains the 6B interaction session and is not merged into Action. Owner despawn cancels the live action. Illegal transitions and double-complete are rejected.
- Reason: Persisting every conceptual planning name (`Requested`, `Validated`, `Rejected`) would duplicate the command pipeline inside World. Interaction and exclusive actions have different lifetimes and wire contracts.
- Consequences: Synthetic `ActionKind::Test` only. Real skills/combat are later phases. Existing Interact / Portal / DevSetChannel envelopes stay on protocol v10 and call the shared gate/preamble internally.

## ADR-0047: Commands versus staged runtime events (Phase 6F)

- Status: accepted
- Decision: Client envelopes remain **commands** (untrusted requests). Runtime facts are closed `RuntimeEvent` values produced into a double-buffered queue: push into pending; commit once per tick; events emitted during commit wait until the next commit. Consumers are explicit `match` sites in World / GameplayOwner. There is no global bus, subscriber list, HashMap iteration order, or async dispatch. Invalid commands reject with a typed reason. Stale runtime targets are a controlled no-op or cancel. Internal impossible invariants use `debug_assert` per project rules.
- Reason: “Arrived ⇒ valid” is not a contract. A global bus would hide stage order and make tick recursion easy.
- Consequences: No protocol v11. Interaction/Portal/DevSetChannel keep existing wire types; new denials map onto existing `Unavailable` where a wire response already exists.

## ADR-0048: Cadence and deferred work budget (Phase 6F)

- Status: accepted
- Decision: 30 Hz is the simulation tick rate, not a requirement that every system run every tick. Cadence is `EveryTick` or `EveryN { n }` with a stagger key (`phase = key % n` for registered work; replication Normal/Low uses `(tick + entity.index()) % n`). Deferred scheduler work uses the ADR-0045 FIFO budget and progress guarantee. Replication cadence staggering replaces a global `tick % n == 0` spike.
- Reason: Coincident low-frequency work and replication Updates would create periodic CPU/bandwidth spikes without changing correctness.
- Consequences: Movement (`tick_player`) stays every tick. EventOnly replication still sends on domain-rev change. Cadence is not NPC AI.

## ADR-0049: Dirty/delta replication is DomainRevs per observer (Phase 6F)

- Status: accepted
- Decision: AOI answers **who** may need state. `DomainRevs` are authoritative World change versions. `ObserverReplicationState` / `CommittedRevs` determine whether a particular observer still needs data. Baseline = Enter on AOI entry or epoch reset. Delta = Update when that observer lags World revs **and** cadence allows. Exit = Leave. Re-enter is a new Enter baseline. One observer’s writer-queue commit must not clear another observer’s required Update/Enter. `DirtyFlags` / `consume_dirty` remain a local convenience (tests/overlay), not the multi-client replication contract. Metrics expose domain-rev advances, per-observer pending Enter/Update, committed Enter/Update/Leave, and cadence-deferred updates — not a misleading global “dirty pending” gauge.
- Reason: Clearing World dirty because one client was sent would drop required deltas for other observers. A global dirty-pending count is not the replication contract.
- Consequences: Protocol stays v10. No payload compression. Production replication must not `consume_dirty`.

## ADR-0050: Developer Tools foundation

- Status: accepted
- Decision: PURGATORY Developer Tools is the development-side control surface (runtime, testing, diagnostics, and later content tools). The current shell is PowerShell + Windows Forms under `tools/dev/`, opened by `DEV.BAT`. Child processes are launched with owned `System.Diagnostics.Process` objects; Windows Terminal / generated `.cmd` wrappers are not the normal path for server, client, or Cargo. Metrics, Health, and Readiness are distinct. Authoritative connection readiness is `purgatory-load --probe` (existing Quinn + protocol v10 Hello/Welcome, login `dev.probe`), not a PowerShell protocol implementation and not OS UDP-port inspection. A future visual Map Editor may be a native binary launched by this shell; it is not required to be WinForms.
- Reason: The previous launcher mixed UI, Cargo inference, Terminal tabs, and process scanning until “process exists + port bound” was treated as running. That does not scale and does not prove the server will accept a client. Reusing `tools/bot_client` avoids a second network stack and does not bump protocol v10.
- Consequences: Developer Tools is not a Cargo workspace member. Closing the UI does not stop game processes; reopen adopts workspace `target\` processes and verifies them. `--probe` currently follows the normal DEV persist/enter path (known debt; see `docs/dev-tools/`). Map/NPC editors and Phase 7 gameplay are out of scope for this decision. Details: [`docs/dev-tools/`](dev-tools/README.md).

## ADR-0051: Load validation is test infrastructure (Phase 6G)

- Status: accepted
- Decision: Synthetic World population, scheduler/action/effect/event/cadence pressure, and isolated persist for validation live behind one namespaced env: `PURGATORY_LOAD_VALIDATION` (JSON). The server applies it only when load-mode is explicit (`PURGATORY_ADMISSION_CAP` set). Production gameplay paths must not depend on `ScheduledKind::TestProbe`, `ActionKind::Test`, or `EffectKind::Test`. Load metrics are schema **4**: monotonic execution totals were added because 1 Hz queue-depth / active-count gauges can stay zero while work is created and consumed inside a tick or between polls (datagram, CSV, analyzer, docs, and backward decode moved together). Pass/fail remains `purgatory-load`. Soak duration is configurable; a ~30 minute Mixed run is initial evidence, not the definition of a soak. Developer Tools Runtime Validation forwards `purgatory-load --preset` argv/env; CLI owns pass/fail.
- Reason: Phase 6G is integrated validation of 6A–6F, not a second runtime. Accidental production dependence on test kinds would block Phase 7 MOB work that must use the generic Entity spine without a fake Character.
- Consequences: Protocol stays v10. No CharacterId on Welcome. Failure injection uses temp/run dirs, never `%LOCALAPPDATA%\Purgatory`. Report: [`docs/PHASE_6G_REPORT.md`](PHASE_6G_REPORT.md). Exit review: [`docs/PHASE_6_EXIT_REVIEW.md`](PHASE_6_EXIT_REVIEW.md). Do not start Phase 7 from this decision.

## ADR-0052: Rust Developer Hub (tooling track)

- Status: accepted
- Decision: PURGATORY Developer Tools remains the development-side control surface (ADR-0050 product identity). The long-term shell is a **separate Rust desktop application** (Developer Hub), not the game client and not a future player launcher. Orchestration lives in `purgatory-dev-runtime` (no egui/winit/wgpu/Quinn). `purgatory-dev-hub` is a provisional eframe GUI that only invokes and presents that crate. The GUI stack is **revisitable** and is not frozen. PowerShell under `tools/dev/` stays the operational fallback until Hub parity is sufficient; `DEV.BAT` is not switched in this decision. Ready remains: tracked process alive **and** `purgatory-load --probe` exit 0. Health/`PURGSTAT` is diagnostic for the Health line and for post-Ready Degraded (existing launcher behavior); it is not the initial Ready gate. Listener observation is diagnostic only. The Hub must not poke `World` through backdoors; a future admin/dev protocol is deferred. Spawned-by-this-session, adopted, and merely discovered workspace processes are distinct. Build/start/probe/stop are explicit jobs with cancellation/supersession so UI clicks cannot overlap lifecycle operations. Activity/log views are bounded in process memory; file logs under `logs/dev-tools/` may grow on disk. **Hub lifetime is not server lifetime:** closing the Hub must not terminate the dedicated server. `DEV_HUB.BAT` builds then launches `purgatory-dev-hub.exe` independently and exits; `cargo run` is not the operational launch path because cargo’s Windows job can kill the Hub and its children. The server is spawned as a detached process (inheritable log-file stdio, breakaway from a job when allowed); cargo and probes remain session-owned and may die with the Hub. A new Hub process discovers a workspace `target\` server, adopts it, and must re-verify with `--probe` before Ready. `server process exited unexpectedly` is only for a process this session had already observed alive.
- Reason: The PowerShell launcher is a successful prototype of the Hub, not the long-term development environment. Putting process orchestration, cargo, bots, and later editors inside `purgatory-client` would mix player-facing and developer responsibilities. A headless core keeps the GUI replaceable. Incremental migration lets 6G.2 capacity work use the Hub without pausing for editors.
- Consequences: ADR-0050 is unchanged. This is not a gameplay phase; root `PHASE` stays `6G`. Do not start Phase 7 from this decision. Do not add map/NPC/content editors, an admin protocol, or a web/Electron stack here. Probe persistence debt (`dev.probe`) is unchanged. Slice 1.1 adds an application shell (Dashboard / Runtime Server / Logs live). Slice 2 adds Runtime Validation orchestration (`purgatory-load` remains pass/fail authority) and a workspace-scoped Hub lock (`logs/dev-tools/hub.lock`); the harness is session-owned and dies with the Hub, the dedicated server does not. Subsequent Hub launcher-parity work adds load/soak, clients, quality gate, Rebuild, Kill All, settings, and bounded `server.log` / `client.log` tails while PowerShell remains the fallback (`DEV.BAT` is not switched here). World/Content stay reserved for editors. Details: [`docs/dev-tools/`](dev-tools/README.md), [`docs/dev-tools/PARITY.md`](dev-tools/PARITY.md).

## ADR-0053: Phase 6G capacity track (6G.1–6G.4)

- Status: accepted
- Decision: Reinterpret the Phase 6G endpoint as an explicit capacity-engineering track before Phase 7 grows. **6G.1** = correctness / integrated validation (automated GREEN; presentation/duration P0 closed). **6G.2** = capacity characterization: freeze a reference build ([`docs/MMO_RUNTIME_BASELINE.md`](MMO_RUNTIME_BASELINE.md)), coarse per-tick domain timings + process CPU/memory as **run artifacts** (not stuffed onto UDP), five scenario ladders (idle / distributed / mutual-AOI hotspot / churn / gameplay-service burst) at 32→64→128→256, then an instrumented ~30-minute Mixed soak. Live UDP `PURGSTAT` is schema **4** for Mixed execution-proof totals (ADR-0051); domain timings stay file artifacts. Crossing 33.33 ms tick work is a recorded capacity signal, not an automatic ladder stop. **6G.2 does not redesign** AOI, replication, scheduler, or storage. **6G.3** = bottleneck remediation only after owner review of 6G.2 evidence (measure → review → redesign). **6G.4** = performance regression gates derived from measured results (budgets not invented in Pass 1). Root `PHASE` is `6G.2` while characterization runs.
- Reason: Progress for the next stretch means proving the existing runtime under measured MMO-like pressure and redesigning only where evidence justifies it—not treating new gameplay systems as the default form of progress while capacity remains unknown.
- Consequences: Do not begin Phase 7 from 6G.2 instrumentation alone. Do not raise `PREDICTION_PENDING_CAP` or admission as a “fix.” Localhost numbers are not player-capacity claims. The 128-client stall is investigated with 6G.2 timings (measure only). Future major systems after 6G.4 must declare an expected runtime cost model.