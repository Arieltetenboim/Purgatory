# PURGATORY

Custom-built native 2D side-scrolling MMORPG.

This repository is the project root. It contains a Cargo virtual workspace for a dedicated headless server, a native desktop client, shared crates, and development tools.

The master execution specification is [`PURGATORY_CURSOR_MASTER_EXECUTION_PLAN.md`](PURGATORY_CURSOR_MASTER_EXECUTION_PLAN.md). Follow it in phase order. Do not skip ahead.

## Current status

Phase 4.7 + 4.8 complete. Phase 5.0 networking foundation is complete through 5.0F. **Phase 5.1–5.7 are complete and GREEN.** **Phase 6.0 / 6A are GREEN.** **Phase 6B is automated GREEN; manual E/overlay interaction check is still required.** **Phase 6C is automated GREEN; manual portal check is still required.** **Phase 6D is automated GREEN; manual runtime check is still required.** **Phase 6E is automated GREEN; manual first-connect / reconnect / restart / two-login / reject-duplicate / sentinel replica check is still required.** **Phase 6F is automated GREEN; manual two-client dirty/AOI, overlay Enter/Update/Leave, and opt-in `PURGATORY_RUNTIME_PROBE` check is still required.** **Phase 6G is GREEN** (architecture closed; production replication-policy tuning deferred; root `PHASE` = `6G`). Phase 7 is planned and **not started**. Manual Mixed/soak/Developer Tools Runtime Validation and Windows server-Stop/exe-replace checks remain part of the evidence track. Protocol is **v10** (`Hello.dev_login`). Impairment is **off by default**. Server remains authoritative. File-backed characters live under `%LOCALAPPDATA%\Purgatory\` on Windows (`PURGATORY_DATA_DIR` override; Hub-spawned servers default to `logs/dev-tools/hub_server_persist`). Load validation uses `PURGATORY_LOAD_VALIDATION` only in load-mode; isolated runs must not use the developer persist tree. Capacity domain timings use `PURGATORY_CAPACITY_ARTIFACT_DIR` (UDP metrics stay schema 4). The repository is not the writable persist root. Do not begin Phase 7 (MOB). Authoritative 5.7 evidence: `logs/load/capacity/20260829_002013/steady_input_final_report.md`. Load testing guide: [`docs/PHASE_57_LOAD_TESTING.md`](docs/PHASE_57_LOAD_TESTING.md). Baseline freeze: [`docs/MMO_RUNTIME_BASELINE.md`](docs/MMO_RUNTIME_BASELINE.md). Two-client Phase 5.6 matrix: [`docs/PHASE_56_LATENCY_MATRIX.md`](docs/PHASE_56_LATENCY_MATRIX.md). Phase 6G report: [`docs/PHASE_6G_REPORT.md`](docs/PHASE_6G_REPORT.md). Phase 6G.5 report: [`docs/PHASE_6G5_REPORT.md`](docs/PHASE_6G5_REPORT.md). Phase 6G.6 report: [`docs/PHASE_6G6_REPORT.md`](docs/PHASE_6G6_REPORT.md). Phase 6G.7A report: [`docs/PHASE_6G7A_REPORT.md`](docs/PHASE_6G7A_REPORT.md). Phase 6 exit review: [`docs/PHASE_6_EXIT_REVIEW.md`](docs/PHASE_6_EXIT_REVIEW.md).
Early runtime output is placeholders only. `Graphic/LOGO.png` is loaded once for the Connection Frontend; the rest of `Graphic/` stays unused until later visual-content phases.

## Workspace

| Path | Crate | Role |
|---|---|---|
| `apps/client` | `purgatory-client` | Native desktop client |
| `apps/server` | `purgatory-server` | Headless dedicated server |
| `apps/dev_hub` | `purgatory-dev-hub` | Developer Hub GUI (provisional eframe) |
| `crates/common` | `purgatory-common` | Shared primitives |
| `crates/simulation` | `purgatory-simulation` | Authoritative simulation |
| `crates/protocol` | `purgatory-protocol` | Client/server protocol |
| `crates/content` | `purgatory-content` | Content definitions |
| `crates/persistence` | `purgatory-persistence` | File-backed character identity and persistence |
| `crates/dev_runtime` | `purgatory-dev-runtime` | Headless Developer Hub orchestration |
| `tools/content_validator` | `purgatory-content-validator` | Content validation tool |
| `tools/bot_client` | `purgatory-bot-client` / `purgatory-load` | Headless QUIC load harness |

## Developer Tools

Windows: run [`DEV.BAT`](DEV.BAT) at this repository root. That opens the current PowerShell Developer Tools shell (fallback). The Rust Developer Hub (server lifecycle, Runtime Validation, load/soak, clients, quality gate, Rebuild, Kill All, settings) is [`DEV_HUB.BAT`](DEV_HUB.BAT): it builds if needed, launches `purgatory-dev-hub.exe`, and exits so the Hub is not bound to that console. Do not use `cargo run -p purgatory-dev-hub` as the normal launch path (that keeps the Hub under cargo’s process job). Closing the Hub does not stop the dedicated server or detached clients. A second Hub for the same workspace is refused (`logs/dev-tools/hub.lock`). Do not drive the same workspace from both shells at once.

See [`docs/dev-tools/README.md`](docs/dev-tools/README.md) and [`docs/dev-tools/PARITY.md`](docs/dev-tools/PARITY.md).

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

The gate runs format check, `cargo check`, Clippy with warnings denied, workspace tests, and `purgatory-content-validator`. It stays fast; the longer network soaks are `#[ignore]` and run separately:

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

Development keys (client mapping only, **Game screen**): **A / Left** move left, **D / Right** move right, **S / Down** hold down, **Space** jump, **Down + Jump** drop through OneWay. **Backquote / `~`** toggles the in-window development debug overlay (Network tab shows QUIC session identity, latest RTT, last failure category, and Connect/Disconnect). The client starts on the Connection Frontend and does **not** auto-connect. Click **CONNECT** to `127.0.0.1:5001`. The Connection Frontend has a **DEV login** field (default `dev.local`). Same login restores the same Character after reconnect or server restart. A second client with that login is rejected (`Already connected`) while the first session is live. If the server is down the client stays alive, CONNECT remains retryable, and the frontend shows a short status (`Connection failed`, `Version mismatch`, `Already connected`, `Connection lost`, …) rather than Quinn/rustls text.

Run the headless server (listens on `127.0.0.1:5001` until Ctrl+C):

```powershell
cargo run -p purgatory-server
```

Optional DEV runtime probe (off by default). When enabled, the server schedules one visible Generic entity ~1 s after map-ready; it is not normal runtime behavior:

```powershell
$env:PURGATORY_RUNTIME_PROBE = "1"
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
- [`docs/PHASE_56_LATENCY_MATRIX.md`](docs/PHASE_56_LATENCY_MATRIX.md)
- [`docs/PHASE_57_LOAD_TESTING.md`](docs/PHASE_57_LOAD_TESTING.md)
