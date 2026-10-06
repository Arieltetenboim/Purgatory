# PURGATORY UI Kit V2

The master PSD and individual transparent PNGs were authored in Photoshop from editable vector fill layers and non-destructive gradient overlays. The supplied reference was used for visual direction; its pixels were not cropped into assets.

## Connected windows

Use `panel_window_9slice.png` for a complete window with an integrated header, or compose `panel_body_9slice.png` with `panel_header_9slice.png`.

For the separate construction, place the header at **(9, 9)** inside the body. Its height stays **40 px** and its width is **body width minus 18 px**. Stretch only the header's center horizontally. Both components share the same inset and color language; the header is an inner surface, not a second outer frame.

The integrated window's top 64 px are fixed in vertical nine-slice resizing. This preserves the header, junction, and upper corners of the content recess. Keep the bottom 18 px fixed. `asset_manifest.json` records all slice insets in **left, top, right, bottom** order.

## Tabs

`tab_active` and `tab_pressed` have a full-width open bottom between their two side borders. Place their bottom 4 px over the panel's upper rim, so the cream surface joins the content. Active tabs begin at y=2; inactive and hover tabs begin at y=6 to make the active tab visibly taller. Inactive tabs have a closed lower border and a receding gray material. Hover uses blue material and a lighter rim.

## States

- Standard buttons: normal, hover, pressed, disabled. Each state is 112 × 44. Columns 0–9 and 102–111 are the rounded end caps; columns 10–101 are identical, so only that center may stretch. The face is a continuous vertical gradient with no stretchable row, so `sliceLTRB` is `[10, 0, 10, 0]` (horizontal 3-slice). Draw the full 44 px height with the rest of the UI scale. Do not reuse the legacy atlas cap of 8 px.
- Tabs: active, inactive, hover, pressed, disabled.
- Close: normal, hover, pressed, disabled; the white X is approximately 14 px across inside a 36 px control.
- Slots and hotbar slots: normal, hover, selected, disabled.
- Checkbox: unchecked, hover, checked, disabled.
- Radio: off, hover, on, disabled.
- Scroll arrows and utility icon controls: normal, hover, pressed.
- Scroll thumb and menu button containers: normal, hover, pressed, disabled.

Pressed buttons move the face down 2 px, add a dark upper recess, and reverse the lower light. The red close control uses the same depth behavior and keeps the smaller X centered on the moving face.

## Reference coverage

1. Window frame and integrated window, including nine-slice metadata.
2. Large, small, and compact HUD window variants.
3. Header surfaces, including disabled.
4. Standard button states.
5. Tab states with an open active connection.
6. Item and action slot states.
7. Checkbox and radio states.
8. Scroll channel, thumb states, and four arrow directions.
9. Plain and ornamental dividers, slider track, and handle states.
10. Dark gold-edged tooltip and separate pointer.
11. HP, MP, EXP fill textures, empty track, and partial/full HP fill variants.
12. Menu icon button states and separate helmet, bag, sword/shield, scroll, and gear artwork.
13. Hotbar body, slots, and counter badge.
14. Player status body and empty portrait frame.
15. Quest body, divider, and yellow/blue quest markers.
16. Chat body, log, input, and speech icon.
17. Minimap frame, viewport, minimize, and zoom controls.
18. Inventory frame, footer, tab/slot families, coin, and list control.

Additional separate icons include potion, fire, and ice. Character portraits, landscape images, equipment art, and sample text are content supplied by the game; they are not baked into the interface skins.

## Files

- `PURGATORY_UI_KIT_V2.psd`: one named top-level group per exported asset.
- `PNG/`: individual transparent PNGs with no captions, state labels, or checkerboard baked in.
- `asset_manifest.json`: dimensions, layer counts, slice boundaries, and connection metadata.
- `build_ui_v2.jsx`: Photoshop source used to build the kit.
- `QA_report.json`: export and manifest validation results, generated after the build.

The master places groups separately for visual comparison. Exported assets contain only their own artwork. The window variants and named HUD skins are reusable surfaces, not populated gameplay screenshots.

## Inner tabbed content panel

- `PNG/tabbed_content_body_9slice.png`: 288 × 192, transparent rounded corners, 8/8/8/8 slices (left/top/right/bottom). The top center is cream with no horizontal border.
- `PNG/tabbed_content_top_edge.png`: 64 × 8, stretch horizontally at a fixed height of 8 pixels. Its ends have no caps or vertical borders.
- Editable Photoshop sources: `Source/InnerTabbedContent/`.

These assets use the existing `slot_normal` and `tab_active` materials: outline `#5F6C70`, highlight `#EEE8D8`, warm bevel `#B3A68F`, and cream `#E9E0CD`. Keep the body's 8-pixel corner regions fixed. Draw the top edge on each side of the active tab, then draw the active tab over the cream body so its open bottom connects directly to the content surface. Inactive tabs retain their closed bottoms. Both assets are registered in `asset_manifest.json`; runtime composition is supplied by the consuming UI.
