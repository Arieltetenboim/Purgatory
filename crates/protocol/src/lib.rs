//! Shared client/server protocol types, framing, and version semantics.
//!
//! The protocol crate is transport-conscious (length-prefixed control frames,
//! bounded datagrams) but gameplay-neutral. It must not depend on Quinn, Tokio,
//! windowing, or simulation.
//!
//! # Trust boundary
//!
//! **All client network input is untrusted.**
//!
//! The server must never assume that:
//! - client values are valid
//! - client packet ordering is valid
//! - client packet frequency is reasonable
//! - client enum/discriminant values are safe
//! - client strings are a reasonable length
//! - client IDs refer to objects owned by that client
//!
//! Client sends requests / intent. Server owns authoritative state.
//! Packet arrival must not directly modify authoritative game state.
//! Gameplay flow: parse → validate → semantic request → simulation.
//! Phase 5.5 transmits intent-only `InputCommand` (with `input_epoch`),
//! `HeldCancel`. Protocol v9 gameplay replication uses `ReplicationFrame`
//! on the server-initiated uni stream, plus DEV-only `DevSetChannel`.
//! The server remains authoritative.

mod ability;
mod config;
mod connection;
mod dialogue;
mod equipment;
mod frame;
mod framing;
mod intent;
mod interact;
mod inventory;
mod item;
mod message;
mod presentation_oneshot;
mod snapshot;
mod version;

pub use ability::{
    ABILITY_ACCEPTED_BYTES, ABILITY_ACTIVATE_INDEPENDENT_BYTES, ABILITY_ACTIVATE_SELECTED_BYTES,
    ABILITY_REJECTED_BYTES, AbilityActivateRequest, AbilityCommandReject, ServerAbility,
};
pub use config::{
    ALPN_PROTOCOL, DEFAULT_DEV_HOST, DEFAULT_DEV_PORT, HANDSHAKE_TIMEOUT, IDLE_TIMEOUT,
    MAX_CONTROL_MESSAGE_BYTES, MAX_DATAGRAM_BYTES, MAX_ENTITIES_PER_SNAPSHOT,
    MAX_GAMEPLAY_SNAPSHOT_BYTES, MAX_LABEL_BYTES, NetworkConfig, PING_INTERVAL, dev_socket_addr,
};
pub use connection::ConnectionId;
pub use dialogue::{
    DIALOGUE_ACTIVE_LINE_BYTES, DIALOGUE_ADVANCE_BYTES, DialogueAdvance, ServerDialogueLine,
};
pub use equipment::{
    EQUIP_REQUEST_BYTES, EQUIPMENT_ACCEPTED_BYTES, EQUIPMENT_DELTA_EQUIP_BYTES,
    EQUIPMENT_DELTA_UNEQUIP_BYTES, EQUIPMENT_FULL_ALL_OCCUPIED_BYTES, EQUIPMENT_FULL_EMPTY_BYTES,
    EQUIPMENT_FULL_ONE_OCCUPIED_BYTES, EQUIPMENT_REJECTED_BYTES, EQUIPMENT_SLOT_COUNT,
    EquipRequest, EquipmentRejectReason, ReplicatedEquipment, ReplicatedEquipmentDelta,
    ServerEquipment, UNEQUIP_REQUEST_BYTES, UnequipRequest, decode_equipment_delta,
    decode_equipment_full, encode_equipment_delta, encode_equipment_full, slot_valid,
};
pub use frame::{
    DomainMask, ObserverAoiDebug, ReplicatedHealth, ReplicationFrame, ReplicationRecord,
    decode_replication_frame, encode_replication_frame, encode_replication_record,
};
pub use framing::{
    FrameError, decode_gameplay_payload, decode_payload, encode_frame, encode_gameplay_frame,
    peek_frame_len, peek_gameplay_frame_len,
};
pub use intent::{IntentNet, move_axis_from_i8};
pub use interact::{
    DevSetChannel, DevSetJump, DevSetSpeed, InteractClose, InteractCloseReason, InteractOpen,
    InteractRejectReason, PortalActivate, ServerInteract,
};
pub use inventory::{InventoryEntry, ServerInventory};
pub use item::{
    PICKUP_ACCEPTED_BYTES, PICKUP_REJECTED_BYTES, PICKUP_REQUEST_BYTES, PickupRejectReason,
    PickupRequest, ServerItem,
};
pub use message::{
    ClientControl, CodecError, DisconnectReason, DisconnectReasonCode, Hello, InputCommand,
    MoveAxis, ServerControl, ServerDatagram, Welcome, decode_client_control,
    decode_client_datagram, decode_server_control, decode_server_datagram, encode_client_control,
    encode_client_datagram, encode_server_control, encode_server_datagram, validate_hello,
};
pub use presentation_oneshot::{
    DEV_PRESENTATION_ONESHOT_BYTES, DevPresentationOneShot, SERVER_PRESENTATION_ONESHOT_BYTES,
    ServerPresentationOneShot, decode_dev_presentation_oneshot, decode_server_presentation_oneshot,
    encode_dev_presentation_oneshot, encode_server_presentation_oneshot,
};
pub use snapshot::{
    PlatformSupportId, ReplicatedKind, SnapshotEntity, WireEntityId, WorldSnapshot,
    decode_world_snapshot, encode_world_snapshot,
};
pub use version::{DEV_CHANNEL_MAX, HELLO_DEV_LOGIN_SINCE, PROTOCOL_VERSION};

/// Cargo package version for this crate.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_nonempty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn common_is_linked() {
        assert!(!purgatory_common::version().is_empty());
    }

    #[test]
    fn protocol_version_is_defined() {
        assert_eq!(PROTOCOL_VERSION, 25);
    }

    #[test]
    fn crate_manifest_excludes_transport_and_sim() {
        let manifest = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
        for forbidden in ["tokio", "quinn", "winit", "wgpu", "egui", "serde"] {
            let present = manifest.lines().any(|line| {
                let trimmed = line.trim();
                trimmed.starts_with(forbidden) && (trimmed.contains('=') || trimmed.contains('{'))
            });
            assert!(
                !present,
                "{forbidden} must not appear as a purgatory-protocol dependency"
            );
        }
    }
}
