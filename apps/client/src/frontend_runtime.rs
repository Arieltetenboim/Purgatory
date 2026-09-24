//! Local frontend navigation. Lifecycle owns Connection/Game; the scene owns camera motion.

use crate::frontend_scene::{FrontendScene, FrontendSceneStop};
use purgatory_common::CharacterId;
use purgatory_protocol::{CharacterCreateRejection, CharacterSummary, CreateCharacterResult};

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
    Occupied {
        character_id: CharacterId,
        name: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CharacterAreaMode {
    Browsing,
    Creating { slot: u8 },
}

pub(crate) const CHARACTER_NAME_MAX: usize = purgatory_common::CHARACTER_NAME_MAX_LEN;
pub(crate) use purgatory_common::CharacterNameError;

pub(crate) fn validate_character_name(name: &str) -> Result<(), CharacterNameError> {
    purgatory_common::CharacterName::parse(name).map(|_| ())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CharacterCreationDraft {
    pub(crate) slot: u8,
    pub(crate) name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CreationState {
    Idle,
    Pending,
    Rejected(CharacterCreateRejection),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FrontendSession {
    Disconnected,
    Pending,
    Ready,
}

/// Projection of the latest authoritative ordered roster.
pub(crate) struct CharacterArea {
    pub(crate) slots: [CharacterSlotState; 3],
    pub(crate) creation: CreationState,
    pub(crate) selected_id: Option<CharacterId>,
    pub(crate) mode: CharacterAreaMode,
    pub(crate) selected_slot: Option<u8>,
    pub(crate) draft: Option<CharacterCreationDraft>,
}

impl CharacterArea {
    fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| CharacterSlotState::Empty),
            creation: CreationState::Idle,
            selected_id: None,
            mode: CharacterAreaMode::Browsing,
            selected_slot: None,
            draft: None,
        }
    }

    pub(crate) fn can_create(&self) -> bool {
        self.creation != CreationState::Pending
            && self.draft.as_ref().is_some_and(|draft| {
                self.mode == (CharacterAreaMode::Creating { slot: draft.slot })
                    && self.slots.get(usize::from(draft.slot)) == Some(&CharacterSlotState::Empty)
                    && validate_character_name(&draft.name).is_ok()
            })
    }

    fn cancel_creation(&mut self) {
        self.mode = CharacterAreaMode::Browsing;
        self.draft = None;
        self.creation = CreationState::Idle;
    }

    pub(crate) fn create_slot(&self) -> Option<u8> {
        self.selected_slot.filter(|&slot| {
            self.mode == CharacterAreaMode::Browsing
                && self.slots.get(usize::from(slot)) == Some(&CharacterSlotState::Empty)
        })
    }
}

pub(crate) struct FrontendRuntime {
    pub(crate) session: FrontendSession,
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
            session: FrontendSession::Disconnected,
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
        if self.character_area.creation == CreationState::Pending
            || self.target.is_some()
            || stage == self.stage
        {
            return;
        }
        self.target = Some(stage);
        self.character_area.cancel_creation();
        scene.request(stage.stop());
    }

    pub(crate) fn act(&mut self, action: FrontendAction, scene: &mut FrontendScene) {
        if self.character_area.creation == CreationState::Pending || self.target.is_some() {
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
            } else if action == FrontendAction::CreateCharacter && self.character_area.can_create()
            {
                self.character_area.creation = CreationState::Pending;
            }
            return;
        }
        let next = match (self.stage, action) {
            (FrontendStage::Login, FrontendAction::ContinueFromLogin)
                if self.session == FrontendSession::Ready =>
            {
                FrontendStage::ChannelSelect
            }
            (FrontendStage::ChannelSelect, FrontendAction::SelectChannel(slot @ 0..=1)) => {
                self.selected_channel = Some(slot);
                FrontendStage::CharacterSelect
            }
            (FrontendStage::CharacterSelect, FrontendAction::SelectCharacter(slot @ 0..=2)) => {
                self.character_area.selected_slot = Some(slot);
                self.character_area.selected_id = match self.character_area.slots[usize::from(slot)]
                {
                    CharacterSlotState::Occupied { character_id, .. } => Some(character_id),
                    CharacterSlotState::Empty => None,
                };
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

    pub(crate) fn replace_roster(&mut self, roster: &[CharacterSummary]) {
        self.character_area.slots = std::array::from_fn(|slot| {
            roster.get(slot).map_or(CharacterSlotState::Empty, |entry| {
                CharacterSlotState::Occupied {
                    character_id: entry.character_id,
                    name: entry.display_name.clone(),
                }
            })
        });
        self.character_area.selected_slot = self.character_area.selected_id.and_then(|id| {
            roster
                .iter()
                .position(|entry| entry.character_id == id)
                .map(|i| i as u8)
        });
        if self.character_area.selected_slot.is_none() {
            self.character_area.selected_id = None;
        }
    }

    pub(crate) fn session_ready(&mut self, roster: &[CharacterSummary], scene: &mut FrontendScene) {
        self.session = FrontendSession::Ready;
        self.replace_roster(roster);
        self.request(FrontendStage::ChannelSelect, scene);
    }

    pub(crate) fn create_result(&mut self, result: &CreateCharacterResult) {
        if self.character_area.creation != CreationState::Pending {
            return;
        }
        match result {
            CreateCharacterResult::Created { roster } => {
                // Another session for this DEV profile may have created earlier entries.
                // Global uniqueness plus the preserved draft identifies our accepted name.
                let created = self.character_area.draft.as_ref().and_then(|draft| {
                    roster
                        .iter()
                        .find(|entry| entry.display_name == draft.name)
                        .map(|entry| entry.character_id)
                });
                self.character_area.selected_id = created;
                self.replace_roster(roster);
                self.character_area.cancel_creation();
            }
            CreateCharacterResult::Rejected(reason) => {
                self.character_area.creation = CreationState::Rejected(*reason)
            }
        }
    }

    pub(crate) fn disconnected(&mut self, scene: &mut FrontendScene) {
        self.session = FrontendSession::Disconnected;
        self.character_area = CharacterArea::new();
        self.selected_channel = None;
        self.target = Some(FrontendStage::Login);
        scene.request(FrontendSceneStop::Login);
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
    fn frontend_creation_waits_for_authority_and_preserves_camera_and_case() {
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
        assert_eq!(runtime.character_area.slots[1], CharacterSlotState::Empty);
        assert_eq!(runtime.character_area.creation, CreationState::Pending);
        runtime.create_result(&CreateCharacterResult::Created {
            roster: vec![
                CharacterSummary {
                    character_id: CharacterId::from_raw(10),
                    display_name: "Wanderer".into(),
                },
                CharacterSummary {
                    character_id: CharacterId::from_raw(30),
                    display_name: "Hero123".into(),
                },
                CharacterSummary {
                    character_id: CharacterId::from_raw(20),
                    display_name: "Warden".into(),
                },
            ],
        });
        assert_eq!(
            runtime.character_area.selected_id,
            Some(CharacterId::from_raw(30))
        );
        assert_eq!(runtime.character_area.slots.len(), 3);
        assert_eq!(
            runtime.character_area.slots[1],
            CharacterSlotState::Occupied {
                character_id: purgatory_common::CharacterId::from_raw(30),
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
                character_id: purgatory_common::CharacterId::from_raw(30),
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
#[cfg(test)]
mod pregame_tests {
    use super::*;
    use crate::lifecycle::{ClientLifecycle, ClientScreen};
    use crate::network::NetworkFailureKind;
    use crate::network::state::{ConnectionState, NetworkEvent};

    fn roster(count: usize) -> Vec<CharacterSummary> {
        (0..count)
            .map(|i| CharacterSummary {
                character_id: CharacterId::from_raw(70 + i as u64),
                display_name: format!("Hero{i}"),
            })
            .collect()
    }

    #[test]
    fn frontend_authoritative_slots_pending_rejections_success_and_disconnect() {
        let mut runtime = FrontendRuntime::new();
        let mut scene = FrontendScene::new();
        for count in [0, 1, 3] {
            runtime.replace_roster(&roster(count));
            assert_eq!(runtime.character_area.slots.len(), 3);
            for slot in 0..3 {
                assert_eq!(
                    matches!(
                        runtime.character_area.slots[slot],
                        CharacterSlotState::Occupied { .. }
                    ),
                    slot < count
                );
            }
        }
        runtime.session_ready(&[], &mut scene);
        runtime.advance(1.0, &mut scene);
        runtime.act(FrontendAction::SelectChannel(0), &mut scene);
        runtime.advance(1.0, &mut scene);
        runtime.act(FrontendAction::SelectCharacter(0), &mut scene);
        runtime.act(FrontendAction::BeginCreate(0), &mut scene);
        runtime.character_area.draft.as_mut().unwrap().name = "Hero0".into();
        let camera = scene.source_rect();
        for reason in [
            CharacterCreateRejection::NameTaken,
            CharacterCreateRejection::RosterFull,
            CharacterCreateRejection::InvalidName,
            CharacterCreateRejection::StorageFailure,
        ] {
            runtime.act(FrontendAction::CreateCharacter, &mut scene);
            assert_eq!(runtime.character_area.creation, CreationState::Pending);
            assert!(!runtime.character_area.can_create());
            for action in [
                FrontendAction::CreateCharacter,
                FrontendAction::CancelCreation,
                FrontendAction::Back,
            ] {
                runtime.act(action, &mut scene);
            }
            runtime.request(FrontendStage::Login, &mut scene);
            assert_eq!(runtime.character_area.slots[0], CharacterSlotState::Empty);
            assert_eq!(runtime.character_area.creation, CreationState::Pending);
            runtime.create_result(&CreateCharacterResult::Rejected(reason));
            assert_eq!(runtime.character_area.draft.as_ref().unwrap().name, "Hero0");
            assert_eq!(
                runtime.character_area.mode,
                CharacterAreaMode::Creating { slot: 0 }
            );
        }
        runtime.act(FrontendAction::CreateCharacter, &mut scene);
        runtime.create_result(&CreateCharacterResult::Created { roster: roster(1) });
        assert_eq!(
            runtime.character_area.selected_id,
            Some(CharacterId::from_raw(70))
        );
        assert_eq!(runtime.character_area.mode, CharacterAreaMode::Browsing);
        assert_eq!(scene.source_rect(), camera);
        assert_eq!(
            runtime.visible_stage(),
            Some(FrontendStage::CharacterSelect)
        );
        runtime.act(FrontendAction::SelectCharacter(1), &mut scene);
        runtime.act(FrontendAction::BeginCreate(1), &mut scene);
        runtime.character_area.draft.as_mut().unwrap().name = "Hero1".into();
        runtime.act(FrontendAction::CreateCharacter, &mut scene);
        runtime.disconnected(&mut scene);
        runtime.advance(1.0, &mut scene);
        assert_eq!(runtime.visible_stage(), Some(FrontendStage::Login));
        assert!(
            runtime
                .character_area
                .slots
                .iter()
                .all(|s| *s == CharacterSlotState::Empty)
        );
        assert_eq!(runtime.character_area.selected_id, None);
        assert_eq!(runtime.character_area.creation, CreationState::Idle);
    }

    #[test]
    fn frontend_lifecycle_filters_stale_roster_and_create_and_reconnects() {
        let mut life = ClientLifecycle::new(purgatory_protocol::dev_socket_addr());
        let a = life.try_begin_connect().unwrap();
        life.apply(NetworkEvent::Handshaking { attempt_id: a });
        let ready = |attempt_id, count| NetworkEvent::FrontendSessionReady {
            attempt_id,
            ready: purgatory_protocol::FrontendSessionReady {
                connection_id: purgatory_protocol::ConnectionId::from_raw(9),
                roster: roster(count),
            },
        };
        assert!(life.apply(ready(a, 0)));
        assert_eq!(life.view().state, ConnectionState::Connected);
        assert_eq!(life.screen(), ClientScreen::Connection);
        assert!(!life.gameplay_actions_allowed());
        life.apply(NetworkEvent::Disconnected {
            attempt_id: a,
            kind: NetworkFailureKind::TransportLost,
        });
        let b = life.try_begin_connect().unwrap();
        assert_ne!(a, b);
        life.apply(NetworkEvent::Handshaking { attempt_id: b });
        assert!(!life.apply(ready(a, 3)));
        assert!(!life.apply(NetworkEvent::CharacterCreateResult {
            attempt_id: a,
            result: CreateCharacterResult::Created { roster: roster(3) }
        }));
        assert!(life.apply(ready(b, 1)));
        assert_eq!(life.screen(), ClientScreen::Connection);
        // A generic Connected event never authorizes gameplay; only explicit Welcome delivery does.
        assert!(life.apply(NetworkEvent::GameplayReady { attempt_id: b }));
        assert_eq!(life.screen(), ClientScreen::Game);
    }
    #[test]
    fn frontend_create_selects_own_name_when_another_session_created_first() {
        let mut runtime = FrontendRuntime::new();
        runtime.character_area.mode = CharacterAreaMode::Creating { slot: 0 };
        runtime.character_area.creation = CreationState::Pending;
        runtime.character_area.draft = Some(CharacterCreationDraft {
            slot: 0,
            name: "Hero1".into(),
        });
        runtime.create_result(&CreateCharacterResult::Created { roster: roster(2) });
        assert_eq!(
            runtime.character_area.selected_id,
            Some(CharacterId::from_raw(71))
        );
        assert_eq!(runtime.character_area.selected_slot, Some(1));
    }
}
