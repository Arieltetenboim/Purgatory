# PURGATORY Recolor Runtime Architecture

Status: **planned architecture / future implementation contract**  
Date: 2026-09-09  
Parent system: [`RECOLOR_MASK_SYSTEM.md`](RECOLOR_MASK_SYSTEM.md)  
Authoring plan: [`CHARACTER_LAB_RECOLOR_PLAN.md`](CHARACTER_LAB_RECOLOR_PLAN.md)

## Purpose

Define the game/runtime half of PURGATORY's generic recolor system in enough detail that the work can be resumed later without reconstructing the original design discussion.

This is intentionally **not a skin system**. Skin is the first proof. The same primitive is intended to support hair, eyes, recolorable equipment/clothing and future presentation assets.

The central rule is:

> A visual may expose one or more semantic recolor slots. Authoring supplies masks. Appearance state supplies palette choices. Client presentation resolves both. The renderer performs one generic recolor operation.

No feature-specific `skin shader`, `hair shader` or `item dye shader` should be introduced unless later evidence proves the generic primitive insufficient.

---

## Current code baseline — observed, not proposed

At the time this document was written:

- `AssetRuntime` owns decoded sprite resources and stable client texture identities.
- `ResolvedVisual` contains the normal sprite texture identity, source rectangle/UVs, pivot, dimensions and pixels-per-unit.
- Domain code owns what a visual means; renderer code owns submission.
- `DrawQuad` has at most one normal `SpriteTextureId` and does not know about bones or slots.
- the primitive WGSL shader samples one sprite texture and multiplies it by the quad color.
- there is no recolor-mask binding, palette runtime or recolor shader path yet.

This makes the existing seam useful: recolor metadata can extend visual/resource resolution while the renderer remains the owner of the actual pixel operation. The implementation should reuse that seam rather than build a separate character-only renderer.

---

## Frozen invariants

The recolor system must not become an alternative ownership model for character presentation.

It must not change:

- humanoid skeleton topology or transforms;
- animation sampling;
- pivots or composition placement;
- semantic draw order;
- Paper Doll attachment ownership;
- `PresentationView` selection;
- gameplay simulation;
- combat/equipment authority;
- left/right facing policy.

PURGATORY has two authored views relevant to this system:

- **Side** — canonical side / three-quarter view toward the user;
- **Back** — character facing away from the user.

There is no third Front authored view. Side and Back remain independent authored visuals and each has matching recolor data.

---

# 1. Semantic data model

## 1.1 Recolor slot

A **recolor slot** identifies an independently selectable color region by meaning, not by texture channel or shader binding.

Examples:

- `skin`
- `hair`
- `eyes`
- `dye_primary`
- `dye_secondary`

The semantic slot is the stable system concept. Physical mask packing is an implementation detail that may change later.

A slot must not encode:

- atlas coordinates;
- texture IDs;
- bone names;
- view selection;
- shader binding numbers.

## 1.2 Ramp roles

The initial generic ramp has three roles:

1. Highlight
2. Base
3. Shadow

The current chibi/cel-shaded art style makes this a useful first primitive: it preserves authored light/base/shadow relationships while permitting materially different colors.

The semantic contract should permit a future asset/profile to use fewer meaningful roles. Eyes, for example, may only require a strong Base contribution. That must not create a separate eye recolor engine.

## 1.3 Palette

A **recolor palette** supplies the colors for the ramp roles of one slot.

Conceptually a palette contains:

- stable palette identity when using authored presets;
- Highlight color;
- Base color;
- Shadow color.

Examples are skin-tone palettes, hair-color palettes or a dye palette.

The runtime contract must not assume palettes are stored inside sprite assets. Art/mask assets define **where** recoloring applies; palettes define **which colors** are applied.

### Initial recommendation

Use explicit authored palette/ramp data for the first implementation rather than generating shadows/highlights from one arbitrary RGB value at runtime. The system can add user-defined/free color generation later if required, but the first proof should prioritize predictable art quality.

## 1.4 Appearance selection

A character or item appearance chooses a palette for a semantic recolor slot.

Conceptually:

- `skin -> skin.medium_01`
- `hair -> hair.black_02`
- `eyes -> eyes.green_01`
- `dye_primary -> dye.red_03`

The appearance layer must store logical choices only. It must not store mask pixels, UVs, atlas rectangles or GPU resource identities.

---

# 2. Ownership boundaries

## 2.1 Authoring tools

Character Lab and future equipment/item editors own:

- assigning semantic recolor slots to authored visuals;
- generating/editing masks;
- validating mask/art alignment;
- exporting technical mask assets and metadata.

They do **not** decide a player's runtime appearance.

## 2.2 Content / asset runtime

The presentation asset layer owns resolution from authored visual identity to renderable resources.

A resolved recolorable visual needs enough information to resolve:

- normal art texture;
- art UV/source rectangle;
- optional recolor-mask binding(s);
- mask texture identity;
- mask UV/source rectangle;
- mask encoding/version;
- semantic recolor slot identity.

The exact Rust type layout is intentionally not frozen yet. The implementation should first inspect the then-current `AssetRuntime`/`ResolvedVisual` ownership and extend the smallest existing seam.

## 2.3 Character/item appearance state

When appearance customization becomes persistent and multiplayer-visible, the authoritative/persistent character or item state should own the **logical palette selections**.

Important boundary:

- server/persistence may own which appearance choices belong to a character/item;
- client presentation owns how those choices become pixels;
- network messages, if required, carry semantic appearance choices, not texture/mask/shader data.

The first local skin rendering proof does not need to solve persistence or protocol changes. Those should be added only when the actual character-appearance feature requires them.

## 2.4 Renderer

The renderer owns the generic pixel operation and GPU resource binding.

It must not know that a slot called `skin` is biologically skin or that `dye_primary` belongs to an item. It receives resolved mask/palette data and executes the same recolor primitive.

---

# 3. Asset binding contract

## 3.1 Art and technical mask are paired resources

For the first Character Lab output:

- normal art atlas and recolor mask atlas have identical dimensions;
- the same visual rectangle addresses corresponding pixels in both;
- masks are exported from the same packing plan as art;
- mask atlases are technical presentation assets, not independent composition sources.

This means existing part crop, pivot and draw-order metadata remains authoritative.

## 3.2 Initial mask encoding

For one recolor slot:

- R = Highlight weight
- G = Base weight
- B = Shadow weight

The mask encoding must be explicit/versioned in metadata.

Mask channels may contain intermediate values to preserve antialiased/transition pixels. The authoring quality gate should prevent nonsensical overlapping weights where the effective total contribution is materially above 1.0 unless a later encoding deliberately defines another rule.

## 3.3 Multiple slots — semantic contract now, physical packing later

A future visual may need more than one independently colored region, for example an item with `dye_primary` and `dye_secondary`.

Do **not** freeze the system to “one RGB texture can represent only one recolor slot.” Instead:

- the semantic asset model should allow zero, one or multiple recolor bindings;
- v1 may implement only one active slot/binding because Skin needs only one;
- later physical implementations may use multiple mask textures, additional channels/textures, texture arrays or another packed representation;
- those optimizations must remain behind the same semantic slot/binding contract.

This avoids prematurely choosing a GPU packing scheme for equipment requirements that do not yet exist.

---

# 4. Render operation

## 4.1 Conceptual input

For each recolorable visual draw, the renderer needs:

- normal art sample;
- mask sample for an active recolor slot;
- selected Highlight/Base/Shadow palette colors;
- existing quad color/modulation and alpha behavior.

## 4.2 Conceptual pixel rule

For the initial RGB ramp mask:

- the R/G/B mask channels weight Highlight/Base/Shadow respectively;
- their combined contribution determines how much of the original art color is replaced;
- the recolored contribution is the weighted combination of the three palette colors;
- pixels with zero mask contribution retain the original art color;
- sprite alpha remains sourced from the normal art so silhouettes/transparent edges stay intact;
- existing quad color/modulation should continue to apply consistently after recolor.

This supports both hard cel-shaded regions and weighted transition pixels.

The exact shader expression is an implementation detail, but these visual invariants are not.

## 4.3 Sampling

Art and mask must sample corresponding UV positions. Recolor must not introduce independent crop math.

The current sprite path uses nearest sampling/no mip assumptions for this presentation art. The implementation should preserve the then-current sprite sampling policy unless a focused visual test proves masks need a different policy. A mask must not bleed between atlas cells.

## 4.4 No per-player texture generation

Changing a character's skin/hair/eye/dye palette must **not** create a new CPU/GPU texture atlas for that character.

The intended model is shared art/mask resources plus small per-draw/per-instance palette data.

This is central to the scalability benefit of the system.

---

# 5. Renderer integration direction

The exact GPU transport should be selected only after inspecting the renderer at implementation time, because batching/instance layout may evolve before this work begins.

Required outcome:

- normal non-recolor sprites stay on a cheap unchanged/fallback path;
- recolorable sprites use the same world presentation ordering;
- a palette change does not require asset re-upload;
- multiple players may share the same art and mask textures while using different palettes;
- recolor does not force a second character renderer;
- renderer batching should not be destroyed unnecessarily by palette selection.

Possible implementation mechanisms include small uniforms, per-instance data or a palette buffer/table. None is frozen by this document.

The current `DrawQuad` only carries one optional sprite texture, so some submission structure will need to grow or a recolor-specific resolved draw payload will need to sit beside it. Prefer the smallest extension that preserves the existing renderer's generic ownership.

---

# 6. Resolution pipeline

The intended data flow is:

1. character/item appearance state selects palettes by semantic slot;
2. Character Presentation chooses the normal visual using existing view/equipment/draw-order rules;
3. asset resolution returns the visual's normal art plus optional recolor bindings;
4. recolor resolution matches each binding's semantic slot with the current appearance palette;
5. presentation emits the normal draw plus resolved recolor payload;
6. renderer samples art/mask and applies the generic ramp operation;
7. final output participates in the existing world pass and draw order.

Recolor never chooses the animation, bone, view or item slot.

---

# 7. Missing data and fallback policy

The system must fail visually safe and diagnostically loud rather than crash normal presentation.

Planned rules:

- visual has no recolor binding -> draw normal art;
- appearance has no selection for an optional recolor slot -> use the asset/content-defined default palette or draw authored default art, according to the eventual asset contract;
- referenced mask asset missing/invalid -> report validation/load error and use normal art where safe;
- art/mask dimensions or rectangles do not align -> authoring/content validation error;
- unknown encoding version -> do not guess channel meaning;
- unknown palette/slot reference -> validation error with deterministic fallback;
- one broken recolor visual must not mutate skeleton/animation state.

The exact strictness at startup vs runtime fallback should follow the project's normal content-validation policy when implemented.

---

# 8. Side / Back behavior

Side and Back are independent authored visuals.

Each may have:

- different art atlas or visual region;
- different technical mask pixels;
- the same semantic slot identity (`skin`, `hair`, etc.);
- the same selected character palette.

Changing `PresentationView` therefore changes which visual/mask pair is resolved, but it does **not** change the character's selected skin/hair/eye palette.

A missing Back visual or mask must follow the existing presentation/content policy; recolor must not invent Side-as-Back fallback behavior.

---

# 9. Permanent base garments

Permanent base garments are part of base-body art and are not removable Paper Doll equipment.

For the skin proof they remain outside the `skin` mask. Their deliberately distinct source colors help automatic mask generation in Character Lab, but runtime correctness comes from the exported mask, not from testing whether a pixel “looks like skin.”

The runtime never performs source-color detection.

---

# 10. Planned implementation slices

These slices are intentionally separate from Character Lab authoring work.

## R-R1 — Generic runtime contract

Define the smallest durable runtime types for:

- semantic recolor slot identity;
- three-role palette/ramp;
- palette selection;
- visual recolor binding metadata.

Do not change the shader yet unless necessary for a tiny compile/test proof.

**Gate:** types represent `skin` without any skin-specific renderer API and can also represent `hair`, `eyes` and future dye slots.

## R-R2 — Asset binding / validation

Extend the existing presentation asset path so a resolved visual can expose its mask binding and encoding.

Validate:

- resource existence;
- dimensions/rect alignment;
- encoding version;
- slot/palette references.

**Gate:** a Side base visual resolves normal art + `skin` mask through the existing asset ownership path.

## R-R3 — Generic renderer primitive

Extend submission/GPU code and shader so one generic recolor slot can be rendered from:

- art texture;
- mask texture;
- Highlight/Base/Shadow palette.

Preserve normal sprites and existing draw order.

**Gate:** a focused renderer test/proof can recolor a masked sprite while leaving unmasked pixels unchanged.

## R-R4 — Skin end-to-end proof

Connect the Character Lab-produced Side mask to the real base character.

Provide a narrow DEV appearance selector if the real character-customization state does not yet exist.

Prove materially different light/medium/dark ramps without generating duplicate textures.

**Gate:** real animated base-body Side art recolors correctly in the normal Character Presentation path.

## R-R5 — Back parity

Once Back art/mask exist, prove the same selected skin palette across Side <-> Back view changes.

**Gate:** view selection changes visuals/masks only; palette selection remains stable.

## R-R6 — Appearance ownership / persistence / replication

Only when actual player customization needs it, decide and implement authoritative/persistent ownership of selected appearance choices and replicate the minimum semantic data required for other clients to render them.

Do not send texture IDs, mask data, UVs or shader details over the protocol.

**Gate:** local and remote clients resolve the same logical appearance through the same recolor renderer path.

## R-R7 — Second semantic-slot proof

Use a real Hair or Eyes asset to prove the system was not accidentally skin-specific.

**Gate:** a second slot uses the same asset binding, palette resolution and renderer primitive with no parallel recolor subsystem.

## R-R8 — Equipment dye extension

Only when real equipment requires it, add/validate multiple independently recolorable regions and choose the physical GPU/mask packing strategy based on measured needs.

**Gate:** equipment dye reuses semantic recolor slots and the same renderer primitive.

---

# 11. Testing strategy

Tests should be proportional and focused.

## Data / validation

- parse/validate recolor slot and palette data;
- reject unknown encoding versions;
- reject misaligned art/mask assets;
- deterministic missing palette/slot fallback;
- Side/Back binding stays view-specific.

## Renderer

Use tiny deterministic textures where possible:

- unmasked pixel equals original art result;
- pure Highlight/Base/Shadow mask samples produce the corresponding palette color;
- weighted mask sample produces the expected blend;
- alpha comes from art and transparent pixels remain transparent;
- different palettes reuse the same art/mask resource identities;
- non-recolor textured quads remain unchanged.

## Character Presentation integration

- normal draw order is unchanged;
- animation transforms are unchanged;
- local/remote presentation can consume the same appearance selection once replication exists;
- Side/Back switches do not reset or reinterpret palette selection.

## Visual/manual proof

For skin, inspect at least:

- light ramp;
- medium ramp;
- dark ramp;
- outlines;
- head/neck shadows;
- limb joins;
- antialiased edges;
- permanent base garments;
- animation poses that expose overlap seams.

---

# 12. Performance constraints

The design exists partly to avoid asset explosion. Therefore:

- one palette choice must not duplicate the art atlas;
- one character must not own a private recolored texture unless future profiling proves such caching is necessary;
- mask textures should be shared by all users of the same visual asset;
- palette data should be small;
- palette changes should be cheap;
- do not sacrifice semantic draw order to batch by palette/texture;
- measure before introducing complex palette atlases, bindless resources or texture arrays.

No performance number is frozen yet. The first proof should establish correctness, then measure the added draw/bind cost in the actual renderer.

---

# 13. Deliberately unresolved decisions

These are **not forgotten requirements**. They are intentionally deferred because current evidence is insufficient.

## Palette choice format

Initial recommendation: authored palette IDs with explicit three-color ramps.

Open later question: whether players may choose arbitrary RGB/base colors and how highlight/shadow ramps are derived while preserving art quality.

## GPU palette transport

Could use uniforms, per-instance fields or a small palette buffer/table. Choose based on the renderer that exists when implemented.

## Multiple recolor slots per visual

Semantic support should exist from the start; physical mask packing should be decided when Hair/Eyes/equipment create a real multi-slot requirement.

## Mask storage packing

Separate paired mask atlas is the first authoring/export choice. Future optimization may combine technical data differently if it preserves the asset contract and tooling clarity.

## Default-palette behavior

Need to decide whether every recolorable visual requires an explicit default palette or may simply fall back to its authored art colors when no selection exists.

## Network/persistence representation

Do not decide protocol representation until appearance customization is a real gameplay/account feature. Whatever is chosen should transport semantic appearance data only.

---

# 14. Stop / escalation conditions for future implementation

Stop and re-evaluate before expanding the design if any of these become true:

- the real art cannot be represented cleanly by the three-role ramp without visible quality loss;
- the renderer would require a second parallel character draw stack;
- multi-slot equipment requires a fundamentally different semantic model rather than only different mask packing;
- palette handling starts affecting simulation/gameplay state;
- Side/Back recoloring requires duplicating appearance choices;
- implementation starts generating per-character recolored textures merely because the shader path is inconvenient.

Those are signals to revisit the contract, not reasons to silently add exceptions.

---

# 15. Definition of success

The generic runtime architecture is proven when:

1. Character Lab exports normal art + aligned technical mask for `skin`.
2. The normal presentation asset path resolves both.
3. A semantic `skin` appearance choice resolves to a three-role palette.
4. The generic renderer recolors only masked pixels.
5. Different characters can use different palettes while sharing the same art/mask resources.
6. Animation, skeleton, pivots, draw order and Paper Doll behavior are unchanged.
7. Side and Back use their own visual/mask assets while retaining the same appearance choice.
8. A later Hair, Eyes or equipment asset can reuse the same semantic and rendering primitive rather than creating a second system.
