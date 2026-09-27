//! Map Lab-owned environment authoring.
//!
//! Tiled owns static world composition. This file owns map-level presentation
//! behavior such as sky gradients and camera-relative parallax layers.

use serde::{Deserialize, Serialize};

pub const MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION: u32 = 1;
pub const MAP_ENVIRONMENT_PRESENTATION_SCHEMA_VERSION: u32 = 1;
pub const CLOUDS_PER_VIEWPORT_AT_FULL_DENSITY: f32 = 12.0;
pub const MAX_CLOUDS_PER_FIELD: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SkyGradient {
    pub top_rgba: [u8; 4],
    pub bottom_rgba: [u8; 4],
}

impl Default for SkyGradient {
    fn default() -> Self {
        Self {
            top_rgba: [104, 155, 214, 255],
            bottom_rgba: [232, 214, 188, 255],
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParallaxDepth {
    Sky,
    Far,
    Mid,
    Near,
}

impl ParallaxDepth {
    pub const ALL: [Self; 4] = [Self::Sky, Self::Far, Self::Mid, Self::Near];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sky => "Sky",
            Self::Far => "Far",
            Self::Mid => "Mid",
            Self::Near => "Near",
        }
    }

    #[must_use]
    pub const fn default_parallax(self) -> f32 {
        match self {
            Self::Sky => 0.0,
            Self::Far => 0.18,
            Self::Mid => 0.38,
            Self::Near => 0.68,
        }
    }

    #[must_use]
    pub const fn default_cloud_scale_range(self) -> [f32; 2] {
        match self {
            Self::Sky => [0.35, 0.55],
            Self::Far => [0.50, 0.85],
            Self::Mid => [0.80, 1.20],
            Self::Near => [1.20, 1.75],
        }
    }

    #[must_use]
    pub const fn default_cloud_speed_range(self) -> [f32; 2] {
        match self {
            Self::Sky => [0.04, 0.08],
            Self::Far => [0.08, 0.16],
            Self::Mid => [0.16, 0.28],
            Self::Near => [0.28, 0.48],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParallaxFillMode {
    Natural,
    #[default]
    Repeat,
    Stretch,
    Fit,
    Cover,
}

impl ParallaxFillMode {
    pub const ALL: [Self; 5] = [
        Self::Natural,
        Self::Repeat,
        Self::Stretch,
        Self::Fit,
        Self::Cover,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Natural => "Natural",
            Self::Repeat => "Repeat",
            Self::Stretch => "Stretch",
            Self::Fit => "Fit",
            Self::Cover => "Cover",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParallaxLayer {
    pub id: String,
    pub asset_path: String,
    pub depth: ParallaxDepth,
    #[serde(default)]
    pub fill_mode: ParallaxFillMode,
    /// 0 = screen-fixed, 1 = world-locked.
    pub parallax: f32,
    #[serde(default)]
    pub offset_world: [f32; 2],
    /// Presentation-only linear drift in world units per second.
    #[serde(default)]
    pub motion_world_per_second: [f32; 2],
    #[serde(default)]
    pub repeat_x: bool,
    #[serde(default)]
    pub repeat_y: bool,
    pub opacity: f32,
}

impl ParallaxLayer {
    /// Effective authored offset after presentation-only environment motion.
    ///
    /// Repeating axes wrap by the rendered tile period so long-running cloud
    /// motion stays numerically bounded and visually seamless.
    #[must_use]
    pub fn animated_offset_world(
        &self,
        elapsed_seconds: f64,
        repeat_period_world: [f32; 2],
    ) -> [f32; 2] {
        std::array::from_fn(|axis| {
            let raw_motion = f64::from(self.motion_world_per_second[axis]) * elapsed_seconds;
            let repeat_axis = self.fill_mode == ParallaxFillMode::Repeat
                && if axis == 0 { self.repeat_x } else { self.repeat_y };
            let period = f64::from(repeat_period_world[axis]);
            let motion = if repeat_axis && period.is_finite() && period > f64::EPSILON {
                raw_motion.rem_euclid(period)
            } else {
                raw_motion
            };
            self.offset_world[axis] + motion as f32
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CloudFieldAuthoring {
    pub id: String,
    /// Directory relative to Graphic/. PNG discovery is non-recursive.
    pub asset_folder: String,
    pub depth: ParallaxDepth,
    /// 0 = screen-fixed, 1 = world-locked.
    pub parallax: f32,
    /// Semantic amount from 0..=1, converted to instance count from coverage width.
    pub density: f32,
    /// Multipliers over each source sprite's natural PPU size.
    pub scale_range: [f32; 2],
    /// Horizontal world-units/second. Signed ranges allow either wind direction.
    pub speed_range: [f32; 2],
    /// Normalized vertical band within the gameplay viewport, 0 = bottom, 1 = top.
    pub height_range: [f32; 2],
    pub opacity_range: [f32; 2],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CloudFieldPresentation {
    pub authored: CloudFieldAuthoring,
    /// Build-resolved Graphic-relative PNG paths. Runtime never scans folders.
    pub asset_paths: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapEnvironmentPresentation {
    pub schema_version: u32,
    pub map_authored: String,
    #[serde(default)]
    pub sky_gradient: Option<SkyGradient>,
    #[serde(default)]
    pub parallax_layers: Vec<ParallaxLayer>,
    #[serde(default)]
    pub cloud_fields: Vec<CloudFieldPresentation>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudInstanceSpec {
    pub asset_index: usize,
    /// Stratified normalized horizontal position in 0..1.
    pub x_unit: f32,
    /// Normalized viewport-height position in 0..1.
    pub height_unit: f32,
    pub scale: f32,
    pub speed_world_per_second: f32,
    pub opacity: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapEnvironmentAuthoring {
    pub schema_version: u32,
    pub map_authored: String,
    #[serde(default)]
    pub sky_gradient: Option<SkyGradient>,
    #[serde(default)]
    pub parallax_layers: Vec<ParallaxLayer>,
    #[serde(default)]
    pub cloud_fields: Vec<CloudFieldAuthoring>,
}

impl MapEnvironmentAuthoring {
    #[must_use]
    pub fn empty(map_authored: impl Into<String>) -> Self {
        Self {
            schema_version: MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION,
            map_authored: map_authored.into(),
            sky_gradient: None,
            parallax_layers: Vec::new(),
            cloud_fields: Vec::new(),
        }
    }
}

#[must_use]
pub fn cloud_instance_count(density: f32, coverage_width: f32, viewport_width: f32) -> usize {
    if !density.is_finite()
        || !coverage_width.is_finite()
        || !viewport_width.is_finite()
        || density <= 0.0
        || coverage_width <= 0.0
        || viewport_width <= 0.0
    {
        return 0;
    }
    let viewports = (coverage_width / viewport_width).max(1.0);
    (density.clamp(0.0, 1.0) * CLOUDS_PER_VIEWPORT_AT_FULL_DENSITY * viewports)
        .round()
        .clamp(0.0, MAX_CLOUDS_PER_FIELD as f32) as usize
}

#[must_use]
pub fn cloud_field_seed(field_id: &str, session_seed: u64) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64 ^ session_seed;
    for byte in field_id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash.max(1)
}

#[must_use]
pub fn cloud_instance_specs(
    field: &CloudFieldAuthoring,
    asset_count: usize,
    seed: u64,
    count: usize,
) -> Vec<CloudInstanceSpec> {
    if asset_count == 0 || count == 0 {
        return Vec::new();
    }
    let mut rng = CloudRng(seed.max(1));
    (0..count)
        .map(|index| {
            let jitter = rng.unit();
            CloudInstanceSpec {
                asset_index: rng.index(asset_count),
                x_unit: (index as f32 + jitter) / count as f32,
                height_unit: rng.range(field.height_range),
                scale: rng.range(field.scale_range),
                speed_world_per_second: rng.range(field.speed_range),
                opacity: rng.range(field.opacity_range),
            }
        })
        .collect()
}

pub fn validate_cloud_field(field: &CloudFieldAuthoring) -> Result<(), String> {
    if field.id.trim().is_empty() {
        return Err("cloud field id must be non-empty".to_owned());
    }
    validate_graphic_relative_path(&field.asset_folder, "cloud asset_folder")?;
    if !field.parallax.is_finite() || !(0.0..=1.0).contains(&field.parallax) {
        return Err(format!(
            "cloud field {} parallax must be within 0..=1",
            field.id
        ));
    }
    if !field.density.is_finite() || !(0.0..=1.0).contains(&field.density) {
        return Err(format!(
            "cloud field {} density must be within 0..=1",
            field.id
        ));
    }
    validate_range(field, "scale_range", field.scale_range, 0.01, f32::INFINITY)?;
    validate_range(
        field,
        "speed_range",
        field.speed_range,
        f32::NEG_INFINITY,
        f32::INFINITY,
    )?;
    validate_range(field, "height_range", field.height_range, 0.0, 1.0)?;
    validate_range(field, "opacity_range", field.opacity_range, 0.0, 1.0)?;
    Ok(())
}

fn validate_range(
    field: &CloudFieldAuthoring,
    name: &str,
    range: [f32; 2],
    minimum: f32,
    maximum: f32,
) -> Result<(), String> {
    if !range.iter().all(|value| value.is_finite())
        || range[0] > range[1]
        || range[0] < minimum
        || range[1] > maximum
    {
        return Err(format!(
            "cloud field {} {name} is invalid: [{}, {}]",
            field.id, range[0], range[1]
        ));
    }
    Ok(())
}

fn validate_graphic_relative_path(value: &str, label: &str) -> Result<(), String> {
    let path = std::path::Path::new(value);
    if value.trim().is_empty()
        || value.contains('\\')
        || value.contains(':')
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "{label} must be a portable Graphic-relative path without traversal: {value:?}"
        ));
    }
    Ok(())
}

#[cfg(feature = "map-authoring")]
pub fn resolve_png_asset_folder(
    graphic_root: &std::path::Path,
    asset_folder: &str,
) -> Result<Vec<String>, String> {
    validate_graphic_relative_path(asset_folder, "cloud asset_folder")?;
    let relative = std::path::Path::new(asset_folder);
    let directory = graphic_root.join(relative);
    if !directory.is_dir() {
        return Err(format!(
            "cloud asset folder does not exist or is not a directory: {}",
            directory.display()
        ));
    }
    let mut paths = Vec::new();
    let entries = std::fs::read_dir(&directory)
        .map_err(|error| format!("read cloud asset folder {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("read cloud asset folder {}: {error}", directory.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("inspect cloud asset {}: {error}", entry.path().display()))?;
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        let is_png = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
        if !is_png {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| format!("cloud asset filename is not UTF-8: {}", path.display()))?;
        paths.push(format!("{}/{}", asset_folder.trim_end_matches('/'), name));
    }
    paths.sort();
    if paths.is_empty() {
        return Err(format!(
            "cloud asset folder {} contains no PNG files",
            directory.display()
        ));
    }
    Ok(paths)
}

#[cfg(feature = "map-authoring")]
pub fn compile_map_environment(
    authoring: &MapEnvironmentAuthoring,
    graphic_root: &std::path::Path,
) -> Result<MapEnvironmentPresentation, String> {
    if authoring.schema_version != MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION {
        return Err(format!(
            "unsupported map environment schema {} (want {})",
            authoring.schema_version, MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for layer in &authoring.parallax_layers {
        if layer.id.trim().is_empty() || !ids.insert(layer.id.as_str()) {
            return Err("environment layer ids must be non-empty and unique".to_owned());
        }
    }

    let mut cloud_fields = Vec::with_capacity(authoring.cloud_fields.len());
    for field in &authoring.cloud_fields {
        validate_cloud_field(field)?;
        if !ids.insert(field.id.as_str()) {
            return Err(format!("duplicate environment layer id {}", field.id));
        }
        cloud_fields.push(CloudFieldPresentation {
            authored: field.clone(),
            asset_paths: resolve_png_asset_folder(graphic_root, &field.asset_folder)?,
        });
    }

    Ok(MapEnvironmentPresentation {
        schema_version: MAP_ENVIRONMENT_PRESENTATION_SCHEMA_VERSION,
        map_authored: authoring.map_authored.clone(),
        sky_gradient: authoring.sky_gradient,
        parallax_layers: authoring.parallax_layers.clone(),
        cloud_fields,
    })
}

#[cfg(feature = "map-authoring")]
pub fn serialize_map_environment_pretty(
    environment: &MapEnvironmentPresentation,
) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(environment)
        .map_err(|error| format!("serialize compiled map environment: {error}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

struct CloudRng(u64);

impl CloudRng {
    fn next_u64(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.0 = value;
        value.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn unit(&mut self) -> f32 {
        let value = (self.next_u64() >> 40) as u32;
        value as f32 / 16_777_216.0
    }

    fn index(&mut self, count: usize) -> usize {
        (self.next_u64() as usize) % count
    }

    fn range(&mut self, range: [f32; 2]) -> f32 {
        range[0] + (range[1] - range[0]) * self.unit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_gradient_is_opaque() {
        let gradient = SkyGradient::default();
        assert_eq!(gradient.top_rgba[3], 255);
        assert_eq!(gradient.bottom_rgba[3], 255);
    }

    #[test]
    fn legacy_parallax_defaults_to_repeat_fill() {
        let json = r#"{
            "id":"bg",
            "asset_path":"assets/maps/BG.png",
            "depth":"far",
            "parallax":0.18,
            "offset_world":[0.0,0.0],
            "repeat_x":true,
            "repeat_y":false,
            "opacity":1.0
        }"#;
        let layer: ParallaxLayer = serde_json::from_str(json).unwrap();
        assert_eq!(layer.fill_mode, ParallaxFillMode::Repeat);
        assert_eq!(layer.motion_world_per_second, [0.0, 0.0]);
    }

    #[test]
    fn repeating_motion_wraps_by_rendered_period() {
        let layer = ParallaxLayer {
            id: "clouds".to_owned(),
            asset_path: "assets/skys/clouds.png".to_owned(),
            depth: ParallaxDepth::Far,
            fill_mode: ParallaxFillMode::Repeat,
            parallax: 0.18,
            offset_world: [1.0, 2.0],
            motion_world_per_second: [2.0, -1.0],
            repeat_x: true,
            repeat_y: false,
            opacity: 1.0,
        };
        let offset = layer.animated_offset_world(3.0, [4.0, 5.0]);
        assert!((offset[0] - 3.0).abs() < 1e-5);
        assert!((offset[1] + 1.0).abs() < 1e-5);
    }

    #[test]
    fn cloud_generation_is_seeded_and_horizontally_stratified() {
        let field = CloudFieldAuthoring {
            id: "clouds.far".to_owned(),
            asset_folder: "assets/skys/clouds".to_owned(),
            depth: ParallaxDepth::Far,
            parallax: ParallaxDepth::Far.default_parallax(),
            density: 0.5,
            scale_range: [0.5, 0.8],
            speed_range: [0.08, 0.16],
            height_range: [0.6, 0.9],
            opacity_range: [0.6, 0.9],
        };
        let first = cloud_instance_specs(&field, 10, 123, 6);
        let second = cloud_instance_specs(&field, 10, 123, 6);
        assert_eq!(first, second);
        for (index, cloud) in first.iter().enumerate() {
            let start = index as f32 / first.len() as f32;
            let end = (index + 1) as f32 / first.len() as f32;
            assert!(cloud.x_unit >= start && cloud.x_unit < end);
            assert!(cloud.asset_index < 10);
        }
    }

    #[test]
    fn density_scales_with_coverage_and_stays_bounded() {
        assert_eq!(cloud_instance_count(0.0, 23.0, 23.0), 0);
        assert_eq!(cloud_instance_count(0.5, 23.0, 23.0), 6);
        assert_eq!(cloud_instance_count(0.5, 46.0, 23.0), 12);
        assert_eq!(
            cloud_instance_count(1.0, 10_000.0, 23.0),
            MAX_CLOUDS_PER_FIELD
        );
    }

    #[cfg(feature = "map-authoring")]
    #[test]
    fn cloud_folder_discovery_is_sorted_non_recursive_and_png_only() {
        let root = std::env::temp_dir().join(format!(
            "purgatory-cloud-folder-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let folder = root.join("assets/skys/clouds");
        std::fs::create_dir_all(folder.join("nested")).unwrap();
        std::fs::write(folder.join("zeta.PNG"), b"png").unwrap();
        std::fs::write(folder.join("alpha.png"), b"png").unwrap();
        std::fs::write(folder.join("notes.txt"), b"ignore").unwrap();
        std::fs::write(folder.join("nested/hidden.png"), b"ignore").unwrap();

        let resolved = resolve_png_asset_folder(&root, "assets/skys/clouds").unwrap();
        assert_eq!(
            resolved,
            vec![
                "assets/skys/clouds/alpha.png".to_owned(),
                "assets/skys/clouds/zeta.PNG".to_owned(),
            ]
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn semantic_depths_have_stable_parallax_defaults() {
        assert_eq!(ParallaxDepth::Sky.default_parallax(), 0.0);
        assert!(ParallaxDepth::Far.default_parallax() < ParallaxDepth::Mid.default_parallax());
        assert!(ParallaxDepth::Mid.default_parallax() < ParallaxDepth::Near.default_parallax());
    }
}
