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

pub const MAP_AUTHORING_SCHEMA_VERSION: u32 = 1;
pub const MAP_PRESENTATION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapAuthoringSource {
    pub schema_version: u32,
    pub id: String,
    pub visual_source: String,
    pub pixels_per_world_unit: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MapPresentation {
    pub schema_version: u32,
    pub map_authored: String,
    pub visual_extent_px: [u32; 2],
    pub pixels_per_world_unit: f32,
    /// Map-local Y-up bounds: `[min_x, min_y, max_x, max_y]`.
    pub world_bounds: [f32; 4],
    pub assets: Vec<PresentationAsset>,
    /// Back-to-front source order from the TMX.
    pub layers: Vec<PresentationLayer>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PresentationAsset {
    pub id: String,
    /// Stable path relative to the repository `Graphic/` root.
    pub source_path: String,
    pub image_size_px: [u32; 2],
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationLayerKind {
    Image,
    Tile,
    Object,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PresentationLayer {
    pub name: String,
    pub kind: PresentationLayerKind,
    pub visible: bool,
    pub opacity: f32,
    pub sprites: Vec<PresentationSprite>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TileTransform {
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    /// Tiled orthogonal anti-diagonal flip. Apply before H/V flips.
    pub flip_diagonal: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PresentationSprite {
    pub asset_id: String,
    pub source_rect_px: [u32; 4],
    pub position_world: [f32; 2],
    pub size_world: [f32; 2],
    pub visible: bool,
    pub opacity: f32,
    pub transform: TileTransform,
    /// Stable order within this visual layer.
    pub draw_order: u32,
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
            format!("unsupported {:?}; W1.3A supports orthogonal only", map.orientation),
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
                let (asset_id, size) = self.register_image(&image.source, image.width, image.height)?;
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
        if finite.width() != map.width || finite.height() != map.height {
            return Err(issue(
                self.tmx_path,
                &layer.name,
                "dimensions",
                "finite tile layer dimensions must match the map",
            ));
        }

        let (xs, ys) = render_axes(map.render_order, map.width, map.height);
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
                    x as f32 * map.tile_width as f32
                        + layer.offset_x
                        + tileset.offset_x as f32,
                    (y + 1) as f32 * map.tile_height as f32
                        - size[1]
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
            let object = object_layer.get_object(index).expect("index from object data");
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
        if !top_left_px.iter().chain(size_px.iter()).all(|v| v.is_finite())
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
        if tileset.image.is_none() {
            return Err(issue(
                path,
                &tileset.name,
                "tileset.image",
                "collection-of-images tilesets are unsupported",
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
    if local_id >= tileset.tilecount {
        return Err(issue(
            path,
            layer,
            "tile.id",
            format!("local tile id {local_id} is outside tileset"),
        ));
    }
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
            .join("../../content/authoring/maps/map.dev.footnote.purgatory-map.json")
    }

    #[test]
    fn real_fixture_compiles_complete_visual_stack() {
        let map = compile_tiled_map(&fixture_sidecar()).expect("fixture compiles");
        assert_eq!(map.schema_version, 1);
        assert_eq!(map.map_authored, "map.dev.footnote");
        assert_eq!(map.visual_extent_px, [1080, 720]);
        assert_eq!(map.world_bounds, [0.0, 0.0, 10.8, 7.2]);
        assert_eq!(
            map.layers.iter().map(|layer| layer.name.as_str()).collect::<Vec<_>>(),
            ["BACKGROUND", "3", "4", "2", "1", "WORLD_ART", "MARKERS"]
        );
        assert_eq!(map.layers[0].sprites.len(), 1);
        assert_eq!(
            map.layers
                .iter()
                .filter(|layer| layer.kind == PresentationLayerKind::Tile)
                .count(),
            4
        );
        assert_eq!(
            map.layers
                .iter()
                .filter(|layer| layer.kind == PresentationLayerKind::Tile)
                .map(|layer| layer.sprites.len())
                .sum::<usize>(),
            11
        );
        assert_eq!(map.layers[5].sprites.len(), 1);
        assert!(map.layers[6].sprites.is_empty());
        assert_eq!(map.assets.len(), 2);
        assert_eq!(map.layers[5].sprites[0].source_rect_px, [1080, 0, 360, 240]);
    }

    #[test]
    fn world_conversion_is_map_local_y_up_and_free_position_is_not_snapped() {
        let map = compile_tiled_map(&fixture_sidecar()).unwrap();
        let object = &map.layers[5].sprites[0];
        assert!((object.position_world[0] - 9.57333).abs() < 1e-4);
        assert!((object.position_world[1] - 2.0).abs() < 1e-4);
        assert_eq!(object.size_world, [3.6, 2.4]);
        let background = &map.layers[0].sprites[0];
        assert!((background.position_world[0] - 1.79333).abs() < 1e-4);
        assert!((background.position_world[1] - 1.99333).abs() < 1e-4);
    }

    #[test]
    fn same_atlas_is_deduplicated_and_tile_source_rects_resolve() {
        let map = compile_tiled_map(&fixture_sidecar()).unwrap();
        let tile_sprites: Vec<_> = map.layers[1..5]
            .iter()
            .flat_map(|layer| &layer.sprites)
            .collect();
        assert!(tile_sprites.len() > 1);
        assert!(tile_sprites
            .iter()
            .all(|sprite| sprite.asset_id == tile_sprites[0].asset_id));
        assert!(tile_sprites
            .iter()
            .any(|sprite| sprite.source_rect_px == [0, 0, 360, 240]));
        assert!(tile_sprites
            .iter()
            .any(|sprite| sprite.source_rect_px == [1080, 0, 360, 240]));
    }

    #[test]
    fn ppu_changes_world_extent_without_changing_tmx_extent() {
        let path = fixture_sidecar();
        let source = load_map_authoring(&path).unwrap();
        let map = compile_tiled_map_with_ppu(&path, &source, 50.0).unwrap();
        assert_eq!(map.visual_extent_px, [1080, 720]);
        assert_eq!(map.world_bounds, [0.0, 0.0, 21.6, 14.4]);
    }

    #[test]
    fn canonical_serialization_roundtrips_and_is_byte_stable() {
        let map = compile_tiled_map(&fixture_sidecar()).unwrap();
        let first = serialize_map_pretty(&map).unwrap();
        let decoded: MapPresentation = serde_json::from_slice(&first).unwrap();
        assert_eq!(decoded, map);
        assert_eq!(first, serialize_map_pretty(&decoded).unwrap());
        assert_eq!(first, serialize_map_pretty(&compile_tiled_map(&fixture_sidecar()).unwrap()).unwrap());
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
