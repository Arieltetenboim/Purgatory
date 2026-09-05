//! Protocol v8 replication frames: Enter / Update / Leave.
//!
//! Historical v1–v7 `WorldSnapshot` bytes stay frozen. v8 is incompatible.

use crate::equipment::{
    ReplicatedEquipment, ReplicatedEquipmentDelta, decode_equipment_delta, decode_equipment_full,
    encode_equipment_delta, encode_equipment_full,
};
use crate::snapshot::{
    PlatformSupportId, ReplicatedKind, SnapshotEntity, WireEntityId, expect_empty, read_entity_id,
    read_finite_f32, read_u16, read_u32, read_u64, split_tag, write_entity_id, write_f32,
};
use crate::{CodecError, MAX_ENTITIES_PER_SNAPSHOT, MAX_GAMEPLAY_SNAPSHOT_BYTES};

const TAG_REPLICATION_FRAME: u8 = 16;
const REC_ENTER: u8 = 1;
const REC_UPDATE: u8 = 2;
const REC_LEAVE: u8 = 3;

const MASK_TRANSFORM: u8 = 1 << 0;
const MASK_HEALTH: u8 = 1 << 1;
const MASK_EQUIPMENT: u8 = 1 << 2;

/// Optional health payload used to prove multi-domain deltas. Not a combat model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReplicatedHealth {
    pub current: f32,
    pub max: f32,
}

/// Domain bits on an Update record.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DomainMask {
    pub transform: bool,
    pub health: bool,
    pub equipment: bool,
}

impl DomainMask {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        let mut bits = 0u8;
        if self.transform {
            bits |= MASK_TRANSFORM;
        }
        if self.health {
            bits |= MASK_HEALTH;
        }
        if self.equipment {
            bits |= MASK_EQUIPMENT;
        }
        bits
    }

    #[must_use]
    pub const fn from_u8(bits: u8) -> Option<Self> {
        if bits & !(MASK_TRANSFORM | MASK_HEALTH | MASK_EQUIPMENT) != 0 {
            return None;
        }
        Some(Self {
            transform: bits & MASK_TRANSFORM != 0,
            health: bits & MASK_HEALTH != 0,
            equipment: bits & MASK_EQUIPMENT != 0,
        })
    }
}

/// One lifecycle or state record.
#[derive(Clone, Debug, PartialEq)]
pub enum ReplicationRecord {
    Enter {
        entity: SnapshotEntity,
        health: Option<ReplicatedHealth>,
        /// `None` = no equipment domain. `Some` (including all-empty) = domain present.
        equipment: Option<ReplicatedEquipment>,
    },
    Update {
        entity_id: WireEntityId,
        domains: DomainMask,
        position: Option<[f32; 2]>,
        velocity: Option<[f32; 2]>,
        health: Option<ReplicatedHealth>,
        equipment: Option<ReplicatedEquipmentDelta>,
    },
    Leave {
        entity_id: WireEntityId,
    },
}

/// Optional observer mailbox counts for DEV overlay. Not gameplay authority.
/// Encoded as an 8-byte trailer after records. Absent in frozen v8 goldens.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ObserverAoiDebug {
    pub candidates: u16,
    pub known: u16,
    pub want_enter: u16,
    pub want_leave: u16,
}

const AOI_DEBUG_TRAILER_BYTES: usize = 8;

/// Per-observer v8 frame. Header acknowledgement fields remain recipient-specific.
#[derive(Clone, Debug, PartialEq)]
pub struct ReplicationFrame {
    pub snapshot_sequence: u32,
    pub server_tick: u64,
    pub local_player_entity: WireEntityId,
    pub input_epoch: u16,
    pub last_acknowledged_input_sequence: u32,
    pub local_grounded: bool,
    pub local_grounded_on: PlatformSupportId,
    pub local_ignored_platform: PlatformSupportId,
    pub continuation_debt: u16,
    pub local_map: u32,
    pub local_channel: u32,
    pub local_instance: u32,
    pub observer_baseline_epoch: u32,
    pub records: Vec<ReplicationRecord>,
    pub aoi_debug: Option<ObserverAoiDebug>,
}

impl ReplicationFrame {
    #[must_use]
    pub fn observer_address(&self) -> (u32, u32, u32) {
        (self.local_map, self.local_channel, self.local_instance)
    }
}

/// Encode a single record (budget pre-check).
pub fn encode_replication_record(record: &ReplicationRecord) -> Result<Vec<u8>, CodecError> {
    let mut out = Vec::with_capacity(40);
    match record {
        ReplicationRecord::Enter {
            entity,
            health,
            equipment,
        } => {
            out.push(REC_ENTER);
            write_entity_id(&mut out, entity.entity_id);
            out.push(entity.kind.as_u8());
            write_f32(&mut out, entity.position[0])?;
            write_f32(&mut out, entity.position[1])?;
            write_f32(&mut out, entity.velocity[0])?;
            write_f32(&mut out, entity.velocity[1])?;
            write_health_opt(&mut out, *health)?;
            write_equipment_opt(&mut out, *equipment);
        }
        ReplicationRecord::Update {
            entity_id,
            domains,
            position,
            velocity,
            health,
            equipment,
        } => {
            if domains.as_u8() == 0 {
                return Err(CodecError::InvalidValue);
            }
            if domains.transform != (position.is_some() && velocity.is_some()) {
                return Err(CodecError::InvalidValue);
            }
            if domains.health != health.is_some() {
                return Err(CodecError::InvalidValue);
            }
            if domains.equipment != equipment.is_some() {
                return Err(CodecError::InvalidValue);
            }
            if let Some(delta) = equipment
                && delta.is_empty()
            {
                return Err(CodecError::InvalidValue);
            }
            out.push(REC_UPDATE);
            write_entity_id(&mut out, *entity_id);
            out.push(domains.as_u8());
            if let (Some(pos), Some(vel)) = (position, velocity) {
                write_f32(&mut out, pos[0])?;
                write_f32(&mut out, pos[1])?;
                write_f32(&mut out, vel[0])?;
                write_f32(&mut out, vel[1])?;
            }
            if let Some(h) = health {
                write_health(&mut out, *h)?;
            }
            if let Some(delta) = equipment {
                out.extend_from_slice(&encode_equipment_delta(delta));
            }
        }
        ReplicationRecord::Leave { entity_id } => {
            out.push(REC_LEAVE);
            write_entity_id(&mut out, *entity_id);
        }
    }
    Ok(out)
}

pub fn encode_replication_frame(frame: &ReplicationFrame) -> Result<Vec<u8>, CodecError> {
    if frame.records.len() > usize::from(MAX_ENTITIES_PER_SNAPSHOT) {
        return Err(CodecError::InvalidValue);
    }
    let count = u16::try_from(frame.records.len()).map_err(|_| CodecError::InvalidValue)?;
    let mut out = Vec::with_capacity(64 + frame.records.len() * 32);
    out.push(TAG_REPLICATION_FRAME);
    out.extend_from_slice(&frame.snapshot_sequence.to_le_bytes());
    out.extend_from_slice(&frame.server_tick.to_le_bytes());
    write_entity_id(&mut out, frame.local_player_entity);
    out.extend_from_slice(&frame.input_epoch.to_le_bytes());
    out.extend_from_slice(&frame.last_acknowledged_input_sequence.to_le_bytes());
    out.push(u8::from(frame.local_grounded));
    out.extend_from_slice(&frame.local_grounded_on.0.to_le_bytes());
    out.extend_from_slice(&frame.local_ignored_platform.0.to_le_bytes());
    out.extend_from_slice(&frame.continuation_debt.to_le_bytes());
    out.extend_from_slice(&frame.local_map.to_le_bytes());
    out.extend_from_slice(&frame.local_channel.to_le_bytes());
    out.extend_from_slice(&frame.local_instance.to_le_bytes());
    out.extend_from_slice(&frame.observer_baseline_epoch.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    for record in &frame.records {
        out.extend_from_slice(&encode_replication_record(record)?);
    }
    if let Some(debug) = frame.aoi_debug {
        out.extend_from_slice(&debug.candidates.to_le_bytes());
        out.extend_from_slice(&debug.known.to_le_bytes());
        out.extend_from_slice(&debug.want_enter.to_le_bytes());
        out.extend_from_slice(&debug.want_leave.to_le_bytes());
    }
    if out.len() > MAX_GAMEPLAY_SNAPSHOT_BYTES as usize {
        return Err(CodecError::SnapshotTooLarge);
    }
    Ok(out)
}

pub fn decode_replication_frame(bytes: &[u8]) -> Result<ReplicationFrame, CodecError> {
    if bytes.len() > MAX_GAMEPLAY_SNAPSHOT_BYTES as usize {
        return Err(CodecError::SnapshotTooLarge);
    }
    let (tag, rest) = split_tag(bytes)?;
    if tag != TAG_REPLICATION_FRAME {
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
    let (observer_baseline_epoch, rest) = read_u32(rest)?;
    let (count, rest) = read_u16(rest)?;
    if count > MAX_ENTITIES_PER_SNAPSHOT {
        return Err(CodecError::InvalidValue);
    }
    let mut records = Vec::with_capacity(usize::from(count));
    let mut rest = rest;
    for _ in 0..count {
        let (record, next) = decode_record(rest)?;
        records.push(record);
        rest = next;
    }
    let aoi_debug = if rest.is_empty() {
        None
    } else if rest.len() == AOI_DEBUG_TRAILER_BYTES {
        let (candidates, rest) = read_u16(rest)?;
        let (known, rest) = read_u16(rest)?;
        let (want_enter, rest) = read_u16(rest)?;
        let (want_leave, rest) = read_u16(rest)?;
        expect_empty(rest)?;
        Some(ObserverAoiDebug {
            candidates,
            known,
            want_enter,
            want_leave,
        })
    } else {
        return Err(CodecError::InvalidValue);
    };
    Ok(ReplicationFrame {
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
        observer_baseline_epoch,
        records,
        aoi_debug,
    })
}

fn decode_record(bytes: &[u8]) -> Result<(ReplicationRecord, &[u8]), CodecError> {
    let (tag, rest) = split_tag(bytes)?;
    match tag {
        REC_ENTER => {
            let (entity_id, rest) = read_entity_id(rest)?;
            if rest.is_empty() {
                return Err(CodecError::Truncated);
            }
            let kind = ReplicatedKind::from_u8(rest[0]).ok_or(CodecError::InvalidValue)?;
            let (px, rest) = read_finite_f32(&rest[1..])?;
            let (py, rest) = read_finite_f32(rest)?;
            let (vx, rest) = read_finite_f32(rest)?;
            let (vy, rest) = read_finite_f32(rest)?;
            let (health, rest) = read_health_opt(rest)?;
            let (equipment, rest) = read_equipment_opt(rest)?;
            Ok((
                ReplicationRecord::Enter {
                    entity: SnapshotEntity {
                        entity_id,
                        kind,
                        position: [px, py],
                        velocity: [vx, vy],
                    },
                    health,
                    equipment,
                },
                rest,
            ))
        }
        REC_UPDATE => {
            let (entity_id, rest) = read_entity_id(rest)?;
            if rest.is_empty() {
                return Err(CodecError::Truncated);
            }
            let domains = DomainMask::from_u8(rest[0]).ok_or(CodecError::InvalidValue)?;
            let mut rest = &rest[1..];
            let mut position = None;
            let mut velocity = None;
            if domains.transform {
                let (px, next) = read_finite_f32(rest)?;
                let (py, next) = read_finite_f32(next)?;
                let (vx, next) = read_finite_f32(next)?;
                let (vy, next) = read_finite_f32(next)?;
                position = Some([px, py]);
                velocity = Some([vx, vy]);
                rest = next;
            }
            let health = if domains.health {
                let (h, next) = read_health(rest)?;
                rest = next;
                Some(h)
            } else {
                None
            };
            let equipment = if domains.equipment {
                let (d, next) = decode_equipment_delta(rest)?;
                rest = next;
                Some(d)
            } else {
                None
            };
            Ok((
                ReplicationRecord::Update {
                    entity_id,
                    domains,
                    position,
                    velocity,
                    health,
                    equipment,
                },
                rest,
            ))
        }
        REC_LEAVE => {
            let (entity_id, rest) = read_entity_id(rest)?;
            Ok((ReplicationRecord::Leave { entity_id }, rest))
        }
        other => Err(CodecError::UnknownDiscriminant(other)),
    }
}

fn write_health_opt(out: &mut Vec<u8>, health: Option<ReplicatedHealth>) -> Result<(), CodecError> {
    match health {
        None => {
            out.push(0);
            Ok(())
        }
        Some(h) => {
            out.push(1);
            write_health(out, h)
        }
    }
}

fn write_health(out: &mut Vec<u8>, health: ReplicatedHealth) -> Result<(), CodecError> {
    write_f32(out, health.current)?;
    write_f32(out, health.max)
}

fn write_equipment_opt(out: &mut Vec<u8>, equipment: Option<ReplicatedEquipment>) {
    match equipment {
        None => out.push(0),
        Some(state) => {
            out.push(1);
            out.extend_from_slice(&encode_equipment_full(&state));
        }
    }
}

fn read_equipment_opt(bytes: &[u8]) -> Result<(Option<ReplicatedEquipment>, &[u8]), CodecError> {
    if bytes.is_empty() {
        return Err(CodecError::Truncated);
    }
    match bytes[0] {
        0 => Ok((None, &bytes[1..])),
        1 => {
            let (state, rest) = decode_equipment_full(&bytes[1..])?;
            Ok((Some(state), rest))
        }
        _ => Err(CodecError::InvalidValue),
    }
}

fn read_health_opt(bytes: &[u8]) -> Result<(Option<ReplicatedHealth>, &[u8]), CodecError> {
    if bytes.is_empty() {
        return Err(CodecError::Truncated);
    }
    match bytes[0] {
        0 => Ok((None, &bytes[1..])),
        1 => {
            let (h, rest) = read_health(&bytes[1..])?;
            Ok((Some(h), rest))
        }
        _ => Err(CodecError::InvalidValue),
    }
}

fn read_health(bytes: &[u8]) -> Result<(ReplicatedHealth, &[u8]), CodecError> {
    let (current, rest) = read_finite_f32(bytes)?;
    let (max, rest) = read_finite_f32(rest)?;
    Ok((ReplicatedHealth { current, max }, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entity() -> SnapshotEntity {
        SnapshotEntity {
            entity_id: WireEntityId {
                index: 5,
                generation: 1,
            },
            kind: ReplicatedKind::Player,
            position: [1.0, 2.0],
            velocity: [3.0, 4.0],
        }
    }

    fn sample_frame(records: Vec<ReplicationRecord>) -> ReplicationFrame {
        ReplicationFrame {
            snapshot_sequence: 4,
            server_tick: 10,
            local_player_entity: WireEntityId {
                index: 5,
                generation: 1,
            },
            input_epoch: 0,
            last_acknowledged_input_sequence: 2,
            local_grounded: true,
            local_grounded_on: PlatformSupportId(1),
            local_ignored_platform: PlatformSupportId::NONE,
            continuation_debt: 0,
            local_map: 1,
            local_channel: 0,
            local_instance: 0,
            observer_baseline_epoch: 1,
            records,
            aoi_debug: None,
        }
    }

    #[test]
    fn enter_update_leave_roundtrip() {
        let frame = sample_frame(vec![
            ReplicationRecord::Enter {
                entity: sample_entity(),
                health: Some(ReplicatedHealth {
                    current: 8.0,
                    max: 10.0,
                }),
                equipment: None,
            },
            ReplicationRecord::Update {
                entity_id: sample_entity().entity_id,
                domains: DomainMask {
                    transform: true,
                    health: false,
                    equipment: false,
                },
                position: Some([2.0, 2.0]),
                velocity: Some([1.0, 0.0]),
                health: None,
                equipment: None,
            },
            ReplicationRecord::Leave {
                entity_id: WireEntityId {
                    index: 9,
                    generation: 2,
                },
            },
        ]);
        let encoded = encode_replication_frame(&frame).expect("encode");
        assert_eq!(decode_replication_frame(&encoded).expect("decode"), frame);
        assert_ne!(encoded[0], 7, "must not reuse WorldSnapshot tag");
    }

    #[test]
    fn aoi_debug_trailer_roundtrips_and_absence_is_none() {
        let mut with = sample_frame(vec![]);
        with.aoi_debug = Some(ObserverAoiDebug {
            candidates: 4,
            known: 2,
            want_enter: 1,
            want_leave: 1,
        });
        let encoded = encode_replication_frame(&with).unwrap();
        let decoded = decode_replication_frame(&encoded).unwrap();
        assert_eq!(decoded.aoi_debug, with.aoi_debug);
        let mut without = with;
        without.aoi_debug = None;
        let encoded = encode_replication_frame(&without).unwrap();
        assert!(
            decode_replication_frame(&encoded)
                .unwrap()
                .aoi_debug
                .is_none()
        );
    }

    #[test]
    fn encoded_record_size_matches_frame_payload_slice() {
        let rec = ReplicationRecord::Leave {
            entity_id: WireEntityId {
                index: 1,
                generation: 1,
            },
        };
        let bytes = encode_replication_record(&rec).unwrap();
        assert_eq!(bytes.len(), 9);
    }
}
