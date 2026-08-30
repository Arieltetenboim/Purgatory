//! Development-only localhost UDP metrics export (Phase 5.7).
//!
//! Not part of gameplay protocol v4. Bind is 127.0.0.1 only.
//! Malformed requests never panic; they increment a counter and are ignored.
//! Missed polls are the harness's problem — this side only answers when asked.

use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use purgatory_common::{LoadMetricsV1, encode_metrics_response, is_metrics_request};
use purgatory_protocol::MAX_ENTITIES_PER_SNAPSHOT;
use tokio::net::UdpSocket;

use super::session::{SessionTable, lock_sessions};
use super::stats::ServerNetStats;

pub struct MetricsExportCtx {
    pub stats: Arc<ServerNetStats>,
    pub sessions: Arc<Mutex<SessionTable>>,
}

/// Spawn the metrics responder. Binds `127.0.0.1:port`. Logs once on failure.
pub fn spawn_metrics_export(port: u16, ctx: MetricsExportCtx) {
    tokio::spawn(async move {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let socket = match UdpSocket::bind(addr).await {
            Ok(s) => s,
            Err(err) => {
                println!("PURGATORY metrics export bind failed on {addr}: {err}");
                return;
            }
        };
        println!("PURGATORY metrics export listening on {addr}");
        let mut buf = [0u8; 64];
        loop {
            let (n, peer) = match socket.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(_) => continue,
            };
            if !is_metrics_request(&buf[..n]) {
                ctx.stats
                    .metrics_malformed_requests
                    .fetch_add(1, Ordering::Relaxed);
                continue;
            }
            let snapshot = build_snapshot(&ctx).await;
            match encode_metrics_response(&snapshot) {
                Ok(payload) => {
                    let _ = socket.send_to(&payload, peer).await;
                }
                Err(_) => {
                    ctx.stats
                        .metrics_encode_failed
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    });
}

async fn build_snapshot(ctx: &MetricsExportCtx) -> LoadMetricsV1 {
    // Refresh memory each poll (1 Hz from harness).
    if let Some(mem) = purgatory_common::current_process_memory() {
        ctx.stats
            .memory_working_set_bytes
            .store(mem.working_set_bytes, Ordering::Relaxed);
        ctx.stats
            .memory_working_set_peak_bytes
            .fetch_max(mem.working_set_bytes, Ordering::Relaxed);
    }

    let (active_sessions, peak_sessions) = {
        let table = lock_sessions(&ctx.sessions);
        (table.len() as u64, table.high_water() as u64)
    };

    let s = &ctx.stats;
    let micros_to_ms = |v: u64| -> f64 { v as f64 / 1000.0 };

    LoadMetricsV1 {
        metrics_schema_version: purgatory_common::LOAD_METRICS_SCHEMA_VERSION,
        admission_cap: s.admission_cap.load(Ordering::Relaxed),
        max_entities_per_snapshot: u64::from(MAX_ENTITIES_PER_SNAPSHOT),
        active_sessions,
        active_player_entities: s.active_player_entities.load(Ordering::Relaxed),
        peak_sessions,
        peak_player_entities: s.peak_player_entities.load(Ordering::Relaxed),
        total_accepted: s.total_accepted.load(Ordering::Relaxed),
        total_rejected: s.total_rejected.load(Ordering::Relaxed),
        admission_refused: s.admission_refused.load(Ordering::Relaxed),
        clean_disconnect: s.clean_disconnect.load(Ordering::Relaxed),
        transport_loss: s.transport_loss.load(Ordering::Relaxed),
        session_created: s.session_created.load(Ordering::Relaxed),
        session_destroyed: s.session_destroyed.load(Ordering::Relaxed),
        player_entity_spawned: s.player_entity_spawned.load(Ordering::Relaxed),
        player_entity_despawned: s.player_entity_despawned.load(Ordering::Relaxed),
        duplicate_session_detected: s.duplicate_session_detected.load(Ordering::Relaxed),
        lifecycle_handoff_dropped: s.lifecycle_handoff_dropped.load(Ordering::Relaxed),
        input_received: s.input_received.load(Ordering::Relaxed),
        input_accepted: s.input_accepted.load(Ordering::Relaxed),
        input_duplicate: s.input_duplicate.load(Ordering::Relaxed),
        input_stale: s.input_stale.load(Ordering::Relaxed),
        input_queue_overflow: s.input_queue_overflow.load(Ordering::Relaxed),
        input_handoff_dropped: s.input_handoff_dropped.load(Ordering::Relaxed),
        input_rate_limited: s.input_rate_limited.load(Ordering::Relaxed),
        input_queue_current: s.input_queue_current.load(Ordering::Relaxed),
        input_queue_max: s.input_queue_max.load(Ordering::Relaxed),
        session_queue_max: s.session_queue_max.load(Ordering::Relaxed),
        snapshots_built: s.snapshots_built.load(Ordering::Relaxed),
        snapshot_build_count: s.snapshot_build_count.load(Ordering::Relaxed),
        snapshots_sent: s.snapshots_sent.load(Ordering::Relaxed),
        snapshot_send_failed: s.snapshot_send_failed.load(Ordering::Relaxed),
        snapshot_encode_failed: s.snapshot_encode_failed.load(Ordering::Relaxed),
        snapshot_sequence: s.snapshot_sequence.load(Ordering::Relaxed),
        last_snapshot_entities: s.last_snapshot_entities.load(Ordering::Relaxed),
        bytes_in: s.bytes_in.load(Ordering::Relaxed),
        bytes_out: s.bytes_out.load(Ordering::Relaxed),
        tick_count: s.tick_count.load(Ordering::Relaxed),
        tick_overrun_count: s.tick_overrun_count.load(Ordering::Relaxed),
        tick_work_mean_ms: micros_to_ms(s.tick_work_mean_micros.load(Ordering::Relaxed)),
        tick_work_p50_ms: micros_to_ms(s.tick_work_p50_micros.load(Ordering::Relaxed)),
        tick_work_p95_ms: micros_to_ms(s.tick_work_p95_micros.load(Ordering::Relaxed)),
        tick_work_p99_ms: micros_to_ms(s.tick_work_p99_micros.load(Ordering::Relaxed)),
        tick_work_max_ms: micros_to_ms(s.tick_work_max_micros.load(Ordering::Relaxed)),
        scheduler_lateness_p95_ms: micros_to_ms(
            s.scheduler_lateness_p95_micros.load(Ordering::Relaxed),
        ),
        scheduler_lateness_max_ms: micros_to_ms(
            s.scheduler_lateness_max_micros.load(Ordering::Relaxed),
        ),
        catch_up_ticks: s.catch_up_ticks.load(Ordering::Relaxed),
        discarded_ns: s.discarded_ns.load(Ordering::Relaxed),
        snapshot_build_time_max_ms: micros_to_ms(
            s.snapshot_build_time_max_micros.load(Ordering::Relaxed),
        ),
        snapshot_encode_time_max_ms: micros_to_ms(
            s.snapshot_encode_time_max_micros.load(Ordering::Relaxed),
        ),
        snapshot_size_max_bytes: s.snapshot_size_max_bytes.load(Ordering::Relaxed),
        memory_working_set_bytes: s.memory_working_set_bytes.load(Ordering::Relaxed),
        memory_working_set_peak_bytes: s.memory_working_set_peak_bytes.load(Ordering::Relaxed),
        aoi_enters: s.aoi_enters.load(Ordering::Relaxed),
        aoi_leaves: s.aoi_leaves.load(Ordering::Relaxed),
        aoi_updates: s.aoi_updates.load(Ordering::Relaxed),
        aoi_churn_reentry: s.aoi_churn_reentry.load(Ordering::Relaxed),
        aoi_update_bytes: s.aoi_update_bytes.load(Ordering::Relaxed),
        oldest_pending_ticks: s.oldest_pending_ticks.load(Ordering::Relaxed),
        max_deferred_ticks: s.max_deferred_ticks.load(Ordering::Relaxed),
        replication_queue_depth_max: s.replication_queue_depth_max.load(Ordering::Relaxed),
        scheduler_queued: s.scheduler_queued.load(Ordering::Relaxed),
        scheduler_due_critical: s.scheduler_due_critical.load(Ordering::Relaxed),
        scheduler_due_deferred: s.scheduler_due_deferred.load(Ordering::Relaxed),
        scheduler_critical_ceiling_hits: s.scheduler_critical_ceiling_hits.load(Ordering::Relaxed),
        scheduler_deferred_exhausted: s.scheduler_deferred_exhausted.load(Ordering::Relaxed),
        actions_active: s.actions_active.load(Ordering::Relaxed),
        events_produced: s.events_produced.load(Ordering::Relaxed),
        events_processed: s.events_processed.load(Ordering::Relaxed),
        spawn_queue_depth: s.spawn_queue_depth.load(Ordering::Relaxed),
        cadence_due: s.cadence_due.load(Ordering::Relaxed),
        command_rejects_gate: s.command_rejects_gate.load(Ordering::Relaxed),
        command_rejects_other: s.command_rejects_other.load(Ordering::Relaxed),
        domain_rev_advances: s.domain_rev_advances.load(Ordering::Relaxed),
        observer_pending_updates: s.observer_pending_updates.load(Ordering::Relaxed),
        observer_pending_enters: s.observer_pending_enters.load(Ordering::Relaxed),
        cadence_deferred_updates: s.cadence_deferred_updates.load(Ordering::Relaxed),
    }
}

/// Ring buffer of recent tick samples for percentile export.
pub struct TickSampleRing {
    work_us: Vec<u64>,
    lateness_us: Vec<u64>,
    cap: usize,
}

impl TickSampleRing {
    #[must_use]
    pub fn new(cap: usize) -> Self {
        Self {
            work_us: Vec::with_capacity(cap),
            lateness_us: Vec::with_capacity(cap),
            cap: cap.max(1),
        }
    }

    pub fn push(&mut self, work: Duration, lateness: Duration) {
        let w = u64::try_from(work.as_micros()).unwrap_or(u64::MAX);
        let l = u64::try_from(lateness.as_micros()).unwrap_or(u64::MAX);
        if self.work_us.len() >= self.cap {
            self.work_us.remove(0);
            self.lateness_us.remove(0);
        }
        self.work_us.push(w);
        self.lateness_us.push(l);
    }

    pub fn publish(&self, stats: &ServerNetStats) {
        if self.work_us.is_empty() {
            return;
        }
        let (mean, p50, p95, p99, max) = percentiles(&self.work_us);
        stats.tick_work_mean_micros.store(mean, Ordering::Relaxed);
        stats.tick_work_p50_micros.store(p50, Ordering::Relaxed);
        stats.tick_work_p95_micros.store(p95, Ordering::Relaxed);
        stats.tick_work_p99_micros.store(p99, Ordering::Relaxed);
        stats.tick_work_max_micros.store(max, Ordering::Relaxed);
        let (_, _, lp95, _, lmax) = percentiles(&self.lateness_us);
        stats
            .scheduler_lateness_p95_micros
            .store(lp95, Ordering::Relaxed);
        stats
            .scheduler_lateness_max_micros
            .store(lmax, Ordering::Relaxed);
    }
}

fn percentiles(samples: &[u64]) -> (u64, u64, u64, u64, u64) {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    let mean = sorted.iter().sum::<u64>() / n as u64;
    let idx = |p: f64| -> u64 {
        let i = ((n as f64 - 1.0) * p).round() as usize;
        sorted[i.min(n - 1)]
    };
    (
        mean,
        idx(0.50),
        idx(0.95),
        idx(0.99),
        *sorted.last().unwrap(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_basic() {
        let samples: Vec<u64> = (1..=100).collect();
        let (mean, p50, p95, p99, max) = percentiles(&samples);
        assert!((50..=51).contains(&mean));
        // Nearest-rank: round((n-1)*p) on 0-based indices → p50 index 50 → value 51.
        assert_eq!(p50, 51);
        assert!(p95 >= 95);
        assert!(p99 >= 99);
        assert_eq!(max, 100);
    }
}
