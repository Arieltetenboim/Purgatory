//! UI Kit V2 metadata and logical asset loading.
//!
//! Callers ask for a logical name such as `panel_window_9slice`.
//! Manifest paths are resolved here and decoded through `ClientAssetLoader`.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::asset_runtime::AssetRuntime;
use crate::assets::ClientAssetLoader;
use crate::renderer::SpriteTextureId;
#[cfg(feature = "dev-diagnostics")]
use crate::renderer::{PixelViewport, UiTexturedQuad};
#[cfg(feature = "dev-diagnostics")]
use crate::ui_panel::compose_standalone_nine_slice;

const MANIFEST_VERSION: u32 = 2;
const MANIFEST_UNITS: &str = "pixels";
const MANIFEST_SLICE_ORDER: &str = "left,top,right,bottom";
const MANIFEST_RELATIVE: &str = "ui/asset_manifest.json";
const PROOF_ASSET: &str = "panel_window_9slice";
const PROOF_MARGIN_UNITS: f32 = 24.0;
const PROOF_GAP_UNITS: f32 = 20.0;
const PROOF_WIDE_UNITS: f32 = 420.0;
const PROOF_TALL_UNITS: f32 = 340.0;

#[derive(Debug, Deserialize)]
struct RawManifest {
    version: u32,
    units: String,
    #[serde(rename = "sliceOrder")]
    slice_order: String,
    assets: HashMap<String, RawAsset>,
}

#[derive(Debug, Deserialize)]
struct RawAsset {
    file: String,
    width: u32,
    height: u32,
    #[serde(default, rename = "sliceLTRB")]
    slice_ltrb: Option<[u32; 4]>,
    #[serde(default, rename = "contentOverlap")]
    content_overlap: Option<u32>,
}

#[derive(Debug)]
struct UiV2AssetRecord {
    width: u32,
    height: u32,
    slice_ltrb: Option<[u32; 4]>,
    content_overlap: Option<u32>,
    graphic_relative: PathBuf,
}

#[derive(Debug)]
pub(crate) struct UiV2Catalog {
    assets: HashMap<String, UiV2AssetRecord>,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UiV2AssetInfo {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) slice_ltrb: Option<[u32; 4]>,
    pub(crate) content_overlap: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiV2NineSlice {
    pub(crate) texture: SpriteTextureId,
    pub(crate) size_px: [u32; 2],
    pub(crate) slice_ltrb: [u32; 4],
}

/// One standalone V2 image. Callers address it by logical name.
#[derive(Clone, Copy, Debug)]
pub(crate) struct UiV2Image {
    pub(crate) texture: SpriteTextureId,
    pub(crate) size_px: [u32; 2],
}

impl UiV2Catalog {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn get(&self, name: &str) -> Result<UiV2AssetInfo, String> {
        let record = self.record(name)?;
        Ok(UiV2AssetInfo {
            width: record.width,
            height: record.height,
            slice_ltrb: record.slice_ltrb,
            content_overlap: record.content_overlap,
        })
    }

    fn record(&self, name: &str) -> Result<&UiV2AssetRecord, String> {
        self.assets
            .get(name)
            .ok_or_else(|| format!("UI V2 asset {name} is not in the manifest"))
    }
}

pub(crate) fn parse_ui_v2_catalog(json: &str) -> Result<UiV2Catalog, String> {
    let raw: RawManifest =
        serde_json::from_str(json).map_err(|error| format!("parse UI V2 manifest: {error}"))?;
    if raw.version != MANIFEST_VERSION {
        return Err(format!(
            "UI V2 manifest has unsupported version {}; expected {MANIFEST_VERSION}",
            raw.version
        ));
    }
    if raw.units != MANIFEST_UNITS {
        return Err(format!(
            "UI V2 manifest units must be {MANIFEST_UNITS}, found {}",
            raw.units
        ));
    }
    if raw.slice_order != MANIFEST_SLICE_ORDER {
        return Err(format!(
            "UI V2 manifest sliceOrder must be {MANIFEST_SLICE_ORDER}, found {}",
            raw.slice_order
        ));
    }

    let mut assets = HashMap::with_capacity(raw.assets.len());
    for (name, asset) in raw.assets {
        if name.is_empty() {
            return Err("UI V2 manifest contains an empty asset name".to_string());
        }
        if asset.width == 0 || asset.height == 0 {
            return Err(format!(
                "UI V2 asset {name} has invalid dimensions {}x{}",
                asset.width, asset.height
            ));
        }
        if let Some(slice) = asset.slice_ltrb {
            let [left, top, right, bottom] = slice;
            if left.saturating_add(right) >= asset.width
                || top.saturating_add(bottom) >= asset.height
            {
                return Err(format!(
                    "UI V2 asset {name} sliceLTRB {slice:?} does not fit {}x{} ({})",
                    asset.width, asset.height, asset.file
                ));
            }
        }
        let graphic_relative = ui_graphic_relative(&name, &asset.file)?;
        assets.insert(
            name,
            UiV2AssetRecord {
                width: asset.width,
                height: asset.height,
                slice_ltrb: asset.slice_ltrb,
                content_overlap: asset.content_overlap,
                graphic_relative,
            },
        );
    }
    Ok(UiV2Catalog { assets })
}

pub(crate) fn load_ui_v2_nine_slice(
    loader: &mut ClientAssetLoader<'_>,
    catalog: &UiV2Catalog,
    name: &str,
) -> Result<UiV2NineSlice, String> {
    let record = catalog.record(name)?;
    let Some(slice_ltrb) = record.slice_ltrb else {
        return Err(format!(
            "UI V2 asset {name} is missing sliceLTRB ({})",
            record.graphic_relative.display()
        ));
    };
    let texture = loader.load_png(name, &record.graphic_relative)?;
    let image = &loader
        .runtime()
        .resource(texture)
        .ok_or_else(|| format!("UI V2 asset {name}: registered texture missing"))?
        .image;
    if image.width() != record.width || image.height() != record.height {
        return Err(format!(
            "UI V2 asset {name} PNG is {}x{}, manifest says {}x{} ({})",
            image.width(),
            image.height(),
            record.width,
            record.height,
            record.graphic_relative.display()
        ));
    }
    Ok(UiV2NineSlice {
        texture,
        size_px: [record.width, record.height],
        slice_ltrb,
    })
}

pub(crate) fn load_ui_v2_catalog(runtime: &mut AssetRuntime) -> Result<UiV2Catalog, String> {
    let manifest = {
        let loader = ClientAssetLoader::new(runtime);
        loader
            .read_relative(MANIFEST_RELATIVE)
            .map_err(|error| format!("UI V2 manifest: {error}"))?
    };
    let text = String::from_utf8(manifest)
        .map_err(|error| format!("UI V2 manifest {MANIFEST_RELATIVE} is not UTF-8: {error}"))?;
    parse_ui_v2_catalog(&text)
}

/// Load one standalone image by logical manifest name.
pub(crate) fn load_ui_v2_image(
    loader: &mut ClientAssetLoader<'_>,
    catalog: &UiV2Catalog,
    name: &str,
) -> Result<UiV2Image, String> {
    let record = catalog.record(name)?;
    let texture = loader.load_png(name, &record.graphic_relative)?;
    let image = &loader
        .runtime()
        .resource(texture)
        .ok_or_else(|| format!("UI V2 asset {name}: registered texture missing"))?
        .image;
    if image.width() != record.width || image.height() != record.height {
        return Err(format!(
            "UI V2 asset {name} PNG is {}x{}, manifest says {}x{} ({})",
            image.width(),
            image.height(),
            record.width,
            record.height,
            record.graphic_relative.display()
        ));
    }
    Ok(UiV2Image {
        texture,
        size_px: [record.width, record.height],
    })
}

/// Load a visual state family and reject members that do not share dimensions.
pub(crate) fn load_ui_v2_state_family(
    loader: &mut ClientAssetLoader<'_>,
    catalog: &UiV2Catalog,
    names: &[&str],
) -> Result<Vec<UiV2Image>, String> {
    if names.is_empty() {
        return Err("UI V2 state family is empty".to_string());
    }
    let mut loaded: Vec<UiV2Image> = Vec::with_capacity(names.len());
    for name in names {
        let image = load_ui_v2_image(loader, catalog, name)?;
        if let Some(first) = loaded.first()
            && image.size_px != first.size_px
        {
            return Err(format!(
                "UI V2 state {name} is {}x{}, incompatible with {}x{} ({})",
                image.size_px[0], image.size_px[1], first.size_px[0], first.size_px[1], names[0]
            ));
        }
        loaded.push(image);
    }
    Ok(loaded)
}

fn ui_graphic_relative(name: &str, file: &str) -> Result<PathBuf, String> {
    let relative = Path::new(file);
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("UI V2 asset {name} has invalid source {file}"));
    }
    Ok(Path::new("ui").join(relative))
}

/// Temporary gameplay UI DEV proof. Delete with the later V2 gallery.
#[cfg(feature = "dev-diagnostics")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct UiDevProof {
    visible: bool,
    asset: UiV2NineSlice,
}

#[cfg(feature = "dev-diagnostics")]
impl UiDevProof {
    pub(crate) fn load(runtime: &mut AssetRuntime) -> Result<Self, String> {
        let catalog = load_ui_v2_catalog(runtime)?;
        let mut loader = ClientAssetLoader::new(runtime);
        let asset = load_ui_v2_nine_slice(&mut loader, &catalog, PROOF_ASSET)?;
        Ok(Self {
            visible: false,
            asset,
        })
    }

    pub(crate) fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub(crate) fn frame(
        &self,
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> Result<Vec<UiTexturedQuad>, String> {
        if !self.visible {
            return Ok(Vec::new());
        }
        layout_ui_dev_proof(&self.asset, viewport, pixels_per_unit)
    }
}

#[cfg(feature = "dev-diagnostics")]
fn layout_ui_dev_proof(
    asset: &UiV2NineSlice,
    viewport: PixelViewport,
    pixels_per_unit: f32,
) -> Result<Vec<UiTexturedQuad>, String> {
    let native = [asset.size_px[0] as f32, asset.size_px[1] as f32];
    let examples = [
        ([PROOF_MARGIN_UNITS, PROOF_MARGIN_UNITS], native),
        (
            [
                PROOF_MARGIN_UNITS + native[0] + PROOF_GAP_UNITS,
                PROOF_MARGIN_UNITS,
            ],
            [PROOF_WIDE_UNITS.max(native[0] + 32.0), native[1]],
        ),
        (
            [
                PROOF_MARGIN_UNITS,
                PROOF_MARGIN_UNITS + native[1] + PROOF_GAP_UNITS,
            ],
            [native[0], PROOF_TALL_UNITS.max(native[1] + 32.0)],
        ),
    ];
    let mut rects = Vec::with_capacity(examples.len() * 9);
    for (origin_units, size_units) in examples {
        let origin_px = [
            viewport.x as f32 + origin_units[0] * pixels_per_unit,
            viewport.y as f32 + origin_units[1] * pixels_per_unit,
        ];
        rects.extend(compose_standalone_nine_slice(
            origin_px,
            size_units,
            pixels_per_unit,
            asset.texture,
            asset.size_px,
            asset.slice_ltrb,
        )?);
    }
    Ok(rects)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(assets: &str) -> String {
        format!(
            r#"{{"version":2,"units":"pixels","sliceOrder":"left,top,right,bottom","panelAssembly":{{"body":"ignored"}},"assets":{{{assets}}}}}"#
        )
    }

    #[test]
    fn manifest_parses_supported_v2_schema_and_ignores_extra_fields() {
        let catalog = parse_ui_v2_catalog(&manifest(
            r#""panel_window_9slice":{"file":"PNG/panel_window_9slice.png","width":288,"height":224,"vectorLayers":12,"sliceLTRB":[14,64,14,18]}"#,
        ))
        .unwrap();
        let info = catalog.get("panel_window_9slice").unwrap();
        assert_eq!(info.width, 288);
        assert_eq!(info.height, 224);
        assert_eq!(info.slice_ltrb, Some([14, 64, 14, 18]));
    }

    #[test]
    fn manifest_rejects_unsupported_contract_fields() {
        let wrong_version = manifest(r#""panel":{"file":"PNG/panel.png","width":10,"height":10}"#)
            .replace("\"version\":2", "\"version\":1");
        let error = parse_ui_v2_catalog(&wrong_version).unwrap_err();
        assert!(error.contains("unsupported version 1"), "{error}");

        let wrong_units = manifest(r#""panel":{"file":"PNG/panel.png","width":10,"height":10}"#)
            .replace("\"units\":\"pixels\"", "\"units\":\"points\"");
        let error = parse_ui_v2_catalog(&wrong_units).unwrap_err();
        assert!(error.contains("units must be pixels"), "{error}");

        let wrong_order = manifest(r#""panel":{"file":"PNG/panel.png","width":10,"height":10}"#)
            .replace(
                "\"sliceOrder\":\"left,top,right,bottom\"",
                "\"sliceOrder\":\"top,right,bottom,left\"",
            );
        let error = parse_ui_v2_catalog(&wrong_order).unwrap_err();
        assert!(
            error.contains("sliceOrder must be left,top,right,bottom"),
            "{error}"
        );
    }

    #[test]
    fn logical_lookup_reports_missing_and_invalid_slice_by_asset_name() {
        let catalog = parse_ui_v2_catalog(&manifest(
            r#""panel_body_9slice":{"file":"PNG/panel_body_9slice.png","width":40,"height":30,"sliceLTRB":[4,4,4,6]}"#,
        ))
        .unwrap();
        let found = catalog.get("panel_body_9slice").unwrap();
        assert_eq!(found.slice_ltrb, Some([4, 4, 4, 6]));
        let missing = catalog.get("panel_window_9slice").unwrap_err();
        assert!(
            missing.contains("panel_window_9slice") && missing.contains("not in the manifest"),
            "{missing}"
        );

        let error = parse_ui_v2_catalog(&manifest(
            r#""broken_9slice":{"file":"PNG/broken_9slice.png","width":20,"height":10,"sliceLTRB":[12,2,12,2]}"#,
        ))
        .unwrap_err();
        assert!(error.contains("broken_9slice"), "{error}");
        assert!(error.contains("sliceLTRB"), "{error}");
        assert!(error.contains("PNG/broken_9slice.png"), "{error}");
    }

    #[test]
    fn nine_slice_load_rejects_png_dimension_mismatch_and_missing_slice() {
        let root = std::env::temp_dir().join(format!(
            "purgatory-ui-v2-{}-{}",
            std::process::id(),
            unique_fixture()
        ));
        std::fs::create_dir_all(root.join("ui/PNG")).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]))
            .save(root.join("ui/PNG/widget.png"))
            .unwrap();
        let catalog = parse_ui_v2_catalog(&manifest(
            r#""widget":{"file":"PNG/widget.png","width":9,"height":9,"sliceLTRB":[1,1,1,1]}"#,
        ))
        .unwrap();
        let mut runtime = AssetRuntime::new();
        let error = load_ui_v2_nine_slice(
            &mut ClientAssetLoader::with_root(&mut runtime, &root),
            &catalog,
            "widget",
        )
        .unwrap_err();
        assert!(error.contains("widget"), "{error}");
        assert!(error.contains("2x2"), "{error}");
        assert!(error.contains("9x9"), "{error}");
        assert!(error.contains("widget.png"), "{error}");

        let plain = parse_ui_v2_catalog(&manifest(
            r#""widget":{"file":"PNG/widget.png","width":2,"height":2}"#,
        ))
        .unwrap();
        let mut runtime = AssetRuntime::new();
        let error = load_ui_v2_nine_slice(
            &mut ClientAssetLoader::with_root(&mut runtime, &root),
            &plain,
            "widget",
        )
        .unwrap_err();
        assert!(error.contains("widget"), "{error}");
        assert!(error.contains("missing sliceLTRB"), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn standalone_image_loads_by_logical_name() {
        let root = std::env::temp_dir().join(format!(
            "purgatory-ui-v2-image-{}-{}",
            std::process::id(),
            unique_fixture()
        ));
        std::fs::create_dir_all(root.join("ui/PNG")).unwrap();
        image::RgbaImage::from_pixel(4, 6, image::Rgba([9, 8, 7, 255]))
            .save(root.join("ui/PNG/slot_normal.png"))
            .unwrap();
        let catalog = parse_ui_v2_catalog(&manifest(
            r#""slot_normal":{"file":"PNG/slot_normal.png","width":4,"height":6,"contentOverlap":3}"#,
        ))
        .unwrap();
        assert_eq!(catalog.get("slot_normal").unwrap().content_overlap, Some(3));
        let mut runtime = AssetRuntime::new();
        let image = load_ui_v2_image(
            &mut ClientAssetLoader::with_root(&mut runtime, &root),
            &catalog,
            "slot_normal",
        )
        .unwrap();
        assert_eq!(image.size_px, [4, 6]);
        assert_eq!(runtime.texture_for_key("slot_normal"), Some(image.texture));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn state_family_rejects_incompatible_dimensions() {
        let root = std::env::temp_dir().join(format!(
            "purgatory-ui-v2-family-{}-{}",
            std::process::id(),
            unique_fixture()
        ));
        std::fs::create_dir_all(root.join("ui/PNG")).unwrap();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 1, 1, 255]))
            .save(root.join("ui/PNG/slot_normal.png"))
            .unwrap();
        image::RgbaImage::from_pixel(5, 4, image::Rgba([2, 2, 2, 255]))
            .save(root.join("ui/PNG/slot_hover.png"))
            .unwrap();
        let catalog = parse_ui_v2_catalog(&manifest(
            r#""slot_normal":{"file":"PNG/slot_normal.png","width":4,"height":4},"slot_hover":{"file":"PNG/slot_hover.png","width":5,"height":4}"#,
        ))
        .unwrap();
        let mut runtime = AssetRuntime::new();
        let error = load_ui_v2_state_family(
            &mut ClientAssetLoader::with_root(&mut runtime, &root),
            &catalog,
            &["slot_normal", "slot_hover"],
        )
        .unwrap_err();
        assert!(error.contains("slot_hover"), "{error}");
        assert!(error.contains("incompatible"), "{error}");
        assert!(error.contains("slot_normal"), "{error}");

        image::RgbaImage::from_pixel(4, 4, image::Rgba([3, 3, 3, 255]))
            .save(root.join("ui/PNG/slot_hover.png"))
            .unwrap();
        let catalog = parse_ui_v2_catalog(&manifest(
            r#""slot_normal":{"file":"PNG/slot_normal.png","width":4,"height":4},"slot_hover":{"file":"PNG/slot_hover.png","width":4,"height":4}"#,
        ))
        .unwrap();
        let mut runtime = AssetRuntime::new();
        let family = load_ui_v2_state_family(
            &mut ClientAssetLoader::with_root(&mut runtime, &root),
            &catalog,
            &["slot_normal", "slot_hover"],
        )
        .unwrap();
        assert_eq!(family.len(), 2);
        assert_eq!(family[0].size_px, family[1].size_px);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn checkout_panel_window_png_matches_manifest_dimensions() {
        let mut runtime = AssetRuntime::new();
        let mut loader = ClientAssetLoader::new(&mut runtime);
        let bytes = loader.read_relative(MANIFEST_RELATIVE).unwrap();
        let catalog = parse_ui_v2_catalog(std::str::from_utf8(&bytes).unwrap()).unwrap();
        let asset = load_ui_v2_nine_slice(&mut loader, &catalog, PROOF_ASSET).unwrap();
        let image = runtime.resource(asset.texture).unwrap();
        assert_eq!(
            image.image.dimensions(),
            (asset.size_px[0], asset.size_px[1])
        );
        assert!(asset.slice_ltrb[0] + asset.slice_ltrb[2] < asset.size_px[0]);
        assert!(asset.slice_ltrb[1] + asset.slice_ltrb[3] < asset.size_px[1]);
        assert_eq!(runtime.texture_for_key(PROOF_ASSET), Some(asset.texture));
    }

    #[cfg(feature = "dev-diagnostics")]
    fn piece_size(piece: &UiTexturedQuad) -> [f32; 2] {
        [
            piece.corners[2][0] - piece.corners[0][0],
            piece.corners[2][1] - piece.corners[0][1],
        ]
    }

    #[cfg(feature = "dev-diagnostics")]
    fn assert_stretch_quad(piece: &UiTexturedQuad) {
        assert_eq!(piece.uvs[1], [piece.uvs[2][0], piece.uvs[0][1]]);
        assert_eq!(piece.uvs[3], [piece.uvs[0][0], piece.uvs[2][1]]);
        assert!(piece.uvs[2][0] > piece.uvs[0][0]);
        assert!(piece.uvs[2][1] > piece.uvs[0][1]);
    }

    fn unique_fixture() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }

    #[cfg(feature = "dev-diagnostics")]
    #[test]
    fn ui_dev_proof_draws_three_sizes_from_asset_metadata_and_toggles() {
        let asset = UiV2NineSlice {
            texture: SpriteTextureId::from_raw(9),
            size_px: [80, 50],
            slice_ltrb: [5, 7, 6, 4],
        };
        let mut proof = UiDevProof {
            visible: false,
            asset,
        };
        let viewport = PixelViewport {
            x: 3,
            y: 4,
            width: 1280,
            height: 720,
        };
        assert!(proof.frame(viewport, 1.0).unwrap().is_empty());
        proof.toggle();
        let pieces = proof.frame(viewport, 1.0).unwrap();
        assert_eq!(pieces.len(), 27);
        assert!(pieces.iter().all(|piece| piece.texture == asset.texture));
        pieces.iter().for_each(assert_stretch_quad);
        assert_eq!(piece_size(&pieces[0]), [5.0, 7.0]);
        assert_eq!(piece_size(&pieces[9]), piece_size(&pieces[0]));
        assert_eq!(piece_size(&pieces[18]), piece_size(&pieces[0]));
        assert_eq!(pieces[0].uvs, pieces[9].uvs);
        assert_eq!(pieces[0].uvs, pieces[18].uvs);
        assert!(piece_size(&pieces[10])[0] > piece_size(&pieces[1])[0]);
        assert_eq!(piece_size(&pieces[10])[1], piece_size(&pieces[1])[1]);
        assert!(piece_size(&pieces[21])[1] > piece_size(&pieces[3])[1]);
        assert_eq!(piece_size(&pieces[21])[0], piece_size(&pieces[3])[0]);
        let native_width =
            piece_size(&pieces[0])[0] + piece_size(&pieces[1])[0] + piece_size(&pieces[2])[0];
        let native_height =
            piece_size(&pieces[0])[1] + piece_size(&pieces[3])[1] + piece_size(&pieces[6])[1];
        assert!((native_width - 80.0).abs() < 0.001);
        assert!((native_height - 50.0).abs() < 0.001);
        let wide_width =
            piece_size(&pieces[9])[0] + piece_size(&pieces[10])[0] + piece_size(&pieces[11])[0];
        let tall_height =
            piece_size(&pieces[18])[1] + piece_size(&pieces[21])[1] + piece_size(&pieces[24])[1];
        assert!((wide_width - PROOF_WIDE_UNITS).abs() < 0.001);
        assert!((tall_height - PROOF_TALL_UNITS).abs() < 0.001);

        let scaled = proof.frame(viewport, 1.5).unwrap();
        assert!((piece_size(&scaled[0])[0] - 7.5).abs() < 0.001);
        assert!((piece_size(&scaled[0])[1] - 10.5).abs() < 0.001);
        assert_eq!(scaled[0].uvs, pieces[0].uvs);
        let scaled_native =
            piece_size(&scaled[0])[0] + piece_size(&scaled[1])[0] + piece_size(&scaled[2])[0];
        assert!((scaled_native - 120.0).abs() < 0.001);
        proof.toggle();
        assert!(proof.frame(viewport, 1.5).unwrap().is_empty());
    }
}
