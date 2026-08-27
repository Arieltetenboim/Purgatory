# Test gates

Record every gate from the master execution plan.

Status: `pending`, `pass`, `fail`, `skipped`.

Owner Phase 0 clarifications:

- Project root is this directory.
- `Graphic/` is left untouched.
- Git steps are skipped (no `git init`, no commits, no parent-repo interaction).

## Gate 0A — Environment detection

- Status: pass
- Command/test: `rustc --version`, `cargo --version`, `rustup --version`, `rustup component list --installed`
- Date: 2026-08-26
- Notes: Rust was not on PATH. Official `rustup-init.exe` installed `stable-x86_64-pc-windows-msvc`. Detected `rustc 1.98.0 (88d9e12ae 2026-08-18)`, `cargo 1.98.0`, `rustup 1.29.0`, with `rustfmt` and `clippy` installed. Existing `C:\Users\Ariel\.rustup\settings.toml` caused rustup to keep the default host triple `x86_64-pc-windows-msvc`. Visual Studio 2022 Community with MSVC tools was already present.

## Gate 0B — Initialize repository

- Status: skipped (Git deferred)
- Command/test: `.gitignore` created; `git init` not run
- Date: 2026-08-26
- Notes: Owner deferred Git. `.gitignore` ignores `/target/`, OS/editor temp files, local logs, profiling output, and local secrets/certificates. `Cargo.lock` is not ignored. No interaction with `C:/Users/Ariel` or any parent Git repository.

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
- Notes: Quinn/QUIC on Tokio. Listen `127.0.0.1:5001`. `PROTOCOL_VERSION = 1`. Hello/Welcome, server `ConnectionId`, 5 s handshake timeout, 4096-byte control frames, datagram nonce ping (client-local Instant RTT). Wire disconnect codes vs `LocalConnectionError`. DEV-ONLY self-signed cert + skip-verify. Client Network debug tab. No gameplay replication. Two simultaneous sessions get distinct ids. Simulation tick independent of packets.

## Later gates

Gates 6–17 remain pending. Do not execute Phase 5.1 (authoritative multiplayer movement) during Phase 5.0.
