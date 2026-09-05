//! Coarse per-tick domain accounting for Phase 6G.2 capacity characterization.
//!
//! Lives on the sim thread. Writes JSON artifacts when
//! `PURGATORY_CAPACITY_ARTIFACT_DIR` is set. Domain timings are file artifacts;
//! they do not belong on the UDP `PURGSTAT` datagram.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use purgatory_common::{
    CAPACITY_ARTIFACT_DIR_ENV, CAPACITY_TICK_BUDGET_NS, CapacityLiveSnapshot, DomainTimingMs,
    GameplayWorkloadSnapshot, InterestLocalitySnapshot, OwnerShareRow, ProcessResourceSnapshot,
    ReplicationFanoutSnapshot, SaturationEvidence, TICK_DOMAIN_WINDOW_SAMPLES, TickDomainSnapshot,
    TickLeafMicros, TickOwnerId, capacity_detail_enabled, classify_saturation,
    current_process_resources, dominant_owner_from_means, worst_spike_owner_from_max,
};

use super::connection_lifecycle::ConnectionLifecycleBook;
use super::network_pressure::NetworkPressureBook;
use super::replication::WRITER_QUEUE_CAP;
use super::session::SessionTable;
use super::stats::ServerNetStats;

const RING_CAP: usize = TICK_DOMAIN_WINDOW_SAMPLES as usize;
const WRITE_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Default)]
pub struct TickDomainSample {
    pub total: Duration,
    pub commands_input: Duration,
    pub simulation_movement: Duration,
    pub npc_activity: Duration,
    pub gameplay_services: Duration,
    pub scheduler: Duration,
    pub actions: Duration,
    pub effects: Duration,
    pub entity_lifecycle: Duration,
    pub cadence: Duration,
    pub spatial_aoi: Duration,
    pub replication: Duration,
    pub replication_discover: Duration,
    pub replication_policy: Duration,
    pub replication_encode: Duration,
    pub replication_enqueue: Duration,
    pub persistence_enqueue: Duration,
    pub unattributed: Duration,
    pub accounting_error: bool,
}

impl TickDomainSample {
    fn leaf_micros(self) -> TickLeafMicros {
        TickLeafMicros {
            commands_input: us(self.commands_input),
            simulation_movement: us(self.simulation_movement),
            npc_activity: us(self.npc_activity),
            gameplay_services: us(self.gameplay_services),
            scheduler: us(self.scheduler),
            actions: us(self.actions),
            effects: us(self.effects),
            entity_lifecycle: us(self.entity_lifecycle),
            cadence: us(self.cadence),
            spatial_aoi: us(self.spatial_aoi),
            replication: us(self.replication),
            replication_discover: us(self.replication_discover),
            replication_policy: us(self.replication_policy),
            replication_encode: us(self.replication_encode),
            replication_enqueue: us(self.replication_enqueue),
            persistence_enqueue: us(self.persistence_enqueue),
        }
    }

    /// Roll up parents from children when detail is on; compute remainder from leaves only.
    pub fn finalize(&mut self, detail: bool) {
        if detail {
            self.gameplay_services =
                self.scheduler + self.actions + self.effects + self.entity_lifecycle + self.cadence;
            self.replication = self.replication_discover
                + self.replication_policy
                + self.replication_encode
                + self.replication_enqueue;
        } else {
            self.replication += self.replication_discover;
            self.replication_discover = Duration::ZERO;
            self.replication_policy = Duration::ZERO;
            self.replication_encode = Duration::ZERO;
            self.replication_enqueue = Duration::ZERO;
            self.scheduler = Duration::ZERO;
            self.actions = Duration::ZERO;
            self.effects = Duration::ZERO;
            self.entity_lifecycle = Duration::ZERO;
            self.cadence = Duration::ZERO;
        }
        let rem = self.leaf_micros().remainder(us(self.total), detail);
        self.unattributed = Duration::from_micros(rem.unattributed_us);
        self.accounting_error = rem.accounting_error;
    }
}

fn us(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

#[derive(Debug)]
pub struct TickDomainAccounting {
    ring: Vec<TickDomainSample>,
    cap: usize,
    tick_count: u64,
    tick_overrun_count: u64,
    artifact_dir: Option<PathBuf>,
    last_write: Option<Instant>,
    run_start: Instant,
    mem_start_bytes: u64,
    mem_peak_bytes: u64,
    last_cpu_time_secs: f64,
    last_cpu_wall: Instant,
    logical_cpus: u32,
    last_cpu_util_pct: f64,
    last_cpu_delta_secs: f64,
    last_cpu_interval_secs: f64,
    last_interest_locality: InterestLocalitySnapshot,
    last_replication_fanout: ReplicationFanoutSnapshot,
    last_gameplay_workload: GameplayWorkloadSnapshot,
    consecutive_overrun_streak: u64,
    max_overrun: Duration,
    accounting_error_ticks: u64,
    cpu_utilization_peak_pct: f64,
    stats: Option<Arc<ServerNetStats>>,
    pressure: Option<Arc<NetworkPressureBook>>,
    lifecycle: Option<Arc<ConnectionLifecycleBook>>,
    sessions: Option<Arc<Mutex<SessionTable>>>,
}

impl Default for TickDomainAccounting {
    fn default() -> Self {
        Self::new()
    }
}

impl TickDomainAccounting {
    #[must_use]
    pub fn new() -> Self {
        let artifact_dir = std::env::var_os(CAPACITY_ARTIFACT_DIR_ENV).and_then(|v| {
            let p = PathBuf::from(v);
            if p.as_os_str().is_empty() {
                None
            } else {
                let _ = fs::create_dir_all(&p);
                Some(p)
            }
        });
        let res = current_process_resources();
        let mem_start = res.map(|r| r.memory.working_set_bytes).unwrap_or(0);
        let cpu0 = res.map(|r| r.cpu.cpu_time_secs).unwrap_or(0.0);
        let cpus = res
            .map(|r| r.cpu.logical_cpus)
            .unwrap_or_else(purgatory_common::logical_cpu_count)
            .max(1);
        Self {
            ring: Vec::with_capacity(RING_CAP),
            cap: RING_CAP,
            tick_count: 0,
            tick_overrun_count: 0,
            artifact_dir,
            last_write: None,
            run_start: Instant::now(),
            mem_start_bytes: mem_start,
            mem_peak_bytes: mem_start,
            last_cpu_time_secs: cpu0,
            last_cpu_wall: Instant::now(),
            logical_cpus: cpus,
            last_cpu_util_pct: 0.0,
            last_cpu_delta_secs: 0.0,
            last_cpu_interval_secs: 0.0,
            last_interest_locality: InterestLocalitySnapshot {
                schema: InterestLocalitySnapshot::SCHEMA,
                ..InterestLocalitySnapshot::default()
            },
            last_replication_fanout: ReplicationFanoutSnapshot {
                schema: ReplicationFanoutSnapshot::SCHEMA,
                ..ReplicationFanoutSnapshot::default()
            },
            last_gameplay_workload: GameplayWorkloadSnapshot {
                schema: GameplayWorkloadSnapshot::SCHEMA,
                ..GameplayWorkloadSnapshot::default()
            },
            consecutive_overrun_streak: 0,
            max_overrun: Duration::ZERO,
            accounting_error_ticks: 0,
            cpu_utilization_peak_pct: 0.0,
            stats: None,
            pressure: None,
            lifecycle: None,
            sessions: None,
        }
    }

    pub fn attach_live_sources(
        &mut self,
        stats: Arc<ServerNetStats>,
        pressure: Arc<NetworkPressureBook>,
        lifecycle: Arc<ConnectionLifecycleBook>,
        sessions: Arc<Mutex<SessionTable>>,
    ) {
        self.stats = Some(stats);
        self.pressure = Some(pressure);
        self.lifecycle = Some(lifecycle);
        self.sessions = Some(sessions);
    }

    pub fn note_interest_locality(&mut self, snap: InterestLocalitySnapshot) {
        self.last_interest_locality = snap;
    }

    pub fn note_replication_fanout(&mut self, snap: ReplicationFanoutSnapshot) {
        self.last_replication_fanout = snap;
    }

    pub fn note_gameplay_workload(&mut self, snap: GameplayWorkloadSnapshot) {
        self.last_gameplay_workload = snap;
    }

    #[must_use]
    pub fn artifact_dir(&self) -> Option<&Path> {
        self.artifact_dir.as_deref()
    }

    pub fn note_overrun(&mut self) {
        self.tick_overrun_count = self.tick_overrun_count.saturating_add(1);
    }

    pub fn push(&mut self, sample: TickDomainSample) {
        self.tick_count = self.tick_count.saturating_add(1);
        let budget = Duration::from_nanos(CAPACITY_TICK_BUDGET_NS);
        if sample.total > budget {
            self.consecutive_overrun_streak = self.consecutive_overrun_streak.saturating_add(1);
            self.max_overrun = self.max_overrun.max(sample.total.saturating_sub(budget));
        } else {
            self.consecutive_overrun_streak = 0;
        }
        if sample.accounting_error {
            self.accounting_error_ticks = self.accounting_error_ticks.saturating_add(1);
        }
        if self.ring.len() >= self.cap {
            self.ring.remove(0);
        }
        self.ring.push(sample);
        self.maybe_write(false);
    }

    pub fn force_write(&mut self) {
        self.maybe_write(true);
    }

    fn maybe_write(&mut self, force: bool) {
        let Some(dir) = self.artifact_dir.clone() else {
            return;
        };
        let now = Instant::now();
        if !force
            && self
                .last_write
                .is_some_and(|t| now.saturating_duration_since(t) < WRITE_INTERVAL)
        {
            return;
        }
        self.last_write = Some(now);
        self.refresh_resources();
        let domains = self.snapshot_domains();
        let resources = self.snapshot_resources();
        let connected = self.connected_sessions();
        if let (Some(stats), Some(pressure)) = (&self.stats, &self.pressure) {
            pressure.note_counters(
                stats.bytes_in.load(Ordering::Relaxed),
                stats.bytes_out.load(Ordering::Relaxed),
            );
        }
        let net = self
            .pressure
            .as_ref()
            .map(|p| p.snapshot(domains.wall_secs, connected));
        let life = self.lifecycle.as_ref().map(|b| b.snapshot());
        let live = self.snapshot_live(&domains, &resources, net.as_ref(), connected);
        let _ = write_json(&dir.join("tick_domains.json"), &domains);
        let _ = write_json(&dir.join("process_resources.json"), &resources);
        let _ = write_json(&dir.join("aoi_locality.json"), &self.last_interest_locality);
        let _ = write_json(
            &dir.join("replication_fanout.json"),
            &self.last_replication_fanout,
        );
        if let Some(net) = &net {
            let _ = write_json(&dir.join("network_pressure.json"), net);
            let _ = append_ndjson(&dir.join("network_pressure.ndjson"), net);
        }
        if let Some(life) = &life {
            let _ = write_json(&dir.join("connection_lifecycle.json"), life);
            let _ = append_ndjson(&dir.join("connection_lifecycle.ndjson"), life);
        }
        let _ = write_json(&dir.join("capacity_live.json"), &live);
        let mut workload = self.last_gameplay_workload.clone();
        workload.schema = GameplayWorkloadSnapshot::SCHEMA;
        workload.wall_secs = domains.wall_secs;
        let _ = write_json(&dir.join("gameplay_workload.json"), &workload);
        let _ = append_ndjson(&dir.join("tick_domains.ndjson"), &domains);
        let _ = append_ndjson(&dir.join("gameplay_workload.ndjson"), &workload);
        let _ = append_ndjson(&dir.join("process_resources.ndjson"), &resources);
        let _ = append_ndjson(
            &dir.join("aoi_locality.ndjson"),
            &self.last_interest_locality,
        );
        let _ = append_ndjson(
            &dir.join("replication_fanout.ndjson"),
            &self.last_replication_fanout,
        );
    }

    fn refresh_resources(&mut self) {
        let Some(res) = current_process_resources() else {
            return;
        };
        self.mem_peak_bytes = self.mem_peak_bytes.max(res.memory.working_set_bytes);
        self.mem_peak_bytes = self.mem_peak_bytes.max(res.memory.peak_working_set_bytes);
        self.logical_cpus = res.cpu.logical_cpus.max(1);
        let now = Instant::now();
        let wall = now
            .saturating_duration_since(self.last_cpu_wall)
            .as_secs_f64();
        let delta = (res.cpu.cpu_time_secs - self.last_cpu_time_secs).max(0.0);
        if wall > 0.0 {
            // 100% = one logical CPU fully busy.
            self.last_cpu_util_pct = (delta / wall) * 100.0;
            self.last_cpu_delta_secs = delta;
            self.last_cpu_interval_secs = wall;
            self.cpu_utilization_peak_pct =
                self.cpu_utilization_peak_pct.max(self.last_cpu_util_pct);
        }
        self.last_cpu_time_secs = res.cpu.cpu_time_secs;
        self.last_cpu_wall = now;
    }

    #[must_use]
    pub fn snapshot_domains(&self) -> TickDomainSnapshot {
        let detail = capacity_detail_enabled();
        let tick_total = percentile_field(&self.ring, |s| s.total);
        let budget_ms = CAPACITY_TICK_BUDGET_NS as f64 / 1_000_000.0;
        let wall = self.run_start.elapsed().as_secs_f64();
        let achieved = if wall > 0.0 {
            self.tick_count as f64 / wall
        } else {
            0.0
        };
        let utilization = if budget_ms > 0.0 {
            (tick_total.mean_ms / budget_ms) * 100.0
        } else {
            0.0
        };
        let remaining = (budget_ms - tick_total.mean_ms).max(0.0);
        let (dominant, spike) = self.owner_labels(detail, &tick_total);
        TickDomainSnapshot {
            schema: TickDomainSnapshot::SCHEMA,
            wall_secs: wall,
            tick_count: self.tick_count,
            tick_overrun_count: self.tick_overrun_count,
            window_samples: TICK_DOMAIN_WINDOW_SAMPLES,
            detail_enabled: detail,
            tick_budget_ms: budget_ms,
            tick_utilization_pct: utilization,
            tick_remaining_mean_ms: remaining,
            consecutive_overrun_streak: self.consecutive_overrun_streak,
            max_overrun_ms: self.max_overrun.as_secs_f64() * 1000.0,
            achieved_tick_hz: achieved,
            accounting_error_ticks: self.accounting_error_ticks,
            dominant_owner: dominant,
            worst_spike_owner: spike,
            tick_total,
            commands_input: percentile_field(&self.ring, |s| s.commands_input),
            simulation_movement: percentile_field(&self.ring, |s| s.simulation_movement),
            npc_activity: percentile_field(&self.ring, |s| s.npc_activity),
            gameplay_services: percentile_field(&self.ring, |s| s.gameplay_services),
            spatial_aoi: percentile_field(&self.ring, |s| s.spatial_aoi),
            replication: percentile_field(&self.ring, |s| s.replication),
            persistence_enqueue: percentile_field(&self.ring, |s| s.persistence_enqueue),
            scheduler: percentile_field(&self.ring, |s| s.scheduler),
            actions: percentile_field(&self.ring, |s| s.actions),
            effects: percentile_field(&self.ring, |s| s.effects),
            entity_lifecycle: percentile_field(&self.ring, |s| s.entity_lifecycle),
            cadence: percentile_field(&self.ring, |s| s.cadence),
            replication_discover: percentile_field(&self.ring, |s| s.replication_discover),
            replication_policy: percentile_field(&self.ring, |s| s.replication_policy),
            replication_encode: percentile_field(&self.ring, |s| s.replication_encode),
            replication_enqueue: percentile_field(&self.ring, |s| s.replication_enqueue),
            unattributed: percentile_field(&self.ring, |s| s.unattributed),
        }
    }

    fn owner_labels(
        &self,
        detail: bool,
        tick_total: &DomainTimingMs,
    ) -> (Option<TickOwnerId>, Option<TickOwnerId>) {
        if self.ring.is_empty() || tick_total.mean_ms <= 0.0 {
            return (None, None);
        }
        let means = self.mean_shares(detail);
        let spikes = self.max_shares(detail);
        (
            dominant_owner_from_means(&means),
            worst_spike_owner_from_max(&spikes),
        )
    }

    fn mean_shares(&self, detail: bool) -> Vec<(TickOwnerId, f64)> {
        let n = self.ring.len() as f64;
        if n <= 0.0 {
            return Vec::new();
        }
        let mut acc = TickLeafMicros::default();
        let mut unattr = 0u64;
        for s in &self.ring {
            let leaf = s.leaf_micros();
            acc.commands_input = acc.commands_input.saturating_add(leaf.commands_input);
            acc.simulation_movement = acc
                .simulation_movement
                .saturating_add(leaf.simulation_movement);
            acc.npc_activity = acc.npc_activity.saturating_add(leaf.npc_activity);
            acc.gameplay_services = acc.gameplay_services.saturating_add(leaf.gameplay_services);
            acc.scheduler = acc.scheduler.saturating_add(leaf.scheduler);
            acc.actions = acc.actions.saturating_add(leaf.actions);
            acc.effects = acc.effects.saturating_add(leaf.effects);
            acc.entity_lifecycle = acc.entity_lifecycle.saturating_add(leaf.entity_lifecycle);
            acc.cadence = acc.cadence.saturating_add(leaf.cadence);
            acc.spatial_aoi = acc.spatial_aoi.saturating_add(leaf.spatial_aoi);
            acc.replication = acc.replication.saturating_add(leaf.replication);
            acc.replication_discover = acc
                .replication_discover
                .saturating_add(leaf.replication_discover);
            acc.replication_policy = acc
                .replication_policy
                .saturating_add(leaf.replication_policy);
            acc.replication_encode = acc
                .replication_encode
                .saturating_add(leaf.replication_encode);
            acc.replication_enqueue = acc
                .replication_enqueue
                .saturating_add(leaf.replication_enqueue);
            acc.persistence_enqueue = acc
                .persistence_enqueue
                .saturating_add(leaf.persistence_enqueue);
            unattr = unattr.saturating_add(us(s.unattributed));
        }
        acc.leaf_pairs(detail, unattr)
            .into_iter()
            .map(|(id, sum)| (id, sum as f64 / n))
            .collect()
    }

    fn max_shares(&self, detail: bool) -> Vec<(TickOwnerId, f64)> {
        let mut max_us: Vec<(TickOwnerId, u64)> = Vec::new();
        for s in &self.ring {
            for (id, v) in s.leaf_micros().leaf_pairs(detail, us(s.unattributed)) {
                if let Some(slot) = max_us.iter_mut().find(|(oid, _)| *oid == id) {
                    slot.1 = slot.1.max(v);
                } else {
                    max_us.push((id, v));
                }
            }
        }
        max_us.into_iter().map(|(id, v)| (id, v as f64)).collect()
    }

    #[must_use]
    pub fn snapshot_resources(&self) -> ProcessResourceSnapshot {
        let ws = current_process_resources()
            .map(|r| r.memory.working_set_bytes)
            .unwrap_or(0);
        let cpus = self.logical_cpus.max(1);
        let raw = self.last_cpu_util_pct;
        ProcessResourceSnapshot {
            schema: ProcessResourceSnapshot::SCHEMA,
            wall_secs: self.run_start.elapsed().as_secs_f64(),
            logical_cpus: cpus,
            working_set_bytes: ws,
            working_set_peak_bytes: self.mem_peak_bytes.max(ws),
            working_set_start_bytes: self.mem_start_bytes,
            cpu_time_secs: self.last_cpu_time_secs,
            cpu_utilization_pct: raw,
            cpu_normalized_per_logical_pct: raw / f64::from(cpus),
            cpu_utilization_peak_pct: self.cpu_utilization_peak_pct.max(raw),
            cpu_time_delta_secs: self.last_cpu_delta_secs,
            sample_interval_secs: self.last_cpu_interval_secs,
            working_set_delta_bytes: ws as i64 - self.mem_start_bytes as i64,
            thread_count: current_process_resources().and_then(|r| r.thread_count),
            handle_count: current_process_resources().and_then(|r| r.handle_count),
        }
    }

    fn connected_sessions(&self) -> u64 {
        self.sessions
            .as_ref()
            .map(|s| {
                s.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .len() as u64
            })
            .unwrap_or(0)
    }

    fn snapshot_live(
        &self,
        domains: &TickDomainSnapshot,
        resources: &ProcessResourceSnapshot,
        net: Option<&purgatory_common::NetworkPressureSnapshot>,
        connected: u64,
    ) -> CapacityLiveSnapshot {
        let write_drain_p99_ms = net.map(|n| n.write_drain.p99 / 1000.0).unwrap_or(0.0);
        let queue_max = net
            .map(|n| n.writer_queue_depth_max)
            .or_else(|| {
                self.stats
                    .as_ref()
                    .map(|s| s.replication_queue_depth_max.load(Ordering::Relaxed))
            })
            .unwrap_or(0);
        let enqueue_fail = net.map(|n| n.writer_queue_push_fail_total).unwrap_or(0);
        let admission_refused = self
            .stats
            .as_ref()
            .map(|s| s.admission_refused.load(Ordering::Relaxed))
            .unwrap_or(0);
        let admission_cap = self
            .stats
            .as_ref()
            .map(|s| s.admission_cap.load(Ordering::Relaxed))
            .unwrap_or(0);
        let entities = self
            .stats
            .as_ref()
            .map(|s| s.active_player_entities.load(Ordering::Relaxed))
            .unwrap_or(0);
        let evidence = SaturationEvidence {
            consecutive_overrun_streak: domains.consecutive_overrun_streak,
            tick_utilization_pct: domains.tick_utilization_pct,
            dominant_owner: domains.dominant_owner,
            writer_queue_push_fail_total: enqueue_fail,
            writer_queue_depth_max: queue_max,
            writer_queue_cap: WRITER_QUEUE_CAP as u64,
            write_drain_p99_ms,
            tick_budget_ms: domains.tick_budget_ms,
            admission_refused_total: admission_refused,
            connected_sessions: connected,
            admission_cap,
            harness_timeouts: 0,
            harness_snapshot_starvation_samples: 0,
        };
        let owners = self.owner_rows(domains);
        CapacityLiveSnapshot {
            schema: CapacityLiveSnapshot::SCHEMA,
            available: true,
            wall_secs: domains.wall_secs,
            classification_is_heuristic: true,
            saturation_class: classify_saturation(&evidence),
            dominant_owner: domains.dominant_owner,
            worst_spike_owner: domains.worst_spike_owner,
            tick_p95_ms: domains.tick_total.p95_ms,
            tick_p99_ms: domains.tick_total.p99_ms,
            tick_max_ms: domains.tick_total.max_ms,
            tick_utilization_pct: domains.tick_utilization_pct,
            tick_overrun_count: domains.tick_overrun_count,
            consecutive_overrun_streak: domains.consecutive_overrun_streak,
            cpu_utilization_pct: resources.cpu_utilization_pct,
            cpu_normalized_per_logical_pct: resources.cpu_normalized_per_logical_pct,
            working_set_bytes: resources.working_set_bytes,
            bytes_in_per_sec: net.map(|n| n.bytes_in_per_sec).unwrap_or(0.0),
            bytes_out_per_sec: net.map(|n| n.bytes_out_per_sec).unwrap_or(0.0),
            connected_sessions: connected,
            entities,
            writer_queue_depth_max: queue_max,
            writer_queue_push_fail_total: enqueue_fail,
            write_drain_p99_ms,
            unattributed_mean_ms: domains.unattributed.mean_ms,
            accounting_error_ticks: domains.accounting_error_ticks,
            owners,
        }
    }

    fn owner_rows(&self, domains: &TickDomainSnapshot) -> Vec<OwnerShareRow> {
        let detail = domains.detail_enabled;
        let total = domains.tick_total.mean_ms.max(1e-9);
        let p99 = |t: DomainTimingMs| t.p99_ms;
        let mut rows = vec![
            (TickOwnerId::CommandsInput, domains.commands_input),
            (TickOwnerId::SimulationMovement, domains.simulation_movement),
            (TickOwnerId::NpcActivity, domains.npc_activity),
            (TickOwnerId::SpatialAoi, domains.spatial_aoi),
            (TickOwnerId::PersistenceEnqueue, domains.persistence_enqueue),
            (TickOwnerId::Unattributed, domains.unattributed),
        ];
        if detail {
            rows.extend([
                (TickOwnerId::Scheduler, domains.scheduler),
                (TickOwnerId::Actions, domains.actions),
                (TickOwnerId::Effects, domains.effects),
                (TickOwnerId::EntityLifecycle, domains.entity_lifecycle),
                (TickOwnerId::Cadence, domains.cadence),
                (
                    TickOwnerId::ReplicationDiscover,
                    domains.replication_discover,
                ),
                (TickOwnerId::ReplicationPolicy, domains.replication_policy),
                (TickOwnerId::ReplicationEncode, domains.replication_encode),
                (TickOwnerId::ReplicationEnqueue, domains.replication_enqueue),
            ]);
        } else {
            rows.extend([
                (TickOwnerId::GameplayServices, domains.gameplay_services),
                (TickOwnerId::Replication, domains.replication),
            ]);
        }
        rows.into_iter()
            .map(|(owner, timing)| OwnerShareRow {
                owner,
                mean_ms: timing.mean_ms,
                p99_ms: p99(timing),
                share_pct: (timing.mean_ms / total) * 100.0,
            })
            .collect()
    }
}

fn percentile_field(
    ring: &[TickDomainSample],
    pick: impl Fn(&TickDomainSample) -> Duration,
) -> DomainTimingMs {
    if ring.is_empty() {
        return DomainTimingMs::default();
    }
    let mut us: Vec<u64> = ring
        .iter()
        .map(|s| u64::try_from(pick(s).as_micros()).unwrap_or(u64::MAX))
        .collect();
    us.sort_unstable();
    let n = us.len();
    let mean = us.iter().sum::<u64>() / n as u64;
    let idx = |p: f64| -> u64 {
        let i = ((n as f64 - 1.0) * p).round() as usize;
        us[i.min(n - 1)]
    };
    DomainTimingMs {
        mean_ms: micros_to_ms(mean),
        p50_ms: micros_to_ms(idx(0.50)),
        p95_ms: micros_to_ms(idx(0.95)),
        p99_ms: micros_to_ms(idx(0.99)),
        max_ms: micros_to_ms(*us.last().unwrap()),
        sample_count: n as u64,
    }
}

fn micros_to_ms(v: u64) -> f64 {
    v as f64 / 1000.0
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, body).map_err(|e| e.to_string())
}

fn append_ndjson<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let mut line = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    line.push(b'\n');
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    f.write_all(&line).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ring_snapshot_is_zero() {
        let acc = TickDomainAccounting::new();
        let snap = acc.snapshot_domains();
        assert_eq!(snap.tick_total.sample_count, 0);
        assert_eq!(snap.schema, TickDomainSnapshot::SCHEMA);
        assert_eq!(snap.window_samples, TICK_DOMAIN_WINDOW_SAMPLES);
    }

    #[test]
    fn finalize_detail_does_not_double_count_parents() {
        let mut s = TickDomainSample {
            total: Duration::from_micros(1000),
            commands_input: Duration::from_micros(100),
            simulation_movement: Duration::from_micros(200),
            scheduler: Duration::from_micros(50),
            actions: Duration::from_micros(20),
            spatial_aoi: Duration::from_micros(300),
            replication_discover: Duration::from_micros(40),
            replication_policy: Duration::from_micros(30),
            replication_encode: Duration::from_micros(20),
            replication_enqueue: Duration::from_micros(10),
            persistence_enqueue: Duration::from_micros(10),
            ..TickDomainSample::default()
        };
        s.finalize(true);
        assert!(!s.accounting_error);
        assert_eq!(us(s.gameplay_services), 70);
        assert_eq!(us(s.replication), 100);
        assert_eq!(us(s.unattributed), 220);
    }

    #[test]
    fn finalize_overlap_sets_accounting_error() {
        let mut s = TickDomainSample {
            total: Duration::from_micros(10),
            commands_input: Duration::from_micros(80),
            simulation_movement: Duration::from_micros(80),
            ..TickDomainSample::default()
        };
        s.finalize(false);
        assert!(s.accounting_error);
        assert_eq!(us(s.unattributed), 0);
    }

    #[test]
    fn push_updates_percentiles() {
        let mut acc = TickDomainAccounting::new();
        for i in 1..=10u64 {
            acc.push(TickDomainSample {
                total: Duration::from_micros(i * 1000),
                commands_input: Duration::from_micros(i * 100),
                simulation_movement: Duration::from_micros(i * 200),
                gameplay_services: Duration::from_micros(i * 50),
                spatial_aoi: Duration::from_micros(i * 300),
                replication: Duration::from_micros(i * 250),
                persistence_enqueue: Duration::from_micros(10),
                ..TickDomainSample::default()
            });
        }
        let snap = acc.snapshot_domains();
        assert_eq!(snap.tick_count, 10);
        assert_eq!(snap.tick_total.sample_count, 10);
        assert!(snap.tick_total.p99_ms >= snap.tick_total.p50_ms);
        assert!(snap.spatial_aoi.max_ms >= snap.commands_input.max_ms);
    }
}
