//! Server-issued network connection identity.
//!
//! This is not a simulation entity id, not a player id, and not a pointer.
//! The client cannot choose its authoritative [`ConnectionId`].

use std::fmt;

/// Compact server-assigned id, unique among *active* connections.
///
/// Generated with a process-local monotonic `u64`. Not a UUID. Not reused
/// while the process lives (wraparound is ignored for Phase 5.0).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConnectionId(u64);

impl ConnectionId {
    /// First id issued by a new server process.
    pub const FIRST: Self = Self(1);

    /// Wrap a server-assigned value. Callers must not use this to let a client
    /// pick its own identity.
    ///
    /// `0` is reserved and is not issued by the server allocator. This
    /// constructor stays infallible so untrusted codec bytes can be represented
    /// without panicking; the allocator asserts against wraparound to `0`.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Next id after `self`. Used by the server allocator.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_and_equality() {
        let a = ConnectionId::from_raw(7);
        let b = a;
        assert_eq!(a, b);
        assert_eq!(a.get(), 7);
        assert_eq!(a.to_string(), "7");
    }

    #[test]
    fn first_and_next_are_monotonic() {
        assert_eq!(ConnectionId::FIRST.get(), 1);
        assert_eq!(ConnectionId::FIRST.next().get(), 2);
        assert_ne!(ConnectionId::FIRST, ConnectionId::FIRST.next());
    }

    #[test]
    fn saturating_next_does_not_panic() {
        let max = ConnectionId::from_raw(u64::MAX);
        assert_eq!(max.next(), max);
    }
}
