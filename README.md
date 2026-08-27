# PURGATORY

Custom-built native 2D side-scrolling MMORPG.

This repository is the project root. It contains a Cargo virtual workspace for a dedicated headless server, a native desktop client, shared crates, and development tools.

The master execution specification is [`PURGATORY_CURSOR_MASTER_EXECUTION_PLAN.md`](PURGATORY_CURSOR_MASTER_EXECUTION_PLAN.md). Follow it in phase order. Do not skip ahead.

## Current status

Phase 4.7 + 4.8 complete. Phase 5.0 networking foundation is complete (Quinn/QUIC handshake, ConnectionId, RTT, Network debug tab). No gameplay replication. Do not start Phase 5.1 until instructed.

Early runtime output is placeholders only. Existing art under `Graphic/` is reserved for later visual-content phases and is not loaded by the runtime. Phase 3 draws colored rectangles; the folder's logo is unused.

## Workspace

| Path | Crate | Role |
|---|---|---|
| `apps/client` | `purgatory-client` | Native desktop client |
| `apps/server` | `purgatory-server` | Headless dedicated server |
| `crates/common` | `purgatory-common` | Shared primitives |
| `crates/simulation` | `purgatory-simulation` | Authoritative simulation |
| `crates/protocol` | `purgatory-protocol` | Client/server protocol |
| `crates/content` | `purgatory-content` | Content definitions |
| `tools/content_validator` | `purgatory-content-validator` | Content validation tool |
| `tools/bot_client` | `purgatory-bot-client` | Headless load-test client |

## Requirements

- Stable Rust via `rustup` (see `rust-toolchain.toml`)
- `rustfmt` and `clippy` components

## Quality gate

Windows:

```powershell
./scripts/check.ps1
```

Linux / macOS:

```bash
./scripts/check.sh
```

The gate runs format check, `cargo check`, Clippy with warnings denied, and workspace tests.

Run the client window:

```powershell
cargo run -p purgatory-client
```

Development keys (client mapping only): **A / Left** move left, **D / Right** move right, **S / Down** hold down, **Space** jump, **Down + Jump** drop through OneWay. **Backquote / `~`** toggles the in-window development debug overlay (Network tab shows QUIC connection state). The client auto-connects to `127.0.0.1:5001`; if the server is down it stays in Disconnected and keeps rendering.

Run the headless server (listens on `127.0.0.1:5001` until Ctrl+C):

```powershell
cargo run -p purgatory-server
```

## Docs

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- [`docs/DECISIONS.md`](docs/DECISIONS.md)
- [`docs/PROTOCOL.md`](docs/PROTOCOL.md)
- [`docs/CONTENT_PIPELINE.md`](docs/CONTENT_PIPELINE.md)
- [`docs/PERFORMANCE_BUDGETS.md`](docs/PERFORMANCE_BUDGETS.md)
- [`docs/TEST_GATES.md`](docs/TEST_GATES.md)
- [`docs/ROADMAP.md`](docs/ROADMAP.md)
