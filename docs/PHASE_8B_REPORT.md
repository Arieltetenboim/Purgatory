# PHASE 8B REPORT — Equipment Content Schema + Validation

**GREEN** — content schema and validation only. Protocol **v11** unchanged. Phase **8C was not started**.

## 1. Where Equipment Content Schema v1 lives

- Types and validators: `crates/content/src/equipment.rs` (`purgatory-content`)
- JSON loader: `crates/content/src/loader.rs` (`Kind::Equipment`, `Kind::EquipmentPresentation`)
- Registry: `crates/content/src/registry.rs`
- Pack: `content/shared/equipment/` (gameplay) and `content/shared/equipment_presentation/` (client presentation)
- Schema constant: `EQUIPMENT_CONTENT_SCHEMA_VERSION = 1` (independent of map/entity `CONTENT_SCHEMA_VERSION`)

Authoritative runtime state remains Phase 8A: `EquipmentSlot → Option<ContentId>` on `World`. 8B does not mutate that state.

## 2. Gameplay definition shape

```text
EquipmentDefinition {
  content_id: ContentId,
  authored_id: String,
  slot: EquipmentSlot,          // headwear|bodywear|pants|gloves|boots|weapon
  domain: ContentDomain,        // Shared for v1 pack files
}
```

JSON:

```json
{ "schema_version": 1, "id": "equipment.debug.cloth_cap", "equipment_slot": "headwear" }
```

Unknown JSON fields are rejected. No stats, inventory, combat, ART, or presentation fields.

## 3. Presentation definition shape

Same `ContentId` as the gameplay file. Slot is **not** duplicated; semantic checks use the gameplay slot.

```text
EquipmentPresentation {
  content_id: ContentId,
  authored_id: String,
  attachments: Vec<PresentationAttachment>,  // 0..N
}

PresentationAttachment {
  id: String,                   // local [a-z][a-z0-9_]*, unique per item, max 32
  bone: BoneTarget,
  anchor: AnchorPoint,
  coverage: CoverageMode,
  hide_base: Vec<BoneTarget>,
  correction: CorrectionOffset, // x, y, rotation; no scale
  visuals: ViewVisuals,         // side: String, back: Option<String>
}
```

Visual keys are logical authored ids (e.g. `equipment.debug.iron_boots.front`), not PNG paths or GPU handles.

## 4. BoneTarget enum

Closed set, snake_case JSON names aligned with Humanoid v0 labels. **Not** a `purgatory-skeleton` type. Root and Pelvis are not targets.

| Variant | JSON |
|---|---|
| Head | `head` |
| Torso | `torso` |
| UpperArmFront | `upper_arm_front` |
| LowerArmFront | `lower_arm_front` |
| HandFront | `hand_front` |
| UpperArmBack | `upper_arm_back` |
| LowerArmBack | `lower_arm_back` |
| HandBack | `hand_back` |
| UpperLegFront | `upper_leg_front` |
| LowerLegFront | `lower_leg_front` |
| FootFront | `foot_front` |
| UpperLegBack | `upper_leg_back` |
| LowerLegBack | `lower_leg_back` |
| FootBack | `foot_back` |

Skeleton still uses dense `BoneIndex` rather than a closed `BoneId` enum. That is **not** a stop-condition conflict: 8B keeps a presentation vocabulary in content and does not import skeleton internals.

## 5. AnchorPoint enum

| Variant | JSON |
|---|---|
| BoneOrigin | `bone_origin` |
| Crown | `crown` |
| Chest | `chest` |
| GripFront | `grip_front` |
| GripBack | `grip_back` |
| FootFront | `foot_front` |
| FootBack | `foot_back` |

No free-form anchor strings. No speculative Back/Cape/jewelry anchors.

## 6. Bone / anchor compatibility

- `BoneOrigin` — any `BoneTarget`
- `Crown` — `Head` only
- `Chest` — `Torso` only
- `GripFront` — `HandFront` only
- `GripBack` — `HandBack` only
- `FootFront` / `FootBack` anchors — matching foot bones only

Slot extra rule: **Weapon must use GripFront or GripBack**. Grips are **weapon-only** (Gloves on `HandFront` + `GripFront` fails).

Example: Weapon + `HandFront` + `GripFront` is valid. Weapon + `FootFront` + `GripFront` fails.

## 7. CoverageMode

Exactly two values: `overlay`, `replace_base`.

- **Overlay** — base body stays visible; `hide_base` must be empty.
- **ReplaceBase** — may list explicit base-body `BoneTarget`s to hide. Empty `hide_base` is allowed. Duplicates are rejected.

## 8. hide_base rules

- Explicit only. Never inferred from `BoneTarget`.
- Overlay + nonempty `hide_base` is invalid.
- Names must parse as `BoneTarget` (unknown → fail). `"weapon"` and other equipment/slot names are **unknown base visuals**, not equipment-to-equipment hides.
- Equipment cannot hide other equipment: there is no field for that, and unknown names fail.
- Hide targets are base-body bones only (the `BoneTarget` set).

## 9. Correction bounds

Authoring space: 128×128 shared canvas. Local correction only.

- `x`, `y`: −8..+8 px (`CORRECTION_OFFSET_MAX_PX`)
- `rotation`: −15..+15 degrees (`CORRECTION_ROTATION_MAX_DEG`)
- Default all zero when omitted
- No scale field (unknown JSON field `scale` fails)
- Out of range / non-finite values fail. No silent clamp.

## 10. Side / Back representation

`ViewVisuals { side: String, back: Option<String> }`.

- Side is mandatory on every attachment (`visuals.side` required at parse; empty rejected).
- Back is optional. Missing Back is valid (Side-only).
- No silent Side-as-Back.
- `PresentationCompleteness { has_side, has_back }` is the **8F hook**. 8B does **not** require Back by slot category.

## 11. Slot / target compatibility matrix

| Slot | Allowed BoneTarget |
|---|---|
| Headwear | Head |
| Bodywear | Torso, UpperArmFront, UpperArmBack, LowerArmFront, LowerArmBack |
| Pants | UpperLegFront, LowerLegFront, UpperLegBack, LowerLegBack |
| Gloves | HandFront, HandBack |
| Boots | FootFront, FootBack |
| Weapon | HandFront, HandBack (grip anchors required) |

## 12. Validation error model

Fail-fast. `ValidationIssue` via existing `ContentError`:

```text
source=equipment | file path
definition=ContentId
field=attachments[<id>].<field>
reason=slot=<slot> rule=<rule>: <detail>
```

Rules include `schema`, `slot_target`, `bone_anchor`, `slot_anchor`, `coverage`, `hide_base`, `correction`.

No silent substitute of slot, anchor, visual, or correction.

Layers:

- **A. Schema** — version, closed enums, ContentId, unique attachment ids, correction bounds, Side required, no paths/extensions, deny unknown JSON fields
- **B. Semantic** — slot↔bone, bone↔anchor, slot↔anchor, Overlay/hide_base, ReplaceBase duplicates, presentation must have matching gameplay
- **C. Completeness hook** — `PresentationCompleteness` for 8F; no animation-system Back-required rules in 8B

## 13. Debug / test fixtures

Valid pack (8 gameplay + 8 presentation pairs):

| ContentId | Exercises |
|---|---|
| `equipment.debug.unadorned` | 0 attachments |
| `equipment.debug.cloth_cap` | Headwear Overlay, Crown, Side+Back |
| `equipment.debug.tunic` | Bodywear multiple overlay attachments |
| `equipment.debug.plate_cuirass` | Bodywear ReplaceBase hide torso+upper arms |
| `equipment.debug.cloth_pants` | Pants four leg attachments, Side+Back |
| `equipment.debug.leather_gloves` | Gloves both hands, Side-only |
| `equipment.debug.iron_boots` | Boots both feet, bounded nonzero correction on front |
| `equipment.debug.practice_sword` | Weapon HandFront + GripFront, Side-only |

Invalid cases are tests (temp JSON + in-memory structs), not pack files: unknown bone (`root`), orphan presentation, Overlay+hide_base, missing Side, illegal hide (`weapon`), incompatible anchor, excessive correction, duplicate attachment id, slot mismatch, unknown JSON field, scale correction.

## 14. Tests / results

`cargo test -p purgatory-content --lib`: **50 passed**.

Includes the requested cases: valid gameplay/presentation, 0 and N attachments, unique ids, Overlay empty/nonempty hide_base, ReplaceBase valid/duplicate hides, bone/anchor valid/invalid, slot/target, correction bounds/default zero/nonzero, Side-only, Side+Back, missing Side, no asset path, weapon grip required, gloves cannot use grip, 8F completeness hook does not require Back by slot, pack load + invalid JSON fixtures (unknown bone, orphan presentation, Overlay+hide_base, missing Side, illegal hide, incompatible anchor, excessive correction, duplicate id, slot mismatch, unknown field, scale).

`cargo test -p purgatory-simulation --lib equipment`: includes `slot_name_roundtrip`.

`purgatory-content-validator`: `equipment=8 defs=15`.

## 15. Files changed

Primary implementation: `crates/content/src/{equipment.rs,loader.rs,registry.rs,lib.rs}`, `crates/simulation/src/equipment.rs` (`EquipmentSlot::as_str` / `parse` for JSON names), `tools/content_validator/src/main.rs`, `content/shared/equipment/`, `content/shared/equipment_presentation/`.

Docs: `PHASE`, `README.md`, `docs/{CONTENT_PIPELINE,ARCHITECTURE,PROTOCOL,ROADMAP,TEST_GATES,PHASE_8B_REPORT}.md`, `docs/dev-tools/ROADMAP.md`.

Incidental quality-gate fix (pre-existing, not 8B behavior): `apps/client/src/skeleton_debug.rs` (`skeleton_debug_quads` is a test helper; P2 placeholder count includes P4/P3 layers).

Protocol crate source: **unchanged in 8B**. `PROTOCOL.md` notes that presentation is not on the wire.

## 16. Protocol version confirmation

`PROTOCOL_VERSION == 11`. No wire types for `BoneTarget`, `AnchorPoint`, `ViewVariant`, visual keys, `CoverageMode`, `hide_base`, correction, or attachment ids. Server still reasons only about `EquipmentSlot → Option<ContentId>`.

## 17. Deferred to 8F / later

- Animation-visibility “Back required when this attachment is visible in ClimbBack” (8F uses `PresentationCompleteness`)
- Skeleton runtime bind of `BoneTarget` → `BoneIndex` (not 8B)
- Renderer / ART resolution of visual keys (placeholders later)
- EquipRequest / World mutation (8C)
- Equipment on the protocol wire
- Inventory, combat, item stats

## 18. Phase 8C was NOT started

No EquipRequest, no accept/reject, no authoritative equip mutation, no skeleton integration, no rendering, no ART pipeline, no protocol bump.

## Quality gate

`./scripts/check.ps1` **PASS** (2026-09-02):

- `cargo fmt --all -- --check`
- `cargo check --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- `cargo run -p purgatory-content-validator -q` → `equipment=8 defs=15`

`PURGATORY quality gate OK`.

## Manual / runtime verification still required

Client Shared pack load includes the new JSON; automated validator/load tests cover that. Visual presentation of attachments is **not** in 8B.

## Deviations

None from the 8B brief. Skeleton naming is aligned without a crate dependency.
