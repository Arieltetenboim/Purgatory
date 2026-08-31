//! CLI argument parsing.

use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use clap::{CommandFactory, FromArgMatches, Parser};

use crate::BotProfile;
use crate::scenario::{LoadScenario, Scenario, ValidationPreset};

#[derive(Parser, Debug)]
#[command(name = "purgatory-load")]
#[command(about = "Headless load-test harness for PURGATORY")]
pub struct Cli {
    #[arg(long, default_value = "1")]
    pub count: u32,

    #[arg(long, value_enum, default_value = "idle")]
    pub profile: BotProfile,

    #[arg(long, value_enum, default_value = "load")]
    pub scenario: Scenario,

    /// Phase 6G typed preset. Overlays defaults; explicit flags win.
    #[arg(long, value_enum)]
    pub preset: Option<ValidationPreset>,

    #[arg(long, default_value = "12345")]
    pub seed: u64,

    /// Recorded run identity fragment used in bot DevLogins. Default mixes seed + time.
    #[arg(long)]
    pub run_id: Option<String>,

    /// For `steady`: active-input duration after activation. Otherwise total run time.
    #[arg(long, value_parser = parse_duration, default_value = "60s")]
    pub duration: Duration,

    /// Wall-clock abort distinct from `--duration`. Not a gameplay timer.
    #[arg(long, value_parser = parse_duration)]
    pub timeout: Option<Duration>,

    #[arg(long, default_value = "100")]
    pub ramp_ms: u64,

    /// Quiet connected hold before input activation (`steady` only).
    #[arg(long, value_parser = parse_duration, default_value = "15s")]
    pub quiet_secs: Duration,

    #[arg(long, value_parser = parse_duration, default_value = "10s")]
    pub churn_interval: Duration,

    #[arg(long, default_value = "0.2")]
    pub churn_fraction: f32,

    #[arg(long, default_value = "32")]
    pub max_bots: u32,

    #[arg(long, default_value = "false")]
    pub allow_high_count: bool,

    /// Request isolated persist (`PURGATORY_DATA_DIR` under the run dir).
    #[arg(long, default_value_t = false)]
    pub isolate_persist: bool,

    /// Opt out of isolation even when a 6G preset is selected.
    #[arg(long, default_value_t = false)]
    pub raw_persist: bool,

    #[arg(long, default_value = "127.0.0.1:5001")]
    pub server: SocketAddr,

    #[arg(long, default_value = "127.0.0.1:5002")]
    pub metrics: SocketAddr,

    /// One-shot Hello/Welcome readiness probe. Does not run a load scenario.
    #[arg(long, default_value_t = false)]
    pub probe: bool,

    /// Print recommended server env JSON and exit. CLI owns scenario semantics.
    #[arg(long, default_value_t = false)]
    pub print_server_env: bool,

    /// Isolated persist directory. Implies isolation. Server should set `PURGATORY_DATA_DIR`.
    #[arg(long)]
    pub persist_root: Option<std::path::PathBuf>,
}

impl Cli {
    pub fn validate(&self) -> Result<(), String> {
        if self.probe || self.print_server_env {
            return Ok(());
        }
        if self.raw_persist && self.isolate_persist {
            return Err("cannot combine --isolate-persist and --raw-persist".into());
        }
        if self.count > self.max_bots && !self.allow_high_count {
            return Err(format!(
                "count {} exceeds max-bots {} (use --allow-high-count to override)",
                self.count, self.max_bots
            ));
        }
        if self.churn_fraction < 0.0 || self.churn_fraction > 1.0 {
            return Err(format!(
                "churn-fraction must be between 0.0 and 1.0, got {}",
                self.churn_fraction
            ));
        }
        Ok(())
    }

    /// Parse argv into CLI plus a resolved [`LoadScenario`].
    pub fn parse_resolved() -> Result<(Self, LoadScenario), String> {
        let matches = Self::command().get_matches();
        let cli = Self::from_arg_matches(&matches).map_err(|err| err.to_string())?;
        cli.validate()?;
        let spec = LoadScenario::from_cli(&cli, Some(&matches))?;
        if !cli.probe && !cli.print_server_env {
            spec.validate_count(cli.max_bots, cli.allow_high_count)?;
        }
        Ok((cli, spec))
    }
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty duration".into());
    }
    if let Some(ms) = s.strip_suffix("ms") {
        let val: u64 = ms
            .parse()
            .map_err(|e| format!("invalid milliseconds: {e}"))?;
        return Ok(Duration::from_millis(val));
    }
    if let Some(secs) = s.strip_suffix('s') {
        let val: u64 = secs.parse().map_err(|e| format!("invalid seconds: {e}"))?;
        return Ok(Duration::from_secs(val));
    }
    if let Some(mins) = s.strip_suffix('m') {
        let val: u64 = mins.parse().map_err(|e| format!("invalid minutes: {e}"))?;
        return Ok(Duration::from_secs(val * 60));
    }
    u64::from_str(s)
        .map(Duration::from_secs)
        .map_err(|e| format!("invalid duration: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_duration_seconds() {
        assert_eq!(parse_duration("10s").unwrap(), Duration::from_secs(10));
        assert_eq!(parse_duration("60s").unwrap(), Duration::from_secs(60));
    }

    #[test]
    fn parse_duration_minutes() {
        assert_eq!(parse_duration("2m").unwrap(), Duration::from_secs(120));
        assert_eq!(parse_duration("10m").unwrap(), Duration::from_secs(600));
    }

    #[test]
    fn parse_duration_milliseconds() {
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
    }

    #[test]
    fn parse_duration_plain_number() {
        assert_eq!(parse_duration("30").unwrap(), Duration::from_secs(30));
    }

    fn sample_cli(count: u32, allow_high_count: bool) -> Cli {
        Cli {
            count,
            profile: BotProfile::Idle,
            scenario: Scenario::Load,
            preset: None,
            seed: 0,
            run_id: None,
            duration: Duration::from_secs(60),
            timeout: None,
            ramp_ms: 100,
            quiet_secs: Duration::from_secs(15),
            churn_interval: Duration::from_secs(10),
            churn_fraction: 0.2,
            max_bots: 32,
            allow_high_count,
            isolate_persist: false,
            raw_persist: false,
            server: "127.0.0.1:5001".parse().unwrap(),
            metrics: "127.0.0.1:5002".parse().unwrap(),
            probe: false,
            print_server_env: false,
            persist_root: None,
        }
    }

    #[test]
    fn validate_count_exceeds_max() {
        assert!(sample_cli(100, false).validate().is_err());
    }

    #[test]
    fn validate_count_exceeds_max_allowed() {
        assert!(sample_cli(100, true).validate().is_ok());
    }

    #[test]
    fn isolate_and_raw_conflict() {
        let mut cli = sample_cli(1, false);
        cli.isolate_persist = true;
        cli.raw_persist = true;
        assert!(cli.validate().is_err());
    }

    #[test]
    fn parse_probe_flag() {
        let cli = Cli::try_parse_from(["purgatory-load", "--probe"]).unwrap();
        assert!(cli.probe);
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn preset_flag_is_part_of_cli_help() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("--preset"), "{help}");
        assert!(help.contains("mixed"), "{help}");
        assert!(help.contains("soak"), "{help}");
        assert!(help.contains("--duration"), "{help}");
        assert!(help.contains("--print-server-env"), "{help}");
    }
}
