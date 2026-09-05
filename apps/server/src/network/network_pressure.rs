//! Off-tick network / queue / write-drain pressure (Phase 7.1 / 7.4).
//!
//! `write_drain` is write/drain/backpressure latency, not QUIC CPU/send cost.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use purgatory_common::{
    ClientPressureRow, IntDistribution, NetworkPressureSnapshot, int_distribution_from_micros,
};

use super::replication::WRITER_QUEUE_CAP;

const TOP_N: usize = 8;
const RING: usize = 256;

#[derive(Clone, Copy, Debug, Default)]
struct ClientSlot {
    queue_depth: u32,
    write_drain_max_us: u64,
    write_drain_last_us: u64,
    queue_age_max_us: u64,
    bytes_out: u64,
    enqueue_fails: u64,
}

#[derive(Debug)]
pub struct NetworkPressureBook {
    clients: Mutex<HashMap<u64, ClientSlot>>,
    write_drain: Mutex<Vec<u64>>,
    queue_age: Mutex<Vec<u64>>,
    bytes_in: AtomicU64,
    bytes_out: AtomicU64,
    last_bytes_in: AtomicU64,
    last_bytes_out: AtomicU64,
    last_rate_wall: Mutex<Option<Instant>>,
    bytes_in_per_sec: Mutex<f64>,
    bytes_out_per_sec: Mutex<f64>,
    queue_depth_max: AtomicU64,
    enqueue_fail_total: AtomicU64,
    frames_publish_attempt_total: AtomicU64,
    frames_encoded_total: AtomicU64,
    frames_enqueued_total: AtomicU64,
    enqueue_attempts_total: AtomicU64,
    frames_drained_total: AtomicU64,
    bytes_encoded_total: AtomicU64,
    bytes_drained_total: AtomicU64,
    write_calls_total: AtomicU64,
}

impl Default for NetworkPressureBook {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkPressureBook {
    #[must_use]
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            write_drain: Mutex::new(Vec::with_capacity(RING)),
            queue_age: Mutex::new(Vec::with_capacity(RING)),
            bytes_in: AtomicU64::new(0),
            bytes_out: AtomicU64::new(0),
            last_bytes_in: AtomicU64::new(0),
            last_bytes_out: AtomicU64::new(0),
            last_rate_wall: Mutex::new(None),
            bytes_in_per_sec: Mutex::new(0.0),
            bytes_out_per_sec: Mutex::new(0.0),
            queue_depth_max: AtomicU64::new(0),
            enqueue_fail_total: AtomicU64::new(0),
            frames_publish_attempt_total: AtomicU64::new(0),
            frames_encoded_total: AtomicU64::new(0),
            frames_enqueued_total: AtomicU64::new(0),
            enqueue_attempts_total: AtomicU64::new(0),
            frames_drained_total: AtomicU64::new(0),
            bytes_encoded_total: AtomicU64::new(0),
            bytes_drained_total: AtomicU64::new(0),
            write_calls_total: AtomicU64::new(0),
        }
    }

    pub fn note_counters(&self, bytes_in: u64, bytes_out: u64) {
        self.bytes_in.store(bytes_in, Ordering::Relaxed);
        self.bytes_out.store(bytes_out, Ordering::Relaxed);
    }

    pub fn note_queue(&self, connection_id: u64, depth: u32, enqueue_fail: bool) {
        self.queue_depth_max
            .fetch_max(u64::from(depth), Ordering::Relaxed);
        if enqueue_fail {
            self.enqueue_fail_total.fetch_add(1, Ordering::Relaxed);
        }
        let Ok(mut map) = self.clients.lock() else {
            return;
        };
        let slot = map.entry(connection_id).or_default();
        slot.queue_depth = depth;
        if enqueue_fail {
            slot.enqueue_fails = slot.enqueue_fails.saturating_add(1);
        }
    }

    /// Outbound ownership: publish attempt → encode → enqueue (Phase 7.4A).
    pub fn note_outbound_publish(
        &self,
        encoded: bool,
        encoded_bytes: u64,
        enqueue_attempted: bool,
        enqueued: bool,
    ) {
        self.frames_publish_attempt_total
            .fetch_add(1, Ordering::Relaxed);
        if encoded {
            self.frames_encoded_total.fetch_add(1, Ordering::Relaxed);
            self.bytes_encoded_total
                .fetch_add(encoded_bytes, Ordering::Relaxed);
        }
        if enqueue_attempted {
            self.enqueue_attempts_total.fetch_add(1, Ordering::Relaxed);
        }
        if enqueued {
            self.frames_enqueued_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn note_write_drain(
        &self,
        connection_id: u64,
        drain_us: u64,
        bytes: u64,
        queue_age_us: u64,
    ) {
        push_ring(&self.write_drain, drain_us);
        if queue_age_us > 0 {
            push_ring(&self.queue_age, queue_age_us);
        }
        self.frames_drained_total.fetch_add(1, Ordering::Relaxed);
        self.bytes_drained_total.fetch_add(bytes, Ordering::Relaxed);
        self.write_calls_total.fetch_add(1, Ordering::Relaxed);
        let Ok(mut map) = self.clients.lock() else {
            return;
        };
        let slot = map.entry(connection_id).or_default();
        slot.write_drain_last_us = drain_us;
        slot.write_drain_max_us = slot.write_drain_max_us.max(drain_us);
        slot.queue_age_max_us = slot.queue_age_max_us.max(queue_age_us);
        slot.bytes_out = slot.bytes_out.saturating_add(bytes);
    }

    pub fn remove_client(&self, connection_id: u64) {
        if let Ok(mut map) = self.clients.lock() {
            map.remove(&connection_id);
        }
    }

    fn refresh_rates(&self) {
        let now = Instant::now();
        let Ok(mut last) = self.last_rate_wall.lock() else {
            return;
        };
        let bin = self.bytes_in.load(Ordering::Relaxed);
        let bout = self.bytes_out.load(Ordering::Relaxed);
        if let Some(prev) = *last {
            let wall = now.saturating_duration_since(prev).as_secs_f64();
            if wall > 0.0 {
                let din = bin.saturating_sub(self.last_bytes_in.load(Ordering::Relaxed)) as f64;
                let dout = bout.saturating_sub(self.last_bytes_out.load(Ordering::Relaxed)) as f64;
                if let Ok(mut v) = self.bytes_in_per_sec.lock() {
                    *v = din / wall;
                }
                if let Ok(mut v) = self.bytes_out_per_sec.lock() {
                    *v = dout / wall;
                }
            }
        }
        *last = Some(now);
        self.last_bytes_in.store(bin, Ordering::Relaxed);
        self.last_bytes_out.store(bout, Ordering::Relaxed);
    }

    #[must_use]
    pub fn snapshot(&self, wall_secs: f64, connected_sessions: u64) -> NetworkPressureSnapshot {
        self.refresh_rates();
        let in_rate = self.bytes_in_per_sec.lock().map(|g| *g).unwrap_or(0.0);
        let out_rate = self.bytes_out_per_sec.lock().map(|g| *g).unwrap_or(0.0);
        let per_sess = if connected_sessions > 0 {
            out_rate / connected_sessions as f64
        } else {
            0.0
        };
        let drain = lock_dist(&self.write_drain);
        let age = lock_dist(&self.queue_age);
        let (top, queued_current) = self.top_clients_and_queued();
        NetworkPressureSnapshot {
            schema: NetworkPressureSnapshot::SCHEMA,
            wall_secs,
            bytes_in_per_sec: in_rate,
            bytes_out_per_sec: out_rate,
            bytes_out_per_session_per_sec: per_sess,
            connected_sessions,
            writer_queue_cap: WRITER_QUEUE_CAP as u64,
            writer_queue_depth_max: self.queue_depth_max.load(Ordering::Relaxed),
            writer_queue_push_fail_total: self.enqueue_fail_total.load(Ordering::Relaxed),
            write_drain: drain,
            queue_age_us: age,
            top_clients: top,
            frames_publish_attempt_total: self.frames_publish_attempt_total.load(Ordering::Relaxed),
            frames_encoded_total: self.frames_encoded_total.load(Ordering::Relaxed),
            frames_enqueued_total: self.frames_enqueued_total.load(Ordering::Relaxed),
            enqueue_attempts_total: self.enqueue_attempts_total.load(Ordering::Relaxed),
            frames_drained_total: self.frames_drained_total.load(Ordering::Relaxed),
            bytes_encoded_total: self.bytes_encoded_total.load(Ordering::Relaxed),
            bytes_drained_total: self.bytes_drained_total.load(Ordering::Relaxed),
            write_calls_total: self.write_calls_total.load(Ordering::Relaxed),
            queued_frames_current: queued_current,
            note: NetworkPressureSnapshot::NOTE.to_string(),
        }
    }

    fn top_clients_and_queued(&self) -> (Vec<ClientPressureRow>, u64) {
        let Ok(map) = self.clients.lock() else {
            return (Vec::new(), 0);
        };
        let queued_current: u64 = map.values().map(|s| u64::from(s.queue_depth)).sum();
        let mut rows: Vec<ClientPressureRow> = map
            .iter()
            .map(|(id, s)| ClientPressureRow {
                connection_id: *id,
                queue_depth: s.queue_depth,
                write_drain_max_us: s.write_drain_max_us,
                write_drain_last_us: s.write_drain_last_us,
                queue_age_max_us: s.queue_age_max_us,
                bytes_out: s.bytes_out,
                enqueue_fails: s.enqueue_fails,
            })
            .collect();
        rows.sort_by(|a, b| {
            b.queue_depth
                .cmp(&a.queue_depth)
                .then(b.write_drain_max_us.cmp(&a.write_drain_max_us))
                .then(b.enqueue_fails.cmp(&a.enqueue_fails))
        });
        rows.truncate(TOP_N);
        (rows, queued_current)
    }
}

fn push_ring(buf: &Mutex<Vec<u64>>, v: u64) {
    let Ok(mut g) = buf.lock() else {
        return;
    };
    if g.len() >= RING {
        let drop_at = g.len() / 2;
        g.drain(0..drop_at);
    }
    g.push(v);
}

fn lock_dist(buf: &Mutex<Vec<u64>>) -> IntDistribution {
    let Ok(g) = buf.lock() else {
        return IntDistribution::default();
    };
    int_distribution_from_micros(&g)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbound_path_counters_accumulate() {
        let book = NetworkPressureBook::new();
        book.note_outbound_publish(true, 100, true, true);
        book.note_outbound_publish(true, 50, true, false);
        book.note_outbound_publish(false, 0, false, false);
        book.note_write_drain(1, 50, 120, 10);
        let snap = book.snapshot(1.0, 1);
        assert_eq!(snap.schema, 2);
        assert_eq!(snap.frames_publish_attempt_total, 3);
        assert_eq!(snap.frames_encoded_total, 2);
        assert_eq!(snap.frames_enqueued_total, 1);
        assert_eq!(snap.enqueue_attempts_total, 2);
        assert_eq!(snap.bytes_encoded_total, 150);
        assert_eq!(snap.frames_drained_total, 1);
        assert_eq!(snap.bytes_drained_total, 120);
        assert_eq!(snap.write_calls_total, 1);
        assert!(!snap.note.is_empty());
    }

    #[test]
    fn top_clients_prefer_queue_depth() {
        let book = NetworkPressureBook::new();
        book.note_queue(1, 1, false);
        book.note_queue(2, 4, true);
        book.note_write_drain(1, 50, 10, 0);
        book.note_write_drain(2, 10, 10, 0);
        let snap = book.snapshot(1.0, 2);
        assert_eq!(snap.writer_queue_cap, WRITER_QUEUE_CAP as u64);
        assert_eq!(snap.top_clients[0].connection_id, 2);
        assert_eq!(snap.queued_frames_current, 5);
    }
}
