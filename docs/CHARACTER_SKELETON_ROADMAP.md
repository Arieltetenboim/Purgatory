# Character skeleton roadmap

Status: **S2 full-rig debug draw owner-accepted (2026-09-02).** Renderer **R1** owner-accepted. **P1–P4** placeholders implemented. Baseline character presentation **1.15×** (2× diagnostic preview). **P4.2** RIGHT-facing 3/4 directional-read / depth-cue pass awaits owner visual confirmation. Joints are small opaque circular dots. Do **not** start S3 or Slot construction. Animation implementation is the **A-track** ([`CHARACTER_ANIMATION_ROADMAP.md`](CHARACTER_ANIMATION_ROADMAP.md)); A0 is docs, A1 is not started. Root `PHASE` is unchanged.

Architecture: [`CHARACTER_SKELETON_ARCHITECTURE.md`](CHARACTER_SKELETON_ARCHITECTURE.md). Rig: [`HUMANOID_RIG_SPEC.md`](HUMANOID_RIG_SPEC.md). Animation: [`CHARACTER_ANIMATION_ARCHITECTURE.md`](CHARACTER_ANIMATION_ARCHITECTURE.md).

Each stage is a stop boundary. Do not pre-build later stages.

## Out of all listed stages

Art, sprites, textures, atlases, paper-doll crops, Character Lab / Hub editors, IK, mesh deformation, physics bones, socket gameplay, inventory, recolor, protocol bone or facing messages, server-side animation.

## S2 visual failure (why the full-rig draw was rejected)

The first S2 draw emitted ~81 AABB quads (16 joints, parent–child bars, local axes, 17 slots, torso placeholder) on a figure ~0.96 wu tall (~49 px at FOOTNOTE camera height 14 wu, 1280×720). `segment_quad` turns a parent–child pair into a **filled AABB**, not a line. Proof pose was on by default. Overlaying that many primitives on a ~62 px player AABB made joint coordinates unreadable. Adapter mapping (root at AABB feet) and 16-bone evaluate were not independently validated on screen.

## Diagnostic draw stages (inside S2; owner-accepted)

| Stage | Draw | Status |
|---|---|---|
| A | Exactly **7** primitives: joints `root`, `pelvis`, `torso`, `head` + 3 parent–child connections. No slots, axes, placeholder, proof pose, or limbs. | **owner-confirmed 2026-09-02** |
| B | Stage A plus front leg (`upper_leg_front` → `lower_leg_front` → `foot_front`). **13** primitives (7 joints + 6 connections). Off-by-default front-leg proof (`Foot independent` / `Shin carries foot`). | **owner-confirmed 2026-09-02** |
| C | Stage B plus front arm (`upper_arm_front` → `lower_arm_front` → `hand_front`). Logical set: **10** joints + **9** connections. Off-by-default front-arm proof (`Hand independent` / `Forearm carries hand`). Diagonal connections stamp multiple thin AABBs (`DrawQuad` cannot rotate; a single parent–child AABB would fill the bounding box). | **owner-confirmed 2026-09-02** |
| D | Remaining back limb chains. Logical set: **16** joints + **15** connections. No slots, axes, placeholders, or extra proof modes. Back limbs emit first (farther layer). Front/back colors are distinct; hierarchy is unchanged. | **owner-confirmed 2026-09-02** |

Full 16-bone pose is still **evaluated** every presented frame. Stage D draws every bone. Proof defaults to **Off**. `MAX_QUADS` stays 192. Topology is unchanged.

**R1** (renderer proof, owner-accepted): oriented solid quads on the existing primitive pass.

**P1** (front-arm placeholders; Hand Independent corrected to rotation-only about the wrist): three bone-attached panels — `upper_arm_front` (shoulder→elbow via `lower_arm_front` rest), `lower_arm_front` (elbow→wrist via `hand_front` rest), `hand_front` (small placeholder pivoted at the wrist, extending away). Translating `hand_front` is not this proof: it opened a forearm→wrist gap on the rigid cutout.

**P2** (front-leg placeholders; owner-accepted 2026-09-02): `upper_leg_front` (hip→knee via `lower_leg_front` rest), `lower_leg_front` (knee→ankle via vertical `foot_front` rest), `foot_front` (placeholder pivoted at the ankle, extending canonical +X). Bind-pose joint semantics corrected so the shin is vertical. Foot Independent is rotation-about-ankle only. Shin carries foot is unchanged. Back-leg placeholders (`upper_leg_back` / `lower_leg_back` / `foot_back`) use the same child-span rule and darker colors; no back-leg proof modes.

**P3** (torso + head): pale-orange torso panel attached to `torso` (hip→head-joint span from rig geometry, not `torso` parent-relative local); cyan head panel pivoted at `head` extending +Y. Draw order: **back leg** → torso → front leg → head → front arm. Back-leg placeholders use the same child-span rule as P2 (darker greens). Not Slots, not back arm, not a full character.

**P3.1** (2026-09-02): head placeholder visual is 2× the prior 0.14×0.16 panel; bind `head` is unchanged. Legacy cyan AABB hide/show remains on the Skeleton tab (`Show legacy blue AABB`). Not animation, not Slots.

**Proportion calibration** (2026-09-02): client presentation scale moved to **1.15×** about planted feet (2× debug preview is diagnostic only). Simulation AABB / collision / world position unchanged.

**P4** (complete placeholder humanoid): back arm/hand added with the same child-span / wrist-pivot rules as the front chain; draw order back arm → back leg → torso → front leg → head → front arm.

**P4.1** (canonical RIGHT-facing 3/4): bind X retuned so the silhouette reads facing screen-right without a front-on chest. Head `+0.05` X. Shoulders `upper_arm_front +0.12` / `upper_arm_back −0.07`. Hips: near/front `−0.06`, far/back `+0.09` (front.x < back.x; |sep| 0.15; knee/ankle stacked). Front/back remain near/far draw identities, not left/right. Torso placeholder uses a skewed inverted trapezoid (front +X more exposed). No stance rotations. Limb lengths unchanged.

**P4.2** (directional read / depth cues): back hip pulled inward to `+0.04` (still `front.x < back.x`). Front shoulder `+0.08`, back shoulder `−0.10`. Head lead `+0.07` with a 3/4 convex head silhouette. Torso far-half is a darker geometric split; front limbs brighter than back. Back forearm has a mild `+0.50` rad (~29°) elbow bend so the far hand peeks toward +X (RIGHT), still behind the torso. Neutral hang; no Idle pose. Awaits owner visual confirmation.

Client debug overlay has a dedicated **Skeleton** tab (View / Inspect / Proof). **1.15×** is the baseline character presentation size about the skeleton root / feet. 2× Debug Preview does not write bind data, Local/World pose, or the simulation AABB.

**Do not start S3** until instructed.

Front/back leg bind locals were corrected (2026-09-02) so hip / knee / ankle origins match the child-span rule: small hip placement, vertical thigh and shin in bind, forward foot length on the placeholder. Arm rest was **not** retuned for Stage C or D.

## Stages

### S0 — Documents

The three design files exist.

### S1 — Standalone skeleton math and data model — **complete**

- Crate: `purgatory-skeleton` (`crates/skeleton`), zero dependencies.
- Definition + local Pose → world Pose.
- Dense integer bone/slot indices; shared immutable definition; Humanoid v0 topology frozen.
- Evaluate into caller-owned buffers; no heap-building in evaluate.
- Tests only (no renderer, no entities, no Player/NPC types in core).
- Bind-world regression: `evaluate(from_bind)` matches compose of current `BIND_*` locals. Topology (parent indices) is frozen. Rest translations are **tunable** (not an architecture freeze).

Stop. No debug draw. No replica binding. No instance pool.

### S2 — One humanoid debug presentation — **owner-accepted 2026-09-02**

What exists and remains:

- `purgatory-client` depends on `purgatory-skeleton`. Math stays in the crate.
- One Humanoid v0 evaluated at the local presented pose (AABB feet). Not replica Enter / NPC lifecycle.
- Evaluate vs debug-draw are separate. Overlay checkbox can hide draw; eval still runs with a presented pose.

Accepted *draw*: Stages A–D (16-bone debug rig). R1 is a **renderer** proof on top of that draw (oriented forearm panel); it does not reopen S2.

Rejected from this stage: full-rig AABB bars, local axes, slot markers, torso placeholder, default-on proof pose.

Stop. No clips. No general entity skeleton lifecycle. **Do not start S3.**

### S3 — Entity / presentation binding

- Adapter binds the standalone pose root to a presented humanoid (local presentation pose first; remotes later if needed).
- Core still has no replica/network types.
- Evaluate vs debug-draw remain separate flags.
- Generation change / Leave must not present a stale skeleton (allocation strategy still implementation-owned).

Stop. No attachment art. Empty slots need not be marked yet.

### S4 — Empty slots / debug markers

- Draw slot markers from slot index → bone + local offset.
- No visual assets. Markers prove Front/Back independence and draw-order data.

Stop. No clips.

### S5–S7 — Animation (superseded as implementation sequence)

Clip sampling, Idle, and Walk are **not** built as skeleton-crate stages. The implementation sequence is **A1–A6** in [`CHARACTER_ANIMATION_ROADMAP.md`](CHARACTER_ANIMATION_ROADMAP.md). Skeleton Core remains Definition + local Pose → world Pose. Do not start A1 from this document.

### S8 — Multiple humanoids / lifecycle / performance validation

- More than one evaluated humanoid if the adapter asks.
- Measure: evaluate cost, allocation, `MAX_QUADS` truncation with debug-draw **off** for extras (debug-draw still defaults to one selected/local unless explicitly expanded).
- Lifecycle allocation (pool vs not) chosen from measurement, not from this document.
- Record numbers in [`PERFORMANCE_BUDGETS.md`](PERFORMANCE_BUDGETS.md). No player-capacity claims.

## Suggested later (not scheduled here)

Registered atlas / paper-doll attachments consuming slot transforms; line renderer; protocol facing; Hub inspection. Each needs its own instruction.

## Dependency sketch

```text
S0 docs
S1 math/tests
S2 one debug humanoid (fixed root) — Stages A–D + R1 + P1–P4; P4.2 3/4 RIGHT; 1.15× presentation
S3 presentation binding
S4 slot markers
S8 N instances + measurement

Animation A0–A6 (separate track; clips write LocalPose above this core)
```

S8 requires S3. S4 can follow S2 or S3; prefer after S3 so markers sit on a bound character. Idle/Walk clips are A3–A6, not S6/S7.

## Quality gate when implementation starts

Use `./scripts/check.ps1`. Compiling is not completion. S2 Stages A–D, R1, and P1–P4 are implemented. P4.2 awaits owner visual confirmation of the RIGHT-facing 3/4 directional read. Baseline presentation scale is 1.15×. Do not start S3 until instructed. Do not start animation A1 until instructed ([`CHARACTER_ANIMATION_ROADMAP.md`](CHARACTER_ANIMATION_ROADMAP.md)).
