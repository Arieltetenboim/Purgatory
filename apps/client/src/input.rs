//! Semantic actions. Keyboard scancodes stay in this client module.

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use purgatory_simulation::PlayerInput;

/// Gameplay action. Simulation never sees `KeyCode`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    MoveLeft,
    MoveRight,
    MoveDown,
    Jump,
}

/// Held buttons plus a jump edge queued for the next simulation tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct ActionState {
    move_left: bool,
    move_right: bool,
    move_down: bool,
    jump_held: bool,
    jump_edge: bool,
}

impl ActionState {
    pub fn apply_key_event(&mut self, event: &KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let Some(action) = map_key(code) else {
            return;
        };
        let pressed = event.state == ElementState::Pressed;
        self.set_action(action, pressed, event.repeat);
    }

    pub fn set_action(&mut self, action: Action, pressed: bool, repeat: bool) {
        match action {
            Action::MoveLeft => {
                if !repeat {
                    self.move_left = pressed;
                }
            }
            Action::MoveRight => {
                if !repeat {
                    self.move_right = pressed;
                }
            }
            Action::MoveDown => {
                if !repeat {
                    self.move_down = pressed;
                }
            }
            Action::Jump => {
                if repeat {
                    return;
                }
                if pressed {
                    if !self.jump_held {
                        self.jump_edge = true;
                    }
                    self.jump_held = true;
                } else {
                    self.jump_held = false;
                }
            }
        }
    }

    /// Snapshot for one simulation tick. Consumes the jump edge.
    #[must_use]
    pub fn consume_tick_input(&mut self) -> PlayerInput {
        let jump_pressed = self.jump_edge;
        self.jump_edge = false;
        PlayerInput::from_buttons_ext(
            self.move_left,
            self.move_right,
            jump_pressed,
            self.move_down,
        )
    }

    /// Drop held gameplay buttons.
    #[cfg(test)]
    pub fn clear(&mut self) {
        self.move_left = false;
        self.move_right = false;
        self.move_down = false;
        self.jump_held = false;
        self.jump_edge = false;
    }
}

#[must_use]
pub fn map_key(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::KeyA | KeyCode::ArrowLeft => Some(Action::MoveLeft),
        KeyCode::KeyD | KeyCode::ArrowRight => Some(Action::MoveRight),
        KeyCode::KeyS | KeyCode::ArrowDown => Some(Action::MoveDown),
        KeyCode::Space => Some(Action::Jump),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_keys_map_to_actions() {
        assert_eq!(map_key(KeyCode::KeyA), Some(Action::MoveLeft));
        assert_eq!(map_key(KeyCode::ArrowLeft), Some(Action::MoveLeft));
        assert_eq!(map_key(KeyCode::KeyD), Some(Action::MoveRight));
        assert_eq!(map_key(KeyCode::ArrowRight), Some(Action::MoveRight));
        assert_eq!(map_key(KeyCode::KeyS), Some(Action::MoveDown));
        assert_eq!(map_key(KeyCode::ArrowDown), Some(Action::MoveDown));
        assert_eq!(map_key(KeyCode::Space), Some(Action::Jump));
        assert_eq!(map_key(KeyCode::KeyW), None);
        assert_eq!(map_key(KeyCode::Backquote), None);
    }

    #[test]
    fn jump_edge_fires_once_until_consumed() {
        let mut state = ActionState::default();
        state.set_action(Action::Jump, true, false);
        state.set_action(Action::Jump, true, true);
        let first = state.consume_tick_input();
        let second = state.consume_tick_input();
        assert!(first.jump_pressed);
        assert!(!second.jump_pressed);
    }

    #[test]
    fn held_move_is_state_not_edge() {
        let mut state = ActionState::default();
        state.set_action(Action::MoveRight, true, false);
        let a = state.consume_tick_input();
        let b = state.consume_tick_input();
        assert_eq!(a.move_axis, 1);
        assert_eq!(b.move_axis, 1);
        state.set_action(Action::MoveRight, false, false);
        assert_eq!(state.consume_tick_input().move_axis, 0);
    }

    #[test]
    fn down_held_passes_to_player_input() {
        let mut state = ActionState::default();
        state.set_action(Action::MoveDown, true, false);
        let input = state.consume_tick_input();
        assert!(input.down_held);
        assert!(!input.jump_pressed);
    }

    #[test]
    fn clear_drops_held_gameplay_state() {
        let mut state = ActionState::default();
        state.set_action(Action::MoveRight, true, false);
        state.set_action(Action::Jump, true, false);
        state.clear();
        let input = state.consume_tick_input();
        assert_eq!(input.move_axis, 0);
        assert!(!input.jump_pressed);
    }

    #[test]
    fn overlay_open_policy_keeps_held_movement() {
        let mut state = ActionState::default();
        state.set_action(Action::MoveRight, true, false);
        assert!(crate::debug::gameplay_receives_keyboard(true, false, true));
        assert_eq!(state.consume_tick_input().move_axis, 1);
    }

    #[test]
    fn text_like_focus_does_not_start_new_gameplay_presses() {
        let mut state = ActionState::default();
        assert!(!crate::debug::gameplay_receives_keyboard(true, true, true));
        assert_eq!(state.consume_tick_input().move_axis, 0);
        assert!(!state.consume_tick_input().jump_pressed);
        assert!(crate::debug::gameplay_receives_keyboard(true, true, false));
        state.set_action(Action::MoveRight, false, false);
        assert_eq!(state.consume_tick_input().move_axis, 0);
    }
}
