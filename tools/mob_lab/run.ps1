param(
    [int]$Port = 8766
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$Server = Join-Path $PSScriptRoot "server.py"
$Url = "http://127.0.0.1:$Port/"
$HealthUrl = "${Url}api/health"

$ExpectedBuild = "m3-prototype-parity-v9"

try {
    $health = Invoke-RestMethod -Uri $HealthUrl -Method Get -TimeoutSec 1
    if ($health.ok -and $health.tool -eq "mob-lab" -and $health.build -eq $ExpectedBuild) {
        Start-Process "$Url?build=$ExpectedBuild" | Out-Null
        exit 0
    }

    if ($health.ok -and $health.tool -eq "mob-lab") {
        Write-Host "MOB_LAB|RESTART|stale server detected"
        $connection = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($null -ne $connection) {
            Stop-Process -Id $connection.OwningProcess -Force -ErrorAction Stop
            Start-Sleep -Milliseconds 250
        }
    }
    elseif ($health.ok) {
        throw "Another local tool is already running on port $Port."
    }
}
catch {
    if ($_.Exception.Message -like "Another local tool*") { throw }
}

$pythonExe = $null
$pythonArgs = @()
if (Get-Command py -ErrorAction SilentlyContinue) {
    $pythonExe = "py"
    $pythonArgs = @("-3")
}
elseif (Get-Command python -ErrorAction SilentlyContinue) {
    $pythonExe = "python"
}
else {
    throw "Mob Lab requires Python 3 on PATH (py -3 or python)."
}

$pythonArgs += @(
    $Server,
    "--root", $RepoRoot,
    "--port", [string]$Port,
    "--open"
)

& $pythonExe @pythonArgs
exit $LASTEXITCODE
