//! Privileged mutating debug commands. Not read-only diagnostics. Not protocol.

use super::ui_state::DebugUiState;
use crate::display::{RenderScale, Resolution};
use crate::renderer::WorldMsaa;

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
    SetResolution(Resolution),
    SetRenderScale(RenderScale),
    SetWorldMsaa(WorldMsaa),
    PresentationAttack,
    PresentationHurt,
    Equip(&'static str),
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
}
