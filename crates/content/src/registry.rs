//! Owned validated content. No global singleton.

use std::collections::{BTreeMap, HashMap};

use crate::dialogue::{NpcDialogueDefinition, NpcDialoguePresentation};
use crate::domain::ContentDomain;
use crate::equipment::{EquipmentDefinition, EquipmentPresentation};
use crate::error::{ContentError, ValidationIssue};
use crate::item::ItemDefinition;
use crate::schema::{EntityDefinition, MapDefinition, Placement, RestorePolicy};
use purgatory_common::{ContentId, MAP_FOOTNOTE_AUTHORED, MapId};
use purgatory_simulation::AbilityDefinition;

/// Validated authored definitions. Runtime systems query this, not JSON.
#[derive(Clone, Debug, Default)]
pub struct ContentRegistry {
    labels: HashMap<ContentId, String>,
    entities: BTreeMap<String, EntityDefinition>,
    maps: BTreeMap<String, MapDefinition>,
    placements: BTreeMap<String, Vec<Placement>>,
    items: BTreeMap<String, ItemDefinition>,
    equipment: BTreeMap<String, EquipmentDefinition>,
    equipment_presentation: BTreeMap<String, EquipmentPresentation>,
    abilities: BTreeMap<String, AbilityDefinition>,
    npc_dialogues: BTreeMap<String, NpcDialogueDefinition>,
    npc_dialogue_presentations: BTreeMap<String, NpcDialoguePresentation>,
    map_id_by_content: HashMap<ContentId, MapId>,
    content_by_map_id: HashMap<MapId, ContentId>,
}

impl ContentRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn definition_count(&self) -> usize {
        self.entities.len()
            + self.maps.len()
            + self.items.len()
            + self.equipment.len()
            + self.abilities.len()
            + self.npc_dialogues.len()
    }

    #[must_use]
    pub fn map_count(&self) -> usize {
        self.maps.len()
    }

    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    #[must_use]
    pub fn ability_count(&self) -> usize {
        self.abilities.len()
    }

    #[must_use]
    pub fn ability(&self, authored: &str) -> Option<&AbilityDefinition> {
        self.abilities.get(authored)
    }

    #[must_use]
    pub fn ability_by_id(&self, id: ContentId) -> Option<&AbilityDefinition> {
        let authored = self.labels.get(&id)?;
        self.abilities.get(authored)
    }

    #[must_use]
    pub fn npc_dialogue_count(&self) -> usize {
        self.npc_dialogues.len()
    }

    #[must_use]
    pub fn npc_dialogue(&self, authored: &str) -> Option<&NpcDialogueDefinition> {
        self.npc_dialogues.get(authored)
    }

    #[must_use]
    pub fn npc_dialogue_by_id(&self, id: ContentId) -> Option<&NpcDialogueDefinition> {
        let authored = self.labels.get(&id)?;
        self.npc_dialogues.get(authored)
    }

    /// Client-safe dialogue lines keyed by the same stable NPC identity. This
    /// projection intentionally excludes authoritative conditions and actions.
    #[must_use]
    pub fn npc_dialogue_presentation_by_id(
        &self,
        id: ContentId,
    ) -> Option<&NpcDialoguePresentation> {
        let authored = self.labels.get(&id)?;
        self.npc_dialogue_presentations.get(authored)
    }

    #[must_use]
    pub fn label(&self, id: ContentId) -> Option<&str> {
        self.labels.get(&id).map(String::as_str)
    }

    #[must_use]
    pub fn entity(&self, authored: &str) -> Option<&EntityDefinition> {
        self.entities.get(authored)
    }

    #[must_use]
    pub fn entity_by_id(&self, id: ContentId) -> Option<&EntityDefinition> {
        let authored = self.labels.get(&id)?;
        self.entities.get(authored)
    }

    #[must_use]
    pub fn map(&self, authored: &str) -> Option<&MapDefinition> {
        self.maps.get(authored)
    }

    #[must_use]
    pub fn map_by_id(&self, id: ContentId) -> Option<&MapDefinition> {
        let authored = self.labels.get(&id)?;
        self.maps.get(authored)
    }

    /// Sole source of `Map ContentId ↔ MapId`.
    #[must_use]
    pub fn map_id(&self, content: ContentId) -> Option<MapId> {
        self.map_id_by_content.get(&content).copied()
    }

    #[must_use]
    pub fn map_content_id(&self, map: MapId) -> Option<ContentId> {
        self.content_by_map_id.get(&map).copied()
    }

    #[must_use]
    pub fn map_by_map_id(&self, map: MapId) -> Option<&MapDefinition> {
        let content = self.map_content_id(map)?;
        self.map_by_id(content)
    }

    #[must_use]
    pub fn placements(&self, map_authored: &str) -> &[Placement] {
        self.placements
            .get(map_authored)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn iter_maps(&self) -> impl Iterator<Item = &MapDefinition> {
        self.maps.values()
    }

    pub fn iter_entities(&self) -> impl Iterator<Item = &EntityDefinition> {
        self.entities.values()
    }

    #[must_use]
    pub fn equipment_count(&self) -> usize {
        self.equipment.len()
    }

    #[must_use]
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn item(&self, authored: &str) -> Option<&ItemDefinition> {
        self.items.get(authored)
    }

    #[must_use]
    pub fn item_by_id(&self, id: ContentId) -> Option<&ItemDefinition> {
        let authored = self.labels.get(&id)?;
        self.items.get(authored)
    }

    #[must_use]
    pub fn equipment(&self, authored: &str) -> Option<&EquipmentDefinition> {
        self.equipment.get(authored)
    }

    #[must_use]
    pub fn equipment_by_id(&self, id: ContentId) -> Option<&EquipmentDefinition> {
        let authored = self.labels.get(&id)?;
        self.equipment.get(authored)
    }

    #[must_use]
    pub fn equipment_presentation(&self, authored: &str) -> Option<&EquipmentPresentation> {
        self.equipment_presentation.get(authored)
    }

    /// Client presentation lookup by the same `ContentId` as gameplay.
    /// Server `authorize_equip` must not use this.
    #[must_use]
    pub fn equipment_presentation_by_id(&self, id: ContentId) -> Option<&EquipmentPresentation> {
        let authored = self.labels.get(&id)?;
        self.equipment_presentation.get(authored)
    }

    pub fn iter_equipment(&self) -> impl Iterator<Item = &EquipmentDefinition> {
        self.equipment.values()
    }

    pub(crate) fn insert_entity(&mut self, def: EntityDefinition) -> Result<(), ContentError> {
        if let Some(dialogue) = self.npc_dialogues.get(&def.authored_id)
            && dialogue.content_id != def.content_id
        {
            return Err(ContentError::one(npc_entity_issue(
                &def.authored_id,
                "NPC entity and dialogue definitions must share the same ContentId",
            )));
        }
        self.intern(&def.authored_id, def.content_id, "entity")?;
        if self.entities.contains_key(&def.authored_id) {
            return Err(duplicate(&def.authored_id, "entity"));
        }
        self.entities.insert(def.authored_id.clone(), def);
        Ok(())
    }

    pub(crate) fn insert_map(&mut self, def: MapDefinition) -> Result<(), ContentError> {
        self.intern(&def.authored_id, def.content_id, "map")?;
        if self.maps.contains_key(&def.authored_id) {
            return Err(duplicate(&def.authored_id, "map"));
        }
        self.maps.insert(def.authored_id.clone(), def);
        Ok(())
    }

    pub(crate) fn insert_equipment(
        &mut self,
        def: EquipmentDefinition,
    ) -> Result<(), ContentError> {
        if self.entities.contains_key(&def.authored_id) || self.maps.contains_key(&def.authored_id)
        {
            return Err(duplicate(&def.authored_id, "equipment"));
        }
        crate::equipment::validate_equipment_definition(&def)?;
        if let Some(item) = self.items.get(&def.authored_id)
            && item.content_id != def.content_id
        {
            return Err(ContentError::one(item_equipment_issue(
                &def.authored_id,
                "id",
                "item and equipment definitions must share the same ContentId",
            )));
        }
        self.intern(&def.authored_id, def.content_id, "equipment")?;
        if self.equipment.contains_key(&def.authored_id) {
            return Err(duplicate(&def.authored_id, "equipment"));
        }
        self.equipment.insert(def.authored_id.clone(), def);
        Ok(())
    }

    pub(crate) fn insert_item(&mut self, def: ItemDefinition) -> Result<(), ContentError> {
        if self.entities.contains_key(&def.authored_id)
            || self.maps.contains_key(&def.authored_id)
            || self.abilities.contains_key(&def.authored_id)
        {
            return Err(duplicate(&def.authored_id, "item"));
        }
        crate::item::validate_item_definition(&def)?;
        if let Some(equipment) = self.equipment.get(&def.authored_id)
            && equipment.content_id != def.content_id
        {
            return Err(ContentError::one(item_equipment_issue(
                &def.authored_id,
                "id",
                "item and equipment definitions must share the same ContentId",
            )));
        }
        self.intern(&def.authored_id, def.content_id, "item")?;
        if self.items.contains_key(&def.authored_id) {
            return Err(duplicate(&def.authored_id, "item"));
        }
        self.items.insert(def.authored_id.clone(), def);
        Ok(())
    }

    pub(crate) fn insert_equipment_presentation(
        &mut self,
        def: EquipmentPresentation,
    ) -> Result<(), ContentError> {
        self.intern(&def.authored_id, def.content_id, "equipment_presentation")?;
        if self.equipment_presentation.contains_key(&def.authored_id) {
            return Err(duplicate(&def.authored_id, "equipment_presentation"));
        }
        self.equipment_presentation
            .insert(def.authored_id.clone(), def);
        Ok(())
    }

    pub(crate) fn insert_ability(
        &mut self,
        authored: String,
        def: AbilityDefinition,
    ) -> Result<(), ContentError> {
        if self.entities.contains_key(&authored)
            || self.maps.contains_key(&authored)
            || self.equipment.contains_key(&authored)
            || self.abilities.contains_key(&authored)
        {
            return Err(duplicate(&authored, "ability"));
        }
        self.intern(&authored, def.id, "ability")?;
        self.abilities.insert(authored, def);
        Ok(())
    }

    pub(crate) fn insert_npc_dialogue(
        &mut self,
        def: NpcDialogueDefinition,
    ) -> Result<(), ContentError> {
        if self.maps.contains_key(&def.authored_id)
            || self.items.contains_key(&def.authored_id)
            || self.equipment.contains_key(&def.authored_id)
            || self.abilities.contains_key(&def.authored_id)
            || self.npc_dialogues.contains_key(&def.authored_id)
        {
            return Err(duplicate(&def.authored_id, "npc_dialogue"));
        }
        if let Some(entity) = self.entities.get(&def.authored_id)
            && entity.content_id != def.content_id
        {
            return Err(ContentError::one(npc_entity_issue(
                &def.authored_id,
                "NPC entity and dialogue definitions must share the same ContentId",
            )));
        }
        self.intern(&def.authored_id, def.content_id, "npc_dialogue")?;
        self.npc_dialogues.insert(def.authored_id.clone(), def);
        Ok(())
    }

    pub(crate) fn insert_npc_dialogue_presentation(
        &mut self,
        def: NpcDialoguePresentation,
    ) -> Result<(), ContentError> {
        self.intern(
            &def.authored_id,
            def.content_id,
            "npc_dialogue_presentation",
        )?;
        if self
            .npc_dialogue_presentations
            .contains_key(&def.authored_id)
        {
            return Err(duplicate(&def.authored_id, "npc_dialogue_presentation"));
        }
        self.npc_dialogue_presentations
            .insert(def.authored_id.clone(), def);
        Ok(())
    }

    pub(crate) fn insert_placements(
        &mut self,
        map_authored: String,
        placements: Vec<Placement>,
    ) -> Result<(), ContentError> {
        if self.placements.contains_key(&map_authored) {
            return Err(duplicate(&map_authored, "placements"));
        }
        self.placements.insert(map_authored, placements);
        Ok(())
    }

    pub(crate) fn finish(&mut self) -> Result<(), ContentError> {
        self.assign_map_ids();
        self.validate_refs()
    }

    fn intern(&mut self, authored: &str, id: ContentId, kind: &str) -> Result<(), ContentError> {
        if let Some(existing) = self.labels.get(&id)
            && existing != authored
        {
            return Err(ContentError::one(ValidationIssue::new(
                kind,
                authored,
                "id",
                format!("token collision with {existing}"),
            )));
        }
        self.labels.insert(id, authored.to_string());
        Ok(())
    }

    fn assign_map_ids(&mut self) {
        self.map_id_by_content.clear();
        self.content_by_map_id.clear();
        if let Ok(footnote) = ContentId::from_authored(MAP_FOOTNOTE_AUTHORED)
            && self.maps.contains_key(MAP_FOOTNOTE_AUTHORED)
        {
            self.bind_map(footnote, MapId::DEV);
        }
        let mut next = 2u32;
        let authoreds: Vec<String> = self.maps.keys().cloned().collect();
        for authored in authoreds {
            if authored == MAP_FOOTNOTE_AUTHORED {
                continue;
            }
            let Ok(cid) = ContentId::from_authored(&authored) else {
                continue;
            };
            if next == MapId::DEV.raw() {
                next = next.saturating_add(1);
            }
            self.bind_map(cid, MapId::from_raw(next));
            next = next.saturating_add(1);
        }
    }

    fn bind_map(&mut self, content: ContentId, map: MapId) {
        self.map_id_by_content.insert(content, map);
        self.content_by_map_id.insert(map, content);
    }

    fn validate_refs(&self) -> Result<(), ContentError> {
        let mut issues = Vec::new();
        for (map_authored, placements) in &self.placements {
            if !self.maps.contains_key(map_authored) {
                issues.push(ValidationIssue::new(
                    "placements",
                    map_authored,
                    "map",
                    "unresolved map reference",
                ));
                continue;
            }
            for (i, p) in placements.iter().enumerate() {
                match self.entities.get(&p.entity_authored) {
                    None => issues.push(ValidationIssue::new(
                        map_authored,
                        &p.entity_authored,
                        format!("placements[{i}].entity"),
                        "unresolved entity reference",
                    )),
                    Some(ent)
                        if self.maps[map_authored].domain == ContentDomain::Shared
                            && ent.domain == ContentDomain::ServerOnly =>
                    {
                        // Shared maps may be listed with server placements; OK.
                    }
                    Some(_) => {}
                }
            }
        }
        for ent in self.entities.values() {
            let Some(tr) = &ent.transition else {
                continue;
            };
            if ent.domain != ContentDomain::ServerOnly {
                issues.push(ValidationIssue::new(
                    &ent.authored_id,
                    &ent.authored_id,
                    "transition",
                    "transition is server-only",
                ));
            }
            if !self.maps.contains_key(&tr.map_authored) {
                issues.push(ValidationIssue::new(
                    &ent.authored_id,
                    &ent.authored_id,
                    "transition.map",
                    "unresolved map reference",
                ));
            }
            if !self.entities.contains_key(&tr.portal_authored) {
                issues.push(ValidationIssue::new(
                    &ent.authored_id,
                    &ent.authored_id,
                    "transition.portal",
                    "unresolved portal reference",
                ));
            } else if self
                .placements
                .get(&tr.map_authored)
                .is_none_or(|placements| {
                    placements
                        .iter()
                        .all(|p| p.entity_authored != tr.portal_authored)
                })
            {
                issues.push(ValidationIssue::new(
                    &ent.authored_id,
                    &ent.authored_id,
                    "transition.portal",
                    format!(
                        "portal '{}' is not placed on '{}'",
                        tr.portal_authored, tr.map_authored
                    ),
                ));
            }
        }
        for map in self.maps.values() {
            match &map.restore {
                RestorePolicy::SafePoint { point_id } | RestorePolicy::Checkpoint { point_id } => {
                    if !map.spawn_points.iter().any(|s| s.id == *point_id) {
                        issues.push(ValidationIssue::new(
                            &map.authored_id,
                            &map.authored_id,
                            "restore.point",
                            format!("unknown spawn point {point_id}"),
                        ));
                    }
                }
                RestorePolicy::NonReenterable {
                    fallback_map,
                    fallback_point,
                } => match self.maps.get(fallback_map) {
                    None => issues.push(ValidationIssue::new(
                        &map.authored_id,
                        &map.authored_id,
                        "restore.fallback_map",
                        "unresolved map reference",
                    )),
                    Some(fallback)
                        if !fallback
                            .spawn_points
                            .iter()
                            .any(|s| s.id == *fallback_point) =>
                    {
                        issues.push(ValidationIssue::new(
                            &map.authored_id,
                            &map.authored_id,
                            "restore.fallback_point",
                            format!("unknown spawn point {fallback_point}"),
                        ));
                    }
                    Some(_) => {}
                },
            }
        }
        for pres in self.equipment_presentation.values() {
            match self.equipment.get(&pres.authored_id) {
                None => issues.push(crate::equipment::equip_issue(
                    &pres.authored_id,
                    None,
                    None,
                    "id",
                    "schema",
                    "presentation has no matching equipment gameplay definition",
                )),
                Some(eq) => {
                    if let Err(err) =
                        crate::equipment::validate_equipment_presentation(pres, eq.slot)
                    {
                        issues.extend(err.issues);
                    }
                }
            }
        }
        for equipment in self.equipment.values() {
            match self.items.get(&equipment.authored_id) {
                None => issues.push(item_equipment_issue(
                    &equipment.authored_id,
                    "item",
                    "equipment definition requires a matching item definition",
                )),
                Some(item) if item.content_id != equipment.content_id => {
                    issues.push(item_equipment_issue(
                        &equipment.authored_id,
                        "id",
                        "item and equipment definitions must share the same ContentId",
                    ));
                }
                Some(_) => {}
            }
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ContentError { issues })
        }
    }
}

fn duplicate(id: &str, kind: &str) -> ContentError {
    ContentError::one(ValidationIssue::new(kind, id, "id", "duplicate ContentId"))
}

fn item_equipment_issue(
    definition: &str,
    field: &str,
    detail: impl std::fmt::Display,
) -> ValidationIssue {
    ValidationIssue::new("item", definition, field, detail.to_string())
}

fn npc_entity_issue(definition: &str, detail: impl std::fmt::Display) -> ValidationIssue {
    ValidationIssue::new("npc_dialogue", definition, "id", detail.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        CONTENT_SCHEMA_VERSION, EntityDefinition, MapPlatform, RestorePolicy, SpawnPoint,
        TransitionRef,
    };
    use purgatory_simulation::{InteractableKind, PlatformKind, WorldBounds};

    fn sample_map(id: &str) -> MapDefinition {
        MapDefinition {
            content_id: ContentId::from_authored(id).unwrap(),
            authored_id: id.into(),
            debug_name: id.into(),
            domain: ContentDomain::Shared,
            bounds: WorldBounds::FOOTNOTE_TEST,
            spawn_points: vec![SpawnPoint {
                id: "default".into(),
                position: [0.0, 0.0],
            }],
            platforms: vec![MapPlatform {
                position: [0.0, 0.0],
                half_extents: [1.0, 0.2],
                kind: PlatformKind::Solid,
            }],
            restore: RestorePolicy::SafePoint {
                point_id: "default".into(),
            },
        }
    }

    #[test]
    fn valid_registration_and_lookup() {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(MAP_FOOTNOTE_AUTHORED)).unwrap();
        reg.finish().unwrap();
        let cid = ContentId::from_authored(MAP_FOOTNOTE_AUTHORED).unwrap();
        assert_eq!(reg.map_id(cid), Some(MapId::DEV));
        assert_eq!(reg.map_content_id(MapId::DEV), Some(cid));
        assert!(reg.map(MAP_FOOTNOTE_AUTHORED).is_some());
        assert_eq!(reg.label(cid), Some(MAP_FOOTNOTE_AUTHORED));
    }

    #[test]
    fn duplicate_map_rejected() {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(MAP_FOOTNOTE_AUTHORED)).unwrap();
        assert!(reg.insert_map(sample_map(MAP_FOOTNOTE_AUTHORED)).is_err());
    }

    #[test]
    fn map_ids_are_registry_assigned_not_file_fields() {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(MAP_FOOTNOTE_AUTHORED)).unwrap();
        reg.insert_map(sample_map("map.dev.second")).unwrap();
        reg.finish().unwrap();
        assert_eq!(
            reg.map_id(ContentId::from_authored(MAP_FOOTNOTE_AUTHORED).unwrap()),
            Some(MapId::DEV)
        );
        assert_eq!(
            reg.map_id(ContentId::from_authored("map.dev.second").unwrap()),
            Some(MapId::from_raw(2))
        );
    }

    #[test]
    fn unresolved_placement_entity_fails() {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(MAP_FOOTNOTE_AUTHORED)).unwrap();
        reg.insert_placements(
            MAP_FOOTNOTE_AUTHORED.into(),
            vec![Placement {
                entity_authored: "entity.missing.thing".into(),
                position: [0.0, 0.0],
            }],
        )
        .unwrap();
        let err = reg.finish().expect_err("unresolved");
        assert!(
            err.issues
                .iter()
                .any(|i| i.reason.contains("unresolved entity"))
        );
    }

    fn sample_entity(id: &str, transition: Option<TransitionRef>) -> EntityDefinition {
        EntityDefinition {
            content_id: ContentId::from_authored(id).unwrap(),
            authored_id: id.into(),
            debug_name: id.into(),
            domain: ContentDomain::ServerOnly,
            visible: true,
            interactable: Some(InteractableKind::Portal),
            transition,
        }
    }

    #[test]
    fn unresolved_portal_destination_fails() {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(MAP_FOOTNOTE_AUTHORED)).unwrap();
        reg.insert_entity(sample_entity(
            "entity.portal.src",
            Some(TransitionRef {
                map_authored: MAP_FOOTNOTE_AUTHORED.into(),
                portal_authored: "entity.portal.missing".into(),
            }),
        ))
        .unwrap();
        let err = reg.finish().expect_err("unresolved dest");
        assert!(
            err.issues
                .iter()
                .any(|i| i.field == "transition.portal" && i.reason.contains("unresolved portal"))
        );
    }

    #[test]
    fn mismatched_item_and_equipment_content_ids_are_rejected() {
        let authored = "equipment.debug.mismatch";
        let mut reg = ContentRegistry::new();
        reg.insert_item(ItemDefinition {
            content_id: ContentId::from_authored(authored).unwrap(),
            authored_id: authored.into(),
            domain: ContentDomain::Shared,
            stack_limit: 1,
        })
        .unwrap();
        let err = reg
            .insert_equipment(EquipmentDefinition {
                content_id: ContentId::from_authored("equipment.debug.other").unwrap(),
                authored_id: authored.into(),
                slot: purgatory_simulation::EquipmentSlot::Headwear,
                domain: ContentDomain::Shared,
            })
            .expect_err("mismatched ids");
        assert!(
            err.to_string()
                .contains("item and equipment definitions must share")
        );
    }

    #[test]
    fn dest_portal_must_be_placed_on_dest_map() {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(MAP_FOOTNOTE_AUTHORED)).unwrap();
        reg.insert_entity(sample_entity("entity.portal.src", None))
            .unwrap();
        reg.insert_entity(sample_entity(
            "entity.portal.dest",
            Some(TransitionRef {
                map_authored: MAP_FOOTNOTE_AUTHORED.into(),
                portal_authored: "entity.portal.src".into(),
            }),
        ))
        .unwrap();
        let err = reg.finish().expect_err("unplaced dest");
        assert!(
            err.issues
                .iter()
                .any(|i| i.reason.contains("is not placed on"))
        );
    }

    #[allow(dead_code)]
    fn _schema_version_is_one() {
        assert_eq!(CONTENT_SCHEMA_VERSION, 1);
    }
}
