# Compatibility stub. The launcher lives at tools/dev/dev_launcher.ps1
$target = Join-Path $PSScriptRoot "dev\dev_launcher.ps1"
if ([Threading.Thread]::CurrentThread.GetApartmentState() -ne "STA") {
    $arg = @(
        "-NoProfile", "-STA", "-ExecutionPolicy", "Bypass",
        "-WindowStyle", "Hidden", "-File", $target
    )
    Start-Process -FilePath "powershell.exe" -ArgumentList $arg | Out-Null
    exit 0
}
& $target
if ($LASTEXITCODE) { exit $LASTEXITCODE }
