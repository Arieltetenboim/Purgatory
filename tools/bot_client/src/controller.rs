//! Load test orchestration.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::json;
use tokio::time::interval;

use purgatory_common::{LoadMetricsV1, current_process_memory};
use purgatory_simulation::{SimulationClock, TICK_DURATION};

use crate::aggregate::ServerRunAggregator;
use crate::classify::{ClassifyInput, RunStatus};
use crate::cli::Cli;
use crate::dashboard::Dashboard;
use crate::endpoint::SharedEndpoint;
use crate::log::{MetricsSample, RunLog, WriteSummaryArgs};
use crate::metrics::{BotMetrics, HarnessMetrics};
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
    sim_ticks: u64,
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
            sim_ticks: 0,
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

        let mut ramp_interval = interval(Duration::from_millis(self.spec.ramp_ms.max(1)));
        ramp_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut churn_interval =
            interval(Duration::from_secs(self.spec.churn_interval_secs.max(1)));
        churn_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let start_time = Instant::now();
        let mut bots_to_spawn = self.spec.bot_count;
        let steady = self.spec.connect == Scenario::Steady;
        self.input_active = !steady;
        let mut quiet_since: Option<Instant> = None;
        let mut active_since: Option<Instant> = None;
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
                break;
            }
            if duration_elapsed {
                break;
            }

            tokio::select! {
                biased;

                _ = tick_interval.tick() => {
                    let now = Instant::now();
                    let elapsed = now.duration_since(last_tick_time);
                    last_tick_time = now;
                    let update = clock.advance(elapsed);

                    for _ in 0..update.ticks_executed {
                        self.sim_ticks = self.sim_ticks.saturating_add(1);
                        self.tick_all_bots(&mut log).await?;
                    }

                    let tick_ms = elapsed.as_secs_f64() * 1000.0;
                    self.metrics.record_tick_time(tick_ms);
                }

                _ = ramp_interval.tick(), if bots_to_spawn > 0 && !self.ramp_complete => {
                    self.spawn_bot(&mut log).await?;
                    bots_to_spawn -= 1;
                    if bots_to_spawn == 0 {
                        self.ramp_complete = true;
                        log.emit_event(
                            "ramp_target_reached",
                            json!({ "target": self.spec.bot_count }),
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
                }

                _ = churn_interval.tick(), if self.ramp_complete
                    && (self.spec.kind == LoadKind::ReconnectChurn
                        || self.spec.connect == Scenario::Churn) =>
                {
                    if self.spec.kind == LoadKind::ReconnectChurn {
                        self.reconnect_churn_bots(&mut log).await?;
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
                    self.sample_metrics(&mut log, &mut server_poller, start_time).await?;
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
            ..ClassifyInput::default()
        }
        .with_server_counters(&self.metrics, server_metrics);
        let classification = classify_input.classify();

        self.close_all_bots().await;

        let extras = crate::aggregate::ServerRunAggregator::with_final_counters(
            self.server_agg
                .to_summary_extras(self.harness_memory_peak_mb),
            self.last_input_handoff_dropped,
        );

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

        Dashboard::print(
            &self.metrics,
            self.spec.bot_count,
            self.metrics.bots.connected,
            server_metrics.as_ref(),
            start_time.elapsed().as_secs_f64(),
            rates.bytes_out_per_sec,
            harness_mb,
        );

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

    async fn spawn_bot(&mut self, log: &mut RunLog) -> Result<(), String> {
        let bot_id = self.next_bot_id;
        self.next_bot_id += 1;

        let mut session = BotSession::new(bot_id, self.spec.profile, self.spec.seed);
        let login = self.spec.bot_login(bot_id);

        match session
            .connect_with_login(self.endpoint.endpoint(), self.cli.server, &login)
            .await
        {
            Ok(()) => {
                let _ = session.accept_snapshot_stream().await;
                self.sessions.insert(bot_id, session);
            }
            Err(e) => {
                session.state = SessionState::Failed;
                self.sessions.insert(bot_id, session);
                log.emit_event(
                    "bot_connect_failed",
                    json!({ "bot_id": bot_id, "error": e }),
                )?;
            }
        }
        Ok(())
    }

    async fn tick_all_bots(&mut self, log: &mut RunLog) -> Result<(), String> {
        let send_portal = self.should_send_portal();
        let mut failed_ids = Vec::new();
        for (id, session) in self.sessions.iter_mut() {
            if session.state != SessionState::Connected {
                continue;
            }

            let _ = session.poll_snapshot().await;

            if !self.input_active {
                continue;
            }

            if let Err(e) = session.send_tick().await {
                session.state = SessionState::Failed;
                self.metrics.unexpected_disconnects += 1;
                failed_ids.push((*id, e));
                continue;
            }
            if send_portal
                && let Some(target) = session.visible_portals.first().copied()
                && session.send_portal_activate(target).await.is_ok()
            {
                self.metrics.portal_activates_sent =
                    self.metrics.portal_activates_sent.saturating_add(1);
            }
        }
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

    fn should_send_portal(&self) -> bool {
        matches!(
            self.spec.kind,
            LoadKind::PortalChurn
                | LoadKind::MixedRuntime
                | LoadKind::Soak
                | LoadKind::PersistenceChurn
        ) && self.sim_ticks > 0
            && self.sim_ticks.is_multiple_of(90)
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
