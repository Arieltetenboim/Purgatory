//! Thin Map Lab document boundary over the shared map compiler.

use std::path::{Path, PathBuf};

use purgatory_content::{
    CANONICAL_MAP_TILE_PX, LoadMode, MAP_AUTHORING_SCHEMA_VERSION,
    MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION, MAP_GAMEPLAY_AUTHORING_SCHEMA_VERSION,
    MIN_MAP_HEIGHT_WU, MIN_MAP_WIDTH_WU, MapAuthoringSource, MapEnvironmentAuthoring,
    MapGameplayAuthoring, MapPresentation, Placement, PlacementKind, compile_tiled_map_with_ppu,
    default_content_root, load_map_authoring, load_placement_file, load_registry,
    resolve_png_asset_folder, serialize_map_pretty, serialize_placements_v2,
    serialize_runtime_map_projection, validate_canonical_map_grid, validate_cloud_field,
};

/// Production visual-scale standard for ordinary PURGATORY maps.
///
/// The per-map sidecar keeps PPU explicit, but normal authored maps should use
/// this value. Camera zoom is a separate presentation concern.
pub const PURGATORY_STANDARD_PPU: f32 = 100.0;

/// Guard against an accidental huge TMX. Not a gameplay size contract.
const MAX_NEW_MAP_TILES: u32 = 4_096;

/// Smallest tile count whose world size meets the one-viewport map minimum.
#[must_use]
pub fn minimum_new_map_tiles() -> (u32, u32) {
    let tile = CANONICAL_MAP_TILE_PX as f32;
    let width = (MIN_MAP_WIDTH_WU * PURGATORY_STANDARD_PPU / tile).ceil() as u32;
    let height = (MIN_MAP_HEIGHT_WU * PURGATORY_STANDARD_PPU / tile).ceil() as u32;
    (width.max(1), height.max(1))
}

#[derive(Clone, Debug)]
pub struct MapLabDocument {
    pub sidecar_path: PathBuf,
    pub gameplay_path: PathBuf,
    pub environment_path: PathBuf,
    pub placements_path: PathBuf,
    pub source: MapAuthoringSource,
    pub presentation: MapPresentation,
    pub gameplay: MapGameplayAuthoring,
    pub environment: MapEnvironmentAuthoring,
    pub placements: Vec<Placement>,
}

impl MapLabDocument {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let source = load_map_authoring(path).map_err(|error| error.to_string())?;
        validate_world_scale(&source)?;
        let tmx_path = tmx_path_for(path, &source)?;
        validate_canonical_map_grid(&tmx_path).map_err(|error| error.to_string())?;
        let presentation = compile_tiled_map_with_ppu(path, &source, source.pixels_per_world_unit)
            .map_err(|error| error.to_string())?;
        let gameplay_path = gameplay_path_for(path)?;
        let environment_path = environment_path_for(path)?;
        let placements_path = placements_path_for(path, &source.id)?;
        let gameplay = if gameplay_path.is_file() {
            let bytes = std::fs::read(&gameplay_path)
                .map_err(|error| format!("read {}: {error}", gameplay_path.display()))?;
            let mut gameplay: MapGameplayAuthoring = serde_json::from_slice(&bytes)
                .map_err(|error| format!("parse {}: {error}", gameplay_path.display()))?;
            if gameplay.name.trim().is_empty() {
                gameplay.name = source.id.clone();
            }
            validate_gameplay(&gameplay_path, &source, &presentation, &gameplay, false)?;
            gameplay
        } else {
            MapGameplayAuthoring::empty(source.id.clone())
        };
        let placements = if placements_path.is_file() {
            let (map_authored, placements) =
                load_placement_file(&placements_path).map_err(|error| error.to_string())?;
            if map_authored != source.id {
                return Err(format!(
                    "{}: placement map {} does not match source {}",
                    placements_path.display(),
                    map_authored,
                    source.id
                ));
            }
            placements
        } else {
            Vec::new()
        };
        let environment = if environment_path.is_file() {
            let bytes = std::fs::read(&environment_path)
                .map_err(|error| format!("read {}: {error}", environment_path.display()))?;
            let environment: MapEnvironmentAuthoring = serde_json::from_slice(&bytes)
                .map_err(|error| format!("parse {}: {error}", environment_path.display()))?;
            validate_environment(&environment_path, &source, &environment)?;
            environment
        } else {
            MapEnvironmentAuthoring::empty(source.id.clone())
        };
        write_runtime_projection(path, &source, &presentation, &gameplay)?;
        Ok(Self {
            sidecar_path: path.to_path_buf(),
            gameplay_path,
            environment_path,
            placements_path,
            source,
            presentation,
            gameplay,
            environment,
            placements,
        })
    }

    pub fn recompile(&mut self, pixels_per_world_unit: f32) -> Result<(), String> {
        let tmx_path = tmx_path_for(&self.sidecar_path, &self.source)?;
        validate_canonical_map_grid(&tmx_path).map_err(|error| error.to_string())?;
        self.presentation =
            compile_tiled_map_with_ppu(&self.sidecar_path, &self.source, pixels_per_world_unit)
                .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Authored positions that sit outside the current visual bounds.
    ///
    /// Reload reports these and leaves the files unchanged. It does not clamp,
    /// delete, or move them.
    #[must_use]
    pub fn bounds_issues(&self) -> Vec<BoundsIssue> {
        let bounds = self.presentation.world_bounds;
        let mut issues = Vec::new();
        for foothold in &self.gameplay.foothold_paths {
            if foothold
                .points
                .iter()
                .any(|point| outside_bounds(*point, bounds))
            {
                issues.push(BoundsIssue {
                    kind: "FOOTNOTE",
                    id: foothold.id.clone(),
                });
            }
        }
        for spawn in &self.gameplay.spawn_points {
            if outside_bounds(spawn.position, bounds) {
                issues.push(BoundsIssue {
                    kind: "spawn",
                    id: spawn.id.clone(),
                });
            }
        }
        for placement in &self.placements {
            if outside_bounds(placement.position, bounds) {
                issues.push(BoundsIssue {
                    kind: placement_kind_label(placement),
                    id: placement.id.clone(),
                });
            }
        }
        issues
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, String> {
        serialize_map_pretty(&self.presentation).map_err(|error| error.to_string())
    }

    #[must_use]
    pub fn snap_spawn_to_foothold(&self, approximate_center: [f32; 2]) -> Option<[f32; 2]> {
        let half_height = purgatory_simulation::PLAYER_HALF_EXTENTS[1];
        self.gameplay
            .foothold_paths
            .iter()
            .flat_map(|path| path.points.windows(2))
            .filter_map(|pair| segment_y_at_x(pair[0], pair[1], approximate_center[0]))
            .map(|surface_y| {
                let center_y = surface_y + half_height;
                (
                    (center_y - approximate_center[1]).abs(),
                    [approximate_center[0], center_y],
                )
            })
            .filter(|(distance, _)| *distance <= 2.0)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, position)| position)
    }

    pub fn gameplay_readiness(&self) -> Result<(), String> {
        if self.gameplay.foothold_paths.is_empty() {
            return Err("map requires at least one FOOTNOTE path".to_owned());
        }
        let Some(default) = self
            .gameplay
            .spawn_points
            .iter()
            .find(|spawn| spawn.id == "default")
        else {
            return Err("map requires a default spawn point".to_owned());
        };
        if !spawn_is_supported(&self.gameplay, default.position) {
            return Err("default spawn must align to a FOOTNOTE surface".to_owned());
        }
        Ok(())
    }

    pub fn save_environment(&self) -> Result<(), String> {
        validate_environment(&self.environment_path, &self.source, &self.environment)?;
        let parent = self
            .environment_path
            .parent()
            .ok_or_else(|| "invalid environment authoring path".to_owned())?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
        let mut bytes = serde_json::to_vec_pretty(&self.environment)
            .map_err(|error| format!("serialize environment authoring: {error}"))?;
        bytes.push(b'\n');
        std::fs::write(&self.environment_path, bytes)
            .map_err(|error| format!("write {}: {error}", self.environment_path.display()))
    }

    pub fn save_placements(&self) -> Result<(), String> {
        let registry = load_registry(&default_content_root(), LoadMode::Full)
            .map_err(|error| error.to_string())?;
        registry
            .validate_placements(&self.source.id, &self.placements)
            .map_err(|error| error.to_string())?;
        let parent = self
            .placements_path
            .parent()
            .ok_or_else(|| "invalid placements path".to_owned())?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
        let bytes = serialize_placements_v2(&self.source.id, &self.placements)
            .map_err(|error| error.to_string())?;
        std::fs::write(&self.placements_path, bytes)
            .map_err(|error| format!("write {}: {error}", self.placements_path.display()))
    }

    pub fn discover_directory(&self) -> Option<&Path> {
        self.sidecar_path.parent()
    }

    pub fn save_gameplay(&self) -> Result<(), String> {
        validate_gameplay(
            &self.gameplay_path,
            &self.source,
            &self.presentation,
            &self.gameplay,
            true,
        )?;
        let parent = self
            .gameplay_path
            .parent()
            .ok_or_else(|| "invalid gameplay authoring path".to_owned())?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
        let mut bytes = serde_json::to_vec_pretty(&self.gameplay)
            .map_err(|error| format!("serialize gameplay authoring: {error}"))?;
        bytes.push(b'\n');
        std::fs::write(&self.gameplay_path, bytes)
            .map_err(|error| format!("write {}: {error}", self.gameplay_path.display()))?;
        write_runtime_projection(
            &self.sidecar_path,
            &self.source,
            &self.presentation,
            &self.gameplay,
        )?;
        Ok(())
    }
}

/// Sidecars in the canonical map authoring directory.
///
/// Map Lab opens one of these at a time through [`MapLabDocument::open`].
pub fn discover_authored_maps(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut maps = Vec::new();
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read {}: {error}", directory.display()))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.is_file() && name.ends_with(".purgatory-map.json") {
            maps.push(path);
        }
    }
    maps.sort();
    Ok(maps)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundsIssue {
    pub kind: &'static str,
    pub id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewMapRequest {
    pub display_name: String,
    pub width_tiles: u32,
    pub height_tiles: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewMapCreated {
    pub sidecar_path: PathBuf,
    pub tmx_path: PathBuf,
    pub content_id: u32,
    pub authored_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapImportCandidate {
    pub tmx_path: PathBuf,
    pub content_id: u32,
    pub authored_id: String,
}

fn repository_root_from_authoring(authoring_directory: &Path) -> Result<PathBuf, String> {
    authoring_directory
        .ancestors()
        .find(|candidate| {
            candidate
                .join("Graphic")
                .join("assets")
                .join("maps")
                .is_dir()
                && candidate
                    .join("content")
                    .join("CONTENT_ID_CATALOG.md")
                    .is_file()
        })
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            format!(
                "cannot locate repository root above {}",
                authoring_directory.display()
            )
        })
}

pub fn discover_unimported_tmx(
    authoring_directory: &Path,
) -> Result<Vec<MapImportCandidate>, String> {
    let repository_root = repository_root_from_authoring(authoring_directory)?;
    let graphic_maps = repository_root.join("Graphic").join("assets").join("maps");
    let mut imported = std::collections::HashSet::new();
    for sidecar in discover_authored_maps(authoring_directory)? {
        let source = load_map_authoring(&sidecar).map_err(|error| error.to_string())?;
        imported.insert(source.content_id);
    }

    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(&graphic_maps)
        .map_err(|error| format!("read {}: {error}", graphic_maps.display()))?
    {
        let path = entry
            .map_err(|error| format!("read {}: {error}", graphic_maps.display()))?
            .path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("tmx") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Ok(content_id) = stem.parse::<u32>() else {
            continue;
        };
        if !(purgatory_common::CONTENT_MAP_START..=purgatory_common::CONTENT_MAP_END)
            .contains(&content_id)
            || imported.contains(&content_id)
        {
            continue;
        }
        candidates.push(MapImportCandidate {
            tmx_path: path,
            content_id,
            authored_id: format!(
                "map.map{}",
                content_id - purgatory_common::CONTENT_MAP_START
            ),
        });
    }
    candidates.sort_by_key(|candidate| candidate.content_id);
    Ok(candidates)
}

/// Next unused map ContentId from the catalog ledger.
///
/// Allocation walks forward from the highest recorded map ID, including
/// retired rows. Gaps are not filled.
pub fn allocate_next_map_id(catalog: &str) -> Result<(u32, String), String> {
    let rows = catalog_map_rows(map_catalog_section(catalog)?)?;
    let mut used = std::collections::BTreeSet::new();
    let mut labels = std::collections::BTreeSet::new();
    for (id, label) in rows {
        if !(purgatory_common::CONTENT_MAP_START..=purgatory_common::CONTENT_MAP_END).contains(&id)
        {
            return Err(format!("catalog id {id} is outside the map range"));
        }
        if !used.insert(id) {
            return Err(format!("ContentId {id} is allocated more than once"));
        }
        if !labels.insert(label) {
            return Err("a map label is allocated more than once".to_owned());
        }
    }
    let next = used
        .iter()
        .next_back()
        .map(|id| id.saturating_add(1))
        .unwrap_or(purgatory_common::CONTENT_MAP_START + 1)
        .max(purgatory_common::CONTENT_MAP_START + 1);
    if next > purgatory_common::CONTENT_MAP_END || used.contains(&next) {
        return Err("map ContentId range is exhausted".to_owned());
    }
    let authored = authored_map_id(next)?;
    if labels.contains(&authored) {
        return Err(format!("authored id {authored} is already allocated"));
    }
    Ok((next, authored))
}

pub fn authored_map_id(content_id: u32) -> Result<String, String> {
    if !(purgatory_common::CONTENT_MAP_START..=purgatory_common::CONTENT_MAP_END)
        .contains(&content_id)
    {
        return Err(format!(
            "map ContentId must be in {}-{}",
            purgatory_common::CONTENT_MAP_START,
            purgatory_common::CONTENT_MAP_END
        ));
    }
    Ok(format!(
        "map.map{}",
        content_id - purgatory_common::CONTENT_MAP_START
    ))
}

pub fn preview_next_map_allocation(authoring_directory: &Path) -> Result<(u32, String), String> {
    let repository_root = repository_root_from_authoring(authoring_directory)?;
    let catalog = std::fs::read_to_string(catalog_path(&repository_root))
        .map_err(|error| format!("read catalog: {error}"))?;
    allocate_next_map_id(&catalog)
}

/// Canonical empty TMX for a new map. Paths stay project-relative; no tileset
/// is embedded unless a canonical default tileset exists.
#[must_use]
pub fn canonical_tmx_document(width_tiles: u32, height_tiles: u32) -> String {
    let csv = canonical_tile_csv(width_tiles, height_tiles);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" tiledversion="1.12.2" orientation="orthogonal" renderorder="right-down" width="{width_tiles}" height="{height_tiles}" tilewidth="{tile}" tileheight="{tile}" infinite="0" nextlayerid="2" nextobjectid="1">
 <layer id="1" name="Tile Layer 1" width="{width_tiles}" height="{height_tiles}">
  <data encoding="csv">
{csv}
</data>
 </layer>
</map>
"#,
        tile = CANONICAL_MAP_TILE_PX,
    )
}

pub fn create_new_map(
    authoring_directory: &Path,
    request: &NewMapRequest,
) -> Result<NewMapCreated, String> {
    let display_name = validate_display_name(&request.display_name)?;
    validate_new_map_dimensions(request.width_tiles, request.height_tiles)?;
    let repository_root = repository_root_from_authoring(authoring_directory)?;
    let catalog_file = catalog_path(&repository_root);
    let catalog = std::fs::read_to_string(&catalog_file)
        .map_err(|error| format!("read {}: {error}", catalog_file.display()))?;
    let (content_id, authored_id) = allocate_next_map_id(&catalog)?;

    for sidecar in discover_authored_maps(authoring_directory)? {
        let source = load_map_authoring(&sidecar).map_err(|error| error.to_string())?;
        if source.content_id == content_id || source.id == authored_id {
            return Err(format!(
                "{} already exists for ContentId {content_id}",
                sidecar.display()
            ));
        }
    }

    let tmx_name = format!("{content_id}.tmx");
    let tmx_path = repository_root
        .join("Graphic")
        .join("assets")
        .join("maps")
        .join(&tmx_name);
    let sidecar_path = authoring_directory.join(format!("{authored_id}.purgatory-map.json"));
    let gameplay_path = gameplay_path_for(&sidecar_path)?;
    let environment_path = environment_path_for(&sidecar_path)?;
    let placements_path = placements_path_for(&sidecar_path, &authored_id)?;
    for path in [
        &tmx_path,
        &sidecar_path,
        &gameplay_path,
        &environment_path,
        &placements_path,
    ] {
        if path.exists() {
            return Err(format!("{} already exists", path.display()));
        }
    }

    let source = MapAuthoringSource {
        schema_version: MAP_AUTHORING_SCHEMA_VERSION,
        content_id,
        id: authored_id.clone(),
        visual_source: format!("../../../Graphic/assets/maps/{tmx_name}"),
        pixels_per_world_unit: PURGATORY_STANDARD_PPU,
    };
    let mut gameplay = MapGameplayAuthoring::empty(authored_id.clone());
    gameplay.name = display_name;
    let environment = MapEnvironmentAuthoring::empty(authored_id.clone());
    let placements = serialize_placements_v2(&authored_id, &[])
        .map_err(|error| format!("serialize placements: {error}"))?;

    let mut created = Vec::new();
    let write_result = (|| {
        write_new_file(
            &tmx_path,
            canonical_tmx_document(request.width_tiles, request.height_tiles).as_bytes(),
            &mut created,
        )?;
        write_new_file(
            &sidecar_path,
            &pretty_json(serde_json::to_vec_pretty(&source))?,
            &mut created,
        )?;
        write_new_file(
            &gameplay_path,
            &pretty_json(serde_json::to_vec_pretty(&gameplay))?,
            &mut created,
        )?;
        write_new_file(
            &environment_path,
            &pretty_json(serde_json::to_vec_pretty(&environment))?,
            &mut created,
        )?;
        write_new_file(&placements_path, &placements, &mut created)?;
        let projection = runtime_map_path_for(&sidecar_path, &authored_id)?;
        created.push(projection);
        MapLabDocument::open(&sidecar_path)?;
        record_map_allocation(&repository_root, content_id, &authored_id, false)
    })();
    if let Err(error) = write_result {
        for path in created.iter().rev() {
            let _ = std::fs::remove_file(path);
        }
        return Err(error);
    }
    Ok(NewMapCreated {
        sidecar_path,
        tmx_path,
        content_id,
        authored_id,
    })
}

fn catalog_path(repository_root: &Path) -> PathBuf {
    repository_root
        .join("content")
        .join("CONTENT_ID_CATALOG.md")
}

fn map_catalog_section(catalog: &str) -> Result<&str, String> {
    let start = catalog
        .find("### Maps")
        .ok_or_else(|| "CONTENT_ID_CATALOG.md: Maps section not found".to_owned())?;
    let rest = &catalog[start..];
    let end = rest[1..]
        .find("\n### ")
        .map(|index| index + 1)
        .unwrap_or(rest.len());
    Ok(&rest[..end])
}

fn catalog_map_rows(section: &str) -> Result<Vec<(u32, String)>, String> {
    let mut rows = Vec::new();
    for line in section.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 2 {
            continue;
        }
        let Ok(id) = cells[0].trim_matches('`').parse::<u32>() else {
            continue;
        };
        let label = cells[1].trim_matches('`').to_owned();
        if label.is_empty() {
            return Err(format!("catalog map {id} is missing a label"));
        }
        rows.push((id, label));
    }
    Ok(rows)
}

fn authored_map_label_matches(content_id: u32, authored_id: &str) -> bool {
    authored_map_id(content_id).is_ok_and(|expected| expected == authored_id)
}

fn validate_display_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("map name is required".to_owned());
    }
    if name.chars().count() > 80 {
        return Err("map name must be at most 80 characters".to_owned());
    }
    if name.chars().any(char::is_control) {
        return Err("map name must not contain control characters".to_owned());
    }
    Ok(name.to_owned())
}

fn validate_new_map_dimensions(width_tiles: u32, height_tiles: u32) -> Result<(), String> {
    if width_tiles == 0 || height_tiles == 0 {
        return Err("map width and height must be positive".to_owned());
    }
    if width_tiles > MAX_NEW_MAP_TILES || height_tiles > MAX_NEW_MAP_TILES {
        return Err(format!(
            "map width and height must be at most {MAX_NEW_MAP_TILES} tiles"
        ));
    }
    let tile = CANONICAL_MAP_TILE_PX as f32;
    let world_width = width_tiles as f32 * tile / PURGATORY_STANDARD_PPU;
    let world_height = height_tiles as f32 * tile / PURGATORY_STANDARD_PPU;
    if world_width + f32::EPSILON < MIN_MAP_WIDTH_WU
        || world_height + f32::EPSILON < MIN_MAP_HEIGHT_WU
    {
        let (min_width, min_height) = minimum_new_map_tiles();
        return Err(format!(
            "map is {world_width:.3} × {world_height:.3} wu; minimum is {min_width}×{min_height} tiles ({MIN_MAP_WIDTH_WU:.3} × {MIN_MAP_HEIGHT_WU:.3} wu)"
        ));
    }
    Ok(())
}

fn canonical_tile_csv(width_tiles: u32, height_tiles: u32) -> String {
    let mut csv = String::new();
    for y in 0..height_tiles {
        for x in 0..width_tiles {
            csv.push('0');
            let last_cell = x + 1 == width_tiles && y + 1 == height_tiles;
            if !last_cell {
                csv.push(',');
            }
        }
        if y + 1 != height_tiles {
            csv.push('\n');
        }
    }
    csv
}

fn pretty_json(bytes: Result<Vec<u8>, serde_json::Error>) -> Result<Vec<u8>, String> {
    let mut bytes = bytes.map_err(|error| format!("serialize json: {error}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write_new_file(path: &Path, bytes: &[u8], created: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    created.push(path.to_path_buf());
    std::fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

fn record_map_allocation(
    repository_root: &Path,
    content_id: u32,
    authored_id: &str,
    allow_existing_match: bool,
) -> Result<(), String> {
    let catalog = repository_root
        .join("content")
        .join("CONTENT_ID_CATALOG.md");
    let mut text = std::fs::read_to_string(&catalog)
        .map_err(|error| format!("read {}: {error}", catalog.display()))?;
    let id_cell = format!("| `{content_id}` |");
    if text.contains(&id_cell) {
        let expected = format!("| `{content_id}` | `{authored_id}` | active |");
        return if allow_existing_match && text.contains(&expected) {
            Ok(())
        } else {
            Err(format!("ContentId {content_id} is already allocated"))
        };
    }
    if !authored_map_label_matches(content_id, authored_id) {
        return Err(format!(
            "authored id {authored_id} does not match ContentId {content_id}"
        ));
    }
    let insert_at = map_catalog_insert_index(&text)
        .map_err(|error| format!("{}: {error}", catalog.display()))?;
    let newline = catalog_line_ending(&text);
    let mut row = format!("| `{content_id}` | `{authored_id}` | active |{newline}");
    if !text[..insert_at].ends_with(newline) {
        row.insert_str(0, newline);
    }
    text.insert_str(insert_at, &row);
    std::fs::write(&catalog, text).map_err(|error| format!("write {}: {error}", catalog.display()))
}

fn catalog_line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Byte index where a new map row belongs: after the Maps section rows and
/// before the blank line that introduces the next heading.
fn map_catalog_insert_index(text: &str) -> Result<usize, String> {
    let start = text
        .find("### Maps")
        .ok_or_else(|| "CONTENT_ID_CATALOG.md: Maps section not found".to_owned())?;
    let section = map_catalog_section(text)?;
    let mut insert_at = start + section.len();
    // `map_catalog_section` ends on `\n### `, so a CRLF boundary leaves the
    // preceding `\r` inside the section. Keep that line ending intact.
    if text.as_bytes().get(insert_at) == Some(&b'\n')
        && text.as_bytes().get(insert_at.wrapping_sub(1)) == Some(&b'\r')
    {
        insert_at -= 1;
    }
    Ok(insert_at)
}

pub fn import_numeric_tmx(
    authoring_directory: &Path,
    candidate: &MapImportCandidate,
) -> Result<PathBuf, String> {
    let repository_root = repository_root_from_authoring(authoring_directory)?;
    let expected_maps = repository_root.join("Graphic").join("assets").join("maps");
    if candidate.tmx_path.parent() != Some(expected_maps.as_path()) {
        return Err("TMX import must come from Graphic/assets/maps".to_owned());
    }
    let expected_name = format!("{}.tmx", candidate.content_id);
    if candidate
        .tmx_path
        .file_name()
        .and_then(|name| name.to_str())
        != Some(expected_name.as_str())
    {
        return Err("TMX filename must equal its numeric ContentId".to_owned());
    }
    if !(purgatory_common::CONTENT_MAP_START..=purgatory_common::CONTENT_MAP_END)
        .contains(&candidate.content_id)
    {
        return Err("map ContentId must be in 50,000-59,999".to_owned());
    }

    let sidecar = authoring_directory.join(format!("{}.purgatory-map.json", candidate.authored_id));
    if sidecar.exists() {
        return Err(format!("{} already exists", sidecar.display()));
    }
    let source = MapAuthoringSource {
        schema_version: MAP_AUTHORING_SCHEMA_VERSION,
        content_id: candidate.content_id,
        id: candidate.authored_id.clone(),
        visual_source: format!("../../../Graphic/assets/maps/{}", expected_name),
        pixels_per_world_unit: PURGATORY_STANDARD_PPU,
    };
    let mut bytes = serde_json::to_vec_pretty(&source)
        .map_err(|error| format!("serialize map sidecar: {error}"))?;
    bytes.push(b'\n');
    std::fs::write(&sidecar, bytes)
        .map_err(|error| format!("write {}: {error}", sidecar.display()))?;
    if let Err(error) = record_map_allocation(
        &repository_root,
        candidate.content_id,
        &candidate.authored_id,
        true,
    ) {
        let _ = std::fs::remove_file(&sidecar);
        return Err(error);
    }
    Ok(sidecar)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MapSwitchKind {
    AlreadyOpen,
    Open,
    Confirm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MapSwitchChoice {
    SaveAndSwitch,
    DiscardAndSwitch,
    Cancel,
}

#[must_use]
pub fn classify_map_switch(same_document: bool, reload: bool, dirty: bool) -> MapSwitchKind {
    if same_document && !reload {
        MapSwitchKind::AlreadyOpen
    } else if dirty {
        MapSwitchKind::Confirm
    } else {
        MapSwitchKind::Open
    }
}

pub fn apply_map_switch_choice<E>(
    choice: MapSwitchChoice,
    save: impl FnOnce() -> Result<(), E>,
) -> Result<bool, E> {
    match choice {
        MapSwitchChoice::Cancel => Ok(false),
        MapSwitchChoice::DiscardAndSwitch => Ok(true),
        MapSwitchChoice::SaveAndSwitch => save().map(|()| true),
    }
}

fn placements_path_for(sidecar: &Path, map_authored: &str) -> Result<PathBuf, String> {
    let content_root = sidecar
        .ancestors()
        .find(|ancestor| ancestor.join("server").is_dir() && ancestor.join("shared").is_dir())
        .ok_or_else(|| format!("cannot locate content/ root above {}", sidecar.display()))?;
    Ok(content_root
        .join("server")
        .join("placements")
        .join(format!("{map_authored}.json")))
}

fn environment_path_for(sidecar: &Path) -> Result<PathBuf, String> {
    let name = sidecar
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid sidecar filename: {}", sidecar.display()))?;
    let stem = name
        .strip_suffix(".purgatory-map.json")
        .ok_or_else(|| format!("sidecar must end in .purgatory-map.json: {name}"))?;
    Ok(sidecar.with_file_name(format!("{stem}.environment.json")))
}

fn validate_environment(
    path: &Path,
    source: &MapAuthoringSource,
    environment: &MapEnvironmentAuthoring,
) -> Result<(), String> {
    if environment.schema_version != MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION {
        return Err(format!(
            "{}: unsupported environment schema {} (want {})",
            path.display(),
            environment.schema_version,
            MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION
        ));
    }
    if environment.map_authored != source.id {
        return Err(format!(
            "{}: environment map {} does not match source {}",
            path.display(),
            environment.map_authored,
            source.id
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for (kind, layer) in environment
        .parallax_layers
        .iter()
        .map(|layer| ("parallax", layer))
        .chain(
            environment
                .foreground_layers
                .iter()
                .map(|layer| ("foreground", layer)),
        )
    {
        if layer.id.trim().is_empty() || !ids.insert(layer.id.as_str()) {
            return Err(format!(
                "{}: environment layer ids must be non-empty and unique",
                path.display()
            ));
        }
        let asset = std::path::Path::new(&layer.asset_path);
        if layer.asset_path.trim().is_empty()
            || asset.is_absolute()
            || asset
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(format!(
                "{}: {kind} layer {} asset_path must be a Graphic-relative path without '..'",
                path.display(),
                layer.id
            ));
        }
        if !layer.parallax.is_finite() || !(0.0..=1.0).contains(&layer.parallax) {
            return Err(format!(
                "{}: {kind} layer {} parallax must be within 0..=1",
                path.display(),
                layer.id
            ));
        }
        if !layer.opacity.is_finite() || !(0.0..=1.0).contains(&layer.opacity) {
            return Err(format!(
                "{}: {kind} layer {} opacity must be within 0..=1",
                path.display(),
                layer.id
            ));
        }
        if !layer.offset_world.iter().all(|value| value.is_finite()) {
            return Err(format!(
                "{}: {kind} layer {} offset must be finite",
                path.display(),
                layer.id
            ));
        }
        if !layer
            .motion_world_per_second
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(format!(
                "{}: {kind} layer {} motion must be finite",
                path.display(),
                layer.id
            ));
        }
    }
    if !environment.cloud_fields.is_empty() {
        let graphic = path
            .ancestors()
            .map(|ancestor| ancestor.join("Graphic"))
            .find(|candidate| candidate.is_dir())
            .ok_or_else(|| format!("cannot locate Graphic/ above {}", path.display()))?;
        for field in &environment.cloud_fields {
            validate_cloud_field(field).map_err(|error| format!("{}: {error}", path.display()))?;
            if !ids.insert(field.id.as_str()) {
                return Err(format!(
                    "{}: environment layer id {} is duplicated",
                    path.display(),
                    field.id
                ));
            }
            resolve_png_asset_folder(&graphic, &field.asset_folder)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
    }
    Ok(())
}

fn gameplay_path_for(sidecar: &Path) -> Result<PathBuf, String> {
    let name = sidecar
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid sidecar filename: {}", sidecar.display()))?;
    let stem = name
        .strip_suffix(".purgatory-map.json")
        .ok_or_else(|| format!("sidecar must end in .purgatory-map.json: {name}"))?;
    Ok(sidecar.with_file_name(format!("{stem}.gameplay.json")))
}

fn runtime_map_path_for(sidecar: &Path, map_authored: &str) -> Result<PathBuf, String> {
    let maps_dir = sidecar
        .parent()
        .ok_or_else(|| format!("invalid sidecar path: {}", sidecar.display()))?;
    let authoring_dir = maps_dir
        .parent()
        .ok_or_else(|| format!("invalid authoring maps path: {}", maps_dir.display()))?;
    let content_root = authoring_dir.parent().ok_or_else(|| {
        format!(
            "invalid content authoring path: {}",
            authoring_dir.display()
        )
    })?;
    Ok(content_root
        .join("shared")
        .join("maps")
        .join(format!("{map_authored}.json")))
}

fn write_runtime_projection(
    sidecar: &Path,
    source: &MapAuthoringSource,
    presentation: &MapPresentation,
    gameplay: &MapGameplayAuthoring,
) -> Result<bool, String> {
    let runtime_path = runtime_map_path_for(sidecar, &source.id)?;
    let bytes = serialize_runtime_map_projection(source, presentation.world_bounds, gameplay)
        .map_err(|error| error.to_string())?;
    if runtime_path.is_file()
        && std::fs::read(&runtime_path).ok().as_deref() == Some(bytes.as_slice())
    {
        return Ok(false);
    }
    if let Some(parent) = runtime_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    std::fs::write(&runtime_path, bytes)
        .map_err(|error| format!("write {}: {error}", runtime_path.display()))?;
    Ok(true)
}

fn validate_world_scale(source: &MapAuthoringSource) -> Result<(), String> {
    if (source.pixels_per_world_unit - PURGATORY_STANDARD_PPU).abs() > 1.0e-4 {
        return Err(format!(
            "pixels_per_world_unit {} does not match the canonical {} px/wu world scale; world-scale migration is not supported",
            source.pixels_per_world_unit, PURGATORY_STANDARD_PPU
        ));
    }
    Ok(())
}

fn tmx_path_for(sidecar: &Path, source: &MapAuthoringSource) -> Result<PathBuf, String> {
    let parent = sidecar
        .parent()
        .ok_or_else(|| format!("invalid sidecar path: {}", sidecar.display()))?;
    Ok(parent.join(&source.visual_source))
}

fn outside_bounds(point: [f32; 2], bounds: [f32; 4]) -> bool {
    let [min_x, min_y, max_x, max_y] = bounds;
    !point[0].is_finite()
        || !point[1].is_finite()
        || point[0] < min_x
        || point[0] > max_x
        || point[1] < min_y
        || point[1] > max_y
}

fn placement_kind_label(placement: &Placement) -> &'static str {
    match placement.kind {
        PlacementKind::Portal => "portal",
        PlacementKind::Monster => "monster",
        PlacementKind::Entity if placement.content_authored.starts_with("npc.") => "NPC",
        PlacementKind::Entity => "entity",
    }
}

fn validate_gameplay(
    path: &Path,
    source: &MapAuthoringSource,
    presentation: &MapPresentation,
    gameplay: &MapGameplayAuthoring,
    enforce_bounds: bool,
) -> Result<(), String> {
    if gameplay.schema_version != MAP_GAMEPLAY_AUTHORING_SCHEMA_VERSION {
        return Err(format!(
            "{}: unsupported gameplay schema {} (want {})",
            path.display(),
            gameplay.schema_version,
            MAP_GAMEPLAY_AUTHORING_SCHEMA_VERSION
        ));
    }
    if gameplay.map_authored != source.id {
        return Err(format!(
            "{}: gameplay map {} does not match source {}",
            path.display(),
            gameplay.map_authored,
            source.id
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for foothold in &gameplay.foothold_paths {
        if foothold.id.trim().is_empty() || !ids.insert(foothold.id.as_str()) {
            return Err(format!(
                "{}: foothold path ids must be non-empty and unique",
                path.display()
            ));
        }
        if foothold.points.len() < 2 {
            return Err(format!(
                "{}: foothold {} needs at least two points",
                path.display(),
                foothold.id
            ));
        }
        if foothold
            .points
            .iter()
            .any(|point| !point[0].is_finite() || !point[1].is_finite())
        {
            return Err(format!(
                "{}: foothold {} contains non-finite coordinates",
                path.display(),
                foothold.id
            ));
        }
        if enforce_bounds
            && foothold
                .points
                .iter()
                .any(|point| outside_bounds(*point, presentation.world_bounds))
        {
            return Err(format!(
                "{}: foothold {} leaves map bounds",
                path.display(),
                foothold.id
            ));
        }
        if foothold.points.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(format!(
                "{}: foothold {} contains a zero-length segment",
                path.display(),
                foothold.id
            ));
        }
    }
    let mut spawn_ids = std::collections::HashSet::new();
    for spawn in &gameplay.spawn_points {
        if spawn.id.trim().is_empty() || !spawn_ids.insert(spawn.id.as_str()) {
            return Err(format!(
                "{}: spawn point ids must be non-empty and unique",
                path.display()
            ));
        }
        if !spawn.position[0].is_finite() || !spawn.position[1].is_finite() {
            return Err(format!(
                "{}: spawn {} contains non-finite coordinates",
                path.display(),
                spawn.id
            ));
        }
        if enforce_bounds && outside_bounds(spawn.position, presentation.world_bounds) {
            return Err(format!(
                "{}: spawn {} leaves map bounds",
                path.display(),
                spawn.id
            ));
        }
    }
    Ok(())
}

fn segment_y_at_x(start: [f32; 2], end: [f32; 2], x: f32) -> Option<f32> {
    let dx = end[0] - start[0];
    if dx.abs() <= f32::EPSILON {
        return None;
    }
    let min_x = start[0].min(end[0]);
    let max_x = start[0].max(end[0]);
    if x < min_x || x > max_x {
        return None;
    }
    let t = (x - start[0]) / dx;
    Some(start[1] + (end[1] - start[1]) * t)
}

fn spawn_is_supported(gameplay: &MapGameplayAuthoring, position: [f32; 2]) -> bool {
    let target_surface = position[1] - purgatory_simulation::PLAYER_HALF_EXTENTS[1];
    gameplay.foothold_paths.iter().any(|path| {
        path.points.windows(2).any(|pair| {
            segment_y_at_x(pair[0], pair[1], position[0])
                .is_some_and(|surface_y| (surface_y - target_surface).abs() <= 0.05)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/authoring/maps/map.map1.purgatory-map.json")
    }

    struct RestoreFiles {
        directory: PathBuf,
        files: Vec<(PathBuf, Vec<u8>)>,
    }

    impl Drop for RestoreFiles {
        fn drop(&mut self) {
            for (path, bytes) in &self.files {
                let _ = std::fs::write(path, bytes);
            }
            if let Ok(entries) = std::fs::read_dir(&self.directory) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|ext| ext.to_str()) == Some("json")
                        && !self.files.iter().any(|(saved, _)| saved == &path)
                    {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
        }
    }

    fn snapshot_shared_maps() -> RestoreFiles {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/shared/maps");
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("json")
                    && let Ok(bytes) = std::fs::read(&path)
                {
                    files.push((path, bytes));
                }
            }
        }
        RestoreFiles { directory, files }
    }

    #[test]
    fn bridge_opens_shared_compiler_output_and_recompiles_ppu() {
        let mut document = MapLabDocument::open(fixture()).expect("open");
        let visual_extent_px = document.presentation.visual_extent_px;
        let source_id = document.source.id.clone();
        let baseline_bounds = document.presentation.world_bounds;
        let preview_ppu = document.source.pixels_per_world_unit * 0.5;

        assert!(visual_extent_px[0] > 0);
        assert!(visual_extent_px[1] > 0);
        assert!(!document.presentation.layers.is_empty());
        assert_eq!(document.gameplay.map_authored, source_id);

        document.recompile(preview_ppu).expect("recompile");
        assert_eq!(document.presentation.visual_extent_px, visual_extent_px);
        assert!((document.presentation.world_bounds[2] - baseline_bounds[2] * 2.0).abs() < 1e-5);
        assert!((document.presentation.world_bounds[3] - baseline_bounds[3] * 2.0).abs() < 1e-5);

        let decoded: MapPresentation =
            serde_json::from_slice(&document.canonical_json().unwrap()).unwrap();
        assert_eq!(decoded, document.presentation);
    }

    #[test]
    fn environment_path_derivation_is_name_agnostic() {
        let path = Path::new("content/authoring/maps/any.map.purgatory-map.json");
        let environment = environment_path_for(path).unwrap();
        assert!(environment.ends_with("any.map.environment.json"));
    }

    #[test]
    fn gameplay_path_derivation_is_name_agnostic() {
        let path = Path::new("content/authoring/maps/any.map.purgatory-map.json");
        let gameplay = gameplay_path_for(path).unwrap();
        assert!(gameplay.ends_with("any.map.gameplay.json"));
    }

    #[test]
    fn runtime_projection_follows_tmx_extent_and_is_deterministic() {
        let (root, authoring) = temp_authoring(
            "projection",
            &catalog_with("| `50002` | `map.map2` | active |\n"),
        );
        let created = create_new_map(
            &authoring,
            &NewMapRequest {
                display_name: "West Road".to_owned(),
                width_tiles: 116,
                height_tiles: 65,
            },
        )
        .unwrap();
        let authored = &created.authored_id;
        let gameplay_path = authoring.join(format!("{authored}.gameplay.json"));
        let environment_path = authoring.join(format!("{authored}.environment.json"));
        let placements_path = root
            .join("content/server/placements")
            .join(format!("{authored}.json"));
        let before = (
            std::fs::read(&gameplay_path).unwrap(),
            std::fs::read(&environment_path).unwrap(),
            std::fs::read(&placements_path).unwrap(),
        );
        let projection = root
            .join("content/shared/maps")
            .join(format!("{authored}.json"));
        let first = std::fs::read_to_string(&projection).unwrap();
        assert!(first.contains("\"spawn_points\": []"));
        assert!(!first.contains("\"restore\""));
        assert!(first.contains("\"content_id\": 50003"));

        std::fs::write(&created.tmx_path, canonical_tmx_document(200, 65)).unwrap();
        let document = MapLabDocument::open(&created.sidecar_path).unwrap();
        let updated = std::fs::read_to_string(&projection).unwrap();
        assert!(updated.contains("\"max_x\": 40.0"));
        assert!((document.presentation.world_bounds[2] - 40.0).abs() < 1e-4);
        assert!(document.gameplay.foothold_paths.is_empty());
        assert!(document.gameplay.spawn_points.is_empty());
        assert_eq!(
            (
                std::fs::read(&gameplay_path).unwrap(),
                std::fs::read(&environment_path).unwrap(),
                std::fs::read(&placements_path).unwrap(),
            ),
            before
        );
        MapLabDocument::open(&created.sidecar_path).unwrap();
        assert_eq!(updated, std::fs::read_to_string(&projection).unwrap());
        assert_ne!(first, updated);

        let registry = load_registry(&root.join("content"), purgatory_content::LoadMode::Shared)
            .expect("new map is known");
        let map = registry.map(authored).expect("authored map");
        assert_eq!(map.content_id.raw(), Some(50_003));
        assert!(map.spawn_points.is_empty());
        assert!(map.restore.is_none());
        assert!(!map.is_gameplay_ready());
        let plan_error = purgatory_content::map_plan(
            &registry,
            authored,
            purgatory_content::world_address_for_map(
                &registry,
                map.content_id,
                purgatory_common::ChannelId::DEFAULT,
                purgatory_common::InstanceId::DEFAULT,
            )
            .unwrap(),
        )
        .expect_err("incomplete map is not enterable");
        assert!(plan_error.to_string().contains("GAMEPLAY NOT READY"));
        assert!(!plan_error.to_string().contains("unknown map"));

        std::fs::write(
            authoring.join("map.mpa999.gameplay.json"),
            br#"{"schema_version":1,"map_authored":"map.mpa999","name":"typo","foothold_paths":[],"spawn_points":[]}"#,
        )
        .unwrap();
        let unknown = load_registry(&root.join("content"), purgatory_content::LoadMode::Shared)
            .expect_err("typo map");
        assert!(unknown.to_string().contains("unknown map"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn default_spawn_is_supported_by_authored_foothold() {
        let document = MapLabDocument::open(fixture()).expect("open");
        assert!(document.gameplay_readiness().is_ok());
    }

    #[test]
    fn discovery_lists_every_authored_sidecar_and_open_uses_that_document() {
        let _restore = snapshot_shared_maps();
        let directory = fixture()
            .parent()
            .expect("authoring directory")
            .to_path_buf();
        let maps = discover_authored_maps(&directory).expect("discover");
        let names: Vec<_> = maps
            .iter()
            .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
            .collect();
        assert!(names.len() >= 2);
        assert!(names.contains(&"map.map1.purgatory-map.json"));
        assert!(names.contains(&"map.map2.purgatory-map.json"));

        let map3 = directory.join("map.map3.purgatory-map.json");
        let catalog = std::fs::read(directory.join("../../CONTENT_ID_CATALOG.md")).unwrap();
        let document = MapLabDocument::open(&map3).expect("open existing map.map3");
        assert_eq!(document.source.content_id, 50_003);
        assert_eq!(document.source.id, "map.map3");
        assert!(document.gameplay.spawn_points.is_empty());
        assert!(document.gameplay_readiness().is_err());
        assert_eq!(
            std::fs::read(directory.join("../../CONTENT_ID_CATALOG.md")).unwrap(),
            catalog
        );

        for path in &maps {
            let document = MapLabDocument::open(path).expect("open discovered map");
            let name = path.file_name().and_then(|name| name.to_str()).unwrap();
            assert!(name.starts_with(&document.source.id));
            assert_eq!(document.sidecar_path, *path);
        }
    }

    #[test]
    fn numeric_tmx_import_creates_sidecar_and_catalog_allocation() {
        let root =
            std::env::temp_dir().join(format!("purgatory-map-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let graphic_maps = root.join("Graphic/assets/maps");
        let authoring = root.join("content/authoring/maps");
        std::fs::create_dir_all(&graphic_maps).unwrap();
        std::fs::create_dir_all(&authoring).unwrap();
        std::fs::write(graphic_maps.join("50077.tmx"), "<map/>").unwrap();
        std::fs::write(
            root.join("content/CONTENT_ID_CATALOG.md"),
            "# Content ID Catalog\n\n### Maps — 50,000–59,999\n\n| ID | Label | Status |\n| ---: | --- | --- |\n\n### Items — 30,000–39,999\n",
        )
        .unwrap();

        let candidates = discover_unimported_tmx(&authoring).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].content_id, 50_077);
        assert_eq!(candidates[0].authored_id, "map.map77");
        let sidecar = import_numeric_tmx(&authoring, &candidates[0]).unwrap();
        let source = load_map_authoring(&sidecar).unwrap();
        assert_eq!(source.content_id, 50_077);
        assert_eq!(source.id, "map.map77");
        let catalog = std::fs::read_to_string(root.join("content/CONTENT_ID_CATALOG.md")).unwrap();
        assert!(catalog.contains("| `50077` | `map.map77` | active |"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_switch_opens_and_dirty_switch_waits_for_a_choice() {
        assert_eq!(
            classify_map_switch(false, false, false),
            MapSwitchKind::Open
        );
        assert_eq!(
            classify_map_switch(true, false, false),
            MapSwitchKind::AlreadyOpen
        );
        assert_eq!(classify_map_switch(true, true, false), MapSwitchKind::Open);
        assert_eq!(
            classify_map_switch(false, false, true),
            MapSwitchKind::Confirm
        );
        assert_eq!(
            classify_map_switch(true, true, true),
            MapSwitchKind::Confirm
        );
        assert!(!apply_map_switch_choice(MapSwitchChoice::Cancel, || Ok::<(), &str>(())).unwrap());
        assert!(
            apply_map_switch_choice(MapSwitchChoice::DiscardAndSwitch, || Ok::<(), &str>(()))
                .unwrap()
        );
        assert!(
            apply_map_switch_choice(MapSwitchChoice::SaveAndSwitch, || Ok::<(), &str>(())).unwrap()
        );
        assert_eq!(
            apply_map_switch_choice(MapSwitchChoice::SaveAndSwitch, || Err("save failed")),
            Err("save failed")
        );
    }

    #[test]
    fn save_switch_persists_discard_does_not_and_failed_save_keeps_the_file() {
        let document = MapLabDocument::open(fixture()).expect("open");
        assert!(!document.placements.is_empty());
        let original = document.placements[0].position;
        let temp = std::env::temp_dir().join(format!(
            "purgatory-map-lab-switch-{}-{}.json",
            std::process::id(),
            document.source.id.replace('.', "_")
        ));
        let _ = std::fs::remove_file(&temp);

        let mut edited = document.clone();
        edited.placements_path = temp.clone();
        edited.placements[0].position = [3.5, 4.5];
        let discarded = MapLabDocument::open(fixture()).expect("discard reload");
        assert_eq!(discarded.placements[0].position, original);

        edited.save_placements().expect("save");
        let (_, saved) = load_placement_file(&temp).expect("saved placements");
        assert_eq!(saved[0].position, [3.5, 4.5]);

        edited.placements.push(Placement {
            id: "placement.mob_999".into(),
            kind: purgatory_content::PlacementKind::Monster,
            content_authored: "monster.not_authored".into(),
            position: [0.0, 0.0],
            portal_link: None,
        });
        let error = edited.save_placements().expect_err("invalid placement");
        assert!(error.contains("monster.not_authored") || error.contains("unresolved"));
        let (_, after_failure) = load_placement_file(&temp).expect("previous save remains");
        assert_eq!(after_failure.len(), saved.len());
        assert_eq!(after_failure[0].position, [3.5, 4.5]);
        assert_eq!(edited.placements[0].position, [3.5, 4.5]);
        let _ = std::fs::remove_file(&temp);
    }

    #[test]
    fn spawn_snap_uses_player_center_above_surface() {
        let document = MapLabDocument::open(fixture()).expect("open");
        let snapped = document.snap_spawn_to_foothold([15.31, 0.9]).unwrap();
        assert!((snapped[0] - 15.31).abs() < 1e-5);
        assert!((snapped[1] - 1.0).abs() < 1e-4);
    }

    const CATALOG_HEAD: &str = "# Content ID Catalog\n\n### Maps — 50,000–59,999\n\n| ID | Label | Status |\n| ---: | --- | --- |\n";
    const CATALOG_TAIL: &str = "\n### Items — 30,000–39,999\n";

    fn catalog_with(rows: &str) -> String {
        format!("{CATALOG_HEAD}{rows}{CATALOG_TAIL}")
    }

    fn temp_authoring(name: &str, catalog: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "purgatory-new-map-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let authoring = root.join("content/authoring/maps");
        std::fs::create_dir_all(&authoring).unwrap();
        std::fs::create_dir_all(root.join("Graphic/assets/maps")).unwrap();
        std::fs::create_dir_all(root.join("content/server/placements")).unwrap();
        std::fs::create_dir_all(root.join("content/shared/maps")).unwrap();
        std::fs::write(root.join("content/CONTENT_ID_CATALOG.md"), catalog).unwrap();
        (root, authoring)
    }

    #[test]
    fn map_allocation_inserts_inside_maps_for_lf_and_crlf_catalogs() {
        for (name, newline) in [("lf", "\n"), ("crlf", "\r\n")] {
            let catalog = sample_catalog(newline, false);
            let (root, _) = temp_authoring(&format!("catalog-{name}"), &catalog);
            record_map_allocation(&root, 50_003, "map.map3", false).expect(name);
            let updated =
                std::fs::read_to_string(root.join("content/CONTENT_ID_CATALOG.md")).unwrap();
            assert_eq!(updated, sample_catalog(newline, true), "{name}");
            let section = map_catalog_section(&updated).unwrap();
            assert!(section.contains("| `50001` | `map.map1` | active |"));
            assert!(section.contains("| `50002` | `map.map2` | active |"));
            assert!(section.contains("| `50003` | `map.map3` | active |"));
            assert!(!section.contains("item.keep"));
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    fn sample_catalog(newline: &str, with_new_row: bool) -> String {
        let mut catalog = format!(
            "# Content ID Catalog{newline}{newline}### Maps — 50,000–59,999{newline}{newline}| ID | Label | Status |{newline}| ---: | --- | --- |{newline}| `50001` | `map.map1` | active |{newline}| `50002` | `map.map2` | active |{newline}"
        );
        if with_new_row {
            catalog.push_str(&format!("| `50003` | `map.map3` | active |{newline}"));
        }
        catalog.push_str(&format!(
            "{newline}### Items — 30,000–39,999{newline}| `30001` | `item.keep` | active |{newline}"
        ));
        catalog
    }

    #[test]
    fn next_content_id_follows_the_catalog() {
        let catalog =
            catalog_with("| `50001` | `map.map1` | active |\n| `50002` | `map.map2` | active |\n");
        assert_eq!(
            allocate_next_map_id(&catalog).unwrap(),
            (50_003, "map.map3".to_owned())
        );
        assert_eq!(
            allocate_next_map_id(&catalog_with("")).unwrap(),
            (50_001, "map.map1".to_owned())
        );
    }

    #[test]
    fn allocation_skips_existing_ids_without_filling_gaps() {
        let catalog =
            catalog_with("| `50001` | `map.map1` | active |\n| `50004` | `map.map4` | active |\n");
        assert_eq!(allocate_next_map_id(&catalog).unwrap().0, 50_005);
    }

    #[test]
    fn allocation_does_not_reuse_retired_ids() {
        let catalog = catalog_with(
            "| `50001` | `map.map1` | active |\n| `50002` | `map.map2` | retired (reserved; never reuse) |\n",
        );
        assert_eq!(
            allocate_next_map_id(&catalog).unwrap(),
            (50_003, "map.map3".to_owned())
        );
    }

    #[test]
    fn exhausted_map_range_is_rejected() {
        let catalog = catalog_with("| `59999` | `map.map999` | active |\n");
        let error = allocate_next_map_id(&catalog).unwrap_err();
        assert!(error.contains("exhausted"));
    }

    #[test]
    fn authored_id_uses_the_map_block_offset() {
        assert_eq!(authored_map_id(50_008).unwrap(), "map.map8");
        assert_eq!(authored_map_id(50_001).unwrap(), "map.map1");
    }

    #[test]
    fn canonical_tmx_uses_the_production_grid() {
        assert_eq!(minimum_new_map_tiles(), (116, 65));
        let tmx = canonical_tmx_document(116, 65);
        assert!(tmx.contains("orientation=\"orthogonal\""));
        assert!(tmx.contains("tilewidth=\"20\""));
        assert!(tmx.contains("tileheight=\"20\""));
        assert!(tmx.contains("width=\"116\""));
        assert!(tmx.contains("height=\"65\""));
        assert!(tmx.contains("name=\"Tile Layer 1\""));
        assert!(!tmx.contains("TEST.tsx"));
        assert!(!tmx.contains(":\\") && !tmx.contains("C:/"));
    }

    #[test]
    fn new_map_writes_required_files_and_is_discoverable() {
        let (root, authoring) = temp_authoring(
            "create",
            &catalog_with("| `50001` | `map.map1` | active |\n| `50002` | `map.map2` | active |\n"),
        );
        let created = create_new_map(
            &authoring,
            &NewMapRequest {
                display_name: "West Road".to_owned(),
                width_tiles: 116,
                height_tiles: 65,
            },
        )
        .unwrap();
        assert_eq!(created.content_id, 50_003);
        assert_eq!(created.authored_id, "map.map3");
        assert!(
            created.tmx_path.ends_with("Graphic/assets/maps/50003.tmx")
                || created
                    .tmx_path
                    .ends_with("Graphic\\assets\\maps\\50003.tmx")
        );
        assert!(
            created
                .sidecar_path
                .ends_with("map.map3.purgatory-map.json")
        );
        assert!(authoring.join("map.map3.gameplay.json").is_file());
        assert!(authoring.join("map.map3.environment.json").is_file());
        assert!(
            root.join("content/server/placements/map.map3.json")
                .is_file()
        );
        let catalog = std::fs::read_to_string(root.join("content/CONTENT_ID_CATALOG.md")).unwrap();
        assert!(catalog.contains("| `50003` | `map.map3` | active |"));
        let document = MapLabDocument::open(&created.sidecar_path).unwrap();
        assert_eq!(
            document.source.visual_source,
            "../../../Graphic/assets/maps/50003.tmx"
        );
        assert_eq!(document.gameplay.name, "West Road");
        assert!(document.gameplay.foothold_paths.is_empty());
        assert!(document.gameplay.spawn_points.is_empty());
        assert!(document.environment.parallax_layers.is_empty());
        assert!(document.placements.is_empty());
        assert!(document.gameplay_readiness().is_err());
        assert!(
            root.join("content/shared/maps")
                .join(format!("{}.json", created.authored_id))
                .is_file()
        );
        let discovered = discover_authored_maps(&authoring).unwrap();
        assert!(discovered.contains(&created.sidecar_path));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn duplicate_new_map_destination_is_rejected() {
        let (root, authoring) = temp_authoring(
            "duplicate",
            &catalog_with("| `50001` | `map.map1` | active |\n| `50002` | `map.map2` | active |\n"),
        );
        let before = std::fs::read_to_string(root.join("content/CONTENT_ID_CATALOG.md")).unwrap();
        std::fs::write(root.join("Graphic/assets/maps/50003.tmx"), "<map/>").unwrap();
        let error = create_new_map(
            &authoring,
            &NewMapRequest {
                display_name: "West Road".to_owned(),
                width_tiles: 116,
                height_tiles: 65,
            },
        )
        .unwrap_err();
        assert!(error.contains("already exists"));
        assert!(!authoring.join("map.map3.purgatory-map.json").exists());
        assert_eq!(
            std::fs::read_to_string(root.join("content/CONTENT_ID_CATALOG.md")).unwrap(),
            before
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_creation_rolls_back_files_and_catalog() {
        let (root, authoring) = temp_authoring(
            "rollback",
            &catalog_with("| `50001` | `map.map1` | active |\n| `50002` | `map.map2` | active |\n"),
        );
        let placements = root.join("content/server/placements");
        std::fs::remove_dir_all(&placements).unwrap();
        std::fs::write(&placements, "not a directory").unwrap();
        let before = std::fs::read_to_string(root.join("content/CONTENT_ID_CATALOG.md")).unwrap();
        let error = create_new_map(
            &authoring,
            &NewMapRequest {
                display_name: "West Road".to_owned(),
                width_tiles: 116,
                height_tiles: 65,
            },
        )
        .unwrap_err();
        assert!(error.contains("placements") || error.contains("create"));
        assert!(!root.join("Graphic/assets/maps/50003.tmx").exists());
        assert!(!authoring.join("map.map3.purgatory-map.json").exists());
        assert!(!authoring.join("map.map3.gameplay.json").exists());
        assert!(!authoring.join("map.map3.environment.json").exists());
        assert_eq!(
            std::fs::read_to_string(root.join("content/CONTENT_ID_CATALOG.md")).unwrap(),
            before
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    fn authored_bytes(root: &Path, authored: &str) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let gameplay = std::fs::read(
            root.join("content/authoring/maps")
                .join(format!("{authored}.gameplay.json")),
        )
        .unwrap();
        let environment = std::fs::read(
            root.join("content/authoring/maps")
                .join(format!("{authored}.environment.json")),
        )
        .unwrap();
        let placements = std::fs::read(
            root.join("content/server/placements")
                .join(format!("{authored}.json")),
        )
        .unwrap();
        (gameplay, environment, placements)
    }

    fn write_authored_sample(root: &Path, authored: &str) {
        let gameplay = purgatory_content::MapGameplayAuthoring {
            schema_version: purgatory_content::MAP_GAMEPLAY_AUTHORING_SCHEMA_VERSION,
            map_authored: authored.to_owned(),
            name: "West Road".to_owned(),
            foothold_paths: vec![purgatory_content::FootholdPath {
                id: "foothold.001".to_owned(),
                kind: purgatory_content::FootholdKind::Solid,
                drop_through: false,
                points: vec![[1.0, 1.0], [10.0, 1.0]],
            }],
            spawn_points: vec![purgatory_content::GameplaySpawnPoint {
                id: "default".to_owned(),
                position: [4.0, 1.6],
            }],
            restore: None,
        };
        let mut environment = purgatory_content::MapEnvironmentAuthoring::empty(authored);
        environment.sky_gradient = Some(purgatory_content::SkyGradient {
            top_rgba: [9, 8, 7, 255],
            bottom_rgba: [6, 5, 4, 255],
        });
        let placements = purgatory_content::serialize_placements_v2(
            authored,
            &[
                Placement {
                    id: "placement.mob_001".to_owned(),
                    kind: purgatory_content::PlacementKind::Monster,
                    content_authored: "monster.moss_crab".to_owned(),
                    position: [8.0, 1.0],
                    portal_link: None,
                },
                Placement {
                    id: "portal.001".to_owned(),
                    kind: purgatory_content::PlacementKind::Portal,
                    content_authored: String::new(),
                    position: [6.0, 1.0],
                    portal_link: Some(purgatory_content::PortalLink {
                        map_authored: "map.map1".to_owned(),
                        portal_id: "portal.001".to_owned(),
                    }),
                },
            ],
        )
        .unwrap();
        std::fs::write(
            root.join("content/authoring/maps")
                .join(format!("{authored}.gameplay.json")),
            serde_json::to_vec_pretty(&gameplay).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("content/authoring/maps")
                .join(format!("{authored}.environment.json")),
            serde_json::to_vec_pretty(&environment).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("content/server/placements")
                .join(format!("{authored}.json")),
            placements,
        )
        .unwrap();
    }

    #[test]
    fn tmx_reload_preserves_gameplay_environment_and_placements() {
        let (root, authoring) = temp_authoring(
            "reload",
            &catalog_with("| `50002` | `map.map2` | active |\n"),
        );
        let created = create_new_map(
            &authoring,
            &NewMapRequest {
                display_name: "West Road".to_owned(),
                width_tiles: 200,
                height_tiles: 65,
            },
        )
        .unwrap();
        write_authored_sample(&root, &created.authored_id);
        let before = authored_bytes(&root, &created.authored_id);
        let mut tmx = std::fs::read_to_string(&created.tmx_path).unwrap();
        tmx = tmx.replace("Tile Layer 1", "Painted Ground");
        std::fs::write(&created.tmx_path, tmx).unwrap();

        let document = MapLabDocument::open(&created.sidecar_path).unwrap();
        assert!(
            document
                .presentation
                .layers
                .iter()
                .any(|layer| layer.name == "Painted Ground")
        );
        assert_eq!(document.gameplay.foothold_paths.len(), 1);
        assert_eq!(document.gameplay.spawn_points[0].id, "default");
        assert_eq!(
            document.environment.sky_gradient.unwrap().top_rgba,
            [9, 8, 7, 255]
        );
        assert_eq!(document.placements.len(), 2);
        assert_eq!(
            document.placements[1]
                .portal_link
                .as_ref()
                .unwrap()
                .map_authored,
            "map.map1"
        );
        assert_eq!(authored_bytes(&root, &created.authored_id), before);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounds_shrink_reports_outside_content_without_deleting_it() {
        let (root, authoring) = temp_authoring(
            "bounds",
            &catalog_with("| `50002` | `map.map2` | active |\n"),
        );
        let created = create_new_map(
            &authoring,
            &NewMapRequest {
                display_name: "West Road".to_owned(),
                width_tiles: 200,
                height_tiles: 65,
            },
        )
        .unwrap();
        let gameplay = purgatory_content::MapGameplayAuthoring {
            schema_version: purgatory_content::MAP_GAMEPLAY_AUTHORING_SCHEMA_VERSION,
            map_authored: created.authored_id.clone(),
            name: "West Road".to_owned(),
            foothold_paths: vec![purgatory_content::FootholdPath {
                id: "foothold.001".to_owned(),
                kind: purgatory_content::FootholdKind::OneWay,
                drop_through: true,
                points: vec![[30.0, 1.0], [35.0, 1.0]],
            }],
            spawn_points: vec![purgatory_content::GameplaySpawnPoint {
                id: "default".to_owned(),
                position: [31.0, 1.6],
            }],
            restore: None,
        };
        let placements = purgatory_content::serialize_placements_v2(
            &created.authored_id,
            &[
                Placement {
                    id: "placement.mob_001".to_owned(),
                    kind: purgatory_content::PlacementKind::Monster,
                    content_authored: "monster.moss_crab".to_owned(),
                    position: [31.0, 1.0],
                    portal_link: None,
                },
                Placement {
                    id: "placement.npc_001".to_owned(),
                    kind: purgatory_content::PlacementKind::Entity,
                    content_authored: "npc.welcome.gate_watchman".to_owned(),
                    position: [31.0, 1.5],
                    portal_link: None,
                },
                Placement {
                    id: "portal.001".to_owned(),
                    kind: purgatory_content::PlacementKind::Portal,
                    content_authored: String::new(),
                    position: [31.0, 2.0],
                    portal_link: Some(purgatory_content::PortalLink {
                        map_authored: "map.map1".to_owned(),
                        portal_id: "portal.001".to_owned(),
                    }),
                },
            ],
        )
        .unwrap();
        std::fs::write(
            authoring.join("map.map3.gameplay.json"),
            serde_json::to_vec_pretty(&gameplay).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("content/server/placements/map.map3.json"),
            placements,
        )
        .unwrap();
        let before = authored_bytes(&root, "map.map3");
        std::fs::write(&created.tmx_path, canonical_tmx_document(116, 65)).unwrap();

        let document = MapLabDocument::open(&created.sidecar_path).unwrap();
        let issues = document.bounds_issues();
        let ids: Vec<_> = issues.iter().map(|issue| issue.id.as_str()).collect();
        assert!(ids.contains(&"foothold.001"));
        assert!(ids.contains(&"default"));
        assert!(ids.contains(&"placement.mob_001"));
        assert!(ids.contains(&"placement.npc_001"));
        assert!(ids.contains(&"portal.001"));
        assert!(issues.iter().any(|issue| issue.kind == "NPC"));
        assert_eq!(document.gameplay.foothold_paths[0].points[0][0], 30.0);
        assert_eq!(document.placements[0].position[0], 31.0);
        assert_eq!(
            document.placements[2]
                .portal_link
                .as_ref()
                .unwrap()
                .portal_id,
            "portal.001"
        );
        assert_eq!(authored_bytes(&root, "map.map3"), before);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn structural_tile_size_mismatch_is_rejected_without_rewriting_authoring() {
        let (root, authoring) = temp_authoring(
            "structure",
            &catalog_with("| `50002` | `map.map2` | active |\n"),
        );
        let created = create_new_map(
            &authoring,
            &NewMapRequest {
                display_name: "West Road".to_owned(),
                width_tiles: 116,
                height_tiles: 65,
            },
        )
        .unwrap();
        write_authored_sample(&root, &created.authored_id);
        let before = authored_bytes(&root, &created.authored_id);
        let tmx = std::fs::read_to_string(&created.tmx_path)
            .unwrap()
            .replace("tilewidth=\"20\"", "tilewidth=\"32\"");
        std::fs::write(&created.tmx_path, tmx).unwrap();
        let error = MapLabDocument::open(&created.sidecar_path).unwrap_err();
        assert!(error.contains("tile-size migration is not supported"));
        assert_eq!(authored_bytes(&root, &created.authored_id), before);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checked_in_runtime_projections_are_stable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for authored in ["map.map1", "map.map2", "map.map3"] {
            let sidecar = root.join(format!(
                "content/authoring/maps/{authored}.purgatory-map.json"
            ));
            MapLabDocument::open(&sidecar).expect(authored);
            let path = root.join(format!("content/shared/maps/{authored}.json"));
            let once = std::fs::read(&path).expect(authored);
            MapLabDocument::open(&sidecar).expect(authored);
            assert_eq!(once, std::fs::read(&path).unwrap(), "{authored}");
            let text = String::from_utf8(once).unwrap();
            assert!(text.contains(&format!("\"id\": \"{authored}\"")));
            assert!(text.contains("\"platforms\": []"));
        }
    }
}
