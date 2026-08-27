//! Shared development network constants. Do not scatter host/port literals.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

/// Development bind/connect host.
pub const DEFAULT_DEV_HOST: &str = "127.0.0.1";

/// Development QUIC UDP port.
pub const DEFAULT_DEV_PORT: u16 = 5001;

/// Maximum control-stream payload (excluding the 4-byte length prefix).
pub const MAX_CONTROL_MESSAGE_BYTES: u32 = 4096;

/// Maximum accepted datagram payload.
pub const MAX_DATAGRAM_BYTES: usize = 256;

/// Maximum bytes in Hello/Welcome/DisconnectReason string fields.
pub const MAX_LABEL_BYTES: usize = 64;

/// QUIC ALPN identifier for this game protocol.
pub const ALPN_PROTOCOL: &[u8] = b"purgatory";

/// How long a peer may sit after QUIC connect without a valid Hello.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Development RTT ping cadence. Not a gameplay timer.
pub const PING_INTERVAL: Duration = Duration::from_secs(1);

/// Shared development endpoint description.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkConfig {
    pub host: Ipv4Addr,
    pub port: u16,
}

impl NetworkConfig {
    /// Local development defaults (`127.0.0.1:5001`).
    pub const DEV: Self = Self {
        host: Ipv4Addr::LOCALHOST,
        port: DEFAULT_DEV_PORT,
    };

    #[must_use]
    pub const fn socket_addr(self) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(self.host, self.port))
    }
}

/// Convenience alias for [`NetworkConfig::DEV`].
#[must_use]
pub const fn dev_socket_addr() -> SocketAddr {
    NetworkConfig::DEV.socket_addr()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_addr_is_localhost_5001() {
        let addr = dev_socket_addr();
        assert_eq!(addr.ip().to_string(), DEFAULT_DEV_HOST);
        assert_eq!(addr.port(), DEFAULT_DEV_PORT);
        assert_eq!(MAX_CONTROL_MESSAGE_BYTES, 4096);
        assert_eq!(MAX_LABEL_BYTES, 64);
        assert_eq!(HANDSHAKE_TIMEOUT, Duration::from_secs(5));
        assert_eq!(PING_INTERVAL, Duration::from_secs(1));
        assert_eq!(ALPN_PROTOCOL, b"purgatory");
    }
}
