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
    fn semantic_depths_have_stable_parallax_defaults() {
        assert_eq!(ParallaxDepth::Sky.default_parallax(), 0.0);
        assert!(ParallaxDepth::Far.default_parallax() < ParallaxDepth::Mid.default_parallax());
        assert!(ParallaxDepth::Mid.default_parallax() < ParallaxDepth::Near.default_parallax());
    }
}
