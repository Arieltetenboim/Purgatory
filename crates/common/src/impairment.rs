//! Development-only deterministic network impairment scheduler.
//!
//! This is not a transport. It delays application delivery of already-formed
//! messages while preserving reliable-ordered FIFO semantics. Disabled by
//! default. Never call this from simulation or render threads with `sleep`.

use std::collections::VecDeque;
use std::time::Duration;

/// Input writes drained per live-loop turn (fairness bound, not gameplay policy).
pub const INPUT_DRAIN_PER_TURN: usize = 8;
/// Snapshot applies drained per live-loop turn.
pub const SNAPSHOT_DRAIN_PER_TURN: usize = 4;
/// Maximum delayed InputCommand / HeldCancel messages.
pub const INPUT_LANE_CAP: usize = 256;
/// Maximum delayed snapshots.
pub const SNAPSHOT_LANE_CAP: usize = 64;

const STREAM_INPUT: u64 = 1;
const STREAM_SNAPSHOT: u64 = 2;
const STREAM_AUTO_STALL: u64 = 3;

const MS: u64 = 1_000_000;

/// Named development presets. Values are test presets, not gameplay policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImpairmentProfile {
    #[default]
    Off,
    Clean,
    Lan,
    Moderate,
    Bad,
    StallTest,
}

impl ImpairmentProfile {
    pub const ALL: [Self; 6] = [
        Self::Off,
        Self::Clean,
        Self::Lan,
        Self::Moderate,
        Self::Bad,
        Self::StallTest,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Clean => "clean",
            Self::Lan => "lan",
            Self::Moderate => "moderate",
            Self::Bad => "bad",
            Self::StallTest => "stalltest",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "off" | "0" | "false" | "none" => Some(Self::Off),
            "clean" => Some(Self::Clean),
            "lan" => Some(Self::Lan),
            "moderate" | "mod" => Some(Self::Moderate),
            "bad" => Some(Self::Bad),
            "stalltest" | "stall" => Some(Self::StallTest),
            _ => None,
        }
    }
}

/// One-way artificial delay configuration. Not measured ping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkImpairmentConfig {
    pub enabled: bool,
    pub profile: ImpairmentProfile,
    pub seed: u64,
    pub input_base_delay_ns: u64,
    pub input_jitter_ns: u64,
    pub snapshot_base_delay_ns: u64,
    pub snapshot_jitter_ns: u64,
    pub snapshot_keep_every_n: Option<u32>,
    pub auto_stall_interval_ns: Option<u64>,
    pub auto_stall_duration_ns: Option<u64>,
}

impl Default for NetworkImpairmentConfig {
    fn default() -> Self {
        Self::off(DEFAULT_IMPAIRMENT_SEED)
    }
}

/// Default master seed when env is unset.
pub const DEFAULT_IMPAIRMENT_SEED: u64 = 0x5056_C0FF_EE00_0001;

impl NetworkImpairmentConfig {
    #[must_use]
    pub fn off(seed: u64) -> Self {
        Self {
            enabled: false,
            profile: ImpairmentProfile::Off,
            seed,
            input_base_delay_ns: 0,
            input_jitter_ns: 0,
            snapshot_base_delay_ns: 0,
            snapshot_jitter_ns: 0,
            snapshot_keep_every_n: None,
            auto_stall_interval_ns: None,
            auto_stall_duration_ns: None,
        }
    }

    #[must_use]
    pub fn from_profile(profile: ImpairmentProfile, seed: u64) -> Self {
        match profile {
            ImpairmentProfile::Off => Self::off(seed),
            ImpairmentProfile::Clean => Self {
                enabled: true,
                profile,
                seed,
                ..Self::off(seed)
            },
            ImpairmentProfile::Lan => Self {
                enabled: true,
                profile,
                seed,
                input_base_delay_ns: 10 * MS,
                input_jitter_ns: 2 * MS,
                snapshot_base_delay_ns: 10 * MS,
                snapshot_jitter_ns: 2 * MS,
                snapshot_keep_every_n: None,
                auto_stall_interval_ns: None,
                auto_stall_duration_ns: None,
            },
            ImpairmentProfile::Moderate => Self {
                enabled: true,
                profile,
                seed,
                input_base_delay_ns: 60 * MS,
                input_jitter_ns: 20 * MS,
                snapshot_base_delay_ns: 60 * MS,
                snapshot_jitter_ns: 20 * MS,
                snapshot_keep_every_n: None,
                auto_stall_interval_ns: None,
                auto_stall_duration_ns: None,
            },
            ImpairmentProfile::Bad => Self {
                enabled: true,
                profile,
                seed,
                input_base_delay_ns: 150 * MS,
                input_jitter_ns: 40 * MS,
                snapshot_base_delay_ns: 150 * MS,
                snapshot_jitter_ns: 40 * MS,
                snapshot_keep_every_n: None,
                auto_stall_interval_ns: None,
                auto_stall_duration_ns: None,
            },
            ImpairmentProfile::StallTest => Self {
                enabled: true,
                profile,
                seed,
                input_base_delay_ns: 0,
                input_jitter_ns: 0,
                snapshot_base_delay_ns: 0,
                snapshot_jitter_ns: 0,
                snapshot_keep_every_n: None,
                auto_stall_interval_ns: Some(4_000 * MS),
                auto_stall_duration_ns: Some(500 * MS),
            },
        }
    }

    /// `PURGATORY_NET_IMPAIRMENT` / `PURGATORY_NET_IMPAIRMENT_SEED`.
    #[must_use]
    pub fn from_env() -> Self {
        let seed = std::env::var("PURGATORY_NET_IMPAIRMENT_SEED")
            .ok()
            .and_then(|s| parse_seed(&s))
            .unwrap_or(DEFAULT_IMPAIRMENT_SEED);
        let profile = std::env::var("PURGATORY_NET_IMPAIRMENT")
            .ok()
            .and_then(|s| ImpairmentProfile::parse(&s))
            .unwrap_or(ImpairmentProfile::Off);
        Self::from_profile(profile, seed)
    }

    #[must_use]
    pub fn is_off(&self) -> bool {
        !self.enabled || self.profile == ImpairmentProfile::Off
    }

    /// Configured artificial round-trip (input one-way + snapshot one-way).
    /// Not measured ping.
    #[must_use]
    pub fn configured_artificial_rtt_ns(&self) -> u64 {
        self.input_base_delay_ns
            .saturating_add(self.snapshot_base_delay_ns)
    }
}

fn parse_seed(s: &str) -> Option<u64> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        t.parse().ok()
    }
}

/// SplitMix64 — independent stream seeds from one master seed.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[must_use]
pub fn stream_seed(master: u64, stream: u64) -> u64 {
    splitmix64(master ^ stream.wrapping_mul(0xD1B5_4A32_D192_ED03))
}

/// Deterministic LCG. Not `thread_rng`.
#[derive(Clone, Debug)]
pub struct Lcg(u64);

impl Lcg {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 11
    }

    fn uniform_inclusive(&mut self, max_inclusive: u64) -> u64 {
        if max_inclusive == 0 {
            return 0;
        }
        self.next_u64() % (max_inclusive.saturating_add(1))
    }
}

fn sample_delay_ns(rng: &mut Lcg, base: u64, jitter: u64) -> u64 {
    if jitter == 0 {
        return base;
    }
    let span = jitter.saturating_mul(2);
    let draw = rng.uniform_inclusive(span);
    base.saturating_add(draw).saturating_sub(jitter)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverflowPolicy {
    /// Reliable command traffic: overflow is an explicit failure.
    Fail,
    /// Snapshots: drop oldest delayed frame (application-level skip).
    DropOldest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnqueueError {
    Overflow,
}

#[derive(Clone, Debug)]
struct Delayed<T> {
    item: T,
    enqueued_at: u64,
    release_at: u64,
}

/// Per-lane FIFO delay/stall scheduler. Virtual-time `now_ns` is injected.
#[derive(Debug)]
pub struct ImpairmentLane<T> {
    cap: usize,
    overflow: OverflowPolicy,
    rng: Lcg,
    base_delay_ns: u64,
    jitter_ns: u64,
    keep_every_n: Option<u32>,
    skip_phase: u32,
    hold_until: u64,
    stall_started_at: u64,
    prev_release_at: u64,
    queue: VecDeque<Delayed<T>>,
    delayed_count: u64,
    largest_delay_ns: u64,
    last_delay_ns: u64,
    overflow_skips: u64,
    application_skips: u64,
}

impl<T> ImpairmentLane<T> {
    #[must_use]
    pub fn new(cap: usize, overflow: OverflowPolicy, rng_seed: u64) -> Self {
        Self {
            cap,
            overflow,
            rng: Lcg::new(rng_seed),
            base_delay_ns: 0,
            jitter_ns: 0,
            keep_every_n: None,
            skip_phase: 0,
            hold_until: 0,
            stall_started_at: 0,
            prev_release_at: 0,
            queue: VecDeque::new(),
            delayed_count: 0,
            largest_delay_ns: 0,
            last_delay_ns: 0,
            overflow_skips: 0,
            application_skips: 0,
        }
    }

    pub fn reseed(&mut self, seed: u64) {
        self.rng = Lcg::new(seed);
    }

    /// Delay/jitter/skip for **future** enqueues. Does not rewrite queued deadlines.
    pub fn set_delay(&mut self, base_delay_ns: u64, jitter_ns: u64, keep_every_n: Option<u32>) {
        self.base_delay_ns = base_delay_ns;
        self.jitter_ns = jitter_ns;
        self.keep_every_n = keep_every_n.filter(|n| *n > 1);
        self.skip_phase = 0;
    }

    /// Reset keep-every-n phase to 0 without changing delay.
    pub fn reset_skip_phase(&mut self) {
        self.skip_phase = 0;
    }

    pub fn begin_stall(&mut self, now_ns: u64, duration_ns: u64) {
        let until = now_ns.saturating_add(duration_ns);
        if now_ns >= self.hold_until {
            self.stall_started_at = now_ns;
        }
        self.hold_until = self.hold_until.max(until);
    }

    /// Make queued items immediately eligible. They still drain via `poll_due`.
    pub fn mark_queued_eligible(&mut self, now_ns: u64) {
        self.hold_until = 0;
        self.stall_started_at = 0;
        for item in &mut self.queue {
            item.release_at = now_ns;
        }
        self.prev_release_at = now_ns;
    }

    pub fn clear_queue(&mut self) {
        self.queue.clear();
        self.hold_until = 0;
        self.stall_started_at = 0;
        self.prev_release_at = 0;
    }

    pub fn reset_metrics(&mut self) {
        self.delayed_count = 0;
        self.largest_delay_ns = 0;
        self.last_delay_ns = 0;
        self.overflow_skips = 0;
        self.application_skips = 0;
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    #[must_use]
    pub fn stalled(&self, now_ns: u64) -> bool {
        now_ns < self.hold_until
    }

    /// Elapsed stall time (includes time already spent in the stall).
    #[must_use]
    pub fn stall_age_ns(&self, now_ns: u64) -> u64 {
        if !self.stalled(now_ns) {
            0
        } else {
            now_ns.saturating_sub(self.stall_started_at)
        }
    }

    #[must_use]
    pub fn should_delay(&self, now_ns: u64) -> bool {
        self.stalled(now_ns)
            || self.base_delay_ns > 0
            || self.jitter_ns > 0
            || self.keep_every_n.is_some()
    }

    pub fn enqueue(&mut self, item: T, now_ns: u64) -> Result<(), EnqueueError> {
        if let Some(n) = self.keep_every_n {
            let keep = self.skip_phase.is_multiple_of(n);
            self.skip_phase = self.skip_phase.wrapping_add(1);
            if !keep {
                self.application_skips = self.application_skips.saturating_add(1);
                return Ok(());
            }
        }
        if self.queue.len() >= self.cap {
            match self.overflow {
                OverflowPolicy::Fail => return Err(EnqueueError::Overflow),
                OverflowPolicy::DropOldest => {
                    self.queue.pop_front();
                    self.overflow_skips = self.overflow_skips.saturating_add(1);
                }
            }
        }
        let delay = sample_delay_ns(&mut self.rng, self.base_delay_ns, self.jitter_ns);
        let raw = now_ns.saturating_add(delay);
        let release_at = raw.max(self.prev_release_at);
        self.prev_release_at = release_at;
        self.queue.push_back(Delayed {
            item,
            enqueued_at: now_ns,
            release_at,
        });
        Ok(())
    }

    /// FIFO release of items whose deadline has passed and stall has ended.
    /// Residence time includes stall. Bounded by `max_n`.
    pub fn poll_due(&mut self, now_ns: u64, max_n: usize) -> Vec<T> {
        if max_n == 0 || self.stalled(now_ns) {
            return Vec::new();
        }
        let mut out = Vec::new();
        while out.len() < max_n {
            let Some(front) = self.queue.front() else {
                break;
            };
            if front.release_at > now_ns {
                break;
            }
            let delayed = self.queue.pop_front().expect("front");
            let residence = now_ns.saturating_sub(delayed.enqueued_at);
            self.last_delay_ns = residence;
            if residence > 0 {
                self.delayed_count = self.delayed_count.saturating_add(1);
            }
            if residence > self.largest_delay_ns {
                self.largest_delay_ns = residence;
            }
            out.push(delayed.item);
        }
        out
    }

    #[must_use]
    pub fn has_due(&self, now_ns: u64) -> bool {
        if self.stalled(now_ns) {
            return false;
        }
        self.queue
            .front()
            .is_some_and(|item| item.release_at <= now_ns)
    }

    #[must_use]
    pub fn next_wake_ns(&self, now_ns: u64) -> Option<u64> {
        if self.queue.is_empty() {
            return None;
        }
        if self.stalled(now_ns) {
            return Some(self.hold_until);
        }
        self.queue.front().map(|item| item.release_at)
    }

    #[must_use]
    pub fn delayed_count(&self) -> u64 {
        self.delayed_count
    }

    #[must_use]
    pub fn largest_delay_ns(&self) -> u64 {
        self.largest_delay_ns
    }

    #[must_use]
    pub fn last_delay_ns(&self) -> u64 {
        self.last_delay_ns
    }

    #[must_use]
    pub fn overflow_skips(&self) -> u64 {
        self.overflow_skips
    }

    #[must_use]
    pub fn application_skips(&self) -> u64 {
        self.application_skips
    }
}

/// Overlay / debug gauges. Latest-wins; not an event log.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImpairmentMetricsSnapshot {
    pub enabled: bool,
    pub profile: ImpairmentProfile,
    pub seed: u64,
    pub input_base_delay_ns: u64,
    pub input_jitter_ns: u64,
    pub snapshot_base_delay_ns: u64,
    pub snapshot_jitter_ns: u64,
    pub configured_artificial_rtt_ns: u64,
    pub snapshot_keep_every_n: Option<u32>,
    pub input_stalled: bool,
    pub stall_age_ns: u64,
    pub input_messages_delayed: u64,
    pub snapshot_messages_delayed: u64,
    pub largest_input_delay_ns: u64,
    pub largest_snapshot_delay_ns: u64,
    pub last_input_delay_ns: u64,
    pub last_snapshot_delay_ns: u64,
    pub input_queue_depth: u32,
    pub snapshot_queue_depth: u32,
    pub snapshot_overflow_skips: u64,
    pub snapshot_application_skips: u64,
    pub stall_trigger_dropped: u64,
}

/// Owns input + snapshot lanes with independent PRNG streams from one master seed.
#[derive(Debug)]
pub struct ImpairmentHarness<I, S> {
    config: NetworkImpairmentConfig,
    input: ImpairmentLane<I>,
    snapshot: ImpairmentLane<S>,
    auto_stall_rng: Lcg,
    last_auto_stall_at: u64,
    stall_trigger_dropped: u64,
}

impl<I, S> ImpairmentHarness<I, S> {
    #[must_use]
    pub fn new(config: NetworkImpairmentConfig) -> Self {
        let mut h = Self {
            input: ImpairmentLane::new(
                INPUT_LANE_CAP,
                OverflowPolicy::Fail,
                stream_seed(config.seed, STREAM_INPUT),
            ),
            snapshot: ImpairmentLane::new(
                SNAPSHOT_LANE_CAP,
                OverflowPolicy::DropOldest,
                stream_seed(config.seed, STREAM_SNAPSHOT),
            ),
            auto_stall_rng: Lcg::new(stream_seed(config.seed, STREAM_AUTO_STALL)),
            last_auto_stall_at: 0,
            stall_trigger_dropped: 0,
            config: NetworkImpairmentConfig::off(config.seed),
        };
        h.apply_config(config, 0);
        h
    }

    #[must_use]
    pub fn config(&self) -> NetworkImpairmentConfig {
        self.config
    }

    #[must_use]
    pub fn input(&self) -> &ImpairmentLane<I> {
        &self.input
    }

    #[must_use]
    pub fn snapshot(&self) -> &ImpairmentLane<S> {
        &self.snapshot
    }

    pub fn input_mut(&mut self) -> &mut ImpairmentLane<I> {
        &mut self.input
    }

    pub fn snapshot_mut(&mut self) -> &mut ImpairmentLane<S> {
        &mut self.snapshot
    }

    /// Config changes affect future enqueues. Queued items keep deadlines.
    /// Switching to Off makes queued items immediately eligible (bounded drain).
    /// Seed change reseeds all three PRNG streams for future draws only.
    /// Any config apply resets snapshot keep-every-n phase to 0.
    /// Does **not** reset accumulated metrics.
    pub fn apply_config(&mut self, config: NetworkImpairmentConfig, now_ns: u64) {
        let seed_changed = config.seed != self.config.seed;
        self.config = config;
        if seed_changed {
            self.input.reseed(stream_seed(config.seed, STREAM_INPUT));
            self.snapshot
                .reseed(stream_seed(config.seed, STREAM_SNAPSHOT));
            self.auto_stall_rng = Lcg::new(stream_seed(config.seed, STREAM_AUTO_STALL));
        }
        self.input.set_delay(
            if config.enabled {
                config.input_base_delay_ns
            } else {
                0
            },
            if config.enabled {
                config.input_jitter_ns
            } else {
                0
            },
            None,
        );
        self.snapshot.set_delay(
            if config.enabled {
                config.snapshot_base_delay_ns
            } else {
                0
            },
            if config.enabled {
                config.snapshot_jitter_ns
            } else {
                0
            },
            if config.enabled {
                config.snapshot_keep_every_n
            } else {
                None
            },
        );
        if config.is_off() {
            self.input.mark_queued_eligible(now_ns);
            self.snapshot.mark_queued_eligible(now_ns);
        }
    }

    pub fn begin_input_stall(&mut self, now_ns: u64, duration_ns: u64) {
        self.input.begin_stall(now_ns, duration_ns);
    }

    pub fn note_stall_trigger_dropped(&mut self) {
        self.stall_trigger_dropped = self.stall_trigger_dropped.saturating_add(1);
    }

    pub fn tick_auto_stall(&mut self, now_ns: u64) {
        if !self.config.enabled {
            return;
        }
        let Some(interval) = self.config.auto_stall_interval_ns else {
            return;
        };
        let Some(duration) = self.config.auto_stall_duration_ns else {
            return;
        };
        if interval == 0 {
            return;
        }
        let extra = self.auto_stall_rng.uniform_inclusive(interval / 8);
        let due_at = self
            .last_auto_stall_at
            .saturating_add(interval)
            .saturating_add(extra);
        if now_ns >= due_at && now_ns > 0 {
            self.input.begin_stall(now_ns, duration);
            self.last_auto_stall_at = now_ns;
        }
    }

    pub fn reset_metrics(&mut self) {
        self.input.reset_metrics();
        self.snapshot.reset_metrics();
        self.stall_trigger_dropped = 0;
    }

    pub fn clear_queues(&mut self) {
        self.input.clear_queue();
        self.snapshot.clear_queue();
    }

    #[must_use]
    pub fn metrics(&self, now_ns: u64) -> ImpairmentMetricsSnapshot {
        let cfg = self.config;
        ImpairmentMetricsSnapshot {
            enabled: cfg.enabled && cfg.profile != ImpairmentProfile::Off,
            profile: cfg.profile,
            seed: cfg.seed,
            input_base_delay_ns: cfg.input_base_delay_ns,
            input_jitter_ns: cfg.input_jitter_ns,
            snapshot_base_delay_ns: cfg.snapshot_base_delay_ns,
            snapshot_jitter_ns: cfg.snapshot_jitter_ns,
            configured_artificial_rtt_ns: cfg.configured_artificial_rtt_ns(),
            snapshot_keep_every_n: cfg.snapshot_keep_every_n,
            input_stalled: self.input.stalled(now_ns),
            stall_age_ns: self.input.stall_age_ns(now_ns),
            input_messages_delayed: self.input.delayed_count(),
            snapshot_messages_delayed: self.snapshot.delayed_count(),
            largest_input_delay_ns: self.input.largest_delay_ns(),
            largest_snapshot_delay_ns: self.snapshot.largest_delay_ns(),
            last_input_delay_ns: self.input.last_delay_ns(),
            last_snapshot_delay_ns: self.snapshot.last_delay_ns(),
            input_queue_depth: self.input.len() as u32,
            snapshot_queue_depth: self.snapshot.len() as u32,
            snapshot_overflow_skips: self.snapshot.overflow_skips(),
            snapshot_application_skips: self.snapshot.application_skips(),
            stall_trigger_dropped: self.stall_trigger_dropped,
        }
    }

    #[must_use]
    pub fn next_wake_ns(&self, now_ns: u64) -> Option<u64> {
        match (
            self.input.next_wake_ns(now_ns),
            self.snapshot.next_wake_ns(now_ns),
        ) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        }
    }
}

/// Convert nanoseconds to whole milliseconds for overlay labels.
#[must_use]
pub fn ns_to_ms(ns: u64) -> u64 {
    ns / MS
}

#[must_use]
pub fn duration_ns(d: Duration) -> u64 {
    u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
}

/// Model of one live-loop fairness turn: bounded input drain, then control plane.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FairnessTurn {
    pub inputs_drained: usize,
    pub inputs_remaining: usize,
    pub snapshots_serviced: u32,
    pub stalls_serviced: u32,
    pub configs_serviced: u32,
    pub disconnects_serviced: u32,
}

/// Drain at most `drain_n` due inputs, then service pending control/snapshot events.
#[must_use]
pub fn fairness_turn(
    due_inputs: usize,
    drain_n: usize,
    pending_snapshots: u32,
    pending_stalls: u32,
    pending_configs: u32,
    pending_disconnects: u32,
) -> FairnessTurn {
    let drained = due_inputs.min(drain_n);
    FairnessTurn {
        inputs_drained: drained,
        inputs_remaining: due_inputs.saturating_sub(drained),
        snapshots_serviced: pending_snapshots,
        stalls_serviced: pending_stalls,
        configs_serviced: pending_configs,
        disconnects_serviced: pending_disconnects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_delay(base_ms: u64, jitter_ms: u64, seed: u64) -> NetworkImpairmentConfig {
        NetworkImpairmentConfig {
            enabled: true,
            profile: ImpairmentProfile::Moderate,
            seed,
            input_base_delay_ns: base_ms * MS,
            input_jitter_ns: jitter_ms * MS,
            snapshot_base_delay_ns: 0,
            snapshot_jitter_ns: 0,
            snapshot_keep_every_n: None,
            auto_stall_interval_ns: None,
            auto_stall_duration_ns: None,
        }
    }

    #[test]
    fn disabled_preserves_order_and_zero_delay() {
        let mut lane: ImpairmentLane<u32> = ImpairmentLane::new(16, OverflowPolicy::Fail, 1);
        for i in 0..5 {
            lane.enqueue(i, 0).unwrap();
        }
        let got = lane.poll_due(0, 16);
        assert_eq!(got, vec![0, 1, 2, 3, 4]);
        assert_eq!(lane.delayed_count(), 0);
        assert_eq!(lane.largest_delay_ns(), 0);
    }

    #[test]
    fn fixed_delay_not_due_before_base() {
        let mut lane: ImpairmentLane<u8> = ImpairmentLane::new(8, OverflowPolicy::Fail, 1);
        lane.set_delay(100 * MS, 0, None);
        lane.enqueue(1, 0).unwrap();
        assert!(lane.poll_due(50 * MS, 8).is_empty());
        assert_eq!(lane.poll_due(100 * MS, 8), vec![1]);
        assert_eq!(lane.last_delay_ns(), 100 * MS);
        assert_eq!(lane.delayed_count(), 1);
    }

    #[test]
    fn ordered_jitter_never_reorders() {
        let mut lane: ImpairmentLane<u32> = ImpairmentLane::new(32, OverflowPolicy::Fail, 42);
        lane.set_delay(80 * MS, 30 * MS, None);
        for i in 0..20 {
            lane.enqueue(i, u64::from(i) * MS).unwrap();
        }
        let mut seen = Vec::new();
        let mut t = 0;
        while seen.len() < 20 {
            t += 10 * MS;
            seen.extend(lane.poll_due(t, 32));
        }
        assert_eq!(seen, (0..20).collect::<Vec<_>>());
    }

    #[test]
    fn stall_accumulates_then_bursts() {
        let mut lane: ImpairmentLane<u32> = ImpairmentLane::new(64, OverflowPolicy::Fail, 1);
        lane.begin_stall(0, 500 * MS);
        for i in 0..10 {
            lane.enqueue(i, u64::from(i) * 10 * MS).unwrap();
        }
        assert!(lane.poll_due(400 * MS, 64).is_empty());
        assert_eq!(lane.len(), 10);
        let burst = lane.poll_due(500 * MS, 64);
        assert_eq!(burst, (0..10).collect::<Vec<_>>());
        assert!(lane.largest_delay_ns() >= 500 * MS - 90 * MS);
    }

    #[test]
    fn same_seed_same_schedule() {
        let mut a: ImpairmentLane<u32> =
            ImpairmentLane::new(16, OverflowPolicy::Fail, stream_seed(7, STREAM_INPUT));
        let mut b: ImpairmentLane<u32> =
            ImpairmentLane::new(16, OverflowPolicy::Fail, stream_seed(7, STREAM_INPUT));
        a.set_delay(50 * MS, 20 * MS, None);
        b.set_delay(50 * MS, 20 * MS, None);
        for i in 0..8 {
            a.enqueue(i, 0).unwrap();
            b.enqueue(i, 0).unwrap();
        }
        assert_eq!(a.poll_due(u64::MAX, 16), b.poll_due(u64::MAX, 16));
    }

    #[test]
    fn independent_streams_do_not_perturb() {
        let mut input_only: ImpairmentLane<u32> =
            ImpairmentLane::new(8, OverflowPolicy::Fail, stream_seed(9, STREAM_INPUT));
        let mut input_after_snap: ImpairmentLane<u32> =
            ImpairmentLane::new(8, OverflowPolicy::Fail, stream_seed(9, STREAM_INPUT));
        let mut snap: ImpairmentLane<u32> = ImpairmentLane::new(
            8,
            OverflowPolicy::DropOldest,
            stream_seed(9, STREAM_SNAPSHOT),
        );
        input_only.set_delay(40 * MS, 20 * MS, None);
        input_after_snap.set_delay(40 * MS, 20 * MS, None);
        snap.set_delay(40 * MS, 20 * MS, None);
        for _ in 0..5 {
            snap.enqueue(0, 0).unwrap();
        }
        input_only.enqueue(1, 0).unwrap();
        input_after_snap.enqueue(1, 0).unwrap();
        let t = u64::MAX;
        assert_eq!(input_only.poll_due(t, 8), input_after_snap.poll_due(t, 8));
    }

    #[test]
    fn input_overflow_fails_without_drop() {
        let mut lane: ImpairmentLane<u32> = ImpairmentLane::new(2, OverflowPolicy::Fail, 1);
        lane.enqueue(1, 0).unwrap();
        lane.enqueue(2, 0).unwrap();
        assert_eq!(lane.enqueue(3, 0), Err(EnqueueError::Overflow));
        assert_eq!(lane.poll_due(0, 8), vec![1, 2]);
    }

    #[test]
    fn snapshot_overflow_drops_oldest() {
        let mut lane: ImpairmentLane<u32> = ImpairmentLane::new(2, OverflowPolicy::DropOldest, 1);
        lane.enqueue(1, 0).unwrap();
        lane.enqueue(2, 0).unwrap();
        lane.enqueue(3, 0).unwrap();
        assert_eq!(lane.overflow_skips(), 1);
        assert_eq!(lane.poll_due(0, 8), vec![2, 3]);
    }

    #[test]
    fn keep_every_n_skips_deterministically() {
        let mut lane: ImpairmentLane<u32> = ImpairmentLane::new(16, OverflowPolicy::DropOldest, 1);
        lane.set_delay(0, 0, Some(3));
        for i in 0..6 {
            lane.enqueue(i, 0).unwrap();
        }
        assert_eq!(lane.poll_due(0, 16), vec![0, 3]);
        assert_eq!(lane.application_skips(), 4);
    }

    #[test]
    fn config_change_resets_skip_phase_not_queued_deadlines() {
        let mut h: ImpairmentHarness<u32, u32> = ImpairmentHarness::new(cfg_delay(100, 0, 1));
        h.input_mut().enqueue(1, 0).unwrap();
        let mut later = cfg_delay(10, 0, 1);
        later.snapshot_keep_every_n = Some(2);
        h.apply_config(later, 50 * MS);
        assert!(h.input_mut().poll_due(50 * MS, 8).is_empty());
        assert_eq!(h.input_mut().poll_due(100 * MS, 8), vec![1]);
        h.snapshot_mut().enqueue(10, 100 * MS).unwrap();
        h.snapshot_mut().enqueue(11, 100 * MS).unwrap();
        assert_eq!(h.snapshot_mut().poll_due(100 * MS, 8), vec![10]);
        assert_eq!(h.snapshot().application_skips(), 1);
    }

    #[test]
    fn seed_change_reseeds_future_only() {
        let mut h: ImpairmentHarness<u32, u32> = ImpairmentHarness::new(cfg_delay(50, 20, 1));
        h.input_mut().enqueue(1, 0).unwrap();
        h.apply_config(cfg_delay(50, 20, 99), 0);
        h.input_mut().enqueue(2, 0).unwrap();
        assert_eq!(h.input_mut().poll_due(u64::MAX, 1), vec![1]);
        let remaining = h.input_mut().poll_due(u64::MAX, 8);
        let mut fresh: ImpairmentLane<u32> =
            ImpairmentLane::new(8, OverflowPolicy::Fail, stream_seed(99, STREAM_INPUT));
        fresh.set_delay(50 * MS, 20 * MS, None);
        fresh.enqueue(2, 0).unwrap();
        assert_eq!(remaining, fresh.poll_due(u64::MAX, 8));
    }

    #[test]
    fn off_flush_makes_queued_eligible_fifo() {
        let mut h: ImpairmentHarness<u32, u32> = ImpairmentHarness::new(cfg_delay(1_000, 0, 1));
        h.input_mut().enqueue(1, 0).unwrap();
        h.input_mut().enqueue(2, 0).unwrap();
        h.apply_config(NetworkImpairmentConfig::off(1), 10 * MS);
        assert_eq!(h.input_mut().poll_due(10 * MS, 1), vec![1]);
        assert_eq!(h.input().len(), 1);
        assert_eq!(h.input_mut().poll_due(10 * MS, 1), vec![2]);
    }

    #[test]
    fn off_flush_does_not_reset_metrics() {
        let mut h: ImpairmentHarness<u32, u32> = ImpairmentHarness::new(cfg_delay(50, 0, 1));
        h.input_mut().enqueue(1, 0).unwrap();
        let _ = h.input_mut().poll_due(50 * MS, 8);
        assert_eq!(h.input().delayed_count(), 1);
        h.apply_config(NetworkImpairmentConfig::off(1), 50 * MS);
        assert_eq!(h.input().delayed_count(), 1);
        h.reset_metrics();
        assert_eq!(h.input().delayed_count(), 0);
    }

    #[test]
    fn poll_due_is_bounded() {
        let mut lane: ImpairmentLane<u32> = ImpairmentLane::new(64, OverflowPolicy::Fail, 1);
        for i in 0..32 {
            lane.enqueue(i, 0).unwrap();
        }
        assert_eq!(lane.poll_due(0, INPUT_DRAIN_PER_TURN).len(), 8);
        assert_eq!(lane.len(), 24);
        assert_eq!(lane.poll_due(0, INPUT_DRAIN_PER_TURN).len(), 8);
    }

    #[test]
    fn stall_time_counts_as_residence() {
        let mut lane: ImpairmentLane<u32> = ImpairmentLane::new(8, OverflowPolicy::Fail, 1);
        lane.enqueue(1, 0).unwrap();
        lane.begin_stall(0, 250 * MS);
        assert!(lane.poll_due(100 * MS, 8).is_empty());
        let got = lane.poll_due(250 * MS, 8);
        assert_eq!(got, vec![1]);
        assert_eq!(lane.last_delay_ns(), 250 * MS);
    }

    #[test]
    fn fairness_turn_services_control_during_input_burst() {
        let turn = fairness_turn(32, INPUT_DRAIN_PER_TURN, 2, 1, 1, 1);
        assert_eq!(turn.inputs_drained, 8);
        assert_eq!(turn.inputs_remaining, 24);
        assert_eq!(turn.snapshots_serviced, 2);
        assert_eq!(turn.stalls_serviced, 1);
        assert_eq!(turn.configs_serviced, 1);
        assert_eq!(turn.disconnects_serviced, 1);
    }

    #[test]
    fn profile_from_env_defaults_off() {
        let cfg = NetworkImpairmentConfig::off(DEFAULT_IMPAIRMENT_SEED);
        assert!(cfg.is_off());
        assert_eq!(cfg.configured_artificial_rtt_ns(), 0);
    }
}
