# Runtime Validation: invoke the same `purgatory-load` CLI a headless run uses.
# PowerShell only builds argv/env and shows status. Pass/fail stays in Rust.

function Show-RuntimeValidationDialog {
    $form = New-Object Windows.Forms.Form
    $form.Text = "Runtime Validation"
    $form.Size = New-Object Drawing.Size(400, 260)
    $form.StartPosition = "CenterParent"
    $form.FormBorderStyle = "FixedDialog"
    $form.MaximizeBox = $false
    $form.MinimizeBox = $false

    [void](New-Label $form "Preset" 20 20 80 $script:Muted 8)
    $boxPreset = New-Object Windows.Forms.ComboBox
    $boxPreset.DropDownStyle = "DropDownList"
    $boxPreset.Location = New-Object Drawing.Point(120, 18)
    $boxPreset.Size = New-Object Drawing.Size(240, 24)
    $boxPreset.Items.AddRange([object[]]@("smoke", "mixed", "stress", "soak", "scheduler", "aoi", "churn", "persistence"))
    $boxPreset.SelectedItem = "mixed"
    $form.Controls.Add($boxPreset)

    [void](New-Label $form "Duration" 20 55 80 $script:Muted 8)
    $boxDur = New-Object Windows.Forms.ComboBox
    $boxDur.DropDownStyle = "DropDownList"
    $boxDur.Location = New-Object Drawing.Point(120, 53)
    $boxDur.Size = New-Object Drawing.Size(240, 24)
    $boxDur.Items.AddRange([object[]]@("(preset default)", "20s", "2m", "5m", "10m", "30m"))
    $boxDur.SelectedItem = "(preset default)"
    $form.Controls.Add($boxDur)

    [void](New-Label $form "Seed" 20 90 80 $script:Muted 8)
    $boxSeed = New-Object Windows.Forms.TextBox
    $boxSeed.Location = New-Object Drawing.Point(120, 88)
    $boxSeed.Size = New-Object Drawing.Size(240, 24)
    $boxSeed.Text = "1234"
    $form.Controls.Add($boxSeed)

    $hint = New-Label $form "CLI is authoritative. This dialog only forwards --preset / --duration / --seed." 20 125 340 $script:Muted 8
    $hint.AutoSize = $false
    $hint.Size = New-Object Drawing.Size(340, 32)

    $ok = New-Object Windows.Forms.Button
    $ok.Text = "RUN"
    $ok.Location = New-Object Drawing.Point(120, 175)
    $ok.Size = New-Object Drawing.Size(90, 28)
    $ok.DialogResult = [Windows.Forms.DialogResult]::OK
    $form.Controls.Add($ok)
    $form.AcceptButton = $ok

    $cancel = New-Object Windows.Forms.Button
    $cancel.Text = "Cancel"
    $cancel.Location = New-Object Drawing.Point(230, 175)
    $cancel.Size = New-Object Drawing.Size(90, 28)
    $cancel.DialogResult = [Windows.Forms.DialogResult]::Cancel
    $form.Controls.Add($cancel)
    $form.CancelButton = $cancel

    if ($form.ShowDialog() -ne [Windows.Forms.DialogResult]::OK) { return $null }

    $seed = $boxSeed.Text.Trim()
    if ([string]::IsNullOrWhiteSpace($seed)) { $seed = "1234" }
    $duration = [string]$boxDur.SelectedItem
    if ($duration -eq "(preset default)") { $duration = $null }
    return @{
        Preset   = [string]$boxPreset.SelectedItem
        Duration = $duration
        Seed     = $seed
    }
}

function Get-RuntimeValidationArgv {
    param($Spec, [string]$PersistRoot)

    # Keep in sync with `developer_tools_runtime_validation_argv_parses`
    # in tools/bot_client/src/scenario.rs. CLI is the contract; this only
    # forwards --preset / --seed / --duration / persist + connection flags.
    $argList = New-Object System.Collections.Generic.List[string]
    [void]$argList.Add("--preset")
    [void]$argList.Add($Spec.Preset)
    [void]$argList.Add("--seed")
    [void]$argList.Add($Spec.Seed)
    [void]$argList.Add("--allow-high-count")
    [void]$argList.Add("--max-bots")
    [void]$argList.Add("256")
    [void]$argList.Add("--server")
    [void]$argList.Add(("{0}:{1}" -f $script:ListenHost, $script:ListenPort))
    [void]$argList.Add("--metrics")
    [void]$argList.Add(("{0}:{1}" -f $script:ListenHost, $script:MetricsPort))
    if ($Spec.Duration) {
        [void]$argList.Add("--duration")
        [void]$argList.Add($Spec.Duration)
    }
    if ($PersistRoot) {
        [void]$argList.Add("--persist-root")
        [void]$argList.Add($PersistRoot)
    }
    return $argList.ToArray()
}

function Write-LoadCliDiagnostics {
    param($Capture, [string]$RebuiltTarget, [string]$RebuiltExe)

    $exeLine = "EXE=MISSING"
    if ($Capture -and $Capture.Exe) {
        $exeLine = Format-ExeIdentity -ExePath $Capture.Exe
    }
    Write-LaunchLog $exeLine
    if ($Capture -and $Capture.ArgvString) {
        Write-LaunchLog ("argv: purgatory-load {0}" -f $Capture.ArgvString)
    }
    if ($Capture) {
        Write-LaunchLog ("exit={0}" -f $Capture.ExitCode)
        if ($Capture.Stderr) {
            Write-LaunchLog ("stderr: {0}" -f $Capture.Stderr)
        }
    }
    if ($RebuiltTarget) {
        Write-LaunchLog ("rebuilt cargo target: {0} --bin purgatory-load -> {1}" -f $RebuiltTarget, $RebuiltExe)
        Write-LaunchLog (Format-ExeIdentity -ExePath $RebuiltExe)
    }
}

function Invoke-PurgatoryLoadCapture {
    param([string]$Exe, [string[]]$ArgumentList)

    $result = @{
        Exe          = $Exe
        Argv         = @($ArgumentList)
        ArgvString   = (ConvertTo-ArgumentString -ArgumentList $ArgumentList)
        ExitCode     = -1
        Stdout       = ""
        Stderr       = ""
        StaleBinary  = $false
        Missing      = $false
    }
    if (-not (Test-Path -LiteralPath $Exe)) {
        $result.Missing = $true
        $result.Stderr = "executable missing"
        return $result
    }

    $outFile = [IO.Path]::GetTempFileName()
    $errFile = [IO.Path]::GetTempFileName()
    $prev = $ErrorActionPreference
    $ErrorActionPreference = "SilentlyContinue"
    try {
        $proc = Start-Process -FilePath $Exe `
            -ArgumentList $result.ArgvString `
            -WorkingDirectory $script:Root `
            -Wait -PassThru -NoNewWindow `
            -RedirectStandardOutput $outFile `
            -RedirectStandardError $errFile
        $result.ExitCode = [int]$proc.ExitCode
        if (Test-Path -LiteralPath $outFile) {
            $result.Stdout = (Get-Content -LiteralPath $outFile -Raw -ErrorAction SilentlyContinue)
        }
        if (Test-Path -LiteralPath $errFile) {
            $result.Stderr = (Get-Content -LiteralPath $errFile -Raw -ErrorAction SilentlyContinue)
        }
        $result.Stdout = ([string]$result.Stdout).Trim()
        $result.Stderr = ([string]$result.Stderr).Trim()
        if ($result.Stderr -match "unexpected argument") {
            $result.StaleBinary = $true
        }
    }
    catch {
        $result.Stderr = [string]$_.Exception.Message
        $result.StaleBinary = $true
    }
    finally {
        $ErrorActionPreference = $prev
        Remove-Item -LiteralPath $outFile -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $errFile -Force -ErrorAction SilentlyContinue
    }
    return $result
}

function Get-CliServerEnv {
    param([string[]]$Argv)

    $exe = Get-LoadExe
    $printArgs = @($Argv) + @("--print-server-env")
    $capture = Invoke-PurgatoryLoadCapture -Exe $exe -ArgumentList $printArgs
    Write-LoadCliDiagnostics -Capture $capture
    if ($capture.Missing -or $capture.StaleBinary -or $capture.ExitCode -ne 0) {
        return @{ Ok = $false; Capture = $capture; Env = $null }
    }
    if ([string]::IsNullOrWhiteSpace($capture.Stdout)) {
        return @{ Ok = $false; Capture = $capture; Env = $null }
    }
    try {
        $env = $capture.Stdout | ConvertFrom-Json
        return @{ Ok = $true; Capture = $capture; Env = $env }
    }
    catch {
        Write-LaunchLog "print-server-env JSON parse failed: $($_.Exception.Message)"
        return @{ Ok = $false; Capture = $capture; Env = $null }
    }
}

function Invoke-RunRuntimeValidation {
    if ($script:Server.State -ne "Ready") {
        [Windows.Forms.MessageBox]::Show(
            "Server must be Ready before Runtime Validation.`nReady is the startup probe, not scenario health.",
            "RUNTIME VALIDATION",
            "OK",
            "Warning"
        ) | Out-Null
        Write-LaunchLog "RUNTIME VALIDATION refused (server not Ready, state=$($script:Server.State))"
        return
    }

    if ($script:LoadTest.State -eq "Running" -or (Test-ProcessAlive -Process $script:LoadTest.Process) -or $null -ne $script:PendingRuntimeSpec) {
        [Windows.Forms.MessageBox]::Show(
            "Runtime Validation is already running. Stop it before starting another.",
            "RUNTIME VALIDATION",
            "OK",
            "Warning"
        ) | Out-Null
        Write-LaunchLog "RUNTIME VALIDATION refused (already running)"
        return
    }

    $spec = Show-RuntimeValidationDialog
    if ($null -eq $spec) { return }

    $stamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $persist = Join-Path (Get-LoadLogsRoot) ("rv_{0}\persist" -f $stamp)
    New-Item -ItemType Directory -Path $persist -Force | Out-Null
    $argv = Get-RuntimeValidationArgv -Spec $spec -PersistRoot $persist

    # Official RV must not launch a stale purgatory-load that still accepts --preset
    # but lacks soak roles / in-zone portal. Rebuild the load binary first.
    Write-LaunchLog "RUNTIME VALIDATION rebuilding $($script:LoadPackage) --bin purgatory-load before launch"
    $script:PendingRuntimeSpec = @{
        Spec    = $spec
        Persist = $persist
        Argv    = $argv
        Phase   = "load-build"
    }
    if (-not (Start-OwnedBuild -Packages @($script:LoadPackage) -Reason "runtime-val-prep")) {
        Write-LaunchLog "RUNTIME VALIDATION could not start a load-binary rebuild"
        $script:PendingRuntimeSpec = $null
    }
}

function Start-RuntimeValidationPrepared {
    param($Spec, [string]$Persist, [string[]]$Argv, $CliEnv)

    $envMap = Get-ChildEnvironment
    $envMap["PURGATORY_ADMISSION_CAP"] = "$($script:LoadAdmissionCap)"
    $envMap["PURGATORY_METRICS_PORT"] = "$($script:MetricsPort)"
    $envMap["PURGATORY_DATA_DIR"] = $Persist
    if ($CliEnv) {
        foreach ($pair in @($CliEnv)) {
            if ($pair.Count -ge 2) {
                $envMap[[string]$pair[0]] = [string]$pair[1]
            }
        }
    }

    $msg = "Official Runtime Validation restarts the server into a clean load-validation mode with an isolated persist root.`n`nRestart now?"
    $ans = [Windows.Forms.MessageBox]::Show($msg, "RUNTIME VALIDATION", "YesNo", "Question")
    if ($ans -ne [Windows.Forms.DialogResult]::Yes) {
        Write-LaunchLog "RUNTIME VALIDATION cancelled (fresh server restart declined)"
        return
    }
    $script:PendingRuntimeSpec = @{
        Spec    = $Spec
        Persist = $Persist
        Argv    = $Argv
        Phase   = "server-ready"
    }
    $script:Server.ExtraEnv = $envMap
    $script:Server.WantLoadMode = $true
    $script:Server.RestartAfterStop = $true
    if (Test-ProcessAlive -Process $script:Server.Process) {
        Request-ServerStop
        $script:Server.RestartAfterStop = $true
        $script:Server.WantLoadMode = $true
        return
    }
    Request-ServerStart -LoadMode
}

function Continue-RuntimeValidationAfterLoadBuild {
    if ($null -eq $script:PendingRuntimeSpec) { return }
    if ([string]$script:PendingRuntimeSpec.Phase -ne "load-build") { return }

    $pending = $script:PendingRuntimeSpec
    $check = Get-CliServerEnv -Argv $pending.Argv
    Write-LoadCliDiagnostics -Capture $check.Capture `
        -RebuiltTarget $script:LoadPackage `
        -RebuiltExe (Get-LoadExe)

    if (-not $check.Ok) {
        $script:PendingRuntimeSpec = $null
        $detail = "purgatory-load still rejected the command after rebuilding purgatory-load."
        if ($check.Capture -and $check.Capture.Stderr) {
            $detail = $check.Capture.Stderr
        }
        Write-LaunchLog "RUNTIME VALIDATION still failed after rebuilding $($script:LoadPackage) --bin purgatory-load"
        [Windows.Forms.MessageBox]::Show(
            "$detail`n`nSee launcher log for exe path, argv, exit, and stderr.`nThis is not a Mixed/Soak duration-display issue.",
            "RUNTIME VALIDATION",
            "OK",
            "Error"
        ) | Out-Null
        return
    }
    if ($script:Server.State -ne "Ready") {
        if (-not $pending.LoggedWaitReady) {
            Write-LaunchLog "RUNTIME VALIDATION load binary rebuilt; waiting for server Ready"
            $script:PendingRuntimeSpec.LoggedWaitReady = $true
        }
        return
    }
    $script:PendingRuntimeSpec = $null
    Start-RuntimeValidationPrepared -Spec $pending.Spec -Persist $pending.Persist -Argv $pending.Argv -CliEnv $check.Env
}

function Start-RuntimeValidationHarness {
    param([string[]]$Argv, [string]$Label)

    $exe = Get-LoadExe
    Write-LaunchLog ("Runtime validation launching {0}" -f (Format-ExeIdentity -ExePath $exe))
    Write-LaunchLog ("argv: purgatory-load {0}" -f (ConvertTo-ArgumentString -ArgumentList $Argv))
    # No VisibleConsole: 1 Hz dashboard/live-status writeln dings an unfocused console.
    # Live progress is the GUI Runtime Val line (live_status.json). stdout still goes to logs/dev-tools/load.log.
    try {
        $proc = Start-OwnedProcess `
            -FilePath $exe `
            -ArgumentList $Argv `
            -Environment (Get-ChildEnvironment) `
            -RedirectOutput `
            -LogName "load"
    }
    catch {
        Write-LaunchLog "Runtime validation harness failed to start: $($_.Exception.Message)"
        return
    }
    $script:LoadTest.Process = $proc
    $script:LoadTest.State = "Running"
    $script:LoadTest.Kind = "runtime-validation"
    $script:LoadTest.StartedAt = [datetime]::UtcNow
    Write-LaunchLog "Runtime validation started: preset=$Label (CLI argv only; pass/fail in Rust)"
}

function Get-RuntimeValidationUiLine {
    $alive = [bool](Test-ProcessAlive -Process $script:LoadTest.Process)
    if ($alive) {
        $live = Get-RuntimeValidationLiveStatus
        if ($live) { return [string]$live }
        $elapsed = ""
        if ($script:LoadTest.StartedAt) {
            $sec = [int]([datetime]::UtcNow - [datetime]$script:LoadTest.StartedAt).TotalSeconds
            if ($sec -lt 0) { $sec = 0 }
            $elapsed = (" {0:00}:{1:00}" -f [int]($sec / 60), ($sec % 60))
        }
        return "RUNNING$elapsed | harness alive | waiting live_status.json"
    }
    $lastRv = Get-LastRuntimeValidationDir
    $current = Get-CurrentLoadRunName
    if ($current) {
        return "Runtime Val: idle (current_run leftover: $current)"
    }
    if ($lastRv) {
        $name = Split-Path -Leaf $lastRv
        return "Runtime Val: last artifact $name"
    }
    return "Runtime Val: idle"
}

function Get-CurrentLoadRunName {
    $pointer = Join-Path (Get-LoadLogsRoot) "current_run.txt"
    if (-not (Test-Path -LiteralPath $pointer)) { return $null }
    $name = (Get-Content -LiteralPath $pointer -Raw).Trim()
    if ([string]::IsNullOrWhiteSpace($name)) { return $null }
    return $name
}

function Get-RuntimeValidationLiveStatus {
    $name = Get-CurrentLoadRunName
    if (-not $name) { return $null }
    $path = Join-Path (Get-LoadLogsRoot) (Join-Path $name "live_status.json")
    if (-not (Test-Path -LiteralPath $path)) { return $null }
    try {
        $obj = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
        if ($obj.status_line) { return [string]$obj.status_line }
    }
    catch { }
    return $null
}

function Get-LastRuntimeValidationDir {
    $pointer = Join-Path (Get-LoadLogsRoot) "last_runtime_validation.txt"
    if (-not (Test-Path -LiteralPath $pointer)) { return $null }
    $name = (Get-Content -LiteralPath $pointer -Raw).Trim()
    if ([string]::IsNullOrWhiteSpace($name)) { return $null }
    $dir = Join-Path (Get-LoadLogsRoot) $name
    if (-not (Test-Path -LiteralPath $dir)) { return $null }
    return $dir
}

function Update-PendingRuntimeValidation {
    if ($null -eq $script:PendingRuntimeSpec) { return }

    if ($script:Server.State -eq "Failed") {
        $reason = $script:Server.LastFailure
        if (-not $reason) { $reason = "server failed before runtime validation" }
        Write-LaunchLog $reason
        [Windows.Forms.MessageBox]::Show($reason, "RUNTIME VALIDATION", "OK", "Error") | Out-Null
        $script:PendingRuntimeSpec = $null
        $script:Server.ExtraEnv = $null
        return
    }

    if ([string]$script:PendingRuntimeSpec.Phase -eq "load-build") {
        if ($script:Server.State -eq "Ready" -and -not (Test-BuildRunning)) {
            Continue-RuntimeValidationAfterLoadBuild
        }
        return
    }
    if ($script:Server.State -ne "Ready") { return }

    $pending = $script:PendingRuntimeSpec
    $script:PendingRuntimeSpec = $null
    Start-RuntimeValidationHarness -Argv $pending.Argv -Label $pending.Spec.Preset
}
