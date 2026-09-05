# PHASE 8A REPORT — Equipment Data Model

## Status

**GREEN** — data model only. Protocol **v11** unchanged. Phase **8B was not started**.

## Implemented scope

- Fixed `EquipmentSlot` v1: Headwear, Bodywear, Pants, Gloves, Boots, Weapon.
- `EquipmentState`: six `Option<ContentId>` fields (array indexed by slot). All-`None` is the default and is valid.
- Slot-level dirty: `EquipmentDirtyMask` (`u8`, one bit per slot).
- Full vs delta measurement codec (`encode_equipment_full` / `encode_equipment_delta`) in simulation — not protocol messages.
- Authoritative storage on `World` entity slots as optional `EquipmentState`. Players spawn with no equipment attached. `None` means no equipped content; `ContentId::from_token(0)` is a real id, not empty.
- Dirty integration: `DirtyFlags.equipment`, `DomainRevs.equipment`, `ReplicationDirtyMask.equipment` (slot bits). Transform/health/membership/replication domains were not redesigned.
- No UI, skeleton, ART, equip request, inventory, combat, or content schema.

## 1. Where authoritative EquipmentState lives

`EntityData.equipment: Option<EquipmentState>` inside `purgatory-simulation` `World` slots.

- `None` — entity has no equipment domain (default, including spawned players).
- `Some(EquipmentState)` — equipment domain present; slots may still all be `None`.

World API: `equipment_of`, `equipment_slot`, `set_equipment_slot`, `clear_equipment_slot`, `equipment_dirty_of`, `consume_equipment_dirty`.

## 2. EquipmentSlot representation

Closed `#[repr(u8)]` enum, dense `0..5`. Not a HashMap. Not string-keyed.

## 3. Dirty-mask representation

`EquipmentDirtyMask { bits: u8 }` with bits 0–5. Reserved bits 6–7 are rejected on decode.

A slot write that changes the stored `Option<ContentId>`:

- sets `DirtyFlags.equipment`
- marks that slot bit
- bumps `DomainRevs.equipment`
- merges `ReplicationDirtyMask::equipment_only(slot)` for 6G.7B fan-out

Idempotent writes and all-`None` idle produce no equipment dirtiness.

Wire Update still uses protocol `DomainMask` transform/health only. Observer `CommittedRevs` / `reconcile_update` do not commit an equipment rev in 8A. Equipment-only pending is cleared as “no transform/health lag” and is not emitted. That is intentional until a later slice adds the wire domain.

## 4. Full vs delta contract

| Form | Use (later) | 8A encode |
|---|---|---|
| `EquipmentState` | Enter / AOI / spawn / reconnect / resync | occupied-mask `u8` + `u64` token per occupied slot |
| `EquipmentDelta` | equip/unequip on a known entity | changed-mask `u8` + per dirty slot: presence `u8` + optional token |

One changed slot serializes without the other five. Presence is explicit; token `0` is not “empty”.

## 5. How all-None equipment is represented

- `EquipmentState::default()` / `empty()`: six `None`s. Occupied mask `0`. Full encode = 1 byte.
- Entity without the capability: `equipment_of` is `None` (spawned player).
- Empty slot: `Option::None`, not a sentinel `ContentId`.

## 6. Tests added / results

- `crates/simulation/src/equipment.rs` — 9 unit tests (default, get/set/clear, idempotent set, slot-isolated dirty, independent bits + take, no sentinel, full/delta roundtrip + sizes).
- `crates/simulation/src/phase8a_tests.rs` — 12 World tests (player valid with no equipment, spawned empty `EquipmentState`, get/set/clear, idempotent no dirty, Some→None target slot only, one-slot isolation, all six bits, consume/clear, token 0, encode sizes, idle no churn, missing entity).

`purgatory-simulation` lib: 290 tests.

## 7. Measured representation sizes

| Payload | Bytes | Constant |
|---|---|---|
| Full, all empty | 1 | `EQUIPMENT_FULL_EMPTY_BYTES` |
| Full, one occupied | 9 | mask + 8-byte token |
| Full, six occupied | 49 | `EQUIPMENT_FULL_ALL_OCCUPIED_BYTES` |
| Delta, one-slot equip | 10 | `EQUIPMENT_DELTA_EQUIP_BYTES` |
| Delta, one-slot unequip | 2 | `EQUIPMENT_DELTA_UNEQUIP_BYTES` |

Existing `ContentId` compact token; no new network-local id map.

## 8. Files changed

**Code:** `crates/simulation/src/equipment.rs` (new), `phase8a_tests.rs` (new), `dirty.rs`, `domain.rs`, `spawn.rs`, `world.rs`, `lib.rs`; `apps/server/src/network/replication.rs`, `replication_policy.rs` (struct literals for the new mask field).

**Docs / marker:** `PHASE`, `README.md`, `docs/ROADMAP.md`, `docs/ARCHITECTURE.md`, `docs/PROTOCOL.md`, `docs/TEST_GATES.md`, `docs/PERFORMANCE_BUDGETS.md`, `docs/CONTENT_PIPELINE.md`, `docs/PHASE_7_PLAN.md`, `docs/dev-tools/ROADMAP.md`, this report.

## 9. Architectural conflict / open question

No stop-condition was hit. `ContentId` was reused. Slot dirty extends the existing domain mask; it does not replace transform/health.

**Deferred (correct later slice, not 8A):** protocol `DomainMask` / Enter-Update payload for equipment; per-observer `CommittedRevs.equipment`; content-pack slot compatibility (8B/8C); equip/unequip requests.

## 10. Phase 8B was NOT started

No EquipRequest, presentation state, skeleton bridge, attachments, animation, renderer, UI, inventory, or combat.

## Quality gate

`./scripts/check.ps1` — **PURGATORY quality gate OK** (2026-09-02).

## Manual / runtime verification still required

None for 8A (no UI, no wire equipment, no client presentation). Visual/multiplayer checks belong to later slices.

## Deviations

None from the 8A brief. Protocol was left at v11 because the existing architecture did not require a wire message to compile or test the model.

## Boundary

Work stopped at Phase 8A. Do not begin Phase 8B.
