# Direct-owned cargo. ExitCode is the only success signal.

function Test-BuildRunning {
    return (Test-ProcessAlive -Process $script:Build.Process)
}

function Start-OwnedBuild {
    param(
        [Parameter(Mandatory = $true)][string[]]$Packages,
        [Parameter(Mandatory = $true)][string]$Reason
    )

    if (-not $script:CargoPath) {
        Write-LaunchLog "cargo is not on PATH"
        return $false
    }
    if (Test-BuildRunning) {
        Write-LaunchLog ("Build already running ({0}: {1})" -f $script:Build.Reason, ($script:Build.Packages -join ","))
        return $false
    }

    $cargoArgs = Get-CargoArgumentList -Packages $Packages
    try {
        $proc = Start-OwnedProcess `
            -FilePath $script:CargoPath `
            -ArgumentList $cargoArgs `
            -Environment (Get-ChildEnvironment) `
            -RedirectOutput `
            -LogName "cargo"
    }
    catch {
        Write-LaunchLog "Failed to start cargo: $($_.Exception.Message)"
        return $false
    }

    $script:Build.Process = $proc
    $script:Build.Packages = @($Packages)
    $script:Build.Reason = $Reason
    $script:Build.StartedAt = [datetime]::UtcNow
    Write-LaunchLog ("Building {0} ({1}) reason={2}" -f ($Packages -join ","), $script:BuildProfile, $Reason)
    return $true
}

function Complete-OwnedBuildIfExited {
    if ($null -eq $script:Build.Process) { return }
    if (-not $script:Build.Process.HasExited) { return }

    $code = Get-ProcessExitCode -Process $script:Build.Process
    $reason = [string]$script:Build.Reason
    $packages = @($script:Build.Packages)
    $script:Build.Process = $null
    $script:Build.Reason = $null
    $script:Build.Packages = @()

    if ($code -ne 0) {
        Write-LaunchLog ("Build FAILED exit={0} ({1})" -f $code, ($packages -join ","))
        switch ($reason) {
            "start-server" { Set-ServerState -State "Failed" -Reason "server build failed (exit $code)" }
            "start-server-load" { Set-ServerState -State "Failed" -Reason "load-mode server build failed (exit $code)" }
            "open-client" { Write-LaunchLog "Client build failed; queued clients not launched" }
            "probe-prep" { Set-ServerState -State "Failed" -Reason "probe binary build failed (exit $code)" }
            "runtime-val-prep" {
                $script:PendingRuntimeSpec = $null
                Write-LaunchLog "RUNTIME VALIDATION load-binary rebuild failed"
            }
            default { }
        }
        return
    }

    Write-LaunchLog ("Build OK ({0})" -f ($packages -join ","))
    switch ($reason) {
        "start-server" { Start-ServerProcess }
        "start-server-load" { Start-ServerProcess }
        "open-client" { Start-QueuedClientLaunches }
        "probe-prep" { }
        "runtime-val-prep" { Continue-RuntimeValidationAfterLoadBuild }
        "rebuild" { Write-LaunchLog "Rebuild finished" }
        default { }
    }
}

function Request-Rebuild {
    if (-not $script:CargoPath) {
        Write-LaunchLog "cargo is not on PATH"
        return
    }

    $packages = New-Object System.Collections.Generic.List[string]
    [void]$packages.Add($script:ServerPackage)
    $clientsRunning = (@(Find-WorkspaceProcesses -Name "purgatory-client")).Count -gt 0
    if ($clientsRunning) {
        Write-LaunchLog "Rebuild $($script:BuildProfile): server only (clients are running)"
    }
    else {
        [void]$packages.Add($script:ClientPackage)
        Write-LaunchLog "Rebuild $($script:BuildProfile) server + client"
    }
    [void](Start-OwnedBuild -Packages $packages.ToArray() -Reason "rebuild")
}
