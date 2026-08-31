# DevRuntimeState. Owned process objects are the primary source of truth.

function Initialize-DevState {
    $script:ClientSerial = 0
    $script:PendingClients = 0
    $script:ClientStaggerUntil = [datetime]::MinValue
    $script:PendingLoadSpec = $null
    $script:PendingRuntimeSpec = $null
    $script:LastRecoveryUtc = [datetime]::MinValue
    $script:LastClientLive = 0
    $script:MetricsCache = $null
    $script:MetricsCacheAt = [datetime]::MinValue
    $script:ListenerCache = "unknown"
    $script:ListenerCacheAt = [datetime]::MinValue

    $script:Build = @{
        Process  = $null
        Packages = @()
        Reason   = $null
        StartedAt = $null
    }

    $script:Server = @{
        Process          = $null
        Pid              = 0
        State            = "Stopped"
        StartedAt        = $null
        LastFailure      = $null
        WantLoadMode     = $false
        ExtraEnv         = $null
        Adopted          = $false
        RestartAfterStop = $false
        VerifyStartedAt  = $null
        ProbeProcess     = $null
    }

    $script:Clients = New-Object System.Collections.ArrayList

    $script:LoadTest = @{
        Process    = $null
        State      = "Stopped"
        Kind       = $null
        StartedAt  = $null
    }

    $script:Health = @{
        ProcessAlive     = $false
        Listener         = "unknown"
        Metrics          = $null
        MetricsOk        = $false
        Connection       = "unknown"
        ConnectionReason = ""
    }

    $script:LogoImage = $null
    $script:ChildLogNames = @{}
    $script:ProbePrepAttempted = $false
    $script:ProbeLoggedStart = $false
    $script:ProbeLoggedFail = $false
    $script:ProbeNextAt = [datetime]::MinValue
    $script:UiTimer = $null
    $script:UiClosing = $false
}

function Reset-HealthSnapshot {
    $script:Health.ProcessAlive = $false
    $script:Health.Listener = "unknown"
    $script:Health.Metrics = $null
    $script:Health.MetricsOk = $false
    $script:Health.Connection = "unknown"
    $script:Health.ConnectionReason = ""
    $script:MetricsCache = $null
    $script:MetricsCacheAt = [datetime]::MinValue
}
