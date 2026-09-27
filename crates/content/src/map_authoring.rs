//! Map sidecar identity. This module does not parse TMX.
//!
//! The sidecar is the canonical authoring identity. Compiled visual bounds are
//! a generated runtime projection. Gameplay authoring owns spawn, FOOTNOTE,
//! display name, and restore.

use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::error::{ContentError, ValidationIssue};
use crate::map_gameplay_authoring::{GameplayRestore, MapGameplayAuthoring};
use purgatory_common::{CONTENT_MAP_START, ContentId, ContentKind};

pub const MAP_AUTHORING_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapAuthoringSource {
    pub schema_version: u32,
    pub content_id: u32,
    pub id: String,
    pub visual_source: String,
    pub pixels_per_world_unit: f32,
}

/// Authored id implied by a map-block ContentId. `50001` is `map.map1`.
#[must_use]
pub fn canonical_map_authored_id(content_id: u32) -> Option<String> {
    if ContentId::from_raw(content_id).kind() != Some(ContentKind::Map) {
        return None;
    }
    Some(format!("map.map{}", content_id - CONTENT_MAP_START))
}

/// True when the catalog ledger records this map ContentId and authored id.
#[must_use]
pub fn catalog_allocates_map(catalog: &str, content_id: u32, authored_id: &str) -> bool {
    let Some(start) = catalog.find("### Maps") else {
        return false;
    };
    let rest = &catalog[start..];
    let end = rest[1..]
        .find("\n### ")
        .map(|index| index + 1)
        .unwrap_or(rest.len());
    let section = &rest[..end];
    let id_cell = format!("`{content_id}`");
    let label_cell = format!("`{authored_id}`");
    section.lines().any(|line| {
        let cells: Vec<&str> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        cells.len() >= 2 && cells[0] == id_cell && cells[1] == label_cell
    })
}

pub fn load_map_authoring(path: &Path) -> Result<MapAuthoringSource, ContentError> {
    let bytes = fs::read(path).map_err(|error| ContentError::from_io(path, &error))?;
    let source: MapAuthoringSource = serde_json::from_slice(&bytes)
        .map_err(|error| issue(path, "-", "json", error.to_string()))?;
    validate_authoring(path, &source)?;
    Ok(source)
}

pub(crate) fn validate_ppu(path: &Path, ppu: f32) -> Result<(), ContentError> {
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
    purgatory_common::validate_authored_id(&source.id)
        .map_err(|error| issue(path, &source.id, "id", format!("{error:?}")))?;
    let Some(expected) = canonical_map_authored_id(source.content_id) else {
        return Err(issue(
            path,
            &source.id,
            "content_id",
            "must be an allocated map ContentId in 50,000-59,999",
        ));
    };
    if source.id != expected {
        return Err(issue(
            path,
            &source.id,
            "id",
            format!("authored id does not match ContentId allocation {expected}"),
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

/// Deterministic runtime projection. Identity comes from the sidecar, bounds
/// from the TMX compiler, and spawn/name/restore from gameplay authoring.
pub fn serialize_runtime_map_projection(
    source: &MapAuthoringSource,
    world_bounds: [f32; 4],
    gameplay: &MapGameplayAuthoring,
) -> Result<Vec<u8>, ContentError> {
    let [min_x, min_y, max_x, max_y] = world_bounds;
    let debug_name = if gameplay.name.trim().is_empty() {
        source.id.as_str()
    } else {
        gameplay.name.trim()
    };
    let file = RuntimeMapProjection {
        schema_version: crate::schema::CONTENT_SCHEMA_VERSION,
        content_id: source.content_id,
        id: &source.id,
        debug_name,
        bounds: RuntimeBounds {
            min_x,
            max_x,
            min_y,
            max_y,
        },
        spawn_points: gameplay
            .spawn_points
            .iter()
            .map(|spawn| RuntimeSpawn {
                id: &spawn.id,
                position: spawn.position,
            })
            .collect(),
        restore: gameplay.restore.as_ref(),
        platforms: Vec::<()>::new(),
    };
    let mut bytes = serde_json::to_vec_pretty(&file)
        .map_err(|error| issue(Path::new("runtime-map"), "-", "json", error.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[derive(Serialize)]
struct RuntimeMapProjection<'a> {
    schema_version: u32,
    content_id: u32,
    id: &'a str,
    debug_name: &'a str,
    bounds: RuntimeBounds,
    spawn_points: Vec<RuntimeSpawn<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    restore: Option<&'a GameplayRestore>,
    platforms: Vec<()>,
}

#[derive(Serialize)]
struct RuntimeBounds {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

#[derive(Serialize)]
struct RuntimeSpawn<'a> {
    id: &'a str,
    position: [f32; 2],
}

fn issue(path: &Path, definition: &str, field: &str, reason: impl Into<String>) -> ContentError {
    ContentError::one(ValidationIssue::new(
        path.display().to_string(),
        definition,
        field,
        reason,
    ))
}
