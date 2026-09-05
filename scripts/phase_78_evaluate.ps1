# Phase 7.8 — evaluate one cell directory against absolute + baseline thresholds.
# Dot-sourced by phase_78_gate.ps1 (or invoke with -CellDir / -CellId for debug).

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Read-78JsonFile {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { return $null }
    return (Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json)
}

function Get-78Prop {
    param($Obj, [string]$Name, $Default = $null)
    if ($null -eq $Obj) { return $Default }
    $p = $Obj.PSObject.Properties[$Name]
    if ($null -eq $p) { return $Default }
    return $p.Value
}

function Get-78DurationSecs {
    param([string]$Duration)
    if ($Duration -match "^(\d+)s$") { return [double]$Matches[1] }
    if ($Duration -match "^(\d+)m$") { return [double]$Matches[1] * 60.0 }
    if ($Duration -match "^(\d+)h$") { return [double]$Matches[1] * 3600.0 }
    return $null
}

function Read-78CellMetrics {
    param(
        [string]$Path,
        [string]$CellId,
        [string]$Duration
    )
    $row = [ordered]@{
        cell_id = $CellId
        path = $Path
        duration = $Duration
    }
    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) {
        $row.artifacts_present = $false
        return [pscustomobject]$row
    }
    $row.artifacts_present = $true

    $meta = Join-Path $Path "ladder_meta.txt"
    if (Test-Path -LiteralPath $meta) {
        Get-Content -LiteralPath $meta | ForEach-Object {
            if ($_ -match "^(\w+)=(.*)$") { $row["meta_$($Matches[1])"] = $Matches[2] }
        }
    }

    $ramp = Read-78JsonFile (Join-Path $Path "connection_ramp.json")
    if ($ramp) {
        $row.spawn_requested = Get-78Prop $ramp "requested_clients"
        $row.spawn_issued = Get-78Prop $ramp "spawn_issued_total"
        $row.transport_established = Get-78Prop $ramp "transport_established_total" (Get-78Prop $ramp "quic_connected_total")
        $row.welcome_total = Get-78Prop $ramp "welcome_total"
        $row.world_entered = Get-78Prop $ramp "world_entered_total" (Get-78Prop $ramp "entered_world_total")
        $row.peak_active = Get-78Prop $ramp "peak_active_clients"
        $row.attainment_pct = Get-78Prop $ramp "attainment_pct"
        $row.funnel_invariant_ok = Get-78Prop $ramp "funnel_invariant_ok"
        $row.controller_tick_p99_ms = Get-78Prop $ramp "controller_tick_p99_ms"
        $row.admission_refused = Get-78Prop $ramp "admission_refused"
    }

    $td = Read-78JsonFile (Join-Path $Path "tick_domains.json")
    if ($td) {
        $tick = Get-78Prop $td "tick_total"
        $row.tick_mean = Get-78Prop $tick "mean_ms"
        $row.tick_p50 = Get-78Prop $tick "p50_ms"
        $row.tick_p95 = Get-78Prop $tick "p95_ms"
        $row.tick_p99 = Get-78Prop $tick "p99_ms"
        $row.tick_max = Get-78Prop $tick "max_ms"
        $row.tick_util = Get-78Prop $td "tick_utilization_pct"
        $row.overruns = Get-78Prop $td "tick_overrun_count"
        $row.consecutive_overrun_streak = Get-78Prop $td "consecutive_overrun_streak"
        $row.dominant = Get-78Prop $td "dominant_owner"
        $row.worst_spike = Get-78Prop $td "worst_spike_owner"
        $row.accounting_error_ticks = Get-78Prop $td "accounting_error_ticks"
        $un = Get-78Prop $td "unattributed"
        $row.unattributed_mean = Get-78Prop $un "mean_ms"
        $row.unattributed_p99 = Get-78Prop $un "p99_ms"
        if ($null -ne $row.tick_mean -and [double]$row.tick_mean -gt 0 -and $null -ne $row.unattributed_mean) {
            $row.unattributed_share_pct = [math]::Round(100.0 * [double]$row.unattributed_mean / [double]$row.tick_mean, 2)
        }
        foreach ($owner in @(
            "npc_activity", "spatial_aoi", "replication",
            "replication_discover", "replication_policy", "replication_encode", "replication_enqueue",
            "scheduler", "actions", "effects", "entity_lifecycle", "cadence"
        )) {
            $o = Get-78Prop $td $owner
            if ($null -ne $o) {
                $row["${owner}_mean"] = Get-78Prop $o "mean_ms"
                $row["${owner}_p95"] = Get-78Prop $o "p95_ms"
                $row["${owner}_p99"] = Get-78Prop $o "p99_ms"
                if ($null -ne $row.tick_mean -and [double]$row.tick_mean -gt 0 -and $null -ne (Get-78Prop $o "mean_ms")) {
                    $row["${owner}_share_pct"] = [math]::Round(100.0 * [double](Get-78Prop $o "mean_ms") / [double]$row.tick_mean, 2)
                }
            }
        }
        if ($null -ne $row.replication_discover_mean -or $null -ne $row.replication_policy_mean) {
            $sum = 0.0
            foreach ($k in @("replication_discover_mean", "replication_policy_mean", "replication_encode_mean", "replication_enqueue_mean")) {
                if ($null -ne $row[$k]) { $sum += [double]$row[$k] }
            }
            $row.replication_children_mean = [math]::Round($sum, 4)
        }
    }

    $wl = Read-78JsonFile (Join-Path $Path "gameplay_workload.json")
    if ($wl) {
        $row.npcs_active = Get-78Prop $wl "npcs_active"
        $row.npc_updates_total = Get-78Prop $wl "npc_updates_total"
        $row.actions_started_total = Get-78Prop $wl "actions_started_total"
        $row.health_mutations_total = Get-78Prop $wl "health_mutations_total"
        $row.deaths_total = Get-78Prop $wl "deaths_total"
        $row.respawns_total = Get-78Prop $wl "respawns_total"
        $row.spawns_total = Get-78Prop $wl "spawns_total"
        $row.despawns_total = Get-78Prop $wl "despawns_total"
        $row.pulse_ticks_total = Get-78Prop $wl "pulse_ticks_total"
        $row.effects_applied_total = Get-78Prop $wl "effects_applied_total"
        $row.scheduler_queued = Get-78Prop $wl "scheduler_queued"
        $row.effects_active = Get-78Prop $wl "effects_active"
    }

    $pr = Read-78JsonFile (Join-Path $Path "process_resources.json")
    if ($pr) {
        $row.server_rss_mb = [math]::Round(([double](Get-78Prop $pr "working_set_bytes" 0)) / 1MB, 2)
        $row.server_rss_peak_mb = [math]::Round(([double](Get-78Prop $pr "working_set_peak_bytes" 0)) / 1MB, 2)
        $row.server_rss_start_mb = [math]::Round(([double](Get-78Prop $pr "working_set_start_bytes" 0)) / 1MB, 2)
        $delta = Get-78Prop $pr "working_set_delta_bytes" 0
        $row.server_rss_delta_mb = [math]::Round([double]$delta / 1MB, 2)
        $row.server_cpu_end_pct = Get-78Prop $pr "cpu_utilization_pct"
        $row.wall_secs = Get-78Prop $pr "wall_secs"
    }

    $np = Read-78JsonFile (Join-Path $Path "network_pressure.json")
    if ($np) {
        $row.bytes_out_per_sec = Get-78Prop $np "bytes_out_per_sec"
        $row.writer_queue_depth_max = Get-78Prop $np "writer_queue_depth_max"
        $row.writer_queue_push_fail = Get-78Prop $np "writer_queue_push_fail_total"
        $row.queue_age_p99_us = Get-78Prop $np "queue_age_p99_us" (Get-78Prop $np "writer_queue_age_p99_us")
    }

    $rf = Read-78JsonFile (Join-Path $Path "replication_fanout.json")
    if ($rf) {
        $row.repl_emitted = Get-78Prop $rf "updates_emitted_total"
        $row.repl_scanned = Get-78Prop $rf "known_relationships_scanned_total"
        $row.repl_bytes = Get-78Prop $rf "bytes_emitted_total"
    }

    $rs = Read-78JsonFile (Join-Path $Path "run_summary.json")
    if ($rs) {
        $row.snapshot_starvation = Get-78Prop $rs "snapshot_starvation_samples" (Get-78Prop $rs "harness_snapshot_starvation_samples" 0)
        $row.harness_failed_reason = Get-78Prop $rs "failed_reason"
        $row.harness_result = Get-78Prop $rs "result" (Get-78Prop $rs "status")
    }

    if ($null -ne $row.peak_active -and [double]$row.peak_active -gt 0 -and $null -ne $row.replication_children_mean) {
        $row.repl_us_per_observer = [math]::Round(1000.0 * [double]$row.replication_children_mean / [double]$row.peak_active, 2)
    }
    if ($null -ne $row.peak_active -and [double]$row.peak_active -gt 0 -and $null -ne $row.replication_policy_mean) {
        $row.policy_us_per_observer = [math]::Round(1000.0 * [double]$row.replication_policy_mean / [double]$row.peak_active, 2)
    }

    return [pscustomobject]$row
}

function Add-78Finding {
    param(
        [System.Collections.Generic.List[object]]$List,
        [string]$Severity,
        [string]$Code,
        [string]$Message
    )
    $List.Add([pscustomobject]@{
            severity = $Severity
            code = $Code
            message = $Message
        }) | Out-Null
}

function Test-78NumberMax {
    param($Value, $Max, [string]$Code, [string]$Label, $Findings, [string]$Severity = "FAIL")
    if ($null -eq $Max) { return }
    if ($null -eq $Value) {
        Add-78Finding $Findings "FAIL" "${Code}_MISSING" "$Label missing (required for gate)"
        return
    }
    if ([double]$Value -gt [double]$Max) {
        Add-78Finding $Findings $Severity $Code ("{0}={1} exceeds max {2}" -f $Label, $Value, $Max)
    }
}

function Evaluate-78Cell {
    param(
        $Metrics,
        $CellCfg,
        $GlobalCfg,
        $BaselineCell
    )
    $findings = New-Object System.Collections.Generic.List[object]
    $status = "PASS"

    if (-not $Metrics.artifacts_present) {
        Add-78Finding $findings "INVALID" "ARTIFACTS_MISSING" "Cell artifact directory missing"
        return [pscustomobject]@{
            cell_id = [string]$Metrics.cell_id
            status = "HARNESS_INVALID"
            findings = [object[]]$findings.ToArray()
            metrics = $Metrics
        }
    }

    # --- Workload attainment (INVALID, not SERVER_FAIL) ---
    $attMin = [double]$GlobalCfg.attainment_min_pct
    if ($null -eq $Metrics.attainment_pct) {
        Add-78Finding $findings "INVALID" "ATTAINMENT_MISSING" "attainment_pct missing"
    } elseif ([double]$Metrics.attainment_pct -lt $attMin) {
        Add-78Finding $findings "INVALID" "ATTAINMENT_LOW" ("attainment_pct={0} < {1}" -f $Metrics.attainment_pct, $attMin)
    }
    if ([bool]$GlobalCfg.funnel_invariant_required) {
        if ($null -eq $Metrics.funnel_invariant_ok) {
            Add-78Finding $findings "INVALID" "FUNNEL_MISSING" "funnel_invariant_ok missing"
        } elseif (-not [bool]$Metrics.funnel_invariant_ok) {
            Add-78Finding $findings "INVALID" "FUNNEL_BROKEN" "funnel_invariant_ok=false"
        }
    }

    # Incomplete duration → harness invalid (do not treat as server capacity fail).
    $expectedSec = Get-78DurationSecs ([string](Get-78Prop $CellCfg "duration" $Metrics.duration))
    if ($null -ne $expectedSec -and $null -ne $Metrics.wall_secs) {
        $minWall = 0.85 * [double]$expectedSec
        if ([double]$Metrics.wall_secs -lt $minWall) {
            Add-78Finding $findings "INVALID" "HARNESS_SHORT_RUN" ("wall_secs={0:N1} < 85% of requested {1:N0}s (harness aborted early)" -f $Metrics.wall_secs, $expectedSec)
        }
    }

    # --- Lifecycle correctness ---
    if ([bool](Get-78Prop $CellCfg "require_lifecycle" $false)) {
        $deaths = if ($null -ne $Metrics.deaths_total) { [int]$Metrics.deaths_total } else { 0 }
        $respawns = if ($null -ne $Metrics.respawns_total) { [int]$Metrics.respawns_total } else { 0 }
        $effects = if ($null -ne $Metrics.effects_applied_total) { [int]$Metrics.effects_applied_total } else { 0 }
        $actions = if ($null -ne $Metrics.actions_started_total) { [int]$Metrics.actions_started_total } else { 0 }
        $pulses = if ($null -ne $Metrics.pulse_ticks_total) { [int]$Metrics.pulse_ticks_total } else { 0 }
        if (($deaths + $respawns) -le 0) {
            Add-78Finding $findings "FAIL" "LIFECYCLE_NO_DEATH_RESPAWN" "expected non-zero deaths/respawns"
        }
        if (($effects + $actions + $pulses) -le 0) {
            Add-78Finding $findings "FAIL" "LIFECYCLE_NO_EFFECTS" "expected non-zero actions/effects/pulse"
        }
    }

    # --- Absolute operational ---
    $abs = $CellCfg.absolute
    Test-78NumberMax $Metrics.tick_p95 (Get-78Prop $abs "tick_p95_ms_max") "TICK_P95" "tick_p95_ms" $findings
    Test-78NumberMax $Metrics.tick_p99 (Get-78Prop $abs "tick_p99_ms_max") "TICK_P99" "tick_p99_ms" $findings
    Test-78NumberMax $Metrics.tick_util (Get-78Prop $abs "tick_util_pct_max") "TICK_UTIL" "tick_util_pct" $findings
    Test-78NumberMax $Metrics.tick_max (Get-78Prop $abs "tick_max_ms_max") "TICK_MAX" "tick_max_ms" $findings
    $tickMaxAbs = Get-78Prop $abs "tick_max_ms_max"
    if ($null -ne $Metrics.tick_max -and $null -ne $GlobalCfg.tick_max_isolated_warn_ms -and $null -ne $tickMaxAbs) {
        if ([double]$Metrics.tick_max -gt [double]$GlobalCfg.tick_max_isolated_warn_ms -and
            [double]$Metrics.tick_max -le [double]$tickMaxAbs) {
            Add-78Finding $findings "WARN" "TICK_MAX_SPIKE" ("tick_max_ms={0} above warn {1} (isolated spike)" -f $Metrics.tick_max, $GlobalCfg.tick_max_isolated_warn_ms)
        }
    }
    Test-78NumberMax $Metrics.overruns $GlobalCfg.overruns_max "OVERRUNS" "tick_overrun_count" $findings
    Test-78NumberMax $Metrics.consecutive_overrun_streak $GlobalCfg.consecutive_overrun_streak_max "OVERRUN_STREAK" "consecutive_overrun_streak" $findings
    Test-78NumberMax $Metrics.accounting_error_ticks $GlobalCfg.accounting_error_ticks_max "ACCOUNTING" "accounting_error_ticks" $findings
    Test-78NumberMax $Metrics.replication_policy_p99 (Get-78Prop $abs "replication_policy_p99_ms_max") "POLICY_P99" "replication_policy_p99_ms" $findings
    Test-78NumberMax $Metrics.npc_activity_p99 (Get-78Prop $abs "npc_activity_p99_ms_max") "NPC_P99" "npc_activity_p99_ms" $findings
    Test-78NumberMax $Metrics.server_rss_peak_mb (Get-78Prop $abs "rss_peak_mb_max") "RSS_PEAK" "rss_peak_mb" $findings
    $rssDeltaMax = Get-78Prop $abs "rss_delta_mb_max"
    if ($null -ne $rssDeltaMax) {
        Test-78NumberMax $Metrics.server_rss_delta_mb $rssDeltaMax "RSS_DELTA" "rss_delta_mb" $findings
    }
    $rssGrowthWarn = Get-78Prop $abs "rss_growth_ratio_warn"
    if ($null -ne $rssGrowthWarn -and $null -ne $Metrics.server_rss_start_mb -and [double]$Metrics.server_rss_start_mb -gt 0 -and $null -ne $Metrics.server_rss_mb) {
        $ratio = [double]$Metrics.server_rss_mb / [double]$Metrics.server_rss_start_mb
        if ($ratio -gt [double]$rssGrowthWarn) {
            Add-78Finding $findings "WARN" "RSS_GROWTH" ("rss growth ratio={0:N2} exceeds warn {1} (start is pre-warm; delta is authoritative)" -f $ratio, $rssGrowthWarn)
        }
    }

    # Unattributed contract
    if ($null -ne $Metrics.unattributed_share_pct) {
        if ([double]$Metrics.unattributed_share_pct -ge [double]$GlobalCfg.unattributed_share_fail_pct) {
            Add-78Finding $findings "FAIL" "UNATTR_FAIL" ("unattributed_share_pct={0} >= fail {1}" -f $Metrics.unattributed_share_pct, $GlobalCfg.unattributed_share_fail_pct)
        } elseif ([double]$Metrics.unattributed_share_pct -ge [double]$GlobalCfg.unattributed_share_warn_pct) {
            Add-78Finding $findings "WARN" "UNATTR_WARN" ("unattributed_share_pct={0} >= warn {1}" -f $Metrics.unattributed_share_pct, $GlobalCfg.unattributed_share_warn_pct)
        }
    }

    # Network / backpressure
    # Low-N connect/disconnect races can record a few push fails with healthy tick
    # (reproduced flake on functional@8 while util ≪ budget). Fail only when
    # push fails coincide with hard-cap depth under material tick load.
    $pushFail = if ($null -ne $Metrics.writer_queue_push_fail) { [int]$Metrics.writer_queue_push_fail } else { 0 }
    $qDepth = if ($null -ne $Metrics.writer_queue_depth_max) { [int]$Metrics.writer_queue_depth_max } else { 0 }
    $util = if ($null -ne $Metrics.tick_util) { [double]$Metrics.tick_util } else { 0.0 }
    if ($pushFail -gt [int]$GlobalCfg.writer_queue_push_fail_max) {
        if ($qDepth -ge [int]$GlobalCfg.writer_queue_depth_hard_max -and $util -ge 5.0) {
            Add-78Finding $findings "FAIL" "PUSH_FAIL" ("writer_queue_push_fail={0} with depth_max={1} under util={2}" -f $pushFail, $qDepth, $util)
        } else {
            Add-78Finding $findings "WARN" "PUSH_FAIL_NOISE" ("writer_queue_push_fail={0} depth_max={1} util={2} (treated as connect/teardown noise)" -f $pushFail, $qDepth, $util)
        }
    }
    if ($qDepth -ge [int]$GlobalCfg.writer_queue_depth_hard_max -and $util -ge 5.0) {
        Add-78Finding $findings "FAIL" "QUEUE_DEPTH" ("writer_queue_depth_max={0} at hard cap {1} under util={2}" -f $qDepth, $GlobalCfg.writer_queue_depth_hard_max, $util)
    } elseif ($qDepth -ge [int]$GlobalCfg.writer_queue_depth_hard_max) {
        Add-78Finding $findings "WARN" "QUEUE_DEPTH_NOISE" ("writer_queue_depth_max={0} at hard cap with low util={1}" -f $qDepth, $util)
    }

    # Harness notes (do not auto-fail server)
    if ($null -ne $Metrics.snapshot_starvation -and [int]$Metrics.snapshot_starvation -gt 0) {
        Add-78Finding $findings "WARN" "HARNESS_STARVE" ("snapshot_starvation_samples={0} (harness; not server fail alone)" -f $Metrics.snapshot_starvation)
    }
    # Controller tick p99 often hundreds of ms at N≥32 (known harness poll cost). Only warn when extreme.
    if ($null -ne $Metrics.controller_tick_p99_ms -and [double]$Metrics.controller_tick_p99_ms -gt 2000.0) {
        Add-78Finding $findings "WARN" "HARNESS_CTRL_TICK" ("controller_tick_p99_ms={0} (severe harness pressure)" -f $Metrics.controller_tick_p99_ms)
    }

    # --- Baseline regression ---
    if ($null -ne $BaselineCell) {
        $reg = $GlobalCfg.regression
        $relChecks = @(
            @{ cur = $Metrics.tick_p99; base = (Get-78Prop $BaselineCell "tick_p99"); frac = $reg.tick_p99_rel_warn; code = "REG_TICK_P99"; label = "tick_p99" }
            @{ cur = $Metrics.tick_util; base = (Get-78Prop $BaselineCell "tick_util"); frac = $reg.tick_util_rel_warn; code = "REG_TICK_UTIL"; label = "tick_util" }
            @{ cur = $Metrics.replication_policy_mean; base = (Get-78Prop $BaselineCell "replication_policy_mean"); frac = $reg.policy_mean_rel_warn; code = "REG_POLICY_MEAN"; label = "replication_policy_mean" }
            @{ cur = $Metrics.policy_us_per_observer; base = (Get-78Prop $BaselineCell "policy_us_per_observer"); frac = $reg.policy_us_per_observer_rel_warn; code = "REG_POLICY_NORM"; label = "policy_us_per_observer" }
            @{ cur = $Metrics.replication_children_mean; base = (Get-78Prop $BaselineCell "replication_children_mean"); frac = $reg.repl_children_mean_rel_warn; code = "REG_REPL_CHILDREN"; label = "replication_children_mean" }
            @{ cur = $Metrics.server_rss_mb; base = (Get-78Prop $BaselineCell "server_rss_mb"); frac = $reg.rss_end_rel_warn; code = "REG_RSS"; label = "rss_end_mb" }
        )
        foreach ($rc in $relChecks) {
            if ($null -eq $rc.cur -or $null -eq $rc.base) { continue }
            $b = [double]$rc.base
            if ($b -le 0) { continue }
            $ratio = ([double]$rc.cur - $b) / $b
            if ($ratio -gt [double]$rc.frac) {
                Add-78Finding $findings "WARN" $rc.code ("{0} regression: current={1} baseline={2} (+{3:P0})" -f $rc.label, $rc.cur, $rc.base, $ratio)
            }
        }
        $baseDelta = Get-78Prop $BaselineCell "server_rss_delta_mb"
        if ($null -ne $Metrics.server_rss_delta_mb -and $null -ne $baseDelta) {
            $extra = [double]$Metrics.server_rss_delta_mb - [double]$baseDelta
            if ($extra -gt [double]$reg.rss_delta_mb_warn) {
                Add-78Finding $findings "WARN" "REG_RSS_DELTA" ("rss_delta_mb extra={0:N2} vs baseline (warn {1})" -f $extra, $reg.rss_delta_mb_warn)
            }
        }
    }

    $hasInvalid = $false
    $hasFail = $false
    $hasWarn = $false
    foreach ($f in $findings) {
        switch ($f.severity) {
            "INVALID" { $hasInvalid = $true }
            "FAIL" { $hasFail = $true }
            "WARN" { $hasWarn = $true }
        }
    }
    if ($hasInvalid) { $status = "HARNESS_INVALID" }
    elseif ($hasFail) {
        $correctness = $false
        foreach ($f in $findings) {
            if ($f.code -like "LIFECYCLE_*" -or $f.code -eq "ACCOUNTING") { $correctness = $true }
        }
        $status = if ($correctness) { "CORRECTNESS_FAIL" } else { "SERVER_FAIL" }
    } elseif ($hasWarn) { $status = "WARN" }
    else { $status = "PASS" }

    return [pscustomobject]@{
        cell_id = [string]$Metrics.cell_id
        status = [string]$status
        findings = [object[]]$findings.ToArray()
        metrics = $Metrics
    }
}

function Reduce-78GateVerdict {
    param([object[]]$CellResults, $UnitStatus)
    $hasInvalid = $false
    $hasRed = $false
    $hasYellow = $false
    foreach ($c in $CellResults) {
        switch ($c.status) {
            "HARNESS_INVALID" { $hasInvalid = $true }
            "SERVER_FAIL" { $hasRed = $true }
            "CORRECTNESS_FAIL" { $hasRed = $true }
            "WARN" { $hasYellow = $true }
        }
    }
    if ($UnitStatus -eq "FAIL") { $hasRed = $true }
    elseif ($UnitStatus -eq "WARN") { $hasYellow = $true }
    elseif ($UnitStatus -eq "INVALID") { $hasInvalid = $true }

    if ($hasInvalid -and -not $hasRed) { return "INVALID" }
    if ($hasRed) { return "RED" }
    if ($hasYellow) { return "YELLOW" }
    return "GREEN"
}
