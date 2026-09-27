//! Owned validated content. No global singleton.

use std::collections::{BTreeMap, HashMap};

use crate::dialogue::{
    DialogueAction, DialogueCondition, NpcDialogueDefinition, NpcDialoguePresentation,
};
use crate::domain::ContentDomain;
use crate::equipment::{EquipmentDefinition, EquipmentPresentation};
use crate::error::{ContentError, ValidationIssue};
use crate::item::{ItemCategory, ItemDefinition, ItemPresentation};
use crate::monster::{
    MonsterDefinition, MonsterPresentationDefinition, validate_monster_definition,
    validate_monster_presentation,
};
use crate::schema::{
    EntityDefinition, MapDefinition, Placement, PlacementKind, RestorePolicy, TransitionRef,
};
use purgatory_common::{CONTENT_MAP_START, ContentId, ContentKind, MapId};
use purgatory_simulation::AbilityDefinition;

/// Validated authored definitions. Runtime systems query this, not JSON.
#[derive(Clone, Debug, Default)]
pub struct ContentRegistry {
    labels: HashMap<ContentId, String>,
    entities: BTreeMap<String, EntityDefinition>,
    maps: BTreeMap<String, MapDefinition>,
    placements: BTreeMap<String, Vec<Placement>>,
    items: BTreeMap<String, ItemDefinition>,
    item_presentations: BTreeMap<String, ItemPresentation>,
    equipment: BTreeMap<String, EquipmentDefinition>,
    equipment_presentation: BTreeMap<String, EquipmentPresentation>,
    abilities: BTreeMap<String, AbilityDefinition>,
    monsters: BTreeMap<String, MonsterDefinition>,
    monster_presentations: BTreeMap<String, MonsterPresentationDefinition>,
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
            + self.monsters.len()
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
    pub fn monster_count(&self) -> usize {
        self.monsters.len()
    }

    #[must_use]
    pub fn monster(&self, authored: &str) -> Option<&MonsterDefinition> {
        self.monsters.get(authored)
    }

    #[must_use]
    pub fn monster_by_id(&self, id: ContentId) -> Option<&MonsterDefinition> {
        let authored = self.labels.get(&id)?;
        self.monsters.get(authored)
    }

    pub fn iter_monsters(&self) -> impl Iterator<Item = &MonsterDefinition> {
        self.monsters.values()
    }

    #[must_use]
    pub fn monster_presentation(&self, authored: &str) -> Option<&MonsterPresentationDefinition> {
        self.monster_presentations.get(authored)
    }

    #[must_use]
    pub fn monster_presentation_by_id(
        &self,
        id: ContentId,
    ) -> Option<&MonsterPresentationDefinition> {
        let authored = self.labels.get(&id)?;
        self.monster_presentations.get(authored)
    }

    pub fn iter_monster_presentations(
        &self,
    ) -> impl Iterator<Item = &MonsterPresentationDefinition> {
        self.monster_presentations.values()
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

    pub fn iter_npc_dialogues(&self) -> impl Iterator<Item = &NpcDialogueDefinition> {
        self.npc_dialogues.values()
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

    /// Client-safe runtime NPC catalogue. Only numerically allocated NPCs
    /// projected from validated authoring content appear here.
    pub fn iter_npc_dialogue_presentations(
        &self,
    ) -> impl Iterator<Item = &NpcDialoguePresentation> {
        self.npc_dialogue_presentations.values()
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

    #[must_use]
    pub fn portal_content_id(&self, map_authored: &str, portal_id: &str) -> Option<ContentId> {
        if let Some(placement) = self
            .placements(map_authored)
            .iter()
            .find(|placement| placement.kind == PlacementKind::Portal && placement.id == portal_id)
        {
            return ContentId::from_authored(&placement.content_authored).ok();
        }

        let legacy = self.placements(map_authored).iter().find(|placement| {
            placement.kind == PlacementKind::Entity && placement.content_authored == portal_id
        })?;
        let entity = self.entities.get(&legacy.content_authored)?;
        (entity.interactable == Some(purgatory_simulation::InteractableKind::Portal))
            .then_some(entity.content_id)
    }

    #[must_use]
    pub fn portal_transition_by_id(&self, id: ContentId) -> Option<TransitionRef> {
        for placements in self.placements.values() {
            if let Some(placement) = placements.iter().find(|placement| {
                placement.kind == PlacementKind::Portal
                    && ContentId::from_authored(&placement.content_authored).ok() == Some(id)
            }) {
                let link = placement.portal_link.as_ref()?;
                return Some(TransitionRef {
                    map_authored: link.map_authored.clone(),
                    portal_authored: link.portal_id.clone(),
                });
            }
        }
        self.entity_by_id(id)?.transition.clone()
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

    pub fn iter_items(&self) -> impl Iterator<Item = &ItemDefinition> {
        self.items.values()
    }

    #[must_use]
    pub fn item_presentation(&self, authored: &str) -> Option<&ItemPresentation> {
        self.item_presentations.get(authored)
    }

    #[must_use]
    pub fn item_presentation_by_id(&self, id: ContentId) -> Option<&ItemPresentation> {
        let authored = self.labels.get(&id)?;
        self.item_presentations.get(authored)
    }

    pub fn iter_item_presentations(&self) -> impl Iterator<Item = &ItemPresentation> {
        self.item_presentations.values()
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

    pub(crate) fn apply_map_gameplay(
        &mut self,
        authored: &str,
        name: &str,
        foothold_paths: Vec<crate::FootholdPath>,
        spawn_points: Vec<crate::GameplaySpawnPoint>,
    ) -> Result<(), ContentError> {
        let Some(map) = self.maps.get_mut(authored) else {
            return Err(ContentError::one(ValidationIssue::new(
                authored,
                authored,
                "map_authored",
                "gameplay authoring references an unknown map",
            )));
        };
        if !name.trim().is_empty() {
            map.debug_name = name.trim().to_owned();
        }
        map.foothold_paths = foothold_paths;
        if !spawn_points.is_empty() {
            map.spawn_points = spawn_points
                .into_iter()
                .map(|spawn| crate::SpawnPoint {
                    id: spawn.id,
                    position: spawn.position,
                })
                .collect();
        }
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

    pub(crate) fn insert_item_presentation(
        &mut self,
        def: ItemPresentation,
    ) -> Result<(), ContentError> {
        crate::item::validate_item_presentation(&def)?;
        self.intern(&def.authored_id, def.content_id, "item_presentation")?;
        if self.item_presentations.contains_key(&def.authored_id) {
            return Err(duplicate(&def.authored_id, "item_presentation"));
        }
        self.item_presentations.insert(def.authored_id.clone(), def);
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

    pub(crate) fn insert_monster(&mut self, def: MonsterDefinition) -> Result<(), ContentError> {
        if self.entities.contains_key(&def.authored_id)
            || self.maps.contains_key(&def.authored_id)
            || self.items.contains_key(&def.authored_id)
            || self.equipment.contains_key(&def.authored_id)
            || self.abilities.contains_key(&def.authored_id)
            || self.monsters.contains_key(&def.authored_id)
        {
            return Err(duplicate(&def.authored_id, "monster"));
        }
        validate_monster_definition(&def)?;
        self.intern(&def.authored_id, def.content_id, "monster")?;
        self.monsters.insert(def.authored_id.clone(), def);
        Ok(())
    }

    pub(crate) fn insert_monster_presentation(
        &mut self,
        def: MonsterPresentationDefinition,
    ) -> Result<(), ContentError> {
        validate_monster_presentation(&def)?;
        self.intern(&def.authored_id, def.content_id, "monster_presentation")?;
        if self.monster_presentations.contains_key(&def.authored_id) {
            return Err(duplicate(&def.authored_id, "monster_presentation"));
        }
        self.monster_presentations
            .insert(def.authored_id.clone(), def);
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

        let maps: Vec<ContentId> = self.maps.values().map(|map| map.content_id).collect();
        let mut legacy_next = 1u32;
        for content in maps {
            let map_id = if content.kind() == Some(ContentKind::Map) {
                let raw = content.raw().expect("numeric Map ContentId");
                MapId::from_raw(raw - CONTENT_MAP_START + 1)
            } else {
                while self
                    .content_by_map_id
                    .contains_key(&MapId::from_raw(legacy_next))
                {
                    legacy_next = legacy_next.saturating_add(1);
                }
                let id = MapId::from_raw(legacy_next);
                legacy_next = legacy_next.saturating_add(1);
                id
            };
            self.bind_map(content, map_id);
        }
    }

    fn bind_map(&mut self, content: ContentId, map: MapId) {
        self.map_id_by_content.insert(content, map);
        self.content_by_map_id.insert(map, content);
    }

    fn placement_issues(
        &self,
        map_authored: &str,
        placements: &[Placement],
    ) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        if !self.maps.contains_key(map_authored) {
            issues.push(ValidationIssue::new(
                "placements",
                map_authored,
                "map",
                "unresolved map reference",
            ));
            return issues;
        }
        let mut ids = std::collections::HashSet::new();
        for (i, p) in placements.iter().enumerate() {
            if !ids.insert(p.id.as_str()) {
                issues.push(ValidationIssue::new(
                    map_authored,
                    &p.id,
                    format!("placements[{i}].id"),
                    "duplicate placement id",
                ));
            }
            match p.kind {
                PlacementKind::Entity => {
                    if !self.entities.contains_key(&p.content_authored) {
                        issues.push(ValidationIssue::new(
                            map_authored,
                            &p.content_authored,
                            format!("placements[{i}].content"),
                            "unresolved entity reference",
                        ));
                    }
                }
                PlacementKind::Monster => {
                    if !self.monsters.contains_key(&p.content_authored) {
                        issues.push(ValidationIssue::new(
                            map_authored,
                            &p.content_authored,
                            format!("placements[{i}].content"),
                            "unresolved monster reference",
                        ));
                    }
                }
                PlacementKind::Portal => {
                    if let Some(link) = &p.portal_link {
                        if !self.maps.contains_key(&link.map_authored) {
                            issues.push(ValidationIssue::new(
                                map_authored,
                                &p.id,
                                format!("placements[{i}].linked_portal.map"),
                                "unresolved map reference",
                            ));
                        } else {
                            let target_exists = if link.map_authored == map_authored {
                                placements.iter().any(|candidate| {
                                    (candidate.kind == PlacementKind::Portal
                                        && candidate.id == link.portal_id)
                                        || (candidate.kind == PlacementKind::Entity
                                            && candidate.content_authored == link.portal_id
                                            && self
                                                .entities
                                                .get(&candidate.content_authored)
                                                .is_some_and(|entity| {
                                                    entity.interactable
                                                        == Some(
                                                            purgatory_simulation::InteractableKind::Portal,
                                                        )
                                                }))
                                })
                            } else {
                                self.portal_content_id(&link.map_authored, &link.portal_id)
                                    .is_some()
                            };
                            if !target_exists {
                                issues.push(ValidationIssue::new(
                                    map_authored,
                                    &p.id,
                                    format!("placements[{i}].linked_portal.portal"),
                                    format!(
                                        "portal '{}' is not placed on '{}'",
                                        link.portal_id, link.map_authored
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
        }
        issues
    }

    pub fn validate_placements(
        &self,
        map_authored: &str,
        placements: &[Placement],
    ) -> Result<(), ContentError> {
        let issues = self.placement_issues(map_authored, placements);
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ContentError { issues })
        }
    }

    fn validate_refs(&self) -> Result<(), ContentError> {
        let mut issues = Vec::new();
        for (map_authored, placements) in &self.placements {
            issues.extend(self.placement_issues(map_authored, placements));
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
            if self
                .portal_content_id(&tr.map_authored, &tr.portal_authored)
                .is_none()
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
        for pres in self.item_presentations.values() {
            if !self.items.contains_key(&pres.authored_id) {
                issues.push(item_equipment_issue(
                    &pres.authored_id,
                    "id",
                    "item presentation has no matching item definition",
                ));
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
                Some(item) if item.category != ItemCategory::Equipment => {
                    issues.push(item_equipment_issue(
                        &equipment.authored_id,
                        "category",
                        "equipment item must use category 'equipment'",
                    ));
                }
                Some(_) => {}
            }
        }
        for item in self.items.values() {
            if item.category == ItemCategory::Equipment
                && !self.equipment.contains_key(&item.authored_id)
            {
                issues.push(item_equipment_issue(
                    &item.authored_id,
                    "category",
                    "category 'equipment' requires a matching equipment definition",
                ));
            }
        }
        for dialogue in self.npc_dialogues.values() {
            for (beat_index, beat) in dialogue.beats.iter().enumerate() {
                for (condition_index, condition) in beat.conditions.iter().enumerate() {
                    let item_authored = match condition {
                        DialogueCondition::ItemOwned { item_authored, .. }
                        | DialogueCondition::ItemEquipped { item_authored, .. } => {
                            Some(item_authored)
                        }
                        DialogueCondition::Fact { .. }
                        | DialogueCondition::NpcMet { .. }
                        | DialogueCondition::DialogueHeard { .. } => None,
                    };
                    if let Some(item_authored) = item_authored
                        && !self.items.contains_key(item_authored)
                    {
                        issues.push(dialogue_item_issue(
                            &dialogue.authored_id,
                            format!("beats[{beat_index}].conditions[{condition_index}]"),
                            item_authored,
                        ));
                    }
                }
                for (choice_index, choice) in beat.choices.iter().enumerate() {
                    for (action_index, action) in choice.actions.iter().enumerate() {
                        let field = format!(
                            "beats[{beat_index}].choices[{choice_index}].actions[{action_index}]"
                        );
                        match action {
                            DialogueAction::GrantAbility { ability_authored } => {
                                if !self.abilities.contains_key(ability_authored) {
                                    issues.push(dialogue_ability_issue(
                                        &dialogue.authored_id,
                                        field,
                                        ability_authored,
                                    ));
                                }
                            }
                            DialogueAction::GiveItem {
                                item_authored,
                                quantity,
                            }
                            | DialogueAction::RemoveItem {
                                item_authored,
                                quantity,
                            } => {
                                let Some(item) = self.items.get(item_authored) else {
                                    issues.push(dialogue_item_issue(
                                        &dialogue.authored_id,
                                        field,
                                        item_authored,
                                    ));
                                    continue;
                                };
                                if matches!(action, DialogueAction::GiveItem { .. })
                                    && *quantity > item.stack_limit
                                {
                                    issues.push(ValidationIssue::new(
                                        "npc_dialogue",
                                        &dialogue.authored_id,
                                        field,
                                        format!(
                                            "Give Item quantity exceeds '{}' stack_limit {}",
                                            item.authored_id, item.stack_limit
                                        ),
                                    ));
                                }
                            }
                            DialogueAction::SetFact { .. } | DialogueAction::MarkNpcMet { .. } => {}
                        }
                    }
                }
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

fn dialogue_item_issue(
    definition: &str,
    field: impl std::fmt::Display,
    item_authored: &str,
) -> ValidationIssue {
    ValidationIssue::new(
        "npc_dialogue",
        definition,
        field.to_string(),
        format!("unresolved item reference '{item_authored}'"),
    )
}

fn dialogue_ability_issue(
    definition: &str,
    field: impl std::fmt::Display,
    ability_authored: &str,
) -> ValidationIssue {
    ValidationIssue::new(
        "npc_dialogue",
        definition,
        field.to_string(),
        format!("unresolved ability reference '{ability_authored}'"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        CONTENT_SCHEMA_VERSION, EntityDefinition, MapPlatform, PlacementKind, RestorePolicy,
        SpawnPoint, TransitionRef,
    };
    use purgatory_common::{MAP1, MAP1_AUTHORED, MAP2, MAP2_AUTHORED, allocated_id_for_label};
    use purgatory_simulation::{InteractableKind, PlatformKind, WorldBounds};

    fn sample_map(id: &str) -> MapDefinition {
        MapDefinition {
            content_id: allocated_id_for_label(id)
                .unwrap_or_else(|| ContentId::from_authored(id).unwrap()),
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
            foothold_paths: Vec::new(),
            restore: RestorePolicy::SafePoint {
                point_id: "default".into(),
            },
        }
    }

    #[test]
    fn valid_registration_and_lookup() {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(MAP1_AUTHORED)).unwrap();
        reg.finish().unwrap();
        let cid = MAP1;
        assert_eq!(reg.map_id(cid), Some(MapId::DEV));
        assert_eq!(reg.map_content_id(MapId::DEV), Some(cid));
        assert!(reg.map(MAP1_AUTHORED).is_some());
        assert_eq!(reg.label(cid), Some(MAP1_AUTHORED));
    }

    #[test]
    fn duplicate_map_rejected() {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(MAP1_AUTHORED)).unwrap();
        assert!(reg.insert_map(sample_map(MAP1_AUTHORED)).is_err());
    }

    #[test]
    fn map_ids_are_registry_assigned_not_file_fields() {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(MAP1_AUTHORED)).unwrap();
        reg.insert_map(sample_map(MAP2_AUTHORED)).unwrap();
        reg.finish().unwrap();
        assert_eq!(reg.map_id(MAP1), Some(MapId::DEV));
        assert_eq!(reg.map_id(MAP2), Some(MapId::from_raw(2)));
    }

    #[test]
    fn unresolved_placement_entity_fails() {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(MAP1_AUTHORED)).unwrap();
        reg.insert_placements(
            MAP1_AUTHORED.into(),
            vec![Placement {
                id: "placement.missing".into(),
                kind: PlacementKind::Entity,
                content_authored: "entity.missing.thing".into(),
                position: [0.0, 0.0],
                portal_link: None,
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

    #[test]
    fn unresolved_placement_monster_fails() {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(MAP1_AUTHORED)).unwrap();
        reg.insert_placements(
            MAP1_AUTHORED.into(),
            vec![Placement {
                id: "placement.mob_001".into(),
                kind: PlacementKind::Monster,
                content_authored: "monster.missing".into(),
                position: [0.0, 0.0],
                portal_link: None,
            }],
        )
        .unwrap();
        let err = reg.finish().expect_err("unresolved monster");
        assert!(
            err.issues
                .iter()
                .any(|issue| issue.reason.contains("unresolved monster"))
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
        reg.insert_map(sample_map(MAP1_AUTHORED)).unwrap();
        reg.insert_entity(sample_entity(
            "entity.portal.src",
            Some(TransitionRef {
                map_authored: MAP1_AUTHORED.into(),
                portal_authored: "entity.portal.missing".into(),
            }),
        ))
        .unwrap();
        let err = reg.finish().expect_err("unresolved dest");
        assert!(
            err.issues
                .iter()
                .any(|issue| issue.field == "transition.portal")
        );
    }

    #[test]
    fn map_owned_portals_link_by_map_and_portal_id() {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(MAP1_AUTHORED)).unwrap();
        let portal_a_authored = crate::portal_runtime_authored(MAP1_AUTHORED, "portal.001");
        let portal_b_authored = crate::portal_runtime_authored(MAP1_AUTHORED, "portal.002");
        reg.insert_placements(
            MAP1_AUTHORED.into(),
            vec![
                Placement {
                    id: "portal.001".into(),
                    kind: PlacementKind::Portal,
                    content_authored: portal_a_authored.clone(),
                    position: [0.0, 0.0],
                    portal_link: Some(crate::PortalLink {
                        map_authored: MAP1_AUTHORED.into(),
                        portal_id: "portal.002".into(),
                    }),
                },
                Placement {
                    id: "portal.002".into(),
                    kind: PlacementKind::Portal,
                    content_authored: portal_b_authored.clone(),
                    position: [2.0, 0.0],
                    portal_link: Some(crate::PortalLink {
                        map_authored: MAP1_AUTHORED.into(),
                        portal_id: "portal.001".into(),
                    }),
                },
            ],
        )
        .unwrap();
        reg.finish().expect("linked portals validate");

        let portal_a = ContentId::from_authored(&portal_a_authored).unwrap();
        let transition = reg
            .portal_transition_by_id(portal_a)
            .expect("portal transition");
        assert_eq!(transition.map_authored, MAP1_AUTHORED);
        assert_eq!(transition.portal_authored, "portal.002");
        assert_eq!(
            reg.portal_content_id(MAP1_AUTHORED, "portal.002"),
            ContentId::from_authored(&portal_b_authored).ok()
        );
    }

    #[test]
    fn item_facets_reject_numeric_id_that_belongs_to_another_catalog_label() {
        let authored = "equipment.debug.cloth_cap";
        let mut reg = ContentRegistry::new();
        reg.insert_item(ItemDefinition {
            content_id: purgatory_common::ITEM_CLOTH_CAP,
            authored_id: authored.into(),
            domain: ContentDomain::Shared,
            category: ItemCategory::Equipment,
            stack_limit: 1,
            drop_requires_confirmation: true,
        })
        .unwrap();
        let err = reg
            .insert_equipment(EquipmentDefinition {
                content_id: purgatory_common::ITEM_PRACTICE_SWORD,
                authored_id: authored.into(),
                slot: purgatory_simulation::EquipmentSlot::Headwear,
                domain: ContentDomain::Shared,
            })
            .expect_err("catalog label mismatch");
        assert!(err.to_string().contains("not allocated to label"));
    }

    #[test]
    fn dest_portal_must_be_placed_on_dest_map() {
        let mut reg = ContentRegistry::new();
        reg.insert_map(sample_map(MAP1_AUTHORED)).unwrap();
        reg.insert_entity(sample_entity("entity.portal.src", None))
            .unwrap();
        reg.insert_entity(sample_entity(
            "entity.portal.dest",
            Some(TransitionRef {
                map_authored: MAP1_AUTHORED.into(),
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
