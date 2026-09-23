# Monster Authoring and Runtime

Status: FORGE M0-M4 implementation is merged to `master`. Automated identity/de-specialization, per-Monster geometry, and removal of the active Red Slime fixture are proven. GitHub Issue #75 remains open until the manual two-Monster visual/runtime smoke is recorded; M5 closeout follows that evidence.

## Purpose

Maintain one content-backed Monster authoring/runtime path. Mob Lab edits that path directly; it does not export to a second model. M4 removes the former Red Slime fixture assumption and proves multiple authored Monster identities through the same authoritative path.

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

Active authored Monsters resolve through `ContentRegistry` and are spawned with their own stable Monster `ContentId`. The current catalog keeps `10001 / monster.slime.red` retired and reserved; active examples include Moss Crab (`10002`) and Shroom (`10003`). Runtime Health, collision bounds, movement speed, behavior, home leash, sprite identity, and scheduled respawn configuration follow the selected authored Monster rather than a Red Slime default.

The bootstrap location, contact-damage amount and respawn delay remain existing runtime/proof policy rather than authored Monster fields. M4 projects per-entity Monster identity and collision-derived approach geometry; the remaining #75 work is manual acceptance evidence, not missing identity/geometry implementation.

## Mob Lab sequence

| Slice | Goal | Status |
|---|---|---|
| M0 | Contract and ownership boundary | rescued |
| M1 | JSON loader, validation, registry and stable Red Slime ID | rescued |
| M2 | Content-backed normal-session runtime proof | complete + merged + manually verified |
| M2.1 | Damage-triggered aggro + passive contact behavior | complete + merged + manually verified |
| M3 | Mob Lab v0.1 + creature manifest v2 + DEV spawn wiring | complete + merged |
| M4 | Multi-monster runtime identity/presentation + test-arena proof | implementation merged; automated GREEN; manual visual smoke pending; #75 |
| M5 | Hub integration + v0.1 closeout | Hub integration landed; final closeout waits on #75 acceptance evidence |

M3 edits schema v4 directly rather than creating a second Monster model. It also authors creature manifest v1/v2 data used by the client presentation loader. New behavior kinds, loot, ability loadouts and placement authoring still require consumer-backed slices; they must not be added merely as unused form fields.


## Branch history

The original `forge/mob-lab`, recovered `forge/mob-lab-v01`, M3, and M4 working branches are historical evidence only. M4 was integrated with the latest Character Lab/art work and merged into `master`. Continue new Monster work from current `master`; do not revive an old FORGE branch as an execution baseline.


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

M4 adds optional stable `ContentId` to replication Enter records (protocol v30). The client retains that identity per replicated entity and selects each authored Monster sprite independently. Server spawn projects collision-derived approach bounds into each NPC runtime config rather than consulting one global Monster definition. The historical Red Slime ContentId remains reserved, but its active Monster definition is removed.
