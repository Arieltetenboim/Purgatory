//! Discrete authoritative simulation time.
//!
//! Tick identity is exact. Gameplay systems must use [`SimulationTick`] and
//! [`SimulationTime`], not wall-clock clocks.

use std::time::Duration;

/// Authoritative simulation ticks per second.
///
/// This is an initial engineering value, not a permanent gameplay promise.
pub const TICK_RATE_HZ: u32 = 30;

/// Nanoseconds in one simulation tick.
///
/// `1_000_000_000 / 30` truncates to `33_333_333` ns (~33.333 ms).
/// Thirty ticks therefore equal `999_999_990` ns of simulation time, not
/// exactly one wall-clock second. One second of *supplied* elapsed time still
/// produces exactly thirty ticks, with `10` ns left in the accumulator.
pub const TICK_DURATION_NANOS: u64 = 1_000_000_000 / TICK_RATE_HZ as u64;

/// Fixed simulation step duration.
pub const TICK_DURATION: Duration = Duration::from_nanos(TICK_DURATION_NANOS);

/// Maximum elapsed time accepted from one outer `advance` call.
///
/// One second lets a single supplied 1 s sample produce 30 ticks at 30 Hz.
/// Longer stalls discard the surplus instead of queuing unbounded catch-up.
pub const MAX_CATCH_UP_NANOS: u64 = 1_000_000_000;

/// [`MAX_CATCH_UP_NANOS`] as a [`Duration`].
pub const MAX_CATCH_UP: Duration = Duration::from_secs(1);

/// Ticks produced from [`MAX_CATCH_UP`] starting from an empty accumulator.
///
/// `1_000_000_000 / 33_333_333 = 30`.
pub const MAX_CATCH_UP_TICKS: u32 = (MAX_CATCH_UP_NANOS / TICK_DURATION_NANOS) as u32;

/// Absolute tick bound for one `advance` call.
///
/// A leftover remainder just under one tick plus one second of accepted time
/// can produce 31 ticks. The accumulator remainder must stay below one tick.
pub const MAX_TICKS_PER_ADVANCE: u32 = MAX_CATCH_UP_TICKS + 1;

/// Count of completed authoritative simulation steps.
///
/// `0` is the initial state before any tick has run. The value is monotonic
/// for a given [`crate::SimulationClock`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SimulationTick(u64);

impl SimulationTick {
    /// No completed ticks.
    pub const ZERO: Self = Self(0);

    pub(crate) fn saturating_add(self, ticks: u32) -> Self {
        Self(self.0.saturating_add(u64::from(ticks)))
    }

    /// Completed tick count.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Elapsed authoritative simulation time.
///
/// Equal to `completed_ticks × TICK_DURATION`. This is not wall-clock time.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SimulationTime {
    inner: Duration,
}

impl SimulationTime {
    /// Zero completed simulation time.
    pub const ZERO: Self = Self {
        inner: Duration::ZERO,
    };

    #[must_use]
    pub(crate) fn from_tick_count(ticks: SimulationTick) -> Self {
        let nanos = ticks.get().saturating_mul(TICK_DURATION_NANOS);
        Self {
            inner: Duration::from_nanos(nanos),
        }
    }

    /// Simulation time as a [`Duration`].
    #[must_use]
    pub const fn as_duration(self) -> Duration {
        self.inner
    }

    /// Simulation time in nanoseconds.
    #[must_use]
    pub const fn as_nanos(self) -> u128 {
        self.inner.as_nanos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_duration_matches_truncated_30hz() {
        assert_eq!(TICK_RATE_HZ, 30);
        assert_eq!(TICK_DURATION_NANOS, 33_333_333);
        assert_eq!(TICK_DURATION, Duration::from_nanos(33_333_333));
        assert_eq!(MAX_CATCH_UP_TICKS, 30);
        assert_eq!(MAX_TICKS_PER_ADVANCE, 31);
        assert_eq!(MAX_CATCH_UP_NANOS, 1_000_000_000);
        assert_eq!(MAX_CATCH_UP, Duration::from_secs(1));
    }

    #[test]
    fn thirty_ticks_are_exactly_tick_count_times_step() {
        let time = SimulationTime::from_tick_count(SimulationTick(30));
        assert_eq!(time.as_nanos(), u128::from(TICK_DURATION_NANOS) * 30);
        assert_ne!(time.as_duration(), Duration::from_secs(1));
    }
}
