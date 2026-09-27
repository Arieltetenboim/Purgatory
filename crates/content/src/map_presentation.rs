//! Runtime-safe canonical visual-map schema.
//!
//! These types contain only PURGATORY-owned presentation concepts. Tiled parsing
//! and authoring-source handling live behind the `map-authoring` feature.

use serde::{Deserialize, Serialize};

pub const MAP_PRESENTATION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MapPresentation {
    pub schema_version: u32,
    pub map_authored: String,
    pub visual_extent_px: [u32; 2],
    pub pixels_per_world_unit: f32,
    /// Map-local Y-up bounds: `[min_x, min_y, max_x, max_y]`.
    pub world_bounds: [f32; 4],
    pub assets: Vec<PresentationAsset>,
    /// Back-to-front authored visual order.
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
    /// Tiled orthogonal anti-diagonal flip, normalized at compile time.
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
