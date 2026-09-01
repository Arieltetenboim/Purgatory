# 6G.5 Design — Incremental AOI invalidation

## Audit: every `note_interest_change` (pre-6G.5)

| Mutation | Can change relevance? | Same spatial bucket? | Affected observers | Smallest safe set |
|---|---|---|---|---|
| `set_transform` / `set_transform_position` (pose change) | Yes if enter/leave membership can change | May stay in cell | Observers whose leave could include old or new pose; the mover if it is an observer | Spatial influence around old∪new on **same** `WorldAddress` |
| Pose unchanged (velocity-only `bump_transform_rev`) | No | N/A | None | **No invalidation** (already) |
| `spatial_insert` / spawn | Yes | N/A | Observers that could enter the new entity; new player as observer | Influence around spawn pose + self |
| `despawn` / `leave_world` / `clear_transform` | Yes (leave) | N/A | Observers that could have tracked it | Influence around last pose |
| `set_address` (map/channel/instance) | Yes | Grid relocates | Old-address neighborhood + new-address neighborhood | Invalidate both addresses’ influence regions |
| `set_replication` (class/meta) | Yes | Unchanged | Observers that could see it under old or new class | Influence around pose (+ self) |
| `rebind_map_address` | Via per-entity `set_address` | — | Per entity as above | — |
| Unrelated address | — | — | **None** | Zero work |

Global `interest_generation` forced **all** observers to reclassify on any row above → O(observers × candidates) under any motion.

## Proposed model (chosen)

**Per-observer dirty bits + conservative spatial influence invalidation.**

1. Drop global generation as the classify gate.
2. `World` keeps `HashSet<EntityId>` of dirty **observer entities** (players).
3. On interest-affecting mutation at address `A` with poses `P…`:
   - Build AABB = union of expand(P, `AOI_INFLUENCE_HALF_EXTENTS`).
   - `AOI_INFLUENCE_HALF_EXTENTS` = FOOTNOTE-measured max leave extent from an observer
     (edge camera clamp; currently `[28.5, 17.6]`), guarded by AOI unit tests.
   - Mark every **player** in that AABB at address `A` dirty; also mark the subject if it is a player.
4. `publish_observer_frame` classifies iff dirty (or never classified); clears dirty after classify.
5. Different `WorldAddress` → no cross-talk.

### Non-goals

- No reverse entity→observer index (influence query reuses the grid).
- No replication redesign / protocol / semantics change.
- Hotspot mutual visibility may still dirty nearly everyone (correct); **distributed** locality is the material win.

### Correctness note

Influence AABB is a **conservative superset** of observers who might gain/lose the subject.
Too-small influence would miss leaves/enters.
