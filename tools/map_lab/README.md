# PURGATORY Map Lab — W1.3A

Standalone visual-map calibration tool. It consumes the same shared
`purgatory-content` compiler used to produce the canonical map presentation;
Map Lab does not parse TMX/TSX independently.

## Run

```powershell
cargo run -p purgatory-map-lab
```

The default document is
`content/authoring/maps/map.map1.purgatory-map.json`.

The map selector lists every `*.purgatory-map.json` sidecar in that authoring
directory. Switching maps reuses the same load path. Unsaved gameplay,
environment, or placement edits must be saved, discarded, or cancelled first.

Use **Fit Map**, zoom/pan, preview-only layer visibility, and the PPU field to
compare the full canonical preview against Tiled. The info panel shows TMX pixel
extent, derived world extent, the locked 23.111 × 13 wu game-camera reference,
and the current player-scale reference. **Apply PPU to Preview** is in-memory
calibration only; source files are not rewritten. **Export Canonical JSON**
writes an inspection artifact under `target/map-lab/`.

W1.3A is visual-only. FOOTNOTE, NPC, Mob, portal, trigger, spawn, and runtime
start-map authoring are intentionally deferred.

## Adding maps

**New Map** is the golden path. Map Lab allocates the next unused map ContentId
from `content/CONTENT_ID_CATALOG.md` (`50001` → `map.map1`, retired IDs are never
reused), writes `Graphic/assets/maps/<ContentId>.tmx` with the canonical 20×20 px
orthogonal grid and 100 px/wu scale, and writes the sidecar, empty gameplay,
empty environment, and empty placement files. The new map opens immediately.
**Open in Tiled** edits that numeric TMX. **Open / Reload** recompiles the visual
source and leaves gameplay, environment, and placements unchanged. Content that
falls outside a later bounds change stays authored and is reported as outside
map bounds.

A newly created map can be visually valid while gameplay is **NOT READY** until
FOOTNOTE and a default spawn exist.

The map selector still lists numeric TMX files under `Graphic/assets/maps` that
have no sidecar, under **Import Existing Numeric TMX**. That path is for
exceptional or legacy files. It is not how new maps are created.

