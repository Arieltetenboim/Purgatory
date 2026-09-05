# Phase 7 Closeout — Weak-Client / Stale-Authority Audit

**Status:** complete  
**Date:** 2026-09-02  
**Root `PHASE`:** `7.closeout`  
**Scope:** Authority contract for currently exposed state-changing paths. Not a full anti-cheat system. Not an inventory/economy proof.

Companion Hub work: Testing → **Phase 7 Stats** (artifact reader for Phase 7.8 gate; HARNESS WARN ≠ SERVER WARN).

---

## Executive answers (B18)

| # | Question | Answer |
|---|---|---|
| 1 | Can snapshot starvation itself create authoritative movement? | **No.** Starvation is loss of client knowledge. Server movement is derived from accepted intents + FOOTNOTE simulation on current `World`. Missing snapshots grant no extra authority. |
| 2 | Can stale client prediction teleport a player on the server? | **No.** Clients do not submit authoritative transforms. `InputCommand` carries axis/jump/down/portal-held + seq/epoch only. |
| 3 | Can a Map A request mutate state after the player is in Map B? | **No** for exposed interact/portal paths: current `WorldAddress` compatibility is checked at execution (`WrongAddress`). |
| 4 | Can a stale `EntityId` target a replacement after despawn/reuse? | **No.** Generation checks yield `StaleId` (sim + gameplay interact/portal validation). |
| 5 | Can duplicate/replayed requests produce duplicate authoritative mutations? | **Movement:** duplicate seq → `Duplicate` (ignored). **Portal:** reentry lock + input gate → single transition. **Interact open:** second open against same/wrong context rejected by session/world rules. No production inventory ops to replay. |
| 6 | Can repeated portal requests create multiple transitions? | **No** under current rules (gate + reentry lock + WrongAddress for old portal). Tested. |
| 7 | Can delayed old-connection packets control a reconnect entity? | **No.** Authority is `ConnectionId` → binding. Detach despawns E1; old packets with no binding are stale/no-ops. Reconnect mint E2 with fresh seq state. Occupancy is CharacterId-gated against dual enter. |
| 8 | Can delayed server-side actions/effects fire against invalid actor/target/world? | **Safe for current runtime actions:** owner despawn cancels pending `CompleteAction` / effects (`cleanup_owned_runtime`). Client cannot keep an action alive by withholding snapshots. No client `ActionRequest` on the wire today. |
| 9 | Open authority risk on exposed paths? | **None unresolved** that can mutate authoritative gameplay state. Residual items are **future contracts** for inventory/trade (not present). |
| 10 | Future pickup/inventory/trade contracts? | See § Future item/economy contracts (B12–B14). |

Classification vocabulary:

- `SAFE_BY_AUTHORITY_DESIGN`
- `SAFE_WITH_TEST_EVIDENCE`
- `FUTURE_TRANSACTION_CONTRACT_REQUIRED`
- `OPEN_RISK`

**Security conclusion (scoped):** For audited exposed paths, a delayed/stale/replaying/reconnecting client cannot force an illegal authoritative mutation. This does **not** claim “cheating impossible” or “item duplication impossible.”

---

## Authority pipeline (B1)

Required:

`client intent/request` → `server validates against CURRENT authoritative state` → `server mutates` → `server replicates`

Observed for:

| Path | Client submits | Server trusts? |
|---|---|---|
| Movement | `InputCommand` (intent + seq/epoch) | Never position/velocity/result |
| Interact open/close | target / session id | Validates actor binding, entity gen, address, range, lifecycle |
| Portal activate | target entity id | `validate_portal_activate` on current world + content transition |
| Dev set channel | channel id (dev) | Membership transition on server |
| Action start | **not on wire** | Server/NPC/runtime only |

Prediction, stale snapshots, delayed render, and missing replication are client-local and never become authority.

---

## Surface classifications

| Surface | Class | Evidence |
|---|---|---|
| Movement / FOOTNOTE | `SAFE_WITH_TEST_EVIDENCE` | Seq/epoch gate; no client pose; portal barrier tests; duplicate seq test |
| Snapshot starvation | `SAFE_BY_AUTHORITY_DESIGN` (+ harness YELLOW note) | Starvation = client miss; Phase 7.8 YELLOW is `HARNESS_STARVE`, not tick fail |
| Interact open | `SAFE_WITH_TEST_EVIDENCE` | WrongAddress / StaleId / OutOfRange / session tests; post-portal Map A interact |
| Portal activate | `SAFE_WITH_TEST_EVIDENCE` | Zone, reentry, gate, WrongAddress, repeated activate, old-world portal |
| Epoch / map transition barrier | `SAFE_WITH_TEST_EVIDENCE` | Input epoch bump; OldEpoch reject; held movement barrier |
| EntityId generation | `SAFE_WITH_TEST_EVIDENCE` | Sim `stale_generation_*`; gameplay stale interact; portal generation |
| Disconnect / reconnect | `SAFE_WITH_TEST_EVIDENCE` | Detach despawn; occupancy; old binding stale; reconnect clean seq |
| Runtime CompleteAction / effects | `SAFE_WITH_TEST_EVIDENCE` | Despawn cancels pending complete; effect target cleanup |
| Client ActionRequest | `SAFE_BY_AUTHORITY_DESIGN` | Not exposed on protocol |
| Pickup / inventory / trade / currency | `FUTURE_TRANSACTION_CONTRACT_REQUIRED` | No production path; contracts frozen below |
| Full anti-cheat / trust scoring | out of scope | Not claimed |

**Unresolved `OPEN_RISK` that mutates authoritative gameplay:** none for current exposed paths.

---

## Scenario coverage (B3–B11, B16)

| Scenario | Expected | Result |
|---|---|---|
| Snapshot starvation + movement | server movement authoritative | Design: no client pose. Harness starve ≠ server authority. |
| Stale movement/input | rejected/absorbed | `Stale` / `OldEpoch` / `Duplicate` / `Gap` |
| Repeated portal activation | single transition | Tested `repeated_portal_activation_single_transition` |
| Old-world request after portal | rejected | `stale_world_interact_after_portal_rejected`, `old_world_portal_after_transition_rejected` |
| Stale epoch request | rejected/ignored | `old_epoch_command_after_portal_is_ignored` |
| Stale EntityId generation | cannot affect replacement | Sim + gameplay tests |
| Disconnect/reconnect + old request | old session no authority | `detach_then_old_connection_*`, occupancy/reconnect tests |
| Repeated action intent | no unintended duplicate | Seq duplicate; action not on wire |
| Target/owner despawn before CompleteAction | cancel/reject | `case_e_despawn_cancels_pending_complete_action` |
| Wrong WorldAddress | rejected | WrongAddress on interact/portal |

### Portal races (B8)

| Scenario | Expected | Status |
|---|---|---|
| A — delayed old-world after activate | no old-world mutation | Covered (WrongAddress) |
| B — repeated activate before dest snapshot | one transition | Covered |
| C — old epoch after entry | rejected/ignored | Covered |
| D — delayed target in source world | reject | Covered |

### Snapshot starvation (B3)

Classification: **safe by architecture** (movement authority never client-pose). Harness `snapshot_starvation` at higher N is a **HARNESS WARN** and must not be collapsed into **SERVER WARN** (Hub Phase 7 Stats enforces label class).

---

## Epoch / defense-in-depth (B6)

Primary guard: **current server state** (binding, WorldAddress, EntityId generation, lifecycle, range, reentry, action gate).

Defense-in-depth: `input_epoch` after map/membership transition; interest replication epoch for presentation baseline.

Sequence/epoch alone is not the only guard; state validation is primary.

---

## Request context contract (B15)

| Candidate field | Attack prevented | Server can derive? | Include? |
|---|---|---|---|
| ConnectionId / session binding | old session control | Yes (transport session) | Server-derived only |
| CharacterId | dual occupancy | Yes after login | Occupancy map; client cannot choose runtime entity |
| WorldAddress in request | stale-map intent | Yes from actor | Optional consistency check only; **never** override server address |
| input_epoch / sequence | replay / reorder | Partially (server epoch) | Keep as replay/order guard |
| operation_id | economy double-commit | N/A today | **Required for future item/trade** |
| Client-supplied pose/HP/inventory | spoof results | N/A | **Forbidden** |

Rule: server-derived truth is primary. Client context is consistency/replay guard only.

---

## Future item/economy contracts (B12–B14)

**Do not claim item duplication is impossible** — no production inventory/trade system exists.

### Item/economy authority (frozen for Phase 8+)

Clients may submit only operation intent, e.g.:

- pick up / drop / move item
- buy / sell / trade / consume

Clients must **never** submit authoritative inventory quantity, currency balance, final ownership, or transaction result.

### Execution-time validation (mandatory)

At commit time the server must verify:

- player/session valid
- player and item in compatible authoritative world context (stale-map pickup → REJECT)
- item exists; ownership current
- pickup/range/state rules
- operation not already committed
- mutation atomic

### Idempotency (B13)

`same operation_id` received multiple times → one authoritative commit → replay returns existing result or safe reject → never execute twice.

Applies to retries, reconnects, packet duplication, timeouts, deliberate replay.

### Atomic ownership transfer (B14)

Forbidden pattern: `add to player` then later `remove world item` with a window where both owners are authoritative.

Required: `validate` → `atomic ownership transition` → `commit` → `replicate`.

### Future acceptance test (documented)

```text
Player authoritatively in Map B
Client still believes Map A (starved/delayed snapshots)
Client sends pickup for item in Map A
→ REJECT (WorldAddress incompatible)
```

Implement when pickup exists; until then the same principle is proven on interact/portal targets.

---

## Tests added / relied on

**New (this closeout):**

- `gameplay.rs`: stale-world interact/portal after transition; repeated portal; old epoch; detach stale; reconnect seq; stale EntityId interact; duplicate seq accounting
- `phase6b_tests.rs`: portal WrongAddress after map move; stale portal generation
- `dev_runtime::phase78`: harness vs server finding class; summary reader

**Existing (cited):** portal barrier / reentry / occupancy / reconnect entity / CompleteAction cancel on despawn / SeqDecision unit tests / phase6b WrongAddress & StaleId interact

---

## Fix policy outcome (B19)

No current authority bug requiring a gameplay mutation fix was found. Closeout adds regression tests and documentation only (plus Hub stats UI).

---

## Phase 8 authorization checklist

1. Phase 7 stats visible in Developer Hub — **yes** (Testing → Phase 7 Stats)
2. Server health vs harness warnings visually distinct — **yes** (`[HARNESS]` / `[SERVER]` labels)
3. Weak-client audit complete — **this document**
4. Exposed state-changing paths validate current authority — **yes**
5. Snapshot starvation = loss of client knowledge — **yes**
6. Stale-world cannot mutate wrong world — **yes** (tested)
7. Stale entity generation cannot hit replacement — **yes** (tested)
8. Old sessions cannot control reconnect entities — **yes** (tested)
9. Replay/duplicate handling documented + tested — **yes**
10. Future item idempotency + atomic transfer recorded — **yes**
11. Focused tests pass — see quality gate
12. `./scripts/check.ps1` passes — see closeout report
13. No unresolved mutating `OPEN_RISK` — **none**

**Do not begin Phase 8 until explicitly instructed after this closeout.**
