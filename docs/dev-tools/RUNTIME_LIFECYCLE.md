# Developer Tools runtime lifecycle

## Temporal invariant

```text
build complete
→ executable / process start
→ runtime becomes responsive
→ readiness checks
→ Ready
→ queued clients may launch
```

No arbitrary sleep is a correctness mechanism. Polling is allowed only against an explicit condition with a timeout that yields **Failed** (or **Degraded** if the process is still alive after a later check fails).

A client requested while the server is Building, Starting, or Verifying is **queued**. It does not launch because a process exists or a UDP port looks busy.

## Server states

| State | Meaning |
|---|---|
| Stopped | No owned or adopted server process. |
| Building | Owned `cargo` is compiling the **server** (or a rebuild that blocks start). No server process yet. |
| Starting | Server process is alive. Readiness checks have not started. Probe-binary compilation, if needed, is a **build** (`Build.Reason = probe-prep`), not Verifying. |
| Verifying | Readiness checks are in flight (metrics + `--probe`). |
| Ready | Readiness passed. Clients may launch. |
| Degraded | Process still alive; a check that had passed now fails. Clients are not launched. |
| Stopping | Stop requested; waiting for the process to exit. |
| Failed | Build, start, or readiness failed. Process may still be alive. |

Building the probe executable must not be shown as Verifying. Server state stays Starting until checks begin; the build row shows target `purgatory-bot-client` / reason `probe-prep`.

## Readiness checks

| Check | Role | Failure policy |
|---|---|---|
| Expected process alive | **Required** | Not Ready. |
| UDP listener on `:5001` | **Diagnostic only** | Recorded. Must not override a successful metrics or protocol probe. |
| Metrics `PURGSTAT` on `:5002` | **Health** | Health FAIL. Still attempt the connection probe; Ready requires connection. |
| `purgatory-load --probe` Hello/Welcome | **Authoritative Connection / Readiness** | Ready iff this succeeds (and the process is still alive). |

If OS UDP listener inspection is wrong or unavailable, but metrics or Hello/Welcome succeed, the server is **not** classified unavailable because of the listener diagnostic.

Ready means: process alive **and** connection probe passed.

Health PASS means: metrics responder answered with a parseable `PURGSTAT` (schema ≥ 1).

## Connection probe

`purgatory-load --probe --server 127.0.0.1:5001`:

1. Quinn connect (ALPN `purgatory`, existing bot TLS skip-verify)
2. Hello protocol v10, `client_build` probe prefix, `dev_login = "dev.probe"`
3. Wait for Welcome or Disconnect (handshake timeout)
4. Close; exit 0 on Welcome, nonzero otherwise

Does not enter the 30 Hz input loop. Does not bump the protocol.

**Persistence debt:** Welcome is sent after identity lookup, restore, spawn, and bind. A successful probe may mint or restore character `dev.probe` on disk. Documented; not redesigned in this pass.

If `purgatory-load.exe` for the selected profile is missing, Developer Tools builds `-p purgatory-bot-client` as a normal owned cargo job, then starts Verifying.

## Client gating

```text
Server requested
→ build if needed (owned cargo, wait for ExitCode)
→ server process starts
→ Starting (probe build if needed)
→ Verifying (metrics + --probe)
→ Ready
→ queued clients may launch
```

If readiness fails: do not launch queued clients; keep them queued; show structured failure.

If clients are already running, do not rebuild the client exe (Windows file lock). Launch additional instances of the existing binary only once the server is Ready.

## Recovery / reopen

On Developer Tools start:

1. Scan `target\` for `purgatory-server`, `purgatory-client`, `purgatory-load`.
2. Adopt surviving processes (retain `Process` objects, subscribe Exited).
3. If more than one server for this workspace, stop extras; keep one.
4. Run readiness verification before declaring Ready.
5. Do not kill a live server solely because its parent PID is dead (that used to mean “terminal tab closed”; now it usually means “Developer Tools was closed”).

Kill All / Stop still terminate workspace-owned trees. Cargo kill on Kill All is **recovery-scoped**: `cargo.exe` whose command line contains this repository root.

## Build lifecycle

```text
BUILD REQUESTED
→ cargo Process started (owned)
→ Building (UI stays responsive)
→ process exits
    0 → continue (start exe, or finish rebuild)
    nonzero → Failed (do not launch)
```

There is no “saw cargo / exe timestamp / launch anyway” path. A hung cargo is Stoppable; it is not treated as success.

## Load mode

Load tests that need admission above the default restart the owned server with `PURGATORY_ADMISSION_CAP=256` and `PURGATORY_METRICS_PORT=5002`, wait until **Ready** plus metrics `admission_cap` / `max_entities_per_snapshot` compatibility, then start the harness. Same state machine as a normal server.

## Exit policy

FormClosed releases the mutex and disposes UI resources. It does **not** stop server, client, load, or cargo. Footer text states this.
