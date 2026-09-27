use std::collections::HashMap;

use purgatory_content::{
    CloudFieldAuthoring, CloudInstanceSpec, ContentRegistry,
    MAP_ENVIRONMENT_PRESENTATION_SCHEMA_VERSION, MAP_PRESENTATION_SCHEMA_VERSION,
    MapEnvironmentPresentation, MapPresentation, ParallaxDepth, ParallaxFillMode, ParallaxLayer,
    PresentationSprite, SkyGradient, cloud_field_seed, cloud_instance_count, cloud_instance_specs,
    validate_cloud_field,
};
use purgatory_simulation::MapId;
use serde_json::from_slice;

use crate::asset_runtime::AssetRuntime;
use crate::assets::ClientAssetLoader;
use crate::renderer::{Camera, DrawQuad, SpriteTextureId};

const COMPILED_PRESENTATION: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/map.map1.presentation.json"));
const COMPILED_ENVIRONMENT: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/map.map1.environment.json"));

pub(crate) struct RuntimeMapPresentation {
    map_authored: String,
    world_bounds: [f32; 4],
    pixels_per_world_unit: f32,
    sky_gradient: Option<SkyGradient>,
    parallax_layers: Vec<RuntimeParallaxLayer>,
    cloud_fields: Vec<RuntimeCloudField>,
    sprites: Vec<RuntimeSprite>,
    environment_time_seconds: f64,
    visual_seed: u64,
}

struct RuntimeSprite {
    authored: PresentationSprite,
    texture: SpriteTextureId,
    image_dimensions: [u32; 2],
    opacity: f32,
}

struct RuntimeParallaxLayer {
    authored: ParallaxLayer,
    texture: SpriteTextureId,
    image_dimensions: [u32; 2],
}

struct RuntimeCloudVariant {
    texture: SpriteTextureId,
    image_dimensions: [u32; 2],
}

struct RuntimeCloudField {
    authored: CloudFieldAuthoring,
    variants: Vec<RuntimeCloudVariant>,
    instances: Vec<CloudInstanceSpec>,
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

        let environment: MapEnvironmentPresentation = from_slice(COMPILED_ENVIRONMENT)
            .map_err(|error| format!("decode compiled map environment: {error}"))?;
        if environment.schema_version != MAP_ENVIRONMENT_PRESENTATION_SCHEMA_VERSION {
            return Err(format!(
                "compiled map environment schema {} is unsupported; expected {}",
                environment.schema_version, MAP_ENVIRONMENT_PRESENTATION_SCHEMA_VERSION
            ));
        }
        if environment.map_authored != map.map_authored {
            return Err(format!(
                "compiled map environment targets {}, expected {}",
                environment.map_authored, map.map_authored
            ));
        }

        let world_bounds = map.world_bounds;
        let pixels_per_world_unit = map.pixels_per_world_unit;
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

        let mut parallax_layers = Vec::new();
        for layer in environment.parallax_layers {
            if layer.asset_path.trim().is_empty() {
                return Err(format!("environment layer {} has empty asset path", layer.id));
            }
            if !layer.parallax.is_finite() || !(0.0..=1.0).contains(&layer.parallax) {
                return Err(format!(
                    "environment layer {} has invalid parallax {}",
                    layer.id, layer.parallax
                ));
            }
            if !layer.opacity.is_finite() || !(0.0..=1.0).contains(&layer.opacity) {
                return Err(format!(
                    "environment layer {} has invalid opacity {}",
                    layer.id, layer.opacity
                ));
            }
            if !layer
                .motion_world_per_second
                .iter()
                .all(|value| value.is_finite())
            {
                return Err(format!(
                    "environment layer {} has non-finite motion",
                    layer.id
                ));
            }
            let texture_id = format!("map.environment.{}", layer.id);
            let texture = loader.load_png(&texture_id, &layer.asset_path)?;
            let image = loader
                .runtime()
                .resource(texture)
                .ok_or_else(|| format!("environment asset {} was not registered", layer.id))?;
            parallax_layers.push(RuntimeParallaxLayer {
                authored: layer,
                texture,
                image_dimensions: [image.image.width(), image.image.height()],
            });
        }

        let mut cloud_fields = Vec::new();
        for (field_index, field) in environment.cloud_fields.into_iter().enumerate() {
            validate_cloud_field(&field.authored)
                .map_err(|error| format!("compiled cloud field invalid: {error}"))?;
            if field.asset_paths.is_empty() {
                return Err(format!(
                    "compiled cloud field {} has no resolved PNG assets",
                    field.authored.id
                ));
            }
            let mut variants = Vec::with_capacity(field.asset_paths.len());
            for (asset_index, asset_path) in field.asset_paths.iter().enumerate() {
                let texture_id = format!("map.cloud.{field_index}.{asset_index}");
                let texture = loader.load_png(&texture_id, asset_path)?;
                let image = loader.runtime().resource(texture).ok_or_else(|| {
                    format!(
                        "cloud asset {}:{} was not registered",
                        field.authored.id, asset_index
                    )
                })?;
                variants.push(RuntimeCloudVariant {
                    texture,
                    image_dimensions: [image.image.width(), image.image.height()],
                });
            }
            cloud_fields.push(RuntimeCloudField {
                authored: field.authored,
                variants,
                instances: Vec::new(),
            });
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
            pixels_per_world_unit,
            sky_gradient: environment.sky_gradient,
            parallax_layers,
            cloud_fields,
            sprites,
            environment_time_seconds: 0.0,
            visual_seed: local_visual_seed(),
        })
    }

    pub(crate) fn active_for_map(&self, map_id: MapId, registry: &ContentRegistry) -> bool {
        registry
            .map_by_map_id(map_id)
            .is_some_and(|map| map.authored_id == self.map_authored)
    }

    pub(crate) fn quads(&mut self, camera: &Camera, frame_dt: f32) -> Vec<DrawQuad> {
        if frame_dt.is_finite() && frame_dt > 0.0 {
            self.environment_time_seconds += f64::from(frame_dt);
        }
        let mut quads = self.sky_quads(camera);
        for depth in ParallaxDepth::ALL {
            for layer in self
                .parallax_layers
                .iter()
                .filter(|layer| layer.authored.depth == depth)
            {
                quads.extend(self.parallax_quads(layer, camera));
            }
            for field in self
                .cloud_fields
                .iter_mut()
                .filter(|field| field.authored.depth == depth)
            {
                quads.extend(cloud_field_quads(
                    field,
                    camera,
                    self.world_bounds,
                    self.pixels_per_world_unit,
                    self.environment_time_seconds,
                    self.visual_seed,
                ));
            }
        }
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

    fn sky_quads(&self, camera: &Camera) -> Vec<DrawQuad> {
        let Some(gradient) = self.sky_gradient else {
            return Vec::new();
        };
        const BANDS: usize = 16;
        let width = camera.viewport_width;
        let height = camera.viewport_height;
        (0..BANDS)
            .map(|index| {
                let t = (index as f32 + 0.5) / BANDS as f32;
                let color = lerp_rgba(gradient.bottom_rgba, gradient.top_rgba, t);
                let band_height = height / BANDS as f32;
                DrawQuad::rect(
                    [
                        camera.position[0],
                        camera.position[1] - height * 0.5
                            + (index as f32 + 0.5) * band_height,
                    ],
                    [width, band_height + 0.002],
                    color,
                )
            })
            .collect()
    }

    fn parallax_quads(
        &self,
        layer: &RuntimeParallaxLayer,
        camera: &Camera,
    ) -> Vec<DrawQuad> {
        let ppu = self.pixels_per_world_unit.max(f32::EPSILON);
        let natural_size = [
            layer.image_dimensions[0] as f32 / ppu,
            layer.image_dimensions[1] as f32 / ppu,
        ];
        if natural_size[0] <= 0.0 || natural_size[1] <= 0.0 {
            return Vec::new();
        }

        let [min_x, min_y, max_x, max_y] = self.world_bounds;
        let map_size = [max_x - min_x, max_y - min_y];
        let map_center = [(min_x + max_x) * 0.5, (min_y + max_y) * 0.5];
        let p = layer.authored.parallax;
        let coverage = parallax_coverage_size(
            map_size,
            [camera.viewport_width, camera.viewport_height],
            p,
        );
        let size = fill_size(layer.authored.fill_mode, natural_size, coverage);
        let animated_offset =
            layer
                .authored
                .animated_offset_world(self.environment_time_seconds, size);
        let base = [
            camera.position[0] * (1.0 - p) + map_center[0] * p + animated_offset[0],
            camera.position[1] * (1.0 - p) + map_center[1] * p + animated_offset[1],
        ];

        let repeat = layer.authored.fill_mode == ParallaxFillMode::Repeat;
        let x_radius = repeat_radius(
            repeat && layer.authored.repeat_x,
            camera.viewport_width,
            size[0],
        );
        let y_radius = repeat_radius(
            repeat && layer.authored.repeat_y,
            camera.viewport_height,
            size[1],
        );
        let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        let mut quads = Vec::new();
        for y in -y_radius..=y_radius {
            for x in -x_radius..=x_radius {
                if quads.len() >= 64 {
                    return quads;
                }
                let center = [base[0] + x as f32 * size[0], base[1] + y as f32 * size[1]];
                let mut quad = DrawQuad::textured_sprite(
                    layer.texture,
                    center,
                    [
                        [-size[0] * 0.5, -size[1] * 0.5],
                        [size[0] * 0.5, -size[1] * 0.5],
                        [size[0] * 0.5, size[1] * 0.5],
                        [-size[0] * 0.5, size[1] * 0.5],
                    ],
                    uvs,
                    0.0,
                );
                quad.color[3] = layer.authored.opacity;
                quads.push(quad);
            }
        }
        quads
    }
}

fn cloud_field_quads(
    field: &mut RuntimeCloudField,
    camera: &Camera,
    world_bounds: [f32; 4],
    pixels_per_world_unit: f32,
    elapsed_seconds: f64,
    visual_seed: u64,
) -> Vec<DrawQuad> {
    let [min_x, min_y, max_x, max_y] = world_bounds;
    let map_size = [max_x - min_x, max_y - min_y];
    let map_center = [(min_x + max_x) * 0.5, (min_y + max_y) * 0.5];
    let p = field.authored.parallax.clamp(0.0, 1.0);
    let viewport = [camera.viewport_width, camera.viewport_height];
    let coverage = parallax_coverage_size(map_size, viewport, p);
    let count = cloud_instance_count(field.authored.density, coverage[0], viewport[0]);
    if field.instances.len() != count {
        field.instances = cloud_instance_specs(
            &field.authored,
            field.variants.len(),
            cloud_field_seed(&field.authored.id, visual_seed),
            count,
        );
    }

    let base = [
        camera.position[0] * (1.0 - p) + map_center[0] * p,
        camera.position[1] * (1.0 - p) + map_center[1] * p,
    ];
    let ppu = pixels_per_world_unit.max(f32::EPSILON);
    let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    field
        .instances
        .iter()
        .filter_map(|cloud| {
            let variant = field.variants.get(cloud.asset_index)?;
            let x = wrap_centered(
                cloud.x_unit * coverage[0]
                    + cloud.speed_world_per_second * elapsed_seconds as f32,
                coverage[0],
            );
            let y = (cloud.height_unit - 0.5) * viewport[1];
            let size = [
                variant.image_dimensions[0] as f32 / ppu * cloud.scale,
                variant.image_dimensions[1] as f32 / ppu * cloud.scale,
            ];
            let center = [base[0] + x, base[1] + y];
            let mut quad = DrawQuad::textured_sprite(
                variant.texture,
                center,
                [
                    [-size[0] * 0.5, -size[1] * 0.5],
                    [size[0] * 0.5, -size[1] * 0.5],
                    [size[0] * 0.5, size[1] * 0.5],
                    [-size[0] * 0.5, size[1] * 0.5],
                ],
                uvs,
                0.0,
            );
            quad.color[3] = cloud.opacity;
            Some(quad)
        })
        .collect()
}

fn wrap_centered(value: f32, period: f32) -> f32 {
    if !period.is_finite() || period <= f32::EPSILON {
        return 0.0;
    }
    value.rem_euclid(period) - period * 0.5
}

fn local_visual_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    (nanos as u64) ^ ((nanos >> 64) as u64) ^ u64::from(std::process::id())
}

fn parallax_coverage_size(map_size: [f32; 2], viewport: [f32; 2], parallax: f32) -> [f32; 2] {
    let p = parallax.clamp(0.0, 1.0);
    [
        viewport[0] + p * (map_size[0] - viewport[0]).max(0.0),
        viewport[1] + p * (map_size[1] - viewport[1]).max(0.0),
    ]
}

fn fill_size(
    mode: ParallaxFillMode,
    natural: [f32; 2],
    coverage: [f32; 2],
) -> [f32; 2] {
    match mode {
        ParallaxFillMode::Natural | ParallaxFillMode::Repeat => natural,
        ParallaxFillMode::Stretch => coverage,
        ParallaxFillMode::Fit | ParallaxFillMode::Cover => {
            let sx = coverage[0] / natural[0];
            let sy = coverage[1] / natural[1];
            let scale = if mode == ParallaxFillMode::Fit {
                sx.min(sy)
            } else {
                sx.max(sy)
            };
            [natural[0] * scale, natural[1] * scale]
        }
    }
}

fn repeat_radius(repeat: bool, viewport: f32, tile: f32) -> i32 {
    if !repeat {
        return 0;
    }
    ((viewport / tile).ceil() as i32 / 2 + 2).clamp(1, 8)
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

    #[test]
    fn cover_preserves_aspect_and_fills_coverage() {
        let size = fill_size(ParallaxFillMode::Cover, [20.0, 10.0], [23.0, 13.0]);
        assert!((size[0] - 26.0).abs() < 1e-5);
        assert!((size[1] - 13.0).abs() < 1e-5);
    }

    #[test]
    fn stretch_matches_parallax_coverage_exactly() {
        let coverage = parallax_coverage_size([46.0, 20.0], [23.0, 13.0], 0.5);
        assert_eq!(coverage, [34.5, 16.5]);
        assert_eq!(
            fill_size(ParallaxFillMode::Stretch, [4.0, 4.0], coverage),
            coverage
        );
    }

    #[test]
    fn cloud_wrap_stays_inside_centered_period() {
        assert!((wrap_centered(0.0, 10.0) + 5.0).abs() < 1e-5);
        assert!((wrap_centered(14.0, 10.0) + 1.0).abs() < 1e-5);
        assert!((wrap_centered(-1.0, 10.0) - 4.0).abs() < 1e-5);
    }

    #[test]
    fn repeat_radius_is_bounded() {
        assert_eq!(repeat_radius(false, 20.0, 2.0), 0);
        assert!((1..=8).contains(&repeat_radius(true, 20.0, 2.0)));
        assert_eq!(repeat_radius(true, 10_000.0, 0.1), 8);
    }
}
