//! Shared development network constants. Do not scatter host/port literals.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

/// Development bind/connect host.
pub const DEFAULT_DEV_HOST: &str = "127.0.0.1";

/// Development QUIC UDP port.
pub const DEFAULT_DEV_PORT: u16 = 5001;

/// Maximum control-stream payload (excluding the 4-byte length prefix).
pub const MAX_CONTROL_MESSAGE_BYTES: u32 = 4096;

/// Maximum gameplay snapshot payload (excluding the 4-byte length prefix).
/// Independent from [`MAX_CONTROL_MESSAGE_BYTES`]. Packet/resource safety, not
/// a population or bandwidth target.
pub const MAX_GAMEPLAY_SNAPSHOT_BYTES: u32 = 8192;

/// Maximum replicated entities in one snapshot. Decode validates this before
/// allocating. Not a gameplay capacity claim. Raised 64→256 in Phase 5.7 as a
/// mechanical protocol-v4 decode bound (count is already `u16`; layout unchanged).
/// `MAX_GAMEPLAY_SNAPSHOT_BYTES` remains the packet wall (~326 entities at 25 B each).
pub const MAX_ENTITIES_PER_SNAPSHOT: u16 = 256;

/// Maximum accepted datagram payload.
pub const MAX_DATAGRAM_BYTES: usize = 256;

/// Maximum bytes in Hello/Welcome/DisconnectReason string fields.
pub const MAX_LABEL_BYTES: usize = 64;

/// QUIC ALPN identifier for this game protocol.
pub const ALPN_PROTOCOL: &[u8] = b"purgatory";

/// How long a peer may sit after QUIC connect without a valid Hello.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Explicit QUIC idle timeout (transport liveness only, not AFK).
///
/// Ping cadence is 1 s. 15 s is long enough that localhost jitter never trips
/// it and short enough for development. Distinct from [`HANDSHAKE_TIMEOUT`].
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(15);

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
        assert_eq!(MAX_GAMEPLAY_SNAPSHOT_BYTES, 8192);
        assert_eq!(MAX_ENTITIES_PER_SNAPSHOT, 256);
        assert_ne!(MAX_CONTROL_MESSAGE_BYTES, MAX_GAMEPLAY_SNAPSHOT_BYTES);
        assert_eq!(MAX_LABEL_BYTES, 64);
        assert_eq!(HANDSHAKE_TIMEOUT, Duration::from_secs(5));
        assert_eq!(IDLE_TIMEOUT, Duration::from_secs(15));
        assert_eq!(PING_INTERVAL, Duration::from_secs(1));
        assert_eq!(ALPN_PROTOCOL, b"purgatory");
    }
}
