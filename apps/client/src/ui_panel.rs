//! Authored panel/window chrome and the local production-UI proof.

use serde::Deserialize;
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::asset_runtime::AssetRuntime;
use crate::renderer::{
    PixelViewport, SpriteTextureId, TextAlignment, TextBlock, TextContent, TextStyle,
    UiTexturedRect,
};

const PANEL_PNG: &[u8] = include_bytes!("../../../Graphic/ui/panel.png");
const PANEL_METADATA: &str = include_str!("../../../Graphic/ui/panel.ui.json");
const PANEL_TEXTURE_FILE: &str = "panel.png";
const HEADER_PNG: &[u8] = include_bytes!("../../../Graphic/ui/header.png");
const HEADER_METADATA: &str = include_str!("../../../Graphic/ui/header.ui.json");
const HEADER_TEXTURE_FILE: &str = "header.png";
const CLOSE_BUTTON_PNG: &[u8] = include_bytes!("../../../Graphic/ui/BTN_quit.png");
const CLOSE_BUTTON_METADATA: &str = include_str!("../../../Graphic/ui/BTN_quit.ui.json");
const CLOSE_BUTTON_TEXTURE_FILE: &str = "BTN_quit.png";
const TAB_PNG: &[u8] = include_bytes!("../../../Graphic/ui/inventory_tab.png");
const TAB_METADATA: &str = include_str!("../../../Graphic/ui/inventory_tab.ui.json");
const TAB_TEXTURE_FILE: &str = "inventory_tab.png";
const NORMAL_SIZE_UNITS: [f32; 2] = [300.0, 440.0];
const TITLE_FONT_SIZE_UNITS: f32 = 15.0;
const TITLE_LEFT_INSET_UNITS: f32 = 12.0;
const TITLE_CONTROL_GAP_UNITS: f32 = 6.0;
const INVENTORY_TAB_TOP_GAP_UNITS: f32 = 4.0;
const INVENTORY_TAB_LABELS: [&str; 5] = ["Equip", "Use", "Mats", "Tools", "Misc"];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ProofPanelMode {
    #[default]
    Hidden,
    Normal,
    Double,
}

impl ProofPanelMode {
    fn logical_size(self) -> Option<[f32; 2]> {
        match self {
            Self::Hidden => None,
            Self::Normal => Some(NORMAL_SIZE_UNITS),
            Self::Double => Some([NORMAL_SIZE_UNITS[0] * 2.0, NORMAL_SIZE_UNITS[1] * 2.0]),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct SourceInsets {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct DestinationBorders {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl DestinationBorders {
    fn scaled(self, scale: f32) -> Self {
        Self {
            left: self.left * scale,
            right: self.right * scale,
            top: self.top * scale,
            bottom: self.bottom * scale,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct HorizontalInsetsPx {
    left: u32,
    right: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct HorizontalCapsUnits {
    left: f32,
    right: f32,
}

impl HorizontalCapsUnits {
    fn scaled(self, scale: f32) -> Self {
        Self {
            left: self.left * scale,
            right: self.right * scale,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct HeaderInsetsUnits {
    left: f32,
    right: f32,
    top: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct SourceRectPx {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl SourceRectPx {
    fn uv_bounds(self, source_size_px: [u32; 2]) -> ([f32; 2], [f32; 2]) {
        let width = source_size_px[0] as f32;
        let height = source_size_px[1] as f32;
        (
            [self.x as f32 / width, self.y as f32 / height],
            [
                (self.x + self.width) as f32 / width,
                (self.y + self.height) as f32 / height,
            ],
        )
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UiPanelMetadata {
    schema_version: u32,
    id: String,
    texture: String,
    slice_px: SourceInsets,
    border_units: DestinationBorders,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UiHeaderMetadata {
    schema_version: u32,
    id: String,
    texture: String,
    slice_px: HorizontalInsetsPx,
    cap_units: HorizontalCapsUnits,
    height_units: f32,
    inset_units: HeaderInsetsUnits,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct CloseButtonStates {
    normal: SourceRectPx,
    hover: SourceRectPx,
    pressed: SourceRectPx,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UiCloseButtonMetadata {
    schema_version: u32,
    id: String,
    texture: String,
    states: CloseButtonStates,
    size_units: [f32; 2],
    right_inset_units: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct TabStates {
    normal: SourceRectPx,
    selected: SourceRectPx,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UiTabMetadata {
    schema_version: u32,
    id: String,
    texture: String,
    states: TabStates,
    slice_px: HorizontalInsetsPx,
    cap_units: HorizontalCapsUnits,
    height_units: f32,
    font_size_units: f32,
    horizontal_text_padding_units: f32,
}

#[derive(Clone, Copy, Debug)]
struct PanelAsset {
    texture: SpriteTextureId,
    source_size_px: [u32; 2],
    slice_px: SourceInsets,
    border_units: DestinationBorders,
}

#[derive(Clone, Copy, Debug)]
struct HeaderAsset {
    texture: SpriteTextureId,
    source_size_px: [u32; 2],
    slice_px: HorizontalInsetsPx,
    cap_units: HorizontalCapsUnits,
    height_units: f32,
    inset_units: HeaderInsetsUnits,
}

#[derive(Clone, Copy, Debug)]
struct CloseButtonAsset {
    texture: SpriteTextureId,
    source_size_px: [u32; 2],
    states: CloseButtonStates,
    size_units: [f32; 2],
    right_inset_units: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiTabAssets {
    texture: SpriteTextureId,
    source_size_px: [u32; 2],
    states: TabStates,
    slice_px: HorizontalInsetsPx,
    cap_units: HorizontalCapsUnits,
    height_units: f32,
    font_size_units: f32,
    horizontal_text_padding_units: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiWindowAssets {
    panel: PanelAsset,
    header: HeaderAsset,
    close_button: CloseButtonAsset,
}

impl UiWindowAssets {
    pub(crate) fn load_embedded(assets: &mut AssetRuntime) -> Result<Self, String> {
        let panel_metadata: UiPanelMetadata = serde_json::from_str(PANEL_METADATA)
            .map_err(|error| format!("parse UI panel metadata: {error}"))?;
        let (panel_texture, panel_source_size) = register_metadata_texture(
            assets,
            &panel_metadata.id,
            &panel_metadata.texture,
            PANEL_TEXTURE_FILE,
            PANEL_PNG,
            "panel",
        )?;
        validate_panel_metadata(&panel_metadata, panel_source_size)?;
        let panel = PanelAsset {
            texture: panel_texture,
            source_size_px: panel_source_size,
            slice_px: panel_metadata.slice_px,
            border_units: panel_metadata.border_units,
        };

        let header_metadata: UiHeaderMetadata = serde_json::from_str(HEADER_METADATA)
            .map_err(|error| format!("parse UI header metadata: {error}"))?;
        let (header_texture, header_source_size) = register_metadata_texture(
            assets,
            &header_metadata.id,
            &header_metadata.texture,
            HEADER_TEXTURE_FILE,
            HEADER_PNG,
            "header",
        )?;
        validate_header_metadata(&header_metadata, header_source_size)?;
        let header = HeaderAsset {
            texture: header_texture,
            source_size_px: header_source_size,
            slice_px: header_metadata.slice_px,
            cap_units: header_metadata.cap_units,
            height_units: header_metadata.height_units,
            inset_units: header_metadata.inset_units,
        };

        let close_metadata: UiCloseButtonMetadata = serde_json::from_str(CLOSE_BUTTON_METADATA)
            .map_err(|error| format!("parse UI close-button metadata: {error}"))?;
        let (close_texture, close_source_size) = register_metadata_texture(
            assets,
            &close_metadata.id,
            &close_metadata.texture,
            CLOSE_BUTTON_TEXTURE_FILE,
            CLOSE_BUTTON_PNG,
            "close button",
        )?;
        validate_close_button_metadata(&close_metadata, close_source_size)?;
        let close_button = CloseButtonAsset {
            texture: close_texture,
            source_size_px: close_source_size,
            states: close_metadata.states,
            size_units: close_metadata.size_units,
            right_inset_units: close_metadata.right_inset_units,
        };

        Ok(Self {
            panel,
            header,
            close_button,
        })
    }

    pub(crate) fn proof_frame(
        self,
        window: &mut ProofPanelWindow,
        title: &str,
        viewport: PixelViewport,
        pixels_per_unit: f32,
        cursor: Option<[f32; 2]>,
    ) -> Result<Option<UiWindowFrame>, String> {
        let Some(layout) = self.layout(window, viewport, pixels_per_unit)? else {
            return Ok(None);
        };
        let mut textured_rects = assemble_nine_slice(
            layout.window,
            self.panel.texture,
            self.panel.source_size_px,
            self.panel.slice_px,
            self.panel.border_units.scaled(pixels_per_unit),
            [1.0; 4],
        )?;
        textured_rects.extend(assemble_horizontal_three_slice(
            layout.header,
            self.header.texture,
            self.header.source_size_px,
            self.header.slice_px,
            self.header.cap_units.scaled(pixels_per_unit),
            [1.0; 4],
        )?);

        let source = match window.close_button_visual(cursor, layout.close_button) {
            CloseButtonVisual::Normal => self.close_button.states.normal,
            CloseButtonVisual::Hover => self.close_button.states.hover,
            CloseButtonVisual::Pressed => self.close_button.states.pressed,
        };
        let (uv_min, uv_max) = source.uv_bounds(self.close_button.source_size_px);
        textured_rects.push(UiTexturedRect {
            min: layout.close_button.min,
            max: layout.close_button.max,
            texture: self.close_button.texture,
            uv_min,
            uv_max,
            tint: [1.0; 4],
        });

        let title_font_size = TITLE_FONT_SIZE_UNITS * pixels_per_unit;
        let title_anchor = [
            layout.header.min[0] + TITLE_LEFT_INSET_UNITS * pixels_per_unit,
            layout.header.min[1]
                + ((self.header.height_units - TITLE_FONT_SIZE_UNITS) * 0.5).max(0.0)
                    * pixels_per_unit,
        ];
        let title_max_width = (layout.close_button.min[0]
            - TITLE_CONTROL_GAP_UNITS * pixels_per_unit
            - title_anchor[0])
            .max(1.0);
        Ok(Some(UiWindowFrame {
            textured_rects,
            title: TextBlock {
                content: TextContent(title.to_owned()),
                style: TextStyle {
                    font_size: title_font_size,
                    color: [0.08, 0.11, 0.16, 1.0],
                    alignment: TextAlignment::Left,
                },
                anchor: title_anchor,
                max_width: Some(title_max_width),
            },
        }))
    }

    fn layout(
        self,
        window: &mut ProofPanelWindow,
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> Result<Option<UiWindowLayout>, String> {
        validate_pixels_per_unit(pixels_per_unit)?;
        let Some(window_size_units) = window.mode.logical_size() else {
            return Ok(None);
        };
        let viewport_size_units = [
            viewport.width as f32 / pixels_per_unit,
            viewport.height as f32 / pixels_per_unit,
        ];
        let centered = [
            ((viewport_size_units[0] - window_size_units[0]) * 0.5).max(0.0),
            ((viewport_size_units[1] - window_size_units[1]) * 0.5).max(0.0),
        ];
        let top_left_units = window.top_left_units.get_or_insert(centered);
        *top_left_units = clamp_top_left(*top_left_units, window_size_units, viewport_size_units);

        let window_min = [
            viewport.x as f32 + top_left_units[0] * pixels_per_unit,
            viewport.y as f32 + top_left_units[1] * pixels_per_unit,
        ];
        let window_max = [
            window_min[0] + window_size_units[0] * pixels_per_unit,
            window_min[1] + window_size_units[1] * pixels_per_unit,
        ];
        let header = ScreenRect {
            min: [
                window_min[0] + self.header.inset_units.left * pixels_per_unit,
                window_min[1] + self.header.inset_units.top * pixels_per_unit,
            ],
            max: [
                window_max[0] - self.header.inset_units.right * pixels_per_unit,
                window_min[1]
                    + (self.header.inset_units.top + self.header.height_units) * pixels_per_unit,
            ],
        };
        let button_size = [
            self.close_button.size_units[0] * pixels_per_unit,
            self.close_button.size_units[1] * pixels_per_unit,
        ];
        let close_max_x = header.max[0] - self.close_button.right_inset_units * pixels_per_unit;
        let close_min_y = header.min[1] + ((header.height() - button_size[1]) * 0.5).max(0.0);
        let close_button = ScreenRect {
            min: [close_max_x - button_size[0], close_min_y],
            max: [close_max_x, close_min_y + button_size[1]],
        };
        Ok(Some(UiWindowLayout {
            window: ScreenRect {
                min: window_min,
                max: window_max,
            },
            header,
            close_button,
        }))
    }
}

impl UiTabAssets {
    pub(crate) fn load_embedded(assets: &mut AssetRuntime) -> Result<Self, String> {
        let metadata: UiTabMetadata = serde_json::from_str(TAB_METADATA)
            .map_err(|error| format!("parse UI tab metadata: {error}"))?;
        let (texture, source_size_px) = register_metadata_texture(
            assets,
            &metadata.id,
            &metadata.texture,
            TAB_TEXTURE_FILE,
            TAB_PNG,
            "tab",
        )?;
        validate_tab_metadata(&metadata, source_size_px)?;
        Ok(Self {
            texture,
            source_size_px,
            states: metadata.states,
            slice_px: metadata.slice_px,
            cap_units: metadata.cap_units,
            height_units: metadata.height_units,
            font_size_units: metadata.font_size_units,
            horizontal_text_padding_units: metadata.horizontal_text_padding_units,
        })
    }

    pub(crate) fn frame(
        self,
        tabs: &UiTabs,
        labels: &[&str],
        bounds: ScreenRect,
        pixels_per_unit: f32,
        cursor: Option<[f32; 2]>,
    ) -> Result<UiTabsFrame, String> {
        validate_pixels_per_unit(pixels_per_unit)?;
        if labels.is_empty() {
            return Err("UI tabs require at least one label".to_string());
        }
        let mut textured_rects = Vec::with_capacity(labels.len() * 3);
        let mut texts = Vec::with_capacity(labels.len());
        for (index, label) in labels.iter().enumerate() {
            let hit_rect = tab_rect(bounds, index, labels.len());
            let hovered = cursor.is_some_and(|cursor| hit_rect.contains(cursor));
            let pressed = hovered && tabs.pressed_index == Some(index);
            let selected = tabs.selected_index() == index;
            let source = if selected {
                self.states.selected
            } else {
                self.states.normal
            };
            let mut draw_rect = hit_rect;
            if pressed {
                draw_rect.min[1] += pixels_per_unit;
                draw_rect.max[1] += pixels_per_unit;
            }
            let tint = if selected || hovered {
                [1.0; 4]
            } else {
                [0.92, 0.94, 0.98, 1.0]
            };
            textured_rects.extend(assemble_horizontal_three_slice_region(
                draw_rect,
                self.texture,
                self.source_size_px,
                source,
                self.slice_px,
                self.cap_units.scaled(pixels_per_unit),
                tint,
            )?);

            let font_size = self.font_size_units * pixels_per_unit;
            texts.push(TextBlock {
                content: TextContent((*label).to_string()),
                style: TextStyle {
                    font_size,
                    color: [0.08, 0.11, 0.16, 1.0],
                    alignment: TextAlignment::Center,
                },
                anchor: [
                    (draw_rect.min[0] + draw_rect.max[0]) * 0.5,
                    draw_rect.min[1] + ((draw_rect.height() - font_size) * 0.5).max(0.0),
                ],
                max_width: Some(
                    (draw_rect.width()
                        - self.horizontal_text_padding_units * 2.0 * pixels_per_unit)
                        .max(1.0),
                ),
            });
        }
        Ok(UiTabsFrame {
            textured_rects,
            texts,
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct UiTabs {
    selected_index: usize,
    pressed_index: Option<usize>,
}

impl UiTabs {
    pub(crate) fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub(crate) fn apply_pointer_button(
        &mut self,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        bounds: ScreenRect,
        tab_count: usize,
    ) -> bool {
        match state {
            ElementState::Pressed => {
                self.pressed_index = cursor.and_then(|cursor| tab_at(bounds, tab_count, cursor));
                self.pressed_index.is_some()
            }
            ElementState::Released => {
                let Some(pressed) = self.pressed_index.take() else {
                    return false;
                };
                if cursor.and_then(|cursor| tab_at(bounds, tab_count, cursor)) == Some(pressed) {
                    self.selected_index = pressed;
                }
                true
            }
        }
    }

    pub(crate) fn cancel_pointer_interaction(&mut self) {
        self.pressed_index = None;
    }
}

pub(crate) struct UiTabsFrame {
    pub(crate) textured_rects: Vec<UiTexturedRect>,
    pub(crate) texts: Vec<TextBlock>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct InventoryWindow {
    chrome: ProofPanelWindow,
    tabs: UiTabs,
}

impl InventoryWindow {
    pub(crate) fn apply_key(
        &mut self,
        physical_key: PhysicalKey,
        state: ElementState,
        repeat: bool,
    ) -> bool {
        if physical_key != PhysicalKey::Code(KeyCode::KeyI) {
            return false;
        }
        self.chrome.apply_key(physical_key, state, repeat)
    }

    pub(crate) fn frame(
        &mut self,
        window_assets: UiWindowAssets,
        tab_assets: UiTabAssets,
        viewport: PixelViewport,
        pixels_per_unit: f32,
        cursor: Option<[f32; 2]>,
    ) -> Result<Option<InventoryWindowFrame>, String> {
        let Some(layout) = window_assets.layout(&mut self.chrome, viewport, pixels_per_unit)?
        else {
            return Ok(None);
        };
        let Some(window_frame) = window_assets.proof_frame(
            &mut self.chrome,
            "Item Inventory",
            viewport,
            pixels_per_unit,
            cursor,
        )?
        else {
            return Ok(None);
        };
        let tab_bounds = inventory_tab_bounds(window_assets, tab_assets, layout, pixels_per_unit)?;
        let tab_frame = tab_assets.frame(
            &self.tabs,
            &INVENTORY_TAB_LABELS,
            tab_bounds,
            pixels_per_unit,
            cursor,
        )?;
        let mut textured_rects = window_frame.textured_rects;
        textured_rects.extend(tab_frame.textured_rects);
        let mut texts = Vec::with_capacity(1 + tab_frame.texts.len());
        texts.push(window_frame.title);
        texts.extend(tab_frame.texts);
        Ok(Some(InventoryWindowFrame {
            textured_rects,
            texts,
        }))
    }

    pub(crate) fn pointer_moved(
        &mut self,
        cursor: [f32; 2],
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        self.chrome.pointer_moved(cursor, viewport, pixels_per_unit)
    }

    pub(crate) fn apply_pointer_button(
        &mut self,
        window_assets: UiWindowAssets,
        tab_assets: UiTabAssets,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        let Ok(Some(layout)) = window_assets.layout(&mut self.chrome, viewport, pixels_per_unit)
        else {
            self.cancel_pointer_interaction();
            return false;
        };
        if let Ok(tab_bounds) =
            inventory_tab_bounds(window_assets, tab_assets, layout, pixels_per_unit)
            && self
                .tabs
                .apply_pointer_button(state, cursor, tab_bounds, INVENTORY_TAB_LABELS.len())
        {
            return true;
        }
        self.chrome
            .apply_pointer_button(window_assets, state, cursor, viewport, pixels_per_unit)
    }

    pub(crate) fn cancel_pointer_interaction(&mut self) {
        self.chrome.cancel_pointer_interaction();
        self.tabs.cancel_pointer_interaction();
    }
}

pub(crate) struct InventoryWindowFrame {
    pub(crate) textured_rects: Vec<UiTexturedRect>,
    pub(crate) texts: Vec<TextBlock>,
}

fn inventory_tab_bounds(
    window_assets: UiWindowAssets,
    tab_assets: UiTabAssets,
    layout: UiWindowLayout,
    pixels_per_unit: f32,
) -> Result<ScreenRect, String> {
    validate_pixels_per_unit(pixels_per_unit)?;
    let min_y = layout.header.max[1] + INVENTORY_TAB_TOP_GAP_UNITS * pixels_per_unit;
    let bounds = ScreenRect {
        min: [
            layout.window.min[0] + window_assets.panel.border_units.left * pixels_per_unit,
            min_y,
        ],
        max: [
            layout.window.max[0] - window_assets.panel.border_units.right * pixels_per_unit,
            min_y + tab_assets.height_units * pixels_per_unit,
        ],
    };
    if bounds.width()
        <= (tab_assets.cap_units.left + tab_assets.cap_units.right)
            * INVENTORY_TAB_LABELS.len() as f32
            * pixels_per_unit
        || bounds.max[1]
            >= layout.window.max[1] - window_assets.panel.border_units.bottom * pixels_per_unit
    {
        return Err("inventory window is too small for its tab strip".to_string());
    }
    Ok(bounds)
}

fn tab_rect(bounds: ScreenRect, index: usize, count: usize) -> ScreenRect {
    let width = bounds.width() / count as f32;
    ScreenRect {
        min: [bounds.min[0] + index as f32 * width, bounds.min[1]],
        max: [
            if index + 1 == count {
                bounds.max[0]
            } else {
                bounds.min[0] + (index + 1) as f32 * width
            },
            bounds.max[1],
        ],
    }
}

fn tab_at(bounds: ScreenRect, count: usize, cursor: [f32; 2]) -> Option<usize> {
    if count == 0 || !bounds.contains(cursor) {
        return None;
    }
    let index = ((cursor[0] - bounds.min[0]) / (bounds.width() / count as f32)).floor();
    Some((index as usize).min(count - 1))
}

fn register_metadata_texture(
    assets: &mut AssetRuntime,
    id: &str,
    authored_texture: &str,
    expected_texture: &str,
    png: &[u8],
    kind: &str,
) -> Result<(SpriteTextureId, [u32; 2]), String> {
    if authored_texture != expected_texture {
        return Err(format!(
            "UI {kind} {id} names texture {authored_texture:?}; expected {expected_texture:?}"
        ));
    }
    let texture = assets.register_png(id, png)?;
    let source_size = assets
        .resource(texture)
        .map(|resource| [resource.image.width(), resource.image.height()])
        .ok_or_else(|| format!("UI {kind} {id} texture registration was lost"))?;
    Ok((texture, source_size))
}

fn validate_panel_metadata(
    metadata: &UiPanelMetadata,
    source_size_px: [u32; 2],
) -> Result<(), String> {
    validate_schema_and_id(metadata.schema_version, &metadata.id, "panel")?;
    if source_size_px.contains(&0)
        || metadata
            .slice_px
            .left
            .saturating_add(metadata.slice_px.right)
            >= source_size_px[0]
        || metadata
            .slice_px
            .top
            .saturating_add(metadata.slice_px.bottom)
            >= source_size_px[1]
    {
        return Err(format!(
            "UI panel {} slice geometry {:?} is outside texture {}x{}",
            metadata.id, metadata.slice_px, source_size_px[0], source_size_px[1]
        ));
    }
    let borders = metadata.border_units;
    if ![borders.left, borders.right, borders.top, borders.bottom]
        .into_iter()
        .all(finite_positive)
    {
        return Err(format!(
            "UI panel {} border_units must be finite and positive",
            metadata.id
        ));
    }
    Ok(())
}

fn validate_header_metadata(
    metadata: &UiHeaderMetadata,
    source_size_px: [u32; 2],
) -> Result<(), String> {
    validate_schema_and_id(metadata.schema_version, &metadata.id, "header")?;
    if source_size_px.contains(&0)
        || metadata
            .slice_px
            .left
            .saturating_add(metadata.slice_px.right)
            >= source_size_px[0]
    {
        return Err(format!(
            "UI header {} slice geometry {:?} is outside texture {}x{}",
            metadata.id, metadata.slice_px, source_size_px[0], source_size_px[1]
        ));
    }
    if ![
        metadata.cap_units.left,
        metadata.cap_units.right,
        metadata.height_units,
    ]
    .into_iter()
    .all(finite_positive)
        || ![
            metadata.inset_units.left,
            metadata.inset_units.right,
            metadata.inset_units.top,
        ]
        .into_iter()
        .all(finite_non_negative)
    {
        return Err(format!(
            "UI header {} units must be finite with positive caps/height and non-negative insets",
            metadata.id
        ));
    }
    Ok(())
}

fn validate_close_button_metadata(
    metadata: &UiCloseButtonMetadata,
    source_size_px: [u32; 2],
) -> Result<(), String> {
    validate_schema_and_id(metadata.schema_version, &metadata.id, "close button")?;
    if source_size_px.contains(&0)
        || ![
            metadata.states.normal,
            metadata.states.hover,
            metadata.states.pressed,
        ]
        .into_iter()
        .all(|rect| source_rect_fits(rect, source_size_px))
    {
        return Err(format!(
            "UI close button {} contains a state outside texture {}x{}",
            metadata.id, source_size_px[0], source_size_px[1]
        ));
    }
    if !metadata.size_units.into_iter().all(finite_positive)
        || !finite_non_negative(metadata.right_inset_units)
    {
        return Err(format!(
            "UI close button {} size must be positive and inset non-negative",
            metadata.id
        ));
    }
    Ok(())
}

fn validate_tab_metadata(metadata: &UiTabMetadata, source_size_px: [u32; 2]) -> Result<(), String> {
    validate_schema_and_id(metadata.schema_version, &metadata.id, "tab")?;
    let states = [metadata.states.normal, metadata.states.selected];
    if source_size_px.contains(&0)
        || !states
            .into_iter()
            .all(|rect| source_rect_fits(rect, source_size_px))
        || metadata.states.normal.width != metadata.states.selected.width
        || metadata.states.normal.height != metadata.states.selected.height
        || metadata
            .slice_px
            .left
            .saturating_add(metadata.slice_px.right)
            >= metadata.states.normal.width
    {
        return Err(format!(
            "UI tab {} contains invalid state or slice geometry for texture {}x{}",
            metadata.id, source_size_px[0], source_size_px[1]
        ));
    }
    if ![
        metadata.cap_units.left,
        metadata.cap_units.right,
        metadata.height_units,
        metadata.font_size_units,
    ]
    .into_iter()
    .all(finite_positive)
        || !finite_non_negative(metadata.horizontal_text_padding_units)
    {
        return Err(format!(
            "UI tab {} units must be finite with positive caps/height/font and non-negative padding",
            metadata.id
        ));
    }
    Ok(())
}

fn validate_schema_and_id(schema_version: u32, id: &str, kind: &str) -> Result<(), String> {
    if schema_version != 1 {
        return Err(format!(
            "UI {kind} {id} has unsupported schema_version {schema_version}"
        ));
    }
    if id.trim().is_empty() {
        return Err(format!("UI {kind} metadata id must not be empty"));
    }
    Ok(())
}

fn source_rect_fits(rect: SourceRectPx, source_size_px: [u32; 2]) -> bool {
    rect.width > 0
        && rect.height > 0
        && rect
            .x
            .checked_add(rect.width)
            .is_some_and(|x| x <= source_size_px[0])
        && rect
            .y
            .checked_add(rect.height)
            .is_some_and(|y| y <= source_size_px[1])
}

fn finite_positive(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

fn finite_non_negative(value: f32) -> bool {
    value.is_finite() && value >= 0.0
}

fn validate_pixels_per_unit(pixels_per_unit: f32) -> Result<(), String> {
    if finite_positive(pixels_per_unit) {
        Ok(())
    } else {
        Err("UI window pixels-per-unit must be finite and positive".to_string())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum PointerInteraction {
    #[default]
    None,
    Drag {
        grab_offset_units: [f32; 2],
    },
    CloseArmed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloseButtonVisual {
    Normal,
    Hover,
    Pressed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ProofPanelWindow {
    mode: ProofPanelMode,
    /// Relative to the gameplay viewport in logical UI units.
    top_left_units: Option<[f32; 2]>,
    interaction: PointerInteraction,
}

impl ProofPanelWindow {
    /// Applies one physical-key event. Returns true only when this proof owns it.
    pub(crate) fn apply_key(
        &mut self,
        physical_key: PhysicalKey,
        state: ElementState,
        repeat: bool,
    ) -> bool {
        if state != ElementState::Pressed || repeat {
            return false;
        }
        let PhysicalKey::Code(code) = physical_key else {
            return false;
        };
        self.mode = match code {
            KeyCode::KeyI if self.mode == ProofPanelMode::Normal => ProofPanelMode::Hidden,
            KeyCode::KeyI => ProofPanelMode::Normal,
            KeyCode::KeyO if self.mode == ProofPanelMode::Double => ProofPanelMode::Hidden,
            KeyCode::KeyO => ProofPanelMode::Double,
            _ => return false,
        };
        // Keyboard open/size changes begin centered, matching the original
        // proof. Drag state only applies to the current visible session.
        self.top_left_units = None;
        self.interaction = PointerInteraction::None;
        true
    }

    pub(crate) fn pointer_moved(
        &mut self,
        cursor: [f32; 2],
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        let PointerInteraction::Drag { grab_offset_units } = self.interaction else {
            return false;
        };
        let Some(window_size_units) = self.mode.logical_size() else {
            self.interaction = PointerInteraction::None;
            return false;
        };
        if validate_pixels_per_unit(pixels_per_unit).is_err() {
            return false;
        }
        let viewport_size_units = [
            viewport.width as f32 / pixels_per_unit,
            viewport.height as f32 / pixels_per_unit,
        ];
        let cursor_units = [
            (cursor[0] - viewport.x as f32) / pixels_per_unit,
            (cursor[1] - viewport.y as f32) / pixels_per_unit,
        ];
        let next = clamp_top_left(
            [
                cursor_units[0] - grab_offset_units[0],
                cursor_units[1] - grab_offset_units[1],
            ],
            window_size_units,
            viewport_size_units,
        );
        let changed = self.top_left_units != Some(next);
        self.top_left_units = Some(next);
        changed
    }

    pub(crate) fn apply_pointer_button(
        &mut self,
        assets: UiWindowAssets,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        let Ok(Some(layout)) = assets.layout(self, viewport, pixels_per_unit) else {
            self.interaction = PointerInteraction::None;
            return false;
        };
        match state {
            ElementState::Pressed => {
                let Some(cursor) = cursor else {
                    return false;
                };
                if layout.close_button.contains(cursor) {
                    self.interaction = PointerInteraction::CloseArmed;
                    return true;
                }
                if layout.header.contains(cursor) {
                    self.interaction = PointerInteraction::Drag {
                        grab_offset_units: [
                            (cursor[0] - layout.window.min[0]) / pixels_per_unit,
                            (cursor[1] - layout.window.min[1]) / pixels_per_unit,
                        ],
                    };
                    return true;
                }
                self.interaction = PointerInteraction::None;
                layout.window.contains(cursor)
            }
            ElementState::Released => {
                let interaction = std::mem::take(&mut self.interaction);
                match interaction {
                    PointerInteraction::None => false,
                    PointerInteraction::Drag { .. } => true,
                    PointerInteraction::CloseArmed => {
                        if cursor.is_some_and(|cursor| layout.close_button.contains(cursor)) {
                            self.mode = ProofPanelMode::Hidden;
                        }
                        true
                    }
                }
            }
        }
    }

    pub(crate) fn cancel_pointer_interaction(&mut self) {
        self.interaction = PointerInteraction::None;
    }

    fn close_button_visual(
        self,
        cursor: Option<[f32; 2]>,
        close_button: ScreenRect,
    ) -> CloseButtonVisual {
        let hovered = cursor.is_some_and(|cursor| close_button.contains(cursor));
        if hovered && self.interaction == PointerInteraction::CloseArmed {
            CloseButtonVisual::Pressed
        } else if hovered {
            CloseButtonVisual::Hover
        } else {
            CloseButtonVisual::Normal
        }
    }
}

pub(crate) struct UiWindowFrame {
    pub(crate) textured_rects: Vec<UiTexturedRect>,
    pub(crate) title: TextBlock,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct UiWindowLayout {
    window: ScreenRect,
    header: ScreenRect,
    close_button: ScreenRect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScreenRect {
    pub(crate) min: [f32; 2],
    pub(crate) max: [f32; 2],
}

impl ScreenRect {
    fn width(self) -> f32 {
        self.max[0] - self.min[0]
    }

    fn height(self) -> f32 {
        self.max[1] - self.min[1]
    }

    fn contains(self, point: [f32; 2]) -> bool {
        point[0] >= self.min[0]
            && point[0] <= self.max[0]
            && point[1] >= self.min[1]
            && point[1] <= self.max[1]
    }
}

fn clamp_top_left(position: [f32; 2], window_size: [f32; 2], viewport_size: [f32; 2]) -> [f32; 2] {
    let max = [
        (viewport_size[0] - window_size[0]).max(0.0),
        (viewport_size[1] - window_size[1]).max(0.0),
    ];
    [
        position[0].clamp(0.0, max[0]),
        position[1].clamp(0.0, max[1]),
    ]
}

fn assemble_nine_slice(
    destination: ScreenRect,
    texture: SpriteTextureId,
    source_size_px: [u32; 2],
    source: SourceInsets,
    borders: DestinationBorders,
    tint: [f32; 4],
) -> Result<Vec<UiTexturedRect>, String> {
    if !destination
        .min
        .into_iter()
        .chain(destination.max)
        .all(f32::is_finite)
        || destination.width() <= borders.left + borders.right
        || destination.height() <= borders.top + borders.bottom
    {
        return Err(
            "UI panel target is invalid or smaller than its destination borders".to_string(),
        );
    }

    let source_x = [
        0.0,
        source.left as f32,
        (source_size_px[0] - source.right) as f32,
        source_size_px[0] as f32,
    ];
    let source_y = [
        0.0,
        source.top as f32,
        (source_size_px[1] - source.bottom) as f32,
        source_size_px[1] as f32,
    ];
    let destination_x = [
        destination.min[0],
        destination.min[0] + borders.left,
        destination.max[0] - borders.right,
        destination.max[0],
    ];
    let destination_y = [
        destination.min[1],
        destination.min[1] + borders.top,
        destination.max[1] - borders.bottom,
        destination.max[1],
    ];
    let source_width = source_size_px[0] as f32;
    let source_height = source_size_px[1] as f32;

    let mut regions = Vec::with_capacity(9);
    for row in 0..3 {
        for column in 0..3 {
            regions.push(UiTexturedRect {
                min: [destination_x[column], destination_y[row]],
                max: [destination_x[column + 1], destination_y[row + 1]],
                texture,
                uv_min: [
                    source_x[column] / source_width,
                    source_y[row] / source_height,
                ],
                uv_max: [
                    source_x[column + 1] / source_width,
                    source_y[row + 1] / source_height,
                ],
                tint,
            });
        }
    }
    Ok(regions)
}

fn assemble_horizontal_three_slice(
    destination: ScreenRect,
    texture: SpriteTextureId,
    source_size_px: [u32; 2],
    source: HorizontalInsetsPx,
    caps: HorizontalCapsUnits,
    tint: [f32; 4],
) -> Result<Vec<UiTexturedRect>, String> {
    assemble_horizontal_three_slice_region(
        destination,
        texture,
        source_size_px,
        SourceRectPx {
            x: 0,
            y: 0,
            width: source_size_px[0],
            height: source_size_px[1],
        },
        source,
        caps,
        tint,
    )
}

fn assemble_horizontal_three_slice_region(
    destination: ScreenRect,
    texture: SpriteTextureId,
    texture_size_px: [u32; 2],
    source_rect: SourceRectPx,
    source: HorizontalInsetsPx,
    caps: HorizontalCapsUnits,
    tint: [f32; 4],
) -> Result<Vec<UiTexturedRect>, String> {
    if !destination
        .min
        .into_iter()
        .chain(destination.max)
        .all(f32::is_finite)
        || destination.width() <= caps.left + caps.right
        || destination.height() <= 0.0
        || !source_rect_fits(source_rect, texture_size_px)
        || source.left.saturating_add(source.right) >= source_rect.width
    {
        return Err("UI horizontal 3-slice target or source geometry is invalid".to_string());
    }
    let source_x = [
        source_rect.x as f32,
        (source_rect.x + source.left) as f32,
        (source_rect.x + source_rect.width - source.right) as f32,
        (source_rect.x + source_rect.width) as f32,
    ];
    let destination_x = [
        destination.min[0],
        destination.min[0] + caps.left,
        destination.max[0] - caps.right,
        destination.max[0],
    ];
    let source_width = texture_size_px[0] as f32;
    let source_height = texture_size_px[1] as f32;
    let source_y = [
        source_rect.y as f32 / source_height,
        (source_rect.y + source_rect.height) as f32 / source_height,
    ];
    let mut regions = Vec::with_capacity(3);
    for column in 0..3 {
        regions.push(UiTexturedRect {
            min: [destination_x[column], destination.min[1]],
            max: [destination_x[column + 1], destination.max[1]],
            texture,
            uv_min: [source_x[column] / source_width, source_y[0]],
            uv_max: [source_x[column + 1] / source_width, source_y[1]],
            tint,
        });
    }
    Ok(regions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn embedded_assets() -> UiWindowAssets {
        UiWindowAssets::load_embedded(&mut AssetRuntime::new()).unwrap()
    }

    fn embedded_tab_assets() -> UiTabAssets {
        UiTabAssets::load_embedded(&mut AssetRuntime::new()).unwrap()
    }

    fn viewport() -> PixelViewport {
        PixelViewport {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        }
    }

    fn normal_window() -> ProofPanelWindow {
        let mut window = ProofPanelWindow::default();
        assert!(window.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        window
    }

    #[test]
    fn embedded_metadata_parses_and_validates_against_decoded_textures() {
        let mut runtime = AssetRuntime::new();
        let assets = UiWindowAssets::load_embedded(&mut runtime).unwrap();
        assert_eq!(assets.panel.source_size_px, [1254, 1254]);
        assert_eq!(assets.header.source_size_px, [2072, 139]);
        assert_eq!(assets.close_button.source_size_px, [1500, 500]);
        assert_eq!(assets.panel.slice_px.left, 128);
        assert_eq!(assets.header.slice_px.left, 140);
        assert_eq!(runtime.resource_count(), 3);
        assert_eq!(
            runtime
                .resource(assets.close_button.texture)
                .unwrap()
                .image
                .get_pixel(0, 0)
                .0[3],
            0
        );
    }

    #[test]
    fn embedded_tab_metadata_parses_and_matches_two_state_sheet() {
        let mut runtime = AssetRuntime::new();
        let assets = UiTabAssets::load_embedded(&mut runtime).unwrap();
        assert_eq!(assets.source_size_px, [1632, 220]);
        assert_eq!(assets.states.normal.width, 816);
        assert_eq!(assets.states.selected.x, 816);
        assert_eq!(assets.slice_px.left, 96);
        assert_eq!(runtime.resource_count(), 1);
        assert_eq!(
            runtime
                .resource(assets.texture)
                .unwrap()
                .image
                .get_pixel(0, 0)
                .0[3],
            0
        );
    }

    #[test]
    fn tabs_component_accepts_arbitrary_text_labels() {
        let assets = embedded_tab_assets();
        let tabs = UiTabs::default();
        let frame = assets
            .frame(
                &tabs,
                &["First", "Second"],
                ScreenRect {
                    min: [10.0, 20.0],
                    max: [210.0, 44.0],
                },
                1.0,
                None,
            )
            .unwrap();
        assert_eq!(frame.textured_rects.len(), 6);
        assert_eq!(frame.texts.len(), 2);
        assert_eq!(frame.texts[0].content.0, "First");
        assert_eq!(frame.texts[1].content.0, "Second");
        assert_eq!(frame.textured_rects[0].uv_min[0], 0.5);
        assert_eq!(frame.textured_rects[3].uv_min[0], 0.0);
    }

    #[test]
    fn inventory_window_composes_five_tabs_and_changes_selection_on_click_release() {
        let window_assets = embedded_assets();
        let tab_assets = embedded_tab_assets();
        let mut inventory = InventoryWindow::default();
        assert!(inventory.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        let frame = inventory
            .frame(window_assets, tab_assets, viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        assert_eq!(frame.textured_rects.len(), 28);
        assert_eq!(frame.texts.len(), 6);
        assert_eq!(frame.texts[0].content.0, "Item Inventory");
        assert_eq!(frame.texts[1].content.0, "Equip");
        assert_eq!(frame.texts[2].content.0, "Use");
        assert_eq!(frame.texts[3].content.0, "Mats");
        assert_eq!(frame.texts[4].content.0, "Tools");
        assert_eq!(frame.texts[5].content.0, "Misc");

        let layout = window_assets
            .layout(&mut inventory.chrome, viewport(), 1.0)
            .unwrap()
            .unwrap();
        let bounds = inventory_tab_bounds(window_assets, tab_assets, layout, 1.0).unwrap();
        let materials = tab_rect(bounds, 2, INVENTORY_TAB_LABELS.len());
        let cursor = [
            (materials.min[0] + materials.max[0]) * 0.5,
            (materials.min[1] + materials.max[1]) * 0.5,
        ];
        assert!(inventory.apply_pointer_button(
            window_assets,
            tab_assets,
            ElementState::Pressed,
            Some(cursor),
            viewport(),
            1.0
        ));
        assert!(inventory.apply_pointer_button(
            window_assets,
            tab_assets,
            ElementState::Released,
            Some(cursor),
            viewport(),
            1.0
        ));
        assert_eq!(inventory.tabs.selected_index, 2);
    }

    #[test]
    fn inventory_window_owns_i_but_not_the_old_double_size_proof_key() {
        let mut inventory = InventoryWindow::default();
        assert!(!inventory.apply_key(
            PhysicalKey::Code(KeyCode::KeyO),
            ElementState::Pressed,
            false
        ));
        assert_eq!(inventory.chrome.mode, ProofPanelMode::Hidden);
        assert!(inventory.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        assert_eq!(inventory.chrome.mode, ProofPanelMode::Normal);
    }

    #[test]
    fn invalid_slice_and_button_state_metadata_are_rejected() {
        let invalid_panel: UiPanelMetadata = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "id": "ui.panel.invalid",
                "texture": "panel.png",
                "slice_px": { "left": 700, "right": 600, "top": 128, "bottom": 128 },
                "border_units": { "left": 32.0, "right": 32.0, "top": 32.0, "bottom": 32.0 }
            }"#,
        )
        .unwrap();
        assert!(validate_panel_metadata(&invalid_panel, [1254, 1254]).is_err());

        let invalid_close: UiCloseButtonMetadata = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "id": "ui.button.invalid",
                "texture": "BTN_quit.png",
                "states": {
                    "normal": { "x": 0, "y": 0, "width": 500, "height": 500 },
                    "hover": { "x": 500, "y": 0, "width": 500, "height": 500 },
                    "pressed": { "x": 1200, "y": 0, "width": 500, "height": 500 }
                },
                "size_units": [24.0, 24.0],
                "right_inset_units": 5.0
            }"#,
        )
        .unwrap();
        assert!(validate_close_button_metadata(&invalid_close, [1500, 500]).is_err());
    }

    #[test]
    fn window_frame_composes_panel_header_button_and_title() {
        let assets = embedded_assets();
        let mut window = normal_window();
        let frame = assets
            .proof_frame(&mut window, "Inventory", viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        assert_eq!(frame.textured_rects.len(), 13);
        assert_eq!(frame.textured_rects[0].size(), [32.0, 32.0]);
        assert_eq!(frame.textured_rects[9].size(), [28.0, 30.0]);
        assert_eq!(frame.textured_rects[10].size(), [210.0, 30.0]);
        assert_eq!(frame.textured_rects[11].size(), [28.0, 30.0]);
        assert_eq!(frame.textured_rects[12].size(), [19.0, 19.0]);
        assert_eq!(frame.textured_rects[12].uv_min, [0.0, 0.0]);
        assert_eq!(frame.textured_rects[12].uv_max, [1.0 / 3.0, 1.0]);
        assert_eq!(frame.title.content.0, "Inventory");
    }

    #[test]
    fn header_caps_and_panel_corners_remain_constant_at_double_size() {
        let assets = embedded_assets();
        let mut normal = normal_window();
        let normal = assets
            .proof_frame(&mut normal, "Panel", viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        let mut double = ProofPanelWindow::default();
        double.apply_key(
            PhysicalKey::Code(KeyCode::KeyO),
            ElementState::Pressed,
            false,
        );
        let double = assets
            .proof_frame(&mut double, "Panel", viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        assert_eq!(
            normal.textured_rects[0].size(),
            double.textured_rects[0].size()
        );
        assert_eq!(
            normal.textured_rects[9].size(),
            double.textured_rects[9].size()
        );
        assert_eq!(
            normal.textured_rects[11].size(),
            double.textured_rects[11].size()
        );
        assert_eq!(double.textured_rects[10].size(), [510.0, 30.0]);
    }

    #[test]
    fn close_button_visual_tracks_hover_and_press_without_moving_hit_bounds() {
        let assets = embedded_assets();
        let mut window = normal_window();
        let layout = assets
            .layout(&mut window, viewport(), 1.0)
            .unwrap()
            .unwrap();
        let cursor = [
            layout.close_button.min[0] + 2.0,
            layout.close_button.min[1] + 2.0,
        ];
        let hover = assets
            .proof_frame(&mut window, "Panel", viewport(), 1.0, Some(cursor))
            .unwrap()
            .unwrap();
        assert_eq!(hover.textured_rects[12].uv_min, [1.0 / 3.0, 0.0]);
        let before = layout.close_button;
        assert!(window.apply_pointer_button(
            assets,
            ElementState::Pressed,
            Some(cursor),
            viewport(),
            1.0
        ));
        let pressed = assets
            .proof_frame(&mut window, "Panel", viewport(), 1.0, Some(cursor))
            .unwrap()
            .unwrap();
        assert_eq!(pressed.textured_rects[12].uv_min, [2.0 / 3.0, 0.0]);
        assert_eq!(
            assets
                .layout(&mut window, viewport(), 1.0)
                .unwrap()
                .unwrap()
                .close_button,
            before
        );
    }

    #[test]
    fn close_requires_press_and_release_inside_button() {
        let assets = embedded_assets();
        let mut window = normal_window();
        let close = assets
            .layout(&mut window, viewport(), 1.0)
            .unwrap()
            .unwrap()
            .close_button;
        let inside = close.min;
        assert!(window.apply_pointer_button(
            assets,
            ElementState::Pressed,
            Some(inside),
            viewport(),
            1.0
        ));
        assert!(window.apply_pointer_button(
            assets,
            ElementState::Released,
            Some([0.0, 0.0]),
            viewport(),
            1.0
        ));
        assert_eq!(window.mode, ProofPanelMode::Normal);

        assert!(window.apply_pointer_button(
            assets,
            ElementState::Pressed,
            Some(inside),
            viewport(),
            1.0
        ));
        assert!(window.apply_pointer_button(
            assets,
            ElementState::Released,
            Some(inside),
            viewport(),
            1.0
        ));
        assert_eq!(window.mode, ProofPanelMode::Hidden);
    }

    #[test]
    fn header_drag_preserves_grab_offset_and_clamps_to_viewport() {
        let assets = embedded_assets();
        let mut window = normal_window();
        let layout = assets
            .layout(&mut window, viewport(), 1.0)
            .unwrap()
            .unwrap();
        let grab = [layout.header.min[0] + 80.0, layout.header.min[1] + 10.0];
        assert!(window.apply_pointer_button(
            assets,
            ElementState::Pressed,
            Some(grab),
            viewport(),
            1.0
        ));
        assert!(window.pointer_moved([-100.0, 100.0], viewport(), 1.0));
        assert_eq!(window.top_left_units, Some([0.0, 84.0]));
        assert!(window.pointer_moved([2000.0, 1000.0], viewport(), 1.0));
        assert_eq!(window.top_left_units, Some([980.0, 280.0]));
        assert!(window.apply_pointer_button(
            assets,
            ElementState::Released,
            Some([2000.0, 1000.0]),
            viewport(),
            1.0
        ));
    }

    #[test]
    fn panel_consumes_pointer_press_but_outside_does_not() {
        let assets = embedded_assets();
        let mut window = normal_window();
        let layout = assets
            .layout(&mut window, viewport(), 1.0)
            .unwrap()
            .unwrap();
        assert!(window.apply_pointer_button(
            assets,
            ElementState::Pressed,
            Some([layout.window.min[0] + 100.0, layout.window.min[1] + 100.0]),
            viewport(),
            1.0
        ));
        assert!(!window.apply_pointer_button(
            assets,
            ElementState::Pressed,
            Some([10.0, 10.0]),
            viewport(),
            1.0
        ));
    }

    #[test]
    fn keyboard_transitions_are_deterministic_and_ignore_release_repeat() {
        let mut window = ProofPanelWindow::default();
        assert!(!window.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Released,
            false
        ));
        assert!(!window.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            true
        ));
        assert_eq!(window.mode, ProofPanelMode::Hidden);

        assert!(window.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        assert_eq!(window.mode, ProofPanelMode::Normal);
        assert!(window.apply_key(
            PhysicalKey::Code(KeyCode::KeyO),
            ElementState::Pressed,
            false
        ));
        assert_eq!(window.mode, ProofPanelMode::Double);
        assert!(window.apply_key(
            PhysicalKey::Code(KeyCode::KeyO),
            ElementState::Pressed,
            false
        ));
        assert_eq!(window.mode, ProofPanelMode::Hidden);
        assert!(!window.apply_key(
            PhysicalKey::Code(KeyCode::KeyP),
            ElementState::Pressed,
            false
        ));
    }
}
