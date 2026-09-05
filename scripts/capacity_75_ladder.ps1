# Phase 7.5 — CPU / owner scaling ladders (measure → optimize only proven owners).
#
# Baseline policy: selective / high, default soft budget, cadence x1.
# Order: audit → player → npc → activity → dense → overlap → canonical.
#
# Usage:
#   ./scripts/capacity_75_ladder.ps1
#   ./scripts/capacity_75_ladder.ps1 -SkipBuild
#   ./scripts/capacity_75_ladder.ps1 -Only audit,player
param(
    [switch]$SkipBuild,
    [string]$Only = "",
    [int]$HealthMax = 6,
    [int]$RespawnDelayTicks = 20,
    [string]$DurationCell = "45s",
    [string]$DurationAudit = "45s"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$summaryDir = Join-Path $root "logs\load\capacity_75\summary_$stamp"
New-Item -ItemType Directory -Force -Path $summaryDir | Out-Null

$env:PURGATORY_REPLICATION_POLICY = "selective"
$env:PURGATORY_POPULATION_CLASS = "high"
# Explicit defaults: no 7.4 candidate knobs on baseline CPU cells.
Remove-Item Env:PURGATORY_REPLICATION_FRAME_BUDGET_BYTES -ErrorAction SilentlyContinue
Remove-Item Env:PURGATORY_REPLICATION_STRANGER_CADENCE_MULT -ErrorAction SilentlyContinue

$ladder = Join-Path $root "scripts\capacity_ladder.ps1"
$script:skipBuild = [bool]$SkipBuild
$want = @{}
if ([string]::IsNullOrWhiteSpace($Only)) {
    foreach ($k in @("audit", "player", "npc", "activity", "dense", "overlap", "canonical")) {
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
if ($want.Count -eq 0) {
    Write-Error "No suites selected from -Only '$Only'"
    exit 1
}

function Invoke-75Cell {
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
        [switch]$PostRampThin
    )
    Write-Host ""
    Write-Host "==== $Name ===="
    $params = @{
        Scenario = $Scenario
        Count = $Count
        Duration = $Duration
        Seed = 7505
        RampMs = $RampMs
        ArtifactRoot = "capacity_75"
        CapacityDetail = "1"
        MaxBots = $MaxBots
        HealthMax = $HealthMax
        RespawnDelayTicks = $RespawnDelayTicks
        ReplicationPolicy = "selective"
        PopulationClass = "high"
    }
    if ($NpcCount -ge 0) { $params.NpcCount = $NpcCount }
    if ($ActionPeriodTicks -ge 0) { $params.ActionPeriodTicks = $ActionPeriodTicks }
    if ($PulsePeriodTicks -ge 0) { $params.PulsePeriodTicks = $PulsePeriodTicks }
    if ($ActivePct -ge 0) { $params.ActivePct = $ActivePct }
    if ($HotspotRadius -ge 0) { $params.HotspotRadius = $HotspotRadius }
    if ($ChurnCount -ge 0) { $params.ChurnCount = $ChurnCount }
    if ($PostRampThin) { $params.PostRampThin = $true }
    if ($script:skipBuild) { $params.SkipBuild = $true }
    & $ladder @params
    $code = $LASTEXITCODE
    $script:skipBuild = $true
    $latest = Get-ChildItem (Join-Path $root "logs\load\capacity_75") -Directory |
        Where-Object { $_.Name -like "*_${Scenario}_${Count}n*" -and $_.Name -notlike "summary_*" } |
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

function Read-MidRunCpuPct {
    param([string]$Path)
    $nd = Join-Path $Path "process_resources.ndjson"
    if (-not (Test-Path $nd)) { return $null }
    $vals = @()
    Get-Content $nd | ForEach-Object {
        if ([string]::IsNullOrWhiteSpace($_)) { return }
        try {
            $j = $_ | ConvertFrom-Json
            if ($null -ne $j.cpu_utilization_pct -and [double]$j.cpu_utilization_pct -gt 0) {
                $vals += [double]$j.cpu_utilization_pct
            }
        } catch {}
    }
    if ($vals.Count -eq 0) { return $null }
    $mid = [int][math]::Floor($vals.Count / 2)
    return [math]::Round($vals[$mid], 2)
}

function Read-75Cell {
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
        $row.peak_active = $j.peak_active_clients
        $row.attainment_pct = $j.attainment_pct
        $row.funnel_invariant_ok = $j.funnel_invariant_ok
        $row.controller_tick_p99_ms = $j.controller_tick_p99_ms
        $row.admission_refused = $j.admission_refused
    }

    $td = Join-Path $r.path "tick_domains.json"
    if (Test-Path $td) {
        $j = Get-Content $td -Raw | ConvertFrom-Json
        $row.tick_mean = $j.tick_total.mean_ms
        $row.tick_p50 = $j.tick_total.p50_ms
        $row.tick_p95 = $j.tick_total.p95_ms
        $row.tick_p99 = $j.tick_total.p99_ms
        $row.tick_max = $j.tick_total.max_ms
        $row.tick_util = $j.tick_utilization_pct
        $row.overruns = $j.tick_overrun_count
        $row.dominant = $j.dominant_owner
        $row.worst_spike = $j.worst_spike_owner
        $row.accounting_error_ticks = $j.accounting_error_ticks
        $row.unattributed_mean = $j.unattributed.mean_ms
        $row.unattributed_p50 = $j.unattributed.p50_ms
        $row.unattributed_p95 = $j.unattributed.p95_ms
        $row.unattributed_p99 = $j.unattributed.p99_ms
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
                $row["${owner}_p50"] = $j.$owner.p50_ms
                $row["${owner}_p95"] = $j.$owner.p95_ms
                $row["${owner}_p99"] = $j.$owner.p99_ms
                if ($null -ne $j.tick_total.mean_ms -and $j.tick_total.mean_ms -gt 0) {
                    $row["${owner}_share_pct"] = [math]::Round(100.0 * $j.$owner.mean_ms / $j.tick_total.mean_ms, 2)
                }
            }
        }
        # Parent replication rollup for convenience when detail leaves exist.
        if ($null -ne $j.replication_discover -and $null -ne $j.replication_policy) {
            $replSum = [double]$j.replication_discover.mean_ms +
                [double]$j.replication_policy.mean_ms +
                [double]$j.replication_encode.mean_ms +
                [double]$j.replication_enqueue.mean_ms
            $row.replication_children_mean = [math]::Round($replSum, 4)
            if ($null -ne $j.tick_total.mean_ms -and $j.tick_total.mean_ms -gt 0) {
                $row.replication_children_share_pct = [math]::Round(100.0 * $replSum / $j.tick_total.mean_ms, 2)
            }
        }
    }

    $live = Join-Path $r.path "capacity_live.json"
    if (Test-Path $live) {
        $lj = Get-Content $live -Raw | ConvertFrom-Json
        $row.saturation_class = $lj.saturation_class
    }

    $wl = Join-Path $r.path "gameplay_workload.json"
    if (Test-Path $wl) {
        $wj = Get-Content $wl -Raw | ConvertFrom-Json
        $row.npcs_active = $wj.npcs_active
        $row.npc_updates_total = $wj.npc_updates_total
        $row.actions_started_total = $wj.actions_started_total
        $row.health_mutations_total = $wj.health_mutations_total
        $row.deaths_total = $wj.deaths_total
        $row.respawns_total = $wj.respawns_total
        $row.pulse_ticks_total = $wj.pulse_ticks_total
        $row.effects_applied_total = $wj.effects_applied_total
        $row.scheduler_queued = $wj.scheduler_queued
    }

    $pr = Join-Path $r.path "process_resources.json"
    if (Test-Path $pr) {
        $pj = Get-Content $pr -Raw | ConvertFrom-Json
        if ($null -ne $pj.cpu_utilization_pct) { $row.server_cpu_end_pct = $pj.cpu_utilization_pct }
        if ($null -ne $pj.working_set_bytes) { $row.server_rss_mb = [math]::Round($pj.working_set_bytes / 1MB, 2) }
        if ($null -ne $pj.working_set_peak_bytes) { $row.server_rss_peak_mb = [math]::Round($pj.working_set_peak_bytes / 1MB, 2) }
    }
    $midCpu = Read-MidRunCpuPct $r.path
    if ($null -ne $midCpu) { $row.server_cpu_mid_pct = $midCpu }

    $np = Join-Path $r.path "network_pressure.json"
    if (Test-Path $np) {
        $nj = Get-Content $np -Raw | ConvertFrom-Json
        if ($null -ne $nj.bytes_out_per_sec) { $row.bytes_out_per_sec = $nj.bytes_out_per_sec }
        if ($null -ne $nj.bytes_out_per_session_per_sec) { $row.bytes_out_per_client_s = $nj.bytes_out_per_session_per_sec }
        if ($null -ne $nj.writer_queue_depth_max) { $row.writer_queue_depth_max = $nj.writer_queue_depth_max }
        if ($null -ne $nj.writer_queue_push_fail_total) { $row.writer_queue_push_fail = $nj.writer_queue_push_fail_total }
    }

    $rf = Join-Path $r.path "replication_fanout.json"
    if (Test-Path $rf) {
        $rj = Get-Content $rf -Raw | ConvertFrom-Json
        if ($null -ne $rj.updates_emitted_total) { $row.repl_emitted = $rj.updates_emitted_total }
        if ($null -ne $rj.policy_eligible_total) { $row.repl_eligible = $rj.policy_eligible_total }
        if ($null -ne $rj.bytes_emitted_total) { $row.repl_bytes = $rj.bytes_emitted_total }
        if ($null -ne $rj.interested_observers_total) { $row.repl_interested = $rj.interested_observers_total }
        if ($null -ne $rj.known_relationships_scanned_total) { $row.repl_scanned = $rj.known_relationships_scanned_total }
        if ($null -ne $rj.publish_passes) { $row.repl_publish_passes = $rj.publish_passes }
        if ($null -ne $rj.serialize_attempts_total) { $row.repl_serialize_attempts = $rj.serialize_attempts_total }
    }

    $aoi = Join-Path $r.path "aoi_locality.json"
    if (Test-Path $aoi) {
        $aj = Get-Content $aoi -Raw | ConvertFrom-Json
        if ($null -ne $aj.entities_moved_total) { $row.entities_moved = $aj.entities_moved_total }
        if ($null -ne $aj.observers_dirtied_total) { $row.observers_dirtied = $aj.observers_dirtied_total }
    }

    $rs = Join-Path $r.path "run_summary.json"
    if (Test-Path $rs) {
        $sj = Get-Content $rs -Raw | ConvertFrom-Json
        if ($null -ne $sj.snapshot_starvation_samples) { $row.snapshot_starvation = $sj.snapshot_starvation_samples }
        if ($null -ne $sj.harness_snapshot_starvation_samples) { $row.snapshot_starvation = $sj.harness_snapshot_starvation_samples }
        if ($null -ne $sj.failed_reason) { $row.harness_failed_reason = $sj.failed_reason }
    }

    # Normalized coefficients (configuration-specific).
    $tickMean = if ($null -ne $row.tick_mean) { [double]$row.tick_mean } else { 0.0 }
    $wallGuess = 45.0
    if ($r.duration -match "^(\d+)s$") { $wallGuess = [double]$Matches[1] }
    elseif ($r.duration -match "^(\d+)m$") { $wallGuess = [double]$Matches[1] * 60.0 }
    $ticksApprox = 30.0 * $wallGuess
    if ($ticksApprox -gt 0 -and $null -ne $row.npc_updates_total -and $row.npc_updates_total -gt 0 -and $null -ne $row.npc_activity_mean) {
        $updPerTick = [double]$row.npc_updates_total / $ticksApprox
        if ($updPerTick -gt 0) {
            $row.npc_us_per_1000_updates = [math]::Round(1000.0 * ([double]$row.npc_activity_mean * 1000.0) / $updPerTick, 2)
        }
    }
    if ($null -ne $row.peak_active -and $row.peak_active -gt 0 -and $null -ne $row.replication_children_mean) {
        $row.repl_us_per_observer = [math]::Round(1000.0 * [double]$row.replication_children_mean / [double]$row.peak_active, 2)
    }
    if ($ticksApprox -gt 0 -and $null -ne $row.repl_emitted -and $row.repl_emitted -gt 0 -and $null -ne $row.replication_children_mean) {
        $emPerTick = [double]$row.repl_emitted / $ticksApprox
        if ($emPerTick -gt 0) {
            $row.repl_us_per_1000_emitted = [math]::Round(1000.0 * ([double]$row.replication_children_mean * 1000.0) / $emPerTick, 2)
        }
    }
    if ($ticksApprox -gt 0 -and $null -ne $row.entities_moved -and $row.entities_moved -gt 0 -and $null -ne $row.spatial_aoi_mean) {
        $movedPerTick = [double]$row.entities_moved / $ticksApprox
        if ($movedPerTick -gt 0) {
            $row.aoi_us_per_moved = [math]::Round(1000.0 * [double]$row.spatial_aoi_mean / $movedPerTick, 2)
        }
    }
    if ($null -ne $row.peak_active -and $row.peak_active -gt 0 -and $null -ne $row.server_cpu_mid_pct) {
        $row.cpu_pct_per_player = [math]::Round([double]$row.server_cpu_mid_pct / [double]$row.peak_active, 3)
    }
    if ($null -ne $row.npcs_active -and $row.npcs_active -gt 0 -and $null -ne $row.server_cpu_mid_pct) {
        $row.cpu_pct_per_npc = [math]::Round([double]$row.server_cpu_mid_pct / [double]$row.npcs_active, 3)
    }
    if ($null -ne $row.peak_active -and $null -ne $row.npcs_active -and $null -ne $row.server_rss_mb) {
        $ents = [double]$row.peak_active + [double]$row.npcs_active
        if ($ents -gt 0) {
            $row.rss_mb_per_entity = [math]::Round([double]$row.server_rss_mb / $ents, 4)
        }
    }

    return [pscustomobject]$row
}

$results = @()

if ($want["audit"]) {
    foreach ($n in @(64, 128)) {
        $ramp = if ($n -ge 64) { 25 } else { 50 }
        $results += Invoke-75Cell -Name "audit_mixed$n" -Scenario "representative-mixed" -Count $n `
            -Duration $DurationAudit -RampMs $ramp -MaxBots ([Math]::Max(256, $n)) -PostRampThin
    }
}

if ($want["player"]) {
    foreach ($n in @(8, 16, 32, 64, 128)) {
        $ramp = if ($n -ge 64) { 25 } else { 50 }
        $thin = $n -ge 32
        $results += Invoke-75Cell -Name "player_$n" -Scenario "representative-mixed" -Count $n `
            -Duration $DurationCell -RampMs $ramp -MaxBots ([Math]::Max(256, $n)) -PostRampThin:$thin
    }
}

if ($want["npc"]) {
    foreach ($n in @(8, 24, 48, 64, 96)) {
        $results += Invoke-75Cell -Name "npc_$n" -Scenario "representative-mixed" -Count 8 `
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
        $results += Invoke-75Cell -Name "activity_a$($pair.a)_p$($pair.p)" -Scenario "representative-mixed" -Count 8 `
            -Duration $DurationCell -MaxBots 32 -NpcCount 24 `
            -ActionPeriodTicks $pair.a -PulsePeriodTicks $pair.p
    }
}

if ($want["dense"]) {
    foreach ($n in @(8, 16, 32, 64)) {
        $ramp = if ($n -ge 64) { 25 } else { 50 }
        $thin = $n -ge 32
        $results += Invoke-75Cell -Name "dense_$n" -Scenario "representative-dense" -Count $n `
            -Duration $DurationCell -RampMs $ramp -MaxBots ([Math]::Max(256, $n)) -PostRampThin:$thin
    }
}

if ($want["overlap"]) {
    # Hold players+NPC, vary hotspot radius (AOI overlap axis).
    foreach ($radius in @(4, 8, 16)) {
        $results += Invoke-75Cell -Name "overlap_r$radius" -Scenario "representative-mixed" -Count 32 `
            -Duration $DurationCell -RampMs 50 -MaxBots 256 -NpcCount 24 `
            -HotspotRadius $radius -PostRampThin
    }
}

if ($want["canonical"]) {
    foreach ($pair in @(
        @{ name = "canonical_mixed64"; scen = "representative-mixed"; n = 64 },
        @{ name = "canonical_mixed128"; scen = "representative-mixed"; n = 128 },
        @{ name = "canonical_dense32"; scen = "representative-dense"; n = 32 },
        @{ name = "canonical_dense64"; scen = "representative-dense"; n = 64 }
    )) {
        $ramp = if ($pair.n -ge 64) { 25 } else { 50 }
        $results += Invoke-75Cell -Name $pair.name -Scenario $pair.scen -Count $pair.n `
            -Duration $DurationCell -RampMs $ramp -MaxBots ([Math]::Max(256, $pair.n)) -PostRampThin
    }
}

Remove-Item Env:PURGATORY_REPLICATION_POLICY -ErrorAction SilentlyContinue
Remove-Item Env:PURGATORY_POPULATION_CLASS -ErrorAction SilentlyContinue

$summary = @()
foreach ($r in $results) {
    $summary += Read-75Cell $r
}

$summaryPath = Join-Path $summaryDir "phase75_summary.json"
$summary | ConvertTo-Json -Depth 8 | Set-Content $summaryPath
$summary | Select-Object name, exit, count_requested, spawn_issued, peak_active, attainment_pct, `
    tick_p99, tick_util, dominant, unattributed_share_pct, replication_children_mean, `
    npc_activity_mean, server_cpu_mid_pct, server_rss_mb, harness_failed_reason |
    Format-Table -AutoSize | Out-String | Write-Host

Write-Host "Summary: $summaryPath"
exit 0
