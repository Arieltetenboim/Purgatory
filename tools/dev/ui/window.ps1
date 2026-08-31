# Main window: Runtime / Testing / Diagnostics. Orchestration lives in runtime/features.

function Get-StateColor {
    param([string]$State)

    switch ($State) {
        "Ready" { return $script:Green }
        "Verifying" { return $script:Yellow }
        "Starting" { return $script:Yellow }
        "Building" { return $script:Yellow }
        "Stopping" { return $script:Yellow }
        "Degraded" { return $script:Yellow }
        "Failed" { return $script:Red }
        default { return $script:Red }
    }
}

function Get-PassFailColor {
    param([string]$Kind)

    switch ($Kind) {
        "pass" { return $script:Green }
        "yes" { return $script:Green }
        "fail" { return $script:Red }
        "no" { return $script:Muted }
        default { return $script:Muted }
    }
}

function Update-LiveStatus {
    if ($script:UiClosing) { return }
    if ($null -eq $script:UiTimer) { return }
    try {
        Drain-PendingUiLogs
        Update-ServerLifecycle
        Update-ClientLifecycle
        Update-PendingLoadTest
        Update-PendingRuntimeValidation
        Update-LoadTestProcess
        Update-RecoveryScan

        $state = [string]$script:Server.State
        $statusColor = Get-StateColor -State $state
        $statusText = $state.ToUpper()
        Set-LabelIfChanged -Label $script:ServerDot -Value $script:Dot -Color $statusColor
        Set-LabelIfChanged -Label $script:ServerStatus -Value $statusText -Color $statusColor

        $buildLine = "Build:  {0}" -f $script:BuildProfile
        if (Test-BuildRunning) {
            $buildLine = "Build:  {0}  ({1})" -f $script:BuildProfile, $script:Build.Reason
        }
        Set-LabelIfChanged -Label $script:BuildLine -Value $buildLine -Color $script:Muted

        $ep = "Endpoint:  $($script:ListenHost):$($script:ListenPort)"
        Set-LabelIfChanged -Label $script:EndpointLine -Value $ep -Color $script:Muted

        $healthText = "Health:  FAIL"
        $healthColor = $script:Red
        if ($script:Health.MetricsOk) {
            $healthText = "Health:  PASS"
            $healthColor = $script:Green
        }
        elseif ($state -eq "Stopped") {
            $healthText = "Health:  -"
            $healthColor = $script:Muted
        }
        Set-LabelIfChanged -Label $script:HealthLine -Value $healthText -Color $healthColor

        $connVal = [string]$script:Health.Connection
        $connText = "Connection:  $($connVal.ToUpper())"
        if ($state -eq "Stopped") { $connText = "Connection:  -" }
        $connColor = Get-PassFailColor -Kind $script:Health.Connection
        if ($script:Health.Connection -eq "pass") { $connText = "Connection:  PASS"; $connColor = $script:Green }
        Set-LabelIfChanged -Label $script:ConnLine -Value $connText -Color $connColor

        $live = Get-ClientLiveCount
        $clientText = "Running:  $live"
        if ($script:PendingClients -gt 0) {
            $clientText = "Running:  $live   queued: $($script:PendingClients)"
        }
        Set-LabelIfChanged -Label $script:ClientCountLabel -Value $clientText -Color $script:Accent

        $rvLine = "Runtime Val:  -"
        if (Get-Command Get-RuntimeValidationUiLine -ErrorAction SilentlyContinue) {
            $rvLine = Get-RuntimeValidationUiLine
        }
        Set-LabelIfChanged -Label $script:RuntimeValLine -Value $rvLine -Color $script:Accent

        $procMark = "FAIL"
        if ($script:Health.ProcessAlive) { $procMark = "PASS" }
        $procText = "Server process      $procMark"
        $procKind = "no"
        if ($script:Health.ProcessAlive) { $procKind = "pass" }
        Set-LabelIfChanged -Label $script:DiagProcess -Value $procText -Color (Get-PassFailColor -Kind $procKind)
        Set-LabelIfChanged -Label $script:DiagListener -Value ("Listener (diag)     {0}" -f $script:Health.Listener) -Color (Get-PassFailColor -Kind $script:Health.Listener)
        $m = "-"
        $mKind = "no"
        if ($script:Health.MetricsOk) { $m = "PASS"; $mKind = "pass" }
        Set-LabelIfChanged -Label $script:DiagMetrics -Value ("Metrics             {0}" -f $m) -Color (Get-PassFailColor -Kind $mKind)
        $p = $script:Health.Connection.ToUpper()
        if ($p -eq "UNKNOWN") { $p = "-" }
        Set-LabelIfChanged -Label $script:DiagProbe -Value ("Connection probe    {0}" -f $p) -Color (Get-PassFailColor -Kind $script:Health.Connection)

        $fail = ""
        if ($script:Server.LastFailure -and $state -in @("Failed", "Degraded")) {
            $fail = [string]$script:Server.LastFailure
        }
        Set-LabelIfChanged -Label $script:DiagFail -Value $fail -Color $script:Red

        $canStart = [bool]$script:CargoPath -and ($state -eq "Stopped" -or ($state -eq "Failed" -and -not (Test-ProcessAlive -Process $script:Server.Process)))
        $canStop = $state -notin @("Stopped") -or (Test-ProcessAlive -Process $script:Server.Process)
        Set-ToggleButton -Button $script:BtnStart -IsEnabled $canStart -ActiveColor $script:Green
        $script:BtnRestart.Enabled = [bool]$script:CargoPath
        Set-ToggleButton -Button $script:BtnStop -IsEnabled $canStop -ActiveColor $script:Red
        $script:BtnStopClients.Enabled = $live -gt 0 -or $script:PendingClients -gt 0
        $openEnabled = [bool]$script:CargoPath
        $script:BtnClient.Enabled = $openEnabled
        $script:BtnClient2.Enabled = $openEnabled
        $script:BtnClient3.Enabled = $openEnabled

        $interval = 1000
        $streamPending = 0
        try {
            if (("PurgatoryStreamPump" -as [type])) {
                $streamPending = [PurgatoryStreamPump]::PendingUiCount()
            }
        }
        catch { }
        if ($state -in @("Building", "Starting", "Verifying", "Stopping") -or (Test-BuildRunning) -or $script:PendingClients -gt 0 -or $streamPending -gt 0) {
            $interval = 250
        }
        if ($null -eq $script:UiTimer) { return }
        if ($script:UiTimer.Interval -ne $interval) { $script:UiTimer.Interval = $interval }
    }
    catch {
        # Keep the UI timer alive even if a status probe fails.
    }
}

function Show-ActivityLogWindow {
    if ($null -ne $script:LogWindow -and -not $script:LogWindow.IsDisposed) {
        if ($script:LogWindow.WindowState -eq [Windows.Forms.FormWindowState]::Minimized) {
            $script:LogWindow.WindowState = [Windows.Forms.FormWindowState]::Normal
        }
        $script:LogWindow.Activate()
        return
    }

    $win = New-Object Windows.Forms.Form
    $win.Text = "PURGATORY Activity Log"
    $win.Size = New-Object Drawing.Size(820, 560)
    $win.MinimumSize = New-Object Drawing.Size(400, 240)
    $win.StartPosition = "WindowsDefaultLocation"
    $win.FormBorderStyle = "Sizable"
    $win.MaximizeBox = $true
    $win.MinimizeBox = $true
    $win.BackColor = $script:Bg
    $win.ForeColor = $script:Text
    $win.ShowInTaskbar = $true

    $box = New-Object Windows.Forms.RichTextBox
    $box.Dock = [Windows.Forms.DockStyle]::Fill
    $box.Multiline = $true
    $box.ReadOnly = $true
    $box.ScrollBars = "Both"
    $box.WordWrap = $false
    $box.BorderStyle = "None"
    $box.HideSelection = $true
    $box.DetectUrls = $false
    $box.BackColor = $script:Panel
    $box.ForeColor = $script:LogColor
    $box.Font = New-Object Drawing.Font("Consolas", 9.5)
    $win.Controls.Add($box)

    try {
        if ($null -ne $script:LogBox -and -not $script:LogBox.IsDisposed -and $script:LogBox.Rtf) {
            $box.Rtf = $script:LogBox.Rtf
            Scroll-LogBoxToEnd -Box $box
        }
    }
    catch { }

    $script:LogWindow = $win
    $script:LogWindowBox = $box
    $win.Add_FormClosed({
            $script:LogWindowBox = $null
            $script:LogWindow = $null
        })
    $win.Show()
}

function Show-DevMainWindow {
    $script:Tips = New-Object Windows.Forms.ToolTip
    $script:Tips.AutoPopDelay = 8000
    $script:Tips.InitialDelay = 400

    $form = New-Object Windows.Forms.Form
    $script:UiForm = $form
    $form.Text = $script:WindowTitle
    $form.Size = New-Object Drawing.Size(620, 984)
    $form.StartPosition = "CenterScreen"
    $form.BackColor = $script:Bg
    $form.ForeColor = $script:Text
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
        $pic.BackColor = $script:Bg
        $form.Controls.Add($pic)
    }
    else {
        [void](New-Label $form "PURGATORY" 28 18 0 $script:Text 20 ([Drawing.FontStyle]::Bold))
    }

    [void](New-Label $form "DEVELOPER TOOLS" 28 74 0 $script:Accent 9 ([Drawing.FontStyle]::Bold))
    $versionLabel = New-Label $form $script:CodeIdentity 28 92 0 $script:Muted 8
    $script:Tips.SetToolTip($versionLabel, "Cargo version, PHASE file, and git commit. * means uncommitted changes.")

    $script:BtnDebug = New-Button $form "DEBUG" 330 22 120 $script:Accent {
        $script:BuildProfile = "debug"
        Set-ProfileButtons
        Update-LiveStatus
        Write-LaunchLog "Profile: debug"
    } "Build and run target\debug" -Height 28
    $script:BtnDebug.Font = New-Font 8 ([Drawing.FontStyle]::Bold)

    $script:BtnRelease = New-Button $form "RELEASE" 456 22 120 $script:Muted {
        $script:BuildProfile = "release"
        Set-ProfileButtons
        Update-LiveStatus
        Write-LaunchLog "Profile: release"
    } "Build and run target\release" -Height 28
    $script:BtnRelease.Font = New-Font 8 ([Drawing.FontStyle]::Bold)

    [void](New-Label $form "LOG LEVEL" 330 56 70 $script:Muted 8 ([Drawing.FontStyle]::Bold))
    $logBoxChoice = New-Object Windows.Forms.ComboBox
    $logBoxChoice.DropDownStyle = "DropDownList"
    $logBoxChoice.Location = New-Object Drawing.Point(456, 54)
    $logBoxChoice.Size = New-Object Drawing.Size(120, 24)
    $logBoxChoice.BackColor = $script:Panel
    $logBoxChoice.ForeColor = $script:Text
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
    $script:Tips.SetToolTip($logBoxChoice, "Applies only to NEW server/client processes.")

    $headerLine = New-Object Windows.Forms.Panel
    $headerLine.Location = New-Object Drawing.Point(28, 118)
    $headerLine.Size = New-Object Drawing.Size(548, 1)
    $headerLine.BackColor = $script:Border
    $form.Controls.Add($headerLine)

    [void](New-Label $form "RUNTIME" 28 126 0 $script:Accent 8 ([Drawing.FontStyle]::Bold))

    $status = New-Card $form 28 146 548 138
    [void](New-Label $status "SERVER" 18 8 0 $script:Muted 8 ([Drawing.FontStyle]::Bold))
    $script:ServerDot = New-Label $status $script:Dot 18 28 0 $script:Red 14
    $script:ServerStatus = New-Label $status "STOPPED" 42 32 0 $script:Red 11 ([Drawing.FontStyle]::Bold)
    $script:BuildLine = New-Label $status "Build:  debug" 18 58 500 $script:Muted 8
    $script:EndpointLine = New-Label $status ("Endpoint:  {0}:{1}" -f $script:ListenHost, $script:ListenPort) 18 76 500 $script:Muted 8
    $script:HealthLine = New-Label $status "Health:  -" 18 94 250 $script:Muted 8
    $script:ConnLine = New-Label $status "Connection:  -" 280 94 250 $script:Muted 8

    $script:BtnStart = New-Button $form "START" 28 292 172 $script:Green { if ($script:BtnStart.Tag) { Request-ServerStart } } "Rebuild then start the server. Clients wait until Ready.  F5"
    $script:BtnRestart = New-Button $form "RESTART" 216 292 172 $script:Yellow { Request-ServerRestart } "Stop, rebuild, start. Use after code changes."
    $script:BtnStop = New-Button $form "STOP" 404 292 172 $script:Red { if ($script:BtnStop.Tag) { Request-ServerStop -ClearLoadQueue } } "Stop this workspace server process tree."
    Set-ToggleButton -Button $script:BtnStart -IsEnabled $true -ActiveColor $script:Green
    Set-ToggleButton -Button $script:BtnStop -IsEnabled $false -ActiveColor $script:Red

    [void](New-Label $form "CLIENTS" 28 342 0 $script:Muted 8 ([Drawing.FontStyle]::Bold))
    $script:ClientCountLabel = New-Label $form "Running:  0" 110 340 400 $script:Accent 9 ([Drawing.FontStyle]::Bold)
    $script:BtnClient = New-Button $form "+ 1" 28 364 127 $script:Accent { Request-Clients 1 } "Queue one client (launches after Ready).  F6"
    $script:BtnClient2 = New-Button $form "+ 2" 165 364 127 $script:Accent { Request-Clients 2 } "Queue two clients."
    $script:BtnClient3 = New-Button $form "+ 3" 302 364 127 $script:Accent { Request-Clients 3 } "Queue three clients."
    $script:BtnStopClients = New-Button $form "STOP ALL" 439 364 137 $script:Red { Request-StopClients } "Stop this workspace client processes."

    [void](New-Label $form "TESTING" 28 418 0 $script:Accent 8 ([Drawing.FontStyle]::Bold))
    $script:BtnCheck = New-Button $form "QUALITY GATE" 28 442 172 $script:Text { Start-QualityGate } "scripts\check.ps1 - fmt, check, clippy, tests (visible console)"
    $script:BtnBuild = New-Button $form "REBUILD" 216 442 172 $script:Yellow { Request-Rebuild } "Owned cargo build. Skips a package whose .exe is running."
    $script:BtnLoadRun = New-Button $form "LOAD TEST" 404 442 172 $script:Accent { Invoke-RunLoadTest } "Headless bots via purgatory-load. May restart server in load mode."
    $script:BtnLoadStop = New-Button $form "STOP LOAD" 28 486 172 $script:Red { Request-StopLoadTest } "Stop workspace-owned purgatory-load only."
    $script:BtnLoadAnalyze = New-Button $form "ANALYZE LAST RUN" 216 486 172 $script:Text { Invoke-AnalyzeLastRun } "Offline charts from a completed logs\load artifact (not an in-progress or future-dated folder)"
    $script:BtnLoadReport = New-Button $form "LAST REPORT" 404 486 172 $script:Text { Invoke-OpenLastReport } "Open last finished run report folder"
    $script:BtnRuntimeVal = New-Button $form "RUNTIME VAL" 28 530 172 $script:Accent { Invoke-RunRuntimeValidation } "Same purgatory-load --preset CLI as headless. Always restarts a clean load-mode server. Pass/fail in Rust."
    $script:RuntimeValLine = New-Label $form "Runtime Val:  idle" 216 536 360 $script:Muted 8

    [void](New-Label $form "DIAGNOSTICS" 28 584 0 $script:Accent 8 ([Drawing.FontStyle]::Bold))
    $diag = New-Card $form 28 604 548 118
    $script:DiagProcess = New-Label $diag "Server process      -" 18 10 500 $script:Muted 8
    $script:DiagListener = New-Label $diag "Listener (diag)     -" 18 28 500 $script:Muted 8
    $script:DiagMetrics = New-Label $diag "Metrics             -" 18 46 500 $script:Muted 8
    $script:DiagProbe = New-Label $diag "Connection probe    -" 18 64 500 $script:Muted 8
    $script:DiagFail = New-Label $diag "" 18 86 510 $script:Red 8
    $script:BtnDevLogs = New-Button $form "OPEN LOGS" 28 730 172 $script:Text { Invoke-OpenDevLogs } "Open logs\dev-tools (launcher, server, cargo, probe)"
    $script:BtnLoadLogs = New-Button $form "LOAD LOGS" 216 730 172 $script:Text { Invoke-OpenLoadLogs } "Open logs\load in Explorer"
    $script:BtnKill = New-Button $form "KILL ALL" 404 730 172 $script:Red { Request-KillAll } "Server, clients, load harness, and cargo builds from this repo."

    $logCard = New-Card $form 28 778 548 120
    [void](New-Label $logCard "ACTIVITY" 12 6 80 $script:Muted 8 ([Drawing.FontStyle]::Bold))
    [void](New-Label $logCard "server" 92 6 52 $script:LogServerColor 8)
    [void](New-Label $logCard "client" 148 6 48 $script:LogClientColor 8)
    $script:BtnLogExpand = New-Button $logCard ([string][char]0x25BE) 508 4 28 $script:Accent { Show-ActivityLogWindow } "Open activity log in a resizable window" -Height 22
    $script:BtnLogExpand.Font = New-Font 10 ([Drawing.FontStyle]::Bold)
    $LogBox = New-Object Windows.Forms.RichTextBox
    $LogBox.Multiline = $true
    $LogBox.ReadOnly = $true
    $LogBox.ScrollBars = "Vertical"
    $LogBox.BorderStyle = "None"
    $LogBox.HideSelection = $true
    $LogBox.DetectUrls = $false
    $LogBox.Location = New-Object Drawing.Point(12, 26)
    $LogBox.Size = New-Object Drawing.Size(522, 86)
    $LogBox.BackColor = $script:Panel
    $LogBox.ForeColor = $script:LogColor
    $LogBox.Font = New-Object Drawing.Font("Consolas", 8.5)
    $logCard.Controls.Add($LogBox)
    $script:LogBox = $LogBox

    $footer = New-Label $form ("{0}:{1}   |   {2}" -f $script:ListenHost, $script:ListenPort, $script:CargoVersion) 28 904 548 $script:Muted 8
    $footer.AutoSize = $false
    $hotkeys = New-Label $form "F5 start/restart server     F6 open client     closing this window does not kill processes" 28 922 548 $script:Muted 8
    $hotkeys.AutoSize = $false

    Set-ProfileButtons

    $form.Add_KeyDown({
            if ($_.KeyCode -eq "F5") {
                if ($script:Server.State -in @("Stopped", "Failed")) { Request-ServerStart } else { Request-ServerRestart }
                $_.Handled = $true
            }
            elseif ($_.KeyCode -eq "F6") {
                Request-Clients 1
                $_.Handled = $true
            }
        })

    $timer = New-Object Windows.Forms.Timer
    $timer.Interval = 1000
    $timer.Add_Tick({ Update-LiveStatus })
    $script:UiTimer = $timer

    $form.Add_Shown({
            try {
                [IO.File]::WriteAllText((Join-Path $env:TEMP "purgatory-dt-shown"), "1")
            }
            catch { }
            try {
                $form.Activate()
                Invoke-StartupRecovery
                Update-LiveStatus
                if (-not $script:CargoPath) {
                    Write-LaunchLog "cargo is not on PATH - start/rebuild disabled"
                    return
                }
                if (Test-ProcessAlive -Process $script:Server.Process) {
                    Write-LaunchLog "Existing server process detected; verifying"
                    return
                }
                Write-LaunchLog "Auto-start disabled; click START"
            }
            catch { }
        })

    $form.Add_FormClosed({
            try {
                $script:UiClosing = $true
                if ($null -ne $script:UiTimer) {
                    try { $script:UiTimer.Stop() } catch { }
                }
                if ($script:LogoImage) {
                    try { $script:LogoImage.Dispose() } catch { }
                }
                try { $script:Mutex.ReleaseMutex() } catch { }
                try { $script:Mutex.Dispose() } catch { }
            }
            catch { }
            [Environment]::Exit(0)
        })

    $form.Add_FormClosing({
            $script:UiClosing = $true
            if ($null -ne $script:UiTimer) {
                try { $script:UiTimer.Stop() } catch { }
            }
            if ($null -ne $script:LogWindow -and -not $script:LogWindow.IsDisposed) {
                try { $script:LogWindow.Close() } catch { }
            }
        })

    $timer.Start()

    try {
        [void]$form.ShowDialog()
    }
    catch {
        [Windows.Forms.MessageBox]::Show(
            $_.Exception.Message,
            $script:WindowTitle,
            [Windows.Forms.MessageBoxButtons]::OK,
            [Windows.Forms.MessageBoxIcon]::Error
        ) | Out-Null
    }
    finally {
        $script:UiClosing = $true
        Stop-DevUiTimer
    }
}

function Stop-DevUiTimer {
    if ($null -eq $script:UiTimer) { return }
    try { $script:UiTimer.Stop() } catch { }
    try { $script:UiTimer.Dispose() } catch { }
    $script:UiTimer = $null
}
