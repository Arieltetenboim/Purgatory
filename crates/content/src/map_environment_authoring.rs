//! Map Lab-owned environment authoring.
//!
//! Tiled owns static world composition. This file owns map-level presentation
//! behavior such as sky gradients and camera-relative parallax layers.

use serde::{Deserialize, Serialize};

pub const MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION: u32 = 1;

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
    #[serde(default)]
    pub repeat_x: bool,
    #[serde(default)]
    pub repeat_y: bool,
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
}

impl MapEnvironmentAuthoring {
    #[must_use]
    pub fn empty(map_authored: impl Into<String>) -> Self {
        Self {
            schema_version: MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION,
            map_authored: map_authored.into(),
            sky_gradient: None,
            parallax_layers: Vec::new(),
        }
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
    }

    #[test]
    fn semantic_depths_have_stable_parallax_defaults() {
        assert_eq!(ParallaxDepth::Sky.default_parallax(), 0.0);
        assert!(ParallaxDepth::Far.default_parallax() < ParallaxDepth::Mid.default_parallax());
        assert!(ParallaxDepth::Mid.default_parallax() < ParallaxDepth::Near.default_parallax());
    }
}
