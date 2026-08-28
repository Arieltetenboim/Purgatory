//! Server listen configuration. Host/port come from [`purgatory_protocol::NetworkConfig`].

use std::net::SocketAddr;
use std::time::Duration;

use purgatory_protocol::{IDLE_TIMEOUT, NetworkConfig};

use super::abuse::NetworkAbuseConfig;

/// Bind address, idle timeout, and centralized abuse policy.
#[derive(Clone, Copy, Debug)]
pub struct ServerEndpointConfig {
    pub bind: SocketAddr,
    pub idle_timeout: Duration,
    pub abuse: NetworkAbuseConfig,
}

impl ServerEndpointConfig {
    /// Local development listener (`127.0.0.1:5001`).
    #[must_use]
    pub const fn dev() -> Self {
        Self {
            bind: NetworkConfig::DEV.socket_addr(),
            idle_timeout: IDLE_TIMEOUT,
            abuse: NetworkAbuseConfig::DEV,
        }
    }

    /// Ephemeral localhost port for tests.
    #[cfg(test)]
    #[must_use]
    pub fn ephemeral() -> Self {
        Self {
            bind: SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 0)),
            idle_timeout: IDLE_TIMEOUT,
            abuse: NetworkAbuseConfig::DEV,
        }
    }
}

impl Default for ServerEndpointConfig {
    fn default() -> Self {
        Self::dev()
    }
}
