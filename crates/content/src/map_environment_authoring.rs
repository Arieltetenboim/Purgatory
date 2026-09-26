//! Map Lab-owned environment authoring.
//!
//! Tiled owns static world composition. This file owns map-level presentation
//! behavior such as sky gradients and, in later slices, parallax/motion.

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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapEnvironmentAuthoring {
    pub schema_version: u32,
    pub map_authored: String,
    #[serde(default)]
    pub sky_gradient: Option<SkyGradient>,
}

impl MapEnvironmentAuthoring {
    #[must_use]
    pub fn empty(map_authored: impl Into<String>) -> Self {
        Self {
            schema_version: MAP_ENVIRONMENT_AUTHORING_SCHEMA_VERSION,
            map_authored: map_authored.into(),
            sky_gradient: None,
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
}
