//! Wire protocol version. Independent from crate / game release version.

/// Incompatible versions must be rejected at handshake. Never accept silently.
///
/// Protocol v2 adds `InputCommand`. Protocol v3 adds `WorldSnapshot`
/// (authoritative replication). Protocol v4 adds input epoch, per-recipient
/// acknowledgement / contact header fields, and `HeldCancel`. Historical
/// golden vectors remain frozen.
pub const PROTOCOL_VERSION: u32 = 4;
