//! Runtime Validation contracts and argv/env helpers.
//!
//! CLI pass/fail stays in `purgatory-load`. This module does not classify scenarios.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{LISTEN_HOST, LISTEN_PORT, LOAD_ADMISSION_CAP, METRICS_PORT};
use crate::job::JobId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationPreset {
    Smoke,
    Mixed,
    Stress,
    Soak,
    Scheduler,
    Aoi,
    Churn,
    Persistence,
}

impl ValidationPreset {
    pub const ALL: [Self; 8] = [
        Self::Smoke,
        Self::Mixed,
        Self::Stress,
        Self::Soak,
        Self::Scheduler,
        Self::Aoi,
        Self::Churn,
        Self::Persistence,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Mixed => "mixed",
            Self::Stress => "stress",
            Self::Soak => "soak",
            Self::Scheduler => "scheduler",
            Self::Aoi => "aoi",
            Self::Churn => "churn",
            Self::Persistence => "persistence",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationDuration {
    TwentySeconds,
    TwoMinutes,
    FiveMinutes,
    TenMinutes,
    ThirtyMinutes,
}

impl ValidationDuration {
    pub const ALL: [Self; 5] = [
        Self::TwentySeconds,
        Self::TwoMinutes,
        Self::FiveMinutes,
        Self::TenMinutes,
        Self::ThirtyMinutes,
    ];

    #[must_use]
    pub fn as_cli(self) -> &'static str {
        match self {
            Self::TwentySeconds => "20s",
            Self::TwoMinutes => "2m",
            Self::FiveMinutes => "5m",
            Self::TenMinutes => "10m",
            Self::ThirtyMinutes => "30m",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationSpec {
    pub preset: ValidationPreset,
    pub duration: Option<ValidationDuration>,
    pub seed: String,
}

impl Default for ValidationSpec {
    fn default() -> Self {
        Self {
            preset: ValidationPreset::Mixed,
            duration: None,
            seed: "1234".to_string(),
        }
    }
}

impl ValidationSpec {
    #[must_use]
    pub fn smoke() -> Self {
        Self {
            preset: ValidationPreset::Smoke,
            duration: Some(ValidationDuration::TwentySeconds),
            seed: "1234".to_string(),
        }
    }

    #[must_use]
    pub fn normalized(mut self) -> Self {
        if self.seed.trim().is_empty() {
            self.seed = "1234".to_string();
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationState {
    Idle,
    Preparing,
    Building,
    PreparingServer,
    WaitingForReady,
    Running,
    Cancelling,
    Passed,
    Failed,
    Cancelled,
    OrchestrationFailed,
}

impl ValidationState {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Preparing => "Preparing",
            Self::Building => "Building",
            Self::PreparingServer => "PreparingServer",
            Self::WaitingForReady => "WaitingForReady",
            Self::Running => "Running",
            Self::Cancelling => "Cancelling",
            Self::Passed => "Passed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
            Self::OrchestrationFailed => "OrchestrationFailed",
        }
    }

    #[must_use]
    pub fn is_active(self) -> bool {
        !matches!(
            self,
            Self::Idle | Self::Passed | Self::Failed | Self::Cancelled | Self::OrchestrationFailed
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationPaths {
    pub stamp: String,
    pub persist_root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ValidationJob {
    pub job_id: JobId,
    pub spec: ValidationSpec,
    pub phase: ValidationState,
    pub paths: ValidationPaths,
    pub argv: Vec<String>,
    pub harness_pid: Option<u32>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ValidationLiveStatus {
    pub status_line: Option<String>,
    pub available: bool,
    pub state: Option<String>,
    pub elapsed_secs: Option<f64>,
    pub duration_secs: Option<u64>,
    pub real_connected: Option<u32>,
    pub persistent_target: Option<u32>,
    pub churn_connected: Option<u32>,
    pub portal_transitions: Option<u64>,
    pub portal_attempts: Option<u64>,
    pub failures: Option<u64>,
    pub early_fail: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatusReasonView {
    pub code: String,
    pub message: String,
}

/// Selected fields from `run_summary.json` for Hub presentation (not pass/fail authority).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunSummaryBrief {
    pub available: bool,
    pub run_status: String,
    pub elapsed_secs: f64,
    pub requested_duration_secs: u64,
    pub requested_bots: u32,
    pub peak_connected: u32,
    pub failure_class: String,
    pub reasons: Vec<StatusReasonView>,
    pub tick_work_mean_ms: Option<f64>,
    pub tick_work_p95_ms: Option<f64>,
    pub tick_work_max_ms: Option<f64>,
    pub memory_peak_mb: Option<f64>,
    pub unexpected_disconnects: u64,
    pub overflow_events: u64,
    pub admission_refusals: u64,
    pub preset: Option<String>,
    pub seed: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ValidationLastResult {
    pub outcome: Option<ValidationState>,
    pub dir: Option<PathBuf>,
    pub summary: RunSummaryBrief,
}

/// Keep in sync with `Get-RuntimeValidationArgv` / `developer_tools_runtime_validation_argv_parses`.
#[must_use]
pub fn validation_argv(spec: &ValidationSpec, persist_root: &Path) -> Vec<String> {
    let mut args = vec![
        "--preset".to_string(),
        spec.preset.as_str().to_string(),
        "--seed".to_string(),
        spec.seed.clone(),
        "--allow-high-count".to_string(),
        "--max-bots".to_string(),
        "256".to_string(),
        "--server".to_string(),
        format!("{LISTEN_HOST}:{LISTEN_PORT}"),
        "--metrics".to_string(),
        format!("{LISTEN_HOST}:{METRICS_PORT}"),
    ];
    if let Some(dur) = spec.duration {
        args.push("--duration".to_string());
        args.push(dur.as_cli().to_string());
    }
    args.push("--persist-root".to_string());
    args.push(persist_root.display().to_string());
    args
}

#[must_use]
pub fn load_logs_root(workspace: &Path) -> PathBuf {
    workspace.join("logs").join("load")
}

#[must_use]
pub fn allocate_persist(workspace: &Path, stamp: &str) -> PathBuf {
    load_logs_root(workspace)
        .join(format!("rv_{stamp}"))
        .join("persist")
}

#[must_use]
pub fn rv_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let tod = secs % 86400;
    let days = secs / 86400;
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}{m:02}{d:02}_{:02}{:02}{:02}",
        tod / 3600,
        (tod / 60) % 60,
        tod % 60
    )
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m, d)
}

pub fn parse_print_server_env(stdout: &str) -> Result<BTreeMap<String, String>, String> {
    let pairs: Vec<(String, String)> =
        serde_json::from_str(stdout.trim()).map_err(|e| format!("print-server-env JSON: {e}"))?;
    Ok(pairs.into_iter().collect())
}

pub fn is_stale_binary_stderr(stderr: &str) -> bool {
    stderr.contains("unexpected argument")
}

pub fn base_load_mode_env(persist_root: &Path) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert(
        purgatory_common::LOAD_MODE_ADMISSION_ENV.to_string(),
        LOAD_ADMISSION_CAP.to_string(),
    );
    env.insert(
        "PURGATORY_METRICS_PORT".to_string(),
        METRICS_PORT.to_string(),
    );
    env.insert(
        "PURGATORY_DATA_DIR".to_string(),
        persist_root.display().to_string(),
    );
    env
}

pub fn merge_server_env(
    persist_root: &Path,
    printed: BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut env = base_load_mode_env(persist_root);
    env.extend(printed);
    env
}

pub fn classify_harness_exit(code: i32) -> ValidationState {
    match code {
        0 => ValidationState::Passed,
        130 => ValidationState::Cancelled,
        _ => ValidationState::Failed,
    }
}

pub fn read_pointer_dir(load_root: &Path, name: &str) -> Option<PathBuf> {
    let text = std::fs::read_to_string(load_root.join(name)).ok()?;
    let line = text.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    let path = PathBuf::from(line);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(load_root.join(path))
    }
}

pub fn read_live_status(run_dir: &Path) -> ValidationLiveStatus {
    let path = run_dir.join("live_status.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return ValidationLiveStatus::default();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return ValidationLiveStatus::default();
    };
    let line = v
        .get("status_line")
        .and_then(|x| x.as_str())
        .map(str::to_string);
    let early = v.get("early_fail").and_then(|x| {
        if x.is_null() {
            None
        } else {
            x.as_str()
                .map(str::to_string)
                .or_else(|| Some(x.to_string()))
        }
    });
    ValidationLiveStatus {
        available: line.is_some() || v.get("elapsed_secs").is_some(),
        status_line: line,
        state: v.get("state").and_then(|x| x.as_str()).map(str::to_string),
        elapsed_secs: v.get("elapsed_secs").and_then(|x| x.as_f64()),
        duration_secs: v
            .get("duration_secs")
            .and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f as u64))),
        real_connected: v
            .get("real_connected")
            .and_then(|x| x.as_u64().map(|n| n as u32)),
        persistent_target: v
            .get("persistent_target")
            .and_then(|x| x.as_u64().map(|n| n as u32)),
        churn_connected: v
            .get("churn_connected")
            .and_then(|x| x.as_u64().map(|n| n as u32)),
        portal_transitions: v.get("portal_transitions").and_then(|x| x.as_u64()),
        portal_attempts: v.get("portal_attempts").and_then(|x| x.as_u64()),
        failures: v.get("failures").and_then(|x| x.as_u64()),
        early_fail: early,
    }
}

/// Best-effort parse of `run_summary.json`. Missing/malformed → unavailable (not failure).
#[must_use]
pub fn read_run_summary(run_dir: &Path) -> RunSummaryBrief {
    let path = run_dir.join("run_summary.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return RunSummaryBrief::default();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return RunSummaryBrief::default();
    };
    let reasons = v
        .get("status_reasons")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| {
                    Some(StatusReasonView {
                        code: r.get("code")?.as_str()?.to_string(),
                        message: r
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    RunSummaryBrief {
        available: true,
        run_status: v
            .get("run_status")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        elapsed_secs: v
            .get("elapsed_secs")
            .and_then(|x| x.as_f64())
            .unwrap_or(0.0),
        requested_duration_secs: v
            .get("requested_duration_secs")
            .and_then(|x| x.as_u64())
            .unwrap_or(0),
        requested_bots: v
            .get("requested_bots")
            .and_then(|x| x.as_u64().map(|n| n as u32))
            .unwrap_or(0),
        peak_connected: v
            .get("peak_connected")
            .and_then(|x| x.as_u64().map(|n| n as u32))
            .unwrap_or(0),
        failure_class: v
            .get("failure_class")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        reasons,
        tick_work_mean_ms: v.get("server_tick_work_mean_ms").and_then(|x| x.as_f64()),
        tick_work_p95_ms: v.get("server_tick_work_p95_ms").and_then(|x| x.as_f64()),
        tick_work_max_ms: v.get("server_tick_work_max_ms").and_then(|x| x.as_f64()),
        memory_peak_mb: v
            .get("server_memory_peak_mb")
            .and_then(|x| x.as_f64())
            .or_else(|| v.get("harness_memory_peak_mb").and_then(|x| x.as_f64())),
        unexpected_disconnects: v
            .get("unexpected_disconnects")
            .and_then(|x| x.as_u64())
            .unwrap_or(0),
        overflow_events: v
            .get("overflow_events")
            .and_then(|x| x.as_u64())
            .unwrap_or(0),
        admission_refusals: v
            .get("admission_refusals")
            .and_then(|x| x.as_u64())
            .unwrap_or(0),
        preset: v.get("preset").and_then(|x| x.as_str().map(str::to_string)),
        seed: v.get("seed").and_then(|x| x.as_u64()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_matches_developer_tools_contract() {
        let spec = ValidationSpec {
            preset: ValidationPreset::Mixed,
            duration: Some(ValidationDuration::FiveMinutes),
            seed: "1234".to_string(),
        };
        let persist = PathBuf::from("logs/load/rv_test/persist");
        let args = validation_argv(&spec, &persist);
        assert_eq!(
            args,
            vec![
                "--preset",
                "mixed",
                "--seed",
                "1234",
                "--allow-high-count",
                "--max-bots",
                "256",
                "--server",
                "127.0.0.1:5001",
                "--metrics",
                "127.0.0.1:5002",
                "--duration",
                "5m",
                "--persist-root",
                persist.display().to_string().as_str(),
            ]
        );
    }

    #[test]
    fn print_env_parses_pair_array() {
        let json = r#"[["PURGATORY_ADMISSION_CAP","256"],["PURGATORY_DATA_DIR","x"]]"#;
        let map = parse_print_server_env(json).unwrap();
        assert_eq!(map.get("PURGATORY_ADMISSION_CAP").unwrap(), "256");
    }

    #[test]
    fn harness_exit_authority() {
        assert_eq!(classify_harness_exit(0), ValidationState::Passed);
        assert_eq!(classify_harness_exit(1), ValidationState::Failed);
        assert_eq!(classify_harness_exit(2), ValidationState::Failed);
        assert_eq!(classify_harness_exit(130), ValidationState::Cancelled);
    }

    #[test]
    fn live_status_parses_structured_fields() {
        let dir = std::env::temp_dir().join(format!("purgatory-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("live_status.json"),
            r#"{"state":"running","elapsed_secs":12.5,"duration_secs":120,"real_connected":2,"persistent_target":4,"churn_connected":1,"portal_transitions":0,"portal_attempts":0,"failures":0,"status_line":"RUNNING 00:12 / 02:00","early_fail":null}"#,
        )
        .unwrap();
        let live = read_live_status(&dir);
        assert!(live.available);
        assert_eq!(live.elapsed_secs, Some(12.5));
        assert_eq!(live.real_connected, Some(2));
        assert_eq!(live.persistent_target, Some(4));
        assert!(live.status_line.as_ref().unwrap().contains("RUNNING"));
    }

    #[test]
    fn run_summary_brief_parses_key_fields() {
        let dir = std::env::temp_dir().join(format!("purgatory-sum-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("run_summary.json"),
            r#"{"run_status":"FAIL","status_reasons":[{"code":"disconnect","message":"unexpected"}],"elapsed_secs":30.0,"requested_bots":10,"peak_connected":8,"failure_class":"scenario","unexpected_disconnects":2,"overflow_events":0,"admission_refusals":0,"requested_duration_secs":60,"server_tick_work_p95_ms":4.2,"preset":"smoke"}"#,
        )
        .unwrap();
        let s = read_run_summary(&dir);
        assert!(s.available);
        assert_eq!(s.run_status, "FAIL");
        assert_eq!(s.peak_connected, 8);
        assert_eq!(s.reasons.len(), 1);
        assert_eq!(s.tick_work_p95_ms, Some(4.2));
    }
}
