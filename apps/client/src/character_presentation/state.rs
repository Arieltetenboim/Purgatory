//! Semantic presentation state. Intentionally small. No GPU, protocol frames, or World.

use purgatory_common::ContentId;
use purgatory_simulation::EquipmentSlot;

/// Authored RIGHT. Renderer/skeleton adapter later chooses mirroring.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Facing {
    #[default]
    Right,
    Left,
}

/// Minimum activity vocabulary for current client motion.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PresentationActivity {
    #[default]
    Idle,
    Move,
    Jump,
    Fall,
    /// Authoritative Attack one-shot (A5). Not inferred from locomotion.
    Attack,
    /// Authoritative Hurt one-shot (A5). Not inferred from locomotion.
    Hurt,
    /// Persistent dead presentation from replicated Health (`current <= 0`).
    /// Not a oneshot. Overrides Attack/Hurt until Alive again.
    Dead,
    /// Rear-facing climb presentation. Not inferred from velocity.
    ClimbBack,
}

/// Semantic presentation camera/view. Not [`Facing`].
///
/// Selects equipment `visuals.side` / `visuals.back` in Character Presentation.
/// It is not the visual key itself and is not a gameplay climb flag.
///
/// - [`PresentationView::Side`]: 3/4 locomotion. Every authored layer is visible;
///   paint slot equals the authored layer. Attachments use `visuals.side`.
/// - [`PresentationView::Back`]: seen from behind (`ClimbBack`). Authored
///   `ArmFront` / `LegFront` are hidden. Remaining `ArmBack` / `LegBack` occupy
///   the near Front paint slots so they draw over Core/Head. Attachments use
///   `visuals.back` when authored; Side is never substituted.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PresentationView {
    #[default]
    Side,
    Back,
}

/// Idle / Move / Jump / Fall / Attack / Hurt / Dead → Side. ClimbBack → Back.
#[must_use]
pub const fn view_for_activity(activity: PresentationActivity) -> PresentationView {
    match activity {
        PresentationActivity::ClimbBack => PresentationView::Back,
        PresentationActivity::Idle
        | PresentationActivity::Move
        | PresentationActivity::Jump
        | PresentationActivity::Fall
        | PresentationActivity::Attack
        | PresentationActivity::Hurt
        | PresentationActivity::Dead => PresentationView::Side,
    }
}

/// Equipment domain as presentation sees it. No synthesized ContentIds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EquipmentView {
    /// No Equipment domain (`None` on the replica).
    #[default]
    Absent,
    /// Domain present, including all-empty.
    Present([Option<ContentId>; EquipmentSlot::COUNT]),
}

impl EquipmentView {
    #[must_use]
    pub const fn empty_present() -> Self {
        Self::Present([None; EquipmentSlot::COUNT])
    }

    #[must_use]
    pub const fn present(slots: [Option<ContentId>; EquipmentSlot::COUNT]) -> Self {
        Self::Present(slots)
    }

    #[must_use]
    pub fn slot(self, slot: EquipmentSlot) -> Option<ContentId> {
        match self {
            Self::Absent => None,
            Self::Present(slots) => slots[slot.index()],
        }
    }

    #[must_use]
    pub const fn is_absent(self) -> bool {
        matches!(self, Self::Absent)
    }

    #[must_use]
    pub fn is_empty_present(self) -> bool {
        match self {
            Self::Absent => false,
            Self::Present(slots) => slots.iter().all(Option::is_none),
        }
    }
}

/// Common presentation input after local/remote adapters. Downstream must not
/// branch on `is_local`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterPresentationState {
    /// Presented body center (same space as replica / local draw AABB center).
    pub pose: [f32; 2],
    pub facing: Facing,
    pub activity: PresentationActivity,
    pub view: PresentationView,
    pub equipment: EquipmentView,
}

impl CharacterPresentationState {
    #[must_use]
    pub fn is_placeholder_ready(self) -> bool {
        // Side and Back are both valid presentation views.
        self.pose[0].is_finite() && self.pose[1].is_finite()
    }
}
