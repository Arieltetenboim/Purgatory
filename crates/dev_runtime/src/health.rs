use std::net::UdpSocket;
use std::time::Instant;

use purgatory_common::{
    LoadMetricsV1, METRICS_MAX_DATAGRAM_BYTES, decode_metrics_response, metrics_request_datagram,
};

use crate::config::{
    LISTEN_HOST, LISTENER_CACHE, METRICS_CACHE, METRICS_PORT, METRICS_RECV_TIMEOUT,
};
use crate::process::ListenerDiag;

pub trait HealthSource {
    fn poll_metrics(&mut self, now: Instant) -> Option<LoadMetricsV1>;
    fn listener(&mut self, now: Instant) -> ListenerDiag;
}

pub struct StdHealthSource {
    metrics_cache: Option<LoadMetricsV1>,
    metrics_at: Option<Instant>,
    listener_cache: ListenerDiag,
    listener_at: Option<Instant>,
}

impl StdHealthSource {
    pub fn new() -> Self {
        Self {
            metrics_cache: None,
            metrics_at: None,
            listener_cache: ListenerDiag::Unknown,
            listener_at: None,
        }
    }
}

impl Default for StdHealthSource {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthSource for StdHealthSource {
    fn poll_metrics(&mut self, now: Instant) -> Option<LoadMetricsV1> {
        if let (Some(cached), Some(at)) = (&self.metrics_cache, self.metrics_at)
            && now.duration_since(at) < METRICS_CACHE
        {
            return Some(cached.clone());
        }
        let got = poll_purgstat();
        if let Some(m) = &got {
            self.metrics_cache = Some(m.clone());
            self.metrics_at = Some(now);
        }
        got
    }

    fn listener(&mut self, now: Instant) -> ListenerDiag {
        if let Some(at) = self.listener_at
            && now.duration_since(at) < LISTENER_CACHE
        {
            return self.listener_cache;
        }
        // Slice 1: no IPGlobalProperties equivalent. Diagnostic only; must not affect Ready.
        self.listener_cache = ListenerDiag::Unknown;
        self.listener_at = Some(now);
        self.listener_cache
    }
}

fn poll_purgstat() -> Option<LoadMetricsV1> {
    let sock = UdpSocket::bind("127.0.0.1:0").ok()?;
    sock.set_read_timeout(Some(METRICS_RECV_TIMEOUT)).ok()?;
    let addr = format!("{LISTEN_HOST}:{METRICS_PORT}");
    sock.connect(&addr).ok()?;
    let req = metrics_request_datagram();
    sock.send(&req).ok()?;
    // Schema 4 raised the datagram cap to 4096; a 2048 recv buffer makes Windows
    // UDP return WSAEMSGSIZE and Hub falsely report metrics health lost.
    let mut buf = [0u8; METRICS_MAX_DATAGRAM_BYTES];
    let n = sock.recv(&mut buf).ok()?;
    decode_metrics_response(&buf[..n])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_recv_buffer_fits_schema_cap() {
        const {
            assert!(METRICS_MAX_DATAGRAM_BYTES >= 4096);
        }
        // Ensure poll_purgstat's stack buffer constant stays coupled to the shared cap.
        let _ = [0u8; METRICS_MAX_DATAGRAM_BYTES];
    }

    #[test]
    #[ignore = "requires a live purgatory-server with PURGSTAT on 127.0.0.1:5002"]
    fn live_metrics_poll_succeeds_against_listening_server() {
        let mut h = StdHealthSource::new();
        let m = h.poll_metrics(Instant::now());
        assert!(
            m.is_some(),
            "poll_metrics None — buffer/timeout/bind regression vs live server"
        );
        assert!(m.unwrap().metrics_schema_version >= 1);
    }
}

#[derive(Clone, Debug)]
pub struct FakeHealthSource {
    pub metrics: Option<LoadMetricsV1>,
    pub listener: ListenerDiag,
}

impl FakeHealthSource {
    pub fn none() -> Self {
        Self {
            metrics: None,
            listener: ListenerDiag::Unknown,
        }
    }

    pub fn healthy() -> Self {
        let mut metrics = LoadMetricsV1::with_schema();
        metrics.admission_cap = 256;
        metrics.max_entities_per_snapshot = 256;
        Self {
            metrics: Some(metrics),
            listener: ListenerDiag::Yes,
        }
    }

    pub fn load_incompatible() -> Self {
        let mut metrics = LoadMetricsV1::with_schema();
        metrics.admission_cap = 1;
        metrics.max_entities_per_snapshot = 1;
        Self {
            metrics: Some(metrics),
            listener: ListenerDiag::Yes,
        }
    }
}

impl HealthSource for FakeHealthSource {
    fn poll_metrics(&mut self, _now: Instant) -> Option<LoadMetricsV1> {
        self.metrics.clone()
    }

    fn listener(&mut self, _now: Instant) -> ListenerDiag {
        self.listener
    }
}
