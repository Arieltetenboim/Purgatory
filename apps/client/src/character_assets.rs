//! Character Visual Pack v1 parsing and resolution.
//!
//! This is character-domain validation over the reusable client asset runtime.
//! It does not select painter order or emit draw commands.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::asset_runtime::{AssetRuntime, ResolvedVisual};

const PACK_KIND: &str = "purgatory_visual_pack";
const PACK_ID: &str = "character.base.dev_01";
const PACK_RIG: &str = "humanoid_v0";
const PACK_VIEW: &str = "side";
const PACK_ATLAS_KEY: &str = "character.base.dev_01.side.atlas";
const VISUAL_PARTS: &[&str] = &[
    "upper_arm_back",
    "lower_arm_back",
    "hand_back",
    "upper_leg_back",
    "lower_leg_back",
    "torso",
    "upper_leg_front",
    "lower_leg_front",
    "head",
    "upper_arm_front",
    "lower_arm_front",
    "hand_front",
];
const EMBEDDED_MANIFEST: &[u8] =
    include_bytes!("../../../Graphic/character/base/character.base.dev_01.visual-pack.json");
const EMBEDDED_ATLAS: &[u8] =
    include_bytes!("../../../Graphic/character/base/character.base.dev_01.side.atlas.png");

#[derive(Debug)]
pub(crate) struct CharacterVisualPack {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) completeness: String,
    visuals: Vec<ResolvedVisual>,
    indices: HashMap<String, usize>,
}

impl CharacterVisualPack {
    pub(crate) fn visual(&self, key: &str) -> Option<&ResolvedVisual> {
        self.indices.get(key).map(|&index| &self.visuals[index])
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn visual_count(&self) -> usize {
        self.visuals.len()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CharacterVisualPackError {
    Json(String),
    Unsupported(&'static str, String),
    Invalid(String),
    AtlasDecode(String),
}

impl std::fmt::Display for CharacterVisualPackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(f, "visual pack JSON: {error}"),
            Self::Unsupported(field, value) => write!(f, "unsupported {field}: {value}"),
            Self::Invalid(error) => f.write_str(error),
            Self::AtlasDecode(error) => write!(f, "visual pack atlas: {error}"),
        }
    }
}

impl std::error::Error for CharacterVisualPackError {}

#[derive(Deserialize)]
struct RawPack {
    schema_version: u32,
    kind: String,
    id: String,
    rig: String,
    view: String,
    pixels_per_unit: f32,
    completeness: String,
    atlas: RawAtlas,
    visuals: Vec<RawVisual>,
    base_appearance: HashMap<String, String>,
    draw_order: Vec<String>,
}

#[derive(Deserialize)]
struct RawAtlas {
    file: String,
    width: u32,
    height: u32,
    padding_px: u32,
}

#[derive(Deserialize)]
struct RawVisual {
    part: String,
    bone: String,
    visual_key: String,
    rect_px: [u32; 4],
    pivot_px: [f32; 2],
}

pub(crate) fn embedded_character_visual_pack(
    assets: &mut AssetRuntime,
) -> Result<CharacterVisualPack, CharacterVisualPackError> {
    load_character_visual_pack(EMBEDDED_MANIFEST, EMBEDDED_ATLAS, assets)
}

pub(crate) fn load_character_visual_pack(
    manifest: &[u8],
    atlas: &[u8],
    assets: &mut AssetRuntime,
) -> Result<CharacterVisualPack, CharacterVisualPackError> {
    let raw: RawPack = serde_json::from_slice(manifest)
        .map_err(|error| CharacterVisualPackError::Json(error.to_string()))?;
    validate_header(&raw)?;
    validate_atlas_declaration(&raw.atlas)?;
    if raw.completeness != "partial_dev" {
        return Err(CharacterVisualPackError::Unsupported(
            "completeness",
            raw.completeness,
        ));
    }
    let decoded = image::load_from_memory(atlas)
        .map_err(|error| CharacterVisualPackError::AtlasDecode(error.to_string()))?
        .to_rgba8();
    if decoded.width() != raw.atlas.width || decoded.height() != raw.atlas.height {
        return Err(CharacterVisualPackError::Invalid(format!(
            "atlas dimensions declare {}x{} but decode as {}x{}",
            raw.atlas.width,
            raw.atlas.height,
            decoded.width(),
            decoded.height()
        )));
    }
    let texture = assets
        .register_png(PACK_ATLAS_KEY, atlas)
        .map_err(CharacterVisualPackError::AtlasDecode)?;
    let mut keys = HashSet::new();
    let mut visuals = Vec::with_capacity(raw.visuals.len());
    let mut indices = HashMap::with_capacity(raw.visuals.len());
    for visual in raw.visuals {
        validate_visual(&visual, raw.atlas.width, raw.atlas.height, &mut keys)?;
        let index = visuals.len();
        let [x, y, width, height] = visual.rect_px;
        visuals.push(ResolvedVisual {
            texture,
            rect_px: visual.rect_px,
            uv: gpu_uvs(x, y, width, height, raw.atlas.width, raw.atlas.height),
            pivot_px: visual.pivot_px,
            dimensions_px: [width, height],
            pixels_per_unit: raw.pixels_per_unit,
        });
        assets
            .register_visual(&visual.visual_key, visuals[index])
            .map_err(CharacterVisualPackError::Invalid)?;
        indices.insert(visual.visual_key, index);
    }
    for (part, key) in &raw.base_appearance {
        if !keys.contains(key) {
            return Err(CharacterVisualPackError::Invalid(format!(
                "base_appearance part {part} references missing visual {key}"
            )));
        }
    }
    let _ = raw.draw_order;
    Ok(CharacterVisualPack {
        completeness: raw.completeness,
        visuals,
        indices,
    })
}

fn validate_header(raw: &RawPack) -> Result<(), CharacterVisualPackError> {
    if raw.schema_version != 1 {
        return Err(CharacterVisualPackError::Unsupported(
            "schema_version",
            raw.schema_version.to_string(),
        ));
    }
    if raw.id != PACK_ID {
        return Err(CharacterVisualPackError::Unsupported("id", raw.id.clone()));
    }
    for (field, actual, expected) in [
        ("kind", raw.kind.as_str(), PACK_KIND),
        ("rig", raw.rig.as_str(), PACK_RIG),
        ("view", raw.view.as_str(), PACK_VIEW),
    ] {
        if actual != expected {
            return Err(CharacterVisualPackError::Unsupported(
                field,
                actual.to_owned(),
            ));
        }
    }
    if !raw.pixels_per_unit.is_finite() || raw.pixels_per_unit <= 0.0 {
        return Err(CharacterVisualPackError::Invalid(
            "pixels_per_unit must be positive and finite".to_owned(),
        ));
    }
    Ok(())
}

fn validate_atlas_declaration(atlas: &RawAtlas) -> Result<(), CharacterVisualPackError> {
    if atlas.file != "character.base.dev_01.side.atlas.png" {
        return Err(CharacterVisualPackError::Invalid(format!(
            "unexpected atlas file {}",
            atlas.file
        )));
    }
    if atlas.width == 0 || atlas.height == 0 {
        return Err(CharacterVisualPackError::Invalid(
            "atlas dimensions must be positive".to_owned(),
        ));
    }
    let _ = atlas.padding_px;
    Ok(())
}

fn validate_visual(
    visual: &RawVisual,
    atlas_width: u32,
    atlas_height: u32,
    keys: &mut HashSet<String>,
) -> Result<(), CharacterVisualPackError> {
    if !keys.insert(visual.visual_key.clone()) {
        return Err(CharacterVisualPackError::Invalid(format!(
            "duplicate visual key {}",
            visual.visual_key
        )));
    }
    if !VISUAL_PARTS.contains(&visual.part.as_str()) {
        return Err(CharacterVisualPackError::Invalid(format!(
            "invalid part {}",
            visual.part
        )));
    }
    if !purgatory_skeleton::HUMANOID_V0_BONE_LABELS.contains(&visual.bone.as_str()) {
        return Err(CharacterVisualPackError::Invalid(format!(
            "invalid bone {}",
            visual.bone
        )));
    }
    let [x, y, width, height] = visual.rect_px;
    if width == 0
        || height == 0
        || x.checked_add(width).is_none_or(|right| right > atlas_width)
        || y.checked_add(height)
            .is_none_or(|bottom| bottom > atlas_height)
    {
        return Err(CharacterVisualPackError::Invalid(format!(
            "visual {} rect is invalid or outside atlas",
            visual.visual_key
        )));
    }
    if visual.pivot_px.iter().any(|value| !value.is_finite()) {
        return Err(CharacterVisualPackError::Invalid(format!(
            "visual {} pivot must be finite",
            visual.visual_key
        )));
    }
    Ok(())
}

fn gpu_uvs(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    atlas_width: u32,
    atlas_height: u32,
) -> [[f32; 2]; 4] {
    let u0 = x as f32 / atlas_width as f32;
    let u1 = (x + width) as f32 / atlas_width as f32;
    let v0 = y as f32 / atlas_height as f32;
    let v1 = (y + height) as f32 / atlas_height as f32;
    [[u0, v1], [u1, v1], [u1, v0], [u0, v0]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> AssetRuntime {
        AssetRuntime::new()
    }

    fn manifest() -> String {
        String::from_utf8(EMBEDDED_MANIFEST.to_vec()).unwrap()
    }

    fn load(
        json: &str,
        assets: &mut AssetRuntime,
    ) -> Result<CharacterVisualPack, CharacterVisualPackError> {
        load_character_visual_pack(json.as_bytes(), EMBEDDED_ATLAS, assets)
    }

    #[test]
    fn current_embedded_pack_loads_and_resolves_metadata() {
        let mut assets = runtime();
        let pack = embedded_character_visual_pack(&mut assets).unwrap();
        assert_eq!(pack.completeness, "partial_dev");
        assert_eq!(pack.visual_count(), 12);
        let visual = pack.visual("character.base.dev_01.head.side").unwrap();
        assert_eq!(
            visual.texture,
            pack.visual("character.base.dev_01.torso.side")
                .unwrap()
                .texture
        );
        assert_eq!(visual.rect_px, [4, 4, 201, 228]);
        assert_eq!(visual.dimensions_px, [201, 228]);
        assert!((visual.pivot_px[0] - 118.45177).abs() < 1e-4);
        assert!((visual.pivot_px[1] - 182.84172).abs() < 1e-4);
        assert_eq!(visual.pixels_per_unit, 256.0);
        assert_eq!(assets.resource_count(), 1);
    }

    #[test]
    fn atlas_dimensions_match_decoded_png() {
        let mut assets = runtime();
        let pack = embedded_character_visual_pack(&mut assets).unwrap();
        let resource = assets
            .resource(
                pack.visual("character.base.dev_01.head.side")
                    .unwrap()
                    .texture,
            )
            .unwrap();
        assert_eq!(
            (resource.image.width(), resource.image.height()),
            (512, 512)
        );
    }

    #[test]
    fn duplicate_visual_key_rejected() {
        let json = manifest().replacen(
            "\"character.base.dev_01.lower_arm_back.side\"",
            "\"character.base.dev_01.upper_arm_back.side\"",
            1,
        );
        assert!(
            matches!(load(&json, &mut runtime()), Err(CharacterVisualPackError::Invalid(error)) if error.contains("duplicate"))
        );
    }

    #[test]
    fn invalid_rect_rejected() {
        let json = manifest().replacen(
            "        86,\n        236,\n        46,\n        51",
            "        500,\n        500,\n        46,\n        51",
            1,
        );
        assert!(
            matches!(load(&json, &mut runtime()), Err(CharacterVisualPackError::Invalid(error)) if error.contains("rect"))
        );
    }

    #[test]
    fn invalid_ppu_rejected() {
        let json = manifest().replacen("\"pixels_per_unit\": 256", "\"pixels_per_unit\": 0", 1);
        assert!(
            matches!(load(&json, &mut runtime()), Err(CharacterVisualPackError::Invalid(error)) if error.contains("pixels_per_unit"))
        );
    }

    #[test]
    fn missing_visual_reference_rejected() {
        let json = manifest().replacen(
            "\"character.base.dev_01.upper_arm_back.side\"",
            "\"missing.visual\"",
            1,
        );
        assert!(
            matches!(load(&json, &mut runtime()), Err(CharacterVisualPackError::Invalid(error)) if error.contains("missing visual"))
        );
    }

    #[test]
    fn outside_and_negative_pivots_are_accepted() {
        let json = manifest().replacen(
            "\"pivot_px\": [21.914373638648044, -8.993649320699319]",
            "\"pivot_px\": [-1000, 1000]",
            1,
        );
        assert!(load(&json, &mut runtime()).is_ok());
    }

    #[test]
    fn unsupported_header_values_rejected() {
        for (field, value) in [
            ("schema_version", "\"schema_version\": 2"),
            ("kind", "\"kind\": \"other\""),
            ("rig", "\"rig\": \"other\""),
            ("view", "\"view\": \"back\""),
        ] {
            let replacement = match field {
                "schema_version" => value,
                "kind" => value,
                "rig" => value,
                _ => value,
            };
            let original = match field {
                "schema_version" => "\"schema_version\": 1",
                "kind" => "\"kind\": \"purgatory_visual_pack\"",
                "rig" => "\"rig\": \"humanoid_v0\"",
                _ => "\"view\": \"side\"",
            };
            let json = manifest().replacen(original, replacement, 1);
            assert!(matches!(
                load(&json, &mut runtime()),
                Err(CharacterVisualPackError::Unsupported(_, _))
            ));
        }
    }

    #[test]
    fn registering_same_resource_key_reuses_identity() {
        let mut assets = runtime();
        let first = assets.register_png("same", EMBEDDED_ATLAS).unwrap();
        let second = assets.register_png("same", b"not decoded again").unwrap();
        assert_eq!(first, second);
        assert_eq!(assets.resource_count(), 1);
    }
}
