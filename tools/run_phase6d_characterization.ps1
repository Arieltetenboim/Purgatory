# Phase 6D Scenario A/B/C characterization. Localhost. Not a capacity claim.
# Requires release binaries. Restarts the server per (placement, N).

param(
    [int[]]$Counts = @(10, 25, 50),
    [string]$Profile = "mixed",
    [string]$Seed = "4242",
    [string]$Duration = "45s",
    [int]$RampMs = 50,
    [int]$Admission = 256
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$ServerExe = Join-Path $Root "target\release\purgatory-server.exe"
$LoadExe = Join-Path $Root "target\release\purgatory-load.exe"
if (-not (Test-Path $ServerExe)) { throw "missing $ServerExe (build release server + bot_client first)" }
if (-not (Test-Path $LoadExe)) { throw "missing $LoadExe" }

$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$OutRoot = Join-Path $Root "logs\load\phase6d_$stamp"
New-Item -ItemType Directory -Force -Path $OutRoot | Out-Null
$listFile = Join-Path $OutRoot "runs.txt"
"" | Set-Content -Path $listFile

$placements = @(
    @{ Name = "A_cluster"; Env = "cluster" },
    @{ Name = "B_spread"; Env = "spread" },
    @{ Name = "C_maps"; Env = "maps" }
)

function Stop-PurgatoryProcs {
    Get-Process -Name "purgatory-server","purgatory-load" -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
}

function Start-LoadServer([string]$Placement, [string]$LogTag) {
    Stop-PurgatoryProcs
    $env:PURGATORY_ADMISSION_CAP = "$Admission"
    $env:PURGATORY_METRICS_PORT = "5002"
    $env:PURGATORY_LOAD_PLACEMENT = $Placement
    $outLog = Join-Path $OutRoot "server_${LogTag}_out.log"
    $errLog = Join-Path $OutRoot "server_${LogTag}_err.log"
    $proc = Start-Process -FilePath $ServerExe -WorkingDirectory $Root `
        -RedirectStandardOutput $outLog -RedirectStandardError $errLog `
        -PassThru -WindowStyle Hidden
    $deadline = (Get-Date).AddSeconds(30)
    $ok = $false
    while ((Get-Date) -lt $deadline) {
        try {
            $udp = New-Object System.Net.Sockets.UdpClient
            $udp.Client.ReceiveTimeout = 500
            $req = [byte[]](0x50,0x55,0x52,0x47,0x53,0x54,0x41,0x54,0x01)
            $udp.Send($req, $req.Length, "127.0.0.1", 5002) | Out-Null
            $remote = New-Object System.Net.IPEndPoint ([System.Net.IPAddress]::Any, 0)
            $null = $udp.Receive([ref]$remote)
            $udp.Close()
            $ok = $true
            break
        } catch {
            Start-Sleep -Milliseconds 200
        }
    }
    if (-not $ok) { throw "server metrics not ready placement=$Placement pid=$($proc.Id)" }
    return $proc
}

Write-Host "=== Phase 6D characterization out=$OutRoot counts=$($Counts -join ',') duration=$Duration profile=$Profile ==="

foreach ($p in $placements) {
    foreach ($count in $Counts) {
        $tag = "$($p.Name)_n$count"
        Write-Host "`n--- $tag ---"
        $null = Start-LoadServer -Placement $p.Env -LogTag $tag

        $runDir = Join-Path $OutRoot $tag
        New-Item -ItemType Directory -Force -Path $runDir | Out-Null
        $stopFile = Join-Path $runDir "poll_stop"
        if (Test-Path $stopFile) { Remove-Item $stopFile -Force }
        $pollOut = Join-Path $runDir "metrics_schema2.jsonl"
        $pollProc = Start-Process -FilePath "python" -ArgumentList @(
            (Join-Path $Root "tools\poll_load_metrics.py"),
            "--out", $pollOut,
            "--until-file", $stopFile,
            "--max-secs", "300"
        ) -WorkingDirectory $Root -PassThru -WindowStyle Hidden `
            -RedirectStandardOutput (Join-Path $runDir "poll_out.log") `
            -RedirectStandardError (Join-Path $runDir "poll_err.log")

        $before = Get-ChildItem (Join-Path $Root "logs\load") -Directory |
            Where-Object { $_.Name -match "^\d{8}_\d{6}_${count}bots_" } |
            Sort-Object Name

        & $LoadExe `
            --count $count `
            --profile $Profile `
            --scenario load `
            --duration $Duration `
            --ramp-ms $RampMs `
            --seed $Seed `
            --max-bots $Admission `
            --allow-high-count `
            --server 127.0.0.1:5001 `
            --metrics 127.0.0.1:5002
        $code = $LASTEXITCODE
        Write-Host "harness exit=$code"
        "stopped" | Set-Content -Path $stopFile
        Start-Sleep -Milliseconds 1200
        if (-not $pollProc.HasExited) {
            Stop-Process -Id $pollProc.Id -Force -ErrorAction SilentlyContinue
        }

        $after = Get-ChildItem (Join-Path $Root "logs\load") -Directory |
            Where-Object { $_.Name -match "^\d{8}_\d{6}_${count}bots_" } |
            Sort-Object Name
        $new = $after | Where-Object { $before.Name -notcontains $_.Name } | Select-Object -Last 1
        $harnessName = ""
        if ($null -ne $new) {
            $harnessName = $new.Name
        } else {
            $ptr = Join-Path $Root "logs\load\latest.txt"
            if (Test-Path $ptr) { $harnessName = (Get-Content $ptr -Raw).Trim() }
        }
        $meta = @{
            scenario = $p.Name
            placement = $p.Env
            count = $count
            harness_exit = $code
            harness_dir = $harnessName
            poll = "metrics_schema2.jsonl"
        } | ConvertTo-Json
        Set-Content -Path (Join-Path $runDir "meta.json") -Value $meta
        Add-Content -Path $listFile -Value "$tag`t$harnessName`t$code"
        Stop-PurgatoryProcs
    }
}

$env:PURGATORY_LOAD_PLACEMENT = $null
$env:PURGATORY_ADMISSION_CAP = $null

python (Join-Path $Root "tools\analyze_phase6d_evidence.py") `
    --root $OutRoot `
    --logs (Join-Path $Root "logs\load") `
    --docs (Join-Path $Root "docs")

Write-Host "DONE out=$OutRoot"
Write-Output $OutRoot
