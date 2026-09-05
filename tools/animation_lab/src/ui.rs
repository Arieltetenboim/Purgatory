use eframe::egui::{
    self, Color32, ColorImage, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Ui, Vec2,
};

use purgatory_animation::{Interpolation, LoopPolicy};
use purgatory_animation_lab::debug_vis::{body_panels, root_translation};
use purgatory_animation_lab::document::{
    ChannelKind, KeyRef, bone_label, channel_authorable, times_equal,
};
use purgatory_animation_lab::headwear_proof;
use purgatory_animation_lab::preview::{
    VIEW_X0, VIEW_X1, VIEW_Y0, VIEW_Y1, angle_about, fit_preview_camera, mirror_x, pick_bone,
};
use purgatory_animation_lab::session::LabSession;
use purgatory_animation_lab::snap::{GRID_STEPS, JOINT_SNAP_STEPS_DEG};
use purgatory_animation_lab::timeline::{
    MARKER_ROW_H, ROW_H, RULER_H, TimelineStrip, channel_rows, hit_channel_key, hit_marker,
    keys_in_range, keys_in_rect,
};
use purgatory_skeleton::{
    BONE_COUNT, BoneIndex, HEAD, HUMANOID_V0_BONE_LABELS, ROOT, SkeletonDef, WorldPose, humanoid_v0,
};

pub struct AnimationLabApp {
    session: Result<LabSession, String>,
    drag: Option<ActiveDrag>,
    show_placeholders: bool,
    show_guides: bool,
    show_headwear_proof: bool,
    headwear_texture: Option<TextureHandle>,
}

enum ActiveDrag {
    Rotate(RotationDrag),
    MoveKeys(KeyTimeDrag),
    ScrubPlayhead,
    BoxSelect(BoxSelectDrag),
}

struct RotationDrag {
    bone: BoneIndex,
    joint: [f32; 2],
    start_mouse_angle: f32,
    start_local: f32,
}

struct KeyTimeDrag {
    origins: Vec<(KeyRef, f32)>,
    grab_time: f32,
}

#[derive(Clone, Copy)]
struct BoxSelectDrag {
    start: [f32; 2],
}

struct TimelineHitTarget<'a> {
    strip: TimelineStrip,
    rect: Rect,
    resp: &'a egui::Response,
}

impl AnimationLabApp {
    pub fn new() -> Self {
        Self {
            session: LabSession::open_workspace(),
            drag: None,
            show_placeholders: true,
            show_guides: true,
            show_headwear_proof: true,
            headwear_texture: None,
        }
    }
}

impl eframe::App for AnimationLabApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();
        if let Ok(session) = &mut self.session {
            let dt = ctx.input(|i| i.stable_dt);
            session.advance_playhead(dt);
            session.advance_transition(dt);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match &mut self.session {
            Err(err) => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.heading("PURGATORY Animation Lab");
                    ui.colored_label(Color32::from_rgb(210, 80, 80), err.as_str());
                });
            }
            Ok(session) => {
                let ctx = ui.ctx().clone();
                handle_shortcuts(&ctx, session, &mut self.drag);
                paint(
                    ui,
                    session,
                    &mut self.drag,
                    &mut self.show_placeholders,
                    &mut self.show_guides,
                    &mut self.show_headwear_proof,
                    &mut self.headwear_texture,
                );
            }
        }
    }
}

fn handle_shortcuts(ctx: &egui::Context, session: &mut LabSession, drag: &mut Option<ActiveDrag>) {
    ctx.input(|i| {
        if i.key_pressed(egui::Key::Escape) && session.transaction.is_some() {
            session.cancel_transaction();
            *drag = None;
        }
    });
    let modifiers = ctx.input(|i| i.modifiers);
    if modifiers.ctrl && ctx.input(|i| i.key_pressed(egui::Key::Z)) {
        if modifiers.shift {
            session.redo();
        } else {
            session.undo();
        }
    }
    if modifiers.ctrl && ctx.input(|i| i.key_pressed(egui::Key::Y)) {
        session.redo();
    }
    if modifiers.ctrl
        && ctx.input(|i| i.key_pressed(egui::Key::S))
        && let Err(e) = session.save()
    {
        session.set_error(e);
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Space)) && !typing_in_text(ctx) {
        session.playing = !session.playing;
    }
    if modifiers.ctrl
        && ctx.input(|i| i.key_pressed(egui::Key::C))
        && !modifiers.shift
        && !typing_in_text(ctx)
        && let Err(e) = session.copy_selection()
    {
        session.set_error(e);
    }
    if modifiers.ctrl
        && modifiers.shift
        && ctx.input(|i| i.key_pressed(egui::Key::C))
        && !typing_in_text(ctx)
    {
        session.copy_current_pose();
    }
    if modifiers.ctrl
        && ctx.input(|i| i.key_pressed(egui::Key::V))
        && !modifiers.shift
        && !typing_in_text(ctx)
        && let Err(e) = session.paste_clipboard()
    {
        session.set_error(e);
    }
    if modifiers.ctrl
        && modifiers.shift
        && ctx.input(|i| i.key_pressed(egui::Key::V))
        && !typing_in_text(ctx)
        && let Err(e) = session.paste_pose()
    {
        session.set_error(e);
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
        && !typing_in_text(ctx)
        && (!session.selected_keys.is_empty() || session.selected_key.is_some())
        && let Err(e) = session.delete_selected_key()
    {
        session.set_error(e);
    }
}

fn typing_in_text(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused().is_some())
}

fn paint(
    ui: &mut Ui,
    session: &mut LabSession,
    drag: &mut Option<ActiveDrag>,
    show_placeholders: &mut bool,
    show_guides: &mut bool,
    show_headwear_proof: &mut bool,
    headwear_texture: &mut Option<TextureHandle>,
) {
    egui::Panel::top("lab_top").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Animation Lab");
            ui.separator();
            ui.label(session.asset_name());
            if session.is_dirty() {
                ui.colored_label(Color32::from_rgb(220, 160, 60), "modified");
            } else {
                ui.colored_label(Color32::from_rgb(63, 185, 120), "saved");
            }
            if ui.button("Save").clicked()
                && let Err(e) = session.save()
            {
                session.set_error(e);
            }
            ui.label("Save As");
            ui.add(
                egui::TextEdit::singleline(&mut session.save_as_name)
                    .desired_width(120.0)
                    .hint_text("name"),
            );
            if ui.button("Save As").clicked()
                && let Err(e) = session.save_as()
            {
                session.set_error(e);
            }
            ui.separator();
            if ui
                .add_enabled(session.history.can_undo(), egui::Button::new("Undo"))
                .clicked()
            {
                session.undo();
                *drag = None;
            }
            if ui
                .add_enabled(session.history.can_redo(), egui::Button::new("Redo"))
                .clicked()
            {
                session.redo();
            }
        });
        if let Some(err) = &session.last_error {
            ui.colored_label(Color32::from_rgb(210, 80, 80), err);
        } else if !session.status.is_empty() {
            ui.colored_label(Color32::from_rgb(125, 133, 144), &session.status);
        }
    });

    egui::Panel::left("lab_files")
        .resizable(true)
        .default_size(260.0)
        .show(ui, |ui| {
            files_panel(ui, session);
        });

    egui::Panel::right("lab_inspector")
        .resizable(true)
        .default_size(300.0)
        .show(ui, |ui| {
            inspector_panel(ui, session);
        });

    egui::Panel::bottom("lab_timeline")
        .resizable(true)
        .default_size(280.0)
        .min_size(220.0)
        .show(ui, |ui| {
            timeline_panel(ui, session, drag);
        });

    egui::CentralPanel::default().show(ui, |ui| {
        preview_panel(
            ui,
            session,
            drag,
            show_placeholders,
            show_guides,
            show_headwear_proof,
            headwear_texture,
        );
    });
}

fn files_panel(ui: &mut Ui, session: &mut LabSession) {
    ui.heading("Clips");
    ui.label("content/shared/animations/dev");
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .id_salt("lab_files_clips")
        .max_height(220.0)
        .show(ui, |ui| {
            let files = session.list_files();
            for path in files {
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("clip.anim");
                let selected = session.path.as_ref() == Some(&path);
                if ui.selectable_label(selected, name).clicked()
                    && let Err(e) = session.open_path(&path)
                {
                    session.set_error(e);
                }
            }
        });
    ui.separator();
    ui.heading("New Clip");
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.text_edit_singleline(&mut session.new_clip.name);
    });
    ui.horizontal(|ui| {
        ui.label("Duration");
        ui.text_edit_singleline(&mut session.new_clip.duration);
    });
    ui.checkbox(&mut session.new_clip.loop_once, "Once (unchecked = Loop)");
    if ui.button("Create").clicked()
        && let Err(e) = session.new_clip()
    {
        session.set_error(e);
    }
}

fn inspector_panel(ui: &mut Ui, session: &mut LabSession) {
    ui.heading("Clip");
    ui.horizontal(|ui| {
        ui.label("Duration");
        let resp = ui.text_edit_singleline(&mut session.duration_draft);
        if resp.lost_focus()
            && let Err(e) = session.commit_duration_draft()
        {
            session.set_error(e);
        }
        if ui.button("Apply").clicked()
            && let Err(e) = session.commit_duration_draft()
        {
            session.set_error(e);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Loop");
        let loop_policy = session.document().loop_policy;
        if ui
            .selectable_label(loop_policy == LoopPolicy::Loop, "Loop")
            .clicked()
            && let Err(e) = session.set_loop_policy(LoopPolicy::Loop)
        {
            session.set_error(e);
        }
        if ui
            .selectable_label(loop_policy == LoopPolicy::Once, "Once")
            .clicked()
            && let Err(e) = session.set_loop_policy(LoopPolicy::Once)
        {
            session.set_error(e);
        }
    });
    if ui
        .button("Set Current Pose as Clip Start")
        .on_hover_text(
            "Write the displayed pose as keys at t = 0 so later keys interpolate from it.",
        )
        .clicked()
        && let Err(e) = session.set_current_pose_as_clip_start()
    {
        session.set_error(e);
    }
    ui.separator();
    ui.heading("Bones");
    egui::ScrollArea::vertical()
        .id_salt("lab_inspector_bones")
        .max_height(200.0)
        .show(ui, |ui| {
            for i in 0..BONE_COUNT {
                let bone = BoneIndex::from_u8(i);
                let label = HUMANOID_V0_BONE_LABELS[i as usize];
                let suffix = if bone == ROOT { " (not keyable)" } else { "" };
                let selected = session.selected_bone == Some(bone);
                if ui
                    .selectable_label(selected, format!("{label}{suffix}"))
                    .clicked()
                {
                    session.select_bone(bone);
                }
            }
        });

    if let Some(bone) = session.selected_bone {
        ui.separator();
        ui.heading("Channel");
        ui.horizontal(|ui| {
            for kind in ChannelKind::ALL {
                if !session.document().channel_visible(bone, kind) {
                    continue;
                }
                let selected = session.selected_channel == kind;
                if ui.selectable_label(selected, kind.label()).clicked() {
                    session.set_selected_channel(kind);
                }
            }
        });
        let can_add = session
            .document()
            .can_add_key(bone, session.selected_channel);
        ui.add_enabled_ui(can_add, |ui| {
            if ui.button("Add key at playhead").clicked()
                && let Err(e) = session.add_key_at_playhead()
            {
                session.set_error(e);
            }
        });
        if !can_add && bone == ROOT {
            ui.colored_label(Color32::GRAY, "root is not a clip channel.");
        } else if !can_add && session.selected_channel == ChannelKind::DepthAngle {
            ui.colored_label(Color32::GRAY, "depth is authored on limb bones only.");
        } else if !can_add {
            ui.colored_label(
                Color32::GRAY,
                "tx/ty add is limited to pelvis / torso / head.",
            );
        }

        let key_rows: Vec<_> = session
            .document()
            .track(bone)
            .map(|track| track.channel(session.selected_channel).to_vec())
            .unwrap_or_default();
        ui.label(format!("Keys ({})", key_rows.len()));
        for (i, key) in key_rows.iter().enumerate() {
            let selected = session.selected_key == Some(i);
            if ui
                .selectable_label(
                    selected,
                    format!("{:.2}  {:.4}  {:?}", key.time, key.value, key.interpolation),
                )
                .clicked()
            {
                session.select_key(i);
            }
        }

        let selected_key = session.selected_key.and_then(|index| {
            session
                .document()
                .track(bone)
                .and_then(|track| track.channel(session.selected_channel).get(index))
                .copied()
        });
        if let Some(key) = selected_key {
            ui.horizontal(|ui| {
                ui.label("Time");
                let time_resp = ui.text_edit_singleline(&mut session.key_time_draft);
                ui.label("Value");
                let value_resp = ui.text_edit_singleline(&mut session.key_value_draft);
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let commit_draft = ui.button("Apply key").clicked()
                    || (enter
                        && (time_resp.has_focus()
                            || value_resp.has_focus()
                            || time_resp.lost_focus()
                            || value_resp.lost_focus()));
                if commit_draft && let Err(e) = session.commit_key_drafts() {
                    session.set_error(e);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Interp");
                let mut interp = key.interpolation;
                if ui
                    .selectable_label(interp == Interpolation::Linear, "Linear")
                    .clicked()
                {
                    interp = Interpolation::Linear;
                }
                if ui
                    .selectable_label(interp == Interpolation::Step, "Step")
                    .clicked()
                {
                    interp = Interpolation::Step;
                }
                if interp != key.interpolation {
                    let mut next = key;
                    next.interpolation = interp;
                    let kind = session.selected_channel;
                    if let Err(e) = session.mutate(
                        format!("Set interp — {}.{}", bone_label(bone), kind.token()),
                        |doc| doc.upsert_key(bone, kind, next),
                    ) {
                        session.set_error(e);
                    }
                }
            });
            if ui.button("Delete key").clicked()
                && let Err(e) = session.delete_selected_key()
            {
                session.set_error(e);
            }
        }
    }

    ui.separator();
    ui.heading("Markers");
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.text_edit_singleline(&mut session.marker_name_draft);
    });
    ui.horizontal(|ui| {
        ui.label("Type");
        ui.text_edit_singleline(&mut session.marker_type_draft);
    });
    if ui.button("Add marker at playhead").clicked()
        && let Err(e) = session.add_marker_at_playhead()
    {
        session.set_error(e);
    }
    let markers: Vec<_> = session.document().markers.clone();
    for (i, marker) in markers.iter().enumerate() {
        let selected = session.selected_marker == Some(i);
        if ui
            .selectable_label(
                selected,
                format!(
                    "{:.2}  {}  {}",
                    marker.time, marker.name, marker.marker_type
                ),
            )
            .clicked()
        {
            session.selected_marker = Some(i);
        }
    }
    if ui.button("Delete marker").clicked()
        && let Err(e) = session.delete_selected_marker()
    {
        session.set_error(e);
    }

    ui.separator();
    ui.heading("History");
    let cursor = session.history.cursor();
    let labels: Vec<String> = session
        .history
        .entries()
        .iter()
        .map(|e| e.label.clone())
        .collect();
    egui::ScrollArea::vertical()
        .id_salt("lab_inspector_history")
        .show(ui, |ui| {
            for (i, label) in labels.iter().enumerate() {
                let mark = if i == cursor { "▶ " } else { "   " };
                if ui
                    .selectable_label(i == cursor, format!("{mark}{label}"))
                    .clicked()
                {
                    session.jump_history(i);
                }
            }
        });
}

fn timeline_panel(ui: &mut Ui, session: &mut LabSession, drag: &mut Option<ActiveDrag>) {
    ui.heading("Timeline / Dope Sheet");
    ui.horizontal(|ui| {
        if ui
            .button(if session.playing { "Pause" } else { "Play" })
            .clicked()
        {
            session.playing = !session.playing;
        }
        ui.checkbox(&mut session.loop_preview, "Loop preview");
        ui.label(format!("t = {:.2}", session.playhead));
        let duration = session.document().duration.max(0.01);
        let mut t = session.playhead;
        if ui
            .add(egui::Slider::new(&mut t, 0.0..=duration).show_value(false))
            .changed()
        {
            session.playing = false;
            session.set_playhead(t);
        }
        ui.separator();
        let can_add = session.selected_bone.is_some_and(|bone| {
            session
                .document()
                .can_add_key(bone, session.selected_channel)
        });
        ui.add_enabled_ui(can_add, |ui| {
            if ui.button("Add key at playhead").clicked()
                && let Err(e) = session.add_key_at_playhead()
            {
                session.set_error(e);
            }
        });
        ui.add_enabled_ui(
            !session.selected_keys.is_empty() || session.selected_key.is_some(),
            |ui| {
                if ui.button("Delete key").clicked()
                    && let Err(e) = session.delete_selected_key()
                {
                    session.set_error(e);
                }
            },
        );
        if ui.button("Copy").clicked()
            && let Err(e) = session.copy_selection()
        {
            session.set_error(e);
        }
        if ui.button("Paste").clicked()
            && let Err(e) = session.paste_clipboard()
        {
            session.set_error(e);
        }
        if ui.button("Copy Pose").clicked() {
            session.copy_current_pose();
        }
        if ui.button("Paste Pose").clicked()
            && let Err(e) = session.paste_pose()
        {
            session.set_error(e);
        }
        if ui
            .button("Set Current Pose as Clip Start")
            .on_hover_text("Capture the displayed pose into keys at t = 0.")
            .clicked()
            && let Err(e) = session.set_current_pose_as_clip_start()
        {
            session.set_error(e);
        }
        if ui.button("Select Keys at Playhead").clicked() {
            session.select_keys_at_playhead();
        }
        if ui.button("Delete Keys at Playhead").clicked()
            && let Err(e) = session.delete_keys_at_playhead()
        {
            session.set_error(e);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Select:")
            .on_hover_text("Click a key. Ctrl+click toggles. Shift+click ranges from the anchor. Ctrl+drag boxes. Delete removes the selection.");
        ui.weak("Ctrl+click add  ·  Shift range  ·  Ctrl+drag box");
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut session.snap.enabled, "Timeline Snap");
        ui.checkbox(&mut session.snap.grid, "Grid");
        ui.checkbox(&mut session.snap.keys, "Keys");
        ui.checkbox(&mut session.snap.markers, "Markers");
        for step in GRID_STEPS {
            let selected = (session.snap.grid_step - step).abs() < 1e-6;
            if ui
                .selectable_label(selected, format!("{step:.2}s"))
                .clicked()
            {
                session.snap.grid_step = step;
            }
        }
    });

    let duration = session.document().duration.max(0.01);
    let doc = session.document().clone();
    let rows = channel_rows(&doc, session.selected_bone, session.selected_channel);
    let sheet_w = ui.available_width().max(8.0);
    let header_h = RULER_H + MARKER_ROW_H;
    let (header_rect, header_resp) =
        ui.allocate_exact_size(Vec2::new(sheet_w, header_h), Sense::click_and_drag());
    let strip = TimelineStrip::from_sheet(header_rect.left(), header_rect.width(), duration);
    paint_timeline_header(
        ui,
        header_rect,
        strip,
        duration,
        session.playhead,
        &doc,
        session.selected_marker,
    );

    let remaining = ui.available_height().max(80.0);
    let rows_h = (rows.len() as f32 * ROW_H).max(remaining);
    let box_now = match drag {
        Some(ActiveDrag::BoxSelect(state)) => ui
            .ctx()
            .pointer_latest_pos()
            .map(|p| (state.start, [p.x, p.y])),
        _ => None,
    };
    let body_inner = egui::ScrollArea::vertical()
        .id_salt("lab_dopesheet_rows")
        .auto_shrink([false, false])
        .max_height(remaining)
        .min_scrolled_height(remaining)
        .show(ui, |ui| {
            ui.set_min_width(sheet_w);
            ui.set_min_height(rows_h);
            let (body_rect, body_resp) =
                ui.allocate_exact_size(Vec2::new(sheet_w, rows_h), Sense::click_and_drag());
            let body_strip =
                TimelineStrip::from_sheet(body_rect.left(), body_rect.width(), duration);
            paint_timeline_rows(ui, body_rect, body_strip, session, &doc, &rows, box_now);
            body_resp
        })
        .inner;

    handle_timeline_input(
        session,
        drag,
        &doc,
        &rows,
        TimelineHitTarget {
            strip,
            rect: header_rect,
            resp: &header_resp,
        },
        TimelineHitTarget {
            strip: TimelineStrip::from_sheet(
                body_inner.rect.left(),
                body_inner.rect.width(),
                duration,
            ),
            rect: body_inner.rect,
            resp: &body_inner,
        },
        ui.ctx().input(|i| i.modifiers),
    );
}

fn paint_timeline_header(
    ui: &Ui,
    rect: Rect,
    strip: TimelineStrip,
    duration: f32,
    playhead: f32,
    doc: &purgatory_animation_lab::document::AnimDocument,
    selected_marker: Option<usize>,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(17, 22, 28));
    let ruler_y = rect.top();
    let marker_top = ruler_y + RULER_H;
    let marker_mid = marker_top + MARKER_ROW_H * 0.5;
    painter.rect_filled(
        Rect::from_min_max(
            Pos2::new(rect.left(), marker_top),
            Pos2::new(rect.right(), rect.bottom()),
        ),
        0.0,
        Color32::from_rgb(24, 20, 32),
    );
    painter.text(
        Pos2::new(rect.left() + 6.0, marker_mid),
        egui::Align2::LEFT_CENTER,
        "markers",
        egui::FontId::proportional(11.0),
        Color32::from_rgb(180, 140, 210),
    );
    painter.text(
        Pos2::new(strip.x0, ruler_y + 2.0),
        egui::Align2::LEFT_TOP,
        "0.00",
        egui::FontId::proportional(10.0),
        Color32::from_rgb(125, 133, 144),
    );
    painter.text(
        Pos2::new(strip.x1, ruler_y + 2.0),
        egui::Align2::RIGHT_TOP,
        format!("{duration:.2}"),
        egui::FontId::proportional(10.0),
        Color32::from_rgb(125, 133, 144),
    );
    let tick_step = if duration <= 0.5 { 0.05 } else { 0.10 };
    let mut tick = tick_step;
    while tick < duration - 1e-4 {
        let x = strip.x_of(tick);
        painter.line_segment(
            [
                Pos2::new(x, ruler_y + 12.0),
                Pos2::new(x, ruler_y + RULER_H),
            ],
            Stroke::new(1.0, Color32::from_rgb(50, 58, 68)),
        );
        tick += tick_step;
    }
    for (mi, marker) in doc.markers.iter().enumerate() {
        let x = strip.x_of(marker.time);
        let color = if selected_marker == Some(mi) {
            Color32::from_rgb(230, 180, 255)
        } else {
            Color32::from_rgb(180, 90, 220)
        };
        painter.circle_filled(Pos2::new(x, marker_mid), 5.0, color);
    }
    let play_x = strip.x_of(playhead);
    painter.line_segment(
        [
            Pos2::new(play_x, rect.top()),
            Pos2::new(play_x, rect.bottom()),
        ],
        Stroke::new(1.5, Color32::from_rgb(47, 129, 247)),
    );
}

fn paint_timeline_rows(
    ui: &Ui,
    rect: Rect,
    strip: TimelineStrip,
    session: &LabSession,
    doc: &purgatory_animation_lab::document::AnimDocument,
    rows: &[purgatory_animation_lab::timeline::ChannelRow],
    box_select: Option<([f32; 2], [f32; 2])>,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(17, 22, 28));
    if rows.is_empty() {
        painter.text(
            Pos2::new(rect.center().x, rect.top() + 28.0),
            egui::Align2::CENTER_CENTER,
            "No authored keys — Add key at playhead after selecting a bone",
            egui::FontId::proportional(13.0),
            Color32::from_rgb(125, 133, 144),
        );
    }
    for (row_i, row) in rows.iter().copied().enumerate() {
        let y0 = rect.top() + row_i as f32 * ROW_H;
        let y1 = y0 + ROW_H;
        let mid = y0 + ROW_H * 0.5;
        let selected_row =
            session.selected_bone == Some(row.bone) && session.selected_channel == row.kind;
        if selected_row {
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(rect.left(), y0), Pos2::new(rect.right(), y1)),
                0.0,
                Color32::from_rgb(28, 36, 48),
            );
        } else if row_i % 2 == 1 {
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(rect.left(), y0), Pos2::new(rect.right(), y1)),
                0.0,
                Color32::from_rgb(19, 24, 31),
            );
        }
        painter.text(
            Pos2::new(rect.left() + 6.0, mid),
            egui::Align2::LEFT_CENTER,
            format!("{}.{}", bone_label(row.bone), row.kind.token()),
            egui::FontId::proportional(11.0),
            Color32::from_rgb(180, 186, 194),
        );
        if let Some(track) = doc.track(row.bone) {
            for (ki, key) in track.channel(row.kind).iter().enumerate() {
                let x = strip.x_of(key.time);
                let key_ref = KeyRef::from_key(row.bone, row.kind, key.time);
                let selected = session.selected_keys.contains(&key_ref)
                    || (selected_row && session.selected_key == Some(ki));
                let color = if selected {
                    Color32::from_rgb(255, 200, 80)
                } else {
                    Color32::from_rgb(90, 180, 220)
                };
                painter.circle_filled(Pos2::new(x, mid), 5.0, color);
            }
        }
    }
    let play_x = strip.x_of(session.playhead);
    painter.line_segment(
        [
            Pos2::new(play_x, rect.top()),
            Pos2::new(play_x, rect.bottom()),
        ],
        Stroke::new(1.5, Color32::from_rgb(47, 129, 247)),
    );
    if let Some((a, b)) = box_select {
        let r = Rect::from_two_pos(Pos2::new(a[0], a[1]), Pos2::new(b[0], b[1]));
        painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(47, 129, 247, 40));
        painter.rect_stroke(
            r,
            0.0,
            Stroke::new(1.0, Color32::from_rgb(47, 129, 247)),
            egui::StrokeKind::Inside,
        );
    }
}

fn handle_timeline_input(
    session: &mut LabSession,
    drag: &mut Option<ActiveDrag>,
    doc: &purgatory_animation_lab::document::AnimDocument,
    rows: &[purgatory_animation_lab::timeline::ChannelRow],
    header: TimelineHitTarget<'_>,
    body: TimelineHitTarget<'_>,
    modifiers: egui::Modifiers,
) {
    let header_strip = header.strip;
    let header_rect = header.rect;
    let header_resp = header.resp;
    let body_rect = body.rect;
    let body_resp = body.resp;
    let body_strip = body.strip;
    let marker_mid = header_rect.top() + RULER_H + MARKER_ROW_H * 0.5;
    let header_ptr = header_resp.interact_pointer_pos().map(|p| [p.x, p.y]);
    let body_ptr = body_resp.interact_pointer_pos().map(|p| [p.x, p.y]);
    let hit_mark = header_ptr.and_then(|p| hit_marker(header_strip, marker_mid, p, doc));
    let hit_key = body_ptr.and_then(|p| hit_channel_key(body_strip, rows, body_rect.top(), p, doc));

    if header_resp.clicked()
        && let Some(pos) = header_ptr
    {
        if let Some(mi) = hit_mark {
            session.selected_marker = Some(mi);
        } else if header_strip.contains_x(pos[0]) {
            session.playing = false;
            session.set_playhead(header_strip.t_of(pos[0]));
        }
    }
    if body_resp.clicked()
        && let Some(pos) = body_ptr
    {
        if let Some((bone, kind, ki)) = hit_key {
            let time = doc
                .track(bone)
                .and_then(|tr| tr.channel(kind).get(ki))
                .map(|k| k.time)
                .unwrap_or(0.0);
            let key_ref = KeyRef::from_key(bone, kind, time);
            if modifiers.ctrl {
                session.toggle_selection(key_ref);
            } else if modifiers.shift {
                if let Some(anchor) = session.selection_anchor {
                    let ranged = keys_in_range(rows, doc, anchor, key_ref);
                    session.replace_selection(ranged, Some(key_ref));
                } else {
                    session.select_bone(bone);
                    session.selected_channel = kind;
                    session.select_key(ki);
                }
            } else {
                session.select_bone(bone);
                session.selected_channel = kind;
                session.select_key(ki);
            }
        } else if body_strip.contains_x(pos[0]) {
            session.playing = false;
            session.set_playhead(body_strip.t_of(pos[0]));
            if !modifiers.ctrl {
                session.selected_keys.clear();
            }
        }
    }

    if drag.is_none() && body_resp.drag_started() {
        if let Some((bone, kind, ki, old_time)) = hit_key.and_then(|(bone, kind, ki)| {
            doc.track(bone)
                .and_then(|tr| tr.channel(kind).get(ki))
                .map(|k| (bone, kind, ki, k.time))
        }) {
            let key_ref = KeyRef::from_key(bone, kind, old_time);
            if !session.selected_keys.contains(&key_ref) {
                session.select_bone(bone);
                session.selected_channel = kind;
                session.select_key(ki);
            }
            let origins: Vec<(KeyRef, f32)> = session
                .selection_refs()
                .into_iter()
                .filter_map(|r| {
                    doc.track(r.bone).and_then(|tr| {
                        tr.channel(r.kind)
                            .iter()
                            .find(|k| times_equal(k.time, r.time()))
                            .map(|k| (r, k.time))
                    })
                })
                .collect();
            let n = origins.len();
            session.begin_transaction(format!("Move {n} key(s)"));
            *drag = Some(ActiveDrag::MoveKeys(KeyTimeDrag {
                origins,
                grab_time: old_time,
            }));
        } else if modifiers.ctrl {
            if let Some(pos) = body_ptr {
                *drag = Some(ActiveDrag::BoxSelect(BoxSelectDrag { start: pos }));
            }
        } else if body_ptr.is_some_and(|p| body_strip.contains_x(p[0])) {
            session.playing = false;
            *drag = Some(ActiveDrag::ScrubPlayhead);
            if let Some(pos) = body_ptr {
                session.set_playhead(body_strip.t_of(pos[0]));
            }
        }
    }
    if drag.is_none()
        && header_resp.drag_started()
        && header_ptr.is_some_and(|p| header_strip.contains_x(p[0]))
        && hit_mark.is_none()
    {
        session.playing = false;
        *drag = Some(ActiveDrag::ScrubPlayhead);
        if let Some(pos) = header_ptr {
            session.set_playhead(header_strip.t_of(pos[0]));
        }
    }

    let drag_pos = body_resp
        .interact_pointer_pos()
        .or_else(|| header_resp.interact_pointer_pos());
    if let Some(ActiveDrag::MoveKeys(state)) = drag.as_mut()
        && (body_resp.dragged() || header_resp.dragged())
        && let Some(pos) = drag_pos
        && body_strip.contains_x(pos.x)
    {
        let t = body_strip.t_of(pos.x);
        let _ = session.preview_batch_move(&state.origins, state.grab_time, t);
    }
    if let Some(ActiveDrag::ScrubPlayhead) = drag
        && (body_resp.dragged() || header_resp.dragged())
        && let Some(pos) = drag_pos
    {
        let strip = if header_rect.contains(pos) {
            header_strip
        } else {
            body_strip
        };
        if strip.contains_x(pos.x) {
            session.set_playhead(strip.t_of(pos.x));
        }
    }
    if body_resp.drag_stopped() || header_resp.drag_stopped() {
        match drag {
            Some(ActiveDrag::MoveKeys(_)) => {
                if let Err(e) = session.commit_transaction() {
                    session.set_error(e);
                    session.cancel_transaction();
                }
                *drag = None;
            }
            Some(ActiveDrag::BoxSelect(state)) => {
                if let Some(pos) = body_ptr.or(Some(state.start)) {
                    let hits =
                        keys_in_rect(body_strip, rows, body_rect.top(), state.start, pos, doc);
                    let refs: Vec<KeyRef> = hits
                        .into_iter()
                        .filter_map(|(bone, kind, ki)| {
                            doc.track(bone)
                                .and_then(|tr| tr.channel(kind).get(ki))
                                .map(|k| KeyRef::from_key(bone, kind, k.time))
                        })
                        .collect();
                    let primary = refs.last().copied();
                    session.replace_selection(refs, primary);
                }
                *drag = None;
            }
            Some(ActiveDrag::ScrubPlayhead) => {
                *drag = None;
            }
            _ => {}
        }
    }
}

fn preview_panel(
    ui: &mut Ui,
    session: &mut LabSession,
    drag: &mut Option<ActiveDrag>,
    show_placeholders: &mut bool,
    show_guides: &mut bool,
    show_headwear_proof: &mut bool,
    headwear_texture: &mut Option<TextureHandle>,
) {
    ui.horizontal(|ui| {
        ui.heading("Preview");
        ui.label("sample + evaluate  ·  drag a joint to rotate");
        ui.separator();
        ui.checkbox(&mut session.mirror_left, "Facing Left");
        ui.checkbox(show_placeholders, "Body");
        ui.checkbox(show_headwear_proof, "Headwear proof")
            .on_hover_text(headwear_proof::VISUAL_KEY);
        ui.checkbox(show_guides, "Guides");
        ui.separator();
        ui.checkbox(&mut session.joint_snap.enabled, "Joint Snap")
            .on_hover_text(
                "Snap viewport joint rotation to an angle increment. Independent of Timeline Snap.",
            );
        for step in JOINT_SNAP_STEPS_DEG {
            let selected = (session.joint_snap.step_deg - step).abs() < 1e-6;
            if ui
                .selectable_label(selected, format!("{step:.0}°"))
                .clicked()
            {
                session.joint_snap.step_deg = step;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut session.transition_enabled, "Transition");
        let files = session.list_files();
        let current = session
            .transition_b
            .as_ref()
            .and_then(|(p, _)| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("(select B)");
        egui::ComboBox::from_id_salt("lab_transition_b")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for path in files {
                    let name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("clip.anim")
                        .to_string();
                    let selected = session
                        .transition_b
                        .as_ref()
                        .is_some_and(|(p, _)| p == &path);
                    if ui.selectable_label(selected, name).clicked() {
                        match session.set_transition_clip(path) {
                            Ok(()) => session.transition_enabled = true,
                            Err(e) => session.set_error(e),
                        }
                    }
                }
            });
        ui.label("dur");
        ui.add(egui::Slider::new(&mut session.transition_duration, 0.05..=2.0).show_value(true));
        ui.label("α");
        if ui
            .add(egui::Slider::new(&mut session.transition_alpha, 0.0..=1.0).show_value(true))
            .changed()
        {
            session.transition_playing = false;
        }
        if ui
            .button(if session.transition_playing {
                "Pause A→B"
            } else {
                "Play A→B"
            })
            .clicked()
        {
            if session.transition_b.is_none() {
                session.set_error("select a valid clip B for transition preview");
                session.transition_playing = false;
            } else if !session.transition_playing {
                session.playing = false;
                session.transition_alpha = 0.0;
                session.transition_playing = true;
                session.transition_enabled = true;
            } else {
                session.transition_playing = false;
            }
        }
    });
    let pose = match session.preview_pose() {
        Ok(p) => p,
        Err(e) => {
            ui.colored_label(Color32::from_rgb(210, 80, 80), e);
            return;
        }
    };
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(11, 14, 20));

    let def = humanoid_v0();
    if *show_headwear_proof {
        ensure_headwear_texture(ui.ctx(), headwear_texture);
    }
    let cam = fit_preview_camera(rect.left(), rect.top(), rect.width(), rect.height());
    let root_x = root_translation(&pose.world)[0];
    let mirror = session.mirror_left;
    let to_screen = |xy: [f32; 2]| {
        let world = if mirror { mirror_x(xy, root_x) } else { xy };
        let p = cam.to_canvas(world);
        Pos2::new(p[0], p[1])
    };
    let to_skel = |p: Pos2| {
        let w = cam.to_world([p.x, p.y]);
        if mirror { mirror_x(w, root_x) } else { w }
    };

    if *show_guides {
        draw_guides(&painter, rect, to_screen);
    }
    if *show_placeholders {
        draw_placeholders(
            &painter,
            def,
            &pose.world,
            to_screen,
            session.selected_bone,
            *show_headwear_proof,
            headwear_texture.as_ref(),
        );
    } else if *show_headwear_proof {
        draw_headwear_sprite(&painter, &pose.world, to_screen, headwear_texture.as_ref());
    }
    draw_skeleton(
        &painter,
        def,
        &pose.world,
        to_screen,
        session.selected_bone,
        *show_placeholders,
    );

    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let skel = to_skel(pos);
        if let Some(bone) = pick_bone(&pose.world, skel, 0.08) {
            session.select_bone(bone);
        }
    }

    if drag.is_none()
        && response.drag_started()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let skel = to_skel(pos);
        if let Some(bone) = pick_bone(&pose.world, skel, 0.10) {
            session.select_bone(bone);
            if channel_authorable(bone, ChannelKind::Rotation)
                && let Some(xf) = pose.world.get(bone)
                && let Some(local) = pose.local.get(bone)
            {
                session.begin_transaction(format!("Rotate bone — {}", bone_label(bone)));
                *drag = Some(ActiveDrag::Rotate(RotationDrag {
                    bone,
                    joint: xf.translation,
                    start_mouse_angle: angle_about(xf.translation, skel),
                    start_local: local.rotation,
                }));
            }
        }
    }

    if response.dragged()
        && let Some(ActiveDrag::Rotate(drag_state)) = drag.as_ref()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let skel = to_skel(pos);
        let angle = angle_about(drag_state.joint, skel);
        let delta = angle - drag_state.start_mouse_angle;
        let local = drag_state.start_local + delta;
        if let Err(e) = session.apply_direct_rotation_working(drag_state.bone, local) {
            session.set_error(e);
        }
    }

    if response.drag_stopped() && matches!(drag, Some(ActiveDrag::Rotate(_))) {
        if let Err(e) = session.commit_transaction() {
            session.set_error(e);
        }
        *drag = None;
    }
}

fn draw_guides(painter: &egui::Painter, rect: Rect, to_screen: impl Fn([f32; 2]) -> Pos2) {
    let grid = Color32::from_rgba_unmultiplied(40, 48, 58, 90);
    let axis = Color32::from_rgba_unmultiplied(70, 82, 96, 140);
    let ground = Color32::from_rgba_unmultiplied(90, 110, 130, 180);
    let mut x = VIEW_X0;
    while x <= VIEW_X1 + 1e-4 {
        let a = to_screen([x, VIEW_Y0]);
        let b = to_screen([x, VIEW_Y1]);
        painter.line_segment(
            [
                Pos2::new(a.x, a.y.clamp(rect.top(), rect.bottom())),
                Pos2::new(b.x, b.y.clamp(rect.top(), rect.bottom())),
            ],
            Stroke::new(1.0, grid),
        );
        x += 0.20;
    }
    let mut y = VIEW_Y0;
    while y <= VIEW_Y1 + 1e-4 {
        let a = to_screen([VIEW_X0, y]);
        let b = to_screen([VIEW_X1, y]);
        painter.line_segment(
            [
                Pos2::new(a.x.clamp(rect.left(), rect.right()), a.y),
                Pos2::new(b.x.clamp(rect.left(), rect.right()), b.y),
            ],
            Stroke::new(1.0, grid),
        );
        y += 0.20;
    }
    let x0a = to_screen([0.0, VIEW_Y0]);
    let x0b = to_screen([0.0, VIEW_Y1]);
    painter.line_segment([x0a, x0b], Stroke::new(1.0, axis));
    let g0 = to_screen([VIEW_X0, 0.0]);
    let g1 = to_screen([VIEW_X1, 0.0]);
    painter.line_segment([g0, g1], Stroke::new(2.0, ground));
}

fn draw_placeholders(
    painter: &egui::Painter,
    def: &SkeletonDef,
    world: &WorldPose,
    to_screen: impl Fn([f32; 2]) -> Pos2,
    selected: Option<BoneIndex>,
    show_headwear: bool,
    headwear: Option<&TextureHandle>,
) {
    for panel in body_panels(def, world) {
        let pts: Vec<Pos2> = panel.corners.iter().copied().map(&to_screen).collect();
        let mut color = rgba(panel.color);
        if selected == Some(panel.bone) {
            color = Color32::from_rgba_unmultiplied(
                color.r().saturating_add(30),
                color.g().saturating_add(30),
                color.b().saturating_add(20),
                color.a(),
            );
        }
        painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        if show_headwear && panel.bone == HEAD {
            draw_headwear_sprite(painter, world, &to_screen, headwear);
        }
    }
}

fn ensure_headwear_texture(ctx: &egui::Context, slot: &mut Option<TextureHandle>) {
    if slot.is_some() {
        return;
    }
    let path = headwear_proof::proof_png_path();
    let Ok(img) = headwear_proof::load_rgba(&path) else {
        return;
    };
    let size = [img.width() as usize, img.height() as usize];
    let color = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    *slot = Some(ctx.load_texture(headwear_proof::VISUAL_KEY, color, TextureOptions::NEAREST));
}

fn draw_headwear_sprite(
    painter: &egui::Painter,
    world: &WorldPose,
    to_screen: impl Fn([f32; 2]) -> Pos2,
    texture: Option<&TextureHandle>,
) {
    let Some(texture) = texture else {
        return;
    };
    let Some(corners) = headwear_proof::sprite_world_corners(world) else {
        return;
    };
    let mut mesh = egui::Mesh::with_texture(texture.id());
    let uvs = [
        Pos2::new(0.0, 0.0),
        Pos2::new(1.0, 0.0),
        Pos2::new(1.0, 1.0),
        Pos2::new(0.0, 1.0),
    ];
    for (i, world_xy) in corners.iter().enumerate() {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: to_screen(*world_xy),
            uv: uvs[i],
            color: Color32::WHITE,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
}

fn rgba(c: [f32; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (c[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (c[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (c[2] * 255.0).round().clamp(0.0, 255.0) as u8,
        (c[3] * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

fn draw_skeleton(
    painter: &egui::Painter,
    def: &SkeletonDef,
    world: &WorldPose,
    to_screen: impl Fn([f32; 2]) -> Pos2,
    selected: Option<BoneIndex>,
    placeholders_on: bool,
) {
    let line_w = if placeholders_on { 1.5 } else { 3.0 };
    let joint_r = if placeholders_on { 3.5 } else { 5.5 };
    for i in 0..def.bone_count() {
        let bone = BoneIndex::from_u8(i as u8);
        let Some(xf) = world.get(bone) else {
            continue;
        };
        let p = to_screen(xf.translation);
        if let Some(parent) = def.parent(bone)
            && let Some(pxf) = world.get(parent)
        {
            let back = (7..=9).contains(&i) || i >= 13;
            let color = if back {
                Color32::from_rgb(70, 80, 90)
            } else {
                Color32::from_rgb(200, 180, 70)
            };
            painter.line_segment([to_screen(pxf.translation), p], Stroke::new(line_w, color));
        }
        let color = if selected == Some(bone) {
            Color32::from_rgb(80, 200, 255)
        } else if bone == ROOT {
            Color32::WHITE
        } else {
            Color32::from_rgb(230, 210, 90)
        };
        painter.circle_filled(p, joint_r, color);
    }
    let root = to_screen(root_translation(world));
    let tick = 7.0;
    painter.line_segment(
        [
            Pos2::new(root.x - tick, root.y),
            Pos2::new(root.x + tick, root.y),
        ],
        Stroke::new(1.5, Color32::from_rgba_unmultiplied(230, 230, 240, 180)),
    );
    painter.line_segment(
        [
            Pos2::new(root.x, root.y - tick),
            Pos2::new(root.x, root.y + tick),
        ],
        Stroke::new(1.5, Color32::from_rgba_unmultiplied(230, 230, 240, 180)),
    );
}
