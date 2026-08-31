# Owned process start/stop and workspace-scoped recovery scans.
# Recovery scans are not the 1 Hz source of truth for processes this session started.

$script:ChildLogNames = @{}

if (-not ("PurgatoryStreamPump" -as [type])) {
    Add-Type -TypeDefinition @"
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Threading;
public static class PurgatoryStreamPump {
    static readonly object FileGate = new object();
    static readonly ConcurrentQueue<string> UiQueue = new ConcurrentQueue<string>();
    const int UiCap = 4000;

    public static void ToFile(Stream src, string path, string name) {
        Thread t = new Thread(delegate() {
            try {
                byte[] buf = new byte[4096];
                StringBuilder leftover = new StringBuilder();
                int n;
                while ((n = src.Read(buf, 0, buf.Length)) > 0) {
                    lock (FileGate) {
                        using (FileStream dst = new FileStream(path, FileMode.Append, FileAccess.Write, FileShare.ReadWrite)) {
                            dst.Write(buf, 0, n);
                        }
                    }
                    leftover.Append(Encoding.UTF8.GetString(buf, 0, n));
                    leftover.Replace("\u0007", "");
                    string all = leftover.ToString();
                    int start = 0;
                    for (int i = 0; i < all.Length; i++) {
                        if (all[i] != '\n') { continue; }
                        string line = all.Substring(start, i - start).TrimEnd('\r');
                        line = line.Replace("\u0007", "");
                        start = i + 1;
                        if (line.Length > 0) { EnqueueUi(name, line); }
                    }
                    leftover.Length = 0;
                    if (start < all.Length) { leftover.Append(all.Substring(start)); }
                }
                if (leftover.Length > 0) {
                    string line = leftover.ToString().TrimEnd('\r', '\n');
                    line = line.Replace("\u0007", "");
                    if (line.Length > 0) { EnqueueUi(name, line); }
                }
            } catch { }
        });
        t.IsBackground = true;
        t.Name = "purgatory-log-" + name;
        t.Start();
    }

    static bool IsNoisyLoadLine(string line) {
        if (string.IsNullOrEmpty(line)) { return true; }
        if (line.StartsWith("RUNNING ")) { return true; }
        if (line.IndexOf("] Bots ", StringComparison.Ordinal) >= 0) { return true; }
        return false;
    }

    static void EnqueueUi(string name, string line) {
        if (string.IsNullOrEmpty(name) || string.IsNullOrEmpty(line)) { return; }
        if (name == "load" && IsNoisyLoadLine(line)) { return; }
        string drop;
        while (UiQueue.Count >= UiCap) {
            if (!UiQueue.TryDequeue(out drop)) { break; }
        }
        UiQueue.Enqueue(name + "\u001f" + line);
    }

    public static string[] DrainUi(int max) {
        List<string> list = new List<string>();
        string item;
        while (list.Count < max && UiQueue.TryDequeue(out item)) {
            list.Add(item);
        }
        return list.ToArray();
    }

    public static int PendingUiCount() {
        return UiQueue.Count;
    }
}
"@
}

function ConvertTo-ArgumentString {
    param([string[]]$ArgumentList = @())

    if ($null -eq $ArgumentList -or $ArgumentList.Count -eq 0) { return "" }
    $parts = foreach ($arg in $ArgumentList) {
        if ($arg -match '[\s"]') {
            '"' + ($arg -replace '"', '\"') + '"'
        }
        else {
            $arg
        }
    }
    return ($parts -join " ")
}

function Test-ProcessAlive {
    param($Process)

    if ($null -eq $Process) { return $false }
    try {
        return -not $Process.HasExited
    }
    catch {
        return $false
    }
}

function Get-ProcessExitCode {
    param($Process)

    if ($null -eq $Process) { return $null }
    try {
        if (-not $Process.HasExited) { return $null }
        return [int]$Process.ExitCode
    }
    catch {
        return $null
    }
}

function Start-OwnedProcess {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [string[]]$ArgumentList = @(),
        [hashtable]$Environment = @{},
        [switch]$RedirectOutput,
        [switch]$CaptureOnExit,
        [string]$LogName = "child",
        [switch]$VisibleConsole
    )

    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $FilePath
    $psi.Arguments = (ConvertTo-ArgumentString -ArgumentList $ArgumentList)
    $psi.WorkingDirectory = $script:Root
    $psi.UseShellExecute = $false

    if ($VisibleConsole) {
        # Parent has FreeConsole()'d so this console app gets a real window
        # instead of writing into DEV.BAT's hidden console (which dings).
        $psi.CreateNoWindow = $false
        $psi.RedirectStandardOutput = $false
        $psi.RedirectStandardError = $false
    }
    elseif ($RedirectOutput -or $CaptureOnExit) {
        $psi.CreateNoWindow = $true
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.RedirectStandardInput = $false
        try {
            $psi.StandardOutputEncoding = [Text.UTF8Encoding]::new($false)
            $psi.StandardErrorEncoding = [Text.UTF8Encoding]::new($false)
        }
        catch { }
    }
    else {
        $psi.CreateNoWindow = $true
    }

    foreach ($key in @($Environment.Keys)) {
        $psi.EnvironmentVariables[$key] = [string]$Environment[$key]
    }

    $proc = New-Object System.Diagnostics.Process
    $proc.StartInfo = $psi
    $proc.EnableRaisingEvents = $false

    if (-not $proc.Start()) {
        throw "failed to start $FilePath"
    }
    if ($RedirectOutput) {
        $script:ChildLogNames[$proc.Id] = $LogName
        $logPath = Join-Path (Get-DevLogDir) "$LogName.log"
        [PurgatoryStreamPump]::ToFile($proc.StandardOutput.BaseStream, $logPath, $LogName)
        [PurgatoryStreamPump]::ToFile($proc.StandardError.BaseStream, $logPath, $LogName)
    }
    elseif ($CaptureOnExit) {
        $script:ChildLogNames[$proc.Id] = $LogName
    }
    return $proc
}

function Flush-OwnedProcessOutput {
    param($Process)

    if ($null -eq $Process) { return }
    $n = "child"
    try {
        if ($script:ChildLogNames -and $script:ChildLogNames.ContainsKey($Process.Id)) {
            $n = [string]$script:ChildLogNames[$Process.Id]
        }
    }
    catch { }
    try {
        $out = $Process.StandardOutput.ReadToEnd()
        $err = $Process.StandardError.ReadToEnd()
        foreach ($chunk in @($out, $err)) {
            if ([string]::IsNullOrEmpty($chunk)) { continue }
            foreach ($line in ($chunk -split "`r?`n")) {
                if (-not [string]::IsNullOrWhiteSpace($line)) {
                    Push-ChildLog -Name $n -Line $line
                }
            }
        }
    }
    catch { }
}

function Stop-ProcessTree {
    param([Parameter(Mandatory = $true)][int]$ProcessId)

    if ($ProcessId -le 4) { return }
    $kill = Start-Process -FilePath "taskkill.exe" `
        -ArgumentList @("/PID", "$ProcessId", "/T", "/F") `
        -Wait -PassThru -WindowStyle Hidden -ErrorAction SilentlyContinue
    if ($kill -and $kill.ExitCode -eq 0) { return }
    Stop-Process -Id $ProcessId -Force -ErrorAction SilentlyContinue
}

function Stop-OwnedProcess {
    param($Process)

    if ($null -eq $Process) { return }
    try {
        if ($Process.HasExited) { return }
        Stop-ProcessTree -ProcessId $Process.Id
    }
    catch { }
}

function Test-ExeUnlocked {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) { return $true }
    try {
        $fs = [System.IO.File]::Open($Path, 'Open', 'ReadWrite', 'None')
        $fs.Close()
        return $true
    }
    catch {
        return $false
    }
}

# Recovery-only: processes whose image path is under this workspace target\.
function Find-WorkspaceProcesses {
    param([string]$Name)

    $exeName = $Name
    if ($exeName -notlike "*.exe") { $exeName = "$exeName.exe" }
    $procName = [IO.Path]::GetFileNameWithoutExtension($exeName)
    $prefix = Get-TargetPrefix
    foreach ($proc in @(Get-Process -Name $procName -ErrorAction SilentlyContinue)) {
        $path = $null
        try { $path = $proc.Path } catch { }
        if ([string]::IsNullOrEmpty($path)) { continue }
        if (-not $path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { continue }
        [pscustomobject]@{
            Id   = [int]$proc.Id
            Path = $path
            Process = $proc
        }
    }
}

function Stop-WorkspaceByName {
    param([string]$Name)

    $killed = 0
    foreach ($row in @(Find-WorkspaceProcesses -Name $Name)) {
        Stop-ProcessTree -ProcessId $row.Id
        $killed++
    }
    return $killed
}

# Recovery / Kill All only. Stops leftover cargo whose command line contains this repo root.
# Normal builds are stopped via the owned cargo Process.
function Stop-WorkspaceCargo {
    param([string]$Package = "*")

    if ($script:Build.Process) {
        Stop-OwnedProcess -Process $script:Build.Process
        $script:Build.Process = $null
    }

    if (-not (Get-Process -Name "cargo" -ErrorAction SilentlyContinue)) {
        return 0
    }

    $killed = 0
    $procs = Get-CimInstance Win32_Process -Filter "Name = 'cargo.exe'" -ErrorAction SilentlyContinue
    foreach ($proc in $procs) {
        $line = $proc.CommandLine
        if ([string]::IsNullOrEmpty($line)) { continue }
        if ($line -notlike "*$($script:Root)*") { continue }
        if ($Package -ne "*" -and $line -notlike "*-p $Package*" -and $line -notlike "*--package $Package*") {
            continue
        }
        Stop-ProcessTree -ProcessId ([int]$proc.ProcessId)
        $killed++
    }
    return $killed
}

function Test-WorkspacePid {
    param([int]$ProcessId, [string]$Name)

    foreach ($row in @(Find-WorkspaceProcesses -Name $Name)) {
        if ($row.Id -eq $ProcessId) { return $true }
    }
    return $false
}
