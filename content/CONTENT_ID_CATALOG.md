# Content ID Catalog

This is the checked first-party allocation ledger for stable global content IDs.

Rules:
- an allocated ID is permanent;
- retired IDs remain recorded and are never reused;
- broad domain blocks are defined by `docs/STABLE_NUMERIC_CONTENT_IDS.md` and `purgatory-common::ContentKind`;
- filenames and labels are metadata, not identity;
- item/equipment/equipment-presentation facets share one Item ID.

## Current allocations

### Skills / Abilities — 40,000–49,999

| ID | Label | Status |
| ---: | --- | --- |
| `40001` | `skill.basic.strike` | active |
| `40002` | `skill.debug.practice_sword_strike` | active |

### Maps — 50,000–59,999

| ID | Label | Status |
| ---: | --- | --- |
| `50001` | `map.dev.footnote` | active |
| `50002` | `map.dev.second` | active |

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

The matching equipment gameplay and equipment-presentation facets use the same Item ID as the item row above.

### World Objects / Interactables — 60,000–69,999

| ID | Label | Status |
| ---: | --- | --- |
| `60001` | `entity.interactable.chest` | active |
| `60002` | `entity.interactable.map_b_switch` | active |
| `60003` | `entity.interactable.switch` | active |
| `60004` | `entity.portal.to_footnote` | active |
| `60005` | `entity.portal.to_second` | active |

### Monsters — 10,000–19,999

No authored catalog definitions allocated yet.

### NPCs — 20,000–29,999

No runtime content definitions allocated yet. FORGE/NPC authoring migration is tracked separately in issue #24; do not allocate NPC Lab string IDs implicitly.
