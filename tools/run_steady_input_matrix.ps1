# Phase 5.7 steady-state input matrix runner (localhost).
# Requires release binaries built. Restarts server in load mode between runs.

param(
    [ValidateSet("before", "after")]
    [string]$Tag = "before",
    [int[]]$Counts = @(100, 150, 200, 256),
    [string]$Profile = "mixed",
    [string]$Seed = "4242",
    [string]$Duration = "180s",
    [string]$Quiet = "15s",
    [int]$RampMs = 100,
    [int]$Admission = 256
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $Root "Cargo.toml"))) {
    $Root = (Get-Location).Path
}
Set-Location $Root

$ServerExe = Join-Path $Root "target\release\purgatory-server.exe"
$LoadExe = Join-Path $Root "target\release\purgatory-load.exe"
if (-not (Test-Path $ServerExe)) { throw "missing $ServerExe" }
if (-not (Test-Path $LoadExe)) { throw "missing $LoadExe" }

function Stop-PurgatoryProcs {
    Get-Process -Name "purgatory-server","purgatory-load" -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
}

function Start-LoadServer {
    Stop-PurgatoryProcs
    $env:PURGATORY_ADMISSION_CAP = "$Admission"
    $env:PURGATORY_METRICS_PORT = "5002"
    $logDir = Join-Path $Root "logs\load"
    New-Item -ItemType Directory -Force -Path $logDir | Out-Null
    $outLog = Join-Path $logDir "server_${Tag}_out.log"
    $errLog = Join-Path $logDir "server_${Tag}_err.log"
    $proc = Start-Process -FilePath $ServerExe -WorkingDirectory $Root `
        -RedirectStandardOutput $outLog -RedirectStandardError $errLog `
        -PassThru -WindowStyle Hidden
    # Wait for metrics port
    $deadline = (Get-Date).AddSeconds(30)
    $ok = $false
    while ((Get-Date) -lt $deadline) {
        try {
            $udp = New-Object System.Net.Sockets.UdpClient
            $udp.Client.ReceiveTimeout = 500
            $req = [byte[]](0x50,0x55,0x52,0x47,0x53,0x54,0x41,0x54,0x01) # PURGSTAT + v1
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
    if (-not $ok) { throw "server metrics not ready (pid=$($proc.Id))" }
    return $proc
}

$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$listFile = Join-Path $Root "logs\load\steady_${Tag}_runs.txt"
New-Item -ItemType Directory -Force -Path (Join-Path $Root "logs\load") | Out-Null
"" | Set-Content -Path $listFile

Write-Host "=== Steady input matrix tag=$Tag counts=$($Counts -join ',') ==="

foreach ($count in $Counts) {
    Write-Host "`n--- $Tag count=$count ---"
    $null = Start-LoadServer
    $before = Get-ChildItem (Join-Path $Root "logs\load") -Directory |
        Where-Object { $_.Name -match "^\d{8}_\d{6}_${count}bots_" } |
        Sort-Object Name

    & $LoadExe `
        --count $count `
        --profile $Profile `
        --scenario steady `
        --duration $Duration `
        --quiet-secs $Quiet `
        --ramp-ms $RampMs `
        --seed $Seed `
        --max-bots $Admission `
        --allow-high-count `
        --server 127.0.0.1:5001 `
        --metrics 127.0.0.1:5002
    $code = $LASTEXITCODE
    Write-Host "harness exit=$code"

    $after = Get-ChildItem (Join-Path $Root "logs\load") -Directory |
        Where-Object { $_.Name -match "^\d{8}_\d{6}_${count}bots_" } |
        Sort-Object Name
    $new = $after | Where-Object { $before.Name -notcontains $_.Name } | Select-Object -Last 1
    if ($null -eq $new) {
        $new = Get-Content (Join-Path $Root "logs\load\latest.txt") -ErrorAction SilentlyContinue
        if ($new) { Add-Content -Path $listFile -Value $new.Trim() }
    } else {
        Add-Content -Path $listFile -Value $new.Name
    }
    Stop-PurgatoryProcs
}

$out = Join-Path $Root "logs\load\capacity\${stamp}\steady_input_$Tag"
New-Item -ItemType Directory -Force -Path $out | Out-Null
python (Join-Path $Root "tools\analyze_steady_input.py") `
    --runs-file $listFile `
    --logs (Join-Path $Root "logs\load") `
    --out $out `
    --tag $Tag

Write-Host "DONE tag=$Tag out=$out list=$listFile"
Write-Output $out
