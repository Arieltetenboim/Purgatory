//! Development world AABB bounds.
//!
//! Owned by [`crate::World`]. Camera clamping is presentation-only and reads
//! these values; simulation uses them for player containment.

/// Axis-aligned development world limits (world units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldBounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

impl WorldBounds {
    /// Phase-4.8 FOOTNOTE arena (~2× prior horizontal span).
    pub const FOOTNOTE_TEST: Self = Self {
        min_x: -24.0,
        max_x: 24.0,
        min_y: -8.0,
        max_y: 10.0,
    };

    /// Compact Phase-4.5 `dev_stage` sandbox.
    pub const DEV_COMPACT: Self = Self {
        min_x: -10.0,
        max_x: 10.0,
        min_y: -6.0,
        max_y: 6.0,
    };

    #[must_use]
    pub const fn width(self) -> f32 {
        self.max_x - self.min_x
    }

    #[must_use]
    pub const fn height(self) -> f32 {
        self.max_y - self.min_y
    }

    #[must_use]
    pub const fn center(self) -> [f32; 2] {
        [
            (self.min_x + self.max_x) * 0.5,
            (self.min_y + self.max_y) * 0.5,
        ]
    }

    /// True when a point lies strictly inside the open interior.
    #[must_use]
    pub fn contains_point(self, p: [f32; 2]) -> bool {
        p[0] >= self.min_x && p[0] <= self.max_x && p[1] >= self.min_y && p[1] <= self.max_y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footnote_bounds_are_about_twice_prior_span() {
        // Prior arena edges were roughly −12…14 (span ≈ 26).
        assert!((WorldBounds::FOOTNOTE_TEST.width() - 48.0).abs() < 0.01);
        assert!(WorldBounds::FOOTNOTE_TEST.width() > 40.0);
    }
}
