//! Server listen configuration. Host/port come from [`purgatory_protocol::NetworkConfig`].

use std::net::SocketAddr;
use std::time::Duration;

use purgatory_protocol::{HANDSHAKE_TIMEOUT, NetworkConfig};

/// Bind address and handshake limits for the dedicated server.
#[derive(Clone, Copy, Debug)]
pub struct ServerEndpointConfig {
    pub bind: SocketAddr,
    pub handshake_timeout: Duration,
}

impl ServerEndpointConfig {
    /// Local development listener (`127.0.0.1:5001`).
    #[must_use]
    pub const fn dev() -> Self {
        Self {
            bind: NetworkConfig::DEV.socket_addr(),
            handshake_timeout: HANDSHAKE_TIMEOUT,
        }
    }

    /// Ephemeral localhost port for tests.
    #[cfg(test)]
    #[must_use]
    pub fn ephemeral() -> Self {
        Self {
            bind: SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 0)),
            handshake_timeout: HANDSHAKE_TIMEOUT,
        }
    }
}

impl Default for ServerEndpointConfig {
    fn default() -> Self {
        Self::dev()
    }
}
