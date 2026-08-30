//! Replication **policy metadata**. Not a scheduler, delta codec, or budget.

/// Who may receive this entity's replicated state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ReplicationClass {
    /// Never replicated (e.g. static platforms today).
    None,
    /// Only the controlling observer (owner-only fields later).
    OwnerOnly,
    /// Eligible observers that pass address / relevance filters.
    VisibleObservers,
}

impl std::fmt::Display for ReplicationClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("None"),
            Self::OwnerOnly => f.write_str("OwnerOnly"),
            Self::VisibleObservers => f.write_str("VisibleObservers"),
        }
    }
}

/// Intended update cadence. Stored only; 6.0/6A must not schedule from this.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum UpdateFrequencyTier {
    EveryTick,
    Low,
    Event,
}

/// State-like payloads may coalesce. Event-like payloads must not be silently dropped.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ReplicationPayloadKind {
    /// Position, velocity, HP — newer state may supersede older.
    State,
    /// Jump trigger, pickup, interaction — stronger delivery semantics later.
    Event,
}

/// Per-entity replication contract. Frequency/priority are metadata only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplicationMeta {
    pub class: ReplicationClass,
    pub priority: u8,
    pub frequency: UpdateFrequencyTier,
    pub payload_kind: ReplicationPayloadKind,
}

impl ReplicationMeta {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            class: ReplicationClass::None,
            priority: 0,
            frequency: UpdateFrequencyTier::EveryTick,
            payload_kind: ReplicationPayloadKind::State,
        }
    }

    #[must_use]
    pub const fn visible_observers() -> Self {
        Self {
            class: ReplicationClass::VisibleObservers,
            priority: 128,
            frequency: UpdateFrequencyTier::EveryTick,
            payload_kind: ReplicationPayloadKind::State,
        }
    }

    #[must_use]
    pub const fn owner_only() -> Self {
        Self {
            class: ReplicationClass::OwnerOnly,
            priority: 200,
            frequency: UpdateFrequencyTier::EveryTick,
            payload_kind: ReplicationPayloadKind::State,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_distinguishes_none_owner_and_visible() {
        assert_ne!(ReplicationClass::None, ReplicationClass::VisibleObservers);
        assert_ne!(
            ReplicationClass::OwnerOnly,
            ReplicationClass::VisibleObservers
        );
        assert_eq!(ReplicationMeta::none().class, ReplicationClass::None);
        assert_eq!(
            ReplicationMeta::visible_observers().class,
            ReplicationClass::VisibleObservers
        );
        assert_eq!(ReplicationPayloadKind::State, ReplicationPayloadKind::State);
        assert_ne!(ReplicationPayloadKind::State, ReplicationPayloadKind::Event);
    }
}
