//! Protocol v13 DEV presentation one-shot (Attack/Hurt) control envelopes.
//!
//! Semantic state only — no bones, sample_t, or animation events on the wire.
//! Snapshot Enter/Update paths are unchanged (0 per-frame animation traffic).

use crate::CodecError;
use crate::snapshot::WireEntityId;

pub(crate) const TAG_DEV_PRESENTATION_ONESHOT: u8 = 22;
pub(crate) const TAG_SERVER_PRESENTATION_ONESHOT: u8 = 23;

/// Client → server DEV request to start an authoritative presentation oneshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DevPresentationOneShot {
    /// `1` = Attack, `2` = Hurt. Other values are malformed.
    pub kind: u8,
}

/// Server → clients: oneshot started or cleared for an entity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServerPresentationOneShot {
    pub entity: WireEntityId,
    /// `0` = cleared / inactive. `1` = Attack. `2` = Hurt.
    pub kind: u8,
    /// Exclusive end tick. Client treats `current_tick >= until_tick` as inactive.
    pub until_tick: u32,
}

impl DevPresentationOneShot {
    #[must_use]
    pub const fn kind_valid(kind: u8) -> bool {
        matches!(kind, 1 | 2)
    }
}

/// Encoded `DevPresentationOneShot` size (tag + kind).
pub const DEV_PRESENTATION_ONESHOT_BYTES: usize = 2;
/// Encoded `ServerPresentationOneShot` size (tag + entity 8 + kind + until_tick 4).
pub const SERVER_PRESENTATION_ONESHOT_BYTES: usize = 1 + 8 + 1 + 4;

#[must_use]
pub fn encode_dev_presentation_oneshot(msg: &DevPresentationOneShot) -> Vec<u8> {
    vec![TAG_DEV_PRESENTATION_ONESHOT, msg.kind]
}

pub fn decode_dev_presentation_oneshot(rest: &[u8]) -> Result<DevPresentationOneShot, CodecError> {
    if rest.len() != 1 {
        return Err(CodecError::Truncated);
    }
    Ok(DevPresentationOneShot { kind: rest[0] })
}

#[must_use]
pub fn encode_server_presentation_oneshot(msg: &ServerPresentationOneShot) -> Vec<u8> {
    let mut out = Vec::with_capacity(SERVER_PRESENTATION_ONESHOT_BYTES);
    out.push(TAG_SERVER_PRESENTATION_ONESHOT);
    out.extend_from_slice(&msg.entity.index.to_le_bytes());
    out.extend_from_slice(&msg.entity.generation.to_le_bytes());
    out.push(msg.kind);
    out.extend_from_slice(&msg.until_tick.to_le_bytes());
    out
}

pub fn decode_server_presentation_oneshot(
    rest: &[u8],
) -> Result<ServerPresentationOneShot, CodecError> {
    if rest.len() != 8 + 1 + 4 {
        return Err(CodecError::Truncated);
    }
    let index = u32::from_le_bytes(rest[0..4].try_into().unwrap());
    let generation = u32::from_le_bytes(rest[4..8].try_into().unwrap());
    let kind = rest[8];
    let until_tick = u32::from_le_bytes(rest[9..13].try_into().unwrap());
    Ok(ServerPresentationOneShot {
        entity: WireEntityId { index, generation },
        kind,
        until_tick,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_roundtrip() {
        let req = DevPresentationOneShot { kind: 1 };
        let bytes = encode_dev_presentation_oneshot(&req);
        assert_eq!(bytes.len(), DEV_PRESENTATION_ONESHOT_BYTES);
        assert_eq!(decode_dev_presentation_oneshot(&bytes[1..]).unwrap(), req);

        let evt = ServerPresentationOneShot {
            entity: WireEntityId {
                index: 3,
                generation: 2,
            },
            kind: 2,
            until_tick: 100,
        };
        let bytes = encode_server_presentation_oneshot(&evt);
        assert_eq!(bytes.len(), SERVER_PRESENTATION_ONESHOT_BYTES);
        assert_eq!(
            decode_server_presentation_oneshot(&bytes[1..]).unwrap(),
            evt
        );
    }
}
