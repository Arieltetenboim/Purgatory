param(
    [int]$Port = 8765
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$Server = Join-Path $PSScriptRoot "server_n6.py"
$Url = "http://127.0.0.1:$Port/"
$HealthUrl = "${Url}api/health"

try {
    $health = Invoke-RestMethod -Uri $HealthUrl -Method Get -TimeoutSec 1
    if ($health.ok -and $health.slice -eq "N6a-preview") {
        Start-Process $Url | Out-Null
        exit 0
    }
    if ($health.ok) {
        throw "An older NPC Lab is already running on port $Port. Stop that terminal/server, then launch NPC Lab again."
    }
}
catch {
    if ($_.Exception.Message -like "An older NPC Lab*") {
        throw
    }
    # No running NPC Lab on this port; start N6a below.
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
    throw "NPC Lab requires Python 3 on PATH (py -3 or python)."
}

$pythonArgs += @(
    $Server,
    "--root", $RepoRoot,
    "--port", [string]$Port,
    "--open"
)

& $pythonExe @pythonArgs
exit $LASTEXITCODE
