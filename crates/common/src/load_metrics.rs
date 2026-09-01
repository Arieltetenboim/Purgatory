//! Shared load-test metrics DTO (Phase 5.7).
//!
//! Used by the development UDP metrics export on the server and by the
//! headless load harness. Not part of gameplay protocol v4.
//!
//! Schema **4** adds monotonic execution totals so Mixed validation can prove
//! scheduler/action/effect/spawn/cadence work ran even when 1 Hz gauge samples
//! of queue depth / active count stay zero. Schema 3 gauges remain. Do not
//! put 6G.2 domain timings on this datagram (those stay run artifacts).
//!
//! Remaining schema candidates (not added): `effects_active` as a live gauge,
//! persistence save counters, occupancy, non-player entity count. Bump only
//! if exporting them materially improves validation, and update the
//! datagram-size test together. Schema 4 needs a 4096-byte localhost cap
//! (was 2048) so execution-total field names fit beside existing gauges.

use serde::{Deserialize, Serialize};

/// Schema version for [`LoadMetricsV1`]. Bump when fields change meaning.
pub const LOAD_METRICS_SCHEMA_VERSION: u32 = 4;

/// Request magic bytes: `PURGSTAT` (8) + version u8.
pub const METRICS_REQUEST_MAGIC: &[u8; 8] = b"PURGSTAT";
pub const METRICS_REQUEST_VERSION: u8 = 1;
pub const METRICS_MAX_REQUEST_BYTES: usize = 32;
pub const METRICS_MAX_DATAGRAM_BYTES: usize = 4096;
pub const DEFAULT_METRICS_PORT: u16 = 5002;

/// Flat numeric snapshot exported at ~1 Hz for load testing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LoadMetricsV1 {
    pub metrics_schema_version: u32,
    pub admission_cap: u64,
    pub max_entities_per_snapshot: u64,
    pub active_sessions: u64,
    pub active_player_entities: u64,
    pub peak_sessions: u64,
    pub peak_player_entities: u64,
    pub total_accepted: u64,
    pub total_rejected: u64,
    pub admission_refused: u64,
    pub clean_disconnect: u64,
    pub transport_loss: u64,
    pub session_created: u64,
    pub session_destroyed: u64,
    pub player_entity_spawned: u64,
    pub player_entity_despawned: u64,
    pub duplicate_session_detected: u64,
    pub lifecycle_handoff_dropped: u64,
    pub input_received: u64,
    pub input_accepted: u64,
    pub input_duplicate: u64,
    pub input_stale: u64,
    pub input_queue_overflow: u64,
    pub input_handoff_dropped: u64,
    pub input_rate_limited: u64,
    pub input_queue_current: u64,
    pub input_queue_max: u64,
    pub session_queue_max: u64,
    pub snapshots_built: u64,
    pub snapshot_build_count: u64,
    pub snapshots_sent: u64,
    pub snapshot_send_failed: u64,
    pub snapshot_encode_failed: u64,
    pub snapshot_sequence: u64,
    pub last_snapshot_entities: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub tick_count: u64,
    pub tick_overrun_count: u64,
    pub tick_work_mean_ms: f64,
    pub tick_work_p50_ms: f64,
    pub tick_work_p95_ms: f64,
    pub tick_work_p99_ms: f64,
    pub tick_work_max_ms: f64,
    pub scheduler_lateness_p95_ms: f64,
    pub scheduler_lateness_max_ms: f64,
    pub catch_up_ticks: u64,
    pub discarded_ns: u64,
    pub snapshot_build_time_max_ms: f64,
    pub snapshot_encode_time_max_ms: f64,
    pub snapshot_size_max_bytes: u64,
    /// Working set bytes when sampled on Windows; 0 if unavailable.
    pub memory_working_set_bytes: u64,
    pub memory_working_set_peak_bytes: u64,
    /// AOI Enter records committed to the writer queue (not client ACK).
    #[serde(default)]
    pub aoi_enters: u64,
    #[serde(default)]
    pub aoi_leaves: u64,
    #[serde(default)]
    pub aoi_updates: u64,
    #[serde(default)]
    pub aoi_churn_reentry: u64,
    /// Encoded frame bytes committed (numerator for bytes/update).
    #[serde(default)]
    pub aoi_update_bytes: u64,
    #[serde(default)]
    pub oldest_pending_ticks: u64,
    #[serde(default)]
    pub max_deferred_ticks: u64,
    #[serde(default)]
    pub replication_queue_depth_max: u64,
    #[serde(default)]
    pub scheduler_queued: u64,
    #[serde(default)]
    pub scheduler_due_critical: u64,
    #[serde(default)]
    pub scheduler_due_deferred: u64,
    #[serde(default)]
    pub scheduler_critical_ceiling_hits: u64,
    #[serde(default)]
    pub scheduler_deferred_exhausted: u64,
    #[serde(default)]
    pub actions_active: u64,
    #[serde(default)]
    pub events_produced: u64,
    #[serde(default)]
    pub events_processed: u64,
    #[serde(default)]
    pub spawn_queue_depth: u64,
    #[serde(default)]
    pub cadence_due: u64,
    #[serde(default)]
    pub command_rejects_gate: u64,
    #[serde(default)]
    pub command_rejects_other: u64,
    #[serde(default)]
    pub domain_rev_advances: u64,
    #[serde(default)]
    pub observer_pending_updates: u64,
    #[serde(default)]
    pub observer_pending_enters: u64,
    #[serde(default)]
    pub cadence_deferred_updates: u64,
    #[serde(default)]
    pub scheduler_scheduled_total: u64,
    #[serde(default)]
    pub scheduler_cancelled_total: u64,
    #[serde(default)]
    pub scheduler_critical_executed_total: u64,
    #[serde(default)]
    pub scheduler_deferred_executed_total: u64,
    #[serde(default)]
    pub actions_started_total: u64,
    #[serde(default)]
    pub actions_completed_total: u64,
    #[serde(default)]
    pub effects_applied_total: u64,
    #[serde(default)]
    pub effects_expired_total: u64,
    #[serde(default)]
    pub spawn_requests_total: u64,
    #[serde(default)]
    pub spawns_completed_total: u64,
    #[serde(default)]
    pub despawns_completed_total: u64,
    #[serde(default)]
    pub cadence_executions_total: u64,
    #[serde(default)]
    pub entities_spawned_total: u64,
}

impl LoadMetricsV1 {
    #[must_use]
    pub fn with_schema() -> Self {
        Self {
            metrics_schema_version: LOAD_METRICS_SCHEMA_VERSION,
            ..Self::default()
        }
    }
}

/// Build a request datagram (magic + version).
#[must_use]
pub fn metrics_request_datagram() -> [u8; 9] {
    let mut buf = [0u8; 9];
    buf[..8].copy_from_slice(METRICS_REQUEST_MAGIC);
    buf[8] = METRICS_REQUEST_VERSION;
    buf
}

/// Returns true if `bytes` is a well-formed metrics poll request.
#[must_use]
pub fn is_metrics_request(bytes: &[u8]) -> bool {
    if bytes.len() > METRICS_MAX_REQUEST_BYTES || bytes.len() < 9 {
        return false;
    }
    bytes[..8] == *METRICS_REQUEST_MAGIC && bytes[8] == METRICS_REQUEST_VERSION
}

/// Encode a response: magic(8) + version(1) + body_len(u16 LE) + JSON.
pub fn encode_metrics_response(metrics: &LoadMetricsV1) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec(metrics).map_err(|e| e.to_string())?;
    let len = u16::try_from(json.len()).map_err(|_| "metrics json too large".to_string())?;
    let total = 8 + 1 + 2 + json.len();
    if total > METRICS_MAX_DATAGRAM_BYTES {
        return Err(format!(
            "metrics datagram {total} exceeds max {}",
            METRICS_MAX_DATAGRAM_BYTES
        ));
    }
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(METRICS_REQUEST_MAGIC);
    out.push(METRICS_REQUEST_VERSION);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&json);
    Ok(out)
}

/// Decode a metrics response. Returns `None` for malformed / wrong version.
#[must_use]
pub fn decode_metrics_response(bytes: &[u8]) -> Option<LoadMetricsV1> {
    if bytes.len() < 11 {
        return None;
    }
    if bytes[..8] != *METRICS_REQUEST_MAGIC || bytes[8] != METRICS_REQUEST_VERSION {
        return None;
    }
    let len = u16::from_le_bytes([bytes[9], bytes[10]]) as usize;
    if bytes.len() != 11 + len || bytes.len() > METRICS_MAX_DATAGRAM_BYTES {
        return None;
    }
    serde_json::from_slice(&bytes[11..]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip_check() {
        let req = metrics_request_datagram();
        assert!(is_metrics_request(&req));
        assert!(!is_metrics_request(&[]));
        assert!(!is_metrics_request(&[0; 40]));
        let mut bad = req;
        bad[8] = 99;
        assert!(!is_metrics_request(&bad));
    }

    #[test]
    fn response_roundtrip() {
        let mut m = LoadMetricsV1::with_schema();
        m.active_sessions = 12;
        m.tick_work_p95_ms = 4.5;
        m.max_entities_per_snapshot = 256;
        let encoded = encode_metrics_response(&m).expect("encode");
        assert!(encoded.len() <= METRICS_MAX_DATAGRAM_BYTES);
        let decoded = decode_metrics_response(&encoded).expect("decode");
        assert_eq!(decoded.active_sessions, 12);
        assert!((decoded.tick_work_p95_ms - 4.5).abs() < 1e-9);
        assert_eq!(decoded.max_entities_per_snapshot, 256);
    }

    #[test]
    fn malformed_response_is_none() {
        assert!(decode_metrics_response(b"garbage").is_none());
        assert!(decode_metrics_response(&[0; 11]).is_none());
    }

    #[test]
    fn schema_default_fits_datagram() {
        let encoded = encode_metrics_response(&LoadMetricsV1::with_schema()).expect("encode");
        assert!(
            encoded.len() <= METRICS_MAX_DATAGRAM_BYTES,
            "datagram {} exceeds max {}",
            encoded.len(),
            METRICS_MAX_DATAGRAM_BYTES
        );
        assert_eq!(
            decode_metrics_response(&encoded)
                .expect("decode")
                .metrics_schema_version,
            LOAD_METRICS_SCHEMA_VERSION
        );
    }

    #[test]
    fn schema4_populated_totals_fit_datagram() {
        let mut m = LoadMetricsV1::with_schema();
        m.scheduler_scheduled_total = 10_000_000;
        m.scheduler_cancelled_total = 1_000_000;
        m.scheduler_critical_executed_total = 5_000_000;
        m.scheduler_deferred_executed_total = 5_000_000;
        m.actions_started_total = 100_000;
        m.actions_completed_total = 100_000;
        m.effects_applied_total = 100_000;
        m.effects_expired_total = 100_000;
        m.spawn_requests_total = 50_000;
        m.spawns_completed_total = 50_000;
        m.despawns_completed_total = 50_000;
        m.cadence_executions_total = 2_000_000;
        m.entities_spawned_total = 200_000;
        m.events_produced = 8_000_000;
        m.events_processed = 8_000_000;
        let encoded = encode_metrics_response(&m).expect("encode");
        assert!(
            encoded.len() <= METRICS_MAX_DATAGRAM_BYTES,
            "populated datagram {} exceeds max {}",
            encoded.len(),
            METRICS_MAX_DATAGRAM_BYTES
        );
    }

    #[test]
    fn schema3_json_decodes_new_totals_as_zero() {
        let mut legacy = LoadMetricsV1::with_schema();
        legacy.metrics_schema_version = 3;
        legacy.scheduler_scheduled_total = 0;
        let json = serde_json::to_vec(&legacy).expect("json");
        // Drop schema-4 keys so this is a true schema-3 body.
        let mut value: serde_json::Value = serde_json::from_slice(&json).expect("parse");
        if let serde_json::Value::Object(map) = &mut value {
            for key in [
                "scheduler_scheduled_total",
                "scheduler_cancelled_total",
                "scheduler_critical_executed_total",
                "scheduler_deferred_executed_total",
                "actions_started_total",
                "actions_completed_total",
                "effects_applied_total",
                "effects_expired_total",
                "spawn_requests_total",
                "spawns_completed_total",
                "despawns_completed_total",
                "cadence_executions_total",
                "entities_spawned_total",
            ] {
                map.remove(key);
            }
        }
        let stripped = serde_json::to_vec(&value).expect("strip");
        let decoded: LoadMetricsV1 = serde_json::from_slice(&stripped).expect("decode");
        assert_eq!(decoded.scheduler_scheduled_total, 0);
        assert_eq!(decoded.entities_spawned_total, 0);
    }
}
