//! Phase 8E: client character presentation + equipment attachment composition.
//! Phase 8F-A: centralized semantic draw-order contract (no per-item z).
//! Phase 8F-B: Front/Back visibility on the same layer table.
//! Phase 8F-C: activity → PresentationView (ClimbBack → Back).
//! Phase 8F-D: ClimbBack samples authored climb_back.anim.
//! Phase 8F-E: PresentationView selects equipment Side/Back visual keys.
//! Phase 8F-F: 8F closeout (no new presentation features).
//!
//! Local predicted pose and remote interpolated replica feed one common
//! [`CharacterPresentationState`] → [`SkeletonInput`] → skeleton evaluate path.
//! Equipped ContentIds resolve to [`BoundAttachment`]s on equipment change only.
//! The skeleton crate never sees EntityId, World, protocol, or local vs remote.

#![allow(dead_code)]

mod adapters;
mod anchors;
mod bone_map;
mod collection;
mod compose;
mod debug_visual;
mod draw_order;
mod oneshot_table;
mod resolve;
mod skeleton_input;
mod state;

pub(crate) use adapters::{
    LocalMotion, RemoteMotion, apply_climb_back_overlay, equipment_view_from_replica,
    from_local_with_presentation, from_remote_with_presentation,
};
pub(crate) use collection::{CharacterPresentationSet, PresentationEntityKey};
pub(crate) use debug_visual::presentation_debug_quads_with_headwear;
pub(crate) use oneshot_table::PresentationOneShotTable;
pub(crate) use state::PresentationView;

#[cfg(test)]
mod phase8e_tests;
#[cfg(test)]
mod phase8f_a_tests;
#[cfg(test)]
mod phase8f_b_tests;
#[cfg(test)]
mod phase8f_c_tests;
#[cfg(test)]
mod phase8f_d_tests;
#[cfg(test)]
mod phase8f_e_tests;
#[cfg(test)]
mod phase8f_f_tests;
#[cfg(test)]
mod tests;
