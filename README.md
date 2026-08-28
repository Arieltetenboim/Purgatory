# PURGATORY

Custom-built native 2D side-scrolling MMORPG.

This repository is the project root. It contains a Cargo virtual workspace for a dedicated headless server, a native desktop client, shared crates, and development tools.

The master execution specification is [`PURGATORY_CURSOR_MASTER_EXECUTION_PLAN.md`](PURGATORY_CURSOR_MASTER_EXECUTION_PLAN.md). Follow it in phase order. Do not skip ahead.

## Current status

Phase 4.7 + 4.8 complete. Phase 5.0 networking foundation is complete through 5.0F. **Phase 5.1–5.5 are complete:** intent input, snapshots, remote interpolation, local prediction, and authoritative input acknowledgement with local restore+replay. Protocol is **v4**. Server remains authoritative. Do not start Phase 5.6 (latency lab / combat / skills) until instructed. Two-client manual verification of Phase 5.5 is the current stop.

Early runtime output is placeholders only. `Graphic/LOGO.png` is loaded once for the Connection Frontend; the rest of `Graphic/` stays unused until later visual-content phases.

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

The gate runs format check, `cargo check`, Clippy with warnings denied, and workspace tests. It stays fast; the longer network soaks are `#[ignore]` and run separately:

```powershell
./scripts/network_soak.ps1
```

```bash
./scripts/network_soak.sh
```

That runs only the extended `#[ignore]` soaks (1000 sequential connect/disconnect cycles, 10×100 multi-client churn, an 8-seed deterministic chaos matrix, admission churn, server restarts, sustained ping cadence). Localhost only; it changes no project state. Individual soaks can be run directly, for example:

```powershell
cargo test -p purgatory-server network::tests::sequential_churn_1000_soak -- --ignored --exact --nocapture
```

Run the client window:

```powershell
cargo run -p purgatory-client
```

Development keys (client mapping only, **Game screen**): **A / Left** move left, **D / Right** move right, **S / Down** hold down, **Space** jump, **Down + Jump** drop through OneWay. **Backquote / `~`** toggles the in-window development debug overlay (Network tab shows QUIC session identity, latest RTT, last failure category, and Connect/Disconnect). The client starts on the Connection Frontend and does **not** auto-connect. Click **CONNECT** to `127.0.0.1:5001`. If the server is down the client stays alive, CONNECT remains retryable, and the frontend shows a short status (`Connection failed`, `Version mismatch`, `Connection lost`, …) rather than Quinn/rustls text.

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
