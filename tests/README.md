# Tests

Phase 0 tests live beside each crate and binary:

- `crates/*/src/lib.rs` unit tests
- `apps/*/src/main.rs` linkage tests
- `tools/*/src/main.rs` linkage tests

Workspace command:

```text
cargo test --workspace
```

Integration and network handshake tests live beside `apps/server/src/network/tests.rs` (localhost Quinn). Protocol unit tests live in `crates/protocol` (including truncation/oversize matrices and a deterministic random decoder corpus). Protocol v1 **golden wire vectors** live in `crates/protocol/tests/wire_golden.rs` and must not be silently rewritten. Protocol v2 `InputCommand` and v3 `WorldSnapshot` vectors live in the same file; historical vectors stay frozen. Phase 5.1 gameplay-owner tests live in `apps/server/src/network/gameplay.rs`; live Quinn input/ownership/snapshot/despawn/backpressure tests are in `tests.rs`. Client replica apply tests live in `apps/client/src/replica.rs`. Client remote interpolation tests live in `apps/client/src/interp.rs` (math, monotonic clock, spawn/despawn timeline, generation reuse, underrun, teleport snap, jitter/skip). Client local prediction tests live in `apps/client/src/prediction.rs` (anchor, move/jump, focus-loss, generation reset, bounded catch-up, OneWay drop-through, replica isolation). Do not add reconciliation/input-replay harnesses before Phase 5.5.

## Network soak, stress, and chaos (Phase 5.0F)

The criterion is **convergence, not survival**: after bounded pressure, active gauges must return to baseline and a healthy probe must pass.

- **Active gauges** (must return to baseline): active sessions, active handshakes, inflight connection tasks, admission permits in use. Asserted by the bounded `wait_until_baseline` helper, which fails with a gauge and counter report instead of sleeping.
- **Cumulative counters and high-water marks** (never return to zero): accepted, rejected, mismatch, malformed, oversized, handshake timeouts, rate-limited, admission refusals, clean disconnects, transport losses, `max_sessions`, `max_inflight`, `max_handshakes`.
- **Healthy probe** (mandatory after every chaos family): connect → Welcome → ping/pong → clean disconnect → baseline.
- **Deterministic chaos**: a test-only operation vocabulary driven by a seeded LCG. CI seeds are `0x1`, `0xC0FFEE`, `0xDEADBEEF`; the seed and operation index are printed on failure. No system-entropy seeding.

Default CI scale (whole server suite ≈ 2 s): 20 and 50 sequential churn cycles, 30 rapid reconnects, 4×5 and 5×8 multi-client cycles, 8 concurrent healthy clients, admission cap 4 fill/refuse/release ×3, stalled and mixed-peer pressure, 40 malformed handshakes, 16 version mismatches, per-connection datagram/rate budget repeats, repeated idle and handshake timeouts, abrupt drops, graceful and abrupt server loss, server restart, 3 chaos seeds × 40 operations. Client-side: telemetry saturation vs lifecycle delivery, command-queue pressure, shutdown-during-connect rounds, disconnect/shutdown race rounds, Welcome/disconnect race soak (3 seeds × 200 rounds), stale-event chaos soak, 500-cycle diagnostic history bound.

Extended `#[ignore]` soaks (≈68 s total) run separately:

```powershell
./scripts/network_soak.ps1
```

```bash
./scripts/network_soak.sh
```

They cover 1000 sequential cycles, 10×100 multi-client churn, 200 malformed handshakes, 20 admission fill/drain rounds, 4 server restart rounds, sustained ping cadence, and an 8-seed × 120-operation chaos matrix. A single soak can be run directly with `cargo test -p purgatory-server network::tests::<name> -- --ignored --exact --nocapture`.

## Protocol golden vectors

`cargo test -p purgatory-protocol --test wire_golden` asserts fixed Protocol v1 byte arrays in both directions: expected message → exact bytes, and exact bytes → expected message. They cover `Hello`, `Welcome`, `DisconnectReason`, the Ping and Pong datagrams, and the complete framed form of two control messages, and they pin little-endian integers, `u8`-length + UTF-8 strings, and field order.

The fixtures cannot update themselves and there is no snapshot file. A failure means the wire format moved: review compatibility and `PROTOCOL_VERSION` before touching a vector (see `docs/PROTOCOL.md`).

## Handshake shutdown and bind failure

- **Live QUIC shutdown during `Handshaking`** (`apps/client/src/network/runtime.rs`): a test-only listener completes the real transport handshake, reads the Hello, and never answers Welcome. The client must reach the real `Handshaking` state and then join its network thread within a bounded timeout. 6 rounds in CI; a 25-round `#[ignore]` variant runs in the extended soak.
- **Interrupted server handshakes** (`apps/server/src/network/tests.rs`): peers that vanish before Welcome, after Hello, and at a full admission cap must leave no session, return every admission permit, converge to baseline, and be followed by a healthy probe.
- **Bind failure**: an ephemeral localhost port is reserved with a temporary socket, then the server is asked to bind the same address. Expected: a typed `Err("failed to bind <addr>: …")`, no panic, no partial state, and a clean bind once the port is released. The fixed dev port is never used in automated tests.

These tests do not prove internet DDoS resistance, production TLS/PKI, authentication, cheat resistance, MMO population capacity, persistence durability, cross-region latency, NAT edge cases, mobile/browser transport, or real packet loss. They are localhost network-foundation hardening only.
