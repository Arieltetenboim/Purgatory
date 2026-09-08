param(
    [string]$RepoRoot = (Get-Location).Path
)

$ErrorActionPreference = "Stop"

function Fail([string]$Message) {
    Write-Host ""
    Write-Host "NPC LAB N1 DIAGNOSTIC FIX FAILED: $Message" -ForegroundColor Red
    exit 1
}

$RepoRoot = [IO.Path]::GetFullPath($RepoRoot)
if (-not (Test-Path (Join-Path $RepoRoot ".git"))) {
    Fail "Run this script from the PURGATORY repository root, or pass -RepoRoot."
}

$utf8NoBom = New-Object Text.UTF8Encoding($false)

# 1) Revert Python isolated mode. It is not required by NPC Lab.
$runPath = Join-Path $RepoRoot "tools\npc_lab\run.ps1"
if (-not (Test-Path $runPath)) {
    Fail "Missing tools/npc_lab/run.ps1"
}

$run = [IO.File]::ReadAllText($runPath)

if ($run.Contains('$pythonArgs = @("-3", "-I")')) {
    $run = $run.Replace('$pythonArgs = @("-3", "-I")', '$pythonArgs = @("-3")')
    Write-Host "PATCH  tools/npc_lab/run.ps1: removed py -I"
}

$pythonIsolatedBranch = @'
elseif (Get-Command python -ErrorAction SilentlyContinue) {
    $pythonExe = "python"
    $pythonArgs = @("-I")
}
'@
$pythonNormalBranch = @'
elseif (Get-Command python -ErrorAction SilentlyContinue) {
    $pythonExe = "python"
}
'@

if ($run.Contains($pythonIsolatedBranch)) {
    $run = $run.Replace($pythonIsolatedBranch, $pythonNormalBranch)
    Write-Host "PATCH  tools/npc_lab/run.ps1: removed python -I"
}

[IO.File]::WriteAllText($runPath, $run, $utf8NoBom)

# 2) Make the real Developer Hub launch NPC Lab visibly during N1.
#    -NoExit keeps the shell open if Python exits with an error.
$contentPath = Join-Path $RepoRoot "apps\dev_hub\src\ui\content.rs"
if (-not (Test-Path $contentPath)) {
    Fail "Missing apps/dev_hub/src/ui/content.rs"
}

$content = [IO.File]::ReadAllText($contentPath)

$oldArgs = @'
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-File",
            ])
'@

$newArgs = @'
            .args([
                "-NoProfile",
                "-NoExit",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
'@

if ($content.Contains($oldArgs)) {
    $content = $content.Replace($oldArgs, $newArgs)
    Write-Host "PATCH  Developer Hub: visible NPC Lab shell + -NoExit"
}
elseif ($content.Contains('"-NoExit",')) {
    Write-Host "SKIP   Developer Hub already uses visible diagnostic shell"
}
else {
    Fail "Could not find expected NPC Lab PowerShell launch arguments in content.rs"
}

[IO.File]::WriteAllText($contentPath, $content, $utf8NoBom)

Write-Host ""
Write-Host "Diagnostic fix applied locally. Nothing was pushed." -ForegroundColor Green
Write-Host ""
Write-Host "Run:"
Write-Host "  cargo check -p purgatory-dev-hub"
Write-Host "  .\DEV_HUB.BAT"
Write-Host ""
Write-Host "Then Content -> NPC Lab -> Launch NPC Lab."
Write-Host "If Python still exits, the PowerShell window will stay open and show the exact error."
