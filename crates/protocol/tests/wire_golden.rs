//! Protocol v1 wire compatibility vectors.
//!
//! Changing these bytes requires intentional protocol review.
//!
//! Roundtrip tests (`decode(encode(msg)) == msg`) cannot catch an encoder and
//! decoder drifting together. These vectors freeze the exact bytes Protocol v1
//! puts on the wire, in both directions:
//!
//! - `expected_message` encodes to exactly the frozen bytes
//! - the frozen bytes decode to exactly `expected_message`
//!
//! The fixtures are plain byte arrays on purpose: no snapshot files, no
//! auto-update path. A failure here means the wire format moved, so review
//! compatibility, update `PROTOCOL_VERSION` if the change is incompatible, and
//! only then update the vector deliberately.
//!
//! Every multi-byte integer is little-endian. Strings are a `u8` byte length
//! followed by that many UTF-8 bytes. Field order is the declaration order.

use purgatory_protocol::{
    ClientControl, ConnectionId, DisconnectReason, DisconnectReasonCode, Hello, InputCommand,
    InteractClose, InteractCloseReason, InteractOpen, InteractRejectReason,
    MAX_CONTROL_MESSAGE_BYTES, MAX_GAMEPLAY_SNAPSHOT_BYTES, MoveAxis, PROTOCOL_VERSION,
    PlatformSupportId, PortalActivate, ReplicatedKind, ReplicationFrame, ReplicationRecord,
    ServerControl, ServerDatagram, ServerInteract, SnapshotEntity, WireEntityId, WorldSnapshot,
    decode_client_control, decode_client_datagram, decode_gameplay_payload, decode_payload,
    decode_replication_frame, decode_server_control, decode_server_datagram, decode_world_snapshot,
    encode_client_control, encode_client_datagram, encode_frame, encode_gameplay_frame,
    encode_replication_frame, encode_server_control, encode_server_datagram, encode_world_snapshot,
    peek_frame_len, peek_gameplay_frame_len,
};

/// Protocol v1 wire compatibility vector: `ClientControl::Hello`.
/// Changing these bytes requires intentional protocol review.
const HELLO_TEST_BUILD: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x01, 0x00, 0x00, 0x00, // protocol_version = 1 (u32 little-endian)
    0x04, // client_build byte length = 4 (u8)
    0x74, 0x65, 0x73, 0x74, // "test" (UTF-8)
];

/// Protocol v1 wire compatibility vector: `Hello` with an empty string field.
/// Changing these bytes requires intentional protocol review.
const HELLO_EMPTY_BUILD: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x01, 0x00, 0x00, 0x00, // protocol_version = 1 (u32 little-endian)
    0x00, // client_build byte length = 0, no string bytes follow
];

/// Protocol v1 wire compatibility vector: framed `Hello`.
/// Changing these bytes requires intentional protocol review.
const HELLO_TEST_BUILD_FRAMED: &[u8] = &[
    0x0A, 0x00, 0x00, 0x00, // frame payload length = 10 (u32 little-endian)
    0x01, // ClientControl::Hello discriminant
    0x01, 0x00, 0x00, 0x00, // protocol_version = 1 (u32 little-endian)
    0x04, // client_build byte length = 4 (u8)
    0x74, 0x65, 0x73, 0x74, // "test" (UTF-8)
];

/// Protocol v1 wire compatibility vector: `ServerControl::Welcome`.
/// Changing these bytes requires intentional protocol review.
const WELCOME_DEV: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x01, 0x00, 0x00, 0x00, // protocol_version = 1 (u32 little-endian)
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234 (u64 LE)
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30 (u32 little-endian)
    0x14, // server_label byte length = 20 (u8)
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

/// Protocol v1 wire compatibility vector: framed `Welcome`.
/// Changing these bytes requires intentional protocol review.
const WELCOME_DEV_FRAMED: &[u8] = &[
    0x26, 0x00, 0x00, 0x00, // frame payload length = 38 (u32 little-endian)
    0x02, // ServerControl::Welcome discriminant
    0x01, 0x00, 0x00, 0x00, // protocol_version = 1 (u32 little-endian)
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234 (u64 LE)
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30 (u32 little-endian)
    0x14, // server_label byte length = 20 (u8)
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

/// Protocol v1 wire compatibility vector: `ServerControl::Disconnect`.
/// Changing these bytes requires intentional protocol review.
const DISCONNECT_VERSION_MISMATCH: &[u8] = &[
    0x03, // ServerControl::Disconnect discriminant
    0x01, // DisconnectReasonCode::VersionMismatch
    0x0C, // detail byte length = 12 (u8)
    0x67, 0x6F, 0x74, 0x20, // "got "
    0x32, 0x20, // "2 "
    0x77, 0x61, 0x6E, 0x74, 0x20, // "want "
    0x31, // "1"
];

/// Protocol v1 wire compatibility vector: client Ping datagram.
/// Changing these bytes requires intentional protocol review.
const PING_DATAGRAM: &[u8] = &[
    0x04, // client Ping datagram discriminant
    0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, // nonce = 0x0102030405060708 (u64 LE)
];

/// Protocol v1 wire compatibility vector: server Pong datagram.
/// Changing these bytes requires intentional protocol review.
const PONG_DATAGRAM: &[u8] = &[
    0x05, // server Pong datagram discriminant
    0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, // nonce = 0x0102030405060708 (u64 LE)
];

/// Protocol v2 wire compatibility vector: `Hello` with current protocol_version.
/// Changing these bytes requires intentional protocol review.
const HELLO_V2: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x02, 0x00, 0x00, 0x00, // protocol_version = 2 (u32 little-endian)
    0x04, // client_build byte length = 4 (u8)
    0x74, 0x65, 0x73, 0x74, // "test" (UTF-8)
];

/// Protocol v2 wire compatibility vector: `Welcome` with current protocol_version.
/// Changing these bytes requires intentional protocol review.
const WELCOME_V2: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x02, 0x00, 0x00, 0x00, // protocol_version = 2 (u32 little-endian)
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234 (u64 LE)
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30 (u32 little-endian)
    0x14, // server_label byte length = 20 (u8)
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

/// Protocol v3 wire compatibility vector: `Hello` with current protocol_version.
/// Changing these bytes requires intentional protocol review.
const HELLO_V3: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x03, 0x00, 0x00, 0x00, // protocol_version = 3 (u32 little-endian)
    0x04, // client_build byte length = 4 (u8)
    0x74, 0x65, 0x73, 0x74, // "test" (UTF-8)
];

/// Protocol v3 wire compatibility vector: `Welcome` with current protocol_version.
/// Changing these bytes requires intentional protocol review.
const WELCOME_V3: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x03, 0x00, 0x00, 0x00, // protocol_version = 3 (u32 little-endian)
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234 (u64 LE)
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30 (u32 little-endian)
    0x14, // server_label byte length = 20 (u8)
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

/// Protocol v3 wire compatibility vector: `WorldSnapshot` with one player.
/// Changing these bytes requires intentional protocol review.
const WORLD_SNAPSHOT_ONE_PLAYER: &[u8] = &[
    0x07, // WorldSnapshot discriminant
    0x04, 0x03, 0x02, 0x01, // snapshot_sequence = 0x01020304 (u32 LE)
    0x0A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // server_tick = 10 (u64 LE)
    0x05, 0x00, 0x00, 0x00, // local_player_entity.index = 5
    0x01, 0x00, 0x00, 0x00, // local_player_entity.generation = 1
    0x01, 0x00, // entity_count = 1 (u16 LE)
    0x05, 0x00, 0x00, 0x00, // entity.index = 5
    0x01, 0x00, 0x00, 0x00, // entity.generation = 1
    0x01, // ReplicatedKind::Player
    0x00, 0x00, 0x80, 0x3F, // position.x = 1.0
    0x00, 0x00, 0x00, 0x40, // position.y = 2.0
    0x00, 0x00, 0x40, 0x40, // velocity.x = 3.0
    0x00, 0x00, 0x80, 0x40, // velocity.y = 4.0
];

/// Protocol v3 wire compatibility vector: framed `WorldSnapshot`.
/// Changing these bytes requires intentional protocol review.
const WORLD_SNAPSHOT_ONE_PLAYER_FRAMED: &[u8] = &[
    0x30, 0x00, 0x00, 0x00, // frame payload length = 48 (u32 little-endian)
    0x07, // WorldSnapshot discriminant
    0x04, 0x03, 0x02, 0x01, // snapshot_sequence = 0x01020304 (u32 LE)
    0x0A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // server_tick = 10 (u64 LE)
    0x05, 0x00, 0x00, 0x00, // local_player_entity.index = 5
    0x01, 0x00, 0x00, 0x00, // local_player_entity.generation = 1
    0x01, 0x00, // entity_count = 1 (u16 LE)
    0x05, 0x00, 0x00, 0x00, // entity.index = 5
    0x01, 0x00, 0x00, 0x00, // entity.generation = 1
    0x01, // ReplicatedKind::Player
    0x00, 0x00, 0x80, 0x3F, // position.x = 1.0
    0x00, 0x00, 0x00, 0x40, // position.y = 2.0
    0x00, 0x00, 0x40, 0x40, // velocity.x = 3.0
    0x00, 0x00, 0x80, 0x40, // velocity.y = 4.0
];

/// Protocol v2 wire compatibility vector: `ClientControl::Input`.
/// Changing these bytes requires intentional protocol review.
const INPUT_RIGHT_JUMP: &[u8] = &[
    0x06, // ClientControl::Input discriminant
    0x04, 0x03, 0x02, 0x01, // sequence = 0x01020304 (u32 little-endian)
    0x02, // MoveAxis::Right
    0x01, // jump_pressed = true
    0x00, // down_held = false
];

/// Protocol v2 wire compatibility vector: framed `InputCommand`.
/// Changing these bytes requires intentional protocol review.
const INPUT_RIGHT_JUMP_FRAMED: &[u8] = &[
    0x08, 0x00, 0x00, 0x00, // frame payload length = 8 (u32 little-endian)
    0x06, // ClientControl::Input discriminant
    0x04, 0x03, 0x02, 0x01, // sequence = 0x01020304 (u32 little-endian)
    0x02, // MoveAxis::Right
    0x01, // jump_pressed = true
    0x00, // down_held = false
];

/// Protocol v4 wire compatibility vector: `Hello` with current protocol_version.
/// Changing these bytes requires intentional protocol review.
const HELLO_V4: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x04, 0x00, 0x00, 0x00, // protocol_version = 4 (u32 little-endian)
    0x04, // client_build byte length = 4 (u8)
    0x74, 0x65, 0x73, 0x74, // "test" (UTF-8)
];

/// Protocol v4 wire compatibility vector: `Welcome` with current protocol_version.
/// Changing these bytes requires intentional protocol review.
const WELCOME_V4: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x04, 0x00, 0x00, 0x00, // protocol_version = 4 (u32 little-endian)
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234 (u64 LE)
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30 (u32 little-endian)
    0x14, // server_label byte length = 20 (u8)
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

/// Protocol v4 `InputCommand` (epoch + sequence).
const INPUT_RIGHT_JUMP_V4: &[u8] = &[
    0x06, // ClientControl::Input
    0x04, 0x03, 0x02, 0x01, // sequence
    0x00, 0x00, // input_epoch = 0
    0x02, // MoveAxis::Right
    0x01, // jump_pressed
    0x00, // down_held
];

const INPUT_RIGHT_JUMP_V4_FRAMED: &[u8] = &[
    0x0A, 0x00, 0x00, 0x00, // payload length = 10
    0x06, 0x04, 0x03, 0x02, 0x01, 0x00, 0x00, 0x02, 0x01, 0x00,
];

const HELD_CANCEL: &[u8] = &[0x08];

/// Protocol v5 wire compatibility vector: `Hello`.
const HELLO_V5: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x05, 0x00, 0x00, 0x00, // protocol_version = 5
    0x04, // client_build byte length = 4
    0x74, 0x65, 0x73, 0x74, // "test"
];

const WELCOME_V5: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x05, 0x00, 0x00, 0x00, // protocol_version = 5
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30
    0x14, // server_label byte length = 20
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

const INTERACT_OPEN_V5: &[u8] = &[
    0x09, // InteractOpen
    0x01, 0x00, 0x00, 0x00, // index = 1
    0x01, 0x00, 0x00, 0x00, // generation = 1
];

const INTERACT_CLOSE_V5: &[u8] = &[
    0x0A, // InteractClose
    0x03, 0x00, 0x00, 0x00, // session_id = 3
];

const INTERACT_OPENED_V5: &[u8] = &[
    0x0B, // Opened
    0x03, 0x00, 0x00, 0x00, // session_id = 3
    0x01, 0x00, 0x00, 0x00, // index = 1
    0x01, 0x00, 0x00, 0x00, // generation = 1
];

const INTERACT_REJECTED_V5: &[u8] = &[
    0x0C, // Rejected
    0x02, 0x00, 0x00, 0x00, // index = 2
    0x03, 0x00, 0x00, 0x00, // generation = 3
    0x04, // OutOfRange
];

const INTERACT_UPDATED_V5: &[u8] = &[
    0x0D, // Updated
    0x03, 0x00, 0x00, 0x00, // session_id = 3
    0x01, 0x00, 0x00, 0x00, // index = 1
    0x01, 0x00, 0x00, 0x00, // generation = 1
];

const INTERACT_CLOSED_V5: &[u8] = &[
    0x0E, // Closed
    0x03, 0x00, 0x00, 0x00, // session_id = 3
    0x01, // Requested
];

/// Protocol v6 wire compatibility vector: `Hello`.
const HELLO_V6: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x06, 0x00, 0x00, 0x00, // protocol_version = 6
    0x04, // client_build byte length = 4
    0x74, 0x65, 0x73, 0x74, // "test"
];

const WELCOME_V6: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x06, 0x00, 0x00, 0x00, // protocol_version = 6
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30
    0x14, // server_label byte length = 20
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

/// Protocol v7 wire compatibility vector: `Hello`.
const HELLO_V7: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x07, 0x00, 0x00, 0x00, // protocol_version = 7
    0x04, // client_build byte length = 4
    0x74, 0x65, 0x73, 0x74, // "test"
];

const WELCOME_V7: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x07, 0x00, 0x00, 0x00, // protocol_version = 7
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30
    0x14, // server_label byte length = 20
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

const HELLO_V8: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x08, 0x00, 0x00, 0x00, // protocol_version = 8
    0x04, // client_build byte length = 4
    0x74, 0x65, 0x73, 0x74, // "test"
];

const WELCOME_V8: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x08, 0x00, 0x00, 0x00, // protocol_version = 8
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30
    0x14, // server_label byte length = 20
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

const HELLO_V9: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x09, 0x00, 0x00, 0x00, // protocol_version = 9
    0x04, // client_build byte length = 4
    0x74, 0x65, 0x73, 0x74, // "test"
];

const HELLO_V10: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x0A, 0x00, 0x00, 0x00, // protocol_version = 10
    0x04, // client_build byte length = 4
    0x74, 0x65, 0x73, 0x74, // "test"
    0x09, // dev_login byte length = 9
    0x64, 0x65, 0x76, 0x2E, 0x6C, 0x6F, 0x63, 0x61, 0x6C, // "dev.local"
];

const WELCOME_V9: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x09, 0x00, 0x00, 0x00, // protocol_version = 9
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30
    0x14, // server_label byte length = 20
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

const WELCOME_V10: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x0A, 0x00, 0x00, 0x00, // protocol_version = 10
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30
    0x14, // server_label byte length = 20
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

const HELLO_V11: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x0B, 0x00, 0x00, 0x00, // protocol_version = 11
    0x04, // client_build byte length = 4
    0x74, 0x65, 0x73, 0x74, // "test"
    0x09, // dev_login byte length = 9
    0x64, 0x65, 0x76, 0x2E, 0x6C, 0x6F, 0x63, 0x61, 0x6C, // "dev.local"
];

const WELCOME_V11: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x0B, 0x00, 0x00, 0x00, // protocol_version = 11
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30
    0x14, // server_label byte length = 20
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

const HELLO_V12: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x0C, 0x00, 0x00, 0x00, // protocol_version = 12
    0x04, // client_build byte length = 4
    0x74, 0x65, 0x73, 0x74, // "test"
    0x09, // dev_login byte length = 9
    0x64, 0x65, 0x76, 0x2E, 0x6C, 0x6F, 0x63, 0x61, 0x6C, // "dev.local"
];

const WELCOME_V12: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x0C, 0x00, 0x00, 0x00, // protocol_version = 12
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30
    0x14, // server_label byte length = 20
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

const HELLO_V13: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x0D, 0x00, 0x00, 0x00, // protocol_version = 13
    0x04, // client_build byte length = 4
    0x74, 0x65, 0x73, 0x74, // "test"
    0x09, // dev_login byte length = 9
    0x64, 0x65, 0x76, 0x2E, 0x6C, 0x6F, 0x63, 0x61, 0x6C, // "dev.local"
];

const WELCOME_V13: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x0D, 0x00, 0x00, 0x00, // protocol_version = 13
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30
    0x14, // server_label byte length = 20
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

const HELLO_V14: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x0E, 0x00, 0x00, 0x00, // protocol_version = 14
    0x04, // client_build byte length = 4
    0x74, 0x65, 0x73, 0x74, // "test"
    0x09, // dev_login byte length = 9
    0x64, 0x65, 0x76, 0x2E, 0x6C, 0x6F, 0x63, 0x61, 0x6C, // "dev.local"
];

const WELCOME_V14: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x0E, 0x00, 0x00, 0x00, // protocol_version = 14
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30
    0x14, // server_label byte length = 20
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

const DEV_RESET_PLAYER_V14: &[u8] = &[
    0x18, // tag 24 DevResetPlayer
];

const HELLO_V15: &[u8] = &[
    0x01, // ClientControl::Hello discriminant
    0x0F, 0x00, 0x00, 0x00, // protocol_version = 15
    0x04, // client_build byte length = 4
    0x74, 0x65, 0x73, 0x74, // "test"
    0x09, // dev_login byte length = 9
    0x64, 0x65, 0x76, 0x2E, 0x6C, 0x6F, 0x63, 0x61, 0x6C, // "dev.local"
];

const WELCOME_V15: &[u8] = &[
    0x02, // ServerControl::Welcome discriminant
    0x0F, 0x00, 0x00, 0x00, // protocol_version = 15
    0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // connection_id = 0x1234
    0x1E, 0x00, 0x00, 0x00, // server_tick_rate = 30
    0x14, // server_label byte length = 20
    0x70, 0x75, 0x72, 0x67, 0x61, 0x74, 0x6F, 0x72, 0x79, 0x2D, // "purgatory-"
    0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x2D, // "server-"
    0x64, 0x65, 0x76, // "dev"
];

const ABILITY_ACTIVATE_INDEPENDENT_V15: &[u8] = &[
    0x19, // tag 25 AbilityActivate
    0x01, 0x00, 0x00, 0x00, // seq = 1
    0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // content token 9
    0x00, // no selected entity
];

const ABILITY_ACCEPTED_V15: &[u8] = &[
    0x1A, // tag 26 Ability accepted
    0x01, 0x00, 0x00, 0x00, // seq = 1
];

const DEV_PRESENTATION_ONESHOT_ATTACK_V13: &[u8] = &[
    0x16, // tag 22 DevPresentationOneShot
    0x01, // Attack
];

const SERVER_PRESENTATION_ONESHOT_HURT_V13: &[u8] = &[
    0x17, // tag 23 ServerPresentationOneShot
    0x03, 0x00, 0x00, 0x00, // index = 3
    0x02, 0x00, 0x00, 0x00, // generation = 2
    0x02, // Hurt
    0x64, 0x00, 0x00, 0x00, // until_tick = 100
];

const DEV_SET_CHANNEL_V9: &[u8] = &[
    0x11, // ClientControl::DevSetChannel discriminant (tag 17)
    0x01, 0x00, 0x00, 0x00, // channel = 1
];

const PORTAL_ACTIVATE_V7: &[u8] = &[
    0x0F, // ClientControl::PortalActivate
    0x01, 0x00, 0x00, 0x00, // index = 1
    0x01, 0x00, 0x00, 0x00, // generation = 1
];

/// Protocol v4 `WorldSnapshot` with one player and local contact header.
const WORLD_SNAPSHOT_V4_ONE_PLAYER: &[u8] = &[
    0x07, // WorldSnapshot
    0x04, 0x03, 0x02, 0x01, // snapshot_sequence
    0x0A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // server_tick = 10
    0x05, 0x00, 0x00, 0x00, // local index
    0x01, 0x00, 0x00, 0x00, // local generation
    0x00, 0x00, // input_epoch
    0x00, 0x00, 0x00, 0x00, // last_acknowledged_input_sequence
    0x01, // local_grounded
    0x01, 0x00, // local_grounded_on = 1
    0x00, 0x00, // local_ignored_platform
    0x00, 0x00, // continuation_debt
    0x01, 0x00, // entity_count
    0x05, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // entity id + kind
    0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x80, 0x40,
];

const WORLD_SNAPSHOT_V4_ONE_PLAYER_FRAMED: &[u8] = &[
    0x3D, 0x00, 0x00, 0x00, // payload length = 61
    0x07, 0x04, 0x03, 0x02, 0x01, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x01, 0x00, 0x05, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x80,
    0x3F, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x80, 0x40,
];

/// Protocol v6 `WorldSnapshot`: v4 contact header plus observer WorldAddress.
const WORLD_SNAPSHOT_V6_ONE_PLAYER: &[u8] = &[
    0x07, // WorldSnapshot
    0x04, 0x03, 0x02, 0x01, // snapshot_sequence
    0x0A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // server_tick = 10
    0x05, 0x00, 0x00, 0x00, // local index
    0x01, 0x00, 0x00, 0x00, // local generation
    0x00, 0x00, // input_epoch
    0x00, 0x00, 0x00, 0x00, // last_acknowledged_input_sequence
    0x01, // local_grounded
    0x01, 0x00, // local_grounded_on = 1
    0x00, 0x00, // local_ignored_platform
    0x00, 0x00, // continuation_debt
    0x01, 0x00, 0x00, 0x00, // local_map = 1
    0x00, 0x00, 0x00, 0x00, // local_channel
    0x00, 0x00, 0x00, 0x00, // local_instance
    0x01, 0x00, // entity_count
    0x05, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // entity id + kind
    0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x80, 0x40,
];

const WORLD_SNAPSHOT_V6_ONE_PLAYER_FRAMED: &[u8] = &[
    0x49, 0x00, 0x00, 0x00, // payload length = 73
    0x07, 0x04, 0x03, 0x02, 0x01, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x05, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x00,
    0x40, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x80, 0x40,
];

/// Nonce shared by the datagram vectors. Each byte differs, so a native-endian
/// or byte-swapped encoder cannot pass by accident.
const GOLDEN_NONCE: u64 = 0x0102_0304_0506_0708;

/// Connection id in the Welcome vectors. `0x1234` reads as `34 12` on the wire.
const GOLDEN_CONNECTION_ID: u64 = 0x1234;

const GOLDEN_SEQUENCE: u32 = 0x0102_0304;

/// Protocol v25 wire compatibility vector: advance the active dialogue.
const DIALOGUE_ADVANCE_V25: &[u8] = &[
    0x23, // DialogueAdvance
    0x07, 0x00, 0x00, 0x00, // session_id = 7
];

/// Protocol v25 wire compatibility vector: authoritative active line identity.
const DIALOGUE_LINE_V25: &[u8] = &[
    0x24, // DialogueLine
    0x07, 0x00, 0x00, 0x00, // session_id = 7
    0x2A, 0x00, 0x00, 0x00, // target.index = 42
    0x03, 0x00, 0x00, 0x00, // target.generation = 3
    0x21, 0x4E, 0x00, 0x00, // npc_content_id = 20001
    0x02, 0x00, 0x00, 0x00, // beat_index = 2
    0x01, 0x00, 0x00, 0x00, // line_index = 1
];

fn hello_test_build() -> Hello {
    Hello {
        protocol_version: 1,
        client_build: "test".to_string(),
        dev_login: String::new(),
    }
}

fn welcome_dev() -> purgatory_protocol::Welcome {
    purgatory_protocol::Welcome {
        protocol_version: 1,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    }
}

fn disconnect_version_mismatch() -> DisconnectReason {
    DisconnectReason {
        code: DisconnectReasonCode::VersionMismatch,
        detail: "got 2 want 1".to_string(),
    }
}

fn input_right_jump() -> InputCommand {
    InputCommand {
        input_epoch: 0,
        sequence: GOLDEN_SEQUENCE,
        move_axis: MoveAxis::Right,
        jump_pressed: true,
        down_held: false,
        portal_held: false,
    }
}

/// v1 and v2 vectors remain frozen after the v3 bump. Do not rewrite them in place.
#[test]
fn v1_golden_vectors_remain_frozen() {
    assert_eq!(HELLO_TEST_BUILD[1], 0x01, "do not rewrite v1 Hello bytes");
    assert_eq!(WELCOME_DEV[1], 0x01, "do not rewrite v1 Welcome bytes");
}

#[test]
fn v2_golden_vectors_remain_frozen() {
    assert_eq!(HELLO_V2[1], 0x02, "do not rewrite v2 Hello bytes");
    assert_eq!(WELCOME_V2[1], 0x02, "do not rewrite v2 Welcome bytes");
}

#[test]
fn v3_golden_vectors_remain_frozen() {
    assert_eq!(HELLO_V3[1], 0x03, "do not rewrite v3 Hello bytes");
    assert_eq!(WELCOME_V3[1], 0x03, "do not rewrite v3 Welcome bytes");
    assert_eq!(WORLD_SNAPSHOT_ONE_PLAYER.len(), 48);
    assert_eq!(WORLD_SNAPSHOT_ONE_PLAYER_FRAMED.len(), 52);
}

#[test]
fn current_protocol_version_is_25() {
    assert_eq!(PROTOCOL_VERSION, 25);
}

#[test]
fn dialogue_v25_matches_golden_bytes() {
    let advance =
        ClientControl::DialogueAdvance(purgatory_protocol::DialogueAdvance { session_id: 7 });
    assert_eq!(
        encode_client_control(&advance).expect("encode"),
        DIALOGUE_ADVANCE_V25
    );
    assert_eq!(
        decode_client_control(DIALOGUE_ADVANCE_V25).expect("decode"),
        advance
    );

    let line = ServerControl::DialogueLine(purgatory_protocol::ServerDialogueLine {
        session_id: 7,
        target: WireEntityId {
            index: 42,
            generation: 3,
        },
        npc_content_id: purgatory_common::ContentId::from_raw(20_001),
        beat_index: 2,
        line_index: 1,
    });
    assert_eq!(
        encode_server_control(&line).expect("encode"),
        DIALOGUE_LINE_V25
    );
    assert_eq!(
        decode_server_control(DIALOGUE_LINE_V25).expect("decode"),
        line
    );
}

#[test]
fn hello_encodes_to_golden_bytes() {
    let encoded = encode_client_control(&ClientControl::Hello(hello_test_build())).expect("encode");
    assert_eq!(encoded, HELLO_TEST_BUILD);
}

#[test]
fn golden_hello_bytes_decode_to_expected_message() {
    assert_eq!(
        decode_client_control(HELLO_TEST_BUILD).expect("decode"),
        ClientControl::Hello(hello_test_build())
    );
}

#[test]
fn hello_with_empty_string_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 1,
        client_build: String::new(),
        dev_login: String::new(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_EMPTY_BUILD);
    assert_eq!(
        decode_client_control(HELLO_EMPTY_BUILD).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_encodes_to_golden_bytes() {
    let encoded = encode_server_control(&ServerControl::Welcome(welcome_dev())).expect("encode");
    assert_eq!(encoded, WELCOME_DEV);
}

#[test]
fn golden_welcome_bytes_decode_to_expected_message() {
    assert_eq!(
        decode_server_control(WELCOME_DEV).expect("decode"),
        ServerControl::Welcome(welcome_dev())
    );
}

#[test]
fn disconnect_reason_encodes_to_golden_bytes() {
    let encoded = encode_server_control(&ServerControl::Disconnect(disconnect_version_mismatch()))
        .expect("encode");
    assert_eq!(encoded, DISCONNECT_VERSION_MISMATCH);
}

#[test]
fn golden_disconnect_bytes_decode_to_expected_message() {
    assert_eq!(
        decode_server_control(DISCONNECT_VERSION_MISMATCH).expect("decode"),
        ServerControl::Disconnect(disconnect_version_mismatch())
    );
}

/// The reason code is a wire discriminant, so its numeric value is frozen too.
#[test]
fn disconnect_reason_codes_keep_their_wire_discriminants() {
    for (code, byte) in [
        (DisconnectReasonCode::VersionMismatch, 1u8),
        (DisconnectReasonCode::Malformed, 2),
        (DisconnectReasonCode::HandshakeTimeout, 3),
        (DisconnectReasonCode::UnexpectedMessage, 4),
        (DisconnectReasonCode::ServerShutdown, 5),
        (DisconnectReasonCode::AlreadyConnected, 6),
    ] {
        assert_eq!(code.as_u8(), byte, "{code:?} discriminant moved");
        assert_eq!(DisconnectReasonCode::from_u8(byte), Some(code));
    }
    assert_eq!(DisconnectReasonCode::from_u8(0), None);
    assert_eq!(DisconnectReasonCode::from_u8(7), None);
}

#[test]
fn ping_datagram_matches_golden_bytes() {
    let encoded = encode_client_datagram(GOLDEN_NONCE).expect("encode");
    assert_eq!(encoded, PING_DATAGRAM);
    assert_eq!(
        decode_client_datagram(PING_DATAGRAM).expect("decode"),
        GOLDEN_NONCE
    );
}

#[test]
fn pong_datagram_matches_golden_bytes() {
    let encoded = encode_server_datagram(ServerDatagram::Pong {
        nonce: GOLDEN_NONCE,
    })
    .expect("encode");
    assert_eq!(encoded, PONG_DATAGRAM);
    assert_eq!(
        decode_server_datagram(PONG_DATAGRAM).expect("decode"),
        ServerDatagram::Pong {
            nonce: GOLDEN_NONCE
        }
    );
}

/// Datagram vectors are the raw application datagram payload: no length prefix,
/// no control-stream framing.
#[test]
fn datagram_vectors_carry_no_framing() {
    assert_eq!(PING_DATAGRAM.len(), 1 + 8);
    assert_eq!(PONG_DATAGRAM.len(), 1 + 8);
    assert_ne!(
        PING_DATAGRAM[0], PONG_DATAGRAM[0],
        "ping and pong must stay distinguishable by tag alone"
    );
}

/// Locks the codec layout and the framing layout together: 4-byte little-endian
/// payload length followed by exactly that many payload bytes.
#[test]
fn framed_hello_matches_golden_bytes() {
    let payload = encode_client_control(&ClientControl::Hello(hello_test_build())).expect("encode");
    let frame = encode_frame(&payload).expect("frame");
    assert_eq!(frame, HELLO_TEST_BUILD_FRAMED);

    let prefix: [u8; 4] = HELLO_TEST_BUILD_FRAMED[..4].try_into().expect("prefix");
    assert_eq!(peek_frame_len(&prefix).expect("peek"), 10);
    let (decoded_payload, rest) = decode_payload(HELLO_TEST_BUILD_FRAMED).expect("decode frame");
    assert!(rest.is_empty());
    assert_eq!(decoded_payload, HELLO_TEST_BUILD);
    assert_eq!(
        decode_client_control(decoded_payload).expect("decode"),
        ClientControl::Hello(hello_test_build())
    );
}

#[test]
fn framed_welcome_matches_golden_bytes() {
    let payload = encode_server_control(&ServerControl::Welcome(welcome_dev())).expect("encode");
    let frame = encode_frame(&payload).expect("frame");
    assert_eq!(frame, WELCOME_DEV_FRAMED);

    let prefix: [u8; 4] = WELCOME_DEV_FRAMED[..4].try_into().expect("prefix");
    assert_eq!(peek_frame_len(&prefix).expect("peek"), 38);
    let (decoded_payload, rest) = decode_payload(WELCOME_DEV_FRAMED).expect("decode frame");
    assert!(rest.is_empty());
    assert_eq!(decoded_payload, WELCOME_DEV);
    assert_eq!(
        decode_server_control(decoded_payload).expect("decode"),
        ServerControl::Welcome(welcome_dev())
    );
}

/// Every multi-byte field is little-endian on the wire. The frozen values were
/// chosen so that a big-endian or byte-swapped encoder yields different bytes.
#[test]
fn multi_byte_fields_are_little_endian() {
    // Frame length: 10 encodes as `0A 00 00 00`, never `00 00 00 0A`.
    assert_eq!(&HELLO_TEST_BUILD_FRAMED[..4], &10u32.to_le_bytes());
    assert_ne!(&HELLO_TEST_BUILD_FRAMED[..4], &10u32.to_be_bytes());

    // protocol_version follows the discriminant.
    assert_eq!(&HELLO_TEST_BUILD[1..5], &1u32.to_le_bytes());

    // connection_id: `0x1234` reads low byte first.
    let id_bytes = &WELCOME_DEV[5..13];
    assert_eq!(id_bytes, &GOLDEN_CONNECTION_ID.to_le_bytes());
    assert_ne!(id_bytes, &GOLDEN_CONNECTION_ID.to_be_bytes());
    assert_eq!(id_bytes[0], 0x34);

    // server_tick_rate sits between the id and the label length.
    assert_eq!(&WELCOME_DEV[13..17], &30u32.to_le_bytes());

    // Datagram nonce: distinct bytes in reverse order of the literal.
    assert_eq!(&PING_DATAGRAM[1..], &GOLDEN_NONCE.to_le_bytes());
    assert_ne!(&PING_DATAGRAM[1..], &GOLDEN_NONCE.to_be_bytes());
    assert_eq!(&PONG_DATAGRAM[1..], &GOLDEN_NONCE.to_le_bytes());
}

/// Strings are a `u8` byte length plus raw UTF-8, and the length counts bytes.
#[test]
fn strings_encode_as_byte_length_then_utf8() {
    assert_eq!(HELLO_TEST_BUILD[5], 4);
    assert_eq!(&HELLO_TEST_BUILD[6..], b"test");

    assert_eq!(WELCOME_DEV[17], 20);
    assert_eq!(&WELCOME_DEV[18..], b"purgatory-server-dev");

    assert_eq!(DISCONNECT_VERSION_MISMATCH[2], 12);
    assert_eq!(&DISCONNECT_VERSION_MISMATCH[3..], b"got 2 want 1");

    // A multi-byte character is counted in bytes, not characters.
    let hello = Hello {
        protocol_version: 1,
        client_build: "dév".to_string(),
        dev_login: String::new(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded[5], 4, "byte length, not character count");
    assert_eq!(&encoded[6..], "dév".as_bytes());
    assert_eq!(
        decode_client_control(&encoded).expect("decode"),
        ClientControl::Hello(hello)
    );
}

/// Field order is frozen: no reordering of discriminant, integers, and strings.
#[test]
fn field_order_is_frozen() {
    assert_eq!(HELLO_TEST_BUILD.len(), 1 + 4 + 1 + 4);
    assert_eq!(WELCOME_DEV.len(), 1 + 4 + 8 + 4 + 1 + 20);
    assert_eq!(DISCONNECT_VERSION_MISMATCH.len(), 1 + 1 + 1 + 12);
    assert_eq!(HELLO_TEST_BUILD_FRAMED.len(), 4 + HELLO_TEST_BUILD.len());
    assert_eq!(WELCOME_DEV_FRAMED.len(), 4 + WELCOME_DEV.len());
    assert_eq!(INPUT_RIGHT_JUMP.len(), 1 + 4 + 1 + 1 + 1);
    assert_eq!(INPUT_RIGHT_JUMP_FRAMED.len(), 4 + INPUT_RIGHT_JUMP.len());
}

/// Golden vectors are additive: they never relax the parser bounds that reject
/// untrusted input.
#[test]
fn golden_vectors_do_not_weaken_parser_bounds() {
    // A vector for one direction is not accepted by the other direction.
    assert!(decode_client_control(WELCOME_DEV).is_err());
    assert!(decode_client_control(DISCONNECT_VERSION_MISMATCH).is_err());
    assert!(decode_server_control(HELLO_TEST_BUILD).is_err());
    // Control payloads are not datagrams and vice versa.
    assert!(decode_client_datagram(HELLO_TEST_BUILD).is_err());
    assert!(decode_server_datagram(WELCOME_DEV).is_err());
    assert!(decode_client_datagram(PONG_DATAGRAM).is_err());
    assert!(decode_server_datagram(PING_DATAGRAM).is_err());
    // Trailing bytes are rejected, so a vector cannot be silently extended.
    let mut extended = HELLO_TEST_BUILD.to_vec();
    extended.push(0);
    assert!(decode_client_control(&extended).is_err());
    // Dropping the last byte of a frozen vector is a decode failure, not a
    // shorter-but-valid message.
    for vector in [
        HELLO_TEST_BUILD,
        WELCOME_DEV,
        DISCONNECT_VERSION_MISMATCH,
        PING_DATAGRAM,
        PONG_DATAGRAM,
        INPUT_RIGHT_JUMP,
        HELLO_V2,
        WELCOME_V2,
        HELLO_V3,
        WELCOME_V3,
        WORLD_SNAPSHOT_ONE_PLAYER,
        HELLO_V4,
        WELCOME_V4,
        INPUT_RIGHT_JUMP_V4,
        WORLD_SNAPSHOT_V4_ONE_PLAYER,
        HELD_CANCEL,
        HELLO_V5,
        WELCOME_V5,
        INTERACT_OPEN_V5,
        INTERACT_CLOSE_V5,
        INTERACT_OPENED_V5,
        INTERACT_REJECTED_V5,
        INTERACT_UPDATED_V5,
        INTERACT_CLOSED_V5,
        HELLO_V6,
        WELCOME_V6,
        WORLD_SNAPSHOT_V6_ONE_PLAYER,
    ] {
        let short = &vector[..vector.len() - 1];
        assert!(decode_client_control(short).is_err());
        assert!(decode_server_control(short).is_err());
        assert!(decode_client_datagram(short).is_err());
        assert!(decode_server_datagram(short).is_err());
        assert!(decode_world_snapshot(short).is_err());
    }
    // Frame bounds still apply to the framed vectors.
    assert!(peek_frame_len(&0u32.to_le_bytes()).is_err());
    assert!(peek_frame_len(&(MAX_CONTROL_MESSAGE_BYTES + 1).to_le_bytes()).is_err());
    assert!(peek_gameplay_frame_len(&(MAX_GAMEPLAY_SNAPSHOT_BYTES + 1).to_le_bytes()).is_err());
}

#[test]
fn hello_v2_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 2,
        client_build: "test".to_string(),
        dev_login: String::new(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V2);
    assert_eq!(
        decode_client_control(HELLO_V2).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v2_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 2,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V2);
    assert_eq!(
        decode_server_control(WELCOME_V2).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn input_command_matches_golden_bytes() {
    let encoded = encode_client_control(&ClientControl::Input(input_right_jump())).expect("encode");
    assert_eq!(encoded, INPUT_RIGHT_JUMP_V4);
    assert_eq!(
        decode_client_control(INPUT_RIGHT_JUMP_V4).expect("decode"),
        ClientControl::Input(input_right_jump())
    );
}

#[test]
fn framed_input_matches_golden_bytes() {
    let payload = encode_client_control(&ClientControl::Input(input_right_jump())).expect("encode");
    let frame = encode_frame(&payload).expect("frame");
    assert_eq!(frame, INPUT_RIGHT_JUMP_V4_FRAMED);
    let prefix: [u8; 4] = INPUT_RIGHT_JUMP_V4_FRAMED[..4].try_into().expect("prefix");
    assert_eq!(peek_frame_len(&prefix).expect("peek"), 10);
    let (decoded_payload, rest) = decode_payload(INPUT_RIGHT_JUMP_V4_FRAMED).expect("decode frame");
    assert!(rest.is_empty());
    assert_eq!(decoded_payload, INPUT_RIGHT_JUMP_V4);
}

#[test]
fn v2_input_bytes_remain_frozen_and_are_incompatible() {
    assert_eq!(INPUT_RIGHT_JUMP.len(), 8);
    assert_eq!(INPUT_RIGHT_JUMP_FRAMED.len(), 12);
    assert!(decode_client_control(INPUT_RIGHT_JUMP).is_err());
}

#[test]
fn input_sequence_and_axis_are_little_endian() {
    assert_eq!(&INPUT_RIGHT_JUMP_V4[1..5], &GOLDEN_SEQUENCE.to_le_bytes());
    assert_ne!(&INPUT_RIGHT_JUMP_V4[1..5], &GOLDEN_SEQUENCE.to_be_bytes());
    assert_eq!(INPUT_RIGHT_JUMP_V4[7], MoveAxis::Right.as_u8());
    assert_eq!(&INPUT_RIGHT_JUMP_V4_FRAMED[..4], &10u32.to_le_bytes());
}

fn golden_snapshot() -> WorldSnapshot {
    WorldSnapshot {
        snapshot_sequence: GOLDEN_SEQUENCE,
        server_tick: 10,
        local_player_entity: WireEntityId {
            index: 5,
            generation: 1,
        },
        input_epoch: 0,
        last_acknowledged_input_sequence: 0,
        local_grounded: true,
        local_grounded_on: PlatformSupportId(1),
        local_ignored_platform: PlatformSupportId::NONE,
        continuation_debt: 0,
        local_map: 1,
        local_channel: 0,
        local_instance: 0,
        entities: vec![SnapshotEntity {
            entity_id: WireEntityId {
                index: 5,
                generation: 1,
            },
            kind: ReplicatedKind::Player,
            position: [1.0, 2.0],
            velocity: [3.0, 4.0],
        }],
    }
}

#[test]
fn hello_v3_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 3,
        client_build: "test".to_string(),
        dev_login: String::new(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V3);
    assert_eq!(
        decode_client_control(HELLO_V3).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v3_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 3,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V3);
    assert_eq!(
        decode_server_control(WELCOME_V3).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn world_snapshot_matches_golden_bytes() {
    let encoded = encode_world_snapshot(&golden_snapshot()).expect("encode");
    assert_eq!(encoded, WORLD_SNAPSHOT_V6_ONE_PLAYER);
    assert_eq!(
        decode_world_snapshot(WORLD_SNAPSHOT_V6_ONE_PLAYER).expect("decode"),
        golden_snapshot()
    );
}

#[test]
fn framed_world_snapshot_matches_golden_bytes() {
    let payload = encode_world_snapshot(&golden_snapshot()).expect("encode");
    let frame = encode_gameplay_frame(&payload).expect("frame");
    assert_eq!(frame, WORLD_SNAPSHOT_V6_ONE_PLAYER_FRAMED);
    let prefix: [u8; 4] = WORLD_SNAPSHOT_V6_ONE_PLAYER_FRAMED[..4]
        .try_into()
        .expect("prefix");
    assert_eq!(peek_gameplay_frame_len(&prefix).expect("peek"), 73);
    let (decoded_payload, rest) =
        decode_gameplay_payload(WORLD_SNAPSHOT_V6_ONE_PLAYER_FRAMED).expect("decode frame");
    assert!(rest.is_empty());
    assert_eq!(decoded_payload, WORLD_SNAPSHOT_V6_ONE_PLAYER);
}

#[test]
fn snapshot_is_not_a_control_message() {
    assert!(decode_server_control(WORLD_SNAPSHOT_V6_ONE_PLAYER).is_err());
    assert!(decode_client_control(WORLD_SNAPSHOT_V6_ONE_PLAYER).is_err());
    assert_eq!(WORLD_SNAPSHOT_V6_ONE_PLAYER.len(), 73);
    assert_eq!(
        WORLD_SNAPSHOT_V6_ONE_PLAYER_FRAMED.len(),
        4 + WORLD_SNAPSHOT_V6_ONE_PLAYER.len()
    );
}

#[test]
fn v3_snapshot_bytes_remain_frozen_and_are_incompatible() {
    assert_eq!(WORLD_SNAPSHOT_ONE_PLAYER.len(), 48);
    assert!(decode_world_snapshot(WORLD_SNAPSHOT_ONE_PLAYER).is_err());
}

#[test]
fn v4_snapshot_bytes_remain_frozen_and_are_incompatible() {
    assert_eq!(WORLD_SNAPSHOT_V4_ONE_PLAYER.len(), 61);
    assert!(decode_world_snapshot(WORLD_SNAPSHOT_V4_ONE_PLAYER).is_err());
    assert_eq!(WORLD_SNAPSHOT_V4_ONE_PLAYER_FRAMED.len(), 4 + 61);
}

#[test]
fn hello_v4_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 4,
        client_build: "test".to_string(),
        dev_login: String::new(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V4);
    assert_eq!(
        decode_client_control(HELLO_V4).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v4_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 4,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V4);
    assert_eq!(
        decode_server_control(WELCOME_V4).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn held_cancel_matches_golden_bytes() {
    let encoded = encode_client_control(&ClientControl::HeldCancel).expect("encode");
    assert_eq!(encoded, HELD_CANCEL);
    assert_eq!(
        decode_client_control(HELD_CANCEL).expect("decode"),
        ClientControl::HeldCancel
    );
}

#[test]
fn v4_golden_vectors_remain_frozen() {
    assert_eq!(HELLO_V4[1], 0x04, "do not rewrite v4 Hello bytes");
    assert_eq!(WELCOME_V4[1], 0x04, "do not rewrite v4 Welcome bytes");
}

#[test]
fn hello_v5_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 5,
        client_build: "test".to_string(),
        dev_login: String::new(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V5);
    assert_eq!(
        decode_client_control(HELLO_V5).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v5_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 5,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V5);
    assert_eq!(
        decode_server_control(WELCOME_V5).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn interact_control_matches_golden_bytes() {
    let target = WireEntityId {
        index: 1,
        generation: 1,
    };
    let open = ClientControl::InteractOpen(InteractOpen { target });
    assert_eq!(
        encode_client_control(&open).expect("encode"),
        INTERACT_OPEN_V5
    );
    assert_eq!(
        decode_client_control(INTERACT_OPEN_V5).expect("decode"),
        open
    );
    let close = ClientControl::InteractClose(InteractClose { session_id: 3 });
    assert_eq!(
        encode_client_control(&close).expect("encode"),
        INTERACT_CLOSE_V5
    );
    assert_eq!(
        decode_client_control(INTERACT_CLOSE_V5).expect("decode"),
        close
    );
    let opened = ServerControl::Interact(ServerInteract::Opened {
        session_id: 3,
        target,
    });
    assert_eq!(
        encode_server_control(&opened).expect("encode"),
        INTERACT_OPENED_V5
    );
    assert_eq!(
        decode_server_control(INTERACT_OPENED_V5).expect("decode"),
        opened
    );
    let rejected = ServerControl::Interact(ServerInteract::Rejected {
        target: WireEntityId {
            index: 2,
            generation: 3,
        },
        reason: InteractRejectReason::OutOfRange,
    });
    assert_eq!(
        encode_server_control(&rejected).expect("encode"),
        INTERACT_REJECTED_V5
    );
    let updated = ServerControl::Interact(ServerInteract::Updated {
        session_id: 3,
        target,
    });
    assert_eq!(
        encode_server_control(&updated).expect("encode"),
        INTERACT_UPDATED_V5
    );
    let closed = ServerControl::Interact(ServerInteract::Closed {
        session_id: 3,
        reason: InteractCloseReason::Requested,
    });
    assert_eq!(
        encode_server_control(&closed).expect("encode"),
        INTERACT_CLOSED_V5
    );
}

#[test]
fn hello_v6_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 6,
        client_build: "test".to_string(),
        dev_login: String::new(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V6);
    assert_eq!(
        decode_client_control(HELLO_V6).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v6_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 6,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V6);
    assert_eq!(
        decode_server_control(WELCOME_V6).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn hello_v7_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 7,
        client_build: "test".to_string(),
        dev_login: String::new(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V7);
    assert_eq!(
        decode_client_control(HELLO_V7).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v7_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 7,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V7);
    assert_eq!(
        decode_server_control(WELCOME_V7).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn hello_v8_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 8,
        client_build: "test".to_string(),
        dev_login: String::new(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V8);
    assert_eq!(
        decode_client_control(HELLO_V8).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v8_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 8,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V8);
    assert_eq!(
        decode_server_control(WELCOME_V8).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn hello_v9_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 9,
        client_build: "test".to_string(),
        dev_login: String::new(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V9);
    assert_eq!(
        decode_client_control(HELLO_V9).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v9_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 9,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V9);
    assert_eq!(
        decode_server_control(WELCOME_V9).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn hello_v10_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 10,
        client_build: "test".to_string(),
        dev_login: "dev.local".to_string(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V10);
    assert_eq!(
        decode_client_control(HELLO_V10).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v10_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 10,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V10);
    assert_eq!(
        decode_server_control(WELCOME_V10).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn hello_v11_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 11,
        client_build: "test".to_string(),
        dev_login: "dev.local".to_string(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V11);
    assert_eq!(
        decode_client_control(HELLO_V11).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v11_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 11,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V11);
    assert_eq!(
        decode_server_control(WELCOME_V11).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn hello_v12_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 12,
        client_build: "test".to_string(),
        dev_login: "dev.local".to_string(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V12);
    assert_eq!(
        decode_client_control(HELLO_V12).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v12_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 12,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V12);
    assert_eq!(
        decode_server_control(WELCOME_V12).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn hello_v13_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 13,
        client_build: "test".to_string(),
        dev_login: "dev.local".to_string(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V13);
    assert_eq!(
        decode_client_control(HELLO_V13).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v13_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 13,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V13);
    assert_eq!(
        decode_server_control(WELCOME_V13).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn hello_v14_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 14,
        client_build: "test".to_string(),
        dev_login: "dev.local".to_string(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V14);
    assert_eq!(
        decode_client_control(HELLO_V14).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v14_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 14,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V14);
    assert_eq!(
        decode_server_control(WELCOME_V14).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn presentation_oneshot_v13_matches_golden_bytes() {
    let req = ClientControl::DevPresentationOneShot(purgatory_protocol::DevPresentationOneShot {
        kind: 1,
    });
    assert_eq!(
        encode_client_control(&req).expect("encode"),
        DEV_PRESENTATION_ONESHOT_ATTACK_V13
    );
    assert_eq!(
        decode_client_control(DEV_PRESENTATION_ONESHOT_ATTACK_V13).expect("decode"),
        req
    );

    let evt = ServerControl::PresentationOneShot(purgatory_protocol::ServerPresentationOneShot {
        entity: WireEntityId {
            index: 3,
            generation: 2,
        },
        kind: 2,
        until_tick: 100,
    });
    assert_eq!(
        encode_server_control(&evt).expect("encode"),
        SERVER_PRESENTATION_ONESHOT_HURT_V13
    );
    assert_eq!(
        decode_server_control(SERVER_PRESENTATION_ONESHOT_HURT_V13).expect("decode"),
        evt
    );
}

#[test]
fn v10_golden_vectors_remain_frozen() {
    assert_eq!(HELLO_V10[1], 0x0A, "do not rewrite v10 Hello bytes");
    assert_eq!(WELCOME_V10[1], 0x0A, "do not rewrite v10 Welcome bytes");
}

#[test]
fn v11_golden_vectors_remain_frozen() {
    assert_eq!(HELLO_V11[1], 0x0B, "do not rewrite v11 Hello bytes");
    assert_eq!(WELCOME_V11[1], 0x0B, "do not rewrite v11 Welcome bytes");
}

#[test]
fn v14_golden_vectors_remain_frozen() {
    assert_eq!(HELLO_V14[1], 0x0E, "do not rewrite v14 Hello bytes");
    assert_eq!(WELCOME_V14[1], 0x0E, "do not rewrite v14 Welcome bytes");
}

#[test]
fn hello_v15_matches_golden_bytes() {
    let hello = Hello {
        protocol_version: 15,
        client_build: "test".to_string(),
        dev_login: "dev.local".to_string(),
    };
    let encoded = encode_client_control(&ClientControl::Hello(hello.clone())).expect("encode");
    assert_eq!(encoded, HELLO_V15);
    assert_eq!(
        decode_client_control(HELLO_V15).expect("decode"),
        ClientControl::Hello(hello)
    );
}

#[test]
fn welcome_v15_matches_golden_bytes() {
    let welcome = purgatory_protocol::Welcome {
        protocol_version: 15,
        connection_id: ConnectionId::from_raw(GOLDEN_CONNECTION_ID),
        server_tick_rate: 30,
        server_label: "purgatory-server-dev".to_string(),
    };
    let encoded = encode_server_control(&ServerControl::Welcome(welcome.clone())).expect("encode");
    assert_eq!(encoded, WELCOME_V15);
    assert_eq!(
        decode_server_control(WELCOME_V15).expect("decode"),
        ServerControl::Welcome(welcome)
    );
}

#[test]
fn ability_activate_v15_matches_golden_bytes() {
    let req = ClientControl::AbilityActivate(purgatory_protocol::AbilityActivateRequest {
        seq: 1,
        ability_id: purgatory_common::ContentId::from_token(9),
        selected: None,
    });
    assert_eq!(
        encode_client_control(&req).expect("encode"),
        ABILITY_ACTIVATE_INDEPENDENT_V15
    );
    assert_eq!(
        decode_client_control(ABILITY_ACTIVATE_INDEPENDENT_V15).expect("decode"),
        req
    );
    let accepted = ServerControl::Ability(purgatory_protocol::ServerAbility::Accepted { seq: 1 });
    assert_eq!(
        encode_server_control(&accepted).expect("encode"),
        ABILITY_ACCEPTED_V15
    );
    assert_eq!(
        decode_server_control(ABILITY_ACCEPTED_V15).expect("decode"),
        accepted
    );
}

#[test]
fn v13_golden_vectors_remain_frozen() {
    assert_eq!(HELLO_V13[1], 0x0D, "do not rewrite v13 Hello bytes");
    assert_eq!(WELCOME_V13[1], 0x0D, "do not rewrite v13 Welcome bytes");
}

#[test]
fn dev_reset_player_v14_matches_golden_bytes() {
    assert_eq!(
        encode_client_control(&ClientControl::DevResetPlayer).expect("encode"),
        DEV_RESET_PLAYER_V14
    );
    assert_eq!(
        decode_client_control(DEV_RESET_PLAYER_V14).expect("decode"),
        ClientControl::DevResetPlayer
    );
}

#[test]
fn dev_set_channel_matches_golden_bytes() {
    let msg = ClientControl::DevSetChannel(purgatory_protocol::DevSetChannel { channel: 1 });
    assert_eq!(
        encode_client_control(&msg).expect("encode"),
        DEV_SET_CHANNEL_V9
    );
    assert_eq!(
        decode_client_control(DEV_SET_CHANNEL_V9).expect("decode"),
        msg
    );
}

#[test]
fn portal_activate_matches_golden_bytes() {
    let target = WireEntityId {
        index: 1,
        generation: 1,
    };
    let msg = ClientControl::PortalActivate(PortalActivate { target });
    assert_eq!(
        encode_client_control(&msg).expect("encode"),
        PORTAL_ACTIVATE_V7
    );
    assert_eq!(
        decode_client_control(PORTAL_ACTIVATE_V7).expect("decode"),
        msg
    );
}

fn sample_v8_replication_frame() -> ReplicationFrame {
    let local = WireEntityId {
        index: 1,
        generation: 1,
    };
    ReplicationFrame {
        snapshot_sequence: 1,
        server_tick: 10,
        local_player_entity: local,
        input_epoch: 0,
        last_acknowledged_input_sequence: 0,
        local_grounded: false,
        local_grounded_on: PlatformSupportId::NONE,
        local_ignored_platform: PlatformSupportId::NONE,
        continuation_debt: 0,
        local_map: 1,
        local_channel: 0,
        local_instance: 0,
        observer_baseline_epoch: 0,
        records: vec![ReplicationRecord::Enter {
            entity: SnapshotEntity {
                entity_id: local,
                kind: ReplicatedKind::Player,
                position: [3.0, 4.0],
                velocity: [0.0, 0.0],
            },
            health: None,
            equipment: None,
        }],
        aoi_debug: None,
    }
}

#[test]
fn v8_replication_frame_roundtrip_and_tag() {
    let frame = sample_v8_replication_frame();
    let encoded = encode_replication_frame(&frame).expect("encode");
    assert_eq!(encoded[0], 16, "v8 frames use tag 16, not WorldSnapshot 7");
    assert_eq!(decode_replication_frame(&encoded).expect("decode"), frame);
}
