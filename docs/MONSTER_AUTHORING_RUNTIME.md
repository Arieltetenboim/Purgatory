# Monster Authoring and Runtime

Status: FORGE M foundation (`M0-M2`) on `forge/mob-lab`.

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

## Monster schema v1

```json
{
  "schema_version": 1,
  "id": "monster.slime.red",
  "debug_name": "Red Slime",
  "health_max": 20.0,
  "half_extents": [0.4, 0.6],
  "movement_speed": 2.0,
  "behavior": {
    "kind": "chase_contact",
    "acquisition_radius": 3.0,
    "home_leash_radius": 3.0
  }
}
```

Rules:

- `id` requires a permanent allocation in the Monster `ContentId` block.
- numeric gameplay values must be finite and greater than zero;
- `home_leash_radius` must be at least `acquisition_radius`;
- schema v1 accepts only the already-proven `chase_contact` behavior;
- unknown fields and unknown behavior kinds fail validation.

## Current proof boundary

The normal-session Red Slime is resolved through `ContentRegistry`, spawned
with its stable Monster `ContentId`, and receives authored Health, collision
half-extents, movement speed, acquisition radius and home leash. Scheduled
respawn preserves the same `ContentId` and runtime configuration.

The bootstrap location, contact damage, approach contact geometry, respawn
delay and visual asset remain existing runtime/proof policy. They have not been
misrepresented as authored monster fields.

## Mob Lab sequence

| Slice | Goal | Status |
|---|---|---|
| M0 | Contract and ownership boundary | implemented |
| M1 | JSON loader, validation, registry and stable Red Slime ID | implemented |
| M2 | Content-backed normal-session runtime proof | implemented; manual proof required |
| M3 | Mob Lab editor: browse/create/duplicate/edit/save/validate | planned |
| M4 | Test Arena: selected spawn, reset and runtime observations | planned |
| M5 | Client-safe presentation bridge | deferred until the asset path is ready |

M3 must edit schema v1 rather than create a second model. New behavior kinds,
loot, ability loadouts, placement authoring and presentation require their own
consumer-backed slices; they must not be added merely as unused form fields.
