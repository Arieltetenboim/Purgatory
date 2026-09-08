//! Per-tick client → server command sequencer.
//!
//! Sequence starts at 1 each epoch. One [`InputCommand`] per predicted
//! simulation step. No send-on-change, no refresh coalescing. Shared by the
//! native client and headless load bots so epoch/sequence rules cannot drift.

use crate::{InputCommand, MoveAxis};

/// Client → server per-tick command sequencer.
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
    pub fn emit_tick(
        &mut self,
        axis: MoveAxis,
        jump_pressed: bool,
        down_held: bool,
    ) -> Option<InputCommand> {
        self.emit(axis, jump_pressed, down_held, false)
    }

    /// Same as [`Self::emit_tick`], including Up-held latch state for portal reentry.
    #[must_use]
    pub fn emit_tick_with_portal(
        &mut self,
        axis: MoveAxis,
        jump_pressed: bool,
        down_held: bool,
        portal_held: bool,
    ) -> Option<InputCommand> {
        self.emit(axis, jump_pressed, down_held, portal_held)
    }

    /// Forced Neutral command paired with a `SimulationClock` step (focus-loss).
    #[must_use]
    pub fn emit_neutral(&mut self) -> Option<InputCommand> {
        self.emit(MoveAxis::Neutral, false, false, false)
    }

    fn emit(
        &mut self,
        axis: MoveAxis,
        jump_pressed: bool,
        down_held: bool,
        portal_held: bool,
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
            portal_held,
        })
    }
}

/// Map simulation `move_axis` i8 (−1 / 0 / 1) onto the wire enum.
#[must_use]
pub const fn move_axis_from_i8(axis: i8) -> MoveAxis {
    match axis {
        -1 => MoveAxis::Left,
        1 => MoveAxis::Right,
        _ => MoveAxis::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_net_emits_every_tick() {
        let mut net = IntentNet::default();
        let first = net.emit_tick(MoveAxis::Right, true, false).expect("send");
        assert_eq!(first.sequence, 1);
        assert_eq!(first.input_epoch, 0);
        assert!(first.jump_pressed);
        assert_eq!(first.move_axis, MoveAxis::Right);
        let second = net.emit_tick(MoveAxis::Right, false, false).expect("held");
        assert_eq!(second.sequence, 2);
        assert!(!second.jump_pressed);
        let third = net.emit_neutral().expect("neutral");
        assert_eq!(third.sequence, 3);
        assert_eq!(third.move_axis, MoveAxis::Neutral);
    }

    #[test]
    fn intent_net_reset_starts_clean() {
        let mut net = IntentNet::default();
        let _ = net.emit_tick(MoveAxis::Left, true, true);
        net.reset();
        assert_eq!(net.sequence, 0);
        assert_eq!(net.commands_sent, 0);
        assert_eq!(net.move_axis, MoveAxis::Neutral);
        assert!(!net.down_held);
        let next = net
            .emit_tick(MoveAxis::Right, false, false)
            .expect("first post-reset");
        assert_eq!(next.sequence, 1);
        assert_eq!(next.move_axis, MoveAxis::Right);
        assert!(!next.jump_pressed);
    }

    #[test]
    fn epoch_change_restarts_sequence() {
        let mut net = IntentNet::default();
        let _ = net.emit_tick(MoveAxis::Neutral, false, false);
        net.set_epoch(2);
        let cmd = net
            .emit_tick(MoveAxis::Neutral, false, false)
            .expect("seq 1");
        assert_eq!(cmd.input_epoch, 2);
        assert_eq!(cmd.sequence, 1);
    }

    #[test]
    fn move_axis_from_i8_maps() {
        assert_eq!(move_axis_from_i8(-1), MoveAxis::Left);
        assert_eq!(move_axis_from_i8(0), MoveAxis::Neutral);
        assert_eq!(move_axis_from_i8(1), MoveAxis::Right);
        assert_eq!(move_axis_from_i8(99), MoveAxis::Neutral);
    }
}
