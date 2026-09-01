//! Load / soak harness contracts (Slice 3). Distinct from Runtime Validation presets.

use std::path::{Path, PathBuf};

use purgatory_common::LoadMetricsV1;

use crate::config::{LISTEN_HOST, LISTEN_PORT, LOAD_ADMISSION_CAP, METRICS_PORT};
use crate::job::JobId;
use crate::validation::{
    ValidationLiveStatus, ValidationState, classify_harness_exit, load_logs_root, read_live_status,
    read_pointer_dir,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadProfile {
    Idle,
    Walker,
    Jumper,
    Mixed,
}

impl LoadProfile {
    pub const ALL: [Self; 4] = [Self::Idle, Self::Walker, Self::Jumper, Self::Mixed];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Walker => "walker",
            Self::Jumper => "jumper",
            Self::Mixed => "mixed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadScenario {
    Load,
    Burst,
    Churn,
}

impl LoadScenario {
    pub const ALL: [Self; 3] = [Self::Load, Self::Burst, Self::Churn];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::Burst => "burst",
            Self::Churn => "churn",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadDuration {
    OneMinute,
    TwoMinutes,
    FiveMinutes,
    TenMinutes,
    ThirtyMinutes,
}

impl LoadDuration {
    pub const ALL: [Self; 5] = [
        Self::OneMinute,
        Self::TwoMinutes,
        Self::FiveMinutes,
        Self::TenMinutes,
        Self::ThirtyMinutes,
    ];

    #[must_use]
    pub fn as_cli(self) -> &'static str {
        match self {
            Self::OneMinute => "1m",
            Self::TwoMinutes => "2m",
            Self::FiveMinutes => "5m",
            Self::TenMinutes => "10m",
            Self::ThirtyMinutes => "30m",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadSpec {
    pub count: u32,
    pub profile: LoadProfile,
    pub scenario: LoadScenario,
    pub duration: LoadDuration,
    pub seed: String,
}

impl Default for LoadSpec {
    fn default() -> Self {
        Self {
            count: 10,
            profile: LoadProfile::Mixed,
            scenario: LoadScenario::Load,
            duration: LoadDuration::TwoMinutes,
            seed: "1234".to_string(),
        }
    }
}

impl LoadSpec {
    pub const COUNTS: [u32; 6] = [1, 2, 10, 25, 50, 100];

    #[must_use]
    pub fn normalized(mut self) -> Self {
        if self.seed.trim().is_empty() {
            self.seed = "1234".to_string();
        }
        if !Self::COUNTS.contains(&self.count) {
            self.count = 10;
        }
        self
    }
}

/// Presentation phases for a load job (not a ServerState).
pub type LoadState = ValidationState;

#[derive(Clone, Debug)]
pub struct LoadJob {
    pub job_id: JobId,
    pub spec: LoadSpec,
    pub phase: LoadState,
    pub argv: Vec<String>,
    pub harness_pid: Option<u32>,
    pub reason: Option<String>,
    pub needs_restart: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadLastResult {
    pub outcome: Option<LoadState>,
    pub dir: Option<PathBuf>,
    pub summary: crate::validation::RunSummaryBrief,
}

#[must_use]
pub fn load_argv(spec: &LoadSpec, max_bots: u32) -> Vec<String> {
    vec![
        "--count".to_string(),
        spec.count.to_string(),
        "--profile".to_string(),
        spec.profile.as_str().to_string(),
        "--scenario".to_string(),
        spec.scenario.as_str().to_string(),
        "--duration".to_string(),
        spec.duration.as_cli().to_string(),
        "--seed".to_string(),
        spec.seed.clone(),
        "--max-bots".to_string(),
        max_bots.to_string(),
        "--allow-high-count".to_string(),
        "--server".to_string(),
        format!("{LISTEN_HOST}:{LISTEN_PORT}"),
        "--metrics".to_string(),
        format!("{LISTEN_HOST}:{METRICS_PORT}"),
    ]
}

/// PowerShell `Test-LoadProbeCompatible`.
#[must_use]
pub fn load_probe_compatible(metrics: Option<&LoadMetricsV1>, count: u32) -> bool {
    let Some(m) = metrics else {
        return false;
    };
    if m.metrics_schema_version < 1 {
        return false;
    }
    if m.admission_cap < u64::from(count) {
        return false;
    }
    if m.max_entities_per_snapshot < u64::from(count) {
        return false;
    }
    true
}

#[must_use]
pub fn load_mode_extra_env() -> std::collections::BTreeMap<String, String> {
    // Persist root for plain load tests is not isolated like RV; PowerShell only
    // sets admission + metrics for WantLoadMode. Keep that contract.
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        purgatory_common::LOAD_MODE_ADMISSION_ENV.to_string(),
        LOAD_ADMISSION_CAP.to_string(),
    );
    env.insert(
        "PURGATORY_METRICS_PORT".to_string(),
        METRICS_PORT.to_string(),
    );
    env
}

pub fn resolve_last_finished_run(workspace: &Path, harness_running: bool) -> Option<PathBuf> {
    let load_root = load_logs_root(workspace);
    let current = read_pointer_dir(&load_root, "current_run.txt").and_then(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .or_else(|| Some(p.display().to_string()))
    });
    for name in [
        "last_runtime_validation.txt",
        "last_finished.txt",
        "latest.txt",
    ] {
        if let Some(dir) = read_pointer_dir(&load_root, name) {
            if harness_running
                && current.as_ref().is_some_and(|c| {
                    dir.ends_with(c.as_str())
                        || dir
                            .file_name()
                            .map(|n| n.to_string_lossy() == c.as_str())
                            .unwrap_or(false)
                })
            {
                continue;
            }
            if dir.join("run_summary.json").is_file() {
                return Some(dir);
            }
        }
    }
    latest_completed_run_dir(&load_root, harness_running.then_some(current).flatten())
}

fn latest_completed_run_dir(load_root: &Path, exclude: Option<String>) -> Option<PathBuf> {
    let entries = std::fs::read_dir(load_root).ok()?;
    let mut best: Option<(PathBuf, std::time::SystemTime)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(ex) = &exclude
            && path
                .file_name()
                .is_some_and(|n| n.to_string_lossy() == ex.as_str())
        {
            continue;
        }
        let summary = path.join("run_summary.json");
        if !summary.is_file() {
            continue;
        }
        let Ok(meta) = summary.metadata() else {
            continue;
        };
        let Ok(mtime) = meta.modified() else {
            continue;
        };
        if best.as_ref().is_none_or(|(_, t)| mtime > *t) {
            best = Some((path, mtime));
        }
    }
    best.map(|(p, _)| p)
}

pub fn observe_load_live(workspace: &Path, running: bool) -> ValidationLiveStatus {
    if !running {
        return ValidationLiveStatus::default();
    }
    let load_root = load_logs_root(workspace);
    read_pointer_dir(&load_root, "current_run.txt")
        .map(|dir| read_live_status(&dir))
        .unwrap_or_default()
}

pub fn classify_load_exit(code: i32) -> LoadState {
    classify_harness_exit(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_matches_powershell_load_contract() {
        let spec = LoadSpec {
            count: 10,
            profile: LoadProfile::Mixed,
            scenario: LoadScenario::Load,
            duration: LoadDuration::TwoMinutes,
            seed: "1234".to_string(),
        };
        assert_eq!(
            load_argv(&spec, 256),
            vec![
                "--count",
                "10",
                "--profile",
                "mixed",
                "--scenario",
                "load",
                "--duration",
                "2m",
                "--seed",
                "1234",
                "--max-bots",
                "256",
                "--allow-high-count",
                "--server",
                "127.0.0.1:5001",
                "--metrics",
                "127.0.0.1:5002",
            ]
        );
    }

    #[test]
    fn probe_compatible_requires_admission() {
        let mut m = LoadMetricsV1::with_schema();
        m.admission_cap = 5;
        m.max_entities_per_snapshot = 256;
        assert!(!load_probe_compatible(Some(&m), 10));
        m.admission_cap = 256;
        assert!(load_probe_compatible(Some(&m), 10));
        assert!(!load_probe_compatible(None, 1));
    }
}
