//! Side paper-doll for `equipment.debug.practice_sword`.
//!
//! The inventory and world-drop icon (`item.training_sword`) is a separate
//! 32×32 sprite. This module registers the authored equipment visual key
//! `equipment.debug.practice_sword.side` from the retained paper-doll PNG.
//! The draw path looks that key up in [`crate::asset_runtime::AssetRuntime`];
//! it does not special-case the item id.
//!
//! Item Lab still authors item metadata and the inventory icon only. A later
//! pass should let it author the equipped paper-doll: visual key, Side/Back
//! variants, source artwork, pivot, and this pixels-per-unit value.

use crate::asset_runtime::{AssetRuntime, ResolvedVisual};

pub(crate) const VISUAL_KEY: &str = "equipment.debug.practice_sword.side";

const PNG: &[u8] =
    include_bytes!("../../../Graphic/items/practice_sword/Practice_Sword_papaperdoll.png");

/// Grip pixel on the paper-doll, image origin top-left, Y down.
///
/// Measured as the handle midpoint on the sprite axis between the pommel and
/// the crossguard. This pixel is the visual pivot, so it lands on GripFront.
pub(crate) const GRIP_PIVOT_PX: [f32; 2] = [290.57, 983.43];

/// Authored pixels per world unit.
///
/// Chosen on `WEAPON_SIDE_MASTER_V1` (256 px/wu, weapon pivot at (512, 512))
/// with this grip pixel on the "+". At 1700 the blade tip sits about at the
/// top of the current character's head and the handle reads as a grip in the
/// fist. Runtime does not recompute this from the body.
pub(crate) const PIXELS_PER_UNIT: f32 = 1700.0;

pub(crate) fn register_assets(assets: &mut AssetRuntime) -> Result<(), String> {
    let image = image::load_from_memory(PNG)
        .map_err(|err| format!("decode practice sword paper-doll: {err}"))?
        .to_rgba8();
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return Err("practice sword paper-doll is empty".to_owned());
    }
    let texture = assets.register_image(VISUAL_KEY, image)?;
    assets.register_visual(
        VISUAL_KEY,
        ResolvedVisual {
            texture,
            rect_px: [0, 0, width, height],
            uv: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            pivot_px: GRIP_PIVOT_PX,
            dimensions_px: [width, height],
            pixels_per_unit: PIXELS_PER_UNIT,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grip_pivot_and_authored_scale_register_the_authored_key() {
        let mut assets = AssetRuntime::new();
        let hand_height_formula = 123.0 * 677.0 / 97.0;
        assert!((PIXELS_PER_UNIT - hand_height_formula).abs() > 1.0);
        register_assets(&mut assets).unwrap();
        let visual = assets.visual(VISUAL_KEY).unwrap();
        assert_eq!(visual.pivot_px, GRIP_PIVOT_PX);
        assert_eq!(visual.pivot_px, [290.57, 983.43]);
        assert!((visual.pixels_per_unit - PIXELS_PER_UNIT).abs() < f32::EPSILON);
        assert_eq!(visual.dimensions_px, [1254, 1254]);
        let err = register_assets(&mut assets).unwrap_err();
        assert!(err.contains(VISUAL_KEY), "{err}");
    }
}
