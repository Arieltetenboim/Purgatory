//! Length-prefixed stream framing.
//!
//! Layout: `u32` little-endian payload length, then exactly that many bytes.
//! One QUIC stream write is **not** one message. Control frames are bounded by
//! [`crate::MAX_CONTROL_MESSAGE_BYTES`]; gameplay snapshots use
//! [`crate::MAX_GAMEPLAY_SNAPSHOT_BYTES`]. The length is checked before the
//! payload buffer is allocated.

use crate::{MAX_CONTROL_MESSAGE_BYTES, MAX_GAMEPLAY_SNAPSHOT_BYTES};

/// Errors while encoding or decoding a length prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    /// Buffer shorter than 4 bytes, or payload shorter than the declared length.
    Truncated,
    /// Declared length is 0 or greater than the applicable maximum.
    InvalidLength(u32),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated length-prefixed frame"),
            Self::InvalidLength(len) => {
                write!(f, "invalid length-prefixed frame length {len}")
            }
        }
    }
}

impl std::error::Error for FrameError {}

/// Encode `payload` as a control-stream frame.
pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    encode_frame_limited(payload, MAX_CONTROL_MESSAGE_BYTES)
}

/// Encode `payload` as a gameplay-snapshot frame.
pub fn encode_gameplay_frame(payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    encode_frame_limited(payload, MAX_GAMEPLAY_SNAPSHOT_BYTES)
}

/// Validate a 4-byte control-stream length prefix without allocating.
pub fn peek_frame_len(prefix: &[u8; 4]) -> Result<u32, FrameError> {
    peek_frame_len_limited(prefix, MAX_CONTROL_MESSAGE_BYTES)
}

/// Validate a 4-byte gameplay-snapshot length prefix without allocating.
pub fn peek_gameplay_frame_len(prefix: &[u8; 4]) -> Result<u32, FrameError> {
    peek_frame_len_limited(prefix, MAX_GAMEPLAY_SNAPSHOT_BYTES)
}

/// Decode one control frame from the front of `bytes`.
pub fn decode_payload(bytes: &[u8]) -> Result<(&[u8], &[u8]), FrameError> {
    decode_payload_limited(bytes, MAX_CONTROL_MESSAGE_BYTES)
}

/// Decode one gameplay-snapshot frame from the front of `bytes`.
pub fn decode_gameplay_payload(bytes: &[u8]) -> Result<(&[u8], &[u8]), FrameError> {
    decode_payload_limited(bytes, MAX_GAMEPLAY_SNAPSHOT_BYTES)
}

fn encode_frame_limited(payload: &[u8], max: u32) -> Result<Vec<u8>, FrameError> {
    let len = u32::try_from(payload.len()).map_err(|_| FrameError::InvalidLength(u32::MAX))?;
    if len == 0 || len > max {
        return Err(FrameError::InvalidLength(len));
    }
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

fn peek_frame_len_limited(prefix: &[u8; 4], max: u32) -> Result<u32, FrameError> {
    let len = u32::from_le_bytes(*prefix);
    if len == 0 || len > max {
        Err(FrameError::InvalidLength(len))
    } else {
        Ok(len)
    }
}

fn decode_payload_limited(bytes: &[u8], max: u32) -> Result<(&[u8], &[u8]), FrameError> {
    if bytes.len() < 4 {
        return Err(FrameError::Truncated);
    }
    let mut prefix = [0u8; 4];
    prefix.copy_from_slice(&bytes[..4]);
    let len = peek_frame_len_limited(&prefix, max)?;
    let len_usize = usize::try_from(len).map_err(|_| FrameError::InvalidLength(len))?;
    let end = 4usize
        .checked_add(len_usize)
        .ok_or(FrameError::InvalidLength(len))?;
    if bytes.len() < end {
        return Err(FrameError::Truncated);
    }
    Ok((&bytes[4..end], &bytes[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_small_payload() {
        let frame = encode_frame(&[1, 2, 3]).expect("encode");
        let (payload, rest) = decode_payload(&frame).expect("decode");
        assert_eq!(payload, &[1, 2, 3]);
        assert!(rest.is_empty());
    }

    #[test]
    fn empty_payload_rejected() {
        assert_eq!(encode_frame(&[]), Err(FrameError::InvalidLength(0)));
        assert_eq!(
            peek_frame_len(&[0, 0, 0, 0]),
            Err(FrameError::InvalidLength(0))
        );
    }

    #[test]
    fn oversized_length_rejected_before_payload() {
        let too_big = MAX_CONTROL_MESSAGE_BYTES + 1;
        assert_eq!(
            peek_frame_len(&too_big.to_le_bytes()),
            Err(FrameError::InvalidLength(too_big))
        );
        let huge = 1_000_000u32;
        assert_eq!(
            peek_frame_len(&huge.to_le_bytes()),
            Err(FrameError::InvalidLength(huge))
        );
    }

    #[test]
    fn truncated_payload_is_error() {
        let mut frame = encode_frame(&[9, 9, 9]).expect("encode");
        frame.pop();
        assert_eq!(decode_payload(&frame), Err(FrameError::Truncated));
    }

    #[test]
    fn truncated_prefix_is_error() {
        assert_eq!(decode_payload(&[1, 2, 3]), Err(FrameError::Truncated));
    }

    #[test]
    fn max_size_payload_is_accepted() {
        let payload = vec![7u8; MAX_CONTROL_MESSAGE_BYTES as usize];
        let frame = encode_frame(&payload).expect("encode max");
        let (decoded, rest) = decode_payload(&frame).expect("decode max");
        assert_eq!(decoded.len(), payload.len());
        assert!(rest.is_empty());
    }

    #[test]
    fn length_prefix_matrix_validated_before_alloc() {
        assert_eq!(
            peek_frame_len(&0u32.to_le_bytes()),
            Err(FrameError::InvalidLength(0))
        );
        assert_eq!(peek_frame_len(&1u32.to_le_bytes()), Ok(1));
        assert_eq!(
            peek_frame_len(&MAX_CONTROL_MESSAGE_BYTES.to_le_bytes()),
            Ok(MAX_CONTROL_MESSAGE_BYTES)
        );
        let max_plus = MAX_CONTROL_MESSAGE_BYTES + 1;
        assert_eq!(
            peek_frame_len(&max_plus.to_le_bytes()),
            Err(FrameError::InvalidLength(max_plus))
        );
        assert_eq!(
            peek_frame_len(&u32::MAX.to_le_bytes()),
            Err(FrameError::InvalidLength(u32::MAX))
        );
        assert_eq!(
            decode_payload(&u32::MAX.to_le_bytes()),
            Err(FrameError::InvalidLength(u32::MAX))
        );
    }

    #[test]
    fn truncated_length_prefix_does_not_allocate() {
        for n in 0..4 {
            assert_eq!(decode_payload(&vec![1u8; n]), Err(FrameError::Truncated));
        }
    }

    #[test]
    fn gameplay_frame_bound_is_independent_from_control() {
        let between = MAX_CONTROL_MESSAGE_BYTES + 1;
        assert_eq!(
            peek_frame_len(&between.to_le_bytes()),
            Err(FrameError::InvalidLength(between))
        );
        assert_eq!(peek_gameplay_frame_len(&between.to_le_bytes()), Ok(between));
        let too_big = MAX_GAMEPLAY_SNAPSHOT_BYTES + 1;
        assert_eq!(
            peek_gameplay_frame_len(&too_big.to_le_bytes()),
            Err(FrameError::InvalidLength(too_big))
        );
    }
}
