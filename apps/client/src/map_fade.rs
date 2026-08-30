//! Client-only transition presentation gate. Not an effects framework.
//!
//! Fade duration is presentation timing. FadeIn readiness is state-driven.
//! `Idle → FadeOut → Hold (black / waiting) → FadeIn → Idle`.
//! Simulation continues while the screen is obscured.

use crate::renderer::{Camera, DrawQuad};

/// Map FadeOut. Old presentation darkens.
pub const MAP_FADE_OUT_SEC: f32 = 0.300;
/// Minimum fully-black time after map FadeOut.
pub const MAP_FADE_HOLD_SEC: f32 = 0.075;
/// Map FadeIn after [`DestinationReady`].
pub const MAP_FADE_IN_SEC: f32 = 0.400;

/// Same-map Channel/Instance FadeOut. Shorter than a map load.
pub const MEMBERSHIP_FADE_OUT_SEC: f32 = 0.200;
/// Minimum black hold for membership.
pub const MEMBERSHIP_FADE_HOLD_SEC: f32 = 0.050;
/// Membership FadeIn after [`MembershipReady`].
pub const MEMBERSHIP_FADE_IN_SEC: f32 = 0.250;

/// DEV: stay black and warn if readiness never arrives. Not production recovery.
pub const READY_STALL_SEC: f32 = 5.0;

const POSE_STABLE_EPS: f32 = 0.05;

/// What kind of WorldAddress change this fade is hiding.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TransitionKind {
    #[default]
    Map,
    /// Same MapId: Channel and/or Instance. Not a map load.
    Membership,
}

/// Map destination may be revealed only after this snapshot is true.
///
/// Callers must not notify until: accepted dest WorldAddress, new epoch/baseline,
/// dest geometry, self Enter, local player seeded from dest pose, prediction
/// synced, camera following that pose, interp cleared of the old map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DestinationReady {
    pub epoch: u32,
    pub local_pose: [f32; 2],
}

/// Same-map Channel/Instance destination. Not a Portal and not a map rebuild.
///
/// Callers must not notify until: accepted Channel/Instance, new epoch/address,
/// old Known/replicas gone, dest Enter begun, local pose stable, presentation
/// WorldAddress matches observer, world-bound InteractionSession closed if
/// the target became WorldAddress-incompatible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MembershipReady {
    pub epoch: u32,
    pub address: (u32, u32, u32),
}

/// DEV flags. Map uses [`DestinationReady`]; membership uses [`MembershipReady`].
/// Do not treat this as a second competing gate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadinessFlags {
    pub observer_accepted: bool,
    pub epoch_applied: bool,
    pub geometry_ready: bool,
    pub self_baseline: bool,
    pub local_seeded: bool,
    pub prediction_synced: bool,
    pub camera_ready: bool,
    pub old_replicas_cleared: bool,
    pub presentation_matches: bool,
    pub session_closed: bool,
}

impl ReadinessFlags {
    #[must_use]
    pub fn map_complete(self) -> bool {
        self.observer_accepted
            && self.epoch_applied
            && self.geometry_ready
            && self.self_baseline
            && self.local_seeded
            && self.prediction_synced
            && self.camera_ready
            && self.old_replicas_cleared
            && self.presentation_matches
    }

    #[must_use]
    pub fn membership_complete(self) -> bool {
        self.observer_accepted
            && self.epoch_applied
            && self.self_baseline
            && self.local_seeded
            && self.old_replicas_cleared
            && self.presentation_matches
            && self.session_closed
    }

    #[must_use]
    pub fn missing_csv(self, kind: TransitionKind) -> String {
        let pairs: &[(&str, bool)] = match kind {
            TransitionKind::Map => &[
                ("observer_accepted", self.observer_accepted),
                ("epoch_applied", self.epoch_applied),
                ("geometry_ready", self.geometry_ready),
                ("self_baseline", self.self_baseline),
                ("local_seeded", self.local_seeded),
                ("prediction_synced", self.prediction_synced),
                ("camera_ready", self.camera_ready),
                ("old_replicas_cleared", self.old_replicas_cleared),
                ("presentation_matches", self.presentation_matches),
            ],
            TransitionKind::Membership => &[
                ("observer_accepted", self.observer_accepted),
                ("epoch_applied", self.epoch_applied),
                ("self_baseline", self.self_baseline),
                ("local_seeded", self.local_seeded),
                ("old_replicas_cleared", self.old_replicas_cleared),
                ("presentation_matches", self.presentation_matches),
                ("session_closed", self.session_closed),
            ],
        };
        pairs
            .iter()
            .filter(|(_, ok)| !*ok)
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// World-bound UI must not still be mid-session when membership is revealed.
#[must_use]
pub fn world_interaction_cleared_for_membership(kind_name: &str) -> bool {
    matches!(kind_name, "IDLE" | "CLOSED" | "REJECTED")
}

#[must_use]
pub fn pose_stable(before: [f32; 2], after: [f32; 2]) -> bool {
    (before[0] - after[0]).abs() <= POSE_STABLE_EPS
        && (before[1] - after[1]).abs() <= POSE_STABLE_EPS
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Phase {
    #[default]
    Idle,
    Out {
        t: f32,
    },
    Hold {
        t: f32,
        waited: f32,
    },
    In {
        t: f32,
    },
}

/// Screen fade driven by wall-clock `dt`. Not a simulation clock consumer.
#[derive(Clone, Copy, Debug, Default)]
pub struct MapFade {
    phase: Phase,
    kind: TransitionKind,
    from: (u32, u32, u32),
    to: (u32, u32, u32),
    pose_at_start: Option<[f32; 2]>,
    map_ready: bool,
    destination_ready: Option<DestinationReady>,
    membership_ready: Option<MembershipReady>,
    flags: ReadinessFlags,
    stalled: bool,
    stall_logged: bool,
}

impl MapFade {
    #[must_use]
    pub fn is_idle(&self) -> bool {
        matches!(self.phase, Phase::Idle)
    }

    #[must_use]
    pub fn kind(&self) -> TransitionKind {
        self.kind
    }

    #[must_use]
    pub fn dest_address(&self) -> (u32, u32, u32) {
        self.to
    }

    #[must_use]
    pub fn pose_at_start(&self) -> Option<[f32; 2]> {
        self.pose_at_start
    }

    #[must_use]
    pub fn stalled(&self) -> bool {
        self.stalled
    }

    #[must_use]
    pub fn flags(&self) -> ReadinessFlags {
        self.flags
    }

    pub fn set_flags(&mut self, flags: ReadinessFlags) {
        self.flags = flags;
    }

    #[cfg(test)]
    pub fn begin_out(&mut self) {
        let _ = self.begin_transition(TransitionKind::Map, (0, 0, 0), (0, 0, 0), None);
    }

    /// Start FadeOut only from Idle. Returns whether a new fade began.
    #[cfg(test)]
    pub fn begin_out_if_idle(&mut self) -> bool {
        self.begin_transition(TransitionKind::Map, (0, 0, 0), (0, 0, 0), None)
    }

    pub fn begin_transition(
        &mut self,
        kind: TransitionKind,
        from: (u32, u32, u32),
        to: (u32, u32, u32),
        pose: Option<[f32; 2]>,
    ) -> bool {
        if !self.is_idle() {
            return false;
        }
        self.phase = Phase::Out { t: 0.0 };
        self.kind = kind;
        self.from = from;
        self.to = to;
        self.pose_at_start = pose;
        self.map_ready = false;
        self.destination_ready = None;
        self.membership_ready = None;
        self.flags = ReadinessFlags::default();
        self.stalled = false;
        self.stall_logged = false;
        true
    }

    /// Test helper: production portal travel uses [`Self::notify_destination_ready`].
    #[cfg(test)]
    pub fn notify_map_ready(&mut self) {
        self.notify_destination_ready(DestinationReady {
            epoch: 0,
            local_pose: [0.0, 0.0],
        });
    }

    /// Geometry, new-epoch self baseline, prediction, and camera share `local_pose`.
    pub fn notify_destination_ready(&mut self, ready: DestinationReady) {
        if matches!(self.phase, Phase::Idle) || self.kind != TransitionKind::Map {
            return;
        }
        self.destination_ready = Some(ready);
        self.map_ready = true;
    }

    pub fn notify_membership_ready(&mut self, ready: MembershipReady) {
        if matches!(self.phase, Phase::Idle) || self.kind != TransitionKind::Membership {
            return;
        }
        self.membership_ready = Some(ready);
        self.map_ready = true;
    }

    #[must_use]
    pub fn destination_ready(&self) -> Option<DestinationReady> {
        self.destination_ready
    }

    #[must_use]
    pub fn membership_ready(&self) -> Option<MembershipReady> {
        self.membership_ready
    }

    #[must_use]
    pub fn presentation_ready(&self) -> bool {
        match self.kind {
            TransitionKind::Map => self.destination_ready.is_some(),
            TransitionKind::Membership => self.membership_ready.is_some(),
        }
    }

    #[must_use]
    pub fn debug_phase(&self) -> &'static str {
        match self.phase {
            Phase::Idle => "Idle",
            Phase::Out { .. } => "FadeOut",
            Phase::Hold { .. } => "Hold",
            Phase::In { .. } => "FadeIn",
        }
    }

    #[must_use]
    pub fn debug_banner(&self) -> Option<String> {
        if self.is_idle() {
            return None;
        }
        let label = match self.kind {
            TransitionKind::Map => "MAP TRANSITION",
            TransitionKind::Membership => membership_banner(self.from, self.to),
        };
        let step = match self.phase {
            Phase::Idle => return None,
            Phase::Out { .. } => "FadeOut",
            Phase::Hold { .. } if self.presentation_ready() => "Hold",
            Phase::Hold { .. } => match self.kind {
                TransitionKind::Map => "Waiting DestinationReady",
                TransitionKind::Membership => "Waiting MembershipReady",
            },
            Phase::In { .. } => "FadeIn",
        };
        let mut text = format!("{label} · {step}");
        if self.stalled {
            text.push_str(" · STALLED");
        }
        Some(text)
    }

    /// Portal request rejected after a speculative fade. Production fade starts
    /// only from an accepted address change, so this is test-only.
    #[cfg(test)]
    pub fn abort_to_in(&mut self) {
        if matches!(self.phase, Phase::Idle) {
            return;
        }
        self.phase = Phase::In { t: 0.0 };
        if self.destination_ready.is_none() && self.kind == TransitionKind::Map {
            self.destination_ready = Some(DestinationReady {
                epoch: 0,
                local_pose: [0.0, 0.0],
            });
        }
        self.map_ready = true;
    }

    /// True when a map baseline rebuild / membership retarget may be shown
    /// (idle, or fully black). Reveal still waits for readiness.
    #[must_use]
    pub fn allows_baseline_swap(&self) -> bool {
        matches!(self.phase, Phase::Idle | Phase::Hold { .. })
    }

    /// True while FadeOut/Hold: gameplay movement must not advance.
    /// FadeIn is the unlock boundary.
    #[must_use]
    pub fn gameplay_input_locked(&self) -> bool {
        matches!(self.phase, Phase::Out { .. } | Phase::Hold { .. })
    }

    /// FadeOut has reached the Hold phase: the overlay is fully black.
    /// This is the presentation commit boundary. Not a wall-clock delay.
    #[must_use]
    pub fn is_fully_black(&self) -> bool {
        matches!(self.phase, Phase::Hold { .. })
    }

    /// FadeIn is in progress; destination presentation is already committed.
    #[must_use]
    pub fn is_fading_in(&self) -> bool {
        matches!(self.phase, Phase::In { .. })
    }

    /// True while FadeOut is still visible (alpha < 1). Source presentation
    /// must stay on screen; destination replica may already be applied.
    #[must_use]
    pub fn holds_source_presentation(&self) -> bool {
        matches!(self.phase, Phase::Out { .. })
    }

    fn timings(&self) -> (f32, f32, f32) {
        match self.kind {
            TransitionKind::Map => (MAP_FADE_OUT_SEC, MAP_FADE_HOLD_SEC, MAP_FADE_IN_SEC),
            TransitionKind::Membership => (
                MEMBERSHIP_FADE_OUT_SEC,
                MEMBERSHIP_FADE_HOLD_SEC,
                MEMBERSHIP_FADE_IN_SEC,
            ),
        }
    }

    pub fn tick(&mut self, dt: f32) -> Option<String> {
        let dt = dt.max(0.0);
        let (out_sec, hold_sec, in_sec) = self.timings();
        let mut stall_log = None;
        match self.phase {
            Phase::Idle => {}
            Phase::Out { t } => {
                let t = t + dt;
                if t >= out_sec {
                    self.phase = Phase::Hold {
                        t: 0.0,
                        waited: 0.0,
                    };
                } else {
                    self.phase = Phase::Out { t };
                }
            }
            Phase::Hold { t, waited } => {
                let t = t + dt;
                let waited = waited + dt;
                if waited >= READY_STALL_SEC && !self.presentation_ready() {
                    self.stalled = true;
                    if !self.stall_logged {
                        self.stall_logged = true;
                        stall_log = Some(format!(
                            "6D_TRANSITION stalled kind={:?} from={:?} to={:?} missing={} waited={waited:.3}",
                            self.kind,
                            self.from,
                            self.to,
                            self.flags.missing_csv(self.kind)
                        ));
                    }
                }
                if t >= hold_sec && self.presentation_ready() && !self.stalled {
                    self.phase = Phase::In { t: 0.0 };
                } else {
                    self.phase = Phase::Hold { t, waited };
                }
            }
            Phase::In { t } => {
                let t = t + dt;
                if t >= in_sec {
                    self.phase = Phase::Idle;
                    self.map_ready = false;
                    self.destination_ready = None;
                    self.membership_ready = None;
                    self.stalled = false;
                    self.stall_logged = false;
                } else {
                    self.phase = Phase::In { t };
                }
            }
        }
        stall_log
    }

    #[must_use]
    pub fn alpha(&self) -> f32 {
        let (out_sec, _, in_sec) = self.timings();
        match self.phase {
            Phase::Idle => 0.0,
            Phase::Out { t } => (t / out_sec).clamp(0.0, 1.0),
            Phase::Hold { .. } => 1.0,
            Phase::In { t } => 1.0 - (t / in_sec).clamp(0.0, 1.0),
        }
    }

    #[must_use]
    pub fn overlay_quad(&self, camera: &Camera) -> Option<DrawQuad> {
        let a = self.alpha();
        if a <= 0.001 {
            return None;
        }
        Some(DrawQuad::rect(
            camera.position,
            [camera.viewport_width * 1.05, camera.viewport_height * 1.05],
            [0.0, 0.0, 0.0, a],
        ))
    }

    #[cfg(test)]
    #[must_use]
    fn is_hold(&self) -> bool {
        matches!(self.phase, Phase::Hold { .. })
    }

    #[cfg(test)]
    #[must_use]
    fn is_in(&self) -> bool {
        matches!(self.phase, Phase::In { .. })
    }
}

#[must_use]
fn membership_banner(from: (u32, u32, u32), to: (u32, u32, u32)) -> &'static str {
    let channel_only = from.0 == to.0 && from.1 != to.1 && from.2 == to.2;
    let instance_only = from.0 == to.0 && from.1 == to.1 && from.2 != to.2;
    if channel_only {
        "CHANNEL TRANSITION"
    } else if instance_only {
        "INSTANCE TRANSITION"
    } else {
        "MEMBERSHIP TRANSITION"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_out_holds_until_map_ready_then_fades_in() {
        let mut fade = MapFade::default();
        fade.begin_out();
        fade.tick(MAP_FADE_OUT_SEC * 0.5);
        assert!(fade.alpha() > 0.4 && fade.alpha() < 0.6);
        assert!(fade.holds_source_presentation());
        assert!(!fade.is_fully_black());
        assert!(!fade.allows_baseline_swap());
        fade.tick(MAP_FADE_OUT_SEC);
        assert!(fade.is_hold());
        assert!(fade.is_fully_black());
        assert!(!fade.holds_source_presentation());
        assert!((fade.alpha() - 1.0).abs() < 1e-4);
        assert!(fade.allows_baseline_swap());
        fade.notify_map_ready();
        fade.tick(MAP_FADE_HOLD_SEC * 0.5);
        assert!(fade.is_hold());
        assert!((fade.alpha() - 1.0).abs() < 1e-4);
        fade.tick(MAP_FADE_HOLD_SEC);
        assert!(fade.is_in());
        assert!(fade.alpha() > 0.9);
        fade.tick(MAP_FADE_IN_SEC + 0.01);
        assert!(fade.is_idle());
        assert!(fade.alpha() <= 0.001);
    }

    #[test]
    fn map_ready_during_out_does_not_skip_black_hold() {
        let mut fade = MapFade::default();
        fade.begin_out();
        fade.tick(MAP_FADE_OUT_SEC * 0.25);
        fade.notify_map_ready();
        fade.tick(MAP_FADE_OUT_SEC);
        assert!(fade.is_hold(), "must sit at black after fade-out");
        assert!((fade.alpha() - 1.0).abs() < 1e-4);
        fade.tick(MAP_FADE_HOLD_SEC * 0.5);
        assert!(fade.is_hold());
        fade.tick(MAP_FADE_HOLD_SEC);
        assert!(fade.is_in());
    }

    #[test]
    fn fade_in_waits_for_destination_ready() {
        let mut fade = MapFade::default();
        fade.begin_out();
        fade.tick(MAP_FADE_OUT_SEC + 0.01);
        fade.tick(MAP_FADE_HOLD_SEC + 0.01);
        assert!(fade.is_hold());
        assert!(fade.destination_ready().is_none());
        fade.tick(0.0);
        assert!(
            fade.is_hold(),
            "FadeIn must not start without DestinationReady"
        );
        fade.notify_destination_ready(DestinationReady {
            epoch: 3,
            local_pose: [10.0, -2.3],
        });
        fade.tick(0.0);
        assert!(fade.is_in());
        assert_eq!(fade.destination_ready().unwrap().local_pose, [10.0, -2.3]);
    }

    #[test]
    fn delayed_destination_ready_keeps_black() {
        let mut fade = MapFade::default();
        fade.begin_transition(TransitionKind::Map, (1, 0, 0), (2, 0, 0), None);
        fade.tick(MAP_FADE_OUT_SEC + MAP_FADE_HOLD_SEC + 1.0);
        assert!(fade.is_hold());
        assert!((fade.alpha() - 1.0).abs() < 1e-4);
        assert_eq!(
            fade.debug_banner().as_deref(),
            Some("MAP TRANSITION · Waiting DestinationReady")
        );
    }

    #[test]
    fn membership_fade_in_waits_for_membership_ready() {
        let mut fade = MapFade::default();
        fade.begin_transition(
            TransitionKind::Membership,
            (1, 0, 0),
            (1, 1, 0),
            Some([-8.0, -3.0]),
        );
        fade.tick(MEMBERSHIP_FADE_OUT_SEC + 0.01);
        fade.tick(MEMBERSHIP_FADE_HOLD_SEC + 0.01);
        assert!(fade.is_hold());
        assert!((fade.alpha() - 1.0).abs() < 1e-4);
        fade.notify_destination_ready(DestinationReady {
            epoch: 1,
            local_pose: [-8.0, -3.0],
        });
        fade.tick(0.0);
        assert!(
            fade.is_hold(),
            "Map DestinationReady must not reveal a membership fade"
        );
        fade.notify_membership_ready(MembershipReady {
            epoch: 1,
            address: (1, 1, 0),
        });
        fade.tick(0.0);
        assert!(fade.is_in());
        assert_eq!(
            fade.debug_banner().as_deref(),
            Some("CHANNEL TRANSITION · FadeIn")
        );
    }

    #[test]
    fn membership_request_without_begin_does_not_obscure() {
        let fade = MapFade::default();
        assert!(fade.is_idle());
        assert!(fade.alpha() <= 0.001);
        assert!(fade.debug_banner().is_none());
    }

    #[test]
    fn stall_stays_black_and_does_not_fake_ready() {
        let mut fade = MapFade::default();
        fade.begin_transition(TransitionKind::Membership, (1, 0, 0), (1, 1, 0), None);
        fade.tick(MEMBERSHIP_FADE_OUT_SEC + 0.01);
        let log = fade.tick(READY_STALL_SEC + 0.01);
        assert!(fade.is_hold());
        assert!(fade.stalled());
        assert!(log.is_some_and(|s| s.contains("stalled") && s.contains("Membership")));
        fade.notify_membership_ready(MembershipReady {
            epoch: 1,
            address: (1, 1, 0),
        });
        fade.tick(0.0);
        assert!(
            fade.is_hold(),
            "stalled gate must not FadeIn even if a late ready arrives"
        );
        assert!(fade.debug_banner().unwrap().contains("STALLED"));
    }

    #[test]
    fn reject_fades_back_in() {
        let mut fade = MapFade::default();
        fade.begin_out();
        fade.tick(MAP_FADE_OUT_SEC * 0.5);
        fade.abort_to_in();
        fade.tick(MAP_FADE_IN_SEC + 0.01);
        assert!(fade.is_idle());
    }

    #[test]
    fn idle_has_no_overlay_until_authoritative_begin_out() {
        let fade = MapFade::default();
        assert!(fade.is_idle());
        assert!(fade.alpha() <= 0.001);
        assert!(fade.allows_baseline_swap());
    }

    #[test]
    fn begin_out_if_idle_does_not_restart_an_active_fade() {
        let mut fade = MapFade::default();
        assert!(fade.begin_out_if_idle());
        fade.tick(MAP_FADE_OUT_SEC * 0.25);
        let alpha = fade.alpha();
        assert!(!fade.begin_out_if_idle());
        assert!((fade.alpha() - alpha).abs() < 1e-5);
    }

    #[test]
    fn membership_flags_require_session_close() {
        let mut flags = ReadinessFlags {
            observer_accepted: true,
            epoch_applied: true,
            self_baseline: true,
            local_seeded: true,
            old_replicas_cleared: true,
            presentation_matches: true,
            session_closed: false,
            ..ReadinessFlags::default()
        };
        assert!(!flags.membership_complete());
        assert!(
            flags
                .missing_csv(TransitionKind::Membership)
                .contains("session_closed")
        );
        flags.session_closed = true;
        assert!(flags.membership_complete());
    }

    #[test]
    fn map_flags_do_not_require_session_closed() {
        let flags = ReadinessFlags {
            observer_accepted: true,
            epoch_applied: true,
            geometry_ready: true,
            self_baseline: true,
            local_seeded: true,
            prediction_synced: true,
            camera_ready: true,
            old_replicas_cleared: true,
            presentation_matches: true,
            session_closed: false,
        };
        assert!(flags.map_complete());
    }

    #[test]
    fn pose_stable_allows_tiny_noise_not_a_teleport() {
        assert!(pose_stable([-8.0, -3.0], [-8.0, -3.0]));
        assert!(pose_stable([-8.0, -3.0], [-8.02, -3.01]));
        assert!(!pose_stable([-8.0, -3.0], [10.0, -2.3]));
    }

    #[test]
    fn early_ready_does_not_commit_presentation_before_blackout() {
        let mut fade = MapFade::default();
        fade.begin_transition(TransitionKind::Map, (1, 0, 0), (2, 0, 0), None);
        fade.notify_destination_ready(DestinationReady {
            epoch: 2,
            local_pose: [10.0, -2.3],
        });
        fade.tick(MAP_FADE_OUT_SEC * 0.4);
        assert!(fade.holds_source_presentation());
        assert!(!fade.is_fully_black());
        assert!(!fade.allows_baseline_swap());
        fade.tick(MAP_FADE_OUT_SEC);
        assert!(fade.is_fully_black());
        assert!(fade.allows_baseline_swap());
    }

    #[test]
    fn fully_black_is_hold_not_an_extra_delay() {
        let mut fade = MapFade::default();
        fade.begin_out();
        assert!(!fade.is_fully_black());
        fade.tick(MAP_FADE_OUT_SEC);
        assert!(fade.is_fully_black());
        assert!((fade.alpha() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn gameplay_input_locked_until_fade_in() {
        let mut fade = MapFade::default();
        assert!(!fade.gameplay_input_locked());
        fade.begin_out();
        assert!(fade.gameplay_input_locked());
        fade.tick(MAP_FADE_OUT_SEC);
        assert!(fade.gameplay_input_locked());
        fade.notify_map_ready();
        fade.tick(MAP_FADE_HOLD_SEC);
        assert!(fade.is_in());
        assert!(
            !fade.gameplay_input_locked(),
            "FadeIn is the gameplay unlock boundary"
        );
        fade.tick(MAP_FADE_IN_SEC + 0.01);
        assert!(fade.is_idle());
        assert!(!fade.gameplay_input_locked());
    }

    #[test]
    fn lock_duration_matches_authoritative_barrier() {
        assert!(
            (MAP_FADE_OUT_SEC + MAP_FADE_HOLD_SEC
                - purgatory_simulation::MAP_TRANSITION_INPUT_LOCK_SECS)
                .abs()
                < 1e-6
        );
        assert!(
            (MEMBERSHIP_FADE_OUT_SEC + MEMBERSHIP_FADE_HOLD_SEC
                - purgatory_simulation::MEMBERSHIP_TRANSITION_INPUT_LOCK_SECS)
                .abs()
                < 1e-6
        );
    }
}
