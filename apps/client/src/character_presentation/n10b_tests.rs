use purgatory_simulation::EquipmentSlot;

use super::state::{EquipmentView, Facing, PresentationActivity, PresentationView};
use super::{SocialNpcMotion, from_social_npc};

#[test]
fn social_npc_adapter_starts_humanoid_in_idle() {
    let equipment = EquipmentView::Present([None; EquipmentSlot::COUNT]);
    let state = from_social_npc(
        SocialNpcMotion {
            pose: [-17.8, -2.9],
            equipment,
        },
        Facing::Right,
    );

    assert_eq!(state.pose, [-17.8, -2.9]);
    assert_eq!(state.facing, Facing::Right);
    assert_eq!(state.activity, PresentationActivity::Idle);
    assert_eq!(state.view, PresentationView::Side);
    assert_eq!(state.equipment, equipment);
}
