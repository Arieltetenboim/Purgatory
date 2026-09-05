# Phase 8C report — Authoritative Equip / Unequip Flow

Status: **GREEN**. Root `PHASE` = `8C`. Protocol **v12**. Phase **8D was not started**.

## 1. Protocol version

`PROTOCOL_VERSION = 12`. v11 has no versioned frame decoder; equipment on Enter/Update is an incompatible layout change. v11 Hello/Welcome goldens remain frozen (`0x0B`). Current Hello/Welcome goldens use `0x0C`. ADR-0057.

## 2. EquipRequest / UnequipRequest wire contract

Reliable control stream. No presentation fields (no BoneTarget, AnchorPoint, visual keys, CoverageMode, hide_base, correction, animation, renderer data).

| Message | Tag | Layout | Bytes with tag |
|---|---:|---|---:|
| EquipRequest | 18 | `seq: u32`, `slot: u8`, `content_id: u64` LE token | 14 |
| UnequipRequest | 19 | `seq: u32`, `slot: u8` | 6 |
| Accepted | 20 | `seq: u32` | 5 |
| Rejected | 21 | `seq: u32`, `reason: u8` | 6 |

Slot is dense `0..=5` (Headwear…Weapon). Invalid slot or `seq == 0` is a codec error on encode.

## 3. Sequence / idempotency policy

Per-connection equipment sequence, **independent** of `InputCommand`. Reuses `SessionInput::classify`:

- First Accept must be `seq == 1`
- `seq == 0` is a codec error (not encoded/decoded). A bypass first `seq == 0` classifies as Gap → `InvalidRequest`
- `seq == last` → Duplicate: resend last Accepted/Rejected; **no** second mutation, rev bump, or dirty
- `seq == last+1` → Accept classification, then validate; store result and advance
- `seq < last` → `StaleRequest`; do not advance (including bypass `seq == 0` after last ≥ 1)
- gap (first seq ≠ 1, or `seq > last+1`) → `InvalidRequest`; do not advance

Reconnect creates a new `PlayerBinding`; the client starts at 1 after Welcome.

## 4. Server validation path

`authorize_equip` in `purgatory-content` uses `equipment_by_id` only (gameplay). Presentation is not loaded.

Equip checks: live player entity, valid slot, gameplay definition exists, `definition.slot == requested slot`, sequence acceptable, transition gate (`StateBlocked`). Exclusive Action busy does **not** block (same as Interact).

Unequip: valid slot + sequence + gate. No content lookup.

## 5. Equipment domain creation semantics

Valid Equip: if `equipment == None`, create `Some(empty)` then write the slot (`World::set_equipment_slot`).

Last Unequip: retain `Some(all-empty)`. Do **not** collapse to `None`. Capability removal is not part of Equip/Unequip.

Unequip with no domain is a no-op and does not create the domain.

## 6. Authoritative mutation path

Only `World` is truth. Equip: slot → `Some(ContentId)`. Unequip: slot → `None`. Same-value Equip is idempotent (no dirty, no rev). Real changes dirty only the target slot and bump only the equipment domain revision. Transform/health are not dirtied.

## 7. Accepted / Rejected model

`ServerControl::Equipment`. Compact reasons: `UnknownContent`, `SlotMismatch`, `StaleRequest`, `InvalidRequest`, `StateBlocked`. Lifecycle metadata only. Persistent truth is replicated `EquipmentState`.

## 8. Full equipment baseline replication

Enter: after optional health, presence `0` = no domain; `1` + occupied-mask + tokens = domain present (including all-empty). Used for initial Enter/AOI, spawn, reconnect/resync. No historical event replay.

## 9. Equipment delta replication

Update domain bit 2. Payload is slot-level dirty only (changed-mask + per dirty slot presence/token). Unchanged slots are omitted. Empty delta is invalid. Stable equipment = no equipment Update traffic.

## 10. Observer revision integration

`CommittedRevs` now has `equipment: u64` and `equipment_state: Option<EquipmentState>`. Deltas are computed as a per-observer slot-wise diff against last committed state. Global `equipment_dirty` is **not** consumed for observer emit. Equipment is appearance: eligible for all relevant relations, not cadence-suppressed, not dropped like stranger health.

## 11. Local / remote replica convergence

One `ReplicatedEntity.equipment: Option<ReplicatedEquipment>` for local and remote. Enter copies full state. Update applies delta (`get_or_insert empty` then `apply_delta`). No separate presentation model.

## 12. AOI behavior

Unchanged interest policy. Enter carries current full equipment. Leave drops the replica (no equipment ghost). Re-enter reconstructs from baseline.

## 13. Reconnect / resync

`reconnect_reconstructs_remote_equipment_from_baseline`: A equips Weapon X, B disconnects, B2 handshakes, Enter carries Weapon X. No EquipAccepted replay.

## 14. Actual wire-size measurements

From protocol encode (tag included for control; domain payload for full/delta):

| Item | Bytes |
|---|---:|
| EquipRequest | 14 |
| UnequipRequest | 6 |
| Accepted | 5 |
| Rejected | 6 |
| Full empty | 1 |
| Full one occupied | 9 |
| Full six occupied | 49 |
| One-slot equip delta | 10 |
| One-slot unequip delta | 2 |

ContentId (8 bytes) dominates occupied payloads. Compact mapping is deferred.

## 15. Idle equipment traffic

After convergence, `two_clients_converge_on_authoritative_equipment` asserts equipment Update counts do not increase across an idle tick window.

## 16. Two-client proof

`apps/server/src/network/tests.rs` `two_clients_converge_on_authoritative_equipment`: two real QUIC clients, mutual AOI, debug `equipment.debug.practice_sword`, both replicas match slot→ContentId, unequip converges to empty domain, idle emits no extra equipment Updates.

## 17. Rejection proof

Same test: sword in Headwear → `SlotMismatch`; authoritative state stays empty; remote sees no false change. Unit tests cover unknown ContentId, stale/gap seq, duplicate seq, last unequip keeps domain.

## 18. Tests and quality gate

Added/extended: `crates/simulation/src/phase8c_tests.rs`, content `authorize_equip_uses_gameplay_only`, protocol size/roundtrip + v12 goldens, server gameplay seq/mutation tests, replication Enter/delta/idle/re-enter, two-client QUIC + reconnect, client replica apply.

Quality gate: `./scripts/check.ps1` **PASS** 2026-09-02 (`fmt --check`, `check --workspace`, `clippy --workspace --all-targets --all-features -D warnings`, `test --workspace`, content validator `equipment=8 defs=15`). `PURGATORY quality gate OK`.

Focused two-client QUIC (also inside workspace tests): `two_clients_converge_on_authoritative_equipment` PASS; `reconnect_reconstructs_remote_equipment_from_baseline` PASS. Server 8C filter: 12 passed. Client `apply_frame_stores_and_deltas_equipment` PASS.

## 19. Files changed

Protocol (`equipment.rs`, `frame.rs`, `message.rs`, `version.rs`, `wire_golden.rs`), content authorize, simulation domain comment + phase8c tests, server gameplay/handshake/replication/replication_policy, client replica/network/lifecycle, bot session/roles, docs (`PROTOCOL`, `ARCHITECTURE`, `DECISIONS` ADR-0057, `ROADMAP`, `README`, `TEST_GATES`, `CONTENT_PIPELINE`, `PERFORMANCE_BUDGETS`, this report), root `PHASE`.

## 20. Open issues

None blocking 8C. ContentId token size is recorded, not optimized. No equipment UI. No compact id map.

## 21. Phase 8D was NOT started

No CharacterPresentationState, skeleton bridge, BoneTarget→BoneIndex, rendering, debug shapes, Overlay/ReplaceBase runtime, hide_base runtime, correction application, animation, climb, combat, inventory, stats, rarity, classes, or equipment UI.
