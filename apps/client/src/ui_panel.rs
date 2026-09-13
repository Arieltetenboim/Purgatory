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
const NORMAL_SIZE_UNITS: [f32; 2] = [320.0, 240.0];
const TITLE_FONT_SIZE_UNITS: f32 = 15.0;
const TITLE_LEFT_INSET_UNITS: f32 = 12.0;
const TITLE_CONTROL_GAP_UNITS: f32 = 6.0;

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
struct ScreenRect {
    min: [f32; 2],
    max: [f32; 2],
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
    if !destination
        .min
        .into_iter()
        .chain(destination.max)
        .all(f32::is_finite)
        || destination.width() <= caps.left + caps.right
        || destination.height() <= 0.0
    {
        return Err("UI header target is invalid or smaller than its caps".to_string());
    }
    let source_x = [
        0.0,
        source.left as f32,
        (source_size_px[0] - source.right) as f32,
        source_size_px[0] as f32,
    ];
    let destination_x = [
        destination.min[0],
        destination.min[0] + caps.left,
        destination.max[0] - caps.right,
        destination.max[0],
    ];
    let source_width = source_size_px[0] as f32;
    let mut regions = Vec::with_capacity(3);
    for column in 0..3 {
        regions.push(UiTexturedRect {
            min: [destination_x[column], destination.min[1]],
            max: [destination_x[column + 1], destination.max[1]],
            texture,
            uv_min: [source_x[column] / source_width, 0.0],
            uv_max: [source_x[column + 1] / source_width, 1.0],
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
        assert_eq!(frame.textured_rects[10].size(), [230.0, 30.0]);
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
        assert_eq!(double.textured_rects[10].size(), [550.0, 30.0]);
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
        assert_eq!(window.top_left_units, Some([960.0, 480.0]));
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
