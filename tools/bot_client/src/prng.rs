//! Deterministic LCG for reproducible bot behavior.

#[derive(Clone, Copy, Debug)]
pub struct Lcg {
    state: u64,
}

impl Lcg {
    const MULTIPLIER: u64 = 6364136223846793005;
    const INCREMENT: u64 = 1442695040888963407;

    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    #[must_use]
    pub fn seeded(seed: u64, bot_id: u32, profile_hash: u32) -> Self {
        let combined = seed ^ (u64::from(bot_id) << 16) ^ (u64::from(profile_hash) << 32);
        Self::new(combined)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(Self::MULTIPLIER)
            .wrapping_add(Self::INCREMENT);
        self.state
    }

    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    pub fn next_bool(&mut self, probability: f32) -> bool {
        let threshold = (probability * f32::from(u16::MAX)) as u32;
        (self.next_u32() & 0xFFFF) < threshold
    }

    pub fn next_range(&mut self, min: u32, max: u32) -> u32 {
        if max <= min {
            return min;
        }
        let range = max - min;
        min + (self.next_u32() % range)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_produces_same_sequence() {
        let mut a = Lcg::new(42);
        let mut b = Lcg::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_produce_different_sequences() {
        let mut a = Lcg::new(42);
        let mut b = Lcg::new(43);
        let mut same_count = 0;
        for _ in 0..100 {
            if a.next_u64() == b.next_u64() {
                same_count += 1;
            }
        }
        assert!(same_count < 5, "sequences should differ");
    }

    #[test]
    fn seeded_with_different_bot_id_differs() {
        let mut a = Lcg::seeded(100, 1, 0);
        let mut b = Lcg::seeded(100, 2, 0);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn seeded_with_different_profile_differs() {
        let mut a = Lcg::seeded(100, 1, 1);
        let mut b = Lcg::seeded(100, 1, 2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn next_range_stays_in_bounds() {
        let mut rng = Lcg::new(12345);
        for _ in 0..1000 {
            let val = rng.next_range(10, 20);
            assert!((10..20).contains(&val));
        }
    }

    #[test]
    fn next_bool_respects_probability() {
        let mut rng = Lcg::new(54321);
        let mut true_count = 0;
        let trials = 10000;
        for _ in 0..trials {
            if rng.next_bool(0.5) {
                true_count += 1;
            }
        }
        let ratio = true_count as f64 / trials as f64;
        assert!(
            ratio > 0.45 && ratio < 0.55,
            "ratio {ratio} should be near 0.5"
        );
    }
}
