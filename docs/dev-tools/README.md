# PURGATORY Developer Tools

PURGATORY Developer Tools is the development-side control surface for running, testing, and diagnosing the game. The launcher is one component of that product, not the whole product.

Open it with [`DEV.BAT`](../../DEV.BAT) at the repository root. That bootstrap starts a hidden STA PowerShell host and [`tools/dev/dev_launcher.ps1`](../../tools/dev/dev_launcher.ps1). It is not a Cargo build.

This document describes **what exists now**. Planned editors and later tool areas are labeled PLANNED.

See also: [ARCHITECTURE.md](ARCHITECTURE.md), [RUNTIME_LIFECYCLE.md](RUNTIME_LIFECYCLE.md), [ROADMAP.md](ROADMAP.md).

## Purpose

Developer Tools owns local development runtime control:

- build and launch the headless server and native clients
- verify that a launched server is actually connectable
- run the quality gate and load harness
- surface logs, metrics, and failure detail

It is not a production admin console. It is not a Map/NPC/content editor.

## CURRENT

Implemented in this pass:

| Area | Behavior |
|---|---|
| Server lifecycle | Start / Restart / Stop with an explicit state machine. Closing Developer Tools does **not** stop the server. |
| Client lifecycle | Open 1 / 2 / 3 clients; Stop All. Clients wait for **Ready**, not merely a live process. |
| Build | Debug/Release. Owned `cargo` process. START rebuilds the server, then launches. Rebuild skips the client package while `purgatory-client.exe` is locked. |
| Quality Gate | Visible console running [`scripts/check.ps1`](../../scripts/check.ps1). |
| Load testing | Dialog (count, profile, scenario, duration, seed), load-mode server (`PURGATORY_ADMISSION_CAP=256`, metrics `:5002`), owned `purgatory-load` harness with a visible dashboard, Stop Load, analyze last run, open logs/report. |
| Runtime Validation | Dialog (`--preset` smoke/mixed/stress/soak/…, optional duration overlay, seed). Forwards the same CLI a headless run uses. Requires Server **Ready**. Isolated `PURGATORY_DATA_DIR`. Pass/fail is Rust, not PowerShell. Exit 2 is CLI parse (often a stale `purgatory-load.exe` without `--preset`), not a Mixed FAIL. The GUI rebuilds `purgatory-bot-client` when `--print-server-env` rejects the argv. |
| Metrics | UDP `PURGSTAT` on `127.0.0.1:5002` (operational measurements + Health). |
| Connection probe | `purgatory-load --probe` (Quinn Hello/Welcome, protocol v10, login `dev.probe`). |
| Single-instance | Mutex `Local\PurgatoryDevLauncher`. A second `DEV.BAT` focuses the existing window. |
| Recovery | On open, adopt workspace `target\` server/client/load processes and **verify** before Ready. Duplicate servers for this workspace are stopped. |
| Logging | Colored activity box (green server, cyan client) plus `logs/dev-tools/`. Server/client/probe stdout is shown live. The ACTIVITY expand control opens a separate resizable log window. |
| Identity | Workspace Cargo version, root `PHASE`, git short hash (`*` if dirty). |
| Environment | Log level combo applies to **new** processes: `RUST_BACKTRACE=1`, optional `RUST_LOG`, `PURGATORY_NET_LOG`, `PURGATORY_NET_VERBOSE`. |
| Autostart | Starts the server on first show unless a server is already present or `PURGATORY_LAUNCHER_NO_AUTOSTART` is set. |
| Kill All | Workspace-scoped cargo (command line contains this repo root) plus owned server/client/load. |

### Metrics vs Health vs Readiness

These are not synonyms. See [RUNTIME_LIFECYCLE.md](RUNTIME_LIFECYCLE.md).

- **Metrics** — `LoadMetricsV1` numbers (tick timing, clients, admission, work counters).
- **Health** — the metrics UDP responder answers. Subsystems that export metrics are alive.
- **Readiness** — a real Quinn + protocol Hello/Welcome succeeds. The server will accept a client.

### Probe persistence (known debt)

`--probe` uses the normal DEV login / persistence / enter path with reserved login `dev.probe`. A successful probe may **create or restore** that character under the persist root (`%LOCALAPPDATA%\Purgatory\` unless `PURGATORY_DATA_DIR` is set). This is not the long-term health design. Do not treat it as a reason to change protocol v10.

## PLANNED

Not implemented. Do not treat these as present in the UI.

```text
Developer Tools
├── Runtime        CURRENT
├── Testing        CURRENT
├── Diagnostics    CURRENT
├── Content        PLANNED
├── Maps           PLANNED (visual editor may be a native tool)
├── NPCs           PLANNED
└── Settings       PLANNED (log level / profile exist today as Runtime controls)
```

Maps, NPC, dialogue, item, and gameplay-admin editors are out of scope until a later Developer Tools step.

## How to run

```text
DEV.BAT
```

Requires: Windows, PowerShell (STA), Rust/`cargo` on PATH for build/start. Python is optional (load-run analyzer).

Closing the window leaves server/client/load processes running. Use Stop / Stop All / Kill All to terminate them.

Hotkeys: **F5** start or restart server, **F6** queue one client.
