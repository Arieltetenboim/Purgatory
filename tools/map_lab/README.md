# PURGATORY Map Lab — W1.3A

Standalone visual-map calibration tool. It consumes the same shared
`purgatory-content` compiler used to produce the canonical map presentation;
Map Lab does not parse TMX/TSX independently.

## Run

```powershell
cargo run -p purgatory-map-lab
```

The default document is
`content/authoring/maps/map.dev.footnote.purgatory-map.json`.

Use **Fit Map**, zoom/pan, preview-only layer visibility, and the PPU field to
compare the full canonical preview against Tiled. The info panel shows TMX pixel
extent, derived world extent, the locked 23.111 × 13 wu game-camera reference,
and the current player-scale reference. **Apply PPU to Preview** is in-memory
calibration only; source files are not rewritten. **Export Canonical JSON**
writes an inspection artifact under `target/map-lab/`.

W1.3A is visual-only. FOOTNOTE, NPC, Mob, portal, trigger, spawn, and runtime
start-map authoring are intentionally deferred.
