# Humanoid rig specification

Status: **Humanoid v0 bone topology frozen for S1.** Implemented as test/dev data in `purgatory-skeleton`. Not a gameplay/collision contract.

This is the geometric contract for Humanoid v0. Architecture: [`CHARACTER_SKELETON_ARCHITECTURE.md`](CHARACTER_SKELETON_ARCHITECTURE.md). Stages: [`CHARACTER_SKELETON_ROADMAP.md`](CHARACTER_SKELETON_ROADMAP.md). Animation clips: [`CHARACTER_ANIMATION_ARCHITECTURE.md`](CHARACTER_ANIMATION_ARCHITECTURE.md).

Identifiers are **dense indices**. Labels below are documentation names. Implementation uses `u8` newtypes (`BoneIndex`, `SlotIndex`). Do **not** treat this table as a requirement to ship a closed Rust `enum` for `BoneId` / `SlotId`.

**Frozen:** the 16-bone parent topology and index identities below. **Not frozen:** exact rest-pose measurements (tunable before art integration). **Accepted for v0:** the 17-slot table. Additional future visual slots may be added without redefining the humanoid skeleton. Slot count is independent of bone count.

## Bone vs Slot

| Need | Use |
|---|---|
| Independent **translation or rotation** during animation | **Bone** (transform node in the hierarchy) |
| Replaceable visual / equipment that **inherits** a bone’s motion | **Slot** (metadata: bone index + rest local offset) |

Slots are not pose channels. A slot’s offset lives on the definition. Animating a body part by moving a slot would invent a second transform graph; do not do that.

Hair, helmet, face, clothing, shoes, gloves/hands, and future equipment are **primarily slots**. A shoe slot attaches to a **Foot** bone. A hand/glove slot attaches to a **Hand** bone. The weapon slot attaches to **HandFront**. Hair slots attach to **Head**. Clothing slots attach to the bones whose motion they must inherit.

**Slot count is intentionally independent of bone count.** Hierarchy and draw order remain independent.

## Coordinate system

- World and pose space: `+X` right, `+Y` up (same as FOOTNOTE / client camera).
- Each bone’s local pose is a 2D rigid transform: **translation + one plane rotation**. The math layer is not rotation-only (see animation policy below).
- Each bone’s **origin is the joint**. A limb visual associated with that bone generally spans from that bone toward its **child** bone. The bone’s own local translation describes its position relative to its **parent**, not its outgoing visual length.
- Example: `lower_arm_front` world translation is the elbow. Rest local `(0, -0.22)` is the shoulder→elbow span (offset from `upper_arm_front`). Elbow→wrist is `hand_front` rest local `(0, -0.10)`. Debug limb panels use that child span only; it is not an automatic limb-meshing system.
- Leg example: `upper_leg_front` origin is the **hip** (small pelvis offset). `lower_leg_front` origin is the **knee**; its rest local is the hip→knee thigh span. `foot_front` origin is the **ankle**; its rest local is the knee→ankle shin span and is approximately vertical in bind. Forward shoe/foot length is a **visual** offset from the ankle, not `foot_front` bind translation.
- Scale is not a v0 clip channel. The presentation adapter may apply root `scale.x = -1` for facing. A7.1 `depth_angle` is an **animation** channel: after sampling, `apply_depth_projection` shortens each child’s parent-relative translation by `cos(|θ|)` with a small floor. It is not BoneTransform scale and does not swap Front/Back identity. See [`CHARACTER_ANIMATION_ARCHITECTURE.md`](CHARACTER_ANIMATION_ARCHITECTURE.md).
- Character **authored facing RIGHT**. Canonical rest pose faces +X.

## Rigid cutout vs free joint translation

`purgatory-skeleton` evaluate accepts local **translation + rotation** on every bone. That is the math contract. It does **not** mean every animation channel should freely translate every joint.

For rigid paper-doll / cutout limbs, the visual segment is attached to a joint and spans toward the child joint. Wrist, elbow, and knee (and the ankle as the shin–foot connection) should normally **keep that child offset** so the cutout and the skeleton connection stay coincident. Translating `hand_front` while `lower_arm_front` stays put opens a gap between the forearm panel and the wrist. The same is true of translating `foot_front` while the shin stays put.

Independent joint **rotation** about the existing origin is the default “independent hand / independent foot” channel. Independent joint **translation** is only used where a motion is explicitly intended (for example a designed foot plant/slide contact offset). It is not the default for diagnostic proofs.

Do not implement stretch, squash, or deformation in v0. Do not restrict skeleton math to rotation-only locals.

## Root vs Pelvis

| Bone | Role |
|---|---|
| `root` | **Character / presentation anchor.** The adapter places this at the presented entity pose (standalone in S2; presented entity in S3+). Core evaluate does not read gameplay position. |
| `pelvis` | **Primary animated body root** beneath `root`. Walk bob, weight shift, and other body motion live here and on descendant body bones. |

Animation clips should normally key **Pelvis and body bones**, not Root. Keying Root would fight the presentation anchor (double-apply locomotion, or move the character independently of the replica/local presentation pose).

## Front / Back (non-negotiable)

**Front** = the visually **near** limb when the character faces **RIGHT** (toward the camera relative to the torso).

**Back** = the visually **far** limb when the character faces **RIGHT** (away from the camera relative to the torso).

These are **not** anatomical left/right. They are not “the limb that happens to be on screen-left after a flip”.

When the adapter mirrors the whole skeleton for left-facing movement (typically root scale `sx = -1`):

- bone/slot **indices stay the same**
- Front remains Front (canonical near-limb in the RIGHT-facing identity)
- Back remains Back
- mirroring must **not** swap Front/Back indices, names, or slot attachments

If a later clip needs a true left/right anatomical overlay, that is a new concept. It must not overwrite Front/Back.

## Bones (Humanoid v0 — frozen topology)

Parent is by **index**. `none` = root. Bone count = **16**.

```text
root
  pelvis
    torso
      head
      upper_arm_front → lower_arm_front → hand_front
      upper_arm_back  → lower_arm_back  → hand_back
    upper_leg_front → lower_leg_front → foot_front
    upper_leg_back  → lower_leg_back  → foot_back
```

| Index | Label | Parent | Role |
|---:|---|---|---|
| 0 | `root` | none | Presentation anchor. Adapter-placed. Not a normal clip channel. |
| 1 | `pelvis` | 0 | Primary animated body root. Walk bob / weight shift. |
| 2 | `torso` | 1 | Upper body. |
| 3 | `head` | 2 | Head. Hair / face / helmet inherit this. |
| 4 | `upper_arm_front` | 2 | Near upper arm (RIGHT-facing). |
| 5 | `lower_arm_front` | 4 | Near forearm. |
| 6 | `hand_front` | 5 | Near hand / wrist. Independent grip and combat wrist. Weapon slot parent. |
| 7 | `upper_arm_back` | 2 | Far upper arm (RIGHT-facing). |
| 8 | `lower_arm_back` | 7 | Far forearm. |
| 9 | `hand_back` | 8 | Far hand / wrist. Independent from the forearm. |
| 10 | `upper_leg_front` | 1 | Near upper leg. |
| 11 | `lower_leg_front` | 10 | Near shin. |
| 12 | `foot_front` | 11 | Near foot. Independent plant, lift, slide, contact rotation. Shoe slot parent. |
| 13 | `upper_leg_back` | 1 | Far upper leg. |
| 14 | `lower_leg_back` | 13 | Far shin. |
| 15 | `foot_back` | 14 | Far foot. Same control as `foot_front`. |

Adding bones later **appends** new indices or uses a new definition version. Do not reuse an index for a different meaning inside the same definition.

### Why feet and hands are bones

Walk contact needs a Foot bone (plant, lift, small forward slide, contact rotation). A shoe slot on the shin cannot do that: slot offset is definition metadata, so the shoe would inherit shin motion only. Lift and most contact motion should rotate the shin (moving the ankle with the rigid segment) and/or rotate the foot about the ankle. Direct ankle translation is an explicit contact offset, not a default independent-foot channel.

Weapon grip and combat need a Hand bone for the same reason. A weapon slot on the forearm would lock weapon angle to the forearm. Wrist/hand rotation would be impossible without rotating the whole lower arm. **Weapon remains a Slot**, parented to `hand_front` — replaceable visual, not a dedicated weapon bone.

### Deferred bones (not in v0)

Do not add these because conventional 3D rigs have them. Add only when a clip needs independent control.

| Bone | Why deferred |
|---|---|
| `weapon` (dedicated socket bone) | v0 has a **Weapon slot** on `hand_front`. A second transform under the hand is only needed if the weapon must rotate/translate independently of the hand itself (e.g. sheathe vs grip). |
| Neck, extra spine, clavicle | Torso + head already give idle/walk torso and look. Extra joints are body-wave / shrug polish. |
| Toes | Foot bone already covers plant, lift, slide, and contact rotation. Toes are IK / mesh / later polish. |

## Rest / bind pose (temporary presentation units)

Skeleton space is **presentation space**. Bind/rest dimensions are **not** coupled to gameplay Player/NPC AABB or collision extents.

Exact rest-pose measurements are **not** a frozen gameplay contract. S1 ships readable temporary proportions in `purgatory-skeleton` (`humanoid::BIND_*`). Tests lock those constants so changes are deliberate; they may still be retuned before art integration. The 16-bone topology does not change when rest numbers move.

Current S1 bind locals (parent-relative, rotation 0 unless noted):

- `root` `(0, 0)`
- `pelvis` `(0, 0.42)`
- `torso` `(0, 0.30)`
- `head` `(0.07, 0.24)` — P4.2 +X lead (RIGHT-facing 3/4)
- `upper_arm_front` `(-0.10, 0.14)` / `lower_arm_front` `(0, -0.22)` / `hand_front` `(0, -0.10)` — Front chain at the former Back shoulder X; Front identity unchanged
- `upper_arm_back` `(0.08, 0.14)` / `lower_arm_back` `(0, -0.22)` rotation `+0.50` (~29°, peeks +X) / `hand_back` `(0, -0.10)` — Back chain at the former Front shoulder X; Back identity unchanged
- `upper_leg_front` `(-0.06, -0.04)` / `lower_leg_front` `(0, -0.20)` / `foot_front` `(0, -0.18)`
- `upper_leg_back` `(0.04, -0.04)` / `lower_leg_back` `(0, -0.20)` / `foot_back` `(0, -0.18)`

Slot rest transforms are full 2D **translation + rotation**, not translation-only. Most v0 slots use identity rest. S1 uses a non-identity rest on `shoe_front` and `weapon` so slot compose is observable in tests.

Phase **8E** presentation anchors are derived from this same bind/slot rest (client `AnchorPoint` → these locals). They are **rig constants** in `purgatory-skeleton`, not ad-hoc offsets in compose/draw:

| Content `AnchorPoint` | Rig constant | Derivation |
|---|---|---|
| `bone_origin` | identity | bone joint |
| `crown` | `ANCHOR_CROWN` | `+Y` by `BIND_HEAD.translation.y` (head has no child) |
| `chest` | `ANCHOR_CHEST` | midpoint of authored torso→head span (`BIND_HEAD`) |
| `grip_front` / `grip_back` | `ANCHOR_GRIP` | `SLOT_REST_WEAPON` (same rest, both hands) |
| `foot_front` / `foot_back` | `ANCHOR_FOOT` | `SLOT_REST_SHOE_FRONT` (same rest, both feet) |

`AnchorPoint` remains content vocabulary. The skeleton crate does not import it.

Front limbs are near, back limbs far: identity + draw order (and small rest x offsets on some limbs). Screen X is independent of that layering: in canonical RIGHT 3/4, the near hip sits slightly toward −X relative to the far hip. 2D math stays in XY.

Debug axes (when drawn in a later stage): bone local +X along the bone toward the child joint; +Y perpendicular in the plane (right-handed with +Z out of screen).

## Slots (accepted v0 table)

Slots are metadata: `slot_index → bone_index + rest local transform` (translation + rotation). No visual asset. Empty slots are valid. S4 may draw a marker at the slot transform without attaching art.

v0 slot count = **17**. That number is **not** required to equal bone count (16). Future visual slots may be appended without changing the 16-bone skeleton.

| Index | Label | Bone | Notes |
|---:|---|---|---|
| 0 | `head` | 3 `head` | Face / head graphic. |
| 1 | `hair_back` | 3 `head` | Far hair layer. |
| 2 | `hair_front` | 3 `head` | Near hair / bangs. |
| 3 | `torso` | 2 `torso` | Clothing / chest. |
| 4 | `upper_arm_back` | 7 `upper_arm_back` | Far upper-arm clothing. |
| 5 | `lower_arm_back` | 8 `lower_arm_back` | Far forearm clothing. |
| 6 | `hand_back` | 9 `hand_back` | Far hand / glove visual. Follows the **hand**, not the forearm. |
| 7 | `upper_arm_front` | 4 `upper_arm_front` | Near upper-arm clothing. |
| 8 | `lower_arm_front` | 5 `lower_arm_front` | Near forearm clothing. |
| 9 | `hand_front` | 6 `hand_front` | Near hand / glove visual. Follows the **hand**, not the forearm. |
| 10 | `upper_leg_back` | 13 `upper_leg_back` | Far thigh clothing. |
| 11 | `lower_leg_back` | 14 `lower_leg_back` | Far shin clothing. |
| 12 | `shoe_back` | 15 `foot_back` | Far shoe. Follows the **foot**, not the shin. |
| 13 | `upper_leg_front` | 10 `upper_leg_front` | Near thigh clothing. |
| 14 | `lower_leg_front` | 11 `lower_leg_front` | Near shin clothing. |
| 15 | `shoe_front` | 12 `foot_front` | Near shoe. Follows the **foot**, not the shin. |
| 16 | `weapon` | 6 `hand_front` | Held item. **Slot**, not a bone. Parents to **HandFront**. |

Deferred **slots** (no new bones required): `helmet` and `face` on `head`. Add when a visual layer needs its own replaceable identity.

## Draw order (independent of hierarchy)

Hierarchy is for posing. Draw order is for overlapping placeholder rects (and later attachments).

v0 **fixed default** (back → front), by **slot index**:

```text
1  hair_back
4  upper_arm_back
5  lower_arm_back
6  hand_back
10 upper_leg_back
11 lower_leg_back
12 shoe_back
3  torso
13 upper_leg_front
14 lower_leg_front
15 shoe_front
0  head
7  upper_arm_front
8  lower_arm_front
9  hand_front
16 weapon
2  hair_front
```

This list is data on the definition (a permutation of slot indices). It is **not** inferred from parent pointers.

**Deferred:** per-pose or per-clip draw-order tracks (e.g. swapping a front arm behind the torso for a specific swing).

## Mirroring

Adapter-only. Recommended: root `scale.x = -1` when facing left, with translation so the feet stay planted.

Forbidden:

- swapping Front/Back indices when facing changes
- using anatomical left/right as the slot key
- baking a second LEFT skeleton definition as the default v0 path

## Master authoring template

Developer Hub exports a Photoshop reference SVG from these same contracts (`apps/dev_hub/src/authoring_template.rs`). It does not copy bind / slot-rest / anchor numbers.

| Property | Value |
|---|---|
| File | [`Graphic/character/HUMANOID_V0_AUTHORING_TEMPLATE.svg`](../Graphic/character/HUMANOID_V0_AUTHORING_TEMPLATE.svg) |
| Size | **1024×1024 px**, transparent, SVG origin **top-left** |
| World | `+X` right, `+Y` up (same as this spec / FOOTNOTE) |
| Scale | **256 px = 1 presentation-world unit** (2× the 8B 128 px attachment-correction canvas) |
| Character origin | canvas **(512, 768)** = world **(0, 0)** (root / ground center) |
| Mapping | `x = 512 + world_x × 256` · `y = 768 − world_y × 256` |
| Ground | world `y = 0` → canvas `y = 768` |

Regenerate from Hub → Content → **Export authoring template**, or:

```text
$env:PURGATORY_WRITE_AUTHORING_TEMPLATE='1'
cargo test -p purgatory-dev-hub --bin purgatory-dev-hub write_authoring_template_if_requested
```

The 128×128 attachment-correction canvas remains **local to an attachment** (`AUTHORING_CANVAS_PX` in client compose). It is not this sheet. No ART/sprites are loaded.

### AI modular reference (V1)

A second Hub export, [`Graphic/character/HUMANOID_V0_AI_MODULAR_REFERENCE_V1.svg`](../Graphic/character/HUMANOID_V0_AI_MODULAR_REFERENCE_V1.svg), is an AI-facing Side Paper-Doll sheet (assembled + exploded parts). Same Humanoid v0 bind / anchors / torso placeholders and the same **256 px/wu** scale. It does **not** replace the technical template. Headwear Side is the first feasibility target (Crown). No Back variants or ART pipeline.

| Property | Value |
|---|---|
| File | [`Graphic/character/HUMANOID_V0_AI_MODULAR_REFERENCE_V1.svg`](../Graphic/character/HUMANOID_V0_AI_MODULAR_REFERENCE_V1.svg) |
| Size | **1600×1024 px**, transparent |
| Scale | **256 px = 1 wu** (same as the technical template) |
| Assembled origin | canvas **(280, 800)** = world **(0, 0)** |
| Mapping | `x = 280 + world_x × 256` · `y = 800 − world_y × 256` |
| Crown | world **(0.07, 1.20)** → assembled canvas **(297.92, 492.80)** |

Same regen env as the technical template (`PURGATORY_WRITE_AUTHORING_TEMPLATE=1` writes both files). Hub → Content → **Export AI modular reference**.

### Headwear Side master (V1 proof)

A third Hub export: an **empty** fixed-grid Side headwear overlay SVG. Four cells, same **256 px/wu** as the templates. Crown `+` is the attachment point, not the visual center. No example art. Client must not load this file.

| Property | Value |
|---|---|
| File | [`Graphic/character/headwear_side/HEADWEAR_SIDE_MASTER_V1.svg`](../Graphic/character/headwear_side/HEADWEAR_SIDE_MASTER_V1.svg) |
| Sheet | **512×512 px**, 2 columns × 2 rows, **256×256** cells, transparent |
| Crown local | **(128, 176)** px from each cell's top-left |
| Safe inset | **16 px** (guide only) |

```text
$env:PURGATORY_WRITE_HEADWEAR_SIDE_MASTER='1'
cargo test -p purgatory-dev-hub --bin purgatory-dev-hub write_headwear_side_master_if_requested
```

No example PNG, Back variants, other slots, atlas, or ART pipeline.

Animation Lab (DEV proof only) can load the single extracted sprite `equipment.debug.headwear_proof.a.side` from [`extracted/equipment.debug.headwear_proof.a.side.png`](../Graphic/character/headwear_side/extracted/equipment.debug.headwear_proof.a.side.png). Compose is `HEAD world ∘ ANCHOR_CROWN` at **256 px/wu**, Crown pivot **(128, 176)**. The game client compile-embeds the four extracted Side cells (`a`–`d`) onto the existing Headwear Crown attachment (no filesystem loader, not an asset pipeline); Player debug overlay selects HEADWEAR 1–4. Hub → Content → **Extract Headwear Side cells** crops a same-size artist PNG (`HEADWEAR_SIDE_MASTER_V1.png`) by grid bounds only.

## What this spec does not include

IK targets, twist bones, squash/stretch, mesh weights, sprite UVs, atlas packing, animation clip curves (A-track: a separate layer that **writes local Pose**, including foot translation and wrist rotation keys; see [`CHARACTER_ANIMATION_ARCHITECTURE.md`](CHARACTER_ANIMATION_ARCHITECTURE.md)).
