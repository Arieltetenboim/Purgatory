# Phase 8F-A report — Character Presentation Draw-Order Contract

Status: **GREEN**. Root `PHASE` = `8F.A`. Protocol **v13 unchanged**. Phase **8F-B was not started**.

## 1. Draw-order contract

Painter's algorithm, far → near, no per-item numeric z:

```text
ArmBack → LegBack → Core → LegFront → Head → ArmFront
```

Within a layer: base body pieces (proximal → distal; torso far shade before torso near), then attachments for that layer.

Attachments inherit their authored `BoneTarget` layer (`layer_for_bone_target`). Slot/anchor do not invent z. Same-layer attachments sort by slot index then attachment id.

Hidden `ReplaceBase` omits that bone's base pieces only. Remaining relative order is unchanged.

## 2. Ownership / API

| Owner | Responsibility |
|---|---|
| `purgatory-skeleton` | Bone structure, bind, evaluate. No draw policy. |
| Character Presentation (`draw_order.rs`) | `PresentationLayer`, `BasePiece`, `plan_character_draw` |
| Equipment content | `BoneTarget` / slot / anchor / coverage only |

`presentation_debug_quads` emits in plan order. Local and remote share `CharacterPresentationSet`. Characters themselves draw in `iter_draw_order` (`PresentationEntityKey` sort), not HashMap order.

## 3. Files

- `apps/client/src/character_presentation/draw_order.rs` (new)
- `debug_visual.rs`, `collection.rs`, `mod.rs`, `app.rs`, `skeleton_debug.rs`, overlay Skeleton tab
- Docs: `PHASE`, `README.md`, `docs/ROADMAP.md`, `docs/ARCHITECTURE.md`, `docs/TEST_GATES.md`

## 4. Visual / debug proof

Existing P4 colors (darker back limbs, brighter front) plus 8E attachment debug shapes. Equip practice sword + iron boots:

- Back boot after back leg, before torso
- Front boot after front leg bases, before head
- Sword after front-arm bases (in front of body)

Skeleton tab notes the 8F-A layer list.

## 5. Tests

`phase8f_a_tests.rs`: canonical layers, bone/attachment mapping, reversed-slice stability, hidden torso, sword after core/back, local/remote shared plan.

## 6. Quality gate

`./scripts/check.ps1` **PASS** 2026-09-03. `PURGATORY quality gate OK`.

## 7. 8F-B recommendation

**8F-B** can introduce Front/Back *visibility* (which layers are shown for `PresentationView::Back` / climb) by masking or remapping the same `PresentationLayer` set. Do not add numeric z or a second order table. Not started here.
