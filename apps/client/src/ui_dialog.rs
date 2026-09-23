//! Reusable modal message dialog UI.

use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::renderer::{PixelViewport, TextAlignment, TextBlock, TextContent, TextStyle};
use crate::ui_panel::{ScreenRect, UiButtonAssets, UiButtonState, UiMessageChrome, UiWindowAssets};

const BODY_FONT_SIZE: f32 = 12.0;
const BUTTON_FONT_SIZE: f32 = 11.0;
const SIDE_INSET: f32 = 24.0;
const BODY_TOP: f32 = 52.0;
const BUTTON_BOTTOM: f32 = 12.0;
const BUTTON_GAP: f32 = 8.0;

#[allow(dead_code)] // Custom actions are consumed by future callers without UI callbacks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DialogAction {
    Cancel,
    Ok,
    Confirm,
    Custom(u16),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DialogButton {
    pub(crate) label: String,
    pub(crate) action: DialogAction,
}

impl DialogButton {
    pub(crate) fn new(label: impl Into<String>, action: DialogAction) -> Self {
        Self {
            label: label.into(),
            action,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MessageDialogRequest {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) buttons: Vec<DialogButton>,
    pub(crate) default_action: Option<DialogAction>,
    pub(crate) cancel_action: Option<DialogAction>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MessageDialogResult {
    pub(crate) id: u64,
    pub(crate) action: DialogAction,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MessageDialog {
    active: Option<MessageDialogRequest>,
    result: Option<MessageDialogResult>,
    pressed_button: Option<usize>,
    button_bounds: Vec<ScreenRect>,
    close_button: Option<ScreenRect>,
}

impl MessageDialog {
    pub(crate) fn open(&mut self, request: MessageDialogRequest) -> bool {
        if self.active.is_some() || self.result.is_some() {
            return false;
        }
        if !(1..=3).contains(&request.buttons.len()) {
            return false;
        }
        self.active = Some(request);
        self.pressed_button = None;
        self.button_bounds.clear();
        self.close_button = None;
        true
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn close(&mut self) {
        self.active = None;
        self.pressed_button = None;
        self.button_bounds.clear();
        self.close_button = None;
    }

    pub(crate) fn cancel(&mut self) -> bool {
        if !self.is_active() {
            return false;
        }
        if let Some(action) = self
            .active
            .as_ref()
            .and_then(|request| request.cancel_action)
        {
            self.finish(action);
        } else {
            self.close();
        }
        true
    }

    pub(crate) fn take_result(&mut self) -> Option<MessageDialogResult> {
        self.result.take()
    }

    pub(crate) fn apply_key(
        &mut self,
        physical_key: PhysicalKey,
        state: ElementState,
        repeat: bool,
    ) -> bool {
        if !self.is_active() {
            return false;
        }
        if state != ElementState::Pressed || repeat {
            return true;
        }
        let action = match physical_key {
            PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
                self.active.as_ref().and_then(|request| {
                    request
                        .default_action
                        .or_else(|| request.buttons.first().map(|button| button.action))
                })
            }
            PhysicalKey::Code(KeyCode::Escape) => {
                self.cancel();
                None
            }
            _ => None,
        };
        if let Some(action) = action {
            self.finish(action);
        }
        true
    }

    pub(crate) fn pointer_moved(&mut self, cursor: [f32; 2]) -> bool {
        self.is_active()
            && (self.close_button.is_some()
                || self.button_bounds.iter().any(|b| b.contains(cursor)))
    }

    pub(crate) fn apply_pointer_button(
        &mut self,
        state: ElementState,
        cursor: Option<[f32; 2]>,
    ) -> bool {
        if !self.is_active() {
            return false;
        }
        let Some(cursor) = cursor else {
            self.pressed_button = None;
            return true;
        };
        match state {
            ElementState::Pressed => {
                if self
                    .close_button
                    .is_some_and(|bounds| bounds.contains(cursor))
                {
                    self.pressed_button = None;
                    return true;
                }
                self.pressed_button = self
                    .button_bounds
                    .iter()
                    .position(|bounds| bounds.contains(cursor));
            }
            ElementState::Released => {
                if let Some(index) = self.pressed_button.take()
                    && self
                        .button_bounds
                        .get(index)
                        .is_some_and(|b| b.contains(cursor))
                    && let Some(action) = self
                        .active
                        .as_ref()
                        .and_then(|request| request.buttons.get(index))
                        .map(|button| button.action)
                {
                    self.finish(action);
                } else if self
                    .close_button
                    .is_some_and(|bounds| bounds.contains(cursor))
                {
                    self.cancel();
                }
            }
        }
        true
    }

    pub(crate) fn frame(
        &mut self,
        assets: UiWindowAssets,
        button_assets: UiButtonAssets,
        viewport: PixelViewport,
        pixels_per_unit: f32,
        cursor: Option<[f32; 2]>,
    ) -> Result<Option<MessageDialogFrame>, String> {
        let Some(request) = self.active.as_ref() else {
            self.button_bounds.clear();
            self.close_button = None;
            return Ok(None);
        };
        let Some(UiMessageChrome {
            frame: chrome,
            window,
            close_button,
        }) = assets.message_chrome(&request.title, viewport, pixels_per_unit, cursor)?
        else {
            return Ok(None);
        };
        let content_width = (window.width() - 2.0 * SIDE_INSET * pixels_per_unit).max(1.0);

        let body_anchor = [
            window.min[0] + SIDE_INSET * pixels_per_unit,
            window.min[1] + BODY_TOP * pixels_per_unit,
        ];
        let button_width = ((content_width
            - BUTTON_GAP * pixels_per_unit * (request.buttons.len() - 1) as f32)
            / request.buttons.len() as f32)
            .max(1.0);
        let button_height = button_assets.height_units * pixels_per_unit;
        let button_y = window.max[1] - BUTTON_BOTTOM * pixels_per_unit - button_height;
        self.button_bounds = request
            .buttons
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let x = window.min[0]
                    + SIDE_INSET * pixels_per_unit
                    + index as f32 * (button_width + BUTTON_GAP * pixels_per_unit);
                ScreenRect {
                    min: [x, button_y],
                    max: [x + button_width, button_y + button_height],
                }
            })
            .collect();
        self.close_button = Some(close_button);
        let mut textured_rects = chrome.textured_rects;
        let mut texts = vec![
            chrome.title,
            TextBlock {
                content: TextContent(request.body.clone()),
                style: TextStyle::at_size(
                    BODY_FONT_SIZE,
                    [0.08, 0.11, 0.16, 1.0],
                    TextAlignment::Left,
                ),
                anchor: body_anchor,
                max_width: Some(content_width),
            },
        ];
        for (index, button) in request.buttons.iter().enumerate() {
            let bounds = self.button_bounds[index];
            let hovered = cursor.is_some_and(|point| bounds.contains(point));
            let state = if self.pressed_button == Some(index) && hovered {
                UiButtonState::Pressed
            } else if hovered {
                UiButtonState::Hover
            } else {
                UiButtonState::Normal
            };
            textured_rects.extend(button_assets.frame(bounds, state, pixels_per_unit)?);
            texts.push(TextBlock {
                content: TextContent(button.label.clone()),
                style: TextStyle::at_size(
                    BUTTON_FONT_SIZE,
                    [0.05, 0.07, 0.11, 1.0],
                    TextAlignment::Center,
                ),
                anchor: [
                    (bounds.min[0] + bounds.max[0]) * 0.5,
                    bounds.min[1] + (bounds.height() - BUTTON_FONT_SIZE * pixels_per_unit) * 0.5,
                ],
                max_width: Some(bounds.width()),
            });
        }
        Ok(Some(MessageDialogFrame {
            textured_rects,
            texts,
        }))
    }

    fn finish(&mut self, action: DialogAction) {
        if let Some(request) = self.active.take() {
            self.result = Some(MessageDialogResult {
                id: request.id,
                action,
            });
            self.pressed_button = None;
            self.button_bounds.clear();
            self.close_button = None;
        }
    }
}

pub(crate) struct MessageDialogFrame {
    pub(crate) textured_rects: Vec<crate::renderer::UiTexturedRect>,
    pub(crate) texts: Vec<TextBlock>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> MessageDialogRequest {
        MessageDialogRequest {
            id: 7,
            title: "Title".into(),
            body: "Body".into(),
            buttons: vec![
                DialogButton::new("Cancel", DialogAction::Cancel),
                DialogButton::new("OK", DialogAction::Ok),
            ],
            default_action: Some(DialogAction::Ok),
            cancel_action: Some(DialogAction::Cancel),
        }
    }

    #[test]
    fn button_result() {
        let mut dialog = MessageDialog::default();
        assert!(dialog.open(request()));
        dialog.button_bounds = vec![
            ScreenRect {
                min: [0.0, 0.0],
                max: [10.0, 10.0],
            },
            ScreenRect {
                min: [20.0, 0.0],
                max: [30.0, 10.0],
            },
        ];
        dialog.apply_pointer_button(ElementState::Pressed, Some([25.0, 5.0]));
        dialog.apply_pointer_button(ElementState::Released, Some([25.0, 5.0]));
        assert_eq!(
            dialog.take_result(),
            Some(MessageDialogResult {
                id: 7,
                action: DialogAction::Ok
            })
        );
    }

    #[test]
    fn escape_cancel_and_enter_default() {
        let mut dialog = MessageDialog::default();
        assert!(dialog.open(request()));
        assert!(dialog.apply_key(
            PhysicalKey::Code(KeyCode::Escape),
            ElementState::Pressed,
            false
        ));
        assert_eq!(
            dialog.take_result().map(|r| r.action),
            Some(DialogAction::Cancel)
        );
        assert!(dialog.open(request()));
        assert!(dialog.apply_key(
            PhysicalKey::Code(KeyCode::Enter),
            ElementState::Pressed,
            false
        ));
        assert_eq!(
            dialog.take_result().map(|r| r.action),
            Some(DialogAction::Ok)
        );
    }

    #[test]
    fn cancel_is_the_shared_semantic_exit_for_escape_and_close() {
        let mut escape = MessageDialog::default();
        assert!(escape.open(request()));
        assert!(escape.apply_key(
            PhysicalKey::Code(KeyCode::Escape),
            ElementState::Pressed,
            false
        ));

        let mut close = MessageDialog::default();
        assert!(close.open(request()));
        assert!(close.cancel());

        assert!(!escape.is_active());
        assert!(!close.is_active());
        assert_eq!(
            escape.take_result().map(|result| result.action),
            Some(DialogAction::Cancel)
        );
        assert_eq!(
            close.take_result().map(|result| result.action),
            Some(DialogAction::Cancel)
        );
    }

    #[test]
    fn active_dialog_is_not_replaced() {
        let mut dialog = MessageDialog::default();
        assert!(dialog.open(request()));
        let mut replacement = request();
        replacement.id = 8;
        assert!(!dialog.open(replacement));
        assert!(dialog.apply_key(
            PhysicalKey::Code(KeyCode::Escape),
            ElementState::Pressed,
            false
        ));
        assert_eq!(dialog.take_result().map(|r| r.id), Some(7));
    }

    #[test]
    fn modal_captures_unhandled_input() {
        let mut dialog = MessageDialog::default();
        assert!(dialog.open(request()));
        assert!(dialog.apply_key(
            PhysicalKey::Code(KeyCode::KeyA),
            ElementState::Pressed,
            false
        ));
        assert!(dialog.apply_pointer_button(ElementState::Pressed, Some([500.0, 500.0])));
    }
}
