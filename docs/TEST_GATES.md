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

- Status: **GREEN (automated).** Manual Mixed/soak/Developer Tools/process-ownership evidence is **still required**.
- Command/test: `./scripts/check.ps1` (includes `purgatory-content-validator`)
- Date: 2026-08-31
- Notes: Protocol stays v10. Metrics schema stays 3. `PURGATORY_LOAD_VALIDATION` is load-mode-only. Isolated persist / failure injection never uses `%LOCALAPPDATA%\Purgatory`. MixedRuntime is the canonical workload: persistent real QUIC baseline + independent churn + in-zone portal transition + AOI/replication over the soak (not timer `PortalActivate`, not “duration after all bots disconnected”). Soak duration is configurable. Gate re-run after the harness tick-path spawn / replica-lag portal walk fix (see [`docs/PHASE_6G_REPORT.md`](PHASE_6G_REPORT.md)). Reports: [`docs/PHASE_6G_REPORT.md`](PHASE_6G_REPORT.md), [`docs/PHASE_6_EXIT_REVIEW.md`](PHASE_6_EXIT_REVIEW.md), [`docs/PHASE_6G_QUEUE_INVENTORY.md`](PHASE_6G_QUEUE_INVENTORY.md). **Do not begin Phase 7 until 6G is GREEN.** Planned Phase 7 gates: [`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md). Local-player standing jitter / falling remainder-Y clip were corrected on the presentation path (see 6G report); the ~128-client load stall remains a separate unresolved empirical issue and is **not** marked fixed.

## Developer Tools — connection probe (tooling, not a gameplay phase)

- Status: recorded with the Developer Tools foundation (ADR-0050). Not a substitute for Gate 6F.
- Command/test: `cargo test -p purgatory-bot-client`; `purgatory-load --probe` is exercised by those tests (parse + fail-when-nothing-listens). Full Ready path is a manual/integration check via `DEV.BAT`.
- Notes: Protocol stays v10. Probe login `dev.probe` uses the normal persist/enter path (documented debt). Docs: [`docs/dev-tools/`](dev-tools/README.md).

## Developer Tools — Hub Slice 1 / 1.1 (tooling, not a gameplay phase)

- Status: recorded. Not a 6G gameplay gate and not Phase 7.
- Command/test: `cargo test -p purgatory-dev-runtime` (22 passed, 1 ignored real-process lifetime test run separately and passed). `cargo test -p purgatory-dev-hub` (2 passed). `./scripts/check.ps1` GREEN on 2026-08-31 (including `leave_then_reenter_sends_baseline_again`; that test had been a pre-existing dirty-tree failure in an earlier Hub Slice 1 gate and was **not** changed in this work).
- Notes: ADR-0052. Headless orchestration + provisional eframe application shell. `DEV_HUB.BAT` builds then launches `purgatory-dev-hub.exe` independently. Dedicated server is detached from Hub lifetime; reopen adopts + `--probe` before Ready. Dashboard / Runtime → Server / Logs are live. PowerShell `DEV.BAT` remains the operational fallback. Inventory: [`docs/dev-tools/PARITY.md`](dev-tools/PARITY.md).

## Developer Tools — Hub Slice 2 Runtime Validation (tooling, not a gameplay phase)

- Status: recorded. Not a 6G gameplay gate and not Phase 7. Slice 3 (load dialog) is not started.
- Command/test: `cargo test -p purgatory-dev-runtime` (37 passed; 1 ignored real-process lifetime test unchanged). `cargo test -p purgatory-dev-hub` (2 passed). `./scripts/check.ps1` GREEN on 2026-08-31 (including `leave_then_reenter_sends_baseline_again`; replication was not changed).
- Notes: Official RV rebuilds `purgatory-load`, captures `--print-server-env`, restarts a detached load-mode server, waits for existing `--probe` Ready, then runs a session-owned harness. CLI exit is pass/fail. Workspace lock `logs/dev-tools/hub.lock` refuses a second Hub. Cancel kills the harness, not the server. Manual smoke (`--preset smoke`, 20s) and a second-Hub lock check are still required. Inventory: [`docs/dev-tools/PARITY.md`](dev-tools/PARITY.md).

## Later gates

Legacy Gate 7 (client reconciliation / input replay) already shipped as Phase 5.5. Legacy Gates 8–17 follow superseded numbering and are **not** the post-6G sequence.

Gameplay Phase 7 is **planned, not started.** Sub-stage gates (7A–7E) are defined in [`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md) and will be recorded here when each stage starts. **Do not begin Phase 7 until 6G is GREEN** (automated gate is recorded; manual Mixed/soak/process-ownership and open AOI evidence still required). Standing-jitter / falling remainder-Y were corrected on the local presentation path; quiet-play visual sign-off is still required. The ~128-client load stall remains unresolved. A 30-minute soak is **not** required for every Phase 7 sub-stage.

Phase 5 is GREEN; Phase 6.0 / 6A / 6B / 6C / 6D / 6E / 6F / 6G (automated) are GREEN. 6G is not marked complete.
