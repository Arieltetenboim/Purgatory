# Character animation roadmap

Status: **A0–A6 complete. A7.0 complete. A7.1 complete.** Do **not** start A7.2 until instructed. Root `PHASE` is independent of this track (do not change it here). **8E is complete; do not modify/reopen/extend it.** Not S3, not Slot construction. Do **not** mark global A7 complete.

Architecture: [`CHARACTER_ANIMATION_ARCHITECTURE.md`](CHARACTER_ANIMATION_ARCHITECTURE.md). Skeleton: [`CHARACTER_SKELETON_ARCHITECTURE.md`](CHARACTER_SKELETON_ARCHITECTURE.md), [`HUMANOID_RIG_SPEC.md`](HUMANOID_RIG_SPEC.md). Phase 8 presentation: [`PHASE_8D_REPORT.md`](PHASE_8D_REPORT.md).

Each slice is a stop boundary. Do not pre-build later slices.

This A-track **is** the animation implementation sequence. It supersedes skeleton-roadmap **S5–S7** as the place where clips, Idle, and Walk are built. Skeleton S3/S4/S8 remain separate.

## Out of all listed slices

Art, sprites, atlases, Character Lab, IK, mesh deformation, physics bones, Slot/draw-order animation, events / notify **dispatch**, root motion, Bezier/easing, additive animation, upper/lower-body masking, complex state machines, equipment attachment animation, production combat clips, server-side animation sampling, protocol bone or facing messages. **8E is complete; do not modify/reopen/extend it.**

Hub **launch** of Animation Lab is A7.0. The lab is not the game client Skeleton tab.

## Slices

### A0 — Documents / contracts — **complete**

### A1 — One bone / minimal keyframes — **complete**

### A2 — Clock + looping — **complete**

### A3 — Semantic activity → per-character playback — **complete**

### A4 — Jump/Fall + minimal transition smoothing — **complete**

### A5 — Attack/Hurt one-shot presentation — **complete**

### A6 — Data-driven `.anim` assets + Walk prototype — **complete**

Validated A6 v1 assets under `content/shared/animations/dev/`. Runtime still `include_str!`s those files. Markers are parsed, not dispatched. `root` is not keyed.

### A7.0 — Animation Lab Core — **complete**

Standalone eframe **Animation Lab** launched from Developer Hub (Content + Dashboard). Authors A6 `.anim` through parse / `AnimationClip::try_new` / `sample` / `evaluate`. New Clip, Open, Save, Save As. Visible bottom Timeline / Dope Sheet (time 0→duration, authored tracks, keys, playhead, reserved marker row). Direct rotation via an **edit transaction**. Command/history Undo/Redo + jump-to-history-point (promoted into A7.0; not deferred to A7.1). Fixed 0.01s key-time grid as a document rule. Authoring policy: `root` non-keyable; `tx`/`ty` add only on `pelvis` / `torso` / `head`. No notify dispatch. No hot reload (client rebuild to see in-game).

### A7.1 — Authoring QoL + Depth/Foreshortening — **complete**

Copy/paste (Ctrl+C/V) and Copy Pose / Paste Pose. Multi-select (Ctrl, Shift range, box select) with batch move/delete. Editor snapping (grid / existing key / marker; 0.01s storage unchanged). Facing Left preview (no asset rewrite). A→B transition preview (no state machine). Authored `depth_angle` channel (`depth` token) with shared `sample_and_project` runtime path. Clipboard / snap / mirror / transition / selection are editor state only.

Stop. Do not start A7.2 automatically.

### A7.2 — Curves + better interpolation — **not started**

Bezier / curve editor. Do not start from A7.1.

### Notify Runtime — **not started**

Dispatch reserved marker data. Not A7.1. After A7.2.

## Future pointers (not scheduled here)

| Topic | Owner when it happens |
|---|---|
| One-leg / Walk polish beyond A6 | Character Presentation + this runtime |
| Data-driven clips under `/assets` | Presentation assets; format unset |
| Many-instance cost | Measure; record in [`PERFORMANCE_BUDGETS.md`](PERFORMANCE_BUDGETS.md) |
| Combat hit frames / events | Gameplay + presentation (not clip-driven authority) |
| Paper Doll sprite foreshortening | Presentation; reuse authored `depth_angle` |

## Dependency sketch

```text
A0 docs/contracts          ← complete
A1 one bone sample + placeholder visual  ← complete
A2 clock + loop            ← complete
A3 semantic Idle/Move per-character playback  ← complete
A4 Jump/Fall + transition blend  ← complete
A5 Attack/Hurt one-shot presentation  ← complete
A6 .anim assets + Walk prototype  ← complete
A7.0 Animation Lab Core    ← complete
A7.1 Authoring QoL + depth ← complete
A7.2 Curves + interpolation
Notify Runtime
```

## Quality gate

A7.0 recorded: `./scripts/check.ps1` GREEN 2026-09-03. A7.1 recorded: `./scripts/check.ps1` (see [`TEST_GATES.md`](TEST_GATES.md) Gate A7.1). Visual Jump/Fall / depth motion after Save + client rebuild still needs owner confirmation. Do not start A7.2.
