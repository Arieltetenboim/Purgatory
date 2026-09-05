# PURGATORY

Custom-built native 2D side-scrolling MMORPG.

This repository is the project root. It contains a Cargo virtual workspace for a dedicated headless server, a native desktop client, shared crates, and development tools.

The master execution specification is [`PURGATORY_CURSOR_MASTER_EXECUTION_PLAN.md`](PURGATORY_CURSOR_MASTER_EXECUTION_PLAN.md). Follow it in phase order. Do not skip ahead.

## Current status

Phase 4.7 + 4.8 complete. Phase 5.0 networking foundation is complete through 5.0F. **Phase 5.1–5.7 are complete and GREEN.** **Phase 6.0 / 6A are GREEN.** **Phase 6B is automated GREEN; manual E/overlay interaction check is still required.** **Phase 6C is automated GREEN; manual portal check is still required.** **Phase 6D is automated GREEN; manual runtime check is still required.** **Phase 6E is automated GREEN; manual first-connect / reconnect / restart / two-login / reject-duplicate / sentinel replica check is still required.** **Phase 6F is automated GREEN; manual two-client dirty/AOI, overlay Enter/Update/Leave, and opt-in `PURGATORY_RUNTIME_PROBE` check is still required.** **Phase 6 is closed:** **Phase 6G is GREEN** (architecture closed; production replication-policy **tuning** deferred, not an architecture reopen). Root `PHASE` = `9E` (**Phase 9E Minimal Creature Combat Driver**; 9A–9D complete; Phase 8 complete + closeout; 8A–8F-F complete; Phase 7 closeout complete). Closeout: [`docs/PHASE_8_CLOSEOUT_REPORT.md`](docs/PHASE_8_CLOSEOUT_REPORT.md). **9A:** [`docs/PHASE_9A_REPORT.md`](docs/PHASE_9A_REPORT.md). **9B:** [`docs/PHASE_9B_REPORT.md`](docs/PHASE_9B_REPORT.md). **9C:** [`docs/PHASE_9C_REPORT.md`](docs/PHASE_9C_REPORT.md). **9D:** [`docs/PHASE_9D_REPORT.md`](docs/PHASE_9D_REPORT.md). **9E:** [`docs/PHASE_9E_REPORT.md`](docs/PHASE_9E_REPORT.md). **7.1–7.8 complete** ([`docs/PHASE_78_REPORT.md`](docs/PHASE_78_REPORT.md)). Closeout: [`docs/PHASE_7_WEAK_CLIENT_AUTHORITY_AUDIT.md`](docs/PHASE_7_WEAK_CLIENT_AUTHORITY_AUDIT.md). Canonical capacity regression: `./scripts/phase_78_gate.ps1`. Phase 7 is **capacity, parallelism & production scaling** ([`docs/PHASE_7_PLAN.md`](docs/PHASE_7_PLAN.md)). Residual 6B–6F two-client manuals are evidence debt, not 6G reopeners. Protocol is **v15**. Impairment is **off by default**. Server remains authoritative. File-backed characters live under `%LOCALAPPDATA%\Purgatory\` on Windows (`PURGATORY_DATA_DIR` override; Hub-spawned servers default to `logs/dev-tools/hub_server_persist`). Load validation uses `PURGATORY_LOAD_VALIDATION` only in load-mode; isolated runs must not use the developer persist tree. Capacity domain timings use `PURGATORY_CAPACITY_ARTIFACT_DIR` (includes `gameplay_workload.json`). Live UDP `PURGSTAT` remains the **schema-3** runtime metrics contract; ownership stays in **file artifacts**. The repository is not the writable persist root. **Phase 9E** is complete (live creature Basic Strike driver through existing Ability Runtime; protocol **v15** unchanged). Authoritative 5.7 evidence: `logs/load/capacity/20260829_002013/steady_input_final_report.md`. Load testing guide: [`docs/PHASE_57_LOAD_TESTING.md`](docs/PHASE_57_LOAD_TESTING.md). Baseline freeze: [`docs/MMO_RUNTIME_BASELINE.md`](docs/MMO_RUNTIME_BASELINE.md). Two-client Phase 5.6 matrix: [`docs/PHASE_56_LATENCY_MATRIX.md`](docs/PHASE_56_LATENCY_MATRIX.md). Phase 6G report: [`docs/PHASE_6G_REPORT.md`](docs/PHASE_6G_REPORT.md). Phase 6G.7C close: [`docs/PHASE_6G7C_REPORT.md`](docs/PHASE_6G7C_REPORT.md). Phase 6 exit review: [`docs/PHASE_6_EXIT_REVIEW.md`](docs/PHASE_6_EXIT_REVIEW.md). Phase 7.1 report: [`docs/PHASE_71_REPORT.md`](docs/PHASE_71_REPORT.md). Phase 7.2 report: [`docs/PHASE_72_REPORT.md`](docs/PHASE_72_REPORT.md).
Early runtime output is placeholders only. `Graphic/LOGO.png` is loaded once for the Connection Frontend. The game client compile-embeds four Headwear Side proof PNGs onto the existing Crown attachment (Player debug overlay selects cells 1–4; not a Graphic/ scan). The rest of `Graphic/` stays unused until later visual-content phases.

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
| `crates/skeleton` | `purgatory-skeleton` | Client presentation skeleton math (Stage D debug draw; P4 placeholders in the client) |
| `crates/animation` | `purgatory-animation` | Client presentation clip sampling + `AnimationPlayer` (depends only on skeleton) |
| `crates/dev_runtime` | `purgatory-dev-runtime` | Headless Developer Hub orchestration |
| `tools/content_validator` | `purgatory-content-validator` | Content validation tool |
| `tools/animation_lab` | `purgatory-animation-lab` | Standalone Animation Lab (A7.0 core + A7.1 QoL; eframe; Hub-launched) |
| `tools/bot_client` | `purgatory-bot-client` / `purgatory-load` | Headless QUIC load harness |

## Developer Tools

Windows: run [`DEV.BAT`](DEV.BAT) at this repository root. That opens the current PowerShell Developer Tools shell (fallback). The Rust Developer Hub (server lifecycle, Runtime Validation, load/soak, clients, quality gate, Rebuild, Kill All, settings, **Animation Lab launch**) is [`DEV_HUB.BAT`](DEV_HUB.BAT): it builds if needed, launches `purgatory-dev-hub.exe`, and exits so the Hub is not bound to that console. Do not use `cargo run -p purgatory-dev-hub` as the normal launch path (that keeps the Hub under cargo’s process job). Closing the Hub does not stop the dedicated server or detached clients. A second Hub for the same workspace is refused (`logs/dev-tools/hub.lock`). Do not drive the same workspace from both shells at once.

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

Development keys (client mapping only, **Game screen**): **A / Left** move left, **D / Right** move right, **S / Down** hold down, **Space** jump, **Down + Jump** drop through OneWay. **E** generic interact. **Up Arrow** portal activate. **J** Basic Strike (intent only; no target). **Backquote / `~`** toggles the in-window development debug overlay (Debug tab is the everyday now-state; Debug → Display selects window resolution presets, world MSAA Off/4×, the RF0 rotated-geometry diagnostic, and the RF1.5–RF3 simultaneous A/B proof; Network tab shows QUIC session identity, latest RTT, last failure category, Channel, and Disconnect). The default client window is **1280×720** physical pixels, windowed. Display resolution and Render Scale are client-only; the gameplay camera is a locked 16:9 world view (higher pixel sizes or render scale add fidelity, not FOV; world default is **200% Render Scale + 4× MSAA**, with **100% + 4×** as the performance fallback; non-16:9 windows letterbox/pillarbox). The client starts on the Connection Frontend and does **not** auto-connect. Click **CONNECT** to `127.0.0.1:5001`. The Connection Frontend has a **DEV login** field (default `dev.local`). Same login restores the same Character after reconnect or server restart. A second client with that login is rejected (`Already connected`) while the first session is live. If the server is down the client stays alive, CONNECT remains retryable, and the frontend shows a short status (`Connection failed`, `Version mismatch`, `Already connected`, `Connection lost`, …) rather than Quinn/rustls text. Default client builds include Cargo feature `dev-diagnostics` (egui overlay). Shipping-style: `cargo build -p purgatory-client --release --no-default-features` (no overlay; auto-connects once — ADR-0060).

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
- [`docs/CHARACTER_ANIMATION_ARCHITECTURE.md`](docs/CHARACTER_ANIMATION_ARCHITECTURE.md)
- [`docs/CHARACTER_ANIMATION_ROADMAP.md`](docs/CHARACTER_ANIMATION_ROADMAP.md)
- [`docs/PHASE_56_LATENCY_MATRIX.md`](docs/PHASE_56_LATENCY_MATRIX.md)
- [`docs/PHASE_57_LOAD_TESTING.md`](docs/PHASE_57_LOAD_TESTING.md)
