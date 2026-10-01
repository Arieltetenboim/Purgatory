# Item Lab

Local authoring tool for canonical item content. It is not a website and it does not keep a second item registry.

## Open it

Developer Hub → **Item Lab**, on the dashboard or the Content page. The button runs `tools/item_lab/run.ps1` from the detected workspace. A second click reuses the running Item Lab. If port 8767 is held by another process, the launcher stops and reports the conflict. It does not kill that process.

Closing the Item Lab PowerShell window stops the server.

## Requirements

Python 3 on `PATH` (`py -3` or `python`). No database credentials and no migrations.

## Content

| File | Role |
| --- | --- |
| `content/shared/items/*.json` | Gameplay definition |
| `content/shared/item_presentation/*.json` | Icon, display name, description |
| `content/shared/equipment/*.json` | Slot for an equipment item, same Content ID and label |
| `content/authoring/item_notes/*.json` | Notes and search tags, not gameplay |
| `Graphic/items/<visual key>.png` | Optional 32×32 RGBA icon |
| `crates/common/src/content_catalog.rs` | Permanent numeric ID |
| `content/CONTENT_ID_CATALOG.md` | Ledger, including retired rows that are never reused |

Editable fields: display name, description, category (only while creating), stack limit (increases only), drop-confirmation flag, icon key, notes, and tags. Category, stack-limit reduction, and equipment-slot changes after creation are rejected.

A new ID is compiled into the catalog. Rebuild the client and server before the game can load it. A JSON-only edit needs a restart of processes that already loaded content.

Catalog writes share `content/.authoring-catalog.lock` with Mob Lab.
