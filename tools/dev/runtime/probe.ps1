# Metrics (Health) and connection probe (Readiness). Listener observation is diagnostic only.

function Get-UdpListenDiagnostic {
    $now = [datetime]::UtcNow
    if (($now - $script:ListenerCacheAt).TotalMilliseconds -lt 800) {
        return [string]$script:ListenerCache
    }

    $listening = $false
    $ok = $false
    try {
        $props = [Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties()
        foreach ($ep in $props.GetActiveUdpListeners()) {
            if ($ep.Port -eq $script:ListenPort) {
                $listening = $true
                break
            }
        }
        $ok = $true
    }
    catch { }

    if (-not $ok) {
        $script:ListenerCache = "unknown"
    }
    elseif ($listening) {
        $script:ListenerCache = "yes"
    }
    else {
        $script:ListenerCache = "no"
    }
    $script:ListenerCacheAt = $now
    return [string]$script:ListenerCache
}

function Probe-ServerMetrics {
    param([int]$TimeoutMs = 150)

    $now = [datetime]::UtcNow
    if (($now - $script:MetricsCacheAt).TotalMilliseconds -lt 400 -and $null -ne $script:MetricsCache) {
        return $script:MetricsCache
    }

    $udp = $null
    try {
        $udp = New-Object System.Net.Sockets.UdpClient
        $udp.Client.ReceiveTimeout = $TimeoutMs
        $udp.Connect($script:ListenHost, $script:MetricsPort)
        $req = [Text.Encoding]::ASCII.GetBytes("PURGSTAT") + [byte[]]@(1)
        [void]$udp.Send($req, $req.Length)
        $remote = New-Object System.Net.IPEndPoint([Net.IPAddress]::Any, 0)
        $bytes = $udp.Receive([ref]$remote)
        if ($null -eq $bytes -or $bytes.Length -lt 11) { return $null }
        $magic = [Text.Encoding]::ASCII.GetString($bytes, 0, 8)
        if ($magic -ne "PURGSTAT" -or $bytes[8] -ne 1) { return $null }
        $len = [BitConverter]::ToUInt16($bytes, 9)
        if ($bytes.Length -lt (11 + $len)) { return $null }
        $json = [Text.Encoding]::UTF8.GetString($bytes, 11, $len)
        $obj = ($json | ConvertFrom-Json)
        $script:MetricsCache = $obj
        $script:MetricsCacheAt = [datetime]::UtcNow
        return $obj
    }
    catch {
        return $null
    }
    finally {
        if ($null -ne $udp) { $udp.Close() }
    }
}

function Test-LoadProbeCompatible {
    param($Probe, [int]$Count)

    if ($null -eq $Probe) { return $false }
    if ([int]$Probe.metrics_schema_version -lt 1) { return $false }
    if ([int]$Probe.admission_cap -lt $Count) { return $false }
    if ([int]$Probe.max_entities_per_snapshot -lt $Count) { return $false }
    return $true
}

function Test-ServerExeSupportsMetrics {
    $exe = Get-ServerExe
    if (-not (Test-Path -LiteralPath $exe)) { return $false }
    try {
        $proc = Start-Process -FilePath "findstr.exe" `
            -ArgumentList @("/M", "/C:PURGSTAT", $exe) `
            -Wait -PassThru -WindowStyle Hidden -ErrorAction Stop
        if ($null -eq $proc) { return $false }
        return ($proc.ExitCode -eq 0)
    }
    catch {
        return $false
    }
}

function Start-ConnectionProbe {
    if (Test-ProcessAlive -Process $script:Server.ProbeProcess) { return }

    $exe = Get-LoadExe
    if (-not (Test-Path -LiteralPath $exe)) { return $false }

    $server = "{0}:{1}" -f $script:ListenHost, $script:ListenPort
    try {
        $proc = Start-OwnedProcess `
            -FilePath $exe `
            -ArgumentList @("--probe", "--server", $server) `
            -Environment (Get-ChildEnvironment) `
            -CaptureOnExit `
            -LogName "probe"
    }
    catch {
        $script:Health.Connection = "fail"
        $script:Health.ConnectionReason = "failed to start probe: $($_.Exception.Message)"
        return $false
    }
    $script:Server.ProbeProcess = $proc
    if (-not $script:ProbeLoggedStart) {
        Write-LaunchLog "Connection probe started (dev.probe)"
        $script:ProbeLoggedStart = $true
    }
    return $true
}

function Stop-ConnectionProbe {
    Stop-OwnedProcess -Process $script:Server.ProbeProcess
    $script:Server.ProbeProcess = $null
}

function Read-LastProbeReason {
    $path = Join-Path (Get-DevLogDir) "probe.log"
    if (-not (Test-Path -LiteralPath $path)) { return "" }
    try {
        $lines = Get-Content -LiteralPath $path -Tail 8 -ErrorAction SilentlyContinue
        if ($null -eq $lines) { return "" }
        $hit = $lines | Where-Object { $_ -like "*purgatory-load --probe*" -or $_ -like "*probe*" } | Select-Object -Last 1
        if ($hit) { return [string]$hit }
        return [string]($lines | Select-Object -Last 1)
    }
    catch {
        return ""
    }
}

function Format-ReadinessFailure {
    param([string]$Reason)

    $alive = if (Test-ProcessAlive -Process $script:Server.Process) { "yes" } else { "no" }
    $health = if ($script:Health.MetricsOk) { "yes" } else { "no" }
    $conn = [string]$script:Health.Connection
    return ("Server readiness failed: process alive: {0}; listener: {1}; metrics: {2}; QUIC probe: {3}; reason: {4}" -f `
            $alive, $script:Health.Listener, $health, $conn, $Reason)
}
