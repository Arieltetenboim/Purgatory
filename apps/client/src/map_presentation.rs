use std::collections::HashMap;

use purgatory_common::{ContentId, ContentKind};
use purgatory_content::{
    CloudFieldAuthoring, CloudInstanceSpec, CloudStackPosition, ContentRegistry,
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

struct CompiledMapBytes {
    content_id: u32,
    presentation: &'static [u8],
    environment: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/compiled_map_catalog.rs"));

pub(crate) struct RuntimeMapPresentationCatalog {
    by_content: HashMap<ContentId, RuntimeMapPresentation>,
}

pub(crate) struct RuntimeMapPresentation {
    world_bounds: [f32; 4],
    pixels_per_world_unit: f32,
    sky_gradient: Option<SkyGradient>,
    parallax_layers: Vec<RuntimeParallaxLayer>,
    foreground_layers: Vec<RuntimeParallaxLayer>,
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

pub(crate) fn environment_layer_texture_key(
    content_id: ContentId,
    kind: &str,
    layer_id: &str,
) -> Result<String, String> {
    Ok(format!(
        "map.{}.environment.{kind}.{layer_id}",
        numeric_map_content(content_id)?
    ))
}

pub(crate) fn cloud_texture_key(
    content_id: ContentId,
    field_index: usize,
    asset_index: usize,
) -> Result<String, String> {
    Ok(format!(
        "map.{}.cloud.{field_index}.{asset_index}",
        numeric_map_content(content_id)?
    ))
}

fn numeric_map_content(content_id: ContentId) -> Result<u32, String> {
    match content_id.raw() {
        Some(raw) if content_id.kind() == Some(ContentKind::Map) => Ok(raw),
        _ => Err(format!(
            "map presentation requires a numeric map ContentId, got {content_id}"
        )),
    }
}

fn missing_presentation_error(
    content_id: ContentId,
    map_id: MapId,
    registry: &ContentRegistry,
) -> String {
    let label = registry.label(content_id).unwrap_or("unknown");
    format!(
        "registered map content {} ({label}) for MapId {} has no compiled presentation",
        numeric_map_content(content_id).unwrap_or(0),
        map_id.raw()
    )
}

fn load_runtime_environment_layer(
    loader: &mut ClientAssetLoader<'_>,
    content_id: ContentId,
    layer: ParallaxLayer,
    kind: &str,
) -> Result<RuntimeParallaxLayer, String> {
    if layer.asset_path.trim().is_empty() {
        return Err(format!("{kind} layer {} has empty asset path", layer.id));
    }
    if !layer.parallax.is_finite() || !(0.0..=1.0).contains(&layer.parallax) {
        return Err(format!(
            "{kind} layer {} has invalid parallax {}",
            layer.id, layer.parallax
        ));
    }
    if !layer.opacity.is_finite() || !(0.0..=1.0).contains(&layer.opacity) {
        return Err(format!(
            "{kind} layer {} has invalid opacity {}",
            layer.id, layer.opacity
        ));
    }
    if !layer
        .motion_world_per_second
        .iter()
        .all(|value| value.is_finite())
    {
        return Err(format!("{kind} layer {} has non-finite motion", layer.id));
    }
    let texture_id = environment_layer_texture_key(content_id, kind, &layer.id)?;
    let texture = loader.load_png(&texture_id, &layer.asset_path)?;
    let image = loader
        .runtime()
        .resource(texture)
        .ok_or_else(|| format!("{kind} asset {} was not registered", layer.id))?;
    Ok(RuntimeParallaxLayer {
        authored: layer,
        texture,
        image_dimensions: [image.image.width(), image.image.height()],
    })
}

impl RuntimeMapPresentationCatalog {
    pub(crate) fn load(
        assets: &mut AssetRuntime,
        registry: &ContentRegistry,
    ) -> Result<Self, String> {
        let mut by_content = HashMap::new();
        for compiled in COMPILED_MAPS {
            let content_id = ContentId::from_raw(compiled.content_id);
            let raw = numeric_map_content(content_id)?;
            let map: MapPresentation = from_slice(compiled.presentation)
                .map_err(|error| format!("decode compiled map presentation {raw}: {error}"))?;
            if map.schema_version != MAP_PRESENTATION_SCHEMA_VERSION {
                return Err(format!(
                    "compiled map presentation {raw} schema {} is unsupported; expected {}",
                    map.schema_version, MAP_PRESENTATION_SCHEMA_VERSION
                ));
            }
            let label = registry.label(content_id).ok_or_else(|| {
                format!("compiled map presentation {raw} is not a registered map")
            })?;
            if map.map_authored != label {
                return Err(format!(
                    "compiled map presentation {raw} targets {}, registered map is {label}",
                    map.map_authored
                ));
            }

            let environment: MapEnvironmentPresentation = from_slice(compiled.environment)
                .map_err(|error| format!("decode compiled map environment {raw}: {error}"))?;
            if environment.schema_version != MAP_ENVIRONMENT_PRESENTATION_SCHEMA_VERSION {
                return Err(format!(
                    "compiled map environment {raw} schema {} is unsupported; expected {}",
                    environment.schema_version, MAP_ENVIRONMENT_PRESENTATION_SCHEMA_VERSION
                ));
            }
            if environment.map_authored != map.map_authored {
                return Err(format!(
                    "compiled map environment {raw} targets {}, expected {}",
                    environment.map_authored, map.map_authored
                ));
            }

            let presentation = load_runtime_presentation(assets, content_id, map, environment)?;
            if by_content.insert(content_id, presentation).is_some() {
                return Err(format!("duplicate compiled map presentation {raw}"));
            }
        }

        for map in registry.iter_maps() {
            if !by_content.contains_key(&map.content_id) {
                return Err(format!(
                    "registered map content {} ({}) has no compiled presentation",
                    map.content_id.raw().unwrap_or(0),
                    map.authored_id
                ));
            }
        }
        Ok(Self { by_content })
    }

    /// `Ok(None)` is an unregistered synthetic/debug map. A registered map with no
    /// compiled presentation is an error.
    pub(crate) fn presentation_mut(
        &mut self,
        map_id: MapId,
        registry: &ContentRegistry,
    ) -> Result<Option<&mut RuntimeMapPresentation>, String> {
        let Some(content_id) = registry.map_content_id(map_id) else {
            return Ok(None);
        };
        match self.by_content.get_mut(&content_id) {
            Some(presentation) => Ok(Some(presentation)),
            None => Err(missing_presentation_error(content_id, map_id, registry)),
        }
    }
}

fn load_runtime_presentation(
    assets: &mut AssetRuntime,
    content_id: ContentId,
    map: MapPresentation,
    environment: MapEnvironmentPresentation,
) -> Result<RuntimeMapPresentation, String> {
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
        parallax_layers.push(load_runtime_environment_layer(
            &mut loader,
            content_id,
            layer,
            "background",
        )?);
    }

    let mut foreground_layers = Vec::new();
    for layer in environment.foreground_layers {
        foreground_layers.push(load_runtime_environment_layer(
            &mut loader,
            content_id,
            layer,
            "foreground",
        )?);
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
            let texture_id = cloud_texture_key(content_id, field_index, asset_index)?;
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

    Ok(RuntimeMapPresentation {
        world_bounds,
        pixels_per_world_unit,
        sky_gradient: environment.sky_gradient,
        parallax_layers,
        foreground_layers,
        cloud_fields,
        sprites,
        environment_time_seconds: 0.0,
        visual_seed: local_visual_seed() ^ u64::from(numeric_map_content(content_id)?),
    })
}

impl RuntimeMapPresentation {
    pub(crate) fn quads(&mut self, camera: &Camera, frame_dt: f32) -> Vec<DrawQuad> {
        if frame_dt.is_finite() && frame_dt > 0.0 {
            self.environment_time_seconds += f64::from(frame_dt);
        }
        let mut quads = self.sky_quads(camera);
        let world_bounds = self.world_bounds;
        let pixels_per_world_unit = self.pixels_per_world_unit;
        let environment_time_seconds = self.environment_time_seconds;
        let visual_seed = self.visual_seed;

        for field in self
            .cloud_fields
            .iter_mut()
            .filter(|field| field.authored.stack_position == CloudStackPosition::BeforeAll)
        {
            quads.extend(cloud_field_quads(
                field,
                camera,
                world_bounds,
                pixels_per_world_unit,
                environment_time_seconds,
                visual_seed,
            ));
        }

        for depth in ParallaxDepth::ALL {
            for layer in self
                .parallax_layers
                .iter()
                .filter(|layer| layer.authored.depth == depth)
            {
                quads.extend(self.parallax_quads(layer, camera));
            }
            let stack_position = CloudStackPosition::for_depth(depth);
            for field in self
                .cloud_fields
                .iter_mut()
                .filter(|field| field.authored.stack_position == stack_position)
            {
                quads.extend(cloud_field_quads(
                    field,
                    camera,
                    world_bounds,
                    pixels_per_world_unit,
                    environment_time_seconds,
                    visual_seed,
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

    pub(crate) fn foreground_quads(&self, camera: &Camera) -> Vec<DrawQuad> {
        self.foreground_layers
            .iter()
            .flat_map(|layer| self.parallax_quads(layer, camera))
            .collect()
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
                        camera.position[1] - height * 0.5 + (index as f32 + 0.5) * band_height,
                    ],
                    [width, band_height + 0.002],
                    color,
                )
            })
            .collect()
    }

    fn parallax_quads(&self, layer: &RuntimeParallaxLayer, camera: &Camera) -> Vec<DrawQuad> {
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
        let coverage =
            parallax_coverage_size(map_size, [camera.viewport_width, camera.viewport_height], p);
        let size = fill_size(layer.authored.fill_mode, natural_size, coverage);
        let animated_offset = layer
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
            let size = [
                variant.image_dimensions[0] as f32 / ppu * cloud.scale,
                variant.image_dimensions[1] as f32 / ppu * cloud.scale,
            ];
            let x = wrap_cloud_center(
                cloud.x_unit,
                cloud.speed_world_per_second,
                elapsed_seconds,
                coverage[0],
                size[0],
            );
            let y = (cloud.height_unit - 0.5) * viewport[1];
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

fn wrap_cloud_center(
    x_unit: f32,
    speed_world_per_second: f32,
    elapsed_seconds: f64,
    coverage_width: f32,
    sprite_width: f32,
) -> f32 {
    let travel_width = coverage_width + sprite_width.max(0.0);
    if !travel_width.is_finite() || travel_width <= f32::EPSILON {
        return 0.0;
    }
    let value = x_unit * travel_width + speed_world_per_second * elapsed_seconds as f32;
    value.rem_euclid(travel_width) - travel_width * 0.5
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

fn fill_size(mode: ParallaxFillMode, natural: [f32; 2], coverage: [f32; 2]) -> [f32; 2] {
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
    fn cloud_wrap_keeps_sprite_fully_outside_before_reentry() {
        let coverage = 10.0;
        let sprite = 4.0;
        assert!((wrap_cloud_center(0.0, 0.0, 0.0, coverage, sprite) + 7.0).abs() < 1e-5);
        let almost_wrapped = wrap_cloud_center(0.0, 1.0, 13.999, coverage, sprite);
        assert!(almost_wrapped > 6.9);
        let wrapped = wrap_cloud_center(0.0, 1.0, 14.0, coverage, sprite);
        assert!((wrapped + 7.0).abs() < 1e-5);
    }

    #[test]
    fn repeat_radius_is_bounded() {
        assert_eq!(repeat_radius(false, 20.0, 2.0), 0);
        assert!((1..=8).contains(&repeat_radius(true, 20.0, 2.0)));
        assert_eq!(repeat_radius(true, 10_000.0, 0.1), 8);
    }

    #[test]
    fn compiled_catalog_matches_registered_maps_by_content_id() {
        let registry = purgatory_content::load_registry(
            &purgatory_content::default_content_root(),
            purgatory_content::LoadMode::Shared,
        )
        .expect("shared content");
        let mut registered: Vec<u32> = registry
            .iter_maps()
            .filter_map(|map| map.content_id.raw())
            .collect();
        registered.sort_unstable();
        let mut compiled: Vec<u32> = COMPILED_MAPS.iter().map(|entry| entry.content_id).collect();
        compiled.sort_unstable();
        assert_eq!(compiled, registered);
        assert!(compiled.len() >= 2);
    }

    #[test]
    fn map_id_resolves_to_that_maps_compiled_presentation() {
        let registry = purgatory_content::load_registry(
            &purgatory_content::default_content_root(),
            purgatory_content::LoadMode::Shared,
        )
        .expect("shared content");
        for entry in COMPILED_MAPS {
            let content_id = ContentId::from_raw(entry.content_id);
            let map_id = registry.map_id(content_id).expect("registered map id");
            assert_eq!(registry.map_content_id(map_id), Some(content_id));
            let (presentation, environment) = decode_compiled(entry);
            let label = registry.label(content_id).expect("label");
            assert_eq!(presentation.map_authored, label);
            assert_eq!(environment.map_authored, label);
        }
    }

    #[test]
    fn switching_active_map_selects_that_maps_environment() {
        let registry = purgatory_content::load_registry(
            &purgatory_content::default_content_root(),
            purgatory_content::LoadMode::Shared,
        )
        .expect("shared content");
        let map_a = registry.map_id(purgatory_common::MAP1).expect("MAP1");
        let map_b = registry.map_id(purgatory_common::MAP2).expect("MAP2");
        let mut catalog = catalog_with([
            (
                purgatory_common::MAP1,
                presentation_for_test(Some(sky([1, 2, 3, 255], [4, 5, 6, 255])), "env-a"),
            ),
            (
                purgatory_common::MAP2,
                presentation_for_test(Some(sky([9, 8, 7, 255], [6, 5, 4, 255])), "env-b"),
            ),
        ]);

        let (gradient_a, cloud_a) = selected(&mut catalog, map_a, &registry);
        let (gradient_b, cloud_b) = selected(&mut catalog, map_b, &registry);
        assert_eq!(gradient_a, Some(sky([1, 2, 3, 255], [4, 5, 6, 255])));
        assert_eq!(gradient_b, Some(sky([9, 8, 7, 255], [6, 5, 4, 255])));
        assert_ne!(gradient_a, gradient_b);
        assert_eq!(cloud_a, "env-a");
        assert_eq!(cloud_b, "env-b");
    }

    #[test]
    fn compiled_map_environments_stay_with_their_content_id() {
        let map1 = decode_compiled(compiled_entry(purgatory_common::MAP1.raw().unwrap()));
        let map2 = decode_compiled(compiled_entry(purgatory_common::MAP2.raw().unwrap()));
        assert_eq!(map1.0.map_authored, purgatory_common::MAP1_AUTHORED);
        assert_eq!(map2.0.map_authored, purgatory_common::MAP2_AUTHORED);
        assert_eq!(map1.1.map_authored, purgatory_common::MAP1_AUTHORED);
        assert_eq!(map2.1.map_authored, purgatory_common::MAP2_AUTHORED);
        assert_ne!(map1.0, map2.0);
        assert_ne!(map1.1, map2.1);
        assert_eq!(
            map1.1.sky_gradient.map(|gradient| gradient.top_rgba),
            Some([104, 155, 214, 255])
        );
        assert_eq!(
            map1.1.sky_gradient.map(|gradient| gradient.bottom_rgba),
            Some([232, 214, 188, 255])
        );
    }

    #[test]
    fn identical_environment_layer_ids_do_not_alias_textures() {
        let mut runtime = AssetRuntime::new();
        let key_a =
            environment_layer_texture_key(purgatory_common::MAP1, "background", "layer.001")
                .unwrap();
        let key_b =
            environment_layer_texture_key(purgatory_common::MAP2, "background", "layer.001")
                .unwrap();
        assert_ne!(key_a, key_b);
        let texture_a = runtime
            .register_png(&key_a, &png_bytes([255, 0, 0, 255]))
            .unwrap();
        let texture_b = runtime
            .register_png(&key_b, &png_bytes([0, 0, 255, 255]))
            .unwrap();
        assert_ne!(texture_a, texture_b);
        assert_ne!(
            runtime.resource(texture_a).unwrap().image.get_pixel(0, 0),
            runtime.resource(texture_b).unwrap().image.get_pixel(0, 0)
        );

        let cloud_a = cloud_texture_key(purgatory_common::MAP1, 0, 0).unwrap();
        let cloud_b = cloud_texture_key(purgatory_common::MAP2, 0, 0).unwrap();
        assert_ne!(cloud_a, cloud_b);
        assert_ne!(
            runtime
                .register_png(&cloud_a, &png_bytes([1, 1, 1, 255]))
                .unwrap(),
            runtime
                .register_png(&cloud_b, &png_bytes([2, 2, 2, 255]))
                .unwrap()
        );
    }

    #[test]
    fn unregistered_map_is_synthetic_and_missing_presentation_fails() {
        let registry = purgatory_content::load_registry(
            &purgatory_content::default_content_root(),
            purgatory_content::LoadMode::Shared,
        )
        .expect("shared content");
        let mut catalog = catalog_with([(
            purgatory_common::MAP1,
            presentation_for_test(None, "only-a"),
        )]);
        match catalog.presentation_mut(MapId::from_raw(424_242), &registry) {
            Ok(None) => {}
            Ok(Some(_)) => panic!("unregistered map must stay synthetic"),
            Err(error) => panic!("unregistered map must stay synthetic, got {error}"),
        }
        let map2 = registry.map_id(purgatory_common::MAP2).expect("MAP2");
        let error = match catalog.presentation_mut(map2, &registry) {
            Err(error) => error,
            Ok(_) => panic!("registered map without a compiled presentation must fail"),
        };
        assert!(error.contains("50002"), "{error}");
        assert!(error.contains("no compiled presentation"), "{error}");
    }

    #[test]
    fn compiled_runtime_bytes_are_not_tiled_documents() {
        for entry in COMPILED_MAPS {
            for bytes in [entry.presentation, entry.environment] {
                let text = std::str::from_utf8(bytes).unwrap();
                assert!(!text.contains("firstgid"), "{}", entry.content_id);
                assert!(!text.contains(".tmx"), "{}", entry.content_id);
                assert!(!text.contains(".tsx"), "{}", entry.content_id);
            }
        }
    }

    fn compiled_entry(content_id: u32) -> &'static CompiledMapBytes {
        COMPILED_MAPS
            .iter()
            .find(|entry| entry.content_id == content_id)
            .unwrap_or_else(|| panic!("compiled presentation {content_id}"))
    }

    fn decode_compiled(entry: &CompiledMapBytes) -> (MapPresentation, MapEnvironmentPresentation) {
        (
            from_slice(entry.presentation).expect("presentation json"),
            from_slice(entry.environment).expect("environment json"),
        )
    }

    fn sky(top: [u8; 4], bottom: [u8; 4]) -> SkyGradient {
        SkyGradient {
            top_rgba: top,
            bottom_rgba: bottom,
        }
    }

    fn presentation_for_test(
        gradient: Option<SkyGradient>,
        cloud_id: &str,
    ) -> RuntimeMapPresentation {
        RuntimeMapPresentation {
            world_bounds: [0.0, 0.0, 10.0, 10.0],
            pixels_per_world_unit: 100.0,
            sky_gradient: gradient,
            parallax_layers: Vec::new(),
            foreground_layers: Vec::new(),
            cloud_fields: vec![RuntimeCloudField {
                authored: CloudFieldAuthoring {
                    id: cloud_id.to_owned(),
                    asset_folder: "assets/skys/cloud_far".to_owned(),
                    depth: ParallaxDepth::Far,
                    stack_position: CloudStackPosition::AfterFar,
                    parallax: 0.2,
                    density: 0.5,
                    scale_range: [0.5, 1.0],
                    speed_range: [0.1, 0.2],
                    height_range: [0.5, 0.9],
                    opacity_range: [0.5, 1.0],
                },
                variants: Vec::new(),
                instances: Vec::new(),
            }],
            sprites: Vec::new(),
            environment_time_seconds: 0.0,
            visual_seed: 1,
        }
    }

    fn catalog_with(
        entries: impl IntoIterator<Item = (ContentId, RuntimeMapPresentation)>,
    ) -> RuntimeMapPresentationCatalog {
        RuntimeMapPresentationCatalog {
            by_content: entries.into_iter().collect(),
        }
    }

    fn selected(
        catalog: &mut RuntimeMapPresentationCatalog,
        map_id: MapId,
        registry: &ContentRegistry,
    ) -> (Option<SkyGradient>, String) {
        let presentation = match catalog.presentation_mut(map_id, registry) {
            Ok(Some(presentation)) => presentation,
            Ok(None) => panic!("registered map resolved as synthetic"),
            Err(error) => panic!("registered map failed to resolve: {error}"),
        };
        (
            presentation.sky_gradient,
            presentation.cloud_fields[0].authored.id.clone(),
        )
    }

    fn png_bytes(pixel: [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba(pixel));
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }
}
