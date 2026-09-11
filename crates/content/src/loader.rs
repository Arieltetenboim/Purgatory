//! Filesystem JSON loader. Not used on the simulation tick.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ability::ABILITY_CONTENT_SCHEMA_VERSION;
use crate::dialogue;
use crate::domain::ContentDomain;
use crate::equipment::{
    AnchorPoint, BoneTarget, CorrectionOffset, CoverageMode, EQUIPMENT_CONTENT_SCHEMA_VERSION,
    EquipmentDefinition, EquipmentPresentation, PresentationAttachment, ViewVisuals,
    validate_equipment_definition,
};
use crate::error::{ContentError, ValidationIssue};
use crate::item::{ITEM_CONTENT_SCHEMA_VERSION, ItemDefinition, validate_item_definition};
use crate::registry::ContentRegistry;
use crate::schema::{
    CONTENT_SCHEMA_VERSION, EntityDefinition, MapDefinition, MapPlatform, Placement, RestorePolicy,
    SpawnPoint, TransitionRef,
};
use purgatory_common::{ContentId, ContentKind, allocated_id_for_label, validate_authored_id};
use purgatory_simulation::{
    AbilityActivation, AbilityDefinition, AbilityDelivery, AbilityEffect, AbilityTiming,
    EquipmentSlot, InteractableKind, PlatformKind, WorldBounds,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadMode {
    /// Client-safe: maps + shared entities + shared equipment + shared abilities.
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
    load_dir(
        &mut registry,
        &mut issues,
        &root.join("shared").join("items"),
        ContentDomain::Shared,
        Kind::Item,
    );
    load_dir(
        &mut registry,
        &mut issues,
        &root.join("shared").join("equipment"),
        ContentDomain::Shared,
        Kind::Equipment,
    );
    load_dir(
        &mut registry,
        &mut issues,
        &root.join("shared").join("equipment_presentation"),
        ContentDomain::Shared,
        Kind::EquipmentPresentation,
    );
    load_dir(
        &mut registry,
        &mut issues,
        &root.join("shared").join("abilities"),
        ContentDomain::Shared,
        Kind::Ability,
    );
    load_npc_authoring_tree(
        &mut registry,
        &mut issues,
        &root.join("authoring").join("npcs"),
        mode,
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

fn load_npc_authoring_tree(
    registry: &mut ContentRegistry,
    issues: &mut Vec<ValidationIssue>,
    root: &Path,
    mode: LoadMode,
) {
    let mut paths = Vec::new();
    collect_json_paths(root, &mut paths, issues);
    paths.sort();
    let mut documents = Vec::new();
    for path in paths {
        let result = fs::read_to_string(&path)
            .map_err(|error| ContentError::from_io(&path, &error))
            .and_then(|text| dialogue::parse_raw(&path, &text));
        match result {
            Ok(document) => documents.push((path, document)),
            Err(error) => issues.extend(error.issues),
        }
    }

    let mut npc_beats = HashMap::new();
    for (path, document) in &documents {
        if npc_beats
            .insert(document.authored_id().to_string(), document.beat_ids())
            .is_some()
        {
            issues.push(ValidationIssue::new(
                path.display().to_string(),
                document.authored_id(),
                "id",
                "duplicate NPC authored id",
            ));
        }
    }

    for (path, document) in documents {
        let content_id = allocated_id_for_label(document.authored_id());
        let result = document
            .validate_npc_references(&path, &npc_beats)
            .and_then(|()| document.into_definition(&path, content_id))
            .and_then(|definition| {
                definition.map_or(Ok(()), |definition| {
                    registry.insert_npc_dialogue_presentation((&definition).into())?;
                    if mode == LoadMode::Full {
                        registry.insert_npc_dialogue(definition)?;
                    }
                    Ok(())
                })
            });
        if let Err(error) = result {
            issues.extend(error.issues);
        }
    }
}

fn collect_json_paths(dir: &Path, paths: &mut Vec<PathBuf>, issues: &mut Vec<ValidationIssue>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            issues.push(ValidationIssue::new(
                dir.display().to_string(),
                "-",
                "io",
                error.to_string(),
            ));
            return;
        }
    };
    let mut entries: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_json_paths(&path, paths, issues);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            paths.push(path);
        }
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Entity,
    Map,
    Placements,
    Item,
    Equipment,
    EquipmentPresentation,
    Ability,
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
        Kind::Item => {
            let raw: RawItem = parse(path, &text)?;
            let def = raw.into_def(path, domain)?;
            registry.insert_item(def)
        }
        Kind::Equipment => {
            let raw: RawEquipment = parse(path, &text)?;
            let def = raw.into_def(path, domain)?;
            registry.insert_equipment(def)
        }
        Kind::EquipmentPresentation => {
            let raw: RawEquipmentPresentation = parse(path, &text)?;
            let def = raw.into_def(path)?;
            registry.insert_equipment_presentation(def)
        }
        Kind::Ability => {
            let raw: RawAbility = parse(path, &text)?;
            let authored = raw.id.clone();
            let def = raw.into_def(path)?;
            registry.insert_ability(authored, def)
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

fn check_equipment_schema(path: &Path, version: u32, def: &str) -> Result<(), ContentError> {
    if version != EQUIPMENT_CONTENT_SCHEMA_VERSION {
        return Err(ContentError::from_path(
            path.to_path_buf(),
            def,
            "schema_version",
            format!(
                "unsupported equipment schema version {version} (want {EQUIPMENT_CONTENT_SCHEMA_VERSION})"
            ),
        ));
    }
    Ok(())
}

fn check_item_schema(path: &Path, version: u32, def: &str) -> Result<(), ContentError> {
    if version != ITEM_CONTENT_SCHEMA_VERSION {
        return Err(ContentError::from_path(
            path.to_path_buf(),
            def,
            "schema_version",
            format!(
                "unsupported item schema version {version} (want {ITEM_CONTENT_SCHEMA_VERSION})"
            ),
        ));
    }
    Ok(())
}

fn check_ability_schema(path: &Path, version: u32, def: &str) -> Result<(), ContentError> {
    if version != ABILITY_CONTENT_SCHEMA_VERSION {
        return Err(ContentError::from_path(
            path.to_path_buf(),
            def,
            "schema_version",
            format!(
                "unsupported ability schema version {version} (want {ABILITY_CONTENT_SCHEMA_VERSION})"
            ),
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEquipment {
    schema_version: u32,
    id: String,
    equipment_slot: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawItem {
    schema_version: u32,
    id: String,
    stack_limit: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEquipmentPresentation {
    schema_version: u32,
    id: String,
    #[serde(default)]
    attachments: Vec<RawAttachment>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAttachment {
    id: String,
    bone: String,
    anchor: String,
    coverage: String,
    #[serde(default)]
    hide_base: Vec<String>,
    #[serde(default)]
    correction: RawCorrection,
    visuals: RawVisuals,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawCorrection {
    #[serde(default)]
    x: f32,
    #[serde(default)]
    y: f32,
    #[serde(default)]
    rotation: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVisuals {
    side: String,
    #[serde(default)]
    back: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAbility {
    schema_version: u32,
    id: String,
    timing: RawAbilityTiming,
    activation: String,
    delivery: RawAbilityDelivery,
    effects: Vec<RawAbilityEffect>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAbilityTiming {
    windup_ticks: u64,
    active_ticks: u64,
    recovery_ticks: u64,
    cooldown_ticks: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAbilityDelivery {
    kind: String,
    #[serde(default)]
    range: Option<f32>,
    #[serde(default)]
    half_height: Option<f32>,
    #[serde(default)]
    max_targets: Option<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAbilityEffect {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    amount: Option<f32>,
}

impl RawAbility {
    fn into_def(self, path: &Path) -> Result<AbilityDefinition, ContentError> {
        check_ability_schema(path, self.schema_version, &self.id)?;
        check_authored(path, &self.id)?;
        let activation = match self.activation.as_str() {
            "independent" => AbilityActivation::Independent,
            "selected_entity" => AbilityActivation::SelectedEntity,
            other => {
                return Err(ContentError::from_path(
                    path.to_path_buf(),
                    &self.id,
                    "activation",
                    format!("unknown activation '{other}'"),
                ));
            }
        };
        let delivery = match self.delivery.kind.as_str() {
            "forward_query" => AbilityDelivery::ForwardQuery {
                range: self.delivery.range.ok_or_else(|| {
                    ContentError::from_path(
                        path.to_path_buf(),
                        &self.id,
                        "delivery.range",
                        "required for forward_query",
                    )
                })?,
                half_height: self.delivery.half_height.ok_or_else(|| {
                    ContentError::from_path(
                        path.to_path_buf(),
                        &self.id,
                        "delivery.half_height",
                        "required for forward_query",
                    )
                })?,
                max_targets: self.delivery.max_targets.ok_or_else(|| {
                    ContentError::from_path(
                        path.to_path_buf(),
                        &self.id,
                        "delivery.max_targets",
                        "required for forward_query",
                    )
                })?,
            },
            "selected_entity" => AbilityDelivery::SelectedEntity,
            other => {
                return Err(ContentError::from_path(
                    path.to_path_buf(),
                    &self.id,
                    "delivery.kind",
                    format!("unknown delivery '{other}'"),
                ));
            }
        };
        let mut effects = Vec::new();
        for raw in self.effects {
            match raw.kind.as_str() {
                "damage" => {
                    let amount = raw.amount.ok_or_else(|| {
                        ContentError::from_path(
                            path.to_path_buf(),
                            &self.id,
                            "effects.amount",
                            "required for damage",
                        )
                    })?;
                    effects.push(AbilityEffect::Damage { amount });
                }
                other => {
                    return Err(ContentError::from_path(
                        path.to_path_buf(),
                        &self.id,
                        "effects.type",
                        format!("unknown effect '{other}'"),
                    ));
                }
            }
        }
        let def = AbilityDefinition {
            id: ContentId::from_authored(&self.id).expect("validated"),
            timing: AbilityTiming {
                windup_ticks: self.timing.windup_ticks,
                active_ticks: self.timing.active_ticks,
                recovery_ticks: self.timing.recovery_ticks,
                cooldown_ticks: self.timing.cooldown_ticks,
            },
            activation,
            delivery,
            effects,
        };
        def.validate().map_err(|e| {
            ContentError::from_path(path.to_path_buf(), &self.id, "ability", format!("{e:?}"))
        })?;
        Ok(def)
    }
}

impl RawEquipment {
    fn into_def(
        self,
        path: &Path,
        domain: ContentDomain,
    ) -> Result<EquipmentDefinition, ContentError> {
        check_equipment_schema(path, self.schema_version, &self.id)?;
        check_authored(path, &self.id)?;
        let slot = EquipmentSlot::parse(&self.equipment_slot).ok_or_else(|| {
            ContentError::from_path(
                path.to_path_buf(),
                &self.id,
                "equipment_slot",
                format!(
                    "rule=schema: unknown equipment slot '{}'",
                    self.equipment_slot
                ),
            )
        })?;
        let def = EquipmentDefinition {
            content_id: ContentId::from_authored(&self.id).expect("validated"),
            authored_id: self.id,
            slot,
            domain,
        };
        validate_equipment_definition(&def)?;
        Ok(def)
    }
}

impl RawItem {
    fn into_def(self, path: &Path, domain: ContentDomain) -> Result<ItemDefinition, ContentError> {
        check_item_schema(path, self.schema_version, &self.id)?;
        check_authored(path, &self.id)?;
        let def = ItemDefinition {
            content_id: ContentId::from_authored(&self.id).expect("validated"),
            authored_id: self.id,
            domain,
            stack_limit: self.stack_limit,
        };
        validate_item_definition(&def)?;
        Ok(def)
    }
}

impl RawEquipmentPresentation {
    fn into_def(self, path: &Path) -> Result<EquipmentPresentation, ContentError> {
        check_equipment_schema(path, self.schema_version, &self.id)?;
        check_authored(path, &self.id)?;
        let mut attachments = Vec::new();
        for (i, raw) in self.attachments.into_iter().enumerate() {
            attachments.push(parse_attachment(path, &self.id, i, raw)?);
        }
        Ok(EquipmentPresentation {
            content_id: ContentId::from_authored(&self.id).expect("validated"),
            authored_id: self.id,
            attachments,
        })
    }
}

fn parse_attachment(
    path: &Path,
    def: &str,
    index: usize,
    raw: RawAttachment,
) -> Result<PresentationAttachment, ContentError> {
    let field = |name: &str| format!("attachments[{index}].{name}");
    let bone = BoneTarget::parse(&raw.bone).ok_or_else(|| {
        ContentError::from_path(
            path.to_path_buf(),
            def,
            &field("bone"),
            format!("rule=schema: unknown BoneTarget '{}'", raw.bone),
        )
    })?;
    let anchor = AnchorPoint::parse(&raw.anchor).ok_or_else(|| {
        ContentError::from_path(
            path.to_path_buf(),
            def,
            &field("anchor"),
            format!("rule=schema: unknown AnchorPoint '{}'", raw.anchor),
        )
    })?;
    let coverage = CoverageMode::parse(&raw.coverage).ok_or_else(|| {
        ContentError::from_path(
            path.to_path_buf(),
            def,
            &field("coverage"),
            format!("rule=schema: unknown CoverageMode '{}'", raw.coverage),
        )
    })?;
    let mut hide_base = Vec::new();
    for (h, name) in raw.hide_base.iter().enumerate() {
        let bone = BoneTarget::parse(name).ok_or_else(|| {
            ContentError::from_path(
                path.to_path_buf(),
                def,
                &format!("attachments[{index}].hide_base[{h}]"),
                format!("rule=schema: unknown base visual '{name}'"),
            )
        })?;
        hide_base.push(bone);
    }
    Ok(PresentationAttachment {
        id: raw.id,
        bone,
        anchor,
        coverage,
        hide_base,
        correction: CorrectionOffset {
            x: raw.correction.x,
            y: raw.correction.y,
            rotation: raw.correction.rotation,
        },
        visuals: ViewVisuals {
            side: raw.visuals.side,
            back: raw.visuals.back,
        },
    })
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
        let content_id = if interactable == Some(InteractableKind::Npc) {
            let content_id = allocated_id_for_label(&self.id).ok_or_else(|| {
                ContentError::from_path(
                    path.to_path_buf(),
                    &self.id,
                    "id",
                    "NPC entity requires a numeric ContentId allocation",
                )
            })?;
            if content_id.kind() != Some(ContentKind::Npc) {
                return Err(ContentError::from_path(
                    path.to_path_buf(),
                    &self.id,
                    "id",
                    "NPC entity ContentId must be allocated in the NPC block",
                ));
            }
            content_id
        } else {
            ContentId::from_authored(&self.id).expect("validated")
        };
        Ok(EntityDefinition {
            content_id,
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

    fn minimal_npc_json(beats: &str) -> String {
        format!(
            r#"{{
                "schema_version": 1,
                "id": "npc.welcome.traveler_stayed",
                "interaction": {{ "beats": [{beats}] }}
            }}"#
        )
    }

    fn minimal_beat(id: &str, priority: i32) -> String {
        format!(
            r#"{{
                "id": "{id}",
                "priority": {priority},
                "entry": true,
                "pool": "mandatory",
                "conditions": [],
                "lines": [{{ "text": "Hello", "voice": "ignored.voice", "animation": "wave" }}],
                "choices": []
            }}"#
        )
    }

    #[test]
    fn workspace_pack_projects_traveler_dialogue_by_numeric_id() {
        use crate::{DialogueAction, DialogueCondition, DialoguePool, DialogueSelectionRole};
        use purgatory_common::{ContentKind, NPC_WELCOME_TRAVELER_STAYED};

        let registry = load_registry(&default_content_root(), LoadMode::Full).expect("pack");
        let traveler = registry
            .npc_dialogue_by_id(NPC_WELCOME_TRAVELER_STAYED)
            .expect("numeric Traveler dialogue");
        assert_eq!(traveler.authored_id, "npc.welcome.traveler_stayed");
        assert_eq!(traveler.content_id.kind(), Some(ContentKind::Npc));
        let traveler_entity = registry
            .entity_by_id(NPC_WELCOME_TRAVELER_STAYED)
            .expect("numeric Traveler entity");
        assert_eq!(traveler_entity.authored_id, traveler.authored_id);
        assert_eq!(traveler_entity.content_id, traveler.content_id);
        assert_eq!(registry.npc_dialogue_count(), 4);
        let spawn_catalog: Vec<_> = registry.iter_npc_dialogue_presentations().collect();
        assert_eq!(spawn_catalog.len(), 4);
        assert!(
            spawn_catalog
                .iter()
                .any(|npc| npc.content_id == NPC_WELCOME_TRAVELER_STAYED)
        );
        for npc in &spawn_catalog {
            let entity = registry
                .entity_by_id(npc.content_id)
                .expect("every DEV-spawnable authored NPC has an entity definition");
            assert_eq!(entity.authored_id, npc.authored_id);
            assert_eq!(entity.content_id, npc.content_id);
        }

        let intro = &traveler.beats[0];
        assert_eq!(intro.id, "intro");
        assert_eq!(intro.selection_role, DialogueSelectionRole::Entry);
        assert_eq!(intro.priority, 100);
        assert_eq!(intro.pool, DialoguePool::Mandatory);
        assert_eq!(intro.lines.len(), 1);
        assert!(intro.lines[0].text.ends_with("there's always work."));
        assert!(matches!(
            &intro.conditions[0],
            DialogueCondition::NpcMet {
                npc_authored,
                equals: false
            } if npc_authored == "npc.welcome.traveler_stayed"
        ));
        assert!(matches!(
            &intro.choices[0].actions[0],
            DialogueAction::MarkNpcMet { npc_authored }
                if npc_authored == "npc.welcome.traveler_stayed"
        ));
        let next = intro.choices[0].next.expect("resolved continuation");
        assert_eq!(traveler.beat(next).expect("beat").id, "intro_place");

        let shared = load_registry(&default_content_root(), LoadMode::Shared).expect("shared");
        assert_eq!(shared.npc_dialogue_count(), 0);
        let presentation = shared
            .npc_dialogue_presentation_by_id(NPC_WELCOME_TRAVELER_STAYED)
            .expect("client-safe Traveler lines");
        assert_eq!(
            presentation
                .beat(crate::DialogueBeatIndex::from_raw(0))
                .expect("intro beat")
                .display_text,
            intro.lines[0].text
        );
        let intro_presentation = presentation
            .beat(crate::DialogueBeatIndex::from_raw(0))
            .expect("intro presentation");
        assert_eq!(intro_presentation.choices.len(), 3);
        assert_eq!(intro_presentation.choices[0].text, "What's here?");
        let lore = presentation
            .beat(crate::DialogueBeatIndex::from_raw(8))
            .expect("multi-line lore Beat");
        assert!(lore.display_text.contains("\n\n"));
    }

    #[test]
    fn social_npc_entity_without_catalog_allocation_fails_clearly() {
        let tmp = std::env::temp_dir().join(format!(
            "purgatory-content-npc-entity-allocation-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("server/entities"),
            "unallocated.json",
            r#"{
                "schema_version": 1,
                "id": "npc.welcome.unallocated",
                "debug_name": "Unallocated NPC",
                "visible": true,
                "interactable": "npc"
            }"#,
        );

        let error = load_registry(&tmp, LoadMode::Full).expect_err("missing NPC allocation");
        assert!(
            error
                .to_string()
                .contains("NPC entity requires a numeric ContentId allocation")
        );
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn npc_projection_preserves_authored_order_and_drops_presentation_cues() {
        let tmp = std::env::temp_dir().join(format!(
            "purgatory-content-npc-order-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let beats = format!("{},{}", minimal_beat("first", 5), minimal_beat("second", 5));
        write_file(
            &tmp.join("authoring/npcs/welcome"),
            "traveler.json",
            &minimal_npc_json(&beats),
        );

        let registry = load_registry(&tmp, LoadMode::Full).expect("valid NPC dialogue");
        let traveler = registry
            .npc_dialogue_by_id(purgatory_common::NPC_WELCOME_TRAVELER_STAYED)
            .expect("Traveler");
        assert_eq!(traveler.beats[0].id, "first");
        assert_eq!(traveler.beats[1].id, "second");
        assert_eq!(traveler.beats[0].lines[0].text, "Hello");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn broken_npc_continuation_fails_clearly() {
        let tmp =
            std::env::temp_dir().join(format!("purgatory-content-npc-next-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let beat = r#"{
            "id": "intro",
            "priority": 1,
            "entry": true,
            "conditions": [],
            "lines": [{ "text": "Hello" }],
            "choices": [{ "id": "go", "text": "Go", "next": "missing", "actions": [] }]
        }"#;
        write_file(
            &tmp.join("authoring/npcs/welcome"),
            "traveler.json",
            &minimal_npc_json(beat),
        );

        let error = load_registry(&tmp, LoadMode::Full).expect_err("broken next");
        assert!(
            error
                .to_string()
                .contains("references missing beat 'missing'")
        );
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn unresolved_npc_reference_fails_clearly() {
        let tmp = std::env::temp_dir().join(format!(
            "purgatory-content-npc-reference-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let beat = r#"{
            "id": "intro",
            "priority": 1,
            "entry": true,
            "conditions": [{ "npc_met": "npc.welcome.missing", "equals": false }],
            "lines": [{ "text": "Hello" }],
            "choices": []
        }"#;
        write_file(
            &tmp.join("authoring/npcs/welcome"),
            "traveler.json",
            &minimal_npc_json(beat),
        );

        let error = load_registry(&tmp, LoadMode::Full).expect_err("unresolved NPC");
        assert!(
            error
                .to_string()
                .contains("references missing authored NPC 'npc.welcome.missing'")
        );
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn duplicate_npc_beat_id_fails_clearly() {
        let tmp = std::env::temp_dir().join(format!(
            "purgatory-content-npc-duplicate-beat-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let beats = format!("{},{}", minimal_beat("same", 2), minimal_beat("same", 1));
        write_file(
            &tmp.join("authoring/npcs/welcome"),
            "traveler.json",
            &minimal_npc_json(&beats),
        );

        let error = load_registry(&tmp, LoadMode::Full).expect_err("duplicate beat");
        assert!(error.to_string().contains("duplicate beat id 'same'"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn duplicate_allocated_npc_definition_fails_clearly() {
        let tmp = std::env::temp_dir().join(format!(
            "purgatory-content-npc-duplicate-definition-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let document = minimal_npc_json(&minimal_beat("intro", 1));
        write_file(
            &tmp.join("authoring/npcs/first"),
            "traveler.json",
            &document,
        );
        write_file(
            &tmp.join("authoring/npcs/second"),
            "traveler.json",
            &document,
        );

        let error = load_registry(&tmp, LoadMode::Full).expect_err("duplicate NPC");
        assert!(error.to_string().contains("duplicate ContentId"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn malformed_npc_condition_fails_clearly() {
        let tmp = std::env::temp_dir().join(format!(
            "purgatory-content-npc-condition-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let beat = r#"{
            "id": "intro",
            "priority": 1,
            "entry": true,
            "conditions": [{
                "fact": "welcome.ready",
                "npc_met": "npc.welcome.traveler_stayed",
                "equals": true
            }],
            "lines": [{ "text": "Hello" }],
            "choices": []
        }"#;
        write_file(
            &tmp.join("authoring/npcs/welcome"),
            "traveler.json",
            &minimal_npc_json(beat),
        );

        let error = load_registry(&tmp, LoadMode::Full).expect_err("malformed condition");
        assert!(
            error
                .to_string()
                .contains("exactly one supported typed check")
        );
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
        assert!(shared.item_count() >= 8);
        assert!(shared.equipment_count() >= 8);
        assert!(shared.ability_count() >= 1);
        assert!(shared.ability("skill.basic.strike").is_some());
        assert!(
            registry
                .entity("entity.portal.to_second")
                .unwrap()
                .transition
                .as_ref()
                .is_some_and(|tr| tr.portal_authored == "entity.portal.to_footnote")
        );
        let cap = registry
            .equipment("equipment.debug.cloth_cap")
            .expect("headwear gameplay");
        assert_eq!(cap.slot, purgatory_simulation::EquipmentSlot::Headwear);
        let cap_p = registry
            .equipment_presentation("equipment.debug.cloth_cap")
            .expect("headwear presentation");
        assert_eq!(cap_p.attachments.len(), 1);
        assert!(cap_p.attachments[0].completeness().has_back);
        let gloves = registry
            .equipment_presentation("equipment.debug.leather_gloves")
            .expect("gloves");
        assert_eq!(gloves.attachments.len(), 2);
        assert!(!gloves.attachments[0].completeness().has_back);
        let unadorned = registry
            .equipment_presentation("equipment.debug.unadorned")
            .expect("empty attachments");
        assert!(unadorned.attachments.is_empty());
    }

    #[test]
    fn equipment_unknown_target_fails() {
        let tmp = std::env::temp_dir().join(format!("purgatory-eq-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/equipment"),
            "equipment.debug.bad_target.json",
            r#"{"schema_version":1,"id":"equipment.debug.bad_target","equipment_slot":"headwear"}"#,
        );
        write_file(
            &tmp.join("shared/equipment_presentation"),
            "equipment.debug.bad_target.json",
            r#"{"schema_version":1,"id":"equipment.debug.bad_target","attachments":[{"id":"x","bone":"root","anchor":"bone_origin","coverage":"overlay","visuals":{"side":"equipment.debug.bad_target.side"}}]}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("unknown bone");
        assert!(err.to_string().contains("unknown BoneTarget"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn equipment_presentation_without_gameplay_fails() {
        let tmp = std::env::temp_dir().join(format!("purgatory-eq-orphan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/equipment_presentation"),
            "equipment.debug.orphan.json",
            r#"{"schema_version":1,"id":"equipment.debug.orphan","attachments":[]}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("orphan");
        assert!(err.to_string().contains("no matching equipment gameplay"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn item_definition_loads_and_resolves_by_content_id() {
        let tmp = std::env::temp_dir().join(format!("purgatory-item-ok-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/items"),
            "item.debug.token.json",
            r#"{"schema_version":1,"id":"item.debug.token","stack_limit":20}"#,
        );
        let registry = load_registry(&tmp, LoadMode::Shared).expect("valid item");
        let id = ContentId::from_authored("item.debug.token").unwrap();
        let item = registry.item_by_id(id).expect("lookup by ContentId");
        assert_eq!(item.authored_id, "item.debug.token");
        assert_eq!(item.stack_limit, 20);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn invalid_item_invariants_are_rejected() {
        let tmp = std::env::temp_dir().join(format!("purgatory-item-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/items"),
            "item.debug.zero.json",
            r#"{"schema_version":1,"id":"item.debug.zero","stack_limit":0}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("zero stack limit");
        assert!(err.to_string().contains("stack_limit"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn equipment_requires_matching_item_definition() {
        let tmp =
            std::env::temp_dir().join(format!("purgatory-item-equipment-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/equipment"),
            "equipment.debug.orphan.json",
            r#"{"schema_version":1,"id":"equipment.debug.orphan","equipment_slot":"headwear"}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("missing matching item");
        assert!(
            err.to_string()
                .contains("requires a matching item definition")
        );
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn matching_item_and_equipment_content_is_accepted() {
        let tmp = std::env::temp_dir().join(format!(
            "purgatory-item-equipment-ok-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/items"),
            "equipment.debug.cap.json",
            r#"{"schema_version":1,"id":"equipment.debug.cap","stack_limit":1}"#,
        );
        write_file(
            &tmp.join("shared/equipment"),
            "equipment.debug.cap.json",
            r#"{"schema_version":1,"id":"equipment.debug.cap","equipment_slot":"headwear"}"#,
        );
        let registry = load_registry(&tmp, LoadMode::Shared).expect("matching definitions");
        let id = ContentId::from_authored("equipment.debug.cap").unwrap();
        assert_eq!(
            registry.item_by_id(id).unwrap().content_id,
            registry.equipment_by_id(id).unwrap().content_id
        );
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn equipment_load_order_is_path_sorted() {
        let registry = load_registry(&default_content_root(), LoadMode::Shared).expect("pack");
        let ids: Vec<&str> = registry
            .iter_equipment()
            .map(|e| e.authored_id.as_str())
            .collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
        let cap = registry
            .equipment("equipment.debug.cloth_cap")
            .expect("cloth_cap gameplay");
        let pres = registry
            .equipment_presentation_by_id(cap.content_id)
            .expect("cloth_cap presentation");
        assert_eq!(pres.authored_id, cap.authored_id);
        assert_eq!(pres.attachments.len(), 1);
    }

    #[test]
    fn equipment_overlay_hide_base_fails() {
        let tmp = std::env::temp_dir().join(format!("purgatory-eq-ov-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/equipment"),
            "equipment.debug.ov.json",
            r#"{"schema_version":1,"id":"equipment.debug.ov","equipment_slot":"headwear"}"#,
        );
        write_file(
            &tmp.join("shared/equipment_presentation"),
            "equipment.debug.ov.json",
            r#"{"schema_version":1,"id":"equipment.debug.ov","attachments":[{"id":"crown","bone":"head","anchor":"crown","coverage":"overlay","hide_base":["head"],"visuals":{"side":"equipment.debug.ov.side"}}]}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("overlay hide");
        assert!(err.to_string().contains("Overlay requires hide_base"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn equipment_missing_side_json_fails() {
        let tmp = std::env::temp_dir().join(format!("purgatory-eq-side-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/equipment"),
            "equipment.debug.noside.json",
            r#"{"schema_version":1,"id":"equipment.debug.noside","equipment_slot":"headwear"}"#,
        );
        write_file(
            &tmp.join("shared/equipment_presentation"),
            "equipment.debug.noside.json",
            r#"{"schema_version":1,"id":"equipment.debug.noside","attachments":[{"id":"crown","bone":"head","anchor":"crown","coverage":"overlay","visuals":{"back":"equipment.debug.noside.back"}}]}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("missing side");
        let text = err.to_string();
        assert!(text.contains("missing field") || text.contains("Side visual"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn equipment_illegal_hide_unknown_base_fails() {
        let tmp = std::env::temp_dir().join(format!("purgatory-eq-hide-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/equipment"),
            "equipment.debug.hide.json",
            r#"{"schema_version":1,"id":"equipment.debug.hide","equipment_slot":"bodywear"}"#,
        );
        write_file(
            &tmp.join("shared/equipment_presentation"),
            "equipment.debug.hide.json",
            r#"{"schema_version":1,"id":"equipment.debug.hide","attachments":[{"id":"shell","bone":"torso","anchor":"chest","coverage":"replace_base","hide_base":["weapon"],"visuals":{"side":"equipment.debug.hide.side"}}]}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("illegal hide");
        assert!(err.to_string().contains("unknown base visual"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn equipment_incompatible_anchor_fails() {
        let tmp = std::env::temp_dir().join(format!("purgatory-eq-anchor-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/equipment"),
            "equipment.debug.bad_anchor.json",
            r#"{"schema_version":1,"id":"equipment.debug.bad_anchor","equipment_slot":"weapon"}"#,
        );
        write_file(
            &tmp.join("shared/equipment_presentation"),
            "equipment.debug.bad_anchor.json",
            r#"{"schema_version":1,"id":"equipment.debug.bad_anchor","attachments":[{"id":"blade","bone":"foot_front","anchor":"grip_front","coverage":"overlay","visuals":{"side":"equipment.debug.bad_anchor.side"}}]}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("bad anchor");
        let text = err.to_string();
        assert!(text.contains("bone_anchor") || text.contains("slot_target"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn equipment_excessive_correction_fails() {
        let tmp = std::env::temp_dir().join(format!("purgatory-eq-corr-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/equipment"),
            "equipment.debug.bad_corr.json",
            r#"{"schema_version":1,"id":"equipment.debug.bad_corr","equipment_slot":"headwear"}"#,
        );
        write_file(
            &tmp.join("shared/equipment_presentation"),
            "equipment.debug.bad_corr.json",
            r#"{"schema_version":1,"id":"equipment.debug.bad_corr","attachments":[{"id":"crown","bone":"head","anchor":"crown","coverage":"overlay","correction":{"x":9.0},"visuals":{"side":"equipment.debug.bad_corr.side"}}]}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("correction");
        assert!(err.to_string().contains("correction"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn equipment_duplicate_attachment_id_fails() {
        let tmp = std::env::temp_dir().join(format!("purgatory-eq-dup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/equipment"),
            "equipment.debug.dup.json",
            r#"{"schema_version":1,"id":"equipment.debug.dup","equipment_slot":"gloves"}"#,
        );
        write_file(
            &tmp.join("shared/equipment_presentation"),
            "equipment.debug.dup.json",
            r#"{"schema_version":1,"id":"equipment.debug.dup","attachments":[{"id":"hand","bone":"hand_front","anchor":"bone_origin","coverage":"overlay","visuals":{"side":"equipment.debug.dup.a"}},{"id":"hand","bone":"hand_back","anchor":"bone_origin","coverage":"overlay","visuals":{"side":"equipment.debug.dup.b"}}]}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("dup id");
        assert!(err.to_string().contains("duplicate attachment id"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn equipment_slot_mismatch_fails() {
        let tmp = std::env::temp_dir().join(format!("purgatory-eq-slot-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/equipment"),
            "equipment.debug.slot.json",
            r#"{"schema_version":1,"id":"equipment.debug.slot","equipment_slot":"headwear"}"#,
        );
        write_file(
            &tmp.join("shared/equipment_presentation"),
            "equipment.debug.slot.json",
            r#"{"schema_version":1,"id":"equipment.debug.slot","attachments":[{"id":"shell","bone":"torso","anchor":"chest","coverage":"overlay","visuals":{"side":"equipment.debug.slot.side"}}]}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("slot mismatch");
        assert!(err.to_string().contains("slot_target"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn equipment_unknown_json_field_fails() {
        let tmp = std::env::temp_dir().join(format!("purgatory-eq-unk-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/equipment"),
            "equipment.debug.unk.json",
            r#"{"schema_version":1,"id":"equipment.debug.unk","equipment_slot":"headwear","png":"cap.png"}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("unknown field");
        let text = err.to_string();
        assert!(text.contains("unknown field") || text.contains("json"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn equipment_gameplay_rejects_scale_correction() {
        let tmp = std::env::temp_dir().join(format!("purgatory-eq-scale-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        write_file(
            &tmp.join("shared/equipment"),
            "equipment.debug.scale.json",
            r#"{"schema_version":1,"id":"equipment.debug.scale","equipment_slot":"boots"}"#,
        );
        write_file(
            &tmp.join("shared/equipment_presentation"),
            "equipment.debug.scale.json",
            r#"{"schema_version":1,"id":"equipment.debug.scale","attachments":[{"id":"foot_front","bone":"foot_front","anchor":"foot_front","coverage":"overlay","correction":{"x":0,"y":0,"rotation":0,"scale":2},"visuals":{"side":"equipment.debug.scale.side"}}]}"#,
        );
        let err = load_registry(&tmp, LoadMode::Shared).expect_err("scale");
        assert!(err.to_string().contains("unknown field") || err.to_string().contains("json"));
        let _ = fs::remove_dir_all(&tmp);
    }
}
