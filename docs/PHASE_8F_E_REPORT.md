# Phase 8F-E report — Equipment Side/Back visual selection

Status: **GREEN**. Root `PHASE` = `8F.E`. Protocol **v13 unchanged**. Phase **8F-F was not started**.

## 1. Content / API contract

Existing Equipment Content Schema v1 already authors logical keys:

```text
attachments[].visuals.side   // required
attachments[].visuals.back   // optional
```

`ViewVisuals::key_for(ViewVariant)` returns `Some(side)` or `Some(back)` / `None`. Character Presentation maps `PresentationView` → that key. Selection is **not** `ClimbBack` and **not** gameplay state.

```text
equipment item (ContentId)
  → EquipmentPresentation
  → BoundAttachment { visual_key_side, visual_key_back, bone, anchor, … }
  → PresentationView selects key at plan/draw
```

Resolve still runs on equipment change only and stores both keys. Anchors, `hidden_base`, bone layer, compose (`world ∘ anchor ∘ correction`), and 8F-A/B order are unchanged by the key switch.

JSON schema version stays **1**. No protocol fields.

## 2. Fallback policy

From 8B / `CONTENT_PIPELINE.md`: Back is optional; there is **no silent Side-as-Back**.

| View | Authored Back | Result |
|---|---|---|
| Side | any | `visuals.side` |
| Back | present | `visuals.back` |
| Back | missing | omit that attachment from the Back plan (do not draw Side) |

Pack examples: cloth cap / plate / tunic / pants / boots have Back keys. Leather gloves and the practice sword blade are Side-only (sword is also hidden by 8F-B ArmFront). Gloves `hand_back` is the live Side-only fallback fixture.

Missing content (unknown id / no presentation file) is unchanged: skip slot, diagnostic, no substitute item.

## 3. Files

- `crates/content/src/equipment.rs` — `ViewVisuals::key_for`
- `apps/client/src/character_presentation/resolve.rs` — `visual_key_back` + `visual_key_for_view`
- `draw_order.rs` — omit attachments with no key for the current view
- `debug_visual.rs` — debug color from selected key
- `state.rs` — `PresentationView` documents key selection
- `phase8e_tests.rs`, `phase8f_b/c/d_tests.rs` — pack Back-plan expectations
- `phase8f_e_tests.rs` — new
- `apps/client/src/debug/overlay.rs` — 8F-E copy
- Docs: `PHASE`, `README.md`, `docs/ROADMAP.md`, `docs/ARCHITECTURE.md`, `docs/TEST_GATES.md`, `docs/CONTENT_PIPELINE.md`, `docs/dev-tools/ROADMAP.md`

## 4. Tests / debug proof

`phase8f_e_tests.rs`: Side key, Back key when authored, gloves omit on Back, local/remote same keys, compose/layer unchanged + debug colors differ, Force Back draw view selects Back keys while activity stays Idle.

Skeleton tab: **Force Back view** (or Force ClimbBack) on **this client**. Judge the P4 / attachment placeholders for every CharacterPresentationSet player, not Stage D joints (local-only) and not the remote magenta AABB. The other client's overlay is independent.

## 5. Quality gate

`./scripts/check.ps1` **PASS** 2026-09-04 (`PURGATORY quality gate OK`). Protocol **v13**.

## 6. Next recommendation

**8F-F** can resolve visual keys to ART/sprites later, or add gameplay climb as the source of `ClimbBack`. Do not invent a second layer table. Equipment product UI remains later. Richer `climb_back.anim` limb keys stay Lab authoring.
