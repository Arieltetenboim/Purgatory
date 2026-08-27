//! Handshake and session tests. Localhost only. Never panic on bad input.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::crypto::rustls::QuicClientConfig;
use quinn::{ClientConfig, Connection, Endpoint, RecvStream, SendStream, TransportConfig};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::time::timeout;

use purgatory_protocol::{
    ClientControl, ConnectionId, DisconnectReasonCode, Hello, MAX_CONTROL_MESSAGE_BYTES,
    PROTOCOL_VERSION, ServerControl, decode_client_datagram, decode_server_control,
    encode_client_control, encode_client_datagram, encode_frame, peek_frame_len,
};

use super::config::ServerEndpointConfig;
use super::endpoint::{self, BoundEndpoint};
use super::handshake;
use super::install_crypto_provider;
use super::session::SessionTable;

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
    sessions: Arc<tokio::sync::Mutex<SessionTable>>,
    endpoint: Endpoint,
    accept: tokio::task::JoinHandle<()>,
}

impl TestServer {
    async fn spawn(handshake_timeout: Duration) -> Self {
        install_crypto_provider().expect("crypto");
        let mut config = ServerEndpointConfig::ephemeral();
        config.handshake_timeout = handshake_timeout;
        let bound = endpoint::bind(&config).expect("bind");
        let addr = bound.local_addr();
        let sessions = bound.sessions.clone();
        let endpoint = bound.endpoint.clone();
        let accept = tokio::spawn(async move {
            accept_loop(bound, handshake_timeout).await;
        });
        Self {
            addr,
            sessions,
            endpoint,
            accept,
        }
    }

    async fn session_count(&self) -> usize {
        self.sessions.lock().await.len()
    }

    async fn contains(&self, id: ConnectionId) -> bool {
        self.sessions.lock().await.contains(id)
    }

    fn shutdown(self) {
        self.endpoint.close(0u32.into(), b"test");
        self.accept.abort();
    }
}

async fn accept_loop(bound: BoundEndpoint, handshake_timeout: Duration) {
    let ids = bound.ids.clone();
    let sessions = bound.sessions.clone();
    while let Some(incoming) = bound.endpoint.accept().await {
        let sessions = sessions.clone();
        let ids = ids.clone();
        tokio::spawn(async move {
            handshake::handle_incoming(incoming, sessions, ids, handshake_timeout).await;
        });
    }
}

fn test_client_endpoint() -> Endpoint {
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

async fn write_hello(send: &mut SendStream, version: u32, build: &str) {
    let payload = encode_client_control(&ClientControl::Hello(Hello {
        protocol_version: version,
        client_build: build.into(),
    }))
    .expect("encode hello");
    let frame = encode_frame(&payload).expect("frame");
    send.write_all(&frame).await.expect("write hello");
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
    assert!(server.contains(id).await);
    assert_eq!(id.get(), 1);
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
    assert_eq!(server.session_count().await, 0);
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
    assert_eq!(server.session_count().await, 0);
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
    assert_eq!(server.session_count().await, 0);
    server.shutdown();
}

#[tokio::test]
async fn no_hello_times_out() {
    let server = TestServer::spawn(Duration::from_millis(400)).await;
    let _client = connect(server.addr).await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(server.session_count().await, 0);
    server.shutdown();
}

#[tokio::test]
async fn two_clients_get_distinct_connection_ids() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let (_a, _send_a, _recv_a, id_a) = handshake_ok(server.addr).await;
    let (_b, _send_b, _recv_b, id_b) = handshake_ok(server.addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_ne!(id_a, id_b);
    assert!(server.contains(id_a).await);
    assert!(server.contains(id_b).await);
    assert_eq!(server.session_count().await, 2);
    server.shutdown();
}

#[tokio::test]
async fn disconnect_removes_session() {
    let server = TestServer::spawn(Duration::from_secs(2)).await;
    let (client, _send, _recv, id) = handshake_ok(server.addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(server.contains(id).await);
    client.conn.close(0u32.into(), b"bye");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!server.contains(id).await);
    assert_eq!(server.session_count().await, 0);
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
    assert!(!server.contains(id).await);
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
