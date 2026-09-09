# PURGATORY Recolor Mask System

Status: **planned architecture / future implementation contract**  
Date: 2026-09-09  
Runtime detail: [`RECOLOR_RUNTIME_ARCHITECTURE.md`](RECOLOR_RUNTIME_ARCHITECTURE.md)

## Purpose

Define one reusable recoloring system for character and equipment presentation instead of creating separate color-specific art variants or separate skin/hair/eye/dye implementations.

The intended uses include:

- skin tone;
- hair color;
- eye color;
- recolorable clothing and equipment;
- future presentation assets that need controlled palette variation.

The first planned Character Lab use is **skin recoloring**. That first use must exercise the generic contract rather than introduce skin-only runtime concepts.

## View terminology

PURGATORY character presentation has two relevant authored views:

- **Side** — the canonical locomotion view: side / three-quarter presentation toward the user. This is the existing `PresentationView::Side` concept.
- **Back** — the character facing away from the user. This is the existing `PresentationView::Back` concept.

There is no separate Front view in this system. Left/right locomotion facing remains the existing presentation mirror policy and is not a third authored view.

Each authored view owns its own art. Recolor data follows the same view identity; Side and Back must never silently substitute for each other.

## Core model

### Recolor slot

A **recolor slot** is a semantic region whose palette may vary independently.

Examples:

- `skin`
- `hair`
- `eyes`
- `dye_primary`
- `dye_secondary`

The runtime and authoring tools should depend on this generic concept, not on hard-coded skin behavior.

### Color ramp

A recolor slot is driven by a small ordered color ramp. The first implementation uses three roles:

1. highlight;
2. base;
3. shadow.

Three roles fit the current clean, cel-shaded character style and preserve authored light/dark structure better than a single flat tint.

A future slot may use a simpler ramp when appropriate. For example, an eye-color asset may only need a meaningful base color. That should remain a data/profile choice, not require a new recoloring engine.

### Recolor mask

A **recolor mask** is a technical image that identifies how strongly each pixel belongs to the ramp roles. It is not a UV map and does not change character geometry.

For the first encoding:

- Red channel = highlight weight;
- Green channel = base weight;
- Blue channel = shadow weight.

Channel values may contain partial weights at anti-aliased edges; the mask is not required to be strictly binary.

The encoding must be versioned or otherwise explicit in asset metadata so channel meaning is never inferred from convention alone.

## Art atlas + mask atlas relationship

The visual art remains the normal art asset. Recoloring uses a **separate technical mask atlas**.

For a given exported view:

- art atlas and mask atlas have identical dimensions;
- each visual uses the same packed rectangle in both atlases;
- crop/packing decisions are made once and applied to both outputs;
- the mask atlas is derived authoring output, not an independent composition truth.

This keeps existing `rect_px`, pivot, bone and draw-order semantics intact. Recoloring must not create a parallel skeleton, composition, or attachment system.

A Side art atlas therefore pairs with a Side recolor mask atlas. A Back art atlas pairs with a Back recolor mask atlas.

## Authoring-source ownership

The source character sheet is a catalog/cutting source. Its source-sheet position does not define the composed character position.

Character Lab should generate and edit recolor masks in source/crop space and then use the exact same crop and packing operations used for the art atlas to produce the technical mask atlas.

The durable authoring state should preserve:

- recolor-slot identity;
- reference colors used for automatic generation;
- tolerance/settings needed to reproduce generation;
- manual corrections;
- source-mask asset references where pixel-level corrections are persisted.

Large pixel masks should not be serialized as raw pixel arrays inside `char.sheet.json`. A source-aligned or part-aligned technical PNG sidecar is preferable when pixel edits must be durable; the authoring JSON references that asset.

## Automatic mask generation

Automatic generation is an authoring convenience, not runtime behavior.

For a selected recolor slot, Character Lab may let the author sample three reference colors from the source art:

- highlight;
- base;
- shadow.

The tool finds pixels sufficiently close to those samples, assigns weighted mask channels, and previews the result. A tolerance control determines how broadly nearby colors are accepted.

The generated mask is only a proposal. The final authored mask is the source of truth after optional manual correction.

No AI segmentation is required for v1.

## Manual correction

Automatic color matching can fail when unrelated art uses similar colors or when shading contains unexpected intermediate values.

Character Lab should therefore eventually provide minimal correction tools:

- add to the selected ramp role;
- erase from the recolor mask;
- inspect the generated mask over the original art.

This is deliberately not a general image editor.

## Base body and permanent base clothing

The base character contains permanent base garments as part of its base art. They are not removable Paper Doll equipment.

Examples may include base shorts/underwear and a permanent top where a body variant requires one.

These pixels must remain outside the `skin` recolor mask. Using deliberately distinct garment colors is useful because it makes automatic skin-mask generation more reliable, but correctness must ultimately come from the authored mask rather than from a runtime color heuristic.

Runtime recoloring never scans the finished art for "skin-colored" pixels.

## Runtime boundary

Recolor application belongs to client presentation/rendering.

The game may eventually own or persist appearance choices such as skin tone, hair color, eye color, or item dye selection. Those choices are logical appearance data. They must not contain atlas coordinates, pixel masks, or shader-specific details.

The runtime is intentionally split into distinct responsibilities:

- **appearance state** selects palettes by semantic recolor slot;
- **asset resolution** binds normal art to optional recolor-mask metadata;
- **palette resolution** turns the semantic choice into highlight/base/shadow colors;
- **presentation** emits the already-selected visual plus recolor payload without choosing animation/view differently;
- **renderer** performs one generic Art + Mask + Palette operation;
- **persistence/networking**, when later required, transport semantic appearance choices only.

The renderer resolves:

1. the normal visual asset;
2. its recolor-slot/mask binding;
3. the selected palette/ramp for that slot;
4. the final rendered pixels.

Unmasked pixels remain the authored art. Masked pixels preserve the authored highlight/base/shadow structure while receiving the selected palette.

The first runtime proof must not create a per-character recolored texture. Art and masks are shared assets; palette data is the variable input.

The system must not affect:

- simulation;
- skeleton transforms;
- animation sampling;
- pivots;
- draw order;
- gameplay authority.

Full runtime ownership, rendering rules, validation/fallbacks, performance constraints, multi-slot strategy and staged implementation are frozen in [`RECOLOR_RUNTIME_ARCHITECTURE.md`](RECOLOR_RUNTIME_ARCHITECTURE.md).

## Scaling beyond skin

The same semantic system should support later uses without redesign:

### Hair

A hair visual may bind to the `hair` slot and use highlight/base/shadow masks. Front-hair, helmet-compatible hair, and Back-view hair remain ordinary view-specific visuals; recoloring is orthogonal to their geometry.

### Eyes

An eye visual may bind to `eyes`. It may use the full ramp or a simpler profile where only the base role contributes meaningfully.

### Equipment / dyes

An equipment visual may bind to one or more dye slots when real content proves that requirement. Multi-zone equipment should be represented as multiple semantic recolor slots rather than adding item-specific shader logic.

The first implementation does **not** need to solve optimized packing of many simultaneous dye zones. The semantic contract should permit multiple slots; physical mask packing can evolve behind that contract when measured needs justify it.

## Initial implementation decision

The first Character Lab implementation will use the generic system for **skin**:

- one `skin` recolor slot;
- three ramp roles: highlight, base, shadow;
- eyedropper/reference-color selection;
- tolerance-based automatic generation;
- live mask and recolored preview;
- paired art/mask atlas export;
- manual correction added when required by real base art;
- Side first as the immediate authored base, with Back using the same contract as soon as Back art is available.

This is a proof of the generic recolor pipeline, not a skin-specific architecture.

## Non-goals for the first slice

- AI image segmentation;
- UV mapping;
- runtime color detection from finished sprites;
- separate PNG character sets for each skin tone;
- hair/eye/equipment editor UI before those assets need it;
- a general-purpose paint application;
- changes to the humanoid rig or animation system;
- a new Front presentation view;
- removing or recoloring permanent base garments as part of skin tone.

## System-level acceptance criteria

The architecture is proven when:

1. one authored visual can be recolored through a named recolor slot without duplicating its art;
2. highlight/base/shadow relationships remain visually stable across materially different palettes;
3. unmasked pixels and permanent base garments are unchanged;
4. art and mask atlases remain pixel-aligned through Character Lab export;
5. the same runtime path can later accept `hair`, `eyes`, or item-dye slots without introducing a second recolor engine;
6. Side and Back remain explicit, independent authored views using the same recolor contract;
7. different characters may share the same art/mask resources while selecting different palettes;
8. a second semantic slot can be added without a feature-specific shader/runtime subsystem.
