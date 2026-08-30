//! Deterministic update cadence. 30 Hz is the simulation tick, not a requirement
//! that every system or entity execute every tick.

/// How often work runs relative to the authoritative tick.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cadence {
    EveryTick,
    /// Run once every `n` ticks, staggered by a key.
    EveryN {
        n: u32,
    },
}

/// Stable stagger key (typically entity index or a registered handle).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct CadenceKey(pub u32);

/// True when `tick` is this cadence's slot for `key`.
#[must_use]
pub fn cadence_due(tick: u64, cadence: Cadence, key: CadenceKey) -> bool {
    match cadence {
        Cadence::EveryTick => true,
        Cadence::EveryN { n } => {
            let n = u64::from(n.max(1));
            let phase = u64::from(key.0) % n;
            tick % n == phase
        }
    }
}

/// Staggered replication/send interval: due when `(tick + key) % interval == 0`.
#[must_use]
pub fn staggered_interval_due(tick: u64, interval: u64, key: u32) -> bool {
    let interval = interval.max(1);
    tick.wrapping_add(u64::from(key)) % interval == 0
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CadenceBinding {
    pub key: CadenceKey,
    pub cadence: Cadence,
    pub token: u32,
}

/// Registered lower-frequency work. Not NPC AI.
#[derive(Clone, Debug, Default)]
pub struct CadenceTable {
    items: Vec<CadenceBinding>,
}

impl CadenceTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, cadence: Cadence, token: u32) -> CadenceKey {
        let key = CadenceKey(u32::try_from(self.items.len()).unwrap_or(u32::MAX));
        self.items.push(CadenceBinding {
            key,
            cadence,
            token,
        });
        key
    }

    #[must_use]
    pub fn due_this_tick(&self, tick: u64) -> Vec<CadenceBinding> {
        self.items
            .iter()
            .copied()
            .filter(|item| cadence_due(tick, item.cadence, item.key))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tick_is_always_due() {
        assert!(cadence_due(0, Cadence::EveryTick, CadenceKey(7)));
        assert!(cadence_due(29, Cadence::EveryTick, CadenceKey(7)));
    }

    #[test]
    fn every_n_staggers_by_key() {
        let cadence = Cadence::EveryN { n: 4 };
        let mut due_on_zero = 0u32;
        for key in 0..8 {
            if cadence_due(0, cadence, CadenceKey(key)) {
                due_on_zero += 1;
            }
        }
        assert_eq!(due_on_zero, 2, "keys 0 and 4 only");
        assert!(cadence_due(1, cadence, CadenceKey(1)));
        assert!(!cadence_due(1, cadence, CadenceKey(0)));
    }

    #[test]
    fn staggered_interval_spreads_keys() {
        let mut on_tick_zero = 0u32;
        for key in 0..8 {
            if staggered_interval_due(0, 4, key) {
                on_tick_zero += 1;
            }
        }
        assert_eq!(on_tick_zero, 2);
        assert!(staggered_interval_due(1, 2, 1));
        assert!(!staggered_interval_due(1, 2, 0));
    }
}
