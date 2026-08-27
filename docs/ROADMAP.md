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
| 6 | Authoritative multiplayer movement | not started |
| 7 | Client prediction, reconciliation, interpolation | not started |
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
- `Graphic/` remains reserved and unused.
- Git integration is deferred.

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
- Debug Network tab. Client auto-connects to `127.0.0.1:5001`. Failed connect does not kill the renderer.
- No InputCommand, snapshots, prediction, or remote players.
- DEV-ONLY self-signed cert + skip-verify. Production PKI is not implemented.
- Do not start Phase 5.1 until instructed.

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
