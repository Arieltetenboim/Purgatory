//! Production frontend presentation and input. Navigation stays in FrontendRuntime.
use purgatory_common::{DEV_LOGIN_MAX_LEN, DevLogin};
use winit::keyboard::{Key, NamedKey};

use crate::frontend_runtime::{
    CHARACTER_NAME_MAX, CharacterAreaMode, CharacterSlotState, CreationState, FrontendAction,
    FrontendRuntime, FrontendSession, FrontendStage,
};
use crate::renderer::{
    DrawQuad, PixelViewport, SpriteTextureId, TextAlignment, TextBlock, TextContent, TextStyle,
    UiRect, UiTexturedQuad, UiTexturedRect,
};
use crate::ui_panel::ScreenRect;

#[derive(Clone, Copy)]
pub(crate) struct FrontendUiAssets {
    pub(crate) logo: SpriteTextureId,
    pub(crate) logo_aspect: f32,
}

pub(crate) struct FrontendView<'a> {
    pub(crate) runtime: &'a FrontendRuntime,
    pub(crate) login: &'a str,
    pub(crate) status: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    Username,
    CharacterName,
    Action(FrontendAction),
}

#[derive(Default)]
pub(crate) struct FrontendUi {
    focused: bool,
    pressed: Option<Control>,
}

/// Minimal append/backspace primitive; acceptance rules belong to its caller.
fn append_single_line(
    value: &mut String,
    text: &str,
    max_bytes: usize,
    accepts: impl Fn(char) -> bool,
) {
    for ch in text.chars().filter(|ch| !ch.is_control() && accepts(*ch)) {
        if value.len() + ch.len_utf8() <= max_bytes {
            value.push(ch);
        }
    }
}

impl FrontendUi {
    pub(crate) fn reset_input(&mut self) {
        self.focused = false;
        self.pressed = None;
    }

    pub(crate) fn key(
        &mut self,
        runtime: &mut FrontendRuntime,
        login: &mut String,
        key: &Key,
        text: Option<&str>,
        repeat: bool,
    ) -> Option<FrontendAction> {
        let stage = runtime.visible_stage()?;
        if runtime.character_area.creation == CreationState::Pending
            || runtime.session == FrontendSession::Pending
        {
            return None;
        }
        if *key == Key::Named(NamedKey::Escape) && !repeat {
            self.reset_input();
            return Some(FrontendAction::Back);
        }
        if !self.focused {
            return None;
        }
        if stage == FrontendStage::CharacterSelect {
            if *key == Key::Named(NamedKey::Enter) {
                if !repeat && runtime.character_area.can_create() {
                    self.reset_input();
                    return Some(FrontendAction::CreateCharacter);
                }
                return None;
            }
            if let Some(draft) = runtime.character_area.draft.as_mut() {
                if *key == Key::Named(NamedKey::Backspace) {
                    draft.name.pop();
                } else if let Some(text) = text {
                    append_single_line(&mut draft.name, text, CHARACTER_NAME_MAX, |ch| {
                        ch.is_ascii_alphanumeric()
                    });
                }
            }
            return None;
        }
        if stage != FrontendStage::Login {
            return None;
        }
        match key {
            Key::Named(NamedKey::Backspace) => {
                login.pop();
            }
            Key::Named(NamedKey::Enter) if !repeat && DevLogin::parse(login).is_ok() => {
                self.reset_input();
                return Some(FrontendAction::ContinueFromLogin);
            }
            _ => {
                if let Some(text) = text {
                    append_single_line(login, text, DEV_LOGIN_MAX_LEN, |ch| {
                        ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '.')
                    });
                }
            }
        }
        None
    }

    pub(crate) fn pointer(
        &mut self,
        runtime: &FrontendRuntime,
        login: &str,
        viewport: PixelViewport,
        cursor: Option<[f32; 2]>,
        pressed: bool,
    ) -> Option<FrontendAction> {
        let hit = cursor.and_then(|point| {
            controls(runtime, login)
                .into_iter()
                .find(|(bounds, _)| Layout(viewport).rect(*bounds).contains(point))
                .map(|(_, control)| control)
        });
        if pressed {
            // Locked slots/background must not steal creation input focus.
            if hit.is_some()
                || !matches!(
                    runtime.character_area.mode,
                    CharacterAreaMode::Creating { .. }
                )
            {
                self.focused = matches!(hit, Some(Control::Username | Control::CharacterName));
            }
            self.pressed = hit;
            return None;
        }
        let previous = self.pressed.take();
        if hit == previous
            && let Some(Control::Action(action)) = hit
        {
            self.reset_input();
            return Some(action);
        }
        None
    }

    pub(crate) fn frame(
        &self,
        view: FrontendView<'_>,
        viewport: PixelViewport,
        pixels_per_unit: f32,
        assets: FrontendUiAssets,
        cursor: Option<[f32; 2]>,
    ) -> FrontendFrame {
        let FrontendView {
            runtime,
            login,
            status,
        } = view;
        let mut frame = FrontendFrame::default();
        let Some(stage) = runtime.visible_stage() else {
            return frame;
        };
        let layout = Layout(viewport);
        let mut label = |text: &str, x: f32, y: f32, width: f32, size: f32| {
            frame.texts.push(TextBlock {
                content: TextContent(text.to_owned()),
                style: TextStyle::at_size(
                    size * layout.scale() / pixels_per_unit,
                    [0.94, 0.88, 0.73, 1.0],
                    TextAlignment::Center,
                ),
                anchor: layout.point([x, y]),
                max_width: Some(width * layout.scale()),
            });
        };
        label(status, 1040.0, 655.0, 340.0, 13.0);
        match stage {
            FrontendStage::Login => {
                let width = 460.0_f32.min(170.0 * assets.logo_aspect);
                let bounds =
                    layout.rect([640.0 - width / 2.0, 85.0, width, width / assets.logo_aspect]);
                frame.textured_rects.push(UiTexturedRect {
                    min: bounds.min,
                    max: bounds.max,
                    texture: assets.logo,
                    uv_min: [0.0, 0.0],
                    uv_max: [1.0, 1.0],
                    tint: [1.0; 4],
                });
                frame
                    .rects
                    .push(layout.fill([405.0, 280.0, 470.0, 275.0], [0.025, 0.03, 0.04, 0.94]));
                frame.rects.push(layout.fill(
                    USERNAME,
                    if self.focused {
                        [0.12, 0.15, 0.19, 1.0]
                    } else {
                        [0.07, 0.08, 0.10, 1.0]
                    },
                ));
                label("USERNAME", 640.0, 304.0, 380.0, 17.0);
                let shown = if self.focused {
                    format!("{login}|")
                } else if login.is_empty() {
                    "Click to enter username".into()
                } else {
                    login.into()
                };
                label(&shown, 640.0, 348.0, 390.0, 17.0);
                label(
                    if DevLogin::parse(login).is_ok() {
                        "DEV profile — no authentication"
                    } else {
                        "2–32: a–z, 0–9, _ or .\nNo leading, trailing or consecutive dots"
                    },
                    640.0,
                    402.0,
                    420.0,
                    14.0,
                );
                if DevLogin::parse(login).is_err() {
                    frame
                        .rects
                        .push(layout.fill(CONTINUE, [0.10, 0.10, 0.10, 0.95]));
                    label("CONTINUE", 640.0, 481.0, 300.0, 17.0);
                }
            }
            FrontendStage::ChannelSelect => {
                frame
                    .rects
                    .push(layout.fill([390.0, 190.0, 500.0, 320.0], [0.025, 0.03, 0.04, 0.94]));
                label("SELECT CHANNEL", 640.0, 220.0, 420.0, 28.0);
                label("Local placeholder channels", 640.0, 270.0, 420.0, 16.0);
            }
            FrontendStage::CharacterSelect => {
                label("CHARACTERS", 640.0, 54.0, 1100.0, 28.0);
                label(
                    "Server roster — Enter World is not available",
                    640.0,
                    99.0,
                    1100.0,
                    16.0,
                );
                for (slot, state) in runtime.character_area.slots.iter().enumerate() {
                    let bounds = slot_bounds(slot as u8);
                    let x = bounds[0] + bounds[2] / 2.0;
                    let selected = runtime.character_area.selected_slot == Some(slot as u8);
                    frame.rects.push(layout.fill(
                        bounds,
                        if selected {
                            [0.12, 0.14, 0.17, 0.96]
                        } else {
                            [0.025, 0.03, 0.04, 0.88]
                        },
                    ));
                    if runtime.character_area.mode
                        == (CharacterAreaMode::Creating { slot: slot as u8 })
                    {
                        label("NEW CHARACTER", x, 181.0, 300.0, 22.0);
                        label("NAME", x + 62.0, 218.0, 195.0, 15.0);
                        let name = runtime
                            .character_area
                            .draft
                            .as_ref()
                            .map(|draft| draft.name.as_str())
                            .unwrap_or("");
                        let shown = if self.focused {
                            format!("{name}|")
                        } else if name.is_empty() {
                            "Click to enter".into()
                        } else {
                            name.into()
                        };
                        frame.rects.push(layout.fill(
                            name_bounds(slot as u8),
                            if self.focused {
                                [0.16, 0.19, 0.24, 1.0]
                            } else {
                                [0.06, 0.07, 0.09, 1.0]
                            },
                        ));
                        label(&shown, x + 62.0, 249.0, 184.0, 17.0);
                        let status = match runtime.character_area.creation {
                            CreationState::Idle => "3–12 letters or numbers",
                            CreationState::Pending => "Creating...",
                            CreationState::Rejected(reason) => match reason {
                                purgatory_protocol::CharacterCreateRejection::InvalidName => {
                                    "Invalid name"
                                }
                                purgatory_protocol::CharacterCreateRejection::NameTaken => {
                                    "Name taken"
                                }
                                purgatory_protocol::CharacterCreateRejection::RosterFull => {
                                    "Roster full"
                                }
                                purgatory_protocol::CharacterCreateRejection::StorageFailure => {
                                    "Server failure. Retry."
                                }
                            },
                        };
                        label(status, x + 62.0, 286.0, 195.0, 12.0);
                        label("APPEARANCE — LOCKED", x + 62.0, 315.0, 195.0, 14.0);
                        for (index, heading) in APPEARANCE_ROWS.iter().enumerate() {
                            label(
                                &format!("{heading}: DEFAULT"),
                                x + 62.0,
                                340.0 + index as f32 * 20.0,
                                195.0,
                                13.0,
                            );
                        }
                        label("ATTRIBUTES", x, 484.0, 300.0, 15.0);
                        label("Customization available later", x, 506.0, 300.0, 12.0);
                        if !runtime.character_area.can_create() {
                            frame.rects.push(
                                layout.fill(create_button(slot as u8), [0.10, 0.10, 0.10, 0.95]),
                            );
                            label("CREATE", x - 78.0, 550.0, 140.0, 16.0);
                        }
                    } else {
                        let name = match state {
                            CharacterSlotState::Empty => "EMPTY",
                            CharacterSlotState::Occupied { name, .. } => name,
                        };
                        label(name, x, 190.0, 300.0, 22.0);
                        label(
                            match state {
                                CharacterSlotState::Empty => "NEW CHARACTER",
                                CharacterSlotState::Occupied { .. } => "CHARACTER",
                            },
                            x,
                            465.0,
                            295.0,
                            15.0,
                        );
                        label(
                            if selected { "SELECTED" } else { "" },
                            x,
                            420.0,
                            295.0,
                            16.0,
                        );
                    }
                }
            }
            FrontendStage::Intro => {}
        }
        for (bounds, control) in controls(runtime, login) {
            let Control::Action(action) = control else {
                continue;
            };
            if let FrontendAction::SelectCharacter(slot) = action
                && runtime.character_area.create_slot() == Some(slot)
            {
                continue;
            }
            // Entire character area selects; its bottom control marks the affordance.
            let button_bounds = if let FrontendAction::SelectCharacter(slot) = action {
                slot_button(slot)
            } else {
                bounds
            };
            let hit_bounds = layout.rect(bounds);
            let hover = cursor.is_some_and(|point| hit_bounds.contains(point));
            // Broad frontend controls use the existing flat primitive: the small
            // production atlas intentionally tiles instead of upscaling its 18px art.
            let [x, y, width, height] = button_bounds;
            let color = if hover && self.pressed == Some(control) {
                [0.36, 0.29, 0.18, 1.0]
            } else if hover {
                [0.72, 0.62, 0.42, 1.0]
            } else {
                [0.55, 0.46, 0.31, 1.0]
            };
            frame
                .rects
                .push(layout.fill(button_bounds, [0.83, 0.73, 0.51, 1.0]));
            frame
                .rects
                .push(layout.fill([x + 2.0, y + 2.0, width - 4.0, height - 4.0], color));
            let text = match action {
                FrontendAction::ContinueFromLogin => "CONTINUE".into(),
                FrontendAction::SelectChannel(slot) => format!(
                    "Channel {}{}",
                    slot + 1,
                    if runtime.selected_channel() == Some(slot) {
                        "  • SELECTED"
                    } else {
                        ""
                    }
                ),
                FrontendAction::SelectCharacter(slot) => {
                    if runtime.selected_character() == Some(slot) {
                        "SELECTED"
                    } else {
                        "SELECT"
                    }
                    .into()
                }
                FrontendAction::BeginCreate(_) => "CREATE NEW CHARACTER".into(),
                FrontendAction::CancelCreation => "CANCEL".into(),
                FrontendAction::CreateCharacter => "CREATE".into(),
                FrontendAction::Back => "BACK".into(),
            };
            let [x, y, width, height] = button_bounds;
            frame.texts.push(TextBlock {
                content: TextContent(text),
                style: TextStyle::at_size(
                    16.0 * layout.scale() / pixels_per_unit,
                    [0.04, 0.03, 0.02, 1.0],
                    TextAlignment::Center,
                ),
                anchor: layout.point([x + width / 2.0, y + (height - 19.2) / 2.0]),
                max_width: Some((width - 16.0) * layout.scale()),
            });
        }
        frame
    }
}

#[derive(Default)]
pub(crate) struct FrontendFrame {
    pub(crate) rects: Vec<UiRect>,
    pub(crate) textured_rects: Vec<UiTexturedRect>,
    pub(crate) texts: Vec<TextBlock>,
    pub(crate) preview_quads: Vec<UiTexturedQuad>,
}

impl FrontendFrame {
    pub(crate) fn add_previews(
        &mut self,
        runtime: &FrontendRuntime,
        viewport: PixelViewport,
        base: &[DrawQuad],
    ) {
        if runtime.visible_stage() != Some(FrontendStage::CharacterSelect) || base.is_empty() {
            return;
        }
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for point in base.iter().flat_map(|quad| quad.world_corners()) {
            for axis in 0..2 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }
        for (slot, state) in runtime.character_area.slots.iter().enumerate() {
            let creating =
                runtime.character_area.mode == (CharacterAreaMode::Creating { slot: slot as u8 });
            if !creating && *state == CharacterSlotState::Empty {
                continue;
            }
            let left = 60.0 + slot as f32 * 400.0;
            let [x, y, width, height] = if creating {
                [left + 15.0, 235.0, 115.0, 235.0]
            } else {
                [left + 110.0, 245.0, 140.0, 165.0]
            };
            let scale = (width / (max[0] - min[0])).min(height / (max[1] - min[1]));
            for quad in base {
                let Some(texture) = quad.sprite_texture_id() else {
                    continue;
                };
                let corners = quad.world_corners().map(|point| {
                    Layout(viewport).point([
                        x + width / 2.0 + (point[0] - (min[0] + max[0]) / 2.0) * scale,
                        y + height / 2.0 - (point[1] - (min[1] + max[1]) / 2.0) * scale,
                    ])
                });
                self.preview_quads.push(UiTexturedQuad {
                    corners,
                    uvs: quad.uvs(),
                    texture,
                    tint: quad.color,
                });
            }
        }
    }
}

const USERNAME: [f32; 4] = [430.0, 336.0, 420.0, 48.0];
const CONTINUE: [f32; 4] = [480.0, 466.0, 320.0, 48.0];
const BACK: [f32; 4] = [60.0, 640.0, 180.0, 44.0];
fn slot_bounds(slot: u8) -> [f32; 4] {
    [60.0 + f32::from(slot) * 400.0, 160.0, 360.0, 440.0]
}
fn slot_button(slot: u8) -> [f32; 4] {
    [90.0 + f32::from(slot) * 400.0, 525.0, 300.0, 48.0]
}
const APPEARANCE_ROWS: [&str; 7] = [
    "Eyes",
    "Hair",
    "Hair Color",
    "Eye Color",
    "Shirt",
    "Pants",
    "Shoes",
];
fn name_bounds(slot: u8) -> [f32; 4] {
    [204.0 + f32::from(slot) * 400.0, 242.0, 196.0, 38.0]
}
fn create_button(slot: u8) -> [f32; 4] {
    [90.0 + f32::from(slot) * 400.0, 535.0, 144.0, 48.0]
}
fn cancel_button(slot: u8) -> [f32; 4] {
    [246.0 + f32::from(slot) * 400.0, 535.0, 144.0, 48.0]
}

fn controls(runtime: &FrontendRuntime, login: &str) -> Vec<([f32; 4], Control)> {
    if runtime.character_area.creation == CreationState::Pending
        || runtime.session == FrontendSession::Pending
    {
        return Vec::new();
    }
    let action = |bounds, intent| (bounds, Control::Action(intent));
    match runtime.visible_stage() {
        Some(FrontendStage::Login) => {
            let mut controls = vec![(USERNAME, Control::Username)];
            if DevLogin::parse(login).is_ok() {
                controls.push(action(CONTINUE, FrontendAction::ContinueFromLogin));
            }
            controls
        }
        Some(FrontendStage::ChannelSelect) => vec![
            action(
                [450.0, 325.0, 380.0, 50.0],
                FrontendAction::SelectChannel(0),
            ),
            action(
                [450.0, 405.0, 380.0, 50.0],
                FrontendAction::SelectChannel(1),
            ),
            action(BACK, FrontendAction::Back),
        ],
        Some(FrontendStage::CharacterSelect) => match runtime.character_area.mode {
            CharacterAreaMode::Creating { slot } => {
                let mut controls = vec![
                    (name_bounds(slot), Control::CharacterName),
                    action(cancel_button(slot), FrontendAction::CancelCreation),
                    action(BACK, FrontendAction::Back),
                ];
                if runtime.character_area.can_create() {
                    controls.push(action(create_button(slot), FrontendAction::CreateCharacter));
                }
                controls
            }
            CharacterAreaMode::Browsing => {
                let mut controls: Vec<_> = (0..3)
                    .map(|slot| action(slot_bounds(slot), FrontendAction::SelectCharacter(slot)))
                    .collect();
                if let Some(slot) = runtime.character_area.create_slot() {
                    // Create replaces the selected area's Select affordance and wins hit testing.
                    controls.insert(
                        0,
                        action(slot_button(slot), FrontendAction::BeginCreate(slot)),
                    );
                }
                controls.push(action(BACK, FrontendAction::Back));
                controls
            }
        },
        _ => Vec::new(),
    }
}

/// Shared framebuffer transform for drawing and hit testing; fits the constrained viewport.
struct Layout(PixelViewport);
impl Layout {
    fn scale(&self) -> f32 {
        self.0.width as f32 / 1280.0
    }
    fn point(&self, [x, y]: [f32; 2]) -> [f32; 2] {
        [
            self.0.x as f32 + x * self.scale(),
            self.0.y as f32 + y * self.scale(),
        ]
    }
    fn rect(&self, [x, y, width, height]: [f32; 4]) -> ScreenRect {
        ScreenRect {
            min: self.point([x, y]),
            max: self.point([x + width, y + height]),
        }
    }
    fn fill(&self, bounds: [f32; 4], color: [f32; 4]) -> UiRect {
        let bounds = self.rect(bounds);
        UiRect {
            min: bounds.min,
            max: bounds.max,
            color,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend_scene::FrontendScene;

    #[test]
    fn frontend_creation_input_focus_filter_backspace_enter_and_cancel() {
        let (mut runtime, mut scene) = at(FrontendStage::CharacterSelect);
        let mut ui = FrontendUi::default();
        let mut login = "login".to_owned();
        runtime.act(FrontendAction::SelectCharacter(1), &mut scene);
        runtime.act(FrontendAction::BeginCreate(1), &mut scene);
        let key = Key::Character("a".into());
        ui.key(&mut runtime, &mut login, &key, Some("Lost"), false);
        assert_eq!(runtime.character_area.draft.as_ref().unwrap().name, "");
        assert_eq!(click(&mut ui, &runtime, &login, [562.0, 550.0]), None);
        click(&mut ui, &runtime, &login, [690.0, 260.0]);
        assert!(ui.focused);
        assert_eq!(click(&mut ui, &runtime, &login, [240.0, 550.0]), None);
        assert!(ui.focused);
        ui.key(&mut runtime, &mut login, &key, Some("Ab _.-!@אבé"), false);
        assert_eq!(runtime.character_area.draft.as_ref().unwrap().name, "Ab");
        assert_eq!(
            ui.key(
                &mut runtime,
                &mut login,
                &Key::Named(NamedKey::Enter),
                None,
                false
            ),
            None
        );
        assert!(
            !controls(&runtime, &login)
                .iter()
                .any(|(_, control)| *control == Control::Action(FrontendAction::CreateCharacter))
        );
        ui.key(&mut runtime, &mut login, &key, Some("3"), false);
        assert!(
            controls(&runtime, &login)
                .iter()
                .any(|(_, control)| *control == Control::Action(FrontendAction::CreateCharacter))
        );
        assert_eq!(
            ui.key(
                &mut runtime,
                &mut login,
                &Key::Named(NamedKey::Enter),
                None,
                true
            ),
            None
        );
        ui.key(&mut runtime, &mut login, &key, Some("1234567890123"), false);
        assert_eq!(
            runtime.character_area.draft.as_ref().unwrap().name,
            "Ab3123456789"
        );
        ui.key(
            &mut runtime,
            &mut login,
            &Key::Named(NamedKey::Backspace),
            None,
            false,
        );
        assert_eq!(
            runtime.character_area.draft.as_ref().unwrap().name,
            "Ab312345678"
        );
        let action = ui
            .key(
                &mut runtime,
                &mut login,
                &Key::Named(NamedKey::Enter),
                None,
                false,
            )
            .unwrap();
        assert_eq!(action, FrontendAction::CreateCharacter);
        runtime.act(action, &mut scene);
        assert_eq!(runtime.character_area.slots[1], CharacterSlotState::Empty);
        assert_eq!(runtime.character_area.creation, CreationState::Pending);
        assert_eq!(login, "login");
        assert_eq!(
            ui.key(
                &mut runtime,
                &mut login,
                &Key::Named(NamedKey::Enter),
                None,
                false
            ),
            None
        );

        let (mut runtime, mut scene) = at(FrontendStage::CharacterSelect);
        runtime.act(FrontendAction::SelectCharacter(1), &mut scene);
        runtime.act(FrontendAction::BeginCreate(1), &mut scene);
        click(&mut ui, &runtime, &login, [690.0, 260.0]);
        ui.key(&mut runtime, &mut login, &key, Some("Discard"), false);
        let action = ui
            .key(
                &mut runtime,
                &mut login,
                &Key::Named(NamedKey::Escape),
                None,
                false,
            )
            .unwrap();
        runtime.act(action, &mut scene);
        assert!(runtime.character_area.draft.is_none());
        assert!(scene.is_at(crate::frontend_scene::FrontendSceneStop::Character));
        runtime.act(FrontendAction::BeginCreate(1), &mut scene);
        let action = click(&mut ui, &runtime, &login, [718.0, 550.0]).unwrap();
        assert_eq!(action, FrontendAction::CancelCreation);
        runtime.act(action, &mut scene);
        assert!(runtime.character_area.draft.is_none());
    }

    #[test]
    fn frontend_creation_click_create_requires_valid_name() {
        for name in [
            "",
            "Ab",
            "ABC",
            "abc123",
            "Abc123Def456",
            "Abc123Def4567",
            "Ab_",
        ] {
            let (mut runtime, mut scene) = at(FrontendStage::CharacterSelect);
            runtime.act(FrontendAction::SelectCharacter(1), &mut scene);
            runtime.act(FrontendAction::BeginCreate(1), &mut scene);
            runtime.character_area.draft.as_mut().unwrap().name = name.into();
            let expected = crate::frontend_runtime::validate_character_name(name)
                .is_ok()
                .then_some(FrontendAction::CreateCharacter);
            let mut ui = FrontendUi::default();
            assert_eq!(click(&mut ui, &runtime, "", [562.0, 555.0]), expected);
            if let Some(action) = expected {
                runtime.act(action, &mut scene);
                assert_eq!(runtime.selected_character(), None);
                assert_eq!(runtime.character_area.creation, CreationState::Pending);
            }
        }
    }

    fn at(stage: FrontendStage) -> (FrontendRuntime, FrontendScene) {
        let mut runtime = FrontendRuntime::new();
        runtime.session = crate::frontend_runtime::FrontendSession::Ready;
        runtime.character_area.slots[0] = CharacterSlotState::Occupied {
            character_id: purgatory_common::CharacterId::from_raw(10),
            name: "Wanderer".into(),
        };
        runtime.character_area.slots[2] = CharacterSlotState::Occupied {
            character_id: purgatory_common::CharacterId::from_raw(20),
            name: "Warden".into(),
        };
        let mut scene = FrontendScene::new();
        runtime.request(stage, &mut scene);
        runtime.advance(1.0, &mut scene);
        (runtime, scene)
    }

    fn viewport() -> PixelViewport {
        PixelViewport {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        }
    }

    fn click(
        ui: &mut FrontendUi,
        runtime: &FrontendRuntime,
        login: &str,
        point: [f32; 2],
    ) -> Option<FrontendAction> {
        assert_eq!(
            ui.pointer(runtime, login, viewport(), Some(point), true),
            None
        );
        ui.pointer(runtime, login, viewport(), Some(point), false)
    }

    #[test]
    fn frontend_login_focus_characters_backspace_and_length() {
        let (mut runtime, _) = at(FrontendStage::Login);
        let mut ui = FrontendUi::default();
        let mut login = String::new();
        let key = Key::Character("a".into());
        ui.key(&mut runtime, &mut login, &key, Some("a"), false);
        assert!(login.is_empty());
        click(&mut ui, &runtime, &login, [640.0, 355.0]);
        ui.key(
            &mut runtime,
            &mut login,
            &key,
            Some("abc_09.defAZ /\\é\n"),
            false,
        );
        assert_eq!(login, "abc_09.def");
        assert!(DevLogin::parse(&login).is_ok());
        ui.key(
            &mut runtime,
            &mut login,
            &Key::Named(NamedKey::Backspace),
            None,
            true,
        );
        assert_eq!(login, "abc_09.de");
        ui.key(&mut runtime, &mut login, &key, Some(&"x".repeat(40)), false);
        assert_eq!(login.len(), DEV_LOGIN_MAX_LEN);
        click(&mut ui, &runtime, &login, [1.0, 1.0]);
        ui.key(
            &mut runtime,
            &mut login,
            &Key::Named(NamedKey::Backspace),
            None,
            false,
        );
        assert_eq!(login.len(), DEV_LOGIN_MAX_LEN);
    }

    #[test]
    fn frontend_login_enter_and_button_share_full_validation() {
        let (mut runtime, _) = at(FrontendStage::Login);
        for raw in ["", "a", ".ab", "ab.", "a..b", "Ab", "valid_09.name"] {
            let mut login = raw.to_string();
            let mut ui = FrontendUi::default();
            click(&mut ui, &runtime, &login, [640.0, 355.0]);
            let expected = DevLogin::parse(raw)
                .is_ok()
                .then_some(FrontendAction::ContinueFromLogin);
            assert_eq!(
                ui.key(
                    &mut runtime,
                    &mut login,
                    &Key::Named(NamedKey::Enter),
                    None,
                    false
                ),
                expected
            );
            assert_eq!(click(&mut ui, &runtime, &login, [640.0, 490.0]), expected);
        }
    }

    #[test]
    fn frontend_pointer_release_must_match_and_travel_locks_all_input() {
        let (mut runtime, mut scene) = at(FrontendStage::Login);
        let mut ui = FrontendUi::default();
        let mut login = "test".to_owned();
        ui.pointer(&runtime, &login, viewport(), Some([640.0, 490.0]), true);
        assert_eq!(
            ui.pointer(&runtime, &login, viewport(), Some([1.0, 1.0]), false),
            None
        );
        click(&mut ui, &runtime, &login, [640.0, 355.0]);
        runtime.act(FrontendAction::ContinueFromLogin, &mut scene);
        assert!(controls(&runtime, &login).is_empty());
        assert_eq!(click(&mut ui, &runtime, &login, [640.0, 490.0]), None);
        ui.key(
            &mut runtime,
            &mut login,
            &Key::Character("x".into()),
            Some("x"),
            false,
        );
        assert_eq!(login, "test");
        assert_eq!(
            ui.key(
                &mut runtime,
                &mut login,
                &Key::Named(NamedKey::Escape),
                None,
                false
            ),
            None
        );
    }

    #[test]
    fn frontend_empty_slot_requires_selection_then_create_and_locks_other_slots() {
        let (mut runtime, mut scene) = at(FrontendStage::CharacterSelect);
        let mut ui = FrontendUi::default();
        let action = click(&mut ui, &runtime, "", [640.0, 550.0]).unwrap();
        assert_eq!(action, FrontendAction::SelectCharacter(1));
        runtime.act(action, &mut scene);
        let action = click(&mut ui, &runtime, "", [640.0, 550.0]).unwrap();
        assert_eq!(action, FrontendAction::BeginCreate(1));
        runtime.act(action, &mut scene);
        assert_eq!(click(&mut ui, &runtime, "", [240.0, 550.0]), None);
        let action = ui
            .key(
                &mut runtime,
                &mut String::new(),
                &Key::Named(NamedKey::Escape),
                None,
                false,
            )
            .unwrap();
        runtime.act(action, &mut scene);
        assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
        assert_eq!(
            runtime.visible_stage(),
            Some(FrontendStage::CharacterSelect)
        );
    }

    #[test]
    fn frontend_login_and_navigation_leave_gameplay_disabled() {
        let lifecycle = crate::lifecycle::ClientLifecycle::new("127.0.0.1:5001".parse().unwrap());
        let (mut runtime, mut scene) = at(FrontendStage::Login);
        let mut ui = FrontendUi::default();
        let mut login = String::new();
        let mut gameplay = crate::input::ActionState::default();
        click(&mut ui, &runtime, &login, [640.0, 355.0]);
        for (character, action) in [
            ("a", crate::input::Action::MoveLeft),
            ("d", crate::input::Action::MoveRight),
            (" ", crate::input::Action::Jump),
        ] {
            ui.key(
                &mut runtime,
                &mut login,
                &Key::Character(character.into()),
                Some(character),
                false,
            );
            if lifecycle.gameplay_actions_allowed() {
                gameplay.set_action(action, true, false);
            }
        }
        assert_eq!(login, "ad");
        assert_eq!(gameplay.move_axis(), purgatory_protocol::MoveAxis::Neutral);
        assert!(!gameplay.jump_pending());
        let action = ui
            .key(
                &mut runtime,
                &mut login,
                &Key::Named(NamedKey::Enter),
                None,
                false,
            )
            .unwrap();
        for action in [
            action,
            FrontendAction::SelectChannel(0),
            FrontendAction::SelectCharacter(1),
            FrontendAction::BeginCreate(1),
        ] {
            runtime.act(action, &mut scene);
            runtime.advance(1.0, &mut scene);
            assert_eq!(
                lifecycle.screen(),
                crate::lifecycle::ClientScreen::Connection
            );
            assert!(!lifecycle.gameplay_actions_allowed());
        }
    }

    #[test]
    fn frontend_frames_and_hits_fit_resized_constrained_viewports() {
        let mut assets = crate::asset_runtime::AssetRuntime::new();
        let logo = assets
            .register_image("test.logo", image::RgbaImage::new(4, 1))
            .unwrap();
        let pack = crate::character_assets::embedded_character_visual_pack(&mut assets).unwrap();
        let preview = crate::character_presentation::fixed_base_preview_quads(&pack);
        assert_eq!(preview.len(), 14);
        assert!(preview.iter().all(|quad| quad.is_textured()));
        let assets = FrontendUiAssets {
            logo,
            logo_aspect: 4.0,
        };
        for (width, height) in [
            (1280, 720),
            (800, 600),
            (2560, 1080),
            (1920, 1080),
            (1920, 1009),
        ] {
            let viewport = crate::renderer::constrained_pixel_viewport(width, height).unwrap();
            let outer = ScreenRect {
                min: [viewport.x as f32, viewport.y as f32],
                max: [
                    (viewport.x + viewport.width) as f32,
                    (viewport.y + viewport.height) as f32,
                ],
            };
            for stage in [
                FrontendStage::Login,
                FrontendStage::ChannelSelect,
                FrontendStage::CharacterSelect,
            ] {
                let (mut runtime, mut scene) = at(stage);
                for creating in [false, true] {
                    if creating && stage == FrontendStage::CharacterSelect {
                        runtime.act(FrontendAction::SelectCharacter(1), &mut scene);
                        runtime.act(FrontendAction::BeginCreate(1), &mut scene);
                    }
                    for ppp in [1.0, 1.5, 2.0] {
                        let mut frame = FrontendUi::default().frame(
                            FrontendView {
                                runtime: &runtime,
                                login: "dev.local",
                                status: "Disconnected",
                            },
                            viewport,
                            ppp,
                            assets,
                            None,
                        );
                        frame.add_previews(&runtime, viewport, &preview);
                        if stage == FrontendStage::CharacterSelect {
                            assert_eq!(frame.preview_quads.len(), if creating { 42 } else { 28 });
                            assert!(
                                frame
                                    .preview_quads
                                    .iter()
                                    .flat_map(|quad| quad.corners)
                                    .all(|point| outer.contains(point))
                            );
                            if creating {
                                for row in APPEARANCE_ROWS {
                                    assert!(frame.texts.iter().any(|text| text.content.0 == format!("{row}: DEFAULT")));
                                }
                                for expected in [
                                    "APPEARANCE — LOCKED",
                                    "ATTRIBUTES",
                                    "Customization available later",
                                    "CREATE",
                                    "CANCEL",
                                ] {
                                    assert!(
                                        frame.texts.iter().any(|text| text.content.0 == expected)
                                    );
                                }
                            }
                        }
                        assert!(!frame.texts.is_empty());
                        if stage == FrontendStage::Login {
                            let button = Layout(viewport).rect(CONTINUE);
                            assert!(
                                frame
                                    .rects
                                    .iter()
                                    .any(|rect| rect.min == button.min && rect.max == button.max)
                            );
                        }
                        for (min, max) in frame
                            .rects
                            .iter()
                            .map(|rect| (rect.min, rect.max))
                            .chain(frame.textured_rects.iter().map(|rect| (rect.min, rect.max)))
                        {
                            assert!(outer.contains(min) && outer.contains(max));
                        }
                        assert!(frame.texts.iter().all(|text| outer.contains(text.anchor)));
                        assert!(frame.rects.len() < 64 && frame.textured_rects.len() < 128);
                    }
                    for (bounds, _) in controls(&runtime, "dev.local") {
                        let rect = Layout(viewport).rect(bounds);
                        assert!(outer.contains(rect.min) && outer.contains(rect.max));
                    }
                }
            }
        }
    }
}
