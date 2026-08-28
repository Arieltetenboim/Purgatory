//! Headless dedicated-server networking (Quinn / QUIC).
//!
//! Packet arrival does not mutate [`purgatory_simulation::World`]. Connection
//! tasks hand off validated `InputCommand` values through bounded channels to
//! a simulation-thread [`gameplay::GameplayOwner`], which owns
//! `ConnectionId → EntityId` and applies FOOTNOTE on the fixed tick.
//!
//! The accept loop never awaits a client's handshake. Each Incoming is either
//! refused at the concurrency cap or spawned as a per-connection task.
//! Every remote peer remains untrusted after QUIC connect and Hello/Welcome.

mod abuse;
mod cert;
mod config;
mod endpoint;
mod gameplay;
mod handshake;
mod session;
mod snapshot;
mod stats;

pub use config::ServerEndpointConfig;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use purgatory_protocol::DisconnectReasonCode;
use tokio::sync::Semaphore;

use purgatory_simulation::{SimulationClock, TICK_DURATION};

pub(crate) struct IncomingDispatch {
    pub sessions: Arc<Mutex<session::SessionTable>>,
    pub ids: Arc<session::ConnectionIdAllocator>,
    pub abuse: abuse::NetworkAbuseConfig,
    pub limiter: Arc<Semaphore>,
    pub inflight: Arc<AtomicU64>,
    pub stats: Arc<stats::ServerNetStats>,
    pub gameplay: Option<gameplay::GameplayTx>,
}

pub(crate) fn dispatch_incoming(incoming: quinn::Incoming, ctx: IncomingDispatch) {
    let Ok(permit) = ctx.limiter.try_acquire_owned() else {
        ctx.stats.admission_refused.fetch_add(1, Ordering::Relaxed);
        incoming.refuse();
        return;
    };
    let prev = ctx.inflight.fetch_add(1, Ordering::Relaxed);
    let current = prev.saturating_add(1);
    ctx.stats.max_inflight.fetch_max(current, Ordering::Relaxed);
    tokio::spawn(async move {
        handshake::handle_incoming(
            incoming,
            ctx.sessions,
            ctx.ids,
            ctx.abuse,
            ctx.stats,
            ctx.gameplay,
        )
        .await;
        ctx.inflight.fetch_sub(1, Ordering::Relaxed);
        drop(permit);
    });
}

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

    let (life_tx, mut life_rx) = tokio::sync::mpsc::channel(gameplay::lifecycle_cap());
    let (input_tx, mut input_rx) = tokio::sync::mpsc::channel(gameplay::input_cap());
    let gameplay_tx = gameplay::GameplayTx {
        lifecycle: life_tx,
        input: input_tx,
    };
    let mut owner = gameplay::GameplayOwner::new();

    let mut clock = SimulationClock::new();
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
                dispatch_incoming(
                    incoming,
                    IncomingDispatch {
                        sessions: bound.sessions.clone(),
                        ids: bound.ids.clone(),
                        abuse: config.abuse,
                        limiter: bound.limiter.clone(),
                        inflight: bound.inflight_tasks.clone(),
                        stats: bound.stats.clone(),
                        gameplay: Some(gameplay_tx.clone()),
                    },
                );
            }
            _ = ticker.tick() => {
                owner.drain(&mut life_rx, &mut input_rx);
                let now = Instant::now();
                let elapsed = now.saturating_duration_since(last);
                last = now;
                let update = clock.advance(elapsed);
                let dt = TICK_DURATION.as_secs_f32();
                for _ in 0..update.ticks_executed {
                    owner.simulate_tick(dt);
                }
                bound.stats.input_accepted.store(owner.input_accepted, Ordering::Relaxed);
                bound.stats.input_duplicate.store(owner.input_duplicate, Ordering::Relaxed);
                bound.stats.input_stale.store(owner.input_stale, Ordering::Relaxed);
                bound.stats.snapshots_built.store(owner.snapshots_built, Ordering::Relaxed);
                bound.stats.snapshot_sequence.store(u64::from(owner.snapshot_sequence), Ordering::Relaxed);
                bound.stats.last_snapshot_entities.store(u64::from(owner.last_snapshot_entities), Ordering::Relaxed);
                bound.stats.snapshot_send_failed.store(owner.snapshot_send_failed, Ordering::Relaxed);
            }
            _ = tokio::signal::ctrl_c() => {
                println!("PURGATORY server shutting down");
                bound.endpoint.close(
                    u32::from(DisconnectReasonCode::ServerShutdown.as_u8()).into(),
                    b"shutdown",
                );
                let (active, peak) = {
                    let table = session::lock_sessions(&bound.sessions);
                    (table.len(), table.high_water())
                };
                println!(
                    "PURGATORY server stats {}",
                    bound.stats.summary(
                        active,
                        bound.inflight_tasks.load(Ordering::Relaxed),
                        peak,
                    )
                );
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
