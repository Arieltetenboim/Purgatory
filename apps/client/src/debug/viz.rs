//! Client-only FOOTNOTE world gizmos. Presentation only; never mutates simulation.

use purgatory_simulation::{EntityId, PlatformKind, World, WorldBounds};

use super::ui_state::DebugUiState;
use crate::renderer::DrawQuad;

const GROUNDED_HIGHLIGHT: [f32; 4] = [0.25, 0.85, 0.35, 0.35];
const IGNORED_HIGHLIGHT: [f32; 4] = [0.95, 0.35, 0.15, 0.40];
const CONTACT_COLOR: [f32; 4] = [1.0, 0.95, 0.2, 1.0];
const SUPPORT_LINE: [f32; 4] = [0.2, 0.9, 1.0, 1.0];
const VEL_X_COLOR: [f32; 4] = [0.95, 0.45, 0.2, 1.0];
const VEL_Y_COLOR: [f32; 4] = [0.45, 0.55, 1.0, 1.0];
const BOUNDS_COLOR: [f32; 4] = [0.9, 0.2, 0.85, 1.0];
const GRID_COLOR: [f32; 4] = [0.35, 0.4, 0.5, 1.0];
const COLLIDER_TINT: [f32; 4] = [0.8, 0.8, 0.2, 0.25];

const VEL_SCALE: f32 = 0.12;
const BAR_THICKNESS: f32 = 0.06;

/// Build overlay-gated FOOTNOTE gizmos from live world state and UI toggles.
///
/// `local_render_pose` is the same presentation position used to draw the local
/// player (ReplicatedWorld). Player-attached gizmos (velocity, contact) originate
/// there so they stay visually locked to the rendered entity. Platform highlights
/// still use authoritative/local world platform geometry.
#[must_use]
pub fn footnote_debug_quads(
    world: &World,
    ui: &DebugUiState,
    local_render_pose: Option<[f32; 2]>,
) -> Vec<DrawQuad> {
    let mut quads = Vec::with_capacity(16);
    let Some(player) = world.player_body() else {
        return quads;
    };
    let origin = local_render_pose.unwrap_or(player.position);

    if ui.show_colliders {
        for view in world.iter_platforms() {
            let aabb = view.aabb();
            quads.push(DrawQuad {
                center: aabb.center,
                size: [aabb.size()[0] * 1.02, aabb.size()[1] * 1.02],
                color: COLLIDER_TINT,
            });
        }
    }

    if ui.show_grounded_highlight
        && let Some(id) = player.grounded_on
    {
        push_platform_highlight(world, id, GROUNDED_HIGHLIGHT, 1.04, &mut quads);
        if let Some((_, platform)) = world.get_platform(id) {
            let view = world
                .iter_platforms()
                .find(|v| v.id == id)
                .expect("grounded platform");
            let top = view.top_surface();
            let width = platform.half_extents[0] * 2.0;
            quads.push(DrawQuad {
                center: [origin[0], top],
                size: [width.min(player.half_extents[0] * 2.2), 0.05],
                color: SUPPORT_LINE,
            });
        }
        quads.push(DrawQuad {
            center: [origin[0], origin[1] - player.half_extents[1]],
            size: [0.12, 0.12],
            color: CONTACT_COLOR,
        });
    }

    if let Some(id) = player.ignored_platform {
        push_platform_highlight(world, id, IGNORED_HIGHLIGHT, 1.06, &mut quads);
    }

    if ui.show_velocity {
        let vx = player.velocity[0] * VEL_SCALE;
        let vy = player.velocity[1] * VEL_SCALE;
        if vx.abs() > 0.02 {
            quads.push(DrawQuad {
                center: [origin[0] + vx * 0.5, origin[1]],
                size: [vx.abs(), BAR_THICKNESS],
                color: VEL_X_COLOR,
            });
        }
        if vy.abs() > 0.02 {
            quads.push(DrawQuad {
                center: [origin[0], origin[1] + vy * 0.5],
                size: [BAR_THICKNESS, vy.abs()],
                color: VEL_Y_COLOR,
            });
        }
    }

    if ui.show_world_bounds {
        push_bounds_outline(world.bounds(), &mut quads);
    }

    if ui.show_grid {
        push_grid(world.bounds(), &mut quads);
    }

    let _ = PlatformKind::Solid;
    quads
}

fn push_bounds_outline(bounds: WorldBounds, quads: &mut Vec<DrawQuad>) {
    let cx = bounds.center()[0];
    let cy = bounds.center()[1];
    let w = bounds.width();
    let h = bounds.height();
    let t = 0.08;
    quads.push(DrawQuad {
        center: [cx, bounds.max_y],
        size: [w, t],
        color: BOUNDS_COLOR,
    });
    quads.push(DrawQuad {
        center: [cx, bounds.min_y],
        size: [w, t],
        color: BOUNDS_COLOR,
    });
    quads.push(DrawQuad {
        center: [bounds.min_x, cy],
        size: [t, h],
        color: BOUNDS_COLOR,
    });
    quads.push(DrawQuad {
        center: [bounds.max_x, cy],
        size: [t, h],
        color: BOUNDS_COLOR,
    });
}

fn push_grid(bounds: WorldBounds, quads: &mut Vec<DrawQuad>) {
    let step = 2.0;
    let t = 0.03;
    let mut x = (bounds.min_x / step).ceil() * step;
    while x <= bounds.max_x {
        quads.push(DrawQuad {
            center: [x, bounds.center()[1]],
            size: [t, bounds.height()],
            color: GRID_COLOR,
        });
        x += step;
    }
    let mut y = (bounds.min_y / step).ceil() * step;
    while y <= bounds.max_y {
        quads.push(DrawQuad {
            center: [bounds.center()[0], y],
            size: [bounds.width(), t],
            color: GRID_COLOR,
        });
        y += step;
    }
}

fn push_platform_highlight(
    world: &World,
    id: EntityId,
    color: [f32; 4],
    scale: f32,
    quads: &mut Vec<DrawQuad>,
) {
    let Some(view) = world.iter_platforms().find(|v| v.id == id) else {
        return;
    };
    let aabb = view.aabb();
    quads.push(DrawQuad {
        center: aabb.center,
        size: [aabb.size()[0] * scale, aabb.size()[1] * scale],
        color,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_simulation::{ONEWAY_A, ONEWAY_A_POSITION, PLAYER_HALF_EXTENTS, Transform};

    #[test]
    fn grounded_player_produces_highlight_and_contact() {
        let world = World::dev_stage();
        let ui = DebugUiState::default();
        let quads = footnote_debug_quads(&world, &ui, None);
        assert!(
            quads.len() >= 3,
            "expected grounded highlight, support line, contact marker"
        );
    }

    #[test]
    fn ignored_platform_adds_highlight() {
        let mut world = World::dev_stage();
        let oa = world
            .iter_platforms()
            .find(|v| v.platform.kind == PlatformKind::OneWay)
            .expect("oneway")
            .id;
        let top = ONEWAY_A.top_surface(Transform::from_position(ONEWAY_A_POSITION));
        if let Some((transform, player)) = world.player_parts_mut() {
            transform.position = [ONEWAY_A_POSITION[0], top + PLAYER_HALF_EXTENTS[1]];
            player.grounded = false;
            player.grounded_on = None;
            player.ignored_platform = Some(oa);
            player.velocity = [2.0, -1.0];
        }
        let ui = DebugUiState {
            show_colliders: false,
            ..DebugUiState::default()
        };
        let quads = footnote_debug_quads(&world, &ui, None);
        assert!(
            quads.iter().any(|q| q.color == IGNORED_HIGHLIGHT),
            "ignored platform highlight missing"
        );
        assert!(
            quads
                .iter()
                .any(|q| q.color == VEL_X_COLOR || q.color == VEL_Y_COLOR),
            "velocity bars expected"
        );
    }

    #[test]
    fn velocity_gizmo_uses_local_render_pose_origin() {
        let mut world = World::dev_stage();
        if let Some((transform, player)) = world.player_parts_mut() {
            transform.position = [0.0, 0.0];
            player.grounded = false;
            player.grounded_on = None;
            player.ignored_platform = None;
            player.velocity = [10.0, 0.0];
        }
        let ui = DebugUiState {
            show_colliders: false,
            show_grounded_highlight: false,
            show_velocity: true,
            ..DebugUiState::default()
        };
        let render_pose = [5.0, 1.0];
        let quads = footnote_debug_quads(&world, &ui, Some(render_pose));
        let bar = quads
            .iter()
            .find(|q| q.color == VEL_X_COLOR)
            .expect("velocity x bar");
        // Bar center is origin + vx*0.5 along x; y matches render pose.
        assert!((bar.center[1] - render_pose[1]).abs() < 1e-4);
        assert!(
            (bar.center[0] - render_pose[0]).abs() > 0.1,
            "bar should be offset from pose along velocity"
        );
        assert!(
            (bar.center[0] - 0.0).abs() > 1.0,
            "must not use world (0,0) as gizmo origin when render pose is provided"
        );
    }

    #[test]
    fn airborne_without_ignore_has_no_grounded_highlight() {
        let mut world = World::dev_stage();
        if let Some((transform, player)) = world.player_parts_mut() {
            transform.position = [-2.0, 2.0];
            player.grounded = false;
            player.grounded_on = None;
            player.ignored_platform = None;
            player.velocity = [0.0, 0.0];
        }
        let ui = DebugUiState {
            show_colliders: false,
            ..DebugUiState::default()
        };
        let quads = footnote_debug_quads(&world, &ui, None);
        assert!(!quads.iter().any(|q| q.color == GROUNDED_HIGHLIGHT));
        assert!(!quads.iter().any(|q| q.color == IGNORED_HIGHLIGHT));
    }
}
