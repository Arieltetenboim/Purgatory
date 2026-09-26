//! Thin Map Lab document boundary over the shared map compiler.

use std::path::{Path, PathBuf};

use purgatory_content::{
    MAP_GAMEPLAY_AUTHORING_SCHEMA_VERSION, MapAuthoringSource, MapGameplayAuthoring,
    MapPresentation, compile_tiled_map_with_ppu, load_map_authoring, serialize_map_pretty,
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
    pub source: MapAuthoringSource,
    pub presentation: MapPresentation,
    pub gameplay: MapGameplayAuthoring,
}

impl MapLabDocument {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let source = load_map_authoring(path).map_err(|error| error.to_string())?;
        let presentation = compile_tiled_map_with_ppu(path, &source, source.pixels_per_world_unit)
            .map_err(|error| error.to_string())?;
        let gameplay_path = gameplay_path_for(path)?;
        let gameplay = if gameplay_path.is_file() {
            let bytes = std::fs::read(&gameplay_path)
                .map_err(|error| format!("read {}: {error}", gameplay_path.display()))?;
            let gameplay: MapGameplayAuthoring = serde_json::from_slice(&bytes)
                .map_err(|error| format!("parse {}: {error}", gameplay_path.display()))?;
            validate_gameplay(&gameplay_path, &source, &presentation, &gameplay)?;
            gameplay
        } else {
            MapGameplayAuthoring::empty(source.id.clone())
        };
        Ok(Self {
            sidecar_path: path.to_path_buf(),
            gameplay_path,
            source,
            presentation,
            gameplay,
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
        if foothold
            .points
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        {
            return Err(format!(
                "{}: foothold {} contains a zero-length segment",
                path.display(),
                foothold.id
            ));
        }
    }
    Ok(())
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
        assert_eq!(document.presentation.visual_extent_px, [4644, 1080]);
        assert_eq!(document.gameplay.map_authored, "map.map1");
        document.recompile(50.0).expect("recompile");
        assert_eq!(document.presentation.world_bounds, [0.0, 0.0, 92.88, 21.6]);
        let decoded: MapPresentation =
            serde_json::from_slice(&document.canonical_json().unwrap()).unwrap();
        assert_eq!(decoded, document.presentation);
    }

    #[test]
    fn gameplay_path_sits_next_to_visual_sidecar() {
        let path = gameplay_path_for(&fixture()).unwrap();
        assert!(path.ends_with("map.map1.gameplay.json"));
    }
}
