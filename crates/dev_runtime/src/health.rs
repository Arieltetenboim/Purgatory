use std::net::UdpSocket;
use std::time::Instant;

use purgatory_common::{LoadMetricsV1, decode_metrics_response, metrics_request_datagram};

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
    let mut buf = [0u8; 2048];
    let n = sock.recv(&mut buf).ok()?;
    decode_metrics_response(&buf[..n])
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
