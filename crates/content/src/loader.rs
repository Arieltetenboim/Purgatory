//! Filesystem JSON loader. Not used on the simulation tick.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::domain::ContentDomain;
use crate::error::{ContentError, ValidationIssue};
use crate::registry::ContentRegistry;
use crate::schema::{
    CONTENT_SCHEMA_VERSION, EntityDefinition, MapDefinition, MapPlatform, Placement, RestorePolicy,
    SpawnPoint, TransitionRef,
};
use purgatory_common::{ContentId, validate_authored_id};
use purgatory_simulation::{InteractableKind, PlatformKind, WorldBounds};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadMode {
    /// Client-safe: maps + shared entities only.
    Shared,
    /// Server: shared + server-only entities and placements.
    Full,
}

#[must_use]
pub fn default_content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("content")
}

pub fn load_registry(root: &Path, mode: LoadMode) -> Result<ContentRegistry, ContentError> {
    let mut registry = ContentRegistry::new();
    let mut issues = Vec::new();
    load_dir(
        &mut registry,
        &mut issues,
        &root.join("shared").join("entities"),
        ContentDomain::Shared,
        Kind::Entity,
    );
    load_dir(
        &mut registry,
        &mut issues,
        &root.join("shared").join("maps"),
        ContentDomain::Shared,
        Kind::Map,
    );
    if mode == LoadMode::Full {
        load_dir(
            &mut registry,
            &mut issues,
            &root.join("server").join("entities"),
            ContentDomain::ServerOnly,
            Kind::Entity,
        );
        load_dir(
            &mut registry,
            &mut issues,
            &root.join("server").join("placements"),
            ContentDomain::ServerOnly,
            Kind::Placements,
        );
    }
    if !issues.is_empty() {
        return Err(ContentError { issues });
    }
    registry.finish()?;
    Ok(registry)
}

#[derive(Clone, Copy)]
enum Kind {
    Entity,
    Map,
    Placements,
}

fn load_dir(
    registry: &mut ContentRegistry,
    issues: &mut Vec<ValidationIssue>,
    dir: &Path,
    domain: ContentDomain,
    kind: Kind,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
        Err(err) => {
            issues.push(ValidationIssue::new(
                dir.display().to_string(),
                "-",
                "io",
                err.to_string(),
            ));
            return;
        }
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    for path in paths {
        if let Err(err) = load_file(registry, &path, domain, kind) {
            issues.extend(err.issues);
        }
    }
}

fn load_file(
    registry: &mut ContentRegistry,
    path: &Path,
    domain: ContentDomain,
    kind: Kind,
) -> Result<(), ContentError> {
    let text = fs::read_to_string(path).map_err(|e| ContentError::from_io(path, &e))?;
    match kind {
        Kind::Entity => {
            let raw: RawEntity = parse(path, &text)?;
            let def = raw.into_def(path, domain)?;
            registry.insert_entity(def)
        }
        Kind::Map => {
            let raw: RawMap = parse(path, &text)?;
            let def = raw.into_def(path, domain)?;
            registry.insert_map(def)
        }
        Kind::Placements => {
            let raw: RawPlacements = parse(path, &text)?;
            check_schema(path, raw.schema_version, &raw.map)?;
            check_authored(path, &raw.map)?;
            let mut placements = Vec::new();
            for (i, p) in raw.placements.iter().enumerate() {
                check_authored(path, &p.entity)?;
                let pos = pair(path, &format!("placements[{i}].position"), p.position)?;
                placements.push(Placement {
                    entity_authored: p.entity.clone(),
                    position: pos,
                });
            }
            registry.insert_placements(raw.map, placements)
        }
    }
}

fn parse<'a, T: Deserialize<'a>>(path: &Path, text: &'a str) -> Result<T, ContentError> {
    serde_json::from_str(text)
        .map_err(|e| ContentError::from_path(path.to_path_buf(), "-", "json", e.to_string()))
}

fn check_schema(path: &Path, version: u32, def: &str) -> Result<(), ContentError> {
    if version != CONTENT_SCHEMA_VERSION {
        return Err(ContentError::from_path(
            path.to_path_buf(),
            def,
            "schema_version",
            format!("unsupported schema version {version} (want {CONTENT_SCHEMA_VERSION})"),
        ));
    }
    Ok(())
}

fn check_authored(path: &Path, id: &str) -> Result<(), ContentError> {
    validate_authored_id(id)
        .map_err(|e| ContentError::from_path(path.to_path_buf(), id, "id", format!("{e:?}")))
}

fn pair(path: &Path, field: &str, value: [f32; 2]) -> Result<[f32; 2], ContentError> {
    if !value[0].is_finite() || !value[1].is_finite() {
        return Err(ContentError::from_path(
            path.to_path_buf(),
            "-",
            field,
            "non-finite number",
        ));
    }
    Ok(value)
}

#[derive(Deserialize)]
struct RawEntity {
    schema_version: u32,
    id: String,
    debug_name: String,
    #[serde(default)]
    visible: bool,
    #[serde(default)]
    interactable: Option<String>,
    #[serde(default)]
    transition: Option<RawTransition>,
}

#[derive(Deserialize)]
struct RawTransition {
    map: String,
    portal: String,
}

#[derive(Deserialize)]
struct RawMap {
    schema_version: u32,
    id: String,
    debug_name: String,
    bounds: RawBounds,
    spawn_points: Vec<RawSpawn>,
    platforms: Vec<RawPlatform>,
    restore: RawRestore,
}

#[derive(Deserialize)]
struct RawBounds {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

#[derive(Deserialize)]
struct RawSpawn {
    id: String,
    position: [f32; 2],
}

#[derive(Deserialize)]
struct RawRestore {
    policy: String,
    #[serde(default)]
    point: Option<String>,
    #[serde(default)]
    fallback_map: Option<String>,
    #[serde(default)]
    fallback_point: Option<String>,
}

#[derive(Deserialize)]
struct RawPlatform {
    position: [f32; 2],
    half_extents: [f32; 2],
    kind: String,
}

#[derive(Deserialize)]
struct RawPlacements {
    schema_version: u32,
    map: String,
    placements: Vec<RawPlacement>,
}

#[derive(Deserialize)]
struct RawPlacement {
    entity: String,
    position: [f32; 2],
}

impl RawEntity {
    fn into_def(
        self,
        path: &Path,
        domain: ContentDomain,
    ) -> Result<EntityDefinition, ContentError> {
        check_schema(path, self.schema_version, &self.id)?;
        check_authored(path, &self.id)?;
        let interactable = match self.interactable.as_deref() {
            None => None,
            Some("generic") => Some(InteractableKind::Generic),
            Some("npc") => Some(InteractableKind::Npc),
            Some("portal") => Some(InteractableKind::Portal),
            Some("chest") => Some(InteractableKind::Chest),
            Some("switch") => Some(InteractableKind::Switch),
            Some(other) => {
                return Err(ContentError::from_path(
                    path.to_path_buf(),
                    &self.id,
                    "interactable",
                    format!("unknown kind {other}"),
                ));
            }
        };
        let transition = if let Some(tr) = self.transition {
            check_authored(path, &tr.map)?;
            check_authored(path, &tr.portal)?;
            Some(TransitionRef {
                map_authored: tr.map,
                portal_authored: tr.portal,
            })
        } else {
            None
        };
        Ok(EntityDefinition {
            content_id: ContentId::from_authored(&self.id).expect("validated"),
            authored_id: self.id,
            debug_name: self.debug_name,
            domain,
            visible: self.visible,
            interactable,
            transition,
        })
    }
}

impl RawMap {
    fn into_def(self, path: &Path, domain: ContentDomain) -> Result<MapDefinition, ContentError> {
        check_schema(path, self.schema_version, &self.id)?;
        check_authored(path, &self.id)?;
        if domain != ContentDomain::Shared {
            return Err(ContentError::from_path(
                path.to_path_buf(),
                &self.id,
                "domain",
                "map definitions are shared content",
            ));
        }
        let bounds = WorldBounds {
            min_x: self.bounds.min_x,
            max_x: self.bounds.max_x,
            min_y: self.bounds.min_y,
            max_y: self.bounds.max_y,
        };
        if !bounds.min_x.is_finite()
            || !bounds.max_x.is_finite()
            || !bounds.min_y.is_finite()
            || !bounds.max_y.is_finite()
            || bounds.max_x <= bounds.min_x
            || bounds.max_y <= bounds.min_y
        {
            return Err(ContentError::from_path(
                path.to_path_buf(),
                &self.id,
                "bounds",
                "invalid bounds",
            ));
        }
        if self.spawn_points.is_empty() {
            return Err(ContentError::from_path(
                path.to_path_buf(),
                &self.id,
                "spawn_points",
                "at least one spawn point is required",
            ));
        }
        let mut spawn_points = Vec::new();
        for sp in self.spawn_points {
            spawn_points.push(SpawnPoint {
                id: sp.id,
                position: pair(path, "spawn_points.position", sp.position)?,
            });
        }
        let mut platforms = Vec::new();
        for (i, p) in self.platforms.iter().enumerate() {
            let kind = match p.kind.as_str() {
                "solid" => PlatformKind::Solid,
                "one_way" => PlatformKind::OneWay,
                other => {
                    return Err(ContentError::from_path(
                        path.to_path_buf(),
                        &self.id,
                        &format!("platforms[{i}].kind"),
                        format!("unknown {other}"),
                    ));
                }
            };
            if p.half_extents[0] <= 0.0 || p.half_extents[1] <= 0.0 {
                return Err(ContentError::from_path(
                    path.to_path_buf(),
                    &self.id,
                    &format!("platforms[{i}].half_extents"),
                    "extents must be positive",
                ));
            }
            platforms.push(MapPlatform {
                position: pair(path, &format!("platforms[{i}].position"), p.position)?,
                half_extents: pair(
                    path,
                    &format!("platforms[{i}].half_extents"),
                    p.half_extents,
                )?,
                kind,
            });
        }
        let restore = parse_restore(path, &self.id, &self.restore, &spawn_points)?;
        Ok(MapDefinition {
            content_id: ContentId::from_authored(&self.id).expect("validated"),
            authored_id: self.id,
            debug_name: self.debug_name,
            domain,
            bounds,
            spawn_points,
            platforms,
            restore,
        })
    }
}

fn parse_restore(
    path: &Path,
    map_id: &str,
    raw: &RawRestore,
    spawn_points: &[SpawnPoint],
) -> Result<RestorePolicy, ContentError> {
    let has_point = |id: &str| spawn_points.iter().any(|s| s.id == id);
    match raw.policy.as_str() {
        "safe_point" => {
            let point_id = raw.point.clone().ok_or_else(|| {
                ContentError::from_path(path.to_path_buf(), map_id, "restore.point", "required")
            })?;
            if !has_point(&point_id) {
                return Err(ContentError::from_path(
                    path.to_path_buf(),
                    map_id,
                    "restore.point",
                    format!("unknown spawn point {point_id}"),
                ));
            }
            Ok(RestorePolicy::SafePoint { point_id })
        }
        "checkpoint" => {
            let point_id = raw.point.clone().ok_or_else(|| {
                ContentError::from_path(path.to_path_buf(), map_id, "restore.point", "required")
            })?;
            if !has_point(&point_id) {
                return Err(ContentError::from_path(
                    path.to_path_buf(),
                    map_id,
                    "restore.point",
                    format!("unknown spawn point {point_id}"),
                ));
            }
            Ok(RestorePolicy::Checkpoint { point_id })
        }
        "non_reenterable" => {
            let fallback_map = raw.fallback_map.clone().ok_or_else(|| {
                ContentError::from_path(
                    path.to_path_buf(),
                    map_id,
                    "restore.fallback_map",
                    "required",
                )
            })?;
            let fallback_point = raw.fallback_point.clone().ok_or_else(|| {
                ContentError::from_path(
                    path.to_path_buf(),
                    map_id,
                    "restore.fallback_point",
                    "required",
                )
            })?;
            check_authored(path, &fallback_map)?;
            Ok(RestorePolicy::NonReenterable {
                fallback_map,
                fallback_point,
            })
        }
        other => Err(ContentError::from_path(
            path.to_path_buf(),
            map_id,
            "restore.policy",
            format!("unknown {other}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_file(dir: &Path, name: &str, body: &str) {
        fs::create_dir_all(dir).unwrap();
        let mut f = fs::File::create(dir.join(name)).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    #[test]
    fn malformed_json_fails() {
        let tmp = std::env::temp_dir().join(format!("purgatory-content-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(&tmp.join("shared/maps"), "bad.json", "{ not json");
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("malformed");
        assert!(err.to_string().contains("json") || err.to_string().contains("expected"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn unsupported_schema_version_fails() {
        let tmp = std::env::temp_dir().join(format!("purgatory-content-v-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/maps"),
            "x.json",
            r#"{"schema_version":99,"id":"map.dev.x","debug_name":"x","bounds":{"min_x":0,"max_x":1,"min_y":0,"max_y":1},"spawn_points":[{"id":"default","position":[0,0]}],"restore":{"policy":"safe_point","point":"default"},"platforms":[{"position":[0,0],"half_extents":[1,0.2],"kind":"solid"}]}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("version");
        assert!(err.to_string().contains("unsupported schema"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn workspace_pack_loads_and_matches_footnote_geometry() {
        let registry = load_registry(&default_content_root(), LoadMode::Full).expect("pack");
        assert!(registry.map_count() >= 2);
        assert!(registry.entity_count() >= 4);
        let map_a = registry
            .map(purgatory_common::MAP_FOOTNOTE_AUTHORED)
            .expect("A");
        assert_eq!(map_a.platforms.len(), 26);
        assert_eq!(
            map_a.platforms[0].position,
            purgatory_simulation::P0_POSITION
        );
        assert_eq!(
            map_a.platforms[0].half_extents,
            purgatory_simulation::P0.half_extents
        );
        assert_eq!(
            map_a.bounds,
            purgatory_simulation::WorldBounds::FOOTNOTE_TEST
        );
        let spawn = map_a
            .spawn_points
            .iter()
            .find(|s| s.id == "default")
            .expect("spawn");
        assert!((spawn.position[0] - purgatory_simulation::FOOTNOTE_SPAWN_X).abs() < 1e-4);
        let cid_a =
            purgatory_common::ContentId::from_authored(purgatory_common::MAP_FOOTNOTE_AUTHORED)
                .unwrap();
        let cid_b =
            purgatory_common::ContentId::from_authored(purgatory_common::MAP_SECOND_AUTHORED)
                .unwrap();
        assert_eq!(registry.map_id(cid_a), Some(purgatory_common::MapId::DEV));
        assert_eq!(
            registry.map_id(cid_b),
            Some(purgatory_common::MapId::from_raw(2))
        );
        let shared = load_registry(&default_content_root(), LoadMode::Shared).expect("shared");
        assert_eq!(shared.map_count(), 2);
        assert_eq!(shared.entity_count(), 0);
        assert!(shared.entity("entity.portal.to_second").is_none());
        assert!(
            registry
                .entity("entity.portal.to_second")
                .unwrap()
                .transition
                .as_ref()
                .is_some_and(|tr| tr.portal_authored == "entity.portal.to_footnote")
        );
    }
}
