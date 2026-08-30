//! Server listen configuration. Host/port come from [`purgatory_protocol::NetworkConfig`].

use std::net::SocketAddr;
use std::time::Duration;

use purgatory_common::DEFAULT_METRICS_PORT;
use purgatory_protocol::{IDLE_TIMEOUT, NetworkConfig};

use super::abuse::NetworkAbuseConfig;

/// Bound for lifecycle/input channel capacities under load mode.
#[allow(dead_code)]
pub const LOAD_MODE_ADMISSION_DEFAULT: usize = 256;

/// Named bounded drain for graceful persistence shutdown. Not an architectural
/// invariant that shutdown always equals this duration.
pub const DEFAULT_PERSISTENCE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// Bind address, idle timeout, abuse policy, and load-channel sizing.
#[derive(Clone, Copy, Debug)]
pub struct ServerEndpointConfig {
    pub bind: SocketAddr,
    pub idle_timeout: Duration,
    pub abuse: NetworkAbuseConfig,
    /// Capacity of the lifecycle mpsc (Attach/Detach/Enter).
    pub lifecycle_cap: usize,
    /// Capacity of the global input handoff mpsc.
    pub input_cap: usize,
    pub metrics_port: u16,
    /// Bound for graceful persistence drain on shutdown.
    pub persistence_shutdown_timeout: Duration,
}

impl ServerEndpointConfig {
    /// Local development listener (`127.0.0.1:5001`). Default admission 32.
    #[must_use]
    pub fn dev() -> Self {
        Self::from_env(NetworkAbuseConfig::DEV)
    }

    /// Build config from defaults, applying `PURGATORY_ADMISSION_CAP` when set.
    #[must_use]
    pub fn from_env(mut abuse: NetworkAbuseConfig) -> Self {
        if let Some(cap) = parse_admission_cap() {
            abuse.max_inflight_connection_tasks = cap;
        }
        let admission = abuse.max_inflight_connection_tasks;
        let lifecycle_cap = admission.max(64);
        let input_cap = (admission.saturating_mul(2)).max(128);
        let metrics_port = parse_metrics_port().unwrap_or(DEFAULT_METRICS_PORT);
        Self {
            bind: NetworkConfig::DEV.socket_addr(),
            idle_timeout: IDLE_TIMEOUT,
            abuse,
            lifecycle_cap,
            input_cap,
            metrics_port,
            persistence_shutdown_timeout: parse_persist_shutdown_timeout()
                .unwrap_or(DEFAULT_PERSISTENCE_SHUTDOWN_TIMEOUT),
        }
    }

    /// Ephemeral localhost port for tests.
    #[cfg(test)]
    #[must_use]
    pub fn ephemeral() -> Self {
        let mut cfg = Self::dev();
        cfg.bind = SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 0));
        cfg
    }
}

impl Default for ServerEndpointConfig {
    fn default() -> Self {
        Self::dev()
    }
}

fn parse_admission_cap() -> Option<usize> {
    let raw = std::env::var("PURGATORY_ADMISSION_CAP").ok()?;
    let cap: usize = raw.parse().ok()?;
    // Bound accidental unbounded input.
    Some(cap.clamp(1, 512))
}

fn parse_metrics_port() -> Option<u16> {
    let raw = std::env::var("PURGATORY_METRICS_PORT").ok()?;
    raw.parse().ok()
}

fn parse_persist_shutdown_timeout() -> Option<Duration> {
    let raw = std::env::var("PURGATORY_PERSISTENCE_SHUTDOWN_TIMEOUT_MS").ok()?;
    let ms: u64 = raw.parse().ok()?;
    Some(Duration::from_millis(ms.clamp(50, 60_000)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dev_admission_is_32() {
        // Ensure env does not leak into this unit test.
        let abuse = NetworkAbuseConfig::DEV;
        assert_eq!(abuse.max_inflight_connection_tasks, 32);
        let lifecycle = abuse.max_inflight_connection_tasks.max(64);
        assert_eq!(lifecycle, 64);
    }

    #[test]
    fn load_mode_caps_scale_with_admission() {
        let admission = LOAD_MODE_ADMISSION_DEFAULT;
        let lifecycle = admission.max(64);
        let input = (admission.saturating_mul(2)).max(128);
        assert_eq!(lifecycle, 256);
        assert_eq!(input, 512);
    }
}
