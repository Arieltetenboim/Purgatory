# Character Lab — Recolor Authoring Plan

Status: **planned**  
System contract: [`RECOLOR_MASK_SYSTEM.md`](RECOLOR_MASK_SYSTEM.md)

## Goal

Add the first practical authoring path for PURGATORY's generic recolor-mask system inside Character Lab.

The first content target is **base-character skin**. The implementation must remain generic enough that later editor surfaces can author hair, eyes, equipment dyes, and other recolorable regions through the same underlying model.

## Existing Character Lab ownership

Character Lab already owns character-sheet cutting/composition concerns: source-sheet crops, part identity, pivots, composition preview, draw-order authoring, and export metadata.

Recolor authoring belongs here because its mask must follow the exact same source regions and atlas packing as the visual art. It should extend the existing character-authoring document and export path rather than create a parallel recolor tool.

## First implementation scope

### C-R1 — Recolor authoring data

Add the smallest durable generic recolor authoring model.

Required concepts:

- named recolor slot;
- three ramp roles: highlight, base, shadow;
- sampled reference color for each role;
- matching tolerance/settings;
- optional manual corrections;
- view identity inherited from the authored visual (`Side` or `Back`);
- reference to persisted source-mask sidecar when manual pixel corrections exist.

The authoring schema must not use skin-specific field names for the generic layer.

**Gate:** a `skin` slot can be represented without preventing a later `hair`, `eyes`, or `dye_primary` slot.

### C-R2 — Three-sample mask generation

Add a Recolor panel for the selected source/view.

Minimum controls:

- recolor-slot selector/identity;
- eyedropper for Highlight;
- eyedropper for Base;
- eyedropper for Shadow;
- tolerance control;
- Generate / Refresh mask.

Generation compares source-art pixels with the three sampled colors and produces weighted RGB mask data:

- R = highlight;
- G = base;
- B = shadow.

Runtime color detection is forbidden; this operation happens only during authoring.

**Gate:** the current base-body Side source can generate a useful skin mask from three sampled skin colors while permanent base clothing remains outside the mask.

### C-R3 — Live diagnostics / preview

Character Lab should make mistakes visible before export.

Provide three useful views:

- normal source/art preview;
- technical RGB mask preview;
- recolored preview using a selectable test palette.

The test palette is diagnostic only. It does not become the player's persisted appearance contract.

At minimum, verify a light, medium, and dark test ramp so the author can expose bad shadows, missed pixels, or garment contamination.

**Gate:** changing the test skin palette changes only intended skin pixels and preserves the visual light/base/shadow structure.

### C-R4 — Minimal correction path

Automatic color matching is expected to be imperfect on some art.

Add only the smallest correction tools proven necessary by the real base character:

- add/paint into a selected ramp role;
- erase from mask;
- clear/regenerate selected region where useful.

Do not build a general raster editor.

Manual corrections must survive Save/Open. Prefer a referenced technical PNG sidecar for pixel data rather than embedding raw mask pixels in JSON.

**Gate:** an incorrect automatic selection can be repaired without editing the PNG in an external application, and the repair survives reopen.

### C-R5 — Coupled atlas export

Extend the existing/exported atlas path so art and recolor mask are packed from the same placement plan.

For each authored view export:

- normal art atlas;
- matching recolor mask atlas;
- identical dimensions;
- identical visual rectangles;
- explicit metadata that identifies the mask encoding and recolor slot binding.

Character Lab must not independently repack the mask after the art atlas has been built.

**Gate:** for every exported visual rectangle, the mask pixels at that rectangle address the same visual pixels in the art atlas.

### C-R6 — Back-view parity

Once the base Back-view art exists, run the same pipeline on it.

Do not add a Front view. PURGATORY's authored character views for this feature are:

- `Side`: canonical side / three-quarter presentation toward the user;
- `Back`: facing away from the user.

Left/right locomotion remains presentation mirroring, not additional source art.

**Gate:** Side and Back can each export their own art+mask pair using the same recolor-slot definition and authoring flow.

## Runtime follow-up — separate implementation scope

Character Lab produces recolor data but does not itself make the game render alternate colors.

A later runtime slice must:

- load recolor mask metadata/assets through the presentation asset path;
- resolve a palette/ramp selected for a semantic recolor slot;
- apply highlight/base/shadow recoloring during character/equipment rendering;
- leave unmasked artwork unchanged;
- keep animation, skeleton transforms, pivots, draw order, gameplay and network authority unchanged.

Skin should be the first end-to-end proof. Hair, eyes and item dyes are future content/editor uses of the same runtime primitive.

## Base-character art assumptions for the first proof

The new base bodies are expected to stay close to the current approved chibi design and humanoid rig proportions.

Permanent base garments are authored directly into the base body and cannot be removed. They are intentionally distinct in color from skin to make automatic mask generation reliable, but the final mask remains the authority over what recolors.

For the initial skin proof, expected masked regions include visible skin on:

- head;
- upper/lower arms;
- hands;
- upper/lower legs where exposed;
- feet where exposed.

Permanent base shorts/underwear and permanent top pixels remain unmasked.

## Suggested implementation order

1. Recolor authoring data contract.
2. Three eyedroppers + tolerance + automatic RGB mask generation.
3. Technical mask preview + recolored preview.
4. Coupled art/mask atlas export.
5. Validate against the real new Side base body.
6. Add minimal correction tooling only if real art exposes errors.
7. Apply the same authoring path to Back art.
8. Separately implement client-runtime recoloring and prove skin end to end.

## Stop boundary

Stop the Character Lab slice once skin authoring and export are proven cleanly.

Do **not** expand the editor immediately into:

- hair authoring UI;
- eye authoring UI;
- equipment dye UI;
- arbitrary multi-layer painting;
- AI segmentation;
- generalized material/shader authoring.

Those should reuse the recolor contract when actual assets require them.

## Acceptance criteria for the first Character Lab proof

1. Open a real base-character Side source in Character Lab.
2. Define a generic recolor slot named `skin`.
3. Sample highlight/base/shadow colors from the source.
4. Generate a technical RGB mask automatically.
5. Preview materially different skin ramps without recoloring permanent base garments or outlines.
6. Save and reopen the authoring state without losing mask configuration/corrections.
7. Export the normal art atlas and its exactly aligned recolor mask atlas.
8. Repeat the same workflow for Back when Back art is available, with no new recolor architecture.
