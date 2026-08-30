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

function Get-CliServerEnv {
    param([string[]]$Argv)

    $exe = Get-LoadExe
    if (-not (Test-Path -LiteralPath $exe)) { return $null }
    $printArgs = @($Argv) + @("--print-server-env")
    $errFile = [IO.Path]::GetTempFileName()
    try {
        $json = & $exe @printArgs 2>$errFile
        $code = $LASTEXITCODE
        if ($code -ne 0) {
            $err = ""
            if (Test-Path -LiteralPath $errFile) {
                $err = (Get-Content -LiteralPath $errFile -Raw -ErrorAction SilentlyContinue)
            }
            $err = ([string]$err).Trim()
            if ($err) {
                Write-LaunchLog ("print-server-env failed (exit {0}): {1}" -f $code, $err)
            }
            else {
                Write-LaunchLog "print-server-env failed (exit $code)"
            }
            return $null
        }
        if ([string]::IsNullOrWhiteSpace($json)) { return $null }
        return $json | ConvertFrom-Json
    }
    catch {
        Write-LaunchLog "print-server-env failed: $($_.Exception.Message)"
        return $null
    }
    finally {
        Remove-Item -LiteralPath $errFile -Force -ErrorAction SilentlyContinue
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

    $spec = Show-RuntimeValidationDialog
    if ($null -eq $spec) { return }

    $stamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $persist = Join-Path (Get-LoadLogsRoot) ("rv_{0}\persist" -f $stamp)
    New-Item -ItemType Directory -Path $persist -Force | Out-Null
    $argv = Get-RuntimeValidationArgv -Spec $spec -PersistRoot $persist

    $exe = Get-LoadExe
    $cliEnv = $null
    if (Test-Path -LiteralPath $exe) {
        $cliEnv = Get-CliServerEnv -Argv $argv
    }
    if (-not (Test-Path -LiteralPath $exe) -or $null -eq $cliEnv) {
        Write-LaunchLog "purgatory-load does not accept this argv (stale binary or missing). Rebuilding $($script:LoadPackage)..."
        $script:PendingRuntimeSpec = @{
            Spec   = $spec
            Persist = $persist
            Argv   = $argv
            Phase  = "load-build"
        }
        if (-not (Start-OwnedBuild -Packages @($script:LoadPackage) -Reason "runtime-val-prep")) {
            Write-LaunchLog "RUNTIME VALIDATION could not start a load-binary rebuild"
            $script:PendingRuntimeSpec = $null
        }
        return
    }

    Start-RuntimeValidationPrepared -Spec $spec -Persist $persist -Argv $argv -CliEnv $cliEnv
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

    $probe = Probe-ServerMetrics
    $needRestart = -not (Test-LoadProbeCompatible -Probe $probe -Count 8)
    if ($needRestart -or -not $script:Server.WantLoadMode) {
        $msg = "Server must restart in load-validation mode (isolated persist, admission 256).`n`nRestart now?"
        $ans = [Windows.Forms.MessageBox]::Show($msg, "RUNTIME VALIDATION", "YesNo", "Question")
        if ($ans -ne [Windows.Forms.DialogResult]::Yes) {
            Write-LaunchLog "RUNTIME VALIDATION cancelled (server env not applied)"
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
        Request-ServerStop
        $script:Server.RestartAfterStop = $true
        $script:Server.WantLoadMode = $true
        return
    }

    Start-RuntimeValidationHarness -Argv $Argv -Label $Spec.Preset
}

function Continue-RuntimeValidationAfterLoadBuild {
    if ($null -eq $script:PendingRuntimeSpec) { return }
    if ([string]$script:PendingRuntimeSpec.Phase -ne "load-build") { return }

    $pending = $script:PendingRuntimeSpec
    $cliEnv = Get-CliServerEnv -Argv $pending.Argv
    if ($null -eq $cliEnv) {
        $script:PendingRuntimeSpec = $null
        Write-LaunchLog "RUNTIME VALIDATION still cannot parse argv after rebuilding $($script:LoadPackage)"
        [Windows.Forms.MessageBox]::Show(
            "purgatory-load still rejected --preset after rebuild.`nSee launcher log. This is not a Mixed result.",
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
    Start-RuntimeValidationPrepared -Spec $pending.Spec -Persist $pending.Persist -Argv $pending.Argv -CliEnv $cliEnv
}

function Start-RuntimeValidationHarness {
    param([string[]]$Argv, [string]$Label)

    $exe = Get-LoadExe
    try {
        $proc = Start-OwnedProcess `
            -FilePath $exe `
            -ArgumentList $Argv `
            -Environment (Get-ChildEnvironment) `
            -VisibleConsole
    }
    catch {
        Write-LaunchLog "Runtime validation harness failed to start: $($_.Exception.Message)"
        return
    }
    $script:LoadTest.Process = $proc
    $script:LoadTest.State = "Running"
    Write-LaunchLog "Runtime validation started: preset=$Label (CLI argv only; pass/fail in Rust)"
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
