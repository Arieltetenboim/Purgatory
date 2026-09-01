# 6G.7B capacity ladder batch (measure only).
param(
    [switch]$SkipBuild
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$summaryDir = Join-Path $root "logs\load\capacity_6g7b\summary_$stamp"
New-Item -ItemType Directory -Force -Path $summaryDir | Out-Null

$cells = @(
    @{ Scenario = "idle"; Count = 64; Duration = "20s" },
    @{ Scenario = "idle"; Count = 128; Duration = "20s" },
    @{ Scenario = "idle"; Count = 256; Duration = "20s" },
    @{ Scenario = "distributed"; Count = 64; Duration = "30s" },
    @{ Scenario = "distributed"; Count = 128; Duration = "30s" },
    @{ Scenario = "distributed"; Count = 256; Duration = "30s" },
    @{ Scenario = "hotspot"; Count = 64; Duration = "30s" },
    @{ Scenario = "hotspot"; Count = 128; Duration = "30s" },
    @{ Scenario = "hotspot"; Count = 256; Duration = "30s" }
)

$results = @()
foreach ($c in $cells) {
    Write-Host "==== $($c.Scenario)@$($c.Count) ===="
    if ($SkipBuild) {
        & "$root\scripts\capacity_ladder.ps1" -Scenario $c.Scenario -Count $c.Count -Duration $c.Duration -Seed 4242 -RampMs 50 -SkipBuild
    } else {
        & "$root\scripts\capacity_ladder.ps1" -Scenario $c.Scenario -Count $c.Count -Duration $c.Duration -Seed 4242 -RampMs 50
    }
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED $($c.Scenario)@$($c.Count) exit=$LASTEXITCODE"
        $results += [pscustomobject]@{ scenario = $c.Scenario; count = $c.Count; ok = $false }
        continue
    }
    $latest = Get-ChildItem (Join-Path $root "logs\load\capacity_6g7b") -Directory |
        Where-Object { $_.Name -like "*_$($c.Scenario)_$($c.Count)n" } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($null -eq $latest) { continue }
    $td = Join-Path $latest.FullName "tick_domains.json"
    $rf = Join-Path $latest.FullName "replication_fanout.json"
    $pr = Join-Path $latest.FullName "process_resources.json"
    $row = [ordered]@{
        scenario = $c.Scenario
        count = $c.Count
        dir = $latest.Name
        ok = $true
    }
    if (Test-Path $td) {
        $j = Get-Content $td -Raw | ConvertFrom-Json
        $row.tick_p99 = $j.tick_total.p99_ms
        $row.aoi_p99 = $j.spatial_aoi.p99_ms
        $row.repl_p99 = $j.replication.p99_ms
        $row.tick_p95 = $j.tick_total.p95_ms
        $row.repl_p95 = $j.replication.p95_ms
        $row.overruns = $j.tick_overrun_count
    }
    if (Test-Path $rf) {
        $f = Get-Content $rf -Raw | ConvertFrom-Json
        $row.known_present = $f.known_relationships_present_total
        $row.known_scanned = $f.known_relationships_scanned_total
        $row.dirty_entities = $f.dirty_entities_total
        $row.interested = $f.interested_observers_total
        $row.updates = $f.updates_emitted_total
        $row.present_per_update = if ($f.updates_emitted_total -gt 0) { [math]::Round($f.known_relationships_present_total / $f.updates_emitted_total, 3) } else { 0 }
        $row.scanned_per_update = if ($f.updates_emitted_total -gt 0) { [math]::Round($f.known_relationships_scanned_total / $f.updates_emitted_total, 3) } else { 0 }
        $row.scanned_per_dirty = if ($f.dirty_entities_total -gt 0) { [math]::Round($f.known_relationships_scanned_total / $f.dirty_entities_total, 3) } else { 0 }
        $row.budget_deferred = $f.budget_deferred_total
        $row.recovery = $f.recovery_rescues_total
    }
    if (Test-Path $pr) {
        $p = Get-Content $pr -Raw | ConvertFrom-Json
        $row.cpu_util = $p.cpu_utilization_pct
        $row.ws_bytes = $p.working_set_bytes
    }
    $results += [pscustomobject]$row
    $results | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $summaryDir "ladder_summary.json")
}

$results | Format-Table -AutoSize | Out-String | Write-Host
Write-Host "Summary: $summaryDir\ladder_summary.json"
