# Load test dialog, load-mode server, harness. Uses shared process/runtime helpers.

function Get-LoadLogsRoot {
    return (Join-Path $script:Root "logs\load")
}

function Get-PointerRunName {
    param([string]$FileName)
    $path = Join-Path (Get-LoadLogsRoot) $FileName
    if (-not (Test-Path -LiteralPath $path)) { return $null }
    $name = (Get-Content -LiteralPath $path -Raw).Trim()
    if ([string]::IsNullOrWhiteSpace($name)) { return $null }
    return $name
}

function Test-CompletedRunDir {
    param([string]$Dir)
    if ([string]::IsNullOrWhiteSpace($Dir)) { return $false }
    if (-not (Test-Path -LiteralPath $Dir)) { return $false }
    return [bool](Test-Path -LiteralPath (Join-Path $Dir "run_summary.json"))
}

function Resolve-CompletedRunDir {
    param([string]$Name, [string]$Current, [bool]$Running)
    if ([string]::IsNullOrWhiteSpace($Name)) { return $null }
    if ($Running -and $Current -and ($Name -eq $Current)) { return $null }
    $dir = Join-Path (Get-LoadLogsRoot) $Name
    if (-not (Test-CompletedRunDir -Dir $dir)) { return $null }
    if (Test-FutureDatedRunName -Name $Name) {
        Write-LaunchLog "Run folder name looks future-dated ($Name); using it anyway (old harness timestamp)"
    }
    return $dir
}

function Get-LastFinishedRunDir {
    $current = Get-PointerRunName -FileName "current_run.txt"
    $running = $script:LoadTest.State -eq "Running" -or (Test-ProcessAlive -Process $script:LoadTest.Process)

    foreach ($pointer in @("last_runtime_validation.txt", "last_finished.txt", "latest.txt")) {
        $dir = Resolve-CompletedRunDir -Name (Get-PointerRunName -FileName $pointer) -Current $current -Running $running
        if ($dir) { return $dir }
    }

    $exclude = $null
    if ($running) { $exclude = $current }
    return Get-LatestCompletedRunDir -ExcludeName $exclude
}

function Test-FutureDatedRunName {
    param([string]$Name)
    if ($Name -notmatch '^(\d{8})_') { return $false }
    try {
        $parsed = [datetime]::ParseExact($Matches[1], "yyyyMMdd", [Globalization.CultureInfo]::InvariantCulture)
        return $parsed.Date -gt ([datetime]::UtcNow.Date.AddDays(1))
    }
    catch {
        return $false
    }
}

function Get-LatestCompletedRunDir {
    param([string]$ExcludeName)
    $root = Get-LoadLogsRoot
    if (-not (Test-Path -LiteralPath $root)) { return $null }
    $best = $null
    $bestTime = [datetime]::MinValue
    Get-ChildItem -LiteralPath $root -Directory -ErrorAction SilentlyContinue | ForEach-Object {
        if ($ExcludeName -and $_.Name -eq $ExcludeName) { return }
        $summary = Join-Path $_.FullName "run_summary.json"
        if (-not (Test-Path -LiteralPath $summary)) { return }
        $t = (Get-Item -LiteralPath $summary).LastWriteTimeUtc
        if ($t -gt $bestTime) {
            $bestTime = $t
            $best = $_.FullName
        }
    }
    return $best
}

function Show-LoadTestDialog {
    $formLoad = New-Object Windows.Forms.Form
    $formLoad.Text = "Run Load Test"
    $formLoad.Size = New-Object Drawing.Size(360, 280)
    $formLoad.StartPosition = "CenterParent"
    $formLoad.FormBorderStyle = "FixedDialog"
    $formLoad.MaximizeBox = $false
    $formLoad.MinimizeBox = $false

    [void](New-Label $formLoad "Count" 20 20 80 $script:Muted 8)
    $boxCount = New-Object Windows.Forms.ComboBox
    $boxCount.DropDownStyle = "DropDownList"
    $boxCount.Location = New-Object Drawing.Point(120, 18)
    $boxCount.Size = New-Object Drawing.Size(200, 24)
    $boxCount.Items.AddRange([object[]]@("1", "2", "10", "25", "50", "100"))
    $boxCount.SelectedItem = "10"
    $formLoad.Controls.Add($boxCount)

    [void](New-Label $formLoad "Profile" 20 55 80 $script:Muted 8)
    $boxProfile = New-Object Windows.Forms.ComboBox
    $boxProfile.DropDownStyle = "DropDownList"
    $boxProfile.Location = New-Object Drawing.Point(120, 53)
    $boxProfile.Size = New-Object Drawing.Size(200, 24)
    $boxProfile.Items.AddRange([object[]]@("idle", "walker", "jumper", "mixed"))
    $boxProfile.SelectedItem = "mixed"
    $formLoad.Controls.Add($boxProfile)

    [void](New-Label $formLoad "Scenario" 20 90 80 $script:Muted 8)
    $boxScenario = New-Object Windows.Forms.ComboBox
    $boxScenario.DropDownStyle = "DropDownList"
    $boxScenario.Location = New-Object Drawing.Point(120, 88)
    $boxScenario.Size = New-Object Drawing.Size(200, 24)
    $boxScenario.Items.AddRange([object[]]@("load", "burst", "churn"))
    $boxScenario.SelectedItem = "load"
    $formLoad.Controls.Add($boxScenario)

    [void](New-Label $formLoad "Duration" 20 125 80 $script:Muted 8)
    $boxDur = New-Object Windows.Forms.ComboBox
    $boxDur.DropDownStyle = "DropDownList"
    $boxDur.Location = New-Object Drawing.Point(120, 123)
    $boxDur.Size = New-Object Drawing.Size(200, 24)
    $boxDur.Items.AddRange([object[]]@("1m", "2m", "5m", "10m", "30m"))
    $boxDur.SelectedItem = "2m"
    $formLoad.Controls.Add($boxDur)

    [void](New-Label $formLoad "Seed" 20 160 80 $script:Muted 8)
    $boxSeed = New-Object Windows.Forms.TextBox
    $boxSeed.Location = New-Object Drawing.Point(120, 158)
    $boxSeed.Size = New-Object Drawing.Size(200, 24)
    $boxSeed.Text = "1234"
    $formLoad.Controls.Add($boxSeed)

    $ok = New-Object Windows.Forms.Button
    $ok.Text = "RUN"
    $ok.Location = New-Object Drawing.Point(120, 200)
    $ok.Size = New-Object Drawing.Size(90, 28)
    $ok.DialogResult = [Windows.Forms.DialogResult]::OK
    $formLoad.Controls.Add($ok)
    $formLoad.AcceptButton = $ok

    $cancel = New-Object Windows.Forms.Button
    $cancel.Text = "Cancel"
    $cancel.Location = New-Object Drawing.Point(230, 200)
    $cancel.Size = New-Object Drawing.Size(90, 28)
    $cancel.DialogResult = [Windows.Forms.DialogResult]::Cancel
    $formLoad.Controls.Add($cancel)
    $formLoad.CancelButton = $cancel

    if ($formLoad.ShowDialog() -ne [Windows.Forms.DialogResult]::OK) { return $null }

    $seed = $boxSeed.Text.Trim()
    if ([string]::IsNullOrWhiteSpace($seed)) { $seed = "1234" }
    return @{
        Count    = [int]$boxCount.SelectedItem
        Profile  = [string]$boxProfile.SelectedItem
        Scenario = [string]$boxScenario.SelectedItem
        Duration = [string]$boxDur.SelectedItem
        Seed     = $seed
    }
}

function Start-LoadHarness {
    param(
        [int]$Count = 10,
        [string]$Profile = "mixed",
        [string]$Scenario = "load",
        [string]$Duration = "2m",
        [string]$Seed = "1234",
        [int]$MaxBots = 256
    )

    $exe = Get-LoadExe
    if (-not (Test-Path -LiteralPath $exe)) {
        Write-LaunchLog "Load binary missing; building $($script:LoadPackage)..."
        [void](Start-OwnedBuild -Packages @($script:LoadPackage) -Reason "rebuild")
        Write-LaunchLog "Re-run Load Test after build finishes."
        return $false
    }

    $server = "{0}:{1}" -f $script:ListenHost, $script:ListenPort
    $metrics = "{0}:{1}" -f $script:ListenHost, $script:MetricsPort
    $argList = @(
        "--count", "$Count",
        "--profile", $Profile,
        "--scenario", $Scenario,
        "--duration", $Duration,
        "--seed", $Seed,
        "--max-bots", "$MaxBots",
        "--allow-high-count",
        "--server", $server,
        "--metrics", $metrics
    )

    try {
        $proc = Start-OwnedProcess `
            -FilePath $exe `
            -ArgumentList $argList `
            -Environment (Get-ChildEnvironment) `
            -VisibleConsole
    }
    catch {
        Write-LaunchLog "Load harness failed to start: $($_.Exception.Message)"
        return $false
    }

    $script:LoadTest.Process = $proc
    $script:LoadTest.State = "Running"
    $script:LoadTest.Kind = "load-test"
    $script:LoadTest.StartedAt = [datetime]::UtcNow
    Write-LaunchLog "Starting load test: $Count bots / $Profile / $Duration / seed $Seed"
    return $true
}

function Request-StopLoadTest {
    $script:PendingLoadSpec = $null
    Stop-OwnedProcess -Process $script:LoadTest.Process
    $script:LoadTest.Process = $null
    $n = Stop-WorkspaceByName -Name "purgatory-load"
    $script:LoadTest.State = "Stopped"
    $script:LoadTest.Kind = $null
    $script:LoadTest.StartedAt = $null
    $current = Join-Path (Get-LoadLogsRoot) "current_run.txt"
    if (Test-Path -LiteralPath $current) {
        Remove-Item -LiteralPath $current -Force -ErrorAction SilentlyContinue
    }
    Write-LaunchLog "Stopped load harness ($n)"
}

function Invoke-RunLoadTest {
    $spec = Show-LoadTestDialog
    if ($null -eq $spec) { return }

    $count = [int]$spec.Count
    $probe = Probe-ServerMetrics
    $compatible = Test-LoadProbeCompatible -Probe $probe -Count $count

    if ($compatible) {
        Write-LaunchLog ("Server Load Mode ready: admission={0} entities={1}" -f [int]$probe.admission_cap, [int]$probe.max_entities_per_snapshot)
        $maxBots = [int]$probe.admission_cap
        [void](Start-LoadHarness -Count $count -Profile $spec.Profile -Scenario $spec.Scenario -Duration $spec.Duration -Seed $spec.Seed -MaxBots $maxBots)
        return
    }

    if (Test-ProcessAlive -Process $script:Server.Process) {
        $msg = "Server must restart in Load Mode for this test (need admission_cap>=$count, metrics on ${script:ListenHost}:$($script:MetricsPort)).`n`nRestart now?"
        $ans = [Windows.Forms.MessageBox]::Show($msg, "LOAD MODE", "YesNo", "Question")
        if ($ans -ne [Windows.Forms.DialogResult]::Yes) {
            Write-LaunchLog "LOAD TEST cancelled (server not load-mode compatible)"
            return
        }
    }
    else {
        Write-LaunchLog "No server running; starting Load Mode automatically..."
    }

    $script:PendingLoadSpec = $spec
    $script:Server.WantLoadMode = $true
    $script:Server.RestartAfterStop = $true
    if (Test-ProcessAlive -Process $script:Server.Process -or $script:Server.State -eq "Building") {
        Request-ServerStop
        $script:Server.RestartAfterStop = $true
        $script:Server.WantLoadMode = $true
        return
    }
    Request-ServerStart -LoadMode
}

function Update-PendingLoadTest {
    if ($null -eq $script:PendingLoadSpec) { return }

    if ($script:Server.State -eq "Failed") {
        $reason = $script:Server.LastFailure
        if (-not $reason) { $reason = "server failed before load test" }
        Write-LaunchLog $reason
        [Windows.Forms.MessageBox]::Show($reason, "LOAD TEST", "OK", "Error") | Out-Null
        $script:PendingLoadSpec = $null
        return
    }

    if ($script:Server.State -ne "Ready") { return }

    $spec = $script:PendingLoadSpec
    $probe = $script:Health.Metrics
    if ($null -eq $probe) { return }
    if (-not (Test-LoadProbeCompatible -Probe $probe -Count ([int]$spec.Count))) {
        $reason = "Server Ready but not load-mode compatible (admission_cap=$($probe.admission_cap), requested $($spec.Count))"
        Write-LaunchLog $reason
        [Windows.Forms.MessageBox]::Show($reason, "LOAD TEST", "OK", "Error") | Out-Null
        $script:PendingLoadSpec = $null
        return
    }

    $maxBots = $script:LoadAdmissionCap
    if ($null -ne $probe -and $probe.admission_cap) { $maxBots = [int]$probe.admission_cap }
    Write-LaunchLog ("Server Load Mode ready: admission={0} entities={1}" -f [int]$probe.admission_cap, [int]$probe.max_entities_per_snapshot)
    $script:PendingLoadSpec = $null
    [void](Start-LoadHarness -Count ([int]$spec.Count) -Profile $spec.Profile -Scenario $spec.Scenario -Duration $spec.Duration -Seed $spec.Seed -MaxBots $maxBots)
}

function Invoke-AnalyzeLastRun {
    $dir = Get-LastFinishedRunDir
    if (-not $dir) {
        [Windows.Forms.MessageBox]::Show(
            "No finished run found under logs\load (need a folder with run_summary.json).`nUse LOAD LOGS if a run directory exists.",
            "ANALYZE",
            "OK",
            "Information"
        ) | Out-Null
        return
    }
    $py = Get-Command python -ErrorAction SilentlyContinue
    if (-not $py) { $py = Get-Command py -ErrorAction SilentlyContinue }
    if (-not $py) {
        Write-LaunchLog "Python not found for analyzer"
        [Windows.Forms.MessageBox]::Show("Python not found on PATH.", "ANALYZE", "OK", "Warning") | Out-Null
        return
    }
    $analyzer = Join-Path $script:Root "tools\analyze_load_run.py"
    $cmd = "`"$($py.Source)`" `"$analyzer`" `"$dir`" & echo. & echo Analyzer finished. & pause"
    try {
        [void](Start-OwnedProcess -FilePath "cmd.exe" -ArgumentList @("/k", $cmd) -VisibleConsole)
    }
    catch {
        Write-LaunchLog "Analyze failed to start: $($_.Exception.Message)"
        return
    }
    Write-LaunchLog "Analyze last finished: $dir"
}

function Invoke-OpenLoadLogs {
    $root = Get-LoadLogsRoot
    if (-not (Test-Path -LiteralPath $root)) {
        New-Item -ItemType Directory -Path $root -Force | Out-Null
    }
    Start-Process explorer.exe $root | Out-Null
}

function Invoke-OpenLastReport {
    $dir = Get-LastFinishedRunDir
    if (-not $dir) {
        [Windows.Forms.MessageBox]::Show("No finished run.", "REPORT", "OK", "Information") | Out-Null
        return
    }
    $report = Join-Path $dir "report"
    if (Test-Path -LiteralPath $report) {
        Start-Process explorer.exe $report | Out-Null
        Write-LaunchLog "Opened last report: $report"
        return
    }
    Start-Process explorer.exe $dir | Out-Null
    Write-LaunchLog "No report/ yet; opened run folder (run_summary.json): $dir"
}

function Update-LoadTestProcess {
    if ($null -eq $script:LoadTest.Process) { return }
    if ($script:LoadTest.Process.HasExited) {
        $code = Get-ProcessExitCode -Process $script:LoadTest.Process
        $script:LoadTest.Process = $null
        $script:LoadTest.State = "Stopped"
        $kind = [string]$script:LoadTest.Kind
        $script:LoadTest.Kind = $null
        $script:LoadTest.StartedAt = $null
        $label = "Load harness"
        if ($kind -eq "runtime-validation") { $label = "Runtime Validation" }
        if ($code -eq 2) {
            Write-LaunchLog "$label exited (2) - CLI parse/usage, not a scenario result. Often a stale purgatory-load.exe (missing --preset). Rebuild purgatory-bot-client and retry."
        }
        elseif ($code -eq 1) {
            Write-LaunchLog "$label exited (1) - scenario FAILED. Server Ready is separate from this result."
        }
        elseif ($code -eq 0) {
            Write-LaunchLog "$label exited (0) - harness completed without FAIL"
        }
        elseif ($code -eq 130) {
            Write-LaunchLog "$label exited (130) - interrupted"
        }
        else {
            Write-LaunchLog "$label exited ($code)"
        }
    }
}
