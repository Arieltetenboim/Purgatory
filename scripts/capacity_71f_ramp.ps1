# Phase 7.1F connection-ramp attribution (measure only — no redesign).
#
# Hotspot 384 with admission 256. Success is a truthful funnel / ownership
# statement, not a 384-capacity claim and not harness exit 0.
param([switch]$SkipBuild)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$env:PURGATORY_REPLICATION_POLICY = "selective"
$env:PURGATORY_POPULATION_CLASS = "high"
$params = @{
    Scenario = "hotspot"
    Count = 384
    Duration = "60s"
    Seed = 4242
    RampMs = 10
    ArtifactRoot = "capacity_71"
    CapacityDetail = "1"
    MaxBots = 384
}
if ($SkipBuild) { $params.SkipBuild = $true }
& (Join-Path $root "scripts\capacity_ladder.ps1") @params
$code = $LASTEXITCODE
Remove-Item Env:PURGATORY_REPLICATION_POLICY -ErrorAction SilentlyContinue
Remove-Item Env:PURGATORY_POPULATION_CLASS -ErrorAction SilentlyContinue

$latest = Get-ChildItem (Join-Path $root "logs\load\capacity_71") -Directory |
    Where-Object { $_.Name -like "*_hotspot_384n" } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if ($latest) {
    $ramp = Join-Path $latest.FullName "connection_ramp.json"
    Write-Host "ramp_dir=$($latest.FullName)"
    if (Test-Path $ramp) {
        Write-Host "connection_ramp.json:"
        Get-Content $ramp -Raw
    } else {
        Write-Host "missing connection_ramp.json"
    }
}
exit $code
