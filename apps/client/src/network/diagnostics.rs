//! Bounded network diagnostic history and session RTT statistics.

use std::time::{Duration, Instant};

use super::failure::NetworkFailureKind;
use super::state::ConnectionState;

pub const NETWORK_HISTORY_CAP: usize = 48;
const EWMA_ALPHA: f64 = 0.25;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagKind {
    Connecting,
    Handshaking,
    Connected,
    Rejected,
    Disconnected,
    StaleIgnored,
}

impl DiagKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connecting => "Connecting",
            Self::Handshaking => "Handshaking",
            Self::Connected => "Connected",
            Self::Rejected => "Rejected",
            Self::Disconnected => "Disconnected",
            Self::StaleIgnored => "StaleIgnored",
        }
    }
}

/// Compact diagnostic record. No heap strings.
#[derive(Clone, Copy, Debug)]
pub struct NetworkDiagEvent {
    pub seq: u64,
    pub elapsed_ms: u32,
    pub attempt_id: u64,
    pub connection_id: u64,
    pub kind: DiagKind,
    pub state: ConnectionState,
    pub failure: Option<NetworkFailureKind>,
    pub retryable: bool,
}

impl NetworkDiagEvent {
    #[must_use]
    pub fn summary(self) -> String {
        let attempt = if self.attempt_id == 0 {
            "—".to_string()
        } else {
            format!("Attempt {}", self.attempt_id)
        };
        let core = match self.kind {
            DiagKind::Connected if self.connection_id != 0 => {
                format!("{attempt} | Connected | CID {}", self.connection_id)
            }
            DiagKind::Disconnected | DiagKind::Rejected => {
                let label = self
                    .failure
                    .map(NetworkFailureKind::debug_label)
                    .unwrap_or(self.kind.as_str());
                let retry = if self.retryable { "yes" } else { "no" };
                format!("{attempt} | {label} | retry={retry}")
            }
            other => format!("{attempt} | {}", other.as_str()),
        };
        format!(
            "#{} {}ms | {} | {core}",
            self.seq,
            self.elapsed_ms,
            self.state.as_str()
        )
    }
}

#[derive(Clone, Debug)]
pub struct NetworkHistory {
    buf: [Option<NetworkDiagEvent>; NETWORK_HISTORY_CAP],
    next: usize,
    len: usize,
    seq: u64,
    started: Instant,
}

impl NetworkHistory {
    #[must_use]
    pub fn new() -> Self {
        Self {
            buf: [None; NETWORK_HISTORY_CAP],
            next: 0,
            len: 0,
            seq: 0,
            started: Instant::now(),
        }
    }

    pub fn clear(&mut self) {
        self.buf = [None; NETWORK_HISTORY_CAP];
        self.next = 0;
        self.len = 0;
        self.seq = 0;
        self.started = Instant::now();
    }

    pub fn push(
        &mut self,
        kind: DiagKind,
        attempt_id: u64,
        connection_id: u64,
        state: ConnectionState,
        failure: Option<NetworkFailureKind>,
    ) {
        self.seq = self.seq.saturating_add(1);
        let elapsed = self.started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
        let event = NetworkDiagEvent {
            seq: self.seq,
            elapsed_ms: elapsed,
            attempt_id,
            connection_id,
            kind,
            state,
            failure,
            retryable: failure.is_some_and(NetworkFailureKind::retryable),
        };
        self.buf[self.next] = Some(event);
        self.next = (self.next + 1) % NETWORK_HISTORY_CAP;
        self.len = (self.len + 1).min(NETWORK_HISTORY_CAP);
    }

    pub fn iter_newest_first(&self) -> impl Iterator<Item = NetworkDiagEvent> + '_ {
        let len = self.len;
        let next = self.next;
        (0..len).filter_map(move |i| {
            let idx = (next + NETWORK_HISTORY_CAP - 1 - i) % NETWORK_HISTORY_CAP;
            self.buf[idx]
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn cap() -> usize {
        NETWORK_HISTORY_CAP
    }

    #[must_use]
    pub fn snapshot_newest_first(&self) -> [Option<NetworkDiagEvent>; NETWORK_HISTORY_CAP] {
        let mut out = [None; NETWORK_HISTORY_CAP];
        for (i, ev) in self.iter_newest_first().enumerate() {
            out[i] = Some(ev);
        }
        out
    }
}

impl Default for NetworkHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RttStats {
    pub latest: Option<Duration>,
    pub min: Option<Duration>,
    pub max: Option<Duration>,
    pub ewma: Option<Duration>,
}

impl RttStats {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn observe(&mut self, sample: Duration) {
        self.latest = Some(sample);
        self.min = Some(self.min.map_or(sample, |m| m.min(sample)));
        self.max = Some(self.max.map_or(sample, |m| m.max(sample)));
        self.ewma = Some(match self.ewma {
            None => sample,
            Some(prev) => {
                let mixed =
                    EWMA_ALPHA * sample.as_secs_f64() + (1.0 - EWMA_ALPHA) * prev.as_secs_f64();
                Duration::from_secs_f64(mixed.max(0.0))
            }
        });
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NetworkCounters {
    pub lifecycle_events: u64,
    pub telemetry_events: u64,
    pub dropped_telemetry: u64,
    pub stale_events_ignored: u64,
    pub reconnect_attempts: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_bounded_and_keeps_newest() {
        let mut hist = NetworkHistory::new();
        for i in 1..=80 {
            hist.push(
                DiagKind::Connecting,
                i,
                0,
                ConnectionState::Connecting,
                None,
            );
        }
        assert_eq!(hist.len(), NETWORK_HISTORY_CAP);
        let newest = hist.iter_newest_first().next().expect("newest");
        assert_eq!(newest.attempt_id, 80);
        let oldest = hist.iter_newest_first().last().expect("oldest");
        assert_eq!(oldest.attempt_id, 80 - NETWORK_HISTORY_CAP as u64 + 1);
    }

    #[test]
    fn rtt_stats_reset_between_sessions() {
        let mut stats = RttStats::default();
        stats.observe(Duration::from_millis(10));
        stats.observe(Duration::from_millis(4));
        assert_eq!(stats.min, Some(Duration::from_millis(4)));
        assert_eq!(stats.max, Some(Duration::from_millis(10)));
        assert!(stats.ewma.is_some());
        stats.reset();
        assert!(stats.latest.is_none());
        assert!(stats.min.is_none());
        assert!(stats.max.is_none());
        assert!(stats.ewma.is_none());
    }
}
