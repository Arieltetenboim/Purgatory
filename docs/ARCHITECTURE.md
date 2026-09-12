# Architecture

PURGATORY is a custom 2D side-scrolling MMORPG engine. This document records the structural rules. The master execution specification remains `PURGATORY_CURSOR_MASTER_EXECUTION_PLAN.md`.

## What is being built

- Native desktop client first.
- Dedicated headless server.
- Server-authoritative realtime simulation.
- Shared protocol, content, and common crates.
- Data-driven content added without rewriting core engine systems.

## What is not being built yet

Phase 4 is the world/entity foundation. Phase 4.1 adds a client-only development debug overlay. Phase 4.5 adds the FOOTNOTE movement foundation. Phase 4.6 expands the hard-coded FOOTNOTE development test arena. Phase 4.8 adds camera follow, world bounds, parallax background, and debug harness improvements. Phase 5.0 adds the Quinn/QUIC session foundation (handshake, ConnectionId, RTT, Network debug tab) **without** gameplay replication. Phase 5.0B adds a Connection Frontend and a client-local connection lifecycle (manual Connect, `ConnectionAttemptId`, Game only after Welcome). Phase 5.0C hardens that lifecycle under races, concurrent clients, retries, stale events, queue pressure, and shutdown. Phase 5.0D adds semantic failure categories, bounded diagnostics, explicit idle timeout, and Network-tab observability. Phase 5.0E hardens the untrusted-peer / abuse foundation. Phase 5.0F proves that foundation converges back to baseline after soak, stress, and deterministic chaos. Phase 5.1 adds intent-only `InputCommand` and server-owned movement (protocol v2). Phase 5.2 adds bounded full `WorldSnapshot` replication and client replica rendering (protocol v3). Phase 5.3 adds client-only remote entity interpolation (no protocol bump). Phase 5.4 adds client-only local player prediction (no protocol bump; no reconciliation). Phase 5.5 adds acknowledgement and restore+replay (protocol v4). Phase 5.6 adds a development-only network impairment lab. Phase 5.7 adds a headless multiplayer load / soak / churn harness (real QUIC bots, localhost metrics). Phase 6.0–6C add runtime identity, composition, interaction, and content/maps. Phase 6D adds a World-owned spatial grid, server interest-policy AOI, protocol **v8** `ReplicationFrame` deltas on the existing persistent uni stream, and protocol **v9** DEV `DevSetChannel` so same-Map Channel membership is a real `WorldAddress` boundary. Phase **6E** adds persistent `CharacterId`, DEV login lookup, file-backed character restore (not last coordinates), and protocol **v10**. Phase **6F** adds World-owned runtime services (scheduler, actions, gates, staged events, effects, cadence, spawn schedule) without a protocol bump. Phase **6G** closed the runtime/replication architecture (dirty fan-out, selective policy foundation; production policy **tuning** deferred). Phase 7 (capacity / production scaling) is **complete + closeout**. Phase **8A** adds optional authoritative `EquipmentState` (six `Option<ContentId>` slots) without a protocol bump. Phase **8B** adds Equipment Content Schema v1 (gameplay slot + client presentation) without a protocol bump. Phase **8C** adds protocol **v12** Equip/Unequip, gameplay-only authorize, and Enter/Update equipment replication. Phase **8D** adds a client-only `CharacterPresentationState` bridge (local predicted and remote interpolated players share one `SkeletonInput` → Humanoid v0 evaluate path). Do not begin **8E**.

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

`ConnectionId` (network session) is not `EntityId` / `RuntimeEntityId` (simulation instance). Phase 5.1 binds `ConnectionId → EntityId` on the server only. Snapshots carry generational `EntityId` and an explicit `local_player_entity`. `WorldAddress` is `MapId + ChannelId + InstanceId` — logical **world membership**, distinct from transform. All three components participate in relevance isolation. **WorldAddress boundary ≠ social identity boundary** (ADR-0040): Channel/Map/Instance isolates replication, AOI, interpolation tied to old relevance, and world-bound `InteractionSession`. `InteractionCloseReason::AddressChanged` invalidates those world-bound runtime interactions, not arbitrary identity/social sessions (whisper, friends, party/guild, presence — not implemented). The DEV Channel control does not mutate client `WorldAddress` until the authoritative observer address/epoch arrives. **Transitions are readiness-gated, not timer-revealed** (ADR-0041): FadeOut hides old presentation; the **visible** presentation world stays on the source until FadeOut is fully black (`MapFade::is_fully_black` / Hold), then presentation commits; the screen may stay black while destination state is prepared; FadeIn starts only on `DestinationReady` (map) or `MembershipReady` (same-map Channel/Instance). Authoritative replica/network may already be on the destination during visible FadeOut. Map readiness ≠ Channel membership readiness. Simulation and replication continue while presentation is obscured. A WorldAddress/Map/Channel transition is also a **gameplay input barrier** (ADR-0042): the server neutralizes held input and rejects movement/actions for the FadeOut+Hold window; the client does not apply live gameplay movement until FadeIn begins. `ContentId` and `PersistentId` are separate domains (ADR-0034).

The server accept loop never awaits a full handshake. Each `Incoming` is refused at a concurrency cap of 32 or spawned as a per-connection task that owns Hello/Welcome and the live session. `SessionLease` removes the table entry exactly once (including panic unwind). `ConnectionId` is server-allocated monotonic `u64` starting at 1; the client cannot choose it. Wraparound is not handled (development scope).

### Client lifecycle (Phase 5.0B)

`ClientScreen` (`Connection` | `Game`) is what the user is viewing. `ConnectionState` is what the connection is doing. They are not merged.

Startup: `ClientScreen::Connection`, `ConnectionState::Disconnected`. The network thread may exist immediately but stays idle until an explicit Connect command. There is no startup auto-connect and no reconnect loop.

Game is entered only after a valid Welcome for the **active** `ConnectionAttemptId`, and only from `Handshaking`. QUIC transport-up is not enough. Stale events from older attempts are ignored. Disconnect invalidates the active attempt **immediately** on the client (before the runtime ack). A delayed Welcome after Disconnect cannot enter Game.

Session-local state (`ConnectionId`, RTT, `connected_since`, protocol/tick-rate) clears on disconnect/reject/new Connecting. A reconnect never reuses a stale `ConnectionId`.

While on the Connection Frontend: gameplay keys do not move the player; `SimulationClock` is not advanced. Frontend wait time is excluded by rebasing `last_instant` each frame (no catch-up burst when Game starts). Tick duration is unchanged.

The Connection Frontend is `ConnectionFrontend::paint` (egui, existing window). It is not the debug overlay. CONNECT lives on that screen. `~` may still toggle debug. Login / channel / character select are not placeholders yet.

`Graphic/LOGO.png` is loaded **once** at frontend init into an `egui::TextureHandle` via a centralized path helper (`apps/client/src/assets.rs`). This load path is temporary. Missing/decode failure logs one warning and falls back to the text title `PURGATORY`. The game client compile-embeds the four Animation Lab Headwear Side proof cells, registers their sprite metadata in the client `AssetRuntime`, and resolves Crown attachment visuals through the shared visual-key path (no filesystem loader). The rest of `Graphic/` stays unused.

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

This section records Phase 5.1 behavior. Phase 6 runtime foundations are separate.

## Authoritative snapshots (Phase 5.2)

```text
World (read-only)
→ spatial_candidates + ObserverReplicationState
→ ReplicationFrame (budgeted Enter/Update/Leave)
→ bounded epoch-tagged writer queue (cap 4)
→ server-initiated persistent uni stream
→ client drains every frame (no watch coalesce)
→ ReplicatedWorld::apply_frame
→ renderer
```

- Full snapshots through protocol v7. Phase 6D sends incremental `ReplicationFrame` records. Cadence is still one sim-tick publish attempt (30 Hz) in development; EventOnly entities skip periodic resend but still send on domain-rev change.
- Sequence: no wrap within process; client ignores older `observer_baseline_epoch`; a newer epoch resets the replica.
- Visibility is the observer AOI policy set (enter/leave rects around the observer, clamped to `WorldBounds`), not “every entity on the server”. Static platforms stay local content.
- The writer queue is bounded (cap 4). If full, intent stays pending (coalesced); the sim does not drop-and-forget a later Update without a committed Enter. One slow client cannot stall simulation. `write_all` failure tears down that session.
- Local cyan marker; remote a distinct debug color.

## Remote interpolation (Phase 5.3)

```text
WorldSnapshot view (from replica after apply_frame)
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
- Pose lerp uses each remote's **distinct-position** ticks, not adjacent snapshot copies. Selective cadence pads stale remotes on ticks that still carry other entities; collapsing those copies is required so Nearby gap-2 actually lerps instead of hold-then-jump. Presence (spawn/despawn) still follows the global snapshot bracket.
- Scalability: interpolation is entirely client-side; the server keeps a coalescing per-observer mailbox plus an ordered writer queue, not latest-wins `watch`.

## Local prediction (Phase 5.4)

```text
physical input → PlayerInput
  ├─ InputCommand → server (authority unchanged)
  └─ LocalPrediction → World::tick (FOOTNOTE) → predicted tick pose
       → remainder extra X + Y lerp between consecutive tick poses + correction offset (render only)
       → renderer + camera (local only)

WorldSnapshot view → ReplicatedWorld (never overwritten by prediction)
Remotes → InterpolationBuffer (unchanged)
```

**AUTHORITATIVE STATE ≠ LOCAL PREDICTED PRESENTATION ≠ REMOTE INTERPOLATED PRESENTATION**

- Reuses deterministic FOOTNOTE movement (gravity, jump, collision, OneWay, drop-through, friction). Client `footnote_test_stage` platforms match the server arena; static geometry is not replicated.
- Prediction clock = shared `SimulationClock` at 30 Hz. Render frames consume 0..N ticks from `clock.advance`; catch-up bounded by `MAX_CATCH_UP` / `MAX_CATCH_UP_TICKS`.
- Init/reset from replica on first local, generation change, screen/disconnect clear. **Lead** is expected RTT×speed — not a correction trigger. Phase 5.5 replays unacked commands after restore.

## Authoritative input acknowledgement and local reconciliation (Phase 5.5)

```text
SimulationClock tick
→ PlayerInput
  ├─ InputCommand (epoch, sequence) → pending window → server queue
  └─ tick_predicted_player (FOOTNOTE)

ReplicationFrame (per-recipient)
→ apply_frame → restore durable pose/vel/grounded/PlatformSupportId
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

## Controlled network impairment lab (Phase 5.6)

Development-only. Off by default. Does not change protocol v4, prediction, or interpolation parameters.

```text
winit InputCommand (after pending append)
→ mpsc
→ live_loop ImpairmentLane (optional one-way delay / stall)
→ write_client_control (same long-lived bi stream)

uni ReplicationFrame read
→ ImpairmentLane (optional one-way application-delivery delay)
→ drain all frames → replica / interp / prediction
```

- Input impairment sits **after** prediction/pending and **before** the QUIC write. Snapshots are delayed **after a successful uni-stream read**, not as server send-buffer / flow-control stall.
- Reliable ordered FIFO: no InputCommand drops, holes, or reordering in normal profiles. Overflow of the input delay queue is an explicit disconnect, not a silent hole.
- `watch` carries current config/profile. `mpsc` carries `TriggerInputStall`. Off makes queued items immediately eligible; they still drain through the bounded FIFO path (8 input writes / 4 snapshot applies per live-loop turn) so a burst cannot starve snapshot reads, config, stall, or disconnect.
- Ping/Pong RTT stays unimpaired. Overlay labels configured one-way delay separately from measured ping.
- Do not treat interpolation underrun beyond the fixed 3-tick buffer as an automatic 5.6 correctness failure; measure it first.

This section records Phase 5.6 behavior. Phase 6 runtime foundations are separate.

## Multiplayer load harness (Phase 5.7)

Headless real-QUIC bots (`purgatory-load` in `tools/bot_client`) exercise Hello → Welcome → 30 Hz `InputCommand` → snapshots. Shared `IntentNet` lives in `purgatory-protocol`; bots omit prediction, HeldCancel, and the graphical client.

```text
purgatory-load
→ shared Quinn Endpoint
→ BotSession × N
→ server FOOTNOTE + SnapshotBuilder
→ UDP LoadMetricsV1 (127.0.0.1:5002)
→ logs/load/<run>/
```

- `MAX_ENTITIES_PER_SNAPSHOT = 256` (mechanical decode bound); byte cap remains 8192.
- Default admission 32; load mode raises admission/channels via env (cap 256).
- Tick overrun = tick work > 33.333 ms (recorded; one overrun ≠ WARN).
- Snapshot physics O(N); sim-thread snapshot builds O(N²) pose copies; encode/send up to O(N²) if writers keep up.
- Artifacts: `latest.txt` vs `last_finished.txt`; charts offline only.

This section records Phase 5.7 behavior. Phase 5 closed GREEN; remaining O(N²) snapshot fan-out is a Phase 6 follow-up, not a Phase 5 blocker.

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

`move_axis` and `down_held` are held state. `jump_pressed` is an edge consumed once per simulation tick. Temporary development bindings: A / Left Arrow → MoveLeft, D / Right Arrow → MoveRight, S / Down Arrow → MoveDown, Space → Jump, **E → Interact** (intent only; server validates). **Down + Jump** while grounded on a OneWay platform drops through (FOOTNOTE); otherwise Jump is a normal grounded jump.

The physical Backquote / Grave key (`~`) is **not** a gameplay action. It toggles the client development debug overlay and is never stored in `PlayerInput`.

`purgatory-simulation` has no `winit`, `wgpu`, or `egui` dependency.

## World and entities (Phase 4)

`World` is the authoritative simulation container. It is not a global registry.

Runtime identity is [`EntityId`]: `{ index, generation }`. It is cheap to copy. The index is a storage slot, not permanent identity. After despawn, the slot’s generation advances; a later occupant in the same slot has a different `EntityId`. Lookups with a stale ID return `None`. IDs are not memory addresses, Rust references, or UUIDs. They are not serialized on the network in this phase.

**EntityId vs Content ID vs CharacterId vs Persistent ID:** `EntityId` (`RuntimeEntityId`) is a temporary runtime instance. `ContentId` is a stable authored definition string (compact FNV-1a storage is an implementation detail; ADR-0036). `CharacterId` is the server-minted persistent player identity (ADR-0043). `PersistentId` is an optional runtime-entity placeholder and is **not** a Character handle. Two green slimes share one content id and have two different `EntityId`s. Do not mix the namespaces. Do not map `CharacterId::raw()` onto `PersistentId`.

Storage is a generational slot vector plus a free list. Live records sit in a contiguous `Vec`. Vacant slots are reused. This is not an ECS and not a general object graph.

`EntityKind` is **derived** from capabilities: player → `Player`, else platform → `Platform`, else `Generic`. Monster, projectile, loot, and NPC **gameplay** are added when those systems exist; they are not separate storage families.

Every live entity has a [`WorldAddress`] and a lifecycle. [`Transform`] is optional (Phase 6A). Address is independent of coordinates. Players also store velocity, grounded, `grounded_on: Option<EntityId>`, and collider half-extents. Platforms store half-extents and `PlatformKind`. Velocity is not forced onto platforms. Optional `Health` is a life container (current/max); Alive/Dead is derived from `current > 0` / `<= 0`. Entities without Health are combat non-participants, not dead. Optional `EquipmentState` (Phase 8A) is six fixed slots of `Option<ContentId>`; all-empty is valid and is not a placeholder content id. `None` means no Equipment domain; `Some(empty)` means the domain exists with every slot empty. Phase **8B** validates content-pack slot/presentation compatibility. Phase **8C** authorizes Equip/Unequip from gameplay definitions only and replicates equipment on protocol **v12** (Enter full state, Update slot delta). Presentation fields never go on the network. Phase **9A** abilities reuse the 6F Action table (Windup/Active/Recovery); cooldown is a World-side table, not a default entity component. Instant ability damage goes through `AbilityEffect`, not ability-specific `set_health`. Phase **9B** authors `skill.basic.strike` as content: Independent activation (no selected target required) and a server-only forward AABB query at Active. Phase **9C** adds protocol **v15** `AbilityActivate` and a World `AbilityGrantTable`. Live server player spawn attaches Health and grants Basic Strike; default `spawn_player` does not. 7.2 `nearest_health_target` skips players. Players store last non-zero horizontal intent as `facing_sign` (not on the wire).

Dirty tracking is per domain (`transform` / `health` / `membership` / `replication` / `equipment`), not a single entity-dirty bit. Equipment also has a slot-level `u8` mask. Read-only queries do not mark dirty.

Optional `Interactable` is a type/intent marker. `World` owns `InteractionSession` domain state (Opened / Active / Updated / Closed). That session is **not** a UI window. The client maps server results to `UIRuntimeState` for overlay presentation. Visible ≠ interactable. Client nearest-target is advisory (ADR-0035). N10b Social NPCs remain one capability-composed entity: they replicate as `ReplicatedKind::Npc`, carry the existing optional Equipment domain as their humanoid Character Presentation facet, and retain server-side `InteractableKind::Npc` as the only interaction authority. N10 dialogue progression is a server-owned per-actor projection tied to that session; transient facts/NPC-Met/Heard state lives in a separate server `NarrativeRuntime`, while items/equipment remain owned by `World`. The protocol carries semantic indexes only, and the client resolves its own safe text/presentation projection. Full ownership and lifecycle details live in [`NPC_DIALOGUE_RUNTIME.md`](NPC_DIALOGUE_RUNTIME.md).

`World::spatial_candidates(observer: EntityId)` is the replication **candidate** set (leave-rect + class; no hysteresis, no `ConnectionId`). Observer is a runtime entity, not a connection. `ObserverReplicationState` owns enter/leave hysteresis, Known membership, and per-observer pending work. AOI enter/leave rectangles are a **server-derived visible-view envelope** (FOOTNOTE logical viewport + camera Dead Zone + clamp to `WorldBounds`) plus documented prefetch (`AOI_PREFETCH_MARGIN = 2` wu) and leave hysteresis (`AOI_LEAVE_MARGIN = 2` wu). They are not a player-centered radius and not client-authored camera coordinates. Grid cell size `4.0` wu is a tunable (ADR-0038). Missing Transform is not globally visible. Full-world player broadcast is **not** the replication architecture (ADR-0034, ADR-0038, ADR-0039).

Phase 6 closed the replication **architecture**: spatial AOI → Enter/Update/Leave → dirty-domain discovery (`InterestFanoutIndex`, `pending_update_ids`) → cadence / coalesce / priority / byte-budget policy → encode/queue. Recovery Known scanning is a fallback, not normal discovery. Idle Known scan is zero. Production **policy tuning** (thresholds, cadences, budgets) may continue in Phase 7 without rebuilding this chain. Evidence: [`PHASE_6G7B_REPORT.md`](PHASE_6G7B_REPORT.md), [`PHASE_6G7C_REPORT.md`](PHASE_6G7C_REPORT.md).

**Scaling architecture (Phase 7.7):** the canonical deployment remains **one process → one `GameplayOwner` → one authoritative `World`** (ADR-0056). Many `WorldAddress`es may live inside that World. Measured Phase 7 envelopes do not justify multithreading, parallel replication, zone servers, or multi-process handoff. Prefer local owner optimization, then optional staged replication parallelism, then independent channels/instances as the first horizontal unit, then multi-process independent worlds — only under the trigger contract in [`PHASE_77_REPORT.md`](PHASE_77_REPORT.md).

Leaving an observer's relevance does not despawn the server entity. Changing address is not destruction.

Gameplay spatial queries (`query_aabb` / `query_radius`, plus `QueryFilter` / optional cap) take `WorldAddress`. The grid already isolates address. There is no second spatial tree. Despawned entities are absent from query results.

A **Channel transition** (same `MapId` and `InstanceId`, different `ChannelId`) is a `WorldAddress` boundary. The runtime player `EntityId` stays live and Active. `World::set_address` relocates spatial-grid membership. Observer replication reuses the map-transition epoch reset (bump `observer_baseline_epoch`, clear Known / WantEnter, purge older queued frames). It is not a despawn, not a reconnect, and not a Portal. Client presentation must not rebuild map geometry or start Portal fade when only Channel/Instance changed; fade and geometry swap are **MapId** changes. Replication reset is shared; presentation is not.

Portal travel preserves the actor's current `ChannelId` and `InstanceId` unless authored transition metadata later overrides them (none exists today). ChannelId and runtime InstanceId are **not** persistent restore identity (ADR-0044). Phase 6E placement currently uses `ChannelId::DEFAULT` / `InstanceId::DEFAULT` as a temporary placement-layer implementation. Production channel allocation remains deferred.

`grounded_on` names a platform `EntityId`. If that platform is despawned, the next tick clears grounding and the player falls. Stale IDs are never dereferenced as live records.

Iteration is ordinary loops (`iter`, `iter_kind`, `iter_platforms`). No thread, task, or timer per entity.

The renderer copies AABBs from `World` each frame. It does not own lifecycle and does not keep a second entity database.

## Runtime services (Phase 6F)

`World` owns focused runtime modules. There is no `RuntimeServices` god object. `GameplayOwner::simulate_tick` orchestrates explicit stages on the simulation thread:

```text
begin_tick (SimulationTick)
→ load-validation pressure (only if load-mode + PURGATORY_LOAD_VALIDATION)
→ optional opt-in DEV probe arm (skipped when load-validation is active)
→ drain critical scheduler
→ tick_player (FOOTNOTE, every tick)
→ tick_npcs + NPC workload drive (Phase 7.2; detail leaf `npc_activity`)
→ portal / interaction maintain
→ commit runtime events once
→ cadence-due pumps
→ drain deferred scheduler (budget)
→ publish_snapshots (AOI classify + dirty fan-out + policy + encode)
```

30 Hz is tick spacing, not a requirement that every system or entity update every tick. Movement stays every tick. Cadence (`EveryTick` / `EveryN`) and replication Normal/Low intervals are staggered.

**Scheduler.** Generational `TimerId`. Critical due work should complete this tick; `CRITICAL_DRAIN_CEILING` is a pathological-overload safeguard (remainder carries forward; `scheduler_critical_ceiling_hits` counts hits). Deferred work is FIFO with a per-tick budget and a progress guarantee. Same-pass recursion is forbidden. Owner despawn cancels owned jobs.

**Actions.** A slot exists only after start succeeds. Stored live phase is `Windup`, `Active`, or `Recovery`; otherwise terminal. `ActionDenialReason::TransitionLocked` absorbs `InputGateReason` without rewriting the ADR-0042 remaining-tick lock. `InteractionSession` stays distinct from Action. Phase **9A/9B** abilities use this table (`ActionKind::Ability` + `AbilityDefinition`). Activation (`Independent` / `SelectedEntity`) is distinct from delivery (`ForwardQuery` / `SelectedEntity`). Instant effects are `AbilityEffect` (v1 `Damage`); `TempEffect` stays duration-based. 7.2 `Strike` remains workload-only. Phase **9C** is the client→server activation command; combat execution stays in `World::request_ability`. Phase **9D** maps ability execution → Attack oneshot, damage → Hurt oneshot, and `Health <= 0` → persistent Dead presentation (ADR-0063).

**Commands vs events.** Client envelopes are untrusted commands. Runtime facts are staged `RuntimeEvent` values (double buffer, commit once per tick). No global bus.

**Effects / spawn schedule.** Temporary test effects expire through the same scheduler. Scheduled spawn always calls `World::spawn` at commit (fresh `EntityId`; never reuse a despawned id). Immediate spawn remains `World::spawn`.

**Dirty/delta.** `DomainRevs` are World change versions. Dirty discovery fans an entity/domain bump to interested Known observers (`InterestFanoutIndex` → `pending_update_ids`). `ObserverReplicationState` then decides Enter/Update/Leave, cadence, coalesce, priority, and byte-budget packing. Production replication must not `consume_dirty`. AOI ≠ dirty. Visible ≠ full replicate every tick. One observer’s commit must not clear another’s pending delta. Walking all Known every tick is not the normal discovery path.

**DEV probe.** `PURGATORY_RUNTIME_PROBE=1|true|yes|on` may schedule one visible Generic after a known delay. Default off. Map-ready does not spawn automatically.

**Load validation (Phase 6G / 7.2).** Synthetic density/scheduler/action/effect/event/cadence pressure is test infrastructure. The server applies `PURGATORY_LOAD_VALIDATION` JSON only when load-mode is on (`PURGATORY_ADMISSION_CAP` set). Production `World` tick does not read that env. Phase 7.2 adds optional `npc_workload` (representative NPC activity / Strike / Pulse / respawn) behind the same env. Live UDP `PURGSTAT` remains **schema 3**. Protocol is **v11** (`ReplicatedKind::Npc`). Phase 6 architecture is closed. Phase 7 is capacity / production scaling; **7.2 COMPLETE** ([`PHASE_72_REPORT.md`](PHASE_72_REPORT.md)); do not begin 7.3 until instructed ([`PHASE_7_PLAN.md`](PHASE_7_PLAN.md)).

**Capacity characterization (6G.2 + 7.1).** Coarse per-tick domain timings and process CPU/memory are written under `PURGATORY_CAPACITY_ARTIFACT_DIR` (run artifacts only). Live UDP `PURGSTAT` is schema 3; domain timings are not on the datagram. 7.1 adds remainder/owner file schema, `capacity_live.json`, network/write-drain pressure, and server-visible connection histograms. `cpu_utilization_pct` is raw (**100 = one logical core busy**); `cpu_normalized_per_logical_pct` is machine-relative. `write_drain` is write/drain/backpressure latency, not automatically QUIC CPU. Localhost ladder numbers are not player-capacity claims. Phase 6 AOI/replication architecture is closed (6G.7A–C). Remaining capacity work is Phase 7 and starts from measurement, not from an assumed 128-client or AOI bottleneck (ADR-0053; [`PHASE_7_PLAN.md`](PHASE_7_PLAN.md)).
Error containment:

```text
invalid command → typed reject
stale runtime target → controlled no-op/cancel
expired/cancelled timer → cannot fire twice
missing owner → cleanup
internal impossible invariant → debug_assert according to project rules
```

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

`WorldBounds { min_x, max_x, min_y, max_y }` lives on `World`. The player is clamped at horizontal (and soft vertical) edges with outward velocity zeroed — no teleport correction. Falling well below `min_y` triggers a **development** respawn at stage spawn (not a final death system). Camera clamping is presentation-only and reads the same bounds. Server AOI derives a legal-camera view envelope from the observer pose, those bounds, the FOOTNOTE viewport, and the same Dead Zone half-extents as the client camera, then adds prefetch/leave margins. The client does not send camera coordinates.

## Client presentation

Rendering lives only in `apps/client`. The client uses `winit` 0.30.13 and `wgpu` 30.0.1.

```text
platform events (including keyboard)
→ Backquote/~ handled as client debug toggle (never PlayerInput)
→ if Connection screen or overlay open: feed event to egui
→ semantic ActionState only on ClientScreen::Game (unless a text-like widget owns the keyboard)
→ if Connection: rebase last_instant; do not SimulationClock::advance
→ if Game: collect elapsed Duration × debug_time_scale → SimulationClock::advance
→ for each executed tick: World::tick(fixed_dt, PlayerInput) / local prediction tick
→ replica apply + reconcile (poll, before or with ticks)
→ finalize local presentation pose once (X remainder extra from last local tick velocity; Y lerp between consecutive predicted FOOTNOTE poses using remainder/tick as alpha + correction-smoothing offset; not the remote interp buffer)
→ update follow camera from that same pose (Dead Zone containment + damp, then clamp to WorldBounds)
→ Connection: ConnectionFrontend::paint (logo + CONNECT); Game: parallax → AABBs (local player uses the finalized presentation pose) → optional gizmos
→ optional development egui overlay
```

Rendering continues when zero simulation ticks are due. Frame delta is never the simulation step. Movement executes only inside those fixed ticks.

S1 humanoid skeleton math lives in `purgatory-skeleton` (Definition + local Pose → world Pose). S2 adapter evaluates one Humanoid v0 on the local presented pose; debug draw is Stage D (all 16 bones, owner-accepted) plus baseline **1.15×** character presentation (2× is a diagnostic preview only). R1 oriented quads, convex four-corner solids, and P1–P4 placeholders use the existing primitive pass ([`CHARACTER_SKELETON_ARCHITECTURE.md`](CHARACTER_SKELETON_ARCHITECTURE.md)). Phase **8B** names presentation `BoneTarget`s after Humanoid v0 labels (`head`, `torso`, `upper_arm_front`, …) but does not depend on this crate; Root/Pelvis are not equipment targets. Phase **8D** binds those targets to `BoneIndex` in the client presentation set and evaluates bind pose per visible player. Phase **8E** composes attachments and draws debug placeholders from that set for every visible player (local and remote share the path). Phase **8F-A** owns the painter's-algorithm contract (`ArmBack → LegBack → Core → LegFront → Head → ArmFront`); attachments follow their `BoneTarget` layer and do not carry unrestricted z. Phase **8F-B** applies Front/Back visibility on that same table (`PresentationView`); attachments inherit bone-layer visibility; no numeric z and no second order table. Phase **8F-C** selects that view from `PresentationActivity` (`ClimbBack` → `Back`; other current activities → `Side`). Phase **8F-D** samples authored `climb_back.anim` for `ClimbBack` on the same Animation Runtime path as Idle/Move (invalid parse → bind/no-animation, not Idle). Phase **8F-E** selects equipment `visuals.side` / `visuals.back` from `PresentationView` (missing Back is omitted; no Side-as-Back). Phase **8F-F** audited that path and closed 8F (no new presentation features). **Phase 8 closeout complete.** Stage D joint overlay remains the local S2 debug draw. Animation sampling lives in `purgatory-animation` above Skeleton Core (sample clip → `LocalPose` → evaluate); the skeleton crate stays clip-unaware ([`CHARACTER_ANIMATION_ARCHITECTURE.md`](CHARACTER_ANIMATION_ARCHITECTURE.md)). **8E is complete; do not modify/reopen/extend it** from the animation track.

Logical world coordinates are independent of physical pixels. The FOOTNOTE arena uses a taller logical viewport height (`FOOTNOTE_TEST_VIEWPORT_HEIGHT`). The client camera uses a **Dead Zone** around the current camera center plus frame-rate-independent exponential follow (named DEV tunables in `apps/client/src/camera_follow.rs`). The zone is a free-movement box: the camera does not move while the player is inside it. Crossing an edge moves the camera only enough to keep the player at that boundary (excess only; the target does not jump to player center and does not recenter). When the player stops just outside an edge, the same damper finishes residual containment so they settle on that edge. Horizontal and vertical half-extents and smooth times are independent. Map destinations snap/seed the camera to the dest local pose (no smoothing across maps). Channel/Instance commits preserve the current camera pose. Viewport clamping still keeps the camera inside `WorldBounds`. Camera pose is not AOI.

### Client display configuration

Display settings are **client presentation only**. They are not stored on `World`, not replicated, and not known to the server.

- **Owner:** `apps/client/src/display.rs` (`DisplaySettings` / `DisplayController`). Debug UI and a future game Settings menu submit the same intent (`DisplayController::set_resolution`, `DisplayController::set_render_scale`).
- **Default window:** **1280×720 physical pixels**, windowed (`Resolution::DEFAULT`). This was the previous hard-coded `DEV_WINDOW_WIDTH` / `DEV_WINDOW_HEIGHT`.
- **Window size** — OS window / swapchain (`Window::inner_size()` physical pixels, including DPI).
- **Internal world render** — offscreen color target at **Render Scale** × the locked 16:9 gameplay pixel rect (not the full window). World geometry, character presentation, and skeleton debug primitives all draw into this target. Equipped visuals resolve through the client `AssetRuntime` and are emitted as textured quads in this same pass (nearest sample, no mips; world blit stays linear). Presets: 50% / 75% / 100% / 125% / **200%**. Above 100% supersamples, then downsamples on blit with the current **linear** filter. Production default **200%** (integer 2:1). Performance fallback **100%**. 150% is not a quality-policy preset. 400% exists only on the RF diagnostic compositor, not as a Display preset. Independent of window size and of camera FOV — scale changes fidelity only, never gameplay visibility. The world pass defaults to **4× MSAA** when the offscreen format (`Rgba8UnormSrgb`) supports 4× + resolve; it resolves into the single-sample target that is blitted. That is coverage for polygon edges, not a size compensation. **1× MSAA** (`Off`) is compatibility / diagnostic fallback only. DEV Debug → Display can switch Off vs 4× for comparison. Switching does not change camera FOV or gameplay visibility. egui stays on the swapchain after the blit.
- **Presentation blit** — world target is sampled with **linear** filtering into the gameplay pixel rect on the swapchain (clamp-to-edge, no stretch). DEV RF1.5/RF2 compositor may nearest-blit diagnostic panels only; that does not change production filtering. Letterbox/pillarbox bars are the swapchain clear color. egui / debug UI stays at native output resolution.
- **UI scale** — multiplier on the OS / winit scale factor (`pixels_per_point = os_scale × ui_scale`). Independent of resolution and of render scale so a 2560×1440 window does not shrink debug/game UI. Default `1.0`. No UI-scale slider in this pass.
- **Window mode:** `Windowed` only. Borderless / exclusive fullscreen are deferred (add a `WindowMode` variant when a real apply path exists).
- **Persistence:** deferred. There is no local client settings file yet. Do not store display configuration on the server.
- **Resize lifecycle:** Debug/Settings intent → `DisplayController::set_resolution` → `Window::request_inner_size` (physical) → `WindowEvent::Resized` / immediate apply → `classify_framebuffer_resize` → `Renderer::resize` (swapchain; skip zero-size and unchanged) → `ensure_world_target` (recreate offscreen RT if gameplay-rect × scale size **or** world MSAA sample count changed) → clamp camera center to `WorldBounds` (world FOV is unchanged). Manual OS dragging uses the same observe/reconfigure path. Render Scale and world MSAA changes skip the window path and only rebuild the world target.
- **Zero-size / minimize:** width or height `0` skips surface reconfiguration; last valid camera/surface/world target is kept; draw may skip (`Occluded` / unusable surface).
- **Camera / aspect policy:** resolution is not camera zoom, and render scale is not camera zoom. Visible world is the FOOTNOTE 16:9 gameplay view (`gameplay_viewport_size` = [`aoi_viewport_size`]; aspect owned by `AOI_VIEWPORT_ASPECT`). Same-aspect pixel sizes (1280×720, 1920×1080, 2560×1440, …) show the same world. Other aspects do not expand FOV: the largest 16:9 pixel rectangle that fits the framebuffer is used (pillarbox if wider, letterbox if taller). Render Scale only changes how many internal pixels cover that rect. No independent X/Y stretch. Ultrawide does not reveal extra gameplay area. Server AOI remains the same 16:9 envelope; the client camera is not a network authority.
- **Debug UI:** Debug tab → **Display** (collapsed). Shows logical window, framebuffer, surface, selected resolution, aspect, scale factor, UI scale, window mode, monitor native size, locked gameplay view size, gameplay pixel rect, render scale (default 200%, 100% performance fallback), internal world render size (and GPU-limit clamp), world MSAA 4× production / 1× fallback, RF0 rotated-geometry scene, RF1.5–RF3 simultaneous A/B compositor (1× vs 4× MSAA, nearest vs linear 1:1, integer 100/200/400% + 4× linear downsample; diagnostic raster is independent of gameplay Render Scale; 400% is not a Display preset; 150% is not part of the quality policy), freeze-camera + probe screen-space vertices, resolution presets, and render-scale presets.
- **Resolution-dependent GPU resources:** swapchain / surface configuration, plus the offscreen world color target (and 4× MSAA target when enabled) and the blit bind group when the applied internal size or sample count changes. Primitive vertex/index/camera buffers are world-space and are not rebuilt on resize or scale change. If a requested internal size exceeds `max_texture_dimension_2d`, both axes scale uniformly to fit; the Debug UI reports the applied size as clamped. Character / skeleton primitives use the same world pass as platforms; egui / debug text stays native after the blit.

The drawn local player and the camera follow target share one per-frame **presentation pose**. That pose starts from client prediction (or replica when prediction is inactive). Remainder extra uses the velocity from the last **locally executed** predicted tick (or non-empty replay), not replica velocity after an empty-pending restore. Horizontal remainder follows last-tick `vx`. Vertical presentation lerps Y between the previous and current **tick-boundary** predicted poses (`alpha = remainder / tick`); both endpoints are post-FOOTNOTE contact-resolved. That is up to one sim tick of **vertical visual** delay, not extra input/sim latency. Falling `vy * remainder` is still not applied (floor-clip guard on extra). Speeds below `EXTRAPOLATE_MIN_SPEED` (0.5 wu/s) are treated as idle so leftover sub-walk residuals cannot modulate with `clock.remainder()`; this is not a positional dead-zone. A decaying visual offset then absorbs small restore+replay corrections so a damped camera cannot expose them as screen-space flicker. Large corrections, map/teleport/reset, hitch, and restore+replay snap Y-lerp history (`prev = current`) rather than blending unrelated poses. The offset does not feed simulation, commands, reconciliation, or server AOI. Remotes keep the delayed interpolation buffer. Header-only frames, a lagged falling replica while local is already grounded (`landing_lead`), and a lagged leftover walk `vx` while local is grounded at rest (`rest_lead`) skip restore so stale replica pose/velocity cannot rewind prediction.

**Parallax** (far ≈ 0.15, mid ≈ 0.40, near ≈ 0.70) is presentation-only: `layer_offset = camera_position * factor`. Simulation entities, collision, FOOTNOTE, and the server are unaware of background layers.

On this development machine the client selected:

- adapter: NVIDIA GeForce RTX 4050 Laptop GPU
- backend: Vulkan
- format: Bgra8UnormSrgb
- initial size: 1280×720

Zero-sized / minimized surfaces skip configure and draw. `Lost` / `Outdated` / suboptimal surfaces reconfigure. wgpu 30 reports these through `CurrentSurfaceTexture` rather than the older `SurfaceError` enum; there is no `OutOfMemory` variant on that enum. Validation failures exit. Timeouts skip the frame.

## Development debug overlay (Phase 4.1 / 4.8)

Parallel **D — Diagnostics** track (D2 complete): compact vs forensic split lives in [`DIAGNOSTICS_ARCHITECTURE.md`](DIAGNOSTICS_ARCHITECTURE.md) and [`DIAGNOSTICS_ROADMAP.md`](DIAGNOSTICS_ROADMAP.md). That track does not change gameplay protocol or root `PHASE`. Do not start D3 until instructed.

The client hosts an in-window **development** overlay. It is not production game UI and not a second OS window.

- Toggle: physical Backquote / Grave (`~`). Press opens; press again closes. OS key-repeat does not toggle. The egui close button also hides it; `~` reopens it.
- Technology: `egui` 0.36.1 + `egui-winit` 0.36.1 + `egui-wgpu` 0.36.1, integrated directly with the existing winit/wgpu client. **eframe is not used.**
- Crate boundary: egui crates may be used by `purgatory-client` (in-window overlay; **eframe is not used** there, ADR-0016), `purgatory-dev-hub` (provisional eframe GUI, ADR-0052, revisitable), and `purgatory-animation-lab` (standalone authoring window, ADR-0058). They must not enter `purgatory-simulation`, `purgatory-server`, `purgatory-protocol`, `purgatory-content`, `purgatory-common`, `purgatory-skeleton`, `purgatory-animation`, or `purgatory-dev-runtime`.
- Tabs: **Debug** (default now-state), **Runtime**, **Player**, **Skeleton**, **World**, **Camera**, **Diagnostics**, **Network**. Player tab marks local simulation **LOCAL DEV / NON-AUTHORITATIVE**. FOOTNOTE contact lives under Player (collapsed). Network forensic sections start collapsed. **Authoritative Replica** reports last-frame and session Enter/Update/Leave counts plus Known entities without an Update this frame (client-observable dirty/delta proof).
- Debug tab owns everyday **time scale** (1.0 / 0.5 / 0.25): scales wall-clock elapsed fed to `SimulationClock` only. `TICK_RATE_HZ` is unchanged. Local-client only; this cannot independently slow the server. Debug **Display** is the in-game resolution control (presets apply through `DisplayController`; not a second resize path).
- Gizmo toggles (Debug → View / gizmos): colliders, velocity, grounded highlight, world bounds, grid, parallax debug, AOI policy rects, camera Dead Zone, replica entity labels, interpolation/prediction gizmos, presentation placeholders/skeleton/AABB.
- World **Entities** inspector: categorized tree (Players / Interactables / Portals / Platforms / Other). Players, Interactables, and Portals start expanded; Platforms and Other start collapsed. Each row labels **World** vs **Replication** separately. Replica rows are observer Known-set only; local World slots are a different identity namespace and are not merged by matching `index:generation`. The **local player** is one semantic entry: `server RuntimeEntityId` and `client World EntityId` are labeled explicitly under that entry and are not peer rows.
- Network **Observer AOI** section: observer `RuntimeEntityId`, `MapId`, `ChannelId`, `InstanceId`, `WorldAddress`, enter/leave policy bounds, mailbox candidate/Known/WantEnter/WantLeave counts, replication epoch, plus Known counts per replica kind. Compact chrome shows `Map / Channel / Instance` plus DEV `[0]` `[1]` Channel buttons that send `DevSetChannel` (server-authoritative; the client does not mutate WorldAddress). Channel is not duplicated on Network → Connection. World-space labels are compact semantic chips (`LOCAL PLAYER`, `REMOTE PLAYER`, `INTERACTABLE`, `PORTAL`) with near-white glyphs, a dark outline, a drop shadow, and category color only as an accent border. The `LOCAL PLAYER` chip uses the same presented pose as the local body and camera. Namespace IDs live in the World Entities inspector, not over the entity. Labels are projected only when `maps_aligned` and the presentation world is ready (idle fade or `DestinationReady` / `MembershipReady`); they are suppressed while a transition is unaligned or still waiting. Overlay chrome shows DEV transition banners (`MAP TRANSITION · Waiting DestinationReady`, `CHANNEL TRANSITION · Waiting MembershipReady`) plus missing-flag detail and a stall warning **only while a transition is active**. The screen-space inspector stays visible during the blackout. Interaction / Target / Portal chrome is exception-driven (non-Idle, candidate exists, or portal eligible). Persistent warning chips remain visible if the overlay is closed while time scale ≠ 1×, jitter isolation, impairment, Force Back, Force ClimbBack, RF0 scene, RF A/B compositor, or RF camera freeze is active.
- Camera tab: position, viewport, presentation player, desired target, Dead Zone half-extents, smooth times, following X/Y, DEV jitter isolation (collapsed; pred/replica/presented/cam/screen X, extra Δx, remainder vx, auth tick, correction, 180-frame screen-X range, follow-X flips). Follow / Center On Player live on the Debug tab.
- Mutation: `DebugAction` values (currently `ResetPlayer`). egui must not poke arbitrary `World` fields.
- Input: movement stays live while the overlay is open except when a text-like egui widget owns key presses.
- Render order: parallax → world primitives → FOOTNOTE/debug gizmos → egui (`LoadOp::Load`).

`DebugCommand::ResetToSpawn` is client-only as a request. When the client is in Game, it sends DEV `DevResetPlayer` (protocol v14); the server resets the bound actor with `World::reset_player_entity`. The client does not apply a local spawn while connected. Offline, it applies simulation `DebugAction::ResetPlayer` locally and centers the camera. EntityIds are not changed. **Reanchor Prediction** (`DebugCommand::ResetPlayer`) snaps client prediction to the replica. Confirmations are a center-screen toast (~3s) using ASCII text only.

The DEV NPC Spawner follows the same authority boundary through its own narrow
`DevSpawnNpc` envelope. The overlay selects a stable NPC `ContentId`; the server
validates the runtime NPC definition, reads the bound player's authoritative
`WorldAddress` and position, then asks the normal content/world spawn path to
allocate a transient runtime entity. It does not edit map content, assign a
`PersistentId`, or create a second dialogue, presentation, or simulation owner.

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

common → simulation
  ↑
content
  ↑
server / client / content_validator

common
  ↑
persistence
  ↑
server

common
  ↑
dev_runtime
  ↑
dev_hub

skeleton
  ↑
dev_hub          (Humanoid v0 authoring-template SVG export; Hub-only)

skeleton
  ↑
animation
  ↑
animation_lab   (eframe; Hub-launched; ADR-0058)
```

`purgatory-skeleton` is client presentation math (Definition + local Pose → world Pose). `purgatory-client` depends on it. Simulation, server, protocol, content, common, and `purgatory-dev-runtime` must not. Animation sampling and playback live in `purgatory-animation` (depends only on `purgatory-skeleton`; sample Clip → LocalPose; A7.1 `depth_angle` projects child translation after sample; A2 `AnimationPlayer` projects Once/Loop). Client, Animation Lab, and Developer Hub (authoring-template export) may depend on it; simulation/server/protocol/content/common/`purgatory-dev-runtime` must not. See [`CHARACTER_ANIMATION_ARCHITECTURE.md`](CHARACTER_ANIMATION_ARCHITECTURE.md). Animation Lab is a Hub-launched eframe binary (`tools/animation_lab`); it does not enter the game client. The Hub Content page can export [`Graphic/character/HUMANOID_V0_AUTHORING_TEMPLATE.svg`](../Graphic/character/HUMANOID_V0_AUTHORING_TEMPLATE.svg) from live Humanoid v0 contracts (not a runtime asset; mapping in [`HUMANOID_RIG_SPEC.md`](HUMANOID_RIG_SPEC.md)).

`purgatory-content` may depend on `purgatory-simulation` to build typed spawn plans. JSON stays in the content crate. `purgatory-simulation` must not depend on `purgatory-content`, serde, or JSON.

`simulation` must not depend on `winit`, `wgpu`, renderer code, UI, or OS window APIs.

`purgatory-persistence` depends on `purgatory-common` and serde only. Simulation does not depend on it. JSON serialization and filesystem IO run on the persistence worker, not the 30 Hz simulation thread. The live data root is a per-user application-data directory (Windows `%LOCALAPPDATA%\Purgatory\`), not the source tree. `PURGATORY_DATA_DIR` overrides it.

`server` must not depend on `winit`, `wgpu`, `egui`, or the client crate.

`purgatory-dev-runtime` must not depend on `purgatory-client`, `purgatory-simulation`, Quinn, egui, winit, or wgpu. It may depend on `purgatory-common` for metrics decode. It spawns `purgatory-load --probe`; it does not open a game protocol session itself.

The existing `Graphic/` directory stays in place. Phase 5.0B loads **only** `Graphic/LOGO.png` for the Connection Frontend (temporary filesystem path). The game client compile-embeds the four Headwear Side proof cells and registers them through the shared client asset runtime; that is not a Graphic/ scan or filesystem asset loader. Do not modify files in `Graphic/` as part of networking work.

Windows development control is **Developer Tools** (ADR-0050). The current operational shell is PowerShell (`DEV.BAT` → `tools/dev/`). The target shell is the Rust Developer Hub (`purgatory-dev-runtime` + provisional `purgatory-dev-hub` GUI, ADR-0052). Both verify Ready via `purgatory-load --probe`; neither reimplements Quinn or the game protocol. Hub orchestration covers server Ready, Runtime Validation, load/soak, clients, quality gate, Rebuild, Kill All, settings, and **Animation Lab launch** (ADR-0058); it holds a workspace `logs/dev-tools/hub.lock`. The Hub must not poke `World`. Do not drive the same workspace from both shells at once. See [`docs/dev-tools/`](dev-tools/README.md).

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

Phase **8B** Equipment Content Schema v1 lives in `purgatory-content` (`content/shared/equipment/` gameplay + `content/shared/equipment_presentation/` client attachments). Presentation vocabulary (`BoneTarget`, `AnchorPoint`, `CoverageMode`, `ViewVariant`, visual keys, `hide_base`, correction) is content-only. It is not a `purgatory-skeleton` type, not stored on `EquipmentState`, and not on the protocol wire. `purgatory-content` does not depend on `purgatory-skeleton`. Phase **8C** replicates only `EquipmentSlot → Option<ContentId>` on protocol v12. Phase **8D** maps `BoneTarget` → Humanoid v0 `BoneIndex` in the client. Phase **8E** looks up presentation by that ContentId on the client only; `authorize_equip` stays gameplay-only.

## Ownership and processing

Every major object and resource has a clear owner and lifetime.

Avoid a default per-entity update loop. Prefer batched, system-level processing and event/timer driven work.

Measure before optimizing. Scalability is planned early; optimization is justified by profiling.

This directory is the Git project root. Phase 0 deferred `git init`; Git was enabled later. Do not nest another repository or treat a parent folder as the project repo.
