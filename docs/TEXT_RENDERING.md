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

Speech and choice text author 14.5 and 14 logical units after visual calibration.
Window/dialog styles no longer pre-scale text. The existing window-header cap
remains layout policy, expressed in logical units before submission.

Choice layout owns its row and hit geometry. The previous 28 physical-pixel
row bypassed UI scaling; no frozen physical-row requirement exists in the N10
presentation contract. Rows now use 28 logical units and the preferred width
uses 420 logical units, still clamped to the assigned physical column. At UI
1.25 × OS 1.5, a row is 52.5px and its single text line is 31.5px. No font
shrinking is introduced. Very long choices/narrow columns can still wrap beyond
one row; responsive multiline bubble layout remains separate work.

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

Explicit bundled fallback chains and complete Regular/Bold/Italic/Bold-Italic
coverage remain unverified. Hebrew/RTL is not a PURGATORY product requirement.
Generic Unicode/advanced shaping remains. Future localization may include
English, Spanish (extended Latin/diacritics), and Chinese (requiring appropriate
CJK assets/fallback in a separate slice); no localization is implemented here
and Chinese/CJK is not production-ready.
Rich spans/markup, localization, emoji, editing and effects are outside 2A.
Advanced shaping already handles every production string; there is no temporary
ASCII mode or manual reversal. Historical Text v0 verification remains in Git
history and is not evidence for this backend.

## Acceptance/calibration — 2026-09-23

Current master was fetched at 17905da64cc9490fe747bbfca25af9c317c0ecf5.
All remaining validation failures are baseline, not introduced:

- Stable 1.98 formatter: app.rs collision-outline calls and NPC debug function
  signature; debug/viz.rs outline call; frontend.rs wrapping throughout;
  skeleton_debug.rs assertion wrapping. Formatter checks on temporary copies
  of master reproduce these hunks. The three latter files are byte-identical
  Git blobs; app changes are confined to the text/choice submission call sites.
- Stable 1.98 Clippy (all client targets/features, warnings denied): unused
  preview_local_player_center/size in skeleton_debug.rs; collapsible settings
  conditional in app.rs; collapsible moon conditional and useless diagnostic
  format call in frontend.rs. All five source locations and the helper usages
  are unchanged from master. The current run reports only these five failures.
- No introduced formatting or Clippy failures remain. Unrelated baseline
  cleanup is explicitly not a push blocker for this acceptance slice.

Static logical-size calibration preserves the hierarchy with approximately
14% smaller em sizes: speech 17→14.5, choice 16→14; dialog body 14→12 and
buttons 13→11; panel title 15→13, section 14→12, rows/currency/quantity/tabs
12→10.5, values/launcher 11→9.5, equipment labels 8→7, tooltips 13→11.
The canonical logical × UI × OS rule is unchanged.

Direct-consumer GPU specimens were inspected at UI 1.0 and 1.25 / OS 1.0,
plus combined scale 1.875 for choice sizing. Speech/choice use their real panels;
settings/dialog use real consumer text positions and panel bounds with neutral
fills (not atlas-art/color acceptance). Text fits the sampled geometry and
retains the intended hierarchy. This is offscreen evidence, not a live monitor
transition test or exhaustive authored-string/skin QA.

Validation (all commands use --offline --locked):

- cargo +1.95.0 test -p purgatory-client renderer::text: 7 passed, 3 ignored.
- cargo +1.95.0 test -p purgatory-client: 736 passed, 4 ignored.
- cargo +1.95.0 check -p purgatory-client --no-default-features: passed.
- cargo +1.95.0 test -p purgatory-client renderer_repeated_text -- --ignored
  --test-threads=1: 3 passed, with PURGATORY_TEXT_SPECIMEN_DIR set.
- Choice measurement/hit-region coverage checks scales 1, 1.25 and 1.875;
  existing choice, panel, dialog and adjacent-column tests pass in the client suite.
- cargo fmt --all -- --check and cargo clippy -p purgatory-client --all-targets
  --all-features --offline --locked -- -D warnings: baseline failures above.
- git diff --check: passed. One wgpu 30.0.1; Glyphon 0.12.0/cosmic-text 0.19.0.
