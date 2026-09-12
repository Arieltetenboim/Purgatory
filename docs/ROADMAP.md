# PURGATORY Roadmap

This is the current development map for PURGATORY.

The root [`PHASE`](../PHASE) file is the exact gameplay-phase marker. Parallel tooling / presentation tracks do **not** change `PHASE` unless explicitly promoted into the main gameplay sequence.

## Current

- **ERA II — Combat & First Playable Loop**
- **Phase 11 — Item Loop: complete + closeout**
- **Root `PHASE`: `11.closeout`**
- **Active parallel track:** FORGE N — NPC authoring/runtime; N10a-N10f complete and merged
- **Main gameplay next:** Phase 12 — Character Continuity, intentionally not started
- **Protocol: v27**

---

# Main gameplay sequence

## Foundation — complete

| Phase | Name | Status |
|---:|---|---|
| 0 | Bootstrap | complete |
| 1 | Custom runtime and clock | complete |
| 2 | Client window + renderer foundation | complete |
| 3 | Input + local character | complete |
| 4 | World / entity / movement foundation | complete |
| 5 | Networking, replication, prediction and load foundation | complete |
| 6 | Runtime, content, persistence, AOI and replication architecture | complete |
| 7 | Capacity, parallelism and production scaling | complete + closeout |
| 8 | Character Presentation + Equipment Runtime | complete + closeout |

Detailed reports remain under `docs/PHASE_*` and are historical evidence, not the active roadmap.

## 9 — Ability & Combat Runtime — complete

| Slice | Name | Status |
|---|---|---|
| 9A | Ability Contracts | complete |
| 9B | Delivery & Effects | complete |
| 9C | Combat Integration | complete |
| 9D | Combat Presentation Events | complete |

Historical implementation also included the minimal creature combat-driver bridge recorded as `9E`; that work is complete and was subsequently absorbed into the Phase 10 PvE loop.

## 10 — PvE Integration & Combat Loop — complete

| Slice | Name | Status |
|---|---|---|
| 10A | Normal-Session Creature Spawn | complete |
| 10B | Target Acquisition & Approach | complete |
| 10C | Creature Ability Driver | complete |
| 10D | Player Death & Respawn | complete |
| 10E | End-to-End PvE Proof | complete |

Phase 10 is closed. Known presentation rough edges belong to the later polish pass unless new evidence shows a runtime failure.

## 11 — Item Loop — complete + closeout

| Slice | Name | Status |
|---|---|---|
| 11A | Item Definitions | complete |
| 11B1 | Authoritative Item Runtime State + World-Drop Manifestation | complete |
| 11B2 | Pickup Transaction | complete |
| 11C | Inventory | complete |
| 11D | Equipment | complete |
| 11E | Equip → Gameplay / Presentation Proof | complete |

Goal: establish the first complete item loop without redesigning the already-proven Character Presentation / Equipment foundation.

Phase 11 is closed. The closeout report records the successful normal
client/server runtime proof and the intentionally deferred Inventory UI,
persistence, advanced stacking, trading, currency, and economy work:
[`PHASE_11_CLOSEOUT_REPORT.md`](PHASE_11_CLOSEOUT_REPORT.md).

## 12 — Character Continuity — planned

| Slice | Name | Status |
|---|---|---|
| 12A | Persistent Character State | planned |
| 12B | Save / Load | planned |
| 12C | Inventory & Equipment Persistence | planned |

Further Phase 12+ slicing should be added only when the current design is agreed, rather than preserving obsolete legacy numbering.

---

# Parallel character / presentation tracks

These tracks can progress beside the gameplay roadmap. They are deliberately separated so tools and visual-content work do not accidentally reopen gameplay phases.

## FORGE N — NPC Authoring & Runtime

The N track now has two proven layers:

- **NPC Lab N0-N6b:** authoring, typed conditions/actions, pools, synthetic Test Bench, player-facing preview and compact selection diagnostics are implemented and locally verified.
- **Runtime N10a-N10f:** the authored model is integrated into the normal server-authoritative game runtime. N10 is complete and merged to `master`.

The current runtime contract is [`NPC_DIALOGUE_RUNTIME.md`](NPC_DIALOGUE_RUNTIME.md). Earlier design and slice handoff documents remain historical implementation evidence and should not be used as current-status sources.

### N10 — NPC Dialogue Runtime — complete

| Slice | Name | Status |
|---|---|---|
| N10a | Runtime Content + Identity | complete + merged |
| N10b | Social NPC + Humanoid Idle | complete + merged |
| N10c | Dialogue Session + NPC Bubble | complete + merged |
| N10d | Choices + Continuation | complete + merged |
| N10e | Narrative State + Authoritative Actions | complete + merged + manually verified |
| N10f | Local Presentation + Multiplayer Isolation | complete + merged + manually verified |

N10f includes the server-authoritative per-player dialogue reopen guard. The normal dialogue flow, authoritative actions, multiplayer isolation, local-only facing/animation and reopen cooldown were accepted manually on 2026-09-12.

### N track — remaining work

These are follow-ups, not unfinished N10 slices:

- close active dialogue when the player or target NPC dies; the current interaction validator does not yet treat Health/death as an invalidation condition;
- persist per-player narrative facts, NPC Met and Dialogue Heard when Character Continuity / Phase 12 resumes;
- N7 Behavior & Activity only after real NPC content proves a stable vocabulary;
- N8 broader Presentation authoring bridge where future NPCs need additional reusable presentation references beyond the proven dialogue cue path;
- N9 Voice authoring/runtime only when the content workflow actually needs it;
- define rare-pool cadence only when real authored content requires it;
- later polish: final bubble art/responsive layout, richer text/localization, controller navigation, schedules, shops and other NPC systems as separate scoped work.

Do not build a generic quest manager, scripting engine, behavior framework or voice pipeline merely to advance the N numbering.

## ART-R — Character ART Integration v1

Purpose: replace geometric character placeholders with real authored character art through the existing skeleton / Character Presentation / Paper Doll architecture.

| Slice | Name | Status |
|---|---|---|
| ART-R1 | Multi-Atlas Renderer Foundation | planned |
| ART-R2 | VisualAssetRegistry + Visual Pack Loader | planned |
| ART-R3 | Base Body → Textured Skeleton | planned |
| ART-R4 | Character Presentation Proof | planned |
| ART-R5 | Equipment Bridge | planned |

### ART-R1 — Multi-Atlas Renderer Foundation

Remove the assumption that every textured character quad uses one texture. Introduce a small texture identity and allow ordered draw ranges to rebind textures while preserving the existing Character Presentation draw order.

**Gate:** existing Headwear still renders correctly; colored quads are unchanged; multiple atlases can interleave without changing semantic layer order.

### ART-R2 — VisualAssetRegistry + Visual Pack Loader

Own the mapping from authored visual keys to texture/atlas regions and load a defined visual pack instead of hard-wiring texture assumptions into rendering code.

### ART-R3 — Base Body → Textured Skeleton

Bind real base-character body visuals to the existing humanoid skeleton while keeping skeleton transforms and animation runtime as the motion source of truth.

### ART-R4 — Character Presentation Proof

Prove the complete local + remote Character Presentation path with real base-body art, existing activity/view selection, layering and animation.

### ART-R5 — Equipment Bridge

Connect equipment visuals to the same asset registry / multi-atlas path so base body and Paper Doll attachments share one presentation architecture rather than parallel render systems.

ART-R is a presentation integration track. It does not redefine gameplay equipment ownership, animation authority or server contracts.

---

## C — Character Lab / Cutting & Composition Tool

Purpose: author the character sheet used by the runtime without manually encoding crop rectangles and presentation metadata in code.

Current design keeps the source art as a sheet and authors crop/source rectangles; it does not require exporting a separate PNG for every body part.

| Slice | Name | Status |
|---|---|---|
| C1 | Character-sheet schema | planned |
| C2 | Hub page + tool shell | planned |
| C3 | Source-sheet loading, pan/zoom and crop authoring | planned |
| C4 | Attachment / part inspector | planned |
| C5 | Slot-driven character composer | planned |
| C6 | Optional pair-link editing | planned |
| C7 | Centralized draw-order data | planned |
| C8 | Save/load `char.sheet.json` + quality gate | planned |

### C1 — Schema

Define the durable authoring data model for body-part / attachment crops, identities, slots and source rectangles.

### C2 — Tool shell

Standalone Character Lab surface launched through the Developer Hub. Keep authoring state out of the game client.

### C3 — Cutting / crop authoring

Load the source character sheet, navigate it with pan/zoom, select a body region and author its source rectangle visually.

### C4 — Inspector

Edit the selected part/attachment metadata without manipulating raw JSON.

### C5 — Composer

Preview the authored parts together through slot semantics so crop mistakes, pivots and overlap problems are visible immediately.

### C6 — Pair-link

Optional authoring support for related front/back or paired parts where useful; not a new runtime ownership model.

### C7 — Draw-order data

Centralize semantic draw order. This is **not** unrestricted numeric Z-values and not a Z-order editor.

### C8 — Persistence + gate

Save/load the authoring document at `Graphic/character/authoring/char.sheet.json`, run the quality gate and stop.

**Stop boundary:** C1–C8 only. Rigging, bones, sockets, animation authoring and atlas export are separate concerns and should not be silently folded into this tool track.

---

## A — Character Animation / Animation Lab

Canonical detail: [`CHARACTER_ANIMATION_ROADMAP.md`](CHARACTER_ANIMATION_ROADMAP.md).

| Slice | Name | Status |
|---|---|---|
| A0 | Documents / Contracts | complete |
| A1 | One Bone / Minimal Keyframes | complete |
| A2 | Clock + Looping | complete |
| A3 | Semantic Activity → Playback | complete |
| A4 | Jump / Fall + Transition Smoothing | complete |
| A5 | Attack / Hurt One-Shots | complete |
| A6 | Data-Driven `.anim` Assets + Walk Prototype | complete |
| A7.0 | Animation Lab Core | complete |
| A7.1 | Authoring QoL + Depth / Foreshortening | complete |
| A7.2 | Curves + Better Interpolation | not started |
| Notify Runtime | Marker dispatch runtime | not started |

### Animation Lab current capability

A7.0 established the standalone authoring loop: New/Open/Save/Save As, timeline/dope sheet, direct pose editing, key editing, Undo/Redo and authored `.anim` files using the same parser/runtime path as the client.

A7.1 added copy/paste, pose copy/paste, multi-select, batch key operations, snapping, facing preview, A→B transition preview and authored `depth_angle` foreshortening.

**Current stop:** do not start A7.2 automatically. Global A7 is not complete.

---