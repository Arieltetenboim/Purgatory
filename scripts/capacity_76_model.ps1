# Phase 7.6 - single-server capacity model (fit / validate / project).
#
# Calibration: 7.5 ladder cells (player/npc/activity/dense/overlap).
# Holdout: 7.5 canonical_* cells.
# Prefer mean owner ms (not p99) as regression targets.
#
# Usage:
#   ./scripts/capacity_76_model.ps1
#   ./scripts/capacity_76_model.ps1 -CalibPath logs/load/capacity_75/summary_.../phase75_summary.json
param(
    [string]$CalibPath = "",
    [string]$HoldoutPath = "",
    [string]$OutDir = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if ([string]::IsNullOrWhiteSpace($CalibPath)) {
    $CalibPath = Join-Path $root "logs\load\capacity_75\summary_20260901_230222\phase75_summary.json"
}
if ([string]::IsNullOrWhiteSpace($HoldoutPath)) {
    $HoldoutPath = Join-Path $root "logs\load\capacity_75\summary_20260901_232330\phase75_summary.json"
}
$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
if ([string]::IsNullOrWhiteSpace($OutDir)) {
    $OutDir = Join-Path $root "logs\load\capacity_76\model_$stamp"
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$TICK_BUDGET_MS = 1000.0 / 30.0

function Get-DurationSecs([object]$row) {
    $d = [string]$row.duration
    if ($d -match "^(\d+)s$") { return [double]$Matches[1] }
    if ($d -match "^(\d+)m$") { return [double]$Matches[1] * 60.0 }
    if ($null -ne $row.meta_elapsed_secs) { return [double]$row.meta_elapsed_secs }
    return 45.0
}

function Enrich-Cell([object]$row) {
    $secs = Get-DurationSecs $row
    $ticks = 30.0 * $secs
    if ($ticks -le 0) { $ticks = 1.0 }
    $obs = if ($null -ne $row.peak_active) { [double]$row.peak_active } else { 0.0 }
    $npc = if ($null -ne $row.npcs_active) { [double]$row.npcs_active } else { 0.0 }
    $scanned = if ($null -ne $row.repl_scanned) { [double]$row.repl_scanned } else { 0.0 }
    $eligible = if ($null -ne $row.repl_eligible) { [double]$row.repl_eligible } else { 0.0 }
    $emitted = if ($null -ne $row.repl_emitted) { [double]$row.repl_emitted } else { 0.0 }
    $npcUpd = if ($null -ne $row.npc_updates_total) { [double]$row.npc_updates_total } else { 0.0 }
    $moved = if ($null -ne $row.entities_moved) { [double]$row.entities_moved } else { 0.0 }
    $bytes = if ($null -ne $row.repl_bytes) { [double]$row.repl_bytes } else { 0.0 }
    $dense = ($row.scenario -eq "representative-dense") -or ($row.name -like "dense_*") -or ($row.name -like "canonical_dense*")
    [pscustomobject]@{
        name = $row.name
        suite = $row.suite
        scenario = $row.scenario
        observers = $obs
        npcs_active = $npc
        duration_secs = $secs
        ticks = $ticks
        scanned_per_tick = $scanned / $ticks
        eligible_per_tick = $eligible / $ticks
        emitted_per_tick = $emitted / $ticks
        npc_updates_per_tick = $npcUpd / $ticks
        moved_per_tick = $moved / $ticks
        dense = [bool]$dense
        # density proxy: scanned per observer per tick (overlap intensity)
        scan_per_obs = if ($obs -gt 0) { ($scanned / $ticks) / $obs } else { 0.0 }
        tick_mean = [double]$row.tick_mean
        tick_p99 = [double]$row.tick_p99
        tick_util = [double]$row.tick_util
        policy_mean = [double]$row.replication_policy_mean
        npc_mean = [double]$row.npc_activity_mean
        aoi_mean = [double]$row.spatial_aoi_mean
        move_mean = [double]$row.simulation_movement_mean
        unattr_mean = [double]$row.unattributed_mean
        unattr_share = [double]$row.unattributed_share_pct
        repl_children = [double]$row.replication_children_mean
        dominant = $row.dominant
        bytes_out_per_sec = if ($null -ne $row.bytes_out_per_sec) { [double]$row.bytes_out_per_sec } else { 0.0 }
        bytes_out_per_client_s = if ($null -ne $row.bytes_out_per_client_s) { [double]$row.bytes_out_per_client_s } else { 0.0 }
        server_rss_mb = if ($null -ne $row.server_rss_mb) { [double]$row.server_rss_mb } else { 0.0 }
        repl_bytes_total = $bytes
        repl_emitted = $emitted
        bytes_per_emitted = if ($emitted -gt 0) { $bytes / $emitted } else { 0.0 }
    }
}

# --- OLS helpers ---
function Fit-Linear1([double[]]$x, [double[]]$y) {
    $n = $x.Length
    if ($n -lt 2) { throw "need >=2 samples" }
    $sx = 0.0; $sy = 0.0; $sxx = 0.0; $sxy = 0.0
    for ($i = 0; $i -lt $n; $i++) {
        $sx += $x[$i]; $sy += $y[$i]
        $sxx += $x[$i] * $x[$i]
        $sxy += $x[$i] * $y[$i]
    }
    $den = $n * $sxx - $sx * $sx
    if ([math]::Abs($den) -lt 1e-12) { throw "singular 1D fit" }
    $b = ($n * $sxy - $sx * $sy) / $den
    $a = ($sy - $b * $sx) / $n
    return @{ a = $a; b = $b }
}

function Fit-Linear2([double[]]$x1, [double[]]$x2, [double[]]$y) {
    # y = a + b*x1 + c*x2  via normal equations
    $n = $y.Length
    $s1 = 0.0; $s2 = 0.0; $sy = 0.0
    $s11 = 0.0; $s22 = 0.0; $s12 = 0.0; $s1y = 0.0; $s2y = 0.0
    for ($i = 0; $i -lt $n; $i++) {
        $s1 += $x1[$i]; $s2 += $x2[$i]; $sy += $y[$i]
        $s11 += $x1[$i] * $x1[$i]; $s22 += $x2[$i] * $x2[$i]
        $s12 += $x1[$i] * $x2[$i]
        $s1y += $x1[$i] * $y[$i]; $s2y += $x2[$i] * $y[$i]
    }
    # Augmented 3x3: [n s1 s2; s1 s11 s12; s2 s12 s22] [a;b;c] = [sy;s1y;s2y]
    $A = @(
        @($n, $s1, $s2, $sy),
        @($s1, $s11, $s12, $s1y),
        @($s2, $s12, $s22, $s2y)
    )
    for ($col = 0; $col -lt 3; $col++) {
        $piv = $col
        for ($r = $col + 1; $r -lt 3; $r++) {
            if ([math]::Abs($A[$r][$col]) -gt [math]::Abs($A[$piv][$col])) { $piv = $r }
        }
        if ($piv -ne $col) { $tmp = $A[$col]; $A[$col] = $A[$piv]; $A[$piv] = $tmp }
        $div = $A[$col][$col]
        if ([math]::Abs($div) -lt 1e-12) { throw "singular 2D fit" }
        for ($c = $col; $c -lt 4; $c++) { $A[$col][$c] = $A[$col][$c] / $div }
        for ($r = 0; $r -lt 3; $r++) {
            if ($r -eq $col) { continue }
            $f = $A[$r][$col]
            for ($c = $col; $c -lt 4; $c++) { $A[$r][$c] = $A[$r][$c] - $f * $A[$col][$c] }
        }
    }
    return @{ a = $A[0][3]; b = $A[1][3]; c = $A[2][3] }
}

function Get-FitStats([double[]]$y, [double[]]$yhat) {
    $n = $y.Length
    $mean = ($y | Measure-Object -Average).Average
    $ssTot = 0.0; $ssRes = 0.0; $absPct = 0.0; $absErr = 0.0
    $validPct = 0
    for ($i = 0; $i -lt $n; $i++) {
        $e = $y[$i] - $yhat[$i]
        $ssRes += $e * $e
        $ssTot += ($y[$i] - $mean) * ($y[$i] - $mean)
        $absErr += [math]::Abs($e)
        if ([math]::Abs($y[$i]) -gt 1e-9) {
            $absPct += [math]::Abs($e / $y[$i])
            $validPct++
        }
    }
    $r2 = if ($ssTot -gt 1e-18) { 1.0 - $ssRes / $ssTot } else { 0.0 }
    $mape = if ($validPct -gt 0) { 100.0 * $absPct / $validPct } else { 0.0 }
    $mae = $absErr / $n
    return @{ r2 = $r2; mape_pct = $mape; mae = $mae; n = $n }
}

function Predict-Policy($coef, $form, $cell) {
    switch ($form) {
        "observers" { return $coef.a + $coef.b * $cell.observers }
        "scanned" { return $coef.a + $coef.b * $cell.scanned_per_tick }
        "obs_scanned" { return $coef.a + $coef.b * $cell.observers + $coef.c * $cell.scanned_per_tick }
        default { throw "unknown policy form $form" }
    }
}

# --- Load ---
$rawCal = Get-Content $CalibPath -Raw | ConvertFrom-Json
$rawVal = Get-Content $HoldoutPath -Raw | ConvertFrom-Json
$calib = @($rawCal | Where-Object {
    $_.name -like "player_*" -or $_.name -like "npc_*" -or $_.name -like "activity_*" -or
    $_.name -like "dense_*" -or $_.name -like "overlap_*"
} | ForEach-Object { Enrich-Cell $_ })
$holdout = @($rawVal | Where-Object { $_.name -like "canonical_*" } | ForEach-Object { Enrich-Cell $_ })

Write-Host ("Calibration cells: {0}" -f $calib.Count)
Write-Host ("Holdout cells: {0}" -f $holdout.Count)

# --- Policy candidates ---
$yPol = @($calib | ForEach-Object { $_.policy_mean })
$xObs = @($calib | ForEach-Object { $_.observers })
$xScan = @($calib | ForEach-Object { $_.scanned_per_tick })

$c1 = Fit-Linear1 $xObs $yPol
$y1 = @($calib | ForEach-Object { $c1.a + $c1.b * $_.observers })
$s1 = Get-FitStats $yPol $y1

$c2 = Fit-Linear1 $xScan $yPol
$y2 = @($calib | ForEach-Object { $c2.a + $c2.b * $_.scanned_per_tick })
$s2 = Get-FitStats $yPol $y2

$c3 = Fit-Linear2 $xObs $xScan $yPol
$y3 = @($calib | ForEach-Object { $c3.a + $c3.b * $_.observers + $c3.c * $_.scanned_per_tick })
$s3 = Get-FitStats $yPol $y3

Write-Host ""
Write-Host "Policy candidate MAPE: observers=$([math]::Round($s1.mape_pct,2))% R2=$([math]::Round($s1.r2,4))"
Write-Host "Policy candidate MAPE: scanned=$([math]::Round($s2.mape_pct,2))% R2=$([math]::Round($s2.r2,4))"
Write-Host "Policy candidate MAPE: obs+scanned=$([math]::Round($s3.mape_pct,2))% R2=$([math]::Round($s3.r2,4))"

# Pick simplest adequate: prefer lower MAPE; if within 5pp of best, prefer fewer params
$cands = @(
    @{ form = "observers"; coef = $c1; stats = $s1; params = 2 },
    @{ form = "scanned"; coef = $c2; stats = $s2; params = 2 },
    @{ form = "obs_scanned"; coef = $c3; stats = $s3; params = 3 }
)
$best = $cands | Sort-Object { $_.stats.mape_pct } | Select-Object -First 1
$simple = $cands | Where-Object { $_.params -eq 2 } | Sort-Object { $_.stats.mape_pct } | Select-Object -First 1
if ($simple.stats.mape_pct -le ($best.stats.mape_pct + 5.0)) {
    $policyPick = $simple
} else {
    $policyPick = $best
}
Write-Host ("Chosen policy form: {0} (MAPE {1:N2}%, R2 {2:N4})" -f $policyPick.form, $policyPick.stats.mape_pct, $policyPick.stats.r2)

# --- Full replication children (discover+policy+encode+enqueue) for tick composition ---
$yRepl = @($calib | ForEach-Object { $_.repl_children })
$r1 = Fit-Linear1 $xObs $yRepl
$ry1 = @($calib | ForEach-Object { $r1.a + $r1.b * $_.observers })
$rs1 = Get-FitStats $yRepl $ry1
$r2c = Fit-Linear1 $xScan $yRepl
$ry2 = @($calib | ForEach-Object { $r2c.a + $r2c.b * $_.scanned_per_tick })
$rs2 = Get-FitStats $yRepl $ry2
$r3 = Fit-Linear2 $xObs $xScan $yRepl
$ry3 = @($calib | ForEach-Object { $r3.a + $r3.b * $_.observers + $r3.c * $_.scanned_per_tick })
$rs3 = Get-FitStats $yRepl $ry3
$replCands = @(
    @{ form = "observers"; coef = $r1; stats = $rs1; params = 2 },
    @{ form = "scanned"; coef = $r2c; stats = $rs2; params = 2 },
    @{ form = "obs_scanned"; coef = $r3; stats = $rs3; params = 3 }
)
$replBest = $replCands | Sort-Object { $_.stats.mape_pct } | Select-Object -First 1
$replSimple = $replCands | Where-Object { $_.params -eq 2 } | Sort-Object { $_.stats.mape_pct } | Select-Object -First 1
if ($replSimple.stats.mape_pct -le ($replBest.stats.mape_pct + 5.0)) {
    $replPick = $replSimple
} else {
    $replPick = $replBest
}
Write-Host ("Chosen repl_children form: {0} (MAPE {1:N2}%, R2 {2:N4})" -f $replPick.form, $replPick.stats.mape_pct, $replPick.stats.r2)

function Predict-ReplChildren($coef, $form, $cell) {
    switch ($form) {
        "observers" { return $coef.a + $coef.b * $cell.observers }
        "scanned" { return $coef.a + $coef.b * $cell.scanned_per_tick }
        "obs_scanned" { return $coef.a + $coef.b * $cell.observers + $coef.c * $cell.scanned_per_tick }
        default { throw "unknown repl form $form" }
    }
}

# --- NPC ---
$yNpc = @($calib | ForEach-Object { $_.npc_mean })
$xNpcUpd = @($calib | ForEach-Object { $_.npc_updates_per_tick })
$xNpcCnt = @($calib | ForEach-Object { $_.npcs_active })
$npcUpdFit = Fit-Linear1 $xNpcUpd $yNpc
$npcCntFit = Fit-Linear1 $xNpcCnt $yNpc
$npcY1 = @($calib | ForEach-Object { $npcUpdFit.a + $npcUpdFit.b * $_.npc_updates_per_tick })
$npcY2 = @($calib | ForEach-Object { $npcCntFit.a + $npcCntFit.b * $_.npcs_active })
$npcS1 = Get-FitStats $yNpc $npcY1
$npcS2 = Get-FitStats $yNpc $npcY2
# Prefer updates_per_tick when MAPE within 5pp (plan preferred form).
if ($npcS1.mape_pct -le ($npcS2.mape_pct + 5.0)) {
    $npcPick = @{ form = "updates_per_tick"; coef = $npcUpdFit; stats = $npcS1 }
} else {
    $npcPick = @{ form = "npcs_active"; coef = $npcCntFit; stats = $npcS2 }
}
Write-Host ("Chosen npc form: {0} (MAPE {1:N2}%, R2 {2:N4})" -f $npcPick.form, $npcPick.stats.mape_pct, $npcPick.stats.r2)

# --- AOI: vs moved_per_tick and vs observers ---
$yAoi = @($calib | ForEach-Object { $_.aoi_mean })
$xMoved = @($calib | ForEach-Object { $_.moved_per_tick })
$aoiMoved = Fit-Linear1 $xMoved $yAoi
$aoiObs = Fit-Linear1 $xObs $yAoi
$aoiY1 = @($calib | ForEach-Object { $aoiMoved.a + $aoiMoved.b * $_.moved_per_tick })
$aoiY2 = @($calib | ForEach-Object { $aoiObs.a + $aoiObs.b * $_.observers })
$aoiS1 = Get-FitStats $yAoi $aoiY1
$aoiS2 = Get-FitStats $yAoi $aoiY2
if ($aoiS2.mape_pct -le $aoiS1.mape_pct) {
    $aoiPick = @{ form = "observers"; coef = $aoiObs; stats = $aoiS2; keep = $true }
} else {
    $aoiPick = @{ form = "moved_per_tick"; coef = $aoiMoved; stats = $aoiS1; keep = $true }
}
if ($aoiPick.stats.r2 -lt 0.5) { $aoiPick.keep = $false }
Write-Host ("AOI form: {0} keep={1} MAPE={2:N2}% R2={3:N4}" -f $aoiPick.form, $aoiPick.keep, $aoiPick.stats.mape_pct, $aoiPick.stats.r2)

# --- Movement: light fit vs observers ---
$yMove = @($calib | ForEach-Object { $_.move_mean })
$moveFit = Fit-Linear1 $xObs $yMove
$moveY = @($calib | ForEach-Object { $moveFit.a + $moveFit.b * $_.observers })
$moveS = Get-FitStats $yMove $moveY
Write-Host ("Movement vs observers: MAPE={0:N2}% R2={1:N4}" -f $moveS.mape_pct, $moveS.r2)

# --- Residual: fit unattr_mean vs observers (grows with N; do not bake into other coeffs) ---
$yUn = @($calib | ForEach-Object { $_.unattr_mean })
$unFit = Fit-Linear1 $xObs $yUn
$unY = @($calib | ForEach-Object { $unFit.a + $unFit.b * $_.observers })
$unS = Get-FitStats $yUn $unY
$unattrMean = ($calib | ForEach-Object { $_.unattr_mean } | Measure-Object -Average).Average
$unattrShare = ($calib | ForEach-Object { $_.unattr_share } | Measure-Object -Average).Average
Write-Host ("Residual model: unattr={0:N4}+{1:N6}*obs MAPE={2:N1}% R2={3:N3} (mean share~{4:N1}%)" -f $unFit.a, $unFit.b, $unS.mape_pct, $unS.r2, $unattrShare)

function Predict-Owners($cell) {
    $pol = Predict-Policy $policyPick.coef $policyPick.form $cell
    $repl = Predict-ReplChildren $replPick.coef $replPick.form $cell
    $npc = if ($npcPick.form -eq "updates_per_tick") {
        $npcPick.coef.a + $npcPick.coef.b * $cell.npc_updates_per_tick
    } else {
        $npcPick.coef.a + $npcPick.coef.b * $cell.npcs_active
    }
    $aoi = if ($aoiPick.keep) {
        if ($aoiPick.form -eq "observers") { $aoiPick.coef.a + $aoiPick.coef.b * $cell.observers }
        else { $aoiPick.coef.a + $aoiPick.coef.b * $cell.moved_per_tick }
    } else { 0.0 }
    $move = $moveFit.a + $moveFit.b * $cell.observers
    $residual = [math]::Max(0.0, $unFit.a + $unFit.b * $cell.observers)
    # Tick uses full replication children (not policy alone) + residual model kept separate.
    $modeled = [math]::Max(0.0, $repl) + [math]::Max(0.0, $npc) + [math]::Max(0.0, $aoi) + [math]::Max(0.0, $move)
    $tick = $modeled + $residual
    [pscustomobject]@{
        policy = $pol
        repl_children = $repl
        npc = $npc
        aoi = $aoi
        move = $move
        residual = $residual
        tick_mean = $tick
        tick_util = 100.0 * $tick / $TICK_BUDGET_MS
        dominant = if ($pol -ge $npc -and $pol -ge $aoi -and $pol -ge $move) { "replication_policy" }
            elseif ($npc -ge $pol -and $npc -ge $aoi) { "npc_activity" }
            elseif ($aoi -ge $move) { "spatial_aoi" }
            else { "simulation_movement" }
    }
}

# Calibration residuals for whole-tick
$tickPredCal = @()
foreach ($cell in $calib) {
    $p = Predict-Owners $cell
    $tickPredCal += [pscustomobject]@{
        name = $cell.name
        measured_tick = $cell.tick_mean
        predicted_tick = $p.tick_mean
        measured_policy = $cell.policy_mean
        predicted_policy = $p.policy
        measured_npc = $cell.npc_mean
        predicted_npc = $p.npc
        err_tick = $cell.tick_mean - $p.tick_mean
        err_pct_tick = if ($cell.tick_mean -gt 0) { 100.0 * ($cell.tick_mean - $p.tick_mean) / $cell.tick_mean } else { 0.0 }
    }
}
$calTickStats = Get-FitStats (@($tickPredCal | ForEach-Object { $_.measured_tick })) (@($tickPredCal | ForEach-Object { $_.predicted_tick }))
$calPolStats = Get-FitStats (@($tickPredCal | ForEach-Object { $_.measured_policy })) (@($tickPredCal | ForEach-Object { $_.predicted_policy }))
$calNpcStats = Get-FitStats (@($tickPredCal | ForEach-Object { $_.measured_npc })) (@($tickPredCal | ForEach-Object { $_.predicted_npc }))
Write-Host ""
Write-Host ("Calib whole-tick MAPE={0:N2}% R2={1:N4}" -f $calTickStats.mape_pct, $calTickStats.r2)
Write-Host ("Calib policy MAPE={0:N2}% R2={1:N4}" -f $calPolStats.mape_pct, $calPolStats.r2)
Write-Host ("Calib npc MAPE={0:N2}% R2={1:N4}" -f $calNpcStats.mape_pct, $calNpcStats.r2)

# --- Holdout validation ---
$valRows = @()
foreach ($cell in $holdout) {
    $p = Predict-Owners $cell
    $valRows += [pscustomobject]@{
        name = $cell.name
        observers = $cell.observers
        npcs_active = $cell.npcs_active
        measured_tick_mean = $cell.tick_mean
        predicted_tick_mean = [math]::Round($p.tick_mean, 4)
        measured_tick_p99 = $cell.tick_p99
        p99_over_mean = if ($cell.tick_mean -gt 0) { [math]::Round($cell.tick_p99 / $cell.tick_mean, 3) } else { 0.0 }
        measured_util = $cell.tick_util
        predicted_util = [math]::Round($p.tick_util, 3)
        measured_policy = $cell.policy_mean
        predicted_policy = [math]::Round($p.policy, 4)
        measured_npc = $cell.npc_mean
        predicted_npc = [math]::Round($p.npc, 4)
        measured_dominant = $cell.dominant
        predicted_dominant = $p.dominant
        err_tick_pct = if ($cell.tick_mean -gt 0) { [math]::Round(100.0 * ($p.tick_mean - $cell.tick_mean) / $cell.tick_mean, 2) } else { 0.0 }
        err_policy_pct = if ($cell.policy_mean -gt 0) { [math]::Round(100.0 * ($p.policy - $cell.policy_mean) / $cell.policy_mean, 2) } else { 0.0 }
        err_npc_pct = if ($cell.npc_mean -gt 0) { [math]::Round(100.0 * ($p.npc - $cell.npc_mean) / $cell.npc_mean, 2) } else { 0.0 }
    }
}
$valTickStats = Get-FitStats (@($valRows | ForEach-Object { $_.measured_tick_mean })) (@($valRows | ForEach-Object { $_.predicted_tick_mean }))
$valPolStats = Get-FitStats (@($valRows | ForEach-Object { $_.measured_policy })) (@($valRows | ForEach-Object { $_.predicted_policy }))
$valNpcStats = Get-FitStats (@($valRows | ForEach-Object { $_.measured_npc })) (@($valRows | ForEach-Object { $_.predicted_npc }))
Write-Host ""
Write-Host "=== HOLDOUT VALIDATION ==="
$valRows | Format-Table name, measured_tick_mean, predicted_tick_mean, err_tick_pct, measured_policy, predicted_policy, err_policy_pct, measured_npc, predicted_npc, err_npc_pct -AutoSize | Out-String | Write-Host
Write-Host ("Holdout tick MAPE={0:N2}% policy={1:N2}% npc={2:N2}%" -f $valTickStats.mape_pct, $valPolStats.mape_pct, $valNpcStats.mape_pct)

# p99/mean ratios
$p99Ratios = @($calib + $holdout | ForEach-Object {
    if ($_.tick_mean -gt 0) { $_.tick_p99 / $_.tick_mean } else { 0.0 }
} | Where-Object { $_ -gt 0 })
$p99RatioMean = ($p99Ratios | Measure-Object -Average).Average
$p99RatioMed = ($p99Ratios | Sort-Object)[[int]([math]::Floor($p99Ratios.Count / 2))]

# --- Network / memory side models ---
$netBytesPerClient = @($calib | Where-Object { $_.bytes_out_per_client_s -gt 0 } | ForEach-Object { $_.bytes_out_per_client_s })
$netAvgBpsClient = if ($netBytesPerClient.Count -gt 0) { ($netBytesPerClient | Measure-Object -Average).Average } else { 0.0 }
$bytesPerEmitted = @($calib | Where-Object { $_.bytes_per_emitted -gt 0 } | ForEach-Object { $_.bytes_per_emitted })
$avgBytesPerEmitted = if ($bytesPerEmitted.Count -gt 0) { ($bytesPerEmitted | Measure-Object -Average).Average } else { 0.0 }

$rssY = @($calib | Where-Object { $_.server_rss_mb -gt 0 } | ForEach-Object { $_.server_rss_mb })
$rssP = @($calib | Where-Object { $_.server_rss_mb -gt 0 } | ForEach-Object { $_.observers })
$rssN = @($calib | Where-Object { $_.server_rss_mb -gt 0 } | ForEach-Object { $_.npcs_active })
$memFit = $null
$memStats = $null
if ($rssY.Count -ge 4) {
    $memFit = Fit-Linear2 $rssP $rssN $rssY
    $memYhat = for ($i = 0; $i -lt $rssY.Count; $i++) { $memFit.a + $memFit.b * $rssP[$i] + $memFit.c * $rssN[$i] }
    $memStats = Get-FitStats $rssY $memYhat
    Write-Host ("Memory: rss~{0:N2}+{1:N4}*P+{2:N4}*NPC  MAPE={3:N1}% R2={4:N3}" -f $memFit.a, $memFit.b, $memFit.c, $memStats.mape_pct, $memStats.r2)
}

# --- Projection helpers ---
function New-SyntheticCell {
    param([int]$Players, [int]$Npcs, [double]$ScanPerObs, [double]$NpcUpdPerNpc = 1.0, [double]$MovedPerPlayer = 0.5, [switch]$Dense)
    $obs = [double]$Players
    $npc = [double]$Npcs
    $scanned = $obs * $ScanPerObs
    $upd = $npc * $NpcUpdPerNpc
    $moved = $obs * $MovedPerPlayer + $npc * 0.3
    [pscustomobject]@{
        name = "proj_p${Players}_n${Npcs}"
        observers = $obs
        npcs_active = $npc
        scanned_per_tick = $scanned
        eligible_per_tick = $scanned
        emitted_per_tick = $scanned * 0.18
        npc_updates_per_tick = $upd
        moved_per_tick = $moved
        dense = [bool]$Dense
        scan_per_obs = $ScanPerObs
        # placeholders for Enrich compatibility
        tick_mean = 0; tick_p99 = 0; tick_util = 0
        policy_mean = 0; npc_mean = 0; aoi_mean = 0; move_mean = 0
        unattr_mean = 0; unattr_share = 0; repl_children = 0
        dominant = ""; bytes_out_per_sec = 0; bytes_out_per_client_s = 0
        server_rss_mb = 0; repl_bytes_total = 0; repl_emitted = 0; bytes_per_emitted = 0
        duration_secs = 45; ticks = 1350; suite = "proj"; scenario = "synthetic"
    }
}

# Empirical scan_per_obs from calib
$mixedScan = @($calib | Where-Object { -not $_.dense } | ForEach-Object { $_.scan_per_obs } | Measure-Object -Average).Average
$denseScan = @($calib | Where-Object { $_.dense } | ForEach-Object { $_.scan_per_obs } | Measure-Object -Average).Average
$npcUpdPerNpc = @($calib | Where-Object { $_.npcs_active -gt 0 } | ForEach-Object {
    $_.npc_updates_per_tick / $_.npcs_active
} | Measure-Object -Average).Average
Write-Host ("Empirical scan/obs: mixed={0:N2} dense={1:N2}; npc_upd/npc={2:N3}" -f $mixedScan, $denseScan, $npcUpdPerNpc)

$projections = @()
# Player-heavy
foreach ($p in @(64, 128, 192, 256)) {
    $cell = New-SyntheticCell -Players $p -Npcs 24 -ScanPerObs $mixedScan -NpcUpdPerNpc $npcUpdPerNpc
    $pred = Predict-Owners $cell
    $band = if ($p -le 128) { "measured_shape" } elseif ($p -le 192) { "limited_extrapolation" } else { "limited_extrapolation" }
    $projections += [pscustomobject]@{
        scenario = "player_heavy_mod_npc"
        band = $band
        players = $p
        npcs = 24
        density = "mixed"
        pred_tick_mean = [math]::Round($pred.tick_mean, 3)
        pred_util = [math]::Round($pred.tick_util, 2)
        pred_dominant = $pred.dominant
        pred_policy = [math]::Round($pred.policy, 3)
        pred_npc = [math]::Round($pred.npc, 3)
        pred_bytes_out_s = [math]::Round($netAvgBpsClient * $p, 0)
        pred_rss_mb = if ($memFit) { [math]::Round($memFit.a + $memFit.b * $p + $memFit.c * 24, 2) } else { $null }
        confidence = if ($p -le 128) { "high" } elseif ($p -le 192) { "medium" } else { "low" }
        note = "model estimate; nearest measured mixed@$([Math]::Min($p,128))"
    }
}
# NPC-heavy
foreach ($n in @(48, 96, 128, 192)) {
    $cell = New-SyntheticCell -Players 8 -Npcs $n -ScanPerObs $mixedScan -NpcUpdPerNpc $npcUpdPerNpc
    $pred = Predict-Owners $cell
    $band = if ($n -le 96) { "measured_shape" } else { "limited_extrapolation" }
    $projections += [pscustomobject]@{
        scenario = "npc_heavy"
        band = $band
        players = 8
        npcs = $n
        density = "mixed"
        pred_tick_mean = [math]::Round($pred.tick_mean, 3)
        pred_util = [math]::Round($pred.tick_util, 2)
        pred_dominant = $pred.dominant
        pred_policy = [math]::Round($pred.policy, 3)
        pred_npc = [math]::Round($pred.npc, 3)
        pred_bytes_out_s = [math]::Round($netAvgBpsClient * 8, 0)
        pred_rss_mb = if ($memFit) { [math]::Round($memFit.a + $memFit.b * 8 + $memFit.c * $n, 2) } else { $null }
        confidence = if ($n -le 96) { "high" } else { "medium" }
        note = "model estimate; nearest measured npc@$([Math]::Min($n,96)) @8p"
    }
}
# Dense
foreach ($p in @(32, 64, 96, 128)) {
    $cell = New-SyntheticCell -Players $p -Npcs 64 -ScanPerObs $denseScan -NpcUpdPerNpc $npcUpdPerNpc -Dense
    $pred = Predict-Owners $cell
    $band = if ($p -le 64) { "measured_shape" } else { "limited_extrapolation" }
    $projections += [pscustomobject]@{
        scenario = "dense_hotspot"
        band = $band
        players = $p
        npcs = 64
        density = "dense"
        pred_tick_mean = [math]::Round($pred.tick_mean, 3)
        pred_util = [math]::Round($pred.tick_util, 2)
        pred_dominant = $pred.dominant
        pred_policy = [math]::Round($pred.policy, 3)
        pred_npc = [math]::Round($pred.npc, 3)
        pred_bytes_out_s = [math]::Round($netAvgBpsClient * $p * 1.5, 0)
        pred_rss_mb = if ($memFit) { [math]::Round($memFit.a + $memFit.b * $p + $memFit.c * 64, 2) } else { $null }
        confidence = if ($p -le 64) { "high" } else { "low" }
        note = "model estimate; nearest measured dense@$([Math]::Min($p,64))"
    }
}
# Find util ~50% and ~80% on player-heavy axis (model estimate)
$utilTargets = @()
foreach ($target in @(50.0, 80.0)) {
    $found = $null
    for ($p = 64; $p -le 2000; $p += 16) {
        $cell = New-SyntheticCell -Players $p -Npcs 24 -ScanPerObs $mixedScan -NpcUpdPerNpc $npcUpdPerNpc
        $pred = Predict-Owners $cell
        if ($pred.tick_util -ge $target) {
            $found = [pscustomobject]@{
                target_util_pct = $target
                players = $p
                npcs = 24
                pred_util = [math]::Round($pred.tick_util, 2)
                pred_tick_mean = [math]::Round($pred.tick_mean, 3)
                pred_dominant = $pred.dominant
                band = "unsupported_far_extrapolation"
                confidence = "very_low"
                note = "MODEL ONLY - far beyond measured envelope (max mixed@128); not a capacity claim"
            }
            break
        }
    }
    if ($found) { $utilTargets += $found }
}

# --- Sensitivity at mixed@64 baseline ---
$base = New-SyntheticCell -Players 64 -Npcs 24 -ScanPerObs $mixedScan -NpcUpdPerNpc $npcUpdPerNpc
$basePred = Predict-Owners $base
$sens = @()
$sensCases = @(
    @{ name = "+10% players"; cell = (New-SyntheticCell -Players 70 -Npcs 24 -ScanPerObs $mixedScan -NpcUpdPerNpc $npcUpdPerNpc) },
    @{ name = "+10% NPCs"; cell = (New-SyntheticCell -Players 64 -Npcs 26 -ScanPerObs $mixedScan -NpcUpdPerNpc $npcUpdPerNpc) },
    @{ name = "+10% overlap (scan/obs)"; cell = (New-SyntheticCell -Players 64 -Npcs 24 -ScanPerObs ($mixedScan * 1.1) -NpcUpdPerNpc $npcUpdPerNpc) },
    @{ name = "+10% activity (npc upd rate)"; cell = (New-SyntheticCell -Players 64 -Npcs 24 -ScanPerObs $mixedScan -NpcUpdPerNpc ($npcUpdPerNpc * 1.1)) }
)
foreach ($sc in $sensCases) {
    $pr = Predict-Owners $sc.cell
    $sens += [pscustomobject]@{
        perturbation = $sc.name
        base_tick = [math]::Round($basePred.tick_mean, 4)
        new_tick = [math]::Round($pr.tick_mean, 4)
        delta_tick_ms = [math]::Round($pr.tick_mean - $basePred.tick_mean, 4)
        delta_pct = if ($basePred.tick_mean -gt 0) { [math]::Round(100.0 * ($pr.tick_mean - $basePred.tick_mean) / $basePred.tick_mean, 2) } else { 0.0 }
        base_dominant = $basePred.dominant
        new_dominant = $pr.dominant
    }
}
Write-Host ""
Write-Host "=== SENSITIVITY (mixed@64 shape) ==="
$sens | Format-Table -AutoSize | Out-String | Write-Host

# --- Falsification predictions ---
$falsifyPred = @(
    @{ name = "falsify_mixed48"; players = 48; npcs = 24; scan = $mixedScan; upd = $npcUpdPerNpc; dense = $false; scenario = "representative-mixed"; action = -1; pulse = -1 },
    @{ name = "falsify_mixed16_npc96"; players = 16; npcs = 96; scan = $mixedScan; upd = $npcUpdPerNpc; dense = $false; scenario = "representative-mixed"; action = -1; pulse = -1 },
    @{ name = "falsify_mixed32_a4p6"; players = 32; npcs = 24; scan = $mixedScan; upd = ($npcUpdPerNpc * 1.2); dense = $false; scenario = "representative-mixed"; action = 4; pulse = 6 },
    @{ name = "falsify_dense48"; players = 48; npcs = 64; scan = $denseScan; upd = $npcUpdPerNpc; dense = $true; scenario = "representative-dense"; action = -1; pulse = -1 }
)
$falsifyOut = @()
foreach ($f in $falsifyPred) {
    $cell = New-SyntheticCell -Players $f.players -Npcs $f.npcs -ScanPerObs $f.scan -NpcUpdPerNpc $f.upd -Dense:$f.dense
    $pr = Predict-Owners $cell
    $falsifyOut += [pscustomobject]@{
        name = $f.name
        scenario = $f.scenario
        players = $f.players
        npcs = $f.npcs
        action_period = $f.action
        pulse_period = $f.pulse
        predicted_tick_mean = [math]::Round($pr.tick_mean, 4)
        predicted_util = [math]::Round($pr.tick_util, 3)
        predicted_policy = [math]::Round($pr.policy, 4)
        predicted_npc = [math]::Round($pr.npc, 4)
        predicted_dominant = $pr.dominant
        predicted_p99_est = [math]::Round($pr.tick_mean * $p99RatioMean, 4)
        p99_note = "p99_est = mean * empirical_ratio; not a direct regression"
    }
}
Write-Host "=== FALSIFY PREDICTIONS ==="
$falsifyOut | Format-Table name, predicted_tick_mean, predicted_util, predicted_policy, predicted_npc, predicted_dominant -AutoSize | Out-String | Write-Host

# Measured envelope table rows
$envelope = @()
foreach ($cell in ($calib + $holdout)) {
    $envelope += [pscustomobject]@{
        scenario = $cell.name
        kind = if ($cell.name -like "canonical_*") { "measured_holdout" } else { "measured_calibration" }
        players = [int]$cell.observers
        npcs = [int]$cell.npcs_active
        density = if ($cell.dense) { "dense" } else { "mixed" }
        tick_util = [math]::Round($cell.tick_util, 2)
        tick_mean = [math]::Round($cell.tick_mean, 3)
        tick_p99 = [math]::Round($cell.tick_p99, 3)
        dominant = $cell.dominant
        bytes_out_per_sec = [math]::Round($cell.bytes_out_per_sec, 0)
        rss_mb = [math]::Round($cell.server_rss_mb, 2)
        confidence = "measured"
    }
}

$model = [ordered]@{
    schema = 1
    phase = "7.6"
    tick_budget_ms = $TICK_BUDGET_MS
    calib_path = $CalibPath
    holdout_path = $HoldoutPath
    calib_n = $calib.Count
    holdout_n = $holdout.Count
    policy = [ordered]@{
        form = $policyPick.form
        a = $policyPick.coef.a
        b = $policyPick.coef.b
        c = if ($policyPick.coef.c) { $policyPick.coef.c } else { $null }
        calib_mape_pct = [math]::Round($policyPick.stats.mape_pct, 3)
        calib_r2 = [math]::Round($policyPick.stats.r2, 5)
        candidates = @(
            @{ form = "observers"; mape = [math]::Round($s1.mape_pct, 3); r2 = [math]::Round($s1.r2, 5); a = $c1.a; b = $c1.b },
            @{ form = "scanned"; mape = [math]::Round($s2.mape_pct, 3); r2 = [math]::Round($s2.r2, 5); a = $c2.a; b = $c2.b },
            @{ form = "obs_scanned"; mape = [math]::Round($s3.mape_pct, 3); r2 = [math]::Round($s3.r2, 5); a = $c3.a; b = $c3.b; c = $c3.c }
        )
    }
    npc = [ordered]@{
        form = $npcPick.form
        a = $npcPick.coef.a
        b = $npcPick.coef.b
        calib_mape_pct = [math]::Round($npcPick.stats.mape_pct, 3)
        calib_r2 = [math]::Round($npcPick.stats.r2, 5)
        alt_form = if ($npcPick.form -eq "updates_per_tick") { "npcs_active" } else { "updates_per_tick" }
        alt_mape = if ($npcPick.form -eq "updates_per_tick") { [math]::Round($npcS2.mape_pct, 3) } else { [math]::Round($npcS1.mape_pct, 3) }
    }
    aoi = [ordered]@{
        kept = $aoiPick.keep
        form = $aoiPick.form
        a = $aoiPick.coef.a
        b = $aoiPick.coef.b
        calib_mape_pct = [math]::Round($aoiPick.stats.mape_pct, 3)
        calib_r2 = [math]::Round($aoiPick.stats.r2, 5)
    }
    movement = [ordered]@{
        form = "observers"
        a = $moveFit.a
        b = $moveFit.b
        calib_mape_pct = [math]::Round($moveS.mape_pct, 3)
        calib_r2 = [math]::Round($moveS.r2, 5)
    }
    residual_allowance_ms = [math]::Round($unattrMean, 4)
    residual_share_pct_mean = [math]::Round($unattrShare, 2)
    residual_model = [ordered]@{
        form = "observers"
        a = $unFit.a
        b = $unFit.b
        calib_mape_pct = [math]::Round($unS.mape_pct, 3)
        calib_r2 = [math]::Round($unS.r2, 5)
        note = "Separate residual; not folded into owner coefficients"
    }
    repl_children = [ordered]@{
        form = $replPick.form
        a = $replPick.coef.a
        b = $replPick.coef.b
        c = if ($replPick.coef.c) { $replPick.coef.c } else { $null }
        calib_mape_pct = [math]::Round($replPick.stats.mape_pct, 3)
        calib_r2 = [math]::Round($replPick.stats.r2, 5)
        note = "Used in whole-tick sum (policy is the reported sub-owner)"
    }
    whole_tick = [ordered]@{
        formula = "tick_mean ~= max(0,repl_children)+max(0,npc)+max(0,aoi)+max(0,move)+residual(obs)"
        calib_mape_pct = [math]::Round($calTickStats.mape_pct, 3)
        calib_r2 = [math]::Round($calTickStats.r2, 5)
    }
    holdout = [ordered]@{
        tick_mape_pct = [math]::Round($valTickStats.mape_pct, 3)
        policy_mape_pct = [math]::Round($valPolStats.mape_pct, 3)
        npc_mape_pct = [math]::Round($valNpcStats.mape_pct, 3)
        rows = $valRows
    }
    p99_mean_ratio = [ordered]@{
        mean = [math]::Round($p99RatioMean, 4)
        median = [math]::Round($p99RatioMed, 4)
        note = "Do not treat mean-model as p99 predictor; use ratio only as rough scale"
    }
    empirics = [ordered]@{
        mixed_scan_per_obs = [math]::Round($mixedScan, 4)
        dense_scan_per_obs = [math]::Round($denseScan, 4)
        npc_updates_per_npc = [math]::Round($npcUpdPerNpc, 4)
        bytes_per_client_s = [math]::Round($netAvgBpsClient, 2)
        bytes_per_emitted = [math]::Round($avgBytesPerEmitted, 2)
    }
    memory = if ($memFit) {
        [ordered]@{
            available = $true
            a = $memFit.a; b = $memFit.b; c = $memFit.c
            mape_pct = [math]::Round($memStats.mape_pct, 3)
            r2 = [math]::Round($memStats.r2, 5)
            formula = "rss_mb ~= a + b*players + c*npcs"
        }
    } else {
        [ordered]@{ available = $false; note = "insufficient stable samples" }
    }
    sensitivity = $sens
    projections = $projections
    util_extrapolation = $utilTargets
    falsify_predictions = $falsifyOut
    envelope_measured = $envelope
    architecture_checkpoint = [ordered]@{
        preliminary = "Current headroom sufficient in measured envelope; replication_policy likely first limiter under player/density growth; no redesign in 7.6"
    }
}

$modelPath = Join-Path $OutDir "phase76_model.json"
($model | ConvertTo-Json -Depth 8) | Set-Content $modelPath
$falsifyOut | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $OutDir "falsify_predictions.json")
$valRows | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $OutDir "holdout_validation.json")
$projections | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $OutDir "projections.json")
$sens | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $OutDir "sensitivity.json")

Write-Host ""
Write-Host "Model written: $modelPath"
Write-Host "OutDir: $OutDir"
