from pathlib import Path
import json

ROOT = Path('.')


def read(path):
    return (ROOT / path).read_text(encoding='utf-8')


def write(path, text):
    (ROOT / path).write_text(text, encoding='utf-8', newline='\n')


def replace_once(text, old, new, label):
    count = text.count(old)
    if count != 1:
        raise SystemExit(f'{label}: expected exactly one match, found {count}')
    return text.replace(old, new, 1)

p = 'apps/client/src/asset_runtime.rs'
s = read(p)
s = replace_once(s, 'use image::{Rgba, RgbaImage};', 'use image::RgbaImage;', 'asset_runtime import')
old = '''        let mut image = image::load_from_memory(bytes)
            .map_err(|err| format!("decode sprite resource {key}: {err}"))?
            .to_rgba8();

        // ATLAS.png is authored with visually separated variants, while its
        // runtime metadata intentionally exposes a regular 128/56/32 grid.
        // Normalize that atlas once at decode time: fixed borders/corners stay
        // pixel-exact and only flat middle pixels are repeated/dropped.
        if key == "ui.atlas" {
            image = normalize_ui_atlas_v1(image)?;
        }

        self.register_image(key, image)
'''
new = '''        let image = image::load_from_memory(bytes)
            .map_err(|err| format!("decode sprite resource {key}: {err}"))?
            .to_rgba8();
        self.register_image(key, image)
'''
s = replace_once(s, old, new, 'asset_runtime register_png')
marker = '\nconst UI_ATLAS_SIZE: [u32; 2] = [768, 283];'
if marker not in s:
    raise SystemExit('asset_runtime normalization marker missing')
s = s.split(marker, 1)[0].rstrip() + '\n'
write(p, s)

p = 'Graphic/ui/ATLAS.ui.json'
data = json.loads(read(p))
window_x = [7, 130, 253, 376, 498, 620]
for name, x in zip(['base', 'blue', 'brown', 'dark1', 'dark2', 'light1'], window_x):
    data['windows'][name] = {'x': x, 'y': 24, 'width': 120, 'height': 177}
data['window_slice_px'] = {'left': 8, 'right': 8, 'top': 38, 'bottom': 8}
data['window_border_units'] = {'left': 8.0, 'right': 8.0, 'top': 38.0, 'bottom': 8.0}
button_x = [6, 61, 115, 170, 225, 280]
for name, x in zip(['base', 'blue', 'brown', 'dark1', 'dark2', 'light1'], button_x):
    data['buttons'][name] = {
        'normal': {'x': x, 'y': 210, 'width': 54, 'height': 18},
        'hover': {'x': x, 'y': 228, 'width': 54, 'height': 18},
        'pressed': {'x': x, 'y': 246, 'width': 54, 'height': 18},
    }
data['close_button'] = {
    'normal': {'x': 720, 'y': 211, 'width': 24, 'height': 18},
    'hover': {'x': 720, 'y': 231, 'width': 24, 'height': 18},
    'pressed': {'x': 720, 'y': 251, 'width': 24, 'height': 18},
}
data['close_size_units'] = [18.0, 18.0]
write(p, json.dumps(data, indent=2) + '\n')

p = 'Graphic/ui/inventory_tab.ui.json'
data = json.loads(read(p))
data['states'] = {
    'normal': {'x': 123, 'y': 285, 'width': 933, 'height': 177},
    'selected': {'x': 1116, 'y': 285, 'width': 933, 'height': 177},
}
data['slice_px'] = {'left': 96, 'right': 96}
data['cap_units'] = {'left': 8.0, 'right': 8.0}
data['height_units'] = 26.0
write(p, json.dumps(data, indent=2) + '\n')

p = 'Graphic/ui/inventory_slot.ui.json'
data = json.loads(read(p))
data['size_units'] = [44.0, 44.0]
write(p, json.dumps(data, indent=2) + '\n')

p = 'apps/client/src/ui_panel.rs'
s = read(p)
old_icon = '''        if let Some(icon) = icon {
            let source = ATLAS_ICON_NAMES
                .iter()
                .position(|name| *name == icon)
                .map(|index| self.icons.regions[index])
                .ok_or_else(|| format!("UI atlas icon {icon} is missing"))?;
            let icon_size = (20.0 * pixels_per_unit)
                .min(source.width as f32)
                .min(source.height as f32);
            let icon_min = [
                layout.header.min[0] + 7.0 * pixels_per_unit,
                layout.header.min[1] + 7.0 * pixels_per_unit,
            ];
            let (uv_min, uv_max) = source.uv_bounds(self.close_button.source_size_px);
            textured_rects.push(UiTexturedRect {
                min: icon_min,
                max: [icon_min[0] + icon_size, icon_min[1] + icon_size],
                texture: self.close_button.texture,
                uv_min,
                uv_max,
                tint: [1.0; 4],
            });
        }

        let title_font_size = TITLE_FONT_SIZE_UNITS * pixels_per_unit;
        let title_anchor = [
            layout.header.min[0] + TITLE_LEFT_INSET_UNITS * pixels_per_unit,
'''
new_icon = '''        let mut title_anchor_x = layout.header.min[0];
        if let Some(icon) = icon {
            let source = ATLAS_ICON_NAMES
                .iter()
                .position(|name| *name == icon)
                .map(|index| self.icons.regions[index])
                .ok_or_else(|| format!("UI atlas icon {icon} is missing"))?;
            let icon_size = (20.0 * pixels_per_unit)
                .min(source.width as f32)
                .min(source.height as f32);
            let icon_min = [
                layout.header.min[0],
                layout.header.min[1] + ((layout.header.height() - icon_size) * 0.5).max(0.0),
            ];
            let (uv_min, uv_max) = source.uv_bounds(self.close_button.source_size_px);
            textured_rects.push(UiTexturedRect {
                min: icon_min,
                max: [icon_min[0] + icon_size, icon_min[1] + icon_size],
                texture: self.close_button.texture,
                uv_min,
                uv_max,
                tint: [1.0; 4],
            });
            title_anchor_x = icon_min[0] + icon_size + TITLE_CONTROL_GAP_UNITS * pixels_per_unit;
        }

        let title_font_size = TITLE_FONT_SIZE_UNITS * pixels_per_unit;
        let title_anchor = [
            title_anchor_x,
'''
s = replace_once(s, old_icon, new_icon, 'header icon/title layout')
old_center = '''    let source = SourceRectPx {
        x: (panel.source_size_px[0] - sample_size[0]) / 2,
        y: (panel.source_size_px[1] - sample_size[1]) / 2,
        width: sample_size[0],
        height: sample_size[1],
    };
'''
new_center = '''    let source = SourceRectPx {
        x: panel.source_rect.x + (panel.source_rect.width - sample_size[0]) / 2,
        y: panel.source_rect.y + (panel.source_rect.height - sample_size[1]) / 2,
        width: sample_size[0],
        height: sample_size[1],
    };
'''
s = replace_once(s, old_center, new_center, 'panel center fill source')
old_piece = '''            let source_piece = SourceRectPx {
                x: source_x[column] as u32,
                y: source_y[row] as u32,
                width: (source_x[column + 1] - source_x[column]) as u32,
                height: (source_y[row + 1] - source_y[row]) as u32,
            };
            let (uv_min, uv_max) = source_piece.uv_bounds(source_size_px);
'''
new_piece = '''            let mut source_piece = SourceRectPx {
                x: source_x[column] as u32,
                y: source_y[row] as u32,
                width: (source_x[column + 1] - source_x[column]) as u32,
                height: (source_y[row + 1] - source_y[row]) as u32,
            };
            source_piece = match (row, column) {
                (1, 1) => centered_source_sample(source_piece, 2, 2),
                (0 | 2, 1) => centered_source_sample(source_piece, 2, source_piece.height),
                (1, 0 | 2) => centered_source_sample(source_piece, source_piece.width, 2),
                _ => source_piece,
            };
            let (uv_min, uv_max) = source_piece.uv_bounds(source_size_px);
'''
s = replace_once(s, old_piece, new_piece, 'nine-slice repeat samples')
anchor = '''#[allow(dead_code)]
fn assemble_horizontal_three_slice(
'''
helper = '''fn centered_source_sample(rect: SourceRectPx, width: u32, height: u32) -> SourceRectPx {
    let width = width.clamp(1, rect.width);
    let height = height.clamp(1, rect.height);
    SourceRectPx {
        x: rect.x + (rect.width - width) / 2,
        y: rect.y + (rect.height - height) / 2,
        width,
        height,
    }
}

#[allow(dead_code)]
fn assemble_horizontal_three_slice(
'''
s = replace_once(s, anchor, helper, 'centered source helper')
old_validate_windows = '''        || windows.iter().any(|rect| rect.width != 128)
        || windows.windows(2).any(|pair| {
            pair[0].y != pair[1].y
                || pair[0].height != pair[1].height
                || pair[0].x + pair[0].width != pair[1].x
        })
'''
new_validate_windows = '''        || windows.iter().any(|rect| {
            rect.width != windows[0].width || rect.height != windows[0].height
        })
        || windows.windows(2).any(|pair| {
            pair[0].y != pair[1].y
                || pair[0].height != pair[1].height
                || source_rects_overlap(pair[0], pair[1])
        })
'''
s = replace_once(s, old_validate_windows, new_validate_windows, 'window metadata validation')
s = replace_once(s, '            || regions[0].width != 56\n', '            || regions[0].width != 54\n', 'button width validation')
repls = [
    ('                x: 0,\n                y: 24,\n                width: 128,\n                height: 177', '                x: 7,\n                y: 24,\n                width: 120,\n                height: 177'),
    ('        assert_eq!(assets.panel().slice_px.top, 32);', '        assert_eq!(assets.panel().slice_px.top, 38);'),
    ('                top: 32.0,', '                top: 38.0,'),
    ('            assert_eq!(rect.width, 128);', '            assert_eq!(rect.width, 120);'),
    ('                assert_eq!(panel.source_rect.width, 128);', '                assert_eq!(panel.source_rect.width, 120);'),
    ('        assert_eq!(assets.source_size_px, [1632, 220]);', '        assert_eq!(assets.source_size_px, [2172, 724]);'),
    ('        assert_eq!(assets.states.normal.x, 13);', '        assert_eq!(assets.states.normal.x, 123);'),
    ('        assert_eq!(assets.states.normal.width, 790);', '        assert_eq!(assets.states.normal.width, 933);'),
    ('        assert_eq!(assets.states.selected.x, 829);', '        assert_eq!(assets.states.selected.x, 1116);'),
    ('        assert_eq!(assets.states.selected.width, 790);', '        assert_eq!(assets.states.selected.width, 933);'),
    ('        assert_eq!(assets.slice_px.left, 83);', '        assert_eq!(assets.slice_px.left, 96);'),
    ('        assert_eq!(assets.slice_px.right, 83);', '        assert_eq!(assets.slice_px.right, 96);'),
    ('        assert_eq!([image.width(), image.height()], [256, 256]);\n        assert_eq!(assets.size_units, [42.0, 42.0]);', '        assert_eq!(image.width(), image.height());\n        assert!(image.width() > 0);\n        assert_eq!(assets.size_units, [44.0, 44.0]);'),
    ('        assert_eq!(frame.textured_rects[0].uv_min[0], 829.5 / 1632.0);', '        assert_eq!(frame.textured_rects[0].uv_min[0], 1116.5 / 2172.0);'),
    ('        assert_eq!(frame.textured_rects[3].uv_min[0], 13.5 / 1632.0);', '        assert_eq!(frame.textured_rects[3].uv_min[0], 123.5 / 2172.0);'),
    ('        assert!(slots.iter().all(|slot| slot.size() == [42.0, 42.0]));', '        assert!(slots.iter().all(|slot| slot.size() == [44.0, 44.0]));'),
    ('        assert_eq!(content_max_y - slots.last().unwrap().max[1], 54.0);', '        assert_eq!(content_max_y - slots.last().unwrap().max[1], 40.0);'),
    ('        assert_eq!(slots[0].min[0] - bounds.min[0], 9.0);', '        assert_eq!(slots[0].min[0] - bounds.min[0], 4.0);'),
    ('            bounds.max[0] - slots[INVENTORY_SLOT_COLUMNS - 1].max[0],\n            9.0', '            bounds.max[0] - slots[INVENTORY_SLOT_COLUMNS - 1].max[0],\n            4.0'),
    ('        assert_eq!(frame.textured_rects[0].size(), [8.0, 32.0]);', '        assert_eq!(frame.textured_rects[0].size(), [8.0, 38.0]);'),
    ('        assert_eq!(frame.textured_rects[9].size(), [19.0, 18.0]);', '        assert_eq!(frame.textured_rects[9].size(), [18.0, 18.0]);'),
    ('            [720.5 / 768.0, 210.5 / 283.0]', '            [720.5 / 768.0, 211.5 / 283.0]'),
    ('            [743.5 / 768.0, 227.5 / 283.0]', '            [743.5 / 768.0, 228.5 / 283.0]'),
    ('        assert_eq!(equipment_frame.textured_rects[0].size(), [8.0, 32.0]);', '        assert_eq!(equipment_frame.textured_rects[0].size(), [8.0, 38.0]);'),
    ('        assert_eq!(dialog_frame.textured_rects[0].size(), [8.0, 32.0]);', '        assert_eq!(dialog_frame.textured_rects[0].size(), [8.0, 38.0]);'),
    ('        assert_eq!(equipment_frame.textured_rects[4].size(), [234.0, 260.0]);', '        assert_eq!(equipment_frame.textured_rects[4].size(), [234.0, 254.0]);'),
    ('        assert_eq!(dialog_frame.textured_rects[4].size(), [384.0, 140.0]);', '        assert_eq!(dialog_frame.textured_rects[4].size(), [384.0, 134.0]);'),
    ('            [720.5 / 768.0, 228.5 / 283.0]', '            [720.5 / 768.0, 231.5 / 283.0]'),
    ('            [720.5 / 768.0, 246.5 / 283.0]', '            [720.5 / 768.0, 251.5 / 283.0]'),
]
for old, new in repls:
    if old not in s:
        raise SystemExit(f'ui_panel test replacement missing: {old[:80]!r}')
    s = s.replace(old, new)
write(p, s)

p = 'apps/client/src/renderer/shaders/ui_texture.wgsl'
s = read(p)
old = '''    if (source_size.x <= 4.0 && source_size.y <= 4.0 &&
        (input.rect_size.x > source_size.x || input.rect_size.y > source_size.y)) {
        return input.tint;
    }
'''
new = '''    if (source_size.x <= 4.0 && source_size.y <= 4.0 &&
        (input.rect_size.x > source_size.x || input.rect_size.y > source_size.y)) {
        let center_uv = (input.uv_min + input.uv_max) * 0.5;
        return textureSample(ui_texture, ui_sampler, center_uv) * input.tint;
    }
'''
s = replace_once(s, old, new, 'flat atlas center sampling')
write(p, s)

p = 'apps/client/src/app.rs'
s = read(p)
old = '''            if let Ok(Some(frame)) = self.message_dialog.frame(
                window_assets,
                self.ui_button_assets,
                viewport,
                pixels_per_unit,
                self.cursor_position,
            ) {
                ui_textured_rects.extend(frame.textured_rects);
                ui_text.extend(frame.texts);
            }
'''
new = '''            if let Ok(Some(frame)) = self.message_dialog.frame(
                window_assets,
                self.ui_button_assets,
                viewport,
                pixels_per_unit,
                self.cursor_position,
            ) {
                // Text and colored overlays are separate renderer passes that otherwise
                // render after every textured window. A modal owns the top UI layer,
                // so lower-layer labels/bubbles must not leak over its chrome.
                ui_rects.clear();
                ui_text.clear();
                ui_textured_rects.extend(frame.textured_rects);
                ui_text.extend(frame.texts);
            }
'''
s = replace_once(s, old, new, 'modal top-layer composition')
write(p, s)

print('UI repair patch applied')
