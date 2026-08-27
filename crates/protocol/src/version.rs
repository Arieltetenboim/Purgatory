//! Wire protocol version. Independent from crate / game release version.

/// Incompatible versions must be rejected at handshake. Never accept silently.
pub const PROTOCOL_VERSION: u32 = 1;
