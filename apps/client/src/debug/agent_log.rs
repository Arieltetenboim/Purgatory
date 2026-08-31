//! Temporary NDJSON debug ingest for Phase 6G AOI/jitter investigation.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const SESSION: &str = "679b99";
const PATHS: &[&str] = &[
    r"C:\Users\Ariel\OneDrive\Desktop\Purgatory\1\debug-679b99.log",
    r"C:\Users\Ariel\OneDrive\Desktop\Purgatory\1\logs\dev-tools\debug-679b99.log",
];

static LAST_MS: [AtomicU64; 4] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];
static ANNOUNCED: AtomicBool = AtomicBool::new(false);

#[must_use]
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// True when `slot` has not emitted within `min_ms`.
#[must_use]
pub fn should_emit(slot: usize, min_ms: u64) -> bool {
    let Some(cell) = LAST_MS.get(slot) else {
        return true;
    };
    let now = now_ms();
    let prev = cell.load(Ordering::Relaxed);
    if now.saturating_sub(prev) < min_ms {
        return false;
    }
    cell.store(now, Ordering::Relaxed);
    true
}

pub fn emit(hypothesis_id: &str, location: &str, message: &str, data_json: &str) {
    let ts = now_ms();
    let line = format!(
        "{{\"sessionId\":\"{SESSION}\",\"runId\":\"post-fix\",\"hypothesisId\":\"{hypothesis_id}\",\"location\":\"{location}\",\"message\":\"{message}\",\"timestamp\":{ts},\"data\":{data_json}}}\n"
    );
    let mut wrote = false;
    for path in PATHS {
        if let Some(parent) = Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path)
            && f.write_all(line.as_bytes()).is_ok()
        {
            wrote = true;
        }
    }
    if wrote && !ANNOUNCED.swap(true, Ordering::Relaxed) {
        println!("PURGATORY agent_log writing {}", PATHS[0]);
    }
}
