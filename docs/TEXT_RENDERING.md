# Production text foundation (2A)

Production UI text is owned by `renderer/text.rs` behind `TextBlock`. Glyphon
**0.12.0** owns GPU rendering and atlas residency. Its re-exported cosmic-text
**0.19.0** owns advanced shaping, font matching, wrapping and line layout, with
Swash rasterization. wgpu remains 30.0.1; winit remains 0.30.13. There is one
production text path, without a Text v0 toggle.

## Contract

- `TextStyle.font_size` and `line_height` are logical UI units. The adapter
  multiplies both once by `UI Scale * window.scale_factor()` before creating
  cosmic-text metrics. Glyphon TextArea scale is 1.0.
- Anchors and optional maximum widths are framebuffer pixels, never scaled again.
  The y anchor is the top of the first line box. Left/Center/Right align each
  line around the supplied x anchor, using cosmic-text's shaped positions.
- `color` is linear straight-alpha RGBA. The adapter encodes RGB to sRGB bytes
  for Glyphon's Accurate mode; alpha is encoded directly. Its shader converts
  RGB back to linear before blending.
- Without maximum width, only explicit newlines break lines. With width, cosmic
  WordOrGlyph wrapping applies. Empty authored lines retain vertical spacing;
  empty content has no layout. `at_size` defaults line height to 1.2 times size.
- Measurement uses the same retained shaped buffer submitted to Glyphon. Width
  is maximum line advance width; height and line count come from line boxes.
  These are not ink bounds. Overhangs can extend beyond them. Maximum width is
  wrapping, not scissoring; the framebuffer viewport clips drawing.
- Invalid/nonfinite size, line height, scale, color, anchor or width is rejected.
  The logical-size maximum remains 128. Preparation processes at most 4096
  Unicode scalars and 4096 requests per composition. Truncation respects UTF-8
  boundaries but need not preserve the final grapheme cluster.
- PURGATORY does not independently snap shaped glyph positions.

Speech and choice text now author 17 and 16 logical units. Window/dialog styles
no longer pre-scale text. Geometry remains physical. The existing window-header
size cap is retained as layout policy and expressed in logical units before
submission; this cutover adds no automatic font shrinking.

Fixed bubble geometry is unchanged. At UI Scale 1.25 and OS scale 1.5, the choice
line box is 36px, exceeding the existing 28px physical row. A measurement test
makes this overflow explicit. A later layout decision is needed for that case.

## Fonts and resource ownership

`assets::UI_FONT` embeds the existing `assets/fonts/DejaVuSans-Bold.ttf`.
Retain `DejaVuSans-LICENSE.txt` in distributions. `FontFamily::Ui` is a stable
application identity; fontdb IDs remain internal. The database is populated
explicitly from bundled bytes, with no operating-system font discovery. Only
the existing Bold face is product-supported in 2A; full face coverage is 2B.

FontSystem, Swash context, atlas, viewport and the renderer pool persist.
Unchanged requests retain their shaped buffers. Glyphon caches by face/glyph,
physical size and fractional-position information, and manages atlas allocation,
residency and growth. Renderer slots retain GPU buffers, growing as needed.
There is no additional PURGATORY bitmap cache. Device recreation rebuilds GPU
resources. The old 48px atlas, character substitution and custom shader are gone.

## Ordered compositions

Text draws directly to the surface framebuffer after the world blit. All text
batches are prepared first, using distinct persistent renderer slots. Each UI
composition then draws its panels followed by its own text; later compositions
can occlude earlier text, and the development overlay remains last. A slot is
never re-prepared for another batch before submission. Atlas usage is trimmed
once at the next frame's beginning, never between batches. World Render Scale
and world MSAA do not enter typography metrics or text target resolution.

## Verification

Run the focused tests before the project gate:

```text
cargo +1.95.0 test -p purgatory-client renderer::text
cargo +1.95.0 test -p purgatory-client renderer_repeated_text -- --ignored
cargo +1.95.0 check -p purgatory-client --no-default-features
```

CPU coverage includes empty/explicit lines, wrapping, alignment, measurement,
validation, physical-size cache keys, scale conversion, color conversion and
composed/decomposed accented-character shaping.

The ignored GPU test reads offscreen pixels from two text batches with an
intervening production UI panel. It checks occlusion, earlier-batch survival,
repeatability, translucent non-white blending, and identical UI pixels after
changing the world target from full to half resolution through the production
blit shader. Its multi-size workload exercises atlas pressure.

Set `PURGATORY_TEXT_SPECIMEN_DIR` before the ignored tests to export PNGs at
12/13/14/16/18/24/32 effective pixels, UI scales 1/1.25 and simulated OS scales
1/1.5. Samples include integer/fractional origins, Latin, digits, punctuation,
accents, wrapping, empty lines, transparency and fixed logical sizing. Inspect
at native resolution. This is an offscreen GPU proof, not live monitor-movement
or exhaustive product-window QA. No specimen is added to product UI.

## Deferred to 2B

Hebrew visual QA, RTL/mixed-bidi product acceptance, explicit bundled fallback
chains and complete Regular/Bold/Italic/Bold-Italic coverage remain unverified.
Rich spans/markup, localization, emoji, editing and effects are outside 2A.
Advanced shaping already handles every production string; there is no temporary
ASCII mode or manual reversal. Historical Text v0 verification remains in Git
history and is not evidence for this backend.

## Cutover verification — 2026-09-23

- Rust 1.95.0: focused CPU text suite passed (7); explicit GPU checks passed (2);
  full client suite passed (736, 3 ignored); no-default-features check passed.
- Dependency tree: one wgpu 30.0.1, Glyphon 0.12.0, cosmic-text 0.19.0.
- GPU PNG specimens were compared with a temporary capture of the retired v0
  backend using the same bundled font. Small text is more legible; the new
  font metric convention also makes equal numeric sizes visibly larger. The
  temporary baseline code was removed after capture. Product-window fit and a
  real monitor transition have not been visually accepted.
- The normal check.ps1 gate stops on existing formatting drift in unchanged
  app sections, debug/viz.rs, frontend.rs and skeleton_debug.rs. Stable 1.98
  client Clippy with warnings denied also fails on existing unused skeleton
  helpers, collapsible conditionals and a useless frontend format call. Those
  unrelated areas were not rewritten. Phase 2A acceptance/push remains pending
  resolution of the validation blockers.
