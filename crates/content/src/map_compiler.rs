//! Tiled authoring source -> versioned PURGATORY visual-map artifact.
//!
//! Tiled is used only at this authoring/compiler boundary. The canonical output
//! contains resolved PURGATORY concepts: no GIDs, firstgids, Tiled numeric IDs,
//! object anchors, or TMX/TSX path semantics.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tiled::{
    DrawOrder, FillMode, Layer, LayerType, ObjectAlignment, ObjectShape, Orientation, RenderOrder,
    TileLayer, TileRenderSize, Tileset,
};

use crate::error::{ContentError, ValidationIssue};
use crate::map_presentation::{
    MAP_PRESENTATION_SCHEMA_VERSION, MapPresentation, PresentationAsset, PresentationLayer,
    PresentationLayerKind, PresentationSprite, TileTransform,
};

pub const MAP_AUTHORING_SCHEMA_VERSION: u32 = 2;

/// Canonical Tiled map grid used by the production sources
/// `Graphic/assets/maps/50001.tmx` and `50002.tmx`.
///
/// New Map emits this size. A later TMX with a different tile size or
/// orientation is a validation error. V1 does not migrate either one.
pub const CANONICAL_MAP_TILE_PX: u32 = 20;

/// Every authored map must contain at least one full gameplay camera viewport.
pub const MIN_MAP_HEIGHT_WU: f32 = purgatory_simulation::FOOTNOTE_TEST_VIEWPORT_HEIGHT;
pub const MIN_MAP_WIDTH_WU: f32 = MIN_MAP_HEIGHT_WU * purgatory_simulation::AOI_VIEWPORT_ASPECT;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapAuthoringSource {
    pub schema_version: u32,
    pub content_id: u32,
    pub id: String,
    pub visual_source: String,
    pub pixels_per_world_unit: f32,
}

pub fn load_map_authoring(path: &Path) -> Result<MapAuthoringSource, ContentError> {
    let bytes = fs::read(path).map_err(|error| ContentError::from_io(path, &error))?;
    let source: MapAuthoringSource = serde_json::from_slice(&bytes)
        .map_err(|error| issue(path, "-", "json", error.to_string()))?;
    validate_authoring(path, &source)?;
    Ok(source)
}

pub fn compile_tiled_map(path: &Path) -> Result<MapPresentation, ContentError> {
    let source = load_map_authoring(path)?;
    compile_tiled_map_with_ppu(path, &source, source.pixels_per_world_unit)
}

/// Map Lab calibration entry point. This does not mutate the sidecar.
pub fn compile_tiled_map_with_ppu(
    sidecar_path: &Path,
    source: &MapAuthoringSource,
    pixels_per_world_unit: f32,
) -> Result<MapPresentation, ContentError> {
    validate_ppu(sidecar_path, pixels_per_world_unit)?;
    let tmx_path = sidecar_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(&source.visual_source);
    let mut loader = tiled::Loader::new();
    let map = loader
        .load_tmx_map(&tmx_path)
        .map_err(|error| issue(&tmx_path, "-", "tmx", error.to_string()))?;

    if map.orientation != Orientation::Orthogonal {
        return Err(issue(
            &tmx_path,
            "-",
            "orientation",
            format!(
                "unsupported {:?}; W1.3A supports orthogonal only",
                map.orientation
            ),
        ));
    }
    if map.infinite() {
        return Err(issue(
            &tmx_path,
            "-",
            "infinite",
            "unsupported; W1.3A supports finite maps only",
        ));
    }
    if map.skew_x != 0 || map.skew_y != 0 {
        return Err(issue(
            &tmx_path,
            "-",
            "skew",
            "oblique map skew is unsupported in W1.3A",
        ));
    }
    if map.background_color.is_some() {
        return Err(issue(
            &tmx_path,
            "-",
            "background_color",
            "TMX map background color is unsupported; author a BACKGROUND image layer instead",
        ));
    }
    if map.width == 0 || map.height == 0 || map.tile_width == 0 || map.tile_height == 0 {
        return Err(issue(
            &tmx_path,
            "-",
            "dimensions",
            "map and grid dimensions must be positive",
        ));
    }
    let pixel_width = map
        .width
        .checked_mul(map.tile_width)
        .ok_or_else(|| issue(&tmx_path, "-", "width", "pixel width overflow"))?;
    let pixel_height = map
        .height
        .checked_mul(map.tile_height)
        .ok_or_else(|| issue(&tmx_path, "-", "height", "pixel height overflow"))?;
    let world_width = pixel_width as f32 / pixels_per_world_unit;
    let world_height = pixel_height as f32 / pixels_per_world_unit;
    if world_width + f32::EPSILON < MIN_MAP_WIDTH_WU
        || world_height + f32::EPSILON < MIN_MAP_HEIGHT_WU
    {
        return Err(issue(
            &tmx_path,
            "-",
            "dimensions",
            format!(
                "map is {world_width:.3} × {world_height:.3} wu; minimum gameplay map is {:.3} × {:.3} wu (one full camera viewport)",
                MIN_MAP_WIDTH_WU, MIN_MAP_HEIGHT_WU
            ),
        ));
    }
    let graphic_root = graphic_root(sidecar_path)?;
    validate_tilesets(&map, &tmx_path)?;

    let mut compiler = Compiler {
        tmx_path: &tmx_path,
        graphic_root: &graphic_root,
        map_height_px: pixel_height as f32,
        ppu: pixels_per_world_unit,
        assets: BTreeMap::new(),
    };
    let mut layers = Vec::with_capacity(map.layers().len());
    for layer in map.layers() {
        layers.push(compiler.compile_layer(layer, &map)?);
    }

    Ok(MapPresentation {
        schema_version: MAP_PRESENTATION_SCHEMA_VERSION,
        map_authored: source.id.clone(),
        visual_extent_px: [pixel_width, pixel_height],
        pixels_per_world_unit,
        world_bounds: [
            0.0,
            0.0,
            pixel_width as f32 / pixels_per_world_unit,
            pixel_height as f32 / pixels_per_world_unit,
        ],
        assets: compiler.assets.into_values().collect(),
        layers,
    })
}

pub fn serialize_map_pretty(map: &MapPresentation) -> Result<Vec<u8>, ContentError> {
    let mut bytes = serde_json::to_vec_pretty(map)
        .map_err(|error| issue(Path::new("canonical-map"), "-", "json", error.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// One registered map compiled from its authoring sidecar and environment.
///
/// Identity is the registry `ContentId`. The TMX path is a compiler input recorded
/// in `inputs` for rebuild tracking; it is not runtime map identity.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledRegisteredMap {
    pub content_id: purgatory_common::ContentId,
    pub authored_id: String,
    pub presentation: MapPresentation,
    pub environment: crate::MapEnvironmentPresentation,
    pub inputs: Vec<PathBuf>,
}

/// Compile every map registered in `content_root` through the shared authoring contract.
///
/// Discovery starts at `ContentRegistry`, then opens
/// `authoring/maps/<authored-id>.purgatory-map.json` and the sibling environment file.
/// Sidecars and TMX files that are not registered maps are ignored.
pub fn compile_registered_map_presentations(
    content_root: &Path,
) -> Result<Vec<CompiledRegisteredMap>, ContentError> {
    let registry = crate::load_registry(content_root, crate::LoadMode::Shared)?;
    let authoring = content_root.join("authoring").join("maps");
    let mut maps: Vec<_> = registry.iter_maps().collect();
    maps.sort_by_key(|map| map.content_id.raw().unwrap_or(u32::MAX));

    let mut compiled = Vec::with_capacity(maps.len());
    for map in maps {
        let sidecar = authoring.join(format!("{}.purgatory-map.json", map.authored_id));
        if !sidecar.is_file() {
            return Err(issue(
                &sidecar,
                &map.authored_id,
                "sidecar",
                "registered map has no authoring sidecar",
            ));
        }
        let source = load_map_authoring(&sidecar)?;
        let source_id = purgatory_common::ContentId::from_raw(source.content_id);
        if source.id != map.authored_id || source_id != map.content_id {
            return Err(issue(
                &sidecar,
                &map.authored_id,
                "identity",
                format!(
                    "sidecar identity {}:{} does not match registered map {}:{}",
                    source.content_id,
                    source.id,
                    map.content_id.raw().unwrap_or(0),
                    map.authored_id
                ),
            ));
        }

        let environment_path = authoring.join(format!("{}.environment.json", map.authored_id));
        if !environment_path.is_file() {
            return Err(issue(
                &environment_path,
                &map.authored_id,
                "environment",
                "registered map has no environment authoring",
            ));
        }
        let environment_bytes = fs::read(&environment_path)
            .map_err(|error| ContentError::from_io(&environment_path, &error))?;
        let environment_authoring: crate::MapEnvironmentAuthoring =
            serde_json::from_slice(&environment_bytes).map_err(|error| {
                issue(
                    &environment_path,
                    &map.authored_id,
                    "json",
                    error.to_string(),
                )
            })?;
        if environment_authoring.schema_version != crate::MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION {
            return Err(issue(
                &environment_path,
                &map.authored_id,
                "schema_version",
                format!(
                    "unsupported environment schema {} (want {})",
                    environment_authoring.schema_version,
                    crate::MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION
                ),
            ));
        }
        if environment_authoring.map_authored != map.authored_id {
            return Err(issue(
                &environment_path,
                &map.authored_id,
                "map_authored",
                format!(
                    "environment map {} does not match registered map {}",
                    environment_authoring.map_authored, map.authored_id
                ),
            ));
        }

        let presentation = compile_tiled_map(&sidecar)?;
        if presentation.map_authored != map.authored_id {
            return Err(issue(
                &sidecar,
                &map.authored_id,
                "map_authored",
                format!(
                    "compiled presentation map {} does not match registered map {}",
                    presentation.map_authored, map.authored_id
                ),
            ));
        }
        let graphic = graphic_root(&sidecar)?;
        let environment = crate::compile_map_environment(&environment_authoring, &graphic)
            .map_err(|error| issue(&environment_path, &map.authored_id, "environment", error))?;
        let tmx_path = sidecar
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&source.visual_source);
        compiled.push(CompiledRegisteredMap {
            content_id: map.content_id,
            authored_id: map.authored_id.clone(),
            presentation,
            environment,
            inputs: vec![
                sidecar,
                environment_path,
                content_root
                    .join("shared")
                    .join("maps")
                    .join(format!("{}.json", map.authored_id)),
                tmx_path,
            ],
        });
    }
    Ok(compiled)
}

struct Compiler<'a> {
    tmx_path: &'a Path,
    graphic_root: &'a Path,
    map_height_px: f32,
    ppu: f32,
    assets: BTreeMap<String, PresentationAsset>,
}

impl Compiler<'_> {
    fn compile_layer(
        &mut self,
        layer: Layer<'_>,
        map: &tiled::Map,
    ) -> Result<PresentationLayer, ContentError> {
        validate_layer(self.tmx_path, &layer)?;
        let kind;
        let sprites = match layer.layer_type() {
            LayerType::Image(image_layer) => {
                kind = PresentationLayerKind::Image;
                if image_layer.repeat_x || image_layer.repeat_y {
                    return Err(issue(
                        self.tmx_path,
                        &layer.name,
                        "repeat",
                        "image-layer repeat is unsupported",
                    ));
                }
                let image = image_layer.image.as_ref().ok_or_else(|| {
                    issue(
                        self.tmx_path,
                        &layer.name,
                        "image",
                        "image layer has no image",
                    )
                })?;
                if image.transparent_colour.is_some() {
                    return Err(issue(
                        self.tmx_path,
                        &layer.name,
                        "image.trans",
                        "color-key transparency is unsupported; use PNG alpha",
                    ));
                }
                let (asset_id, size) =
                    self.register_image(&image.source, image.width, image.height)?;
                vec![self.sprite(
                    asset_id,
                    [0, 0, size[0], size[1]],
                    [layer.offset_x, layer.offset_y],
                    [size[0] as f32, size[1] as f32],
                    true,
                    1.0,
                    TileTransform::default(),
                    0,
                )?]
            }
            LayerType::Tiles(tile_layer) => {
                kind = PresentationLayerKind::Tile;
                self.compile_tile_layer(&layer, tile_layer, map)?
            }
            LayerType::Objects(object_layer) => {
                kind = PresentationLayerKind::Object;
                self.compile_object_layer(&layer, object_layer)?
            }
            LayerType::Group(_) => {
                return Err(issue(
                    self.tmx_path,
                    &layer.name,
                    "group",
                    "group nesting is unsupported in W1.3A",
                ));
            }
        };
        Ok(PresentationLayer {
            name: layer.name.clone(),
            kind,
            visible: layer.visible,
            opacity: layer.opacity,
            sprites,
        })
    }

    fn compile_tile_layer(
        &mut self,
        layer: &Layer<'_>,
        tile_layer: TileLayer<'_>,
        map: &tiled::Map,
    ) -> Result<Vec<PresentationSprite>, ContentError> {
        let TileLayer::Finite(finite) = tile_layer else {
            return Err(issue(
                self.tmx_path,
                &layer.name,
                "tile_layer",
                "infinite tile layers are unsupported",
            ));
        };
        let (xs, ys) = render_axes(map.render_order, finite.width(), finite.height());
        let mut sprites = Vec::new();
        for y in ys {
            for &x in &xs {
                let Some(tile) = finite.get_tile(x as i32, y as i32) else {
                    continue;
                };
                let tileset = tile.get_tileset();
                validate_tile_usage(self.tmx_path, &layer.name, tileset, tile.get_tile())?;
                let rect = tile_source_rect(self.tmx_path, &layer.name, tile.id(), tileset)?;
                let image = tileset.image.as_ref().expect("validated atlas tileset");
                let (asset_id, _) =
                    self.register_image(&image.source, image.width, image.height)?;
                let size = [tileset.tile_width as f32, tileset.tile_height as f32];
                let top_left = [
                    x as f32 * map.tile_width as f32 + layer.offset_x + tileset.offset_x as f32,
                    (y + 1) as f32 * map.tile_height as f32 - size[1]
                        + layer.offset_y
                        + tileset.offset_y as f32,
                ];
                sprites.push(self.sprite(
                    asset_id,
                    rect,
                    top_left,
                    size,
                    true,
                    1.0,
                    TileTransform {
                        flip_horizontal: tile.flip_h,
                        flip_vertical: tile.flip_v,
                        flip_diagonal: tile.flip_d,
                    },
                    sprites.len() as u32,
                )?);
            }
        }
        Ok(sprites)
    }

    fn compile_object_layer(
        &mut self,
        layer: &Layer<'_>,
        object_layer: tiled::ObjectLayer<'_>,
    ) -> Result<Vec<PresentationSprite>, ContentError> {
        let mut indices: Vec<_> = (0..object_layer.object_data().len()).collect();
        if object_layer.draw_order == DrawOrder::TopDown {
            indices.sort_by(|&a, &b| {
                object_layer.object_data()[a]
                    .y
                    .total_cmp(&object_layer.object_data()[b].y)
                    .then(a.cmp(&b))
            });
        }
        let mut sprites = Vec::new();
        for index in indices {
            let object = object_layer
                .get_object(index)
                .expect("index from object data");
            let Some(tile) = object.get_tile() else {
                continue;
            };
            if object.rotation.abs() > f32::EPSILON {
                return Err(issue(
                    self.tmx_path,
                    &layer.name,
                    "object.rotation",
                    "rotated tile objects are unsupported in W1.3A",
                ));
            }
            let tileset = tile.get_tileset();
            validate_tile_usage(self.tmx_path, &layer.name, tileset, tile.get_tile())?;
            let rect = tile_source_rect(self.tmx_path, &layer.name, tile.id(), tileset)?;
            let image = tileset.image.as_ref().expect("validated atlas tileset");
            let (asset_id, _) = self.register_image(&image.source, image.width, image.height)?;
            let (raw_width, raw_height) = match object.shape {
                ObjectShape::Rect { width, height } => (width, height),
                _ => {
                    return Err(issue(
                        self.tmx_path,
                        &layer.name,
                        "object.shape",
                        "tile object must use rectangular bounds",
                    ));
                }
            };
            let size = [
                if raw_width > 0.0 {
                    raw_width
                } else {
                    tileset.tile_width as f32
                },
                if raw_height > 0.0 {
                    raw_height
                } else {
                    tileset.tile_height as f32
                },
            ];
            let alignment = match tileset.object_alignment {
                ObjectAlignment::Unspecified => ObjectAlignment::BottomLeft,
                other => other,
            };
            let anchor_offset = object_anchor_top_left(alignment, size);
            let top_left = [
                object.x + layer.offset_x + anchor_offset[0] + tileset.offset_x as f32,
                object.y + layer.offset_y + anchor_offset[1] + tileset.offset_y as f32,
            ];
            sprites.push(self.sprite(
                asset_id,
                rect,
                top_left,
                size,
                object.visible,
                object.opacity,
                TileTransform {
                    flip_horizontal: tile.flip_h,
                    flip_vertical: tile.flip_v,
                    flip_diagonal: tile.flip_d,
                },
                sprites.len() as u32,
            )?);
        }
        Ok(sprites)
    }

    #[allow(clippy::too_many_arguments)]
    fn sprite(
        &self,
        asset_id: String,
        source_rect_px: [u32; 4],
        top_left_px: [f32; 2],
        size_px: [f32; 2],
        visible: bool,
        opacity: f32,
        transform: TileTransform,
        draw_order: u32,
    ) -> Result<PresentationSprite, ContentError> {
        if !top_left_px
            .iter()
            .chain(size_px.iter())
            .all(|v| v.is_finite())
            || size_px.iter().any(|v| *v <= 0.0)
        {
            return Err(issue(
                self.tmx_path,
                "-",
                "sprite.geometry",
                "sprite geometry must be finite and positive",
            ));
        }
        validate_opacity(self.tmx_path, "-", "sprite.opacity", opacity)?;
        Ok(PresentationSprite {
            asset_id,
            source_rect_px,
            position_world: [
                (top_left_px[0] + size_px[0] * 0.5) / self.ppu,
                (self.map_height_px - top_left_px[1] - size_px[1] * 0.5) / self.ppu,
            ],
            size_world: [size_px[0] / self.ppu, size_px[1] / self.ppu],
            visible,
            opacity,
            transform,
            draw_order,
        })
    }

    fn register_image(
        &mut self,
        source: &Path,
        width: i32,
        height: i32,
    ) -> Result<(String, [u32; 2]), ContentError> {
        if width <= 0 || height <= 0 {
            return Err(issue(
                self.tmx_path,
                "-",
                "image.dimensions",
                "image dimensions must be positive",
            ));
        }
        let canonical = source.canonicalize().map_err(|error| {
            issue(
                self.tmx_path,
                "-",
                "image.source",
                format!("cannot resolve {}: {error}", source.display()),
            )
        })?;
        let relative = canonical.strip_prefix(self.graphic_root).map_err(|_| {
            issue(
                self.tmx_path,
                "-",
                "image.source",
                format!("image must be under Graphic/: {}", source.display()),
            )
        })?;
        let source_path = relative.to_string_lossy().replace('\\', "/");
        let id = stable_asset_id(&source_path);
        let size = [width as u32, height as u32];
        let asset = PresentationAsset {
            id: id.clone(),
            source_path,
            image_size_px: size,
        };
        if let Some(existing) = self.assets.get(&id) {
            if existing != &asset {
                return Err(issue(
                    self.tmx_path,
                    "-",
                    "asset_id",
                    format!("stable asset identity collision for {id}"),
                ));
            }
        } else {
            self.assets.insert(id.clone(), asset);
        }
        Ok((id, size))
    }
}

fn validate_authoring(path: &Path, source: &MapAuthoringSource) -> Result<(), ContentError> {
    if source.schema_version != MAP_AUTHORING_SCHEMA_VERSION {
        return Err(issue(
            path,
            &source.id,
            "schema_version",
            format!(
                "expected {}, got {}",
                MAP_AUTHORING_SCHEMA_VERSION, source.schema_version
            ),
        ));
    }
    purgatory_common::ContentId::from_authored(&source.id)
        .map_err(|error| issue(path, &source.id, "id", format!("{error:?}")))?;
    let content_id = purgatory_common::ContentId::from_raw(source.content_id);
    if content_id.kind() != Some(purgatory_common::ContentKind::Map) {
        return Err(issue(
            path,
            &source.id,
            "content_id",
            "must be an allocated map ContentId in 50,000-59,999",
        ));
    }
    let visual = Path::new(&source.visual_source);
    if visual.as_os_str().is_empty() || visual.is_absolute() {
        return Err(issue(
            path,
            &source.id,
            "visual_source",
            "must be a nonempty relative path",
        ));
    }
    validate_ppu(path, source.pixels_per_world_unit)
}

/// Reject tile-size or orientation drift from the canonical map grid.
///
/// Width and height may change. Tile size and orientation may not.
pub fn validate_canonical_map_grid(tmx_path: &Path) -> Result<(), ContentError> {
    let mut loader = tiled::Loader::new();
    let map = loader
        .load_tmx_map(tmx_path)
        .map_err(|error| issue(tmx_path, "-", "tmx", error.to_string()))?;
    if map.orientation != Orientation::Orthogonal {
        return Err(issue(
            tmx_path,
            "-",
            "orientation",
            format!(
                "orientation {:?} does not match the canonical orthogonal map; orientation migration is not supported",
                map.orientation
            ),
        ));
    }
    if map.tile_width != CANONICAL_MAP_TILE_PX || map.tile_height != CANONICAL_MAP_TILE_PX {
        return Err(issue(
            tmx_path,
            "-",
            "tile_size",
            format!(
                "tile size {}×{} px does not match the canonical {CANONICAL_MAP_TILE_PX}×{CANONICAL_MAP_TILE_PX} px map grid; tile-size migration is not supported",
                map.tile_width, map.tile_height
            ),
        ));
    }
    Ok(())
}

fn validate_ppu(path: &Path, ppu: f32) -> Result<(), ContentError> {
    if !ppu.is_finite() || ppu <= 0.0 {
        return Err(issue(
            path,
            "-",
            "pixels_per_world_unit",
            "must be finite and positive",
        ));
    }
    Ok(())
}

fn validate_tilesets(map: &tiled::Map, path: &Path) -> Result<(), ContentError> {
    for tileset in map.tilesets() {
        if tileset.source == map.source {
            return Err(issue(
                path,
                &tileset.name,
                "tileset.source",
                "embedded tilesets are unsupported; use an external TSX",
            ));
        }
        let image = tileset.image.as_ref().ok_or_else(|| {
            issue(
                path,
                &tileset.name,
                "tileset.image",
                "collection-of-images tilesets are unsupported",
            )
        })?;
        if image.width <= 0 || image.height <= 0 {
            return Err(issue(
                path,
                &tileset.name,
                "tileset.image.dimensions",
                "atlas image dimensions must be positive",
            ));
        }
        if image.transparent_colour.is_some() {
            return Err(issue(
                path,
                &tileset.name,
                "tileset.image.trans",
                "color-key transparency is unsupported; use PNG alpha",
            ));
        }
        if tileset.tile_width == 0
            || tileset.tile_height == 0
            || tileset.columns == 0
            || tileset.tilecount == 0
        {
            return Err(issue(
                path,
                &tileset.name,
                "tileset.dimensions",
                "atlas dimensions, columns, and tile count must be positive",
            ));
        }
        if tileset.tile_render_size != TileRenderSize::Tile
            || tileset.fill_mode != FillMode::Stretch
        {
            return Err(issue(
                path,
                &tileset.name,
                "tileset.rendering",
                "grid render size and preserve-aspect fill are unsupported",
            ));
        }
    }
    Ok(())
}

fn validate_layer(path: &Path, layer: &Layer<'_>) -> Result<(), ContentError> {
    for (field, value) in [
        ("offset_x", layer.offset_x),
        ("offset_y", layer.offset_y),
        ("parallax_x", layer.parallax_x),
        ("parallax_y", layer.parallax_y),
    ] {
        if !value.is_finite() {
            return Err(issue(path, &layer.name, field, "must be finite"));
        }
    }
    validate_opacity(path, &layer.name, "opacity", layer.opacity)?;
    if (layer.parallax_x - 1.0).abs() > f32::EPSILON
        || (layer.parallax_y - 1.0).abs() > f32::EPSILON
    {
        return Err(issue(
            path,
            &layer.name,
            "parallax",
            "layer parallax is unsupported in W1.3A",
        ));
    }
    if layer.tint_color.is_some() {
        return Err(issue(
            path,
            &layer.name,
            "tint_color",
            "layer tint is unsupported in W1.3A",
        ));
    }
    if layer.blend_mode != "normal" {
        return Err(issue(
            path,
            &layer.name,
            "blend_mode",
            "non-normal blend modes are unsupported in W1.3A",
        ));
    }
    Ok(())
}

fn validate_opacity(
    path: &Path,
    definition: &str,
    field: &str,
    value: f32,
) -> Result<(), ContentError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(issue(path, definition, field, "must be within 0..=1"))
    }
}

fn validate_tile_usage(
    path: &Path,
    layer: &str,
    tileset: &Tileset,
    tile: Option<tiled::Tile<'_>>,
) -> Result<(), ContentError> {
    let tile = tile.ok_or_else(|| {
        issue(
            path,
            layer,
            "tile",
            "tile reference is outside its tileset (hexagonal bit 29 is not valid here)",
        )
    })?;
    if tile.animation.is_some() {
        return Err(issue(
            path,
            layer,
            "tile.animation",
            "tile animation is unsupported in W1.3A",
        ));
    }
    if tileset.image.is_none() {
        return Err(issue(
            path,
            layer,
            "tileset.image",
            "collection-of-images tilesets are unsupported",
        ));
    }
    Ok(())
}

fn tile_source_rect(
    path: &Path,
    layer: &str,
    local_id: u32,
    tileset: &Tileset,
) -> Result<[u32; 4], ContentError> {
    // For atlas tilesets Tiled can legally carry explicitly-authored tile IDs
    // beyond tilecount. Presence is already validated through rs-tiled's
    // get_tile(); the atlas rectangle is the actual renderability bound here.
    let col = local_id % tileset.columns;
    let row = local_id / tileset.columns;
    let x = tileset.margin + col * (tileset.tile_width + tileset.spacing);
    let y = tileset.margin + row * (tileset.tile_height + tileset.spacing);
    let rect = [x, y, tileset.tile_width, tileset.tile_height];
    let image = tileset.image.as_ref().expect("validated atlas");
    if x.saturating_add(rect[2]) > image.width as u32
        || y.saturating_add(rect[3]) > image.height as u32
    {
        return Err(issue(
            path,
            layer,
            "source_rect",
            format!("tile {local_id} source rectangle exceeds atlas"),
        ));
    }
    Ok(rect)
}

fn render_axes(order: RenderOrder, width: u32, height: u32) -> (Vec<u32>, Vec<u32>) {
    let mut xs: Vec<_> = (0..width).collect();
    let mut ys: Vec<_> = (0..height).collect();
    if matches!(order, RenderOrder::LeftDown | RenderOrder::LeftUp) {
        xs.reverse();
    }
    if matches!(order, RenderOrder::RightUp | RenderOrder::LeftUp) {
        ys.reverse();
    }
    (xs, ys)
}

fn object_anchor_top_left(alignment: ObjectAlignment, size: [f32; 2]) -> [f32; 2] {
    let [w, h] = size;
    match alignment {
        ObjectAlignment::Unspecified | ObjectAlignment::BottomLeft => [0.0, -h],
        ObjectAlignment::TopLeft => [0.0, 0.0],
        ObjectAlignment::Top => [-w * 0.5, 0.0],
        ObjectAlignment::TopRight => [-w, 0.0],
        ObjectAlignment::Left => [0.0, -h * 0.5],
        ObjectAlignment::Center => [-w * 0.5, -h * 0.5],
        ObjectAlignment::Right => [-w, -h * 0.5],
        ObjectAlignment::Bottom => [-w * 0.5, -h],
        ObjectAlignment::BottomRight => [-w, -h],
    }
}

fn stable_asset_id(path: &str) -> String {
    let mut id = String::from("graphic.");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' {
            id.push(byte as char);
        } else {
            id.push('_');
            id.push_str(&format!("{byte:02X}"));
        }
    }
    id
}

fn graphic_root(sidecar: &Path) -> Result<PathBuf, ContentError> {
    for ancestor in sidecar.ancestors() {
        let graphic = ancestor.join("Graphic");
        if graphic.is_dir() {
            return graphic.canonicalize().map_err(|error| {
                issue(
                    sidecar,
                    "-",
                    "Graphic",
                    format!("cannot resolve {}: {error}", graphic.display()),
                )
            });
        }
    }
    Err(issue(
        sidecar,
        "-",
        "Graphic",
        "cannot locate repository Graphic/ root",
    ))
}

fn issue(path: &Path, definition: &str, field: &str, reason: impl Into<String>) -> ContentError {
    ContentError::one(ValidationIssue::new(
        path.display().to_string(),
        definition,
        field,
        reason,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_sidecar() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/authoring/maps/map.map1.purgatory-map.json")
    }

    #[test]
    fn production_maps_use_the_canonical_tile_grid() {
        let maps = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Graphic/assets/maps");
        for name in ["50001.tmx", "50002.tmx"] {
            validate_canonical_map_grid(&maps.join(name)).expect(name);
        }
    }

    #[test]
    fn real_fixture_compiles_to_valid_nonempty_visual_map() {
        let path = fixture_sidecar();
        let source = load_map_authoring(&path).expect("fixture sidecar loads");
        let map = compile_tiled_map(&path).expect("fixture compiles");

        assert_eq!(map.schema_version, MAP_PRESENTATION_SCHEMA_VERSION);
        assert_eq!(map.map_authored, source.id);
        assert!(map.visual_extent_px[0] > 0);
        assert!(map.visual_extent_px[1] > 0);
        assert!(map.pixels_per_world_unit.is_finite());
        assert!(map.pixels_per_world_unit > 0.0);
        assert!(map.world_bounds.iter().all(|value| value.is_finite()));
        assert!(map.world_bounds[2] > map.world_bounds[0]);
        assert!(map.world_bounds[3] > map.world_bounds[1]);
        assert!(!map.layers.is_empty());

        let expected_width = map.visual_extent_px[0] as f32 / map.pixels_per_world_unit;
        let expected_height = map.visual_extent_px[1] as f32 / map.pixels_per_world_unit;
        assert!((map.world_bounds[2] - expected_width).abs() < 1e-5);
        assert!((map.world_bounds[3] - expected_height).abs() < 1e-5);
    }

    #[test]
    fn world_conversion_is_map_local_y_up_and_free_position_is_not_snapped() {
        let compiler = Compiler {
            tmx_path: Path::new("synthetic.tmx"),
            graphic_root: Path::new("."),
            map_height_px: 1000.0,
            ppu: 100.0,
            assets: BTreeMap::new(),
        };
        let sprite = compiler
            .sprite(
                "graphic.synthetic".to_owned(),
                [0, 0, 40, 20],
                [123.0, 456.0],
                [40.0, 20.0],
                true,
                1.0,
                TileTransform::default(),
                0,
            )
            .unwrap();

        assert_eq!(sprite.size_world, [0.4, 0.2]);
        assert!((sprite.position_world[0] - 1.43).abs() < 1e-5);
        assert!((sprite.position_world[1] - 5.34).abs() < 1e-5);
    }

    #[test]
    fn compiled_sprite_asset_refs_and_source_rects_are_valid() {
        let map = compile_tiled_map(&fixture_sidecar()).unwrap();

        for layer in &map.layers {
            for sprite in &layer.sprites {
                let asset = map
                    .assets
                    .iter()
                    .find(|asset| asset.id == sprite.asset_id)
                    .expect("every sprite asset id resolves");
                let [x, y, width, height] = sprite.source_rect_px;
                assert!(width > 0);
                assert!(height > 0);
                assert!(x.saturating_add(width) <= asset.image_size_px[0]);
                assert!(y.saturating_add(height) <= asset.image_size_px[1]);
                assert!(sprite.position_world.iter().all(|value| value.is_finite()));
                assert!(
                    sprite
                        .size_world
                        .iter()
                        .all(|value| value.is_finite() && *value > 0.0)
                );
            }
        }
    }

    #[test]
    fn ppu_changes_world_extent_without_changing_tmx_extent() {
        let path = fixture_sidecar();
        let source = load_map_authoring(&path).unwrap();
        let baseline =
            compile_tiled_map_with_ppu(&path, &source, source.pixels_per_world_unit).unwrap();
        let preview_ppu = source.pixels_per_world_unit * 0.5;
        let scaled = compile_tiled_map_with_ppu(&path, &source, preview_ppu).unwrap();

        assert_eq!(scaled.visual_extent_px, baseline.visual_extent_px);
        assert!((scaled.world_bounds[2] - baseline.world_bounds[2] * 2.0).abs() < 1e-5);
        assert!((scaled.world_bounds[3] - baseline.world_bounds[3] * 2.0).abs() < 1e-5);
    }

    #[test]
    fn canonical_serialization_roundtrips_and_is_byte_stable() {
        let map = compile_tiled_map(&fixture_sidecar()).unwrap();
        let first = serialize_map_pretty(&map).unwrap();
        let decoded: MapPresentation = serde_json::from_slice(&first).unwrap();
        assert_eq!(decoded, map);
        assert_eq!(first, serialize_map_pretty(&decoded).unwrap());
        assert_eq!(
            first,
            serialize_map_pretty(&compile_tiled_map(&fixture_sidecar()).unwrap()).unwrap()
        );
    }

    #[test]
    fn orthogonal_flip_flags_are_canonical_not_raw_gids() {
        let transform = TileTransform {
            flip_horizontal: true,
            flip_vertical: true,
            flip_diagonal: true,
        };
        let json = serde_json::to_string(&transform).unwrap();
        assert_eq!(
            json,
            r#"{"flip_horizontal":true,"flip_vertical":true,"flip_diagonal":true}"#
        );
    }

    #[test]
    fn registered_maps_compile_without_a_handwritten_client_list() {
        let root = crate::default_content_root();
        let compiled = compile_registered_map_presentations(&root).expect("production maps");
        let registry = crate::load_registry(&root, crate::LoadMode::Shared).expect("registry");
        let mut expected: Vec<_> = registry.iter_maps().map(|map| map.content_id).collect();
        expected.sort_by_key(|id| id.raw());
        let actual: Vec<_> = compiled.iter().map(|map| map.content_id).collect();
        assert_eq!(actual, expected);
        assert!(actual.len() >= 2);

        let map1 = compiled
            .iter()
            .find(|map| map.content_id == purgatory_common::MAP1)
            .expect("MAP1");
        let direct = compile_tiled_map(
            &root
                .join("authoring")
                .join("maps")
                .join("map.map1.purgatory-map.json"),
        )
        .expect("MAP1 sidecar");
        assert_eq!(map1.presentation, direct);
        assert_eq!(map1.environment.map_authored, map1.authored_id);

        let map2 = compiled
            .iter()
            .find(|map| map.content_id == purgatory_common::MAP2)
            .expect("MAP2");
        let direct = compile_tiled_map(
            &root
                .join("authoring")
                .join("maps")
                .join("map.map2.purgatory-map.json"),
        )
        .expect("MAP2 sidecar");
        assert_eq!(map2.presentation, direct);
        assert_eq!(map2.environment.map_authored, "map.map2");
        assert_ne!(
            map1.presentation.map_authored,
            map2.presentation.map_authored
        );
        assert_ne!(map1.environment, map2.environment);

        let text = String::from_utf8(serialize_map_pretty(&map2.presentation).unwrap()).unwrap();
        assert!(!text.contains("firstgid"));
        assert!(!text.contains(".tmx"));
        assert!(!text.contains(".tsx"));
    }

    #[test]
    fn a_third_registered_map_compiles_without_a_source_list() {
        let root = fixture_content_root("third-map");
        let tmx = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Graphic/assets/maps/50001.tmx");
        let visual = relative_to(&root.join("authoring/maps"), &tmx);
        for (authored, content_id, top) in [
            ("map.fixture.a", 50_010, [1, 2, 3, 255]),
            ("map.fixture.b", 50_011, [7, 8, 9, 255]),
            ("map.fixture.c", 50_012, [13, 14, 15, 255]),
        ] {
            write_registered_map(&root, authored, content_id, &visual, top, true);
        }
        fs::write(
            root.join("authoring/maps/map.extra.purgatory-map.json"),
            b"not-a-map",
        )
        .unwrap();
        fs::write(root.join("authoring/maps/99999.tmx"), b"<map/>").unwrap();

        let compiled = compile_registered_map_presentations(&root).expect("three fixture maps");
        let ids: Vec<u32> = compiled
            .iter()
            .map(|map| map.content_id.raw().unwrap())
            .collect();
        assert_eq!(ids, vec![50_010, 50_011, 50_012]);
        assert_eq!(
            compiled[0].environment.sky_gradient.unwrap().top_rgba,
            [1, 2, 3, 255]
        );
        assert_eq!(
            compiled[1].environment.sky_gradient.unwrap().top_rgba,
            [7, 8, 9, 255]
        );
        assert_eq!(
            compiled[2].environment.sky_gradient.unwrap().top_rgba,
            [13, 14, 15, 255]
        );
        assert!(
            compiled
                .iter()
                .all(|map| map.presentation.layers.len() == compiled[0].presentation.layers.len())
        );
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn registered_map_without_environment_fails_explicitly() {
        let root = fixture_content_root("missing-environment");
        let tmx = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Graphic/assets/maps/50001.tmx");
        let visual = relative_to(&root.join("authoring/maps"), &tmx);
        write_registered_map(
            &root,
            "map.fixture.a",
            50_010,
            &visual,
            [1, 2, 3, 255],
            false,
        );
        let error = compile_registered_map_presentations(&root).unwrap_err();
        let text = error.to_string();
        assert!(text.contains("environment"), "{text}");
        assert!(text.contains("map.fixture.a"), "{text}");
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    fn fixture_content_root(name: &str) -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/map-presentation-fixtures")
            .join(format!(
                "{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
            .join("content");
        fs::create_dir_all(root.join("authoring/maps")).unwrap();
        fs::create_dir_all(root.join("shared/maps")).unwrap();
        root
    }

    fn write_registered_map(
        content_root: &Path,
        authored: &str,
        content_id: u32,
        visual_source: &str,
        top_rgba: [u8; 4],
        with_environment: bool,
    ) {
        let map = format!(
            r#"{{
  "schema_version": 1,
  "content_id": {content_id},
  "id": "{authored}",
  "debug_name": "{authored}",
  "bounds": {{ "min_x": 0.0, "max_x": 40.0, "min_y": 0.0, "max_y": 13.0 }},
  "spawn_points": [{{ "id": "default", "position": [2.0, 1.0] }}],
  "restore": {{ "policy": "safe_point", "point": "default" }},
  "platforms": []
}}
"#
        );
        fs::write(
            content_root
                .join("shared/maps")
                .join(format!("{authored}.json")),
            map,
        )
        .unwrap();
        let sidecar = format!(
            r#"{{
  "schema_version": 2,
  "content_id": {content_id},
  "id": "{authored}",
  "visual_source": "{visual_source}",
  "pixels_per_world_unit": 100.0
}}
"#
        );
        fs::write(
            content_root
                .join("authoring/maps")
                .join(format!("{authored}.purgatory-map.json")),
            sidecar,
        )
        .unwrap();
        if with_environment {
            let environment = format!(
                r#"{{
  "schema_version": 1,
  "map_authored": "{authored}",
  "sky_gradient": {{
    "top_rgba": [{}, {}, {}, {}],
    "bottom_rgba": [4, 5, 6, 255]
  }},
  "parallax_layers": [],
  "foreground_layers": [],
  "cloud_fields": []
}}
"#,
                top_rgba[0], top_rgba[1], top_rgba[2], top_rgba[3]
            );
            fs::write(
                content_root
                    .join("authoring/maps")
                    .join(format!("{authored}.environment.json")),
                environment,
            )
            .unwrap();
        }
    }

    fn relative_to(from_dir: &Path, target: &Path) -> String {
        let from = from_dir.canonicalize().unwrap();
        let target = target.canonicalize().unwrap();
        let mut ups = PathBuf::new();
        let mut cursor = from.as_path();
        loop {
            if let Ok(rest) = target.strip_prefix(cursor) {
                return ups.join(rest).to_string_lossy().replace('\\', "/");
            }
            ups.push("..");
            cursor = cursor
                .parent()
                .expect("relative path stays in the filesystem");
        }
    }

    #[test]
    fn anchors_cover_tiled_orthogonal_tile_object_alignment() {
        assert_eq!(
            object_anchor_top_left(ObjectAlignment::BottomLeft, [12.0, 8.0]),
            [0.0, -8.0]
        );
        assert_eq!(
            object_anchor_top_left(ObjectAlignment::Center, [12.0, 8.0]),
            [-6.0, -4.0]
        );
        assert_eq!(
            object_anchor_top_left(ObjectAlignment::TopRight, [12.0, 8.0]),
            [-12.0, 0.0]
        );
    }
}
