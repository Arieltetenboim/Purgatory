# Content ID Catalog

This is the checked first-party allocation ledger for stable global content IDs.

Rules:
- an allocated ID is permanent;
- retired IDs remain recorded and are never reused;
- broad domain blocks are defined by [Content and Authoring](https://github.com/Arieltetenboim/Purgatory/wiki/Content-and-Authoring) and `purgatory-common::ContentKind`;
- filenames and labels are metadata, not identity;
- item/equipment/equipment-presentation facets share one Item ID.

## Current allocations

### Skills / Abilities — 40,000–49,999

| ID | Label | Status |
| ---: | --- | --- |
| `40001` | `skill.basic.strike` | active |
| `40002` | `skill.debug.practice_sword_strike` | active |
| `40003` | `skill.movement.dash` | active |

### Maps — 50,000–59,999

| ID | Label | Status |
| ---: | --- | --- |
| `50001` | `map.map1` | active |
| `50002` | `map.map2` | active |
| `50003` | `map.map3` | active |

### Items — 30,000–39,999

| ID | Label | Status |
| ---: | --- | --- |
| `30001` | `equipment.debug.cloth_cap` | active |
| `30002` | `equipment.debug.cloth_pants` | active |
| `30003` | `equipment.debug.iron_boots` | active |
| `30004` | `equipment.debug.leather_gloves` | active |
| `30005` | `equipment.debug.plate_cuirass` | active |
| `30006` | `equipment.debug.practice_sword` | active |
| `30007` | `equipment.debug.tunic` | active |
| `30008` | `equipment.debug.unadorned` | active |
| `30009` | `item.debug.iron_scrap` | active |
| `30010` | `item.debug.repair_hammer` | active |
| `30011` | `item.debug.small_potion` | active |
| `30012` | `item.package` | active |
| `30013` | `item.welcome.road_marker_cloth_bundle` | active |
| `30014` | `item.welcome.watch_signal_lantern` | active |
| `30015` | `item.dev.sample_scrap` | active |

The matching equipment gameplay and equipment-presentation facets use the same Item ID as the item row above.

### World Objects / Interactables — 60,000–69,999

| ID | Label | Status |
| ---: | --- | --- |
| `60001` | `entity.interactable.chest` | active |
| `60002` | `entity.interactable.map_b_switch` | active |
| `60003` | `entity.interactable.switch` | active |
| `60004` | `map.map1.portal.001` | active |
| `60005` | `map.map2.portal.001` | active |

`60004` and `60005` previously named the removed DEV definitions `entity.portal.to_footnote` and `entity.portal.to_second`. Those definitions were deleted in a controlled migration, and the owner explicitly approved reusing the IDs for the MAP1 and MAP2 map-owned portals. `portal.001` remains the map-local authoring label; the number is the canonical identity.

### Monsters — 10,000–19,999

| ID | Label | Status |
| ---: | --- | --- |
| `10001` | `monster.slime.red` | retired (reserved; never reuse) |
| `10002` | `monster.moss_crab` | active |

| `10003` | `monster.shroom` | active |
| `10004` | `monster.dev.guaranteed_drop` | active |
| `10005` | `monster.dev.mixed_drops` | active |

### NPCs — 20,000–29,999

| ID | Label | Status |
| ---: | --- | --- |
| `20001` | `npc.welcome.traveler_stayed` | active |
| `20002` | `npc.welcome.gate_watchman` | active |
| `20003` | `npc.welcome.shopkeeper` | active |
| `20004` | `npc.welcome.workshop_craftsperson` | active |
