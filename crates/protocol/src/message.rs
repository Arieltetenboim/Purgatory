//! Bounded control and datagram messages. Explicit little-endian encoding.
//!
//! Unknown discriminants, truncated payloads, and oversized strings fail
//! with [`CodecError`]. Decoders never panic on untrusted input.

use crate::ability::{
    ABILITY_ACTIVATE_INDEPENDENT_BYTES, ABILITY_ACTIVATE_SELECTED_BYTES, AbilityActivateRequest,
    AbilityCommandReject, ServerAbility,
};
use crate::dialogue::{
    DialogueAdvance, DialogueChoose, ServerDialogueChoiceAccepted, ServerDialogueLine,
};
use crate::equipment::{
    EquipRequest, EquipmentRejectReason, ServerEquipment, UnequipRequest, slot_valid,
};
use crate::interact::{
    DevSetChannel, DevSetJump, DevSetSpeed, DevSpawnNpc, InteractClose, InteractOpen,
    PortalActivate, ServerInteract,
};
use crate::inventory::{
    INVENTORY_CAPACITY, InventoryEntry, ServerInventory, decode_inventory_entry,
};
use crate::item::{PickupRejectReason, PickupRequest, ServerItem, decode_item_instance_id};
use crate::presentation_oneshot::{
    DevPresentationOneShot, ServerPresentationOneShot, TAG_DEV_PRESENTATION_ONESHOT,
    TAG_SERVER_PRESENTATION_ONESHOT, decode_dev_presentation_oneshot,
    decode_server_presentation_oneshot, encode_dev_presentation_oneshot,
    encode_server_presentation_oneshot,
};
use crate::snapshot::WireEntityId;
use crate::{
    ConnectionId, HELLO_DEV_LOGIN_SINCE, MAX_DATAGRAM_BYTES, MAX_LABEL_BYTES, PROTOCOL_VERSION,
};

const TAG_HELLO: u8 = 1;
const TAG_WELCOME: u8 = 2;
const TAG_DISCONNECT: u8 = 3;
const TAG_DATAGRAM_PING: u8 = 4;
const TAG_DATAGRAM_PONG: u8 = 5;
const TAG_INPUT: u8 = 6;
const TAG_HELD_CANCEL: u8 = 8;
const TAG_INTERACT_OPEN: u8 = 9;
const TAG_INTERACT_CLOSE: u8 = 10;
const TAG_INTERACT_OPENED: u8 = 11;
const TAG_INTERACT_REJECTED: u8 = 12;
const TAG_INTERACT_UPDATED: u8 = 13;
const TAG_INTERACT_CLOSED: u8 = 14;
const TAG_PORTAL_ACTIVATE: u8 = 15;
const TAG_DEV_SET_CHANNEL: u8 = 17;
const TAG_EQUIP: u8 = 18;
const TAG_UNEQUIP: u8 = 19;
const TAG_EQUIPMENT_ACCEPTED: u8 = 20;
const TAG_EQUIPMENT_REJECTED: u8 = 21;
const TAG_DEV_RESET_PLAYER: u8 = 24;
const TAG_ABILITY_ACTIVATE: u8 = 25;
const TAG_ABILITY_ACCEPTED: u8 = 26;
const TAG_ABILITY_REJECTED: u8 = 27;
const TAG_RESPAWN: u8 = 28;
const TAG_DEV_SET_SPEED: u8 = 29;
const TAG_DEV_SET_JUMP: u8 = 30;
const TAG_PICKUP: u8 = 31;
const TAG_PICKUP_ACCEPTED: u8 = 32;
const TAG_PICKUP_REJECTED: u8 = 33;
const TAG_INVENTORY_SNAPSHOT: u8 = 34;
const TAG_DIALOGUE_ADVANCE: u8 = 35;
const TAG_DIALOGUE_ACTIVE_LINE: u8 = 36;
const TAG_DEV_SPAWN_NPC: u8 = 37;
const TAG_DIALOGUE_CHOOSE: u8 = 38;
const TAG_DIALOGUE_CHOICE_ACCEPTED: u8 = 39;

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
    AlreadyConnected = 6,
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
            6 => Some(Self::AlreadyConnected),
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
            Self::AlreadyConnected => "already connected",
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
///
/// `dev_login` is a temporary DEV lookup identity (protocol v10+). The client
/// cannot choose `ConnectionId` or `CharacterId`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hello {
    pub protocol_version: u32,
    pub client_build: String,
    pub dev_login: String,
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
    /// True while Up (portal activate) is held. Optional 1-byte trailer;
    /// omitted when false. Server uses a falling edge to clear the portal
    /// reentry lock. Not movement.
    pub portal_held: bool,
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
    InteractOpen(InteractOpen),
    InteractClose(InteractClose),
    DialogueAdvance(DialogueAdvance),
    DialogueChoose(DialogueChoose),
    PortalActivate(PortalActivate),
    /// DEV overlay Channel request. Server validates and owns WorldAddress.
    DevSetChannel(DevSetChannel),
    /// DEV overlay movement speed request. Server validates and owns the result.
    DevSetSpeed(DevSetSpeed),
    /// DEV overlay jump-speed request. Server validates and owns the result.
    DevSetJump(DevSetJump),
    /// DEV overlay NPC spawn request. Server resolves content and owns placement.
    DevSpawnNpc(DevSpawnNpc),
    Equip(EquipRequest),
    Unequip(UnequipRequest),
    /// DEV presentation Attack/Hurt oneshot request (protocol v13).
    DevPresentationOneShot(DevPresentationOneShot),
    /// DEV overlay spawn reset. Server applies `DebugAction::ResetPlayer` to the bound actor.
    DevResetPlayer,
    /// Ability activation intent (protocol v15). Ability id + optional selected entity.
    AbilityActivate(AbilityActivateRequest),
    /// Request pickup of a visible world-drop manifestation.
    Pickup(PickupRequest),
    /// Request authoritative restoration of the bound player after death.
    Respawn,
}

/// Server → client reliable control.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerControl {
    Welcome(Welcome),
    Disconnect(DisconnectReason),
    Interact(ServerInteract),
    DialogueLine(ServerDialogueLine),
    DialogueChoiceAccepted(ServerDialogueChoiceAccepted),
    Equipment(ServerEquipment),
    /// Authoritative presentation oneshot start/clear (protocol v13).
    PresentationOneShot(ServerPresentationOneShot),
    /// Ability request lifecycle only (protocol v15). Not a hit or Health write.
    Ability(ServerAbility),
    /// Authoritative item transaction result.
    Item(ServerItem),
    /// Owner-private authoritative inventory baseline.
    Inventory(ServerInventory),
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
    if purgatory_common::DevLogin::parse(&hello.dev_login).is_err() {
        return Err(DisconnectReason::new(
            DisconnectReasonCode::Malformed,
            "dev_login",
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
            if hello.protocol_version >= HELLO_DEV_LOGIN_SINCE {
                write_bounded_string(&mut out, &hello.dev_login)?;
            }
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
            if cmd.portal_held {
                out.push(1);
            }
            Ok(out)
        }
        ClientControl::HeldCancel => Ok(vec![TAG_HELD_CANCEL]),
        ClientControl::InteractOpen(open) => {
            let mut out = Vec::with_capacity(1 + 8);
            out.push(TAG_INTERACT_OPEN);
            write_wire_entity(&mut out, open.target);
            Ok(out)
        }
        ClientControl::InteractClose(close) => {
            let mut out = Vec::with_capacity(1 + 4);
            out.push(TAG_INTERACT_CLOSE);
            out.extend_from_slice(&close.session_id.to_le_bytes());
            Ok(out)
        }
        ClientControl::DialogueAdvance(advance) => {
            if advance.session_id == 0 {
                return Err(CodecError::InvalidValue);
            }
            let mut out = Vec::with_capacity(1 + 4);
            out.push(TAG_DIALOGUE_ADVANCE);
            out.extend_from_slice(&advance.session_id.to_le_bytes());
            Ok(out)
        }
        ClientControl::DialogueChoose(choice) => {
            if choice.session_id == 0 {
                return Err(CodecError::InvalidValue);
            }
            let mut out = Vec::with_capacity(1 + crate::DIALOGUE_CHOOSE_BYTES);
            out.push(TAG_DIALOGUE_CHOOSE);
            out.extend_from_slice(&choice.session_id.to_le_bytes());
            out.extend_from_slice(&choice.beat_index.to_le_bytes());
            out.extend_from_slice(&choice.choice_index.to_le_bytes());
            Ok(out)
        }
        ClientControl::PortalActivate(activate) => {
            let mut out = Vec::with_capacity(1 + 8);
            out.push(TAG_PORTAL_ACTIVATE);
            write_wire_entity(&mut out, activate.target);
            Ok(out)
        }
        ClientControl::DevSetChannel(req) => {
            let mut out = Vec::with_capacity(1 + 4);
            out.push(TAG_DEV_SET_CHANNEL);
            out.extend_from_slice(&req.channel.to_le_bytes());
            Ok(out)
        }
        ClientControl::DevSetSpeed(req) => {
            let mut out = Vec::with_capacity(1 + 1 + 2);
            out.push(TAG_DEV_SET_SPEED);
            match req.speed {
                Some(speed) => {
                    out.push(1);
                    out.extend_from_slice(&speed.to_le_bytes());
                }
                None => out.push(0),
            }
            Ok(out)
        }
        ClientControl::DevSetJump(req) => {
            let mut out = Vec::with_capacity(1 + 1 + 2);
            out.push(TAG_DEV_SET_JUMP);
            match req.jump {
                Some(jump) => {
                    out.push(1);
                    out.extend_from_slice(&jump.to_le_bytes());
                }
                None => out.push(0),
            }
            Ok(out)
        }
        ClientControl::DevSpawnNpc(req) => {
            if req.npc_content_id.kind() != Some(purgatory_common::ContentKind::Npc) {
                return Err(CodecError::InvalidValue);
            }
            let npc_content_id = req.npc_content_id.raw().ok_or(CodecError::InvalidValue)?;
            let mut out = Vec::with_capacity(1 + crate::DEV_SPAWN_NPC_BYTES);
            out.push(TAG_DEV_SPAWN_NPC);
            out.extend_from_slice(&npc_content_id.to_le_bytes());
            Ok(out)
        }
        ClientControl::Equip(req) => {
            if !slot_valid(req.slot) || req.seq == 0 {
                return Err(CodecError::InvalidValue);
            }
            let mut out = Vec::with_capacity(1 + 13);
            out.push(TAG_EQUIP);
            out.extend_from_slice(&req.seq.to_le_bytes());
            out.push(req.slot);
            out.extend_from_slice(&req.item_instance_id.raw().to_le_bytes());
            Ok(out)
        }
        ClientControl::Unequip(req) => {
            if !slot_valid(req.slot) || req.seq == 0 {
                return Err(CodecError::InvalidValue);
            }
            let mut out = Vec::with_capacity(1 + 5);
            out.push(TAG_UNEQUIP);
            out.extend_from_slice(&req.seq.to_le_bytes());
            out.push(req.slot);
            Ok(out)
        }
        ClientControl::DevPresentationOneShot(req) => {
            if !DevPresentationOneShot::kind_valid(req.kind) {
                return Err(CodecError::InvalidValue);
            }
            Ok(encode_dev_presentation_oneshot(req))
        }
        ClientControl::DevResetPlayer => Ok(vec![TAG_DEV_RESET_PLAYER]),
        ClientControl::AbilityActivate(req) => encode_ability_activate(*req),
        ClientControl::Respawn => Ok(vec![TAG_RESPAWN]),
        ClientControl::Pickup(req) => {
            if req.seq == 0 {
                return Err(CodecError::InvalidValue);
            }
            let mut out = Vec::with_capacity(1 + 4 + 8);
            out.push(TAG_PICKUP);
            out.extend_from_slice(&req.seq.to_le_bytes());
            write_wire_entity(&mut out, req.target);
            Ok(out)
        }
    }
}

pub fn decode_client_control(bytes: &[u8]) -> Result<ClientControl, CodecError> {
    let (tag, rest) = split_tag(bytes)?;
    match tag {
        TAG_HELLO => {
            let (protocol_version, rest) = read_u32(rest)?;
            let (client_build, rest) = read_bounded_string(rest)?;
            let (dev_login, rest) = if protocol_version >= HELLO_DEV_LOGIN_SINCE {
                read_bounded_string(rest)?
            } else {
                (String::new(), rest)
            };
            expect_empty(rest)?;
            Ok(ClientControl::Hello(Hello {
                protocol_version,
                client_build,
                dev_login,
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
            let trailer = &rest[5..];
            let input_trailer = if trailer.is_empty() {
                0
            } else if trailer.len() == 1 && trailer[0] <= 1 {
                trailer[0]
            } else {
                return Err(CodecError::InvalidValue);
            };
            let portal_held = input_trailer & 1 != 0;
            Ok(ClientControl::Input(InputCommand {
                input_epoch,
                sequence,
                move_axis,
                jump_pressed,
                down_held,
                portal_held,
            }))
        }
        TAG_HELD_CANCEL => {
            expect_empty(rest)?;
            Ok(ClientControl::HeldCancel)
        }
        TAG_INTERACT_OPEN => {
            let (target, rest) = read_wire_entity(rest)?;
            expect_empty(rest)?;
            Ok(ClientControl::InteractOpen(InteractOpen { target }))
        }
        TAG_INTERACT_CLOSE => {
            let (session_id, rest) = read_u32(rest)?;
            expect_empty(rest)?;
            Ok(ClientControl::InteractClose(InteractClose { session_id }))
        }
        TAG_DIALOGUE_ADVANCE => {
            let (session_id, rest) = read_u32(rest)?;
            expect_empty(rest)?;
            if session_id == 0 {
                return Err(CodecError::InvalidValue);
            }
            Ok(ClientControl::DialogueAdvance(DialogueAdvance {
                session_id,
            }))
        }
        TAG_DIALOGUE_CHOOSE => {
            let (session_id, rest) = read_u32(rest)?;
            let (beat_index, rest) = read_u32(rest)?;
            let (choice_index, rest) = read_u32(rest)?;
            expect_empty(rest)?;
            if session_id == 0 {
                return Err(CodecError::InvalidValue);
            }
            Ok(ClientControl::DialogueChoose(DialogueChoose {
                session_id,
                beat_index,
                choice_index,
            }))
        }
        TAG_PORTAL_ACTIVATE => {
            let (target, rest) = read_wire_entity(rest)?;
            expect_empty(rest)?;
            Ok(ClientControl::PortalActivate(PortalActivate { target }))
        }
        TAG_DEV_SET_CHANNEL => {
            let (channel, rest) = read_u32(rest)?;
            expect_empty(rest)?;
            Ok(ClientControl::DevSetChannel(DevSetChannel { channel }))
        }
        TAG_DEV_SET_SPEED => {
            let Some((&present, rest)) = rest.split_first() else {
                return Err(CodecError::Truncated);
            };
            let speed = match present {
                0 => {
                    expect_empty(rest)?;
                    None
                }
                1 => {
                    if rest.len() != 2 {
                        return Err(if rest.len() < 2 {
                            CodecError::Truncated
                        } else {
                            CodecError::InvalidValue
                        });
                    }
                    Some(u16::from_le_bytes(
                        rest.try_into().map_err(|_| CodecError::Truncated)?,
                    ))
                }
                _ => return Err(CodecError::InvalidValue),
            };
            Ok(ClientControl::DevSetSpeed(DevSetSpeed { speed }))
        }
        TAG_DEV_SET_JUMP => {
            let Some((&present, rest)) = rest.split_first() else {
                return Err(CodecError::Truncated);
            };
            let jump = match present {
                0 => {
                    expect_empty(rest)?;
                    None
                }
                1 => {
                    if rest.len() != 2 {
                        return Err(if rest.len() < 2 {
                            CodecError::Truncated
                        } else {
                            CodecError::InvalidValue
                        });
                    }
                    Some(u16::from_le_bytes(
                        rest.try_into().map_err(|_| CodecError::Truncated)?,
                    ))
                }
                _ => return Err(CodecError::InvalidValue),
            };
            Ok(ClientControl::DevSetJump(DevSetJump { jump }))
        }
        TAG_DEV_SPAWN_NPC => {
            let (npc_content_id, rest) = read_u32(rest)?;
            expect_empty(rest)?;
            let npc_content_id = purgatory_common::ContentId::from_raw(npc_content_id);
            if npc_content_id.kind() != Some(purgatory_common::ContentKind::Npc) {
                return Err(CodecError::InvalidValue);
            }
            Ok(ClientControl::DevSpawnNpc(DevSpawnNpc { npc_content_id }))
        }
        TAG_EQUIP => {
            let (seq, rest) = read_u32(rest)?;
            if rest.len() != 9 {
                return Err(if rest.len() < 9 {
                    CodecError::Truncated
                } else {
                    CodecError::InvalidValue
                });
            }
            let slot = rest[0];
            if !slot_valid(slot) || seq == 0 {
                return Err(CodecError::InvalidValue);
            }
            let item_instance_id =
                u64::from_le_bytes(rest[1..9].try_into().map_err(|_| CodecError::Truncated)?);
            Ok(ClientControl::Equip(EquipRequest {
                seq,
                slot,
                item_instance_id: purgatory_common::ItemInstanceId::from_raw(item_instance_id),
            }))
        }
        TAG_UNEQUIP => {
            let (seq, rest) = read_u32(rest)?;
            if rest.len() != 1 {
                return Err(if rest.is_empty() {
                    CodecError::Truncated
                } else {
                    CodecError::InvalidValue
                });
            }
            let slot = rest[0];
            if !slot_valid(slot) || seq == 0 {
                return Err(CodecError::InvalidValue);
            }
            Ok(ClientControl::Unequip(UnequipRequest { seq, slot }))
        }
        TAG_DEV_PRESENTATION_ONESHOT => {
            let req = decode_dev_presentation_oneshot(rest)?;
            if !DevPresentationOneShot::kind_valid(req.kind) {
                return Err(CodecError::InvalidValue);
            }
            Ok(ClientControl::DevPresentationOneShot(req))
        }
        TAG_DEV_RESET_PLAYER => {
            expect_empty(rest)?;
            Ok(ClientControl::DevResetPlayer)
        }
        TAG_ABILITY_ACTIVATE => Ok(ClientControl::AbilityActivate(decode_ability_activate(
            rest,
        )?)),
        TAG_RESPAWN => {
            expect_empty(rest)?;
            Ok(ClientControl::Respawn)
        }
        TAG_PICKUP => {
            let (seq, rest) = read_u32(rest)?;
            let (target, rest) = read_wire_entity(rest)?;
            expect_empty(rest)?;
            if seq == 0 {
                return Err(CodecError::InvalidValue);
            }
            Ok(ClientControl::Pickup(PickupRequest { seq, target }))
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
        ServerControl::Interact(event) => encode_server_interact(event),
        ServerControl::DialogueLine(event) => encode_server_dialogue_line(*event),
        ServerControl::DialogueChoiceAccepted(event) => {
            if event.session_id == 0 {
                return Err(CodecError::InvalidValue);
            }
            let mut out = Vec::with_capacity(1 + crate::DIALOGUE_CHOICE_ACCEPTED_BYTES);
            out.push(TAG_DIALOGUE_CHOICE_ACCEPTED);
            out.extend_from_slice(&event.session_id.to_le_bytes());
            out.extend_from_slice(&event.beat_index.to_le_bytes());
            out.extend_from_slice(&event.choice_index.to_le_bytes());
            Ok(out)
        }
        ServerControl::Equipment(event) => encode_server_equipment(event),
        ServerControl::PresentationOneShot(event) => Ok(encode_server_presentation_oneshot(event)),
        ServerControl::Ability(event) => encode_server_ability(event),
        ServerControl::Item(event) => encode_server_item(event),
        ServerControl::Inventory(snapshot) => encode_server_inventory(snapshot),
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
        TAG_INTERACT_OPENED
        | TAG_INTERACT_REJECTED
        | TAG_INTERACT_UPDATED
        | TAG_INTERACT_CLOSED => Ok(ServerControl::Interact(decode_server_interact(tag, rest)?)),
        TAG_DIALOGUE_ACTIVE_LINE => Ok(ServerControl::DialogueLine(decode_server_dialogue_line(
            rest,
        )?)),
        TAG_DIALOGUE_CHOICE_ACCEPTED => {
            let (session_id, rest) = read_u32(rest)?;
            let (beat_index, rest) = read_u32(rest)?;
            let (choice_index, rest) = read_u32(rest)?;
            expect_empty(rest)?;
            if session_id == 0 {
                return Err(CodecError::InvalidValue);
            }
            Ok(ServerControl::DialogueChoiceAccepted(
                ServerDialogueChoiceAccepted {
                    session_id,
                    beat_index,
                    choice_index,
                },
            ))
        }
        TAG_EQUIPMENT_ACCEPTED | TAG_EQUIPMENT_REJECTED => Ok(ServerControl::Equipment(
            decode_server_equipment(tag, rest)?,
        )),
        TAG_SERVER_PRESENTATION_ONESHOT => Ok(ServerControl::PresentationOneShot(
            decode_server_presentation_oneshot(rest)?,
        )),
        TAG_ABILITY_ACCEPTED | TAG_ABILITY_REJECTED => {
            Ok(ServerControl::Ability(decode_server_ability(tag, rest)?))
        }
        TAG_PICKUP_ACCEPTED | TAG_PICKUP_REJECTED => {
            Ok(ServerControl::Item(decode_server_item(tag, rest)?))
        }
        TAG_INVENTORY_SNAPSHOT => Ok(ServerControl::Inventory(decode_server_inventory(rest)?)),
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

fn write_wire_entity(out: &mut Vec<u8>, id: WireEntityId) {
    out.extend_from_slice(&id.index.to_le_bytes());
    out.extend_from_slice(&id.generation.to_le_bytes());
}

fn read_wire_entity(bytes: &[u8]) -> Result<(WireEntityId, &[u8]), CodecError> {
    let (index, rest) = read_u32(bytes)?;
    let (generation, rest) = read_u32(rest)?;
    Ok((WireEntityId { index, generation }, rest))
}

fn encode_server_interact(event: &ServerInteract) -> Result<Vec<u8>, CodecError> {
    match *event {
        ServerInteract::Opened { session_id, target } => {
            let mut out = Vec::with_capacity(1 + 4 + 8);
            out.push(TAG_INTERACT_OPENED);
            out.extend_from_slice(&session_id.to_le_bytes());
            write_wire_entity(&mut out, target);
            Ok(out)
        }
        ServerInteract::Rejected { target, reason } => {
            let mut out = Vec::with_capacity(1 + 8 + 1);
            out.push(TAG_INTERACT_REJECTED);
            write_wire_entity(&mut out, target);
            out.push(reason.as_u8());
            Ok(out)
        }
        ServerInteract::Updated { session_id, target } => {
            let mut out = Vec::with_capacity(1 + 4 + 8);
            out.push(TAG_INTERACT_UPDATED);
            out.extend_from_slice(&session_id.to_le_bytes());
            write_wire_entity(&mut out, target);
            Ok(out)
        }
        ServerInteract::Closed { session_id, reason } => {
            let mut out = Vec::with_capacity(1 + 4 + 1);
            out.push(TAG_INTERACT_CLOSED);
            out.extend_from_slice(&session_id.to_le_bytes());
            out.push(reason.as_u8());
            Ok(out)
        }
    }
}

fn decode_server_interact(tag: u8, rest: &[u8]) -> Result<ServerInteract, CodecError> {
    match tag {
        TAG_INTERACT_OPENED => {
            let (session_id, rest) = read_u32(rest)?;
            let (target, rest) = read_wire_entity(rest)?;
            expect_empty(rest)?;
            Ok(ServerInteract::Opened { session_id, target })
        }
        TAG_INTERACT_REJECTED => {
            let (target, rest) = read_wire_entity(rest)?;
            if rest.is_empty() {
                return Err(CodecError::Truncated);
            }
            let reason = crate::interact::InteractRejectReason::from_u8(rest[0])
                .ok_or(CodecError::InvalidValue)?;
            expect_empty(&rest[1..])?;
            Ok(ServerInteract::Rejected { target, reason })
        }
        TAG_INTERACT_UPDATED => {
            let (session_id, rest) = read_u32(rest)?;
            let (target, rest) = read_wire_entity(rest)?;
            expect_empty(rest)?;
            Ok(ServerInteract::Updated { session_id, target })
        }
        TAG_INTERACT_CLOSED => {
            let (session_id, rest) = read_u32(rest)?;
            if rest.is_empty() {
                return Err(CodecError::Truncated);
            }
            let reason = crate::interact::InteractCloseReason::from_u8(rest[0])
                .ok_or(CodecError::InvalidValue)?;
            expect_empty(&rest[1..])?;
            Ok(ServerInteract::Closed { session_id, reason })
        }
        other => Err(CodecError::UnknownDiscriminant(other)),
    }
}

fn encode_server_dialogue_line(event: ServerDialogueLine) -> Result<Vec<u8>, CodecError> {
    if event.session_id == 0
        || event.npc_content_id.kind() != Some(purgatory_common::ContentKind::Npc)
    {
        return Err(CodecError::InvalidValue);
    }
    let npc_content_id = event.npc_content_id.raw().ok_or(CodecError::InvalidValue)?;
    let mut out = Vec::with_capacity(1 + crate::DIALOGUE_ACTIVE_LINE_BYTES);
    out.push(TAG_DIALOGUE_ACTIVE_LINE);
    out.extend_from_slice(&event.session_id.to_le_bytes());
    write_wire_entity(&mut out, event.target);
    out.extend_from_slice(&npc_content_id.to_le_bytes());
    out.extend_from_slice(&event.beat_index.to_le_bytes());
    out.extend_from_slice(&event.line_index.to_le_bytes());
    Ok(out)
}

fn decode_server_dialogue_line(rest: &[u8]) -> Result<ServerDialogueLine, CodecError> {
    let (session_id, rest) = read_u32(rest)?;
    let (target, rest) = read_wire_entity(rest)?;
    let (npc_content_id, rest) = read_u32(rest)?;
    let (beat_index, rest) = read_u32(rest)?;
    let (line_index, rest) = read_u32(rest)?;
    expect_empty(rest)?;
    let npc_content_id = purgatory_common::ContentId::from_raw(npc_content_id);
    if session_id == 0 || npc_content_id.kind() != Some(purgatory_common::ContentKind::Npc) {
        return Err(CodecError::InvalidValue);
    }
    Ok(ServerDialogueLine {
        session_id,
        target,
        npc_content_id,
        beat_index,
        line_index,
    })
}

fn encode_server_equipment(event: &ServerEquipment) -> Result<Vec<u8>, CodecError> {
    match *event {
        ServerEquipment::Accepted { seq } => {
            let mut out = Vec::with_capacity(1 + 4);
            out.push(TAG_EQUIPMENT_ACCEPTED);
            out.extend_from_slice(&seq.to_le_bytes());
            Ok(out)
        }
        ServerEquipment::Rejected { seq, reason } => {
            let mut out = Vec::with_capacity(1 + 4 + 1);
            out.push(TAG_EQUIPMENT_REJECTED);
            out.extend_from_slice(&seq.to_le_bytes());
            out.push(reason.as_u8());
            Ok(out)
        }
    }
}

fn decode_server_equipment(tag: u8, rest: &[u8]) -> Result<ServerEquipment, CodecError> {
    match tag {
        TAG_EQUIPMENT_ACCEPTED => {
            let (seq, rest) = read_u32(rest)?;
            expect_empty(rest)?;
            Ok(ServerEquipment::Accepted { seq })
        }
        TAG_EQUIPMENT_REJECTED => {
            let (seq, rest) = read_u32(rest)?;
            if rest.is_empty() {
                return Err(CodecError::Truncated);
            }
            let reason = EquipmentRejectReason::from_u8(rest[0]).ok_or(CodecError::InvalidValue)?;
            expect_empty(&rest[1..])?;
            Ok(ServerEquipment::Rejected { seq, reason })
        }
        other => Err(CodecError::UnknownDiscriminant(other)),
    }
}

fn encode_server_item(event: &ServerItem) -> Result<Vec<u8>, CodecError> {
    match *event {
        ServerItem::PickupAccepted {
            seq,
            item_instance_id,
            slot,
        } => {
            let mut out = Vec::with_capacity(1 + 4 + 8 + 2);
            out.push(TAG_PICKUP_ACCEPTED);
            out.extend_from_slice(&seq.to_le_bytes());
            out.extend_from_slice(&item_instance_id.raw().to_le_bytes());
            out.extend_from_slice(&slot.to_le_bytes());
            Ok(out)
        }
        ServerItem::PickupRejected { seq, reason } => {
            let mut out = Vec::with_capacity(1 + 4 + 1);
            out.push(TAG_PICKUP_REJECTED);
            out.extend_from_slice(&seq.to_le_bytes());
            out.push(reason.as_u8());
            Ok(out)
        }
    }
}

fn decode_server_item(tag: u8, rest: &[u8]) -> Result<ServerItem, CodecError> {
    let (seq, rest) = read_u32(rest)?;
    if seq == 0 {
        return Err(CodecError::InvalidValue);
    }
    match tag {
        TAG_PICKUP_ACCEPTED => {
            if rest.len() != 10 {
                return Err(if rest.len() < 10 {
                    CodecError::Truncated
                } else {
                    CodecError::InvalidValue
                });
            }
            let item_instance_id = decode_item_instance_id(&rest[..8])?;
            let slot =
                u16::from_le_bytes(rest[8..10].try_into().map_err(|_| CodecError::Truncated)?);
            Ok(ServerItem::PickupAccepted {
                seq,
                item_instance_id,
                slot,
            })
        }
        TAG_PICKUP_REJECTED => {
            if rest.is_empty() {
                return Err(CodecError::Truncated);
            }
            let reason = PickupRejectReason::from_u8(rest[0]).ok_or(CodecError::InvalidValue)?;
            expect_empty(&rest[1..])?;
            Ok(ServerItem::PickupRejected { seq, reason })
        }
        other => Err(CodecError::UnknownDiscriminant(other)),
    }
}

fn encode_server_inventory(snapshot: &ServerInventory) -> Result<Vec<u8>, CodecError> {
    if snapshot.entries.len() > INVENTORY_CAPACITY {
        return Err(CodecError::InvalidValue);
    }
    let mut out = Vec::with_capacity(2 + snapshot.entries.len() * 22);
    out.push(TAG_INVENTORY_SNAPSHOT);
    out.push(u8::try_from(snapshot.entries.len()).map_err(|_| CodecError::InvalidValue)?);
    for entry in &snapshot.entries {
        if usize::from(entry.slot) >= INVENTORY_CAPACITY || entry.quantity == 0 {
            return Err(CodecError::InvalidValue);
        }
        out.extend_from_slice(&entry.slot.to_le_bytes());
        out.extend_from_slice(&entry.item_instance_id.raw().to_le_bytes());
        out.extend_from_slice(&entry.definition.token().to_le_bytes());
        out.extend_from_slice(&entry.quantity.to_le_bytes());
    }
    Ok(out)
}

fn decode_server_inventory(rest: &[u8]) -> Result<ServerInventory, CodecError> {
    let Some(&count) = rest.first() else {
        return Err(CodecError::Truncated);
    };
    let count = usize::from(count);
    if count > INVENTORY_CAPACITY || rest.len() != 1 + count * 22 {
        return Err(CodecError::InvalidValue);
    }
    let mut entries = Vec::with_capacity(count);
    for chunk in rest[1..].chunks(22) {
        let entry = decode_inventory_entry(chunk)?;
        if usize::from(entry.slot) >= INVENTORY_CAPACITY || entry.quantity == 0 {
            return Err(CodecError::InvalidValue);
        }
        if entries.iter().any(|previous: &InventoryEntry| {
            previous.slot == entry.slot || previous.item_instance_id == entry.item_instance_id
        }) {
            return Err(CodecError::InvalidValue);
        }
        entries.push(entry);
    }
    Ok(ServerInventory { entries })
}

fn encode_ability_activate(req: AbilityActivateRequest) -> Result<Vec<u8>, CodecError> {
    if req.seq == 0 {
        return Err(CodecError::InvalidValue);
    }
    let cap = if req.selected.is_some() {
        1 + ABILITY_ACTIVATE_SELECTED_BYTES
    } else {
        1 + ABILITY_ACTIVATE_INDEPENDENT_BYTES
    };
    let mut out = Vec::with_capacity(cap);
    out.push(TAG_ABILITY_ACTIVATE);
    out.extend_from_slice(&req.seq.to_le_bytes());
    out.extend_from_slice(&req.ability_id.token().to_le_bytes());
    match req.selected {
        None => out.push(0),
        Some(id) => {
            out.push(1);
            write_wire_entity(&mut out, id);
        }
    }
    Ok(out)
}

fn decode_ability_activate(rest: &[u8]) -> Result<AbilityActivateRequest, CodecError> {
    let (seq, rest) = read_u32(rest)?;
    if seq == 0 {
        return Err(CodecError::InvalidValue);
    }
    let (token, rest) = read_u64(rest)?;
    if rest.is_empty() {
        return Err(CodecError::Truncated);
    }
    let flag = rest[0];
    let rest = &rest[1..];
    let selected = match flag {
        0 => {
            expect_empty(rest)?;
            None
        }
        1 => {
            let (id, rest) = read_wire_entity(rest)?;
            expect_empty(rest)?;
            Some(id)
        }
        _ => return Err(CodecError::InvalidValue),
    };
    Ok(AbilityActivateRequest {
        seq,
        ability_id: purgatory_common::ContentId::from_token(token),
        selected,
    })
}

fn encode_server_ability(event: &ServerAbility) -> Result<Vec<u8>, CodecError> {
    match *event {
        ServerAbility::Accepted { seq } => {
            let mut out = Vec::with_capacity(1 + 4);
            out.push(TAG_ABILITY_ACCEPTED);
            out.extend_from_slice(&seq.to_le_bytes());
            Ok(out)
        }
        ServerAbility::Rejected { seq, reason } => {
            let mut out = Vec::with_capacity(1 + 5);
            out.push(TAG_ABILITY_REJECTED);
            out.extend_from_slice(&seq.to_le_bytes());
            out.push(reason.as_u8());
            Ok(out)
        }
    }
}

fn decode_server_ability(tag: u8, rest: &[u8]) -> Result<ServerAbility, CodecError> {
    match tag {
        TAG_ABILITY_ACCEPTED => {
            let (seq, rest) = read_u32(rest)?;
            expect_empty(rest)?;
            Ok(ServerAbility::Accepted { seq })
        }
        TAG_ABILITY_REJECTED => {
            let (seq, rest) = read_u32(rest)?;
            if rest.is_empty() {
                return Err(CodecError::Truncated);
            }
            let reason = AbilityCommandReject::from_u8(rest[0]).ok_or(CodecError::InvalidValue)?;
            expect_empty(&rest[1..])?;
            Ok(ServerAbility::Rejected { seq, reason })
        }
        other => Err(CodecError::UnknownDiscriminant(other)),
    }
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
            dev_login: purgatory_common::DEFAULT_DEV_LOGIN.into(),
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
            portal_held: false,
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
    fn portal_held_trailer_roundtrips_and_absence_is_false() {
        let mut held = input_dev();
        held.portal_held = true;
        let encoded = encode_client_control(&ClientControl::Input(held)).unwrap();
        assert_eq!(encoded.len(), 1 + 4 + 2 + 1 + 1 + 1 + 1);
        assert_eq!(encoded[encoded.len() - 1], 1);
        assert_eq!(
            decode_client_control(&encoded).unwrap(),
            ClientControl::Input(held)
        );
        let omitted = encode_client_control(&ClientControl::Input(input_dev())).unwrap();
        match decode_client_control(&omitted).unwrap() {
            ClientControl::Input(cmd) => assert!(!cmd.portal_held),
            other => panic!("{other:?}"),
        }
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
            dev_login: String::new(),
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
            dev_login: String::new(),
        };
        let err = validate_hello(&hello).expect_err("v1");
        assert_eq!(err.code, DisconnectReasonCode::VersionMismatch);
        assert_ne!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn malformed_dev_login_is_rejected_after_version_ok() {
        let hello = Hello {
            protocol_version: PROTOCOL_VERSION,
            client_build: "dev".into(),
            dev_login: "A".into(),
        };
        let err = validate_hello(&hello).expect_err("login");
        assert_eq!(err.code, DisconnectReasonCode::Malformed);
    }

    #[test]
    fn v9_hello_bytes_decode_then_version_mismatch() {
        let encoded = encode_client_control(&ClientControl::Hello(Hello {
            protocol_version: 9,
            client_build: "test".into(),
            dev_login: String::new(),
        }))
        .unwrap();
        let ClientControl::Hello(hello) = decode_client_control(&encoded).unwrap() else {
            panic!("hello");
        };
        assert_eq!(hello.dev_login, "");
        let err = validate_hello(&hello).expect_err("v9");
        assert_eq!(err.code, DisconnectReasonCode::VersionMismatch);
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
    fn interact_open_roundtrip() {
        let msg = ClientControl::InteractOpen(InteractOpen {
            target: WireEntityId {
                index: 42,
                generation: 3,
            },
        });
        let encoded = encode_client_control(&msg).unwrap();
        assert_eq!(encoded[0], TAG_INTERACT_OPEN);
        assert_eq!(decode_client_control(&encoded).unwrap(), msg);
    }

    #[test]
    fn dialogue_advance_roundtrip_and_rejects_invalid_sessions() {
        let msg = ClientControl::DialogueAdvance(DialogueAdvance { session_id: 7 });
        let encoded = encode_client_control(&msg).unwrap();
        assert_eq!(encoded, [TAG_DIALOGUE_ADVANCE, 7, 0, 0, 0]);
        assert_eq!(decode_client_control(&encoded).unwrap(), msg);

        assert_eq!(
            encode_client_control(&ClientControl::DialogueAdvance(DialogueAdvance {
                session_id: 0,
            })),
            Err(CodecError::InvalidValue)
        );
        assert_eq!(
            decode_client_control(&[TAG_DIALOGUE_ADVANCE, 0, 0, 0, 0]),
            Err(CodecError::InvalidValue)
        );
        assert_eq!(
            decode_client_control(&[TAG_DIALOGUE_ADVANCE, 7, 0, 0]),
            Err(CodecError::Truncated)
        );
    }

    #[test]
    fn dialogue_choice_roundtrips_and_rejects_invalid_sessions() {
        let msg = ClientControl::DialogueChoose(DialogueChoose {
            session_id: 7,
            beat_index: 2,
            choice_index: 1,
        });
        let encoded = encode_client_control(&msg).unwrap();
        assert_eq!(encoded[0], TAG_DIALOGUE_CHOOSE);
        assert_eq!(decode_client_control(&encoded).unwrap(), msg);
        assert_eq!(
            encode_client_control(&ClientControl::DialogueChoose(DialogueChoose {
                session_id: 0,
                beat_index: 0,
                choice_index: 0,
            })),
            Err(CodecError::InvalidValue)
        );
    }

    #[test]
    fn portal_activate_roundtrip() {
        let msg = ClientControl::PortalActivate(PortalActivate {
            target: WireEntityId {
                index: 11,
                generation: 2,
            },
        });
        let encoded = encode_client_control(&msg).unwrap();
        assert_eq!(encoded[0], TAG_PORTAL_ACTIVATE);
        assert_eq!(decode_client_control(&encoded).unwrap(), msg);
    }

    #[test]
    fn dev_set_channel_roundtrip() {
        let msg = ClientControl::DevSetChannel(DevSetChannel { channel: 1 });
        let encoded = encode_client_control(&msg).unwrap();
        assert_eq!(encoded[0], TAG_DEV_SET_CHANNEL);
        assert_eq!(decode_client_control(&encoded).unwrap(), msg);
    }

    #[test]
    fn dev_set_speed_roundtrip_and_reset() {
        for speed in [Some(1_200), None] {
            let msg = ClientControl::DevSetSpeed(DevSetSpeed { speed });
            let encoded = encode_client_control(&msg).unwrap();
            assert_eq!(encoded[0], TAG_DEV_SET_SPEED);
            assert_eq!(decode_client_control(&encoded).unwrap(), msg);
        }
    }

    #[test]
    fn dev_set_speed_rejects_invalid_presence_and_length() {
        assert_eq!(
            decode_client_control(&[TAG_DEV_SET_SPEED, 2]),
            Err(CodecError::InvalidValue)
        );
        assert_eq!(
            decode_client_control(&[TAG_DEV_SET_SPEED, 1, 0]),
            Err(CodecError::Truncated)
        );
    }

    #[test]
    fn dev_set_jump_roundtrip_and_reset() {
        for jump in [Some(2_000), None] {
            let msg = ClientControl::DevSetJump(DevSetJump { jump });
            let encoded = encode_client_control(&msg).unwrap();
            assert_eq!(encoded[0], TAG_DEV_SET_JUMP);
            assert_eq!(decode_client_control(&encoded).unwrap(), msg);
        }
    }

    #[test]
    fn dev_set_jump_rejects_invalid_presence_and_length() {
        assert_eq!(
            decode_client_control(&[TAG_DEV_SET_JUMP, 2]),
            Err(CodecError::InvalidValue)
        );
        assert_eq!(
            decode_client_control(&[TAG_DEV_SET_JUMP, 1, 0]),
            Err(CodecError::Truncated)
        );
    }

    #[test]
    fn dev_spawn_npc_roundtrip_and_rejects_non_npc_identity() {
        let msg = ClientControl::DevSpawnNpc(DevSpawnNpc {
            npc_content_id: purgatory_common::ContentId::from_raw(20_001),
        });
        let encoded = encode_client_control(&msg).unwrap();
        assert_eq!(encoded, [TAG_DEV_SPAWN_NPC, 0x21, 0x4e, 0, 0]);
        assert_eq!(decode_client_control(&encoded).unwrap(), msg);
        assert_eq!(
            encode_client_control(&ClientControl::DevSpawnNpc(DevSpawnNpc {
                npc_content_id: purgatory_common::ContentId::from_raw(30_001),
            })),
            Err(CodecError::InvalidValue)
        );
        assert_eq!(
            decode_client_control(&[TAG_DEV_SPAWN_NPC, 0x31, 0x75, 0, 0]),
            Err(CodecError::InvalidValue)
        );
    }

    #[test]
    fn dev_reset_player_roundtrip() {
        let encoded = encode_client_control(&ClientControl::DevResetPlayer).unwrap();
        assert_eq!(encoded, [TAG_DEV_RESET_PLAYER]);
        assert_eq!(
            decode_client_control(&encoded).unwrap(),
            ClientControl::DevResetPlayer
        );
    }

    #[test]
    fn respawn_roundtrip() {
        let encoded = encode_client_control(&ClientControl::Respawn).unwrap();
        assert_eq!(encoded, [TAG_RESPAWN]);
        assert_eq!(
            decode_client_control(&encoded).unwrap(),
            ClientControl::Respawn
        );
    }

    #[test]
    fn interact_rejected_server_roundtrip() {
        let msg = ServerControl::Interact(ServerInteract::Rejected {
            target: WireEntityId {
                index: 42,
                generation: 3,
            },
            reason: crate::InteractRejectReason::OutOfRange,
        });
        let encoded = encode_server_control(&msg).unwrap();
        assert_eq!(decode_server_control(&encoded).unwrap(), msg);
    }

    #[test]
    fn dialogue_line_roundtrip_and_rejects_non_npc_identity() {
        let msg = ServerControl::DialogueLine(ServerDialogueLine {
            session_id: 7,
            target: WireEntityId {
                index: 42,
                generation: 3,
            },
            npc_content_id: purgatory_common::ContentId::from_raw(20_001),
            beat_index: 2,
            line_index: 1,
        });
        let encoded = encode_server_control(&msg).unwrap();
        assert_eq!(encoded[0], TAG_DIALOGUE_ACTIVE_LINE);
        assert_eq!(decode_server_control(&encoded).unwrap(), msg);

        let invalid = ServerControl::DialogueLine(ServerDialogueLine {
            session_id: 7,
            target: WireEntityId {
                index: 42,
                generation: 3,
            },
            npc_content_id: purgatory_common::ContentId::from_raw(1),
            beat_index: 0,
            line_index: 0,
        });
        assert_eq!(
            encode_server_control(&invalid),
            Err(CodecError::InvalidValue)
        );
    }

    #[test]
    fn dialogue_choice_accepted_roundtrips() {
        let msg = ServerControl::DialogueChoiceAccepted(ServerDialogueChoiceAccepted {
            session_id: 7,
            beat_index: 2,
            choice_index: 1,
        });
        let encoded = encode_server_control(&msg).unwrap();
        assert_eq!(encoded[0], TAG_DIALOGUE_CHOICE_ACCEPTED);
        assert_eq!(decode_server_control(&encoded).unwrap(), msg);
    }

    #[test]
    fn old_protocol_version_2_is_rejected() {
        let hello = Hello {
            protocol_version: 2,
            client_build: "legacy-v2".into(),
            dev_login: String::new(),
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
            dev_login: String::new(),
        };
        let err = validate_hello(&hello).expect_err("v3");
        assert_eq!(err.code, DisconnectReasonCode::VersionMismatch);
        assert_ne!(PROTOCOL_VERSION, 3);
    }

    #[test]
    fn old_protocol_version_4_is_rejected() {
        let hello = Hello {
            protocol_version: 4,
            client_build: "legacy-v4".into(),
            dev_login: String::new(),
        };
        let err = validate_hello(&hello).expect_err("v4");
        assert_eq!(err.code, DisconnectReasonCode::VersionMismatch);
        assert_ne!(PROTOCOL_VERSION, 4);
    }

    #[test]
    fn old_protocol_version_5_is_rejected() {
        let hello = Hello {
            protocol_version: 5,
            client_build: "legacy-v5".into(),
            dev_login: String::new(),
        };
        let err = validate_hello(&hello).expect_err("v5");
        assert_eq!(err.code, DisconnectReasonCode::VersionMismatch);
        assert_ne!(PROTOCOL_VERSION, 5);
        assert_ne!(PROTOCOL_VERSION, 7);
    }

    #[test]
    fn string_too_long_rejected() {
        let hello = Hello {
            protocol_version: PROTOCOL_VERSION,
            client_build: "x".repeat(MAX_LABEL_BYTES + 1),
            dev_login: String::new(),
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
        // tag + u32 version + u8 strlen + "dev" + u8 strlen + "dev.local"
        assert_eq!(encoded.len(), 1 + 4 + 1 + 3 + 1 + 9);
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
            local_map: 1,
            local_channel: 0,
            local_instance: 0,
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

    #[test]
    fn equipment_request_and_result_roundtrip_and_sizes() {
        let equip = ClientControl::Equip(EquipRequest {
            seq: 1,
            slot: 5,
            item_instance_id: purgatory_common::ItemInstanceId::from_raw(42),
        });
        let encoded = encode_client_control(&equip).unwrap();
        assert_eq!(encoded.len(), 1 + crate::EQUIP_REQUEST_BYTES);
        assert_eq!(decode_client_control(&encoded).unwrap(), equip);

        let unequip = ClientControl::Unequip(UnequipRequest { seq: 2, slot: 5 });
        let encoded = encode_client_control(&unequip).unwrap();
        assert_eq!(encoded.len(), 1 + crate::UNEQUIP_REQUEST_BYTES);
        assert_eq!(decode_client_control(&encoded).unwrap(), unequip);

        let accepted = ServerControl::Equipment(ServerEquipment::Accepted { seq: 1 });
        let encoded = encode_server_control(&accepted).unwrap();
        assert_eq!(encoded.len(), 1 + crate::EQUIPMENT_ACCEPTED_BYTES);
        assert_eq!(decode_server_control(&encoded).unwrap(), accepted);

        let rejected = ServerControl::Equipment(ServerEquipment::Rejected {
            seq: 3,
            reason: EquipmentRejectReason::SlotMismatch,
        });
        let encoded = encode_server_control(&rejected).unwrap();
        assert_eq!(encoded.len(), 1 + crate::EQUIPMENT_REJECTED_BYTES);
        assert_eq!(decode_server_control(&encoded).unwrap(), rejected);
    }

    #[test]
    fn pickup_request_and_result_roundtrip_and_sizes() {
        let pickup = ClientControl::Pickup(PickupRequest {
            seq: 4,
            target: WireEntityId {
                index: 7,
                generation: 3,
            },
        });
        let encoded = encode_client_control(&pickup).unwrap();
        assert_eq!(encoded.len(), 1 + crate::PICKUP_REQUEST_BYTES);
        assert_eq!(decode_client_control(&encoded).unwrap(), pickup);

        let accepted = ServerControl::Item(ServerItem::PickupAccepted {
            seq: 4,
            item_instance_id: purgatory_common::ItemInstanceId::from_raw(0x1234),
            slot: 2,
        });
        let encoded = encode_server_control(&accepted).unwrap();
        assert_eq!(encoded.len(), 1 + crate::PICKUP_ACCEPTED_BYTES);
        assert_eq!(decode_server_control(&encoded).unwrap(), accepted);

        let rejected = ServerControl::Item(ServerItem::PickupRejected {
            seq: 5,
            reason: PickupRejectReason::InventoryFull,
        });
        let encoded = encode_server_control(&rejected).unwrap();
        assert_eq!(encoded.len(), 1 + crate::PICKUP_REJECTED_BYTES);
        assert_eq!(decode_server_control(&encoded).unwrap(), rejected);
    }

    #[test]
    fn ability_activate_roundtrip_and_sizes() {
        let independent = ClientControl::AbilityActivate(AbilityActivateRequest {
            seq: 1,
            ability_id: purgatory_common::ContentId::from_token(9),
            selected: None,
        });
        let encoded = encode_client_control(&independent).unwrap();
        assert_eq!(encoded.len(), 1 + crate::ABILITY_ACTIVATE_INDEPENDENT_BYTES);
        assert_eq!(decode_client_control(&encoded).unwrap(), independent);

        let selected = ClientControl::AbilityActivate(AbilityActivateRequest {
            seq: 2,
            ability_id: purgatory_common::ContentId::from_token(9),
            selected: Some(WireEntityId {
                index: 4,
                generation: 1,
            }),
        });
        let encoded = encode_client_control(&selected).unwrap();
        assert_eq!(encoded.len(), 1 + crate::ABILITY_ACTIVATE_SELECTED_BYTES);
        assert_eq!(decode_client_control(&encoded).unwrap(), selected);

        let accepted = ServerControl::Ability(ServerAbility::Accepted { seq: 1 });
        let encoded = encode_server_control(&accepted).unwrap();
        assert_eq!(encoded.len(), 1 + crate::ABILITY_ACCEPTED_BYTES);
        assert_eq!(decode_server_control(&encoded).unwrap(), accepted);

        let rejected = ServerControl::Ability(ServerAbility::Rejected {
            seq: 3,
            reason: AbilityCommandReject::NotGranted,
        });
        let encoded = encode_server_control(&rejected).unwrap();
        assert_eq!(encoded.len(), 1 + crate::ABILITY_REJECTED_BYTES);
        assert_eq!(decode_server_control(&encoded).unwrap(), rejected);

        assert!(
            encode_client_control(&ClientControl::AbilityActivate(AbilityActivateRequest {
                seq: 0,
                ability_id: purgatory_common::ContentId::from_token(1),
                selected: None,
            }))
            .is_err()
        );
    }

    #[test]
    fn equipment_seq_zero_and_bad_slot_rejected() {
        assert!(
            encode_client_control(&ClientControl::Equip(EquipRequest {
                seq: 0,
                slot: 5,
                item_instance_id: purgatory_common::ItemInstanceId::from_raw(1),
            }))
            .is_err()
        );
        assert!(
            encode_client_control(&ClientControl::Unequip(UnequipRequest { seq: 1, slot: 6 }))
                .is_err()
        );
    }

    #[test]
    fn owner_private_inventory_snapshot_roundtrips_and_rejects_duplicates() {
        let snapshot = ServerControl::Inventory(ServerInventory {
            entries: vec![
                InventoryEntry {
                    slot: 0,
                    item_instance_id: purgatory_common::ItemInstanceId::from_raw(7),
                    definition: purgatory_common::ContentId::from_token(8),
                    quantity: 3,
                },
                InventoryEntry {
                    slot: 4,
                    item_instance_id: purgatory_common::ItemInstanceId::from_raw(9),
                    definition: purgatory_common::ContentId::from_token(10),
                    quantity: 1,
                },
            ],
        });
        let encoded = encode_server_control(&snapshot).unwrap();
        assert_eq!(decode_server_control(&encoded).unwrap(), snapshot);

        let mut duplicate = encoded;
        duplicate[2 + 22] = 0;
        duplicate[3 + 22] = 0;
        assert_eq!(
            decode_server_control(&duplicate),
            Err(CodecError::InvalidValue)
        );
    }
}
