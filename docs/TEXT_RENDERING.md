# Text v0

Text v0 is client presentation only, implemented in `apps/client/src/renderer/text.rs`.
It draws plain `TextContent(String)` into the framebuffer through wgpu, after the
world blit and before the existing development overlay. It does not use egui or
change Debug, Connection Frontend, gameplay, simulation, networking or persistence.
The single proof message is enabled by the application only on the Game screen.
Its top-center anchor is at half the framebuffer width, 24 pixels from the top.
Camera movement, world MSAA and internal Render Scale do not affect its layout.

## Contract

- `TextStyle`: font size in physical framebuffer pixels, linear RGBA color in
  `[0, 1]`, and Left / Center / Right alignment around the supplied x anchor.
  The y anchor is the top of the first font line. Each line aligns independently
  using advance width, including spaces. Default: 24 pixels, white, Left.
- Only explicit `\n` breaks lines. Empty lines retain vertical spacing. There is
  no wrapping or shaping. Line height and baseline use the font's own metrics.
- One unmodified embedded font: Hack Regular, with printable ASCII and em dash
  enabled. Unsupported characters (including controls other than `\n`) use `?`
  from the same font. No operating-system font discovery or fallback font.
- Invalid size (nonfinite, nonpositive or above 128), invalid color, invalid
  anchor or zero viewport produces no geometry. At most 4096 input glyphs and
  4096 lines are processed per preparation; excess is truncated deliberately.
- Layout emits internal glyph quads; content has no spans or markup.

## Assets and resources

`assets::UI_FONT` is the single asset entry point. `apps/client/assets/fonts/`
contains Hack-Regular.ttf and its full MIT / Bitstream Vera notices (with DejaVu
contributions in the public domain). The license permits embedding and distribution
with the game; retain the notices. The exact unmodified asset and notices were
copied from the locally installed epaint_default_fonts 0.36.1 package. The new
renderer has no runtime dependency on that package or on egui.

The client directly depends on `ab_glyph` 0.2.32 for font metrics and CPU coverage
rasterization. This small rasterizer already existed in the workspace lockfile;
it avoids adding a shaping/UI stack for the deliberately limited text contract.

One 1024 x 1024 R8 GPU atlas uses padded 64 x 64 cells. The 96 supported glyphs
fit without eviction or growth. Coverage is lazily rasterized at a fixed 48-pixel
font scale and uploaded once per distinct resolved character. Requested font size
scales those cached glyphs; large sizes may appear softer. A changed size, color,
alignment or viewport does not create new glyph resources. One persistent pipeline,
bind group and vertex buffer render one prepared text block. Repeated preparation
recomputes CPU geometry and updates that buffer, but does not recreate GPU objects
or upload cached glyphs. Device recreation constructs a fresh renderer/cache.

## Verification and exclusions

Unit tests cover style validation, size/color application, explicit/empty lines,
no wrapping, all three alignments per line, unsupported glyphs, supported atlas
capacity and cache reuse. The GPU test prepares repeated text with changed size,
alignment and viewport, verifies unchanged upload counts, and submits the actual
text shader/pass to an offscreen target.

Manual checks: connect to a running server; verify the two-line message, resize
and minimize/restore, move the camera, change Render Scale, disconnect/reconnect,
and confirm the Connection Frontend and Debug retain their existing text. Check
both default and `--no-default-features` builds where the existing connection flow
allows Game entry. GPU validation is not proof of visual appearance.

Intentionally deferred: RichText/spans, bold/italic, custom spacing/line height,
wrapping, Hebrew/RTL, localisation, font fallback, emoji, outlines/shadows/effects,
world-space labels, NPC names, chat, damage numbers and text animation. No new
roadmap Phase or Stage is introduced.

The GPU-dependent test is explicitly ignored by the headless workspace gate;
run it with `cargo test -p purgatory-client renderer_repeated_text -- --ignored`.
The default suite tests the same preparation/cache path without requiring an
adapter. Run both `cargo test -p purgatory-client renderer::text` and the explicit
GPU test when changing this renderer.

## Verification record

For implementation commit `78dcc14`, the owner reported that the in-game manual
checklist passed: the message appears only in Game, stays fixed during camera
movement, remains centered through resize and minimize/restore, is independent
of Render Scale, coexists with Debug, and disappears/reappears on disconnect and
reconnect. This is owner-reported runtime evidence, not an automated visual test.
The tested build configuration was not specified; separate visual confirmation
of the `--no-default-features` build is not claimed.

Automated verification passed: seven CPU unit tests, the explicitly run GPU test,
workspace check/tests, workspace Clippy with warnings denied, content validation,
and client check without default features. The canonical `./scripts/check.ps1`
was run but is not green: its formatting step stops on pre-existing formatting in
`crates/common/src/identity.rs` and `crates/common/src/lib.rs` from base commit
`4d3e388`. Those files remain unchanged; subsequent gate commands were run
separately. Manual acceptance does not resolve that quality-gate blocker.
