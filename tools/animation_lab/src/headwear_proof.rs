//! DEV-only Headwear Side sprite proof for Animation Lab.
//!
//! One visual key, one embedded-path PNG, compose = `HEAD world ∘ ANCHOR_CROWN`.
//! Not a production asset loader. The game client does not use this module.

use std::path::{Path, PathBuf};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, Rgba, RgbaImage};
use purgatory_skeleton::{ANCHOR_CROWN, BoneTransform, HEAD, WorldPose};

/// Logical visual key. Same string the content pack will use later; not a path.
pub const VISUAL_KEY: &str = "equipment.debug.headwear_proof.a.side";
pub const SPRITE_PX: u32 = 256;
pub const CROWN_LOCAL_X: f32 = 128.0;
pub const CROWN_LOCAL_Y: f32 = 176.0;
/// Headwear Side master / Humanoid authoring scale (not the 8B 128 px correction canvas).
pub const PX_PER_WU: f32 = 256.0;

const OUTPUT_REL: &str =
    "Graphic/character/headwear_side/extracted/equipment.debug.headwear_proof.a.side.png";

#[must_use]
pub fn proof_png_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(OUTPUT_REL)
}

/// `world(HEAD) ∘ ANCHOR_CROWN` (identity correction). Same contract as client compose.
#[must_use]
pub fn compose_crown(world: &WorldPose) -> Option<BoneTransform> {
    Some(world.get(HEAD)?.compose(ANCHOR_CROWN))
}

/// Sprite corners in attachment-local space (Y up). Local origin is Crown.
///
/// `local_x = (px - crown_x) / PX_PER_WU`
/// `local_y = (crown_y - py) / PX_PER_WU`
#[must_use]
pub fn sprite_local_corners() -> [[f32; 2]; 4] {
    let w = SPRITE_PX as f32;
    let h = SPRITE_PX as f32;
    [
        pixel_to_local(0.0, 0.0),
        pixel_to_local(w, 0.0),
        pixel_to_local(w, h),
        pixel_to_local(0.0, h),
    ]
}

#[must_use]
pub fn pixel_to_local(px: f32, py: f32) -> [f32; 2] {
    [
        (px - CROWN_LOCAL_X) / PX_PER_WU,
        (CROWN_LOCAL_Y - py) / PX_PER_WU,
    ]
}

#[must_use]
pub fn apply_local(xf: BoneTransform, local: [f32; 2]) -> [f32; 2] {
    let (sin, cos) = xf.rotation.sin_cos();
    [
        xf.translation[0] + local[0] * cos - local[1] * sin,
        xf.translation[1] + local[0] * sin + local[1] * cos,
    ]
}

/// World-space quad (TL, TR, BR, BL) for the untrimmed 256×256 sprite.
#[must_use]
pub fn sprite_world_corners(world: &WorldPose) -> Option<[[f32; 2]; 4]> {
    let xf = compose_crown(world)?;
    Some(sprite_local_corners().map(|p| apply_local(xf, p)))
}

pub fn load_rgba(path: &Path) -> Result<RgbaImage, String> {
    let bytes = std::fs::read(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let decoded = image::load_from_memory(&bytes).map_err(|err| format!("decode PNG: {err}"))?;
    let img = decoded.to_rgba8();
    if img.width() != SPRITE_PX || img.height() != SPRITE_PX {
        return Err(format!(
            "proof PNG is {}×{}, expected {SPRITE_PX}×{SPRITE_PX}",
            img.width(),
            img.height()
        ));
    }
    Ok(img)
}

pub fn write_extracted_png(path: &Path) -> Result<PathBuf, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("create dir: {err}"))?;
    }
    std::fs::write(path, encode_png(&render_extracted_png())?)
        .map_err(|err| format!("write {}: {err}", path.display()))?;
    Ok(path.to_path_buf())
}

/// Distinctive Side cap whose head-contact covers Crown (128, 176). Not a master sheet.
#[must_use]
pub fn render_extracted_png() -> RgbaImage {
    let mut img = RgbaImage::new(SPRITE_PX, SPRITE_PX);
    let cx = CROWN_LOCAL_X as i32;
    let cy = CROWN_LOCAL_Y as i32;
    let brown = Rgba([168, 92, 42, 255]);
    let dark = Rgba([92, 48, 22, 255]);
    fill_ellipse(&mut img, cx, cy - 22, 46, 30, brown);
    fill_rect(&mut img, cx - 54, cy - 6, 118, 12, dark);
    fill_rect(&mut img, cx + 40, cy - 4, 28, 8, brown);
    img.put_pixel(CROWN_LOCAL_X as u32, CROWN_LOCAL_Y as u32, brown);
    img
}

fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, color: Rgba<u8>) {
    let max_x = img.width() as i32;
    let max_y = img.height() as i32;
    for py in y.max(0)..(y + h).min(max_y) {
        for px in x.max(0)..(x + w).min(max_x) {
            img.put_pixel(px as u32, py as u32, color);
        }
    }
}

fn fill_ellipse(img: &mut RgbaImage, cx: i32, cy: i32, rx: i32, ry: i32, color: Rgba<u8>) {
    if rx <= 0 || ry <= 0 {
        return;
    }
    let max_x = img.width() as i32;
    let max_y = img.height() as i32;
    let rx2 = (rx * rx) as f32;
    let ry2 = (ry * ry) as f32;
    for py in (cy - ry).max(0)..(cy + ry + 1).min(max_y) {
        for px in (cx - rx).max(0)..(cx + rx + 1).min(max_x) {
            let dx = (px - cx) as f32;
            let dy = (py - cy) as f32;
            if (dx * dx) / rx2 + (dy * dy) / ry2 <= 1.0 {
                img.put_pixel(px as u32, py as u32, color);
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_skeleton::{HEAD, LocalPose, WorldPose, evaluate, humanoid_v0};

    fn bind_world() -> WorldPose {
        let def = humanoid_v0();
        let local = LocalPose::from_bind(def);
        let mut world = WorldPose::new(def);
        evaluate(def, &local, &mut world).unwrap();
        world
    }

    #[test]
    fn visual_key_is_the_selection_key_not_a_path() {
        assert_eq!(VISUAL_KEY, "equipment.debug.headwear_proof.a.side");
        assert!(!VISUAL_KEY.contains('/'));
        assert!(!VISUAL_KEY.ends_with(".png"));
    }

    #[test]
    fn compose_is_head_world_then_crown_anchor() {
        let world = bind_world();
        let got = compose_crown(&world).unwrap();
        let want = world.get(HEAD).unwrap().compose(ANCHOR_CROWN);
        assert_eq!(got.translation, want.translation);
        assert_eq!(got.rotation, want.rotation);
    }

    #[test]
    fn crown_pixel_is_local_origin() {
        assert_eq!(pixel_to_local(CROWN_LOCAL_X, CROWN_LOCAL_Y), [0.0, 0.0]);
        let xf = compose_crown(&bind_world()).unwrap();
        let p = apply_local(xf, [0.0, 0.0]);
        assert!((p[0] - xf.translation[0]).abs() < 1e-6);
        assert!((p[1] - xf.translation[1]).abs() < 1e-6);
    }

    #[test]
    fn sprite_is_one_wu_at_authoring_scale() {
        let corners = sprite_local_corners();
        let width = corners[1][0] - corners[0][0];
        let height = corners[0][1] - corners[3][1];
        assert!((width - 1.0).abs() < 1e-6);
        assert!((height - 1.0).abs() < 1e-6);
        assert!((PX_PER_WU - 256.0).abs() < f32::EPSILON);
    }

    #[test]
    fn rotation_orbits_crown_without_pivot_drift() {
        let def = humanoid_v0();
        let mut local = LocalPose::from_bind(def);
        local.get_mut(HEAD).unwrap().rotation = 0.6;
        let mut world = WorldPose::new(def);
        evaluate(def, &local, &mut world).unwrap();
        let xf = compose_crown(&world).unwrap();
        let corners = sprite_world_corners(&world).unwrap();
        for c in corners {
            let dx = c[0] - xf.translation[0];
            let dy = c[1] - xf.translation[1];
            let dist = (dx * dx + dy * dy).sqrt();
            assert!(dist > 0.2);
            assert!(dist < 0.9);
        }
        let crown = apply_local(xf, [0.0, 0.0]);
        assert!((crown[0] - xf.translation[0]).abs() < 1e-5);
        assert!((crown[1] - xf.translation[1]).abs() < 1e-5);
        assert!((xf.rotation - world.get(HEAD).unwrap().rotation).abs() < 1e-5);
    }

    #[test]
    fn extracted_png_covers_crown_and_is_untrimmed() {
        let img = render_extracted_png();
        assert_eq!(img.width(), SPRITE_PX);
        assert_eq!(img.height(), SPRITE_PX);
        assert_eq!(img.get_pixel(0, 0)[3], 0);
        assert_eq!(img.get_pixel(SPRITE_PX - 1, SPRITE_PX - 1)[3], 0);
        assert_ne!(
            img.get_pixel(CROWN_LOCAL_X as u32, CROWN_LOCAL_Y as u32)[3],
            0
        );
    }

    #[test]
    fn write_extracted_png_if_requested() {
        if std::env::var("PURGATORY_WRITE_HEADWEAR_PROOF_PNG").as_deref() != Ok("1") {
            return;
        }
        write_extracted_png(&proof_png_path()).expect("write extracted proof PNG");
    }

    #[test]
    fn committed_extracted_png_is_256_cell() {
        let on_disk = load_rgba(&proof_png_path()).expect(
            "missing extracted Headwear Side cell; Hub Content → Extract Headwear Side cells",
        );
        assert_eq!(on_disk.width(), SPRITE_PX);
        assert_eq!(on_disk.height(), SPRITE_PX);
    }
}
