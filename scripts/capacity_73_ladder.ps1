# Phase 7.3 — capacity ladders (measure only; no server/harness optimization).
#
# Order: lifecycle proof → player → NPC → activity → dense → stable soak.
# Default NPC health_max=6 so lifecycle counters fire in short cells.
#
# Usage:
#   ./scripts/capacity_73_ladder.ps1
#   ./scripts/capacity_73_ladder.ps1 -SkipBuild
#   ./scripts/capacity_73_ladder.ps1 -Only lifecycle,player
param(
    [switch]$SkipBuild,
    [string]$Only = "",
    [int]$HealthMax = 6,
    [int]$RespawnDelayTicks = 20,
    [string]$DurationCell = "45s",
    [string]$DurationLifecycle = "90s",
    [string]$DurationSoak = "8m"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$summaryDir = Join-Path $root "logs\load\capacity_73\summary_$stamp"
New-Item -ItemType Directory -Force -Path $summaryDir | Out-Null

$env:PURGATORY_REPLICATION_POLICY = "selective"
$env:PURGATORY_POPULATION_CLASS = "high"

$ladder = Join-Path $root "scripts\capacity_ladder.ps1"
$script:skipBuild = [bool]$SkipBuild
$want = @{}
if ([string]::IsNullOrWhiteSpace($Only)) {
    foreach ($k in @("lifecycle", "player", "npc", "activity", "dense", "soak")) { $want[$k] = $true }
} else {
    # Accept comma/space/semicolon separators; unquoted `a,b` becomes a PowerShell array joined by spaces.
    $parts = @($Only) -join ","
    foreach ($k in ($parts -split '[,;\s]+')) {
        $key = $k.Trim().ToLower()
        if ($key) { $want[$key] = $true }
    }
}
Write-Host ("Suites: " + (($want.Keys | Sort-Object) -join ", "))
if ($want.Count -eq 0) {
    Write-Error "No suites selected from -Only '$Only'"
    exit 1
}

function Invoke-73Cell {
    param(
        [string]$Name,
        [string]$Scenario,
        [int]$Count,
        [string]$Duration,
        [int]$RampMs = 50,
        [int]$MaxBots = 256,
        [int]$NpcCount = -1,
        [int]$ActionPeriodTicks = -1,
        [int]$PulsePeriodTicks = -1,
        [int]$ActivePct = -1,
        [int]$HotspotRadius = -1,
        [int]$ChurnCount = -1,
        [int]$CellHealthMax = -1,
        [int]$CellRespawnDelay = -1
    )
    Write-Host ""
    Write-Host "==== $Name ===="
    $params = @{
        Scenario = $Scenario
        Count = $Count
        Duration = $Duration
        Seed = 7303
        RampMs = $RampMs
        ArtifactRoot = "capacity_73"
        CapacityDetail = "1"
        MaxBots = $MaxBots
        HealthMax = $(if ($CellHealthMax -ge 0) { $CellHealthMax } else { $HealthMax })
        RespawnDelayTicks = $(if ($CellRespawnDelay -ge 0) { $CellRespawnDelay } else { $RespawnDelayTicks })
    }
    if ($NpcCount -ge 0) { $params.NpcCount = $NpcCount }
    if ($ActionPeriodTicks -ge 0) { $params.ActionPeriodTicks = $ActionPeriodTicks }
    if ($PulsePeriodTicks -ge 0) { $params.PulsePeriodTicks = $PulsePeriodTicks }
    if ($ActivePct -ge 0) { $params.ActivePct = $ActivePct }
    if ($HotspotRadius -ge 0) { $params.HotspotRadius = $HotspotRadius }
    if ($ChurnCount -ge 0) { $params.ChurnCount = $ChurnCount }
    if ($script:skipBuild) { $params.SkipBuild = $true }
    & $ladder @params
    $code = $LASTEXITCODE
    $script:skipBuild = $true
    $latest = Get-ChildItem (Join-Path $root "logs\load\capacity_73") -Directory |
        Where-Object { $_.Name -like "*_${Scenario}_${Count}n*" } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    [pscustomobject]@{
        name = $Name
        suite = ($Name -split "_")[0]
        scenario = $Scenario
        count = $Count
        duration = $Duration
        exit = $code
        dir = if ($latest) { $latest.Name } else { $null }
        path = if ($latest) { $latest.FullName } else { $null }
    }
}

function Read-73Cell {
    param($r)
    $row = [ordered]@{
        name = $r.name
        suite = $r.suite
        scenario = $r.scenario
        count_requested = $r.count
        duration = $r.duration
        exit = $r.exit
        dir = $r.dir
    }
    if (-not $r.path) { return [pscustomobject]$row }

    $meta = Join-Path $r.path "ladder_meta.txt"
    if (Test-Path $meta) {
        Get-Content $meta | ForEach-Object {
            if ($_ -match "^(\w+)=(.*)$") { $row["meta_$($Matches[1])"] = $Matches[2] }
        }
    }

    $ramp = Join-Path $r.path "connection_ramp.json"
    if (Test-Path $ramp) {
        $j = Get-Content $ramp -Raw | ConvertFrom-Json
        $row.spawn_requested = $j.requested_clients
        $row.spawn_issued = $j.spawn_issued_total
        $row.transport_connected = $j.transport_established_total
        $row.welcome_ok = $j.welcome_total
        $row.world_entered = $j.world_entered_total
        $row.peak_active = $j.peak_active_clients
        $row.attainment_pct = $j.attainment_pct
        $row.funnel_invariant_ok = $j.funnel_invariant_ok
        $row.controller_tick_p99_ms = $j.controller_tick_p99_ms
        $row.tick_all_bots_p99_ms = $j.tick_all_bots_p99_ms
        $row.admission_refused = $j.admission_refused
        $row.funnel_note = $j.funnel_invariant_note
        $row.spawn_catchup_issued_total = $j.spawn_catchup_issued_total
    }

    $td = Join-Path $r.path "tick_domains.json"
    if (Test-Path $td) {
        $j = Get-Content $td -Raw | ConvertFrom-Json
        $row.tick_p50 = $j.tick_total.p50_ms
        $row.tick_p95 = $j.tick_total.p95_ms
        $row.tick_p99 = $j.tick_total.p99_ms
        $row.tick_max = $j.tick_total.max_ms
        $row.tick_util = $j.tick_utilization_pct
        $row.overruns = $j.tick_overrun_count
        $row.dominant = $j.dominant_owner
        $row.unattributed_mean = $j.unattributed.mean_ms
        if ($null -ne $j.tick_total.mean_ms -and $j.tick_total.mean_ms -gt 0 -and $null -ne $j.unattributed.mean_ms) {
            $row.unattributed_share_pct = [math]::Round(100.0 * $j.unattributed.mean_ms / $j.tick_total.mean_ms, 2)
        }
        foreach ($owner in @(
            "npc_activity", "simulation_movement", "gameplay_services", "spatial_aoi",
            "replication", "scheduler", "actions", "effects", "entity_lifecycle", "cadence",
            "replication_discover", "replication_policy", "replication_encode",
            "replication_enqueue", "persistence_enqueue", "commands_input"
        )) {
            if ($null -ne $j.$owner) {
                $row["${owner}_mean"] = $j.$owner.mean_ms
                $row["${owner}_p99"] = $j.$owner.p99_ms
            }
        }
    }

    $live = Join-Path $r.path "capacity_live.json"
    if (Test-Path $live) {
        $lj = Get-Content $live -Raw | ConvertFrom-Json
        $row.saturation_class = $lj.saturation_class
        $row.saturation_note = $lj.saturation_note
    }

    $wl = Join-Path $r.path "gameplay_workload.json"
    if (Test-Path $wl) {
        $wj = Get-Content $wl -Raw | ConvertFrom-Json
        $row.npcs_active = $wj.npcs_active
        $row.npc_updates_total = $wj.npc_updates_total
        $row.actions_attempted_total = $wj.actions_attempted_total
        $row.actions_started_total = $wj.actions_started_total
        $row.actions_rejected_total = $wj.actions_rejected_total
        $row.health_mutations_total = $wj.health_mutations_total
        $row.deaths_total = $wj.deaths_total
        $row.respawns_total = $wj.respawns_total
        $row.pulse_ticks_total = $wj.pulse_ticks_total
        $row.effects_active = $wj.effects_active
        $row.effects_applied_total = $wj.effects_applied_total
        $row.scheduler_queued = $wj.scheduler_queued
        $row.actions_active = $wj.actions_active
    }

    $pr = Join-Path $r.path "process_resources.json"
    if (Test-Path $pr) {
        $pj = Get-Content $pr -Raw | ConvertFrom-Json
        if ($null -ne $pj.cpu_utilization_pct) { $row.server_cpu_pct = $pj.cpu_utilization_pct }
        if ($null -ne $pj.working_set_bytes) { $row.server_rss_mb = [math]::Round($pj.working_set_bytes / 1MB, 2) }
        if ($null -ne $pj.working_set_peak_bytes) { $row.server_rss_peak_mb = [math]::Round($pj.working_set_peak_bytes / 1MB, 2) }
    }

    $hr = Join-Path $r.path "harness_resources.json"
    if (Test-Path $hr) {
        $hj = Get-Content $hr -Raw | ConvertFrom-Json
        if ($null -ne $hj.cpu_utilization_pct) { $row.harness_cpu_pct = $hj.cpu_utilization_pct }
        if ($null -ne $hj.working_set_bytes) { $row.harness_rss_mb = [math]::Round($hj.working_set_bytes / 1MB, 2) }
    }

    $np = Join-Path $r.path "network_pressure.json"
    if (Test-Path $np) {
        $nj = Get-Content $np -Raw | ConvertFrom-Json
        foreach ($k in @("bytes_out_total","outbound_bytes_total","bytes_written_total")) {
            if ($null -ne $nj.$k) { $row.net_bytes_out_total = $nj.$k; break }
        }
        foreach ($k in @("bytes_in_total","inbound_bytes_total","bytes_read_total")) {
            if ($null -ne $nj.$k) { $row.net_bytes_in_total = $nj.$k; break }
        }
        foreach ($k in @("queue_depth_peak","outbound_queue_peak","enqueue_depth_peak")) {
            if ($null -ne $nj.$k) { $row.net_queue_peak = $nj.$k; break }
        }
        foreach ($k in @("write_drain_p99_ms","drain_latency_p99_ms")) {
            if ($null -ne $nj.$k) { $row.net_write_drain_p99_ms = $nj.$k; break }
        }
    }

    $rf = Join-Path $r.path "replication_fanout.json"
    if (Test-Path $rf) {
        $rj = Get-Content $rf -Raw | ConvertFrom-Json
        foreach ($pair in @(
            @{dst="repl_eligible"; src=@("eligible_total","eligible")},
            @{dst="repl_emitted"; src=@("emitted_total","emitted")},
            @{dst="repl_coalesced"; src=@("coalesced_total","coalesced")},
            @{dst="repl_suppressed"; src=@("suppressed_total","suppressed")},
            @{dst="repl_bytes"; src=@("bytes_total","bytes")},
            @{dst="repl_fanout_p99"; src=@("fanout_p99","fanout_p99_ms")},
            @{dst="repl_recovery"; src=@("recovery_total","stale_recovery_total")},
            @{dst="repl_budget_deferred"; src=@("budget_deferred_total","priority_deferred_total")}
        )) {
            foreach ($s in $pair.src) {
                if ($null -ne $rj.$s) { $row[$pair.dst] = $rj.$s; break }
            }
        }
    }

    $aoi = Join-Path $r.path "aoi_locality.json"
    if (Test-Path $aoi) {
        $aj = Get-Content $aoi -Raw | ConvertFrom-Json
        if ($null -ne $aj.enter_total) { $row.aoi_enter_total = $aj.enter_total }
        if ($null -ne $aj.leave_total) { $row.aoi_leave_total = $aj.leave_total }
        if ($null -ne $aj.enters_total) { $row.aoi_enter_total = $aj.enters_total }
        if ($null -ne $aj.leaves_total) { $row.aoi_leave_total = $aj.leaves_total }
    }

    $cl = Join-Path $r.path "connection_lifecycle.json"
    if (Test-Path $cl) {
        $cj = Get-Content $cl -Raw | ConvertFrom-Json
        if ($null -ne $cj.welcome_ok_total) { $row.server_welcome_ok = $cj.welcome_ok_total }
        if ($null -ne $cj.reconnect_ok_total) { $row.server_reconnect_ok = $cj.reconnect_ok_total }
        if ($null -ne $cj.disconnect_total) { $row.server_disconnect_total = $cj.disconnect_total }
    }

    $rs = Join-Path $r.path "run_summary.json"
    if (Test-Path $rs) {
        $sj = Get-Content $rs -Raw | ConvertFrom-Json
        if ($null -ne $sj.snapshot_starvation_samples) { $row.snapshot_starvation = $sj.snapshot_starvation_samples }
        if ($null -ne $sj.harness_snapshot_starvation_samples) { $row.snapshot_starvation = $sj.harness_snapshot_starvation_samples }
        if ($null -ne $sj.failed_reason) { $row.harness_failed_reason = $sj.failed_reason }
        if ($null -ne $sj.outcome) { $row.harness_outcome = $sj.outcome }
    }

    $starve = 0
    if ($null -ne $row.snapshot_starvation) { $starve = [int64]$row.snapshot_starvation }
    $row.harness_snapshot_starvation = $starve
    $row.harness_caveat = ($starve -gt 0) -or ($r.exit -ne 0 -and "$($row.harness_failed_reason)" -match "snapshot_starvation")

    return [pscustomobject]$row
}

$results = @()

if ($want["lifecycle"]) {
    $results += Invoke-73Cell -Name "lifecycle_mixed8" -Scenario "representative-mixed" -Count 8 `
        -Duration $DurationLifecycle -MaxBots 32 -CellHealthMax $HealthMax -CellRespawnDelay $RespawnDelayTicks
}

if ($want["player"]) {
    foreach ($n in @(8, 16, 32, 64, 128)) {
        $ramp = if ($n -ge 64) { 25 } else { 50 }
        $results += Invoke-73Cell -Name "player_$n" -Scenario "representative-mixed" -Count $n `
            -Duration $DurationCell -RampMs $ramp -MaxBots ([Math]::Max(256, $n))
    }
}

if ($want["npc"]) {
    foreach ($n in @(8, 24, 48, 64, 96)) {
        $results += Invoke-73Cell -Name "npc_$n" -Scenario "representative-mixed" -Count 8 `
            -Duration $DurationCell -MaxBots 32 -NpcCount $n
    }
}

if ($want["activity"]) {
    $pairs = @(
        @{ a = 40; p = 60 },
        @{ a = 20; p = 30 },
        @{ a = 8; p = 12 },
        @{ a = 4; p = 6 }
    )
    foreach ($pair in $pairs) {
        $results += Invoke-73Cell -Name "activity_a$($pair.a)_p$($pair.p)" -Scenario "representative-mixed" -Count 8 `
            -Duration $DurationCell -MaxBots 32 -NpcCount 24 `
            -ActionPeriodTicks $pair.a -PulsePeriodTicks $pair.p
    }
}

if ($want["dense"]) {
    foreach ($n in @(8, 16, 32, 64)) {
        $ramp = if ($n -ge 64) { 25 } else { 50 }
        $results += Invoke-73Cell -Name "dense_$n" -Scenario "representative-dense" -Count $n `
            -Duration $DurationCell -RampMs $ramp -MaxBots ([Math]::Max(256, $n))
    }
}

if ($want["soak"]) {
    # Stable point: mixed@16 below first pressure; extend only if prior cells look healthy.
    $results += Invoke-73Cell -Name "soak_mixed16" -Scenario "representative-mixed" -Count 16 `
        -Duration $DurationSoak -MaxBots 64
}

Remove-Item Env:PURGATORY_REPLICATION_POLICY -ErrorAction SilentlyContinue
Remove-Item Env:PURGATORY_POPULATION_CLASS -ErrorAction SilentlyContinue

$summary = @()
foreach ($r in $results) {
    $summary += Read-73Cell $r
}

$summaryPath = Join-Path $summaryDir "phase73_summary.json"
$summary | ConvertTo-Json -Depth 8 | Set-Content $summaryPath
$summary | Select-Object name, exit, count_requested, spawn_issued, peak_active, attainment_pct, funnel_invariant_ok, `
    tick_p99, tick_util, dominant, deaths_total, respawns_total, harness_caveat, saturation_class |
    Format-Table -AutoSize | Out-String | Write-Host

Write-Host "Summary: $summaryPath"

# Lifecycle proof hard gate when that suite ran.
$life = @($summary | Where-Object { $_.name -eq "lifecycle_mixed8" })
if ($life.Count -gt 0) {
    $l = $life[0]
    $ok = ($l.deaths_total -gt 0) -and ($l.respawns_total -gt 0)
    if (-not $ok) {
        Write-Host "LIFECYCLE PROOF FAILED: deaths=$($l.deaths_total) respawns=$($l.respawns_total)"
        exit 2
    }
    Write-Host "LIFECYCLE PROOF PASS: deaths=$($l.deaths_total) respawns=$($l.respawns_total)"
}

exit 0
