//! Map Lab-owned gameplay authoring schema.
//!
//! Visual composition stays in Tiled. Gameplay authoring such as footholds
//! stays in PURGATORY-owned data and is compiled separately into runtime maps.

use serde::{Deserialize, Serialize};

pub const MAP_GAMEPLAY_AUTHORING_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FootholdKind {
    OneWay,
    Solid,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FootholdPath {
    pub id: String,
    pub kind: FootholdKind,
    pub drop_through: bool,
    pub points: Vec<[f32; 2]>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapGameplayAuthoring {
    pub schema_version: u32,
    pub map_authored: String,
    #[serde(default)]
    pub foothold_paths: Vec<FootholdPath>,
}

impl MapGameplayAuthoring {
    #[must_use]
    pub fn empty(map_authored: impl Into<String>) -> Self {
        Self {
            schema_version: MAP_GAMEPLAY_AUTHORING_SCHEMA_VERSION,
            map_authored: map_authored.into(),
            foothold_paths: Vec::new(),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_authoring_is_versioned_and_map_owned() {
        let authoring = MapGameplayAuthoring::empty("map.map1");
        assert_eq!(authoring.schema_version, MAP_GAMEPLAY_AUTHORING_SCHEMA_VERSION);
        assert_eq!(authoring.map_authored, "map.map1");
        assert!(authoring.foothold_paths.is_empty());
    }

    #[test]
    fn foothold_kind_serializes_stably() {
        assert_eq!(
            serde_json::to_string(&FootholdKind::OneWay).unwrap(),
            "\"one_way\""
        );
        assert_eq!(
            serde_json::to_string(&FootholdKind::Solid).unwrap(),
            "\"solid\""
        );
    }
}
