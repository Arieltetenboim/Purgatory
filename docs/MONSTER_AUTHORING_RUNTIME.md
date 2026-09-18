# Monster Authoring and Runtime

Status: FORGE M foundation (`M0-M2`) merged to `master`; M2.1 aligns damage-triggered aggro before Mob Lab UI.

## Purpose

Establish one content-backed monster path before building the Mob Lab user
interface. The existing Red Slime PvE proof remains the runtime proof, but its
gameplay identity and editable parameters come from validated repository JSON
instead of server constants.

## Ownership

```text
Mob Lab (future editor)
→ content/definitions/monsters/*.json
→ purgatory-content validation + ContentRegistry
→ server spawn/bootstrap adapter
→ existing authoritative World/NpcState runtime
```

- Repository JSON is the source of truth. Mob Lab must not own a private
  database.
- Monster definitions are server-only gameplay content. They are not loaded in
  `LoadMode::Shared`.
- `ContentId` identifies the monster definition. `EntityId` identifies one live
  spawn. `NpcState::type_token` remains legacy workload classification and is
  not monster identity.
- `World` and `NpcState` continue to own authoritative movement, targeting,
  contact, Health, death and scheduled respawn.
- Placement belongs to map/server placement content, not `MonsterDefinition`.
- Presentation belongs to a separate client-safe definition and asset path. It
  is deliberately not part of schema v1.

## Monster schema v3

```json
{
  "schema_version": 3,
  "id": "monster.slime.red",
  "debug_name": "Red Slime",
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
- schema v3 accepts only `chase_contact` with `aggro: "when_attacked"`;
- proximity alone never acquires a target; successful player damage assigns that player as target;
- contact damage is independent of aggro: collider touch or overlap within `CONTACT_EPSILON` damages a live player even while the monster is passive;
- `home_leash_radius` limits retention/pursuit after aggro;
- unknown fields and unknown behavior kinds fail validation.

## Current proof boundary

The normal-session Red Slime is resolved through `ContentRegistry`, spawned
with its stable Monster `ContentId`, and receives authored Health, collision
half-extents, movement speed, behavior and home leash. Scheduled
respawn preserves the same `ContentId` and runtime configuration.

The bootstrap location, contact damage, approach contact geometry, respawn
delay and visual asset remain existing runtime/proof policy. They have not been
misrepresented as authored monster fields.

## Mob Lab sequence

| Slice | Goal | Status |
|---|---|---|
| M0 | Contract and ownership boundary | rescued |
| M1 | JSON loader, validation, registry and stable Red Slime ID | rescued |
| M2 | Content-backed normal-session runtime proof | complete + merged + manually verified |
| M2.1 | Damage-triggered aggro + passive contact behavior | complete + merged + manually verified |
| M3 | Mob Lab v0.1 editor: browse/create/duplicate/edit/save/validate | planned |
| M4 | Real-runtime Test Arena: selected spawn, reset and observations | planned |
| M5 | Hub integration + v0.1 closeout | planned |

M3 must edit schema v3 rather than create a second model. New behavior kinds,
loot, ability loadouts, placement authoring and presentation require their own
consumer-backed slices; they must not be added merely as unused form fields.


## Branch recovery note

The original `forge/mob-lab` branch diverged far behind current `master`.
It is retained only as historical evidence. M0-M2 were forward-ported onto
`forge/mob-lab-v01` by applying the monster-specific delta to the current
owners and APIs; no old branch history was merged. Future FORGE M work must
continue from the recovered branch or a fresh branch based on a current master.


### Collision bounds and presentation origin

Monster schema v3 authors collision edges as distances from the entity/presentation origin:

- `left`, `right`
- `bottom`, `top`

This is intentionally asymmetric. A tall creature may keep its entity origin near
the feet by using a small `bottom` and a large `top`. Runtime derives a centered
internal AABB plus collision-center offset from these four distances; presentation
continues to render around the entity origin.
