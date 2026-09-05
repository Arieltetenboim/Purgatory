//! Compact Debug chrome policy. Presentation only; not diagnostics ownership.

use super::ui_state::DebugUiState;
use crate::jitter_forensics::CameraJitterMode;
use crate::network::ImpairmentProfile;
use crate::ui_runtime::InteractKind;

/// Persistent warning for a dev mode that survives overlay close.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevWarning {
    pub label: String,
    pub rgb: (u8, u8, u8),
}

#[must_use]
pub fn time_scale_abnormal(scale: f32) -> bool {
    (scale - 1.0).abs() > 1e-4
}

#[must_use]
pub fn interaction_chrome_visible(kind: InteractKind, flash_live: bool) -> bool {
    kind != InteractKind::Idle || flash_live
}

#[must_use]
pub fn target_chrome_visible(id: Option<&str>, _distance: Option<f32>) -> bool {
    id.is_some()
}

#[must_use]
pub fn portal_chrome_visible(id: Option<&str>, eligible: bool) -> bool {
    id.is_some() || eligible
}

#[must_use]
pub fn transition_chrome_visible(banner: &str) -> bool {
    !banner.is_empty()
}

#[must_use]
pub fn input_gate_chrome_visible(locked: bool) -> bool {
    locked
}

/// Modes that stay on [`DebugUiState`] after the overlay is closed.
#[must_use]
pub fn persistent_dev_warnings(ui: &DebugUiState) -> Vec<DevWarning> {
    let mut out = Vec::new();
    if time_scale_abnormal(ui.time_scale) {
        out.push(DevWarning {
            label: format!("TIME {}", ui.time_scale_label()),
            rgb: (255, 180, 60),
        });
    }
    if ui.camera_jitter_mode != CameraJitterMode::Normal {
        out.push(DevWarning {
            label: format!("JITTER {}", ui.camera_jitter_mode.as_str()),
            rgb: (255, 120, 90),
        });
    }
    if ui.impairment_profile != ImpairmentProfile::Off {
        out.push(DevWarning {
            label: format!("IMPAIR {}", ui.impairment_profile.as_str()),
            rgb: (220, 90, 255),
        });
    }
    if ui.presentation_view_back {
        out.push(DevWarning {
            label: "FORCE BACK".into(),
            rgb: (90, 200, 255),
        });
    }
    if ui.presentation_force_climb_back {
        out.push(DevWarning {
            label: "FORCE CLIMBBACK".into(),
            rgb: (90, 200, 255),
        });
    }
    if ui.show_rf_scene {
        out.push(DevWarning {
            label: "RF0 SCENE".into(),
            rgb: (120, 210, 90),
        });
    }
    if ui.show_rf_ab {
        out.push(DevWarning {
            label: "RF A/B".into(),
            rgb: (120, 210, 90),
        });
    }
    if ui.rf_freeze_camera {
        out.push(DevWarning {
            label: "RF CAM FREEZE".into(),
            rgb: (255, 120, 90),
        });
    }
    out
}

#[must_use]
pub fn has_persistent_dev_warnings(ui: &DebugUiState) -> bool {
    time_scale_abnormal(ui.time_scale)
        || ui.camera_jitter_mode != CameraJitterMode::Normal
        || ui.impairment_profile != ImpairmentProfile::Off
        || ui.presentation_view_back
        || ui.presentation_force_climb_back
        || ui.show_rf_scene
        || ui.show_rf_ab
        || ui.rf_freeze_camera
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_simulation::INTERACT_RANGE;

    #[test]
    fn idle_chrome_hides_interaction_target_portal_and_gate() {
        // Live-exception flags only. Overlay still reserves Interact/Target/Portal
        // rows at idle (NONE) so show/hide does not jump the Debug window.
        assert!(!interaction_chrome_visible(InteractKind::Idle, false));
        assert!(!target_chrome_visible(None, None));
        assert!(!portal_chrome_visible(None, false));
        assert!(!transition_chrome_visible(""));
        assert!(!input_gate_chrome_visible(false));
    }

    #[test]
    fn exceptions_show_interaction_target_portal_and_gate() {
        assert!(interaction_chrome_visible(InteractKind::Open, false));
        assert!(interaction_chrome_visible(InteractKind::Idle, true));
        assert!(target_chrome_visible(Some("12:1"), Some(0.4)));
        assert!(target_chrome_visible(
            Some("12:1"),
            Some(INTERACT_RANGE + 1.0)
        ));
        assert!(portal_chrome_visible(Some("9:1"), false));
        assert!(portal_chrome_visible(None, true));
        assert!(transition_chrome_visible("FADE"));
        assert!(input_gate_chrome_visible(true));
    }

    #[test]
    fn default_ui_has_no_persistent_warnings() {
        let ui = DebugUiState::default();
        assert!(!has_persistent_dev_warnings(&ui));
        assert!(persistent_dev_warnings(&ui).is_empty());
    }

    #[test]
    fn time_jitter_impair_and_proof_emit_warnings() {
        let mut ui = DebugUiState {
            time_scale: 0.5,
            camera_jitter_mode: CameraJitterMode::CameraFrozen,
            impairment_profile: ImpairmentProfile::Lan,
            presentation_view_back: true,
            presentation_force_climb_back: true,
            ..Default::default()
        };
        let labels: Vec<_> = persistent_dev_warnings(&ui)
            .into_iter()
            .map(|w| w.label)
            .collect();
        assert!(labels.iter().any(|l| l.contains("TIME")));
        assert!(labels.iter().any(|l| l.contains("JITTER")));
        assert!(labels.iter().any(|l| l.contains("IMPAIR")));
        assert!(labels.iter().any(|l| l.contains("FORCE BACK")));
        assert!(labels.iter().any(|l| l.contains("FORCE CLIMBBACK")));
        ui.time_scale = 1.0;
        ui.camera_jitter_mode = CameraJitterMode::Normal;
        ui.impairment_profile = ImpairmentProfile::Off;
        ui.presentation_view_back = false;
        ui.presentation_force_climb_back = false;
        assert!(!has_persistent_dev_warnings(&ui));
        ui.show_rf_ab = true;
        let labels: Vec<_> = persistent_dev_warnings(&ui)
            .into_iter()
            .map(|w| w.label)
            .collect();
        assert!(labels.iter().any(|l| l == "RF A/B"));
        assert!(has_persistent_dev_warnings(&ui));
    }
}
