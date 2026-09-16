//! Client-owned embedded sprite resources.
//!
//! This layer owns decoded resource data and stable client texture identities.
//! Domain code owns what a visual means; the renderer owns submission.

use std::collections::HashMap;

use image::{Rgba, RgbaImage};

use crate::renderer::SpriteTextureId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolvedVisual {
    pub(crate) texture: SpriteTextureId,
    pub(crate) rect_px: [u32; 4],
    pub(crate) uv: [[f32; 2]; 4],
    pub(crate) pivot_px: [f32; 2],
    pub(crate) dimensions_px: [u32; 2],
    pub(crate) pixels_per_unit: f32,
}

#[derive(Debug)]
pub(crate) struct SpriteResource {
    pub(crate) id: SpriteTextureId,
    pub(crate) image: RgbaImage,
}

#[derive(Default, Debug)]
pub(crate) struct AssetRuntime {
    resources: Vec<SpriteResource>,
    by_key: HashMap<String, SpriteTextureId>,
    visuals: HashMap<String, ResolvedVisual>,
    next_id: u32,
}

impl AssetRuntime {
    pub(crate) fn new() -> Self {
        Self {
            resources: Vec::new(),
            by_key: HashMap::new(),
            visuals: HashMap::new(),
            next_id: 1,
        }
    }

    pub(crate) fn register_png(
        &mut self,
        key: &str,
        bytes: &[u8],
    ) -> Result<SpriteTextureId, String> {
        if let Some(&id) = self.by_key.get(key) {
            return Ok(id);
        }
        let mut image = image::load_from_memory(bytes)
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
    }

    pub(crate) fn register_image(
        &mut self,
        key: &str,
        image: RgbaImage,
    ) -> Result<SpriteTextureId, String> {
        if let Some(&id) = self.by_key.get(key) {
            return Ok(id);
        }
        let id = SpriteTextureId::from_raw(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.by_key.insert(key.to_owned(), id);
        self.resources.push(SpriteResource { id, image });
        Ok(id)
    }

    pub(crate) fn register_visual(
        &mut self,
        key: &str,
        visual: ResolvedVisual,
    ) -> Result<(), String> {
        if self.visuals.insert(key.to_owned(), visual).is_some() {
            return Err(format!("duplicate visual key {key}"));
        }
        Ok(())
    }

    pub(crate) fn visual(&self, key: &str) -> Option<&ResolvedVisual> {
        self.visuals.get(key)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn resource(&self, id: SpriteTextureId) -> Option<&SpriteResource> {
        self.resources.iter().find(|resource| resource.id == id)
    }

    pub(crate) fn resources(&self) -> &[SpriteResource] {
        &self.resources
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn resource_count(&self) -> usize {
        self.resources.len()
    }
}

const UI_ATLAS_SIZE: [u32; 2] = [768, 283];
const WINDOW_SOURCE_X: [u32; 6] = [7, 130, 253, 376, 498, 620];
const BUTTON_SOURCE_X: [u32; 6] = [6, 61, 115, 170, 225, 280];

fn normalize_ui_atlas_v1(source: RgbaImage) -> Result<RgbaImage, String> {
    if [source.width(), source.height()] != UI_ATLAS_SIZE {
        return Err(format!(
            "UI atlas v1 expected {}x{}, decoded {}x{}",
            UI_ATLAS_SIZE[0],
            UI_ATLAS_SIZE[1],
            source.width(),
            source.height()
        ));
    }

    let mut out = RgbaImage::from_pixel(
        UI_ATLAS_SIZE[0],
        UI_ATLAS_SIZE[1],
        Rgba([0, 0, 0, 0]),
    );

    for (index, source_x) in WINDOW_SOURCE_X.into_iter().enumerate() {
        copy_normalized_window(&source, &mut out, source_x, index as u32 * 128)?;
    }

    for (row, source_y) in [210_u32, 228, 246].into_iter().enumerate() {
        for (index, source_x) in BUTTON_SOURCE_X.into_iter().enumerate() {
            copy_horizontal_fixed_caps(
                &source,
                &mut out,
                [source_x, source_y, 54, 18],
                [index as u32 * 56, 210 + row as u32 * 18, 56, 18],
                8,
                8,
            )?;
        }
    }

    let icon_centers_top = [
        354_u32, 384, 414, 444, 474, 504, 536, 567, 600, 634, 666, 698,
    ];
    let icon_centers_bottom = [
        352_u32, 380, 410, 435, 459, 487, 518, 547, 577, 606, 636, 665,
    ];
    for (index, center_x) in icon_centers_top.into_iter().enumerate() {
        copy_centered_cell(
            &source,
            &mut out,
            center_x,
            224,
            336 + index as u32 * 32,
            208,
            32,
        )?;
    }
    for (index, center_x) in icon_centers_bottom.into_iter().enumerate() {
        copy_centered_cell(
            &source,
            &mut out,
            center_x,
            256,
            336 + index as u32 * 32,
            240,
            32,
        )?;
    }

    for (index, source_y) in [211_u32, 231, 251].into_iter().enumerate() {
        copy_rect(
            &source,
            &mut out,
            [720, source_y, 24, 18],
            [720, 210 + index as u32 * 18],
        )?;
    }

    Ok(out)
}

fn copy_normalized_window(
    source: &RgbaImage,
    target: &mut RgbaImage,
    source_x: u32,
    target_x: u32,
) -> Result<(), String> {
    for target_y in 0..177_u32 {
        let source_rel_y = remap_fixed_ends(target_y, 177, 32, 8, 35, 10);
        let source_y = 24 + source_rel_y;
        for target_rel_x in 0..128_u32 {
            let source_rel_x = remap_fixed_ends(target_rel_x, 128, 8, 8, 8, 8);
            let pixel = checked_pixel(source, source_x + source_rel_x, source_y)?;
            target.put_pixel(target_x + target_rel_x, 24 + target_y, pixel);
        }
    }
    Ok(())
}

fn copy_horizontal_fixed_caps(
    source: &RgbaImage,
    target: &mut RgbaImage,
    src: [u32; 4],
    dst: [u32; 4],
    left: u32,
    right: u32,
) -> Result<(), String> {
    if src[3] != dst[3] {
        return Err("UI atlas button normalization requires equal heights".to_string());
    }
    for y in 0..dst[3] {
        for x in 0..dst[2] {
            let sx = remap_fixed_ends(x, dst[2], left, right, left, right);
            let pixel = checked_pixel(source, src[0] + sx, src[1] + y)?;
            target.put_pixel(dst[0] + x, dst[1] + y, pixel);
        }
    }
    Ok(())
}

fn remap_fixed_ends(
    target_index: u32,
    target_len: u32,
    target_start: u32,
    target_end: u32,
    source_start: u32,
    source_end: u32,
) -> u32 {
    let source_len = match (target_len, target_start, target_end) {
        (128, 8, 8) => 120,
        (177, 32, 8) => 177,
        (56, 8, 8) => 54,
        _ => target_len,
    };
    if target_index < target_start {
        return target_index.min(source_start.saturating_sub(1));
    }
    if target_index >= target_len.saturating_sub(target_end) {
        let from_end = target_len - 1 - target_index;
        return source_len - 1 - from_end.min(source_end.saturating_sub(1));
    }

    let source_middle_start = source_start;
    let source_middle_end = source_len.saturating_sub(source_end);
    let source_middle_len = source_middle_end.saturating_sub(source_middle_start).max(1);
    let target_middle_index = target_index - target_start;
    let target_middle_len = target_len.saturating_sub(target_start + target_end).max(1);

    if source_middle_len >= target_middle_len {
        let trim = source_middle_len - target_middle_len;
        source_middle_start + trim / 2 + target_middle_index
    } else {
        let extra = target_middle_len - source_middle_len;
        let repeat_at = source_middle_len / 2;
        if target_middle_index < repeat_at {
            source_middle_start + target_middle_index
        } else if target_middle_index < repeat_at + extra {
            source_middle_start + repeat_at
        } else {
            source_middle_start + target_middle_index - extra
        }
    }
}

fn copy_centered_cell(
    source: &RgbaImage,
    target: &mut RgbaImage,
    center_x: u32,
    center_y: u32,
    target_x: u32,
    target_y: u32,
    size: u32,
) -> Result<(), String> {
    let half = size / 2;
    let source_x = center_x.saturating_sub(half);
    let source_y = center_y.saturating_sub(half);
    copy_rect(
        source,
        target,
        [source_x, source_y, size, size],
        [target_x, target_y],
    )
}

fn copy_rect(
    source: &RgbaImage,
    target: &mut RgbaImage,
    src: [u32; 4],
    dst: [u32; 2],
) -> Result<(), String> {
    for y in 0..src[3] {
        for x in 0..src[2] {
            let pixel = checked_pixel(source, src[0] + x, src[1] + y)?;
            if dst[0] + x >= target.width() || dst[1] + y >= target.height() {
                return Err("UI atlas normalized destination is out of bounds".to_string());
            }
            target.put_pixel(dst[0] + x, dst[1] + y, pixel);
        }
    }
    Ok(())
}

fn checked_pixel(image: &RgbaImage, x: u32, y: u32) -> Result<Rgba<u8>, String> {
    if x >= image.width() || y >= image.height() {
        return Err(format!(
            "UI atlas normalization source pixel {x},{y} is out of bounds {}x{}",
            image.width(),
            image.height()
        ));
    }
    Ok(*image.get_pixel(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_end_remap_never_moves_window_borders() {
        assert_eq!(remap_fixed_ends(0, 128, 8, 8, 8, 8), 0);
        assert_eq!(remap_fixed_ends(7, 128, 8, 8, 8, 8), 7);
        assert_eq!(remap_fixed_ends(120, 128, 8, 8, 8, 8), 112);
        assert_eq!(remap_fixed_ends(127, 128, 8, 8, 8, 8), 119);
    }
}
