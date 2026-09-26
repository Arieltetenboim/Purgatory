//! Thin Map Lab document boundary over the shared map compiler.

use std::path::{Path, PathBuf};

use purgatory_content::{
    MapAuthoringSource, MapPresentation, compile_tiled_map_with_ppu, load_map_authoring,
    serialize_map_pretty,
};

/// Production visual-scale standard for ordinary PURGATORY maps.
///
/// The per-map sidecar keeps PPU explicit, but normal authored maps should use
/// this value. Camera zoom is a separate presentation concern.
pub const PURGATORY_STANDARD_PPU: f32 = 100.0;

#[derive(Clone, Debug)]
pub struct MapLabDocument {
    pub sidecar_path: PathBuf,
    pub source: MapAuthoringSource,
    pub presentation: MapPresentation,
}

impl MapLabDocument {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let source = load_map_authoring(path).map_err(|error| error.to_string())?;
        let presentation = compile_tiled_map_with_ppu(path, &source, source.pixels_per_world_unit)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            sidecar_path: path.to_path_buf(),
            source,
            presentation,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/authoring/maps/map.dev.footnote.purgatory-map.json")
    }

    #[test]
    fn bridge_opens_shared_compiler_output_and_recompiles_ppu() {
        let mut document = MapLabDocument::open(fixture()).expect("open");
        assert_eq!(document.presentation.visual_extent_px, [1944, 1080]);
        document.recompile(50.0).expect("recompile");
        assert_eq!(document.presentation.world_bounds, [0.0, 0.0, 38.88, 21.6]);
        let decoded: MapPresentation =
            serde_json::from_slice(&document.canonical_json().unwrap()).unwrap();
        assert_eq!(decoded, document.presentation);
    }
}
