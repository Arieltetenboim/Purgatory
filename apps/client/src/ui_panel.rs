//! Authored panel projection and the first local production-UI proof.

use serde::Deserialize;
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::asset_runtime::AssetRuntime;
use crate::renderer::{PixelViewport, SpriteTextureId, UiTexturedRect};

const PANEL_PNG: &[u8] = include_bytes!("../../../Graphic/ui/panel.png");
const PANEL_METADATA: &str = include_str!("../../../Graphic/ui/panel.ui.json");
const PANEL_TEXTURE_FILE: &str = "panel.png";
const NORMAL_SIZE_UNITS: [f32; 2] = [320.0, 240.0];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ProofPanelMode {
    #[default]
    Hidden,
    Normal,
    Double,
}

impl ProofPanelMode {
    /// Applies one physical-key event. Returns true only when this proof owns it.
    pub(crate) fn apply_key(
        &mut self,
        physical_key: PhysicalKey,
        state: ElementState,
        repeat: bool,
    ) -> bool {
        if state != ElementState::Pressed || repeat {
            return false;
        }
        let PhysicalKey::Code(code) = physical_key else {
            return false;
        };
        *self = match code {
            KeyCode::KeyI if *self == Self::Normal => Self::Hidden,
            KeyCode::KeyI => Self::Normal,
            KeyCode::KeyO if *self == Self::Double => Self::Hidden,
            KeyCode::KeyO => Self::Double,
            _ => return false,
        };
        true
    }

    fn logical_size(self) -> Option<[f32; 2]> {
        match self {
            Self::Hidden => None,
            Self::Normal => Some(NORMAL_SIZE_UNITS),
            Self::Double => Some([NORMAL_SIZE_UNITS[0] * 2.0, NORMAL_SIZE_UNITS[1] * 2.0]),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct SourceInsets {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct DestinationBorders {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl DestinationBorders {
    fn scaled(self, scale: f32) -> Self {
        Self {
            left: self.left * scale,
            right: self.right * scale,
            top: self.top * scale,
            bottom: self.bottom * scale,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UiPanelMetadata {
    schema_version: u32,
    id: String,
    texture: String,
    slice_px: SourceInsets,
    border_units: DestinationBorders,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiPanelAsset {
    texture: SpriteTextureId,
    source_size_px: [u32; 2],
    slice_px: SourceInsets,
    border_units: DestinationBorders,
}

impl UiPanelAsset {
    pub(crate) fn load_embedded(assets: &mut AssetRuntime) -> Result<Self, String> {
        let metadata: UiPanelMetadata = serde_json::from_str(PANEL_METADATA)
            .map_err(|error| format!("parse UI panel metadata: {error}"))?;
        if metadata.texture != PANEL_TEXTURE_FILE {
            return Err(format!(
                "UI panel {} names texture {:?}; expected {:?}",
                metadata.id, metadata.texture, PANEL_TEXTURE_FILE
            ));
        }

        let texture = assets.register_png(&metadata.id, PANEL_PNG)?;
        let source_size_px = assets
            .resource(texture)
            .map(|resource| [resource.image.width(), resource.image.height()])
            .ok_or_else(|| format!("UI panel {} texture registration was lost", metadata.id))?;
        validate_metadata(&metadata, source_size_px)?;

        Ok(Self {
            texture,
            source_size_px,
            slice_px: metadata.slice_px,
            border_units: metadata.border_units,
        })
    }

    pub(crate) fn proof_regions(
        self,
        mode: ProofPanelMode,
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> Result<Vec<UiTexturedRect>, String> {
        let Some(logical_size) = mode.logical_size() else {
            return Ok(Vec::new());
        };
        if !pixels_per_unit.is_finite() || pixels_per_unit <= 0.0 {
            return Err("UI panel pixels-per-unit must be finite and positive".to_string());
        }
        let size = [
            logical_size[0] * pixels_per_unit,
            logical_size[1] * pixels_per_unit,
        ];
        let center = [
            viewport.x as f32 + viewport.width as f32 * 0.5,
            viewport.y as f32 + viewport.height as f32 * 0.5,
        ];
        let destination = ScreenRect {
            min: [center[0] - size[0] * 0.5, center[1] - size[1] * 0.5],
            max: [center[0] + size[0] * 0.5, center[1] + size[1] * 0.5],
        };
        assemble_nine_slice(
            destination,
            self.texture,
            self.source_size_px,
            self.slice_px,
            self.border_units.scaled(pixels_per_unit),
            [1.0; 4],
        )
    }
}

fn validate_metadata(metadata: &UiPanelMetadata, source_size_px: [u32; 2]) -> Result<(), String> {
    if metadata.schema_version != 1 {
        return Err(format!(
            "UI panel {} has unsupported schema_version {}",
            metadata.id, metadata.schema_version
        ));
    }
    if metadata.id.trim().is_empty() {
        return Err("UI panel metadata id must not be empty".to_string());
    }
    if source_size_px.contains(&0)
        || metadata
            .slice_px
            .left
            .saturating_add(metadata.slice_px.right)
            >= source_size_px[0]
        || metadata
            .slice_px
            .top
            .saturating_add(metadata.slice_px.bottom)
            >= source_size_px[1]
    {
        return Err(format!(
            "UI panel {} slice geometry {:?} is outside texture {}x{}",
            metadata.id, metadata.slice_px, source_size_px[0], source_size_px[1]
        ));
    }
    let borders = metadata.border_units;
    if ![borders.left, borders.right, borders.top, borders.bottom]
        .into_iter()
        .all(|value| value.is_finite() && value > 0.0)
    {
        return Err(format!(
            "UI panel {} border_units must be finite and positive",
            metadata.id
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScreenRect {
    min: [f32; 2],
    max: [f32; 2],
}

fn assemble_nine_slice(
    destination: ScreenRect,
    texture: SpriteTextureId,
    source_size_px: [u32; 2],
    source: SourceInsets,
    borders: DestinationBorders,
    tint: [f32; 4],
) -> Result<Vec<UiTexturedRect>, String> {
    let width = destination.max[0] - destination.min[0];
    let height = destination.max[1] - destination.min[1];
    if !destination
        .min
        .into_iter()
        .chain(destination.max)
        .all(f32::is_finite)
        || width <= borders.left + borders.right
        || height <= borders.top + borders.bottom
    {
        return Err(
            "UI panel target is invalid or smaller than its destination borders".to_string(),
        );
    }

    let source_x = [
        0.0,
        source.left as f32,
        (source_size_px[0] - source.right) as f32,
        source_size_px[0] as f32,
    ];
    let source_y = [
        0.0,
        source.top as f32,
        (source_size_px[1] - source.bottom) as f32,
        source_size_px[1] as f32,
    ];
    let destination_x = [
        destination.min[0],
        destination.min[0] + borders.left,
        destination.max[0] - borders.right,
        destination.max[0],
    ];
    let destination_y = [
        destination.min[1],
        destination.min[1] + borders.top,
        destination.max[1] - borders.bottom,
        destination.max[1],
    ];
    let source_width = source_size_px[0] as f32;
    let source_height = source_size_px[1] as f32;

    let mut regions = Vec::with_capacity(9);
    for row in 0..3 {
        for column in 0..3 {
            regions.push(UiTexturedRect {
                min: [destination_x[column], destination_y[row]],
                max: [destination_x[column + 1], destination_y[row + 1]],
                texture,
                uv_min: [
                    source_x[column] / source_width,
                    source_y[row] / source_height,
                ],
                uv_max: [
                    source_x[column + 1] / source_width,
                    source_y[row + 1] / source_height,
                ],
                tint,
            });
        }
    }
    Ok(regions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn embedded_asset() -> UiPanelAsset {
        UiPanelAsset::load_embedded(&mut AssetRuntime::new()).unwrap()
    }

    fn viewport() -> PixelViewport {
        PixelViewport {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        }
    }

    #[test]
    fn embedded_metadata_parses_and_validates_against_decoded_texture() {
        let mut assets = AssetRuntime::new();
        let panel = UiPanelAsset::load_embedded(&mut assets).unwrap();
        assert_eq!(panel.source_size_px, [1254, 1254]);
        assert_eq!(panel.slice_px.left, 128);
        assert_eq!(assets.resource_count(), 1);
        assert_eq!(
            assets.resource(panel.texture).unwrap().image.dimensions(),
            (1254, 1254)
        );
    }

    #[test]
    fn invalid_slice_metadata_is_rejected() {
        let invalid: UiPanelMetadata = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "id": "ui.panel.invalid",
                "texture": "panel.png",
                "slice_px": { "left": 700, "right": 600, "top": 128, "bottom": 128 },
                "border_units": { "left": 32.0, "right": 32.0, "top": 32.0, "bottom": 32.0 }
            }"#,
        )
        .unwrap();
        assert!(validate_metadata(&invalid, [1254, 1254]).is_err());
    }

    #[test]
    fn nine_slice_partitions_destination_and_keeps_corners_constant() {
        let panel = embedded_asset();
        let normal = panel
            .proof_regions(ProofPanelMode::Normal, viewport(), 1.0)
            .unwrap();
        let double = panel
            .proof_regions(ProofPanelMode::Double, viewport(), 1.0)
            .unwrap();
        assert_eq!(normal.len(), 9);
        assert_eq!(double.len(), 9);
        assert_eq!(normal[0].size(), [32.0, 32.0]);
        assert_eq!(double[0].size(), [32.0, 32.0]);
        assert_eq!(normal[8].size(), [32.0, 32.0]);
        assert_eq!(double[8].size(), [32.0, 32.0]);
        assert_eq!(normal[1].size(), [256.0, 32.0]);
        assert_eq!(double[1].size(), [576.0, 32.0]);
        assert_eq!(normal[3].size(), [32.0, 176.0]);
        assert_eq!(double[3].size(), [32.0, 416.0]);
        assert_eq!(normal[4].size(), [256.0, 176.0]);
        assert_eq!(double[4].size(), [576.0, 416.0]);
        assert_eq!(normal[0].min, [480.0, 240.0]);
        assert_eq!(normal[8].max, [800.0, 480.0]);
        assert_eq!(double[0].min, [320.0, 120.0]);
        assert_eq!(double[8].max, [960.0, 600.0]);
    }

    #[test]
    fn nine_slice_uvs_stay_in_range_and_partition_source() {
        let regions = embedded_asset()
            .proof_regions(ProofPanelMode::Normal, viewport(), 1.0)
            .unwrap();
        assert!(regions.iter().all(|region| {
            region
                .uv_min
                .into_iter()
                .chain(region.uv_max)
                .all(|value| (0.0..=1.0).contains(&value))
        }));
        assert_eq!(regions[0].uv_min, [0.0, 0.0]);
        assert_eq!(regions[8].uv_max, [1.0, 1.0]);
        assert_eq!(regions[0].uv_max[0], regions[1].uv_min[0]);
        assert_eq!(regions[1].uv_max[0], regions[2].uv_min[0]);
        assert_eq!(regions[0].uv_max[1], regions[3].uv_min[1]);
        assert_eq!(regions[3].uv_max[1], regions[6].uv_min[1]);
    }

    #[test]
    fn target_smaller_than_destination_borders_is_rejected() {
        let panel = embedded_asset();
        let result = assemble_nine_slice(
            ScreenRect {
                min: [0.0, 0.0],
                max: [64.0, 63.0],
            },
            panel.texture,
            panel.source_size_px,
            panel.slice_px,
            panel.border_units,
            [1.0; 4],
        );
        assert!(result.is_err());
    }

    #[test]
    fn proof_keyboard_transitions_are_deterministic_and_ignore_release_repeat() {
        let mut mode = ProofPanelMode::Hidden;
        assert!(!mode.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Released,
            false
        ));
        assert!(!mode.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            true
        ));
        assert_eq!(mode, ProofPanelMode::Hidden);

        assert!(mode.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        assert_eq!(mode, ProofPanelMode::Normal);
        assert!(mode.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        assert_eq!(mode, ProofPanelMode::Hidden);

        assert!(mode.apply_key(
            PhysicalKey::Code(KeyCode::KeyO),
            ElementState::Pressed,
            false
        ));
        assert_eq!(mode, ProofPanelMode::Double);
        assert!(mode.apply_key(
            PhysicalKey::Code(KeyCode::KeyO),
            ElementState::Pressed,
            false
        ));
        assert_eq!(mode, ProofPanelMode::Hidden);

        assert!(mode.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        assert!(mode.apply_key(
            PhysicalKey::Code(KeyCode::KeyO),
            ElementState::Pressed,
            false
        ));
        assert_eq!(mode, ProofPanelMode::Double);
        assert!(mode.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        assert_eq!(mode, ProofPanelMode::Normal);
        assert!(!mode.apply_key(
            PhysicalKey::Code(KeyCode::KeyP),
            ElementState::Pressed,
            false
        ));
        assert_eq!(mode, ProofPanelMode::Normal);
    }
}
