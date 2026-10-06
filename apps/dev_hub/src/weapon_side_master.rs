//! Side weapon paper-doll calibration sheet (Developer Hub only).
//!
//! The red "+" is the weapon visual pivot. Runtime places that pixel on
//! GripFront (`HandFront` composed with [`ANCHOR_GRIP`]). The hand image is
//! guide art in that same weapon-local space. Not a runtime sprite.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, RgbaImage, imageops};
use purgatory_skeleton::{
    ANCHOR_GRIP, BoneTransform, HAND_FRONT, LocalPose, WorldPose, evaluate, humanoid_v0,
    humanoid_v0_bone_by_label,
};

use crate::authoring_template::{PX_PER_WU, fmt_num};

pub const CANVAS_W: u32 = 1024;
pub const CANVAS_H: u32 = 1024;
/// Weapon pivot. This pixel is GripFront, and it is the origin of the authored weapon sprite.
pub const PIVOT_X: u32 = 512;
pub const PIVOT_Y: u32 = 512;

const OUTPUT_REL: &str = "Graphic/character/weapon_side/WEAPON_SIDE_MASTER_V1.svg";
const VISUAL_PACK_REL: &str = "Graphic/character/base/character.base.dev_01.visual-pack.json";
const ATLAS_REL: &str = "Graphic/character/base/character.base.dev_01.side.atlas.png";
const HAND_VISUAL_KEY: &str = "character.base.dev_01.hand_front.side";
const HAND_GROUP: &str = "REFERENCE_HAND_DO_NOT_EXPORT";
const CHARACTER_GROUP: &str = "REFERENCE_CHARACTER_DO_NOT_EXPORT";

#[derive(Clone, Debug, PartialEq)]
pub struct PartVisual {
    pub bone: String,
    pub visual_key: String,
    pub rect_px: [u32; 4],
    pub pivot_px: [f32; 2],
    pub pixels_per_unit: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImagePlacement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// SVG degrees. Positive is clockwise on this Y-down sheet.
    pub rotation_deg: f32,
}

#[must_use]
pub fn default_output_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(OUTPUT_REL)
}

pub fn export_to(path: &Path) -> Result<PathBuf, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("create dir: {err}"))?;
    }
    std::fs::write(path, render_svg()?)
        .map_err(|err| format!("write {}: {err}", path.display()))?;
    Ok(path.to_path_buf())
}

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(relative)
}

/// Bind-pose HandFront and the GripFront attachment (`hand.compose(ANCHOR_GRIP)`).
#[must_use]
pub fn bind_hand_and_grip() -> (BoneTransform, BoneTransform) {
    let def = humanoid_v0();
    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).expect("humanoid v0 bind pose");
    let hand = world.get(HAND_FRONT).expect("hand_front");
    let grip = hand.compose(ANCHOR_GRIP);
    (hand, grip)
}

fn rotate(point: [f32; 2], radians: f32) -> [f32; 2] {
    let (sin, cos) = radians.sin_cos();
    [
        cos * point[0] - sin * point[1],
        sin * point[0] + cos * point[1],
    ]
}

/// Canvas position of a bone origin, in weapon-local space whose origin is `grip`.
#[cfg(test)]
#[must_use]
pub fn bone_origin_canvas(bone: BoneTransform, grip: BoneTransform) -> [f32; 2] {
    let local = rotate(
        [
            bone.translation[0] - grip.translation[0],
            bone.translation[1] - grip.translation[1],
        ],
        -grip.rotation,
    );
    [
        PIVOT_X as f32 + local[0] * PX_PER_WU,
        PIVOT_Y as f32 - local[1] * PX_PER_WU,
    ]
}

/// Place a sprite's top-left so its pivot sits on `bone` and the sheet origin is `grip`.
#[must_use]
pub fn place_visual(
    rect_px: [u32; 4],
    pivot_px: [f32; 2],
    pixels_per_unit: f32,
    bone: BoneTransform,
    grip: BoneTransform,
) -> ImagePlacement {
    let scale = PX_PER_WU / pixels_per_unit;
    let top_left_local = [
        -pivot_px[0] / pixels_per_unit,
        pivot_px[1] / pixels_per_unit,
    ];
    let top_left_world = bone.compose(BoneTransform::from_translation_rotation(
        top_left_local,
        0.0,
    ));
    let local = rotate(
        [
            top_left_world.translation[0] - grip.translation[0],
            top_left_world.translation[1] - grip.translation[1],
        ],
        -grip.rotation,
    );
    let rotation_deg = -(bone.rotation - grip.rotation).to_degrees();
    ImagePlacement {
        x: PIVOT_X as f32 + local[0] * PX_PER_WU,
        y: PIVOT_Y as f32 - local[1] * PX_PER_WU,
        width: rect_px[2] as f32 * scale,
        height: rect_px[3] as f32 * scale,
        rotation_deg,
    }
}

pub fn load_visual_pack() -> Result<(f32, Vec<PartVisual>), String> {
    let bytes = std::fs::read(repo_path(VISUAL_PACK_REL))
        .map_err(|err| format!("read visual pack: {err}"))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|err| format!("visual pack JSON: {err}"))?;
    let pixels_per_unit = value
        .get("pixels_per_unit")
        .and_then(serde_json::Value::as_f64)
        .ok_or("visual pack is missing pixels_per_unit")? as f32;
    let visuals = value
        .get("visuals")
        .and_then(serde_json::Value::as_array)
        .ok_or("visual pack is missing visuals")?;
    let mut parts = Vec::new();
    for visual in visuals {
        let bone = visual
            .get("bone")
            .and_then(serde_json::Value::as_str)
            .ok_or("visual is missing bone")?
            .to_owned();
        let visual_key = visual
            .get("visual_key")
            .and_then(serde_json::Value::as_str)
            .ok_or("visual is missing visual_key")?
            .to_owned();
        let rect = number_array(visual, "rect_px")?;
        let pivot = number_array(visual, "pivot_px")?;
        parts.push(PartVisual {
            bone,
            visual_key,
            rect_px: [
                rect[0] as u32,
                rect[1] as u32,
                rect[2] as u32,
                rect[3] as u32,
            ],
            pivot_px: [pivot[0] as f32, pivot[1] as f32],
            pixels_per_unit,
        });
    }
    Ok((pixels_per_unit, parts))
}

fn number_array(visual: &serde_json::Value, field: &str) -> Result<Vec<f64>, String> {
    visual
        .get(field)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("visual is missing {field}"))?
        .iter()
        .map(|value| {
            value
                .as_f64()
                .ok_or_else(|| format!("visual {field} entry is not a number"))
        })
        .collect()
}

pub fn hand_visual(parts: &[PartVisual]) -> Result<&PartVisual, String> {
    parts
        .iter()
        .find(|part| part.visual_key == HAND_VISUAL_KEY)
        .ok_or_else(|| format!("visual pack has no {HAND_VISUAL_KEY}"))
}

fn crop_png(atlas: &RgbaImage, rect: [u32; 4]) -> Result<Vec<u8>, String> {
    let [x, y, width, height] = rect;
    if x.saturating_add(width) > atlas.width() || y.saturating_add(height) > atlas.height() {
        return Err("visual rect does not fit the side atlas".to_owned());
    }
    let crop = imageops::crop_imm(atlas, x, y, width, height).to_image();
    let mut buf = Vec::new();
    let encoder =
        PngEncoder::new_with_quality(&mut buf, CompressionType::Fast, FilterType::NoFilter);
    encoder
        .write_image(
            crop.as_raw(),
            crop.width(),
            crop.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|err| format!("encode png: {err}"))?;
    Ok(buf)
}

fn load_atlas() -> Result<RgbaImage, String> {
    let bytes = std::fs::read(repo_path(ATLAS_REL)).map_err(|err| format!("read atlas: {err}"))?;
    image::load_from_memory(&bytes)
        .map_err(|err| format!("decode atlas: {err}"))
        .map(|image| image.to_rgba8())
}

pub fn render_svg() -> Result<String, String> {
    let (_ppu, parts) = load_visual_pack()?;
    let hand = hand_visual(&parts)?.clone();
    let atlas = load_atlas()?;
    let (hand_world, grip) = bind_hand_and_grip();
    let def = humanoid_v0();
    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).expect("humanoid v0 bind pose");

    let mut out = String::new();
    let _ = writeln!(
        out,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{CANVAS_W}\" height=\"{CANVAS_H}\" viewBox=\"0 0 {CANVAS_W} {CANVAS_H}\">\n\
  <title>PURGATORY Weapon Side master v1</title>\n\
  <desc>Weapon paper-doll sheet. {CANVAS_W}×{CANVAS_H} px · {px} px/wu · pivot ({PIVOT_X}, {PIVOT_Y}) is the weapon visual origin and GripFront. Draw weapon pixels around that +. {HAND_GROUP} and {CHARACTER_GROUP} are guide art and must not be exported into the weapon PNG.</desc>",
        px = PX_PER_WU as u32,
    );
    write_grid(&mut out);
    let _ = writeln!(out, "  <g id=\"{CHARACTER_GROUP}\" opacity=\"0.28\">");
    for part in &parts {
        let Some(bone_index) = humanoid_v0_bone_by_label(&part.bone) else {
            continue;
        };
        let bone = world.get(bone_index).expect("bone");
        let placement = place_visual(
            part.rect_px,
            part.pivot_px,
            part.pixels_per_unit,
            bone,
            grip,
        );
        let png = base64_encode(&crop_png(&atlas, part.rect_px)?);
        write_image(&mut out, &placement, &png);
    }
    let _ = writeln!(out, "  </g>");
    let hand_placement = place_visual(
        hand.rect_px,
        hand.pivot_px,
        hand.pixels_per_unit,
        hand_world,
        grip,
    );
    let hand_png = base64_encode(&crop_png(&atlas, hand.rect_px)?);
    let _ = writeln!(out, "  <g id=\"{HAND_GROUP}\">");
    write_image(&mut out, &hand_placement, &hand_png);
    let _ = writeln!(out, "  </g>");
    write_pivot(&mut out);
    let _ = writeln!(out, "</svg>");
    Ok(out)
}

fn write_grid(out: &mut String) {
    let step = PX_PER_WU * 0.5;
    let _ = writeln!(
        out,
        "  <g id=\"grid\" fill=\"none\" stroke=\"#e7e5e4\" stroke-width=\"1\">"
    );
    let mut x = 0.0;
    while x <= CANVAS_W as f32 {
        let _ = writeln!(
            out,
            "    <line x1=\"{}\" y1=\"0\" x2=\"{}\" y2=\"{CANVAS_H}\"/>",
            fmt_num(x),
            fmt_num(x)
        );
        x += step;
    }
    let mut y = 0.0;
    while y <= CANVAS_H as f32 {
        let _ = writeln!(
            out,
            "    <line x1=\"0\" y1=\"{}\" x2=\"{CANVAS_W}\" y2=\"{}\"/>",
            fmt_num(y),
            fmt_num(y)
        );
        y += step;
    }
    let _ = writeln!(out, "  </g>");
    let bar = PX_PER_WU;
    let _ = writeln!(
        out,
        "  <g id=\"scale\" stroke=\"#44403c\" fill=\"#44403c\" stroke-width=\"1.5\" font-family=\"Segoe UI,Arial,sans-serif\" font-size=\"12\">\n\
    <line x1=\"{x0}\" y1=\"{y}\" x2=\"{x1}\" y2=\"{y}\"/>\n\
    <text x=\"{x0}\" y=\"{ty}\">1 world unit</text>\n\
  </g>",
        x0 = fmt_num(48.0),
        x1 = fmt_num(48.0 + bar),
        y = fmt_num(CANVAS_H as f32 - 36.0),
        ty = fmt_num(CANVAS_H as f32 - 44.0),
    );
}

fn write_image(out: &mut String, placement: &ImagePlacement, png: &str) {
    let _ = writeln!(
        out,
        "    <image x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" transform=\"translate({},{}) rotate({})\" href=\"data:image/png;base64,{png}\"/>",
        fmt_num(placement.width),
        fmt_num(placement.height),
        fmt_num(placement.x),
        fmt_num(placement.y),
        fmt_num(placement.rotation_deg),
    );
}

fn write_pivot(out: &mut String) {
    let x = PIVOT_X as f32;
    let y = PIVOT_Y as f32;
    let arm = 14.0;
    let _ = writeln!(
        out,
        "  <g id=\"pivot\" stroke=\"#b91c1c\" stroke-width=\"2\" fill=\"#b91c1c\" font-family=\"Segoe UI,Arial,sans-serif\" font-size=\"13\">\n\
    <line x1=\"{}\" y1=\"{y}\" x2=\"{}\" y2=\"{y}\"/>\n\
    <line x1=\"{x}\" y1=\"{}\" x2=\"{x}\" y2=\"{}\"/>\n\
    <text x=\"{}\" y=\"{}\">GripFront</text>\n\
  </g>",
        fmt_num(x - arm),
        fmt_num(x + arm),
        fmt_num(y - arm),
        fmt_num(y + arm),
        fmt_num(x + 18.0),
        fmt_num(y - 16.0),
    );
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
mod tests {
    use super::*;

    #[test]
    fn pivot_is_deterministic_and_hand_tracks_grip_compose() {
        assert_eq!([CANVAS_W, CANVAS_H], [1024, 1024]);
        assert_eq!([PIVOT_X, PIVOT_Y], [512, 512]);
        assert!((PX_PER_WU - 256.0).abs() < f32::EPSILON);
        let (hand, grip) = bind_hand_and_grip();
        let composed = hand.compose(ANCHOR_GRIP);
        assert_eq!(grip, composed);
        let origin = bone_origin_canvas(hand, grip);
        let dx = hand.translation[0] - grip.translation[0];
        let dy = hand.translation[1] - grip.translation[1];
        let world_distance = (dx * dx + dy * dy).sqrt();
        let canvas_distance =
            ((origin[0] - PIVOT_X as f32).powi(2) + (origin[1] - PIVOT_Y as f32).powi(2)).sqrt();
        assert!((canvas_distance - world_distance * PX_PER_WU).abs() < 1e-3);
        assert!(
            canvas_distance > 1.0,
            "the + is GripFront, not the hand bone"
        );
    }

    #[test]
    fn moving_grip_or_hand_pivot_moves_the_reference() {
        let (hand_world, grip) = bind_hand_and_grip();
        let (_ppu, parts) = load_visual_pack().unwrap();
        let hand = hand_visual(&parts).unwrap();
        let placed = place_visual(
            hand.rect_px,
            hand.pivot_px,
            hand.pixels_per_unit,
            hand_world,
            grip,
        );
        let shifted_pivot = [hand.pivot_px[0] + 17.0, hand.pivot_px[1]];
        let moved_pivot = place_visual(
            hand.rect_px,
            shifted_pivot,
            hand.pixels_per_unit,
            hand_world,
            grip,
        );
        assert!((moved_pivot.x - placed.x).abs() + (moved_pivot.y - placed.y).abs() > 1.0);
        let shifted_grip = BoneTransform::from_translation_rotation(
            [grip.translation[0] + 0.25, grip.translation[1]],
            grip.rotation,
        );
        let moved_grip = place_visual(
            hand.rect_px,
            hand.pivot_px,
            hand.pixels_per_unit,
            hand_world,
            shifted_grip,
        );
        assert!((moved_grip.x - placed.x).abs() + (moved_grip.y - placed.y).abs() > 10.0);
    }

    #[test]
    fn svg_marks_the_pivot_and_embeds_the_canonical_hand() {
        let svg = render_svg().unwrap();
        assert!(svg.contains(&format!("width=\"{CANVAS_W}\"")));
        assert!(svg.contains("id=\"pivot\""));
        assert!(svg.contains(&format!("id=\"{HAND_GROUP}\"")));
        assert!(svg.contains(&format!("id=\"{CHARACTER_GROUP}\"")));
        assert!(svg.contains("GripFront"));
        let (_ppu, parts) = load_visual_pack().unwrap();
        let hand = hand_visual(&parts).unwrap();
        let (hand_world, grip) = bind_hand_and_grip();
        let placed = place_visual(
            hand.rect_px,
            hand.pivot_px,
            hand.pixels_per_unit,
            hand_world,
            grip,
        );
        assert!(svg.contains(&format!(
            "translate({},{}) rotate({})",
            fmt_num(placed.x),
            fmt_num(placed.y),
            fmt_num(placed.rotation_deg)
        )));
        let marker = "data:image/png;base64,";
        let hand_section = svg.split(&format!("id=\"{HAND_GROUP}\"")).nth(1).unwrap();
        let start = hand_section.find(marker).unwrap() + marker.len();
        let end = hand_section[start..].find('"').unwrap() + start;
        let decoded = base64_decode(&hand_section[start..end]).unwrap();
        let png = image::load_from_memory(&decoded).unwrap().to_rgba8();
        let atlas = load_atlas().unwrap();
        assert_eq!(png.dimensions(), (hand.rect_px[2], hand.rect_px[3]));
        assert_eq!(
            *png.get_pixel(0, 0),
            *atlas.get_pixel(hand.rect_px[0], hand.rect_px[1])
        );
    }

    #[test]
    fn export_writes_svg_only() {
        let dir = std::env::temp_dir().join("purgatory_weapon_side_master_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("WEAPON_SIDE_MASTER_V1.svg");
        export_to(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            render_svg().unwrap()
        );
    }

    #[test]
    fn write_weapon_side_master_if_requested() {
        if std::env::var("PURGATORY_WRITE_WEAPON_SIDE_MASTER").as_deref() != Ok("1") {
            return;
        }
        export_to(&default_output_path()).expect("write Weapon Side master");
    }

    #[test]
    fn committed_svg_matches_generator() {
        if std::env::var("PURGATORY_WRITE_WEAPON_SIDE_MASTER").as_deref() == Ok("1") {
            return;
        }
        let on_disk = std::fs::read_to_string(default_output_path()).unwrap_or_default();
        assert_eq!(
            on_disk.replace("\r\n", "\n"),
            render_svg().unwrap().replace("\r\n", "\n"),
            "re-export with PURGATORY_WRITE_WEAPON_SIDE_MASTER=1 cargo test -p purgatory-dev-hub --bin purgatory-dev-hub write_weapon_side_master_if_requested"
        );
    }

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
}
