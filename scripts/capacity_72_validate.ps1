# Phase 7.2 workload validation (measure only — no optimization / no 7.3 ladder).
#
# Small functional (light@4), mixed representative (mixed@8), dense smoke (dense@8 short).
param(
    [switch]$SkipBuild
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$summaryDir = Join-Path $root "logs\load\capacity_72\summary_$stamp"
New-Item -ItemType Directory -Force -Path $summaryDir | Out-Null

$env:PURGATORY_REPLICATION_POLICY = "selective"
$env:PURGATORY_POPULATION_CLASS = "high"

$ladder = Join-Path $root "scripts\capacity_ladder.ps1"
$script:skipBuild = [bool]$SkipBuild

function Invoke-72Run {
    param(
        [string]$Name,
        [string]$Scenario,
        [int]$Count,
        [string]$Duration,
        [string]$Detail = "1"
    )
    Write-Host "==== $Name ===="
    $params = @{
        Scenario = $Scenario
        Count = $Count
        Duration = $Duration
        Seed = 7202
        RampMs = 50
        ArtifactRoot = "capacity_72"
        CapacityDetail = $Detail
        MaxBots = 32
    }
    if ($script:skipBuild) { $params.SkipBuild = $true }
    & $ladder @params
    $code = $LASTEXITCODE
    $script:skipBuild = $true
    if ($code -ne 0) {
        Write-Host "FAILED $Name exit=$code"
    }
    $latest = Get-ChildItem (Join-Path $root "logs\load\capacity_72") -Directory |
        Where-Object { $_.Name -like "*_${Scenario}_${Count}n" } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    [pscustomobject]@{
        name = $Name
        scenario = $Scenario
        count = $Count
        detail = $Detail
        exit = $code
        dir = if ($latest) { $latest.Name } else { $null }
        path = if ($latest) { $latest.FullName } else { $null }
    }
}

$results = @()
$results += Invoke-72Run -Name "small_functional_light4" -Scenario "representative-light" -Count 4 -Duration "20s"
$results += Invoke-72Run -Name "mixed_representative_8" -Scenario "representative-mixed" -Count 8 -Duration "30s"
$results += Invoke-72Run -Name "dense_smoke_8" -Scenario "representative-dense" -Count 8 -Duration "15s"

Remove-Item Env:PURGATORY_REPLICATION_POLICY -ErrorAction SilentlyContinue
Remove-Item Env:PURGATORY_POPULATION_CLASS -ErrorAction SilentlyContinue

$summary = @()
foreach ($r in $results) {
    $row = [ordered]@{
        name = $r.name
        exit = $r.exit
        dir = $r.dir
        detail = $r.detail
        count = $r.count
    }
    if ($r.path) {
        $td = Join-Path $r.path "tick_domains.json"
        $live = Join-Path $r.path "capacity_live.json"
        if (Test-Path $td) {
            $j = Get-Content $td -Raw | ConvertFrom-Json
            $row.tick_p99 = $j.tick_total.p99_ms
            $row.tick_util = $j.tick_utilization_pct
            $row.overruns = $j.tick_overrun_count
            $row.dominant = $j.dominant_owner
            $row.unattributed_mean = $j.unattributed.mean_ms
            if ($null -ne $j.npc_activity) {
                $row.npc_activity_mean = $j.npc_activity.mean_ms
            }
        }
        if (Test-Path $live) {
            $lj = Get-Content $live -Raw | ConvertFrom-Json
            $row.saturation_class = $lj.saturation_class
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
        }
    }
    $summary += [pscustomobject]$row
}

$summary | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $summaryDir "phase72_summary.json")
$summary | Format-Table -AutoSize | Out-String | Write-Host
Write-Host "Summary: $summaryDir"
$failed = @($summary | Where-Object { $_.exit -ne 0 }).Count
if ($failed -gt 0) { exit 1 }
exit 0
