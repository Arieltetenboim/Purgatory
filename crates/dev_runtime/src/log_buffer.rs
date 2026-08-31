use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{ACTIVITY_LOG_CAP, ACTIVITY_VIEW_LINES, PUMP_QUEUE_CAP, UI_LOG_DRAIN};

#[derive(Clone, Debug)]
pub struct LogLine {
    pub text: String,
}

pub struct ActivityLog {
    lines: VecDeque<String>,
}

impl ActivityLog {
    pub fn new() -> Self {
        Self {
            lines: VecDeque::new(),
        }
    }

    pub fn push(&mut self, line: String) {
        let line = strip_bell(&line);
        if self.lines.len() >= ACTIVITY_LOG_CAP {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    #[must_use]
    pub fn view_lines(&self) -> Vec<String> {
        let skip = self.lines.len().saturating_sub(ACTIVITY_VIEW_LINES);
        self.lines.iter().skip(skip).cloned().collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

impl Default for ActivityLog {
    fn default() -> Self {
        Self::new()
    }
}

/// Cross-thread stdout pump → UI. Drop-oldest at [`PUMP_QUEUE_CAP`].
#[derive(Clone)]
pub struct IncomingLog {
    inner: Arc<Mutex<VecDeque<(String, String)>>>,
}

impl IncomingLog {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn push(&self, name: &str, line: &str) {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        while q.len() >= PUMP_QUEUE_CAP {
            q.pop_front();
        }
        q.push_back((name.to_string(), strip_bell(line)));
    }

    pub fn pending_count(&self) -> usize {
        self.inner.lock().map(|q| q.len()).unwrap_or(0)
    }

    pub fn drain(&self, max: usize) -> Vec<(String, String)> {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let n = max.min(q.len());
        q.drain(..n).collect()
    }
}

impl Default for IncomingLog {
    fn default() -> Self {
        Self::new()
    }
}

pub fn drain_into_activity(incoming: &IncomingLog, activity: &mut ActivityLog) {
    for (name, line) in incoming.drain(UI_LOG_DRAIN) {
        if name == "probe" {
            continue;
        }
        activity.push(stamp_line(&format!("{name} | {line}")));
    }
}

pub fn stamp_line(message: &str) -> String {
    format!("{}  {}", utc_hms(), message)
}

pub fn utc_hms() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let tod = secs % 86400;
    format!("{:02}:{:02}:{:02}", tod / 3600, (tod / 60) % 60, tod % 60)
}

pub fn strip_bell(s: &str) -> String {
    s.replace('\u{0007}', "")
}

pub fn ensure_log_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))
}

pub fn append_file_line(dir: &Path, name: &str, line: &str) -> Result<(), String> {
    ensure_log_dir(dir)?;
    let path = dir.join(name);
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    writeln!(f, "{line}").map_err(|e| format!("write {}: {e}", path.display()))
}

pub fn log_file_path(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{stem}.log"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_ring_is_bounded() {
        let mut log = ActivityLog::new();
        for i in 0..(ACTIVITY_LOG_CAP + 50) {
            log.push(format!("line {i}"));
        }
        assert_eq!(log.len(), ACTIVITY_LOG_CAP);
        let view = log.view_lines();
        assert_eq!(view.len(), ACTIVITY_VIEW_LINES);
        assert!(
            view.last()
                .unwrap()
                .contains(&format!("line {}", ACTIVITY_LOG_CAP + 49))
        );
    }

    #[test]
    fn incoming_queue_drops_oldest() {
        let q = IncomingLog::new();
        for i in 0..(PUMP_QUEUE_CAP + 10) {
            q.push("server", &format!("{i}"));
        }
        assert_eq!(q.pending_count(), PUMP_QUEUE_CAP);
        let drained = q.drain(UI_LOG_DRAIN);
        assert_eq!(drained.len(), UI_LOG_DRAIN);
        assert_eq!(drained[0].1, "10");
    }
}
