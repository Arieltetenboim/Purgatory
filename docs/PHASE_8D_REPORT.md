# Phase 8D report — Character Presentation Bridge

Status: **GREEN**. Root `PHASE` = `8D`. Protocol **v12 unchanged**. Phase **8E was not started**.

## 1. Where CharacterPresentationState lives

`apps/client/src/character_presentation/` (client-only). Not in `purgatory-skeleton`, simulation, protocol, or content.

## 2. Exact state fields

`CharacterPresentationState`:

- `pose: [f32; 2]` — presented body-center
- `facing: Facing` — `Right` | `Left`
- `activity: PresentationActivity` — `Idle` | `Move` | `Jump` | `Fall`
- `view: PresentationView` — `Side` now; `Back` reserved for later 8F
- `equipment: EquipmentView` — `Absent` or `Present([Option<ContentId>; 6])`

No GPU types, no protocol frames, no `EntityId` / `WireEntityId`, no `ReplicatedEntity`.

## 3. Local adapter path

`from_local(LocalMotion, held_facing)`:

- pose: predicted/presented body center
- velocity + grounded: **same source** — predicted `world.player_body()` when prediction is active; otherwise replica entity velocity + `local_grounded`
- equipment: copied from replica `Option<ReplicatedEquipment>` via `equipment_view_from_replica`

Do not pair predicted velocity with lagged `replica.local_grounded` (that kept airborne locals on Idle/Move).

## 4. Remote adapter path

`from_remote(RemoteMotion, held_facing)`:

- pose: interpolated replica pose (fallback replica position)
- velocity: replica velocity (no per-remote grounded on the wire)
- equipment: same replica conversion

Both adapters call one `build_character_presentation`.

## 5. Proof that downstream path is shared

`CharacterPresentationState` → `skeleton_input_from_state` → `prepare_skeleton` / `evaluate` on Humanoid v0. No `is_local` after the adapters. `CharacterPresentationSet::sync` uses that path for every visible player.

## 6. Equipment None / all-empty behavior

- Replica `None` → `EquipmentView::Absent`
- `Some(all-empty)` → `EquipmentView::Present` all `None`
- Partial slots carried as `Option<ContentId>`
- No synthesized ContentIds. Bind-pose skeleton evaluate succeeds with Absent or empty Present.

## 7. SKELETON public API used

Existing math API only: `humanoid_v0`, `LocalPose`, `WorldPose`, `evaluate`, `ROOT`, `BoneIndex`. Added bind-time `HUMANOID_V0_BONE_LABELS` / `humanoid_v0_bone_by_label`. No World, EntityId, protocol, or client types in the crate. Manifest test forbids those deps.

`SkeletonInput` is a **client** type. Skeleton still evaluates Definition + local Pose only (architecture invariant). Facing/activity are recorded on the input for later clips; 8D writes bind locals plus root translation.

## 8. BoneTarget → BoneIndex binding

`BoneTargetMap` binds once (`CharacterPresentationSet::new`). Lookup is `BoneTarget::as_str()` → `humanoid_v0_bone_by_label`. Missing canonical bone returns `MissingCanonicalBone { label }` — never Root/Pelvis. Root/Pelvis labels exist on the rig but are not BoneTargets.

## 9. Activity / facing mapping

Facing: `|vx| > 0.5` sets Left/Right; otherwise hold last (default Right).

Activity:

- airborne first (local `!grounded`; remote `|vy| > 0.25`): `vy > 0.25` → Jump else Fall (apex stays Fall, never Idle/Move)
- grounded / remote non-airborne: `|vx| > 0.5` → Move else Idle
- World +Y is up

View is always `Side`.

## 10. Lifecycle / AOI behavior

`CharacterPresentationSet` membership is rebuilt from `ReplicatedWorld` Player entities each frame. Leave/despawn drops the entry (generational `PresentationEntityKey`). Connection screen clears the set. No second AOI. Pose buffers allocated on enter, reused while the key remains.

## 11. Placeholder / no-ART readiness

Humanoid v0 bind pose evaluates with zero equipment and zero ART. Output is sufficient for later debug circles/rects/bones. S2 debug draw is unchanged (still one local overlay). 8D does not attach equipment visuals.

## 12. Two-character proof

`two_characters_share_path_and_remote_leave_drops_entry`: local A (Weapon) + remote B (Bodywear) share the builder; both produce SkeletonInput; B removal drops B only.

## 13. Tests / quality gate

See Gate 8D in [`TEST_GATES.md`](TEST_GATES.md). Focused: `character_presentation` (15) + skeleton label/manifest tests.

Quality gate: `./scripts/check.ps1` **PASS** 2026-09-02 (`fmt --check`, `check --workspace`, `clippy --workspace --all-targets --all-features -D warnings`, `test --workspace`, content validator `equipment=8 defs=15`). `PURGATORY quality gate OK`.

## 14. Per-character / per-frame cost

- BoneTarget map: once per set (16-label scan × 14 targets at bind)
- Enter: one `LocalPose` + `WorldPose` allocation (16 bones)
- Stable membership: `copy_bind` + `evaluate` O(16), no per-bone heap
- Sync: small `Vec` of Copy states; HashMap retain by epoch
- Extra vs S2: local player is evaluated twice this slice (S2 debug + 8D set). Not unified on purpose.

## 15. Files changed

`crates/skeleton` labels + tests; `apps/client/src/character_presentation/*`; `apps/client/src/app.rs`, `main.rs`, `skeleton_debug.rs` label re-export; docs + root `PHASE`.

## 16. Protocol version

**12.** Asserted by test. No new messages or replication fields.

## 17. Open issues for 8E / 8F

- Remote grounded is not on the wire; aerial activity for remotes is velocity-inferred
- Facing is client-derived from `vx`, not replicated
- `PresentationView::Back` is reserved; no Side-as-Back fallback
- Equipment ContentId is carried, not resolved to presentation attachments
- S2 debug draw is not yet driven by this set (still one local humanoid)

## 18. Phase 8E was NOT started

No ContentId → visual resolution, PresentationAttachment runtime, Overlay / ReplaceBase / hide_base / correction, Side/Back equipment selection, climb/attack animation, sprites, ART, or equipment UI.
