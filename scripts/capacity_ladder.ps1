# Phase 6G.2 capacity ladder helper (measure only — no redesign).
#
# Starts a Release load-mode server with capacity artifacts enabled, runs one
# purgatory-load scenario, then stops the server. Localhost numbers are not
# player-capacity claims.
#
# Examples:
#   ./scripts/capacity_ladder.ps1 -Scenario idle -Count 64 -Duration 20s
#   ./scripts/capacity_ladder.ps1 -Scenario hotspot -Count 128 -Duration 45s
#   ./scripts/capacity_ladder.ps1 -Scenario mixed-stall -Count 128 -Duration 60s
#   ./scripts/capacity_ladder.ps1 -Scenario soak -Duration 30m
#   ./scripts/capacity_ladder.ps1 -Scenario representative-mixed -Count 8 -HealthMax 6 -NpcCount 24

[CmdletBinding()]
param(
    [ValidateSet("idle", "distributed", "hotspot", "churn", "scheduler", "mixed-stall", "soak", "smoke", "representative-light", "representative-mixed", "representative-dense")]
    [string]$Scenario = "idle",
    [int]$Count = 32,
    [string]$Duration = "20s",
    [int]$Seed = 4242,
    [int]$RampMs = 50,
    [string]$ArtifactRoot = "capacity_71",
    [string]$CapacityDetail = "1",
    [int]$MaxBots = 256,
    # Optional NPC workload overrides (-1 = leave preset / printed env unchanged).
    [int]$NpcCount = -1,
    [int]$ActivePct = -1,
    [int]$HotspotRadius = -1,
    [int]$ActionPeriodTicks = -1,
    [int]$PulsePeriodTicks = -1,
    [int]$RespawnDelayTicks = -1,
    [int]$HealthMax = -1,
    [int]$ChurnCount = -1,
    # Phase 7.4 network knobs (optional; -1 / empty = unset).
    [int]$SlowDrainCount = -1,
    [switch]$PostRampThin,
    [switch]$RelaxPortalGate,
    [int]$FrameBudgetBytes = -1,
    [int]$StrangerCadenceMult = -1,
    [string]$ReplicationPolicy = "",
    [string]$PopulationClass = "",
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $ProjectRoot

$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$npcTag = if ($NpcCount -ge 0) { "_npc${NpcCount}" } else { "" }
$actTag = if ($ActionPeriodTicks -ge 0) { "_ap${ActionPeriodTicks}" } else { "" }
$budTag = if ($FrameBudgetBytes -ge 0) { "_bud${FrameBudgetBytes}" } else { "" }
$slowTag = if ($SlowDrainCount -gt 0) { "_slow${SlowDrainCount}" } else { "" }
$runRoot = Join-Path $ProjectRoot "logs\load\${ArtifactRoot}\${stamp}_${Scenario}_${Count}n${npcTag}${actTag}${budTag}${slowTag}"
New-Item -ItemType Directory -Force -Path $runRoot | Out-Null
$persist = Join-Path $runRoot "persist"
$artifactDir = $runRoot
New-Item -ItemType Directory -Force -Path $persist | Out-Null

$serverExe = Join-Path $ProjectRoot "target\release\purgatory-server.exe"
$loadExe = Join-Path $ProjectRoot "target\release\purgatory-load.exe"

if (-not $SkipBuild) {
    Write-Host ">> cargo build -p purgatory-server -p purgatory-bot-client --release"
    cargo build -p purgatory-server -p purgatory-bot-client --release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

function Stop-OwnedServer {
    Get-CimInstance Win32_Process -Filter "Name = 'purgatory-server.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.CommandLine -like "*$ProjectRoot*" -or $_.ExecutablePath -like "*$ProjectRoot*" } |
        ForEach-Object {
            Write-Host "Stopping server PID $($_.ProcessId)"
            Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
        }
}

function Merge-NpcWorkloadOverrides {
    if (-not $env:PURGATORY_LOAD_VALIDATION) {
        return
    }
    $cfg = $env:PURGATORY_LOAD_VALIDATION | ConvertFrom-Json
    if (-not $cfg.npc_workload) {
        $cfg | Add-Member -NotePropertyName npc_workload -NotePropertyValue ([pscustomobject]@{}) -Force
    }
    $wl = $cfg.npc_workload
    if ($NpcCount -ge 0) { $wl | Add-Member -NotePropertyName count -NotePropertyValue $NpcCount -Force }
    if ($ActivePct -ge 0) { $wl | Add-Member -NotePropertyName active_pct -NotePropertyValue $ActivePct -Force }
    if ($HotspotRadius -ge 0) { $wl | Add-Member -NotePropertyName hotspot_radius -NotePropertyValue $HotspotRadius -Force }
    if ($ActionPeriodTicks -ge 0) { $wl | Add-Member -NotePropertyName action_period_ticks -NotePropertyValue $ActionPeriodTicks -Force }
    if ($PulsePeriodTicks -ge 0) { $wl | Add-Member -NotePropertyName pulse_period_ticks -NotePropertyValue $PulsePeriodTicks -Force }
    if ($RespawnDelayTicks -ge 0) { $wl | Add-Member -NotePropertyName respawn_delay_ticks -NotePropertyValue $RespawnDelayTicks -Force }
    if ($HealthMax -ge 0) { $wl | Add-Member -NotePropertyName health_max -NotePropertyValue $HealthMax -Force }
    if ($ChurnCount -ge 0) { $wl | Add-Member -NotePropertyName churn_count -NotePropertyValue $ChurnCount -Force }
    $cfg.npc_workload = $wl
    $env:PURGATORY_LOAD_VALIDATION = ($cfg | ConvertTo-Json -Compress -Depth 6)
    Write-Host "npc_workload overrides applied: $($env:PURGATORY_LOAD_VALIDATION)"
}

Stop-OwnedServer
Start-Sleep -Seconds 1

$env:PURGATORY_ADMISSION_CAP = "256"
$env:PURGATORY_METRICS_PORT = "5002"
$env:PURGATORY_DATA_DIR = ($persist -replace '\\', '/')
$env:PURGATORY_CAPACITY_ARTIFACT_DIR = ($artifactDir -replace '\\', '/')
$env:PURGATORY_CAPACITY_DETAIL = $CapacityDetail
if (-not [string]::IsNullOrWhiteSpace($ReplicationPolicy)) {
    $env:PURGATORY_REPLICATION_POLICY = $ReplicationPolicy
}
if (-not [string]::IsNullOrWhiteSpace($PopulationClass)) {
    $env:PURGATORY_POPULATION_CLASS = $PopulationClass
}
if ($FrameBudgetBytes -ge 0) {
    $env:PURGATORY_REPLICATION_FRAME_BUDGET_BYTES = "$FrameBudgetBytes"
} else {
    Remove-Item Env:PURGATORY_REPLICATION_FRAME_BUDGET_BYTES -ErrorAction SilentlyContinue
}
if ($StrangerCadenceMult -ge 1) {
    $env:PURGATORY_REPLICATION_STRANGER_CADENCE_MULT = "$StrangerCadenceMult"
} else {
    Remove-Item Env:PURGATORY_REPLICATION_STRANGER_CADENCE_MULT -ErrorAction SilentlyContinue
}
if ($SlowDrainCount -ge 0) {
    $env:PURGATORY_LOAD_SLOW_DRAIN_COUNT = "$SlowDrainCount"
} else {
    Remove-Item Env:PURGATORY_LOAD_SLOW_DRAIN_COUNT -ErrorAction SilentlyContinue
}
if ($PostRampThin) {
    $env:PURGATORY_LOAD_POST_RAMP_THIN = "1"
} else {
    Remove-Item Env:PURGATORY_LOAD_POST_RAMP_THIN -ErrorAction SilentlyContinue
}
if ($RelaxPortalGate) {
    $env:PURGATORY_LOAD_RELAX_PORTAL_GATE = "1"
} else {
    Remove-Item Env:PURGATORY_LOAD_RELAX_PORTAL_GATE -ErrorAction SilentlyContinue
}
Remove-Item Env:PURGATORY_LOAD_PLACEMENT -ErrorAction SilentlyContinue
Remove-Item Env:PURGATORY_LOAD_VALIDATION -ErrorAction SilentlyContinue

$loadArgs = @()
switch ($Scenario) {
    "idle" {
        $env:PURGATORY_LOAD_PLACEMENT = "cluster"
        $loadArgs = @(
            "--count", "$Count", "--profile", "idle", "--scenario", "load",
            "--duration", $Duration, "--seed", "$Seed", "--ramp-ms", "$RampMs",
            "--allow-high-count", "--max-bots", "$MaxBots",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "distributed" {
        $env:PURGATORY_LOAD_PLACEMENT = "spread"
        $loadArgs = @(
            "--count", "$Count", "--profile", "walker", "--scenario", "load",
            "--duration", $Duration, "--seed", "$Seed", "--ramp-ms", "$RampMs",
            "--allow-high-count", "--max-bots", "$MaxBots",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "hotspot" {
        $env:PURGATORY_LOAD_PLACEMENT = "hotspot"
        $loadArgs = @(
            "--count", "$Count", "--profile", "mixed", "--scenario", "load",
            "--duration", $Duration, "--seed", "$Seed", "--ramp-ms", "$RampMs",
            "--allow-high-count", "--max-bots", "$MaxBots",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "churn" {
        $env:PURGATORY_LOAD_PLACEMENT = "cluster"
        $loadArgs = @(
            "--count", "$Count", "--profile", "mixed", "--scenario", "churn",
            "--duration", $Duration, "--seed", "$Seed", "--ramp-ms", "$RampMs",
            "--allow-high-count", "--max-bots", "$MaxBots",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "scheduler" {
        $loadArgs = @(
            "--preset", "scheduler", "--seed", "$Seed",
            "--allow-high-count", "--max-bots", "$MaxBots",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "mixed-stall" {
        $env:PURGATORY_LOAD_PLACEMENT = "hotspot"
        $loadArgs = @(
            "--count", "$Count", "--profile", "mixed", "--scenario", "load",
            "--duration", $Duration, "--seed", "$Seed", "--ramp-ms", "$RampMs",
            "--allow-high-count", "--max-bots", "$MaxBots",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "smoke" {
        $loadArgs = @(
            "--preset", "smoke", "--seed", "$Seed",
            "--allow-high-count", "--max-bots", "$MaxBots",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "soak" {
        $loadArgs = @(
            "--preset", "soak", "--duration", $Duration, "--seed", "$Seed",
            "--allow-high-count", "--max-bots", "$MaxBots",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "representative-light" {
        $env:PURGATORY_LOAD_PLACEMENT = "hotspot"
        $loadArgs = @(
            "--preset", "representative-light", "--count", "$Count",
            "--duration", $Duration, "--seed", "$Seed", "--ramp-ms", "$RampMs",
            "--allow-high-count", "--max-bots", "$MaxBots",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "representative-mixed" {
        $env:PURGATORY_LOAD_PLACEMENT = "hotspot"
        $loadArgs = @(
            "--preset", "representative-mixed", "--count", "$Count",
            "--duration", $Duration, "--seed", "$Seed", "--ramp-ms", "$RampMs",
            "--allow-high-count", "--max-bots", "$MaxBots",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "representative-dense" {
        $env:PURGATORY_LOAD_PLACEMENT = "hotspot"
        $loadArgs = @(
            "--preset", "representative-dense", "--count", "$Count",
            "--duration", $Duration, "--seed", "$Seed", "--ramp-ms", "$RampMs",
            "--allow-high-count", "--max-bots", "$MaxBots",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
}

# Capture printed env for presets that inject PURGATORY_LOAD_VALIDATION.
if ($Scenario -in @("scheduler", "smoke", "soak", "representative-light", "representative-mixed", "representative-dense")) {
    $printArgs = $loadArgs + @("--print-server-env")
    $printOut = & $loadExe @printArgs 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        Write-Host $printOut
        exit $LASTEXITCODE
    }
    $pairs = $printOut.Trim() | ConvertFrom-Json
    foreach ($pair in $pairs) {
        Set-Item -Path "Env:$($pair[0])" -Value "$($pair[1])"
    }
    # Keep capacity artifacts under this script's run root.
    $env:PURGATORY_CAPACITY_ARTIFACT_DIR = ($artifactDir -replace '\\', '/')
    $env:PURGATORY_DATA_DIR = ($persist -replace '\\', '/')
    Merge-NpcWorkloadOverrides
    # Re-assert 7.4 knobs after print-server-env (preset env may not include them).
    if ($FrameBudgetBytes -ge 0) { $env:PURGATORY_REPLICATION_FRAME_BUDGET_BYTES = "$FrameBudgetBytes" }
    if ($StrangerCadenceMult -ge 1) { $env:PURGATORY_REPLICATION_STRANGER_CADENCE_MULT = "$StrangerCadenceMult" }
    if (-not [string]::IsNullOrWhiteSpace($ReplicationPolicy)) { $env:PURGATORY_REPLICATION_POLICY = $ReplicationPolicy }
    if (-not [string]::IsNullOrWhiteSpace($PopulationClass)) { $env:PURGATORY_POPULATION_CLASS = $PopulationClass }
} elseif ($NpcCount -ge 0 -or $HealthMax -ge 0 -or $ActionPeriodTicks -ge 0 -or $PulsePeriodTicks -ge 0 -or $ActivePct -ge 0) {
    # Non-preset scenarios (e.g. hotspot soak): seed LOAD_VALIDATION so NPC overrides apply.
    if (-not $env:PURGATORY_LOAD_VALIDATION) {
        $env:PURGATORY_LOAD_VALIDATION = '{"npc_workload":{}}'
    }
    Merge-NpcWorkloadOverrides
}

if ($SlowDrainCount -gt 0) {
    $loadArgs += @("--slow-drain-count", "$SlowDrainCount")
}
if ($PostRampThin) {
    $loadArgs += @("--post-ramp-thin")
}
if ($RelaxPortalGate) {
    $loadArgs += @("--relax-portal-gate")
}

Write-Host "Run dir: $runRoot"
Write-Host "Starting server (load-mode, capacity artifacts on)"
# Do not RedirectStandardOutput: verbose portal logs can fill pipes; let the
# console inherit so a long soak cannot stall the server on a full buffer.
$serverProc = Start-Process -FilePath $serverExe -WorkingDirectory $ProjectRoot -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 2
if ($serverProc.HasExited) {
    Write-Error "Server exited early with code $($serverProc.ExitCode)"
    exit 1
}

Write-Host ">> purgatory-load $($loadArgs -join ' ')"
$started = Get-Date
& $loadExe @loadArgs | Out-Host
$loadExit = $LASTEXITCODE
$elapsed = [math]::Round(((Get-Date) - $started).TotalSeconds, 1)

Write-Host "Stopping server..."
if (-not $serverProc.HasExited) {
    Stop-Process -Id $serverProc.Id -Force -ErrorAction SilentlyContinue
}
Stop-OwnedServer

# Copy harness-owned run_summary into capacity artifact dir when written under logs/load.
$latestHarness = Get-ChildItem (Join-Path $ProjectRoot "logs\load") -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -notlike "capacity_*" -and $_.LastWriteTime -gt $started.AddSeconds(-5) } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if ($latestHarness) {
    foreach ($name in @("run_summary.json", "connection_ramp.json", "harness_resources.json", "harness_connection.json", "events.ndjson", "metrics.csv")) {
        $src = Join-Path $latestHarness.FullName $name
        $dst = Join-Path $runRoot $name
        if ((Test-Path $src) -and -not (Test-Path $dst)) {
            Copy-Item -LiteralPath $src -Destination $dst -Force
            Write-Host "copied harness artifact: $name"
        }
    }
}

foreach ($name in @("tick_domains.json", "process_resources.json", "capacity_live.json", "gameplay_workload.json", "network_pressure.json", "connection_lifecycle.json", "harness_resources.json", "harness_connection.json", "connection_ramp.json", "run_summary.json", "replication_fanout.json", "aoi_locality.json")) {
    $src = Join-Path $runRoot $name
    if (Test-Path $src) {
        Write-Host "artifact: $src"
    }
}

@"
scenario=$Scenario
count=$Count
duration=$Duration
seed=$Seed
npc_count=$NpcCount
action_period=$ActionPeriodTicks
pulse_period=$PulsePeriodTicks
health_max=$HealthMax
respawn_delay=$RespawnDelayTicks
load_exit=$loadExit
elapsed_secs=$elapsed
run_dir=$runRoot
"@ | Set-Content -Path (Join-Path $runRoot "ladder_meta.txt")

Write-Host "capacity_ladder done exit=$loadExit elapsed=${elapsed}s dir=$runRoot"
exit $loadExit
