//! Categorized DEV entity inspector. Presentation only.
//!
//! World rows are client `World` slots. Replica rows are observer Known-set
//! only. This module does not invent spatial candidates or merge namespaces
//! by matching `index:generation`.
//!
//! The local player is one semantic entry. Server `RuntimeEntityId` and
//! client-local `World` `EntityId` are labeled as distinct namespaces under
//! that entry; they are not peer rows.

use purgatory_simulation::{
    ContentId, DirtyFlags, EntityId, EntityKind, EntityLifecycle, InteractableKind,
    ReplicationClass, WorldAddress,
};

use super::aoi_view::{ReplicaEntityDebug, ReplicaRole};

const SEC_PLAYERS: &str = "world.entities.players";
const SEC_PLAYERS_LOCAL: &str = "world.entities.players.local";
const SEC_PLAYERS_REMOTE: &str = "world.entities.players.remote";
const SEC_INTERACTABLES: &str = "world.entities.interactables";
const SEC_PORTALS: &str = "world.entities.portals";
const SEC_PLATFORMS: &str = "world.entities.platforms";
const SEC_OTHER: &str = "world.entities.other";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldEntityInput {
    pub id: EntityId,
    pub kind: EntityKind,
    pub address: WorldAddress,
    pub lifecycle: EntityLifecycle,
    pub content_id: Option<ContentId>,
    pub has_persistent_id: bool,
    pub replication: ReplicationClass,
    pub has_transform: bool,
    pub has_health: bool,
    pub dirty: DirtyFlags,
    pub interactable: Option<InteractableKind>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InspectorSource {
    World,
    Replica,
    /// One semantic local player; both identity namespaces live on this row.
    LocalPlayer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InspectorAccent {
    LocalPlayer,
    RemotePlayer,
    Interactable,
    Portal,
    Platform,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InspectorRow {
    pub title: String,
    pub source: InspectorSource,
    pub world_line: String,
    pub replication_line: String,
    /// Server observer Known-set id. Not a client `World` slot.
    pub server_runtime_entity_id: Option<String>,
    /// Client-local `World` slot. Not a server `RuntimeEntityId`.
    pub client_world_entity_id: Option<String>,
    pub accent: InspectorAccent,
    pub sort_index: u32,
    pub sort_generation: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InspectorView {
    pub players: Vec<InspectorRow>,
    pub interactables: Vec<InspectorRow>,
    pub portals: Vec<InspectorRow>,
    pub platforms: Vec<InspectorRow>,
    pub other: Vec<InspectorRow>,
}

impl InspectorView {
    #[must_use]
    pub fn category_section_id(category: InspectorCategory) -> &'static str {
        match category {
            InspectorCategory::Players => SEC_PLAYERS,
            InspectorCategory::Interactables => SEC_INTERACTABLES,
            InspectorCategory::Portals => SEC_PORTALS,
            InspectorCategory::Platforms => SEC_PLATFORMS,
            InspectorCategory::Other => SEC_OTHER,
        }
    }

    #[must_use]
    pub fn players_local_section_id() -> &'static str {
        SEC_PLAYERS_LOCAL
    }

    #[must_use]
    pub fn players_remote_section_id() -> &'static str {
        SEC_PLAYERS_REMOTE
    }

    #[must_use]
    pub fn category_default_open(category: InspectorCategory) -> bool {
        !matches!(
            category,
            InspectorCategory::Platforms | InspectorCategory::Other
        )
    }

    #[must_use]
    pub fn known_players(&self) -> usize {
        self.players
            .iter()
            .filter(|r| r.server_runtime_entity_id.is_some())
            .count()
    }

    #[must_use]
    pub fn known_interactables(&self) -> usize {
        self.interactables
            .iter()
            .filter(|r| r.source == InspectorSource::Replica)
            .count()
    }

    #[must_use]
    pub fn known_portals(&self) -> usize {
        self.portals
            .iter()
            .filter(|r| r.source == InspectorSource::Replica)
            .count()
    }

    #[must_use]
    pub fn world_count(&self, category: InspectorCategory) -> usize {
        self.rows(category)
            .iter()
            .filter(|r| r.client_world_entity_id.is_some() || r.source == InspectorSource::World)
            .count()
    }

    #[must_use]
    pub fn world_platforms(&self) -> usize {
        self.platforms.len()
    }

    #[must_use]
    pub fn rows(&self, category: InspectorCategory) -> &[InspectorRow] {
        match category {
            InspectorCategory::Players => &self.players,
            InspectorCategory::Interactables => &self.interactables,
            InspectorCategory::Portals => &self.portals,
            InspectorCategory::Platforms => &self.platforms,
            InspectorCategory::Other => &self.other,
        }
    }

    #[must_use]
    pub fn category_summary(&self, category: InspectorCategory) -> String {
        match category {
            InspectorCategory::Platforms => {
                if self
                    .platforms
                    .iter()
                    .all(|r| r.replication_line.contains("class=None"))
                {
                    format!("{} World / repl=None", self.world_platforms())
                } else {
                    format!("{} World", self.world_platforms())
                }
            }
            InspectorCategory::Other => format!("{} World", self.other.len()),
            InspectorCategory::Players => known_and_world(
                self.known_players(),
                self.world_count(InspectorCategory::Players),
            ),
            InspectorCategory::Interactables => known_and_world(
                self.known_interactables(),
                self.world_count(InspectorCategory::Interactables),
            ),
            InspectorCategory::Portals => known_and_world(
                self.known_portals(),
                self.world_count(InspectorCategory::Portals),
            ),
        }
    }

    #[must_use]
    pub fn entities_header_summary(&self) -> String {
        format!(
            "Players {} · Interactables {} · Portals {} · Platforms {}",
            self.category_summary(InspectorCategory::Players),
            self.category_summary(InspectorCategory::Interactables),
            self.category_summary(InspectorCategory::Portals),
            self.category_summary(InspectorCategory::Platforms),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InspectorCategory {
    Players,
    Interactables,
    Portals,
    Platforms,
    Other,
}

fn known_and_world(known: usize, world: usize) -> String {
    match (known, world) {
        (k, 0) => format!("{k} Known"),
        (0, w) => format!("{w} World"),
        (k, w) => format!("{k} Known · {w} World"),
    }
}

#[must_use]
pub fn world_category(
    kind: EntityKind,
    interactable: Option<InteractableKind>,
) -> InspectorCategory {
    match kind {
        EntityKind::Player => InspectorCategory::Players,
        EntityKind::Platform => InspectorCategory::Platforms,
        EntityKind::Generic => match interactable {
            Some(InteractableKind::Portal) => InspectorCategory::Portals,
            Some(_) => InspectorCategory::Interactables,
            None => InspectorCategory::Other,
        },
    }
}

#[must_use]
pub fn world_compact_title(
    kind: EntityKind,
    id: EntityId,
    interactable: Option<InteractableKind>,
    local_player: bool,
) -> String {
    match (kind, interactable, local_player) {
        (EntityKind::Player, _, true) => "LOCAL PLAYER".into(),
        (EntityKind::Player, _, false) => format!("PLAYER {id}"),
        (EntityKind::Platform, _, _) => format!("PLATFORM {id}"),
        (_, Some(InteractableKind::Portal), _) => format!("PORTAL {id}"),
        (_, Some(_), _) => format!("INTERACTABLE {id}"),
        (EntityKind::Generic, None, _) => format!("GENERIC {id}"),
    }
}

#[must_use]
pub fn replica_compact_title(row: &ReplicaEntityDebug) -> String {
    let id = row.entity_id;
    match row.role {
        ReplicaRole::LocalPlayer => "LOCAL PLAYER".into(),
        ReplicaRole::RemotePlayer => format!("PLAYER {id} · REMOTE"),
        ReplicaRole::Interactable => format!("INTERACTABLE {id}"),
        ReplicaRole::Portal => format!("PORTAL {id}"),
        ReplicaRole::Npc => format!("NPC {id}"),
    }
}

fn world_row(entity: &WorldEntityInput, local_player: Option<EntityId>) -> InspectorRow {
    let is_local = Some(entity.id) == local_player;
    let title = world_compact_title(entity.kind, entity.id, entity.interactable, is_local);
    let accent = match world_category(entity.kind, entity.interactable) {
        InspectorCategory::Players if is_local => InspectorAccent::LocalPlayer,
        InspectorCategory::Players => InspectorAccent::RemotePlayer,
        InspectorCategory::Interactables => InspectorAccent::Interactable,
        InspectorCategory::Portals => InspectorAccent::Portal,
        InspectorCategory::Platforms => InspectorAccent::Platform,
        InspectorCategory::Other => InspectorAccent::Other,
    };
    let interactable = entity
        .interactable
        .map(|k| format!(" interactable={k}"))
        .unwrap_or_default();
    let world_line = format!(
        "{} {} content={} persist={} caps={}{}{interactable} dirty=t{}h{}m{}r{}",
        entity.address,
        entity.lifecycle,
        entity
            .content_id
            .map(|c| c.to_string())
            .unwrap_or_else(|| "-".into()),
        if entity.has_persistent_id {
            "yes"
        } else {
            "no"
        },
        if entity.has_transform { "t" } else { "-" },
        if entity.has_health { "h" } else { "-" },
        u8::from(entity.dirty.transform),
        u8::from(entity.dirty.health),
        u8::from(entity.dirty.membership),
        u8::from(entity.dirty.replication),
    );
    let replication_line = match entity.replication {
        ReplicationClass::None => "class=None · not replicated".into(),
        other => format!("class={other} · local World metadata (not observer Known-set)"),
    };
    InspectorRow {
        title,
        source: InspectorSource::World,
        world_line,
        replication_line,
        server_runtime_entity_id: None,
        client_world_entity_id: Some(entity.id.to_string()),
        accent,
        sort_index: entity.id.index(),
        sort_generation: entity.id.generation(),
    }
}

fn replica_row(row: &ReplicaEntityDebug) -> InspectorRow {
    let title = replica_compact_title(row);
    let accent = match row.role {
        ReplicaRole::LocalPlayer => InspectorAccent::LocalPlayer,
        ReplicaRole::RemotePlayer => InspectorAccent::RemotePlayer,
        ReplicaRole::Interactable => InspectorAccent::Interactable,
        ReplicaRole::Portal => InspectorAccent::Portal,
        ReplicaRole::Npc => InspectorAccent::Other,
    };
    let recent = row.recent.map(|s| format!(" · {s}")).unwrap_or_default();
    InspectorRow {
        title,
        source: InspectorSource::Replica,
        world_line: "— (server RuntimeEntityId; not a local World slot)".into(),
        replication_line: format!("{}{recent}", row.band_label),
        server_runtime_entity_id: Some(row.entity_id.to_string()),
        client_world_entity_id: None,
        accent,
        sort_index: row.entity_id.index,
        sort_generation: row.entity_id.generation,
    }
}

fn merge_local_player_row(
    world: Option<InspectorRow>,
    replica: Option<InspectorRow>,
) -> InspectorRow {
    let client_world_entity_id = world
        .as_ref()
        .and_then(|w| w.client_world_entity_id.clone());
    let server_runtime_entity_id = replica
        .as_ref()
        .and_then(|r| r.server_runtime_entity_id.clone());
    let world_line = world
        .as_ref()
        .map(|w| w.world_line.clone())
        .unwrap_or_else(|| "— (no local World slot)".into());
    let replication_line = replica
        .as_ref()
        .map(|r| r.replication_line.clone())
        .unwrap_or_else(|| "— (no observer Known-set entry)".into());
    let sort_index = replica
        .as_ref()
        .or(world.as_ref())
        .map(|r| r.sort_index)
        .unwrap_or(0);
    let sort_generation = replica
        .as_ref()
        .or(world.as_ref())
        .map(|r| r.sort_generation)
        .unwrap_or(0);
    InspectorRow {
        title: "LOCAL PLAYER".into(),
        source: InspectorSource::LocalPlayer,
        world_line,
        replication_line,
        server_runtime_entity_id,
        client_world_entity_id,
        accent: InspectorAccent::LocalPlayer,
        sort_index,
        sort_generation,
    }
}

fn sort_key(row: &InspectorRow) -> (u8, u8, u32, u32) {
    let source_rank = match row.source {
        InspectorSource::LocalPlayer => 0,
        InspectorSource::Replica => 1,
        InspectorSource::World => 2,
    };
    let accent_rank = match row.accent {
        InspectorAccent::LocalPlayer => 0,
        InspectorAccent::RemotePlayer => 1,
        InspectorAccent::Interactable => 2,
        InspectorAccent::Portal => 3,
        InspectorAccent::Platform => 4,
        InspectorAccent::Other => 5,
    };
    (
        source_rank,
        accent_rank,
        row.sort_index,
        row.sort_generation,
    )
}

/// Build categorized inspector rows. Replica rows are Known only.
#[must_use]
pub fn build(
    world_entities: impl IntoIterator<Item = WorldEntityInput>,
    local_world_player: Option<EntityId>,
    replica_rows: &[ReplicaEntityDebug],
) -> InspectorView {
    let mut view = InspectorView::default();
    let mut world_local = None;
    for entity in world_entities {
        let row = world_row(&entity, local_world_player);
        let is_local_world_player =
            entity.kind == EntityKind::Player && Some(entity.id) == local_world_player;
        if is_local_world_player {
            world_local = Some(row);
            continue;
        }
        match world_category(entity.kind, entity.interactable) {
            InspectorCategory::Players => view.players.push(row),
            InspectorCategory::Interactables => view.interactables.push(row),
            InspectorCategory::Portals => view.portals.push(row),
            InspectorCategory::Platforms => view.platforms.push(row),
            InspectorCategory::Other => view.other.push(row),
        }
    }
    let mut replica_local = None;
    for replica in replica_rows {
        let row = replica_row(replica);
        match replica.role {
            ReplicaRole::LocalPlayer => replica_local = Some(row),
            ReplicaRole::RemotePlayer => view.players.push(row),
            ReplicaRole::Interactable => view.interactables.push(row),
            ReplicaRole::Portal => view.portals.push(row),
            ReplicaRole::Npc => view.other.push(row),
        }
    }
    if world_local.is_some() || replica_local.is_some() {
        view.players
            .push(merge_local_player_row(world_local, replica_local));
    }
    view.players.sort_by_key(sort_key);
    view.interactables.sort_by_key(sort_key);
    view.portals.sort_by_key(sort_key);
    view.platforms.sort_by_key(sort_key);
    view.other.sort_by_key(sort_key);
    view
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_protocol::{ReplicatedKind, WireEntityId};

    use super::super::aoi_view::{AoiBand, band_label};

    fn eid(index: u32) -> EntityId {
        EntityId::from_raw(index, 1)
    }

    fn world_ent(
        index: u32,
        kind: EntityKind,
        replication: ReplicationClass,
        interactable: Option<InteractableKind>,
    ) -> WorldEntityInput {
        WorldEntityInput {
            id: eid(index),
            kind,
            address: WorldAddress::DEV,
            lifecycle: EntityLifecycle::Active,
            content_id: None,
            has_persistent_id: false,
            replication,
            has_transform: true,
            has_health: false,
            dirty: DirtyFlags::default(),
            interactable,
        }
    }

    fn replica(
        index: u32,
        role: ReplicaRole,
        kind: ReplicatedKind,
        local: bool,
    ) -> ReplicaEntityDebug {
        let entity_id = WireEntityId {
            index,
            generation: 1,
        };
        ReplicaEntityDebug {
            entity_id,
            role,
            label: super::super::aoi_view::semantic_label(kind, entity_id, local),
            band: AoiBand::InEnter,
            band_label: band_label(AoiBand::InEnter),
            recent: None,
            position: [0.0, 0.0],
        }
    }

    #[test]
    fn platforms_do_not_bury_players_or_replica_kinds() {
        let world = [
            world_ent(1, EntityKind::Platform, ReplicationClass::None, None),
            world_ent(2, EntityKind::Platform, ReplicationClass::None, None),
            world_ent(
                3,
                EntityKind::Player,
                ReplicationClass::VisibleObservers,
                None,
            ),
        ];
        let replica_rows = vec![
            replica(26, ReplicaRole::LocalPlayer, ReplicatedKind::Player, true),
            replica(35, ReplicaRole::RemotePlayer, ReplicatedKind::Player, false),
            replica(
                27,
                ReplicaRole::Interactable,
                ReplicatedKind::Interactable,
                false,
            ),
            replica(28, ReplicaRole::Portal, ReplicatedKind::Portal, false),
        ];
        let view = build(world, Some(eid(3)), &replica_rows);
        assert_eq!(view.platforms.len(), 2);
        let local_rows: Vec<_> = view
            .players
            .iter()
            .filter(|r| r.accent == InspectorAccent::LocalPlayer)
            .collect();
        assert_eq!(local_rows.len(), 1, "local player must be one semantic row");
        assert_eq!(local_rows[0].title, "LOCAL PLAYER");
        assert_eq!(local_rows[0].source, InspectorSource::LocalPlayer);
        assert_eq!(
            local_rows[0].server_runtime_entity_id.as_deref(),
            Some("26:1")
        );
        assert_eq!(local_rows[0].client_world_entity_id.as_deref(), Some("3:1"));
        assert!(
            view.players
                .iter()
                .any(|r| r.title.contains("PLAYER 35:1 · REMOTE")
                    && r.source == InspectorSource::Replica)
        );
        assert_eq!(view.interactables.len(), 1);
        assert!(view.interactables[0].title.starts_with("INTERACTABLE 27:1"));
        assert_eq!(view.portals.len(), 1);
        assert!(view.portals[0].title.starts_with("PORTAL 28:1"));
        assert_eq!(view.known_players(), 2);
        assert_eq!(view.world_platforms(), 2);
        assert_eq!(
            view.category_summary(InspectorCategory::Platforms),
            "2 World / repl=None"
        );
        assert_eq!(
            view.category_summary(InspectorCategory::Players),
            "2 Known · 1 World"
        );
        assert!(
            view.platforms
                .iter()
                .all(|r| r.replication_line.contains("class=None"))
        );
        let remote = view
            .players
            .iter()
            .find(|r| r.title.contains("REMOTE"))
            .unwrap();
        assert!(remote.replication_line.contains("Known"));
        assert!(!remote.replication_line.contains("class=None"));
        assert!(remote.world_line.contains("not a local World slot"));
    }

    #[test]
    fn world_generic_portal_capability_is_not_a_platform() {
        let world = [world_ent(
            9,
            EntityKind::Generic,
            ReplicationClass::VisibleObservers,
            Some(InteractableKind::Portal),
        )];
        let view = build(world, None, &[]);
        assert!(view.platforms.is_empty());
        assert_eq!(view.portals.len(), 1);
        assert!(view.portals[0].title.starts_with("PORTAL 9:1"));
    }

    #[test]
    fn world_chest_is_interactable_not_player() {
        let world = [world_ent(
            7,
            EntityKind::Generic,
            ReplicationClass::VisibleObservers,
            Some(InteractableKind::Chest),
        )];
        let view = build(world, None, &[]);
        assert_eq!(view.interactables.len(), 1);
        assert!(view.players.is_empty());
        assert!(
            view.interactables[0]
                .world_line
                .contains("interactable=Chest")
        );
    }

    #[test]
    fn replica_sorts_local_before_remote_inside_players() {
        let replica_rows = vec![
            replica(40, ReplicaRole::RemotePlayer, ReplicatedKind::Player, false),
            replica(10, ReplicaRole::LocalPlayer, ReplicatedKind::Player, true),
        ];
        let view = build([], None, &replica_rows);
        assert_eq!(view.players[0].title, "LOCAL PLAYER");
        assert_eq!(view.players[0].source, InspectorSource::LocalPlayer);
        assert_eq!(
            view.players[0].server_runtime_entity_id.as_deref(),
            Some("10:1")
        );
        assert!(view.players[0].client_world_entity_id.is_none());
        assert!(view.players[1].title.contains("REMOTE"));
    }

    #[test]
    fn local_player_does_not_split_namespaces_into_peer_rows() {
        let world = [world_ent(
            1,
            EntityKind::Player,
            ReplicationClass::VisibleObservers,
            None,
        )];
        let replica_rows = vec![replica(
            26,
            ReplicaRole::LocalPlayer,
            ReplicatedKind::Player,
            true,
        )];
        let view = build(world, Some(eid(1)), &replica_rows);
        assert_eq!(view.players.len(), 1);
        assert_eq!(view.players[0].title, "LOCAL PLAYER");
        assert_eq!(view.players[0].source, InspectorSource::LocalPlayer);
        assert_eq!(
            view.players[0].server_runtime_entity_id.as_deref(),
            Some("26:1")
        );
        assert_eq!(
            view.players[0].client_world_entity_id.as_deref(),
            Some("1:1")
        );
        assert!(!view.players.iter().any(|r| r.title.contains("PLAYER 26:1")));
        assert!(!view.players.iter().any(|r| r.title.contains("PLAYER 1:1")));
    }

    #[test]
    fn category_defaults_expand_gameplay_and_collapse_platforms() {
        assert!(InspectorView::category_default_open(
            InspectorCategory::Players
        ));
        assert!(InspectorView::category_default_open(
            InspectorCategory::Interactables
        ));
        assert!(InspectorView::category_default_open(
            InspectorCategory::Portals
        ));
        assert!(!InspectorView::category_default_open(
            InspectorCategory::Platforms
        ));
        assert!(!InspectorView::category_default_open(
            InspectorCategory::Other
        ));
    }
}
