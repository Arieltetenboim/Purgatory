//! Reusable modal message dialog UI.

use winit::event::{ElementState, MouseScrollDelta};
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::asset_runtime::AssetRuntime;
use crate::assets::ClientAssetLoader;
use crate::renderer::text::measure_text;
use crate::renderer::{
    PixelViewport, SpriteTextureId, TextAlignment, TextBlock, TextContent, TextStyle,
    UiTexturedQuad,
};
use crate::ui_controls::{
    UiButton, UiButtonVisual, UiPointerOutcome, UiScrollGeometry, UiScrollbar, scroll_geometry,
    scroll_max_offset, scroll_offset_after_wheel, scroll_rows_from_wheel,
};
use crate::ui_panel::{
    ScreenRect, button_label_text, compose_nine_slice_with_borders, compose_stretched_quad,
    compose_v2_text_button, load_v2_text_buttons,
};
use crate::ui_v2::{
    UiV2Image, UiV2NineSlice, load_ui_v2_catalog, load_ui_v2_nine_slice, load_ui_v2_state_family,
};

const BODY_FONT_SIZE: f32 = 12.0;
const BUTTON_FONT_SIZE: f32 = 11.0;
const TITLE_FONT_SIZE: f32 = 13.0;
const SIDE_INSET_UNITS: f32 = 24.0;
const BUTTON_BOTTOM_UNITS: f32 = 12.0;
const BUTTON_GAP_UNITS: f32 = 8.0;
/// Authored V2 button height. Width stays capped at 112; height stays 44 so the
/// face is not squashed.
const BUTTON_HEIGHT_UNITS: f32 = 44.0;
const BUTTON_ROW_GAP_UNITS: f32 = 8.0;
/// Authored V2 button width. Dialog buttons never stretch past this.
const BUTTON_MAX_WIDTH_UNITS: f32 = 112.0;
const BODY_GAP_UNITS: f32 = 8.0;
const SCROLL_GAP_UNITS: f32 = 4.0;
const SCROLL_ARROW_UNITS: f32 = 16.0;
const SCROLL_TRACK_BORDER: [f32; 4] = [8.0, 10.0, 8.0, 10.0];
/// Long copy stops growing and scrolls the body once the window reaches this share of the viewport.
const DIALOG_MAX_HEIGHT_FRACTION: f32 = 0.75;
const HEADER_TOP_INSET_UNITS: f32 = 6.0;
const HEADER_HEIGHT_UNITS: f32 = 34.0;
const HEADER_SIDE_INSET_UNITS: f32 = 8.0;
const TITLE_SIDE_INSET_UNITS: f32 = 8.0;
const TITLE_CONTROL_GAP_UNITS: f32 = 5.0;
const CLOSE_SIZE_UNITS: f32 = 18.0;
const CLOSE_INSET_UNITS: f32 = 4.0;
const CLOSE_RIGHT_INSET_UNITS: f32 = 6.0;
const DIALOG_WIDTH_UNITS: f32 = 360.0;
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
    scroll_offset: usize,
    max_scroll: usize,
    scrollbar: UiScrollbar,
    scroll_up: UiButton,
    scroll_down: UiButton,
    body_bounds: Option<ScreenRect>,
    scroll_geom: Option<UiScrollGeometry>,
}

pub(crate) struct MessageDialogV2Assets {
    panel: UiV2NineSlice,
    header: UiV2NineSlice,
    close: [UiV2Image; 3],
    buttons: [UiV2NineSlice; 3],
    scroll_track: UiV2NineSlice,
    scroll_thumbs: [SpriteTextureId; 4],
    scroll_up: [SpriteTextureId; 3],
    scroll_down: [SpriteTextureId; 3],
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
        let buttons = load_v2_text_buttons(&mut loader, &catalog, &BUTTON_ASSETS)?
            .try_into()
            .map_err(|_| "V2 text button family length is not 3".to_string())?;
        let scroll_track =
            load_ui_v2_nine_slice(&mut loader, &catalog, "scrollbar_track_vertical_9slice")?;
        let thumbs = load_ui_v2_state_family(
            &mut loader,
            &catalog,
            &[
                "scrollbar_thumb_normal",
                "scrollbar_thumb_hover",
                "scrollbar_thumb_pressed",
                "scrollbar_thumb_disabled",
            ],
        )?;
        let scroll_thumbs = thumbs
            .iter()
            .map(|image| image.texture)
            .collect::<Vec<_>>()
            .try_into()
            .map_err(|_| "scrollbar thumb family length is not 4".to_string())?;
        let scroll_up = textures3(load_ui_v2_state_family(
            &mut loader,
            &catalog,
            &[
                "scroll_arrow_up_normal",
                "scroll_arrow_up_hover",
                "scroll_arrow_up_pressed",
            ],
        )?)?;
        let scroll_down = textures3(load_ui_v2_state_family(
            &mut loader,
            &catalog,
            &[
                "scroll_arrow_down_normal",
                "scroll_arrow_down_hover",
                "scroll_arrow_down_pressed",
            ],
        )?)?;
        Ok(Self {
            panel,
            header,
            close,
            buttons,
            scroll_track,
            scroll_thumbs,
            scroll_up,
            scroll_down,
        })
    }
}

struct MessageDialogLayout {
    window: ScreenRect,
    header: ScreenRect,
    close_button: Option<ScreenRect>,
    /// Text viewport. Narrower than the content row when a scrollbar is present.
    body_bounds: ScreenRect,
    /// Full inner width used to center the button row.
    #[cfg_attr(not(test), allow(dead_code))]
    content_bounds: ScreenRect,
    buttons: Vec<ScreenRect>,
    scroll: Option<UiScrollGeometry>,
    line_height: f32,
    scroll_offset: usize,
    max_offset: usize,
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
        self.reset_interaction();
        self.scroll_offset = 0;
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
        self.reset_interaction();
        self.scroll_offset = 0;
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
        if !self.is_active() {
            return false;
        }
        if self.scrollbar.is_dragging()
            && let Some(geometry) = self.scroll_geom
            && let Some(offset) =
                self.scrollbar
                    .pointer_moved(cursor[1], &geometry, self.max_scroll)
        {
            self.scroll_offset = offset;
        }
        self.close_button.is_some()
            || self
                .button_bounds
                .iter()
                .any(|bounds| bounds.contains(cursor))
            || self
                .body_bounds
                .is_some_and(|bounds| bounds.contains(cursor))
            || self
                .scroll_geom
                .is_some_and(|geometry| geometry.region.contains(cursor))
    }

    /// Wheel scrolls the body only while the pointer is over that body.
    pub(crate) fn apply_wheel(
        &mut self,
        delta: MouseScrollDelta,
        cursor: Option<[f32; 2]>,
    ) -> bool {
        if !self.is_active() || self.max_scroll == 0 {
            return false;
        }
        let Some(cursor) = cursor else {
            return false;
        };
        if !self
            .body_bounds
            .is_some_and(|bounds| bounds.contains(cursor))
        {
            return false;
        }
        let next = scroll_offset_after_wheel(
            self.scroll_offset,
            self.max_scroll,
            scroll_rows_from_wheel(delta),
        );
        let changed = next != self.scroll_offset;
        self.scroll_offset = next;
        changed
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
            self.scrollbar.cancel();
            self.scroll_up.cancel();
            self.scroll_down.cancel();
            return true;
        };
        if let Some(geometry) = self.scroll_geom {
            if self.scrollbar.is_dragging() {
                if state == ElementState::Released {
                    let _ = self.scrollbar.apply_thumb(
                        ElementState::Released,
                        Some(cursor),
                        &geometry,
                        self.max_scroll,
                    );
                }
                return true;
            }
            if self.scroll_up.is_pressed() || self.scroll_down.is_pressed() {
                if state == ElementState::Released {
                    if self
                        .scroll_up
                        .apply(ElementState::Released, Some(cursor), geometry.up, true)
                        == UiPointerOutcome::Activated
                    {
                        self.scroll_offset = self.scroll_offset.saturating_sub(1);
                    }
                    if self.scroll_down.apply(
                        ElementState::Released,
                        Some(cursor),
                        geometry.down,
                        true,
                    ) == UiPointerOutcome::Activated
                    {
                        self.scroll_offset = (self.scroll_offset + 1).min(self.max_scroll);
                    }
                }
                return true;
            }
            if state == ElementState::Pressed {
                if self.scrollbar.apply_thumb(
                    ElementState::Pressed,
                    Some(cursor),
                    &geometry,
                    self.max_scroll,
                ) {
                    self.pressed_button = None;
                    self.close_pressed = false;
                    return true;
                }
                if self
                    .scroll_up
                    .apply(ElementState::Pressed, Some(cursor), geometry.up, true)
                    != UiPointerOutcome::Idle
                    || self.scroll_down.apply(
                        ElementState::Pressed,
                        Some(cursor),
                        geometry.down,
                        true,
                    ) != UiPointerOutcome::Idle
                {
                    self.pressed_button = None;
                    self.close_pressed = false;
                    return true;
                }
            }
        }
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
        let Some(request) = self.active.clone() else {
            self.reset_interaction();
            return Ok(None);
        };
        let layout = layout_message_dialog(
            viewport,
            pixels_per_unit,
            &request.body,
            request.buttons.len(),
            request.dismissible,
            self.scroll_offset,
        )?;
        self.scroll_offset = layout.scroll_offset;
        self.max_scroll = layout.max_offset;
        self.body_bounds = Some(layout.body_bounds);
        self.scroll_geom = layout.scroll;
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
                anchor: [
                    layout.body_bounds.min[0],
                    layout.body_bounds.min[1] - layout.scroll_offset as f32 * layout.line_height,
                ],
                max_width: Some(layout.body_bounds.width()),
                clip: Some([
                    layout.body_bounds.min[0],
                    layout.body_bounds.min[1],
                    layout.body_bounds.max[0],
                    layout.body_bounds.max[1],
                ]),
            },
        ];
        for (index, button) in request.buttons.iter().enumerate() {
            let bounds = layout.buttons[index];
            let skin = button_asset(assets, bounds, cursor, self.pressed_button == Some(index));
            skin_quads.extend(compose_v2_text_button(bounds, skin, pixels_per_unit)?);
            texts.push(button_label_text(
                &button.label,
                bounds,
                BUTTON_FONT_SIZE,
                BUTTON_LABEL_COLOR,
                pixels_per_unit,
            ));
        }
        if let Some(bounds) = layout.close_button {
            let texture = close_texture(assets, bounds, cursor, self.close_pressed);
            skin_quads.push(compose_stretched_quad(bounds, texture)?);
        }
        if let Some(geometry) = layout.scroll {
            push_dialog_scroll(
                &mut skin_quads,
                assets,
                geometry,
                DialogScrollPointer {
                    cursor,
                    dragging: self.scrollbar.is_dragging(),
                    up: &self.scroll_up,
                    down: &self.scroll_down,
                },
                pixels_per_unit,
            )?;
        }
        Ok(Some(MessageDialogFrame { skin_quads, texts }))
    }

    fn finish(&mut self, action: DialogAction) {
        if let Some(request) = self.active.take() {
            self.result = Some(MessageDialogResult {
                id: request.id,
                action,
            });
            self.reset_interaction();
            self.scroll_offset = 0;
        }
    }

    fn reset_interaction(&mut self) {
        self.pressed_button = None;
        self.close_pressed = false;
        self.button_bounds.clear();
        self.close_button = None;
        self.max_scroll = 0;
        self.scrollbar.cancel();
        self.scroll_up.cancel();
        self.scroll_down.cancel();
        self.body_bounds = None;
        self.scroll_geom = None;
    }
}

pub(crate) struct MessageDialogFrame {
    pub(crate) skin_quads: Vec<UiTexturedQuad>,
    pub(crate) texts: Vec<TextBlock>,
}

fn layout_message_dialog(
    viewport: PixelViewport,
    pixels_per_unit: f32,
    body: &str,
    button_count: usize,
    dismissible: bool,
    scroll_offset: usize,
) -> Result<MessageDialogLayout, String> {
    if !pixels_per_unit.is_finite() || pixels_per_unit <= 0.0 {
        return Err("message dialog pixels-per-unit must be finite and positive".to_string());
    }
    if !(1..=3).contains(&button_count) {
        return Err("message dialog requires 1 to 3 buttons".to_string());
    }
    let scale = pixels_per_unit;
    let above = (HEADER_TOP_INSET_UNITS + HEADER_HEIGHT_UNITS + BODY_GAP_UNITS) * scale;
    let below = (BUTTON_ROW_GAP_UNITS + BUTTON_HEIGHT_UNITS + BUTTON_BOTTOM_UNITS) * scale;
    let content_width = (DIALOG_WIDTH_UNITS - SIDE_INSET_UNITS * 2.0) * scale;
    let full = measure_dialog_body(body, content_width.max(1.0), scale);
    let max_window = viewport.height as f32 * DIALOG_MAX_HEIGHT_FRACTION;
    let natural = above + full.height + below;
    let mut body_height = if natural <= max_window {
        full.height
    } else {
        (max_window - above - below).max(full.line_height)
    };
    let scroll_column = (SCROLL_GAP_UNITS + SCROLL_ARROW_UNITS) * scale;
    let min_scroll_body = SCROLL_ARROW_UNITS * scale * 2.0 + 1.0;
    let mut text_width = content_width.max(1.0);
    let mut measured = full;
    let mut show_scroll = measured.height > body_height + 0.5;
    if show_scroll {
        if body_height < min_scroll_body {
            body_height = min_scroll_body;
        }
        text_width = (content_width - scroll_column).max(1.0);
        measured = measure_dialog_body(body, text_width, scale);
        show_scroll = measured.height > body_height + 0.5;
        if !show_scroll {
            text_width = content_width.max(1.0);
            measured = full;
        }
    }
    let window_height = above + body_height + below;
    let window = place_dialog_window(viewport, scale, window_height);
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
    let content_bounds = ScreenRect {
        min: [window.min[0] + SIDE_INSET_UNITS * scale, body_min_y],
        max: [
            window.max[0] - SIDE_INSET_UNITS * scale,
            body_min_y + body_height,
        ],
    };
    let body_bounds = ScreenRect {
        min: content_bounds.min,
        max: [content_bounds.min[0] + text_width, content_bounds.max[1]],
    };
    let visible = visible_body_lines(body_height, measured.line_height, measured.line_count);
    let max_offset = if show_scroll {
        scroll_max_offset(measured.line_count, visible)
    } else {
        0
    };
    let scroll_offset = scroll_offset.min(max_offset);
    let scroll = if max_offset > 0 {
        Some(scroll_geometry(
            body_bounds,
            SCROLL_GAP_UNITS * scale,
            SCROLL_ARROW_UNITS * scale,
            scroll_offset,
            measured.line_count,
            visible,
        )?)
    } else {
        None
    };
    let button_height = BUTTON_HEIGHT_UNITS * scale;
    let button_y = content_bounds.max[1] + BUTTON_ROW_GAP_UNITS * scale;
    let buttons =
        place_dialog_buttons(content_bounds, button_count, button_y, button_height, scale);
    let buttons_fit = buttons.iter().all(|button| {
        button.min[0] >= content_bounds.min[0] - 0.05
            && button.max[0] <= content_bounds.max[0] + 0.05
            && button.max[1] <= window.max[1] + 0.05
            && button.min[1] + 0.05 >= content_bounds.max[1]
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
        || body_bounds.height() <= 0.0
    {
        return Err("message dialog layout does not fit its window".to_string());
    }
    Ok(MessageDialogLayout {
        window,
        header,
        close_button,
        body_bounds,
        content_bounds,
        buttons,
        scroll,
        line_height: measured.line_height,
        scroll_offset,
        max_offset,
    })
}

fn place_dialog_buttons(
    body: ScreenRect,
    count: usize,
    button_y: f32,
    button_height: f32,
    scale: f32,
) -> Vec<ScreenRect> {
    let gap = BUTTON_GAP_UNITS * scale;
    let available = (body.width() - gap * count.saturating_sub(1) as f32).max(1.0);
    let equal = available / count as f32;
    let button_width = equal.min(BUTTON_MAX_WIDTH_UNITS * scale).max(1.0);
    let row_width = count as f32 * button_width + gap * count.saturating_sub(1) as f32;
    let slack = body.width() - row_width;
    let row_start = if slack >= 0.0 {
        body.min[0] + slack * 0.5
    } else {
        body.min[0]
    };
    (0..count)
        .map(|index| {
            let x = row_start + index as f32 * (button_width + gap);
            ScreenRect {
                min: [x, button_y],
                max: [x + button_width, button_y + button_height],
            }
        })
        .collect()
}

fn place_dialog_window(
    viewport: PixelViewport,
    pixels_per_unit: f32,
    height_px: f32,
) -> ScreenRect {
    let width_px = DIALOG_WIDTH_UNITS * pixels_per_unit;
    let viewport_size = [viewport.width as f32, viewport.height as f32];
    let top_left = [
        ((viewport_size[0] - width_px) * 0.5).max(0.0),
        ((viewport_size[1] - height_px) * 0.5).max(0.0),
    ];
    let min = [
        viewport.x as f32 + top_left[0],
        viewport.y as f32 + top_left[1],
    ];
    ScreenRect {
        min,
        max: [min[0] + width_px, min[1] + height_px],
    }
}

#[derive(Clone, Copy)]
struct DialogBodyMeasure {
    height: f32,
    line_height: f32,
    line_count: usize,
}

fn measure_dialog_body(text: &str, width: f32, scale: f32) -> DialogBodyMeasure {
    let fallback = BODY_FONT_SIZE * 1.2 * scale;
    if text.is_empty() || !width.is_finite() || width <= 0.0 {
        return DialogBodyMeasure {
            height: fallback,
            line_height: fallback,
            line_count: 1,
        };
    }
    let block = TextBlock {
        content: TextContent(text.to_owned()),
        style: TextStyle::at_size(BODY_FONT_SIZE, BODY_COLOR, TextAlignment::Left),
        anchor: [0.0, 0.0],
        max_width: Some(width),
        clip: None,
    };
    let Some(metrics) = measure_text(&block, scale) else {
        return DialogBodyMeasure {
            height: fallback,
            line_height: fallback,
            line_count: 1,
        };
    };
    let line_count = metrics.line_count.max(1) as usize;
    let height = if metrics.height.is_finite() && metrics.height > 0.0 {
        metrics.height
    } else {
        fallback
    };
    DialogBodyMeasure {
        height,
        line_height: height / line_count as f32,
        line_count,
    }
}

fn visible_body_lines(body_height: f32, line_height: f32, line_count: usize) -> usize {
    if line_height <= 0.0 || !line_height.is_finite() {
        return line_count.max(1);
    }
    let fitted = (body_height / line_height).floor() as usize;
    fitted.clamp(1, line_count.max(1))
}

struct DialogScrollPointer<'a> {
    cursor: Option<[f32; 2]>,
    dragging: bool,
    up: &'a UiButton,
    down: &'a UiButton,
}

fn push_dialog_scroll(
    quads: &mut Vec<UiTexturedQuad>,
    assets: &MessageDialogV2Assets,
    geometry: UiScrollGeometry,
    pointer: DialogScrollPointer<'_>,
    scale: f32,
) -> Result<(), String> {
    push_nine_slice(
        quads,
        geometry.track,
        &assets.scroll_track,
        SCROLL_TRACK_BORDER,
        scale,
    )?;
    let thumb = if pointer.dragging {
        assets.scroll_thumbs[2]
    } else if pointer
        .cursor
        .is_some_and(|point| geometry.thumb.contains(point))
    {
        assets.scroll_thumbs[1]
    } else {
        assets.scroll_thumbs[0]
    };
    quads.push(compose_stretched_quad(geometry.thumb, thumb)?);
    quads.push(compose_stretched_quad(
        geometry.up,
        arrow_texture(
            assets.scroll_up,
            pointer.up.visual(pointer.cursor, geometry.up, true),
        ),
    )?);
    quads.push(compose_stretched_quad(
        geometry.down,
        arrow_texture(
            assets.scroll_down,
            pointer.down.visual(pointer.cursor, geometry.down, true),
        ),
    )?);
    Ok(())
}

fn arrow_texture(textures: [SpriteTextureId; 3], visual: UiButtonVisual) -> SpriteTextureId {
    match visual {
        UiButtonVisual::Hover => textures[1],
        UiButtonVisual::Pressed => textures[2],
        UiButtonVisual::Normal | UiButtonVisual::Disabled => textures[0],
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
        clip: None,
    }
}

fn button_asset(
    assets: &MessageDialogV2Assets,
    bounds: ScreenRect,
    cursor: Option<[f32; 2]>,
    pressed: bool,
) -> &UiV2NineSlice {
    let hovered = cursor.is_some_and(|point| bounds.contains(point));
    if hovered && pressed {
        &assets.buttons[2]
    } else if hovered {
        &assets.buttons[1]
    } else {
        &assets.buttons[0]
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

fn textures3(images: Vec<UiV2Image>) -> Result<[SpriteTextureId; 3], String> {
    let images = array3(images)?;
    Ok([images[0].texture, images[1].texture, images[2].texture])
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
fn assert_sliced_button(pieces: &[&UiTexturedQuad], bounds: ScreenRect, scale: f32) {
    assert_eq!(pieces.len(), 3);
    let cap = 10.0 * scale;
    assert!((pieces[0].corners[0][0] - bounds.min[0]).abs() < 0.05);
    assert!((pieces[0].corners[2][0] - (bounds.min[0] + cap)).abs() < 0.05);
    assert!((pieces[2].corners[2][0] - bounds.max[0]).abs() < 0.05);
    assert!((pieces[2].corners[0][0] - (bounds.max[0] - cap)).abs() < 0.05);
    assert!((pieces[0].corners[0][1] - bounds.min[1]).abs() < 0.05);
    assert!((pieces[0].corners[2][1] - bounds.max[1]).abs() < 0.05);
    assert!(pieces[1].uvs[0][0] > 0.0);
    assert!(pieces[1].uvs[2][0] < 1.0);
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
                .all(|image| image.size_px == [112, 44] && image.slice_ltrb == [10, 0, 10, 0])
        );
    }

    #[test]
    fn layout_fits_one_two_and_three_buttons() {
        for (count, dismissible) in [(1, false), (2, true), (3, true)] {
            let layout =
                layout_message_dialog(viewport(), 1.0, "Body", count, dismissible, 0).unwrap();
            assert!((layout.window.width() - DIALOG_WIDTH_UNITS).abs() < 0.01);
            assert!(layout.scroll.is_none());
            assert!(layout.window.height() < viewport().height as f32 * DIALOG_MAX_HEIGHT_FRACTION);
            assert!(layout.body_bounds.height() < 40.0);
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
            let body_center = (layout.body_bounds.min[0] + layout.body_bounds.max[0]) * 0.5;
            let row_start = layout.buttons[0].min[0];
            let row_end = layout.buttons[count - 1].max[0];
            assert!((body_center - (row_start + row_end) * 0.5).abs() < 0.05);
            assert!(width <= BUTTON_MAX_WIDTH_UNITS + 0.05);
            let expected = if count == 3 {
                (layout.body_bounds.width() - BUTTON_GAP_UNITS * 2.0) / 3.0
            } else {
                BUTTON_MAX_WIDTH_UNITS
            };
            assert!((width - expected).abs() < 0.05);
            assert!(row_start >= layout.body_bounds.min[0] - 0.05);
            assert!(row_end <= layout.body_bounds.max[0] + 0.05);
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
        assert_eq!(buttons.len(), dialog.button_bounds.len() * 3);
        for (pieces, bounds) in buttons.chunks(3).zip(dialog.button_bounds.iter()) {
            assert_sliced_button(pieces, *bounds, 1.0);
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
        for (text, bounds) in frame.texts.iter().skip(2).zip(dialog.button_bounds.iter()) {
            let max_width = text.max_width.expect("button label width");
            assert!(max_width < bounds.width() - 1.0);
            let content_top = bounds.min[1] + crate::ui_panel::BUTTON_TEXT_PAD_Y;
            let content_bottom = bounds.max[1] - crate::ui_panel::BUTTON_TEXT_PAD_Y;
            assert!(text.anchor[1] >= content_top - 0.01);
            assert!(text.anchor[1] + BUTTON_FONT_SIZE <= content_bottom + 0.01);
        }
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
        let header = layout_message_dialog(viewport(), 1.0, "Notice", 1, false, 0)
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
        let base = layout_message_dialog(viewport(), 1.0, "Body", 3, true, 0).unwrap();
        for scale in [0.9_f32, 1.0, 1.25] {
            let layout = layout_message_dialog(viewport(), scale, "Body", 3, true, 0).unwrap();
            assert!((layout.window.width() - base.window.width() * scale).abs() < 0.05);
            assert!((layout.window.height() - base.window.height() * scale).abs() < 1.5);
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
            assert!((offset[1] - base_offset[1] * scale).abs() < 1.5);
        }
    }

    fn medium_body() -> String {
        "This notice wraps across several lines and should grow the dialog without reaching the viewport cap or showing a scrollbar.".to_owned()
    }

    fn long_body() -> String {
        "The road through the message is long enough that the dialog must stop growing and scroll only the body. ".repeat(30)
    }

    #[test]
    fn dialog_body_grows_then_caps_and_scrolls() {
        let (_runtime, assets) = loaded_assets();
        let short = layout_message_dialog(viewport(), 1.0, "Hi", 2, true, 0).unwrap();
        let medium = layout_message_dialog(viewport(), 1.0, &medium_body(), 2, true, 0).unwrap();
        let long = layout_message_dialog(viewport(), 1.0, &long_body(), 2, true, 4).unwrap();
        assert!(short.window.height() < medium.window.height());
        assert!(medium.window.height() < long.window.height());
        assert!(short.scroll.is_none());
        assert!(medium.scroll.is_none());
        assert!(long.scroll.is_some());
        assert!(long.max_offset > 0);
        assert_eq!(long.scroll_offset, 4);
        assert!(
            long.window.height() <= viewport().height as f32 * DIALOG_MAX_HEIGHT_FRACTION + 0.6
        );
        assert!(long.body_bounds.width() < short.body_bounds.width() - 8.0);
        assert!((short.body_bounds.width() - short.content_bounds.width()).abs() < 0.05);
        assert!(long.header.max[1] <= long.body_bounds.min[1] + 0.05);
        assert!(long.buttons[0].min[1] >= long.body_bounds.max[1] - 0.05);
        assert!(long.buttons[0].max[1] <= long.window.max[1] + 0.05);

        let mut dialog = MessageDialog::default();
        let mut opened = request();
        opened.body = long_body();
        assert!(dialog.open(opened));
        assert_eq!(dialog.scroll_offset, 0);
        let frame = dialog
            .frame(&assets, viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        let body = frame
            .texts
            .iter()
            .find(|text| text.content.0.contains("road"))
            .unwrap();
        assert_eq!(
            body.clip,
            Some([
                dialog.body_bounds.unwrap().min[0],
                dialog.body_bounds.unwrap().min[1],
                dialog.body_bounds.unwrap().max[0],
                dialog.body_bounds.unwrap().max[1],
            ])
        );
        assert!(frame.texts[0].clip.is_none());
        let header_y = dialog_header_top(&frame, &assets);
        let button_y = dialog.button_bounds[0].min[1];
        assert!(dialog.apply_wheel(
            MouseScrollDelta::LineDelta(0.0, -3.0),
            Some(center_of(dialog.body_bounds.unwrap())),
        ));
        assert!(dialog.scroll_offset > 0);
        assert!(!dialog.apply_wheel(MouseScrollDelta::LineDelta(0.0, -1.0), Some([0.0, 0.0]),));
        let scrolled = dialog
            .frame(&assets, viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        assert!((dialog_header_top(&scrolled, &assets) - header_y).abs() < 0.05);
        assert!((dialog.button_bounds[0].min[1] - button_y).abs() < 0.05);
        let scrolled_body = scrolled
            .texts
            .iter()
            .find(|text| text.content.0.contains("road"))
            .unwrap();
        assert!(scrolled_body.anchor[1] < body.anchor[1]);
        assert_eq!(scrolled_body.clip, body.clip);
        let track = assets.scroll_track.texture;
        assert!(scrolled.skin_quads.iter().any(|quad| quad.texture == track));
        let short_frame = {
            let mut compact = MessageDialog::default();
            assert!(compact.open(request()));
            compact
                .frame(&assets, viewport(), 1.0, None)
                .unwrap()
                .unwrap()
        };
        assert!(
            short_frame
                .skin_quads
                .iter()
                .all(|quad| quad.texture != track)
        );

        let geometry = dialog.scroll_geom.expect("long dialog has a scrollbar");
        let thumb = center_of(geometry.thumb);
        assert!(dialog.apply_pointer_button(ElementState::Pressed, Some(thumb)));
        assert!(dialog.scrollbar.is_dragging());
        dialog.pointer_moved([thumb[0], geometry.track.max[1] + 400.0]);
        assert_eq!(dialog.scroll_offset, dialog.max_scroll);
        dialog.pointer_moved([thumb[0], geometry.track.min[1] - 400.0]);
        assert_eq!(dialog.scroll_offset, 0);
        assert!(dialog.apply_pointer_button(ElementState::Released, Some(thumb)));
        assert!(!dialog.scrollbar.is_dragging());

        dialog.close();
        let mut again = request();
        again.body = long_body();
        assert!(dialog.open(again));
        let _ = dialog.frame(&assets, viewport(), 1.0, None).unwrap();
        assert_eq!(dialog.scroll_offset, 0);

        for scale in [0.9_f32, 1.0, 1.25] {
            let fitted =
                layout_message_dialog(viewport(), scale, &long_body(), 1, false, 0).unwrap();
            assert!(fitted.window.height() <= viewport().height as f32 * 0.8 + 1.0);
            assert!(fitted.header.max[1] < fitted.body_bounds.min[1]);
            assert!(fitted.buttons[0].min[1] > fitted.body_bounds.max[1] - 0.5);
            assert!(fitted.scroll.is_some());
        }
    }

    fn center_of(bounds: ScreenRect) -> [f32; 2] {
        [
            (bounds.min[0] + bounds.max[0]) * 0.5,
            (bounds.min[1] + bounds.max[1]) * 0.5,
        ]
    }

    fn dialog_header_top(frame: &MessageDialogFrame, assets: &MessageDialogV2Assets) -> f32 {
        frame
            .skin_quads
            .iter()
            .find(|quad| quad.texture == assets.header.texture)
            .expect("header")
            .corners[0][1]
    }
}
