//! Network session table. Connection identity is not a game entity.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
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
}

impl SessionTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn insert(&mut self, session: ConnectionSession) {
        self.sessions.insert(session.connection_id, session);
    }

    pub fn remove(&mut self, id: ConnectionId) -> Option<ConnectionSession> {
        self.sessions.remove(&id)
    }

    #[cfg(test)]
    #[must_use]
    pub fn contains(&self, id: ConnectionId) -> bool {
        self.sessions.contains_key(&id)
    }

    #[cfg(test)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }
}

/// Monotonic connection-id source. Unique among active connections.
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
        ConnectionId::from_raw(raw)
    }
}

impl Default for ConnectionIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}
