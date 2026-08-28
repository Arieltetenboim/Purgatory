//! Server-side abuse / admission policy. Not client-configurable.
//!
//! QUIC encryption is not client honesty. Every remote peer is untrusted.

use std::time::{Duration, Instant};

use purgatory_protocol::{
    HANDSHAKE_TIMEOUT, MAX_CONTROL_MESSAGE_BYTES, MAX_DATAGRAM_BYTES, MAX_LABEL_BYTES,
};

/// Concurrent handshake+session tasks. Excess `Incoming` is refused.
/// Admission/concurrency safety, not future MMO player capacity.
pub const MAX_INFLIGHT_CONNECTION_TASKS: usize = 32;

/// Development abuse limits. Wire sizes come from `purgatory-protocol`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkAbuseConfig {
    pub max_control_frame_bytes: u32,
    pub max_datagram_bytes: usize,
    pub max_label_bytes: usize,
    pub handshake_timeout: Duration,
    pub max_inflight_connection_tasks: usize,
    pub max_bidi_streams: u32,
    pub max_uni_streams: u32,
    pub malformed_control_budget: u32,
    pub invalid_datagram_budget: u32,
    pub control_messages_per_window: u32,
    pub control_rate_window: Duration,
    pub rate_drops_before_disconnect: u32,
    pub input_messages_per_window: u32,
    pub input_rate_window: Duration,
    pub input_drops_before_disconnect: u32,
}

impl NetworkAbuseConfig {
    /// Local development defaults. Not a production DDoS profile.
    pub const DEV: Self = Self {
        max_control_frame_bytes: MAX_CONTROL_MESSAGE_BYTES,
        max_datagram_bytes: MAX_DATAGRAM_BYTES,
        max_label_bytes: MAX_LABEL_BYTES,
        handshake_timeout: HANDSHAKE_TIMEOUT,
        max_inflight_connection_tasks: MAX_INFLIGHT_CONNECTION_TASKS,
        max_bidi_streams: 1,
        max_uni_streams: 0,
        malformed_control_budget: 32,
        invalid_datagram_budget: 16,
        control_messages_per_window: 8,
        control_rate_window: Duration::from_secs(1),
        rate_drops_before_disconnect: 16,
        // Gameplay input is a per-tick command stream (30 Hz) plus catch-up.
        // 128/s covers one command per tick plus a one-second hitch burst.
        // Persistent floods are dropped, then disconnected.
        input_messages_per_window: 128,
        input_rate_window: Duration::from_secs(1),
        input_drops_before_disconnect: 256,
    };
}

impl Default for NetworkAbuseConfig {
    fn default() -> Self {
        Self::DEV
    }
}

/// Per-connection counters used by policy. Fixed-size integers only.
#[derive(Clone, Copy, Debug, Default)]
pub struct ConnectionAbuse {
    pub malformed_messages: u32,
    pub invalid_datagrams: u32,
}

impl ConnectionAbuse {
    pub fn note_malformed(&mut self, budget: u32) -> bool {
        self.malformed_messages = self.malformed_messages.saturating_add(1);
        self.malformed_messages >= budget
    }

    pub fn note_invalid_datagram(&mut self, budget: u32) -> bool {
        self.invalid_datagrams = self.invalid_datagrams.saturating_add(1);
        self.invalid_datagrams >= budget
    }
}

/// Control-message rate limiter. Server `Instant` only. No per-packet heap.
#[derive(Clone, Copy, Debug)]
pub struct ControlRateLimit {
    window_start: Instant,
    count: u32,
    drops: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RateDecision {
    Allow,
    Drop,
    Disconnect,
}

impl ControlRateLimit {
    #[must_use]
    pub fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            count: 0,
            drops: 0,
        }
    }

    pub fn note(&mut self, now: Instant, cfg: NetworkAbuseConfig) -> RateDecision {
        self.note_window(
            now,
            cfg.control_messages_per_window,
            cfg.control_rate_window,
            cfg.rate_drops_before_disconnect,
        )
    }

    pub fn note_input(&mut self, now: Instant, cfg: NetworkAbuseConfig) -> RateDecision {
        self.note_window(
            now,
            cfg.input_messages_per_window,
            cfg.input_rate_window,
            cfg.input_drops_before_disconnect,
        )
    }

    fn note_window(
        &mut self,
        now: Instant,
        per_window: u32,
        window: Duration,
        disconnect_after: u32,
    ) -> RateDecision {
        if now.saturating_duration_since(self.window_start) >= window {
            self.window_start = now;
            self.count = 0;
        }
        self.count = self.count.saturating_add(1);
        if self.count <= per_window {
            RateDecision::Allow
        } else {
            self.drops = self.drops.saturating_add(1);
            if self.drops >= disconnect_after {
                RateDecision::Disconnect
            } else {
                RateDecision::Drop
            }
        }
    }
}

/// Bound peer-provided text for logs. No multiline/control spam.
#[must_use]
pub fn sanitize_log_text(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            }
        })
        .take(MAX_LABEL_BYTES)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_allows_under_threshold() {
        let cfg = NetworkAbuseConfig::DEV;
        let t0 = Instant::now();
        let mut lim = ControlRateLimit::new(t0);
        for _ in 0..cfg.control_messages_per_window {
            assert_eq!(lim.note(t0, cfg), RateDecision::Allow);
        }
    }

    #[test]
    fn rate_limit_drops_then_disconnects() {
        let mut cfg = NetworkAbuseConfig::DEV;
        cfg.control_messages_per_window = 2;
        cfg.rate_drops_before_disconnect = 3;
        let t0 = Instant::now();
        let mut lim = ControlRateLimit::new(t0);
        assert_eq!(lim.note(t0, cfg), RateDecision::Allow);
        assert_eq!(lim.note(t0, cfg), RateDecision::Allow);
        assert_eq!(lim.note(t0, cfg), RateDecision::Drop);
        assert_eq!(lim.note(t0, cfg), RateDecision::Drop);
        assert_eq!(lim.note(t0, cfg), RateDecision::Disconnect);
    }

    #[test]
    fn rate_limit_window_resets_count() {
        let mut cfg = NetworkAbuseConfig::DEV;
        cfg.control_messages_per_window = 1;
        cfg.control_rate_window = Duration::from_millis(50);
        cfg.rate_drops_before_disconnect = 100;
        let t0 = Instant::now();
        let mut lim = ControlRateLimit::new(t0);
        assert_eq!(lim.note(t0, cfg), RateDecision::Allow);
        assert_eq!(lim.note(t0, cfg), RateDecision::Drop);
        let t1 = t0 + Duration::from_millis(80);
        assert_eq!(lim.note(t1, cfg), RateDecision::Allow);
    }

    #[test]
    fn malformed_budget_trips() {
        let mut abuse = ConnectionAbuse::default();
        assert!(!abuse.note_malformed(3));
        assert!(!abuse.note_malformed(3));
        assert!(abuse.note_malformed(3));
        assert_eq!(abuse.malformed_messages, 3);
    }

    #[test]
    fn sanitize_strips_control_and_bounds() {
        let dirty = "ok\nline\x00\t";
        let clean = sanitize_log_text(dirty);
        assert!(!clean.contains('\n'));
        assert!(!clean.contains('\0'));
        assert_eq!(sanitize_log_text(&"x".repeat(200)).len(), MAX_LABEL_BYTES);
    }

    #[test]
    fn input_rate_is_independent_of_control_rate() {
        let mut cfg = NetworkAbuseConfig::DEV;
        cfg.control_messages_per_window = 1;
        cfg.input_messages_per_window = 3;
        cfg.input_drops_before_disconnect = 2;
        let t0 = Instant::now();
        let mut input = ControlRateLimit::new(t0);
        assert_eq!(input.note_input(t0, cfg), RateDecision::Allow);
        assert_eq!(input.note_input(t0, cfg), RateDecision::Allow);
        assert_eq!(input.note_input(t0, cfg), RateDecision::Allow);
        assert_eq!(input.note_input(t0, cfg), RateDecision::Drop);
        assert_eq!(input.note_input(t0, cfg), RateDecision::Disconnect);
        let mut control = ControlRateLimit::new(t0);
        assert_eq!(control.note(t0, cfg), RateDecision::Allow);
        assert_eq!(control.note(t0, cfg), RateDecision::Drop);
    }

    #[test]
    fn new_connection_abuse_starts_clean() {
        let a = ConnectionAbuse::default();
        let b = ConnectionAbuse::default();
        assert_eq!(a.malformed_messages, b.malformed_messages);
        assert_eq!(a.invalid_datagrams, 0);
    }
}
