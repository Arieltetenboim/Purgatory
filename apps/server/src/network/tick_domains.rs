//! Coarse per-tick domain accounting for Phase 6G.2 capacity characterization.
//!
//! Lives on the sim thread. Writes JSON artifacts when
//! `PURGATORY_CAPACITY_ARTIFACT_DIR` is set. Domain timings are file artifacts;
//! they do not belong on the UDP `PURGSTAT` datagram.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use purgatory_common::{
    CAPACITY_ARTIFACT_DIR_ENV, DomainTimingMs, InterestLocalitySnapshot, ProcessResourceSnapshot,
    ReplicationFanoutSnapshot, TickDomainSnapshot, current_process_resources,
};

const RING_CAP: usize = 120;
const WRITE_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Default)]
pub struct TickDomainSample {
    pub total: Duration,
    pub commands_input: Duration,
    pub simulation_movement: Duration,
    pub gameplay_services: Duration,
    pub spatial_aoi: Duration,
    pub replication: Duration,
    pub persistence_enqueue: Duration,
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
        }
    }

    pub fn note_interest_locality(&mut self, snap: InterestLocalitySnapshot) {
        self.last_interest_locality = snap;
    }

    pub fn note_replication_fanout(&mut self, snap: ReplicationFanoutSnapshot) {
        self.last_replication_fanout = snap;
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
        let _ = write_json(&dir.join("tick_domains.json"), &domains);
        let _ = write_json(&dir.join("process_resources.json"), &resources);
        let _ = write_json(&dir.join("aoi_locality.json"), &self.last_interest_locality);
        let _ = write_json(
            &dir.join("replication_fanout.json"),
            &self.last_replication_fanout,
        );
        let _ = append_ndjson(&dir.join("tick_domains.ndjson"), &domains);
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
        }
        self.last_cpu_time_secs = res.cpu.cpu_time_secs;
        self.last_cpu_wall = now;
    }

    #[must_use]
    pub fn snapshot_domains(&self) -> TickDomainSnapshot {
        TickDomainSnapshot {
            schema: TickDomainSnapshot::SCHEMA,
            wall_secs: self.run_start.elapsed().as_secs_f64(),
            tick_count: self.tick_count,
            tick_overrun_count: self.tick_overrun_count,
            tick_total: percentile_field(&self.ring, |s| s.total),
            commands_input: percentile_field(&self.ring, |s| s.commands_input),
            simulation_movement: percentile_field(&self.ring, |s| s.simulation_movement),
            gameplay_services: percentile_field(&self.ring, |s| s.gameplay_services),
            spatial_aoi: percentile_field(&self.ring, |s| s.spatial_aoi),
            replication: percentile_field(&self.ring, |s| s.replication),
            persistence_enqueue: percentile_field(&self.ring, |s| s.persistence_enqueue),
        }
    }

    #[must_use]
    pub fn snapshot_resources(&self) -> ProcessResourceSnapshot {
        let ws = current_process_resources()
            .map(|r| r.memory.working_set_bytes)
            .unwrap_or(0);
        ProcessResourceSnapshot {
            schema: ProcessResourceSnapshot::SCHEMA,
            wall_secs: self.run_start.elapsed().as_secs_f64(),
            logical_cpus: self.logical_cpus,
            working_set_bytes: ws,
            working_set_peak_bytes: self.mem_peak_bytes.max(ws),
            working_set_start_bytes: self.mem_start_bytes,
            cpu_time_secs: self.last_cpu_time_secs,
            cpu_utilization_pct: self.last_cpu_util_pct,
            cpu_time_delta_secs: self.last_cpu_delta_secs,
            sample_interval_secs: self.last_cpu_interval_secs,
        }
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
            });
        }
        let snap = acc.snapshot_domains();
        assert_eq!(snap.tick_count, 10);
        assert_eq!(snap.tick_total.sample_count, 10);
        assert!(snap.tick_total.p99_ms >= snap.tick_total.p50_ms);
        assert!(snap.spatial_aoi.max_ms >= snap.commands_input.max_ms);
    }
}
