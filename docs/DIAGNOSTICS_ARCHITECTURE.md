# Diagnostics architecture

Status: **D4 complete** (`dev-diagnostics` Cargo feature; shipping `--no-default-features`). Do **not** start D5 until instructed. Root `PHASE` is independent of this track (do not change it here). This track is **not** the Animation A-track and must not reopen 8E / A7.1.

Companion: [`DIAGNOSTICS_ROADMAP.md`](DIAGNOSTICS_ROADMAP.md). Overlay structural rules remain in [`ARCHITECTURE.md`](ARCHITECTURE.md) (ADR-0016). Protocol trust boundary remains in [`PROTOCOL.md`](PROTOCOL.md).

## Purpose

Split today’s in-client debug overlay into:

1. a compact in-client **Debug UI** for everyday development (**what is happening now**);
2. a full **Diagnostics** surface that can later feed an external dev-only observer process (**why it happened**).

D1 is the client diagnostics boundary. D0 was documents only. No external diagnostics application. Do not add diagnostic fields to snapshots. DEV overlay spawn while connected uses `DevResetPlayer` (ADR-0059), not a client World teleport.

## Non-negotiable constraints

- Do not add diagnostic fields to snapshots, Hello/Welcome, or intent. DEV overlay **Reset to Spawn Point** while connected is `DevResetPlayer` (protocol v14, ADR-0059), not a client-side spawn.
- Diagnostics must not cause the server to send information the normal client would not otherwise know. Existing optional `ObserverAoiDebug` trailer stays as-is until a later shipping-profile task omits it (absence is already valid).
- Do not create a second/fake game client. Do not duplicate replication, prediction, interpolation, or simulation.
- External diagnostics must not own `World` and must not become authority.
- Read-only observation and mutating debug commands stay separate types and separate paths.
- External diagnostics transport and privileged commands are **development-only**.
- Shipping builds must remove privileged diagnostics and debug-command surfaces at **compile time**, not by hiding widgets.
- Production-safe operational metrics may remain where justified (client FPS, connection/RTT candidates). They are not an excuse to ship the overlay.
- Any future diagnostics transport must be bounded and non-blocking. A slow or missing consumer must not stall simulation, render, or network.
- Closing the overlay / not running the external process must have negligible runtime cost.
- ADR-0016 still applies: the game client does not grow a second OS window. An external diagnostics **process** may have its own window (same pattern as Animation Lab / Hub).

Developer Hub’s **Diagnostics** page (`docs/dev-tools/`) is process/runtime control (logs, metrics UDP, health). It is a different product surface. Do not merge it into this client-overlay split in D2.

## What exists today

D1 composes a read-only [`DiagnosticsFrame`](../apps/client/src/debug/frame.rs) from subsystem `diagnostics()` types. D3 deleted the flat `DebugSnapshot` adapter; the overlay reads domain fields on the frame directly. Assembly runs only when a **diagnostics consumer** is active (`DiagnosticsDemand`: overlay visible today; `ipc_subscribed` reserved for D5).

```text
ClientApp subsystems (World, replica, interp, prediction,
  camera, map_fade, ui_runtime, characters, lifecycle, network, skeleton)
        │
        │  only if DiagnosticsDemand::is_active()
        ▼
DiagnosticsFrame (runtime / physics / world / network / camera / presentation)
        │
        ├── DebugOverlay::submit_frame  (egui; skipped if overlay closed,
        │                                 no Connection Frontend, and no
        │                                 persistent dev-mode warnings)
        ├── draw_world_entity_labels    (egui Areas; overlay + toggle)
        └── footnote/aoi/interp/pred/camera gizmos  (GPU; overlay + master toggle)
                │
                └── viz.rs still reads World directly for FOOTNOTE gizmos
```

When no consumer is active: skip World/inspector walks, replica row strings, network-history copies, remote-motion probes, impairment polls, and jitter-ring samples. Connection Frontend with overlay closed still builds a display-only stub for egui pixels-per-point.

Command path:

```text
egui widgets
  → DebugUiState view toggles (persistent) and one-shot flags
  → DebugCommand (drain after overlay writeback; ResetPlayer / Connect also pushed directly)
  → ClientApp::apply_debug_commands
       ├─ World::apply_debug_action          (Reset to Spawn Point: offline only)
       ├─ network DevResetPlayer             (Reset to Spawn Point: connected)
       ├─ prediction.force_reanchor_from_replica  (Reanchor Prediction)
       ├─ DisplayController
       ├─ network Connect/Disconnect, impairment, DevSetChannel
       ├─ Equip/Unequip / DevPresentationOneShot
       └─ camera center / animation play-pause-reset / jitter dump
```

### Existing intermediate types worth reusing

Do not invent a parallel snapshot language. Promote these into domain structs:

| Type | Owner today | Notes |
|---|---|---|
| `PlayerDebug`, `FootnoteDebug`, `EntityRuntimeDebug` | `debug/snapshot.rs` | Copied from `World` in `WorldRosterDiagnostics::from_world` |
| `PlayerMotionDebug` | simulation | Last sim-tick motion/correction |
| `CameraMotionDebug` | `debug/camera_debug.rs` | Camera Δ / clamp / discontinuity |
| `DisplayDebug` | `display.rs` | Window / render-scale / world MSAA readout |
| `NetworkSnapshot` | `network/state.rs` | Lifecycle, RTT, counters, history ring |
| `ImpairmentMetricsSnapshot` | network impairment | Dev lab metrics |
| `InterpDiagnostics` | `interp.rs` | Already a subsystem `diagnostics()` |
| `PredictionDiagnostics` | `prediction.rs` | Already a subsystem `diagnostics()` |
| `JitterSummary` / `ForensicTrace` | `jitter_forensics.rs` | 180-frame ring + CSV dump |
| `InteractStatusView` | `debug/interact_status.rs` | Session headline + trail |
| `InspectorView` | `debug/entity_inspector.rs` | World vs replica rows |
| `ReplicaEntityDebug` | `debug/aoi_view.rs` | Known-set labels and AOI bands |
| `SkeletonInspectDebug` | `debug/snapshot.rs` | Selected bone Local/World/Screen |
| `RemoteMotionProbe` | `debug/snapshot.rs` | Temporary 8E remote-player forensic |

`DebugSnapshot` is **deleted** (D3). Overlay consumes `DiagnosticsFrame` domain fields. No replacement god-view-model.

### UI does not read subsystems directly

`overlay.rs` reads `DiagnosticsFrame` + `DebugUiState`. It does **not** walk `World` or `ReplicatedWorld`.

The assembler is `ClientApp` (render path). World gizmos in `viz.rs` **do** walk `World` live. That is the main remaining direct coupling on the draw path.

### Compile-time strip: not wired

`debug/mod.rs` documents a later `cfg(debug_assertions)` / cargo feature. The overlay is always constructed after window create. `apps/client/Cargo.toml` has no diagnostics feature. egui is an unconditional client dependency.

## Classification

Legend:

- **Compact** — belongs in everyday in-client Debug UI (now-state).
- **Diagnostics** — forensic / history / why; keep off the compact chrome.
- **Ops** — candidate production-safe operational telemetry (not the overlay).
- **Command** — developer-only mutating or privileged control.
- **View** — local presentation/gizmo command (client-owned; not simulation authority).
- **Dup** — duplicated control or readout.

### Permanent chrome (every tab)

| Item | Class | Notes |
|---|---|---|
| Time-scale banner | Compact + Command | Exception-driven is already correct (hidden at 1.0×) |
| INTERACTION / TARGET / PORTAL | Compact reserved rows | Always three one-line rows (NONE when idle). Live vs idle is color, not show/hide. Flash stays on the Interact line. Truncate + hover for overflow. |
| WORLD map/channel/instance | Compact | One line of now-state |
| Channel `[0]` `[1]` | Command | Compact chrome beside Map / Channel / Instance. Existing `DevSetChannel` path. |
| Transition banner / stall / waiting | Compact | Exception-driven already |
| Input-gate chip | Compact | Exception-driven (emphasize when locked) |
| Channel-transition flash | Compact | Exception-driven (3s) |

### Runtime

| Item | Class |
|---|---|
| Frame / Clock readouts (frames, tick, 30 Hz, window, FPS) | Compact; FPS/tick also **Ops** candidates |
| Simulation Speed 1.0/0.5/0.25 | Command |
| Display sizes / aspect / scale / gameplay rect | Compact (when changing res) else Diagnostics |
| Resolution / render-scale / world MSAA presets | Command (client display only; not protocol) |
| RF0 rotated-geometry scene + freeze camera + probe vertices | View (existing world pass; not a parallel debug framework) |
| RF1.5–RF3 A/B compositor (1× vs 4×, nearest vs linear 1:1, integer 100/200/400% + 4×) | View (dedicated diagnostic targets; does not inherit gameplay Render Scale; 400% is not a Display preset; 150% is not in the quality policy) |
| Show overlay gizmos (master) | View; D2.1: enabling a child world gizmo arms the master so Debug-tab checkboxes are not silent no-ops |
| Colliders, velocity, grounded, bounds, grid, parallax, interp gizmos, pred gizmos, AOI rects, dead zone, entity labels | View; Debug → View / gizmos |

### Player / FOOTNOTE

| Item | Class |
|---|---|
| Pose identity + position/velocity/grounded | Compact |
| Input X / down held | Compact |
| Reanchor Prediction / Reset to Spawn Point | Command | Debug tab always shows **Reset to Spawn Point**. Connected: `DevResetPlayer` (server spawn). Offline: local `DebugAction::ResetPlayer`. Replica present also shows **Reanchor Prediction**. Confirmation is a center-screen ASCII toast (~3s), not a Debug-window chip. |
| Motion last-tick Δ / discontinuity / correction | Diagnostics (last-tick Compact summary: grounded + disc flag) |
| FOOTNOTE Contact / Pose / Input | Dup resolved in D2: compact pose on Debug; contact on Player (collapsed) |

### Skeleton / presentation

| Item | Class |
|---|---|
| Placeholder / skeleton / AABB / 2× preview checkboxes | View |
| Force Back / Force ClimbBack | Command (presentation proof; A/8F, not D-track implementation) |
| Bone inspect Local/World/Screen | Diagnostics |
| Debug equipment equip/unequip | Command (server-authoritative DEV) |
| Front-leg / front-arm proof | Command (A-track proof poses) |
| A1/A2 animation proof | Command (A-track; do not move into D-track ownership) |
| A5 Attack/Hurt buttons | Command (`DevPresentationOneShot`) |

D-track must not take ownership of A-track proof controls. Compact Debug UI may keep a short Presentation row (placeholder/skeleton/AABB). Proof/equip/oneshot stay DEV commands, later behind the compile-time gate.

### World

| Item | Class |
|---|---|
| Stage name, map, channel, instance, bounds | Compact |
| Content registry listing | Diagnostics |
| Bounds/grid checkboxes | View; Debug → View / gizmos |
| Entities inspector tree | Diagnostics (Compact: counts only) |

### Camera

| Item | Class |
|---|---|
| Camera pos, viewport, following X/Y | Compact |
| Dead zone / smooth times / desired | Compact one-liners or Diagnostics |
| 180-frame jitter block | Diagnostics |
| Follow / Center On Player | View |
| Jitter Isolation modes + CSV dump | Command |
| Parallax factors | Diagnostics; debug markers are View (Debug gizmos) |

### Diagnostics tab

| Item | Class |
|---|---|
| Discontinuity detector / log / verbose collision | Command (enables recording) |
| Last tick summary | Compact one-liner possible |
| Recent events ring (16) | Diagnostics |
| Clear History | Command |

### Network

| Item | Class |
|---|---|
| Connection state, RTT latest | Compact; also **Ops** candidates |
| RTT min/max/EWMA, attempt id, ConnectionId | Diagnostics |
| Connect / Disconnect | Command (frontend already owns Connect) |
| Observer AOI mailbox counts | Diagnostics (`ObserverAoiDebug` trailer is already on the wire) |
| Channel buttons | Command; compact chrome beside Map / Channel / Instance |
| Known replica list / Recent Left | Diagnostics |
| Interaction forensic block | Diagnostics (collapsed); compact chrome is reserved Interact/Target/Portal rows |
| Gameplay input seq/semantic | Compact one-liner |
| Replica Enter/Update/Leave | Diagnostics (Compact: known count + last seq) |
| Remote interpolation + motion probe | Diagnostics |
| Local prediction / reconciliation | Diagnostics (Compact: active + lead error) |
| Impairment profile / stall / reset | Command |
| Failure category | Compact when failed; else Diagnostics |
| Counters / history / log checkboxes | Diagnostics + Command |

## Duplicated / redundant controls

D2 assigned one canonical UI owner. Underlying `DebugUiState` flags and `DebugCommand`s are unchanged. Lead error is `PredictionDiagnostics::lead_error` only (the flat `pred_error` adapter alias is gone with `DebugSnapshot`).

| Control / state | Canonical UI owner | Removed from |
|---|---|---|
| Channel `[0]`/`[1]` | Compact chrome (beside Map / Channel / Instance) | Network → Connection; Network → Observer AOI |
| Show World Bounds / Grid | Debug → View / gizmos | World → View (section removed) |
| Show Parallax Debug | Debug → View / gizmos | Camera → Parallax (readout only) |
| Show AOI Policy Rects / entity labels | Debug → View / gizmos | Network → Observer AOI |
| Compact local pose | Debug → Local player | FOOTNOTE Pose/Input sections (removed) |
| Identity + Reset (full) | Player → Pose | — |
| FOOTNOTE contact / drop-through | Player → FOOTNOTE contact (collapsed) | FOOTNOTE tab (removed) |
| Follow / Center On Player | Debug → Camera | Camera → Follow |
| Time scale buttons | Debug tab | Runtime |
| Display presets | Debug → Display (collapsed) | Runtime |
| Overlay gizmos master + core gizmos | Debug → View / gizmos | Runtime |
| Interactable/portal lists | Network → Authoritative Replica | Network → Interaction |
| Interaction forensic copy | Network → Interaction (collapsed) | Permanent chrome (reserved Interact/Target/Portal rows) |
| Interpolation bracket/alpha | Network → Remote Interpolation (once) | Duplicate print in the same section |
| Lead error readout | Debug compact + Network prediction (one line; `pred.lead_error`) | Dual labels / flat `pred_error` alias (removed in D3) |
| Connect | Connection Frontend when `client_screen == "Connection"`; Network → Connection otherwise | Overlay Connect on the Connection screen |
| Disconnect | Network → Connection | — |
| Skeleton Expand all | Includes `skeleton.equipment` | — |

World vs replica identity namespaces stay unmerged in World → Entities.

Ignored-platform highlight has no checkbox: it draws whenever overlay gizmos are on.

Remaining convenience overlap (not a second owner of a unique control): placeholder/skeleton/AABB checkboxes remain on Skeleton → View because that tab is A-track proof surface (do not redesign it here). **Reset to Spawn Point** is on Debug (everyday, top of the tab) and Player → Pose (identity). Replica present also shows **Reanchor Prediction**. Connected spawn is `DevResetPlayer`. Reanchor is `DebugCommand::ResetPlayer`. Confirmation is a center-screen toast only.

## Direct coupling remaining after D1

1. **`DebugSnapshot` god-struct** — **removed in D3**. Overlay reads `DiagnosticsFrame` domains.
2. **`viz.rs` live `World` walk** for gizmos — stays behind overlay + gizmo gate; compiled out with `dev-diagnostics` off (D4).
3. **`DebugUiState` as mixed bag** — compiled out with the overlay when `dev-diagnostics` is off (D4).
4. **Presentation proof flags on the hot draw path** — cfg to off without `dev-diagnostics` (D4).
5. **Jitter isolation modes** — compiled out without `dev-diagnostics` (D4).

Do **not** make overlay.rs call `World`. That direction is already correct.

## Proposed domain API / ownership

Names are not frozen. Prefer **composition**, not a second god-struct.

```text
DiagnosticsFrame {          // assembled only if a consumer exists
  runtime: RuntimeDiagnostics,
  physics: PhysicsDiagnostics,      // player + FOOTNOTE + last motion
  world: WorldDiagnostics,          // stage + inspector + AOI known rows
  network: NetworkDiagnostics,      // lifecycle + replica + interp + pred + interact
  camera: CameraDiagnostics,
  presentation: PresentationDiagnostics,  // skeleton inspect + character set counts
}
```

Ownership rule: **the subsystem that owns the live state owns `fn diagnostics(&self) -> …`**. `ClientApp` only concatenates. This already exists for interpolation and prediction.

| Domain | Emits from | Must not |
|---|---|---|
| Runtime | clock, renderer, `DisplayController` | Walk replica |
| Physics | client `World` player + `PlayerMotionDebug` | Invent authority |
| World | `World` entity iterator + replica Known-set mapping (`entity_inspector`) | Merge identity namespaces |
| Network | `lifecycle.snapshot()`, replica counters, `interp.diagnostics()`, `prediction.diagnostics()`, `InteractStatusView` | Open a second QUIC session |
| Camera | follow controller + `CameraMotionDebug` + optional jitter summary | Change follow unless a View/Command says so |
| Presentation | `CharacterPresentationSet` counts + selected-bone inspect | Own clip selection (A-track) |

`DiagnosticsFrame` is **read-only**. No command fields.

### When to assemble

```text
consumer = overlay_visible || ipc_subscriber_alive
if consumer { assemble DiagnosticsFrame } else { skip }
```

Gizmo GPU work stays behind `overlay_visible && show_overlay_gizmos` (already true for most gizmos). Skeleton/placeholder draw is independent today; Compact UI will still own those view flags.

## Read-only diagnostics vs debug commands

Three kinds, not two:

| Kind | Examples | External process |
|---|---|---|
| **Observation** (`DiagnosticsFrame`) | FPS, pose, RTT, replica counts, interp brackets | May receive a copy |
| **Local view** (`ViewCommand`) | gizmo toggles, follow camera, skeleton draw | Client-only in D1–D3; later may be requested, never executed by the observer |
| **Privileged command** (`DebugCommand`) | ResetPlayer, DevSetChannel, SpawnNpc, Equip, oneshot, impairment, time scale, jitter isolation, proof poses | Client executes. Observer must not apply them to a World |

`DebugAction` in simulation stays the World mutation enum (today: `ResetPlayer` only). Client `DebugCommand::ResetToSpawn` sends `DevResetPlayer` while in Game; offline it applies the local spawn path. `DebugCommand::ResetPlayer` reanchors prediction when a replica local entity is present. Temporary confirmations (Reset / Reanchor / Channel) are a center-screen ASCII toast for ~3 seconds and are not duplicated in the Debug window.

Do not put `DebugCommand` itself on the gameplay protocol. Explicit DEV envelopes remain narrow requests. `DevSpawnNpc` carries only a stable NPC `ContentId`; the server resolves content, chooses the authoritative player pose and allocates the runtime `EntityId`. The Debug Overlay never mutates a World directly.

## Compact Debug UI (D2)

In-client egui. Default tab is **Debug**. Forensic tabs remain on the in-client Diagnostics / Network surfaces until D4/D6 move observation out. `DebugSnapshot` is deleted; tabs read `DiagnosticsFrame`.

**Always (Game screen, overlay open):**

- FPS · tick · connection state · RTT latest
- Map / channel / instance plus Channel `[0]`/`[1]` (`DevSetChannel`)
- Center-screen ASCII toast (~3s, wrap at 70% viewport width, smaller type) for Reset / Reanchor / Channel. Not duplicated in the Debug window.
- Time-scale / jitter / impairment / Force Back / Force ClimbBack warning chips when those modes are active (also painted if the overlay is closed)
- Transition / input-gate chips **only when not idle / locked**
- Interact / Target / Portal: **reserved** compact rows (NONE when idle). No show/hide jump.

**Debug tab body:**

- Time scale 1.0 / 0.5 / 0.25
- Reset to Spawn Point (always); Reanchor Prediction when a replica local entity is present
- DEV NPC Spawner: select a projected runtime NPC and request a transient spawn at the server-authoritative player pose. The spawn is not map or persistence content and lasts until server shutdown.
- Local pose: position, velocity, grounded, input X/down
- Camera: following X/Y, dead-zone half-extents, Follow / Center On Player
- Replica: known count, last seq, pred active / lead error
- Presentation + gizmo toggles
- Display presets (collapsed)

**Not in compact chrome:** bone inspect, entity tree, interp probe, prediction forensics, impairment, collision history, jitter 180-frame block, equipment buttons, A1/A2 proof. Channel `[0]`/`[1]` **are** in compact chrome (one canonical control).

**Diagnostics page / later external app:** everything classified Diagnostics above, plus history rings and dumps.

World gizmos stay in-client. The external observer shows numbers and timelines, not a second renderer.

## Compile-time DEV / shipping boundary

**D4 complete** (ADR-0060). Cargo feature, **not** `cfg(debug_assertions)` alone. Release capacity runs still need the overlay (`cargo run --release` with default features).

```text
purgatory-client
  default = ["dev-diagnostics"]
  # shipping profile: --no-default-features

dev-diagnostics
  → optional egui / egui-winit / egui-wgpu
  → overlay module, Connection Frontend, DebugCommand, DiagnosticsFrame assembly,
     jitter isolation, impairment UI, gizmos, privileged proof surfaces
```

Shipping client (`--no-default-features`):

- no egui link; no Debug overlay / Connection Frontend / `DebugCommand`
- time scale fixed at 1.0; network impairment forced Off at connect
- presentation proof/force flags compile to off
- auto-connect once on Connection (no in-window connect UI yet)
- product character draw remains (`skeleton_debug` / presentation placeholders)

Shipping **server** DEV-handler strip remains a **later follow-up** (not D4). Wire tags stay; shipping servers should ignore or never compile DEV handlers — record that choice when done.

## Risks / invasive extraction

| Area | Risk |
|---|---|
| `overlay.rs` | Reads `DiagnosticsFrame` domain fields (D3). Compact chrome + tab dedup from D2. |
| egui as unconditional dependency | **Resolved in D4:** optional behind `dev-diagnostics`. |
| Presentation view flags in draw | **D4:** proof/force injections cfg to off without the feature. |
| `ObserverAoiDebug` on the wire | Already DEV data on the gameplay session. Do not expand. Omitting it is server-profile, not D1. |
| Time scale | Affects `SimulationClock` locally only; must not exist in shipping. |
| A-track overlay controls | D-track classifies them; A-track keeps proof semantics. Do not “clean up” Skeleton tab as a D2 redesign. |
| Identity namespaces in inspector | Extraction must keep World vs replica unmerged. |
| Hub Diagnostics vs this track | Name collision only. Keep separate. |

## Simpler than a new crate + IPC now

The requested end-state (compact UI + local IPC + external observer) is right. D1 did **not** implement IPC, a `purgatory-diagnostics` crate, or a shared wire format in `purgatory-protocol`.

D1 landed the minimum architecture that unblocks the split:

1. Domain structs + `DiagnosticsFrame` composition inside `apps/client`. **done**
2. `DebugCommand` separate from the frame (`DebugUiState` remains the local view bag). **done**
3. Skip assembly when no consumer. **done**
4. Compact chrome (D2). **done**
5. Overlay consumes `DiagnosticsFrame`; delete `DebugSnapshot` (D3). **done**
6. Compile-time `dev-diagnostics` strip (D4). **done**
7. IPC only after the structs are stable (D5).

That is simpler than introducing a diagnostics crate or a second snapshot language in D1, and it does not fight ADR-0016.

## Future observer topology

```text
Client subsystems
      ↓  diagnostics() / commands
dev diagnostics boundary          (client-internal; DEV-only)
      ↓
 ┌───────────────┬────────────────────┐
 │               │                    │
compact UI    local DEV IPC       trace/dump
                  ↓
         purgatory-diagnostics     (later process; observer only)
```

IPC is not the gameplay QUIC session. It is local, bounded, non-blocking, drop-oldest or skip-frame under backpressure. No subscriber ⇒ no assemble (after D1).
