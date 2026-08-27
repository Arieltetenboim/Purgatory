# Requires: cargo, rustfmt, clippy
# Fails on the first failing command.

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $ProjectRoot

function Invoke-RequiredCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,
        [Parameter(Mandatory = $true)]
        [string[]]$ArgumentList
    )

    Write-Host ">> $FilePath $($ArgumentList -join ' ')"
    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

$cargo = Get-Command cargo -ErrorAction Stop

Invoke-RequiredCommand -FilePath $cargo.Source -ArgumentList @("fmt", "--all", "--", "--check")
Invoke-RequiredCommand -FilePath $cargo.Source -ArgumentList @("check", "--workspace")
Invoke-RequiredCommand -FilePath $cargo.Source -ArgumentList @("clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings")
Invoke-RequiredCommand -FilePath $cargo.Source -ArgumentList @("test", "--workspace")

Write-Host "PURGATORY quality gate OK"
