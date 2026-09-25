//! Authored panel/window chrome and the local production-UI proof.

use std::collections::HashMap;

use purgatory_common::{ContentId, ItemInstanceId};
use purgatory_content::{ContentRegistry, ItemCategory};
use purgatory_protocol::{InventoryEntry, ReplicatedEquipment};
use purgatory_simulation::EquipmentSlot;
use serde::Deserialize;
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::asset_runtime::{AssetRuntime, ResolvedVisual};
use crate::display::{
    DisplaySettings, RESOLUTION_PRESETS, RenderScale, Resolution, UI_SCALE_PRESETS, UiScale,
    WindowMode,
};
use crate::renderer::{
    PixelViewport, SpriteTextureId, TextAlignment, TextBlock, TextContent, TextStyle,
    UiTexturedRect,
};

const ATLAS_PNG: &[u8] = include_bytes!("../../../Graphic/ui/ATLAS.png");
const ATLAS_METADATA: &str = include_str!("../../../Graphic/ui/ATLAS.ui.json");
const ATLAS_TEXTURE_FILE: &str = "ATLAS.png";
const TITLE_FONT_SIZE_UNITS: f32 = 13.0;
const TITLE_MAX_HEADER_HEIGHT_FRACTION: f32 = 2.0 / 3.0;
const TITLE_LEFT_INSET_UNITS: f32 = 4.0;
const HEADER_CONTENT_OFFSET_UNITS: f32 = 2.0;
const TITLE_CONTROL_GAP_UNITS: f32 = 5.0;
const HEADER_ICON_SIZE_UNITS: f32 = 16.0;
const HEADER_ICON_GAP_UNITS: f32 = 3.0;
#[cfg(feature = "dev-diagnostics")]
const SETTINGS_WINDOW_SIZE_UNITS: [f32; 2] = [360.0, 290.0];
#[cfg(not(feature = "dev-diagnostics"))]
const SETTINGS_WINDOW_SIZE_UNITS: [f32; 2] = [360.0, 261.0];
const SETTINGS_SECTION_FONT_SIZE_UNITS: f32 = 12.0;
const SETTINGS_ROW_FONT_SIZE_UNITS: f32 = 10.5;
const SETTINGS_VALUE_FONT_SIZE_UNITS: f32 = 9.5;
const SETTINGS_CONTENT_SIDE_INSET_UNITS: f32 = 24.0;
const SETTINGS_VALUE_WIDTH_UNITS: f32 = 136.0;
const SETTINGS_ROW_HEIGHT_UNITS: f32 = 18.0;
const SETTINGS_DISPLAY_HEADING_Y_UNITS: f32 = 39.0;
const SETTINGS_FULLSCREEN_Y_UNITS: f32 = 62.0;
const SETTINGS_RESOLUTION_Y_UNITS: f32 = 91.0;
const SETTINGS_GRAPHICS_HEADING_Y_UNITS: f32 = 128.0;
const SETTINGS_RENDER_QUALITY_Y_UNITS: f32 = 151.0;
const SETTINGS_UI_SCALE_Y_UNITS: f32 = 180.0;
const SETTINGS_SESSION_HEADING_Y_UNITS: f32 = 209.0;
#[cfg(feature = "dev-diagnostics")]
const SETTINGS_RETURN_TO_LOGIN_Y_UNITS: f32 = 232.0;
#[cfg(feature = "dev-diagnostics")]
const SETTINGS_EXIT_GAME_Y_UNITS: f32 = 261.0;
#[cfg(not(feature = "dev-diagnostics"))]
const SETTINGS_EXIT_GAME_Y_UNITS: f32 = 232.0;
const SETTINGS_LAUNCHER_WIDTH_UNITS: f32 = 104.0;
const SETTINGS_LAUNCHER_INSET_UNITS: f32 = 10.0;
const SETTINGS_LAUNCHER_ICON_SIZE_UNITS: f32 = 13.0;
const SETTINGS_LAUNCHER_FONT_SIZE_UNITS: f32 = 9.5;
const SETTINGS_TEXT_COLOR: [f32; 4] = [0.05, 0.07, 0.1, 1.0];
const SETTINGS_DISABLED_TEXT_COLOR: [f32; 4] = [0.36, 0.39, 0.43, 1.0];
const INVENTORY_CONTENT_SIDE_INSET_UNITS: f32 = 23.0;
const INVENTORY_TAB_TOP_GAP_UNITS: f32 = 4.0;
const INVENTORY_SLOT_TOP_GAP_UNITS: f32 = 4.0;
const INVENTORY_TAB_SEPARATOR_HEIGHT_UNITS: f32 = 2.0;
const INVENTORY_GRID_SIDE_PADDING_UNITS: f32 = 1.0;
const INVENTORY_GRID_BOTTOM_PADDING_UNITS: f32 = 2.0;
const INVENTORY_FOOTER_RESERVED_UNITS: f32 = 28.0;
const INVENTORY_CURRENCY_VERTICAL_INSET_UNITS: f32 = 4.0;
const CURRENCY_FONT_SIZE_UNITS: f32 = 10.5;
const TAB_EMBOLDEN_OFFSET_UNITS: f32 = 0.35;
const TAB_TEXT_COLOR: [f32; 4] = [0.03, 0.045, 0.07, 1.0];
const GOLD_TEXT_COLOR: [f32; 4] = [0.48, 0.3, 0.035, 1.0];
const SILVER_TEXT_COLOR: [f32; 4] = [0.2, 0.27, 0.36, 1.0];
const INVENTORY_GRID_BACKGROUND_TINT: [f32; 4] = [0.88, 0.9, 0.92, 1.0];
const INVENTORY_TAB_SEPARATOR_TINT: [f32; 4] = [0.73, 0.18, 0.17, 1.0];
const INVENTORY_TAB_LABELS: [&str; 5] = ["Equip", "Cons.", "Mats", "Tools", "Misc"];
const INVENTORY_SLOT_COLUMNS: usize = 5;
const INVENTORY_SLOT_ROWS: usize = 7;
const INVENTORY_SLOT_SIZE_UNITS: f32 = 44.0;
const INVENTORY_SLOT_GAP_UNITS: f32 = 2.0;
const INVENTORY_RIGHT_PADDING_UNITS: f32 = 2.0;
const INVENTORY_ICON_INSET_UNITS: f32 = 4.0;
const INVENTORY_QUANTITY_FONT_SIZE_UNITS: f32 = 10.5;
const INVENTORY_QUANTITY_INSET_UNITS: f32 = 3.0;
const INVENTORY_QUANTITY_COLOR: [f32; 4] = [0.04, 0.055, 0.08, 1.0];
const EQUIPMENT_SLOT_COLUMNS: usize = 2;
const EQUIPMENT_SLOT_ROWS: usize = 3;
const EQUIPMENT_SLOT_GAP_UNITS: f32 = 20.0;
const EQUIPMENT_LABEL_FONT_SIZE_UNITS: f32 = 7.0;
const EQUIPMENT_LABEL_GAP_UNITS: f32 = 2.0;
const EQUIPMENT_LABEL_COLOR: [f32; 4] = [0.08, 0.11, 0.16, 1.0];
const EQUIPMENT_SLOT_LABELS: [&str; EquipmentSlot::COUNT] =
    ["Headwear", "Bodywear", "Pants", "Gloves", "Boots", "Weapon"];
const INVENTORY_SLOT_HOVER_TINT: [f32; 4] = [0.9, 0.96, 1.0, 1.0];
const INVENTORY_SLOT_SELECTED_TINT: [f32; 4] = [1.0, 0.9, 0.68, 1.0];
const ITEM_DRAG_THRESHOLD_PX: f32 = 4.0;
const INVENTORY_TOOLTIP_WIDTH_UNITS: f32 = 218.0;
const INVENTORY_TOOLTIP_HEIGHT_UNITS: f32 = 54.0;
const INVENTORY_TOOLTIP_OFFSET_UNITS: f32 = 10.0;
const INVENTORY_TOOLTIP_PADDING_UNITS: f32 = 7.0;
const INVENTORY_TOOLTIP_FONT_SIZE_UNITS: f32 = 11.0;
const INVENTORY_TOOLTIP_LINE_GAP_UNITS: f32 = 4.0;
const INVENTORY_TOOLTIP_BACKGROUND_TINT: [f32; 4] = [0.93, 0.94, 0.95, 0.98];
const INVENTORY_TOOLTIP_TITLE_COLOR: [f32; 4] = [0.04, 0.055, 0.08, 1.0];
const INVENTORY_TOOLTIP_DETAIL_COLOR: [f32; 4] = [0.16, 0.21, 0.28, 1.0];
const INVENTORY_INITIAL_CENTER_OFFSET_UNITS: [f32; 2] = [-56.0, -24.0];
const EQUIPMENT_INITIAL_CENTER_OFFSET_UNITS: [f32; 2] = [56.0, 24.0];
const ITEM_PLACEHOLDER_VISUAL_KEY: &str = "item.placeholder";
const ITEM_PLACEHOLDER_SIZE_PX: u32 = 32;
const INVENTORY_TAB_CATEGORIES: [ItemCategory; 5] = [
    ItemCategory::Equipment,
    ItemCategory::Consumable,
    ItemCategory::Material,
    ItemCategory::Tool,
    ItemCategory::Misc,
];
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ProofPanelMode {
    #[default]
    Hidden,
    Normal,
}

impl ProofPanelMode {
    fn logical_size(self, size_units: [f32; 2]) -> Option<[f32; 2]> {
        match self {
            Self::Hidden => None,
            Self::Normal => Some(size_units),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum PanelStyle {
    #[default]
    Base,
    Blue,
    Brown,
    Dark1,
    Dark2,
    Light1,
}

impl PanelStyle {
    pub(crate) const ALL: [Self; 6] = [
        Self::Base,
        Self::Blue,
        Self::Brown,
        Self::Dark1,
        Self::Dark2,
        Self::Light1,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Base => "Base",
            Self::Blue => "Blue",
            Self::Brown => "Brown",
            Self::Dark1 => "Dark 1",
            Self::Dark2 => "Dark 2",
            Self::Light1 => "Light 1",
        }
    }

    fn tint(self) -> [f32; 4] {
        match self {
            Self::Base => [1.0, 1.0, 1.0, 1.0],
            Self::Blue => [0.82, 0.9, 1.0, 1.0],
            Self::Brown => [1.0, 0.88, 0.74, 1.0],
            Self::Dark1 => [0.72, 0.76, 0.84, 1.0],
            Self::Dark2 => [0.58, 0.62, 0.72, 1.0],
            Self::Light1 => [1.0, 0.96, 0.88, 1.0],
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

    fn clamped_to_source(self, source: SourceInsets) -> Self {
        Self {
            left: self.left.min(source.left as f32),
            right: self.right.min(source.right as f32),
            top: self.top.min(source.top as f32),
            bottom: self.bottom.min(source.bottom as f32),
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

    fn clamped_to_source(self, source: HorizontalInsetsPx) -> Self {
        Self {
            left: self.left.min(source.left as f32),
            right: self.right.min(source.right as f32),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
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
            [
                (self.x as f32 + 0.5) / width,
                (self.y as f32 + 0.5) / height,
            ],
            [
                (self.x as f32 + self.width as f32 - 0.5) / width,
                (self.y as f32 + self.height as f32 - 0.5) / height,
            ],
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct CloseButtonStates {
    normal: SourceRectPx,
    hover: SourceRectPx,
    pressed: SourceRectPx,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct TabStates {
    normal: SourceRectPx,
    selected: SourceRectPx,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ButtonStates {
    normal: SourceRectPx,
    hover: SourceRectPx,
    pressed: SourceRectPx,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ButtonStyles {
    beige: ButtonStates,
    red: ButtonStates,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SmallButtonStyles {
    confirm: CloseButtonStates,
    close: CloseButtonStates,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UiAtlasMetadata {
    schema_version: u32,
    id: String,
    texture: String,
    dimensions_px: [u32; 2],
    window: SourceRectPx,
    slot: SourceRectPx,
    message_window: SourceRectPx,
    window_slice_px: SourceInsets,
    window_border_units: DestinationBorders,
    message_window_slice_px: SourceInsets,
    message_window_border_units: DestinationBorders,
    buttons: ButtonStyles,
    small_buttons: SmallButtonStyles,
    button_slice_px: HorizontalInsetsPx,
    button_cap_units: HorizontalCapsUnits,
    button_height_units: f32,
    close_size_units: [f32; 2],
    close_right_inset_units: f32,
    icons: HashMap<String, SourceRectPx>,
}

#[derive(Clone, Copy, Debug)]
struct PanelAsset {
    texture: SpriteTextureId,
    source_size_px: [u32; 2],
    source_rect: SourceRectPx,
    slice_px: SourceInsets,
    border_units: DestinationBorders,
    tint: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
struct PanelVariants {
    assets: [PanelAsset; 6],
}

impl PanelVariants {
    fn selected(self, style: PanelStyle) -> PanelAsset {
        let mut panel = self.assets[0];
        panel.tint = style.tint();
        panel
    }
}

const ATLAS_ICON_NAMES: [&str; 8] = [
    "skull",
    "bag",
    "sword",
    "speech",
    "scroll",
    "gear",
    "magnifier",
    "group",
];

#[derive(Clone, Copy, Debug)]
struct AtlasIcons {
    regions: [SourceRectPx; 8],
}

impl AtlasIcons {
    fn source(self, name: &str) -> Option<SourceRectPx> {
        ATLAS_ICON_NAMES
            .iter()
            .position(|candidate| *candidate == name)
            .map(|index| self.regions[index])
    }
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
    gap_units: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiSlotAssets {
    texture: SpriteTextureId,
    source_size_px: [u32; 2],
    source_rect: SourceRectPx,
    size_units: [f32; 2],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiButtonState {
    Normal,
    Hover,
    Pressed,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiButtonAssets {
    texture: SpriteTextureId,
    source_size_px: [u32; 2],
    variants: [ButtonStates; 6],
    variant: usize,
    slice_px: HorizontalInsetsPx,
    cap_units: HorizontalCapsUnits,
    pub(crate) height_units: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct UiItemIconVisual {
    texture: SpriteTextureId,
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    dimensions_px: [u32; 2],
}

#[derive(Debug)]
pub(crate) struct UiItemIconAssets {
    fallback: UiItemIconVisual,
    by_definition: HashMap<ContentId, UiItemIconVisual>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct UiWindowAssets {
    panels: PanelVariants,
    #[allow(dead_code)]
    message_panel: PanelAsset,
    close_button: CloseButtonAsset,
    #[allow(dead_code)]
    icons: AtlasIcons,
    panel_style: PanelStyle,
}

impl UiWindowAssets {
    pub(crate) fn load_embedded(assets: &mut AssetRuntime) -> Result<Self, String> {
        let metadata: UiAtlasMetadata = serde_json::from_str(ATLAS_METADATA)
            .map_err(|error| format!("parse UI atlas metadata: {error}"))?;
        let (texture, source_size_px) = register_metadata_texture(
            assets,
            &metadata.id,
            &metadata.texture,
            ATLAS_TEXTURE_FILE,
            ATLAS_PNG,
            "atlas",
        )?;
        validate_atlas_metadata(&metadata, source_size_px)?;
        let panel = PanelAsset {
            texture,
            source_size_px,
            source_rect: metadata.window,
            slice_px: metadata.window_slice_px,
            border_units: metadata.window_border_units,
            tint: PanelStyle::Base.tint(),
        };
        let message_panel = PanelAsset {
            texture,
            source_size_px,
            source_rect: metadata.message_window,
            slice_px: metadata.message_window_slice_px,
            border_units: metadata.message_window_border_units,
            tint: [1.0; 4],
        };
        let close_button = CloseButtonAsset {
            texture,
            source_size_px,
            states: metadata.small_buttons.close,
            size_units: metadata.close_size_units,
            right_inset_units: metadata.close_right_inset_units,
        };

        Ok(Self {
            panels: PanelVariants { assets: [panel; 6] },
            message_panel,
            close_button,
            icons: AtlasIcons {
                regions: std::array::from_fn(|index| metadata.icons[ATLAS_ICON_NAMES[index]]),
            },
            panel_style: PanelStyle::default(),
        })
    }

    pub(crate) fn with_panel_style(mut self, style: PanelStyle) -> Self {
        self.panel_style = style;
        self
    }

    fn panel(self) -> PanelAsset {
        self.panels.selected(self.panel_style)
    }

    fn icon_rect(self, name: &str, bounds: ScreenRect, tint: [f32; 4]) -> Option<UiTexturedRect> {
        let source = self.icons.source(name)?;
        let panel = self.panel();
        let (uv_min, uv_max) = source.uv_bounds(panel.source_size_px);
        Some(UiTexturedRect {
            min: bounds.min,
            max: bounds.max,
            texture: panel.texture,
            uv_min,
            uv_max,
            tint,
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
        self.proof_frame_with_icon(window, title, None, viewport, pixels_per_unit, cursor)
    }

    fn proof_frame_with_icon(
        self,
        window: &mut ProofPanelWindow,
        title: &str,
        icon: Option<&str>,
        viewport: PixelViewport,
        pixels_per_unit: f32,
        cursor: Option<[f32; 2]>,
    ) -> Result<Option<UiWindowFrame>, String> {
        let Some(layout) = self.layout(window, viewport, pixels_per_unit)? else {
            return Ok(None);
        };
        let panel = self.panels.selected(self.panel_style);
        let destination_borders = panel
            .border_units
            .scaled(pixels_per_unit)
            .clamped_to_source(panel.slice_px);
        let mut textured_rects = assemble_nine_slice_region(
            layout.window,
            panel.texture,
            panel.source_size_px,
            panel.source_rect,
            panel.slice_px,
            destination_borders,
            panel.tint,
        )?;

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
        let mut title_anchor_x = layout.header.min[0] + TITLE_LEFT_INSET_UNITS * pixels_per_unit;
        let header_content_scale =
            destination_borders.top / panel.border_units.top.max(f32::EPSILON);
        if let Some(icon) = icon {
            let header_content_offset = HEADER_CONTENT_OFFSET_UNITS * header_content_scale;
            let icon_size = (HEADER_ICON_SIZE_UNITS * pixels_per_unit)
                .min((destination_borders.top - header_content_offset * 2.0).max(1.0));
            let icon_min = [
                title_anchor_x,
                layout.header.min[1]
                    + ((layout.header.height() - icon_size) * 0.5).max(0.0)
                    + header_content_offset,
            ];
            if let Some(icon_rect) = self.icon_rect(
                icon,
                ScreenRect {
                    min: icon_min,
                    max: [icon_min[0] + icon_size, icon_min[1] + icon_size],
                },
                [1.0; 4],
            ) {
                textured_rects.push(icon_rect);
                title_anchor_x +=
                    (HEADER_ICON_SIZE_UNITS + HEADER_ICON_GAP_UNITS) * pixels_per_unit;
            }
        }

        let title_font_size = (TITLE_FONT_SIZE_UNITS * pixels_per_unit)
            .min(destination_borders.top * TITLE_MAX_HEADER_HEIGHT_FRACTION);
        let title_anchor = [
            title_anchor_x,
            layout.header.min[1]
                + ((layout.header.height() - title_font_size) * 0.5).max(0.0)
                + HEADER_CONTENT_OFFSET_UNITS * header_content_scale,
        ];
        let title_max_width = (layout.close_button.min[0]
            - TITLE_CONTROL_GAP_UNITS * pixels_per_unit
            - title_anchor[0])
            .max(1.0);
        Ok(Some(UiWindowFrame {
            textured_rects,
            title: TextBlock {
                content: TextContent(title.to_owned()),
                style: TextStyle::at_size(
                    title_font_size / pixels_per_unit,
                    [1.0; 4],
                    TextAlignment::Left,
                ),
                anchor: title_anchor,
                max_width: Some(title_max_width),
            },
        }))
    }

    pub(crate) fn message_chrome(
        self,
        title: &str,
        viewport: PixelViewport,
        pixels_per_unit: f32,
        cursor: Option<[f32; 2]>,
    ) -> Result<Option<UiMessageChrome>, String> {
        let mut window = ProofPanelWindow {
            mode: ProofPanelMode::Normal,
            size_units: [400.0, 180.0],
            ..ProofPanelWindow::default()
        };
        let Some(layout) = self.layout(&mut window, viewport, pixels_per_unit)? else {
            return Ok(None);
        };
        let Some(frame) =
            self.proof_frame(&mut window, title, viewport, pixels_per_unit, cursor)?
        else {
            return Ok(None);
        };
        Ok(Some(UiMessageChrome {
            frame,
            window: layout.window,
            close_button: layout.close_button,
        }))
    }

    fn layout(
        self,
        window: &mut ProofPanelWindow,
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> Result<Option<UiWindowLayout>, String> {
        validate_pixels_per_unit(pixels_per_unit)?;
        let Some(window_size_units) = window.mode.logical_size(window.size_units) else {
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
        let initial = [
            centered[0] + window.initial_center_offset_units[0],
            centered[1] + window.initial_center_offset_units[1],
        ];
        let top_left_units = window.top_left_units.get_or_insert(initial);
        *top_left_units = clamp_top_left(*top_left_units, window_size_units, viewport_size_units);

        let window_min = [
            viewport.x as f32 + top_left_units[0] * pixels_per_unit,
            viewport.y as f32 + top_left_units[1] * pixels_per_unit,
        ];
        let window_max = [
            window_min[0] + window_size_units[0] * pixels_per_unit,
            window_min[1] + window_size_units[1] * pixels_per_unit,
        ];
        let panel = self.panel();
        let destination_borders = panel
            .border_units
            .scaled(pixels_per_unit)
            .clamped_to_source(panel.slice_px);
        let header_content_scale =
            destination_borders.top / panel.border_units.top.max(f32::EPSILON);
        let header = ScreenRect {
            min: [
                window_min[0] + panel.border_units.left * pixels_per_unit,
                window_min[1],
            ],
            max: [
                window_max[0] - panel.border_units.right * pixels_per_unit,
                window_min[1] + destination_borders.top,
            ],
        };
        let button_size = [
            (self.close_button.size_units[0] * pixels_per_unit).min(
                self.close_button
                    .states
                    .normal
                    .width
                    .min(self.close_button.states.hover.width)
                    .min(self.close_button.states.pressed.width) as f32,
            ),
            (self.close_button.size_units[1] * pixels_per_unit).min(
                self.close_button
                    .states
                    .normal
                    .height
                    .min(self.close_button.states.hover.height)
                    .min(self.close_button.states.pressed.height) as f32,
            ),
        ];
        let close_max_x = header.max[0] - self.close_button.right_inset_units * pixels_per_unit;
        let close_min_y = header.min[1]
            + ((header.height() - button_size[1]) * 0.5).max(0.0)
            + HEADER_CONTENT_OFFSET_UNITS * header_content_scale;
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
        let metadata: UiAtlasMetadata = serde_json::from_str(ATLAS_METADATA)
            .map_err(|error| format!("parse UI atlas metadata for tabs: {error}"))?;
        let (texture, source_size_px) = register_metadata_texture(
            assets,
            &metadata.id,
            &metadata.texture,
            ATLAS_TEXTURE_FILE,
            ATLAS_PNG,
            "atlas",
        )?;
        validate_atlas_metadata(&metadata, source_size_px)?;
        Ok(Self {
            texture,
            source_size_px,
            states: TabStates {
                normal: metadata.buttons.beige.normal,
                selected: metadata.buttons.red.normal,
            },
            slice_px: metadata.button_slice_px,
            cap_units: metadata.button_cap_units,
            height_units: metadata.button_height_units,
            font_size_units: 10.5,
            horizontal_text_padding_units: 4.0,
            gap_units: 1.0,
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
        let mut texts = Vec::with_capacity(labels.len() * 2);
        let mut emboldened_texts = Vec::with_capacity(labels.len());
        for (index, label) in labels.iter().enumerate() {
            let hit_rect = tab_rect(
                bounds,
                index,
                labels.len(),
                self.gap_units * pixels_per_unit,
            );
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
                self.cap_units
                    .scaled(pixels_per_unit)
                    .clamped_to_source(self.slice_px),
                tint,
            )?);

            let font_size = self.font_size_units * pixels_per_unit;
            let center_x = (draw_rect.min[0] + draw_rect.max[0]) * 0.5;
            let anchor_y = draw_rect.min[1] + ((draw_rect.height() - font_size) * 0.5).max(0.0);
            let max_width = Some(
                (draw_rect.width() - self.horizontal_text_padding_units * 2.0 * pixels_per_unit)
                    .max(1.0),
            );
            let embolden_offset = TAB_EMBOLDEN_OFFSET_UNITS * pixels_per_unit;
            texts.push(tab_text_block(
                label,
                self.font_size_units,
                [center_x - embolden_offset, anchor_y],
                max_width,
            ));
            emboldened_texts.push(tab_text_block(
                label,
                self.font_size_units,
                [center_x + embolden_offset, anchor_y],
                max_width,
            ));
        }
        texts.extend(emboldened_texts);
        Ok(UiTabsFrame {
            textured_rects,
            texts,
        })
    }
}

fn tab_text_block(
    label: &str,
    font_size: f32,
    anchor: [f32; 2],
    max_width: Option<f32>,
) -> TextBlock {
    TextBlock {
        content: TextContent(label.to_string()),
        style: TextStyle::at_size(font_size, TAB_TEXT_COLOR, TextAlignment::Center),
        anchor,
        max_width,
    }
}

impl UiSlotAssets {
    pub(crate) fn load_embedded(assets: &mut AssetRuntime) -> Result<Self, String> {
        let metadata: UiAtlasMetadata = serde_json::from_str(ATLAS_METADATA)
            .map_err(|error| format!("parse UI atlas metadata for slots: {error}"))?;
        let (texture, source_size_px) = register_metadata_texture(
            assets,
            &metadata.id,
            &metadata.texture,
            ATLAS_TEXTURE_FILE,
            ATLAS_PNG,
            "atlas",
        )?;
        validate_atlas_metadata(&metadata, source_size_px)?;
        Ok(Self {
            texture,
            source_size_px,
            source_rect: metadata.slot,
            size_units: [44.0, 44.0],
        })
    }

    fn frame(
        self,
        grid: UiSlotGrid,
        origin: [f32; 2],
        pixels_per_unit: f32,
    ) -> Result<Vec<UiTexturedRect>, String> {
        validate_pixels_per_unit(pixels_per_unit)?;
        grid.validate()?;
        if !origin.into_iter().all(f32::is_finite) {
            return Err("UI slot-grid origin must be finite".to_string());
        }

        let slot_size = [
            self.size_units[0] * pixels_per_unit,
            self.size_units[1] * pixels_per_unit,
        ];
        let gap = grid.gap_units * pixels_per_unit;
        let mut textured_rects = Vec::with_capacity(grid.slot_count());
        for row in 0..grid.rows {
            for column in 0..grid.columns {
                let min = [
                    origin[0] + column as f32 * (slot_size[0] + gap),
                    origin[1] + row as f32 * (slot_size[1] + gap),
                ];
                textured_rects.push(UiTexturedRect {
                    min,
                    max: [min[0] + slot_size[0], min[1] + slot_size[1]],
                    texture: self.texture,
                    uv_min: self.source_rect.uv_bounds(self.source_size_px).0,
                    uv_max: self.source_rect.uv_bounds(self.source_size_px).1,
                    tint: [1.0; 4],
                });
            }
        }
        Ok(textured_rects)
    }
}

impl UiButtonAssets {
    pub(crate) fn load_embedded(assets: &mut AssetRuntime) -> Result<Self, String> {
        let metadata: UiAtlasMetadata = serde_json::from_str(ATLAS_METADATA)
            .map_err(|error| format!("parse UI atlas metadata for buttons: {error}"))?;
        let (texture, source_size_px) = register_metadata_texture(
            assets,
            &metadata.id,
            &metadata.texture,
            ATLAS_TEXTURE_FILE,
            ATLAS_PNG,
            "atlas",
        )?;
        validate_atlas_metadata(&metadata, source_size_px)?;
        let variants = [
            metadata.buttons.beige,
            metadata.buttons.red,
            metadata.buttons.beige,
            metadata.buttons.red,
            metadata.buttons.beige,
            metadata.buttons.red,
        ];
        Ok(Self {
            texture,
            source_size_px,
            variants,
            variant: 0,
            slice_px: metadata.button_slice_px,
            cap_units: metadata.button_cap_units,
            height_units: metadata.button_height_units,
        })
    }

    pub(crate) fn frame(
        self,
        bounds: ScreenRect,
        state: UiButtonState,
        pixels_per_unit: f32,
    ) -> Result<Vec<UiTexturedRect>, String> {
        validate_pixels_per_unit(pixels_per_unit)?;
        let states = self.variants[self.variant];
        let source = match state {
            UiButtonState::Normal => states.normal,
            UiButtonState::Hover => states.hover,
            UiButtonState::Pressed => states.pressed,
        };
        assemble_horizontal_three_slice_region(
            bounds,
            self.texture,
            self.source_size_px,
            source,
            self.slice_px,
            self.cap_units
                .scaled(pixels_per_unit)
                .clamped_to_source(self.slice_px),
            [1.0; 4],
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettingsControl {
    Fullscreen,
    Resolution,
    RenderQuality,
    UiScale,
    #[cfg(feature = "dev-diagnostics")]
    ReturnToLogin,
    ExitGame,
}

const SETTINGS_CONTROLS: &[SettingsControl] = &[
    SettingsControl::Fullscreen,
    SettingsControl::Resolution,
    SettingsControl::RenderQuality,
    SettingsControl::UiScale,
    #[cfg(feature = "dev-diagnostics")]
    SettingsControl::ReturnToLogin,
    SettingsControl::ExitGame,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SettingsAction {
    SetWindowMode(WindowMode),
    SetResolution(Resolution),
    SetRenderScale(RenderScale),
    SetUiScale(UiScale),
    #[cfg(feature = "dev-diagnostics")]
    ReturnToLogin,
    ExitGame,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SettingsWindow {
    chrome: ProofPanelWindow,
    pressed_control: Option<SettingsControl>,
    completed_action: Option<SettingsAction>,
}

impl Default for SettingsWindow {
    fn default() -> Self {
        Self {
            chrome: ProofPanelWindow::with_size(SETTINGS_WINDOW_SIZE_UNITS),
            pressed_control: None,
            completed_action: None,
        }
    }
}

impl SettingsWindow {
    pub(crate) fn open(&mut self) {
        self.chrome.open();
    }

    pub(crate) fn close(&mut self) {
        self.chrome.close();
        self.pressed_control = None;
        self.completed_action = None;
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.chrome.is_visible()
    }

    pub(crate) fn frame(
        &mut self,
        window_assets: UiWindowAssets,
        button_assets: UiButtonAssets,
        settings: DisplaySettings,
        viewport: PixelViewport,
        pixels_per_unit: f32,
        cursor: Option<[f32; 2]>,
    ) -> Result<Option<SettingsWindowFrame>, String> {
        let Some(layout) = window_assets.layout(&mut self.chrome, viewport, pixels_per_unit)?
        else {
            return Ok(None);
        };
        let Some(window_frame) = window_assets.proof_frame_with_icon(
            &mut self.chrome,
            "SETTINGS",
            Some("gear"),
            viewport,
            pixels_per_unit,
            cursor,
        )?
        else {
            return Ok(None);
        };

        let mut textured_rects = window_frame.textured_rects;
        let mut texts = vec![window_frame.title];
        texts.push(settings_text(
            "Display",
            SETTINGS_SECTION_FONT_SIZE_UNITS,
            SETTINGS_TEXT_COLOR,
            [
                layout.window.min[0] + SETTINGS_CONTENT_SIDE_INSET_UNITS * pixels_per_unit,
                layout.window.min[1] + SETTINGS_DISPLAY_HEADING_Y_UNITS * pixels_per_unit,
            ],
            TextAlignment::Left,
            None,
        ));
        texts.push(settings_text(
            "Graphics",
            SETTINGS_SECTION_FONT_SIZE_UNITS,
            SETTINGS_TEXT_COLOR,
            [
                layout.window.min[0] + SETTINGS_CONTENT_SIDE_INSET_UNITS * pixels_per_unit,
                layout.window.min[1] + SETTINGS_GRAPHICS_HEADING_Y_UNITS * pixels_per_unit,
            ],
            TextAlignment::Left,
            None,
        ));
        texts.push(settings_text(
            "Session",
            SETTINGS_SECTION_FONT_SIZE_UNITS,
            SETTINGS_TEXT_COLOR,
            [
                layout.window.min[0] + SETTINGS_CONTENT_SIDE_INSET_UNITS * pixels_per_unit,
                layout.window.min[1] + SETTINGS_SESSION_HEADING_Y_UNITS * pixels_per_unit,
            ],
            TextAlignment::Left,
            None,
        ));

        for control in SETTINGS_CONTROLS.iter().copied() {
            let bounds = settings_control_bounds(layout.window, control, pixels_per_unit);
            let enabled = settings_control_enabled(control, settings);
            let hovered = enabled && cursor.is_some_and(|cursor| bounds.contains(cursor));
            let state = if hovered && self.pressed_control == Some(control) {
                UiButtonState::Pressed
            } else if hovered {
                UiButtonState::Hover
            } else {
                UiButtonState::Normal
            };
            let mut button_rects = button_assets.frame(bounds, state, pixels_per_unit)?;
            if !enabled {
                for rect in &mut button_rects {
                    rect.tint = [0.62, 0.64, 0.67, 1.0];
                }
            }
            textured_rects.extend(button_rects);

            let label_y = bounds.min[1]
                + ((bounds.height() - SETTINGS_ROW_FONT_SIZE_UNITS * pixels_per_unit) * 0.5)
                    .max(0.0);
            texts.push(settings_text(
                settings_control_name(control),
                SETTINGS_ROW_FONT_SIZE_UNITS,
                if enabled {
                    SETTINGS_TEXT_COLOR
                } else {
                    SETTINGS_DISABLED_TEXT_COLOR
                },
                [
                    layout.window.min[0] + SETTINGS_CONTENT_SIDE_INSET_UNITS * pixels_per_unit,
                    label_y,
                ],
                TextAlignment::Left,
                Some(
                    (bounds.min[0]
                        - layout.window.min[0]
                        - (SETTINGS_CONTENT_SIDE_INSET_UNITS + 8.0) * pixels_per_unit)
                        .max(1.0),
                ),
            ));
            let value_font_size = SETTINGS_VALUE_FONT_SIZE_UNITS * pixels_per_unit;
            texts.push(settings_text(
                &settings_control_value(control, settings),
                SETTINGS_VALUE_FONT_SIZE_UNITS,
                if enabled {
                    SETTINGS_TEXT_COLOR
                } else {
                    SETTINGS_DISABLED_TEXT_COLOR
                },
                [
                    (bounds.min[0] + bounds.max[0]) * 0.5,
                    bounds.min[1] + ((bounds.height() - value_font_size) * 0.5).max(0.0),
                ],
                TextAlignment::Center,
                Some((bounds.width() - 8.0 * pixels_per_unit).max(1.0)),
            ));
        }

        Ok(Some(SettingsWindowFrame {
            textured_rects,
            texts,
        }))
    }

    pub(crate) fn apply_pointer_button(
        &mut self,
        window_assets: UiWindowAssets,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        viewport: PixelViewport,
        pixels_per_unit: f32,
        settings: DisplaySettings,
    ) -> bool {
        let Ok(Some(layout)) = window_assets.layout(&mut self.chrome, viewport, pixels_per_unit)
        else {
            self.cancel_pointer_interaction();
            return false;
        };
        let hit = cursor.and_then(|point| {
            SETTINGS_CONTROLS.iter().copied().find(|control| {
                settings_control_enabled(*control, settings)
                    && settings_control_bounds(layout.window, *control, pixels_per_unit)
                        .contains(point)
            })
        });
        match state {
            ElementState::Pressed if hit.is_some() => {
                self.pressed_control = hit;
                true
            }
            ElementState::Released if self.pressed_control.is_some() => {
                let pressed = self.pressed_control.take();
                if pressed == hit {
                    self.completed_action =
                        pressed.and_then(|control| settings_action(control, settings));
                }
                true
            }
            _ => self.chrome.apply_pointer_button(
                window_assets,
                state,
                cursor,
                viewport,
                pixels_per_unit,
            ),
        }
    }

    pub(crate) fn pointer_moved(
        &mut self,
        cursor: [f32; 2],
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        self.chrome.pointer_moved(cursor, viewport, pixels_per_unit)
    }

    pub(crate) fn contains_window(
        &mut self,
        window_assets: UiWindowAssets,
        cursor: [f32; 2],
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        window_assets
            .layout(&mut self.chrome, viewport, pixels_per_unit)
            .ok()
            .flatten()
            .is_some_and(|layout| layout.window.contains(cursor))
    }

    pub(crate) fn cancel_pointer_interaction(&mut self) {
        self.chrome.cancel_pointer_interaction();
        self.pressed_control = None;
    }

    pub(crate) fn take_completed_action(&mut self) -> Option<SettingsAction> {
        self.completed_action.take()
    }
}

pub(crate) struct SettingsWindowFrame {
    pub(crate) textured_rects: Vec<UiTexturedRect>,
    pub(crate) texts: Vec<TextBlock>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SettingsLauncher {
    pressed: bool,
    open_requested: bool,
}

impl SettingsLauncher {
    pub(crate) fn frame(
        &self,
        window_assets: UiWindowAssets,
        button_assets: UiButtonAssets,
        viewport: PixelViewport,
        pixels_per_unit: f32,
        cursor: Option<[f32; 2]>,
    ) -> Result<SettingsLauncherFrame, String> {
        let bounds = settings_launcher_bounds(viewport, pixels_per_unit)?;
        let hovered = cursor.is_some_and(|cursor| bounds.contains(cursor));
        let state = if hovered && self.pressed {
            UiButtonState::Pressed
        } else if hovered {
            UiButtonState::Hover
        } else {
            UiButtonState::Normal
        };
        let mut textured_rects = button_assets.frame(bounds, state, pixels_per_unit)?;
        let icon_size = SETTINGS_LAUNCHER_ICON_SIZE_UNITS * pixels_per_unit;
        let icon_min = [
            bounds.min[0] + 7.0 * pixels_per_unit,
            bounds.min[1] + ((bounds.height() - icon_size) * 0.5).max(0.0),
        ];
        if let Some(icon) = window_assets.icon_rect(
            "gear",
            ScreenRect {
                min: icon_min,
                max: [icon_min[0] + icon_size, icon_min[1] + icon_size],
            },
            [1.0; 4],
        ) {
            textured_rects.push(icon);
        }
        let font_size = SETTINGS_LAUNCHER_FONT_SIZE_UNITS * pixels_per_unit;
        let text = settings_text(
            "SETTINGS",
            SETTINGS_LAUNCHER_FONT_SIZE_UNITS,
            SETTINGS_TEXT_COLOR,
            [
                bounds.min[0] + bounds.width() * 0.58,
                bounds.min[1] + ((bounds.height() - font_size) * 0.5).max(0.0),
            ],
            TextAlignment::Center,
            Some(bounds.width() * 0.72),
        );
        Ok(SettingsLauncherFrame {
            textured_rects,
            texts: vec![text],
        })
    }

    pub(crate) fn apply_pointer_button(
        &mut self,
        state: ElementState,
        cursor: Option<[f32; 2]>,
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        let Ok(bounds) = settings_launcher_bounds(viewport, pixels_per_unit) else {
            self.pressed = false;
            return false;
        };
        match state {
            ElementState::Pressed => {
                self.pressed = cursor.is_some_and(|cursor| bounds.contains(cursor));
                self.pressed
            }
            ElementState::Released => {
                if !self.pressed {
                    return false;
                }
                self.pressed = false;
                self.open_requested = cursor.is_some_and(|cursor| bounds.contains(cursor));
                true
            }
        }
    }

    pub(crate) fn cancel_pointer_interaction(&mut self) {
        self.pressed = false;
    }

    pub(crate) fn take_open_requested(&mut self) -> bool {
        std::mem::take(&mut self.open_requested)
    }
}

pub(crate) struct SettingsLauncherFrame {
    pub(crate) textured_rects: Vec<UiTexturedRect>,
    pub(crate) texts: Vec<TextBlock>,
}

fn settings_launcher_bounds(
    viewport: PixelViewport,
    pixels_per_unit: f32,
) -> Result<ScreenRect, String> {
    validate_pixels_per_unit(pixels_per_unit)?;
    let width = SETTINGS_LAUNCHER_WIDTH_UNITS * pixels_per_unit;
    let height = SETTINGS_ROW_HEIGHT_UNITS * pixels_per_unit;
    let inset = SETTINGS_LAUNCHER_INSET_UNITS * pixels_per_unit;
    let max = [
        viewport.x as f32 + viewport.width as f32 - inset,
        viewport.y as f32 + inset + height,
    ];
    Ok(ScreenRect {
        min: [max[0] - width, max[1] - height],
        max,
    })
}

fn settings_control_bounds(
    window: ScreenRect,
    control: SettingsControl,
    pixels_per_unit: f32,
) -> ScreenRect {
    let y_units = match control {
        SettingsControl::Fullscreen => SETTINGS_FULLSCREEN_Y_UNITS,
        SettingsControl::Resolution => SETTINGS_RESOLUTION_Y_UNITS,
        SettingsControl::RenderQuality => SETTINGS_RENDER_QUALITY_Y_UNITS,
        SettingsControl::UiScale => SETTINGS_UI_SCALE_Y_UNITS,
        #[cfg(feature = "dev-diagnostics")]
        SettingsControl::ReturnToLogin => SETTINGS_RETURN_TO_LOGIN_Y_UNITS,
        SettingsControl::ExitGame => SETTINGS_EXIT_GAME_Y_UNITS,
    };
    let max_x = window.max[0] - SETTINGS_CONTENT_SIDE_INSET_UNITS * pixels_per_unit;
    let min_y = window.min[1] + y_units * pixels_per_unit;
    ScreenRect {
        min: [max_x - SETTINGS_VALUE_WIDTH_UNITS * pixels_per_unit, min_y],
        max: [max_x, min_y + SETTINGS_ROW_HEIGHT_UNITS * pixels_per_unit],
    }
}

fn settings_control_enabled(control: SettingsControl, settings: DisplaySettings) -> bool {
    control != SettingsControl::Resolution || settings.window_mode == WindowMode::Windowed
}

fn settings_control_name(control: SettingsControl) -> &'static str {
    match control {
        SettingsControl::Fullscreen => "Fullscreen",
        SettingsControl::Resolution => "Resolution",
        SettingsControl::RenderQuality => "Render Quality",
        SettingsControl::UiScale => "UI Scale",
        #[cfg(feature = "dev-diagnostics")]
        SettingsControl::ReturnToLogin => "Return to Login",
        SettingsControl::ExitGame => "Exit Game",
    }
}

fn settings_control_value(control: SettingsControl, settings: DisplaySettings) -> String {
    match control {
        SettingsControl::Fullscreen => match settings.window_mode {
            WindowMode::Windowed => "OFF".to_string(),
            WindowMode::BorderlessFullscreen => "ON".to_string(),
        },
        SettingsControl::Resolution => format!("{} ▼", settings.resolution.label()),
        SettingsControl::RenderQuality => {
            format!("{} ▼", render_quality_label(settings.render_scale))
        }
        SettingsControl::UiScale => format!("{}% ▼", settings.ui_scale.percent()),
        #[cfg(feature = "dev-diagnostics")]
        SettingsControl::ReturnToLogin => "RETURN".to_string(),
        SettingsControl::ExitGame => "EXIT".to_string(),
    }
}

fn render_quality_label(scale: RenderScale) -> &'static str {
    if scale == RenderScale::DEFAULT {
        "High"
    } else {
        "Performance"
    }
}

fn settings_action(control: SettingsControl, settings: DisplaySettings) -> Option<SettingsAction> {
    match control {
        SettingsControl::Fullscreen => {
            Some(SettingsAction::SetWindowMode(match settings.window_mode {
                WindowMode::Windowed => WindowMode::BorderlessFullscreen,
                WindowMode::BorderlessFullscreen => WindowMode::Windowed,
            }))
        }
        SettingsControl::Resolution if settings.window_mode != WindowMode::Windowed => None,
        SettingsControl::Resolution => {
            let current = RESOLUTION_PRESETS
                .iter()
                .position(|preset| *preset == settings.resolution)
                .unwrap_or(0);
            Some(SettingsAction::SetResolution(
                RESOLUTION_PRESETS[(current + 1) % RESOLUTION_PRESETS.len()],
            ))
        }
        SettingsControl::RenderQuality => Some(SettingsAction::SetRenderScale(
            if settings.render_scale == RenderScale::DEFAULT {
                RenderScale::PERFORMANCE_FALLBACK
            } else {
                RenderScale::DEFAULT
            },
        )),
        SettingsControl::UiScale => {
            let current = UI_SCALE_PRESETS
                .iter()
                .position(|preset| (preset.get() - settings.ui_scale.get()).abs() < 0.001)
                .unwrap_or(1);
            Some(SettingsAction::SetUiScale(
                UI_SCALE_PRESETS[(current + 1) % UI_SCALE_PRESETS.len()],
            ))
        }
        #[cfg(feature = "dev-diagnostics")]
        SettingsControl::ReturnToLogin => Some(SettingsAction::ReturnToLogin),
        SettingsControl::ExitGame => Some(SettingsAction::ExitGame),
    }
}

fn settings_text(
    text: &str,
    font_size: f32,
    color: [f32; 4],
    anchor: [f32; 2],
    alignment: TextAlignment,
    max_width: Option<f32>,
) -> TextBlock {
    TextBlock {
        content: TextContent(text.to_string()),
        style: TextStyle::at_size(font_size, color, alignment),
        anchor,
        max_width,
    }
}

impl UiItemIconAssets {
    pub(crate) fn load_placeholder(
        assets: &mut AssetRuntime,
        registry: &ContentRegistry,
    ) -> Result<Self, String> {
        let texture = assets.register_image(
            "ui.inventory.item.placeholder",
            placeholder_item_icon_image(),
        )?;
        let resolved = ResolvedVisual {
            texture,
            rect_px: [0, 0, ITEM_PLACEHOLDER_SIZE_PX, ITEM_PLACEHOLDER_SIZE_PX],
            uv: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            pivot_px: [0.0, 0.0],
            dimensions_px: [ITEM_PLACEHOLDER_SIZE_PX, ITEM_PLACEHOLDER_SIZE_PX],
            pixels_per_unit: 1.0,
        };
        assets.register_visual(ITEM_PLACEHOLDER_VISUAL_KEY, resolved)?;
        let fallback = ui_item_icon_visual(resolved);
        let mut by_definition = HashMap::new();
        for presentation in registry.iter_item_presentations() {
            match assets.visual(&presentation.icon).copied() {
                Some(visual) => {
                    by_definition.insert(presentation.content_id, ui_item_icon_visual(visual));
                }
                None => eprintln!(
                    "PURGATORY item icon fallback: definition='{}' missing visual='{}'",
                    presentation.authored_id, presentation.icon
                ),
            }
        }
        Ok(Self {
            fallback,
            by_definition,
        })
    }

    fn resolve(&self, definition: ContentId) -> UiItemIconVisual {
        self.by_definition
            .get(&definition)
            .copied()
            .unwrap_or(self.fallback)
    }
}

fn ui_item_icon_visual(visual: ResolvedVisual) -> UiItemIconVisual {
    UiItemIconVisual {
        texture: visual.texture,
        uv_min: visual.uv[3],
        uv_max: visual.uv[1],
        dimensions_px: visual.dimensions_px,
    }
}

fn placeholder_item_icon_image() -> image::RgbaImage {
    let mut image = image::RgbaImage::new(ITEM_PLACEHOLDER_SIZE_PX, ITEM_PLACEHOLDER_SIZE_PX);
    let center = (ITEM_PLACEHOLDER_SIZE_PX / 2) as i32;
    for y in 3..ITEM_PLACEHOLDER_SIZE_PX - 3 {
        for x in 3..ITEM_PLACEHOLDER_SIZE_PX - 3 {
            let distance = (x as i32 - center).abs() + (y as i32 - center).abs();
            if distance <= 13 {
                let color = if distance >= 11 {
                    image::Rgba([55, 72, 94, 255])
                } else {
                    image::Rgba([226, 214, 177, 255])
                };
                image.put_pixel(x, y, color);
            }
        }
    }
    const QUESTION_MARK: [&str; 7] = [
        "01110", "10001", "00001", "00110", "00100", "00000", "00100",
    ];
    for (row, pixels) in QUESTION_MARK.iter().enumerate() {
        for (column, pixel) in pixels.bytes().enumerate() {
            if pixel == b'1' {
                for offset_y in 0..2 {
                    for offset_x in 0..2 {
                        image.put_pixel(
                            11 + column as u32 * 2 + offset_x,
                            8 + row as u32 * 2 + offset_y,
                            image::Rgba([38, 48, 64, 255]),
                        );
                    }
                }
            }
        }
    }
    image
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
        gap: f32,
    ) -> bool {
        if !finite_non_negative(gap) {
            self.pressed_index = None;
            return false;
        }
        match state {
            ElementState::Pressed => {
                self.pressed_index =
                    cursor.and_then(|cursor| tab_at(bounds, tab_count, gap, cursor));
                self.pressed_index.is_some()
            }
            ElementState::Released => {
                let Some(pressed) = self.pressed_index.take() else {
                    return false;
                };
                if cursor.and_then(|cursor| tab_at(bounds, tab_count, gap, cursor)) == Some(pressed)
                {
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct UiCurrencyDisplay {
    gold: u64,
    silver: u64,
}

impl UiCurrencyDisplay {
    fn frame(self, bounds: ScreenRect, pixels_per_unit: f32) -> Result<Vec<TextBlock>, String> {
        validate_pixels_per_unit(pixels_per_unit)?;
        if !bounds.min.into_iter().chain(bounds.max).all(f32::is_finite)
            || bounds.width() <= 0.0
            || bounds.height() < CURRENCY_FONT_SIZE_UNITS * pixels_per_unit
        {
            return Err("inventory currency bounds are too small".to_string());
        }
        let font_size = CURRENCY_FONT_SIZE_UNITS * pixels_per_unit;
        let anchor_y = bounds.min[1] + ((bounds.height() - font_size) * 0.5).max(0.0);
        let column_width = bounds.width() * 0.5;
        Ok(vec![
            TextBlock {
                content: TextContent(format!("Gold: {}", self.gold)),
                style: TextStyle::at_size(
                    CURRENCY_FONT_SIZE_UNITS,
                    GOLD_TEXT_COLOR,
                    TextAlignment::Center,
                ),
                anchor: [bounds.min[0] + column_width * 0.5, anchor_y],
                max_width: Some(column_width),
            },
            TextBlock {
                content: TextContent(format!("Silver: {}", self.silver)),
                style: TextStyle::at_size(
                    CURRENCY_FONT_SIZE_UNITS,
                    SILVER_TEXT_COLOR,
                    TextAlignment::Center,
                ),
                anchor: [bounds.min[0] + column_width * 1.5, anchor_y],
                max_width: Some(column_width),
            },
        ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct UiSlotGrid {
    columns: usize,
    rows: usize,
    gap_units: f32,
}

impl UiSlotGrid {
    const fn new(columns: usize, rows: usize, gap_units: f32) -> Self {
        Self {
            columns,
            rows,
            gap_units,
        }
    }

    fn validate(self) -> Result<(), String> {
        if self.columns == 0 || self.rows == 0 || !finite_non_negative(self.gap_units) {
            return Err(
                "UI slot grid requires rows/columns and a finite non-negative gap".to_string(),
            );
        }
        Ok(())
    }

    fn slot_count(self) -> usize {
        self.columns.saturating_mul(self.rows)
    }

    fn logical_size(self, assets: UiSlotAssets) -> Result<[f32; 2], String> {
        self.validate()?;
        Ok([
            self.columns as f32 * assets.size_units[0]
                + self.columns.saturating_sub(1) as f32 * self.gap_units,
            self.rows as f32 * assets.size_units[1]
                + self.rows.saturating_sub(1) as f32 * self.gap_units,
        ])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InventoryWindow {
    chrome: ProofPanelWindow,
    tabs: UiTabs,
    slots: UiSlotGrid,
    currency: UiCurrencyDisplay,
    selected_item: Option<ItemInstanceId>,
    pressed_item: Option<ItemInstanceId>,
    pressed_position: Option<[f32; 2]>,
    dragging_item: Option<ItemInstanceId>,
    completed_drag: Option<ItemInstanceId>,
    completed_click: Option<ItemInstanceId>,
    slot_hit_regions: Vec<ScreenRect>,
    item_hit_regions: Vec<(ScreenRect, ItemInstanceId)>,
}

impl Default for InventoryWindow {
    fn default() -> Self {
        Self {
            chrome: ProofPanelWindow::with_size_and_center_offset(
                [
                    2.0 * INVENTORY_CONTENT_SIDE_INSET_UNITS
                        + INVENTORY_SLOT_COLUMNS as f32 * INVENTORY_SLOT_SIZE_UNITS
                        + INVENTORY_SLOT_COLUMNS.saturating_sub(1) as f32
                            * INVENTORY_SLOT_GAP_UNITS
                        + INVENTORY_RIGHT_PADDING_UNITS,
                    440.0,
                ],
                INVENTORY_INITIAL_CENTER_OFFSET_UNITS,
            ),
            tabs: UiTabs::default(),
            slots: UiSlotGrid::new(
                INVENTORY_SLOT_COLUMNS,
                INVENTORY_SLOT_ROWS,
                INVENTORY_SLOT_GAP_UNITS,
            ),
            currency: UiCurrencyDisplay::default(),
            selected_item: None,
            pressed_item: None,
            pressed_position: None,
            dragging_item: None,
            completed_drag: None,
            completed_click: None,
            slot_hit_regions: Vec::new(),
            item_hit_regions: Vec::new(),
        }
    }
}

pub(crate) struct InventoryWindowFrameInput<'a> {
    pub(crate) window_assets: UiWindowAssets,
    pub(crate) tab_assets: UiTabAssets,
    pub(crate) slot_assets: UiSlotAssets,
    pub(crate) item_icon_assets: &'a UiItemIconAssets,
    pub(crate) entries: &'a [InventoryEntry],
    pub(crate) registry: &'a ContentRegistry,
    pub(crate) viewport: PixelViewport,
    pub(crate) pixels_per_unit: f32,
    pub(crate) cursor: Option<[f32; 2]>,
}

impl InventoryWindow {
    pub(crate) fn is_visible(&self) -> bool {
        self.chrome.is_visible()
    }

    pub(crate) fn close(&mut self) {
        self.chrome.close();
    }

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
        input: InventoryWindowFrameInput<'_>,
    ) -> Result<Option<InventoryWindowFrame>, String> {
        let InventoryWindowFrameInput {
            window_assets,
            tab_assets,
            slot_assets,
            item_icon_assets,
            entries,
            registry,
            viewport,
            pixels_per_unit,
            cursor,
        } = input;
        let Some(layout) = window_assets.layout(&mut self.chrome, viewport, pixels_per_unit)?
        else {
            self.slot_hit_regions.clear();
            self.item_hit_regions.clear();
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
            self.slot_hit_regions.clear();
            self.item_hit_regions.clear();
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
        let slot_origin = inventory_slot_origin(
            window_assets,
            slot_assets,
            self.slots,
            layout,
            tab_bounds,
            pixels_per_unit,
        )?;
        let mut slot_rects = slot_assets.frame(self.slots, slot_origin, pixels_per_unit)?;
        let category = INVENTORY_TAB_CATEGORIES[self.tabs.selected_index()];
        let visible = visible_inventory_entries(registry, entries, category, slot_rects.len());
        self.slot_hit_regions = slot_rects
            .iter()
            .map(|slot| ScreenRect {
                min: slot.min,
                max: slot.max,
            })
            .collect();
        self.item_hit_regions = visible
            .iter()
            .zip(self.slot_hit_regions.iter().copied())
            .map(|(entry, bounds)| (bounds, entry.item_instance_id))
            .collect();
        if self.selected_item.is_some_and(|selected| {
            !entries
                .iter()
                .any(|entry| entry.item_instance_id == selected)
        }) {
            self.selected_item = None;
        }
        let hovered_item = cursor.and_then(|cursor| self.item_at(cursor));
        for (index, slot) in slot_rects.iter_mut().enumerate() {
            let item = self.item_hit_regions.get(index).map(|(_, item)| *item);
            slot.tint = if item.is_some() && item == self.selected_item {
                INVENTORY_SLOT_SELECTED_TINT
            } else if item.is_some() && item == hovered_item {
                INVENTORY_SLOT_HOVER_TINT
            } else {
                [1.0; 4]
            };
        }
        let item_frame = inventory_items_frame(
            registry,
            item_icon_assets,
            entries,
            category,
            &slot_rects,
            pixels_per_unit,
            self.dragging_item,
        )?;
        let tooltip = hovered_item
            .and_then(|item| entries.iter().find(|entry| entry.item_instance_id == item))
            .map(|entry| {
                inventory_tooltip_frame(
                    window_assets,
                    registry,
                    entry,
                    cursor.expect("hovered item requires cursor"),
                    viewport,
                    pixels_per_unit,
                )
            })
            .transpose()?;
        let grid_chrome = inventory_grid_chrome(
            window_assets,
            slot_assets,
            self.slots,
            layout,
            tab_bounds,
            slot_origin,
            pixels_per_unit,
        )?;
        let currency_bounds = inventory_currency_bounds(
            window_assets,
            slot_assets,
            self.slots,
            layout,
            slot_origin,
            pixels_per_unit,
        )?;
        let currency_texts = self.currency.frame(currency_bounds, pixels_per_unit)?;
        let mut textured_rects = window_frame.textured_rects;
        textured_rects.extend(grid_chrome);
        textured_rects.extend(tab_frame.textured_rects);
        textured_rects.extend(slot_rects);
        textured_rects.extend(item_frame.textured_rects);
        let tooltip_text_count = tooltip.as_ref().map_or(0, |tooltip| tooltip.texts.len());
        let mut texts = Vec::with_capacity(
            1 + tab_frame.texts.len()
                + currency_texts.len()
                + item_frame.texts.len()
                + tooltip_text_count,
        );
        texts.push(window_frame.title);
        texts.extend(tab_frame.texts);
        texts.extend(currency_texts);
        texts.extend(item_frame.texts);
        if let Some(tooltip) = tooltip {
            textured_rects.push(tooltip.background);
            texts.extend(tooltip.texts);
        }
        if let (Some(item), Some(cursor)) = (self.dragging_item, cursor)
            && let Some(entry) = entries.iter().find(|entry| entry.item_instance_id == item)
        {
            textured_rects.push(drag_preview_icon(
                item_icon_assets.resolve(entry.definition),
                cursor,
                pixels_per_unit,
            )?);
        }
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
        self.update_drag(cursor);
        self.chrome.pointer_moved(cursor, viewport, pixels_per_unit) || self.is_dragging()
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
        let selected_tab_before = self.tabs.selected_index();
        if let Ok(tab_bounds) =
            inventory_tab_bounds(window_assets, tab_assets, layout, pixels_per_unit)
            && self.tabs.apply_pointer_button(
                state,
                cursor,
                tab_bounds,
                INVENTORY_TAB_LABELS.len(),
                tab_assets.gap_units * pixels_per_unit,
            )
        {
            self.pressed_item = None;
            if state == ElementState::Released && self.tabs.selected_index() != selected_tab_before
            {
                self.selected_item = None;
            }
            return true;
        }

        let hit_item = cursor.and_then(|cursor| self.item_at(cursor));
        let hit_slot = cursor.is_some_and(|cursor| {
            self.slot_hit_regions
                .iter()
                .copied()
                .any(|slot| slot.contains(cursor))
        });
        match state {
            ElementState::Pressed if hit_slot => {
                self.pressed_item = hit_item;
                self.pressed_position = cursor;
                self.dragging_item = None;
                if hit_item.is_none() {
                    self.selected_item = None;
                }
                return true;
            }
            ElementState::Released => {
                let pressed = self.pressed_item.take();
                self.pressed_position = None;
                if let Some(pressed) = pressed {
                    if self.dragging_item.take().is_some() {
                        self.completed_drag = Some(pressed);
                    } else if hit_item == Some(pressed) {
                        self.selected_item = Some(pressed);
                        self.completed_click = Some(pressed);
                    }
                    return true;
                }
                if hit_slot {
                    return true;
                }
            }
            ElementState::Pressed => {}
        }

        self.chrome
            .apply_pointer_button(window_assets, state, cursor, viewport, pixels_per_unit)
    }

    pub(crate) fn cancel_pointer_interaction(&mut self) {
        self.chrome.cancel_pointer_interaction();
        self.tabs.cancel_pointer_interaction();
        self.pressed_item = None;
        self.pressed_position = None;
        self.dragging_item = None;
        self.completed_drag = None;
        self.completed_click = None;
    }

    pub(crate) fn take_completed_drag(&mut self) -> Option<ItemInstanceId> {
        self.completed_drag.take()
    }

    pub(crate) fn take_completed_click(&mut self) -> Option<ItemInstanceId> {
        self.completed_click.take()
    }

    pub(crate) fn contains_window(
        &mut self,
        window_assets: UiWindowAssets,
        cursor: [f32; 2],
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        window_assets
            .layout(&mut self.chrome, viewport, pixels_per_unit)
            .ok()
            .flatten()
            .is_some_and(|layout| layout.window.contains(cursor))
    }

    pub(crate) fn is_dragging(&self) -> bool {
        self.dragging_item.is_some()
    }

    fn update_drag(&mut self, cursor: [f32; 2]) {
        let Some(item) = self.pressed_item else {
            return;
        };
        let Some(origin) = self.pressed_position else {
            return;
        };
        let dx = cursor[0] - origin[0];
        let dy = cursor[1] - origin[1];
        if dx.mul_add(dx, dy * dy) >= ITEM_DRAG_THRESHOLD_PX.powi(2) {
            self.dragging_item = Some(item);
        }
    }

    fn item_at(&self, cursor: [f32; 2]) -> Option<ItemInstanceId> {
        self.item_hit_regions
            .iter()
            .find_map(|(bounds, item)| bounds.contains(cursor).then_some(*item))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DragSource {
    Inventory(ItemInstanceId),
    Equipped(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DragDestination {
    Inventory,
    Equipment(u8),
    EquipmentWindow,
    Outside,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DragResolution {
    Equip {
        item_instance_id: ItemInstanceId,
        slot: u8,
    },
    Unequip {
        slot: u8,
    },
    Drop {
        item_instance_id: ItemInstanceId,
    },
    Noop,
}

pub(crate) fn resolve_drag(
    source: DragSource,
    destination: DragDestination,
    inventory_equipment_slot: Option<u8>,
) -> DragResolution {
    match (source, destination) {
        (DragSource::Inventory(item_instance_id), DragDestination::Equipment(slot))
            if inventory_equipment_slot == Some(slot) =>
        {
            DragResolution::Equip {
                item_instance_id,
                slot,
            }
        }
        (DragSource::Equipped(slot), DragDestination::Inventory) => {
            DragResolution::Unequip { slot }
        }
        (DragSource::Inventory(item_instance_id), DragDestination::Outside) => {
            DragResolution::Drop { item_instance_id }
        }
        _ => DragResolution::Noop,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EquipmentWindow {
    chrome: ProofPanelWindow,
    slots: UiSlotGrid,
    slot_hit_regions: Vec<ScreenRect>,
    occupied_slots: [Option<ContentId>; EquipmentSlot::COUNT],
    pressed_slot: Option<u8>,
    pressed_position: Option<[f32; 2]>,
    dragging_slot: Option<u8>,
    completed_drag: Option<u8>,
    completed_click: Option<u8>,
}

pub(crate) struct EquipmentWindowFrameInput<'a> {
    pub(crate) window_assets: UiWindowAssets,
    pub(crate) slot_assets: UiSlotAssets,
    pub(crate) item_icon_assets: &'a UiItemIconAssets,
    pub(crate) equipment: Option<ReplicatedEquipment>,
    pub(crate) viewport: PixelViewport,
    pub(crate) pixels_per_unit: f32,
    pub(crate) cursor: Option<[f32; 2]>,
}

impl Default for EquipmentWindow {
    fn default() -> Self {
        Self {
            chrome: ProofPanelWindow::with_size_and_center_offset(
                [250.0, 300.0],
                EQUIPMENT_INITIAL_CENTER_OFFSET_UNITS,
            ),
            slots: UiSlotGrid::new(
                EQUIPMENT_SLOT_COLUMNS,
                EQUIPMENT_SLOT_ROWS,
                EQUIPMENT_SLOT_GAP_UNITS,
            ),
            slot_hit_regions: Vec::new(),
            occupied_slots: [None; EquipmentSlot::COUNT],
            pressed_slot: None,
            pressed_position: None,
            dragging_slot: None,
            completed_drag: None,
            completed_click: None,
        }
    }
}

impl EquipmentWindow {
    pub(crate) fn is_visible(&self) -> bool {
        self.chrome.is_visible()
    }

    pub(crate) fn close(&mut self) {
        self.chrome.close();
    }

    pub(crate) fn apply_key(
        &mut self,
        physical_key: PhysicalKey,
        state: ElementState,
        repeat: bool,
    ) -> bool {
        if physical_key != PhysicalKey::Code(KeyCode::KeyO) {
            return false;
        }
        self.chrome
            .apply_key(PhysicalKey::Code(KeyCode::KeyI), state, repeat)
    }

    pub(crate) fn frame(
        &mut self,
        input: EquipmentWindowFrameInput<'_>,
    ) -> Result<Option<EquipmentWindowFrame>, String> {
        let EquipmentWindowFrameInput {
            window_assets,
            slot_assets,
            item_icon_assets,
            equipment,
            viewport,
            pixels_per_unit,
            cursor,
        } = input;
        let Some(layout) = window_assets.layout(&mut self.chrome, viewport, pixels_per_unit)?
        else {
            self.slot_hit_regions.clear();
            return Ok(None);
        };
        let Some(window_frame) = window_assets.proof_frame(
            &mut self.chrome,
            "Equipment",
            viewport,
            pixels_per_unit,
            cursor,
        )?
        else {
            self.slot_hit_regions.clear();
            return Ok(None);
        };
        let origin = equipment_slot_origin(
            window_assets,
            slot_assets,
            self.slots,
            layout,
            pixels_per_unit,
        )?;
        let mut slot_rects = slot_assets.frame(self.slots, origin, pixels_per_unit)?;
        self.slot_hit_regions = slot_rects
            .iter()
            .map(|slot| ScreenRect {
                min: slot.min,
                max: slot.max,
            })
            .collect();
        self.occupied_slots =
            std::array::from_fn(|index| equipment.and_then(|state| state.get(index as u8)));
        for (index, slot) in slot_rects.iter_mut().enumerate() {
            slot.tint = if cursor.is_some_and(|point| {
                self.slot_hit_regions
                    .get(index)
                    .is_some_and(|bounds| bounds.contains(point))
            }) {
                INVENTORY_SLOT_HOVER_TINT
            } else {
                [1.0; 4]
            };
        }
        let item_frame = equipment_items_frame(
            item_icon_assets,
            &self.occupied_slots,
            &slot_rects,
            pixels_per_unit,
            self.dragging_slot,
        )?;

        let labels: Vec<_> = slot_rects
            .iter()
            .zip(EQUIPMENT_SLOT_LABELS)
            .map(|(slot, label)| TextBlock {
                content: TextContent(label.to_string()),
                style: TextStyle::at_size(
                    EQUIPMENT_LABEL_FONT_SIZE_UNITS,
                    EQUIPMENT_LABEL_COLOR,
                    TextAlignment::Center,
                ),
                anchor: [
                    (slot.min[0] + slot.max[0]) * 0.5,
                    slot.max[1]
                        - (EQUIPMENT_LABEL_FONT_SIZE_UNITS + EQUIPMENT_LABEL_GAP_UNITS)
                            * pixels_per_unit,
                ],
                max_width: Some((slot.max[0] - slot.min[0] - 4.0 * pixels_per_unit).max(1.0)),
            })
            .collect();
        let mut textured_rects = window_frame.textured_rects;
        textured_rects.extend(slot_rects);
        textured_rects.extend(item_frame.textured_rects);
        let mut texts = vec![window_frame.title];
        texts.extend(labels);
        texts.extend(item_frame.texts);
        if let (Some(slot), Some(cursor)) = (self.dragging_slot, cursor)
            && let Some(definition) = self.occupied_slots[usize::from(slot)]
        {
            textured_rects.push(drag_preview_icon(
                item_icon_assets.resolve(definition),
                cursor,
                pixels_per_unit,
            )?);
        }
        Ok(Some(EquipmentWindowFrame {
            textured_rects,
            texts,
        }))
    }

    pub(crate) fn apply_pointer_button(
        &mut self,
        window_assets: UiWindowAssets,
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
        let hit_slot = cursor.and_then(|point| self.slot_at(point));
        match state {
            ElementState::Pressed if hit_slot.is_some() => {
                self.pressed_slot =
                    hit_slot.filter(|slot| self.occupied_slots[usize::from(*slot)].is_some());
                self.pressed_position = cursor;
                self.dragging_slot = None;
                return true;
            }
            ElementState::Released => {
                let pressed = self.pressed_slot.take();
                self.pressed_position = None;
                if let Some(slot) = pressed {
                    if self.dragging_slot.take().is_some() {
                        self.completed_drag = Some(slot);
                    } else if hit_slot == Some(slot) {
                        self.completed_click = Some(slot);
                    }
                    return true;
                }
                if hit_slot.is_some() {
                    return true;
                }
            }
            ElementState::Pressed => {}
        }
        self.chrome
            .apply_pointer_button(window_assets, state, cursor, viewport, pixels_per_unit)
            || layout.window.contains(cursor.unwrap_or([-1.0, -1.0]))
    }

    pub(crate) fn pointer_moved(
        &mut self,
        cursor: [f32; 2],
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        self.update_drag(cursor);
        self.chrome.pointer_moved(cursor, viewport, pixels_per_unit) || self.is_dragging()
    }

    pub(crate) fn cancel_pointer_interaction(&mut self) {
        self.chrome.cancel_pointer_interaction();
        self.pressed_slot = None;
        self.pressed_position = None;
        self.dragging_slot = None;
        self.completed_drag = None;
        self.completed_click = None;
    }

    pub(crate) fn take_completed_drag(&mut self) -> Option<u8> {
        self.completed_drag.take()
    }

    pub(crate) fn take_completed_click(&mut self) -> Option<u8> {
        self.completed_click.take()
    }

    pub(crate) fn slot_at(&self, cursor: [f32; 2]) -> Option<u8> {
        self.slot_hit_regions
            .iter()
            .position(|bounds| bounds.contains(cursor))
            .and_then(|index| u8::try_from(index).ok())
    }

    pub(crate) fn contains_window(
        &mut self,
        window_assets: UiWindowAssets,
        cursor: [f32; 2],
        viewport: PixelViewport,
        pixels_per_unit: f32,
    ) -> bool {
        window_assets
            .layout(&mut self.chrome, viewport, pixels_per_unit)
            .ok()
            .flatten()
            .is_some_and(|layout| layout.window.contains(cursor))
    }

    pub(crate) fn is_dragging(&self) -> bool {
        self.dragging_slot.is_some()
    }

    fn update_drag(&mut self, cursor: [f32; 2]) {
        let Some(slot) = self.pressed_slot else {
            return;
        };
        let Some(origin) = self.pressed_position else {
            return;
        };
        let dx = cursor[0] - origin[0];
        let dy = cursor[1] - origin[1];
        if dx.mul_add(dx, dy * dy) >= ITEM_DRAG_THRESHOLD_PX.powi(2) {
            self.dragging_slot = Some(slot);
        }
    }
}

pub(crate) struct EquipmentWindowFrame {
    pub(crate) textured_rects: Vec<UiTexturedRect>,
    pub(crate) texts: Vec<TextBlock>,
}

struct UiInventoryItemsFrame {
    textured_rects: Vec<UiTexturedRect>,
    texts: Vec<TextBlock>,
}

struct UiInventoryTooltipFrame {
    background: UiTexturedRect,
    texts: Vec<TextBlock>,
}

fn visible_inventory_entries<'a>(
    registry: &ContentRegistry,
    entries: &'a [InventoryEntry],
    category: ItemCategory,
    capacity: usize,
) -> Vec<&'a InventoryEntry> {
    let mut visible: Vec<&InventoryEntry> = entries
        .iter()
        .filter(|entry| {
            registry
                .item_by_id(entry.definition)
                .map(|definition| definition.category)
                .unwrap_or(ItemCategory::Misc)
                == category
        })
        .collect();
    visible.sort_by_key(|entry| entry.slot);
    visible.truncate(capacity);
    visible
}

fn inventory_items_frame(
    registry: &ContentRegistry,
    item_icon_assets: &UiItemIconAssets,
    entries: &[InventoryEntry],
    category: ItemCategory,
    slots: &[UiTexturedRect],
    pixels_per_unit: f32,
    dragging_item: Option<ItemInstanceId>,
) -> Result<UiInventoryItemsFrame, String> {
    validate_pixels_per_unit(pixels_per_unit)?;
    let visible = visible_inventory_entries(registry, entries, category, slots.len());

    let mut textured_rects = Vec::with_capacity(visible.len());
    let mut texts = Vec::new();
    for (entry, slot) in visible.into_iter().zip(slots.iter().copied()) {
        if dragging_item == Some(entry.item_instance_id) {
            continue;
        }
        let icon = item_icon_assets.resolve(entry.definition);
        let icon_bounds = fit_item_icon(slot, icon.dimensions_px, pixels_per_unit)?;
        textured_rects.push(UiTexturedRect {
            min: icon_bounds.min,
            max: icon_bounds.max,
            texture: icon.texture,
            uv_min: icon.uv_min,
            uv_max: icon.uv_max,
            tint: [1.0; 4],
        });
        if entry.quantity > 1 {
            let font_size = INVENTORY_QUANTITY_FONT_SIZE_UNITS * pixels_per_unit;
            let inset = INVENTORY_QUANTITY_INSET_UNITS * pixels_per_unit;
            texts.push(TextBlock {
                content: TextContent(entry.quantity.to_string()),
                style: TextStyle::at_size(
                    INVENTORY_QUANTITY_FONT_SIZE_UNITS,
                    INVENTORY_QUANTITY_COLOR,
                    TextAlignment::Right,
                ),
                anchor: [slot.max[0] - inset, slot.max[1] - font_size - inset],
                max_width: Some((slot.max[0] - slot.min[0] - inset * 2.0).max(1.0)),
            });
        }
    }
    Ok(UiInventoryItemsFrame {
        textured_rects,
        texts,
    })
}

struct UiEquipmentItemsFrame {
    textured_rects: Vec<UiTexturedRect>,
    texts: Vec<TextBlock>,
}

fn equipment_items_frame(
    item_icon_assets: &UiItemIconAssets,
    occupied_slots: &[Option<ContentId>; EquipmentSlot::COUNT],
    slots: &[UiTexturedRect],
    pixels_per_unit: f32,
    dragging_slot: Option<u8>,
) -> Result<UiEquipmentItemsFrame, String> {
    validate_pixels_per_unit(pixels_per_unit)?;
    let mut textured_rects = Vec::new();
    for (index, (definition, slot)) in occupied_slots.iter().zip(slots.iter().copied()).enumerate()
    {
        if dragging_slot == u8::try_from(index).ok() {
            continue;
        }
        let Some(definition) = definition else {
            continue;
        };
        let icon = item_icon_assets.resolve(*definition);
        let icon_bounds = fit_item_icon(slot, icon.dimensions_px, pixels_per_unit)?;
        textured_rects.push(UiTexturedRect {
            min: icon_bounds.min,
            max: icon_bounds.max,
            texture: icon.texture,
            uv_min: icon.uv_min,
            uv_max: icon.uv_max,
            tint: [1.0; 4],
        });
    }
    Ok(UiEquipmentItemsFrame {
        textured_rects,
        texts: Vec::new(),
    })
}

fn drag_preview_icon(
    icon: UiItemIconVisual,
    cursor: [f32; 2],
    pixels_per_unit: f32,
) -> Result<UiTexturedRect, String> {
    let half_slot = 20.0 * pixels_per_unit;
    let slot = UiTexturedRect {
        min: [cursor[0] - half_slot, cursor[1] - half_slot],
        max: [cursor[0] + half_slot, cursor[1] + half_slot],
        texture: icon.texture,
        uv_min: icon.uv_min,
        uv_max: icon.uv_max,
        tint: [1.0; 4],
    };
    let bounds = fit_item_icon(slot, icon.dimensions_px, pixels_per_unit)?;
    Ok(UiTexturedRect {
        min: bounds.min,
        max: bounds.max,
        texture: icon.texture,
        uv_min: icon.uv_min,
        uv_max: icon.uv_max,
        tint: [1.0; 4],
    })
}

fn inventory_tooltip_frame(
    window_assets: UiWindowAssets,
    registry: &ContentRegistry,
    entry: &InventoryEntry,
    cursor: [f32; 2],
    viewport: PixelViewport,
    pixels_per_unit: f32,
) -> Result<UiInventoryTooltipFrame, String> {
    validate_pixels_per_unit(pixels_per_unit)?;
    let width = INVENTORY_TOOLTIP_WIDTH_UNITS * pixels_per_unit;
    let height = INVENTORY_TOOLTIP_HEIGHT_UNITS * pixels_per_unit;
    let offset = INVENTORY_TOOLTIP_OFFSET_UNITS * pixels_per_unit;
    let viewport_min = [viewport.x as f32, viewport.y as f32];
    let viewport_max = [
        viewport.x as f32 + viewport.width as f32,
        viewport.y as f32 + viewport.height as f32,
    ];
    let mut min = [cursor[0] + offset, cursor[1] + offset];
    if min[0] + width > viewport_max[0] {
        min[0] = (cursor[0] - offset - width).max(viewport_min[0]);
    }
    if min[1] + height > viewport_max[1] {
        min[1] = (cursor[1] - offset - height).max(viewport_min[1]);
    }
    min[0] = min[0].clamp(
        viewport_min[0],
        (viewport_max[0] - width).max(viewport_min[0]),
    );
    min[1] = min[1].clamp(
        viewport_min[1],
        (viewport_max[1] - height).max(viewport_min[1]),
    );
    let bounds = ScreenRect {
        min,
        max: [min[0] + width, min[1] + height],
    };
    let padding = INVENTORY_TOOLTIP_PADDING_UNITS * pixels_per_unit;
    let font_size = INVENTORY_TOOLTIP_FONT_SIZE_UNITS * pixels_per_unit;
    let line_gap = INVENTORY_TOOLTIP_LINE_GAP_UNITS * pixels_per_unit;
    let text_width = (width - padding * 2.0).max(1.0);
    let (title, detail) = if let Some(definition) = registry.item_by_id(entry.definition) {
        (
            definition.authored_id.clone(),
            format!(
                "{} | Qty {} | Stack {}",
                definition.category.as_str(),
                entry.quantity,
                definition.stack_limit
            ),
        )
    } else {
        (
            "Unknown item".to_string(),
            format!("misc | Qty {} | definition unavailable", entry.quantity),
        )
    };
    let title_anchor = [bounds.min[0] + padding, bounds.min[1] + padding];
    let detail_anchor = [title_anchor[0], title_anchor[1] + font_size + line_gap];
    Ok(UiInventoryTooltipFrame {
        background: panel_center_fill(
            window_assets.panel(),
            bounds,
            INVENTORY_TOOLTIP_BACKGROUND_TINT,
        ),
        texts: vec![
            TextBlock {
                content: TextContent(title),
                style: TextStyle::at_size(
                    INVENTORY_TOOLTIP_FONT_SIZE_UNITS,
                    INVENTORY_TOOLTIP_TITLE_COLOR,
                    TextAlignment::Left,
                ),
                anchor: title_anchor,
                max_width: Some(text_width),
            },
            TextBlock {
                content: TextContent(detail),
                style: TextStyle::at_size(
                    INVENTORY_TOOLTIP_FONT_SIZE_UNITS,
                    INVENTORY_TOOLTIP_DETAIL_COLOR,
                    TextAlignment::Left,
                ),
                anchor: detail_anchor,
                max_width: Some(text_width),
            },
        ],
    })
}

fn fit_item_icon(
    slot: UiTexturedRect,
    dimensions_px: [u32; 2],
    pixels_per_unit: f32,
) -> Result<ScreenRect, String> {
    validate_pixels_per_unit(pixels_per_unit)?;
    if dimensions_px.contains(&0) {
        return Err("inventory item icon dimensions must be non-zero".to_string());
    }
    let inset = INVENTORY_ICON_INSET_UNITS * pixels_per_unit;
    let available = [
        slot.max[0] - slot.min[0] - inset * 2.0,
        slot.max[1] - slot.min[1] - inset * 2.0,
    ];
    if available[0] <= 0.0 || available[1] <= 0.0 {
        return Err("inventory slot is too small for an item icon".to_string());
    }
    let scale =
        (available[0] / dimensions_px[0] as f32).min(available[1] / dimensions_px[1] as f32);
    let size = [
        dimensions_px[0] as f32 * scale,
        dimensions_px[1] as f32 * scale,
    ];
    let min = [
        slot.min[0] + (slot.max[0] - slot.min[0] - size[0]) * 0.5,
        slot.min[1] + (slot.max[1] - slot.min[1] - size[1]) * 0.5,
    ];
    Ok(ScreenRect {
        min,
        max: [min[0] + size[0], min[1] + size[1]],
    })
}

fn inventory_grid_chrome(
    window_assets: UiWindowAssets,
    slot_assets: UiSlotAssets,
    grid: UiSlotGrid,
    layout: UiWindowLayout,
    tab_bounds: ScreenRect,
    slot_origin: [f32; 2],
    pixels_per_unit: f32,
) -> Result<[UiTexturedRect; 2], String> {
    validate_pixels_per_unit(pixels_per_unit)?;
    let grid_size = grid.logical_size(slot_assets)?;
    let separator = ScreenRect {
        min: [tab_bounds.min[0], tab_bounds.max[1]],
        max: [
            tab_bounds.max[0],
            tab_bounds.max[1] + INVENTORY_TAB_SEPARATOR_HEIGHT_UNITS * pixels_per_unit,
        ],
    };
    let background = ScreenRect {
        min: [
            slot_origin[0] - INVENTORY_GRID_SIDE_PADDING_UNITS * pixels_per_unit,
            separator.max[1],
        ],
        max: [
            slot_origin[0] + (grid_size[0] + INVENTORY_GRID_SIDE_PADDING_UNITS) * pixels_per_unit,
            slot_origin[1] + (grid_size[1] + INVENTORY_GRID_BOTTOM_PADDING_UNITS) * pixels_per_unit,
        ],
    };
    let content_max_y =
        layout.window.max[1] - window_assets.panel().border_units.bottom * pixels_per_unit;
    if separator.max[1] > slot_origin[1]
        || background.min[0] < tab_bounds.min[0]
        || background.max[0] > tab_bounds.max[0]
        || background.width() <= 0.0
        || background.height() <= 0.0
        || background.max[1] > content_max_y
    {
        return Err("inventory grid chrome does not fit inside the panel content".to_string());
    }
    Ok([
        panel_center_fill(
            window_assets.panel(),
            background,
            INVENTORY_GRID_BACKGROUND_TINT,
        ),
        panel_center_fill(
            window_assets.panel(),
            separator,
            INVENTORY_TAB_SEPARATOR_TINT,
        ),
    ])
}

fn panel_center_fill(panel: PanelAsset, bounds: ScreenRect, tint: [f32; 4]) -> UiTexturedRect {
    let sample_size = [
        panel.source_size_px[0].min(4),
        panel.source_size_px[1].min(4),
    ];
    let source = SourceRectPx {
        x: panel.source_rect.x + (panel.source_rect.width - sample_size[0]) / 2,
        y: panel.source_rect.y + (panel.source_rect.height - sample_size[1]) / 2,
        width: sample_size[0],
        height: sample_size[1],
    };
    let (uv_min, uv_max) = source.uv_bounds(panel.source_size_px);
    UiTexturedRect {
        min: bounds.min,
        max: bounds.max,
        texture: panel.texture,
        uv_min,
        uv_max,
        tint,
    }
}

fn inventory_currency_bounds(
    window_assets: UiWindowAssets,
    slot_assets: UiSlotAssets,
    grid: UiSlotGrid,
    layout: UiWindowLayout,
    slot_origin: [f32; 2],
    pixels_per_unit: f32,
) -> Result<ScreenRect, String> {
    validate_pixels_per_unit(pixels_per_unit)?;
    let grid_size = grid.logical_size(slot_assets)?;
    let inset = INVENTORY_CURRENCY_VERTICAL_INSET_UNITS * pixels_per_unit;
    let bounds = ScreenRect {
        min: [
            layout.window.min[0] + INVENTORY_CONTENT_SIDE_INSET_UNITS * pixels_per_unit,
            slot_origin[1] + grid_size[1] * pixels_per_unit + inset,
        ],
        max: [
            layout.window.max[0] - INVENTORY_CONTENT_SIDE_INSET_UNITS * pixels_per_unit,
            layout.window.max[1]
                - window_assets.panel().border_units.bottom * pixels_per_unit
                - inset,
        ],
    };
    if bounds.width() <= 0.0 || bounds.height() < CURRENCY_FONT_SIZE_UNITS * pixels_per_unit {
        return Err("inventory window is too small for its currency row".to_string());
    }
    Ok(bounds)
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
            layout.window.min[0] + INVENTORY_CONTENT_SIDE_INSET_UNITS * pixels_per_unit,
            min_y,
        ],
        max: [
            layout.window.max[0] - INVENTORY_CONTENT_SIDE_INSET_UNITS * pixels_per_unit,
            min_y + tab_assets.height_units * pixels_per_unit,
        ],
    };
    let tab_count = INVENTORY_TAB_LABELS.len();
    let minimum_width = ((tab_assets.cap_units.left + tab_assets.cap_units.right)
        * tab_count as f32
        + tab_assets.gap_units * tab_count.saturating_sub(1) as f32)
        * pixels_per_unit;
    if bounds.width() <= minimum_width
        || bounds.max[1]
            >= layout.window.max[1] - window_assets.panel().border_units.bottom * pixels_per_unit
    {
        return Err("inventory window is too small for its tab strip".to_string());
    }
    Ok(bounds)
}

fn inventory_slot_origin(
    window_assets: UiWindowAssets,
    slot_assets: UiSlotAssets,
    grid: UiSlotGrid,
    layout: UiWindowLayout,
    tab_bounds: ScreenRect,
    pixels_per_unit: f32,
) -> Result<[f32; 2], String> {
    validate_pixels_per_unit(pixels_per_unit)?;
    let grid_size = grid.logical_size(slot_assets)?;
    let content_min_x = layout.window.min[0] + INVENTORY_CONTENT_SIDE_INSET_UNITS * pixels_per_unit;
    let content_max_x = layout.window.max[0] - INVENTORY_CONTENT_SIDE_INSET_UNITS * pixels_per_unit;
    let origin = [
        content_min_x
            + ((content_max_x - content_min_x) / pixels_per_unit - grid_size[0])
                * 0.5
                * pixels_per_unit,
        tab_bounds.max[1] + INVENTORY_SLOT_TOP_GAP_UNITS * pixels_per_unit,
    ];
    let max = [
        origin[0] + grid_size[0] * pixels_per_unit,
        origin[1] + grid_size[1] * pixels_per_unit,
    ];
    let content_max_y =
        layout.window.max[1] - window_assets.panel().border_units.bottom * pixels_per_unit;
    let grid_max_y = content_max_y - INVENTORY_FOOTER_RESERVED_UNITS * pixels_per_unit;
    if origin[0] < content_min_x || max[0] > content_max_x || max[1] > grid_max_y {
        return Err("inventory window is too small for its slot grid".to_string());
    }
    Ok(origin)
}

fn equipment_slot_origin(
    window_assets: UiWindowAssets,
    slot_assets: UiSlotAssets,
    grid: UiSlotGrid,
    layout: UiWindowLayout,
    pixels_per_unit: f32,
) -> Result<[f32; 2], String> {
    validate_pixels_per_unit(pixels_per_unit)?;
    let grid_size = grid.logical_size(slot_assets)?;
    let content_min_x = layout.window.min[0] + INVENTORY_CONTENT_SIDE_INSET_UNITS * pixels_per_unit;
    let content_max_x = layout.window.max[0] - INVENTORY_CONTENT_SIDE_INSET_UNITS * pixels_per_unit;
    let content_min_y = layout.header.max[1] + 8.0 * pixels_per_unit;
    let content_max_y =
        layout.window.max[1] - window_assets.panel().border_units.bottom * pixels_per_unit;
    let group_height = (grid_size[1] + EQUIPMENT_LABEL_GAP_UNITS + EQUIPMENT_LABEL_FONT_SIZE_UNITS)
        * pixels_per_unit;
    let origin = [
        content_min_x
            + ((content_max_x - content_min_x) / pixels_per_unit - grid_size[0])
                * 0.5
                * pixels_per_unit,
        content_min_y + (content_max_y - content_min_y - group_height).max(0.0) * 0.5,
    ];
    let max_y = origin[1]
        + (grid_size[1] + EQUIPMENT_LABEL_GAP_UNITS + EQUIPMENT_LABEL_FONT_SIZE_UNITS)
            * pixels_per_unit;
    if origin[0] < content_min_x
        || origin[0] + grid_size[0] * pixels_per_unit > content_max_x
        || max_y > content_max_y
    {
        return Err("equipment slots do not fit inside the panel content".to_string());
    }
    Ok(origin)
}

fn tab_rect(bounds: ScreenRect, index: usize, count: usize, gap: f32) -> ScreenRect {
    let width = (bounds.width() - gap * count.saturating_sub(1) as f32) / count as f32;
    let min_x = bounds.min[0] + index as f32 * (width + gap);
    ScreenRect {
        min: [min_x, bounds.min[1]],
        max: [
            if index + 1 == count {
                bounds.max[0]
            } else {
                min_x + width
            },
            bounds.max[1],
        ],
    }
}

fn tab_at(bounds: ScreenRect, count: usize, gap: f32, cursor: [f32; 2]) -> Option<usize> {
    if count == 0 || !bounds.contains(cursor) {
        return None;
    }
    (0..count).find(|&index| tab_rect(bounds, index, count, gap).contains(cursor))
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

fn validate_atlas_metadata(
    metadata: &UiAtlasMetadata,
    source_size_px: [u32; 2],
) -> Result<(), String> {
    validate_schema_and_id(metadata.schema_version, &metadata.id, "atlas")?;
    if source_size_px != metadata.dimensions_px {
        return Err(format!(
            "UI atlas dimensions {:?} do not match decoded texture {:?}",
            metadata.dimensions_px, source_size_px
        ));
    }
    let windows = [metadata.window, metadata.message_window];
    if [metadata.window, metadata.slot, metadata.message_window]
        .into_iter()
        .any(|rect| !source_rect_fits(rect, source_size_px))
        || source_rects_overlap(metadata.window, metadata.slot)
        || source_rects_overlap(metadata.window, metadata.message_window)
        || source_rects_overlap(metadata.slot, metadata.message_window)
        || metadata.window_slice_px.left + metadata.window_slice_px.right >= metadata.window.width
        || metadata.window_slice_px.top + metadata.window_slice_px.bottom >= metadata.window.height
        || metadata.message_window_slice_px.left + metadata.message_window_slice_px.right
            >= metadata.message_window.width
        || metadata.message_window_slice_px.top + metadata.message_window_slice_px.bottom
            >= metadata.message_window.height
        || ![
            metadata.window_border_units.left,
            metadata.window_border_units.right,
            metadata.window_border_units.top,
            metadata.window_border_units.bottom,
            metadata.message_window_border_units.left,
            metadata.message_window_border_units.right,
            metadata.message_window_border_units.top,
            metadata.message_window_border_units.bottom,
        ]
        .into_iter()
        .all(finite_positive)
    {
        return Err("UI atlas window regions or slice geometry are invalid".to_string());
    }
    let button_regions = [
        ("beige", metadata.buttons.beige),
        ("red", metadata.buttons.red),
    ];
    for (name, states) in button_regions {
        let regions = [states.normal, states.hover, states.pressed];
        if !regions
            .into_iter()
            .all(|rect| source_rect_fits(rect, source_size_px))
            || !regions
                .windows(2)
                .all(|pair| pair[0].width == pair[1].width && pair[0].x == pair[1].x)
            || regions.iter().any(|rect| rect.width < 2 || rect.height < 2)
            || regions
                .windows(2)
                .any(|pair| source_rects_overlap(pair[0], pair[1]))
            || metadata.button_slice_px.left + metadata.button_slice_px.right >= states.normal.width
        {
            return Err(format!("UI atlas button variant {name} is invalid"));
        }
    }
    let small_button_regions = [
        ("confirm", metadata.small_buttons.confirm),
        ("close", metadata.small_buttons.close),
    ];
    if !small_button_regions
        .iter()
        .flat_map(|(_, states)| [states.normal, states.hover, states.pressed])
        .all(|rect| source_rect_fits(rect, source_size_px))
        || !metadata.close_size_units.into_iter().all(finite_positive)
        || !finite_non_negative(metadata.close_right_inset_units)
    {
        return Err("UI atlas small-button metadata is invalid".to_string());
    }
    let mut all_regions = windows.to_vec();
    for (_, states) in button_regions {
        all_regions.extend([states.normal, states.hover, states.pressed]);
    }
    for (_, states) in small_button_regions {
        all_regions.extend([states.normal, states.hover, states.pressed]);
    }
    all_regions.extend(metadata.icons.values().copied());
    if metadata.icons.len() != ATLAS_ICON_NAMES.len()
        || metadata
            .icons
            .values()
            .any(|rect| !source_rect_fits(*rect, source_size_px))
    {
        return Err("UI atlas icon metadata is incomplete or out of bounds".to_string());
    }
    if all_regions.iter().enumerate().any(|(index, left)| {
        all_regions
            .iter()
            .skip(index + 1)
            .any(|right| source_rects_overlap(*left, *right))
    }) {
        return Err("UI atlas source regions overlap".to_string());
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

fn source_rects_overlap(left: SourceRectPx, right: SourceRectPx) -> bool {
    left.x < right.x.saturating_add(right.width)
        && right.x < left.x.saturating_add(left.width)
        && left.y < right.y.saturating_add(right.height)
        && right.y < left.y.saturating_add(left.height)
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ProofPanelWindow {
    mode: ProofPanelMode,
    size_units: [f32; 2],
    /// Applied once, relative to a perfectly centered window.
    initial_center_offset_units: [f32; 2],
    /// Relative to the gameplay viewport in logical UI units.
    top_left_units: Option<[f32; 2]>,
    interaction: PointerInteraction,
}

impl Default for ProofPanelWindow {
    fn default() -> Self {
        Self::with_size([282.0, 440.0])
    }
}

impl ProofPanelWindow {
    fn with_size(size_units: [f32; 2]) -> Self {
        Self::with_size_and_center_offset(size_units, [0.0, 0.0])
    }

    fn with_size_and_center_offset(
        size_units: [f32; 2],
        initial_center_offset_units: [f32; 2],
    ) -> Self {
        Self {
            mode: ProofPanelMode::Hidden,
            size_units,
            initial_center_offset_units,
            top_left_units: None,
            interaction: PointerInteraction::None,
        }
    }

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
            _ => return false,
        };
        self.interaction = PointerInteraction::None;
        true
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.mode == ProofPanelMode::Normal
    }

    pub(crate) fn open(&mut self) {
        self.mode = ProofPanelMode::Normal;
        self.interaction = PointerInteraction::None;
    }

    pub(crate) fn close(&mut self) {
        self.mode = ProofPanelMode::Hidden;
        self.interaction = PointerInteraction::None;
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
        let Some(window_size_units) = self.mode.logical_size(self.size_units) else {
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

pub(crate) struct UiMessageChrome {
    pub(crate) frame: UiWindowFrame,
    pub(crate) window: ScreenRect,
    pub(crate) close_button: ScreenRect,
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
    pub(crate) fn width(self) -> f32 {
        self.max[0] - self.min[0]
    }

    pub(crate) fn height(self) -> f32 {
        self.max[1] - self.min[1]
    }

    pub(crate) fn contains(self, point: [f32; 2]) -> bool {
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

#[allow(dead_code)]
fn assemble_nine_slice(
    destination: ScreenRect,
    texture: SpriteTextureId,
    source_size_px: [u32; 2],
    source: SourceInsets,
    borders: DestinationBorders,
    tint: [f32; 4],
) -> Result<Vec<UiTexturedRect>, String> {
    assemble_nine_slice_region(
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
        borders,
        tint,
    )
}

fn assemble_nine_slice_region(
    destination: ScreenRect,
    texture: SpriteTextureId,
    source_size_px: [u32; 2],
    source_rect: SourceRectPx,
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
        || !source_rect_fits(source_rect, source_size_px)
        || source.left + source.right >= source_rect.width
        || source.top + source.bottom >= source_rect.height
    {
        return Err(
            "UI panel target is invalid or smaller than its destination borders".to_string(),
        );
    }

    let source_x = [
        source_rect.x as f32,
        (source_rect.x + source.left) as f32,
        (source_rect.x + source_rect.width - source.right) as f32,
        (source_rect.x + source_rect.width) as f32,
    ];
    let source_y = [
        source_rect.y as f32,
        (source_rect.y + source.top) as f32,
        (source_rect.y + source_rect.height - source.bottom) as f32,
        (source_rect.y + source_rect.height) as f32,
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
    let mut regions = Vec::with_capacity(9);
    for row in 0..3 {
        for column in 0..3 {
            let source_piece = SourceRectPx {
                x: source_x[column] as u32,
                y: source_y[row] as u32,
                width: (source_x[column + 1] - source_x[column]) as u32,
                height: (source_y[row + 1] - source_y[row]) as u32,
            };
            let (uv_min, uv_max) = source_piece.uv_bounds(source_size_px);
            regions.push(UiTexturedRect {
                min: [destination_x[column], destination_y[row]],
                max: [destination_x[column + 1], destination_y[row + 1]],
                texture,
                uv_min,
                uv_max,
                tint,
            });
        }
    }
    Ok(regions)
}

#[allow(dead_code)]
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
    let draw_height = destination.height().min(source_rect.height as f32);
    let draw_max_y = destination.min[1] + draw_height;
    let source_y = [
        source_rect.y as f32,
        (source_rect.y + source_rect.height) as f32,
    ];
    let mut regions = Vec::with_capacity(3);
    for column in 0..3 {
        let source_piece = SourceRectPx {
            x: source_x[column] as u32,
            y: source_y[0] as u32,
            width: (source_x[column + 1] - source_x[column]) as u32,
            height: (source_y[1] - source_y[0]) as u32,
        };
        let (uv_min, uv_max) = source_piece.uv_bounds(texture_size_px);
        regions.push(UiTexturedRect {
            min: [destination_x[column], destination.min[1]],
            max: [destination_x[column + 1], draw_max_y],
            texture,
            uv_min,
            uv_max,
            tint,
        });
    }
    Ok(regions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inventory_registry() -> ContentRegistry {
        purgatory_content::load_registry(
            &purgatory_content::default_content_root(),
            purgatory_content::LoadMode::Shared,
        )
        .unwrap()
    }

    fn placeholder_item_icons(registry: &ContentRegistry) -> UiItemIconAssets {
        UiItemIconAssets::load_placeholder(&mut AssetRuntime::new(), registry).unwrap()
    }

    fn embedded_assets() -> UiWindowAssets {
        UiWindowAssets::load_embedded(&mut AssetRuntime::new()).unwrap()
    }

    fn embedded_button_assets() -> UiButtonAssets {
        UiButtonAssets::load_embedded(&mut AssetRuntime::new()).unwrap()
    }

    fn embedded_tab_assets() -> UiTabAssets {
        UiTabAssets::load_embedded(&mut AssetRuntime::new()).unwrap()
    }

    fn embedded_slot_assets() -> UiSlotAssets {
        UiSlotAssets::load_embedded(&mut AssetRuntime::new()).unwrap()
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
        assert_eq!(assets.panel().source_size_px, [192, 192]);
        assert_eq!(
            assets.panel().source_rect,
            SourceRectPx {
                x: 1,
                y: 0,
                width: 60,
                height: 60
            }
        );
        assert_eq!(assets.close_button.source_size_px, [192, 192]);
        assert_eq!(assets.panel().slice_px.left, 9);
        assert_eq!(assets.panel().slice_px.top, 24);
        assert_eq!(
            assets.panel().border_units,
            DestinationBorders {
                left: 9.0,
                right: 9.0,
                top: 24.0,
                bottom: 9.0,
            }
        );
        for style in PanelStyle::ALL {
            let panel = assets.with_panel_style(style).panel();
            let rect = panel.source_rect;
            assert!(rect.width >= 58);
            assert!(rect.width <= 60);
            assert!(rect.y <= 1);
            assert!(rect.height >= 58);
            assert!(rect.height <= 60);
        }
        assert_eq!(runtime.resource_count(), 1);
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
    fn panel_variants_share_metadata_geometry_and_texture() {
        let assets = embedded_assets();
        let base = assets.panel();
        let textures: Vec<_> = PanelStyle::ALL
            .into_iter()
            .map(|style| {
                let panel = assets.with_panel_style(style).panel();
                assert_eq!(panel.source_size_px, base.source_size_px);
                assert_eq!(panel.source_rect, base.source_rect);
                assert_eq!(panel.slice_px, base.slice_px);
                assert_eq!(panel.border_units, base.border_units);
                assert!(panel.source_rect.width >= 58);
                assert!(panel.source_rect.height >= 58);
                panel.texture
            })
            .collect();
        assert_eq!(textures.len(), 6);
        assert_eq!(
            textures
                .windows(2)
                .filter(|pair| pair[0] != pair[1])
                .count(),
            0
        );
        assert_eq!(base.tint, [1.0; 4]);
        assert!(
            PanelStyle::ALL
                .into_iter()
                .map(PanelStyle::tint)
                .any(|tint| tint != [1.0; 4])
        );
    }

    #[test]
    fn button_states_use_explicit_sheet_regions_and_preserve_three_slice_caps() {
        let button = embedded_button_assets();
        assert_eq!(button.source_size_px, [192, 192]);
        assert_eq!(button.variants[0].normal.y, 69);
        assert_eq!(button.variants[0].hover.y, 87);
        assert_eq!(button.variants[0].hover.height, 18);
        assert_eq!(button.variants[0].pressed.y, 105);
        assert_eq!(button.height_units, 18.0);
        for state in [
            UiButtonState::Normal,
            UiButtonState::Hover,
            UiButtonState::Pressed,
        ] {
            let regions = button
                .frame(
                    ScreenRect {
                        min: [10.0, 20.0],
                        max: [210.0, 52.0],
                    },
                    state,
                    1.0,
                )
                .unwrap();
            assert_eq!(regions.len(), 3);
            assert_eq!(regions[0].size(), [8.0, 18.0]);
            assert_eq!(regions[2].size(), [8.0, 18.0]);
            assert_eq!(regions[1].size(), [184.0, 18.0]);
        }
    }

    #[test]
    fn settings_actions_map_to_existing_display_types() {
        let settings = DisplaySettings::default_dev();
        assert_eq!(
            settings_action(SettingsControl::Fullscreen, settings),
            Some(SettingsAction::SetWindowMode(
                WindowMode::BorderlessFullscreen
            ))
        );
        assert_eq!(
            settings_action(SettingsControl::RenderQuality, settings),
            Some(SettingsAction::SetRenderScale(
                RenderScale::PERFORMANCE_FALLBACK
            ))
        );
        assert_eq!(
            settings_action(SettingsControl::Resolution, settings),
            Some(SettingsAction::SetResolution(RESOLUTION_PRESETS[1]))
        );
        assert_eq!(render_quality_label(RenderScale::DEFAULT), "High");
        assert_eq!(
            render_quality_label(RenderScale::PERFORMANCE_FALLBACK),
            "Performance"
        );
        assert_eq!(
            settings_action(SettingsControl::UiScale, settings),
            Some(SettingsAction::SetUiScale(UiScale::P110))
        );
        #[cfg(feature = "dev-diagnostics")]
        assert_eq!(
            settings_action(SettingsControl::ReturnToLogin, settings),
            Some(SettingsAction::ReturnToLogin)
        );
        assert_eq!(
            settings_action(SettingsControl::ExitGame, settings),
            Some(SettingsAction::ExitGame)
        );

        let mut fullscreen = settings;
        fullscreen.window_mode = WindowMode::BorderlessFullscreen;
        assert!(!settings_control_enabled(
            SettingsControl::Resolution,
            fullscreen
        ));
        assert_eq!(
            settings_action(SettingsControl::Resolution, fullscreen),
            None
        );
    }

    #[test]
    fn settings_window_uses_game_chrome_gear_title_and_close_button() {
        let assets = embedded_assets();
        let buttons = embedded_button_assets();
        let mut settings_window = SettingsWindow::default();
        settings_window.open();
        let frame = settings_window
            .frame(
                assets,
                buttons,
                DisplaySettings::default_dev(),
                viewport(),
                1.0,
                None,
            )
            .unwrap()
            .unwrap();
        assert!(frame.texts.iter().any(|text| text.content.0 == "SETTINGS"));
        assert!(
            frame
                .texts
                .iter()
                .any(|text| text.content.0 == "Render Quality")
        );
        assert!(frame.texts.iter().any(|text| text.content.0 == "Session"));
        #[cfg(feature = "dev-diagnostics")]
        assert!(
            frame
                .texts
                .iter()
                .any(|text| text.content.0 == "Return to Login")
        );
        assert!(frame.texts.iter().any(|text| text.content.0 == "Exit Game"));
        let gear = assets.icons.source("gear").unwrap();
        let gear_uv = gear.uv_bounds(assets.panel().source_size_px);
        assert!(
            frame
                .textured_rects
                .iter()
                .any(|rect| rect.uv_min == gear_uv.0 && rect.uv_max == gear_uv.1)
        );

        let close = assets
            .layout(&mut settings_window.chrome, viewport(), 1.0)
            .unwrap()
            .unwrap()
            .close_button;
        let cursor = close.min;
        assert!(settings_window.apply_pointer_button(
            assets,
            ElementState::Pressed,
            Some(cursor),
            viewport(),
            1.0,
            DisplaySettings::default_dev(),
        ));
        assert!(settings_window.apply_pointer_button(
            assets,
            ElementState::Released,
            Some(cursor),
            viewport(),
            1.0,
            DisplaySettings::default_dev(),
        ));
        assert!(!settings_window.is_visible());
    }

    #[test]
    fn settings_session_buttons_emit_player_actions() {
        let assets = embedded_assets();
        let settings = DisplaySettings::default_dev();
        let mut settings_window = SettingsWindow::default();
        settings_window.open();
        let layout = assets
            .layout(&mut settings_window.chrome, viewport(), 1.0)
            .unwrap()
            .unwrap();

        #[cfg(feature = "dev-diagnostics")]
        let session_actions = [
            (
                SettingsControl::ReturnToLogin,
                SettingsAction::ReturnToLogin,
            ),
            (SettingsControl::ExitGame, SettingsAction::ExitGame),
        ];
        #[cfg(not(feature = "dev-diagnostics"))]
        let session_actions = [(SettingsControl::ExitGame, SettingsAction::ExitGame)];

        for (control, expected) in session_actions {
            let bounds = settings_control_bounds(layout.window, control, 1.0);
            let cursor = [
                (bounds.min[0] + bounds.max[0]) * 0.5,
                (bounds.min[1] + bounds.max[1]) * 0.5,
            ];
            assert!(settings_window.apply_pointer_button(
                assets,
                ElementState::Pressed,
                Some(cursor),
                viewport(),
                1.0,
                settings,
            ));
            assert!(settings_window.apply_pointer_button(
                assets,
                ElementState::Released,
                Some(cursor),
                viewport(),
                1.0,
                settings,
            ));
            assert_eq!(settings_window.take_completed_action(), Some(expected));
        }
    }

    #[test]
    fn settings_launcher_requests_open_only_after_inside_click() {
        let mut launcher = SettingsLauncher::default();
        let bounds = settings_launcher_bounds(viewport(), 1.0).unwrap();
        let cursor = [
            (bounds.min[0] + bounds.max[0]) * 0.5,
            (bounds.min[1] + bounds.max[1]) * 0.5,
        ];
        assert!(launcher.apply_pointer_button(
            ElementState::Pressed,
            Some(cursor),
            viewport(),
            1.0,
        ));
        assert!(launcher.apply_pointer_button(
            ElementState::Released,
            Some(cursor),
            viewport(),
            1.0,
        ));
        assert!(launcher.take_open_requested());
        assert!(!launcher.take_open_requested());
    }

    #[test]
    fn atlas_fixed_chrome_never_upscales_at_supported_ui_scales() {
        let assets = embedded_assets();
        let mut window = normal_window();
        for pixels_per_unit in [1.0, 1.5, 2.0, 3.0] {
            let frame = assets
                .proof_frame(&mut window, "Panel", viewport(), pixels_per_unit, None)
                .unwrap()
                .unwrap();
            let panel = assets.panel();
            for index in [0, 2, 6, 8] {
                let region = frame.textured_rects[index];
                assert!(
                    region.max[0] - region.min[0]
                        <= panel.slice_px.left.max(panel.slice_px.right) as f32
                );
                assert!(
                    region.max[1] - region.min[1]
                        <= panel.slice_px.top.max(panel.slice_px.bottom) as f32
                );
            }
            let close = frame.textured_rects[9];
            let close_source = assets.close_button.states.normal;
            assert!(close.max[0] - close.min[0] <= close_source.width as f32);
            assert!(close.max[1] - close.min[1] <= close_source.height as f32);
        }

        let button = embedded_button_assets();
        for pixels_per_unit in [1.0, 1.5, 2.0, 3.0] {
            let regions = button
                .frame(
                    ScreenRect {
                        min: [10.0, 20.0],
                        max: [410.0, 52.0],
                    },
                    UiButtonState::Normal,
                    pixels_per_unit,
                )
                .unwrap();
            assert!(regions[0].max[0] - regions[0].min[0] <= button.slice_px.left as f32);
            assert!(regions[2].max[0] - regions[2].min[0] <= button.slice_px.right as f32);
        }
    }

    #[test]
    fn atlas_source_partitions_are_disjoint() {
        let metadata: UiAtlasMetadata = serde_json::from_str(ATLAS_METADATA).unwrap();
        let panels = [metadata.window, metadata.slot, metadata.message_window];
        assert!(
            panels
                .windows(2)
                .all(|pair| !source_rects_overlap(pair[0], pair[1]))
        );

        let button = embedded_button_assets();
        for states in [button.variants[0], button.variants[1]] {
            assert!(!source_rects_overlap(states.normal, states.hover));
            assert!(!source_rects_overlap(states.hover, states.pressed));
            assert!(!source_rects_overlap(states.normal, states.pressed));
        }
        let close = metadata.small_buttons.close;
        assert!(!source_rects_overlap(close.normal, close.hover));
        assert!(!source_rects_overlap(close.hover, close.pressed));
        assert!(!source_rects_overlap(close.normal, close.pressed));
    }

    #[test]
    fn inventory_equipment_and_message_windows_resolve_individual_sizes() {
        let assets = embedded_assets();
        let mut inventory = InventoryWindow::default();
        let mut equipment = EquipmentWindow::default();
        inventory.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false,
        );
        equipment.apply_key(
            PhysicalKey::Code(KeyCode::KeyO),
            ElementState::Pressed,
            false,
        );
        let inventory_layout = assets
            .layout(&mut inventory.chrome, viewport(), 1.0)
            .unwrap()
            .unwrap();
        assert_eq!(
            [
                inventory_layout.window.width(),
                inventory_layout.window.height()
            ],
            [276.0, 440.0]
        );
        let equipment_layout = assets
            .layout(&mut equipment.chrome, viewport(), 1.0)
            .unwrap()
            .unwrap();
        assert_eq!(
            [
                equipment_layout.window.width(),
                equipment_layout.window.height()
            ],
            [250.0, 300.0]
        );
        let message = assets
            .message_chrome("Message", viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        assert_eq!(
            [message.window.width(), message.window.height()],
            [400.0, 180.0]
        );
    }

    #[test]
    fn embedded_tab_metadata_parses_and_matches_two_state_sheet() {
        let mut runtime = AssetRuntime::new();
        let assets = UiTabAssets::load_embedded(&mut runtime).unwrap();
        assert_eq!(assets.source_size_px, [192, 192]);
        assert_eq!(assets.states.normal.x, 5);
        assert_eq!(assets.states.normal.width, 50);
        assert_eq!(assets.states.selected.x, 70);
        assert_eq!(assets.states.selected.width, 50);
        assert_eq!(assets.slice_px.left, 8);
        assert_eq!(assets.slice_px.right, 8);
        assert_eq!(assets.height_units, 18.0);
        assert_eq!(assets.font_size_units, 10.5);
        assert_eq!(assets.gap_units, 1.0);
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
    fn embedded_slot_metadata_parses_as_transparent_square() {
        let mut runtime = AssetRuntime::new();
        let assets = UiSlotAssets::load_embedded(&mut runtime).unwrap();
        let image = &runtime.resource(assets.texture).unwrap().image;
        assert_eq!(image.width(), image.height());
        assert!(image.width() > 0);
        assert_eq!(assets.size_units, [44.0, 44.0]);
        assert_eq!(runtime.resource_count(), 1);
        assert_eq!(image.get_pixel(0, 0).0[3], 0);
        assert!(image.get_pixel(image.width() / 2, image.height() / 2).0[3] > 0);
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
        assert_eq!(frame.texts.len(), 4);
        assert_eq!(frame.texts[0].content.0, "First");
        assert_eq!(frame.texts[1].content.0, "Second");
        assert_eq!(frame.texts[2].content.0, "First");
        assert_eq!(frame.texts[3].content.0, "Second");
        assert!(
            ((frame.texts[2].anchor[0] - frame.texts[0].anchor[0]) - 0.7).abs()
                < f32::EPSILON * 32.0
        );
        assert_eq!(
            frame.textured_rects[3].min[0] - frame.textured_rects[2].max[0],
            1.0
        );
        assert_eq!(frame.textured_rects[0].uv_min[0], 70.5 / 192.0);
        assert_eq!(frame.textured_rects[3].uv_min[0], 5.5 / 192.0);
    }

    #[test]
    fn inventory_window_composes_five_tabs_and_changes_selection_on_click_release() {
        let window_assets = embedded_assets();
        let tab_assets = embedded_tab_assets();
        let slot_assets = embedded_slot_assets();
        let registry = inventory_registry();
        let item_icons = placeholder_item_icons(&registry);
        let mut inventory = InventoryWindow::default();
        assert!(inventory.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        let frame = inventory
            .frame(InventoryWindowFrameInput {
                window_assets,
                tab_assets,
                slot_assets,
                item_icon_assets: &item_icons,
                entries: &[],
                registry: &registry,
                viewport: viewport(),
                pixels_per_unit: 1.0,
                cursor: None,
            })
            .unwrap()
            .unwrap();
        assert_eq!(frame.textured_rects.len(), 62);
        assert_eq!(frame.texts.len(), 13);
        assert_eq!(frame.texts[0].content.0, "Item Inventory");
        assert_eq!(frame.texts[1].content.0, "Equip");
        assert_eq!(frame.texts[2].content.0, "Cons.");
        assert_eq!(frame.texts[3].content.0, "Mats");
        assert_eq!(frame.texts[4].content.0, "Tools");
        assert_eq!(frame.texts[5].content.0, "Misc");
        assert_eq!(frame.texts[6].content.0, "Equip");
        assert_eq!(frame.texts[10].content.0, "Misc");
        assert_eq!(frame.texts[11].content.0, "Gold: 0");
        assert_eq!(frame.texts[12].content.0, "Silver: 0");

        let slot_count = INVENTORY_SLOT_COLUMNS * INVENTORY_SLOT_ROWS;
        let slots = &frame.textured_rects[frame.textured_rects.len() - slot_count..];
        assert_eq!(slots.len(), INVENTORY_SLOT_COLUMNS * INVENTORY_SLOT_ROWS);
        assert!(slots.iter().all(|slot| slot.size() == [44.0, 44.0]));
        assert_eq!(slots[1].min[0] - slots[0].max[0], 2.0);
        assert_eq!(slots[INVENTORY_SLOT_COLUMNS].min[1] - slots[0].max[1], 2.0);

        let layout = window_assets
            .layout(&mut inventory.chrome, viewport(), 1.0)
            .unwrap()
            .unwrap();
        let content_max_y = layout.window.max[1] - window_assets.panel().border_units.bottom;
        assert_eq!(content_max_y - slots.last().unwrap().max[1], 61.0);
        let currency_bounds = inventory_currency_bounds(
            window_assets,
            slot_assets,
            inventory.slots,
            layout,
            slots[0].min,
            1.0,
        )
        .unwrap();
        assert_eq!(currency_bounds.min[1] - slots.last().unwrap().max[1], 4.0);
        assert_eq!(content_max_y - currency_bounds.max[1], 4.0);

        let layout = window_assets
            .layout(&mut inventory.chrome, viewport(), 1.0)
            .unwrap()
            .unwrap();
        let bounds = inventory_tab_bounds(window_assets, tab_assets, layout, 1.0).unwrap();
        assert_eq!(layout.window.width(), 276.0);
        assert_eq!(bounds.width(), 230.0);
        assert_eq!(bounds.min[0] - layout.window.min[0], 23.0);
        assert_eq!(layout.window.max[0] - bounds.max[0], 23.0);
        let background = frame.textured_rects[10];
        let separator = frame.textured_rects[11];
        assert_eq!(background.texture, window_assets.panel().texture);
        assert_eq!(separator.texture, window_assets.panel().texture);
        assert_eq!(background.tint, INVENTORY_GRID_BACKGROUND_TINT);
        assert_eq!(separator.tint, INVENTORY_TAB_SEPARATOR_TINT);
        assert_eq!(separator.min, [bounds.min[0], bounds.max[1]]);
        assert_eq!(separator.size(), [bounds.width(), 2.0]);
        assert_eq!(background.min[1], separator.max[1]);
        assert_eq!(slots[0].min[0] - background.min[0], 1.0);
        assert_eq!(
            background.max[0] - slots[INVENTORY_SLOT_COLUMNS - 1].max[0],
            1.0
        );
        assert_eq!(slots[0].min[1] - background.min[1], 2.0);
        assert_eq!(background.max[1] - slots.last().unwrap().max[1], 2.0);
        assert_eq!(slots[0].min[0] - bounds.min[0], 1.0);
        assert_eq!(
            bounds.max[0] - slots[INVENTORY_SLOT_COLUMNS - 1].max[0],
            1.0
        );
        let tab_gap = tab_assets.gap_units;
        let materials = tab_rect(bounds, 2, INVENTORY_TAB_LABELS.len(), tab_gap);
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

        let equip = tab_rect(bounds, 0, INVENTORY_TAB_LABELS.len(), tab_gap);
        let consumables = tab_rect(bounds, 1, INVENTORY_TAB_LABELS.len(), tab_gap);
        let gap_cursor = [
            (equip.max[0] + consumables.min[0]) * 0.5,
            (bounds.min[1] + bounds.max[1]) * 0.5,
        ];
        assert_eq!(
            tab_at(bounds, INVENTORY_TAB_LABELS.len(), tab_gap, gap_cursor),
            None
        );
        assert!(inventory.apply_pointer_button(
            window_assets,
            tab_assets,
            ElementState::Pressed,
            Some(gap_cursor),
            viewport(),
            1.0
        ));
        assert_eq!(inventory.tabs.pressed_index, None);
        assert_eq!(inventory.tabs.selected_index, 2);
    }

    #[test]
    fn inventory_items_filter_by_tab_and_stack_quantity_overlays_the_first_slot() {
        let window_assets = embedded_assets();
        let tab_assets = embedded_tab_assets();
        let slot_assets = embedded_slot_assets();
        let registry = inventory_registry();
        let item_icons = placeholder_item_icons(&registry);
        let sword = purgatory_common::ITEM_PRACTICE_SWORD;
        let potion = purgatory_common::ITEM_SMALL_POTION;
        let entries = [
            InventoryEntry {
                slot: 9,
                item_instance_id: purgatory_common::ItemInstanceId::from_raw(2),
                definition: potion,
                quantity: 12,
            },
            InventoryEntry {
                slot: 3,
                item_instance_id: purgatory_common::ItemInstanceId::from_raw(1),
                definition: sword,
                quantity: 1,
            },
        ];
        let mut inventory = InventoryWindow::default();
        assert!(inventory.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));

        let equip = inventory
            .frame(InventoryWindowFrameInput {
                window_assets,
                tab_assets,
                slot_assets,
                item_icon_assets: &item_icons,
                entries: &entries,
                registry: &registry,
                viewport: viewport(),
                pixels_per_unit: 1.0,
                cursor: None,
            })
            .unwrap()
            .unwrap();
        assert_eq!(equip.textured_rects.len(), 63);
        assert_eq!(equip.texts.len(), 13);
        let first_slot = equip.textured_rects[27];
        let sword_icon = equip.textured_rects[62];
        assert_eq!(sword_icon.texture, item_icons.resolve(sword).texture);
        assert_eq!(sword_icon.min[0] - first_slot.min[0], 4.0);
        assert_eq!(first_slot.max[0] - sword_icon.max[0], 4.0);

        inventory.tabs.selected_index = 1;
        let consumables = inventory
            .frame(InventoryWindowFrameInput {
                window_assets,
                tab_assets,
                slot_assets,
                item_icon_assets: &item_icons,
                entries: &entries,
                registry: &registry,
                viewport: viewport(),
                pixels_per_unit: 1.0,
                cursor: None,
            })
            .unwrap()
            .unwrap();
        assert_eq!(consumables.textured_rects.len(), 63);
        assert_eq!(
            consumables.textured_rects[62].texture,
            item_icons.resolve(potion).texture
        );
        assert_eq!(consumables.texts.len(), 14);
        assert_eq!(consumables.texts[13].content.0, "12");
        assert_eq!(consumables.texts[13].style.alignment, TextAlignment::Right);
    }

    #[test]
    fn inventory_item_hover_tooltip_and_click_selection_share_visible_slot_mapping() {
        let window_assets = embedded_assets();
        let tab_assets = embedded_tab_assets();
        let slot_assets = embedded_slot_assets();
        let registry = inventory_registry();
        let item_icons = placeholder_item_icons(&registry);
        let sword = purgatory_common::ITEM_PRACTICE_SWORD;
        let sword_item = ItemInstanceId::from_raw(41);
        let entries = [InventoryEntry {
            slot: 7,
            item_instance_id: sword_item,
            definition: sword,
            quantity: 1,
        }];
        let mut inventory = InventoryWindow::default();
        assert!(inventory.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        inventory
            .frame(InventoryWindowFrameInput {
                window_assets,
                tab_assets,
                slot_assets,
                item_icon_assets: &item_icons,
                entries: &entries,
                registry: &registry,
                viewport: viewport(),
                pixels_per_unit: 1.0,
                cursor: None,
            })
            .unwrap()
            .unwrap();
        let first_hit = inventory.item_hit_regions[0].0;
        let cursor = [
            (first_hit.min[0] + first_hit.max[0]) * 0.5,
            (first_hit.min[1] + first_hit.max[1]) * 0.5,
        ];
        assert!(inventory.apply_pointer_button(
            window_assets,
            tab_assets,
            ElementState::Pressed,
            Some(cursor),
            viewport(),
            1.0,
        ));
        assert_eq!(inventory.selected_item, None);
        assert!(inventory.apply_pointer_button(
            window_assets,
            tab_assets,
            ElementState::Released,
            Some(cursor),
            viewport(),
            1.0,
        ));
        assert_eq!(inventory.selected_item, Some(sword_item));
        assert_eq!(inventory.take_completed_click(), Some(sword_item));
        assert_eq!(inventory.take_completed_drag(), None);

        let frame = inventory
            .frame(InventoryWindowFrameInput {
                window_assets,
                tab_assets,
                slot_assets,
                item_icon_assets: &item_icons,
                entries: &entries,
                registry: &registry,
                viewport: viewport(),
                pixels_per_unit: 1.0,
                cursor: Some(cursor),
            })
            .unwrap()
            .unwrap();
        assert_eq!(frame.textured_rects[27].tint, INVENTORY_SLOT_SELECTED_TINT);
        assert_eq!(
            frame.textured_rects.len(),
            64,
            "one icon + tooltip background"
        );
        assert_eq!(
            frame.texts.len(),
            15,
            "title/tabs/currency + two tooltip lines"
        );
        assert!(
            frame
                .texts
                .iter()
                .any(|text| text.content.0 == "equipment.debug.practice_sword")
        );
        assert!(frame.texts.iter().any(|text| {
            text.content.0.contains("equipment")
                && text.content.0.contains("Qty 1")
                && text.content.0.contains("Stack 1")
        }));
    }

    #[test]
    fn unknown_inventory_definition_uses_placeholder_in_misc() {
        let registry = inventory_registry();
        let item_icons = placeholder_item_icons(&registry);
        let unknown = ContentId::from_authored("item.unknown.client_mismatch").unwrap();
        let entries = [InventoryEntry {
            slot: 0,
            item_instance_id: purgatory_common::ItemInstanceId::from_raw(1),
            definition: unknown,
            quantity: 1,
        }];
        let slots = embedded_slot_assets()
            .frame(UiSlotGrid::new(1, 1, 0.0), [10.0, 20.0], 1.0)
            .unwrap();
        let frame = inventory_items_frame(
            &registry,
            &item_icons,
            &entries,
            ItemCategory::Misc,
            &slots,
            1.0,
            None,
        )
        .unwrap();
        assert_eq!(frame.textured_rects.len(), 1);
        assert_eq!(frame.textured_rects[0].texture, item_icons.fallback.texture);
        assert!(frame.texts.is_empty());
    }

    #[test]
    fn currency_display_formats_runtime_gold_and_silver_values() {
        let texts = UiCurrencyDisplay {
            gold: 12,
            silver: 34,
        }
        .frame(
            ScreenRect {
                min: [10.0, 20.0],
                max: [210.0, 42.0],
            },
            1.0,
        )
        .unwrap();

        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0].content.0, "Gold: 12");
        assert_eq!(texts[1].content.0, "Silver: 34");
        assert_eq!(texts[0].anchor[0], 60.0);
        assert_eq!(texts[1].anchor[0], 160.0);
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
    fn inventory_and_equipment_hotkeys_and_close_buttons_are_independent() {
        let assets = embedded_assets();
        let mut inventory = InventoryWindow::default();
        let mut equipment = EquipmentWindow::default();

        assert!(inventory.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        assert_eq!(inventory.chrome.mode, ProofPanelMode::Normal);
        assert_eq!(equipment.chrome.mode, ProofPanelMode::Hidden);

        assert!(equipment.apply_key(
            PhysicalKey::Code(KeyCode::KeyO),
            ElementState::Pressed,
            false
        ));
        assert_eq!(inventory.chrome.mode, ProofPanelMode::Normal);
        assert_eq!(equipment.chrome.mode, ProofPanelMode::Normal);
        assert_eq!(
            assets
                .layout(&mut equipment.chrome, viewport(), 1.0)
                .unwrap()
                .unwrap()
                .window
                .width(),
            250.0
        );

        assert!(inventory.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        assert_eq!(inventory.chrome.mode, ProofPanelMode::Hidden);
        assert_eq!(equipment.chrome.mode, ProofPanelMode::Normal);
        assert!(equipment.apply_key(
            PhysicalKey::Code(KeyCode::KeyO),
            ElementState::Pressed,
            false
        ));
        assert_eq!(inventory.chrome.mode, ProofPanelMode::Hidden);
        assert_eq!(equipment.chrome.mode, ProofPanelMode::Hidden);
        assert!(inventory.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        assert!(equipment.apply_key(
            PhysicalKey::Code(KeyCode::KeyO),
            ElementState::Pressed,
            false
        ));

        let inventory_close = assets
            .layout(&mut inventory.chrome, viewport(), 1.0)
            .unwrap()
            .unwrap()
            .close_button
            .min;
        assert!(inventory.apply_pointer_button(
            assets,
            embedded_tab_assets(),
            ElementState::Pressed,
            Some(inventory_close),
            viewport(),
            1.0,
        ));
        assert!(inventory.apply_pointer_button(
            assets,
            embedded_tab_assets(),
            ElementState::Released,
            Some(inventory_close),
            viewport(),
            1.0,
        ));
        assert_eq!(inventory.chrome.mode, ProofPanelMode::Hidden);
        assert_eq!(equipment.chrome.mode, ProofPanelMode::Normal);

        assert!(inventory.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        let equipment_close = assets
            .layout(&mut equipment.chrome, viewport(), 1.0)
            .unwrap()
            .unwrap()
            .close_button
            .min;
        assert!(equipment.apply_pointer_button(
            assets,
            ElementState::Pressed,
            Some(equipment_close),
            viewport(),
            1.0,
        ));
        assert!(equipment.apply_pointer_button(
            assets,
            ElementState::Released,
            Some(equipment_close),
            viewport(),
            1.0,
        ));
        assert_eq!(inventory.chrome.mode, ProofPanelMode::Normal);
        assert_eq!(equipment.chrome.mode, ProofPanelMode::Hidden);
    }

    #[test]
    fn equipment_same_slot_release_is_click_and_elsewhere_is_drag() {
        let window_assets = embedded_assets();
        let mut equipment = EquipmentWindow {
            chrome: normal_window(),
            ..EquipmentWindow::default()
        };
        let layout = window_assets
            .layout(&mut equipment.chrome, viewport(), 1.0)
            .unwrap()
            .unwrap();
        let bounds = ScreenRect {
            min: [layout.window.min[0] + 30.0, layout.window.min[1] + 60.0],
            max: [layout.window.min[0] + 70.0, layout.window.min[1] + 100.0],
        };
        equipment.slot_hit_regions = vec![bounds];
        equipment.occupied_slots[0] = Some(ContentId::from_token(1));
        let cursor = [
            (bounds.min[0] + bounds.max[0]) * 0.5,
            (bounds.min[1] + bounds.max[1]) * 0.5,
        ];
        assert!(equipment.apply_pointer_button(
            window_assets,
            ElementState::Pressed,
            Some(cursor),
            viewport(),
            1.0
        ));
        equipment.pointer_moved(
            [cursor[0] + ITEM_DRAG_THRESHOLD_PX - 1.0, cursor[1]],
            viewport(),
            1.0,
        );
        assert!(!equipment.is_dragging());
        assert!(equipment.apply_pointer_button(
            window_assets,
            ElementState::Released,
            Some(cursor),
            viewport(),
            1.0
        ));
        assert_eq!(equipment.take_completed_click(), Some(0));
        assert_eq!(equipment.take_completed_drag(), None);

        assert!(equipment.apply_pointer_button(
            window_assets,
            ElementState::Pressed,
            Some(cursor),
            viewport(),
            1.0
        ));
        assert!(equipment.pointer_moved(
            [cursor[0] + ITEM_DRAG_THRESHOLD_PX + 1.0, cursor[1]],
            viewport(),
            1.0
        ));
        assert!(equipment.is_dragging());
        assert!(equipment.apply_pointer_button(
            window_assets,
            ElementState::Released,
            Some([bounds.max[0] + 20.0, bounds.max[1] + 20.0]),
            viewport(),
            1.0
        ));
        assert_eq!(equipment.take_completed_click(), None);
        assert_eq!(equipment.take_completed_drag(), Some(0));
        assert!(!equipment.is_dragging());
    }

    #[test]
    fn drag_preview_uses_the_source_content_icon_and_is_cursor_centered() {
        let registry = inventory_registry();
        let item_icons = placeholder_item_icons(&registry);
        let definition =
            ContentId::from_authored("equipment.debug.practice_sword").expect("sword id");
        let cursor = [100.0, 80.0];
        let preview = drag_preview_icon(item_icons.resolve(definition), cursor, 1.0).unwrap();

        assert_eq!(preview.texture, item_icons.resolve(definition).texture);
        assert_eq!(
            [
                (preview.min[0] + preview.max[0]) * 0.5,
                (preview.min[1] + preview.max[1]) * 0.5
            ],
            cursor
        );
    }

    #[test]
    fn drag_resolution_accepts_only_matching_equipment_slots() {
        let item = ItemInstanceId::from_raw(7);
        assert_eq!(
            resolve_drag(
                DragSource::Inventory(item),
                DragDestination::Equipment(5),
                Some(5)
            ),
            DragResolution::Equip {
                item_instance_id: item,
                slot: 5
            }
        );
        assert_eq!(
            resolve_drag(
                DragSource::Inventory(item),
                DragDestination::Equipment(4),
                Some(5)
            ),
            DragResolution::Noop
        );
        assert_eq!(
            resolve_drag(
                DragSource::Inventory(item),
                DragDestination::Equipment(5),
                None
            ),
            DragResolution::Noop
        );
    }

    #[test]
    fn drag_resolution_handles_unequip_drop_and_no_ops_without_mutation() {
        let item = ItemInstanceId::from_raw(8);
        assert_eq!(
            resolve_drag(DragSource::Equipped(2), DragDestination::Inventory, None),
            DragResolution::Unequip { slot: 2 }
        );
        assert_eq!(
            resolve_drag(
                DragSource::Inventory(item),
                DragDestination::Outside,
                Some(0)
            ),
            DragResolution::Drop {
                item_instance_id: item
            }
        );
        assert_eq!(
            resolve_drag(
                DragSource::Inventory(item),
                DragDestination::Inventory,
                Some(0)
            ),
            DragResolution::Noop
        );
        assert_eq!(
            resolve_drag(DragSource::Equipped(1), DragDestination::Equipment(3), None),
            DragResolution::Noop
        );
    }

    #[test]
    fn invalid_slice_and_button_state_metadata_are_rejected() {
        let mut metadata: UiAtlasMetadata = serde_json::from_str(ATLAS_METADATA).unwrap();
        metadata.window = SourceRectPx {
            x: 1500,
            y: 900,
            width: 100,
            height: 200,
        };
        assert!(validate_atlas_metadata(&metadata, [768, 283]).is_err());
        metadata = serde_json::from_str(ATLAS_METADATA).unwrap();
        metadata.small_buttons.close.normal.width = 500;
        metadata.small_buttons.close.normal.x = 1200;
        assert!(validate_atlas_metadata(&metadata, [768, 283]).is_err());
    }

    #[test]
    fn window_frame_composes_integrated_window_and_title() {
        let assets = embedded_assets();
        let mut window = normal_window();
        let frame = assets
            .proof_frame(&mut window, "Inventory", viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        assert_eq!(frame.textured_rects.len(), 10);
        assert_eq!(frame.textured_rects[0].size(), [9.0, 24.0]);
        assert_eq!(frame.textured_rects[9].size(), [14.0, 12.0]);
        assert_eq!(
            frame.textured_rects[9].uv_min,
            [162.5 / 192.0, 105.5 / 192.0]
        );
        assert_eq!(
            frame.textured_rects[9].uv_max,
            [179.5 / 192.0, 122.5 / 192.0]
        );
        assert_eq!(frame.title.content.0, "Inventory");
    }

    #[test]
    fn normal_windows_start_staggered_around_viewport_center() {
        let assets = embedded_assets();
        let mut inventory = InventoryWindow::default();
        inventory.chrome.mode = ProofPanelMode::Normal;
        let inventory_layout = assets
            .layout(&mut inventory.chrome, viewport(), 1.0)
            .unwrap()
            .unwrap();
        let mut equipment = EquipmentWindow::default();
        equipment.chrome.mode = ProofPanelMode::Normal;
        let equipment_layout = assets
            .layout(&mut equipment.chrome, viewport(), 1.0)
            .unwrap()
            .unwrap();
        let center = |rect: ScreenRect| {
            [
                (rect.min[0] + rect.max[0]) * 0.5,
                (rect.min[1] + rect.max[1]) * 0.5,
            ]
        };

        assert_eq!(center(inventory_layout.window), [584.0, 336.0]);
        assert_eq!(center(equipment_layout.window), [696.0, 384.0]);
        assert_ne!(inventory_layout.window.min, equipment_layout.window.min);
    }

    #[test]
    fn integrated_nine_slice_keeps_header_and_corners_fixed() {
        let assets = embedded_assets();
        let mut equipment = ProofPanelWindow::with_size([250.0, 300.0]);
        equipment.mode = ProofPanelMode::Normal;
        let equipment_frame = assets
            .proof_frame(&mut equipment, "Equipment", viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        let mut dialog = ProofPanelWindow::with_size([400.0, 180.0]);
        dialog.mode = ProofPanelMode::Normal;
        let dialog_frame = assets
            .proof_frame(&mut dialog, "Dialog", viewport(), 1.0, None)
            .unwrap()
            .unwrap();
        assert_eq!(equipment_frame.textured_rects[0].size(), [9.0, 24.0]);
        assert_eq!(dialog_frame.textured_rects[0].size(), [9.0, 24.0]);
        assert_eq!(equipment_frame.textured_rects[4].size(), [232.0, 267.0]);
        assert_eq!(dialog_frame.textured_rects[4].size(), [382.0, 147.0]);
        assert_eq!(equipment_frame.textured_rects[8].size(), [9.0, 9.0]);
        assert_eq!(dialog_frame.textured_rects[8].size(), [9.0, 9.0]);
    }

    #[test]
    fn enlarged_ui_keeps_title_inside_fixed_header_chrome() {
        let assets = embedded_assets();
        let mut window = ProofPanelWindow::with_size([250.0, 300.0]);
        window.mode = ProofPanelMode::Normal;
        let frame = assets
            .proof_frame(&mut window, "SETTINGS", viewport(), 1.25, None)
            .unwrap()
            .unwrap();
        let layout = assets
            .layout(&mut window, viewport(), 1.25)
            .unwrap()
            .unwrap();

        assert_eq!(layout.header.height(), 24.0);
        assert_eq!(frame.title.style.font_size * 1.25, 16.0);
        assert!(frame.title.style.font_size < TITLE_FONT_SIZE_UNITS);
        assert!(
            frame.title.anchor[1] + frame.title.style.font_size * 1.25 <= layout.header.max[1],
            "scaled title must remain inside the rendered header"
        );
    }

    #[test]
    fn inventory_and_equipment_headers_use_atlas_chrome_icons() {
        let window_assets = embedded_assets();
        let tab_assets = embedded_tab_assets();
        let slot_assets = embedded_slot_assets();
        let registry = inventory_registry();
        let item_icons = placeholder_item_icons(&registry);
        let mut inventory = InventoryWindow::default();
        inventory.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false,
        );
        let inventory_frame = inventory
            .frame(InventoryWindowFrameInput {
                window_assets,
                tab_assets,
                slot_assets,
                item_icon_assets: &item_icons,
                entries: &[],
                registry: &registry,
                viewport: viewport(),
                pixels_per_unit: 1.0,
                cursor: None,
            })
            .unwrap()
            .unwrap();
        let mut equipment = EquipmentWindow::default();
        equipment.apply_key(
            PhysicalKey::Code(KeyCode::KeyO),
            ElementState::Pressed,
            false,
        );
        let equipment_frame = equipment
            .frame(EquipmentWindowFrameInput {
                window_assets,
                slot_assets,
                item_icon_assets: &item_icons,
                equipment: None,
                viewport: viewport(),
                pixels_per_unit: 1.0,
                cursor: None,
            })
            .unwrap()
            .unwrap();
        let atlas = window_assets.panel().texture;
        assert_eq!(inventory_frame.textured_rects[10].texture, atlas);
        assert_eq!(equipment_frame.textured_rects[10].texture, atlas);
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
        assert_eq!(
            hover.textured_rects[9].uv_min,
            [162.5 / 192.0, 69.5 / 192.0]
        );
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
        assert_eq!(
            pressed.textured_rects[9].uv_min,
            [162.5 / 192.0, 87.5 / 192.0]
        );
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
        assert_eq!(window.top_left_units, Some([0.0, 90.0]));
        assert!(window.pointer_moved([2000.0, 1000.0], viewport(), 1.0));
        assert_eq!(window.top_left_units, Some([998.0, 280.0]));
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
        assert!(!window.apply_key(
            PhysicalKey::Code(KeyCode::KeyP),
            ElementState::Pressed,
            false
        ));
    }

    #[test]
    fn closing_and_reopening_preserves_session_position() {
        let mut window = InventoryWindow::default();
        assert!(window.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        window.chrome.top_left_units = Some([31.0, 17.0]);
        assert!(window.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        assert!(!window.is_visible());
        assert!(window.apply_key(
            PhysicalKey::Code(KeyCode::KeyI),
            ElementState::Pressed,
            false
        ));
        assert!(window.is_visible());
        assert_eq!(window.chrome.top_left_units, Some([31.0, 17.0]));
    }
}
