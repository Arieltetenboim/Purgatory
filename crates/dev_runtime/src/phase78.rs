//! Phase 7.8 production gate summary reader (Hub display only).
//!
//! Does not recompute capacity metrics. Authority stays with
//! `phase78_gate_summary.json` written by `scripts/phase_78_gate.ps1`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// How the latest gate summary was resolved for Hub display.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Phase78ReadStatus {
    /// No `gate_*` directory or no summary file under capacity_78.
    #[default]
    Missing,
    /// Summary file parsed successfully.
    Ok,
    /// Summary file exists but could not be read or parsed.
    ParseError,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Phase78GateBrief {
    pub status: Phase78ReadStatus,
    /// True only when [`Phase78ReadStatus::Ok`].
    pub available: bool,
    /// Compact human reason when status is ParseError (or Missing detail).
    pub read_error: Option<String>,
    pub verdict: String,
    pub stamp: String,
    pub run_root: String,
    pub baseline_compared: bool,
    pub baseline_path: Option<String>,
    pub unit_budget_status: String,
    pub cells: Vec<Phase78CellBrief>,
    /// Human note when verdict is YELLOW for known harness reasons.
    pub verdict_explanation: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Phase78CellBrief {
    pub cell_id: String,
    pub status: String,
    pub findings: Vec<Phase78FindingBrief>,
    pub tick_p95: Option<f64>,
    pub tick_p99: Option<f64>,
    pub tick_max: Option<f64>,
    pub tick_util: Option<f64>,
    pub dominant: Option<String>,
    pub server_cpu_mid: Option<f64>,
    pub server_rss_mb: Option<f64>,
    pub server_rss_delta_mb: Option<f64>,
    pub bytes_out_per_sec: Option<f64>,
    pub writer_queue_depth_max: Option<u64>,
    pub writer_queue_push_fail: Option<u64>,
    pub spawn_requested: Option<u64>,
    pub peak_active: Option<u64>,
    pub attainment_pct: Option<f64>,
    pub deaths_total: Option<u64>,
    pub snapshot_starvation: Option<u64>,
    pub harness_exit: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Phase78FindingBrief {
    pub severity: String,
    pub code: String,
    pub message: String,
}

impl Phase78FindingBrief {
    /// Hub must not collapse harness warnings into "server warn".
    #[must_use]
    pub fn class_label(&self) -> &'static str {
        if self.severity.eq_ignore_ascii_case("INVALID")
            || self.code.starts_with("HARNESS_")
            || self.code.starts_with("ATTAINMENT_")
            || self.code.starts_with("FUNNEL_")
            || self.code == "ARTIFACTS_MISSING"
            || self.code == "HARNESS_SHORT_RUN"
        {
            "HARNESS"
        } else if self.severity.eq_ignore_ascii_case("FAIL")
            || self.code.starts_with("TICK_")
            || self.code.starts_with("POLICY_")
            || self.code.starts_with("NPC_")
            || self.code.starts_with("RSS_")
            || self.code.starts_with("PUSH_FAIL")
            || self.code.starts_with("QUEUE_")
            || self.code.starts_with("LIFECYCLE_")
            || self.code == "ACCOUNTING"
            || self.code == "OVERRUNS"
            || self.code == "OVERRUN_STREAK"
            || self.code.starts_with("UNATTR_")
        {
            "SERVER"
        } else if self.code.starts_with("REG_") {
            "REGRESSION"
        } else if self.severity.eq_ignore_ascii_case("WARN") {
            "WARN"
        } else {
            "OTHER"
        }
    }
}

#[derive(Deserialize)]
struct RawSummary {
    #[serde(default)]
    verdict: String,
    #[serde(default)]
    stamp: String,
    #[serde(default)]
    run_root: String,
    #[serde(default)]
    baseline_compared: bool,
    #[serde(default)]
    baseline_path: Option<String>,
    #[serde(default)]
    unit_budget: Option<RawUnitBudget>,
    #[serde(default)]
    cells: Vec<RawCell>,
    #[serde(default)]
    semantics: Option<RawSemantics>,
}

#[derive(Deserialize)]
struct RawUnitBudget {
    #[serde(default)]
    status: String,
}

#[derive(Deserialize)]
struct RawSemantics {
    #[serde(default)]
    #[serde(rename = "YELLOW")]
    yellow: Option<String>,
}

#[derive(Deserialize)]
struct RawCell {
    #[serde(default)]
    cell_id: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    findings: Vec<RawFinding>,
    #[serde(default)]
    metrics: serde_json::Value,
}

#[derive(Deserialize)]
struct RawFinding {
    #[serde(default)]
    severity: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
}

fn json_f64(v: &serde_json::Value, key: &str) -> Option<f64> {
    v.get(key)
        .and_then(|x| x.as_f64().or_else(|| x.as_i64().map(|i| i as f64)))
}

fn json_u64(v: &serde_json::Value, key: &str) -> Option<u64> {
    v.get(key)
        .and_then(|x| x.as_u64().or_else(|| x.as_i64().map(|i| i as u64)))
}

fn json_i64(v: &serde_json::Value, key: &str) -> Option<i64> {
    v.get(key)
        .and_then(|x| x.as_i64().or_else(|| x.as_u64().map(|u| u as i64)))
}

fn json_str(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str().map(str::to_string))
}

/// Strip a leading UTF-8 BOM if present (PowerShell `Set-Content` often writes one).
#[must_use]
pub fn strip_utf8_bom(raw: &str) -> &str {
    raw.strip_prefix('\u{feff}').unwrap_or(raw)
}

fn explain_verdict(
    verdict: &str,
    cells: &[Phase78CellBrief],
    semantics_yellow: Option<&str>,
) -> String {
    if !verdict.eq_ignore_ascii_case("YELLOW") {
        return String::new();
    }
    let harness_starve = cells.iter().any(|c| {
        c.findings
            .iter()
            .any(|f| f.code == "HARNESS_STARVE" || f.code.starts_with("HARNESS_"))
    });
    let server_fail = cells
        .iter()
        .any(|c| c.status == "SERVER_FAIL" || c.status == "CORRECTNESS_FAIL");
    if harness_starve && !server_fail {
        "YELLOW originates from harness warnings (typically snapshot_starvation at higher N), not a demonstrated server tick or transport failure. Do not treat this as SERVER WARN."
            .into()
    } else if let Some(s) = semantics_yellow {
        s.to_string()
    } else {
        "YELLOW: WARN findings present while absolute operational thresholds remain intact.".into()
    }
}

fn missing_brief(detail: impl Into<String>) -> Phase78GateBrief {
    Phase78GateBrief {
        status: Phase78ReadStatus::Missing,
        available: false,
        read_error: Some(detail.into()),
        ..Phase78GateBrief::default()
    }
}

fn parse_error_brief(detail: impl Into<String>) -> Phase78GateBrief {
    Phase78GateBrief {
        status: Phase78ReadStatus::ParseError,
        available: false,
        read_error: Some(detail.into()),
        ..Phase78GateBrief::default()
    }
}

fn brief_from_summary(summary: RawSummary) -> Phase78GateBrief {
    let cells: Vec<Phase78CellBrief> = summary
        .cells
        .into_iter()
        .map(|c| {
            let m = &c.metrics;
            Phase78CellBrief {
                cell_id: c.cell_id,
                status: c.status,
                findings: c
                    .findings
                    .into_iter()
                    .map(|f| Phase78FindingBrief {
                        severity: f.severity,
                        code: f.code,
                        message: f.message,
                    })
                    .collect(),
                tick_p95: json_f64(m, "tick_p95"),
                tick_p99: json_f64(m, "tick_p99"),
                tick_max: json_f64(m, "tick_max"),
                tick_util: json_f64(m, "tick_util"),
                dominant: json_str(m, "dominant"),
                server_cpu_mid: json_f64(m, "server_cpu_mid_pct"),
                server_rss_mb: json_f64(m, "server_rss_mb"),
                server_rss_delta_mb: json_f64(m, "server_rss_delta_mb"),
                bytes_out_per_sec: json_f64(m, "bytes_out_per_sec"),
                writer_queue_depth_max: json_u64(m, "writer_queue_depth_max"),
                writer_queue_push_fail: json_u64(m, "writer_queue_push_fail"),
                spawn_requested: json_u64(m, "spawn_requested"),
                peak_active: json_u64(m, "peak_active"),
                attainment_pct: json_f64(m, "attainment_pct"),
                deaths_total: json_u64(m, "deaths_total"),
                snapshot_starvation: json_u64(m, "snapshot_starvation"),
                harness_exit: json_i64(m, "harness_exit"),
            }
        })
        .collect();
    let yellow = summary.semantics.as_ref().and_then(|s| s.yellow.as_deref());
    let verdict_explanation = explain_verdict(&summary.verdict, &cells, yellow);
    Phase78GateBrief {
        status: Phase78ReadStatus::Ok,
        available: true,
        read_error: None,
        verdict: summary.verdict,
        stamp: summary.stamp,
        run_root: summary.run_root,
        baseline_compared: summary.baseline_compared,
        baseline_path: summary.baseline_path,
        unit_budget_status: summary
            .unit_budget
            .map(|u| u.status)
            .unwrap_or_else(|| "—".into()),
        cells,
        verdict_explanation,
    }
}

/// Latest `gate_*` directory under `logs/load/capacity_78`.
#[must_use]
pub fn latest_phase78_gate_dir(capacity_78_root: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(capacity_78_root).ok()?;
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("gate_"))
        })
        .collect();
    dirs.sort_by(|a, b| {
        let ta = fs::metadata(a).and_then(|m| m.modified()).ok();
        let tb = fs::metadata(b).and_then(|m| m.modified()).ok();
        ta.cmp(&tb)
    });
    dirs.pop()
}

#[must_use]
pub fn capacity_78_root(workspace: &Path) -> PathBuf {
    workspace.join("logs").join("load").join("capacity_78")
}

#[must_use]
pub fn read_phase78_gate_summary(gate_dir: &Path) -> Phase78GateBrief {
    let path = gate_dir.join("phase78_gate_summary.json");
    if !path.is_file() {
        return missing_brief(format!(
            "no phase78_gate_summary.json in {}",
            gate_dir.display()
        ));
    }
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            return parse_error_brief(format!("failed to read {}: {e}", path.display()));
        }
    };
    let text = strip_utf8_bom(&raw);
    let summary = match serde_json::from_str::<RawSummary>(text) {
        Ok(s) => s,
        Err(e) => {
            return parse_error_brief(format!("failed to parse {}: {e}", path.display()));
        }
    };
    brief_from_summary(summary)
}

#[must_use]
pub fn read_latest_phase78_gate(workspace: &Path) -> Phase78GateBrief {
    let root = capacity_78_root(workspace);
    match latest_phase78_gate_dir(&root) {
        Some(dir) => read_phase78_gate_summary(&dir),
        None => missing_brief(format!("no gate_* directory under {}", root.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_dir(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("purgatory_phase78_{tag}_{stamp}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const MINIMAL_JSON: &str = r#"{
              "verdict":"YELLOW",
              "stamp":"t1",
              "run_root":"r",
              "baseline_compared":false,
              "unit_budget":{"status":"PASS"},
              "cells":[{
                "cell_id":"standard_mixed64",
                "status":"WARN",
                "findings":[{"severity":"WARN","code":"HARNESS_STARVE","message":"starve"}],
                "metrics":{"tick_p99":1.5,"tick_util":3.0,"attainment_pct":100.0,"dominant":"replication_policy"}
              }]
            }"#;

    #[test]
    fn finding_class_distinguishes_harness_from_server() {
        let h = Phase78FindingBrief {
            severity: "WARN".into(),
            code: "HARNESS_STARVE".into(),
            message: "snapshot_starvation".into(),
        };
        assert_eq!(h.class_label(), "HARNESS");
        let s = Phase78FindingBrief {
            severity: "FAIL".into(),
            code: "TICK_P99".into(),
            message: "too high".into(),
        };
        assert_eq!(s.class_label(), "SERVER");
    }

    #[test]
    fn strip_utf8_bom_removes_prefix() {
        assert_eq!(strip_utf8_bom("{\"a\":1}"), "{\"a\":1}");
        assert_eq!(strip_utf8_bom("\u{feff}{\"a\":1}"), "{\"a\":1}");
    }

    #[test]
    fn reads_minimal_summary_json_without_bom() {
        let gate = unique_dir("nobom");
        fs::write(gate.join("phase78_gate_summary.json"), MINIMAL_JSON).unwrap();
        let brief = read_phase78_gate_summary(&gate);
        let _ = fs::remove_dir_all(&gate);
        assert_eq!(brief.status, Phase78ReadStatus::Ok);
        assert!(brief.available);
        assert!(brief.read_error.is_none());
        assert_eq!(brief.verdict, "YELLOW");
        assert!(brief.verdict_explanation.contains("harness"));
        assert_eq!(brief.cells.len(), 1);
        assert_eq!(brief.cells[0].tick_p99, Some(1.5));
        assert_eq!(brief.cells[0].findings[0].class_label(), "HARNESS");
    }

    #[test]
    fn reads_minimal_summary_json_with_bom() {
        let gate = unique_dir("bom");
        let mut raw = String::from('\u{feff}');
        raw.push_str(MINIMAL_JSON);
        fs::write(gate.join("phase78_gate_summary.json"), raw).unwrap();
        let brief = read_phase78_gate_summary(&gate);
        let _ = fs::remove_dir_all(&gate);
        assert_eq!(brief.status, Phase78ReadStatus::Ok);
        assert!(brief.available);
        assert_eq!(brief.verdict, "YELLOW");
        assert_eq!(brief.cells.len(), 1);
    }

    #[test]
    fn malformed_summary_is_parse_error_not_missing() {
        let gate = unique_dir("bad");
        fs::write(gate.join("phase78_gate_summary.json"), "{not-json").unwrap();
        let brief = read_phase78_gate_summary(&gate);
        let _ = fs::remove_dir_all(&gate);
        assert_eq!(brief.status, Phase78ReadStatus::ParseError);
        assert!(!brief.available);
        assert!(
            brief
                .read_error
                .as_deref()
                .is_some_and(|e| e.contains("failed to parse"))
        );
    }

    #[test]
    fn missing_summary_file_is_missing() {
        let gate = unique_dir("nosummary");
        let brief = read_phase78_gate_summary(&gate);
        let _ = fs::remove_dir_all(&gate);
        assert_eq!(brief.status, Phase78ReadStatus::Missing);
        assert!(!brief.available);
        assert!(
            brief
                .read_error
                .as_deref()
                .is_some_and(|e| e.contains("no phase78_gate_summary.json"))
        );
    }

    #[test]
    fn latest_gate_selection_prefers_newest_mtime() {
        let root = unique_dir("latest_root");
        let older = root.join("gate_old");
        let newer = root.join("gate_new");
        fs::create_dir_all(&older).unwrap();
        fs::create_dir_all(&newer).unwrap();
        fs::write(
            older.join("phase78_gate_summary.json"),
            r#"{"verdict":"GREEN","stamp":"old","cells":[]}"#,
        )
        .unwrap();
        // Ensure newer mtime.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(
            newer.join("phase78_gate_summary.json"),
            r#"{"verdict":"INVALID","stamp":"new","cells":[]}"#,
        )
        .unwrap();
        let picked = latest_phase78_gate_dir(&root).expect("gate dir");
        assert_eq!(
            picked.file_name().and_then(|n| n.to_str()),
            Some("gate_new")
        );
        let brief = read_phase78_gate_summary(&picked);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(brief.verdict, "INVALID");
        assert_eq!(brief.status, Phase78ReadStatus::Ok);
    }

    #[test]
    fn reads_real_gate_summary_with_bom_if_present() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../logs/load/capacity_78/gate_20260902_094914");
        if !path.join("phase78_gate_summary.json").is_file() {
            return;
        }
        let brief = read_phase78_gate_summary(&path);
        assert_eq!(
            brief.status,
            Phase78ReadStatus::Ok,
            "read_error={:?}",
            brief.read_error
        );
        assert!(brief.available);
        assert_eq!(brief.verdict, "INVALID");
        assert!(!brief.cells.is_empty());
    }
}
