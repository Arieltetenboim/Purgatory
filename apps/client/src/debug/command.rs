//! Privileged mutating debug commands. Not read-only diagnostics. Not protocol.

use super::ui_state::{
    DEBUG_JUMP_SPEED_DEFAULT, DEBUG_MOVE_SPEED_DEFAULT, DEBUG_TUNING_SEND_INTERVAL, DebugUiState,
    sanitize_debug_jump_speed, sanitize_debug_move_speed,
};
use crate::display::{RenderScale, Resolution};
use crate::renderer::WorldMsaa;
use std::time::Instant;

/// Client-owned DEV commands. `ClientApp` executes these; overlay widgets enqueue them.
///
/// Simulation `DebugAction` stays in `purgatory-simulation`. This enum is the
/// client-level set (display, network DEV envelopes, prediction reanchor, …).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugCommand {
    /// Client-only prediction reanchor to the replica. Not a spawn reset.
    ResetPlayer,
    /// Connected: `DevResetPlayer`. Offline: local `DebugAction::ResetPlayer`.
    ResetToSpawn,
    Respawn,
    Connect,
    Disconnect,
    SetChannel(u32),
    SetMoveSpeed(Option<u16>),
    SetJumpSpeed(Option<u16>),
    /// Request a server-authoritative transient NPC at the current player pose.
    SpawnNpc(purgatory_common::ContentId),
    SetResolution(Resolution),
    SetRenderScale(RenderScale),
    SetWorldMsaa(WorldMsaa),
    PresentationAttack,
    PresentationHurt,
    Equip(&'static str),
    EquipItem(purgatory_common::ItemInstanceId),
    UnequipSlot(u8),
    UnequipAll,
    AnimationPlay,
    AnimationPause,
    AnimationReset,
    DumpJitterTrace,
    ImpairmentStall {
        ms: u32,
    },
    ResetImpairmentMetrics,
    ClearNetworkHistory,
    CenterOnPlayer,
}

impl DebugUiState {
    /// Take one-shot privileged requests. Persistent view toggles stay on `self`.
    pub fn drain_commands(&mut self) -> Vec<DebugCommand> {
        self.drain_commands_at(Instant::now())
    }

    fn drain_commands_at(&mut self, now: Instant) -> Vec<DebugCommand> {
        let mut out = Vec::new();
        if self.network_connect {
            self.network_connect = false;
            out.push(DebugCommand::Connect);
        }
        if self.network_disconnect {
            self.network_disconnect = false;
            out.push(DebugCommand::Disconnect);
        }
        if let Some(channel) = self.request_channel.take() {
            out.push(DebugCommand::SetChannel(channel));
        }
        let mut tuning_sent = false;
        if self.debug_move_speed_reset_requested {
            self.debug_move_speed_reset_requested = false;
            self.last_sent_debug_move_speed = Some(DEBUG_MOVE_SPEED_DEFAULT);
            out.push(DebugCommand::SetMoveSpeed(None));
            tuning_sent = true;
        }
        if self.debug_jump_speed_reset_requested {
            self.debug_jump_speed_reset_requested = false;
            self.last_sent_debug_jump_speed = Some(DEBUG_JUMP_SPEED_DEFAULT);
            out.push(DebugCommand::SetJumpSpeed(None));
            tuning_sent = true;
        }

        let tuning_due = self
            .debug_tuning_next_send_at
            .is_none_or(|deadline| now >= deadline);
        if !tuning_sent && tuning_due {
            let speed = sanitize_debug_move_speed(self.debug_move_speed);
            let jump = sanitize_debug_jump_speed(self.debug_jump_speed);
            let speed_pending = self.last_sent_debug_move_speed != Some(speed);
            let jump_pending = self.last_sent_debug_jump_speed != Some(jump);
            let send_speed = speed_pending && (!jump_pending || self.debug_tuning_send_speed_next);
            let send_jump = jump_pending && !send_speed;
            if send_speed {
                self.last_sent_debug_move_speed = Some(speed);
                self.debug_tuning_send_speed_next = false;
                out.push(DebugCommand::SetMoveSpeed(Some(
                    (speed * 100.0).round() as u16
                )));
                tuning_sent = true;
            } else if send_jump {
                self.last_sent_debug_jump_speed = Some(jump);
                self.debug_tuning_send_speed_next = true;
                out.push(DebugCommand::SetJumpSpeed(Some(
                    (jump * 100.0).round() as u16
                )));
                tuning_sent = true;
            }
        }
        if tuning_sent {
            self.debug_tuning_next_send_at = Some(now + DEBUG_TUNING_SEND_INTERVAL);
        }
        if let Some(resolution) = self.requested_resolution.take() {
            out.push(DebugCommand::SetResolution(resolution));
        }
        if let Some(scale) = self.requested_render_scale.take() {
            out.push(DebugCommand::SetRenderScale(scale));
        }
        if let Some(msaa) = self.requested_world_msaa.take() {
            out.push(DebugCommand::SetWorldMsaa(msaa));
        }
        if self.request_presentation_attack {
            self.request_presentation_attack = false;
            out.push(DebugCommand::PresentationAttack);
        }
        if self.request_presentation_hurt {
            self.request_presentation_hurt = false;
            out.push(DebugCommand::PresentationHurt);
        }
        if let Some(authored) = self.request_debug_equip.take() {
            out.push(DebugCommand::Equip(authored));
        }
        if let Some(item) = self.request_debug_equip_item.take() {
            out.push(DebugCommand::EquipItem(item));
        }
        if let Some(slot) = self.request_debug_unequip_slot.take() {
            out.push(DebugCommand::UnequipSlot(slot));
        }
        if self.request_debug_unequip_all {
            self.request_debug_unequip_all = false;
            out.push(DebugCommand::UnequipAll);
        }
        if self.request_animation_play {
            self.request_animation_play = false;
            out.push(DebugCommand::AnimationPlay);
        }
        if self.request_animation_pause {
            self.request_animation_pause = false;
            out.push(DebugCommand::AnimationPause);
        }
        if self.request_animation_reset {
            self.request_animation_reset = false;
            out.push(DebugCommand::AnimationReset);
        }
        if self.dump_jitter_trace {
            self.dump_jitter_trace = false;
            out.push(DebugCommand::DumpJitterTrace);
        }
        if let Some(ms) = self.impairment_stall_ms.take() {
            out.push(DebugCommand::ImpairmentStall { ms });
        }
        if self.reset_impairment_metrics {
            self.reset_impairment_metrics = false;
            out.push(DebugCommand::ResetImpairmentMetrics);
        }
        if self.clear_network_history {
            self.clear_network_history = false;
            out.push(DebugCommand::ClearNetworkHistory);
        }
        if self.center_on_player {
            self.center_on_player = false;
            out.push(DebugCommand::CenterOnPlayer);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_takes_oneshots_and_leaves_view_toggles() {
        let mut ui = DebugUiState {
            show_skeleton: true,
            time_scale: 0.5,
            network_connect: true,
            request_channel: Some(1),
            request_presentation_attack: true,
            center_on_player: true,
            requested_render_scale: Some(crate::display::RenderScale::P200),
            requested_world_msaa: Some(crate::renderer::WorldMsaa::Off),
            ..Default::default()
        };
        let commands = ui.drain_commands();
        assert!(commands.contains(&DebugCommand::Connect));
        assert!(commands.contains(&DebugCommand::SetChannel(1)));
        assert!(commands.contains(&DebugCommand::PresentationAttack));
        assert!(commands.contains(&DebugCommand::CenterOnPlayer));
        assert!(commands.contains(&DebugCommand::SetRenderScale(
            crate::display::RenderScale::P200
        )));
        assert!(commands.contains(&DebugCommand::SetWorldMsaa(crate::renderer::WorldMsaa::Off)));
        assert!(!ui.network_connect);
        assert!(ui.request_channel.is_none());
        assert!(!ui.request_presentation_attack);
        assert!(!ui.center_on_player);
        assert!(ui.requested_render_scale.is_none());
        assert!(ui.requested_world_msaa.is_none());
        assert!(ui.show_skeleton);
        assert!((ui.time_scale - 0.5).abs() < f32::EPSILON);
        assert!(ui.drain_commands().is_empty());
    }

    #[test]
    fn rapid_tuning_changes_are_throttled_and_latest_value_is_retained() {
        let t0 = Instant::now();
        let mut ui = DebugUiState {
            last_sent_debug_move_speed: Some(DEBUG_MOVE_SPEED_DEFAULT),
            last_sent_debug_jump_speed: Some(DEBUG_JUMP_SPEED_DEFAULT),
            debug_move_speed: 5.0,
            ..Default::default()
        };

        assert_eq!(
            ui.drain_commands_at(t0),
            vec![DebugCommand::SetMoveSpeed(Some(500))]
        );
        ui.debug_move_speed = 6.0;
        ui.debug_move_speed = 7.0;
        assert!(
            ui.drain_commands_at(t0 + std::time::Duration::from_millis(1))
                .is_empty()
        );
        assert_eq!(
            ui.drain_commands_at(t0 + DEBUG_TUNING_SEND_INTERVAL),
            vec![DebugCommand::SetMoveSpeed(Some(700))]
        );
    }

    #[test]
    fn speed_and_jump_share_the_same_cadence_policy() {
        let t0 = Instant::now();
        let mut ui = DebugUiState {
            last_sent_debug_move_speed: Some(DEBUG_MOVE_SPEED_DEFAULT),
            last_sent_debug_jump_speed: Some(DEBUG_JUMP_SPEED_DEFAULT),
            debug_move_speed: 5.0,
            debug_jump_speed: 14.0,
            ..Default::default()
        };

        assert_eq!(
            ui.drain_commands_at(t0),
            vec![DebugCommand::SetMoveSpeed(Some(500))]
        );
        assert_eq!(
            ui.drain_commands_at(t0 + DEBUG_TUNING_SEND_INTERVAL),
            vec![DebugCommand::SetJumpSpeed(Some(1_400))]
        );
    }

    #[test]
    fn reset_bypasses_cadence_and_is_retained_as_authoritative_command() {
        let t0 = Instant::now();
        let mut ui = DebugUiState {
            last_sent_debug_move_speed: Some(6.0),
            debug_move_speed: 4.0,
            ..Default::default()
        };
        assert_eq!(
            ui.drain_commands_at(t0),
            vec![DebugCommand::SetMoveSpeed(Some(400))]
        );
        ui.request_debug_move_speed_reset();
        assert_eq!(
            ui.drain_commands_at(t0 + std::time::Duration::from_millis(1)),
            vec![DebugCommand::SetMoveSpeed(None)]
        );
    }
}
