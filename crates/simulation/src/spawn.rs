//! Content-backed and transient runtime spawn request.

use crate::body::PlayerState;
use crate::equipment::EquipmentState;
use crate::health::Health;
use crate::interactable::Interactable;
use crate::npc::NpcState;
use crate::platform::Platform;
use crate::replication::ReplicationMeta;
use crate::transform::Transform;
use purgatory_common::{ContentId, PersistentId, WorldAddress};

/// Validated spawn description. Not every entity comes from content.
#[derive(Clone, Debug)]
pub struct RuntimeSpawnRequest {
    pub address: WorldAddress,
    pub transform: Option<Transform>,
    pub content_id: Option<ContentId>,
    pub persistent_id: Option<PersistentId>,
    pub replication: ReplicationMeta,
    pub player: Option<PlayerState>,
    pub platform: Option<Platform>,
    pub health: Option<Health>,
    pub interactable: Option<Interactable>,
    pub npc: Option<NpcState>,
    pub equipment: Option<EquipmentState>,
}

impl RuntimeSpawnRequest {
    #[must_use]
    pub fn transient_at(address: WorldAddress) -> Self {
        Self {
            address,
            transform: None,
            content_id: None,
            persistent_id: None,
            replication: ReplicationMeta::none(),
            player: None,
            platform: None,
            health: None,
            interactable: None,
            npc: None,
            equipment: None,
        }
    }

    #[must_use]
    pub fn with_transform(mut self, transform: Transform) -> Self {
        self.transform = Some(transform);
        self
    }

    #[must_use]
    pub fn with_content(mut self, content_id: ContentId) -> Self {
        self.content_id = Some(content_id);
        self
    }

    #[must_use]
    pub fn visible(mut self) -> Self {
        self.replication = ReplicationMeta::visible_observers();
        self
    }

    #[must_use]
    pub fn with_health(mut self, health: Health) -> Self {
        self.health = Some(health);
        self
    }

    #[must_use]
    pub fn with_interactable(mut self, interactable: Interactable) -> Self {
        self.interactable = Some(interactable);
        self
    }

    #[must_use]
    pub fn with_npc(mut self, npc: NpcState) -> Self {
        self.npc = Some(npc);
        self
    }

    #[must_use]
    pub fn with_equipment(mut self, equipment: EquipmentState) -> Self {
        self.equipment = Some(equipment);
        self
    }

    #[must_use]
    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    #[must_use]
    pub fn at_address(mut self, address: WorldAddress) -> Self {
        self.address = address;
        self
    }
}
