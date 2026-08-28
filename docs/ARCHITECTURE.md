# Architecture

PURGATORY is a custom 2D side-scrolling MMORPG engine. This document records the structural rules. The master execution specification remains `PURGATORY_CURSOR_MASTER_EXECUTION_PLAN.md`.

## What is being built

- Native desktop client first.
- Dedicated headless server.
- Server-authoritative realtime simulation.
- Shared protocol, content, and common crates.
- Data-driven content added without rewriting core engine systems.

## What is not being built yet

Phase 4 is the world/entity foundation. Phase 4.1 adds a client-only development debug overlay. Phase 4.5 adds the FOOTNOTE movement foundation. Phase 4.6 expands the hard-coded FOOTNOTE development test arena. Phase 4.8 adds camera follow, world bounds, parallax background, and debug harness improvements. Phase 5.0 adds the Quinn/QUIC session foundation (handshake, ConnectionId, RTT, Network debug tab) **without** gameplay replication. Phase 5.0B adds a Connection Frontend and a client-local connection lifecycle (manual Connect, `ConnectionAttemptId`, Game only after Welcome). Phase 5.0C hardens that lifecycle under races, concurrent clients, retries, stale events, queue pressure, and shutdown. Phase 5.0D adds semantic failure categories, bounded diagnostics, explicit idle timeout, and Network-tab observability. Phase 5.0E hardens the untrusted-peer / abuse foundation. Phase 5.0F proves that foundation converges back to baseline after soak, stress, and deterministic chaos. Phase 5.1 adds intent-only `InputCommand` and server-owned movement (protocol v2). Phase 5.2 adds bounded full `WorldSnapshot` replication and client replica rendering (protocol v3). Phase 5.3 adds client-only remote entity interpolation (no protocol bump). Phase 5.4 adds client-only local player prediction (no protocol bump; no reconciliation). Later phases add reconciliation, combat, persistence, maps, and presentation.

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

The server must never require a window, renderer, or GPU. The dedicated server runs a headless 30 Hz `GameplayOwner` tick independent of packet arrival. Connecting or disconnecting does not change tick rate. Connection tasks never lock or tick `World`.

## Networking (Phase 5.0)

Transport is Quinn/QUIC. The game protocol is in `purgatory-protocol` (framing, Hello/Welcome, ConnectionId). Quinn types live only in `apps/server/src/network/` and `apps/client/src/network/`.

```text
winit thread                         purgatory-net Tokio thread
Connect         mpsc try_send   →    one session task at a time
Disconnect/Shut watch epoch/flag →    cancel that session
lifecycle events  bounded mpsc  ←    send (interruptible by shutdown)
RttUpdated        bounded mpsc  ←    try_send (droppable)
```

The render loop never `.block_on`s network IO. Async tasks must not call `World::tick`.

**Trust:** all client bytes are untrusted. Parse and validate before any session insert. See [`PROTOCOL.md`](PROTOCOL.md).

Development listen address: `127.0.0.1:5001` (`NetworkConfig::DEV` in `purgatory-protocol`). Handshake timeout 5 s. Idle timeout 15 s (transport liveness, not AFK). Datagram ping ~1 s, nonce only. Max datagram payload 256 bytes.

`ConnectionId` (network session) is not `EntityId` (simulation instance). Phase 5.1 binds `ConnectionId → EntityId` on the server only. Snapshots carry generational `EntityId` and an explicit `local_player_entity`. A later `MapInstanceId` will also be a distinct, server-owned identity.

The server accept loop never awaits a full handshake. Each `Incoming` is refused at a concurrency cap of 32 or spawned as a per-connection task that owns Hello/Welcome and the live session. `SessionLease` removes the table entry exactly once (including panic unwind). `ConnectionId` is server-allocated monotonic `u64` starting at 1; the client cannot choose it. Wraparound is not handled (development scope).

### Client lifecycle (Phase 5.0B)

`ClientScreen` (`Connection` | `Game`) is what the user is viewing. `ConnectionState` is what the connection is doing. They are not merged.

Startup: `ClientScreen::Connection`, `ConnectionState::Disconnected`. The network thread may exist immediately but stays idle until an explicit Connect command. There is no startup auto-connect and no reconnect loop.

Game is entered only after a valid Welcome for the **active** `ConnectionAttemptId`, and only from `Handshaking`. QUIC transport-up is not enough. Stale events from older attempts are ignored. Disconnect invalidates the active attempt **immediately** on the client (before the runtime ack). A delayed Welcome after Disconnect cannot enter Game.

Session-local state (`ConnectionId`, RTT, `connected_since`, protocol/tick-rate) clears on disconnect/reject/new Connecting. A reconnect never reuses a stale `ConnectionId`.

While on the Connection Frontend: gameplay keys do not move the player; `SimulationClock` is not advanced. Frontend wait time is excluded by rebasing `last_instant` each frame (no catch-up burst when Game starts). Tick duration is unchanged.

The Connection Frontend is `ConnectionFrontend::paint` (egui, existing window). It is not the debug overlay. CONNECT lives on that screen. `~` may still toggle debug. Login / channel / character select are not placeholders yet.

`Graphic/LOGO.png` is loaded **once** at frontend init into an `egui::TextureHandle` via a centralized path helper (`apps/client/src/assets.rs`). This load path is temporary. Missing/decode failure logs one warning and falls back to the text title `PURGATORY`. The rest of `Graphic/` stays unused.

### Connection lifecycle hardening (Phase 5.0C)

Allowed `ConnectionState` transitions (illegal events are ignored, never panic):

```text
Disconnected → Connecting
Rejected     → Connecting
Connecting   → Handshaking | Disconnected
Handshaking  → Connected | Rejected | Disconnected
Connected    → Disconnected
```

`RttUpdated` is legal only while `Connected` and never changes lifecycle or screen.

**Stale-event rule:** `event.attempt_id != active_attempt` is rejected in `ClientLifecycle::apply` before any mutation. Duplicate Connect is rejected by `try_begin_connect` (one active attempt). The runtime also ignores extra Connect commands during a live session and drains extras queued before a session starts (no parallel QUIC, no duplicate Hello).

**Queues:** Connect commands cap 8 (`try_send`; render thread never blocks). Disconnect/Shutdown use a watch control plane (disconnect epoch + shutdown flag) so they remain possible under Connect pressure. There is no unbounded fallback. Lifecycle events (Connecting, Handshaking, Connected, Rejected, Disconnected) use a dedicated bounded channel (16) and are not silently dropped; send waits unless shutdown is in progress. Telemetry (`RttUpdated`) uses a separate bounded channel (32) and may be dropped. `poll` drains lifecycle first.

**Task ownership:** winit owns `NetworkHandle`. Drop sets shutdown and joins the `purgatory-net` thread. That thread owns the current-thread Tokio runtime, the Quinn `Endpoint`, at most one session task, the ping loop, and the control stream. Cancellation is the disconnect epoch / shutdown flag. No `process::exit`.

**Ping:** at most 4 outstanding nonces; the oldest is dropped when full. Unknown, duplicate, or stale-attempt Pongs are ignored. Telemetry cannot change connection lifecycle. A Pong after disconnect or after a new attempt cannot mutate the new session (attempt id filter).

**Concurrent clients:** Hello/Welcome isolation is per spawned task. One malformed or stalled peer does not block the accept loop or other sessions. Abrupt process kill is not simulated in CI; Drop of the QUIC connection is. True process disappearance is cleaned when the explicit idle timeout fires (`IDLE_TIMEOUT` = 15 s).

### Diagnostics and failure semantics (Phase 5.0D)

Quinn/rustls types never leave `apps/*/src/network/`. UI and lifecycle see `NetworkFailureKind`.

Wire (`DisconnectReasonCode`): VersionMismatch, Malformed, HandshakeTimeout, UnexpectedMessage, ServerShutdown. Local-only (never on the wire): ConnectFailed, TransportLost, IdleTimeout, ClientRequestedDisconnect, LocalShutdown, InternalNetworkError.

Retryable (no auto-reconnect): ConnectFailed, TransportLost, IdleTimeout, ServerShutdown, HandshakeTimeout, InternalNetworkError. Not retryable: VersionMismatch, MalformedMessage, UnexpectedMessage, ProtocolRejected, ClientRequestedDisconnect, LocalShutdown.

Frontend status comes from one mapping (`NetworkFailureKind::frontend_status`). Manual disconnect and app close show `Disconnected`, not `Connection failed`.

Handshake timeout is 5 s (Hello/Welcome incomplete). Idle timeout is 15 s (established transport liveness). They are not the same.

Diagnostic history is a 48-slot ring of lifecycle/failure events. RTT is latest/min/max/EWMA (α = 0.25) and does not fill history. Counters are fixed `u64`s (lifecycle, telemetry, dropped telemetry, stale ignored, reconnect attempts). Log Network Lifecycle and Verbose Network Trace default OFF.

Failure response:

- Malformed peer → reject/close; server stays up
- Handshake timeout → close; no session
- Idle / transport loss → cleanup + frontend `Connection lost`
- Version mismatch / protocol reject → `Rejected`; not retryable
- Client requested disconnect → clean close + `Disconnected`
- Local app shutdown → clean teardown; not a failure
- Server shutdown → controlled `ServerShutdown` when possible
- Internal network error → log + local teardown

**Identity boundary (future, documentation only):** `ConnectionId` ≠ `EntityId` ≠ `MapInstanceId`. A future MapInstance is server-authoritative. Two players in the same instance share that world. Private maps/dungeons are separate server-owned instances, not client-local worlds. Not implemented in 5.0D.

### Security and abuse foundation (Phase 5.0E)

Every remote peer is untrusted. QUIC encryption protects the path; it does not make Hello/Welcome gameplay-trusted. Packet arrival does not mutate `World`.

`NetworkAbuseConfig` is the server-only policy (sizes, handshake timeout, admission cap 32, 1 bidi / 0 uni streams, malformed/datagram budgets, control-message rate). Peers cannot configure it.

Severe (immediate disconnect/reject): oversized/zero control frames, invalid handshake, repeated Hello, forbidden direction. Tolerable: malformed/unknown datagrams and unknown complete control tags until a per-connection budget or rate-drop limit; then close that peer only.

Admission cap is concurrency safety, not MMO capacity. Excess `Incoming` is refused. Session mutexes are not held across `.await`. Poisoned mutexes recover via `into_inner`.

Parser enforcement is not behind `cfg(debug_assertions)`. Wire reject text stays generic (`protocol`, `malformed`). Peer strings in logs are bounded and control-stripped.

Future Phase 5.1 gameplay input will also be untrusted (legal values, tick/sequence, ownership, cooldowns). Not implemented here. No anti-cheat client, no IP ban list, no production DDoS fabric.

### Stability under pressure (Phase 5.0F)

The networking foundation is judged by **convergence**, not survival. After any bounded soak, stress, or chaos scenario, active state must return to baseline and a normal client must still work end to end.

**Active gauges vs cumulative counters.** Active gauges are transient and must return to baseline:

```text
active_sessions
active_handshakes
inflight_connection_tasks
admission permits in use
```

Cumulative counters (`accepted`, `rejected`, `mismatch`, `malformed`, `oversized`, `hs_timeout`, `rate_limited`, `admission_refused`, `clean_dc`, `transport_loss`) only grow; they are never expected to return to zero. High-water marks (`max_sessions`, `max_inflight`, `max_handshakes`) are also cumulative and exist to prove a bound was exercised. `SessionTable::high_water` records peak concurrency; the live table itself keeps no tombstones.

**Recovery-to-baseline invariant.** Test scenarios poll gauges with a bounded `wait_until_baseline` helper and fail with a gauge/counter report rather than sleeping and hoping. Every stress and chaos family ends with a mandatory healthy probe: connect → Welcome → ping/pong → clean disconnect.

**Deterministic chaos.** A test-only operation vocabulary (connect normal / stalled / wrong-version / malformed, send malformed control, send invalid datagram, oversized frame, repeated Hello, graceful disconnect, abrupt drop, wait) is driven by a seeded LCG. CI runs seeds `0x1`, `0xC0FFEE`, `0xDEADBEEF`; failures print the seed and operation index. There is no system-entropy seeding and no scenario scripting language. The runner is test-only and never compiled into the server.

**Loss semantics.** A graceful client close is always classified as a clean disconnect (the server consults the connection's own close reason rather than assuming loss). A peer that vanishes without a close frame is reaped by the explicit idle timeout. Graceful server shutdown delivers `ServerShutdown`; an abrupt server loss never fabricates one — clients classify it from transport symptoms only. Restarting the server produces fresh sessions and fresh `ConnectionId`s; nothing is resurrected.

**Simulation independence under churn.** Connection churn does not drive, pause, or accelerate the tick loop, and no packet mutates `World`. The invariant is architectural independence, not hard real-time scheduling under machine load.

**Not capacity.** A passing localhost soak says nothing about supported player counts. There is no gameplay state, replication, AI, persistence, combat, or bandwidth cost yet. This phase also does not prove internet DDoS resistance, production TLS/PKI, authentication, cheat resistance, persistence durability, cross-region latency, NAT traversal, or mobile/browser transport.

Do not start Phase 5.1 from this section.

## Authoritative input (Phase 5.1)

```text
network tasks
→ bounded validated InputCommand handoff
→ GameplayOwner (simulation thread)
→ World::tick_player (existing FOOTNOTE)
```

- Client sends `move_axis` / `jump_pressed` / `down_held` / `sequence` only.
- Held state is latest-wins. Jump is a one-shot edge, coalesced to one pending bit.
- Sequence: first accept, duplicate ignore, stale ignore, newer accept. `u32` does not wrap within a session.
- Each session owns exactly one player entity. Disconnect/loss despawns it.
- Gameplay input rate is separate from control-message rate (dev: 128/s command stream, 256 drops then disconnect).
- Client local FOOTNOTE is the Phase 5.4/5.5 prediction body (presentation only). Authority remains server-side.

Do not start Phase 5.6 from this section.

## Authoritative snapshots (Phase 5.2)

```text
World (read-only)
→ SnapshotBuilder (visibility set)
→ WorldSnapshot
→ per-connection watch (latest wins)
→ server-initiated uni stream
→ ReplicatedWorld (atomic apply)
→ renderer
```

- Full snapshots. Cadence: one per simulation tick (30 Hz) in development.
- Sequence: no wrap within process; stale/duplicate ignored; no rewind.
- Visibility is the shared arena's dynamic players, not “every entity on the server”. Static platforms stay local content.
- Snapshot buffering is replaceable state. Old snapshots are dropped. One slow client cannot stall simulation or peers. Lifecycle/control stays higher priority (`biased` select).
- Local cyan marker; remote a distinct debug color. Stepped/delayed remotes until Phase 5.3; local delay until Phase 5.4.

## Remote interpolation (Phase 5.3)

```text
WorldSnapshot (accepted)
→ ReplicatedWorld (authoritative)
→ InterpolationBuffer (bounded history)
→ estimated_server_tick (monotonic) − delay
→ bracket A/B → remote presentation poses
→ renderer (remote = interp; local = Phase 5.4 prediction)
```

- Presentation only. Does not modify replica, input, collision, or server simulation.
- Delay: 3 ticks via `INTERPOLATION_DELAY_TICKS` and `TICK_DURATION` (~100 ms at 30 Hz).
- Clock: advance from local `Instant`; snapshot `server_tick` may catch up forward only. `render_tick` never moves backward.
- Despawn waits until `render_tick` reaches the later bracket B. Wire arrival of a newer snapshot does not hide early.
- No extrapolation. Buffer underrun holds newest. Teleport snap (≥ 8 wu) is client presentation tuning.
- Scalability: interpolation is entirely client-side; server keeps latest-wins watches with no per-client history.

## Local prediction (Phase 5.4)

```text
physical input → PlayerInput
  ├─ InputCommand → server (authority unchanged)
  └─ LocalPrediction → World::tick (FOOTNOTE) → predicted pose
       → renderer + camera (local only)

WorldSnapshot → ReplicatedWorld (never overwritten by prediction)
Remotes → InterpolationBuffer (unchanged)
```

**AUTHORITATIVE STATE ≠ LOCAL PREDICTED PRESENTATION ≠ REMOTE INTERPOLATED PRESENTATION**

- Reuses deterministic FOOTNOTE movement (gravity, jump, collision, OneWay, drop-through, friction). Client `footnote_test_stage` platforms match the server arena; static geometry is not replicated.
- Prediction clock = shared `SimulationClock` at 30 Hz. Render frames consume 0..N ticks from `clock.advance`; catch-up bounded by `MAX_CATCH_UP` / `MAX_CATCH_UP_TICKS`.
- Init/reset from replica on first local, generation change, screen/disconnect clear. **Lead** is expected RTT×speed — not a correction trigger. Phase 5.5 replays unacked commands after restore.

Do not start Phase 5.6 from this section.

## Authoritative input acknowledgement and local reconciliation (Phase 5.5)

```text
SimulationClock tick
→ PlayerInput
  ├─ InputCommand (epoch, sequence) → pending window → server queue
  └─ tick_predicted_player (FOOTNOTE)

WorldSnapshot (per-recipient)
→ restore durable pose/vel/grounded/PlatformSupportId
→ drop pending with sequence ≤ last_acknowledged_input_sequence
→ replay remaining via tick_player
```

- Protocol v4. Server always `tick_player` at 30 Hz: Consumed (pop one command) or Continuation (held axis/down, `jump_pressed = false`). Gravity never pauses for a late packet.
- Command identity is `(input_epoch, sequence)`. First Accept of an epoch is sequence 1. Epoch is server-owned; bump on attach/replace. `u16::MAX` next bump disconnects — never wrap to 0.
- `last_acknowledged_input_sequence` is the accounted replay boundary, not one physics step per sequence. Consumed, late-collapse of a prefix, or HeldCancel cancel-ack may advance it.
- **Late-collapse** is intentional authoritative input compaction: after Continuation starvation (`unmatched_continuation_ticks` / continuation debt), a prefix of delayed commands is applied as one `tick_player` (latest held + `jump_pressed` OR). Intermediate historical held commands may be acknowledged without individual physics steps. Debt saturates; it never integer-wraps. Remainder debt is `K - N`, not unconditionally zeroed.
- `jump_pressed` OR during late-collapse is **not** a generic skill-action policy. Future dash/skills need explicit late-arrival semantics (ADR-0031).
- `PlatformSupportId` (`u16`, 0 = none) is stamped at `spawn_platform`. `last_contact` is transient and not on the wire.
- Hitch (`ticks_executed > 1`): prediction discontinuity. Clear pose history, keep pending, restore+replay, emit one new clock command — not N catch-up commands.
- Focus-loss: release `ActionState` immediately. Normal path: one forced `SimulationClock` Neutral step paired with a Neutral command. Full send window: `HeldCancel` with an immutable `(epoch, target_sequence)` barrier captured at send; pre-confirm snapshots are restore+replay, not a pending hole.
- Failsafes while pending > 0 skip 8 wu / VerticalSettled / LeadSafety. Empty pending may still hard-snap. DriftCorrection is not used.

Do not start Phase 5.6 from this section.

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
→ InputCommand every predicted tick (Connected / Game only)
→ server SessionInput → PlayerInput
→ World::tick_player (authoritative)

Client visual path (LOCAL DEV / NON-AUTHORITATIVE, not prediction):
→ PlayerInput { move_axis: -1|0|1, jump_pressed, down_held }
→ World::tick(dt, input)  // local FOOTNOTE for presentation
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
→ if Connection screen or overlay open: feed event to egui
→ semantic ActionState only on ClientScreen::Game (unless a text-like widget owns the keyboard)
→ if Connection: rebase last_instant; do not SimulationClock::advance
→ if Game: collect elapsed Duration × debug_time_scale → SimulationClock::advance
→ for each executed tick: World::tick(fixed_dt, PlayerInput)
→ update follow camera; clamp to WorldBounds
→ Connection: ConnectionFrontend::paint (logo + CONNECT); Game: parallax → AABBs → optional gizmos
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
- Tabs: **Runtime**, **Player**, **FOOTNOTE**, **World**, **Camera**, **Diagnostics**, **Network**. Player tab marks local simulation **LOCAL DEV / NON-AUTHORITATIVE**. Network tab adds last input sequence sent, input commands sent, and current semantic input.
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

The existing `Graphic/` directory stays in place. Phase 5.0B loads **only** `Graphic/LOGO.png` for the Connection Frontend (temporary filesystem path). The rest of `Graphic/` is unused until sprite and Paper Doll phases. Do not modify files in `Graphic/` as part of networking work.

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

This directory is the Git project root. Phase 0 deferred `git init`; Git was enabled later. Do not nest another repository or treat a parent folder as the project repo.
