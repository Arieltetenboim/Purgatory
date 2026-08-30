//! Bot behavior profiles and action generation.

use clap::ValueEnum;
use purgatory_protocol::MoveAxis;

use crate::prng::Lcg;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BotAction {
    Move { axis: MoveAxis },
    Jump,
    DropThrough,
    Idle,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    ValueEnum,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum BotProfile {
    #[default]
    Idle,
    Walker,
    Jumper,
    Mixed,
}

impl BotProfile {
    pub fn hash(self) -> u32 {
        match self {
            Self::Idle => 0,
            Self::Walker => 1,
            Self::Jumper => 2,
            Self::Mixed => 3,
        }
    }
}

pub struct BotBehavior {
    profile: BotProfile,
    rng: Lcg,
    current_direction: MoveAxis,
    ticks_until_direction_change: u32,
    last_grounded: bool,
    received_first_snapshot: bool,
}

impl BotBehavior {
    pub fn new(profile: BotProfile, seed: u64, bot_id: u32) -> Self {
        let mut rng = Lcg::seeded(seed, bot_id, profile.hash());
        let current_direction = if rng.next_bool(0.5) {
            MoveAxis::Left
        } else {
            MoveAxis::Right
        };
        let ticks_until_direction_change = rng.next_range(30, 150);
        Self {
            profile,
            rng,
            current_direction,
            ticks_until_direction_change,
            last_grounded: false,
            received_first_snapshot: false,
        }
    }

    pub fn on_snapshot(&mut self, local_grounded: bool) {
        self.received_first_snapshot = true;
        self.last_grounded = local_grounded;
    }

    pub fn next_action(&mut self) -> BotAction {
        match self.profile {
            BotProfile::Idle => BotAction::Idle,
            BotProfile::Walker => self.walker_action(),
            BotProfile::Jumper => self.jumper_action(),
            BotProfile::Mixed => self.mixed_action(),
        }
    }

    pub fn action_to_input(&self, action: BotAction) -> (MoveAxis, bool, bool) {
        match action {
            BotAction::Idle => (MoveAxis::Neutral, false, false),
            BotAction::Move { axis } => (axis, false, false),
            BotAction::Jump => (self.current_direction, true, false),
            BotAction::DropThrough => (MoveAxis::Neutral, false, true),
        }
    }

    fn walker_action(&mut self) -> BotAction {
        self.maybe_change_direction();
        BotAction::Move {
            axis: self.current_direction,
        }
    }

    fn jumper_action(&mut self) -> BotAction {
        if !self.received_first_snapshot {
            return BotAction::Idle;
        }
        self.maybe_change_direction();
        if self.last_grounded && self.rng.next_bool(0.15) {
            BotAction::Jump
        } else {
            BotAction::Move {
                axis: self.current_direction,
            }
        }
    }

    fn mixed_action(&mut self) -> BotAction {
        if !self.received_first_snapshot {
            return BotAction::Idle;
        }
        self.maybe_change_direction();
        let roll = self.rng.next_u32() % 100;
        if roll < 40 {
            BotAction::Idle
        } else if roll < 75 {
            BotAction::Move {
                axis: self.current_direction,
            }
        } else if roll < 90 && self.last_grounded {
            BotAction::Jump
        } else if roll < 95 && self.last_grounded {
            BotAction::DropThrough
        } else {
            let new_dir = if self.rng.next_bool(0.5) {
                MoveAxis::Left
            } else {
                MoveAxis::Right
            };
            BotAction::Move { axis: new_dir }
        }
    }

    fn maybe_change_direction(&mut self) {
        if self.ticks_until_direction_change > 0 {
            self.ticks_until_direction_change -= 1;
        } else {
            self.current_direction = match self.current_direction {
                MoveAxis::Left => MoveAxis::Right,
                MoveAxis::Right => MoveAxis::Left,
                MoveAxis::Neutral => {
                    if self.rng.next_bool(0.5) {
                        MoveAxis::Left
                    } else {
                        MoveAxis::Right
                    }
                }
            };
            self.ticks_until_direction_change = self.rng.next_range(30, 150);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = BotBehavior::new(BotProfile::Walker, 42, 1);
        let mut b = BotBehavior::new(BotProfile::Walker, 42, 1);
        for _ in 0..100 {
            assert_eq!(a.next_action(), b.next_action());
        }
    }

    #[test]
    fn different_bot_id_different_sequence() {
        let mut a = BotBehavior::new(BotProfile::Walker, 42, 1);
        let mut b = BotBehavior::new(BotProfile::Walker, 42, 2);
        let mut same = 0;
        for _ in 0..100 {
            if a.next_action() == b.next_action() {
                same += 1;
            }
        }
        assert!(same < 90, "sequences should mostly differ");
    }

    #[test]
    fn idle_always_idle() {
        let mut b = BotBehavior::new(BotProfile::Idle, 0, 0);
        for _ in 0..100 {
            assert_eq!(b.next_action(), BotAction::Idle);
        }
    }

    #[test]
    fn walker_never_idle() {
        let mut b = BotBehavior::new(BotProfile::Walker, 12345, 1);
        for _ in 0..100 {
            let action = b.next_action();
            assert!(matches!(action, BotAction::Move { .. }));
        }
    }

    #[test]
    fn jumper_idles_until_first_snapshot() {
        let mut b = BotBehavior::new(BotProfile::Jumper, 12345, 1);
        assert_eq!(b.next_action(), BotAction::Idle);
        b.on_snapshot(true);
        let action = b.next_action();
        assert!(
            !matches!(action, BotAction::Idle)
                || matches!(action, BotAction::Move { .. } | BotAction::Jump)
        );
    }

    #[test]
    fn one_command_per_tick() {
        let mut b = BotBehavior::new(BotProfile::Mixed, 54321, 5);
        b.on_snapshot(true);
        for _ in 0..1000 {
            let _action = b.next_action();
        }
    }

    #[test]
    fn jump_edge_not_sticky() {
        let mut b = BotBehavior::new(BotProfile::Jumper, 99999, 1);
        b.on_snapshot(true);
        let mut consecutive_jumps = 0;
        let mut max_consecutive = 0;
        for _ in 0..1000 {
            let action = b.next_action();
            if matches!(action, BotAction::Jump) {
                consecutive_jumps += 1;
                max_consecutive = max_consecutive.max(consecutive_jumps);
            } else {
                consecutive_jumps = 0;
            }
        }
        assert!(
            max_consecutive < 50,
            "jump should not be sticky, max consecutive: {max_consecutive}"
        );
    }

    #[test]
    fn sequence_increments() {
        let mut b = BotBehavior::new(BotProfile::Walker, 100, 1);
        let mut intent = purgatory_protocol::IntentNet::default();
        for expected_seq in 1..=100 {
            let action = b.next_action();
            let (axis, jump, down) = b.action_to_input(action);
            let cmd = intent.emit_tick(axis, jump, down).expect("emit");
            assert_eq!(cmd.sequence, expected_seq);
        }
    }
}
