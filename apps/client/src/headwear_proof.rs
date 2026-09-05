//! Compile-time Headwear Side sprite proof.
//!
//! Same PNGs and Crown local space as Animation Lab. Not a runtime loader,
//! registry, or asset pipeline. Gameplay still resolves Headwear →
//! [`crate::character_presentation`] attachments; this module only supplies
//! the Side sprite quad. Player debug overlay selects cells 1–4 (a–d).

#[cfg(test)]
use purgatory_skeleton::BoneTransform;

use crate::asset_runtime::{AssetRuntime, ResolvedVisual};
#[cfg(test)]
use crate::renderer::{DrawQuad, SpriteTextureId};
#[cfg(test)]
use crate::skeleton_debug::{sanitize_preview_scale, scale_about_root};

/// Same visual-key string Animation Lab uses. Content packs may use a
/// different Side key (`equipment.debug.cloth_cap.side`); the sprite is still
/// one of the compile-embedded cells until a real atlas exists.
pub const VISUAL_KEYS: [&str; CELL_COUNT] = [
    "equipment.debug.headwear_proof.a.side",
    "equipment.debug.headwear_proof.b.side",
    "equipment.debug.headwear_proof.c.side",
    "equipment.debug.headwear_proof.d.side",
];
pub const VISUAL_KEY: &str = VISUAL_KEYS[0];
pub const SPRITE_PX: u32 = 256;
pub const ATLAS_PX: u32 = SPRITE_PX * 2;
pub const CELL_COUNT: usize = 4;
pub const CROWN_LOCAL_X: f32 = 128.0;
pub const CROWN_LOCAL_Y: f32 = 176.0;
pub const PX_PER_WU: f32 = 256.0;

/// Animation Lab extracted Side cells (a–d / HEADWEAR 1–4). Embedded at compile time.
pub const PNG_CELLS: [&[u8]; CELL_COUNT] = [
    include_bytes!(
        "../../../Graphic/character/headwear_side/extracted/equipment.debug.headwear_proof.a.side.png"
    ),
    include_bytes!(
        "../../../Graphic/character/headwear_side/extracted/equipment.debug.headwear_proof.b.side.png"
    ),
    include_bytes!(
        "../../../Graphic/character/headwear_side/extracted/equipment.debug.headwear_proof.c.side.png"
    ),
    include_bytes!(
        "../../../Graphic/character/headwear_side/extracted/equipment.debug.headwear_proof.d.side.png"
    ),
];

#[must_use]
pub fn clamp_cell(cell: u8) -> u8 {
    cell.min((CELL_COUNT - 1) as u8)
}

/// GPU UVs for one 256 cell inside the 2×2 atlas (a=r0c0, b=r0c1, c=r1c0, d=r1c1).
#[must_use]
pub fn gpu_uvs_for_cell(cell: u8) -> [[f32; 2]; 4] {
    let i = usize::from(clamp_cell(cell));
    let col = (i % 2) as f32;
    let row = (i / 2) as f32;
    let u0 = col * 0.5;
    let u1 = u0 + 0.5;
    let v0 = row * 0.5;
    let v1 = v0 + 0.5;
    [[u0, v1], [u1, v1], [u1, v0], [u0, v0]]
}

/// 512×512 atlas of the four extracted Side cells. CPU composite; no filesystem.
pub fn atlas_rgba() -> Result<image::RgbaImage, String> {
    let cell = SPRITE_PX;
    let mut atlas = image::RgbaImage::new(ATLAS_PX, ATLAS_PX);
    for (i, bytes) in PNG_CELLS.iter().enumerate() {
        let img = image::load_from_memory(bytes)
            .map_err(|err| format!("decode headwear cell {i}: {err}"))?
            .to_rgba8();
        if img.width() != cell || img.height() != cell {
            return Err(format!(
                "headwear cell {i} must be {cell}x{cell}, got {}x{}",
                img.width(),
                img.height()
            ));
        }
        let x = (i as u32 % 2) * cell;
        let y = (i as u32 / 2) * cell;
        image::imageops::replace(&mut atlas, &img, i64::from(x), i64::from(y));
    }
    Ok(atlas)
}

pub fn register_assets(assets: &mut AssetRuntime) -> Result<(), String> {
    let atlas = atlas_rgba()?;
    let texture = assets.register_image("equipment.debug.headwear_proof.atlas", atlas)?;
    for (index, key) in VISUAL_KEYS.iter().enumerate() {
        let x = (index as u32 % 2) * SPRITE_PX;
        let y = (index as u32 / 2) * SPRITE_PX;
        assets.register_visual(
            key,
            ResolvedVisual {
                texture,
                rect_px: [x, y, SPRITE_PX, SPRITE_PX],
                uv: gpu_uvs_for_cell(index as u8),
                pivot_px: [CROWN_LOCAL_X, CROWN_LOCAL_Y],
                dimensions_px: [SPRITE_PX, SPRITE_PX],
                pixels_per_unit: PX_PER_WU,
            },
        )?;
    }
    // The authored cloth-cap key is the currently shipped first proof cell.
    let first_visual = *assets
        .visual(VISUAL_KEY)
        .expect("registered headwear proof key");
    assets.register_visual("equipment.debug.cloth_cap.side", first_visual)?;
    Ok(())
}

#[must_use]
#[cfg(test)]
pub fn pixel_to_local(px: f32, py: f32) -> [f32; 2] {
    [
        (px - CROWN_LOCAL_X) / PX_PER_WU,
        (CROWN_LOCAL_Y - py) / PX_PER_WU,
    ]
}

/// Image-space TL, TR, BR, BL (Lab order).
#[must_use]
#[cfg(test)]
pub fn sprite_local_corners_lab() -> [[f32; 2]; 4] {
    let w = SPRITE_PX as f32;
    let h = SPRITE_PX as f32;
    [
        pixel_to_local(0.0, 0.0),
        pixel_to_local(w, 0.0),
        pixel_to_local(w, h),
        pixel_to_local(0.0, h),
    ]
}

/// GPU winding: BL, BR, TR, TL.
#[must_use]
#[cfg(test)]
pub fn sprite_local_corners_gpu() -> [[f32; 2]; 4] {
    let lab = sprite_local_corners_lab();
    [lab[3], lab[2], lab[1], lab[0]]
}

#[cfg(test)]
#[must_use]
pub fn apply_local(xf: BoneTransform, local: [f32; 2]) -> [f32; 2] {
    let (sin, cos) = xf.rotation.sin_cos();
    [
        xf.translation[0] + local[0] * cos - local[1] * sin,
        xf.translation[1] + local[0] * sin + local[1] * cos,
    ]
}

/// Textured crown sprite. `xf` is [`super::character_presentation`] compose
/// (`HEAD world ∘ ANCHOR_CROWN ∘ correction`). Local origin is Crown.
/// `cell` is atlas index `0..=3` (HEADWEAR 1–4).
#[must_use]
#[cfg(test)]
pub fn sprite_quad_for_cell(
    xf: BoneTransform,
    preview_scale: f32,
    root: [f32; 2],
    cell: u8,
) -> DrawQuad {
    let scale = sanitize_preview_scale(preview_scale);
    let pivot = scale_about_root(xf.translation, root, scale);
    let locals = sprite_local_corners_gpu().map(|p| [p[0] * scale, p[1] * scale]);
    DrawQuad::textured_sprite(
        SpriteTextureId::HEADWEAR,
        pivot,
        locals,
        gpu_uvs_for_cell(cell),
        xf.rotation,
    )
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
    fn embedded_pngs_are_lab_side_cells() {
        assert_eq!(VISUAL_KEY, VISUAL_KEYS[0]);
        for (i, bytes) in PNG_CELLS.iter().enumerate() {
            assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']), "cell {i}");
            let img = image::load_from_memory(bytes).expect("decode");
            assert_eq!(img.width(), SPRITE_PX);
            assert_eq!(img.height(), SPRITE_PX);
        }
        let rgba = image::load_from_memory(PNG_CELLS[0])
            .expect("decode a")
            .to_rgba8();
        assert_ne!(
            rgba.get_pixel(CROWN_LOCAL_X as u32, CROWN_LOCAL_Y as u32)[3],
            0
        );
    }

    #[test]
    fn atlas_is_2x2_cells() {
        let atlas = atlas_rgba().expect("atlas");
        assert_eq!(atlas.width(), ATLAS_PX);
        assert_eq!(atlas.height(), ATLAS_PX);
        let a = image::load_from_memory(PNG_CELLS[0]).unwrap().to_rgba8();
        assert_eq!(atlas.get_pixel(0, 0), a.get_pixel(0, 0));
        let b = image::load_from_memory(PNG_CELLS[1]).unwrap().to_rgba8();
        assert_eq!(atlas.get_pixel(SPRITE_PX, 0), b.get_pixel(0, 0));
    }

    #[test]
    fn headwear_visual_keys_resolve_through_shared_asset_runtime() {
        let mut assets = AssetRuntime::new();
        register_assets(&mut assets).unwrap();
        let proof = assets.visual(VISUAL_KEYS[0]).unwrap();
        let cloth_cap = assets.visual("equipment.debug.cloth_cap.side").unwrap();
        assert_eq!(assets.resource_count(), 1);
        assert_eq!(proof.texture, SpriteTextureId::HEADWEAR);
        assert_eq!(cloth_cap, proof);
        assert_eq!(proof.rect_px, [0, 0, SPRITE_PX, SPRITE_PX]);
        assert_eq!(proof.pivot_px, [CROWN_LOCAL_X, CROWN_LOCAL_Y]);
        assert_eq!(proof.pixels_per_unit, PX_PER_WU);
    }

    #[test]
    fn cell_uvs_cover_atlas_quadrants() {
        assert_eq!(
            gpu_uvs_for_cell(0),
            [[0.0, 0.5], [0.5, 0.5], [0.5, 0.0], [0.0, 0.0]]
        );
        assert_eq!(
            gpu_uvs_for_cell(1),
            [[0.5, 0.5], [1.0, 0.5], [1.0, 0.0], [0.5, 0.0]]
        );
        assert_eq!(gpu_uvs_for_cell(3), gpu_uvs_for_cell(99));
    }

    #[test]
    fn crown_pixel_is_local_origin_and_sprite_is_one_wu() {
        assert_eq!(pixel_to_local(CROWN_LOCAL_X, CROWN_LOCAL_Y), [0.0, 0.0]);
        let corners = sprite_local_corners_lab();
        let width = corners[1][0] - corners[0][0];
        let height = corners[0][1] - corners[3][1];
        assert!((width - 1.0).abs() < 1e-6);
        assert!((height - 1.0).abs() < 1e-6);
    }

    #[test]
    fn gpu_winding_keeps_crown_inside_quad() {
        let xf = bind_world()
            .get(HEAD)
            .unwrap()
            .compose(purgatory_skeleton::ANCHOR_CROWN);
        let quad = sprite_quad_for_cell(xf, 1.0, [0.0, 0.0], 0);
        assert!(quad.is_textured());
        assert_eq!(quad.uvs(), gpu_uvs_for_cell(0));
        let cell1 = sprite_quad_for_cell(xf, 1.0, [0.0, 0.0], 1);
        assert_eq!(cell1.uvs(), gpu_uvs_for_cell(1));
        assert_ne!(quad.uvs(), cell1.uvs());
        let crown = apply_local(xf, [0.0, 0.0]);
        assert!((crown[0] - xf.translation[0]).abs() < 1e-5);
        assert!((crown[1] - xf.translation[1]).abs() < 1e-5);
        for c in quad.world_corners() {
            let dx = c[0] - xf.translation[0];
            let dy = c[1] - xf.translation[1];
            let dist = (dx * dx + dy * dy).sqrt();
            assert!(dist > 0.2);
            assert!(dist < 0.9);
        }
    }

    #[test]
    fn rotation_follows_head_without_pivot_drift() {
        let def = humanoid_v0();
        let mut local = LocalPose::from_bind(def);
        let mut world = WorldPose::new(def);
        local.get_mut(HEAD).unwrap().rotation = 0.6;
        evaluate(def, &local, &mut world).unwrap();
        let xf = world
            .get(HEAD)
            .unwrap()
            .compose(purgatory_skeleton::ANCHOR_CROWN);
        let quad = sprite_quad_for_cell(xf, 1.0, [0.0, 0.0], 0);
        assert!((xf.rotation - world.get(HEAD).unwrap().rotation).abs() < 1e-5);
        let crown = apply_local(xf, [0.0, 0.0]);
        assert!((crown[0] - xf.translation[0]).abs() < 1e-5);
        assert!((crown[1] - xf.translation[1]).abs() < 1e-5);
        for c in quad.world_corners() {
            let dx = c[0] - xf.translation[0];
            let dy = c[1] - xf.translation[1];
            assert!((dx * dx + dy * dy).sqrt() < 0.9);
        }
    }
}
