use purgatory_content::{MapPresentation, PresentationSprite};
use serde_json::from_slice;

use crate::asset_runtime::AssetRuntime;
use crate::assets::ClientAssetLoader;
use crate::renderer::{DrawQuad, SpriteTextureId};

const COMPILED_PRESENTATION: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/map.dev.footnote.presentation.json"
));

pub(crate) struct RuntimeMapPresentation {
    sprites: Vec<RuntimeSprite>,
}

struct RuntimeSprite {
    authored: PresentationSprite,
    texture: SpriteTextureId,
    image_dimensions: [u32; 2],
}

impl RuntimeMapPresentation {
    pub(crate) fn load(assets: &mut AssetRuntime) -> Result<Self, String> {
        let map: MapPresentation = from_slice(COMPILED_PRESENTATION)
            .map_err(|error| format!("decode compiled map presentation: {error}"))?;
        if map.map_authored != purgatory_common::MAP_FOOTNOTE_AUTHORED {
            return Err(format!(
                "compiled map presentation targets {}, expected {}",
                map.map_authored,
                purgatory_common::MAP_FOOTNOTE_AUTHORED
            ));
        }
        let mut loader = ClientAssetLoader::new(assets);
        let mut sprites = Vec::new();
        for authored in map.backgrounds.into_iter().chain(map.world_art) {
            let texture = loader.load_png(&authored.asset_key, &authored.asset_path)?;
            let image = loader
                .runtime()
                .resource(texture)
                .ok_or_else(|| format!("map asset {} was not registered", authored.asset_key))?;
            let [x, y, width, height] = authored.source_rect_px;
            if width == 0
                || height == 0
                || x.saturating_add(width) > image.image.width()
                || y.saturating_add(height) > image.image.height()
            {
                return Err(format!(
                    "map asset {} has invalid source rectangle {:?}",
                    authored.asset_key, authored.source_rect_px
                ));
            }
            if !authored.position.iter().all(|value| value.is_finite())
                || !authored
                    .size
                    .iter()
                    .all(|value| value.is_finite() && *value > 0.0)
            {
                return Err(format!(
                    "map asset {} has non-finite or non-positive normalized geometry",
                    authored.asset_key
                ));
            }
            sprites.push(RuntimeSprite {
                authored,
                texture,
                image_dimensions: [image.image.width(), image.image.height()],
            });
        }
        Ok(Self { sprites })
    }

    pub(crate) fn quads(&self) -> Vec<DrawQuad> {
        self.sprites
            .iter()
            .map(|sprite| {
                let uvs = sprite.uvs_for();
                let [w, h] = sprite.authored.size;
                DrawQuad::textured_sprite(
                    sprite.texture,
                    sprite.authored.position,
                    [
                        [-w / 2.0, -h / 2.0],
                        [w / 2.0, -h / 2.0],
                        [w / 2.0, h / 2.0],
                        [-w / 2.0, h / 2.0],
                    ],
                    uvs,
                    0.0,
                )
            })
            .collect()
    }
}

impl RuntimeSprite {
    fn uvs_for(&self) -> [[f32; 2]; 4] {
        let [x, y, width, height] = self.authored.source_rect_px;
        let [image_width, image_height] = self.image_dimensions;
        let u0 = x as f32 / image_width as f32;
        let v0 = y as f32 / image_height as f32;
        let u1 = (x + width) as f32 / image_width as f32;
        let v1 = (y + height) as f32 / image_height as f32;
        [[u0, v1], [u1, v1], [u1, v0], [u0, v0]]
    }
}
