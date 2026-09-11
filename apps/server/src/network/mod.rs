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
mod connection_lifecycle;
mod dialogue;
mod endpoint;
mod gameplay;
mod handshake;
mod load_pressure;
mod metrics_export;
mod network_pressure;
mod persist;
mod replication;
mod replication_fanout;
mod replication_policy;
mod session;
mod snapshot;
mod stats;
mod tick_domains;

pub use config::ServerEndpointConfig;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use purgatory_protocol::DisconnectReasonCode;
use tokio::sync::Semaphore;

use purgatory_simulation::{SimulationClock, TICK_DURATION};

use metrics_export::{MetricsExportCtx, TickSampleRing, spawn_metrics_export};
use tick_domains::TickDomainAccounting;

pub(crate) struct IncomingDispatch {
    pub sessions: Arc<Mutex<session::SessionTable>>,
    pub ids: Arc<session::ConnectionIdAllocator>,
    pub abuse: abuse::NetworkAbuseConfig,
    pub limiter: Arc<Semaphore>,
    pub inflight: Arc<AtomicU64>,
    pub stats: Arc<stats::ServerNetStats>,
    pub gameplay: Option<gameplay::GameplayTx>,
    pub persist: Option<persist::PersistenceHandle>,
    pub lifecycle: Arc<connection_lifecycle::ConnectionLifecycleBook>,
    pub pressure: Arc<network_pressure::NetworkPressureBook>,
}

pub(crate) fn dispatch_incoming(incoming: quinn::Incoming, ctx: IncomingDispatch) {
    let limiter = ctx.limiter.clone();
    let inflight = ctx.inflight.clone();
    let Ok(permit) = limiter.try_acquire_owned() else {
        ctx.stats.admission_refused.fetch_add(1, Ordering::Relaxed);
        incoming.refuse();
        return;
    };
    let prev = inflight.fetch_add(1, Ordering::Relaxed);
    let current = prev.saturating_add(1);
    ctx.stats.max_inflight.fetch_max(current, Ordering::Relaxed);
    tokio::spawn(async move {
        handshake::handle_incoming(incoming, ctx).await;
        inflight.fetch_sub(1, Ordering::Relaxed);
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

    bound.stats.admission_cap.store(
        config.abuse.max_inflight_connection_tasks as u64,
        Ordering::Relaxed,
    );

    let (gameplay_tx, mut life_rx, mut input_rx) =
        gameplay::gameplay_channels(config.lifecycle_cap, config.input_cap);
    let data_dir = persist::data_dir_from_env();
    println!("PURGATORY persist data_dir={}", data_dir.display());
    let persist = persist::PersistenceHandle::spawn(&data_dir)?;
    let mut owner = gameplay::GameplayOwner::new();
    owner.set_persist(persist.clone());
    let pressure = Arc::new(network_pressure::NetworkPressureBook::new());
    let lifecycle = Arc::new(connection_lifecycle::ConnectionLifecycleBook::new());
    owner.set_pressure(pressure.clone());
    let input_cap = config.input_cap;

    spawn_metrics_export(
        config.metrics_port,
        MetricsExportCtx {
            stats: bound.stats.clone(),
            sessions: bound.sessions.clone(),
        },
    );

    let mut clock = SimulationClock::new();
    let mut last = Instant::now();
    let mut ticker = tokio::time::interval(Duration::from_millis(8));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut samples = TickSampleRing::new(120);
    let mut domains = TickDomainAccounting::new();
    domains.attach_live_sources(
        bound.stats.clone(),
        pressure.clone(),
        lifecycle.clone(),
        bound.sessions.clone(),
    );
    if let Some(dir) = domains.artifact_dir() {
        println!("PURGATORY capacity artifacts dir={}", dir.display());
    }
    let outer_period = Duration::from_millis(8);

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
                        persist: Some(persist.clone()),
                        lifecycle: lifecycle.clone(),
                        pressure: pressure.clone(),
                    },
                );
            }
            _ = ticker.tick() => {
                let wake = Instant::now();
                let elapsed = wake.saturating_duration_since(last);
                last = wake;
                let lateness = elapsed.saturating_sub(outer_period);

                let drain_start = Instant::now();
                owner.drain(&mut life_rx, &mut input_rx);
                let input_depth = input_cap.saturating_sub(input_rx.capacity()) as u64;
                bound.stats.input_queue_current.store(input_depth, Ordering::Relaxed);
                bound.stats.input_queue_max.fetch_max(input_depth, Ordering::Relaxed);

                let update = clock.advance(elapsed);
                bound.stats.catch_up_ticks.fetch_add(
                    u64::from(update.ticks_executed.saturating_sub(1)),
                    Ordering::Relaxed,
                );
                bound.stats.discarded_ns.fetch_add(
                    u64::try_from(update.discarded.as_nanos()).unwrap_or(u64::MAX),
                    Ordering::Relaxed,
                );

                let dt = TICK_DURATION.as_secs_f32();
                for _ in 0..update.ticks_executed {
                    let tick_start = Instant::now();
                    let mut domain = owner.simulate_tick(dt);
                    let work = tick_start.elapsed();
                    bound.stats.tick_count.fetch_add(1, Ordering::Relaxed);
                    if work > TICK_DURATION {
                        bound.stats.tick_overrun_count.fetch_add(1, Ordering::Relaxed);
                        domains.note_overrun();
                        owner.set_tick_overrun_hint(true);
                    } else {
                        owner.set_tick_overrun_hint(false);
                    }
                    samples.push(work, lateness);
                    domain.total = work;
                    domain.finalize(purgatory_common::capacity_detail_enabled());
                    domains.push(domain);
                    domains.note_interest_locality(owner.world_interest_locality());
                    domains.note_replication_fanout(owner.replication_fanout_snapshot());
                    domains.note_gameplay_workload(gameplay_workload_from_runtime(
                        owner.runtime_stats(),
                    ));
                    // Drain between ticks so long snapshot work does not starve
                    // awaiting producers on the input handoff.
                    owner.drain(&mut life_rx, &mut input_rx);
                    let depth = input_cap.saturating_sub(input_rx.capacity()) as u64;
                    bound.stats.input_queue_current.store(depth, Ordering::Relaxed);
                    bound.stats.input_queue_max.fetch_max(depth, Ordering::Relaxed);
                }
                // If no sim ticks this wake, still record lateness with zero work.
                if update.ticks_executed == 0 {
                    samples.push(drain_start.elapsed(), lateness);
                }
                samples.publish(&bound.stats);

                mirror_owner_stats(&bound.stats, &owner);
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
                owner.flush_persistent_snapshots();
                domains.force_write();
                persist
                    .shutdown(config.persistence_shutdown_timeout)
                    .await;
                break;
            }
        }
    }
    Ok(())
}

fn gameplay_workload_from_runtime(
    rt: purgatory_simulation::RuntimeStats,
) -> purgatory_common::GameplayWorkloadSnapshot {
    purgatory_common::GameplayWorkloadSnapshot {
        schema: purgatory_common::GameplayWorkloadSnapshot::SCHEMA,
        wall_secs: 0.0,
        npcs_active: rt.npcs_active,
        npc_updates_total: rt.npc_updates_total,
        actions_attempted_total: rt.actions_attempted_total,
        actions_started_total: rt.actions_started_total,
        actions_completed_total: rt.actions_completed_total,
        actions_rejected_total: rt.actions_rejected_total,
        health_mutations_total: rt.health_mutations_total,
        deaths_total: rt.deaths_total,
        respawns_total: rt.respawns_total,
        pulse_ticks_total: rt.pulse_ticks_total,
        effects_active: rt.effects_active,
        effects_applied_total: rt.effects_applied_total,
        effects_expired_total: rt.effects_expired_total,
        actions_active: rt.actions_active,
        scheduler_queued: rt.scheduler_queued,
    }
}

fn mirror_owner_stats(stats: &stats::ServerNetStats, owner: &gameplay::GameplayOwner) {
    stats
        .input_accepted
        .store(owner.input_accepted, Ordering::Relaxed);
    stats
        .input_duplicate
        .store(owner.input_duplicate, Ordering::Relaxed);
    stats
        .input_stale
        .store(owner.input_stale, Ordering::Relaxed);
    stats
        .input_queue_overflow
        .store(owner.input_queue_overflow, Ordering::Relaxed);
    stats
        .snapshots_built
        .store(owner.snapshots_built, Ordering::Relaxed);
    stats
        .snapshot_build_count
        .store(owner.snapshot_build_count, Ordering::Relaxed);
    stats
        .snapshot_sequence
        .store(u64::from(owner.snapshot_sequence), Ordering::Relaxed);
    stats
        .last_snapshot_entities
        .store(u64::from(owner.last_snapshot_entities), Ordering::Relaxed);
    stats
        .snapshot_send_failed
        .store(owner.snapshot_send_failed, Ordering::Relaxed);
    stats
        .player_entity_spawned
        .store(owner.player_entity_spawned, Ordering::Relaxed);
    stats
        .player_entity_despawned
        .store(owner.player_entity_despawned, Ordering::Relaxed);
    stats
        .duplicate_session_detected
        .store(owner.duplicate_session_detected, Ordering::Relaxed);
    stats
        .session_queue_max
        .store(owner.session_queue_max, Ordering::Relaxed);
    stats
        .snapshot_build_time_max_micros
        .store(owner.snapshot_build_time_max_us, Ordering::Relaxed);
    stats.aoi_enters.store(owner.aoi_enters, Ordering::Relaxed);
    stats.aoi_leaves.store(owner.aoi_leaves, Ordering::Relaxed);
    stats
        .aoi_updates
        .store(owner.aoi_updates, Ordering::Relaxed);
    stats
        .aoi_churn_reentry
        .store(owner.aoi_churn_reentry, Ordering::Relaxed);
    stats
        .aoi_update_bytes
        .store(owner.aoi_update_bytes, Ordering::Relaxed);
    stats
        .oldest_pending_ticks
        .store(owner.oldest_pending_ticks, Ordering::Relaxed);
    stats
        .max_deferred_ticks
        .store(owner.max_deferred_ticks, Ordering::Relaxed);
    stats
        .replication_queue_depth_max
        .store(owner.replication_queue_depth_max, Ordering::Relaxed);
    stats
        .command_rejects_gate
        .store(owner.command_rejects_gate, Ordering::Relaxed);
    stats
        .command_rejects_other
        .store(owner.command_rejects_other, Ordering::Relaxed);
    stats
        .observer_pending_updates
        .store(owner.observer_pending_updates, Ordering::Relaxed);
    stats
        .observer_pending_enters
        .store(owner.observer_pending_enters, Ordering::Relaxed);
    stats
        .cadence_deferred_updates
        .store(owner.cadence_deferred_updates, Ordering::Relaxed);
    let rt = owner.runtime_stats();
    stats
        .scheduler_queued
        .store(u64::from(rt.scheduler_queued), Ordering::Relaxed);
    stats
        .scheduler_due_critical
        .store(u64::from(rt.scheduler_due_critical), Ordering::Relaxed);
    stats
        .scheduler_due_deferred
        .store(u64::from(rt.scheduler_due_deferred), Ordering::Relaxed);
    stats
        .scheduler_critical_ceiling_hits
        .store(rt.scheduler_critical_ceiling_hits, Ordering::Relaxed);
    stats
        .scheduler_deferred_exhausted
        .store(rt.scheduler_deferred_exhausted, Ordering::Relaxed);
    stats
        .actions_active
        .store(u64::from(rt.actions_active), Ordering::Relaxed);
    stats
        .events_produced
        .store(rt.events_produced, Ordering::Relaxed);
    stats
        .events_processed
        .store(rt.events_processed, Ordering::Relaxed);
    stats
        .spawn_queue_depth
        .store(u64::from(rt.spawn_queue_depth), Ordering::Relaxed);
    stats
        .cadence_due
        .store(u64::from(rt.cadence_due), Ordering::Relaxed);
    stats
        .domain_rev_advances
        .store(rt.domain_rev_advances, Ordering::Relaxed);
    stats
        .scheduler_scheduled_total
        .store(rt.scheduler_scheduled_total, Ordering::Relaxed);
    stats
        .scheduler_cancelled_total
        .store(rt.scheduler_cancelled_total, Ordering::Relaxed);
    stats
        .scheduler_critical_executed_total
        .store(rt.scheduler_critical_executed_total, Ordering::Relaxed);
    stats
        .scheduler_deferred_executed_total
        .store(rt.scheduler_deferred_executed_total, Ordering::Relaxed);
    stats
        .actions_started_total
        .store(rt.actions_started_total, Ordering::Relaxed);
    stats
        .actions_completed_total
        .store(rt.actions_completed_total, Ordering::Relaxed);
    stats
        .effects_applied_total
        .store(rt.effects_applied_total, Ordering::Relaxed);
    stats
        .effects_expired_total
        .store(rt.effects_expired_total, Ordering::Relaxed);
    stats
        .spawn_requests_total
        .store(rt.spawn_requests_total, Ordering::Relaxed);
    stats
        .spawns_completed_total
        .store(rt.spawns_completed_total, Ordering::Relaxed);
    stats
        .despawns_completed_total
        .store(rt.despawns_completed_total, Ordering::Relaxed);
    stats
        .cadence_executions_total
        .store(rt.cadence_executions_total, Ordering::Relaxed);
    stats
        .entities_spawned_total
        .store(rt.entities_spawned_total, Ordering::Relaxed);
    let entities = owner.player_count() as u64;
    stats
        .active_player_entities
        .store(entities, Ordering::Relaxed);
    stats
        .peak_player_entities
        .fetch_max(entities, Ordering::Relaxed);
}

#[cfg(test)]
mod tests;
