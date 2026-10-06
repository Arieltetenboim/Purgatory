//! Dev-only UI DEBUG window. It consumes reusable controls; it does not invent them.

use winit::event::ElementState;

use crate::asset_runtime::AssetRuntime;
use crate::assets::ClientAssetLoader;
use crate::renderer::{
    PixelViewport, SpriteTextureId, TextAlignment, TextBlock, TextContent, TextStyle,
    UiTexturedQuad,
};
use crate::ui_controls::{
    UiButton, UiButtonSkin, UiCheckbox, UiCheckboxSkin, UiPointerOutcome, UiRadioGroup,
    UiRadioOutcome, UiRadioSkin, UiTabSkin, UiTabVisual, tab_visual,
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
}

impl Default for UiDebugState {
    fn default() -> Self {
        Self {
            click_count: 0,
            icon_click_count: 0,
            checkbox: false,
            radio: UiDebugRadio::Medium,
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
            return self.tabs.apply_pointer_button(
                ElementState::Released,
                cursor,
                tab_strip(&layout.tabs),
                PAGES.len(),
                0.0,
            );
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
        && radio_rows[RADIO_LABELS.len() - 1].max[1] <= limit_y + 0.05;
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
    })
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
    use crate::ui_controls::UiButtonVisual;

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
        assert!(text_has(&frame, PLANNED));
        assert!(!text_has(&frame, "Click Me"));
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
}
