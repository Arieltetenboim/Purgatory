# Phase 7.1 instrumentation validation (measure only — no redesign).
#
# Mixed 8 overhead (detail 0 vs 1), Mixed 8 baseline, hotspot 128 / 256 vs 6G.7C,
# optional 384 admission-wall diagnostic (admission stays 256).
param(
    [switch]$SkipBuild,
    [switch]$SkipAdmissionWall
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$summaryDir = Join-Path $root "logs\load\capacity_71\summary_$stamp"
New-Item -ItemType Directory -Force -Path $summaryDir | Out-Null

$env:PURGATORY_REPLICATION_POLICY = "selective"
$env:PURGATORY_POPULATION_CLASS = "high"

$ladder = Join-Path $root "scripts\capacity_ladder.ps1"
$script:skipBuild = [bool]$SkipBuild

function Invoke-71Run {
    param(
        [string]$Name,
        [string]$Scenario,
        [int]$Count,
        [string]$Duration,
        [string]$Detail,
        [int]$RampMs = 50,
        [int]$MaxBots = 256
    )
    Write-Host "==== $Name ===="
    $params = @{
        Scenario = $Scenario
        Count = $Count
        Duration = $Duration
        Seed = 4242
        RampMs = $RampMs
        ArtifactRoot = "capacity_71"
        CapacityDetail = $Detail
        MaxBots = $MaxBots
    }
    if ($script:skipBuild) { $params.SkipBuild = $true }
    & $ladder @params
    $code = $LASTEXITCODE
    $script:skipBuild = $true
    if ($code -ne 0) {
        Write-Host "FAILED $Name exit=$code"
    }
    $latest = Get-ChildItem (Join-Path $root "logs\load\capacity_71") -Directory |
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
$results += Invoke-71Run -Name "overhead_detail0_mixed8" -Scenario "mixed-stall" -Count 8 -Duration "20s" -Detail "0"
$results += Invoke-71Run -Name "overhead_detail1_mixed8" -Scenario "mixed-stall" -Count 8 -Duration "20s" -Detail "1"
$results += Invoke-71Run -Name "baseline_mixed8" -Scenario "mixed-stall" -Count 8 -Duration "20s" -Detail "1"
$results += Invoke-71Run -Name "mid_hotspot128" -Scenario "hotspot" -Count 128 -Duration "30s" -Detail "1"
$results += Invoke-71Run -Name "upper_hotspot256" -Scenario "hotspot" -Count 256 -Duration "30s" -Detail "1"
if (-not $SkipAdmissionWall) {
    # Admission-wall diagnostic only: admission stays 256. Duration must be long
    # enough for connect/handshake to reach the cap (~2 sessions/s on this host).
    $results += Invoke-71Run -Name "admission_wall_384" -Scenario "hotspot" -Count 384 -Duration "150s" -Detail "1" -RampMs 10 -MaxBots 384
}

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
        $net = Join-Path $r.path "network_pressure.json"
        $life = Join-Path $r.path "connection_lifecycle.json"
        $hr = Join-Path $r.path "harness_resources.json"
        $hc = Join-Path $r.path "harness_connection.json"
        if (Test-Path $td) {
            $j = Get-Content $td -Raw | ConvertFrom-Json
            $row.tick_p99 = $j.tick_total.p99_ms
            $row.tick_util = $j.tick_utilization_pct
            $row.overruns = $j.tick_overrun_count
            $row.dominant = $j.dominant_owner
            $row.unattributed_mean = $j.unattributed.mean_ms
            $row.accounting_errors = $j.accounting_error_ticks
        }
        if (Test-Path $live) {
            $l = Get-Content $live -Raw | ConvertFrom-Json
            $row.saturation_class = $l.saturation_class
            $row.cpu_raw = $l.cpu_utilization_pct
            $row.cpu_norm = $l.cpu_normalized_per_logical_pct
            $row.connected_sessions = $l.connected_sessions
        }
        $row.has_network = Test-Path $net
        $row.has_lifecycle = Test-Path $life
        $row.has_harness_resources = Test-Path $hr
        $row.has_harness_connection = Test-Path $hc
        if (Test-Path $life) {
            $lf = Get-Content $life -Raw | ConvertFrom-Json
            $row.session_accepted = $lf.session_accepted
        }
    }
    $summary += [pscustomobject]$row
}

$summary | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $summaryDir "phase71_summary.json")
$summary | Format-Table -AutoSize | Out-String | Write-Host
Write-Host "Summary: $summaryDir\phase71_summary.json"
$failed = @($summary | Where-Object { $_.exit -ne 0 }).Count
if ($failed -gt 0) { exit 1 }
exit 0
