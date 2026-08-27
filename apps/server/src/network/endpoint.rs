//! Quinn endpoint bind. Transport types stay in this module.

use std::sync::Arc;

use quinn::crypto::rustls::QuicServerConfig;
use quinn::{Endpoint, ServerConfig, TransportConfig};
use tokio::sync::Mutex;

use purgatory_protocol::ALPN_PROTOCOL;

use super::cert::generate_dev_only_self_signed;
use super::config::ServerEndpointConfig;
use super::session::{ConnectionIdAllocator, SessionTable};

pub(crate) struct BoundEndpoint {
    pub endpoint: Endpoint,
    pub sessions: Arc<Mutex<SessionTable>>,
    pub ids: Arc<ConnectionIdAllocator>,
}

impl BoundEndpoint {
    pub fn local_addr(&self) -> std::net::SocketAddr {
        self.endpoint
            .local_addr()
            .expect("bound endpoint has a local addr")
    }
}

pub(crate) fn bind(config: &ServerEndpointConfig) -> Result<BoundEndpoint, String> {
    let (cert, key) = generate_dev_only_self_signed()?;
    let mut tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .map_err(|err| format!("tls server config: {err}"))?;
    tls.alpn_protocols = vec![ALPN_PROTOCOL.to_vec()];
    tls.max_early_data_size = 0;

    let mut transport = TransportConfig::default();
    transport.datagram_receive_buffer_size(Some(4096));
    transport.datagram_send_buffer_size(4096);

    let mut server_config = ServerConfig::with_crypto(Arc::new(
        QuicServerConfig::try_from(tls).map_err(|err| format!("quic server tls: {err}"))?,
    ));
    server_config.transport_config(Arc::new(transport));

    let endpoint = Endpoint::server(server_config, config.bind)
        .map_err(|err| format!("bind {}: {err}", config.bind))?;

    Ok(BoundEndpoint {
        endpoint,
        sessions: Arc::new(Mutex::new(SessionTable::new())),
        ids: Arc::new(ConnectionIdAllocator::new()),
    })
}
