//! Dev-only UI DEBUG window. It consumes reusable controls; it does not invent them.

use std::time::Instant;

use winit::event::{ElementState, MouseScrollDelta};
use winit::keyboard::Key;

use crate::asset_runtime::AssetRuntime;
use crate::assets::ClientAssetLoader;
use crate::renderer::text::{TextCaretBoundaries, measure_caret_boundaries, measure_text};
use crate::renderer::{
    PixelViewport, SpriteTextureId, TextAlignment, TextBlock, TextContent, TextStyle, UiRect,
    UiTexturedQuad,
};
use crate::ui_controls::{
    PROGRESS_FILL_INSET, TooltipContent, UiButton, UiButtonSkin, UiButtonVisual, UiCheckbox,
    UiCheckboxSkin, UiPointerOutcome, UiRadioGroup, UiRadioOutcome, UiRadioSkin, UiScrollGeometry,
    UiScrollbar, UiSlider, UiSliderGeometry, UiSliderOutcome, UiSliderSkin, UiSlotVisual,
    UiTabSkin, UiTabVisual, UiTextInput, UiTextMetrics, UiTextNav, compose_progress_bar,
    notification_badge_label, notification_badge_scale, scroll_geometry, scroll_max_offset,
    scroll_offset_after_wheel, scroll_rows_from_wheel, slider_geometry, slot_visual, tab_visual,
    visible_row_indices,
};
use crate::ui_panel::{
    CloseButtonVisual, ProofPanelWindow, ScreenRect, UiTabs, WindowChromeLayout, button_label_text,
    compose_nine_slice_with_borders, compose_stretched_quad, compose_v2_text_button,
    load_v2_text_buttons,
};
use crate::ui_v2::{
    UiV2Catalog, UiV2Image, UiV2NineSlice, load_ui_v2_catalog, load_ui_v2_image,
    load_ui_v2_nine_slice, load_ui_v2_state_family,
};

const UI_DEBUG_WINDOW_UNITS: [f32; 2] = [560.0, 460.0];
const HEADER_INSET_X: f32 = 10.0;
const HEADER_TOP: f32 = 6.0;
const HEADER_HEIGHT: f32 = 32.0;
const CLOSE_SIZE: f32 = 18.0;
const GEAR_SIZE: f32 = 16.0;
const HEADER_CONTROL_INSET: f32 = 6.0;
const TAB_HEADER_GAP: f32 = 4.0;
const TAB_HEIGHT: f32 = 24.0;
const TAB_SHOULDER: f32 = 6.0;
const CONTENT_BOTTOM: f32 = 10.0;
const PAGE_PAD: f32 = 10.0;
const PANEL_BORDER: [f32; 4] = [10.0, 10.0, 10.0, 12.0];
const HEADER_BORDER: [f32; 4] = [6.0, 6.0, 6.0, 6.0];
const TABBED_BORDER: [f32; 4] = [4.0, 4.0, 4.0, 4.0];
const RAIL_HEIGHT: f32 = 4.0;
const BUTTON_WIDTH: f32 = 136.0;
/// Authored V2 button face. Shorter destinations squash the vertical gradient.
const BUTTON_HEIGHT: f32 = 44.0;
const ICON_BUTTON_SIZE: f32 = 30.0;
const ICON_GLYPH_SIZE: f32 = 16.0;
const MARK_SIZE: f32 = 16.0;
const MARK_INSET: f32 = 3.0;
const ROW_HEIGHT: f32 = 22.0;
const ROW_WIDTH: f32 = 210.0;
const ROW_GAP: f32 = 4.0;
const SECTION_GAP: f32 = 8.0;
const HEADING_HEIGHT: f32 = 14.0;
const STATE_COLUMN_X: f32 = 236.0;
const TEXT_GAP: f32 = 8.0;
const LINE_HEIGHT: f32 = 16.0;
const TITLE_FONT: f32 = 13.0;
const SECTION_FONT: f32 = 11.0;
const BODY_FONT: f32 = 10.5;
const TAB_FONT: f32 = 9.0;
const INK: [f32; 4] = [0.05, 0.07, 0.1, 1.0];
const MUTED: [f32; 4] = [0.36, 0.39, 0.43, 1.0];
const TITLE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const PAGES: [&str; 5] = ["Basics", "Values", "Input", "Containers", "Composite"];
const RADIO_LABELS: [&str; 4] = ["Low", "Medium", "High", "Disabled"];
const BASICS: usize = 0;
const VALUES: usize = 1;
const INPUT_PAGE: usize = 2;
const CONTAINERS_PAGE: usize = 3;
const COMPOSITE_PAGE: usize = 4;
const VALUE_LABEL_HEIGHT: f32 = 16.0;
const PROGRESS_TRACK_WIDTH: f32 = 320.0;
const PROGRESS_TRACK_HEIGHT: f32 = 24.0;
const SLIDER_TRACK_WIDTH: f32 = 320.0;
const SLIDER_TRACK_HEIGHT: f32 = 7.0;
const SLIDER_HANDLE_SIZE: f32 = 16.0;
const HP_INITIAL: f32 = 0.67;
const MP_SAMPLE: f32 = 0.40;
const EXP_SAMPLE: f32 = 0.82;
const INPUT_WIDTH: f32 = 320.0;
/// Authored chat-field height. The white edit surface sits inside its 10 px border.
const INPUT_HEIGHT: f32 = 40.0;
const INPUT_TEXT_PAD: f32 = 6.0;
const INPUT_FILL: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const SCROLL_ITEMS: usize = 24;
const SCROLL_VISIBLE: usize = 6;
const SCROLL_ROW: f32 = 18.0;
const SCROLL_LIST_WIDTH: f32 = 260.0;
const SCROLL_GAP: f32 = 4.0;
const SCROLL_ARROW: f32 = 16.0;
const SECTION_BUTTON_WIDTH: f32 = 200.0;
const SLOT_SIZE: f32 = 36.0;
const SLOT_GAP: f32 = 6.0;
const HOTBAR_COUNT: usize = 5;
const HOTBAR_SLOT: f32 = 40.0;
const HOTBAR_GAP: f32 = 6.0;
const HOTBAR_COUNTER_SLOT: usize = 1;
const HOTBAR_COUNTER_VALUE: u32 = 12;
const HOTBAR_COUNTER_SIZE: [f32; 2] = [28.0, 22.0];
const BADGE_SIZE: f32 = 14.0;
const TOOLTIP_INNER_PAD_X: f32 = 8.0;
const TOOLTIP_INNER_PAD_Y: f32 = 6.0;
const TOOLTIP_LINE_GAP: f32 = 4.0;
const TOOLTIP_MAX_WIDTH: f32 = 220.0;
const TOOLTIP_POINTER: [f32; 2] = [10.0, 6.0];
const TOOLTIP_BODY: [f32; 4] = [0.74, 0.78, 0.82, 1.0];
const TOOLTIP_ITEM_TITLE: [f32; 4] = [0.93, 0.72, 0.28, 1.0];
const TOOLTIP_DIVIDER_HEIGHT: f32 = 8.0;
const SELECTION_FILL: [f32; 4] = [0.62, 0.78, 0.95, 0.55];
const NOTIFICATION_PULSE_MS: u32 = 180;
const CHAT_BORDER: [f32; 4] = [10.0, 10.0, 10.0, 10.0];
const SCROLL_TRACK_BORDER: [f32; 4] = [8.0, 10.0, 8.0, 10.0];
const TOOLTIP_BORDER: [f32; 4] = [14.0, 14.0, 14.0, 18.0];
const HOTBAR_BORDER: [f32; 4] = [14.0, 14.0, 14.0, 18.0];
const COUNTER_BORDER: [f32; 4] = [6.0, 6.0, 6.0, 6.0];
const SECTION_ROWS: [&str; 3] = ["Row A", "Row B", "Row C"];
/// Dev-only dialog id. It does not overlap drop sequence ids or the death modal.
pub(crate) const UI_DEBUG_MESSAGE_DIALOG_ID: u64 = 0xA11D_DB60;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiDebugEvent {
    OpenMessageDialog,
}

#[must_use]
pub(crate) fn text_input_owns_keyboard(visible: bool, on_input_page: bool, focused: bool) -> bool {
    visible && on_input_page && focused
}

/// Printable U toggles UI DEBUG only while the text field does not own the keyboard.
#[must_use]
pub(crate) fn u_key_toggles_ui_debug(text_input_owns_keyboard: bool) -> bool {
    !text_input_owns_keyboard
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UiDebugRadio {
    Low,
    Medium,
    High,
}

impl UiDebugRadio {
    fn index(self) -> usize {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
        }
    }

    fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Low),
            1 => Some(Self::Medium),
            2 => Some(Self::High),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }
}

struct UiDebugState {
    click_count: u32,
    icon_click_count: u32,
    checkbox: bool,
    radio: UiDebugRadio,
    hp_value: f32,
    input_text: String,
    scroll_offset: usize,
    section_open: bool,
    selected_slot: usize,
    selected_hotbar_slot: usize,
    notification_count: u32,
    notification_pulse_at: Option<Instant>,
    notification_pulse_generation: u64,
}

impl Default for UiDebugState {
    fn default() -> Self {
        Self {
            click_count: 0,
            icon_click_count: 0,
            checkbox: false,
            radio: UiDebugRadio::Medium,
            hp_value: HP_INITIAL,
            input_text: "Hello world".to_owned(),
            scroll_offset: 0,
            section_open: false,
            selected_slot: 2,
            selected_hotbar_slot: 0,
            notification_count: 0,
            notification_pulse_at: None,
            notification_pulse_generation: 0,
        }
    }
}

struct TextButtons {
    states: [UiV2NineSlice; 4],
}

impl TextButtons {
    fn asset(&self, visual: UiButtonVisual) -> &UiV2NineSlice {
        &self.states[match visual {
            UiButtonVisual::Normal => 0,
            UiButtonVisual::Hover => 1,
            UiButtonVisual::Pressed => 2,
            UiButtonVisual::Disabled => 3,
        }]
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn texture(&self, visual: UiButtonVisual) -> SpriteTextureId {
        self.asset(visual).texture
    }
}

pub(crate) struct UiDebugAssets {
    panel: UiV2NineSlice,
    header: UiV2NineSlice,
    tabbed_body: UiV2NineSlice,
    tabbed_rail: SpriteTextureId,
    close: [SpriteTextureId; 3],
    tabs: UiTabSkin,
    buttons: TextButtons,
    icon_buttons: UiButtonSkin,
    checkboxes: UiCheckboxSkin,
    radios: UiRadioSkin,
    slider_track: UiV2NineSlice,
    slider_handles: UiSliderSkin,
    status_track: UiV2NineSlice,
    status_hp: UiV2NineSlice,
    status_mp: UiV2NineSlice,
    status_exp: UiV2NineSlice,
    gear: SpriteTextureId,
    chat_input: UiV2NineSlice,
    scroll_track: UiV2NineSlice,
    scroll_thumbs: [SpriteTextureId; 4],
    scroll_up: [SpriteTextureId; 3],
    scroll_down: [SpriteTextureId; 3],
    tooltip_body: UiV2NineSlice,
    tooltip_pointer: SpriteTextureId,
    tooltip_divider: UiV2NineSlice,
    slots: [SpriteTextureId; 4],
    hotbar_body: UiV2NineSlice,
    hotbar_slots: [SpriteTextureId; 4],
    hotbar_counter: UiV2NineSlice,
    notification_badge: SpriteTextureId,
    tab_overlap: f32,
}

impl UiDebugAssets {
    pub(crate) fn load(runtime: &mut AssetRuntime) -> Result<Self, String> {
        let catalog = load_ui_v2_catalog(runtime)?;
        let tab_overlap = tab_overlap_units(&catalog, TAB_HEIGHT)?;
        let mut loader = ClientAssetLoader::new(runtime);
        let panel = load_ui_v2_nine_slice(&mut loader, &catalog, "panel_body_9slice")?;
        let header = load_ui_v2_nine_slice(&mut loader, &catalog, "panel_header_9slice")?;
        let tabbed_body =
            load_ui_v2_nine_slice(&mut loader, &catalog, "tabbed_content_body_9slice")?;
        let tabbed_rail = load_ui_v2_image(&mut loader, &catalog, "tabbed_content_top_edge")?;
        let close = textures(load_ui_v2_state_family(
            &mut loader,
            &catalog,
            &[
                "close_button_normal",
                "close_button_hover",
                "close_button_pressed",
            ],
        )?)?;
        let tabs = UiTabSkin::from_family(family(
            &mut loader,
            &catalog,
            &["tab_active", "tab_inactive", "tab_hover", "tab_pressed"],
        )?);
        let buttons = TextButtons {
            states: load_v2_text_buttons(
                &mut loader,
                &catalog,
                &[
                    "button_normal",
                    "button_hover",
                    "button_pressed",
                    "button_disabled",
                ],
            )?
            .try_into()
            .map_err(|_| "V2 text button family length is not 4".to_string())?,
        };
        let icon_buttons = UiButtonSkin::from_family(family(
            &mut loader,
            &catalog,
            &[
                "menu_icon_button_normal",
                "menu_icon_button_hover",
                "menu_icon_button_pressed",
                "menu_icon_button_disabled",
            ],
        )?);
        let checkboxes = UiCheckboxSkin::from_family(family(
            &mut loader,
            &catalog,
            &[
                "checkbox_unchecked",
                "checkbox_hover",
                "checkbox_checked",
                "checkbox_disabled",
            ],
        )?);
        let radios = UiRadioSkin::from_family(family(
            &mut loader,
            &catalog,
            &["radio_off", "radio_hover", "radio_on", "radio_disabled"],
        )?);
        let slider_track = load_ui_v2_nine_slice(&mut loader, &catalog, "slider_track_9slice")?;
        let slider_handles = UiSliderSkin::from_textures(textures(load_ui_v2_state_family(
            &mut loader,
            &catalog,
            &[
                "slider_handle_normal",
                "slider_handle_hover",
                "slider_handle_pressed",
            ],
        )?)?);
        let status_track = load_ui_v2_nine_slice(&mut loader, &catalog, "status_track_9slice")?;
        let status_hp = load_ui_v2_nine_slice(&mut loader, &catalog, "status_hp_fill_9slice")?;
        let status_mp = load_ui_v2_nine_slice(&mut loader, &catalog, "status_mp_fill_9slice")?;
        let status_exp = load_ui_v2_nine_slice(&mut loader, &catalog, "status_exp_fill_9slice")?;
        let gear = load_ui_v2_image(&mut loader, &catalog, "icon_gear")?.texture;
        let chat_input = load_ui_v2_nine_slice(&mut loader, &catalog, "chat_input_9slice")?;
        let scroll_track =
            load_ui_v2_nine_slice(&mut loader, &catalog, "scrollbar_track_vertical_9slice")?;
        let scroll_thumbs = image_textures(family(
            &mut loader,
            &catalog,
            &[
                "scrollbar_thumb_normal",
                "scrollbar_thumb_hover",
                "scrollbar_thumb_pressed",
                "scrollbar_thumb_disabled",
            ],
        )?);
        let scroll_up = textures(load_ui_v2_state_family(
            &mut loader,
            &catalog,
            &[
                "scroll_arrow_up_normal",
                "scroll_arrow_up_hover",
                "scroll_arrow_up_pressed",
            ],
        )?)?;
        let scroll_down = textures(load_ui_v2_state_family(
            &mut loader,
            &catalog,
            &[
                "scroll_arrow_down_normal",
                "scroll_arrow_down_hover",
                "scroll_arrow_down_pressed",
            ],
        )?)?;
        let tooltip_body = load_ui_v2_nine_slice(&mut loader, &catalog, "tooltip_body_9slice")?;
        let tooltip_pointer = load_ui_v2_image(&mut loader, &catalog, "tooltip_pointer")?.texture;
        let tooltip_divider = load_ui_v2_nine_slice(&mut loader, &catalog, "divider_plain_9slice")?;
        let slots = image_textures(family(
            &mut loader,
            &catalog,
            &[
                "slot_normal",
                "slot_hover",
                "slot_selected",
                "slot_disabled",
            ],
        )?);
        let hotbar_body = load_ui_v2_nine_slice(&mut loader, &catalog, "hotbar_body_9slice")?;
        let hotbar_slots = image_textures(family(
            &mut loader,
            &catalog,
            &[
                "hotbar_slot_normal",
                "hotbar_slot_hover",
                "hotbar_slot_selected",
                "hotbar_slot_disabled",
            ],
        )?);
        let hotbar_counter =
            load_ui_v2_nine_slice(&mut loader, &catalog, "hotbar_counter_badge_9slice")?;
        let notification_badge =
            load_ui_v2_image(&mut loader, &catalog, "notification_badge")?.texture;
        Ok(Self {
            panel,
            header,
            tabbed_body,
            tabbed_rail: tabbed_rail.texture,
            close,
            tabs,
            buttons,
            icon_buttons,
            checkboxes,
            radios,
            slider_track,
            slider_handles,
            status_track,
            status_hp,
            status_mp,
            status_exp,
            gear,
            chat_input,
            scroll_track,
            scroll_thumbs,
            scroll_up,
            scroll_down,
            tooltip_body,
            tooltip_pointer,
            tooltip_divider,
            slots,
            hotbar_body,
            hotbar_slots,
            hotbar_counter,
            notification_badge,
            tab_overlap,
        })
    }
}

pub(crate) struct UiDebugFrame {
    pub(crate) rects: Vec<UiRect>,
    pub(crate) skin_quads: Vec<UiTexturedQuad>,
    pub(crate) texts: Vec<TextBlock>,
}

struct DebugLayout {
    window: ScreenRect,
    header: ScreenRect,
    close_button: ScreenRect,
    gear: ScreenRect,
    tabs: [ScreenRect; PAGES.len()],
    inner: ScreenRect,
    click_button: ScreenRect,
    icon_button: ScreenRect,
    icon_glyph: ScreenRect,
    disabled_button: ScreenRect,
    checkbox_row: ScreenRect,
    checkbox_mark: ScreenRect,
    disabled_checkbox_row: ScreenRect,
    disabled_checkbox_mark: ScreenRect,
    radio_rows: [ScreenRect; RADIO_LABELS.len()],
    radio_marks: [ScreenRect; RADIO_LABELS.len()],
    hp_label: ScreenRect,
    hp_track: ScreenRect,
    hp_slider_track: ScreenRect,
    mp_label: ScreenRect,
    mp_track: ScreenRect,
    exp_label: ScreenRect,
    exp_track: ScreenRect,
    input: InputPlaces,
    containers: ContainerPlaces,
    composite: CompositePlaces,
}

pub(crate) struct UiDebugWindow {
    assets: UiDebugAssets,
    chrome: ProofPanelWindow,
    tabs: UiTabs,
    click_button: UiButton,
    icon_button: UiButton,
    disabled_button: UiButton,
    checkbox: UiCheckbox,
    disabled_checkbox: UiCheckbox,
    radios: UiRadioGroup,
    hp_slider: UiSlider,
    text_input: UiTextInput,
    scrollbar: UiScrollbar,
    scroll_up: UiButton,
    scroll_down: UiButton,
    section_button: UiButton,
    slots: UiRadioGroup,
    hotbar_slots: UiRadioGroup,
    notify_button: UiButton,
    clear_button: UiButton,
    dialog_button: UiButton,
    pending_event: Option<UiDebugEvent>,
    state: UiDebugState,
}

impl UiDebugWindow {
    pub(crate) fn load(runtime: &mut AssetRuntime) -> Result<Self, String> {
        Ok(Self::with_assets(UiDebugAssets::load(runtime)?))
    }

    fn with_assets(assets: UiDebugAssets) -> Self {
        Self {
            assets,
            chrome: ProofPanelWindow::with_size(UI_DEBUG_WINDOW_UNITS),
            tabs: UiTabs::default(),
            click_button: UiButton::default(),
            icon_button: UiButton::default(),
            disabled_button: UiButton::default(),
            checkbox: UiCheckbox::default(),
            disabled_checkbox: UiCheckbox::default(),
            radios: UiRadioGroup::default(),
            hp_slider: UiSlider::default(),
            text_input: UiTextInput::default(),
            scrollbar: UiScrollbar::default(),
            scroll_up: UiButton::default(),
            scroll_down: UiButton::default(),
            section_button: UiButton::default(),
            slots: UiRadioGroup::default(),
            hotbar_slots: UiRadioGroup::default(),
            notify_button: UiButton::default(),
            clear_button: UiButton::default(),
            dialog_button: UiButton::default(),
            pending_event: None,
            state: UiDebugState::default(),
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.chrome.is_visible()
    }

    pub(crate) fn toggle(&mut self) {
        if self.is_visible() {
            self.chrome.close();
        } else {
            self.chrome.open();
        }
        self.cancel_pointer_interaction();
    }

    pub(crate) fn wants_text_keyboard(&self) -> bool {
        text_input_owns_keyboard(
            self.is_visible(),
            self.tabs.selected_index() == INPUT_PAGE,
            self.text_input.is_focused(),
        )
    }

    pub(crate) fn blur_text_input(&mut self) {
        self.text_input.blur();
    }

    pub(crate) fn apply_text_key(
        &mut self,
        key: &Key,
        text: Option<&str>,
        repeat: bool,
        nav: UiTextNav,
        scale: f32,
    ) -> bool {
        if !self.wants_text_keyboard() {
            return false;
        }
        self.text_input.apply_key(
            &mut self.state.input_text,
            key,
            text,
            repeat,
            nav,
            input_metrics(scale),
        )
    }

    pub(crate) fn take_event(&mut self) -> Option<UiDebugEvent> {
        self.pending_event.take()
    }

    pub(crate) fn apply_wheel(
        &mut self,
        delta: MouseScrollDelta,
        cursor: Option<[f32; 2]>,
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        if !self.is_visible() || self.tabs.selected_index() != CONTAINERS_PAGE {
            return false;
        }
        let Some(point) = cursor else {
            return false;
        };
        let Ok(Some(layout)) = self.layout(viewport, pixels_per_unit) else {
            return false;
        };
        if !layout.containers.scroll.region.contains(point) {
            return false;
        }
        let max_offset = scroll_max_offset(SCROLL_ITEMS, SCROLL_VISIBLE);
        self.state.scroll_offset = scroll_offset_after_wheel(
            self.state.scroll_offset,
            max_offset,
            scroll_rows_from_wheel(delta),
        );
        true
    }

    pub(crate) fn frame(
        &mut self,
        viewport: PixelViewport,
        pixels_per_unit: f32,
        cursor: Option<[f32; 2]>,
    ) -> Result<Option<UiDebugFrame>, String> {
        if self.is_visible() {
            self.sync_page_interaction();
        }
        let Some(layout) = self.layout(viewport, pixels_per_unit)? else {
            return Ok(None);
        };
        Ok(Some(compose(self, &layout, cursor, pixels_per_unit)?))
    }

    pub(crate) fn pointer_moved(
        &mut self,
        cursor: [f32; 2],
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        if !self.is_visible() {
            return false;
        }
        self.sync_page_interaction();
        if self.hp_slider.is_dragging() {
            let Ok(Some(layout)) = self.layout(viewport, pixels_per_unit) else {
                self.hp_slider.cancel();
                return false;
            };
            return match self.hp_slider.pointer_moved(cursor, layout.hp_slider_track) {
                Ok(UiSliderOutcome::Changed(value)) => {
                    self.state.hp_value = value;
                    true
                }
                Ok(_) => false,
                Err(_) => false,
            };
        }
        if self.text_input.is_dragging() && self.tabs.selected_index() == INPUT_PAGE {
            let Ok(Some(layout)) = self.layout(viewport, pixels_per_unit) else {
                self.text_input.end_drag();
                return false;
            };
            return self.text_input.apply_drag(
                input_local_x(
                    cursor[0],
                    layout.input.field,
                    pixels_per_unit,
                    self.text_input.scroll(),
                ),
                &self.state.input_text,
                input_metrics(pixels_per_unit),
            );
        }
        if self.scrollbar.is_dragging() {
            let Ok(Some(layout)) = self.layout(viewport, pixels_per_unit) else {
                self.scrollbar.cancel();
                return false;
            };
            let max_offset = scroll_max_offset(SCROLL_ITEMS, SCROLL_VISIBLE);
            if let Some(offset) =
                self.scrollbar
                    .pointer_moved(cursor[1], &layout.containers.scroll, max_offset)
            {
                self.state.scroll_offset = offset;
                return true;
            }
            return false;
        }
        self.chrome.pointer_moved(cursor, viewport, pixels_per_unit)
    }

    pub(crate) fn contains_window(
        &mut self,
        cursor: [f32; 2],
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        self.layout(viewport, pixels_per_unit)
            .ok()
            .flatten()
            .is_some_and(|layout| layout.window.contains(cursor))
    }

    pub(crate) fn apply_pointer_button(
        &mut self,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        if !self.is_visible() {
            return false;
        }
        let Ok(Some(layout)) = self.layout(viewport, pixels_per_unit) else {
            self.cancel_pointer_interaction();
            return false;
        };
        if self.hp_slider.is_dragging() && self.tabs.selected_index() != VALUES {
            self.hp_slider.cancel();
            if state == ElementState::Released {
                return true;
            }
        }
        if self.scrollbar.is_dragging() && self.tabs.selected_index() != CONTAINERS_PAGE {
            self.scrollbar.cancel();
            if state == ElementState::Released {
                return true;
            }
        }
        if self.text_input.is_dragging() {
            if state == ElementState::Released || self.tabs.selected_index() != INPUT_PAGE {
                self.text_input.end_drag();
            }
            if state == ElementState::Released {
                return true;
            }
        }
        if self.hp_slider.is_dragging() {
            return self.route_slider_capture(state, cursor, &layout, pixels_per_unit);
        }
        if self.scrollbar.is_dragging() {
            return self.route_scroll_capture(state, cursor, &layout);
        }
        match state {
            ElementState::Released => {
                if self.deliver_release(&layout, cursor) {
                    return true;
                }
                self.apply_chrome(ElementState::Released, cursor, &layout, pixels_per_unit)
            }
            ElementState::Pressed => {
                if self.press_tab(&layout, cursor) {
                    return true;
                }
                if self.press_page(&layout, cursor, pixels_per_unit) {
                    return true;
                }
                self.apply_chrome(ElementState::Pressed, cursor, &layout, pixels_per_unit)
            }
        }
    }

    pub(crate) fn cancel_pointer_interaction(&mut self) {
        self.chrome.cancel_pointer_interaction();
        self.tabs.cancel_pointer_interaction();
        self.click_button.cancel();
        self.icon_button.cancel();
        self.disabled_button.cancel();
        self.checkbox.cancel();
        self.disabled_checkbox.cancel();
        self.radios.cancel();
        self.hp_slider.cancel();
        self.text_input.blur();
        self.scrollbar.cancel();
        self.scroll_up.cancel();
        self.scroll_down.cancel();
        self.section_button.cancel();
        self.slots.cancel();
        self.hotbar_slots.cancel();
        self.notify_button.cancel();
        self.clear_button.cancel();
        self.dialog_button.cancel();
    }

    fn sync_page_interaction(&mut self) {
        let page = self.tabs.selected_index();
        if page != VALUES {
            self.hp_slider.cancel();
        }
        if page != CONTAINERS_PAGE {
            self.scrollbar.cancel();
            self.scroll_up.cancel();
            self.scroll_down.cancel();
            self.section_button.cancel();
        }
        if page != INPUT_PAGE {
            self.text_input.blur();
        }
        if page != COMPOSITE_PAGE {
            self.slots.cancel();
            self.hotbar_slots.cancel();
            self.notify_button.cancel();
            self.clear_button.cancel();
            self.dialog_button.cancel();
        }
        if page != BASICS {
            self.click_button.cancel();
            self.icon_button.cancel();
            self.disabled_button.cancel();
            self.checkbox.cancel();
            self.disabled_checkbox.cancel();
            self.radios.cancel();
        }
    }

    fn layout(
        &mut self,
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> Result<Option<DebugLayout>, String> {
        let Some(window) = self.chrome.placed_bounds(viewport, pixels_per_unit)? else {
            return Ok(None);
        };
        layout_debug(
            window,
            self.assets.tab_overlap,
            pixels_per_unit,
            self.state.scroll_offset,
        )
        .map(Some)
    }

    fn apply_chrome(
        &mut self,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        layout: &DebugLayout,
        pixels_per_unit: f32,
    ) -> bool {
        self.chrome.apply_chrome_pointer(
            Some(WindowChromeLayout {
                window: layout.window,
                header: layout.header,
                close_button: layout.close_button,
            }),
            state,
            cursor,
            pixels_per_unit,
        )
    }

    fn press_tab(&mut self, layout: &DebugLayout, cursor: Option<[f32; 2]>) -> bool {
        self.tabs.apply_pointer_button(
            ElementState::Pressed,
            cursor,
            tab_strip(&layout.tabs),
            PAGES.len(),
            0.0,
        )
    }

    fn press_controls(&mut self, layout: &DebugLayout, cursor: Option<[f32; 2]>) -> bool {
        if self
            .click_button
            .apply(ElementState::Pressed, cursor, layout.click_button, true)
            != UiPointerOutcome::Idle
        {
            return true;
        }
        if self
            .icon_button
            .apply(ElementState::Pressed, cursor, layout.icon_button, true)
            != UiPointerOutcome::Idle
        {
            return true;
        }
        if self
            .disabled_button
            .apply(ElementState::Pressed, cursor, layout.disabled_button, false)
            != UiPointerOutcome::Idle
        {
            return true;
        }
        if self
            .checkbox
            .apply(ElementState::Pressed, cursor, layout.checkbox_row, true)
            != UiPointerOutcome::Idle
        {
            return true;
        }
        if self.disabled_checkbox.apply(
            ElementState::Pressed,
            cursor,
            layout.disabled_checkbox_row,
            false,
        ) != UiPointerOutcome::Idle
        {
            return true;
        }
        let hits = radio_hits(layout);
        self.radios.apply(ElementState::Pressed, cursor, &hits) != UiRadioOutcome::Idle
    }

    fn press_slider(&mut self, layout: &DebugLayout, cursor: Option<[f32; 2]>, scale: f32) -> bool {
        let Ok(geometry) = self.slider_geometry(layout, scale) else {
            return false;
        };
        match self
            .hp_slider
            .apply(ElementState::Pressed, cursor, geometry)
        {
            Ok(outcome) => {
                let captured = outcome != UiSliderOutcome::Idle;
                self.store_slider_outcome(outcome);
                captured
            }
            Err(_) => false,
        }
    }

    fn route_slider_capture(
        &mut self,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        layout: &DebugLayout,
        scale: f32,
    ) -> bool {
        if state == ElementState::Pressed {
            return true;
        }
        let Ok(geometry) = self.slider_geometry(layout, scale) else {
            self.hp_slider.cancel();
            return true;
        };
        match self.hp_slider.apply(state, cursor, geometry) {
            Ok(outcome) => self.store_slider_outcome(outcome),
            Err(_) => self.hp_slider.cancel(),
        }
        true
    }

    fn slider_geometry(
        &self,
        layout: &DebugLayout,
        scale: f32,
    ) -> Result<UiSliderGeometry, String> {
        slider_geometry(
            layout.hp_slider_track,
            SLIDER_HANDLE_SIZE * scale,
            self.state.hp_value,
        )
    }

    fn store_slider_outcome(&mut self, outcome: UiSliderOutcome) {
        if let UiSliderOutcome::Changed(value) = outcome {
            self.state.hp_value = value;
        }
    }

    fn deliver_release(&mut self, layout: &DebugLayout, cursor: Option<[f32; 2]>) -> bool {
        if self.click_button.is_pressed() {
            if self
                .click_button
                .apply(ElementState::Released, cursor, layout.click_button, true)
                == UiPointerOutcome::Activated
            {
                self.state.click_count = self.state.click_count.saturating_add(1);
            }
            return true;
        }
        if self.icon_button.is_pressed() {
            if self
                .icon_button
                .apply(ElementState::Released, cursor, layout.icon_button, true)
                == UiPointerOutcome::Activated
            {
                self.state.icon_click_count = self.state.icon_click_count.saturating_add(1);
            }
            return true;
        }
        if self.disabled_button.is_pressed() {
            let _ = self.disabled_button.apply(
                ElementState::Released,
                cursor,
                layout.disabled_button,
                false,
            );
            return true;
        }
        if self.checkbox.is_pressed() {
            if self
                .checkbox
                .apply(ElementState::Released, cursor, layout.checkbox_row, true)
                == UiPointerOutcome::Activated
            {
                self.state.checkbox = !self.state.checkbox;
            }
            return true;
        }
        if self.disabled_checkbox.is_pressed() {
            let _ = self.disabled_checkbox.apply(
                ElementState::Released,
                cursor,
                layout.disabled_checkbox_row,
                false,
            );
            return true;
        }
        if self.radios.is_pressed() {
            let hits = radio_hits(layout);
            if let UiRadioOutcome::Selected(index) =
                self.radios.apply(ElementState::Released, cursor, &hits)
                && let Some(value) = UiDebugRadio::from_index(index)
            {
                self.state.radio = value;
            }
            return true;
        }
        if self.scroll_up.is_pressed() {
            if self.scroll_up.apply(
                ElementState::Released,
                cursor,
                layout.containers.scroll.up,
                true,
            ) == UiPointerOutcome::Activated
            {
                self.state.scroll_offset = self.state.scroll_offset.saturating_sub(1);
            }
            return true;
        }
        if self.scroll_down.is_pressed() {
            if self.scroll_down.apply(
                ElementState::Released,
                cursor,
                layout.containers.scroll.down,
                true,
            ) == UiPointerOutcome::Activated
            {
                let max_offset = scroll_max_offset(SCROLL_ITEMS, SCROLL_VISIBLE);
                self.state.scroll_offset = (self.state.scroll_offset + 1).min(max_offset);
            }
            return true;
        }
        if self.section_button.is_pressed() {
            if self.section_button.apply(
                ElementState::Released,
                cursor,
                layout.containers.section_button,
                true,
            ) == UiPointerOutcome::Activated
            {
                self.state.section_open = !self.state.section_open;
            }
            return true;
        }
        if self.slots.is_pressed() {
            if let UiRadioOutcome::Selected(index) = self.slots.apply(
                ElementState::Released,
                cursor,
                &slot_hits(&layout.composite.slots),
            ) {
                self.state.selected_slot = index;
            }
            return true;
        }
        if self.hotbar_slots.is_pressed() {
            if let UiRadioOutcome::Selected(index) = self.hotbar_slots.apply(
                ElementState::Released,
                cursor,
                &hotbar_hits(&layout.composite.hotbar_slots),
            ) {
                self.state.selected_hotbar_slot = index;
            }
            return true;
        }
        if self.notify_button.is_pressed() {
            if self.notify_button.apply(
                ElementState::Released,
                cursor,
                layout.composite.notify_button,
                true,
            ) == UiPointerOutcome::Activated
            {
                self.state.notification_count = self.state.notification_count.saturating_add(1);
                self.state.notification_pulse_at = Some(Instant::now());
                self.state.notification_pulse_generation =
                    self.state.notification_pulse_generation.saturating_add(1);
            }
            return true;
        }
        if self.clear_button.is_pressed() {
            if self.clear_button.apply(
                ElementState::Released,
                cursor,
                layout.composite.clear_button,
                true,
            ) == UiPointerOutcome::Activated
            {
                self.state.notification_count = 0;
            }
            return true;
        }
        if self.dialog_button.is_pressed() {
            if self.dialog_button.apply(
                ElementState::Released,
                cursor,
                layout.composite.dialog_button,
                true,
            ) == UiPointerOutcome::Activated
            {
                self.pending_event = Some(UiDebugEvent::OpenMessageDialog);
            }
            return true;
        }
        if self.tabs.pressed_index().is_some() {
            let previous = self.tabs.selected_index();
            let handled = self.tabs.apply_pointer_button(
                ElementState::Released,
                cursor,
                tab_strip(&layout.tabs),
                PAGES.len(),
                0.0,
            );
            if self.tabs.selected_index() != previous {
                self.sync_page_interaction();
            }
            return handled;
        }
        false
    }

    fn press_page(&mut self, layout: &DebugLayout, cursor: Option<[f32; 2]>, scale: f32) -> bool {
        match self.tabs.selected_index() {
            BASICS => self.press_controls(layout, cursor),
            VALUES => self.press_slider(layout, cursor, scale),
            INPUT_PAGE => {
                let inside = inside(layout.input.field, cursor);
                let local_x = cursor
                    .map(|point| {
                        input_local_x(
                            point[0],
                            layout.input.field,
                            scale,
                            self.text_input.scroll(),
                        )
                    })
                    .unwrap_or(0.0);
                self.text_input.apply_press(
                    inside,
                    local_x,
                    &self.state.input_text,
                    input_metrics(scale),
                )
            }
            CONTAINERS_PAGE => self.press_containers(layout, cursor),
            COMPOSITE_PAGE => self.press_composite(layout, cursor),
            _ => false,
        }
    }

    fn press_containers(&mut self, layout: &DebugLayout, cursor: Option<[f32; 2]>) -> bool {
        let max_offset = scroll_max_offset(SCROLL_ITEMS, SCROLL_VISIBLE);
        if self.scrollbar.apply_thumb(
            ElementState::Pressed,
            cursor,
            &layout.containers.scroll,
            max_offset,
        ) {
            return true;
        }
        if self.scroll_up.apply(
            ElementState::Pressed,
            cursor,
            layout.containers.scroll.up,
            true,
        ) != UiPointerOutcome::Idle
        {
            return true;
        }
        if self.scroll_down.apply(
            ElementState::Pressed,
            cursor,
            layout.containers.scroll.down,
            true,
        ) != UiPointerOutcome::Idle
        {
            return true;
        }
        self.section_button.apply(
            ElementState::Pressed,
            cursor,
            layout.containers.section_button,
            true,
        ) != UiPointerOutcome::Idle
    }

    fn press_composite(&mut self, layout: &DebugLayout, cursor: Option<[f32; 2]>) -> bool {
        if self.slots.apply(
            ElementState::Pressed,
            cursor,
            &slot_hits(&layout.composite.slots),
        ) != UiRadioOutcome::Idle
        {
            return true;
        }
        if self.hotbar_slots.apply(
            ElementState::Pressed,
            cursor,
            &hotbar_hits(&layout.composite.hotbar_slots),
        ) != UiRadioOutcome::Idle
        {
            return true;
        }
        if self.notify_button.apply(
            ElementState::Pressed,
            cursor,
            layout.composite.notify_button,
            true,
        ) != UiPointerOutcome::Idle
        {
            return true;
        }
        if self.clear_button.apply(
            ElementState::Pressed,
            cursor,
            layout.composite.clear_button,
            true,
        ) != UiPointerOutcome::Idle
        {
            return true;
        }
        self.dialog_button.apply(
            ElementState::Pressed,
            cursor,
            layout.composite.dialog_button,
            true,
        ) != UiPointerOutcome::Idle
    }

    fn route_scroll_capture(
        &mut self,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        layout: &DebugLayout,
    ) -> bool {
        if state == ElementState::Pressed {
            return true;
        }
        let max_offset = scroll_max_offset(SCROLL_ITEMS, SCROLL_VISIBLE);
        if let Some(point) = cursor
            && let Some(offset) =
                self.scrollbar
                    .pointer_moved(point[1], &layout.containers.scroll, max_offset)
        {
            self.state.scroll_offset = offset;
        }
        self.scrollbar.cancel();
        true
    }
}

fn family(
    loader: &mut ClientAssetLoader<'_>,
    catalog: &UiV2Catalog,
    names: &[&str],
) -> Result<[UiV2Image; 4], String> {
    let loaded = load_ui_v2_state_family(loader, catalog, names)?;
    loaded
        .try_into()
        .map_err(|_| "UI DEBUG state family length is not 4".to_string())
}

fn image_textures(images: [UiV2Image; 4]) -> [SpriteTextureId; 4] {
    [
        images[0].texture,
        images[1].texture,
        images[2].texture,
        images[3].texture,
    ]
}

fn inside(bounds: ScreenRect, cursor: Option<[f32; 2]>) -> bool {
    cursor.is_some_and(|point| bounds.contains(point))
}

fn slot_enabled(index: usize) -> bool {
    index != 3
}

fn hotbar_enabled(index: usize) -> bool {
    index + 1 != HOTBAR_COUNT
}

fn slot_hits(slots: &[ScreenRect; 4]) -> [(ScreenRect, bool); 4] {
    let mut hits = [(slots[0], false); 4];
    for (index, bounds) in slots.iter().copied().enumerate() {
        hits[index] = (bounds, slot_enabled(index));
    }
    hits
}

fn hotbar_hits(slots: &[ScreenRect; HOTBAR_COUNT]) -> [(ScreenRect, bool); HOTBAR_COUNT] {
    let mut hits = [(slots[0], false); HOTBAR_COUNT];
    for (index, bounds) in slots.iter().copied().enumerate() {
        hits[index] = (bounds, hotbar_enabled(index));
    }
    hits
}

fn textures(images: Vec<UiV2Image>) -> Result<[SpriteTextureId; 3], String> {
    let images: [UiV2Image; 3] = images
        .try_into()
        .map_err(|_| "UI DEBUG state family length is not 3".to_string())?;
    Ok([images[0].texture, images[1].texture, images[2].texture])
}

fn tab_overlap_units(catalog: &UiV2Catalog, tab_height: f32) -> Result<f32, String> {
    let names = ["tab_active", "tab_inactive", "tab_hover", "tab_pressed"];
    let mut overlap = None;
    let mut height = None;
    for name in names {
        let info = catalog.get(name)?;
        let value = info
            .content_overlap
            .ok_or_else(|| format!("UI V2 asset {name} is missing contentOverlap"))?;
        if let Some(previous) = overlap
            && previous != value
        {
            return Err(format!(
                "UI V2 asset {name} contentOverlap {value} does not match {previous}"
            ));
        }
        if let Some(previous) = height
            && previous != info.height
        {
            return Err(format!(
                "UI V2 asset {name} height {} does not match {previous}",
                info.height
            ));
        }
        overlap = Some(value);
        height = Some(info.height);
    }
    let (overlap, height) = overlap.zip(height).ok_or("UI V2 tab family is empty")?;
    if overlap == 0 || overlap >= height {
        return Err(format!(
            "UI V2 tab contentOverlap {overlap} does not fit height {height}"
        ));
    }
    Ok(overlap as f32 / height as f32 * tab_height)
}

fn radio_enabled(index: usize) -> bool {
    index < 3
}

fn radio_hits(layout: &DebugLayout) -> [(ScreenRect, bool); RADIO_LABELS.len()] {
    let mut hits = [(layout.radio_rows[0], false); RADIO_LABELS.len()];
    for (index, row) in layout.radio_rows.iter().copied().enumerate() {
        hits[index] = (row, radio_enabled(index));
    }
    hits
}

fn tab_strip(tabs: &[ScreenRect; PAGES.len()]) -> ScreenRect {
    ScreenRect {
        min: tabs[0].min,
        max: tabs[tabs.len() - 1].max,
    }
}

fn content_origin(inner: ScreenRect, scale: f32) -> [f32; 2] {
    [
        inner.min[0] + (TABBED_BORDER[0] + PAGE_PAD) * scale,
        inner.min[1] + (TABBED_BORDER[1] + PAGE_PAD) * scale,
    ]
}

fn unit_rect(origin: [f32; 2], x: f32, y: f32, width: f32, height: f32, scale: f32) -> ScreenRect {
    let min = [origin[0] + x * scale, origin[1] + y * scale];
    ScreenRect {
        min,
        max: [min[0] + width * scale, min[1] + height * scale],
    }
}

fn centered_mark(row: ScreenRect, scale: f32) -> ScreenRect {
    let size = MARK_SIZE * scale;
    let min = [
        row.min[0] + MARK_INSET * scale,
        row.min[1] + (row.height() - size) * 0.5,
    ];
    ScreenRect {
        min,
        max: [min[0] + size, min[1] + size],
    }
}

fn split_tabs(bounds: ScreenRect) -> [ScreenRect; PAGES.len()] {
    let count = PAGES.len();
    let width = bounds.width() / count as f32;
    let mut tabs = [ScreenRect {
        min: [0.0, 0.0],
        max: [0.0, 0.0],
    }; PAGES.len()];
    for (index, tab) in tabs.iter_mut().enumerate() {
        let min_x = bounds.min[0] + index as f32 * width;
        let max_x = if index + 1 == count {
            bounds.max[0]
        } else {
            min_x + width
        };
        *tab = ScreenRect {
            min: [min_x, bounds.min[1]],
            max: [max_x, bounds.max[1]],
        };
    }
    tabs
}

fn layout_debug(
    window: ScreenRect,
    tab_overlap: f32,
    scale: f32,
    scroll_offset: usize,
) -> Result<DebugLayout, String> {
    if !scale.is_finite() || scale <= 0.0 || !tab_overlap.is_finite() || tab_overlap <= 0.0 {
        return Err("UI DEBUG scale is invalid".to_string());
    }
    if tab_overlap >= TAB_HEIGHT {
        return Err("UI DEBUG tab overlap does not fit the tab".to_string());
    }
    let header = ScreenRect {
        min: [
            window.min[0] + HEADER_INSET_X * scale,
            window.min[1] + HEADER_TOP * scale,
        ],
        max: [
            window.max[0] - HEADER_INSET_X * scale,
            window.min[1] + (HEADER_TOP + HEADER_HEIGHT) * scale,
        ],
    };
    let close_size = CLOSE_SIZE * scale;
    let close_max_x = header.max[0] - HEADER_CONTROL_INSET * scale;
    let close_min_y = header.min[1] + (header.height() - close_size) * 0.5;
    let close_button = ScreenRect {
        min: [close_max_x - close_size, close_min_y],
        max: [close_max_x, close_min_y + close_size],
    };
    let gear_size = GEAR_SIZE * scale;
    let gear_min_x = header.min[0] + HEADER_CONTROL_INSET * scale;
    let gear_min_y = header.min[1] + (header.height() - gear_size) * 0.5;
    let gear = ScreenRect {
        min: [gear_min_x, gear_min_y],
        max: [gear_min_x + gear_size, gear_min_y + gear_size],
    };
    let tab_top = header.max[1] + TAB_HEADER_GAP * scale;
    let inner = ScreenRect {
        min: [header.min[0], tab_top + (TAB_HEIGHT - tab_overlap) * scale],
        max: [header.max[0], window.max[1] - CONTENT_BOTTOM * scale],
    };
    let strip = ScreenRect {
        min: [
            inner.min[0] + (TABBED_BORDER[0] + TAB_SHOULDER) * scale,
            tab_top,
        ],
        max: [
            inner.max[0] - (TABBED_BORDER[2] + TAB_SHOULDER) * scale,
            tab_top + TAB_HEIGHT * scale,
        ],
    };
    let tabs = split_tabs(strip);
    let origin = content_origin(inner, scale);
    let limit_y = inner.max[1] - (TABBED_BORDER[3] + PAGE_PAD) * scale;
    let mut y = 0.0;
    y += HEADING_HEIGHT;
    let click_button = unit_rect(origin, 0.0, y, BUTTON_WIDTH, BUTTON_HEIGHT, scale);
    y += BUTTON_HEIGHT + ROW_GAP;
    let icon_button = unit_rect(origin, 0.0, y, ICON_BUTTON_SIZE, ICON_BUTTON_SIZE, scale);
    let glyph = ICON_GLYPH_SIZE * scale;
    let icon_glyph = ScreenRect {
        min: [
            icon_button.min[0] + (icon_button.width() - glyph) * 0.5,
            icon_button.min[1] + (icon_button.height() - glyph) * 0.5,
        ],
        max: [
            icon_button.min[0] + (icon_button.width() + glyph) * 0.5,
            icon_button.min[1] + (icon_button.height() + glyph) * 0.5,
        ],
    };
    y += ICON_BUTTON_SIZE + ROW_GAP;
    let disabled_button = unit_rect(origin, 0.0, y, BUTTON_WIDTH, BUTTON_HEIGHT, scale);
    y += BUTTON_HEIGHT + SECTION_GAP + HEADING_HEIGHT;
    let checkbox_row = unit_rect(origin, 0.0, y, ROW_WIDTH, ROW_HEIGHT, scale);
    let checkbox_mark = centered_mark(checkbox_row, scale);
    y += ROW_HEIGHT + ROW_GAP;
    let disabled_checkbox_row = unit_rect(origin, 0.0, y, ROW_WIDTH, ROW_HEIGHT, scale);
    let disabled_checkbox_mark = centered_mark(disabled_checkbox_row, scale);
    y += ROW_HEIGHT + SECTION_GAP + HEADING_HEIGHT;
    let mut radio_rows = [checkbox_row; RADIO_LABELS.len()];
    let mut radio_marks = [checkbox_mark; RADIO_LABELS.len()];
    for index in 0..RADIO_LABELS.len() {
        radio_rows[index] = unit_rect(origin, 0.0, y, ROW_WIDTH, ROW_HEIGHT, scale);
        radio_marks[index] = centered_mark(radio_rows[index], scale);
        y += ROW_HEIGHT;
        if index + 1 != RADIO_LABELS.len() {
            y += ROW_GAP;
        }
    }
    let content_bottom = origin[1] + y * scale;
    let values = place_values(origin, scale);
    let input = place_input(origin, scale);
    let containers = place_containers(origin, scale, scroll_offset)?;
    let composite = place_composite(origin, scale);
    let state_x = origin[0] + STATE_COLUMN_X * scale;
    let fits = gear.max[0] < close_button.min[0]
        && close_button.max[0] <= header.max[0]
        && tabs[0].min[1] < inner.min[1]
        && (tabs[0].max[1] - inner.min[1] - tab_overlap * scale).abs() < 0.05
        && checkbox_mark.max[0] < checkbox_row.max[0]
        && checkbox_row.max[0] < state_x
        && icon_glyph.min[0] > icon_button.min[0]
        && icon_glyph.max[0] < icon_button.max[0]
        && content_bottom <= limit_y + 0.05
        && radio_rows[RADIO_LABELS.len() - 1].max[1] <= limit_y + 0.05
        && values.bottom <= limit_y + 0.05
        && values.hp_track.max[0] <= inner.max[0]
        && values.slider_track.max[1] <= values.mp_label.min[1]
        && input.bottom <= limit_y + 0.05
        && containers.bottom <= limit_y + 0.05
        && composite.bottom <= limit_y + 0.05
        && containers.scroll.region.max[0] <= inner.max[0]
        && composite.dialog_button.max[0] <= inner.max[0]
        && composite.badge.max[1] > composite.badge_button.min[1];
    if !fits {
        return Err(format!(
            "UI DEBUG layout does not fit {UI_DEBUG_WINDOW_UNITS:?} at scale {scale}"
        ));
    }
    Ok(DebugLayout {
        window,
        header,
        close_button,
        gear,
        tabs,
        inner,
        click_button,
        icon_button,
        icon_glyph,
        disabled_button,
        checkbox_row,
        checkbox_mark,
        disabled_checkbox_row,
        disabled_checkbox_mark,
        radio_rows,
        radio_marks,
        hp_label: values.hp_label,
        hp_track: values.hp_track,
        hp_slider_track: values.slider_track,
        mp_label: values.mp_label,
        mp_track: values.mp_track,
        exp_label: values.exp_label,
        exp_track: values.exp_track,
        input,
        containers,
        composite,
    })
}

fn place_values(origin: [f32; 2], scale: f32) -> ValuesPlaces {
    let mut y = HEADING_HEIGHT;
    let hp_label = unit_rect(
        origin,
        0.0,
        y,
        PROGRESS_TRACK_WIDTH,
        VALUE_LABEL_HEIGHT,
        scale,
    );
    y += VALUE_LABEL_HEIGHT;
    let hp_track = unit_rect(
        origin,
        0.0,
        y,
        PROGRESS_TRACK_WIDTH,
        PROGRESS_TRACK_HEIGHT,
        scale,
    );
    y += PROGRESS_TRACK_HEIGHT + ROW_GAP;
    let slider_row = unit_rect(
        origin,
        0.0,
        y,
        SLIDER_TRACK_WIDTH,
        SLIDER_HANDLE_SIZE,
        scale,
    );
    let track_height = SLIDER_TRACK_HEIGHT * scale;
    let track_y = slider_row.min[1] + (slider_row.height() - track_height) * 0.5;
    let slider_track = ScreenRect {
        min: [slider_row.min[0], track_y],
        max: [slider_row.max[0], track_y + track_height],
    };
    y += SLIDER_HANDLE_SIZE + SECTION_GAP;
    let mp_label = unit_rect(
        origin,
        0.0,
        y,
        PROGRESS_TRACK_WIDTH,
        VALUE_LABEL_HEIGHT,
        scale,
    );
    y += VALUE_LABEL_HEIGHT;
    let mp_track = unit_rect(
        origin,
        0.0,
        y,
        PROGRESS_TRACK_WIDTH,
        PROGRESS_TRACK_HEIGHT,
        scale,
    );
    y += PROGRESS_TRACK_HEIGHT + ROW_GAP;
    let exp_label = unit_rect(
        origin,
        0.0,
        y,
        PROGRESS_TRACK_WIDTH,
        VALUE_LABEL_HEIGHT,
        scale,
    );
    y += VALUE_LABEL_HEIGHT;
    let exp_track = unit_rect(
        origin,
        0.0,
        y,
        PROGRESS_TRACK_WIDTH,
        PROGRESS_TRACK_HEIGHT,
        scale,
    );
    y += PROGRESS_TRACK_HEIGHT + SECTION_GAP + HEADING_HEIGHT + LINE_HEIGHT * 3.0;
    ValuesPlaces {
        hp_label,
        hp_track,
        slider_track,
        mp_label,
        mp_track,
        exp_label,
        exp_track,
        bottom: origin[1] + y * scale,
    }
}

struct ValuesPlaces {
    hp_label: ScreenRect,
    hp_track: ScreenRect,
    slider_track: ScreenRect,
    mp_label: ScreenRect,
    mp_track: ScreenRect,
    exp_label: ScreenRect,
    exp_track: ScreenRect,
    bottom: f32,
}

struct InputPlaces {
    field: ScreenRect,
    static_field: ScreenRect,
    bottom: f32,
}

struct ContainerPlaces {
    scroll: UiScrollGeometry,
    section_button: ScreenRect,
    section_rows: [ScreenRect; 3],
    bottom: f32,
}

struct CompositePlaces {
    slots: [ScreenRect; 4],
    hotbar: ScreenRect,
    hotbar_slots: [ScreenRect; HOTBAR_COUNT],
    counter: ScreenRect,
    badge_button: ScreenRect,
    badge: ScreenRect,
    notify_button: ScreenRect,
    clear_button: ScreenRect,
    dialog_button: ScreenRect,
    bottom: f32,
}

fn place_input(origin: [f32; 2], scale: f32) -> InputPlaces {
    let mut y = HEADING_HEIGHT + LINE_HEIGHT;
    let field = unit_rect(origin, 0.0, y, INPUT_WIDTH, INPUT_HEIGHT, scale);
    y += INPUT_HEIGHT + ROW_GAP + LINE_HEIGHT * 2.0 + SECTION_GAP + LINE_HEIGHT;
    let static_field = unit_rect(origin, 0.0, y, INPUT_WIDTH, INPUT_HEIGHT, scale);
    y += INPUT_HEIGHT;
    InputPlaces {
        field,
        static_field,
        bottom: origin[1] + y * scale,
    }
}

fn place_containers(
    origin: [f32; 2],
    scale: f32,
    offset: usize,
) -> Result<ContainerPlaces, String> {
    let mut y = HEADING_HEIGHT;
    let list = unit_rect(
        origin,
        0.0,
        y,
        SCROLL_LIST_WIDTH,
        SCROLL_ROW * SCROLL_VISIBLE as f32,
        scale,
    );
    let scroll = scroll_geometry(
        list,
        SCROLL_GAP * scale,
        SCROLL_ARROW * scale,
        offset,
        SCROLL_ITEMS,
        SCROLL_VISIBLE,
    )?;
    y += SCROLL_ROW * SCROLL_VISIBLE as f32 + ROW_GAP + LINE_HEIGHT + SECTION_GAP + HEADING_HEIGHT;
    let section_button = unit_rect(origin, 0.0, y, SECTION_BUTTON_WIDTH, BUTTON_HEIGHT, scale);
    y += BUTTON_HEIGHT + ROW_GAP + LINE_HEIGHT;
    let mut section_rows = [section_button; 3];
    for row in &mut section_rows {
        *row = unit_rect(origin, 0.0, y, 180.0, LINE_HEIGHT, scale);
        y += LINE_HEIGHT;
    }
    Ok(ContainerPlaces {
        scroll,
        section_button,
        section_rows,
        bottom: origin[1] + y * scale,
    })
}

fn place_composite(origin: [f32; 2], scale: f32) -> CompositePlaces {
    let mut y = HEADING_HEIGHT;
    let mut slots = [unit_rect(origin, 0.0, y, SLOT_SIZE, SLOT_SIZE, scale); 4];
    for (index, slot) in slots.iter_mut().enumerate() {
        let column = (index % 2) as f32;
        let row = (index / 2) as f32;
        *slot = unit_rect(
            origin,
            column * (SLOT_SIZE + SLOT_GAP),
            y + row * (SLOT_SIZE + SLOT_GAP),
            SLOT_SIZE,
            SLOT_SIZE,
            scale,
        );
    }
    y += SLOT_SIZE * 2.0 + SLOT_GAP + SECTION_GAP + HEADING_HEIGHT;
    let inset_l = HOTBAR_BORDER[0];
    let inset_t = HOTBAR_BORDER[1];
    let inset_r = HOTBAR_BORDER[2];
    let inset_b = HOTBAR_BORDER[3];
    let hotbar_width = inset_l
        + HOTBAR_COUNT as f32 * HOTBAR_SLOT
        + (HOTBAR_COUNT - 1) as f32 * HOTBAR_GAP
        + inset_r;
    let hotbar_height = inset_t + HOTBAR_SLOT + inset_b;
    let hotbar = unit_rect(origin, 0.0, y, hotbar_width, hotbar_height, scale);
    let mut hotbar_slots = [hotbar; HOTBAR_COUNT];
    for (index, slot) in hotbar_slots.iter_mut().enumerate() {
        *slot = unit_rect(
            origin,
            inset_l + index as f32 * (HOTBAR_SLOT + HOTBAR_GAP),
            y + inset_t,
            HOTBAR_SLOT,
            HOTBAR_SLOT,
            scale,
        );
    }
    let counter_slot = hotbar_slots[HOTBAR_COUNTER_SLOT];
    let counter = ScreenRect {
        min: [
            counter_slot.max[0] - HOTBAR_COUNTER_SIZE[0] * scale,
            counter_slot.max[1] - HOTBAR_COUNTER_SIZE[1] * scale,
        ],
        max: counter_slot.max,
    };
    y += hotbar_height + SECTION_GAP + HEADING_HEIGHT;
    let badge_button = unit_rect(origin, 0.0, y, ICON_BUTTON_SIZE, ICON_BUTTON_SIZE, scale);
    let badge_size = BADGE_SIZE * scale;
    let badge = ScreenRect {
        min: [
            badge_button.max[0] - badge_size * 0.7,
            badge_button.min[1] - badge_size * 0.25,
        ],
        max: [
            badge_button.max[0] + badge_size * 0.3,
            badge_button.min[1] + badge_size * 0.75,
        ],
    };
    y += ICON_BUTTON_SIZE + ROW_GAP;
    let notify_button = unit_rect(origin, 0.0, y, 150.0, BUTTON_HEIGHT, scale);
    let clear_button = unit_rect(origin, 158.0, y, 72.0, BUTTON_HEIGHT, scale);
    y += BUTTON_HEIGHT + ROW_GAP;
    let dialog_button = unit_rect(origin, 0.0, y, BUTTON_WIDTH, BUTTON_HEIGHT, scale);
    y += BUTTON_HEIGHT;
    CompositePlaces {
        slots,
        hotbar,
        hotbar_slots,
        counter,
        badge_button,
        badge,
        notify_button,
        clear_button,
        dialog_button,
        bottom: origin[1] + y * scale,
    }
}

fn content_rails(inner: ScreenRect, active: ScreenRect, scale: f32) -> [ScreenRect; 2] {
    let left = TABBED_BORDER[0] * scale;
    let right = TABBED_BORDER[2] * scale;
    let height = RAIL_HEIGHT * scale;
    let top = inner.min[1];
    [
        ScreenRect {
            min: [inner.min[0] + left, top],
            max: [active.min[0], top + height],
        },
        ScreenRect {
            min: [active.max[0], top],
            max: [inner.max[0] - right, top + height],
        },
    ]
}

fn compose(
    window: &UiDebugWindow,
    layout: &DebugLayout,
    cursor: Option<[f32; 2]>,
    scale: f32,
) -> Result<UiDebugFrame, String> {
    let mut rects = Vec::new();
    let mut skin_quads = Vec::new();
    let mut texts = Vec::new();
    push_slice(
        &mut skin_quads,
        layout.window,
        &window.assets.panel,
        PANEL_BORDER,
        scale,
    )?;
    push_slice(
        &mut skin_quads,
        layout.header,
        &window.assets.header,
        HEADER_BORDER,
        scale,
    )?;
    let mut background = Vec::new();
    let mut active = None;
    let mut pressed = None;
    for (index, tab) in layout.tabs.iter().copied().enumerate() {
        let hovered = cursor.is_some_and(|point| tab.contains(point));
        let armed = hovered && window.tabs.pressed_index() == Some(index);
        let visual = tab_visual(window.tabs.selected_index() == index, hovered, armed);
        let quad = compose_stretched_quad(tab, window.assets.tabs.texture(visual))?;
        match visual {
            UiTabVisual::Pressed => pressed = Some(quad),
            UiTabVisual::Active => active = Some(quad),
            UiTabVisual::Inactive | UiTabVisual::Hover => background.push(quad),
        }
        texts.push(centered_label(PAGES[index], tab, TAB_FONT, INK, scale));
    }
    skin_quads.extend(background);
    push_slice(
        &mut skin_quads,
        layout.inner,
        &window.assets.tabbed_body,
        TABBED_BORDER,
        scale,
    )?;
    let selected = layout.tabs[window.tabs.selected_index()];
    for rail in content_rails(layout.inner, selected, scale) {
        skin_quads.push(compose_stretched_quad(rail, window.assets.tabbed_rail)?);
    }
    if let Some(quad) = active {
        skin_quads.push(quad);
    }
    if let Some(quad) = pressed {
        skin_quads.push(quad);
    }
    if window.tabs.selected_index() == BASICS {
        push_basics(&mut skin_quads, &mut texts, window, layout, cursor, scale)?;
    } else if window.tabs.selected_index() == VALUES {
        push_values(&mut skin_quads, &mut texts, window, layout, cursor, scale)?;
    } else if window.tabs.selected_index() == INPUT_PAGE {
        push_input(
            &mut rects,
            &mut skin_quads,
            &mut texts,
            window,
            layout,
            scale,
        )?;
    } else if window.tabs.selected_index() == CONTAINERS_PAGE {
        push_containers(&mut skin_quads, &mut texts, window, layout, cursor, scale)?;
    } else if window.tabs.selected_index() == COMPOSITE_PAGE {
        push_composite(&mut skin_quads, &mut texts, window, layout, cursor, scale)?;
    }
    skin_quads.push(compose_stretched_quad(layout.gear, window.assets.gear)?);
    let close_visual = window
        .chrome
        .close_button_visual(cursor, layout.close_button);
    let close_texture = match close_visual {
        CloseButtonVisual::Normal => window.assets.close[0],
        CloseButtonVisual::Hover => window.assets.close[1],
        CloseButtonVisual::Pressed => window.assets.close[2],
    };
    skin_quads.push(compose_stretched_quad(layout.close_button, close_texture)?);
    texts.push(title_text(layout, scale));
    Ok(UiDebugFrame {
        rects,
        skin_quads,
        texts,
    })
}

fn push_basics(
    quads: &mut Vec<UiTexturedQuad>,
    texts: &mut Vec<TextBlock>,
    window: &UiDebugWindow,
    layout: &DebugLayout,
    cursor: Option<[f32; 2]>,
    scale: f32,
) -> Result<(), String> {
    let origin = content_origin(layout.inner, scale);
    texts.push(heading("BUTTONS", [layout.click_button.min[0], origin[1]]));
    push_text_button(
        quads,
        layout.click_button,
        window.assets.buttons.asset(
            window
                .click_button
                .visual(cursor, layout.click_button, true),
        ),
        scale,
    )?;
    texts.push(button_label_text(
        "Click Me",
        layout.click_button,
        BODY_FONT,
        INK,
        scale,
    ));
    texts.push(side_label(
        &format!("Clicks: {}", window.state.click_count),
        layout.click_button,
        scale,
    ));
    push_button(
        quads,
        layout.icon_button,
        window
            .assets
            .icon_buttons
            .texture(window.icon_button.visual(cursor, layout.icon_button, true)),
    )?;
    push_button(quads, layout.icon_glyph, window.assets.gear)?;
    texts.push(side_label(
        &format!("Icon: {}", window.state.icon_click_count),
        layout.icon_button,
        scale,
    ));
    push_text_button(
        quads,
        layout.disabled_button,
        window.assets.buttons.asset(window.disabled_button.visual(
            cursor,
            layout.disabled_button,
            false,
        )),
        scale,
    )?;
    texts.push(button_label_text(
        "Disabled",
        layout.disabled_button,
        BODY_FONT,
        MUTED,
        scale,
    ));
    texts.push(heading(
        "CHECKBOX",
        [
            layout.checkbox_row.min[0],
            layout.checkbox_row.min[1] - HEADING_HEIGHT * scale,
        ],
    ));
    push_button(
        quads,
        layout.checkbox_mark,
        window.assets.checkboxes.texture(window.checkbox.visual(
            cursor,
            layout.checkbox_row,
            window.state.checkbox,
            true,
        )),
    )?;
    texts.push(row_label(
        "Enable Example",
        layout.checkbox_mark,
        layout.checkbox_row,
        INK,
        scale,
    ));
    push_button(
        quads,
        layout.disabled_checkbox_mark,
        window
            .assets
            .checkboxes
            .texture(window.disabled_checkbox.visual(
                cursor,
                layout.disabled_checkbox_row,
                false,
                false,
            )),
    )?;
    texts.push(row_label(
        "Disabled",
        layout.disabled_checkbox_mark,
        layout.disabled_checkbox_row,
        MUTED,
        scale,
    ));
    texts.push(heading(
        "RADIO",
        [
            layout.radio_rows[0].min[0],
            layout.radio_rows[0].min[1] - HEADING_HEIGHT * scale,
        ],
    ));
    for (index, row) in layout.radio_rows.iter().copied().enumerate() {
        let enabled = radio_enabled(index);
        let selected = enabled && window.state.radio.index() == index;
        let visual = window.radios.visual(index, selected, cursor, row, enabled);
        push_button(
            quads,
            layout.radio_marks[index],
            window.assets.radios.texture(visual),
        )?;
        texts.push(row_label(
            RADIO_LABELS[index],
            layout.radio_marks[index],
            row,
            if enabled { INK } else { MUTED },
            scale,
        ));
    }
    texts.push(heading(
        "STATE",
        [origin[0] + STATE_COLUMN_X * scale, origin[1]],
    ));
    let lines = [
        format!("Page: {}", PAGES[window.tabs.selected_index()]),
        format!("Clicks: {}", window.state.click_count),
        format!("Icon: {}", window.state.icon_click_count),
        format!("Checkbox: {}", window.state.checkbox),
        format!("Radio: {}", window.state.radio.label()),
    ];
    for (index, line) in lines.iter().enumerate() {
        texts.push(body_text(
            line,
            [
                origin[0] + STATE_COLUMN_X * scale,
                origin[1] + HEADING_HEIGHT * scale + index as f32 * LINE_HEIGHT * scale,
            ],
            (layout.inner.max[0] - (origin[0] + STATE_COLUMN_X * scale) - PAGE_PAD * scale)
                .max(1.0),
            INK,
        ));
    }
    Ok(())
}

fn push_values(
    quads: &mut Vec<UiTexturedQuad>,
    texts: &mut Vec<TextBlock>,
    window: &UiDebugWindow,
    layout: &DebugLayout,
    cursor: Option<[f32; 2]>,
    scale: f32,
) -> Result<(), String> {
    let origin = content_origin(layout.inner, scale);
    texts.push(heading("VALUES", [layout.hp_label.min[0], origin[1]]));
    push_meter(
        quads,
        texts,
        Meter {
            name: "HP",
            value: window.state.hp_value,
            label: layout.hp_label,
            track: layout.hp_track,
            fill: &window.assets.status_hp,
        },
        &window.assets.status_track,
        scale,
    )?;
    let geometry = slider_geometry(
        layout.hp_slider_track,
        SLIDER_HANDLE_SIZE * scale,
        window.state.hp_value,
    )?;
    let slice = window.assets.slider_track.slice_ltrb;
    push_slice(
        quads,
        geometry.track,
        &window.assets.slider_track,
        [
            slice[0] as f32,
            slice[1] as f32,
            slice[2] as f32,
            slice[3] as f32,
        ],
        scale,
    )?;
    quads.push(compose_stretched_quad(
        geometry.handle,
        window
            .assets
            .slider_handles
            .texture(window.hp_slider.visual(cursor, geometry.handle)),
    )?);
    push_meter(
        quads,
        texts,
        Meter {
            name: "MP",
            value: MP_SAMPLE,
            label: layout.mp_label,
            track: layout.mp_track,
            fill: &window.assets.status_mp,
        },
        &window.assets.status_track,
        scale,
    )?;
    push_meter(
        quads,
        texts,
        Meter {
            name: "EXP",
            value: EXP_SAMPLE,
            label: layout.exp_label,
            track: layout.exp_track,
            fill: &window.assets.status_exp,
        },
        &window.assets.status_track,
        scale,
    )?;
    let state_y = layout.exp_track.max[1] + SECTION_GAP * scale;
    texts.push(heading("STATE", [layout.exp_track.min[0], state_y]));
    let dragging = if window.hp_slider.is_dragging() {
        "Yes"
    } else {
        "No"
    };
    let lines = [
        format!("HP: {}", percent_text(window.state.hp_value)),
        format!("Value: {:.3}", window.state.hp_value),
        format!("Dragging: {dragging}"),
    ];
    let width = (layout.inner.max[0] - layout.exp_track.min[0] - PAGE_PAD * scale).max(1.0);
    for (index, line) in lines.iter().enumerate() {
        texts.push(body_text(
            line,
            [
                layout.exp_track.min[0],
                state_y + (HEADING_HEIGHT + index as f32 * LINE_HEIGHT) * scale,
            ],
            width,
            INK,
        ));
    }
    Ok(())
}

fn push_meter(
    quads: &mut Vec<UiTexturedQuad>,
    texts: &mut Vec<TextBlock>,
    meter: Meter<'_>,
    track_asset: &UiV2NineSlice,
    scale: f32,
) -> Result<(), String> {
    texts.push(meter_label(meter.name, meter.label, false, scale));
    texts.push(meter_label(
        &percent_text(meter.value),
        meter.label,
        true,
        scale,
    ));
    quads.extend(compose_progress_bar(
        meter.track,
        PROGRESS_FILL_INSET.map(|edge| edge * scale),
        meter.value,
        track_asset,
        meter.fill,
        scale,
    )?);
    Ok(())
}

struct Meter<'a> {
    name: &'a str,
    value: f32,
    label: ScreenRect,
    track: ScreenRect,
    fill: &'a UiV2NineSlice,
}

fn percent_text(value: f32) -> String {
    format!("{}%", (value.clamp(0.0, 1.0) * 100.0).round() as i32)
}

fn meter_label(label: &str, row: ScreenRect, align_right: bool, scale: f32) -> TextBlock {
    let font_px = BODY_FONT * scale;
    let y = row.min[1] + ((row.height() - font_px) * 0.5).max(0.0);
    if align_right {
        TextBlock {
            content: TextContent(label.to_owned()),
            style: TextStyle::at_size(BODY_FONT, INK, TextAlignment::Right),
            anchor: [row.max[0], y],
            max_width: Some(row.width().max(1.0)),
            clip: None,
        }
    } else {
        body_text(label, [row.min[0], y], row.width().max(1.0), INK)
    }
}

fn push_slice(
    quads: &mut Vec<UiTexturedQuad>,
    bounds: ScreenRect,
    asset: &UiV2NineSlice,
    border: [f32; 4],
    scale: f32,
) -> Result<(), String> {
    quads.extend(compose_nine_slice_with_borders(
        bounds.min,
        [bounds.width() / scale, bounds.height() / scale],
        scale,
        asset.texture,
        asset.size_px,
        asset.slice_ltrb,
        border,
    )?);
    Ok(())
}

fn push_button(
    quads: &mut Vec<UiTexturedQuad>,
    bounds: ScreenRect,
    texture: SpriteTextureId,
) -> Result<(), String> {
    quads.push(compose_stretched_quad(bounds, texture)?);
    Ok(())
}

fn push_text_button(
    quads: &mut Vec<UiTexturedQuad>,
    bounds: ScreenRect,
    asset: &UiV2NineSlice,
    scale: f32,
) -> Result<(), String> {
    quads.extend(compose_v2_text_button(bounds, asset, scale)?);
    Ok(())
}

fn heading(label: &str, anchor: [f32; 2]) -> TextBlock {
    TextBlock {
        content: TextContent(label.to_owned()),
        style: TextStyle::at_size(SECTION_FONT, INK, TextAlignment::Left),
        anchor,
        max_width: Some(180.0),
        clip: None,
    }
}

fn body_text(label: &str, anchor: [f32; 2], max_width: f32, color: [f32; 4]) -> TextBlock {
    TextBlock {
        content: TextContent(label.to_owned()),
        style: TextStyle::at_size(BODY_FONT, color, TextAlignment::Left),
        anchor,
        max_width: Some(max_width.max(1.0)),
        clip: None,
    }
}

fn push_input(
    rects: &mut Vec<UiRect>,
    quads: &mut Vec<UiTexturedQuad>,
    texts: &mut Vec<TextBlock>,
    window: &UiDebugWindow,
    layout: &DebugLayout,
    scale: f32,
) -> Result<(), String> {
    let origin = content_origin(layout.inner, scale);
    texts.push(heading(
        "TEXT INPUT",
        [layout.input.field.min[0], origin[1]],
    ));
    texts.push(body_text(
        "Name:",
        [
            layout.input.field.min[0],
            layout.input.field.min[1] - LINE_HEIGHT * scale,
        ],
        80.0 * scale,
        INK,
    ));
    push_slice(
        quads,
        layout.input.field,
        &window.assets.chat_input,
        CHAT_BORDER,
        scale,
    )?;
    let interior = field_interior(layout.input.field, scale);
    rects.push(UiRect {
        min: interior.min,
        max: interior.max,
        color: INPUT_FILL,
    });
    push_field_editor(
        rects,
        texts,
        &window.text_input,
        &window.state.input_text,
        layout.input.field,
        scale,
    );
    let count = window.state.input_text.chars().count();
    texts.push(body_text(
        &format!("Characters: {count}"),
        [
            layout.input.field.min[0],
            layout.input.field.max[1] + ROW_GAP * scale,
        ],
        layout.input.field.width(),
        INK,
    ));
    let focused = if window.text_input.is_focused() {
        "Yes"
    } else {
        "No"
    };
    texts.push(body_text(
        &format!("Focused: {focused}"),
        [
            layout.input.field.min[0],
            layout.input.field.max[1] + (ROW_GAP + LINE_HEIGHT) * scale,
        ],
        layout.input.field.width(),
        INK,
    ));
    texts.push(body_text(
        "Static",
        [
            layout.input.static_field.min[0],
            layout.input.static_field.min[1] - LINE_HEIGHT * scale,
        ],
        80.0 * scale,
        MUTED,
    ));
    push_slice(
        quads,
        layout.input.static_field,
        &window.assets.chat_input,
        CHAT_BORDER,
        scale,
    )?;
    texts.push(field_text(
        "Sample",
        layout.input.static_field,
        TITLE_COLOR,
        scale,
    ));
    Ok(())
}

fn push_containers(
    quads: &mut Vec<UiTexturedQuad>,
    texts: &mut Vec<TextBlock>,
    window: &UiDebugWindow,
    layout: &DebugLayout,
    cursor: Option<[f32; 2]>,
    scale: f32,
) -> Result<(), String> {
    let origin = content_origin(layout.inner, scale);
    let scroll = layout.containers.scroll;
    texts.push(heading("SCROLL VIEW", [scroll.list.min[0], origin[1]]));
    let max_offset = scroll_max_offset(SCROLL_ITEMS, SCROLL_VISIBLE);
    for index in visible_row_indices(window.state.scroll_offset, SCROLL_ITEMS, SCROLL_VISIBLE) {
        let row = index - window.state.scroll_offset.min(SCROLL_ITEMS);
        let y = scroll.list.min[1] + row as f32 * SCROLL_ROW * scale;
        texts.push(body_text(
            &format!("Item {:02}", index + 1),
            [scroll.list.min[0] + 6.0 * scale, y + 2.0 * scale],
            scroll.list.width() - 12.0 * scale,
            INK,
        ));
    }
    push_slice(
        quads,
        scroll.track,
        &window.assets.scroll_track,
        SCROLL_TRACK_BORDER,
        scale,
    )?;
    let thumb = if max_offset == 0 {
        window.assets.scroll_thumbs[3]
    } else if window.scrollbar.is_dragging() {
        window.assets.scroll_thumbs[2]
    } else if inside(scroll.thumb, cursor) {
        window.assets.scroll_thumbs[1]
    } else {
        window.assets.scroll_thumbs[0]
    };
    push_button(quads, scroll.thumb, thumb)?;
    push_button(
        quads,
        scroll.up,
        arrow_texture(
            window.assets.scroll_up,
            window.scroll_up.visual(cursor, scroll.up, true),
        ),
    )?;
    push_button(
        quads,
        scroll.down,
        arrow_texture(
            window.assets.scroll_down,
            window.scroll_down.visual(cursor, scroll.down, true),
        ),
    )?;
    texts.push(body_text(
        &format!("Offset: {} / {max_offset}", window.state.scroll_offset),
        [scroll.list.min[0], scroll.list.max[1] + ROW_GAP * scale],
        scroll.list.width(),
        INK,
    ));
    texts.push(heading(
        "SECTION",
        [
            layout.containers.section_button.min[0],
            layout.containers.section_button.min[1] - HEADING_HEIGHT * scale,
        ],
    ));
    let section_label = if window.state.section_open {
        "Close Advanced Section"
    } else {
        "Open Advanced Section"
    };
    push_text_button(
        quads,
        layout.containers.section_button,
        window.assets.buttons.asset(window.section_button.visual(
            cursor,
            layout.containers.section_button,
            true,
        )),
        scale,
    )?;
    texts.push(button_label_text(
        section_label,
        layout.containers.section_button,
        BODY_FONT,
        INK,
        scale,
    ));
    if window.state.section_open {
        texts.push(body_text(
            "Advanced content:",
            [
                layout.containers.section_rows[0].min[0],
                layout.containers.section_button.max[1] + ROW_GAP * scale,
            ],
            180.0 * scale,
            INK,
        ));
        for (label, row) in SECTION_ROWS.iter().zip(layout.containers.section_rows) {
            texts.push(body_text(label, [row.min[0], row.min[1]], row.width(), INK));
        }
    }
    Ok(())
}

fn push_composite(
    quads: &mut Vec<UiTexturedQuad>,
    texts: &mut Vec<TextBlock>,
    window: &UiDebugWindow,
    layout: &DebugLayout,
    cursor: Option<[f32; 2]>,
    scale: f32,
) -> Result<(), String> {
    let origin = content_origin(layout.inner, scale);
    texts.push(heading(
        "SLOTS",
        [layout.composite.slots[0].min[0], origin[1]],
    ));
    for (index, bounds) in layout.composite.slots.iter().copied().enumerate() {
        let hovered = inside(bounds, cursor);
        let visual = slot_visual(
            slot_enabled(index),
            window.state.selected_slot == index,
            hovered,
        );
        push_button(quads, bounds, slot_texture(window.assets.slots, visual))?;
    }
    texts.push(heading(
        "HOTBAR",
        [
            layout.composite.hotbar.min[0],
            layout.composite.hotbar.min[1] - HEADING_HEIGHT * scale,
        ],
    ));
    push_slice(
        quads,
        layout.composite.hotbar,
        &window.assets.hotbar_body,
        HOTBAR_BORDER,
        scale,
    )?;
    for (index, bounds) in layout.composite.hotbar_slots.iter().copied().enumerate() {
        let visual = slot_visual(
            hotbar_enabled(index),
            window.state.selected_hotbar_slot == index,
            inside(bounds, cursor),
        );
        push_button(
            quads,
            bounds,
            slot_texture(window.assets.hotbar_slots, visual),
        )?;
    }
    push_slice(
        quads,
        layout.composite.counter,
        &window.assets.hotbar_counter,
        COUNTER_BORDER,
        scale,
    )?;
    texts.push(centered_label(
        &HOTBAR_COUNTER_VALUE.to_string(),
        layout.composite.counter,
        8.0,
        INK,
        scale,
    ));
    texts.push(heading(
        "BADGE",
        [
            layout.composite.badge_button.min[0],
            layout.composite.badge_button.min[1] - HEADING_HEIGHT * scale,
        ],
    ));
    push_button(
        quads,
        layout.composite.badge_button,
        window.assets.icon_buttons.texture(UiButtonVisual::Normal),
    )?;
    let pulse = notification_badge_scale(notification_pulse_elapsed(
        window.state.notification_pulse_at,
    ));
    let badge = scale_about_center(layout.composite.badge, pulse);
    push_button(quads, badge, window.assets.notification_badge)?;
    texts.push(centered_label(
        &notification_badge_label(window.state.notification_count),
        badge,
        8.0 * pulse,
        TITLE_COLOR,
        scale,
    ));
    push_labeled_button(
        quads,
        texts,
        layout.composite.notify_button,
        "+1 Notification",
        window
            .notify_button
            .visual(cursor, layout.composite.notify_button, true),
        window,
        scale,
    )?;
    push_labeled_button(
        quads,
        texts,
        layout.composite.clear_button,
        "Clear",
        window
            .clear_button
            .visual(cursor, layout.composite.clear_button, true),
        window,
        scale,
    )?;
    push_labeled_button(
        quads,
        texts,
        layout.composite.dialog_button,
        "Open Dialog",
        window
            .dialog_button
            .visual(cursor, layout.composite.dialog_button, true),
        window,
        scale,
    )?;
    if inside(layout.composite.slots[1], cursor) {
        let placed = place_tooltip(
            &sample_tooltip(),
            layout.composite.slots[1],
            layout.window,
            scale,
        )?;
        push_slice(
            quads,
            placed.body,
            &window.assets.tooltip_body,
            TOOLTIP_BORDER,
            scale,
        )?;
        push_button(quads, placed.pointer, window.assets.tooltip_pointer)?;
        quads.extend(compose_v2_text_button(
            placed.divider,
            &window.assets.tooltip_divider,
            scale,
        )?);
        texts.extend(placed.lines);
    }
    Ok(())
}

fn push_labeled_button(
    quads: &mut Vec<UiTexturedQuad>,
    texts: &mut Vec<TextBlock>,
    bounds: ScreenRect,
    label: &str,
    visual: UiButtonVisual,
    window: &UiDebugWindow,
    scale: f32,
) -> Result<(), String> {
    push_text_button(quads, bounds, window.assets.buttons.asset(visual), scale)?;
    texts.push(button_label_text(label, bounds, BODY_FONT, INK, scale));
    Ok(())
}

fn field_interior(bounds: ScreenRect, scale: f32) -> ScreenRect {
    ScreenRect {
        min: [
            bounds.min[0] + CHAT_BORDER[0] * scale,
            bounds.min[1] + CHAT_BORDER[1] * scale,
        ],
        max: [
            bounds.max[0] - CHAT_BORDER[2] * scale,
            bounds.max[1] - CHAT_BORDER[3] * scale,
        ],
    }
}

fn field_text(value: &str, bounds: ScreenRect, color: [f32; 4], scale: f32) -> TextBlock {
    let interior = field_interior(bounds, scale);
    let pad = INPUT_TEXT_PAD * scale;
    let font_px = BODY_FONT * scale;
    let mut block = body_text(
        value,
        [
            interior.min[0] + pad,
            interior.min[1] + ((interior.height() - font_px) * 0.5).max(0.0),
        ],
        (interior.width() - pad * 2.0).max(1.0),
        color,
    );
    block.clip = Some([
        interior.min[0] + pad,
        interior.min[1],
        interior.max[0] - pad,
        interior.max[1],
    ]);
    block
}

fn input_metrics(scale: f32) -> UiTextMetrics {
    let interior_width = (INPUT_WIDTH - CHAT_BORDER[0] - CHAT_BORDER[2]) * scale;
    let pad = INPUT_TEXT_PAD * scale;
    UiTextMetrics {
        style: TextStyle::at_size(BODY_FONT, INK, TextAlignment::Left),
        scale,
        view_width: (interior_width - pad * 2.0).max(1.0),
    }
}

fn input_local_x(cursor_x: f32, field: ScreenRect, scale: f32, scroll: f32) -> f32 {
    let interior = field_interior(field, scale);
    let pad = INPUT_TEXT_PAD * scale;
    cursor_x - (interior.min[0] + pad) + scroll
}

fn push_field_editor(
    rects: &mut Vec<UiRect>,
    texts: &mut Vec<TextBlock>,
    input: &UiTextInput,
    value: &str,
    field: ScreenRect,
    scale: f32,
) {
    let metrics = input_metrics(scale);
    let interior = field_interior(field, scale);
    let pad = INPUT_TEXT_PAD * scale;
    let origin_x = interior.min[0] + pad;
    let clip_left = origin_x;
    let clip_right = origin_x + metrics.view_width;
    let font_px = BODY_FONT * scale;
    let text_y = interior.min[1] + ((interior.height() - font_px) * 0.5).max(0.0);
    let line_height = BODY_FONT * 1.2 * scale;
    let stops = measure_field_stops(value, metrics);
    let scroll = input.scroll();
    if input.is_focused()
        && let Some((start, end)) = input.selection()
        && let (Some(x0), Some(x1)) = (stops.x.get(start), stops.x.get(end))
        && let Some((left, right)) = clipped_span(
            origin_x + x0 - scroll,
            origin_x + x1 - scroll,
            clip_left,
            clip_right,
        )
    {
        rects.push(UiRect {
            min: [left, text_y],
            max: [right, (text_y + line_height).min(interior.max[1])],
            color: SELECTION_FILL,
        });
    }
    if input.is_focused() {
        let caret = input.caret().min(stops.x.len().saturating_sub(1));
        let caret_x = origin_x + stops.x.get(caret).copied().unwrap_or(0.0) - scroll;
        if let Some((left, right)) = clipped_span(caret_x, caret_x + 1.0, clip_left, clip_right) {
            rects.push(UiRect {
                min: [left, text_y],
                max: [right, (text_y + line_height).min(interior.max[1])],
                color: INK,
            });
        }
    }
    let mut block = body_text(
        value,
        [origin_x - scroll, text_y],
        metrics.view_width.max(1.0),
        INK,
    );
    block.max_width = None;
    block.clip = Some([clip_left, interior.min[1], clip_right, interior.max[1]]);
    texts.push(block);
}

fn measure_field_stops(value: &str, metrics: UiTextMetrics) -> TextCaretBoundaries {
    let block = TextBlock {
        content: TextContent(value.to_owned()),
        style: metrics.style,
        anchor: [0.0, 0.0],
        max_width: None,
        clip: None,
    };
    measure_caret_boundaries(&block, metrics.scale).unwrap_or(TextCaretBoundaries { x: vec![0.0] })
}

fn clipped_span(start: f32, end: f32, left: f32, right: f32) -> Option<(f32, f32)> {
    let min = start.min(end).max(left);
    let max = start.max(end).min(right);
    (max - min >= 0.4).then_some((min, max))
}

fn notification_pulse_elapsed(started: Option<Instant>) -> u32 {
    let Some(started) = started else {
        return NOTIFICATION_PULSE_MS;
    };
    u32::try_from(started.elapsed().as_millis()).unwrap_or(NOTIFICATION_PULSE_MS)
}

fn scale_about_center(bounds: ScreenRect, factor: f32) -> ScreenRect {
    let center = [
        (bounds.min[0] + bounds.max[0]) * 0.5,
        (bounds.min[1] + bounds.max[1]) * 0.5,
    ];
    let width = bounds.width() * factor;
    let height = bounds.height() * factor;
    ScreenRect {
        min: [center[0] - width * 0.5, center[1] - height * 0.5],
        max: [center[0] + width * 0.5, center[1] + height * 0.5],
    }
}

fn sample_tooltip() -> TooltipContent {
    TooltipContent {
        title: "Training Sword".to_owned(),
        title_color: TOOLTIP_ITEM_TITLE,
        description: "Example UI DEBUG tooltip that wraps once the copy is wider than the tooltip."
            .to_owned(),
    }
}

fn arrow_texture(textures: [SpriteTextureId; 3], visual: UiButtonVisual) -> SpriteTextureId {
    match visual {
        UiButtonVisual::Hover => textures[1],
        UiButtonVisual::Pressed => textures[2],
        UiButtonVisual::Normal | UiButtonVisual::Disabled => textures[0],
    }
}

fn slot_texture(textures: [SpriteTextureId; 4], visual: UiSlotVisual) -> SpriteTextureId {
    match visual {
        UiSlotVisual::Normal => textures[0],
        UiSlotVisual::Hover => textures[1],
        UiSlotVisual::Selected => textures[2],
        UiSlotVisual::Disabled => textures[3],
    }
}

struct PlacedTooltip {
    body: ScreenRect,
    pointer: ScreenRect,
    divider: ScreenRect,
    lines: Vec<TextBlock>,
}

/// Ordered tooltip pieces. Later categories append another piece here; they are not fields yet.
enum TooltipPiece {
    Title,
    Divider,
    Description,
}

fn tooltip_pieces() -> [TooltipPiece; 3] {
    [
        TooltipPiece::Title,
        TooltipPiece::Divider,
        TooltipPiece::Description,
    ]
}

fn place_tooltip(
    content: &TooltipContent,
    target: ScreenRect,
    window: ScreenRect,
    scale: f32,
) -> Result<PlacedTooltip, String> {
    let border = [
        TOOLTIP_BORDER[0] * scale,
        TOOLTIP_BORDER[1] * scale,
        TOOLTIP_BORDER[2] * scale,
        TOOLTIP_BORDER[3] * scale,
    ];
    let pad_x = TOOLTIP_INNER_PAD_X * scale;
    let pad_y = TOOLTIP_INNER_PAD_Y * scale;
    let gap = TOOLTIP_LINE_GAP * scale;
    let content_limit = (TOOLTIP_MAX_WIDTH * scale - border[0] - border[2] - pad_x * 2.0).max(1.0);
    let title = measure_tooltip_text(&content.title, content.title_color, content_limit, scale)?;
    let description =
        measure_tooltip_text(&content.description, TOOLTIP_BODY, content_limit, scale)?;
    let content_width = title
        .1
        .width
        .max(description.1.width)
        .max(24.0 * scale)
        .min(content_limit);
    let divider_height = TOOLTIP_DIVIDER_HEIGHT * scale;
    let mut content_height = 0.0;
    for piece in tooltip_pieces() {
        content_height += match piece {
            TooltipPiece::Title => title.1.height,
            TooltipPiece::Divider => divider_height,
            TooltipPiece::Description => description.1.height,
        };
        content_height += gap;
    }
    content_height = (content_height - gap).max(divider_height);
    let width = (content_width + border[0] + border[2] + pad_x * 2.0).clamp(
        border[0] + border[2] + 2.0 * scale,
        TOOLTIP_MAX_WIDTH * scale,
    );
    let height = (content_height + border[1] + border[3] + pad_y * 2.0)
        .max(border[1] + border[3] + 2.0 * scale);
    let margin = 4.0 * scale;
    let pointer = [TOOLTIP_POINTER[0] * scale, TOOLTIP_POINTER[1] * scale];
    let limit_min = [window.min[0] + margin, window.min[1] + margin];
    let limit_max = [window.max[0] - margin, window.max[1] - margin];
    let mut x = target.max[0] + pointer[0];
    let mut y = target.min[1];
    if x + width > limit_max[0] {
        x = target.min[0] - pointer[0] - width;
    }
    x = x.clamp(limit_min[0], (limit_max[0] - width).max(limit_min[0]));
    y = y.clamp(limit_min[1], (limit_max[1] - height).max(limit_min[1]));
    let body = ScreenRect {
        min: [x, y],
        max: [x + width, y + height],
    };
    let pointing_left = body.min[0] >= target.max[0] - 0.5;
    let pointer_x = if pointing_left {
        body.min[0] - pointer[0]
    } else {
        body.max[0]
    };
    let pointer_y = (target.min[1] + target.height() * 0.5 - pointer[1] * 0.5)
        .clamp(body.min[1], (body.max[1] - pointer[1]).max(body.min[1]));
    let pointer_rect = ScreenRect {
        min: [pointer_x, pointer_y],
        max: [pointer_x + pointer[0], pointer_y + pointer[1]],
    };
    let mut cursor_y = body.min[1] + border[1] + pad_y;
    let text_x = body.min[0] + border[0] + pad_x;
    let mut lines = Vec::new();
    let mut divider = ScreenRect {
        min: [text_x, cursor_y],
        max: [text_x + content_width, cursor_y + divider_height],
    };
    for piece in tooltip_pieces() {
        match piece {
            TooltipPiece::Title => {
                let mut block = title.0.clone();
                block.anchor = [text_x, cursor_y];
                block.max_width = Some(content_width);
                block.style.color = content.title_color;
                lines.push(block);
                cursor_y += title.1.height + gap;
            }
            TooltipPiece::Divider => {
                divider = ScreenRect {
                    min: [text_x, cursor_y],
                    max: [text_x + content_width, cursor_y + divider_height],
                };
                cursor_y += divider_height + gap;
            }
            TooltipPiece::Description => {
                let mut block = description.0.clone();
                block.anchor = [text_x, cursor_y];
                block.max_width = Some(content_width);
                lines.push(block);
            }
        }
    }
    Ok(PlacedTooltip {
        body,
        pointer: pointer_rect,
        divider,
        lines,
    })
}

fn measure_tooltip_text(
    text: &str,
    color: [f32; 4],
    max_width: f32,
    scale: f32,
) -> Result<(TextBlock, crate::renderer::text::TextMetrics), String> {
    let block = body_text(text, [0.0, 0.0], max_width, color);
    let metrics = measure_text(&block, scale)
        .ok_or_else(|| format!("UI DEBUG tooltip cannot measure {text:?}"))?;
    Ok((block, metrics))
}

fn centered_label(
    label: &str,
    bounds: ScreenRect,
    font: f32,
    color: [f32; 4],
    scale: f32,
) -> TextBlock {
    let font_px = font * scale;
    TextBlock {
        content: TextContent(label.to_owned()),
        style: TextStyle::at_size(font, color, TextAlignment::Center),
        anchor: [
            (bounds.min[0] + bounds.max[0]) * 0.5,
            bounds.min[1] + ((bounds.height() - font_px) * 0.5).max(0.0),
        ],
        max_width: Some((bounds.width() - 8.0 * scale).max(1.0)),
        clip: None,
    }
}

fn side_label(label: &str, bounds: ScreenRect, scale: f32) -> TextBlock {
    let font_px = BODY_FONT * scale;
    body_text(
        label,
        [
            bounds.max[0] + TEXT_GAP * scale,
            bounds.min[1] + ((bounds.height() - font_px) * 0.5).max(0.0),
        ],
        80.0 * scale,
        INK,
    )
}

fn row_label(
    label: &str,
    mark: ScreenRect,
    row: ScreenRect,
    color: [f32; 4],
    scale: f32,
) -> TextBlock {
    let font_px = BODY_FONT * scale;
    body_text(
        label,
        [
            mark.max[0] + TEXT_GAP * scale,
            row.min[1] + ((row.height() - font_px) * 0.5).max(0.0),
        ],
        (row.max[0] - mark.max[0] - TEXT_GAP * scale).max(1.0),
        color,
    )
}

fn title_text(layout: &DebugLayout, scale: f32) -> TextBlock {
    let font_px = TITLE_FONT * scale;
    let anchor = [
        layout.gear.max[0] + TEXT_GAP * scale,
        layout.header.min[1] + ((layout.header.height() - font_px) * 0.5).max(0.0),
    ];
    TextBlock {
        content: TextContent("UI DEBUG".to_owned()),
        style: TextStyle::at_size(TITLE_FONT, TITLE_COLOR, TextAlignment::Left),
        anchor,
        max_width: Some((layout.close_button.min[0] - TEXT_GAP * scale - anchor[0]).max(1.0)),
        clip: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_controls::{
        UiButtonVisual, UiSliderHandleVisual, progress_geometry, slider_value_from_x,
    };

    fn viewport() -> PixelViewport {
        PixelViewport {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        }
    }

    fn image(id: u32) -> UiV2Image {
        UiV2Image {
            texture: SpriteTextureId::from_raw(id),
            size_px: [8, 8],
        }
    }

    fn family(start: u32) -> [UiV2Image; 4] {
        [
            image(start),
            image(start + 1),
            image(start + 2),
            image(start + 3),
        ]
    }

    fn synthetic() -> UiDebugWindow {
        let slice = |id: u32, size: [u32; 2], slice_ltrb: [u32; 4]| UiV2NineSlice {
            texture: SpriteTextureId::from_raw(id),
            size_px: size,
            slice_ltrb,
        };
        UiDebugWindow::with_assets(UiDebugAssets {
            panel: slice(1, [288, 192], [14, 14, 14, 18]),
            header: slice(2, [270, 40], [8, 8, 8, 8]),
            tabbed_body: slice(3, [288, 192], [8, 8, 8, 8]),
            tabbed_rail: image(4).texture,
            close: [image(5).texture, image(6).texture, image(7).texture],
            tabs: UiTabSkin::from_family(family(10)),
            buttons: TextButtons {
                states: [
                    slice(20, [112, 44], [10, 0, 10, 0]),
                    slice(21, [112, 44], [10, 0, 10, 0]),
                    slice(22, [112, 44], [10, 0, 10, 0]),
                    slice(23, [112, 44], [10, 0, 10, 0]),
                ],
            },
            icon_buttons: UiButtonSkin::from_family(family(30)),
            checkboxes: UiCheckboxSkin::from_family(family(40)),
            radios: UiRadioSkin::from_family(family(50)),
            slider_track: slice(70, [240, 12], [6, 0, 6, 0]),
            slider_handles: UiSliderSkin::from_textures([
                image(71).texture,
                image(72).texture,
                image(73).texture,
            ]),
            status_track: slice(80, [288, 28], [10, 10, 10, 10]),
            status_hp: slice(81, [270, 18], [6, 6, 6, 6]),
            status_mp: slice(82, [270, 18], [6, 6, 6, 6]),
            status_exp: slice(83, [270, 18], [6, 6, 6, 6]),
            gear: image(60).texture,
            chat_input: slice(90, [400, 40], [10, 10, 10, 10]),
            scroll_track: slice(91, [28, 160], [8, 10, 8, 10]),
            scroll_thumbs: [
                image(92).texture,
                image(93).texture,
                image(94).texture,
                image(95).texture,
            ],
            scroll_up: [image(96).texture, image(97).texture, image(98).texture],
            scroll_down: [image(99).texture, image(100).texture, image(101).texture],
            tooltip_body: slice(102, [224, 112], [14, 14, 14, 18]),
            tooltip_pointer: image(103).texture,
            tooltip_divider: slice(104, [240, 8], [6, 0, 6, 0]),
            slots: [
                image(110).texture,
                image(111).texture,
                image(112).texture,
                image(113).texture,
            ],
            hotbar_body: slice(120, [704, 80], [14, 14, 14, 18]),
            hotbar_slots: [
                image(121).texture,
                image(122).texture,
                image(123).texture,
                image(124).texture,
            ],
            hotbar_counter: slice(125, [28, 22], [6, 6, 6, 6]),
            notification_badge: image(126).texture,
            tab_overlap: 2.0,
        })
    }

    fn show(window: &mut UiDebugWindow) {
        assert!(!window.is_visible());
        window.toggle();
        assert!(window.is_visible());
    }

    fn center(bounds: ScreenRect) -> [f32; 2] {
        [
            (bounds.min[0] + bounds.max[0]) * 0.5,
            (bounds.min[1] + bounds.max[1]) * 0.5,
        ]
    }

    fn quad_rect(quad: &UiTexturedQuad) -> ScreenRect {
        ScreenRect {
            min: quad.corners[0],
            max: quad.corners[2],
        }
    }

    fn click(window: &mut UiDebugWindow, point: [f32; 2]) {
        assert!(window.apply_pointer_button(ElementState::Pressed, Some(point), viewport(), 1.0));
        assert!(window.apply_pointer_button(ElementState::Released, Some(point), viewport(), 1.0));
    }

    fn union_rect(quads: &[&UiTexturedQuad]) -> ScreenRect {
        let mut min = [f32::MAX, f32::MAX];
        let mut max = [f32::MIN, f32::MIN];
        for quad in quads {
            for corner in quad.corners {
                min[0] = min[0].min(corner[0]);
                min[1] = min[1].min(corner[1]);
                max[0] = max[0].max(corner[0]);
                max[1] = max[1].max(corner[1]);
            }
        }
        ScreenRect { min, max }
    }

    fn assert_sliced_text_button(pieces: &[&UiTexturedQuad], bounds: ScreenRect, scale: f32) {
        assert_eq!(pieces.len(), 3);
        let cap = 10.0 * scale;
        assert!((pieces[0].corners[0][0] - bounds.min[0]).abs() < 0.05);
        assert!((pieces[0].corners[2][0] - (bounds.min[0] + cap)).abs() < 0.05);
        assert!((pieces[2].corners[2][0] - bounds.max[0]).abs() < 0.05);
        assert!((pieces[0].corners[0][1] - bounds.min[1]).abs() < 0.05);
        assert!((pieces[0].corners[2][1] - bounds.max[1]).abs() < 0.05);
    }

    fn text_has(frame: &UiDebugFrame, expected: &str) -> bool {
        frame
            .texts
            .iter()
            .any(|block| block.content.0.contains(expected))
    }

    #[test]
    fn debug_assets_resolve_required_v2_names() {
        let mut runtime = AssetRuntime::new();
        let _window = UiDebugWindow::load(&mut runtime).unwrap();
        for name in [
            "panel_body_9slice",
            "panel_header_9slice",
            "close_button_normal",
            "close_button_hover",
            "close_button_pressed",
            "icon_gear",
            "tab_active",
            "tab_inactive",
            "tab_hover",
            "tab_pressed",
            "tabbed_content_body_9slice",
            "tabbed_content_top_edge",
            "button_normal",
            "button_hover",
            "button_pressed",
            "button_disabled",
            "menu_icon_button_normal",
            "menu_icon_button_hover",
            "menu_icon_button_pressed",
            "checkbox_unchecked",
            "checkbox_hover",
            "checkbox_checked",
            "checkbox_disabled",
            "radio_off",
            "radio_hover",
            "radio_on",
            "radio_disabled",
            "slider_track_9slice",
            "slider_handle_normal",
            "slider_handle_hover",
            "slider_handle_pressed",
            "status_track_9slice",
            "status_hp_fill_9slice",
            "status_mp_fill_9slice",
            "status_exp_fill_9slice",
            "chat_input_9slice",
            "scrollbar_track_vertical_9slice",
            "scrollbar_thumb_normal",
            "scrollbar_thumb_hover",
            "scrollbar_thumb_pressed",
            "scrollbar_thumb_disabled",
            "scroll_arrow_up_normal",
            "scroll_arrow_up_hover",
            "scroll_arrow_up_pressed",
            "scroll_arrow_down_normal",
            "scroll_arrow_down_hover",
            "scroll_arrow_down_pressed",
            "tooltip_body_9slice",
            "tooltip_pointer",
            "divider_plain_9slice",
            "slot_normal",
            "slot_hover",
            "slot_selected",
            "slot_disabled",
            "hotbar_body_9slice",
            "hotbar_slot_normal",
            "hotbar_slot_hover",
            "hotbar_slot_selected",
            "hotbar_slot_disabled",
            "hotbar_counter_badge_9slice",
            "notification_badge",
        ] {
            assert!(runtime.texture_for_key(name).is_some(), "{name}");
        }
    }

    #[test]
    fn toggle_replaces_hidden_dev_proof_visibility() {
        let mut window = synthetic();
        assert!(window.frame(viewport(), 1.0, None).unwrap().is_none());
        show(&mut window);
        assert!(window.frame(viewport(), 1.0, None).unwrap().is_some());
        window.toggle();
        assert!(!window.is_visible());
        assert!(window.frame(viewport(), 1.0, None).unwrap().is_none());
    }

    #[test]
    fn hidden_debug_window_captures_no_pointer() {
        let mut window = synthetic();
        show(&mut window);
        let layout = window.layout(viewport(), 1.0).unwrap().unwrap();
        let point = center(layout.click_button);
        window.toggle();
        assert!(!window.apply_pointer_button(ElementState::Pressed, Some(point), viewport(), 1.0));
        assert!(!window.apply_pointer_button(ElementState::Released, Some(point), viewport(), 1.0));
        assert_eq!(window.state.click_count, 0);
        assert!(!window.contains_window(point, viewport(), 1.0));
    }

    #[test]
    fn tabs_select_on_release_inside_and_ignore_release_outside() {
        let mut window = synthetic();
        show(&mut window);
        let layout = window.layout(viewport(), 1.0).unwrap().unwrap();
        let values = center(layout.tabs[1]);
        window.apply_pointer_button(ElementState::Pressed, Some(values), viewport(), 1.0);
        window.apply_pointer_button(
            ElementState::Released,
            Some([layout.window.max[0] + 30.0, values[1]]),
            viewport(),
            1.0,
        );
        assert_eq!(window.tabs.selected_index(), BASICS);
        click(&mut window, values);
        assert_eq!(window.tabs.selected_index(), 1);
        let frame = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(text_has(&frame, "VALUES"));
        assert!(!text_has(&frame, "TEXT INPUT"));
        assert!(!text_has(&frame, "Click Me"));
        let input = center(window.layout(viewport(), 1.0).unwrap().unwrap().tabs[2]);
        click(&mut window, input);
        assert_eq!(window.tabs.selected_index(), 2);
        let planned = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(text_has(&planned, "TEXT INPUT"));
        assert!(text_has(&planned, "Hello world"));
        assert!(!text_has(&planned, "Click Me"));
        let basics = center(window.layout(viewport(), 1.0).unwrap().unwrap().tabs[0]);
        click(&mut window, basics);
        assert_eq!(window.tabs.selected_index(), BASICS);
    }

    #[test]
    fn basics_button_checkbox_radio_and_icon_follow_press_release() {
        let mut window = synthetic();
        show(&mut window);
        let layout = window.layout(viewport(), 1.0).unwrap().unwrap();
        let frame = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(text_has(&frame, "Clicks: 0"));
        assert!(text_has(&frame, "Checkbox: false"));
        assert!(text_has(&frame, "Radio: Medium"));
        assert!(text_has(&frame, "Page: Basics"));
        click(&mut window, center(layout.click_button));
        assert_eq!(window.state.click_count, 1);
        window.apply_pointer_button(
            ElementState::Pressed,
            Some(center(layout.click_button)),
            viewport(),
            1.0,
        );
        window.apply_pointer_button(
            ElementState::Released,
            Some([layout.window.max[0] + 20.0, layout.window.min[1]]),
            viewport(),
            1.0,
        );
        assert_eq!(window.state.click_count, 1);
        assert!(window.apply_pointer_button(
            ElementState::Pressed,
            Some(center(layout.disabled_button)),
            viewport(),
            1.0
        ));
        assert!(!window.disabled_button.is_pressed());
        window.apply_pointer_button(
            ElementState::Released,
            Some(center(layout.disabled_button)),
            viewport(),
            1.0,
        );
        assert_eq!(window.state.click_count, 1);
        click(&mut window, center(layout.icon_button));
        assert_eq!(window.state.icon_click_count, 1);
        let label = [
            layout.checkbox_row.max[0] - 12.0,
            center(layout.checkbox_row)[1],
        ];
        assert!(!layout.checkbox_mark.contains(label));
        assert!(layout.checkbox_row.contains(label));
        click(&mut window, label);
        assert!(window.state.checkbox);
        window.apply_pointer_button(ElementState::Pressed, Some(label), viewport(), 1.0);
        window.apply_pointer_button(
            ElementState::Released,
            Some([layout.window.min[0] - 10.0, label[1]]),
            viewport(),
            1.0,
        );
        assert!(window.state.checkbox);
        assert!(window.apply_pointer_button(
            ElementState::Pressed,
            Some(center(layout.disabled_checkbox_row)),
            viewport(),
            1.0
        ));
        window.apply_pointer_button(
            ElementState::Released,
            Some(center(layout.disabled_checkbox_row)),
            viewport(),
            1.0,
        );
        assert!(window.state.checkbox);
        click(&mut window, center(layout.radio_rows[2]));
        assert_eq!(window.state.radio, UiDebugRadio::High);
        click(&mut window, center(layout.radio_rows[2]));
        assert_eq!(window.state.radio, UiDebugRadio::High);
        assert!(window.apply_pointer_button(
            ElementState::Pressed,
            Some(center(layout.radio_rows[3])),
            viewport(),
            1.0
        ));
        assert!(!window.radios.is_pressed());
        window.apply_pointer_button(
            ElementState::Released,
            Some(center(layout.radio_rows[3])),
            viewport(),
            1.0,
        );
        assert_eq!(window.state.radio, UiDebugRadio::High);
        let shown = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(text_has(&shown, "Clicks: 1"));
        assert!(text_has(&shown, "Icon: 1"));
        assert!(text_has(&shown, "Checkbox: true"));
        assert!(text_has(&shown, "Radio: High"));
        assert_eq!(
            shown
                .skin_quads
                .iter()
                .filter(|quad| quad.texture
                    == window
                        .assets
                        .radios
                        .texture(crate::ui_controls::UiRadioVisual::On))
                .count(),
            1
        );
    }

    #[test]
    fn rendered_control_and_close_bounds_match_hits() {
        let mut window = synthetic();
        show(&mut window);
        let layout = window.layout(viewport(), 1.0).unwrap().unwrap();
        let frame = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        let click = frame
            .skin_quads
            .iter()
            .filter(|quad| quad.texture == window.assets.buttons.texture(UiButtonVisual::Normal))
            .collect::<Vec<_>>();
        assert_sliced_text_button(&click, layout.click_button, 1.0);
        let disabled = frame
            .skin_quads
            .iter()
            .filter(|quad| quad.texture == window.assets.buttons.texture(UiButtonVisual::Disabled))
            .collect::<Vec<_>>();
        assert_sliced_text_button(&disabled, layout.disabled_button, 1.0);
        let icon = frame
            .skin_quads
            .iter()
            .find(|quad| quad.texture == window.assets.icon_buttons.texture(UiButtonVisual::Normal))
            .unwrap();
        assert_eq!(quad_rect(icon), layout.icon_button);
        let glyph = frame
            .skin_quads
            .iter()
            .find(|quad| {
                quad.texture == window.assets.gear && layout.icon_button.contains(quad.corners[0])
            })
            .unwrap();
        assert_eq!(quad_rect(glyph), layout.icon_glyph);
        let mark = frame
            .skin_quads
            .iter()
            .find(|quad| {
                quad.texture
                    == window
                        .assets
                        .checkboxes
                        .texture(crate::ui_controls::UiCheckboxVisual::Unchecked)
            })
            .unwrap();
        assert_eq!(quad_rect(mark), layout.checkbox_mark);
        assert!(layout.checkbox_row.contains(center(layout.checkbox_mark)));
        let close = frame
            .skin_quads
            .iter()
            .find(|quad| quad.texture == window.assets.close[0])
            .unwrap();
        assert_eq!(quad_rect(close), layout.close_button);
        let hovered = window
            .frame(viewport(), 1.0, Some(center(layout.click_button)))
            .unwrap()
            .unwrap();
        assert!(
            hovered.skin_quads.iter().any(|quad| {
                quad.texture == window.assets.buttons.texture(UiButtonVisual::Hover)
            })
        );
        window.apply_pointer_button(
            ElementState::Pressed,
            Some(center(layout.click_button)),
            viewport(),
            1.0,
        );
        let pressed = window
            .frame(viewport(), 1.0, Some(center(layout.click_button)))
            .unwrap()
            .unwrap();
        assert!(pressed.skin_quads.iter().any(|quad| {
            quad.texture == window.assets.buttons.texture(UiButtonVisual::Pressed)
        }));
        window.cancel_pointer_interaction();
    }

    #[test]
    fn close_and_header_drag_use_chrome_bounds() {
        let mut window = synthetic();
        show(&mut window);
        let layout = window.layout(viewport(), 1.0).unwrap().unwrap();
        let close = center(layout.close_button);
        window.apply_pointer_button(ElementState::Pressed, Some(close), viewport(), 1.0);
        window.apply_pointer_button(
            ElementState::Released,
            Some([layout.window.max[0] + 12.0, close[1]]),
            viewport(),
            1.0,
        );
        assert!(window.is_visible());
        click(&mut window, close);
        assert!(!window.is_visible());

        show(&mut window);
        let layout = window.layout(viewport(), 1.0).unwrap().unwrap();
        let header = [
            (layout.header.min[0] + layout.header.max[0]) * 0.5,
            (layout.header.min[1] + layout.header.max[1]) * 0.5,
        ];
        assert!(layout.header.contains(header));
        assert!(!layout.close_button.contains(header));
        assert!(window.apply_pointer_button(ElementState::Pressed, Some(header), viewport(), 1.0));
        assert!(window.pointer_moved([header[0] + 24.0, header[1] + 10.0], viewport(), 1.0));
        let moved = window.layout(viewport(), 1.0).unwrap().unwrap();
        assert!(moved.window.min[0] > layout.window.min[0] + 10.0);
        assert!(window.is_visible());
        window.apply_pointer_button(
            ElementState::Released,
            Some([header[0] + 24.0, header[1] + 10.0]),
            viewport(),
            1.0,
        );
        assert!(window.is_visible());
    }

    #[test]
    fn layout_stays_aligned_at_90_100_and_125_percent() {
        let mut window = synthetic();
        show(&mut window);
        let mut boxes = Vec::new();
        for scale in [0.9_f32, 1.0, 1.25] {
            let layout = window.layout(viewport(), scale).unwrap().unwrap();
            assert!((layout.window.width() / scale - UI_DEBUG_WINDOW_UNITS[0]).abs() < 0.05);
            assert!((layout.window.height() / scale - UI_DEBUG_WINDOW_UNITS[1]).abs() < 0.05);
            assert!((layout.click_button.width() / scale - BUTTON_WIDTH).abs() < 0.05);
            assert!((layout.click_button.height() / scale - BUTTON_HEIGHT).abs() < 0.05);
            assert!((layout.icon_button.width() / scale - ICON_BUTTON_SIZE).abs() < 0.05);
            assert!((layout.close_button.width() / scale - CLOSE_SIZE).abs() < 0.05);
            assert!((layout.checkbox_mark.width() / scale - MARK_SIZE).abs() < 0.05);
            let frame = window.frame(viewport(), scale, None).unwrap().unwrap();
            let click = frame
                .texts
                .iter()
                .find(|block| block.content.0 == "Click Me")
                .unwrap();
            assert!((click.style.font_size - BODY_FONT).abs() < 0.001);
            assert!(
                click
                    .max_width
                    .is_some_and(|width| width < layout.click_button.width())
            );
            let content = crate::ui_panel::button_content_bounds(layout.click_button, scale);
            assert!(click.anchor[1] >= content.min[1] - 0.05);
            assert!(click.anchor[1] + BODY_FONT * scale <= content.max[1] + 0.05);
            let relative = |rect: ScreenRect| {
                let quantize = |value: f32| (value / scale * 1000.0).round() as i32;
                [
                    quantize(rect.min[0] - layout.window.min[0]),
                    quantize(rect.min[1] - layout.window.min[1]),
                    quantize(rect.width()),
                    quantize(rect.height()),
                ]
            };
            boxes.push((
                relative(layout.click_button),
                relative(layout.checkbox_row),
                relative(layout.radio_rows[1]),
                relative(layout.tabs[0]),
                relative(layout.close_button),
                relative(quad_rect(
                    frame
                        .skin_quads
                        .iter()
                        .find(|quad| {
                            quad.texture == window.assets.buttons.texture(UiButtonVisual::Normal)
                        })
                        .unwrap(),
                )),
            ));
            assert!(layout.click_button.contains(center(layout.click_button)));
            assert_eq!(
                quad_rect(
                    frame
                        .skin_quads
                        .iter()
                        .find(|quad| quad.texture == window.assets.close[0])
                        .unwrap()
                ),
                layout.close_button
            );
        }
        assert_eq!(boxes[0], boxes[1]);
        assert_eq!(boxes[1], boxes[2]);
    }

    #[test]
    fn connected_tabs_keep_a_full_body_and_side_rails() {
        let mut window = synthetic();
        show(&mut window);
        for selected in [0_usize, 4] {
            if selected != 0 {
                let layout = window.layout(viewport(), 1.0).unwrap().unwrap();
                click(&mut window, center(layout.tabs[selected]));
            }
            let layout = window.layout(viewport(), 1.0).unwrap().unwrap();
            let frame = window.frame(viewport(), 1.0, None).unwrap().unwrap();
            let body: Vec<_> = frame
                .skin_quads
                .iter()
                .filter(|quad| quad.texture == window.assets.tabbed_body.texture)
                .collect();
            assert_eq!(body.len(), 9);
            let rails: Vec<_> = frame
                .skin_quads
                .iter()
                .filter(|quad| quad.texture == window.assets.tabbed_rail)
                .collect();
            assert_eq!(rails.len(), 2);
            let active = layout.tabs[selected];
            assert!(
                (rails[0].corners[1][0] - active.min[0]).abs() < 0.6
                    || (rails[1].corners[1][0] - active.min[0]).abs() < 0.6
            );
            assert!(
                rails
                    .iter()
                    .any(|quad| (quad.corners[0][0] - active.max[0]).abs() < 0.6
                        || (quad.corners[1][0] - active.min[0]).abs() < 0.6)
            );
            let body_at = frame
                .skin_quads
                .iter()
                .position(|quad| quad.texture == window.assets.tabbed_body.texture)
                .unwrap();
            let inactive_at = frame
                .skin_quads
                .iter()
                .position(|quad| quad.texture == window.assets.tabs.texture(UiTabVisual::Inactive))
                .unwrap();
            let active_at = frame
                .skin_quads
                .iter()
                .rposition(|quad| quad.texture == window.assets.tabs.texture(UiTabVisual::Active))
                .unwrap();
            let rail_at = frame
                .skin_quads
                .iter()
                .rposition(|quad| quad.texture == window.assets.tabbed_rail)
                .unwrap();
            assert!(inactive_at < body_at);
            assert!(rail_at < active_at);
        }
    }

    fn open_values(window: &mut UiDebugWindow) -> DebugLayout {
        show(window);
        let values = center(window.layout(viewport(), 1.0).unwrap().unwrap().tabs[VALUES]);
        click(window, values);
        window.layout(viewport(), 1.0).unwrap().unwrap()
    }

    fn progress_inset(scale: f32) -> [f32; 4] {
        PROGRESS_FILL_INSET.map(|edge| edge * scale)
    }

    fn union_quads<'a>(quads: impl Iterator<Item = &'a UiTexturedQuad>) -> ScreenRect {
        let mut min = [f32::MAX, f32::MAX];
        let mut max = [f32::MIN, f32::MIN];
        for quad in quads {
            for corner in quad.corners {
                min[0] = min[0].min(corner[0]);
                min[1] = min[1].min(corner[1]);
                max[0] = max[0].max(corner[0]);
                max[1] = max[1].max(corner[1]);
            }
        }
        ScreenRect { min, max }
    }

    fn quads_covering<'a>(
        quads: impl Iterator<Item = &'a UiTexturedQuad>,
        texture: SpriteTextureId,
        bounds: ScreenRect,
    ) -> Vec<&'a UiTexturedQuad> {
        quads
            .filter(|quad| quad.texture == texture)
            .filter(|quad| bounds.contains(center(quad_rect(quad))))
            .collect()
    }

    fn close_rects(actual: ScreenRect, expected: ScreenRect) {
        for (left, right) in actual
            .min
            .into_iter()
            .chain(actual.max)
            .zip(expected.min.into_iter().chain(expected.max))
        {
            assert!((left - right).abs() < 0.6, "{actual:?} != {expected:?}");
        }
    }

    #[test]
    fn values_page_drives_hp_from_the_slider() {
        let mut window = synthetic();
        let layout = open_values(&mut window);
        let frame = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(text_has(&frame, "VALUES"));
        assert!(text_has(&frame, "67%"));
        assert!(text_has(&frame, "40%"));
        assert!(text_has(&frame, "82%"));
        assert!(text_has(&frame, "Value: 0.670"));
        assert!(text_has(&frame, "Dragging: No"));
        assert!(!text_has(&frame, "TEXT INPUT"));
        assert!(
            frame
                .skin_quads
                .iter()
                .any(|quad| quad.texture == window.assets.status_hp.texture)
        );
        assert!(
            frame
                .skin_quads
                .iter()
                .any(|quad| quad.texture == window.assets.status_mp.texture)
        );
        assert!(
            frame
                .skin_quads
                .iter()
                .any(|quad| quad.texture == window.assets.status_exp.texture)
        );
        let hp =
            progress_geometry(layout.hp_track, progress_inset(1.0), window.state.hp_value).unwrap();
        let hp_fill = union_quads(
            quads_covering(
                frame.skin_quads.iter(),
                window.assets.status_hp.texture,
                layout.hp_track,
            )
            .into_iter(),
        );
        close_rects(hp_fill, hp.fill.unwrap());
        let mp = progress_geometry(layout.mp_track, progress_inset(1.0), MP_SAMPLE).unwrap();
        let mp_fill = union_quads(
            quads_covering(
                frame.skin_quads.iter(),
                window.assets.status_mp.texture,
                layout.mp_track,
            )
            .into_iter(),
        );
        close_rects(mp_fill, mp.fill.unwrap());
        let exp = progress_geometry(layout.exp_track, progress_inset(1.0), EXP_SAMPLE).unwrap();
        let exp_fill = union_quads(
            quads_covering(
                frame.skin_quads.iter(),
                window.assets.status_exp.texture,
                layout.exp_track,
            )
            .into_iter(),
        );
        close_rects(exp_fill, exp.fill.unwrap());

        let handle = slider_geometry(
            layout.hp_slider_track,
            SLIDER_HANDLE_SIZE,
            window.state.hp_value,
        )
        .unwrap();
        let before = window.state.hp_value;
        assert!(window.apply_pointer_button(
            ElementState::Pressed,
            Some(center(handle.handle)),
            viewport(),
            1.0
        ));
        assert!(window.hp_slider.is_dragging());
        assert!((window.state.hp_value - before).abs() < 1e-5);
        let pressed = window
            .frame(viewport(), 1.0, Some(center(handle.handle)))
            .unwrap()
            .unwrap();
        assert!(text_has(&pressed, "Dragging: Yes"));
        assert!(pressed.skin_quads.iter().any(|quad| {
            quad.texture
                == window
                    .assets
                    .slider_handles
                    .texture(UiSliderHandleVisual::Pressed)
        }));
        window.apply_pointer_button(
            ElementState::Released,
            Some(center(handle.handle)),
            viewport(),
            1.0,
        );
        assert!(!window.hp_slider.is_dragging());

        let jump = [
            layout.hp_slider_track.min[0] + 1.0,
            center(layout.hp_slider_track)[1],
        ];
        assert!(layout.hp_slider_track.contains(jump));
        assert!(!handle.handle.contains(jump));
        assert!(window.apply_pointer_button(ElementState::Pressed, Some(jump), viewport(), 1.0));
        let jumped = slider_value_from_x(layout.hp_slider_track, jump[0]).unwrap();
        assert!((window.state.hp_value - jumped).abs() < 1e-4);
        let origin = layout.window.min;
        let outside = [layout.window.max[0] + 80.0, layout.window.min[1] - 120.0];
        assert!(window.pointer_moved(outside, viewport(), 1.0));
        assert!(window.hp_slider.is_dragging());
        assert!((window.state.hp_value - 1.0).abs() < 1e-4);
        let stayed = window.layout(viewport(), 1.0).unwrap().unwrap();
        assert_eq!(stayed.window.min, origin);
        let above = [
            layout.hp_slider_track.min[0] - 40.0,
            layout.window.max[1] + 200.0,
        ];
        assert!(window.pointer_moved(above, viewport(), 1.0));
        assert!(window.hp_slider.is_dragging());
        assert!(window.state.hp_value.abs() < 1e-4);
        window.apply_pointer_button(ElementState::Released, Some(above), viewport(), 1.0);
        assert!(!window.hp_slider.is_dragging());
        let done = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(text_has(&done, "0%"));
        assert!(text_has(&done, "Value: 0.000"));
        assert!(text_has(&done, "Dragging: No"));
        let zero =
            progress_geometry(stayed.hp_track, progress_inset(1.0), window.state.hp_value).unwrap();
        assert!(zero.fill.is_none());
        assert!(
            !done
                .skin_quads
                .iter()
                .any(|quad| quad.texture == window.assets.status_hp.texture)
        );
        assert!((window.state.hp_value).abs() < 1e-4);

        window.toggle();
        assert!(!window.apply_pointer_button(ElementState::Pressed, Some(jump), viewport(), 1.0));
        assert!(window.state.hp_value.abs() < 1e-4);
        assert!(!window.hp_slider.is_dragging());
    }

    #[test]
    fn switching_pages_cancels_slider_capture_and_keeps_hp() {
        let mut window = synthetic();
        let layout = open_values(&mut window);
        let jump = [
            layout.hp_slider_track.max[0] - 1.0,
            center(layout.hp_slider_track)[1],
        ];
        assert!(window.apply_pointer_button(ElementState::Pressed, Some(jump), viewport(), 1.0));
        assert!(window.hp_slider.is_dragging());
        let on_basics = center(layout.tabs[BASICS]);
        window.apply_pointer_button(ElementState::Released, Some(on_basics), viewport(), 1.0);
        assert!(!window.hp_slider.is_dragging());
        assert_eq!(window.tabs.selected_index(), VALUES);
        let committed = slider_value_from_x(layout.hp_slider_track, on_basics[0]).unwrap();
        assert!((window.state.hp_value - committed).abs() < 1e-4);

        assert!(window.apply_pointer_button(ElementState::Pressed, Some(jump), viewport(), 1.0));
        window.apply_pointer_button(ElementState::Released, Some(jump), viewport(), 1.0);
        assert!(!window.hp_slider.is_dragging());
        let kept = window.state.hp_value;
        assert!(kept > 0.9);
        click(&mut window, on_basics);
        assert_eq!(window.tabs.selected_index(), BASICS);
        assert!(!window.hp_slider.is_dragging());
        assert!((window.state.hp_value - kept).abs() < 1e-4);
        let basics_frame = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(text_has(&basics_frame, "Click Me"));
        assert!(!text_has(&basics_frame, "Dragging:"));

        let values_tab = center(window.layout(viewport(), 1.0).unwrap().unwrap().tabs[VALUES]);
        click(&mut window, values_tab);
        assert!(window.apply_pointer_button(ElementState::Pressed, Some(jump), viewport(), 1.0));
        assert!(window.hp_slider.is_dragging());
        let during = window.state.hp_value;
        let strip = tab_strip(&layout.tabs);
        window.tabs.apply_pointer_button(
            ElementState::Pressed,
            Some(on_basics),
            strip,
            PAGES.len(),
            0.0,
        );
        window.tabs.apply_pointer_button(
            ElementState::Released,
            Some(on_basics),
            strip,
            PAGES.len(),
            0.0,
        );
        assert_eq!(window.tabs.selected_index(), BASICS);
        let _ = window.frame(viewport(), 1.0, None).unwrap();
        assert!(!window.hp_slider.is_dragging());
        assert!((window.state.hp_value - during).abs() < 1e-4);
        let basics_frame = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(text_has(&basics_frame, "Click Me"));
        assert!(!text_has(&basics_frame, "Dragging:"));
        let values_tab = center(window.layout(viewport(), 1.0).unwrap().unwrap().tabs[VALUES]);
        click(&mut window, values_tab);
        let restored = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(text_has(&restored, "VALUES"));
        assert!(text_has(&restored, "Value:"));
        assert!((window.state.hp_value - kept).abs() < 1e-4);
        for (index, heading) in [(2, "TEXT INPUT"), (3, "SCROLL VIEW"), (4, "SLOTS")] {
            let tab = center(window.layout(viewport(), 1.0).unwrap().unwrap().tabs[index]);
            click(&mut window, tab);
            let page = window.frame(viewport(), 1.0, None).unwrap().unwrap();
            assert!(text_has(&page, heading));
            assert!(!window.hp_slider.is_dragging());
        }
    }

    #[test]
    fn values_slider_stays_aligned_at_90_100_and_125_percent() {
        let mut window = synthetic();
        let _ = open_values(&mut window);
        let mut boxes = Vec::new();
        for scale in [0.9_f32, 1.0, 1.25] {
            let layout = window.layout(viewport(), scale).unwrap().unwrap();
            let slider = slider_geometry(
                layout.hp_slider_track,
                SLIDER_HANDLE_SIZE * scale,
                window.state.hp_value,
            )
            .unwrap();
            let bar = progress_geometry(
                layout.hp_track,
                progress_inset(scale),
                window.state.hp_value,
            )
            .unwrap();
            let frame = window.frame(viewport(), scale, None).unwrap().unwrap();
            let track = union_quads(
                frame
                    .skin_quads
                    .iter()
                    .filter(|quad| quad.texture == window.assets.slider_track.texture),
            );
            close_rects(track, layout.hp_slider_track);
            let handle = frame
                .skin_quads
                .iter()
                .find(|quad| {
                    quad.texture
                        == window
                            .assets
                            .slider_handles
                            .texture(UiSliderHandleVisual::Normal)
                })
                .unwrap();
            close_rects(quad_rect(handle), slider.handle);
            let hp_track = union_quads(
                quads_covering(
                    frame.skin_quads.iter(),
                    window.assets.status_track.texture,
                    layout.hp_track,
                )
                .into_iter(),
            );
            close_rects(hp_track, layout.hp_track);
            let hp_fill = union_quads(
                quads_covering(
                    frame.skin_quads.iter(),
                    window.assets.status_hp.texture,
                    layout.hp_track,
                )
                .into_iter(),
            );
            close_rects(hp_fill, bar.fill.unwrap());
            assert!(slider.hit.contains(center(slider.handle)));
            assert!(slider.hit.contains(center(layout.hp_slider_track)));
            let relative = |rect: ScreenRect| {
                let quantize = |value: f32| (value / scale * 1000.0).round() as i32;
                [
                    quantize(rect.min[0] - layout.window.min[0]),
                    quantize(rect.min[1] - layout.window.min[1]),
                    quantize(rect.width()),
                    quantize(rect.height()),
                ]
            };
            boxes.push((
                relative(layout.hp_slider_track),
                relative(slider.handle),
                relative(slider.hit),
                relative(layout.hp_track),
                relative(bar.fill.unwrap()),
            ));
        }
        assert_eq!(boxes[0], boxes[1]);
        assert_eq!(boxes[1], boxes[2]);
    }

    fn open_page(window: &mut UiDebugWindow, index: usize) -> DebugLayout {
        show(window);
        let tab = center(window.layout(viewport(), 1.0).unwrap().unwrap().tabs[index]);
        click(window, tab);
        assert_eq!(window.tabs.selected_index(), index);
        window.layout(viewport(), 1.0).unwrap().unwrap()
    }

    #[test]
    fn text_input_edits_blur_and_keeps_u_for_the_field() {
        let mut window = synthetic();
        let layout = open_page(&mut window, INPUT_PAGE);
        let frame = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert_eq!(frame.rects.len(), 1);
        assert_eq!(frame.rects[0].color, INPUT_FILL);
        let interior = field_interior(layout.input.field, 1.0);
        assert_eq!(frame.rects[0].min, interior.min);
        assert_eq!(frame.rects[0].max, interior.max);
        assert!(interior.width() > 40.0 && interior.height() > BODY_FONT);
        assert!(text_has(&frame, "Hello world"));
        assert!(text_has(&frame, "Characters: 11"));
        assert!(text_has(&frame, "Focused: No"));
        assert!(!frame.texts.iter().any(|text| text.content.0.contains('|')));
        assert!(window.apply_pointer_button(
            ElementState::Pressed,
            Some(center(layout.input.field)),
            viewport(),
            1.0,
        ));
        let _ = window.apply_pointer_button(
            ElementState::Released,
            Some(center(layout.input.field)),
            viewport(),
            1.0,
        );
        assert!(window.wants_text_keyboard());
        assert!(!u_key_toggles_ui_debug(window.wants_text_keyboard()));
        assert!(window.apply_text_key(
            &Key::Named(winit::keyboard::NamedKey::End),
            None,
            false,
            UiTextNav::default(),
            1.0,
        ));
        assert_eq!(window.text_input.caret(), 11);
        assert!(window.apply_text_key(
            &Key::Named(winit::keyboard::NamedKey::ArrowLeft),
            None,
            false,
            UiTextNav::default(),
            1.0,
        ));
        assert_eq!(window.text_input.caret(), 10);
        assert!(window.apply_text_key(
            &Key::Character("u".into()),
            Some("u"),
            false,
            UiTextNav::default(),
            1.0,
        ));
        assert_eq!(window.state.input_text, "Hello worlud");
        assert!(window.wants_text_keyboard());
        let focused = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(focused.rects.iter().any(|rect| rect.color == INK));
        assert!(
            !focused
                .texts
                .iter()
                .any(|text| text.content.0.contains('|'))
        );
        assert!(text_has(&focused, "Focused: Yes"));
        window.apply_pointer_button(
            ElementState::Pressed,
            Some([layout.window.max[0] + 20.0, layout.window.min[1]]),
            viewport(),
            1.0,
        );
        assert!(!window.wants_text_keyboard());
        assert!(u_key_toggles_ui_debug(window.wants_text_keyboard()));
        assert!(window.apply_pointer_button(
            ElementState::Pressed,
            Some(center(layout.input.field)),
            viewport(),
            1.0,
        ));
        let _ = window.apply_pointer_button(
            ElementState::Released,
            Some(center(layout.input.field)),
            viewport(),
            1.0,
        );
        let before = window.state.input_text.clone();
        let basics = center(window.layout(viewport(), 1.0).unwrap().unwrap().tabs[BASICS]);
        click(&mut window, basics);
        assert!(!window.wants_text_keyboard());
        assert!(!window.apply_text_key(
            &Key::Character("z".into()),
            Some("z"),
            false,
            UiTextNav::default(),
            1.0,
        ));
        assert_eq!(window.state.input_text, before);
        window.toggle();
        assert!(!window.wants_text_keyboard());
        assert!(!window.apply_text_key(
            &Key::Named(winit::keyboard::NamedKey::Backspace),
            None,
            true,
            UiTextNav::default(),
            1.0,
        ));
    }

    #[test]
    fn scroll_view_wheel_arrows_thumb_and_visible_rows() {
        let mut window = synthetic();
        let layout = open_page(&mut window, CONTAINERS_PAGE);
        let frame = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(text_has(&frame, "Item 01"));
        assert!(text_has(&frame, "Item 06"));
        assert!(!text_has(&frame, "Item 07"));
        assert!(text_has(&frame, "Offset: 0 / 18"));
        assert!(!window.apply_wheel(
            MouseScrollDelta::LineDelta(0.0, -1.0),
            Some([layout.window.min[0] - 8.0, layout.window.min[1]]),
            viewport(),
            1.0,
        ));
        assert_eq!(window.state.scroll_offset, 0);
        assert!(window.apply_wheel(
            MouseScrollDelta::LineDelta(0.0, -1.0),
            Some(center(layout.containers.scroll.list)),
            viewport(),
            1.0,
        ));
        assert_eq!(window.state.scroll_offset, 1);
        let up = center(layout.containers.scroll.up);
        click(&mut window, up);
        assert_eq!(window.state.scroll_offset, 0);
        window.apply_pointer_button(ElementState::Pressed, Some(up), viewport(), 1.0);
        window.apply_pointer_button(
            ElementState::Released,
            Some([layout.window.max[0] + 30.0, up[1]]),
            viewport(),
            1.0,
        );
        assert_eq!(window.state.scroll_offset, 0);
        let down = center(
            window
                .layout(viewport(), 1.0)
                .unwrap()
                .unwrap()
                .containers
                .scroll
                .down,
        );
        click(&mut window, down);
        assert_eq!(window.state.scroll_offset, 1);
        let thumb = center(
            window
                .layout(viewport(), 1.0)
                .unwrap()
                .unwrap()
                .containers
                .scroll
                .thumb,
        );
        window.apply_pointer_button(ElementState::Pressed, Some(thumb), viewport(), 1.0);
        assert!(window.scrollbar.is_dragging());
        let moved = window.pointer_moved([thumb[0], thumb[1] + 40.0], viewport(), 1.0);
        assert!(moved);
        assert!(window.state.scroll_offset > 1);
        let during = window.state.scroll_offset;
        window.apply_pointer_button(
            ElementState::Released,
            Some([thumb[0], thumb[1] + 40.0]),
            viewport(),
            1.0,
        );
        assert!(!window.scrollbar.is_dragging());
        assert_eq!(window.state.scroll_offset, during);
        let shown = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        let first = window.state.scroll_offset + 1;
        assert!(text_has(&shown, &format!("Item {first:02}")));
        assert!(!text_has(&shown, "Item 01"));
        for _ in 0..30 {
            assert!(window.apply_wheel(
                MouseScrollDelta::LineDelta(0.0, -1.0),
                Some(center(layout.containers.scroll.list)),
                viewport(),
                1.0,
            ));
        }
        assert_eq!(
            window.state.scroll_offset,
            scroll_max_offset(SCROLL_ITEMS, SCROLL_VISIBLE)
        );
        let layout = window.layout(viewport(), 1.0).unwrap().unwrap();
        let thumb = center(layout.containers.scroll.thumb);
        window.apply_pointer_button(ElementState::Pressed, Some(thumb), viewport(), 1.0);
        assert!(window.scrollbar.is_dragging());
        let basics = center(layout.tabs[BASICS]);
        let strip = tab_strip(&layout.tabs);
        window.tabs.apply_pointer_button(
            ElementState::Pressed,
            Some(basics),
            strip,
            PAGES.len(),
            0.0,
        );
        window.tabs.apply_pointer_button(
            ElementState::Released,
            Some(basics),
            strip,
            PAGES.len(),
            0.0,
        );
        assert_eq!(window.tabs.selected_index(), BASICS);
        let _ = window.frame(viewport(), 1.0, None).unwrap();
        assert!(!window.scrollbar.is_dragging());
        window.cancel_pointer_interaction();
        assert!(!window.text_input.is_focused());
    }

    #[test]
    fn section_opens_and_closes_only_on_release_inside() {
        let mut window = synthetic();
        let layout = open_page(&mut window, CONTAINERS_PAGE);
        assert!(!window.state.section_open);
        let closed = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(text_has(&closed, "Open Advanced Section"));
        assert!(!text_has(&closed, "Row A"));
        let button = center(layout.containers.section_button);
        window.apply_pointer_button(ElementState::Pressed, Some(button), viewport(), 1.0);
        window.apply_pointer_button(
            ElementState::Released,
            Some([layout.window.max[0] + 20.0, button[1]]),
            viewport(),
            1.0,
        );
        assert!(!window.state.section_open);
        click(&mut window, button);
        assert!(window.state.section_open);
        let open = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(text_has(&open, "Close Advanced Section"));
        assert!(text_has(&open, "Row A"));
        assert!(text_has(&open, "Row C"));
        click(&mut window, button);
        assert!(!window.state.section_open);
        let hidden = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(!text_has(&hidden, "Row B"));
    }

    #[test]
    fn tooltip_size_follows_content_and_wraps_inside_the_window() {
        let window = ScreenRect {
            min: [0.0, 0.0],
            max: [560.0, 420.0],
        };
        let target = ScreenRect {
            min: [40.0, 80.0],
            max: [76.0, 116.0],
        };
        let short = place_tooltip(
            &TooltipContent {
                title: "Hi".into(),
                title_color: TOOLTIP_ITEM_TITLE,
                description: "Ok".into(),
            },
            target,
            window,
            1.0,
        )
        .unwrap();
        let long_copy =
            "This tooltip sentence is long enough that it must wrap inside the maximum width.";
        let long = place_tooltip(
            &TooltipContent {
                title: "Training Sword".into(),
                title_color: TOOLTIP_ITEM_TITLE,
                description: long_copy.into(),
            },
            target,
            window,
            1.0,
        )
        .unwrap();
        assert_eq!(long.lines[0].style.color, TOOLTIP_ITEM_TITLE);
        assert_eq!(long.lines[1].style.color, TOOLTIP_BODY);
        assert!(long.divider.min[1] >= long.lines[0].anchor[1]);
        assert!(long.lines[1].anchor[1] >= long.divider.max[1] - 0.05);
        let description = measure_text(&long.lines[1], 1.0).unwrap();
        assert!(description.line_count > 1);
        assert!(short.body.width() < TOOLTIP_MAX_WIDTH - 20.0);
        assert!(long.body.width() <= TOOLTIP_MAX_WIDTH + 0.05);
        assert!(long.body.width() > short.body.width());
        assert!(long.body.height() > short.body.height() + TOOLTIP_LINE_GAP);
        let content_limit =
            TOOLTIP_MAX_WIDTH - TOOLTIP_BORDER[0] - TOOLTIP_BORDER[2] - TOOLTIP_INNER_PAD_X * 2.0;
        assert!(long.lines[1].max_width.unwrap() <= content_limit + 0.05);
        let inset = [
            TOOLTIP_BORDER[0] + TOOLTIP_INNER_PAD_X,
            TOOLTIP_BORDER[1] + TOOLTIP_INNER_PAD_Y,
            TOOLTIP_BORDER[2] + TOOLTIP_INNER_PAD_X,
            TOOLTIP_BORDER[3] + TOOLTIP_INNER_PAD_Y,
        ];
        assert!(long.lines[0].anchor[0] >= long.body.min[0] + inset[0] - 0.05);
        assert!(long.lines[0].anchor[1] >= long.body.min[1] + inset[1] - 0.05);
        assert!(
            long.lines[1].anchor[0] + long.lines[1].max_width.unwrap()
                <= long.body.max[0] - inset[2] + 0.05
        );
        assert!(long.lines[1].anchor[1] + description.height <= long.body.max[1] - inset[3] + 0.05);
        assert!(long.divider.max[0] <= long.body.max[0] - inset[2] + 0.05);
        assert!((short.pointer.max[0] - short.body.min[0]).abs() < 0.05);
        for scale in [0.9_f32, 1.25] {
            let placed = place_tooltip(&sample_tooltip(), target, window, scale).unwrap();
            assert!(placed.body.max[0] <= window.max[0]);
            assert!(placed.body.max[1] <= window.max[1]);
            assert!(placed.body.min[0] >= window.min[0]);
            assert!(placed.pointer.max[1] <= placed.body.max[1] + 0.05);
            assert_eq!(placed.lines.len(), 2);
            assert_eq!(placed.lines[0].style.color, TOOLTIP_ITEM_TITLE);
        }
    }

    #[test]
    fn composite_tooltip_slots_hotbar_badge_and_dialog_event() {
        let mut window = synthetic();
        let layout = open_page(&mut window, COMPOSITE_PAGE);
        assert_eq!(window.state.selected_slot, 2);
        assert_eq!(window.state.selected_hotbar_slot, 0);
        let away = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(!text_has(&away, "Training Sword"));
        let hover = window
            .frame(viewport(), 1.0, Some(center(layout.composite.slots[1])))
            .unwrap()
            .unwrap();
        assert!(text_has(&hover, "Training Sword"));
        assert!(text_has(&hover, "Example UI DEBUG tooltip"));
        let tooltip = hover
            .skin_quads
            .iter()
            .filter(|quad| quad.texture == window.assets.tooltip_body.texture)
            .collect::<Vec<_>>();
        assert!(!tooltip.is_empty());
        for quad in &tooltip {
            for corner in quad.corners {
                assert!(layout.window.contains(corner));
            }
        }
        let pointer = hover
            .skin_quads
            .iter()
            .find(|quad| quad.texture == window.assets.tooltip_pointer)
            .unwrap();
        for corner in pointer.corners {
            assert!(layout.window.contains(corner));
        }
        let body = union_rect(&tooltip);
        let attached = (pointer.corners[1][0] - body.min[0]).abs() < 0.05
            || (pointer.corners[0][0] - body.max[0]).abs() < 0.05;
        assert!(attached);
        assert!(pointer.corners[0][1] >= body.min[1] - 0.05);
        assert!(pointer.corners[2][1] <= body.max[1] + 0.05);
        click(&mut window, center(layout.composite.slots[1]));
        assert_eq!(window.state.selected_slot, 1);
        let disabled_slot = center(layout.composite.slots[3]);
        window.apply_pointer_button(ElementState::Pressed, Some(disabled_slot), viewport(), 1.0);
        window.apply_pointer_button(ElementState::Released, Some(disabled_slot), viewport(), 1.0);
        assert_eq!(window.state.selected_slot, 1);
        click(&mut window, center(layout.composite.hotbar_slots[2]));
        assert_eq!(window.state.selected_hotbar_slot, 2);
        let disabled_hotbar = center(layout.composite.hotbar_slots[HOTBAR_COUNT - 1]);
        window.apply_pointer_button(
            ElementState::Pressed,
            Some(disabled_hotbar),
            viewport(),
            1.0,
        );
        window.apply_pointer_button(
            ElementState::Released,
            Some(disabled_hotbar),
            viewport(),
            1.0,
        );
        assert_eq!(window.state.selected_hotbar_slot, 2);
        let counter = layout.composite.counter;
        let slot = layout.composite.hotbar_slots[HOTBAR_COUNTER_SLOT];
        assert!(counter.min[0] >= slot.min[0] && counter.max[0] <= slot.max[0]);
        assert!(counter.min[1] >= slot.min[1] && counter.max[1] <= slot.max[1]);
        let marked = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(marked.texts.iter().any(|text| text.content.0 == "12"));
        assert!(marked.texts.iter().any(|text| text.content.0 == "0"));
        click(&mut window, center(layout.composite.notify_button));
        click(&mut window, center(layout.composite.notify_button));
        assert_eq!(window.state.notification_count, 2);
        assert_eq!(window.state.notification_pulse_generation, 2);
        let counted = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(counted.texts.iter().any(|text| text.content.0 == "2"));
        assert!(counted.texts.iter().any(|text| text.content.0 == "12"));
        for _ in 0..8 {
            click(&mut window, center(layout.composite.notify_button));
        }
        assert_eq!(window.state.notification_count, 10);
        assert_eq!(window.state.notification_pulse_generation, 10);
        let plus = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(plus.texts.iter().any(|text| text.content.0 == "+"));
        assert!(!plus.texts.iter().any(|text| text.content.0 == "10"));
        click(&mut window, center(layout.composite.notify_button));
        assert_eq!(window.state.notification_count, 11);
        assert_eq!(window.state.notification_pulse_generation, 11);
        let still_plus = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(still_plus.texts.iter().any(|text| text.content.0 == "+"));
        assert!(!still_plus.texts.iter().any(|text| text.content.0 == "11"));
        click(&mut window, center(layout.composite.dialog_button));
        assert_eq!(window.take_event(), Some(UiDebugEvent::OpenMessageDialog));
        assert!(window.take_event().is_none());
        window.apply_pointer_button(
            ElementState::Pressed,
            Some(center(layout.composite.dialog_button)),
            viewport(),
            1.0,
        );
        window.apply_pointer_button(
            ElementState::Released,
            Some([layout.window.max[0] + 12.0, layout.window.min[1]]),
            viewport(),
            1.0,
        );
        assert!(window.take_event().is_none());
    }
}
