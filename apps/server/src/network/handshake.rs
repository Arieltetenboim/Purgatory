//! Hello / Welcome handshake. All client bytes are untrusted.

use std::sync::Arc;
use std::time::{Duration, Instant};

use quinn::{Connection, RecvStream, SendStream};
use tokio::sync::Mutex;
use tokio::time::timeout;

use purgatory_protocol::{
    ClientControl, DisconnectReason, DisconnectReasonCode, Hello, PROTOCOL_VERSION, ServerControl,
    ServerDatagram, Welcome, decode_client_control, decode_client_datagram, encode_frame,
    encode_server_control, encode_server_datagram, peek_frame_len, validate_hello,
};
use purgatory_simulation::TICK_RATE_HZ;

use super::session::{ConnectionIdAllocator, ConnectionSession, SessionTable};

const SERVER_LABEL: &str = "purgatory-server-dev";

pub(crate) async fn handle_incoming(
    incoming: quinn::Incoming,
    sessions: Arc<Mutex<SessionTable>>,
    ids: Arc<ConnectionIdAllocator>,
    handshake_timeout: Duration,
) {
    let connecting_remote = incoming.remote_address();
    let connection = match incoming.await {
        Ok(conn) => conn,
        Err(err) => {
            println!("connection failed from {connecting_remote}: {err}");
            return;
        }
    };
    let remote = connection.remote_address();
    println!("connection accepted {remote}");

    let handshake = timeout(handshake_timeout, handshake_streams(&connection)).await;
    let (mut send, recv, hello) = match handshake {
        Ok(Ok(parts)) => parts,
        Ok(Err(reason)) => {
            println!(
                "handshake rejected {remote} reason={}",
                reason.code.as_str()
            );
            let _ = send_disconnect_best_effort(&connection, &reason).await;
            connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
            return;
        }
        Err(_) => {
            println!("handshake rejected {remote} reason=handshake timeout");
            let reason = DisconnectReason::new(DisconnectReasonCode::HandshakeTimeout, "no hello");
            let _ = send_disconnect_best_effort(&connection, &reason).await;
            connection.close(
                DisconnectReasonCode::HandshakeTimeout.as_u8().into(),
                b"timeout",
            );
            return;
        }
    };

    if let Err(reason) = validate_hello(&hello) {
        println!(
            "handshake rejected {remote} reason={}",
            reason.code.as_str()
        );
        let _ = write_server_control(&mut send, &ServerControl::Disconnect(reason.clone())).await;
        connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
        return;
    }

    let connection_id = ids.allocate();
    let welcome = Welcome {
        protocol_version: PROTOCOL_VERSION,
        connection_id,
        server_tick_rate: TICK_RATE_HZ,
        server_label: SERVER_LABEL.to_string(),
    };
    if write_server_control(&mut send, &ServerControl::Welcome(welcome))
        .await
        .is_err()
    {
        println!("handshake rejected {remote} reason=welcome write failed");
        connection.close(0u32.into(), b"welcome");
        return;
    }

    let session = ConnectionSession {
        connection_id,
        protocol_version: PROTOCOL_VERSION,
        connected_since: Instant::now(),
        remote,
    };
    sessions.lock().await.insert(session.clone());
    println!(
        "handshake accepted connection_id={} protocol={}",
        session.connection_id, session.protocol_version
    );

    serve_connection(connection, send, recv, session, sessions).await;
}

async fn handshake_streams(
    connection: &Connection,
) -> Result<(SendStream, RecvStream, Hello), DisconnectReason> {
    let (send, mut recv) = connection
        .accept_bi()
        .await
        .map_err(|_| DisconnectReason::new(DisconnectReasonCode::Malformed, "no control stream"))?;
    let control = read_client_control(&mut recv).await?;
    let ClientControl::Hello(hello) = control;
    Ok((send, recv, hello))
}

async fn serve_connection(
    connection: Connection,
    mut send: SendStream,
    mut recv: RecvStream,
    session: ConnectionSession,
    sessions: Arc<Mutex<SessionTable>>,
) {
    let id = session.connection_id;
    let remote = session.remote;
    loop {
        tokio::select! {
            datagram = connection.read_datagram() => {
                match datagram {
                    Ok(bytes) => {
                        if let Ok(nonce) = decode_client_datagram(&bytes)
                            && let Ok(payload) =
                                encode_server_datagram(ServerDatagram::Pong { nonce })
                        {
                            let _ = connection.send_datagram(payload.into());
                        }
                    }
                    Err(_) => break,
                }
            }
            control = read_client_control(&mut recv) => {
                match control {
                    Ok(ClientControl::Hello(_)) => {
                        let reason = DisconnectReason::new(
                            DisconnectReasonCode::UnexpectedMessage,
                            "repeated hello",
                        );
                        let _ = write_server_control(
                            &mut send,
                            &ServerControl::Disconnect(reason.clone()),
                        )
                        .await;
                        connection.close(
                            reason.code.as_u8().into(),
                            reason.code.as_str().as_bytes(),
                        );
                        break;
                    }
                    Err(_) => break,
                }
            }
        }
    }

    sessions.lock().await.remove(id);
    println!(
        "disconnect connection_id={id} peer={remote} lived_ms={}",
        session.connected_since.elapsed().as_millis()
    );
}

async fn read_client_control(recv: &mut RecvStream) -> Result<ClientControl, DisconnectReason> {
    let mut prefix = [0u8; 4];
    recv.read_exact(&mut prefix)
        .await
        .map_err(|_| DisconnectReason::new(DisconnectReasonCode::Malformed, "length prefix"))?;
    let len = peek_frame_len(&prefix).map_err(|err| {
        DisconnectReason::new(DisconnectReasonCode::Malformed, format!("frame {err}"))
    })?;
    let mut payload = vec![0u8; len as usize];
    recv.read_exact(&mut payload)
        .await
        .map_err(|_| DisconnectReason::new(DisconnectReasonCode::Malformed, "payload"))?;
    decode_client_control(&payload)
        .map_err(|_| DisconnectReason::new(DisconnectReasonCode::Malformed, "decode"))
}

async fn write_server_control(
    send: &mut SendStream,
    msg: &ServerControl,
) -> Result<(), DisconnectReason> {
    let payload = encode_server_control(msg)
        .map_err(|_| DisconnectReason::new(DisconnectReasonCode::Malformed, "encode"))?;
    let frame = encode_frame(&payload)
        .map_err(|_| DisconnectReason::new(DisconnectReasonCode::Malformed, "frame"))?;
    send.write_all(&frame)
        .await
        .map_err(|_| DisconnectReason::new(DisconnectReasonCode::Malformed, "write"))?;
    Ok(())
}

async fn send_disconnect_best_effort(connection: &Connection, reason: &DisconnectReason) {
    if let Ok((mut send, _)) = connection.open_bi().await
        && write_server_control(&mut send, &ServerControl::Disconnect(reason.clone()))
            .await
            .is_ok()
    {
        let _ = send.finish();
    }
}
