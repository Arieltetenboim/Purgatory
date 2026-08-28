//! FOOTNOTE tick orchestration: intent, accel, gravity, integrate, contact.

use crate::body::PlayerState;
use crate::collision::{recover_solid_penetration, resolve_horizontal, resolve_vertical};
use crate::contact::CONTACT_EPSILON;
use crate::entity::{EntityId, EntityKind};
use crate::footnote::FootnoteConfig;
use crate::footnote::contact::ContactEvent;
use crate::input::PlayerInput;
use crate::motion_debug::{CorrectionAxis, PlayerMotionDebug, ResponseKind};
use crate::platform::{Approach, PlatformKind, PlatformView};
use crate::transform::Transform;
use crate::world::World;
#[derive(Clone, Copy)]
struct PlatformScratch {
    data: [Option<PlatformView>; Self::CAP],
    len: usize,
}

impl PlatformScratch {
    // Phase 4.6 FOOTNOTE arena can exceed 30 platforms; keep stack-only.
    const CAP: usize = 64;

    fn push(&mut self, view: PlatformView) {
        if self.len < Self::CAP {
            self.data[self.len] = Some(view);
            self.len += 1;
        }
    }

    fn iter(&self) -> impl Iterator<Item = PlatformView> + '_ {
        self.data[..self.len].iter().filter_map(|slot| *slot)
    }

    fn find(&self, id: EntityId) -> Option<PlatformView> {
        self.iter().find(|view| view.id == id)
    }
}

impl Default for PlatformScratch {
    fn default() -> Self {
        Self {
            data: [None; Self::CAP],
            len: 0,
        }
    }
}

impl World {
    /// Integrate one fixed simulation tick for the local player via FOOTNOTE.
    pub fn tick(&mut self, dt_seconds: f32, input: PlayerInput) {
        if let Some(id) = self.player_id() {
            self.tick_player(id, dt_seconds, input);
        }
    }

    /// Integrate one fixed tick for a specific player entity.
    ///
    /// Stale IDs are a no-op. FOOTNOTE rules are unchanged.
    pub fn tick_player(&mut self, id: EntityId, dt_seconds: f32, input: PlayerInput) {
        self.tick_player_with_config(id, dt_seconds, input, FootnoteConfig::DEFAULT);
    }

    pub fn tick_with_config(
        &mut self,
        dt_seconds: f32,
        input: PlayerInput,
        config: FootnoteConfig,
    ) {
        if let Some(id) = self.player_id() {
            self.tick_player_with_config(id, dt_seconds, input, config);
        }
    }

    pub fn tick_player_with_config(
        &mut self,
        id: EntityId,
        dt_seconds: f32,
        input: PlayerInput,
        config: FootnoteConfig,
    ) {
        debug_assert!(dt_seconds > 0.0 && dt_seconds.is_finite());

        if self.get_player(id).is_none() {
            return;
        }

        self.clear_stale_footnote_ids_for(id);

        let (prev_pos, prev_half, prev_grounded_on) = {
            let Some((transform, player)) = self.get_player(id) else {
                return;
            };
            (transform.position, player.half_extents, player.grounded_on)
        };
        let previous_bottom = prev_pos[1] - prev_half[1];
        let previous_top = prev_pos[1] + prev_half[1];
        let previous_left = prev_pos[0] - prev_half[0];
        let previous_right = prev_pos[0] + prev_half[0];
        let support_kind = prev_grounded_on.and_then(|id| self.platform_kind(id));

        let mut scratch = PlatformScratch::default();
        for view in self.iter_platforms() {
            scratch.push(view);
        }

        let mut collision_candidate = None;
        let mut correction = [0.0_f32, 0.0];
        let mut correction_axis = CorrectionAxis::None;
        let mut response_kind = ResponseKind::None;
        let mut pre_integrate_velocity;

        {
            let Some((transform, player)) = self.player_parts_mut_for(id) else {
                return;
            };

            // Exceptional recovery only when already meaningfully inside a Solid.
            if let Some(rec) = recover_solid_penetration(transform, player, scratch.iter()) {
                collision_candidate = Some(rec.platform);
                correction = rec.correction;
                correction_axis = if rec.correction[0].abs() >= rec.correction[1].abs() {
                    CorrectionAxis::Horizontal
                } else {
                    CorrectionAxis::Vertical
                };
                response_kind = ResponseKind::Recovery;
            }

            let mut contact = ContactEvent::None;
            let dropped = apply_drop_through(player, input, support_kind, &mut contact);
            player.last_contact = contact;

            if !dropped {
                apply_jump(player, input, &config);
            }
            apply_horizontal(player, input, &config, dt_seconds);

            // Grounded characters must not sink-then-snap from gravity each tick.
            let stay_grounded = player.grounded;
            if stay_grounded {
                player.velocity[1] = 0.0;
            } else {
                apply_gravity(player, &config, dt_seconds);
            }
            pre_integrate_velocity = player.velocity;

            integrate_axis(&mut transform.position[0], player.velocity[0], dt_seconds);
            if let Some((id, dx)) = resolve_horizontal(
                transform,
                player,
                scratch.iter(),
                previous_bottom,
                previous_left,
                previous_right,
            ) {
                collision_candidate = Some(id);
                correction[0] = dx;
                correction_axis = CorrectionAxis::Horizontal;
                response_kind = ResponseKind::Normal;
            }

            if stay_grounded {
                match glue_to_support(transform, player, scratch.iter(), previous_bottom) {
                    Some((id, dy)) => {
                        collision_candidate = Some(id);
                        if dy.abs() > correction[1].abs() {
                            correction[1] = dy;
                        }
                        if dy.abs() > 1e-5 {
                            correction_axis = CorrectionAxis::Vertical;
                            if response_kind != ResponseKind::Recovery {
                                response_kind = ResponseKind::Normal;
                            }
                        }
                        apply_grounding(player, prev_grounded_on, Some(id));
                    }
                    None => {
                        apply_grounding(player, prev_grounded_on, None);
                        apply_gravity(player, &config, dt_seconds);
                        pre_integrate_velocity = player.velocity;
                        integrate_axis(&mut transform.position[1], player.velocity[1], dt_seconds);
                        let (contact, dy) = resolve_vertical(
                            transform,
                            player,
                            scratch.iter(),
                            previous_bottom,
                            previous_top,
                        );
                        if dy.abs() > 1e-6 {
                            correction[1] = dy;
                            correction_axis = CorrectionAxis::Vertical;
                            response_kind = ResponseKind::Normal;
                        }
                        if let Some(id) = contact.platform() {
                            collision_candidate = Some(id);
                        }
                        apply_grounding(player, prev_grounded_on, contact.landing());
                    }
                }
            } else {
                integrate_axis(&mut transform.position[1], player.velocity[1], dt_seconds);
                let (contact, dy) = resolve_vertical(
                    transform,
                    player,
                    scratch.iter(),
                    previous_bottom,
                    previous_top,
                );
                if dy.abs() > 1e-6 {
                    correction[1] = dy;
                    correction_axis = CorrectionAxis::Vertical;
                    response_kind = ResponseKind::Normal;
                }
                if let Some(id) = contact.platform() {
                    collision_candidate = Some(id);
                }
                apply_grounding(player, prev_grounded_on, contact.landing());
            }
            expire_ignored(player, transform.position, &scratch);
        }

        let before_bounds = self
            .player_body_of(id)
            .map(|b| b.position)
            .unwrap_or(prev_pos);
        self.apply_world_bounds_for(id);
        if let Some(body) = self.player_body_of(id) {
            let bdx = body.position[0] - before_bounds[0];
            let bdy = body.position[1] - before_bounds[1];
            if bdx.abs() > 1e-6 || bdy.abs() > 1e-6 {
                correction[0] += bdx;
                correction[1] += bdy;
                correction_axis = CorrectionAxis::WorldBound;
                response_kind = ResponseKind::Normal;
            }
        }

        let body = self.player_body_of(id);
        let pos = body.map(|b| b.position).unwrap_or(prev_pos);
        let vel = body.map(|b| b.velocity).unwrap_or(pre_integrate_velocity);
        let grounded_on = body.and_then(|b| b.grounded_on);
        let delta = [pos[0] - prev_pos[0], pos[1] - prev_pos[1]];
        let expected_max = PlayerMotionDebug::expected_max_step(pre_integrate_velocity, dt_seconds);
        let delta_len = (delta[0] * delta[0] + delta[1] * delta[1]).sqrt();
        let discontinuity = delta_len > expected_max + CONTACT_EPSILON;
        let motion = PlayerMotionDebug {
            previous_position: prev_pos,
            position: pos,
            velocity: vel,
            delta,
            expected_max_delta: expected_max,
            discontinuity,
            grounded_on,
            collision_candidate,
            correction,
            correction_axis,
            response_kind,
        };
        // Console spam is client-gated; simulation only records structured data.
        self.set_last_motion_debug(motion);
    }

    fn apply_world_bounds_for(&mut self, player_id: EntityId) {
        let bounds = self.bounds();
        let Some((transform, player)) = self.get_player(player_id) else {
            return;
        };
        let half = player.half_extents;
        let bottom = transform.position[1] - half[1];
        // Development fallback — not a death system.
        if bottom < bounds.min_y - 2.0 {
            self.reset_player_entity(player_id);
            return;
        }

        let Some((transform, player)) = self.player_parts_mut_for(player_id) else {
            return;
        };
        let min_cx = bounds.min_x + half[0];
        let max_cx = bounds.max_x - half[0];
        if transform.position[0] < min_cx {
            transform.position[0] = min_cx;
            if player.velocity[0] < 0.0 {
                player.velocity[0] = 0.0;
            }
        } else if transform.position[0] > max_cx {
            transform.position[0] = max_cx;
            if player.velocity[0] > 0.0 {
                player.velocity[0] = 0.0;
            }
        }

        let min_cy = bounds.min_y + half[1];
        let max_cy = bounds.max_y - half[1];
        if transform.position[1] < min_cy {
            transform.position[1] = min_cy;
            if player.velocity[1] < 0.0 {
                player.velocity[1] = 0.0;
            }
        } else if transform.position[1] > max_cy {
            transform.position[1] = max_cy;
            if player.velocity[1] > 0.0 {
                player.velocity[1] = 0.0;
            }
        }
    }

    fn platform_kind(&self, id: EntityId) -> Option<PlatformKind> {
        self.get_platform(id).map(|(_, platform)| platform.kind)
    }

    /// Clear stale grounded_on / ignored_platform EntityIds.
    pub fn clear_stale_footnote_ids(&mut self) {
        if let Some(id) = self.player_id() {
            self.clear_stale_footnote_ids_for(id);
        }
    }

    fn clear_stale_footnote_ids_for(&mut self, id: EntityId) {
        self.clear_stale_grounding_for(id);
        let ignored = match self.get_player(id) {
            Some((_, player)) => player.ignored_platform,
            None => return,
        };
        let Some(ignored) = ignored else {
            return;
        };
        if self.contains(ignored) && self.kind(ignored) == Some(EntityKind::Platform) {
            return;
        }
        if let Some((_, player)) = self.player_parts_mut_for(id) {
            player.ignored_platform = None;
        }
    }
}

fn apply_drop_through(
    player: &mut PlayerState,
    input: PlayerInput,
    support_kind: Option<PlatformKind>,
    contact: &mut ContactEvent,
) -> bool {
    if !(input.down_held && input.jump_pressed && player.grounded) {
        return false;
    }
    let Some(support) = player.grounded_on else {
        return false;
    };
    if support_kind != Some(PlatformKind::OneWay) {
        return false;
    }
    player.ignored_platform = Some(support);
    player.grounded = false;
    player.grounded_on = None;
    *contact = ContactEvent::LeftGround { platform: support };
    true
}

fn apply_jump(player: &mut PlayerState, input: PlayerInput, config: &FootnoteConfig) {
    if input.jump_pressed && player.grounded {
        player.velocity[1] = config.jump_velocity;
        if let Some(platform) = player.grounded_on {
            player.last_contact = ContactEvent::LeftGround { platform };
        }
        player.grounded = false;
        player.grounded_on = None;
    }
}

/// Keep a grounded player glued to a valid support top without gravity sink.
/// Returns `(platform, y_correction)` or `None` if support was lost (walk-off).
fn glue_to_support(
    transform: &mut Transform,
    player: &mut PlayerState,
    platforms: impl Iterator<Item = PlatformView>,
    previous_bottom: f32,
) -> Option<(EntityId, f32)> {
    let half = player.half_extents;
    let feet = transform.position[1] - half[1];
    let left = transform.position[0] - half[0];
    let right = transform.position[0] + half[0];
    const GLUE_EPS: f32 = 0.12;

    let mut best: Option<(f32, EntityId)> = None;
    for view in platforms {
        let top = view.top_surface();
        if (feet - top).abs() > GLUE_EPS
            && !(previous_bottom >= top - 1e-4 && feet <= top + GLUE_EPS)
        {
            continue;
        }
        let plat = view.aabb();
        if right <= plat.min_x() || left >= plat.max_x() {
            continue;
        }
        let query = crate::footnote::BlockQuery {
            approach: Approach::Down,
            platform_id: view.id,
            previous_bottom: previous_bottom.max(top),
            platform_top: top,
            ignored_platform: player.ignored_platform,
        };
        if !crate::footnote::surface_blocks(view.platform, query) {
            continue;
        }
        best = Some(match best {
            Some((bt, bid)) if bt >= top => (bt, bid),
            _ => (top, view.id),
        });
    }

    let (top, id) = best?;
    let before = transform.position[1];
    transform.position[1] = top + half[1];
    player.velocity[1] = 0.0;
    Some((id, transform.position[1] - before))
}

fn apply_horizontal(
    player: &mut PlayerState,
    input: PlayerInput,
    config: &FootnoteConfig,
    dt: f32,
) {
    let axis = f32::from(input.move_axis);
    if player.grounded {
        let target = axis * config.max_ground_speed;
        if axis == 0.0 {
            move_toward(
                &mut player.velocity[0],
                0.0,
                config.ground_deceleration * dt,
            );
        } else {
            move_toward(
                &mut player.velocity[0],
                target,
                config.ground_acceleration * dt,
            );
            clamp_abs(&mut player.velocity[0], config.max_ground_speed);
        }
    } else if axis != 0.0 {
        let target = axis * config.max_air_speed;
        move_toward(
            &mut player.velocity[0],
            target,
            config.air_acceleration * dt,
        );
        clamp_abs(&mut player.velocity[0], config.max_air_speed);
    }
}

fn apply_gravity(player: &mut PlayerState, config: &FootnoteConfig, dt: f32) {
    player.velocity[1] -= config.gravity * dt;
}

fn integrate_axis(position: &mut f32, velocity: f32, dt: f32) {
    *position += velocity * dt;
}

fn apply_grounding(
    player: &mut PlayerState,
    previous_grounded_on: Option<EntityId>,
    landed: Option<EntityId>,
) {
    match landed {
        Some(id) => {
            if previous_grounded_on != Some(id) && matches!(player.last_contact, ContactEvent::None)
            {
                player.last_contact = ContactEvent::Landed { platform: id };
            }
            player.grounded = true;
            player.grounded_on = Some(id);
            // Secondary: any landing completes the drop-through traversal.
            // Primary clear remains expire_ignored (below support top).
            player.ignored_platform = None;
        }
        None => {
            if let Some(prev) = previous_grounded_on
                && matches!(player.last_contact, ContactEvent::None)
            {
                player.last_contact = ContactEvent::LeftGround { platform: prev };
            }
            player.grounded = false;
            player.grounded_on = None;
        }
    }
}

fn expire_ignored(player: &mut PlayerState, position: [f32; 2], platforms: &PlatformScratch) {
    let Some(ignored) = player.ignored_platform else {
        return;
    };
    let Some(view) = platforms.find(ignored) else {
        player.ignored_platform = None;
        return;
    };
    // Primary lifecycle: ignore ends once the collider is fully below the
    // platform's top/support region — not the platform bottom, not the floor.
    let player_top = position[1] + player.half_extents[1];
    let platform_top = view.top_surface();
    if player_top < platform_top {
        player.ignored_platform = None;
    }
}

fn move_toward(value: &mut f32, target: f32, max_delta: f32) {
    let delta = target - *value;
    if delta.abs() <= max_delta {
        *value = target;
    } else {
        *value += max_delta.copysign(delta);
    }
}

fn clamp_abs(value: &mut f32, max: f32) {
    if *value > max {
        *value = max;
    } else if *value < -max {
        *value = -max;
    }
}
