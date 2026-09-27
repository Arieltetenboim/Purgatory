//! Thin Map Lab document boundary over the shared map compiler.

use std::path::{Path, PathBuf};

use purgatory_content::{
    LoadMode, MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION, MAP_GAMEPLAY_AUTHORING_SCHEMA_VERSION,
    MapAuthoringSource, MapEnvironmentAuthoring, MapGameplayAuthoring, MapPresentation, Placement,
    compile_tiled_map_with_ppu, default_content_root, load_map_authoring, load_placement_file,
    load_registry, resolve_png_asset_folder, serialize_map_pretty, serialize_placements_v2,
    validate_cloud_field,
};

/// Production visual-scale standard for ordinary PURGATORY maps.
///
/// The per-map sidecar keeps PPU explicit, but normal authored maps should use
/// this value. Camera zoom is a separate presentation concern.
pub const PURGATORY_STANDARD_PPU: f32 = 100.0;

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
        let presentation = compile_tiled_map_with_ppu(path, &source, source.pixels_per_world_unit)
            .map_err(|error| error.to_string())?;
        sync_runtime_bounds_file(path, &source.id, presentation.world_bounds)?;
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
            validate_gameplay(&gameplay_path, &source, &presentation, &gameplay)?;
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
        self.presentation =
            compile_tiled_map_with_ppu(&self.sidecar_path, &self.source, pixels_per_world_unit)
                .map_err(|error| error.to_string())?;
        Ok(())
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

    pub fn save_gameplay(&self) -> Result<(), String> {
        validate_gameplay(
            &self.gameplay_path,
            &self.source,
            &self.presentation,
            &self.gameplay,
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
            .map_err(|error| format!("write {}: {error}", self.gameplay_path.display()))
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

fn sync_runtime_bounds_file(
    sidecar: &Path,
    map_authored: &str,
    world_bounds: [f32; 4],
) -> Result<bool, String> {
    let runtime_path = runtime_map_path_for(sidecar, map_authored)?;
    if !runtime_path.is_file() {
        return Ok(false);
    }

    let text = std::fs::read_to_string(&runtime_path)
        .map_err(|error| format!("read {}: {error}", runtime_path.display()))?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("parse {}: {error}", runtime_path.display()))?;
    let bounds = json
        .get("bounds")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| format!("{}: missing bounds object", runtime_path.display()))?;

    let [min_x, min_y, max_x, max_y] = world_bounds;
    let expected = [
        ("min_x", min_x),
        ("max_x", max_x),
        ("min_y", min_y),
        ("max_y", max_y),
    ];
    let already_synced = expected.iter().all(|(key, value)| {
        bounds
            .get(*key)
            .and_then(serde_json::Value::as_f64)
            .is_some_and(|current| (current - f64::from(*value)).abs() <= 1.0e-5)
    });
    if already_synced {
        return Ok(false);
    }

    let bounds_key = text
        .find("\"bounds\"")
        .ok_or_else(|| format!("{}: bounds key not found", runtime_path.display()))?;
    let object_start = bounds_key
        + text[bounds_key..]
            .find('{')
            .ok_or_else(|| format!("{}: bounds object start not found", runtime_path.display()))?;
    let object_end = object_start
        + text[object_start..]
            .find('}')
            .ok_or_else(|| format!("{}: bounds object end not found", runtime_path.display()))?;
    let line_start = text[..bounds_key].rfind('\n').map_or(0, |index| index + 1);
    let indent = &text[line_start..bounds_key];
    if !indent.chars().all(char::is_whitespace) {
        return Err(format!(
            "{}: bounds indentation is not plain whitespace",
            runtime_path.display()
        ));
    }
    let child_indent = format!("{indent}  ");
    let encode = |value: f32| {
        serde_json::to_string(&value)
            .map_err(|error| format!("serialize {} bounds: {error}", runtime_path.display()))
    };
    let replacement = format!(
        "{{\n{child_indent}\"min_x\": {},\n{child_indent}\"max_x\": {},\n{child_indent}\"min_y\": {},\n{child_indent}\"max_y\": {}\n{indent}}}",
        encode(min_x)?,
        encode(max_x)?,
        encode(min_y)?,
        encode(max_y)?,
    );

    let mut updated = String::with_capacity(text.len() + replacement.len());
    updated.push_str(&text[..object_start]);
    updated.push_str(&replacement);
    updated.push_str(&text[object_end + 1..]);
    std::fs::write(&runtime_path, updated)
        .map_err(|error| format!("write {}: {error}", runtime_path.display()))?;
    Ok(true)
}

fn validate_gameplay(
    path: &Path,
    source: &MapAuthoringSource,
    presentation: &MapPresentation,
    gameplay: &MapGameplayAuthoring,
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
    let [min_x, min_y, max_x, max_y] = presentation.world_bounds;
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
        if foothold.points.iter().any(|point| {
            point[0] < min_x || point[0] > max_x || point[1] < min_y || point[1] > max_y
        }) {
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
        let [x, y] = spawn.position;
        if !x.is_finite() || !y.is_finite() || x < min_x || x > max_x || y < min_y || y > max_y {
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
    fn runtime_bounds_follow_tmx_extent_and_sync_is_stable() {
        let root = std::env::temp_dir().join(format!(
            "purgatory-map-lab-bounds-sync-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let authoring_maps = root.join("content/authoring/maps");
        let shared_maps = root.join("content/shared/maps");
        std::fs::create_dir_all(&authoring_maps).unwrap();
        std::fs::create_dir_all(&shared_maps).unwrap();

        let sidecar = authoring_maps.join("map.test.purgatory-map.json");
        std::fs::write(&sidecar, "{}\n").unwrap();
        let runtime = shared_maps.join("map.test.json");
        std::fs::write(
            &runtime,
            r#"{
  "schema_version": 1,
  "id": "map.test",
  "debug_name": "TEST",
  "bounds": {
    "min_x": 0,
    "max_x": 46.44,
    "min_y": 0,
    "max_y": 13.32
  },
  "spawn_points": [],
  "platforms": [],
  "restore": {
    "policy": "safe_point",
    "point": "default"
  }
}
"#,
        )
        .unwrap();

        assert!(sync_runtime_bounds_file(&sidecar, "map.test", [0.0, 0.0, 23.2, 13.0]).unwrap());
        let first = std::fs::read_to_string(&runtime).unwrap();
        assert!(first.contains("\"debug_name\": \"TEST\""));
        assert!(first.contains("\"max_x\": 23.2"));
        assert!(first.contains("\"max_y\": 13.0"));
        assert!(!sync_runtime_bounds_file(&sidecar, "map.test", [0.0, 0.0, 23.2, 13.0]).unwrap());
        assert_eq!(first, std::fs::read_to_string(&runtime).unwrap());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn default_spawn_is_supported_by_authored_foothold() {
        let document = MapLabDocument::open(fixture()).expect("open");
        assert!(document.gameplay_readiness().is_ok());
    }

    #[test]
    fn spawn_snap_uses_player_center_above_surface() {
        let document = MapLabDocument::open(fixture()).expect("open");
        let snapped = document.snap_spawn_to_foothold([15.31, 0.9]).unwrap();
        assert!((snapped[0] - 15.31).abs() < 1e-5);
        assert!((snapped[1] - 1.0).abs() < 1e-4);
    }
}
