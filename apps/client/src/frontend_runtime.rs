//! Local frontend navigation. Lifecycle owns Connection/Game; the scene owns camera motion.

use crate::frontend_scene::{FrontendScene, FrontendSceneStop};

/// Intro presentation time, supplied by the client frame delta (never wall-clock reads).
const INTRO_DELAY_SECONDS: f32 = 1.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrontendStage {
    Intro,
    Login,
    ChannelSelect,
    CharacterSelect,
}

impl FrontendStage {
    fn stop(self) -> FrontendSceneStop {
        match self {
            Self::Intro => FrontendSceneStop::Intro,
            Self::Login => FrontendSceneStop::Login,
            Self::ChannelSelect => FrontendSceneStop::Channel,
            Self::CharacterSelect => FrontendSceneStop::Character,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrontendAction {
    ContinueFromLogin,
    SelectChannel(u8),
    SelectCharacter(u8),
    BeginCreate(u8),
    CancelCreation,
    CreateCharacter,
    Back,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CharacterSlotState {
    Empty,
    Occupied { name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CharacterAreaMode {
    Browsing,
    Creating { slot: u8 },
}

pub(crate) const CHARACTER_NAME_MAX: usize = 12;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CharacterNameError {
    Length,
    Characters,
}

pub(crate) fn validate_character_name(name: &str) -> Result<(), CharacterNameError> {
    if !name.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err(CharacterNameError::Characters);
    }
    if !(3..=CHARACTER_NAME_MAX).contains(&name.len()) {
        return Err(CharacterNameError::Length);
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CharacterCreationDraft {
    pub(crate) slot: u8,
    pub(crate) name: String,
}

/// Local R4 proof roster, not server identities or persisted characters.
pub(crate) struct CharacterArea {
    pub(crate) slots: [CharacterSlotState; 3],
    pub(crate) mode: CharacterAreaMode,
    pub(crate) selected_slot: Option<u8>,
    pub(crate) draft: Option<CharacterCreationDraft>,
}

impl CharacterArea {
    fn new() -> Self {
        Self {
            slots: [
                CharacterSlotState::Occupied {
                    name: "Local Wanderer".into(),
                },
                CharacterSlotState::Empty,
                CharacterSlotState::Occupied {
                    name: "Local Warden".into(),
                },
            ],
            mode: CharacterAreaMode::Browsing,
            selected_slot: None,
            draft: None,
        }
    }

    pub(crate) fn can_create(&self) -> bool {
        self.draft.as_ref().is_some_and(|draft| {
            self.mode == (CharacterAreaMode::Creating { slot: draft.slot })
                && self.slots.get(usize::from(draft.slot)) == Some(&CharacterSlotState::Empty)
                && validate_character_name(&draft.name).is_ok()
        })
    }

    fn cancel_creation(&mut self) {
        self.mode = CharacterAreaMode::Browsing;
        self.draft = None;
    }

    pub(crate) fn create_slot(&self) -> Option<u8> {
        self.selected_slot.filter(|&slot| {
            self.mode == CharacterAreaMode::Browsing
                && self.slots.get(usize::from(slot)) == Some(&CharacterSlotState::Empty)
        })
    }
}

pub(crate) struct FrontendRuntime {
    stage: FrontendStage,
    target: Option<FrontendStage>,
    intro_elapsed: f32,
    // Zero-based local placeholder slots, never authoritative identities.
    selected_channel: Option<u8>,
    pub(crate) character_area: CharacterArea,
}

impl FrontendRuntime {
    pub(crate) fn new() -> Self {
        Self {
            stage: FrontendStage::Intro,
            target: None,
            intro_elapsed: 0.0,
            selected_channel: None,
            character_area: CharacterArea::new(),
        }
    }

    /// No foreground is painted or accepts input during travel (or during Intro).
    pub(crate) fn visible_stage(&self) -> Option<FrontendStage> {
        (self.target.is_none() && self.stage != FrontendStage::Intro).then_some(self.stage)
    }

    pub(crate) fn selected_channel(&self) -> Option<u8> {
        self.selected_channel
    }

    pub(crate) fn selected_character(&self) -> Option<u8> {
        self.character_area.selected_slot.filter(|&slot| {
            matches!(
                self.character_area.slots.get(usize::from(slot)),
                Some(CharacterSlotState::Occupied { .. })
            )
        })
    }

    /// Also used by diagnostic shortcuts; active transitions cannot be retargeted.
    pub(crate) fn request(&mut self, stage: FrontendStage, scene: &mut FrontendScene) {
        if self.target.is_some() || stage == self.stage {
            return;
        }
        self.target = Some(stage);
        self.character_area.cancel_creation();
        scene.request(stage.stop());
    }

    pub(crate) fn act(&mut self, action: FrontendAction, scene: &mut FrontendScene) {
        if self.target.is_some() {
            return;
        }
        if self.stage == FrontendStage::CharacterSelect
            && matches!(self.character_area.mode, CharacterAreaMode::Creating { .. })
        {
            if matches!(
                action,
                FrontendAction::Back | FrontendAction::CancelCreation
            ) {
                self.character_area.cancel_creation();
            } else if action == FrontendAction::CreateCharacter
                && self.character_area.can_create()
                && let Some(draft) = self.character_area.draft.take()
            {
                self.character_area.slots[usize::from(draft.slot)] =
                    CharacterSlotState::Occupied { name: draft.name };
                self.character_area.selected_slot = Some(draft.slot);
                self.character_area.mode = CharacterAreaMode::Browsing;
            }
            return;
        }
        let next = match (self.stage, action) {
            (FrontendStage::Login, FrontendAction::ContinueFromLogin) => {
                FrontendStage::ChannelSelect
            }
            (FrontendStage::ChannelSelect, FrontendAction::SelectChannel(slot @ 0..=1)) => {
                self.selected_channel = Some(slot);
                FrontendStage::CharacterSelect
            }
            (FrontendStage::CharacterSelect, FrontendAction::SelectCharacter(slot @ 0..=2)) => {
                self.character_area.selected_slot = Some(slot);
                return;
            }
            (FrontendStage::CharacterSelect, FrontendAction::BeginCreate(slot)) => {
                if self.character_area.create_slot() == Some(slot) {
                    self.character_area.mode = CharacterAreaMode::Creating { slot };
                    self.character_area.draft = Some(CharacterCreationDraft {
                        slot,
                        name: String::new(),
                    });
                }
                return;
            }
            (FrontendStage::CharacterSelect, FrontendAction::Back) => FrontendStage::ChannelSelect,
            (FrontendStage::ChannelSelect, FrontendAction::Back) => FrontendStage::Login,
            _ => return,
        };
        self.request(next, scene);
    }

    pub(crate) fn advance(&mut self, dt: f32, scene: &mut FrontendScene) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        if let Some(target) = self.target {
            scene.advance(dt);
            if scene.is_at(target.stop()) {
                self.stage = target;
                self.target = None;
                self.intro_elapsed = 0.0;
            }
        } else if self.stage == FrontendStage::Intro {
            self.intro_elapsed += dt;
            if self.intro_elapsed >= INTRO_DELAY_SECONDS {
                self.request(FrontendStage::Login, scene);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn frontend_character_name_contract() {
        for name in ["ABC", "abc123", "Ab3", "Abc123Def456"] {
            assert_eq!(super::validate_character_name(name), Ok(()), "{name}");
        }
        for name in [
            "",
            "AB",
            "Abc123Def4567",
            "A B",
            "A_B",
            "A.B",
            "A-B",
            "A!B",
            "A@B",
            "אבג",
            "Abé",
        ] {
            assert!(super::validate_character_name(name).is_err(), "{name}");
        }
    }

    #[test]
    fn frontend_creation_validates_commits_locally_and_preserves_camera_and_case() {
        let (mut runtime, mut scene) = at(FrontendStage::CharacterSelect);
        let camera = scene.source_rect();
        for slot in [0, 2] {
            runtime.act(FrontendAction::SelectCharacter(slot), &mut scene);
            runtime.act(FrontendAction::BeginCreate(slot), &mut scene);
            assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
            assert!(runtime.character_area.draft.is_none());
        }
        runtime.act(FrontendAction::SelectCharacter(1), &mut scene);
        runtime.act(FrontendAction::BeginCreate(1), &mut scene);
        assert_eq!(runtime.character_area.draft.as_ref().unwrap().name, "");
        for name in ["", "Ab", "abc_", "Abc123Def4567", "אבג"] {
            runtime.character_area.draft.as_mut().unwrap().name = name.into();
            assert!(!runtime.character_area.can_create());
            runtime.act(FrontendAction::CreateCharacter, &mut scene);
            assert_eq!(runtime.character_area.slots[1], CharacterSlotState::Empty);
        }
        for action in [
            FrontendAction::SelectCharacter(0),
            FrontendAction::SelectChannel(0),
            FrontendAction::BeginCreate(2),
        ] {
            runtime.act(action, &mut scene);
            assert_eq!(
                runtime.character_area.mode,
                CharacterAreaMode::Creating { slot: 1 }
            );
            assert_eq!(runtime.character_area.selected_slot, Some(1));
        }
        runtime.character_area.draft.as_mut().unwrap().name = "Hero123".into();
        assert!(runtime.character_area.can_create());
        runtime.act(FrontendAction::CreateCharacter, &mut scene);
        assert_eq!(runtime.character_area.slots.len(), 3);
        assert_eq!(
            runtime.character_area.slots[1],
            CharacterSlotState::Occupied {
                name: "Hero123".into()
            }
        );
        assert_eq!(runtime.selected_character(), Some(1));
        assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
        assert!(runtime.character_area.draft.is_none());
        assert_eq!(
            runtime.visible_stage(),
            Some(FrontendStage::CharacterSelect)
        );
        assert_eq!(scene.source_rect(), camera);
        assert!(scene.is_at(FrontendSceneStop::Character));
        runtime.act(FrontendAction::BeginCreate(1), &mut scene);
        runtime.act(FrontendAction::CreateCharacter, &mut scene);
        assert!(runtime.character_area.draft.is_none());
        runtime.act(FrontendAction::Back, &mut scene);
        runtime.advance(1.0, &mut scene);
        assert_eq!(runtime.visible_stage(), Some(FrontendStage::ChannelSelect));
        assert_eq!(
            runtime.character_area.slots[1],
            CharacterSlotState::Occupied {
                name: "Hero123".into()
            }
        );
    }

    #[test]
    fn frontend_cancel_and_dev_navigation_discard_drafts() {
        for action in [FrontendAction::CancelCreation, FrontendAction::Back] {
            let (mut runtime, mut scene) = at(FrontendStage::CharacterSelect);
            let camera = scene.source_rect();
            runtime.act(FrontendAction::SelectCharacter(1), &mut scene);
            runtime.act(FrontendAction::BeginCreate(1), &mut scene);
            runtime.character_area.draft.as_mut().unwrap().name = "Abandoned".into();
            runtime.act(action, &mut scene);
            assert!(runtime.character_area.draft.is_none());
            assert_eq!(runtime.character_area.slots[1], CharacterSlotState::Empty);
            assert_eq!(runtime.selected_character(), None);
            assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
            assert_eq!(scene.source_rect(), camera);
            assert!(scene.is_at(FrontendSceneStop::Character));
            runtime.act(FrontendAction::BeginCreate(1), &mut scene);
            assert_eq!(runtime.character_area.draft.as_ref().unwrap().name, "");
        }
        for destination in [
            FrontendStage::Intro,
            FrontendStage::Login,
            FrontendStage::ChannelSelect,
        ] {
            let (mut runtime, mut scene) = at(FrontendStage::CharacterSelect);
            runtime.act(FrontendAction::SelectCharacter(1), &mut scene);
            runtime.act(FrontendAction::BeginCreate(1), &mut scene);
            runtime.character_area.draft.as_mut().unwrap().name = "Discard".into();
            runtime.request(destination, &mut scene);
            assert!(runtime.character_area.draft.is_none());
            assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
            runtime.advance(1.0, &mut scene);
            runtime.request(FrontendStage::CharacterSelect, &mut scene);
            runtime.advance(1.0, &mut scene);
            assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
        }
    }
    use super::*;

    fn at(stage: FrontendStage) -> (FrontendRuntime, FrontendScene) {
        let mut runtime = FrontendRuntime::new();
        let mut scene = FrontendScene::new();
        runtime.request(stage, &mut scene);
        if stage != FrontendStage::Intro {
            runtime.advance(1.0, &mut scene);
        }
        (runtime, scene)
    }

    #[test]
    fn stages_use_the_locked_camera_stops() {
        for (stage, stop) in [
            (FrontendStage::Intro, FrontendSceneStop::Intro),
            (FrontendStage::Login, FrontendSceneStop::Login),
            (FrontendStage::ChannelSelect, FrontendSceneStop::Channel),
            (FrontendStage::CharacterSelect, FrontendSceneStop::Character),
        ] {
            let (_, scene) = at(stage);
            assert!(scene.is_at(stop));
        }
    }

    #[test]
    fn startup_and_intro_delay() {
        let (mut runtime, mut scene) = at(FrontendStage::Intro);
        assert_eq!(runtime.stage, FrontendStage::Intro);
        assert!(scene.is_at(FrontendSceneStop::Intro));
        assert_eq!(runtime.visible_stage(), None);
        runtime.advance(INTRO_DELAY_SECONDS - 0.25, &mut scene);
        assert_eq!(runtime.target, None);
        runtime.advance(0.25, &mut scene);
        assert_eq!(runtime.target, Some(FrontendStage::Login));
        assert_eq!(runtime.stage, FrontendStage::Intro);
        assert!(!scene.is_at(FrontendSceneStop::Login));
        runtime.advance(1.0, &mut scene);
        assert_eq!(runtime.visible_stage(), Some(FrontendStage::Login));
    }

    #[test]
    fn forward_and_back_wait_for_camera_arrival() {
        for (from, action, to) in [
            (
                FrontendStage::Login,
                FrontendAction::ContinueFromLogin,
                FrontendStage::ChannelSelect,
            ),
            (
                FrontendStage::ChannelSelect,
                FrontendAction::SelectChannel(1),
                FrontendStage::CharacterSelect,
            ),
            (
                FrontendStage::CharacterSelect,
                FrontendAction::Back,
                FrontendStage::ChannelSelect,
            ),
            (
                FrontendStage::ChannelSelect,
                FrontendAction::Back,
                FrontendStage::Login,
            ),
        ] {
            let (mut runtime, mut scene) = at(from);
            runtime.act(action, &mut scene);
            assert_eq!(runtime.target, Some(to));
            assert_eq!(runtime.visible_stage(), None);
            runtime.advance(0.5, &mut scene);
            assert_eq!(runtime.stage, from);
            assert!(!scene.is_at(to.stop()));
            runtime.advance(0.5, &mut scene);
            assert_eq!(runtime.stage, to);
            assert_eq!(runtime.target, None);
            assert_eq!(runtime.visible_stage(), Some(to));
            assert!(scene.is_at(to.stop()));
        }
    }

    #[test]
    fn login_and_intro_back_and_current_stage_requests_are_noops() {
        for stage in [
            FrontendStage::Intro,
            FrontendStage::Login,
            FrontendStage::ChannelSelect,
            FrontendStage::CharacterSelect,
        ] {
            let (mut runtime, mut scene) = at(stage);
            runtime.request(stage, &mut scene);
            assert_eq!(runtime.target, None);
            assert!(scene.is_at(stage.stop()));
            if matches!(stage, FrontendStage::Intro | FrontendStage::Login) {
                runtime.act(FrontendAction::Back, &mut scene);
                assert_eq!(runtime.target, None);
            }
        }
    }

    #[test]
    fn transition_ignores_spam_back_selection_and_diagnostic_retarget() {
        let (mut runtime, mut scene) = at(FrontendStage::ChannelSelect);
        runtime.act(FrontendAction::SelectChannel(0), &mut scene);
        runtime.advance(0.5, &mut scene);
        let rect = scene.source_rect();
        for action in [
            FrontendAction::Back,
            FrontendAction::ContinueFromLogin,
            FrontendAction::SelectChannel(1),
            FrontendAction::SelectCharacter(2),
        ] {
            runtime.act(action, &mut scene);
        }
        runtime.request(FrontendStage::Login, &mut scene);
        assert_eq!(scene.source_rect(), rect);
        assert_eq!(runtime.selected_channel(), Some(0));
        assert_eq!(runtime.selected_character(), None);
        assert_eq!(runtime.stage, FrontendStage::ChannelSelect);
        runtime.advance(0.5, &mut scene);
        assert_eq!(
            runtime.visible_stage(),
            Some(FrontendStage::CharacterSelect)
        );
    }

    #[test]
    fn local_selections_persist_through_forward_and_back() {
        let (mut runtime, mut scene) = at(FrontendStage::Login);
        for action in [
            FrontendAction::ContinueFromLogin,
            FrontendAction::SelectChannel(1),
            FrontendAction::SelectCharacter(2),
            FrontendAction::Back,
            FrontendAction::Back,
            FrontendAction::ContinueFromLogin,
            FrontendAction::SelectChannel(1),
        ] {
            runtime.act(action, &mut scene);
            runtime.advance(1.0, &mut scene);
        }
        assert_eq!(
            runtime.visible_stage(),
            Some(FrontendStage::CharacterSelect)
        );
        assert_eq!(runtime.selected_channel(), Some(1));
        assert_eq!(runtime.selected_character(), Some(2));
        assert_eq!(runtime.target, None); // Character selection never enters Game.
    }

    #[test]
    fn invalid_dt_actions_and_slots_are_ignored() {
        let (mut runtime, mut scene) = at(FrontendStage::Intro);
        for dt in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            runtime.advance(dt, &mut scene);
        }
        assert_eq!(runtime.intro_elapsed, 0.0);
        for stage in [
            FrontendStage::Intro,
            FrontendStage::Login,
            FrontendStage::ChannelSelect,
            FrontendStage::CharacterSelect,
        ] {
            let (mut runtime, mut scene) = at(stage);
            runtime.act(FrontendAction::SelectChannel(9), &mut scene);
            runtime.act(FrontendAction::SelectCharacter(9), &mut scene);
            assert_eq!(runtime.target, None);
            assert_eq!(runtime.selected_channel(), None);
            assert_eq!(runtime.selected_character(), None);
        }
    }

    #[test]
    fn diagnostic_intro_restarts_presentation_delay_after_arrival() {
        let (mut runtime, mut scene) = at(FrontendStage::Login);
        runtime.request(FrontendStage::Intro, &mut scene);
        runtime.advance(1.0, &mut scene);
        assert_eq!(runtime.intro_elapsed, 0.0);
        runtime.advance(INTRO_DELAY_SECONDS, &mut scene);
        assert_eq!(runtime.target, Some(FrontendStage::Login));
    }

    #[test]
    fn character_roster_distinguishes_area_selection_from_playable_selection() {
        let (mut runtime, mut scene) = at(FrontendStage::CharacterSelect);
        assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
        assert_eq!(runtime.character_area.slots.len(), 3);
        assert!(matches!(
            runtime.character_area.slots[0],
            CharacterSlotState::Occupied { .. }
        ));
        assert_eq!(runtime.character_area.slots[1], CharacterSlotState::Empty);
        runtime.act(FrontendAction::SelectCharacter(0), &mut scene);
        assert_eq!(runtime.selected_character(), Some(0));
        assert_eq!(runtime.character_area.create_slot(), None);
        runtime.act(FrontendAction::BeginCreate(0), &mut scene);
        assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
        runtime.act(FrontendAction::BeginCreate(1), &mut scene);
        assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
        runtime.act(FrontendAction::SelectCharacter(1), &mut scene);
        assert_eq!(runtime.character_area.selected_slot, Some(1));
        assert_eq!(runtime.selected_character(), None);
        assert_eq!(runtime.character_area.create_slot(), Some(1));
    }

    #[test]
    fn creation_cancel_and_back_stay_at_character_camera_then_browsing_back_travels() {
        for cancel in [FrontendAction::CancelCreation, FrontendAction::Back] {
            let (mut runtime, mut scene) = at(FrontendStage::CharacterSelect);
            runtime.act(FrontendAction::SelectCharacter(1), &mut scene);
            let rect = scene.source_rect();
            runtime.act(FrontendAction::BeginCreate(1), &mut scene);
            assert_eq!(
                runtime.character_area.mode,
                CharacterAreaMode::Creating { slot: 1 }
            );
            assert_eq!(runtime.character_area.create_slot(), None);
            runtime.act(FrontendAction::SelectCharacter(0), &mut scene);
            runtime.act(FrontendAction::BeginCreate(2), &mut scene);
            assert_eq!(runtime.character_area.selected_slot, Some(1));
            runtime.advance(2.0, &mut scene);
            assert_eq!(scene.source_rect(), rect);
            assert!(scene.is_at(FrontendSceneStop::Character));
            runtime.act(cancel, &mut scene);
            assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
            assert_eq!(
                runtime.visible_stage(),
                Some(FrontendStage::CharacterSelect)
            );
            assert_eq!(scene.source_rect(), rect);
            runtime.act(FrontendAction::Back, &mut scene);
            assert_eq!(runtime.target, Some(FrontendStage::ChannelSelect));
            assert_eq!(runtime.visible_stage(), None);
            runtime.act(FrontendAction::BeginCreate(1), &mut scene);
            assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
        }
    }

    #[test]
    fn diagnostic_departure_discards_creation_mode() {
        let (mut runtime, mut scene) = at(FrontendStage::CharacterSelect);
        runtime.act(FrontendAction::SelectCharacter(1), &mut scene);
        runtime.act(FrontendAction::BeginCreate(1), &mut scene);
        runtime.request(FrontendStage::Login, &mut scene);
        assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
        runtime.advance(1.0, &mut scene);
        runtime.request(FrontendStage::CharacterSelect, &mut scene);
        runtime.advance(1.0, &mut scene);
        assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
    }
}
