//! Reusable production UI controls.
//!
//! Each control owns transient press state only. Callers own values, skins, and results.
//! A click or toggle requires press inside the same bounds used for drawing, then release
//! inside those bounds.

use winit::event::ElementState;

use crate::renderer::SpriteTextureId;
use crate::ui_panel::ScreenRect;
use crate::ui_v2::UiV2Image;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiPointerOutcome {
    Idle,
    Captured,
    Activated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiRadioOutcome {
    Idle,
    Captured,
    Selected(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiButtonVisual {
    Normal,
    Hover,
    Pressed,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiCheckboxVisual {
    Unchecked,
    Hover,
    Checked,
    CheckedHover,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiRadioVisual {
    Off,
    Hover,
    On,
    OnHover,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiTabVisual {
    Active,
    Inactive,
    Hover,
    Pressed,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiButtonSkin {
    normal: SpriteTextureId,
    hover: SpriteTextureId,
    pressed: SpriteTextureId,
    disabled: SpriteTextureId,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiCheckboxSkin {
    unchecked: SpriteTextureId,
    hover: SpriteTextureId,
    checked: SpriteTextureId,
    disabled: SpriteTextureId,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiRadioSkin {
    off: SpriteTextureId,
    hover: SpriteTextureId,
    on: SpriteTextureId,
    disabled: SpriteTextureId,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiTabSkin {
    active: SpriteTextureId,
    inactive: SpriteTextureId,
    hover: SpriteTextureId,
    pressed: SpriteTextureId,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct UiButton {
    pressed: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct UiCheckbox {
    pressed: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct UiRadioGroup {
    pressed: Option<usize>,
}

impl UiButtonSkin {
    pub(crate) fn from_family(images: [UiV2Image; 4]) -> Self {
        Self {
            normal: images[0].texture,
            hover: images[1].texture,
            pressed: images[2].texture,
            disabled: images[3].texture,
        }
    }

    pub(crate) fn texture(self, visual: UiButtonVisual) -> SpriteTextureId {
        match visual {
            UiButtonVisual::Normal => self.normal,
            UiButtonVisual::Hover => self.hover,
            UiButtonVisual::Pressed => self.pressed,
            UiButtonVisual::Disabled => self.disabled,
        }
    }
}

impl UiCheckboxSkin {
    pub(crate) fn from_family(images: [UiV2Image; 4]) -> Self {
        Self {
            unchecked: images[0].texture,
            hover: images[1].texture,
            checked: images[2].texture,
            disabled: images[3].texture,
        }
    }

    pub(crate) fn texture(self, visual: UiCheckboxVisual) -> SpriteTextureId {
        match visual {
            UiCheckboxVisual::Unchecked => self.unchecked,
            UiCheckboxVisual::Hover => self.hover,
            UiCheckboxVisual::Checked | UiCheckboxVisual::CheckedHover => self.checked,
            UiCheckboxVisual::Disabled => self.disabled,
        }
    }
}

impl UiRadioSkin {
    pub(crate) fn from_family(images: [UiV2Image; 4]) -> Self {
        Self {
            off: images[0].texture,
            hover: images[1].texture,
            on: images[2].texture,
            disabled: images[3].texture,
        }
    }

    pub(crate) fn texture(self, visual: UiRadioVisual) -> SpriteTextureId {
        match visual {
            UiRadioVisual::Off => self.off,
            UiRadioVisual::Hover => self.hover,
            UiRadioVisual::On | UiRadioVisual::OnHover => self.on,
            UiRadioVisual::Disabled => self.disabled,
        }
    }
}

impl UiTabSkin {
    pub(crate) fn from_family(images: [UiV2Image; 4]) -> Self {
        Self {
            active: images[0].texture,
            inactive: images[1].texture,
            hover: images[2].texture,
            pressed: images[3].texture,
        }
    }

    pub(crate) fn texture(self, visual: UiTabVisual) -> SpriteTextureId {
        match visual {
            UiTabVisual::Active => self.active,
            UiTabVisual::Inactive => self.inactive,
            UiTabVisual::Hover => self.hover,
            UiTabVisual::Pressed => self.pressed,
        }
    }
}

pub(crate) fn tab_visual(selected: bool, hovered: bool, pressed: bool) -> UiTabVisual {
    if pressed {
        UiTabVisual::Pressed
    } else if selected {
        UiTabVisual::Active
    } else if hovered {
        UiTabVisual::Hover
    } else {
        UiTabVisual::Inactive
    }
}

fn inside(bounds: ScreenRect, cursor: Option<[f32; 2]>) -> bool {
    cursor.is_some_and(|point| bounds.contains(point))
}

impl UiButton {
    pub(crate) fn is_pressed(self) -> bool {
        self.pressed
    }

    pub(crate) fn cancel(&mut self) {
        self.pressed = false;
    }

    pub(crate) fn visual(
        self,
        cursor: Option<[f32; 2]>,
        bounds: ScreenRect,
        enabled: bool,
    ) -> UiButtonVisual {
        if !enabled {
            return UiButtonVisual::Disabled;
        }
        let hovered = inside(bounds, cursor);
        if self.pressed && hovered {
            UiButtonVisual::Pressed
        } else if hovered {
            UiButtonVisual::Hover
        } else {
            UiButtonVisual::Normal
        }
    }

    pub(crate) fn apply(
        &mut self,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        bounds: ScreenRect,
        enabled: bool,
    ) -> UiPointerOutcome {
        let hovered = inside(bounds, cursor);
        match state {
            ElementState::Pressed => {
                if !hovered {
                    return UiPointerOutcome::Idle;
                }
                if !enabled {
                    return UiPointerOutcome::Captured;
                }
                self.pressed = true;
                UiPointerOutcome::Captured
            }
            ElementState::Released => {
                if !self.pressed {
                    return UiPointerOutcome::Idle;
                }
                self.pressed = false;
                if enabled && hovered {
                    UiPointerOutcome::Activated
                } else {
                    UiPointerOutcome::Captured
                }
            }
        }
    }
}

impl UiCheckbox {
    pub(crate) fn is_pressed(self) -> bool {
        self.pressed
    }

    pub(crate) fn cancel(&mut self) {
        self.pressed = false;
    }

    pub(crate) fn visual(
        self,
        cursor: Option<[f32; 2]>,
        bounds: ScreenRect,
        checked: bool,
        enabled: bool,
    ) -> UiCheckboxVisual {
        if !enabled {
            return UiCheckboxVisual::Disabled;
        }
        let hovered = inside(bounds, cursor);
        if checked {
            if hovered {
                UiCheckboxVisual::CheckedHover
            } else {
                UiCheckboxVisual::Checked
            }
        } else if hovered {
            UiCheckboxVisual::Hover
        } else {
            UiCheckboxVisual::Unchecked
        }
    }

    pub(crate) fn apply(
        &mut self,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        bounds: ScreenRect,
        enabled: bool,
    ) -> UiPointerOutcome {
        let hovered = inside(bounds, cursor);
        match state {
            ElementState::Pressed => {
                if !hovered {
                    return UiPointerOutcome::Idle;
                }
                if !enabled {
                    return UiPointerOutcome::Captured;
                }
                self.pressed = true;
                UiPointerOutcome::Captured
            }
            ElementState::Released => {
                if !self.pressed {
                    return UiPointerOutcome::Idle;
                }
                self.pressed = false;
                if enabled && hovered {
                    UiPointerOutcome::Activated
                } else {
                    UiPointerOutcome::Captured
                }
            }
        }
    }
}

impl UiRadioGroup {
    pub(crate) fn is_pressed(self) -> bool {
        self.pressed.is_some()
    }

    pub(crate) fn cancel(&mut self) {
        self.pressed = None;
    }

    pub(crate) fn visual(
        self,
        index: usize,
        selected: bool,
        cursor: Option<[f32; 2]>,
        bounds: ScreenRect,
        enabled: bool,
    ) -> UiRadioVisual {
        if !enabled {
            return UiRadioVisual::Disabled;
        }
        let armed_elsewhere = self.pressed.is_some_and(|pressed| pressed != index);
        let hovered = inside(bounds, cursor) && !armed_elsewhere;
        if selected {
            if hovered {
                UiRadioVisual::OnHover
            } else {
                UiRadioVisual::On
            }
        } else if hovered {
            UiRadioVisual::Hover
        } else {
            UiRadioVisual::Off
        }
    }

    pub(crate) fn apply(
        &mut self,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        options: &[(ScreenRect, bool)],
    ) -> UiRadioOutcome {
        let hit = cursor.and_then(|point| {
            options
                .iter()
                .position(|(bounds, _)| bounds.contains(point))
        });
        match state {
            ElementState::Pressed => {
                let Some(index) = hit else {
                    return UiRadioOutcome::Idle;
                };
                if !options[index].1 {
                    return UiRadioOutcome::Captured;
                }
                self.pressed = Some(index);
                UiRadioOutcome::Captured
            }
            ElementState::Released => {
                let Some(pressed) = self.pressed.take() else {
                    return UiRadioOutcome::Idle;
                };
                if hit == Some(pressed) && options.get(pressed).is_some_and(|(_, enabled)| *enabled)
                {
                    UiRadioOutcome::Selected(pressed)
                } else {
                    UiRadioOutcome::Captured
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> ScreenRect {
        ScreenRect {
            min: [10.0, 20.0],
            max: [110.0, 50.0],
        }
    }

    fn inside_point() -> [f32; 2] {
        [40.0, 30.0]
    }

    fn outside_point() -> [f32; 2] {
        [0.0, 0.0]
    }

    fn press_release(
        control: &mut UiButton,
        cursor: Option<[f32; 2]>,
        enabled: bool,
    ) -> UiPointerOutcome {
        let pressed = control.apply(ElementState::Pressed, cursor, bounds(), enabled);
        assert_eq!(
            pressed,
            if cursor.is_some_and(|point| bounds().contains(point)) {
                UiPointerOutcome::Captured
            } else {
                UiPointerOutcome::Idle
            }
        );
        control.apply(ElementState::Released, cursor, bounds(), enabled)
    }

    #[test]
    fn button_hover_press_and_click_use_one_bounds() {
        let mut button = UiButton::default();
        assert_eq!(button.visual(None, bounds(), true), UiButtonVisual::Normal);
        assert_eq!(
            button.visual(Some(inside_point()), bounds(), true),
            UiButtonVisual::Hover
        );
        assert_eq!(
            button.apply(ElementState::Pressed, Some(inside_point()), bounds(), true),
            UiPointerOutcome::Captured
        );
        assert!(button.is_pressed());
        assert_eq!(
            button.visual(Some(inside_point()), bounds(), true),
            UiButtonVisual::Pressed
        );
        assert_eq!(
            button.visual(Some(outside_point()), bounds(), true),
            UiButtonVisual::Normal
        );
        assert_eq!(
            button.apply(ElementState::Released, Some(inside_point()), bounds(), true),
            UiPointerOutcome::Activated
        );
        assert!(!button.is_pressed());
        assert_eq!(
            button.visual(Some(inside_point()), bounds(), true),
            UiButtonVisual::Hover
        );
    }

    #[test]
    fn button_release_outside_does_not_click() {
        let mut button = UiButton::default();
        assert_eq!(
            button.apply(ElementState::Pressed, Some(inside_point()), bounds(), true),
            UiPointerOutcome::Captured
        );
        assert_eq!(
            button.apply(
                ElementState::Released,
                Some(outside_point()),
                bounds(),
                true
            ),
            UiPointerOutcome::Captured
        );
        assert!(!button.is_pressed());
    }

    #[test]
    fn disabled_button_never_clicks() {
        let mut button = UiButton::default();
        assert_eq!(
            button.visual(Some(inside_point()), bounds(), false),
            UiButtonVisual::Disabled
        );
        assert_eq!(
            button.apply(ElementState::Pressed, Some(inside_point()), bounds(), false),
            UiPointerOutcome::Captured
        );
        assert!(!button.is_pressed());
        assert_eq!(
            button.apply(
                ElementState::Released,
                Some(inside_point()),
                bounds(),
                false
            ),
            UiPointerOutcome::Idle
        );
    }

    #[test]
    fn button_cancel_clears_pressed_state() {
        let mut button = UiButton::default();
        button.apply(ElementState::Pressed, Some(inside_point()), bounds(), true);
        button.cancel();
        assert_eq!(
            button.apply(ElementState::Released, Some(inside_point()), bounds(), true),
            UiPointerOutcome::Idle
        );
    }

    #[test]
    fn icon_button_uses_button_press_release_contract() {
        let mut icon = UiButton::default();
        assert_eq!(
            press_release(&mut icon, Some(inside_point()), true),
            UiPointerOutcome::Activated
        );
        let mut missed = UiButton::default();
        missed.apply(ElementState::Pressed, Some(inside_point()), bounds(), true);
        assert_eq!(
            missed.apply(
                ElementState::Released,
                Some(outside_point()),
                bounds(),
                true
            ),
            UiPointerOutcome::Captured
        );
        let mut disabled = UiButton::default();
        assert_eq!(
            disabled.apply(ElementState::Pressed, Some(inside_point()), bounds(), false),
            UiPointerOutcome::Captured
        );
        assert!(!disabled.is_pressed());
    }

    #[test]
    fn checkbox_toggles_once_on_release_inside_only() {
        let mut checkbox = UiCheckbox::default();
        assert_eq!(
            checkbox.visual(None, bounds(), false, true),
            UiCheckboxVisual::Unchecked
        );
        assert_eq!(
            checkbox.visual(Some(inside_point()), bounds(), false, true),
            UiCheckboxVisual::Hover
        );
        assert_eq!(
            checkbox.apply(ElementState::Pressed, Some(inside_point()), bounds(), true),
            UiPointerOutcome::Captured
        );
        assert_eq!(
            checkbox.visual(Some(inside_point()), bounds(), false, true),
            UiCheckboxVisual::Hover
        );
        assert_eq!(
            checkbox.apply(ElementState::Released, Some(inside_point()), bounds(), true),
            UiPointerOutcome::Activated
        );
        assert_eq!(
            checkbox.visual(None, bounds(), true, true),
            UiCheckboxVisual::Checked
        );
        assert_eq!(
            checkbox.visual(Some(inside_point()), bounds(), true, true),
            UiCheckboxVisual::CheckedHover
        );
        checkbox.apply(ElementState::Pressed, Some(inside_point()), bounds(), true);
        assert_eq!(
            checkbox.apply(
                ElementState::Released,
                Some(outside_point()),
                bounds(),
                true
            ),
            UiPointerOutcome::Captured
        );
    }

    #[test]
    fn disabled_checkbox_does_not_toggle() {
        let mut checkbox = UiCheckbox::default();
        assert_eq!(
            checkbox.visual(Some(inside_point()), bounds(), false, false),
            UiCheckboxVisual::Disabled
        );
        assert_eq!(
            checkbox.apply(ElementState::Pressed, Some(inside_point()), bounds(), false),
            UiPointerOutcome::Captured
        );
        assert!(!checkbox.is_pressed());
        assert_eq!(
            checkbox.apply(
                ElementState::Released,
                Some(inside_point()),
                bounds(),
                false
            ),
            UiPointerOutcome::Idle
        );
    }

    #[test]
    fn radio_selects_one_option_and_keeps_current_selection() {
        let low = ScreenRect {
            min: [0.0, 0.0],
            max: [40.0, 16.0],
        };
        let medium = ScreenRect {
            min: [0.0, 20.0],
            max: [40.0, 36.0],
        };
        let high = ScreenRect {
            min: [0.0, 40.0],
            max: [40.0, 56.0],
        };
        let disabled = ScreenRect {
            min: [0.0, 60.0],
            max: [40.0, 76.0],
        };
        let options = [(low, true), (medium, true), (high, true), (disabled, false)];
        let mut group = UiRadioGroup::default();
        assert_eq!(group.visual(1, true, None, medium, true), UiRadioVisual::On);
        assert_eq!(
            group.visual(0, false, Some([10.0, 8.0]), low, true),
            UiRadioVisual::Hover
        );
        assert_eq!(
            group.apply(ElementState::Pressed, Some([10.0, 8.0]), &options),
            UiRadioOutcome::Captured
        );
        assert_eq!(
            group.apply(ElementState::Released, Some([10.0, 8.0]), &options),
            UiRadioOutcome::Selected(0)
        );
        assert_eq!(
            group.apply(ElementState::Pressed, Some([200.0, 8.0]), &options),
            UiRadioOutcome::Idle
        );
        group.apply(ElementState::Pressed, Some([10.0, 8.0]), &options);
        assert_eq!(
            group.apply(ElementState::Released, Some([10.0, 8.0]), &options),
            UiRadioOutcome::Selected(0)
        );
        group.apply(ElementState::Pressed, Some([10.0, 48.0]), &options);
        let selected = match group.apply(ElementState::Released, Some([10.0, 48.0]), &options) {
            UiRadioOutcome::Selected(index) => index,
            other => panic!("expected a radio selection, got {other:?}"),
        };
        assert_eq!(selected, 2);
        assert_eq!(
            group.visual(3, false, Some([10.0, 68.0]), disabled, false),
            UiRadioVisual::Disabled
        );
        assert_eq!(
            group.apply(ElementState::Pressed, Some([10.0, 68.0]), &options),
            UiRadioOutcome::Captured
        );
        assert!(!group.is_pressed());
        assert_eq!(
            group.apply(ElementState::Released, Some([10.0, 68.0]), &options),
            UiRadioOutcome::Idle
        );
        assert_eq!(selected, 2);
    }

    #[test]
    fn tab_visual_prefers_pressed_then_selected_then_hover() {
        assert_eq!(tab_visual(true, true, true), UiTabVisual::Pressed);
        assert_eq!(tab_visual(true, true, false), UiTabVisual::Active);
        assert_eq!(tab_visual(false, true, false), UiTabVisual::Hover);
        assert_eq!(tab_visual(false, false, false), UiTabVisual::Inactive);
    }
}
