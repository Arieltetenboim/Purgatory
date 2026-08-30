//! Shared Quinn client endpoint for all bot sessions.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::crypto::rustls::QuicClientConfig;
use quinn::{ClientConfig, Endpoint, TransportConfig, VarInt};

use purgatory_protocol::{ALPN_PROTOCOL, IDLE_TIMEOUT};

use crate::cert::DevOnlySkipServerVerification;

pub struct SharedEndpoint {
    endpoint: Endpoint,
}

impl SharedEndpoint {
    pub fn new() -> Result<Self, String> {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let mut tls = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(DevOnlySkipServerVerification::new())
            .with_no_client_auth();
        tls.alpn_protocols = vec![ALPN_PROTOCOL.to_vec()];

        let mut client = ClientConfig::new(Arc::new(
            QuicClientConfig::try_from(tls).map_err(|e| format!("quic client tls: {e}"))?,
        ));

        let mut transport = TransportConfig::default();
        transport.datagram_receive_buffer_size(Some(4096));
        transport.datagram_send_buffer_size(4096);
        transport.max_concurrent_uni_streams(VarInt::from_u32(1));
        apply_idle_timeout(&mut transport, IDLE_TIMEOUT);
        client.transport_config(Arc::new(transport));

        let mut endpoint = Endpoint::client(SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 0)))
            .map_err(|e| format!("client bind: {e}"))?;
        endpoint.set_default_client_config(client);

        Ok(Self { endpoint })
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"shutdown");
    }
}

fn apply_idle_timeout(transport: &mut TransportConfig, idle: Duration) {
    if let Ok(timeout) = idle.try_into() {
        transport.max_idle_timeout(Some(timeout));
    }
}
