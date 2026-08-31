# PURGATORY Developer Tools.
# Opened by DEV.BAT. Dot-sourced modules share $script: state.

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
    [DllImport("user32.dll", CharSet = CharSet.Auto)] public static extern IntPtr SendMessage(IntPtr hWnd, int msg, IntPtr wParam, IntPtr lParam);
    [DllImport("kernel32.dll", SetLastError = true)] public static extern bool FreeConsole();
    [DllImport("shcore.dll")] public static extern int SetProcessDpiAwareness(int value);
    public const int WM_VSCROLL = 0x0115;
    public const int SB_BOTTOM = 7;
    public const int EM_SETSEL = 0x00B1;
}
"@
}

try { [PurgatoryNative]::SetProcessDpiAwareness(2) | Out-Null } catch { }

$script:DevRoot = $PSScriptRoot
$script:Root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

$script:LoadedModules = @()
$modFiles = @(
    "core\environment.ps1",
    "core\process.ps1",
    "core\state.ps1",
    "core\logging.ps1",
    "runtime\build.ps1",
    "runtime\probe.ps1",
    "runtime\server.ps1",
    "runtime\client.ps1",
    "features\load_testing.ps1",
    "features\runtime_validation.ps1",
    "features\quality_gate.ps1",
    "features\reports.ps1",
    "ui\theme.ps1",
    "ui\window.ps1"
)
foreach ($mod in $modFiles) {
    $path = Join-Path $script:DevRoot $mod
    . $path
    $script:LoadedModules += $mod
}

try {
    Initialize-DevEnvironment
    Initialize-DevTheme
    Initialize-DevState
    Initialize-DevLogging
}
catch {
    throw
}

$script:Mutex = New-Object Threading.Mutex($false, $script:MutexName)
$owned = $false
try {
    $owned = $script:Mutex.WaitOne(0, $false)
}
catch [Threading.AbandonedMutexException] {
    $owned = $true
}

if (-not $owned) {
    $other = Get-Process | Where-Object { $_.MainWindowTitle -eq $script:WindowTitle } | Select-Object -First 1
    if ($other -and $other.MainWindowHandle -ne [IntPtr]::Zero) {
        if ([PurgatoryNative]::IsIconic($other.MainWindowHandle)) {
            [PurgatoryNative]::ShowWindow($other.MainWindowHandle, 9) | Out-Null
        }
        [PurgatoryNative]::SetForegroundWindow($other.MainWindowHandle) | Out-Null
    }
    else {
        [Windows.Forms.MessageBox]::Show(
            "Developer Tools is already running, but its window could not be found.",
            $script:WindowTitle,
            [Windows.Forms.MessageBoxButtons]::OK,
            [Windows.Forms.MessageBoxIcon]::Information
        ) | Out-Null
    }
    exit 0
}

try {
    Show-DevMainWindow
}
catch {
    $msg = $_.Exception.Message
    try { Write-LaunchLog "Developer Tools failed: $msg" } catch { }
    try {
        [Windows.Forms.MessageBox]::Show(
            $msg,
            $script:WindowTitle,
            [Windows.Forms.MessageBoxButtons]::OK,
            [Windows.Forms.MessageBoxIcon]::Error
        ) | Out-Null
    }
    catch { }
    exit 1
}

[Environment]::Exit(0)
