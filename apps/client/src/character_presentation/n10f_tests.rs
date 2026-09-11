use purgatory_content::{ContentRegistry, LoadMode, default_content_root, load_registry};

use crate::dialogue_animation::DialogueAnimationCatalog;

use super::{
    CharacterPresentationSet, DialogueAnimationRequest, Facing, PresentationEntityKey,
    SocialNpcMotion, apply_local_dialogue_facing, from_social_npc,
};

fn social_state(x: f32) -> super::state::CharacterPresentationState {
    from_social_npc(
        SocialNpcMotion {
            pose: [x, 1.0],
            equipment: super::state::EquipmentView::empty_present(),
        },
        Facing::Right,
    )
}

fn pack() -> ContentRegistry {
    load_registry(&default_content_root(), LoadMode::Shared).expect("shared content")
}

#[test]
fn local_dialogue_facing_points_only_the_derived_state_at_the_player() {
    let ordinary = social_state(4.0);
    let toward_left_player = apply_local_dialogue_facing(ordinary, 4.0, 2.0);
    let toward_right_player = apply_local_dialogue_facing(ordinary, 4.0, 6.0);

    assert_eq!(ordinary.facing, Facing::Right);
    assert_eq!(toward_left_player.facing, Facing::Left);
    assert_eq!(toward_right_player.facing, Facing::Right);
}

#[test]
fn authored_dialogue_clip_restarts_per_visible_line_and_returns_to_idle() {
    let catalog = DialogueAnimationCatalog::load(&default_content_root());
    assert!(catalog.clip("dialogue_talk").is_some());
    let registry = pack();
    let key = PresentationEntityKey::new(20, 1);
    let mut set = CharacterPresentationSet::with_dialogue_animations(catalog);

    set.sync_with_dialogue(
        [(
            key,
            social_state(4.0),
            Some(DialogueAnimationRequest {
                revision: 1,
                authored_id: "dialogue_talk",
            }),
        )],
        &registry,
        0.25,
    );
    let first_t = set.get(key).unwrap().selected_sample_t();
    assert_eq!(
        set.get(key).unwrap().dialogue_animation_id(),
        Some("dialogue_talk")
    );

    set.sync_with_dialogue(
        [(
            key,
            social_state(4.0),
            Some(DialogueAnimationRequest {
                revision: 1,
                authored_id: "dialogue_talk",
            }),
        )],
        &registry,
        0.25,
    );
    assert!(set.get(key).unwrap().selected_sample_t() > first_t);

    set.sync_with_dialogue(
        [(
            key,
            social_state(4.0),
            Some(DialogueAnimationRequest {
                revision: 2,
                authored_id: "dialogue_talk",
            }),
        )],
        &registry,
        0.05,
    );
    assert!(set.get(key).unwrap().selected_sample_t() < first_t);

    set.sync([(key, social_state(4.0))], &registry, 0.05);
    let ordinary = set.get(key).unwrap();
    assert_eq!(ordinary.dialogue_animation_id(), None);
    assert_eq!(
        ordinary.playback_activity(),
        super::PresentationActivity::Idle
    );
}

#[test]
fn missing_cue_falls_back_and_an_override_does_not_leak_to_another_npc() {
    let catalog = DialogueAnimationCatalog::load(&default_content_root());
    let registry = pack();
    let speaking = PresentationEntityKey::new(20, 1);
    let observing = PresentationEntityKey::new(21, 1);
    let mut set = CharacterPresentationSet::with_dialogue_animations(catalog);

    set.sync_with_dialogue(
        [
            (
                speaking,
                social_state(4.0),
                Some(DialogueAnimationRequest {
                    revision: 1,
                    authored_id: "dialogue_talk",
                }),
            ),
            (observing, social_state(5.0), None),
        ],
        &registry,
        0.1,
    );
    assert_eq!(
        set.get(speaking).unwrap().dialogue_animation_id(),
        Some("dialogue_talk")
    );
    assert_eq!(set.get(observing).unwrap().dialogue_animation_id(), None);

    set.sync_with_dialogue(
        [(
            speaking,
            social_state(4.0),
            Some(DialogueAnimationRequest {
                revision: 2,
                authored_id: "missing_cue",
            }),
        )],
        &registry,
        0.1,
    );
    assert_eq!(set.get(speaking).unwrap().dialogue_animation_id(), None);
    assert_eq!(
        set.get(speaking).unwrap().playback_activity(),
        super::PresentationActivity::Idle
    );
}
