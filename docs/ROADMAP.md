# PURGATORY Roadmap

This is the current development map for PURGATORY.

The root [`PHASE`](../PHASE) file is the exact gameplay-phase marker. Parallel tooling / presentation tracks do **not** change `PHASE` unless explicitly promoted into the main gameplay sequence.

## Current

- **ERA II — Combat & First Playable Loop**
- **Phase 11 — Item Loop: complete + closeout**
- **Root `PHASE`: `11.closeout`**
- **Active parallel track:** FORGE M — Monster Authoring & Runtime on `forge/mob-lab`
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

## FORGE M — Monster Authoring & Runtime

Purpose: turn the proven single-creature PvE fixture into a validated,
content-backed path before building the Mob Lab editor. The current contract is
[`MONSTER_AUTHORING_RUNTIME.md`](MONSTER_AUTHORING_RUNTIME.md).

| Slice | Name | Status |
|---|---|---|
| M0 | Monster Contract + Ownership | implemented on `forge/mob-lab` |
| M1 | Monster Content + Validation | implemented on `forge/mob-lab` |
| M2 | Content-backed Red Slime Runtime | implemented; manual proof pending |
| M3 | Mob Lab Editor | planned |
| M4 | Mob Lab Test Arena | planned |
| M5 | Monster Presentation Bridge | deferred |

The whole FORGE M sequence uses one branch. Slice boundaries remain separate
commits/checkpoints rather than separate branches. Do not add unused ability,
loot, placement, AI-tree or presentation fields before their runtime consumer
is deliberately scoped.

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

## D — Diagnostics

Canonical detail: [`DIAGNOSTICS_ROADMAP.md`](DIAGNOSTICS_ROADMAP.md).

- D0–D4 complete.
- D4 compile-time separates privileged development diagnostics from shipping-style client builds.
- D5+ remains future work unless explicitly started.

---

# Engineering history & phase notes

This section intentionally preserves useful project archaeology: decisions, regressions, measurements, constraints and small details that explain **why the system looks the way it does**. These are historical notes, not current execution instructions.

## Early world / movement foundation

### Phase 3

- Input became semantic at the client boundary; physical `KeyCode` values stay client-side.
- `World` owns the gameplay entity loop and orchestrates intent, gravity, jump and integration.
- Renderer originally presented simulation AABBs only; early visuals were deliberately placeholders.

### Phase 4 / 4.1

- `World` introduced generational `EntityId` slots; stale IDs are rejected rather than accidentally aliasing a reused slot.
- `EntityId` was explicitly kept separate from authored Content IDs.
- The debug overlay was built as client-only egui over the existing winit/wgpu window — not a second OS window and not simulation ownership.
- Inspection and mutation were separated: snapshot-style read state vs explicit debug actions.

### FOOTNOTE — 4.5–4.8

- FOOTNOTE became the movement/platform interaction layer: acceleration/deceleration, air control, Solid + OneWay platforms and drop-through.
- Down + Jump ignores the supporting OneWay until the character has actually cleared its top; this closed an early floor-dependent drop-through bug.
- Vertical collision was changed to choose the nearest crossed surface, fixing overlap/jump teleports.
- Collision later moved to crossing-based normal response using previous→proposed AABBs rather than stale-overlap nearest-face pushes.
- A separate capped Solid penetration recovery path was kept for bad starting states instead of mixing penetration correction into normal motion.
- `CONTACT_EPSILON` was centralized after seam/corner/head-bonk regressions exposed repeated magic tolerances.
- The development arena accumulated intentionally ugly test geometry: stacked OneWays, seams, momentum routes, approximate slopes and isolated overlap regressions.
- `WorldBounds` became simulation-owned; camera follow/clamp and parallax remained presentation-only.

## Networking evolution

### Phase 5.0 — connection foundation

- Quinn/QUIC + Tokio became the transport stack.
- `ConnectionId` was deliberately kept distinct from `EntityId` and later world/map identities.
- The client starts from an explicit Connection Frontend. Auto-connect was deliberately avoided in the normal development build.
- A stale connection attempt cannot mutate a newer one; client lifecycle uses a local `ConnectionAttemptId` barrier.
- Lifecycle events and droppable RTT telemetry were eventually split after bounded-channel pressure exposed that they have different reliability requirements.
- Admission caps were treated as concurrency safety, **not** player-capacity claims.
- QUIC encryption was never treated as client honesty; network payloads do not directly mutate `World`.
- Severe protocol/abuse violations disconnect; decoders are bounded and expected to survive arbitrary bytes without panic.
- Stress/chaos success was defined as **convergence back to baseline**, not simply “the process survived.”
- Bind failure became a normal typed startup error; the server does not log “listening” until bind actually succeeds.

### Protocol golden-vector lesson

- Wire bytes became frozen by golden vectors early. Matching encoder+decoder changes are not sufficient evidence of compatibility.
- Intentional wire changes therefore require an explicit protocol version review and deliberate fixture update.

### Phase 5.1–5.5 — authority, replication, prediction

- Client input is intent-only; the server owns actual FOOTNOTE movement.
- Full snapshots arrived before interpolation/prediction so authority stayed obvious while each layer was added.
- Remote interpolation uses bounded history and a delayed render tick; snapshot arrival jitter must never rewind presentation time.
- Local prediction reuses the same simulation movement path rather than maintaining a separate “client physics” implementation.
- Raw predicted-vs-latest-authoritative distance was explicitly rejected as a divergence detector because visible lead is normal under latency.
- The useful metric became **aligned residual** after matching authoritative state against predicted history.
- Input emission was aligned to the same fixed simulation tick/sample consumed by prediction, closing a systematic ±1-tick press/release/jump phase error.
- Phase 5.5 added input epoch + sequence acknowledgement and restore/replay reconciliation.
- Late input collapse is intentional authoritative compaction; acknowledgement does not imply each historical held command received an individual physics step.
- `jump_pressed` collapsing behavior was recorded as movement-specific, not a generic policy for future skill actions.

### Phase 5.6–5.7 — impairment and load

- The impairment lab distinguishes reliable-stream delay/stall/head-of-line blocking from imaginary packet disappearance semantics.
- The load harness uses real headless QUIC bots instead of a second fake network stack.
- A dropped input-handoff issue was isolated and closed; final recorded runs showed `input_handoff_dropped = 0` at 100/150/200/256 connected clients.
- Localhost load numbers were never promoted directly into production player-capacity promises.

## Runtime / MMO architecture — Phase 6

### Identity and composition

- `WorldAddress` introduced explicit location identity while preserving the distinction between runtime entity, connection, character and content identities.
- Runtime composition stayed on the existing slot-vector World rather than introducing an ECS crate simply because the system became more component-like.
- Transform, Health and other runtime domains became optional composition with per-domain revision/dirty information.

### Interaction and content

- `InteractionSession` is authoritative gameplay/session state, not a UI window.
- `ContentId` is authored identity; runtime `MapId` is registry-assigned identity.
- Portals eventually received their own activation semantics; generic `E` interaction and portal activation were deliberately separated.
- Portal destination is authored transition metadata, not matching by `InteractableKind`.

### AOI — Area of Interest

AOI means **Area of Interest**: the server-side visibility/relevance region used to decide which world entities an observer needs.

- AOI became server-derived; the client does not report a trusted camera rectangle.
- The interest envelope follows the actual visible-view model plus prefetch/hysteresis rather than a simplistic player-centered radius.
- WorldAddress transitions are readiness-gated. Visibility does not reappear merely because a fade timer elapsed.
- Transition gameplay input is neutralized server-side and locked client-side until FadeIn completes.
- The final local presentation pose is shared by draw and camera so reconciliation smoothing does not let the camera and character disagree.

### Persistence

- `CharacterId` is server-minted and persists across reconnects; `EntityId` is a live runtime identity and changes.
- Duplicate live login is rejected rather than spawning a second authority for one Character.
- File persistence lives outside the repository. Source/install location is not writable game-state storage.
- Persistence I/O was kept off the 30 Hz simulation thread and revisions protect saves from stale completion.
- Restore intent was explicitly separated from current WorldAddress.

### Runtime scheduler / dirty replication

- Phase 6F used the existing World as owner instead of creating a generic `RuntimeServices` bag.
- Dirty/delta ownership stayed based on domain revisions and per-observer committed state rather than inventing a second parallel dirty-state model.
- Critical and Deferred runtime work received explicit scheduling/budget concepts.

## Phase 6G — scaling archaeology

This phase produced several of the project's most useful “measure first” lessons.

- A previously observed ~128-client stall was investigated rather than immediately labeled a simulation bottleneck.
- On the tested machine it was **not reproduced as a server-domain hang/blow-up** under the new timing instrumentation.
- Capacity instrumentation recorded coarse tick domains plus process CPU/memory into artifacts.
- Targeted AOI work roughly halved idle AOI/tick cost at 256 in the recorded environment, while continuous-motion AOI remained the important case.
- The 256-motion result was recorded as a characterized limit; 128 had healthy headroom. This was treated as evidence, not a reason to redesign blindly.
- Global interest generation was removed in favor of incremental observer/spatial invalidation.
- Exact enter/leave XOR invalidation eventually made tiny movement dirty essentially the mover/local neighborhood rather than the world.
- Dirty-driven replication changed fan-out from “scan everybody every time” toward changed-entity → interested-observer pending work.
- Relationship/density/budget policy reduced hotspot stranger updates by roughly **3×** versus the recorded baseline.
- This closed the replication **architecture**. Future production tuning was deliberately separated from reopening the architecture.

## Phase 7 — capacity / production scaling

- Phase 7 intentionally began with measurement rather than another architecture rewrite.
- The project recorded that “128-client degradation” was not itself proof of a server simulation bottleneck.
- 7.1–7.8 remained focused on evidence-backed single-process capacity, representative workload and attribution.
- The Phase 7 production performance gate closed YELLOW rather than forcing a fake GREEN.
- A weak-client-authority audit was part of closeout, keeping scaling work from quietly expanding client trust.

## Phase 8 — Character Presentation + Equipment

### Equipment model

- Equipment became six authoritative optional slots with `None` as a valid empty value — no fake sentinel ContentId.
- Equipment dirty state is slot-level inside the existing replication/revision model.
- Gameplay equipment definitions and client presentation definitions share identity but not authority responsibilities.

### Presentation bridge

- Local predicted and remote interpolated players share `CharacterPresentationState` / `SkeletonInput` presentation logic.
- `BoneTarget` is bound to Humanoid rig bones once per presentation set rather than resolved repeatedly per frame.
- Equipped content resolves to bound attachments when equipment changes, not every draw.
- Attachment composition is bone world transform ∘ authored local anchor ∘ correction.

### Draw-order contract

- Character Presentation owns semantic layering: `ArmBack → LegBack → Core → LegFront → Head → ArmFront`.
- Attachments inherit their semantic bone layer instead of gaining arbitrary numeric Z.
- Back view hides front-authored limbs/attachments and remaps the surviving near limbs into the visible paint positions.
- `hidden_base` remains narrowly scoped to hiding matching base-body pieces rather than becoming a generic visibility override.

### ClimbBack / view selection

- Activity determines `PresentationView`; `ClimbBack` maps to Back instead of inferring orientation from velocity.
- `ClimbBack` samples the authored `climb_back.anim` through the same animation runtime path as other activities.
- Missing authored animation at compile time fails via `include_str!`; invalid parse falls back to bind/no-animation, not silently to Idle.
- Equipment selects Side/Back visual keys from the view; missing Back visual means omission, not an automatic Side-as-Back substitute.
- 8F closeout deliberately removed a leftover identity `playback_activity()` remap and kept one canonical activity → view → clip → pose → draw-plan path.

## Character Animation track

- Animation is presentation-only; gameplay authority never depends on sampling a client animation clip.
- A0–A6 established keyframes, playback, activity mapping, Jump/Fall transition smoothing, Attack/Hurt one-shots and data-driven `.anim` assets.
- The runtime still compile-embeds current development `.anim` assets; client rebuild is required to see authored changes in-game.

### Animation Lab A7.0

- Built as a standalone eframe tool launched from Developer Hub rather than stuffing authoring UI into the game client's Skeleton/Debug page.
- Uses the same parse → `AnimationClip::try_new` → sample → skeleton evaluate path as runtime.
- New/Open/Save/Save As, timeline/dope sheet, direct pose editing and command/history Undo/Redo are core functionality.
- The 0.01 s key-time grid is a document/storage rule, not merely visual snapping.
- Root remains non-keyable; translation channels are restricted to selected central bones.

### Animation Lab A7.1

- Added copy/paste, Copy Pose/Paste Pose, multi-selection and batch key operations.
- Snapping is editor state; stored key times remain on the document grid.
- Facing-left preview does not rewrite the asset.
- A→B transition preview is an authoring preview, not a runtime state machine.
- `depth_angle` became authored data and shares the runtime `sample_and_project` path, preparing Paper Doll foreshortening without giving the editor separate motion semantics.
- A7.2 Curves remains intentionally separate; global A7 is not complete.

## Diagnostics track

- Diagnostics progressively separated domain snapshots/commands from presentation UI.
- D4 made privileged diagnostics compile-time optional (`dev-diagnostics`).
- Shipping-style `--no-default-features` means the debug overlay and privileged diagnostics code are absent by compilation, not merely hidden.
- The external-observer direction remains deferred; the project did not prematurely build a second diagnostics application.

## Combat — Phase 9

- Ability lifecycle reuses the existing Action Runtime (`Windup / Active / Recovery`) rather than creating an ability-specific scheduler.
- `AbilityDefinition` is content-driven; damage executes through an `AbilityEffect` path rather than direct ability-specific Health writes.
- Health remains optional; Alive/Dead is derived.
- The first Basic Strike uses a forward AABB query and can successfully activate even if it hits nothing.
- Client sends ability intent, never damage values.
- Server authorizes ownership/grant/activation shape before requesting the ability from World.
- Ability execution produces Attack presentation; authoritative non-lethal damage produces Hurt; Health <= 0 produces persistent Dead.
- Dead presentation overrides transient Attack/Hurt.

## PvE loop — Phase 10

- Normal-session creature spawning, acquisition/approach, creature ability driving, player death/respawn and the full end-to-end PvE proof are complete.
- Final manual proof included a live creature pursuing the player, attacks producing Hurt, death presentation after repeated hits and creature pursuit stopping against the dead player.
- The current awkward/frozen Dead visual is a known presentation rough edge, **not** evidence that the combat loop failed.
- NPC Hurt/Dead/Respawn presentation was wired using existing Health/lifecycle events rather than inventing a second combat presentation state.
- Phase 10 closed with an end-to-end normal-session integration proof.

## Known polish / future revisit bucket

These are intentionally not phase reopeners unless new evidence shows a shared runtime problem.

- Animation Lab still has authoring/UI rough edges that deserve a dedicated polish pass.
- Current Dead presentation is a placeholder/rough edge.
- Rendering / art integration still needs the ART-R track before the character stops looking like geometric/debug presentation.
- Production replication policy values remain tuning work; the replication architecture itself was closed in 6G.
- Manual/debug-tool improvements should not silently redefine authority or frozen gameplay contracts.

---

# Important architecture references

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — overall runtime architecture
- [`DECISIONS.md`](DECISIONS.md) — Architecture Decision Records (ADRs) and frozen contracts
- [`CHARACTER_SKELETON_ARCHITECTURE.md`](CHARACTER_SKELETON_ARCHITECTURE.md) — humanoid skeleton
- [`CHARACTER_ANIMATION_ARCHITECTURE.md`](CHARACTER_ANIMATION_ARCHITECTURE.md) — animation runtime
- [`CHARACTER_ANIMATION_ROADMAP.md`](CHARACTER_ANIMATION_ROADMAP.md) — detailed A-track
- [`DIAGNOSTICS_ROADMAP.md`](DIAGNOSTICS_ROADMAP.md) — detailed D-track
- [`TEST_GATES.md`](TEST_GATES.md) — validation gates
- [`PERFORMANCE_BUDGETS.md`](PERFORMANCE_BUDGETS.md) — measured performance budgets

# Roadmap rule

The roadmap records **current intended order and durable parallel tracks**, while the Engineering History preserves useful project archaeology. Detailed proof remains in phase reports / ADRs. Historical stop instructions must not be mistaken for current execution guidance.
