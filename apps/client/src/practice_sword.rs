//! Side paper-doll for `equipment.debug.practice_sword`.
//!
//! The inventory and world-drop icon (`item.training_sword`) is a separate
//! 32×32 sprite. This module registers the authored equipment visual key
//! `equipment.debug.practice_sword.side` from the retained paper-doll PNG.
//! The draw path looks that key up in [`crate::asset_runtime::AssetRuntime`];
//! it does not special-case the item id.

use crate::asset_runtime::{AssetRuntime, ResolvedVisual};

pub(crate) const VISUAL_KEY: &str = "equipment.debug.practice_sword.side";

const PNG: &[u8] =
    include_bytes!("../../../Graphic/items/practice_sword/Practice_Sword_papaperdoll.png");

/// Grip pixel on the paper-doll, image origin top-left, Y down.
///
/// Measured as the handle midpoint on the sprite axis between the pommel and
/// the crossguard. This pixel is the visual pivot, so it lands on GripFront.
pub(crate) const GRIP_PIVOT_PX: [f32; 2] = [290.57, 983.43];

/// Opaque thickness of the handle at the grip, perpendicular to the blade.
const HANDLE_THICKNESS_PX: f32 = 123.0;

/// Scale the paper-doll so the handle thickness matches the hand sprite's
/// world height. The grip stays on the hand without a world-space offset.
#[must_use]
pub(crate) fn pixels_per_unit_for_hand(
    hand_height_px: u32,
    hand_pixels_per_unit: f32,
) -> Option<f32> {
    if hand_height_px == 0 || !hand_pixels_per_unit.is_finite() || hand_pixels_per_unit <= 0.0 {
        return None;
    }
    let hand_world = hand_height_px as f32 / hand_pixels_per_unit;
    let ppu = HANDLE_THICKNESS_PX / hand_world;
    if ppu.is_finite() && ppu > 0.0 {
        Some(ppu)
    } else {
        None
    }
}

pub(crate) fn register_assets(
    assets: &mut AssetRuntime,
    hand: ResolvedVisual,
) -> Result<(), String> {
    let image = image::load_from_memory(PNG)
        .map_err(|err| format!("decode practice sword paper-doll: {err}"))?
        .to_rgba8();
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return Err("practice sword paper-doll is empty".to_owned());
    }
    let pixels_per_unit = pixels_per_unit_for_hand(hand.dimensions_px[1], hand.pixels_per_unit)
        .ok_or("practice sword scale requires a positive hand visual")?;
    let texture = assets.register_image(VISUAL_KEY, image)?;
    assets.register_visual(
        VISUAL_KEY,
        ResolvedVisual {
            texture,
            rect_px: [0, 0, width, height],
            uv: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            pivot_px: GRIP_PIVOT_PX,
            dimensions_px: [width, height],
            pixels_per_unit,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grip_pivot_and_hand_scale_register_the_authored_key() {
        let mut assets = AssetRuntime::new();
        let hand = ResolvedVisual {
            texture: crate::renderer::SpriteTextureId::from_raw(1),
            rect_px: [0, 0, 99, 97],
            uv: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            pivot_px: [0.0, 0.0],
            dimensions_px: [99, 97],
            pixels_per_unit: 677.0,
        };
        let expected = pixels_per_unit_for_hand(97, 677.0).unwrap();
        assert!((expected - (123.0 * 677.0 / 97.0)).abs() < 1e-3);
        register_assets(&mut assets, hand).unwrap();
        let visual = assets.visual(VISUAL_KEY).unwrap();
        assert_eq!(visual.pivot_px, GRIP_PIVOT_PX);
        assert!((visual.pixels_per_unit - expected).abs() < 1e-3);
        assert!(visual.dimensions_px[0] > 0 && visual.dimensions_px[1] > 0);
        let err = register_assets(&mut assets, hand).unwrap_err();
        assert!(err.contains(VISUAL_KEY), "{err}");
    }
}
