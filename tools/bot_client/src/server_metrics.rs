//! UDP polling for server LoadMetricsV1.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

use purgatory_common::{
    LoadMetricsV1, METRICS_MAX_DATAGRAM_BYTES, decode_metrics_response, metrics_request_datagram,
};

pub struct ServerMetricsPoller {
    socket: UdpSocket,
    server_addr: SocketAddr,
    last_metrics: Option<LoadMetricsV1>,
    last_poll_at: Option<Instant>,
    poll_ok_count: u64,
    poll_fail_count: u64,
}

impl ServerMetricsPoller {
    pub async fn new(metrics_addr: SocketAddr) -> Result<Self, String> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| format!("bind metrics socket: {e}"))?;
        Ok(Self {
            socket,
            server_addr: metrics_addr,
            last_metrics: None,
            last_poll_at: None,
            poll_ok_count: 0,
            poll_fail_count: 0,
        })
    }

    pub async fn poll(&mut self) -> Option<LoadMetricsV1> {
        let request = metrics_request_datagram();
        if self
            .socket
            .send_to(&request, self.server_addr)
            .await
            .is_err()
        {
            self.poll_fail_count += 1;
            return None;
        }

        let mut buf = [0u8; METRICS_MAX_DATAGRAM_BYTES];
        let result =
            tokio::time::timeout(Duration::from_millis(500), self.socket.recv_from(&mut buf)).await;

        match result {
            Ok(Ok((len, _addr))) => {
                if let Some(metrics) = decode_metrics_response(&buf[..len]) {
                    self.last_metrics = Some(metrics.clone());
                    self.last_poll_at = Some(Instant::now());
                    self.poll_ok_count += 1;
                    Some(metrics)
                } else {
                    self.poll_fail_count += 1;
                    None
                }
            }
            _ => {
                self.poll_fail_count += 1;
                None
            }
        }
    }

    pub fn last_metrics(&self) -> Option<&LoadMetricsV1> {
        self.last_metrics.as_ref()
    }

    pub fn poll_ok_count(&self) -> u64 {
        self.poll_ok_count
    }

    pub fn poll_fail_count(&self) -> u64 {
        self.poll_fail_count
    }

    pub fn server_metrics_ok(&self) -> bool {
        self.poll_ok_count > 0 && self.poll_fail_count < self.poll_ok_count
    }
}
