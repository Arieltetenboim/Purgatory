//! Handshake and session tests. Localhost only. Never panic on bad input.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use quinn::crypto::rustls::QuicClientConfig;
use quinn::{ClientConfig, Connection, Endpoint, RecvStream, SendStream, TransportConfig};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::time::timeout;

use purgatory_protocol::{
    AbilityActivateRequest, ClientControl, ConnectionId, DisconnectReasonCode, EquipRequest,
    EquipmentRejectReason, Hello, InputCommand, MAX_CONTROL_MESSAGE_BYTES, MoveAxis,
    PROTOCOL_VERSION, ReplicatedEquipment, ReplicatedKind, ReplicationFrame, ReplicationRecord,
    ServerAbility, ServerControl, ServerEquipment, SnapshotEntity, UnequipRequest, WireEntityId,
    decode_client_datagram, decode_replication_frame, decode_server_control, encode_client_control,
    encode_client_datagram, encode_frame, peek_frame_len, peek_gameplay_frame_len,
};

use super::abuse::NetworkAbuseConfig;
use super::config::ServerEndpointConfig;
use super::endpoint::{self, BoundEndpoint};
use super::install_crypto_provider;
use super::session::{SessionTable, lock_sessions};
use super::{IncomingDispatch, dispatch_incoming};

#[derive(Debug)]
struct DevOnlySkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

impl DevOnlySkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self(Arc::new(rustls::crypto::ring::default_provider())))
    }
}

impl ServerCertVerifier for DevOnlySkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

struct TestServer {
    addr: SocketAddr,
    sessions: Arc<std::sync::Mutex<SessionTable>>,
    inflight: Arc<std::sync::atomic::AtomicU64>,
    limiter: Arc<tokio::sync::Semaphore>,
    stats: Arc<super::stats::ServerNetStats>,
    endpoint: Endpoint,
    accept: tokio::task::JoinHandle<()>,
    abuse: NetworkAbuseConfig,
}

/// Active gauges only. Every field must return to baseline after a scenario.
/// Cumulative counters (accepted, rejected, …) are checked separately.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ActiveGauges {
    sessions: usize,
    handshakes: u64,
    inflight: u64,
    admission_in_use: usize,
}

impl ActiveGauges {
    fn is_baseline(self) -> bool {
        self == Self::default()
    }
}

impl TestServer {
    async fn spawn(handshake_timeout: Duration) -> Self {
        Self::spawn_with(handshake_timeout, purgatory_protocol::IDLE_TIMEOUT).await
    }

    async fn spawn_with(handshake_timeout: Duration, idle_timeout: Duration) -> Self {
        Self::spawn_abuse(handshake_timeout, idle_timeout, NetworkAbuseConfig::DEV).await
    }

    async fn spawn_abuse(
        handshake_timeout: Duration,
        idle_timeout: Duration,
        mut abuse: NetworkAbuseConfig,
    ) -> Self {
        install_crypto_provider().expect("crypto");
        let mut config = ServerEndpointConfig::ephemeral();
        abuse.handshake_timeout = handshake_timeout;
        config.idle_timeout = idle_timeout;
        config.abuse = abuse;
        let bound = endpoint::bind(&config).expect("bind");
        let addr = bound.local_addr();
        let sessions = bound.sessions.clone();
        let inflight = bound.inflight_tasks.clone();
        let limiter = bound.limiter.clone();
        let stats = bound.stats.clone();
        let endpoint = bound.endpoint.clone();
        let abuse = config.abuse;
        let accept = tokio::spawn(async move {
            accept_loop(bound, abuse, None, None).await;
        });
        Self {
            addr,
            sessions,
            inflight,
            limiter,
            stats,
            endpoint,
            accept,
            abuse,
        }
    }

    fn session_count(&self) -> usize {
        lock_sessions(&self.sessions).len()
    }

    fn contains(&self, id: ConnectionId) -> bool {
        lock_sessions(&self.sessions).contains(id)
    }

    fn inflight_tasks(&self) -> u64 {
        self.inflight.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn stat(
        &self,
        pick: impl Fn(&super::stats::ServerNetStats) -> &std::sync::atomic::AtomicU64,
    ) -> u64 {
        pick(&self.stats).load(std::sync::atomic::Ordering::Relaxed)
    }

    fn peak_sessions(&self) -> usize {
        lock_sessions(&self.sessions).high_water()
    }

    fn admission_cap(&self) -> usize {
        self.abuse.max_inflight_connection_tasks
    }

    fn gauges(&self) -> ActiveGauges {
        ActiveGauges {
            sessions: self.session_count(),
            handshakes: self
                .stats
                .active_handshakes
                .load(std::sync::atomic::Ordering::Relaxed),
            inflight: self.inflight_tasks(),
            admission_in_use: self
                .admission_cap()
                .saturating_sub(self.limiter.available_permits()),
        }
    }

    /// Cumulative counters + peaks, for failure reports only.
    fn report(&self) -> String {
        let (active, peak) = {
            let table = lock_sessions(&self.sessions);
            (table.len(), table.high_water())
        };
        format!(
            "{} admission={}/{}",
            self.stats.summary(active, self.inflight_tasks(), peak),
            self.gauges().admission_in_use,
            self.admission_cap()
        )
    }

    /// Polls active gauges back to baseline. Never a bare sleep assertion.
    async fn wait_until_baseline(&self, scenario: &str, limit: Duration) {
        let ok = wait_until(|| self.gauges().is_baseline(), limit).await;
        assert!(
            ok,
            "[{scenario}] active state did not return to baseline: {:?} | {}",
            self.gauges(),
            self.report()
        );
    }

    /// High-water marks may never exceed the configured admission cap.
    fn assert_admission_bounds(&self, scenario: &str) {
        let cap = self.admission_cap() as u64;
        let max_inflight = self
            .stats
            .max_inflight
            .load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            max_inflight <= cap,
            "[{scenario}] max inflight {max_inflight} exceeded cap {cap} | {}",
            self.report()
        );
        assert!(
            self.peak_sessions() as u64 <= cap,
            "[{scenario}] peak sessions {} exceeded cap {cap} | {}",
            self.peak_sessions(),
            self.report()
        );
    }

    /// Mandatory recovery probe: a normal client must still work end to end.
    async fn assert_healthy_probe(&self, scenario: &str) {
        let probe = timeout(Duration::from_secs(5), handshake_peer(self.addr))
            .await
            .unwrap_or_else(|_| panic!("[{scenario}] healthy probe timed out | {}", self.report()));
        let id = probe.id;
        assert!(
            wait_until(|| self.contains(id), Duration::from_secs(3)).await,
            "[{scenario}] probe session missing | {}",
            self.report()
        );
        let nonce = 0xF0;
        probe
            .client
            .conn
            .send_datagram(encode_client_datagram(nonce).expect("ping").into())
            .expect("probe ping");
        let pong = timeout(Duration::from_secs(3), probe.client.conn.read_datagram())
            .await
            .unwrap_or_else(|_| panic!("[{scenario}] probe pong timed out | {}", self.report()))
            .expect("probe pong");
        assert_eq!(
            purgatory_protocol::decode_server_datagram(&pong).expect("pong"),
            purgatory_protocol::ServerDatagram::Pong { nonce }
        );
        probe.client.conn.close(0u32.into(), b"probe");
        drop(probe);
        self.wait_until_baseline(scenario, Duration::from_secs(5))
            .await;
    }

    fn shutdown(self) {
        self.endpoint.close(
            u32::from(DisconnectReasonCode::ServerShutdown.as_u8()).into(),
            b"shutdown",
        );
        self.accept.abort();
    }
}

async fn accept_loop(
    bound: BoundEndpoint,
    abuse: NetworkAbuseConfig,
    gameplay: Option<super::gameplay::GameplayTx>,
    persist: Option<super::persist::PersistenceHandle>,
) {
    let lifecycle = Arc::new(super::connection_lifecycle::ConnectionLifecycleBook::new());
    let pressure = Arc::new(super::network_pressure::NetworkPressureBook::new());
    while let Some(incoming) = bound.endpoint.accept().await {
        dispatch_incoming(
            incoming,
            IncomingDispatch {
                sessions: bound.sessions.clone(),
                ids: bound.ids.clone(),
                abuse,
                limiter: bound.limiter.clone(),
                inflight: bound.inflight_tasks.clone(),
                stats: bound.stats.clone(),
                gameplay: gameplay.clone(),
                persist: persist.clone(),
                lifecycle: lifecycle.clone(),
                pressure: pressure.clone(),
            },
        );
    }
}

fn test_client_endpoint() -> Endpoint {
    test_client_endpoint_idle(purgatory_protocol::IDLE_TIMEOUT)
}

fn test_client_endpoint_idle(idle: Duration) -> Endpoint {
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(DevOnlySkipServerVerification::new())
        .with_no_client_auth();
    tls.alpn_protocols = vec![purgatory_protocol::ALPN_PROTOCOL.to_vec()];
    let mut client_crypto = ClientConfig::new(Arc::new(
        QuicClientConfig::try_from(tls).expect("quic client tls"),
    ));
    let mut transport = TransportConfig::default();
    transport.datagram_receive_buffer_size(Some(4096));
    transport.datagram_send_buffer_size(4096);
    transport.max_concurrent_uni_streams(quinn::VarInt::from_u32(1));
    if let Ok(timeout) = idle.try_into() {
        transport.max_idle_timeout(Some(timeout));
    }
    client_crypto.transport_config(Arc::new(transport));
    let mut endpoint = Endpoint::client(SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 0)))
        .expect("client bind");
    endpoint.set_default_client_config(client_crypto);
    endpoint
}

struct TestClient {
    _endpoint: Endpoint,
    conn: Connection,
}

async fn connect(server: SocketAddr) -> TestClient {
    let endpoint = test_client_endpoint();
    let conn = endpoint
        .connect(server, "localhost")
        .expect("connect start")
        .await
        .expect("connect");
    TestClient {
        _endpoint: endpoint,
        conn,
    }
}

async fn handshake_ok(server: SocketAddr) -> (TestClient, SendStream, RecvStream, ConnectionId) {
    let client = connect(server).await;
    let (mut send, mut recv) = client.conn.open_bi().await.expect("open_bi");
    write_hello(&mut send, PROTOCOL_VERSION, "test").await;
    match read_server_control(&mut recv).await {
        Ok(ServerControl::Welcome(welcome)) => (client, send, recv, welcome.connection_id),
        other => panic!("expected welcome, got {other:?}"),
    }
}

async fn read_server_control(
    recv: &mut RecvStream,
) -> Result<ServerControl, quinn::ReadExactError> {
    let mut prefix = [0u8; 4];
    recv.read_exact(&mut prefix).await?;
    let len = peek_frame_len(&prefix).expect("peek");
    let mut payload = vec![0u8; len as usize];
    recv.read_exact(&mut payload).await?;
    Ok(decode_server_control(&payload).expect("decode"))
}

fn assert_app_close(err: &quinn::ReadExactError, code: DisconnectReasonCode) {
    match err {
        quinn::ReadExactError::ReadError(quinn::ReadError::ConnectionLost(
            quinn::ConnectionError::ApplicationClosed(close),
        )) => {
            assert_eq!(
                close.error_code,
                quinn::VarInt::from_u32(u32::from(code.as_u8()))
            );
        }
        other => panic!("expected application close {:?}, got {other:?}", code),
    }
}

/// Chaos scenarios must never panic on a write the server already closed.
async fn write_hello_best_effort(send: &mut SendStream, version: u32, build: &str) -> bool {
    let Ok(payload) = encode_client_control(&ClientControl::Hello(Hello {
        protocol_version: version,
        client_build: build.into(),
        dev_login: next_test_login(),
    })) else {
        return false;
    };
    let Ok(frame) = encode_frame(&payload) else {
        return false;
    };
    send.write_all(&frame).await.is_ok()
}

async fn write_hello(send: &mut SendStream, version: u32, build: &str) {
    write_hello_login(send, version, build, &next_test_login()).await;
}

async fn write_hello_login(send: &mut SendStream, version: u32, build: &str, login: &str) {
    let payload = encode_client_control(&ClientControl::Hello(Hello {
        protocol_version: version,
        client_build: build.into(),
        dev_login: login.into(),
    }))
    .expect("encode hello");
    let frame = encode_frame(&payload).expect("frame");
    send.write_all(&frame).await.expect("write hello");
}

fn next_test_login() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(1);
    format!("t.{:04}", SEQ.fetch_add(1, Ordering::Relaxed) % 10_000)
}

async fn write_input(
    send: &mut SendStream,
    sequence: u32,
    move_axis: MoveAxis,
    jump_pressed: bool,
    down_held: bool,
) {
    let payload = encode_client_control(&ClientControl::Input(InputCommand {
        input_epoch: 0,
        sequence,
        move_axis,
        jump_pressed,
        down_held,
        portal_held: false,
    }))
    .expect("encode input");
    let frame = encode_frame(&payload).expect("frame");
    send.write_all(&frame).await.expect("write input");
}

async fn write_control(send: &mut SendStream, msg: ClientControl) {
    let payload = encode_client_control(&msg).expect("encode control");
    let frame = encode_frame(&payload).expect("frame");
    send.write_all(&frame).await.expect("write control");
}

async fn expect_disconnect(recv: &mut RecvStream, code: DisconnectReasonCode) {
    match timeout(Duration::from_secs(2), read_server_control(recv)).await {
        Ok(Ok(ServerControl::Disconnect(reason))) => {
            assert_eq!(reason.code, code);
        }
        Ok(Err(err)) => assert_app_close(&err, code),
        other => panic!("expected disconnect {code:?}, got {other:?}"),
    }
}

#[tokio::test]
async fn valid_hello_receives_welcome() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let (_client, _send, _recv, id) = handshake_ok(server.addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(server.contains(id));
    assert_eq!(id.get(), 1);
    server.shutdown();
}

#[tokio::test]
async fn duplicate_live_character_is_rejected() {
    let (server, sim) = spawn_gameplay().await;
    let login = "alice";
    let client_a = connect(server.addr).await;
    let (mut send_a, mut recv_a) = client_a.conn.open_bi().await.expect("bi");
    write_hello_login(&mut send_a, PROTOCOL_VERSION, "a", login).await;
    let id = match timeout(Duration::from_secs(5), read_server_control(&mut recv_a)).await {
        Ok(Ok(ServerControl::Welcome(welcome))) => welcome.connection_id,
        other => panic!("expected welcome, got {other:?}"),
    };
    assert!(wait_attached(&sim, id).await);
    let client_b = connect(server.addr).await;
    let (mut send_b, mut recv_b) = client_b.conn.open_bi().await.expect("bi");
    write_hello_login(&mut send_b, PROTOCOL_VERSION, "b", login).await;
    expect_disconnect(&mut recv_b, DisconnectReasonCode::AlreadyConnected).await;
    server.shutdown();
}

#[tokio::test]
async fn abrupt_drop_releases_character_occupancy() {
    let (server, sim) = spawn_gameplay_abuse(
        Duration::from_secs(2),
        Duration::from_millis(400),
        NetworkAbuseConfig::DEV,
    )
    .await;
    let login = "alice";
    let client_a = connect(server.addr).await;
    let (mut send_a, mut recv_a) = client_a.conn.open_bi().await.expect("bi");
    write_hello_login(&mut send_a, PROTOCOL_VERSION, "a", login).await;
    let id = match timeout(Duration::from_secs(5), read_server_control(&mut recv_a)).await {
        Ok(Ok(ServerControl::Welcome(welcome))) => welcome.connection_id,
        other => panic!("expected welcome, got {other:?}"),
    };
    assert!(wait_attached(&sim, id).await);
    drop(client_a);
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.player_count() == 0
            },
            Duration::from_secs(8),
        )
        .await,
        "character occupancy must release after abrupt drop"
    );
    let client_b = connect(server.addr).await;
    let (mut send_b, mut recv_b) = client_b.conn.open_bi().await.expect("bi");
    write_hello_login(&mut send_b, PROTOCOL_VERSION, "b", login).await;
    match timeout(Duration::from_secs(5), read_server_control(&mut recv_b)).await {
        Ok(Ok(ServerControl::Welcome(_))) => {}
        other => panic!("expected welcome after occupancy release, got {other:?}"),
    }
    server.shutdown();
}

#[tokio::test]
async fn live_v10_hello_dev_local_receives_welcome() {
    let (server, sim) = spawn_gameplay().await;
    let client = connect(server.addr).await;
    let (mut send, mut recv) = client.conn.open_bi().await.expect("bi");
    write_hello_login(
        &mut send,
        PROTOCOL_VERSION,
        "purgatory-client-0.1.0",
        "dev.local",
    )
    .await;
    let id = match timeout(Duration::from_secs(5), read_server_control(&mut recv)).await {
        Ok(Ok(ServerControl::Welcome(welcome))) => welcome.connection_id,
        other => panic!("expected welcome after Enter, got {other:?}"),
    };
    assert!(wait_attached(&sim, id).await);
    server.shutdown();
}

#[tokio::test]
async fn v9_hello_is_version_mismatch_not_login_failure() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let client = connect(server.addr).await;
    let (mut send, mut recv) = client.conn.open_bi().await.expect("bi");
    write_hello(&mut send, 9, "test").await;
    expect_disconnect(&mut recv, DisconnectReasonCode::VersionMismatch).await;
    server.shutdown();
}

#[tokio::test]
async fn wrong_protocol_version_is_rejected() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let client = connect(server.addr).await;
    let (mut send, mut recv) = client.conn.open_bi().await.expect("open_bi");
    write_hello(&mut send, PROTOCOL_VERSION + 1, "test").await;
    expect_disconnect(&mut recv, DisconnectReasonCode::VersionMismatch).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(server.session_count(), 0);
    server.shutdown();
}

#[tokio::test]
async fn protocol_v1_hello_is_rejected() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let client = connect(server.addr).await;
    let (mut send, mut recv) = client.conn.open_bi().await.expect("open_bi");
    write_hello(&mut send, 1, "legacy-v1").await;
    expect_disconnect(&mut recv, DisconnectReasonCode::VersionMismatch).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(server.session_count(), 0);
    server.shutdown();
}

#[tokio::test]
async fn malformed_hello_is_rejected() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let client = connect(server.addr).await;
    let (mut send, _recv) = client.conn.open_bi().await.expect("open_bi");
    send.write_all(&encode_frame(&[0xff]).expect("frame"))
        .await
        .expect("write garbage");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.session_count(), 0);
    server.shutdown();
}

#[tokio::test]
async fn oversized_length_prefix_is_rejected() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let client = connect(server.addr).await;
    let (mut send, _recv) = client.conn.open_bi().await.expect("open_bi");
    let huge = (MAX_CONTROL_MESSAGE_BYTES + 1).to_le_bytes();
    send.write_all(&huge).await.expect("write huge len");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.session_count(), 0);
    server.shutdown();
}

#[tokio::test]
async fn no_hello_times_out() {
    let server = TestServer::spawn(Duration::from_millis(400)).await;
    let _client = connect(server.addr).await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(server.session_count(), 0);
    server.shutdown();
}

#[tokio::test]
async fn two_clients_get_distinct_connection_ids() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let (_a, _send_a, _recv_a, id_a) = handshake_ok(server.addr).await;
    let (_b, _send_b, _recv_b, id_b) = handshake_ok(server.addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_ne!(id_a, id_b);
    assert!(server.contains(id_a));
    assert!(server.contains(id_b));
    assert_eq!(server.session_count(), 2);
    server.shutdown();
}

#[tokio::test]
async fn disconnect_removes_session() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let (client, _send, _recv, id) = handshake_ok(server.addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(server.contains(id));
    client.conn.close(0u32.into(), b"bye");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!server.contains(id));
    assert_eq!(server.session_count(), 0);
    server.shutdown();
}

#[tokio::test]
async fn repeated_hello_after_welcome_disconnects() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let (_client, mut send, mut recv, id) = handshake_ok(server.addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    write_hello(&mut send, PROTOCOL_VERSION, "again").await;
    expect_disconnect(&mut recv, DisconnectReasonCode::UnexpectedMessage).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!server.contains(id));
    server.shutdown();
}

#[tokio::test]
async fn datagram_ping_echoes_nonce() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let (client, _send, _recv, _id) = handshake_ok(server.addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let ping = encode_client_datagram(7).expect("ping");
    client
        .conn
        .send_datagram(ping.into())
        .expect("send datagram");
    let pong = timeout(Duration::from_secs(2), client.conn.read_datagram())
        .await
        .expect("pong wait")
        .expect("pong");
    assert_eq!(decode_client_datagram(&pong).ok(), None);
    let decoded = purgatory_protocol::decode_server_datagram(&pong).expect("decode pong");
    assert_eq!(
        decoded,
        purgatory_protocol::ServerDatagram::Pong { nonce: 7 }
    );
    server.shutdown();
}

/// A fully handshaked test peer, kept alive as one value. `_recv` is held so
/// the server keeps seeing an open control stream.
struct LivePeer {
    client: TestClient,
    send: SendStream,
    _recv: RecvStream,
    id: ConnectionId,
}

async fn handshake_peer(addr: SocketAddr) -> LivePeer {
    let (client, send, recv, id) = handshake_ok(addr).await;
    LivePeer {
        client,
        send,
        _recv: recv,
        id,
    }
}

/// Peer with an explicit idle timeout, for server-loss scenarios.
async fn handshake_peer_idle(addr: SocketAddr, idle: Duration) -> LivePeer {
    let endpoint = test_client_endpoint_idle(idle);
    let conn = endpoint
        .connect(addr, "localhost")
        .expect("connect start")
        .await
        .expect("connect");
    let client = TestClient {
        _endpoint: endpoint,
        conn,
    };
    let (mut send, mut recv) = client.conn.open_bi().await.expect("open_bi");
    write_hello(&mut send, PROTOCOL_VERSION, "soak").await;
    let id = match read_server_control(&mut recv).await {
        Ok(ServerControl::Welcome(welcome)) => welcome.connection_id,
        other => panic!("expected welcome, got {other:?}"),
    };
    LivePeer {
        client,
        send,
        _recv: recv,
        id,
    }
}

/// Fallible variant for chaos scenarios where admission may refuse the peer.
async fn try_handshake_peer(addr: SocketAddr) -> Option<LivePeer> {
    let endpoint = test_client_endpoint();
    let conn = endpoint.connect(addr, "localhost").ok()?.await.ok()?;
    let client = TestClient {
        _endpoint: endpoint,
        conn,
    };
    let (mut send, mut recv) = client.conn.open_bi().await.ok()?;
    if !write_hello_best_effort(&mut send, PROTOCOL_VERSION, "chaos").await {
        return None;
    }
    match timeout(Duration::from_secs(3), read_server_control(&mut recv)).await {
        Ok(Ok(ServerControl::Welcome(welcome))) => Some(LivePeer {
            client,
            send,
            _recv: recv,
            id: welcome.connection_id,
        }),
        _ => None,
    }
}

async fn wait_until(mut pred: impl FnMut() -> bool, limit: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        if pred() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return pred();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_session_count(server: &TestServer, n: usize) {
    let ok = wait_until(|| server.session_count() == n, Duration::from_secs(3)).await;
    assert!(ok, "expected {n} sessions, got {}", server.session_count());
}

async fn handshake_many(
    addr: SocketAddr,
    n: usize,
) -> Vec<(TestClient, SendStream, RecvStream, ConnectionId)> {
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..n {
        set.spawn(async move { handshake_ok(addr).await });
    }
    let mut out = Vec::new();
    while let Some(joined) = set.join_next().await {
        out.push(joined.expect("handshake task"));
    }
    out
}

fn close_clients(clients: Vec<(TestClient, SendStream, RecvStream, ConnectionId)>) {
    for (client, send, recv, _) in clients {
        client.conn.close(0u32.into(), b"bye");
        drop((send, recv, client));
    }
}

#[tokio::test]
async fn five_simultaneous_clients_get_unique_ids() {
    let server = TestServer::spawn(Duration::from_secs(5)).await;
    let clients = handshake_many(server.addr, 5).await;
    let mut ids = HashSet::new();
    for (_, _, _, id) in &clients {
        assert!(ids.insert(*id), "duplicate ConnectionId {id}");
        assert!(server.contains(*id));
    }
    assert_eq!(ids.len(), 5);
    wait_session_count(&server, 5).await;
    close_clients(clients);
    wait_session_count(&server, 0).await;
    server.shutdown();
}

#[tokio::test]
async fn ten_simultaneous_clients_get_unique_ids() {
    let server = TestServer::spawn(Duration::from_secs(8)).await;
    let clients = handshake_many(server.addr, 10).await;
    let mut ids = HashSet::new();
    for (_, _, _, id) in &clients {
        assert!(ids.insert(*id), "duplicate ConnectionId {id}");
    }
    assert_eq!(ids.len(), 10);
    wait_session_count(&server, 10).await;
    close_clients(clients);
    wait_session_count(&server, 0).await;
    server.shutdown();
}

#[tokio::test]
async fn simultaneous_disconnects_clean_all_sessions() {
    let server = TestServer::spawn(Duration::from_secs(5)).await;
    let clients = handshake_many(server.addr, 5).await;
    wait_session_count(&server, 5).await;
    close_clients(clients);
    wait_session_count(&server, 0).await;
    assert!(wait_until(|| server.inflight_tasks() == 0, Duration::from_secs(3)).await);
    server.shutdown();
}

#[tokio::test]
async fn malformed_client_does_not_affect_healthy_peer() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let (healthy, _send, _recv, id) = handshake_ok(server.addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(server.contains(id));

    let bad = connect(server.addr).await;
    let (mut send, _recv) = bad.conn.open_bi().await.expect("open_bi");
    send.write_all(&encode_frame(&[0xff]).expect("frame"))
        .await
        .expect("garbage");
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(server.contains(id));
    assert_eq!(server.session_count(), 1);
    drop((healthy, bad, send));
    server.shutdown();
}

#[tokio::test]
async fn stalled_hello_does_not_block_fast_client() {
    let server = TestServer::spawn(Duration::from_millis(800)).await;
    let _stalled = connect(server.addr).await;
    let started = tokio::time::Instant::now();
    let (_fast, _send, _recv, fast_id) = handshake_ok(server.addr).await;
    assert!(
        started.elapsed() < Duration::from_millis(400),
        "fast client waited on stalled handshake: {:?}",
        started.elapsed()
    );
    assert!(server.contains(fast_id));
    assert_eq!(server.session_count(), 1);
    tokio::time::sleep(Duration::from_millis(1000)).await;
    assert!(server.contains(fast_id));
    assert_eq!(server.session_count(), 1);
    server.shutdown();
}

#[tokio::test]
async fn churn_20_connect_disconnect_returns_to_baseline() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let mut seen = HashSet::new();
    for _ in 0..20 {
        let (client, send, recv, id) = handshake_ok(server.addr).await;
        assert!(seen.insert(id));
        wait_session_count(&server, 1).await;
        client.conn.close(0u32.into(), b"bye");
        drop((send, recv, client));
        wait_session_count(&server, 0).await;
    }
    assert!(wait_until(|| server.inflight_tasks() == 0, Duration::from_secs(3)).await);
    server.shutdown();
}

#[tokio::test]
#[ignore = "optional 100-cycle soak; default suite uses churn_20"]
async fn churn_100_connect_disconnect_optional() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    for _ in 0..100 {
        let (client, send, recv, _id) = handshake_ok(server.addr).await;
        wait_session_count(&server, 1).await;
        client.conn.close(0u32.into(), b"bye");
        drop((send, recv, client));
        wait_session_count(&server, 0).await;
    }
    server.shutdown();
}

#[tokio::test]
async fn five_clients_eight_churn_cycles() {
    let server = TestServer::spawn(Duration::from_secs(8)).await;
    for _ in 0..8 {
        let clients = handshake_many(server.addr, 5).await;
        let mut ids = HashSet::new();
        for (_, _, _, id) in &clients {
            assert!(ids.insert(*id));
        }
        wait_session_count(&server, 5).await;
        close_clients(clients);
        wait_session_count(&server, 0).await;
    }
    assert!(wait_until(|| server.inflight_tasks() == 0, Duration::from_secs(3)).await);
    server.shutdown();
}

#[tokio::test]
#[ignore = "optional 5x20 multi-client soak; default suite uses five_clients_eight_churn_cycles"]
async fn five_clients_twenty_churn_cycles_optional() {
    let server = TestServer::spawn(Duration::from_secs(8)).await;
    for _ in 0..20 {
        let clients = handshake_many(server.addr, 5).await;
        wait_session_count(&server, 5).await;
        close_clients(clients);
        wait_session_count(&server, 0).await;
    }
    server.shutdown();
}

#[tokio::test]
async fn ungraceful_drop_eventually_cleans_session() {
    // Simulates abrupt peer disappearance by dropping the QUIC connection
    // without an application close. True OS process kill is not used here;
    // the explicit idle timeout (`IDLE_TIMEOUT` = 15 s) applies when the
    // peer vanishes without a close frame. Drop of `Connection` still notifies
    // the server promptly in this harness.
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let (client, send, recv, id) = handshake_ok(server.addr).await;
    wait_session_count(&server, 1).await;
    drop((client, send, recv));
    wait_session_count(&server, 0).await;
    assert!(!server.contains(id));
    server.shutdown();
}

#[tokio::test]
async fn unknown_datagram_does_not_panic_or_drop_session() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let (client, _send, _recv, id) = handshake_ok(server.addr).await;
    wait_session_count(&server, 1).await;
    client.conn.send_datagram(vec![0xff_u8, 0x00].into()).ok();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(server.contains(id));
    server.shutdown();
}

#[tokio::test]
async fn idle_timeout_cleans_forgotten_peer() {
    let idle = Duration::from_millis(400);
    let server = TestServer::spawn_with(Duration::from_secs(2), idle).await;
    let endpoint = test_client_endpoint_idle(idle);
    let conn = endpoint
        .connect(server.addr, "localhost")
        .expect("connect start")
        .await
        .expect("connect");
    let (mut send, mut recv) = conn.open_bi().await.expect("open_bi");
    write_hello(&mut send, PROTOCOL_VERSION, "idle").await;
    match read_server_control(&mut recv).await {
        Ok(ServerControl::Welcome(_)) => {}
        other => panic!("expected welcome, got {other:?}"),
    }
    wait_session_count(&server, 1).await;
    std::mem::forget(send);
    std::mem::forget(recv);
    std::mem::forget(conn);
    std::mem::forget(endpoint);
    assert!(
        wait_until(|| server.session_count() == 0, Duration::from_secs(5)).await,
        "idle timeout should drop the forgotten peer"
    );
    assert!(
        server
            .stats
            .transport_loss
            .load(std::sync::atomic::Ordering::Relaxed)
            >= 1
    );
    server.shutdown();
}

#[tokio::test]
async fn graceful_shutdown_close_code_is_server_shutdown() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let (client, _send, _recv, _id) = handshake_ok(server.addr).await;
    wait_session_count(&server, 1).await;
    server.endpoint.close(
        u32::from(DisconnectReasonCode::ServerShutdown.as_u8()).into(),
        b"shutdown",
    );
    match client.conn.closed().await {
        quinn::ConnectionError::ApplicationClosed(close) => {
            assert_eq!(
                u64::from(close.error_code),
                u64::from(DisconnectReasonCode::ServerShutdown.as_u8())
            );
        }
        other => panic!("expected ServerShutdown application close, got {other:?}"),
    }
    server.accept.abort();
}

#[tokio::test]
async fn healthy_client_unaffected_by_peer_handshake_timeout() {
    let server = TestServer::spawn(Duration::from_millis(400)).await;
    let (healthy, _s, _r, id) = handshake_ok(server.addr).await;
    wait_session_count(&server, 1).await;
    let _stalled = connect(server.addr).await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(server.contains(id));
    assert_eq!(server.session_count(), 1);
    drop(healthy);
    server.shutdown();
}

async fn write_unknown_control(send: &mut SendStream) {
    send.write_all(&encode_frame(&[99, 1, 2, 3]).expect("frame"))
        .await
        .ok();
}

#[tokio::test]
async fn oversized_frame_increments_oversized_counter_and_cleans_session() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let client = connect(server.addr).await;
    let (mut send, _recv) = client.conn.open_bi().await.expect("open_bi");
    let huge = (MAX_CONTROL_MESSAGE_BYTES + 1).to_le_bytes();
    send.write_all(&huge).await.expect("write huge len");
    assert!(
        wait_until(
            || {
                server
                    .stats
                    .rejected_oversized
                    .load(std::sync::atomic::Ordering::Relaxed)
                    >= 1
            },
            Duration::from_secs(2)
        )
        .await
    );
    assert_eq!(server.session_count(), 0);
    server.shutdown();
}

#[tokio::test]
async fn malformed_churn_releases_capacity_then_healthy_connects() {
    let server = TestServer::spawn(Duration::from_secs(4)).await;
    for _ in 0..24 {
        let client = connect(server.addr).await;
        if let Ok((mut send, _recv)) = client.conn.open_bi().await {
            let _ = send.write_all(&encode_frame(&[0xff]).expect("frame")).await;
        }
        drop(client);
    }
    assert!(wait_until(|| server.session_count() == 0, Duration::from_secs(3)).await);
    assert!(wait_until(|| server.inflight_tasks() == 0, Duration::from_secs(3)).await);
    let (_ok, _s, _r, id) = handshake_ok(server.addr).await;
    wait_session_count(&server, 1).await;
    assert!(server.contains(id));
    server.shutdown();
}

#[tokio::test]
async fn concurrent_malformed_peers_do_not_block_healthy() {
    let server = TestServer::spawn(Duration::from_secs(4)).await;
    let (healthy, _s, _r, id) = handshake_ok(server.addr).await;
    wait_session_count(&server, 1).await;
    let mut bad = Vec::new();
    for _ in 0..4 {
        let client = connect(server.addr).await;
        if let Ok((mut send, recv)) = client.conn.open_bi().await {
            let _ = send.write_all(&encode_frame(&[0xff]).expect("frame")).await;
            bad.push((client, send, recv));
        }
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(server.contains(id));
    assert_eq!(server.session_count(), 1);
    drop((healthy, bad));
    server.shutdown();
}

#[tokio::test]
async fn admission_cap_refuses_excess_then_releases() {
    let mut abuse = NetworkAbuseConfig::DEV;
    abuse.max_inflight_connection_tasks = 2;
    abuse.handshake_timeout = Duration::from_secs(4);
    let server = TestServer::spawn_abuse(
        Duration::from_secs(4),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    let _a = connect(server.addr).await;
    let _b = connect(server.addr).await;
    assert!(wait_until(|| server.inflight_tasks() == 2, Duration::from_secs(2)).await);
    let third = test_client_endpoint()
        .connect(server.addr, "localhost")
        .expect("connect start")
        .await;
    assert!(third.is_err(), "excess incoming should be refused");
    assert!(
        server
            .stats
            .admission_refused
            .load(std::sync::atomic::Ordering::Relaxed)
            >= 1
    );
    assert!(
        server
            .stats
            .max_inflight
            .load(std::sync::atomic::Ordering::Relaxed)
            <= 2
    );
    drop((_a, _b));
    assert!(wait_until(|| server.inflight_tasks() == 0, Duration::from_secs(3)).await);
    let (_ok, _s, _r, id) = handshake_ok(server.addr).await;
    wait_session_count(&server, 1).await;
    assert!(server.contains(id));
    server.shutdown();
}

#[tokio::test]
async fn invalid_datagram_budget_disconnects_only_offender() {
    let mut abuse = NetworkAbuseConfig::DEV;
    abuse.invalid_datagram_budget = 3;
    let server = TestServer::spawn_abuse(
        Duration::from_secs(3),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    let (healthy, _hs, _hr, hid) = handshake_ok(server.addr).await;
    let (bad, _bs, _br, bid) = handshake_ok(server.addr).await;
    wait_session_count(&server, 2).await;
    for _ in 0..3 {
        bad.conn.send_datagram(vec![0xff_u8, 0x00].into()).ok();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(wait_until(|| !server.contains(bid), Duration::from_secs(2)).await);
    assert!(server.contains(hid));
    drop(healthy);
    server.shutdown();
}

#[tokio::test]
async fn control_rate_limit_disconnects_flood_not_healthy_peer() {
    let mut abuse = NetworkAbuseConfig::DEV;
    abuse.control_messages_per_window = 4;
    abuse.rate_drops_before_disconnect = 8;
    let server = TestServer::spawn_abuse(
        Duration::from_secs(3),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    let (healthy, _hs, _hr, hid) = handshake_ok(server.addr).await;
    let (bad, mut send, _br, bid) = handshake_ok(server.addr).await;
    wait_session_count(&server, 2).await;
    for _ in 0..24 {
        write_unknown_control(&mut send).await;
    }
    assert!(wait_until(|| !server.contains(bid), Duration::from_secs(2)).await);
    assert!(
        server
            .stats
            .rate_limited
            .load(std::sync::atomic::Ordering::Relaxed)
            >= 1
    );
    assert!(server.contains(hid));
    drop((healthy, bad, send));
    server.shutdown();
}

#[tokio::test]
async fn malformed_control_budget_is_per_connection() {
    let mut abuse = NetworkAbuseConfig::DEV;
    abuse.malformed_control_budget = 3;
    abuse.control_messages_per_window = 100;
    abuse.rate_drops_before_disconnect = 100;
    let server = TestServer::spawn_abuse(
        Duration::from_secs(3),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    let (_c, mut send, _r, id) = handshake_ok(server.addr).await;
    wait_session_count(&server, 1).await;
    for _ in 0..3 {
        write_unknown_control(&mut send).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(wait_until(|| !server.contains(id), Duration::from_secs(2)).await);
    let (_ok, _s, _r, id2) = handshake_ok(server.addr).await;
    wait_session_count(&server, 1).await;
    assert_ne!(id, id2);
    server.shutdown();
}

// ---------------------------------------------------------------------------
// Phase 5.0F — soak / stress / chaos / recovery
//
// These tests prove convergence, not capacity. A localhost pass here says
// nothing about supported player counts: there is no gameplay state,
// replication, AI, persistence, or bandwidth cost yet.
//
// CI scale is deliberately small. Heavier variants are `#[ignore]`; see
// `scripts/network_soak.ps1`.
// ---------------------------------------------------------------------------

/// Deterministic PRNG. Chaos scenarios must be reproducible from a seed.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 11
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }
}

/// CI seed set. Extended soaks add more; system entropy is never used.
const CHAOS_SEEDS: [u64; 3] = [0x1, 0x00C0_FFEE, 0xDEAD_BEEF];

async fn try_connect(addr: SocketAddr) -> Option<TestClient> {
    let endpoint = test_client_endpoint();
    let conn = endpoint.connect(addr, "localhost").ok()?.await.ok()?;
    Some(TestClient {
        _endpoint: endpoint,
        conn,
    })
}

/// Waits for the server's rejection instead of dropping the peer immediately,
/// so bad-peer scenarios are deterministic rather than a write/close race.
async fn await_server_close(client: &TestClient) {
    let _ = timeout(Duration::from_secs(3), client.conn.closed()).await;
}

async fn oversized_frame_peer(addr: SocketAddr) {
    let Some(client) = try_connect(addr).await else {
        return;
    };
    if let Ok((mut send, _recv)) = client.conn.open_bi().await {
        let _ = send
            .write_all(&(MAX_CONTROL_MESSAGE_BYTES + 1).to_le_bytes())
            .await;
        await_server_close(&client).await;
    }
}

async fn wrong_version_peer(addr: SocketAddr) {
    let Some(client) = try_connect(addr).await else {
        return;
    };
    if let Ok((mut send, _recv)) = client.conn.open_bi().await
        && write_hello_best_effort(&mut send, PROTOCOL_VERSION + 7, "chaos").await
    {
        await_server_close(&client).await;
    }
}

async fn malformed_handshake_peer(addr: SocketAddr) {
    let Some(client) = try_connect(addr).await else {
        return;
    };
    if let Ok((mut send, _recv)) = client.conn.open_bi().await
        && let Ok(frame) = encode_frame(&[0xff])
        && send.write_all(&frame).await.is_ok()
    {
        await_server_close(&client).await;
    }
}

/// Test-only scenario vocabulary. Not a scripting language, not shipped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChaosOp {
    ConnectNormal,
    ConnectStalled,
    ConnectWrongVersion,
    ConnectMalformed,
    SendMalformedControl,
    SendInvalidDatagram,
    SendOversizedFrame,
    RepeatHello,
    DisconnectGraceful,
    DropAbrupt,
    Wait,
}

const CHAOS_OPS: [ChaosOp; 11] = [
    ChaosOp::ConnectNormal,
    ChaosOp::ConnectStalled,
    ChaosOp::ConnectWrongVersion,
    ChaosOp::ConnectMalformed,
    ChaosOp::SendMalformedControl,
    ChaosOp::SendInvalidDatagram,
    ChaosOp::SendOversizedFrame,
    ChaosOp::RepeatHello,
    ChaosOp::DisconnectGraceful,
    ChaosOp::DropAbrupt,
    ChaosOp::Wait,
];

/// Keeps localhost pressure bounded so the runner tests policy, not OS limits.
const MAX_CHAOS_LIVE: usize = 4;
const MAX_CHAOS_STALLED: usize = 2;

struct ChaosRunner<'a> {
    server: &'a TestServer,
    seed: u64,
    rng: Lcg,
    live: Vec<LivePeer>,
    stalled: Vec<TestClient>,
    index: usize,
}

impl<'a> ChaosRunner<'a> {
    fn new(server: &'a TestServer, seed: u64) -> Self {
        Self {
            server,
            seed,
            rng: Lcg::new(seed),
            live: Vec::new(),
            stalled: Vec::new(),
            index: 0,
        }
    }

    /// Failure reports carry the seed and operation index, never payloads.
    fn context(&self, op: ChaosOp) -> String {
        format!("chaos seed=0x{:X} op#{}={op:?}", self.seed, self.index)
    }

    async fn run(&mut self, ops: usize) {
        for _ in 0..ops {
            let op = CHAOS_OPS[self.rng.below(CHAOS_OPS.len())];
            self.index += 1;
            self.step(op).await;
            let cap = self.server.admission_cap();
            let gauges = self.server.gauges();
            assert!(
                gauges.inflight <= cap as u64,
                "[{}] inflight {} exceeded cap {cap} | {}",
                self.context(op),
                gauges.inflight,
                self.server.report()
            );
            assert!(
                gauges.sessions <= cap,
                "[{}] sessions {} exceeded cap {cap} | {}",
                self.context(op),
                gauges.sessions,
                self.server.report()
            );
        }
    }

    async fn step(&mut self, op: ChaosOp) {
        let addr = self.server.addr;
        match op {
            ChaosOp::ConnectNormal => {
                if self.live.len() >= MAX_CHAOS_LIVE {
                    self.close_one_live();
                }
                if let Some(peer) = try_handshake_peer(addr).await {
                    self.live.push(peer);
                }
            }
            ChaosOp::ConnectStalled => {
                if self.stalled.len() >= MAX_CHAOS_STALLED {
                    self.stalled.remove(0);
                }
                if let Some(client) = try_connect(addr).await {
                    self.stalled.push(client);
                }
            }
            ChaosOp::ConnectWrongVersion => wrong_version_peer(addr).await,
            ChaosOp::ConnectMalformed => malformed_handshake_peer(addr).await,
            ChaosOp::SendOversizedFrame => oversized_frame_peer(addr).await,
            ChaosOp::SendMalformedControl => {
                if let Some(idx) = self.pick_live() {
                    write_unknown_control(&mut self.live[idx].send).await;
                }
            }
            ChaosOp::SendInvalidDatagram => {
                if let Some(idx) = self.pick_live() {
                    let _ = self.live[idx]
                        .client
                        .conn
                        .send_datagram(vec![0xff_u8, 0x00].into());
                }
            }
            ChaosOp::RepeatHello => {
                if let Some(idx) = self.pick_live() {
                    let mut peer = self.live.remove(idx);
                    // A second Hello is a severe violation: the server closes
                    // this peer, so it stops being a live peer here.
                    write_hello_best_effort(&mut peer.send, PROTOCOL_VERSION, "again").await;
                }
            }
            ChaosOp::DisconnectGraceful => self.close_one_live(),
            ChaosOp::DropAbrupt => {
                if !self.live.is_empty() {
                    let idx = self.rng.below(self.live.len());
                    drop(self.live.remove(idx));
                }
            }
            ChaosOp::Wait => tokio::time::sleep(Duration::from_millis(5)).await,
        }
    }

    fn pick_live(&mut self) -> Option<usize> {
        if self.live.is_empty() {
            return None;
        }
        Some(self.rng.below(self.live.len()))
    }

    fn close_one_live(&mut self) {
        if self.live.is_empty() {
            return;
        }
        let idx = self.rng.below(self.live.len());
        let peer = self.live.remove(idx);
        peer.client.conn.close(0u32.into(), b"bye");
    }

    /// Drains every peer, waits for baseline, then runs the healthy probe.
    async fn finish(mut self) {
        let scenario = format!("chaos seed=0x{:X} ops={}", self.seed, self.index);
        for peer in std::mem::take(&mut self.live) {
            peer.client.conn.close(0u32.into(), b"bye");
        }
        self.stalled.clear();
        self.server
            .wait_until_baseline(&scenario, Duration::from_secs(10))
            .await;
        self.server.assert_admission_bounds(&scenario);
        self.server.assert_healthy_probe(&scenario).await;
    }
}

/// How a test server stops. `Abrupt` drops the runtime so every connection
/// task disappears at once: the closest safe analogue of a lost server.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StopMode {
    Graceful,
    Abrupt,
}

/// A server on its own thread and runtime, so it can be made to vanish.
struct ServerProcess {
    addr: SocketAddr,
    sessions: Arc<std::sync::Mutex<SessionTable>>,
    kill: Option<tokio::sync::oneshot::Sender<StopMode>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ServerProcess {
    fn start(idle: Duration, abuse: NetworkAbuseConfig) -> Self {
        let (addr_tx, addr_rx) = std::sync::mpsc::channel();
        let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<StopMode>();
        let thread = std::thread::Builder::new()
            .name("purgatory-test-server".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("server runtime");
                runtime.block_on(async move {
                    install_crypto_provider().expect("crypto");
                    let mut config = ServerEndpointConfig::ephemeral();
                    config.idle_timeout = idle;
                    config.abuse = abuse;
                    let bound = endpoint::bind(&config).expect("bind");
                    let _ = addr_tx.send((bound.local_addr(), bound.sessions.clone()));
                    let endpoint = bound.endpoint.clone();
                    let accept = tokio::spawn(accept_loop(bound, abuse, None, None));
                    let mode = kill_rx.await.unwrap_or(StopMode::Abrupt);
                    if mode == StopMode::Graceful {
                        endpoint.close(
                            u32::from(DisconnectReasonCode::ServerShutdown.as_u8()).into(),
                            b"shutdown",
                        );
                        tokio::time::sleep(Duration::from_millis(150)).await;
                    }
                    accept.abort();
                });
            })
            .expect("server thread");
        let (addr, sessions) = addr_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("server bind addr");
        Self {
            addr,
            sessions,
            kill: Some(kill_tx),
            thread: Some(thread),
        }
    }

    fn session_count(&self) -> usize {
        lock_sessions(&self.sessions).len()
    }

    fn stop(&mut self, mode: StopMode) {
        if let Some(kill) = self.kill.take() {
            let _ = kill.send(mode);
        }
        if let Some(thread) = self.thread.take() {
            thread.join().expect("server thread join");
        }
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        self.stop(StopMode::Abrupt);
    }
}

fn soak_abuse(cap: usize) -> NetworkAbuseConfig {
    let mut abuse = NetworkAbuseConfig::DEV;
    abuse.max_inflight_connection_tasks = cap;
    abuse
}

/// One sequential cycle: connect, handshake, ping, disconnect, converge.
async fn churn_cycle(server: &TestServer, scenario: &str) -> ConnectionId {
    let peer = timeout(Duration::from_secs(5), handshake_peer(server.addr))
        .await
        .unwrap_or_else(|_| panic!("[{scenario}] handshake timed out | {}", server.report()));
    let id = peer.id;
    assert!(
        wait_until(|| server.contains(id), Duration::from_secs(3)).await,
        "[{scenario}] session {id} missing | {}",
        server.report()
    );
    peer.client.conn.close(0u32.into(), b"bye");
    drop(peer);
    assert!(
        wait_until(|| !server.contains(id), Duration::from_secs(5)).await,
        "[{scenario}] session {id} not cleaned | {}",
        server.report()
    );
    id
}

async fn sequential_churn(server: &TestServer, scenario: &str, cycles: usize) {
    let mut seen = HashSet::new();
    for _ in 0..cycles {
        let id = churn_cycle(server, scenario).await;
        assert!(
            seen.insert(id),
            "[{scenario}] ConnectionId {id} reused | {}",
            server.report()
        );
        assert_ne!(id.get(), 0, "[{scenario}] allocator issued 0");
    }
    assert_eq!(seen.len(), cycles);
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_healthy_probe(scenario).await;
}

#[tokio::test]
async fn sequential_churn_50_returns_to_baseline() {
    let server = TestServer::spawn(Duration::from_secs(3)).await;
    sequential_churn(&server, "sequential churn 50", 50).await;
    assert!(
        server.stat(|s| &s.total_accepted) >= 51,
        "{}",
        server.report()
    );
    assert_eq!(server.peak_sessions(), 1, "{}", server.report());
    server.shutdown();
}

#[tokio::test]
async fn rapid_reconnect_soak_leaves_no_stale_state() {
    // No artificial delay between disconnect and the next connect.
    let server = TestServer::spawn(Duration::from_secs(3)).await;
    let scenario = "rapid reconnect 30";
    let mut seen = HashSet::new();
    for _ in 0..30 {
        let peer = handshake_peer(server.addr).await;
        assert!(seen.insert(peer.id), "reused id {}", peer.id);
        peer.client.conn.close(0u32.into(), b"bye");
        drop(peer);
    }
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_admission_bounds(scenario);
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
async fn multi_client_churn_returns_to_baseline() {
    let server = TestServer::spawn(Duration::from_secs(8)).await;
    let scenario = "multi-client churn 4x5";
    for _ in 0..5 {
        let clients = handshake_many(server.addr, 4).await;
        let mut ids = HashSet::new();
        for (_, _, _, id) in &clients {
            assert!(ids.insert(*id), "duplicate ConnectionId {id}");
        }
        wait_session_count(&server, 4).await;
        close_clients(clients);
        wait_session_count(&server, 0).await;
    }
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    assert_eq!(server.peak_sessions(), 4, "{}", server.report());
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
async fn moderate_concurrent_healthy_clients_stay_functional() {
    // Eight concurrent localhost peers below the development admission cap.
    // This is a boundedness check, NOT a statement about player capacity.
    let server = TestServer::spawn(Duration::from_secs(8)).await;
    let scenario = "8 concurrent healthy";
    let mut peers = Vec::new();
    for _ in 0..8 {
        peers.push(handshake_peer(server.addr).await);
    }
    wait_session_count(&server, 8).await;
    let mut ids = HashSet::new();
    for (n, peer) in peers.iter().enumerate() {
        assert!(ids.insert(peer.id));
        let nonce = 100 + n as u64;
        peer.client
            .conn
            .send_datagram(encode_client_datagram(nonce).expect("ping").into())
            .expect("ping");
        let pong = timeout(Duration::from_secs(3), peer.client.conn.read_datagram())
            .await
            .expect("pong wait")
            .expect("pong");
        assert_eq!(
            purgatory_protocol::decode_server_datagram(&pong).expect("pong"),
            purgatory_protocol::ServerDatagram::Pong { nonce }
        );
    }
    for peer in &peers {
        peer.client.conn.close(0u32.into(), b"bye");
    }
    drop(peers);
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    assert_eq!(server.peak_sessions(), 8, "{}", server.report());
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
async fn admission_fill_refuse_release_repeats() {
    // Proves permit reuse across rounds, not just one-shot refusal.
    let cap = 4;
    let server = TestServer::spawn_abuse(
        Duration::from_secs(4),
        purgatory_protocol::IDLE_TIMEOUT,
        soak_abuse(cap),
    )
    .await;
    let scenario = "admission fill/refuse/release x3";
    for round in 0..3 {
        let mut held = Vec::new();
        for _ in 0..cap {
            held.push(connect(server.addr).await);
        }
        assert!(
            wait_until(
                || server.gauges().admission_in_use == cap,
                Duration::from_secs(3)
            )
            .await,
            "[{scenario}] round {round} did not fill the cap | {}",
            server.report()
        );
        let refused_before = server.stat(|s| &s.admission_refused);
        let overflow = test_client_endpoint()
            .connect(server.addr, "localhost")
            .expect("connect start")
            .await;
        assert!(
            overflow.is_err(),
            "[{scenario}] round {round} admitted an over-cap peer | {}",
            server.report()
        );
        assert!(
            wait_until(
                || server.stat(|s| &s.admission_refused) > refused_before,
                Duration::from_secs(3)
            )
            .await,
            "[{scenario}] round {round} missing refusal counter | {}",
            server.report()
        );
        drop(held);
        server
            .wait_until_baseline(scenario, Duration::from_secs(10))
            .await;
    }
    server.assert_admission_bounds(scenario);
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
async fn stalled_handshake_pressure_recovers_after_timeout() {
    let cap = 4;
    let mut abuse = soak_abuse(cap);
    abuse.handshake_timeout = Duration::from_millis(400);
    let server = TestServer::spawn_abuse(
        Duration::from_millis(400),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    let scenario = "stalled handshake pressure";
    let mut stalled = Vec::new();
    for _ in 0..cap {
        stalled.push(connect(server.addr).await);
    }
    assert!(
        wait_until(
            || server.gauges().admission_in_use == cap,
            Duration::from_secs(3)
        )
        .await,
        "[{scenario}] stalled peers did not occupy the cap | {}",
        server.report()
    );
    // Each stalled peer times out independently and returns its permit.
    assert!(
        wait_until(
            || server.stat(|s| &s.handshake_timeout) >= cap as u64,
            Duration::from_secs(5)
        )
        .await,
        "[{scenario}] handshake timeouts missing | {}",
        server.report()
    );
    assert_eq!(server.session_count(), 0, "{}", server.report());
    drop(stalled);
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
async fn mixed_healthy_and_stalled_peers_stay_isolated() {
    let mut abuse = soak_abuse(8);
    abuse.handshake_timeout = Duration::from_millis(400);
    let server = TestServer::spawn_abuse(
        Duration::from_millis(400),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    let scenario = "mixed healthy + stalled";
    let healthy = handshake_peer(server.addr).await;
    let id = healthy.id;
    wait_session_count(&server, 1).await;
    let mut stalled = Vec::new();
    for _ in 0..3 {
        stalled.push(connect(server.addr).await);
    }
    // A healthy peer must still complete while stalled peers occupy tasks.
    let started = tokio::time::Instant::now();
    let second = handshake_peer(server.addr).await;
    assert!(
        started.elapsed() < Duration::from_millis(400),
        "[{scenario}] healthy handshake serialized behind stalled peers: {:?}",
        started.elapsed()
    );
    assert!(
        wait_until(
            || server.stat(|s| &s.handshake_timeout) >= 3,
            Duration::from_secs(5)
        )
        .await,
        "[{scenario}] stalled peers did not time out | {}",
        server.report()
    );
    assert!(server.contains(id), "{}", server.report());
    assert!(server.contains(second.id), "{}", server.report());
    assert_eq!(server.session_count(), 2, "{}", server.report());
    healthy.client.conn.close(0u32.into(), b"bye");
    second.client.conn.close(0u32.into(), b"bye");
    drop((healthy, second, stalled));
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
async fn malformed_handshake_churn_does_not_degrade_later_clients() {
    let server = TestServer::spawn(Duration::from_secs(4)).await;
    let scenario = "malformed handshake churn 40";
    let rejected_before = server.stat(|s| &s.total_rejected);
    for _ in 0..40 {
        malformed_handshake_peer(server.addr).await;
    }
    assert!(
        wait_until(
            || server.stat(|s| &s.total_rejected) > rejected_before,
            Duration::from_secs(5)
        )
        .await,
        "[{scenario}] rejections not counted | {}",
        server.report()
    );
    assert_eq!(server.peak_sessions(), 0, "{}", server.report());
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

/// Deterministic malformed mix (§18). Every peer goes through the real parser.
#[tokio::test]
async fn seeded_malformed_mix_isolates_bad_peers() {
    let mut abuse = soak_abuse(8);
    abuse.handshake_timeout = Duration::from_millis(400);
    let server = TestServer::spawn_abuse(
        Duration::from_millis(400),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    for seed in CHAOS_SEEDS {
        let scenario = format!("malformed mix seed=0x{seed:X}");
        let healthy = handshake_peer(server.addr).await;
        let id = healthy.id;
        let mut rng = Lcg::new(seed);
        let mut stalled = Vec::new();
        for step in 0..12 {
            match rng.below(6) {
                0 => malformed_handshake_peer(server.addr).await,
                1 => wrong_version_peer(server.addr).await,
                2 => oversized_frame_peer(server.addr).await,
                3 => {
                    if let Some(peer) = try_handshake_peer(server.addr).await {
                        // Repeated Hello: severe, this peer alone is closed.
                        let mut peer = peer;
                        write_hello_best_effort(&mut peer.send, PROTOCOL_VERSION, "again").await;
                    }
                }
                4 => {
                    if let Some(client) = try_connect(server.addr).await {
                        stalled.push(client);
                    }
                }
                _ => {
                    if let Some(peer) = try_handshake_peer(server.addr).await {
                        let mut peer = peer;
                        write_unknown_control(&mut peer.send).await;
                        let _ = peer.client.conn.send_datagram(vec![0xff_u8, 0x00].into());
                    }
                }
            }
            assert!(
                server.contains(id),
                "[{scenario}] step {step} disturbed the healthy peer | {}",
                server.report()
            );
        }
        healthy.client.conn.close(0u32.into(), b"bye");
        drop((healthy, stalled));
        server
            .wait_until_baseline(&scenario, Duration::from_secs(10))
            .await;
        server.assert_admission_bounds(&scenario);
        server.assert_healthy_probe(&scenario).await;
    }
    server.shutdown();
}

#[tokio::test]
async fn invalid_datagram_budget_resets_for_each_connection() {
    let mut abuse = NetworkAbuseConfig::DEV;
    abuse.invalid_datagram_budget = 3;
    let server = TestServer::spawn_abuse(
        Duration::from_secs(3),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    let scenario = "invalid datagram budget x3 connections";
    let healthy = handshake_peer(server.addr).await;
    for round in 0..3 {
        let bad = handshake_peer(server.addr).await;
        let id = bad.id;
        for _ in 0..3 {
            let _ = bad.client.conn.send_datagram(vec![0xff_u8, 0x00].into());
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            wait_until(|| !server.contains(id), Duration::from_secs(3)).await,
            "[{scenario}] round {round} offender survived its budget | {}",
            server.report()
        );
        assert!(
            server.contains(healthy.id),
            "[{scenario}] round {round} disturbed the healthy peer | {}",
            server.report()
        );
        drop(bad);
    }
    healthy.client.conn.close(0u32.into(), b"bye");
    drop(healthy);
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
async fn control_rate_limit_repeats_across_connections() {
    let mut abuse = NetworkAbuseConfig::DEV;
    abuse.control_messages_per_window = 4;
    abuse.rate_drops_before_disconnect = 8;
    let server = TestServer::spawn_abuse(
        Duration::from_secs(3),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    let scenario = "rate limit x3 connections";
    let healthy = handshake_peer(server.addr).await;
    for round in 0..3 {
        let mut offender = handshake_peer(server.addr).await;
        let id = offender.id;
        for _ in 0..24 {
            write_unknown_control(&mut offender.send).await;
        }
        assert!(
            wait_until(|| !server.contains(id), Duration::from_secs(3)).await,
            "[{scenario}] round {round} offender was not disconnected | {}",
            server.report()
        );
        assert!(
            server.contains(healthy.id),
            "[{scenario}] round {round} disturbed the healthy peer | {}",
            server.report()
        );
        drop(offender);
    }
    assert!(server.stat(|s| &s.rate_limited) >= 1, "{}", server.report());
    // A normal peer that never bursts is never rate-limited.
    for n in 0..4u64 {
        healthy
            .client
            .conn
            .send_datagram(encode_client_datagram(500 + n).expect("ping").into())
            .expect("ping");
        let pong = timeout(Duration::from_secs(3), healthy.client.conn.read_datagram())
            .await
            .expect("pong wait")
            .expect("pong");
        assert_eq!(
            purgatory_protocol::decode_server_datagram(&pong).expect("pong"),
            purgatory_protocol::ServerDatagram::Pong { nonce: 500 + n }
        );
    }
    healthy.client.conn.close(0u32.into(), b"bye");
    drop(healthy);
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

/// A graceful client close must be classified as a clean disconnect on every
/// cycle, whichever select arm observes it first.
#[tokio::test]
async fn graceful_client_close_is_never_counted_as_transport_loss() {
    let server = TestServer::spawn(Duration::from_secs(3)).await;
    let scenario = "graceful close classification x20";
    for _ in 0..20 {
        churn_cycle(&server, scenario).await;
    }
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    assert_eq!(
        server.stat(|s| &s.transport_loss),
        0,
        "graceful closes must not count as transport loss | {}",
        server.report()
    );
    assert_eq!(
        server.stat(|s| &s.clean_disconnect),
        20,
        "{}",
        server.report()
    );
    server.shutdown();
}

/// A declared length of zero is malformed, not a lost transport.
#[tokio::test]
async fn zero_length_frame_is_malformed_after_welcome() {
    let server = TestServer::spawn(Duration::from_secs(3)).await;
    let scenario = "zero-length frame";
    let mut peer = handshake_peer(server.addr).await;
    let id = peer.id;
    wait_session_count(&server, 1).await;
    let malformed_before = server.stat(|s| &s.malformed);
    peer.send
        .write_all(&0u32.to_le_bytes())
        .await
        .expect("write zero length");
    assert!(
        wait_until(
            || server.stat(|s| &s.malformed) > malformed_before,
            Duration::from_secs(3)
        )
        .await,
        "[{scenario}] not counted as malformed | {}",
        server.report()
    );
    assert!(wait_until(|| !server.contains(id), Duration::from_secs(3)).await);
    assert_eq!(
        server.stat(|s| &s.transport_loss),
        0,
        "[{scenario}] misclassified as transport loss | {}",
        server.report()
    );
    drop(peer);
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
async fn many_abrupt_drops_clean_every_session() {
    let server = TestServer::spawn(Duration::from_secs(5)).await;
    let scenario = "5 abrupt drops";
    let mut peers = Vec::new();
    for _ in 0..5 {
        peers.push(handshake_peer(server.addr).await);
    }
    wait_session_count(&server, 5).await;
    // No application close: every peer disappears at once.
    drop(peers);
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

/// A peer that vanishes without any close frame; cleanup latency is bounded by
/// the explicit idle timeout (`IDLE_TIMEOUT` in production, shortened here).
async fn forget_peer(addr: SocketAddr, idle: Duration) -> ConnectionId {
    let endpoint = test_client_endpoint_idle(idle);
    let conn = endpoint
        .connect(addr, "localhost")
        .expect("connect start")
        .await
        .expect("connect");
    let (mut send, mut recv) = conn.open_bi().await.expect("open_bi");
    write_hello(&mut send, PROTOCOL_VERSION, "vanish").await;
    let id = match read_server_control(&mut recv).await {
        Ok(ServerControl::Welcome(welcome)) => welcome.connection_id,
        other => panic!("expected welcome, got {other:?}"),
    };
    std::mem::forget(send);
    std::mem::forget(recv);
    std::mem::forget(conn);
    std::mem::forget(endpoint);
    id
}

#[tokio::test]
async fn idle_timeout_repeats_without_leaking_capacity() {
    let idle = Duration::from_millis(400);
    let server = TestServer::spawn_with(Duration::from_secs(2), idle).await;
    let scenario = "idle timeout x2";
    for round in 0..2 {
        let id = forget_peer(server.addr, idle).await;
        assert!(
            wait_until(|| server.contains(id), Duration::from_secs(3)).await,
            "[{scenario}] round {round} session missing | {}",
            server.report()
        );
        assert!(
            wait_until(|| !server.contains(id), Duration::from_secs(6)).await,
            "[{scenario}] round {round} idle peer not reaped | {}",
            server.report()
        );
        server
            .wait_until_baseline(scenario, Duration::from_secs(10))
            .await;
        // Capacity recovered: the next valid client still succeeds.
        server.assert_healthy_probe(scenario).await;
    }
    assert!(
        server.stat(|s| &s.transport_loss) >= 2,
        "{}",
        server.report()
    );
    server.shutdown();
}

#[tokio::test]
async fn handshake_timeout_repeats_without_leaking_capacity() {
    let server = TestServer::spawn(Duration::from_millis(300)).await;
    let scenario = "handshake timeout x4";
    for round in 0..4u64 {
        let stalled = connect(server.addr).await;
        assert!(
            wait_until(
                || server.stat(|s| &s.handshake_timeout) > round,
                Duration::from_secs(3)
            )
            .await,
            "[{scenario}] round {round} did not time out | {}",
            server.report()
        );
        assert_eq!(
            server.session_count(),
            0,
            "[{scenario}] stalled peer created a session | {}",
            server.report()
        );
        drop(stalled);
        server
            .wait_until_baseline(scenario, Duration::from_secs(10))
            .await;
    }
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
async fn version_mismatch_churn_then_healthy() {
    let server = TestServer::spawn(Duration::from_secs(3)).await;
    let scenario = "version mismatch churn 16";
    for _ in 0..16 {
        wrong_version_peer(server.addr).await;
    }
    assert!(
        wait_until(
            || server.stat(|s| &s.version_mismatch) >= 16,
            Duration::from_secs(5)
        )
        .await,
        "[{scenario}] mismatches not counted | {}",
        server.report()
    );
    assert_eq!(server.peak_sessions(), 0, "{}", server.report());
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
async fn unknown_control_soak_stays_bounded() {
    let mut abuse = NetworkAbuseConfig::DEV;
    abuse.malformed_control_budget = 8;
    abuse.control_messages_per_window = 1000;
    abuse.rate_drops_before_disconnect = 1000;
    let server = TestServer::spawn_abuse(
        Duration::from_secs(3),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    let scenario = "unknown control soak";
    let mut peer = handshake_peer(server.addr).await;
    let id = peer.id;
    for _ in 0..8 {
        write_unknown_control(&mut peer.send).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        wait_until(|| !server.contains(id), Duration::from_secs(3)).await,
        "[{scenario}] budget did not close the peer | {}",
        server.report()
    );
    drop(peer);
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
async fn ping_pong_repeats_without_unbounded_state() {
    let server = TestServer::spawn(Duration::from_secs(3)).await;
    let scenario = "ping/pong x16";
    let peer = handshake_peer(server.addr).await;
    for n in 0..16u64 {
        peer.client
            .conn
            .send_datagram(encode_client_datagram(n + 1).expect("ping").into())
            .expect("ping");
        let pong = timeout(Duration::from_secs(3), peer.client.conn.read_datagram())
            .await
            .expect("pong wait")
            .expect("pong");
        assert_eq!(
            purgatory_protocol::decode_server_datagram(&pong).expect("pong"),
            purgatory_protocol::ServerDatagram::Pong { nonce: n + 1 }
        );
    }
    // Duplicate and unknown nonces are droppable, not lifecycle events.
    peer.client
        .conn
        .send_datagram(encode_client_datagram(1).expect("dup").into())
        .expect("dup ping");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(server.contains(peer.id), "{}", server.report());
    peer.client.conn.close(0u32.into(), b"bye");
    drop(peer);
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
async fn accept_loop_serves_healthy_peer_under_mixed_pressure() {
    // One slow peer, one malformed peer, one over-cap peer must not block
    // accept processing for an unrelated healthy peer.
    let cap = 3;
    let mut abuse = soak_abuse(cap);
    abuse.handshake_timeout = Duration::from_millis(500);
    let server = TestServer::spawn_abuse(
        Duration::from_millis(500),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    let scenario = "accept loop robustness";
    let stalled = connect(server.addr).await;
    malformed_handshake_peer(server.addr).await;
    let started = tokio::time::Instant::now();
    let healthy = handshake_peer(server.addr).await;
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "[{scenario}] healthy peer waited on unrelated peers: {:?} | {}",
        started.elapsed(),
        server.report()
    );
    assert!(server.contains(healthy.id), "{}", server.report());
    healthy.client.conn.close(0u32.into(), b"bye");
    drop((healthy, stalled));
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_admission_bounds(scenario);
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
async fn seeded_chaos_mix_converges_for_every_seed() {
    let mut abuse = soak_abuse(8);
    abuse.handshake_timeout = Duration::from_millis(400);
    let server = TestServer::spawn_abuse(
        Duration::from_millis(400),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    for seed in CHAOS_SEEDS {
        let mut runner = ChaosRunner::new(&server, seed);
        runner.run(40).await;
        runner.finish().await;
    }
    server.shutdown();
}

#[tokio::test]
async fn simulation_ticks_progress_during_connection_churn() {
    // Architectural independence: packets never drive `World`, and connection
    // churn must not stop the tick loop. Wall-clock equality is not asserted.
    let server = TestServer::spawn(Duration::from_secs(3)).await;
    let ticks = Arc::new(AtomicU64::new(0));
    let entities = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let ticker = tokio::spawn({
        let ticks = Arc::clone(&ticks);
        let entities = Arc::clone(&entities);
        let stop = Arc::clone(&stop);
        async move {
            let mut clock = purgatory_simulation::SimulationClock::new();
            let mut world = purgatory_simulation::World::footnote_test_stage();
            let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
            let mut last = std::time::Instant::now();
            let mut interval = tokio::time::interval(Duration::from_millis(8));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            entities.store(u64::from(world.len()), Ordering::Relaxed);
            while !stop.load(Ordering::Relaxed) {
                interval.tick().await;
                let now = std::time::Instant::now();
                let elapsed = now.saturating_duration_since(last);
                last = now;
                let update = clock.advance(elapsed);
                for _ in 0..update.ticks_executed {
                    world.tick(dt, purgatory_simulation::PlayerInput::idle());
                }
                ticks.fetch_add(update.ticks_executed as u64, Ordering::Relaxed);
            }
            u64::from(world.len())
        }
    });

    let window = Duration::from_millis(400);
    tokio::time::sleep(window).await;
    let idle_ticks = ticks.load(Ordering::Relaxed);
    assert!(idle_ticks > 0, "idle server produced no ticks");

    let before_churn = ticks.load(Ordering::Relaxed);
    let churn_started = tokio::time::Instant::now();
    while churn_started.elapsed() < window {
        let peer = handshake_peer(server.addr).await;
        peer.client.conn.close(0u32.into(), b"bye");
        drop(peer);
    }
    let churn_ticks = ticks.load(Ordering::Relaxed) - before_churn;
    stop.store(true, Ordering::Relaxed);
    let final_entities = ticker.await.expect("ticker task");

    assert!(
        churn_ticks > 0,
        "simulation stalled during connection churn"
    );
    assert!(
        churn_ticks * 4 >= idle_ticks,
        "churn ticks {churn_ticks} collapsed against idle ticks {idle_ticks}"
    );
    assert_eq!(
        final_entities,
        entities.load(Ordering::Relaxed),
        "network churn changed the entity population"
    );
    server
        .wait_until_baseline("simulation independence", Duration::from_secs(10))
        .await;
    server.shutdown();
}

#[tokio::test]
async fn graceful_server_shutdown_converges_multiple_clients() {
    let mut server =
        ServerProcess::start(purgatory_protocol::IDLE_TIMEOUT, NetworkAbuseConfig::DEV);
    let a = handshake_peer(server.addr).await;
    let b = handshake_peer(server.addr).await;
    assert!(
        wait_until(|| server.session_count() == 2, Duration::from_secs(3)).await,
        "both clients should be registered"
    );
    server.stop(StopMode::Graceful);
    for peer in [&a, &b] {
        match timeout(Duration::from_secs(5), peer.client.conn.closed())
            .await
            .expect("client must observe shutdown")
        {
            quinn::ConnectionError::ApplicationClosed(close) => assert_eq!(
                u64::from(close.error_code),
                u64::from(DisconnectReasonCode::ServerShutdown.as_u8())
            ),
            other => panic!("expected ServerShutdown application close, got {other:?}"),
        }
    }
    assert_eq!(
        server.session_count(),
        0,
        "graceful shutdown must clean every session"
    );
    drop((a, b));
}

#[tokio::test]
async fn abrupt_server_loss_is_not_reported_as_graceful_shutdown() {
    let idle = Duration::from_millis(700);
    let mut server = ServerProcess::start(idle, NetworkAbuseConfig::DEV);
    let peer = handshake_peer_idle(server.addr, idle).await;
    assert!(wait_until(|| server.session_count() == 1, Duration::from_secs(3)).await);
    // Drops the server runtime: no close frame is ever sent.
    server.stop(StopMode::Abrupt);
    let err = timeout(Duration::from_secs(8), peer.client.conn.closed())
        .await
        .expect("client must notice the lost server");
    assert!(
        !matches!(err, quinn::ConnectionError::ApplicationClosed(_)),
        "no graceful reason was received, got {err:?}"
    );
    drop(peer);
}

#[tokio::test]
async fn server_restart_creates_fresh_session_state() {
    let mut first = ServerProcess::start(purgatory_protocol::IDLE_TIMEOUT, NetworkAbuseConfig::DEV);
    let peer_a = handshake_peer(first.addr).await;
    assert_eq!(peer_a.id.get(), 1, "a fresh allocator starts at 1");
    assert!(wait_until(|| first.session_count() == 1, Duration::from_secs(3)).await);
    first.stop(StopMode::Graceful);
    let closed = timeout(Duration::from_secs(5), peer_a.client.conn.closed())
        .await
        .expect("client must converge after server stop");
    assert!(matches!(
        closed,
        quinn::ConnectionError::ApplicationClosed(_)
    ));
    drop(peer_a);

    let mut second =
        ServerProcess::start(purgatory_protocol::IDLE_TIMEOUT, NetworkAbuseConfig::DEV);
    assert_ne!(first.addr, second.addr, "restart binds a new endpoint");
    let peer_b = handshake_peer(second.addr).await;
    assert_eq!(
        peer_b.id.get(),
        1,
        "server B owns its own ids; no session is resurrected"
    );
    assert!(wait_until(|| second.session_count() == 1, Duration::from_secs(3)).await);
    peer_b.client.conn.close(0u32.into(), b"bye");
    drop(peer_b);
    assert!(wait_until(|| second.session_count() == 0, Duration::from_secs(5)).await);
    second.stop(StopMode::Graceful);
}

// ---------------------------------------------------------------------------
// Interrupted handshakes and startup failure.
//
// A peer that vanishes before Welcome must leave nothing behind, and a server
// that cannot bind must fail cleanly instead of half-starting.
// ---------------------------------------------------------------------------

/// A peer that has completed QUIC but sent no Hello holds the server in the
/// pre-Welcome phase deterministically: the handshake cannot advance without a
/// Hello, so no production timing or sleep is involved.
#[tokio::test]
async fn live_peer_shutdown_before_welcome_leaves_no_session() {
    let server = TestServer::spawn(Duration::from_secs(5)).await;
    let scenario = "shutdown before welcome";
    for round in 0..8 {
        let client = connect(server.addr).await;
        assert!(
            wait_until(|| server.gauges().handshakes == 1, Duration::from_secs(3)).await,
            "[{scenario}] round {round} handshake task never started | {}",
            server.report()
        );
        assert_eq!(
            server.gauges().admission_in_use,
            1,
            "[{scenario}] round {round} permit not held | {}",
            server.report()
        );
        // Session insertion happens only after the Welcome write succeeds.
        assert_eq!(
            server.session_count(),
            0,
            "[{scenario}] round {round} session before Welcome | {}",
            server.report()
        );
        // The peer's network owner shuts down mid-handshake.
        client.conn.close(0u32.into(), b"shutdown");
        drop(client);
        server
            .wait_until_baseline(&format!("{scenario} round {round}"), Duration::from_secs(5))
            .await;
    }
    assert_eq!(
        server.stat(|s| &s.total_accepted),
        0,
        "[{scenario}] a peer became a session without Welcome | {}",
        server.report()
    );
    assert_eq!(
        server.peak_sessions(),
        0,
        "[{scenario}] session table was populated | {}",
        server.report()
    );
    server.assert_admission_bounds(scenario);
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

/// Same shutdown, one step later: the Hello is on the wire but the peer never
/// waits for Welcome. Whichever side wins the race, the server must converge.
#[tokio::test]
async fn live_peer_shutdown_after_hello_converges_without_leak() {
    let server = TestServer::spawn(Duration::from_secs(5)).await;
    let scenario = "shutdown after hello";
    for round in 0..8 {
        let client = connect(server.addr).await;
        let (mut send, recv) = client.conn.open_bi().await.expect("open_bi");
        let _ = write_hello_best_effort(&mut send, PROTOCOL_VERSION, "shutdown").await;
        client.conn.close(0u32.into(), b"shutdown");
        drop((send, recv, client));
        server
            .wait_until_baseline(&format!("{scenario} round {round}"), Duration::from_secs(5))
            .await;
        server.assert_admission_bounds(scenario);
    }
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

/// Interrupted handshakes must return admission capacity, not consume it.
#[tokio::test]
async fn interrupted_handshakes_return_admission_capacity() {
    let cap = 3;
    let server = TestServer::spawn_abuse(
        Duration::from_secs(5),
        purgatory_protocol::IDLE_TIMEOUT,
        soak_abuse(cap),
    )
    .await;
    let scenario = "interrupted handshakes at cap";
    let mut held = Vec::new();
    for _ in 0..cap {
        held.push(connect(server.addr).await);
    }
    assert!(
        wait_until(
            || server.gauges().admission_in_use == cap,
            Duration::from_secs(3)
        )
        .await,
        "[{scenario}] admission never filled | {}",
        server.report()
    );
    assert_eq!(
        server.session_count(),
        0,
        "[{scenario}] pre-Welcome peers became sessions | {}",
        server.report()
    );
    for client in &held {
        client.conn.close(0u32.into(), b"shutdown");
    }
    drop(held);
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    assert_eq!(
        server.gauges().admission_in_use,
        0,
        "[{scenario}] permits leaked | {}",
        server.report()
    );
    server.assert_admission_bounds(scenario);
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

/// Reserve an ephemeral localhost port. Automated tests must never depend on
/// whether the fixed dev port happens to be free.
fn reserved_udp_port() -> (std::net::UdpSocket, SocketAddr) {
    let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .expect("reserve ephemeral port");
    let addr = socket.local_addr().expect("reserved local addr");
    (socket, addr)
}

#[tokio::test]
async fn bind_to_address_in_use_fails_without_panic() {
    install_crypto_provider().expect("crypto");
    let (reserved, addr) = reserved_udp_port();
    let mut config = ServerEndpointConfig::ephemeral();
    config.bind = addr;

    let err = endpoint::bind(&config)
        .err()
        .expect("bind to a held address must fail");
    assert!(
        err.contains("failed to bind"),
        "bind failure must be actionable: {err}"
    );
    assert!(
        err.contains(&addr.to_string()),
        "bind failure must name the address: {err}"
    );

    // The failed attempt took nothing: the port is still owned by the reserver.
    assert_eq!(reserved.local_addr().expect("still bound"), addr);
    drop(reserved);

    // Released, the same address binds cleanly: no partial state lingered.
    let bound = endpoint::bind(&config).expect("bind after release");
    assert_eq!(bound.local_addr(), addr);
    assert_eq!(lock_sessions(&bound.sessions).len(), 0);
    assert_eq!(bound.inflight_tasks.load(Ordering::Relaxed), 0);
    bound.endpoint.close(0u32.into(), b"done");
}

/// Startup failure is a returned error, not a panic and not a retry loop.
/// `run_blocking` owns its runtime, so this test must stay non-async.
#[test]
fn run_blocking_reports_bind_failure_and_returns() {
    let (reserved, addr) = reserved_udp_port();
    let mut config = ServerEndpointConfig::ephemeral();
    config.bind = addr;
    let err = super::run_blocking(config).expect_err("startup must fail");
    assert!(err.contains("failed to bind"), "{err}");
    assert!(err.contains(&addr.to_string()), "{err}");
    drop(reserved);
}

/// The ready log must be semantically true: it may only follow a real bind.
#[test]
fn ready_log_only_follows_successful_bind() {
    let src = include_str!("mod.rs");
    assert!(
        src.contains("let bound = endpoint::bind(&config)?"),
        "bind failure must propagate before anything else starts"
    );
    let bind_at = src.find("endpoint::bind(&config)?").expect("bind call");
    let log_at = src.find("network listening on").expect("ready log");
    assert!(
        bind_at < log_at,
        "the listening log must come after a successful bind"
    );
    assert_eq!(
        src.matches("network listening").count(),
        1,
        "exactly one network readiness log"
    );
}

// --- Extended soaks: run with `./scripts/network_soak.ps1` -----------------

#[tokio::test]
#[ignore = "extended soak: 1000 sequential connect/disconnect cycles"]
async fn sequential_churn_1000_soak() {
    let server = TestServer::spawn(Duration::from_secs(5)).await;
    sequential_churn(&server, "extended sequential churn 1000", 1000).await;
    // Observational only, visible with `--nocapture`. No threshold is asserted.
    println!("PURGATORY soak sequential-1000 {}", server.report());
    server.shutdown();
}

#[tokio::test]
#[ignore = "extended soak: 10 clients x 100 churn cycles"]
async fn multi_client_churn_10x100_soak() {
    let server = TestServer::spawn(Duration::from_secs(10)).await;
    let scenario = "extended multi-client churn 10x100";
    for round in 0..100 {
        let clients = handshake_many(server.addr, 10).await;
        let mut ids = HashSet::new();
        for (_, _, _, id) in &clients {
            assert!(ids.insert(*id), "round {round} duplicate ConnectionId {id}");
        }
        wait_session_count(&server, 10).await;
        close_clients(clients);
        wait_session_count(&server, 0).await;
    }
    server
        .wait_until_baseline(scenario, Duration::from_secs(15))
        .await;
    assert_eq!(server.peak_sessions(), 10, "{}", server.report());
    server.assert_healthy_probe(scenario).await;
    println!("PURGATORY soak multi-10x100 {}", server.report());
    server.shutdown();
}

#[tokio::test]
#[ignore = "extended soak: 200 malformed handshakes, then healthy"]
async fn malformed_churn_200_soak() {
    let server = TestServer::spawn(Duration::from_secs(5)).await;
    let scenario = "extended malformed churn 200";
    for _ in 0..200 {
        malformed_handshake_peer(server.addr).await;
    }
    server
        .wait_until_baseline(scenario, Duration::from_secs(15))
        .await;
    assert_eq!(server.peak_sessions(), 0, "{}", server.report());
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
#[ignore = "extended soak: 20 admission fill/drain rounds"]
async fn admission_churn_20_rounds_soak() {
    let cap = 4;
    let server = TestServer::spawn_abuse(
        Duration::from_secs(4),
        purgatory_protocol::IDLE_TIMEOUT,
        soak_abuse(cap),
    )
    .await;
    let scenario = "extended admission churn x20";
    for round in 0..20 {
        let mut held = Vec::new();
        for _ in 0..cap {
            held.push(connect(server.addr).await);
        }
        assert!(
            wait_until(
                || server.gauges().admission_in_use == cap,
                Duration::from_secs(3)
            )
            .await,
            "[{scenario}] round {round} did not fill | {}",
            server.report()
        );
        let _ = test_client_endpoint()
            .connect(server.addr, "localhost")
            .expect("connect start")
            .await;
        drop(held);
        server
            .wait_until_baseline(scenario, Duration::from_secs(10))
            .await;
        server.assert_admission_bounds(scenario);
    }
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
#[ignore = "extended soak: repeated server restart rounds"]
async fn server_restart_rounds_soak() {
    for round in 0..4 {
        let mut server =
            ServerProcess::start(purgatory_protocol::IDLE_TIMEOUT, NetworkAbuseConfig::DEV);
        let peer = handshake_peer(server.addr).await;
        assert_eq!(peer.id.get(), 1, "round {round} must start fresh");
        assert!(wait_until(|| server.session_count() == 1, Duration::from_secs(3)).await);
        server.stop(StopMode::Graceful);
        let closed = timeout(Duration::from_secs(5), peer.client.conn.closed())
            .await
            .expect("client must converge");
        assert!(matches!(
            closed,
            quinn::ConnectionError::ApplicationClosed(_)
        ));
        drop(peer);
    }
}

#[tokio::test]
#[ignore = "extended soak: sustained ping cadence"]
async fn ping_cadence_soak() {
    let server = TestServer::spawn(Duration::from_secs(5)).await;
    let scenario = "extended ping cadence";
    let peer = handshake_peer(server.addr).await;
    for n in 0..80u64 {
        peer.client
            .conn
            .send_datagram(encode_client_datagram(n + 1).expect("ping").into())
            .expect("ping");
        let pong = timeout(Duration::from_secs(3), peer.client.conn.read_datagram())
            .await
            .expect("pong wait")
            .expect("pong");
        assert_eq!(
            purgatory_protocol::decode_server_datagram(&pong).expect("pong"),
            purgatory_protocol::ServerDatagram::Pong { nonce: n + 1 }
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert!(server.contains(peer.id), "{}", server.report());
    peer.client.conn.close(0u32.into(), b"bye");
    drop(peer);
    server
        .wait_until_baseline(scenario, Duration::from_secs(10))
        .await;
    server.assert_healthy_probe(scenario).await;
    server.shutdown();
}

#[tokio::test]
#[ignore = "extended soak: wider deterministic chaos seed matrix"]
async fn chaos_seed_matrix_soak() {
    let mut abuse = soak_abuse(8);
    abuse.handshake_timeout = Duration::from_millis(400);
    let server = TestServer::spawn_abuse(
        Duration::from_millis(400),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    let seeds: [u64; 8] = [
        0x1,
        0x2,
        0x00C0_FFEE,
        0xDEAD_BEEF,
        0xFEED_FACE,
        0x5EED,
        0xABCD_1234,
        0x7FFF_FFFF,
    ];
    for seed in seeds {
        let mut runner = ChaosRunner::new(&server, seed);
        runner.run(120).await;
        runner.finish().await;
    }
    println!("PURGATORY soak chaos-matrix {}", server.report());
    server.shutdown();
}

// ---------------------------------------------------------------------------
// Phase 5.1 — authoritative intent input
// ---------------------------------------------------------------------------

struct GameplaySim {
    owner: super::gameplay::GameplayOwner,
    life_rx: tokio::sync::mpsc::Receiver<super::gameplay::LifecycleCmd>,
    input_rx: tokio::sync::mpsc::Receiver<super::gameplay::InputUpdate>,
}

impl GameplaySim {
    fn pump(&mut self) {
        self.owner.drain(&mut self.life_rx, &mut self.input_rx);
    }

    fn tick_n(&mut self, n: u32) {
        let dt = purgatory_simulation::TICK_DURATION.as_secs_f32();
        for _ in 0..n {
            self.pump();
            self.owner.simulate_tick(dt);
        }
    }
}

async fn spawn_gameplay() -> (TestServer, Arc<Mutex<GameplaySim>>) {
    spawn_gameplay_abuse(
        Duration::from_secs(2),
        purgatory_protocol::IDLE_TIMEOUT,
        NetworkAbuseConfig::DEV,
    )
    .await
}

async fn spawn_gameplay_abuse(
    handshake_timeout: Duration,
    idle_timeout: Duration,
    mut abuse: NetworkAbuseConfig,
) -> (TestServer, Arc<Mutex<GameplaySim>>) {
    super::install_crypto_provider().expect("crypto");
    let mut config = super::config::ServerEndpointConfig::ephemeral();
    abuse.handshake_timeout = handshake_timeout;
    config.idle_timeout = idle_timeout;
    config.abuse = abuse;
    let bound = super::endpoint::bind(&config).expect("bind");
    let addr = bound.local_addr();
    let sessions = bound.sessions.clone();
    let inflight = bound.inflight_tasks.clone();
    let limiter = bound.limiter.clone();
    let stats = bound.stats.clone();
    let endpoint = bound.endpoint.clone();
    let abuse = config.abuse;
    let (life_tx, life_rx) = tokio::sync::mpsc::channel(super::gameplay::lifecycle_cap());
    let (input_tx, input_rx) = tokio::sync::mpsc::channel(super::gameplay::input_cap());
    let tx = super::gameplay::GameplayTx {
        lifecycle: life_tx,
        input: input_tx,
    };
    let sim = Arc::new(Mutex::new(GameplaySim {
        owner: super::gameplay::GameplayOwner::new(),
        life_rx,
        input_rx,
    }));
    let persist_dir = std::env::temp_dir().join(format!(
        "purgatory-test-persist-{}-{}",
        std::process::id(),
        addr.port()
    ));
    let persist = super::persist::PersistenceHandle::spawn(&persist_dir).expect("persist");
    {
        let mut g = sim.lock().unwrap_or_else(|err| err.into_inner());
        g.owner.set_persist(persist.clone());
    }
    let sim_pump = sim.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(4));
        loop {
            ticker.tick().await;
            lock_sim(&sim_pump).pump();
        }
    });
    let persist_accept = persist.clone();
    let accept = tokio::spawn(async move {
        accept_loop(bound, abuse, Some(tx), Some(persist_accept)).await;
    });
    (
        TestServer {
            addr,
            sessions,
            inflight,
            limiter,
            stats,
            endpoint,
            accept,
            abuse,
        },
        sim,
    )
}

fn lock_sim(sim: &Mutex<GameplaySim>) -> std::sync::MutexGuard<'_, GameplaySim> {
    sim.lock().unwrap_or_else(|err| err.into_inner())
}

async fn wait_attached(sim: &Mutex<GameplaySim>, id: ConnectionId) -> bool {
    wait_until(
        || {
            let mut g = lock_sim(sim);
            g.pump();
            g.owner.entity_of(id).is_some()
        },
        Duration::from_secs(2),
    )
    .await
}

#[tokio::test]
async fn authoritative_right_moves_player() {
    let (server, sim) = spawn_gameplay().await;
    let (_c, mut send, _r, id) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id).await);
    write_input(&mut send, 1, MoveAxis::Right, false, false).await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.last_received(id) == Some(1)
            },
            Duration::from_secs(2),
        )
        .await
    );
    lock_sim(&sim).tick_n(12);
    let x = {
        let g = lock_sim(&sim);
        let entity = g.owner.entity_of(id).unwrap();
        g.owner.world().player_body_of(entity).unwrap().position[0]
    };
    assert!(x > purgatory_simulation::FOOTNOTE_SPAWN_X);
    server.shutdown();
}

#[tokio::test]
async fn authoritative_left_moves_player() {
    let (server, sim) = spawn_gameplay().await;
    let (_c, mut send, _r, id) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id).await);
    assert!(lock_sim(&sim).owner.set_player_x(id, 0.0));
    write_input(&mut send, 1, MoveAxis::Left, false, false).await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.last_received(id) == Some(1)
            },
            Duration::from_secs(2),
        )
        .await
    );
    lock_sim(&sim).tick_n(12);
    let x = {
        let g = lock_sim(&sim);
        let entity = g.owner.entity_of(id).unwrap();
        g.owner.world().player_body_of(entity).unwrap().position[0]
    };
    assert!(x < -0.2, "left from x=0, got {x}");
    server.shutdown();
}

#[tokio::test]
async fn jump_from_solid_is_authoritative() {
    let (server, sim) = spawn_gameplay().await;
    let (_c, mut send, _r, id) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id).await);
    write_input(&mut send, 1, MoveAxis::Neutral, true, false).await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.last_received(id) == Some(1)
            },
            Duration::from_secs(2),
        )
        .await
    );
    lock_sim(&sim).tick_n(1);
    let body = {
        let g = lock_sim(&sim);
        let entity = g.owner.entity_of(id).unwrap();
        g.owner.world().player_body_of(entity).unwrap()
    };
    assert!(
        !body.grounded && body.position[1] > -3.2,
        "authoritative jump: grounded={} vy={} pos={:?}",
        body.grounded,
        body.velocity[1],
        body.position
    );
    server.shutdown();
}

#[tokio::test]
async fn down_jump_drop_through_oneway() {
    let (server, sim) = spawn_gameplay().await;
    let (_c, mut send, _r, id) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id).await);
    let platform = {
        let mut g = lock_sim(&sim);
        assert!(g.owner.stand_on_first_oneway(id));
        g.owner
            .world()
            .player_body_of(g.owner.entity_of(id).unwrap())
            .unwrap()
            .grounded_on
    };
    write_input(&mut send, 1, MoveAxis::Neutral, true, true).await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.last_received(id) == Some(1)
            },
            Duration::from_secs(2),
        )
        .await
    );
    lock_sim(&sim).tick_n(1);
    let body = {
        let g = lock_sim(&sim);
        let entity = g.owner.entity_of(id).unwrap();
        g.owner.world().player_body_of(entity).unwrap()
    };
    assert!(!body.grounded);
    assert_eq!(body.ignored_platform, platform);
    server.shutdown();
}

#[tokio::test]
async fn packet_burst_does_not_create_simulation_ticks() {
    let (server, sim) = spawn_gameplay().await;
    let (_c, mut send, _r, id) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id).await);
    let ticks_before = lock_sim(&sim).owner.ticks();
    for seq in 1..=40 {
        write_input(&mut send, seq, MoveAxis::Right, false, false).await;
    }
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.last_received(id) == Some(40)
            },
            Duration::from_secs(2),
        )
        .await
    );
    assert_eq!(lock_sim(&sim).owner.ticks(), ticks_before);
    lock_sim(&sim).tick_n(1);
    assert_eq!(lock_sim(&sim).owner.ticks(), ticks_before + 1);
    server.shutdown();
}

#[tokio::test]
async fn a_input_does_not_control_b() {
    let (server, sim) = spawn_gameplay().await;
    let (_a, mut send_a, _ra, id_a) = handshake_ok(server.addr).await;
    let (_b, mut send_b, _rb, id_b) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id_a).await);
    assert!(wait_attached(&sim, id_b).await);
    let (ea, eb, bx0) = {
        let g = lock_sim(&sim);
        let ea = g.owner.entity_of(id_a).unwrap();
        let eb = g.owner.entity_of(id_b).unwrap();
        assert_ne!(ea, eb);
        let bx0 = g.owner.world().player_body_of(eb).unwrap().position[0];
        (ea, eb, bx0)
    };
    write_input(&mut send_a, 1, MoveAxis::Right, false, false).await;
    write_input(&mut send_b, 1, MoveAxis::Neutral, false, false).await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.last_received(id_a) == Some(1)
            },
            Duration::from_secs(2),
        )
        .await
    );
    lock_sim(&sim).tick_n(12);
    let (ax, bx) = {
        let g = lock_sim(&sim);
        (
            g.owner.world().player_body_of(ea).unwrap().position[0],
            g.owner.world().player_body_of(eb).unwrap().position[0],
        )
    };
    assert!(ax > purgatory_simulation::FOOTNOTE_SPAWN_X);
    assert!((bx - bx0).abs() < 0.05);
    server.shutdown();
}

#[tokio::test]
async fn graceful_disconnect_removes_player() {
    let (server, sim) = spawn_gameplay().await;
    let (client, _s, _r, id) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id).await);
    client.conn.close(0u32.into(), b"bye");
    drop(client);
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.player_count() == 0 && g.owner.entity_of(id).is_none()
            },
            Duration::from_secs(3),
        )
        .await
    );
    server.shutdown();
}

#[tokio::test]
async fn abrupt_loss_removes_player() {
    let (server, sim) = spawn_gameplay_abuse(
        Duration::from_secs(2),
        Duration::from_millis(400),
        NetworkAbuseConfig::DEV,
    )
    .await;
    let (client, _s, _r, id) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id).await);
    drop(client);
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.player_count() == 0
            },
            Duration::from_secs(3),
        )
        .await
    );
    server.shutdown();
}

#[tokio::test]
async fn reconnect_starts_with_clean_neutral() {
    let (server, sim) = spawn_gameplay().await;
    let (client, mut send, _r, id) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id).await);
    write_input(&mut send, 1, MoveAxis::Right, true, true).await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.last_received(id) == Some(1)
            },
            Duration::from_secs(2),
        )
        .await
    );
    let old = lock_sim(&sim).owner.entity_of(id).unwrap();
    client.conn.close(0u32.into(), b"bye");
    drop(client);
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.player_count() == 0
            },
            Duration::from_secs(3),
        )
        .await
    );
    let (_c2, mut send2, _r2, id2) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id2).await);
    {
        let g = lock_sim(&sim);
        assert_ne!(id, id2);
        let new = g.owner.entity_of(id2).unwrap();
        assert_ne!(old, new);
        assert!(!g.owner.contains_entity(old));
        assert_eq!(g.owner.last_seq(id2), None);
        let body = g.owner.world().player_body_of(new).unwrap();
        assert!(body.grounded);
        assert_eq!(body.velocity, [0.0, 0.0]);
    }
    write_input(&mut send2, 1, MoveAxis::Neutral, false, false).await;
    server.shutdown();
}

#[tokio::test]
async fn malformed_input_cannot_mutate_world() {
    let (server, sim) = spawn_gameplay().await;
    let (_c, mut send, _r, id) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id).await);
    let x0 = {
        let g = lock_sim(&sim);
        g.owner
            .world()
            .player_body_of(g.owner.entity_of(id).unwrap())
            .unwrap()
            .position[0]
    };
    let mut payload = vec![6u8];
    payload.extend_from_slice(&1u32.to_le_bytes());
    payload.extend_from_slice(&[3, 0, 0]);
    send.write_all(&encode_frame(&payload).expect("frame"))
        .await
        .expect("write bad");
    assert!(
        wait_until(
            || server.stat(|s| &s.input_invalid) >= 1,
            Duration::from_secs(2),
        )
        .await
    );
    lock_sim(&sim).tick_n(4);
    let x1 = {
        let g = lock_sim(&sim);
        g.owner
            .world()
            .player_body_of(g.owner.entity_of(id).unwrap())
            .unwrap()
            .position[0]
    };
    assert!((x1 - x0).abs() < 0.05);
    server.shutdown();
}

#[tokio::test]
async fn input_rate_offender_is_isolated() {
    let mut abuse = NetworkAbuseConfig::DEV;
    abuse.input_messages_per_window = 8;
    abuse.input_drops_before_disconnect = 10_000;
    let (server, sim) = spawn_gameplay_abuse(
        Duration::from_secs(2),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    let (_a, mut send_a, _ra, id_a) = handshake_ok(server.addr).await;
    let (_b, mut send_b, _rb, id_b) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id_a).await);
    assert!(wait_attached(&sim, id_b).await);
    for seq in 1..=80 {
        write_input(&mut send_a, seq, MoveAxis::Left, false, false).await;
    }
    write_input(&mut send_b, 1, MoveAxis::Right, false, false).await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.last_received(id_b) == Some(1)
                    && server.stat(|s| &s.input_rate_limited) >= 1
            },
            Duration::from_secs(3),
        )
        .await
    );
    lock_sim(&sim).tick_n(12);
    let bx = {
        let g = lock_sim(&sim);
        g.owner
            .world()
            .player_body_of(g.owner.entity_of(id_b).unwrap())
            .unwrap()
            .position[0]
    };
    assert!(bx > purgatory_simulation::FOOTNOTE_SPAWN_X);
    assert!(server.contains(id_b));
    server.shutdown();
}

#[tokio::test]
async fn input_handoff_awaits_instead_of_dropping() {
    // Raise input rate so the variable under test is handoff backpressure, not
    // the abuse window (DEV default is 128 msgs/s).
    let mut abuse = NetworkAbuseConfig::DEV;
    abuse.input_messages_per_window = 10_000;
    abuse.input_drops_before_disconnect = 10_000;
    let (server, sim) = spawn_gameplay_abuse(
        Duration::from_secs(2),
        purgatory_protocol::IDLE_TIMEOUT,
        abuse,
    )
    .await;
    let (_c, mut send, _r, id) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id).await);

    let sim_pump = Arc::clone(&sim);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_pump = Arc::clone(&stop);
    let pump = tokio::spawn(async move {
        while !stop_pump.load(Ordering::Relaxed) {
            lock_sim(&sim_pump).pump();
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        lock_sim(&sim_pump).pump();
    });

    for seq in 1..=200 {
        write_input(&mut send, seq, MoveAxis::Right, false, false).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    stop.store(true, Ordering::Relaxed);
    let _ = timeout(Duration::from_secs(2), pump).await;
    lock_sim(&sim).pump();

    assert_eq!(server.stat(|s| &s.input_handoff_dropped), 0);
    assert!(
        lock_sim(&sim).owner.input_received >= 200,
        "received={}",
        lock_sim(&sim).owner.input_received
    );
    server.shutdown();
}

async fn accept_snapshot_stream(client: &TestClient) -> RecvStream {
    timeout(Duration::from_secs(2), client.conn.accept_uni())
        .await
        .expect("accept_uni timeout")
        .expect("accept_uni")
}

struct ReplicaView {
    epoch: u32,
    snapshot_sequence: u32,
    local_player_entity: WireEntityId,
    last_acknowledged_input_sequence: u32,
    input_epoch: u16,
    local_grounded: bool,
    entities: HashMap<WireEntityId, SnapshotEntity>,
    equipment: HashMap<WireEntityId, Option<ReplicatedEquipment>>,
    equipment_updates: u32,
}

impl ReplicaView {
    fn new() -> Self {
        Self {
            epoch: 0,
            snapshot_sequence: 0,
            local_player_entity: WireEntityId {
                index: 0,
                generation: 0,
            },
            last_acknowledged_input_sequence: 0,
            input_epoch: 0,
            local_grounded: false,
            entities: HashMap::new(),
            equipment: HashMap::new(),
            equipment_updates: 0,
        }
    }

    fn apply(&mut self, frame: ReplicationFrame) {
        if frame.observer_baseline_epoch < self.epoch {
            return;
        }
        if frame.observer_baseline_epoch > self.epoch {
            self.entities.clear();
            self.equipment.clear();
            self.epoch = frame.observer_baseline_epoch;
        }
        self.snapshot_sequence = frame.snapshot_sequence;
        self.local_player_entity = frame.local_player_entity;
        self.last_acknowledged_input_sequence = frame.last_acknowledged_input_sequence;
        self.input_epoch = frame.input_epoch;
        self.local_grounded = frame.local_grounded;
        for rec in frame.records {
            match rec {
                ReplicationRecord::Enter {
                    entity, equipment, ..
                } => {
                    self.entities.insert(entity.entity_id, entity);
                    self.equipment.insert(entity.entity_id, equipment);
                }
                ReplicationRecord::Update {
                    entity_id,
                    position,
                    velocity,
                    equipment,
                    domains,
                    ..
                } => {
                    if let Some(e) = self.entities.get_mut(&entity_id) {
                        if let Some(p) = position {
                            e.position = p;
                        }
                        if let Some(v) = velocity {
                            e.velocity = v;
                        }
                    }
                    if domains.equipment {
                        self.equipment_updates = self.equipment_updates.saturating_add(1);
                        if let Some(delta) = equipment {
                            let slot = self
                                .equipment
                                .entry(entity_id)
                                .or_insert_with(|| Some(ReplicatedEquipment::empty()));
                            let state = slot.get_or_insert_with(ReplicatedEquipment::empty);
                            state.apply_delta(&delta);
                        }
                    }
                }
                ReplicationRecord::Leave { entity_id } => {
                    self.entities.remove(&entity_id);
                    self.equipment.remove(&entity_id);
                }
            }
        }
    }

    fn player_count(&self) -> usize {
        self.entities
            .values()
            .filter(|e| e.kind == ReplicatedKind::Player)
            .count()
    }

    fn kind_count(&self, kind: ReplicatedKind) -> usize {
        self.entities.values().filter(|e| e.kind == kind).count()
    }
}

async fn read_replication_frame(recv: &mut RecvStream) -> ReplicationFrame {
    let mut prefix = [0u8; 4];
    recv.read_exact(&mut prefix).await.expect("prefix");
    let len = peek_gameplay_frame_len(&prefix).expect("peek snapshot");
    let mut payload = vec![0u8; len as usize];
    recv.read_exact(&mut payload).await.expect("payload");
    decode_replication_frame(&payload).expect("decode replication frame")
}

async fn drain_frames(recv: &mut RecvStream, view: &mut ReplicaView, idle: Duration) {
    while let Ok(frame) = timeout(idle, read_replication_frame(recv)).await {
        view.apply(frame);
    }
}

async fn read_latest_view(recv: &mut RecvStream) -> ReplicaView {
    let mut view = ReplicaView::new();
    let first = timeout(Duration::from_secs(2), read_replication_frame(recv))
        .await
        .expect("first replication frame");
    view.apply(first);
    drain_frames(recv, &mut view, Duration::from_millis(80)).await;
    view
}

#[tokio::test]
async fn client_receives_authoritative_snapshot() {
    let (server, sim) = spawn_gameplay().await;
    let (client, mut send, _r, id) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id).await);
    write_input(&mut send, 1, MoveAxis::Right, false, false).await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.last_received(id) == Some(1)
            },
            Duration::from_secs(2),
        )
        .await
    );
    assert!(lock_sim(&sim).owner.set_player_x(id, -8.0));
    lock_sim(&sim).tick_n(8);
    let mut uni = accept_snapshot_stream(&client).await;
    let snap = read_latest_view(&mut uni).await;
    assert_eq!(snap.player_count(), 1);
    let entity = lock_sim(&sim).owner.entity_of(id).unwrap();
    assert_eq!(
        snap.local_player_entity,
        super::snapshot::to_wire_id(entity)
    );
    let player = snap
        .entities
        .get(&snap.local_player_entity)
        .expect("local player in snapshot");
    assert!(player.position[0] > purgatory_simulation::FOOTNOTE_SPAWN_X);
    assert_eq!(
        snap.kind_count(ReplicatedKind::Interactable),
        2,
        "AOI at x=-8 must include Map A switch and chest"
    );
    assert_eq!(
        snap.kind_count(ReplicatedKind::Portal),
        1,
        "AOI at x=-8 must include the Map A portal"
    );
    server.shutdown();
}

#[tokio::test]
async fn two_clients_see_both_entities_and_distinct_local_ids() {
    let (server, sim) = spawn_gameplay().await;
    let (client_a, mut send_a, _ra, id_a) = handshake_ok(server.addr).await;
    let (client_b, mut send_b, _rb, id_b) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id_a).await);
    assert!(wait_attached(&sim, id_b).await);
    assert!(lock_sim(&sim).owner.set_player_x(id_a, 0.0));
    assert!(lock_sim(&sim).owner.set_player_x(id_b, 0.0));
    write_input(&mut send_a, 1, MoveAxis::Right, false, false).await;
    write_input(&mut send_b, 1, MoveAxis::Left, false, false).await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.last_received(id_a) == Some(1) && g.owner.last_received(id_b) == Some(1)
            },
            Duration::from_secs(2),
        )
        .await
    );
    lock_sim(&sim).tick_n(12);
    let mut uni_a = accept_snapshot_stream(&client_a).await;
    let mut uni_b = accept_snapshot_stream(&client_b).await;
    let snap_a = read_latest_view(&mut uni_a).await;
    let snap_b = read_latest_view(&mut uni_b).await;
    assert_eq!(snap_a.player_count(), 2);
    assert_eq!(snap_b.player_count(), 2);
    assert_ne!(snap_a.local_player_entity, snap_b.local_player_entity);
    let ea = super::snapshot::to_wire_id(lock_sim(&sim).owner.entity_of(id_a).unwrap());
    let eb = super::snapshot::to_wire_id(lock_sim(&sim).owner.entity_of(id_b).unwrap());
    assert_eq!(snap_a.local_player_entity, ea);
    assert_eq!(snap_b.local_player_entity, eb);
    let ax = snap_a.entities.get(&ea).unwrap().position[0];
    let bx = snap_a.entities.get(&eb).unwrap().position[0];
    assert!(ax > 0.2, "A right from 0, got {ax}");
    assert!(bx < -0.2, "B left from 0, got {bx}");
    server.shutdown();
}

#[tokio::test]
async fn disconnect_removes_entity_from_next_snapshot() {
    let (server, sim) = spawn_gameplay().await;
    let (client_a, _sa, _ra, id_a) = handshake_ok(server.addr).await;
    let (client_b, send_b, recv_b, id_b) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id_a).await);
    assert!(wait_attached(&sim, id_b).await);
    lock_sim(&sim).tick_n(1);
    let mut uni_a = accept_snapshot_stream(&client_a).await;
    let mut view = read_latest_view(&mut uni_a).await;
    assert_eq!(view.player_count(), 2);
    let old_b = lock_sim(&sim).owner.entity_of(id_b).unwrap();
    drop((client_b, send_b, recv_b));
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.entity_of(id_b).is_none()
            },
            Duration::from_secs(2),
        )
        .await
    );
    lock_sim(&sim).tick_n(1);
    for _ in 0..8 {
        if let Ok(frame) = timeout(Duration::from_secs(2), read_replication_frame(&mut uni_a)).await
        {
            view.apply(frame);
        }
        if view.player_count() == 1 {
            break;
        }
    }
    assert_eq!(view.player_count(), 1);
    assert!(
        !view
            .entities
            .contains_key(&super::snapshot::to_wire_id(old_b))
    );
    server.shutdown();
}

#[tokio::test]
async fn reconnect_uses_fresh_generational_id() {
    let (server, sim) = spawn_gameplay().await;
    let (client_a, _sa, _ra, id_a) = handshake_ok(server.addr).await;
    let (client_b, send_b, recv_b, id_b) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id_a).await);
    assert!(wait_attached(&sim, id_b).await);
    lock_sim(&sim).tick_n(1);
    let mut uni_a = accept_snapshot_stream(&client_a).await;
    let mut view = read_latest_view(&mut uni_a).await;
    let old = lock_sim(&sim).owner.entity_of(id_b).unwrap();
    drop((client_b, send_b, recv_b));
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.entity_of(id_b).is_none()
            },
            Duration::from_secs(2),
        )
        .await
    );
    let (client_c, _sc, _rc, id_c) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id_c).await);
    let new = lock_sim(&sim).owner.entity_of(id_c).unwrap();
    assert_ne!(old, new);
    assert!(!lock_sim(&sim).owner.contains_entity(old));
    lock_sim(&sim).tick_n(2);
    drain_frames(&mut uni_a, &mut view, Duration::from_millis(200)).await;
    assert!(
        view.entities
            .contains_key(&super::snapshot::to_wire_id(new))
    );
    assert!(
        !view
            .entities
            .contains_key(&super::snapshot::to_wire_id(old))
    );
    let _ = client_c;
    server.shutdown();
}

#[tokio::test]
async fn slow_snapshot_client_does_not_block_simulation_or_peer() {
    let (server, sim) = spawn_gameplay().await;
    let (client_a, mut send_a, _ra, id_a) = handshake_ok(server.addr).await;
    let (_client_b, _sb, _rb, id_b) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id_a).await);
    assert!(wait_attached(&sim, id_b).await);
    write_input(&mut send_a, 1, MoveAxis::Right, false, false).await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.last_received(id_a) == Some(1)
            },
            Duration::from_secs(2),
        )
        .await
    );
    lock_sim(&sim).tick_n(24);
    let mut uni_a = accept_snapshot_stream(&client_a).await;
    assert_eq!(lock_sim(&sim).owner.ticks(), 24);
    let mut view = ReplicaView::new();
    for _ in 0..24 {
        match timeout(
            Duration::from_millis(200),
            read_replication_frame(&mut uni_a),
        )
        .await
        {
            Ok(frame) => view.apply(frame),
            Err(_) => break,
        }
    }
    assert!(view.snapshot_sequence >= 1);
    assert_eq!(view.player_count(), 2);
    server.shutdown();
}

fn debug_sword() -> purgatory_common::ContentId {
    purgatory_common::ContentId::from_authored("equipment.debug.practice_sword").unwrap()
}

async fn expect_equipment(recv: &mut RecvStream) -> ServerEquipment {
    timeout(Duration::from_secs(2), async {
        loop {
            match read_server_control(recv).await {
                Ok(ServerControl::Equipment(event)) => return event,
                Ok(_) => {}
                Err(err) => panic!("control read failed: {err}"),
            }
        }
    })
    .await
    .expect("equipment control")
}

#[tokio::test]
async fn two_clients_converge_on_authoritative_equipment() {
    let (server, sim) = spawn_gameplay().await;
    let (client_a, mut send_a, mut recv_a, id_a) = handshake_ok(server.addr).await;
    let (client_b, mut send_b, _recv_b, id_b) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id_a).await);
    assert!(wait_attached(&sim, id_b).await);
    assert!(lock_sim(&sim).owner.set_player_x(id_a, 0.0));
    assert!(lock_sim(&sim).owner.set_player_x(id_b, 0.0));
    write_input(&mut send_a, 1, MoveAxis::Neutral, false, false).await;
    write_input(&mut send_b, 1, MoveAxis::Neutral, false, false).await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.last_received(id_a) == Some(1) && g.owner.last_received(id_b) == Some(1)
            },
            Duration::from_secs(2),
        )
        .await
    );
    lock_sim(&sim).tick_n(8);
    let mut uni_a = accept_snapshot_stream(&client_a).await;
    let mut uni_b = accept_snapshot_stream(&client_b).await;
    let mut view_a = read_latest_view(&mut uni_a).await;
    let mut view_b = read_latest_view(&mut uni_b).await;
    assert_eq!(view_a.player_count(), 2);
    assert_eq!(view_b.player_count(), 2);
    let ea = super::snapshot::to_wire_id(lock_sim(&sim).owner.entity_of(id_a).unwrap());
    assert!(view_a.equipment.get(&ea).copied().flatten().is_none());
    assert!(view_b.equipment.get(&ea).copied().flatten().is_none());

    write_control(
        &mut send_a,
        ClientControl::Equip(EquipRequest {
            seq: 1,
            slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
            content_id: debug_sword(),
        }),
    )
    .await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.entity_of(id_a).and_then(|actor| {
                    g.owner
                        .world()
                        .equipment_slot(actor, purgatory_simulation::EquipmentSlot::Weapon)
                }) == Some(debug_sword())
            },
            Duration::from_secs(2),
        )
        .await
    );
    assert_eq!(
        expect_equipment(&mut recv_a).await,
        ServerEquipment::Accepted { seq: 1 }
    );
    lock_sim(&sim).tick_n(4);
    drain_frames(&mut uni_a, &mut view_a, Duration::from_millis(120)).await;
    drain_frames(&mut uni_b, &mut view_b, Duration::from_millis(120)).await;
    let slot = purgatory_simulation::EquipmentSlot::Weapon as u8;
    let a_eq = view_a
        .equipment
        .get(&ea)
        .copied()
        .flatten()
        .expect("A local equipment domain");
    let b_eq = view_b
        .equipment
        .get(&ea)
        .copied()
        .flatten()
        .expect("B remote equipment domain");
    assert_eq!(a_eq.get(slot), Some(debug_sword()));
    assert_eq!(b_eq.get(slot), Some(debug_sword()));
    assert_eq!(a_eq, b_eq);

    write_control(
        &mut send_a,
        ClientControl::Unequip(UnequipRequest { seq: 2, slot }),
    )
    .await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.entity_of(id_a).is_some_and(|actor| {
                    g.owner
                        .world()
                        .equipment_of(actor)
                        .is_some_and(|s| s.is_empty())
                })
            },
            Duration::from_secs(2),
        )
        .await
    );
    assert_eq!(
        expect_equipment(&mut recv_a).await,
        ServerEquipment::Accepted { seq: 2 }
    );
    lock_sim(&sim).tick_n(4);
    drain_frames(&mut uni_a, &mut view_a, Duration::from_millis(120)).await;
    drain_frames(&mut uni_b, &mut view_b, Duration::from_millis(120)).await;
    let a_empty = view_a
        .equipment
        .get(&ea)
        .copied()
        .flatten()
        .expect("domain");
    let b_empty = view_b
        .equipment
        .get(&ea)
        .copied()
        .flatten()
        .expect("domain");
    assert!(a_empty.get(slot).is_none());
    assert!(b_empty.get(slot).is_none());
    assert!(a_empty.is_empty());
    assert_eq!(a_empty, b_empty);

    let updates_after = view_a.equipment_updates.max(view_b.equipment_updates);
    lock_sim(&sim).tick_n(8);
    drain_frames(&mut uni_a, &mut view_a, Duration::from_millis(80)).await;
    drain_frames(&mut uni_b, &mut view_b, Duration::from_millis(80)).await;
    assert_eq!(
        view_a.equipment_updates.max(view_b.equipment_updates),
        updates_after,
        "stable equipment must not emit recurring equipment Updates"
    );

    write_control(
        &mut send_a,
        ClientControl::Equip(EquipRequest {
            seq: 3,
            slot: purgatory_simulation::EquipmentSlot::Headwear as u8,
            content_id: debug_sword(),
        }),
    )
    .await;
    assert_eq!(
        expect_equipment(&mut recv_a).await,
        ServerEquipment::Rejected {
            seq: 3,
            reason: EquipmentRejectReason::SlotMismatch,
        }
    );
    {
        let g = lock_sim(&sim);
        let actor = g.owner.entity_of(id_a).unwrap();
        assert!(g.owner.world().equipment_of(actor).unwrap().is_empty());
    }
    lock_sim(&sim).tick_n(4);
    drain_frames(&mut uni_b, &mut view_b, Duration::from_millis(80)).await;
    let b_after_reject = view_b
        .equipment
        .get(&ea)
        .copied()
        .flatten()
        .expect("domain");
    assert!(b_after_reject.get(slot).is_none());
    assert!(b_after_reject.is_empty());
    server.shutdown();
}

#[tokio::test]
async fn reconnect_reconstructs_remote_equipment_from_baseline() {
    let (server, sim) = spawn_gameplay().await;
    let (_client_a, mut send_a, mut recv_a, id_a) = handshake_ok(server.addr).await;
    let (client_b, send_b, recv_b, id_b) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id_a).await);
    assert!(wait_attached(&sim, id_b).await);
    assert!(lock_sim(&sim).owner.set_player_x(id_a, 0.0));
    assert!(lock_sim(&sim).owner.set_player_x(id_b, 0.0));
    write_control(
        &mut send_a,
        ClientControl::Equip(EquipRequest {
            seq: 1,
            slot: purgatory_simulation::EquipmentSlot::Weapon as u8,
            content_id: debug_sword(),
        }),
    )
    .await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.entity_of(id_a).and_then(|actor| {
                    g.owner
                        .world()
                        .equipment_slot(actor, purgatory_simulation::EquipmentSlot::Weapon)
                }) == Some(debug_sword())
            },
            Duration::from_secs(2),
        )
        .await
    );
    assert_eq!(
        expect_equipment(&mut recv_a).await,
        ServerEquipment::Accepted { seq: 1 }
    );
    drop((client_b, send_b, recv_b));
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.entity_of(id_b).is_none()
            },
            Duration::from_secs(2),
        )
        .await
    );
    let (client_b2, _send_b2, _recv_b2, id_b2) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id_b2).await);
    assert!(lock_sim(&sim).owner.set_player_x(id_b2, 0.0));
    lock_sim(&sim).tick_n(8);
    let mut uni_b = accept_snapshot_stream(&client_b2).await;
    let view_b = read_latest_view(&mut uni_b).await;
    let ea = super::snapshot::to_wire_id(lock_sim(&sim).owner.entity_of(id_a).unwrap());
    let eq = view_b
        .equipment
        .get(&ea)
        .copied()
        .flatten()
        .expect("reconnect baseline equipment");
    assert_eq!(
        eq.get(purgatory_simulation::EquipmentSlot::Weapon as u8),
        Some(debug_sword())
    );
    server.shutdown();
}

fn basic_strike_id() -> purgatory_common::ContentId {
    purgatory_common::ContentId::from_authored("skill.basic.strike").unwrap()
}

async fn expect_ability(recv: &mut RecvStream) -> ServerAbility {
    timeout(Duration::from_secs(2), async {
        loop {
            match read_server_control(recv).await {
                Ok(ServerControl::Ability(event)) => return event,
                Ok(_) => {}
                Err(err) => panic!("control read failed: {err}"),
            }
        }
    })
    .await
    .expect("ability control")
}

#[tokio::test]
async fn ability_activate_empty_swing_over_quic() {
    let (server, sim) = spawn_gameplay().await;
    let (_client, mut send, mut recv, id) = handshake_ok(server.addr).await;
    assert!(wait_attached(&sim, id).await);
    {
        let g = lock_sim(&sim);
        let actor = g.owner.entity_of(id).expect("actor");
        assert!(g.owner.world().health_of(actor).is_some());
        assert!(g.owner.world().ability_granted(actor, basic_strike_id()));
    }
    write_control(
        &mut send,
        ClientControl::AbilityActivate(AbilityActivateRequest {
            seq: 1,
            ability_id: basic_strike_id(),
            selected: None,
        }),
    )
    .await;
    assert!(
        wait_until(
            || {
                let mut g = lock_sim(&sim);
                g.pump();
                g.owner.entity_of(id).is_some_and(|actor| {
                    g.owner.world().active_action(actor).is_some_and(|action| {
                        action.kind
                            == purgatory_simulation::ActionKind::Ability {
                                id: basic_strike_id(),
                            }
                    })
                })
            },
            Duration::from_secs(2),
        )
        .await
    );
    assert_eq!(
        expect_ability(&mut recv).await,
        ServerAbility::Accepted { seq: 1 }
    );
    lock_sim(&sim).tick_n(12);
    {
        let g = lock_sim(&sim);
        let actor = g.owner.entity_of(id).expect("actor");
        assert!(g.owner.world().active_action(actor).is_none());
        assert_eq!(
            g.owner.world().health_of(actor).unwrap().current,
            purgatory_simulation::PLAYER_HEALTH_MAX
        );
    }
    server.shutdown();
}

#[test]
fn replication_uni_is_opened_once_and_write_failure_ends_the_session() {
    let src = include_str!("handshake.rs");
    assert_eq!(
        src.matches("open_uni()").count(),
        1,
        "must not open_uni per frame or reopen a sibling replication stream"
    );
    assert!(
        src.contains("if write_failed"),
        "write_all failure must tear down the session"
    );
    assert!(
        !src.lines().any(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("snap_send = None")
        }),
        "must not drop the stream and continue later frames"
    );
}
