# Phase 7.4 — network throughput & backpressure (measure + justified policy tune).
#
# Suites: baseline → budget → cadence → slow-drain → dense → candidate → soak.
# Harness post-ramp thin is ON for N>=64 so snapshot starvation is not confused
# with server transport pressure. Slow-drain is an explicit diagnostic.
param(
    [switch]$SkipBuild,
    [string]$Only = "",
    [string]$DurationCell = "45s",
    [string]$DurationSoak = "8m",
    [int]$HealthMax = 6,
    [int]$RespawnDelayTicks = 20
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$summaryDir = Join-Path $root "logs\load\capacity_74\summary_$stamp"
New-Item -ItemType Directory -Force -Path $summaryDir | Out-Null

# Defaults for 7.4 (callers may override per-cell).
$env:PURGATORY_REPLICATION_POLICY = "selective"
$env:PURGATORY_POPULATION_CLASS = "high"

$ladder = Join-Path $root "scripts\capacity_ladder.ps1"
$script:skipBuild = [bool]$SkipBuild
$want = @{}
if ([string]::IsNullOrWhiteSpace($Only)) {
    foreach ($k in @("baseline", "budget", "cadence", "slow", "dense", "candidate", "soak")) {
        $want[$k] = $true
    }
} else {
    $parts = @($Only) -join ","
    foreach ($k in ($parts -split '[,;\s]+')) {
        $key = $k.Trim().ToLower()
        if ($key) { $want[$key] = $true }
    }
}
Write-Host ("Suites: " + (($want.Keys | Sort-Object) -join ", "))

function Invoke-74Cell {
    param(
        [string]$Name,
        [string]$Scenario,
        [int]$Count,
        [string]$Duration,
        [int]$RampMs = 50,
        [int]$MaxBots = 256,
        [int]$SlowDrainCount = 0,
        [switch]$PostRampThin,
        [int]$FrameBudgetBytes = -1,
        [int]$StrangerCadenceMult = -1,
        [string]$ReplicationPolicy = "selective",
        [string]$PopulationClass = "high",
        [string]$Tag = ""
    )
    Write-Host ""
    Write-Host "==== $Name ===="
    $params = @{
        Scenario = $Scenario
        Count = $Count
        Duration = $Duration
        Seed = 7404
        RampMs = $RampMs
        ArtifactRoot = "capacity_74"
        CapacityDetail = "1"
        MaxBots = $MaxBots
        HealthMax = $HealthMax
        RespawnDelayTicks = $RespawnDelayTicks
        ReplicationPolicy = $ReplicationPolicy
        PopulationClass = $PopulationClass
        SlowDrainCount = $SlowDrainCount
    }
    if ($FrameBudgetBytes -ge 0) { $params.FrameBudgetBytes = $FrameBudgetBytes }
    if ($StrangerCadenceMult -ge 1) { $params.StrangerCadenceMult = $StrangerCadenceMult }
    if ($PostRampThin) { $params.PostRampThin = $true }
    if ($script:skipBuild) { $params.SkipBuild = $true }
    & $ladder @params
    $code = $LASTEXITCODE
    $script:skipBuild = $true
    $pattern = "*_${Scenario}_${Count}n*"
    $latest = Get-ChildItem (Join-Path $root "logs\load\capacity_74") -Directory |
        Where-Object { $_.Name -like $pattern } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    [pscustomobject]@{
        name = $Name
        tag = $Tag
        scenario = $Scenario
        count = $Count
        duration = $Duration
        exit = $code
        dir = if ($latest) { $latest.Name } else { $null }
        path = if ($latest) { $latest.FullName } else { $null }
        frame_budget = $FrameBudgetBytes
        cadence_mult = $StrangerCadenceMult
        slow_drain = $SlowDrainCount
        post_ramp_thin = [bool]$PostRampThin
        policy = $ReplicationPolicy
        population = $PopulationClass
    }
}

function Read-74Cell {
    param($r)
    $row = [ordered]@{
        name = $r.name
        tag = $r.tag
        scenario = $r.scenario
        count_requested = $r.count
        duration = $r.duration
        exit = $r.exit
        dir = $r.dir
        frame_budget = $r.frame_budget
        cadence_mult = $r.cadence_mult
        slow_drain = $r.slow_drain
        post_ramp_thin = $r.post_ramp_thin
        policy = $r.policy
        population = $r.population
    }
    if (-not $r.path) { return [pscustomobject]$row }

    $ramp = Join-Path $r.path "connection_ramp.json"
    if (Test-Path $ramp) {
        $j = Get-Content $ramp -Raw | ConvertFrom-Json
        $row.spawn_issued = $j.spawn_issued_total
        $row.peak_active = $j.peak_active_clients
        $row.attainment_pct = $j.attainment_pct
        $row.funnel_invariant_ok = $j.funnel_invariant_ok
        $row.controller_tick_p99_ms = $j.controller_tick_p99_ms
    }

    $td = Join-Path $r.path "tick_domains.json"
    if (Test-Path $td) {
        $j = Get-Content $td -Raw | ConvertFrom-Json
        $row.tick_p95 = $j.tick_total.p95_ms
        $row.tick_p99 = $j.tick_total.p99_ms
        $row.tick_util = $j.tick_utilization_pct
        $row.overruns = $j.tick_overrun_count
        $row.dominant = $j.dominant_owner
        if ($null -ne $j.replication) { $row.replication_mean = $j.replication.mean_ms }
        if ($null -ne $j.replication_encode) { $row.encode_mean = $j.replication_encode.mean_ms }
        if ($null -ne $j.replication_enqueue) { $row.enqueue_mean = $j.replication_enqueue.mean_ms }
        if ($null -ne $j.npc_activity) { $row.npc_activity_mean = $j.npc_activity.mean_ms }
    }

    $live = Join-Path $r.path "capacity_live.json"
    if (Test-Path $live) {
        $lj = Get-Content $live -Raw | ConvertFrom-Json
        $row.saturation_class = $lj.saturation_class
        $row.write_drain_p99_ms = $lj.write_drain_p99_ms
        $row.cpu_pct = $lj.cpu_utilization_pct
        if ($null -ne $lj.working_set_bytes) {
            $row.server_rss_mb = [math]::Round($lj.working_set_bytes / 1MB, 2)
        }
    }

    $np = Join-Path $r.path "network_pressure.json"
    if (Test-Path $np) {
        $nj = Get-Content $np -Raw | ConvertFrom-Json
        $row.bytes_out_per_sec = $nj.bytes_out_per_sec
        $row.bytes_out_per_session = $nj.bytes_out_per_session_per_sec
        $row.queue_depth_max = $nj.writer_queue_depth_max
        $row.queue_push_fail = $nj.writer_queue_push_fail_total
        $row.queue_age_p99_us = $nj.queue_age_us.p99
        $row.write_drain_p99_us = $nj.write_drain.p99
        $row.frames_publish = $nj.frames_publish_attempt_total
        $row.frames_encoded = $nj.frames_encoded_total
        $row.frames_enqueued = $nj.frames_enqueued_total
        $row.frames_drained = $nj.frames_drained_total
        $row.bytes_encoded = $nj.bytes_encoded_total
        $row.bytes_drained = $nj.bytes_drained_total
        $row.write_calls = $nj.write_calls_total
        $row.queued_frames_current = $nj.queued_frames_current
        if ($nj.top_clients -and $nj.top_clients.Count -gt 0) {
            $row.slowest_client_id = $nj.top_clients[0].connection_id
            $row.slowest_client_depth = $nj.top_clients[0].queue_depth
            $row.slowest_client_age_us = $nj.top_clients[0].queue_age_max_us
            $row.slowest_client_fails = $nj.top_clients[0].enqueue_fails
        }
    }

    $rf = Join-Path $r.path "replication_fanout.json"
    if (Test-Path $rf) {
        $rj = Get-Content $rf -Raw | ConvertFrom-Json
        $row.repl_emitted = $rj.updates_emitted_total
        $row.repl_bytes = $rj.bytes_emitted_total
        $row.budget_deferred = $rj.budget_deferred_total
        $row.priority_deferred = $rj.priority_deferred_total
        $row.cadence_deferred = $rj.cadence_deferred_total
        $row.state_coalesced = $rj.state_coalesced_total
        $row.policy_eligible = $rj.policy_eligible_total
        $row.policy_suppressed = $rj.policy_domain_suppressed_total
        $row.recovery = $rj.recovery_rescues_total
    }

    $pr = Join-Path $r.path "process_resources.json"
    if (Test-Path $pr) {
        $pj = Get-Content $pr -Raw | ConvertFrom-Json
        if ($null -ne $pj.working_set_bytes) {
            $row.server_rss_mb = [math]::Round($pj.working_set_bytes / 1MB, 2)
        }
        if ($null -ne $pj.working_set_peak_bytes) {
            $row.server_rss_peak_mb = [math]::Round($pj.working_set_peak_bytes / 1MB, 2)
        }
        if ($null -ne $pj.working_set_start_bytes) {
            $row.server_rss_start_mb = [math]::Round($pj.working_set_start_bytes / 1MB, 2)
        }
    }

    $rs = Join-Path $r.path "run_summary.json"
    if (Test-Path $rs) {
        $sj = Get-Content $rs -Raw | ConvertFrom-Json
        $row.harness_status = $sj.run_status
        $row.snapshot_starvation = $sj.snapshot_starvation_samples
        $row.failure_class = $sj.failure_class
        $row.aoi_enters = $sj.aoi_enters_total
        $row.aoi_leaves = $sj.aoi_leaves_total
    }

    $starve = 0
    if ($null -ne $row.snapshot_starvation) { $starve = [int64]$row.snapshot_starvation }
    $row.harness_caveat = ($starve -gt 0)
    return [pscustomobject]$row
}

$results = @()

if ($want["baseline"]) {
    $results += Invoke-74Cell -Name "baseline_mixed64" -Scenario "representative-mixed" -Count 64 `
        -Duration $DurationCell -RampMs 25 -PostRampThin -Tag "baseline"
    $results += Invoke-74Cell -Name "baseline_mixed128" -Scenario "representative-mixed" -Count 128 `
        -Duration $DurationCell -RampMs 25 -PostRampThin -Tag "baseline"
}

if ($want["budget"]) {
    foreach ($b in @(4096, 2048, 1024, 512)) {
        $results += Invoke-74Cell -Name "budget_mixed64_$b" -Scenario "representative-mixed" -Count 64 `
            -Duration $DurationCell -RampMs 25 -PostRampThin -FrameBudgetBytes $b -Tag "budget"
    }
}

if ($want["cadence"]) {
    foreach ($m in @(1, 2, 4)) {
        $results += Invoke-74Cell -Name "cadence_mixed64_x$m" -Scenario "representative-mixed" -Count 64 `
            -Duration $DurationCell -RampMs 25 -PostRampThin -StrangerCadenceMult $m -Tag "cadence"
    }
}

if ($want["slow"]) {
    # Controlled slow receivers among healthy peers (mixed@64).
    $results += Invoke-74Cell -Name "slow_mixed64_4" -Scenario "representative-mixed" -Count 64 `
        -Duration $DurationCell -RampMs 25 -PostRampThin -SlowDrainCount 4 -Tag "slow"
    $results += Invoke-74Cell -Name "slow_mixed32_2" -Scenario "representative-mixed" -Count 32 `
        -Duration $DurationCell -RampMs 50 -PostRampThin -SlowDrainCount 2 -Tag "slow"
}

if ($want["dense"]) {
    $results += Invoke-74Cell -Name "dense32" -Scenario "representative-dense" -Count 32 `
        -Duration $DurationCell -RampMs 50 -PostRampThin -Tag "dense"
    $results += Invoke-74Cell -Name "dense64" -Scenario "representative-dense" -Count 64 `
        -Duration $DurationCell -RampMs 25 -PostRampThin -Tag "dense"
}

if ($want["candidate"]) {
    # Candidate: selective/high + soft budget 2048 + stranger cadence ×2 (tuned after budget/cadence cells).
    $results += Invoke-74Cell -Name "candidate_mixed64" -Scenario "representative-mixed" -Count 64 `
        -Duration $DurationCell -RampMs 25 -PostRampThin -FrameBudgetBytes 2048 -StrangerCadenceMult 2 `
        -Tag "candidate"
    $results += Invoke-74Cell -Name "candidate_dense32" -Scenario "representative-dense" -Count 32 `
        -Duration $DurationCell -RampMs 50 -PostRampThin -FrameBudgetBytes 2048 -StrangerCadenceMult 2 `
        -Tag "candidate"
}

if ($want["soak"]) {
    $results += Invoke-74Cell -Name "soak_mixed64" -Scenario "representative-mixed" -Count 64 `
        -Duration $DurationSoak -RampMs 25 -PostRampThin -FrameBudgetBytes 2048 -StrangerCadenceMult 2 `
        -Tag "soak"
}

$summary = @()
foreach ($r in $results) { $summary += Read-74Cell $r }
$path = Join-Path $summaryDir "phase74_summary.json"
$summary | ConvertTo-Json -Depth 8 | Set-Content $path
$summary | Select-Object name, exit, spawn_issued, peak_active, attainment_pct, tick_p99, tick_util, `
    bytes_out_per_sec, bytes_out_per_session, queue_depth_max, queue_push_fail, budget_deferred, `
    frames_enqueued, frames_drained, saturation_class, harness_caveat |
    Format-Table -AutoSize | Out-String | Write-Host
Write-Host "Summary: $path"
exit 0
