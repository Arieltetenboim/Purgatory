//! Length-prefixed control-stream framing.
//!
//! Layout: `u32` little-endian payload length, then exactly that many bytes.
//! One QUIC stream write is **not** one message. The length is checked against
//! [`crate::MAX_CONTROL_MESSAGE_BYTES`] before the payload buffer is allocated.

use crate::MAX_CONTROL_MESSAGE_BYTES;

/// Errors while encoding or decoding a length prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    /// Buffer shorter than 4 bytes, or payload shorter than the declared length.
    Truncated,
    /// Declared length is 0 or greater than [`MAX_CONTROL_MESSAGE_BYTES`].
    InvalidLength(u32),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated control frame"),
            Self::InvalidLength(len) => {
                write!(f, "invalid control frame length {len}")
            }
        }
    }
}

impl std::error::Error for FrameError {}

/// Encode `payload` as a length-prefixed frame. Fails if payload is empty or too large.
pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    let len = u32::try_from(payload.len()).map_err(|_| FrameError::InvalidLength(u32::MAX))?;
    if len == 0 || len > MAX_CONTROL_MESSAGE_BYTES {
        return Err(FrameError::InvalidLength(len));
    }
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Validate a 4-byte length prefix without allocating a payload buffer.
pub fn peek_frame_len(prefix: &[u8; 4]) -> Result<u32, FrameError> {
    let len = u32::from_le_bytes(*prefix);
    if len == 0 || len > MAX_CONTROL_MESSAGE_BYTES {
        Err(FrameError::InvalidLength(len))
    } else {
        Ok(len)
    }
}

/// Decode one frame from the front of `bytes`.
pub fn decode_payload(bytes: &[u8]) -> Result<(&[u8], &[u8]), FrameError> {
    if bytes.len() < 4 {
        return Err(FrameError::Truncated);
    }
    let mut prefix = [0u8; 4];
    prefix.copy_from_slice(&bytes[..4]);
    let len = peek_frame_len(&prefix)?;
    let len_usize = len as usize;
    let end = 4 + len_usize;
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
}
