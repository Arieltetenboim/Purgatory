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
/// Protocol v11 adds `ReplicatedKind::Npc` so visible Generics/NPCs Enter
/// AOI on the wire (Phase 7.2). Protocol v12 adds equipment request envelopes
/// and an equipment domain on Enter/Update. Protocol v13 adds DEV presentation
/// Attack/Hurt oneshot control envelopes (tags 22/23); Enter/Update snapshot
/// layout unchanged (0 per-frame animation traffic). Protocol v14 adds DEV-only
/// `DevResetPlayer` (tag 24): server-authoritative spawn reset for the bound
/// player. Protocol v15 adds ability activation envelopes (tags 25–27): client
/// `AbilityActivate` (ability id + optional selected entity) and server
/// `Ability` accepted/rejected. Protocol v16 adds client `Respawn` (tag 28).
/// Protocol v17 adds the authoritative damage-immunity bit to replicated Health.
/// Protocol v18 adds DEV-authoritative player speed control (tag 29).
/// Protocol v19 adds DEV-authoritative player jump control (tag 30).
/// Protocol v21 removes the unused `jump_held` short-hop field from the input
/// intent after fixed-height jump behavior was retained. Protocol v22 adds the
/// reliable authoritative world-drop pickup contract.
/// Protocol v23 changes `EquipRequest` to carry the exact owned
/// `ItemInstanceId` rather than a client-selected `ContentId`.
/// Protocol v24 adds the Item world-drop replication kind used by Phase 11E.
/// Protocol v25 adds authoritative dialogue advance and active-line control
/// envelopes (tags 35–36). Text remains client-side presentation content.
/// Protocol v26 adds DEV-only `DevSpawnNpc` (tag 37), carrying stable NPC
/// `ContentId`; the server owns lookup, address, position and runtime identity.
/// Historical golden vectors remain frozen.
pub const PROTOCOL_VERSION: u32 = 26;

/// `Hello` includes `dev_login` from this version onward. Older goldens omit it.
pub const HELLO_DEV_LOGIN_SINCE: u32 = 10;

/// Highest `ChannelId` the DEV overlay may request. Not a capacity, allocation,
/// or persistence policy.
pub const DEV_CHANNEL_MAX: u32 = 1;
