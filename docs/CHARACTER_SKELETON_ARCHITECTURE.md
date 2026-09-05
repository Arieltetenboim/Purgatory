# Character skeleton architecture

Status: **S1 complete.** S2 full-rig debug draw and renderer **R1** are **owner-accepted**. **P1–P4** placeholders are implemented. Baseline character presentation scale is **1.15×** (2× is diagnostic preview only). **P4.2** is a RIGHT-facing 3/4 directional-read / depth-cue pass (owner visual gate). Skeleton Core is not animation and not Slots. Animation contracts: [`CHARACTER_ANIMATION_ARCHITECTURE.md`](CHARACTER_ANIMATION_ARCHITECTURE.md) (**A0** docs; A1 not started). Client debug overlay has a dedicated Skeleton tab (see [`CHARACTER_SKELETON_ROADMAP.md`](CHARACTER_SKELETON_ROADMAP.md)). Not a gameplay phase. Root `PHASE` is unchanged.

This document is the structural contract for a client-side humanoid skeleton presentation foundation. Companion documents: [`HUMANOID_RIG_SPEC.md`](HUMANOID_RIG_SPEC.md) (bone/slot geometry), [`CHARACTER_SKELETON_ROADMAP.md`](CHARACTER_SKELETON_ROADMAP.md) (implementation stages), [`CHARACTER_ANIMATION_ARCHITECTURE.md`](CHARACTER_ANIMATION_ARCHITECTURE.md) (clip sampling layer above this core).

Living budgets stay in [`PERFORMANCE_BUDGETS.md`](PERFORMANCE_BUDGETS.md). ADRs stay in [`DECISIONS.md`](DECISIONS.md). Do not copy volatile numbers here.

## Purpose

PURGATORY will present modular 2D humanoids. The runtime model is:

```text
shared immutable Skeleton Definition
    → local Pose (per instance, bone-indexed)
    → world Pose (evaluated)
    → optional debug / later attachment presentation
```

The **core contract** of skeleton math is only:

```text
Definition + local Pose  →  world Pose
```

That core must have **no knowledge** of Player, NPC, velocity, animation clips, networking, replicas, FOOTNOTE, or gameplay identity.

A later **client presentation adapter** (outside the core) may:

- supply a root transform from an already-finalized presentation pose
- choose whether to **evaluate** a pose this frame
- choose whether to **debug-draw** that pose

Evaluation visibility and debug-draw visibility are **separate**. Debug overlay state (ADR-0016) must never become a runtime animation dependency.

## What exists today (constraints)

- Server/simulation owns AABB `Transform` and FOOTNOTE / NPC locomotion. Bones do not exist there. They must not be added.
- Replication carries pose + velocity ([`crates/protocol/src/snapshot.rs`](../crates/protocol/src/snapshot.rs)). No facing bit, no bone channels.
- Local player draw uses predicted + remainder presentation ([`apps/client/src/local_presentation.rs`](../apps/client/src/local_presentation.rs)). Remotes and NPCs use interpolation ([`apps/client/src/interp.rs`](../apps/client/src/interp.rs)).
- The renderer uploads colored AABBs / triangles (`DrawQuad`). Debug “lines” are thin quads ([`apps/client/src/debug/viz.rs`](../apps/client/src/debug/viz.rs)).
- `MAX_QUADS = 192` ([`apps/client/src/renderer/gpu.rs`](../apps/client/src/renderer/gpu.rs)). Excess is truncated. Early debug visualization targets **one** local/selected humanoid. Do **not** raise `MAX_QUADS` to show many debug skeletons.
- World `+Y` is up. Side-scroller. Authored character facing is **right**.
- `Graphic/` is not a runtime content pipeline ([`CONTENT_PIPELINE.md`](CONTENT_PIPELINE.md)). This foundation does not import sheets, atlases, or Character Lab.

## Ownership

```text
purgatory-skeleton (crates/skeleton) — S1
    Definition, local Pose, world Pose, hierarchy eval, slot index tables
    zero crate dependencies; tests with no wgpu / egui / Quinn / simulation World tick

client presentation adapter (apps/client only) — not in S1
    root transform from presented entity pose
    eval/draw gating
    debug primitive emission into the existing quad path

simulation / server / protocol
    unaware of bones

debug overlay (egui)
    may toggle debug-draw
    must not gate evaluation or clip selection
```

S1 lives in `purgatory-skeleton` so math tests do not compile the client GPU/network graph. The client does not depend on this crate until S2. `purgatory-simulation`, `purgatory-server`, `purgatory-protocol`, `purgatory-content`, `purgatory-common`, and `purgatory-dev-runtime` must not depend on it.

## Identifiers

Bones and slots use **stable dense integer indices** (`0 .. n-1` within a definition). S1 representation: `BoneIndex` / `SlotIndex` as `u8` newtypes. Humanoid v0 names are constants on that definition, **not** a closed bone enum.

- Names in [`HUMANOID_RIG_SPEC.md`](HUMANOID_RIG_SPEC.md) are **authoring labels** for humans and tests.
- Do **not** freeze `BoneId` / `SlotId` as closed Rust enums. Adding a bone must not require a public enum churn as the first design choice.
- Hot paths look up by index. No string hash maps in evaluate / compose.

A definition owns:

- bone count, parent index per bone
- rest/bind local transform per bone (translation + plane rotation)
- slot count, slot → bone index + rest local **translation + rotation**
- default draw-order as a permutation of slot indices (not hierarchy order)

**Exactly one root:** bone index 0 has no parent. Every bone index `> 0` has exactly one parent. Parent index is strictly less than child index (parent-before-child forward evaluate).

Bind and slot-rest transforms must be **finite** (no NaN/Infinity) at definition construction.

**Bone vs slot (contract):** if a part needs independent translation or rotation in animation, it is a bone. If it only needs to be a replaceable visual that inherits a bone, it is a slot. Slots are not per-pose channels; do not animate by rewriting slot offsets. Shoe visuals attach to Foot bones; hair to Head. See [`HUMANOID_RIG_SPEC.md`](HUMANOID_RIG_SPEC.md).

One shared immutable definition is referenced by many poses. Do not clone the definition per character.

Skeleton bind/rest dimensions are presentation units. They are **not** coupled to Player/NPC collision AABBs.

## Data flow

```text
Definition (immutable, shared)
LocalPose[bone_index]  = final local translation + plane rotation relative to parent
                ↓  evaluate(def, local, out_world)  // counts must match
WorldPose[bone_index]  = composed 2D transform in the pose's space
```

`LocalPose` is **not** an animation-delta buffer. Bind pose is initialized by copying the definition's bind locals. Animation sampling (A-track) writes final locals into this buffer; skeleton core does not know about clips, additive layers, velocity, or gameplay. See [`CHARACTER_ANIMATION_ARCHITECTURE.md`](CHARACTER_ANIMATION_ARCHITECTURE.md).

`evaluate` checks that definition, local, and world **bone counts match**, then runs an `O(bone_count)` write into the caller-owned world buffer. It does not use accidental short-slice indexing. Mismatched counts return `PoseError` and do not evaluate.

The hot path reads `def`/`local` and writes `world`. It does not allocate, grow buffers, or build temporary collections. Pose buffers are allocated by the caller (`LocalPose::from_bind`, `WorldPose::new`) before evaluate. A test that bone_count is unchanged after a second evaluate shows **buffer reuse**, not a global allocator proof.

Lifecycle allocation (when a pose buffer is created or released) is **not** an instance pool. It remains measurable and implementation-dependent.

## Visibility (two flags)

| Concern | Meaning | Must not depend on |
|---|---|---|
| **Evaluate** | Whether `local → world` runs this frame | Debug overlay, gizmo checkboxes |
| **Debug-draw** | Whether lines/joints/axes/rects are submitted to the renderer | Animation clip choice, evaluate itself |

A pose may be evaluated with debug-draw off (future animation still needs world joints). Debug-draw may be off while evaluate is on. Debug-draw on with evaluate off is invalid (nothing to draw). Overlay `~` / FOOTNOTE gizmos may control **debug-draw only**.

S3 binds a root to a presented entity. That binding is adapter code, not core math. Phase **8D** adds a client `CharacterPresentationState` collection that evaluates Humanoid v0 bind pose for visible player replicas. Phase **8E** resolves equipped ContentIds into bound attachments and draws debug placeholders from that same set (local and remote). It is not S3 debug-draw unification and does not start clip sampling, Back view, or ART. Animation A0 is docs only; A1 is not started.

## Invariants

1. Simulation and the server never see bones.
2. No protocol change for this foundation.
3. Core math knows only Definition + local Pose → world Pose.
4. Front/Back limb indices are **canonical RIGHT-facing near/far** ([`HUMANOID_RIG_SPEC.md`](HUMANOID_RIG_SPEC.md)). Mirroring is a root/scale transform in the adapter. It must **not** swap Front/Back identities.
5. Draw order is independent of parent hierarchy. v0 uses a fixed default list. Dynamic/animated draw order is deferred.
6. Slots are attachment **metadata** (bone index + rest local translation + rotation). They are not pose channels. No textures, UVs, crops, or atlases in this foundation.
7. Evaluate operates only on caller-owned preallocated storage. It contains no heap-building operations or temporary collections.
8. No string lookup on the evaluate hot path.
9. Definitions are shared and immutable after construction.
10. `+Y` up. Each bone local pose is translation plus one plane rotation. Rotation-only pose is insufficient (e.g. independent foot lift/slide). Uniform scale is not a v0 clip channel; the adapter may apply root `scale.x` for facing.
11. Exactly one root at index 0; parent index strictly less than child; bind and slot-rest values are finite.

## Performance constraints

- Evaluate is `O(bone_count)` per evaluated pose, index walks only.
- Early stages evaluate **one** humanoid (roadmap S2). Many-character cost is S8, after a visible one-character path.
- Debug-draw for S2–S4 must fit the **existing** `MAX_QUADS` budget for **one** skeleton plus the current scene. If one debug skeleton plus world quads would truncate, reduce debug primitive count (fewer axis ticks, thinner set) — do not raise the cap to visualize a crowd.
- Do not add work in core for characters the adapter did not ask to evaluate.
- Record measurements in [`PERFORMANCE_BUDGETS.md`](PERFORMANCE_BUDGETS.md) when S8 runs; do not invent player-capacity claims here.

## Test strategy (core)

No pixel-perfect GUI tests. No wgpu tests required for S1.

Minimum:

- parent composition: child world = parent world ∘ local
- bind/rest pose is deterministic for a fixed definition
- index range checks; out-of-range is a definition/pose error, not a panic on arbitrary bytes in the game sense — tests may assert errors
- slot index resolves to the specified bone; names are test-only labels
- evaluate into a reused caller-owned buffer (same world buffer rewritten; counts must match)
- mirroring adapter tests (when adapter exists): Front index still Front after root scale `-X`
- draw-order list is a permutation of slots, not parent order

S3+ adapter tests (client): bind to a presented root without ticking `World` bones; Leave/generation change does not use a stale pose (strategy implementation-dependent).

## Relationship to current presentation

S2 evaluates **one** Humanoid v0 on the **local presented pose** (same `frame_local.presented` used by the player AABB). The adapter maps AABB center → skeleton root at the body feet (`center.y - PLAYER_HALF_EXTENTS[1]`). That mapping is presentation-only; skeleton rest proportions stay independent of collision.

Evaluate runs whenever that presented pose exists. Debug-draw is a separate client flag (`show_skeleton`); it is not an animation eligibility contract. Overlay `~` is not required to evaluate.

**Stage C draw** (owner-confirmed 2026-09-02): Stage A/B plus front arm. 10 joints and 9 parent–child connections.

**Stage D draw** (owner-confirmed 2026-09-02): all 16 Humanoid v0 bones (16 joints + 15 parent–child connections). No slots, axes, or placeholders. Back limbs emit first so they read as the farther layer; Front/Back identities are unchanged. Front-leg and front-arm proofs remain Off by default; no back-limb proof modes. Baseline **1.15×** character presentation (2× diagnostic preview) scales draw offsets about the skeleton root; it does not modify bind data or the simulation AABB. Joint markers are small opaque circular dots.

**R1** (renderer proof; owner-accepted): `DrawQuad::oriented` on the existing primitive pass.

**P1** (front-arm placeholders; Hand Independent is rotation about the wrist): three bone-attached panels (`upper_arm_front`, `lower_arm_front`, `hand_front`). Outgoing length from child rest, not parent-relative local. Hand visual pivots at the wrist and extends away. Skeleton joints are small opaque dots underneath. Not Slots, not S3.

**P2** (front-leg placeholders; owner-accepted): `upper_leg_front`, `lower_leg_front`, `foot_front`. Same child-span rule. `foot_front` bind is the vertical knee→ankle span; forward foot length is the ankle-pivoted placeholder. Foot Independent is rotation about the ankle (no shin→ankle gap). Back-leg placeholders (`upper_leg_back`, `lower_leg_back`, `foot_back`) use the same rule and darker colors; no back-leg proofs. Not Slots, not S3.

**P3** (torso + head): inverted-trapezoid chest from hip Y to head joint Y (rig geometry, not `torso` parent-relative local); head panel pivoted at `head` extending +Y. **P3.1:** head visual is 2× the prior panel. **P4:** complete placeholder humanoid including back arm/hand; draw order back arm → back leg → torso → front leg → head → front arm. **P4.1:** canonical RIGHT-facing 3/4 bind X and torso-corner skew (neutral hang; no stance rotations). **P4.2:** directional read and near/far value cues (inward back hip, 3/4 head silhouette, torso far-half shade). Baseline presentation **1.15×** about planted feet; 2× is diagnostic only. Legacy cyan AABB hide/show (`show_local_player_quad`); the AABB itself stays simulation size. Not Slots, not S3, not animation.

Animation/rig policy (not a `purgatory-skeleton` math restriction): rigid cutout limbs keep connecting-joint offsets; independent proofs rotate about the existing joint. See [`HUMANOID_RIG_SPEC.md`](HUMANOID_RIG_SPEC.md).

S3 generalizes to remotes/NPCs and lifecycle. The local cyan AABB can be hidden from the Skeleton tab (`Show legacy blue AABB`); whether it stays as a default beside the skeleton is still a product choice.

## Explicitly out of scope (all current roadmap stages)

Sprites, textures, atlases, Character Lab, Hub editors, IK, mesh deformation, physics bones, socket gameplay, inventory, recolor, protocol facing, server-side animation.

## Unresolved decisions

Do not silently freeze these in a later implementation slice. Record the choice in an ADR or an update to this document.

1. **Module home** — **`purgatory-skeleton`.** S2: `purgatory-client` depends on it for debug presentation only.
2. **Integer width** — S1 uses `u8` newtypes. Still not a closed bone enum.
3. **Local transform packing** — S1: `BoneTransform { translation: [f32; 2], rotation: f32 }` (radians, CCW). Not matrices. Not unresolved for v0.
4. **Root facing** — adapter may infer facing from presented `vx` locally, or a future protocol facing bit (owner decision; protocol change is not authorized here).
5. **Evaluate set in S8** — all replica-known humanoids vs camera frustum vs “selected + local only”. Independent of debug-draw.
6. **AABB vs skeleton** — overlay skeleton on existing player/NPC quads vs replace those quads when skeleton debug-draw is on. Bind rest is **not** sized from those AABBs.
7. **Authored definition storage** — S1: hard-coded Humanoid v0 in Rust. Later JSON under `/assets/dev` remains open. `Graphic/` stays unused by this foundation.
8. **Weapon parent** — v0 is a **slot** on `hand_front`, not a dedicated weapon bone. Hand bones are part of Humanoid v0. A weapon *bone* (transform independent of the hand) remains deferred. See [`HUMANOID_RIG_SPEC.md`](HUMANOID_RIG_SPEC.md).
9. **One definition vs scaled instances** — player vs NPC AABB sizes are a later adapter concern. Core must not know Player/NPC.
10. **Line primitive / oriented quads** — S2 debug bones stay thin AABB stamps. **R1** adds CPU-generated oriented solid quads (`DrawQuad::oriented`) on the existing primitive pass (same shader, batch, `MAX_QUADS`). Convex four-corner solids (`DrawQuad::convex`) are the same primitive for placeholder trapezoids. That is the attachment foundation for later atlas sprites (UVs on the same corner winding). A dedicated line pipeline is still not required. R1 is a renderer proof, not S3 and not Slot rendering.
