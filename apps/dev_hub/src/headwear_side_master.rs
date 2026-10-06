//! Post-8 Headwear Side master overlay (Developer Hub only).
//!
//! Fixed 2×2 grid SVG. Crown `+` is the attachment point, not the visual center.
//! Export writes the overlay only — no example art. Not a runtime asset.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, RgbaImage, imageops};

use purgatory_skeleton::{ANCHOR_CROWN, HEAD, LocalPose, WorldPose, evaluate, humanoid_v0};

use crate::authoring_template::{PX_PER_WU, fmt_num};

pub const SHEET_W: u32 = 512;
pub const SHEET_H: u32 = 512;
pub const COLUMNS: u32 = 2;
pub const ROWS: u32 = 2;
pub const CELL_W: u32 = 256;
pub const CELL_H: u32 = 256;
/// Crown `+` inside every cell (pixels from the cell's top-left).
pub const CROWN_LOCAL_X: u32 = 128;
pub const CROWN_LOCAL_Y: u32 = 176;
pub const SAFE_INSET_PX: u32 = 16;

const OUTPUT_REL: &str = "Graphic/character/headwear_side/HEADWEAR_SIDE_MASTER_V1.svg";
const VISUAL_PACK_REL: &str = "Graphic/character/base/character.base.dev_01.visual-pack.json";
const ATLAS_REL: &str = "Graphic/character/base/character.base.dev_01.side.atlas.png";
const HEAD_VISUAL_KEY: &str = "character.base.dev_01.head.side";
const REFERENCE_GROUP: &str = "REFERENCE_HEAD_DO_NOT_EXPORT";

pub const CELLS: [CellSpec; 4] = [
    CellSpec {
        id: "r0c0",
        row: 0,
        col: 0,
        visual_key: "equipment.debug.headwear_proof.a.side",
    },
    CellSpec {
        id: "r0c1",
        row: 0,
        col: 1,
        visual_key: "equipment.debug.headwear_proof.b.side",
    },
    CellSpec {
        id: "r1c0",
        row: 1,
        col: 0,
        visual_key: "equipment.debug.headwear_proof.c.side",
    },
    CellSpec {
        id: "r1c1",
        row: 1,
        col: 1,
        visual_key: "equipment.debug.headwear_proof.d.side",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellSpec {
    pub id: &'static str,
    pub row: u32,
    pub col: u32,
    pub visual_key: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellBounds {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[must_use]
pub fn default_output_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(OUTPUT_REL)
}

#[must_use]
pub fn default_dir() -> PathBuf {
    default_output_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(default_output_path)
}

#[must_use]
pub fn sheet_png_path() -> PathBuf {
    default_dir().join("HEADWEAR_SIDE_MASTER_V1.png")
}

#[must_use]
pub fn extracted_dir() -> PathBuf {
    default_dir().join("extracted")
}

#[must_use]
pub fn sprite_file_name(visual_key: &str) -> String {
    format!("{visual_key}.png")
}

#[must_use]
pub fn cell_bounds(spec: CellSpec) -> CellBounds {
    CellBounds {
        x: spec.col * CELL_W,
        y: spec.row * CELL_H,
        w: CELL_W,
        h: CELL_H,
    }
}

#[must_use]
pub fn crown_sheet_px(spec: CellSpec) -> [u32; 2] {
    let b = cell_bounds(spec);
    [b.x + CROWN_LOCAL_X, b.y + CROWN_LOCAL_Y]
}

pub fn export_to(path: &Path) -> Result<PathBuf, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("create dir: {err}"))?;
    }
    std::fs::write(path, render_svg()?)
        .map_err(|err| format!("write {}: {err}", path.display()))?;
    Ok(path.to_path_buf())
}

/// Crop exactly the configured cell rectangle. Does not inspect pixels.
pub fn crop_cell(sheet: &RgbaImage, spec: CellSpec) -> Result<RgbaImage, String> {
    if sheet.width() != SHEET_W || sheet.height() != SHEET_H {
        return Err(format!(
            "sheet is {}×{}, expected {SHEET_W}×{SHEET_H}",
            sheet.width(),
            sheet.height()
        ));
    }
    let b = cell_bounds(spec);
    Ok(imageops::crop_imm(sheet, b.x, b.y, b.w, b.h).to_image())
}

/// Grid-only extract from an artist PNG. No content detect, no transparent trim.
pub fn extract_from_png(sheet_path: &Path, extracted: &Path) -> Result<PathBuf, String> {
    let bytes = std::fs::read(sheet_path).map_err(|err| format!("read sheet: {err}"))?;
    let decoded = image::load_from_memory(&bytes).map_err(|err| format!("decode sheet: {err}"))?;
    let sheet = decoded.to_rgba8();
    std::fs::create_dir_all(extracted).map_err(|err| format!("create extracted: {err}"))?;
    for spec in CELLS {
        let sprite = crop_cell(&sheet, spec)?;
        let path = extracted.join(sprite_file_name(spec.visual_key));
        std::fs::write(&path, encode_png(&sprite)?)
            .map_err(|err| format!("write {}: {err}", path.display()))?;
    }
    std::fs::write(extracted.join("mapping.json"), render_mapping_json())
        .map_err(|err| format!("write mapping: {err}"))?;
    Ok(extracted.to_path_buf())
}

fn encode_png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let encoder =
            PngEncoder::new_with_quality(&mut buf, CompressionType::Fast, FilterType::NoFilter);
        encoder
            .write_image(
                img.as_raw(),
                img.width(),
                img.height(),
                ExtendedColorType::Rgba8,
            )
            .map_err(|err| format!("encode png: {err}"))?;
    }
    Ok(buf)
}

#[must_use]
pub fn render_mapping_json() -> String {
    let mut sprites = String::new();
    for (i, spec) in CELLS.iter().enumerate() {
        let comma = if i + 1 == CELLS.len() { "" } else { "," };
        let file = sprite_file_name(spec.visual_key);
        let _ = writeln!(
            sprites,
            "    {{\"cell\": \"{}\", \"visual_key\": \"{}\", \"file\": \"extracted/{file}\"}}{comma}",
            spec.id, spec.visual_key
        );
    }
    format!(
        "{{\n\
  \"schema_version\": 1,\n\
  \"sheet\": \"HEADWEAR_SIDE_MASTER_V1.png\",\n\
  \"crown_local_px\": [{CROWN_LOCAL_X}, {CROWN_LOCAL_Y}],\n\
  \"sprite_px\": [{CELL_W}, {CELL_H}],\n\
  \"extract\": \"grid_bounds_only\",\n\
  \"sprites\": [\n{sprites}  ]\n\
}}\n"
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadVisualGeometry {
    pub rect_px: [u32; 4],
    pub pivot_px: [f32; 2],
    pub pixels_per_unit: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReferenceHeadPlacement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(relative)
}

/// Vector from the Crown attachment to the head bone, in world units, Y up.
/// Same compose the runtime uses: head world ∘ [`ANCHOR_CROWN`].
#[must_use]
pub fn crown_to_head_world() -> [f32; 2] {
    let def = humanoid_v0();
    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).expect("humanoid v0 bind pose");
    let head = world.get(HEAD).expect("head bone");
    let crown = head.compose(ANCHOR_CROWN);
    [
        head.translation[0] - crown.translation[0],
        head.translation[1] - crown.translation[1],
    ]
}

/// Place the head sprite's top-left inside one cell so its pivot sits on HEAD
/// and Crown stays at [`CROWN_LOCAL_X`], [`CROWN_LOCAL_Y`].
#[must_use]
pub fn reference_head_placement(head: HeadVisualGeometry) -> ReferenceHeadPlacement {
    let crown_to_head = crown_to_head_world();
    let ppu = head.pixels_per_unit;
    let dx = crown_to_head[0] - head.pivot_px[0] / ppu;
    let dy = crown_to_head[1] + head.pivot_px[1] / ppu;
    let scale = PX_PER_WU / ppu;
    ReferenceHeadPlacement {
        x: CROWN_LOCAL_X as f32 + dx * PX_PER_WU,
        y: CROWN_LOCAL_Y as f32 - dy * PX_PER_WU,
        width: head.rect_px[2] as f32 * scale,
        height: head.rect_px[3] as f32 * scale,
    }
}

pub fn load_canonical_head_geometry() -> Result<HeadVisualGeometry, String> {
    let bytes = std::fs::read(repo_path(VISUAL_PACK_REL))
        .map_err(|err| format!("read visual pack: {err}"))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|err| format!("visual pack JSON: {err}"))?;
    let pixels_per_unit = value
        .get("pixels_per_unit")
        .and_then(serde_json::Value::as_f64)
        .ok_or("visual pack is missing pixels_per_unit")?;
    let visuals = value
        .get("visuals")
        .and_then(serde_json::Value::as_array)
        .ok_or("visual pack is missing visuals")?;
    for visual in visuals {
        if visual.get("visual_key").and_then(serde_json::Value::as_str) != Some(HEAD_VISUAL_KEY) {
            continue;
        }
        let rect = visual
            .get("rect_px")
            .and_then(serde_json::Value::as_array)
            .ok_or("head visual is missing rect_px")?;
        let pivot = visual
            .get("pivot_px")
            .and_then(serde_json::Value::as_array)
            .ok_or("head visual is missing pivot_px")?;
        let component =
            |values: &[serde_json::Value], index: usize, label: &str| -> Result<f64, String> {
                values
                    .get(index)
                    .and_then(serde_json::Value::as_f64)
                    .ok_or_else(|| format!("head visual {label}[{index}] is not a number"))
            };
        return Ok(HeadVisualGeometry {
            rect_px: [
                component(rect, 0, "rect_px")? as u32,
                component(rect, 1, "rect_px")? as u32,
                component(rect, 2, "rect_px")? as u32,
                component(rect, 3, "rect_px")? as u32,
            ],
            pivot_px: [
                component(pivot, 0, "pivot_px")? as f32,
                component(pivot, 1, "pivot_px")? as f32,
            ],
            pixels_per_unit: pixels_per_unit as f32,
        });
    }
    Err(format!("visual pack has no {HEAD_VISUAL_KEY}"))
}

fn reference_head_png(head: HeadVisualGeometry) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(repo_path(ATLAS_REL)).map_err(|err| format!("read atlas: {err}"))?;
    let atlas = image::load_from_memory(&bytes)
        .map_err(|err| format!("decode atlas: {err}"))?
        .to_rgba8();
    let [x, y, width, height] = head.rect_px;
    if x.saturating_add(width) > atlas.width() || y.saturating_add(height) > atlas.height() {
        return Err("head rect does not fit the side atlas".to_owned());
    }
    let crop = imageops::crop_imm(&atlas, x, y, width, height).to_image();
    encode_png(&crop)
}

pub fn render_svg() -> Result<String, String> {
    let head = load_canonical_head_geometry()?;
    let placement = reference_head_placement(head);
    let png = base64_encode(&reference_head_png(head)?);
    let mut out = String::with_capacity(png.len() + 8_000);
    let _ = writeln!(
        out,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{SHEET_W}\" height=\"{SHEET_H}\" viewBox=\"0 0 {SHEET_W} {SHEET_H}\">\n\
  <title>PURGATORY Headwear Side master v1</title>\n\
  <desc>Fixed 2×2 grid. {SHEET_W}×{SHEET_H} px · {COLUMNS}×{ROWS} cells of {CELL_W}×{CELL_H} · Crown local ({CROWN_LOCAL_X}, {CROWN_LOCAL_Y}) · {px} px/wu. The {REFERENCE_GROUP} group is guide art from character.base.dev_01.head.side and must not be exported as headwear. Crop by grid only.</desc>",
        px = PX_PER_WU as u32,
    );
    let _ = writeln!(
        out,
        "  <defs>\n\
    <g id=\"{REFERENCE_GROUP}\">\n\
      <image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"data:image/png;base64,{png}\"/>\n\
    </g>\n\
  </defs>",
        fmt_num(placement.x),
        fmt_num(placement.y),
        fmt_num(placement.width),
        fmt_num(placement.height),
    );
    for spec in CELLS {
        let b = cell_bounds(spec);
        let _ = writeln!(
            out,
            "  <use href=\"#{REFERENCE_GROUP}\" x=\"{}\" y=\"{}\"/>",
            b.x, b.y
        );
    }
    let _ = writeln!(
        out,
        "  <g id=\"grid\" fill=\"none\" stroke=\"#a8a29e\" stroke-width=\"1\">"
    );
    for spec in CELLS {
        let b = cell_bounds(spec);
        let _ = writeln!(
            out,
            "    <rect id=\"{}\" data-visual-key=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
            spec.id, spec.visual_key, b.x, b.y, b.w, b.h
        );
    }
    let _ = writeln!(out, "  </g>");
    let _ = writeln!(
        out,
        "  <g id=\"safe\" fill=\"none\" stroke=\"#a8a29e\" stroke-width=\"1\" stroke-dasharray=\"4 4\">"
    );
    for spec in CELLS {
        let b = cell_bounds(spec);
        let x = b.x + SAFE_INSET_PX;
        let y = b.y + SAFE_INSET_PX;
        let w = b.w - SAFE_INSET_PX * 2;
        let h = b.h - SAFE_INSET_PX * 2;
        let _ = writeln!(
            out,
            "    <rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"/>"
        );
    }
    let _ = writeln!(out, "  </g>");
    let _ = writeln!(
        out,
        "  <g id=\"crown\" data-local-px=\"{CROWN_LOCAL_X},{CROWN_LOCAL_Y}\">"
    );
    for spec in CELLS {
        let [cx, cy] = crown_sheet_px(spec);
        let _ = writeln!(
            out,
            "    <g id=\"crown-{}\" data-sheet-px=\"{cx},{cy}\"></g>",
            spec.id
        );
    }
    let _ = writeln!(out, "  </g>");
    let _ = writeln!(
        out,
        "  <g id=\"ids\" font-family=\"Segoe UI,Arial,sans-serif\" font-size=\"11\" fill=\"#44403c\">"
    );
    for spec in CELLS {
        let b = cell_bounds(spec);
        let _ = writeln!(
            out,
            "    <text x=\"{}\" y=\"{}\">{}</text>",
            b.x + 10,
            b.y + 22,
            spec.id
        );
    }
    let _ = writeln!(out, "  </g>\n</svg>");
    Ok(out)
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut index = 0;
    while index + 3 <= data.len() {
        let value = (u32::from(data[index]) << 16)
            | (u32::from(data[index + 1]) << 8)
            | u32::from(data[index + 2]);
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        out.push(TABLE[((value >> 6) & 63) as usize] as char);
        out.push(TABLE[(value & 63) as usize] as char);
        index += 3;
    }
    let rest = data.len() - index;
    if rest == 1 {
        let value = u32::from(data[index]) << 16;
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rest == 2 {
        let value = (u32::from(data[index]) << 16) | (u32::from(data[index + 1]) << 8);
        out.push(TABLE[((value >> 18) & 63) as usize] as char);
        out.push(TABLE[((value >> 12) & 63) as usize] as char);
        out.push(TABLE[((value >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}

#[cfg(test)]
fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    fn value(byte: u8) -> Result<u8, String> {
        match byte {
            b'A'..=b'Z' => Ok(byte - b'A'),
            b'a'..=b'z' => Ok(byte - b'a' + 26),
            b'0'..=b'9' => Ok(byte - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err("invalid base64".to_owned()),
        }
    }
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("base64 length is not a multiple of 4".to_owned());
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().filter(|byte| **byte == b'=').count();
        let mut n = 0u32;
        for (offset, byte) in chunk.iter().copied().enumerate() {
            if byte == b'=' {
                continue;
            }
            n |= u32::from(value(byte)?) << (18 - offset * 6);
        }
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn layout_is_fixed_2x2() {
        assert_eq!(SHEET_W, COLUMNS * CELL_W);
        assert_eq!(SHEET_H, ROWS * CELL_H);
        assert_eq!(CELLS.len(), 4);
        assert_eq!(CELL_W, PX_PER_WU as u32);
        assert_eq!(CELL_H, PX_PER_WU as u32);
        assert_eq!([CROWN_LOCAL_X, CROWN_LOCAL_Y], [128, 176]);
        let ids: Vec<_> = CELLS.iter().map(|c| c.id).collect();
        assert_eq!(ids, ["r0c0", "r0c1", "r1c0", "r1c1"]);
    }

    #[test]
    fn svg_keeps_the_grid_and_marks_the_head_as_guide_art() {
        let svg = render_svg().unwrap();
        let structure = svg.split("base64,").next().unwrap();
        assert!(svg.contains(&format!("width=\"{SHEET_W}\"")));
        assert!(svg.contains(&format!("height=\"{SHEET_H}\"")));
        assert!(svg.contains("id=\"crown\""));
        assert!(svg.contains("id=\"grid\""));
        assert!(svg.contains("id=\"safe\""));
        assert!(structure.contains(&format!("id=\"{REFERENCE_GROUP}\"")));
        assert!(!structure.contains("<line"));
        assert!(!structure.contains("<polygon"));
        assert!(!structure.contains("<ellipse"));
        assert!(!svg.contains("world_x"));
        for spec in CELLS {
            assert!(svg.contains(spec.id));
            assert!(svg.contains(spec.visual_key));
            let b = cell_bounds(spec);
            assert!(svg.contains(&format!(
                "<use href=\"#{REFERENCE_GROUP}\" x=\"{}\" y=\"{}\"/>",
                b.x, b.y
            )));
        }
    }

    #[test]
    fn reference_head_placement_follows_visual_pack_and_crown_compose() {
        let head = load_canonical_head_geometry().unwrap();
        assert_ne!(head.rect_px, [0, 0, 0, 0]);
        assert!(head.pixels_per_unit > 0.0);
        let placed = reference_head_placement(head);
        let crown_to_head = crown_to_head_world();
        let dx = crown_to_head[0] - head.pivot_px[0] / head.pixels_per_unit;
        let dy = crown_to_head[1] + head.pivot_px[1] / head.pixels_per_unit;
        assert!((placed.x - (CROWN_LOCAL_X as f32 + dx * PX_PER_WU)).abs() < 1e-3);
        assert!((placed.y - (CROWN_LOCAL_Y as f32 - dy * PX_PER_WU)).abs() < 1e-3);
        assert!(
            (placed.width - head.rect_px[2] as f32 * PX_PER_WU / head.pixels_per_unit).abs() < 1e-3
        );
        let mut shifted = head;
        shifted.pivot_px[0] += 17.0;
        let moved = reference_head_placement(shifted);
        let expected_shift = -17.0 / head.pixels_per_unit * PX_PER_WU;
        assert!((moved.x - placed.x - expected_shift).abs() < 1e-3);
        assert!((crown_to_head[0] + ANCHOR_CROWN.translation[0]).abs() < 1e-4);
        assert!((crown_to_head[1] + ANCHOR_CROWN.translation[1]).abs() < 1e-4);

        let svg = render_svg().unwrap();
        assert!(svg.contains(&format!("x=\"{}\"", fmt_num(placed.x))));
        assert!(svg.contains(&format!("y=\"{}\"", fmt_num(placed.y))));
        assert!(svg.contains(&format!("width=\"{}\"", fmt_num(placed.width))));
        assert!(svg.contains(&format!("height=\"{}\"", fmt_num(placed.height))));
        let marker = "data:image/png;base64,";
        let start = svg.find(marker).unwrap() + marker.len();
        let end = svg[start..].find('"').unwrap() + start;
        let decoded = base64_decode(&svg[start..end]).unwrap();
        let png = image::load_from_memory(&decoded).unwrap().to_rgba8();
        let atlas = image::load_from_memory(&std::fs::read(repo_path(ATLAS_REL)).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(png.width(), head.rect_px[2]);
        assert_eq!(png.height(), head.rect_px[3]);
        assert_eq!(
            *png.get_pixel(0, 0),
            *atlas.get_pixel(head.rect_px[0], head.rect_px[1])
        );
        assert_eq!(
            *png.get_pixel(png.width() - 1, png.height() - 1),
            *atlas.get_pixel(
                head.rect_px[0] + head.rect_px[2] - 1,
                head.rect_px[1] + head.rect_px[3] - 1
            )
        );
    }

    #[test]
    fn extract_is_exact_grid_crop_not_content_detect() {
        let mut sheet = RgbaImage::new(SHEET_W, SHEET_H);
        let marker = Rgba([1, 2, 3, 255]);
        sheet.put_pixel(0, 0, marker);
        sheet.put_pixel(CELL_W - 1, CELL_H - 1, marker);
        let empty = crop_cell(&sheet, CELLS[1]).unwrap();
        assert!(
            empty.pixels().all(|p| p.0[3] == 0),
            "empty configured cell must still emit a full transparent sprite"
        );
        let populated = crop_cell(&sheet, CELLS[0]).unwrap();
        assert_eq!(populated.width(), CELL_W);
        assert_eq!(populated.height(), CELL_H);
        assert_eq!(*populated.get_pixel(0, 0), marker);
        assert_eq!(*populated.get_pixel(CELL_W - 1, CELL_H - 1), marker);
        let sheet_px = crown_sheet_px(CELLS[0]);
        assert_eq!(
            *populated.get_pixel(CROWN_LOCAL_X, CROWN_LOCAL_Y),
            *sheet.get_pixel(sheet_px[0], sheet_px[1])
        );
    }

    #[test]
    fn export_writes_svg_only() {
        let dir = std::env::temp_dir().join("purgatory_headwear_side_svg_only_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("HEADWEAR_SIDE_MASTER_V1.svg");
        export_to(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            render_svg().unwrap()
        );
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["HEADWEAR_SIDE_MASTER_V1.svg"]);
    }

    #[test]
    fn extract_from_png_writes_grid_cells_and_mapping() {
        let dir = std::env::temp_dir().join("purgatory_headwear_side_extract_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut sheet = RgbaImage::new(SHEET_W, SHEET_H);
        sheet.put_pixel(0, 0, Rgba([9, 8, 7, 255]));
        let png = dir.join("HEADWEAR_SIDE_MASTER_V1.png");
        std::fs::write(&png, encode_png(&sheet).unwrap()).unwrap();
        let extracted = dir.join("extracted");
        extract_from_png(&png, &extracted).unwrap();
        let a = image::load_from_memory(
            &std::fs::read(extracted.join(sprite_file_name(CELLS[0].visual_key))).unwrap(),
        )
        .unwrap()
        .to_rgba8();
        assert_eq!(a.width(), CELL_W);
        assert_eq!(a.height(), CELL_H);
        assert_eq!(*a.get_pixel(0, 0), Rgba([9, 8, 7, 255]));
        let mapping = std::fs::read_to_string(extracted.join("mapping.json")).unwrap();
        assert_eq!(mapping, render_mapping_json());
        assert!(!dir.join("HEADWEAR_SIDE_MASTER_V1.svg").exists());
    }

    #[test]
    fn write_headwear_side_master_if_requested() {
        if std::env::var("PURGATORY_WRITE_HEADWEAR_SIDE_MASTER").as_deref() != Ok("1") {
            return;
        }
        export_to(&default_output_path()).expect("write committed Headwear Side SVG");
    }

    #[test]
    fn committed_svg_matches_generator() {
        if std::env::var("PURGATORY_WRITE_HEADWEAR_SIDE_MASTER").as_deref() == Ok("1") {
            return;
        }
        let on_disk = std::fs::read_to_string(default_output_path()).unwrap_or_default();
        assert_eq!(
            on_disk.replace("\r\n", "\n"),
            render_svg().unwrap().replace("\r\n", "\n"),
            "re-export with PURGATORY_WRITE_HEADWEAR_SIDE_MASTER=1 cargo test -p purgatory-dev-hub --bin purgatory-dev-hub write_headwear_side_master_if_requested"
        );
    }
}
