# Developer Tools capability / parity inventory

Source of truth for “what the PowerShell launcher does today.” The Rust Developer Hub must not silently drop a CURRENT capability. Slice tags say when Hub work is allowed to take it.

This is tooling, not a gameplay phase. Do not start Phase 7 from this document.

See [`README.md`](README.md) (what exists now), [`RUNTIME_LIFECYCLE.md`](RUNTIME_LIFECYCLE.md) (state machine), [`ARCHITECTURE.md`](ARCHITECTURE.md).

## Slice tags

| Tag | Meaning |
|---|---|
| **Slice 1** | Server lifecycle, readiness/probe, logs, Hub application shell |
| **Slice 2** | Runtime Validation (preserve CLI semantics) |
| **Slice 3** | Load / soak launcher |
| **Later / launcher parity** | Clients, quality gate, Rebuild, Kill All, settings, file log tails — **now in Hub** |
| **Out of scope** | Editors, admin protocol, player launcher |

GUI visual match is **not** a slice gate.

## Slice 2 invariants

Runtime Validation is orchestration around `purgatory-load`. The harness owns pass/fail.

- Ready is required before StartValidation. Ready is not a correctly configured load-mode server; official RV always rebuilds `purgatory-load`, captures `--print-server-env`, and restarts a **new** detached server with ExtraEnv (`PURGATORY_ADMISSION_CAP=256`, `PURGATORY_METRICS_PORT=5002`, printed `PURGATORY_LOAD_VALIDATION`, isolated `PURGATORY_DATA_DIR`). Persist is `logs/load/rv_{stamp}/persist` — never `%LOCALAPPDATA%\Purgatory`.
- Argv matches `Get-RuntimeValidationArgv` / `developer_tools_runtime_validation_argv_parses`: `--preset --seed --allow-high-count --max-bots 256 --server 127.0.0.1:5001 --metrics 127.0.0.1:5002` plus optional `--duration` and `--persist-root`.
- Stale binary: rebuild once (`runtime-val-prep`), then `--print-server-env`. Post-rebuild `unexpected argument` / nonzero is OrchestrationFailed. No second rebuild loop. Harness exit 2 is Failed.
- Exit authority: `0` Passed, `1`/`2`/other Failed, `130` Cancelled.
- One Validation job. Do not add `ServerState::Validating`. RV is a caller of generic `ServerLaunchOptions { extra_env }`.
- Cancel kills the session-owned harness (and ValidatePrep cargo), not the detached server. Server Stop clears pending RV.
- Workspace Hub lock: `{workspace}/logs/dev-tools/hub.lock` (Windows exclusive `share_mode(0)`). Distinct from PowerShell `Local\PurgatoryDevLauncher`. A second Hub for the same workspace is refused.
- `live_status.json` is best-effort presentation. Malformed/missing does not fail the run.

## Slice 3 / launcher-parity invariants

Load/soak is orchestration around `purgatory-load` without `--preset`. One **LoadJob** sibling to ValidationJob. Do not add `ServerState::Loading`.

- Dialog fields: count `1|2|10|25|50|100` (default 10), profile `idle|walker|jumper|mixed`, scenario `load|burst|churn`, duration `1m|2m|5m|10m|30m` (default 2m), seed default `1234`.
- Argv: `--count --profile --scenario --duration --seed --max-bots --allow-high-count --server 127.0.0.1:5001 --metrics 127.0.0.1:5002` (no `--preset`).
- If metrics probe is load-compatible (`admission_cap >= count` and `max_entities_per_snapshot >= count` and schema ≥ 1): spawn harness immediately. Else: ExtraEnv restart (`PURGATORY_ADMISSION_CAP=256`, `PURGATORY_METRICS_PORT=5002`), wait for `--probe` Ready, then harness. START LOAD that needs a restart is consent (no MessageBox).
- RV and load refuse each other (same binary). Stop Load / cancel kills harness only, not the server. Server Stop / Kill All clear both pending queues.
- Harness is Session-owned, `log_name: "load"`, `ui_pump: false` (PowerShell used VisibleConsole — see differences).
- ANALYZE LAST RUN resolves finished artifacts (`last_runtime_validation.txt` / `last_finished.txt` / `latest.txt` with ignore-in-progress rules) and may open a visible Python console.
- Clients: queue until Ready, stagger 140 ms, Detached + `client.log`, skip client rebuild if workspace clients live or exe locked, adopt on reopen. Stop All kills tracked + discovered clients.
- Quality gate: visible `powershell … scripts/check.ps1` console, detached from Hub close.
- Phase 7.8 performance gate: Settings → **PHASE 7.8 GATE** launches `scripts/phase_78_gate.ps1` (visible console; long). Pass/fail semantics in [`docs/PHASE_78_REPORT.md`](../PHASE_78_REPORT.md).
- Phase 7 Stats: Testing → **Phase 7 Stats** reads latest `logs/load/capacity_78/gate_*/phase78_gate_summary.json` (no recompute). Labels findings as HARNESS vs SERVER; current YELLOW is harness `snapshot_starvation`, not server tick failure.
- Rebuild skips packages whose workspace exe is running. Kill All kills workspace cargo (command line contains repo root), server, client, load, and **does** stop the dedicated server (unlike Hub close).
- Settings: debug/release profile and Default/Debug/Trace log level apply to **new** processes only.

## Slice 1 invariants

These bind Slice 1 even when the PowerShell code is less explicit.

### Discovery vs ownership vs controllability

Do not collapse these:

| Concept | Meaning |
|---|---|
| **Discovered** | A `purgatory-server` (or client/load) image whose path is under this workspace `target\`. A scan result. |
| **Spawned** | This Hub/session started the process (`Command` / `ProcessStartInfo`). |
| **Adopted** | This session attached to a discovered process it did not spawn (startup recovery or 5 s recovery scan). |
| **Tracked** | Spawned or adopted. The 1 Hz lifecycle uses the tracked handle, not a fresh scan. |
| **Controllable** | An explicit Stop/Kill may terminate a tracked **or** discovered workspace process. Discovery alone does not make the 1 Hz loop treat it as our server. |

Closing the UI does not stop the dedicated server. Reopen discovers a workspace `target\` server, **adopts** it (origin Adopted, not Spawned), then **verifies** with `purgatory-load --probe` before Ready. Process existence is not Ready. `server process exited unexpectedly` is only when this Hub session had already observed that process alive.

### Ready semantics (copy, do not improve)

Copied from [`RUNTIME_LIFECYCLE.md`](RUNTIME_LIFECYCLE.md) and `tools/dev/runtime/server.ps1`:

- **Ready** iff the tracked server process is alive **and** `purgatory-load --probe` exits 0.
- Initial Ready does **not** require Health/`PURGSTAT`. Verifying still polls metrics for the Health line and still starts the connection probe if metrics fail.
- After Ready, if metrics were passing and then fail, the launcher moves to **Degraded** (process kept; last connection pass retained). Metrics returning moves Degraded → Ready **without** a new probe. That post-Ready Health coupling is existing launcher behavior, not a new gate.
- UDP listener on `:5001` is diagnostic only. It must not override a successful probe.
- Probe retry: 1 s after a failed probe; overall Verifying timeout 45 s (`ReadyTimeoutSec`). Exit 2 on first probe attempts a `probe-prep` rebuild of `purgatory-load`.
- Probe-prep cargo is a **build** (server stays Starting), not Verifying.

Do not change these timings or retry rules while establishing parity. Record unintentional differences below instead of “fixing” them.

### Jobs and supersession

Build, start, probe, and stop are explicit jobs with generational ids. A late probe/cargo exit from a cancelled job is ignored. Repeated UI Start while Start/Build/Probe is running is a no-op (log “already in progress”), matching the launcher. Stop cancels the current job (cargo + probe) then stops the server. Restart is Stop with `restart-after-stop`.

### Bounded UI logs

File logs under `logs/dev-tools/` may grow. The UI process must not. Match the launcher caps: incoming pump drop-oldest at 4000, drain ≤ 500 per tick, activity ring 4000, main view shows 180. Server/client handshake lines live in file tails (`server.log` / `client.log`), not the activity ring.

## CURRENT launcher capabilities

### Runtime — Slice 1 unless noted

| Capability | Slice | Notes |
|---|---|---|
| Server state machine (Stopped/Building/Starting/Verifying/Ready/Degraded/Stopping/Failed) | 1 | |
| Start / Restart / Stop | 1 | F5 in Hub and PowerShell |
| Closing UI does not stop children | 1 | Clients Detached like server |
| Owned cargo rebuild before start | 1 | Rebuild button skips locked exe |
| Debug/Release profile toggle | Hub settings | Applies to **new** cargo/exe lookups |
| Process exists ≠ Ready | 1 | |
| Queued clients wait for Ready | Hub Clients | Stagger 140 ms; F6 = +1 |
| Autostart on first show | — | **Code** currently logs `Auto-start disabled; click START` unless a server was adopted. README still lists autostart. Hub matches **code**. |
| Startup recovery adopt + verify; stop extra workspace servers | 1 | Clients adopted on reopen |
| Recovery scan every 5 s when Stopped/Failed | 1 | |
| Load-mode env (`PURGATORY_ADMISSION_CAP=256`, metrics `:5002`) | 2 (RV ExtraEnv) / 3 (load dialog) | |

### Clients — Hub

Open +1/+2/+3, Stop All, F6, stagger 140 ms, skip client rebuild if exe locked, Detached + `client.log` tail, adopt on reopen.

### Testing

| Capability | Slice |
|---|---|
| Quality gate → visible `scripts/check.ps1` | Hub |
| Rebuild (skip running server/client/Animation Lab exe) | Hub |
| Load test dialog, Stop Load, analyze last run, last report | 3 |
| Runtime Validation dialog; CLI pass/fail; isolated persist; refuse concurrent harness | 2 |
| Kill All | Hub |

### Diagnostics

| Capability | Slice |
|---|---|
| Metrics vs Health vs Readiness | 1 |
| Activity log + file logs; OPEN LOGS | 1 |
| Bounded SERVER LOG / CLIENT LOG file tails | Hub |
| Expandable activity window (4000 lines) | Later (core keeps 4000; Hub GUI shows 180) |
| LOAD LOGS | 3 |
| Identity: workspace version, `PHASE`, git hash (`*` if dirty) | 1 |
| Log-level combo for **new** processes | Hub Settings |
| Single-instance mutex `Local\PurgatoryDevLauncher` | PowerShell only |

## Recorded Hub differences (intentional, not silent improvements)

- Hub GUI is provisional eframe, not WinForms. Not a visual clone. Live: Dashboard, Runtime → Server / Clients, Validation, Performance, Logs, Settings, Content (launches Animation Lab; ADR-0058). World stays a placeholder (map editor later).
- Hub single-instance lock is workspace `logs/dev-tools/hub.lock`, not `Local\PurgatoryDevLauncher`. PowerShell still uses that mutex. Do not run Hub plus PowerShell against the same workspace.
- Slice 1 listener diagnostic may report `unknown` (no `IPGlobalProperties` port). Must not affect Ready.
- Activity timestamps in the Hub may be UTC `HH:MM:SS` rather than local `Get-Date`.
- Activity and SERVER/CLIENT file-tail lines are stamped `HH:MM:SS` (no date) when shown in the Hub. Hub stamps use UTC wall-clock; PowerShell uses local `Get-Date`.
- Dedicated server and clients use Detached + file stdio (`server.log` / `client.log`). Hub presents **bounded file tails**, not a live stdout re-pipe. PowerShell load harness used VisibleConsole; Hub load harness is Session with file log (`ui_pump: false`). Hub also tails `load.log` on Validation/Performance and charts bounded samples from `metrics.csv` (does not invent series).
- Validation/Performance completed UI shows a Result summary + expandable details from `run_summary.json`; live compact strip reads `capacity_live.json` when present (7.1 ownership; not a per-client wall). Dashboard uses a small shared design system (tokens, HubCard, button variants, status badges) with semantic wide/medium/narrow composition. Recent Activity is real `ActivityLog` only. No host CPU/Mem System Status row.
- START VALIDATION / START LOAD that need a restart are consent via the button (no WinForms MessageBox).
- Explicit `JobId` supersession is stricter internally than PowerShell flags; user-visible Start/Stop/Restart rules stay the same.
- `DEV.BAT` remains PowerShell fallback. Hub: [`DEV_HUB.BAT`](../../DEV_HUB.BAT) (build, then independent `purgatory-dev-hub.exe`). `cargo run -p purgatory-dev-hub` is not the operational launch path.

If a timing, retry, or process-behavior difference is discovered later, add it here. Do not silently tune it.
