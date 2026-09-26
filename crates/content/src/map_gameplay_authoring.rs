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
pub struct GameplaySpawnPoint {
    pub id: String,
    pub position: [f32; 2],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapGameplayAuthoring {
    pub schema_version: u32,
    pub map_authored: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub foothold_paths: Vec<FootholdPath>,
    #[serde(default)]
    pub spawn_points: Vec<GameplaySpawnPoint>,
}

impl MapGameplayAuthoring {
    #[must_use]
    pub fn empty(map_authored: impl Into<String>) -> Self {
        let map_authored = map_authored.into();
        Self {
            schema_version: MAP_GAMEPLAY_AUTHORING_SCHEMA_VERSION,
            name: map_authored.clone(),
            map_authored,
            foothold_paths: Vec::new(),
            spawn_points: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_authoring_is_versioned_and_map_owned() {
        let authoring = MapGameplayAuthoring::empty("map.map1");
        assert_eq!(
            authoring.schema_version,
            MAP_GAMEPLAY_AUTHORING_SCHEMA_VERSION
        );
        assert_eq!(authoring.map_authored, "map.map1");
        assert_eq!(authoring.name, "map.map1");
        assert!(authoring.foothold_paths.is_empty());
        assert!(authoring.spawn_points.is_empty());
    }

    #[test]
    fn spawn_point_serializes_stably() {
        let spawn = GameplaySpawnPoint {
            id: "default".to_owned(),
            position: [12.5, 3.0],
        };
        let json = serde_json::to_string(&spawn).unwrap();
        assert!(json.contains("\"id\":\"default\""));
        assert!(json.contains("\"position\":[12.5,3.0]"));
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
