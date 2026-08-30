//! Namespaced load/test validation configuration.
//!
//! Serialized as JSON in [`LOAD_VALIDATION_ENV`]. Unset or empty means
//! production behavior: no synthetic entities and no test pressure.
//!
//! This is **test infrastructure**, not production runtime semantics. The
//! server must ignore it unless load/test mode is explicitly enabled.

use serde::{Deserialize, Serialize};

/// Environment variable holding a JSON [`LoadValidationConfig`].
pub const LOAD_VALIDATION_ENV: &str = "PURGATORY_LOAD_VALIDATION";

/// Existing load-mode switch. Not a synthetic-kind variable; production
/// servers leave this unset (default admission). Pressure is applied only
/// when this is set **and** [`LOAD_VALIDATION_ENV`] is active.
pub const LOAD_MODE_ADMISSION_ENV: &str = "PURGATORY_ADMISSION_CAP";

/// Opt-in load-validation workload. Default is inactive.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadValidationConfig {
    /// Visible Generic entities for density / spatial / AOI pressure.
    #[serde(default)]
    pub synthetic_entities: u32,
    #[serde(default)]
    pub scheduler: SchedulerPressure,
    #[serde(default)]
    pub spawn_despawn: SpawnPressure,
    /// Synthetic `ActionKind::Test` owners (not network-originated).
    #[serde(default)]
    pub actions: u32,
    /// Synthetic `EffectKind::Test` targets.
    #[serde(default)]
    pub effects: u32,
    /// Synthetic staged `RuntimeEvent` producers.
    #[serde(default)]
    pub events: u32,
    /// Synthetic cadence consumers (`EveryN` stagger).
    #[serde(default)]
    pub cadence_consumers: u32,
}

/// Scheduler pressure. Zero means no extra jobs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerPressure {
    #[serde(default)]
    pub critical: u32,
    #[serde(default)]
    pub deferred: u32,
    #[serde(default)]
    pub cancel_churn: u32,
}

/// Scheduled spawn/despawn churn. Zero count means none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnPressure {
    #[serde(default)]
    pub count: u32,
    /// Delay between spawn and despawn in simulation ticks. 0 = use server default.
    #[serde(default)]
    pub interval_ticks: u32,
}

impl LoadValidationConfig {
    /// True when any pressure field is non-zero.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.synthetic_entities > 0
            || self.scheduler.critical > 0
            || self.scheduler.deferred > 0
            || self.scheduler.cancel_churn > 0
            || self.spawn_despawn.count > 0
            || self.actions > 0
            || self.effects > 0
            || self.events > 0
            || self.cadence_consumers > 0
    }

    /// Parse env text. `None`, empty, and `{}` are inactive configs.
    pub fn parse_env(raw: Option<&str>) -> Result<Self, String> {
        let Some(text) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(Self::default());
        };
        serde_json::from_str(text).map_err(|err| format!("{LOAD_VALIDATION_ENV}: {err}"))
    }

    /// Compact JSON suitable for a single environment variable.
    #[must_use]
    pub fn to_env_value(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// True when the process is in load/test mode (`PURGATORY_ADMISSION_CAP` set).
    #[must_use]
    pub fn load_mode_from(raw_admission: Option<&str>) -> bool {
        raw_admission.is_some_and(|s| !s.trim().is_empty())
    }

    /// Parse validation JSON only when load-mode is enabled. Otherwise inactive.
    pub fn from_load_mode(load_mode: bool, raw_json: Option<&str>) -> Result<Self, String> {
        if !load_mode {
            return Ok(Self::default());
        }
        Self::parse_env(raw_json)
    }

    /// Process env: ignore validation unless load-mode is on. Malformed JSON
    /// yields inactive config (logged by the caller).
    pub fn from_process_env() -> Result<Self, String> {
        let load_mode =
            Self::load_mode_from(std::env::var(LOAD_MODE_ADMISSION_ENV).ok().as_deref());
        Self::from_load_mode(
            load_mode,
            std::env::var(LOAD_VALIDATION_ENV).ok().as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_and_empty_are_inactive() {
        assert!(!LoadValidationConfig::parse_env(None).unwrap().is_active());
        assert!(
            !LoadValidationConfig::parse_env(Some(""))
                .unwrap()
                .is_active()
        );
        assert!(
            !LoadValidationConfig::parse_env(Some("  "))
                .unwrap()
                .is_active()
        );
        assert!(
            !LoadValidationConfig::parse_env(Some("{}"))
                .unwrap()
                .is_active()
        );
    }

    #[test]
    fn json_roundtrip_preserves_pressure() {
        let cfg = LoadValidationConfig {
            synthetic_entities: 64,
            scheduler: SchedulerPressure {
                critical: 8,
                deferred: 32,
                cancel_churn: 4,
            },
            spawn_despawn: SpawnPressure {
                count: 16,
                interval_ticks: 30,
            },
            actions: 2,
            effects: 3,
            events: 4,
            cadence_consumers: 12,
        };
        let parsed = LoadValidationConfig::parse_env(Some(&cfg.to_env_value())).unwrap();
        assert_eq!(parsed, cfg);
        assert!(parsed.is_active());
    }

    #[test]
    fn malformed_json_is_error() {
        assert!(LoadValidationConfig::parse_env(Some("{")).is_err());
    }

    #[test]
    fn env_name_is_stable() {
        assert_eq!(LOAD_VALIDATION_ENV, "PURGATORY_LOAD_VALIDATION");
        assert_eq!(LOAD_MODE_ADMISSION_ENV, "PURGATORY_ADMISSION_CAP");
    }

    #[test]
    fn inactive_without_load_mode_even_if_json_is_set() {
        let json = r#"{"synthetic_entities":8}"#;
        let parsed = LoadValidationConfig::from_load_mode(false, Some(json)).unwrap();
        assert!(!parsed.is_active());
        assert_eq!(parsed.synthetic_entities, 0);
    }

    #[test]
    fn load_mode_applies_json() {
        let json = r#"{"synthetic_entities":8,"actions":1}"#;
        let parsed = LoadValidationConfig::from_load_mode(true, Some(json)).unwrap();
        assert!(parsed.is_active());
        assert_eq!(parsed.synthetic_entities, 8);
        assert_eq!(parsed.actions, 1);
    }

    #[test]
    fn load_mode_requires_nonempty_admission() {
        assert!(!LoadValidationConfig::load_mode_from(None));
        assert!(!LoadValidationConfig::load_mode_from(Some("")));
        assert!(!LoadValidationConfig::load_mode_from(Some("  ")));
        assert!(LoadValidationConfig::load_mode_from(Some("256")));
    }
}
