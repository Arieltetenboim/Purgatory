//! Axis-aligned bounding boxes in world space.

/// Rectangle whose edges stay aligned with the world axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    /// Center in world units.
    pub center: [f32; 2],
    /// Half-width and half-height.
    pub half_extents: [f32; 2],
}

impl Aabb {
    #[must_use]
    pub const fn new(center: [f32; 2], half_extents: [f32; 2]) -> Self {
        Self {
            center,
            half_extents,
        }
    }

    #[must_use]
    pub fn min_x(self) -> f32 {
        self.center[0] - self.half_extents[0]
    }

    #[must_use]
    pub fn max_x(self) -> f32 {
        self.center[0] + self.half_extents[0]
    }

    #[must_use]
    pub fn min_y(self) -> f32 {
        self.center[1] - self.half_extents[1]
    }

    #[must_use]
    pub fn max_y(self) -> f32 {
        self.center[1] + self.half_extents[1]
    }

    #[must_use]
    pub fn size(self) -> [f32; 2] {
        [self.half_extents[0] * 2.0, self.half_extents[1] * 2.0]
    }

    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        self.min_x() < other.max_x()
            && self.max_x() > other.min_x()
            && self.min_y() < other.max_y()
            && self.max_y() > other.min_y()
    }

    #[must_use]
    pub fn contains_point(self, position: [f32; 2]) -> bool {
        position[0] >= self.min_x()
            && position[0] <= self.max_x()
            && position[1] >= self.min_y()
            && position[1] <= self.max_y()
    }

    #[must_use]
    pub fn from_min_max(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        let center = [(min_x + max_x) * 0.5, (min_y + max_y) * 0.5];
        let half_extents = [(max_x - min_x) * 0.5, (max_y - min_y) * 0.5];
        Self {
            center,
            half_extents,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_and_separated_boxes() {
        let a = Aabb::new([0.0, 0.0], [0.5, 0.5]);
        let b = Aabb::new([0.75, 0.0], [0.5, 0.5]);
        let c = Aabb::new([2.0, 0.0], [0.5, 0.5]);
        assert!(a.overlaps(b));
        assert!(!a.overlaps(c));
    }
}
