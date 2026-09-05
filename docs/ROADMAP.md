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
| 5.5 | Authoritative input ack + reconciliation | complete |
| 5.6 | Controlled network impairment lab | complete |
| 5.7 | Multiplayer load / soak / churn harness | complete (GREEN) |
| 6.0 | Runtime foundation and replication contracts | complete |
| 6A | Runtime model (composition, spawn, dirty tracking) | complete |
| 6B | Interaction + UI runtime | complete (automated); manual window pending |
| 6C | World + content runtime | complete (automated); portal refinement pending manual check |
| 6D | Runtime query, AOI, replication relevance | complete (automated); **TRANSITION INPUT BARRIER READY FOR USER CHECK** |
| 6E | Character + persistence | complete (automated); **manual first-connect / reconnect / restart / duplicate-login check still required** |
| 6F | Runtime gameplay readiness | complete (automated); **manual two-client dirty/AOI + opt-in probe check still required** |
| 6G | Runtime hardening + integrated scale validation | **GREEN 2026-09-01** — architecture closed; production policy tuning deferred |
| 6G.2 | Capacity characterization (instrument + ladder + soak) | **recorded 2026-09-01** — closed |
| 6G.3 | Targeted AOI fan-out optimization | **recorded 2026-09-01** — idle@256 AOI/tick ↓; motion AOI remains |
| 6G.4 | Motion / capacity gate (measure only) | **recorded 2026-09-01** — **A: GREEN WITH KNOWN LIMIT** |
| 6G.5 | Incremental AOI invalidation | **recorded 2026-09-01** — global gen removed; locality proven; hotspot@256 AOI↓ |
| 6G.6 | AOI locality + relevance replication characterization | **recorded 2026-09-01** — recommendation **D** (drove 6G.7A–C) |
| 6G.7A | Exact incremental AOI invalidation (enter/leave XOR) | **recorded 2026-09-01** — dirtied/move ≈1.5–1.8; accepted/closed for AOI scope |
| 6G.7B | Dirty-driven replication fan-out foundation | **recorded 2026-09-01** — scanned/update ≈1.3; idle scanned=0; accepted/closed for fan-out scope |
| 6G.7C | Relationship / density / budget policy foundation | **recorded 2026-09-01** — selective ≈3× fewer hotspot updates; **accepted; closes 6G architecture** |
| 6G.4 | Performance regression gate | deferred follow-up — budgets from measured results (not required to reopen 6G) |
| 7 | Capacity, parallelism & production scaling | **7.closeout complete — Phase 7 + closeout**. Plan: [`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md); audit: [`PHASE_7_WEAK_CLIENT_AUTHORITY_AUDIT.md`](PHASE_7_WEAK_CLIENT_AUTHORITY_AUDIT.md) |
| 8A | Equipment data model | **complete** — authoritative slots + dirty mask; no UI, no skeleton, no protocol bump. Report: [`PHASE_8A_REPORT.md`](PHASE_8A_REPORT.md) |
| 8B | Equipment content schema + validation | **complete**. Report: [`PHASE_8B_REPORT.md`](PHASE_8B_REPORT.md) |
| 8C | Authoritative equip / unequip flow | **complete**. Protocol **v12**. Report: [`PHASE_8C_REPORT.md`](PHASE_8C_REPORT.md) |
| 8D | Character presentation bridge | **complete**. Protocol **v12** unchanged. Report: [`PHASE_8D_REPORT.md`](PHASE_8D_REPORT.md) |
| 8E | Equipment attachment composition + debug placeholders | **complete**. Protocol **v12** unchanged. Report: [`PHASE_8E_REPORT.md`](PHASE_8E_REPORT.md) |
| 8F-A | Character presentation draw-order contract | **complete**. Protocol **v13** unchanged. Report: [`PHASE_8F_A_REPORT.md`](PHASE_8F_A_REPORT.md) |
| 8F-B | Front/Back visibility | **complete**. Protocol **v13** unchanged. Report: [`PHASE_8F_B_REPORT.md`](PHASE_8F_B_REPORT.md) |
| 8F-C | Activity → PresentationView | **complete**. Protocol **v13** unchanged. Report: [`PHASE_8F_C_REPORT.md`](PHASE_8F_C_REPORT.md) |
| 8F-D | ClimbBack animation clip wiring | **complete**. Protocol **v13** unchanged. Report: [`PHASE_8F_D_REPORT.md`](PHASE_8F_D_REPORT.md) |
| 8F-E | Equipment Side/Back visual selection | **complete**. Protocol **v13** unchanged. Report: [`PHASE_8F_E_REPORT.md`](PHASE_8F_E_REPORT.md) |
| 8F-F | 8F closeout / audit | **complete**. Root `PHASE` was `8F`. Protocol **v14** unchanged. Report: [`PHASE_8F_F_REPORT.md`](PHASE_8F_F_REPORT.md) |
| 8.closeout | Phase 8 closeout | **complete**. Root `PHASE` was `8.closeout`. Protocol **v14** unchanged. Report: [`PHASE_8_CLOSEOUT_REPORT.md`](PHASE_8_CLOSEOUT_REPORT.md). |
| 9A | Ability foundation contracts | **complete**. Protocol **v14** unchanged. Report: [`PHASE_9A_REPORT.md`](PHASE_9A_REPORT.md). |
| 9B | Basic Attack executable path | **complete**. Protocol **v14** unchanged. Report: [`PHASE_9B_REPORT.md`](PHASE_9B_REPORT.md). |
| 9C | Ability command path | **complete**. Protocol **v15**. Report: [`PHASE_9C_REPORT.md`](PHASE_9C_REPORT.md). |
| 9D | Ability → Character Presentation | **complete**. Protocol **v15** unchanged. Report: [`PHASE_9D_REPORT.md`](PHASE_9D_REPORT.md). |
| 9E | Minimal Creature Combat Driver | **complete**. Root `PHASE` = `9E`. Protocol **v15** unchanged. Report: [`PHASE_9E_REPORT.md`](PHASE_9E_REPORT.md). |
| 8 | Content foundation | legacy numbering (superseded) |
| 9 | Combat core | legacy numbering (superseded) |
| 10 | Loot, inventory, and progression | legacy numbering (superseded) |
| 11 | Persistence boundary | legacy numbering (superseded) |
| 12 | Maps, zones, and interest management | legacy numbering (superseded) |
| 13 | Scalability harness | legacy numbering (superseded) |
| 14 | Sprite and asset pipeline | legacy numbering (superseded) |
| 15 | Animation and Paper Doll | legacy numbering (superseded) |
| 16 | Content authoring quality | legacy numbering (superseded) |
| 17 | Hardening | legacy numbering (superseded) |

Legacy roadmap numbering is **superseded by the post-6G roadmap**. Historical completed work is unchanged: client reconciliation (legacy table Phase 7 / master-plan §14) shipped as Phase 5.5; maps/content, character persistence, AOI/interest, and scale harness shipped inside Phase 6 / 5.7 without consuming legacy rows 8–13. Rows 8–17 remain historical product direction from the master execution plan; they are **not** the next implementation order. **Phase 6 is closed** (6G GREEN — architecture closed; production replication-policy **tuning** deferred to Phase 7.4, not an architecture reopen). Post-6G Phase 7 is **capacity, parallelism & production scaling** ([`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md)), **not** a second AOI/replication-architecture phase and **not** the superseded gameplay-vocabulary draft. **7.1–7.8 complete** ([`PHASE_78_REPORT.md`](PHASE_78_REPORT.md); ADR-0056). **Phase 7 closeout complete** ([`PHASE_7_WEAK_CLIENT_AUTHORITY_AUDIT.md`](PHASE_7_WEAK_CLIENT_AUTHORITY_AUDIT.md); Hub Phase 7 Stats). **Post-7 Phase 8 is Character Presentation + Equipment Runtime**, distinct from legacy table row 8. **8A** is the equipment data model. **8B** is the equipment content schema. **8C** is authoritative equip/unequip + replication. **8D** is the client character presentation bridge. **8E** is attachment composition + debug placeholders. **8F-A** is the Character Presentation draw-order contract. **8F-B** is Front/Back visibility. **8F-C** is activity → PresentationView. **8F-D** is ClimbBack → `climb_back.anim`. **8F-E** is equipment Side/Back visual keys. **8F-F** is the 8F closeout/audit. **8F is complete + closed.** **Phase 8 closeout complete** ([`PHASE_8_CLOSEOUT_REPORT.md`](PHASE_8_CLOSEOUT_REPORT.md)). **Phase 9A complete** ([`PHASE_9A_REPORT.md`](PHASE_9A_REPORT.md)). **Phase 9B complete** ([`PHASE_9B_REPORT.md`](PHASE_9B_REPORT.md)). **Phase 9C complete** ([`PHASE_9C_REPORT.md`](PHASE_9C_REPORT.md)). **Phase 9D complete** ([`PHASE_9D_REPORT.md`](PHASE_9D_REPORT.md)). Do not begin **9E** until instructed.

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

## Phase 5.6 notes

- Development-only network impairment lab. Off by default. Protocol stays v4. No prediction/interpolation retune.
- Reliable-stream loss is delay/stall/HOL, not InputCommand disappearance. Snapshot skip is application-level. Snapshot delay is post-uni-read, not QUIC send-buffer stall.
- Closed by Phase 5.7 load / soak / churn harness.

## Phase 5.7 notes

- Real headless QUIC bots (`purgatory-load`); no second fake stack; no `purgatory-client` dependency.
- Mechanical entity decode bound 256; default admission 32; load-mode admission 256.
- Localhost UDP metrics v1 + in-process Working Set; missed poll ≠ zeros.
- Gate is infrastructure + truthful metrics + a soak this machine can hold — not a 100-bot PASS.
- Authoritative close: 5.7 steady-state input-handoff isolation (`logs/load/capacity/20260829_002013/steady_input_final_report.md`). After-fix matrix: `input_handoff_dropped` = 0 at 100/150/200/256 connected.
- **PHASE 5 GREEN — Phase 6 may begin.** Non-blocking follow-ups: admission/entity wall 256; full-visibility O(N²)-like network fan-out; snapshot/sim-thread pose-copy pressure; WorldAddress / visibility / relevance / dirty tracking / replication budgeting.

## Phase 6.0 notes

- Runtime contracts: `WorldAddress`, identity separation, lifecycle, query, visibility/relevance, replication metadata (ADR-0034).
- `World::relevance_for(RuntimeEntityId)` feeds SnapshotBuilder. Same-address players keep Phase 5 snapshot contents. Protocol stays v4.
- Full-world broadcast is not the final replication architecture. Spatial AOI / scheduling / budgets remain later (6D).
- Report: [`docs/PHASE_60_REPORT.md`](PHASE_60_REPORT.md).

## Phase 6A notes

- Composition on the existing slot-vector `World`: optional Transform / Health / player / platform. `EntityKind::Generic` for non-player, non-platform entities.
- Per-domain dirty flags (`transform` / `health` / `membership` / `replication`). No replication scheduler. Protocol stays v4.
- Report: [`docs/PHASE_6A_REPORT.md`](PHASE_6A_REPORT.md).

## Phase 6B notes

- Authoritative interaction + client UI-runtime. `InteractionSession` is not a UI window. Protocol v5. Nearest-target is advisory. Replica-only interactable drawing (no local visual substitute). Automated gate GREEN; user must still confirm E opens/rejects in the overlay.
- Report: [`docs/PHASE_6B_REPORT.md`](PHASE_6B_REPORT.md).

## Phase 6C notes

- Authored-string `ContentId`, shared vs server-only domains, registry-assigned `MapId`, content-driven Map A/B, lazy `ensure_map`/`destroy_map` (dev-eager A+B in `GameplayOwner`). Protocol v7 observer `WorldAddress` plus `ReplicatedKind::Portal` / `PortalActivate`. Portal travel uses entity `transition.{map,portal}` metadata, not `InteractableKind` matching. Arrival is at the linked portal. Activation is Up Arrow + centered zone; E is generic only. Overlay reports nearest generic, nearest portal, and portal eligible separately.
- Report: [`docs/PHASE_6C_REPORT.md`](PHASE_6C_REPORT.md).

## Phase 6D notes

- World-owned uniform grid per `WorldAddress`; cell size `4.0` wu is a tunable (ADR-0038).
- Server interest-policy AOI enter/leave rects; hysteresis only in `ObserverReplicationState`.
- Protocol **v8** `ReplicationFrame` on the existing one-uni-stream transport. Coalescing mailbox, epoch-aware writer queue cap 4, size-aware progressive Enter (ADR-0039). Protocol **v9** `DevSetChannel` (tag 17). Channel-only WorldAddress transition: live EntityId, spatial relocate, observer epoch reset, membership fade + `MembershipReady` (no map rebuild; ADR-0040 / ADR-0041). **WorldAddress boundary ≠ social identity boundary.** **Transitions are readiness-gated, not timer-revealed.** Visible presentation commits at fully black. **Transition gameplay input barrier** (ADR-0042): server-neutralized held input; client lock until FadeIn. Client camera Dead Zone + exponential follow is presentation-only. Local draw and camera share a finalized presentation pose: remainder extrapolation of the last predicted tick plus a decaying visual offset for small reconcile pops.
- Load placement `PURGATORY_LOAD_PLACEMENT=cluster|spread|maps` for Scenario A/B/C characterization. Scenario B keeps the same movement/profile/dirty-rate.
- Evidence: [`docs/PHASE_6D_PERFORMANCE.md`](PHASE_6D_PERFORMANCE.md). Report: [`docs/PHASE_6D_REPORT.md`](PHASE_6D_REPORT.md).

## Phase 6E notes

- Protocol **v10** DEV `Hello.dev_login`. Server-minted `CharacterId`. Same login → same Character; reconnect → new `EntityId`; duplicate live Character → `AlreadyConnected`.
- File-backed `purgatory-persistence` in a per-user application-data directory (Windows: `%LOCALAPPDATA%\Purgatory\`; `PURGATORY_DATA_DIR` override). Source/install location is not the writable persist root. Identity allocation is serialized on one worker. Saves are revision-protected; JSON/filesystem stay off the 30 Hz sim thread.
- RestoreIntent ≠ WorldAddress (ADR-0044). Authored map restore policy. Phase 6E placement uses DEFAULT channel/instance as a temporary placement-layer implementation.
- Welcome only after spawn/bind/replication-ready. Report: [`docs/PHASE_6E_REPORT.md`](PHASE_6E_REPORT.md).

## Phase 6F notes

- World-owned scheduler (Critical ceiling + Deferred budget), lean Action lifecycle, typed action gate (`InputGateReason` → `TransitionLocked`), staged `RuntimeEvent`, test effects, cadence stagger, scheduled spawn. No protocol v11. No `RuntimeServices` bag.
- Dirty/delta is existing `DomainRevs` + per-observer `ObserverReplicationState`. Metrics are domain-rev advances and observer pending Enter/Update, not a global dirty-pending gauge.
- Opt-in DEV probe: `PURGATORY_RUNTIME_PROBE=1`. Default off.
- Report: [`docs/PHASE_6F_REPORT.md`](PHASE_6F_REPORT.md). Stop before the next phase.

## Phase 6G notes

- Final Phase 6 hardening / integrated validation. Protocol **v10**. Adopted live `PURGSTAT` contract is **schema 3** (schema 4 execution totals were inventoried/deferred). Synthetic pressure is `PURGATORY_LOAD_VALIDATION` JSON, applied only in load-mode. MixedRuntime is the canonical workload (persistent real clients + churn + in-zone portal + AOI over `--duration`). Soak duration is configurable; ~30 min Mixed is initial evidence, not a magic threshold.
- AOI interest is a server-derived visible-view envelope (FOOTNOTE viewport + Dead Zone + camera clamp) plus 2 wu prefetch and 2 wu leave hysteresis, not a player-centered `[16, 9]` radius. Client camera coordinates are not trusted.
- Welcome does not expose `CharacterId`. Queue inventory before new caps: [`docs/PHASE_6G_QUEUE_INVENTORY.md`](PHASE_6G_QUEUE_INVENTORY.md).
- Report: [`docs/PHASE_6G_REPORT.md`](PHASE_6G_REPORT.md). Exit review: [`docs/PHASE_6_EXIT_REVIEW.md`](PHASE_6_EXIT_REVIEW.md). **6G = GREEN — architecture closed, production policy tuning deferred.** Next: [`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md) (capacity / production scaling; starts at 7.1 instrumentation). **Not started** until instructed.
- **6G endpoint reinterpretation (ADR-0053):** 6G.1 correctness → **6G.2** capacity characterization → 6G.3–6G.7C remediation/foundation → GREEN. Freeze doc: [`docs/MMO_RUNTIME_BASELINE.md`](MMO_RUNTIME_BASELINE.md).
- Local-player standing jitter and floor-clip/landing are **closed** (presentation path + owner confirmation). Duration overlay (`timeout >= duration`) is closed in CLI.
- The previously observed ~128-client load stall was **investigated with 6G.2 timings** and **not reproduced** as a server-domain hang/blow-up on this machine (see [`PHASE_6G2_REPORT.md`](PHASE_6G2_REPORT.md)). `PREDICTION_PENDING_CAP` stays 128.
- Capacity Pass 1 recorded: coarse tick domains + process CPU/memory as **run artifacts** (`PURGATORY_CAPACITY_ARTIFACT_DIR`). Report: [`docs/PHASE_6G2_REPORT.md`](PHASE_6G2_REPORT.md).
- **6G.3 (targeted AOI):** skip steady-interest classify + single unsorted candidate query. Idle@256 AOI/tick ≈ half; continuous-motion AOI still O(observers×candidates). Report: [`docs/PHASE_6G3_REPORT.md`](PHASE_6G3_REPORT.md).
- **6G.4 (motion gate, measure only):** distributed + hotspot @ 64/128/256. Classification **A — GREEN WITH KNOWN LIMIT** (128 motion stable with headroom; 256 characterized limit). Report: [`docs/PHASE_6G4_REPORT.md`](PHASE_6G4_REPORT.md).
- **6G.5 (incremental AOI invalidation):** replaced global `interest_generation` with per-observer dirty + spatial influence invalidation. Report: [`docs/PHASE_6G5_REPORT.md`](PHASE_6G5_REPORT.md).
- **6G.6 (locality + relevance characterization):** recommendation **D** (drove 6G.7A–C). Report: [`docs/PHASE_6G6_REPORT.md`](PHASE_6G6_REPORT.md).
- **6G.7A (exact AOI XOR invalidation):** influence prefilter + enter/leave XOR; tiny-move dirties mover only. Report: [`docs/PHASE_6G7A_REPORT.md`](PHASE_6G7A_REPORT.md).
- **6G.7B (dirty replication fan-out foundation):** entity/domain dirty → interested-observer pending → existing cadence/budget packer. Report: [`docs/PHASE_6G7B_REPORT.md`](PHASE_6G7B_REPORT.md).
- **6G.7C (relationship/density/budget policy foundation):** same protocol; selective stranger cadence/domain/priority; hotspot updates ≈3× down vs baseline. Accepted recommendation **A**. Report: [`docs/PHASE_6G7C_REPORT.md`](PHASE_6G7C_REPORT.md). Design: [`docs/PHASE_6G7C_DESIGN.md`](PHASE_6G7C_DESIGN.md). **Closes 6G architecture.**

## Phase 7 notes

- Status: **7.closeout complete — Phase 7 complete + closeout**. Root `PHASE` was `7.closeout`. Phase 6 is closed. **7.1–7.8 complete** ([`PHASE_78_REPORT.md`](PHASE_78_REPORT.md); ADR-0056). Closeout: Hub Phase 7 Stats + weak-client authority audit ([`PHASE_7_WEAK_CLIENT_AUTHORITY_AUDIT.md`](PHASE_7_WEAK_CLIENT_AUTHORITY_AUDIT.md)). Next: post-7 Phase 8 (Character Presentation + Equipment Runtime).
- Purpose: evidence-backed single-process capacity — measure ownership, introduce a small representative gameplay workload, isolate bottlenecks, then optimize or redesign only when justified. Canonical text: [`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md).
- Sub-stages: 7.1–7.6 complete → 7.7 remain single-process → **7.8 production performance gate** (`scripts/phase_78_gate.ps1`; verdict YELLOW).
- Starts from measurement. 128-client degradation is **not** an established server bottleneck; 6G.7B/C showed cheap replication at 128 and healthy server-side 256 under the validated workload. Higher-load timeouts are unattributed (simulation/tick vs server transport vs harness/client).
- A previous 7A–7E gameplay-vocabulary outline is **superseded** and deferred after Phase 7. Do not treat Phase 7 as “implement AOI/replication scaling” — that closed in 6G.7A–C.
- Legacy “Phase 7 = client reconciliation” remains historically true as Phase 5.5; that numbering is superseded for work after 6G.

## Phase 8A notes

- Authoritative optional `EquipmentState` on World entity slots: six fixed slots (`Headwear`…`Weapon`) of `Option<ContentId>`. All-`None` is valid. No sentinel ContentId.
- Slot-level dirty is a `u8` mask on the existing equipment domain (`DirtyFlags` / `DomainRevs` / `ReplicationDirtyMask`). Transform/health domains are unchanged. Protocol **v11** is unchanged: equipment is not on the wire.
- Full vs delta codec lives in simulation for size measurement only. Equip request, presentation, skeleton, ART, inventory, and combat are out of scope.
- Report: [`docs/PHASE_8A_REPORT.md`](PHASE_8A_REPORT.md). **8B** follows (content schema).

## Phase 8B notes

- Equipment Content Schema v1 in `purgatory-content`: gameplay `{ schema_version, id, equipment_slot }` plus client presentation `{ id, attachments[] }` sharing `ContentId`.
- Presentation vocabulary: `BoneTarget`, `AnchorPoint`, `CoverageMode`, `ViewVariant` (Side required, Back optional). Not on the wire. Protocol **v11** unchanged.
- Report: [`docs/PHASE_8B_REPORT.md`](PHASE_8B_REPORT.md). **8C** follows (authoritative equip/unequip).

## Phase 8C notes

- Protocol **v12**: Equip/Unequip requests, Accepted/Rejected lifecycle, equipment domain on Enter/Update.
- Server authorizes from gameplay `EquipmentDefinition` only. Last unequip keeps `Some(empty)`. Observer `CommittedRevs` tracks equipment.
- Report: [`docs/PHASE_8C_REPORT.md`](PHASE_8C_REPORT.md). **8D** follows (character presentation bridge).

## Phase 8D notes

- Client `CharacterPresentationState` + shared `SkeletonInput` path for local predicted and remote interpolated players.
- `BoneTarget` → Humanoid v0 `BoneIndex` bound once per presentation set. Equipment is carried, not resolved.
- Report: [`docs/PHASE_8D_REPORT.md`](PHASE_8D_REPORT.md). **8E** follows (attachment composition).

## Phase 8E notes

- Client resolves equipped ContentIds to `BoundAttachment[]` on equipment change only. Compose is `bone world ∘ anchor local ∘ correction`.
- Anchors are Humanoid v0 rig constants (`ANCHOR_CROWN` / `CHEST` / `GRIP` / `FOOT`). Debug shapes follow attachment semantics, not visual-key substrings.
- Placeholders for every visible player come from `CharacterPresentationSet`, not the S2 local-only overlay.
- Report: [`docs/PHASE_8E_REPORT.md`](PHASE_8E_REPORT.md).

## Phase 8F-A notes

- Character Presentation owns draw order: `ArmBack → LegBack → Core → LegFront → Head → ArmFront`.
- Attachments emit after their semantic bone layer. Content has no unrestricted z.
- Report: [`docs/PHASE_8F_A_REPORT.md`](PHASE_8F_A_REPORT.md).

## Phase 8F-B notes

- `PresentationView::Side` shows every authored layer; paint slot equals authored layer.
- `PresentationView::Back` hides authored `ArmFront` / `LegFront` (and their attachments). Remaining `ArmBack` / `LegBack` occupy the near Front paint slots.
- Attachments inherit bone-layer visibility. `hidden_base` still omits only matching base pieces.
- Same `plan_character_draw` path for local and remote. Overlay "Force Back view" is draw-time proof only.
- Report: [`docs/PHASE_8F_B_REPORT.md`](PHASE_8F_B_REPORT.md).

## Phase 8F-C notes

- `view_for_activity`: Idle/Move/Jump/Fall/Attack/Hurt → `Side`; `ClimbBack` → `Back`.
- ClimbBack is presentation activity, not inferred from velocity and not a wire oneshot.
- Overlay "Force ClimbBack activity" injects that activity on the shared local/remote path. "Force Back view" remains draw-only and does not change activity.
- Report: [`docs/PHASE_8F_C_REPORT.md`](PHASE_8F_C_REPORT.md). ClimbBack clip wiring is 8F-D.

## Phase 8F-D notes

- `ClimbBack` samples authored `content/shared/animations/dev/climb_back.anim` through the existing A6 path (`include_str!` → `parse_animation_asset_v1` → `climb_back_clip()` → `clip_for_playback_activity`).
- Other activities keep their existing clips. Local and remote share the same activity → clip → `LocalPose` path.
- Missing file fails compile (`include_str!`). Invalid parse uses bind/no-animation (`bind_rotation_noop_clip`), not silent Idle.
- `ClimbBack` → `PresentationView::Back`, 8F-A order, 8F-B visibility, attachments, and `hidden_base` are unchanged.
- Report: [`docs/PHASE_8F_D_REPORT.md`](PHASE_8F_D_REPORT.md). Equipment Side/Back visual keys are 8F-E.

## Phase 8F-E notes

- `PresentationView` selects `visuals.side` or `visuals.back` in Character Presentation. Not `ClimbBack` and not gameplay state.
- Resolve still caches both keys on equipment change. Plan/draw select per view. Anchors, `hidden_base`, bone layer, and compose transform are unchanged by the key switch.
- Missing Back: omit that attachment from the Back plan (8B: no Side-as-Back). Side-only pack items stay valid.
- Debug color hashes the selected key so Side vs Back is visible.
- Report: [`docs/PHASE_8F_E_REPORT.md`](PHASE_8F_E_REPORT.md). Closeout: [`docs/PHASE_8F_F_REPORT.md`](PHASE_8F_F_REPORT.md).

## Phase 8F-F notes

- Audit-only closeout. No sprites/ART, gameplay climb, equipment UI, protocol state, or new presentation abstractions.
- Canonical path: `activity → view_for_activity → clip_for_playback_activity → LocalPose → plan_character_draw → visual_key_for_view`.
- One `PresentationLayer` table; no numeric z; `ClimbBack → Back → climb_back.anim`; local/remote share `CharacterPresentationSet`; missing Back omits the attachment.
- Removed leftover identity `playback_activity()` remap. Per-entry `playback_activity` remains for transitions.
- Report: [`docs/PHASE_8F_F_REPORT.md`](PHASE_8F_F_REPORT.md). **8F CLOSE.**

## Phase 8 closeout notes

- ClimbBack clip duration is authored (`climb_back.anim`); no duplicated Rust constant.
- Final audit of the 8A–8F presentation path, render policy, and DEV-only controls: [`docs/PHASE_8_CLOSEOUT_REPORT.md`](PHASE_8_CLOSEOUT_REPORT.md).

## Phase 9A notes

- Ability lifecycle owner is the existing Action Runtime (Windup / Active / Recovery on `ActionTable`).
- `AbilityDefinition` is the minimum content-driven shape. Instant `Damage` goes through `AbilityEffect`, not ability-specific Health writes.
- Health remains optional; Alive/Dead is derived. Cooldown is a World table.
- Protocol **v14** unchanged. Client will request ability id + optional selected entity, never damage.
- Report: [`docs/PHASE_9A_REPORT.md`](PHASE_9A_REPORT.md). **9A CLOSE.**

## Phase 9B notes

- First authored ability `skill.basic.strike` (`AbilityDefinition` JSON). Activation is Independent; Active uses a forward AABB query.
- Activation succeeds with zero nearby entities. Damage is `AbilityEffect` → `execute_ability_effect` → `apply_damage`.
- 7.2 `Strike` remains workload-only. Protocol **v14** unchanged. No client command.
- Report: [`docs/PHASE_9B_REPORT.md`](PHASE_9B_REPORT.md). **9B CLOSE.**

## Phase 9C notes

- Client `J` sends `AbilityActivate` intent only (ability id; Independent ⇒ no selected entity).
- Server authorizes (ownership, pack, grant, activation shape) then delegates to `World::request_ability`.
- Live players receive Health + `skill.basic.strike` grant. 7.2 Strike isolation: `nearest_health_target` skips players.
- Protocol **v15**. Report: [`docs/PHASE_9C_REPORT.md`](PHASE_9C_REPORT.md). **9C CLOSE.**

## Phase 9D notes

- Ability execution → Attack oneshot (empty swings included). Authoritative non-lethal damage → Hurt. `Health <= 0` → persistent Dead.
- Dead overrides Attack/Hurt in Character Presentation. Protocol **v15** unchanged (reuses v13 oneshot envelopes).
- Report: [`docs/PHASE_9D_REPORT.md`](PHASE_9D_REPORT.md). **9D CLOSE.** Do not begin 9E.

## Character animation track (A0–A7.1)

- Presentation-only. Not a gameplay phase. Root `PHASE` is independent of this track (do not change it here). **8E is complete; do not modify/reopen/extend it.**
- Docs: [`CHARACTER_ANIMATION_ARCHITECTURE.md`](CHARACTER_ANIMATION_ARCHITECTURE.md), [`CHARACTER_ANIMATION_ROADMAP.md`](CHARACTER_ANIMATION_ROADMAP.md). ADR-0058.
- **A0–A6 complete.** **A7.0 complete.** **A7.1 complete:** copy/paste, multi-select, snapping, mirror preview, transition preview, `depth_angle` foreshortening on the shared animation runtime. Runtime still `include_str!`s `.anim` files; in-game observation needs a client rebuild.
- Sequence: A7.2 Curves → Notify Runtime. Do **not** start A7.2 until instructed. Do **not** mark global A7 complete.

## Diagnostics track (D0+)

- Client debug/diagnostics split. Not a gameplay phase. Root `PHASE` is independent of this track (do not change it here). Not the Animation A-track.
- Docs: [`DIAGNOSTICS_ARCHITECTURE.md`](DIAGNOSTICS_ARCHITECTURE.md), [`DIAGNOSTICS_ROADMAP.md`](DIAGNOSTICS_ROADMAP.md).
- **D4 complete** (`dev-diagnostics` default; shipping `--no-default-features`, ADR-0060). Do **not** start D5 until instructed.
- Sequence: D1 domain structs + command split → D2 compact Debug UI → D3 diagnostics page → D4 compile-time strip → D5 local DEV IPC → D6 external observer. No external diagnostics app in D0–D3.

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
