//! Dedicated Tokio thread for Quinn IO.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use quinn::crypto::rustls::QuicClientConfig;
use quinn::{ClientConfig, Connection, Endpoint, RecvStream, SendStream, TransportConfig};
use tokio::sync::mpsc;

use purgatory_protocol::{
    ALPN_PROTOCOL, ClientControl, HANDSHAKE_TIMEOUT, Hello, PING_INTERVAL, PROTOCOL_VERSION,
    ServerControl, ServerDatagram, decode_server_control, decode_server_datagram,
    encode_client_control, encode_client_datagram, encode_frame, peek_frame_len,
};

use super::cert::DevOnlySkipServerVerification;
use super::config::ClientEndpointConfig;
use super::state::{
    ConnectionState, LocalConnectionError, NetworkCommand, NetworkEvent, NetworkView,
};

const CMD_CAP: usize = 8;
const EVT_CAP: usize = 32;
const MAX_OUTSTANDING_PINGS: usize = 4;

/// Handle owned by the winit thread.
pub struct NetworkHandle {
    commands: mpsc::Sender<NetworkCommand>,
    events: mpsc::Receiver<NetworkEvent>,
    thread: Option<JoinHandle<()>>,
    pub view: NetworkView,
}

impl NetworkHandle {
    pub fn start(config: ClientEndpointConfig) -> Result<Self, String> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CAP);
        let (evt_tx, evt_rx) = mpsc::channel(EVT_CAP);
        let thread = std::thread::Builder::new()
            .name("purgatory-net".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("network tokio runtime");
                runtime.block_on(network_loop(config, cmd_rx, evt_tx));
            })
            .map_err(|err| format!("network thread: {err}"))?;
        let handle = Self {
            commands: cmd_tx,
            events: evt_rx,
            thread: Some(thread),
            view: NetworkView::new(config.server),
        };
        let _ = handle.try_send(NetworkCommand::Connect);
        Ok(handle)
    }

    /// Non-blocking. Never stall the render thread.
    pub fn try_send(&self, command: NetworkCommand) -> bool {
        self.commands.try_send(command).is_ok()
    }

    /// Drain available events into [`Self::view`].
    pub fn poll(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            log_event(&event);
            self.view.apply(event);
        }
    }

    pub fn request_reconnect(&self) {
        match self.view.state {
            ConnectionState::Disconnected | ConnectionState::Rejected => {
                let _ = self.try_send(NetworkCommand::Connect);
            }
            _ => {
                let _ = self.try_send(NetworkCommand::Disconnect);
                let _ = self.try_send(NetworkCommand::Connect);
            }
        }
    }

    pub fn request_disconnect(&self) {
        let _ = self.try_send(NetworkCommand::Disconnect);
    }
}

impl Drop for NetworkHandle {
    fn drop(&mut self) {
        let _ = self.commands.try_send(NetworkCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn log_event(event: &NetworkEvent) {
    match event {
        NetworkEvent::State(ConnectionState::Connecting) => {
            println!("PURGATORY client connecting");
        }
        NetworkEvent::Connected {
            connection_id,
            protocol_version,
            ..
        } => {
            println!(
                "PURGATORY client connected connection_id={connection_id} protocol={protocol_version}"
            );
        }
        NetworkEvent::Rejected { reason } => {
            println!("PURGATORY client rejected reason={}", reason.code.as_str());
        }
        NetworkEvent::Disconnected {
            error: LocalConnectionError::ConnectFailed,
        } => {
            println!("PURGATORY client disconnected reason=connect failed");
        }
        NetworkEvent::Disconnected { error } => {
            println!("PURGATORY client disconnected reason={}", error.as_str());
        }
        NetworkEvent::State(_) | NetworkEvent::RttUpdated { .. } => {}
    }
}

fn emit(tx: &mpsc::Sender<NetworkEvent>, view_drops: &mut u64, event: NetworkEvent) {
    if tx.try_send(event).is_err() {
        *view_drops = view_drops.saturating_add(1);
    }
}

async fn network_loop(
    config: ClientEndpointConfig,
    mut commands: mpsc::Receiver<NetworkCommand>,
    events: mpsc::Sender<NetworkEvent>,
) {
    let endpoint = match make_endpoint() {
        Ok(ep) => ep,
        Err(err) => {
            eprintln!("PURGATORY client network endpoint failed: {err}");
            return;
        }
    };
    let mut dropped = 0u64;
    loop {
        let cmd = commands.recv().await;
        match cmd {
            None | Some(NetworkCommand::Shutdown) => {
                endpoint.close(0u32.into(), b"shutdown");
                break;
            }
            Some(NetworkCommand::Disconnect) => {}
            Some(NetworkCommand::Connect) => {
                run_session(
                    &endpoint,
                    config.server,
                    &mut commands,
                    &events,
                    &mut dropped,
                )
                .await;
            }
        }
    }
}

async fn run_session(
    endpoint: &Endpoint,
    server: SocketAddr,
    commands: &mut mpsc::Receiver<NetworkCommand>,
    events: &mpsc::Sender<NetworkEvent>,
    dropped: &mut u64,
) {
    emit(
        events,
        dropped,
        NetworkEvent::State(ConnectionState::Connecting),
    );
    let connecting = match endpoint.connect(server, "localhost") {
        Ok(c) => c,
        Err(_) => {
            emit(
                events,
                dropped,
                NetworkEvent::Disconnected {
                    error: LocalConnectionError::ConnectFailed,
                },
            );
            return;
        }
    };

    let connection = tokio::select! {
        result = connecting => match result {
            Ok(conn) => conn,
            Err(_) => {
                emit(
                    events,
                    dropped,
                    NetworkEvent::Disconnected {
                        error: LocalConnectionError::ConnectFailed,
                    },
                );
                return;
            }
        },
        cmd = commands.recv() => {
            match cmd {
                Some(NetworkCommand::Connect) => {
                    emit(
                        events,
                        dropped,
                        NetworkEvent::Disconnected {
                            error: LocalConnectionError::ConnectFailed,
                        },
                    );
                    return;
                }
                Some(NetworkCommand::Disconnect) | Some(NetworkCommand::Shutdown) | None => {
                    emit(
                        events,
                        dropped,
                        NetworkEvent::Disconnected {
                            error: LocalConnectionError::ClientClosed,
                        },
                    );
                    return;
                }
            }
        }
    };

    emit(
        events,
        dropped,
        NetworkEvent::State(ConnectionState::Handshaking),
    );

    if let Err(outcome) = handshake_and_live(connection, commands, events, dropped).await {
        emit(events, dropped, outcome);
    }
}

async fn handshake_and_live(
    connection: Connection,
    commands: &mut mpsc::Receiver<NetworkCommand>,
    events: &mpsc::Sender<NetworkEvent>,
    dropped: &mut u64,
) -> Result<(), NetworkEvent> {
    let (mut send, mut recv) = tokio::select! {
        opened = connection.open_bi() => {
            opened.map_err(|_| NetworkEvent::Disconnected {
                error: LocalConnectionError::TransportError,
            })?
        }
        cmd = commands.recv() => {
            connection.close(0u32.into(), b"cancel");
            return Err(disconnect_from_command(cmd));
        }
    };

    let hello = ClientControl::Hello(Hello {
        protocol_version: PROTOCOL_VERSION,
        client_build: format!("purgatory-client-{}", env!("CARGO_PKG_VERSION")),
    });
    write_client_control(&mut send, &hello)
        .await
        .map_err(|_| NetworkEvent::Disconnected {
            error: LocalConnectionError::TransportError,
        })?;

    let control = tokio::select! {
        msg = read_server_control(&mut recv) => msg,
        () = tokio::time::sleep(HANDSHAKE_TIMEOUT) => {
            connection.close(0u32.into(), b"handshake");
            return Err(NetworkEvent::Disconnected {
                error: LocalConnectionError::TransportError,
            });
        }
        cmd = commands.recv() => {
            connection.close(0u32.into(), b"cancel");
            return Err(disconnect_from_command(cmd));
        }
    };

    match control {
        Ok(ServerControl::Welcome(welcome)) => {
            emit(
                events,
                dropped,
                NetworkEvent::Connected {
                    connection_id: welcome.connection_id,
                    protocol_version: welcome.protocol_version,
                    server_tick_rate: welcome.server_tick_rate,
                },
            );
        }
        Ok(ServerControl::Disconnect(reason)) => {
            connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
            return Err(NetworkEvent::Rejected { reason });
        }
        Err(_) => {
            connection.close(0u32.into(), b"handshake");
            return Err(NetworkEvent::Disconnected {
                error: LocalConnectionError::TransportError,
            });
        }
    }

    live_loop(connection, send, recv, commands, events, dropped).await
}

async fn live_loop(
    connection: Connection,
    _send: SendStream,
    mut recv: RecvStream,
    commands: &mut mpsc::Receiver<NetworkCommand>,
    events: &mpsc::Sender<NetworkEvent>,
    dropped: &mut u64,
) -> Result<(), NetworkEvent> {
    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut nonce: u64 = 1;
    let mut outstanding: HashMap<u64, Instant> = HashMap::new();

    loop {
        tokio::select! {
            cmd = commands.recv() => {
                match cmd {
                    Some(NetworkCommand::Connect) => {
                        connection.close(0u32.into(), b"reconnect");
                        return Err(NetworkEvent::Disconnected {
                            error: LocalConnectionError::ClientClosed,
                        });
                    }
                    Some(NetworkCommand::Disconnect) | Some(NetworkCommand::Shutdown) | None => {
                        connection.close(0u32.into(), b"client");
                        return Err(NetworkEvent::Disconnected {
                            error: LocalConnectionError::ClientClosed,
                        });
                    }
                }
            }
            datagram = connection.read_datagram() => {
                match datagram {
                    Ok(bytes) => {
                        if let Ok(ServerDatagram::Pong { nonce }) = decode_server_datagram(&bytes)
                            && let Some(sent) = outstanding.remove(&nonce)
                        {
                            emit(
                                events,
                                dropped,
                                NetworkEvent::RttUpdated {
                                    rtt: sent.elapsed(),
                                },
                            );
                        }
                    }
                    Err(_) => {
                        return Err(NetworkEvent::Disconnected {
                            error: LocalConnectionError::TransportError,
                        });
                    }
                }
            }
            control = read_server_control(&mut recv) => {
                match control {
                    Ok(ServerControl::Disconnect(reason)) => {
                        connection.close(reason.code.as_u8().into(), reason.code.as_str().as_bytes());
                        return Err(NetworkEvent::Rejected { reason });
                    }
                    Ok(ServerControl::Welcome(_)) => {
                        return Err(NetworkEvent::Disconnected {
                            error: LocalConnectionError::TransportError,
                        });
                    }
                    Err(_) => {
                        return Err(NetworkEvent::Disconnected {
                            error: LocalConnectionError::TransportError,
                        });
                    }
                }
            }
            _ = ping.tick() => {
                let id = nonce;
                nonce = nonce.wrapping_add(1);
                if outstanding.len() >= MAX_OUTSTANDING_PINGS {
                    outstanding.clear();
                }
                outstanding.insert(id, Instant::now());
                if let Ok(payload) = encode_client_datagram(id) {
                    let _ = connection.send_datagram(payload.into());
                }
            }
        }
    }
}

fn disconnect_from_command(cmd: Option<NetworkCommand>) -> NetworkEvent {
    match cmd {
        Some(NetworkCommand::Disconnect) | Some(NetworkCommand::Shutdown) | None => {
            NetworkEvent::Disconnected {
                error: LocalConnectionError::ClientClosed,
            }
        }
        Some(NetworkCommand::Connect) => NetworkEvent::Disconnected {
            error: LocalConnectionError::ConnectFailed,
        },
    }
}

fn make_endpoint() -> Result<Endpoint, String> {
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(DevOnlySkipServerVerification::new())
        .with_no_client_auth();
    tls.alpn_protocols = vec![ALPN_PROTOCOL.to_vec()];
    let mut client = ClientConfig::new(Arc::new(
        QuicClientConfig::try_from(tls).map_err(|err| format!("quic client tls: {err}"))?,
    ));
    let mut transport = TransportConfig::default();
    transport.datagram_receive_buffer_size(Some(4096));
    transport.datagram_send_buffer_size(4096);
    client.transport_config(Arc::new(transport));
    let mut endpoint = Endpoint::client(SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 0)))
        .map_err(|err| format!("client bind: {err}"))?;
    endpoint.set_default_client_config(client);
    Ok(endpoint)
}

async fn write_client_control(
    send: &mut SendStream,
    msg: &ClientControl,
) -> Result<(), quinn::WriteError> {
    let payload = encode_client_control(msg).expect("encode hello");
    let frame = encode_frame(&payload).expect("frame hello");
    send.write_all(&frame).await
}

async fn read_server_control(recv: &mut RecvStream) -> Result<ServerControl, ()> {
    let mut prefix = [0u8; 4];
    recv.read_exact(&mut prefix).await.map_err(|_| ())?;
    let len = peek_frame_len(&prefix).map_err(|_| ())?;
    let mut payload = vec![0u8; len as usize];
    recv.read_exact(&mut payload).await.map_err(|_| ())?;
    decode_server_control(&payload).map_err(|_| ())
}
