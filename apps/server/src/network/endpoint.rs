//! Quinn endpoint bind. Transport types stay in this module.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use quinn::crypto::rustls::QuicServerConfig;
use quinn::{Endpoint, ServerConfig, TransportConfig};
use tokio::sync::Semaphore;

use purgatory_protocol::ALPN_PROTOCOL;
use quinn::VarInt;

use super::abuse::NetworkAbuseConfig;
use super::cert::generate_dev_only_self_signed;
use super::config::ServerEndpointConfig;
use super::session::{ConnectionIdAllocator, SessionTable};
use super::stats::ServerNetStats;

pub(crate) struct BoundEndpoint {
    pub endpoint: Endpoint,
    pub sessions: Arc<Mutex<SessionTable>>,
    pub ids: Arc<ConnectionIdAllocator>,
    pub inflight_tasks: Arc<AtomicU64>,
    pub limiter: Arc<Semaphore>,
    pub stats: Arc<ServerNetStats>,
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
    apply_stream_limits(&mut transport, &config.abuse);
    if let Ok(timeout) = config.idle_timeout.try_into() {
        transport.max_idle_timeout(Some(timeout));
    }

    let mut server_config = ServerConfig::with_crypto(Arc::new(
        QuicServerConfig::try_from(tls).map_err(|err| format!("quic server tls: {err}"))?,
    ));
    server_config.transport_config(Arc::new(transport));

    // The low-level IO/Quinn error stays inside this boundary; callers get one
    // concise, actionable line naming the address that could not be bound.
    let endpoint = Endpoint::server(server_config, config.bind)
        .map_err(|err| format!("failed to bind {}: {err}", config.bind))?;

    Ok(BoundEndpoint {
        endpoint,
        sessions: Arc::new(Mutex::new(SessionTable::new())),
        ids: Arc::new(ConnectionIdAllocator::new()),
        inflight_tasks: Arc::new(AtomicU64::new(0)),
        limiter: Arc::new(Semaphore::new(config.abuse.max_inflight_connection_tasks)),
        stats: Arc::new(ServerNetStats::default()),
    })
}

fn apply_stream_limits(transport: &mut TransportConfig, abuse: &NetworkAbuseConfig) {
    transport.max_concurrent_bidi_streams(VarInt::from_u32(abuse.max_bidi_streams));
    transport.max_concurrent_uni_streams(VarInt::from_u32(abuse.max_uni_streams));
}
