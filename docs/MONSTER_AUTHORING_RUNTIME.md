# Monster Authoring and Runtime

Status: FORGE M0-M2.1 merged to `master`; M3 implementation is complete on `forge/mob-lab-m3` / PR #72 pending closeout smoke + final gate. M4 runtime identity/presentation proof is tracked by #75.

## Purpose

Maintain one content-backed Monster authoring/runtime path. Mob Lab now edits that path directly; it does not export to a second model. The existing Red Slime PvE proof remains the normal-session baseline while M4 extends the proof to multiple authored Monster identities.

## Ownership

```text
Mob Lab
→ content/definitions/monsters/*.json
→ purgatory-content validation + ContentRegistry
→ server spawn/bootstrap adapter
→ existing authoritative World/NpcState runtime
```

- Repository JSON is the source of truth. Mob Lab must not own a private
  database.
- `LoadMode::Shared` projects only client-safe Monster presentation identity (`ContentId`, authored id, sprite id). `LoadMode::Full` additionally loads the authoritative gameplay definition.
- `ContentId` identifies the monster definition. `EntityId` identifies one live
  spawn. `NpcState::type_token` remains legacy workload classification and is
  not monster identity.
- `World` and `NpcState` continue to own authoritative movement, targeting,
  contact, Health, death and scheduled respawn.
- Placement belongs to map/server placement content, not `MonsterDefinition`.
- Presentation identity (`sprite`) is projected client-side from the same Monster definition, while creature animation geometry/timing lives in `Graphic/creature/*/manifest.json`. Creature manifest schema v2 supports explicit frame rectangles/origins, sockets, variable-duration steps, and presentation annotations.

## Monster schema v4

```json
{
  "schema_version": 4,
  "id": "monster.slime.red",
  "debug_name": "Red Slime",
  "sprite": "creature.red_slime",
  "health_max": 20.0,
  "collision_bounds": { "left": 0.4, "right": 0.4, "bottom": 0.6, "top": 0.6 },
  "movement_speed": 2.0,
  "behavior": {
    "kind": "chase_contact",
    "aggro": "when_attacked",
    "home_leash_radius": 3.0
  }
}
```

Rules:

- `id` requires a permanent allocation in the Monster `ContentId` block.
- numeric gameplay values must be finite and greater than zero;
- schema v4 accepts only `chase_contact` with `aggro: "when_attacked"`;
- proximity alone never acquires a target; successful player damage assigns that player as target;
- contact damage is independent of aggro: collider touch or overlap within `CONTACT_EPSILON` damages a live player even while the monster is passive;
- `home_leash_radius` limits retention/pursuit after aggro;
- unknown fields and unknown behavior kinds fail validation.

## Current proof boundary

The normal-session Red Slime is resolved through `ContentRegistry`, spawned
with its stable Monster `ContentId`, and receives authored Health, collision
half-extents, movement speed, behavior and home leash. Scheduled
respawn preserves the same `ContentId` and runtime configuration.

The bootstrap location, contact-damage amount and respawn delay remain existing runtime/proof policy rather than authored Monster fields. Sprite identity is authored and resolves through the creature manifest path. One approach/contact path is still Red-Slime-derived globally; M4 / #75 owns making that geometry per Monster.

## Mob Lab sequence

| Slice | Goal | Status |
|---|---|---|
| M0 | Contract and ownership boundary | rescued |
| M1 | JSON loader, validation, registry and stable Red Slime ID | rescued |
| M2 | Content-backed normal-session runtime proof | complete + merged + manually verified |
| M2.1 | Damage-triggered aggro + passive contact behavior | complete + merged + manually verified |
| M3 | Mob Lab v0.1 + creature manifest v2 + DEV spawn wiring | implemented on `forge/mob-lab-m3`; PR #72 draft |
| M4 | Multi-monster runtime identity/presentation + test-arena proof | next; #75 |
| M5 | Hub integration + v0.1 closeout | partially landed; final closeout pending M4 |

M3 edits schema v4 directly rather than creating a second Monster model. It also authors creature manifest v1/v2 data used by the client presentation loader. New behavior kinds, loot, ability loadouts and placement authoring still require consumer-backed slices; they must not be added merely as unused form fields.


## Branch history

The original `forge/mob-lab` and recovered `forge/mob-lab-v01` branches are historical evidence only. M0-M2 were forward-ported rather than merging stale branch history. Current M3 work lives on `forge/mob-lab-m3`; after PR #72 merges, M4 should start from the resulting current `master` on a fresh M4 branch.


### Collision bounds and presentation origin

Monster schema v4 authors collision edges as distances from the entity/presentation origin:

- `left`, `right`
- `bottom`, `top`

This is intentionally asymmetric. A tall creature may keep its entity origin near
the feet by using a small `bottom` and a large `top`. Runtime derives a centered
internal AABB plus collision-center offset from these four distances; presentation
continues to render around the entity origin.


### Sprite selection

Monster schema v4 stores a stable `sprite` id (for example
`creature.red_slime`) in the same Monster JSON as gameplay authoring.

The client-safe content projection exposes only Monster identity + sprite id;
gameplay fields remain server-owned. The client resolves the selected sprite from
`Graphic/creature/*/manifest.json`, and Mob Lab discovers the same manifests.
There is no Monster-to-sprite hardcoded table in Mob Lab.

Current limitation: DEV spawn-by-ContentId is implemented, but live replication still does not carry enough per-entity Monster presentation identity for the client to choose distinct authored sprites for multiple simultaneous Monster types. The server approach/contact envelope also still has a Red-Slime-derived global path. Both gaps are explicitly owned by M4 / issue #75 rather than hidden inside M3.
