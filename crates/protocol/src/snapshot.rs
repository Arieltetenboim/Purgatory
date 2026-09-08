//! Authoritative world snapshots (server → client).
//!
//! Full snapshots only. Entity identity is generational (`index` + `generation`).
//! Decode validates bounds before allocating the entity vector. Non-finite
//! floats are rejected. This is packet/resource safety, not a population cap.
//!
//! Protocol v4 snapshots are **per-recipient**. Acknowledgement, epoch, and
//! local contact fields describe the owning client's player only.

use crate::{CodecError, MAX_ENTITIES_PER_SNAPSHOT, MAX_GAMEPLAY_SNAPSHOT_BYTES};

const TAG_WORLD_SNAPSHOT: u8 = 7;
const KIND_PLAYER: u8 = 1;
const KIND_INTERACTABLE: u8 = 2;
const KIND_PORTAL: u8 = 3;
const KIND_NPC: u8 = 4;
const KIND_ITEM: u8 = 5;

/// Wire entity identity. Generation is part of equality; index reuse is a
/// different entity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct WireEntityId {
    pub index: u32,
    pub generation: u32,
}

impl std::fmt::Display for WireEntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.index, self.generation)
    }
}

/// Stage-local support identity. `0` means none. Not a runtime `EntityId`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct PlatformSupportId(pub u16);

impl PlatformSupportId {
    pub const NONE: Self = Self(0);

    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn get(self) -> Option<u16> {
        if self.0 == 0 { None } else { Some(self.0) }
    }

    #[must_use]
    pub const fn from_raw(id: u16) -> Self {
        Self(id)
    }
}

/// Replicated entity classification on the wire. Unknown values are rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ReplicatedKind {
    Player = 1,
    Interactable = 2,
    Portal = 3,
    /// Visible non-player actor (sim `EntityKind::Generic` with Transform).
    Npc = 4,
    /// Authoritative Item-backed world drop.
    Item = 5,
}

impl std::fmt::Display for ReplicatedKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Player => "Player",
            Self::Interactable => "Interactable",
            Self::Portal => "Portal",
            Self::Npc => "Npc",
            Self::Item => "Item",
        })
    }
}

impl ReplicatedKind {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            KIND_PLAYER => Some(Self::Player),
            KIND_INTERACTABLE => Some(Self::Interactable),
            KIND_PORTAL => Some(Self::Portal),
            KIND_NPC => Some(Self::Npc),
            KIND_ITEM => Some(Self::Item),
            _ => None,
        }
    }
}

/// One dynamic replicated entity. Static map platforms are not included.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapshotEntity {
    pub entity_id: WireEntityId,
    pub kind: ReplicatedKind,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
}

/// Server-produced full world snapshot. `local_player_entity` is explicit and
/// must not be inferred from array order, spawn position, or `ConnectionId`.
///
/// Header acknowledgement / contact fields are **recipient-specific**.
/// Never reuse one `WorldSnapshot` value across connections.
///
/// `last_acknowledged_input_sequence` is the accounted replay boundary for
/// this recipient's epoch: every command with `sequence <= ack` must not be
/// replayed. A sequence may be acknowledged by a single Consumed tick, by
/// late-collapse of a prefix (intentional authoritative input compaction —
/// intermediate historical held commands may be acknowledged without
/// individual physics steps), or by HeldCancel cancel-ack. It is not
/// highest-received, highest-queued, or one `tick_player` per sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldSnapshot {
    pub snapshot_sequence: u32,
    pub server_tick: u64,
    pub local_player_entity: WireEntityId,
    pub input_epoch: u16,
    pub last_acknowledged_input_sequence: u32,
    pub local_grounded: bool,
    pub local_grounded_on: PlatformSupportId,
    pub local_ignored_platform: PlatformSupportId,
    pub continuation_debt: u16,
    /// Observer map membership. A change is a replication baseline boundary.
    pub local_map: u32,
    pub local_channel: u32,
    pub local_instance: u32,
    pub entities: Vec<SnapshotEntity>,
}

impl WorldSnapshot {
    /// Pose payload with zeroed Phase 5.5 header fields. Tests that need
    /// acknowledgement or contact must set those fields explicitly.
    #[must_use]
    pub fn from_poses(
        snapshot_sequence: u32,
        server_tick: u64,
        local_player_entity: WireEntityId,
        entities: Vec<SnapshotEntity>,
    ) -> Self {
        Self {
            snapshot_sequence,
            server_tick,
            local_player_entity,
            input_epoch: 0,
            last_acknowledged_input_sequence: 0,
            local_grounded: false,
            local_grounded_on: PlatformSupportId::NONE,
            local_ignored_platform: PlatformSupportId::NONE,
            continuation_debt: 0,
            local_map: 1,
            local_channel: 0,
            local_instance: 0,
            entities,
        }
    }

    #[must_use]
    pub fn observer_address(&self) -> (u32, u32, u32) {
        (self.local_map, self.local_channel, self.local_instance)
    }
}

pub fn encode_world_snapshot(snap: &WorldSnapshot) -> Result<Vec<u8>, CodecError> {
    if snap.entities.len() > usize::from(MAX_ENTITIES_PER_SNAPSHOT) {
        return Err(CodecError::InvalidValue);
    }
    let count = u16::try_from(snap.entities.len()).map_err(|_| CodecError::InvalidValue)?;
    let mut out = Vec::with_capacity(34 + snap.entities.len() * 25);
    out.push(TAG_WORLD_SNAPSHOT);
    out.extend_from_slice(&snap.snapshot_sequence.to_le_bytes());
    out.extend_from_slice(&snap.server_tick.to_le_bytes());
    write_entity_id(&mut out, snap.local_player_entity);
    out.extend_from_slice(&snap.input_epoch.to_le_bytes());
    out.extend_from_slice(&snap.last_acknowledged_input_sequence.to_le_bytes());
    out.push(u8::from(snap.local_grounded));
    out.extend_from_slice(&snap.local_grounded_on.0.to_le_bytes());
    out.extend_from_slice(&snap.local_ignored_platform.0.to_le_bytes());
    out.extend_from_slice(&snap.continuation_debt.to_le_bytes());
    out.extend_from_slice(&snap.local_map.to_le_bytes());
    out.extend_from_slice(&snap.local_channel.to_le_bytes());
    out.extend_from_slice(&snap.local_instance.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    for entity in &snap.entities {
        write_entity_id(&mut out, entity.entity_id);
        out.push(entity.kind.as_u8());
        write_f32(&mut out, entity.position[0])?;
        write_f32(&mut out, entity.position[1])?;
        write_f32(&mut out, entity.velocity[0])?;
        write_f32(&mut out, entity.velocity[1])?;
    }
    if out.len() > MAX_GAMEPLAY_SNAPSHOT_BYTES as usize {
        return Err(CodecError::SnapshotTooLarge);
    }
    Ok(out)
}

pub fn decode_world_snapshot(bytes: &[u8]) -> Result<WorldSnapshot, CodecError> {
    if bytes.len() > MAX_GAMEPLAY_SNAPSHOT_BYTES as usize {
        return Err(CodecError::SnapshotTooLarge);
    }
    let (tag, rest) = split_tag(bytes)?;
    if tag != TAG_WORLD_SNAPSHOT {
        return Err(CodecError::UnknownDiscriminant(tag));
    }
    let (snapshot_sequence, rest) = read_u32(rest)?;
    let (server_tick, rest) = read_u64(rest)?;
    let (local_player_entity, rest) = read_entity_id(rest)?;
    let (input_epoch, rest) = read_u16(rest)?;
    let (last_acknowledged_input_sequence, rest) = read_u32(rest)?;
    if rest.is_empty() {
        return Err(CodecError::Truncated);
    }
    let local_grounded = match rest[0] {
        0 => false,
        1 => true,
        _ => return Err(CodecError::InvalidValue),
    };
    let (local_grounded_on, rest) = read_u16(&rest[1..])?;
    let (local_ignored_platform, rest) = read_u16(rest)?;
    let (continuation_debt, rest) = read_u16(rest)?;
    let (local_map, rest) = read_u32(rest)?;
    let (local_channel, rest) = read_u32(rest)?;
    let (local_instance, rest) = read_u32(rest)?;
    let (count, rest) = read_u16(rest)?;
    if count > MAX_ENTITIES_PER_SNAPSHOT {
        return Err(CodecError::InvalidValue);
    }
    let mut entities = Vec::with_capacity(usize::from(count));
    let mut rest = rest;
    for _ in 0..count {
        let (entity_id, next) = read_entity_id(rest)?;
        if next.is_empty() {
            return Err(CodecError::Truncated);
        }
        let kind = ReplicatedKind::from_u8(next[0]).ok_or(CodecError::InvalidValue)?;
        let (px, next) = read_finite_f32(&next[1..])?;
        let (py, next) = read_finite_f32(next)?;
        let (vx, next) = read_finite_f32(next)?;
        let (vy, next) = read_finite_f32(next)?;
        entities.push(SnapshotEntity {
            entity_id,
            kind,
            position: [px, py],
            velocity: [vx, vy],
        });
        rest = next;
    }
    expect_empty(rest)?;
    Ok(WorldSnapshot {
        snapshot_sequence,
        server_tick,
        local_player_entity,
        input_epoch,
        last_acknowledged_input_sequence,
        local_grounded,
        local_grounded_on: PlatformSupportId(local_grounded_on),
        local_ignored_platform: PlatformSupportId(local_ignored_platform),
        continuation_debt,
        local_map,
        local_channel,
        local_instance,
        entities,
    })
}

pub(crate) fn write_entity_id(out: &mut Vec<u8>, id: WireEntityId) {
    out.extend_from_slice(&id.index.to_le_bytes());
    out.extend_from_slice(&id.generation.to_le_bytes());
}

pub(crate) fn write_f32(out: &mut Vec<u8>, value: f32) -> Result<(), CodecError> {
    if !value.is_finite() {
        return Err(CodecError::InvalidValue);
    }
    out.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

pub(crate) fn split_tag(bytes: &[u8]) -> Result<(u8, &[u8]), CodecError> {
    let (tag, rest) = bytes.split_first().ok_or(CodecError::Truncated)?;
    Ok((*tag, rest))
}

pub(crate) fn read_u16(bytes: &[u8]) -> Result<(u16, &[u8]), CodecError> {
    if bytes.len() < 2 {
        return Err(CodecError::Truncated);
    }
    Ok((u16::from_le_bytes([bytes[0], bytes[1]]), &bytes[2..]))
}

pub(crate) fn read_u32(bytes: &[u8]) -> Result<(u32, &[u8]), CodecError> {
    if bytes.len() < 4 {
        return Err(CodecError::Truncated);
    }
    Ok((
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        &bytes[4..],
    ))
}

pub(crate) fn read_u64(bytes: &[u8]) -> Result<(u64, &[u8]), CodecError> {
    if bytes.len() < 8 {
        return Err(CodecError::Truncated);
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[..8]);
    Ok((u64::from_le_bytes(buf), &bytes[8..]))
}

pub(crate) fn read_entity_id(bytes: &[u8]) -> Result<(WireEntityId, &[u8]), CodecError> {
    let (index, rest) = read_u32(bytes)?;
    let (generation, rest) = read_u32(rest)?;
    Ok((WireEntityId { index, generation }, rest))
}

pub(crate) fn read_finite_f32(bytes: &[u8]) -> Result<(f32, &[u8]), CodecError> {
    if bytes.len() < 4 {
        return Err(CodecError::Truncated);
    }
    let value = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if !value.is_finite() {
        return Err(CodecError::InvalidValue);
    }
    Ok((value, &bytes[4..]))
}

pub(crate) fn expect_empty(rest: &[u8]) -> Result<(), CodecError> {
    if rest.is_empty() {
        Ok(())
    } else {
        Err(CodecError::TrailingBytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> WorldSnapshot {
        WorldSnapshot {
            snapshot_sequence: 0x0102_0304,
            server_tick: 10,
            local_player_entity: WireEntityId {
                index: 5,
                generation: 1,
            },
            input_epoch: 0,
            last_acknowledged_input_sequence: 7,
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
    fn snapshot_roundtrip() {
        let snap = sample();
        let encoded = encode_world_snapshot(&snap).expect("encode");
        assert_eq!(decode_world_snapshot(&encoded).expect("decode"), snap);
    }

    #[test]
    fn snapshot_roundtrip_preserves_interactable_kind() {
        let snap = WorldSnapshot::from_poses(
            1,
            1,
            WireEntityId {
                index: 9,
                generation: 1,
            },
            vec![SnapshotEntity {
                entity_id: WireEntityId {
                    index: 4,
                    generation: 2,
                },
                kind: ReplicatedKind::Interactable,
                position: [-18.2, -3.05],
                velocity: [0.0, 0.0],
            }],
        );
        let encoded = encode_world_snapshot(&snap).expect("encode");
        let decoded = decode_world_snapshot(&encoded).expect("decode");
        assert_eq!(decoded.entities.len(), 1);
        assert_eq!(decoded.entities[0].kind, ReplicatedKind::Interactable);
        assert_eq!(decoded.entities[0].position, [-18.2, -3.05]);
    }

    #[test]
    fn snapshot_roundtrip_preserves_portal_kind() {
        let snap = WorldSnapshot::from_poses(
            1,
            1,
            WireEntityId {
                index: 9,
                generation: 1,
            },
            vec![SnapshotEntity {
                entity_id: WireEntityId {
                    index: 8,
                    generation: 1,
                },
                kind: ReplicatedKind::Portal,
                position: [6.0, -2.9],
                velocity: [0.0, 0.0],
            }],
        );
        let encoded = encode_world_snapshot(&snap).expect("encode");
        let decoded = decode_world_snapshot(&encoded).expect("decode");
        assert_eq!(decoded.entities[0].kind, ReplicatedKind::Portal);
        assert_eq!(decoded.entities[0].position, [6.0, -2.9]);
    }

    #[test]
    fn generational_ids_are_distinct() {
        let a = WireEntityId {
            index: 5,
            generation: 1,
        };
        let b = WireEntityId {
            index: 5,
            generation: 2,
        };
        assert_ne!(a, b);
        assert_eq!(a.to_string(), "5:1");
    }

    #[test]
    fn entity_count_above_bound_rejected_before_alloc_logic() {
        let mut bytes = Vec::new();
        bytes.push(TAG_WORLD_SNAPSHOT);
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.push(1);
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&(MAX_ENTITIES_PER_SNAPSHOT + 1).to_le_bytes());
        assert_eq!(decode_world_snapshot(&bytes), Err(CodecError::InvalidValue));
    }

    #[test]
    fn oversized_payload_rejected() {
        let bytes = vec![0u8; MAX_GAMEPLAY_SNAPSHOT_BYTES as usize + 1];
        assert_eq!(
            decode_world_snapshot(&bytes),
            Err(CodecError::SnapshotTooLarge)
        );
    }

    #[test]
    fn truncated_snapshot_rejected() {
        let encoded = encode_world_snapshot(&sample()).expect("encode");
        assert_eq!(
            decode_world_snapshot(&encoded[..encoded.len() - 1]),
            Err(CodecError::Truncated)
        );
    }

    #[test]
    fn invalid_kind_rejected() {
        let mut encoded = encode_world_snapshot(&sample()).expect("encode");
        // header before first entity kind: tag(1)+seq(4)+tick(8)+local(8)
        // +epoch(2)+ack(4)+grounded(1)+on(2)+ign(2)+debt(2)+map(4)+ch(4)+inst(4)
        // +count(2)+id(8) = 56
        encoded[56] = 99;
        assert_eq!(
            decode_world_snapshot(&encoded),
            Err(CodecError::InvalidValue)
        );
    }

    #[test]
    fn nan_and_inf_rejected() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut encoded = encode_world_snapshot(&sample()).expect("encode");
            encoded[57..61].copy_from_slice(&bad.to_le_bytes());
            assert_eq!(
                decode_world_snapshot(&encoded),
                Err(CodecError::InvalidValue),
                "expected reject for {bad}"
            );
        }
        let mut snap = sample();
        snap.entities[0].position[0] = f32::NAN;
        assert_eq!(encode_world_snapshot(&snap), Err(CodecError::InvalidValue));
    }

    #[test]
    fn unknown_tag_rejected() {
        assert_eq!(
            decode_world_snapshot(&[1, 0, 0, 0, 0]),
            Err(CodecError::UnknownDiscriminant(1))
        );
    }

    #[test]
    fn trailing_bytes_rejected() {
        let mut encoded = encode_world_snapshot(&sample()).expect("encode");
        encoded.push(0);
        assert_eq!(
            decode_world_snapshot(&encoded),
            Err(CodecError::TrailingBytes)
        );
    }

    #[test]
    fn support_id_zero_is_none() {
        assert!(PlatformSupportId::NONE.is_none());
        assert_eq!(PlatformSupportId(3).get(), Some(3));
    }
}
