# Phase 7.6 - targeted falsification cells (predict first, then measure).
#
# Reads falsify_predictions.json from the latest model_*/ dir (or -ModelDir).
# Runs four cells not on the 7.5 ladder, then compares prediction vs measurement.
#
# Usage:
#   ./scripts/capacity_76_falsify.ps1
#   ./scripts/capacity_76_falsify.ps1 -SkipBuild -ModelDir logs/load/capacity_76/model_...
param(
    [switch]$SkipBuild,
    [string]$ModelDir = "",
    [string]$DurationCell = "45s",
    [int]$HealthMax = 6,
    [int]$RespawnDelayTicks = 20
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if ([string]::IsNullOrWhiteSpace($ModelDir)) {
    $ModelDir = Get-ChildItem (Join-Path $root "logs\load\capacity_76") -Directory |
        Where-Object { $_.Name -like "model_*" } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $ModelDir -or -not (Test-Path $ModelDir)) {
    Write-Error "ModelDir not found. Run capacity_76_model.ps1 first."
    exit 1
}
$predPath = Join-Path $ModelDir "falsify_predictions.json"
if (-not (Test-Path $predPath)) {
    Write-Error "Missing $predPath"
    exit 1
}
$preds = Get-Content $predPath -Raw | ConvertFrom-Json
Write-Host "Using predictions from: $predPath"

$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$summaryDir = Join-Path $root "logs\load\capacity_76\falsify_$stamp"
New-Item -ItemType Directory -Force -Path $summaryDir | Out-Null

$env:PURGATORY_REPLICATION_POLICY = "selective"
$env:PURGATORY_POPULATION_CLASS = "high"
Remove-Item Env:PURGATORY_REPLICATION_FRAME_BUDGET_BYTES -ErrorAction SilentlyContinue
Remove-Item Env:PURGATORY_REPLICATION_STRANGER_CADENCE_MULT -ErrorAction SilentlyContinue

$ladder = Join-Path $root "scripts\capacity_ladder.ps1"
$script:skipBuild = [bool]$SkipBuild

function Invoke-76Cell {
    param(
        [string]$Name,
        [string]$Scenario,
        [int]$Count,
        [int]$NpcCount = -1,
        [int]$ActionPeriodTicks = -1,
        [int]$PulsePeriodTicks = -1
    )
    Write-Host ""
    Write-Host "==== $Name ===="
    $ramp = if ($Count -ge 64) { 25 } else { 50 }
    $thin = $Count -ge 32
    $params = @{
        Scenario = $Scenario
        Count = $Count
        Duration = $DurationCell
        Seed = 7606
        RampMs = $ramp
        ArtifactRoot = "capacity_76"
        CapacityDetail = "1"
        MaxBots = [Math]::Max(256, $Count)
        HealthMax = $HealthMax
        RespawnDelayTicks = $RespawnDelayTicks
        ReplicationPolicy = "selective"
        PopulationClass = "high"
    }
    if ($NpcCount -ge 0) { $params.NpcCount = $NpcCount }
    if ($ActionPeriodTicks -ge 0) { $params.ActionPeriodTicks = $ActionPeriodTicks }
    if ($PulsePeriodTicks -ge 0) { $params.PulsePeriodTicks = $PulsePeriodTicks }
    if ($thin) { $params.PostRampThin = $true }
    if ($script:skipBuild) { $params.SkipBuild = $true }
    & $ladder @params
    $code = $LASTEXITCODE
    $script:skipBuild = $true
    $latest = Get-ChildItem (Join-Path $root "logs\load\capacity_76") -Directory |
        Where-Object { $_.Name -like "*_${Scenario}_${Count}n*" -and $_.Name -notlike "model_*" -and $_.Name -notlike "falsify_*" -and $_.Name -notlike "summary_*" } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    [pscustomobject]@{
        name = $Name
        scenario = $Scenario
        count = $Count
        exit = $code
        dir = if ($latest) { $latest.Name } else { $null }
        path = if ($latest) { $latest.FullName } else { $null }
    }
}

function Read-76Measured([string]$Path) {
    $row = [ordered]@{}
    if (-not $Path) { return [pscustomobject]$row }
    $td = Join-Path $Path "tick_domains.json"
    if (Test-Path $td) {
        $j = Get-Content $td -Raw | ConvertFrom-Json
        $row.tick_mean = $j.tick_total.mean_ms
        $row.tick_p99 = $j.tick_total.p99_ms
        $row.tick_util = $j.tick_utilization_pct
        $row.dominant = $j.dominant_owner
        $row.policy_mean = $j.replication_policy.mean_ms
        $row.npc_mean = $j.npc_activity.mean_ms
        $row.unattr_mean = $j.unattributed.mean_ms
        if ($null -ne $j.tick_total.mean_ms -and $j.tick_total.mean_ms -gt 0) {
            $row.unattr_share_pct = [math]::Round(100.0 * $j.unattributed.mean_ms / $j.tick_total.mean_ms, 2)
        }
        $row.repl_children = [math]::Round(
            [double]$j.replication_discover.mean_ms +
            [double]$j.replication_policy.mean_ms +
            [double]$j.replication_encode.mean_ms +
            [double]$j.replication_enqueue.mean_ms, 4)
    }
    $ramp = Join-Path $Path "connection_ramp.json"
    if (Test-Path $ramp) {
        $rj = Get-Content $ramp -Raw | ConvertFrom-Json
        $row.peak_active = $rj.peak_active_clients
        $row.spawn_issued = $rj.spawn_issued_total
    }
    $wl = Join-Path $Path "gameplay_workload.json"
    if (Test-Path $wl) {
        $wj = Get-Content $wl -Raw | ConvertFrom-Json
        $row.npcs_active = $wj.npcs_active
        $row.npc_updates_total = $wj.npc_updates_total
    }
    $pr = Join-Path $Path "process_resources.json"
    if (Test-Path $pr) {
        $pj = Get-Content $pr -Raw | ConvertFrom-Json
        if ($null -ne $pj.working_set_bytes) { $row.server_rss_mb = [math]::Round($pj.working_set_bytes / 1MB, 2) }
    }
    $np = Join-Path $Path "network_pressure.json"
    if (Test-Path $np) {
        $nj = Get-Content $np -Raw | ConvertFrom-Json
        if ($null -ne $nj.bytes_out_per_sec) { $row.bytes_out_per_sec = $nj.bytes_out_per_sec }
        if ($null -ne $nj.writer_queue_depth_max) { $row.writer_queue_depth_max = $nj.writer_queue_depth_max }
    }
    return [pscustomobject]$row
}

# Cell map matching plan
$cells = @(
    @{ name = "falsify_mixed48"; Scenario = "representative-mixed"; Count = 48; Npc = -1; A = -1; P = -1 },
    @{ name = "falsify_mixed16_npc96"; Scenario = "representative-mixed"; Count = 16; Npc = 96; A = -1; P = -1 },
    @{ name = "falsify_mixed32_a4p6"; Scenario = "representative-mixed"; Count = 32; Npc = 24; A = 4; P = 6 },
    @{ name = "falsify_dense48"; Scenario = "representative-dense"; Count = 48; Npc = -1; A = -1; P = -1 }
)

$results = @()
foreach ($c in $cells) {
    $results += Invoke-76Cell -Name $c.name -Scenario $c.Scenario -Count $c.Count `
        -NpcCount $c.Npc -ActionPeriodTicks $c.A -PulsePeriodTicks $c.P
}

$compare = @()
foreach ($r in $results) {
    $pred = $preds | Where-Object { $_.name -eq $r.name } | Select-Object -First 1
    $meas = Read-76Measured $r.path
    $errTick = $null
    $errPol = $null
    $errNpc = $null
    if ($pred -and $meas.tick_mean -gt 0) {
        $errTick = [math]::Round(100.0 * ($pred.predicted_tick_mean - $meas.tick_mean) / $meas.tick_mean, 2)
    }
    if ($pred -and $meas.policy_mean -gt 0) {
        $errPol = [math]::Round(100.0 * ($pred.predicted_policy - $meas.policy_mean) / $meas.policy_mean, 2)
    }
    if ($pred -and $meas.npc_mean -gt 0) {
        $errNpc = [math]::Round(100.0 * ($pred.predicted_npc - $meas.npc_mean) / $meas.npc_mean, 2)
    }
    $compare += [pscustomobject]@{
        name = $r.name
        exit = $r.exit
        dir = $r.dir
        peak_active = $meas.peak_active
        npcs_active = $meas.npcs_active
        predicted_tick_mean = if ($pred) { $pred.predicted_tick_mean } else { $null }
        measured_tick_mean = $meas.tick_mean
        err_tick_pct = $errTick
        predicted_policy = if ($pred) { $pred.predicted_policy } else { $null }
        measured_policy = $meas.policy_mean
        err_policy_pct = $errPol
        predicted_npc = if ($pred) { $pred.predicted_npc } else { $null }
        measured_npc = $meas.npc_mean
        err_npc_pct = $errNpc
        predicted_dominant = if ($pred) { $pred.predicted_dominant } else { $null }
        measured_dominant = $meas.dominant
        measured_tick_p99 = $meas.tick_p99
        measured_util = $meas.tick_util
        server_rss_mb = $meas.server_rss_mb
        bytes_out_per_sec = $meas.bytes_out_per_sec
        writer_queue_depth_max = $meas.writer_queue_depth_max
    }
}

Remove-Item Env:PURGATORY_REPLICATION_POLICY -ErrorAction SilentlyContinue
Remove-Item Env:PURGATORY_POPULATION_CLASS -ErrorAction SilentlyContinue

$outPath = Join-Path $summaryDir "falsify_compare.json"
$compare | ConvertTo-Json -Depth 6 | Set-Content $outPath
Copy-Item $predPath (Join-Path $summaryDir "falsify_predictions.json")
$compare | Select-Object name, exit, predicted_tick_mean, measured_tick_mean, err_tick_pct, predicted_policy, measured_policy, err_policy_pct, predicted_npc, measured_npc, err_npc_pct, predicted_dominant, measured_dominant |
    Format-Table -AutoSize | Out-String | Write-Host
Write-Host "Falsify compare: $outPath"
Write-Host "ModelDir: $ModelDir"
exit 0
