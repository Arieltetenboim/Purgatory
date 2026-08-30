//! Minimal life container. Not combat resolution.

/// Current / max health. Optional capability.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    #[must_use]
    pub const fn full(max: f32) -> Self {
        Self { current: max, max }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_matches_max() {
        let h = Health::full(10.0);
        assert_eq!(h.current, 10.0);
        assert_eq!(h.max, 10.0);
    }
}
