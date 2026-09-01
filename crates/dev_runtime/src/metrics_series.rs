//! Bounded read of `metrics.csv` for Hub charts (presentation only).
//!
//! Source of truth remains the harness file. This does not invent samples.

use std::path::Path;

/// Cap on in-memory series samples returned to the GUI.
pub const METRICS_SERIES_CAP: usize = 180;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetricsSeriesSample {
    pub elapsed_secs: f64,
    pub connected_bots: f64,
    pub tick_work_mean_ms: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetricsSeries {
    pub samples: Vec<MetricsSeriesSample>,
    pub available: bool,
}

/// Read the last `max_points` rows from `run_dir/metrics.csv`.
#[must_use]
pub fn read_metrics_series(run_dir: &Path, max_points: usize) -> MetricsSeries {
    let path = run_dir.join("metrics.csv");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return MetricsSeries::default();
    };
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let Some(header) = lines.next() else {
        return MetricsSeries::default();
    };
    let cols: Vec<&str> = header.split(',').collect();
    let idx_elapsed = col_index(&cols, "elapsed_secs");
    let idx_connected = col_index(&cols, "connected_bots");
    let idx_tick = col_index(&cols, "server_tick_work_mean_ms");
    let (Some(i_elapsed), Some(i_connected)) = (idx_elapsed, idx_connected) else {
        return MetricsSeries::default();
    };
    let rows: Vec<&str> = lines.collect();
    let start = rows.len().saturating_sub(max_points.max(1));
    let mut samples = Vec::with_capacity(rows.len().saturating_sub(start));
    for row in &rows[start..] {
        let cells: Vec<&str> = row.split(',').collect();
        let Some(elapsed) = cells.get(i_elapsed).and_then(|s| s.parse().ok()) else {
            continue;
        };
        let connected = cells
            .get(i_connected)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        let tick = idx_tick.and_then(|i| cells.get(i).and_then(|s| parse_opt_f64(s)));
        samples.push(MetricsSeriesSample {
            elapsed_secs: elapsed,
            connected_bots: connected,
            tick_work_mean_ms: tick,
        });
    }
    MetricsSeries {
        available: !samples.is_empty(),
        samples,
    }
}

fn col_index(cols: &[&str], name: &str) -> Option<usize> {
    cols.iter().position(|c| *c == name)
}

fn parse_opt_f64(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn missing_csv_is_empty() {
        let dir =
            std::env::temp_dir().join(format!("purgatory-metrics-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let series = read_metrics_series(&dir, 10);
        assert!(!series.available);
        assert!(series.samples.is_empty());
    }

    #[test]
    fn reads_bounded_tail() {
        let dir =
            std::env::temp_dir().join(format!("purgatory-metrics-tail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut csv = String::from("elapsed_secs,connected_bots,server_tick_work_mean_ms\n");
        for i in 0..20 {
            csv.push_str(&format!("{i},{i},1.{i}\n"));
        }
        fs::write(dir.join("metrics.csv"), csv).unwrap();
        let series = read_metrics_series(&dir, 5);
        assert!(series.available);
        assert_eq!(series.samples.len(), 5);
        assert_eq!(series.samples[0].elapsed_secs, 15.0);
        assert_eq!(series.samples[4].connected_bots, 19.0);
        assert_eq!(series.samples[4].tick_work_mean_ms, Some(1.19));
    }
}
