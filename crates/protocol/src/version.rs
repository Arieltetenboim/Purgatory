//! Wire protocol version. Independent from crate / game release version.

/// Incompatible versions must be rejected at handshake. Never accept silently.
///
/// Protocol v2 adds `InputCommand`. Protocol v3 adds `WorldSnapshot`
/// (authoritative replication). Protocol v4 adds input epoch, per-recipient
/// acknowledgement / contact header fields, and `HeldCancel`. Protocol v5 adds
/// reliable interaction control envelopes and optional interactable snapshot
/// kinds. Protocol v6 adds observer `WorldAddress` (map/channel/instance) on
/// each snapshot; a change is a client replication baseline boundary.
/// Protocol v7 adds `ReplicatedKind::Portal` and client `PortalActivate`.
/// Protocol v8 replaces full `WorldSnapshot` on the gameplay uni stream with
/// `ReplicationFrame` (Enter/Update/Leave, `observer_baseline_epoch`, optional
/// health domain). Protocol v9 adds DEV-only `DevSetChannel` (tag 17) so a
/// ChannelId change is an authoritative `WorldAddress` boundary. Protocol v10
/// adds DEV `Hello.dev_login` (temporary lookup identity; not an account).
/// Historical golden vectors remain frozen.
pub const PROTOCOL_VERSION: u32 = 10;

/// `Hello` includes `dev_login` from this version onward. Older goldens omit it.
pub const HELLO_DEV_LOGIN_SINCE: u32 = 10;

/// Highest `ChannelId` the DEV overlay may request. Not a capacity, allocation,
/// or persistence policy.
pub const DEV_CHANNEL_MAX: u32 = 1;
