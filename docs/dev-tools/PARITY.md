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
| **Later** | Remaining launcher features; not lost |
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

File logs under `logs/dev-tools/` may grow. The UI process must not. Match the launcher caps: incoming pump drop-oldest at 4000, drain ≤ 500 per tick, activity ring 4000, main view shows 180.

## CURRENT launcher capabilities

### Runtime — Slice 1 unless noted

| Capability | Slice | Notes |
|---|---|---|
| Server state machine (Stopped/Building/Starting/Verifying/Ready/Degraded/Stopping/Failed) | 1 | |
| Start / Restart / Stop | 1 | F5 in PowerShell GUI; Hub may bind the same later |
| Closing UI does not stop children | 1 | |
| Owned cargo rebuild before start | 1 | Skip locked `.exe` is Later (Rebuild button) |
| Debug/Release profile toggle | Later | Slice 1 Hub is debug-only |
| Process exists ≠ Ready | 1 | |
| Queued clients wait for Ready | Later | Clients not in Slice 1 |
| Autostart on first show | — | **Code** currently logs `Auto-start disabled; click START` unless a server was adopted. README still lists autostart. Hub matches **code**. |
| Startup recovery adopt + verify; stop extra workspace servers | 1 | |
| Recovery scan every 5 s when Stopped/Failed | 1 | |
| Load-mode env (`PURGATORY_ADMISSION_CAP=256`, metrics `:5002`) | 2 (RV ExtraEnv) / 3 (load dialog) | Slice 2 sets ExtraEnv on a new server for official RV. Slice 3 load dialog may reuse the same launch options. |

### Clients — Later

Open +1/+2/+3, Stop All, F6, stagger 140 ms, skip client rebuild if exe locked.

### Testing

| Capability | Slice |
|---|---|
| Quality gate → visible `scripts/check.ps1` | Later |
| Rebuild (skip running server/client exe) | Later |
| Load test dialog, Stop Load, analyze last run, last report | 3 |
| Runtime Validation dialog; CLI pass/fail; isolated persist; refuse concurrent harness | 2 |

### Diagnostics

| Capability | Slice |
|---|---|
| Metrics vs Health vs Readiness | 1 |
| Activity log + file logs; OPEN LOGS | 1 |
| Expandable activity window (4000 lines) | Later (core keeps 4000; Slice 1 GUI shows 180) |
| LOAD LOGS | 3 |
| KILL ALL | Later |
| Identity: workspace version, `PHASE`, git hash (`*` if dirty) | 1 |
| Log-level combo for **new** processes | Later |
| Single-instance mutex `Local\PurgatoryDevLauncher` | PowerShell only |

## Recorded Hub differences (intentional, not silent improvements)

- Hub GUI is provisional eframe, not WinForms. Not a visual clone. Dashboard, Runtime → Server, Validation, and Logs are live; Performance / World / Content / Clients / Settings are placeholders.
- Hub single-instance lock is workspace `logs/dev-tools/hub.lock`, not `Local\PurgatoryDevLauncher`. PowerShell still uses that mutex. Do not run Hub plus PowerShell against the same workspace.
- Slice 1 listener diagnostic may report `unknown` (no `IPGlobalProperties` port). Must not affect Ready.
- Activity timestamps in the Hub may be UTC `HH:MM:SS` rather than local `Get-Date`.
- Slice 1/2 have no debug/release toggle, log-level combo, client buttons, quality gate, load dialog, OPEN FOLDER/REPORT, or Kill All.
- START VALIDATION is restart consent (no WinForms MessageBox).
- Explicit `JobId` supersession is stricter internally than PowerShell flags; user-visible Start/Stop/Restart rules stay the same.
- `DEV.BAT` remains PowerShell. Hub: [`DEV_HUB.BAT`](../../DEV_HUB.BAT) (build, then independent `purgatory-dev-hub.exe`). `cargo run -p purgatory-dev-hub` is not the operational launch path.

If a timing, retry, or process-behavior difference is discovered later, add it here. Do not silently tune it.
