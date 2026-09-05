//! Server-visible connection stage histograms (Phase 7.1).
//!
//! Client/harness owns connect-attempt start. Server timing begins at
//! transport accept / stream ready / Hello.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use purgatory_common::{ConnectionLifecycleSnapshot, int_distribution_from_micros};

const RING: usize = 512;

#[derive(Debug)]
pub struct ConnectionLifecycleBook {
    run_start: Instant,
    transport_accept_ok: AtomicU64,
    hello_ok: AtomicU64,
    welcome_ok: AtomicU64,
    session_accepted: AtomicU64,
    fail_transport_accept: AtomicU64,
    fail_hello: AtomicU64,
    fail_welcome: AtomicU64,
    fail_enter: AtomicU64,
    disconnects: AtomicU64,
    accept_to_hello: Mutex<Vec<u64>>,
    hello_to_welcome: Mutex<Vec<u64>>,
    welcome_to_session: Mutex<Vec<u64>>,
    session_to_disconnect: Mutex<Vec<u64>>,
}

impl Default for ConnectionLifecycleBook {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionLifecycleBook {
    #[must_use]
    pub fn new() -> Self {
        Self {
            run_start: Instant::now(),
            transport_accept_ok: AtomicU64::new(0),
            hello_ok: AtomicU64::new(0),
            welcome_ok: AtomicU64::new(0),
            session_accepted: AtomicU64::new(0),
            fail_transport_accept: AtomicU64::new(0),
            fail_hello: AtomicU64::new(0),
            fail_welcome: AtomicU64::new(0),
            fail_enter: AtomicU64::new(0),
            disconnects: AtomicU64::new(0),
            accept_to_hello: Mutex::new(Vec::with_capacity(RING)),
            hello_to_welcome: Mutex::new(Vec::with_capacity(RING)),
            welcome_to_session: Mutex::new(Vec::with_capacity(RING)),
            session_to_disconnect: Mutex::new(Vec::with_capacity(RING)),
        }
    }

    pub fn note_transport_accept_ok(&self) {
        self.transport_accept_ok.fetch_add(1, Ordering::Relaxed);
    }

    pub fn note_transport_accept_fail(&self) {
        self.fail_transport_accept.fetch_add(1, Ordering::Relaxed);
    }

    pub fn note_hello_ok(&self, accept_to_hello_us: u64) {
        self.hello_ok.fetch_add(1, Ordering::Relaxed);
        push_ring(&self.accept_to_hello, accept_to_hello_us);
    }

    pub fn note_hello_fail(&self) {
        self.fail_hello.fetch_add(1, Ordering::Relaxed);
    }

    pub fn note_welcome_ok(&self, hello_to_welcome_us: u64) {
        self.welcome_ok.fetch_add(1, Ordering::Relaxed);
        push_ring(&self.hello_to_welcome, hello_to_welcome_us);
    }

    pub fn note_welcome_fail(&self) {
        self.fail_welcome.fetch_add(1, Ordering::Relaxed);
    }

    pub fn note_enter_fail(&self) {
        self.fail_enter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn note_session_accepted(&self, welcome_to_session_us: u64) {
        self.session_accepted.fetch_add(1, Ordering::Relaxed);
        push_ring(&self.welcome_to_session, welcome_to_session_us);
    }

    pub fn note_disconnect(&self, session_to_disconnect_us: u64) {
        self.disconnects.fetch_add(1, Ordering::Relaxed);
        push_ring(&self.session_to_disconnect, session_to_disconnect_us);
    }

    #[must_use]
    pub fn snapshot(&self) -> ConnectionLifecycleSnapshot {
        ConnectionLifecycleSnapshot {
            schema: ConnectionLifecycleSnapshot::SCHEMA,
            wall_secs: self.run_start.elapsed().as_secs_f64(),
            note: ConnectionLifecycleSnapshot::NOTE.to_string(),
            transport_accept_ok: self.transport_accept_ok.load(Ordering::Relaxed),
            hello_ok: self.hello_ok.load(Ordering::Relaxed),
            welcome_ok: self.welcome_ok.load(Ordering::Relaxed),
            session_accepted: self.session_accepted.load(Ordering::Relaxed),
            fail_transport_accept: self.fail_transport_accept.load(Ordering::Relaxed),
            fail_hello: self.fail_hello.load(Ordering::Relaxed),
            fail_welcome: self.fail_welcome.load(Ordering::Relaxed),
            fail_enter: self.fail_enter.load(Ordering::Relaxed),
            disconnects: self.disconnects.load(Ordering::Relaxed),
            accept_to_hello_us: dist(&self.accept_to_hello),
            hello_to_welcome_us: dist(&self.hello_to_welcome),
            welcome_to_session_us: dist(&self.welcome_to_session),
            session_to_disconnect_us: dist(&self.session_to_disconnect),
        }
    }
}

fn push_ring(buf: &Mutex<Vec<u64>>, v: u64) {
    let Ok(mut g) = buf.lock() else {
        return;
    };
    if g.len() >= RING {
        g.remove(0);
    }
    g.push(v);
}

fn dist(buf: &Mutex<Vec<u64>>) -> purgatory_common::IntDistribution {
    buf.lock()
        .map(|g| int_distribution_from_micros(&g))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histograms_record_server_visible_stages_only() {
        let book = ConnectionLifecycleBook::new();
        book.note_transport_accept_ok();
        book.note_hello_ok(1_000);
        book.note_welcome_ok(2_000);
        book.note_session_accepted(100);
        let snap = book.snapshot();
        assert_eq!(snap.transport_accept_ok, 1);
        assert_eq!(snap.hello_ok, 1);
        assert_eq!(snap.accept_to_hello_us.sample_count, 1);
        assert_eq!(snap.accept_to_hello_us.max, 1_000);
        assert!(snap.note.contains("connect-attempt"));
    }
}
