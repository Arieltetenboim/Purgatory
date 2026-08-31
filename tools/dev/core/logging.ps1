# Activity log + logs/dev-tools/. Child redirect readers must consume output (no pipe deadlock).

$script:LogLock = New-Object object
$script:PendingUiLogs = New-Object System.Collections.ArrayList
$script:LogBox = $null
$script:LogWindow = $null
$script:LogWindowBox = $null

function Write-DevFileLog {
    param(
        [string]$Name,
        [string]$Line
    )

    $path = Join-Path (Get-DevLogDir) $Name
    $text = $Line + [Environment]::NewLine
    [System.Threading.Monitor]::Enter($script:LogLock)
    try {
        [IO.File]::AppendAllText($path, $text, [Text.UTF8Encoding]::new($false))
    }
    catch { }
    finally {
        [System.Threading.Monitor]::Exit($script:LogLock)
    }
}

function Push-ChildLog {
    param(
        [string]$Name,
        [string]$Line
    )

    if ([string]::IsNullOrEmpty($Line)) { return }
    Write-DevFileLog -Name "$Name.log" -Line $Line
    if ($Name -in @("server", "client", "cargo")) {
        [System.Threading.Monitor]::Enter($script:LogLock)
        try {
            [void]$script:PendingUiLogs.Add("$Name | $Line")
        }
        catch { }
        finally {
            [System.Threading.Monitor]::Exit($script:LogLock)
        }
    }
}

function Drain-PendingUiLogs {
    $batch = $null
    [System.Threading.Monitor]::Enter($script:LogLock)
    try {
        if ($script:PendingUiLogs.Count -gt 0) {
            $batch = @($script:PendingUiLogs.ToArray())
            $script:PendingUiLogs.Clear()
        }
    }
    finally {
        [System.Threading.Monitor]::Exit($script:LogLock)
    }
    if ($null -eq $batch) { $batch = @() }
    if (("PurgatoryStreamPump" -as [type])) {
        try {
            foreach ($item in @([PurgatoryStreamPump]::DrainUi(500))) {
                if ([string]::IsNullOrEmpty($item)) { continue }
                $sep = $item.IndexOf([char]31)
                if ($sep -le 0) { continue }
                $name = $item.Substring(0, $sep)
                if ($name -eq "probe") { continue }
                $text = $item.Substring($sep + 1)
                Write-LaunchLog -Message ("{0} | {1}" -f $name, $text) -SkipFile
            }
        }
        catch { }
    }
    if ($null -eq $batch -or $batch.Count -eq 0) { return }
    foreach ($line in $batch) {
        Write-LaunchLog -Message $line -SkipFile
    }
}

function Get-LogLineColor {
    param([string]$Line)

    if ($Line -match '(^|\s)server\s\|') { return $script:LogServerColor }
    if ($Line -match '(^|\s)client\s\|') { return $script:LogClientColor }
    if ($Line -match '(^|\s)probe\s\|') { return $script:LogProbeColor }
    if ($Line -match '(^|\s)cargo\s\|') { return $script:LogCargoColor }
    return $script:LogColor
}

function Strip-LogBellCharacters {
    param([string]$Line)

    if ([string]::IsNullOrEmpty($Line)) { return $Line }
    return $Line.Replace([string][char]7, "")
}

function Set-LogBoxSelection {
    param($Box, [int]$Start, [int]$End)

    if ($null -eq $Box -or $Box.IsDisposed) { return }
    if (-not ("PurgatoryNative" -as [type])) { return }
    try {
        [void][PurgatoryNative]::SendMessage(
            $Box.Handle,
            [PurgatoryNative]::EM_SETSEL,
            [IntPtr]$Start,
            [IntPtr]$End
        )
    }
    catch { }
}

function Scroll-LogBoxToEnd {
    param($Box)

    if ($null -eq $Box -or $Box.IsDisposed) { return }
    if (-not ("PurgatoryNative" -as [type])) { return }
    try {
        [void][PurgatoryNative]::SendMessage(
            $Box.Handle,
            [PurgatoryNative]::WM_VSCROLL,
            [IntPtr][PurgatoryNative]::SB_BOTTOM,
            [IntPtr]::Zero
        )
    }
    catch { }
}

function Add-LogBoxLine {
    param(
        $Box,
        [string]$Line,
        $Color,
        [int]$MaxLines = 0
    )

    if ($null -eq $Box -or $Box.IsDisposed) { return }
    $Line = Strip-LogBellCharacters -Line $Line
    try {
        if ($MaxLines -gt 0 -and $Box.Lines.Count -gt $MaxLines) {
            $keep = [Math]::Max(1, [int]($MaxLines * 2 / 3))
            $cut = $Box.GetFirstCharIndexFromLine($Box.Lines.Count - $keep)
            if ($cut -gt 0) {
                $Box.Text = $Box.Text.Substring($cut)
            }
        }
    }
    catch { }
    try {
        if ($Box.Focused) { $Box.SelectionColor = $Color }
    }
    catch { }
    $Box.AppendText($Line + [Environment]::NewLine)
    Scroll-LogBoxToEnd -Box $Box
}

function Write-LaunchLog {
    param(
        [string]$Message,
        [switch]$SkipFile
    )

    $line = "{0}  {1}" -f (Get-Date -Format "HH:mm:ss"), $Message
    if (-not $SkipFile) {
        Write-DevFileLog -Name "launcher.log" -Line $line
    }

    $color = Get-LogLineColor -Line $line
    try { Add-LogBoxLine -Box $script:LogBox -Line $line -Color $color -MaxLines 180 } catch { }
    try { Add-LogBoxLine -Box $script:LogWindowBox -Line $line -Color $color -MaxLines 4000 } catch { }
}

function Initialize-DevLogging {
    $script:PendingUiLogs.Clear() | Out-Null
    Write-DevFileLog -Name "launcher.log" -Line ("---- {0} {1} ----" -f (Get-Date -Format "yyyy-MM-dd HH:mm:ss"), $script:CodeIdentity)
}

function Invoke-OpenDevLogs {
    $dir = Get-DevLogDir
    if (-not (Test-Path -LiteralPath $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
    Start-Process explorer.exe $dir | Out-Null
}
