//! Run artifact logging.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;
use serde_json::json;

use crate::classify::{RunStatus, StatusReason, ValidationExecution};
use crate::cli::Cli;
use crate::metrics::HarnessMetrics;
use crate::scenario::LoadScenario;

pub const METRICS_CSV_HEADER: &str = concat!(
    "elapsed_secs,target_bots,connected_bots,commands_sent,snapshots_received,",
    "bot_scheduler_p50_ms,bot_scheduler_p95_ms,bot_scheduler_p99_ms,bot_scheduler_max_ms,",
    "harness_memory_mb,",
    "server_active_sessions,server_active_player_entities,",
    "server_tick_work_mean_ms,server_tick_work_p50_ms,server_tick_work_p95_ms,",
    "server_tick_work_p99_ms,server_tick_work_max_ms,server_tick_overruns_total,",
    "server_scheduler_lateness_p95_ms,server_scheduler_lateness_max_ms,",
    "server_input_msgs_per_sec,server_snapshot_msgs_per_sec,",
    "server_bytes_in_per_sec,server_bytes_out_per_sec,",
    "server_input_queue_current,server_input_queue_max,server_session_queue_max,",
    "snapshot_build_count,snapshot_build_time_max_ms,snapshot_encode_time_max_ms,",
    "snapshot_size_max_bytes,snapshot_encode_failures_total,",
    "server_memory_mb,server_memory_peak_mb,",
    "server_input_received,server_input_accepted,server_input_stale,",
    "server_input_handoff_dropped,server_input_rate_limited,input_active,",
    "server_aoi_enters,server_aoi_leaves,server_aoi_updates,server_aoi_churn_reentry,",
    "server_aoi_update_bytes,server_oldest_pending_ticks,server_max_deferred_ticks,",
    "server_replication_queue_depth_max,server_scheduler_queued,",
    "server_scheduler_due_critical,server_scheduler_due_deferred,",
    "server_scheduler_critical_ceiling_hits,server_scheduler_deferred_exhausted,",
    "server_actions_active,server_events_produced,server_events_processed,",
    "server_spawn_queue_depth,server_cadence_due,server_command_rejects_gate,",
    "server_command_rejects_other,server_domain_rev_advances,",
    "server_observer_pending_updates,server_observer_pending_enters,",
    "server_cadence_deferred_updates,",
    "server_scheduler_scheduled_total,server_scheduler_cancelled_total,",
    "server_scheduler_critical_executed_total,server_scheduler_deferred_executed_total,",
    "server_actions_started_total,server_actions_completed_total,",
    "server_effects_applied_total,server_effects_expired_total,",
    "server_spawn_requests_total,server_spawns_completed_total,",
    "server_despawns_completed_total,server_cadence_executions_total,",
    "server_entities_spawned_total"
);

pub struct RunLog {
    dir: PathBuf,
    logs_base: PathBuf,
    #[allow(dead_code)]
    config_path: PathBuf,
    metrics_writer: BufWriter<File>,
    events_writer: BufWriter<File>,
    start_time: Instant,
}

#[derive(Serialize)]
pub struct RunConfig {
    pub count: u32,
    pub profile: String,
    pub scenario: String,
    pub seed: u64,
    pub duration_secs: u64,
    pub quiet_secs: u64,
    pub ramp_ms: u64,
    pub server: String,
    pub metrics_addr: String,
    pub client_build: String,
}

#[derive(Serialize)]
pub struct RunSummary {
    pub run_status: String,
    pub status_reasons: Vec<StatusReason>,
    pub elapsed_secs: f64,
    pub requested_bots: u32,
    pub peak_connected: u32,
    pub failure_class: String,
    pub total_commands_sent: u64,
    pub total_snapshots_received: u64,
    pub overflow_events: u64,
    pub encode_failures: u64,
    pub snapshot_starvation_samples: u32,
    pub unexpected_disconnects: u64,
    pub admission_refusals: u64,
    pub server_metrics_ok: bool,
    pub server_metrics_health: String,
    pub server_metrics_samples_ok: u64,
    pub server_metrics_samples_missed: u64,
    /// Always `peak_of_window_percentiles` for p95/p99 fields (not full-run).
    pub tick_percentile_semantics: String,
    pub server_tick_overruns_total: Option<u64>,
    pub server_tick_work_mean_ms: Option<f64>,
    pub server_tick_work_p95_ms: Option<f64>,
    pub server_tick_work_p99_ms: Option<f64>,
    pub server_tick_work_max_ms: Option<f64>,
    pub server_scheduler_lateness_p95_ms: Option<f64>,
    pub server_scheduler_lateness_max_ms: Option<f64>,
    pub server_bytes_in: Option<u64>,
    pub server_bytes_out: Option<u64>,
    pub server_memory_start_mb: Option<f64>,
    pub server_memory_peak_mb: Option<f64>,
    pub server_memory_end_mb: Option<f64>,
    pub harness_memory_peak_mb: Option<f64>,
    pub server_input_queue_max: Option<u64>,
    pub server_session_queue_max: Option<u64>,
    pub input_handoff_dropped_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_queued_start: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_queued_end: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_queued_max: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions_active_max: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn_queue_depth_max: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aoi_enters_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aoi_leaves_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aoi_updates_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_critical_ceiling_hits: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_deferred_exhausted: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_scheduled_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_cancelled_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_critical_executed_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler_deferred_executed_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions_started_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions_completed_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects_applied_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects_expired_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn_requests_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawns_completed_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub despawns_completed_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cadence_executions_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entities_spawned_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events_produced_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events_processed_total: Option<u64>,
    pub requested_duration_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soak: Option<SoakEvidence>,
}

/// Mixed/soak evidence recorded in the harness summary (not a metrics schema bump).
#[derive(Clone, Debug, Serialize)]
pub struct SoakEvidence {
    pub persistent_target: u32,
    pub min_persistent_connected: u32,
    pub avg_persistent_connected: f64,
    pub max_persistent_connected: u32,
    pub final_persistent_connected: u32,
    pub time_below_baseline_secs: f64,
    pub connected_seconds: f64,
    pub churn_connects: u64,
    pub churn_disconnects: u64,
    pub unexpected_disconnects: u64,
    pub portal_attempts: u64,
    pub portal_out_of_range: u64,
    pub portal_rejected: u64,
    pub portal_transitions: u64,
    pub portal_seen_ticks: u64,
    pub portal_in_zone_ticks: u64,
    pub aoi_enters_start: Option<u64>,
    pub aoi_enters_end: Option<u64>,
    pub aoi_updates_start: Option<u64>,
    pub aoi_updates_end: Option<u64>,
}

/// One 1 Hz sample. `Option` fields stay empty in CSV when the poll missed.
#[derive(Clone, Debug, Default)]
pub struct MetricsSample {
    pub elapsed_secs: f64,
    pub target_bots: u32,
    pub connected_bots: u32,
    pub commands_sent: u64,
    pub snapshots_received: u64,
    pub bot_scheduler_p50_ms: f64,
    pub bot_scheduler_p95_ms: f64,
    pub bot_scheduler_p99_ms: f64,
    pub bot_scheduler_max_ms: f64,
    pub harness_memory_mb: Option<f64>,
    pub server_active_sessions: Option<u64>,
    pub server_active_player_entities: Option<u64>,
    pub server_tick_work_mean_ms: Option<f64>,
    pub server_tick_work_p50_ms: Option<f64>,
    pub server_tick_work_p95_ms: Option<f64>,
    pub server_tick_work_p99_ms: Option<f64>,
    pub server_tick_work_max_ms: Option<f64>,
    pub server_tick_overruns_total: Option<u64>,
    pub server_scheduler_lateness_p95_ms: Option<f64>,
    pub server_scheduler_lateness_max_ms: Option<f64>,
    pub server_input_msgs_per_sec: Option<f64>,
    pub server_snapshot_msgs_per_sec: Option<f64>,
    pub server_bytes_in_per_sec: Option<f64>,
    pub server_bytes_out_per_sec: Option<f64>,
    pub server_input_queue_current: Option<u64>,
    pub server_input_queue_max: Option<u64>,
    pub server_session_queue_max: Option<u64>,
    pub snapshot_build_count: Option<u64>,
    pub snapshot_build_time_max_ms: Option<f64>,
    pub snapshot_encode_time_max_ms: Option<f64>,
    pub snapshot_size_max_bytes: Option<u64>,
    pub snapshot_encode_failures_total: Option<u64>,
    pub server_memory_mb: Option<f64>,
    pub server_memory_peak_mb: Option<f64>,
    pub server_input_received: Option<u64>,
    pub server_input_accepted: Option<u64>,
    pub server_input_stale: Option<u64>,
    pub server_input_handoff_dropped: Option<u64>,
    pub server_input_rate_limited: Option<u64>,
    pub input_active: bool,
    pub server_aoi_enters: Option<u64>,
    pub server_aoi_leaves: Option<u64>,
    pub server_aoi_updates: Option<u64>,
    pub server_aoi_churn_reentry: Option<u64>,
    pub server_aoi_update_bytes: Option<u64>,
    pub server_oldest_pending_ticks: Option<u64>,
    pub server_max_deferred_ticks: Option<u64>,
    pub server_replication_queue_depth_max: Option<u64>,
    pub server_scheduler_queued: Option<u64>,
    pub server_scheduler_due_critical: Option<u64>,
    pub server_scheduler_due_deferred: Option<u64>,
    pub server_scheduler_critical_ceiling_hits: Option<u64>,
    pub server_scheduler_deferred_exhausted: Option<u64>,
    pub server_actions_active: Option<u64>,
    pub server_events_produced: Option<u64>,
    pub server_events_processed: Option<u64>,
    pub server_spawn_queue_depth: Option<u64>,
    pub server_cadence_due: Option<u64>,
    pub server_command_rejects_gate: Option<u64>,
    pub server_command_rejects_other: Option<u64>,
    pub server_domain_rev_advances: Option<u64>,
    pub server_observer_pending_updates: Option<u64>,
    pub server_observer_pending_enters: Option<u64>,
    pub server_cadence_deferred_updates: Option<u64>,
    pub server_scheduler_scheduled_total: Option<u64>,
    pub server_scheduler_cancelled_total: Option<u64>,
    pub server_scheduler_critical_executed_total: Option<u64>,
    pub server_scheduler_deferred_executed_total: Option<u64>,
    pub server_actions_started_total: Option<u64>,
    pub server_actions_completed_total: Option<u64>,
    pub server_effects_applied_total: Option<u64>,
    pub server_effects_expired_total: Option<u64>,
    pub server_spawn_requests_total: Option<u64>,
    pub server_spawns_completed_total: Option<u64>,
    pub server_despawns_completed_total: Option<u64>,
    pub server_cadence_executions_total: Option<u64>,
    pub server_entities_spawned_total: Option<u64>,
}

impl MetricsSample {
    pub fn fill_schema3(&mut self, s: &purgatory_common::LoadMetricsV1) {
        self.server_aoi_enters = Some(s.aoi_enters);
        self.server_aoi_leaves = Some(s.aoi_leaves);
        self.server_aoi_updates = Some(s.aoi_updates);
        self.server_aoi_churn_reentry = Some(s.aoi_churn_reentry);
        self.server_aoi_update_bytes = Some(s.aoi_update_bytes);
        self.server_oldest_pending_ticks = Some(s.oldest_pending_ticks);
        self.server_max_deferred_ticks = Some(s.max_deferred_ticks);
        self.server_replication_queue_depth_max = Some(s.replication_queue_depth_max);
        self.server_scheduler_queued = Some(s.scheduler_queued);
        self.server_scheduler_due_critical = Some(s.scheduler_due_critical);
        self.server_scheduler_due_deferred = Some(s.scheduler_due_deferred);
        self.server_scheduler_critical_ceiling_hits = Some(s.scheduler_critical_ceiling_hits);
        self.server_scheduler_deferred_exhausted = Some(s.scheduler_deferred_exhausted);
        self.server_actions_active = Some(s.actions_active);
        self.server_events_produced = Some(s.events_produced);
        self.server_events_processed = Some(s.events_processed);
        self.server_spawn_queue_depth = Some(s.spawn_queue_depth);
        self.server_cadence_due = Some(s.cadence_due);
        self.server_command_rejects_gate = Some(s.command_rejects_gate);
        self.server_command_rejects_other = Some(s.command_rejects_other);
        self.server_domain_rev_advances = Some(s.domain_rev_advances);
        self.server_observer_pending_updates = Some(s.observer_pending_updates);
        self.server_observer_pending_enters = Some(s.observer_pending_enters);
        self.server_cadence_deferred_updates = Some(s.cadence_deferred_updates);
        self.server_scheduler_scheduled_total = Some(s.scheduler_scheduled_total);
        self.server_scheduler_cancelled_total = Some(s.scheduler_cancelled_total);
        self.server_scheduler_critical_executed_total = Some(s.scheduler_critical_executed_total);
        self.server_scheduler_deferred_executed_total = Some(s.scheduler_deferred_executed_total);
        self.server_actions_started_total = Some(s.actions_started_total);
        self.server_actions_completed_total = Some(s.actions_completed_total);
        self.server_effects_applied_total = Some(s.effects_applied_total);
        self.server_effects_expired_total = Some(s.effects_expired_total);
        self.server_spawn_requests_total = Some(s.spawn_requests_total);
        self.server_spawns_completed_total = Some(s.spawns_completed_total);
        self.server_despawns_completed_total = Some(s.despawns_completed_total);
        self.server_cadence_executions_total = Some(s.cadence_executions_total);
        self.server_entities_spawned_total = Some(s.entities_spawned_total);
    }
}

fn opt_u64(v: Option<u64>) -> String {
    v.map_or(String::new(), |x| x.to_string())
}

fn opt_f64(v: Option<f64>) -> String {
    v.map_or(String::new(), |x| format!("{x:.3}"))
}

#[derive(Clone, Debug)]
pub struct WriteSummaryArgs<'a> {
    pub status: RunStatus,
    pub reasons: &'a [StatusReason],
    pub metrics: &'a HarnessMetrics,
    pub peak_connected: u32,
    pub requested_bots: u32,
    pub server_metrics_ok: bool,
    pub server_metrics_health: &'a str,
    pub server_metrics_samples_ok: u64,
    pub server_metrics_samples_missed: u64,
    pub extras: SummaryExtras,
    pub failure_class: &'a str,
    pub requested_duration_secs: u64,
    pub preset: Option<String>,
    pub seed: u64,
    pub soak: Option<SoakEvidence>,
}

impl RunLog {
    pub fn create(cli: &Cli, spec: &LoadScenario) -> Result<Self, String> {
        Self::create_in(Path::new("logs/load"), cli, spec)
    }

    pub fn create_in(logs_base: &Path, cli: &Cli, spec: &LoadScenario) -> Result<Self, String> {
        fs::create_dir_all(logs_base).map_err(|e| format!("create logs dir: {e}"))?;

        let timestamp = chrono_like_timestamp();
        let kind = format!("{:?}", spec.kind).to_lowercase();
        let dir_name = format!(
            "{timestamp}_{count}bots_{kind}_seed{seed}_r{run_id}",
            count = spec.bot_count,
            seed = spec.seed,
            run_id = spec.run_id,
        );
        let dir = logs_base.join(&dir_name);
        fs::create_dir_all(&dir).map_err(|e| format!("create run dir: {e}"))?;

        let mut spec = spec.clone().with_persist_root(&dir);
        if spec.isolate_persist {
            let persist = match spec.persist_root.as_deref() {
                Some(existing) => PathBuf::from(existing),
                None => dir.join("persist"),
            };
            fs::create_dir_all(&persist).map_err(|e| format!("create persist dir: {e}"))?;
            spec.persist_root = Some(persist.to_string_lossy().replace('\\', "/"));
        }

        let scenario_json =
            serde_json::to_string_pretty(&spec).map_err(|e| format!("serialize scenario: {e}"))?;
        fs::write(dir.join("scenario.json"), scenario_json)
            .map_err(|e| format!("write scenario.json: {e}"))?;

        let env_pairs = spec.recommended_server_env();
        if !env_pairs.is_empty() {
            let env_json = serde_json::to_string_pretty(&env_pairs)
                .map_err(|e| format!("serialize server env: {e}"))?;
            fs::write(dir.join("server_env.json"), env_json)
                .map_err(|e| format!("write server_env.json: {e}"))?;
        }

        let config_path = dir.join("config.json");
        let config = RunConfig {
            count: spec.bot_count,
            profile: format!("{:?}", spec.profile).to_lowercase(),
            scenario: format!("{:?}", spec.connect).to_lowercase(),
            seed: spec.seed,
            duration_secs: spec.duration_secs,
            quiet_secs: spec.quiet_secs,
            ramp_ms: spec.ramp_ms,
            server: cli.server.to_string(),
            metrics_addr: cli.metrics.to_string(),
            client_build: crate::client_build(),
        };
        let config_json =
            serde_json::to_string_pretty(&config).map_err(|e| format!("serialize config: {e}"))?;
        fs::write(&config_path, config_json).map_err(|e| format!("write config: {e}"))?;

        let metrics_file = File::create(dir.join("metrics.csv"))
            .map_err(|e| format!("create metrics.csv: {e}"))?;
        let mut metrics_writer = BufWriter::new(metrics_file);
        writeln!(metrics_writer, "{METRICS_CSV_HEADER}")
            .map_err(|e| format!("write metrics header: {e}"))?;

        let events_file = File::create(dir.join("events.ndjson"))
            .map_err(|e| format!("create events.ndjson: {e}"))?;
        let events_writer = BufWriter::new(events_file);

        let latest_path = logs_base.join("latest.txt");
        fs::write(&latest_path, &dir_name).ok();
        fs::write(logs_base.join("current_run.txt"), &dir_name).ok();

        let mut log = Self {
            dir,
            logs_base: logs_base.to_path_buf(),
            config_path,
            metrics_writer,
            events_writer,
            start_time: Instant::now(),
        };
        log.emit_event(
            "run_start",
            json!({
                "count": spec.bot_count,
                "kind": kind,
                "connect": format!("{:?}", spec.connect).to_lowercase(),
                "seed": spec.seed,
                "run_id": spec.run_id,
                "isolate_persist": spec.isolate_persist,
            }),
        )?;
        Ok(log)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }

    pub fn emit_event(&mut self, kind: &str, fields: serde_json::Value) -> Result<(), String> {
        let mut obj = serde_json::Map::new();
        obj.insert("ts".into(), json!(self.elapsed_secs()));
        obj.insert("event".into(), json!(kind));
        if let Some(map) = fields.as_object() {
            for (k, v) in map {
                obj.insert(k.clone(), v.clone());
            }
        }
        self.write_event(&serde_json::Value::Object(obj))?;
        self.events_writer
            .flush()
            .map_err(|e| format!("flush events: {e}"))
    }

    pub fn write_metrics_sample(&mut self, sample: &MetricsSample) -> Result<(), String> {
        let cells: Vec<String> = vec![
            sample.elapsed_secs.to_string(),
            sample.target_bots.to_string(),
            sample.connected_bots.to_string(),
            sample.commands_sent.to_string(),
            sample.snapshots_received.to_string(),
            format!("{:.3}", sample.bot_scheduler_p50_ms),
            format!("{:.3}", sample.bot_scheduler_p95_ms),
            format!("{:.3}", sample.bot_scheduler_p99_ms),
            format!("{:.3}", sample.bot_scheduler_max_ms),
            opt_f64(sample.harness_memory_mb),
            opt_u64(sample.server_active_sessions),
            opt_u64(sample.server_active_player_entities),
            opt_f64(sample.server_tick_work_mean_ms),
            opt_f64(sample.server_tick_work_p50_ms),
            opt_f64(sample.server_tick_work_p95_ms),
            opt_f64(sample.server_tick_work_p99_ms),
            opt_f64(sample.server_tick_work_max_ms),
            opt_u64(sample.server_tick_overruns_total),
            opt_f64(sample.server_scheduler_lateness_p95_ms),
            opt_f64(sample.server_scheduler_lateness_max_ms),
            opt_f64(sample.server_input_msgs_per_sec),
            opt_f64(sample.server_snapshot_msgs_per_sec),
            opt_f64(sample.server_bytes_in_per_sec),
            opt_f64(sample.server_bytes_out_per_sec),
            opt_u64(sample.server_input_queue_current),
            opt_u64(sample.server_input_queue_max),
            opt_u64(sample.server_session_queue_max),
            opt_u64(sample.snapshot_build_count),
            opt_f64(sample.snapshot_build_time_max_ms),
            opt_f64(sample.snapshot_encode_time_max_ms),
            opt_u64(sample.snapshot_size_max_bytes),
            opt_u64(sample.snapshot_encode_failures_total),
            opt_f64(sample.server_memory_mb),
            opt_f64(sample.server_memory_peak_mb),
            opt_u64(sample.server_input_received),
            opt_u64(sample.server_input_accepted),
            opt_u64(sample.server_input_stale),
            opt_u64(sample.server_input_handoff_dropped),
            opt_u64(sample.server_input_rate_limited),
            u8::from(sample.input_active).to_string(),
            opt_u64(sample.server_aoi_enters),
            opt_u64(sample.server_aoi_leaves),
            opt_u64(sample.server_aoi_updates),
            opt_u64(sample.server_aoi_churn_reentry),
            opt_u64(sample.server_aoi_update_bytes),
            opt_u64(sample.server_oldest_pending_ticks),
            opt_u64(sample.server_max_deferred_ticks),
            opt_u64(sample.server_replication_queue_depth_max),
            opt_u64(sample.server_scheduler_queued),
            opt_u64(sample.server_scheduler_due_critical),
            opt_u64(sample.server_scheduler_due_deferred),
            opt_u64(sample.server_scheduler_critical_ceiling_hits),
            opt_u64(sample.server_scheduler_deferred_exhausted),
            opt_u64(sample.server_actions_active),
            opt_u64(sample.server_events_produced),
            opt_u64(sample.server_events_processed),
            opt_u64(sample.server_spawn_queue_depth),
            opt_u64(sample.server_cadence_due),
            opt_u64(sample.server_command_rejects_gate),
            opt_u64(sample.server_command_rejects_other),
            opt_u64(sample.server_domain_rev_advances),
            opt_u64(sample.server_observer_pending_updates),
            opt_u64(sample.server_observer_pending_enters),
            opt_u64(sample.server_cadence_deferred_updates),
            opt_u64(sample.server_scheduler_scheduled_total),
            opt_u64(sample.server_scheduler_cancelled_total),
            opt_u64(sample.server_scheduler_critical_executed_total),
            opt_u64(sample.server_scheduler_deferred_executed_total),
            opt_u64(sample.server_actions_started_total),
            opt_u64(sample.server_actions_completed_total),
            opt_u64(sample.server_effects_applied_total),
            opt_u64(sample.server_effects_expired_total),
            opt_u64(sample.server_spawn_requests_total),
            opt_u64(sample.server_spawns_completed_total),
            opt_u64(sample.server_despawns_completed_total),
            opt_u64(sample.server_cadence_executions_total),
            opt_u64(sample.server_entities_spawned_total),
        ];
        debug_assert_eq!(cells.len(), METRICS_CSV_HEADER.split(',').count());
        writeln!(self.metrics_writer, "{}", cells.join(","))
            .map_err(|e| format!("write metrics: {e}"))
    }

    pub fn write_live_status(&mut self, status: &serde_json::Value) -> Result<(), String> {
        let json = serde_json::to_string_pretty(status)
            .map_err(|e| format!("serialize live status: {e}"))?;
        fs::write(self.dir.join("live_status.json"), json)
            .map_err(|e| format!("write live_status.json: {e}"))
    }

    pub fn write_event(&mut self, event: &serde_json::Value) -> Result<(), String> {
        serde_json::to_writer(&mut self.events_writer, event)
            .map_err(|e| format!("write event: {e}"))?;
        writeln!(self.events_writer).map_err(|e| format!("write newline: {e}"))
    }

    pub fn flush(&mut self) -> Result<(), String> {
        self.metrics_writer
            .flush()
            .map_err(|e| format!("flush metrics: {e}"))?;
        self.events_writer
            .flush()
            .map_err(|e| format!("flush events: {e}"))
    }

    pub fn write_summary(&mut self, args: WriteSummaryArgs<'_>) -> Result<(), String> {
        let stop_event = if args.status == RunStatus::Aborted {
            "run_aborted"
        } else {
            "run_completed"
        };
        self.emit_event(
            stop_event,
            json!({
                "status": args.status.as_str(),
                "reasons": args.reasons,
            }),
        )?;

        let summary = RunSummary {
            run_status: args.status.as_str().to_string(),
            status_reasons: args.reasons.to_vec(),
            elapsed_secs: self.start_time.elapsed().as_secs_f64(),
            requested_bots: args.requested_bots,
            peak_connected: args.peak_connected,
            failure_class: args.failure_class.to_string(),
            total_commands_sent: args.metrics.bots.total_commands_sent,
            total_snapshots_received: args.metrics.bots.total_snapshots_received,
            overflow_events: args.metrics.overflow_events,
            encode_failures: args.metrics.encode_failures,
            snapshot_starvation_samples: args.metrics.snapshot_starvation_samples,
            unexpected_disconnects: args.metrics.unexpected_disconnects,
            admission_refusals: args.metrics.admission_refusals,
            server_metrics_ok: args.server_metrics_ok,
            server_metrics_health: args.server_metrics_health.to_string(),
            server_metrics_samples_ok: args.server_metrics_samples_ok,
            server_metrics_samples_missed: args.server_metrics_samples_missed,
            tick_percentile_semantics: crate::aggregate::TICK_PERCENTILE_SEMANTICS.to_string(),
            server_tick_overruns_total: args.extras.server_tick_overruns_total,
            server_tick_work_mean_ms: args.extras.server_tick_work_mean_ms,
            server_tick_work_p95_ms: args.extras.server_tick_work_p95_ms,
            server_tick_work_p99_ms: args.extras.server_tick_work_p99_ms,
            server_tick_work_max_ms: args.extras.server_tick_work_max_ms,
            server_scheduler_lateness_p95_ms: args.extras.server_scheduler_lateness_p95_ms,
            server_scheduler_lateness_max_ms: args.extras.server_scheduler_lateness_max_ms,
            server_bytes_in: args.extras.server_bytes_in,
            server_bytes_out: args.extras.server_bytes_out,
            server_memory_start_mb: args.extras.server_memory_start_mb,
            server_memory_peak_mb: args.extras.server_memory_peak_mb,
            server_memory_end_mb: args.extras.server_memory_end_mb,
            harness_memory_peak_mb: args.extras.harness_memory_peak_mb,
            server_input_queue_max: args.extras.server_input_queue_max,
            server_session_queue_max: args.extras.server_session_queue_max,
            input_handoff_dropped_total: args.extras.input_handoff_dropped_total,
            scheduler_queued_start: args.extras.scheduler_queued_start,
            scheduler_queued_end: args.extras.scheduler_queued_end,
            scheduler_queued_max: args.extras.scheduler_queued_max,
            actions_active_max: args.extras.actions_active_max,
            spawn_queue_depth_max: args.extras.spawn_queue_depth_max,
            aoi_enters_total: args.extras.aoi_enters_total,
            aoi_leaves_total: args.extras.aoi_leaves_total,
            aoi_updates_total: args.extras.aoi_updates_total,
            scheduler_critical_ceiling_hits: args.extras.scheduler_critical_ceiling_hits,
            scheduler_deferred_exhausted: args.extras.scheduler_deferred_exhausted,
            scheduler_scheduled_total: args.extras.scheduler_scheduled_total,
            scheduler_cancelled_total: args.extras.scheduler_cancelled_total,
            scheduler_critical_executed_total: args.extras.scheduler_critical_executed_total,
            scheduler_deferred_executed_total: args.extras.scheduler_deferred_executed_total,
            actions_started_total: args.extras.actions_started_total,
            actions_completed_total: args.extras.actions_completed_total,
            effects_applied_total: args.extras.effects_applied_total,
            effects_expired_total: args.extras.effects_expired_total,
            spawn_requests_total: args.extras.spawn_requests_total,
            spawns_completed_total: args.extras.spawns_completed_total,
            despawns_completed_total: args.extras.despawns_completed_total,
            cadence_executions_total: args.extras.cadence_executions_total,
            entities_spawned_total: args.extras.entities_spawned_total,
            events_produced_total: args.extras.events_produced_total,
            events_processed_total: args.extras.events_processed_total,
            requested_duration_secs: args.requested_duration_secs,
            preset: args.preset.clone(),
            seed: Some(args.seed),
            soak: args.soak.clone(),
        };
        let summary_json = serde_json::to_string_pretty(&summary)
            .map_err(|e| format!("serialize summary: {e}"))?;
        fs::write(self.dir.join("run_summary.json"), summary_json)
            .map_err(|e| format!("write summary: {e}"))?;

        let dir_name = self.dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        fs::write(self.logs_base.join("last_finished.txt"), dir_name).ok();
        if args.preset.is_some() {
            fs::write(self.logs_base.join("last_runtime_validation.txt"), dir_name).ok();
        }
        let _ = fs::remove_file(self.logs_base.join("current_run.txt"));

        self.flush()
    }
}

#[derive(Clone, Debug, Default)]
pub struct SummaryExtras {
    pub server_tick_overruns_total: Option<u64>,
    pub server_tick_work_mean_ms: Option<f64>,
    pub server_tick_work_p95_ms: Option<f64>,
    pub server_tick_work_p99_ms: Option<f64>,
    pub server_tick_work_max_ms: Option<f64>,
    pub server_scheduler_lateness_p95_ms: Option<f64>,
    pub server_scheduler_lateness_max_ms: Option<f64>,
    pub server_bytes_in: Option<u64>,
    pub server_bytes_out: Option<u64>,
    pub server_memory_start_mb: Option<f64>,
    pub server_memory_peak_mb: Option<f64>,
    pub server_memory_end_mb: Option<f64>,
    pub harness_memory_peak_mb: Option<f64>,
    pub server_input_queue_max: Option<u64>,
    pub server_session_queue_max: Option<u64>,
    pub input_handoff_dropped_total: Option<u64>,
    pub scheduler_queued_start: Option<u64>,
    pub scheduler_queued_end: Option<u64>,
    pub scheduler_queued_max: Option<u64>,
    pub actions_active_max: Option<u64>,
    pub spawn_queue_depth_max: Option<u64>,
    pub aoi_enters_total: Option<u64>,
    pub aoi_leaves_total: Option<u64>,
    pub aoi_updates_total: Option<u64>,
    pub scheduler_critical_ceiling_hits: Option<u64>,
    pub scheduler_deferred_exhausted: Option<u64>,
    pub scheduler_scheduled_total: Option<u64>,
    pub scheduler_cancelled_total: Option<u64>,
    pub scheduler_critical_executed_total: Option<u64>,
    pub scheduler_deferred_executed_total: Option<u64>,
    pub actions_started_total: Option<u64>,
    pub actions_completed_total: Option<u64>,
    pub effects_applied_total: Option<u64>,
    pub effects_expired_total: Option<u64>,
    pub spawn_requests_total: Option<u64>,
    pub spawns_completed_total: Option<u64>,
    pub despawns_completed_total: Option<u64>,
    pub cadence_executions_total: Option<u64>,
    pub entities_spawned_total: Option<u64>,
    pub events_produced_total: Option<u64>,
    pub events_processed_total: Option<u64>,
}

impl SummaryExtras {
    #[must_use]
    pub fn validation_execution(&self) -> ValidationExecution {
        ValidationExecution {
            scheduler_scheduled_total: self.scheduler_scheduled_total,
            scheduler_cancelled_total: self.scheduler_cancelled_total,
            scheduler_critical_executed_total: self.scheduler_critical_executed_total,
            scheduler_deferred_executed_total: self.scheduler_deferred_executed_total,
            actions_started_total: self.actions_started_total,
            actions_completed_total: self.actions_completed_total,
            effects_applied_total: self.effects_applied_total,
            effects_expired_total: self.effects_expired_total,
            spawn_requests_total: self.spawn_requests_total,
            spawns_completed_total: self.spawns_completed_total,
            despawns_completed_total: self.despawns_completed_total,
            cadence_executions_total: self.cadence_executions_total,
            entities_spawned_total: self.entities_spawned_total,
            events_produced_total: self.events_produced_total,
            events_processed_total: self.events_processed_total,
        }
    }
}

fn chrono_like_timestamp() -> String {
    format_unix_timestamp(
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    )
}

/// Civil date from Unix days (Howard Hinnant, public domain). Leap-year aware.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

fn format_unix_timestamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    let hour = (secs % 86_400) / 3600;
    let minute = (secs % 3600) / 60;
    let second = secs % 60;
    format!("{year:04}{month:02}{day:02}_{hour:02}{minute:02}{second:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_format() {
        let ts = chrono_like_timestamp();
        assert_eq!(ts.len(), 15);
        assert!(ts.contains('_'));
    }

    #[test]
    fn unix_epoch_formats_1970() {
        assert_eq!(format_unix_timestamp(0), "19700101_000000");
    }

    #[test]
    fn timestamp_is_leap_aware_not_365_day_months() {
        // 2026-08-31 00:00:00 UTC must not format as 20260916.
        let ts = format_unix_timestamp(1_788_134_400);
        assert!(ts.starts_with("20260831_"), "got {ts}");
        let ts_noon = format_unix_timestamp(1_788_177_600);
        assert!(ts_noon.starts_with("20260831_"), "got {ts_noon}");
    }

    #[test]
    fn metrics_header_is_stable() {
        assert!(METRICS_CSV_HEADER.contains("bot_scheduler_p99_ms"));
        assert!(METRICS_CSV_HEADER.contains("server_tick_work_p99_ms"));
        assert!(METRICS_CSV_HEADER.contains("server_bytes_out_per_sec"));
        assert!(METRICS_CSV_HEADER.contains("server_scheduler_queued"));
        assert!(METRICS_CSV_HEADER.contains("server_aoi_enters"));
        assert!(METRICS_CSV_HEADER.contains("server_cadence_deferred_updates"));
        assert!(METRICS_CSV_HEADER.contains("server_scheduler_scheduled_total"));
        assert!(METRICS_CSV_HEADER.contains("server_entities_spawned_total"));
        assert!(!METRICS_CSV_HEADER.contains(",tick_p99_ms,"));
    }

    #[test]
    fn missing_server_fields_write_empty_cells() {
        assert_eq!(opt_u64(None), "");
        assert_eq!(opt_f64(None), "");
        assert_eq!(opt_u64(Some(7)), "7");
        assert_eq!(opt_f64(Some(1.5)), "1.500");
    }

    #[test]
    fn run_dir_creation_emits_run_start_event() {
        let test_dir =
            std::env::temp_dir().join(format!("purgatory-load-log-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&test_dir);
        let cli = crate::cli::Cli {
            count: 5,
            profile: crate::BotProfile::Idle,
            scenario: crate::scenario::Scenario::Load,
            preset: None,
            seed: 12345,
            run_id: Some("testrun1".into()),
            duration: std::time::Duration::from_secs(10),
            timeout: None,
            ramp_ms: 100,
            quiet_secs: std::time::Duration::from_secs(15),
            churn_interval: std::time::Duration::from_secs(10),
            churn_fraction: 0.2,
            max_bots: 32,
            allow_high_count: false,
            isolate_persist: true,
            raw_persist: false,
            server: "127.0.0.1:5001".parse().unwrap(),
            metrics: "127.0.0.1:5002".parse().unwrap(),
            probe: false,
            print_server_env: false,
            persist_root: None,
            slow_drain_count: 0,
            post_ramp_thin: false,
            relax_portal_gate: false,
        };
        let spec = crate::scenario::LoadScenario::from_cli(&cli, None).expect("spec");
        let mut log = RunLog::create_in(&test_dir, &cli, &spec).expect("create");
        log.emit_event("ramp_target_reached", json!({ "connected": 5 }))
            .unwrap();
        log.flush().unwrap();
        let events = fs::read_to_string(log.dir().join("events.ndjson")).unwrap();
        assert!(events.contains("run_start"));
        assert!(events.contains("ramp_target_reached"));
        assert!(log.dir().join("scenario.json").is_file());
        assert!(log.dir().join("persist").is_dir());
        let scenario: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(log.dir().join("scenario.json")).unwrap())
                .unwrap();
        assert_eq!(scenario["seed"], 12345);
        assert_eq!(scenario["run_id"], "testrun1");
        let lines: Vec<_> = events.lines().filter(|l| !l.is_empty()).collect();
        assert!(lines.len() >= 2);
        for line in lines {
            let _: serde_json::Value = serde_json::from_str(line).expect("ndjson");
        }
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn csv_header_column_count_matches_sample_row() {
        let sample = MetricsSample::default();
        let header_n = METRICS_CSV_HEADER.split(',').count();
        let mut log = {
            let dir = std::env::temp_dir()
                .join(format!("purgatory-load-csv-test-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            let cli = crate::cli::Cli {
                count: 1,
                profile: crate::BotProfile::Idle,
                scenario: crate::scenario::Scenario::Load,
                preset: None,
                seed: 1,
                run_id: Some("csvtest1".into()),
                duration: std::time::Duration::from_secs(1),
                timeout: None,
                ramp_ms: 100,
                quiet_secs: std::time::Duration::from_secs(1),
                churn_interval: std::time::Duration::from_secs(10),
                churn_fraction: 0.2,
                max_bots: 32,
                allow_high_count: false,
                isolate_persist: false,
                raw_persist: false,
                server: "127.0.0.1:5001".parse().unwrap(),
                metrics: "127.0.0.1:5002".parse().unwrap(),
                probe: false,
                print_server_env: false,
                persist_root: None,
                slow_drain_count: 0,
                post_ramp_thin: false,
                relax_portal_gate: false,
            };
            let spec = crate::scenario::LoadScenario::from_cli(&cli, None).unwrap();
            RunLog::create_in(&dir, &cli, &spec).unwrap()
        };
        log.write_metrics_sample(&sample).unwrap();
        log.flush().unwrap();
        let csv = fs::read_to_string(log.dir().join("metrics.csv")).unwrap();
        let mut lines = csv.lines();
        let header = lines.next().unwrap();
        let row = lines.next().unwrap();
        assert_eq!(header.split(',').count(), header_n);
        assert_eq!(row.split(',').count(), header_n);
        let _ = fs::remove_dir_all(log.dir().parent().unwrap());
    }
}
