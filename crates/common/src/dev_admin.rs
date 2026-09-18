//! Local Developer Hub <-> dedicated-server admin control contract.
//!
//! This is a DEV-only loopback control plane. It is deliberately separate from
//! the public gameplay protocol: the Hub is not a fake player connection.

use serde::{Deserialize, Serialize};

pub const DEV_ADMIN_PORT_ENV: &str = "PURGATORY_DEV_ADMIN_PORT";
pub const DEFAULT_DEV_ADMIN_PORT: u16 = 7790;
pub const DEV_ADMIN_MAX_LINE_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DevAdminRequest {
    Snapshot,
    SpawnNpc {
        connection_id: u64,
        npc_content_id: u32,
    },
    SpawnItem {
        connection_id: u64,
        item_content_id: u32,
        quantity: u32,
    },
    SetNarrativeFact {
        connection_id: u64,
        fact_id: String,
        value: bool,
    },
    ClearNarrativeFact {
        connection_id: u64,
        fact_id: String,
    },
    MarkNpcMet {
        connection_id: u64,
        npc_authored_id: String,
    },
    ResetNarrative {
        connection_id: u64,
    },
    ResetPlayer {
        connection_id: u64,
    },
    SetChannel {
        connection_id: u64,
        channel: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DevAdminPlayer {
    pub connection_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DevAdminContentEntry {
    pub content_id: u32,
    pub authored_id: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DevAdminSnapshot {
    pub players: Vec<DevAdminPlayer>,
    pub npcs: Vec<DevAdminContentEntry>,
    /// Added after the original DEV-admin snapshot contract. Old server binaries
    /// omit this field, so Hub clients must decode that snapshot as an empty item
    /// catalogue rather than treating the whole admin channel as unavailable.
    #[serde(default)]
    pub items: Vec<DevAdminContentEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DevAdminResponse {
    Snapshot { snapshot: DevAdminSnapshot },
    Command { ok: bool, message: String },
}

impl DevAdminResponse {
    #[must_use]
    pub fn command_ok(message: impl Into<String>) -> Self {
        Self::Command {
            ok: true,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn command_err(message: impl Into<String>) -> Self {
        Self::Command {
            ok: false,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip_is_tagged_json() {
        let request = DevAdminRequest::SpawnItem {
            connection_id: 7,
            item_content_id: 10_001,
            quantity: 3,
        };
        let json = serde_json::to_string(&request).expect("encode");
        assert!(json.contains("spawn_item"));
        assert_eq!(
            serde_json::from_str::<DevAdminRequest>(&json).expect("decode"),
            request
        );
    }

    #[test]
    fn narrative_request_roundtrip_is_typed_json() {
        let request = DevAdminRequest::SetNarrativeFact {
            connection_id: 9,
            fact_id: "welcome.workshop.package_delivered".into(),
            value: true,
        };
        let json = serde_json::to_string(&request).expect("encode");
        assert!(json.contains("set_narrative_fact"));
        assert_eq!(
            serde_json::from_str::<DevAdminRequest>(&json).expect("decode"),
            request
        );
    }

    #[test]
    fn snapshot_without_items_decodes_as_empty_catalogue() {
        let json = r#"{"type":"snapshot","snapshot":{"players":[],"npcs":[]}}"#;
        let decoded = serde_json::from_str::<DevAdminResponse>(json).expect("decode old snapshot");
        assert_eq!(
            decoded,
            DevAdminResponse::Snapshot {
                snapshot: DevAdminSnapshot::default()
            }
        );
    }
}
