param(
    [int]$Port = 8767,
    [switch]$NoBrowser
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$Server = Join-Path $PSScriptRoot "server.py"
$Url = "http://127.0.0.1:$Port/"
$HealthUrl = "${Url}api/health"
$ExpectedBuild = "item-lab-v1"

try {
    $Host.UI.RawUI.WindowTitle = "PURGATORY Item Lab - $ExpectedBuild - close window to stop server"
} catch {
}

Write-Host "ITEM_LAB|LAUNCH|build=$ExpectedBuild"
Write-Host "ITEM_LAB|LIFETIME|Close this PowerShell window or press Ctrl+C to stop the server."

try {
    $health = Invoke-RestMethod -Uri $HealthUrl -Method Get -TimeoutSec 1
    if ($health.tool -eq "item-lab" -and $health.build -eq $ExpectedBuild) {
        if (-not $NoBrowser) {
            Start-Process $Url | Out-Null
        }
        exit 0
    }
    throw "Port $Port is already in use by another process. Item Lab will not stop it."
}
catch {
    if ($_.Exception.Message -like "Port $Port is already*") { throw }
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
    throw "Item Lab requires Python 3 on PATH (py -3 or python)."
}

$pythonArgs += @($Server, "--port", [string]$Port)
if (-not $NoBrowser) { $pythonArgs += "--open" }
& $pythonExe @pythonArgs
exit $LASTEXITCODE
