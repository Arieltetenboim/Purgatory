# Phase 8F-D report — ClimbBack animation clip wiring

Status: **GREEN**. Root `PHASE` = `8F.D`. Protocol **v13 unchanged**. Phase **8F-E was not started**.

## 1. Asset / registry path

Animations are not in `purgatory-content`. ClimbBack uses the existing A6 Animation Runtime path:

```text
content/shared/animations/dev/climb_back.anim
  → include_str! in crates/animation/src/dev.rs (asset name climb_back.anim)
  → parse_animation_asset_v1 → OnceLock<AnimationClip> via climb_back_clip()
  → clip_for_playback_activity(ClimbBack) in CharacterPresentationSet
  → AnimationPlayer + sample → LocalPose → evaluate (same as Idle/Move/Jump/Fall/Attack/Hurt)
```

Authored id is the file stem `climb_back`. Missing file fails compile (`include_str!`). Invalid parse logs and uses `bind_rotation_noop_clip` (bind/no-animation), not silent Idle.

## 2. Activity → clip mapping

| Activity | Clip |
|---|---|
| Idle | `a3_idle.anim` |
| Move | `a3_move.anim` |
| Jump | `a4_jump.anim` |
| Fall | `a4_fall.anim` |
| Attack | `a5_attack.anim` |
| Hurt | `a5_hurt.anim` |
| **ClimbBack** | **`climb_back.anim`** (was Idle) |

`view_for_activity(ClimbBack)` remains `Back`. Overlay **Force ClimbBack activity** still injects semantic ClimbBack on the shared local/remote path. **Force Back view** remains draw-only.

Preserved: 8F-A draw order, 8F-B visibility/remap, equipment attachments, `hidden_base`.

## 3. Files

- `content/shared/animations/dev/climb_back.anim` — authored Lab asset (unchanged by this step)
- `crates/animation/src/dev.rs`, `lib.rs`, `tests.rs` — `climb_back_clip()` + sample proof
- `apps/client/src/character_presentation/collection.rs` — ClimbBack → `climb_back_clip()`
- `apps/client/src/character_presentation/mod.rs` — 8F-D tests module
- `apps/client/src/character_presentation/phase8f_c_tests.rs` — drop Idle-pointer reuse; keep playback identity
- `apps/client/src/character_presentation/phase8f_d_tests.rs` — clip selection, sampled pose, local/remote, Back + equipment
- `apps/client/src/debug/overlay.rs` — 8F-D copy (rebuild after Lab save); FOOTNOTE contact now reads snapshot pose/velocity so clippy `-D warnings` stays green
- Docs: `PHASE`, `README.md`, `docs/ROADMAP.md`, `docs/ARCHITECTURE.md`, `docs/TEST_GATES.md`, `docs/CHARACTER_ANIMATION_ARCHITECTURE.md`, `docs/dev-tools/ROADMAP.md`

## 4. Tests

- `crates/animation`: `climb_back_clip_samples_pelvis_tx_not_idle`
- `phase8f_d_tests.rs`: ClimbBack clip ≠ Idle; Side activities unchanged; sampled pelvis `tx` reaches the presentation set; local/remote same clip and sample t; Back plan still hides front sword/hand and keeps back boot/glove; climb pelvis `tx` ≠ Idle

## 5. Live / debug proof

Skeleton tab: **Force ClimbBack activity** on, **Force Back view** off.

Expected: Back visibility (front limbs/attachments hidden) plus `climb_back.anim` pelvis sway (`tx`), not Idle breathing. Equipped sword/boots/gloves: front attachments hidden; back boot/glove follow the moving skeleton. Rebuild the client after Animation Lab save (`include_str!`, no hot reload).

## 6. Quality gate

`./scripts/check.ps1` **PASS** 2026-09-04 (`PURGATORY quality gate OK`). Protocol **v13**. Clippy required FOOTNOTE overlay to read previously unread `FootnoteDebug` pose/velocity fields (`-D warnings`).

## 7. 8F-E recommendation

**8F-E** can add equipment Side/Back visual keys (content presentation variants selected by `PresentationView`), still without sprites, equipment UI, or gameplay climb. Do not invent a second pose format. Richer limb keys in `climb_back.anim` are Lab authoring on this same path. Gameplay climb (ladder/rope, server activity) remains a later source for the same `PresentationActivity::ClimbBack`.
