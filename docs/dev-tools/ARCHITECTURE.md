# Developer Tools architecture

Developer Tools is one product concept. The current operational shell is PowerShell + Windows Forms. The target shell is the Rust Developer Hub (ADR-0052). PowerShell is not a requirement that every future tool be written in PowerShell.

## Technology boundary

PowerShell + Windows Forms remains the fallback for capabilities not yet on the Hub ([`PARITY.md`](PARITY.md)).

The Hub is two crates:

```text
apps/dev_hub          purgatory-dev-hub       provisional eframe application shell
crates/dev_runtime    purgatory-dev-runtime   orchestration (no GUI)
```

Hub GUI modules (presentation only):

```text
apps/dev_hub/src/
    main.rs              Windows GUI subsystem; entry
    app.rs               window chrome, nav, status bar
    navigation.rs        pages / live vs placeholder
    theme.rs             restrained desktop visuals
    ui/dashboard.rs      Slice 1 overview + quick actions
    ui/runtime_server.rs Slice 1 server controls
    ui/logs.rs           bounded activity + OPEN LOGS
    ui/placeholders.rs   future categories
    ui/status.rs         global status strip
```

The GUI may only invoke and present `purgatory-dev-runtime`. eframe is revisitable; do not freeze it. The game protocol stays in Rust (`purgatory-protocol`, Quinn in `apps/server` and `tools/bot_client`). Neither PowerShell nor the Hub reimplements Quinn or Hello/Welcome. Ready is `purgatory-load --probe`.

A future visual Map Editor may be a dedicated native tool. Do not build that editor in Slice 1.

## PowerShell module layout

Scripts live under `tools/dev/`. They are **dot-sourced** `.ps1` files, not `.psm1` modules. The application is one STA process that shares `$script:` state. A module manifest would isolate that state for no gain. Dynamic discovery is not used; the entry script loads files in a fixed order.

```text
tools/dev/
    dev_launcher.ps1          entry: STA, mutex, load, message loop, shutdown
    core/
        environment.ps1       constants, paths, identity, cargo discovery
        process.ps1          Start-OwnedProcess, stop, workspace scan, adopt
        state.ps1            DevRuntimeState
        logging.ps1          UI + logs/dev-tools/, deadlock-safe redirect
    runtime/
        build.ps1            owned cargo
        server.ps1           lifecycle, Ready gating, client queue
        client.ps1          client processes
        probe.ps1           metrics UDP + purgatory-load --probe
    features/
        load_testing.ps1     dialog, load mode, harness, analyze, logs
        quality_gate.ps1    visible check.ps1
        reports.ps1         last-run / Explorer helpers used by Testing
    ui/
        theme.ps1            colors, fonts, control factories
        window.ps1          form layout: Runtime / Testing / Diagnostics
```

`tools/dev_launcher.ps1` is a compatibility stub that re-invokes `tools/dev/dev_launcher.ps1`.

Do not invest in new PowerShell architecture beyond fixes required to keep this shell usable.

## Dependency direction

```text
Hub GUI (eframe)
 ↓
purgatory-dev-runtime
 ↓
process / build / health
 ↓
OS / Cargo / PURGATORY executables
```

UI click handlers send `HubCommand` values. They do not own process orchestration, Cargo inference, or readiness policy.

## Process ownership

Spawned, adopted, and discovered are distinct ([`PARITY.md`](PARITY.md), [`RUNTIME_LIFECYCLE.md`](RUNTIME_LIFECYCLE.md)).

Workspace `target\` scans are **recovery-only**: startup adopt, duplicate detection, explicit Stop when nothing is tracked. They are not the lifecycle source of truth for processes this session started.

Closing Developer Tools / the Hub does not stop the dedicated server. Cargo and probes may die with the Hub (session-owned). The next open **discovers** a workspace `target\` server (exe path under this repo, not a raw PID), **adopts** it, and **verifies** with `--probe` before Ready.

The operational Hub launch is `DEV_HUB.BAT` → independent `purgatory-dev-hub.exe`. Do not leave the Hub under `cargo run` (Windows job / console process group).

## Connection probe

`purgatory-load --probe` in `tools/bot_client` reuses the existing Quinn client stack and protocol **v10** Hello/Welcome. No protocol bump. No second networking implementation.

Reserved DEV login: `dev.probe`. See [README.md](README.md) (probe persistence debt) and [RUNTIME_LIFECYCLE.md](RUNTIME_LIFECYCLE.md).

## Related crates

The Hub is a Cargo workspace member. It drives:

- `purgatory-server`
- `purgatory-client` (PowerShell / later Hub slices)
- `purgatory-bot-client` / `purgatory-load` (harness + `--probe`)
- `scripts/check.ps1` (PowerShell / later Hub slices)
