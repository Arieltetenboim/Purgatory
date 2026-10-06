//! Dev-only UI DEBUG window. It consumes reusable controls; it does not invent them.

use winit::event::ElementState;

use crate::asset_runtime::AssetRuntime;
use crate::assets::ClientAssetLoader;
use crate::renderer::{
    PixelViewport, SpriteTextureId, TextAlignment, TextBlock, TextContent, TextStyle,
    UiTexturedQuad,
};
use crate::ui_controls::{
    PROGRESS_FILL_INSET, UiButton, UiButtonSkin, UiCheckbox, UiCheckboxSkin, UiPointerOutcome,
    UiRadioGroup, UiRadioOutcome, UiRadioSkin, UiSlider, UiSliderGeometry, UiSliderOutcome,
    UiSliderSkin, UiTabSkin, UiTabVisual, compose_progress_bar, slider_geometry, tab_visual,
};
use crate::ui_panel::{
    CloseButtonVisual, ProofPanelWindow, ScreenRect, UiTabs, WindowChromeLayout,
    compose_nine_slice_with_borders, compose_stretched_quad,
};
use crate::ui_v2::{
    UiV2Catalog, UiV2Image, UiV2NineSlice, load_ui_v2_catalog, load_ui_v2_image,
    load_ui_v2_nine_slice, load_ui_v2_state_family,
};

const UI_DEBUG_WINDOW_UNITS: [f32; 2] = [560.0, 420.0];
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
const BUTTON_HEIGHT: f32 = 26.0;
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
const PLANNED: &str = "Planned for the next UI DEBUG slice.";
const BASICS: usize = 0;
const VALUES: usize = 1;
const VALUE_LABEL_HEIGHT: f32 = 16.0;
const PROGRESS_TRACK_WIDTH: f32 = 320.0;
const PROGRESS_TRACK_HEIGHT: f32 = 24.0;
const SLIDER_TRACK_WIDTH: f32 = 320.0;
const SLIDER_TRACK_HEIGHT: f32 = 7.0;
const SLIDER_HANDLE_SIZE: f32 = 16.0;
const HP_INITIAL: f32 = 0.67;
const MP_SAMPLE: f32 = 0.40;
const EXP_SAMPLE: f32 = 0.82;

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
}

impl Default for UiDebugState {
    fn default() -> Self {
        Self {
            click_count: 0,
            icon_click_count: 0,
            checkbox: false,
            radio: UiDebugRadio::Medium,
            hp_value: HP_INITIAL,
        }
    }
}

pub(crate) struct UiDebugAssets {
    panel: UiV2NineSlice,
    header: UiV2NineSlice,
    tabbed_body: UiV2NineSlice,
    tabbed_rail: SpriteTextureId,
    close: [SpriteTextureId; 3],
    tabs: UiTabSkin,
    buttons: UiButtonSkin,
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
        let buttons = UiButtonSkin::from_family(family(
            &mut loader,
            &catalog,
            &[
                "button_normal",
                "button_hover",
                "button_pressed",
                "button_disabled",
            ],
        )?);
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
            tab_overlap,
        })
    }
}

pub(crate) struct UiDebugFrame {
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

    pub(crate) fn frame(
        &mut self,
        viewport: PixelViewport,
        pixels_per_unit: f32,
        cursor: Option<[f32; 2]>,
    ) -> Result<Option<UiDebugFrame>, String> {
        if self.is_visible() && self.tabs.selected_index() != VALUES {
            self.hp_slider.cancel();
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
        if self.tabs.selected_index() != VALUES {
            self.hp_slider.cancel();
            return self.chrome.pointer_moved(cursor, viewport, pixels_per_unit);
        }
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
        if self.hp_slider.is_dragging() {
            return self.route_slider_capture(state, cursor, &layout, pixels_per_unit);
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
                if self.tabs.selected_index() == BASICS && self.press_controls(&layout, cursor) {
                    return true;
                }
                if self.tabs.selected_index() == VALUES
                    && self.press_slider(&layout, cursor, pixels_per_unit)
                {
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
    }

    fn layout(
        &mut self,
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> Result<Option<DebugLayout>, String> {
        let Some(window) = self.chrome.placed_bounds(viewport, pixels_per_unit)? else {
            return Ok(None);
        };
        layout_debug(window, self.assets.tab_overlap, pixels_per_unit).map(Some)
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
        if self.tabs.pressed_index().is_some() {
            let handled = self.tabs.apply_pointer_button(
                ElementState::Released,
                cursor,
                tab_strip(&layout.tabs),
                PAGES.len(),
                0.0,
            );
            if self.tabs.selected_index() != VALUES {
                self.hp_slider.cancel();
            }
            return handled;
        }
        false
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

fn layout_debug(window: ScreenRect, tab_overlap: f32, scale: f32) -> Result<DebugLayout, String> {
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
        && values.slider_track.max[1] <= values.mp_label.min[1];
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
    } else {
        texts.push(body_text(
            PLANNED,
            content_origin(layout.inner, scale),
            layout.inner.width() - (TABBED_BORDER[0] + TABBED_BORDER[2] + PAGE_PAD * 2.0) * scale,
            INK,
        ));
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
    Ok(UiDebugFrame { skin_quads, texts })
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
    push_button(
        quads,
        layout.click_button,
        window.assets.buttons.texture(window.click_button.visual(
            cursor,
            layout.click_button,
            true,
        )),
    )?;
    texts.push(centered_label(
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
    push_button(
        quads,
        layout.disabled_button,
        window.assets.buttons.texture(window.disabled_button.visual(
            cursor,
            layout.disabled_button,
            false,
        )),
    )?;
    texts.push(centered_label(
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

fn heading(label: &str, anchor: [f32; 2]) -> TextBlock {
    TextBlock {
        content: TextContent(label.to_owned()),
        style: TextStyle::at_size(SECTION_FONT, INK, TextAlignment::Left),
        anchor,
        max_width: Some(180.0),
    }
}

fn body_text(label: &str, anchor: [f32; 2], max_width: f32, color: [f32; 4]) -> TextBlock {
    TextBlock {
        content: TextContent(label.to_owned()),
        style: TextStyle::at_size(BODY_FONT, color, TextAlignment::Left),
        anchor,
        max_width: Some(max_width.max(1.0)),
    }
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
            buttons: UiButtonSkin::from_family(family(20)),
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
        assert!(!text_has(&frame, PLANNED));
        assert!(!text_has(&frame, "Click Me"));
        let input = center(window.layout(viewport(), 1.0).unwrap().unwrap().tabs[2]);
        click(&mut window, input);
        assert_eq!(window.tabs.selected_index(), 2);
        let planned = window.frame(viewport(), 1.0, None).unwrap().unwrap();
        assert!(text_has(&planned, PLANNED));
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
            .find(|quad| quad.texture == window.assets.buttons.texture(UiButtonVisual::Normal))
            .unwrap();
        assert_eq!(quad_rect(click), layout.click_button);
        let disabled = frame
            .skin_quads
            .iter()
            .find(|quad| quad.texture == window.assets.buttons.texture(UiButtonVisual::Disabled))
            .unwrap();
        assert_eq!(quad_rect(disabled), layout.disabled_button);
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
        assert!(!text_has(&frame, PLANNED));
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
        for index in [2, 3, 4] {
            let tab = center(window.layout(viewport(), 1.0).unwrap().unwrap().tabs[index]);
            click(&mut window, tab);
            let page = window.frame(viewport(), 1.0, None).unwrap().unwrap();
            assert!(text_has(&page, PLANNED));
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
}
