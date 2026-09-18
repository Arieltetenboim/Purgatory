//! Reusable Hub layout primitives (cards, page chrome, metrics, charts).

use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, Vec2};

use crate::navigation::NavIcon;
use crate::theme;

pub struct PageOutcome {
    pub command: Option<purgatory_dev_runtime::HubCommand>,
    pub navigate: Option<crate::navigation::HubPage>,
}

impl PageOutcome {
    pub fn none() -> Self {
        Self {
            command: None,
            navigate: None,
        }
    }

    pub fn command(cmd: purgatory_dev_runtime::HubCommand) -> Self {
        Self {
            command: Some(cmd),
            navigate: None,
        }
    }

    pub fn merge(&mut self, other: PageOutcome) {
        if other.command.is_some() {
            self.command = other.command;
        }
        if other.navigate.is_some() {
            self.navigate = other.navigate;
        }
    }
}

pub fn page_header(ui: &mut egui::Ui, title: &str, blurb: &str) {
    ui.label(
        RichText::new(title)
            .font(theme::title_font())
            .strong()
            .color(theme::body()),
    );
    if !blurb.is_empty() {
        ui.add_space(2.0);
        ui.label(
            RichText::new(blurb)
                .font(theme::subtitle_font())
                .color(theme::muted()),
        );
    }
    ui.add_space(theme::SECTION_GAP);
}

pub fn chip(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(Color32::from_rgb(28, 34, 42))
        .stroke(theme::card_stroke())
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .font(theme::subtitle_font())
                    .color(theme::muted()),
            );
        });
}

pub fn status_badge(ui: &mut egui::Ui, label: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 3.5, color);
        ui.label(
            RichText::new(label)
                .color(color)
                .strong()
                .font(theme::section_font()),
        );
    });
}

pub fn status_pill(ui: &mut egui::Ui, label: &str, color: Color32) {
    status_badge(ui, label, color);
}

pub fn kv_row(ui: &mut egui::Ui, key: &str, value: impl AsRef<str>) {
    metric_row(ui, key, value.as_ref(), true);
}

pub fn metric_row(ui: &mut egui::Ui, key: &str, value: &str, mono: bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            RichText::new(key)
                .font(theme::subtitle_font())
                .color(theme::muted()),
        );
        let mut text = RichText::new(value).color(theme::body()).strong();
        if mono {
            text = text.font(theme::mono_small());
        } else {
            text = text.font(theme::subtitle_font());
        }
        ui.add(egui::Label::new(text).wrap().selectable(true));
    });
}

pub fn metric_flow(ui: &mut egui::Ui, pairs: &[(&str, &str, bool)]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(16.0, 2.0);
        for (key, value, mono) in pairs {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(
                    RichText::new(*key)
                        .font(theme::subtitle_font())
                        .color(theme::muted()),
                );
                let mut text = RichText::new(*value).color(theme::body()).strong();
                if *mono {
                    text = text.font(theme::mono_small());
                } else {
                    text = text.font(theme::subtitle_font());
                }
                ui.label(text);
            });
        }
    });
}

pub fn hub_card(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> egui::InnerResponse<()> {
    egui::Frame::new()
        .fill(theme::card_fill_elevated())
        .stroke(theme::card_stroke())
        .corner_radius(theme::CARD_RADIUS)
        .inner_margin(theme::CARD_PAD)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            module_title(ui, icon, title);
            add_contents(ui);
        })
}

pub fn metric_tile(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.colored_label(theme::muted(), label);
        ui.label(RichText::new(value).font(theme::metric_font()).strong());
    });
}

pub fn card(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    let _ = hub_card(ui, "", title, add_contents);
}

pub fn module_title(ui: &mut egui::Ui, icon: &str, title: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        if !icon.is_empty() {
            ui.label(RichText::new(icon).color(theme::muted()));
        }
        ui.label(
            RichText::new(title)
                .font(theme::section_font())
                .color(theme::muted())
                .strong(),
        );
    });
    ui.add_space(2.0);
}

pub fn btn_primary(label: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(label.to_owned())
            .strong()
            .color(Color32::WHITE),
    )
    .fill(theme::accent())
    .stroke(Stroke::NONE)
    .min_size(Vec2::new(0.0, theme::BTN_MIN_H))
    .corner_radius(6.0)
}

pub fn btn_ghost(label: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label.to_owned()).color(theme::body()))
        .fill(Color32::from_rgb(28, 34, 42))
        .stroke(theme::card_stroke())
        .min_size(Vec2::new(0.0, theme::BTN_MIN_H))
        .corner_radius(6.0)
}

pub fn btn_destructive(label: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(label.to_owned())
            .strong()
            .color(theme::destructive()),
    )
    .fill(theme::destructive_fill())
    .stroke(Stroke::new(1.0, theme::destructive().gamma_multiply(0.7)))
    .min_size(Vec2::new(0.0, theme::BTN_MIN_H))
    .corner_radius(6.0)
}

pub fn nav_item(ui: &mut egui::Ui, selected: bool, icon: NavIcon, label: &str) -> egui::Response {
    let w = ui.available_width();
    let h = theme::SIDEBAR_ITEM_H;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 6.0, theme::nav_active_fill());
        let bar = egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.top() + 4.0),
            Vec2::new(3.0, h - 8.0),
        );
        painter.rect_filled(bar, 1.5, theme::accent());
    } else if resp.hovered() {
        painter.rect_filled(rect, 6.0, Color32::from_rgb(24, 30, 38));
    }

    let color = if selected {
        theme::body()
    } else {
        theme::muted()
    };
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 17.0, rect.center().y),
        Vec2::splat(14.0),
    );
    paint_nav_icon(painter, icon_rect, icon, color);

    painter.text(
        egui::pos2(rect.left() + 35.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        theme::section_font(),
        if selected {
            theme::body()
        } else {
            Color32::from_rgb(170, 176, 186)
        },
    );
    resp
}

fn paint_nav_icon(painter: &egui::Painter, rect: egui::Rect, icon: NavIcon, color: Color32) {
    let stroke = Stroke::new(1.2, color);
    let c = rect.center();
    let l = rect.left();
    let r = rect.right();
    let t = rect.top();
    let b = rect.bottom();

    match icon {
        NavIcon::Dashboard => {
            for row in 0..2 {
                for col in 0..2 {
                    painter.rect_stroke(
                        egui::Rect::from_min_size(
                            egui::pos2(l + 1.0 + col as f32 * 7.0, t + 1.0 + row as f32 * 7.0),
                            Vec2::splat(5.0),
                        ),
                        0.5,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
        NavIcon::Server => {
            for y in [t + 1.0, c.y - 1.5, b - 4.0] {
                painter.rect_stroke(
                    egui::Rect::from_min_max(
                        egui::pos2(l + 1.0, y),
                        egui::pos2(r - 1.0, y + 3.0),
                    ),
                    0.5,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        NavIcon::Clients => {
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(l + 1.0, t + 1.0),
                    egui::pos2(c.x + 1.0, b - 3.0),
                ),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 1.0, t + 4.0),
                    egui::pos2(r - 1.0, b - 1.0),
                ),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        NavIcon::Validation => {
            painter.rect_stroke(
                rect.shrink(1.0),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [egui::pos2(l + 3.0, c.y), egui::pos2(c.x - 0.5, b - 3.0)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x - 0.5, b - 3.0), egui::pos2(r - 2.0, t + 3.0)],
                stroke,
            );
        }
        NavIcon::Performance => {
            for (x, height) in [(l + 2.0, 4.0), (c.x, 8.0), (r - 2.0, 11.0)] {
                painter.line_segment(
                    [egui::pos2(x, b - 1.0), egui::pos2(x, b - 1.0 - height)],
                    Stroke::new(2.0, color),
                );
            }
        }
        NavIcon::PhaseStats => {
            painter.rect_stroke(rect.shrink(1.0), 1.0, stroke, egui::StrokeKind::Inside);
            painter.line_segment([egui::pos2(c.x, t + 1.0), egui::pos2(c.x, b - 1.0)], stroke);
            painter.line_segment([egui::pos2(l + 1.0, c.y), egui::pos2(r - 1.0, c.y)], stroke);
        }
        NavIcon::World => {
            painter.circle_stroke(c, 6.0, stroke);
            painter.line_segment([egui::pos2(c.x, t + 1.0), egui::pos2(c.x, b - 1.0)], stroke);
            painter.line_segment([egui::pos2(l + 1.0, c.y), egui::pos2(r - 1.0, c.y)], stroke);
        }
        NavIcon::Content => {
            painter.line_segment([egui::pos2(c.x, t + 1.0), egui::pos2(r - 1.0, c.y)], stroke);
            painter.line_segment([egui::pos2(r - 1.0, c.y), egui::pos2(c.x, b - 1.0)], stroke);
            painter.line_segment([egui::pos2(c.x, b - 1.0), egui::pos2(l + 1.0, c.y)], stroke);
            painter.line_segment([egui::pos2(l + 1.0, c.y), egui::pos2(c.x, t + 1.0)], stroke);
        }
        NavIcon::Logs => {
            for y in [t + 3.0, c.y, b - 3.0] {
                painter.line_segment(
                    [egui::pos2(l + 1.0, y), egui::pos2(r - 1.0, y)],
                    stroke,
                );
            }
        }
        NavIcon::Settings => {
            painter.circle_stroke(c, 4.0, stroke);
            painter.circle_filled(c, 1.3, color);
            for (dx, dy) in [(0.0, -6.0), (6.0, 0.0), (0.0, 6.0), (-6.0, 0.0)] {
                painter.line_segment(
                    [
                        egui::pos2(c.x + dx * 0.65, c.y + dy * 0.65),
                        egui::pos2(c.x + dx, c.y + dy),
                    ],
                    stroke,
                );
            }
        }
    }
}

pub fn format_secs(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "—".into();
    }
    let total = secs.round() as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

pub fn format_ms(v: f64) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    format!("{v:.2} ms")
}

pub fn format_mb(v: f64) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    format!("{v:.1} MiB")
}

pub fn log_scroll_view(
    ui: &mut egui::Ui,
    id_salt: &str,
    lines: &[String],
    empty: &str,
    max_height: f32,
) {
    let height = max_height.max(96.0);
    egui::ScrollArea::both()
        .id_salt(id_salt)
        .max_height(height)
        .min_scrolled_height((height * 0.75).min(height))
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            if lines.is_empty() {
                ui.colored_label(theme::muted(), empty);
                return;
            }
            for line in lines {
                ui.add(
                    egui::Label::new(
                        RichText::new(line)
                            .font(theme::mono_small())
                            .color(theme::log_color(line)),
                    )
                    .wrap_mode(egui::TextWrapMode::Extend)
                    .selectable(true),
                );
            }
        });
}

pub fn log_panel(
    ui: &mut egui::Ui,
    id_salt: &str,
    lines: &[String],
    empty: &str,
    max_height: f32,
    hint: &str,
) -> bool {
    let mut clear = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if !hint.is_empty() {
            ui.colored_label(
                theme::muted(),
                RichText::new(hint).font(theme::subtitle_font()),
            );
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.add(btn_ghost("Clear")).clicked() {
                clear = true;
            }
        });
    });
    ui.add_space(2.0);
    log_scroll_view(ui, id_salt, lines, empty, max_height);
    clear
}

pub fn bounded_log_panel(ui: &mut egui::Ui, id_salt: &str, lines: &[String], empty: &str) -> bool {
    log_panel(
        ui,
        id_salt,
        lines,
        empty,
        theme::LOG_VIEW_HEIGHT,
        "scroll ↕↔ for long lines",
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChartSeries {
    Connected,
    TickMean,
}

impl ChartSeries {
    pub fn label(self) -> &'static str {
        match self {
            Self::Connected => "connected bots",
            Self::TickMean => "tick work mean (ms)",
        }
    }
}

pub fn metrics_chart(
    ui: &mut egui::Ui,
    id_salt: &str,
    series: &purgatory_dev_runtime::MetricsSeries,
    selected: &mut ChartSeries,
) {
    ui.horizontal(|ui| {
        ui.label("Series");
        egui::ComboBox::from_id_salt(format!("{id_salt}_series"))
            .selected_text(selected.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    selected,
                    ChartSeries::Connected,
                    ChartSeries::Connected.label(),
                );
                ui.selectable_value(
                    selected,
                    ChartSeries::TickMean,
                    ChartSeries::TickMean.label(),
                );
            });
    });
    if !series.available || series.samples.is_empty() {
        ui.colored_label(
            theme::muted(),
            "No metrics.csv samples yet (harness writes ~1 Hz).",
        );
        return;
    }
    let samples = &series.samples;
    let values: Vec<(f64, f64)> = samples
        .iter()
        .filter_map(|s| {
            let v = match *selected {
                ChartSeries::Connected => Some(s.connected_bots),
                ChartSeries::TickMean => s.tick_work_mean_ms,
            }?;
            v.is_finite().then_some((s.elapsed_secs, v))
        })
        .collect();
    if values.is_empty() {
        ui.colored_label(
            theme::muted(),
            "Selected series has no numeric samples yet.",
        );
        return;
    }
    let min_v = values.iter().map(|(_, v)| *v).fold(f64::INFINITY, f64::min);
    let max_v = values
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::NEG_INFINITY, f64::max);
    let pad_v =
        ((max_v - min_v).abs() * 0.08).max(if matches!(*selected, ChartSeries::Connected) {
            0.5
        } else {
            0.05
        });
    let axis_min = (min_v - pad_v).max(0.0);
    let axis_max = max_v + pad_v;
    let t0 = values.first().map(|(t, _)| *t).unwrap_or(0.0);
    let t1 = values.last().map(|(t, _)| *t).unwrap_or(t0);
    let cur = values.last().map(|(_, v)| *v).unwrap_or(0.0);
    ui.horizontal(|ui| {
        ui.label(format!("now {}", format_series_value(*selected, cur)));
        ui.separator();
        ui.colored_label(
            theme::muted(),
            format!(
                "min {}   max {}   x: {} → {}",
                format_series_value(*selected, min_v),
                format_series_value(*selected, max_v),
                format_secs(t0),
                format_secs(t1)
            ),
        );
    });

    let height = theme::CHART_HEIGHT.min(ui.available_height().max(80.0));
    let (rect, _resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, theme::bg());
    painter.rect_stroke(rect, 4.0, theme::card_stroke(), egui::StrokeKind::Inside);
    let left_axis = 52.0;
    let bottom_axis = 22.0;
    let top_pad = 8.0;
    let right_pad = 10.0;
    let plot = egui::Rect::from_min_max(
        egui::pos2(rect.left() + left_axis, rect.top() + top_pad),
        egui::pos2(rect.right() - right_pad, rect.bottom() - bottom_axis),
    );
    if plot.width() < 8.0 || plot.height() < 8.0 {
        return;
    }
    let span_v = (axis_max - axis_min).abs().max(1e-6);
    let span_t = (t1 - t0).abs().max(1e-6);
    let grid = Color32::from_rgb(45, 50, 58);
    let axis = Color32::from_rgb(120, 128, 140);
    let font = theme::mono_small();
    for i in 0..=4 {
        let frac = i as f32 / 4.0;
        let y = plot.bottom() - frac * plot.height();
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            Stroke::new(1.0, grid),
        );
        let v = axis_min + f64::from(frac) * span_v;
        painter.text(
            egui::pos2(plot.left() - 4.0, y),
            egui::Align2::RIGHT_CENTER,
            format_axis_value(*selected, v),
            font.clone(),
            axis,
        );
    }
    for i in 0..=4 {
        let frac = i as f32 / 4.0;
        let x = plot.left() + frac * plot.width();
        painter.line_segment(
            [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
            Stroke::new(1.0, grid),
        );
        let t = t0 + f64::from(frac) * span_t;
        painter.text(
            egui::pos2(x, plot.bottom() + 4.0),
            egui::Align2::CENTER_TOP,
            format_secs(t),
            font.clone(),
            axis,
        );
    }
    painter.line_segment(
        [
            egui::pos2(plot.left(), plot.top()),
            egui::pos2(plot.left(), plot.bottom()),
        ],
        Stroke::new(1.0, axis),
    );
    painter.line_segment(
        [
            egui::pos2(plot.left(), plot.bottom()),
            egui::pos2(plot.right(), plot.bottom()),
        ],
        Stroke::new(1.0, axis),
    );
    let mut points = Vec::with_capacity(values.len());
    for (t, v) in &values {
        let tx = ((*t - t0) / span_t) as f32;
        let vy = ((*v - axis_min) / span_v) as f32;
        points.push(egui::pos2(
            plot.left() + tx * plot.width(),
            plot.bottom() - vy * plot.height(),
        ));
    }
    if points.len() >= 2 {
        painter.add(egui::Shape::line(points, Stroke::new(1.8, theme::accent())));
    } else if let Some(p) = points.first() {
        painter.circle_filled(*p, 3.0, theme::accent());
    }
}

fn format_axis_value(series: ChartSeries, v: f64) -> String {
    match series {
        ChartSeries::Connected => format!("{v:.0}"),
        ChartSeries::TickMean => format!("{v:.2}"),
    }
}

fn format_series_value(series: ChartSeries, v: f64) -> String {
    match series {
        ChartSeries::Connected => format!("{v:.0}"),
        ChartSeries::TickMean => format_ms(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_secs_mmss() {
        assert_eq!(format_secs(65.2), "01:05");
        assert_eq!(format_secs(0.0), "00:00");
    }
}
