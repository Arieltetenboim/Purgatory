# Phase 8F-B report — Front/Back Visibility

Status: **GREEN**. Root `PHASE` = `8F.B`. Protocol **v13 unchanged**. Phase **8F-C was not started**.

## 1. Visibility contract

Same canonical painter order as 8F-A (far → near, no numeric z):

```text
ArmBack → LegBack → Core → LegFront → Head → ArmFront
```

`PresentationView` is the semantic camera/view. It is not equipment Side/Back visual selection and not `Facing`.

| View | Authored visibility | Paint remap |
|---|---|---|
| **Side** | All layers | Identity (paint slot = authored layer) |
| **Back** | Hide `ArmFront` and `LegFront`. Show `ArmBack`, `LegBack`, `Core`, `Head` | `ArmBack → ArmFront`, `LegBack → LegFront`. Core/Head unchanged |

Attachments inherit their `BoneTarget` layer for both visibility and paint slot. `hidden_base` still omits only matching **base** pieces; it does not drop attachments.

`view_for_activity` returns `Side` for current Idle/Move/Jump/Fall/Attack/Hurt. ClimbBack (later) maps to `Back` through that hook. Adapters do not hard-code a second draw-order table.

Local and remote share `plan_character_draw` / `presentation_debug_quads`. Overlay **Force Back view** is draw-time proof only; locomotion state stays `Side` until climb exists.

## 2. Files

- `apps/client/src/character_presentation/state.rs` — `PresentationView` contract + `view_for_activity`
- `apps/client/src/character_presentation/draw_order.rs` — `authored_layer_visible`, `paint_layer`, `plan_character_draw(..., view)`
- `apps/client/src/character_presentation/adapters.rs` — view from `view_for_activity`
- `apps/client/src/character_presentation/debug_visual.rs`, `mod.rs`
- `apps/client/src/app.rs` — overlay Back override on the shared draw path
- `apps/client/src/debug/ui_state.rs`, `overlay.rs` — Force Back checkbox
- Tests: `phase8f_a_tests.rs` (Side arg), `phase8f_b_tests.rs` (new)
- Docs: `PHASE`, `README.md`, `docs/ROADMAP.md`, `docs/ARCHITECTURE.md`, `docs/TEST_GATES.md`

## 3. Debug proof

Skeleton tab: **Force Back view (8F-B visibility)**. Debug shapes/colors only (P4 placeholders + 8E attachment quads).

Expected when checked (equip practice sword + iron boots + gloves):

- Authored front limbs disappear (brighter P4 front pieces)
- Darker back limbs paint in front of torso/head
- Front boot / front glove / sword disappear; back boot / back glove remain and follow the remapped near slots
- Local and remote characters on the same path

## 4. Tests

`phase8f_b_tests.rs`: Side identity, Back hide/remap, attachment inheritance, unchanged canonical order, Side plan still 8F-A, `hidden_base` + Back, plate cuirass ReplaceBase, local/remote shared Back plan + debug quad counts. `phase8f_a_tests.rs` still covers Side draw order.

## 5. Quality gate

Official `./scripts/check.ps1` could not finish: workspace member `tools/animation_lab` (`purgatory-animation-lab`) was mid-implementation (fmt could not resolve `tests.rs`).

Equivalent gate **PASS** 2026-09-03 excluding that package:

```text
cargo fmt -p <existing workspace crates> -- --check
cargo check --workspace --exclude purgatory-animation-lab
cargo clippy --workspace --all-targets --all-features --exclude purgatory-animation-lab -- -D warnings
cargo test --workspace --exclude purgatory-animation-lab
```

Focused: 19 `phase8f_*` client tests passed. Clippy `-D warnings` clean on those packages.

## 6. 8F-C recommendation

**8F-C** should map climb/back **activity → `PresentationView::Back`** through `view_for_activity` so the overlay force is no longer required for that pose. Do not add a second layer table, numeric z, sprites, or a full climb animation clip set in that step unless the phase explicitly asks for clips.
