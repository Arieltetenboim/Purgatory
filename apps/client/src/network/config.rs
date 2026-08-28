//! Client connect configuration. Host/port come from [`purgatory_protocol::NetworkConfig`].

use std::net::SocketAddr;
use std::time::Duration;

use purgatory_protocol::{IDLE_TIMEOUT, NetworkConfig};

/// Where the desktop client connects in development.
#[derive(Clone, Copy, Debug)]
pub struct ClientEndpointConfig {
    pub server: SocketAddr,
    pub idle_timeout: Duration,
}

impl ClientEndpointConfig {
    #[must_use]
    pub const fn dev() -> Self {
        Self {
            server: NetworkConfig::DEV.socket_addr(),
            idle_timeout: IDLE_TIMEOUT,
        }
    }
}

impl Default for ClientEndpointConfig {
    fn default() -> Self {
        Self::dev()
    }
}
