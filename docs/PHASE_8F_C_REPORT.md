# Phase 8F-C report — Activity → PresentationView

Status: **GREEN**. Root `PHASE` = `8F.C`. Protocol **v13 unchanged**. Phase **8F-D was not started**.

## 1. State / data path

```text
local predicted pose / remote interpolated replica
  → from_local_with_oneshot / from_remote_with_oneshot
      (velocity+grounded → Idle/Move/Jump/Fall; Attack/Hurt oneshot overlay)
  → optional DEV apply_climb_back_overlay (ClimbBack; skipped while a oneshot is active)
  → view_for_activity(activity)
  → CharacterPresentationState { activity, view, … }
  → CharacterPresentationSet (shared local/remote)
  → plan_character_draw(hidden_base, attachments, view)
```

Skeleton still sees only Definition + local Pose. No World/replication types enter `purgatory-skeleton`.

| Activity | View |
|---|---|
| Idle, Move, Jump, Fall, Attack, Hurt | Side |
| ClimbBack | Back |

ClimbBack is not inferred from velocity and is not a protocol oneshot. Playback reuses the Idle clip (no climb clip in this step). Overlay **Force Back view** remains draw-time only and does not change `activity`. Overlay **Force ClimbBack activity** injects semantic ClimbBack on the same local/remote path for proof.

## 2. Files

- `apps/client/src/character_presentation/state.rs` — `ClimbBack` + `view_for_activity`
- `adapters.rs` — `apply_climb_back_overlay`
- `collection.rs` — ClimbBack reuses Idle clip
- `mod.rs`, `app.rs` — shared-path overlay after oneshot
- `apps/client/src/debug/ui_state.rs`, `overlay.rs`
- Tests: `phase8f_c_tests.rs`
- Docs: `PHASE`, `README.md`, `docs/ROADMAP.md`, `docs/ARCHITECTURE.md`, `docs/TEST_GATES.md`, `docs/CHARACTER_ANIMATION_ARCHITECTURE.md`, `docs/dev-tools/ROADMAP.md`

## 3. Debug proof

Skeleton tab: **Force ClimbBack activity (8F-C)** with **Force Back view** unchecked.

Expected: local and remote placeholders use the 8F-B Back plan (front limbs/attachments hidden; back limbs/equipment near). Activity is ClimbBack. Toggling only Force Back still shows Back draw while activity stays Idle/Move/Jump/Fall.

## 4. Tests

`phase8f_c_tests.rs`: activity→view map, local/remote ClimbBack view, Back plan + equipment inheritance, Side activities unchanged, shared draw, Force Back does not mutate activity, oneshot wins over ClimbBack overlay, Idle clip reuse.

## 5. Quality gate

`./scripts/check.ps1` **PASS** 2026-09-03 (`PURGATORY quality gate OK`). Focused: 28 `phase8f_*` client tests passed (including 8F-C). Protocol **v13**.

## 6. 8F-D recommendation

**8F-D** can add a presentation climb/back clip (or authored pose) sampled for `ClimbBack`, still without sprites or equipment UI. Do not invent a second layer table. Gameplay climb (ladder/rope, server activity) remains a later source for this same `PresentationActivity`.
