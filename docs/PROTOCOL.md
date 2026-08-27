# Protocol

Phase 5.0 implements the transport/session foundation only. There is **no gameplay replication**: no InputCommand, no snapshots, no remote players.

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

Packet arrival must **not** directly modify authoritative game state. Future gameplay flow:

```text
packet → parse → validate → semantic request → server simulation
```

Phase 5.0 does not implement gameplay commands. FOOTNOTE movement remains local/simulation-side.

## Version

`PROTOCOL_VERSION: u32 = 1` in `purgatory-protocol`. Independent from crate / game release version (`0.1.0`).

Client Hello includes `protocol_version`. The server rejects mismatches with `DisconnectReasonCode::VersionMismatch`. Mismatches are never accepted silently.

## Endpoints

Development defaults live in `NetworkConfig` (do not scatter literals):

- host: `127.0.0.1`
- UDP port: `5001`
- ALPN: `purgatory`

## Connection identity

`ConnectionId(u64)` is assigned by the server (monotonic, starting at 1). The client cannot choose it. It is not an `EntityId`, not a player id, and not a pointer. Unique among *active* connections.

A network session (`ConnectionSession`) is not a gameplay player.

## Framing (reliable control stream)

One QUIC write is **not** one message.

Layout: little-endian `u32` payload length, then exactly that many bytes.

- `MAX_CONTROL_MESSAGE_BYTES = 4096`
- length `0` or `> MAX` is rejected **before** allocating the payload
- truncated frames fail without panic

The client opens one bidirectional stream, sends `Hello`, and reads `Welcome` / `DisconnectReason` on that stream.

## Messages (Phase 5.0)

Explicit little-endian tagged binary. No serde on the wire. String fields are length-prefixed `u8` + UTF-8, max `MAX_LABEL_BYTES = 64`.

### Client → server (control)

`Hello { protocol_version: u32, client_build: String }`

### Server → client (control)

`Welcome { protocol_version, connection_id, server_tick_rate, server_label }`

`DisconnectReason { code, detail }` — **wire codes only**:

- `VersionMismatch`
- `Malformed`
- `HandshakeTimeout`
- `UnexpectedMessage`
- `ServerShutdown`

Local client failures (`ConnectFailed`, transport errors, `ClientClosed`) are **not** wire codes. They live in `LocalConnectionError` on the client. The Network debug tab may show either as one status string.

### Datagrams

`ClientDatagramPing { nonce: u64 }`

`ServerDatagramPong { nonce: u64 }`

The server echoes the nonce only. The client records `Instant` per outstanding nonce and computes RTT locally. **No client wall-clock / unix timestamps on the wire.** Cadence ≈ 1 s while Connected. Not used for gameplay timing.

Datagrams are enabled via Quinn `datagram_receive_buffer_size`. Malformed datagrams are ignored (unreliable path).

## Handshake

```text
QUIC connect
→ client opens bi stream, sends Hello
→ server validates type / size / version / string bounds (5 s timeout)
→ server assigns ConnectionId, inserts session, sends Welcome
→ client enters Connected
```

Invalid Hello: `DisconnectReason` when a stream exists, then close. No Hello in time: close, no session entry. Repeated Hello after Welcome: `UnexpectedMessage` + close.

## Development certificates

The server generates an in-memory self-signed certificate at startup (`generate_dev_only_self_signed`). Keys are not committed.

The client uses `DevOnlySkipServerVerification`. **DEV ONLY.** Do not treat this as production trust.

## Production TODO (not implemented)

- real certificate / server identity verification
- accounts / authentication
- rate limiting and abuse monitoring
- operational logging
- version compatibility policy beyond exact `PROTOCOL_VERSION` match

## Authority rule (unchanged)

The client never sends authoritative position, damage, item grants, XP, or death results. Phase 5.0 does not send those fields at all.
