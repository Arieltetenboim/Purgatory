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

[CmdletBinding()]
param(
    [ValidateSet("idle", "distributed", "hotspot", "churn", "scheduler", "mixed-stall", "soak", "smoke")]
    [string]$Scenario = "idle",
    [int]$Count = 32,
    [string]$Duration = "20s",
    [int]$Seed = 4242,
    [int]$RampMs = 50,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $ProjectRoot

$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$runRoot = Join-Path $ProjectRoot "logs\load\capacity_6g7c\${stamp}_${Scenario}_${Count}n"
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

Stop-OwnedServer
Start-Sleep -Seconds 1

$env:PURGATORY_ADMISSION_CAP = "256"
$env:PURGATORY_METRICS_PORT = "5002"
$env:PURGATORY_DATA_DIR = ($persist -replace '\\', '/')
$env:PURGATORY_CAPACITY_ARTIFACT_DIR = ($artifactDir -replace '\\', '/')
Remove-Item Env:PURGATORY_LOAD_PLACEMENT -ErrorAction SilentlyContinue
Remove-Item Env:PURGATORY_LOAD_VALIDATION -ErrorAction SilentlyContinue

$loadArgs = @()
switch ($Scenario) {
    "idle" {
        $env:PURGATORY_LOAD_PLACEMENT = "cluster"
        $loadArgs = @(
            "--count", "$Count", "--profile", "idle", "--scenario", "load",
            "--duration", $Duration, "--seed", "$Seed", "--ramp-ms", "$RampMs",
            "--allow-high-count", "--max-bots", "256",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "distributed" {
        $env:PURGATORY_LOAD_PLACEMENT = "spread"
        $loadArgs = @(
            "--count", "$Count", "--profile", "walker", "--scenario", "load",
            "--duration", $Duration, "--seed", "$Seed", "--ramp-ms", "$RampMs",
            "--allow-high-count", "--max-bots", "256",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "hotspot" {
        $env:PURGATORY_LOAD_PLACEMENT = "hotspot"
        $loadArgs = @(
            "--count", "$Count", "--profile", "mixed", "--scenario", "load",
            "--duration", $Duration, "--seed", "$Seed", "--ramp-ms", "$RampMs",
            "--allow-high-count", "--max-bots", "256",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "churn" {
        $env:PURGATORY_LOAD_PLACEMENT = "cluster"
        $loadArgs = @(
            "--count", "$Count", "--profile", "mixed", "--scenario", "churn",
            "--duration", $Duration, "--seed", "$Seed", "--ramp-ms", "$RampMs",
            "--allow-high-count", "--max-bots", "256",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "scheduler" {
        $loadArgs = @(
            "--preset", "scheduler", "--seed", "$Seed",
            "--allow-high-count", "--max-bots", "256",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "mixed-stall" {
        $env:PURGATORY_LOAD_PLACEMENT = "hotspot"
        $loadArgs = @(
            "--count", "$Count", "--profile", "mixed", "--scenario", "load",
            "--duration", $Duration, "--seed", "$Seed", "--ramp-ms", "$RampMs",
            "--allow-high-count", "--max-bots", "256",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "smoke" {
        $loadArgs = @(
            "--preset", "smoke", "--seed", "$Seed",
            "--allow-high-count", "--max-bots", "256",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
    "soak" {
        $loadArgs = @(
            "--preset", "soak", "--duration", $Duration, "--seed", "$Seed",
            "--allow-high-count", "--max-bots", "256",
            "--server", "127.0.0.1:5001", "--metrics", "127.0.0.1:5002",
            "--persist-root", ($persist -replace '\\', '/')
        )
    }
}

# Capture printed env for presets that inject PURGATORY_LOAD_VALIDATION.
if ($Scenario -in @("scheduler", "smoke", "soak")) {
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

foreach ($name in @("tick_domains.json", "process_resources.json", "run_summary.json")) {
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
load_exit=$loadExit
elapsed_secs=$elapsed
run_dir=$runRoot
"@ | Set-Content -Path (Join-Path $runRoot "ladder_meta.txt")

Write-Host "capacity_ladder done exit=$loadExit elapsed=${elapsed}s dir=$runRoot"
exit $loadExit
