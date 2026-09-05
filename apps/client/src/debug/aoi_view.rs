//! DEV-only mapping of replica entities to semantic labels and policy-rect bands.
//!
//! Presentation only. Does not decide server interest. WantEnter entities are
//! not in the replica, so they cannot be drawn.

use purgatory_protocol::{ReplicatedKind, WireEntityId};
use purgatory_simulation::{AoiRects, point_in_aabb};

use crate::replica::{
    ReplicaLifecycleEvent, ReplicaLifecycleNote, ReplicatedEntity, ReplicatedWorld,
};

/// How long a frame Enter/Leave stays highlighted (~0.5 s at 30 Hz).
pub const RECENT_LIFECYCLE_TICKS: u64 = 15;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplicaRole {
    LocalPlayer,
    RemotePlayer,
    Interactable,
    Portal,
    Npc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AoiBand {
    /// Inside the enter rectangle (and therefore the leave rectangle).
    InEnter,
    /// Inside leave, outside enter — hysteresis retention if Known.
    LeaveHysteresis,
    /// Outside the leave rectangle. A Known entity here should Leave soon.
    OutsideLeave,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplicaEntityDebug {
    pub entity_id: WireEntityId,
    pub role: ReplicaRole,
    pub label: String,
    pub band: AoiBand,
    pub band_label: &'static str,
    pub recent: Option<&'static str>,
    pub position: [f32; 2],
}

#[must_use]
pub fn semantic_label(kind: ReplicatedKind, id: WireEntityId, local: bool) -> String {
    match kind {
        ReplicatedKind::Player if local => format!("PLAYER {id} · LOCAL"),
        ReplicatedKind::Player => format!("PLAYER {id} · REMOTE"),
        ReplicatedKind::Interactable => format!("INTERACTABLE {id}"),
        ReplicatedKind::Portal => format!("PORTAL {id}"),
        ReplicatedKind::Npc => format!("NPC {id}"),
    }
}

#[must_use]
pub fn role_of(kind: ReplicatedKind, local: bool) -> ReplicaRole {
    match kind {
        ReplicatedKind::Player if local => ReplicaRole::LocalPlayer,
        ReplicatedKind::Player => ReplicaRole::RemotePlayer,
        ReplicatedKind::Interactable => ReplicaRole::Interactable,
        ReplicatedKind::Portal => ReplicaRole::Portal,
        ReplicatedKind::Npc => ReplicaRole::Npc,
    }
}

#[must_use]
pub fn classify_band(position: [f32; 2], rects: AoiRects) -> AoiBand {
    if point_in_aabb(position, rects.enter) {
        AoiBand::InEnter
    } else if point_in_aabb(position, rects.leave) {
        AoiBand::LeaveHysteresis
    } else {
        AoiBand::OutsideLeave
    }
}

#[must_use]
pub fn band_label(band: AoiBand) -> &'static str {
    match band {
        AoiBand::InEnter => "Known · Enter AOI",
        AoiBand::LeaveHysteresis => "Known · Leave band",
        AoiBand::OutsideLeave => "Known · outside leave",
    }
}

#[must_use]
pub fn recent_note(
    id: WireEntityId,
    notes: &[ReplicaLifecycleNote],
    now_tick: u64,
) -> Option<&'static str> {
    notes.iter().rev().find_map(|n| {
        if n.entity_id != id {
            return None;
        }
        if now_tick.saturating_sub(n.tick) > RECENT_LIFECYCLE_TICKS {
            return None;
        }
        Some(match n.event {
            ReplicaLifecycleEvent::Entered => "Entered",
            ReplicaLifecycleEvent::Left => "Left",
        })
    })
}

/// Build overlay rows for currently Known replica entities.
#[must_use]
pub fn replica_entity_debug_rows(
    replica: &ReplicatedWorld,
    rects: AoiRects,
    pose_of: impl Fn(WireEntityId) -> Option<[f32; 2]>,
) -> Vec<ReplicaEntityDebug> {
    let local = replica.local_player();
    let notes: Vec<ReplicaLifecycleNote> = replica.recent_lifecycle().collect();
    let now = replica.last_server_tick();
    let mut rows: Vec<ReplicaEntityDebug> = replica
        .iter()
        .map(|entity: ReplicatedEntity| {
            let is_local = Some(entity.entity_id) == local;
            let position = pose_of(entity.entity_id).unwrap_or(entity.position);
            let band = classify_band(position, rects);
            ReplicaEntityDebug {
                entity_id: entity.entity_id,
                role: role_of(entity.kind, is_local),
                label: semantic_label(entity.kind, entity.entity_id, is_local),
                band,
                band_label: band_label(band),
                recent: recent_note(entity.entity_id, &notes, now),
                position,
            }
        })
        .collect();
    rows.sort_by_key(|r| {
        (
            replica_role_tag(r.role),
            r.entity_id.index,
            r.entity_id.generation,
        )
    });
    rows
}

/// Compact world-space chip. Namespace IDs belong in the inspector.
#[must_use]
pub fn compact_world_space_label(role: ReplicaRole) -> &'static str {
    match role {
        ReplicaRole::LocalPlayer => "LOCAL PLAYER",
        ReplicaRole::RemotePlayer => "REMOTE PLAYER",
        ReplicaRole::Interactable => "INTERACTABLE",
        ReplicaRole::Portal => "PORTAL",
        ReplicaRole::Npc => "NPC",
    }
}

#[must_use]
pub fn replica_role_tag(role: ReplicaRole) -> u8 {
    match role {
        ReplicaRole::LocalPlayer => 0,
        ReplicaRole::RemotePlayer => 1,
        ReplicaRole::Interactable => 2,
        ReplicaRole::Portal => 3,
        ReplicaRole::Npc => 4,
    }
}

/// World-space labels may be projected only when source poses and the active
/// camera belong to the same ready presentation world.
///
/// Reuses the gameplay transition gate: `maps_aligned` plus either an idle
/// fade (current map is the presentation world) or presentation-ready
/// ([`crate::map_fade::DestinationReady`] / [`crate::map_fade::MembershipReady`]).
#[must_use]
pub fn world_space_labels_eligible(
    maps_aligned: bool,
    fade_idle: bool,
    destination_ready: bool,
) -> bool {
    maps_aligned && (fade_idle || destination_ready)
}

/// Projectable world-space label entries. Empty when ineligible so no pose is
/// projected through a mismatched camera. Every replica role shares this gate.
#[must_use]
pub fn world_space_label_entries(
    eligible: bool,
    rows: &[ReplicaEntityDebug],
    mut text_of: impl FnMut(&ReplicaEntityDebug) -> String,
) -> Vec<(String, [f32; 2], u8)> {
    if !eligible {
        return Vec::new();
    }
    rows.iter()
        .map(|row| (text_of(row), row.position, replica_role_tag(row.role)))
        .collect()
}

/// Pin the LOCAL PLAYER chip to the same pose as local draw and camera follow.
///
/// Replica/tick X plus a render-rate camera makes this overlay label sawtooth
/// in screen space even when the body is already remainder-extrapolated.
pub fn bind_local_player_label_pose(
    labels: &mut [(String, [f32; 2], u8)],
    presented: Option<[f32; 2]>,
) {
    let Some(pose) = presented else {
        return;
    };
    let tag = replica_role_tag(ReplicaRole::LocalPlayer);
    for (_, world, t) in labels.iter_mut() {
        if *t == tag {
            *world = pose;
        }
    }
}

/// World→NDC used by the overlay. Must not run for ineligible frames.
#[must_use]
pub fn label_ndc(world: [f32; 2], camera: [f32; 2], viewport: [f32; 2]) -> [f32; 2] {
    [
        (world[0] - camera[0]) * (2.0 / viewport[0]),
        (world[1] - camera[1]) * (2.0 / viewport[1]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replica::ReplicatedWorld;
    use purgatory_protocol::{ReplicationFrame, ReplicationRecord, SnapshotEntity};
    use purgatory_simulation::{WorldBounds, aoi_policy_rects};

    fn id(index: u32) -> WireEntityId {
        WireEntityId {
            index,
            generation: 1,
        }
    }

    fn frame(records: Vec<ReplicationRecord>) -> ReplicationFrame {
        ReplicationFrame {
            snapshot_sequence: 1,
            server_tick: 1,
            local_player_entity: id(1),
            input_epoch: 0,
            last_acknowledged_input_sequence: 0,
            local_grounded: false,
            local_grounded_on: purgatory_protocol::PlatformSupportId::NONE,
            local_ignored_platform: purgatory_protocol::PlatformSupportId::NONE,
            continuation_debt: 0,
            local_map: 1,
            local_channel: 0,
            local_instance: 0,
            observer_baseline_epoch: 0,
            records,
            aoi_debug: None,
        }
    }

    #[test]
    fn labels_distinguish_local_remote_player_and_kinds() {
        let local = id(12);
        let remote = id(13);
        assert_eq!(
            semantic_label(ReplicatedKind::Player, local, true),
            "PLAYER 12:1 · LOCAL"
        );
        assert_eq!(
            semantic_label(ReplicatedKind::Player, remote, false),
            "PLAYER 13:1 · REMOTE"
        );
        assert_eq!(
            semantic_label(ReplicatedKind::Interactable, id(19), false),
            "INTERACTABLE 19:1"
        );
        assert_eq!(
            semantic_label(ReplicatedKind::Portal, id(27), false),
            "PORTAL 27:1"
        );
        assert_eq!(
            role_of(ReplicatedKind::Player, false),
            ReplicaRole::RemotePlayer
        );
        assert_eq!(
            compact_world_space_label(ReplicaRole::LocalPlayer),
            "LOCAL PLAYER"
        );
        assert_eq!(
            compact_world_space_label(ReplicaRole::RemotePlayer),
            "REMOTE PLAYER"
        );
        assert_eq!(
            compact_world_space_label(ReplicaRole::Interactable),
            "INTERACTABLE"
        );
        assert_eq!(compact_world_space_label(ReplicaRole::Portal), "PORTAL");
    }

    #[test]
    fn band_classifies_enter_hysteresis_and_outside() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let rects = aoi_policy_rects([0.0, 0.0], bounds);
        assert_eq!(classify_band([0.0, 0.0], rects), AoiBand::InEnter);
        let hx = rects.enter.max_x() + 0.5;
        if hx < rects.leave.max_x() {
            assert_eq!(classify_band([hx, 0.0], rects), AoiBand::LeaveHysteresis);
        }
        assert_eq!(
            classify_band([rects.leave.max_x() + 1.0, 0.0], rects),
            AoiBand::OutsideLeave
        );
    }

    #[test]
    fn replica_rows_mark_remote_player_not_a_generic_id() {
        let local = id(1);
        let remote = id(2);
        let chest = id(19);
        let mut replica = ReplicatedWorld::new();
        replica.apply_frame(frame(vec![
            enter(local, ReplicatedKind::Player, [0.0, 0.0]),
            enter(remote, ReplicatedKind::Player, [2.0, 0.0]),
            enter(chest, ReplicatedKind::Interactable, [1.0, 0.0]),
        ]));
        let rects = aoi_policy_rects([0.0, 0.0], WorldBounds::FOOTNOTE_TEST);
        let rows = replica_entity_debug_rows(&replica, rects, |_| None);
        let remote_row = rows.iter().find(|r| r.entity_id == remote).expect("remote");
        assert_eq!(remote_row.role, ReplicaRole::RemotePlayer);
        assert!(remote_row.label.contains("PLAYER"));
        assert!(remote_row.label.contains("REMOTE"));
        assert!(!remote_row.label.contains("INTERACTABLE"));
        let chest_row = rows.iter().find(|r| r.entity_id == chest).unwrap();
        assert_eq!(chest_row.role, ReplicaRole::Interactable);
        assert!(chest_row.label.starts_with("INTERACTABLE"));
        assert_eq!(
            rows.iter().find(|r| r.entity_id == local).unwrap().recent,
            Some("Entered")
        );
    }

    fn enter(
        entity_id: WireEntityId,
        kind: ReplicatedKind,
        position: [f32; 2],
    ) -> ReplicationRecord {
        ReplicationRecord::Enter {
            entity: SnapshotEntity {
                entity_id,
                kind,
                position,
                velocity: [0.0, 0.0],
            },
            health: None,
            equipment: None,
        }
    }

    fn debug_row(index: u32, role: ReplicaRole, position: [f32; 2]) -> ReplicaEntityDebug {
        let entity_id = id(index);
        let (kind, local) = match role {
            ReplicaRole::LocalPlayer => (ReplicatedKind::Player, true),
            ReplicaRole::RemotePlayer => (ReplicatedKind::Player, false),
            ReplicaRole::Interactable => (ReplicatedKind::Interactable, false),
            ReplicaRole::Portal => (ReplicatedKind::Portal, false),
            ReplicaRole::Npc => (ReplicatedKind::Npc, false),
        };
        ReplicaEntityDebug {
            entity_id,
            role,
            label: semantic_label(kind, entity_id, local),
            band: AoiBand::InEnter,
            band_label: band_label(AoiBand::InEnter),
            recent: None,
            position,
        }
    }

    #[test]
    fn world_space_labels_visible_on_aligned_idle_map() {
        assert!(world_space_labels_eligible(true, true, false));
    }

    #[test]
    fn world_space_labels_hidden_when_maps_unaligned() {
        assert!(!world_space_labels_eligible(false, false, false));
        assert!(!world_space_labels_eligible(false, true, false));
        assert!(!world_space_labels_eligible(false, false, true));
    }

    #[test]
    fn world_space_labels_hidden_after_new_epoch_before_destination_ready() {
        assert!(
            !world_space_labels_eligible(false, false, false),
            "FadeOut/Hold with dest replica and old camera must not project labels"
        );
        assert!(
            !world_space_labels_eligible(true, false, false),
            "aligned geometry without DestinationReady/MembershipReady during a fade is not presentation-ready"
        );
    }

    #[test]
    fn world_space_labels_visible_when_destination_ready_and_aligned() {
        assert!(world_space_labels_eligible(true, false, true));
    }

    #[test]
    fn world_space_label_entries_share_one_gate_for_every_role() {
        let dest = [10.0, -2.9];
        let rows = [
            debug_row(26, ReplicaRole::LocalPlayer, dest),
            debug_row(35, ReplicaRole::RemotePlayer, [8.0, -2.9]),
            debug_row(27, ReplicaRole::Interactable, [4.0, -2.4]),
            debug_row(28, ReplicaRole::Portal, dest),
        ];
        let hidden = world_space_label_entries(false, &rows, |r| r.label.clone());
        assert!(
            hidden.is_empty(),
            "unaligned/blackout must emit no projectable poses"
        );
        let shown = world_space_label_entries(true, &rows, |r| r.label.clone());
        assert_eq!(shown.len(), 4);
        assert!(shown.iter().any(|(_, _, tag)| *tag == 0));
        assert!(shown.iter().any(|(_, _, tag)| *tag == 1));
        assert!(shown.iter().any(|(_, _, tag)| *tag == 2));
        assert!(shown.iter().any(|(_, _, tag)| *tag == 3));
        assert_eq!(shown[0].1, dest);
    }

    #[test]
    fn local_debug_label_uses_presented_pose_not_tick_pose() {
        let tick = [4.00, 0.0];
        let presented = [4.12, 0.0];
        let camera = [2.0, 0.0];
        let viewport = [16.0, 9.0];
        let rows = [debug_row(1, ReplicaRole::LocalPlayer, tick)];
        let mut labels = world_space_label_entries(true, &rows, |r| {
            compact_world_space_label(r.role).to_string()
        });
        assert_eq!(labels[0].1, tick);
        let tick_ndc = label_ndc(tick, camera, viewport);
        let body_ndc = label_ndc(presented, camera, viewport);
        assert!(
            (tick_ndc[0] - body_ndc[0]).abs() > 0.01,
            "tick-posed label would drift from the presented body"
        );
        bind_local_player_label_pose(&mut labels, Some(presented));
        assert_eq!(labels[0].1, presented);
        assert_eq!(label_ndc(labels[0].1, camera, viewport), body_ndc);
    }

    #[test]
    fn dest_pose_must_not_be_projected_through_previous_map_camera() {
        let dest_pose = [10.0, -2.9];
        let old_camera = [6.0, -2.0];
        let dest_camera = [10.0, -2.0];
        let viewport = [16.0, 9.0];
        let mismatched = label_ndc(dest_pose, old_camera, viewport);
        let aligned = label_ndc(dest_pose, dest_camera, viewport);
        assert!(
            mismatched[0].abs() > 0.4,
            "dest x through old camera is a sideways jump"
        );
        assert!(aligned[0].abs() < 0.05);
        let rows = [debug_row(28, ReplicaRole::Portal, dest_pose)];
        assert!(
            world_space_label_entries(false, &rows, |r| r.label.clone()).is_empty(),
            "gate must refuse the mismatched projection"
        );
    }
}
