# Phase 8E report — Equipment Attachment Composition + Debug Placeholders

Status: **GREEN**. Root `PHASE` = `8E`. Protocol **v12 unchanged**. Phase **8F was not started**.

## 1. Where composition lives

Client-only, in `apps/client/src/character_presentation/` (`resolve.rs`, `anchors.rs`, `compose.rs`, `debug_visual.rs`). Not in simulation, protocol, or `purgatory-skeleton` (skeleton still evaluates Definition + local Pose only).

## 2. BoundAttachment fields

`BoundAttachment`: `slot`, `content_id`, `attachment_id`, `visual_key_side`, `bone` (`BoneIndex`), `bone_target`, `anchor`, `coverage`, `hide_base` (`u16` bitset), `correction` (`BoneTransform`, px→wu and deg→rad at bind).

## 3. Resolve cache

`CharacterPresentationSet` stores `equipment_key: EquipmentView`. Registry lookup runs only when that key changes (enter or equip/unequip). `resolve_count` increments only then. Stable sync keeps the same `BoundAttachment[]` allocation (`as_ptr()` identity). AOI leave already drops the entry.

## 4. Compose order

`world(bone).compose(anchor_local).compose(correction_local)`.

## 5. Anchors

Derived from Humanoid v0 bind/slot rest, centralized in `purgatory-skeleton`:

| `AnchorPoint` | Rig constant | Source |
|---|---|---|
| BoneOrigin | identity | joint |
| Crown | `ANCHOR_CROWN` | `BIND_HEAD.translation.y` |
| Chest | `ANCHOR_CHEST` | midpoint of `BIND_HEAD` |
| GripFront / GripBack | `ANCHOR_GRIP` | `SLOT_REST_WEAPON` |
| FootFront / FootBack | `ANCHOR_FOOT` | `SLOT_REST_SHOE_FRONT` |

Content `AnchorPoint` does not enter the skeleton crate. No scattered ad-hoc offsets in compose/draw.

## 6. Overlay vs ReplaceBase / hide_base

Hide union is **ReplaceBase only**. Overlay hide bits are ignored even if present. `hidden_base` is a `u16` whose bit `i` is `BoneTarget::ALL[i]` (locked + tested). Hiding Torso skips both P4 torso panels.

## 7. Missing content

Unknown ContentId or gameplay-without-presentation: diagnostic `{slot, content_id, reason}`, skip that slot, no panic, no substitute item. Other slots still resolve.

## 8. Debug visual policy

**Shape** from attachment semantics (`EquipmentSlot` + `AnchorPoint` + `CoverageMode` + `BoneTarget`). Weapon+grip → blade rect even if the visual key has no `"blade"` substring. **Color** from FNV-ish hash of the Side visual key (DEV distinctness only).

## 9. Draw path

P4 placeholders + attachment debug quads for **every** `CharacterPresentationSet` entry (local predicted and remote interpolated). Stage D joints remain the local S2 overlay (`skeleton_overlay_quads(..., placeholders=false)`). This is the first visual proof that local and remote share one pipeline. Existing `DrawQuad` / present pass; no new GPU pass. No production ART.

## 10. Local / remote share

Both adapters still feed one `CharacterPresentationState`. Resolve + compose + debug draw consume that set with no `is_local` branch.

## 11. Correction conversion

Authoring canvas 128×128 px → 1 presentation unit: `translation = [x,y] / 128`, `rotation = degrees.to_radians()`. No re-clamp (8B already validated bounds). Iron boots front correction is observable vs bone origin.

## 12. Protocol version

**12.** Asserted by test. No new messages, no replication fields, no facing/grounded on the wire.

## 13. Tests / quality gate

Focused: `character_presentation` (29 tests: prior 8D + 8E resolve/compose/hide/cache/shape). Skeleton: `presentation_anchors_are_derived_from_humanoid_v0_bind`; client `character_placeholder_quads_skip_hidden_torso`. Content: `equipment_presentation_by_id` + `authorize_equip` still gameplay-only.

Quality gate: `./scripts/check.ps1` **PASS** 2026-09-02 (`fmt --check`, `check --workspace`, `clippy --workspace --all-targets --all-features -D warnings`, `test --workspace`, content validator `equipment=8 defs=15`). `PURGATORY quality gate OK`. Client bin: 460 passed, 1 ignored. Skeleton lib: 26 passed.

## 14. Per-character / per-frame cost

- Resolve: only on equipment change (string clones of attachment id + visual key at bind)
- Stable frame: `copy_bind` + `evaluate` O(16) + compose O(bound) with no registry lookup
- Hide mask: `u16` test per placeholder bone
- Extra vs 8D: one compose + one debug quad per bound attachment; local S2 still evaluates for joints/inspect

## 15. Files changed

`crates/content` registry `equipment_presentation_by_id`; `crates/skeleton` `ANCHOR_*`; client `character_presentation/{anchors,resolve,compose,debug_visual,collection,phase8e_tests}`; `skeleton_debug` filtered placeholders; `app.rs` draw + DEV equip; overlay Skeleton → Debug equipment; docs + root `PHASE`.

## 16. DEV overlay

Skeleton tab **Debug equipment**: 8 debug fixtures, per-slot unequip, unequip-all. Sends protocol Equip/Unequip (server-authoritative). Shows bound count, hidden mask, missing-presentation diagnostics.

## 17. Quality-gate results

`./scripts/check.ps1` **PASS** 2026-09-02:

- `cargo fmt --all -- --check`
- `cargo check --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- `purgatory-content-validator`: `equipment=8 defs=15`

`PURGATORY quality gate OK`.

## 18. Manual / runtime verification still required

- Two clients: local and remote both show presentation-set placeholders (not S2-only local)
- Equip cloth cap / plate cuirass / practice sword / iron boots via overlay; confirm crown/chest/grip/foot placement and ReplaceBase hide
- Confirm unadorned (0 attachments) does not hide base
- Confirm 2× debug preview still scales presentation about planted feet
- Visual smoke only; no ART

## 19. Deviations / unresolved / 8F not started

Deviations: none from the locked 8E plan + owner refinements (derived anchors, semantic shapes, locked hide bits, bound-buffer identity, set-driven placeholders).

Unresolved: remote grounded and facing still client-derived (8D). `PresentationView::Back` unused. S2 proof poses still affect local joints only, not set placeholders.

**8F was not started:** no Back view switching, ClimbBack, animation visibility, final z-order, sprites, equipment product UI, inventory, or wire facing/grounded.
