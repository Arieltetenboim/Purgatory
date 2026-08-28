//! Network session table. Connection identity is not a game entity.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use purgatory_protocol::ConnectionId;

/// Active QUIC session. No player / entity fields.
#[derive(Clone, Debug)]
pub struct ConnectionSession {
    pub connection_id: ConnectionId,
    pub protocol_version: u32,
    pub connected_since: Instant,
    pub remote: SocketAddr,
}

#[derive(Debug, Default)]
pub struct SessionTable {
    sessions: HashMap<ConnectionId, ConnectionSession>,
    /// Highest concurrent session count seen. Never decreases; it proves a
    /// bound was actually exercised. Removed sessions leave no other trace.
    high_water: usize,
}

impl SessionTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            high_water: 0,
        }
    }

    pub fn insert(&mut self, session: ConnectionSession) {
        let id = session.connection_id;
        let previous = self.sessions.insert(id, session);
        assert!(
            previous.is_none(),
            "duplicate ConnectionId inserted: {}",
            id.get()
        );
        self.high_water = self.high_water.max(self.sessions.len());
    }

    /// Idempotent: missing ids are a no-op.
    pub fn remove(&mut self, id: ConnectionId) -> Option<ConnectionSession> {
        self.sessions.remove(&id)
    }

    #[cfg(test)]
    #[must_use]
    pub fn contains(&self, id: ConnectionId) -> bool {
        self.sessions.contains_key(&id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Peak concurrent sessions since bind. Cumulative, not an active gauge.
    #[must_use]
    pub fn high_water(&self) -> usize {
        self.high_water
    }
}

pub(crate) fn lock_sessions(
    sessions: &Mutex<SessionTable>,
) -> std::sync::MutexGuard<'_, SessionTable> {
    sessions
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Removes the session exactly once, including on panic unwind.
pub struct SessionLease {
    sessions: Arc<Mutex<SessionTable>>,
    id: ConnectionId,
    removed: AtomicBool,
}

impl SessionLease {
    pub fn insert(sessions: Arc<Mutex<SessionTable>>, session: ConnectionSession) -> Self {
        let id = session.connection_id;
        lock_sessions(&sessions).insert(session);
        Self {
            sessions,
            id,
            removed: AtomicBool::new(false),
        }
    }

    pub fn remove_once(&self) {
        if self.removed.swap(true, Ordering::AcqRel) {
            return;
        }
        lock_sessions(&self.sessions).remove(self.id);
    }
}

impl Drop for SessionLease {
    fn drop(&mut self) {
        self.remove_once();
    }
}

/// Monotonic connection-id source.
///
/// IDs are not reused during the lifetime of the server process, excluding
/// `u64` wraparound (practically unreachable; not an exhaustion scheme).
/// A disconnected id is therefore never reassigned while another session
/// still holds it, because the counter only increases.
#[derive(Debug)]
pub struct ConnectionIdAllocator {
    next: AtomicU64,
}

impl ConnectionIdAllocator {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next: AtomicU64::new(ConnectionId::FIRST.get()),
        }
    }

    #[must_use]
    pub fn allocate(&self) -> ConnectionId {
        let raw = self.next.fetch_add(1, Ordering::Relaxed);
        // `from_raw` does not reject 0 (codec/tests may construct it). The
        // allocator must not silently issue 0 after wraparound.
        assert_ne!(raw, 0, "ConnectionId allocator wrapped to 0");
        ConnectionId::from_raw(raw)
    }
}

impl Default for ConnectionIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::net::{IpAddr, Ipv4Addr};

    fn dummy(id: u64) -> ConnectionSession {
        ConnectionSession {
            connection_id: ConnectionId::from_raw(id),
            protocol_version: 1,
            connected_since: Instant::now(),
            remote: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1),
        }
    }

    #[test]
    fn remove_is_idempotent() {
        let mut table = SessionTable::new();
        table.insert(dummy(3));
        assert!(table.remove(ConnectionId::from_raw(3)).is_some());
        assert!(table.remove(ConnectionId::from_raw(3)).is_none());
        assert_eq!(table.len(), 0);
    }

    #[test]
    #[should_panic(expected = "duplicate ConnectionId inserted")]
    fn duplicate_insert_is_rejected() {
        let mut table = SessionTable::new();
        table.insert(dummy(3));
        table.insert(dummy(3));
    }

    #[test]
    fn high_water_records_peak_and_survives_removal() {
        let mut table = SessionTable::new();
        table.insert(dummy(1));
        table.insert(dummy(2));
        assert_eq!(table.high_water(), 2);
        table.remove(ConnectionId::from_raw(1));
        table.remove(ConnectionId::from_raw(2));
        assert_eq!(table.len(), 0);
        assert_eq!(table.high_water(), 2, "peak is cumulative, not a gauge");
        table.insert(dummy(3));
        assert_eq!(table.high_water(), 2, "one session cannot raise the peak");
    }

    #[test]
    fn churned_table_keeps_no_live_entries() {
        let mut table = SessionTable::new();
        for id in 1..=200u64 {
            table.insert(dummy(id));
            table.remove(ConnectionId::from_raw(id));
        }
        assert_eq!(table.len(), 0);
        assert_eq!(table.high_water(), 1);
    }

    #[test]
    fn lease_remove_once_is_idempotent() {
        let sessions = Arc::new(Mutex::new(SessionTable::new()));
        let lease = SessionLease::insert(Arc::clone(&sessions), dummy(4));
        assert!(lock_sessions(&sessions).contains(ConnectionId::from_raw(4)));
        lease.remove_once();
        lease.remove_once();
        assert_eq!(lock_sessions(&sessions).len(), 0);
        drop(lease);
        assert_eq!(lock_sessions(&sessions).len(), 0);
    }

    #[test]
    fn dropping_lease_removes_session() {
        let sessions = Arc::new(Mutex::new(SessionTable::new()));
        let id = ConnectionId::from_raw(5);

        {
            let _lease = SessionLease::insert(Arc::clone(&sessions), dummy(5));
            assert!(lock_sessions(&sessions).contains(id));
        }

        assert!(!lock_sessions(&sessions).contains(id));
    }

    #[test]
    #[should_panic(expected = "wrapped to 0")]
    fn allocator_rejects_wrap_to_zero() {
        let alloc = ConnectionIdAllocator {
            next: AtomicU64::new(0),
        };
        let _ = alloc.allocate();
    }

    #[test]
    fn concurrent_allocate_unique() {
        let alloc = Arc::new(ConnectionIdAllocator::new());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let alloc = Arc::clone(&alloc);
            handles.push(std::thread::spawn(move || {
                (0..200).map(|_| alloc.allocate().get()).collect::<Vec<_>>()
            }));
        }
        let mut all = HashSet::new();
        for handle in handles {
            for id in handle.join().expect("thread") {
                assert!(all.insert(id), "duplicate ConnectionId {id}");
            }
        }
        assert_eq!(all.len(), 1600);
    }
}
