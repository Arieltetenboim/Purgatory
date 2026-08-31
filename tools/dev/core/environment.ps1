# Developer Tools environment: constants, paths, identity.
# Dot-sourced. Uses $script:Root / $script:DevRoot from the entry script.

$script:ServerPackage = "purgatory-server"
$script:ClientPackage = "purgatory-client"
$script:LoadPackage = "purgatory-bot-client"
$script:ListenHost = "127.0.0.1"
$script:ListenPort = 5001
$script:MetricsPort = 5002
$script:LoadAdmissionCap = 256
$script:WindowTitle = "PURGATORY DEVELOPER TOOLS"
$script:MutexName = "Local\PurgatoryDevLauncher"
$script:ProbeLogin = "dev.probe"
$script:ReadyTimeoutSec = 45
$script:RecoveryIntervalSec = 5
$script:ClientStaggerMs = 140
$script:Dot = [string][char]0x25CF

function Get-TargetPrefix {
    return (Join-Path $script:Root "target")
}

function Get-ProfileDir {
    return (Join-Path (Get-TargetPrefix) $script:BuildProfile)
}

function Get-ServerExe {
    return (Join-Path (Get-ProfileDir) "purgatory-server.exe")
}

function Get-ClientExe {
    return (Join-Path (Get-ProfileDir) "purgatory-client.exe")
}

function Get-LoadExe {
    return (Join-Path (Get-ProfileDir) "purgatory-load.exe")
}

function Get-DevLogDir {
    return (Join-Path $script:Root "logs\dev-tools")
}

function Get-WorkspaceVersion {
    $tomlPath = Join-Path $script:Root "Cargo.toml"
    if (-not (Test-Path -LiteralPath $tomlPath)) { return "0.0.0" }
    $toml = Get-Content -LiteralPath $tomlPath -Raw
    if ($toml -match '(?s)\[workspace\.package\].*?version\s*=\s*"([^"]+)"') {
        return $Matches[1]
    }
    return "0.0.0"
}

function Get-Phase {
    $path = Join-Path $script:Root "PHASE"
    if (-not (Test-Path -LiteralPath $path)) { return "?" }
    return ((Get-Content -LiteralPath $path -Raw).Trim())
}

function Get-GitStamp {
    $git = Get-Command "git" -ErrorAction SilentlyContinue
    if (-not $git) { return "" }
    try {
        $hash = (& $git.Source -C $script:Root rev-parse --short HEAD 2>$null)
        if (-not $hash) { return "" }
        $hash = "$hash".Trim()
        $dirty = (& $git.Source -C $script:Root status --porcelain 2>$null)
        if ($dirty) { return "$hash*" }
        return $hash
    }
    catch {
        return ""
    }
}

function Get-ChildEnvironment {
    $map = @{
        RUST_BACKTRACE = "1"
    }
    if ($script:RustLog) { $map["RUST_LOG"] = $script:RustLog }
    if ($script:NetLog) { $map["PURGATORY_NET_LOG"] = "1" }
    if ($script:NetVerbose) { $map["PURGATORY_NET_VERBOSE"] = "1" }
    return $map
}

function Get-CargoArgumentList {
    param([string[]]$Packages)

    $list = New-Object System.Collections.Generic.List[string]
    [void]$list.Add("build")
    if ($script:BuildProfile -eq "release") {
        [void]$list.Add("--release")
    }
    foreach ($pkg in $Packages) {
        [void]$list.Add("-p")
        [void]$list.Add($pkg)
        if ($pkg -eq $script:LoadPackage) {
            [void]$list.Add("--bin")
            [void]$list.Add("purgatory-load")
        }
    }
    return $list.ToArray()
}

function Format-ExeIdentity {
    param([string]$ExePath)

    if (-not (Test-Path -LiteralPath $ExePath)) {
        return "EXE=MISSING $ExePath"
    }
    $item = Get-Item -LiteralPath $ExePath
    return ("EXE={0} LASTWRITE={1:yyyy-MM-dd HH:mm:ss} SIZE={2}" -f $item.FullName, $item.LastWriteTime, $item.Length)
}

function Initialize-DevEnvironment {
    $script:BuildProfile = "debug"
    $script:RustLog = ""
    $script:NetLog = $false
    $script:NetVerbose = $false
    $script:CargoPath = $null
    $script:CargoVersion = "cargo not found"

    $cargoCmd = Get-Command "cargo" -ErrorAction SilentlyContinue
    if ($cargoCmd) { $script:CargoPath = $cargoCmd.Source }
    if ($script:CargoPath) {
        try {
            $script:CargoVersion = (& $script:CargoPath --version 2>$null | Select-Object -First 1)
            if (-not $script:CargoVersion) { $script:CargoVersion = "cargo" }
        }
        catch { $script:CargoVersion = "cargo" }
    }

    $script:AppVersion = Get-WorkspaceVersion
    $script:AppPhase = Get-Phase
    $script:GitStamp = Get-GitStamp
    $script:CodeIdentity = "v$($script:AppVersion) - Phase $($script:AppPhase)"
    if ($script:GitStamp) {
        $script:CodeIdentity = "$($script:CodeIdentity) - $($script:GitStamp)"
    }

    $dir = Get-DevLogDir
    if (-not (Test-Path -LiteralPath $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }

    # DEV.BAT starts a hidden console host. VisibleConsole children would inherit it
    # and every writeln (dashboard / live status) can ding that hidden console.
    try { [void][PurgatoryNative]::FreeConsole() } catch { }
}
