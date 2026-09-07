//! Semantic player input. No keyboard or window types.

/// Intent sampled for one simulation tick.
///
/// `move_axis` and `down_held` are held state. `jump_pressed` is an edge.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlayerInput {
    /// Horizontal intent: `-1` left, `0` idle, `1` right.
    pub move_axis: i8,
    /// True only on the tick that should attempt a jump (or drop-through).
    pub jump_pressed: bool,
    /// True while Jump remains held; used for authoritative short-hop control.
    pub jump_held: bool,
    /// True while Down is held. Combined with jump for OneWay drop-through.
    pub down_held: bool,
}

impl PlayerInput {
    #[must_use]
    pub const fn idle() -> Self {
        Self {
            move_axis: 0,
            jump_pressed: false,
            jump_held: false,
            down_held: false,
        }
    }

    /// Build input from held directions and a jump edge.
    ///
    /// Opposite left/right cancel to `0`.
    #[must_use]
    pub const fn from_buttons(left: bool, right: bool, jump_pressed: bool) -> Self {
        Self::from_buttons_ext(left, right, jump_pressed, false)
    }

    /// Build input including down hold.
    #[must_use]
    pub const fn from_buttons_ext(
        left: bool,
        right: bool,
        jump_pressed: bool,
        down_held: bool,
    ) -> Self {
        let move_axis = match (left, right) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };
        Self {
            move_axis,
            jump_pressed,
            jump_held: jump_pressed,
            down_held,
        }
    }

    #[must_use]
    pub const fn with_jump_held(mut self, jump_held: bool) -> Self {
        self.jump_held = jump_held;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_directions_cancel() {
        let input = PlayerInput::from_buttons(true, true, false);
        assert_eq!(input.move_axis, 0);
        assert!(!input.jump_pressed);
        assert!(!input.down_held);
    }

    #[test]
    fn left_and_right_are_signed() {
        assert_eq!(PlayerInput::from_buttons(true, false, false).move_axis, -1);
        assert_eq!(PlayerInput::from_buttons(false, true, true).move_axis, 1);
        assert!(PlayerInput::from_buttons(false, true, true).jump_pressed);
    }

    #[test]
    fn down_held_is_independent() {
        let input = PlayerInput::from_buttons_ext(false, true, true, true);
        assert_eq!(input.move_axis, 1);
        assert!(input.jump_pressed);
        assert!(input.down_held);
    }
}
