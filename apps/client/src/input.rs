//! Semantic actions. Keyboard scancodes stay in this client module.

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use purgatory_protocol::MoveAxis;
#[cfg(test)]
use purgatory_protocol::{InputCommand, move_axis_from_i8};
use purgatory_simulation::PlayerInput;

pub use purgatory_protocol::IntentNet;

/// Gameplay action. Simulation never sees `KeyCode`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    MoveLeft,
    MoveRight,
    MoveDown,
    Jump,
    Interact,
    ActivatePortal,
    BasicStrike,
}

/// Held buttons plus a jump edge queued for the next simulation tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct ActionState {
    move_left: bool,
    move_right: bool,
    move_down: bool,
    jump_held: bool,
    jump_edge: bool,
    interact_edge: bool,
    portal_edge: bool,
    portal_held: bool,
    ability_edge: bool,
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
            Action::Interact => {
                if repeat {
                    return;
                }
                if pressed {
                    self.interact_edge = true;
                }
            }
            Action::ActivatePortal => {
                if repeat {
                    return;
                }
                if pressed {
                    if !self.portal_held {
                        self.portal_edge = true;
                    }
                    self.portal_held = true;
                } else {
                    self.portal_held = false;
                }
            }
            Action::BasicStrike => {
                if repeat {
                    return;
                }
                if pressed {
                    self.ability_edge = true;
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
        self.interact_edge = false;
        self.portal_edge = false;
        self.portal_held = false;
        self.ability_edge = false;
    }

    /// Drop jump/interact/portal edges without releasing held movement.
    /// Used while a transition input barrier is active so discrete actions
    /// cannot replay on unlock.
    pub fn discard_locked_edges(&mut self) {
        self.jump_edge = false;
        self.interact_edge = false;
        self.portal_edge = false;
        self.ability_edge = false;
    }

    /// Edge-triggered interact. Not movement. Not authoritative eligibility.
    #[must_use]
    pub fn consume_interact_edge(&mut self) -> bool {
        let edge = self.interact_edge;
        self.interact_edge = false;
        edge
    }

    /// Edge-triggered Basic Strike. Intent only; server owns hit/query.
    #[must_use]
    pub fn consume_ability_edge(&mut self) -> bool {
        let edge = self.ability_edge;
        self.ability_edge = false;
        edge
    }

    /// Edge-triggered portal activate (Up Arrow). Holding does not retrigger.
    #[must_use]
    pub fn consume_portal_edge(&mut self) -> bool {
        let edge = self.portal_edge;
        self.portal_edge = false;
        edge
    }

    #[must_use]
    pub fn portal_held(&self) -> bool {
        self.portal_held
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

/// Client adapter: map simulation [`PlayerInput`] onto shared [`IntentNet`].
#[cfg(test)]
pub trait IntentNetExt {
    fn consider_tick_input(&mut self, input: PlayerInput) -> Option<InputCommand>;
}

#[cfg(test)]
impl IntentNetExt for IntentNet {
    fn consider_tick_input(&mut self, input: PlayerInput) -> Option<InputCommand> {
        self.emit_tick(
            move_axis_from_i8(input.move_axis),
            input.jump_pressed,
            input.down_held,
        )
    }
}

#[must_use]
pub fn map_key(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::KeyA | KeyCode::ArrowLeft => Some(Action::MoveLeft),
        KeyCode::KeyD | KeyCode::ArrowRight => Some(Action::MoveRight),
        KeyCode::KeyS | KeyCode::ArrowDown => Some(Action::MoveDown),
        KeyCode::Space => Some(Action::Jump),
        KeyCode::KeyE => Some(Action::Interact),
        KeyCode::KeyJ => Some(Action::BasicStrike),
        KeyCode::ArrowUp => Some(Action::ActivatePortal),
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
        assert_eq!(map_key(KeyCode::KeyE), Some(Action::Interact));
        assert_eq!(map_key(KeyCode::KeyJ), Some(Action::BasicStrike));
        assert_eq!(map_key(KeyCode::ArrowUp), Some(Action::ActivatePortal));
        assert_eq!(map_key(KeyCode::KeyW), None);
        assert_eq!(map_key(KeyCode::Backquote), None);
    }

    #[test]
    fn interact_edge_fires_once_until_consumed() {
        let mut state = ActionState::default();
        state.set_action(Action::Interact, true, false);
        state.set_action(Action::Interact, true, true);
        assert!(state.consume_interact_edge());
        assert!(!state.consume_interact_edge());
        state.set_action(Action::Interact, false, false);
        assert!(!state.consume_interact_edge());
        state.set_action(Action::Interact, true, false);
        assert!(state.consume_interact_edge());
    }

    #[test]
    fn ability_edge_fires_once_until_consumed() {
        let mut state = ActionState::default();
        state.set_action(Action::BasicStrike, true, false);
        state.set_action(Action::BasicStrike, true, true);
        assert!(state.consume_ability_edge());
        assert!(!state.consume_ability_edge());
        state.set_action(Action::BasicStrike, false, false);
        assert!(!state.consume_ability_edge());
        state.set_action(Action::BasicStrike, true, false);
        assert!(state.consume_ability_edge());
    }

    #[test]
    fn portal_held_does_not_retrigger_until_release() {
        let mut state = ActionState::default();
        state.set_action(Action::ActivatePortal, true, false);
        assert!(state.consume_portal_edge());
        assert!(state.portal_held());
        state.set_action(Action::ActivatePortal, true, false);
        assert!(
            !state.consume_portal_edge(),
            "held Up must not synthesize another edge"
        );
        state.set_action(Action::ActivatePortal, false, false);
        assert!(!state.portal_held());
        assert!(!state.consume_portal_edge());
        state.set_action(Action::ActivatePortal, true, false);
        assert!(state.consume_portal_edge());
    }

    #[test]
    fn consume_tick_input_does_not_eat_interact_edge() {
        let mut state = ActionState::default();
        state.set_action(Action::Interact, true, false);
        let _ = state.consume_tick_input();
        assert!(state.consume_interact_edge());
    }

    #[test]
    fn consume_tick_input_does_not_eat_portal_edge() {
        let mut state = ActionState::default();
        state.set_action(Action::ActivatePortal, true, false);
        let _ = state.consume_tick_input();
        assert!(state.consume_portal_edge());
    }

    #[test]
    fn jump_edge_fires_once_per_press() {
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
    fn discard_locked_edges_keeps_held_movement() {
        let mut state = ActionState::default();
        state.set_action(Action::MoveRight, true, false);
        state.set_action(Action::Jump, true, false);
        state.set_action(Action::Interact, true, false);
        state.set_action(Action::ActivatePortal, true, false);
        state.discard_locked_edges();
        let input = state.consume_tick_input();
        assert_eq!(input.move_axis, 1);
        assert!(!input.jump_pressed, "locked jump must not replay");
        assert!(!state.consume_interact_edge());
        assert!(!state.consume_portal_edge());
        assert!(state.portal_held());
        let held = state.consume_tick_input();
        assert_eq!(held.move_axis, 1, "held Right must resume after unlock");
    }

    #[test]
    fn discard_locked_edges_does_not_arm_held_up() {
        let mut state = ActionState::default();
        state.set_action(Action::ActivatePortal, true, false);
        assert!(state.consume_portal_edge());
        state.discard_locked_edges();
        assert!(
            !state.consume_portal_edge(),
            "held Up across a barrier must not synthesize an edge"
        );
        state.set_action(Action::ActivatePortal, false, false);
        state.set_action(Action::ActivatePortal, true, false);
        assert!(state.consume_portal_edge());
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

    #[cfg(feature = "dev-diagnostics")]
    #[test]
    fn overlay_open_policy_keeps_held_movement() {
        let mut state = ActionState::default();
        state.set_action(Action::MoveRight, true, false);
        assert!(crate::debug::gameplay_receives_keyboard(true, false, true));
        assert_eq!(state.consume_tick_input().move_axis, 1);
    }

    #[cfg(feature = "dev-diagnostics")]
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
