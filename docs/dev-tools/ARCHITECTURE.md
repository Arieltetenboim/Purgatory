# Developer Tools architecture

Developer Tools is one product concept. The current shell is PowerShell + Windows Forms. That is a deliberate, bounded choice, not a requirement that every future tool be written in PowerShell.

## Technology boundary

PowerShell + Windows Forms is suitable for:

- runtime control
- builds
- process management
- diagnostics
- settings
- simple property forms
- reports

A future visual Map Editor may be a dedicated native tool using project-native technology (`winit` / `wgpu` or similar). Developer Tools may later launch or integrate that process. Do not build that editor here.

The game protocol stays in Rust (`purgatory-protocol`, Quinn in `apps/server` and `tools/bot_client`). PowerShell must not reimplement Quinn or Hello/Welcome.

## Module layout

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

## Dependency direction

```text
UI
 ↓
feature / runtime services
 ↓
process / build / health abstractions
 ↓
OS / Cargo / PURGATORY executables
```

UI click handlers call `Request-*` / `Start-*` / `Stop-*` functions. They do not own process orchestration, Cargo inference, or readiness policy.

## Process ownership

Primary source of truth: retained `System.Diagnostics.Process` objects on `DevRuntimeState`.

```text
Developer Tools → ProcessStartInfo (cwd, env, args) → purgatory-server.exe
Developer Tools → ProcessStartInfo → purgatory-client.exe
Developer Tools → ProcessStartInfo → cargo.exe
```

Workspace `Get-Process` scans (path under this repo `target\`) are **recovery-only**: startup adopt, duplicate detection, Kill All. They are not the 1 Hz source of truth for processes this session started.

Closing Developer Tools does not stop children. The next open **adopts** surviving workspace processes and verifies them. Parent-PID “orphan” killing is gone; it existed to compensate for Windows Terminal tabs.

## Connection probe

`purgatory-load --probe` in `tools/bot_client` reuses the existing Quinn client stack and protocol **v10** Hello/Welcome. No protocol bump. No second networking implementation.

Reserved DEV login: `dev.probe`. See [README.md](README.md) (probe persistence debt) and [RUNTIME_LIFECYCLE.md](RUNTIME_LIFECYCLE.md).

## Visible shells

A process does not get a visible console merely because it is executable.

| Task | Window |
|---|---|
| Server | None (redirected to `logs/dev-tools/`) |
| Client | Game window only |
| Cargo | None (redirected; status in Activity) |
| Quality Gate | Visible PowerShell (`-NoExit`) |
| Load harness | Owned `purgatory-load.exe` console (dashboard) |
| Analyze last run | Visible console |

## Related crates

Developer Tools itself is not a Cargo member. It drives:

- `purgatory-server`
- `purgatory-client`
- `purgatory-bot-client` / `purgatory-load` (harness + `--probe`)
- `scripts/check.ps1` (fmt, check, clippy, test, content-validator)
