# Phase 7.3A — harness issuance proof (measure only).
#
# Moderate (128) + high-N (384 requested / admission 256). Success is issuance
# attainment + funnel_invariant_ok, not a 384-server capacity claim.
param(
    [switch]$SkipBuild
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$summaryDir = Join-Path $root "logs\load\capacity_73\summary_73a_$stamp"
New-Item -ItemType Directory -Force -Path $summaryDir | Out-Null

$env:PURGATORY_REPLICATION_POLICY = "selective"
$env:PURGATORY_POPULATION_CLASS = "high"

$ladder = Join-Path $root "scripts\capacity_ladder.ps1"
$script:skipBuild = [bool]$SkipBuild

function Invoke-73aRun {
    param(
        [string]$Name,
        [string]$Scenario,
        [int]$Count,
        [string]$Duration,
        [int]$RampMs,
        [int]$MaxBots
    )
    Write-Host "==== $Name ===="
    $params = @{
        Scenario = $Scenario
        Count = $Count
        Duration = $Duration
        Seed = 7301
        RampMs = $RampMs
        ArtifactRoot = "capacity_73"
        CapacityDetail = "1"
        MaxBots = $MaxBots
    }
    if ($script:skipBuild) { $params.SkipBuild = $true }
    & $ladder @params
    $code = $LASTEXITCODE
    $script:skipBuild = $true
    $latest = Get-ChildItem (Join-Path $root "logs\load\capacity_73") -Directory |
        Where-Object { $_.Name -like "*_${Scenario}_${Count}n" } |
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

$results = @()
$results += Invoke-73aRun -Name "moderate_128" -Scenario "hotspot" -Count 128 -Duration "45s" -RampMs 10 -MaxBots 256
$results += Invoke-73aRun -Name "highn_384_admit256" -Scenario "hotspot" -Count 384 -Duration "60s" -RampMs 10 -MaxBots 384

Remove-Item Env:PURGATORY_REPLICATION_POLICY -ErrorAction SilentlyContinue
Remove-Item Env:PURGATORY_POPULATION_CLASS -ErrorAction SilentlyContinue

$summary = @()
$gateFail = 0
foreach ($r in $results) {
    $row = [ordered]@{
        name = $r.name
        exit = $r.exit
        dir = $r.dir
        count = $r.count
    }
    if ($r.path) {
        $ramp = Join-Path $r.path "connection_ramp.json"
        if (Test-Path $ramp) {
            $j = Get-Content $ramp -Raw | ConvertFrom-Json
            $row.requested = $j.requested_clients
            $row.spawn_issued = $j.spawn_issued_total
            $row.attempts = $j.connect_attempts_completed
            $row.transport = $j.transport_established_total
            $row.welcome = $j.welcome_total
            $row.world_entered = $j.world_entered_total
            $row.peak_active = $j.peak_active_clients
            $row.attainment_pct = $j.attainment_pct
            $row.admission_refused = $j.admission_refused
            $row.admission_cap = $j.admission_cap
            $row.controller_ticks = $j.controller_ticks
            $row.controller_tick_p50_ms = $j.controller_tick_p50_ms
            $row.controller_tick_p99_ms = $j.controller_tick_p99_ms
            $row.tick_all_bots_p50_ms = $j.tick_all_bots_p50_ms
            $row.tick_all_bots_p99_ms = $j.tick_all_bots_p99_ms
            $row.spawn_catchup = $j.spawn_catchup_issued_total
            $row.spawn_due_peak = $j.spawn_due_peak
            $row.funnel_invariant_ok = $j.funnel_invariant_ok
            $row.funnel_invariant_note = $j.funnel_invariant_note
            $row.ownership = $j.ownership_statement
            $issueRatio = if ($j.requested_clients -gt 0) { $j.spawn_issued_total / $j.requested_clients } else { 0 }
            $row.issue_ratio = [math]::Round($issueRatio, 4)
            # 7.3A gate is issuance + funnel invariant. Load exit may be FAILED for
            # unrelated post-ramp snapshot starvation; admission-wall cells exit
            # non-zero when peak < requested. Those are not issuance failures.
            $okIssue = $issueRatio -ge 0.95
            $okInv = [bool]$j.funnel_invariant_ok
            $row.gate_issue_ok = $okIssue
            $row.gate_invariant_ok = $okInv
            if ($r.name -eq "highn_384_admit256") {
                $row.gate_admit_wall_ok = ($j.peak_active_clients -ge ($j.admission_cap * 0.9)) -and ($j.admission_refused -gt 0)
            }
            if (-not $okIssue -or -not $okInv) {
                $gateFail++
            }
        } else {
            $gateFail++
            $row.error = "missing connection_ramp.json"
        }
    } else {
        $gateFail++
        $row.error = "missing run dir"
    }
    $summary += [pscustomobject]$row
}

$summary | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $summaryDir "phase73a_issuance_proof.json")
$summary | Format-List | Out-String | Write-Host
Write-Host "Summary: $summaryDir"
if ($gateFail -gt 0) {
    Write-Host "7.3A issuance gate FAILED ($gateFail cells)"
    exit 1
}
Write-Host "7.3A issuance gate PASS"
exit 0
