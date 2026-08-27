//! Fixed-timestep accumulator and tick advancement.
//!
//! Real elapsed time is supplied by the caller. This module never reads
//! `std::time::Instant` or other wall-clock sources.

use std::time::Duration;

use crate::time::{
    MAX_CATCH_UP, MAX_TICKS_PER_ADVANCE, SimulationTick, SimulationTime, TICK_DURATION,
    TICK_RATE_HZ,
};

/// Centralized fixed-step policy used by [`SimulationClock`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockConfig {
    tick_duration: Duration,
    max_catch_up: Duration,
    max_catch_up_ticks: u32,
}

impl ClockConfig {
    /// Default 30 Hz policy with a 1 s elapsed-time catch-up cap.
    pub const DEFAULT: Self = Self {
        tick_duration: TICK_DURATION,
        max_catch_up: MAX_CATCH_UP,
        max_catch_up_ticks: MAX_TICKS_PER_ADVANCE,
    };

    /// Authoritative step duration.
    #[must_use]
    pub const fn tick_duration(self) -> Duration {
        self.tick_duration
    }

    /// Maximum elapsed time accepted from one [`SimulationClock::advance`] call.
    #[must_use]
    pub const fn max_catch_up(self) -> Duration {
        self.max_catch_up
    }

    /// Maximum ticks that one [`SimulationClock::advance`] call may execute.
    #[must_use]
    pub const fn max_catch_up_ticks(self) -> u32 {
        self.max_catch_up_ticks
    }
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Result of supplying elapsed time to [`SimulationClock::advance`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockUpdate {
    /// Simulation ticks executed during this call.
    pub ticks_executed: u32,
    /// Supplied elapsed time beyond [`ClockConfig::max_catch_up`].
    ///
    /// Discarded time is not stored in the accumulator. The simulation does
    /// not later catch up that lost wall time.
    pub discarded: Duration,
}

/// Fixed-timestep simulation clock.
///
/// Callers supply elapsed [`Duration`] values from their own outer loop.
/// Remainder below one tick stays in the accumulator for the next call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulationClock {
    config: ClockConfig,
    tick: SimulationTick,
    accumulator: Duration,
}

impl Default for SimulationClock {
    fn default() -> Self {
        Self::new()
    }
}

impl SimulationClock {
    /// 30 Hz clock with the default catch-up cap.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            config: ClockConfig::DEFAULT,
            tick: SimulationTick::ZERO,
            accumulator: Duration::ZERO,
        }
    }

    /// Active timing policy.
    #[must_use]
    pub const fn config(&self) -> ClockConfig {
        self.config
    }

    /// Completed tick count. Monotonic for this clock instance.
    #[must_use]
    pub const fn tick(&self) -> SimulationTick {
        self.tick
    }

    /// `completed_ticks × tick_duration`. Independent of supplied wall time.
    #[must_use]
    pub fn simulation_time(&self) -> SimulationTime {
        SimulationTime::from_tick_count(self.tick)
    }

    /// Unconsumed elapsed time strictly less than one tick.
    ///
    /// Exposed for later interpolation by presentation code. This is not
    /// authoritative simulation time.
    #[must_use]
    pub const fn remainder(&self) -> Duration {
        self.accumulator
    }

    /// Add supplied elapsed time and execute zero or more fixed ticks.
    ///
    /// Elapsed samples larger than [`MAX_CATCH_UP`] are clamped. The surplus is
    /// returned as [`ClockUpdate::discarded`] and is not retained.
    pub fn advance(&mut self, elapsed: Duration) -> ClockUpdate {
        let (accepted, discarded) = clamp_elapsed(elapsed, self.config.max_catch_up);
        self.accumulator += accepted;

        let mut ticks_executed = 0_u32;
        while self.accumulator >= self.config.tick_duration
            && ticks_executed < self.config.max_catch_up_ticks
        {
            self.accumulator -= self.config.tick_duration;
            self.tick = self.tick.saturating_add(1);
            ticks_executed += 1;
        }

        debug_assert!(
            self.accumulator < self.config.tick_duration,
            "accumulator remainder must stay below one tick"
        );

        ClockUpdate {
            ticks_executed,
            discarded,
        }
    }
}

fn clamp_elapsed(elapsed: Duration, max_catch_up: Duration) -> (Duration, Duration) {
    if elapsed > max_catch_up {
        (max_catch_up, elapsed - max_catch_up)
    } else {
        (elapsed, Duration::ZERO)
    }
}

const _: () = assert!(TICK_RATE_HZ == 30);
const _: () = assert!(MAX_TICKS_PER_ADVANCE == 31);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{MAX_CATCH_UP_TICKS, TICK_DURATION_NANOS};

    const ONE_SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn one_second_at_30hz_produces_30_ticks() {
        let mut clock = SimulationClock::new();
        let update = clock.advance(ONE_SECOND);
        assert_eq!(update.ticks_executed, 30);
        assert_eq!(update.discarded, Duration::ZERO);
        assert_eq!(clock.tick().get(), 30);
        assert_eq!(clock.remainder(), Duration::from_nanos(10));
    }

    #[test]
    fn different_frame_patterns_produce_the_same_progression() {
        let mut pattern_a = SimulationClock::new();
        let mut pattern_b = SimulationClock::new();

        for _ in 0..10 {
            pattern_a.advance(Duration::from_millis(100));
        }
        for _ in 0..100 {
            pattern_b.advance(Duration::from_millis(10));
        }

        assert_eq!(pattern_a.tick(), pattern_b.tick());
        assert_eq!(pattern_a.tick().get(), 30);
        assert_eq!(pattern_a.simulation_time(), pattern_b.simulation_time());
        assert_eq!(pattern_a.remainder(), pattern_b.remainder());
    }

    #[test]
    fn sub_tick_elapsed_is_retained() {
        let mut clock = SimulationClock::new();
        let sub = Duration::from_millis(1);
        let update = clock.advance(sub);
        assert_eq!(update.ticks_executed, 0);
        assert_eq!(clock.tick(), SimulationTick::ZERO);
        assert_eq!(clock.remainder(), sub);

        let update = clock.advance(sub);
        assert_eq!(update.ticks_executed, 0);
        assert_eq!(clock.remainder(), Duration::from_millis(2));
    }

    #[test]
    fn large_elapsed_spike_is_clamped_and_discarded() {
        let mut clock = SimulationClock::new();
        let spike = Duration::from_secs(10);
        let update = clock.advance(spike);

        assert_eq!(update.ticks_executed, MAX_CATCH_UP_TICKS);
        assert_eq!(clock.tick().get(), u64::from(MAX_CATCH_UP_TICKS));
        assert_eq!(update.discarded, spike - MAX_CATCH_UP);
        assert_eq!(clock.remainder(), Duration::from_nanos(10));
        assert_eq!(
            clock.simulation_time().as_nanos(),
            u128::from(TICK_DURATION_NANOS) * u128::from(MAX_CATCH_UP_TICKS)
        );
    }

    #[test]
    fn catch_up_cap_does_not_retain_surplus_in_the_accumulator() {
        let mut clock = SimulationClock::new();
        clock.advance(Duration::from_secs(5));
        let after_spike = clock.remainder();
        let update = clock.advance(Duration::from_millis(1));
        assert_eq!(after_spike, Duration::from_nanos(10));
        assert_eq!(update.ticks_executed, 0);
        assert_eq!(update.discarded, Duration::ZERO);
        assert_eq!(
            clock.remainder(),
            Duration::from_millis(1) + Duration::from_nanos(10)
        );
        assert_eq!(clock.tick().get(), u64::from(MAX_CATCH_UP_TICKS));
    }

    #[test]
    fn tick_numbering_is_monotonic() {
        let mut clock = SimulationClock::new();
        let mut previous = clock.tick().get();
        for _ in 0..20 {
            clock.advance(TICK_DURATION);
            let current = clock.tick().get();
            assert!(current > previous);
            assert_eq!(current, previous + 1);
            previous = current;
        }
    }

    #[test]
    fn simulation_time_is_tick_count_times_step() {
        let mut clock = SimulationClock::new();
        clock.advance(Duration::from_millis(100));
        clock.advance(Duration::from_millis(100));
        let ticks = clock.tick().get();
        assert_eq!(
            clock.simulation_time().as_nanos(),
            u128::from(ticks) * u128::from(TICK_DURATION_NANOS)
        );
        assert_ne!(
            clock.simulation_time().as_duration(),
            Duration::from_millis(200)
        );
    }

    #[test]
    fn zero_elapsed_produces_no_tick() {
        let mut clock = SimulationClock::new();
        let update = clock.advance(Duration::ZERO);
        assert_eq!(update.ticks_executed, 0);
        assert_eq!(update.discarded, Duration::ZERO);
        assert_eq!(clock.tick(), SimulationTick::ZERO);
        assert_eq!(clock.remainder(), Duration::ZERO);
        assert_eq!(clock.simulation_time(), SimulationTime::ZERO);
    }

    #[test]
    fn remainder_plus_one_second_emits_at_most_max_ticks_per_advance() {
        let mut clock = SimulationClock::new();
        clock.advance(Duration::from_nanos(TICK_DURATION_NANOS - 1));
        let update = clock.advance(Duration::from_secs(1));
        assert_eq!(update.ticks_executed, MAX_TICKS_PER_ADVANCE);
        assert_eq!(update.discarded, Duration::ZERO);
        assert!(clock.remainder() < TICK_DURATION);
        assert_eq!(clock.tick().get(), u64::from(MAX_TICKS_PER_ADVANCE));
    }

    #[test]
    fn remainder_plus_enough_time_emits_exactly_one_tick() {
        let mut clock = SimulationClock::new();
        clock.advance(Duration::from_nanos(TICK_DURATION_NANOS - 5));
        assert_eq!(clock.tick().get(), 0);
        let update = clock.advance(Duration::from_nanos(5));
        assert_eq!(update.ticks_executed, 1);
        assert_eq!(clock.tick().get(), 1);
        assert_eq!(clock.remainder(), Duration::ZERO);
    }

    #[test]
    fn crate_manifest_excludes_presentation_network_and_async_deps() {
        let manifest = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
        for forbidden in [
            "winit", "wgpu", "tokio", "quinn", "bevy", "hecs", "legion", "specs", "egui",
        ] {
            let present = manifest.lines().any(|line| {
                let trimmed = line.trim();
                trimmed.starts_with(forbidden) && (trimmed.contains('=') || trimmed.contains('{'))
            });
            assert!(
                !present,
                "{forbidden} must not appear as a purgatory-simulation dependency"
            );
        }
        assert!(manifest.contains("purgatory-common"));
    }
}
