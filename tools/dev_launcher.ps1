# PURGATORY development launcher.
# Opened by DEV.BAT. Tracks real processes from this workspace, not guessed flags.

Set-StrictMode -Version 1
$ErrorActionPreference = "Continue"

if ([Threading.Thread]::CurrentThread.GetApartmentState() -ne "STA") {
    $arg = @(
        "-NoProfile", "-STA", "-ExecutionPolicy", "Bypass",
        "-WindowStyle", "Hidden", "-File", $PSCommandPath
    )
    Start-Process -FilePath "powershell.exe" -ArgumentList $arg | Out-Null
    exit 0
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[Windows.Forms.Application]::EnableVisualStyles()

if (-not ("PurgatoryNative" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class PurgatoryNative {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
    [DllImport("shcore.dll")] public static extern int SetProcessDpiAwareness(int value);
}
"@
}

try { [PurgatoryNative]::SetProcessDpiAwareness(2) | Out-Null } catch { }

$Root = Split-Path -Parent $PSScriptRoot
$ServerPackage = "purgatory-server"
$ClientPackage = "purgatory-client"
$TerminalWindow = "PURGATORY"
$ListenHost = "127.0.0.1"
$ListenPort = 5001
$WindowTitle = "PURGATORY DEV LAUNCHER"

$script:BuildProfile = "debug"
$script:RustLog = ""
$script:NetLog = $false
$script:NetVerbose = $false
$script:ClientSerial = 0
$script:PendingClients = 0
$script:PendingServerStart = $false
$script:WasServerAlive = $false
$script:LastClientLive = 0
$script:CargoPath = $null
$script:WtPath = $null
$script:Mutex = $null
$script:LogoImage = $null
$script:ServerRunning = $false
$script:ServerBuilding = $false
$script:LaunchHoldUntil = [datetime]::MinValue
$script:Dot = [string][char]0x25CF

# ------------------------------------------------------------
# Single instance
# ------------------------------------------------------------

$script:Mutex = New-Object Threading.Mutex($false, "Local\PurgatoryDevLauncher")
$owned = $false
try {
    $owned = $script:Mutex.WaitOne(0, $false)
}
catch [Threading.AbandonedMutexException] {
    $owned = $true
}

if (-not $owned) {
    $other = Get-Process | Where-Object { $_.MainWindowTitle -eq $WindowTitle } | Select-Object -First 1
    if ($other -and $other.MainWindowHandle -ne [IntPtr]::Zero) {
        if ([PurgatoryNative]::IsIconic($other.MainWindowHandle)) {
            [PurgatoryNative]::ShowWindow($other.MainWindowHandle, 9) | Out-Null
        }
        [PurgatoryNative]::SetForegroundWindow($other.MainWindowHandle) | Out-Null
    }
    else {
        [Windows.Forms.MessageBox]::Show(
            "The launcher is already running, but its window could not be found.",
            $WindowTitle,
            [Windows.Forms.MessageBoxButtons]::OK,
            [Windows.Forms.MessageBoxIcon]::Information
        ) | Out-Null
    }
    exit 0
}

# ------------------------------------------------------------
# Colors (saturation -20%)
# ------------------------------------------------------------

function New-DesaturatedColor {
    param(
        [int]$R,
        [int]$G,
        [int]$B,
        [double]$Amount = 0.20
    )

    $r0 = $R / 255.0
    $g0 = $G / 255.0
    $b0 = $B / 255.0
    $max = [Math]::Max($r0, [Math]::Max($g0, $b0))
    $min = [Math]::Min($r0, [Math]::Min($g0, $b0))
    $l = ($max + $min) / 2.0
    $d = $max - $min
    if ($d -lt 1e-6) {
        return [Drawing.Color]::FromArgb($R, $G, $B)
    }

    $s = $d / (1.0 - [Math]::Abs((2.0 * $l) - 1.0))
    $s = $s * (1.0 - $Amount)

    if ($max -eq $r0) {
        $h = (($g0 - $b0) / $d) % 6.0
        if ($h -lt 0) { $h += 6.0 }
    }
    elseif ($max -eq $g0) {
        $h = (($b0 - $r0) / $d) + 2.0
    }
    else {
        $h = (($r0 - $g0) / $d) + 4.0
    }

    $c = (1.0 - [Math]::Abs((2.0 * $l) - 1.0)) * $s
    $x = $c * (1.0 - [Math]::Abs(($h % 2.0) - 1.0))
    $m = $l - ($c / 2.0)
    $rp = 0.0
    $gp = 0.0
    $bp = 0.0
    if ($h -lt 1) { $rp = $c; $gp = $x }
    elseif ($h -lt 2) { $rp = $x; $gp = $c }
    elseif ($h -lt 3) { $gp = $c; $bp = $x }
    elseif ($h -lt 4) { $gp = $x; $bp = $c }
    elseif ($h -lt 5) { $rp = $x; $bp = $c }
    else { $rp = $c; $bp = $x }

    $to8 = {
        param($V)
        [int][Math]::Max(0, [Math]::Min(255, [Math]::Round($V * 255.0)))
    }
    return [Drawing.Color]::FromArgb((& $to8 ($rp + $m)), (& $to8 ($gp + $m)), (& $to8 ($bp + $m)))
}

$Bg        = New-DesaturatedColor 12 10 9
$Panel     = New-DesaturatedColor 26 22 20
$PanelHi   = New-DesaturatedColor 36 30 26
$Border    = New-DesaturatedColor 74 50 36
$Text      = New-DesaturatedColor 242 232 220
$Muted     = New-DesaturatedColor 160 144 128
$Accent    = New-DesaturatedColor 232 120 48
$Green     = New-DesaturatedColor 95 212 138
$Red       = New-DesaturatedColor 232 90 76
$Yellow    = New-DesaturatedColor 232 184 74
$LogColor  = New-DesaturatedColor 196 176 156
$Disabled  = [Drawing.Color]::FromArgb(156, 156, 156)
$BtnPress  = [Drawing.Color]::FromArgb(48, 38, 30)

# ------------------------------------------------------------
# Environment
# ------------------------------------------------------------

function Find-Wt {
    $cmd = Get-Command "wt.exe" -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $store = Join-Path $env:LOCALAPPDATA "Microsoft\WindowsApps\wt.exe"
    if (Test-Path -LiteralPath $store) { return $store }
    return $null
}

$cargoCmd = Get-Command "cargo" -ErrorAction SilentlyContinue
if ($cargoCmd) { $script:CargoPath = $cargoCmd.Source }
$script:WtPath = Find-Wt

$script:CargoVersion = "cargo not found"
if ($script:CargoPath) {
    try {
        $script:CargoVersion = (& $script:CargoPath --version 2>$null | Select-Object -First 1)
        if (-not $script:CargoVersion) { $script:CargoVersion = "cargo" }
    }
    catch { $script:CargoVersion = "cargo" }
}

function Get-WorkspaceVersion {
    $tomlPath = Join-Path $Root "Cargo.toml"
    if (-not (Test-Path -LiteralPath $tomlPath)) { return "0.0.0" }
    $toml = Get-Content -LiteralPath $tomlPath -Raw
    if ($toml -match '(?s)\[workspace\.package\].*?version\s*=\s*"([^"]+)"') {
        return $Matches[1]
    }
    return "0.0.0"
}

function Get-Phase {
    $path = Join-Path $Root "PHASE"
    if (-not (Test-Path -LiteralPath $path)) { return "?" }
    return ((Get-Content -LiteralPath $path -Raw).Trim())
}

function Get-GitStamp {
    $git = Get-Command "git" -ErrorAction SilentlyContinue
    if (-not $git) { return "" }
    try {
        $hash = (& $git.Source -C $Root rev-parse --short HEAD 2>$null)
        if (-not $hash) { return "" }
        $hash = "$hash".Trim()
        $dirty = (& $git.Source -C $Root status --porcelain 2>$null)
        if ($dirty) { return "$hash*" }
        return $hash
    }
    catch {
        return ""
    }
}

$script:AppVersion = Get-WorkspaceVersion
$script:AppPhase = Get-Phase
$script:GitStamp = Get-GitStamp
$script:CodeIdentity = "v$($script:AppVersion) - Phase $($script:AppPhase)"
if ($script:GitStamp) {
    $script:CodeIdentity = "$($script:CodeIdentity) - $($script:GitStamp)"
}

# ------------------------------------------------------------
# Process helpers
# ------------------------------------------------------------

function Get-TargetPrefix {
    return (Join-Path $Root "target")
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

function Get-OwnedProcesses {
    param([string]$Name)

    $exeName = $Name
    if ($exeName -notlike "*.exe") { $exeName = "$exeName.exe" }
    $prefix = Get-TargetPrefix
    $rows = @(Get-CimInstance Win32_Process -Filter "Name = '$exeName'" -ErrorAction SilentlyContinue)
    foreach ($row in $rows) {
        $path = $row.ExecutablePath
        if ([string]::IsNullOrEmpty($path)) { continue }
        if (-not $path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { continue }
        [pscustomobject]@{
            Id              = [int]$row.ProcessId
            ParentProcessId = [int]$row.ParentProcessId
            Path            = $path
        }
    }
}

function Test-PidAlive {
    param([int]$ProcessId)

    if ($ProcessId -le 4) { return $false }
    $null -ne (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)
}

function Stop-OrphanedServers {
    foreach ($proc in @(Get-OwnedProcesses -Name "purgatory-server")) {
        if (Test-PidAlive $proc.ParentProcessId) { continue }
        Write-LaunchLog "Server console closed; stopping pid $($proc.Id)"
        Stop-OwnedTree -ProcessId $proc.Id
    }
}

function Test-CargoBuilding {
    param([string]$Package)

    if (-not (Get-Process -Name "cargo" -ErrorAction SilentlyContinue)) {
        return $false
    }

    $procs = Get-CimInstance Win32_Process -Filter "Name = 'cargo.exe'" -ErrorAction SilentlyContinue
    foreach ($proc in $procs) {
        $line = $proc.CommandLine
        if ([string]::IsNullOrEmpty($line)) { continue }
        if ($line -notlike "*$Root*") { continue }
        if ($line -notmatch '\bbuild\b') { continue }
        if ($Package -eq "*") { return $true }
        if ($line -like "*-p $Package*" -or $line -like "*--package $Package*") { return $true }
        if ($line -notlike "*-p *" -and $line -notlike "*--package *") { return $true }
    }
    return $false
}

function Test-PortOpen {
    param([int[]]$OwnerPids = @())

    try {
        $eps = @(Get-NetUDPEndpoint -LocalPort $ListenPort -ErrorAction SilentlyContinue)
        if ($eps.Count -eq 0) { return $false }
        if ($OwnerPids.Count -eq 0) { return $false }
        foreach ($ep in $eps) {
            if ($OwnerPids -contains [int]$ep.OwningProcess) { return $true }
        }
        return $false
    }
    catch {
        if ($OwnerPids.Count -eq 0) { return $false }
        try {
            $props = [Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties()
            foreach ($ep in $props.GetActiveUdpListeners()) {
                if ($ep.Port -eq $ListenPort) { return $true }
            }
        }
        catch { }
        return $false
    }
}

function Stop-OwnedTree {
    param([Parameter(Mandatory = $true)][int]$ProcessId)

    $kill = Start-Process -FilePath "taskkill.exe" -ArgumentList @("/PID", "$ProcessId", "/T", "/F") -Wait -PassThru -WindowStyle Hidden -ErrorAction SilentlyContinue
    if ($kill -and $kill.ExitCode -eq 0) { return }
    Stop-Process -Id $ProcessId -Force -ErrorAction SilentlyContinue
}

function Stop-OwnedByName {
    param([string]$Name)

    $killed = 0
    foreach ($proc in @(Get-OwnedProcesses -Name $Name)) {
        Stop-OwnedTree -ProcessId $proc.Id
        $killed++
    }
    return $killed
}

function Stop-WorkspaceCargo {
    param([string]$Package)

    if (-not (Get-Process -Name "cargo" -ErrorAction SilentlyContinue)) {
        return 0
    }

    $killed = 0
    $procs = Get-CimInstance Win32_Process -Filter "Name = 'cargo.exe'" -ErrorAction SilentlyContinue
    foreach ($proc in $procs) {
        $line = $proc.CommandLine
        if ([string]::IsNullOrEmpty($line)) { continue }
        if ($line -notlike "*$Root*") { continue }
        if ($Package -ne "*" -and $line -notlike "*-p $Package*" -and $line -notlike "*--package $Package*") {
            continue
        }
        Stop-OwnedTree -ProcessId ([int]$proc.ProcessId)
        $killed++
    }
    return $killed
}

function Wait-OwnedGone {
    param([string]$Name, [int]$TimeoutMs = 5000)

    $deadline = [datetime]::UtcNow.AddMilliseconds($TimeoutMs)
    while ([datetime]::UtcNow -lt $deadline) {
        if ((@(Get-OwnedProcesses -Name $Name)).Count -eq 0) { return $true }
        Start-Sleep -Milliseconds 80
        [Windows.Forms.Application]::DoEvents()
    }
    return ((@(Get-OwnedProcesses -Name $Name)).Count -eq 0)
}

function Get-CargoBuildArgs {
    param([string]$Package)

    if ($script:BuildProfile -eq "release") {
        return "cargo build --release -p $Package"
    }
    return "cargo build -p $Package"
}

function Get-LaunchCmdDir {
    $dir = Join-Path $env:TEMP "purgatory-dev-launcher"
    if (-not (Test-Path -LiteralPath $dir)) {
        New-Item -ItemType Directory -Path $dir | Out-Null
    }
    return $dir
}

function Write-LaunchCmd {
    param(
        [string]$FileName,
        [string[]]$Body
    )

    $path = Join-Path (Get-LaunchCmdDir) $FileName
    $lines = New-Object System.Collections.Generic.List[string]
    [void]$lines.Add("@echo off")
    [void]$lines.Add("setlocal")
    [void]$lines.Add("cd /d `"$Root`"")
    [void]$lines.Add("set RUST_BACKTRACE=1")
    if ($script:RustLog) {
        [void]$lines.Add("set `"RUST_LOG=$($script:RustLog)`"")
    }
    if ($script:NetLog) {
        [void]$lines.Add("set PURGATORY_NET_LOG=1")
    }
    if ($script:NetVerbose) {
        [void]$lines.Add("set PURGATORY_NET_VERBOSE=1")
    }
    foreach ($line in $Body) {
        [void]$lines.Add($line)
    }
    [IO.File]::WriteAllLines($path, $lines.ToArray(), [Text.Encoding]::ASCII)
    return $path
}

function Invoke-TerminalTab {
    param(
        [string]$Title,
        [string]$ScriptPath
    )

    if ($script:WtPath) {
        $wtArgs = "-w `"$TerminalWindow`" new-tab --title `"$Title`" --startingDirectory `"$Root`" -- cmd.exe /k `"$ScriptPath`""
        Start-Process -FilePath $script:WtPath -ArgumentList $wtArgs | Out-Null
        return
    }

    Start-Process -FilePath "cmd.exe" -WorkingDirectory $Root -ArgumentList @("/k", $ScriptPath) | Out-Null
}

function Start-ExeTab {
    param(
        [string]$ExePath,
        [string]$Banner,
        [string]$Title,
        [string]$FileName
    )

    $body = New-Object System.Collections.Generic.List[string]
    [void]$body.Add("title $Title")
    [void]$body.Add("echo.")
    [void]$body.Add("echo   $Banner")
    [void]$body.Add("echo($($script:CodeIdentity)")
    [void]$body.Add("echo   ==============================")
    [void]$body.Add("echo.")
    if ($script:RustLog) {
        [void]$body.Add("echo RUST_LOG=$($script:RustLog)")
    }
    else {
        [void]$body.Add("echo RUST_LOG=default  info + quinn=warn")
    }
    if ($script:NetVerbose) {
        [void]$body.Add("echo PURGATORY_NET_VERBOSE=1  snapshots every tick")
    }
    elseif ($script:NetLog) {
        [void]$body.Add("echo PURGATORY_NET_LOG=1  connect / handshake lines")
    }
    [void]$body.Add("echo.")
    [void]$body.Add("echo Starting...")
    [void]$body.Add("`"$ExePath`"")
    [void]$body.Add("echo.")
    [void]$body.Add("echo Process exited %ERRORLEVEL%")

    $scriptPath = Write-LaunchCmd -FileName $FileName -Body $body.ToArray()
    Invoke-TerminalTab -Title $Title -ScriptPath $scriptPath
}

function Start-BuildOnlyTab {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Package
    )

    if (Test-CargoBuilding -Package $Package) {
        return
    }

    $build = Get-CargoBuildArgs -Package $Package
    $profileName = $script:BuildProfile
    $body = @(
        "title BUILD",
        "echo.",
        "echo   BUILD $Package $profileName",
        "echo($($script:CodeIdentity)",
        "echo   ==============================",
        "echo.",
        "echo $build",
        $build,
        "if errorlevel 1 goto fail",
        "echo.",
        "echo BUILD OK",
        "goto :eof",
        ":fail",
        "echo.",
        "echo BUILD FAILED",
        "pause",
        "exit /b 1"
    )
    $safeName = $Package.Replace("-", "_")
    $scriptPath = Write-LaunchCmd -FileName "build-$safeName.cmd" -Body $body
    Invoke-TerminalTab -Title "BUILD" -ScriptPath $scriptPath
    Write-LaunchLog "Building $Package ($profileName)"
}

# ------------------------------------------------------------
# Logging + status
# ------------------------------------------------------------

function Write-LaunchLog {
    param([string]$Message)

    if (-not $LogBox) { return }
    $line = "{0}  {1}" -f (Get-Date -Format "HH:mm:ss"), $Message
    $lines = $LogBox.Lines
    if ($lines -and $lines.Count -gt 180) {
        $keep = $lines | Select-Object -Last 120
        $LogBox.Lines = [string[]]$keep
    }
    $LogBox.AppendText($line + [Environment]::NewLine)
}

function Set-LabelIfChanged {
    param($Label, [string]$Value, $Color = $null)

    if ($Label.Text -ne $Value) { $Label.Text = $Value }
    if ($null -ne $Color -and $Label.ForeColor -ne $Color) { $Label.ForeColor = $Color }
}

function Update-LiveStatus {
    Stop-OrphanedServers
    $serverProcs = @(Get-OwnedProcesses -Name "purgatory-server")
    $clientProcs = @(Get-OwnedProcesses -Name "purgatory-client")
    $buildingServer = Test-CargoBuilding -Package $ServerPackage
    $buildingClient = Test-CargoBuilding -Package $ClientPackage
    $serverPids = @($serverProcs | ForEach-Object { $_.Id })
    $portOpen = Test-PortOpen -OwnerPids $serverPids
    $serverAlive = $serverProcs.Count -gt 0

    if (-not $serverAlive -and -not $buildingServer) {
        $script:LaunchHoldUntil = [datetime]::MinValue
    }

    $statusText = "STOPPED"
    $statusColor = $Red
    if ($buildingServer -and -not $serverAlive) {
        $statusText = "BUILDING"
        $statusColor = $Yellow
    }
    elseif ($serverAlive -and $portOpen) {
        $statusText = "RUNNING"
        $statusColor = $Green
    }
    elseif ($serverAlive) {
        $statusText = "STARTING"
        $statusColor = $Yellow
    }

    Set-LabelIfChanged -Label $ServerDot -Value $script:Dot -Color $statusColor
    Set-LabelIfChanged -Label $ServerStatus -Value $statusText -Color $statusColor

    $portColor = $Muted
    $portText = "UDP ${ListenPort}  closed"
    if ($portOpen) {
        $portColor = $Green
        $portText = "UDP ${ListenPort}  listening"
    }
    Set-LabelIfChanged -Label $PortStatus -Value $portText -Color $portColor

    $live = $clientProcs.Count
    Set-LabelIfChanged -Label $ClientCountLabel -Value "$live" -Color $Accent
    $clientSub = "live process"
    if ($live -ne 1) { $clientSub = "live processes" }
    if ($buildingClient) { $clientSub = "building + $clientSub" }
    Set-LabelIfChanged -Label $ClientSubLabel -Value $clientSub -Color $Muted

    $serverExe = Test-Path -LiteralPath (Get-ServerExe)
    $clientExe = Test-Path -LiteralPath (Get-ClientExe)
    $binBits = @()
    if ($serverExe) { $binBits += "server" } else { $binBits += "no server exe" }
    if ($clientExe) { $binBits += "client" } else { $binBits += "no client exe" }
    Set-LabelIfChanged -Label $BinaryStatus -Value ("{0}  |  {1}" -f $script:BuildProfile, ($binBits -join "  |  ")) -Color $Muted

    $held = [datetime]::UtcNow -lt $script:LaunchHoldUntil
    $canStart = [bool]$script:CargoPath -and -not $serverAlive -and -not $buildingServer -and -not $held
    $canStop = $serverAlive -or $buildingServer -or $held
    Set-ToggleButton -Button $btnStart -IsEnabled $canStart -ActiveColor $Green
    $btnRestart.Enabled = [bool]$script:CargoPath
    Set-ToggleButton -Button $btnStop -IsEnabled $canStop -ActiveColor $Red
    $btnStopClients.Enabled = $live -gt 0
    $openEnabled = [bool]$script:CargoPath
    $btnClient.Enabled = $openEnabled
    $btnClient2.Enabled = $openEnabled
    $btnClient3.Enabled = $openEnabled

    if ($script:WasServerAlive -and -not $serverAlive -and -not $buildingServer) {
        Write-LaunchLog "Server process exited"
    }
    if ($live -lt $script:LastClientLive) {
        $dropped = $script:LastClientLive - $live
        if ($dropped -eq 1) {
            Write-LaunchLog "Client closed"
        }
        else {
            Write-LaunchLog "$dropped clients closed"
        }
    }

    $script:WasServerAlive = $serverAlive
    $script:LastClientLive = $live
    $script:ServerRunning = $serverAlive
    $script:ServerBuilding = $buildingServer

    Pump-PendingClients
    Pump-PendingServer
}

# ------------------------------------------------------------
# Actions
# ------------------------------------------------------------

function Start-Server {
    param([switch]$Rebuild)

    if (-not $script:CargoPath) {
        Write-LaunchLog "cargo is not on PATH"
        [Windows.Forms.MessageBox]::Show(
            "cargo was not found on PATH.`nInstall Rust via rustup, then reopen the launcher.",
            $WindowTitle,
            [Windows.Forms.MessageBoxButtons]::OK,
            [Windows.Forms.MessageBoxIcon]::Warning
        ) | Out-Null
        return
    }

    if (-not $Rebuild -and [datetime]::UtcNow -lt $script:LaunchHoldUntil) {
        return
    }

    $alive = @(Get-OwnedProcesses -Name "purgatory-server")
    if ($alive.Count -gt 0) {
        Write-LaunchLog "Server already running"
        return
    }

    $script:PendingServerStart = $true
    $exe = Get-ServerExe
    if ($Rebuild -or -not (Test-Path -LiteralPath $exe)) {
        if ($Rebuild) {
            Write-LaunchLog "Rebuilding server then starting ($($script:BuildProfile))"
        }
        else {
            Write-LaunchLog "Building server then starting ($($script:BuildProfile))"
        }
        Start-BuildOnlyTab -Package $ServerPackage
        return
    }

    Start-ServerExe
}

function Start-ServerExe {
    $exe = Get-ServerExe
    if (-not (Test-Path -LiteralPath $exe)) { return }
    if ((@(Get-OwnedProcesses -Name "purgatory-server")).Count -gt 0) {
        $script:PendingServerStart = $false
        return
    }

    $script:PendingServerStart = $false
    Start-ExeTab `
        -ExePath $exe `
        -Banner "PURGATORY SERVER" `
        -Title "SERVER" `
        -FileName "server.cmd"
    $script:LaunchHoldUntil = [datetime]::UtcNow.AddSeconds(10)
    Write-LaunchLog "Starting server ($($script:BuildProfile))"
}

function Pump-PendingServer {
    if (-not $script:PendingServerStart) { return }
    if (Test-CargoBuilding -Package $ServerPackage) { return }
    if (-not (Test-Path -LiteralPath (Get-ServerExe))) { return }
    Start-ServerExe
}

function Stop-Server {
    $script:LaunchHoldUntil = [datetime]::MinValue
    $script:PendingServerStart = $false
    $cargoKilled = Stop-WorkspaceCargo -Package $ServerPackage
    $procKilled = Stop-OwnedByName -Name "purgatory-server"
    if ($cargoKilled -eq 0 -and $procKilled -eq 0) {
        Write-LaunchLog "Server is not running"
        return
    }
    Wait-OwnedGone -Name "purgatory-server" | Out-Null
    Write-LaunchLog "Server stopped"
}

function Restart-Server {
    $wasAlive = (@(Get-OwnedProcesses -Name "purgatory-server")).Count -gt 0
    if ($wasAlive -or (Test-CargoBuilding -Package $ServerPackage)) {
        Stop-Server
        if (-not (Wait-OwnedGone -Name "purgatory-server")) {
            Write-LaunchLog "Server did not exit; restart aborted"
            return
        }
    }
    Start-Server -Rebuild
}

function Start-ClientTab {
    $script:ClientSerial++
    $n = $script:ClientSerial
    $exe = Get-ClientExe
    Start-ExeTab `
        -ExePath $exe `
        -Banner "PURGATORY CLIENT $n" `
        -Title "CLIENT $n" `
        -FileName "client-$n.cmd"
    Write-LaunchLog "Opening client $n"
}

function Request-Clients {
    param([int]$Count)

    if (-not $script:CargoPath) {
        Write-LaunchLog "cargo is not on PATH"
        return
    }

    $exe = Get-ClientExe
    if (Test-Path -LiteralPath $exe) {
        for ($i = 0; $i -lt $Count; $i++) {
            Start-ClientTab
            if ($i -lt $Count - 1) {
                Start-Sleep -Milliseconds 140
                [Windows.Forms.Application]::DoEvents()
            }
        }
        return
    }

    $script:PendingClients += $Count
    Write-LaunchLog "Client exe missing; building once, then opening $Count"
    Pump-PendingClients
}

function Pump-PendingClients {
    if ($script:PendingClients -le 0) { return }

    $exe = Get-ClientExe
    if (Test-Path -LiteralPath $exe) {
        while ($script:PendingClients -gt 0) {
            $script:PendingClients--
            Start-ClientTab
            if ($script:PendingClients -gt 0) {
                Start-Sleep -Milliseconds 140
                [Windows.Forms.Application]::DoEvents()
            }
        }
        return
    }

    if (Test-CargoBuilding -Package $ClientPackage) { return }
    if ((@(Get-OwnedProcesses -Name "purgatory-client")).Count -gt 0) { return }

    Start-BuildOnlyTab -Package $ClientPackage
}

function Stop-Clients {
    $n = Stop-OwnedByName -Name "purgatory-client"
    Stop-WorkspaceCargo -Package $ClientPackage | Out-Null
    $script:PendingClients = 0
    if ($n -eq 0) {
        Write-LaunchLog "No client processes"
        return
    }
    Write-LaunchLog "Stopped $n client(s)"
}

function Start-QualityGate {
    if (-not $script:CargoPath) {
        Write-LaunchLog "cargo is not on PATH"
        return
    }

    $gate = Join-Path (Join-Path $Root "scripts") "check.ps1"
    $body = @(
        "title CHECK",
        "echo.",
        "echo   QUALITY GATE",
        "echo   ==============================",
        "echo.",
        "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$gate`""
    )
    $scriptPath = Write-LaunchCmd -FileName "check.cmd" -Body $body
    Invoke-TerminalTab -Title "CHECK" -ScriptPath $scriptPath
    Write-LaunchLog "Quality gate started"
}

function Start-Rebuild {
    if (-not $script:CargoPath) {
        Write-LaunchLog "cargo is not on PATH"
        return
    }

    $buildServer = Get-CargoBuildArgs -Package $ServerPackage
    $profileName = $script:BuildProfile
    $clientsRunning = (@(Get-OwnedProcesses -Name "purgatory-client")).Count -gt 0
    $body = New-Object System.Collections.Generic.List[string]
    [void]$body.Add("title BUILD")
    [void]$body.Add("echo.")
    [void]$body.Add("echo   REBUILD $profileName")
    [void]$body.Add("echo($($script:CodeIdentity)")
    [void]$body.Add("echo   ==============================")
    [void]$body.Add("echo.")
    [void]$body.Add("echo $buildServer")
    [void]$body.Add($buildServer)
    [void]$body.Add("if errorlevel 1 goto fail")
    if ($clientsRunning) {
        [void]$body.Add("echo.")
        [void]$body.Add("echo Skipping client rebuild: purgatory-client.exe is in use")
        Write-LaunchLog "Rebuild ${profileName}: server only (clients are running)"
    }
    else {
        $buildClient = Get-CargoBuildArgs -Package $ClientPackage
        [void]$body.Add("echo.")
        [void]$body.Add("echo $buildClient")
        [void]$body.Add($buildClient)
        [void]$body.Add("if errorlevel 1 goto fail")
        Write-LaunchLog "Rebuild $profileName server + client"
    }
    [void]$body.Add("echo.")
    [void]$body.Add("echo BUILD OK")
    [void]$body.Add("goto :eof")
    [void]$body.Add(":fail")
    [void]$body.Add("echo.")
    [void]$body.Add("echo BUILD FAILED")
    [void]$body.Add("pause")
    [void]$body.Add("exit /b 1")
    $scriptPath = Write-LaunchCmd -FileName "rebuild.cmd" -Body $body.ToArray()
    Invoke-TerminalTab -Title "BUILD" -ScriptPath $scriptPath
}

function Stop-Everything {
    $script:PendingClients = 0
    $script:PendingServerStart = $false
    $script:LaunchHoldUntil = [datetime]::MinValue
    Stop-WorkspaceCargo -Package "*" | Out-Null
    $servers = Stop-OwnedByName -Name "purgatory-server"
    $clients = Stop-OwnedByName -Name "purgatory-client"
    Write-LaunchLog "Kill all  (server=$servers  clients=$clients)"
}

# ------------------------------------------------------------
# UI helpers
# ------------------------------------------------------------

function New-Font {
    param([float]$Size, [Drawing.FontStyle]$Style = [Drawing.FontStyle]::Regular)
    return New-Object Drawing.Font("Segoe UI", $Size, $Style)
}

function New-Label {
    param(
        $Parent,
        [string]$Text,
        [int]$X,
        [int]$Y,
        [int]$Width = 0,
        [Drawing.Color]$Color = $Text,
        [float]$Size = 9,
        [Drawing.FontStyle]$Style = [Drawing.FontStyle]::Regular
    )

    $label = New-Object Windows.Forms.Label
    $label.Text = $Text
    $label.Location = New-Object Drawing.Point($X, $Y)
    $label.ForeColor = $Color
    $label.BackColor = [Drawing.Color]::Transparent
    $label.Font = New-Font $Size $Style
    if ($Width -gt 0) {
        $label.AutoSize = $false
        $label.Size = New-Object Drawing.Size($Width, [int]($Size + 12))
    }
    else {
        $label.AutoSize = $true
    }
    $Parent.Controls.Add($label)
    return $label
}

function New-Card {
    param($Parent, [int]$X, [int]$Y, [int]$Width, [int]$Height)

    $edge = New-Object Windows.Forms.Panel
    $edge.Location = New-Object Drawing.Point($X, $Y)
    $edge.Size = New-Object Drawing.Size($Width, $Height)
    $edge.BackColor = $Border
    $Parent.Controls.Add($edge)

    $inner = New-Object Windows.Forms.Panel
    $inner.Location = New-Object Drawing.Point(1, 1)
    $inner.Size = New-Object Drawing.Size(($Width - 2), ($Height - 2))
    $inner.BackColor = $Panel
    $edge.Controls.Add($inner)
    return $inner
}

function New-Button {
    param(
        $Parent,
        [string]$Text,
        [int]$X,
        [int]$Y,
        [int]$Width,
        [Drawing.Color]$Color,
        [scriptblock]$Click,
        [string]$Tip = ""
    )

    $button = New-Object Windows.Forms.Button
    $button.Text = $Text
    $button.Location = New-Object Drawing.Point($X, $Y)
    $button.Size = New-Object Drawing.Size($Width, 40)
    $button.FlatStyle = "Flat"
    $button.FlatAppearance.BorderSize = 1
    $button.FlatAppearance.BorderColor = $Color
    $button.FlatAppearance.MouseOverBackColor = $PanelHi
    $button.FlatAppearance.MouseDownBackColor = $BtnPress
    $button.BackColor = $Panel
    $button.ForeColor = $Color
    $button.Font = New-Font 9 ([Drawing.FontStyle]::Bold)
    $button.Cursor = [Windows.Forms.Cursors]::Hand
    $button.TabStop = $true
    if ($Tip) {
        $tips.SetToolTip($button, $Tip)
    }
    $button.Add_Click($Click)
    $Parent.Controls.Add($button)
    return $button
}

function Set-ToggleButton {
    param(
        $Button,
        [bool]$IsEnabled,
        $ActiveColor
    )

    $Button.Tag = $IsEnabled
    $Button.TabStop = $IsEnabled
    $Button.BackColor = $Panel
    if ($IsEnabled) {
        $Button.ForeColor = $ActiveColor
        $Button.FlatAppearance.BorderColor = $ActiveColor
        $Button.FlatAppearance.MouseOverBackColor = $PanelHi
        $Button.FlatAppearance.MouseDownBackColor = $BtnPress
        $Button.Cursor = [Windows.Forms.Cursors]::Hand
    }
    else {
        $Button.ForeColor = $Disabled
        $Button.FlatAppearance.BorderColor = $Disabled
        $Button.FlatAppearance.MouseOverBackColor = $Panel
        $Button.FlatAppearance.MouseDownBackColor = $Panel
        $Button.Cursor = [Windows.Forms.Cursors]::Default
    }
}

function Set-ProfileButtons {
    if ($script:BuildProfile -eq "release") {
        $btnDebug.BackColor = $Panel
        $btnDebug.ForeColor = $Muted
        $btnDebug.FlatAppearance.BorderColor = $Border
        $btnRelease.BackColor = $PanelHi
        $btnRelease.ForeColor = $Accent
        $btnRelease.FlatAppearance.BorderColor = $Accent
    }
    else {
        $btnDebug.BackColor = $PanelHi
        $btnDebug.ForeColor = $Accent
        $btnDebug.FlatAppearance.BorderColor = $Accent
        $btnRelease.BackColor = $Panel
        $btnRelease.ForeColor = $Muted
        $btnRelease.FlatAppearance.BorderColor = $Border
    }
}

function Get-LogoImage {
    $path = Join-Path (Join-Path $Root "Graphic") "LOGO.png"
    if (-not (Test-Path -LiteralPath $path)) { return $null }
    try {
        $bytes = [IO.File]::ReadAllBytes($path)
        $ms = New-Object IO.MemoryStream(,$bytes)
        return [Drawing.Image]::FromStream($ms)
    }
    catch {
        return $null
    }
}

# ------------------------------------------------------------
# Window
# ------------------------------------------------------------

$tips = New-Object Windows.Forms.ToolTip
$tips.AutoPopDelay = 8000
$tips.InitialDelay = 400

$form = New-Object Windows.Forms.Form
$form.Text = $WindowTitle
$form.Size = New-Object Drawing.Size(620, 742)
$form.StartPosition = "CenterScreen"
$form.BackColor = $Bg
$form.ForeColor = $Text
$form.FormBorderStyle = "FixedSingle"
$form.MaximizeBox = $false
$form.MinimizeBox = $true
$form.Font = New-Font 9
$form.KeyPreview = $true

try {
    $buf = $form.GetType().GetProperty("DoubleBuffered", [Reflection.BindingFlags]"NonPublic,Instance")
    if ($buf) { $buf.SetValue($form, $true, $null) }
}
catch { }

$script:LogoImage = Get-LogoImage
if ($script:LogoImage) {
    $pic = New-Object Windows.Forms.PictureBox
    $pic.Image = $script:LogoImage
    $pic.Location = New-Object Drawing.Point(24, 14)
    $pic.Size = New-Object Drawing.Size(250, 56)
    $pic.SizeMode = "Zoom"
    $pic.BackColor = $Bg
    $form.Controls.Add($pic)
}
else {
    [void](New-Label $form "PURGATORY" 28 18 0 $Text 20 ([Drawing.FontStyle]::Bold))
}

[void](New-Label $form "DEVELOPMENT LAUNCHER" 28 74 0 $Accent 9 ([Drawing.FontStyle]::Bold))
$versionLabel = New-Label $form $script:CodeIdentity 28 92 0 $Muted 8
$versionLabel.AutoEllipsis = $false
$tips.SetToolTip($versionLabel, "Cargo version, master-plan phase (PHASE file), and git commit. * means uncommitted changes.")

$btnDebug = New-Button $form "DEBUG" 330 22 120 $Accent {
    $script:BuildProfile = "debug"
    Set-ProfileButtons
    Update-LiveStatus
    Write-LaunchLog "Profile: debug"
} "Build and run target\\debug"
$btnDebug.Size = New-Object Drawing.Size(120, 28)
$btnDebug.Font = New-Font 8 ([Drawing.FontStyle]::Bold)

$btnRelease = New-Button $form "RELEASE" 456 22 120 $Muted {
    $script:BuildProfile = "release"
    Set-ProfileButtons
    Update-LiveStatus
    Write-LaunchLog "Profile: release"
} "Build and run target\\release"
$btnRelease.Size = New-Object Drawing.Size(120, 28)
$btnRelease.Font = New-Font 8 ([Drawing.FontStyle]::Bold)

$logLabel = New-Label $form "LOG LEVEL" 330 56 70 $Muted 8 ([Drawing.FontStyle]::Bold)
$logBoxChoice = New-Object Windows.Forms.ComboBox
$logBoxChoice.DropDownStyle = "DropDownList"
$logBoxChoice.Location = New-Object Drawing.Point(456, 54)
$logBoxChoice.Size = New-Object Drawing.Size(120, 24)
$logBoxChoice.BackColor = $Panel
$logBoxChoice.ForeColor = $Text
$logBoxChoice.FlatStyle = "Flat"
$logBoxChoice.Items.AddRange([object[]]@("Default", "Debug", "Trace"))
$logBoxChoice.SelectedIndex = 0
$logBoxChoice.Add_SelectedIndexChanged({
    switch ([int]$logBoxChoice.SelectedIndex) {
        1 {
            $script:RustLog = "debug,quinn=debug,quinn_proto=warn,rustls=warn"
            $script:NetLog = $true
            $script:NetVerbose = $false
        }
        2 {
            $script:RustLog = "trace,quinn=debug,quinn_proto=debug,rustls=warn"
            $script:NetLog = $true
            $script:NetVerbose = $true
        }
        default {
            $script:RustLog = ""
            $script:NetLog = $false
            $script:NetVerbose = $false
        }
    }
    Write-LaunchLog ("Log level: {0}" -f $logBoxChoice.SelectedItem)
})
$form.Controls.Add($logBoxChoice)
$tips.SetToolTip($logBoxChoice, "Applies only to NEW server/client tabs. Logs print in those terminal tabs, not in this window.")

$headerLine = New-Object Windows.Forms.Panel
$headerLine.Location = New-Object Drawing.Point(28, 118)
$headerLine.Size = New-Object Drawing.Size(548, 1)
$headerLine.BackColor = $Border
$form.Controls.Add($headerLine)

$status = New-Card $form 28 128 548 108

[void](New-Label $status "SERVER" 18 12 0 $Muted 8 ([Drawing.FontStyle]::Bold))
$ServerDot = New-Label $status $script:Dot 18 36 0 $Red 14
$ServerStatus = New-Label $status "STOPPED" 42 40 0 $Red 11 ([Drawing.FontStyle]::Bold)
$PortStatus = New-Label $status "UDP ${ListenPort}  closed" 18 70 240 $Muted 8

[void](New-Label $status "CLIENTS" 300 12 0 $Muted 8 ([Drawing.FontStyle]::Bold))
$ClientCountLabel = New-Label $status "0" 300 32 0 $Accent 18 ([Drawing.FontStyle]::Bold)
$ClientSubLabel = New-Label $status "live processes" 338 42 180 $Muted 8
$BinaryStatus = New-Label $status "debug" 300 70 220 $Muted 8

[void](New-Label $form "SERVER" 28 242 0 $Muted 8 ([Drawing.FontStyle]::Bold))
$btnStart = New-Button $form "START" 28 266 172 $Green { if ($btnStart.Tag) { Start-Server } } "Run the current binary. Builds only if it is missing.  F5"
$btnRestart = New-Button $form "RESTART" 216 266 172 $Yellow { Restart-Server } "Stop, rebuild, start. Use this after code changes."
$btnStop = New-Button $form "STOP" 404 266 172 $Red { if ($btnStop.Tag) { Stop-Server } } "Kill this workspace's server process tree."
Set-ToggleButton -Button $btnStart -IsEnabled $true -ActiveColor $Green
Set-ToggleButton -Button $btnStop -IsEnabled $false -ActiveColor $Red

[void](New-Label $form "CLIENTS" 28 322 0 $Muted 8 ([Drawing.FontStyle]::Bold))
$btnClient = New-Button $form "+ 1" 28 346 127 $Accent { Request-Clients 1 } "Open one client.  F6"
$btnClient2 = New-Button $form "+ 2" 165 346 127 $Accent { Request-Clients 2 } "Open two clients. Staggers launches to avoid cargo/GPU pile-up."
$btnClient3 = New-Button $form "+ 3" 302 346 127 $Accent { Request-Clients 3 } "Open three clients."
$btnStopClients = New-Button $form "STOP ALL" 439 346 137 $Red { Stop-Clients } "Kill this workspace's client processes."

[void](New-Label $form "TOOLS" 28 402 0 $Muted 8 ([Drawing.FontStyle]::Bold))
$btnCheck = New-Button $form "QUALITY GATE" 28 426 172 $Text { Start-QualityGate } "scripts\check.ps1  -  fmt, check, clippy, tests"
$btnBuild = New-Button $form "REBUILD" 216 426 172 $Yellow { Start-Rebuild } "cargo build into target. Skips the client package while client.exe is running (Windows file lock)."
$btnKill = New-Button $form "KILL ALL" 404 426 172 $Red { Stop-Everything } "Server, clients, and cargo builds from this repo."

$logCard = New-Card $form 28 484 548 148
[void](New-Label $logCard "ACTIVITY" 12 8 0 $Muted 8 ([Drawing.FontStyle]::Bold))

$LogBox = New-Object Windows.Forms.TextBox
$LogBox.Multiline = $true
$LogBox.ReadOnly = $true
$LogBox.ScrollBars = "Vertical"
$LogBox.BorderStyle = "None"
$LogBox.Location = New-Object Drawing.Point(12, 30)
$LogBox.Size = New-Object Drawing.Size(522, 108)
$LogBox.BackColor = $Panel
$LogBox.ForeColor = $LogColor
$LogBox.Font = New-Object Drawing.Font("Consolas", 8.5)
$logCard.Controls.Add($LogBox)

$termHint = "Windows Terminal tabs"
if (-not $script:WtPath) { $termHint = "cmd windows  (install Windows Terminal for tabs)" }

$footer = New-Label $form ("${ListenHost}:${ListenPort}   |   $termHint   |   $($script:CargoVersion)") 28 644 548 $Muted 8
$footer.AutoSize = $false
$hotkeys = New-Label $form "F5 start/restart server     F6 open client     closing this window does not kill processes" 28 664 548 $Muted 8
$hotkeys.AutoSize = $false

Set-ProfileButtons

$form.Add_KeyDown({
    if ($_.KeyCode -eq "F5") {
        if ($script:ServerRunning -or $script:ServerBuilding) { Restart-Server } else { Start-Server }
        $_.Handled = $true
    }
    elseif ($_.KeyCode -eq "F6") {
        Request-Clients 1
        $_.Handled = $true
    }
})

$timer = New-Object Windows.Forms.Timer
$timer.Interval = 500
$timer.Add_Tick({ Update-LiveStatus })

$form.Add_Shown({
    $form.Activate()
    Update-LiveStatus
    if (-not $script:CargoPath) {
        Write-LaunchLog "cargo is not on PATH - start/rebuild disabled"
        return
    }
    if ($script:ServerRunning -or $script:ServerBuilding) {
        Write-LaunchLog "Existing server process detected"
        return
    }
    if ($env:PURGATORY_LAUNCHER_NO_AUTOSTART) {
        Write-LaunchLog "Auto-start disabled"
        return
    }
    Start-Server
})

$form.Add_FormClosed({
    $timer.Stop()
    $timer.Dispose()
    if ($script:LogoImage) { $script:LogoImage.Dispose() }
    try { $script:Mutex.ReleaseMutex() } catch { }
    try { $script:Mutex.Dispose() } catch { }
})

$timer.Start()

try {
    [void]$form.ShowDialog()
}
catch {
    [Windows.Forms.MessageBox]::Show(
        $_.Exception.Message,
        $WindowTitle,
        [Windows.Forms.MessageBoxButtons]::OK,
        [Windows.Forms.MessageBoxIcon]::Error
    ) | Out-Null
}
finally {
    $timer.Stop()
}
