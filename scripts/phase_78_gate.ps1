# Phase 7.8 — Production Performance Gate
#
# Canonical entry point for the single-process performance regression gate.
# Does NOT reopen scaling architecture (ADR-0056).
#
# Usage:
#   ./scripts/phase_78_gate.ps1
#   ./scripts/phase_78_gate.ps1 -SkipBuild
#   ./scripts/phase_78_gate.ps1 -SkipSoak
#   ./scripts/phase_78_gate.ps1 -Only functional_mixed8,standard_mixed64
#   ./scripts/phase_78_gate.ps1 -FreezeBaseline
#
# Exit codes:
#   0 = GREEN or YELLOW
#   1 = RED
#   2 = INVALID (harness/workload)

param(
    [switch]$SkipBuild,
    [switch]$SkipSoak,
    [switch]$SkipUnit,
    [switch]$FreezeBaseline,
    [string]$Only = "",
    [string]$ThresholdsPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

. (Join-Path $PSScriptRoot "phase_78_evaluate.ps1")

if ([string]::IsNullOrWhiteSpace($ThresholdsPath)) {
    $ThresholdsPath = Join-Path $PSScriptRoot "phase_78_thresholds.json"
}
$cfg = Get-Content -LiteralPath $ThresholdsPath -Raw | ConvertFrom-Json
$policy = $cfg.policy
$globalCfg = $cfg.global

$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$runRoot = Join-Path $root "logs\load\capacity_78\gate_$stamp"
New-Item -ItemType Directory -Force -Path $runRoot | Out-Null
$baselineDir = Join-Path $root "logs\load\capacity_78\baseline"
$baselinePath = Join-Path $baselineDir "baseline.json"

$cellOrder = @(
    "functional_mixed8",
    "standard_mixed64",
    "high_mixed128",
    "dense_dense64",
    "soak_mixed32"
)

$want = @{}
if ([string]::IsNullOrWhiteSpace($Only)) {
    foreach ($id in $cellOrder) { $want[$id] = $true }
} else {
    foreach ($k in ($Only -split '[,;\s]+')) {
        $key = $k.Trim()
        if ($key) { $want[$key] = $true }
    }
}
if ($SkipSoak) { $want.Remove("soak_mixed32") }

Write-Host "Phase 7.8 Production Performance Gate"
Write-Host "Run root: $runRoot"
Write-Host ("Cells: " + (($want.Keys | Sort-Object) -join ", "))

if (-not $SkipBuild) {
    Write-Host ">> cargo build -p purgatory-server -p purgatory-bot-client --release"
    cargo build -p purgatory-server -p purgatory-bot-client --release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $script:skipBuild = $true
} else {
    $script:skipBuild = $true
}

$ladder = Join-Path $root "scripts\capacity_ladder.ps1"

function Invoke-78LadderCell {
    param($CellCfg)
    Write-Host ""
    Write-Host ("==== {0} ({1} @{2} {3}) ====" -f $CellCfg.name, $CellCfg.scenario, $CellCfg.count, $CellCfg.duration)
    Start-Sleep -Seconds 2
    $seed = [int]$policy.seed
    $seedOverride = Get-78Prop $CellCfg "seed"
    if ($null -ne $seedOverride) { $seed = [int]$seedOverride }
    $health = [int]$policy.health_max
    $healthOverride = Get-78Prop $CellCfg "health_max"
    if ($null -ne $healthOverride) { $health = [int]$healthOverride }
    $params = @{
        Scenario = [string]$CellCfg.scenario
        Count = [int]$CellCfg.count
        Duration = [string]$CellCfg.duration
        Seed = $seed
        RampMs = [int]$CellCfg.ramp_ms
        ArtifactRoot = "capacity_78"
        CapacityDetail = [string]$policy.capacity_detail
        MaxBots = [int]$CellCfg.max_bots
        HealthMax = $health
        RespawnDelayTicks = [int]$policy.respawn_delay_ticks
        ReplicationPolicy = [string]$policy.replication_policy
        PopulationClass = [string]$policy.population_class
        FrameBudgetBytes = [int]$policy.frame_budget_bytes
        StrangerCadenceMult = [int]$policy.stranger_cadence_mult
    }
    $npcCount = Get-78Prop $CellCfg "npc_count"
    if ($null -ne $npcCount) { $params.NpcCount = [int]$npcCount }
    $activePct = Get-78Prop $CellCfg "active_pct"
    if ($null -ne $activePct) { $params.ActivePct = [int]$activePct }
    $ap = Get-78Prop $CellCfg "action_period_ticks"
    if ($null -ne $ap) { $params.ActionPeriodTicks = [int]$ap }
    $pp = Get-78Prop $CellCfg "pulse_period_ticks"
    if ($null -ne $pp) { $params.PulsePeriodTicks = [int]$pp }
    if ([bool](Get-78Prop $CellCfg "post_ramp_thin" $false)) { $params.PostRampThin = $true }
    if ([bool](Get-78Prop $CellCfg "relax_portal_gate" $false)) { $params.RelaxPortalGate = $true }
    if ($script:skipBuild) { $params.SkipBuild = $true }
    & $ladder @params
    $code = $LASTEXITCODE
    $script:skipBuild = $true

    $latest = Get-ChildItem (Join-Path $root "logs\load\capacity_78") -Directory -ErrorAction SilentlyContinue |
        Where-Object {
            $_.Name -like "*_$($CellCfg.scenario)_$($CellCfg.count)n*" -and
            $_.Name -notlike "gate_*" -and
            $_.Name -ne "baseline"
        } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1

    $alias = Join-Path $runRoot $CellCfg.name
    if ($latest) {
        New-Item -ItemType Directory -Force -Path $alias | Out-Null
        Set-Content -LiteralPath (Join-Path $alias "source_dir.txt") -Value $latest.FullName
        foreach ($f in @(
            "tick_domains.json", "process_resources.json", "network_pressure.json",
            "connection_ramp.json", "gameplay_workload.json", "run_summary.json",
            "replication_fanout.json", "capacity_live.json", "ladder_meta.txt", "scenario.json"
        )) {
            $src = Join-Path $latest.FullName $f
            if (Test-Path -LiteralPath $src) {
                Copy-Item -LiteralPath $src -Destination (Join-Path $alias $f) -Force
            }
        }
    }

    return [pscustomobject]@{
        cell_id = $CellCfg.name
        exit = $code
        path = if ($latest) { $latest.FullName } else { $null }
        alias = $alias
    }
}

function Invoke-78UnitBudget {
    Write-Host ""
    Write-Host "==== unit_budget (cargo test) ===="
    $filters = @($cfg.unit_budget_tests)
    $joined = ($filters | ForEach-Object { $_ }) -join "|"
    $args = @(
        "test", "-p", "purgatory-server", "--release", "--", "--test-threads", "1"
    )
    # Run named tests; rustc filter is substring OR via multiple --exact calls is awkward —
    # use a single filter regex-ish by running each.
    $failed = @()
    $passed = @()
    foreach ($t in $filters) {
        Write-Host ">> cargo test -p purgatory-server --release $t"
        $prevEa = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        $out = & cargo test -p purgatory-server --release $t -- --test-threads 1 2>&1 | ForEach-Object { "$_" } | Out-String
        $code = $LASTEXITCODE
        $ErrorActionPreference = $prevEa
        Write-Host $out
        if ($code -ne 0) {
            $failed += $t
            continue
        }
        if ($out -notmatch "test result: ok\.\s+[1-9]\d* passed") {
            Write-Host "WARN: filter '$t' did not report >=1 passed test"
            $failed += $t
        } else {
            $passed += $t
        }
    }
    $status = if ($failed.Count -gt 0) { "FAIL" } else { "PASS" }
    return [pscustomobject]@{
        status = $status
        passed = $passed
        failed = $failed
    }
}

$baseline = $null
if (Test-Path -LiteralPath $baselinePath) {
    $baseline = Get-Content -LiteralPath $baselinePath -Raw | ConvertFrom-Json
    Write-Host "Loaded baseline: $baselinePath"
} else {
    Write-Host "No frozen baseline yet (will freeze on GREEN/YELLOW if -FreezeBaseline or first success)."
}

$unitResult = $null
if (-not $SkipUnit) {
    $unitResult = Invoke-78UnitBudget
    $script:skipBuild = $true
} else {
    $unitResult = [pscustomobject]@{ status = "PASS"; passed = @(); failed = @(); skipped = $true }
}

# Resolve cell config from thresholds JSON (PowerShell ConvertFrom-Json).
function Get-78CellCfg {
    param($Cfg, [string]$Id)
    $p = $Cfg.cells.PSObject.Properties[$Id]
    if ($null -eq $p) { return $null }
    return $p.Value
}

function Get-78BaselineCell {
    param($Baseline, [string]$Id)
    if ($null -eq $Baseline -or $null -eq $Baseline.cells) { return $null }
    $p = $Baseline.cells.PSObject.Properties[$Id]
    if ($null -eq $p) { return $null }
    return $p.Value
}

$cellResults = @()
$metricSnapshots = @{}

foreach ($id in $cellOrder) {
    if (-not $want.ContainsKey($id)) { continue }
    $cellCfg = Get-78CellCfg $cfg $id
    if ($null -eq $cellCfg) {
        Write-Error "Unknown cell id in thresholds: $id"
    }
    $retries = 1
    $retryOverride = Get-78Prop $cellCfg "soak_retries"
    if ($null -ne $retryOverride) { $retries = [int]$retryOverride }
    $eval = $null
    $metrics = $null
    for ($attempt = 1; $attempt -le $retries; $attempt++) {
        if ($attempt -gt 1) {
            Write-Host ("Retry {0}/{1} for {2} (prior harness invalid/short run)" -f $attempt, $retries, $id)
            # Vary seed on retry
            $cellCfg | Add-Member -NotePropertyName seed -NotePropertyValue ([int](Get-78Prop $cellCfg "seed" $policy.seed) + $attempt) -Force
        }
        $run = Invoke-78LadderCell $cellCfg
        $metrics = Read-78CellMetrics -Path $run.path -CellId $id -Duration ([string]$cellCfg.duration)
        $metrics | Add-Member -NotePropertyName harness_exit -NotePropertyValue $run.exit -Force
        $baseCell = Get-78BaselineCell $baseline $id
        $eval = Evaluate-78Cell -Metrics $metrics -CellCfg $cellCfg -GlobalCfg $globalCfg -BaselineCell $baseCell
        $short = $false
        foreach ($f in $eval.findings) {
            if ($f.code -eq "HARNESS_SHORT_RUN") { $short = $true }
        }
        if ($eval.status -ne "HARNESS_INVALID" -or -not $short -or $attempt -eq $retries) {
            break
        }
    }
    $cellResults += $eval
    $metricSnapshots[$id] = $metrics

    Write-Host ("  -> {0}  tick_p99={1} util={2} att={3}% dominant={4}" -f `
        $eval.status, $metrics.tick_p99, $metrics.tick_util, $metrics.attainment_pct, $metrics.dominant)
    foreach ($f in $eval.findings) {
        Write-Host ("     [{0}] {1}: {2}" -f $f.severity, $f.code, $f.message)
    }
}

$verdict = Reduce-78GateVerdict -CellResults $cellResults -UnitStatus $unitResult.status

# Build metadata
$gitSha = $null
try { $gitSha = (git -C $root rev-parse --short HEAD 2>$null) } catch {}
$rustc = $null
try { $rustc = (rustc --version 2>$null) } catch {}

$summary = [ordered]@{
    schema = 1
    phase = "7.8"
    verdict = $verdict
    stamp = $stamp
    run_root = $runRoot
    thresholds = $ThresholdsPath
    baseline_path = if (Test-Path $baselinePath) { $baselinePath } else { $null }
    baseline_compared = ($null -ne $baseline)
    policy = $policy
    git_sha = "$gitSha"
    rustc = "$rustc"
    unit_budget = $unitResult
    cells = @($cellResults | ForEach-Object {
            [ordered]@{
                cell_id = $_.cell_id
                status = $_.status
                findings = @($_.findings)
                metrics = $_.metrics
            }
        })
    semantics = [ordered]@{
        GREEN = "All cells PASS absolute + baseline (or no baseline yet)."
        YELLOW = "WARN findings (incl. harness starve / relative regression) but absolute operational thresholds intact."
        RED = "SERVER_FAIL or CORRECTNESS_FAIL on any cell, or unit budget tests failed."
        INVALID = "Harness/workload attainment invalid without a server absolute failure."
    }
}

$summaryPath = Join-Path $runRoot "phase78_gate_summary.json"
($summary | ConvertTo-Json -Depth 10) | Set-Content -LiteralPath $summaryPath -Encoding utf8

$human = Join-Path $runRoot "phase78_gate_summary.txt"
$lines = @(
    "Phase 7.8 Production Performance Gate",
    "Verdict: $verdict",
    "Stamp: $stamp",
    "Run: $runRoot",
    ""
)
foreach ($c in $cellResults) {
    $lines += ("[{0}] {1}" -f $c.status, $c.cell_id)
    foreach ($f in $c.findings) {
        $lines += ("  - {0} {1}: {2}" -f $f.severity, $f.code, $f.message)
    }
}
$lines += ""
$lines += ("unit_budget: {0}" -f $unitResult.status)
if ($unitResult.failed) { $lines += ("  failed: {0}" -f ($unitResult.failed -join ", ")) }
$lines | Set-Content -LiteralPath $human -Encoding utf8

Write-Host ""
Write-Host "======== VERDICT: $verdict ========"
Write-Host "Summary JSON: $summaryPath"
Write-Host "Summary text: $human"

$shouldFreeze = $false
if ($FreezeBaseline) { $shouldFreeze = $true }
elseif (($verdict -eq "GREEN" -or $verdict -eq "YELLOW") -and -not (Test-Path -LiteralPath $baselinePath)) {
    $shouldFreeze = $true
    Write-Host "Auto-freezing first successful baseline."
}

if ($shouldFreeze -and ($verdict -eq "GREEN" -or $verdict -eq "YELLOW")) {
    New-Item -ItemType Directory -Force -Path $baselineDir | Out-Null
    $cellsObj = [ordered]@{}
    foreach ($id in $metricSnapshots.Keys) {
        $cellsObj[$id] = $metricSnapshots[$id]
    }
    $baseDoc = [ordered]@{
        schema = 1
        phase = "7.8"
        frozen_at = (Get-Date -Format "o")
        stamp = $stamp
        source_run = $runRoot
        git_sha = "$gitSha"
        policy = $policy
        thresholds = $ThresholdsPath
        cells = $cellsObj
    }
    ($baseDoc | ConvertTo-Json -Depth 10) | Set-Content -LiteralPath $baselinePath -Encoding utf8
    # Also keep a stamped copy
    Copy-Item -LiteralPath $baselinePath -Destination (Join-Path $baselineDir "baseline_$stamp.json") -Force
    Write-Host "Baseline frozen: $baselinePath"
} elseif ($FreezeBaseline -and $verdict -ne "GREEN" -and $verdict -ne "YELLOW") {
    Write-Host "Refusing to freeze baseline on verdict $verdict"
}

switch ($verdict) {
    "GREEN" { $exitCode = 0 }
    "YELLOW" { $exitCode = 0 }
    "RED" { $exitCode = 1 }
    "INVALID" { $exitCode = 2 }
    default { $exitCode = 1 }
}

# Do not leak load-mode env into the parent shell (pollutes subsequent cargo test).
foreach ($name in @(
    "PURGATORY_LOAD_VALIDATION", "PURGATORY_CAPACITY_ARTIFACT_DIR", "PURGATORY_CAPACITY_DETAIL",
    "PURGATORY_DATA_DIR", "PURGATORY_ADMISSION_CAP", "PURGATORY_METRICS_PORT",
    "PURGATORY_REPLICATION_POLICY", "PURGATORY_POPULATION_CLASS",
    "PURGATORY_REPLICATION_FRAME_BUDGET_BYTES", "PURGATORY_REPLICATION_STRANGER_CADENCE_MULT",
    "PURGATORY_LOAD_POST_RAMP_THIN", "PURGATORY_LOAD_RELAX_PORTAL_GATE", "PURGATORY_LOAD_SLOW_DRAIN_COUNT",
    "PURGATORY_LOAD_PLACEMENT"
)) {
    Remove-Item "Env:$name" -ErrorAction SilentlyContinue
}

exit $exitCode
