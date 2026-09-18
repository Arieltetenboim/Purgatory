# Requires: cargo, rustfmt, clippy
# Fails on the first failing command.
# HUB_GATE markers are a small stable contract consumed by Developer Hub UI.

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $ProjectRoot

function Invoke-GateStep {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Id,
        [Parameter(Mandatory = $true)]
        [string]$Label,
        [Parameter(Mandatory = $true)]
        [string]$FilePath,
        [Parameter(Mandatory = $true)]
        [string[]]$ArgumentList
    )

    Write-Host "HUB_GATE|START|$Id|$Label"
    Write-Host ">> $FilePath $($ArgumentList -join ' ')"
    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        Write-Host "HUB_GATE|FAIL|$Id|$Label|exit=$LASTEXITCODE"
        exit $LASTEXITCODE
    }
    Write-Host "HUB_GATE|PASS|$Id|$Label"
}

$cargo = Get-Command cargo -ErrorAction Stop
$python = Get-Command py -ErrorAction SilentlyContinue
$pythonArgs = @("-3")
if ($null -eq $python) {
    $python = Get-Command python -ErrorAction Stop
    $pythonArgs = @()
}

Invoke-GateStep -Id "fmt" -Label "Format" -FilePath $cargo.Source -ArgumentList @("fmt", "--all", "--", "--check")
Invoke-GateStep -Id "check" -Label "Cargo Check" -FilePath $cargo.Source -ArgumentList @("check", "--workspace")
Invoke-GateStep -Id "clippy" -Label "Clippy" -FilePath $cargo.Source -ArgumentList @("clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings")
Invoke-GateStep -Id "tests" -Label "Workspace Tests" -FilePath $cargo.Source -ArgumentList @("test", "--workspace")
Invoke-GateStep -Id "mob-lab" -Label "Mob Lab Tests" -FilePath $python.Source -ArgumentList ($pythonArgs + @("-m", "unittest", "discover", "-s", ".\tools\mob_lab", "-p", "test_*.py"))
Invoke-GateStep -Id "content" -Label "Content Validation" -FilePath $cargo.Source -ArgumentList @("run", "-p", "purgatory-content-validator", "-q")

Write-Host "HUB_GATE|DONE|quality|Quality Gate"
Write-Host "PURGATORY quality gate OK"
