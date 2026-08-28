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
//! `HeldCancel`, and per-recipient `WorldSnapshot` acknowledgement / contact
//! headers. The server remains authoritative.

mod config;
mod connection;
mod framing;
mod message;
mod snapshot;
mod version;

pub use config::{
    ALPN_PROTOCOL, DEFAULT_DEV_HOST, DEFAULT_DEV_PORT, HANDSHAKE_TIMEOUT, IDLE_TIMEOUT,
    MAX_CONTROL_MESSAGE_BYTES, MAX_DATAGRAM_BYTES, MAX_ENTITIES_PER_SNAPSHOT,
    MAX_GAMEPLAY_SNAPSHOT_BYTES, MAX_LABEL_BYTES, NetworkConfig, PING_INTERVAL, dev_socket_addr,
};
pub use connection::ConnectionId;
pub use framing::{
    FrameError, decode_gameplay_payload, decode_payload, encode_frame, encode_gameplay_frame,
    peek_frame_len, peek_gameplay_frame_len,
};
pub use message::{
    ClientControl, CodecError, DisconnectReason, DisconnectReasonCode, Hello, InputCommand,
    MoveAxis, ServerControl, ServerDatagram, Welcome, decode_client_control,
    decode_client_datagram, decode_server_control, decode_server_datagram, encode_client_control,
    encode_client_datagram, encode_server_control, encode_server_datagram, validate_hello,
};
pub use snapshot::{
    PlatformSupportId, ReplicatedKind, SnapshotEntity, WireEntityId, WorldSnapshot,
    decode_world_snapshot, encode_world_snapshot,
};
pub use version::PROTOCOL_VERSION;

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
        assert_eq!(PROTOCOL_VERSION, 4);
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
