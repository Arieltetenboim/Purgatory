//! Load test orchestration.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::json;
use tokio::task::JoinSet;
use tokio::time::interval;

use purgatory_common::{
    CapacityLiveSnapshot, ConnectionLifecycleSnapshot, ConnectionRampSnapshot,
    HarnessConnectionSnapshot, LoadMetricsV1, NetworkPressureSnapshot, ProcessResourceSnapshot,
    RampFunnel, check_ramp_funnel_invariant, compose_ramp_ownership, current_process_memory,
    current_process_resources, int_distribution_from_micros, logical_cpu_count,
    ramp_attainment_pct,
};
use purgatory_simulation::{SimulationClock, TICK_DURATION};

use crate::aggregate::ServerRunAggregator;
use crate::classify::{ClassifyInput, RunStatus, SoakClassify, occupancy_seconds};
use crate::cli::Cli;
use crate::dashboard::Dashboard;
use crate::endpoint::SharedEndpoint;
use crate::log::{MetricsSample, RunLog, SoakEvidence, WriteSummaryArgs};
use crate::metrics::{BotMetrics, HarnessMetrics};
use crate::roles::{
    BotRole, RolePlan, requires_mixed_churn, requires_persistent_baseline, requires_portal_gate,
    role_for, role_plan,
};
use crate::scenario::{LoadKind, LoadScenario, Scenario};
use crate::server_metrics::ServerMetricsPoller;
use crate::session::{BotSession, SessionState};

const STARVATION_THRESHOLD_SECS: f64 = 1.0;
const SERVER_TICK_BUDGET_MS: f64 = 33.333;
/// Sparse diagnostic: emit when server window tick max reaches this (ms).
const TICK_SPIKE_EVENT_MS: f64 = 10.0;
const TICK_SPIKE_EVENT_CAP: u32 = 32;
const TICK_SPIKE_COOLDOWN: Duration = Duration::from_secs(2);

pub struct Controller {
    cli: Cli,
    spec: LoadScenario,
    endpoint: SharedEndpoint,
    sessions: HashMap<u32, BotSession>,
    next_bot_id: u32,
    metrics: HarnessMetrics,
    ramp_complete: bool,
    peak_connected: u32,
    shutdown: Arc<AtomicBool>,
    consecutive_starvation_samples: u32,
    consecutive_server_p99_over_budget: u32,
    starvation_event_emitted: bool,
    metrics_unavailable_event_emitted: bool,
    overflow_event_emitted: bool,
    steady_handoff_drop_event_emitted: bool,
    tick_spike_events_emitted: u32,
    last_tick_spike_at: Option<Instant>,
    prev_input_handoff_dropped: u64,
    prev_server: Option<PrevServerCounters>,
    server_agg: ServerRunAggregator,
    harness_memory_peak_mb: Option<f64>,
    last_bytes_out_per_sec: Option<f64>,
    /// Final observed input_handoff_dropped (for summary).
    last_input_handoff_dropped: u64,
    /// When true, bots emit normal 30 Hz gameplay input.
    input_active: bool,
    /// Wall time when input was activated (`steady` scenario).
    input_activated_at: Option<Instant>,
    /// Elapsed secs at input activation (for artifacts).
    input_activation_elapsed_secs: Option<f64>,
    duplicate_probed: bool,
    role_plan: RolePlan,
    min_persistent_connected: u32,
    max_persistent_connected: u32,
    persistent_connected_sum: f64,
    persistent_connected_samples: u64,
    time_below_baseline_secs: f64,
    consecutive_below_secs: f64,
    connected_seconds: f64,
    churn_connects: u64,
    churn_disconnects: u64,
    persistent_unexpected_disconnects: u64,
    portal_attempts: u64,
    portal_out_of_range: u64,
    portal_rejected: u64,
    portal_transitions: u64,
    aoi_enters_start: Option<u64>,
    aoi_enters_end: Option<u64>,
    aoi_updates_start: Option<u64>,
    aoi_updates_end: Option<u64>,
    early_fail: Option<String>,
    ramp_completed_at: Option<Instant>,
    last_soak_observe: Option<Instant>,
    pending_spawns: JoinSet<SpawnOutcome>,
    last_cpu_time_secs: f64,
    last_cpu_wall: Instant,
    cpu_util_pct: f64,
    cpu_util_peak_pct: f64,
    mem_start_bytes: u64,
    mem_peak_bytes: u64,
    conn_attempts: u64,
    conn_ok: u64,
    conn_fail: u64,
    attempt_to_quic_us: Vec<u64>,
    quic_to_welcome_us: Vec<u64>,
    attempt_to_welcome_us: Vec<u64>,
    harness_timeouts: u64,
    spawn_issued: u64,
    spawns_finished: u64,
    peak_in_flight: u64,
    quic_ready_ok: u64,
    peak_transport: u64,
    peak_world_entered: u64,
    controller_ticks: u64,
    last_server_metrics: Option<LoadMetricsV1>,
    spawn_catchup_issued_total: u64,
    spawn_due_peak: u64,
    tick_all_bots_ms: Vec<f64>,
    controller_tick_ms: Vec<f64>,
    ramp_poll_cursor: usize,
}

struct SpawnOutcome {
    bot_id: u32,
    session: BotSession,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct PrevServerCounters {
    at: Instant,
    input_received: u64,
    snapshots_sent: u64,
    bytes_in: u64,
    bytes_out: u64,
}

impl Controller {
    pub fn new(cli: Cli, spec: LoadScenario, shutdown: Arc<AtomicBool>) -> Result<Self, String> {
        cli.validate()?;
        spec.validate_count(cli.max_bots, cli.allow_high_count)?;
        let endpoint = SharedEndpoint::new()?;
        let plan = role_plan(spec.kind, spec.bot_count);
        Ok(Self {
            cli,
            spec,
            endpoint,
            sessions: HashMap::new(),
            next_bot_id: 1,
            metrics: HarnessMetrics::new(),
            ramp_complete: false,
            peak_connected: 0,
            shutdown,
            consecutive_starvation_samples: 0,
            consecutive_server_p99_over_budget: 0,
            starvation_event_emitted: false,
            metrics_unavailable_event_emitted: false,
            overflow_event_emitted: false,
            steady_handoff_drop_event_emitted: false,
            tick_spike_events_emitted: 0,
            last_tick_spike_at: None,
            prev_input_handoff_dropped: 0,
            prev_server: None,
            server_agg: ServerRunAggregator::new(),
            harness_memory_peak_mb: None,
            last_bytes_out_per_sec: None,
            last_input_handoff_dropped: 0,
            input_active: false,
            input_activated_at: None,
            input_activation_elapsed_secs: None,
            duplicate_probed: false,
            role_plan: plan,
            min_persistent_connected: u32::MAX,
            max_persistent_connected: 0,
            persistent_connected_sum: 0.0,
            persistent_connected_samples: 0,
            time_below_baseline_secs: 0.0,
            consecutive_below_secs: 0.0,
            connected_seconds: 0.0,
            churn_connects: 0,
            churn_disconnects: 0,
            persistent_unexpected_disconnects: 0,
            portal_attempts: 0,
            portal_out_of_range: 0,
            portal_rejected: 0,
            portal_transitions: 0,
            aoi_enters_start: None,
            aoi_enters_end: None,
            aoi_updates_start: None,
            aoi_updates_end: None,
            early_fail: None,
            ramp_completed_at: None,
            last_soak_observe: None,
            pending_spawns: JoinSet::new(),
            last_cpu_time_secs: current_process_resources()
                .map(|r| r.cpu.cpu_time_secs)
                .unwrap_or(0.0),
            last_cpu_wall: Instant::now(),
            cpu_util_pct: 0.0,
            cpu_util_peak_pct: 0.0,
            mem_start_bytes: current_process_resources()
                .map(|r| r.memory.working_set_bytes)
                .unwrap_or(0),
            mem_peak_bytes: current_process_resources()
                .map(|r| r.memory.working_set_bytes)
                .unwrap_or(0),
            conn_attempts: 0,
            conn_ok: 0,
            conn_fail: 0,
            attempt_to_quic_us: Vec::new(),
            quic_to_welcome_us: Vec::new(),
            attempt_to_welcome_us: Vec::new(),
            harness_timeouts: 0,
            spawn_issued: 0,
            spawns_finished: 0,
            peak_in_flight: 0,
            quic_ready_ok: 0,
            peak_transport: 0,
            peak_world_entered: 0,
            controller_ticks: 0,
            last_server_metrics: None,
            spawn_catchup_issued_total: 0,
            spawn_due_peak: 0,
            tick_all_bots_ms: Vec::with_capacity(128),
            controller_tick_ms: Vec::with_capacity(128),
            ramp_poll_cursor: 0,
        })
    }

    pub async fn run(&mut self) -> Result<RunStatus, String> {
        let mut log = RunLog::create(&self.cli, &self.spec)?;
        let mut server_poller = match ServerMetricsPoller::new(self.cli.metrics).await {
            Ok(p) => Some(p),
            Err(err) => {
                eprintln!("warning: metrics poller unavailable: {err}");
                let _ = log.emit_event("server_metrics_unavailable", json!({ "error": err }));
                None
            }
        };

        let mut clock = SimulationClock::new();
        let mut last_tick_time = Instant::now();
        let mut tick_interval = interval(TICK_DURATION);
        tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut metrics_interval = interval(Duration::from_secs(1));
        metrics_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut churn_interval =
            interval(Duration::from_secs(self.spec.churn_interval_secs.max(1)));
        churn_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let start_time = Instant::now();
        self.last_soak_observe = Some(start_time);
        let mut bots_to_spawn = self.spec.bot_count;
        let steady = self.spec.connect == Scenario::Steady;
        self.input_active = !steady;
        let mut quiet_since: Option<Instant> = None;
        let mut active_since: Option<Instant> = None;
        let mut next_spawn_due: Option<Instant> = None;
        let mut last_metrics_at = Instant::now();
        let mut timed_out = false;

        if self.spec.connect == Scenario::Burst {
            log.emit_event(
                "ramp_start",
                json!({ "mode": "burst", "count": self.spec.bot_count }),
            )?;
            for _ in 0..self.spec.bot_count {
                self.spawn_bot(&mut log).await?;
            }
            bots_to_spawn = 0;
            self.ramp_complete = true;
            self.ramp_completed_at = Some(Instant::now());
            log.emit_event(
                "ramp_target_reached",
                json!({ "connected": self.sessions.len() }),
            )?;
        } else if bots_to_spawn > 0 {
            log.emit_event(
                "ramp_start",
                json!({
                    "mode": if steady { "steady" } else { "load" },
                    "count": self.spec.bot_count,
                    "ramp_ms": self.spec.ramp_ms,
                    "quiet_secs": self.spec.quiet_secs,
                }),
            )?;
        }

        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                break;
            }

            let duration_elapsed = if steady {
                active_since.is_some_and(|t| t.elapsed() >= self.spec.duration())
            } else {
                start_time.elapsed() >= self.spec.duration()
            };
            if start_time.elapsed() >= self.spec.timeout() && !duration_elapsed {
                timed_out = true;
                self.harness_timeouts = self.harness_timeouts.saturating_add(1);
                break;
            }
            if duration_elapsed {
                break;
            }
            if self.early_fail.is_some() {
                break;
            }

            tokio::select! {
                _ = tick_interval.tick() => {
                    let now = Instant::now();
                    let elapsed = now.duration_since(last_tick_time);
                    last_tick_time = now;
                    let update = clock.advance(elapsed);
                    self.controller_ticks = self.controller_ticks.saturating_add(1);

                    self.drain_pending_spawns(&mut log).await?;

                    // Issue due connects before O(N) bot I/O so ramp cannot starve.
                    if bots_to_spawn > 0 && !self.ramp_complete {
                        let issued = self.issue_due_spawns(&mut bots_to_spawn, &mut next_spawn_due, now);
                        if issued > 0 {
                            log.emit_event(
                                "spawn_issue_burst",
                                json!({
                                    "issued": issued,
                                    "remaining": bots_to_spawn,
                                    "catchup_total": self.spawn_catchup_issued_total,
                                }),
                            )?;
                        }
                    }

                    // One I/O pass per interval. During ramp, thin-poll so
                    // STREAM_POLL_TIMEOUT × N cannot collapse issuance cadence.
                    let bot_ticks = update.ticks_executed.min(1);
                    for _ in 0..bot_ticks {
                        let t0 = Instant::now();
                        self.tick_all_bots(&mut log).await?;
                        self.tick_all_bots_ms
                            .push(t0.elapsed().as_secs_f64() * 1000.0);
                    }

                    let tick_ms = elapsed.as_secs_f64() * 1000.0;
                    self.metrics.record_tick_time(tick_ms);
                    self.controller_tick_ms.push(tick_ms);

                    if !self.ramp_complete
                        && bots_to_spawn == 0
                        && self.pending_spawns.is_empty()
                        && self.spec.bot_count > 0
                    {
                        self.ramp_complete = true;
                        self.ramp_completed_at = Some(Instant::now());
                        log.emit_event(
                            "ramp_target_reached",
                            json!({
                                "target": self.spec.bot_count,
                                "spawn_issued": self.spawn_issued,
                                "catchup_issued": self.spawn_catchup_issued_total,
                            }),
                        )?;
                        if steady {
                            quiet_since = Some(Instant::now());
                            log.emit_event(
                                "quiet_start",
                                json!({
                                    "connected": self.metrics.bots.connected,
                                    "quiet_secs": self.spec.quiet_secs,
                                }),
                            )?;
                        }
                    }

                    if last_metrics_at.elapsed() >= Duration::from_secs(1) {
                        self.sample_metrics(&mut log, &mut server_poller, start_time)
                            .await?;
                        last_metrics_at = Instant::now();
                    }
                }

                _ = churn_interval.tick(), if self.ramp_complete
                    && (self.spec.kind == LoadKind::ReconnectChurn
                        || self.spec.connect == Scenario::Churn
                        || requires_mixed_churn(self.spec.kind)) =>
                {
                    if self.spec.kind == LoadKind::ReconnectChurn {
                        self.reconnect_churn_bots(&mut log).await?;
                    } else if requires_mixed_churn(self.spec.kind) {
                        self.mixed_churn_bots(&mut log).await?;
                    } else {
                        self.churn_bots(&mut log).await?;
                    }
                }

                _ = metrics_interval.tick() => {
                    // Steady phase transitions on the metrics tick (1 Hz): settle then activate.
                    if steady && !self.input_active {
                        self.update_bot_metrics();
                        if self.ramp_complete {
                            let connected = self.metrics.bots.connected;
                            let target = self.spec.bot_count;
                            if quiet_since.is_none() {
                                quiet_since = Some(Instant::now());
                                log.emit_event(
                                    "quiet_start",
                                    json!({
                                        "connected": connected,
                                        "quiet_secs": self.spec.quiet_secs,
                                    }),
                                )?;
                            }
                            let quiet_elapsed = quiet_since.map(|t| t.elapsed()).unwrap_or_default();
                            let population_ok = connected >= target.saturating_sub(0)
                                && connected > 0;
                            if population_ok && quiet_elapsed >= Duration::from_secs(self.spec.quiet_secs) {
                                self.input_active = true;
                                let now = Instant::now();
                                self.input_activated_at = Some(now);
                                active_since = Some(now);
                                self.input_activation_elapsed_secs =
                                    Some(start_time.elapsed().as_secs_f64());
                                log.emit_event(
                                    "input_activation",
                                    json!({
                                        "elapsed_secs": self.input_activation_elapsed_secs,
                                        "connected": connected,
                                        "requested": target,
                                        "quiet_secs": self.spec.quiet_secs,
                                    }),
                                )?;
                            }
                        }
                    }
                    if last_metrics_at.elapsed() >= Duration::from_millis(800) {
                        self.sample_metrics(&mut log, &mut server_poller, start_time)
                            .await?;
                        last_metrics_at = Instant::now();
                    }
                    if self.ramp_complete
                        && self.spec.kind == LoadKind::DuplicateIdentity
                        && !self.duplicate_probed
                    {
                        self.probe_duplicate_identity(&mut log).await?;
                    }
                }
            }
        }

        let aborted = self.shutdown.load(Ordering::Relaxed);
        self.drain_pending_spawns(&mut log).await?;
        self.update_bot_metrics();
        {
            let end_server = server_poller.as_ref().and_then(|p| p.last_metrics());
            self.observe_soak(start_time, end_server);
        }

        // Final poll after the loop so shutdown does not drop the last window.
        if let Some(p) = server_poller.as_mut() {
            match p.poll().await {
                Some(s) => {
                    self.last_input_handoff_dropped = s.input_handoff_dropped;
                    self.server_agg.observe(&s);
                }
                None => {
                    self.server_agg.note_miss();
                    if let Some(s) = p.last_metrics() {
                        self.last_input_handoff_dropped = s.input_handoff_dropped;
                    }
                }
            }
        }

        let (starved, all_starved) = self.count_starved_bots();
        let server_metrics = server_poller.as_ref().and_then(|p| p.last_metrics());
        let metrics_health = if server_poller.is_none() && self.server_agg.samples_ok() == 0 {
            // Poller never started — treat as unavailable.
            crate::aggregate::MetricsHealth::None
        } else {
            self.server_agg.health()
        };

        let extras = crate::aggregate::ServerRunAggregator::with_final_counters(
            self.server_agg
                .to_summary_extras(self.harness_memory_peak_mb),
            self.last_input_handoff_dropped,
        );

        let classify_input = ClassifyInput {
            aborted,
            ramp_complete: self.ramp_complete,
            connected_bots: self.metrics.bots.connected,
            starved_bots: starved,
            consecutive_starvation_samples: self.consecutive_starvation_samples,
            all_connected_starved: all_starved,
            consecutive_server_p99_over_budget: self.consecutive_server_p99_over_budget,
            cleanup_leak: false,
            timed_out,
            reconnect_entity_unchanged: self.metrics.reconnect_entity_unchanged,
            duplicate_welcome: self.metrics.duplicate_welcome,
            metrics_health,
            metrics_samples_ok: self.server_agg.samples_ok(),
            metrics_samples_missed: self.server_agg.samples_missed(),
            soak: self.soak_classify(),
            validation: self
                .spec
                .validation
                .is_active()
                .then(|| self.spec.validation.clone()),
            execution: Some(extras.validation_execution()),
            ..ClassifyInput::default()
        }
        .with_server_counters(&self.metrics, server_metrics);
        let classification = classify_input.classify();
        let soak_out = if requires_persistent_baseline(self.spec.kind)
            || requires_portal_gate(self.spec.kind)
        {
            Some(self.soak_evidence())
        } else {
            None
        };

        self.close_all_bots().await;

        log.write_summary(WriteSummaryArgs {
            status: classification.status,
            reasons: &classification.reasons,
            metrics: &self.metrics,
            peak_connected: self.peak_connected,
            requested_bots: self.spec.bot_count,
            server_metrics_ok: self.server_agg.server_metrics_ok(),
            server_metrics_health: metrics_health.as_str(),
            server_metrics_samples_ok: self.server_agg.samples_ok(),
            server_metrics_samples_missed: self.server_agg.samples_missed(),
            extras,
            failure_class: classification.failure_class(),
            requested_duration_secs: self.spec.duration_secs,
            preset: self.spec.preset.map(|p| format!("{p:?}").to_lowercase()),
            seed: self.spec.seed,
            soak: soak_out,
        })?;

        print_status_line(classification.status, &classification.reasons);

        self.endpoint.close();

        Ok(classification.status)
    }

    async fn sample_metrics(
        &mut self,
        log: &mut RunLog,
        server_poller: &mut Option<ServerMetricsPoller>,
        start_time: Instant,
    ) -> Result<(), String> {
        self.update_bot_metrics();
        self.peak_connected = self.peak_connected.max(self.metrics.bots.connected);
        self.peak_transport = self
            .peak_transport
            .max(u64::from(self.metrics.bots.connected));

        let harness_mb =
            current_process_memory().map(|m| m.working_set_bytes as f64 / (1024.0 * 1024.0));
        if let Some(mb) = harness_mb {
            self.harness_memory_peak_mb = Some(self.harness_memory_peak_mb.unwrap_or(0.0).max(mb));
        }

        let server_metrics = match server_poller.as_mut() {
            Some(p) => p.poll().await,
            None => None,
        };

        match (&server_metrics, server_poller.is_some()) {
            (Some(s), _) => self.server_agg.observe(s),
            (None, true) => {
                self.server_agg.note_miss();
                if !self.metrics_unavailable_event_emitted {
                    self.metrics_unavailable_event_emitted = true;
                    let _ = log.emit_event("server_metrics_unavailable", json!({}));
                }
            }
            (None, false) => {}
        }

        let (starved, _all_starved) = self.count_starved_bots();
        if starved > 0 {
            self.consecutive_starvation_samples += 1;
            self.metrics.snapshot_starvation_samples =
                self.metrics.snapshot_starvation_samples.max(1);
            if self.ramp_complete && !self.starvation_event_emitted {
                self.starvation_event_emitted = true;
                log.emit_event("snapshot_starvation", json!({ "starved_bots": starved }))?;
            }
        } else {
            self.consecutive_starvation_samples = 0;
        }

        if let Some(ref s) = server_metrics {
            if s.tick_work_p99_ms > SERVER_TICK_BUDGET_MS {
                self.consecutive_server_p99_over_budget += 1;
            } else {
                self.consecutive_server_p99_over_budget = 0;
            }

            self.last_input_handoff_dropped = s.input_handoff_dropped;
            let handoff_delta = s
                .input_handoff_dropped
                .saturating_sub(self.prev_input_handoff_dropped);
            if handoff_delta > 0 {
                self.metrics.overflow_events = self.metrics.overflow_events.max(
                    s.lifecycle_handoff_dropped
                        .saturating_add(s.input_queue_overflow)
                        .saturating_add(s.input_handoff_dropped),
                );
                if !self.overflow_event_emitted {
                    self.overflow_event_emitted = true;
                    log.emit_event(
                        "queue_overflow",
                        json!({
                            "lifecycle_handoff_dropped": s.lifecycle_handoff_dropped,
                            "input_queue_overflow": s.input_queue_overflow,
                            "input_handoff_dropped": s.input_handoff_dropped,
                            "input_handoff_delta": handoff_delta,
                            "ramp_complete": self.ramp_complete,
                            "connected_bots": self.metrics.bots.connected,
                        }),
                    )?;
                } else if self.ramp_complete && !self.steady_handoff_drop_event_emitted {
                    self.steady_handoff_drop_event_emitted = true;
                    log.emit_event(
                        "input_handoff_drop_steady",
                        json!({
                            "input_handoff_dropped": s.input_handoff_dropped,
                            "input_handoff_delta": handoff_delta,
                            "connected_bots": self.metrics.bots.connected,
                        }),
                    )?;
                }
                self.prev_input_handoff_dropped = s.input_handoff_dropped;
            }

            if s.lifecycle_handoff_dropped + s.input_queue_overflow > 0
                && !self.overflow_event_emitted
            {
                self.overflow_event_emitted = true;
                self.metrics.overflow_events = s
                    .lifecycle_handoff_dropped
                    .saturating_add(s.input_queue_overflow)
                    .saturating_add(s.input_handoff_dropped);
                log.emit_event(
                    "queue_overflow",
                    json!({
                        "lifecycle_handoff_dropped": s.lifecycle_handoff_dropped,
                        "input_queue_overflow": s.input_queue_overflow,
                        "input_handoff_dropped": s.input_handoff_dropped,
                        "ramp_complete": self.ramp_complete,
                        "connected_bots": self.metrics.bots.connected,
                    }),
                )?;
            }

            // Sparse tick-spike correlation (bounded).
            if s.tick_work_max_ms >= TICK_SPIKE_EVENT_MS
                && self.tick_spike_events_emitted < TICK_SPIKE_EVENT_CAP
            {
                let now = Instant::now();
                let cooled = self
                    .last_tick_spike_at
                    .is_none_or(|t| now.duration_since(t) >= TICK_SPIKE_COOLDOWN);
                if cooled {
                    self.last_tick_spike_at = Some(now);
                    self.tick_spike_events_emitted += 1;
                    log.emit_event(
                        "tick_work_spike",
                        json!({
                            "tick_work_max_ms": s.tick_work_max_ms,
                            "tick_work_mean_ms": s.tick_work_mean_ms,
                            "tick_work_p99_ms": s.tick_work_p99_ms,
                            "active_sessions": s.active_sessions,
                            "connected_bots": self.metrics.bots.connected,
                            "ramp_complete": self.ramp_complete,
                            "input_active": self.input_active,
                            "snapshot_build_time_max_ms": s.snapshot_build_time_max_ms,
                            "snapshot_encode_time_max_ms": s.snapshot_encode_time_max_ms,
                            "input_queue_current": s.input_queue_current,
                            "input_queue_max": s.input_queue_max,
                            "session_queue_max": s.session_queue_max,
                            "input_handoff_dropped": s.input_handoff_dropped,
                            "input_handoff_delta": handoff_delta,
                        }),
                    )?;
                }
            }

            self.metrics.encode_failures = s.snapshot_encode_failed;
            self.metrics.admission_refusals = s.admission_refused;
            self.last_server_metrics = Some(s.clone());
            self.peak_transport = self
                .peak_transport
                .max(s.peak_sessions)
                .max(s.active_sessions);
            self.peak_world_entered = self.peak_world_entered.max(u64::from(self.peak_connected));
        }

        let rates = self.compute_rates(server_metrics.as_ref());
        self.last_bytes_out_per_sec = rates.bytes_out_per_sec;

        let mut sample = MetricsSample {
            elapsed_secs: start_time.elapsed().as_secs_f64(),
            target_bots: self.spec.bot_count,
            connected_bots: self.metrics.bots.connected,
            commands_sent: self.metrics.bots.total_commands_sent,
            snapshots_received: self.metrics.bots.total_snapshots_received,
            bot_scheduler_p50_ms: self.metrics.bot_scheduler_p50_ms(),
            bot_scheduler_p95_ms: self.metrics.bot_scheduler_p95_ms(),
            bot_scheduler_p99_ms: self.metrics.bot_scheduler_p99_ms(),
            bot_scheduler_max_ms: self.metrics.bot_scheduler_max_ms(),
            harness_memory_mb: harness_mb,
            server_active_sessions: server_metrics.as_ref().map(|m| m.active_sessions),
            server_active_player_entities: server_metrics
                .as_ref()
                .map(|m| m.active_player_entities),
            server_tick_work_mean_ms: server_metrics.as_ref().map(|m| m.tick_work_mean_ms),
            server_tick_work_p50_ms: server_metrics.as_ref().map(|m| m.tick_work_p50_ms),
            server_tick_work_p95_ms: server_metrics.as_ref().map(|m| m.tick_work_p95_ms),
            server_tick_work_p99_ms: server_metrics.as_ref().map(|m| m.tick_work_p99_ms),
            server_tick_work_max_ms: server_metrics.as_ref().map(|m| m.tick_work_max_ms),
            server_tick_overruns_total: server_metrics.as_ref().map(|m| m.tick_overrun_count),
            server_scheduler_lateness_p95_ms: server_metrics
                .as_ref()
                .map(|m| m.scheduler_lateness_p95_ms),
            server_scheduler_lateness_max_ms: server_metrics
                .as_ref()
                .map(|m| m.scheduler_lateness_max_ms),
            server_input_msgs_per_sec: rates.input_per_sec,
            server_snapshot_msgs_per_sec: rates.snapshots_per_sec,
            server_bytes_in_per_sec: rates.bytes_in_per_sec,
            server_bytes_out_per_sec: rates.bytes_out_per_sec,
            server_input_queue_current: server_metrics.as_ref().map(|m| m.input_queue_current),
            server_input_queue_max: server_metrics.as_ref().map(|m| m.input_queue_max),
            server_session_queue_max: server_metrics.as_ref().map(|m| m.session_queue_max),
            snapshot_build_count: server_metrics.as_ref().map(|m| m.snapshot_build_count),
            snapshot_build_time_max_ms: server_metrics
                .as_ref()
                .map(|m| m.snapshot_build_time_max_ms),
            snapshot_encode_time_max_ms: server_metrics
                .as_ref()
                .map(|m| m.snapshot_encode_time_max_ms),
            snapshot_size_max_bytes: server_metrics.as_ref().map(|m| m.snapshot_size_max_bytes),
            snapshot_encode_failures_total: server_metrics
                .as_ref()
                .map(|m| m.snapshot_encode_failed),
            server_memory_mb: server_metrics
                .as_ref()
                .map(|m| m.memory_working_set_bytes as f64 / (1024.0 * 1024.0)),
            server_memory_peak_mb: server_metrics
                .as_ref()
                .map(|m| m.memory_working_set_peak_bytes as f64 / (1024.0 * 1024.0)),
            server_input_received: server_metrics.as_ref().map(|m| m.input_received),
            server_input_accepted: server_metrics.as_ref().map(|m| m.input_accepted),
            server_input_stale: server_metrics.as_ref().map(|m| m.input_stale),
            server_input_handoff_dropped: server_metrics.as_ref().map(|m| m.input_handoff_dropped),
            server_input_rate_limited: server_metrics.as_ref().map(|m| m.input_rate_limited),
            input_active: self.input_active,
            ..MetricsSample::default()
        };
        if let Some(s) = server_metrics.as_ref() {
            sample.fill_schema3(s);
        }
        log.write_metrics_sample(&sample)?;
        self.write_harness_capacity_artifacts(log)?;

        Dashboard::print(
            &self.metrics,
            self.spec.bot_count,
            self.metrics.bots.connected,
            server_metrics.as_ref(),
            start_time.elapsed().as_secs_f64(),
            rates.bytes_out_per_sec,
            harness_mb,
        );
        self.observe_soak(start_time, server_metrics.as_ref());
        self.write_live_status(log, start_time)?;

        self.metrics.clear_tick_times();
        Ok(())
    }

    fn compute_rates(&mut self, server: Option<&LoadMetricsV1>) -> RateSample {
        let Some(s) = server else {
            return RateSample::default();
        };
        let now = Instant::now();
        let rates = if let Some(prev) = self.prev_server {
            let dt = now.duration_since(prev.at).as_secs_f64().max(1e-6);
            RateSample {
                input_per_sec: Some(
                    s.input_received.saturating_sub(prev.input_received) as f64 / dt,
                ),
                snapshots_per_sec: Some(
                    s.snapshots_sent.saturating_sub(prev.snapshots_sent) as f64 / dt,
                ),
                bytes_in_per_sec: Some(s.bytes_in.saturating_sub(prev.bytes_in) as f64 / dt),
                bytes_out_per_sec: Some(s.bytes_out.saturating_sub(prev.bytes_out) as f64 / dt),
            }
        } else {
            RateSample::default()
        };
        self.prev_server = Some(PrevServerCounters {
            at: now,
            input_received: s.input_received,
            snapshots_sent: s.snapshots_sent,
            bytes_in: s.bytes_in,
            bytes_out: s.bytes_out,
        });
        rates
    }

    fn queue_bot_connect(&mut self) {
        let bot_id = self.next_bot_id;
        self.next_bot_id += 1;
        let endpoint = self.endpoint.endpoint().clone();
        let server = self.cli.server;
        let login = self.spec.bot_login(bot_id);
        let profile = self.spec.profile;
        let seed = self.spec.seed;
        let role = role_for(self.spec.kind, self.spec.bot_count, bot_id);
        self.note_spawn_issued();
        self.pending_spawns.spawn(async move {
            let mut session = BotSession::new(bot_id, profile, seed);
            session.role = role;
            match session.connect_with_login(&endpoint, server, &login).await {
                Ok(()) => SpawnOutcome {
                    bot_id,
                    session,
                    error: None,
                },
                Err(e) => SpawnOutcome {
                    bot_id,
                    session,
                    error: Some(e),
                },
            }
        });
    }

    /// Enqueue every connection whose ramp schedule is already due.
    /// Advances the schedule by `ramp_ms` per issue (not wall `now`).
    fn issue_due_spawns(
        &mut self,
        bots_to_spawn: &mut u32,
        next_spawn_due: &mut Option<Instant>,
        now: Instant,
    ) -> u32 {
        let ramp = Duration::from_millis(self.spec.ramp_ms.max(1));
        let due = count_due_spawns(*bots_to_spawn, *next_spawn_due, now, ramp);
        if due == 0 {
            return 0;
        }
        self.spawn_due_peak = self.spawn_due_peak.max(u64::from(due));
        if due > 1 {
            self.spawn_catchup_issued_total = self
                .spawn_catchup_issued_total
                .saturating_add(u64::from(due.saturating_sub(1)));
        }
        let mut schedule = next_spawn_due.unwrap_or(now);
        for _ in 0..due {
            self.queue_bot_connect();
            *bots_to_spawn = bots_to_spawn.saturating_sub(1);
            schedule += ramp;
        }
        *next_spawn_due = Some(schedule);
        due
    }

    async fn drain_pending_spawns(&mut self, log: &mut RunLog) -> Result<(), String> {
        while let Some(joined) = self.pending_spawns.try_join_next() {
            self.spawns_finished = self.spawns_finished.saturating_add(1);
            match joined {
                Ok(outcome) => {
                    let SpawnOutcome {
                        bot_id,
                        mut session,
                        error,
                    } = outcome;
                    if let Some(e) = error {
                        self.note_connect_attempt(&session, false);
                        session.state = SessionState::Failed;
                        self.sessions.insert(bot_id, session);
                        log.emit_event(
                            "bot_connect_failed",
                            json!({ "bot_id": bot_id, "error": e }),
                        )?;
                    } else {
                        self.note_connect_attempt(&session, true);
                        let _ = session.poll_accept_snapshot().await;
                        self.sessions.insert(bot_id, session);
                    }
                }
                Err(err) => {
                    log.emit_event(
                        "bot_connect_failed",
                        json!({ "error": format!("join: {err}") }),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn note_spawn_issued(&mut self) {
        self.spawn_issued = self.spawn_issued.saturating_add(1);
        let inflight = self.spawn_issued.saturating_sub(self.spawns_finished);
        self.peak_in_flight = self.peak_in_flight.max(inflight);
    }

    fn note_connect_attempt(&mut self, session: &BotSession, ok: bool) {
        const RING: usize = 512;
        self.conn_attempts = self.conn_attempts.saturating_add(1);
        if ok {
            self.conn_ok = self.conn_ok.saturating_add(1);
        } else {
            self.conn_fail = self.conn_fail.saturating_add(1);
        }
        if let Some(v) = session.attempt_to_quic_ready_us {
            self.quic_ready_ok = self.quic_ready_ok.saturating_add(1);
            push_us(&mut self.attempt_to_quic_us, v, RING);
        }
        if let Some(v) = session.quic_ready_to_welcome_us {
            push_us(&mut self.quic_to_welcome_us, v, RING);
        }
        if let Some(v) = session.attempt_to_welcome_us {
            push_us(&mut self.attempt_to_welcome_us, v, RING);
        }
    }

    fn write_harness_capacity_artifacts(&mut self, log: &RunLog) -> Result<(), String> {
        self.refresh_harness_cpu();
        let res = current_process_resources();
        let ws = res.map(|r| r.memory.working_set_bytes).unwrap_or(0);
        self.mem_peak_bytes = self.mem_peak_bytes.max(ws);
        let cpus = res
            .map(|r| r.cpu.logical_cpus)
            .unwrap_or_else(logical_cpu_count)
            .max(1);
        let resources = ProcessResourceSnapshot {
            schema: ProcessResourceSnapshot::SCHEMA,
            wall_secs: log.elapsed_secs(),
            logical_cpus: cpus,
            working_set_bytes: ws,
            working_set_peak_bytes: self.mem_peak_bytes.max(ws),
            working_set_start_bytes: self.mem_start_bytes,
            cpu_time_secs: res
                .map(|r| r.cpu.cpu_time_secs)
                .unwrap_or(self.last_cpu_time_secs),
            cpu_utilization_pct: self.cpu_util_pct,
            cpu_normalized_per_logical_pct: self.cpu_util_pct / f64::from(cpus),
            cpu_utilization_peak_pct: self.cpu_util_peak_pct,
            cpu_time_delta_secs: 0.0,
            sample_interval_secs: 1.0,
            working_set_delta_bytes: ws as i64 - self.mem_start_bytes as i64,
            thread_count: res.and_then(|r| r.thread_count),
            handle_count: res.and_then(|r| r.handle_count),
        };
        let conn = HarnessConnectionSnapshot {
            schema: HarnessConnectionSnapshot::SCHEMA,
            wall_secs: log.elapsed_secs(),
            note: HarnessConnectionSnapshot::NOTE.to_string(),
            connect_attempts: self.conn_attempts,
            connect_ok: self.conn_ok,
            connect_fail: self.conn_fail,
            harness_timeouts: self.harness_timeouts,
            attempt_to_quic_ready_us: int_distribution_from_micros(&self.attempt_to_quic_us),
            quic_ready_to_welcome_us: int_distribution_from_micros(&self.quic_to_welcome_us),
            attempt_to_welcome_us: int_distribution_from_micros(&self.attempt_to_welcome_us),
        };
        let res_json = serde_json::to_vec_pretty(&resources)
            .map_err(|e| format!("serialize harness_resources: {e}"))?;
        let conn_json = serde_json::to_vec_pretty(&conn)
            .map_err(|e| format!("serialize harness_connection: {e}"))?;
        std::fs::write(log.dir().join("harness_resources.json"), &res_json)
            .map_err(|e| format!("write harness_resources: {e}"))?;
        std::fs::write(log.dir().join("harness_connection.json"), &conn_json)
            .map_err(|e| format!("write harness_connection: {e}"))?;
        if let Some(root) = &self.spec.persist_root {
            let persist = std::path::Path::new(root);
            if let Some(shared) = persist.parent() {
                let shared_res = shared.join("harness_resources.json");
                let shared_conn = shared.join("harness_connection.json");
                if shared_res != log.dir().join("harness_resources.json") {
                    let _ = std::fs::write(shared_res, &res_json);
                    let _ = std::fs::write(shared_conn, &conn_json);
                }
            }
        }
        self.write_connection_ramp_artifact(log, &resources)?;
        Ok(())
    }

    fn write_connection_ramp_artifact(
        &self,
        log: &RunLog,
        harness_res: &ProcessResourceSnapshot,
    ) -> Result<(), String> {
        let shared = self.spec.persist_root.as_ref().and_then(|root| {
            std::path::Path::new(root)
                .parent()
                .map(std::path::Path::to_path_buf)
        });
        let live = shared
            .as_ref()
            .and_then(|p| read_json::<CapacityLiveSnapshot>(&p.join("capacity_live.json")));
        let life = shared.as_ref().and_then(|p| {
            read_json::<ConnectionLifecycleSnapshot>(&p.join("connection_lifecycle.json"))
        });
        let net = shared
            .as_ref()
            .and_then(|p| read_json::<NetworkPressureSnapshot>(&p.join("network_pressure.json")));
        let server = self.last_server_metrics.as_ref();
        let server_transport = life.as_ref().map(|l| l.transport_accept_ok).unwrap_or(0);
        let server_welcome = life.as_ref().map(|l| l.welcome_ok).unwrap_or(0);
        let server_session = life
            .as_ref()
            .map(|l| l.session_accepted)
            .or_else(|| server.map(|s| s.session_created))
            .unwrap_or(0);
        // Issuance funnel is harness-owned only. Never `.max()` with server lifecycle
        // totals — reconnect churn / probes inflate server accepts above spawn_issued.
        let transport_total = self.quic_ready_ok;
        let welcome_total = self.conn_ok;
        let world_entered = self.conn_ok;
        let disconnects = life
            .as_ref()
            .map(|l| l.disconnects)
            .or_else(|| server.map(|s| s.session_destroyed))
            .unwrap_or(0);
        let admission_cap = server.map(|s| s.admission_cap).unwrap_or(0);
        let admission_refused = server
            .map(|s| s.admission_refused)
            .unwrap_or(self.metrics.admission_refusals);
        let peak_active =
            u64::from(self.peak_connected).max(server.map(|s| s.peak_sessions).unwrap_or(0));
        let pending_in_flight = self.spawn_issued.saturating_sub(self.spawns_finished);
        let funnel = RampFunnel {
            requested_clients: u64::from(self.spec.bot_count),
            spawn_issued_total: self.spawn_issued,
            connect_attempts_completed: self.conn_attempts,
            transport_established_total: transport_total,
            welcome_total,
            world_entered_total: world_entered,
            peak_active_clients: peak_active,
            connect_fail_total: self.conn_fail,
            harness_timeouts: self.harness_timeouts,
            admission_refused,
            admission_cap,
            disconnects,
            reconnect_ok_total: self.churn_connects,
            server_transport_accept_ok: server_transport,
            server_welcome_ok: server_welcome,
            server_session_accepted: server_session,
        };
        let invariant = check_ramp_funnel_invariant(&funnel, pending_in_flight);
        let cpus = harness_res.logical_cpus.max(1);
        let snap = ConnectionRampSnapshot {
            schema: ConnectionRampSnapshot::SCHEMA,
            wall_secs: log.elapsed_secs(),
            note: ConnectionRampSnapshot::NOTE.to_string(),
            harness_exit_is_not_attainment: true,
            requested_clients: funnel.requested_clients,
            spawn_issued_total: funnel.spawn_issued_total,
            connect_attempts_completed: funnel.connect_attempts_completed,
            peak_connection_attempts: self.peak_in_flight,
            transport_established_total: funnel.transport_established_total,
            peak_transport_established: self.peak_transport.max(transport_total),
            welcome_total: funnel.welcome_total,
            world_entered_total: funnel.world_entered_total,
            peak_world_entered: u64::from(self.peak_connected).max(world_entered),
            peak_active_clients: funnel.peak_active_clients,
            attainment_pct: ramp_attainment_pct(
                funnel.peak_active_clients,
                funnel.requested_clients,
            ),
            connect_fail_total: funnel.connect_fail_total,
            harness_timeouts: funnel.harness_timeouts,
            admission_refused: funnel.admission_refused,
            admission_cap: funnel.admission_cap,
            disconnects: funnel.disconnects,
            controller_ticks: self.controller_ticks,
            pending_in_flight,
            reconnect_ok_total: funnel.reconnect_ok_total,
            server_transport_accept_ok: funnel.server_transport_accept_ok,
            server_welcome_ok: funnel.server_welcome_ok,
            server_session_accepted: funnel.server_session_accepted,
            funnel_invariant_ok: invariant.ok,
            funnel_invariant_note: invariant.note.clone(),
            controller_tick_p50_ms: crate::metrics::percentile(&self.controller_tick_ms, 50.0),
            controller_tick_p99_ms: crate::metrics::percentile(&self.controller_tick_ms, 99.0),
            tick_all_bots_p50_ms: crate::metrics::percentile(&self.tick_all_bots_ms, 50.0),
            tick_all_bots_p99_ms: crate::metrics::percentile(&self.tick_all_bots_ms, 99.0),
            spawn_catchup_issued_total: self.spawn_catchup_issued_total,
            spawn_due_peak: self.spawn_due_peak,
            attempt_to_quic_ready_us: int_distribution_from_micros(&self.attempt_to_quic_us),
            quic_ready_to_welcome_us: int_distribution_from_micros(&self.quic_to_welcome_us),
            attempt_to_welcome_us: int_distribution_from_micros(&self.attempt_to_welcome_us),
            accept_to_hello_us: life
                .as_ref()
                .map(|l| l.accept_to_hello_us)
                .unwrap_or_default(),
            hello_to_welcome_us: life
                .as_ref()
                .map(|l| l.hello_to_welcome_us)
                .unwrap_or_default(),
            server_cpu_utilization_pct: live.as_ref().map(|l| l.cpu_utilization_pct).unwrap_or(0.0),
            server_cpu_normalized_per_logical_pct: live
                .as_ref()
                .map(|l| l.cpu_normalized_per_logical_pct)
                .unwrap_or(0.0),
            harness_cpu_utilization_pct: harness_res.cpu_utilization_pct,
            harness_cpu_normalized_per_logical_pct: harness_res.cpu_utilization_pct
                / f64::from(cpus),
            server_tick_p99_ms: live
                .as_ref()
                .map(|l| l.tick_p99_ms)
                .or_else(|| server.map(|s| s.tick_work_p99_ms))
                .unwrap_or(0.0),
            server_tick_utilization_pct: live
                .as_ref()
                .map(|l| l.tick_utilization_pct)
                .unwrap_or(0.0),
            writer_queue_depth_max: net
                .as_ref()
                .map(|n| n.writer_queue_depth_max)
                .or_else(|| live.as_ref().map(|l| l.writer_queue_depth_max))
                .unwrap_or(0),
            writer_queue_push_fail_total: net
                .as_ref()
                .map(|n| n.writer_queue_push_fail_total)
                .or_else(|| live.as_ref().map(|l| l.writer_queue_push_fail_total))
                .unwrap_or(0),
            write_drain_p99_ms: net
                .as_ref()
                .map(|n| n.write_drain.p99 / 1000.0)
                .or_else(|| live.as_ref().map(|l| l.write_drain_p99_ms))
                .unwrap_or(0.0),
            saturation_class: live
                .as_ref()
                .map(|l| l.saturation_class)
                .unwrap_or_default(),
            ownership_statement: compose_ramp_ownership(&funnel),
        };
        let json = serde_json::to_vec_pretty(&snap)
            .map_err(|e| format!("serialize connection_ramp: {e}"))?;
        std::fs::write(log.dir().join("connection_ramp.json"), &json)
            .map_err(|e| format!("write connection_ramp: {e}"))?;
        let nd = serde_json::to_string(&snap).unwrap_or_default();
        if !nd.is_empty() {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log.dir().join("connection_ramp.ndjson"))
            {
                let _ = writeln!(f, "{nd}");
            }
        }
        if let Some(shared) = shared {
            let dest = shared.join("connection_ramp.json");
            if dest != log.dir().join("connection_ramp.json") {
                let _ = std::fs::write(&dest, &json);
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(shared.join("connection_ramp.ndjson"))
                {
                    use std::io::Write;
                    let _ = writeln!(f, "{nd}");
                }
            }
        }
        Ok(())
    }

    fn refresh_harness_cpu(&mut self) {
        let Some(res) = current_process_resources() else {
            return;
        };
        let now = Instant::now();
        let wall = now
            .saturating_duration_since(self.last_cpu_wall)
            .as_secs_f64();
        let delta = (res.cpu.cpu_time_secs - self.last_cpu_time_secs).max(0.0);
        if wall > 0.0 {
            self.cpu_util_pct = (delta / wall) * 100.0;
            self.cpu_util_peak_pct = self.cpu_util_peak_pct.max(self.cpu_util_pct);
        }
        self.last_cpu_time_secs = res.cpu.cpu_time_secs;
        self.last_cpu_wall = now;
    }

    async fn spawn_bot(&mut self, log: &mut RunLog) -> Result<(), String> {
        let bot_id = self.next_bot_id;
        self.next_bot_id += 1;
        self.note_spawn_issued();

        let mut session = BotSession::new(bot_id, self.spec.profile, self.spec.seed);
        session.role = role_for(self.spec.kind, self.spec.bot_count, bot_id);
        let login = self.spec.bot_login(bot_id);

        match session
            .connect_with_login(self.endpoint.endpoint(), self.cli.server, &login)
            .await
        {
            Ok(()) => {
                self.note_connect_attempt(&session, true);
                let _ = session.poll_accept_snapshot().await;
                self.sessions.insert(bot_id, session);
            }
            Err(e) => {
                self.note_connect_attempt(&session, false);
                session.state = SessionState::Failed;
                self.sessions.insert(bot_id, session);
                log.emit_event(
                    "bot_connect_failed",
                    json!({ "bot_id": bot_id, "error": e }),
                )?;
            }
        }
        self.spawns_finished = self.spawns_finished.saturating_add(1);
        Ok(())
    }

    async fn tick_all_bots(&mut self, log: &mut RunLog) -> Result<(), String> {
        let connected: Vec<u32> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.state == SessionState::Connected)
            .map(|(id, _)| *id)
            .collect();
        if connected.is_empty() {
            return Ok(());
        }

        // During ramp, rotate a small subset so 1 ms stream polls × N cannot
        // starve catch-up issuance. After ramp: full poll by default, or thin
        // rotate when `--post-ramp-thin` / PURGATORY_LOAD_POST_RAMP_THIN=1
        // (7.4 harness hygiene for high-N network measurements).
        let post_ramp_thin = self.cli.post_ramp_thin
            || std::env::var("PURGATORY_LOAD_POST_RAMP_THIN")
                .ok()
                .is_some_and(|v| matches!(v.trim(), "1" | "true" | "yes"));
        let slow_drain = {
            let from_env = std::env::var("PURGATORY_LOAD_SLOW_DRAIN_COUNT")
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(0);
            self.cli.slow_drain_count.max(from_env)
        };
        let (poll_ids, default_drain_cap): (Vec<u32>, u32) = if !self.ramp_complete {
            const RAMP_BOT_POLL_CAP: usize = 8;
            let n = connected.len();
            let take = n.min(RAMP_BOT_POLL_CAP);
            let start = self.ramp_poll_cursor % n.max(1);
            let mut ids = Vec::with_capacity(take);
            for i in 0..take {
                ids.push(connected[(start + i) % n]);
            }
            self.ramp_poll_cursor = start.saturating_add(take);
            (ids, 1)
        } else if post_ramp_thin {
            const THIN_BOT_POLL_CAP: usize = 16;
            let n = connected.len();
            let take = n.min(THIN_BOT_POLL_CAP);
            let start = self.ramp_poll_cursor % n.max(1);
            let mut ids = Vec::with_capacity(take);
            for i in 0..take {
                ids.push(connected[(start + i) % n]);
            }
            self.ramp_poll_cursor = start.saturating_add(take);
            (ids, 4)
        } else {
            (connected.clone(), 8)
        };

        let mut failed_ids = Vec::new();
        for id in poll_ids {
            let Some(session) = self.sessions.get_mut(&id) else {
                continue;
            };
            if session.state != SessionState::Connected {
                continue;
            }

            if let Err(e) = session.poll_accept_snapshot().await {
                session.state = SessionState::Failed;
                if session.role != BotRole::Churn {
                    self.metrics.unexpected_disconnects += 1;
                    self.persistent_unexpected_disconnects =
                        self.persistent_unexpected_disconnects.saturating_add(1);
                }
                failed_ids.push((id, e));
                continue;
            }
            // Slow receivers: refuse uni drain so Quinn back-pressures server writes.
            // Lowest bot_ids are preferred so the set is stable across ticks.
            let is_slow = slow_drain > 0 && session.bot_id < slow_drain;
            let snapshot_drain_cap = if is_slow { 0 } else { default_drain_cap };
            let mut drained = 0u32;
            if snapshot_drain_cap > 0 {
                loop {
                    match session.poll_snapshot().await {
                        Ok(Some(_)) => {
                            drained += 1;
                            if drained >= snapshot_drain_cap {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            session.state = SessionState::Failed;
                            if session.role != BotRole::Churn {
                                self.metrics.unexpected_disconnects += 1;
                                self.persistent_unexpected_disconnects =
                                    self.persistent_unexpected_disconnects.saturating_add(1);
                            }
                            failed_ids.push((id, e));
                            break;
                        }
                    }
                }
            }
            if session.state != SessionState::Connected {
                continue;
            }
            match session.poll_control().await {
                Ok(poll) => {
                    self.portal_out_of_range =
                        self.portal_out_of_range.saturating_add(poll.out_of_range);
                    self.portal_rejected = self.portal_rejected.saturating_add(poll.rejected);
                    if poll.out_of_range > 0 {
                        session.replica.note_out_of_range();
                    }
                    if poll.disconnected {
                        session.state = SessionState::Failed;
                        if session.role != BotRole::Churn {
                            self.metrics.unexpected_disconnects += 1;
                            self.persistent_unexpected_disconnects =
                                self.persistent_unexpected_disconnects.saturating_add(1);
                        }
                        failed_ids.push((id, "server disconnect".into()));
                        continue;
                    }
                }
                Err(e) => {
                    session.state = SessionState::Failed;
                    if session.role != BotRole::Churn {
                        self.metrics.unexpected_disconnects += 1;
                        self.persistent_unexpected_disconnects =
                            self.persistent_unexpected_disconnects.saturating_add(1);
                    }
                    failed_ids.push((id, e));
                    continue;
                }
            }

            if !self.input_active {
                continue;
            }

            let send_result = if session.role == BotRole::PersistentPortal {
                let intent = session.replica.portal_intent();
                let tick = session.send_input(intent.axis, false, false).await;
                if tick.is_ok()
                    && intent.activate
                    && let Some(target) = intent.target
                    && session.send_portal_activate(target).await.is_ok()
                {
                    self.portal_attempts = self.portal_attempts.saturating_add(1);
                    self.metrics.portal_activates_sent =
                        self.metrics.portal_activates_sent.saturating_add(1);
                }
                tick
            } else {
                session.send_tick().await
            };
            if let Err(e) = send_result {
                session.state = SessionState::Failed;
                if session.role != BotRole::Churn {
                    self.metrics.unexpected_disconnects += 1;
                    self.persistent_unexpected_disconnects =
                        self.persistent_unexpected_disconnects.saturating_add(1);
                }
                failed_ids.push((id, e));
            }
        }
        self.portal_transitions = self.sessions.values().map(|s| s.replica.transitions).sum();
        for (bot_id, error) in failed_ids {
            log.emit_event(
                "bot_unexpected_disconnect",
                json!({ "bot_id": bot_id, "error": error }),
            )?;
        }
        Ok(())
    }

    async fn churn_bots(&mut self, log: &mut RunLog) -> Result<(), String> {
        let connected: Vec<u32> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.state == SessionState::Connected)
            .map(|(id, _)| *id)
            .collect();

        let to_disconnect = (connected.len() as f32 * self.spec.churn_fraction) as usize;
        log.emit_event("churn_wave_start", json!({ "disconnect": to_disconnect }))?;
        for &bot_id in connected.iter().take(to_disconnect) {
            if let Some(session) = self.sessions.get_mut(&bot_id) {
                session.close().await;
            }
        }

        for _ in 0..to_disconnect {
            self.spawn_bot(log).await?;
        }
        log.emit_event("churn_wave_complete", json!({ "respawned": to_disconnect }))?;
        Ok(())
    }

    async fn mixed_churn_bots(&mut self, log: &mut RunLog) -> Result<(), String> {
        let ids: Vec<u32> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.role == BotRole::Churn && s.state == SessionState::Connected)
            .map(|(id, _)| *id)
            .collect();
        log.emit_event(
            "churn_wave_start",
            json!({ "disconnect": ids.len(), "role": "churn" }),
        )?;
        let server = self.cli.server;
        for bot_id in ids {
            if let Some(session) = self.sessions.get_mut(&bot_id) {
                self.churn_disconnects = self.churn_disconnects.saturating_add(1);
                match session
                    .reconnect_same_login(self.endpoint.endpoint(), server)
                    .await
                {
                    Ok(()) => {
                        self.churn_connects = self.churn_connects.saturating_add(1);
                    }
                    Err(err) => {
                        log.emit_event(
                            "bot_reconnect_failed",
                            json!({ "bot_id": bot_id, "error": err, "role": "churn" }),
                        )?;
                    }
                }
            }
        }
        log.emit_event(
            "churn_wave_complete",
            json!({ "reconnected": self.churn_connects }),
        )?;
        Ok(())
    }

    fn observe_soak(&mut self, start_time: Instant, server: Option<&LoadMetricsV1>) {
        let now = Instant::now();
        let dt = self
            .last_soak_observe
            .map(|t| now.saturating_duration_since(t).as_secs_f64())
            .unwrap_or_else(|| start_time.elapsed().as_secs_f64());
        self.last_soak_observe = Some(now);

        let persistent = self.persistent_connected();
        let target = self.role_plan.persistent_target();
        self.connected_seconds += occupancy_seconds(persistent, dt);
        if self.ramp_complete {
            self.min_persistent_connected = if self.min_persistent_connected == u32::MAX {
                persistent
            } else {
                self.min_persistent_connected.min(persistent)
            };
            self.max_persistent_connected = self.max_persistent_connected.max(persistent);
            self.persistent_connected_sum += f64::from(persistent);
            self.persistent_connected_samples = self.persistent_connected_samples.saturating_add(1);
            if target > 0 && persistent < target {
                self.time_below_baseline_secs += dt;
                self.consecutive_below_secs += dt;
            } else {
                self.consecutive_below_secs = 0.0;
            }
        }
        if let Some(s) = server {
            if self.aoi_enters_start.is_none() && self.ramp_complete {
                self.aoi_enters_start = Some(s.aoi_enters);
                self.aoi_updates_start = Some(s.aoi_updates);
            }
            self.aoi_enters_end = Some(s.aoi_enters);
            self.aoi_updates_end = Some(s.aoi_updates);
        }

        let grace_ok = self
            .ramp_completed_at
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(5));
        if self.early_fail.is_none() && grace_ok && requires_persistent_baseline(self.spec.kind) {
            if target > 0 && persistent == 0 && self.consecutive_below_secs >= 5.0 {
                self.early_fail = Some(format!(
                    "persistent real clients dropped to 0 for {:.0}s",
                    self.consecutive_below_secs
                ));
            }
            let portal_dead = self.sessions.iter().any(|(_, s)| {
                s.role == BotRole::PersistentPortal && s.state == SessionState::Failed
            });
            if requires_portal_gate(self.spec.kind)
                && !self.spec.relax_portal_gate
                && portal_dead
                && self.portal_transitions == 0
                && start_time.elapsed() >= Duration::from_secs(12)
            {
                self.early_fail =
                    Some("portal scenario client failed before an authoritative transition".into());
            }
        }
    }

    fn write_live_status(&self, log: &mut RunLog, start_time: Instant) -> Result<(), String> {
        let elapsed = start_time.elapsed().as_secs_f64();
        let persistent = self.persistent_connected();
        let churn = self
            .sessions
            .values()
            .filter(|s| s.role == BotRole::Churn && s.state == SessionState::Connected)
            .count();
        let failures = self.persistent_unexpected_disconnects;
        let line = format!(
            "RUNNING {elapsed} / {duration} | real {persistent}/{target} | churn {churn} | portal {portal}/1 | failures {failures}",
            elapsed = format_mmss(elapsed),
            duration = format_mmss(self.spec.duration_secs as f64),
            target = self.role_plan.persistent_target(),
            portal = self.portal_transitions,
        );
        log.write_live_status(&json!({
            "state": if self.early_fail.is_some() { "failing" } else { "running" },
            "elapsed_secs": elapsed,
            "duration_secs": self.spec.duration_secs,
            "process": "alive",
            "real_connected": persistent,
            "persistent_target": self.role_plan.persistent_target(),
            "churn_connected": churn,
            "portal_transitions": self.portal_transitions,
            "portal_attempts": self.portal_attempts,
            "failures": failures,
            "status_line": line,
            "early_fail": self.early_fail,
        }))
    }

    fn persistent_connected(&self) -> u32 {
        self.sessions
            .values()
            .filter(|s| s.role != BotRole::Churn && s.state == SessionState::Connected)
            .count() as u32
    }

    fn soak_evidence(&self) -> SoakEvidence {
        let min = if self.min_persistent_connected == u32::MAX {
            0
        } else {
            self.min_persistent_connected
        };
        let avg = if self.persistent_connected_samples == 0 {
            0.0
        } else {
            self.persistent_connected_sum / self.persistent_connected_samples as f64
        };
        SoakEvidence {
            persistent_target: self.role_plan.persistent_target(),
            min_persistent_connected: min,
            avg_persistent_connected: avg,
            max_persistent_connected: self.max_persistent_connected,
            final_persistent_connected: self.persistent_connected(),
            time_below_baseline_secs: self.time_below_baseline_secs,
            connected_seconds: self.connected_seconds,
            churn_connects: self.churn_connects,
            churn_disconnects: self.churn_disconnects,
            unexpected_disconnects: self.persistent_unexpected_disconnects,
            portal_attempts: self.portal_attempts,
            portal_out_of_range: self.portal_out_of_range,
            portal_rejected: self.portal_rejected,
            portal_transitions: self.portal_transitions,
            portal_seen_ticks: self
                .sessions
                .values()
                .map(|s| s.replica.portal_seen_ticks)
                .sum(),
            portal_in_zone_ticks: self
                .sessions
                .values()
                .map(|s| s.replica.portal_in_zone_ticks)
                .sum(),
            aoi_enters_start: self.aoi_enters_start,
            aoi_enters_end: self.aoi_enters_end,
            aoi_updates_start: self.aoi_updates_start,
            aoi_updates_end: self.aoi_updates_end,
        }
    }

    fn soak_classify(&self) -> Option<SoakClassify> {
        let kind = self.spec.kind;
        if !requires_persistent_baseline(kind) && !requires_portal_gate(kind) {
            return None;
        }
        let evidence = self.soak_evidence();
        let aoi_enters_delta = match (evidence.aoi_enters_start, evidence.aoi_enters_end) {
            (Some(start), Some(end)) => end.saturating_sub(start),
            _ => 0,
        };
        let aoi_updates_delta = match (self.aoi_updates_start, self.aoi_updates_end) {
            (Some(start), Some(end)) => end.saturating_sub(start),
            _ => 0,
        };
        Some(SoakClassify {
            require_persistent_baseline: requires_persistent_baseline(kind),
            require_portal_transition: requires_portal_gate(kind) && !self.spec.relax_portal_gate,
            require_churn: requires_mixed_churn(kind) && self.role_plan.churn > 0,
            persistent_target: evidence.persistent_target,
            min_persistent_connected: evidence.min_persistent_connected,
            avg_persistent_connected: evidence.avg_persistent_connected,
            time_below_baseline_secs: evidence.time_below_baseline_secs,
            consecutive_below_baseline_secs: self.consecutive_below_secs,
            connected_seconds: evidence.connected_seconds,
            duration_secs: self.spec.duration_secs as f64,
            ramp_complete: self.ramp_complete,
            portal_transitions: evidence.portal_transitions,
            portal_attempts: evidence.portal_attempts,
            portal_seen_ticks: evidence.portal_seen_ticks,
            portal_in_zone_ticks: evidence.portal_in_zone_ticks,
            aoi_updates_delta,
            aoi_enters_delta,
            aoi_observed: evidence.aoi_enters_start.is_some(),
            churn_disconnects: evidence.churn_disconnects,
            early_fail: self.early_fail.clone(),
        })
    }

    async fn reconnect_churn_bots(&mut self, log: &mut RunLog) -> Result<(), String> {
        let connected: Vec<u32> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.state == SessionState::Connected)
            .map(|(id, _)| *id)
            .collect();
        let to_recycle = (connected.len() as f32 * self.spec.churn_fraction) as usize;
        log.emit_event("reconnect_wave_start", json!({ "reconnect": to_recycle }))?;
        let server = self.cli.server;
        for &bot_id in connected.iter().take(to_recycle) {
            if let Some(session) = self.sessions.get_mut(&bot_id) {
                match session
                    .reconnect_same_login(self.endpoint.endpoint(), server)
                    .await
                {
                    Ok(()) => {
                        self.churn_connects = self.churn_connects.saturating_add(1);
                        if session.entity_changed_on_reconnect() == Some(false) {
                            self.metrics.reconnect_entity_unchanged =
                                self.metrics.reconnect_entity_unchanged.saturating_add(1);
                        }
                    }
                    Err(err) => {
                        log.emit_event(
                            "bot_reconnect_failed",
                            json!({ "bot_id": bot_id, "error": err }),
                        )?;
                    }
                }
            }
        }
        log.emit_event(
            "reconnect_wave_complete",
            json!({ "reconnected": to_recycle }),
        )?;
        Ok(())
    }

    async fn probe_duplicate_identity(&mut self, log: &mut RunLog) -> Result<(), String> {
        self.duplicate_probed = true;
        let Some((bot_id, login)) = self
            .sessions
            .iter()
            .find(|(_, s)| s.state == SessionState::Connected && !s.login.is_empty())
            .map(|(id, s)| (*id, s.login.clone()))
        else {
            return Ok(());
        };
        log.emit_event(
            "duplicate_identity_probe",
            json!({ "bot_id": bot_id, "login": login }),
        )?;
        let mut challenger = BotSession::new(u32::MAX, self.spec.profile, self.spec.seed);
        match challenger
            .connect_with_login(self.endpoint.endpoint(), self.cli.server, &login)
            .await
        {
            Ok(()) => {
                self.metrics.duplicate_welcome = self.metrics.duplicate_welcome.saturating_add(1);
                log.emit_event(
                    "duplicate_identity_unexpected_welcome",
                    json!({ "login": login }),
                )?;
                challenger.close().await;
            }
            Err(_)
                if challenger.last_disconnect
                    == Some(purgatory_protocol::DisconnectReasonCode::AlreadyConnected) =>
            {
                self.metrics.already_connected_rejects =
                    self.metrics.already_connected_rejects.saturating_add(1);
                log.emit_event("duplicate_identity_rejected", json!({ "login": login }))?;
            }
            Err(err) => {
                log.emit_event(
                    "duplicate_identity_other_error",
                    json!({ "login": login, "error": err }),
                )?;
            }
        }
        Ok(())
    }

    async fn close_all_bots(&mut self) {
        for session in self.sessions.values_mut() {
            session.close().await;
        }
    }

    fn update_bot_metrics(&mut self) {
        let mut bot_metrics = BotMetrics::default();

        for session in self.sessions.values() {
            match session.state {
                SessionState::Connected => bot_metrics.connected += 1,
                SessionState::Connecting | SessionState::Handshaking => bot_metrics.connecting += 1,
                SessionState::Disconnected => bot_metrics.disconnected += 1,
                SessionState::Failed => bot_metrics.failed += 1,
            }
            bot_metrics.total_commands_sent += session.metrics.commands_sent;
            bot_metrics.total_snapshots_received += session.metrics.snapshots_received;
        }

        self.metrics.bots = bot_metrics;
    }

    fn count_starved_bots(&self) -> (u32, bool) {
        let now = Instant::now();
        let mut starved = 0u32;
        let mut connected = 0u32;

        for session in self.sessions.values() {
            if session.state != SessionState::Connected {
                continue;
            }
            connected += 1;
            if let Some(last) = session.metrics.last_snapshot_at {
                if now.duration_since(last).as_secs_f64() > STARVATION_THRESHOLD_SECS {
                    starved += 1;
                }
            } else {
                starved += 1;
            }
        }

        let all_starved = connected > 0 && starved == connected;
        (starved, all_starved)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct RateSample {
    input_per_sec: Option<f64>,
    snapshots_per_sec: Option<f64>,
    bytes_in_per_sec: Option<f64>,
    bytes_out_per_sec: Option<f64>,
}

fn print_status_line(status: RunStatus, reasons: &[crate::classify::StatusReason]) {
    if reasons.is_empty() {
        println!("run_status={}", status.as_str());
        return;
    }
    let joined = reasons
        .iter()
        .map(|r| r.message.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    println!("run_status={} — {joined}", status.as_str());
}

fn format_mmss(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn push_us(buf: &mut Vec<u64>, v: u64, cap: usize) {
    if buf.len() >= cap {
        buf.remove(0);
    }
    buf.push(v);
}

/// How many ramp connects are due now, without mutating the schedule.
/// `next_due = None` means the first issue is due immediately.
#[must_use]
fn count_due_spawns(
    remaining: u32,
    next_due: Option<Instant>,
    now: Instant,
    ramp: Duration,
) -> u32 {
    if remaining == 0 {
        return 0;
    }
    let Some(mut due_at) = next_due else {
        return 1;
    };
    // First call with Some: count how many ramp slots have elapsed.
    // If next_due is in the future, nothing is due.
    if now < due_at {
        return 0;
    }
    let mut due = 0u32;
    while due < remaining && now >= due_at {
        due = due.saturating_add(1);
        due_at += ramp;
    }
    due
}

#[cfg(test)]
mod issuance_tests {
    use super::*;

    #[test]
    fn first_spawn_is_immediately_due() {
        let now = Instant::now();
        assert_eq!(
            count_due_spawns(10, None, now, Duration::from_millis(10)),
            1
        );
    }

    #[test]
    fn catchup_counts_all_elapsed_slots() {
        let start = Instant::now();
        let ramp = Duration::from_millis(10);
        // Schedule started 55ms ago → slots at 0,10,20,30,40,50 → 6 due.
        let next = Some(start);
        let now = start + Duration::from_millis(55);
        assert_eq!(count_due_spawns(384, next, now, ramp), 6);
    }

    #[test]
    fn catchup_respects_remaining_cap() {
        let start = Instant::now();
        let ramp = Duration::from_millis(10);
        let now = start + Duration::from_millis(1000);
        assert_eq!(count_due_spawns(3, Some(start), now, ramp), 3);
    }

    #[test]
    fn future_schedule_issues_nothing() {
        let now = Instant::now();
        let next = Some(now + Duration::from_millis(50));
        assert_eq!(
            count_due_spawns(10, next, now, Duration::from_millis(10)),
            0
        );
    }
}
