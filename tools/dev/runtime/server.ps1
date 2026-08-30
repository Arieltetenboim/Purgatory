# Server lifecycle. Process alive is required; Hello/Welcome is Ready.

function Set-ServerState {
    param(
        [Parameter(Mandatory = $true)][string]$State,
        [string]$Reason = $null
    )

    $prev = [string]$script:Server.State
    if ($Reason) { $script:Server.LastFailure = $Reason }
    $script:Server.State = $State
    if ($prev -ne $State) {
        $extra = ""
        if ($Reason) { $extra = ": $Reason" }
        Write-LaunchLog "Server $prev -> $State$extra"
    }
}

function Request-ServerStart {
    param([switch]$LoadMode)

    if (-not $script:CargoPath) {
        Write-LaunchLog "cargo is not on PATH"
        [Windows.Forms.MessageBox]::Show(
            "cargo was not found on PATH.`nInstall Rust via rustup, then reopen Developer Tools.",
            $script:WindowTitle,
            [Windows.Forms.MessageBoxButtons]::OK,
            [Windows.Forms.MessageBoxIcon]::Warning
        ) | Out-Null
        return
    }

    if ($LoadMode) {
        $script:Server.WantLoadMode = $true
    }

    if (Test-ProcessAlive -Process $script:Server.Process) {
        if ($LoadMode) {
            $script:Server.RestartAfterStop = $true
            Request-ServerStop
            return
        }
        Write-LaunchLog "Server already running"
        return
    }

    if ($script:Server.State -eq "Building" -and (Test-BuildRunning)) {
        Write-LaunchLog "Server build already in progress"
        return
    }

    $reason = "start-server"
    if ($script:Server.WantLoadMode) { $reason = "start-server-load" }
    Set-ServerState -State "Building"
    if (-not (Start-OwnedBuild -Packages @($script:ServerPackage) -Reason $reason)) {
        $detail = "could not start cargo"
        if ($script:CargoPath) {
            $detail = "could not start cargo ($($script:CargoPath))"
        }
        Set-ServerState -State "Failed" -Reason $detail
    }
}

function Start-ServerProcess {
    $exe = Get-ServerExe
    if (-not (Test-Path -LiteralPath $exe)) {
        Set-ServerState -State "Failed" -Reason "server executable missing after build"
        return
    }
    if (Test-ProcessAlive -Process $script:Server.Process) {
        Set-ServerState -State "Starting"
        return
    }

    $envMap = Get-ChildEnvironment
    if ($script:Server.WantLoadMode) {
        $envMap["PURGATORY_ADMISSION_CAP"] = "$($script:LoadAdmissionCap)"
        $envMap["PURGATORY_METRICS_PORT"] = "$($script:MetricsPort)"
    }
    if ($script:Server.ExtraEnv) {
        foreach ($key in @($script:Server.ExtraEnv.Keys)) {
            $envMap[$key] = $script:Server.ExtraEnv[$key]
        }
    }

    try {
        $proc = Start-OwnedProcess `
            -FilePath $exe `
            -Environment $envMap `
            -RedirectOutput `
            -LogName "server"
    }
    catch {
        Set-ServerState -State "Failed" -Reason "server executable failed to start: $($_.Exception.Message)"
        return
    }

    $script:Server.Process = $proc
    $script:Server.Pid = $proc.Id
    $script:Server.StartedAt = [datetime]::UtcNow
    $script:Server.Adopted = $false
    $script:ProbePrepAttempted = $false
    $script:Health.Connection = "unknown"
    $script:Health.ConnectionReason = ""
    $script:Health.MetricsOk = $false
    Set-ServerState -State "Starting"
    if ($script:Server.WantLoadMode) {
        Write-LaunchLog ("Starting server LOAD MODE admission={0} {1}" -f $script:LoadAdmissionCap, (Format-ExeIdentity -ExePath $exe))
    }
    else {
        Write-LaunchLog ("Starting server ({0}) {1}" -f $script:BuildProfile, (Format-ExeIdentity -ExePath $exe))
    }
}

function Request-ServerStop {
    param([switch]$ClearLoadQueue)

    if ($ClearLoadQueue) {
        $script:PendingLoadSpec = $null
        $script:PendingRuntimeSpec = $null
        $script:Server.ExtraEnv = $null
    }
    Stop-ConnectionProbe

    $wasBuilding = ($script:Server.State -eq "Building")
    if (Test-BuildRunning) {
        Stop-OwnedProcess -Process $script:Build.Process
        $script:Build.Process = $null
        $script:Build.Reason = $null
    }

    if (Test-ProcessAlive -Process $script:Server.Process) {
        Set-ServerState -State "Stopping"
        Stop-OwnedProcess -Process $script:Server.Process
        return
    }

    $killed = Stop-WorkspaceByName -Name "purgatory-server"
    if ($killed -eq 0 -and -not $wasBuilding) {
        Set-ServerState -State "Stopped"
        Write-LaunchLog "Server is not running"
        return
    }
    Set-ServerState -State "Stopping"
}

function Request-ServerRestart {
    $script:Server.RestartAfterStop = $true
    if (Test-ProcessAlive -Process $script:Server.Process -or $script:Server.State -eq "Building") {
        Request-ServerStop
        $script:Server.RestartAfterStop = $true
        return
    }
    $script:Server.RestartAfterStop = $false
    Request-ServerStart
}

function Adopt-ServerProcess {
    param($Process)

    $script:Server.Process = $Process
    $script:Server.Pid = $Process.Id
    $script:Server.StartedAt = [datetime]::UtcNow
    $script:Server.Adopted = $true
    $script:Health.Connection = "unknown"
    Set-ServerState -State "Starting" -Reason "adopted pid $($Process.Id)"
}

function Update-ServerLifecycle {
    Complete-OwnedBuildIfExited

    $state = [string]$script:Server.State
    $alive = Test-ProcessAlive -Process $script:Server.Process
    $script:Health.ProcessAlive = $alive
    $script:Health.Listener = Get-UdpListenDiagnostic

    if ($state -eq "Stopping") {
        if (-not $alive) {
            $script:Server.Process = $null
            $script:Server.Pid = 0
            Set-ServerState -State "Stopped"
            Reset-HealthSnapshot
            $restart = [bool]$script:Server.RestartAfterStop
            $script:Server.RestartAfterStop = $false
            if ($restart) {
                Request-ServerStart
            }
        }
        return
    }

    if ($state -in @("Starting", "Verifying", "Ready", "Degraded")) {
        if (-not $alive) {
            Stop-ConnectionProbe
            Set-ServerState -State "Failed" -Reason "server process exited unexpectedly"
            $script:Server.Process = $null
            $script:Server.Pid = 0
            return
        }
    }

    if ($state -eq "Building") {
        if (-not (Test-BuildRunning) -and $null -eq $script:Build.Process) {
            # Complete-OwnedBuildIfExited already handled success/fail.
        }
        return
    }

    if ($state -eq "Starting") {
        Update-HealthFromMetrics
        if (-not (Test-Path -LiteralPath (Get-LoadExe))) {
            if (-not (Test-BuildRunning)) {
                [void](Start-OwnedBuild -Packages @($script:LoadPackage) -Reason "probe-prep")
            }
            return
        }
        if ((Test-BuildRunning) -and $script:Build.Reason -eq "probe-prep") {
            return
        }
        Set-ServerState -State "Verifying"
        $script:Server.VerifyStartedAt = [datetime]::UtcNow
        $script:ProbeLoggedStart = $false
        $script:ProbeLoggedFail = $false
        $script:ProbeNextAt = [datetime]::MinValue
        [void](Start-ConnectionProbe)
        return
    }

    if ($state -eq "Verifying") {
        Update-HealthFromMetrics
        $elapsed = 0
        if ($script:Server.VerifyStartedAt) {
            $elapsed = ([datetime]::UtcNow - $script:Server.VerifyStartedAt).TotalSeconds
        }
        if ($elapsed -gt $script:ReadyTimeoutSec) {
            Stop-ConnectionProbe
            $script:Health.Connection = "fail"
            $script:Health.ConnectionReason = "timed out"
            Set-ServerState -State "Failed" -Reason (Format-ReadinessFailure "readiness timed out after $($script:ReadyTimeoutSec)s")
            return
        }

        $probe = $script:Server.ProbeProcess
        if (-not (Test-ProcessAlive -Process $probe)) {
            if ($null -eq $probe) {
                if ([datetime]::UtcNow -lt $script:ProbeNextAt) { return }
                [void](Start-ConnectionProbe)
                return
            }
            $code = Get-ProcessExitCode -Process $probe
            Flush-OwnedProcessOutput -Process $probe
            $script:Server.ProbeProcess = $null
            if ($code -eq 0) {
                $script:Health.Connection = "pass"
                $script:Health.ConnectionReason = ""
                Set-ServerState -State "Ready"
            }
            elseif ($code -eq 2 -and -not $script:ProbePrepAttempted) {
                # Stale purgatory-load.exe (built before --probe) exits 2 on unknown clap flags.
                $script:ProbePrepAttempted = $true
                Write-LaunchLog "Probe binary rejected --probe (exit 2); rebuilding $($script:LoadPackage)"
                Set-ServerState -State "Starting"
                [void](Start-OwnedBuild -Packages @($script:LoadPackage) -Reason "probe-prep")
            }
            else {
                $detail = Read-LastProbeReason
                if (-not $detail) { $detail = "probe exit $code" }
                if (-not $script:ProbeLoggedFail) {
                    Write-LaunchLog "Probe unsuccessful ($detail); retrying until Ready timeout"
                    $script:ProbeLoggedFail = $true
                }
                $script:Health.Connection = "fail"
                $script:Health.ConnectionReason = $detail
                $script:ProbeNextAt = [datetime]::UtcNow.AddSeconds(1)
            }
        }
        return
    }

    if ($state -eq "Ready") {
        Update-HealthFromMetrics
        if (-not $script:Health.MetricsOk -and $script:Health.Connection -eq "pass") {
            # Metrics dropped after Ready: Degraded, keep process. Connection remains last pass.
            Set-ServerState -State "Degraded" -Reason "metrics health lost after Ready"
        }
        return
    }

    if ($state -eq "Degraded") {
        Update-HealthFromMetrics
        if ($script:Health.MetricsOk) {
            Set-ServerState -State "Ready"
        }
    }
}

function Update-HealthFromMetrics {
    $metrics = Probe-ServerMetrics
    if ($null -ne $metrics) {
        $script:Health.Metrics = $metrics
        $script:Health.MetricsOk = ([int]$metrics.metrics_schema_version -ge 1)
    }
    else {
        $script:Health.MetricsOk = $false
    }
}

function Invoke-StartupRecovery {
    $servers = @(Find-WorkspaceProcesses -Name "purgatory-server")
    if ($servers.Count -gt 1) {
        $keep = $servers[0]
        for ($i = 1; $i -lt $servers.Count; $i++) {
            Write-LaunchLog "Stopping extra workspace server pid $($servers[$i].Id)"
            Stop-ProcessTree -ProcessId $servers[$i].Id
        }
        $servers = @($keep)
    }
    if ($servers.Count -eq 1 -and -not (Test-ProcessAlive -Process $script:Server.Process)) {
        Adopt-ServerProcess -Process $servers[0].Process
    }

    foreach ($row in @(Find-WorkspaceProcesses -Name "purgatory-client")) {
        if (-not (Test-ClientPidKnown -ProcessId $row.Id)) {
            Adopt-ClientProcess -Process $row.Process
        }
    }

    $loads = @(Find-WorkspaceProcesses -Name "purgatory-load")
    if ($loads.Count -gt 0 -and -not (Test-ProcessAlive -Process $script:LoadTest.Process)) {
        $script:LoadTest.Process = $loads[0].Process
        $script:LoadTest.State = "Running"
        Write-LaunchLog "Adopted load harness pid $($loads[0].Id)"
    }
}

function Update-RecoveryScan {
    if (([datetime]::UtcNow - $script:LastRecoveryUtc).TotalSeconds -lt $script:RecoveryIntervalSec) {
        return
    }
    $script:LastRecoveryUtc = [datetime]::UtcNow

    if ($script:Server.State -eq "Stopped" -or $script:Server.State -eq "Failed") {
        if (-not (Test-ProcessAlive -Process $script:Server.Process)) {
            $servers = @(Find-WorkspaceProcesses -Name "purgatory-server")
            if ($servers.Count -eq 1) {
                Adopt-ServerProcess -Process $servers[0].Process
            }
        }
    }
}

function Request-KillAll {
    $script:PendingClients = 0
    $script:PendingLoadSpec = $null
    $script:PendingRuntimeSpec = $null
    $script:Server.RestartAfterStop = $false
    Stop-ConnectionProbe
    Stop-WorkspaceCargo -Package "*" | Out-Null
    $servers = Stop-WorkspaceByName -Name "purgatory-server"
    $clients = Stop-WorkspaceByName -Name "purgatory-client"
    $loads = Stop-WorkspaceByName -Name "purgatory-load"
    $script:Server.Process = $null
    $script:Server.Pid = 0
    $script:Clients.Clear()
    $script:LoadTest.Process = $null
    $script:LoadTest.State = "Stopped"
    Set-ServerState -State "Stopped"
    Reset-HealthSnapshot
    Write-LaunchLog "Kill all  (server=$servers  clients=$clients  load=$loads)"
}
