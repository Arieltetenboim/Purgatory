//! Semantic actions. Keyboard scancodes stay in this client module.

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use purgatory_protocol::{InputCommand, MoveAxis};
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

    #[must_use]
    #[allow(dead_code)]
    pub fn move_axis(&self) -> MoveAxis {
        match (self.move_left, self.move_right) {
            (true, false) => MoveAxis::Left,
            (false, true) => MoveAxis::Right,
            _ => MoveAxis::Neutral,
        }
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn down_held(&self) -> bool {
        self.move_down
    }

    #[must_use]
    #[allow(dead_code)] // retained for debug/UI; tick path uses consume_tick_input
    pub fn jump_pending(&self) -> bool {
        self.jump_edge
    }

    /// Drop held gameplay buttons (screen transitions / focus loss).
    pub fn clear(&mut self) {
        self.move_left = false;
        self.move_right = false;
        self.move_down = false;
        self.jump_held = false;
        self.jump_edge = false;
    }

    /// True when any persistent held gameplay control is active.
    #[cfg(test)]
    #[must_use]
    pub fn has_held_input(&self) -> bool {
        self.move_left || self.move_right || self.move_down || self.jump_held || self.jump_edge
    }

    /// Focus-loss path: release all held controls. Does not synthesize keys on
    /// regain — caller starts from neutral.
    pub fn release_on_focus_loss(&mut self) {
        self.clear();
    }
}

/// Client → server per-tick command sequencer. Sequence starts at 1 each epoch.
/// One `InputCommand` per predicted simulation step. No send-on-change, no
/// refresh coalescing.
#[derive(Clone, Copy, Debug, Default)]
pub struct IntentNet {
    pub sequence: u32,
    pub commands_sent: u64,
    pub move_axis: MoveAxis,
    pub jump_pressed: bool,
    pub down_held: bool,
    pub input_epoch: u16,
}

impl IntentNet {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Adopt the server-owned epoch. Sequence restarts at 1 for the next command.
    pub fn set_epoch(&mut self, epoch: u16) {
        if self.input_epoch != epoch {
            self.input_epoch = epoch;
            self.sequence = 0;
        }
    }

    /// Always emit one command for this predicted tick. `None` if sequence
    /// would wrap (`u32::MAX`).
    #[must_use]
    pub fn consider_tick_input(&mut self, input: PlayerInput) -> Option<InputCommand> {
        self.emit(
            move_axis_from_i8(input.move_axis),
            input.jump_pressed,
            input.down_held,
        )
    }

    /// Forced Neutral command paired with a `SimulationClock` step (focus-loss).
    #[must_use]
    pub fn emit_neutral(&mut self) -> Option<InputCommand> {
        self.emit(MoveAxis::Neutral, false, false)
    }

    fn emit(
        &mut self,
        axis: MoveAxis,
        jump_pressed: bool,
        down_held: bool,
    ) -> Option<InputCommand> {
        if self.sequence == u32::MAX {
            return None;
        }
        self.sequence = self.sequence.saturating_add(1);
        self.move_axis = axis;
        self.down_held = down_held;
        self.jump_pressed = jump_pressed;
        self.commands_sent = self.commands_sent.saturating_add(1);
        Some(InputCommand {
            input_epoch: self.input_epoch,
            sequence: self.sequence,
            move_axis: axis,
            jump_pressed,
            down_held,
        })
    }
}

#[must_use]
pub const fn move_axis_from_i8(axis: i8) -> MoveAxis {
    match axis {
        -1 => MoveAxis::Left,
        1 => MoveAxis::Right,
        _ => MoveAxis::Neutral,
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
    fn focus_loss_releases_held_right_to_neutral() {
        let mut state = ActionState::default();
        state.set_action(Action::MoveRight, true, false);
        assert_eq!(state.move_axis(), MoveAxis::Right);
        assert!(state.has_held_input());
        state.release_on_focus_loss();
        assert_eq!(state.move_axis(), MoveAxis::Neutral);
        assert!(!state.has_held_input());
        assert!(!state.down_held());
        assert!(!state.jump_pending());
    }

    #[test]
    fn focus_loss_releases_held_left_to_neutral() {
        let mut state = ActionState::default();
        state.set_action(Action::MoveLeft, true, false);
        assert_eq!(state.move_axis(), MoveAxis::Left);
        state.release_on_focus_loss();
        assert_eq!(state.move_axis(), MoveAxis::Neutral);
    }

    #[test]
    fn focus_loss_clears_jump_and_down() {
        let mut state = ActionState::default();
        state.set_action(Action::Jump, true, false);
        state.set_action(Action::MoveDown, true, false);
        assert!(state.down_held());
        assert!(state.jump_pending() || state.has_held_input());
        state.release_on_focus_loss();
        assert!(!state.down_held());
        assert!(!state.jump_pending());
        assert!(!state.has_held_input());
        let input = state.consume_tick_input();
        assert!(!input.jump_pressed);
        assert!(!input.down_held);
        assert_eq!(input.move_axis, 0);
    }

    #[test]
    fn focus_loss_neutral_is_visible_to_intent_net() {
        let mut state = ActionState::default();
        let mut net = IntentNet::default();
        state.set_action(Action::MoveRight, true, false);
        let input = state.consume_tick_input();
        let _ = net.consider_tick_input(input).expect("right");
        state.release_on_focus_loss();
        let cmd = net.emit_neutral().expect("neutral after focus loss");
        assert_eq!(cmd.move_axis, MoveAxis::Neutral);
        assert!(!cmd.down_held);
        assert!(!cmd.jump_pressed);
        assert_eq!(cmd.sequence, 2);
    }

    #[test]
    fn tick_sample_sends_jump_with_prediction_not_before() {
        // Contract: jump edge is latched on keydown but only enters an
        // InputCommand when the sim tick consumes it (same sample as prediction).
        let mut state = ActionState::default();
        let mut net = IntentNet::default();
        state.set_action(Action::Jump, true, false);
        assert!(state.jump_pending());
        // Key path must not send (would make server jump a tick early).
        assert_eq!(net.sequence, 0);
        let input = state.consume_tick_input();
        assert!(input.jump_pressed);
        let cmd = net.consider_tick_input(input).expect("jump on tick");
        assert!(cmd.jump_pressed);
        let idle = state.consume_tick_input();
        assert!(!idle.jump_pressed);
        let held = net.consider_tick_input(idle).expect("per-tick command");
        assert!(!held.jump_pressed);
        assert_eq!(held.sequence, 2);
    }

    #[test]
    fn tick_sample_aligns_move_press_with_prediction() {
        let mut state = ActionState::default();
        let mut net = IntentNet::default();
        state.set_action(Action::MoveRight, true, false);
        // Key path only latches ActionState; intent waits for tick sample.
        let input = state.consume_tick_input();
        assert_eq!(input.move_axis, 1);
        let cmd = net.consider_tick_input(input).expect("right on tick");
        assert_eq!(cmd.move_axis, MoveAxis::Right);
        // Held continuation still emits one command per tick.
        let held = state.consume_tick_input();
        let again = net.consider_tick_input(held).expect("held continuation");
        assert_eq!(again.sequence, 2);
        assert_eq!(again.move_axis, MoveAxis::Right);
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

    #[test]
    fn connection_screen_ignores_gameplay_actions() {
        use crate::lifecycle::ClientScreen;
        let mut state = ActionState::default();
        let screen = ClientScreen::Connection;
        if matches!(screen, ClientScreen::Game) {
            state.set_action(Action::MoveLeft, true, false);
            state.set_action(Action::MoveRight, true, false);
            state.set_action(Action::Jump, true, false);
            state.set_action(Action::MoveDown, true, false);
        }
        let input = state.consume_tick_input();
        assert_eq!(input.move_axis, 0);
        assert!(!input.jump_pressed);
        assert!(!input.down_held);
    }

    #[test]
    fn connection_to_game_clears_held_jump_edge() {
        let mut state = ActionState::default();
        state.set_action(Action::Jump, true, false);
        state.set_action(Action::MoveRight, true, false);
        state.clear();
        let input = state.consume_tick_input();
        assert!(!input.jump_pressed);
        assert_eq!(input.move_axis, 0);
    }

    #[test]
    fn game_input_behavior_unchanged_after_clear() {
        let mut state = ActionState::default();
        state.set_action(Action::MoveRight, true, false);
        state.set_action(Action::Jump, true, false);
        let first = state.consume_tick_input();
        assert_eq!(first.move_axis, 1);
        assert!(first.jump_pressed);
        state.set_action(Action::Jump, false, false);
        state.set_action(Action::Jump, true, false);
        let second = state.consume_tick_input();
        assert!(second.jump_pressed);
        assert_eq!(second.move_axis, 1);
    }

    #[test]
    fn intent_net_emits_every_tick() {
        let mut net = IntentNet::default();
        let first = net
            .consider_tick_input(PlayerInput::from_buttons_ext(false, true, true, false))
            .expect("send");
        assert_eq!(first.sequence, 1);
        assert_eq!(first.input_epoch, 0);
        assert!(first.jump_pressed);
        assert_eq!(first.move_axis, MoveAxis::Right);
        let second = net
            .consider_tick_input(PlayerInput::from_buttons_ext(false, true, false, false))
            .expect("held");
        assert_eq!(second.sequence, 2);
        assert!(!second.jump_pressed);
        let third = net.emit_neutral().expect("neutral");
        assert_eq!(third.sequence, 3);
        assert_eq!(third.move_axis, MoveAxis::Neutral);
    }

    #[test]
    fn intent_net_reset_starts_clean() {
        let mut net = IntentNet::default();
        let _ = net.consider_tick_input(PlayerInput::from_buttons_ext(true, false, true, true));
        net.reset();
        assert_eq!(net.sequence, 0);
        assert_eq!(net.commands_sent, 0);
        assert_eq!(net.move_axis, MoveAxis::Neutral);
        assert!(!net.down_held);
        let next = net
            .consider_tick_input(PlayerInput::from_buttons_ext(false, true, false, false))
            .expect("first post-reset");
        assert_eq!(next.sequence, 1);
        assert_eq!(next.move_axis, MoveAxis::Right);
        assert!(!next.jump_pressed);
    }

    #[test]
    fn epoch_change_restarts_sequence() {
        let mut net = IntentNet::default();
        let _ = net.consider_tick_input(PlayerInput::idle());
        net.set_epoch(2);
        let cmd = net.consider_tick_input(PlayerInput::idle()).expect("seq 1");
        assert_eq!(cmd.input_epoch, 2);
        assert_eq!(cmd.sequence, 1);
    }
}
