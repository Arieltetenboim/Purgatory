//! Compile the small visual-map subset used by the W1.2 authoring proof.
//!
//! This module is an authoring boundary. Its output contains no TMX gids,
//! firstgids, layer ids, or Tiled paths and is safe to consume at runtime.

use std::fs;
use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use serde::{Deserialize, Serialize};

use crate::error::{ContentError, ValidationIssue};

pub const TILED_PIXELS_PER_WORLD_UNIT: f32 = 100.0;
pub const TILED_WORLD_ORIGIN: [f32; 2] = [-24.0, 0.0];
const GID_FLIP_HORIZONTAL: u32 = 0x8000_0000;
const GID_FLIP_VERTICAL: u32 = 0x4000_0000;
const GID_FLIP_DIAGONAL: u32 = 0x2000_0000;
const GID_MASK: u32 = !(GID_FLIP_HORIZONTAL | GID_FLIP_VERTICAL | GID_FLIP_DIAGONAL);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MapPresentation {
    pub map_authored: String,
    pub pixels_per_world_unit: f32,
    pub world_origin: [f32; 2],
    pub backgrounds: Vec<PresentationSprite>,
    pub world_art: Vec<PresentationSprite>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PresentationLayer {
    Background,
    WorldArt,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PresentationSprite {
    pub asset_key: String,
    pub asset_path: String,
    pub source_rect_px: [u32; 4],
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub layer: PresentationLayer,
    pub draw_order: u32,
}

#[derive(Clone, Debug)]
struct Tileset {
    first_gid: u32,
    tile_width: u32,
    tile_height: u32,
    columns: u32,
    image_width: u32,
    image_height: u32,
    image_path: PathBuf,
}

pub fn compile_tiled_map(
    path: &Path,
    pixels_per_world_unit: f32,
) -> Result<MapPresentation, ContentError> {
    if !pixels_per_world_unit.is_finite() || pixels_per_world_unit <= 0.0 {
        return Err(issue(
            path,
            "-",
            "pixels_per_world_unit",
            "must be finite and positive",
        ));
    }
    let bytes = fs::read(path).map_err(|error| ContentError::from_io(path, &error))?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut reader = Reader::from_reader(bytes.as_slice());
    let mut buf = Vec::new();
    let mut tilesets = Vec::new();
    let mut backgrounds = Vec::new();
    let mut world_art = Vec::new();
    let mut current_layer = None;
    let mut background_offset = [0.0_f32, 0.0_f32];
    let mut map_authored = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("map")
        .to_owned();
    let mut order = 0;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(tag)) if tag.name().as_ref() == b"map" => {
                if let Some(name) = attr(&tag, b"class")?.or(attr(&tag, b"name")?) {
                    map_authored = name;
                }
            }
            Ok(Event::Empty(tag)) if tag.name().as_ref() == b"tileset" => {
                let first_gid = required_u32(&tag, b"firstgid", path, "tileset.firstgid")?;
                let source = required_string(&tag, b"source", path, "tileset.source")?;
                tilesets.push(parse_tileset(base.join(source), first_gid)?);
            }
            Ok(Event::Start(tag)) if tag.name().as_ref() == b"imagelayer" => {
                current_layer = attr(&tag, b"name")?.and_then(|name| {
                    (name == "BACKGROUND").then_some(PresentationLayer::Background)
                });
                if current_layer == Some(PresentationLayer::Background) {
                    background_offset = [
                        attr(&tag, b"offsetx")?
                            .unwrap_or_else(|| "0".to_owned())
                            .parse()
                            .map_err(|_| issue(path, "BACKGROUND", "offsetx", "invalid f32"))?,
                        attr(&tag, b"offsety")?
                            .unwrap_or_else(|| "0".to_owned())
                            .parse()
                            .map_err(|_| issue(path, "BACKGROUND", "offsety", "invalid f32"))?,
                    ];
                }
            }
            Ok(Event::Empty(tag))
                if tag.name().as_ref() == b"image"
                    && current_layer == Some(PresentationLayer::Background) =>
            {
                let source = required_string(&tag, b"source", path, "BACKGROUND.image.source")?;
                let width = required_u32(&tag, b"width", path, "BACKGROUND.image.width")?;
                let height = required_u32(&tag, b"height", path, "BACKGROUND.image.height")?;
                let x = background_offset[0];
                let y = background_offset[1];
                backgrounds.push(PresentationSprite {
                    asset_key: "map.dev.footnote.background".to_owned(),
                    asset_path: canonical_asset_path(base, &base.join(source), path)?,
                    source_rect_px: [0, 0, width, height],
                    position: pixel_center_to_world(
                        x,
                        y,
                        width,
                        height,
                        pixels_per_world_unit,
                        TILED_WORLD_ORIGIN,
                    ),
                    size: pixel_size_to_world(width, height, pixels_per_world_unit),
                    layer: PresentationLayer::Background,
                    draw_order: order,
                });
                order += 1;
            }
            Ok(Event::End(tag)) if tag.name().as_ref() == b"imagelayer" => current_layer = None,
            Ok(Event::Start(tag)) if tag.name().as_ref() == b"objectgroup" => {
                current_layer = attr(&tag, b"name")?
                    .and_then(|name| (name == "WORLD_ART").then_some(PresentationLayer::WorldArt));
            }
            Ok(Event::Empty(tag))
                if tag.name().as_ref() == b"object"
                    && current_layer == Some(PresentationLayer::WorldArt) =>
            {
                let gid = required_u32(&tag, b"gid", path, "WORLD_ART.object.gid")?;
                let x = required_f32(&tag, b"x", path, "WORLD_ART.object.x")?;
                let y = required_f32(&tag, b"y", path, "WORLD_ART.object.y")?;
                let width = required_u32(&tag, b"width", path, "WORLD_ART.object.width")?;
                let height = required_u32(&tag, b"height", path, "WORLD_ART.object.height")?;
                let tile = resolve_gid(gid, &tilesets, path)?;
                let source_rect = tile_source_rect(tile.local_id, tile.tileset);
                validate_rect(source_rect, tile.tileset, path)?;
                let sprite = PresentationSprite {
                    asset_key: "map.dev.footnote.world_art".to_owned(),
                    asset_path: canonical_asset_path(base, &tile.tileset.image_path, path)?,
                    source_rect_px: source_rect,
                    position: pixel_center_to_world(
                        x,
                        y,
                        width,
                        height,
                        pixels_per_world_unit,
                        TILED_WORLD_ORIGIN,
                    ),
                    size: pixel_size_to_world(width, height, pixels_per_world_unit),
                    layer: PresentationLayer::WorldArt,
                    draw_order: order,
                };
                world_art.push(sprite);
                order += 1;
            }
            Ok(Event::End(tag)) if tag.name().as_ref() == b"objectgroup" => current_layer = None,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(issue(path, "-", "xml", error.to_string())),
        }
        buf.clear();
    }

    if backgrounds.is_empty() {
        return Err(issue(
            path,
            "BACKGROUND",
            "image",
            "required image layer is missing",
        ));
    }
    if world_art.is_empty() {
        return Err(issue(
            path,
            "WORLD_ART",
            "object",
            "required visual object is missing",
        ));
    }
    Ok(MapPresentation {
        map_authored,
        pixels_per_world_unit,
        world_origin: TILED_WORLD_ORIGIN,
        backgrounds,
        world_art,
    })
}

#[derive(Debug)]
struct ResolvedGid<'a> {
    local_id: u32,
    tileset: &'a Tileset,
}

fn resolve_gid<'a>(
    gid: u32,
    tilesets: &'a [Tileset],
    path: &Path,
) -> Result<ResolvedGid<'a>, ContentError> {
    let clean = gid & GID_MASK;
    if clean == 0 {
        return Err(issue(path, "WORLD_ART", "gid", "zero gid is not a tile"));
    }
    if gid & (GID_FLIP_HORIZONTAL | GID_FLIP_VERTICAL | GID_FLIP_DIAGONAL) != 0 {
        return Err(issue(
            path,
            "WORLD_ART",
            "gid",
            "tile transforms are not supported in W1.2",
        ));
    }
    let index = tilesets
        .iter()
        .enumerate()
        .filter(|(_, set)| set.first_gid <= clean)
        .max_by_key(|(_, set)| set.first_gid)
        .map(|(index, _)| index)
        .ok_or_else(|| issue(path, "WORLD_ART", "gid", format!("unresolvable gid {gid}")))?;
    let set = &tilesets[index];
    let local_id = clean - set.first_gid;
    let capacity = set
        .columns
        .saturating_mul(set.image_height / set.tile_height);
    if local_id >= capacity {
        return Err(issue(
            path,
            "WORLD_ART",
            "gid",
            format!("gid {gid} is outside tileset"),
        ));
    }
    Ok(ResolvedGid {
        local_id,
        tileset: set,
    })
}

fn parse_tileset(path: PathBuf, first_gid: u32) -> Result<Tileset, ContentError> {
    let bytes = fs::read(&path).map_err(|error| {
        issue(
            &path,
            "tileset",
            "source",
            format!("missing referenced TSX: {error}"),
        )
    })?;
    let mut reader = Reader::from_reader(bytes.as_slice());
    let mut buf = Vec::new();
    let mut tile_width = None;
    let mut tile_height = None;
    let mut columns = None;
    let mut image_width = None;
    let mut image_height = None;
    let mut image_source = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(tag)) | Ok(Event::Empty(tag)) if tag.name().as_ref() == b"tileset" => {
                tile_width = Some(required_u32(
                    &tag,
                    b"tilewidth",
                    &path,
                    "tileset.tilewidth",
                )?);
                tile_height = Some(required_u32(
                    &tag,
                    b"tileheight",
                    &path,
                    "tileset.tileheight",
                )?);
                columns = Some(required_u32(&tag, b"columns", &path, "tileset.columns")?);
            }
            Ok(Event::Empty(tag)) if tag.name().as_ref() == b"image" => {
                image_source = Some(required_string(&tag, b"source", &path, "image.source")?);
                image_width = Some(required_u32(&tag, b"width", &path, "image.width")?);
                image_height = Some(required_u32(&tag, b"height", &path, "image.height")?);
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(issue(&path, "-", "xml", error.to_string())),
        }
        buf.clear();
    }
    let image_path = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(image_source.ok_or_else(|| issue(&path, "tileset", "image", "missing image"))?);
    let set = Tileset {
        first_gid,
        tile_width: tile_width.ok_or_else(|| issue(&path, "tileset", "tilewidth", "missing"))?,
        tile_height: tile_height.ok_or_else(|| issue(&path, "tileset", "tileheight", "missing"))?,
        columns: columns.ok_or_else(|| issue(&path, "tileset", "columns", "missing"))?,
        image_width: image_width
            .ok_or_else(|| issue(&path, "tileset", "image.width", "missing"))?,
        image_height: image_height
            .ok_or_else(|| issue(&path, "tileset", "image.height", "missing"))?,
        image_path,
    };
    if !set.image_path.is_file() {
        return Err(issue(
            &path,
            "tileset",
            "image",
            format!("missing referenced image {}", set.image_path.display()),
        ));
    }
    if set.tile_width == 0 || set.tile_height == 0 || set.columns == 0 {
        return Err(issue(&path, "tileset", "dimensions", "must be positive"));
    }
    Ok(set)
}

fn tile_source_rect(local_id: u32, tileset: &Tileset) -> [u32; 4] {
    [
        (local_id % tileset.columns) * tileset.tile_width,
        (local_id / tileset.columns) * tileset.tile_height,
        tileset.tile_width,
        tileset.tile_height,
    ]
}

fn validate_rect(rect: [u32; 4], tileset: &Tileset, path: &Path) -> Result<(), ContentError> {
    if rect[2] == 0
        || rect[3] == 0
        || rect[0].saturating_add(rect[2]) > tileset.image_width
        || rect[1].saturating_add(rect[3]) > tileset.image_height
    {
        return Err(issue(
            path,
            "WORLD_ART",
            "source_rect",
            "invalid source rectangle",
        ));
    }
    Ok(())
}

fn pixel_center_to_world(
    x: f32,
    y: f32,
    width: u32,
    height: u32,
    ppu: f32,
    origin: [f32; 2],
) -> [f32; 2] {
    [
        origin[0] + (x + width as f32 * 0.5) / ppu,
        origin[1] - (y - height as f32 * 0.5) / ppu,
    ]
}

fn pixel_size_to_world(width: u32, height: u32, ppu: f32) -> [f32; 2] {
    [width as f32 / ppu, height as f32 / ppu]
}

fn canonical_asset_path(base: &Path, path: &Path, source: &Path) -> Result<String, ContentError> {
    if !path.is_file() {
        return Err(issue(
            source,
            "image",
            "source",
            format!("missing referenced image {}", path.display()),
        ));
    }
    let graphic = base
        .ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == "Graphic"))
        .ok_or_else(|| {
            issue(
                source,
                "image",
                "source",
                "cannot determine Graphic asset root",
            )
        })?;
    path.strip_prefix(graphic)
        .map_err(|_| issue(source, "image", "source", "image must be under Graphic"))
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

fn attr(tag: &BytesStart<'_>, name: &[u8]) -> Result<Option<String>, ContentError> {
    tag.attributes()
        .with_checks(false)
        .find_map(|result| match result {
            Ok(value) if value.key.as_ref() == name => Some(
                value
                    .unescape_value()
                    .map(|value| value.into_owned())
                    .map_err(|error| {
                        issue(Path::new("tiled"), "-", "attribute", error.to_string())
                    }),
            ),
            _ => None,
        })
        .transpose()
}

fn required_string(
    tag: &BytesStart<'_>,
    name: &[u8],
    path: &Path,
    field: &str,
) -> Result<String, ContentError> {
    attr(tag, name)?.ok_or_else(|| issue(path, "-", field, "missing attribute"))
}

fn required_u32(
    tag: &BytesStart<'_>,
    name: &[u8],
    path: &Path,
    field: &str,
) -> Result<u32, ContentError> {
    required_string(tag, name, path, field)?
        .parse()
        .map_err(|_| issue(path, "-", field, "invalid u32"))
}

fn required_f32(
    tag: &BytesStart<'_>,
    name: &[u8],
    path: &Path,
    field: &str,
) -> Result<f32, ContentError> {
    let value: f32 = required_string(tag, name, path, field)?
        .parse()
        .map_err(|_| issue(path, "-", field, "invalid f32"))?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(issue(path, "-", field, "must be finite"))
    }
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

    #[test]
    fn gid_flags_are_separate_from_tile_identity() {
        let set = Tileset {
            first_gid: 1,
            tile_width: 360,
            tile_height: 240,
            columns: 6,
            image_width: 2160,
            image_height: 1200,
            image_path: PathBuf::from("unused"),
        };
        let error = resolve_gid(1 | GID_FLIP_HORIZONTAL, &[set], Path::new("map.tmx"))
            .expect_err("unsupported transform");
        assert!(error.to_string().contains("transforms"));
    }

    #[test]
    fn pixel_anchor_conversion_is_y_up_and_not_grid_snapped() {
        assert_eq!(
            pixel_center_to_world(777.333, 640.0, 360, 240, 100.0, TILED_WORLD_ORIGIN),
            [-14.42667, -5.2]
        );
        assert_eq!(pixel_size_to_world(360, 240, 100.0), [3.6, 2.4]);
    }

    #[test]
    fn real_w12_fixture_compiles_to_background_and_free_world_art() {
        let map = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Graphic/assets/maps/map1.tmx");
        let presentation =
            compile_tiled_map(&map, TILED_PIXELS_PER_WORLD_UNIT).expect("fixture compiles");
        assert_eq!(presentation.map_authored, "map.dev.footnote");
        assert_eq!(presentation.backgrounds.len(), 1);
        assert_eq!(presentation.world_art.len(), 1);
        assert_eq!(
            presentation.world_art[0].source_rect_px,
            [1080, 0, 360, 240]
        );
        assert!((presentation.world_art[0].position[0] + 14.42667).abs() < 1e-4);
    }
}
