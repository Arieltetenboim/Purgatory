# Character animation architecture

Status: **A7.1 complete** (A7.0 lab + copy/paste, multi-select, snapping, mirror/transition preview, `depth_angle` foreshortening). Root `PHASE` is independent of this track (do not change it here). **8E is complete; do not modify/reopen/extend it.** Not S3, not Slot construction. A6 proof clips remain validated external assets with reserved timed markers (not dispatched). Do **not** mark global A7 complete. **A7.2 is next.**

Companion documents: [`CHARACTER_ANIMATION_ROADMAP.md`](CHARACTER_ANIMATION_ROADMAP.md) (A1–A7.1 slices), [`CHARACTER_SKELETON_ARCHITECTURE.md`](CHARACTER_SKELETON_ARCHITECTURE.md) (Definition + local Pose → world Pose), [`HUMANOID_RIG_SPEC.md`](HUMANOID_RIG_SPEC.md) (Humanoid v0 bones / root vs pelvis / rigid-cutout policy), [`PHASE_8D_REPORT.md`](PHASE_8D_REPORT.md) (semantic `CharacterPresentationState`).

Living budgets stay in [`PERFORMANCE_BUDGETS.md`](PERFORMANCE_BUDGETS.md). ADRs stay in [`DECISIONS.md`](DECISIONS.md). Do not copy volatile numbers here.

## Purpose

PURGATORY will animate modular 2D humanoids on the **client presentation** path. Animation sits between Character Presentation and `purgatory-skeleton`:

```text
semantic character presentation state
    → animation selection / playback   (Character Presentation; A3 Idle/Move)
    → sample AnimationClip at presentation time  (rot / tx / ty + optional depth_angle)
    → apply_depth_projection onto caller-owned LocalPose
    → produce final LocalPose
    → purgatory-skeleton::evaluate
    → WorldPose
    → placeholder / later attachment presentation
    → renderer
```

The **core contract of Skeleton** remains only:

```text
Definition + final LocalPose  →  WorldPose
```

Animation must not contaminate Skeleton Core, simulation, protocol, or renderer ownership.

## Layer ownership

| Layer | Owns | Must not own |
|---|---|---|
| **Character Presentation** (`apps/client/src/character_presentation/`) | Semantic state (`Idle` / `Move` / `Jump` / `Fall` / `Attack` / `Hurt`). Mapping semantic state → which clip to play. Root position / facing from already-finalized local or remote presentation. | Clip sampling math. Bone hierarchy evaluate. GPU. Wire encode. |
| **Animation Runtime** (`purgatory-animation`) | Immutable `AnimationClip` data. Sampling clip at time `t` into a caller-owned `LocalPose`. Mutable per-instance playback state (A2+). | Skeleton Definition mutation. World movement. Player vs NPC. Local vs remote. Equipment. Rendering. Network. |
| **Skeleton** (`purgatory-skeleton`) | Definition, bind locals, parent hierarchy, `evaluate`, slot metadata. | Clips, time, Idle/Walk/Attack/Hurt, velocity, Player/NPC, local/remote, networking, equipment, rendering. |
| **Renderer** | Draw already-evaluated world transforms (placeholders today). | Pose sampling, clip selection, simulation. |
| **Simulation / server / protocol** | Authoritative motion, optional later **semantic** activity. | Bone transforms, animation frames, keyframe streams, Skeleton poses. |

Debug overlay (ADR-0016) may expose A1 sample-time / later playback debug. It must not become the production clip-selection system. Overlay `~` must not gate evaluation.

Existing S2 front-arm / front-leg **proof poses** are diagnostic LocalPose writes. They are **not** the animation system. A1 must not extend those enums into a clip model.

## What exists today (constraints)

- Humanoid v0: frozen **16-bone** topology. `root` (index 0) is the presentation anchor. `pelvis` is the primary animated body root. See [`HUMANOID_RIG_SPEC.md`](HUMANOID_RIG_SPEC.md).
- `LocalPose` is the **final parent-relative pose**, not an animation-delta buffer ([`crates/skeleton/src/pose.rs`](../crates/skeleton/src/pose.rs)). Bind pose is `copy_bind` / `from_bind`.
- Phase **8D** evaluates bind pose for visible players: `CharacterPresentationState` → `SkeletonInput` → `copy_bind` + write `root` translation from presented feet → `evaluate`. **A3** samples Idle/Move clips into that path before the root adapter.
- Local draw uses predicted + remainder presentation. Remotes use interpolation. Animation must not know which source produced the entity root.
- Placeholder presentation (P1–P4) already proves pivots, rigid cutout pieces, draw order, and oriented quads. A1 visual proof uses that path.
- Replication carries pose + velocity + equipment ContentIds. No facing bit, no bone channels. Protocol **v13** adds DEV Attack/Hurt oneshot **control** envelopes only; Enter/Update snapshot layout is unchanged (0 per-frame animation traffic).
- `Graphic/` is not a runtime content pipeline. This track does not import sheets or Character Lab.

## Module / crate ownership

**A1 home:** crate `purgatory-animation` at `crates/animation`.

- Depends **only** on `purgatory-skeleton` (`BoneIndex`, `BoneTransform`, `LocalPose`, `SkeletonDef` for construction validation).
- Zero dependencies on winit, wgpu, egui, Quinn, Tokio, simulation, protocol, content, common, client.
- Client depends on it for sampling. Character Presentation owns **which** clip and **when** playback starts/stops.
- A1 tests compile without the client GPU/network graph (same reason S1 is a crate).

Rejected for the sampler:

- Inside `purgatory-skeleton` — would make Skeleton aware of clips/time.
- Inside simulation / server / protocol / content / common.
- As the only home: `character_presentation` — mixes semantic mapping with sampling and forces A1 tests through the client crate. Presentation remains the **consumer**.

**A4 presentation ownership:** `CharacterPresentationSet` maps `PresentationActivity` to Idle/Move/Jump/Fall clips. Local activity uses predicted body velocity **and** grounded together when prediction is active (do not pair predicted velocity with lagged `replica.local_grounded`). Remotes infer Jump/Fall from replica `|vy|` (no per-remote grounded on the wire). Airborne classification precedes Idle/Move; apex while known-airborne is Fall.

**A5 one-shot ownership:** Simulation owns Attack/Hurt semantic duration and interruption (`Hurt` may interrupt `Attack`; `Attack` does not interrupt `Hurt`; locomotion never clears an active oneshot). Protocol v13 carries start events only (`DevPresentationOneShot` / `ServerPresentationOneShot`); clients overlay oneshot on locomotion activity. `LoopPolicy::Once` holds the final pose while semantic state remains active. Leaving Attack/Hurt returns to the current locomotion activity with the existing A4 ~0.10s blend. Clip completion / sample time / animation frames never grant gameplay authority.

**Phase 9A gameplay→presentation:** Ability/combat emits semantic cues (`Attack` / `Hurt` / `Dead`). Character Presentation consumes Attack/Hurt as oneshots. Dead is Health-derived (`current <= 0`) as persistent `PresentationActivity::Dead` (Hurt-clip placeholder). Ability code must not name clips or bones. Animation Runtime is unchanged.

Crate graph:

```text
purgatory-skeleton          (zero deps)
        ↑
purgatory-animation         (sampling only)
        ↑
purgatory-client            (selection, playback drive, placeholder draw)

simulation / server / protocol / content / common
    unaware of AnimationClip
```

## Data flow

```text
CharacterPresentationState          // pose, facing, activity, view, equipment
        │
        ▼
CharacterPresentationSet            // one AnimationPlayer + transition state per visible character
        │  A5: map activity → Idle/Move/Jump/Fall/Attack/Hurt/ClimbBack
        │  same mapped activity continues; change blends from current presented pose
        │  Once clips hold at duration while semantic oneshot remains active
        │
        ├─ LocalPose::copy_bind(def)
        ├─ sample(clip, t, &mut clip_local)
        ├─ optional blend(transition_from → clip_local) into presented local
        ├─ adapter writes root from presented pose   // always last; clips do not own world move
        └─ evaluate(def, local, world)
                ▼
            WorldPose → placeholders / attachments → renderer
```

A1/A2 Stage D diagnostics may still sample the head proof clip separately; they do not replace A3 runtime for visible characters.

Sampling must **not** mutate `SkeletonDef`. Bind locals on the definition are immutable after construction.

### Unkeyed Bone / channel

Initialize `LocalPose` from Bind Pose, then apply only channels the active clip keys.

| Situation | Result |
|---|---|
| Bone has no track | Keep bind local translation and rotation |
| Track exists, rotation keyed, translation not | Keep bind translation; write sampled rotation |
| Track exists, translation X keyed, Y not | Keep bind Y; write sampled X |
| Channel has no keys | Treat as unkeyed (keep bind) |

Do not zero unkeyed channels. Do not leave identity `(0,0,0)` on unkeyed bones.

### Root after sample

The presentation adapter **always** writes `root` translation (and facing scale, when that adapter exists) from the already-finalized presented entity pose **after** sampling. Locomotion clips must not key `root`. Accidental root keys are overwritten by the adapter and must not become gameplay movement.

## Animation data model (conceptual)

Immutable, shareable by many humanoid instances. No per-character clip clone.

```text
AnimationClip
    duration: f32                  // seconds, presentation time
    loop_policy: Loop | Once       // metadata; A1 sample ignores wrap
    bone_count: usize              // v0 compatibility guard only (see below)
    tracks: contiguous BoneTrack   // sparse; only keyed bones

BoneTrack
    bone: BoneIndex                // dense; no string on the hot path
    rotation: Option<Channel<f32>> // local plane radians, CCW, +Y up
    translation_x: Option<Channel<f32>>
    translation_y: Option<Channel<f32>>

Channel<T>
    keys: contiguous Keyframe<T>   // sorted by time, t in [0, duration]

Keyframe<T>
    time: f32
    value: T
    interpolation: Linear | Step   // policy toward the next key
```

Public contract: **immutable contiguous track data**. `Box<[T]>` is an allowed implementation detail, not a long-term API freeze.

One `BoneTrack` per keyed bone, with **independent optional channels** inside. A1 keys rotation only. Rigid-cutout clips will often key rotation without translation.

Do not freeze a combined “always write full `BoneTransform`” track: that would overwrite unkeyed translation with zeros or force every rotation clip to re-author bind translation.

**`bone_count` (if stored on the clip):** a **v0 compatibility guard** so `sample` can reject a `LocalPose` whose length does not match the definition used at `try_new`. It is **not** Skeleton identity, a definition handle, or a rig-version type.

### Clip validation (construction)

Validate at `AnimationClip::try_new` (or equivalent), **not** on the sampling hot path. Reject at least:

- non-finite duration, key time, or key value
- duration `<= 0`
- negative key times
- unsorted key times
- keys outside `[0, duration]`
- duplicate key times on the same channel
- `BoneIndex` out of range for the target `SkeletonDef`
- duplicate bone tracks (A1 has no merge semantics)
- empty keyed channel (`Some` with zero keys)

### Explicit-time sample boundaries (A1)

`sample(clip, t, &mut LocalPose)` does **not** wrap or loop. Deterministic:

| `t` | Result |
|---|---|
| Exact key time | Exact key value |
| Before first key | Hold first key value |
| Between keys | Interpolate (Linear / Step) |
| After last key | Hold last key value |

Non-finite `t` or bone-count mismatch → `Err`. On **any** `Err`, leave the caller-owned `LocalPose` **unchanged**.

`loop_policy` may live on the clip as metadata. Loop / Once wrap belongs to A2 `AnimationPlayer`, which converts playback time into clip-local `t` before calling `sample`.

### v0 channels

Supported: local translation X, local translation Y, local rotation, and A7.1 **`depth_angle`** (see below).

**Not clip channels:**

- Arbitrary scale / shear (adapter may still apply root `scale.x` for facing; that is not a clip channel)
- IK, mesh deformation, physics
- Slot animation, draw-order animation
- Events / notifies (A6 reserves timed-marker data in animation assets for future notify dispatch, but `purgatory-animation` does not execute them)
- Root motion
- Bezier / easing / hermite curves (A7.2)

Generic scale is rejected as an unknown `.anim` token. Foreshortening is the dedicated `depth` channel, not disguised scale.

### Depth / foreshortening (`depth_angle`, A7.1)

`depth_angle` is a **signed projection angle in radians** on a bone. It describes how a 2D limb segment orients into / out of the screen. It is **not** `tx`/`ty`, body rotation, Front/Back semantic swap, 3D XYZ, sprite deformation, or BoneTransform scale.

| Contract | Rule |
|---|---|
| Token | `depth <time> <radians> <Linear\|Step>` inside a `track`. Omitted = unkeyed = `0`. |
| Default | `depth_angle = 0` reproduces pre-A7.1 2D projected length exactly. Existing A6 assets stay valid. |
| Limits | Construction rejects non-finite values and `|θ| > π/2`. |
| Sign | Preserved. `+θ` and `-θ` produce the same projected length (`cos(|θ|)`). Sign does **not** swap `ArmFront`/`ArmBack` or `LegFront`/`LegBack`. |
| Projection | After sampling rot/tx/ty into a caller-owned `LocalPose`, `apply_depth_projection` scales each **child’s parent-relative translation** by `max(cos(|parent.depth_angle|), DEPTH_PROJECTION_MIN)` (`0.08`). Parent joint stays fixed; the child slides along the current offset ray; descendants follow `evaluate`. |
| Ownership | `DepthPose` is animation-crate sampled state. Skeleton Core stays translation + one plane rotation. Do not bake placeholder rectangle sizes into the clip. |
| Lab policy | Add/edit on limb bones (`upper_*` / `lower_*` / `hand_*` / `foot_*`). `root` remains non-keyable. Torso/head stay existing local-space conventions. Hand/foot depth has no Humanoid v0 child, so pose is a no-op; the value is stored for later Paper Doll. |
| Paper Doll | Future sprite presentation may interpret the same authored angle as projected limb orientation. Do not implement mesh deformation here. |

`sample` still writes only rot/tx/ty. `sample_and_project` is the shared helper (sample + depth + project). Client Character Presentation blends **unprojected** locals and depth, then projects once before `evaluate`. Lab preview uses the same path. There is no hidden Lab-only preview transform.

A later clip version can still add other channels without changing Skeleton Core.

### Interpolation

- **Linear** is the A1 default and the v0 production interpolation for scalar translation channels.
- **Step** is allowed for tests (hold value until the next key).
- Do not implement Bezier or easing in A1.
- **Rotation (A1 / v0):** shortest-angle interpolation between adjacent rotational keys. Wrap the delta into `(-π, π]`, then lerp. Do **not** raw-lerp radians across the `+π` / `-π` boundary.
- Intentional continuous spins **exceeding 180° between adjacent keys** are **outside the initial v0 contract** and may need a later authored-rotation policy.

### Key lookup (v0 recommendation)

A1 clips have two or three keys. **Linear scan from the start of the channel** is enough.

Do not overengineer for large authored clips that do not exist. A later cheap improvement: per-player sample cursor (last key index) stored on playback state, **not** on the shared clip. Binary search is optional after clips are long enough to measure.

### Clip identity (outside the hot path)

**Recommendation:** opaque `ClipId` (`u32` newtype). Debug names live in a side table or `#[cfg(debug_assertions)]` label. Sampling takes `&AnimationClip` (or a handle that yields that). No string hash in `sample`.

A1 may use a single hard-coded clip with no ID table.

## LocalPose semantics

Preserve the Skeleton contract:

- `LocalPose` is the **final** parent-relative pose.
- Animation does not introduce a parallel delta-pose type.
- **Recommendation:** `sample` writes rot/tx/ty directly into a caller-owned `LocalPose` that was just `copy_bind`'d. `DepthPose` is parallel sampled channel state, not a second skeleton pose type. Projection writes the **final** parent-relative translation into that same `LocalPose` before `evaluate`.
- A future A↔B crossfade samples into two caller-owned `LocalPose` buffers, then lerp channels into a third. That seam stays possible because sample does not assume it is the only writer forever. **Do not implement the mixer in A1–A6.**

## Facing / authoring direction

```text
All animation clips are authored for Facing::Right.

Facing::Right → use authored pose as-is
Facing::Left  → presentation-layer horizontal mirror (game adapter and Lab **preview only**)

Front/Back bone identities do not swap under facing changes. Depth values stay authored; mirror preview does not rewrite keys.
```

Plane rotation is radians, counterclockwise, `+Y` up ([`BoneTransform`](../crates/skeleton/src/xform.rs)). For a RIGHT-facing humanoid, a **positive** local rotation on a downward-hanging limb (upper arm / upper leg) moves the distal child toward **+X** (facing-forward / screen-right). A-track debug clips follow that sign (Attack swings toward +X; Move pairs near-leg forward with near-arm back). Canonical clips remain Facing Right. Lab Facing Left is preview-only (no dirty, no history, no serialized left-facing asset). Do not implement Left mirroring inside `purgatory-animation`.

## Root motion policy

`root` is **not** normally animated by locomotion clips.

Character world movement remains driven by authoritative / predicted / interpolated **presentation state**. Idle / Walk / Jump / Attack may animate `pelvis`, torso, and limbs.

Root Motion is **deferred**. Do not implement it in the initial animation system. Revisit only if a future gameplay requirement explicitly demands clip-driven displacement.

## Playback state

Separate immutable clip data from mutable playback.

**A2:** `AnimationPlayer` owns mutable elapsed (implementation-owned accumulation; prefer `f64` internally), `playing`, and `speed`. Callers pass canonical frame `dt` into `advance`; the player applies `scaled_dt = dt × speed`. Do not pre-multiply speed at the call site. `speed == 0` is valid; negative / non-finite `dt` or `speed` are rejected without mutating state.

Conceptual separation:

```text
advance(dt, clip)   → mutates playback state
sample_time(clip)   → read-only projection to clip-local t (Once clamp / Loop rem_euclid)
sample(clip, t, …)  → mutates caller-owned LocalPose (unchanged A1 contract)
```

`sample_time` has no side effects. Exact Loop wrap: duration `1.0` + `advance(1.0)` → sample `t == 0.0`.

Do not store mutable time inside `AnimationClip`. Do not store per-track mix weights in A1–A3.

**A3/A4:** each visible `CharacterPresentation` entry owns an independent `AnimationPlayer`. A4 adds per-entry transition capture + short pose blend on activity change (interruptible from the currently presented pose). Advance each entry **at most once per frame** with canonical `frame_dt`. Local and remote parity: identical path; no per-frame animation network traffic. Gameplay remains authoritative; animation only presents inferred activity.

A2 Stage D proof still resolves one debug `selected_animation_sample_t` (Manual A1 / Playback A2) for the local Stage D overlay only — not for CharacterPresentationSet playback.

`blend_local_poses` lives in `purgatory-animation` (shortest-angle rotation; linear translation; alpha clamped; no per-frame heap). Not a general mixer.

Hard-coded debug clips: A3 Idle/Move, A4 Jump/Fall, A5 Attack/Hurt, plus authored `climb_back.anim` for ClimbBack. LoopPolicy placement may be reassessed before broader clip authoring.

## Presentation time

Animation is client presentation. **Sample on render/presentation time**, not on the 30 Hz authoritative simulation tick.

| Source | Root / body pose | Animation time |
|---|---|---|
| Local player | Predicted tick pose + remainder presentation (existing `frame_local.presented`) | Frame delta (same client frame as skeleton evaluate / draw) |
| Remote player / NPC | Interpolated replica pose (existing interp buffer) | Same frame delta on the observing client |

The animation runtime must not take `is_local`, replica types, or `SimulationTick` as sampling inputs. It receives `t` (or a player that was advanced with `dt`) and a clip.

Debug time scale that already scales `SimulationClock` elapsed does **not** automatically scale animation unless the client chooses to feed scaled `dt` into the player. A2 feeds the raw canonical frame wall delta into `advance` (speed is applied inside the player). Do not couple clip sampling to `World::tick`.

Exact insert point in the client frame: `CharacterPresentationSet::sync(..., frame_dt)` advances per-character players and samples Idle/Move; Stage D may separately consume A1/A2 debug `selected_animation_sample_t`.

## Phase 8 boundary

Phase 8 direction is Character Presentation + Equipment Runtime, with **minimal semantic replication** and **no replicated bone/frame transforms**.

Today `PresentationActivity` is `Idle | Move | Jump | Fall | Attack | Hurt | ClimbBack`. Attack/Hurt are authoritative overlays (not velocity-inferred). ClimbBack selects `PresentationView::Back` and samples `climb_back.anim` (not Idle).

| May exist later on server / wire | Must never be replicated |
|---|---|
| Semantic activity (Idle, Move, Jump, Fall, Attack, Hurt, …) | Bone transforms |
| | Animation frames / keyframe time streams |
| | Skeleton `LocalPose` / `WorldPose` |

Character Presentation owns **semantic state → visual animation**. Animation Runtime owns **clip → LocalPose**. Skeleton owns **hierarchy math**. Keep these separate.

**Attack / Hurt (v1 policy):** they **replace** locomotion presentation (one active clip). No upper/lower-body masking, no additive layers, no region blend in v1.

A1–A6 do **not** implement production combat animation, equipment integration, or remote activity selection. They may note those as future consumers of this contract.

This track does **not** begin Phase **8E** (ContentId → visual attachments). Equipment remains carried, not resolved.

## Blending / transitions

A1–A6: **one active clip**, no mixer.

Forward-compatible seam (not current scope):

- Sample is a pure function of `(clip, t, &mut LocalPose)` after bind copy.
- A future caller can sample clip A and clip B into two buffers and lerp.
- Do not bake “only one Pose can ever contribute” into the clip format.
- Do not add a multi-track mixer, body masking, or a complex state machine in A1–A6.
- Crossfade ownership is unresolved (presentation vs animation crate). Recommendation: a later `mix_local_poses(a, b, alpha, out)` next to the sampler, driven by Character Presentation.

## Performance contracts

Animation must remain suitable for many humanoid presentation instances.

**Invariants:**

1. Clip data is immutable and **shared** (no duplicated clip definition per character).
2. Tracks target dense `BoneIndex`. No string lookup in sampling.
3. No per-sample heap allocation. No `Vec` growth inside `sample`.
4. Caller owns / reuses `LocalPose`, `DepthPose`, and `WorldPose`. Allocate on presentation enter; reuse while the entity remains visible (same pattern as 8D pose buffers).
5. Sampling cost is proportional to **active tracks and keys in the lookup window**, not to unkeyed bones.
6. Evaluate remains `O(bone_count)` in Skeleton Core.
7. Do **not** introduce object pools without measurement.
8. Do not clone `SkeletonDef` or `AnimationClip` per instance.

Record measurements in [`PERFORMANCE_BUDGETS.md`](PERFORMANCE_BUDGETS.md) when a many-instance slice runs. Do not invent player-capacity claims here.

## Animation authoring

A1–A6 do **not** include an editor. **A7.0** adds the standalone Animation Lab (`tools/animation_lab`, ADR-0058). **A7.1** adds authoring QoL and `depth_angle` on the shared runtime path.

- A1/A2: hard-coded Rust / dev data.
- A3–A6 proof clips: A6 v1 `.anim` text under `content/shared/animations/dev/`. `parse_animation_asset_v1` / `serialize_animation_asset_v1` are inverses for structured `clip` + `markers` (comments/whitespace are not preserved). Unknown tokens fail parse; do not silently drop schema. Optional `depth` keys are A7.1; omitted = `0`.
- Runtime still `include_str!`s those files into `OnceLock` clips. Saving in the lab updates the file; the running game sees it after a **client rebuild**. No hot-reload architecture.
- Authoring policy is not the same as schema capability: `rot`/`tx`/`ty`/`depth` exist on the grammar; the lab refuses to **add** keys on `root`, exposes **add** `tx`/`ty` only on `pelvis` / `torso` / `head`, and exposes **add** `depth` only on limb bones. Existing out-of-policy keys remain visible so data is not dropped.
- Markers are data only until Notify Runtime. A7.0 displays them on a reserved timeline marker row and may add/delete/move them.
- Command/history Undo/Redo is A7.0. Asset mutations (including batch paste/move/delete and depth edits) are one history entry each. Selection, clipboard contents, snap settings, mirror preview, transition preview, playhead, and viewport camera are editor state: no dirty, no history, no serialization.
- A7.1 lab: copy/paste (Ctrl+C/V) and Copy Pose / Paste Pose; multi-select (Ctrl, Shift range, box select); editor snapping (grid / existing key / marker; 0.01s storage grid unchanged); Facing Left preview; A→B transition preview using `blend_local_poses` + `blend_depth_poses` (not a state machine).
- A7.2 is curves. Notify Runtime is later.
- The lab bottom **Timeline / Dope Sheet** is a first-class panel: time 0→duration, authored bone/channel rows, key diamonds, playhead, click/scrub, select/move/add/delete. It must allocate layout height (not paint into an unallocated rect).

Authoring labels (`head`, `hand_front`, …) may resolve to `BoneIndex` at load/bind time (same idea as 8D `BoneTargetMap`). The sampler never sees those strings.

## Rig / animation policy

Skeleton math supports translation + rotation on every bone. That does **not** mean every bone should freely translate in normal rigid-cutout animation.

| Bones | Normal clip policy |
|---|---|
| `wrist` / `elbow` / `knee` / `ankle` (`hand_*`, `lower_arm_*`, `lower_leg_*`, `foot_*`) | Preserve rigid limb connectivity (child rest offset). Independent hand/foot motion is **rotation about the existing joint**. |
| `pelvis` / `torso` / `head` | Small translations are allowed where visually appropriate (bob, weight shift, look). |
| `root` | Not a locomotion clip channel. Adapter-placed. |

Explicit translation is allowed only where the authored clip intends it (for example a designed foot plant/slide). This is animation/rig policy, not a Skeleton Core restriction. See [`HUMANOID_RIG_SPEC.md`](HUMANOID_RIG_SPEC.md).

## A1 acceptance contract

A1 is extremely small. It proves the pipeline, not a character.

**Must prove:**

1. An `AnimationClip` can target a bone by dense `BoneIndex`.
2. Sampling at arbitrary `t` produces the expected local bone transform.
3. Bind Pose is preserved for unkeyed bones and unkeyed channels.
4. Linear / shortest-angle interpolation between keys is correct (and Step if used in a test).
5. The resulting `LocalPose` passes through existing `evaluate`.
6. The existing placeholder visual follows the animated bone.
7. No root / world gameplay movement is introduced (`root` stays adapter-placed; presented entity pose unchanged).
8. Sampling reuses a caller-owned `LocalPose` (stable length + backing pointer across successful samples). That proves buffer reuse for this API; it does **not** prove that no allocation can occur anywhere in the process.
9. On `sample` error, `LocalPose` is left unchanged.

**Must not include:** playback clock, looping wrap, activity mapping, Walk, equipment, remote clip selection, mixer, new renderer, protocol, Skeleton Def changes.

### Recommended A1 bone: `head`

**Use `head` (index 3).** Rotation-only clip. Two or three keys. Visible nod/turn on the P3.1 head placeholder.

Why `head` rather than `hand_front` or `pelvis`:

- Clearly visible (doubled head panel) without looking like locomotion or combat.
- Rotation about the existing joint does not open a rigid-cutout gap (unlike translating `hand_front` / `foot_front`).
- Independent of S2 front-arm / front-leg proof enums, so A1 is not confused with those diagnostics.
- Unkeyed limbs stay in bind, which is the bind-preservation proof.
- Does not write `root` or `pelvis`, so it cannot be mistaken for a second movement system.

`hand_front` remains a valid alternative (rotation-only, already proven as Hand Independent) but overlaps the proof-pose UI and is a smaller visual. `pelvis` would read as locomotion bob — reject for A1.

**Visual:** existing S2/P4 placeholder on the local presented pose. Debug overlay may expose sample `t` (slider or stepped values). Do not auto-advance time in A1 (that is A2). When A1 demo sampling is enabled, do not also apply front-arm/leg proof writes to the same `LocalPose`.

## Unresolved decisions

Do not silently freeze these in A1 unless A1 cannot ship without a local choice. Record the A1 choice in the A1 report and, if it is architectural, an ADR or an update here.

| # | Decision | Recommendation | Freeze in A1? |
|---|---|---|---|
| 1 | Runtime ownership: new crate vs client module | `purgatory-animation` (`crates/animation`), depends only on `purgatory-skeleton`. Client presentation selects clips. | **A1: crate** |
| 2 | Exact Clip/Track/Key Rust types | Immutable contiguous tracks; `Box<[T]>` allowed as implementation, not a public freeze. Independent optional channels on one `BoneTrack` per keyed bone. | A1 may use rotation-only without freezing serde |
| 3 | Key lookup | A1: linear scan from start (2–3 keys). Later: optional per-player cursor on playback state | No binary search required |
| 4 | Independent channels vs one combined bone track | Independent optional channels on one `BoneTrack` per keyed bone | A1 rotation-only is compatible with either; prefer independent |
| 5 | Clip ID / name | Opaque `ClipId`; names off the hot path. A1: one hard-coded clip | No registry required |
| 6 | Render-frame scheduling | After finalized presentation pose, same frame as current skeleton evaluate, before draw | A1 may hook S2 `HumanoidDebug` only |
| 7 | Future crossfade ownership | `mix_local_poses` beside the sampler; Character Presentation drives alpha | Not A1 |
| 8 | Authored asset format | Keep runtime vs storage separable. `/assets/dev` later. No JSON/binary freeze | Not A1 |
| 9 | Sample into `LocalPose` vs a temp pose | Write directly into caller-owned `LocalPose` after `copy_bind` | A1 should follow this so a later mixer stays possible |

No ADR is opened by A0. Open an ADR when crate ownership or clip-on-wire (forbidden) would change architecture.

## Explicitly out of scope (A0 and A1–A6)

Sprites, atlases, Character Lab, Hub-embedded animation editors, IK, mesh deformation, physics bones, Slot/draw-order tracks, events, root motion, Bezier, additive animation, body masking, complex state machines, equipment attachment animation, production combat clips, server-side animation, protocol bone/facing messages. **8E is complete; do not modify/reopen/extend it** from this track.

A7.0 is a **launched** lab (not in this out-of-scope list). A7.1 is complete. A7.2 (curves) and Notify Runtime remain out until instructed.

## Relationship to the skeleton roadmap

[`CHARACTER_SKELETON_ROADMAP.md`](CHARACTER_SKELETON_ROADMAP.md) stages **S5–S7** were a placeholder “clips write LocalPose / Idle / Walk” sequence **inside** the skeleton track. **This A-track is the animation implementation sequence.** Skeleton S1–S4 / S8 remain skeleton/presentation work (math, debug draw, entity bind, slots, N instances). Do not implement animation as a module of `purgatory-skeleton`.
