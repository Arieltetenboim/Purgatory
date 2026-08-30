# Visible quality gate. Live cargo output is the point of the console.

function Start-QualityGate {
    if (-not $script:CargoPath) {
        Write-LaunchLog "cargo is not on PATH"
        return
    }

    $gate = Join-Path (Join-Path $script:Root "scripts") "check.ps1"
    try {
        [void](Start-OwnedProcess `
                -FilePath "powershell.exe" `
                -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-NoExit", "-File", $gate) `
                -VisibleConsole)
    }
    catch {
        Write-LaunchLog "Quality gate failed to start: $($_.Exception.Message)"
        return
    }
    Write-LaunchLog "Quality gate started"
}
