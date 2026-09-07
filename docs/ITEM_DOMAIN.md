# Item Domain Boundary

Status: Phase 11 design contract — research gate complete

## Purpose

Define the smallest durable authoritative Item domain for Phase 11 without replacing the proven Equipment foundation or pre-building Phase 12+ systems.

## Canonical identities

- `ContentId` identifies an authored item definition/type.
- `ItemInstanceId` identifies one authoritative economic item instance. It is server-minted, opaque, and must never be derived from `ContentId`, `EntityId`, `CharacterId`, or client input.
- `EntityId` remains ephemeral runtime entity identity. A world-drop entity is a manifestation of an item instance, not the item identity itself.
- Existing generic `PersistentId` is not reused as item-instance identity.

For stackable content, `ItemInstanceId` identifies the authoritative stack instance rather than every physical unit. Splitting a stack must mint a new instance identity; merging must retire one identity atomically. Phase 11 does not need to implement split/merge unless required by its vertical slice.

## Definition relationship

`ItemDefinition` is the generic item-content contract and is keyed by the same canonical `ContentId` used elsewhere.

Existing `EquipmentDefinition` remains the equipment capability/compatibility contract. It is not replaced and no second equipment identity is introduced. Equip-capable items use the same `ContentId` across Item and Equipment content, with registry validation enforcing consistency.

The minimal generic item definition should contain only semantics required by the Item loop, such as stackability/stack limit when needed. Presentation and broad stat/progression models do not belong here.

## Authoritative runtime owner

Phase 11 introduces one authoritative Item-domain state owned by gameplay simulation. Conceptually:

```text
ItemInstanceId -> ItemRecord {
    definition: ContentId,
    quantity,
    location,
}
```

The canonical item record exists once. `ItemLocation` is exclusive:

```text
WorldDrop(EntityId)
Inventory { owner: EntityId, slot }
Equipped  { owner: EntityId, slot: EquipmentSlot }
```

Absence from the authoritative table represents destruction/consumption unless a later persistence design requires explicit tombstones.

A character remains the economic owner of inventory/equipped items, but containment is exclusive: an equipped item is not simultaneously present in Inventory.

## Equipment bridge

Existing `EquipmentState` remains authoritative for the proven six-slot gameplay/presentation projection and observer replication. It is not the source of economic ownership.

Equip/unequip operations must be Item-domain transactions:

1. resolve the owned `ItemInstanceId`,
2. validate the referenced definition through existing `authorize_equip` semantics,
3. atomically move the item location between Inventory and Equipped,
4. update the existing `EquipmentState` projection in the same successful transaction.

A failed operation leaves both item ownership and `EquipmentState` unchanged.

## Core invariants

- Every live `ItemInstanceId` appears exactly once in authoritative Item state.
- One item instance has exactly one authoritative location at a time.
- Clients never mint item identities or ownership results.
- `quantity > 0` and must respect the definition's stack constraints.
- Equipped items must have a valid `EquipmentDefinition` and matching `EquipmentSlot`.
- Every occupied public `EquipmentState` slot must correspond to exactly one authoritative equipped item for the same actor/slot/content.
- Every world-drop manifestation must resolve to exactly one authoritative item instance, and vice versa.
- Ownership transitions are atomic: validation failure leaves the source untouched.

These invariants are the basis for duplicate/corruption detection in Phase 11 tests.

## Pickup semantics

The client targets a visible world-drop runtime entity; it does not decide the economic item identity or ownership result.

The server validates actor/state/range/relevance/capacity and then performs one atomic transition from `WorldDrop` to `Inventory`. The world-drop manifestation is removed only after the transition succeeds.

Consequences:

- duplicate pickup: only the first valid transition can succeed;
- two-player race: only one actor can move the instance out of `WorldDrop`;
- full inventory: no item or drop state changes;
- stale/despawned target: rejected/no-op without creating another instance.

Durable disconnect/restart continuity is Phase 12 persistence work; Phase 11 must nevertheless keep the runtime ownership model compatible with that projection.

## Replication and privacy

Public/relevant observers receive only what gameplay/presentation requires:

- world drop: runtime entity identity plus public definition/presentation state such as `ContentId` and quantity when required;
- equipped state: existing six-slot `ContentId` projection.

`ItemInstanceId` and full inventory state are owner-private/server-authoritative data. The owning client may receive instance IDs as required for inventory/equip intents; observers do not need them.

Inventory should not be forced into AOI entity replication merely because owner-only replication exists. Use the smallest owner-private contract appropriate to the implementation slice.

## Protocol decision

This research gate does not bump the protocol version.

Phase 11 implementation will require an intentional protocol review when the first wire-visible Item behavior lands. In particular, the current content-only equip request cannot prove ownership of one exact item when multiple copies may exist. The Item-backed equip intent should identify the owned `ItemInstanceId` (or an equally unambiguous owner-private handle if later evidence justifies one), while the existing public equipment replication remains definition-based.

Every actual wire change must preserve the repository's explicit version/golden-vector discipline; no speculative version bump is authorized by this document.

## Smallest equipment -> gameplay proof

Do not introduce a broad stat/modifier framework in Phase 11.

The preferred proof is to reuse the existing `AbilityGrantTable` boundary: equipped content may act as one explicit grant source for an authored ability. Phase 11E should prove one equipment-driven ability grant/removal through that existing runtime seam, with no generic stats/progression abstraction.

The exact proof content is an implementation detail of 11E and must avoid breaking prior unconditional ability assumptions without focused tests.

## Currency is a separate domain

Currency is deliberately not modeled as ordinary Item instances.

```text
Character Economy
├── Item Domain
│   ├── World Drops
│   ├── Inventory
│   └── Equipment
└── Currency Domain
    ├── CurrencyDefinition
    └── CurrencyBalance
```

Currency uses authoritative balances and credit/debit transactions rather than item-instance ownership, inventory slots, or equipment state.

Two future designs remain valid and intentionally undecided:

1. denominations of one underlying value (for example gold = N silver), best stored canonically in the smallest unit;
2. truly distinct currencies (for example Gold + Stones) with independent balances and rules.

Phase 11 must only preserve this boundary. Currency implementation is deferred until its gameplay requirements are chosen.

## Future persistence projection

Phase 12 character persistence will need to preserve, at minimum, owner-relevant item identity and state:

- `ItemInstanceId`,
- `ContentId`,
- quantity,
- character-relative location (`Inventory(slot)` or `Equipped(slot)`).

Runtime `EntityId` must not be persisted as durable item identity. Saved `ItemInstanceId` values must survive load unchanged, and the eventual minting strategy must guarantee non-reuse across restarts.

World-drop persistence is a separate world/persistence concern and is not defined here.

## Implementation-facing code audit

Targeted inspection of the current repository confirms that Phase 11 needs new Item-domain primitives rather than an Equipment rewrite.

### Proven code that must be reused

- `crates/simulation/src/equipment.rs` already owns the fixed six-slot `EquipmentState`, dirty masks, deltas, and idempotent slot mutation. Its module contract explicitly excludes inventory and combat.
- `crates/content/src/equipment.rs` already owns `EquipmentDefinition`, slot compatibility, `authorize_equip`, and presentation metadata. Its gameplay definition is intentionally slot-only and has no inventory/stat ownership model.
- `World` already owns authoritative gameplay state and Equipment replication dirtiness; the Item runtime should be World-owned rather than a parallel server-side gameplay manager.
- `AbilityGrantTable` already exists as the explicit seam for future equipment/progression/status ability grants and is the correct Phase 11E proof boundary.
- Character persistence v1 currently stores identity/restore context only and deliberately excludes runtime `EntityId`; Phase 12 can project durable item state without changing the Phase 11 runtime identity model.

### Missing code that Phase 11 must create

1. **Identity primitive** — new `ItemInstanceId` in the common identity domain, with server-only minting authority and tests that reject reserved/invalid identity semantics as appropriate.
2. **Item content contract** — new minimal `ItemDefinition` plus loader/registry lookup and validation. Equipment-capable definitions share the same `ContentId` with existing `EquipmentDefinition`; registry validation prevents mismatched identities.
3. **Authoritative Item runtime table** — one World-owned table keyed by `ItemInstanceId`, storing definition, quantity, and exclusive `ItemLocation`.
4. **World-drop bridge** — a runtime drop entity maps one-to-one to an Item record. `EntityId` remains only the visible/runtime pickup target.
5. **Atomic transfer API** — Item-domain operations own `WorldDrop -> Inventory`, `Inventory -> Equipped`, `Equipped -> Inventory`, and destruction/consumption transitions. Callers do not mutate multiple containers independently.
6. **Inventory state/query surface** — bounded owner inventory slots/capacity backed by the canonical Item table; no duplicate Item records inside a second inventory store.
7. **Owner-private synchronization** — minimum reliable/private inventory state needed by the owning client, including instance identity where the client must reference an exact owned item.
8. **Item-backed equipment command path** — replace the current content-only ownership assumption with an intent that resolves one owned `ItemInstanceId`, then reuses `authorize_equip` and writes the existing `EquipmentState` projection atomically.
9. **Invariant/audit tests** — duplicate instance insertion, one-location rule, drop reciprocity, equipment projection reciprocity, pickup race, duplicate request, full inventory, failed transaction rollback, and client-cannot-mint ownership.
10. **Phase 12 projection seam only** — a snapshot shape capable of preserving instance id/content/quantity/character-relative location later; no persistence implementation in Phase 11.

### Explicitly not required for Phase 11

- a second equipment slot/state model,
- generic ECS or inventory service layer,
- database/economy service,
- trading,
- account-wide inventory,
- generic stat/modifier framework,
- persistent world-drop storage,
- stack split/merge unless a concrete Phase 11 gameplay case requires it.

## Recommended Phase 11 implementation slices

1. **11A — Item identity + definitions**: add `ItemInstanceId`, minimal `ItemDefinition`, registry consistency with existing Equipment definitions. No wire change unless concrete need appears.
2. **11B1 — Authoritative runtime Item state + world-drop manifestation**: one canonical item table/location state machine and invariant tests.
3. **11B2 — Pickup transaction**: server validation, atomic world-drop -> inventory move, race/duplicate/full-capacity tests, then the minimum required wire contract.
4. **11C — Inventory**: capacity/slots and owner-private replication/state synchronization.
5. **11D — Equipment ownership bridge**: equip by owned item instance; reuse `authorize_equip` and existing `EquipmentState`; no second slot model.
6. **11E — Gameplay proof**: equipment grants/removes one authored ability through existing `AbilityGrantTable`.

Stop and re-audit before adding generic stats, currency implementation, trading, database services, account-wide inventories, or other Phase 12+ economy systems.

## Research gate closeout

Issue #3's design questions are resolved at the contract level. No existing ADR, Equipment identity, protocol discipline, or persistence ownership rule was found to conflict with this model.

Implementation remains intentionally unstarted. Root `PHASE = 11R` may remain the preparation marker until the first 11A implementation task is explicitly started.