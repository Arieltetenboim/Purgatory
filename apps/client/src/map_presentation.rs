use std::collections::HashMap;

use purgatory_content::{MAP_PRESENTATION_SCHEMA_VERSION, MapPresentation, PresentationSprite};
use purgatory_simulation::{MapId, WorldBounds};
use serde_json::from_slice;

use crate::asset_runtime::AssetRuntime;
use crate::assets::ClientAssetLoader;
use crate::renderer::{DrawQuad, SpriteTextureId};

const COMPILED_PRESENTATION: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/map.dev.footnote.presentation.json"
));

pub(crate) struct RuntimeMapPresentation {
    map_id: MapId,
    canonical_bounds: [f32; 4],
    sprites: Vec<RuntimeSprite>,
}

struct RuntimeSprite {
    authored: PresentationSprite,
    texture: SpriteTextureId,
    image_dimensions: [u32; 2],
    opacity: f32,
}

impl RuntimeMapPresentation {
    pub(crate) fn load(assets: &mut AssetRuntime) -> Result<Self, String> {
        let map: MapPresentation = from_slice(COMPILED_PRESENTATION)
            .map_err(|error| format!("decode compiled map presentation: {error}"))?;
        if map.schema_version != MAP_PRESENTATION_SCHEMA_VERSION {
            return Err(format!(
                "compiled map presentation schema {} is unsupported; expected {}",
                map.schema_version, MAP_PRESENTATION_SCHEMA_VERSION
            ));
        }
        if map.map_authored != purgatory_common::MAP_FOOTNOTE_AUTHORED {
            return Err(format!(
                "compiled map presentation targets {}, expected {}",
                map.map_authored,
                purgatory_common::MAP_FOOTNOTE_AUTHORED
            ));
        }
        let mut loader = ClientAssetLoader::new(assets);
        let mut textures = HashMap::new();
        for asset in &map.assets {
            let texture = loader.load_png(&asset.id, &asset.source_path)?;
            let image = loader
                .runtime()
                .resource(texture)
                .ok_or_else(|| format!("map asset {} was not registered", asset.id))?;
            let dimensions = [image.image.width(), image.image.height()];
            if dimensions != asset.image_size_px {
                return Err(format!(
                    "map asset {} is {:?}, canonical map expects {:?}",
                    asset.id, dimensions, asset.image_size_px
                ));
            }
            textures.insert(asset.id.clone(), (texture, dimensions));
        }
        let mut sprites = Vec::new();
        for layer in map.layers.into_iter().filter(|layer| layer.visible) {
            for authored in layer.sprites.into_iter().filter(|sprite| sprite.visible) {
                let &(texture, image_dimensions) =
                    textures.get(&authored.asset_id).ok_or_else(|| {
                        format!("map sprite references missing asset {}", authored.asset_id)
                    })?;
                let [x, y, width, height] = authored.source_rect_px;
                if width == 0
                    || height == 0
                    || x.saturating_add(width) > image_dimensions[0]
                    || y.saturating_add(height) > image_dimensions[1]
                {
                    return Err(format!(
                        "map asset {} has invalid source rectangle {:?}",
                        authored.asset_id, authored.source_rect_px
                    ));
                }
                if !authored
                    .position_world
                    .iter()
                    .all(|value| value.is_finite())
                    || !authored
                        .size_world
                        .iter()
                        .all(|value| value.is_finite() && *value > 0.0)
                {
                    return Err(format!(
                        "map asset {} has non-finite or non-positive normalized geometry",
                        authored.asset_id
                    ));
                }
                sprites.push(RuntimeSprite {
                    authored,
                    texture,
                    image_dimensions,
                    opacity: layer.opacity,
                });
            }
        }
        Ok(Self {
            map_id: MapId::DEV,
            canonical_bounds: map.world_bounds,
            sprites,
        })
    }

    pub(crate) fn active_for_map(&self, map_id: MapId) -> bool {
        self.map_id == map_id
    }

    /// Runtime gameplay geometry still owns bounds during W1.4. Center the
    /// canonical Tiled presentation inside those bounds without changing any
    /// authored relative positions or scale. W2 gameplay authoring can remove
    /// this compatibility offset once gameplay bounds come from Map Lab.
    pub(crate) fn quads_centered_in(&self, runtime_bounds: WorldBounds) -> Vec<DrawQuad> {
        let offset = presentation_center_offset(self.canonical_bounds, runtime_bounds);
        self.sprites
            .iter()
            .map(|sprite| {
                let uvs = sprite.uvs_for();
                let [w, h] = sprite.authored.size_world;
                let center = [
                    sprite.authored.position_world[0] + offset[0],
                    sprite.authored.position_world[1] + offset[1],
                ];
                let mut quad = DrawQuad::textured_sprite(
                    sprite.texture,
                    center,
                    [
                        [-w / 2.0, -h / 2.0],
                        [w / 2.0, -h / 2.0],
                        [w / 2.0, h / 2.0],
                        [-w / 2.0, h / 2.0],
                    ],
                    uvs,
                    0.0,
                );
                quad.color[3] = sprite.opacity * sprite.authored.opacity;
                quad
            })
            .collect()
    }
}

fn presentation_center_offset(
    canonical_bounds: [f32; 4],
    runtime_bounds: WorldBounds,
) -> [f32; 2] {
    let canonical_center = [
        (canonical_bounds[0] + canonical_bounds[2]) * 0.5,
        (canonical_bounds[1] + canonical_bounds[3]) * 0.5,
    ];
    let runtime_center = [
        (runtime_bounds.min_x + runtime_bounds.max_x) * 0.5,
        (runtime_bounds.min_y + runtime_bounds.max_y) * 0.5,
    ];
    [
        runtime_center[0] - canonical_center[0],
        runtime_center[1] - canonical_center[1],
    ]
}

impl RuntimeSprite {
    fn uvs_for(&self) -> [[f32; 2]; 4] {
        let [x, y, width, height] = self.authored.source_rect_px;
        let [image_width, image_height] = self.image_dimensions;
        let u0 = x as f32 / image_width as f32;
        let v0 = y as f32 / image_height as f32;
        let u1 = (x + width) as f32 / image_width as f32;
        let v1 = (y + height) as f32 / image_height as f32;
        let transform = sprite_uv_transform(
            self.authored.transform.flip_horizontal,
            self.authored.transform.flip_vertical,
            self.authored.transform.flip_diagonal,
        );
        transform.map(|[u, v]| [u0 + (u1 - u0) * u, v0 + (v1 - v0) * v])
    }
}

fn sprite_uv_transform(horizontal: bool, vertical: bool, diagonal: bool) -> [[f32; 2]; 4] {
    [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]].map(|mut uv| {
        if diagonal {
            uv.swap(0, 1);
        }
        if horizontal {
            uv[0] = 1.0 - uv[0];
        }
        if vertical {
            uv[1] = 1.0 - uv[1];
        }
        uv
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_visual_center_can_align_to_runtime_bounds_without_rescaling() {
        let offset = presentation_center_offset(
            [0.0, 0.0, 46.44, 10.8],
            WorldBounds {
                min_x: -24.0,
                max_x: 24.0,
                min_y: -8.0,
                max_y: 10.0,
            },
        );
        assert!((offset[0] + 23.22).abs() < 1e-4);
        assert!((offset[1] + 4.4).abs() < 1e-4);
    }

    #[test]
    fn tile_uv_transform_keeps_existing_flip_order() {
        let transformed = sprite_uv_transform(true, false, true);
        assert_eq!(transformed[0], [0.0, 0.0]);
        assert_eq!(transformed[2], [1.0, 1.0]);
    }
}
