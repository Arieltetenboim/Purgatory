//! Mixed/soak bot roles: persistent load vs churn vs portal navigation.
//!
//! Timer-based `PortalActivate` is not a valid soak. Portal bots walk using
//! ordinary intent until the authoritative replica shows them inside
//! [`purgatory_simulation::in_portal_activation_zone`].

use std::collections::HashMap;

use purgatory_protocol::{
    MoveAxis, ReplicatedKind, ReplicationFrame, ReplicationRecord, WireEntityId,
};
use purgatory_simulation::in_portal_activation_zone;

use crate::scenario::LoadKind;

/// Workload role for one real QUIC bot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BotRole {
    /// Stay connected for the full duration; generate movement/AOI traffic.
    PersistentMove,
    /// Stay connected; seek an authored portal and complete a transition.
    PersistentPortal,
    /// Bounded lifetime + reconnect; must not drain the persistent baseline.
    Churn,
}

/// Counts derived from [`LoadKind`] and bot_count. Does not invent extra scale.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RolePlan {
    pub portal: u32,
    pub churn: u32,
    pub persistent_move: u32,
}

impl RolePlan {
    #[must_use]
    pub fn persistent_target(self) -> u32 {
        self.portal.saturating_add(self.persistent_move)
    }

    #[must_use]
    pub fn total(self) -> u32 {
        self.portal
            .saturating_add(self.churn)
            .saturating_add(self.persistent_move)
    }
}

/// Split `bot_count` into mixed soak roles. Other kinds keep a single role.
#[must_use]
pub fn role_plan(kind: LoadKind, bot_count: u32) -> RolePlan {
    if bot_count == 0 {
        return RolePlan {
            portal: 0,
            churn: 0,
            persistent_move: 0,
        };
    }
    match kind {
        LoadKind::MixedRuntime | LoadKind::Soak => {
            let portal = 1;
            let churn = if bot_count >= 3 {
                (bot_count / 4).max(1).min(bot_count - portal - 1)
            } else {
                0
            };
            let persistent_move = bot_count.saturating_sub(portal + churn);
            RolePlan {
                portal,
                churn,
                persistent_move,
            }
        }
        LoadKind::PortalChurn | LoadKind::PersistenceChurn => RolePlan {
            portal: 1.min(bot_count),
            churn: 0,
            persistent_move: bot_count.saturating_sub(1.min(bot_count)),
        },
        LoadKind::ReconnectChurn => RolePlan {
            portal: 0,
            churn: bot_count,
            persistent_move: 0,
        },
        _ => RolePlan {
            portal: 0,
            churn: 0,
            persistent_move: bot_count,
        },
    }
}

/// `bot_id` is 1-based (controller spawn order).
#[must_use]
pub fn role_for(kind: LoadKind, bot_count: u32, bot_id: u32) -> BotRole {
    let plan = role_plan(kind, bot_count);
    let idx = bot_id.saturating_sub(1);
    if idx < plan.portal {
        BotRole::PersistentPortal
    } else if idx < plan.portal.saturating_add(plan.churn) {
        BotRole::Churn
    } else {
        BotRole::PersistentMove
    }
}

#[must_use]
pub fn requires_portal_gate(kind: LoadKind) -> bool {
    matches!(
        kind,
        LoadKind::MixedRuntime
            | LoadKind::Soak
            | LoadKind::PortalChurn
            | LoadKind::PersistenceChurn
    )
}

#[must_use]
pub fn requires_persistent_baseline(kind: LoadKind) -> bool {
    matches!(kind, LoadKind::MixedRuntime | LoadKind::Soak)
}

#[must_use]
pub fn requires_mixed_churn(kind: LoadKind) -> bool {
    matches!(kind, LoadKind::MixedRuntime | LoadKind::Soak)
}

/// Replica used only for test-client navigation. Not server authority.
#[derive(Clone, Debug, Default)]
pub struct ReplicaView {
    pub local_pose: Option<[f32; 2]>,
    pub observer: Option<(u32, u32, u32)>,
    pub origin_observer: Option<(u32, u32, u32)>,
    pub portals: HashMap<WireEntityId, [f32; 2]>,
    pub transitions: u64,
    pub activate_cooldown: u32,
    pub pending_transition: bool,
    /// Frames where a portal was present in the replica (eligibility signal).
    pub portal_seen_ticks: u64,
    /// Frames where local pose was inside the portal activation zone.
    pub portal_in_zone_ticks: u64,
    last_walk_axis: MoveAxis,
    rejected_in_zone: bool,
}

impl ReplicaView {
    pub fn apply_frame(&mut self, frame: &ReplicationFrame) {
        let addr = (frame.local_map, frame.local_channel, frame.local_instance);
        if self.origin_observer.is_none() {
            self.origin_observer = Some(addr);
        }
        if self.observer.is_some() && self.observer != Some(addr) {
            self.transitions = self.transitions.saturating_add(1);
            self.pending_transition = false;
            self.rejected_in_zone = false;
        }
        self.observer = Some(addr);

        let local = frame.local_player_entity;
        for record in &frame.records {
            match record {
                ReplicationRecord::Enter { entity, .. } => {
                    if entity.kind == ReplicatedKind::Portal {
                        self.portals.insert(entity.entity_id, entity.position);
                    }
                    if entity.entity_id == local {
                        self.local_pose = Some(entity.position);
                    }
                }
                ReplicationRecord::Update {
                    entity_id,
                    position,
                    ..
                } => {
                    if let Some(pos) = position {
                        if *entity_id == local {
                            self.local_pose = Some(*pos);
                        }
                        if let Some(stored) = self.portals.get_mut(entity_id) {
                            *stored = *pos;
                        }
                    }
                }
                ReplicationRecord::Leave { entity_id } => {
                    self.portals.remove(entity_id);
                }
            }
        }
    }

    /// Server rejected this replica-zone activate. Keep walking; do not freeze Neutral.
    pub fn note_out_of_range(&mut self) {
        self.pending_transition = false;
        self.rejected_in_zone = true;
        self.activate_cooldown = 0;
    }

    /// Ordinary walk input toward the nearest known portal, plus an activate
    /// edge only when the replica pose is inside the authoritative zone.
    #[must_use]
    pub fn portal_intent(&mut self) -> PortalIntent {
        if self.activate_cooldown > 0 {
            self.activate_cooldown -= 1;
        }
        let Some(me) = self.local_pose else {
            return PortalIntent::idle();
        };
        let Some((id, portal)) = self.nearest_portal(me) else {
            // Map A spawn is west of the authored portal. Walk east until the
            // replica actually contains the portal; do not activate on a timer.
            self.last_walk_axis = MoveAxis::Right;
            return PortalIntent {
                axis: MoveAxis::Right,
                activate: false,
                target: None,
            };
        };
        self.portal_seen_ticks = self.portal_seen_ticks.saturating_add(1);
        let toward = axis_toward(me[0], portal[0]);
        if toward != MoveAxis::Neutral {
            self.last_walk_axis = toward;
        }
        let in_zone = in_portal_activation_zone(me, portal);
        if in_zone {
            self.portal_in_zone_ticks = self.portal_in_zone_ticks.saturating_add(1);
        }
        if !in_zone {
            self.rejected_in_zone = false;
        }
        // Replica lag: Neutral-on-in-zone froze the actor just outside the
        // server AABB. Dwell only when the replica is centered and the server
        // has not just rejected; after OutOfRange keep the approach axis.
        let axis = if self.rejected_in_zone {
            if self.last_walk_axis != MoveAxis::Neutral {
                self.last_walk_axis
            } else {
                MoveAxis::Right
            }
        } else if toward != MoveAxis::Neutral {
            toward
        } else if in_zone {
            MoveAxis::Neutral
        } else if self.last_walk_axis != MoveAxis::Neutral {
            self.last_walk_axis
        } else {
            MoveAxis::Right
        };
        let activate = in_zone && self.activate_cooldown == 0;
        if activate {
            self.activate_cooldown = 12;
            self.pending_transition = true;
        }
        PortalIntent {
            axis,
            activate,
            target: Some(id),
        }
    }

    fn nearest_portal(&self, me: [f32; 2]) -> Option<(WireEntityId, [f32; 2])> {
        self.portals
            .iter()
            .map(|(id, pos)| (*id, *pos))
            .min_by(|a, b| {
                dist2(me, a.1)
                    .partial_cmp(&dist2(me, b.1))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortalIntent {
    pub axis: MoveAxis,
    pub activate: bool,
    pub target: Option<WireEntityId>,
}

impl PortalIntent {
    #[must_use]
    pub fn idle() -> Self {
        Self {
            axis: MoveAxis::Neutral,
            activate: false,
            target: None,
        }
    }
}

#[must_use]
pub fn axis_toward(from_x: f32, to_x: f32) -> MoveAxis {
    let dx = to_x - from_x;
    if dx > 0.12 {
        MoveAxis::Right
    } else if dx < -0.12 {
        MoveAxis::Left
    } else {
        MoveAxis::Neutral
    }
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_protocol::{ReplicatedKind, SnapshotEntity};

    #[test]
    fn mixed_eight_has_persistent_baseline_and_churn() {
        let plan = role_plan(LoadKind::MixedRuntime, 8);
        assert_eq!(plan.portal, 1);
        assert_eq!(plan.churn, 2);
        assert_eq!(plan.persistent_move, 5);
        assert_eq!(plan.persistent_target(), 6);
        assert_eq!(plan.total(), 8);
        assert_eq!(
            role_for(LoadKind::MixedRuntime, 8, 1),
            BotRole::PersistentPortal
        );
        assert_eq!(role_for(LoadKind::MixedRuntime, 8, 2), BotRole::Churn);
        assert_eq!(role_for(LoadKind::MixedRuntime, 8, 3), BotRole::Churn);
        assert_eq!(
            role_for(LoadKind::MixedRuntime, 8, 4),
            BotRole::PersistentMove
        );
        assert_eq!(
            role_for(LoadKind::MixedRuntime, 8, 8),
            BotRole::PersistentMove
        );
    }

    #[test]
    fn smoke_four_still_has_a_portal_bot() {
        let plan = role_plan(LoadKind::MixedRuntime, 4);
        assert_eq!(plan.portal, 1);
        assert_eq!(plan.churn, 1);
        assert_eq!(plan.persistent_move, 2);
        assert_eq!(plan.persistent_target(), 3);
    }

    #[test]
    fn connectivity_kind_is_all_persistent_move() {
        let plan = role_plan(LoadKind::Connectivity, 8);
        assert_eq!(plan.churn, 0);
        assert_eq!(plan.portal, 0);
        assert_eq!(plan.persistent_move, 8);
    }

    #[test]
    fn churn_does_not_consume_the_only_bots() {
        let plan = role_plan(LoadKind::MixedRuntime, 2);
        assert_eq!(plan.portal, 1);
        assert_eq!(plan.churn, 0);
        assert_eq!(plan.persistent_move, 1);
    }

    fn enter_frame(
        local: WireEntityId,
        portal: WireEntityId,
        me: [f32; 2],
        ppos: [f32; 2],
    ) -> ReplicationFrame {
        ReplicationFrame {
            snapshot_sequence: 1,
            server_tick: 1,
            local_player_entity: local,
            input_epoch: 0,
            last_acknowledged_input_sequence: 0,
            local_grounded: true,
            local_grounded_on: purgatory_protocol::PlatformSupportId(0),
            local_ignored_platform: purgatory_protocol::PlatformSupportId(0),
            continuation_debt: 0,
            local_map: 1,
            local_channel: 0,
            local_instance: 0,
            observer_baseline_epoch: 1,
            records: vec![
                ReplicationRecord::Enter {
                    entity: SnapshotEntity {
                        entity_id: local,
                        kind: ReplicatedKind::Player,
                        position: me,
                        velocity: [0.0, 0.0],
                    },
                    health: None,
                },
                ReplicationRecord::Enter {
                    entity: SnapshotEntity {
                        entity_id: portal,
                        kind: ReplicatedKind::Portal,
                        position: ppos,
                        velocity: [0.0, 0.0],
                    },
                    health: None,
                },
            ],
            aoi_debug: None,
        }
    }

    #[test]
    fn no_replica_portal_walks_east_without_activate() {
        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let mut view = ReplicaView::default();
        let mut frame = enter_frame(
            local,
            WireEntityId {
                index: 99,
                generation: 1,
            },
            [-19.4, -3.0],
            [6.0, -2.9],
        );
        frame.records.truncate(1);
        view.apply_frame(&frame);
        let intent = view.portal_intent();
        assert_eq!(intent.axis, MoveAxis::Right);
        assert!(!intent.activate);
        assert!(intent.target.is_none());
    }

    #[test]
    fn far_from_portal_walks_does_not_activate() {
        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let portal = WireEntityId {
            index: 28,
            generation: 1,
        };
        let mut view = ReplicaView::default();
        view.apply_frame(&enter_frame(local, portal, [-19.4, -3.0], [6.0, -2.9]));
        let intent = view.portal_intent();
        assert_eq!(intent.axis, MoveAxis::Right);
        assert!(!intent.activate);
        assert_eq!(intent.target, Some(portal));
    }

    #[test]
    fn in_zone_activates_once() {
        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let portal = WireEntityId {
            index: 28,
            generation: 1,
        };
        let mut view = ReplicaView::default();
        view.apply_frame(&enter_frame(local, portal, [6.0, -3.0], [6.0, -2.9]));
        let first = view.portal_intent();
        assert_eq!(first.axis, MoveAxis::Neutral);
        assert!(first.activate);
        let second = view.portal_intent();
        assert!(!second.activate);
        assert_eq!(second.axis, MoveAxis::Neutral);
    }

    #[test]
    fn in_zone_but_not_centered_keeps_walking() {
        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let portal = WireEntityId {
            index: 28,
            generation: 1,
        };
        let mut view = ReplicaView::default();
        view.apply_frame(&enter_frame(local, portal, [5.6, -3.0], [6.0, -2.9]));
        let intent = view.portal_intent();
        assert_eq!(intent.axis, MoveAxis::Right);
        assert!(intent.activate);
    }

    #[test]
    fn out_of_range_keeps_walking_instead_of_freezing() {
        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let portal = WireEntityId {
            index: 28,
            generation: 1,
        };
        let mut view = ReplicaView::default();
        view.apply_frame(&enter_frame(local, portal, [6.0, -3.0], [6.0, -2.9]));
        let _ = view.portal_intent();
        view.note_out_of_range();
        let after = view.portal_intent();
        assert_eq!(after.axis, MoveAxis::Right);
        assert!(after.activate);
    }

    #[test]
    fn map_change_counts_as_transition() {
        let local = WireEntityId {
            index: 1,
            generation: 1,
        };
        let portal = WireEntityId {
            index: 28,
            generation: 1,
        };
        let mut view = ReplicaView::default();
        let mut frame = enter_frame(local, portal, [6.0, -3.0], [6.0, -2.9]);
        view.apply_frame(&frame);
        frame.local_map = 2;
        frame.snapshot_sequence = 2;
        view.apply_frame(&frame);
        assert_eq!(view.transitions, 1);
    }
}
