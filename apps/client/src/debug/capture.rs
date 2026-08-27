//! Client-only debug input routing. Simulation never sees egui.

/// Whether a keyboard event should also map to gameplay [`crate::input::ActionState`].
///
/// Opening the overlay does not steal keys. A focused text-like widget steals
/// **presses** only. Releases still reach gameplay so held movement cannot stick.
#[must_use]
pub fn gameplay_receives_keyboard(
    overlay_visible: bool,
    text_like_focus: bool,
    pressed: bool,
) -> bool {
    if !overlay_visible {
        return true;
    }
    !(text_like_focus && pressed)
}

/// Whether a pointer event should map to future gameplay mouse actions.
///
/// There is no gameplay mouse yet. Hovering or clicking egui must not become
/// a game click later.
#[must_use]
pub fn gameplay_receives_pointer(overlay_visible: bool, egui_wants_pointer: bool) -> bool {
    if !overlay_visible {
        return true;
    }
    !egui_wants_pointer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_open_does_not_block_gameplay_keys() {
        assert!(gameplay_receives_keyboard(true, false, true));
        assert!(gameplay_receives_keyboard(true, false, false));
    }

    #[test]
    fn overlay_closed_always_forwards_keys() {
        assert!(gameplay_receives_keyboard(false, false, true));
        assert!(gameplay_receives_keyboard(false, true, true));
    }

    #[test]
    fn button_or_window_focus_is_not_text_like() {
        let overlay_visible = true;
        let text_like_focus = false;
        assert!(gameplay_receives_keyboard(
            overlay_visible,
            text_like_focus,
            true
        ));
    }

    #[test]
    fn text_like_focus_steals_presses_not_releases() {
        assert!(!gameplay_receives_keyboard(true, true, true));
        assert!(gameplay_receives_keyboard(true, true, false));
    }

    #[test]
    fn pointer_over_egui_is_not_gameplay_mouse() {
        assert!(!gameplay_receives_pointer(true, true));
        assert!(gameplay_receives_pointer(true, false));
        assert!(gameplay_receives_pointer(false, true));
    }
}
