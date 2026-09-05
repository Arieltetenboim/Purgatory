# Diagnostics roadmap

Status: **D4 complete. Do not start D5 until instructed.** Root `PHASE` is independent of this track. Not the Animation A-track. Do not reopen 8E or start A7.1 from this work.

Architecture: [`DIAGNOSTICS_ARCHITECTURE.md`](DIAGNOSTICS_ARCHITECTURE.md).

Each slice is a stop boundary. Do not pre-build later slices. Do not implement the external diagnostics application until that slice.

## Out of all listed slices

Gameplay protocol changes. Server-authority changes. Extra snapshot fields for diagnostics. A second game client. External process owning `World`. Shipping-profile work before D4. Animation Lab / A7.1. Merging Developer Hub Diagnostics into this overlay.

## Slices

### D0 — Architecture & inventory — **complete**

Current overlay/data-flow map, classification, domain boundary, compact-UI recommendation, compile-time gate, D1+ plan. Documents only.

### D1 — Domain structs + command split — **complete**

Inside `purgatory-client` only:

- Compose `DiagnosticsFrame` from subsystem `diagnostics()` methods (reuse `InterpDiagnostics`, `PredictionDiagnostics`, `NetworkSnapshot`, …).
- Introduce `DebugCommand` and keep `ViewCommand` / `DebugUiState` toggles off the read-only frame.
- Assemble the frame **only when a diagnostics consumer is active** (`DiagnosticsDemand`: overlay today; IPC slot unused until D5). Connection Frontend with overlay closed uses a display-only stub.
- No chrome redesign. No IPC. No new crate. No A-track proof changes beyond re-routing flags if required.

Stop. Overlay still looks like today.

### D2 — Compact Debug UI — **complete**

Apply the compact chrome rules in the architecture doc:

- Exception-driven Interaction / Target / Portal / transition chips.
- Default page = now-state. Forensic tabs stay available but are not the landing view.
- Deduplicate Channel / gizmo / pose controls.

Stop. Still in-client egui. Still no IPC. `DebugSnapshot` remains until D3.

### D2.1 — Overlay regression repair — **complete**

Gizmo master-arm, Reset path vs replica presence, Display render-scale summary. Stop. Do not start D3.

### D2.2 — Channel access + Reset semantics — **complete**

Channel `[0]`/`[1]` on compact chrome beside Map / Channel / Instance (`DevSetChannel` only). Debug tab **Reset to Spawn Point**: connected sends `DevResetPlayer` (protocol v14, ADR-0059); offline uses local `DebugAction::ResetPlayer`. Replica present also shows **Reanchor Prediction**. Center-screen ASCII toast only. Stop. Do not start D3.

### D3 — Remove DebugSnapshot adapter — **complete**

Overlay consumes `DiagnosticsFrame` domain fields directly. Delete flat `DebugSnapshot` / `from_frame` / `pred_error` adapter alias. No replacement god-view-model. Demand gating and domain `diagnostics()` ownership unchanged. Compact chrome / commands / gizmos / protocol unchanged. Stop. Do not start D4.

### D4 — Compile-time DEV/shipping boundary — **complete**

Cargo feature `dev-diagnostics` (default on; ADR-0060). Shipping `--no-default-features` omits egui, overlay, `DebugCommand`, Connection Frontend, impairment UI, jitter isolation, and privileged proof surfaces. Auto-connect once without Connection Frontend. Server DEV-handler strip is **not** in D4 (follow-up). Stop. Do not start D5.

### D5 — Local DEV IPC — **not started**

Bounded, non-blocking, drop-under-backpressure. Observer receives `DiagnosticsFrame` (or a compact encode of it). **Not** `purgatory-protocol`. Missing consumer must not stall. No assemble when no subscriber and overlay closed.

### D6 — External `purgatory-diagnostics` process — **not started**

Read-only observer UI. Hub-launched is allowed (Animation Lab pattern). Must not simulate, predict, or replicate. Must not send privileged commands in the first version (observe only).

## Dependency sketch

```text
D0 docs/inventory                 ← complete
D1 domain structs + commands + skip-assemble-when-closed  ← complete
D2 compact Debug UI                    ← complete
D2.1 overlay regression repair         ← complete
D2.2 Channel chrome + Reset semantics  ← complete
D3 delete DebugSnapshot / frame consumers ← complete
D4 compile-time strip                  ← complete
D5 local DEV IPC
D6 external observer process
```

## Quality gate

D1–D3: see prior gates. D4: prove both `cargo clippy/test -p purgatory-client` (default `dev-diagnostics`) and `cargo clippy/test -p purgatory-client --no-default-features`. ADR-0060. Server DEV-handler strip is not part of this gate.
