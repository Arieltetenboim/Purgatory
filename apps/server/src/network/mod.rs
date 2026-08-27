//! Headless dedicated-server networking (Quinn / QUIC).
//!
//! Networking is separable from simulation. Packet arrival does not modify
//! [`purgatory_simulation::World`]. Connecting or disconnecting must not change
//! the fixed tick rate.

mod cert;
mod config;
mod endpoint;
mod handshake;
mod session;

pub use config::ServerEndpointConfig;

use std::time::Instant;

use purgatory_simulation::{PlayerInput, SimulationClock, TICK_DURATION, World};

/// Bind, accept, handshake, and tick simulation until Ctrl+C.
pub fn run_blocking(config: ServerEndpointConfig) -> Result<(), String> {
    install_crypto_provider()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("purgatory-server")
        .build()
        .map_err(|err| format!("tokio runtime: {err}"))?;
    runtime.block_on(run(config))
}

pub(crate) fn install_crypto_provider() -> Result<(), String> {
    // Err means a provider is already installed; that is fine for tests.
    let _ = rustls::crypto::ring::default_provider().install_default();
    Ok(())
}

async fn run(config: ServerEndpointConfig) -> Result<(), String> {
    let bound = endpoint::bind(&config)?;
    println!("network listening on {}", bound.local_addr());

    let mut clock = SimulationClock::new();
    let mut world = World::footnote_test_stage();
    let mut last = Instant::now();
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(8));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            incoming = bound.endpoint.accept() => {
                let Some(incoming) = incoming else {
                    println!("PURGATORY server endpoint closed");
                    break;
                };
                let sessions = bound.sessions.clone();
                let ids = bound.ids.clone();
                let timeout = config.handshake_timeout;
                tokio::spawn(async move {
                    handshake::handle_incoming(incoming, sessions, ids, timeout).await;
                });
            }
            _ = ticker.tick() => {
                let now = Instant::now();
                let elapsed = now.saturating_duration_since(last);
                last = now;
                let update = clock.advance(elapsed);
                let dt = TICK_DURATION.as_secs_f32();
                for _ in 0..update.ticks_executed {
                    world.tick(dt, PlayerInput::idle());
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("PURGATORY server shutting down");
                bound.endpoint.close(0u32.into(), b"shutdown");
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
