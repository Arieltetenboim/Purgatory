//! World-space 2D transform. Authoritative simulation state, not a GPU matrix.
//!
//! Coordinate convention: **+X right, +Y up**.

/// Position in world units. Rotation and scale are not used in Phase 4.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: [f32; 2],
}

impl Transform {
    #[must_use]
    pub const fn from_position(position: [f32; 2]) -> Self {
        Self { position }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_is_zero() {
        let t = Transform::from_position([0.0, 0.0]);
        assert_eq!(t.position, [0.0, 0.0]);
    }
}
