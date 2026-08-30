# Protocol

Phase 5.2 adds **authoritative gameplay replication**. Protocol version is **10**. The client sends per-tick `InputCommand` values identified by `(input_epoch, sequence)`. Phase 5.3 is client-only remote interpolation. Phase 5.4 is client-only local prediction. Phase 5.5 adds acknowledgement, continuation debt, late-collapse compaction, and local restore+replay. Phase 5.6 adds a **development-only** network impairment lab (delay/stall/HOL on the existing reliable streams). Phase 5.7 adds off-protocol localhost load metrics and raises the mechanical entity decode bound to 256. Phase 6.0 adds runtime/replication **contracts**; 6A composition; **6B** adds reliable interaction control envelopes and optional `ReplicatedKind::Interactable`; **6C** adds observer `WorldAddress`, `ReplicatedKind::Portal`, and `PortalActivate`. **6D** replaces full `WorldSnapshot` on the gameplay uni stream with `ReplicationFrame` (Enter/Update/Leave) and server interest-policy AOI, then adds DEV-only `DevSetChannel` (tag 17) so a Channel change is an authoritative `WorldAddress` boundary. **6E** adds DEV `Hello.dev_login` (temporary lookup identity) and `DisconnectReasonCode::AlreadyConnected`. Phase **6F** adds server-side runtime services (scheduler, actions, staged events, effects, cadence) **without** a protocol bump: no v11, no new control tags, frame fields, or reject enums. Historical v1–v9 Hello/Welcome and v1–v7 snapshot goldens stay frozen. The server visibility set comes from `World::spatial_candidates` plus per-observer known-set classification. There is **no** combat or skill model yet. Health on v8 frames proves multi-domain deltas only.

## Trust boundary

**All client network input is untrusted.**

The server never assumes that:

- client values are valid
- client packet ordering is valid
- client packet frequency is reasonable
- client enum/discriminant values are safe
- client strings are a reasonable length
- client IDs refer to objects owned by that client

Client sends requests / intent. Server owns authoritative state.

Packet arrival must **not** directly modify authoritative game state.

```text
keyboard → InputCommand → decode → validate → session-owned input state
→ fixed simulation tick → existing World / FOOTNOTE
```

Permanent invariants:

- Remote peer input is untrusted.
- Network data never directly mutates `World`.
- The client does not authoritatively choose `ConnectionId`.
- The client cannot choose or control `EntityId`.
- One peer failure must remain isolated to that peer.
- Every peer-controlled variable is bounded before resource use.
- No network parser may panic on arbitrary bytes.
- Security enforcement is server-side.

## Version

`PROTOCOL_VERSION: u32 = 10` in `purgatory-protocol`. Independent from crate / game release version (`0.1.0`).

v10 is an intentional incompatible bump: v1–v9 peers are rejected with `DisconnectReasonCode::VersionMismatch`. Mismatches are never accepted silently. Hello is decoded **version-first**: a v9 Hello (no `dev_login` field) decodes, then fails version check — it is not treated as a malformed login.

Client Hello includes `protocol_version`. The server rejects mismatches with `DisconnectReasonCode::VersionMismatch`.

## Golden wire vectors

Protocol v1 vectors in `crates/protocol/tests/wire_golden.rs` are **frozen and unchanged**. They still encode `protocol_version = 1` so accidental v1 drift fails.

Protocol v2 vectors (`Hello` / `Welcome` / `InputCommand`) are **frozen and unchanged**.

Protocol v3 vectors (`Hello` / `Welcome` / v3 `WorldSnapshot`) are **frozen and unchanged**.

Protocol v4 vectors (`Hello` / `Welcome` / v4 `InputCommand` / `HeldCancel` / v4 `WorldSnapshot`) are **frozen and unchanged**.

Protocol v5 vectors (`Hello` / `Welcome` / interaction envelopes / v5 `WorldSnapshot` layout) are **frozen and unchanged**.

Protocol v6 adds observer `local_map` / `local_channel` / `local_instance` (`u32`×3) after `continuation_debt` on `WorldSnapshot`. A change is a client replication / interpolation / prediction baseline boundary. v6 `Hello` / `Welcome` / snapshot goldens live beside the frozen older vectors.

Protocol v7 adds snapshot `ReplicatedKind::Portal` (kind `3`) and client `PortalActivate` (tag 15). v6 Hello/Welcome/snapshot goldens remain frozen.

Protocol v8 Hello/Welcome goldens use `protocol_version = 8`. The gameplay uni payload is `ReplicationFrame` (tag **16**), not tag-7 `WorldSnapshot`. v1–v7 snapshot goldens remain frozen.

Protocol v9 Hello/Welcome goldens use `protocol_version = 9`. v9 adds DEV-only `DevSetChannel` (tag **17**, little-endian `u32` ChannelId). The client cannot mutate `WorldAddress`; the server validates Channel 0..=`DEV_CHANNEL_MAX` (currently 1), preserves MapId and InstanceId, relocates the live player, and bumps the observer replication epoch. v8 Hello/Welcome remain frozen.

Protocol v10 Hello goldens add `dev_login` after `client_build` (same `u8` length + UTF-8). Welcome layout is unchanged except `protocol_version = 10`. v9 Hello/Welcome remain frozen.

**Version change policy.** A failing golden vector means the wire format moved. Do not regenerate the fixture to make the test pass. Instead:

1. review protocol compatibility for existing peers
2. update `PROTOCOL_VERSION` if the change is incompatible
3. update the affected vectors deliberately, as part of that review
   Never silently rewrite old golden vectors.

## Endpoints

Development defaults live in `NetworkConfig` (do not scatter literals):

- host: `127.0.0.1`
- UDP port: `5001`
- ALPN: `purgatory`

## Timeouts

These live in `purgatory-protocol` (`config.rs`) and are applied by both endpoints. They are not the same timer.

- `HANDSHAKE_TIMEOUT` = 5 s: QUIC is up but Hello/Welcome does not complete. Classified as `HandshakeTimeout`.
- `IDLE_TIMEOUT` = 15 s: explicit Quinn `max_idle_timeout` on client and server. Transport liveness only — ping cadence is 1 s, so 15 s is long enough that localhost jitter never trips it and short enough for development. Not an AFK / “player idle” timer. Classified as `IdleTimeout`.
- `PING_INTERVAL` = 1 s: datagram nonce ping while Connected. Not a gameplay timer.

## Connection identity

`ConnectionId(u64)` is assigned by the server (monotonic `AtomicU64`, starting at 1). The client cannot choose it. It is not an `EntityId`, not a player id, and not a pointer. IDs are not reused during the lifetime of the server process, excluding `u64` wraparound. Disconnect does not reassign an id still held by another session. Wraparound is not handled at this scope; the allocator asserts if the counter wraps to `0`.

A later **MapInstanceId** (not implemented) will also be distinct: a server-authoritative instance of a world. Two connections in the same instance share that authoritative world. Private maps/dungeons will be separate server-owned instances, not client-local worlds.

A network session (`ConnectionSession`) is not a gameplay player. Insert/remove is exactly once (`SessionLease`). Handshake failures never insert. The accept loop spawns a per-connection task and continues; it does not await Hello. Concurrent handshake+session tasks are capped at 32; further `Incoming` values are refused. One stalled or malformed client does not block others.

## Framing (reliable control stream)

One QUIC write is **not** one message.

Layout: little-endian `u32` payload length, then exactly that many bytes.

- `MAX_CONTROL_MESSAGE_BYTES = 4096`
- length `0` or `> MAX` is rejected **before** allocating the payload
- truncated frames fail without panic

The client opens one bidirectional stream, sends `Hello`, and reads `Welcome` / `DisconnectReason` on that stream.

After Welcome, the server opens **one unidirectional stream** for `ReplicationFrame` payloads. Control/lifecycle traffic stays on the bidirectional stream. Frames use `MAX_GAMEPLAY_SNAPSHOT_BYTES` (8192), not `MAX_CONTROL_MESSAGE_BYTES` (4096). The stream is long-lived and ordered. The server does **not** `open_uni` per frame and does **not** reopen a sibling uni after write failure (that session is torn down). Soft encode budget: `REPLICATION_FRAME_BUDGET_BYTES` (4096), always ≤ the protocol wall.

**Phase 5.6 impairment (dev-only, off by default).** QUIC packet loss on these reliable streams is represented as added delay, jitter, temporary stall, and HOL burst release — not as disappearing `InputCommand` values. Sequence holes remain protocol/corruption tests. Snapshot skip, if enabled, is an explicit **application-level** keep-every-N filter, not transport packet loss. Client snapshot impairment delays `push_snapshot` after a successful uni read; it does not stall the server's QUIC send buffer or flow control.

## Messages (Phase 5.0 + 5.1 + 5.2)

Explicit little-endian tagged binary. No serde on the wire. String fields are length-prefixed `u8` + UTF-8, max `MAX_LABEL_BYTES = 64`.

### Client → server (control)

`Hello { protocol_version: u32, client_build: String, dev_login: String }` — tag 1. `dev_login` is present from protocol v10 (`HELLO_DEV_LOGIN_SINCE`). Older goldens omit it. The field is a temporary DEV lookup identity, not an account and not a `CharacterId`. Server validation: length `2..=32`, charset `[a-z0-9_.]`, reject `..`, path separators, leading/trailing `.`. Exact input is the lookup key. The client cannot choose `ConnectionId` or `CharacterId`.

`InputCommand { input_epoch: u16, sequence: u32, move_axis, jump_pressed, down_held, optional portal_held }` — tag 6. Frozen v2/v4 goldens omit `portal_held` (decoder treats absence as false). When Up is held, encode appends one `u8` flag (`1`). The server uses a falling edge of `portal_held` to clear the portal reentry lock. This is not movement and not a client-authoritative travel decision.

Intent only. Identity is `(input_epoch, sequence)`. Wire layout after the tag: `u32` LE sequence, `u16` LE epoch, `u8` move axis (`0=Left`, `1=Neutral`, `2=Right`), `u8` jump (0/1), `u8` down (0/1). Invalid axis or non-0/1 flags are `CodecError::InvalidValue`.

**Never on the wire:** position, velocity, grounded, collision results, platform id, `EntityId`, `ConnectionId` as identity, or client-computed physics.

`sequence` starts at 1 and increases by 1 per predicted simulation step within an epoch. **No wrap within a session.** First Accept of an epoch must be sequence 1. Duplicate (equal to last received) is ignored. Gap/stale/old-epoch are rejected.

The client emits **one command per predicted tick**. There is no send-on-change and no client-side coalescing on the reliable stream.

During a server-recognized Map/Channel transition the session is **input-gated** (ADR-0042). New-epoch `InputCommand` values are still accepted for sequence/ack so the replay window stays aligned, but they are applied as idle and must not adopt held movement. `PortalActivate` / `InteractOpen` / `DevSetChannel` are rejected while gated. No new wire tag; `input_epoch` bump already invalidates pre-transition commands.

`HeldCancel` — tag 8. Pathological focus-loss / full send-window barrier. No sequence. Rate-limited as gameplay input. Server handling is idempotent: idle held state, flush queue, cancel-ack `last_acknowledged := last_received`.

### Interaction control (Phase 6B)

Event/change-driven on the reliable control stream. Not per-tick snapshot spam. Client nearest-target is **advisory**; the server validates generation, address, range, and `Interactable` capability. The E candidate set is `ReplicatedKind::Interactable` only — `ReplicatedKind::Portal` is excluded. Portals use `PortalActivate` and the shared `in_portal_activation_zone` check.

Client:

- `InteractOpen { target: WireEntityId }` — tag 9
- `InteractClose { session_id: u32 }` — tag 10
- `PortalActivate { target: WireEntityId }` — tag 15. Edge-triggered portal travel. Not `InteractOpen`. `E` must not use this path.
- `DevSetChannel { channel: u32 }` — tag 17. DEV overlay only. Server-authoritative Channel request. Not a Portal, not a reconnect, and not client WorldAddress mutation. Channel values above `DEV_CHANNEL_MAX` are ignored (not a disconnect).

Server (`ServerControl::Interact`):

- Opened — tag 11 — `session_id` + target
- Rejected — tag 12 — target + reason (`TargetMissing`, `StaleId`, `WrongAddress`, `OutOfRange`, `NotInteractable`, `Unavailable`, `InvalidSession`)
- Updated — tag 13 — `session_id` + target
- Closed — tag 14 — `session_id` + reason (`Requested`, `TargetGone`, `AddressChanged`, `OutOfRange`, `Disconnected`). `AddressChanged` closes this **world-bound** `InteractionSession` when actor/target is no longer WorldAddress-compatible. It is not a generic “close every player-related session” signal (ADR-0040: WorldAddress boundary ≠ social identity boundary).

Load bots only need the version bump; they do not send interact.

### Acknowledgement and late-collapse

`last_acknowledged_input_sequence` on each per-recipient snapshot is the **accounted replay boundary**: every command with `sequence <= ack` in that epoch must not be replayed. A sequence may be acknowledged by a single Consumed tick, by late-collapse of a prefix, or by HeldCancel cancel-ack. It is not highest-received, highest-queued, or one `tick_player` per sequence.

**Late-collapse is intentional authoritative input compaction.** After Continuation starvation (`unmatched_continuation_ticks` / continuation debt), a prefix of delayed commands is applied as one `tick_player` (latest held + `jump_pressed` OR). Intermediate historical held commands may be acknowledged without receiving individual physics steps. Continuation debt saturates (`saturating_add` / `saturating_sub`) and never integer-wraps. Remainder debt is `K - N`.

`jump_pressed` OR during late-collapse is not a generic future skill-action policy. Future actions require explicit late-arrival semantics (ADR-0031).

### Server → client (control)

`Welcome { protocol_version, connection_id, server_tick_rate, server_label }`

`DisconnectReason { code, detail }` — **wire codes only**:

- `VersionMismatch`
- `Malformed`
- `HandshakeTimeout`
- `UnexpectedMessage`
- `ServerShutdown`
- `AlreadyConnected` (v10; duplicate live Character)

Local client failures (`ConnectFailed`, `TransportLost`, `IdleTimeout`, `ClientRequestedDisconnect`, `LocalShutdown`, `InternalNetworkError`) are **not** wire codes. They live in `NetworkFailureKind` on the client. The Connection Frontend and Network debug tab share one human-readable mapping. Quinn/rustls types never appear in UI state.

Welcome is sent **after** successful authoritative entry (identity, restore, occupancy, spawn, bind, replication-ready), not merely after protocol authentication. Empty/default replica values such as `MapId` 0 remain uninitialized sentinels.

Wire `detail` strings are bounded (`MAX_LABEL_BYTES`) and controlled (`no hello`, `decode`, `frame`, …). They must not contain file paths, panic text, task names, or parser dumps.

### Datagrams

`ClientDatagramPing { nonce: u64 }`

`ServerDatagramPong { nonce: u64 }`

The server echoes the nonce only. The client records `Instant` per outstanding nonce and computes RTT locally. **No client wall-clock / unix timestamps on the wire.** Cadence ≈ 1 s while Connected. Not used for gameplay timing.

At most **4** outstanding ping nonces. When the set is full, the oldest entry is dropped (lost Pongs cannot grow a map). Unknown, duplicate, or stale-attempt Pongs are ignored. A Pong never changes connection lifecycle, `ConnectionId`, or screen.

### Server → client (gameplay replication)

Historical v1–v7 `WorldSnapshot` is tag 7. It is **not** sent on the v8 uni stream. Frozen goldens still decode it.

`ReplicationFrame` — tag **16**, **not** a `ServerControl` value. It must not be decoded on the control stream.

```text
snapshot_sequence: u32
server_tick: u64
local_player_entity: { index: u32, generation: u32 }
input_epoch: u16
last_acknowledged_input_sequence: u32
local_grounded: u8
local_grounded_on: u16
local_ignored_platform: u16
continuation_debt: u16
local_map: u32
local_channel: u32
local_instance: u32
observer_baseline_epoch: u32
record_count: u16
records[]: Enter | Update | Leave
optional aoi_debug trailer (8 bytes): candidates, known, want_enter, want_leave as u16 LE
```

Record tags: Enter `1`, Update `2`, Leave `3`. Enter carries a full `SnapshotEntity` plus optional health. Update carries a domain mask (transform bit 0, health bit 1) and only those payloads. Leave carries `entity_id` only.

The optional 8-byte `ObserverAoiDebug` trailer is **DEV overlay counts** from the observer mailbox at encode time. It is not an application ACK and does not change interest. Frozen v8 goldens omit it (`aoi_debug: None`). Decoder: 0 leftover bytes → None; 8 leftover bytes → Some; any other leftover length → `InvalidValue`. Frame layout is unchanged in v9.

Header acknowledgement and contact remain **recipient-specific**. `last_committed_rev` is a server observer field that advances when a record is **accepted by the writer queue**, not a client ACK. 6D has no application ACK.

Leaves are encoded before Enters in a frame. Updates are only emitted for entities already Known (Enter already queue-committed). A missing Update does not delete. Client: `frame.epoch < replica.epoch` → ignore; `frame.epoch > replica.epoch` → clear replica then apply.

`write_all` failure on the persistent uni tears down the session. Do not skip the failed frame and continue later deltas on a new stream.

Static map platforms are **not** included.

Bounds (decode before allocate):

- `MAX_CONTROL_MESSAGE_BYTES` = 4096 (lifecycle/input)
- `MAX_GAMEPLAY_SNAPSHOT_BYTES` = 8192
- `MAX_ENTITIES_PER_SNAPSHOT` = 256 (mechanical decode bound / packet safety, not capacity; raised in Phase 5.7; layout unchanged)

Non-finite floats (NaN, ±Inf) are `InvalidValue`. Sequence policy matches input: first any value, then strictly greater; equal is duplicate; lower is stale; `u32` does not wrap within a server process. No rewind.

The frame builder receives **spatial candidates** from `World::spatial_candidates(observer)` (leave-rect + class; no hysteresis). Per-observer `ObserverReplicationState` applies enter/leave hysteresis and domain revisions. Static map platforms are not included. Do not assume “send every entity on the server”.

Datagrams are enabled via Quinn `datagram_receive_buffer_size`. Payloads larger than `MAX_DATAGRAM_BYTES` (256) are codec-rejected. Malformed datagrams are ignored (unreliable path).

Peer disappearance without a close frame is detected via the explicit `IDLE_TIMEOUT` (15 s). Tests simulate ungraceful loss by dropping the `Connection`; they do not require zero-delay cleanup. True OS process-kill without a close frame waits for that idle timeout.

`HANDSHAKE_TIMEOUT` (5 s) is different: the QUIC connection exists but Hello/Welcome does not complete. Do not classify both as a generic timeout.

## Failure response policy

| Failure | Action |
| --- | --- |
| ConnectFailed | cleanup + frontend `Connection failed` (retryable) |
| HandshakeTimeout | close + frontend `Handshake timed out` |
| IdleTimeout | cleanup + frontend `Connection lost` |
| TransportLost | cleanup + frontend `Connection lost` |
| VersionMismatch | Rejected, retryable=false |
| MalformedMessage | reject/close; server stays up; healthy peers unaffected |
| UnexpectedMessage | reject/close (known message in illegal lifecycle state) |
| ServerShutdown | controlled close code when possible; frontend `Server shutting down` |
| AlreadyConnected | reject new session; existing Character session stays; frontend `Already connected` |
| ClientRequestedDisconnect | clean close + frontend `Disconnected` (not a failure) |
| LocalShutdown | local teardown; not a failure; no "connection lost" |
| InternalNetworkError | log + local teardown + frontend `Connection lost` |

No undefined response. Local-only kinds are never serialized. Quinn/rustls types never appear in UI state.

## Abuse policy (Phase 5.0E)

Server-only `NetworkAbuseConfig` (not client-configurable).

QUIC already encrypts and integrity-protects the transport. PURGATORY still validates protocol, rate, ownership (later), and gameplay (later). Encryption is not trust.

| Class | Examples | Response |
| --- | --- | --- |
| Severe | oversized/zero control frame, invalid handshake, repeated Hello, forbidden direction | immediate reject/close; generic wire text |
| Tolerable | unknown/malformed datagram, unknown complete control tag, duplicate/stale ping | drop + per-connection counter; close that peer at budget/rate |

Control-message rate uses server `Instant` (8 / 1 s window, 16 drops then disconnect). **Gameplay `InputCommand` / `HeldCancel` has a separate policy** (development defaults: 128 / 1 s window covering 30 Hz plus catch-up, 256 drops then disconnect). The two limiters do not share counters. Invalid datagram budget 16. Malformed complete control budget 32. Handshake: one Hello, one bidi stream, no session until Welcome succeeds. Admission: 32 handshake/session tasks; excess `Incoming::refuse`. An input-rate offender is isolated; a healthy peer's movement continues.

Length prefix is checked against the stream's maximum (`MAX_CONTROL_MESSAGE_BYTES` or `MAX_GAMEPLAY_SNAPSHOT_BYTES`) before payload allocation. Strings: declared length, remaining bytes, UTF-8, `MAX_LABEL_BYTES`. Unknown discriminants return `CodecError`, never panic.

Wire details stay generic. Server logs may include sanitized (bounded, control-stripped) peer address text. Verbose traces must not dump raw payloads.

## Handshake

```text
QUIC connect
→ client opens bi stream, sends Hello
→ server validates type / size / version / string bounds (5 s timeout)
→ server assigns ConnectionId, sends Welcome, then inserts SessionTable
→ server opens one uni stream for ReplicationFrame
→ client enters Connected
```

Invalid Hello: `DisconnectReason` when a stream exists, then close. No Hello in time: close, no session entry. Repeated Hello after Welcome: `UnexpectedMessage` + close.

### Shutdown during Handshaking

A peer that shuts down after QUIC connect but before Welcome completes never becomes a session. Live-QUIC regression tests cover both sides: the client network owner shutting down while genuinely in `Handshaking` (real transport, Hello already sent, Welcome withheld) must join its network thread, and the server's partial handshake must release its admission permit and leave the session table empty, then accept a healthy client immediately afterwards.

The server currently counts an abandoned pre-Hello handshake as `malformed` (the control stream never arrives). That is a diagnostic-labelling nuance, not a resource leak; peer isolation, admission capacity, and session state are unaffected.

## Startup and bind failure

Bind happens before the server declares readiness: `network listening on <addr>` is printed only after `Endpoint::server` succeeds. A bind failure (for example the port already in use) is an expected startup failure, not a bug:

- `endpoint::bind` returns `Err("failed to bind <addr>: <io error>")`; the low-level Quinn/IO error stays inside the networking boundary
- the top level prints one concise line and exits with a failure status
- no panic backtrace, no half-started listener, no retry loop, no silent exit

## Loss and shutdown semantics (Phase 5.0F)

| Event | Wire | Server classification | Client classification |
| --- | --- | --- | --- |
| Client closes gracefully | QUIC application close | `clean_dc` (never transport loss) | `ClientRequestedDisconnect` / `LocalShutdown` |
| Client vanishes (no close frame) | none | `transport_loss` after `IDLE_TIMEOUT` | `IdleTimeout` / `TransportLost` |
| Server shuts down gracefully | `ServerShutdown` close code | sessions cleaned before exit | `ServerShutdown` |
| Server vanishes | none | n/a (process gone) | `IdleTimeout` / `TransportLost` — never a fabricated `ServerShutdown` |
| Zero-length control frame | close, generic text | `malformed` (severe) | `MalformedMessage` |

A `ConnectionId` is never reused within a server process and never resurrected across a restart: a restarted server allocates its own ids from `ConnectionId::FIRST`, and clients must reconnect explicitly with a fresh `ConnectionAttemptId`.

Cleanup latency: a graceful close is observed as soon as the peer's close frame arrives. A silent disappearance is bounded by the negotiated idle timeout (`IDLE_TIMEOUT` = 15 s in production; tests shorten it). Handshake stalls are bounded by `HANDSHAKE_TIMEOUT` and never create a session entry.

## Development certificates

The server generates an in-memory self-signed certificate at startup (`generate_dev_only_self_signed`). Keys stay in process memory. **DEV ONLY.**

The client uses `DevOnlySkipServerVerification`. **DEV ONLY.** No production flag maps to skip-verify.

## Production TODO (not implemented)

- real certificate / server identity verification
- accounts / authentication
- production DDoS / IP-ban infrastructure
- operational logging
- version compatibility policy beyond exact `PROTOCOL_VERSION` match
- combat, skills (later phases)
- production gameplay anti-cheat beyond input validation and rate policy

## Off-protocol load metrics (Phase 5.7)

Development-only. **Not** part of the QUIC control/gameplay codec.

- UDP localhost `127.0.0.1:5002` (override with `PURGATORY_METRICS_PORT`)
- Datagram: magic `PURGSTAT` + version `1` + bounded JSON (`LoadMetricsV1` in `purgatory-common`)
- Schema **3** adds scheduler/action/event gauges plus per-observer pending Enter/Update and cadence-deferred Updates. New fields use `serde(default)` so older polls still decode. There is no global “dirty pending” gauge: `domain_rev_advances` is World-side; `observer_pending_*` is summed from this tick’s observers.
- Missed poll must not be treated as zeros
- Admission: default 32; load mode `PURGATORY_ADMISSION_CAP=256`

See [`docs/PHASE_57_LOAD_TESTING.md`](PHASE_57_LOAD_TESTING.md).

## Authority rule (Phase 5.1)

The client sends intent only. The server owns movement. Each active gameplay session has one authoritative player:

```text
ConnectionId → EntityId
```

They remain different identities. The server assigns the entity. Disconnect / transport loss despawns the player, drops input state, and removes the binding. Reconnect starts Neutral, down=false, no pending jump, fresh sequence.

Packet arrival never ticks `World`. Network tasks `try_send` on bounded lifecycle/input channels. The simulation owner drains, then on each fixed tick: apply latest validated input → resolve `PlayerInput` → existing FOOTNOTE `tick_player` → consume the jump edge.

The desktop client may still run local FOOTNOTE for **LOCAL DEV / NON-AUTHORITATIVE** prediction groundwork. Phase 5.2 rendering uses the replica, not that local player body.

## Command versus Event (Phase 6F)

Client envelopes (`InputCommand`, `InteractOpen`, `PortalActivate`, `DevSetChannel`) are **commands**: untrusted requests. They are not runtime facts.

Authoritative occurrences are staged `RuntimeEvent` values inside simulation (spawn/despawn, action start/end/reject, effect apply/expire, scheduled fire, cadence). They are not a second wire codec. Protocol stays **v10**. Phase 6G did not add Welcome `CharacterId` or test-only replica fields.

```text
invalid command → typed reject (existing Unavailable where a wire response already exists)
stale runtime target → controlled no-op/cancel
```

“Arrived ⇒ valid” is not a contract. Parse and validate before session insert or gameplay handoff.

## Authoritative snapshots (Phase 5.2 / 6D / 6F)

```text
InputCommand → server tick → World → ObserverReplicationState → ReplicationFrame
→ bounded epoch-tagged writer queue → persistent uni write_all
→ uni-stream → client ReplicatedWorld → renderer
```

The client never sends snapshot or transform state back as input. Seeing another `EntityId` does not authorize controlling it.

**Dirty/delta (ADR-0049).** AOI answers **who** may need state. `DomainRevs` are World change versions. Per-observer `CommittedRevs` answer whether **that** observer still needs data.

- Enter = baseline when the entity becomes relevant (AOI entry or epoch reset)
- Update = observer lags World revs **and** cadence allows
- Unchanged Known entities produce no Update
- Leave = AOI exit; re-enter is a new Enter baseline
- One observer’s writer-queue commit must not drop another observer’s required Update/Enter
- `DirtyFlags` / `consume_dirty` are not the multi-client replication contract
- Visible ≠ full replicate every tick. Cadence Normal/Low is staggered by entity index, not a global `tick % n == 0`
