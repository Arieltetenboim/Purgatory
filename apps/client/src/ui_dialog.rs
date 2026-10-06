//! Reusable modal message dialog UI.

use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::asset_runtime::AssetRuntime;
use crate::assets::ClientAssetLoader;
use crate::renderer::{
    PixelViewport, SpriteTextureId, TextAlignment, TextBlock, TextContent, TextStyle,
    UiTexturedQuad,
};
use crate::ui_panel::{ScreenRect, compose_nine_slice_with_borders, compose_stretched_quad};
use crate::ui_v2::{
    UiV2Image, UiV2NineSlice, load_ui_v2_catalog, load_ui_v2_nine_slice, load_ui_v2_state_family,
};

const BODY_FONT_SIZE: f32 = 12.0;
const BUTTON_FONT_SIZE: f32 = 11.0;
const TITLE_FONT_SIZE: f32 = 13.0;
const SIDE_INSET_UNITS: f32 = 24.0;
const BUTTON_BOTTOM_UNITS: f32 = 12.0;
const BUTTON_GAP_UNITS: f32 = 8.0;
const BUTTON_HEIGHT_UNITS: f32 = 22.0;
const BUTTON_ROW_GAP_UNITS: f32 = 8.0;
/// Vertical room under the header before the button row. Matches the previous
/// message-chrome body span so existing copy keeps the same max-width idea.
const BODY_REGION_UNITS: f32 = 98.0;
const BODY_GAP_UNITS: f32 = 8.0;
const HEADER_TOP_INSET_UNITS: f32 = 6.0;
const HEADER_HEIGHT_UNITS: f32 = 34.0;
const HEADER_SIDE_INSET_UNITS: f32 = 8.0;
const TITLE_SIDE_INSET_UNITS: f32 = 8.0;
const TITLE_CONTROL_GAP_UNITS: f32 = 5.0;
const CLOSE_SIZE_UNITS: f32 = 18.0;
const CLOSE_INSET_UNITS: f32 = 4.0;
const CLOSE_RIGHT_INSET_UNITS: f32 = 6.0;
const DIALOG_WIDTH_UNITS: f32 = 360.0;
const DIALOG_HEIGHT_UNITS: f32 = HEADER_TOP_INSET_UNITS
    + HEADER_HEIGHT_UNITS
    + BODY_GAP_UNITS
    + BODY_REGION_UNITS
    + BUTTON_ROW_GAP_UNITS
    + BUTTON_HEIGHT_UNITS
    + BUTTON_BOTTOM_UNITS;
const PANEL_BORDER_UNITS: [f32; 4] = [10.0, 10.0, 10.0, 12.0];
const HEADER_BORDER_UNITS: [f32; 4] = [6.0, 6.0, 6.0, 6.0];
const PANEL_ASSET: &str = "panel_body_9slice";
const HEADER_ASSET: &str = "panel_header_9slice";
const CLOSE_ASSETS: [&str; 3] = [
    "close_button_normal",
    "close_button_hover",
    "close_button_pressed",
];
const BUTTON_ASSETS: [&str; 3] = ["button_normal", "button_hover", "button_pressed"];
const BODY_COLOR: [f32; 4] = [0.08, 0.11, 0.16, 1.0];
const BUTTON_LABEL_COLOR: [f32; 4] = [0.05, 0.07, 0.11, 1.0];
const TITLE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

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
    /// When false, Escape and the title-bar close control do not dismiss the dialog.
    pub(crate) dismissible: bool,
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
    close_pressed: bool,
    button_bounds: Vec<ScreenRect>,
    close_button: Option<ScreenRect>,
}

pub(crate) struct MessageDialogV2Assets {
    panel: UiV2NineSlice,
    header: UiV2NineSlice,
    close: [UiV2Image; 3],
    buttons: [UiV2Image; 3],
}

impl MessageDialogV2Assets {
    pub(crate) fn load(runtime: &mut AssetRuntime) -> Result<Self, String> {
        let catalog = load_ui_v2_catalog(runtime)?;
        let mut loader = ClientAssetLoader::new(runtime);
        let panel = load_ui_v2_nine_slice(&mut loader, &catalog, PANEL_ASSET)?;
        let header = load_ui_v2_nine_slice(&mut loader, &catalog, HEADER_ASSET)?;
        let close = array3(load_ui_v2_state_family(
            &mut loader,
            &catalog,
            &CLOSE_ASSETS,
        )?)?;
        let buttons = array3(load_ui_v2_state_family(
            &mut loader,
            &catalog,
            &BUTTON_ASSETS,
        )?)?;
        Ok(Self {
            panel,
            header,
            close,
            buttons,
        })
    }
}

struct MessageDialogLayout {
    window: ScreenRect,
    header: ScreenRect,
    close_button: Option<ScreenRect>,
    body_bounds: ScreenRect,
    buttons: Vec<ScreenRect>,
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
        self.close_pressed = false;
        self.button_bounds.clear();
        self.close_button = None;
        true
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn active_id(&self) -> Option<u64> {
        self.active.as_ref().map(|request| request.id)
    }

    #[cfg(test)]
    pub(crate) fn active_body(&self) -> Option<&str> {
        self.active.as_ref().map(|request| request.body.as_str())
    }

    pub(crate) fn is_dismissible(&self) -> bool {
        self.active
            .as_ref()
            .is_none_or(|request| request.dismissible)
    }

    pub(crate) fn close(&mut self) {
        self.active = None;
        self.pressed_button = None;
        self.close_pressed = false;
        self.button_bounds.clear();
        self.close_button = None;
    }

    pub(crate) fn cancel(&mut self) -> bool {
        if !self.is_active() {
            return false;
        }
        if !self.is_dismissible() {
            return true;
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
                if self.is_dismissible() {
                    self.cancel();
                }
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
            self.close_pressed = false;
            return true;
        };
        match state {
            ElementState::Pressed => {
                if self
                    .close_button
                    .is_some_and(|bounds| bounds.contains(cursor))
                {
                    self.pressed_button = None;
                    self.close_pressed = true;
                    return true;
                }
                self.close_pressed = false;
                self.pressed_button = self
                    .button_bounds
                    .iter()
                    .position(|bounds| bounds.contains(cursor));
            }
            ElementState::Released => {
                self.close_pressed = false;
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
                } else if self.is_dismissible()
                    && self
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
        assets: &MessageDialogV2Assets,
        viewport: PixelViewport,
        pixels_per_unit: f32,
        cursor: Option<[f32; 2]>,
    ) -> Result<Option<MessageDialogFrame>, String> {
        let Some(request) = self.active.as_ref() else {
            self.button_bounds.clear();
            self.close_button = None;
            self.close_pressed = false;
            return Ok(None);
        };
        let layout = layout_message_dialog(
            viewport,
            pixels_per_unit,
            request.buttons.len(),
            request.dismissible,
        )?;
        self.button_bounds.clone_from(&layout.buttons);
        self.close_button = layout.close_button;
        let mut skin_quads = Vec::new();
        push_nine_slice(
            &mut skin_quads,
            layout.window,
            &assets.panel,
            PANEL_BORDER_UNITS,
            pixels_per_unit,
        )?;
        push_nine_slice(
            &mut skin_quads,
            layout.header,
            &assets.header,
            HEADER_BORDER_UNITS,
            pixels_per_unit,
        )?;
        let mut texts = vec![
            title_text(&layout, &request.title, pixels_per_unit),
            TextBlock {
                content: TextContent(request.body.clone()),
                style: TextStyle::at_size(BODY_FONT_SIZE, BODY_COLOR, TextAlignment::Left),
                anchor: layout.body_bounds.min,
                max_width: Some(layout.body_bounds.width()),
            },
        ];
        for (index, button) in request.buttons.iter().enumerate() {
            let bounds = layout.buttons[index];
            let texture =
                button_texture(assets, bounds, cursor, self.pressed_button == Some(index));
            skin_quads.push(compose_stretched_quad(bounds, texture)?);
            texts.push(TextBlock {
                content: TextContent(button.label.clone()),
                style: TextStyle::at_size(
                    BUTTON_FONT_SIZE,
                    BUTTON_LABEL_COLOR,
                    TextAlignment::Center,
                ),
                anchor: [
                    (bounds.min[0] + bounds.max[0]) * 0.5,
                    bounds.min[1] + (bounds.height() - BUTTON_FONT_SIZE * pixels_per_unit) * 0.5,
                ],
                max_width: Some(bounds.width()),
            });
        }
        if let Some(bounds) = layout.close_button {
            let texture = close_texture(assets, bounds, cursor, self.close_pressed);
            skin_quads.push(compose_stretched_quad(bounds, texture)?);
        }
        Ok(Some(MessageDialogFrame { skin_quads, texts }))
    }

    fn finish(&mut self, action: DialogAction) {
        if let Some(request) = self.active.take() {
            self.result = Some(MessageDialogResult {
                id: request.id,
                action,
            });
            self.pressed_button = None;
            self.close_pressed = false;
            self.button_bounds.clear();
            self.close_button = None;
        }
    }
}

pub(crate) struct MessageDialogFrame {
    pub(crate) skin_quads: Vec<UiTexturedQuad>,
    pub(crate) texts: Vec<TextBlock>,
}

fn layout_message_dialog(
    viewport: PixelViewport,
    pixels_per_unit: f32,
    button_count: usize,
    dismissible: bool,
) -> Result<MessageDialogLayout, String> {
    if !pixels_per_unit.is_finite() || pixels_per_unit <= 0.0 {
        return Err("message dialog pixels-per-unit must be finite and positive".to_string());
    }
    if !(1..=3).contains(&button_count) {
        return Err("message dialog requires 1 to 3 buttons".to_string());
    }
    let scale = pixels_per_unit;
    let window = place_dialog_window(viewport, scale);
    let header = ScreenRect {
        min: [
            window.min[0] + HEADER_SIDE_INSET_UNITS * scale,
            window.min[1] + HEADER_TOP_INSET_UNITS * scale,
        ],
        max: [
            window.max[0] - HEADER_SIDE_INSET_UNITS * scale,
            window.min[1] + (HEADER_TOP_INSET_UNITS + HEADER_HEIGHT_UNITS) * scale,
        ],
    };
    let close_button = dismissible.then(|| {
        let close_size = CLOSE_SIZE_UNITS * scale;
        let close_max_x = header.max[0] - CLOSE_RIGHT_INSET_UNITS * scale;
        let close_min_y = header.min[1] + CLOSE_INSET_UNITS * scale;
        ScreenRect {
            min: [close_max_x - close_size, close_min_y],
            max: [close_max_x, close_min_y + close_size],
        }
    });
    let body_min_y = header.max[1] + BODY_GAP_UNITS * scale;
    let body_bounds = ScreenRect {
        min: [window.min[0] + SIDE_INSET_UNITS * scale, body_min_y],
        max: [
            window.max[0] - SIDE_INSET_UNITS * scale,
            body_min_y + BODY_REGION_UNITS * scale,
        ],
    };
    let button_height = BUTTON_HEIGHT_UNITS * scale;
    let button_y = window.max[1] - BUTTON_BOTTOM_UNITS * scale - button_height;
    let gap = BUTTON_GAP_UNITS * scale;
    let button_width =
        ((body_bounds.width() - gap * (button_count - 1) as f32) / button_count as f32).max(1.0);
    let buttons = (0..button_count)
        .map(|index| {
            let x = body_bounds.min[0] + index as f32 * (button_width + gap);
            ScreenRect {
                min: [x, button_y],
                max: [x + button_width, button_y + button_height],
            }
        })
        .collect::<Vec<_>>();
    let stacked_button_y = body_bounds.max[1] + BUTTON_ROW_GAP_UNITS * scale;
    let buttons_fit = buttons.iter().all(|button| {
        button.min[0] >= body_bounds.min[0]
            && button.max[0] <= body_bounds.max[0] + 0.05
            && button.max[1] <= window.max[1]
            && button.width() > 0.0
            && button.height() > 0.0
    });
    let close_fits = close_button.is_none_or(|close| {
        close.min[0] >= header.min[0]
            && close.max[0] <= header.max[0]
            && close.min[1] >= header.min[1]
            && close.max[1] <= header.max[1]
    });
    if !buttons_fit
        || !close_fits
        || header.width() <= 0.0
        || header.height() <= 0.0
        || body_bounds.width() <= 0.0
        || (button_y - stacked_button_y).abs() > 0.6
    {
        return Err("message dialog layout does not fit its window".to_string());
    }
    Ok(MessageDialogLayout {
        window,
        header,
        close_button,
        body_bounds,
        buttons,
    })
}

fn place_dialog_window(viewport: PixelViewport, pixels_per_unit: f32) -> ScreenRect {
    let size = [DIALOG_WIDTH_UNITS, DIALOG_HEIGHT_UNITS];
    let viewport_size = [
        viewport.width as f32 / pixels_per_unit,
        viewport.height as f32 / pixels_per_unit,
    ];
    let top_left = [
        ((viewport_size[0] - size[0]) * 0.5).max(0.0),
        ((viewport_size[1] - size[1]) * 0.5).max(0.0),
    ];
    let min = [
        viewport.x as f32 + top_left[0] * pixels_per_unit,
        viewport.y as f32 + top_left[1] * pixels_per_unit,
    ];
    ScreenRect {
        min,
        max: [
            min[0] + size[0] * pixels_per_unit,
            min[1] + size[1] * pixels_per_unit,
        ],
    }
}

fn title_text(layout: &MessageDialogLayout, title: &str, pixels_per_unit: f32) -> TextBlock {
    let font_px = TITLE_FONT_SIZE * pixels_per_unit;
    let anchor = [
        layout.header.min[0] + TITLE_SIDE_INSET_UNITS * pixels_per_unit,
        layout.header.min[1] + (layout.header.height() - font_px).max(0.0) * 0.5,
    ];
    let right = layout
        .close_button
        .map(|close| close.min[0] - TITLE_CONTROL_GAP_UNITS * pixels_per_unit)
        .unwrap_or(layout.header.max[0] - TITLE_SIDE_INSET_UNITS * pixels_per_unit);
    TextBlock {
        content: TextContent(title.to_owned()),
        style: TextStyle::at_size(TITLE_FONT_SIZE, TITLE_COLOR, TextAlignment::Left),
        anchor,
        max_width: Some((right - anchor[0]).max(1.0)),
    }
}

fn button_texture(
    assets: &MessageDialogV2Assets,
    bounds: ScreenRect,
    cursor: Option<[f32; 2]>,
    pressed: bool,
) -> SpriteTextureId {
    let hovered = cursor.is_some_and(|point| bounds.contains(point));
    if hovered && pressed {
        assets.buttons[2].texture
    } else if hovered {
        assets.buttons[1].texture
    } else {
        assets.buttons[0].texture
    }
}

fn close_texture(
    assets: &MessageDialogV2Assets,
    bounds: ScreenRect,
    cursor: Option<[f32; 2]>,
    pressed: bool,
) -> SpriteTextureId {
    let hovered = cursor.is_some_and(|point| bounds.contains(point));
    if hovered && pressed {
        assets.close[2].texture
    } else if hovered {
        assets.close[1].texture
    } else {
        assets.close[0].texture
    }
}

fn push_nine_slice(
    quads: &mut Vec<UiTexturedQuad>,
    bounds: ScreenRect,
    asset: &UiV2NineSlice,
    border_units: [f32; 4],
    pixels_per_unit: f32,
) -> Result<(), String> {
    quads.extend(compose_nine_slice_with_borders(
        bounds.min,
        [
            bounds.width() / pixels_per_unit,
            bounds.height() / pixels_per_unit,
        ],
        pixels_per_unit,
        asset.texture,
        asset.size_px,
        asset.slice_ltrb,
        border_units,
    )?);
    Ok(())
}

fn array3(images: Vec<UiV2Image>) -> Result<[UiV2Image; 3], String> {
    images
        .try_into()
        .map_err(|_| "UI V2 state family length is not 3".to_string())
}

#[cfg(test)]
fn quad_covers(quad: &UiTexturedQuad, bounds: ScreenRect) -> bool {
    quad.corners[0] == bounds.min && quad.corners[2] == bounds.max
}

#[cfg(test)]
fn full_image_uv(quad: &UiTexturedQuad) -> bool {
    quad.uvs == [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
        && quad.tint == [1.0, 1.0, 1.0, 1.0]
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
            dismissible: true,
        }
    }

    fn viewport() -> PixelViewport {
        PixelViewport {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        }
    }

    fn loaded_assets() -> (AssetRuntime, MessageDialogV2Assets) {
        let mut runtime = AssetRuntime::new();
        let assets = MessageDialogV2Assets::load(&mut runtime).unwrap();
        (runtime, assets)
    }

    fn button_quads<'a>(
        frame: &'a MessageDialogFrame,
        assets: &MessageDialogV2Assets,
    ) -> Vec<&'a UiTexturedQuad> {
        let textures = assets.buttons.map(|image| image.texture);
        frame
            .skin_quads
            .iter()
            .filter(|quad| textures.contains(&quad.texture))
            .collect()
    }

    fn close_quads<'a>(
        frame: &'a MessageDialogFrame,
        assets: &MessageDialogV2Assets,
    ) -> Vec<&'a UiTexturedQuad> {
        let textures = assets.close.map(|image| image.texture);
        frame
            .skin_quads
            .iter()
            .filter(|quad| textures.contains(&quad.texture))
            .collect()
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
    fn release_off_button_does_not_finish() {
        let mut dialog = MessageDialog::default();
        assert!(dialog.open(request()));
        dialog.button_bounds = vec![ScreenRect {
            min: [0.0, 0.0],
            max: [10.0, 10.0],
        }];
        dialog.apply_pointer_button(ElementState::Pressed, Some([5.0, 5.0]));
        dialog.apply_pointer_button(ElementState::Released, Some([40.0, 40.0]));
        assert!(dialog.is_active());
        assert!(dialog.take_result().is_none());
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
    fn enter_without_default_uses_first_button() {
        let mut dialog = MessageDialog::default();
        let mut request = request();
        request.default_action = None;
        assert!(dialog.open(request));
        assert!(dialog.apply_key(
            PhysicalKey::Code(KeyCode::NumpadEnter),
            ElementState::Pressed,
            false
        ));
        assert_eq!(
            dialog.take_result().map(|result| result.action),
            Some(DialogAction::Cancel)
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
    fn non_dismissible_dialog_ignores_escape() {
        let mut dialog = MessageDialog::default();
        let request = MessageDialogRequest {
            id: 99,
            title: "Notice".into(),
            body: "You have died.".into(),
            buttons: vec![DialogButton::new("OK", DialogAction::Confirm)],
            default_action: Some(DialogAction::Confirm),
            cancel_action: None,
            dismissible: false,
        };
        assert!(dialog.open(request));
        assert!(dialog.apply_key(
            PhysicalKey::Code(KeyCode::Escape),
            ElementState::Pressed,
            false
        ));
        assert!(dialog.is_active());
        assert!(dialog.take_result().is_none());
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

    #[test]
    fn message_dialog_v2_assets_resolve_logical_names() {
        let mut runtime = AssetRuntime::new();
        let catalog = load_ui_v2_catalog(&mut runtime).unwrap();
        for name in [PANEL_ASSET, HEADER_ASSET]
            .into_iter()
            .chain(CLOSE_ASSETS)
            .chain(BUTTON_ASSETS)
        {
            catalog
                .get(name)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
        }
        let assets = MessageDialogV2Assets::load(&mut runtime).unwrap();
        assert_eq!(assets.panel.size_px, [288, 192]);
        assert_eq!(assets.header.size_px, [270, 40]);
        assert!(assets.close.iter().all(|image| image.size_px == [36, 36]));
        assert!(
            assets
                .buttons
                .iter()
                .all(|image| image.size_px == [112, 44])
        );
    }

    #[test]
    fn layout_fits_one_two_and_three_buttons() {
        for (count, dismissible) in [(1, false), (2, true), (3, true)] {
            let layout = layout_message_dialog(viewport(), 1.0, count, dismissible).unwrap();
            assert!((layout.window.width() - DIALOG_WIDTH_UNITS).abs() < 0.01);
            assert!((layout.window.height() - DIALOG_HEIGHT_UNITS).abs() < 0.01);
            assert!((layout.header.height() - HEADER_HEIGHT_UNITS).abs() < 0.01);
            assert_eq!(layout.buttons.len(), count);
            assert_eq!(layout.close_button.is_some(), dismissible);
            let width = layout.buttons[0].width();
            assert!(
                layout
                    .buttons
                    .iter()
                    .all(|button| (button.width() - width).abs() < 0.01)
            );
            for pair in layout.buttons.windows(2) {
                assert!((pair[1].min[0] - pair[0].max[0] - BUTTON_GAP_UNITS).abs() < 0.01);
                assert!(pair[0].min[0] < pair[1].min[0]);
            }
            if let Some(close) = layout.close_button {
                assert!(close.max[0] <= layout.header.max[0]);
                assert!(close.max[1] <= layout.header.max[1]);
            }
        }
    }

    #[test]
    fn rendered_button_and_close_bounds_match_hit_bounds() {
        let (_runtime, assets) = loaded_assets();
        let mut dialog = MessageDialog::default();
        assert!(dialog.open(request()));
        let frame = dialog
            .frame(&assets, viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        let buttons = button_quads(&frame, &assets);
        assert_eq!(buttons.len(), dialog.button_bounds.len());
        for (quad, bounds) in buttons.iter().zip(dialog.button_bounds.iter()) {
            assert!(quad_covers(quad, *bounds));
            assert!(full_image_uv(quad));
        }
        let close = dialog.close_button.expect("dismissible close");
        let close_quads = close_quads(&frame, &assets);
        assert_eq!(close_quads.len(), 1);
        assert!(quad_covers(close_quads[0], close));
        assert!(full_image_uv(close_quads[0]));
        assert_eq!(frame.texts[0].content.0, "Title");
        assert_eq!(frame.texts[1].content.0, "Body");
        assert_eq!(frame.texts[2].content.0, "Cancel");
        assert_eq!(frame.texts[3].content.0, "OK");
    }

    #[test]
    fn button_states_use_v2_textures() {
        let (_runtime, assets) = loaded_assets();
        let mut dialog = MessageDialog::default();
        assert!(dialog.open(request()));
        let normal = dialog
            .frame(&assets, viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        assert_eq!(
            button_quads(&normal, &assets)[0].texture,
            assets.buttons[0].texture
        );
        let hover_point = [
            (dialog.button_bounds[0].min[0] + dialog.button_bounds[0].max[0]) * 0.5,
            (dialog.button_bounds[0].min[1] + dialog.button_bounds[0].max[1]) * 0.5,
        ];
        let hover = dialog
            .frame(&assets, viewport(), 1.0, Some(hover_point))
            .unwrap()
            .unwrap();
        assert_eq!(
            button_quads(&hover, &assets)[0].texture,
            assets.buttons[1].texture
        );
        dialog.apply_pointer_button(ElementState::Pressed, Some(hover_point));
        let pressed = dialog
            .frame(&assets, viewport(), 1.0, Some(hover_point))
            .unwrap()
            .unwrap();
        assert_eq!(
            button_quads(&pressed, &assets)[0].texture,
            assets.buttons[2].texture
        );
    }

    #[test]
    fn close_states_use_v2_textures() {
        let (_runtime, assets) = loaded_assets();
        let mut dialog = MessageDialog::default();
        assert!(dialog.open(request()));
        let normal = dialog
            .frame(&assets, viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        assert_eq!(
            close_quads(&normal, &assets)[0].texture,
            assets.close[0].texture
        );
        let close = dialog.close_button.expect("close");
        let point = [
            (close.min[0] + close.max[0]) * 0.5,
            (close.min[1] + close.max[1]) * 0.5,
        ];
        let hover = dialog
            .frame(&assets, viewport(), 1.0, Some(point))
            .unwrap()
            .unwrap();
        assert_eq!(
            close_quads(&hover, &assets)[0].texture,
            assets.close[1].texture
        );
        dialog.apply_pointer_button(ElementState::Pressed, Some(point));
        let pressed = dialog
            .frame(&assets, viewport(), 1.0, Some(point))
            .unwrap()
            .unwrap();
        assert_eq!(
            close_quads(&pressed, &assets)[0].texture,
            assets.close[2].texture
        );
        dialog.apply_pointer_button(ElementState::Released, Some(point));
        assert_eq!(
            dialog.take_result().map(|result| result.action),
            Some(DialogAction::Cancel)
        );
    }

    #[test]
    fn non_dismissible_dialog_has_no_close_hit_target() {
        let (_runtime, assets) = loaded_assets();
        let mut dialog = MessageDialog::default();
        let mut request = request();
        request.dismissible = false;
        request.buttons.truncate(1);
        request.cancel_action = None;
        assert!(dialog.open(request));
        let frame = dialog
            .frame(&assets, viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        assert!(dialog.close_button.is_none());
        assert!(close_quads(&frame, &assets).is_empty());
        let header = layout_message_dialog(viewport(), 1.0, 1, false)
            .unwrap()
            .header;
        let point = [header.max[0] - 4.0, header.min[1] + 8.0];
        dialog.apply_pointer_button(ElementState::Pressed, Some(point));
        dialog.apply_pointer_button(ElementState::Released, Some(point));
        assert!(dialog.is_active());
        assert!(dialog.take_result().is_none());
    }

    #[test]
    fn geometry_scales_together() {
        let base = layout_message_dialog(viewport(), 1.0, 3, true).unwrap();
        for scale in [0.9_f32, 1.25] {
            let layout = layout_message_dialog(viewport(), scale, 3, true).unwrap();
            assert!((layout.window.width() - base.window.width() * scale).abs() < 0.05);
            assert!((layout.window.height() - base.window.height() * scale).abs() < 0.05);
            assert!((layout.header.height() - base.header.height() * scale).abs() < 0.05);
            assert!((layout.body_bounds.width() - base.body_bounds.width() * scale).abs() < 0.05);
            let close = layout.close_button.expect("close");
            let base_close = base.close_button.expect("close");
            assert!((close.width() - base_close.width() * scale).abs() < 0.05);
            assert!((layout.buttons[0].width() - base.buttons[0].width() * scale).abs() < 0.05);
            assert!((layout.buttons[0].height() - base.buttons[0].height() * scale).abs() < 0.05);
            let offset = [
                layout.buttons[2].min[0] - layout.window.min[0],
                layout.buttons[2].min[1] - layout.window.min[1],
            ];
            let base_offset = [
                base.buttons[2].min[0] - base.window.min[0],
                base.buttons[2].min[1] - base.window.min[1],
            ];
            assert!((offset[0] - base_offset[0] * scale).abs() < 0.05);
            assert!((offset[1] - base_offset[1] * scale).abs() < 0.05);
        }
    }
}
