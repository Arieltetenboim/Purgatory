# Client processes. Launch only after server Ready.

function Test-ClientPidKnown {
    param([int]$ProcessId)

    foreach ($row in @($script:Clients)) {
        if ($null -ne $row.Process -and $row.Process.Id -eq $ProcessId) { return $true }
    }
    return $false
}

function Prune-ClientList {
    $dead = 0
    for ($i = $script:Clients.Count - 1; $i -ge 0; $i--) {
        $row = $script:Clients[$i]
        if (-not (Test-ProcessAlive -Process $row.Process)) {
            $script:Clients.RemoveAt($i)
            $dead++
        }
    }
    return $dead
}

function Get-ClientLiveCount {
    Prune-ClientList | Out-Null
    return $script:Clients.Count
}

function Adopt-ClientProcess {
    param($Process)

    $script:ClientSerial++
    [void]$script:Clients.Add(@{
            Process   = $Process
            Serial    = $script:ClientSerial
            StartedAt = [datetime]::UtcNow
            Adopted   = $true
        })
    Write-LaunchLog "Adopted client pid $($Process.Id)"
}

function Start-OneClient {
    $exe = Get-ClientExe
    if (-not (Test-Path -LiteralPath $exe)) {
        Write-LaunchLog "Client executable missing"
        return $false
    }

    $script:ClientSerial++
    $n = $script:ClientSerial
    try {
        $proc = Start-OwnedProcess `
            -FilePath $exe `
            -Environment (Get-ChildEnvironment) `
            -RedirectOutput `
            -LogName "client"
    }
    catch {
        Write-LaunchLog "Client $n failed to start: $($_.Exception.Message)"
        return $false
    }

    [void]$script:Clients.Add(@{
            Process   = $proc
            Serial    = $n
            StartedAt = [datetime]::UtcNow
            Adopted   = $false
        })
    Write-LaunchLog ("Opening client {0} {1}" -f $n, (Format-ExeIdentity -ExePath $exe))
    return $true
}

function Start-QueuedClientLaunches {
    if ($script:Server.State -ne "Ready") {
        return
    }
    if ($script:PendingClients -le 0) { return }
    if ([datetime]::UtcNow -lt $script:ClientStaggerUntil) { return }

    if (-not (Test-Path -LiteralPath (Get-ClientExe))) {
        if (-not (Test-BuildRunning)) {
            [void](Start-OwnedBuild -Packages @($script:ClientPackage) -Reason "open-client")
        }
        return
    }

    if (Start-OneClient) {
        $script:PendingClients--
        if ($script:PendingClients -gt 0) {
            $script:ClientStaggerUntil = [datetime]::UtcNow.AddMilliseconds($script:ClientStaggerMs)
        }
    }
    else {
        Write-LaunchLog "Client launch failed; remaining queue=$($script:PendingClients)"
    }
}

function Request-Clients {
    param([int]$Count)

    if (-not $script:CargoPath) {
        Write-LaunchLog "cargo is not on PATH"
        return
    }

    $script:PendingClients += $Count
    Write-LaunchLog "Client request +$Count (queued=$($script:PendingClients))"

    if ($script:Server.State -ne "Ready") {
        Write-LaunchLog "Clients queued until server is Ready (state=$($script:Server.State))"
        return
    }

    $clientsRunning = (Get-ClientLiveCount) -gt 0
    $workspaceClients = @(Find-WorkspaceProcesses -Name "purgatory-client")
    $exeLocked = -not (Test-ExeUnlocked -Path (Get-ClientExe))
    if ($clientsRunning -or $workspaceClients.Count -gt 0 -or $exeLocked) {
        Write-LaunchLog "WARNING: client already running; cannot rebuild (exe locked). Stop clients first to pick up a new build. Launching existing exe."
        Start-QueuedClientLaunches
        return
    }

    [void](Start-OwnedBuild -Packages @($script:ClientPackage) -Reason "open-client")
}

function Request-StopClients {
    $script:PendingClients = 0
    $n = 0
    foreach ($row in @($script:Clients)) {
        Stop-OwnedProcess -Process $row.Process
        $n++
    }
    $script:Clients.Clear()
    $extra = Stop-WorkspaceByName -Name "purgatory-client"
    if ($n -eq 0 -and $extra -eq 0) {
        Write-LaunchLog "No client processes"
        return
    }
    Write-LaunchLog "Stopped client(s)"
}

function Update-ClientLifecycle {
    $dead = Prune-ClientList
    $live = $script:Clients.Count
    if ($live -lt $script:LastClientLive) {
        $dropped = $script:LastClientLive - $live
        if ($dropped -eq 1) { Write-LaunchLog "Client closed" }
        else { Write-LaunchLog "$dropped clients closed" }
    }
    $script:LastClientLive = $live
    Start-QueuedClientLaunches
}
