//! Bounded control and datagram messages. Explicit little-endian encoding.
//!
//! Unknown discriminants, truncated payloads, and oversized strings fail
//! with [`CodecError`]. Decoders never panic on untrusted input.

use crate::{ConnectionId, MAX_DATAGRAM_BYTES, MAX_LABEL_BYTES, PROTOCOL_VERSION};

const TAG_HELLO: u8 = 1;
const TAG_WELCOME: u8 = 2;
const TAG_DISCONNECT: u8 = 3;
const TAG_DATAGRAM_PING: u8 = 4;
const TAG_DATAGRAM_PONG: u8 = 5;
const TAG_INPUT: u8 = 6;
const TAG_HELD_CANCEL: u8 = 8;

/// Codec failure. Never treated as a successful message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    Truncated,
    UnknownDiscriminant(u8),
    InvalidUtf8,
    StringTooLong,
    TrailingBytes,
    DatagramTooLarge,
    SnapshotTooLarge,
    InvalidValue,
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated protocol payload"),
            Self::UnknownDiscriminant(tag) => write!(f, "unknown message discriminant {tag}"),
            Self::InvalidUtf8 => f.write_str("invalid utf-8 in bounded string"),
            Self::StringTooLong => f.write_str("bounded string exceeds MAX_LABEL_BYTES"),
            Self::TrailingBytes => f.write_str("unexpected trailing bytes in payload"),
            Self::DatagramTooLarge => f.write_str("datagram exceeds MAX_DATAGRAM_BYTES"),
            Self::SnapshotTooLarge => f.write_str("snapshot exceeds MAX_GAMEPLAY_SNAPSHOT_BYTES"),
            Self::InvalidValue => f.write_str("invalid field value"),
        }
    }
}

impl std::error::Error for CodecError {}

/// Wire disconnect reason. Only values the peer/server can meaningfully send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DisconnectReasonCode {
    VersionMismatch = 1,
    Malformed = 2,
    HandshakeTimeout = 3,
    UnexpectedMessage = 4,
    ServerShutdown = 5,
}

impl DisconnectReasonCode {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::VersionMismatch),
            2 => Some(Self::Malformed),
            3 => Some(Self::HandshakeTimeout),
            4 => Some(Self::UnexpectedMessage),
            5 => Some(Self::ServerShutdown),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VersionMismatch => "version mismatch",
            Self::Malformed => "malformed message",
            Self::HandshakeTimeout => "handshake timeout",
            Self::UnexpectedMessage => "unexpected message",
            Self::ServerShutdown => "server shutdown",
        }
    }
}

impl std::fmt::Display for DisconnectReasonCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Server-sent disconnect on the reliable control stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisconnectReason {
    pub code: DisconnectReasonCode,
    pub detail: String,
}

impl DisconnectReason {
    #[must_use]
    pub fn new(code: DisconnectReasonCode, detail: impl Into<String>) -> Self {
        let mut detail = detail.into();
        if detail.len() > MAX_LABEL_BYTES {
            detail.truncate(MAX_LABEL_BYTES);
        }
        Self { code, detail }
    }
}

/// Client Hello. Does not include a client-chosen connection id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hello {
    pub protocol_version: u32,
    pub client_build: String,
}

/// Server Welcome after a valid Hello.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Welcome {
    pub protocol_version: u32,
    pub connection_id: ConnectionId,
    pub server_tick_rate: u32,
    pub server_label: String,
}

/// Horizontal intent. Wire encoding is a single `u8`. Invalid values are rejected.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum MoveAxis {
    Left = 0,
    #[default]
    Neutral = 1,
    Right = 2,
}

impl MoveAxis {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Left),
            1 => Some(Self::Neutral),
            2 => Some(Self::Right),
            _ => None,
        }
    }

    /// Simulation `PlayerInput.move_axis`: `-1` / `0` / `1`.
    #[must_use]
    pub const fn to_i8(self) -> i8 {
        match self {
            Self::Left => -1,
            Self::Neutral => 0,
            Self::Right => 1,
        }
    }

    #[must_use]
    pub const fn from_i8(value: i8) -> Self {
        match value {
            -1 => Self::Left,
            1 => Self::Right,
            _ => Self::Neutral,
        }
    }
}

impl std::fmt::Display for MoveAxis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Left => "Left",
            Self::Neutral => "Neutral",
            Self::Right => "Right",
        })
    }
}

/// Client → server intent-only gameplay input. Never carries position, velocity,
/// grounded state, `EntityId`, or `ConnectionId`.
///
/// Identity is `(input_epoch, sequence)`. `sequence` starts at 1 and increases
/// by 1 per predicted simulation step within an epoch. Wrapping of sequence or
/// epoch within a session is not supported and is treated as stale / disconnect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputCommand {
    pub input_epoch: u16,
    pub sequence: u32,
    pub move_axis: MoveAxis,
    pub jump_pressed: bool,
    pub down_held: bool,
}

impl InputCommand {
    /// Command identity used by prediction, acknowledgement, and tests.
    #[must_use]
    pub const fn identity(self) -> (u16, u32) {
        (self.input_epoch, self.sequence)
    }
}

/// Client → server reliable control.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClientControl {
    Hello(Hello),
    Input(InputCommand),
    /// Pathological focus-loss / send-window barrier. No sequence.
    HeldCancel,
}

/// Server → client reliable control.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerControl {
    Welcome(Welcome),
    Disconnect(DisconnectReason),
}

/// Server datagram (pong only in Phase 5.0).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerDatagram {
    Pong { nonce: u64 },
}

/// Validate a decoded Hello. Protocol mismatch is never accepted silently.
pub fn validate_hello(hello: &Hello) -> Result<(), DisconnectReason> {
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(DisconnectReason::new(
            DisconnectReasonCode::VersionMismatch,
            format!("got {} want {PROTOCOL_VERSION}", hello.protocol_version),
        ));
    }
    if hello.client_build.len() > MAX_LABEL_BYTES {
        return Err(DisconnectReason::new(
            DisconnectReasonCode::Malformed,
            "client_build too long",
        ));
    }
    Ok(())
}

pub fn encode_client_control(msg: &ClientControl) -> Result<Vec<u8>, CodecError> {
    match msg {
        ClientControl::Hello(hello) => {
            let mut out = Vec::new();
            out.push(TAG_HELLO);
            out.extend_from_slice(&hello.protocol_version.to_le_bytes());
            write_bounded_string(&mut out, &hello.client_build)?;
            Ok(out)
        }
        ClientControl::Input(cmd) => {
            let mut out = Vec::with_capacity(10);
            out.push(TAG_INPUT);
            out.extend_from_slice(&cmd.sequence.to_le_bytes());
            out.extend_from_slice(&cmd.input_epoch.to_le_bytes());
            out.push(cmd.move_axis.as_u8());
            out.push(u8::from(cmd.jump_pressed));
            out.push(u8::from(cmd.down_held));
            Ok(out)
        }
        ClientControl::HeldCancel => Ok(vec![TAG_HELD_CANCEL]),
    }
}

pub fn decode_client_control(bytes: &[u8]) -> Result<ClientControl, CodecError> {
    let (tag, rest) = split_tag(bytes)?;
    match tag {
        TAG_HELLO => {
            let (protocol_version, rest) = read_u32(rest)?;
            let (client_build, rest) = read_bounded_string(rest)?;
            expect_empty(rest)?;
            Ok(ClientControl::Hello(Hello {
                protocol_version,
                client_build,
            }))
        }
        TAG_INPUT => {
            let (sequence, rest) = read_u32(rest)?;
            if rest.len() < 5 {
                return Err(CodecError::Truncated);
            }
            let input_epoch = u16::from_le_bytes([rest[0], rest[1]]);
            let move_axis = MoveAxis::from_u8(rest[2]).ok_or(CodecError::InvalidValue)?;
            let jump_pressed = read_flag(rest[3])?;
            let down_held = read_flag(rest[4])?;
            expect_empty(&rest[5..])?;
            Ok(ClientControl::Input(InputCommand {
                input_epoch,
                sequence,
                move_axis,
                jump_pressed,
                down_held,
            }))
        }
        TAG_HELD_CANCEL => {
            expect_empty(rest)?;
            Ok(ClientControl::HeldCancel)
        }
        other => Err(CodecError::UnknownDiscriminant(other)),
    }
}

pub fn encode_server_control(msg: &ServerControl) -> Result<Vec<u8>, CodecError> {
    match msg {
        ServerControl::Welcome(welcome) => {
            let mut out = Vec::new();
            out.push(TAG_WELCOME);
            out.extend_from_slice(&welcome.protocol_version.to_le_bytes());
            out.extend_from_slice(&welcome.connection_id.get().to_le_bytes());
            out.extend_from_slice(&welcome.server_tick_rate.to_le_bytes());
            write_bounded_string(&mut out, &welcome.server_label)?;
            Ok(out)
        }
        ServerControl::Disconnect(reason) => {
            let mut out = Vec::new();
            out.push(TAG_DISCONNECT);
            out.push(reason.code.as_u8());
            write_bounded_string(&mut out, &reason.detail)?;
            Ok(out)
        }
    }
}

pub fn decode_server_control(bytes: &[u8]) -> Result<ServerControl, CodecError> {
    let (tag, rest) = split_tag(bytes)?;
    match tag {
        TAG_WELCOME => {
            let (protocol_version, rest) = read_u32(rest)?;
            let (connection_id, rest) = read_u64(rest)?;
            let (server_tick_rate, rest) = read_u32(rest)?;
            let (server_label, rest) = read_bounded_string(rest)?;
            expect_empty(rest)?;
            Ok(ServerControl::Welcome(Welcome {
                protocol_version,
                connection_id: ConnectionId::from_raw(connection_id),
                server_tick_rate,
                server_label,
            }))
        }
        TAG_DISCONNECT => {
            if rest.is_empty() {
                return Err(CodecError::Truncated);
            }
            let code = DisconnectReasonCode::from_u8(rest[0])
                .ok_or(CodecError::UnknownDiscriminant(rest[0]))?;
            let (detail, rest) = read_bounded_string(&rest[1..])?;
            expect_empty(rest)?;
            Ok(ServerControl::Disconnect(DisconnectReason { code, detail }))
        }
        other => Err(CodecError::UnknownDiscriminant(other)),
    }
}

pub fn encode_client_datagram(nonce: u64) -> Result<Vec<u8>, CodecError> {
    let mut out = Vec::with_capacity(1 + 8);
    out.push(TAG_DATAGRAM_PING);
    out.extend_from_slice(&nonce.to_le_bytes());
    if out.len() > MAX_DATAGRAM_BYTES {
        return Err(CodecError::DatagramTooLarge);
    }
    Ok(out)
}

pub fn decode_client_datagram(bytes: &[u8]) -> Result<u64, CodecError> {
    if bytes.len() > MAX_DATAGRAM_BYTES {
        return Err(CodecError::DatagramTooLarge);
    }
    let (tag, rest) = split_tag(bytes)?;
    if tag != TAG_DATAGRAM_PING {
        return Err(CodecError::UnknownDiscriminant(tag));
    }
    let (nonce, rest) = read_u64(rest)?;
    expect_empty(rest)?;
    Ok(nonce)
}

pub fn encode_server_datagram(msg: ServerDatagram) -> Result<Vec<u8>, CodecError> {
    match msg {
        ServerDatagram::Pong { nonce } => {
            let mut out = Vec::with_capacity(1 + 8);
            out.push(TAG_DATAGRAM_PONG);
            out.extend_from_slice(&nonce.to_le_bytes());
            if out.len() > MAX_DATAGRAM_BYTES {
                return Err(CodecError::DatagramTooLarge);
            }
            Ok(out)
        }
    }
}

pub fn decode_server_datagram(bytes: &[u8]) -> Result<ServerDatagram, CodecError> {
    if bytes.len() > MAX_DATAGRAM_BYTES {
        return Err(CodecError::DatagramTooLarge);
    }
    let (tag, rest) = split_tag(bytes)?;
    if tag != TAG_DATAGRAM_PONG {
        return Err(CodecError::UnknownDiscriminant(tag));
    }
    let (nonce, rest) = read_u64(rest)?;
    expect_empty(rest)?;
    Ok(ServerDatagram::Pong { nonce })
}

fn split_tag(bytes: &[u8]) -> Result<(u8, &[u8]), CodecError> {
    if bytes.is_empty() {
        Err(CodecError::Truncated)
    } else {
        Ok((bytes[0], &bytes[1..]))
    }
}

fn read_u32(bytes: &[u8]) -> Result<(u32, &[u8]), CodecError> {
    if bytes.len() < 4 {
        return Err(CodecError::Truncated);
    }
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&bytes[..4]);
    Ok((u32::from_le_bytes(buf), &bytes[4..]))
}

fn read_u64(bytes: &[u8]) -> Result<(u64, &[u8]), CodecError> {
    if bytes.len() < 8 {
        return Err(CodecError::Truncated);
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[..8]);
    Ok((u64::from_le_bytes(buf), &bytes[8..]))
}

fn write_bounded_string(out: &mut Vec<u8>, value: &str) -> Result<(), CodecError> {
    if value.len() > MAX_LABEL_BYTES {
        return Err(CodecError::StringTooLong);
    }
    let len = u8::try_from(value.len()).map_err(|_| CodecError::StringTooLong)?;
    out.push(len);
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn read_bounded_string(bytes: &[u8]) -> Result<(String, &[u8]), CodecError> {
    if bytes.is_empty() {
        return Err(CodecError::Truncated);
    }
    let len = usize::from(bytes[0]);
    if len > MAX_LABEL_BYTES {
        return Err(CodecError::StringTooLong);
    }
    let rest = &bytes[1..];
    if rest.len() < len {
        return Err(CodecError::Truncated);
    }
    let text = std::str::from_utf8(&rest[..len]).map_err(|_| CodecError::InvalidUtf8)?;
    Ok((text.to_string(), &rest[len..]))
}

fn read_flag(value: u8) -> Result<bool, CodecError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(CodecError::InvalidValue),
    }
}

fn expect_empty(bytes: &[u8]) -> Result<(), CodecError> {
    if bytes.is_empty() {
        Ok(())
    } else {
        Err(CodecError::TrailingBytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode_payload, encode_frame, peek_frame_len};

    fn hello_dev() -> Hello {
        Hello {
            protocol_version: PROTOCOL_VERSION,
            client_build: "dev".into(),
        }
    }

    fn welcome_dev() -> Welcome {
        Welcome {
            protocol_version: PROTOCOL_VERSION,
            connection_id: ConnectionId::from_raw(3),
            server_tick_rate: 30,
            server_label: "purgatory-server-dev".into(),
        }
    }

    #[test]
    fn hello_roundtrip() {
        let encoded = encode_client_control(&ClientControl::Hello(hello_dev())).unwrap();
        let decoded = decode_client_control(&encoded).unwrap();
        assert_eq!(decoded, ClientControl::Hello(hello_dev()));
    }

    #[test]
    fn welcome_roundtrip() {
        let encoded = encode_server_control(&ServerControl::Welcome(welcome_dev())).unwrap();
        let decoded = decode_server_control(&encoded).unwrap();
        assert_eq!(decoded, ServerControl::Welcome(welcome_dev()));
    }

    #[test]
    fn disconnect_reason_roundtrip() {
        let reason = DisconnectReason::new(DisconnectReasonCode::Malformed, "bad hello");
        let encoded = encode_server_control(&ServerControl::Disconnect(reason.clone())).unwrap();
        let decoded = decode_server_control(&encoded).unwrap();
        assert_eq!(decoded, ServerControl::Disconnect(reason));
    }

    #[test]
    fn ping_pong_nonce_only_roundtrip() {
        let ping = encode_client_datagram(42).unwrap();
        assert_eq!(decode_client_datagram(&ping).unwrap(), 42);
        let pong = encode_server_datagram(ServerDatagram::Pong { nonce: 42 }).unwrap();
        assert_eq!(
            decode_server_datagram(&pong).unwrap(),
            ServerDatagram::Pong { nonce: 42 }
        );
        // No timestamp bytes on the wire: tag + u64 only.
        assert_eq!(ping.len(), 9);
        assert_eq!(pong.len(), 9);
    }

    fn input_dev() -> InputCommand {
        InputCommand {
            input_epoch: 0,
            sequence: 7,
            move_axis: MoveAxis::Right,
            jump_pressed: true,
            down_held: false,
        }
    }

    #[test]
    fn input_command_roundtrip() {
        let encoded = encode_client_control(&ClientControl::Input(input_dev())).unwrap();
        let decoded = decode_client_control(&encoded).unwrap();
        assert_eq!(decoded, ClientControl::Input(input_dev()));
        // tag + u32 seq + u16 epoch + axis + jump + down
        assert_eq!(encoded.len(), 1 + 4 + 2 + 1 + 1 + 1);
    }

    #[test]
    fn invalid_move_axis_is_rejected() {
        let mut bytes = vec![TAG_INPUT];
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 3, 0, 0]);
        assert_eq!(decode_client_control(&bytes), Err(CodecError::InvalidValue));
    }

    #[test]
    fn invalid_jump_or_down_flag_is_rejected() {
        let mut jump = vec![TAG_INPUT];
        jump.extend_from_slice(&1u32.to_le_bytes());
        jump.extend_from_slice(&[0, 0, MoveAxis::Neutral.as_u8(), 2, 0]);
        assert_eq!(decode_client_control(&jump), Err(CodecError::InvalidValue));
        let mut down = vec![TAG_INPUT];
        down.extend_from_slice(&1u32.to_le_bytes());
        down.extend_from_slice(&[0, 0, MoveAxis::Neutral.as_u8(), 0, 9]);
        assert_eq!(decode_client_control(&down), Err(CodecError::InvalidValue));
    }

    #[test]
    fn input_command_has_no_identity_fields() {
        let src = include_str!("message.rs");
        let start = src.find("pub struct InputCommand").expect("struct");
        let body = &src[start..start + 420];
        assert!(!body.contains("EntityId"));
        assert!(!body.contains("ConnectionId"));
        assert!(!body.contains("position"));
        assert!(!body.contains("velocity"));
    }

    #[test]
    fn framed_hello_roundtrip() {
        let payload = encode_client_control(&ClientControl::Hello(hello_dev())).unwrap();
        let frame = encode_frame(&payload).unwrap();
        let (decoded_payload, rest) = crate::decode_payload(&frame).unwrap();
        assert!(rest.is_empty());
        assert_eq!(
            decode_client_control(decoded_payload).unwrap(),
            ClientControl::Hello(hello_dev())
        );
    }

    #[test]
    fn unknown_discriminant_is_error() {
        assert_eq!(
            decode_client_control(&[99]),
            Err(CodecError::UnknownDiscriminant(99))
        );
        assert_eq!(
            decode_server_control(&[99]),
            Err(CodecError::UnknownDiscriminant(99))
        );
        assert!(DisconnectReasonCode::from_u8(99).is_none());
    }

    #[test]
    fn truncated_payload_is_error() {
        assert_eq!(decode_client_control(&[]), Err(CodecError::Truncated));
        assert_eq!(
            decode_client_control(&[TAG_HELLO]),
            Err(CodecError::Truncated)
        );
        assert_eq!(
            decode_client_control(&[TAG_HELLO, 1, 2, 3]),
            Err(CodecError::Truncated)
        );
        assert_eq!(
            decode_client_datagram(&[TAG_DATAGRAM_PING, 1, 2]),
            Err(CodecError::Truncated)
        );
    }

    #[test]
    fn protocol_mismatch_is_rejected_by_validate() {
        let hello = Hello {
            protocol_version: PROTOCOL_VERSION + 9,
            client_build: "dev".into(),
        };
        let err = validate_hello(&hello).expect_err("mismatch");
        assert_eq!(err.code, DisconnectReasonCode::VersionMismatch);
        assert!(validate_hello(&hello_dev()).is_ok());
    }

    #[test]
    fn old_protocol_version_1_is_rejected() {
        let hello = Hello {
            protocol_version: 1,
            client_build: "legacy-v1".into(),
        };
        let err = validate_hello(&hello).expect_err("v1");
        assert_eq!(err.code, DisconnectReasonCode::VersionMismatch);
        assert_ne!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn held_cancel_roundtrip() {
        let encoded = encode_client_control(&ClientControl::HeldCancel).unwrap();
        assert_eq!(encoded, vec![TAG_HELD_CANCEL]);
        assert_eq!(
            decode_client_control(&encoded).unwrap(),
            ClientControl::HeldCancel
        );
    }

    #[test]
    fn old_protocol_version_2_is_rejected() {
        let hello = Hello {
            protocol_version: 2,
            client_build: "legacy-v2".into(),
        };
        let err = validate_hello(&hello).expect_err("v2");
        assert_eq!(err.code, DisconnectReasonCode::VersionMismatch);
        assert_ne!(PROTOCOL_VERSION, 2);
    }

    #[test]
    fn old_protocol_version_3_is_rejected() {
        let hello = Hello {
            protocol_version: 3,
            client_build: "legacy-v3".into(),
        };
        let err = validate_hello(&hello).expect_err("v3");
        assert_eq!(err.code, DisconnectReasonCode::VersionMismatch);
        assert_ne!(PROTOCOL_VERSION, 3);
    }

    #[test]
    fn string_too_long_rejected() {
        let hello = Hello {
            protocol_version: PROTOCOL_VERSION,
            client_build: "x".repeat(MAX_LABEL_BYTES + 1),
        };
        assert_eq!(
            encode_client_control(&ClientControl::Hello(hello)),
            Err(CodecError::StringTooLong)
        );
        let mut bytes = vec![TAG_HELLO];
        bytes.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        bytes.push(u8::try_from(MAX_LABEL_BYTES + 1).unwrap_or(255));
        bytes.extend(std::iter::repeat_n(b'a', 65));
        assert_eq!(
            decode_client_control(&bytes),
            Err(CodecError::StringTooLong)
        );
    }

    #[test]
    fn oversized_length_prefix_rejected() {
        assert!(peek_frame_len(&1_000_000u32.to_le_bytes()).is_err());
    }

    #[test]
    fn invalid_utf8_rejected() {
        let mut bytes = vec![TAG_HELLO];
        bytes.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        bytes.push(2);
        bytes.extend_from_slice(&[0xff, 0xfe]);
        assert_eq!(decode_client_control(&bytes), Err(CodecError::InvalidUtf8));
    }

    #[test]
    fn hello_does_not_carry_connection_id() {
        let encoded = encode_client_control(&ClientControl::Hello(hello_dev())).unwrap();
        // tag + u32 version + u8 strlen + "dev"
        assert_eq!(encoded.len(), 1 + 4 + 1 + 3);
        let decoded = decode_client_control(&encoded).unwrap();
        let ClientControl::Hello(hello) = decoded else {
            panic!("expected Hello");
        };
        assert_eq!(hello.client_build, "dev");
    }

    #[test]
    fn client_cannot_send_welcome_or_disconnect() {
        let welcome = encode_server_control(&ServerControl::Welcome(welcome_dev())).unwrap();
        assert!(matches!(
            decode_client_control(&welcome),
            Err(CodecError::UnknownDiscriminant(_))
        ));
        let disc = encode_server_control(&ServerControl::Disconnect(DisconnectReason::new(
            DisconnectReasonCode::Malformed,
            "x",
        )))
        .unwrap();
        assert!(matches!(
            decode_client_control(&disc),
            Err(CodecError::UnknownDiscriminant(_))
        ));
    }

    #[test]
    fn truncation_matrix_never_panics() {
        let hello = encode_client_control(&ClientControl::Hello(hello_dev())).unwrap();
        let welcome = encode_server_control(&ServerControl::Welcome(welcome_dev())).unwrap();
        let ping = encode_client_datagram(1).unwrap();
        let pong = encode_server_datagram(ServerDatagram::Pong { nonce: 1 }).unwrap();
        let input = encode_client_control(&ClientControl::Input(input_dev())).unwrap();
        let snapshot = crate::encode_world_snapshot(&crate::WorldSnapshot {
            snapshot_sequence: 1,
            server_tick: 1,
            local_player_entity: crate::WireEntityId {
                index: 1,
                generation: 1,
            },
            input_epoch: 0,
            last_acknowledged_input_sequence: 0,
            local_grounded: true,
            local_grounded_on: crate::PlatformSupportId::NONE,
            local_ignored_platform: crate::PlatformSupportId::NONE,
            continuation_debt: 0,
            entities: Vec::new(),
        })
        .unwrap();
        for encoded in [&hello, &welcome, &ping, &pong, &input, &snapshot] {
            for n in 0..=encoded.len() {
                let slice = &encoded[..n];
                let _ = decode_client_control(slice);
                let _ = decode_server_control(slice);
                let _ = decode_client_datagram(slice);
                let _ = decode_server_datagram(slice);
                let _ = decode_payload(slice);
                let _ = crate::decode_world_snapshot(slice);
            }
        }
        let mut hello_frame = encode_frame(&hello).unwrap();
        let full = hello_frame.len();
        for n in 0..full {
            hello_frame.truncate(n);
            let _ = decode_payload(&hello_frame);
            hello_frame = encode_frame(&hello).unwrap();
        }
    }

    #[test]
    fn random_decoder_corpus_never_panics() {
        let mut seed = 0xC0FFEE_u64;
        for _ in 0..4000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let len = (seed % 96) as usize;
            let mut buf = vec![0u8; len];
            for byte in &mut buf {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                *byte = (seed >> 24) as u8;
            }
            let _ = decode_payload(&buf);
            let _ = decode_client_control(&buf);
            let _ = decode_server_control(&buf);
            let _ = decode_client_datagram(&buf);
            let _ = decode_server_datagram(&buf);
            let _ = crate::decode_world_snapshot(&buf);
            if buf.len() >= 4 {
                let prefix: [u8; 4] = buf[..4].try_into().unwrap();
                let _ = peek_frame_len(&prefix);
                let _ = crate::peek_gameplay_frame_len(&prefix);
            }
        }
    }

    #[test]
    fn declared_string_length_without_bytes_is_truncated() {
        let mut bytes = vec![TAG_HELLO];
        bytes.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        bytes.push(8);
        bytes.extend_from_slice(b"ab");
        assert_eq!(decode_client_control(&bytes), Err(CodecError::Truncated));
    }

    #[test]
    fn payload_one_byte_short_is_truncated() {
        let encoded = encode_client_control(&ClientControl::Hello(hello_dev())).unwrap();
        assert_eq!(
            decode_client_control(&encoded[..encoded.len() - 1]),
            Err(CodecError::Truncated)
        );
    }
}
