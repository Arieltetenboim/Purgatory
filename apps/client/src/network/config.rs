//! Client connect configuration. Host/port come from [`purgatory_protocol::NetworkConfig`].

use std::net::SocketAddr;

use purgatory_protocol::NetworkConfig;

/// Where the desktop client connects in development.
#[derive(Clone, Copy, Debug)]
pub struct ClientEndpointConfig {
    pub server: SocketAddr,
}

impl ClientEndpointConfig {
    #[must_use]
    pub const fn dev() -> Self {
        Self {
            server: NetworkConfig::DEV.socket_addr(),
        }
    }
}

impl Default for ClientEndpointConfig {
    fn default() -> Self {
        Self::dev()
    }
}
