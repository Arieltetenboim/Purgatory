# Extended PURGATORY network soak (Phase 5.0F).
#
# Runs the `#[ignore]` network soak tests only. These are longer than the
# normal quality gate (`scripts/check.ps1`), which stays fast for routine use.
#
# Localhost only: no internet, no external services, no project state changes.
# A pass here proves convergence and boundedness, NOT player capacity.
#
# Requires: cargo. Fails on the first failing suite.

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $ProjectRoot

$cargo = Get-Command cargo -ErrorAction Stop
$packages = @("purgatory-server", "purgatory-client")
$timings = [ordered]@{}

foreach ($package in $packages) {
    $arguments = @("test", "-p", $package, "--", "--ignored", "--test-threads=1")
    Write-Host ">> cargo $($arguments -join ' ')"
    $started = Get-Date
    & $cargo.Source @arguments | Out-Host
    if ($LASTEXITCODE -ne 0) {
        Write-Host "SOAK FAILED: $package"
        exit $LASTEXITCODE
    }
    $timings[$package] = [math]::Round(((Get-Date) - $started).TotalSeconds, 1)
}

Write-Host ""
Write-Host "PURGATORY extended network soak summary"
foreach ($package in $packages) {
    Write-Host ("  {0}: {1}s" -f $package, $timings[$package])
}
Write-Host "PURGATORY extended network soak OK"
