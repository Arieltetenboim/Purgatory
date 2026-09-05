# 6G.7C baseline vs selective hotspot ladder.
param([switch]$SkipBuild)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$summaryDir = Join-Path $root "logs\load\capacity_6g7c\summary_$stamp"
New-Item -ItemType Directory -Force -Path $summaryDir | Out-Null

$modes = @("baseline", "selective")
$counts = @(64, 128, 256)
$results = @()

foreach ($mode in $modes) {
    $env:PURGATORY_REPLICATION_POLICY = $mode
    $env:PURGATORY_POPULATION_CLASS = if ($mode -eq "selective") { "high" } else { "low" }
    foreach ($n in $counts) {
        Write-Host "==== policy=$mode hotspot@$n ===="
        if ($SkipBuild) {
            & "$root\scripts\capacity_ladder.ps1" -Scenario hotspot -Count $n -Duration "30s" -Seed 4242 -RampMs 50 -ArtifactRoot capacity_6g7c -SkipBuild
        } else {
            & "$root\scripts\capacity_ladder.ps1" -Scenario hotspot -Count $n -Duration "30s" -Seed 4242 -RampMs 50 -ArtifactRoot capacity_6g7c
            $SkipBuild = $true
        }
        if ($LASTEXITCODE -ne 0) {
            $results += [pscustomobject]@{ mode = $mode; count = $n; ok = $false }
            continue
        }
        $latest = Get-ChildItem (Join-Path $root "logs\load\capacity_6g7c") -Directory |
            Where-Object { $_.Name -like "*_hotspot_${n}n" } |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1
        if ($null -eq $latest) { continue }
        # Tag policy mode into a sidecar for clarity
        Set-Content (Join-Path $latest.FullName "policy_mode.txt") $mode
        $td = Join-Path $latest.FullName "tick_domains.json"
        $rf = Join-Path $latest.FullName "replication_fanout.json"
        $pr = Join-Path $latest.FullName "process_resources.json"
        $row = [ordered]@{ mode = $mode; count = $n; dir = $latest.Name; ok = $true }
        if (Test-Path $td) {
            $j = Get-Content $td -Raw | ConvertFrom-Json
            $row.tick_p99 = $j.tick_total.p99_ms
            $row.repl_p99 = $j.replication.p99_ms
            $row.aoi_p99 = $j.spatial_aoi.p99_ms
            $row.overruns = $j.tick_overrun_count
            $row.tick_count = $j.tick_count
        }
        if (Test-Path $rf) {
            $f = Get-Content $rf -Raw | ConvertFrom-Json
            $row.updates = $f.updates_emitted_total
            $row.bytes = $f.bytes_emitted_total
            $row.eligible = $f.policy_eligible_total
            $row.suppressed = $f.policy_domain_suppressed_total
            $row.coalesced = $f.state_coalesced_total
            $row.priority_deferred = $f.priority_deferred_total
            $row.interested = $f.interested_observers_total
            $secs = if ($row.tick_count) { [double]$row.tick_count / 30.0 } else { 30.0 }
            $row.bytes_per_sec = if ($secs -gt 0) { [math]::Round($f.bytes_emitted_total / $secs, 1) } else { 0 }
            $row.bytes_per_client_sec = if ($secs -gt 0 -and $n -gt 0) { [math]::Round(($f.bytes_emitted_total / $secs) / $n, 1) } else { 0 }
        }
        if (Test-Path $pr) {
            $p = Get-Content $pr -Raw | ConvertFrom-Json
            $row.cpu_util = $p.cpu_utilization_pct
            $row.ws_bytes = $p.working_set_bytes
        }
        $results += [pscustomobject]$row
        $results | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $summaryDir "policy_ladder_summary.json")
    }
}

Remove-Item Env:PURGATORY_REPLICATION_POLICY -ErrorAction SilentlyContinue
Remove-Item Env:PURGATORY_POPULATION_CLASS -ErrorAction SilentlyContinue
$results | Format-Table -AutoSize | Out-String | Write-Host
Write-Host "Summary: $summaryDir\policy_ladder_summary.json"
