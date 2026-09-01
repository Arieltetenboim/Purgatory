use std::path::PathBuf;
use std::time::Instant;

use crate::backend::{ProcessBackend, SpawnSpec, StdProcessBackend};
use crate::config::{
    CLIENT_PACKAGE, CLIENT_STAGGER, CLIENT_STEM, LIFECYCLE_FAST, LIFECYCLE_IDLE, LISTEN_HOST,
    LISTEN_PORT, LOAD_BIN, LOAD_PACKAGE, LOAD_STEM, PROBE_RETRY, READY_TIMEOUT, RECOVERY_INTERVAL,
    SERVER_PACKAGE, SERVER_STEM,
};
use crate::health::{HealthSource, StdHealthSource};
use crate::identity::CodeIdentity;
use crate::instance::WorkspaceLock;
use crate::job::{CommandOutcome, HubCommand, JobId, JobOp, JobPhase};
use crate::load::{
    LoadJob, LoadLastResult, LoadSpec, LoadState, classify_load_exit, load_argv,
    load_mode_extra_env, load_probe_compatible, resolve_last_finished_run,
};
use crate::log_buffer::{
    ActivityLog, IncomingLog, append_file_line, drain_into_activity, ensure_log_dir, stamp_line,
};
use crate::log_tail::FileTail;
use crate::metrics_series::{METRICS_SERIES_CAP, MetricsSeries, read_metrics_series};
use crate::paths::{WorkspacePaths, find_in_path};
use crate::process::{
    BuildReason, CheckStatus, ListenerDiag, ProcessLifetime, ProcessOrigin, ServerLaunchOptions,
    ServerState, TrackedProcess,
};
use crate::settings::{BuildProfile, LogLevel};
use crate::validation::{
    ValidationJob, ValidationLastResult, ValidationLiveStatus, ValidationPaths, ValidationSpec,
    ValidationState, allocate_persist, classify_harness_exit, is_stale_binary_stderr,
    load_logs_root, merge_server_env, parse_print_server_env, read_live_status, read_pointer_dir,
    read_run_summary, rv_stamp, validation_argv,
};

pub type LiveHubSession = HubSession<StdProcessBackend, StdHealthSource>;

pub struct HubSession<B: ProcessBackend, H: HealthSource> {
    pub(crate) paths: WorkspacePaths,
    pub(crate) backend: B,
    pub(crate) health: H,
    cargo_path: Option<PathBuf>,
    identity: CodeIdentity,
    incoming: IncomingLog,
    pub(crate) activity: ActivityLog,
    pub(crate) state: ServerState,
    pub(crate) tracked: Option<TrackedProcess>,
    pub(crate) last_failure: Option<String>,
    pub(crate) job: JobPhase,
    next_job_id: JobId,
    pub(crate) build: Option<BuildSlot>,
    pub(crate) probe_pid: Option<u32>,
    pub(crate) probe_job_id: Option<JobId>,
    verify_started_at: Option<Instant>,
    probe_next_at: Instant,
    probe_logged_start: bool,
    probe_logged_fail: bool,
    probe_prep_attempted: bool,
    restart_after_stop: bool,
    pub(crate) connection: CheckStatus,
    connection_reason: String,
    pub(crate) metrics_ok: bool,
    pub(crate) listener: ListenerDiag,
    last_recovery: Option<Instant>,
    next_lifecycle_at: Instant,
    seen_alive: bool,
    #[allow(dead_code)]
    lock: Option<WorkspaceLock>,
    pub(crate) validation: Option<ValidationJob>,
    pub(crate) load: Option<LoadJob>,
    server_launch: ServerLaunchOptions,
    validation_restart_after_stop: bool,
    load_restart_after_stop: bool,
    server_log: FileTail,
    client_log: FileTail,
    load_log: FileTail,
    clients: Vec<TrackedProcess>,
    pending_clients: u32,
    client_serial: u32,
    client_stagger_until: Instant,
    last_client_live: usize,
    log_level: LogLevel,
}

pub(crate) struct BuildSlot {
    pub pid: u32,
    pub job_id: JobId,
    pub reason: BuildReason,
}

#[derive(Clone, Debug)]
pub struct HubSnapshot {
    pub identity: String,
    pub server_state: ServerState,
    pub process_alive: bool,
    pub process_origin: Option<ProcessOrigin>,
    pub pid: Option<u32>,
    pub health: CheckStatus,
    pub connection: CheckStatus,
    pub listener: ListenerDiag,
    pub job: JobPhase,
    pub last_failure: Option<String>,
    pub endpoint: String,
    pub build_line: String,
    pub can_start: bool,
    pub can_stop: bool,
    pub can_restart: bool,
    pub log_lines: Vec<String>,
    pub cargo_found: bool,
    pub phase: String,
    pub workspace: String,
    pub build_profile: String,
    pub log_dir: String,
    pub validation: ValidationState,
    pub validation_reason: Option<String>,
    pub can_start_validation: bool,
    pub can_stop_validation: bool,
    pub validation_live: ValidationLiveStatus,
    pub validation_last: ValidationLastResult,
    pub validation_active_label: Option<String>,
    pub validation_metrics: MetricsSeries,
    pub server_log_lines: Vec<String>,
    pub load: LoadState,
    pub load_reason: Option<String>,
    pub can_start_load: bool,
    pub can_stop_load: bool,
    pub load_live: ValidationLiveStatus,
    pub load_last: LoadLastResult,
    pub load_active_label: Option<String>,
    pub load_metrics: MetricsSeries,
    pub load_log_lines: Vec<String>,
    pub client_count: usize,
    pub pending_clients: u32,
    pub can_request_clients: bool,
    pub can_stop_clients: bool,
    pub client_log_lines: Vec<String>,
    pub log_level: LogLevel,
}

impl LiveHubSession {
    pub fn open() -> Result<Self, String> {
        let paths = WorkspacePaths::detect()?;
        Self::open_at(paths, Instant::now())
    }

    pub fn open_at(paths: WorkspacePaths, now: Instant) -> Result<Self, String> {
        HubSession::new(
            paths,
            StdProcessBackend::new(),
            StdHealthSource::new(),
            find_in_path("cargo"),
            now,
        )
    }
}

impl<B: ProcessBackend, H: HealthSource> HubSession<B, H> {
    pub fn new(
        paths: WorkspacePaths,
        backend: B,
        health: H,
        cargo_path: Option<PathBuf>,
        now: Instant,
    ) -> Result<Self, String> {
        ensure_log_dir(&paths.dev_log_dir())?;
        let lock = Some(WorkspaceLock::acquire(&paths)?);
        let identity = CodeIdentity::load(&paths);
        let server_log = FileTail::new(paths.dev_log_dir().join("server.log"), "server");
        let client_log = FileTail::new(paths.dev_log_dir().join("client.log"), "client");
        let load_log = FileTail::new(paths.dev_log_dir().join("load.log"), "load");
        let mut session = Self {
            paths,
            backend,
            health,
            cargo_path,
            identity,
            incoming: IncomingLog::new(),
            activity: ActivityLog::new(),
            state: ServerState::Stopped,
            tracked: None,
            last_failure: None,
            job: JobPhase::Idle,
            next_job_id: JobId(1),
            build: None,
            probe_pid: None,
            probe_job_id: None,
            verify_started_at: None,
            probe_next_at: now,
            probe_logged_start: false,
            probe_logged_fail: false,
            probe_prep_attempted: false,
            restart_after_stop: false,
            connection: CheckStatus::Unknown,
            connection_reason: String::new(),
            metrics_ok: false,
            listener: ListenerDiag::Unknown,
            last_recovery: None,
            next_lifecycle_at: now,
            seen_alive: false,
            lock,
            validation: None,
            load: None,
            server_launch: ServerLaunchOptions::default(),
            validation_restart_after_stop: false,
            load_restart_after_stop: false,
            server_log,
            client_log,
            load_log,
            clients: Vec::new(),
            pending_clients: 0,
            client_serial: 0,
            client_stagger_until: now,
            last_client_live: 0,
            log_level: LogLevel::Default,
        };
        session.log_hub(&format!("---- {} ----", session.identity.display()));
        session.startup_recovery(now);
        session.adopt_clients_on_startup();
        Ok(session)
    }

    pub fn paths(&self) -> &WorkspacePaths {
        &self.paths
    }

    pub fn identity(&self) -> &crate::identity::CodeIdentity {
        &self.identity
    }

    pub fn command(&mut self, cmd: HubCommand, now: Instant) -> CommandOutcome {
        match cmd {
            HubCommand::Start => self.request_start(now),
            HubCommand::Stop => self.request_stop(now, true),
            HubCommand::Restart => self.request_restart(now),
            HubCommand::StartValidation { spec } => self.request_start_validation(spec, now),
            HubCommand::StopValidation => self.request_stop_validation(now),
            HubCommand::StartLoad { spec } => self.request_start_load(spec, now),
            HubCommand::StopLoad => self.request_stop_load(now),
            HubCommand::AnalyzeLastRun => self.request_analyze_last_run(),
            HubCommand::OpenLoadLogs => self.request_open_load_logs(),
            HubCommand::OpenLastReport => self.request_open_last_report(),
            HubCommand::RequestClients { count } => self.request_clients(count, now),
            HubCommand::StopClients => self.request_stop_clients(),
            HubCommand::QualityGate => self.request_quality_gate(),
            HubCommand::Rebuild => self.request_rebuild(now),
            HubCommand::KillAll => self.request_kill_all(now),
            HubCommand::SetBuildProfile { profile } => self.set_build_profile(profile),
            HubCommand::SetLogLevel { level } => self.set_log_level(level),
            HubCommand::ClearActivityLog => {
                self.activity.clear();
                CommandOutcome::Accepted
            }
            HubCommand::ClearServerLog => {
                self.server_log.clear_view();
                CommandOutcome::Accepted
            }
            HubCommand::ClearClientLog => {
                self.client_log.clear_view();
                CommandOutcome::Accepted
            }
            HubCommand::ClearLoadLog => {
                self.load_log.clear_view();
                CommandOutcome::Accepted
            }
        }
    }

    pub fn tick(&mut self, now: Instant) {
        drain_into_activity(&self.incoming, &mut self.activity);
        self.server_log.poll();
        self.client_log.poll();
        self.load_log.poll();
        if now < self.next_lifecycle_at {
            return;
        }
        self.run_lifecycle(now);
        let fast = self.wants_fast_poll();
        self.next_lifecycle_at = now + if fast { LIFECYCLE_FAST } else { LIFECYCLE_IDLE };
    }

    pub fn snapshot(&mut self, _now: Instant) -> HubSnapshot {
        let alive = self.server_alive();
        let health = if self.metrics_ok {
            CheckStatus::Pass
        } else if self.state == ServerState::Stopped {
            CheckStatus::Unknown
        } else {
            CheckStatus::Fail
        };
        let mut build_line = format!("Build:  {}", self.paths.profile.as_str());
        if let Some(b) = &self.build {
            build_line = format!(
                "Build:  {}  ({})",
                self.paths.profile.as_str(),
                b.reason.as_str()
            );
        }
        self.prune_clients();
        let (live, last, validation_metrics) = self.observe_validation();
        let (load_live, load_metrics) = self.observe_load_presentation();
        let load_last = self.observe_load_last();
        let validation_active_label = self.validation.as_ref().map(|v| {
            let dur = v
                .spec
                .duration
                .map(|d| d.as_cli().to_string())
                .unwrap_or_else(|| "preset default".to_string());
            format!(
                "preset={}  duration={}  seed={}",
                v.spec.preset.as_str(),
                dur,
                v.spec.seed
            )
        });
        let load_active_label = self.load.as_ref().map(|j| {
            format!(
                "count={}  profile={}  scenario={}  duration={}  seed={}",
                j.spec.count,
                j.spec.profile.as_str(),
                j.spec.scenario.as_str(),
                j.spec.duration.as_cli(),
                j.spec.seed
            )
        });
        HubSnapshot {
            identity: self.identity.display(),
            server_state: self.state,
            process_alive: alive,
            process_origin: self.tracked.as_ref().map(|t| t.origin),
            pid: self.tracked.as_ref().map(|t| t.pid),
            health,
            connection: if self.state == ServerState::Stopped {
                CheckStatus::Unknown
            } else {
                self.connection
            },
            listener: self.listener,
            job: self.job,
            last_failure: if matches!(self.state, ServerState::Failed | ServerState::Degraded) {
                self.last_failure.clone()
            } else {
                None
            },
            endpoint: format!("{LISTEN_HOST}:{LISTEN_PORT}"),
            build_line,
            can_start: self.cargo_path.is_some()
                && (self.state == ServerState::Stopped
                    || (self.state == ServerState::Failed && !alive)),
            can_stop: self.state != ServerState::Stopped || alive,
            can_restart: self.cargo_path.is_some(),
            log_lines: self.activity.view_lines(),
            cargo_found: self.cargo_path.is_some(),
            phase: self.identity.phase.clone(),
            workspace: self.paths.root.display().to_string(),
            build_profile: self.paths.profile.as_str().to_string(),
            log_dir: self.paths.dev_log_dir().display().to_string(),
            validation: self.validation_phase(),
            validation_reason: self.validation.as_ref().and_then(|v| v.reason.clone()),
            can_start_validation: self.can_start_validation(),
            can_stop_validation: self.validation_is_active(),
            validation_live: live,
            validation_last: last,
            validation_active_label,
            validation_metrics,
            server_log_lines: self.server_log.view_lines(),
            load: self.load_phase(),
            load_reason: self.load.as_ref().and_then(|v| v.reason.clone()),
            can_start_load: self.can_start_load(),
            can_stop_load: self.load_is_active(),
            load_live,
            load_last,
            load_active_label,
            load_metrics,
            load_log_lines: self.load_log.view_lines(),
            client_count: self.clients.len(),
            pending_clients: self.pending_clients,
            can_request_clients: self.cargo_path.is_some(),
            can_stop_clients: !self.clients.is_empty()
                || self.pending_clients > 0
                || !self
                    .backend
                    .discover_workspace(CLIENT_STEM, &self.paths.target_prefix())
                    .is_empty(),
            client_log_lines: self.client_log.view_lines(),
            log_level: self.log_level,
        }
    }

    pub fn incoming_pending(&self) -> usize {
        self.incoming.pending_count()
    }

    fn wants_fast_poll(&self) -> bool {
        matches!(
            self.state,
            ServerState::Building
                | ServerState::Starting
                | ServerState::Verifying
                | ServerState::Stopping
        ) || self.build.is_some()
            || self.incoming.pending_count() > 0
            || self.validation_is_active()
            || self.load_is_active()
            || self.pending_clients > 0
    }

    fn alloc_job(&mut self) -> JobId {
        let id = self.next_job_id;
        self.next_job_id = id.next();
        id
    }

    fn set_running(&mut self, id: JobId, op: JobOp) {
        self.job = JobPhase::Running { id, op };
    }

    fn clear_job(&mut self) {
        self.job = JobPhase::Idle;
    }

    fn job_id(&self) -> Option<JobId> {
        self.job.running_id()
    }

    pub(crate) fn log_hub(&mut self, msg: &str) {
        let line = stamp_line(msg);
        self.activity.push(line.clone());
        let _ = append_file_line(&self.paths.dev_log_dir(), "launcher.log", &line);
    }

    fn set_state(&mut self, next: ServerState, reason: Option<&str>) {
        if let Some(r) = reason {
            self.last_failure = Some(r.to_string());
        }
        let prev = self.state;
        self.state = next;
        if prev != next {
            match reason {
                Some(r) => self.log_hub(&format!(
                    "Server {} -> {}: {r}",
                    prev.as_str(),
                    next.as_str()
                )),
                None => self.log_hub(&format!("Server {} -> {}", prev.as_str(), next.as_str())),
            }
        }
    }

    fn child_env(&self) -> Vec<(String, String)> {
        let mut env = vec![("RUST_BACKTRACE".to_string(), "1".to_string())];
        env.extend(self.log_level.child_env());
        env
    }

    pub(crate) fn server_alive(&mut self) -> bool {
        let Some(tracked) = self.tracked.clone() else {
            return false;
        };
        let alive = match tracked.origin {
            ProcessOrigin::Spawned => self.backend.is_alive(tracked.pid),
            ProcessOrigin::Adopted => self
                .backend
                .discover_workspace(SERVER_STEM, &self.paths.target_prefix())
                .iter()
                .any(|d| d.pid == tracked.pid && exe_paths_match(&d.exe_path, &tracked.exe_path)),
        };
        if alive {
            self.seen_alive = true;
        }
        alive
    }

    fn request_start(&mut self, now: Instant) -> CommandOutcome {
        if !self.validation_is_active() && !self.load_is_active() {
            self.server_launch.extra_env.clear();
        }
        let Some(cargo) = self.cargo_path.clone() else {
            self.log_hub("cargo is not on PATH");
            return CommandOutcome::Ignored;
        };
        if self.server_alive() {
            self.log_hub("Server already running");
            return CommandOutcome::Ignored;
        }
        if self.job.blocks_start() || (self.state == ServerState::Building && self.build.is_some())
        {
            self.log_hub("Server build already in progress");
            return CommandOutcome::Ignored;
        }
        let id = self.alloc_job();
        self.set_running(id, JobOp::Build);
        self.set_state(ServerState::Building, None);
        if !self.start_build(cargo, &[SERVER_PACKAGE], BuildReason::StartServer, id, now) {
            self.set_state(
                ServerState::Failed,
                Some(&format!(
                    "could not start cargo ({})",
                    self.cargo_path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                )),
            );
            self.clear_job();
            CommandOutcome::Ignored
        } else {
            CommandOutcome::Accepted
        }
    }

    fn request_restart(&mut self, now: Instant) -> CommandOutcome {
        self.restart_after_stop = true;
        if self.server_alive() || self.state == ServerState::Building {
            self.request_stop(now, false);
            self.restart_after_stop = true;
            return CommandOutcome::Accepted;
        }
        self.restart_after_stop = false;
        self.request_start(now)
    }

    fn request_stop(&mut self, now: Instant, _clear_load_queue: bool) -> CommandOutcome {
        self.abort_validation(
            ValidationState::Cancelled,
            "server stop cleared Runtime Validation",
        );
        self.abort_load(LoadState::Cancelled, "server stop cleared Load Test");
        let id = self.alloc_job();
        self.set_running(id, JobOp::Stop);
        self.stop_probe();
        let was_building = self.state == ServerState::Building;
        if let Some(build) = self.build.take() {
            self.backend.kill_tree(build.pid);
        }
        if self.server_alive() {
            self.set_state(ServerState::Stopping, None);
            if let Some(t) = &self.tracked {
                self.backend.kill_tree(t.pid);
            }
            return CommandOutcome::Accepted;
        }
        let discovered = self
            .backend
            .discover_workspace(SERVER_STEM, &self.paths.target_prefix());
        let killed = discovered.len();
        for d in discovered {
            self.backend.kill_tree(d.pid);
        }
        if killed == 0 && !was_building {
            self.tracked = None;
            self.set_state(ServerState::Stopped, None);
            self.reset_health();
            self.clear_job();
            self.log_hub("Server is not running");
            return CommandOutcome::Accepted;
        }
        self.set_state(ServerState::Stopping, None);
        let _ = now;
        CommandOutcome::Accepted
    }

    fn validation_phase(&self) -> ValidationState {
        self.validation
            .as_ref()
            .map(|v| v.phase)
            .unwrap_or(ValidationState::Idle)
    }

    fn validation_is_active(&self) -> bool {
        self.validation
            .as_ref()
            .is_some_and(|v| v.phase.is_active())
    }

    fn can_start_validation(&self) -> bool {
        self.cargo_path.is_some()
            && self.state == ServerState::Ready
            && !self.validation_is_active()
            && !self.load_is_active()
            && !self.job.blocks_start()
    }

    fn observe_validation(&self) -> (ValidationLiveStatus, ValidationLastResult, MetricsSeries) {
        let load_root = load_logs_root(&self.paths.root);
        let current = read_pointer_dir(&load_root, "current_run.txt");
        let live = if self.validation.as_ref().is_some_and(|v| {
            matches!(
                v.phase,
                ValidationState::Running | ValidationState::WaitingForReady
            )
        }) {
            current
                .as_ref()
                .map(|dir| read_live_status(dir))
                .unwrap_or_default()
        } else {
            ValidationLiveStatus::default()
        };
        let last_dir = read_pointer_dir(&load_root, "last_runtime_validation.txt")
            .or_else(|| read_pointer_dir(&load_root, "last_finished.txt"));
        let outcome = self.validation.as_ref().and_then(|v| {
            if v.phase.is_active() || v.phase == ValidationState::Idle {
                None
            } else {
                Some(v.phase)
            }
        });
        let summary = last_dir
            .as_ref()
            .map(|d| read_run_summary(d))
            .unwrap_or_default();
        let metrics_dir = if self.validation_is_active() {
            current.or(last_dir.clone())
        } else {
            last_dir.clone()
        };
        let metrics = metrics_dir
            .as_ref()
            .map(|d| read_metrics_series(d, METRICS_SERIES_CAP))
            .unwrap_or_default();
        (
            live,
            ValidationLastResult {
                outcome,
                dir: last_dir,
                summary,
            },
            metrics,
        )
    }

    fn observe_load_presentation(&self) -> (ValidationLiveStatus, MetricsSeries) {
        let load_root = load_logs_root(&self.paths.root);
        let current = read_pointer_dir(&load_root, "current_run.txt");
        let live = if self.load_is_active() {
            current
                .as_ref()
                .map(|dir| read_live_status(dir))
                .unwrap_or_default()
        } else {
            ValidationLiveStatus::default()
        };
        let last = resolve_last_finished_run(&self.paths.root, self.load_is_active());
        let metrics_dir = if self.load_is_active() {
            current.or(last)
        } else {
            last
        };
        let metrics = metrics_dir
            .as_ref()
            .map(|d| read_metrics_series(d, METRICS_SERIES_CAP))
            .unwrap_or_default();
        (live, metrics)
    }

    fn set_validation_phase(&mut self, phase: ValidationState) {
        if let Some(job) = &mut self.validation {
            job.phase = phase;
        }
    }

    fn finish_validation(&mut self, phase: ValidationState, reason: Option<String>) {
        if let Some(job) = &mut self.validation {
            job.phase = phase;
            job.reason = reason;
            job.harness_pid = None;
        }
        if matches!(
            self.job,
            JobPhase::Running {
                op: JobOp::Validate,
                ..
            } | JobPhase::Cancelling {
                op: JobOp::Validate,
                ..
            }
        ) {
            self.clear_job();
        }
    }

    fn abort_validation(&mut self, phase: ValidationState, reason: &str) {
        if !self.validation_is_active() {
            return;
        }
        if let Some(pid) = self.validation.as_ref().and_then(|v| v.harness_pid) {
            self.backend.kill_tree(pid);
        }
        if self
            .build
            .as_ref()
            .is_some_and(|b| b.reason == BuildReason::ValidatePrep)
            && let Some(build) = self.build.take()
        {
            self.backend.kill_tree(build.pid);
        }
        self.validation_restart_after_stop = false;
        self.log_hub(reason);
        self.finish_validation(phase, Some(reason.to_string()));
    }

    fn request_start_validation(&mut self, spec: ValidationSpec, now: Instant) -> CommandOutcome {
        if self.state != ServerState::Ready {
            self.log_hub("Runtime Validation requires Server Ready");
            return CommandOutcome::Ignored;
        }
        if self.validation_is_active() {
            self.log_hub("Runtime Validation already running");
            return CommandOutcome::Ignored;
        }
        if self.load_is_active() {
            self.log_hub("Refuse: Load Test already running");
            return CommandOutcome::Ignored;
        }
        if !self
            .backend
            .discover_workspace(LOAD_STEM, &self.paths.target_prefix())
            .is_empty()
        {
            self.log_hub("Refuse: purgatory-load already running in this workspace");
            return CommandOutcome::Ignored;
        }
        let Some(cargo) = self.cargo_path.clone() else {
            self.log_hub("cargo is not on PATH");
            return CommandOutcome::Ignored;
        };
        let spec = spec.normalized();
        let stamp = rv_stamp();
        let persist_root = allocate_persist(&self.paths.root, &stamp);
        if let Err(e) = std::fs::create_dir_all(&persist_root) {
            self.log_hub(&format!("could not create persist root: {e}"));
            return CommandOutcome::Ignored;
        }
        let argv = validation_argv(&spec, &persist_root);
        let id = self.alloc_job();
        self.set_running(id, JobOp::Validate);
        self.validation = Some(ValidationJob {
            job_id: id,
            spec,
            phase: ValidationState::Building,
            paths: ValidationPaths {
                stamp,
                persist_root,
            },
            argv,
            harness_pid: None,
            reason: None,
        });
        self.log_hub("Runtime Validation: rebuilding purgatory-load (runtime-val-prep)");
        if !self.start_build(cargo, &[LOAD_PACKAGE], BuildReason::ValidatePrep, id, now) {
            self.finish_validation(
                ValidationState::OrchestrationFailed,
                Some("could not start cargo for runtime-val-prep".to_string()),
            );
            CommandOutcome::Ignored
        } else {
            CommandOutcome::Accepted
        }
    }

    fn request_stop_validation(&mut self, now: Instant) -> CommandOutcome {
        let _ = now;
        if !self.validation_is_active() {
            self.log_hub("Runtime Validation is not running");
            return CommandOutcome::Ignored;
        }
        self.set_validation_phase(ValidationState::Cancelling);
        self.abort_validation(ValidationState::Cancelled, "Runtime Validation cancelled");
        CommandOutcome::Accepted
    }

    fn load_phase(&self) -> LoadState {
        self.load
            .as_ref()
            .map(|v| v.phase)
            .unwrap_or(LoadState::Idle)
    }

    fn load_is_active(&self) -> bool {
        self.load.as_ref().is_some_and(|v| v.phase.is_active())
    }

    fn can_start_load(&self) -> bool {
        self.cargo_path.is_some()
            && !self.load_is_active()
            && !self.validation_is_active()
            && !self.job.blocks_start()
            && (self.state == ServerState::Ready
                || self.state == ServerState::Stopped
                || self.state == ServerState::Failed)
    }

    fn observe_load_last(&self) -> LoadLastResult {
        let dir = resolve_last_finished_run(&self.paths.root, self.load_is_active());
        let outcome = self.load.as_ref().and_then(|v| {
            if v.phase.is_active() || v.phase == LoadState::Idle {
                None
            } else {
                Some(v.phase)
            }
        });
        let summary = dir
            .as_ref()
            .map(|d| read_run_summary(d))
            .unwrap_or_default();
        LoadLastResult {
            outcome,
            dir,
            summary,
        }
    }

    fn set_load_phase(&mut self, phase: LoadState) {
        if let Some(job) = &mut self.load {
            job.phase = phase;
        }
    }

    fn finish_load(&mut self, phase: LoadState, reason: Option<String>) {
        if let Some(job) = &mut self.load {
            job.phase = phase;
            job.reason = reason;
            job.harness_pid = None;
        }
        if matches!(
            self.job,
            JobPhase::Running {
                op: JobOp::Load,
                ..
            } | JobPhase::Cancelling {
                op: JobOp::Load,
                ..
            }
        ) {
            self.clear_job();
        }
    }

    fn abort_load(&mut self, phase: LoadState, reason: &str) {
        if !self.load_is_active() {
            return;
        }
        if let Some(pid) = self.load.as_ref().and_then(|v| v.harness_pid) {
            self.backend.kill_tree(pid);
        }
        for d in self
            .backend
            .discover_workspace(LOAD_STEM, &self.paths.target_prefix())
        {
            self.backend.kill_tree(d.pid);
        }
        self.load_restart_after_stop = false;
        self.log_hub(reason);
        self.finish_load(phase, Some(reason.to_string()));
    }

    fn request_start_load(&mut self, spec: LoadSpec, now: Instant) -> CommandOutcome {
        if self.load_is_active() {
            self.log_hub("Load Test already running");
            return CommandOutcome::Ignored;
        }
        if self.validation_is_active() {
            self.log_hub("Refuse: Runtime Validation already running");
            return CommandOutcome::Ignored;
        }
        if !self
            .backend
            .discover_workspace(LOAD_STEM, &self.paths.target_prefix())
            .is_empty()
        {
            self.log_hub("Refuse: purgatory-load already running in this workspace");
            return CommandOutcome::Ignored;
        }
        if self.cargo_path.is_none() {
            self.log_hub("cargo is not on PATH");
            return CommandOutcome::Ignored;
        }
        let spec = spec.normalized();
        let metrics = self.health.poll_metrics(now);
        let compatible =
            load_probe_compatible(metrics.as_ref(), spec.count) && self.state == ServerState::Ready;
        let max_bots = metrics
            .as_ref()
            .map(|m| m.admission_cap as u32)
            .unwrap_or(u32::from(crate::config::LOAD_ADMISSION_CAP))
            .max(spec.count);
        let argv = load_argv(
            &spec,
            max_bots.max(u32::from(crate::config::LOAD_ADMISSION_CAP)),
        );
        let id = self.alloc_job();
        self.set_running(id, JobOp::Load);
        self.load = Some(LoadJob {
            job_id: id,
            spec: spec.clone(),
            phase: if compatible {
                LoadState::Running
            } else {
                LoadState::PreparingServer
            },
            argv: argv.clone(),
            harness_pid: None,
            reason: None,
            needs_restart: !compatible,
        });
        if compatible {
            self.log_hub(&format!(
                "Starting load test: {} bots / {} / {} / seed {}",
                spec.count,
                spec.profile.as_str(),
                spec.duration.as_cli(),
                spec.seed
            ));
            if !self.spawn_load_harness() {
                return CommandOutcome::Ignored;
            }
            return CommandOutcome::Accepted;
        }
        self.log_hub("Load Test: restarting server in load-mode (START LOAD is restart consent)");
        self.server_launch.extra_env = load_mode_extra_env();
        self.stop_server_for_load_restart(now);
        CommandOutcome::Accepted
    }

    fn request_stop_load(&mut self, now: Instant) -> CommandOutcome {
        let _ = now;
        if !self.load_is_active() {
            self.log_hub("Load Test is not running");
            return CommandOutcome::Ignored;
        }
        self.set_load_phase(LoadState::Cancelling);
        self.abort_load(LoadState::Cancelled, "Load Test stopped");
        CommandOutcome::Accepted
    }

    fn stop_server_for_load_restart(&mut self, now: Instant) {
        self.load_restart_after_stop = true;
        self.stop_probe();
        if self.server_alive() {
            self.set_state(ServerState::Stopping, None);
            if let Some(t) = &self.tracked {
                self.backend.kill_tree(t.pid);
            }
            self.set_load_phase(LoadState::PreparingServer);
            return;
        }
        self.set_load_phase(LoadState::WaitingForReady);
        self.start_server_process(now);
    }

    fn spawn_load_harness(&mut self) -> bool {
        let Some(job) = &self.load else {
            return false;
        };
        if job.harness_pid.is_some() {
            return true;
        }
        if !self.paths.load_exe().is_file() {
            self.finish_load(
                LoadState::OrchestrationFailed,
                Some("purgatory-load executable missing".to_string()),
            );
            return false;
        }
        let argv = job.argv.clone();
        let spec = SpawnSpec {
            program: self.paths.load_exe(),
            args: argv,
            cwd: self.paths.root.clone(),
            env: self.child_env(),
            log_name: "load",
            ui_pump: false,
            lifetime: ProcessLifetime::Session,
        };
        match self
            .backend
            .spawn(spec, &self.incoming, &self.paths.dev_log_dir())
        {
            Ok(pid) => {
                if let Some(job) = &mut self.load {
                    job.harness_pid = Some(pid);
                    job.phase = LoadState::Running;
                    job.needs_restart = false;
                }
                self.log_hub("Load harness started");
                true
            }
            Err(e) => {
                self.finish_load(
                    LoadState::OrchestrationFailed,
                    Some(format!("failed to start purgatory-load: {e}")),
                );
                false
            }
        }
    }

    fn maybe_spawn_load_harness(&mut self) -> bool {
        let Some(job) = &self.load else {
            return false;
        };
        if !job.phase.is_active() {
            return false;
        }
        if job.phase == LoadState::WaitingForReady || job.needs_restart {
            if job.harness_pid.is_some() {
                return true;
            }
            return self.spawn_load_harness();
        }
        job.phase.is_active()
    }

    fn complete_load_harness_if_exited(&mut self) {
        let Some(pid) = self.load.as_ref().and_then(|v| v.harness_pid) else {
            return;
        };
        let Some(code) = self.backend.try_wait(pid) else {
            return;
        };
        let outcome = classify_load_exit(code);
        self.log_hub(&format!(
            "Load Test {} (harness exit {code})",
            outcome.as_str()
        ));
        self.finish_load(outcome, None);
    }

    fn request_analyze_last_run(&mut self) -> CommandOutcome {
        let Some(dir) = resolve_last_finished_run(&self.paths.root, self.load_is_active()) else {
            self.log_hub("No finished run found under logs/load");
            return CommandOutcome::Ignored;
        };
        let analyzer = self.paths.root.join("tools").join("analyze_load_run.py");
        if !analyzer.is_file() {
            self.log_hub("tools/analyze_load_run.py missing");
            return CommandOutcome::Ignored;
        }
        let py = find_in_path("python").or_else(|| find_in_path("py"));
        let Some(py) = py else {
            self.log_hub("Python not found on PATH for analyzer");
            return CommandOutcome::Ignored;
        };
        let cmd = format!(
            "\"{}\" \"{}\" \"{}\" & echo. & echo Analyzer finished. & pause",
            py.display(),
            analyzer.display(),
            dir.display()
        );
        let spec = SpawnSpec {
            program: PathBuf::from("cmd.exe"),
            args: vec!["/k".to_string(), cmd],
            cwd: self.paths.root.clone(),
            env: self.child_env(),
            log_name: "analyze",
            ui_pump: false,
            lifetime: ProcessLifetime::Detached,
        };
        match self.backend.spawn_visible(spec) {
            Ok(_) => {
                self.log_hub(&format!("Analyze last finished: {}", dir.display()));
                CommandOutcome::Accepted
            }
            Err(e) => {
                self.log_hub(&format!("Analyze failed to start: {e}"));
                CommandOutcome::Ignored
            }
        }
    }

    fn request_open_load_logs(&mut self) -> CommandOutcome {
        let root = crate::validation::load_logs_root(&self.paths.root);
        let _ = std::fs::create_dir_all(&root);
        open_explorer(&root);
        self.log_hub(&format!("Opened load logs: {}", root.display()));
        CommandOutcome::Accepted
    }

    fn request_open_last_report(&mut self) -> CommandOutcome {
        let Some(dir) = resolve_last_finished_run(&self.paths.root, self.load_is_active()) else {
            self.log_hub("No finished run.");
            return CommandOutcome::Ignored;
        };
        let report = dir.join("report");
        if report.is_dir() {
            open_explorer(&report);
            self.log_hub(&format!("Opened last report: {}", report.display()));
        } else {
            open_explorer(&dir);
            self.log_hub(&format!(
                "No report/ yet; opened run folder: {}",
                dir.display()
            ));
        }
        CommandOutcome::Accepted
    }

    fn prune_clients(&mut self) {
        self.clients.retain(|c| match c.origin {
            ProcessOrigin::Spawned => self.backend.is_alive(c.pid),
            ProcessOrigin::Adopted => self
                .backend
                .discover_workspace(CLIENT_STEM, &self.paths.target_prefix())
                .iter()
                .any(|d| d.pid == c.pid && exe_paths_match(&d.exe_path, &c.exe_path)),
        });
        let live = self.clients.len();
        if live < self.last_client_live {
            let dropped = self.last_client_live - live;
            if dropped == 1 {
                self.log_hub("Client closed");
            } else {
                self.log_hub(&format!("{dropped} clients closed"));
            }
        }
        self.last_client_live = live;
    }

    fn client_exe_locked_or_running(&mut self) -> bool {
        if !self.clients.is_empty() {
            return true;
        }
        if !self
            .backend
            .discover_workspace(CLIENT_STEM, &self.paths.target_prefix())
            .is_empty()
        {
            return true;
        }
        exe_appears_locked(&self.paths.client_exe())
    }

    fn request_clients(&mut self, count: u32, now: Instant) -> CommandOutcome {
        if self.cargo_path.is_none() {
            self.log_hub("cargo is not on PATH");
            return CommandOutcome::Ignored;
        }
        let count = count.clamp(1, 3);
        self.pending_clients = self.pending_clients.saturating_add(count);
        self.log_hub(&format!(
            "Client request +{count} (queued={})",
            self.pending_clients
        ));
        if self.state != ServerState::Ready {
            self.log_hub(&format!(
                "Clients queued until server is Ready (state={})",
                self.state.as_str()
            ));
            return CommandOutcome::Accepted;
        }
        if self.client_exe_locked_or_running() {
            self.log_hub(
                "WARNING: client already running; cannot rebuild (exe locked). Stop clients first to pick up a new build. Launching existing exe.",
            );
            self.drain_client_queue(now);
            return CommandOutcome::Accepted;
        }
        if let Some(cargo) = self.cargo_path.clone() {
            let id = self.alloc_job();
            self.set_running(id, JobOp::Build);
            if !self.start_build(cargo, &[CLIENT_PACKAGE], BuildReason::OpenClient, id, now) {
                self.clear_job();
                self.drain_client_queue(now);
            }
        }
        CommandOutcome::Accepted
    }

    fn drain_client_queue(&mut self, now: Instant) {
        if self.state != ServerState::Ready {
            return;
        }
        if self.pending_clients == 0 {
            return;
        }
        if now < self.client_stagger_until {
            return;
        }
        // Never launch while open-client cargo is rewriting the exe (Access Denied).
        if self
            .build
            .as_ref()
            .is_some_and(|b| b.reason == BuildReason::OpenClient)
        {
            return;
        }
        if !self.paths.client_exe().is_file() {
            if self.build.is_none()
                && let Some(cargo) = self.cargo_path.clone()
            {
                let id = self.alloc_job();
                self.set_running(id, JobOp::Build);
                let _ =
                    self.start_build(cargo, &[CLIENT_PACKAGE], BuildReason::OpenClient, id, now);
            }
            return;
        }
        if self.start_one_client() {
            self.pending_clients = self.pending_clients.saturating_sub(1);
            if self.pending_clients > 0 {
                self.client_stagger_until = now + CLIENT_STAGGER;
            }
        } else {
            self.log_hub(&format!(
                "Client launch failed; remaining queue={}",
                self.pending_clients
            ));
        }
    }

    fn start_one_client(&mut self) -> bool {
        let exe = self.paths.client_exe();
        if !exe.is_file() {
            self.log_hub("Client executable missing");
            return false;
        }
        self.client_serial = self.client_serial.saturating_add(1);
        let n = self.client_serial;
        let spec = SpawnSpec {
            program: exe.clone(),
            args: Vec::new(),
            cwd: self.paths.root.clone(),
            env: self.child_env(),
            log_name: "client",
            ui_pump: false,
            lifetime: ProcessLifetime::Detached,
        };
        match self
            .backend
            .spawn(spec, &self.incoming, &self.paths.dev_log_dir())
        {
            Ok(pid) => {
                self.clients.push(TrackedProcess {
                    pid,
                    origin: ProcessOrigin::Spawned,
                    exe_path: exe.clone(),
                });
                self.last_client_live = self.clients.len();
                self.log_hub(&format!("Opening client {n} EXE={}", exe.display()));
                true
            }
            Err(e) => {
                self.log_hub(&format!("Client {n} failed to start: {e}"));
                false
            }
        }
    }

    fn request_stop_clients(&mut self) -> CommandOutcome {
        self.pending_clients = 0;
        let mut n = 0;
        for c in std::mem::take(&mut self.clients) {
            self.backend.kill_tree(c.pid);
            n += 1;
        }
        let extra = self
            .backend
            .discover_workspace(CLIENT_STEM, &self.paths.target_prefix());
        let extra_n = extra.len();
        for d in extra {
            self.backend.kill_tree(d.pid);
        }
        self.last_client_live = 0;
        if n == 0 && extra_n == 0 {
            self.log_hub("No client processes");
        } else {
            self.log_hub("Stopped client(s)");
        }
        CommandOutcome::Accepted
    }

    fn adopt_clients_on_startup(&mut self) {
        let found = self
            .backend
            .discover_workspace(CLIENT_STEM, &self.paths.target_prefix());
        for d in found {
            if self.clients.iter().any(|c| c.pid == d.pid) {
                continue;
            }
            self.client_serial = self.client_serial.saturating_add(1);
            self.clients.push(TrackedProcess {
                pid: d.pid,
                origin: ProcessOrigin::Adopted,
                exe_path: d.exe_path.clone(),
            });
            self.log_hub(&format!("Adopted client pid {}", d.pid));
        }
        self.last_client_live = self.clients.len();
    }

    fn request_quality_gate(&mut self) -> CommandOutcome {
        if self.cargo_path.is_none() {
            self.log_hub("cargo is not on PATH");
            return CommandOutcome::Ignored;
        }
        let gate = self.paths.check_script();
        if !gate.is_file() {
            self.log_hub("scripts/check.ps1 missing");
            return CommandOutcome::Ignored;
        }
        let spec = SpawnSpec {
            program: PathBuf::from("powershell.exe"),
            args: vec![
                "-NoProfile".to_string(),
                "-ExecutionPolicy".to_string(),
                "Bypass".to_string(),
                "-NoExit".to_string(),
                "-File".to_string(),
                gate.display().to_string(),
            ],
            cwd: self.paths.root.clone(),
            env: self.child_env(),
            log_name: "quality-gate",
            ui_pump: false,
            lifetime: ProcessLifetime::Detached,
        };
        match self.backend.spawn_visible(spec) {
            Ok(_) => {
                self.log_hub("Quality gate started");
                CommandOutcome::Accepted
            }
            Err(e) => {
                self.log_hub(&format!("Quality gate failed to start: {e}"));
                CommandOutcome::Ignored
            }
        }
    }

    fn request_rebuild(&mut self, now: Instant) -> CommandOutcome {
        let Some(cargo) = self.cargo_path.clone() else {
            self.log_hub("cargo is not on PATH");
            return CommandOutcome::Ignored;
        };
        let mut packages: Vec<&str> = Vec::new();
        let server_running = !self
            .backend
            .discover_workspace(SERVER_STEM, &self.paths.target_prefix())
            .is_empty()
            || self.server_alive();
        if server_running {
            self.log_hub("Rebuild skips server (purgatory-server.exe is running; Stop first)");
        } else {
            packages.push(SERVER_PACKAGE);
        }
        let clients_running = self.client_exe_locked_or_running();
        if clients_running {
            self.log_hub(&format!(
                "Rebuild {}: skips client (clients are running)",
                self.paths.profile.as_str()
            ));
        } else {
            packages.push(CLIENT_PACKAGE);
        }
        if packages.is_empty() {
            self.log_hub("Rebuild skipped: server and client are running (Stop first)");
            return CommandOutcome::Ignored;
        }
        let id = self.alloc_job();
        self.set_running(id, JobOp::Build);
        if !self.start_build(cargo, &packages, BuildReason::Rebuild, id, now) {
            self.clear_job();
            return CommandOutcome::Ignored;
        }
        CommandOutcome::Accepted
    }

    fn request_kill_all(&mut self, now: Instant) -> CommandOutcome {
        let _ = now;
        self.pending_clients = 0;
        self.load_restart_after_stop = false;
        self.validation_restart_after_stop = false;
        self.restart_after_stop = false;
        self.abort_validation(
            ValidationState::Cancelled,
            "Kill All cleared Runtime Validation",
        );
        self.abort_load(LoadState::Cancelled, "Kill All cleared Load Test");
        self.stop_probe();
        if let Some(build) = self.build.take() {
            self.backend.kill_tree(build.pid);
        }
        let _cargo_n = self.backend.kill_workspace_cargo(&self.paths.root);
        let servers = self
            .backend
            .discover_workspace(SERVER_STEM, &self.paths.target_prefix());
        let server_n = servers.len();
        for d in servers {
            self.backend.kill_tree(d.pid);
        }
        if let Some(t) = self.tracked.take() {
            self.backend.kill_tree(t.pid);
        }
        let mut client_n = 0usize;
        for c in std::mem::take(&mut self.clients) {
            self.backend.kill_tree(c.pid);
            client_n += 1;
        }
        let clients = self
            .backend
            .discover_workspace(CLIENT_STEM, &self.paths.target_prefix());
        client_n += clients.len();
        for d in clients {
            self.backend.kill_tree(d.pid);
        }
        self.last_client_live = 0;
        let loads = self
            .backend
            .discover_workspace(LOAD_STEM, &self.paths.target_prefix());
        let load_n = loads.len();
        for d in loads {
            self.backend.kill_tree(d.pid);
        }
        self.seen_alive = false;
        self.set_state(ServerState::Stopped, None);
        self.reset_health();
        self.clear_job();
        self.log_hub(&format!(
            "Kill all  (server={server_n}  clients={client_n}  load={load_n})"
        ));
        CommandOutcome::Accepted
    }

    fn set_build_profile(&mut self, profile: BuildProfile) -> CommandOutcome {
        self.paths.set_profile(profile);
        self.log_hub(&format!("Profile: {}", profile.as_str()));
        CommandOutcome::Accepted
    }

    fn set_log_level(&mut self, level: LogLevel) -> CommandOutcome {
        self.log_level = level;
        self.log_hub(&format!("Log level: {}", level.as_str()));
        CommandOutcome::Accepted
    }

    fn stop_server_for_validation_restart(&mut self, now: Instant) {
        self.validation_restart_after_stop = true;
        self.stop_probe();
        if self.server_alive() {
            self.set_state(ServerState::Stopping, None);
            if let Some(t) = &self.tracked {
                self.backend.kill_tree(t.pid);
            }
            self.set_validation_phase(ValidationState::PreparingServer);
            return;
        }
        self.set_validation_phase(ValidationState::WaitingForReady);
        self.start_server_process(now);
    }

    fn begin_print_server_env(&mut self, now: Instant) {
        let persist = self
            .validation
            .as_ref()
            .map(|v| v.paths.persist_root.clone());
        let Some(persist) = persist else {
            return;
        };
        let spec = SpawnSpec {
            program: self.paths.load_exe(),
            args: vec!["--print-server-env".to_string()],
            cwd: self.paths.root.clone(),
            env: self.child_env(),
            log_name: "load",
            ui_pump: false,
            lifetime: ProcessLifetime::Session,
        };
        match self.backend.run_capture(spec) {
            Ok(out) => {
                if out.exit != 0 || is_stale_binary_stderr(&out.stderr) {
                    self.log_hub(&format!(
                        "Runtime Validation: --print-server-env failed exit={} (stale binary is hard-fail)",
                        out.exit
                    ));
                    self.finish_validation(
                        ValidationState::OrchestrationFailed,
                        Some(format!(
                            "--print-server-env failed exit {}{}",
                            out.exit,
                            if is_stale_binary_stderr(&out.stderr) {
                                "; unexpected argument (stale binary)"
                            } else {
                                ""
                            }
                        )),
                    );
                    return;
                }
                match parse_print_server_env(&out.stdout) {
                    Ok(printed) => {
                        self.server_launch.extra_env = merge_server_env(&persist, printed);
                        self.log_hub(
                            "Runtime Validation: restarting server with load-mode ExtraEnv",
                        );
                        self.stop_server_for_validation_restart(now);
                    }
                    Err(e) => {
                        self.finish_validation(ValidationState::OrchestrationFailed, Some(e));
                    }
                }
            }
            Err(e) => {
                self.finish_validation(
                    ValidationState::OrchestrationFailed,
                    Some(format!("--print-server-env: {e}")),
                );
            }
        }
    }

    fn maybe_spawn_validation_harness(&mut self) -> bool {
        let Some(job) = &self.validation else {
            return false;
        };
        if job.phase != ValidationState::WaitingForReady {
            return job.phase.is_active();
        }
        if job.harness_pid.is_some() {
            return true;
        }
        let argv = job.argv.clone();
        let spec = SpawnSpec {
            program: self.paths.load_exe(),
            args: argv,
            cwd: self.paths.root.clone(),
            env: self.child_env(),
            log_name: "load",
            ui_pump: false,
            lifetime: ProcessLifetime::Session,
        };
        match self
            .backend
            .spawn(spec, &self.incoming, &self.paths.dev_log_dir())
        {
            Ok(pid) => {
                if let Some(job) = &mut self.validation {
                    job.harness_pid = Some(pid);
                    job.phase = ValidationState::Running;
                }
                self.log_hub("Runtime Validation harness started");
                true
            }
            Err(e) => {
                self.finish_validation(
                    ValidationState::OrchestrationFailed,
                    Some(format!("failed to start purgatory-load: {e}")),
                );
                false
            }
        }
    }

    fn complete_validation_harness_if_exited(&mut self) {
        let Some(pid) = self.validation.as_ref().and_then(|v| v.harness_pid) else {
            return;
        };
        let Some(code) = self.backend.try_wait(pid) else {
            return;
        };
        let outcome = classify_harness_exit(code);
        self.log_hub(&format!(
            "Runtime Validation {} (harness exit {code})",
            outcome.as_str()
        ));
        self.finish_validation(outcome, None);
    }

    fn server_launch_env(&self) -> Vec<(String, String)> {
        let mut env = self.child_env();
        env.extend(
            self.server_launch
                .extra_env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        // Isolate Ready-probe character minting away from %LOCALAPPDATA%\Purgatory
        // unless ExtraEnv (RV/load) already chose a persist root.
        if !env.iter().any(|(k, _)| k == "PURGATORY_DATA_DIR") {
            let persist = self.paths.dev_log_dir().join("hub_server_persist");
            let _ = std::fs::create_dir_all(&persist);
            env.push((
                "PURGATORY_DATA_DIR".to_string(),
                persist.to_string_lossy().replace('\\', "/"),
            ));
        }
        env
    }

    fn fail_server_or_validation(&mut self, reason: &str) {
        if self.validation_is_active() {
            self.finish_validation(
                ValidationState::OrchestrationFailed,
                Some(reason.to_string()),
            );
        } else if self.load_is_active() {
            self.finish_load(LoadState::OrchestrationFailed, Some(reason.to_string()));
        } else {
            self.clear_job();
        }
    }

    fn start_build(
        &mut self,
        cargo: PathBuf,
        packages: &[&str],
        reason: BuildReason,
        job_id: JobId,
        _now: Instant,
    ) -> bool {
        if let Some(build) = &self.build {
            self.log_hub(&format!(
                "Build already running ({})",
                build.reason.as_str()
            ));
            return false;
        }
        let mut args = vec!["build".to_string()];
        if self.paths.profile.cargo_release_flag() {
            args.push("--release".to_string());
        }
        for pkg in packages {
            args.push("-p".to_string());
            args.push((*pkg).to_string());
            if *pkg == LOAD_PACKAGE {
                args.push("--bin".to_string());
                args.push(LOAD_BIN.to_string());
            }
        }
        let spec = SpawnSpec {
            program: cargo,
            args,
            cwd: self.paths.root.clone(),
            env: self.child_env(),
            log_name: "cargo",
            ui_pump: true,
            lifetime: ProcessLifetime::Session,
        };
        match self
            .backend
            .spawn(spec, &self.incoming, &self.paths.dev_log_dir())
        {
            Ok(pid) => {
                self.build = Some(BuildSlot {
                    pid,
                    job_id,
                    reason,
                });
                self.log_hub(&format!(
                    "Building {} ({}) reason={}",
                    packages.join(","),
                    self.paths.profile.as_str(),
                    reason.as_str()
                ));
                true
            }
            Err(e) => {
                self.log_hub(&format!("Failed to start cargo: {e}"));
                false
            }
        }
    }

    fn start_server_process(&mut self, now: Instant) {
        let exe = self.paths.server_exe();
        if !exe.is_file() {
            self.set_state(
                ServerState::Failed,
                Some("server executable missing after build"),
            );
            self.fail_server_or_validation("server executable missing after build");
            return;
        }
        if self.server_alive() {
            self.set_state(ServerState::Starting, None);
            return;
        }
        let spec = SpawnSpec {
            program: exe.clone(),
            args: Vec::new(),
            cwd: self.paths.root.clone(),
            env: self.server_launch_env(),
            log_name: "server",
            ui_pump: false,
            lifetime: ProcessLifetime::Detached,
        };
        match self
            .backend
            .spawn(spec, &self.incoming, &self.paths.dev_log_dir())
        {
            Ok(pid) => {
                if !self.validation_is_active() && !self.load_is_active() {
                    let id = self.job_id().unwrap_or_else(|| self.alloc_job());
                    self.set_running(id, JobOp::Start);
                }
                self.tracked = Some(TrackedProcess {
                    pid,
                    origin: ProcessOrigin::Spawned,
                    exe_path: exe.clone(),
                });
                self.seen_alive = true;
                self.probe_prep_attempted = false;
                self.connection = CheckStatus::Unknown;
                self.connection_reason.clear();
                self.metrics_ok = false;
                self.set_state(ServerState::Starting, None);
                self.log_hub(&format!("Starting server (debug) EXE={}", exe.display()));
            }
            Err(e) => {
                self.set_state(
                    ServerState::Failed,
                    Some(&format!("server executable failed to start: {e}")),
                );
                self.fail_server_or_validation(&format!("server executable failed to start: {e}"));
            }
        }
        let _ = now;
    }

    fn start_probe(&mut self, now: Instant) -> bool {
        if let Some(pid) = self.probe_pid
            && self.backend.is_alive(pid)
        {
            return true;
        }
        let exe = self.paths.load_exe();
        if !exe.is_file() {
            return false;
        }
        let server = format!("{LISTEN_HOST}:{LISTEN_PORT}");
        let spec = SpawnSpec {
            program: exe,
            args: vec!["--probe".to_string(), "--server".to_string(), server],
            cwd: self.paths.root.clone(),
            env: self.child_env(),
            log_name: "probe",
            ui_pump: false,
            lifetime: ProcessLifetime::Session,
        };
        match self
            .backend
            .spawn(spec, &self.incoming, &self.paths.dev_log_dir())
        {
            Ok(pid) => {
                self.probe_pid = Some(pid);
                self.probe_job_id = self.job_id();
                if !self.probe_logged_start {
                    self.log_hub("Connection probe started (dev.probe)");
                    self.probe_logged_start = true;
                }
                true
            }
            Err(e) => {
                self.connection = CheckStatus::Fail;
                self.connection_reason = format!("failed to start probe: {e}");
                let _ = now;
                false
            }
        }
    }

    fn stop_probe(&mut self) {
        if let Some(pid) = self.probe_pid.take() {
            self.backend.kill_tree(pid);
        }
        self.probe_job_id = None;
    }

    fn reset_health(&mut self) {
        self.metrics_ok = false;
        self.connection = CheckStatus::Unknown;
        self.connection_reason.clear();
        self.listener = ListenerDiag::Unknown;
    }

    fn update_health(&mut self, now: Instant) {
        self.listener = self.health.listener(now);
        match self.health.poll_metrics(now) {
            Some(m) => self.metrics_ok = m.metrics_schema_version >= 1,
            None => self.metrics_ok = false,
        }
    }

    fn complete_build_if_exited(&mut self, now: Instant) {
        let Some(build) = &self.build else {
            return;
        };
        let pid = build.pid;
        let job_id = build.job_id;
        let reason = build.reason;
        let Some(code) = self.backend.try_wait(pid) else {
            return;
        };
        self.build = None;
        if self.job_id() != Some(job_id)
            && !matches!(
                reason,
                BuildReason::ProbePrep | BuildReason::OpenClient | BuildReason::Rebuild
            )
        {
            return;
        }
        if code != 0 {
            self.log_hub(&format!("Build FAILED exit={code}"));
            match reason {
                BuildReason::StartServer => {
                    self.set_state(
                        ServerState::Failed,
                        Some(&format!("server build failed (exit {code})")),
                    );
                    self.clear_job();
                }
                BuildReason::ProbePrep => {
                    self.set_state(
                        ServerState::Failed,
                        Some(&format!("probe binary build failed (exit {code})")),
                    );
                    self.clear_job();
                }
                BuildReason::ValidatePrep => {
                    self.finish_validation(
                        ValidationState::OrchestrationFailed,
                        Some(format!("runtime-val-prep failed (exit {code})")),
                    );
                }
                BuildReason::OpenClient => {
                    self.clear_job();
                    if self.paths.client_exe().is_file() {
                        self.log_hub(
                            "Client build failed; launching existing exe (Stop clients first to rebuild)",
                        );
                        self.drain_client_queue(now);
                    } else {
                        self.log_hub("Client build failed; queued clients not launched");
                    }
                }
                BuildReason::Rebuild => {
                    self.log_hub(&format!("Rebuild failed (exit {code})"));
                    self.clear_job();
                }
            }
            return;
        }
        self.log_hub("Build OK");
        match reason {
            BuildReason::StartServer => self.start_server_process(now),
            BuildReason::ProbePrep => {}
            BuildReason::ValidatePrep => self.begin_print_server_env(now),
            BuildReason::OpenClient => {
                self.clear_job();
                self.drain_client_queue(now);
            }
            BuildReason::Rebuild => {
                self.clear_job();
                self.log_hub("Rebuild finished");
            }
        }
    }

    fn format_readiness_failure(&self, reason: &str) -> String {
        let alive = if self.tracked.is_some() { "yes" } else { "no" };
        let health = if self.metrics_ok { "yes" } else { "no" };
        format!(
            "Server readiness failed: process alive: {alive}; listener: {}; metrics: {health}; QUIC probe: {}; reason: {reason}",
            self.listener.as_ui(),
            self.connection.as_ui()
        )
    }

    pub(crate) fn run_lifecycle(&mut self, now: Instant) {
        self.complete_build_if_exited(now);
        self.complete_validation_harness_if_exited();
        self.complete_load_harness_if_exited();
        let alive = self.server_alive();
        self.update_health(now);

        if self.state == ServerState::Stopping {
            if !alive {
                self.tracked = None;
                self.seen_alive = false;
                self.set_state(ServerState::Stopped, None);
                self.reset_health();
                let rv_restart = self.validation_restart_after_stop;
                self.validation_restart_after_stop = false;
                if rv_restart && self.validation_is_active() {
                    self.set_validation_phase(ValidationState::WaitingForReady);
                    self.start_server_process(now);
                    return;
                }
                let load_restart = self.load_restart_after_stop;
                self.load_restart_after_stop = false;
                if load_restart && self.load_is_active() {
                    self.set_load_phase(LoadState::WaitingForReady);
                    self.start_server_process(now);
                    return;
                }
                self.clear_job();
                let restart = self.restart_after_stop;
                self.restart_after_stop = false;
                if restart {
                    self.request_start(now);
                }
            }
            return;
        }

        if self.state.uses_live_process() && !alive {
            self.stop_probe();
            if self.seen_alive {
                self.set_state(
                    ServerState::Failed,
                    Some("server process exited unexpectedly"),
                );
            } else {
                self.log_hub("No live workspace server; not treating as unexpected exit");
                self.set_state(ServerState::Stopped, None);
                self.reset_health();
            }
            self.tracked = None;
            self.seen_alive = false;
            if self.validation_is_active() {
                self.finish_validation(
                    ValidationState::OrchestrationFailed,
                    Some("server process exited unexpectedly".to_string()),
                );
            } else if self.load_is_active() {
                self.finish_load(
                    LoadState::OrchestrationFailed,
                    Some("server process exited unexpectedly".to_string()),
                );
            } else {
                self.clear_job();
            }
            return;
        }

        if self.state == ServerState::Building {
            return;
        }

        if self.state == ServerState::Starting {
            if !self.paths.load_exe().is_file() {
                if self.build.is_none()
                    && let Some(cargo) = self.cargo_path.clone()
                {
                    let id = self.job_id().unwrap_or_else(|| self.alloc_job());
                    if !self.validation_is_active() && !self.load_is_active() {
                        self.set_running(id, JobOp::Build);
                    }
                    let _ =
                        self.start_build(cargo, &[LOAD_PACKAGE], BuildReason::ProbePrep, id, now);
                }
                return;
            }
            if self
                .build
                .as_ref()
                .is_some_and(|b| b.reason == BuildReason::ProbePrep)
            {
                return;
            }
            if !self.validation_is_active() && !self.load_is_active() {
                let id = self.job_id().unwrap_or_else(|| self.alloc_job());
                self.set_running(id, JobOp::Probe);
            }
            self.set_state(ServerState::Verifying, None);
            self.verify_started_at = Some(now);
            self.probe_logged_start = false;
            self.probe_logged_fail = false;
            self.probe_next_at = now;
            let _ = self.start_probe(now);
            return;
        }

        if self.state == ServerState::Verifying {
            let elapsed = self
                .verify_started_at
                .map(|t| now.saturating_duration_since(t))
                .unwrap_or_default();
            if elapsed > READY_TIMEOUT {
                self.stop_probe();
                self.connection = CheckStatus::Fail;
                self.connection_reason = "timed out".to_string();
                let msg = self.format_readiness_failure(&format!(
                    "readiness timed out after {}s",
                    READY_TIMEOUT.as_secs()
                ));
                self.set_state(ServerState::Failed, Some(&msg));
                if self.validation_is_active() {
                    self.finish_validation(ValidationState::OrchestrationFailed, Some(msg.clone()));
                } else {
                    self.clear_job();
                }
                return;
            }
            if self.probe_pid.is_none() {
                if now < self.probe_next_at {
                    return;
                }
                let _ = self.start_probe(now);
                return;
            }
            let pid = self.probe_pid.unwrap();
            let Some(code) = self.backend.try_wait(pid) else {
                return;
            };
            let job_ok = self.probe_job_id == self.job_id();
            self.probe_pid = None;
            if !job_ok {
                return;
            }
            if code == 0 {
                self.connection = CheckStatus::Pass;
                self.connection_reason.clear();
                self.set_state(ServerState::Ready, None);
                let keep_job =
                    self.maybe_spawn_validation_harness() || self.maybe_spawn_load_harness();
                if !keep_job {
                    self.clear_job();
                }
            } else if code == 2 && !self.probe_prep_attempted {
                self.probe_prep_attempted = true;
                self.log_hub(
                    "Probe binary rejected --probe (exit 2); rebuilding purgatory-bot-client",
                );
                self.set_state(ServerState::Starting, None);
                if let Some(cargo) = self.cargo_path.clone() {
                    let id = self.job_id().unwrap_or_else(|| self.alloc_job());
                    if !self.validation_is_active() && !self.load_is_active() {
                        self.set_running(id, JobOp::Build);
                    }
                    let _ =
                        self.start_build(cargo, &[LOAD_PACKAGE], BuildReason::ProbePrep, id, now);
                }
            } else {
                let detail = if self.connection_reason.is_empty() {
                    format!("probe exit {code}")
                } else {
                    self.connection_reason.clone()
                };
                if !self.probe_logged_fail {
                    self.log_hub(&format!(
                        "Probe unsuccessful ({detail}); retrying until Ready timeout"
                    ));
                    self.probe_logged_fail = true;
                }
                self.connection = CheckStatus::Fail;
                self.connection_reason = detail;
                self.probe_next_at = now + PROBE_RETRY;
            }
            return;
        }

        if self.state == ServerState::Ready {
            self.drain_client_queue(now);
            if self
                .load
                .as_ref()
                .is_some_and(|j| j.phase == LoadState::WaitingForReady && j.harness_pid.is_none())
            {
                let _ = self.maybe_spawn_load_harness();
            }
            if !self.metrics_ok && self.connection == CheckStatus::Pass {
                self.set_state(
                    ServerState::Degraded,
                    Some("metrics health lost after Ready"),
                );
            }
            return;
        }

        if self.state == ServerState::Degraded && self.metrics_ok {
            self.set_state(ServerState::Ready, None);
        }

        self.recovery_scan(now);
    }

    fn startup_recovery(&mut self, now: Instant) {
        let mut servers = self
            .backend
            .discover_workspace(SERVER_STEM, &self.paths.target_prefix());
        if servers.len() > 1 {
            let keep = servers.remove(0);
            for extra in servers {
                self.log_hub(&format!(
                    "Stopping extra workspace server pid {}",
                    extra.pid
                ));
                self.backend.kill_tree(extra.pid);
            }
            servers = vec![keep];
        }
        if servers.len() == 1 && !self.server_alive() {
            self.adopt_server(servers[0].clone(), now);
        }
    }

    fn recovery_scan(&mut self, now: Instant) {
        if let Some(last) = self.last_recovery
            && now.saturating_duration_since(last) < RECOVERY_INTERVAL
        {
            return;
        }
        self.last_recovery = Some(now);
        if self.state != ServerState::Stopped && self.state != ServerState::Failed {
            return;
        }
        if self.server_alive() {
            return;
        }
        let servers = self
            .backend
            .discover_workspace(SERVER_STEM, &self.paths.target_prefix());
        if servers.len() == 1 {
            self.adopt_server(servers[0].clone(), now);
        }
    }

    fn adopt_server(&mut self, found: crate::process::DiscoveredProcess, _now: Instant) {
        self.tracked = Some(TrackedProcess {
            pid: found.pid,
            origin: ProcessOrigin::Adopted,
            exe_path: found.exe_path.clone(),
        });
        // Do not set seen_alive here. A scan hit can vanish or be a reused PID
        // before the first lifecycle tick; that is Stopped, not unexpected exit.
        self.connection = CheckStatus::Unknown;
        let id = self.alloc_job();
        self.set_running(id, JobOp::Start);
        self.set_state(ServerState::Starting, None);
        self.log_hub(&format!(
            "Adopted workspace server pid {} ({}); verifying (process exists is not Ready)",
            found.pid,
            found.exe_path.display()
        ));
    }

    pub(crate) fn shutdown_session_jobs(&mut self) {
        if let Some(pid) = self.validation.as_ref().and_then(|v| v.harness_pid) {
            self.backend.kill_tree(pid);
            if let Some(job) = &mut self.validation {
                job.harness_pid = None;
            }
        }
        if let Some(pid) = self.load.as_ref().and_then(|v| v.harness_pid) {
            self.backend.kill_tree(pid);
            if let Some(job) = &mut self.load {
                job.harness_pid = None;
            }
        }
        if let Some(pid) = self.probe_pid.take() {
            self.backend.kill_tree(pid);
        }
        if let Some(build) = self.build.take() {
            self.backend.kill_tree(build.pid);
        }
        for c in &self.clients {
            self.backend.detach(c.pid);
        }
        if let Some(tracked) = &self.tracked {
            self.backend.detach(tracked.pid);
        }
    }
}

impl<B: ProcessBackend, H: HealthSource> Drop for HubSession<B, H> {
    fn drop(&mut self) {
        self.shutdown_session_jobs();
    }
}

fn exe_paths_match(a: &std::path::Path, b: &std::path::Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

fn open_explorer(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

fn exe_appears_locked(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }
    std::fs::OpenOptions::new().append(true).open(path).is_err()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::FakeProcessBackend;
    use crate::config::ACTIVITY_LOG_CAP;
    use crate::health::FakeHealthSource;
    use crate::paths::exe_name;
    use crate::process::{CheckStatus, DiscoveredProcess, ProcessLifetime};
    use std::fs;
    use std::time::Duration;

    fn test_root(tag: &str) -> WorkspacePaths {
        let dir =
            std::env::temp_dir().join(format!("purgatory-dev-hub-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("target").join("debug")).unwrap();
        fs::write(dir.join("PHASE"), "6G\n").unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[workspace.package]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(dir.join("logs").join("dev-tools")).unwrap();
        fs::write(
            dir.join("target")
                .join("debug")
                .join(exe_name("purgatory-server")),
            b"",
        )
        .unwrap();
        fs::write(
            dir.join("target")
                .join("debug")
                .join(exe_name("purgatory-load")),
            b"",
        )
        .unwrap();
        fs::write(
            dir.join("target")
                .join("debug")
                .join(exe_name("purgatory-client")),
            b"",
        )
        .unwrap();
        WorkspacePaths::from_root(dir).unwrap()
    }

    fn harness(
        tag: &str,
        backend: FakeProcessBackend,
        health: FakeHealthSource,
    ) -> (HubSession<FakeProcessBackend, FakeHealthSource>, Instant) {
        let now = Instant::now();
        let paths = test_root(tag);
        let session =
            HubSession::new(paths, backend, health, Some(PathBuf::from("cargo")), now).unwrap();
        (session, now)
    }

    fn drive_to_ready(
        session: &mut HubSession<FakeProcessBackend, FakeHealthSource>,
        mut now: Instant,
    ) -> Instant {
        session.command(HubCommand::Start, now);
        now += LIFECYCLE_FAST;
        session.tick(now);
        now += LIFECYCLE_FAST;
        session.tick(now);
        now += LIFECYCLE_FAST;
        session.tick(now);
        now
    }

    #[test]
    fn process_exists_is_not_ready() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = None;
        let (mut session, now) = harness("alive-not-ready", backend, FakeHealthSource::healthy());
        drive_to_ready(&mut session, now);
        assert!(session.server_alive());
        assert_ne!(session.state, ServerState::Ready);
        assert!(matches!(
            session.state,
            ServerState::Starting | ServerState::Verifying
        ));
    }

    #[test]
    fn ready_requires_probe_not_health() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("ready-probe", backend, FakeHealthSource::none());
        drive_to_ready(&mut session, now);
        assert_eq!(session.state, ServerState::Ready);
        assert!(!session.metrics_ok);
        assert_eq!(
            session.tracked.as_ref().unwrap().origin,
            ProcessOrigin::Spawned
        );
    }

    #[test]
    fn listener_unknown_does_not_block_ready() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let mut health = FakeHealthSource::healthy();
        health.listener = ListenerDiag::Unknown;
        let (mut session, now) = harness("listener", backend, health);
        drive_to_ready(&mut session, now);
        assert_eq!(session.state, ServerState::Ready);
        assert_eq!(session.listener, ListenerDiag::Unknown);
    }

    #[test]
    fn overlapping_start_is_ignored() {
        let mut backend = FakeProcessBackend::new();
        backend.hold_cargo = true;
        let (mut session, now) = harness("overlap", backend, FakeHealthSource::none());
        assert_eq!(
            session.command(HubCommand::Start, now),
            CommandOutcome::Accepted
        );
        assert_eq!(
            session.command(HubCommand::Start, now),
            CommandOutcome::Ignored
        );
        assert_eq!(session.backend.spawn_log.len(), 1);
    }

    #[test]
    fn stop_supersedes_build() {
        let mut backend = FakeProcessBackend::new();
        backend.hold_cargo = true;
        let (mut session, now) = harness("supersede", backend, FakeHealthSource::none());
        session.command(HubCommand::Start, now);
        assert_eq!(session.state, ServerState::Building);
        session.command(HubCommand::Stop, now);
        assert!(!session.backend.kill_log.is_empty());
        assert!(session.job.is_stop() || session.state == ServerState::Stopping);
    }

    #[test]
    fn late_probe_after_cancel_is_ignored() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = None;
        let (mut session, now) = harness("late-probe", backend, FakeHealthSource::none());
        let now = drive_to_ready(&mut session, now);
        assert_eq!(session.state, ServerState::Verifying);
        session.probe_job_id = Some(JobId(999));
        if let Some(pid) = session.probe_pid
            && let Some(proc) = session.backend.alive.get_mut(&pid)
        {
            proc.pending_exit = Some(0);
        }
        session.run_lifecycle(now + LIFECYCLE_FAST);
        assert_ne!(session.state, ServerState::Ready);
    }

    #[test]
    fn adopted_origin_is_not_spawned() {
        let mut backend = FakeProcessBackend::new();
        backend.alive.insert(
            50,
            crate::backend::FakeProc {
                kind: crate::backend::FakeKind::Server,
                pending_exit: None,
            },
        );
        backend.discovered = vec![DiscoveredProcess {
            pid: 50,
            exe_path: PathBuf::from("target/debug/purgatory-server.exe"),
        }];
        let (session, _) = harness("adopt", backend, FakeHealthSource::none());
        let t = session.tracked.clone().expect("adopted");
        assert_eq!(t.pid, 50);
        assert_eq!(t.origin, ProcessOrigin::Adopted);
        assert_eq!(session.state, ServerState::Starting);
    }

    #[test]
    fn discovery_without_adopt_is_not_tracked() {
        let (mut session, _) = harness(
            "discover-only",
            FakeProcessBackend::new(),
            FakeHealthSource::none(),
        );
        session.backend.discovered = vec![DiscoveredProcess {
            pid: 77,
            exe_path: PathBuf::from("x"),
        }];
        assert!(session.tracked.is_none());
        assert_eq!(session.state, ServerState::Stopped);
    }

    #[test]
    fn probe_prep_stays_starting() {
        let paths = test_root("probe-prep");
        let _ = fs::remove_file(paths.load_exe());
        let mut backend = FakeProcessBackend::new();
        backend.hold_cargo = true;
        let now = Instant::now();
        let mut session = HubSession::new(
            paths,
            backend,
            FakeHealthSource::none(),
            Some(PathBuf::from("cargo")),
            now,
        )
        .unwrap();
        session.command(HubCommand::Start, now);
        session.backend.hold_cargo = false;
        session.backend.cargo_exit = 0;
        if let Some(pid) = session.build.as_ref().map(|b| b.pid)
            && let Some(p) = session.backend.alive.get_mut(&pid)
        {
            p.pending_exit = Some(0);
        }
        session.run_lifecycle(now + LIFECYCLE_FAST);
        assert_eq!(session.state, ServerState::Starting);
        assert!(
            session
                .build
                .as_ref()
                .is_some_and(|b| b.reason == BuildReason::ProbePrep)
                || session
                    .backend
                    .spawn_log
                    .iter()
                    .any(|s| s.contains("purgatory-bot-client"))
        );
        assert_ne!(session.state, ServerState::Verifying);
    }

    #[test]
    fn readiness_timeout_fails_but_leaves_process() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = None;
        let (mut session, now) = harness("timeout", backend, FakeHealthSource::none());
        let now = drive_to_ready(&mut session, now);
        session.run_lifecycle(now + READY_TIMEOUT + Duration::from_secs(1));
        assert_eq!(session.state, ServerState::Failed);
        assert!(session.tracked.is_some());
        assert!(session.server_alive());
    }

    #[test]
    fn degraded_after_ready_when_metrics_drop() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("degraded", backend, FakeHealthSource::healthy());
        drive_to_ready(&mut session, now);
        assert_eq!(session.state, ServerState::Ready);
        session.health.metrics = None;
        session.run_lifecycle(now + LIFECYCLE_IDLE);
        assert_eq!(session.state, ServerState::Degraded);
    }

    #[test]
    fn activity_log_does_not_grow_without_bound() {
        let (mut session, _) = harness(
            "logcap",
            FakeProcessBackend::new(),
            FakeHealthSource::none(),
        );
        for i in 0..(ACTIVITY_LOG_CAP + 80) {
            session.log_hub(&format!("noise {i}"));
        }
        assert_eq!(session.activity.len(), ACTIVITY_LOG_CAP);
        assert_eq!(
            session.activity.view_lines().len(),
            crate::config::ACTIVITY_VIEW_LINES
        );
    }

    #[test]
    fn adopt_does_not_claim_ready() {
        let mut backend = FakeProcessBackend::new();
        backend.discovered = vec![DiscoveredProcess {
            pid: 50,
            exe_path: PathBuf::from("target/debug/purgatory-server.exe"),
        }];
        let (mut session, now) = harness("adopt-not-ready", backend, FakeHealthSource::none());
        assert_eq!(session.state, ServerState::Starting);
        assert_eq!(
            session.tracked.as_ref().unwrap().origin,
            ProcessOrigin::Adopted
        );
        session.tick(now + LIFECYCLE_FAST);
        assert_ne!(session.state, ServerState::Ready);
        assert!(
            session
                .activity
                .view_lines()
                .iter()
                .any(|l| l.contains("verifying") || l.contains("Adopted"))
        );
    }

    #[test]
    fn empty_recovery_is_stopped_not_unexpected_exit() {
        let (mut session, now) = harness(
            "empty-recovery",
            FakeProcessBackend::new(),
            FakeHealthSource::none(),
        );
        session.tick(now + LIFECYCLE_IDLE);
        assert_eq!(session.state, ServerState::Stopped);
        assert!(session.tracked.is_none());
        assert!(session.last_failure.is_none() || session.state != ServerState::Failed);
        let logs = session.activity.view_lines().join("\n");
        assert!(
            !logs.contains("exited unexpectedly"),
            "unexpected-exit must not fire on a fresh session with no server"
        );
    }

    #[test]
    fn unexpected_exit_only_after_seen_alive() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("seen-alive-exit", backend, FakeHealthSource::none());
        drive_to_ready(&mut session, now);
        assert_eq!(session.state, ServerState::Ready);
        let pid = session.tracked.as_ref().unwrap().pid;
        session.backend.alive.remove(&pid);
        session.run_lifecycle(now + LIFECYCLE_IDLE);
        assert_eq!(session.state, ServerState::Failed);
        assert!(
            session
                .last_failure
                .as_deref()
                .is_some_and(|s| s.contains("exited unexpectedly"))
        );
    }

    #[test]
    fn drop_does_not_kill_detached_server() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = None;
        let (mut session, now) = harness("drop-detach", backend, FakeHealthSource::none());
        drive_to_ready(&mut session, now);
        let pid = session.tracked.as_ref().expect("spawned").pid;
        session.shutdown_session_jobs();
        assert!(
            !session.backend.kill_log.contains(&pid),
            "Hub shutdown must not kill the dedicated server"
        );
        assert!(
            session.backend.detach_log.contains(&pid),
            "Hub shutdown must detach the dedicated server wait-handle"
        );
    }

    #[test]
    fn vanished_adopt_is_stopped_not_unexpected_exit() {
        let mut backend = FakeProcessBackend::new();
        backend.discovered = vec![DiscoveredProcess {
            pid: 50,
            exe_path: PathBuf::from("target/debug/purgatory-server.exe"),
        }];
        let (mut session, now) = harness("vanished-adopt", backend, FakeHealthSource::none());
        assert_eq!(session.state, ServerState::Starting);
        session.backend.discovered.clear();
        session.tick(now + LIFECYCLE_FAST);
        assert_eq!(session.state, ServerState::Stopped);
        assert!(session.tracked.is_none());
        let logs = session.activity.view_lines().join("\n");
        assert!(
            !logs.contains("exited unexpectedly"),
            "a scan hit that is gone before this session observed it live is not unexpected exit"
        );
    }

    #[test]
    fn adopted_pid_requires_matching_exe_path() {
        let mut backend = FakeProcessBackend::new();
        backend.discovered = vec![DiscoveredProcess {
            pid: 50,
            exe_path: PathBuf::from("target/debug/purgatory-server.exe"),
        }];
        let (mut session, now) = harness("pid-reuse", backend, FakeHealthSource::none());
        session.tick(now + LIFECYCLE_FAST);
        assert!(session.seen_alive);
        assert_ne!(session.state, ServerState::Stopped);
        session.backend.discovered = vec![DiscoveredProcess {
            pid: 50,
            exe_path: PathBuf::from("C:/Windows/System32/notepad.exe"),
        }];
        session.run_lifecycle(now + LIFECYCLE_IDLE);
        assert_eq!(session.state, ServerState::Failed);
        assert!(
            session
                .last_failure
                .as_deref()
                .is_some_and(|s| s.contains("exited unexpectedly"))
        );
    }

    #[test]
    fn adopt_then_verify_is_not_immediately_ready() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = None;
        backend.discovered = vec![DiscoveredProcess {
            pid: 50,
            exe_path: PathBuf::from("target/debug/purgatory-server.exe"),
        }];
        let (mut session, now) = harness("adopt-verify", backend, FakeHealthSource::none());
        assert_eq!(session.state, ServerState::Starting);
        session.tick(now + LIFECYCLE_FAST);
        assert_eq!(session.state, ServerState::Verifying);
        assert_eq!(session.connection, CheckStatus::Unknown);
        assert_ne!(session.state, ServerState::Ready);
        if let Some(pid) = session.probe_pid
            && let Some(proc) = session.backend.alive.get_mut(&pid)
        {
            proc.pending_exit = Some(0);
        }
        session.run_lifecycle(now + LIFECYCLE_FAST + LIFECYCLE_FAST);
        assert_eq!(session.state, ServerState::Ready);
        assert_eq!(
            session.tracked.as_ref().unwrap().origin,
            ProcessOrigin::Adopted
        );
        assert_eq!(session.tracked.as_ref().unwrap().pid, 50);
    }

    #[test]
    fn spawned_server_defaults_isolated_persist_dir() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("persist-isolate", backend, FakeHealthSource::none());
        drive_to_ready(&mut session, now);
        let persist = session
            .backend
            .extra_env_log
            .iter()
            .find(|(k, _)| k == "PURGATORY_DATA_DIR")
            .map(|(_, v)| v.as_str());
        assert!(
            persist.is_some_and(|v| v.contains("hub_server_persist")),
            "expected hub_server_persist, got {persist:?}; log={:?}",
            session.backend.extra_env_log
        );
    }

    #[test]
    fn spawned_server_uses_detached_lifetime() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = None;
        let (mut session, now) = harness("lifetime", backend, FakeHealthSource::none());
        drive_to_ready(&mut session, now);
        assert!(
            session
                .backend
                .lifetimes
                .contains(&ProcessLifetime::Detached)
        );
        assert!(
            session
                .backend
                .lifetimes
                .contains(&ProcessLifetime::Session)
        );
    }

    fn drive_ticks(
        session: &mut HubSession<FakeProcessBackend, FakeHealthSource>,
        mut now: Instant,
        n: u32,
    ) -> Instant {
        for _ in 0..n {
            now += LIFECYCLE_FAST;
            session.tick(now);
        }
        now
    }

    #[test]
    fn start_validation_ignored_unless_ready() {
        let (mut session, now) = harness(
            "rv-not-ready",
            FakeProcessBackend::new(),
            FakeHealthSource::none(),
        );
        assert_eq!(
            session.command(
                HubCommand::StartValidation {
                    spec: ValidationSpec::smoke(),
                },
                now
            ),
            CommandOutcome::Ignored
        );
        assert_eq!(session.validation_phase(), ValidationState::Idle);
    }

    #[test]
    fn runtime_validation_happy_path_passes() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        backend.harness_exit = Some(0);
        let (mut session, now) = harness("rv-happy", backend, FakeHealthSource::healthy());
        let now = drive_to_ready(&mut session, now);
        assert_eq!(session.state, ServerState::Ready);
        assert_eq!(
            session.command(
                HubCommand::StartValidation {
                    spec: ValidationSpec::smoke(),
                },
                now
            ),
            CommandOutcome::Accepted
        );
        drive_ticks(&mut session, now, 8);
        assert_eq!(session.validation_phase(), ValidationState::Passed);
        assert_eq!(session.state, ServerState::Ready);
        assert!(
            session
                .backend
                .extra_env_log
                .iter()
                .any(|(k, v)| k == "PURGATORY_ADMISSION_CAP" && v == "256"),
            "RV restart must launch the server with load-mode ExtraEnv"
        );
        assert!(
            session
                .backend
                .spawn_log
                .iter()
                .any(|s| s.contains("--preset") && s.contains("smoke")),
            "harness argv must include the preset"
        );
        assert!(
            session
                .backend
                .spawn_log
                .iter()
                .any(|s| s.contains("print-server-env") || s.contains("--print-server-env")),
        );
    }

    #[test]
    fn duplicate_start_validation_is_ignored() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("rv-dup", backend, FakeHealthSource::none());
        let now = drive_to_ready(&mut session, now);
        session.backend.hold_cargo = true;
        assert_eq!(
            session.command(
                HubCommand::StartValidation {
                    spec: ValidationSpec::smoke(),
                },
                now
            ),
            CommandOutcome::Accepted
        );
        assert_eq!(
            session.command(
                HubCommand::StartValidation {
                    spec: ValidationSpec::smoke(),
                },
                now
            ),
            CommandOutcome::Ignored
        );
    }

    #[test]
    fn discovered_load_refuses_start_validation() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("rv-load-running", backend, FakeHealthSource::none());
        let now = drive_to_ready(&mut session, now);
        session.backend.discovered = vec![DiscoveredProcess {
            pid: 9,
            exe_path: PathBuf::from("target/debug/purgatory-load.exe"),
        }];
        assert_eq!(
            session.command(
                HubCommand::StartValidation {
                    spec: ValidationSpec::smoke(),
                },
                now
            ),
            CommandOutcome::Ignored
        );
        assert_eq!(session.validation_phase(), ValidationState::Idle);
    }

    #[test]
    fn cancel_during_validate_prep_does_not_kill_server() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("rv-cancel-build", backend, FakeHealthSource::none());
        let now = drive_to_ready(&mut session, now);
        session.backend.hold_cargo = true;
        session.command(
            HubCommand::StartValidation {
                spec: ValidationSpec::smoke(),
            },
            now,
        );
        let server_pid = session.tracked.as_ref().unwrap().pid;
        session.command(HubCommand::StopValidation, now);
        assert_eq!(session.validation_phase(), ValidationState::Cancelled);
        assert_eq!(session.state, ServerState::Ready);
        assert!(
            !session.backend.kill_log.contains(&server_pid),
            "cancel must not kill the detached server"
        );
    }

    #[test]
    fn cancel_during_harness_kills_harness_not_server() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        backend.harness_exit = None;
        let (mut session, now) = harness("rv-cancel-harness", backend, FakeHealthSource::healthy());
        let now = drive_to_ready(&mut session, now);
        session.command(
            HubCommand::StartValidation {
                spec: ValidationSpec::smoke(),
            },
            now,
        );
        drive_ticks(&mut session, now, 8);
        assert_eq!(session.validation_phase(), ValidationState::Running);
        let server_pid = session.tracked.as_ref().unwrap().pid;
        let harness_pid = session.validation.as_ref().unwrap().harness_pid.unwrap();
        session.command(HubCommand::StopValidation, Instant::now());
        assert_eq!(session.validation_phase(), ValidationState::Cancelled);
        assert!(session.backend.kill_log.contains(&harness_pid));
        assert!(
            !session.backend.kill_log.contains(&server_pid),
            "cancel must not kill the current detached server"
        );
        assert!(session.server_alive());
    }

    #[test]
    fn server_stop_clears_runtime_validation() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("rv-stop-clears", backend, FakeHealthSource::none());
        let now = drive_to_ready(&mut session, now);
        session.backend.hold_cargo = true;
        session.command(
            HubCommand::StartValidation {
                spec: ValidationSpec::smoke(),
            },
            now,
        );
        session.command(HubCommand::Stop, now);
        assert!(!session.validation_is_active());
        assert_eq!(session.validation_phase(), ValidationState::Cancelled);
    }

    #[test]
    fn drop_kills_harness_not_detached_server() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        backend.harness_exit = None;
        let (mut session, now) = harness("rv-drop", backend, FakeHealthSource::healthy());
        let now = drive_to_ready(&mut session, now);
        session.command(
            HubCommand::StartValidation {
                spec: ValidationSpec::smoke(),
            },
            now,
        );
        drive_ticks(&mut session, now, 8);
        let server_pid = session.tracked.as_ref().unwrap().pid;
        let harness_pid = session.validation.as_ref().unwrap().harness_pid.unwrap();
        session.shutdown_session_jobs();
        assert!(session.backend.kill_log.contains(&harness_pid));
        assert!(
            !session.backend.kill_log.contains(&server_pid),
            "Hub drop must not kill the dedicated server"
        );
        assert!(session.backend.detach_log.contains(&server_pid));
    }

    #[test]
    fn stale_print_server_env_is_orchestration_failed() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        backend.print_env_exit = 2;
        backend.print_env_stderr = "error: unexpected argument '--preset' found".to_string();
        let (mut session, now) = harness("rv-stale", backend, FakeHealthSource::healthy());
        let now = drive_to_ready(&mut session, now);
        session.command(
            HubCommand::StartValidation {
                spec: ValidationSpec::smoke(),
            },
            now,
        );
        drive_ticks(&mut session, now, 4);
        assert_eq!(
            session.validation_phase(),
            ValidationState::OrchestrationFailed
        );
        assert_eq!(session.state, ServerState::Ready);
    }

    #[test]
    fn second_hub_session_on_same_workspace_fails() {
        let paths = test_root("rv-lock");
        let now = Instant::now();
        let first = HubSession::new(
            paths.clone(),
            FakeProcessBackend::new(),
            FakeHealthSource::none(),
            Some(PathBuf::from("cargo")),
            now,
        )
        .expect("first session");
        let second = HubSession::new(
            paths,
            FakeProcessBackend::new(),
            FakeHealthSource::none(),
            Some(PathBuf::from("cargo")),
            now,
        );
        assert!(second.is_err(), "second Hub must be refused");
        drop(first);
    }

    #[test]
    fn live_status_malformed_is_not_failure() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        backend.harness_exit = None;
        let (mut session, now) = harness("rv-live", backend, FakeHealthSource::healthy());
        let now = drive_to_ready(&mut session, now);
        session.command(
            HubCommand::StartValidation {
                spec: ValidationSpec::smoke(),
            },
            now,
        );
        drive_ticks(&mut session, now, 8);
        assert_eq!(session.validation_phase(), ValidationState::Running);
        let load_root = session.paths.root.join("logs").join("load");
        fs::create_dir_all(load_root.join("run_x")).unwrap();
        fs::write(load_root.join("current_run.txt"), "run_x\n").unwrap();
        fs::write(
            load_root.join("run_x").join("live_status.json"),
            "{not json",
        )
        .unwrap();
        let snap = session.snapshot(now);
        assert!(!snap.validation_live.available);
        assert_eq!(snap.validation, ValidationState::Running);
    }

    #[test]
    fn server_log_tail_reads_file() {
        let (mut session, now) = harness(
            "server-log",
            FakeProcessBackend::new(),
            FakeHealthSource::none(),
        );
        let path = session.paths.dev_log_dir().join("server.log");
        fs::write(&path, "handshake accepted connection_id=7\n").unwrap();
        session.tick(now + LIFECYCLE_IDLE);
        let snap = session.snapshot(now);
        assert!(
            snap.server_log_lines
                .iter()
                .any(|l| l.contains("handshake accepted") && l.contains("server |")),
            "server page must show tailed server.log lines"
        );
        assert!(
            snap.server_log_lines.iter().any(|l| {
                l.as_bytes().get(2) == Some(&b':') && l.as_bytes().get(5) == Some(&b':')
            }),
            "tailed lines must include HH:MM:SS"
        );
    }

    #[test]
    fn missing_server_log_is_empty_not_failure() {
        let (mut session, now) = harness(
            "server-log-missing",
            FakeProcessBackend::new(),
            FakeHealthSource::none(),
        );
        let _ = fs::remove_file(session.paths.dev_log_dir().join("server.log"));
        session.tick(now + LIFECYCLE_IDLE);
        let snap = session.snapshot(now);
        assert!(snap.server_log_lines.is_empty());
        assert_eq!(snap.server_state, ServerState::Stopped);
    }

    #[test]
    fn load_compatible_ready_spawns_harness() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        backend.harness_exit = Some(0);
        let (mut session, now) = harness("load-compat", backend, FakeHealthSource::healthy());
        let now = drive_to_ready(&mut session, now);
        assert_eq!(
            session.command(
                HubCommand::StartLoad {
                    spec: LoadSpec {
                        count: 2,
                        duration: crate::load::LoadDuration::OneMinute,
                        ..LoadSpec::default()
                    },
                },
                now
            ),
            CommandOutcome::Accepted
        );
        drive_ticks(&mut session, now, 4);
        assert_eq!(session.load_phase(), LoadState::Passed);
        assert!(
            session
                .backend
                .spawn_log
                .iter()
                .any(|s| s.contains("--count") && s.contains("2"))
        );
    }

    #[test]
    fn load_incompatible_restarts_with_extra_env() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        backend.harness_exit = Some(0);
        let (mut session, now) = harness(
            "load-restart",
            backend,
            FakeHealthSource::load_incompatible(),
        );
        let now = drive_to_ready(&mut session, now);
        // After Ready, make metrics incompatible for the StartLoad gate.
        session.health = FakeHealthSource::load_incompatible();
        assert_eq!(
            session.command(
                HubCommand::StartLoad {
                    spec: LoadSpec {
                        count: 10,
                        ..LoadSpec::default()
                    },
                },
                now
            ),
            CommandOutcome::Accepted
        );
        drive_ticks(&mut session, now, 10);
        assert_eq!(session.load_phase(), LoadState::Passed);
        assert!(
            session
                .backend
                .extra_env_log
                .iter()
                .any(|(k, v)| k == "PURGATORY_ADMISSION_CAP" && v == "256")
        );
    }

    #[test]
    fn load_refused_while_validation_active() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("load-vs-rv", backend, FakeHealthSource::healthy());
        let now = drive_to_ready(&mut session, now);
        session.backend.hold_cargo = true;
        assert_eq!(
            session.command(
                HubCommand::StartValidation {
                    spec: ValidationSpec::smoke(),
                },
                now
            ),
            CommandOutcome::Accepted
        );
        assert!(session.validation_is_active());
        assert_eq!(
            session.command(
                HubCommand::StartLoad {
                    spec: LoadSpec::default(),
                },
                now
            ),
            CommandOutcome::Ignored
        );
    }

    #[test]
    fn stop_load_does_not_kill_server() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        backend.harness_exit = None;
        let (mut session, now) = harness("load-stop", backend, FakeHealthSource::healthy());
        let now = drive_to_ready(&mut session, now);
        session.command(
            HubCommand::StartLoad {
                spec: LoadSpec {
                    count: 2,
                    ..LoadSpec::default()
                },
            },
            now,
        );
        drive_ticks(&mut session, now, 2);
        let server_pid = session.tracked.as_ref().unwrap().pid;
        let harness_pid = session.load.as_ref().unwrap().harness_pid.unwrap();
        session.command(HubCommand::StopLoad, Instant::now());
        assert_eq!(session.load_phase(), LoadState::Cancelled);
        assert!(session.backend.kill_log.contains(&harness_pid));
        assert!(!session.backend.kill_log.contains(&server_pid));
    }

    #[test]
    fn clients_queue_until_ready_then_launch() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("clients-queue", backend, FakeHealthSource::healthy());
        assert_eq!(
            session.command(HubCommand::RequestClients { count: 1 }, now),
            CommandOutcome::Accepted
        );
        assert_eq!(session.pending_clients, 1);
        assert!(session.clients.is_empty());
        let now = drive_to_ready(&mut session, now);
        drive_ticks(&mut session, now, 4);
        assert_eq!(session.pending_clients, 0);
        assert_eq!(session.clients.len(), 1);
        assert!(
            session
                .backend
                .lifetimes
                .contains(&ProcessLifetime::Detached)
        );
    }

    #[test]
    fn open_client_build_does_not_launch_until_cargo_exits() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("client-build-wait", backend, FakeHealthSource::healthy());
        let now = drive_to_ready(&mut session, now);
        assert_eq!(session.state, ServerState::Ready);
        session.backend.hold_cargo = true;
        assert_eq!(
            session.command(HubCommand::RequestClients { count: 1 }, now),
            CommandOutcome::Accepted
        );
        assert!(session.build.is_some());
        assert_eq!(
            session.build.as_ref().unwrap().reason,
            BuildReason::OpenClient
        );
        drive_ticks(&mut session, now, 6);
        assert!(
            session.clients.is_empty(),
            "must not launch while open-client cargo holds the exe"
        );
        assert_eq!(session.pending_clients, 1);
        let cargo_pid = session.build.as_ref().unwrap().pid;
        if let Some(proc) = session.backend.alive.get_mut(&cargo_pid) {
            proc.pending_exit = Some(0);
        }
        drive_ticks(&mut session, now + Duration::from_millis(500), 6);
        assert_eq!(session.pending_clients, 0);
        assert_eq!(session.clients.len(), 1);
    }

    #[test]
    fn drop_does_not_kill_detached_clients() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("client-drop", backend, FakeHealthSource::healthy());
        let now = drive_to_ready(&mut session, now);
        session.command(HubCommand::RequestClients { count: 1 }, now);
        drive_ticks(&mut session, now, 4);
        let client_pid = session.clients[0].pid;
        session.shutdown_session_jobs();
        assert!(!session.backend.kill_log.contains(&client_pid));
        assert!(session.backend.detach_log.contains(&client_pid));
    }

    #[test]
    fn rebuild_skips_running_server() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("rebuild-skip", backend, FakeHealthSource::healthy());
        let now = drive_to_ready(&mut session, now);
        session.backend.discovered = vec![DiscoveredProcess {
            pid: session.tracked.as_ref().unwrap().pid,
            exe_path: session.paths.server_exe(),
        }];
        session.backend.hold_cargo = true;
        assert_eq!(
            session.command(HubCommand::Rebuild, now),
            CommandOutcome::Accepted
        );
        assert!(
            session
                .backend
                .spawn_log
                .iter()
                .any(|s| s.contains("purgatory-client"))
        );
        assert!(
            !session
                .backend
                .spawn_log
                .last()
                .unwrap()
                .contains("purgatory-server")
        );
    }

    #[test]
    fn kill_all_stops_server_and_clients() {
        let mut backend = FakeProcessBackend::new();
        backend.probe_exit = Some(0);
        let (mut session, now) = harness("kill-all", backend, FakeHealthSource::healthy());
        let now = drive_to_ready(&mut session, now);
        session.command(HubCommand::RequestClients { count: 1 }, now);
        drive_ticks(&mut session, now, 4);
        let server_pid = session.tracked.as_ref().unwrap().pid;
        let client_pid = session.clients[0].pid;
        session.command(HubCommand::KillAll, Instant::now());
        assert_eq!(session.state, ServerState::Stopped);
        assert!(session.backend.kill_log.contains(&server_pid));
        assert!(session.backend.kill_log.contains(&client_pid));
    }

    #[test]
    fn quality_gate_spawns_visible() {
        let (mut session, now) = harness("qg", FakeProcessBackend::new(), FakeHealthSource::none());
        // Create fake check.ps1
        let scripts = session.paths.root.join("scripts");
        fs::create_dir_all(&scripts).unwrap();
        fs::write(scripts.join("check.ps1"), "echo ok\n").unwrap();
        assert_eq!(
            session.command(HubCommand::QualityGate, now),
            CommandOutcome::Accepted
        );
        assert!(
            session
                .backend
                .spawn_log
                .iter()
                .any(|s| s.contains("visible") && s.contains("check.ps1"))
        );
    }
}
