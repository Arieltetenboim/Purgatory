# PURGATORY Developer Tools

PURGATORY Developer Tools is the development-side control surface for running, testing, and diagnosing the game. The launcher is one component of that product, not the whole product.

Open it with [`DEV.BAT`](../../DEV.BAT) at the repository root (PowerShell fallback). The Rust Developer Hub is [`DEV_HUB.BAT`](../../DEV_HUB.BAT): it builds `purgatory-dev-hub` if needed, starts `target\debug\purgatory-dev-hub.exe` independently, and exits. The Hub is a Windows GUI process (no bootstrap console). Do not use `cargo run -p purgatory-dev-hub` as the normal path — that leaves a console and can put the Hub (and a spawned server) in cargo’s job, so closing the shell kills them. The PowerShell bootstrap is not a Cargo build.

This document describes **what exists now**. Planned editors and later tool areas are labeled PLANNED. Capability inventory and Hub migration slices: [`PARITY.md`](PARITY.md). Architecture: ADR-0050 (foundation) and ADR-0052 (Rust Hub).

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
| Client lifecycle | Open 1 / 2 / 3 clients; Stop All. Clients wait for **Ready**, not merely a live process. Open Client skips cargo rebuild when any workspace `purgatory-client` is running or the exe is locked (avoids Access Denied); Stop clients first to pick up a new build. |
| Build | Debug/Release. Owned `cargo` process. START rebuilds the server, then launches. Rebuild skips a package while its `.exe` is running (`purgatory-server` / `purgatory-client`) so cargo does not hit Access Denied. |
| Quality Gate | Visible console running [`scripts/check.ps1`](../../scripts/check.ps1). |
| Load testing | Dialog (count, profile, scenario, duration, seed), load-mode server (`PURGATORY_ADMISSION_CAP=256`, metrics `:5002`), owned `purgatory-load` harness with a visible dashboard, Stop Load, analyze last run, open logs/report. |
| Runtime Validation | Dialog (`--preset` smoke/mixed/stress/soak/…, optional `--duration` overlay, seed). Forwards the same CLI a headless run uses. Requires Server **Ready**. Always rebuilds `purgatory-load` first, then restarts a clean load-mode server with isolated `PURGATORY_DATA_DIR`. Refuses a second concurrent harness. Live status shows elapsed/duration, real-client count, and portal progress from `live_status.json` (no extra console window; 1 Hz dashboard writeln would ding). Pass/fail is Rust, not PowerShell. Mixed soak keeps a persistent real-QUIC baseline; churn is a separate role; portal bots walk into the activation zone. `--duration` without `--timeout` raises the wall-clock timeout in Rust (not in the GUI). ANALYZE LAST RUN uses a completed artifact (`last_finished.txt`), not an in-progress `current_run` or a future-dated folder. Exit 2 with `unexpected argument` rebuilds `-p purgatory-bot-client --bin purgatory-load`. Other CLI errors are shown as-is (not treated as a stale binary). |
| Metrics | UDP `PURGSTAT` on `127.0.0.1:5002` (operational measurements + Health). |
| Connection probe | `purgatory-load --probe` (Quinn Hello/Welcome, protocol v10, login `dev.probe`). |
| Single-instance | Mutex `Local\PurgatoryDevLauncher`. A second `DEV.BAT` focuses the existing window. |
| Recovery | On open, adopt workspace `target\` server/client/load processes and **verify** before Ready. Duplicate servers for this workspace are stopped. |
| Logging | Colored activity box (green server, cyan client) plus `logs/dev-tools/`. Server/client/probe stdout is shown live. The ACTIVITY expand control opens a separate resizable log window. Routine log/status/portal lines are silent. Windows dialog sounds still play only for blocking `MessageBox` calls (refusals, failures, already-running). |
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
└── Settings       CURRENT (profile / log level / quality gate / rebuild / Kill All)
```

Maps, NPC, dialogue, item, and gameplay-admin editors are out of scope until a later Developer Tools step.

## How to run

```text
DEV.BAT
```

PowerShell fallback. Requires: Windows, PowerShell (STA), Rust/`cargo` on PATH for build/start. Python is optional (load-run analyzer).

```text
DEV_HUB.BAT
```

Builds if needed, launches `purgatory-dev-hub.exe`, then the bootstrap exits. Live pages: Dashboard, Runtime → Server / Clients, Validation, Performance, Logs, Settings. World / Content remain placeholders (editors). The Hub window is **fixed size** (1280×800, non-resizable) until a full responsive layout pass exists. Closing the Hub does **not** stop the dedicated server or detached clients; reopen adopts workspace processes and re-verifies the server with `--probe` before Ready. Kill All **does** stop server/clients/load. A second Hub for this workspace is refused (`logs/dev-tools/hub.lock`). Do not drive the same workspace from both shells at once.

### Hub UI presentation (current)

- **Dashboard** — shared design-system modules (StatusCard / Project / Attention / Quick Actions + real ActivityLog strip). Semantic wide/medium/narrow composition; primary/ghost/destructive buttons; no invented host System Status gauges.
- **Validation & Performance** — live run header + structured `live_status.json` fields, bounded `load.log` tail, `metrics.csv` chart (connected bots / tick mean), completed **Result summary** with **Show full details** expander over `run_summary.json`. Pass/fail remains harness CLI authority. Cancelled is distinct from Failed / OrchestrationFailed.
- **Visual system** — shared cards, status pills, metric tiles, page headers in `apps/dev_hub` (`theme` + `ui/layout`). Minimum practical window ~720×520.
- Runtime snapshot owns presentation data (`ValidationLiveStatus`, `RunSummaryBrief`, `MetricsSeries`, `load_log_lines`). GUI does not spawn harnesses or decide PASS/FAIL.

Do not use `cargo run -p purgatory-dev-hub` as the operational launch path.

Closing the window leaves server/client/load processes running. Use Stop / Stop All / Kill All to terminate them.

Hotkeys: **F5** start or restart server, **F6** queue one client.
