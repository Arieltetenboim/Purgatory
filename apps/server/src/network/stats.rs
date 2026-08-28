//! Server-side connection counters. Fixed-size atomics only.

use std::sync::atomic::{AtomicU64, Ordering};

use purgatory_protocol::DisconnectReasonCode;

#[derive(Debug, Default)]
pub struct ServerNetStats {
    pub active_handshakes: AtomicU64,
    pub total_accepted: AtomicU64,
    pub total_rejected: AtomicU64,
    pub version_mismatch: AtomicU64,
    pub malformed: AtomicU64,
    pub handshake_timeout: AtomicU64,
    pub unexpected: AtomicU64,
    pub clean_disconnect: AtomicU64,
    pub transport_loss: AtomicU64,
    pub rejected_oversized: AtomicU64,
    pub admission_refused: AtomicU64,
    pub rate_limited: AtomicU64,
    pub max_inflight: AtomicU64,
    pub max_handshakes: AtomicU64,
    pub input_received: AtomicU64,
    pub input_accepted: AtomicU64,
    pub input_duplicate: AtomicU64,
    pub input_stale: AtomicU64,
    pub input_invalid: AtomicU64,
    pub input_rate_limited: AtomicU64,
    pub input_handoff_dropped: AtomicU64,
    pub snapshots_built: AtomicU64,
    pub snapshot_sequence: AtomicU64,
    pub last_snapshot_entities: AtomicU64,
    pub snapshot_send_failed: AtomicU64,
    pub snapshots_sent: AtomicU64,
}

impl ServerNetStats {
    /// Enters a handshake and updates the concurrency high-water mark.
    pub fn enter_handshake(&self) {
        let now = self
            .active_handshakes
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        self.max_handshakes.fetch_max(now, Ordering::Relaxed);
    }

    pub fn leave_handshake(&self) {
        self.active_handshakes.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn note_reject(&self, code: DisconnectReasonCode) {
        self.total_rejected.fetch_add(1, Ordering::Relaxed);
        match code {
            DisconnectReasonCode::VersionMismatch => {
                self.version_mismatch.fetch_add(1, Ordering::Relaxed);
            }
            DisconnectReasonCode::Malformed => {
                self.malformed.fetch_add(1, Ordering::Relaxed);
            }
            DisconnectReasonCode::HandshakeTimeout => {
                self.handshake_timeout.fetch_add(1, Ordering::Relaxed);
            }
            DisconnectReasonCode::UnexpectedMessage => {
                self.unexpected.fetch_add(1, Ordering::Relaxed);
            }
            DisconnectReasonCode::ServerShutdown => {}
        }
    }

    /// Active gauges (`sessions`, `inflight`, `handshakes`) return to baseline
    /// after churn. Every other field is cumulative and only grows.
    pub fn summary(&self, active_sessions: usize, inflight: u64, peak_sessions: usize) -> String {
        format!(
            "sessions={active_sessions} inflight={inflight} handshakes={} accepted={} rejected={} mismatch={} malformed={} oversized={} hs_timeout={} unexpected={} admission_refused={} rate_limited={} clean_dc={} transport_loss={} max_sessions={peak_sessions} max_inflight={} max_handshakes={} input_rx={} input_ok={} input_dup={} input_stale={} input_bad={} input_rl={} input_drop={} snap_built={} snap_seq={} snap_ents={} snap_fail={} snap_sent={}",
            self.active_handshakes.load(Ordering::Relaxed),
            self.total_accepted.load(Ordering::Relaxed),
            self.total_rejected.load(Ordering::Relaxed),
            self.version_mismatch.load(Ordering::Relaxed),
            self.malformed.load(Ordering::Relaxed),
            self.rejected_oversized.load(Ordering::Relaxed),
            self.handshake_timeout.load(Ordering::Relaxed),
            self.unexpected.load(Ordering::Relaxed),
            self.admission_refused.load(Ordering::Relaxed),
            self.rate_limited.load(Ordering::Relaxed),
            self.clean_disconnect.load(Ordering::Relaxed),
            self.transport_loss.load(Ordering::Relaxed),
            self.max_inflight.load(Ordering::Relaxed),
            self.max_handshakes.load(Ordering::Relaxed),
            self.input_received.load(Ordering::Relaxed),
            self.input_accepted.load(Ordering::Relaxed),
            self.input_duplicate.load(Ordering::Relaxed),
            self.input_stale.load(Ordering::Relaxed),
            self.input_invalid.load(Ordering::Relaxed),
            self.input_rate_limited.load(Ordering::Relaxed),
            self.input_handoff_dropped.load(Ordering::Relaxed),
            self.snapshots_built.load(Ordering::Relaxed),
            self.snapshot_sequence.load(Ordering::Relaxed),
            self.last_snapshot_entities.load(Ordering::Relaxed),
            self.snapshot_send_failed.load(Ordering::Relaxed),
            self.snapshots_sent.load(Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_gauge_returns_to_zero_and_keeps_peak() {
        let stats = ServerNetStats::default();
        stats.enter_handshake();
        stats.enter_handshake();
        assert_eq!(stats.active_handshakes.load(Ordering::Relaxed), 2);
        stats.leave_handshake();
        stats.leave_handshake();
        assert_eq!(stats.active_handshakes.load(Ordering::Relaxed), 0);
        assert_eq!(stats.max_handshakes.load(Ordering::Relaxed), 2);
        stats.enter_handshake();
        stats.leave_handshake();
        assert_eq!(stats.max_handshakes.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn summary_reports_gauges_and_peaks() {
        let stats = ServerNetStats::default();
        stats.enter_handshake();
        let line = stats.summary(1, 2, 3);
        assert!(line.contains("sessions=1"));
        assert!(line.contains("inflight=2"));
        assert!(line.contains("max_sessions=3"));
        assert!(line.contains("max_handshakes=1"));
    }
}
