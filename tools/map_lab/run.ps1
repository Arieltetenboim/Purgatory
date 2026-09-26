$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location -LiteralPath $RepoRoot

Write-Host "MAP_LAB|LAUNCH|cargo run -p purgatory-map-lab"
cargo run -p purgatory-map-lab
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
