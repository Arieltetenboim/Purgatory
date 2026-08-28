# Roadmap

Phases execute in order. A later phase starts only after the current gate is green.

| Phase | Name | Status |
|---:|---|---|
| 0 | Bootstrap from the existing project root | complete |
| 1 | Custom runtime and clock | complete |
| 2 | Client window + placeholder renderer | complete |
| 3 | Input + local placeholder character | complete |
| 4 | World and entity foundation | complete |
| 4.1 | Development debug overlay | complete |
| 4.5 | FOOTNOTE foundation | complete |
| 4.6 | FOOTNOTE expanded development test map | complete |
| 4.7 | Collision robustness & edge-case hardening | complete |
| 4.8 | Camera, world bounds & development harness | complete |
| 5 | Network connection foundation | complete |
| 5.0B | Connection frontend + client lifecycle | complete |
| 5.0C | Connection lifecycle & race hardening | complete |
| 5.0D | Network diagnostics & failure semantics | complete |
| 5.0E | Security & abuse foundation | complete |
| 5.0F | Soak / stress / chaos hardening | complete |
| 5.1 | Authoritative intent-only input | complete |
| 5.2 | Authoritative world snapshots | complete |
| 5.3 | Remote entity interpolation | complete |
| 5.4 | Local player prediction | complete |
| 6 | Authoritative multiplayer movement | not started |
| 7 | Client reconciliation (input replay) | complete (Phase 5.5) |
| 8 | Content foundation | not started |
| 9 | Combat core | not started |
| 10 | Loot, inventory, and progression | not started |
| 11 | Persistence boundary | not started |
| 12 | Maps, zones, and interest management | not started |
| 13 | Scalability harness | not started |
| 14 | Sprite and asset pipeline | not started |
| 15 | Animation and Paper Doll | not started |
| 16 | Content authoring quality | not started |
| 17 | Hardening | not started |

## Phase 0 notes

- No nested `PURGATORY/` directory.
- At Phase 0, `Graphic/` was reserved unused and Git was deferred. Later: the Connection Frontend loads `Graphic/LOGO.png`; Git exists at this project root.

## Phase 3 notes

- Semantic input only; `KeyCode` stays in the client.
- `World` owns entities and orchestrates intent, gravity, jump, and integration. Collision detection and solid response are separate.
- `Platform` is collider data (kind, top surface); runtime identity is `EntityId`. FOOTNOTE owns advanced platform interaction (foundation in Phase 4.5).
- Renderer presents simulation AABBs. Camera is static.
- `Graphic/` remains unused (no textures, sprites, or logo).

## Phase 4 notes

- `World` owns generational `EntityId` slots. Stale IDs are rejected.
- Player and platforms are world entities. `grounded_on` is a platform `EntityId`.
- `EntityId` is not a Content ID. No ECS crate. FOOTNOTE foundation is Phase 4.5.
- Do not start Phase 5 networking until instructed.

## Phase 4.1 notes

- Client-only egui overlay over the existing winit/wgpu window. Not production UI. Not eframe. Not a second OS window.
- Toggle with physical Backquote / `~`. Inspection uses a snapshot; mutations use `DebugAction`.
- Simulation and the headless server have no egui dependency.

## Phase 4.5 notes

- FOOTNOTE module in `purgatory-simulation`: accel/decel, air control, Solid + OneWay, drop-through, contact events.
- Client: S/Down held, Down+Jump drops through OneWay; debug FOOTNOTE panel + overlay-gated world gizmos.
- Do not start Phase 5 networking until instructed.

## Phase 4.6 notes

- `World::footnote_test_stage()` hard-coded FOOTNOTE laboratory (main route, descent, multi-drop, freestyle, momentum, slope approximation, isolated overlap regression).
- Drop-through ignore clears below platform top (regression fixed); not floor-dependent.
- Vertical collision picks one nearest crossed surface (fixes overlap jump teleport).
- Static camera enlarged for the arena only. No map loader, no camera follow, no networking.
- Do not start Phase 5 networking until instructed.

## Phase 4.7 notes

- Crossing-based normal collision (previous→proposed AABB); no stale-overlap nearest-face horizontal shove.
- Separate capped Solid penetration recovery; `CONTACT_EPSILON` centralized.
- Regression matrix: Entity-16 head-bonk, seams, corners, order independence, 30/40 Hz, seeded properties.
- Debug Diagnostics tab: discontinuity detector / console log / verbose trace default OFF; 16-event history.
- Do not start Phase 5 networking until instructed.

## Phase 4.8 notes

- `WorldBounds` on `World`; player contained at left/right/top; fall-through below `min_y` development-respawns.
- Client camera follows player and clamps to world bounds (presentation only).
- 3-layer procedural parallax background (presentation only).
- Debug overlay tabs (Runtime / Player / FOOTNOTE / World / Camera), gizmo toggles, local time scale 1.0 / 0.5 / 0.25 (scales wall elapsed into `SimulationClock`; tick rate unchanged).
- Map horizontal extent ≈ 2× Phase 4.6 without doubling platform count.
- Phase 5.0 networking is implemented separately; see Phase 5.0 notes.

## Phase 5.0 notes

- Quinn/QUIC + Tokio. Hello/Welcome handshake, server-issued `ConnectionId`, datagram nonce ping/RTT.
- Debug Network tab. DEV-ONLY self-signed cert + skip-verify.
- No InputCommand, snapshots, prediction, or remote players (closed in 5.0).
- Phase 5.1 authoritative input follows.

## Phase 5.0B notes

- Connection Frontend in the existing window. Startup: Connection screen + Disconnected. No auto-connect.
- Explicit CONNECT. Client-local `ConnectionAttemptId`. Stale events ignored. Duplicate Connect during Connecting/Handshaking rejected.
- `ClientScreen` vs `ConnectionState`. Game only after Welcome from active Handshaking.
- Disconnect invalidates the attempt immediately on the client. Delayed Welcome after Disconnect cannot enter Game.
- Gameplay input gated on the frontend. `SimulationClock` is not advanced while waiting (no catch-up).
- `Graphic/LOGO.png` loaded once as an egui texture (temporary path helper). Rest of `Graphic/` unused.
- Bounded event queue: ~4-slot RTT reserve is a mitigation, **not** a lossless guarantee. Closed by Phase 5.0C split channels.
- Login / channel / character select are deferred.

## Phase 5.0C notes

- Stale `ConnectionAttemptId` events cannot mutate a newer attempt. Duplicate Connect cannot start a parallel session.
- Lifecycle events use a dedicated bounded channel; RTT telemetry is separately droppable. Disconnect/Shutdown use a watch epoch so Connect queue pressure cannot strand them.
- Server accept loop spawns per connection (cap 32, else refuse). Session remove is exactly-once via `SessionLease`. Concurrent Hello/Welcome clients get unique `ConnectionId`s.
- Default churn tests: 20 sequential connect/disconnect; 5 clients × 8 cycles. Optional ignored soaks: 100 sequential; 5×20.
- Abrupt OS process kill is not in CI. Drop of the QUIC connection is. Peer disappearance without a close frame waits for the explicit QUIC idle timeout (`IDLE_TIMEOUT` = 15 s).
- No gameplay replication. Phase 5.0D diagnostics/failure semantics follow this phase.

## Phase 5.0D notes

- Semantic `NetworkFailureKind` on the client. Wire `DisconnectReasonCode` stays peer/server-only; local-only kinds are never serialized.
- Retryability is diagnostic metadata. There is no automatic reconnect loop. Manual Disconnect and local app close are not shown as connection failures.
- Frontend uses short status strings (`Connection failed`, `Connection lost`, `Version mismatch`, `Disconnected`, …). Debug uses category labels. Quinn/rustls types stay in network modules.
- Bounded diagnostic history (48 events, no heap strings). RTT latest / min / max / EWMA reset between sessions. History does not record ping/pong.
- Explicit `IDLE_TIMEOUT` (15 s) is transport liveness, distinct from `HANDSHAKE_TIMEOUT` (5 s). Not an AFK timer.
- Server prints atomic counters on shutdown (sessions, handshakes, accepted, rejected, mismatch, malformed, timeout, clean disconnect, transport loss). Wire reason text is bounded; no paths/panics/task names on the wire.
- Future identity: `ConnectionId` ≠ `EntityId` ≠ a later server-owned `MapInstanceId`. Do not implement map instances here.
- No gameplay replication. Phase 5.0E security/abuse foundation follows this phase.

## Phase 5.0E notes

- Remote peer input is untrusted. QUIC encryption ≠ client honesty. Network data never mutates `World`.
- `NetworkAbuseConfig` centralizes limits. Admission cap 32 is concurrency safety, not player capacity. Excess connections are refused.
- Pre-handshake: one Hello, one bidi stream, bounded time/bytes, no SessionTable entry until Welcome succeeds.
- Severe violations disconnect immediately. Invalid datagrams and unknown complete control tags use per-connection budgets / control-message rate limits (server `Instant`).
- Decoders never panic on arbitrary bytes. Length prefix is validated before payload allocation.
- DEV-only cert skip-verify is explicitly named; no production flag selects it.
- No gameplay anti-cheat, accounts, or replication. Phase 5.0F soak/stress/chaos hardening follows this phase.

## Phase 5.0F notes

- The phase result is convergence, not survival: after bounded pressure stops, **active gauges must return to baseline**.
- Active gauges (`active_sessions`, `active_handshakes`, `inflight_connection_tasks`, admission permits in use) are distinct from cumulative counters (accepted, rejected, malformed, …), which never return to zero. High-water marks (`max_sessions`, `max_inflight`, `max_handshakes`) prove a bound was actually exercised.
- Chaos is deterministic: fixed seeds (`0x1`, `0xC0FFEE`, `0xDEADBEEF` in CI), a test-only operation vocabulary, and bounded delays. Failures report seed and operation index.
- Every stress/chaos family ends with a healthy probe (connect → Welcome → ping/pong → clean disconnect) and a bounded `wait_until_baseline`, never a bare sleep.
- CI soaks stay short (~2 s for the whole server suite). Heavier soaks are `#[ignore]` and run via `scripts/network_soak.ps1`.
- Localhost stress numbers are **not** player capacity. Real capacity needs gameplay state, replication, AI, persistence, combat, and bandwidth profiling.
- No limits were enlarged and no production sleeps were added. Two diagnostic classification fixes came out of the soak: a graceful client close is never counted as transport loss, and a zero-length frame counts as malformed.
- No gameplay replication in 5.0F. Phase 5.1 follows.

## Phase 5.0 final follow-up notes

- Protocol v1 wire bytes are frozen by golden vectors (`crates/protocol/tests/wire_golden.rs`). A wire change now fails a test instead of passing silently through matching encoder/decoder drift. No Protocol v1 byte changed while adding them.
- An intentional incompatible wire change means: review compatibility, update `PROTOCOL_VERSION`, then update the vectors deliberately. Never regenerate fixtures to make a test pass.
- Live QUIC shutdown during `Handshaking` is regression-tested on both sides, with a test-only listener that withholds Welcome and test peers that vanish before Welcome. No production sleeps or handshake-timing changes.
- Server bind failure is an expected startup failure: typed error, concise message, failure exit, no panic and no half-started listener. The `network listening on <addr>` log only follows a successful bind.

## Phase 5.1 notes

- Protocol v2. Intent-only `InputCommand`. Server owns movement via existing FOOTNOTE.
- `ConnectionId → EntityId` is server-assigned. Client cannot select an entity.
- Sequence + held/jump-edge semantics. Separate gameplay input rate policy.
- v1 golden vectors left frozen; v2 `InputCommand` golden vector added.
- Two clients remain isolated (no snapshots). Temporary local-dev rendering is marked **LOCAL DEV / NON-AUTHORITATIVE**.
- Closed: do not start Phase 5.2 from 5.1 notes.

## Phase 5.2 notes

- Protocol v3. Server→client full `WorldSnapshot` on a server-initiated unidirectional stream (control stream stays lifecycle/input).
- Snapshot cadence is one per simulation tick (30 Hz) for development; server-owned, independent of client FPS. Not production tuning.
- Full snapshots: entity absent from a newer snapshot is removed. Sequence is no-wrap `u32` (first accept, duplicate/stale ignore, no rewind).
- Visibility set is the shared FOOTNOTE arena's dynamic players. Static platforms are not replicated. Future MapInstance/interest management will supply the set.
- Per-connection `watch` latest-wins; slow clients lose stale snapshots. Simulation and other clients do not stall.
- Client `ReplicatedWorld` applies atomically after a complete decode. Renderer uses replica positions (stepped/delayed is expected). No interpolation, prediction, or reconciliation.
- Closed by Phase 5.3 remote interpolation.

## Phase 5.3 notes

- Client-only remote interpolation. Protocol stays at v3. Server snapshot cadence and latest-wins watch unchanged.
- Bounded history (`INTERPOLATION_HISTORY_CAP = 16`). Delay = 3 snapshot ticks (~100 ms at 30 Hz).
- Monotonic estimated/render server-tick clock: wall Instant advances the estimate; snapshots only catch up forward. Arrival jitter never rewinds `render_tick`.
- Local player: direct replica. Remotes: lerp between bracket samples A/B at `render_tick = estimated - delay`.
- No extrapolation: underrun holds newest. Spawn/despawn follow the render timeline (not packet arrival). Generations never cross-lerp. Snap distance 8 wu is presentation tuning only.
- Authoritative replica positions are never overwritten. Closed by Phase 5.4 local prediction.

## Phase 5.4 notes

- Client-only local player prediction. Protocol stays at v3. No client-position messages. Server authority unchanged.
- Same `PlayerInput` / FOOTNOTE `World::tick` path as the authoritative simulation (shared client stage geometry).
- State separation: `ReplicatedWorld` (auth) ≠ predicted local presentation ≠ remote `InterpolationBuffer` poses.
- Fixed 30 Hz via `SimulationClock` / `TICK_DURATION`. Catch-up bounded by existing `MAX_CATCH_UP` (no unlimited prediction loop).
- Init/re-anchor from replica on first local, entity generation change, disconnect/screen clear, **aligned residual** ≥ `PREDICTION_REANCHOR_DISTANCE` (8 wu), lead ≥ 8 wu safety, or settled vertical path-split. Visible lead (pred ahead of lagged auth) is expected — **no** Phase 5.5 input-replay yet; residual aligned error from unacked inputs is the 5.5 input.
- **Divergence vs lag:** the aligned residual is the leftover after matching the snapshot against the predicted history ring (position + velocity), bounded by `PREDICTION_MAX_PLAUSIBLE_LAG_TICKS` (16). Sustained residual (`PREDICTION_ALIGNED_RESIDUAL_WU` for `PREDICTION_DIVERGENCE_SNAPSHOTS`) triggers an offset drift correction that keeps the lead. Raw pred-vs-latest-auth distance never triggers a correction.
- **Input tick alignment:** `InputCommand` is emitted from the same `PlayerInput` sample that local prediction consumes on each fixed sim tick (not on raw keydown). Focus-loss still flushes Neutral immediately. This removes the systematic ±1 tick press/jump/release phase offset that caused platform walk-off and failed authoritative jumps.
- Camera follows predicted local pose. Focus-loss clears held input for both networking and prediction.

## Phase 5.5 notes

- Protocol v4. Per-tick `InputCommand` with `(input_epoch, sequence)`. Per-recipient snapshot header: ack, epoch, local contact (`PlatformSupportId`), continuation debt.
- Server always simulates at 30 Hz (Consumed or Continuation). Late-collapse is intentional authoritative input compaction: intermediate historical held commands may be acknowledged without individual physics steps. Continuation debt saturates and never wraps.
- Client restore+replay via `tick_predicted_player` / `tick_player`. Hitch is a prediction discontinuity (one new command, not N). HeldCancel uses an immutable `(epoch, target_sequence)` barrier.
- `jump_pressed` OR during late-collapse is not a generic skill-action policy (ADR-0031).
- Closed: do not start Phase 5.6 (latency lab / combat / skills) until instructed.

## Priority when tasks compete

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
