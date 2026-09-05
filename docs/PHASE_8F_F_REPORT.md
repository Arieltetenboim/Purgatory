# Phase 8F-F report — 8F closeout / audit

Status: **GREEN**. Root `PHASE` = `8F` (8F complete + closeout). Protocol **v14 unchanged** by this step. **Phase 9 was not started.**

## 1. Canonical path (locked)

```text
PresentationActivity
  → view_for_activity → PresentationView
  → clip_for_playback_activity → AnimationClip → sample → LocalPose → evaluate
  → plan_character_draw(hidden_base, bound, view)
  → BoundAttachment::visual_key_for_view(view)
```

| Check | Result |
|---|---|
| One `PresentationLayer` table | `ArmBack → LegBack → Core → LegFront → Head → ArmFront` only |
| Numeric z / second order table | None. Paint remap is the same enum |
| `ClimbBack → Back → climb_back.anim` | `view_for_activity` + `clip_for_playback_activity` |
| Local/remote | One `CharacterPresentationSet`; adapters only at the edge |
| Equipment Back from `PresentationView` | Not from `ClimbBack` or gameplay |
| Missing Back | Omit from Back plan; no Side-as-Back |
| Anchors / `hidden_base` / compose | Unchanged (`world ∘ anchor ∘ correction`) |
| Force Back / Force ClimbBack | DEV overlay only (`dev-diagnostics`); observing-client CharacterPresentationSet entries |

## 2. Cleanup

Removed the identity `playback_activity()` remap (leftover from 8F-C ClimbBack→Idle). Clip selection is a direct activity match. Per-entry `playback_activity` field remains (transition detection).

## 3. Remaining debt (not 8F blockers)

- No sprites/ART; debug color hashes the visual key
- No gameplay climb; ClimbBack is DEV-forced or future server activity
- No equipment product UI
- `climb_back.anim` is a pelvis `tx` loop
- Remote magenta AABB and Stage D joints are outside this path
- Protocol v14 (`DevResetPlayer`) is independent of 8F

## 4. Tests

`phase8f_f_tests.rs`: layer table, ClimbBack view/clip/visual-key chain (cap Back + gloves omit), local/remote shared plan/anchor/`hidden_base`. Prior 8F-A..E tests remain.

## 5. Quality gate

`./scripts/check.ps1` **PASS** 2026-09-04 (`PURGATORY quality gate OK`). Protocol **v14**.

## 6. Close

**8F CLOSE.** Do not start Phase 9.
