//! DEV overlay headline for interaction / target / portal. Not production UI.

use purgatory_simulation::INTERACT_RANGE;

use crate::ui_runtime::{InteractKind, UIRuntimeState};

#[derive(Clone, Debug, PartialEq)]
pub struct InteractStatusView {
    pub kind: InteractKind,
    pub session_id: Option<u32>,
    pub target: Option<String>,
    pub reject_reason: Option<String>,
    pub close_reason: Option<String>,
    pub last_transition: Option<String>,
    pub trail: String,
}

impl Default for InteractStatusView {
    fn default() -> Self {
        Self {
            kind: InteractKind::Idle,
            session_id: None,
            target: None,
            reject_reason: None,
            close_reason: None,
            last_transition: None,
            trail: String::new(),
        }
    }
}

impl InteractStatusView {
    #[must_use]
    pub fn from_runtime(
        state: &UIRuntimeState,
        last_transition: Option<String>,
        trail: &str,
    ) -> Self {
        let (session_id, target, reject_reason, close_reason) = match *state {
            UIRuntimeState::Idle => (None, None, None, None),
            UIRuntimeState::Opening { target } => (None, Some(target.to_string()), None, None),
            UIRuntimeState::Active { session_id, target }
            | UIRuntimeState::Closing { session_id, target } => {
                (Some(session_id), Some(target.to_string()), None, None)
            }
            UIRuntimeState::Rejected { target, reason } => (
                None,
                Some(target.to_string()),
                Some(reason.to_string()),
                None,
            ),
            UIRuntimeState::Closed { session_id, reason } => {
                (Some(session_id), None, None, Some(reason.to_string()))
            }
        };
        Self {
            kind: state.kind(),
            session_id,
            target,
            reject_reason,
            close_reason,
            last_transition,
            trail: trail.to_string(),
        }
    }

    /// Current-state headline. History is not included.
    #[must_use]
    pub fn interaction_line(&self) -> String {
        let mut parts = vec![format!("{} {}", self.kind.glyph(), self.kind.name())];
        if self.kind == InteractKind::Rejected
            && let Some(reason) = &self.reject_reason
        {
            parts.push(reason.clone());
        }
        if let Some(session) = self.session_id
            && matches!(
                self.kind,
                InteractKind::Open | InteractKind::Closing | InteractKind::Closed
            )
        {
            parts.push(format!("Session #{session}"));
        }
        if let Some(target) = &self.target
            && matches!(
                self.kind,
                InteractKind::Opening | InteractKind::Open | InteractKind::Closing
            )
        {
            parts.push(format!("Interactable {target}"));
        }
        parts.join(" · ")
    }

    #[must_use]
    pub fn details_trail(&self) -> String {
        if self.trail.is_empty() {
            "—".into()
        } else if let Some(session) = self.session_id {
            format!("{} · Session #{session}", self.trail)
        } else {
            self.trail.clone()
        }
    }
}

#[must_use]
pub fn format_target_line(id: Option<&str>, distance: Option<f32>) -> String {
    match (id, distance) {
        (None, _) => "NONE".into(),
        (Some(id), Some(dist)) if dist > INTERACT_RANGE => {
            format!("Interactable {id} · {dist:.2} wu · OUT OF RANGE")
        }
        (Some(id), Some(dist)) => format!("Interactable {id} · {dist:.2} wu"),
        (Some(id), None) => format!("Interactable {id}"),
    }
}

#[must_use]
pub fn format_portal_line(id: Option<&str>, eligible: bool) -> String {
    match id {
        None => "NONE".into(),
        Some(id) if eligible => format!("{id} · ELIGIBLE"),
        Some(id) => format!("{id} · OUTSIDE ZONE"),
    }
}

/// Compact chrome Interact value. Flash stays on this line so a second chip
/// cannot change row height.
#[must_use]
pub fn compact_interact_value(interaction_line: &str, flash: Option<&str>) -> String {
    match flash {
        Some(flash) if !flash.is_empty() => {
            format!("{interaction_line} · {}", flash.replace('→', "->"))
        }
        _ => interaction_line.to_string(),
    }
}

#[must_use]
pub fn interact_kind_color(kind: InteractKind) -> (u8, u8, u8) {
    match kind {
        InteractKind::Idle => (170, 175, 185),
        InteractKind::Opening => (255, 210, 70),
        InteractKind::Open => (90, 220, 120),
        InteractKind::Closing => (255, 170, 80),
        InteractKind::Closed => (140, 200, 160),
        InteractKind::Rejected => (255, 90, 90),
    }
}

#[must_use]
pub fn note_kind_change(
    previous: Option<InteractKind>,
    next: InteractKind,
    trail: &mut Vec<&'static str>,
) -> Option<String> {
    if previous == Some(next) {
        return None;
    }
    let prev = previous.unwrap_or(InteractKind::Idle);
    if prev == next {
        return None;
    }
    trail.push(next.name());
    const MAX: usize = 8;
    if trail.len() > MAX {
        trail.remove(0);
    }
    Some(format!("{} → {}", prev.name(), next.name()))
}

#[must_use]
pub fn trail_display(trail: &[&'static str]) -> String {
    trail.join(" → ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_protocol::{InteractCloseReason, InteractRejectReason, WireEntityId};

    fn eid(index: u32) -> WireEntityId {
        WireEntityId {
            index,
            generation: 1,
        }
    }

    #[test]
    fn headline_is_current_state_not_history() {
        let view = InteractStatusView::from_runtime(
            &UIRuntimeState::Active {
                session_id: 3,
                target: eid(27),
            },
            Some("OPENING → OPEN".into()),
            "IDLE → OPENING → OPEN",
        );
        let line = view.interaction_line();
        assert!(line.contains("OPEN"), "{line}");
        assert!(line.contains("Session #3"), "{line}");
        assert!(line.contains("27:1"), "{line}");
        assert!(!line.contains("OPENING →"), "{line}");
        assert!(!line.contains("Requested"), "{line}");
        assert!(!line.contains("Closed"), "{line}");
        assert_eq!(view.details_trail(), "IDLE → OPENING → OPEN · Session #3");
    }

    #[test]
    fn rejected_shows_reason_on_current_line() {
        let view = InteractStatusView::from_runtime(
            &UIRuntimeState::Rejected {
                target: eid(27),
                reason: InteractRejectReason::OutOfRange,
            },
            Some("OPENING → REJECTED".into()),
            "OPENING → REJECTED",
        );
        let line = view.interaction_line();
        assert!(line.contains("REJECTED"), "{line}");
        assert!(line.contains("OutOfRange"), "{line}");
        assert!(!line.contains("OPENING → REJECTED"), "{line}");
    }

    #[test]
    fn closed_does_not_look_like_open() {
        let view = InteractStatusView::from_runtime(
            &UIRuntimeState::Closed {
                session_id: 3,
                reason: InteractCloseReason::Requested,
            },
            Some("CLOSING → CLOSED".into()),
            "OPEN → CLOSING → CLOSED",
        );
        let line = view.interaction_line();
        assert!(line.contains("CLOSED"), "{line}");
        assert!(line.contains("Session #3"), "{line}");
        assert!(!line.contains("Requested"), "{line}");
        assert!(!line.contains("OPEN ·"), "{line}");
    }

    #[test]
    fn target_none_in_range_and_out_of_range() {
        assert_eq!(format_target_line(None, None), "NONE");
        assert_eq!(
            format_target_line(Some("27:1"), Some(2.19)),
            "Interactable 27:1 · 2.19 wu"
        );
        assert_eq!(
            format_target_line(Some("27:1"), Some(5.2)),
            "Interactable 27:1 · 5.20 wu · OUT OF RANGE"
        );
    }

    #[test]
    fn portal_is_not_an_e_session() {
        assert_eq!(format_portal_line(None, false), "NONE");
        assert_eq!(format_portal_line(Some("28:1"), true), "28:1 · ELIGIBLE");
        assert_eq!(
            format_portal_line(Some("28:1"), false),
            "28:1 · OUTSIDE ZONE"
        );
    }

    #[test]
    fn compact_interact_value_stays_one_slot_when_flashing() {
        let idle = InteractStatusView::default().interaction_line();
        assert_eq!(compact_interact_value(&idle, None), idle);
        assert_eq!(compact_interact_value(&idle, Some("")), idle);
        assert_eq!(
            compact_interact_value(&idle, Some("OPENING → OPEN")),
            format!("{idle} · OPENING -> OPEN")
        );
    }

    #[test]
    fn kind_change_records_transition_not_simultaneous_states() {
        let mut trail = Vec::new();
        assert_eq!(
            note_kind_change(None, InteractKind::Opening, &mut trail).as_deref(),
            Some("IDLE → OPENING")
        );
        assert_eq!(
            note_kind_change(Some(InteractKind::Opening), InteractKind::Open, &mut trail)
                .as_deref(),
            Some("OPENING → OPEN")
        );
        assert_eq!(
            note_kind_change(Some(InteractKind::Open), InteractKind::Closing, &mut trail)
                .as_deref(),
            Some("OPEN → CLOSING")
        );
        assert_eq!(
            note_kind_change(
                Some(InteractKind::Closing),
                InteractKind::Closed,
                &mut trail
            )
            .as_deref(),
            Some("CLOSING → CLOSED")
        );
        assert_eq!(trail_display(&trail), "OPENING → OPEN → CLOSING → CLOSED");
    }
}
