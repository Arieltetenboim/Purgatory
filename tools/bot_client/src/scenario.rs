//! Typed load/validation scenario definition (Phase 6G).
//!
//! CLI flags and Developer Tools presets resolve to [`LoadScenario`]. Scenario
//! behavior, assertions, and pass/fail stay in Rust. PowerShell only forwards
//! argv and environment.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::ValueEnum;
use clap::parser::{ArgMatches, ValueSource};
use serde::{Deserialize, Serialize};

use purgatory_common::{
    DEV_LOGIN_MAX_LEN, DevLogin, LoadValidationConfig, SchedulerPressure, SpawnPressure,
};

use crate::behavior::BotProfile;
use crate::cli::Cli;

/// Phase 5 connect cadence. Preserved for ramp/burst/churn/steady compatibility.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    #[default]
    Load,
    Burst,
    Churn,
    /// Connect + quiet hold, then activate 30 Hz input (ramp-isolation).
    Steady,
}

/// High-level 6G workload kind. Connect cadence remains [`Scenario`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum LoadKind {
    /// Hello/Welcome/replication baseline (also the Phase 5 default).
    #[default]
    Connectivity,
    Movement,
    AoiChurn,
    PortalChurn,
    ReconnectChurn,
    DuplicateIdentity,
    RuntimeScheduler,
    SpawnDespawn,
    PersistenceChurn,
    /// Canonical Phase 6 integrated acceptance workload.
    MixedRuntime,
    Soak,
}

/// Named 6G presets. CLI flags overlay these defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ValidationPreset {
    Smoke,
    Churn,
    Aoi,
    Scheduler,
    Persistence,
    Mixed,
    Stress,
    Soak,
}

/// Resolved, serializable scenario. Recorded as `scenario.json`.
///
/// Welcome (protocol v10) carries `ConnectionId` only — not `CharacterId`.
/// Local `EntityId` comes from `ReplicationFrame.local_player_entity`.
/// Same-login Character identity is validated in-process / occupancy metrics,
/// not by adding protocol fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadScenario {
    pub kind: LoadKind,
    pub preset: Option<ValidationPreset>,
    pub connect: Scenario,
    pub profile: BotProfile,
    pub bot_count: u32,
    pub seed: u64,
    pub run_id: String,
    pub duration_secs: u64,
    /// Wall-clock abort distinct from scenario duration. Not a gameplay timer.
    pub timeout_secs: u64,
    pub ramp_ms: u64,
    pub quiet_secs: u64,
    pub churn_interval_secs: u64,
    pub churn_fraction: f32,
    pub isolate_persist: bool,
    /// Intended persist root for isolated runs. Server must set `PURGATORY_DATA_DIR`.
    pub persist_root: Option<String>,
    pub validation: LoadValidationConfig,
    pub protocol_version: u32,
}

impl LoadScenario {
    /// Resolve CLI + optional clap matches (so preset defaults do not clobber flags).
    pub fn from_cli(cli: &Cli, matches: Option<&ArgMatches>) -> Result<Self, String> {
        let user = |id: &str| {
            matches.is_some_and(|m| m.value_source(id) == Some(ValueSource::CommandLine))
        };

        let mut kind = LoadKind::Connectivity;
        let mut connect = cli.scenario;
        let mut profile = cli.profile;
        let mut bot_count = cli.count;
        let mut duration = cli.duration;
        let mut timeout = cli.timeout;
        let mut ramp_ms = cli.ramp_ms;
        let mut isolate = cli.isolate_persist;
        let mut validation = LoadValidationConfig::default();

        if let Some(preset) = cli.preset {
            let d = preset.defaults();
            kind = d.kind;
            if !user("scenario") {
                connect = d.connect;
            }
            if !user("profile") {
                profile = d.profile;
            }
            if !user("count") {
                bot_count = d.bot_count;
            }
            if !user("duration") {
                duration = d.duration;
            }
            if timeout.is_none() {
                timeout = Some(d.timeout);
            }
            if !user("ramp_ms") {
                ramp_ms = d.ramp_ms;
            }
            if !cli.raw_persist && !user("isolate_persist") {
                isolate = true;
            }
            validation = d.validation;
        }

        if cli.raw_persist {
            isolate = false;
        } else if cli.isolate_persist {
            isolate = true;
        }

        let timeout = timeout.unwrap_or(duration.saturating_add(Duration::from_secs(60)));
        if timeout < duration {
            return Err("timeout must be >= duration".into());
        }

        let run_id = match cli.run_id.as_deref() {
            Some(raw) => normalize_run_id(raw)?,
            None => default_run_id(cli.seed),
        };

        let mut spec = Self {
            kind,
            preset: cli.preset,
            connect,
            profile,
            bot_count,
            seed: cli.seed,
            run_id,
            duration_secs: duration.as_secs().max(1),
            timeout_secs: timeout.as_secs().max(1),
            ramp_ms,
            quiet_secs: cli.quiet_secs.as_secs(),
            churn_interval_secs: cli.churn_interval.as_secs().max(1),
            churn_fraction: cli.churn_fraction,
            isolate_persist: isolate,
            persist_root: None,
            validation,
            protocol_version: purgatory_protocol::PROTOCOL_VERSION,
        };
        if let Some(root) = &cli.persist_root {
            spec.isolate_persist = true;
            spec.persist_root = Some(root.to_string_lossy().replace('\\', "/"));
        }
        Ok(spec)
    }

    pub fn validate_count(&self, max_bots: u32, allow_high_count: bool) -> Result<(), String> {
        if self.bot_count > max_bots && !allow_high_count {
            return Err(format!(
                "count {} exceeds max-bots {max_bots} (use --allow-high-count to override)",
                self.bot_count
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn duration(&self) -> Duration {
        Duration::from_secs(self.duration_secs)
    }

    #[must_use]
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }

    /// Deterministic DevLogin for a bot index. Unique per run; valid charset.
    #[must_use]
    pub fn bot_login(&self, bot_index: u32) -> String {
        bot_dev_login(&self.run_id, bot_index)
    }

    /// Bind persist to an existing directory (Developer Tools / headless agreement).
    pub fn with_explicit_persist_root(mut self, root: &Path) -> Self {
        self.isolate_persist = true;
        self.persist_root = Some(root.to_string_lossy().replace('\\', "/"));
        self
    }

    /// Bind the persist directory that lives under the artifact run dir.
    pub fn with_persist_root(mut self, run_dir: &Path) -> Self {
        if self.isolate_persist && self.persist_root.is_none() {
            self.persist_root = Some(run_dir.join("persist").to_string_lossy().replace('\\', "/"));
        }
        self
    }

    #[must_use]
    pub fn recommended_server_env(&self) -> Vec<(String, String)> {
        let mut env = Vec::new();
        if let Some(root) = &self.persist_root {
            env.push(("PURGATORY_DATA_DIR".into(), root.clone()));
        }
        if self.validation.is_active() {
            env.push((
                purgatory_common::LOAD_MODE_ADMISSION_ENV.into(),
                "256".into(),
            ));
            env.push((
                "PURGATORY_METRICS_PORT".into(),
                purgatory_common::DEFAULT_METRICS_PORT.to_string(),
            ));
            env.push((
                purgatory_common::LOAD_VALIDATION_ENV.into(),
                self.validation.to_env_value(),
            ));
        }
        env
    }
}

struct PresetDefaults {
    kind: LoadKind,
    connect: Scenario,
    profile: BotProfile,
    bot_count: u32,
    duration: Duration,
    timeout: Duration,
    ramp_ms: u64,
    validation: LoadValidationConfig,
}

impl ValidationPreset {
    fn defaults(self) -> PresetDefaults {
        match self {
            Self::Smoke => PresetDefaults {
                kind: LoadKind::MixedRuntime,
                connect: Scenario::Load,
                profile: BotProfile::Mixed,
                bot_count: 4,
                duration: Duration::from_secs(20),
                timeout: Duration::from_secs(45),
                ramp_ms: 50,
                validation: LoadValidationConfig {
                    synthetic_entities: 16,
                    scheduler: SchedulerPressure {
                        critical: 8,
                        deferred: 16,
                        cancel_churn: 4,
                    },
                    spawn_despawn: SpawnPressure {
                        count: 8,
                        interval_ticks: 30,
                    },
                    actions: 2,
                    effects: 2,
                    events: 4,
                    cadence_consumers: 8,
                },
            },
            Self::Churn => PresetDefaults {
                kind: LoadKind::ReconnectChurn,
                connect: Scenario::Churn,
                profile: BotProfile::Idle,
                bot_count: 8,
                duration: Duration::from_secs(60),
                timeout: Duration::from_secs(90),
                ramp_ms: 50,
                validation: LoadValidationConfig::default(),
            },
            Self::Aoi => PresetDefaults {
                kind: LoadKind::AoiChurn,
                connect: Scenario::Load,
                profile: BotProfile::Walker,
                bot_count: 8,
                duration: Duration::from_secs(60),
                timeout: Duration::from_secs(90),
                ramp_ms: 50,
                validation: LoadValidationConfig {
                    synthetic_entities: 32,
                    ..LoadValidationConfig::default()
                },
            },
            Self::Scheduler => PresetDefaults {
                kind: LoadKind::RuntimeScheduler,
                connect: Scenario::Load,
                profile: BotProfile::Idle,
                bot_count: 2,
                duration: Duration::from_secs(45),
                timeout: Duration::from_secs(75),
                ramp_ms: 50,
                validation: LoadValidationConfig {
                    scheduler: SchedulerPressure {
                        critical: 256,
                        deferred: 64,
                        cancel_churn: 32,
                    },
                    cadence_consumers: 64,
                    ..LoadValidationConfig::default()
                },
            },
            Self::Persistence => PresetDefaults {
                kind: LoadKind::PersistenceChurn,
                connect: Scenario::Churn,
                profile: BotProfile::Idle,
                bot_count: 6,
                duration: Duration::from_secs(60),
                timeout: Duration::from_secs(90),
                ramp_ms: 50,
                validation: LoadValidationConfig::default(),
            },
            Self::Mixed => PresetDefaults {
                kind: LoadKind::MixedRuntime,
                connect: Scenario::Load,
                profile: BotProfile::Mixed,
                bot_count: 8,
                duration: Duration::from_secs(120),
                timeout: Duration::from_secs(180),
                ramp_ms: 50,
                validation: mixed_validation(64),
            },
            Self::Stress => PresetDefaults {
                kind: LoadKind::MixedRuntime,
                connect: Scenario::Load,
                profile: BotProfile::Mixed,
                bot_count: 25,
                duration: Duration::from_secs(120),
                timeout: Duration::from_secs(240),
                ramp_ms: 25,
                validation: mixed_validation(256),
            },
            Self::Soak => PresetDefaults {
                kind: LoadKind::Soak,
                connect: Scenario::Load,
                profile: BotProfile::Mixed,
                bot_count: 8,
                duration: Duration::from_secs(30 * 60),
                timeout: Duration::from_secs(32 * 60),
                ramp_ms: 50,
                validation: mixed_validation(64),
            },
        }
    }
}

fn mixed_validation(synthetic: u32) -> LoadValidationConfig {
    LoadValidationConfig {
        synthetic_entities: synthetic,
        scheduler: SchedulerPressure {
            critical: 32,
            deferred: 48,
            cancel_churn: 8,
        },
        spawn_despawn: SpawnPressure {
            count: 16,
            interval_ticks: 60,
        },
        actions: 4,
        effects: 8,
        events: 16,
        cadence_consumers: 32,
    }
}

fn normalize_run_id(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("run-id must be non-empty".into());
    }
    let mut out = String::new();
    for c in trimmed.chars() {
        if c.is_ascii_uppercase() {
            out.push(c.to_ascii_lowercase());
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
            out.push(c);
        } else {
            return Err(format!("run-id has invalid character {c:?}"));
        }
        if out.len() == 8 {
            break;
        }
    }
    if out.len() < 2 {
        return Err("run-id must have at least 2 [a-z0-9_] characters".into());
    }
    Ok(out)
}

fn default_run_id(seed: u64) -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{:04x}{:04x}", seed as u16, secs as u16)
}

/// `r{run_id}.{idx:04}` — unique per run, DevLogin-valid, not a CharacterId.
#[must_use]
pub fn bot_dev_login(run_id: &str, bot_index: u32) -> String {
    let login = format!("r{run_id}.{bot_index:04}");
    debug_assert!(login.len() <= DEV_LOGIN_MAX_LEN);
    debug_assert!(DevLogin::parse(&login).is_ok());
    login
}

/// Persist path under a run directory. Never `%LOCALAPPDATA%\Purgatory`.
#[must_use]
pub fn isolated_persist_dir(run_dir: &Path) -> PathBuf {
    run_dir.join("persist")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, FromArgMatches};

    fn parse(args: &[&str]) -> (Cli, LoadScenario) {
        let matches = Cli::command()
            .try_get_matches_from(args)
            .unwrap_or_else(|e| panic!("{e}"));
        let cli = Cli::from_arg_matches(&matches).expect("cli");
        let spec = LoadScenario::from_cli(&cli, Some(&matches)).expect("spec");
        (cli, spec)
    }

    #[test]
    fn legacy_cli_keeps_phase5_defaults() {
        let (_cli, spec) = parse(&["purgatory-load", "--count", "3", "--seed", "9"]);
        assert_eq!(spec.kind, LoadKind::Connectivity);
        assert_eq!(spec.bot_count, 3);
        assert_eq!(spec.seed, 9);
        assert!(!spec.isolate_persist);
        assert!(!spec.validation.is_active());
        assert_eq!(spec.protocol_version, purgatory_protocol::PROTOCOL_VERSION);
    }

    #[test]
    fn preset_mixed_is_canonical_workload() {
        let (_cli, spec) = parse(&["purgatory-load", "--preset", "mixed"]);
        assert_eq!(spec.kind, LoadKind::MixedRuntime);
        assert_eq!(spec.preset, Some(ValidationPreset::Mixed));
        assert_eq!(spec.bot_count, 8);
        assert_eq!(spec.duration_secs, 120);
        assert!(spec.isolate_persist);
        assert!(spec.validation.is_active());
        assert_eq!(spec.validation.synthetic_entities, 64);
    }

    #[test]
    fn explicit_flags_overlay_preset() {
        let (_cli, spec) = parse(&[
            "purgatory-load",
            "--preset",
            "mixed",
            "--count",
            "12",
            "--duration",
            "30s",
            "--seed",
            "7",
        ]);
        assert_eq!(spec.bot_count, 12);
        assert_eq!(spec.duration_secs, 30);
        assert_eq!(spec.seed, 7);
        assert_eq!(spec.kind, LoadKind::MixedRuntime);
    }

    #[test]
    fn soak_duration_is_configurable_not_magic() {
        let (_cli, default_soak) = parse(&["purgatory-load", "--preset", "soak"]);
        assert_eq!(default_soak.duration_secs, 30 * 60);
        assert_eq!(default_soak.kind, LoadKind::Soak);
        let (_cli, short) = parse(&["purgatory-load", "--preset", "soak", "--duration", "2m"]);
        assert_eq!(short.duration_secs, 120);
        assert_eq!(short.kind, LoadKind::Soak);
        assert!(short.timeout_secs >= short.duration_secs);
    }

    #[test]
    fn timeout_is_distinct_from_duration() {
        let (_cli, spec) = parse(&["purgatory-load", "--duration", "10s", "--timeout", "25s"]);
        assert_eq!(spec.duration_secs, 10);
        assert_eq!(spec.timeout_secs, 25);
    }

    #[test]
    fn timeout_below_duration_is_rejected() {
        let matches = Cli::command()
            .try_get_matches_from(["purgatory-load", "--duration", "60s", "--timeout", "10s"])
            .unwrap();
        let cli = Cli::from_arg_matches(&matches).unwrap();
        assert!(LoadScenario::from_cli(&cli, Some(&matches)).is_err());
    }

    #[test]
    fn bot_login_is_valid_dev_login_and_unique() {
        let spec = LoadScenario {
            kind: LoadKind::Connectivity,
            preset: None,
            connect: Scenario::Load,
            profile: BotProfile::Idle,
            bot_count: 2,
            seed: 1,
            run_id: "abcd1234".into(),
            duration_secs: 10,
            timeout_secs: 20,
            ramp_ms: 100,
            quiet_secs: 0,
            churn_interval_secs: 10,
            churn_fraction: 0.2,
            isolate_persist: true,
            persist_root: None,
            validation: LoadValidationConfig::default(),
            protocol_version: 10,
        };
        let a = spec.bot_login(1);
        let b = spec.bot_login(2);
        assert_ne!(a, b);
        assert_eq!(a, "rabcd1234.0001");
        assert!(DevLogin::parse(&a).is_ok());
        assert!(DevLogin::parse(&b).is_ok());
        assert!(a.len() <= DEV_LOGIN_MAX_LEN);
    }

    #[test]
    fn run_id_override_is_recorded() {
        let (_cli, spec) = parse(&["purgatory-load", "--run-id", "DEADBEEF"]);
        assert_eq!(spec.run_id, "deadbeef");
        assert!(spec.bot_login(1).starts_with("rdeadbeef."));
    }

    #[test]
    fn isolated_persist_is_under_run_dir_not_localappdata() {
        let run = Path::new("logs/load/example_run");
        let persist = isolated_persist_dir(run);
        let text = persist.to_string_lossy().replace('\\', "/");
        assert!(text.ends_with("logs/load/example_run/persist"));
        assert!(!text.to_ascii_lowercase().contains("localappdata"));
        assert!(!text.contains("AppData"));
    }

    #[test]
    fn raw_persist_disables_isolation_on_preset() {
        let (_cli, spec) = parse(&["purgatory-load", "--preset", "smoke", "--raw-persist"]);
        assert!(!spec.isolate_persist);
    }

    #[test]
    fn smoke_preset_is_short_mixed() {
        let (_cli, spec) = parse(&["purgatory-load", "--preset", "smoke"]);
        assert_eq!(spec.kind, LoadKind::MixedRuntime);
        assert_eq!(spec.bot_count, 4);
        assert_eq!(spec.duration_secs, 20);
        assert!(spec.timeout_secs > spec.duration_secs);
        assert!(spec.validation.synthetic_entities > 0);
    }

    #[test]
    fn scheduler_preset_uses_few_bots_and_synthetic_pressure() {
        let (_cli, spec) = parse(&["purgatory-load", "--preset", "scheduler"]);
        assert_eq!(spec.kind, LoadKind::RuntimeScheduler);
        assert_eq!(spec.bot_count, 2);
        assert!(spec.validation.scheduler.critical > 0);
        assert!(spec.validation.cadence_consumers > 0);
    }

    #[test]
    fn recommended_env_includes_namespaced_validation() {
        let mut spec = parse(&["purgatory-load", "--preset", "mixed"]).1;
        spec = spec.with_persist_root(Path::new("logs/load/t"));
        let env = spec.recommended_server_env();
        assert!(
            env.iter()
                .any(|(k, v)| k == "PURGATORY_DATA_DIR" && v.contains("/persist"))
        );
        assert!(
            env.iter()
                .any(|(k, _)| k == purgatory_common::LOAD_VALIDATION_ENV)
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == purgatory_common::LOAD_MODE_ADMISSION_ENV && v == "256")
        );
    }

    #[test]
    fn serde_roundtrip_records_seed() {
        let spec = parse(&["purgatory-load", "--preset", "mixed", "--seed", "4242"]).1;
        let json = serde_json::to_string(&spec).unwrap();
        let back: LoadScenario = serde_json::from_str(&json).unwrap();
        assert_eq!(back.seed, 4242);
        assert_eq!(back.kind, LoadKind::MixedRuntime);
    }

    #[test]
    fn persist_root_flag_isolates_and_is_in_recommended_env() {
        let (_cli, spec) = parse(&[
            "purgatory-load",
            "--preset",
            "mixed",
            "--persist-root",
            "D:/tmp/purgatory-run/persist",
        ]);
        assert!(spec.isolate_persist);
        assert_eq!(
            spec.persist_root.as_deref(),
            Some("D:/tmp/purgatory-run/persist")
        );
        let env = spec.recommended_server_env();
        assert!(
            env.iter()
                .any(|(k, v)| k == "PURGATORY_DATA_DIR" && v.contains("purgatory-run/persist"))
        );
    }
}
