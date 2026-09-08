# Test gates

Record every gate from the master execution plan.

Status: `pending`, `pass`, `fail`, `skipped`.

Owner Phase 0 clarifications:

- Project root is this directory.
- `Graphic/` is left in place. Phase 5.0B later loads **only** `Graphic/LOGO.png` for the Connection Frontend.
- Git init was skipped in Phase 0. The workspace is now a Git repository at this project root.

## Gate 0A — Environment detection

- Status: pass
- Command/test: `rustc --version`, `cargo --version`, `rustup --version`, `rustup component list --installed`
- Date: 2026-08-26
- Notes: Rust was not on PATH. Official `rustup-init.exe` installed `stable-x86_64-pc-windows-msvc`. Detected `rustc 1.98.0 (88d9e12ae 2026-08-18)`, `cargo 1.98.0`, `rustup 1.29.0`, with `rustfmt` and `clippy` installed. Existing `C:\Users\Ariel\.rustup\settings.toml` caused rustup to keep the default host triple `x86_64-pc-windows-msvc`. Visual Studio 2022 Community with MSVC tools was already present.

## Gate 0B — Initialize repository

- Status: skipped (Git deferred)
- Command/test: `.gitignore` created; `git init` not run
- Date: 2026-08-26
- Notes: Owner deferred Git at Phase 0. `.gitignore` ignores `/target/`, OS/editor temp files, local logs, profiling output, and local secrets/certificates. `Cargo.lock` is not ignored. Git was enabled later at this project root; no interaction with `C:/Users/Ariel` or any parent Git repository.

## Gate 0C — Cargo workspace and Phase-0 crates

- Status: pass
- Command/test: `cargo check --workspace`, `cargo build --workspace`, `cargo test --workspace`; `cargo run -p purgatory-client|server|content-validator|bot-client`
- Date: 2026-08-26
- Notes: Virtual workspace at project root, resolver 3, edition 2024. All members compile. Workspace tests passed (12 unit tests). Bootstrap output: `PURGATORY client bootstrap OK`, `PURGATORY server bootstrap OK`, `PURGATORY content validator bootstrap OK`, `PURGATORY bot client bootstrap OK`. `Cargo.lock` generated and present. Server has no window/GPU dependencies.

## Gate 0D — Quality gate scripts

- Status: pass
- Command/test: `./scripts/check.ps1`
- Date: 2026-08-26
- Notes: Script ran `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace`. Result: `PURGATORY quality gate OK`. `scripts/check.sh` is present for Linux/macOS and was not executed on this Windows host.

## Gate 0E — Core documentation

- Status: pass
- Command/test: docs under `/docs` plus root `README.md`
- Date: 2026-08-26
- Notes: Architecture, ADRs 0001–0010, protocol boundary, content pipeline, performance budgets, roadmap, and this gate log are in place. A new developer/agent can see what is being built, what is not, authority boundaries, crate ownership, and how to verify the repository.

## Gate 1 — Custom runtime and clock

- Status: pass
- Command/test: `cargo test -p purgatory-simulation --lib`; `cargo test -p purgatory-server`; `cargo run -p purgatory-server`; `./scripts/check.ps1`
- Date: 2026-08-26
- Notes: `purgatory-simulation` owns `SimulationClock` with a 30 Hz integer nanosecond step (`33_333_333` ns). Catch-up clamp is 1 s of supplied elapsed time (30 ticks from rest, 31 max with remainder). Tests cover 1 s → 30 ticks, 10×100 ms vs 100×10 ms, remainder retention, spike discard, monotonic ticks, `simulation_time = ticks × step`, zero elapsed, and Cargo.toml exclusion of winit/wgpu/tokio/quinn. Server drives the clock with a canned 1 s `Duration` and exits; no window/GPU/network/async dependencies were added to simulation. Interpolation is not implemented; remainder is exposed as `Duration` only.

## Gate 2 — Native window + placeholder renderer

- Status: pass
- Command/test: `cargo test --workspace`; `cargo run -p purgatory-client`; resize/minimize/restore/close; `cargo run -p purgatory-server`; `./scripts/check.ps1`
- Date: 2026-08-26
- Notes: Client opens a 1280×720 native window titled `PURGATORY — Engine Dev`. Custom wgpu path draws a dark clear, a cyan rectangle, and a yellow debug marker. Adapter on this machine: NVIDIA GeForce RTX 4050 Laptop GPU, backend Vulkan, format `Bgra8UnormSrgb`. Logical viewport is 16×9 world units at that size (height fixed at 9, width from aspect). Title showed independent counters (example: tick 2822 / frames 13358). Resize to ~900×700 updated projection and kept both primitives visible. Minimize/restore did not crash. Close printed `PURGATORY client closing` and exited 0. Server still headless. `winit`/`wgpu` are client-only. wgpu 30 surface recovery uses `CurrentSurfaceTexture` rather than `SurfaceError::OutOfMemory`. Renderer module is `gpu.rs` instead of `renderer.rs` to satisfy Clippy `module_inception`.

## Gate 3 — Input + local placeholder character

- Status: pass
- Command/test: `cargo test -p purgatory-simulation --lib`; `cargo test --workspace`; `cargo run -p purgatory-client`; `cargo run -p purgatory-server`; `./scripts/check.ps1`
- Date: 2026-08-26
- Notes: Semantic `Action` / `PlayerInput` in the client and simulation. Movement, gravity, jump, and AABB collision live in `purgatory-simulation`. Collision detection is separate from intent and solid response. Platforms are `Platform` values (id, kind, `top_surface`, `blocks_approach`); Phase 3 only implements solid blocking. FOOTNOTE is documented as the future owner of advanced platform interaction and is not implemented. Speeds are units/second × fixed `dt`. Tests cover idle, left/right, 1 s distance, 30 Hz vs 40 Hz dt, outer-frame 100 ms vs 10 ms patterns, gravity, floor, grounded jump, airborne reject, held-jump while airborne, walk-off, landing, raised platform, velocity retention, overlap detection without mutation, and no `use winit`/`wgpu` in movement sources. Manual client run: 1280×720 window, cyan player on floor, tan elevated platform, yellow debug marker, static camera, independent tick/frame counters, close printed `PURGATORY client closing` and exited 0. Server still headless with the Phase-1 clock sample only (`ticks=30`). `Graphic/` is unused (no textures/sprites/logo).

## Gate 4 — World and entity foundation

- Status: pass
- Command/test: `cargo test -p purgatory-simulation --lib`; `cargo test --workspace`; `cargo run -p purgatory-client`; `cargo run -p purgatory-server`; `./scripts/check.ps1`
- Date: 2026-08-26
- Notes: Generational `EntityId` (index + generation) in a `World` slot vector. Stale IDs rejected; reused slots get a new generation. Player and platforms are world entities. `grounded_on` is `Option<EntityId>`; despawned supports are cleared next tick and the player falls. Movement/collision behavior unchanged. FOOTNOTE not implemented. 1,000-entity Debug lifecycle: spawn ≈ 86 µs, iter ≈ 27 µs, despawn ≈ 58 µs (Windows MSVC, not a scale claim). 10,000-entity storage correctness test also passes. Server instantiates `World::dev_stage()` headlessly. No ECS crate. No `winit`/`wgpu` in simulation.

## Gate 4.1 — Development debug overlay

- Status: pass
- Command/test: `cargo test -p purgatory-simulation --lib`; `cargo test --workspace`; `cargo run -p purgatory-client`; `cargo run -p purgatory-server`; `./scripts/check.ps1`
- Date: 2026-08-26
- Notes: Client-only `egui` 0.36.1 / `egui-winit` 0.36.1 / `egui-wgpu` 0.36.1 over the existing winit/wgpu window. Physical Backquote toggles `PURGATORY DEBUG` (hidden at start; key-repeat ignored). Overlay reads a `DebugSnapshot`; mutations go through `DebugAction::ResetPlayer`. Opening the overlay does not block movement/jump; only a focused text-like egui widget steals key presses. Mouse hits on the overlay stay with egui. FOOTNOTE section is a placeholder only. Simulation and server Cargo.toml exclude egui. No pixel-level UI tests. `Graphic/` unused. Do not start Phase 4.5 or networking.

## Gate 4.5 — FOOTNOTE foundation

- Status: pass
- Command/test: `cargo test -p purgatory-simulation --lib`; `cargo test --workspace`; `cargo run -p purgatory-client`; `cargo run -p purgatory-server`; `./scripts/check.ps1`
- Date: 2026-08-26
- Notes: `footnote/` module with `FootnoteConfig`, accel/decel, air control, momentum preservation, `ContactEvent`, `PlatformKind::OneWay`, drop-through via `ignored_platform`, previous-bottom top-surface landing. Client `down_held` (S/Down); Down+Jump drops OneWay only. Dev stage: solid floor + raised solid + two OneWays. Debug FOOTNOTE panel + overlay-gated world gizmos. No egui/winit/wgpu in simulation/server. No networking. Do not start Phase 5.

## Gate 4.6 — FOOTNOTE expanded development test map

- Status: pass
- Command/test: `cargo test -p purgatory-simulation --lib`; `cargo test --workspace`; `cargo run -p purgatory-client`; `cargo run -p purgatory-server`; `./scripts/check.ps1`
- Date: 2026-08-27
- Notes: `World::footnote_test_stage()` hard-coded arena (thinned ~26 platforms + player): main flow P0–P4, descent D1–D4 + LP, multi-drop stack, freestyle (~5), momentum (3), slope approximation (3 AABB steps), isolated **OV overlap regression** pair. Vertical resolve selects one nearest crossed surface (no sequential overlap teleport). Drop-through ignore clears below platform top. Compact `dev_stage()` retained for unit tests. No map loader, no rotated colliders, no networking. Do not start Phase 5.

## Gate 4.7 — Collision robustness & edge-case hardening

- Status: pass
- Command/test: `cargo test -p purgatory-simulation --lib`; `cargo test --workspace`; `./scripts/check.ps1`
- Date: 2026-08-27
- Notes: Crossing-only normal collision; removed stale nearest-face horizontal depen (Entity-16 fix). Separate capped Solid recovery. `CONTACT_EPSILON=1e-3`. Regression matrix (head-bonk, seam, corners, order, 30/40 Hz, seeded). Diagnostics tab: detector/log/verbose default OFF; 16-event history. No networking.

## Gate 4.8 — Camera, world bounds & development harness

- Status: pass
- Command/test: `cargo test --workspace`; `cargo run -p purgatory-client`; `cargo run -p purgatory-server`; `./scripts/check.ps1`
- Date: 2026-08-27
- Notes: `WorldBounds` (−24…24 × −8…10); player side clamps + fall-below respawn; client follow camera with bound clamp; 3-layer parallax; debug tabs + gizmo toggles + local time scale 1.0/0.5/0.25; map width ≈ 2× Phase 4.6 without doubling platforms (~26). Simulation/server remain free of egui/wgpu camera deps.

## Gate 5 — Network connection foundation

- Status: pass
- Command/test: `cargo test -p purgatory-protocol --lib`; `cargo test -p purgatory-server`; `cargo test --workspace`; `cargo run -p purgatory-server`; `cargo run -p purgatory-client`; `./scripts/check.ps1`
- Date: 2026-08-27
- Notes: Quinn/QUIC on Tokio. Listen `127.0.0.1:5001`. `PROTOCOL_VERSION = 1`. Hello/Welcome, server `ConnectionId`, 5 s handshake timeout, 4096-byte control frames, datagram nonce ping (client-local Instant RTT). Wire disconnect codes vs local client failures (later unified as `NetworkFailureKind` in 5.0D). DEV-ONLY self-signed cert + skip-verify. Client Network debug tab. No gameplay replication. Two simultaneous sessions get distinct ids. Simulation tick independent of packets.

## Gate 5.0B — Connection frontend + client lifecycle

- Status: pass
- Command/test: `cargo test -p purgatory-client`; `cargo test --workspace`; `cargo run -p purgatory-server`; `cargo run -p purgatory-client`; `./scripts/check.ps1`
- Date: 2026-08-27
- Notes: Client starts on Connection Frontend, no auto-connect. Explicit CONNECT. Client-local `ConnectionAttemptId`. Stale events ignored. Welcome only from active Handshaking enters Game. Immediate client-side Disconnect invalidation (delayed Welcome ignored). Input gated on frontend; SimulationClock not advanced while waiting. `Graphic/LOGO.png` loaded once. Bounded event queue: 4-slot RTT reserve was a mitigation, closed by Gate 5.0C. No protocol expansion, no gameplay replication.

## Gate 5.0C — Connection lifecycle & race hardening

- Status: pass
- Command/test: `cargo test -p purgatory-client`; `cargo test -p purgatory-server`; `cargo test --workspace`; `./scripts/check.ps1`
- Date: 2026-08-27
- Notes: Central stale-attempt filter. Duplicate Connect rejected in lifecycle and runtime (no parallel QUIC). Split lifecycle/telemetry queues; Disconnect/Shutdown via watch epoch. SessionLease exactly-once cleanup. Accept loop spawns per connection (cap 32). 2/5/10 simultaneous clients, malformed/stalled isolation, 20-cycle churn + 5×8 multi-client churn. Ping outstanding bound 4 (drop oldest). No gameplay replication. Phase 5.0D follows.

## Gate 5.0D — Network diagnostics & failure semantics

- Status: pass
- Command/test: `cargo test -p purgatory-protocol --lib`; `cargo test -p purgatory-client`; `cargo test -p purgatory-server`; `cargo test --workspace`; `./scripts/check.ps1`
- Date: 2026-08-27
- Notes: `NetworkFailureKind` maps wire codes and transport symptoms; retryability and frontend strings are single mappings. Manual disconnect / local shutdown are benign. Bounded 48-event history; RTT EWMA resets between sessions; history does not record RTT. Explicit `IDLE_TIMEOUT` = 15 s vs `HANDSHAKE_TIMEOUT` = 5 s. Server `ServerNetStats` atomics; Ctrl+C sends `ServerShutdown`. No gameplay replication. Phase 5.0E follows.

## Gate 5.0E — Security & abuse foundation

- Status: pass
- Command/test: `cargo test -p purgatory-protocol --lib`; `cargo test -p purgatory-server`; `cargo test --workspace`; `./scripts/check.ps1`
- Date: 2026-08-27
- Notes: Central `NetworkAbuseConfig`. Length prefix validated before alloc. Random decoder corpus. Admission cap refuse + release. Malformed churn then healthy connect. Control rate limit and datagram budget isolate offenders. Stream cap 1 bidi / 0 uni. No gameplay replication. Phase 5.0F follows.

## Gate 5.0F — Soak / stress / chaos hardening

- Status: pass
- Command/test: `cargo test -p purgatory-client`; `cargo test -p purgatory-server`; `cargo test --workspace`; `./scripts/check.ps1`; then `./scripts/network_soak.ps1` (extended `#[ignore]` soaks)
- Date: 2026-08-27
- Notes: Recovery-to-baseline is the acceptance criterion. Active gauges (sessions, handshakes, inflight tasks, admission permits) are polled back to baseline by `wait_until_baseline`; cumulative counters and high-water marks (`max_sessions`, `max_inflight`, `max_handshakes`) are reported separately. CI: 50 sequential cycles, 30 rapid reconnects, 4×5 and 8 concurrent clients, admission cap 4 fill/refuse/release ×3, stalled + mixed pressure, 40 malformed handshakes, 16 version mismatches, deterministic chaos over seeds `0x1 / 0xC0FFEE / 0xDEADBEEF` (40 ops each), repeated idle/handshake timeouts, abrupt drops, graceful and abrupt server loss, server restart, telemetry saturation vs lifecycle delivery, command-queue pressure, 500-cycle diagnostic history bound. Extended: 1000 sequential cycles (≈31 s), 10×100 multi-client (≈6 s), 8-seed × 120-op chaos matrix (≈3.5 s), 200 malformed, 20 admission rounds, 4 restart rounds, sustained ping cadence (≈21 s); total ≈68 s. Every chaos family ends with a healthy probe. Localhost stress is not player capacity. No gameplay replication in 5.0F.

## Gate 5.0 final follow-up — Wire stability, live handshake shutdown, bind failure

- Status: pass
- Command/test: `cargo test -p purgatory-protocol --test wire_golden`; `cargo test -p purgatory-client`; `cargo test -p purgatory-server`; `cargo test --workspace`; `./scripts/check.ps1`
- Date: 2026-08-27
- Notes: Protocol v1 golden vectors in `crates/protocol/tests/wire_golden.rs` freeze exact bytes for `Hello` (non-empty + empty string), `Welcome`, `DisconnectReason`, Ping and Pong datagrams, plus the complete framed `Hello` and `Welcome`. Each vector asserts both directions (message → bytes and bytes → message) and pins little-endian layout, `u8`-length + UTF-8 strings, field order, and reason-code discriminants. Byte arrays live in source; there is no fixture file and no auto-update path, so a wire change fails loudly. **No Protocol v1 byte changed.** Live QUIC shutdown during `Handshaking` is regression-tested on both sides: the client test stands up a test-only listener that completes the real transport handshake, reads the Hello, and never answers Welcome (6 CI rounds, 25 in the extended soak), asserting the real `Handshaking` event and a bounded network-thread join; the server tests interrupt 8 pre-Welcome handshakes and 8 post-Hello handshakes, plus a full admission cap of interrupted peers, asserting no session, no permit leak, convergence to baseline, and a mandatory healthy probe afterwards. Bind failure is covered with a reserved ephemeral localhost port (never the fixed dev port): `endpoint::bind` returns `failed to bind <addr>: <io error>`, `run_blocking` returns that error instead of panicking or retrying, the reserved socket keeps the port, and the address binds cleanly once released. Startup order and the `network listening on <addr>` log are asserted to follow a successful bind. No production sleeps or handshake-timing changes were added.

## Gate 5.1 — Authoritative input

- Status: pass
- Command/test: `cargo test -p purgatory-protocol --lib`; `cargo test -p purgatory-protocol --test wire_golden`; `cargo test -p purgatory-server`; `cargo test -p purgatory-client`; `cargo test --workspace`; `./scripts/check.ps1`
- Date: 2026-08-27
- Notes: Protocol v2 `InputCommand` (sequence, move_axis, jump_pressed, down_held). v1 golden vectors unchanged. Old protocol Hello rejected. Server `GameplayOwner` binds `ConnectionId → EntityId`, bounded handoff, held vs jump-edge, separate input rate policy. Packet bursts do not tick `World`. Disconnect/loss despawn the player. Client send-on-change; local FOOTNOTE marked LOCAL DEV / NON-AUTHORITATIVE. No snapshots/prediction.

## Gate 5.2 — Authoritative world snapshots

- Status: pass
- Command/test: `cargo test -p purgatory-protocol --lib`; `cargo test -p purgatory-protocol --test wire_golden`; `cargo test -p purgatory-server`; `cargo test -p purgatory-client`; `cargo test --workspace`; `./scripts/check.ps1`
- Date: 2026-08-27
- Notes: Protocol v3 `WorldSnapshot` (sequence, server_tick, local_player_entity, bounded entities). v1/v2 golden vectors unchanged. Full snapshots; entity count and payload size bounded before allocation; NaN/Inf rejected. SnapshotBuilder + per-connection latest-wins watch. Client `ReplicatedWorld` atomic apply, stale/duplicate ignore. Renderer uses replica positions. Static platforms not replicated. Two clients see both players. Disconnect despawn; reconnect new generation. Slow snapshot consumer does not stall simulation. Closed by Phase 5.3.

## Gate 5.3 — Remote entity interpolation

- Status: pass
- Command/test: `cargo test -p purgatory-client interp::`; `cargo test -p purgatory-client`; `cargo test --workspace`; `./scripts/check.ps1`
- Date: 2026-08-28
- Notes: Client-only `InterpolationBuffer` (cap 16, delay 3 ticks). Monotonic estimated/render clock. Local was replica in 5.3; remotes lerp. Spawn/despawn on render timeline. No cross-generation lerp. Underrun hold; teleport snap presentation-only. No protocol bump. Server unchanged. Closed by Phase 5.4 for local presentation path.

## Gate 5.4 — Local player prediction

- Status: pass
- Command/test: `cargo test -p purgatory-client prediction::`; `cargo test -p purgatory-client`; `cargo test --workspace`; `./scripts/check.ps1`
- Date: 2026-08-28
- Notes: Client-only `LocalPrediction` + shared FOOTNOTE `World::tick`. Local render/camera from predicted pose; remotes unchanged (interp). `ReplicatedWorld` never overwritten. **Lead vs aligned:** re-anchor uses the aligned residual (best history match on position + velocity, within 16 ticks of plausible lag), never pred-now vs lagged snapshot; lead ≥ 8 wu is last-resort safety; settled vertical snap uses aligned `|dy|` at 2 wu (unchanged). Sustained residual ≥ 0.05 wu over 4 snapshots = drift with no temporal explanation → offset correction preserving lead. Covered by `temporal_lag_within_window_is_not_corrected`, `persistent_at_rest_drift_is_corrected_by_offset`, `stale_match_outside_window_does_not_excuse_divergence`. **Input tick alignment:** intent is sent from the same fixed-tick `PlayerInput` sample as prediction (keydown only latches `ActionState`); fixes ±1 tick press/jump/release phase offset that produced same-tick path splits (platform walk-off / failed jumps). Remaining same-tick residual after that is pure RTT/unacked input — Phase 5.5. Protocol v3 unchanged. Catch-up via `SimulationClock` bounds.

## Gate 5.5 — Authoritative input acknowledgement and local reconciliation

- Status: pass (automated); **STOP for two-client manual verification** before Phase 5.6
- Command/test: `cargo fmt --all`; `cargo check --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`
- Date: 2026-08-28
- Notes: Protocol v4. Per-tick commands, continuation debt (`unmatched_continuation_ticks`), late-collapse compaction, HeldCancel idempotent cancel-ack with immutable `(epoch, target_sequence)` barrier, restore+replay via `tick_player`. Covered: split HOL remainder debt, `K=0` no collapse, repeated HeldCancel, late-jump after grounded change (legitimate correction), hitch one-command, two-recipient headers, epoch-at-MAX no wrap. Manual: two clients, movement, hitch, focus-loss Neutral vs HeldCancel.

## Gate 5.6 — Controlled network impairment lab

- Status: pass (automated); **STOP for two-client manual matrix** before Phase 5.7
- Command/test: `cargo fmt --all`; `cargo check --workspace --all-targets`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`
- Date: 2026-08-28
- Notes: Dev-only deterministic FIFO delay/stall lanes. Off by default. No protocol bump. Input delay after pending / before QUIC write. Snapshot delay is post-uni-read application delivery. Watch=config, mpsc=stall. Off flush is bounded FIFO. Independent PRNG streams. Correction = pre→post restore+replay. Ack delta is not late-collapse. Interpolation 3-tick buffer unchanged; underrun is measured, not an automatic fail. Manual matrix: [`docs/PHASE_56_LATENCY_MATRIX.md`](PHASE_56_LATENCY_MATRIX.md). Closed by Phase 5.7.

## Gate 5.7 — Multiplayer load, soak, churn & scaling validation

- Status: **GREEN**
- Command/test: `cargo fmt --all`; `cargo check --workspace --all-targets`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`; Phase 5.7 steady-state input-handoff isolation (before/after matrix)
- Date: 2026-08-29
- Notes: Real QUIC bots (`purgatory-load`). `MAX_ENTITIES_PER_SNAPSHOT` 256 (mechanical). Default admission 32; load mode `PURGATORY_ADMISSION_CAP=256`. UDP localhost metrics `LoadMetricsV1`. Guide: [`docs/PHASE_57_LOAD_TESTING.md`](PHASE_57_LOAD_TESTING.md).
- **Authoritative final gate:** steady-state input-handoff isolation, Mixed / seed 4242 / quiet then 180 s active, 100 / 150 / 200 / 256 connected. After the correctness-preserving handoff fix (`send().await` + inter-tick drain), `input_handoff_dropped` = 0 at every population; all four after-runs COMPLETE. Report: `logs/load/capacity/20260829_002013/steady_input_final_report.md` (mirrored at `logs/load/capacity/20260828_235654/steady_input_final_report.md`).
- Verdict: **PHASE 5 GREEN — Phase 6 may begin.**
- Non-blocking Phase 6 follow-ups (not Phase 5 failures): admission/entity wall remains 256; full-visibility snapshot fan-out is O(N²)-like on the network; snapshot/sim-thread pose-copy pressure remains; WorldAddress / visibility / relevance / dirty tracking / replication budgeting are Phase 6 contracts.

## Gate 6.0 — Runtime foundation and replication contracts

- Status: **GREEN**
- Command/test: `./scripts/check.ps1`
- Date: 2026-08-29
- Notes: `WorldAddress`, identity separation, lifecycle, query, `World::relevance_for(RuntimeEntityId)`, replication metadata (no scheduler). Protocol v4 unchanged. ADR-0034. Report: [`docs/PHASE_60_REPORT.md`](PHASE_60_REPORT.md). Phase 6A may begin.

## Gate 6A — Runtime model

- Status: **GREEN**
- Command/test: `./scripts/check.ps1`
- Date: 2026-08-29
- Notes: Optional capabilities on the existing slot-vector `World` (not an ECS). `Transform` is optional. Per-domain dirty flags. `RuntimeSpawnRequest`. Protocol v4 unchanged. No scheduler. Report: [`docs/PHASE_6A_REPORT.md`](PHASE_6A_REPORT.md). Phase 6B may begin.

## Gate 6B — Interaction + UI runtime

- Status: **GREEN**
- Command/test: `./scripts/check.ps1`
- Date: 2026-08-29
- Notes: Server-authoritative interaction. `InteractionSession` is domain state, not a UI window. Protocol **v5**. Client nearest-target is advisory. Replica interactables are the only client draw source (no local-World visual substitute). Automated gate GREEN; **manual E/overlay interaction check is still required**. Report: [`docs/PHASE_6B_REPORT.md`](PHASE_6B_REPORT.md). Stop before 6C.

## Gate 6C — World + content runtime

- Status: **GREEN (automated).** Manual Map A ↔ Map B portal check is **still required**.
- Command/test: `./scripts/check.ps1` (includes `purgatory-content-validator`)
- Date: 2026-08-29
- Notes: Authored-string `ContentId`. Shared vs server-only content. Registry-assigned `MapId`. Protocol **v7** observer address, `ReplicatedKind::Portal`, and `PortalActivate`. Content-driven linked portal travel. E excludes portals; Up Arrow uses the activation zone. Client fails clearly on missing MapId. Report: [`docs/PHASE_6C_REPORT.md`](PHASE_6C_REPORT.md).

## Gate 6D — Runtime query, AOI, and replication relevance

- Status: **GREEN (automated).** Manual runtime check is **still required**. Final status: `CAMERA DEAD-ZONE SMOOTH FOLLOW READY FOR USER CHECK` / `TRANSITION BLACKOUT COMMIT FIX READY FOR USER CHECK` / `LOCAL PLAYER CAMERA JITTER READY FOR USER CHECK` / `TRANSITION INPUT BARRIER READY FOR USER CHECK`.
- Command/test: `./scripts/check.ps1` (includes `purgatory-content-validator`)
- Date: 2026-08-30
- Notes: World-owned spatial grid. Server interest-policy AOI. Protocol **v9** (`ReplicationFrame` tag 16 plus DEV `DevSetChannel` tag 17). Epoch purge after queue-commit. `write_all` failure tears down the session. Load metrics schema **2**. Channel transition: same MapId, live `RuntimeEntityId`, spatial relocate, observer epoch reset. Presentation is **readiness-gated** (ADR-0041): map `DestinationReady`, membership `MembershipReady`; FadeIn is not timer-revealed; visible presentation commits only at fully black. **Transition gameplay input barrier** (ADR-0042): server-neutralized held input + tick-gated idle apply; client lock until FadeIn. Client camera is Dead Zone + exponential follow (presentation only; not AOI). Local player draw and camera follow share one finalized presentation pose; small reconcile corrections are absorbed into a decaying visual offset. **WorldAddress boundary ≠ social identity boundary** (`AddressChanged` closes world-bound `InteractionSession` only; server emits Closed on Channel change). Overlay: categorized inspector; Observer AOI Map/Channel/Instance; DEV Channel `[0] [1]`; transition banners; `INPUT:` gate chip; camera Dead Zone gizmo. Automated gate GREEN; **manual two-client Channel + portal fade + held-input barrier + camera-feel check is still required**. Report: [`docs/PHASE_6D_REPORT.md`](PHASE_6D_REPORT.md).

## Gate 6E — Character + persistence

- Status: **GREEN (automated).** Manual first-connect / reconnect / restart / duplicate-login / sentinel replica check is **still required**.
- Command/test: `./scripts/check.ps1` (includes `purgatory-content-validator`)
- Date: 2026-08-30
- Notes: Protocol **v10** `Hello.dev_login`. Server-minted `CharacterId` (not `PersistentId`). File-backed identity + character store. RestoreIntent ≠ WorldAddress. Welcome after bind. Report: [`docs/PHASE_6E_REPORT.md`](PHASE_6E_REPORT.md).

## Gate 6F — Runtime gameplay readiness

- Status: **GREEN (automated).** Manual two-client dirty/AOI, overlay Enter/Update/Leave, and opt-in `PURGATORY_RUNTIME_PROBE` check is **still required**.
- Command/test: `./scripts/check.ps1` (includes `purgatory-content-validator`)
- Date: 2026-08-30
- Notes: No protocol bump (v10). Simulation-time scheduler (Critical ceiling 1024, Deferred budget 32, capacity 4096). Lean Action storage (`Active` + terminal). `InputGateReason` absorbed as `ActionDenialReason::TransitionLocked`. Staged events, not a bus. Dirty/delta = `DomainRevs` per observer. Load metrics schema **3**. Probe is opt-in. Report: [`docs/PHASE_6F_REPORT.md`](PHASE_6F_REPORT.md). Stop before the next phase.

## Gate 6G — Runtime hardening and Phase 6 exit

- Status: **GREEN (automated).** Architecture closed in 6G.7C. Official Mixed soak recorded in Gate 6G.2. Residual 6B–6F two-client presentation manuals remain evidence debt, not 6G reopeners.
- Command/test: `./scripts/check.ps1` (includes `purgatory-content-validator`)
- Date: 2026-08-31
- Notes: Protocol stays v10. Runtime metrics contract stays schema **3**. `PURGATORY_LOAD_VALIDATION` is load-mode-only. Isolated persist / failure injection never uses `%LOCALAPPDATA%\Purgatory`. MixedRuntime is the canonical workload: persistent real QUIC baseline + independent churn + in-zone portal transition + AOI/replication over the soak (not timer `PortalActivate`, not “duration after all bots disconnected”). Soak duration is configurable. Gate re-run after the harness tick-path spawn / replica-lag portal walk fix (see [`docs/PHASE_6G_REPORT.md`](PHASE_6G_REPORT.md)). Reports: [`docs/PHASE_6G_REPORT.md`](PHASE_6G_REPORT.md), [`docs/PHASE_6_EXIT_REVIEW.md`](PHASE_6_EXIT_REVIEW.md), [`docs/PHASE_6G_QUEUE_INVENTORY.md`](PHASE_6G_QUEUE_INVENTORY.md). Closing chain: [`docs/PHASE_6G7C_REPORT.md`](PHASE_6G7C_REPORT.md). Local-player standing jitter / falling remainder-Y clip were corrected on the presentation path (see 6G report). The previously observed ~128-client load stall is **not** an established server bottleneck (6G.2 did not reproduce a server-domain hang; 6G.7B/C showed cheap replication at 128). Phase 7 is planned separately ([`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md)).

## Gate 6G.2 — Capacity characterization (Pass 1)

- Status: **Pass 1 characterization recorded 2026-09-01 — stop for owner review (no 6G.3 redesign)**
- Command/test: `./scripts/check.ps1` (PASS); Release ladder via `scripts/capacity_ladder.ps1`; official soak `--preset soak --duration 30m`
- Date: 2026-09-01
- Notes: Root `PHASE` = `6G.2`. Adopted live `PURGSTAT` contract is **schema 3** (schema 4 execution totals were inventoried/deferred; **domain timings stay in run artifacts**, ADR-0053). Freeze + ladder + soak: [`docs/MMO_RUNTIME_BASELINE.md`](MMO_RUNTIME_BASELINE.md). Report: [`docs/PHASE_6G2_REPORT.md`](PHASE_6G2_REPORT.md). Artifacts: `logs/load/capacity_6g2/`. **AOI dominant in this Pass 1 snapshot** (idle@256 tick p99 ≈ 16 ms, AOI ≈ 13 ms ≈ 79%) — not the post-6G.7C bottleneck claim. High-N ramp disconnects observed. 128-load hang **not reproduced**. Official soak **COMPLETE** 1800s (`logs/load/20260901_050045_8bots_soak_seed4242_r10925bfd`; capacity `.../20260901_080043_soak_official`); 1 overrun; summary tick p99 14.4 ms / max 101.9 ms; handoff_dropped=0. Windows Stop-then-replace evidence recorded. **No architectural optimization in this gate.**

## Gate 6G.3 — Targeted AOI fan-out optimization

- Status: **recorded 2026-09-01** (idle high-N win; motion AOI remaining limit)
- Command/test: `./scripts/check.ps1` (PASS); Release idle/hotspot ladder via `scripts/capacity_ladder.ps1`
- Date: 2026-09-01
- Notes: Root `PHASE` = `6G.3`. Skip classify while `World::interest_generation` unchanged; one unsorted `spatial_candidates` when classifying. Idle@256: tick p99 16.0→7.95 ms, AOI p99 12.7→5.80 ms. Hotspot/motion: no reliable AOI win (generation churn). Report: [`docs/PHASE_6G3_REPORT.md`](PHASE_6G3_REPORT.md). No pending-cap raise, no replication redesign, no Phase 7.

## Gate 6G.4 — Motion / capacity gate (measure only)

- Status: **recorded 2026-09-01 — classification A: GREEN WITH KNOWN LIMIT**
- Command/test: Release `scripts/capacity_ladder.ps1` distributed + hotspot @ 64/128/256 (seed 4242, ramp 50)
- Date: 2026-09-01
- Notes: Root `PHASE` = `6G.4`. No AOI redesign. 128 motion COMPLETE with tick p99 ≈ 13–18 ms domain (hotspot summary peak p99 ≈ 27 ms). 256 = characterized limit (distributed ramp disconnects; hotspot AOI/tick saturation p99 ≈ 50 ms). Report: [`docs/PHASE_6G4_REPORT.md`](PHASE_6G4_REPORT.md). Artifacts: `logs/load/capacity_6g4/20260901_140001/`. Do not begin Phase 7 from this gate alone.

## Gate 6G.5 — Incremental AOI invalidation

- Status: **recorded 2026-09-01 — implemented + measured — stop for owner review (do not close all of 6G)**
- Command/test: `./scripts/check.ps1` components PASS; Release motion ladder distributed + hotspot @ 64/128/256; locality unit tests
- Date: 2026-09-01
- Notes: Root `PHASE` = `6G.5`. Per-observer dirty + spatial influence (`AOI_INFLUENCE_HALF_EXTENTS`); global `interest_generation` removed. Locality tests prove unrelated observers are not reclassified. vs 6G.4: hotspot@256 AOI/tick improved materially; distributed FOOTNOTE still dense under leave influence. Report: [`docs/PHASE_6G5_REPORT.md`](PHASE_6G5_REPORT.md). Design: [`docs/PHASE_6G5_DESIGN.md`](PHASE_6G5_DESIGN.md). Artifacts: `logs/load/capacity_6g5/20260901_161031/`. No pending-cap raise, no replication redesign, no Phase 7.

## Gate 6G.6 — AOI locality + relevance replication characterization

- Status: **recorded 2026-09-01 — characterization only — recommendation D — stop for owner review (do not close all of 6G)**
- Command/test: `phase6g6` locality tests; capacity ladder with `aoi_locality.json`; replication/code audit
- Date: 2026-09-01
- Notes: Root `PHASE` = `6G.6`. Tiny same-cell move: influence dirties observers while exact leave-XOR set is empty. Replication: DomainRevs change-driven payloads exist, but no entity→interested fan-out or relationship domain policy. Report: [`docs/PHASE_6G6_REPORT.md`](PHASE_6G6_REPORT.md). Artifacts: `logs/load/capacity_6g6/`. No Phase 7, no cap raise, no large redesign.

## Gate 6G.7A — Exact incremental AOI invalidation

- Status: **recorded 2026-09-01 — implemented + validated — accepted; closed for 6G.7A scope (do not close all of 6G)**
- Command/test: `phase6g7a` proofs; Release motion ladder @64/128/256; replication tests
- Date: 2026-09-01
- Notes: Root `PHASE` was `6G.7A`. Enter/leave XOR after influence prefilter; tiny-move ⇒ mover-only dirty. Ladder dirtied/move ≈1.5–1.8 vs ~35–45 influence-set (6G.6). Report: [`docs/PHASE_6G7A_REPORT.md`](PHASE_6G7A_REPORT.md). Design: [`docs/PHASE_6G7A_DESIGN.md`](PHASE_6G7A_DESIGN.md). Artifacts: `logs/load/capacity_6g7a/`. Next: 6G.7B replication relevance (authorized). No Phase 7, no cap raise.

## Gate 6G.7B — Dirty-driven replication fan-out foundation

- Status: **recorded 2026-09-01 — accepted/closed for fan-out scope (do not close all of 6G)**
- Command/test: replication fan-out unit tests; Release capacity ladder idle/distributed/hotspot @64/128/256; `replication_fanout.json`
- Date: 2026-09-01
- Notes: Root `PHASE` was `6G.7B`. Idle Known scan=0; motion scanned/update ≈1.3. Report: [`docs/PHASE_6G7B_REPORT.md`](PHASE_6G7B_REPORT.md).

## Gate 6G.7C — Relationship / density / budget policy foundation

- Status: **recorded 2026-09-01 — accepted; closes 6G architecture (GREEN)**
- Command/test: replication + policy unit tests; Release hotspot baseline vs selective @64/128/256; `replication_fanout.json` schema 2
- Date: 2026-09-01
- Notes: Root `PHASE` = `6G`. Same protocol; selective ≈3× fewer Updates / ≈½ bytes on hotspot (primarily stranger transform cadence coalesce). Owner checklist (correctness / savings attribution / pressure) in report. **6G = GREEN — architecture closed, production policy tuning deferred.** Report: [`docs/PHASE_6G7C_REPORT.md`](PHASE_6G7C_REPORT.md). Design: [`docs/PHASE_6G7C_DESIGN.md`](PHASE_6G7C_DESIGN.md). Artifacts: `logs/load/capacity_6g7c/`. Phase 7 not started. No cap raise.

## Gate 7.1 — Capacity instrumentation and work ownership

- Status: **closed 2026-09-01 — complete.** 7.1F harness issuance deferred to **7.3**.
- Command/test: `./scripts/check.ps1` **PASS** 2026-09-01 (`PURGATORY quality gate OK`); Release `scripts/capacity_71_validate.ps1` + longer 384 admission-wall diagnostic via `scripts/capacity_ladder.ps1`; 7.1F `scripts/capacity_71f_ramp.ps1`
- Date: 2026-09-01
- Notes: Root `PHASE` advanced to `7.2` after close. File-schema tick owners + remainder + `unknown_unattributed` default; live UDP `PURGSTAT` not bumped. `dominant_owner` is mean share over 120 samples. CPU raw vs normalized-per-logical. `write_drain` is write/backpressure latency, not QUIC CPU. 384-count run is an **admission-wall diagnostic** (admission stays 256) and **did not fill the wall**; class stayed `unknown_unattributed` (honest). **7.1F:** `connection_ramp.json` on hotspot@384/60s: 384 requested, harness issued **114** attempts (`controller_ticks=114`); attainment 29.7%; statement *“384 were requested, but the harness only created 114 connection attempts within the run window.”* Harness COMPLETE/exit 0 is not attainment. 128/256 30 s cells reached ~62 sessions, not N. Report: [`docs/PHASE_71_REPORT.md`](PHASE_71_REPORT.md). Artifacts: `logs/load/capacity_71/`. No cap raise. Do not fix harness issuance in 7.2.

## Gate 7.2 — Representative gameplay workload

- Status: **closed 2026-09-01 — complete.** Do not begin 7.3 until instructed.
- Command/test: `cargo fmt --check` + `cargo check` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo test --workspace` **PASS**; Release `scripts/capacity_72_validate.ps1` (light@4 / mixed@8 / dense@8)
- Date: 2026-09-01
- Notes: Root `PHASE` remains `7.2`. Protocol **v11** (`ReplicatedKind::Npc`). NPC runtime + Strike/Health/Pulse/lifecycle; `npc_activity` tick leaf; `gameplay_workload.json` file artifact; live UDP `PURGSTAT` schema 3 unchanged. Representative presets canonical for 7.3. Measure only — no ladder optimization, no 7.1F harness fix. Short smokes: mixed/dense dominant_owner `npc_activity`; deaths/respawns 0 (unit-tested). Report: [`docs/PHASE_72_REPORT.md`](PHASE_72_REPORT.md). Artifacts: `logs/load/capacity_72/`.

## Gate 7.3 — Capacity ladder & bottleneck isolation

- Status: **closed 2026-09-01 — complete.**
- Command/test: Release `scripts/capacity_73a_issuance_proof.ps1` (7.3A); `scripts/capacity_73_ladder.ps1` (lifecycle + player/NPC/activity/dense + soak). Merged summary `logs/load/capacity_73/summary_20260901_213932/phase73_summary_merged.json`.
- Notes: Root `PHASE` = `7.3`. Funnel schema 2; issuance 100% through mixed@128 / dense@64. Envelope ends **before server saturation** (tick util ≤~5.4%, overruns 0). Harness `snapshot_starvation` / portal FAILED recorded separately — not server capacity. Lifecycle proof deaths/respawns > 0. Report: [`docs/PHASE_73_REPORT.md`](PHASE_73_REPORT.md).

## Gate 7.4 — Network throughput & backpressure

- Status: **closed 2026-09-01 — complete.**
- Command/test: Release `scripts/capacity_74_ladder.ps1` (baseline/budget/cadence/slow/dense/candidate/soak). Summary under `logs/load/capacity_74/summary_*/phase74_summary.json`.
- Notes: Root `PHASE` advanced through 7.4. `network_pressure.json` schema 2 outbound path. Soft budget binds near 1024 on mixed@64; writer queue never filled (depth max 1). Localhost slow-drain did not produce transport saturation. Candidate policy documented (provisional). Report: [`docs/PHASE_74_REPORT.md`](PHASE_74_REPORT.md).

## Gate 7.5 — CPU / owner scaling & measured optimization

- Status: **closed 2026-09-01 — complete.**
- Command/test: Release `scripts/capacity_75_ladder.ps1` (audit/player/npc/activity/dense/overlap/canonical). `./scripts/check.ps1` **PASS**. Summaries under `logs/load/capacity_75/summary_*/phase75_summary.json`.
- Notes: Root `PHASE` advanced through 7.5. Instant accounting close (enqueue commit + outer glue + begin_tick). Unattributed ~23–25% residual (timer noise). Dominant growth owner `replication_policy`. Peak util ≲11% through mixed@128 / dense@64. **No CPU owner optimization accepted** (headroom). Parallelism verdict: **No**. Report: [`docs/PHASE_75_REPORT.md`](PHASE_75_REPORT.md).

## Gate 7.6 — Single-server capacity model

- Status: **closed 2026-09-02 — complete.**
- Command/test: `scripts/capacity_76_model.ps1` (fit/validate/project); Release `scripts/capacity_76_falsify.ps1` (4 challenge cells). Artifacts under `logs/load/capacity_76/`.
- Notes: Root `PHASE` advanced through 7.6. Policy ~ scanned_per_tick; npc ~ updates_per_tick; whole-tick holdout MAPE ~22%. Sensitivity: players > overlap > NPC. No production player cap. No redesign. Report: [`docs/PHASE_76_REPORT.md`](PHASE_76_REPORT.md).

## Gate 7.7 — Scaling architecture decision & readiness

- Status: **closed 2026-09-02 — complete.**
- Command/test: Docs/ADR only — `./scripts/check.ps1`. No capacity ladder required (decision phase; no scaling implementation).
- Notes: Root `PHASE` advanced through 7.7. **Remain single-process** (ADR-0056). Triggers, evolutionary path, ownership, replication-parallelism feasibility, channels/instances readiness, decision matrix, readiness backlog in report. No multithreading/sharding/handoff. Report: [`docs/PHASE_77_REPORT.md`](PHASE_77_REPORT.md).

## Gate 7.8 — Production performance gate

- Status: **closed 2026-09-02 — complete (YELLOW). Phase 7 complete.**
- Command/test: `./scripts/phase_78_gate.ps1` (unit budget tests + functional/standard/high/dense + 8m soak). Baseline: `logs/load/capacity_78/baseline/baseline.json`. Evidence: `logs/load/capacity_78/gate_20260902_082337/`. `./scripts/check.ps1` **PASS**.
- Notes: Root `PHASE` was `7.8` at gate close. Absolute operational thresholds PASS; WARN = harness snapshot_starvation on N≥32. Soak hotspot@32+NPC completed (~480s). Thresholds: [`scripts/phase_78_thresholds.json`](../scripts/phase_78_thresholds.json). Report: [`docs/PHASE_78_REPORT.md`](PHASE_78_REPORT.md). ADR-0056 unchanged. Do not begin Phase 8 until instructed.

## Gate 7.closeout — Stats UI + weak-client authority audit

- Status: **closed 2026-09-02 — complete.**
- Command/test: `./scripts/check.ps1`; `cargo test -p purgatory-server --lib network::gameplay`; `cargo test -p purgatory-simulation --lib phase6b`; `cargo test -p purgatory-dev-runtime --lib phase78`; Hub Testing → Phase 7 Stats.
- Notes: Root `PHASE` = `7.closeout`. Hub reads `phase78_gate_summary.json` (no metric recompute); HARNESS WARN ≠ SERVER WARN. Authority audit: [`docs/PHASE_7_WEAK_CLIENT_AUTHORITY_AUDIT.md`](PHASE_7_WEAK_CLIENT_AUTHORITY_AUDIT.md). No unresolved mutating OPEN_RISK on exposed paths.

## Gate 8A — Equipment data model

- Status: **closed 2026-09-02 — complete (GREEN).**
- Command/test: `./scripts/check.ps1` **PASS**. `cargo test -p purgatory-simulation --lib equipment` (9); `cargo test -p purgatory-simulation --lib phase8a_tests` (12).
- Notes: Root `PHASE` = `8A`. Optional `EquipmentState` on World entities. Six fixed slots of `Option<ContentId>`. Slot dirty `u8`. Full/delta encode is a measurement codec, not a wire message. Report: [`docs/PHASE_8A_REPORT.md`](PHASE_8A_REPORT.md).

## Gate 8B — Equipment content schema + validation

- Status: **closed 2026-09-02 — complete (GREEN).**
- Command/test: `./scripts/check.ps1` **PASS**. `cargo test -p purgatory-content --lib` (50). Validator: `equipment=8 defs=15`.
- Notes: Root `PHASE` = `8B`. Gameplay + presentation JSON under `content/shared/equipment/` and `content/shared/equipment_presentation/`. Schema/semantic validation in `purgatory-content`. Protocol **v11** unchanged. Report: [`docs/PHASE_8B_REPORT.md`](PHASE_8B_REPORT.md).

## Gate 8C — Authoritative equip / unequip flow

- Status: **closed 2026-09-02 — complete (GREEN).**
- Command/test: `./scripts/check.ps1` **PASS**. Focused: `two_clients_converge_on_authoritative_equipment`; `reconnect_reconstructs_remote_equipment_from_baseline` (12 server 8C tests). Validator: `equipment=8 defs=15`.
- Date: 2026-09-02
- Notes: Root `PHASE` = `8C`. Protocol **v12**. Gameplay-only authorize. Slot-delta replication + observer `CommittedRevs.equipment`. Report: [`docs/PHASE_8C_REPORT.md`](PHASE_8C_REPORT.md).

## Gate 8D — Character presentation bridge

- Status: **closed 2026-09-02 — complete (GREEN).**
- Command/test: `./scripts/check.ps1` **PASS**. Focused: `character_presentation` (15) + skeleton `humanoid_v0_labels` / `skeleton_manifest`. Validator: `equipment=8 defs=15`.
- Date: 2026-09-02
- Notes: Root `PHASE` = `8D`. Protocol **v12** unchanged. Client `CharacterPresentationState` + shared SkeletonInput path. Report: [`docs/PHASE_8D_REPORT.md`](PHASE_8D_REPORT.md).

## Gate 8E — Equipment attachment composition + debug placeholders

- Status: **closed 2026-09-02 — complete (GREEN).**
- Command/test: `./scripts/check.ps1` **PASS**. Focused: `character_presentation` 8E tests (resolve/compose/hide/cache) + skeleton `presentation_anchors` / placeholder hide. Validator: `equipment=8 defs=15`.
- Date: 2026-09-02
- Notes: Root `PHASE` = `8E`. Protocol **v12** unchanged. Bound attachments + CharacterPresentationSet debug draw. Report: [`docs/PHASE_8E_REPORT.md`](PHASE_8E_REPORT.md).

## Gate 8F-A — Character presentation draw-order contract

- Status: **closed 2026-09-03 — complete (GREEN).**
- Command/test: `./scripts/check.ps1`. Focused: `character_presentation` 8F-A draw-order tests.
- Date: 2026-09-03
- Notes: Root `PHASE` = `8F.A`. Protocol **v13** unchanged. Semantic layers; attachments follow bone layer. Report: [`docs/PHASE_8F_A_REPORT.md`](PHASE_8F_A_REPORT.md).

## Gate 8F-B — Front/Back visibility

- Status: **closed 2026-09-03 — complete (GREEN).**
- Command/test: workspace fmt/check/clippy/test **PASS** excluding in-progress `purgatory-animation-lab` (official `./scripts/check.ps1` blocked on that crate's missing `tests.rs` at gate time). Focused: `character_presentation` 8F-B visibility tests.
- Date: 2026-09-03
- Notes: Root `PHASE` = `8F.B`. Protocol **v13** unchanged. Same `PresentationLayer` table; attachments inherit bone-layer visibility. Report: [`docs/PHASE_8F_B_REPORT.md`](PHASE_8F_B_REPORT.md).

## Gate 8F-C — Activity → PresentationView

- Status: **closed 2026-09-03 — complete (GREEN).**
- Command/test: `./scripts/check.ps1`. Focused: `character_presentation` 8F-C activity→view tests.
- Date: 2026-09-03
- Notes: Root `PHASE` = `8F.C`. Protocol **v13** unchanged. `ClimbBack` → `PresentationView::Back`. Force Back remains draw-only. Report: [`docs/PHASE_8F_C_REPORT.md`](PHASE_8F_C_REPORT.md).

## Gate 8F-D — ClimbBack animation clip wiring

- Status: **closed 2026-09-04 — complete (GREEN).**
- Command/test: `./scripts/check.ps1`. Focused: `climb_back_clip` + `character_presentation` 8F-D clip-selection tests.
- Date: 2026-09-04
- Notes: Root `PHASE` = `8F.D`. Protocol **v13** unchanged. `ClimbBack` → `climb_back.anim` (not Idle). Missing file fails compile; invalid parse is bind/no-animation. Report: [`docs/PHASE_8F_D_REPORT.md`](PHASE_8F_D_REPORT.md).

## Gate 8F-E — Equipment Side/Back visual selection

- Status: **closed 2026-09-04 — complete (GREEN).**
- Command/test: `./scripts/check.ps1`. Focused: `character_presentation` 8F-E visual-key tests + content `ViewVisuals::key_for`.
- Date: 2026-09-04
- Notes: Root `PHASE` = `8F.E`. Protocol **v13** unchanged. `PresentationView` selects Side/Back visual keys. Missing Back is omitted, not Side. Report: [`docs/PHASE_8F_E_REPORT.md`](PHASE_8F_E_REPORT.md).

## Gate 8F-F — 8F closeout / audit

- Status: **closed 2026-09-04 — complete (GREEN).** **8F CLOSE.** Do not start Phase 9.
- Command/test: `./scripts/check.ps1`. Focused: `character_presentation` 8F-F closeout lock tests.
- Date: 2026-09-04
- Notes: Root `PHASE` = `8F`. Protocol **v14** unchanged by 8F. Canonical path audited; identity `playback_activity()` remap removed. Report: [`docs/PHASE_8F_F_REPORT.md`](PHASE_8F_F_REPORT.md).

## Gate 8.closeout — Phase 8 closeout

- Status: **closed 2026-09-04 — complete (GREEN).** **Phase 8 CLOSE.** Do not start Phase 9.
- Command/test: `./scripts/check.ps1` **PASS**. Focused: ClimbBack duration from authored clip; `cargo test -p purgatory-client` DEV **587** / shipping **523**; clippy `-D warnings` DEV and `--no-default-features`.
- Date: 2026-09-04
- Notes: Root `PHASE` = `8.closeout`. Protocol **v14** unchanged. ClimbBack duration derived from `climb_back.anim` (removed duplicated `CLIMB_BACK_CLIP_DURATION`). Report: [`docs/PHASE_8_CLOSEOUT_REPORT.md`](PHASE_8_CLOSEOUT_REPORT.md).

## Gate 9A — Ability foundation contracts

- Status: **closed 2026-09-05 — complete (GREEN).**
- Command/test: focused `cargo test -p purgatory-simulation --lib phase9a`; `cargo test -p purgatory-simulation --lib ability::`; `cargo clippy -p purgatory-simulation --all-targets -- -D warnings`.
- Date: 2026-09-05
- Notes: Root `PHASE` was `9A`. Protocol **v14** unchanged. Action Runtime owns ability lifecycle. `AbilityDefinition` + `AbilityEffect::Damage`. Report: [`docs/PHASE_9A_REPORT.md`](PHASE_9A_REPORT.md).

## Gate 9B — Basic Attack executable path

- Status: **closed 2026-09-05 — complete (GREEN).**
- Command/test: focused `cargo test -p purgatory-simulation --lib phase9a`; `cargo test -p purgatory-simulation --lib phase9b`; `cargo test -p purgatory-simulation --lib ability::`; `cargo test -p purgatory-content --lib pack_loads_basic_strike`; `cargo clippy -p purgatory-simulation -p purgatory-content --all-targets -- -D warnings`.
- Date: 2026-09-05
- Notes: Root `PHASE` was `9B`. Protocol **v14** unchanged. `skill.basic.strike` JSON + Independent activation + forward query at Active. No client/network. Report: [`docs/PHASE_9B_REPORT.md`](PHASE_9B_REPORT.md).

## Gate 9C — Ability command path

- Status: **closed 2026-09-05 — complete (GREEN).** Do not start 9D until instructed.
- Command/test: focused protocol/simulation/server/client tests plus `cargo clippy -p purgatory-protocol -p purgatory-simulation -p purgatory-server -p purgatory-client --all-targets -- -D warnings`. See [`docs/PHASE_9C_REPORT.md`](PHASE_9C_REPORT.md).
- Date: 2026-09-05
- Notes: Root `PHASE` was `9C`. Protocol **v15**. Client intent → grant/activation validation → `request_ability`. Live player Health + Basic Strike grant. 7.2 Strike still skips players. Report: [`docs/PHASE_9C_REPORT.md`](PHASE_9C_REPORT.md).

## Gate 9D — Ability → Character Presentation

- Status: **closed 2026-09-05 — complete (GREEN).** Do not start 9E until instructed.
- Command/test: focused `cargo test -p purgatory-simulation --lib phase9`; server `ability` tests; client Dead/oneshot tests; `cargo clippy -p purgatory-simulation -p purgatory-server -p purgatory-client --all-targets -- -D warnings`.
- Date: 2026-09-05
- Notes: Root `PHASE` = `9D`. Protocol **v15** unchanged. Attack on ability execute; Hurt on damage; Dead from Health. Report: [`docs/PHASE_9D_REPORT.md`](PHASE_9D_REPORT.md).

## Gate A7.0 — Animation Lab Core (presentation authoring; not a gameplay phase)

- Status: **closed 2026-09-03 — A7.0 complete (GREEN).** Not “A7 complete”. Do not start A7.1.
- Command/test: `./scripts/check.ps1` **PASS** (`PURGATORY quality gate OK`). Focused: `purgatory-animation` 47; `purgatory-animation-lab` 17; Hub launch test `launch_animation_lab_does_not_require_ready` + `kill_all_does_not_stop_animation_lab`.
- Date: 2026-09-03
- Notes: Root `PHASE` remains unchanged. Protocol **v13** unchanged. Standalone eframe lab (`tools/animation_lab`) launched from Hub Content + Dashboard. Visible bottom Timeline / Dope Sheet (A7.0 gap: the sheet was painted with `ui.interact` and did not allocate height, so egui collapsed the panel to the transport strip). New Clip / Open / Save / Save As through A6 parse+serialize. Command/history Undo/Redo is A7.0. Direct manipulation uses an edit transaction. Manual Jump/Fall still required (including client rebuild). Docs: [`CHARACTER_ANIMATION_ROADMAP.md`](CHARACTER_ANIMATION_ROADMAP.md), ADR-0058.

## Gate A7.1 — Animation Lab QoL + Depth/Foreshortening (presentation authoring; not a gameplay phase)

- Status: **closed 2026-09-04 — A7.1 complete (GREEN).** Not “A7 complete”. Do not start A7.2.
- Command/test: `./scripts/check.ps1` **PASS** (`PURGATORY quality gate OK`). Focused: `purgatory-animation` 57; `purgatory-animation-lab` 39.
- Date: 2026-09-04
- Notes: Root `PHASE` remains unchanged. Protocol **v13** unchanged. Copy/paste, multi-select, editor snapping, Facing Left preview, A→B transition preview, authored `depth_angle` (`depth` token) with shared `sample_and_project`. Existing A6 assets without `depth` remain valid. No hot reload. Docs: [`CHARACTER_ANIMATION_ARCHITECTURE.md`](CHARACTER_ANIMATION_ARCHITECTURE.md), [`CHARACTER_ANIMATION_ROADMAP.md`](CHARACTER_ANIMATION_ROADMAP.md).

## Gate D2 — Compact Debug UI (diagnostics track; not a gameplay phase)

- Status: **closed 2026-09-04 — D2 complete.** Do not start D3 until instructed.
- Command/test: `./scripts/check.ps1` **PASS** (`PURGATORY quality gate OK`). Focused: `cargo test -p purgatory-client` (548 passed, 1 ignored). Chrome policy tests in `debug::chrome`; overlay section/collapse tests in `debug::overlay`.
- Date: 2026-09-04
- Notes: Root `PHASE` unchanged. Overlay default tab is Debug (now-state). Exception-driven chrome; persistent warning chips survive overlay close. Duplicate controls assigned a canonical owner. `DebugSnapshot` removed in D3. Manual overlay visual check of idle chrome + warning chips is still required. Docs: [`DIAGNOSTICS_ARCHITECTURE.md`](DIAGNOSTICS_ARCHITECTURE.md), [`DIAGNOSTICS_ROADMAP.md`](DIAGNOSTICS_ROADMAP.md).

## Gate D2.1 — Debug UI regression repair (diagnostics track; not a gameplay phase)

- Status: **closed 2026-09-04 — D2.1 complete.** Do not start D3 until instructed.
- Command/test: `cargo clippy -p purgatory-client --all-targets --all-features -- -D warnings` **PASS**. `cargo test -p purgatory-client debug::` **123 passed**. `cargo test -p purgatory-client internal_size_table` **2 passed**. Full `./scripts/check.ps1` was not green due to pre-existing workspace fmt (8F-E / Animation Lab) unrelated to D2.1.
- Date: 2026-09-04
- Notes: World-gizmo child checkboxes arm the master gate. Reset Player uses replica presence (reanchor) vs local spawn. Display summary shows render-scale → internal size. Manual overlay proof of gizmos / Reset / 150% internal resolution is still required.

## Gate D2.2 — Channel access + Reset semantics (diagnostics track; not a gameplay phase)

- Status: **closed — D2.2 complete.** Do not start D3 until instructed.
- Command/test: `cargo clippy -p purgatory-client --all-targets --all-features -- -D warnings` **PASS**. `cargo test -p purgatory-client debug::` **124 passed**. Full `./scripts/check.ps1` not required to be green here: pre-existing workspace fmt (8F-E / Animation Lab) is unrelated to D2.2.
- Date: 2026-09-04
- Notes: Compact chrome owns Channel `[0]`/`[1]` (`DevSetChannel`). Connected replica-local action is **Reanchor Prediction**. Offline/no replica local entity is **Reset to Spawn**. D1 demand gating unchanged. Connected spawn reset is protocol v14 `DevResetPlayer` (ADR-0059); see follow-up gate below.

## Gate D2.2 follow-up — server-authoritative Reset to Spawn + center toast

- Status: **closed 2026-09-04.** Do not start D4 until instructed. Root `PHASE` unchanged (8F.E).
- Command/test: `cargo test -p purgatory-protocol` **PASS**. `cargo test -p purgatory-server dev_reset_player` **PASS**. `cargo clippy -p purgatory-client -p purgatory-server -p purgatory-protocol --all-targets --all-features -- -D warnings` **PASS**. `cargo test -p purgatory-client debug::` **PASS**. `cargo test -p purgatory-client protocol_version_is_current` **PASS**. Full `./scripts/check.ps1` not required to be green here: pre-existing workspace fmt (8F-E / Animation Lab) is unrelated.
- Date: 2026-09-04
- Notes: Protocol **v14**. DEV `DevResetPlayer` (tag 24). Connected **Reset to Spawn Point** does not mutate the client World. Confirmation is a center-screen ASCII toast (~3s), not a Debug-window chip. Rebuild **both** client and server. Manual proof: connected Reset stays at spawn (no snap-back); toast has no tofu glyph. ADR-0059. Docs: [`PROTOCOL.md`](PROTOCOL.md), [`DIAGNOSTICS_ARCHITECTURE.md`](DIAGNOSTICS_ARCHITECTURE.md).

## Gate D3 — Remove DebugSnapshot adapter (diagnostics track; not a gameplay phase)

- Status: **closed — D3 complete.** Do not start D4 until instructed. Root `PHASE` unchanged (8F.E).
- Command/test: `cargo clippy -p purgatory-client --all-targets --all-features -- -D warnings` **PASS**. `cargo test -p purgatory-client debug::` **125 passed**. Full `./scripts/check.ps1` not required to be green here: pre-existing workspace fmt (8F-E / Animation Lab) is unrelated.
- Date: 2026-09-04
- Notes: Overlay reads `DiagnosticsFrame` domain fields. Flat `DebugSnapshot` / `from_frame` / `pred_error` adapter alias deleted. Domain types remain in `debug/snapshot.rs`. Consumer-demand gating unchanged. No chrome/command/gizmo/protocol change. Manual overlay smoke still required. Docs: [`DIAGNOSTICS_ARCHITECTURE.md`](DIAGNOSTICS_ARCHITECTURE.md), [`DIAGNOSTICS_ROADMAP.md`](DIAGNOSTICS_ROADMAP.md).

## Gate D4 — Compile-time `dev-diagnostics` strip (diagnostics track; not a gameplay phase)

- Status: **closed — D4 complete.** Do not start D5 until instructed. Root `PHASE` unchanged (8F.E).
- Command/test:
  - DEV (default features): `cargo clippy -p purgatory-client --all-targets --all-features -- -D warnings` **PASS**. `cargo test -p purgatory-client` **566 passed**, 1 ignored. `cargo test -p purgatory-client debug::` **125 passed**.
  - Shipping: `cargo clippy -p purgatory-client --no-default-features --all-targets -- -D warnings` **PASS**. `cargo test -p purgatory-client --no-default-features` **503 passed**, 1 ignored.
  - Full `./scripts/check.ps1` not required to be green here: pre-existing workspace fmt (8F-E / Animation Lab) is unrelated.
- Date: 2026-09-04
- Notes: Feature `dev-diagnostics` default on; egui optional. Shipping: no overlay/`DebugCommand`/Connection Frontend; auto-connect once; impairment Off; time scale 1.0; proof/force off. Server DEV-handler strip deferred. ADR-0060. Manual: rebuild default client and confirm overlay; optional shipping smoke with `--no-default-features`.

## Developer Tools — connection probe (tooling, not a gameplay phase)

- Status: recorded with the Developer Tools foundation (ADR-0050). Not a substitute for Gate 6F.
- Command/test: `cargo test -p purgatory-bot-client`; `purgatory-load --probe` is exercised by those tests (parse + fail-when-nothing-listens). Full Ready path is a manual/integration check via `DEV.BAT`.
- Notes: Protocol stays current (`PROTOCOL_VERSION`). Probe login `dev.probe` uses the normal persist/enter path (documented debt). Docs: [`docs/dev-tools/`](dev-tools/README.md).

## Developer Tools — Hub Slice 1 / 1.1 (tooling, not a gameplay phase)

- Status: recorded. Not a 6G gameplay gate and not Phase 7.
- Command/test: `cargo test -p purgatory-dev-runtime` (22 passed, 1 ignored real-process lifetime test run separately and passed). `cargo test -p purgatory-dev-hub` (2 passed). `./scripts/check.ps1` GREEN on 2026-08-31 (including `leave_then_reenter_sends_baseline_again`; that test had been a pre-existing dirty-tree failure in an earlier Hub Slice 1 gate and was **not** changed in this work).
- Notes: ADR-0052. Headless orchestration + provisional eframe application shell. `DEV_HUB.BAT` builds then launches `purgatory-dev-hub.exe` independently. Dedicated server is detached from Hub lifetime; reopen adopts + `--probe` before Ready. Dashboard / Runtime → Server / Logs are live. PowerShell `DEV.BAT` remains the operational fallback. Inventory: [`docs/dev-tools/PARITY.md`](dev-tools/PARITY.md).

## Developer Tools — Hub Slice 2 Runtime Validation (tooling, not a gameplay phase)

- Status: recorded. Not a 6G gameplay gate and not Phase 7.
- Command/test: `cargo test -p purgatory-dev-runtime` (37 passed; 1 ignored real-process lifetime test unchanged). `cargo test -p purgatory-dev-hub` (2 passed). `./scripts/check.ps1` GREEN on 2026-08-31 (including `leave_then_reenter_sends_baseline_again`; replication was not changed).
- Notes: Official RV rebuilds `purgatory-load`, captures `--print-server-env`, restarts a detached load-mode server, waits for existing `--probe` Ready, then runs a session-owned harness. CLI exit is pass/fail. Workspace lock `logs/dev-tools/hub.lock` refuses a second Hub. Cancel kills the harness, not the server. Manual smoke (`--preset smoke`, 20s) and a second-Hub lock check are still required. Inventory: [`docs/dev-tools/PARITY.md`](dev-tools/PARITY.md).

## Developer Tools — Hub launcher parity (Slice 3 + remaining CURRENT capabilities; tooling, not a gameplay phase)

- Status: recorded. Not a 6G gameplay gate and not Phase 7. Editors / `DEV.BAT` switch not started.
- Command/test: `cargo test -p purgatory-dev-runtime --lib` (53 passed). `cargo test -p purgatory-dev-hub` (2 passed). `./scripts/check.ps1` GREEN on 2026-08-31 (tooling-only; gameplay/replication not changed in this work).
- Notes: Bounded `server.log` / `client.log` file tails; LoadJob (compatible Ready vs ExtraEnv restart); clients queue/stagger/Detached/adopt; quality gate visible console; Rebuild skip-locked; Kill All (including workspace cargo by command line); Settings profile + log level for new processes. RV and load refuse each other. Manual acceptance still required (Server log lines, short load, +1 client adopt-after-close, QUALITY GATE console, Kill All, profile/log-level on next Start). Inventory: [`docs/dev-tools/PARITY.md`](dev-tools/PARITY.md).

## Developer Tools — Dashboard redesign (tooling presentation, not a gameplay phase)

- Status: recorded automated gate. Visual/manual resize acceptance still required.
- Command/test: `cargo test -p purgatory-dev-hub` (6 passed, including breakpoint + presentation helpers). `./scripts/check.ps1` GREEN on 2026-08-31.
- Notes: Replaced generic card auto-fit with semantic wide/medium/narrow Dashboard composition; StatusBadge / MetricRow / module frames; Dashboard view-models (`dashboard_model`); state-aware Server metrics/actions; Quick Actions strip; subdued header + Overview sidebar grouping. Runtime ownership unchanged. Docs: [`docs/dev-tools/README.md`](dev-tools/README.md), [`docs/dev-tools/PARITY.md`](dev-tools/PARITY.md).

## Developer Tools — Dashboard design-system visual pass (tooling presentation)

- Status: recorded automated gate. Manual resize Visual QA still required (wide / medium / narrow / min).
- Command/test: `cargo test -p purgatory-dev-hub` (6 passed). `./scripts/check.ps1` GREEN on 2026-09-01.
- Notes: Mockup-aligned palette/tokens; HubCard + btn_primary/ghost/destructive; sidebar active accent + real CONNECTED/job strip; Phase/profile chips; Project 2-col metrics; Attention healthy check; Recent Activity from real ActivityLog only; no System Status host gauges / Customize / profile block. Docs: [`docs/dev-tools/README.md`](dev-tools/README.md).

## Gate 11.closeout — Phase 11 Item Loop closeout

- Status: **closed 2026-09-07 — complete (GREEN).** **Phase 11 CLOSE.** Do not start Phase 12 from this closeout.
- Command/test: No automated workspace validation rerun; this was a documentation/marker closeout and no runtime code changed. Phase 11A–11E were already GREEN.
- Date: 2026-09-07
- Notes: Manual normal client/server proof passed: visible real world Item drop; `E` pickup; owned Inventory Item; DEV selection by real `ItemInstanceId`; equip and sword presentation; equipment-granted attack ability; unequip returning the same Item to Inventory; and removal of the equipment-derived ability. Production Inventory UI, Phase 12 persistence, advanced stacking, trading, currency, and economy remain deferred. Temporary/dev presentation or diagnostics rough edges do not reopen Phase 11. Report: [`docs/PHASE_11_CLOSEOUT_REPORT.md`](PHASE_11_CLOSEOUT_REPORT.md).

## Later gates

Legacy Gate 7 (client reconciliation / input replay) already shipped as Phase 5.5. Legacy Gates 8–17 follow superseded numbering and are **not** the post-6G sequence.

**Phase 6 architecture is closed** (Gate 6G.7C GREEN). Residual 6B–6F two-client presentation manuals are evidence debt, not 6G reopeners. Official Mixed soak was recorded in Gate 6G.2. Production replication-policy **tuning** is deferred to Phase 7.4; the replication **architecture** is not reopened.

Phase 7 (capacity, parallelism & production scaling) is **7.closeout complete — Phase 7 complete + closeout**. Sub-stage gates (7.1–7.8) are defined in [`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md). Canonical regression command: `./scripts/phase_78_gate.ps1`. Closeout: Hub Phase 7 Stats + [`docs/PHASE_7_WEAK_CLIENT_AUTHORITY_AUDIT.md`](PHASE_7_WEAK_CLIENT_AUTHORITY_AUDIT.md). Post-7 **Phase 8** is Character Presentation + Equipment Runtime. **8A** is the equipment data model. **8B** is the equipment content schema. **8C** is authoritative equip/unequip (protocol v12). **8D** is the character presentation bridge. **8E** is attachment composition + debug placeholders. **8F-A** is the Character Presentation draw-order contract. **8F-B** is Front/Back visibility. **8F-C** is activity → PresentationView. **8F-D** is ClimbBack clip wiring. **8F-E** is equipment Side/Back visual keys. **8F-F** is the 8F closeout/audit. **8F is complete + closed.** **Phase 8 closeout complete** ([`docs/PHASE_8_CLOSEOUT_REPORT.md`](PHASE_8_CLOSEOUT_REPORT.md)). **Phase 9A complete** ([`docs/PHASE_9A_REPORT.md`](PHASE_9A_REPORT.md)). **Phase 9B complete** ([`docs/PHASE_9B_REPORT.md`](PHASE_9B_REPORT.md)). Do not begin **9C**. Standing jitter is **closed** (presentation-path fix; quiet-play Δx not observed; owner visual confirmation 2026-09-01). Floor-clip / landing is **closed** (falling extra holds tick Y; owner confirmation 2026-08-31 landing no longer sinks; Y lerp is the jump-smooth follow-up already in tree). Duration overlay (`timeout >= duration`) is **closed** in CLI tests. The previously observed ~128-client load stall is **not** an unresolved server bottleneck; later 6G.7B/C evidence showed cheap replication at 128 and healthy server-side 256 under the validated workload. Higher-load timeouts remain unattributed unless 7.1 classification has exclusive evidence (`simulation_tick` / `server_transport_backpressure` / `harness_client`); otherwise the class is `unknown_unattributed`. 7.1F harness issuance (114/384) is addressed in **7.3A**. A 30-minute soak is **not** required for every Phase 7 sub-stage. Do not invent capacity targets here.

Phase 5 is GREEN; Phase 6.0 / 6A / 6B / 6C / 6D / 6E / 6F / 6G are GREEN (architecture closed).
