//! Frontend-owned instance of ordinary Character Presentation. All fixed base
//! previews share one Idle pose; no runtime entity or session is allocated.
use purgatory_content::ContentRegistry;

use super::state::{CharacterPresentationState, EquipmentView};
use super::{
    CharacterPresentationSet, Facing, PresentationActivity, PresentationEntityKey,
    PresentationView, presentation_debug_quads_with_assets,
};
use crate::{
    asset_runtime::AssetRuntime, character_assets::CharacterVisualPack, renderer::DrawQuad,
};

const PREVIEW: PresentationEntityKey = PresentationEntityKey {
    index: 0,
    generation: 0,
};

pub(crate) struct FrontendCharacterPreview {
    presentation: CharacterPresentationSet,
}

impl FrontendCharacterPreview {
    pub(crate) fn new() -> Self {
        Self {
            presentation: CharacterPresentationSet::new(),
        }
    }

    pub(crate) fn advance(&mut self, visible: bool, registry: &ContentRegistry, dt: f32) {
        let state = CharacterPresentationState {
            pose: [0.0, 0.0],
            facing: Facing::Right,
            activity: PresentationActivity::Idle,
            view: PresentationView::Side,
            equipment: EquipmentView::empty_present(),
        };
        self.presentation
            .sync(visible.then_some((PREVIEW, state)), registry, dt);
    }

    pub(crate) fn quads(&self, assets: &AssetRuntime, pack: &CharacterVisualPack) -> Vec<DrawQuad> {
        self.presentation
            .get(PREVIEW)
            .map_or_else(Vec::new, |entry| {
                presentation_debug_quads_with_assets(
                    self.presentation.bone_map(),
                    entry,
                    assets,
                    pack,
                    1.0,
                    true,
                    PresentationView::Side,
                    0,
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_idle_advances_loops_without_gameplay_and_resets_when_hidden() {
        let mut preview = FrontendCharacterPreview::new();
        let registry = ContentRegistry::new();
        preview.advance(true, &registry, 0.125);
        let entry = preview.presentation.get(PREVIEW).unwrap();
        assert_eq!(entry.state().activity, PresentationActivity::Idle);
        assert_eq!(entry.playback_activity(), PresentationActivity::Idle);
        assert!((entry.selected_sample_t() - 0.125).abs() < 0.0001);
        preview.advance(
            true,
            &registry,
            purgatory_animation::a3_idle_clip().duration(),
        );
        assert!(
            (preview
                .presentation
                .get(PREVIEW)
                .unwrap()
                .selected_sample_t()
                - 0.125)
                .abs()
                < 0.0001
        );
        preview.advance(false, &registry, 0.5);
        assert!(preview.presentation.is_empty());
        preview.advance(true, &registry, 0.0);
        assert_eq!(
            preview
                .presentation
                .get(PREVIEW)
                .unwrap()
                .selected_sample_t(),
            0.0
        );
    }
}
