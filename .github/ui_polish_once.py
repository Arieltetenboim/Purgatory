from pathlib import Path
import json

ROOT = Path(__file__).resolve().parents[1]
UI = ROOT / "apps/client/src/ui_panel.rs"
ATLAS = ROOT / "Graphic/ui/ATLAS.ui.json"
TAB = ROOT / "Graphic/ui/inventory_tab.ui.json"


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


text = UI.read_text(encoding="utf-8")

text = replace_once(
    text,
    "const TITLE_FONT_SIZE_UNITS: f32 = 15.0;\nconst TITLE_LEFT_INSET_UNITS: f32 = 12.0;\nconst HEADER_HEIGHT_UNITS: f32 = 38.0;\nconst HEADER_TOP_INSET_UNITS: f32 = 6.0;\nconst TITLE_CONTROL_GAP_UNITS: f32 = 6.0;",
    "const TITLE_FONT_SIZE_UNITS: f32 = 15.0;\nconst TITLE_LEFT_INSET_UNITS: f32 = 4.0;\nconst TITLE_CONTROL_GAP_UNITS: f32 = 5.0;",
    "header constants",
)
text = replace_once(
    text,
    "const EQUIPMENT_LABEL_FONT_SIZE_UNITS: f32 = 10.0;\nconst EQUIPMENT_LABEL_GAP_UNITS: f32 = 3.0;",
    "const EQUIPMENT_LABEL_FONT_SIZE_UNITS: f32 = 8.0;\nconst EQUIPMENT_LABEL_GAP_UNITS: f32 = 2.0;",
    "equipment label constants",
)

text = replace_once(
    text,
    "        let mut title_anchor_x = layout.header.min[0];",
    "        let mut title_anchor_x =\n            layout.header.min[0] + TITLE_LEFT_INSET_UNITS * pixels_per_unit;",
    "title left inset",
)
text = replace_once(
    text,
    "            let icon_min = [\n                layout.header.min[0],\n                layout.header.min[1] + ((layout.header.height() - icon_size) * 0.5).max(0.0),\n            ];",
    "            let icon_min = [\n                title_anchor_x,\n                layout.header.min[1] + ((layout.header.height() - icon_size) * 0.5).max(0.0),\n            ];",
    "header icon placement",
)
text = replace_once(
    text,
    "        let title_anchor = [\n            title_anchor_x,\n            layout.header.min[1]\n                + ((HEADER_HEIGHT_UNITS - TITLE_FONT_SIZE_UNITS) * 0.5).max(0.0) * pixels_per_unit,\n        ];",
    "        let title_anchor = [\n            title_anchor_x,\n            layout.header.min[1]\n                + ((layout.header.height() - title_font_size) * 0.5).max(0.0),\n        ];",
    "title vertical centering",
)

text = replace_once(
    text,
    "        let header = ScreenRect {\n            min: [\n                window_min[0] + TITLE_LEFT_INSET_UNITS * pixels_per_unit,\n                window_min[1] + HEADER_TOP_INSET_UNITS * pixels_per_unit,\n            ],\n            max: [\n                window_max[0] - TITLE_LEFT_INSET_UNITS * pixels_per_unit,\n                window_min[1] + HEADER_HEIGHT_UNITS * pixels_per_unit,\n            ],\n        };",
    "        let panel = self.panel();\n        let header = ScreenRect {\n            min: [\n                window_min[0] + panel.border_units.left * pixels_per_unit,\n                window_min[1],\n            ],\n            max: [\n                window_max[0] - panel.border_units.right * pixels_per_unit,\n                window_min[1] + panel.border_units.top * pixels_per_unit,\n            ],\n        };",
    "authored header bounds",
)

text = replace_once(
    text,
    "            let mut source_piece = SourceRectPx {\n                x: source_x[column] as u32,\n                y: source_y[row] as u32,\n                width: (source_x[column + 1] - source_x[column]) as u32,\n                height: (source_y[row + 1] - source_y[row]) as u32,\n            };\n            source_piece = match (row, column) {\n                (1, 1) => centered_source_sample(source_piece, 2, 2),\n                (0 | 2, 1) => centered_source_sample(source_piece, 2, source_piece.height),\n                (1, 0 | 2) => centered_source_sample(source_piece, source_piece.width, 2),\n                _ => source_piece,\n            };",
    "            let source_piece = SourceRectPx {\n                x: source_x[column] as u32,\n                y: source_y[row] as u32,\n                width: (source_x[column + 1] - source_x[column]) as u32,\n                height: (source_y[row + 1] - source_y[row]) as u32,\n            };",
    "simple nine slice",
)
text = replace_once(
    text,
    "\nfn centered_source_sample(rect: SourceRectPx, width: u32, height: u32) -> SourceRectPx {\n    let width = width.clamp(1, rect.width);\n    let height = height.clamp(1, rect.height);\n    SourceRectPx {\n        x: rect.x + (rect.width - width) / 2,\n        y: rect.y + (rect.height - height) / 2,\n        width,\n        height,\n    }\n}\n",
    "\n",
    "remove center sampling helper",
)

text = replace_once(
    text,
    "                anchor: [\n                    (slot.min[0] + slot.max[0]) * 0.5,\n                    slot.max[1] + EQUIPMENT_LABEL_GAP_UNITS * pixels_per_unit,\n                ],\n                max_width: Some(slot.max[0] - slot.min[0]),",
    "                anchor: [\n                    (slot.min[0] + slot.max[0]) * 0.5,\n                    slot.max[1]\n                        - (EQUIPMENT_LABEL_FONT_SIZE_UNITS + EQUIPMENT_LABEL_GAP_UNITS)\n                            * pixels_per_unit,\n                ],\n                max_width: Some(\n                    (slot.max[0] - slot.min[0] - 4.0 * pixels_per_unit).max(1.0),\n                ),",
    "equipment labels inside slots",
)
text = replace_once(
    text,
    "        assert_eq!(frame.textured_rects[9].size(), [18.0, 18.0]);",
    "        assert_eq!(frame.textured_rects[9].size(), [24.0, 18.0]);",
    "close aspect test",
)
text = replace_once(
    text,
    "        assert_eq!(window.top_left_units, Some([0.0, 84.0]));",
    "        assert_eq!(window.top_left_units, Some([0.0, 90.0]));",
    "header drag expected offset",
)

UI.write_text(text, encoding="utf-8")

atlas = json.loads(ATLAS.read_text(encoding="utf-8"))
# Preserve the authored close-button aspect ratio (24x18 source region).
atlas["close_size_units"] = [24.0, 18.0]
ATLAS.write_text(json.dumps(atlas, indent=2) + "\n", encoding="utf-8")

tab = json.loads(TAB.read_text(encoding="utf-8"))
# The source cap is 96 px on a 177 px-high state rendered at 26 UI units.
# ~14 units keeps the cap scaling isotropic instead of crushing the side borders.
tab["cap_units"] = [14.0, 14.0] if isinstance(tab.get("cap_units"), list) else {"left": 14.0, "right": 14.0}
TAB.write_text(json.dumps(tab, indent=2) + "\n", encoding="utf-8")

print("Focused UI chrome polish applied")
