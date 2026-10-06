//! Reusable production UI controls.
//!
//! Each control owns transient pointer state only. Callers own values, skins, and results.
//! A click or toggle requires press inside the same bounds used for drawing, then release
//! inside those bounds. A slider captures a drag from its handle or track and reports a
//! normalized value. Progress bars are display-only.

use winit::event::{ElementState, MouseScrollDelta};
use winit::keyboard::{Key, NamedKey};

use crate::renderer::{SpriteTextureId, UiTexturedQuad};
use crate::ui_panel::{ScreenRect, compose_nine_slice_with_borders};
use crate::ui_v2::{UiV2Image, UiV2NineSlice};

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

/// Fill inset for `status_track_9slice` (288×28) around a 270×18 fill.
/// Left, top, right, bottom in logical units. It does not change with value.
pub(crate) const PROGRESS_FILL_INSET: [f32; 4] = [9.0, 5.0, 9.0, 5.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum UiSliderOutcome {
    Idle,
    Captured,
    Changed(f32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiSliderHandleVisual {
    Normal,
    Hover,
    Pressed,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiSliderSkin {
    normal: SpriteTextureId,
    hover: SpriteTextureId,
    pressed: SpriteTextureId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiSliderGeometry {
    pub(crate) track: ScreenRect,
    pub(crate) handle: ScreenRect,
    pub(crate) hit: ScreenRect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct UiSlider {
    dragging: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiProgressGeometry {
    pub(crate) track: ScreenRect,
    pub(crate) content: ScreenRect,
    pub(crate) fill: Option<ScreenRect>,
}

impl UiSliderSkin {
    pub(crate) fn from_textures(textures: [SpriteTextureId; 3]) -> Self {
        Self {
            normal: textures[0],
            hover: textures[1],
            pressed: textures[2],
        }
    }

    pub(crate) fn texture(self, visual: UiSliderHandleVisual) -> SpriteTextureId {
        match visual {
            UiSliderHandleVisual::Normal => self.normal,
            UiSliderHandleVisual::Hover => self.hover,
            UiSliderHandleVisual::Pressed => self.pressed,
        }
    }
}

impl UiSlider {
    pub(crate) fn is_dragging(self) -> bool {
        self.dragging
    }

    pub(crate) fn cancel(&mut self) {
        self.dragging = false;
    }

    pub(crate) fn visual(
        self,
        cursor: Option<[f32; 2]>,
        handle: ScreenRect,
    ) -> UiSliderHandleVisual {
        if self.dragging {
            UiSliderHandleVisual::Pressed
        } else if inside(handle, cursor) {
            UiSliderHandleVisual::Hover
        } else {
            UiSliderHandleVisual::Normal
        }
    }

    pub(crate) fn apply(
        &mut self,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        geometry: UiSliderGeometry,
    ) -> Result<UiSliderOutcome, String> {
        match state {
            ElementState::Pressed => {
                let Some(point) = cursor else {
                    return Ok(UiSliderOutcome::Idle);
                };
                if geometry.handle.contains(point) {
                    self.dragging = true;
                    return Ok(UiSliderOutcome::Captured);
                }
                if geometry.track.contains(point) {
                    let value = slider_value_from_x(geometry.track, point[0])?;
                    self.dragging = true;
                    return Ok(UiSliderOutcome::Changed(value));
                }
                Ok(UiSliderOutcome::Idle)
            }
            ElementState::Released => {
                if !self.dragging {
                    return Ok(UiSliderOutcome::Idle);
                }
                self.dragging = false;
                let Some(point) = cursor else {
                    return Ok(UiSliderOutcome::Captured);
                };
                Ok(UiSliderOutcome::Changed(slider_value_from_x(
                    geometry.track,
                    point[0],
                )?))
            }
        }
    }

    pub(crate) fn pointer_moved(
        &mut self,
        cursor: [f32; 2],
        track: ScreenRect,
    ) -> Result<UiSliderOutcome, String> {
        if !self.dragging {
            return Ok(UiSliderOutcome::Idle);
        }
        if !cursor[1].is_finite() {
            return Err("UI slider cursor is invalid".to_string());
        }
        Ok(UiSliderOutcome::Changed(slider_value_from_x(
            track, cursor[0],
        )?))
    }
}

/// Handle center sits on the track span: 0 at the left edge, 1 at the right edge.
pub(crate) fn slider_geometry(
    track: ScreenRect,
    handle_size: f32,
    value: f32,
) -> Result<UiSliderGeometry, String> {
    if !handle_size.is_finite() || handle_size <= 0.0 || !rect_is_positive(track) {
        return Err("UI slider geometry is invalid".to_string());
    }
    let center_x = span_position(track.min[0], track.max[0], value)?;
    let center_y = (track.min[1] + track.max[1]) * 0.5;
    if !center_y.is_finite() {
        return Err("UI slider geometry is invalid".to_string());
    }
    let half = handle_size * 0.5;
    let handle = ScreenRect {
        min: [center_x - half, center_y - half],
        max: [center_x + half, center_y + half],
    };
    let hit = ScreenRect {
        min: [
            track.min[0].min(handle.min[0]),
            track.min[1].min(handle.min[1]),
        ],
        max: [
            track.max[0].max(handle.max[0]),
            track.max[1].max(handle.max[1]),
        ],
    };
    Ok(UiSliderGeometry { track, handle, hit })
}

pub(crate) fn slider_value_from_x(track: ScreenRect, x: f32) -> Result<f32, String> {
    if !rect_is_positive(track) {
        return Err("UI slider track is invalid".to_string());
    }
    span_value(track.min[0], track.max[0], x)
}

pub(crate) fn progress_geometry(
    track: ScreenRect,
    inset: [f32; 4],
    value: f32,
) -> Result<UiProgressGeometry, String> {
    if !rect_is_positive(track) || !inset.into_iter().all(finite_non_negative) {
        return Err("UI progress geometry is invalid".to_string());
    }
    let value = finite_unit(value)?;
    let content = ScreenRect {
        min: [track.min[0] + inset[0], track.min[1] + inset[1]],
        max: [track.max[0] - inset[2], track.max[1] - inset[3]],
    };
    if !rect_is_positive(content) {
        return Err("UI progress content inset does not fit the track".to_string());
    }
    let fill = if value == 0.0 {
        None
    } else {
        let max_x = if value >= 1.0 {
            content.max[0]
        } else {
            content.min[0] + content.width() * value
        };
        if !max_x.is_finite() || max_x <= content.min[0] || max_x > content.max[0] {
            return Err("UI progress fill is invalid".to_string());
        }
        Some(ScreenRect {
            min: content.min,
            max: [max_x, content.max[1]],
        })
    };
    Ok(UiProgressGeometry {
        track,
        content,
        fill,
    })
}

pub(crate) fn compose_progress_bar(
    track: ScreenRect,
    inset: [f32; 4],
    value: f32,
    track_asset: &UiV2NineSlice,
    fill_asset: &UiV2NineSlice,
    scale: f32,
) -> Result<Vec<UiTexturedQuad>, String> {
    let geometry = progress_geometry(track, inset, value)?;
    let mut quads = compose_nine_slice_with_borders(
        geometry.track.min,
        [
            geometry.track.width() / scale,
            geometry.track.height() / scale,
        ],
        scale,
        track_asset.texture,
        track_asset.size_px,
        track_asset.slice_ltrb,
        slice_border(track_asset.slice_ltrb),
    )?;
    if let Some(fill) = geometry.fill {
        quads.extend(compose_nine_slice_with_borders(
            fill.min,
            [fill.width() / scale, fill.height() / scale],
            scale,
            fill_asset.texture,
            fill_asset.size_px,
            fill_asset.slice_ltrb,
            slice_border(fill_asset.slice_ltrb),
        )?);
    }
    Ok(quads)
}

fn slice_border(slice: [u32; 4]) -> [f32; 4] {
    [
        slice[0] as f32,
        slice[1] as f32,
        slice[2] as f32,
        slice[3] as f32,
    ]
}

fn finite_unit(value: f32) -> Result<f32, String> {
    if !value.is_finite() {
        return Err("UI value is non-finite".to_string());
    }
    Ok(value.clamp(0.0, 1.0))
}

fn finite_non_negative(value: f32) -> bool {
    value.is_finite() && value >= 0.0
}

fn rect_is_positive(rect: ScreenRect) -> bool {
    rect.min.into_iter().chain(rect.max).all(f32::is_finite)
        && rect.width() > 0.0
        && rect.height() > 0.0
}

fn span_position(start: f32, end: f32, value: f32) -> Result<f32, String> {
    let value = finite_unit(value)?;
    let span = span_width(start, end)?;
    Ok(start + span * value)
}

fn span_value(start: f32, end: f32, position: f32) -> Result<f32, String> {
    if !position.is_finite() {
        return Err("UI slider cursor is invalid".to_string());
    }
    let span = span_width(start, end)?;
    Ok(((position - start) / span).clamp(0.0, 1.0))
}

fn span_width(start: f32, end: f32) -> Result<f32, String> {
    if !start.is_finite() || !end.is_finite() || end <= start {
        return Err("UI slider track is invalid".to_string());
    }
    Ok(end - start)
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

/// Single-line text field. The caller owns the string. The control owns focus and caret.
pub(crate) const UI_TEXT_INPUT_LIMIT: usize = 32;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct UiTextInput {
    focused: bool,
    caret: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiSlotVisual {
    Normal,
    Hover,
    Selected,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiScrollGeometry {
    pub(crate) list: ScreenRect,
    pub(crate) up: ScreenRect,
    pub(crate) down: ScreenRect,
    pub(crate) track: ScreenRect,
    pub(crate) thumb: ScreenRect,
    pub(crate) region: ScreenRect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct UiScrollbar {
    dragging: bool,
    grab: f32,
}

impl UiTextInput {
    pub(crate) fn is_focused(self) -> bool {
        self.focused
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn caret(self) -> usize {
        self.caret
    }

    pub(crate) fn blur(&mut self) {
        self.focused = false;
    }

    /// Press inside focuses the field. Press outside blurs it and does not capture.
    pub(crate) fn apply_press(&mut self, inside: bool, value: &str) -> bool {
        if inside {
            if !self.focused {
                self.focused = true;
                self.caret = value.chars().count();
            }
            self.caret = self.caret.min(value.chars().count());
            true
        } else {
            self.focused = false;
            false
        }
    }

    pub(crate) fn apply_key(
        &mut self,
        value: &mut String,
        key: &Key,
        text: Option<&str>,
        _repeat: bool,
    ) -> bool {
        if !self.focused {
            return false;
        }
        self.caret = self.caret.min(value.chars().count());
        match key {
            Key::Named(NamedKey::Backspace) => delete_before(value, &mut self.caret),
            Key::Named(NamedKey::Delete) => delete_after(value, &mut self.caret),
            Key::Named(NamedKey::ArrowLeft) => {
                self.caret = self.caret.saturating_sub(1);
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.caret = (self.caret + 1).min(value.chars().count());
            }
            Key::Named(NamedKey::Home) => self.caret = 0,
            Key::Named(NamedKey::End) => self.caret = value.chars().count(),
            Key::Named(NamedKey::Enter) => self.focused = false,
            _ => {
                if let Some(text) = text {
                    insert_text(value, &mut self.caret, text, UI_TEXT_INPUT_LIMIT);
                }
            }
        }
        true
    }

    pub(crate) fn display_text(self, value: &str) -> String {
        if !self.focused {
            return value.to_owned();
        }
        let caret = self.caret.min(value.chars().count());
        let byte = byte_at_char(value, caret);
        let mut shown = String::with_capacity(value.len() + 1);
        shown.push_str(&value[..byte]);
        shown.push('|');
        shown.push_str(&value[byte..]);
        shown
    }
}

pub(crate) fn slot_visual(enabled: bool, selected: bool, hovered: bool) -> UiSlotVisual {
    if !enabled {
        UiSlotVisual::Disabled
    } else if selected {
        UiSlotVisual::Selected
    } else if hovered {
        UiSlotVisual::Hover
    } else {
        UiSlotVisual::Normal
    }
}

impl UiScrollbar {
    pub(crate) fn is_dragging(self) -> bool {
        self.dragging
    }

    pub(crate) fn cancel(&mut self) {
        self.dragging = false;
        self.grab = 0.0;
    }

    pub(crate) fn apply_thumb(
        &mut self,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        geometry: &UiScrollGeometry,
        max_offset: usize,
    ) -> bool {
        if max_offset == 0 {
            self.cancel();
            return false;
        }
        match state {
            ElementState::Pressed => {
                let Some(point) = cursor else {
                    return false;
                };
                if !geometry.thumb.contains(point) {
                    return false;
                }
                self.dragging = true;
                self.grab = (point[1] - geometry.thumb.min[1]).clamp(0.0, geometry.thumb.height());
                true
            }
            ElementState::Released => {
                if !self.dragging {
                    return false;
                }
                self.dragging = false;
                self.grab = 0.0;
                true
            }
        }
    }

    pub(crate) fn pointer_moved(
        &mut self,
        cursor_y: f32,
        geometry: &UiScrollGeometry,
        max_offset: usize,
    ) -> Option<usize> {
        if !self.dragging || max_offset == 0 || !cursor_y.is_finite() {
            return None;
        }
        Some(scroll_offset_from_thumb_top(
            geometry.track,
            geometry.thumb.height(),
            cursor_y - self.grab,
            max_offset,
        ))
    }
}

pub(crate) fn scroll_max_offset(total: usize, visible: usize) -> usize {
    total.saturating_sub(visible)
}

pub(crate) fn visible_row_indices(
    offset: usize,
    total: usize,
    visible: usize,
) -> std::ops::Range<usize> {
    let start = offset.min(total);
    let end = start.saturating_add(visible).min(total);
    start..end
}

pub(crate) fn scroll_geometry(
    list: ScreenRect,
    gap: f32,
    arrow: f32,
    offset: usize,
    total: usize,
    visible: usize,
) -> Result<UiScrollGeometry, String> {
    if !rect_is_positive(list)
        || !gap.is_finite()
        || gap < 0.0
        || !arrow.is_finite()
        || arrow <= 0.0
    {
        return Err("UI scroll geometry is invalid".to_string());
    }
    if list.height() <= arrow * 2.0 {
        return Err("UI scroll list is shorter than its arrows".to_string());
    }
    let column_x = list.max[0] + gap;
    let up = ScreenRect {
        min: [column_x, list.min[1]],
        max: [column_x + arrow, list.min[1] + arrow],
    };
    let down = ScreenRect {
        min: [column_x, list.max[1] - arrow],
        max: [column_x + arrow, list.max[1]],
    };
    let track = ScreenRect {
        min: [column_x, up.max[1]],
        max: [column_x + arrow, down.min[1]],
    };
    if !rect_is_positive(track) {
        return Err("UI scroll track is invalid".to_string());
    }
    let min_thumb = (arrow * 0.75).min(track.height());
    let thumb_height = thumb_height(track.height(), visible, total, min_thumb);
    let max_offset = scroll_max_offset(total, visible);
    let travel = (track.height() - thumb_height).max(0.0);
    let top = thumb_top(track.min[1], travel, offset, max_offset);
    let inset = (arrow * 0.12).min(track.width() * 0.2);
    let thumb = ScreenRect {
        min: [track.min[0] + inset, top],
        max: [track.max[0] - inset, top + thumb_height],
    };
    let region = ScreenRect {
        min: list.min,
        max: [down.max[0], list.max[1]],
    };
    Ok(UiScrollGeometry {
        list,
        up,
        down,
        track,
        thumb,
        region,
    })
}

pub(crate) fn scroll_rows_from_wheel(delta: MouseScrollDelta) -> i32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => finite_rows(f64::from(y)),
        MouseScrollDelta::PixelDelta(position) => finite_rows(position.y / 48.0),
    }
}

/// Positive rows move toward the start of the list.
pub(crate) fn scroll_offset_after_wheel(offset: usize, max_offset: usize, rows: i32) -> usize {
    let offset = offset.min(max_offset);
    if rows > 0 {
        offset.saturating_sub(rows as usize)
    } else if rows < 0 {
        offset
            .saturating_add(rows.unsigned_abs() as usize)
            .min(max_offset)
    } else {
        offset
    }
}

pub(crate) fn scroll_offset_from_thumb_top(
    track: ScreenRect,
    thumb_height: f32,
    thumb_top: f32,
    max_offset: usize,
) -> usize {
    if max_offset == 0 || !thumb_height.is_finite() || !thumb_top.is_finite() {
        return 0;
    }
    let travel = (track.height() - thumb_height).max(0.0);
    if travel <= 0.0 {
        return 0;
    }
    let top = thumb_top.clamp(track.min[1], track.min[1] + travel);
    let fraction = (top - track.min[1]) / travel;
    let scaled = fraction * max_offset as f32;
    if !scaled.is_finite() {
        return 0;
    }
    (scaled.round() as usize).min(max_offset)
}

fn thumb_height(track_height: f32, visible: usize, total: usize, min_thumb: f32) -> f32 {
    if total == 0 {
        return track_height;
    }
    let fraction = visible.min(total) as f32 / total as f32;
    (track_height * fraction).clamp(min_thumb.min(track_height), track_height)
}

fn thumb_top(track_min: f32, travel: f32, offset: usize, max_offset: usize) -> f32 {
    if max_offset == 0 || travel <= 0.0 {
        track_min
    } else {
        track_min + travel * (offset.min(max_offset) as f32) / max_offset as f32
    }
}

fn finite_rows(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    let rounded = value.round();
    if rounded > 64.0 {
        64
    } else if rounded < -64.0 {
        -64
    } else {
        rounded as i32
    }
}

fn byte_at_char(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

fn delete_before(value: &mut String, caret: &mut usize) {
    if *caret == 0 {
        return;
    }
    let end = byte_at_char(value, *caret);
    let start = byte_at_char(value, *caret - 1);
    value.replace_range(start..end, "");
    *caret -= 1;
}

fn delete_after(value: &mut String, caret: &mut usize) {
    let count = value.chars().count();
    if *caret >= count {
        return;
    }
    let start = byte_at_char(value, *caret);
    let end = byte_at_char(value, *caret + 1);
    value.replace_range(start..end, "");
}

fn insert_text(value: &mut String, caret: &mut usize, text: &str, limit: usize) {
    let incoming: String = text.chars().filter(|ch| !ch.is_control()).collect();
    if incoming.is_empty() {
        return;
    }
    let room = limit.saturating_sub(value.chars().count());
    let accepted: String = incoming.chars().take(room).collect();
    if accepted.is_empty() {
        return;
    }
    let byte = byte_at_char(value, *caret);
    value.insert_str(byte, &accepted);
    *caret += accepted.chars().count();
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

    fn slider_track() -> ScreenRect {
        ScreenRect {
            min: [10.0, 40.0],
            max: [210.0, 47.0],
        }
    }

    fn handle_center_x(value: f32) -> f32 {
        let geometry = slider_geometry(slider_track(), 16.0, value).unwrap();
        (geometry.handle.min[0] + geometry.handle.max[0]) * 0.5
    }

    fn slider_at(value: f32) -> UiSliderGeometry {
        slider_geometry(slider_track(), 16.0, value).unwrap()
    }

    #[test]
    fn slider_handle_maps_endpoints_and_center() {
        let track = slider_track();
        let left = slider_at(0.0);
        let center = slider_at(0.5);
        let right = slider_at(1.0);
        assert!((handle_center_x(0.0) - track.min[0]).abs() < 1e-4);
        assert!((handle_center_x(0.5) - (track.min[0] + track.max[0]) * 0.5).abs() < 1e-4);
        assert!((handle_center_x(1.0) - track.max[0]).abs() < 1e-4);
        assert!(
            (left.handle.min[1] + left.handle.max[1] - center.handle.min[1] - center.handle.max[1])
                .abs()
                < 1e-4
        );
        assert_eq!(left.track, track);
        assert!(left.hit.contains(left.handle.min));
        assert!(right.hit.contains(right.handle.max));
        assert!((handle_center_x(1.4) - track.max[0]).abs() < 1e-4);
        assert!((handle_center_x(-0.2) - track.min[0]).abs() < 1e-4);
        assert!(slider_geometry(track, 16.0, f32::NAN).is_err());
        assert!(slider_geometry(track, 0.0, 0.5).is_err());
    }

    #[test]
    fn slider_cursor_inverts_handle_mapping() {
        let track = slider_track();
        for value in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let x = handle_center_x(value);
            let restored = slider_value_from_x(track, x).unwrap();
            assert!(
                (restored - value).abs() < 1e-5,
                "{value} -> {x} -> {restored}"
            );
        }
        assert!((slider_value_from_x(track, track.min[0] - 40.0).unwrap()).abs() < 1e-5);
        assert!((slider_value_from_x(track, track.max[0] + 80.0).unwrap() - 1.0).abs() < 1e-5);
        assert!(slider_value_from_x(track, f32::NAN).is_err());
    }

    #[test]
    fn slider_press_handle_captures_without_jumping() {
        let geometry = slider_at(0.5);
        let mut slider = UiSlider::default();
        let point = [
            handle_center_x(0.5),
            (geometry.handle.min[1] + geometry.handle.max[1]) * 0.5,
        ];
        assert!(geometry.handle.contains(point));
        assert_eq!(
            slider
                .apply(ElementState::Pressed, Some(point), geometry)
                .unwrap(),
            UiSliderOutcome::Captured
        );
        assert!(slider.is_dragging());
        assert_eq!(
            slider.visual(Some(point), geometry.handle),
            UiSliderHandleVisual::Pressed
        );
    }

    #[test]
    fn slider_press_track_changes_value_and_captures() {
        let geometry = slider_at(0.5);
        let mut slider = UiSlider::default();
        let point = [30.0, 43.5];
        assert!(geometry.track.contains(point));
        assert!(!geometry.handle.contains(point));
        match slider
            .apply(ElementState::Pressed, Some(point), geometry)
            .unwrap()
        {
            UiSliderOutcome::Changed(value) => assert!((value - 0.1).abs() < 1e-5),
            other => panic!("expected a track change, got {other:?}"),
        }
        assert!(slider.is_dragging());
        assert_eq!(
            slider.visual(Some([0.0, -200.0]), geometry.handle),
            UiSliderHandleVisual::Pressed
        );
    }

    #[test]
    fn slider_drag_follows_x_and_clamps_past_the_ends() {
        let geometry = slider_at(0.5);
        let mut slider = UiSlider::default();
        let y = 43.5;
        slider
            .apply(ElementState::Pressed, Some([110.0, y]), geometry)
            .unwrap();
        assert_eq!(
            slider.pointer_moved([160.0, y], slider_track()).unwrap(),
            UiSliderOutcome::Changed(0.75)
        );
        assert_eq!(
            slider
                .pointer_moved([slider_track().min[0] - 50.0, y], slider_track())
                .unwrap(),
            UiSliderOutcome::Changed(0.0)
        );
        assert_eq!(
            slider
                .pointer_moved([slider_track().max[0] + 90.0, -1000.0], slider_track())
                .unwrap(),
            UiSliderOutcome::Changed(1.0)
        );
        assert!(slider.is_dragging());
    }

    #[test]
    fn slider_vertical_exit_does_not_end_capture() {
        let geometry = slider_at(0.25);
        let mut slider = UiSlider::default();
        slider
            .apply(ElementState::Pressed, Some([60.0, 43.5]), geometry)
            .unwrap();
        match slider
            .pointer_moved([80.0, 10_000.0], slider_track())
            .unwrap()
        {
            UiSliderOutcome::Changed(value) => assert!((value - 0.35).abs() < 1e-5),
            other => panic!("expected a dragged value, got {other:?}"),
        }
        assert!(slider.is_dragging());
        assert_eq!(
            slider.visual(Some([80.0, 10_000.0]), geometry.handle),
            UiSliderHandleVisual::Pressed
        );
    }

    #[test]
    fn slider_release_and_cancel_end_capture() {
        let geometry = slider_at(0.5);
        let mut slider = UiSlider::default();
        slider
            .apply(ElementState::Pressed, Some([110.0, 43.5]), geometry)
            .unwrap();
        assert_eq!(
            slider
                .apply(ElementState::Released, Some([10.0, -40.0]), geometry)
                .unwrap(),
            UiSliderOutcome::Changed(0.0)
        );
        assert!(!slider.is_dragging());
        assert_eq!(
            slider.pointer_moved([200.0, 43.5], slider_track()).unwrap(),
            UiSliderOutcome::Idle
        );

        slider
            .apply(ElementState::Pressed, Some([110.0, 43.5]), geometry)
            .unwrap();
        slider.cancel();
        assert!(!slider.is_dragging());
        assert_eq!(
            slider
                .apply(ElementState::Released, Some([160.0, 43.5]), geometry)
                .unwrap(),
            UiSliderOutcome::Idle
        );
        assert_eq!(
            slider.visual(None, geometry.handle),
            UiSliderHandleVisual::Normal
        );
        assert_eq!(
            slider.visual(Some([110.0, 43.5]), geometry.handle),
            UiSliderHandleVisual::Hover
        );
    }

    #[test]
    fn slider_handle_skin_uses_normal_hover_and_pressed() {
        let skin = UiSliderSkin::from_textures([
            SpriteTextureId::from_raw(1),
            SpriteTextureId::from_raw(2),
            SpriteTextureId::from_raw(3),
        ]);
        assert_eq!(
            skin.texture(UiSliderHandleVisual::Normal),
            SpriteTextureId::from_raw(1)
        );
        assert_eq!(
            skin.texture(UiSliderHandleVisual::Hover),
            SpriteTextureId::from_raw(2)
        );
        assert_eq!(
            skin.texture(UiSliderHandleVisual::Pressed),
            SpriteTextureId::from_raw(3)
        );
        let geometry = slider_at(0.5);
        let mut slider = UiSlider::default();
        let point = [110.0, 43.5];
        assert_eq!(
            slider.visual(None, geometry.handle),
            UiSliderHandleVisual::Normal
        );
        assert_eq!(
            slider.visual(Some(point), geometry.handle),
            UiSliderHandleVisual::Hover
        );
        slider
            .apply(ElementState::Pressed, Some(point), geometry)
            .unwrap();
        assert_eq!(
            slider.visual(Some(point), geometry.handle),
            UiSliderHandleVisual::Pressed
        );
        assert_eq!(
            skin.texture(slider.visual(Some([-20.0, 43.5]), geometry.handle)),
            SpriteTextureId::from_raw(3)
        );
    }

    fn progress_track() -> ScreenRect {
        ScreenRect {
            min: [0.0, 0.0],
            max: [200.0, 24.0],
        }
    }

    fn progress_at(value: f32) -> UiProgressGeometry {
        progress_geometry(progress_track(), PROGRESS_FILL_INSET, value).unwrap()
    }

    fn fill_width(value: f32) -> f32 {
        progress_at(value).fill.unwrap().width()
    }

    fn status_slices() -> (UiV2NineSlice, UiV2NineSlice) {
        (
            UiV2NineSlice {
                texture: SpriteTextureId::from_raw(10),
                size_px: [288, 28],
                slice_ltrb: [10, 10, 10, 10],
            },
            UiV2NineSlice {
                texture: SpriteTextureId::from_raw(11),
                size_px: [270, 18],
                slice_ltrb: [6, 6, 6, 6],
            },
        )
    }

    fn assert_positive_quads(quads: &[UiTexturedQuad]) {
        assert!(!quads.is_empty());
        for quad in quads {
            let width = quad.corners[1][0] - quad.corners[0][0];
            let height = quad.corners[3][1] - quad.corners[0][1];
            assert!(width > 0.0 && height > 0.0, "{width} x {height}");
            assert!(quad.corners.into_iter().flatten().all(f32::is_finite));
        }
    }

    #[test]
    fn progress_fill_tracks_normalized_value_inside_the_content() {
        let empty = progress_at(0.0);
        assert!(empty.fill.is_none());
        assert_eq!(empty.track, progress_track());
        let content_width = empty.content.width();
        assert!((content_width - (200.0 - 18.0)).abs() < 1e-4);
        assert!((fill_width(0.01) - content_width * 0.01).abs() < 1e-4);
        assert!((fill_width(0.5) - content_width * 0.5).abs() < 1e-4);
        assert!((fill_width(0.99) - content_width * 0.99).abs() < 1e-4);
        assert!((fill_width(1.0) - content_width).abs() < 1e-4);
        assert!((progress_at(1.0).fill.unwrap().max[0] - empty.content.max[0]).abs() < 1e-4);
        let widths = [0.01, 0.5, 0.99, 1.0].map(fill_width);
        assert!(widths.windows(2).all(|pair| pair[0] < pair[1]));
        for value in [0.0, 0.01, 0.5, 0.99, 1.0, -0.4, 1.6] {
            let geometry = progress_geometry(progress_track(), PROGRESS_FILL_INSET, value).unwrap();
            assert_eq!(geometry.track, progress_track());
            let Some(fill) = geometry.fill else {
                assert_eq!(value.clamp(0.0, 1.0), 0.0);
                continue;
            };
            assert!(fill.width() > 0.0 && fill.height() > 0.0);
            assert!(fill.min[0] >= geometry.content.min[0] - 1e-4);
            assert!(fill.max[0] <= geometry.content.max[0] + 1e-4);
            assert!((fill.min[1] - geometry.content.min[1]).abs() < 1e-4);
            assert!((fill.max[1] - geometry.content.max[1]).abs() < 1e-4);
            assert!(fill.min[0] >= geometry.track.min[0]);
            assert!(fill.max[0] <= geometry.track.max[0]);
        }
        assert!(progress_at(1.5).fill.unwrap().width() - content_width < 1e-4);
        assert!(progress_at(-1.0).fill.is_none());
        assert!(progress_geometry(progress_track(), PROGRESS_FILL_INSET, f32::NAN).is_err());
        assert!(progress_geometry(progress_track(), PROGRESS_FILL_INSET, f32::INFINITY).is_err());
    }

    #[test]
    fn progress_quads_skip_zero_and_stay_positive() {
        let (track_asset, fill_asset) = status_slices();
        for value in [0.0, 0.01, 0.5, 0.99, 1.0] {
            let quads = compose_progress_bar(
                progress_track(),
                PROGRESS_FILL_INSET,
                value,
                &track_asset,
                &fill_asset,
                1.0,
            )
            .unwrap();
            assert_positive_quads(&quads);
            let geometry = progress_at(value);
            let track_quads: Vec<_> = quads
                .iter()
                .filter(|quad| quad.texture == track_asset.texture)
                .collect();
            let fill_quads: Vec<_> = quads
                .iter()
                .filter(|quad| quad.texture == fill_asset.texture)
                .collect();
            assert!(!track_quads.is_empty());
            if value == 0.0 {
                assert!(fill_quads.is_empty());
                continue;
            }
            assert!(!fill_quads.is_empty());
            for quad in fill_quads {
                for corner in quad.corners {
                    assert!(corner[0] >= geometry.content.min[0] - 0.05);
                    assert!(corner[0] <= geometry.content.max[0] + 0.05);
                    assert!(corner[1] >= geometry.content.min[1] - 0.05);
                    assert!(corner[1] <= geometry.content.max[1] + 0.05);
                }
            }
        }
        assert!(
            compose_progress_bar(
                progress_track(),
                PROGRESS_FILL_INSET,
                f32::NAN,
                &track_asset,
                &fill_asset,
                1.0,
            )
            .is_err()
        );
    }

    fn key_named(named: NamedKey) -> Key {
        Key::Named(named)
    }

    #[test]
    fn text_input_focus_blur_and_caret_edits() {
        let bounds = bounds();
        let mut input = UiTextInput::default();
        let mut value = "Hello world".to_string();
        assert!(!input.apply_press(false, &value));
        assert!(!input.is_focused());
        assert_eq!(input.display_text(&value), "Hello world");
        assert!(input.apply_press(true, &value));
        assert!(input.is_focused());
        assert_eq!(input.caret(), 11);
        assert_eq!(input.display_text(&value), "Hello world|");
        assert!(input.apply_key(&mut value, &key_named(NamedKey::ArrowLeft), None, false));
        assert_eq!(input.caret(), 10);
        assert!(input.apply_key(&mut value, &Key::Character("!".into()), Some("!"), false));
        assert_eq!(value, "Hello worl!d");
        assert_eq!(input.caret(), 11);
        assert!(input.apply_key(&mut value, &key_named(NamedKey::Backspace), None, true));
        assert_eq!(value, "Hello world");
        assert!(input.apply_key(&mut value, &key_named(NamedKey::Delete), None, true));
        assert_eq!(value, "Hello worl");
        assert!(input.apply_key(&mut value, &key_named(NamedKey::Home), None, false));
        assert_eq!(input.caret(), 0);
        assert!(input.apply_key(&mut value, &key_named(NamedKey::Delete), None, false));
        assert_eq!(value, "ello worl");
        assert!(input.apply_key(&mut value, &key_named(NamedKey::End), None, false));
        assert_eq!(input.caret(), value.chars().count());
        assert!(input.apply_key(&mut value, &key_named(NamedKey::ArrowRight), None, true));
        assert_eq!(input.caret(), value.chars().count());
        assert!(!input.apply_press(bounds.contains(inside_point()), &value) || input.is_focused());
        input.blur();
        assert!(!input.is_focused());
        assert!(!input.display_text(&value).contains('|'));
        assert!(!input.apply_key(
            &mut value,
            &key_named(NamedKey::Backspace),
            Some("u"),
            false
        ));
        assert_eq!(value, "ello worl");
    }

    #[test]
    fn text_input_is_unicode_safe_and_length_limited() {
        let mut input = UiTextInput::default();
        let mut value = "a😀b".to_string();
        assert!(input.apply_press(true, &value));
        assert!(input.apply_key(&mut value, &key_named(NamedKey::ArrowLeft), None, false));
        assert!(input.apply_key(&mut value, &key_named(NamedKey::Backspace), None, false));
        assert_eq!(value, "ab");
        value = "é".to_string();
        input.blur();
        assert!(input.apply_press(true, &value));
        assert!(input.apply_key(&mut value, &key_named(NamedKey::ArrowLeft), None, false));
        assert!(input.apply_key(&mut value, &Key::Character("x".into()), Some("x"), false));
        assert_eq!(value, "xé");
        assert!(input.apply_key(
            &mut value,
            &Key::Character("\u{0007}".into()),
            Some("\u{0007}"),
            false
        ));
        assert_eq!(value, "xé");
        let mut full = "a".repeat(UI_TEXT_INPUT_LIMIT);
        input.blur();
        assert!(input.apply_press(true, &full));
        assert!(input.apply_key(&mut full, &Key::Character("z".into()), Some("z"), false));
        assert_eq!(full.chars().count(), UI_TEXT_INPUT_LIMIT);
        assert!(!full.contains('z'));
        assert!(input.apply_key(&mut full, &key_named(NamedKey::Enter), Some("\n"), false));
        assert!(!input.is_focused());
        assert!(!input.display_text(&full).contains('|'));
    }

    fn list_rect() -> ScreenRect {
        ScreenRect {
            min: [0.0, 0.0],
            max: [200.0, 120.0],
        }
    }

    #[test]
    fn scroll_wheel_arrows_and_thumb_follow_offset() {
        let total = 24;
        let visible = 6;
        let max_offset = scroll_max_offset(total, visible);
        assert_eq!(max_offset, 18);
        assert_eq!(visible_row_indices(0, total, visible), 0..6);
        assert_eq!(visible_row_indices(4, total, visible), 4..10);
        assert_eq!(visible_row_indices(20, total, visible), 20..24);
        assert_eq!(
            scroll_offset_after_wheel(
                0,
                max_offset,
                scroll_rows_from_wheel(MouseScrollDelta::LineDelta(0.0, 1.0))
            ),
            0
        );
        assert_eq!(
            scroll_offset_after_wheel(
                4,
                max_offset,
                scroll_rows_from_wheel(MouseScrollDelta::LineDelta(0.0, -1.0))
            ),
            5
        );
        assert_eq!(
            scroll_offset_after_wheel(
                4,
                max_offset,
                scroll_rows_from_wheel(MouseScrollDelta::PixelDelta(
                    winit::dpi::PhysicalPosition { x: 0.0, y: -96.0 }
                ))
            ),
            6
        );
        assert_eq!(scroll_offset_after_wheel(18, max_offset, -5), 18);
        let top = scroll_geometry(list_rect(), 4.0, 16.0, 0, total, visible).unwrap();
        let mid = scroll_geometry(list_rect(), 4.0, 16.0, 9, total, visible).unwrap();
        let bottom = scroll_geometry(list_rect(), 4.0, 16.0, 18, total, visible).unwrap();
        assert!(top.thumb.height() < top.track.height());
        assert!(
            (top.thumb.height() - top.track.height() * (visible as f32 / total as f32)).abs() < 1.0
                || top.thumb.height() >= 16.0 * 0.75 - 0.1
        );
        assert!(top.thumb.min[1] <= mid.thumb.min[1]);
        assert!(mid.thumb.min[1] <= bottom.thumb.min[1]);
        assert!((bottom.thumb.max[1] - bottom.track.max[1]).abs() < 0.6);
        assert!(top.thumb.min[1] >= top.track.min[1] - 0.01);
        let mut bar = UiScrollbar::default();
        assert!(bar.apply_thumb(
            ElementState::Pressed,
            Some(center_of(top.thumb)),
            &top,
            max_offset
        ));
        assert!(bar.is_dragging());
        let dragged = bar
            .pointer_moved(bottom.thumb.min[1] + bar.grab, &top, max_offset)
            .unwrap();
        assert!(dragged > 0);
        assert!(bar.apply_thumb(ElementState::Released, None, &top, max_offset));
        assert!(!bar.is_dragging());
        bar.cancel();
        assert!(bar.pointer_moved(40.0, &top, max_offset).is_none());
    }

    fn center_of(bounds: ScreenRect) -> [f32; 2] {
        [
            (bounds.min[0] + bounds.max[0]) * 0.5,
            (bounds.min[1] + bounds.max[1]) * 0.5,
        ]
    }

    #[test]
    fn slot_visual_disables_and_selects_one_state() {
        assert_eq!(slot_visual(false, true, true), UiSlotVisual::Disabled);
        assert_eq!(slot_visual(true, true, false), UiSlotVisual::Selected);
        assert_eq!(slot_visual(true, false, true), UiSlotVisual::Hover);
        assert_eq!(slot_visual(true, false, false), UiSlotVisual::Normal);
    }
}
