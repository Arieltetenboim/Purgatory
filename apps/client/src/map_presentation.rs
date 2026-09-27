use std::collections::HashMap;

use purgatory_content::{
    ContentRegistry, MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION, MAP_PRESENTATION_SCHEMA_VERSION,
    MapEnvironmentAuthoring, MapPresentation, PresentationSprite, SkyGradient,
};
use purgatory_simulation::MapId;
use serde_json::from_slice;

use crate::asset_runtime::AssetRuntime;
use crate::assets::ClientAssetLoader;
use crate::renderer::{DrawQuad, SpriteTextureId};

const COMPILED_PRESENTATION: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/map.map1.presentation.json"));
const COMPILED_ENVIRONMENT: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/map.map1.environment.json"));

pub(crate) struct RuntimeMapPresentation {
    map_authored: String,
    world_bounds: [f32; 4],
    sky_gradient: Option<SkyGradient>,
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
        if map.map_authored != purgatory_common::MAP1_AUTHORED {
            return Err(format!(
                "compiled map presentation targets {}, expected {}",
                map.map_authored,
                purgatory_common::MAP1_AUTHORED
            ));
        }
        let environment: MapEnvironmentAuthoring = from_slice(COMPILED_ENVIRONMENT)
            .map_err(|error| format!("decode compiled map environment: {error}"))?;
        if environment.schema_version != MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION {
            return Err(format!(
                "compiled map environment schema {} is unsupported; expected {}",
                environment.schema_version, MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION
            ));
        }
        if environment.map_authored != map.map_authored {
            return Err(format!(
                "compiled map environment targets {}, expected {}",
                environment.map_authored, map.map_authored
            ));
        }
        let world_bounds = map.world_bounds;
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
            map_authored: map.map_authored,
            world_bounds,
            sky_gradient: environment.sky_gradient,
            sprites,
        })
    }

    pub(crate) fn active_for_map(&self, map_id: MapId, registry: &ContentRegistry) -> bool {
        registry
            .map_by_map_id(map_id)
            .is_some_and(|map| map.authored_id == self.map_authored)
    }

    pub(crate) fn quads(&self) -> Vec<DrawQuad> {
        let mut quads = self.sky_quads();
        quads.extend(
            self.sprites
                .iter()
                .map(|sprite| {
                    let uvs = sprite.uvs_for();
                    let [w, h] = sprite.authored.size_world;
                    let mut quad = DrawQuad::textured_sprite(
                        sprite.texture,
                        sprite.authored.position_world,
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
                .collect::<Vec<_>>(),
        );
        quads
    }

    fn sky_quads(&self) -> Vec<DrawQuad> {
        let Some(gradient) = self.sky_gradient else {
            return Vec::new();
        };
        const BANDS: usize = 16;
        let [min_x, min_y, max_x, max_y] = self.world_bounds;
        let width = max_x - min_x;
        let height = max_y - min_y;
        if width <= 0.0 || height <= 0.0 {
            return Vec::new();
        }
        (0..BANDS)
            .map(|index| {
                let t = (index as f32 + 0.5) / BANDS as f32;
                let color = lerp_rgba(gradient.bottom_rgba, gradient.top_rgba, t);
                let band_height = height / BANDS as f32;
                DrawQuad::rect(
                    [
                        (min_x + max_x) * 0.5,
                        min_y + (index as f32 + 0.5) * band_height,
                    ],
                    [width, band_height + 0.002],
                    color,
                )
            })
            .collect()
    }
}

fn lerp_rgba(bottom: [u8; 4], top: [u8; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    std::array::from_fn(|index| {
        (bottom[index] as f32 + (top[index] as f32 - bottom[index] as f32) * t) / 255.0
    })
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
    fn tile_uv_transform_keeps_existing_flip_order() {
        let transformed = sprite_uv_transform(true, false, true);
        assert_eq!(transformed[0], [0.0, 0.0]);
        assert_eq!(transformed[2], [1.0, 1.0]);
    }
}
